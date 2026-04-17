# Devnet deployments

Live on Solana devnet. Program IDs are stable across redeploys; the underlying bytecode is upgraded in place.

> **⚠️ Linked perc5ive redeploy blocked on an on-chain VM upgrade (2026-04-17)** — the currently-deployed perc5ive engine is the *pre-linked* 463 B binary (sentinel-stubbed handlers). The linked binary is 1550 B and locally reproducible (`cargo run --bin link-perc5ive`; verified by `tests/devnet_reproducibility.rs::linked_perc5ive_has_no_sentinels_and_expected_size`), but it can't ship to the currently-deployed VM loader. Details in the "Deploy blockers" section below.

| Artifact | Program ID | Initial deploy tx | Current bytecode size | After relink (needs redeploy) |
|---|---|---|---|---|
| **perc5ive engine** | [`2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK`](https://explorer.solana.com/address/2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK?cluster=devnet) | [`2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4`](https://explorer.solana.com/tx/2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4?cluster=devnet) | 463 B (stale, unlinked) | **1550 B (linked)** |
| **Sov** (inverted memecoin perp) | [`2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV`](https://explorer.solana.com/address/2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV?cluster=devnet) | [`3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje`](https://explorer.solana.com/tx/3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje?cluster=devnet) | 281 B | 281 B (no change) |
| **PythRaceMarket** | [`5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i`](https://explorer.solana.com/address/5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i?cluster=devnet) | [`4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG`](https://explorer.solana.com/tx/4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG?cluster=devnet) | 283 B | 283 B (no change) |
| **LPPerp** | [`DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp`](https://explorer.solana.com/address/DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp?cluster=devnet) | [`2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b`](https://explorer.solana.com/tx/2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b?cluster=devnet) | 266 B | 266 B (no change) |

The perc5ive engine's VM-state PDA is `H5ykzUdetT5Lk81GHBe8Netejyw7t1spkN2ZehgRQZpp`.

## Deploy blockers (2026-04-17)

Three distinct problems surfaced while attempting a live redeploy on 2026-04-17. Two are fixed locally; one is outside our control.

### 1. ✅ Fixed locally — `FIVE_VM_PROGRAM_ID` placeholder in five-sdk

`five-sdk/src/types.ts` shipped with `FIVE_VM_PROGRAM_ID = "Five111111111111111111111111111111111111111"` — a placeholder that points at no deployed program. Every deploy that didn't pass an explicit `FiveSDKConfig.fiveVMProgramId` override failed with `ProgramAccountNotFound`.

Upstream PR: [5iveVM/five-sdk#2](https://github.com/5iveVM/five-sdk/pull/2). Also tightens error surfacing in `deployLargeProgramToSolana`'s catch block so the actual RPC error propagates instead of being collapsed to `"Unknown large deployment error"`.

### 2. ✅ Fixed locally — null-check race in chunked deploy

When chunked deployment queries the script account before appending, an eventual-consistency window can return `null` and the SDK would NPE on `.data.length`. Retry-after-delay fix was already present in `five-sdk` main (commit `e583f06`, 2025-12-30); this is only visible to callers once the npm `@five-vm/cli` republishes with the updated SDK bundle. No PR needed from us.

### 3. ⛔ Blocked on upstream — on-chain VM predates chunked deployment

The deployed loader program at `J99pDwVh1PqcxyBGKRvPKk8MUvW8V8KF6TmVEavKnzaF` was last deployed on **2025-08-20** (slot 402433866, visible via `solana program show ... --url devnet`). The `InitLargeProgram` / `AppendBytecode` discriminators (4 / 5) that chunked deployment relies on were added to `five-solana` in commit `db5703d` on **2025-12-30** — four months after the on-chain program was last updated.

Net effect: **no chunked-deploy path works on this devnet loader**, regardless of any SDK-side fix. The 1550 B linked perc5ive can't deploy until the 5ive team redeploys the on-chain loader with a post-`db5703d` build.

This is a platform-level dependency. We don't hold the upgrade authority for the loader (`EMoPytP7RY3JhCLtNwvZowMzgNNRLTF7FHuERjQ2wHFt`), so the redeploy has to come from the 5ive maintainers. Filed context in the five-sdk PR body above; follow-up outreach is a project concern, not a code concern.

### Current state summary

| Artifact | Current devnet state | After upstream VM upgrade |
|---|---|---|
| perc5ive engine | 463 B, sentinel-stubbed (handlers unreachable) | deploy 1550 B linked — all 9 Full-scope handlers execute |
| Markets (sov / pyth_race / lp_perp) | ≤283 B each, already-working single-shot Deploy | unchanged |
| PercolatorBench conformance (`bench/`) | 24 tests green — proves bytecode matches Percolator Rust bit-exactly | unchanged |
| Link correctness (`tests/devnet_reproducibility.rs`) | 6 tests green — proves the 1550 B output is well-formed VM-native with every sentinel rewritten | unchanged |

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
