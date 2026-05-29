# Perc5ive

**Percolator, ported to 5ive.** A Solana perpetual-futures stack that takes Anatoly Yakovenko's published [Percolator](https://github.com/aeyakovenko/percolator) risk engine — a pure-Rust research library with no deployable program — and ships it as a live DSL-based market factory on the [5ive VM](https://github.com/5iveVM), plus three markets built on top (Sov, PythRaceMarket, LPPerp), a conformance test suite (PercolatorBench), and an MCP server that lets AI assistants audit the whole system.

Submitted to **Colosseum Frontier 2026**.

## Status

| Component | State | Tests |
|---|---|---|
| `src/bytecode/{u256,i256,i128}.rs` — math primitives | full port vs. Anatoly's wide_math.rs, incl. `wide_signed_mul_div_floor` bytecode + `saturating_add_i256` runtime-rhs | 66 |
| `src/bytecode/handlers.rs` — instruction bodies | **9 of 9 handlers Full scope** (all spec-level invariants + body-relative JUMP guards) | 22 unit |
| `src/bytecode/link.rs` — linker | sentinel-rewrite pipeline + body-relative JUMP fix-up at append | 6 e2e |
| `src/bytecode/dsl_header.rs` — header normalizer | DSL→VM header conversion | — |
| `tests/e2e_integration.rs` — VM × AccountInfo | full pipeline with real pinocchio AccountInfo shim | 9 e2e |
| `tests/e2e_full_link.rs` — compiled main.v × all 9 handler bodies | build.rs output linked end-to-end | 4 e2e |
| `tests/devnet_reproducibility.rs` — cold-clone verification | artifact sizes match DEVNET.md advertised sizes | 5 e2e |
| `build.rs` — DSL build step | compiles `dsl/src/main.v` + three market binaries on every build | — |
| `dsl/src/main.v` — Percolator in 5ive DSL | full handler signatures; sentinel stubs wired to bytecode bodies | — |
| `meta/src/main.v` + `src/bytecode/meta_handlers.rs` — **MetaGenesis** | percolator-meta fair-launch layer ported to 5ive: 12 genesis-lifecycle handlers (deposit→kickstart→vote→mint→finalize→withdraw) executing end-to-end against the real linked binary | 8 e2e/unit |
| `markets/{sov,pyth_race,lp_perp}/src/main.v` | 3 thin DSL wrappers; Sov fair-launches through MetaGenesis (`tests/e2e_sov_genesis.rs`) | 1 e2e |
| `bench/` — PercolatorBench conformance harness | meta vote-weight bytecode-vs-reference + split/quorum/recovery/rent-zero-extraction conformance; 5 properties total (pre-mono u256 suite quarantined for the v16 rebase) | — |
| `mcp/` — MCP-Perc5ive stdio server | 24 tool catalogue; **10 simulation tools wired** incl. genesis/futarchy (vote weight, kickstart split, COIN distribution, lifecycle, rent audit) | 18 |

**The percolator-meta genesis lifecycle runs end-to-end against the real linked binary** (`tests/e2e_meta_genesis.rs`), and Sov fair-launches through it (`tests/e2e_sov_genesis.rs`). The 9 risk-engine handlers ship at Full scope **against v12.17** — every spec-level guard (OI bounds, fee cap, dust protection, free-collateral check, flat-position fee sweep, time monotonicity) enforced in bytecode via the body-relative JUMP infrastructure.

> **v16 conformance:** upstream rewrote the engine into a multi-asset `v16.rs` (171 commits ahead). The risk-handler conformance is calibrated to **v12.17** (tagged `v12.17-port-ref`); the v16 re-port is in progress with the delta analysis + blockers documented internally. See `SPEC.md` for the honest status. The genesis (meta) layer is current to percolator-meta `b6d5f2a`.

