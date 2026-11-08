//! Signed previous-epoch proofs and continuous hashing with a delayed seed.
use alloy_primitives::U256;
use serde_json::{Value, json};
use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex},
};
use zeroize::Zeroizing;
use zoru_miner::{
    abi, pow,
    session::{Options, Run, Session},
    wallet,
};

#[test]
#[ignore = "requires the isolated TEST ONLY delayed-beacon deployment"]
fn rollover_hashes_remain_reward_eligible_until_authenticated_handoff() {
    let cfg: Value =
        serde_json::from_slice(&fs::read(std::env::var("ZORU_TEST_OPTIONS").unwrap()).unwrap())
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let states = Arc::new(Mutex::new(Vec::<Value>::new()));
    let captured = states.clone();
    let session = Session::connect(
        Options {
            manifest: PathBuf::from(cfg["manifest"].as_str().unwrap()),
            read: cfg["read"].as_str().unwrap().into(),
            submit: cfg["submit"].as_str().unwrap().into(),
            native: None,
            wallet: Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),
            expected_wallet: None,
            observer: None,
            sync_on_connect: true,
            fee_budget: None,
            state_dir: directory.path().into(),
        },
        Zeroizing::new(cfg["password"].as_str().unwrap().into()),
        Arc::new(move |event| {
            if event["event"] == "state" {
                captured.lock().unwrap().push(event["data"].clone());
            }
        }),
    )
    .unwrap();
    let c = &session.client;
    assert_eq!(c.manifest.mode, "local");
    assert_eq!(c.manifest.rollover_grace_seconds, 30);
    let signer = wallet::unlock(
        &PathBuf::from(cfg["wallet"].as_str().unwrap()),
        Zeroizing::new(cfg["password"].as_str().unwrap().into()),
    )
    .unwrap();
    let advance = |time| {
        c.read
            .request("evm_setNextBlockTimestamp", json!([time]))
            .unwrap();
        c.read.request("evm_mine", json!([])).unwrap();
    };
    let launch = c.manifest.launch_time;
    let now = c.now().unwrap();
    let epoch = (now - launch) / 60 + 1;
    let start = launch + epoch * 60;
    advance(start + 21);
    let first = c.work(session.account).unwrap().unwrap();
    assert_eq!(first.epoch, epoch);
    let candidate = |work: &pow::Work, initial: u64| {
        let mut nonce = U256::from(initial);
        while !pow::valid(pow::digest(work, session.account, nonce), work.target) {
            nonce += U256::from(1);
        }
        abi::call(
            "submit(uint48,address,uint256)",
            &[
                abi::uint(U256::from(work.epoch)),
                abi::address(session.account),
                abi::uint(nonce),
            ],
        )
    };
    advance(start + 61);
    let bridge = c.work(session.account).unwrap().unwrap();
    assert_eq!(bridge, first);
    let receipt = c
        .send(
            &signer,
            "mined-proof",
            c.manifest.ledger,
            candidate(&bridge, 1_000_000),
            Some(&bridge),
        )
        .unwrap();
    assert_eq!(
        receipt["status"], "0x1",
        "rollover proof earns a canonical acceptance"
    );
    session.begin_action();
    let result = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.2,
            seconds: 20,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    let samples = states.lock().unwrap();
    let measured: Vec<_> = samples
        .iter()
        .filter(|s| {
            s["elapsedSeconds"].as_f64().is_some_and(|n| n >= 2.0) && s["status"] != "stopped"
        })
        .collect();
    assert!(measured.len() >= 20);
    assert!(
        measured
            .iter()
            .all(|s| s["hashesPerSecond"].as_f64().unwrap_or(0.0) > 0.0),
        "real hashrate must remain positive across delayed-seed handoff"
    );
    let pending: Vec<_> = measured
        .iter()
        .filter(|s| s["beaconPending"] == true)
        .collect();
    assert!(
        pending.len() >= 2
            && pending.last().unwrap()["hashes"].as_u64()
                > pending.first().unwrap()["hashes"].as_u64(),
        "hashing must progress while beacon verification is pending"
    );
    assert!(
        result["contexts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["epoch"] == epoch)
    );
    assert!(
        result["contexts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|w| w["epoch"] == epoch + 1)
    );
    drop(samples);
    c.cancelled
        .store(false, std::sync::atomic::Ordering::Relaxed);
    let before = c.nonce(session.account, "latest").unwrap();
    let error = c
        .send(
            &signer,
            "mined-proof",
            c.manifest.ledger,
            candidate(&bridge, 2_000_000),
            Some(&bridge),
        )
        .unwrap_err();
    assert!(error.to_string().contains("STALE_WORK"), "{error:#}");
    assert_eq!(
        c.nonce(session.account, "latest").unwrap(),
        before,
        "obsolete work must not consume a nonce"
    );
}
