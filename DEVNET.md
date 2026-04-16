# Devnet deployments

Live on Solana devnet as of 2026-04-16. All four perc5ive artifacts are deployed and on-chain — submission can link directly to these explorer pages.

| Artifact | Program ID | Initial deploy tx | Bytecode size |
|---|---|---|---|
| **perc5ive engine** | [`2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK`](https://explorer.solana.com/address/2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK?cluster=devnet) | [`2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4`](https://explorer.solana.com/tx/2ymT1oVELvLjSksgrgYQsxRar3V3HnZ85ummZcUBVHmCRhWyBAtPZ3Vypkg4aakAwm7czx9cDPXuxSVhqi2NkkA4?cluster=devnet) | 463 B |
| **Sov** (inverted memecoin perp) | [`2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV`](https://explorer.solana.com/address/2k6PjRKHbkBDQhaFxY4Fht2ZL3eEKcSh2GWJnbncuZJV?cluster=devnet) | [`3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje`](https://explorer.solana.com/tx/3egPALu7oy2WJMvspNDsg3iKjrQjtUM45KQGpjGoBHC8ganVK6YipQ4SGUJJCbhWHzL8URmYaLAfYRFDPrnKaje?cluster=devnet) | 281 B |
| **PythRaceMarket** | [`5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i`](https://explorer.solana.com/address/5vj6Mi2dYwgMSA6a8zyJFtEokRSu7T8FCpwVfDV8YV3i?cluster=devnet) | [`4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG`](https://explorer.solana.com/tx/4bhvatckNsjHobVBDHxXCGnkK4yENYjQgnz6LrfzD8DPAEAayRb7SdcCM4UaXguAUxUK7ePhNPWWv3ZD7BGdQ2UG?cluster=devnet) | 283 B |
| **LPPerp** | [`DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp`](https://explorer.solana.com/address/DevEEA1JcuQCQnqrb38SjKn3fEsxKQ3BjML7um6DH2Bp?cluster=devnet) | [`2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b`](https://explorer.solana.com/tx/2U3UGgkRezh2MuZ35kFBQearU5gE2ZUdt3xLG2y9TG4XkL9RzTipUL1h64UQNrnFtrFz3E6QYRHWWXJs5u9NP58b?cluster=devnet) | 266 B |

The perc5ive engine's VM-state PDA is `H5ykzUdetT5Lk81GHBe8Netejyw7t1spkN2ZehgRQZpp`.

## How to redeploy

The build script regenerates each `.fbin` on every `cargo build`; the wrapper renames to `.bin` (CLI-required extension) and shells out to `five deploy`:

```
cargo build                           # regenerate all four .fbin artifacts
scripts/deploy.sh perc5ive --target devnet
scripts/deploy.sh sov --target devnet
scripts/deploy.sh pyth_race --target devnet
scripts/deploy.sh lp_perp --target devnet
```

Pass `--dry-run` to simulate without spending SOL. The script reads `solana config get` for the default cluster + keypair; override with `--target` and `--keypair`.

## How to verify

```
solana account 2oRBYXUFxKb9AVP5aZTCoUKpgpU3318PvgM944gmh8VK --url devnet
```

Or open any of the explorer links above. Each program account holds the bytecode whose first four bytes are `5IVE`.

## Cost

All four deploys cost a combined ~0.014 SOL (rent-exempt minimums for 463/281/283/266-byte data accounts plus signature fees). Redeploys reuse the existing program accounts and are cheaper.
