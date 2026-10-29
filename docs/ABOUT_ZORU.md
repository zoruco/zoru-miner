# About ZORU

ZORU uses CPU proof of work on Vexanium EVM mainnet. A miner searches for a Keccak-256 digest below the active target and submits the proof to the ZORU Ledger.

## Proofs and epochs

Epochs last 60 seconds. Multiple valid proofs can be accepted in one epoch. Each proof includes the current epoch, authenticated beacon seed, contract generation and beneficiary address. Work from an expired epoch is not eligible for submission.

The Ledger allocates a reward budget when an epoch opens. Accepted proofs share that budget. The initial allocation is 5 ZORU per allocated epoch, with reductions defined by the contract's emission schedule. Halving progress counts allocated epochs, rather than individual proofs or wallet payments.

## Rewards

An accepted proof is not a token transfer. After its epoch closes, the epoch can be sealed and the proof reward claimed. A claim transfers the proof's calculated share to its beneficiary. Claims can be paused independently from proof admission.

The desktop offers manual claims in Rewards. The CLI can claim automatically after a configurable number of accepted proofs remain unclaimed.

ZORU has 8 decimal places and a lifetime mint cap of 21 million tokens. Contract state, source code and the reviewed deployment manifest are maintained in [zoru-pow](https://github.com/zoruco/zoru-pow).

## Fees

VEX pays the network fees for proof submissions, beacon publications, epoch sealing and claims. A claim threshold does not reduce the number of claim transactions. Included transactions may spend gas even when they revert.

Mining performance, reward shares and network fees vary. The application reports contract readings and confirmed transaction outcomes; it does not promise a return.
