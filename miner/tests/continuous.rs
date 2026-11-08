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

fn generation_progressed(states: &[Value], generation: &str) -> bool {
    let hashes: Vec<u64> = states
        .iter()
        .filter(|s| {
            s["status"] == "mining"
                && s["contexts"]
                    .as_array()
                    .and_then(|contexts| contexts.last())
                    .is_some_and(|work| work["generation"] == generation)
        })
        .filter_map(|s| s["hashes"].as_u64())
        .collect();
    hashes.len() >= 2 && hashes.last() > hashes.first()
}

fn resumed_after_thaw(states: &[Value]) -> bool {
    let hashes: Vec<u64> = states
        .iter()
        .filter(|s| s["status"] == "mining" && s["elapsedSeconds"].as_f64().unwrap_or(0.0) >= 20.0)
        .filter_map(|s| s["hashes"].as_u64())
        .collect();
    hashes.len() >= 2 && hashes.last() > hashes.first()
}

fn wait_for_headroom() {
    let deadline = Instant::now() + Duration::from_secs(180);
    let cpus = thread::available_parallelism().map_or(1, usize::from) as f64;
    let mut calm_since = Instant::now();
    loop {
        let load = fs::read_to_string("/proc/loadavg").ok().and_then(|s| {
            s.split_whitespace()
                .next()
                .and_then(|v| v.parse::<f64>().ok())
        });
        if zoru_miner::session::resource_blocked() || load.is_some_and(|n| n >= cpus * 0.8) {
            calm_since = Instant::now();
        } else if calm_since.elapsed() >= Duration::from_secs(5) {
            return;
        }
        assert!(
            Instant::now() < deadline,
            "Insufficient resource headroom for concurrency evidence; production guards remain enabled"
        );
        thread::sleep(Duration::from_millis(250));
    }
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
    let slow_inspection = json!({"data":abi::call("totalProofs()", &[]),"milliseconds":10_000});
    storage::atomic_json(&control, &json!({"delayCall":slow_inspection})).unwrap();
    let inspections = Arc::new(Mutex::new(Vec::<Value>::new()));
    let inspection_events = inspections.clone();
    let states = Arc::new(Mutex::new(Vec::new()));
    let observed = states.clone();
    let delayed_proof = Arc::new(AtomicBool::new(false));
    let delayed_claim = Arc::new(AtomicBool::new(false));
    let delay_proof = delayed_proof.clone();
    let delay_claim = delayed_claim.clone();
    let tx_hash = Arc::new(Mutex::new(None::<B256>));
    let submitted = tx_hash.clone();
    let controls = control.clone();
    let slow_read = slow_inspection.clone();
    let directory = PathBuf::from(cfg["report"].as_str().unwrap())
        .parent()
        .unwrap()
        .join("continuous-state");
    fs::create_dir_all(&directory).unwrap();
    let session = Session::connect(Options {
        manifest:PathBuf::from(cfg["manifest"].as_str().unwrap()),
        read:cfg["faultRead"].as_str().unwrap().into(), submit:cfg["submit"].as_str().unwrap().into(),native:None,
        wallet:Some(PathBuf::from(cfg["wallet"].as_str().unwrap())),expected_wallet:None,observer:None,
        fee_budget:None,sync_on_connect:true,state_dir:directory.clone(),
    },Zeroizing::new(cfg["password"].as_str().unwrap().into()),Arc::new(move |event| {
        if event["event"]=="inspection" {
            inspection_events.lock().unwrap().push(event["data"].clone());
        }
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
                let mut faults = json!({"delayReceiptsUntil":SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs_f64()+6.0});
                if proof { faults["delayCall"] = slow_read.clone(); }
                storage::atomic_json(&controls,&faults).unwrap();
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
        .arg(directory.join("reader"))
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
    let snapshots = String::from_utf8_lossy(&inspected.stdout)
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|event| event["event"] == "inspection")
        .count();
    assert_eq!(
        snapshots, 1,
        "A read-only command must fetch its snapshot once"
    );
    assert_eq!(
        session.client.nonce(session.account, "latest").unwrap(),
        before
    );
    let record = |phase: &str, result: &Value| {
        storage::atomic_json(
            &directory.join(format!("phase-{phase}.json")),
            &json!({
                "result":result,"states":*states.lock().unwrap(),
                "transactions":*session.client.transactions.lock().unwrap()
            }),
        )
        .unwrap();
    };
    let launch = session.client.manifest.launch_time;
    wait_for_headroom();
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
    // Let the deliberately slow snapshot finish while proofs continue. An
    // exact proof limit can stop its RPC sequence before it publishes any
    // snapshot when earlier fixtures have left more history to synchronize.
    // The separate mining_limit fixture covers the exact writer limit.
    let completed_states = states.clone();
    let completed_inspections = inspections.clone();
    let completed_stop = session.stop.clone();
    let completion = thread::spawn(move || {
        while !completed_stop.load(Ordering::Relaxed) {
            let accepted = completed_states
                .lock()
                .unwrap()
                .last()
                .and_then(|s| s["accepted"].as_u64())
                .unwrap_or(0);
            let incomplete = completed_inspections.lock().unwrap().iter().any(|s| {
                s["myRewardsHistoryComplete"] == false && s["myUnclaimedBaseUnits"].is_null()
            });
            if accepted >= 2 && incomplete {
                completed_stop.store(true, Ordering::SeqCst);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let result = session
        .run(Run {
            workers: 2,
            cpu_budget: 0.5,
            seconds: 100,
            max_accepted: 0,
            auto_claim_after: 0,
            dry_run: false,
        })
        .unwrap();
    rollover.join().unwrap();
    completion.join().unwrap();
    record("proof", &result);
    assert!(result["accepted"].as_u64().unwrap_or(0) >= 2, "{result}");
    assert_eq!(result["rejected"], 0, "{result}");
    assert_eq!(
        result["errors"],
        json!([]),
        "Slow inspection must not mix proof heights: {result}"
    );
    assert!(
        inspections
            .lock()
            .unwrap()
            .iter()
            .any(|snapshot| snapshot["myRewardsHistoryComplete"] == false
                && snapshot["myUnclaimedBaseUnits"].is_null()),
        "Concurrent history changes must leave personal totals unavailable until a consistent snapshot"
    );
    storage::atomic_json(&control, &json!({})).unwrap();
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
    wait_for_headroom();
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
    record("claim", &claims);
    assert!(claims["autoClaimed"].as_u64().unwrap_or(0) > 0, "{claims}");
    assert!(delayed_claim.load(Ordering::SeqCst));
    assert!(
        progressed(&states.lock().unwrap(), "claim"),
        "hashing must continue during claim receipt tracking; inspect phase-claim.json; result: {claims}"
    );
    wait_for_headroom();
    next_epoch(&raw, launch);
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
    record("outage", &outage);
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
    wait_for_headroom();
    next_epoch(&raw, launch);
    states.lock().unwrap().clear();
    let frozen = raw
        .request("eth_getBlockByNumber", json!(["latest", false]))
        .unwrap();
    storage::atomic_json(&control, &json!({"frozenBlock":frozen})).unwrap();
    let thawed = control.clone();
    let thaw_states = states.clone();
    let thaw_stop = session.stop.clone();
    let thaw = thread::spawn(move || {
        thread::sleep(Duration::from_secs(19));
        storage::atomic_json(&thawed, &json!({})).unwrap();
        while !thaw_stop.load(Ordering::Relaxed) {
            if resumed_after_thaw(&thaw_states.lock().unwrap()) {
                thaw_stop.store(true, Ordering::Relaxed);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    session.begin_action();
    let freeze = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.1,
            seconds: 90,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    thaw.join().unwrap();
    record("frozen-head", &freeze);
    assert_eq!(freeze["rejected"], 0);
    let frozen_states = states.lock().unwrap();
    let waiting: Vec<_> = frozen_states
        .iter()
        .filter(|s| {
            s["status"] == "waiting-beacon-or-recovery"
                && (16.0..19.0).contains(&s["elapsedSeconds"].as_f64().unwrap_or(0.0))
        })
        .collect();
    assert!(
        waiting.len() >= 2,
        "an expired work lease must hold hashing on a frozen head"
    );
    assert!(waiting.windows(2).any(|s| s[0]["hashes"] == s[1]["hashes"]));
    assert!(
        resumed_after_thaw(&frozen_states),
        "hashes must increase on an advancing head; inspect phase-frozen-head.json; result: {freeze}"
    );
    drop(frozen_states);
    wait_for_headroom();
    next_epoch(&raw, launch);
    states.lock().unwrap().clear();
    let upgrade_chain = raw.clone();
    let upgrade_states = states.clone();
    let upgrade_stop = session.stop.clone();
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
        // Give the production resource guard time to permit real work. Stop
        // only after hashes advance on the verified new generation; a status
        // label or a resource pause alone cannot satisfy this assertion.
        while !upgrade_stop.load(Ordering::Relaxed) {
            if generation_progressed(&upgrade_states.lock().unwrap(), "2") {
                upgrade_stop.store(true, Ordering::Relaxed);
                break;
            }
            thread::sleep(Duration::from_millis(100));
        }
    });
    let upgrade = session
        .run(Run {
            workers: 1,
            cpu_budget: 0.1,
            seconds: 90,
            dry_run: true,
            auto_claim_after: 0,
            ..Run::default()
        })
        .unwrap();
    upgrading.join().unwrap();
    record("upgrade", &upgrade);
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
        generation_progressed(&states.lock().unwrap(), "2"),
        "hashes must increase after upgrade verification; inspect phase-upgrade.json; result: {upgrade}"
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
