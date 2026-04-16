# Perc5ive

**Percolator, ported to 5ive.** A Solana perpetual-futures stack that takes Anatoly Yakovenko's published [Percolator](https://github.com/aeyakovenko/percolator) risk engine — a pure-Rust research library with no deployable program — and ships it as a live DSL-based market factory on the [5ive VM](https://github.com/5iveVM), plus three markets built on top (Sov, PythRaceMarket, LPPerp), a conformance test suite (PercolatorBench), and an MCP server that lets AI assistants audit the whole system.

Submitted to **Colosseum Frontier 2026**.

## Status

| Component | State | Tests |
|---|---|---|
| `src/bytecode/{u256,i256,i128}.rs` — math primitives | full port vs. Anatoly's wide_math.rs | 52 conformance |
| `src/bytecode/handlers.rs` — instruction bodies | 9 handlers (5 full, 4 simplified) | 10 unit |
| `src/bytecode/link.rs` — linker | sentinel-rewrite pipeline | 6 e2e |
| `src/bytecode/dsl_header.rs` — header normalizer | DSL→VM header conversion | 8 e2e |
| `tests/e2e_integration.rs` — VM × AccountInfo | full pipeline with real pinocchio AccountInfo shim | 9 e2e |
| `dsl/src/main.v` — Percolator in 5ive DSL | signatures + sentinel stubs | — |
| `markets/{sov,pyth_race,lp_perp}/src/main.v` | 3 thin DSL wrappers | — |
| `bench/` — PercolatorBench conformance harness | crate scaffold, 11 tests | 11 |
| `mcp/` — MCP-Perc5ive tool catalogue | 19 tool schemas, transport pending | 4 |

**141 tests green across 3 crates.**

## Why

Percolator is the most rigorous public perps-risk-engine spec anyone has shipped for Solana. Its README says *"EDUCATIONAL RESEARCH PROJECT — NOT PRODUCTION READY"* and the repo is `[lib]`-only — no entrypoint, no deployable program. Every team that wants to ship a Percolator-based perp faces the same multi-month translation project.

Perc5ive does that translation once, on 5ive, and ships the artifacts every downstream builder would need anyway:

- **The port itself** — u256 / i256 / i128 math plus the instruction state machine, bit-for-bit conformance against the upstream Rust.
- **Three reference markets** — Sov (an inverted memecoin perp matching Anatoly's April 2026 sketch), PythRaceMarket (head-to-head price race on Pyth feeds), LPPerp (AMM-LP hedge instrument).
- **PercolatorBench** — an open-source conformance suite. Run it against Anatoly's reference, against our port, against any future Percolator-derivative. Divergence = bug.
- **MCP-Perc5ive** — 19 tools exposing on-chain state and a simulation surface to any AI assistant that speaks MCP (Claude, Cursor, Anthropic Console, etc.). Ask *"simulate a 50% BONK drop on Sov"* and watch the cascade in your terminal.

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
├── tests/
│   ├── e2e_u256.rs        # Multiprecision VM conformance
│   ├── e2e_i256.rs        # Signed 256-bit
│   ├── e2e_i128.rs        # i128
│   ├── e2e_link.rs        # Linker append + rewrite
│   ├── e2e_real_dsl.rs    # DSL-compiled binary round-trip
│   ├── e2e_integration.rs # VM × AccountInfo × handlers
│   └── common/mod.rs      # pinocchio 0.9.2 AccountInfo shim
├── dsl/src/main.v         # Percolator in 5ive DSL (sentinel-stubbed)
├── markets/
│   ├── sov/src/main.v     # Inverted memecoin perp
│   ├── pyth_race/src/main.v
│   └── lp_perp/src/main.v
├── bench/                 # PercolatorBench conformance harness
├── mcp/                   # MCP-Perc5ive tool catalogue
├── launch/                # Demo script + launch tweet thread
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
cargo test                           # run all perc5ive tests
cd bench && cargo test               # run PercolatorBench
cd mcp && cargo test                 # run MCP tool-schema tests
```

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
