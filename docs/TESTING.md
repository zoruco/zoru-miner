# Mainnet release validation

Use the release manifest and matching binaries. An isolated Anvil instance is suitable for deterministic regression testing, but it does not prove live mainnet transaction success.

## Rust build and automated tests

```sh
cargo fmt --check --manifest-path miner/Cargo.toml
cargo clippy --locked --manifest-path miner/Cargo.toml --all-targets -- -D warnings
cargo test --locked --manifest-path miner/Cargo.toml

cmake -S . -B build -DCMAKE_BUILD_TYPE=Release -DZORU_DEPLOYMENT_MANIFEST=/path/to/mainnet.json
cmake --build build -j 1
QT_QPA_PLATFORM=offscreen ctest --test-dir build --output-on-failure
```

## Local integration fixtures

Use an isolated Anvil chain with ID **6736**, temporary wallets and disposable funds. The integration test harness reads its own `ZORU_TEST_OPTIONS` configuration. Never use production wallet credentials.

The mining-limit fixture needs blocks to keep advancing during hashing. Start Anvil with `--block-time 1`, or set `evm_setIntervalMining` to one second. With transaction-only mining, the latest block timestamp can lag the pending transaction timestamp and make otherwise valid work expire during gas estimation. Run the signed fixtures sequentially and give each test its own `ZORU_STATE_ROOT`.

```sh
cargo test --release --locked --manifest-path miner/Cargo.toml --test recovery -- --ignored --test-threads=1
cargo test --release --locked --manifest-path miner/Cargo.toml --test mining_limit -- --ignored --test-threads=1
cargo test --release --locked --manifest-path miner/Cargo.toml --test claim_reorg -- --ignored --test-threads=1
cargo test --release --locked --manifest-path miner/Cargo.toml --test automatic_claim -- --ignored --test-threads=1
```

These tests cover nonce recovery, duplicate-proof handling, receipt verification, accepted work, claim accounting, and reorganization behavior. Keep separate state directories and signing journals for every independent chain.

## Read-only network checks

Test CLI and GUI connectivity against the confirmed mainnet deployment on chain 6736. Check bytecode, owner, issuer, and reward readings before signed mining tests.

The CLI includes a bounded transaction-free stress command. It measures local hashing throughput, not real network transactions or the number of independent miner devices.

## Packaging

Use `packaging/bundle.py` with the final verified mainnet deployment manifest, matching binaries, and expected token and Ledger proxy addresses. Run `python3 packaging/test_bundle.py -v`. The resulting archive includes checksums and dependencies' licenses.

Before publication, verify the installation and signed mining/claim behavior on the intended mainnet and retain receipts. A skipped environment-dependent test remains unverified rather than passing.
