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

#[test]
fn malformed_wallet_finishes_connect_and_releases_the_command_slot() {
    let directory = tempfile::tempdir().unwrap();
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
    let manifest_path = directory.path().join("manifest.json");
    fs::write(&manifest_path, serde_json::to_vec(&manifest).unwrap()).unwrap();
    zoru_miner::protocol::Manifest::load(&manifest_path).unwrap();
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
