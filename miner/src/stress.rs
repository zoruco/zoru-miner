use crate::{
    abi,
    pow::{self, Work},
    session::{process_cpu, resource_blocked},
};
use alloy_primitives::{Address, U256};
use anyhow::{Result, ensure};
use serde_json::{Value, json};
use std::{
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub fn run(clients: u64, workers: usize, budget: f64, seconds: u64) -> Result<Value> {
    ensure!(
        (1..=10_000_000).contains(&clients)
            && [1, 2, 4].contains(&workers)
            && budget > 0.0
            && budget <= 2.0
            && seconds > 0
            && seconds <= 600,
        "Invalid bounded stress configuration"
    );
    let work = Work {
        chain_id: 6736,
        ledger: Address::repeat_byte(0x11),
        token: Address::repeat_byte(0x22),
        generation: U256::from(1) << 128,
        epoch: 1,
        seed: abi::keccak(b"ZORU stress fixture"),
        target: U256::MAX >> 16,
        end_time: 0,
    };
    let next = Arc::new(AtomicU64::new(0));
    let processed = Arc::new(AtomicU64::new(0));
    let found = Arc::new(AtomicU64::new(0));
    let pauses = Arc::new(AtomicU64::new(0));
    let samples = Arc::new(Mutex::new(Vec::new()));
    let started = Instant::now();
    let deadline = started + Duration::from_secs(seconds);
    let cpu = process_cpu();
    let mut handles = Vec::new();
    let duty = (budget / workers as f64).min(0.8);
    for _ in 0..workers {
        let (next, processed, found, pauses, samples, work) = (
            next.clone(),
            processed.clone(),
            found.clone(),
            pauses.clone(),
            samples.clone(),
            work.clone(),
        );
        handles.push(thread::spawn(move||{
        while Instant::now()<deadline{
            if resource_blocked(){pauses.fetch_add(1,Ordering::Relaxed);thread::sleep(Duration::from_millis(100));continue}
            let slice=Instant::now();let active=slice+Duration::from_secs_f64(0.01*duty);
            loop{
                let id=next.fetch_add(1,Ordering::Relaxed);if id>=clients{return}
                let identity=abi::keccak(&id.to_be_bytes());let address=Address::from_slice(&identity.as_slice()[12..]);let nonce=(U256::from(1)<<128)+U256::from(id);
                let digest=pow::digest(&work,address,nonce);processed.fetch_add(1,Ordering::Relaxed);
                if pow::valid(digest,work.target){found.fetch_add(1,Ordering::Relaxed);let mut examples=samples.lock().unwrap();if examples.len()<16{examples.push(json!({"client":id,"beneficiary":address,"nonce":nonce.to_string(),"digest":digest}));}}
                if Instant::now()>=active{break}
            }
            if let Some(rest)=Duration::from_millis(10).checked_sub(slice.elapsed()){thread::sleep(rest)}
        }
    }));
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| anyhow::anyhow!("Stress worker failed"))?;
    }
    let completed = processed.load(Ordering::Relaxed);
    let elapsed = started.elapsed().as_secs_f64();
    Ok(
        json!({"status":if completed==clients{"PASS"}else{"BLOCKED_RESOURCE_OR_TIME_BUDGET"},"mode":"offchain-virtual-client-stress","physicalMinersClaimed":false,
        "clientsRequested":clients,"clientsProcessed":completed,"realCpuHashes":completed,"validProofsFound":found.load(Ordering::Relaxed),"workers":workers,"cpuBudgetCores":budget,
        "elapsedSeconds":elapsed,"processCpuSeconds":process_cpu()-cpu,"hashesPerSecond":completed as f64/elapsed.max(0.001),"resourcePauses":pauses.load(Ordering::Relaxed),"liveTransactions":0,"samples":*samples.lock().unwrap()}),
    )
}
