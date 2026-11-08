# ZORU Miner 1.6.0

The CLI and desktop share updated mining scheduling. Beacon signatures are fetched and authenticated in the background while the single transaction writer retains control of nonce ordering, signing and recovery. Ready current-epoch beacons take priority over optional settlement. Automatic claims yield to queued proofs between settlement turns.

The proof submission guard starts at 15 seconds and, after three successful proof transactions, adapts between 5 and 15 seconds using the slowest recent full transaction duration plus two seconds of headroom. It resets on a new client session. This reduces the fixed idle window on a responsive RPC. It does not guarantee uninterrupted hashing: missing epoch seeds, closed admission, expired work leases and insufficient submission time still pause useful mining.

Both interfaces report the adaptive guard and the reason for waiting. The terminal has a responsive colored dashboard with a connected history line that retains zero-rate intervals. The desktop also retains measured zero-rate samples and shows the current rate separately from the last recorded rate. Fresh setups use `https://api.windcrypto.com/rpc`; saved user endpoints remain unchanged.

This release includes native Linux x86_64 desktop/CLI packages and a Termux ARM64 CLI archive. Windows binaries are not included because native Windows validation has not been performed for this release. Keep the encrypted wallet, recovery backup and signing journals between upgrades. No wallet credentials are included in the packages.

Automatic settlement reserves at least 20 seconds, or twice the current send guard, before the next epoch. It does not start while an epoch beacon is missing. A verified RPC fallback change resets learned timing to the conservative initial guard.
