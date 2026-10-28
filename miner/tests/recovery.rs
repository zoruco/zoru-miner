//! Signed transaction recovery against an explicitly selected isolated Anvil.
//! Run with the private fixture options described in docs/TESTING.md.
use alloy_consensus::{SignableTransaction, TxEnvelope, TxLegacy};
use alloy_eips::eip2718::Encodable2718;
use alloy_primitives::{Bytes, TxKind, U256};
use alloy_signer::SignerSync;
use alloy_signer_local::PrivateKeySigner;
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
use zeroize::Zeroizing;
use zoru_miner::{
    abi, pow,
    protocol::{CHAIN, Client, GAS_PRICE, Journal, Manifest},
    storage, wallet,
};

fn prepared(
    c: &Client,
    signer: &PrivateKeySigner,
    nonce: u64,
    data: &str,
    gas_price: u128,
) -> Journal {
    let tx = TxLegacy {
        chain_id: Some(CHAIN),
        nonce,
        gas_price,
        gas_limit: 900_000,
        to: TxKind::Call(c.manifest.ledger),
        value: U256::ZERO,
        input: Bytes::from(abi::bytes(data).unwrap()),
    };
    let sig = signer.sign_hash_sync(&tx.signature_hash()).unwrap();
    let raw = TxEnvelope::from(tx.into_signed(sig)).encoded_2718();
    Journal {
        chain_id: CHAIN,
        account: signer.address(),
        nonce,
        hash: abi::keccak(&raw),
        raw: format!("0x{}", hex::encode(raw)),
        state: "prepared".into(),
        label: "recovery-regression".into(),
        receipt: None,
        replacement: None,
    }
}
fn record(c: &Client, j: &Journal) {
    storage::atomic_json(
        &c.journal
            .join(format!("{:#x}-{:020}.json", j.account, j.nonce)),
        j,
    )
    .unwrap();
}

