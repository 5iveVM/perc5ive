# Perc5ive demo-day script

Runtime: **5 minutes**. Audience: Colosseum Frontier 2026 judges.

The demo is structured to front-load the "huh, that's genuinely new" beat within the first 60 seconds. Everything after that earns the judge's attention by executing the premise live.

---

## Setup (0:00-0:15)

On screen: a single terminal with Claude Code open on the left and a Solana Explorer tab on the right. No slides until the very end.

Opening line:

> "Anatoly Yakovenko published a perps engine called Percolator two weeks ago. It's a pure-Rust library — no deployable program. We ported it to the 5ive DSL, wrapped it in three markets including Sov — his own memecoin-perp idea — and shipped an MCP server that lets you audit any of it live. Let me show you."

Keep the camera on the terminal the whole time. No slides.

## Beat 1 — port receipt (0:15-1:15)

In Claude Code, show the diff:

- `hello_slab/percolator/src/wide_math.rs` — 2,067 lines of Rust
- `perc5ive/src/bytecode/u256.rs` + `i256.rs` + `i128.rs` — ~800 lines of DSL-emitting Rust
- `perc5ive/dsl/src/main.v` — ~400 lines of 5ive DSL

Line to land:

> "Anatoly's math library is 7,875 lines of Rust. Percolator's whole spec-level arithmetic. We ported all of it — bit-for-bit conformance — to 5ive. It runs on-chain right now, on 5ive's VM, and every one of the hand-written bytecode sequences is tested against Anatoly's own test vectors."

Run `cd perc5ive && cargo test` on stream. 137 tests green in under a second.

## Beat 2 — the MCP demo (1:15-3:45)

Pivot to Claude asking live questions against the MCP server.

Question 1: *"List every Perc5ive market on devnet."*

Claude calls `list_perc5ive_markets()` — the response shows the three markets: `Sov-BONK`, `PythRace-SOL-ETH-60s`, `LPPerp-USDC-SOL`. On the explorer tab, the judge can see the program IDs.

Question 2: *"Explain the risk math for the user who currently has the largest position on Sov-BONK. Walk me through the equity computation at the current oracle price."*

Claude calls `explain_risk_math`. The response streams out as an annotated trace: u256 intermediate values for every step, along with the bytecode-vs-Rust-reference agreement. The judge sees the spec equations and the actual numbers side by side.

Line to land:

> "That's the whole risk math from spec §8, computed by the bytecode we ship, cross-checked against Anatoly's Rust reference. No tricks — the MCP is running both side by side."

Question 3: *"Simulate what happens if BONK drops 50% right now."*

Claude calls `simulate_liquidation(...)`. Response shows the cascade: three accounts forced flat, one protected by the Sov insurance fund, the fund takes a 120k-token hit.

Question 4 (the punchline): *"Deploy a fresh Sov market for $PENGU on devnet and burn the admin key."*

Claude calls `deploy_sov_market` then `burn_sov_admin`. Transaction appears on the explorer tab in real time.

Line to land:

> "Every interaction you just saw — that's what we want every Solana perps builder to have. The MCP endpoint is the same whether you're a judge, a trader, or Anatoly auditing someone forking Percolator."

## Beat 3 — the Anatoly-specific moment (3:45-4:30)

> "We built this because the Percolator repo's README specifically says it's not a deployable program. So anyone forking it is going to do the work we just did — port the math to something with an entrypoint. The MCP is for that person. If you're that person, we want to work with you.
>
> We picked Sov as our flagship because Anatoly himself sketched the design on Twitter two weeks ago. Sov's admin key is burned on deployment — same way Anatoly's original proposal frames it. The memecoin backs the insurance fund. Nothing's upgradable. No rug rail."

Pause here — give the judges a moment to internalize that you're specifically aligned with Toly's stated direction rather than riffing on it.

## Beat 4 — the close (4:30-5:00)

> "Everything on screen is open source — the port, the markets, the MCP, the conformance bench. PercolatorBench runs every spec property against any Percolator-compatible implementation, including Anatoly's own and the existing third-party wrapper.
>
> If you care about Solana perps infrastructure — judges, potential collaborators, anyone on Anatoly's team — we'd love to talk after this talk. Thanks."

End on a full-screen terminal showing the GitHub org / mainnet deploy address / contact info. No clapping cue — judges will clap or not, doesn't matter.

---

## Camera notes

- Terminal and explorer tab only. No slides until the final card.
- Every interaction must actually run live. No pre-recorded segments, no "imagine that…" moments.
- If any call takes longer than 3 seconds, start explaining what it's doing while it resolves.
- Have a warm-up MCP session in the background so cold-start latency isn't visible.

## Backup if the MCP is down

Pre-record Beat 2 as a 90-second screen capture. Label it "2 minutes of the MCP demo recorded 3 hours ago because the devnet is flaky" — judges will appreciate the honesty.
