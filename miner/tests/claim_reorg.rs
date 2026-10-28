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
    session::{Options, Session},
    wallet,
};

#[test]
#[ignore = "requires an isolated Anvil deployment and disposable encrypted wallet"]
fn orphaned_claims_do_not_remain_in_paid_rewards() {
    let cfg: Value =
        serde_json::from_slice(&fs::read(std::env::var("ZORU_TEST_OPTIONS").unwrap()).unwrap())
            .unwrap();
    let state = tempfile::tempdir().unwrap();
    let events = Arc::new(Mutex::new(Vec::<Value>::new()));
    let output = events.clone();
    let options = Options {
        manifest: PathBuf::from(cfg["manifest"].as_str().unwrap()),
        read: cfg["read"].as_str().unwrap().into(),
        submit: cfg["submit"].as_str().unwrap().into(),
        native: "http://127.0.0.1:1".into(),
        wallet: Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),
        state_dir: state.path().into(),
    };
    let session = Session::connect(
        options,
        Zeroizing::new(cfg["password"].as_str().unwrap().to_owned()),
        Arc::new(move |e| output.lock().unwrap().push(e)),
    )
    .unwrap();
    let c = &session.client;
    assert_eq!(c.manifest.mode, "local");
    session.settle().unwrap();
    let baseline = events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["event"] == "state")
        .unwrap()["data"]["rewardPaidBaseUnits"]
        .clone();
    let now = c.now().unwrap();
    let next = c.manifest.launch_time + ((now - c.manifest.launch_time) / 60 + 1) * 60 + 1;
    c.read
        .request("evm_setNextBlockTimestamp", json!([next]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    let snapshot = c.read.request("evm_snapshot", json!([])).unwrap();
    let signer = wallet::unlock(
        &PathBuf::from(cfg["wallet"].as_str().unwrap()),
        Zeroizing::new(cfg["password"].as_str().unwrap().to_owned()),
    )
    .unwrap();
    let work = c.work(session.account).unwrap().unwrap();
    let mut nonce = U256::ZERO;
    while !pow::valid(pow::digest(&work, session.account, nonce), work.target) {
        nonce += U256::from(1);
        assert!(nonce < U256::from(20_000_000));
    }
    c.send(
        &signer,
        "claim-reorg-proof",
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
    session.settle().unwrap();
    let paid = events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["event"] == "state")
        .unwrap()["data"]["rewardPaidBaseUnits"]
        .clone();
    assert_ne!(paid, baseline);
    assert_eq!(
        c.read.request("evm_revert", json!([snapshot])).unwrap(),
        true
    );
    session.sync_history().unwrap();
    session.publish().unwrap();
    let after = events
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|e| e["event"] == "state")
        .unwrap()["data"]["rewardPaidBaseUnits"]
        .clone();
    assert_eq!(
        after, baseline,
        "orphaned claim must not be counted as paid"
    );
    zoru_miner::storage::atomic_json(&PathBuf::from(cfg["report"].as_str().unwrap()).with_file_name("native-claim-reorg.json"),&json!({"status":"PASS","beforeReorgPaid":paid,"afterReorgPaid":after,"baselinePaid":baseline})).unwrap();
}
