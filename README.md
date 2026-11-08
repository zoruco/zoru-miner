# ZORU Miner

ZORU Miner provides a desktop app and a Rust command line miner for Vexanium EVM mainnet. Both use the same mining, wallet, transaction and reward code.

[ZORU project home](https://github.com/zoruco/zoruco) · [Protocol and contracts](https://github.com/zoruco/zoru-pow)

## Install

Download the Linux x86_64 package from [Releases](https://github.com/zoruco/zoru-miner/releases/latest).

- **Debian or Ubuntu:** install the `.deb` with `sudo apt install ./ZORU-Miner-<version>-Linux-amd64.deb`.
- **Linux portable:** extract the archive and open `bin/zoru-miner-gui`. Qt 6.2 or newer and its QML modules must be installed.

Compare the download with `SHA256SUMS.txt` attached to the same release. Each package includes the reviewed mainnet deployment manifest. A Windows package requires separate runtime validation before publication.

## Desktop

Create or import a wallet, save a separate recovery backup, and fund its EVM address with VEX for gas. Choose your CPU settings and fee allowance, then start from **Miner**.

**Proofs accepted** counts successful proof submissions. Rewards are claimed separately with **Claim Rewards**. The **Rewards** page shows your pending, unclaimed and claimed rewards alongside global contract totals.

The desktop asks for a new fee allowance for each session. It stops before signing a transaction whose maximum fee would exceed the remaining allowance. Transactions already sent can still confirm after stopping.

**Software Updates** checks official releases, verifies signed release information and verifies each downloaded installer. Installing ends the current mining session. It does not require your wallet password.

See the [App Guide](docs/APP_GUIDE.md) for setup, mining and updates.

## Command line

For native Android installation and wallet setup, see the [Termux guide](docs/TERMUX.md).

Use an existing encrypted Web3 version 3 wallet and the reviewed manifest included with the release. Keep your wallet backup and state directory between runs. Start with the guided setup:

```sh
zoru-miner-cli setup
zoru-miner-cli mine
```

Setup selects your existing wallet, trusted manifest, EVM RPC, state directory, workers, CPU budget and auto-claim threshold. It verifies the deployment before saving and refuses to replace an existing configuration. It never creates a wallet or replaces a transaction journal. For scripted setup, supply those choices as flags:

```sh
zoru-miner-cli setup \
  --manifest ./share/zoru-miner/deployment.json \
  --rpc https://api.windcrypto.com/rpc \
  --wallet ./wallet.json --state-dir ./state \
  --workers 2 --cpu-budget 1 --auto-claim-after 50
```

Configuration lives at `$XDG_CONFIG_HOME/zoru-miner/config.json` (normally `~/.config/zoru-miner/config.json`) on Linux, or `%APPDATA%\ZORU\config.json` on Windows. CLI overrides take precedence over saved settings, followed by safe defaults: one worker, 0.5 CPU core and auto-claim after 50 unclaimed proofs. For example, `zoru-miner-cli mine --workers 2 --cpu-budget 1` changes only that run. Back up the config before deliberately replacing it. The desktop keeps its existing preferences and uses the same Rust connection validation and mining engine.

The miner prompts for your wallet password at startup. `--password-stdin` reads one password line for controlled automation; supply it through a protected pipe or file descriptor. Configuration contains paths and preferences, never wallet passwords or private keys. A native RPC is optional; normal mining uses just `--rpc https://api.windcrypto.com/rpc`.

An interactive terminal shows a dark dashboard with fixed balances, contract and engine metrics above scrolling events. **Q** stops gracefully, **Up/Down** and **Page Up/Page Down** scroll, **L** focuses logs, and **R** redraws the cached view. Use `--ascii` for basic borders or `--plain` for JSON events. Redirected output, services and noninteractive terminals automatically use JSON. Unavailable or incompletely indexed rewards show as N/A; the snapshot age identifies cached data.

Hash workers continue while the single transaction writer submits proofs, tracks receipts, seals epochs and claims rewards. Candidates are bounded and checked against their epoch, seed, generation and target. Workers switch work at epoch changes and stop when work expires, admission closes, the proof limit is reached or RPC validation fails. Locally finding a candidate does not guarantee on-chain acceptance.

The shared engine reads one atomic work context and prepares authenticated beacons in the background. Eligible preceding-epoch work remains usable for up to 30 seconds while the next seed is pending. Its send guard starts at 15 seconds and adapts to 5–15 seconds after three successful proof transactions using measured full transaction duration. Mining can still wait for a new verified epoch or an expired work lease; the CLI and GUI show the reason.

The CLI claims eligible rewards automatically once at least 50 accepted proofs remain unclaimed. Set `--auto-claim-after 0` to disable this, or choose another threshold. The threshold does not combine claims into one transaction: each proof still needs its own claim transaction. Queued proofs receive priority between claims, with settlement given a turn after three accepted proofs or when the queue is empty. The desktop uses manual claims.

Use `rewards` with the same connection options for balances and rewards, or `claim` for a manual claim session. A public `--account <address>` allows read-only reward inspection without opening a wallet. `--session-budget-wei` optionally limits total fees for one CLI session; without it the CLI has no session allowance.

The release manifest can specify trusted RPC fallbacks. Each fallback must report the declared chain before use. A lost submission response is reconciled through its retained transaction hash and nonce before another transaction is considered. Preserve the state directory and signing journals (normally under `~/.zoru/signers`) across restarts. Admission pauses stop new proofs; claim pauses suspend settlement independently of hashing.

If setup reports a missing wallet or incompatible manifest, select the existing encrypted wallet and the matching reviewed manifest. A wrong-chain error requires correcting the endpoint, not bypassing validation. For an RPC outage or frozen head, wait for a healthy endpoint or restart with a verified `--rpc` override. If a nonce or receipt cannot be reconciled, preserve the journals and inspect the canonical transaction history before resuming; do not delete journals, replace the wallet, or repeatedly rebroadcast uncertain transactions. A native relay NET, RAM or CPU error requires a healthy relay endpoint; increasing local worker count cannot resolve it. The retained transaction remains uncertain until receipt and nonce reconciliation. `progress.json` and `summary.json` in the state directory contain measured queue, RPC, CPU, hashing and receipt results.

Mining and claiming spend VEX. Accepted proofs share an epoch budget, and mining does not guarantee a profit. Never put a private key or password in a command argument, log or issue.

## Build and contribute

Source builds require Rust 1.94.1, CMake 3.21 or newer, a C++17 compiler and Qt 6.2 or newer with Quick, Quick Controls, Network and Test modules. Use the reviewed manifest from the matching [contract deployment](https://github.com/zoruco/zoru-pow).

```sh
cargo build --release --locked --manifest-path miner/Cargo.toml
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release \
  -DZORU_DEPLOYMENT_MANIFEST=/path/to/mainnet.json
cmake --build build -j 2
./build/bin/zoru-miner-gui
```

Read [Contributing](CONTRIBUTING.md), [Testing](docs/TESTING.md), [Architecture](docs/ARCHITECTURE.md) and [Security](SECURITY.md) before changing mining or signing behavior. Build and packaging instructions are in [Building releases](docs/RELEASING.md).
