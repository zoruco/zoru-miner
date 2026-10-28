# Contributing

Contributions to the miner, desktop, tests, and documentation are welcome. Open an issue before proposing a substantial change, or submit a focused pull request with a clear description of the problem.

The Rust backend owns mining rules, signing, transaction recovery, and reward accounting. Desktop changes should use that backend rather than duplicate its behavior.

## Before opening a pull request

- Keep the change focused and add a regression test for behavior changes.
- Run Rust formatting, Clippy, and the relevant tests from [Testing](docs/TESTING.md).
- Run isolated signing and recovery tests when changing wallets, nonces, receipts, proof limits, or claims.
- Include the commands used, outcomes observed, and any tests not run.
- Preserve third-party licenses and required notices.

Do not commit credentials, wallet files, private journals, local deployment records, or test results containing account data. Report exploitable security issues privately as described in [Security](SECURITY.md).

Maintainers review source changes and releases separately. A merged pull request does not authorize deployment or on-chain transactions.
