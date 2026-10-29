//! Narrow update verifier/installer. It never opens a wallet or submits transactions.
use clap::{Parser, Subcommand};
use serde_json::json;
#[cfg(target_os = "linux")]
use std::io::Write;
#[cfg(unix)]
use std::time::Duration;
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};
use zoru_miner::release::{self, Result};

#[derive(Parser)]
struct Args {
    #[command(subcommand)]
    command: Operation,
}
#[derive(Subcommand)]
enum Operation {
    Check {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        current_version: Option<String>,
    },
    Verify {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        current_version: Option<String>,
    },
    Install {
        #[arg(long)]
        metadata: PathBuf,
        #[arg(long)]
        package: PathBuf,
        #[arg(long)]
        parent: Option<u32>,
        #[arg(long)]
        application: Option<PathBuf>,
    },
    Restart {
        #[arg(long)]
        parent: u32,
    },
}
fn metadata_file(file: fs::File) -> Result<release::Release> {
    let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
    release::verify_metadata(&release::read_metadata(file)?, i64::try_from(now)?)
}
fn metadata(path: &Path) -> Result<release::Release> {
    metadata_file(open_input(path)?)
}
fn newer(r: &release::Release, current: &str) -> Result<bool> {
    let a = release::version(&r.version)?;
    let b = release::version(current)?;
    if a < b {
        return Err("The update version is older than the installed version".into());
    }
    Ok(a > b)
}
fn open_input(path: &Path) -> Result<fs::File> {
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK | libc::O_CLOEXEC);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err("Update package must be a regular file".into());
    }
    Ok(file)
}
fn package_identity(path: &Path, version: &str) -> Result<()> {
    #[cfg(target_os = "linux")]
    {
        let out = Command::new("/usr/bin/dpkg-deb")
            .args(["--field"])
            .arg(path)
            .args(["Package", "Version", "Architecture"])
            .output()?;
        if !out.status.success() || out.stdout.len() > 4096 {
            return Err("The update is not a valid application package".into());
        }
        let text = String::from_utf8(out.stdout)?;
        let fields: std::collections::BTreeMap<_, _> =
            text.lines().filter_map(|s| s.split_once(": ")).collect();
        if fields.get("Package") != Some(&"zoru-miner")
            || fields.get("Version") != Some(&version)
            || fields.get("Architecture") != Some(&"amd64")
        {
            return Err(
                "The package name, version or architecture does not match the update".into(),
            );
        }
    }
    #[cfg(not(target_os = "linux"))]
    {
        let _ = (path, version);
    }
    Ok(())
}
#[cfg(target_os = "linux")]
fn install(manifest: &Path, path: &Path) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    if unsafe { libc::geteuid() } != 0 {
        return Err("Installation requires system administrator authorization".into());
    }
    let installed = Command::new("/usr/bin/dpkg-query")
        .args(["--show", "--showformat=${Version}", "zoru-miner"])
        .output()?;
    if !installed.status.success() {
        return Err("Install ZORU Miner once before using in-app updates".into());
    }
    let caller = std::env::var("PKEXEC_UID")
        .ok()
        .map(|v| v.parse::<u32>())
        .transpose()?
        .unwrap_or(0);
    let manifest_file = open_input(manifest)?;
    let manifest_info = manifest_file.metadata()?;
    if manifest_info.uid() != caller || manifest_info.mode() & 0o022 != 0 {
        return Err("The update information has unsafe ownership or permissions".into());
    }
    let r = metadata_file(manifest_file)?;
    if !newer(&r, String::from_utf8(installed.stdout)?.trim())? {
        return Err("This version is already installed".into());
    }
    let a = release::artifact(&r)?;
    let mut input = open_input(path)?;
    let info = input.metadata()?;
    if info.uid() != caller || info.mode() & 0o022 != 0 {
        return Err("The update file has unsafe ownership or permissions".into());
    }
    let directory = Path::new("/var/cache/zoru-miner");
    if directory.exists() {
        let info = fs::symlink_metadata(directory)?;
        if !info.is_dir() || info.uid() != 0 || info.mode() & 0o022 != 0 {
            return Err("Unsafe installation cache directory".into());
        }
    } else {
        fs::create_dir(directory)?;
    }
    fs::set_permissions(directory, fs::Permissions::from_mode(0o700))?;
    let staging = tempfile::Builder::new()
        .prefix("update-")
        .tempdir_in(directory)?;
    let staged = staging.path().join(&a.file_name);
    let mut file = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)?;
    let count = std::io::copy(
        &mut std::io::Read::by_ref(&mut input).take(a.size + 1),
        &mut file,
    )?;
    if count != a.size {
        return Err("The downloaded package size changed".into());
    }
    file.flush()?;
    file.sync_all()?;
    drop(file);
    // Verify the root-owned copy, closing the user-file race before apt executes it.
    release::check_package(fs::File::open(&staged)?, a)?;
    package_identity(&staged, &r.version)?;
    let result = Command::new("/usr/bin/apt-get")
        .args([
            "--yes",
            "--no-remove",
            "-o",
            "DPkg::Options::=--force-confold",
            "install",
            "--",
        ])
        .arg(&staged)
        .env_clear()
        .env("PATH", "/usr/sbin:/usr/bin:/sbin:/bin")
        .env("DEBIAN_FRONTEND", "noninteractive")
        .env("LC_ALL", "C")
        .stdin(Stdio::null())
        .output()?;
    if !result.status.success() {
        return Err("The system package manager could not install the update".into());
    }
    Ok(())
}
#[cfg(windows)]
fn wait_for_parent(parent: u32) -> Result<()> {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_OBJECT_0},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    if parent == 0 || parent == std::process::id() {
        return Err("Invalid application process".into());
    }
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, parent);
        if handle.is_null() {
            return Ok(());
        }
        let result = WaitForSingleObject(handle, 30_000);
        CloseHandle(handle);
        if result != WAIT_OBJECT_0 {
            return Err("The application did not close".into());
        }
    }
    Ok(())
}
#[cfg(windows)]
fn install(manifest: &Path, path: &Path, parent: u32, application: &Path) -> Result<()> {
    use std::os::windows::process::CommandExt;
    let r = metadata(manifest)?;
    if !newer(&r, env!("CARGO_PKG_VERSION"))? {
        return Err("This version is already installed".into());
    }
    let artifact = release::artifact(&r)?;
    release::check_package(open_input(path)?, artifact)?;
    let application = application.canonicalize()?;
    if application.file_name().and_then(|name| name.to_str()) != Some("zoru-miner-gui.exe") {
        return Err("Invalid application path".into());
    }
    let destination = application
        .parent()
        .and_then(Path::parent)
        .ok_or("Application directory is missing")?;
    let directory = tempfile::tempdir()?;
    let staged = directory.path().join(&artifact.file_name);
    let mut input = open_input(path)?;
    let mut output = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&staged)?;
    let count = std::io::copy(
        &mut std::io::Read::by_ref(&mut input).take(artifact.size + 1),
        &mut output,
    )?;
    output.sync_all()?;
    drop(output);
    if count != artifact.size {
        return Err("Update package size changed".into());
    }
    release::check_package(open_input(&staged)?, artifact)?;
    wait_for_parent(parent)?;
    let status = Command::new(&staged)
        .arg("/S")
        .raw_arg(format!(
            "/D={}",
            destination
                .display()
                .to_string()
                .trim_start_matches(r"\\?\")
        ))
        .stdin(Stdio::null())
        .status()?;
    let restarted = Command::new(&application)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
    if !status.success() {
        return Err("The Windows installer did not finish".into());
    }
    restarted?;
    Ok(())
}
#[cfg(not(any(target_os = "linux", windows)))]
fn install(_manifest: &Path, _path: &Path) -> Result<()> {
    Err("This operating system is not supported".into())
}

