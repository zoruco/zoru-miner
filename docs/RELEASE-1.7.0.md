# ZORU Miner 1.7.0

Mining continues on an eligible preceding-epoch job while the next authenticated beacon is prepared. The transition lasts at most 30 seconds and ends when the new seed becomes available. The active work epoch and its deadline are shown in the terminal. Both the terminal and desktop use the same Rust engine.

Work is obtained through one atomic Ledger response. Transaction admission remains separate from hashing. A 15-second hashing lease tolerates brief RPC delays; signing still requires a snapshot less than five seconds old. Claims yield to beacon publication and queued proofs. Candidate queues remain bounded and nonblocking.

This release requires the continuous-work Ledger and its matching deployment manifest. It contains one mining protocol path. Scheduled upgrades, authenticated randomness, canonical receipts, nonce journals, reward accounting and supply limits remain enforced. No wallet, password or signer state is included in the packages.

Normal epoch rollover should no longer cause an idle interval while an eligible preceding job exists. Closed admission, resource limits, stale RPC work, or a beacon unavailable beyond the bounded transition can still pause hashing. Rates always reflect hashes actually computed.

Packages: Linux x86_64 desktop/CLI, Linux amd64 Debian package and Termux aarch64 CLI archive. Validation runs on the VPS. No workflows or CI are included.
