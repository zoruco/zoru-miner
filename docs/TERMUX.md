# Termux CLI

Build the CLI natively in Termux's private storage. The Linux x86_64 release
binary cannot run on an ARM64 Android device. Install Rust 1.94.1 or newer:

```sh
pkg install git rust clang make pkg-config
git clone https://github.com/zoruco/zoru-miner.git
cd zoru-miner
cargo build --release --locked --manifest-path miner/Cargo.toml --bin zoru-miner-cli
install -m 755 miner/target/release/zoru-miner-cli "$PREFIX/bin/zoru-miner-cli"
```

Obtain the reviewed `deployment.json` from the matching official release and
copy it to `~/.local/share/zoru-miner/deployment.json`. Verify its provenance
before setup. Create one wallet and retain its encrypted file and a separate
backup:

```sh
mkdir -p "$HOME/.local/share/zoru-miner"
chmod 700 "$HOME/.local/share/zoru-miner"
zoru-miner-cli wallet-create \
  --wallet "$HOME/.local/share/zoru-miner/mining.wallet.json"
```

Enter a password of at least 12 characters at the hidden prompt. The command
prints the public EVM address. Send VEX to that address on Vexanium EVM
(chain ID 6736) for transaction gas. Keep the wallet and password out of Git.

Save the connection once:

```sh
zoru-miner-cli setup \
  --manifest "$HOME/.local/share/zoru-miner/deployment.json" \
  --rpc https://vexascan.com/rpc \
  --wallet "$HOME/.local/share/zoru-miner/mining.wallet.json" \
  --state-dir "$HOME/.local/share/zoru-miner/state" \
  --workers 1 --cpu-budget 0.5 --auto-claim-after 50
```

Subsequent runs need just:

```sh
zoru-miner-cli mine
```

Enter the wallet password when prompted. Initial contract verification and
history indexing can take time before the dashboard appears. Press **Q** to
stop gracefully. Keep the state directory and `~/.zoru/signers` between runs.
Run the signing wallet on one device only.

`Insufficient bounded gas balance` means the account cannot cover the
transaction's maximum gas fee. Check its VEX balance before restarting.
`waiting-beacon-or-recovery` means the engine is waiting for usable work;
the next verified work context resumes hashing automatically. An accepted
proof is confirmed on-chain; its reward is settled separately. The default
auto-claim threshold is 50 accepted, unclaimed proofs.

For logs instead of the dashboard use `zoru-miner-cli mine --plain`. For a
transaction-free check use `zoru-miner-cli mine --dry-run --seconds 30`.
Android may suspend Termux in the background; a foreground terminal and a
Termux wake lock can help, subject to the device's battery restrictions.

The dashboard uses colored status badges, compact balance cards and a
dedicated hashrate panel with the current rate, measured peak and a two-row
history graph. New samples arrive at the right edge. Phone screens use two columns;
wider terminals show network and engine details side by side. `--ascii`
uses ASCII glyphs for borders and the trend graph. Uptime appears once in
the session panel. The main activity feed focuses on epoch changes,
transactions, warnings and confirmed proofs. On tall screens the main feed
stays compact. Press **L** to expand it and show candidate details.
Long messages wrap to keep RPC reasons readable; identical warnings
or errors share one row with an occurrence count. Hashing and queue
transitions already appear in live metrics. Raw JSON output from `--plain`
retains the full engine event stream.

If the dashboard has no colors, check whether the shell or a tmux server
inherited `NO_COLOR`. Enable colors for just this process with:

```sh
env -u NO_COLOR zoru-miner-cli mine
```

To keep the miner available from another Termux terminal:

```sh
pkg install tmux
tmux new-session -s zoru-miner 'env -u NO_COLOR zoru-miner-cli mine'
```

Enter the password at the hidden prompt. Detach with **Ctrl+B**, then **D**.
Open the running dashboard again with `tmux attach -t zoru-miner`. Pressing
**Q** stops the miner. Do not start another signing instance while it runs.

Native Android validation covers wallet creation, saved setup, password
unlocking, mainnet verification and proof receipts. Run the CLI and wallet
unit tests with:

```sh
cargo test --locked --manifest-path miner/Cargo.toml --bin zoru-miner-cli
cargo test --locked --manifest-path miner/Cargo.toml --lib wallet
```

The seccomp fault-injection suite `wallet_filesystem` is Linux-only and runs
zero tests on Android; use Linux for those checks as described in
[Testing](TESTING.md).
