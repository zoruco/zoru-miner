# Architecture — ZORU mainnet

The Rust CLI (`miner/`) and Qt desktop (`app/`) share one backend. The GUI communicates through a local JSON command/event pipe. Both consume the same manifest, wallet, transaction journal, and proof history.

## Mining

Before submitting a transaction, the client checks EVM chain ID **6736**, VEX native identity, the manifest checksum and schema, contract bytecode, proxy implementations, and protocol configuration. Worker threads compute Keccak proof candidates for the active epoch. Stale work is discarded before broadcast.

A single transaction writer submits accepted candidates and verifies receipt events. Accepted means confirmed by an on-chain `ProofAccepted` event, not merely a locally discovered hash.

## Wallet and transaction recovery

Wallets remain encrypted on disk. The Rust backend handles wallet signing. Signed transactions are journaled before submission. Retries use the original signed bytes. If a nonce is consumed but its canonical outcome cannot be verified, signing stops pending reconciliation.

Per-account local locks do not coordinate separate machines. Keep journals and SQLite state directories stable across upgrades, and avoid sharing an account between hosts.

## Rewards

SQLite indexes accepted proof events and claims. The global dashboard reads allocated/funded/paid supply directly from the Ledger. Personal pending and unclaimed values are calculated from the wallet's accepted proof indices and each epoch's budget and proof count, with explicit partial-history status if the local index is incomplete.

## Mainnet release

The mainnet manifest schema is `zoru-mainnet-suite-1`. Packages use the confirmed deployment addresses and verified contract fingerprints.
