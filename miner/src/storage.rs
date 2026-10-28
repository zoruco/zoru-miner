use anyhow::{Result, ensure};
use fs2::FileExt;
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
};

pub fn atomic_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Missing state directory"))?;
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
