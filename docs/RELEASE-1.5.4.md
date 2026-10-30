# ZORU Miner 1.5.4

## Reliability

- Stop mining work at the 15-second relay-inclusion safety cutoff before an epoch ends. The same cutoff predicate is used when validating a proof submission, avoiding unnecessary hashing and queued candidates that cannot be included in time.
- Stale candidates are discarded locally and remain warnings, not on-chain rejected proofs. Queued results from an old epoch are dropped before the transaction writer, avoiding a burst of per-candidate stale warnings.

## Platform validation

- Linux x86_64 package and automated Rust test suite are validated.
- Windows x64 binaries and installer are cross-built, but native Windows UI/runtime testing has **not** been completed. The Wine smoke test was inconclusive because Wine prefix initialization timed out. Windows UI, mining controls, updater installation, and wallet flows remain unverified.
- The Windows installer is not Authenticode-signed.

See [UPDATES.md](../docs/UPDATES.md) and [TESTING.md](../docs/TESTING.md) for validation details.

## In-app update feed

The signed update feed is not published yet because the available release signing key does not match the application's configured verification public key. The verification key was left unchanged; no unsigned or mismatched metadata was published. The desktop updater will not offer 1.5.4 until the matching authorized signing key is available.
