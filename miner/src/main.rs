use alloy_primitives::{Address, B256, U256};
use anyhow::{Context, Result, ensure};
use clap::{Args, Parser, Subcommand};
use serde_json::{Value, json};
use std::{
    io::{self, BufRead, Read, Write},
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};
use zeroize::Zeroizing;
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
    manifest: PathBuf,
    /// HTTPS or loopback EVM endpoint for chain reads.
    #[arg(long)]
    read: String,
    /// HTTPS or loopback endpoint for signed transaction submission.
    #[arg(long)]
    submit: String,
    /// Native Vexanium endpoint used to verify network identity and health.
    #[arg(long)]
    native: String,
    /// Encrypted Web3 version 3 wallet; omitted for read-only inspection.
    #[arg(long)]
    wallet: Option<PathBuf>,
    /// Directory for proof history and receipts; retain it between runs.
    #[arg(long)]
    state_dir: PathBuf,
    /// Read one password line from stdin instead of prompting.
    #[arg(long)]
    password_stdin: bool,
    /// Optional expected signer address, checked natively before any signing.
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
    fn options(&self) -> Options {
        Options {
            manifest: self.manifest.clone(),
            read: self.read.clone(),
            submit: self.submit.clone(),
            native: self.native.clone(),
            wallet: self.wallet.clone(),
            expected_wallet: self.expected_wallet,
            fee_budget: self.session_budget_wei,
            observer: self.account,
            sync_on_connect: true,
            state_dir: self.state_dir.clone(),
        }
    }
}
#[derive(Subcommand)]
enum Command {
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
        #[arg(long, default_value_t = 1)]
        workers: usize,
        /// Aggregate CPU budget in cores, greater than 0 and at most 2.
        #[arg(long, default_value_t = 0.5)]
        cpu_budget: f64,
        /// Stop after this many seconds; 0 runs until interrupted.
        #[arg(long, default_value_t = 0)]
        seconds: u64,
        /// Stop after this many accepted proofs; 0 has no proof limit.
        #[arg(long, default_value_t = 0)]
        max_accepted: u64,
        /// Claim eligible rewards after this many accepted proofs; 0 disables it.
        #[arg(long, default_value_t = 50)]
        auto_claim_after: u64,
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
) -> Result<Arc<Session>> {
    let session = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let mut options = connection.options();
        options.sync_on_connect = sync_history;
        Session::connect(options, password, Arc::new(emit))
    }))
    .unwrap_or_else(|_| {
        Err(anyhow::anyhow!(
            "Backend initialization failed; check the wallet and reconnect"
        ))
    })?;
    Ok(session)
}

fn serve(connection: Connection) -> Result<()> {
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
        if matches!(action.as_str(), "mine" | "claim")
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
                    let session = connect(&connection, secret, false)?;
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
                            workers: command["workers"].as_u64().unwrap_or(1) as usize,
                            cpu_budget: command["cpuBudget"].as_f64().unwrap_or(0.5),
                            seconds: command["seconds"].as_u64().unwrap_or(0),
                            max_accepted: command["maxAccepted"].as_u64().unwrap_or(0),
                            auto_claim_after: 0,
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
fn run() -> Result<()> {
    match Cli::parse().command {
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
        Command::Serve(connection) => serve(connection)?,
        command => {
            let (connection, mode, options) = match command {
                Command::Inspect(c) => (c, "inspect", None),
                Command::Rewards(c) => (c, "rewards", None),
                Command::Claim(c) => (c, "claim", None),
                Command::Mine {
                    connection,
                    workers,
                    cpu_budget,
                    seconds,
                    max_accepted,
                    auto_claim_after,
                } => (
                    connection,
                    "mine",
                    Some(Run {
                        workers,
                        cpu_budget,
                        seconds,
                        max_accepted,
                        auto_claim_after,
                    }),
                ),
                _ => unreachable!(),
            };
            let secret = if connection.wallet.is_some() {
                password(connection.password_stdin)?
            } else {
                Zeroizing::new(String::new())
            };
            let session = connect(&connection, secret, true)?;
            session.begin_action();
            let stop = session.stop.clone();
            ctrlc::set_handler(move || stop.store(true, Ordering::Relaxed))?;
            match mode {
                "inspect" | "rewards" => {
                    session.inspect()?;
                }
                "claim" => emit(json!({"event":"claims","data":session.settle()?})),
                _ => {
                    let result = session.run(options.unwrap())?;
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
