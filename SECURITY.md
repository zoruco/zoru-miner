# Security

## Wallets

Use an encrypted Web3 v3 keystore and keep its password separate. The miner supports bounded scrypt and PBKDF2-HMAC-SHA256 parameters. Do not put passwords or private keys in command arguments, source code or logs.

Keep a signing wallet and its transaction journal on one machine. Local file locks prevent concurrent signers on the same host but cannot prevent the same wallet from being used elsewhere. Retain the journal for nonce recovery; do not blindly resend transactions when their outcome is unknown.

## Network and contracts

The miner verifies VEX EVM chain ID **6736**, native network identity, the deployment manifest, contract bytecode and module implementations. Use trusted RPC endpoints and verify the manifest's publisher separately. A checksum verifies file integrity, **not publisher authenticity**.

The ZORU Token owner can change the issuer, upgrade mining modules, and pause mining or claims. The lifetime supply cap remains enforced by the Token. The mining-only issuance policy depends on the owner's decisions.

Mining spends VEX gas. Accepted work does not guarantee profit or immediate settlement. Network reorganizations, unavailable RPC nodes and incomplete local history can delay confirmation or require recovery.

## Reporting

Send reproducible security issues privately to the maintainers. Never include keystore passwords, private keys or signing journals.
