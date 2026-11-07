use alloy_primitives::{Address, B256, U256};
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use zeroize::Zeroizing;
mod dashboard;
use zoru_miner::{
    abi,
    pow::{self, Work},
    session::{Options, Run, Session},
    storage, stress, wallet,
};

#[derive(Parser)]
#[command(
    name = "zoru-miner-cli",
    version,
    about = "ZORU CPU miner for Vexanium EVM mainnet"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Args, Clone)]
struct Connection {
    /// Trusted deployment manifest with reviewed contract fingerprints.
    #[arg(long)]
    manifest: Option<PathBuf>,
    /// One EVM RPC endpoint for reads and signed transaction submission.
    #[arg(long, conflicts_with_all = ["read", "submit"])]
    rpc: Option<String>,
    /// Optional separate EVM read endpoint.
    #[arg(long, requires = "submit")]
    read: Option<String>,
    /// Optional separate EVM transaction endpoint.
    #[arg(long, requires = "read")]
    submit: Option<String>,
    /// Native Vexanium endpoint used to verify network identity and health.
    #[arg(long)]
    native: Option<String>,
    /// Encrypted Web3 version 3 wallet; omitted for read-only inspection.
    #[arg(long)]
    wallet: Option<PathBuf>,
    /// Directory for proof history and receipts; retain it between runs.
    #[arg(long)]
    state_dir: Option<PathBuf>,
    /// Read one password line from stdin instead of prompting.
    #[arg(long)]
    password_stdin: bool,
    /// Optional expected signer address, checked before journal recovery or signing.
    #[arg(long)]
    expected_wallet: Option<Address>,
    /// Maximum total gas fees for this session, in wei.
    #[arg(long)]
    session_budget_wei: Option<U256>,
    /// Public account for read-only balances and proof history, without a keystore.
    #[arg(long)]
    account: Option<Address>,
}
impl Connection {
    fn complete(&self) -> bool {
        self.manifest.is_some()
            && self.state_dir.is_some()
            && (self.rpc.is_some() || (self.read.is_some() && self.submit.is_some()))
    }
    fn options(&self) -> Options {
        Options {
            manifest: self.manifest.clone().expect("resolved manifest"),
            read: self
                .rpc
                .clone()
                .or_else(|| self.read.clone())
                .expect("validated EVM RPC"),
            submit: self
                .rpc
                .clone()
                .or_else(|| self.submit.clone())
                .expect("validated EVM RPC"),
            native: self.native.clone(),
            wallet: self.wallet.clone(),
            expected_wallet: self.expected_wallet,
            fee_budget: self.session_budget_wei,
            observer: self.account,
            sync_on_connect: true,
            state_dir: self.state_dir.clone().expect("resolved state directory"),
        }
    }
}
#[derive(Subcommand)]
enum Command {
    /// Save a trusted mainnet deployment, RPC and encrypted wallet path for future runs.
    Setup(Setup),
    /// Verify the network, contract suite and balances.
    Inspect(Connection),
    /// Show personal and global rewards, including pending and unclaimed ZORU.
    Rewards(Connection),
    /// Seal closed epochs and claim this wallet's eligible proof rewards.
    Claim(Connection),
    /// Run the shared desktop backend over the local JSON pipe.
    Serve(Connection),
    /// Search for valid proofs and submit them to the ledger.
    Mine {
        #[command(flatten)]
        connection: Connection,
        /// Hashing threads: 1, 2 or 4.
        #[arg(long)]
        workers: Option<usize>,
        /// Aggregate CPU budget in cores, greater than 0 and at most 2.
        #[arg(long)]
        cpu_budget: Option<f64>,
        /// Stop after this many seconds; 0 runs until interrupted.
        #[arg(long, default_value_t = 0)]
        seconds: u64,
        /// Stop after this many accepted proofs; 0 has no proof limit.
        #[arg(long, default_value_t = 0)]
        max_accepted: u64,
        /// Claim eligible rewards after this many accepted proofs; 0 disables it.
        #[arg(long)]
        auto_claim_after: Option<u64>,
        /// Emit JSON events instead of the interactive terminal dashboard.
        #[arg(long)]
        plain: bool,
        /// Use ASCII borders and status text in the terminal dashboard.
        #[arg(long)]
        ascii: bool,
        /// Run the real mining loop without signing or broadcasting transactions.
        #[arg(long, requires = "account")]
        dry_run: bool,
    },
    /// Create an encrypted wallet without overwriting an existing file.
    WalletCreate {
        #[arg(long)]
        wallet: PathBuf,
        #[arg(long)]
        password_stdin: bool,
    },
    /// Perform a local wallet operation through the private desktop pipe.
    WalletManage {
        #[arg(long)]
        directory: PathBuf,
    },
    /// Benchmark streamed virtual miners with real hashes and no transactions.
    Stress {
        #[arg(long)]
        clients: u64,
        #[arg(long, default_value_t = 1)]
        workers: usize,
        #[arg(long, default_value_t = 0.5)]
        cpu_budget: f64,
        #[arg(long, default_value_t = 180)]
        seconds: u64,
        #[arg(long)]
        output: Option<PathBuf>,
    },
    /// Calculate the full-domain Keccak digest for an explicit work context.
    Hash {
        #[arg(long)]
        chain: u64,
        #[arg(long)]
        ledger: Address,
        #[arg(long)]
        token: Address,
        #[arg(long)]
        generation: String,
        #[arg(long)]
        epoch: u64,
        #[arg(long)]
        seed: B256,
        #[arg(long)]
        beneficiary: Address,
        #[arg(long)]
        nonce: String,
    },
}

