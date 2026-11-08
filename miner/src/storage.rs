use anyhow::{Context, Result, ensure};
use fs2::FileExt;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn create_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(value)?;
    bytes.push(b'\n');
    create_private_file(path, &bytes)
}

/// Create an encrypted wallet, archive or configuration without replacing any
/// existing directory entry (including a dangling symlink). O_EXCL is also the
/// arbiter between concurrent creators; an existence check cannot provide this.
pub(crate) fn create_private_file(path: &Path, bytes: &[u8]) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    let result = (|| -> std::io::Result<()> {
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            file.set_permissions(fs::Permissions::from_mode(0o600))?;
        }
        file.write_all(bytes)?;
        file.sync_all()?;
        #[cfg(unix)]
        File::open(parent)?.sync_all()?;
        Ok(())
    })();
    if let Err(error) = result {
        // Only the creator that successfully opened the file may clean it up.
        // A losing O_EXCL caller must never unlink the winning caller's wallet.
        drop(file);
        fs::remove_file(path)
            .with_context(|| format!("Cannot remove incomplete file after: {error}"))?;
        return Err(error.into());
    }
    Ok(())
}

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.as_file()
            .set_permissions(fs::Permissions::from_mode(0o600))?;
    }
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.as_file().sync_all()?;
    file.persist(path)?;
    #[cfg(unix)]
    File::open(parent)?.sync_all()?;
    Ok(())
}
pub struct WriterLock {
    _file: File,
}
impl WriterLock {
    pub fn acquire(directory: &Path, account: &str) -> Result<Self> {
        fs::create_dir_all(directory)?;
        let path = directory.join(format!("{}.lock", account.to_lowercase()));
        let mut options = OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(path)?;
        ensure!(
            file.try_lock_exclusive().is_ok(),
            "This account already has a signing session"
        );
        Ok(Self { _file: file })
    }
}
pub fn default_root() -> Result<PathBuf> {
    let root = std::env::var_os("ZORU_STATE_ROOT")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|v| PathBuf::from(v).join(".zoru")))
        .or_else(|| std::env::var_os("LOCALAPPDATA").map(|v| PathBuf::from(v).join("ZORU")))
        .ok_or_else(|| anyhow::anyhow!("Choose a state directory"))?;
    fs::create_dir_all(&root)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
    }
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn signing_lock_is_exclusive_and_released() {
        let d = tempfile::tempdir().unwrap();
        let first = WriterLock::acquire(d.path(), "0xabc").unwrap();
        assert!(WriterLock::acquire(d.path(), "0xABC").is_err());
        drop(first);
        assert!(WriterLock::acquire(d.path(), "0xabc").is_ok());
    }
    #[test]
    fn exclusive_create_has_one_winner_and_never_removes_existing_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("config.json");
        let barrier = std::sync::Barrier::new(8);
        let results = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..8)
                .map(|n| {
                    let path = &path;
                    let barrier = &barrier;
                    scope.spawn(move || {
                        barrier.wait();
                        create_json(path, &serde_json::json!({"winner":n})).map(|()| n)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|handle| handle.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let winner = results.into_iter().find_map(Result::ok).unwrap();
        let bytes = fs::read(&path).unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["winner"],
            winner
        );
        assert!(create_private_file(&path, b"replacement").is_err());
        assert_eq!(fs::read(&path).unwrap(), bytes);
        assert!(create_private_file(directory.path(), b"replacement").is_err());
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
    }
    #[test]
    fn atomic_write_preserves_complete_document() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("record.json");
        for n in 0..10 {
            atomic_json(&p, &serde_json::json!({"n":n})).unwrap();
            assert_eq!(
                serde_json::from_slice::<serde_json::Value>(&fs::read(&p).unwrap()).unwrap()["n"],
                n
            );
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(fs::metadata(p).unwrap().permissions().mode() & 0o777, 0o600);
        }
    }
}
