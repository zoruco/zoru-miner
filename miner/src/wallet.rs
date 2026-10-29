use alloy_signer_local::PrivateKeySigner;
use anyhow::{Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
use zeroize::Zeroizing;

fn parameter_set(value: &Value, names: &[&str]) -> bool {
    value.as_object().is_some_and(|fields| {
        fields.len() == names.len() && names.iter().all(|name| fields.contains_key(*name))
    })
}

pub fn unlock(path: &Path, password: Zeroizing<String>) -> Result<PrivateKeySigner> {
    let mut data = Vec::new();
    fs::File::open(path)?
        .take(1024 * 1024 + 1)
        .read_to_end(&mut data)?;
    ensure!(data.len() <= 1024 * 1024, "Wallet file is too large");
    let mut value: Value = serde_json::from_slice(&data)?;
    ensure!(
        !(value.get("crypto").is_some() && value.get("Crypto").is_some()),
        "Ambiguous encrypted wallet crypto sections"
    );
    let crypto = value
        .get("crypto")
        .or_else(|| value.get("Crypto"))
        .ok_or_else(|| anyhow::anyhow!("Invalid encrypted wallet"))?;
    match crypto["kdf"].as_str() {
        Some("scrypt") => {
            let p = &crypto["kdfparams"];
            let n = p["n"].as_u64().unwrap_or(0);
            let r = p["r"].as_u64().unwrap_or(0);
            let parallel = p["p"].as_u64().unwrap_or(0);
            ensure!(
                parameter_set(p, &["n", "r", "p", "dklen", "salt"])
                    && n >= 2
                    && n.is_power_of_two()
                    && n <= 262144
                    && r > 0
                    && r <= 8
                    && parallel > 0
                    && parallel <= 8
                    && p["dklen"] == 32,
                "Wallet KDF exceeds safe limits"
            );
        }
        Some("pbkdf2") => {
            let p = &crypto["kdfparams"];
            ensure!(
                parameter_set(p, &["c", "dklen", "prf", "salt"])
                    && p["dklen"] == 32
                    && p["prf"] == "hmac-sha256"
                    && p["c"].as_u64().is_some_and(|v| v > 0 && v <= 1_000_000),
                "Wallet KDF exceeds safe limits"
            );
        }
        _ => bail!("Unsupported encrypted wallet"),
    }
    let hex_length = |value: &Value, bytes: usize| {
        value
            .as_str()
            .is_some_and(|text| text.len() == bytes * 2 && hex::decode(text).is_ok())
    };
    ensure!(
        value["version"] == 3
            && crypto["cipher"] == "aes-128-ctr"
            && hex_length(&crypto["cipherparams"]["iv"], 16)
            && hex_length(&crypto["ciphertext"], 32)
            && hex_length(&crypto["mac"], 32)
            && crypto["kdfparams"]["salt"]
                .as_str()
                .is_some_and(|salt| !salt.is_empty()
                    && salt.len() <= 2048
                    && salt.len() % 2 == 0
                    && hex::decode(salt).is_ok()),
        "Invalid encrypted wallet encoding"
    );
    // The upstream decoder uses lowercase names. Normalize only the validated
    // encrypted snapshot; never rewrite the user's wallet or read its path again.
    if let Some(crypto) = value.as_object_mut().and_then(|v| v.remove("Crypto")) {
        value["crypto"] = crypto;
    }
    let mut snapshot = tempfile::NamedTempFile::new()?;
    snapshot.write_all(&serde_json::to_vec(&value)?)?;
    let signer = PrivateKeySigner::decrypt_keystore(snapshot.path(), password.as_bytes())
        .map_err(|_| anyhow::anyhow!("Unable to unlock wallet"))?;
    ensure!(
        crate::abi::keccak(signer.address().as_slice())
            != alloy_primitives::b256!(
                "35dee2f24e860bfaf8d62e8f853c801602a8bbea6f640b9a319eb5079bb22fea"
            ),
        "Reserved owner account is not a mining wallet"
    );
    Ok(signer)
}
pub fn store(
    path: &Path,
    signer: &PrivateKeySigner,
    password: &str,
    replace: bool,
) -> Result<Value> {
    ensure!(
        password.chars().count() >= 12
            && password.len() <= 1024
            && !password.contains(['\n', '\r', '\0']),
        "Use at least 12 characters for the wallet password"
    );
    ensure!(replace || !path.exists(), "Wallet already exists");
    let parent = path
        .parent()
        .filter(|v| !v.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let temporary = tempfile::tempdir_in(parent)?;
    let key = Zeroizing::new(signer.to_bytes().0);
    PrivateKeySigner::encrypt_keystore(
        temporary.path(),
        &mut rand::thread_rng(),
        key.as_ref(),
        password.as_bytes(),
        Some("wallet.json"),
    )?;
    let source = temporary.path().join("wallet.json");
    let mut encrypted: Value = serde_json::from_slice(&fs::read(&source)?)?;
    encrypted["address"] = json!(format!("{:x}", signer.address()));
    fs::write(&source, serde_json::to_vec(&encrypted)?)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&source, fs::Permissions::from_mode(0o600))?;
    }
    fs::File::open(&source)?.sync_all()?;
    if replace {
        fs::rename(&source, path)?;
    } else {
        fs::hard_link(&source, path)?;
    }
    #[cfg(unix)]
    fs::File::open(parent)?.sync_all()?;
    Ok(json!({"address":signer.address(),"encrypted":true,"wallet":path}))
}
pub fn create(path: &Path, password: Zeroizing<String>) -> Result<Value> {
    store(path, &PrivateKeySigner::random(), &password, false)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn encrypted_wallet_roundtrip_password_and_no_clobber() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("wallet.json");
        let pass = "disposable test password";
        let info = create(&p, Zeroizing::new(pass.into())).unwrap();
        let signer = unlock(&p, Zeroizing::new(pass.into())).unwrap();
        assert_eq!(info["address"], format!("{:#x}", signer.address()));
        assert!(unlock(&p, Zeroizing::new("wrong password".into())).is_err());
        assert!(create(&p, Zeroizing::new(pass.into())).is_err());
    }
    #[test]
    fn standard_pbkdf2_wallet_unlocks() {
        // Public vector: private key 0x07 repeated 32 times; never use it for funds.
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let bytes = include_bytes!("../tests/fixtures/pbkdf2-v3.json");
        fs::write(&path, bytes).unwrap();
        let expected: Value = serde_json::from_slice(bytes).unwrap();
        let signer = unlock(&path, Zeroizing::new("public fixture password".into())).unwrap();
        let expected_address = format!(
            "0x{}",
            expected["address"]
                .as_str()
                .unwrap()
                .trim_start_matches("0x")
        )
        .parse::<alloy_primitives::Address>()
        .unwrap();
        assert_eq!(signer.address(), expected_address);
    }
    #[test]
    fn uppercase_crypto_wallet_unlocks_without_rewriting_the_input() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let mut value: Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/pbkdf2-v3.json")).unwrap();
        let crypto = value.as_object_mut().unwrap().remove("crypto").unwrap();
        value["Crypto"] = crypto;
        let bytes = serde_json::to_vec(&value).unwrap();
        fs::write(&path, &bytes).unwrap();
        let signer = unlock(&path, Zeroizing::new("public fixture password".into())).unwrap();
        assert_eq!(
            format!("{:x}", signer.address()),
            value["address"].as_str().unwrap().to_lowercase()
        );
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "import must preserve the original wallet file"
        );
        assert!(unlock(&path, Zeroizing::new("wrong password".into())).is_err());
        value["Crypto"]["kdfparams"]["c"] = json!(1_000_001);
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            unlock(&path, Zeroizing::new("public fixture password".into()))
                .unwrap_err()
                .to_string()
                .contains("KDF")
        );
    }
    #[test]
    fn ambiguous_crypto_sections_are_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let mut value: Value =
            serde_json::from_slice(include_bytes!("../tests/fixtures/pbkdf2-v3.json")).unwrap();
        value["Crypto"] = value["crypto"].clone();
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            unlock(&path, Zeroizing::new("public fixture password".into()))
                .unwrap_err()
                .to_string()
                .contains("Ambiguous")
        );
    }
    #[test]
    fn excessive_kdf_is_rejected_before_decryption() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("wallet.json");
        fs::write(
            &p,
            r#"{"crypto":{"kdf":"scrypt","kdfparams":{"n":1073741824,"r":8,"p":1,"dklen":32}}}"#,
        )
        .unwrap();
        assert!(
            unlock(&p, Zeroizing::new(String::new()))
                .unwrap_err()
                .to_string()
                .contains("KDF")
        );
    }
    #[test]
    fn pbkdf2_parameters_are_bounded_before_decryption() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let mut value = json!({"version":3,"id":"00000000-0000-4000-8000-000000000000","crypto":{
            "cipher":"aes-128-ctr","cipherparams":{"iv":"00".repeat(16)},"ciphertext":"00".repeat(32),"mac":"00".repeat(32),
            "kdf":"pbkdf2","kdfparams":{"c":1,"dklen":32,"prf":"hmac-sha256","salt":"00".repeat(32)}}});
        // Validate lengths before the dependency indexes the derived key bytes.
        for length in [0u64, 16, 31, 33, 1_073_741_824] {
            value["crypto"]["kdfparams"]["dklen"] = json!(length);
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            let error = unlock(&path, Zeroizing::new("test password".into())).unwrap_err();
            assert!(error.to_string().contains("KDF"));
        }
        value["crypto"]["kdfparams"]["dklen"] = json!(32);
        value["crypto"]["kdfparams"]["prf"] = json!("hmac-sha512");
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            unlock(&path, Zeroizing::new("test password".into()))
                .unwrap_err()
                .to_string()
                .contains("KDF")
        );
    }

    #[test]
    fn malformed_cipher_iv_is_rejected_without_panicking() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let password = "disposable cipher test password";
        create(&path, Zeroizing::new(password.into())).unwrap();
        let mut value: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        for iv in [String::new(), "00".into(), "11".repeat(17)] {
            value["crypto"]["cipherparams"]["iv"] = json!(iv);
            fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            assert!(unlock(&path, Zeroizing::new(password.into())).is_err());
        }
    }
    #[test]
    fn mixed_kdf_parameters_cannot_bypass_work_limits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("wallet.json");
        let value = json!({"version":3,"id":"00000000-0000-4000-8000-000000000000","crypto":{
            "cipher":"aes-128-ctr","cipherparams":{"iv":"00".repeat(16)},"ciphertext":"00".repeat(32),"mac":"00".repeat(32),
            "kdf":"scrypt","kdfparams":{"c":1,"dklen":32,"prf":"hmac-sha256","salt":"00".repeat(32),"n":8192,"r":8,"p":1}}});
        fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
        assert!(
            unlock(&path, Zeroizing::new("test password".into()))
                .unwrap_err()
                .to_string()
                .contains("KDF")
        );
    }
}