#[derive(Args)]
struct Setup {
    #[arg(long)]
    manifest: Option<PathBuf>,
    #[arg(long)]
    rpc: Option<String>,
    #[arg(long)]
    wallet: Option<PathBuf>,
    #[arg(long)]
    state_dir: Option<PathBuf>,
    #[arg(long)]
    workers: Option<usize>,
    #[arg(long)]
    cpu_budget: Option<f64>,
    /// Accepted unclaimed proofs before automatic settlement; 0 disables it.
    #[arg(long)]
    auto_claim_after: Option<u64>,
}
fn wire(value: Value) -> Value {
    match value {
        Value::Number(ref number) if number.as_u64().is_some_and(|v| v > 9_007_199_254_740_991) => {
            Value::String(number.to_string())
        }
        Value::Array(v) => Value::Array(v.into_iter().map(wire).collect()),
        Value::Object(v) => Value::Object(v.into_iter().map(|(k, v)| (k, wire(v))).collect()),
        other => other,
    }
}
fn emit(value: Value) {
    let stdout = io::stdout();
    let mut out = stdout.lock();
    let _ = serde_json::to_writer(&mut out, &wire(value));
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}
fn password(stdin: bool) -> Result<Zeroizing<String>> {
    if stdin {
        let mut text = Zeroizing::new(String::new());
        io::stdin().read_line(&mut text)?;
        while text.ends_with(['\n', '\r']) {
            text.pop();
        }
        Ok(text)
    } else {
        Ok(Zeroizing::new(rpassword::prompt_password(
            "Wallet password: ",
        )?))
    }
}
fn connect(
    connection: &Connection,
    password: Zeroizing<String>,
    sync_history: bool,
    observer: zoru_miner::Emit,
) -> Result<Arc<Session>> {
    let session = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut options = connection.options();
        options.sync_on_connect = sync_history;
        Session::connect(options, password, observer)
    }))
    .unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "Backend initialization failed; check the wallet and reconnect"
        ))
    })?;
    Ok(session)
}

