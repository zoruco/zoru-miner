#![cfg(target_os = "linux")]

use serde_json::{Value, json};
use std::{
    fs,
    os::unix::{fs::PermissionsExt, process::CommandExt},
    process::{Command, Stdio},
};
use zeroize::Zeroizing;
use zoru_miner::{storage, wallet, wallet_ipc};

// Run restrictions in a separate process so they cannot affect parallel tests.
// Denying link/linkat reproduces Android's policy even on a Linux filesystem
// that normally supports hard links.
fn restricted_child(name: &str, failures: &[(libc::c_long, u32)], size_limit: bool) -> bool {
    if std::env::var("ZORU_FS_TEST_CHILD").as_deref() == Ok(name) {
        return true;
    }
    let mut denied = vec![(libc::SYS_linkat, libc::EACCES as u32)];
    #[cfg(any(target_arch = "x86_64", target_arch = "x86", target_arch = "arm"))]
    denied.push((libc::SYS_link, libc::EACCES as u32));
    denied.extend_from_slice(failures);
    let mut filter = vec![libc::sock_filter {
        code: 0x20, // BPF_LD | BPF_W | BPF_ABS: seccomp_data.nr
        jt: 0,
        jf: 0,
        k: 0,
    }];
    for (syscall, errno) in denied {
        filter.push(libc::sock_filter {
            code: 0x15, // BPF_JMP | BPF_JEQ | BPF_K
            jt: 0,
            jf: 1,
            k: syscall as u32,
        });
        filter.push(libc::sock_filter {
            code: 0x06, // BPF_RET | BPF_K
            jt: 0,
            jf: 0,
            k: libc::SECCOMP_RET_ERRNO | errno,
        });
    }
    filter.push(libc::sock_filter {
        code: 0x06,
        jt: 0,
        jf: 0,
        k: libc::SECCOMP_RET_ALLOW,
    });
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args(["--exact", name, "--nocapture"])
        .env("ZORU_FS_TEST_CHILD", name);
    // SAFETY: Only async-signal-safe syscalls run after fork. The filter and
    // limit are already allocated, and remain valid until prctl copies them.
    unsafe {
        command.pre_exec(move || {
            let program = libc::sock_fprog {
                len: filter.len() as u16,
                filter: filter.as_ptr().cast_mut(),
            };
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0
                || libc::prctl(libc::PR_SET_SECCOMP, libc::SECCOMP_MODE_FILTER, &program) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            if size_limit {
                libc::signal(libc::SIGXFSZ, libc::SIG_IGN);
                let limit = libc::rlimit {
                    rlim_cur: 128,
                    rlim_max: 128,
                };
                if libc::setrlimit(libc::RLIMIT_FSIZE, &limit) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "restricted test failed: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    false
}

#[test]
fn wallets_and_setup_storage_work_when_hard_links_are_denied() {
    if !restricted_child(
        "wallets_and_setup_storage_work_when_hard_links_are_denied",
        &[],
        false,
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mining-wallet.json");
    let password = "disposable restricted filesystem password";
    let mut command = Command::new(env!("CARGO_BIN_EXE_zoru-miner-cli"));
    command
        .args(["wallet-create", "--wallet"])
        .arg(&path)
        .arg("--password-stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command.spawn().unwrap();
    std::io::Write::write_all(
        &mut child.stdin.take().unwrap(),
        format!("{password}\n").as_bytes(),
    )
    .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let event: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(event["event"], "wallet-created");
    let original = fs::read(&path).unwrap();
    let signer = wallet::unlock(&path, Zeroizing::new(password.into())).unwrap();
    assert_eq!(event["data"]["address"], format!("{:#x}", signer.address()));
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(wallet::create(&path, Zeroizing::new(password.into())).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::hard_link(&path, directory.path().join("denied-link"))
            .unwrap_err()
            .raw_os_error(),
        Some(libc::EACCES)
    );

    let backup = directory.path().join("backup/wallet.json");
    fs::create_dir(backup.parent().unwrap()).unwrap();
    wallet_ipc::execute(
        directory.path(),
        json!({"action":"backup","password":password,"file":backup}),
    )
    .unwrap();
    assert_eq!(
        wallet::unlock(&backup, Zeroizing::new(password.into()))
            .unwrap()
            .address(),
        signer.address()
    );
    wallet_ipc::execute(directory.path(), json!({"action":"password","password":password,"next":"rotated restricted filesystem password"})).unwrap();
    assert!(wallet::unlock(&path, Zeroizing::new(password.into())).is_err());
    assert_eq!(
        wallet::unlock(
            &path,
            Zeroizing::new("rotated restricted filesystem password".into())
        )
        .unwrap()
        .address(),
        signer.address()
    );
    let rotated = fs::read(&path).unwrap();
    wallet_ipc::execute(
        directory.path(),
        json!({"action":"archive","extra":"RESTORE WALLET"}),
    )
    .unwrap();
    assert!(!path.exists());
    let archived = fs::read_dir(directory.path().join("wallet-archives"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    assert_eq!(fs::read(&archived).unwrap(), rotated);
    assert_eq!(
        fs::metadata(archived).unwrap().permissions().mode() & 0o777,
        0o600
    );

    let config = directory.path().join("config.json");
    storage::create_json(&config, &json!({"wallet":backup})).unwrap();
    let saved = fs::read(&config).unwrap();
    assert!(storage::create_json(&config, &json!({"wallet":"other"})).is_err());
    assert_eq!(fs::read(&config).unwrap(), saved);
}

#[test]
fn wallet_sync_failure_removes_incomplete_file_and_preserves_archive_source() {
    if !restricted_child(
        "wallet_sync_failure_removes_incomplete_file_and_preserves_archive_source",
        &[(libc::SYS_fsync, libc::EIO as u32)],
        false,
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("mining-wallet.json");
    let error = wallet::create(
        &path,
        Zeroizing::new("disposable sync failure password".into()),
    )
    .unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::EIO)
    );
    assert!(!path.exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);

    let original = include_bytes!("fixtures/pbkdf2-v3.json");
    fs::write(&path, original).unwrap();
    let signer = wallet::unlock(&path, Zeroizing::new("public fixture password".into())).unwrap();
    assert!(wallet::store(&path, &signer, "disposable replacement password", true).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert!(
        wallet_ipc::execute(
            directory.path(),
            json!({"action":"archive","extra":"RESTORE WALLET"})
        )
        .is_err()
    );
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        fs::read_dir(directory.path().join("wallet-archives"))
            .unwrap()
            .count(),
        0
    );
}

#[test]
fn password_rotation_rename_failure_preserves_the_original_wallet() {
    let mut failures = vec![
        (libc::SYS_renameat, libc::EACCES as u32),
        (libc::SYS_renameat2, libc::EACCES as u32),
    ];
    #[cfg(any(target_arch = "x86_64", target_arch = "x86", target_arch = "arm"))]
    failures.push((libc::SYS_rename, libc::EACCES as u32));
    if !restricted_child(
        "password_rotation_rename_failure_preserves_the_original_wallet",
        &failures,
        false,
    ) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("wallet.json");
    let original = include_bytes!("fixtures/pbkdf2-v3.json");
    fs::write(&path, original).unwrap();
    let signer = wallet::unlock(&path, Zeroizing::new("public fixture password".into())).unwrap();
    assert!(wallet::store(&path, &signer, "disposable rotated password", true).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(
        wallet::unlock(&path, Zeroizing::new("public fixture password".into()))
            .unwrap()
            .address(),
        signer.address()
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}

#[test]
fn partial_write_failure_removes_only_the_new_file() {
    if !restricted_child("partial_write_failure_removes_only_the_new_file", &[], true) {
        return;
    }
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("config.json");
    let error = storage::create_json(&path, &"x".repeat(1024)).unwrap_err();
    assert_eq!(
        error
            .downcast_ref::<std::io::Error>()
            .unwrap()
            .raw_os_error(),
        Some(libc::EFBIG)
    );
    assert!(!path.exists());
    fs::write(&path, b"existing wallet").unwrap();
    assert!(storage::create_json(&path, &"x".repeat(1024)).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"existing wallet");
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