fn restart(parent: u32) -> Result<()> {
    if parent == 0 || parent == std::process::id() {
        return Err("Invalid application process".into());
    }
    #[cfg(unix)]
    {
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while unsafe { libc::kill(i32::try_from(parent)?, 0) } == 0 {
            #[cfg(target_os = "linux")]
            if fs::read_to_string(format!("/proc/{parent}/stat"))
                .ok()
                .and_then(|s| {
                    s.rsplit_once(')')
                        .map(|(_, tail)| tail.trim_start().starts_with('Z'))
                })
                == Some(true)
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("The application did not close".into());
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        Command::new("/usr/bin/zoru-miner-gui")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()?;
        Ok(())
    }
    #[cfg(not(unix))]
    {
        let _ = parent;
        Err("Restart is not supported on this operating system yet".into())
    }
}
fn run() -> Result<serde_json::Value> {
    match Args::parse().command {
        Operation::Check {
            metadata: path,
            current_version,
        } => {
            let r = metadata(&path)?;
            let available = newer(
                &r,
                current_version
                    .as_deref()
                    .unwrap_or(env!("CARGO_PKG_VERSION")),
            )?;
            let a = r.artifacts.iter().find(|a| a.target == release::target());
            if available && a.is_none() {
                return Ok(
                    json!({"ok":true,"available":false,"status":"unsupported","version":r.version,"notes":r.notes}),
                );
            }
            Ok(
                json!({"ok":true,"available":available,"version":r.version,"notes":r.notes,"artifact":if available {a} else {None},"expiresAt":r.expires_at}),
            )
        }
        Operation::Verify {
            metadata: path,
            package,
            current_version,
        } => {
            let r = metadata(&path)?;
            if !newer(
                &r,
                current_version
                    .as_deref()
                    .unwrap_or(env!("CARGO_PKG_VERSION")),
            )? {
                return Err("This version is already installed".into());
            }
            release::check_package(open_input(&package)?, release::artifact(&r)?)?;
            package_identity(&package, &r.version)?;
            Ok(json!({"ok":true,"verified":true,"version":r.version}))
        }
        Operation::Install {
            metadata,
            package,
            parent,
            application,
        } => {
            #[cfg(windows)]
            install(
                &metadata,
                &package,
                parent.ok_or("Application process is missing")?,
                &application.ok_or("Application path is missing")?,
            )?;
            #[cfg(not(windows))]
            {
                let _ = (parent, application);
                install(&metadata, &package)?;
            }
            Ok(json!({"ok":true,"installed":true}))
        }
        Operation::Restart { parent } => {
            restart(parent)?;
            Ok(json!({"ok":true}))
        }
    }
}
fn main() {
    match run() {
        Ok(value) => println!("{value}"),
        Err(error) => {
            eprintln!("{error}");
            println!(
                "{}",
                json!({"ok":false,"message":"The update could not be verified or installed."})
            );
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn equal_versions_do_not_authorize_install_and_older_versions_are_rejected() {
        let r = release::Release {
            schema_version: 1,
            product: "zoru-miner".into(),
            channel: "stable".into(),
            version: "1.5.0".into(),
            published_at: String::new(),
            expires_at: String::new(),
            notes: String::new(),
            artifacts: vec![],
        };
        assert!(!newer(&r, "1.5.0").unwrap());
        assert!(newer(&r, "1.4.9").unwrap());
        assert!(newer(&r, "1.6.0").is_err());
    }
    #[cfg(unix)]
    #[test]
    fn package_symlinks_and_directories_are_rejected() {
        let d = tempfile::tempdir().unwrap();
        let target = d.path().join("package");
        fs::write(&target, b"abc").unwrap();
        let link = d.path().join("link");
        std::os::unix::fs::symlink(&target, &link).unwrap();
        assert!(open_input(&link).is_err());
        assert!(open_input(d.path()).is_err());
        assert!(open_input(&target).is_ok());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn debian_package_identity_must_match_the_signed_product_version_and_architecture() {
        let dir = tempfile::tempdir().unwrap();
        let tree = dir.path().join("tree");
        fs::create_dir_all(tree.join("DEBIAN")).unwrap();
        let package = dir.path().join("test.deb");
        for (name, version, architecture, accepted) in [
            ("zoru-miner", "9.0.0", "amd64", true),
            ("other", "9.0.0", "amd64", false),
            ("zoru-miner", "8.0.0", "amd64", false),
            ("zoru-miner", "9.0.0", "arm64", false),
        ] {
            fs::write(tree.join("DEBIAN/control"),format!("Package: {name}\nVersion: {version}\nArchitecture: {architecture}\nMaintainer: Test\nDescription: Update identity fixture\n")).unwrap();
            let built = Command::new("/usr/bin/dpkg-deb")
                .arg("--build")
                .arg(&tree)
                .arg(&package)
                .output()
                .unwrap();
            assert!(built.status.success());
            assert_eq!(package_identity(&package, "9.0.0").is_ok(), accepted);
        }
    }
}
