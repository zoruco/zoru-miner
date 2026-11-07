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

Use an encrypted Web3 version 3 wallet and the manifest included with the release. Keep the state directory between runs. For first-time CLI setup, run:

```sh
zoru-miner-cli setup --manifest ./share/zoru-miner/deployment.json \
  --rpc https://vexascan.com/rpc --wallet ./wallet.json --state-dir ./state
zoru-miner-cli mine
```

Setup stores only the manifest, RPC URL, encrypted wallet path and state path (not a password or private key). It refuses to overwrite an existing configuration. The miner prompts for the encrypted wallet password at startup. Explicit CLI flags can still override saved connection settings. GUI continues to use its own preferences and shared Rust engine.

```sh
zoru-miner-cli mine \
  --manifest ./share/zoru-miner/deployment.json \
  --rpc https://vexascan.com/rpc \
  --wallet ./wallet.json --state-dir ./state \
  --workers 2 --cpu-budget 1 --auto-claim-after 50
```

The CLI claims eligible rewards automatically once at least 50 accepted proofs remain unclaimed. Set `--auto-claim-after 0` to disable this, or choose another threshold. The threshold does not combine claims into one transaction: each proof still needs its own claim transaction. The desktop uses manual claims.

Use `rewards` with the same connection options for balances and rewards, or `claim` for a manual claim session. A public `--account <address>` allows read-only reward inspection without opening a wallet. `--session-budget-wei` optionally limits total fees for one CLI session; without it the CLI has no session allowance.

The release manifest can specify trusted RPC fallbacks. Each fallback must report the declared chain before use. A lost submission response is reconciled through its retained transaction hash and nonce before another transaction is considered. Preserve the state directory and journals across restarts. Both contract maintenance pauses stop mining and automatic settlement.

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
