# Software updates

Open **Software Updates** to check for a current release. ZORU Miner verifies the signed release information, then checks the downloaded installer's size and checksum before offering installation.

Installing an update ends the current mining session. Transactions already sent can still confirm afterward. Your wallet, proof history and transaction journals remain in their existing data directory. Keep that directory when replacing or reinstalling the application.

Debian and Ubuntu installations use the system package manager. Portable users can replace the application files from a verified release archive while keeping their wallet and data directories separate. Updating the application does not require a wallet password.

The current release provides Linux x86_64 packages through [official Releases](https://github.com/zoruco/zoru-miner/releases/latest). Each package includes its matching mainnet deployment manifest; replace the application and manifest together.

If release information is expired or verification fails, the updater does not install the download. Check the official release page and the checksums attached to that release before reinstalling. Keep a separate wallet recovery backup.
