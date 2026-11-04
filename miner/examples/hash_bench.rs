//! Transaction-free comparison of worker hashing with prepared-prefix hashing.
use alloy_primitives::{Address, U256};
use std::{hint::black_box, time::Instant};
use zoru_miner::{
    abi,
    pow::{self, PreparedProof, Work},
};

fn main() {
    let work = Work {
        chain_id: 6736,
        ledger: Address::repeat_byte(1),
        token: Address::repeat_byte(2),
        generation: U256::from(1),
        epoch: 100,
        seed: abi::keccak(b"hash performance fixture"),
        target: U256::MAX >> 16,
        end_time: 0,
    };
    let beneficiary = Address::repeat_byte(3);
    let prepared = PreparedProof::new(&work, beneficiary);
    let mut preimage = pow::preimage(&work, beneficiary, U256::ZERO);
    let count = 250_000u64;
    let mut samples = Vec::new();
    for _ in 0..3 {
        let start = Instant::now();
        for nonce in 0..count {
            preimage[256..].copy_from_slice(&black_box(U256::from(nonce)).to_be_bytes::<32>());
            black_box(abi::keccak(black_box(&preimage)));
        }
        let full = start.elapsed().as_secs_f64();
        let start = Instant::now();
        for nonce in 0..count {
            black_box(prepared.digest(black_box(U256::from(nonce))));
        }
        let cached = start.elapsed().as_secs_f64();
        samples.push(serde_json::json!({"hashes":count,"workerPreimageSeconds":full,"preparedPrefixSeconds":cached,
            "workerPreimageHashesPerSecond":count as f64/full,"preparedPrefixHashesPerSecond":count as f64/cached}));
    }
    println!(
        "{}",
        serde_json::json!({"mode":"offchain-cpu-benchmark","transactions":0,"samples":samples})
    );
}