fn serve(connection: Connection) -> Result<()> {
    ensure!(
        connection.rpc.is_some() || (connection.read.is_some() && connection.submit.is_some()),
        "Provide --rpc or both --read and --submit"
    );
    let holder: Arc<Mutex<Option<Arc<Session>>>> = Arc::new(Mutex::new(None));
    let busy = Arc::new(AtomicBool::new(false));
    let mut active: Option<thread::JoinHandle<()>> = None;
    for line in io::stdin().lock().lines() {
        let line = Zeroizing::new(line?);
        if line.len() > 16384 {
            emit(json!({"event":"error","error":"Command is too large"}));
            continue;
        }
        let mut command: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                emit(json!({"event":"error","error":"Invalid backend command"}));
                continue;
            }
        };
        let action = command["action"].as_str().unwrap_or_default().to_owned();
        if matches!(action.as_str(), "stop" | "pause" | "resume" | "quit") {
            if let Some(session) = holder.lock().unwrap().as_ref() {
                match action.as_str() {
                    "stop" | "quit" => session.stop.store(true, Ordering::Relaxed),
                    "pause" => session.pause.store(true, Ordering::Relaxed),
                    _ => session.pause.store(false, Ordering::Relaxed),
                }
            }
            emit(json!({"event":"control","action":action}));
            if action == "quit" {
                break;
            }
            continue;
        }
        if busy.swap(true, Ordering::SeqCst) {
            emit(
                json!({"event":"error","error":"Stop the current action before starting another"}),
            );
            continue;
        }
        if let Some(handle) = active.take() {
            let _ = handle.join();
        }
        if matches!(action.as_str(), "mine" | "claim" | "inspect")
            && let Some(session) = holder.lock().unwrap().as_ref()
        {
            session.begin_action();
        }
        let holder = holder.clone();
        let busy = busy.clone();
        let connection = connection.clone();
        active = Some(thread::spawn(move || {
            let result = (|| -> Result<()> {
                if action == "connect" {
                    *holder.lock().unwrap() = None;
                    let secret = if let Value::String(value) = command["password"].take() {
                        Zeroizing::new(value)
                    } else {
                        Zeroizing::new(String::new())
                    };
                    let session = connect(&connection, secret, false, Arc::new(emit))?;
                    session.inspect()?;
                    *holder.lock().unwrap() = Some(session);
                    return Ok(());
                }
                let session = holder
                    .lock()
                    .unwrap()
                    .clone()
                    .context("Connect to a verified deployment first")?;
                match action.as_str() {
                    "mine" => {
                        let result = session.run(Run {
                            workers: usize::try_from(
                                command["workers"]
                                    .as_u64()
                                    .unwrap_or(default_workers() as u64),
                            )?,
                            cpu_budget: command["cpuBudget"]
                                .as_f64()
                                .unwrap_or(default_cpu_budget()),
                            seconds: command["seconds"].as_u64().unwrap_or(0),
                            max_accepted: command["maxAccepted"].as_u64().unwrap_or(0),
                            auto_claim_after: 0,
                            dry_run: false,
                        })?;
                        ensure!(
                            result["runOutcome"] != "failed",
                            "Mining failed without an accepted proof"
                        );
                    }
                    "claim" => emit(json!({"event":"claims","data":session.settle()?})),
                    "inspect" => {
                        if session.account != Address::ZERO {
                            session.sync_history()?;
                        }
                        session.inspect()?;
                    }
                    _ => anyhow::bail!("Unknown backend action"),
                }
                Ok(())
            })();
            if let Err(error) = result {
                if let Some(session) = holder.lock().unwrap().as_ref() {
                    let _ = session.publish();
                }
                emit(
                    json!({"event":"error","error":error.to_string().chars().take(400).collect::<String>()}),
                );
            }
            busy.store(false, Ordering::SeqCst);
            emit(json!({"event":"command-complete","action":action}));
        }));
    }
    if let Some(session) = holder.lock().unwrap().as_ref() {
        session.stop.store(true, Ordering::Relaxed);
    }
    if let Some(handle) = active {
        let _ = handle.join();
    }
    Ok(())
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct SavedConnection {
    manifest: PathBuf,
    rpc: String,
    wallet: PathBuf,
    state_dir: PathBuf,
    #[serde(default = "default_workers")]
    workers: usize,
    #[serde(default = "default_cpu_budget")]
    cpu_budget: f64,
    #[serde(default = "default_auto_claim")]
    auto_claim_after: u64,
}
fn default_workers() -> usize {
    Run::default().workers
}
fn default_cpu_budget() -> f64 {
    Run::default().cpu_budget
}
fn default_auto_claim() -> u64 {
    Run::default().auto_claim_after
}

fn config_path() -> Result<PathBuf> {
    #[cfg(windows)]
    {
        return Ok(PathBuf::from(
            std::env::var_os("APPDATA").context("Cannot locate APPDATA for miner configuration")?,
        )
        .join("ZORU/config.json"));
    }
    #[cfg(not(windows))]
    {
        if let Some(base) = std::env::var_os("XDG_CONFIG_HOME").filter(|v| !v.is_empty()) {
            ensure!(
                Path::new(&base).is_absolute(),
                "XDG_CONFIG_HOME must be an absolute path"
            );
            return Ok(PathBuf::from(base).join("zoru-miner/config.json"));
        }
        let home = std::env::var_os("HOME")
            .context("Cannot locate home directory for miner configuration")?;
        Ok(PathBuf::from(home).join(".config/zoru-miner/config.json"))
    }
}

