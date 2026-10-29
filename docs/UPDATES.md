# Releases and updates

## Using Software Updates

The desktop checks the signed update feed attached to the latest [official release](https://github.com/zoruco/zoru-miner/releases/latest). It verifies the release signature before offering a download, then checks the installer's size and SHA-256 checksum.

Debian and Ubuntu installations use the system package manager. Windows installations close the application and run the verified installer. Wallet files and mining history are kept outside the installation directory. Portable users can also replace their application files from a verified release archive.

## Building packages

Build the application and tests first. The package version comes from `miner/Cargo.toml`; the desktop and command line tools use that version.

For Linux, run `packaging/bundle.py --help`, then create a Debian package with `packaging/debian.py`. Supply the reviewed mainnet manifest and intended token and Ledger addresses. Packages must contain `bin/zoru-miner-gui`, `bin/zoru-miner-cli`, `bin/zoru-update` and `share/zoru-miner/deployment.json`.

For a Windows cross build, install MinGW-w64, NSIS, the Rust `x86_64-pc-windows-gnu` target, Qt 6.2.4 Windows MinGW libraries and matching host Qt tools. Set `QT_WIN_ROOT` and `ZORU_MAINNET_MANIFEST`, then run `packaging/windows-cross-build.sh`.

Run the Windows binaries and desktop tests in a Windows runtime before publication, and record the runtime used. A cross build alone does not verify application behavior. Include Linux installation, desktop navigation, mining controls, receipt handling and reward claims in release validation. Environment-dependent tests that were skipped are not evidence of a passing deployment.

## Signing update information

`config/update-public-key.hex` contains the update verification key. The private signing key belongs outside the repository and release archives.

Use `packaging/sign-update.py` with the release version, user-facing release notes and the final Linux and Windows installers. It checks installer identity and produces `update.json` for the release. Keep its expiry current when maintaining a release feed. Publish checksums for every downloadable package.

Signing update information does not Authenticode-sign the Windows executable or installer. Do not describe an unsigned Windows installer as code-signed.
