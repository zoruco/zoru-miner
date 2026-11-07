# ZORU Miner CLI — Execution Roadmap

This document tracks implementation on the canonical `main` branch of `zoruco/zoru-miner`. Checkboxes describe verified implementation work, not intentions.

## Constraints

- Mainnet-first production CLI for Vexanium EVM (chain ID 6736).
- Preserve the trusted deployment manifest, wallet encryption, chain/contract checks, safe transaction nonce reconciliation, persistent journal, and existing proof accounting.
- No mandatory native RPC for normal EVM mining; validate which native-only health observations must be replaced using EVM RPC before removal.
- Use a single configurable EVM RPC URL for ordinary operation, with separate read and submit clients internally if needed.
- Proof discovery must not be blocked by RPC transmission, receipt polling, sealing, or claiming; never hash or broadcast against knowingly stale epoch work.
- Retain a noninteractive, plain log mode for services/redirected output. Never expose wallet secrets in logs or config.
- Avoid unnecessary requests and duplicate fetch logic. Profile and measure before optimizing.

## Phase 0 — Baseline and safety

- [ ] Record baseline startup behavior, RPC calls/minute, hash throughput, stale-proof rate, accepted/rejected counts, transaction latency, and CPU use.
- [ ] Map all native RPC, read/submit, epoch refresh, receipt, sync, seal, claim and wallet call sites.
- [ ] Capture baseline tests; ensure no source tree or runtime changes affect the active mainnet miner.
- [ ] Define acceptance tests for restart, RPC stall, duplicate nonce, epoch rollover, relay outage, network mismatch, and Ctrl-C.

## Phase 1 — One-command CLI and configuration

- [ ] Add `zoru-miner-cli setup` for first-run configuration and encrypted-wallet selection.
- [ ] Support `zoru-miner-cli mine` reading an XDG config by default (Windows equivalent per OS).
- [ ] Keep explicit flags as overrides, including deployment manifest selection, and validate effective settings.
- [ ] Add `--rpc` as one EVM endpoint, preserving separate internal client pacing and safe submit semantics.
- [ ] Remove required `--native` from the CLI only after EVM-only health/identity test coverage.
- [ ] Resolve trusted manifest, state path, workers, CPU budget, and autoclaim defaults automatically.
- [ ] Define safe wallet unlocking without persisting plaintext password in the TOML config.
- [ ] Test clean install, existing installation migration, config precedence, invalid config, and platform paths.

## Phase 2 — Stable terminal dashboard (TUI)

- [ ] Add a responsive terminal UI using `ratatui` and `crossterm` or comparably maintained Rust libraries.
- [ ] Fixed header: VEX/ZORU balance, unclaimed rewards, mining status, hash rate, epoch/target, and contract-global stats verifiable from chain.
- [ ] Scrolling event panel with bounded memory, coloured categories, and keyboard controls (Q, arrows, logs).
- [ ] Reuse existing mining state snapshots; UI refresh must not trigger a dedicated RPC loop.
- [ ] Render unavailable data as N/A; never infer active miner counts from proof totals.
- [ ] Auto-detect non-TTY environments and support `mine --plain`; limit screen redraws.
- [ ] Test short terminals, resize, error visibility, and terminal restore after failures/signals.

## Phase 3 — Continuous mining pipeline

- [ ] Separate proof hashing from local proof validation, queueing, signed submission, receipt tracking, seal, and claim.
- [ ] Ensure that finding a valid nonce does not synchronously pause hashing workers on RPC or receipt.
- [ ] Bounded proof queues with duplicate suppression and epoch/generation/seed/target tagging.
- [ ] Implement prompt epoch-work switching and cancellation of known-stale work, including an epoch rollover near transaction inclusion.
- [ ] Preserve single-writer transaction nonce management and existing durable journal reconciliation; enforce safe backpressure during relay outages.
- [ ] Ensure auto-claim and seal operations never block hashing on valid work.
- [ ] Verify max-proofs-per-epoch, admission pause, upgrades, seed invalidation, wallet funding, and user-requested stop behavior.
- [ ] Add concurrency, stale-proof, nonce-collision, retry and epoch-race tests.

## Phase 4 — RPC efficiency and acceptance

- [ ] Profile RPC methods, response latency, errors and client-side request rates with production-like workloads.
- [ ] Eliminate repeat reads where justified, cache immutable-per-work data with clear invalidation, and minimize history replays.
- [ ] Compare CPU hash throughput, accepted proofs, rejected proofs, stale work, fees, and RPC volume against Phase 0 baseline.
- [ ] Pass Rust checks, unit/integration tests, a controlled EVM end-to-end test, and a restart/recovery test.
- [ ] Update installation, security, and CLI documentation for mainnet only.
- [ ] Commit changes to `main` with reviewable atomic commits; push only after verified tests.
- [ ] Release and deploy separately after verifying the live miner is not interrupted unexpectedly.

## Completion rule

Check off an item **only** when the corresponding code/docs actually exist and the relevant test or verification is recorded. Do not label planned features as completed. Record test commands and results here as phases complete.

## Current baseline (2026-11-07)

- VPS1 source: `/root/windcrypto/labs/zoru-miner`, `main`, git HEAD `a79e26f` when roadmap created, working tree clean.
- CLI package version: `1.5.7`. The current `Connection` requires manifest, read, submit, native and state-dir arguments.
- VPS2 production `zoru-mainnet-zevo.service` was observed `active`; production has not been modified.
- Contract review: `ZoruLedger.submit` accepts multiple proofs per epoch and is independent from later seal/claim. Proof validity is determined at transaction inclusion; epoch/seed/target/generation changes still require careful synchronization.
