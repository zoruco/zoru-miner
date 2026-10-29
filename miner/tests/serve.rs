use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader, Write},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::Duration,
};

fn test_manifest(directory: &std::path::Path) -> std::path::PathBuf {
    let mut manifest = json!({
        "schema":"zoru-mainnet-suite-1","chainId":6736,"mode":"local",
        "token":"0x1111111111111111111111111111111111111111",
        "ledger":"0x2222222222222222222222222222222222222222",
        "engine":"0x3333333333333333333333333333333333333333",
        "beacon":"0x4444444444444444444444444444444444444444",
        "admin":"0x5555555555555555555555555555555555555555",
        "owner":"0x6666666666666666666666666666666666666666",
        "launchTime":1,"epochSeconds":60,"decimals":8,"deploymentBlockNumber":"0x0",
        "drandChainHash":zoru_miner::protocol::DRAND_HASH,
        "runtimeCodeHashes":{},"approvedImplementationCodeHashes":{}
    });
    manifest["manifestSha256"] = json!(hex::encode(Sha256::digest(
        serde_json::to_vec(&manifest).unwrap()
    )));
    let manifest_path = directory.join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    zoru_miner::protocol::Manifest::load(&manifest_path).unwrap();
    manifest_path
}

#[test]
fn expected_wallet_mismatch_stops_before_rpc_or_journal_recovery() {
    let directory = tempfile::tempdir().unwrap();
    let manifest = test_manifest(directory.path());
    let wallet = directory.path().join("wallet.json");
    fs::write(&wallet, include_bytes!("fixtures/pbkdf2-v3.json")).unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}", listener.local_addr().unwrap());
    let root = directory.path().join("root");
    let mut child = Command::new(env!("CARGO_BIN_EXE_zoru-miner-cli"))
        .arg("inspect")
        .arg("--manifest")
        .arg(manifest)
        .arg("--wallet")
        .arg(wallet)
        .args([
            "--read", &endpoint, "--submit", &endpoint, "--native", &endpoint,
        ])
        .args([
            "--expected-wallet",
            "0x0000000000000000000000000000000000000001",
            "--password-stdin",
        ])
        .arg("--state-dir")
        .arg(directory.path().join("state"))
        .env("ZORU_STATE_ROOT", &root)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    writeln!(child.stdin.take().unwrap(), "public fixture password").unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(!output.status.success());
    let event: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["error"], "Wallet does not match --expected-wallet");
    assert!(
        !root.exists(),
        "the wrong wallet must not open signing state"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
}

#[test]
fn malformed_wallet_finishes_connect_and_releases_the_command_slot() {
    let directory = tempfile::tempdir().unwrap();
    let manifest_path = test_manifest(directory.path());
    let wallet_path = directory.path().join("wallet.json");
    fs::write(&wallet_path, serde_json::to_vec(&json!({"version":3,"id":"00000000-0000-4000-8000-000000000000","crypto":{
        "cipher":"aes-128-ctr","cipherparams":{"iv":"00".repeat(16)},"ciphertext":"00".repeat(32),"mac":"00".repeat(32),
        "kdf":"pbkdf2","kdfparams":{"c":1,"dklen":0,"prf":"hmac-sha256","salt":"00".repeat(32)}}})).unwrap()).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_zoru-miner-cli"))
        .arg("serve")
        .arg("--manifest")
        .arg(&manifest_path)
        .arg("--wallet")
        .arg(&wallet_path)
        .args([
            "--read",
            "http://127.0.0.1:1",
            "--submit",
            "http://127.0.0.1:1",
            "--native",
            "http://127.0.0.1:1",
        ])
        .arg("--state-dir")
        .arg(directory.path().join("state"))
        .env("ZORU_STATE_ROOT", directory.path().join("root"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let stdout = child.stdout.take().unwrap();
    let (tx, rx) = mpsc::channel();
    let reader = thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            if tx
                .send(serde_json::from_str::<Value>(&line.unwrap()).unwrap())
                .is_err()
            {
                break;
            }
        }
    });
    for action in ["connect", "inspect"] {
        writeln!(
            child.stdin.as_mut().unwrap(),
            "{}",
            json!({"action":action,"password":"not a private password"})
        )
        .unwrap();
        loop {
            let event = match rx.recv_timeout(Duration::from_secs(10)) {
                Ok(value) => value,
                Err(error) => {
                    let _ = child.kill();
                    panic!("Backend did not finish {action}: {error}");
                }
            };
            assert_ne!(
                event["error"],
                "Stop the current action before starting another"
            );
            if event["event"] == "command-complete" && event["action"] == action {
                break;
            }
        }
    }
    writeln!(
        child.stdin.as_mut().unwrap(),
        "{}",
        json!({"action":"quit"})
    )
    .unwrap();
    assert!(child.wait().unwrap().success());
    reader.join().unwrap();
}
