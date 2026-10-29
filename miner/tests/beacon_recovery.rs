//! Recover obsolete signed publications without broadcasting or losing intent.
use alloy_consensus::{SignableTransaction, TxEnvelope, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Address, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use serde_json::{Value, json};
use std::{
    fs,
    io::{BufRead, BufReader, Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
use zoru_miner::{
    abi,
    protocol::{CHAIN, Client, Journal, Manifest},
    storage,
};

#[test]
fn obsolete_beacons_recover_and_reused_nonce_archives_preserve_both_hashes() {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let stop = Arc::new(AtomicBool::new(false));
    let server_stop = stop.clone();
    let calls = Arc::new(Mutex::new(Vec::new()));
    let server_calls = calls.clone();
    let server = thread::spawn(move || {
        while !server_stop.load(Ordering::Relaxed) {
            let (mut socket, _) = match listener.accept() {
                Ok(connection) => connection,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(1));
                    continue;
                }
                Err(error) => panic!("{error}"),
            };
            socket
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let mut reader = BufReader::new(&mut socket);
            let mut length = 0;
            let mut route = String::new();
            loop {
                let mut line = String::new();
                reader.read_line(&mut line).unwrap();
                if line == "\r\n" {
                    break;
                }
                if line.starts_with("POST ") {
                    route = line.split_whitespace().nth(1).unwrap().to_owned();
                }
                if let Some(value) = line.to_lowercase().strip_prefix("content-length:") {
                    length = value.trim().parse().unwrap();
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let mut request: Value = serde_json::from_slice(&body).unwrap();
            request["route"] = json!(route);
            server_calls.lock().unwrap().push(request.clone());
            let result = match request["method"].as_str().unwrap() {
                "eth_chainId" if route == "/wrong-chain" => json!("0x1"),
                "eth_chainId" => json!(format!("0x{CHAIN:x}")),
                "eth_gasPrice" if route == "/submit" => json!("0x2e90edd000"),
                "eth_gasPrice" => json!("0x22ecb25c00"),
                "web3_clientVersion" => json!("anvil"),
                "eth_blockNumber" => json!("0x100"),
                "eth_getBlockByNumber" => json!({"timestamp":"0x10000"}),
                "eth_getTransactionReceipt" | "eth_getTransactionByHash" => Value::Null,
                "eth_getTransactionCount" => json!("0x5"),
                // Epoch 22 was never published, but its mining window is closed.
                "eth_call" => {
                    let epoch_22 = abi::call("verifiedSeed(uint48)", &[abi::uint(U256::from(22))]);
                    json!(format!(
                        "0x{}",
                        if request["params"][0]["data"] == epoch_22 {
                            "00"
                        } else {
                            "42"
                        }
                        .repeat(32)
                    ))
                }
                other => panic!("Unexpected RPC, recovery must not broadcast: {other}"),
            };
            let response = json!({"jsonrpc":"2.0","id":request["id"],"result":result}).to_string();
            write!(
                socket,
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{response}",
                response.len()
            )
            .unwrap();
        }
    });
    let dir = tempfile::tempdir().unwrap();
    let beacon = Address::repeat_byte(0x56);
    let manifest: Manifest = serde_json::from_value(json!({
        "schema":"zoru-mainnet-suite-1", "chainId":CHAIN, "mode":"local",
        "token":Address::repeat_byte(1), "ledger":Address::repeat_byte(2),
        "engine":Address::repeat_byte(3), "beacon":beacon, "admin":Address::repeat_byte(4),
        "owner":Address::repeat_byte(5), "launchTime":1, "epochSeconds":60,
        "decimals":8, "deploymentBlockNumber":"0x0", "drandChainHash":zoru_miner::protocol::DRAND_HASH,
        "runtimeCodeHashes":{}, "approvedImplementationCodeHashes":{}, "manifestSha256":"unused"
    })).unwrap();
    let client = Client::new(
        manifest.clone(),
        &format!("{endpoint}/read"),
        &format!("{endpoint}/submit"),
        "http://127.0.0.1:1",
        dir.path().join("journal"),
        dir.path().join("receipts"),
    )
    .unwrap();
    assert_eq!(client.gas_price().unwrap(), 200_000_000_000);
    let wrong = Client::new(
        manifest,
        &format!("{endpoint}/read"),
        &format!("{endpoint}/wrong-chain"),
        "http://127.0.0.1:1",
        dir.path().join("wrong-journal"),
        dir.path().join("wrong-receipts"),
    )
    .unwrap();
    assert!(
        wrong
            .gas_price()
            .unwrap_err()
            .to_string()
            .contains("Wrong EVM chain")
    );
    let signer = PrivateKeySigner::random();
    let mut hashes = Vec::new();
    for (label, epoch) in [
        ("publish-drand", 21u64),
        ("publish-drand", 22),
        ("mined-proof", 23),
    ] {
        let (input, recipient) = if label == "publish-drand" {
            let mut input = abi::bytes(&abi::call(
                "publish(uint48,bytes)",
                &[
                    abi::uint(U256::from(epoch)),
                    abi::uint(U256::from(64)),
                    abi::uint(U256::from(64)),
                ],
            ))
            .unwrap();
            input.extend_from_slice(&[0x12; 64]);
            (input, beacon)
        } else {
            (
                abi::bytes(&abi::call(
                    "submit(uint48,address,uint256)",
                    &[
                        abi::uint(U256::from(epoch)),
                        abi::address(signer.address()),
                        abi::uint(U256::from(7)),
                    ],
                ))
                .unwrap(),
                client.manifest.ledger,
            )
        };
        let tx = TxLegacy {
            chain_id: Some(CHAIN),
            nonce: 5,
            gas_price: 150_000_000_000,
            gas_limit: 900_000,
            to: recipient.into(),
            value: U256::ZERO,
            input: input.into(),
        };
        let signature = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
        let raw = TxEnvelope::from(tx.into_signed(signature)).encoded_2718();
        let hash = abi::keccak(&raw);
        hashes.push(hash);
        let record = json!({"chain_id":CHAIN,"account":signer.address(),"nonce":5,
            "hash":hash,"raw":format!("0x{}",hex::encode(raw)),"state":"prepared",
            "label":label,"receipt":null,"replacement":null});
        assert!(record.get("beacon_epoch").is_none());
        let path = client
            .journal
            .join(format!("{:#x}-{:020}.json", signer.address(), 5));
        storage::atomic_json(&path, &record).unwrap();
        client.reconcile(signer.address(), true).unwrap();
        assert!(!path.exists());
        let archived = client.journal.join("resolved").join(format!(
            "{:#x}-{:020}-{hash:#x}.json",
            signer.address(),
            5
        ));
        let saved: Journal = serde_json::from_slice(&fs::read(archived).unwrap()).unwrap();
        assert_eq!(saved.state, "superseded");
        assert_eq!(saved.hash, hash);
        assert_eq!(saved.raw, record["raw"].as_str().unwrap());
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_eq!(
        fs::read_dir(client.journal.join("resolved"))
            .unwrap()
            .count(),
        3
    );
    let calls = calls.lock().unwrap();
    assert!(
        !calls
            .iter()
            .any(|call| call["route"] == "/read" && call["method"] == "eth_gasPrice")
    );
    assert!(
        !calls
            .iter()
            .any(|call| call["route"] == "/wrong-chain" && call["method"] == "eth_gasPrice")
    );
    for epoch in [21u64, 22] {
        assert!(calls.iter().any(|call| call["method"] == "eth_call"
            && call["params"][0]["data"]
                == abi::call("verifiedSeed(uint48)", &[abi::uint(U256::from(epoch))])));
    }
    stop.store(true, Ordering::Relaxed);
    server.join().unwrap();
}
