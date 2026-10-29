use alloy_primitives::U256;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};
use zeroize::Zeroizing;
use zoru_miner::{
    abi, pow,
    session::{Options, Run, Session},
    wallet,
};

#[test]
#[ignore = "requires an isolated Anvil deployment and disposable encrypted wallet"]
fn recovered_proofs_trigger_automatic_claim_and_late_work_is_not_signed() {
    let cfg: Value =
        serde_json::from_slice(&fs::read(std::env::var("ZORU_TEST_OPTIONS").unwrap()).unwrap())
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let claimed = Arc::new(AtomicBool::new(false));
    let output = claimed.clone();
    let options = Options {
        manifest: PathBuf::from(cfg["manifest"].as_str().unwrap()),
        read: cfg["read"].as_str().unwrap().into(),
        submit: cfg["submit"].as_str().unwrap().into(),
        native: "http://127.0.0.1:1".into(),
        wallet: Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),
        expected_wallet: None,
        observer: None,
        sync_on_connect: true,
        fee_budget: None,
        state_dir: directory.path().into(),
    };
    let session = Session::connect(
        options.clone(),
        Zeroizing::new(cfg["password"].as_str().unwrap().into()),
        Arc::new(move |event| {
            if event["event"] == "auto-claims"
                && !event["data"]["claims"].as_array().unwrap().is_empty()
            {
                output.store(true, Ordering::SeqCst);
            }
        }),
    )
    .unwrap();
    let c = &session.client;
    assert_eq!(c.manifest.mode, "local");
    let now = c.now().unwrap();
    let next = c.manifest.launch_time + ((now - c.manifest.launch_time) / 60 + 1) * 60 + 1;
    c.read
        .request("evm_setNextBlockTimestamp", json!([next]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    let work = c.work(session.account).unwrap().unwrap();
    let mut nonce = U256::ZERO;
    while !pow::valid(pow::digest(&work, session.account, nonce), work.target) {
        nonce += U256::from(1);
        assert!(nonce < U256::from(20_000_000));
    }
    let signer = wallet::unlock(
        &PathBuf::from(cfg["wallet"].as_str().unwrap()),
        Zeroizing::new(cfg["password"].as_str().unwrap().into()),
    )
    .unwrap();
    let data = abi::call(
        "submit(uint48,address,uint256)",
        &[
            abi::uint(U256::from(work.epoch)),
            abi::address(session.account),
            abi::uint(nonce),
        ],
    );
    c.read
        .request("evm_setNextBlockTimestamp", json!([work.end_time - 14]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    let before = c.nonce(session.account, "latest").unwrap();
    let error = c
        .send(&signer, "mined-proof", c.manifest.ledger, data, Some(&work))
        .unwrap_err();
    assert!(error.to_string().starts_with("STALE_WORK:"));
    assert_eq!(c.nonce(session.account, "latest").unwrap(), before);
    c.read
        .request("evm_setNextBlockTimestamp", json!([work.end_time + 1]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    let work = c.work(session.account).unwrap().unwrap();
    nonce = U256::ZERO;
    while !pow::valid(pow::digest(&work, session.account, nonce), work.target) {
        nonce += U256::from(1);
        assert!(nonce < U256::from(20_000_000));
    }
    c.send(
        &signer,
        "mined-proof",
        c.manifest.ledger,
        abi::call(
            "submit(uint48,address,uint256)",
            &[
                abi::uint(U256::from(work.epoch)),
                abi::address(session.account),
                abi::uint(nonce),
            ],
        ),
        Some(&work),
    )
    .unwrap();
    c.read
        .request("evm_setNextBlockTimestamp", json!([work.end_time + 1]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    session.sync_history().unwrap();
    let stop = session.stop.clone();
    let observed = claimed.clone();
    let monitor = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(60);
        while !observed.load(Ordering::SeqCst) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(50));
        }
        stop.store(true, Ordering::SeqCst);
    });
    let result = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.01,
            seconds: 65,
            max_accepted: 0,
            auto_claim_after: 1,
        })
        .unwrap();
    monitor.join().unwrap();
    assert!(
        claimed.load(Ordering::SeqCst),
        "eligible history should trigger claims without waiting for a new accepted proof: {result}"
    );
    assert!(result["autoClaimed"].as_u64().unwrap_or(0) > 0);
    drop(session);
    let mut limited = options;
    limited.fee_budget = Some(U256::from(1));
    let session = Session::connect(
        limited,
        Zeroizing::new(cfg["password"].as_str().unwrap().into()),
        Arc::new(|_| {}),
    )
    .unwrap();
    let c = &session.client;
    let before = c.nonce(session.account, "latest").unwrap();
    let error = c
        .send(
            &signer,
            "budget-check",
            c.manifest.ledger,
            abi::call(
                "openEpoch(uint48)",
                &[abi::uint(U256::from(c.epoch().unwrap()))],
            ),
            None,
        )
        .unwrap_err();
    assert!(
        error.to_string().starts_with("FEE_BUDGET_EXHAUSTED:"),
        "{error}"
    );
    assert_eq!(c.nonce(session.account, "latest").unwrap(), before);
}
