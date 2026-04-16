# Multiprecision DSL surface — decision and deferred work

**Date:** 2026-04-15
**Status:** Interim decision; permanent fix deferred post-hackathon.

## Context

After shipping the full multiprecision opcode range in the VM
([five-protocol#37](https://github.com/5iveVM/five-protocol/pull/37),
[five-vm-mito#84](https://github.com/5iveVM/five-vm-mito/pull/84)) — u256
+ i128 + i256 across 14 opcodes (0xC0-0xCD) with 35 unit tests — we
investigated the compiler-side work needed to expose these opcodes to 5ive
DSL programs.

Three architectural paths emerged, each with real cost.

## The three paths considered

### Path 1 — intrinsics operate on account fields by name

Signatures like `u256_add(acct.a, acct.b, acct.result)`. Compiler resolves
field refs and emits a load→op→store sequence for each call.

- **Pros:** Fits Percolator's BYOM pattern (all risk state lives in accounts).
- **Cons:** Requires compiler to understand "this field is a u256 = 4 u64s
  wide" at field-access codegen. Hits the HIGH-severity offset-calculation
  bugs flagged in
  `five-dsl-compiler/src/bytecode_generator/ast_generator/KNOWN_LIMITATIONS.md`.
- **Estimated effort:** 1-2 engineer-days plus compiler test sweep.

### Path 2 — `ValueRef::U256` on the VM stack

Treat u256 as a single tagged stack cell the way u128 is today
(`ValueRef::U128([u64; 2])`). Drops our specialized 0xC0-0xCD opcodes in
favor of the existing polymorphic dispatch in `handlers/arithmetic.rs`.

- **Pros:** Uniform with u128's story.
- **Cons:** Enlarges every stack cell (even programs that never touch u256
  pay the memory cost), and every handler that cares about value widths needs
  a sweep. Largest regression surface.
- **Estimated effort:** 2-3 engineer-days plus full mito regression run.

### Path 3 — hand-written bytecode for u256 hotspots, DSL for the rest ✅

Most of Percolator's code is u128-range logic that compiles from DSL today.
The u256 math is concentrated in `wide_math.rs` and a handful of haircut-
ratio computations (~100 Rust lines, estimated ~50-150 lines of bytecode).

We write those hotspots as **hand-crafted bytecode modules** that call the
new multiprecision opcodes directly, and have the DSL call into them via
normal function-call bytecode. The rest of Percolator ports to DSL
conventionally.

- **Pros:** Unblocks Step 3 today. No compiler risk for the hackathon
  deadline. Lets us discover the real u256 shapes in Percolator before
  committing to a compiler surface design.
- **Cons:** Two source languages in one program. The hand-written bytecode
  modules won't be reviewable by reading DSL. Documented technical debt.
- **Estimated effort:** Integrated into the Percolator port timeline; no
  separate compiler PR.

## Decision — 2026-04-15

**Path 3 for the hackathon.** Permanent compiler surface is **deferred** until
after the Colosseum Frontier 2026 submission, with direction to be decided
once we see the real u256 shapes the port actually uses.

User direction: "3 for now, document for more permanent solution later."

## What the permanent solution needs to do

Captured for the post-hackathon follow-up. Any future compiler PR should:

1. **Express multi-word stack values** cleanly. Today, u128 sits as a single
   tagged cell; this doesn't extend to u256 without inflating every stack
   cell. Either switch to a uniform "N u64 slots" representation or add a
   new cell variant carefully.

2. **Solve multi-return** at least for built-in intrinsics. A function that
   pops 8 u64s (two u256s) and pushes 4 u64s (one u256) cannot be called from
   DSL today because DSL functions return exactly one value. Options are
   tuple-returns, out-params, or "side-effecting" statements that write to
   a bound variable.

3. **Fix the struct-local codegen gap.** `AstNode::StructLiteral` parses but
   has no production usage in the 50-protocol migration set and likely fails
   at bytecode emission for locals. Account struct fields work because they
   live in account memory, not on the stack.

4. **Avoid the KNOWN_LIMITATIONS offset heuristics** when adding multi-word
   fields. The current field-offset calculation assumes each field is 1-8
   bytes; 32-byte u256 fields would trigger the flagged edge case.

## Where the hand-written bytecode will live

Under `perc5ive/src/bytecode/` (to be created during Step 3). Each module is
a self-contained `.rs` file that emits a specific sequence of opcodes using
the `five_protocol` opcode constants, exposing a Rust-level function that
returns `Vec<u8>` for the compiler to stitch into the final `.five` binary.

Naming convention:
- `muldiv_haircut.rs` — the `(position * haircut_ratio) / scale` chain
- `u256_scale_up.rs` — u128 × scale → u256 promotion helper
- etc., named after the Percolator function they back

Each module gets a doc comment linking to the Rust source it's translating
and the Rust test vector(s) it must pass.

## References

- VM primitives: `five-vm-mito/src/handlers/multiprecision.rs`,
  `five-vm-mito/src/multiprecision_math.rs`
- Opcode constants: `five-protocol/src/opcodes.rs` (0xC0-0xCD)
- Compiler edit sites (for the future permanent fix):
  - Intrinsic table: `five-dsl-compiler/src/bytecode_generator/ast_generator/functions.rs:140-269`
  - Type system: `five-dsl-compiler/src/type_checker/type_helpers.rs`,
    `.../validation.rs`, `src/tokenizer.rs:1067`, `src/five_file.rs:14-23`
  - Known limitations:
    `five-dsl-compiler/src/bytecode_generator/ast_generator/KNOWN_LIMITATIONS.md`
