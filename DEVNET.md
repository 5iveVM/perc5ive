# Devnet deployments

Live on Solana devnet. Program IDs are stable across redeploys; the underlying bytecode is upgraded in place.

> **⚠️ Stale as of 2026-04-17** — the currently-deployed perc5ive engine is the *pre-linked* 463-byte binary (sentinel-stubbed handlers). The linked binary including all 9 hand-written handler bodies is **1550 B** and needs a redeploy before the on-chain program can execute deposit / withdraw / settle / execute_trade / liquidate. Markets (sov / pyth_race / lp_perp) have no sentinels and don't need relinking. See "How to redeploy" below.

| Artifact | Program ID | Initial deploy tx | Current bytecode size | After relink (needs redeploy) |
|---|---|---|---|---|
| **perc5ive engine** | [`2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK`](https://explorer.solana.com/address/2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK?cluster=devnet) | [`2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4`](https://explorer.solana.com/tx/2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4?cluster=devnet) | 463 B (stale, unlinked) | **1550 B (linked)** |
| **Sov** (inverted memecoin perp) | [`2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV`](https://explorer.solana.com/address/2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV?cluster=devnet) | [`3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje`](https://explorer.solana.com/tx/3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje?cluster=devnet) | 281 B | 281 B (no change) |
| **PythRaceMarket** | [`5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i`](https://explorer.solana.com/address/5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i?cluster=devnet) | [`4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG`](https://explorer.solana.com/tx/4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG?cluster=devnet) | 283 B | 283 B (no change) |
| **LPPerp** | [`DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp`](https://explorer.solana.com/address/DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp?cluster=devnet) | [`2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b`](https://explorer.solana.com/tx/2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b?cluster=devnet) | 266 B | 266 B (no change) |

The perc5ive engine's VM-state PDA is `H5ykzUdetT5Lk81GHBe8Netejyw7t1spkN2ZehgRQZpp`.

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
