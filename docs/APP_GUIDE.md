# App guide

## Set up a wallet

Open **Wallet** and create a wallet or import an encrypted wallet, EVM private key or recovery phrase. Recovery phrase imports use the first Ethereum account at `m/44'/60'/0'/0/0`.

Save an encrypted backup outside the application directory. Keep the backup password and recovery details safe. VEX and ZORU use the EVM address shown in Wallet; a native Vexanium account name is a different account type.

Add VEX to this EVM address to pay transaction fees.

## Connect

Open **Nodes**, select your RPC and wait for **Chain verified**. The application verifies Vexanium EVM mainnet and the deployed ZORU contracts before mining. You can add another HTTPS RPC.

## Choose mining settings

Choose 1, 2 or 4 threads, within your CPU capacity. The miner limits its aggregate CPU budget to two cores. Lower intensity reduces CPU use.

Set **Maximum per session (VEX)** and save the allowance. This covers all transactions in that session. The displayed transaction limit is a separate upper bound, and the remaining session allowance can be smaller.

**Stop after first accepted proof** ends the session after a verified proof submission. It does not wait for a reward claim.

## Mine

Select **Start Mining**, enter the wallet password and review the fee authorization. The app searches for valid proofs and submits them to the contract.

An accepted proof is eligible for a share of its epoch's budget. Its reward can change while other proofs are being accepted into that epoch. The application does not count an accepted proof as a paid reward.

**Pause** holds new work. **Stop**, locking the wallet or quitting ends the session. An already submitted transaction can still confirm. Minimized or hidden windows keep the session running. Inactivity auto-lock applies while mining is stopped.

## Claim rewards

Open **Rewards**. **My pending rewards** covers unsealed epochs; **My unclaimed rewards** covers sealed epochs. **My claimed rewards** records rewards already transferred to the wallet.

Select **Claim Rewards** to authorize a separate claim session. It seals eligible closed epochs when needed and claims proofs within the fee allowance. Claims require gas per proof. If the allowance ends before all claims finish, the remaining proofs stay available for another session.

The Rust CLI has its own automatic claim threshold. Desktop mining always uses the manual claim action.

## Read global totals

The Overview and Rewards pages show network totals from the same verified contract snapshot. Allocated rewards, funded rewards, rewards paid, pending rewards and unclaimed rewards describe different stages of the reward process. They are separate from your wallet's totals.

The epoch allocation is shared between accepted proofs. It is not a promised amount per proof. Halving progress follows the contract's emission index of allocated epochs.

## Update

Open **Settings → Software Updates → Check for Updates**. When an update is available, download it and wait for verification before installing.

On Debian or Ubuntu, approve the system administrator request if prompted. Windows closes the app before replacing its files. Mining stops for installation, and starting a new session requires a new authorization.

Updates do not need your wallet password or recovery details. Your wallet, preferences and mining history are stored outside the installation directory.
