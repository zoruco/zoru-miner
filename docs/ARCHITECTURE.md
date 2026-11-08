# Architecture — ZORU mainnet

The Rust CLI (`miner/`) and Qt desktop (`app/`) share one backend. The GUI communicates through a local JSON command/event pipe. Both consume the same manifest, wallet, transaction journal, and proof history.

## Mining

Before submitting a transaction, the client checks EVM chain ID **6736**, the manifest checksum and schema, contract bytecode, proxy implementations, and protocol configuration. Normal operation requires one EVM RPC. If a native endpoint is explicitly supplied, its native identity and head are also checked.

The existing hashing workers send candidates through a bounded 64-entry channel using nonblocking admission. Duplicate tracking is bounded; durable proof history prevents resubmitting a recorded proof. A work poller reads one block-consistent epoch context. A single transaction writer validates candidates, signs, submits, verifies receipts and performs settlement. Claiming never gates hashing workers. Accepted means a verified canonical `ProofAccepted` receipt, not a locally discovered hash.

The Ledger accepts multiple proofs per epoch without changing that epoch's target or seed after each submission. Inclusion must occur in the current epoch with active coordinated work, open admission, a valid issuer, an unused digest below the strict target and available proof capacity. The first proof also requires emission capacity. Once stored, the Ledger's seed takes precedence over the beacon cache; upgrades can change the generation and clear that stored seed. Proofs carry their complete work context and are discarded if it changes. The inclusion margin starts at 15 seconds. After three successful proof transactions, it uses the slowest of the last 16 preflight-to-receipt durations, rounded up with two seconds of headroom and bounded to 5–15 seconds. Learning is local to the current client session; chain, work, fee and nonce checks remain in force. The worker deadline uses the actual epoch end and a fresh work lease; the signing deadline separately subtracts the inclusion margin. Closing the signing window does not stop hashing. Late candidates are discarded locally before transaction RPC/signing and cannot be assumed to earn rewards. Workers stop when work actually expires, including when an RPC stops advancing.

A background preparer fetches and authenticates beacon signatures without occupying the transaction writer. The writer prioritizes a ready current-epoch beacon and rechecks its epoch, seed and expected round before signing. Beacon retries reuse the work poller's fresh block snapshot to select the epoch. They run only when work is unavailable and the inclusion window is still open. A closed window or stale snapshot does not trigger redundant beacon lookups. Publication retains its own on-chain epoch, seed and admission checks. Automatic settlement yields to queued proofs for up to three accepted proofs between claims; an empty proof queue permits settlement when the rollover reserve is available. The CLI and desktop receive the same adaptive margin and explicit work-wait reason.

The terminal dashboard and desktop consume shared session snapshots and events. Rendering never calls RPC or runs mining. Wallet/global snapshots refresh every 15 seconds independently of work and transaction handling; the desktop's standalone monitor runs only outside an active session. The terminal retains 512 events, redraws at most five times per second and falls back to JSON without a TTY. If proof or claim history changes while a snapshot is collected, personal totals remain unavailable until the next consistent refresh. RPC counters, queue bounds, stale candidates, CPU use and receipt latency are measured by the shared backend.

## Wallet and transaction recovery

Wallets remain encrypted on disk. The Rust backend handles wallet signing. Signed transactions are journaled before submission. Retries use the original signed bytes and require an absent transaction hash and unchanged latest/pending nonces. Known pending transactions are not rebroadcast. If a nonce is consumed but its canonical outcome cannot be verified, signing stops pending reconciliation.

The expected wallet address is checked before opening signing state or recovering transactions. Recovery reads the mining epoch from the signed calldata and verifies its signer, chain, nonce, destination and hash. A proof or beacon transaction is archived as obsolete only when its hash is absent, both nonce views are unchanged, and its mining epoch is closed. A beacon publication can also be archived after its seed is verified on-chain. Archive names include the transaction hash so reusing a nonce cannot overwrite an earlier record.

New proof journals additionally retain their validated work context. A replay must match current epoch, seed, generation and target. Older journals remain readable for receipt/nonce recovery and closed-epoch archival; an old proof without work metadata cannot be automatically replayed while its epoch remains open.

New transactions use the submission RPC's `eth_gasPrice` quote after checking its chain ID. The gas limit cannot exceed 22,000,000, and the maximum transaction fee cannot exceed 3.3 VEX. The wallet must cover the quoted maximum fee before signing. See the [JSON-RPC specification](https://ethereum.org/developers/docs/apis/json-rpc/#eth_gasprice).

Per-account local locks do not coordinate separate machines. Keep journals and SQLite state directories stable across upgrades, and avoid sharing an account between hosts.

## Rewards

SQLite indexes accepted proof events and claims. The global dashboard reads allocated/funded/paid supply directly from the Ledger. Personal pending and unclaimed values are calculated from the wallet's accepted proof indices and each epoch's budget and proof count, with explicit partial-history status if the local index is incomplete.

## Mainnet release

The mainnet manifest schema is `zoru-mainnet-suite-1`. Packages use the confirmed deployment addresses and verified contract fingerprints.

Automatic settlement reserves at least 20 seconds, or twice the current send guard, before the next epoch. It does not start while an epoch beacon is missing. A verified RPC fallback change resets learned timing to the conservative initial guard.
