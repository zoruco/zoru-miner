use alloy_signer_local::PrivateKeySigner;
use anyhow::{Result, anyhow, ensure};
use argon2::{Algorithm, Argon2, Params, Version};
use chacha20poly1305::{
    KeyInit, XChaCha20Poly1305, XNonce,
    aead::{Aead, Payload},
};
use serde::Deserialize;
use std::{
    fs,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::{Zeroize, Zeroizing};

// Decoder for the original encrypted ZORU wallet, used only at the migration boundary.
// The encrypted source is archived before a Web3 v3 keystore is created.
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Envelope {
    format: String,
    version: u32,
    address: String,
    memory_kib: u32,
    iterations: u32,
    parallelism: u32,
    salt: String,
    nonce: String,
    ciphertext: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
struct Recovery {
    private_key: String,
    mnemonic: Option<String>,
}
impl Drop for Recovery {
    fn drop(&mut self) {
        self.private_key.zeroize();
        if let Some(mnemonic) = &mut self.mnemonic {
            mnemonic.zeroize();
        }
    }
}
impl Envelope {
    fn aad(&self) -> Vec<u8> {
        format!(
            "{}:{}:{}:argon2id:{}:{}:{}:{}:xchacha20poly1305:{}",
            self.format,
            self.version,
            self.address,
            self.memory_kib,
            self.iterations,
            self.parallelism,
            self.salt,
            self.nonce
        )
        .into_bytes()
    }
    fn decode(&self) -> Result<(Vec<u8>, Vec<u8>, Vec<u8>)> {
        ensure!(
            self.format == "zoru-vault" && self.version == 2,
            "Unsupported original wallet format"
        );
        ensure!(
            self.memory_kib == 65_536 && self.iterations == 3 && self.parallelism == 1,
            "Unexpected original wallet protection settings"
        );
        ensure!(
            self.address.starts_with("0x")
                && self.address.len() == 42
                && hex::decode(&self.address[2..])?.len() == 20,
            "Invalid original wallet identity"
        );
        let salt = hex::decode(&self.salt)?;
        let nonce = hex::decode(&self.nonce)?;
        let ciphertext = hex::decode(&self.ciphertext)?;
        ensure!(
            salt.len() == 16 && nonce.len() == 24,
            "Invalid encrypted wallet parameters"
        );
        ensure!(
            if self.version == 1 {
                ciphertext.len() == 48
            } else {
                (64..=4096).contains(&ciphertext.len())
            },
            "Invalid encrypted wallet payload"
        );
        Ok((salt, nonce, ciphertext))
    }
}

pub fn recognizes(value: &serde_json::Value) -> bool {
    value["format"] == "zoru-vault" && value["version"] == 2
}

/// Validate public metadata for wallet status without requiring its password.
pub fn address(path: &Path) -> Result<alloy_primitives::Address> {
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() > 0 && meta.len() <= 1_048_576,
        "Invalid encrypted wallet file"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let mut encrypted = Vec::new();
    file.take(1_048_577).read_to_end(&mut encrypted)?;
    ensure!(encrypted.len() <= 1_048_576, "Wallet file is too large");
    let envelope: Envelope = serde_json::from_slice(&encrypted)?;
    ensure!(envelope.version == 2, "Unsupported wallet format");
    envelope.decode()?;
    let address: alloy_primitives::Address = envelope.address.parse()?;
    Ok(address)
}

/// Upgrade only after the password and wallet address are verified. A byte-for-byte
/// encrypted archive remains available even if creation of the new keystore fails.
pub fn migrate(path: &Path, password: &str) -> Result<PrivateKeySigner> {
    let signer = unlock(path, password)?;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("Missing wallet directory"))?;
    let archive = parent.join("wallet-archives");
    fs::create_dir_all(&archive)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&archive, fs::Permissions::from_mode(0o700))?;
    }
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let backup = archive.join(format!("wallet-before-web3-{stamp}.json"));
    crate::storage::create_private_file(&backup, &fs::read(path)?)?;
    crate::wallet::store(path, &signer, password, true)?;
    Ok(signer)
}

