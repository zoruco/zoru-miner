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
cargo test --release --locked --manifest-path miner/Cargo.toml --test continuous -- --ignored --test-threads=1
```

These tests cover nonce recovery, duplicate-proof handling, receipt verification, accepted work, claim accounting, and reorganization behavior. Keep separate state directories and signing journals for every independent chain.

The complete fixture runner builds the reviewed contract sources into a temporary artifact directory, starts its own loopback Anvil on chain 6736 and creates a disposable encrypted wallet. It uses the real Ledger, Keccak engine and coordinator with an explicitly isolated deterministic beacon. It never changes the protocol source checkout or connects to a public chain for writes:

```sh
python3 tests/support/isolated.py --contracts /path/to/zoru-pow \
  --output /tmp/zoru-isolated-validation --terminal --gui-build ./build
```

Foundry and the matching Solidity compiler must be available. The runner compiles tests with one build job and waits for CPU headroom before signed mining checks. Production CPU and memory guards remain enabled; a resource pause does not establish continuous-hashing success. The output path must not already exist. Reports contain local receipts, measured hashes, queue depths, RPC counts and CPU time. The private fixture options and encrypted wallet remain inside the permission-restricted output directory. Do not publish its credentials. `--baseline` runs a 20-second transaction-free measurement using two workers and one CPU core.

The continuous fixture delays proof and claim receipts, rolls the epoch while a receipt is outstanding, checks that hashes keep increasing, and interrupts then restores RPC service. It also freezes the EVM head until the work lease expires and checks recovery when blocks advance again. A real coordinator upgrade checks generation changes and resumption on new work. Phase reports preserve the actual snapshots and receipts when a continuous-mining assertion fails. A slow inspection response also forces history updates after the snapshot block was pinned, checking that personal totals remain unavailable rather than mixing proof heights. Each phase waits for CPU headroom; bounded recovery waits require measured hash progress rather than accepting a status label or treating resource pauses as success. A read-only rewards command is also checked while the signing wallet is locked; it must leave the nonce unchanged. `--terminal` tests guided/interrupted setup, saved settings, overrides, non-TTY logs, terminal resizing, scrolling and restoration after Q, SIGINT and SIGTERM. Long desktop integration runs can use `QTEST_FUNCTION_TIMEOUT=1200000` to allow resource pauses without weakening individual assertions. The GUI fixture stages its own binaries and manifest, mines and claims with the shared engine, exercises pause/resume/stop and verifies preservation of the encrypted wallet. No existing deployment manifest or user profile is replaced.

## Read-only network checks

Test CLI and GUI connectivity against the confirmed mainnet deployment on chain 6736. Check bytecode, owner, issuer, and reward readings before signed mining tests.

The CLI includes a bounded transaction-free stress command. It measures local hashing throughput, not real network transactions or the number of independent miner devices.

## Packaging

Use `packaging/bundle.py` with the final verified mainnet deployment manifest, matching binaries, and expected token and Ledger proxy addresses. Run `python3 packaging/test_bundle.py -v`. The resulting archive includes checksums and dependencies' licenses.

Before publication, verify the installation and signed mining/claim behavior on the intended mainnet and retain receipts. A skipped environment-dependent test remains unverified rather than passing.

## Platform coverage

The current release is validated for Linux x86_64. Windows requires its own runtime checks before publication. Earlier cross-build results and their limitations are preserved in [the release history](RELEASE-1.5.4.md).

### Android / Termux wallet regression

Android can reject `link` and `linkat` even in a writable private directory. Wallet creation, wallet archives and pre-migration backups therefore use exclusive file creation, with mode `0600` set before writing encrypted bytes. The shared storage routine uses `create_new(true)` to reject existing files, directories and symlinks and to choose one winner between concurrent creators. It syncs the file and parent directory before reporting success and removes its new file on a write or sync error. Password rotation retains the existing temporary-file and atomic-rename path.

The audit also covers `setup`: configuration creation must not use `persist_noclobber`, whose fallback can require a hard link. `inspect` and `mine` share the existing Web3 V3 unlocker; signing locks, journals, proof hashing and transaction logic are unchanged. New-file creation writes through the exclusive file handle, so readers must wait for creation to succeed before opening it. An abruptly terminated creator can leave an incomplete file, which a subsequent create must refuse to overwrite.

On Linux, the filesystem regression tests deny hard-link syscalls with `EACCES`, inject `fsync` and rename errors, and impose a file-size limit to exercise partial-write cleanup. Restrictions apply only to disposable test subprocesses. All wallets and passwords are temporary test data; these tests need no deployment or public RPC:

```sh
cargo test --locked --manifest-path miner/Cargo.toml --test wallet_filesystem
cargo test --locked --manifest-path miner/Cargo.toml --lib wallet
cargo test --locked --manifest-path miner/Cargo.toml --bin zoru-miner-cli
```

An ARM64 Android cross build checks compilation, not execution in Termux. Before publishing an Android release, run the wallet and CLI unit tests natively and verify `wallet-create`, `setup`, `inspect` and `mine` with disposable credentials in the intended Android environment. Keep wallets and configuration in Termux's private storage, where Unix permissions are supported.
