# Architecture — ZORU mainnet

The Rust CLI (`miner/`) and Qt desktop (`app/`) share one backend. The GUI communicates through a local JSON command/event pipe. Both consume the same manifest, wallet, transaction journal, and proof history.

## Mining

Before submitting a transaction, the client checks EVM chain ID **6736**, VEX native identity, the manifest checksum and schema, contract bytecode, proxy implementations, and protocol configuration. Worker threads compute Keccak proof candidates for the active epoch. Stale work is discarded before broadcast.

A single transaction writer submits accepted candidates and verifies receipt events. Accepted means confirmed by an on-chain `ProofAccepted` event, not merely a locally discovered hash.

## Wallet and transaction recovery

Wallets remain encrypted on disk. The Rust backend handles wallet signing. Signed transactions are journaled before submission. Retries use the original signed bytes. If a nonce is consumed but its canonical outcome cannot be verified, signing stops pending reconciliation.

The expected wallet address is checked before opening signing state or recovering transactions. Recovery reads the mining epoch from the signed calldata and verifies its signer, chain, nonce, destination and hash. A proof or beacon transaction is archived as obsolete only when its hash is absent, both nonce views are unchanged, and its mining epoch is closed. A beacon publication can also be archived after its seed is verified on-chain. Archive names include the transaction hash so reusing a nonce cannot overwrite an earlier record.

New transactions use the submission RPC's `eth_gasPrice` quote after checking its chain ID. The gas limit cannot exceed 22,000,000, and the maximum transaction fee cannot exceed 3.3 VEX. The wallet must cover the quoted maximum fee before signing. See the [JSON-RPC specification](https://ethereum.org/developers/docs/apis/json-rpc/#eth_gasprice).

Per-account local locks do not coordinate separate machines. Keep journals and SQLite state directories stable across upgrades, and avoid sharing an account between hosts.

## Rewards

SQLite indexes accepted proof events and claims. The global dashboard reads allocated/funded/paid supply directly from the Ledger. Personal pending and unclaimed values are calculated from the wallet's accepted proof indices and each epoch's budget and proof count, with explicit partial-history status if the local index is incomplete.

## Mainnet release

The mainnet manifest schema is `zoru-mainnet-suite-1`. Packages use the confirmed deployment addresses and verified contract fingerprints.
