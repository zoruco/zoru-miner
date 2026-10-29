use alloy_primitives::U256;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, sync::Arc};
use zeroize::Zeroizing;
use zoru_miner::session::{Options, Run, Session};

#[test]
#[ignore = "requires an isolated Anvil deployment and disposable encrypted wallet"]
fn accepted_proof_limit_stops_the_transaction_writer() {
    let cfg: Value =
        serde_json::from_slice(&fs::read(std::env::var("ZORU_TEST_OPTIONS").unwrap()).unwrap())
            .unwrap();
    let directory = tempfile::tempdir().unwrap();
    let session = Session::connect(
        Options {
            manifest: PathBuf::from(cfg["manifest"].as_str().unwrap()),
            read: cfg["read"].as_str().unwrap().into(),
            submit: cfg["submit"].as_str().unwrap().into(),
            native: "http://127.0.0.1:1".into(),
            wallet: Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),
            expected_wallet: None,
            state_dir: directory.path().into(),
        },
        Zeroizing::new(cfg["password"].as_str().unwrap().to_owned()),
        Arc::new(|_| {}),
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
    let before = c.number(c.manifest.ledger, "totalProofs()").unwrap();
    let result = session
        .run(Run {
            workers: 4,
            cpu_budget: 0.6,
            seconds: 120,
            max_accepted: 1,
        })
        .unwrap();
    assert_eq!(
        result["accepted"], 1,
        "queued candidates must not overshoot the requested limit: {result}"
    );
    assert_eq!(
        c.number(c.manifest.ledger, "totalProofs()").unwrap(),
        before + U256::from(1)
    );
    zoru_miner::storage::atomic_json(
        &PathBuf::from(cfg["report"].as_str().unwrap()).with_file_name("mining-limit.json"),
        &json!({"status":"PASS","accepted":result["accepted"],"validFound":result["validFound"],
            "hashes":result["hashes"],"processCpuSeconds":result["processCpuSeconds"]}),
    )
    .unwrap();
}
