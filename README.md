# ZORU Miner — VEX EVM Mainnet

The ZORU Miner is a Rust proof-of-work application with a Qt desktop and command-line interface. Both use the same Rust engine for Keccak hashing, encrypted wallets, transaction submission, receipt verification, and reward history.

**Network:** Vexanium EVM mainnet, chain ID **6736**. ZORU mining begins at **29 October 2026, 15:13:01 UTC (22:13:01 WIB)**. Use the deployed contract addresses in the checksummed `deployment.json`.

## Install or build

Linux x86_64 requires glibc 2.34+ and Qt 6.2+. Source builds use Rust 1.94.1 and CMake 3.21+.

```sh
cargo build --release --locked --manifest-path miner/Cargo.toml
cmake -S . -B build -DCMAKE_BUILD_TYPE=Release
cmake --build build -j 1
```

The binaries are `build/zoru-miner-cli` and `build/zoru-miner-gui`. Use only an authentic release archive with a verified mainnet `deployment.json`.

## Mining and rewards

Set the RPC endpoints before running the CLI:

```sh
export EVM_RPC=https://api.windcrypto.com/rpc
export EVM_SUBMIT_RPC=https://api.windcrypto.com/rpc
export VEX_NATIVE_RPC=https://api.windcrypto.com
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
