# perc5ive-bench

Conformance + adversarial test harness for Percolator-compatible
implementations. Two top-level capabilities:

## 1. `bounty_fuzz` — three-way differential harness

Built in bounty hunt **Session 2**. Routes adversarial inputs through up
to three implementations of the Percolator engine and reports any
pairwise divergence.

| Leg | What | Status |
| --- | --- | --- |
| `RustRef` | `aeyakovenko/percolator` Rust reference library | always on |
| `Dsl` | 5ive DSL port (`perc5ive::bytecode`) | feature-gated `--features dsl_leg`, Session 3 work |
| `Bpf` | `aeyakovenko/percolator-prog` BPF wrapper via litesvm 0.11 | always on, requires `.so` |

### Building the BPF leg's `.so`

```bash
cd hello_slab/percolator-prog
cargo build-sbf
```

Produces `target/deploy/percolator_prog.so`. The runner picks it up from
the canonical path; override via the `PERCOLATOR_PROG_SO` env var if you
want to point at a different build.

### Running the harness

```bash
# OpenSSL pkg-config is required for litesvm 0.11's transitive deps.
export PKG_CONFIG_PATH=/home/linuxbrew/.linuxbrew/lib/pkgconfig

# Sanity check (must emit zero divergences — verifies the comparison logic).
cargo run --bin bounty_fuzz -- --target sanity --probes 1000

# Stub targets: T1-T4 from BOUNTY_HUNT_PLAN.md Session 1 output.
# These pass through clean in Session 2 — Session 3 replaces the stubs
# with real adversarial probe payloads.
cargo run --bin bounty_fuzz -- --target t1_funding      --probes 1000
cargo run --bin bounty_fuzz -- --target t2_conservation --probes 1000
cargo run --bin bounty_fuzz -- --target t3_margin       --probes 1000
cargo run --bin bounty_fuzz -- --target t4_riskbuffer   --probes 1000

# Verify the BPF leg actually loads. Errors with exit code 2 if the .so
# is missing — useful guardrail before a long hunt.
cargo run --bin bounty_fuzz -- --target t1_funding --probes 1 --require-bpf
```

Each run writes one JSONL file to `bench/fuzz_results/<unix_ts>_<target>.jsonl`,
one line per probe:

```json
{"probe_id":0,"divergences":[]}
{"probe_id":1,"divergences":[
  {"target":"t1_funding","field":"cum_funding_long_e18",
   "leg_a":"RustRef","leg_b":"Bpf","value_a":"...","value_b":"...",
   "note":"seed=0xC0FFEE i=1"}
]}
```

`cargo test --lib bounty_fuzz` runs the harness unit tests, including a
1000-probe sanity check.

### Adding a probe (Session 3 workflow)

1. Implement `Probe` from `bounty_fuzz::probe` for your target.
2. Register it in `bounty_fuzz::targets::run_target`.
3. The Session 1 targets (T1–T4) are already enum variants — replace the
   `StubProbe` branch with your implementation.

A probe's `run(probe_id, seed)` returns a `ProbeOutcome { probe_id,
divergences }`. Emit one `Divergence` per pairwise field mismatch; the
harness collapses these per-target and the triage scripts in Session 3
cluster by `(field, leg_a, leg_b, note)`.

## 2. Pre-mono conformance modules (legacy, feature-gated)

The crate's original purpose was bit-exact u256/i256/i128 conformance
between Anatoly's reference and our 5ive DSL bytecode. The mono port
dropped the multiprecision opcodes, so those modules
(`anatoly_conformance`, `arithmetic_conformance`) reference dead symbols
and are now behind `--features legacy_u256`. Don't compile them unless
the multiprecision DSL surface is reintroduced.

`field_access_conformance` and `handler_conformance` are doc-only
catalogs at HEAD; the actual round-trip tests they describe live in
`perc5ive/tests/`.

## Toolchain notes

- `litesvm = "0.11"` targets the Agave/Anza Solana 2.x runtime.
  Compatible with `.so` blobs built by `solana-cargo-build-sbf` 3.x
  (verified against percolator-prog @ `ba667e8c`).
- `solana-account = "3"`, `solana-address = "2"`, `solana-keypair = "2"`,
  `solana-signer = "2"` — split-crate Solana 2.x/3.x. Versions
  intentionally lockstep with litesvm 0.11's transitive deps to keep the
  `Account` / `Address` types out of the v1/v2 split-universe.
- `solana_pubkey::Pubkey` (returned by `Keypair::pubkey()`) and
  `solana_address::Address` (used by litesvm) are different wrapper types
  for the same 32-byte payload. `BpfRunner::payer_address()` bridges them.