fn load_config(path: &Path) -> Result<Option<SavedConnection>> {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error).context("Cannot read saved miner configuration"),
    };
    let mut bytes = Vec::new();
    file.take(16385).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() <= 16384,
        "Saved miner configuration is too large"
    );
    let config: SavedConnection = serde_json::from_slice(&bytes)
        .context("Invalid saved configuration; review config.json or restore its backup")?;
    Ok(Some(config))
}

fn mining_options(
    workers: Option<usize>,
    cpu_budget: Option<f64>,
    auto_claim_after: Option<u64>,
    saved: Option<&SavedConnection>,
) -> Run {
    Run {
        workers: workers
            .or_else(|| saved.map(|s| s.workers))
            .unwrap_or(default_workers()),
        cpu_budget: cpu_budget
            .or_else(|| saved.map(|s| s.cpu_budget))
            .unwrap_or(default_cpu_budget()),
        auto_claim_after: auto_claim_after
            .or_else(|| saved.map(|s| s.auto_claim_after))
            .unwrap_or(default_auto_claim()),
        ..Run::default()
    }
}

fn save_config(path: &Path, config: &SavedConnection) -> Result<()> {
    ensure!(
        !path.exists(),
        "Configuration already exists; back it up before replacing"
    );
    ensure!(
        config.manifest.is_file(),
        "Deployment manifest does not exist"
    );
    ensure!(config.wallet.is_file(), "Encrypted wallet does not exist");
    zoru_miner::protocol::Manifest::load(&config.manifest)
        .context("Invalid deployment manifest")?;
    wallet::validate(&config.wallet).context("Invalid encrypted wallet")?;
    zoru_miner::rpc::Rpc::new(&config.rpc, std::time::Duration::ZERO)?;
    mining_options(None, None, None, Some(config)).validate()?;
    ensure!(
        !config.state_dir.exists() || config.state_dir.is_dir(),
        "State path must be a directory; existing data was preserved"
    );
    let parent = path.parent().context("Invalid configuration directory")?;
    fs::create_dir_all(parent)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))?;
    }
    let config = SavedConnection {
        manifest: fs::canonicalize(&config.manifest)?,
        rpc: config.rpc.clone(),
        wallet: fs::canonicalize(&config.wallet)?,
        state_dir: std::path::absolute(&config.state_dir)?,
        workers: config.workers,
        cpu_budget: config.cpu_budget,
        auto_claim_after: config.auto_claim_after,
    };
    storage::create_json(path, &config)?;
    Ok(())
}

fn resolve_with_config(
    mut connection: Connection,
    config: Option<&SavedConnection>,
) -> Result<Connection> {
    if let Some(config) = config {
        connection
            .manifest
            .get_or_insert_with(|| config.manifest.clone());
        connection
            .state_dir
            .get_or_insert_with(|| config.state_dir.clone());
        if connection.rpc.is_none() && connection.read.is_none() && connection.submit.is_none() {
            connection.rpc = Some(config.rpc.clone());
        }
        if connection.wallet.is_none() && connection.account.is_none() {
            connection.wallet = Some(config.wallet.clone());
        }
    }
    ensure!(
        connection.manifest.is_some() && connection.state_dir.is_some(),
        "Missing connection settings; run zoru-miner-cli setup"
    );
    ensure!(
        connection.rpc.is_some() || (connection.read.is_some() && connection.submit.is_some()),
        "Provide --rpc or both --read and --submit, or run zoru-miner-cli setup"
    );
    Ok(connection)
}

fn resolve_connection(connection: Connection) -> Result<Connection> {
    // The desktop passes a complete connection and retains its own UI preferences.
    if connection.complete() {
        return resolve_with_config(connection, None);
    }
    resolve_with_config(connection, load_config(&config_path()?)?.as_ref())
}

