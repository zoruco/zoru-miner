//! Real Keccak proofs, delayed receipts, work rollover and claims on local EVM.
use alloy_primitives::{B256, U256};
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;
use zoru_miner::{
    abi,
    rpc::Rpc,
    session::{Options, Run, Session},
    storage,
};

fn next_epoch(rpc: &Rpc, launch: u64) {
    let now = abi::quantity(
        rpc.request("eth_getBlockByNumber", json!(["latest", false]))
            .unwrap()["timestamp"]
            .as_str()
            .unwrap(),
    )
    .unwrap()
    .to::<u64>();
    let next = launch + ((now - launch) / 60 + 1) * 60 + 1;
    rpc.request("evm_setNextBlockTimestamp", json!([next]))
        .unwrap();
    rpc.request("evm_mine", json!([])).unwrap();
}

fn progressed(states: &[Value], label: &str) -> bool {
    let hashes: Vec<u64> = states
        .iter()
        .filter(|s| {
            s["status"] == "mining"
                && s["transactionPipeline"]["label"] == label
                && s["transactionPipeline"]["status"] == "receipt-tracking"
        })
        .filter_map(|s| s["hashes"].as_u64())
        .collect();
    hashes.len() >= 2 && hashes.last() > hashes.first()
}

#[test]
#[ignore = "requires an isolated Anvil deployment and disposable encrypted wallet"]
fn hashing_continues_during_receipts_claims_rollover_and_recovers_from_outage() {
    let cfg: Value =
        serde_json::from_slice(&fs::read(std::env::var("ZORU_TEST_OPTIONS").unwrap()).unwrap())
            .unwrap();
    let raw = Arc::new(Rpc::new(cfg["read"].as_str().unwrap(), Duration::ZERO).unwrap());
    assert!(
        raw.request("web3_clientVersion", json!([]))
            .unwrap()
            .as_str()
            .unwrap()
            .to_lowercase()
            .contains("anvil")
    );
    let control = PathBuf::from(cfg["faultControl"].as_str().unwrap());
    storage::atomic_json(&control, &json!({})).unwrap();
    let states = Arc::new(Mutex::new(Vec::new()));
    let observed = states.clone();
    let delayed_proof = Arc::new(AtomicBool::new(false));
    let delayed_claim = Arc::new(AtomicBool::new(false));
    let delay_proof = delayed_proof.clone();
    let delay_claim = delayed_claim.clone();
    let tx_hash = Arc::new(Mutex::new(None::<B256>));
    let submitted = tx_hash.clone();
    let controls = control.clone();
    let directory = tempfile::tempdir().unwrap();
    let session = Session::connect(Options {
        manifest:PathBuf::from(cfg["manifest"].as_str().unwrap()),
        read:cfg["faultRead"].as_str().unwrap().into(), submit:cfg["submit"].as_str().unwrap().into(),native:None,
        wallet:Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),expected_wallet:None,observer:None,
        fee_budget:None,sync_on_connect:true,state_dir:directory.path().into(),
    },Zeroizing::new(cfg["password"].as_str().unwrap().into()),Arc::new(move |event| {
        if event["event"]=="state" {
            let mut states = observed.lock().unwrap();
            states.push(event["data"].clone());
            if states.len()>256 { states.remove(0); }
        }
        if event["event"]=="transaction" && event["data"]["status"]=="receipt-tracking" {
            let proof = event["data"]["label"]=="mined-proof";
            let claim = event["data"]["label"]=="claim";
            let first = if proof { !delay_proof.swap(true,Ordering::SeqCst) }
                else if claim { !delay_claim.swap(true,Ordering::SeqCst) } else {false};
            if first {
                storage::atomic_json(&controls,&json!({"delayReceiptsUntil":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()+6.0})).unwrap();
                if proof { *submitted.lock().unwrap()=event["data"]["hash"].as_str().unwrap().parse().ok(); }
            }
        }
    })).unwrap();
    assert_eq!(session.client.manifest.mode, "local");
    // A rewards reader must not acquire the wallet's already-held signing
    // lock, reconcile a journal, or change its nonce.
    let before = session.client.nonce(session.account, "latest").unwrap();
    let mut reader = Command::new(env!("CARGO_BIN_EXE_zoru-miner-cli"))
        .arg("rewards")
        .arg("--manifest")
        .arg(cfg["manifest"].as_str().unwrap())
        .arg("--rpc")
        .arg(cfg["read"].as_str().unwrap())
        .arg("--wallet")
        .arg(cfg["wallet"].as_str().unwrap())
        .arg("--state-dir")
        .arg(directory.path().join("reader"))
        .arg("--password-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(
        reader.stdin.take().unwrap(),
        "{}",
        cfg["password"].as_str().unwrap()
    )
    .unwrap();
    let inspected = reader.wait_with_output().unwrap();
    assert!(
        inspected.status.success(),
        "{}",
        String::from_utf8_lossy(&inspected.stdout)
    );
    assert!(String::from_utf8_lossy(&inspected.stdout).contains("\"event\":\"inspection\""));
    assert_eq!(
        session.client.nonce(session.account, "latest").unwrap(),
        before
    );
    let launch = session.client.manifest.launch_time;
    next_epoch(&raw, launch);
    let chain = raw.clone();
    let rollover = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(90);
        while Instant::now() < deadline {
            let hash = *tx_hash.lock().unwrap();
            if let Some(hash) = hash
                && !chain
                    .request("eth_getTransactionReceipt", json!([hash]))
                    .unwrap()
                    .is_null()
            {
                next_epoch(&chain, launch);
                return;
            }
            thread::sleep(Duration::from_millis(100));
        }
        panic!("No mined proof for the rollover fixture");
    });
    let result = session
        .run(Run {
            workers: 2,
            cpu_budget: 0.5,
            seconds: 100,
            max_accepted: 2,
            auto_claim_after: 0,
            dry_run: false,
        })
        .unwrap();
    rollover.join().unwrap();
    assert_eq!(result["accepted"], 2, "{result}");
    assert_eq!(result["rejected"], 0, "{result}");
    assert!(
        progressed(&states.lock().unwrap(), "mined-proof"),
        "hash count must increase while a proof receipt is delayed"
    );
    assert!(
        result["contexts"].as_array().unwrap().len() >= 2,
        "work must switch at rollover"
    );
    assert!(
        result["staleBeforeBroadcast"].as_u64().unwrap() > 0,
        "old queued candidates must be discarded"
    );
    states.lock().unwrap().clear();
    next_epoch(&raw, launch);
    session.begin_action();
    let claims = session
        .run(Run {
            workers: 2,
            cpu_budget: 0.2,
            seconds: 22,
            auto_claim_after: 1,
            ..Run::default()
        })
        .unwrap();
    assert!(claims["autoClaimed"].as_u64().unwrap_or(0) > 0, "{claims}");
    assert!(delayed_claim.load(Ordering::SeqCst));
    assert!(
        progressed(&states.lock().unwrap(), "claim"),
        "hashing must continue during claim receipt tracking"
    );
    states.lock().unwrap().clear();
    storage::atomic_json(&control, &json!({"outage":true})).unwrap();
    let restored = control.clone();
    let restore = thread::spawn(move || {
        thread::sleep(Duration::from_secs(3));
        storage::atomic_json(&restored, &json!({})).unwrap();
    });
    session.begin_action();
    let outage = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.1,
            seconds: 9,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    restore.join().unwrap();
    assert_eq!(outage["rejected"], 0);
    let seen = states.lock().unwrap();
    assert!(seen.iter().any(|s| s["status"] == "rpc-degraded"));
    assert!(
        seen.iter().any(|s| s["status"] == "mining"),
        "work must recover after the outage"
    );
    assert!(
        seen.iter()
            .all(|s| s["candidateQueueDepth"].as_u64().unwrap_or(0) <= 64)
    );
    drop(seen);
    next_epoch(&raw, launch);
    states.lock().unwrap().clear();
    let frozen = raw
        .request("eth_getBlockByNumber", json!(["latest", false]))
        .unwrap();
    storage::atomic_json(&control, &json!({"frozenBlock":frozen})).unwrap();
    let thawed = control.clone();
    let thaw = thread::spawn(move || {
        thread::sleep(Duration::from_secs(8));
        storage::atomic_json(&thawed, &json!({})).unwrap();
    });
    session.begin_action();
    let freeze = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.1,
            seconds: 13,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    thaw.join().unwrap();
    assert_eq!(freeze["rejected"], 0);
    let frozen_states = states.lock().unwrap();
    let waiting: Vec<_> = frozen_states
        .iter()
        .filter(|s| s["status"] == "waiting-beacon-or-recovery")
        .collect();
    assert!(
        waiting.len() >= 2,
        "an expired work lease must hold hashing on a frozen head"
    );
    assert!(waiting.windows(2).any(|s| s[0]["hashes"] == s[1]["hashes"]));
    assert!(
        frozen_states
            .iter()
            .rev()
            .take(4)
            .any(|s| s["status"] == "mining"),
        "hashing must resume on an advancing head"
    );
    drop(frozen_states);
    next_epoch(&raw, launch);
    states.lock().unwrap().clear();
    let upgrade_chain = raw.clone();
    let upgrade_states = states.clone();
    let engine = session.client.manifest.engine;
    let admin = session.client.manifest.admin;
    let owner = session.client.manifest.owner;
    session.begin_action();
    let upgrading = thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(30);
        while !upgrade_states
            .lock()
            .unwrap()
            .iter()
            .any(|s| s["status"] == "mining")
        {
            assert!(
                Instant::now() < deadline,
                "No valid work before the isolated upgrade"
            );
            thread::sleep(Duration::from_millis(100));
        }
        let slot = upgrade_chain
            .request(
                "eth_getStorageAt",
                json!([engine, zoru_miner::protocol::IMPL_SLOT, "latest"]),
            )
            .unwrap();
        let implementation = abi::abi_address(slot.as_str().unwrap()).unwrap();
        let zero = alloy_primitives::Address::ZERO;
        let tuple = format!("({zero:#x},0x,{implementation:#x},0x,{zero:#x},0x)");
        let encoded = Command::new("cast")
            .args([
                "calldata",
                "emergencyUpgrade((address,bytes,address,bytes,address,bytes))",
                &tuple,
            ])
            .output()
            .unwrap();
        assert!(encoded.status.success());
        let data = String::from_utf8(encoded.stdout).unwrap();
        // Reinstall the approved implementation on the disposable suite to
        // exercise a genuine coordinator generation change without new code.
        let hash = upgrade_chain
            .request(
                "eth_sendTransaction",
                json!([{"from":owner,"to":admin,"data":data.trim(),"gas":"0x989680"}]),
            )
            .unwrap();
        loop {
            let receipt = upgrade_chain
                .request("eth_getTransactionReceipt", json!([hash]))
                .unwrap();
            if !receipt.is_null() {
                assert_eq!(receipt["status"], "0x1");
                break;
            }
            assert!(Instant::now() < deadline, "Isolated upgrade was not mined");
            thread::sleep(Duration::from_millis(100));
        }
    });
    let upgrade = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.1,
            seconds: 20,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    upgrading.join().unwrap();
    assert!(
        upgrade["contexts"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["generation"] == "2"),
        "new generation must be verified and activated"
    );
    assert_eq!(upgrade["rejected"], 0);
    assert!(upgrade["hashes"].as_u64().unwrap() > 0);
    assert!(
        states
            .lock()
            .unwrap()
            .iter()
            .rev()
            .take(4)
            .any(|s| s["status"] == "mining"),
        "hashing must resume after upgrade verification"
    );
    session.begin_action();
    session.client.reconcile(session.account, true).unwrap();
    let pending = session.client.nonce(session.account, "pending").unwrap();
    assert_eq!(
        pending,
        session.client.nonce(session.account, "latest").unwrap()
    );
    assert!(
        session
            .client
            .number(session.client.manifest.ledger, "totalProofs()")
            .unwrap()
            > U256::ZERO
    );
    storage::atomic_json(
        &PathBuf::from(cfg["report"].as_str().unwrap()).with_file_name("continuous.json"),
        &json!({"status":"PASS","proofRun":result,"claimRun":claims,"outageRun":outage,"frozenHeadRun":freeze,"upgradeRun":upgrade}),
    )
    .unwrap();
}
