# Devnet deployments

Live on Solana devnet as of 2026-04-17. Perc5ive now runs on a **custom Five VM loader** we built from the latest `five-solana` + `five-vm-mito` + `five-protocol` sources (includes all seven upstream opcode PRs bundled in). Markets stay on the stock loader — they're pure DSL and don't need the new opcodes.

## Loaders

| Loader | Program ID | Source | Purpose |
|---|---|---|---|
| **Custom (ours)** | [`CTSPYe2YTciJr2oHqGZZr6H8GnaSNCNENZdcjTM85Dq9`](https://explorer.solana.com/address/CTSPYe2YTciJr2oHqGZZr6H8GnaSNCNENZdcjTM85Dq9?cluster=devnet) | `/home/marche/5iveVM/five-solana` @ main + our branches on `five-protocol#37,38,39` and `five-vm-mito#84,85,86,87` | Runs the linked perc5ive engine — needs `MULDIV_REM_U256`, field-u128/i128 ops, chunked deploy |
| **Stock** | [`J99pDwVh1PqcxyBGKRvPKk8MUvW8V8KF6TmVEavKnzaF`](https://explorer.solana.com/address/J99pDwVh1PqcxyBGKRvPKk8MUvW8V8KF6TmVEavKnzaF?cluster=devnet) | 5iveVM canonical loader, last deploy 2025-08-20 | Runs all three markets (sov / pyth_race / lp_perp) — pure DSL, no new opcodes |

Both loaders share the same VM ABI for the DSL subset, so the markets-vs-engine dual-loader setup is transparent to clients.

## Scripts

> **ℹ️ Partial devnet live, partial stale as of 2026-04-17** — perc5ive is live in its full 1550 B linked form on our custom loader. The legacy 463 B perc5ive at `2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK` on the stock loader is kept for reference but is NOT the canonical entry point anymore.

| Artifact | Program ID | Loader | Deploy tx | Bytecode size |
|---|---|---|---|---|
| **perc5ive engine** (linked, Full-scope) | [`873y96dgbUKBfRu971Vx8UTTSCVqQz1MopJfaCk18yS5`](https://explorer.solana.com/address/873y96dgbUKBfRu971Vx8UTTSCVqQz1MopJfaCk18yS5?cluster=devnet) | Custom `CTSPYe...` | [`4a9YEfsTVkynL3rDWSFMQu8XLbEakiJPwoQFA5CoMD4L2mHzncYvWpP7UXGYqg1Xt6KiXUeNw42iDT9MQk3TX8rW`](https://explorer.solana.com/tx/4a9YEfsTVkynL3rDWSFMQu8XLbEakiJPwoQFA5CoMD4L2mHzncYvWpP7UXGYqg1Xt6KiXUeNw42iDT9MQk3TX8rW?cluster=devnet) + 3 chunks | **1550 B** |
| perc5ive engine (legacy, unlinked) | [`2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK`](https://explorer.solana.com/address/2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK?cluster=devnet) | Stock `J99p...` | [`2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4`](https://explorer.solana.com/tx/2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4?cluster=devnet) | 463 B (kept for historical reference) |
| **Sov** (inverted memecoin perp) | [`2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV`](https://explorer.solana.com/address/2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV?cluster=devnet) | Stock `J99p...` | [`3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje`](https://explorer.solana.com/tx/3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje?cluster=devnet) | 281 B |
| **PythRaceMarket** | [`5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i`](https://explorer.solana.com/address/5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i?cluster=devnet) | Stock `J99p...` | [`4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG`](https://explorer.solana.com/tx/4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG?cluster=devnet) | 283 B |
| **LPPerp** | [`DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp`](https://explorer.solana.com/address/DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp?cluster=devnet) | Stock `J99p...` | [`2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b`](https://explorer.solana.com/tx/2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b?cluster=devnet) | 266 B |

Custom-loader VM state account: `BF8N6oqw2RmRf6XGWfJ96DZXgPzyc6xCgE6R9zR4u7GY`. Stock-loader VM state PDA (used by the legacy perc5ive + the three markets): `H5ykzUdetT5Lk81GHBe8Netejyw7t1spkN2ZehgRQZpp`.

## Why a custom loader

When we attempted to redeploy the linked perc5ive (1550 B) against the stock loader on 2026-04-17, three distinct issues surfaced:

1. **Stock loader predates chunked deploy.** The `InitLargeProgram`/`AppendBytecode` instructions (discriminators 4/5) that programs over 800 B need were added to `five-solana` in commit `db5703d` (2025-12-30), but the program at `J99p...` was last deployed 2025-08-20. Four months of skew.
2. **`FIVE_VM_PROGRAM_ID` placeholder in five-sdk.** `src/types.ts` shipped with `"Five111111111111111111111111111111111111111"` — points at nothing. Filed as [`5iveVM/five-sdk#2`](https://github.com/5iveVM/five-sdk/pull/2) with a fix + better error surfacing.
3. **`VM_STATE_SIZE` drift in the SDK.** SDK allocated 48 B; the current on-chain state layout is 56 B (`FIVEVMState::LEN` in `five-solana/src/state.rs`). Local fix on our SDK build.

The upstream fix path (waiting on 5ive maintainers to merge our PRs, republish `@five-vm/cli`, and redeploy the on-chain loader) is weeks of coordination. The self-host path was half a day:

```bash
cd /home/marche/5iveVM/five-solana
cargo build-sbf                                      # link in our four open upstream PRs
solana program deploy target/deploy/five.so --url devnet \
    --program-id target/deploy/five-keypair.json      # our loader at CTSPYe...
```

Our loader keypair (`CTSPYe2YTciJr2oHqGZZr6H8GnaSNCNENZdcjTM85Dq9`) is stable — future perc5ive upgrades are `solana program deploy`-in-place. Upgrade authority sits with the deployer wallet (`25TudRmaeaGcbhWXrZfamMeJJ8yQEjz63UpkK7LVQiUA`).

## Current state

| Property | Value |
|---|---|
| Perc5ive runs Full-scope Percolator handlers on-chain | ✅ yes (1550 B linked, all 9 handlers wired) |
| Markets deployable | ✅ yes (stock loader, no blocking issue) |
| Deploy is reproducible from fresh clone | ✅ yes (`cargo run --bin link-perc5ive` is byte-exact) |
| Test coverage | 191 tests green across perc5ive + bench + mcp |
| Upstream VM loader has our opcodes | ⏳ pending maintainer action — not blocking our path |

Judges can reproduce the linked binary offline from a fresh clone:
```bash
cargo build && cargo run --bin link-perc5ive
cargo test --test devnet_reproducibility   # proves the output is correct
```

## What "linked" means

`cargo build` produces `target/perc5ive.fbin` — the DSL compiler's output with 9 handler bodies stubbed as `PUSH_U64 <sentinel>; RETURN_VALUE`. On its own this binary can't execute any of the u128 / i256 risk-math handlers; the VM would hit a sentinel, push it, and return with no side effect.

The linker (`cargo run --bin link-perc5ive`) takes that `.fbin`, normalizes the 6-byte DSL header to the 10-byte VM-native header, appends all 9 hand-written handler bodies, and rewrites every sentinel `PUSH_U64 <sentinel>; RETURN_VALUE` in-place to `CALL 0 <callee>; RETURN_VALUE; NOP*`. The result is `target/perc5ive.linked.bin` — 1550 bytes, zero sentinels, one contiguous VM-native binary ready for `five deploy`.

## How to redeploy

```bash
cargo build                              # regenerate .fbin artifacts
scripts/deploy.sh perc5ive --target devnet   # auto-runs link-perc5ive then deploys
scripts/deploy.sh sov --target devnet
scripts/deploy.sh pyth_race --target devnet
scripts/deploy.sh lp_perp --target devnet
```

Pass `--dry-run` to simulate without spending SOL. The script reads `solana config get` for the default cluster + keypair; override with `--target` and `--keypair`.

For perc5ive specifically, `scripts/deploy.sh` runs the linker before handing the binary to `five deploy`. To inspect what would be deployed without actually pushing:

```bash
cargo run --bin link-perc5ive         # prints the append map: sentinel → bytes → callee offset
xxd target/perc5ive.linked.bin | head
```

## How to verify

```bash
# Program bytecode on devnet:
solana account 2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK --url devnet

# Bytecode matches what we'd re-link locally (reproducibility check):
cargo test --test devnet_reproducibility
```

Or open any of the explorer links above. Each program account holds the bytecode whose first four bytes are `5IVE`. If `scripts/deploy.sh perc5ive --target devnet` has been run since 2026-04-17, the deployed data account should be 1550 bytes, not 463.

## Cost

Deployment cost scales with account size (rent-exempt minimum). Linked perc5ive at 1550 bytes is ~3× the raw fbin's 463-byte rent minimum but still well under 0.02 SOL. Markets are unchanged. Redeploys reuse the existing program accounts and reimburse the delta only.
