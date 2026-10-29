use crate::{storage::WriterLock, wallet};
use alloy_primitives::Address;
use alloy_signer_local::{MnemonicBuilder, PrivateKeySigner};
use anyhow::{Context, Result, bail, ensure};
use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::Path,
    time::{SystemTime, UNIX_EPOCH},
};
use zeroize::Zeroizing;

fn take_secret(command: &mut Value, name: &str) -> Zeroizing<String> {
    Zeroizing::new(match command[name].take() {
        Value::String(secret) => secret,
        _ => String::new(),
    })
}

/// Private, bounded stdin/stdout channel used by the desktop wallet service.
pub fn execute(directory: &Path, mut command: Value) -> Result<Value> {
    fs::create_dir_all(directory)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    }
    let _lock = WriterLock::acquire(directory, "wallet")?;
    let path = directory.join("mining-wallet.json");
    let action = command["action"].as_str().unwrap_or_default().to_owned();
    let file = command["file"].as_str().unwrap_or_default().to_owned();
    let password = take_secret(&mut command, "password");
    let extra = take_secret(&mut command, "extra");
    let next = take_secret(&mut command, "next");
    ensure!(
        password.len() <= 1024 && next.len() <= 1024 && extra.len() <= 2048,
        "Wallet input is too long"
    );
    if action == "status" {
        if !path.exists() {
            return Ok(json!({"exists":false}));
        }
        let mut bytes = Vec::new();
        fs::File::open(&path)?
            .take(1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        ensure!(bytes.len() <= 1024 * 1024, "Wallet file is too large");
        let value: Value = serde_json::from_slice(&bytes).context("The wallet file is damaged")?;
        ensure!(value["version"] == 3, "Expected a Web3 version 3 wallet");
        let address = value["address"].as_str().unwrap_or_default();
        let address: Address = format!("0x{}", address.trim_start_matches("0x"))
            .parse()
            .context("Wallet address is missing")?;
        return Ok(json!({"exists":true,"address":address}));
    }
    if action == "archive" {
        ensure!(
            &*extra == "RESTORE WALLET",
            "Type RESTORE WALLET to continue"
        );
        let archive = directory.join("wallet-archives");
        fs::create_dir_all(&archive)?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
        let target = archive.join(format!("wallet-{stamp}.json"));
        fs::hard_link(&path, &target)?;
        fs::File::open(&target)?.sync_all()?;
        fs::remove_file(&path)?;
        return Ok(json!({"archived":true}));
    }
    if action == "create" {
        let signer = PrivateKeySigner::random();
        let mut result = wallet::store(&path, &signer, &password, false)?;
        let key = Zeroizing::new(signer.to_bytes().0);
        result["recoveryKind"] = json!("private_key");
        result["recoverySecret"] = json!(format!("0x{}", hex::encode(key.as_ref())));
        return Ok(result);
    }
    if action == "import" || action == "recovery" {
        ensure!(!path.exists(), "A wallet is already stored here");
        let signer = if action == "import" {
            wallet::unlock(Path::new(&file), password.clone())?
        } else if file == "private_key" {
            let key = Zeroizing::new(
                hex::decode(extra.trim().trim_start_matches("0x"))
                    .map_err(|_| anyhow::anyhow!("Enter a valid EVM private key"))?,
            );
            ensure!(key.len() == 32, "Enter a 32-byte EVM private key");
            PrivateKeySigner::from_slice(&key)
                .map_err(|_| anyhow::anyhow!("Invalid EVM private key"))?
        } else if file == "recovery_phrase" {
            MnemonicBuilder::english()
                .phrase(extra.as_str())
                .build()
                .map_err(|_| anyhow::anyhow!("Invalid recovery phrase"))?
        } else {
            bail!("Choose a supported recovery method");
        };
        let local_password = if action == "import" {
            next.as_str()
        } else {
            password.as_str()
        };
        wallet::store(&path, &signer, local_password, false)?;
        return Ok(json!({"address":signer.address(),"backupRequired":action!="import"}));
    }
    let signer = wallet::unlock(&path, password.clone())?;
    match action.as_str() {
        "unlock" | "authorize" | "acknowledge" => Ok(json!({"address":signer.address()})),
        "reveal" => {
            let key = Zeroizing::new(signer.to_bytes().0);
            Ok(json!({"kind":"private_key","secret":format!("0x{}",hex::encode(key.as_ref()))}))
        }
        "password" => wallet::store(&path, &signer, &next, true),
        "backup" => {
            let target = Path::new(&file);
            let parent = target.parent().context("Choose a backup file")?;
            ensure!(
                parent.canonicalize()? != directory.canonicalize()?,
                "Save the backup outside the application directory"
            );
            wallet::store(target, &signer, &password, false)
        }
        "remove" => {
            ensure!(&*extra == "REMOVE WALLET", "Type REMOVE WALLET to continue");
            fs::remove_file(&path)?;
            Ok(json!({"removed":true}))
        }
        _ => bail!("Choose a supported wallet action"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imported_wallet_uses_the_same_unlocker_and_password_rotation_preserves_identity() {
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("fixture.json");
        fs::write(&fixture, include_bytes!("../tests/fixtures/pbkdf2-v3.json")).unwrap();
        let input = json!({"action":"import","file":fixture,"password":"public fixture password","next":"new disposable password"});
        let imported = execute(root.path(), input).unwrap();
        let address = imported["address"].clone();
        assert_eq!(
            execute(root.path(), json!({"action":"status"})).unwrap()["address"],
            address
        );
        execute(root.path(), json!({"action":"password","password":"new disposable password","next":"rotated disposable password"})).unwrap();
        assert!(
            execute(
                root.path(),
                json!({"action":"unlock","password":"new disposable password"})
            )
            .is_err()
        );
        assert_eq!(
            execute(
                root.path(),
                json!({"action":"unlock","password":"rotated disposable password"})
            )
            .unwrap()["address"],
            address
        );
        assert!(
            execute(
                root.path(),
                json!({"action":"remove","password":"rotated disposable password","extra":"wrong"})
            )
            .is_err()
        );
        assert!(root.path().join("mining-wallet.json").is_file());
    }
}