**Upstream VM work** (as of 2026-04-17): 7 open PRs extend `five-protocol` + `five-vm-mito` with the opcode set Percolator needs — `u256/i128/i256` arithmetic (#37/#84), sized field access (#38/#85), `input_data` u128 typed params (#86), and **MULDIV_REM_U256** (#39/#87) which surfaces the 512-bit remainder that `wide_signed_mul_div_floor` needs for floor-toward-(-∞) rounding.

## Why

Percolator is the most rigorous public perps-risk-engine spec anyone has shipped for Solana. Its README says *"EDUCATIONAL RESEARCH PROJECT — NOT PRODUCTION READY"* and the repo is `[lib]`-only — no entrypoint, no deployable program. Every team that wants to ship a Percolator-based perp faces the same multi-month translation project.

Perc5ive does that translation once, on 5ive, and ships the artifacts every downstream builder would need anyway:

- **The port itself** — u256 / i256 / i128 math plus the instruction state machine, bit-for-bit conformance against the upstream Rust.
- **Three reference markets** — Sov (an inverted memecoin perp matching Anatoly's April 2026 sketch), PythRaceMarket (head-to-head price race on Pyth feeds), LPPerp (AMM-LP hedge instrument).
- **PercolatorBench** — an open-source conformance suite. Run it against Anatoly's reference, against our port, against any future Percolator-derivative. Divergence = bug.
- **MCP-Perc5ive** — 24 tools exposing on-chain state and a simulation surface to any AI assistant that speaks MCP (Claude, Cursor, Anthropic Console, etc.). Ask *"simulate a 50% BONK drop on Sov"* and watch the cascade in your terminal.

## Repository layout

```
perc5ive/
├── src/bytecode/          # Rust-emitted bytecode for multiprecision hotspots
│   ├── emit.rs            # Program builder (VLE, push_u256, jumps, CALL, ...)
│   ├── link.rs            # Linker: append + sentinel-rewrite
│   ├── dsl_header.rs      # five-dsl-compiler ↔ VM header converter
│   ├── handlers.rs        # One body emitter per Percolator instruction
│   ├── u256.rs            # Rust refs + bytecode programs for u256 ops
│   ├── i256.rs            # Signed 256-bit ops
│   └── i128.rs            # BPF-safe i128 wrapper
├── build.rs               # Compiles dsl/src/main.v → target/perc5ive.fbin
├── tests/
│   ├── e2e_u256.rs        # Multiprecision VM conformance
│   ├── e2e_i256.rs        # Signed 256-bit
│   ├── e2e_i128.rs        # i128
│   ├── e2e_link.rs        # Linker append + rewrite
│   ├── e2e_real_dsl.rs    # DSL-compiled binary round-trip
│   ├── e2e_integration.rs # VM × AccountInfo × handlers (hand-written bytecode)
│   ├── e2e_full_link.rs   # Compiled main.v × all 9 sentinels linked end-to-end
│   └── common/mod.rs      # pinocchio 0.9.2 AccountInfo shim
├── dsl/src/main.v         # Percolator in 5ive DSL (sentinel-stubbed)
├── markets/
│   ├── sov/src/main.v     # Inverted memecoin perp
│   ├── pyth_race/src/main.v
│   └── lp_perp/src/main.v
├── bench/                 # PercolatorBench conformance harness
├── mcp/                   # MCP-Perc5ive tool catalogue + stdio server
├── scripts/               # deploy.sh + helper tooling
├── hello_slab/            # Anatoly's percolator as a reference (gitignored; clone separately)
├── SPEC.md                # Percolator spec v12.17.0 primitives
├── RESOURCES.md           # External references + tools
├── TOLY_CORPUS.md         # Canonical Anatoly quotes backing the thesis
├── MULTIPRECISION_DSL_DECISION.md  # Why Path 3 (hand-written bytecode)
├── MCP_PERCOLATOR.md      # MCP server spec
└── PERCOLATOR_BENCH.md    # Conformance suite spec
```

## Build

```
cargo test                           # run all perc5ive tests (regenerates .fbin via build.rs)
cd bench && cargo test               # run PercolatorBench (includes Anatoly-direct conformance)
cd mcp && cargo test                 # run MCP tool-schema + simulation handler tests
cd mcp && cargo run --bin mcp-perc5ive   # start the stdio MCP server
```

The full-link path — compiled main.v + all 9 hand-written handler bodies appended and sentinel-rewritten — is exercised by `tests/e2e_full_link.rs`. The devnet deployments advertised in `DEVNET.md` are byte-for-byte reproducible from a fresh clone; `tests/devnet_reproducibility.rs` asserts that invariant.

### Local dependencies

Perc5ive uses path dependencies on three sibling 5ive repos. Check them out alongside this directory:

```
5iveVM/
├── perc5ive/              # this repo
├── five-protocol/         # runtime dep — VM types and opcodes
├── five-vm-mito/          # runtime dep — MitoVM interpreter
└── five-dsl-compiler/     # build-time dep — compiles dsl/src/main.v
```

Clone each from `https://github.com/5iveVM/<name>.git`. The compiler is pinned against `main` at HEAD `3d92ade` — bumping that pin in `Cargo.toml` is how we pull in new compiler work.

The `hello_slab/percolator/` directory is a local clone of `aeyakovenko/percolator` used as a conformance oracle. It's gitignored; clone it separately:

```
git clone https://github.com/aeyakovenko/percolator.git hello_slab/percolator
```

## Acknowledgments

- [@aeyakovenko](https://github.com/aeyakovenko) — author of the Percolator spec + Rust reference
- [@HaidarIDK](https://github.com/HaidarIDK) — shipped the original Pinocchio-based Percolator program wrapper, which validated that the engine can be made deployable
- The 5ive team — DSL + VM work upstream

## License

MIT. See `LICENSE`.