pub fn unlock(path: &Path, password: &str) -> Result<PrivateKeySigner> {
    ensure!(
        !password.is_empty() && password.len() <= 1024,
        "Invalid wallet password"
    );
    let meta = fs::symlink_metadata(path)?;
    ensure!(
        meta.file_type().is_file() && meta.len() > 0 && meta.len() <= 1_048_576,
        "Invalid original encrypted wallet file"
    );
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW);
    }
    let file = options.open(path)?;
    let mut encrypted = Vec::new();
    file.take(1_048_577).read_to_end(&mut encrypted)?;
    ensure!(encrypted.len() <= 1_048_576, "Wallet file is too large");
    let envelope: Envelope = serde_json::from_slice(&encrypted)?;
    let (salt, nonce, ciphertext) = envelope.decode()?;
    let params = Params::new(
        envelope.memory_kib,
        envelope.iterations,
        envelope.parallelism,
        Some(32),
    )
    .map_err(|_| anyhow!("Invalid legacy key derivation"))?;
    let mut key = Zeroizing::new([0u8; 32]);
    Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
        .hash_password_into(password.as_bytes(), &salt, key.as_mut())
        .map_err(|_| anyhow!("Unable to unlock original wallet"))?;
    let cipher = XChaCha20Poly1305::new_from_slice(key.as_ref())
        .map_err(|_| anyhow!("Original wallet encryption error"))?;
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &ciphertext,
                    aad: &envelope.aad(),
                },
            )
            .map_err(|_| anyhow!("Incorrect password or damaged wallet"))?,
    );
    let mut raw = if envelope.version == 1 {
        ensure!(plain.len() == 32, "Invalid original wallet key");
        Zeroizing::new(plain.to_vec())
    } else {
        let record: Recovery = serde_json::from_slice(&plain)?;
        let private_key = record
            .private_key
            .strip_prefix("0x")
            .unwrap_or(&record.private_key);
        let decoded = hex::decode(private_key)?;
        ensure!(decoded.len() == 32, "Invalid original wallet key length");
        Zeroizing::new(decoded)
    };
    let signer =
        PrivateKeySigner::from_slice(&raw).map_err(|_| anyhow!("Invalid original wallet key"));
    raw.zeroize();
    let signer = signer?;
    ensure!(
        format!("{:#x}", signer.address()).eq_ignore_ascii_case(&envelope.address),
        "Original wallet address mismatch"
    );
    ensure!(
        crate::abi::keccak(signer.address().as_slice())
            != alloy_primitives::b256!(
                "35dee2f24e860bfaf8d62e8f853c801602a8bbea6f640b9a319eb5079bb22fea"
            ),
        "Reserved owner account is not a mining wallet"
    );
    Ok(signer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chacha20poly1305::aead::Aead;
    use rand::RngCore;
    use serde_json::json;
    use tempfile::tempdir;

    #[test]
    fn original_v154_vault_is_compatible_and_never_rewritten() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("mining-wallet.json");
        let mut secret = [0u8; 32];
        rand::thread_rng().fill_bytes(&mut secret);
        let expected = PrivateKeySigner::from_slice(&secret).unwrap();
        let mut salt = [0u8; 16];
        let mut nonce = [0u8; 24];
        rand::thread_rng().fill_bytes(&mut salt);
        rand::thread_rng().fill_bytes(&mut nonce);
        let pass = "Ephemeral-test-only-secret";
        let mut envelope = Envelope {
            format: "zoru-vault".into(),
            version: 2,
            address: format!("{:#x}", expected.address()),
            memory_kib: 65_536,
            iterations: 3,
            parallelism: 1,
            salt: hex::encode(salt),
            nonce: hex::encode(nonce),
            ciphertext: String::new(),
        };
        let mut key = Zeroizing::new([0u8; 32]);
        let params = Params::new(65_536, 3, 1, Some(32)).unwrap();
        Argon2::new(Algorithm::Argon2id, Version::V0x13, params)
            .hash_password_into(pass.as_bytes(), &salt, key.as_mut())
            .unwrap();
        let plaintext = Zeroizing::new(
            json!({
                "privateKey":hex::encode(secret),"mnemonic":null
            })
            .to_string(),
        );
        secret.zeroize();
        envelope.ciphertext = hex::encode(
            XChaCha20Poly1305::new_from_slice(key.as_ref())
                .unwrap()
                .encrypt(
                    XNonce::from_slice(&nonce),
                    Payload {
                        msg: plaintext.as_bytes(),
                        aad: &envelope.aad(),
                    },
                )
                .unwrap(),
        );
        let original = serde_json::to_vec(&json!({
            "format":envelope.format,"version":envelope.version,
            "address":envelope.address,"memoryKib":envelope.memory_kib,
            "iterations":envelope.iterations,"parallelism":envelope.parallelism,
            "salt":envelope.salt,"nonce":envelope.nonce,
            "ciphertext":envelope.ciphertext
        }))
        .unwrap();
        fs::write(&file, &original).unwrap();
        let got = unlock(&file, pass).unwrap();
        assert_eq!(got.address(), expected.address());
        assert_eq!(fs::read(&file).unwrap(), original);
        assert!(recognizes(
            &serde_json::from_slice::<serde_json::Value>(&original).unwrap()
        ));
        assert_eq!(address(&file).unwrap(), expected.address());
        let status = crate::wallet_ipc::execute(dir.path(), json!({"action":"status"})).unwrap();
        assert_eq!(status["address"], json!(expected.address()));
        assert_eq!(status["needsMigration"], true);
        assert!(
            crate::wallet_ipc::execute(
                dir.path(),
                json!({"action":"unlock","password":"wrong pass"})
            )
            .is_err()
        );
        assert_eq!(fs::read(&file).unwrap(), original);
        assert!(!dir.path().join("wallet-archives").exists());
        let upgraded =
            crate::wallet_ipc::execute(dir.path(), json!({"action":"unlock","password":pass}))
                .unwrap();
        assert_eq!(upgraded["address"], json!(expected.address()));
        assert_eq!(upgraded["migrated"], true);
        let archive = dir.path().join("wallet-archives");
        let backups: Vec<_> = fs::read_dir(archive)
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect();
        assert_eq!(backups.len(), 1);
        assert_eq!(fs::read(&backups[0]).unwrap(), original);
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&fs::read(&file).unwrap()).unwrap()["version"],
            3
        );
        assert_eq!(
            crate::wallet::unlock(&file, Zeroizing::new(pass.to_string()))
                .unwrap()
                .address(),
            expected.address()
        );
        let status_after =
            crate::wallet_ipc::execute(dir.path(), json!({"action":"status"})).unwrap();
        assert_eq!(status_after["needsMigration"], false);
        let second =
            crate::wallet_ipc::execute(dir.path(), json!({"action":"unlock","password":pass}))
                .unwrap();
        assert_eq!(second["migrated"], false);
        assert_eq!(fs::read(&backups[0]).unwrap(), original);
    }
}
