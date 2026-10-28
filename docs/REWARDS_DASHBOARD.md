# ZORU mainnet reward dashboard

The Rust CLI and Qt desktop share the `inspection` event. All token amounts use decimal **strings** in base units (8 ZORU decimals), not floating-point JSON numbers.

## Global metrics

| JSON field | Ledger / token source | Meaning |
| --- | --- | --- |
| `allocatedSupplyBaseUnits` | Ledger `allocatedSupply()` | All allocated epoch rewards |
| `globalPendingBaseUnits` | `allocatedSupply - fundedSupply` | Rewards allocated but not funded by a seal |
| `globalUnclaimedBaseUnits` | `fundedSupply - paidSupply` | Funded reward liability owed to miners |
| `globalClaimedBaseUnits` | `paidSupply()` | Rewards transferred out through claims |
| `totalSupplyBaseUnits` | Token `totalSupply()` | Current token supply |
| `lifetimeMintedBaseUnits` | Token `lifetimeMinted()` | Cumulative minting |
| `totalAcceptedProofs` | Ledger `totalProofs()` | Accepted proofs, not unique miners |
| `currentEpoch` | Ledger `epochNow()` | Current epoch |
| `admissionPaused`, `claimsPaused` | Ledger getters | Mining admission and claim availability |

Unclaimed is a contract liability, not necessarily claimable if claims are paused. Pending is an allocation, not yet an actual wallet token balance.

## Personal wallet metrics

- `myPendingBaseUnits`: accepted unclaimed proofs in unsealed epochs.
- `myUnclaimedBaseUnits`: accepted unclaimed proofs in sealed epochs.
- `myClaimedBaseUnits`: paid rewards in the synchronized local history.
- `myRewardsHistoryComplete`: false when history is behind the recent head or the pending scan exceeds 10,000 records.
- `myPendingProofsCounted`: number of local outstanding proofs used.

For a proof with zero-based index `i`, epoch budget `B`, and final proof count `n`:

```text
reward = floor(B * (i + 1) / n) - floor(B * i / n)
```

A wallet's history must be synchronized before personal totals can be treated as complete. The read-only mode without a wallet omits personal values rather than displaying misleading zeros.

## CLI

```sh
zoru-miner-cli rewards --manifest ./deployment.json \
  --read "$EVM_RPC" --submit "$EVM_SUBMIT_RPC" \
  --native "$VEX_NATIVE_RPC" --state-dir ./miner-state
```

Add `--wallet ./wallet.json` for personal metrics; the CLI prompts for the keystore password. `rewards` only reads chain state; it does not submit a transaction.

The desktop displays **My mining rewards** and **Global ZORU network**. Its existing Claim action can broadcast transactions and consume VEX gas. Both clients use the confirmed mainnet manifest.
