# Building releases

Build the application and run the relevant checks in [Testing](TESTING.md) before packaging. The package version comes from `miner/Cargo.toml`; the desktop and command line tools use that version.

## Linux

Run `packaging/bundle.py --help`, then create a Debian package with `packaging/debian.py`. Supply the reviewed mainnet manifest and intended token and Ledger proxy addresses. Packages must contain `bin/zoru-miner-gui`, `bin/zoru-miner-cli`, `bin/zoru-update` and `share/zoru-miner/deployment.json`.

Validate installation, desktop navigation, mining controls, receipt handling and reward claims on the intended platform. Publish the archive and installer checksums with the matching source revision and deployment manifest.

## Windows

A cross build requires MinGW-w64, NSIS, the Rust `x86_64-pc-windows-gnu` target, Qt 6.2.4 Windows MinGW libraries and matching host Qt tools. Set `QT_WIN_ROOT` and `ZORU_MAINNET_MANIFEST`, then run `packaging/windows-cross-build.sh`.

Run the Windows binaries and desktop tests in a Windows runtime before publication, and record the runtime used. A cross build alone does not verify application behavior. The current release is published for Linux; earlier Windows coverage is recorded in [the release history](RELEASE-1.5.4.md).

## Signed update information

`config/update-public-key.hex` contains the update verification key. Keep the existing private signing key outside the repository and release archives.

Use `packaging/sign-update.py` with the release version, user-facing release notes and the final installers for the validated platforms. It checks installer identity and produces `update.json`. Keep the feed expiry current and publish checksums for every downloadable package.

Signing update information does not Authenticode-sign a Windows executable or installer. Describe platform validation and code signing accurately in the release notes.