#[test]
#[ignore = "requires an isolated Anvil deployment and disposable encrypted wallet"]
fn signed_retry_restart_reorg_and_replacement() {
    let options: Value = serde_json::from_slice(
        &fs::read(std::env::var("ZORU_TEST_OPTIONS").expect("ZORU_TEST_OPTIONS")).unwrap(),
    )
    .unwrap();
    let manifest = Manifest::load(&PathBuf::from(options["manifest"].as_str().unwrap())).unwrap();
    assert_eq!(manifest.mode, "local");
    let signer = wallet::unlock(
        &PathBuf::from(options["wallet"].as_str().unwrap()),
        Zeroizing::new(options["password"].as_str().unwrap().to_owned()),
    )
    .unwrap();
    let dir = tempfile::tempdir().unwrap();
    let _lock = storage::WriterLock::acquire(
        &storage::default_root().unwrap().join("signers"),
        &format!("{:#x}", signer.address()),
    )
    .unwrap();
    let c = Client::new(
        manifest,
        options["read"].as_str().unwrap(),
        options["submit"].as_str().unwrap(),
        "http://127.0.0.1:1",
        dir.path().join("journal"),
        dir.path().join("receipts"),
    )
    .unwrap();
    c.verify().unwrap();
    let mut wrong_owner = c.manifest.clone();
    wrong_owner.owner = alloy_primitives::Address::repeat_byte(0x11);
    assert_ne!(wrong_owner.owner, c.manifest.owner);
    let wrong = Client::new(
        wrong_owner,
        options["read"].as_str().unwrap(),
        options["submit"].as_str().unwrap(),
        "http://127.0.0.1:1",
        dir.path().join("wrong-owner-journal"),
        dir.path().join("wrong-owner-receipts"),
    )
    .unwrap();
    assert!(
        wrong
            .verify()
            .unwrap_err()
            .to_string()
            .contains("Token owner differs")
    );
    let now = c.now().unwrap();
    let launch = c.manifest.launch_time;
    let next = launch + ((now.saturating_sub(launch)) / 60 + 1) * 60 + 1;
    c.read
        .request("evm_setNextBlockTimestamp", json!([next]))
        .unwrap();
    c.read.request("evm_mine", json!([])).unwrap();
    let nonce_before_owner_check = c.nonce(signer.address(), "latest").unwrap();
    let owner_error = wrong
        .send(
            &signer,
            "owner-intent-regression",
            c.manifest.ledger,
            abi::call(
                "openEpoch(uint48)",
                &[abi::uint(U256::from(c.epoch().unwrap()))],
            ),
            None,
        )
        .unwrap_err();
    assert!(owner_error.to_string().contains("Token owner differs"));
    assert_eq!(
        c.nonce(signer.address(), "latest").unwrap(),
        nonce_before_owner_check,
        "owner mismatch must stop a new signature before broadcast"
    );
    let work = c.work(signer.address()).unwrap().unwrap();
    let mut nonce = U256::ZERO;
    while !pow::valid(pow::digest(&work, signer.address(), nonce), work.target) {
        nonce += U256::from(1);
        assert!(nonce < U256::from(20_000_000));
    }
    let data = abi::call(
        "submit(uint48,address,uint256)",
        &[
            abi::uint(U256::from(work.epoch)),
            abi::address(signer.address()),
            abi::uint(nonce),
        ],
    );
    storage::atomic_json(
        &PathBuf::from(options["faultControl"].as_str().unwrap()),
        &json!({"dropNextSendResponse":1}),
    )
    .unwrap();
    let before = c.nonce(signer.address(), "latest").unwrap();
    let receipt = c
        .send(
            &signer,
            "lost-response",
            c.manifest.ledger,
            data.clone(),
            Some(&work),
        )
        .unwrap();
    assert_eq!(receipt["status"], "0x1");
    assert_eq!(c.nonce(signer.address(), "latest").unwrap(), before + 1);
    assert!(
        c.send(&signer, "duplicate-proof", c.manifest.ledger, data, None)
            .is_err()
    );
    assert_eq!(c.nonce(signer.address(), "latest").unwrap(), before + 1);
    c.reconcile(signer.address(), true).unwrap();
    assert_eq!(c.nonce(signer.address(), "latest").unwrap(), before + 1);
    let data = abi::call(
        "openEpoch(uint48)",
        &[abi::uint(U256::from(c.epoch().unwrap()))],
    );
    for broadcast in [false, true] {
        let nonce = c.nonce(signer.address(), "latest").unwrap();
        let journal = prepared(&c, &signer, nonce, &data, GAS_PRICE);
        record(&c, &journal);
        if broadcast {
            c.submit
                .request("eth_sendRawTransaction", json!([journal.raw]))
                .unwrap();
            c.receipt(journal.hash, Duration::from_secs(10)).unwrap();
        }
        c.reconcile(signer.address(), true).unwrap();
        assert_eq!(c.nonce(signer.address(), "latest").unwrap(), nonce + 1);
        c.reconcile(signer.address(), true).unwrap();
        assert_eq!(c.nonce(signer.address(), "latest").unwrap(), nonce + 1);
    }
    let snap = c.read.request("evm_snapshot", json!([])).unwrap();
    let nonce = c.nonce(signer.address(), "latest").unwrap();
    let journal = prepared(&c, &signer, nonce, &data, GAS_PRICE);
    record(&c, &journal);
    c.reconcile(signer.address(), true).unwrap();
    assert_eq!(c.nonce(signer.address(), "latest").unwrap(), nonce + 1);
    assert_eq!(c.read.request("evm_revert", json!([snap])).unwrap(), true);
    assert!(
        c.read
            .request("eth_getTransactionReceipt", json!([journal.hash]))
            .unwrap()
            .is_null()
    );
    c.reconcile(signer.address(), true).unwrap();
    assert_eq!(c.nonce(signer.address(), "latest").unwrap(), nonce + 1);
    let nonce = c.nonce(signer.address(), "latest").unwrap();
    let original = prepared(&c, &signer, nonce, &data, GAS_PRICE);
    record(&c, &original);
    let replacement = prepared(&c, &signer, nonce, &data, GAS_PRICE * 12 / 10);
    assert_ne!(replacement.hash, original.hash);
    c.submit
        .request("eth_sendRawTransaction", json!([replacement.raw]))
        .unwrap();
    c.receipt(replacement.hash, Duration::from_secs(10))
        .unwrap();
    c.reconcile(signer.address(), true).unwrap();
    let saved: Journal = serde_json::from_slice(
        &fs::read(
            c.journal
                .join(format!("{:#x}-{:020}.json", signer.address(), nonce)),
        )
        .unwrap(),
    )
    .unwrap();
    assert_eq!(saved.state, "replaced");
    assert_eq!(saved.replacement, Some(replacement.hash));
    c.read.request("anvil_mine", json!(["0x81"])).unwrap();
    c.reconcile(signer.address(), true).unwrap();
    assert_eq!(
        fs::read_dir(&c.journal)
            .unwrap()
            .filter_map(Result::ok)
            .filter(|v| v.path().extension().is_some_and(|e| e == "json"))
            .count(),
        0
    );
    assert!(fs::read_dir(c.journal.join("resolved")).unwrap().count() >= 5);

    // A consumed nonce outside the bounded lookup must retain the original intent.
    let nonce = c.nonce(signer.address(), "latest").unwrap();
    let data = abi::call(
        "openEpoch(uint48)",
        &[abi::uint(U256::from(c.epoch().unwrap()))],
    );
    let unresolved = prepared(&c, &signer, nonce, &data, GAS_PRICE);
    record(&c, &unresolved);
    let cancellation = prepared(&c, &signer, nonce, &data, GAS_PRICE * 12 / 10);
    c.submit
        .request("eth_sendRawTransaction", json!([cancellation.raw]))
        .unwrap();
    c.receipt(cancellation.hash, Duration::from_secs(10))
        .unwrap();
    c.read.request("anvil_mine", json!(["0x21"])).unwrap();
    for _ in 0..2 {
        let error = c.reconcile(signer.address(), true).unwrap_err();
        assert!(
            error
                .to_string()
                .contains("Consumed nonce could not be reconciled")
        );
        assert_eq!(c.nonce(signer.address(), "latest").unwrap(), nonce + 1);
        let saved: Journal = serde_json::from_slice(
            &fs::read(
                c.journal
                    .join(format!("{:#x}-{:020}.json", signer.address(), nonce)),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(saved.state, "prepared");
        assert_eq!(saved.hash, unresolved.hash);
        assert_eq!(saved.raw, unresolved.raw);
    }
    let report = json!({"status":"PASS","signedByRust":true,"lostResponse":true,"duplicateProof":true,"restartBeforeBroadcast":true,"restartAfterBroadcast":true,"orphanedReceiptReplay":true,"sameNonceReplacement":true,"unresolvedConsumedNonceFailsClosed":true,"signer":signer.address(),"proofTransaction":receipt["transactionHash"]});
    storage::atomic_json(&PathBuf::from(options["report"].as_str().unwrap()), &report).unwrap();
}