fn prompt(label: &str, default: Option<&str>) -> Result<String> {
    ensure!(
        io::stdin().is_terminal(),
        "Missing {label}; supply setup flags when input is redirected"
    );
    match default {
        Some(value) => eprint!("{label} [{value}]: "),
        None => eprint!("{label}: "),
    }
    io::stderr().flush()?;
    let mut value = String::new();
    ensure!(
        io::stdin().read_line(&mut value)? != 0,
        "Setup interrupted; no configuration was saved"
    );
    let value = value.trim();
    let value = if value.is_empty() {
        default.unwrap_or_default()
    } else {
        value
    };
    ensure!(
        !value.is_empty(),
        "{label} is required; no configuration was saved"
    );
    Ok(value.to_owned())
}

fn setup(args: Setup) -> Result<()> {
    let path = config_path()?;
    ensure!(
        !path.exists(),
        "Configuration already exists; back it up before replacing"
    );
    let interactive = io::stdin().is_terminal();
    let packaged = std::env::current_exe()?
        .parent()
        .and_then(Path::parent)
        .map(|p| p.join("share/zoru-miner/deployment.json"));
    let manifest = match args.manifest {
        Some(value) => value,
        None => PathBuf::from(prompt(
            "Trusted deployment manifest",
            packaged
                .as_ref()
                .filter(|p| p.is_file())
                .and_then(|p| p.to_str()),
        )?),
    };
    let wallet = match args.wallet {
        Some(value) => value,
        None => PathBuf::from(prompt("Existing encrypted wallet", None)?),
    };
    let rpc = match args.rpc {
        Some(value) => value,
        None if interactive => prompt("EVM RPC endpoint", Some("https://vexascan.com/rpc"))?,
        None => "https://vexascan.com/rpc".to_owned(),
    };
    let state_dir = match args.state_dir {
        Some(value) => value,
        None => {
            let default = storage::default_root()?.join("state");
            if interactive {
                PathBuf::from(prompt(
                    "State directory (existing journals are retained)",
                    default.to_str(),
                )?)
            } else {
                default
            }
        }
    };
    let workers = match args.workers {
        Some(value) => value,
        None if interactive => prompt("Workers (1, 2 or 4)", Some(&default_workers().to_string()))?
            .parse()
            .context("Invalid worker count")?,
        None => default_workers(),
    };
    let cpu_budget = match args.cpu_budget {
        Some(value) => value,
        None if interactive => prompt(
            "CPU budget in cores (greater than 0, at most 2)",
            Some(&default_cpu_budget().to_string()),
        )?
        .parse()
        .context("Invalid CPU budget")?,
        None => default_cpu_budget(),
    };
    let auto_claim_after = match args.auto_claim_after {
        Some(value) => value,
        None if interactive => {
            prompt("Auto-claim after unclaimed proofs (0 disables)", Some("50"))?
                .parse()
                .context("Invalid auto-claim threshold")?
        }
        None => default_auto_claim(),
    };
    let config = SavedConnection {
        manifest,
        rpc,
        wallet,
        state_dir,
        workers,
        cpu_budget,
        auto_claim_after,
    };
    mining_options(None, None, None, Some(&config)).validate()?;
    ensure!(
        config.wallet.is_file(),
        "Encrypted wallet not found; choose the existing encrypted wallet"
    );
    ensure!(
        config.manifest.is_file(),
        "Trusted deployment manifest not found"
    );
    wallet::validate(&config.wallet).context("Invalid encrypted wallet")?;
    let manifest = zoru_miner::protocol::Manifest::load(&config.manifest)
        .context("Invalid deployment manifest")?;
    let validation = tempfile::tempdir()?;
    let client = zoru_miner::protocol::Client::new(
        manifest,
        &config.rpc,
        &config.rpc,
        None,
        validation.path().join("journal"),
        validation.path().join("receipts"),
    )?;
    client
        .verify()
        .context("Setup connection verification failed; no configuration was saved")?;
    save_config(&path, &config)?;
    emit(json!({"event":"configured","path":path}));
    Ok(())
}

