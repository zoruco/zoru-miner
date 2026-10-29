# ZORU Miner — VEX EVM Mainnet

The ZORU Miner is a Rust proof-of-work application with a Qt desktop and command-line interface. Both use the same Rust engine for Keccak hashing, encrypted wallets, transaction submission, receipt verification, and reward history.

**Network:** Vexanium EVM mainnet, chain ID **6736**. ZORU mining begins at **29 October 2026, 15:13:01 UTC (22:13:01 WIB)**. Use the deployed contract addresses in the checksummed `deployment.json`.

## Install or build

Linux x86_64 requires glibc 2.34+ and Qt 6.2+. Windows x64 packages use MinGW-w64 and Qt 6.2.4. Source builds use Rust 1.94.1 and CMake 3.21+.

### Install

Download the build for your operating system from [GitHub Releases](https://github.com/zoruco/zoru-miner/releases/tag/v0.1.0).

- **Debian/Ubuntu:** install the `.deb` using `sudo apt install ./zoru-miner-v0.1.0-linux-amd64.deb`.
- **Windows x64:** run `zoru-miner-v0.1.0-windows-x64-setup.exe`, or extract the portable `.zip`.
- **Linux portable:** extract the `.tar.gz`. The Qt GUI and Rust CLI are in `bin/`.

Verify the release SHA-256 checksums before installation. The Windows installer is not code-signed, so Windows SmartScreen may display a warning. The mainnet manifest is included with each package; you will need an encrypted keystore wallet to mine. Never share the wallet password.

The Windows x64 binaries and installer can be built on Linux without GitHub Actions. Install the Rust `x86_64-pc-windows-gnu` target, MinGW-w64, NSIS, the Qt 6.2.4 Windows MinGW libraries, and matching Linux Qt `moc`. Then run:

```sh
QT_WIN_ROOT=/path/to/Qt/6.2.4/mingw_64 \
ZORU_MAINNET_MANIFEST=/path/to/mainnet.json \
./packaging/windows-cross-build.sh
```

This creates a Windows portable ZIP and NSIS installer locally. The installer and CLI require Windows 10 or 11 x64; the release is not Authenticode-signed.

```sh
cargo build --release --locked --manifest-path miner/Cargo.toml
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j 1
```

The binaries are `build/zoru-miner-cli` and `build/zoru-miner-gui`. Use only an authentic release archive with a verified mainnet `deployment.json`.

## Original onboarding interface (v0.1.1 preview branch)

The original Qt Quick / QML desktop interface returns with the introductory
screen, Overview / Wallet / Settings navigation, create or import wallet,
encrypted backup, light/dark appearance and CPU worker controls. It uses the
current VEX mainnet Rust CLI (zoru-miner-cli serve) for verified
contract checks, mining and reward claims; the old testnet miner is not used.
Create or import a local wallet, unlock it, then fund a small amount of native
VEX for gas before enabling mining. This is a development preview, not yet a
published installer. The v0.1.0 packages remain the public release.

Linux GUI additionally requires Qt 6 QML / Quick / Quick Controls modules;
the .deb installs these as dependencies. The encrypted keystore and locally
persisted state stay in the user's app data directory. Wallet recovery secrets
must never be uploaded or shared.

## Mining and rewards

Set the RPC endpoints before running the CLI. For other VEX EVM endpoints, see [Chainlist](https://chainlist.org/chain/6736) and check the chain ID before connecting:

```sh
export EVM_RPC=https://vexascan.com/rpc
export EVM_SUBMIT_RPC=https://vexascan.com/rpc
export VEX_NATIVE_RPC=https://vexascan.com:8443
```

```sh
build/zoru-miner-cli rewards --manifest ./deployment.json \
  --read "$EVM_RPC" --submit "$EVM_SUBMIT_RPC" \
  --native "$VEX_NATIVE_RPC" --state-dir ./miner-state
```

Without a wallet, `rewards` reads global metrics only. With an encrypted wallet and its password, it also displays **My Pending**, **My Unclaimed**, and **My Claimed**. No transaction is broadcast by the `rewards` command.

```sh
build/zoru-miner-cli mine --manifest ./deployment.json \
  --read "$EVM_RPC" --submit "$EVM_SUBMIT_RPC" \
  --native "$VEX_NATIVE_RPC" --wallet ./wallet.json \
  --state-dir ./miner-state --workers 2 --cpu-budget 0.5 --seconds 180
```

`claim` seals eligible epochs and collects accepted proof rewards using the same connection flags and wallet. Mining and claiming spend native VEX gas; a valid proof is not guaranteed to earn more than the gas spent. Wallet keys and passwords must never be included in a log, shell history, or release archive.

The Qt desktop provides **My Mining Rewards**, **Global ZORU Network**, mining controls, balances, and claim history. Its connection fields use the same mainnet manifest as the CLI.

## Documents

[Architecture](docs/ARCHITECTURE.md) · [Rewards dashboard](docs/REWARDS_DASHBOARD.md) · [Testing](docs/TESTING.md) · [Security](SECURITY.md) · [Contributing](CONTRIBUTING.md).

The token's owner can replace the issuer and change the upgradeable mining modules. Read the protocol's public security model before using real funds.
