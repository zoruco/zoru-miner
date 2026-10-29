# ZORU Miner

ZORU Miner provides a desktop app and a Rust command line miner for Vexanium EVM mainnet. Both use the same mining, wallet, transaction and reward code.

## Install

Download the package for your system from [Releases](https://github.com/zoruco/zoru-miner/releases/latest).

- **Debian or Ubuntu:** install the `.deb` with `sudo apt install ./ZORU-Miner-<version>-Linux-amd64.deb`.
- **Windows 10 or 11 x64:** run the Windows installer, or extract the portable ZIP and open `bin/zoru-miner-gui.exe`.
- **Linux portable:** extract the archive and open `bin/zoru-miner-gui`. Qt 6.2 or newer and its QML modules must be installed.

Compare the download with `SHA256SUMS` from the same release. Windows packages are not Authenticode signed. Each package includes the reviewed mainnet deployment manifest.

## Desktop

Create or import a wallet, save a separate recovery backup, and fund its EVM address with VEX for gas. Choose your CPU settings and fee allowance, then start from **Miner**.

**Proofs accepted** counts successful proof submissions. Rewards are claimed separately with **Claim Rewards**. The **Rewards** page shows your pending, unclaimed and claimed rewards alongside global contract totals.

The desktop asks for a new fee allowance for each session. It stops before signing a transaction whose maximum fee would exceed the remaining allowance. Transactions already sent can still confirm after stopping.

**Software Updates** checks official releases, verifies signed release information and verifies each downloaded installer. Installing ends the current mining session. It does not require your wallet password.

See the [App Guide](docs/APP_GUIDE.md) for setup, mining and updates.

## Command line

Use an encrypted Web3 version 3 wallet and the manifest included with the release. Keep the state directory between runs.

```sh
zoru-miner-cli mine \
  --manifest ./share/zoru-miner/deployment.json \
  --read https://vexascan.com/rpc \
  --submit https://vexascan.com/rpc \
  --native https://vexascan.com:8443 \
  --wallet ./wallet.json --state-dir ./state \
  --workers 2 --cpu-budget 1 --auto-claim-after 50
```

The CLI claims eligible rewards automatically once at least 50 accepted proofs remain unclaimed. Set `--auto-claim-after 0` to disable this, or choose another threshold. The threshold does not combine claims into one transaction: each proof still needs its own claim transaction. The desktop uses manual claims.

Use `rewards` with the same connection options for balances and rewards, or `claim` for a manual claim session. A public `--account <address>` allows read-only reward inspection without opening a wallet. `--session-budget-wei` optionally limits total fees for one CLI session; without it the CLI has no session allowance.

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

Read [Contributing](CONTRIBUTING.md), [Testing](docs/TESTING.md), [Architecture](docs/ARCHITECTURE.md) and [Security](SECURITY.md) before changing mining or signing behavior. Build and packaging instructions are in [Releases](docs/UPDATES.md).