fn run() -> Result<()> {
    match Cli::parse().command {
        Command::Setup(args) => setup(args)?,
        Command::WalletManage { directory } => {
            let mut input = Zeroizing::new(String::new());
            io::stdin().take(16_385).read_to_string(&mut input)?;
            ensure!(input.len() <= 16_384, "Wallet command is too large");
            let result = serde_json::from_str(&input)
                .map_err(anyhow::Error::from)
                .and_then(|value| zoru_miner::wallet_ipc::execute(&directory, value));
            emit(match result {
                Ok(data) => json!({"ok":true,"data":data}),
                Err(error) => {
                    json!({"ok":false,"error":error.to_string().chars().take(400).collect::<String>()})
                }
            });
        }
        Command::WalletCreate {
            wallet: target,
            password_stdin,
        } => emit(
            json!({"event":"wallet-created","data":wallet::create(&target,password(password_stdin)?)?}),
        ),
        Command::Stress {
            clients,
            workers,
            cpu_budget,
            seconds,
            output,
        } => {
            let result = stress::run(clients, workers, cpu_budget, seconds)?;
            if let Some(path) = output {
                storage::atomic_json(&path, &result)?;
            }
            emit(result);
        }
        Command::Hash {
            chain,
            ledger,
            token,
            generation,
            epoch,
            seed,
            beneficiary,
            nonce,
        } => {
            ensure!(epoch < 1u64 << 48, "Epoch must fit uint48");
            let work = Work {
                chain_id: chain,
                ledger,
                token,
                generation: abi::quantity(&generation)?,
                epoch,
                seed,
                target: U256::MAX,
                end_time: 0,
            };
            println!(
                "{:#x}",
                pow::digest(&work, beneficiary, abi::quantity(&nonce)?)
            );
        }
        Command::Serve(connection) => serve(resolve_connection(connection)?)?,
        command => {
            let saved = match &command {
                Command::Inspect(c) | Command::Rewards(c) | Command::Claim(c) if c.complete() => {
                    None
                }
                _ => load_config(&config_path()?)?,
            };
            let (connection, mode, options, plain, ascii) = match command {
                Command::Inspect(c) => (c, "inspect", None, true, false),
                Command::Rewards(c) => (c, "rewards", None, true, false),
                Command::Claim(c) => (c, "claim", None, true, false),
                Command::Mine {
                    connection,
                    workers,
                    cpu_budget,
                    seconds,
                    max_accepted,
                    auto_claim_after,
                    plain,
                    ascii,
                    dry_run,
                } => (
                    connection,
                    "mine",
                    Some(Run {
                        seconds,
                        max_accepted,
                        dry_run,
                        ..mining_options(
                            workers,
                            cpu_budget,
                            if dry_run { Some(0) } else { auto_claim_after },
                            saved.as_ref(),
                        )
                    }),
                    plain,
                    ascii,
                ),
                _ => unreachable!(),
            };
            let mut connection = resolve_with_config(connection, saved.as_ref())?;
            if let Some(options) = &options {
                options.validate()?;
            }
            let interactive = dashboard::available(plain);
            let view = Arc::new(Mutex::new(dashboard::View::default()));
            let display = view.clone();
            let observer: zoru_miner::Emit = if interactive {
                Arc::new(move |value| display.lock().unwrap().receive(value))
            } else {
                Arc::new(emit)
            };
            let mut secret = if connection.wallet.is_some() {
                password(connection.password_stdin)?
            } else {
                Zeroizing::new(String::new())
            };
            // Read-only commands can authenticate a keystore's public address,
            // but must never open signing state or replay a transaction journal.
            if matches!(mode, "inspect" | "rewards")
                && connection.account.is_none()
                && let Some(path) = &connection.wallet
            {
                ensure!(
                    path.is_file(),
                    "Encrypted wallet not found; review setup or --wallet"
                );
                connection.account = Some(wallet::unlock(path, secret)?.address());
                connection.wallet = None;
                secret = Zeroizing::new(String::new());
            }
            let session = connect(&connection, secret, true, observer)?;
            session.begin_action();
            let stop = session.stop.clone();
            ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed))?;
            match mode {
                "inspect" | "rewards" => {
                    session.inspect()?;
                }
                "claim" => emit(json!({"event":"claims","data":session.settle()?})),
                _ => {
                    let result = if interactive {
                        let locale = std::env::var("LC_ALL")
                            .or_else(|_| std::env::var("LC_CTYPE"))
                            .or_else(|_| std::env::var("LANG"))
                            .unwrap_or_default();
                        let engine = session.clone();
                        let options = options.unwrap();
                        let mining = thread::spawn(move || engine.run(options));
                        let rendering = dashboard::run(
                            &session,
                            &mining,
                            view,
                            ascii || matches!(locale.as_str(), "C" | "POSIX"),
                        );
                        if rendering.is_err() {
                            session.stop.store(true, Ordering::SeqCst);
                        }
                        let result = mining.join().map_err(|_| {
                            anyhow::anyhow!("Mining thread failed; signed journals are retained")
                        })?;
                        rendering?;
                        result?
                    } else {
                        session.run(options.unwrap())?
                    };
                    if interactive {
                        emit(json!({"event":"mine-result","data":result}));
                    }
                    ensure!(
                        result["runOutcome"] != "failed",
                        "Mining failed without an accepted proof"
                    );
                }
            }
        }
    }
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        emit(
            json!({"event":"error","error":error.to_string().chars().take(400).collect::<String>()}),
        );
        std::process::exit(1)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture(directory: &Path) -> SavedConnection {
        use sha2::Digest;
        let mut manifest = json!({
            "schema":"zoru-mainnet-suite-1","chainId":6736,"mode":"local",
            "token":Address::repeat_byte(1),"ledger":Address::repeat_byte(2),
            "engine":Address::repeat_byte(3),"beacon":Address::repeat_byte(4),
            "admin":Address::repeat_byte(5),"owner":Address::repeat_byte(6),
            "launchTime":1,"epochSeconds":60,"decimals":8,"deploymentBlockNumber":"0x0",
            "drandChainHash":zoru_miner::protocol::DRAND_HASH,
            "runtimeCodeHashes":{},"approvedImplementationCodeHashes":{}
        });
        manifest["manifestSha256"] = json!(hex::encode(sha2::Sha256::digest(
            serde_json::to_vec(&manifest).unwrap()
        )));
        let config = SavedConnection {
            manifest: directory.join("deployment.json"),
            rpc: "https://vexascan.com/rpc".into(),
            wallet: directory.join("wallet.json"),
            state_dir: directory.join("state"),
            workers: 2,
            cpu_budget: 1.0,
            auto_claim_after: 0,
        };
        fs::write(&config.manifest, serde_json::to_vec(&manifest).unwrap()).unwrap();
        fs::write(
            &config.wallet,
            include_bytes!("../tests/fixtures/pbkdf2-v3.json"),
        )
        .unwrap();
        config
    }
    #[test]
    fn accepts_single_evm_rpc() {
        let cli = Cli::try_parse_from([
            "zoru-miner-cli",
            "mine",
            "--manifest",
            "deployment.json",
            "--rpc",
            "https://vexascan.com/rpc",
            "--state-dir",
            "state",
        ])
        .unwrap();
        let Command::Mine { connection, .. } = cli.command else {
            panic!("expected mine")
        };
        assert!(connection.native.is_none());
        assert_eq!(connection.options().read, "https://vexascan.com/rpc");
        assert_eq!(connection.options().submit, "https://vexascan.com/rpc");
    }

    #[test]
    fn rejects_ambiguous_rpc_flags() {
        assert!(
            Cli::try_parse_from([
                "zoru-miner-cli",
                "mine",
                "--manifest",
                "deployment.json",
                "--rpc",
                "https://vexascan.com/rpc",
                "--read",
                "http://127.0.0.1:18546",
                "--native",
                "https://vexascan.com:8443",
                "--state-dir",
                "state",
            ])
            .is_err()
        );
    }

    #[test]
    fn mine_without_flags_parses_and_requires_saved_setup() {
        let cli = Cli::try_parse_from(["zoru-miner-cli", "mine"]).unwrap();
        let Command::Mine { connection, .. } = cli.command else {
            panic!("expected mine")
        };
        assert!(connection.manifest.is_none());
        assert!(connection.state_dir.is_none());
        assert!(connection.rpc.is_none());
    }

    #[test]
    fn setup_rejects_missing_wallet_and_manifest() {
        let dir = tempfile::tempdir().unwrap();
        let config = SavedConnection {
            manifest: dir.path().join("missing-manifest.json"),
            rpc: "https://vexascan.com/rpc".into(),
            wallet: dir.path().join("missing-wallet.json"),
            state_dir: dir.path().join("state"),
            workers: default_workers(),
            cpu_budget: default_cpu_budget(),
            auto_claim_after: default_auto_claim(),
        };
        assert!(save_config(&dir.path().join("config.json"), &config).is_err());
        assert!(!dir.path().join("config.json").exists());
    }

    #[test]
    fn setup_persists_only_paths_and_rpc_and_refuses_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let config = fixture(dir.path());
        save_config(&path, &config).unwrap();
        let saved: SavedConnection = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.manifest, config.manifest);
        assert_eq!(saved.wallet, config.wallet);
        assert_eq!(saved.rpc, config.rpc);
        assert!(save_config(&path, &config).is_err());
        let content = fs::read_to_string(path).unwrap();
        assert!(!content.contains("password") && !content.contains("private"));
        assert_eq!(saved.workers, 2);
        assert_eq!(saved.cpu_budget, 1.0);
        assert_eq!(saved.auto_claim_after, 0);
    }

    #[test]
    fn cli_overrides_saved_preferences_then_uses_safe_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let saved = fixture(dir.path());
        let defaults = mining_options(None, None, None, None);
        assert_eq!(
            (
                defaults.workers,
                defaults.cpu_budget,
                defaults.auto_claim_after
            ),
            (1, 0.5, 50)
        );
        let preferences = mining_options(None, None, None, Some(&saved));
        assert_eq!(
            (
                preferences.workers,
                preferences.cpu_budget,
                preferences.auto_claim_after
            ),
            (2, 1.0, 0)
        );
        let overridden = mining_options(Some(4), Some(2.0), Some(10), Some(&saved));
        assert_eq!(
            (
                overridden.workers,
                overridden.cpu_budget,
                overridden.auto_claim_after
            ),
            (4, 2.0, 10)
        );
        let Command::Mine { connection, .. } =
            Cli::try_parse_from(["zoru-miner-cli", "mine", "--rpc", "https://example.com/rpc"])
                .unwrap()
                .command
        else {
            panic!()
        };
        let resolved = resolve_with_config(connection, Some(&saved)).unwrap();
        assert_eq!(resolved.options().read, "https://example.com/rpc");
        assert_eq!(resolved.wallet, Some(saved.wallet));
    }

    #[test]
    fn old_path_only_configuration_gets_defaults_and_invalid_configuration_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let saved = fixture(dir.path());
        let path = dir.path().join("config.json");
        fs::write(&path, json!({"manifest":saved.manifest,"wallet":saved.wallet,"rpc":saved.rpc,"state_dir":saved.state_dir}).to_string()).unwrap();
        assert_eq!(
            load_config(&path).unwrap().unwrap().workers,
            default_workers()
        );
        fs::write(&path, "{unfinished").unwrap();
        assert!(load_config(&path).is_err());
        let mut config = fixture(dir.path());
        config.cpu_budget = f64::NAN;
        assert!(save_config(&dir.path().join("new.json"), &config).is_err());
        config.cpu_budget = 1.0;
        fs::write(&config.manifest, "{}").unwrap();
        assert!(save_config(&dir.path().join("new.json"), &config).is_err());
        assert!(!dir.path().join("new.json").exists());
    }

    #[test]
    fn explicit_options_can_repair_invalid_saved_mining_values() {
        let dir = tempfile::tempdir().unwrap();
        let mut saved = fixture(dir.path());
        saved.workers = 8;
        saved.cpu_budget = 3.0;
        let path = dir.path().join("config.json");
        fs::write(&path, serde_json::to_vec(&saved).unwrap()).unwrap();
        let loaded = load_config(&path).unwrap().unwrap();
        assert!(
            mining_options(None, None, None, Some(&loaded))
                .validate()
                .is_err()
        );
        assert!(
            mining_options(Some(2), Some(1.0), None, Some(&loaded))
                .validate()
                .is_ok()
        );
    }

    #[test]
    fn exact_uint256_wire_format() {
        assert_eq!(
            wire(json!({"generation":u64::MAX}))["generation"],
            u64::MAX.to_string()
        );
    }
}

#[cfg(test)]
mod cli_address_tests {
    use alloy_primitives::Address;

    #[test]
    fn checksum_case_does_not_change_signer_address() {
        let mixed: Address = "0x238246f218a41363eAd2bB3c8e8bA6135705e202"
            .parse()
            .unwrap();
        let lower: Address = "0x238246f218a41363ead2bb3c8e8ba6135705e202"
            .parse()
            .unwrap();
        let wrong: Address = "0x0000000000000000000000000000000000000001"
            .parse()
            .unwrap();
        assert_eq!(mixed, lower);
        assert_ne!(mixed, wrong);
    }
}
