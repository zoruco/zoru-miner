use ethers_signers::{coins_bip39::English, LocalWallet, MnemonicBuilder, Signer};
use rand::rngs::OsRng;
use serde_json::{json, Value};
use std::{
    ffi::{c_char, CStr, CString},
    fs,
    io::Write,
    panic::{catch_unwind, AssertUnwindSafe},
    str::FromStr,
    path::{Path, PathBuf},
    sync::Mutex,
};
use tempfile::{Builder, NamedTempFile};
use zeroize::Zeroize;

static WALLET_MUTEX: Mutex<()> = Mutex::new(());
const FILE_NAME: &str = "mining-wallet.json";
const FILE_MAX: u64 = 1_048_576;
fn ok(data: Value) -> Value { json!({"ok": true, "data": data}) }
fn error(msg: &str) -> Value { json!({"ok": false, "error": msg}) }
fn enc(v: Value) -> *mut c_char { CString::new(v.to_string()).unwrap().into_raw() }
fn value(p: *const c_char) -> Result<String, &'static str> {
    if p.is_null() { return Err("Missing information"); }
    unsafe { CStr::from_ptr(p) }.to_str().map(str::to_owned).map_err(|_| "Invalid text")
}
fn ffi(f: impl FnOnce() -> Value) -> *mut c_char {
    enc(catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| error("The wallet could not complete this action")))
}
fn root(name: &str) -> Result<PathBuf, &'static str> {
    let p = PathBuf::from(name);
    if !p.is_absolute() { return Err("Unable to access wallet storage"); }
    fs::create_dir_all(&p).map_err(|_| "Unable to access wallet storage")?;
    #[cfg(unix)] {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&p, fs::Permissions::from_mode(0o700)).map_err(|_| "Unable to protect wallet storage")?;
    }
    Ok(p)
}
fn keystore_path(root: &Path) -> PathBuf { root.join(FILE_NAME) }
fn valid_keystore(b: &[u8]) -> bool {
    let Ok(v) = serde_json::from_slice::<Value>(b) else { return false };
    if v.get("version").and_then(Value::as_u64) != Some(3) { return false; }
    let Some(c) = v.get("crypto").or_else(|| v.get("Crypto")) else { return false };
    for field in ["cipher", "ciphertext", "kdf", "mac"] {
        if c.get(field).and_then(Value::as_str).filter(|s| !s.is_empty()).is_none() { return false; }
    }
    let Some(p) = c.get("kdfparams") else { return false };
    if p.get("dklen").and_then(Value::as_u64) != Some(32) { return false; }
    match c.get("kdf").and_then(Value::as_str).unwrap_or("") {
        "scrypt" => {
            let n = p["n"].as_u64().unwrap_or(0);
            let r = p["r"].as_u64().unwrap_or(0);
            let threads = p["p"].as_u64().unwrap_or(0);
            n >= 1024 && n <= 262144 && n.is_power_of_two() && (1..=8).contains(&r)
                && (1..=8).contains(&threads) && r * threads <= 16
        }
        "pbkdf2" => (1000..=2_000_000).contains(&p["c"].as_u64().unwrap_or(0)),
        _ => false,
    }
}
fn load(path: &Path) -> Result<Vec<u8>, &'static str> {
    let m = fs::symlink_metadata(path).map_err(|_| "Encrypted wallet not found")?;
    if !m.file_type().is_file() || m.len() == 0 || m.len() > FILE_MAX { return Err("Invalid encrypted wallet file"); }
    let bytes = fs::read(path).map_err(|_| "Cannot read encrypted wallet")?;
    if !valid_keystore(&bytes) { return Err("Select an encrypted Web3 wallet JSON file"); }
    Ok(bytes)
}
fn save_new(root: &Path, contents: &[u8]) -> Result<(), &'static str> {
    if keystore_path(root).exists() { return Err("A wallet already exists on this device"); }
    let mut t = NamedTempFile::new_in(root).map_err(|_| "Cannot save wallet securely")?;
    t.write_all(contents).and_then(|_| t.as_file().sync_all()).map_err(|_| "Cannot save wallet securely")?;
    t.persist_noclobber(keystore_path(root)).map_err(|_| "A wallet already exists or could not be saved")?;
    Ok(())
}
fn address(w: &LocalWallet) -> String { format!("{:#x}",w.address()) }
fn create(dir: &str, pass: &str) -> Value {
    let _guard = match WALLET_MUTEX.lock() { Ok(g) => g, Err(_) => return error("Wallet is busy") };
    if !(12..=1024).contains(&pass.as_bytes().len()) { return error("Use a password of at least 12 characters"); }
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    if keystore_path(&root).exists() { return error("A wallet already exists on this device"); }
    let mut rng = OsRng;
    let wallet = LocalWallet::new(&mut rng);
    let output_address = address(&wallet);
    let mut secret = wallet.signer().to_bytes();
    let result = (|| {
        let tmp_dir = Builder::new().prefix(".zoru-").tempdir_in(&root).map_err(|_| "Cannot securely prepare wallet")?;
        LocalWallet::encrypt_keystore(tmp_dir.path(), &mut rng, &secret, pass.as_bytes(), Some("wallet.json"))
            .map_err(|_| "Could not encrypt wallet")?;
        let bytes = load(&tmp_dir.path().join("wallet.json"))?;
        save_new(&root,&bytes)
    })();
    secret.zeroize();
    match result { Ok(()) => ok(json!({"address": output_address})), Err(msg) => error(msg) }
}
// Reuse the same encryption and atomic no-overwrite policy for all imports.
fn store_signer(dir: &str, pass: &str, wallet: &LocalWallet) -> Value {
    if !(12..=1024).contains(&pass.as_bytes().len()) {
        return error("Choose a new password with at least 12 characters");
    }
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    if keystore_path(&root).exists() { return error("A wallet already exists on this device"); }
    let addr = address(wallet);
    let mut secret = wallet.signer().to_bytes();
    let mut rng = OsRng;
    let result = (|| {
        let temporary = Builder::new().prefix(".zoru-").tempdir_in(&root)
            .map_err(|_| "Cannot prepare wallet storage")?;
        LocalWallet::encrypt_keystore(
            temporary.path(), &mut rng, &secret, pass.as_bytes(), Some("wallet.json")
        ).map_err(|_| "Unable to encrypt imported wallet")?;
        let bytes = load(&temporary.path().join("wallet.json"))?;
        save_new(&root, &bytes)
    })();
    secret.zeroize();
    match result {
        Ok(()) => ok(json!({"address":addr})),
        Err(m) => error(m),
    }
}
fn import_recovery(dir: &str, method: &str, input: &str, pass: &str) -> Value {
    let _guard = match WALLET_MUTEX.lock() { Ok(g) => g, Err(_) => return error("Wallet is busy") };
    if input.len()>2048 || input.trim().is_empty() {
        return error("Enter a valid wallet recovery phrase or private key");
    }
    let signer = match method {
        "private_key" => {
            match LocalWallet::from_str(input.trim()) {
                Ok(w) => w,
                Err(_) => return error("Invalid EVM private key"),
            }
        }
        "recovery_phrase" => {
            let words: Vec<&str> = input.split_whitespace().collect();
            if ![12,15,18,21,24].contains(&words.len()) {
                return error("Enter a valid 12–24-word recovery phrase");
            }
            let phrase = words.join(" ");
            match MnemonicBuilder::<English>::default().phrase(phrase.as_str()).build() {
                Ok(w) => w,
                Err(_) => return error("Invalid recovery phrase"),
            }
        }
        _ => return error("Unsupported import method"),
    };
    store_signer(dir,pass,&signer)
}
// Only deletes this app's encrypted keystore. A pending signed transaction
// journal is never discarded by this operation.
fn remove_wallet(dir: &str, confirm: &str) -> Value {
    let _guard = match WALLET_MUTEX.lock() { Ok(g) => g, Err(_) => return error("Wallet is busy") };
    if confirm!="REMOVE WALLET" {return error("Removal not confirmed");}
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    let key = keystore_path(&root);
    if root.join("mining-journal.json").exists() {
        return error("Cannot remove wallet while a mining transaction is awaiting settlement");
    }
    if !key.exists() { return error("No wallet found on this device"); }
    // Even a damaged encrypted file must be removable through explicit user
    // confirmation. Never follow a symlink or delete another path.
    let metadata=match fs::symlink_metadata(&key) {
        Ok(m)=>m,Err(_)=>return error("No local wallet found"),
    };
    if !metadata.file_type().is_file() {
        return error("Invalid local wallet file");
    }
    match fs::remove_file(&key) {
        Ok(())=>ok(json!({"removed":true})),
        Err(_)=>error("Unable to remove local wallet"),
    }
}
fn import(dir: &str, path: &str, pass: &str) -> Value {
    let _guard = match WALLET_MUTEX.lock() { Ok(g) => g, Err(_) => return error("Wallet is busy") };
    if pass.is_empty() || pass.len()>1024 { return error("Enter your wallet password"); }
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    if keystore_path(&root).exists() { return error("A wallet already exists on this device"); }
    let source = PathBuf::from(path);
    let bytes = match load(&source) { Ok(x) => x, Err(m) => return error(m) };
    let wallet = match LocalWallet::decrypt_keystore(&source,pass.as_bytes()) {
        Ok(w) => w, Err(_) => return error("Incorrect password or invalid wallet")
    };
    let addr=address(&wallet);
    let mut normalized: Value = match serde_json::from_slice(&bytes) { Ok(v) => v, Err(_) => return error("Invalid wallet format") };
    normalized["address"] = json!(&addr[2..]);
    let bytes = normalized.to_string().into_bytes();
    match save_new(&root,&bytes) { Ok(()) => ok(json!({"address": addr})), Err(m) => error(m) }
}
fn unlock(dir: &str, pass: &str) -> Value {
    if pass.is_empty() || pass.len()>1024 { return error("Enter your wallet password"); }
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    let path = keystore_path(&root);
    if let Err(m) = load(&path) { return error(m); }
    match LocalWallet::decrypt_keystore(path,pass.as_bytes()) {
        Ok(w) => ok(json!({"address":address(&w)})),
        Err(_) => error("Incorrect password or invalid wallet")
    }
}
fn status(dir: &str) -> Value {
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    let path=keystore_path(&root);
    if !path.exists() { return ok(json!({"exists": false})); }
    match load(&path) {
        Ok(_) => ok(json!({"exists":true})),
        Err(m) => error(m)
    }
}
fn backup(dir: &str, destination: &str) -> Value {
    let root = match root(dir) { Ok(x) => x, Err(m) => return error(m) };
    let bytes = match load(&keystore_path(&root)) { Ok(x) => x, Err(m) => return error(m) };
    let dest = PathBuf::from(destination);
    if !dest.is_absolute() || dest == keystore_path(&root) { return error("Choose a different backup location"); }
    let Some(parent) = dest.parent().filter(|p|p.is_dir()) else { return error("Backup folder not found"); };
    let mut tmp = match NamedTempFile::new_in(parent) { Ok(x) => x, Err(_) => return error("Unable to save backup") };
    if tmp.write_all(&bytes).and_then(|_|tmp.as_file().sync_all()).is_err() { return error("Unable to save backup"); }
    match tmp.persist_noclobber(dest) { Ok(_) => ok(json!({"saved":true})), Err(_) => error("Backup already exists or cannot be saved") }
}
#[no_mangle] pub extern "C" fn zoru_wallet_status(dir:*const c_char)->*mut c_char {
    ffi(|| match value(dir) {Ok(d)=>status(&d),Err(m)=>error(m)})
}
#[no_mangle] pub extern "C" fn zoru_wallet_create(dir:*const c_char,p:*const c_char)->*mut c_char {
    ffi(|| match (value(dir),value(p)) {(Ok(d),Ok(p))=>create(&d,&p),_=>error("Invalid information")})
}
#[no_mangle] pub extern "C" fn zoru_wallet_import(dir:*const c_char,file:*const c_char,p:*const c_char)->*mut c_char {
    ffi(|| match (value(dir),value(file),value(p)) {(Ok(d),Ok(f),Ok(p))=>import(&d,&f,&p),_=>error("Invalid information")})
}
#[no_mangle] pub extern "C" fn zoru_wallet_unlock(dir:*const c_char,p:*const c_char)->*mut c_char {
    ffi(|| match (value(dir),value(p)) {(Ok(d),Ok(p))=>unlock(&d,&p),_=>error("Invalid information")})
}
#[no_mangle] pub extern "C" fn zoru_wallet_backup(dir:*const c_char,dest:*const c_char)->*mut c_char {
    ffi(|| match (value(dir),value(dest)) {(Ok(d),Ok(f))=>backup(&d,&f),_=>error("Invalid information")})
}
#[no_mangle] pub extern "C" fn zoru_wallet_import_recovery(dir:*const c_char,method:*const c_char,secret:*const c_char,pass:*const c_char)->*mut c_char {
    ffi(||match (value(dir),value(method),value(secret),value(pass)) {
        (Ok(d),Ok(m),Ok(s),Ok(p))=>import_recovery(&d,&m,&s,&p),_=>error("Invalid information")
    })
}
#[no_mangle] pub extern "C" fn zoru_wallet_remove(dir:*const c_char,confirm:*const c_char)->*mut c_char {
    ffi(||match (value(dir),value(confirm)) {
        (Ok(d),Ok(c))=>remove_wallet(&d,&c),_=>error("Invalid information")
    })
}
#[no_mangle] pub extern "C" fn zoru_wallet_string_free(ptr:*mut c_char) {
    if !ptr.is_null() { unsafe { drop(CString::from_raw(ptr)); } }
}
