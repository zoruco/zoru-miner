# ZORU Miner 1.5.5

The miner supports the current ZORU mainnet contracts and preserves existing wallet files, transaction journals and proof history when updated.

- Faster hashing reuses the constant part of each mining challenge.
- Transaction recovery checks the canonical transaction that consumed a nonce, including transactions outside the recent block window.
- Lost submission responses retain the signed transaction hash for reconciliation before another submission.
- Trusted RPC fallbacks verify the chain before use.
- Beacon publication follows the current epoch after slow requests, with bounded retries and no duplicate publication queue.
- Mining and automatic settlement respect both contract maintenance pauses.
- Reward history avoids repeated queries for finalized proofs while retaining recent reorganization checks.

The Linux release includes the reviewed mainnet manifest. Existing users should replace the application and manifest together and keep their wallet and state directories.

Linux validation includes the Rust test suite, signed recovery and claim fixtures, four desktop test groups, package checks, isolated native mining and automatic claiming. The native integration also covers two miners, restart, an RPC outage, delayed responses and loss of an accepted submission response. Continuous native mining ran for 30 minutes with 106 accepted proofs and no recorded errors.

This release is published for Linux x86_64. A Windows package requires separate runtime validation before publication.
