# ZORU Miner 1.5.4

## Reliability

- Stop mining work at the 15-second relay-inclusion safety cutoff before an epoch ends. The same cutoff predicate is used when validating a proof submission, avoiding unnecessary hashing and queued candidates that cannot be included in time.
- Stale candidates are discarded locally and counted in staleBeforeBroadcast, not treated as on-chain rejected proofs. Queued results from an old epoch are dropped before the transaction writer; last-moment epoch races are also counted as local discards instead of RPC warnings.

## Platform validation

- Linux x86_64 package and automated Rust test suite are validated.
- Windows x64 binaries and installer are cross-built, but native Windows UI/runtime testing has **not** been completed. The Wine smoke test was inconclusive because Wine prefix initialization timed out. Windows UI, mining controls, updater installation, and wallet flows remain unverified.
- The Windows installer is not Authenticode-signed.

See [UPDATES.md](../docs/UPDATES.md) and [TESTING.md](../docs/TESTING.md) for validation details.

## In-app update feed

The verification key is restored to the established public key used by v1.5.3 and matches the protected release signing key stored outside the repository. The v1.5.4 feed is signed and published. Builds of v1.5.4 distributed before this correction contain a mismatched trust anchor and require manual replacement; v1.5.3 builds can verify the signed 1.5.4 update.
