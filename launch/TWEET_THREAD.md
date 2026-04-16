# Perc5ive launch tweet thread

Post when Perc5ive ships to devnet + the MCP endpoint is live. Keep each tweet under 280 chars, no emoji.

**Tagging convention:** Do NOT @ Anatoly in thread-1 or any follow-up. The goal is that he finds the post organically (via the repo activity or a quote-retweet from someone else in the ecosystem). Cold @s have low reply rates and make the work look like pleading for attention. If Haidar ships a similar demo before us, cite `@HaidarIDK` in a dedicated credit tweet near the end of the thread — their wrapper is prior art and the ecosystem credit costs us nothing.

## Thread

### Tweet 1 — the hook

> we ported percolator to 5ive
>
> anatoly's perps engine is a 7,875-line rust library. no deployable program. we shipped it on-chain with three markets including sov (his own sketch) and an MCP server that lets you audit any of it live.
>
> github.com/[org]/perc5ive

### Tweet 2 — what's in the repo

> what actually shipped:
>
> 1. full port of wide_math + i128 + percolator.rs to 5ive DSL + hand-written bytecode for the u256 hotspots
> 2. three markets: sov (inverted memecoin perp), pyth race, lp perp
> 3. MCP server — AI assistants can query + simulate the whole thing
> 4. conformance bench vs anatoly's rust reference

### Tweet 3 — the VM work

> the VM itself needed two PRs:
>
> — 5iveVM/five-protocol#37 + five-vm-mito#84: u256/i128/i256 multiprecision opcodes (14 opcodes, 35 tests)
> — 5iveVM/five-protocol#38 + five-vm-mito#85: sized LOAD/STORE_FIELD for u128/u16, so packed account layouts round-trip correctly

### Tweet 4 — sov

> sov is the flagship market.
>
> inverted perp on a memecoin, denominated in the memecoin itself. admin key burned at init. insurance fund seeded + fed by fees. the trust-minimized version of what anatoly sketched in his April 2026 tweet.

### Tweet 5 — pyth race

> pyth race is a head-to-head on two pyth feeds.
>
> bet which asset outperforms between two slots. percolator's risk engine handles intra-race liquidations using the synthetic ratio oracle. winners take losers' stakes proportionally at resolve.

### Tweet 6 — lp perp

> lp perp is the hedge instrument LPs actually need.
>
> underlying is a 5ive AMM pool's reserve ratio. LPs open size equal to their share and IL stops compounding. speculators on the other side take directional bets on pool composition.

### Tweet 7 — the MCP moment

> the demo-day moment:
>
> ask claude "deploy a fresh sov market backed by $PENGU, burn the admin key, then show me what happens if PENGU drops 50%". it runs the full sequence — deploy, burn, simulate — and streams the explorer link. no UI required.

### Tweet 8 — conformance

> percolator bench is in /bench. it runs anatoly's own test vectors against both the rust reference AND the perc5ive bytecode. every property that passes on his stack passes on ours.
>
> if you fork percolator to ship a production program, our bench is your regression net.

### Tweet 9 — what's next

> queued for the next iteration:
>
> — MULDIV_REM_U256 opcode to finish wide_signed_mul_div_floor
> — pure-DSL handler bodies once the u128-rvalue compiler path lands
> — sov mainnet deploy with a real memecoin backer
> — kani-equivalent formal verification layer on the 5ive side

### Tweet 10 — the ask

> if you work on percolator, sov, or anything adjacent — @ us. the MCP endpoint is live on devnet right now, and it's the fastest way to iterate on a percolator fork we've seen.
>
> we're building 5ive to make this kind of port take a weekend, not a quarter.

---

## Reply / interaction strategy

* **If @aeyakovenko retweets or replies:** reply once with a thank-you and ONE concrete offer ("happy to open-source the bench PR against your repo if useful"). Don't ask for follow-up engagement. One beat, move on.

* **If @HaidarIDK engages:** reciprocate with a credit tweet: "`@HaidarIDK` shipped the original deployable percolator wrapper — we're a DSL layer on top, not a replacement. pointing at their repo in the readme."

* **If other Solana-perps accounts engage:** reply with the specific piece of Perc5ive that's relevant to their interest. Don't sprinkle the whole thread.

* **If the @pyth_network team engages on the Pyth Race tweet:** offer the race-market deployer addresses for them to inspect.

* **No engagement at all:** quote-retweet tweet 1 with "added: running conformance vs anatoly's Feb 2026 commit — bit-identical pass" after the first `cargo test` run against the latest upstream head. Pin that quote. No further boosts.

## Timing

Post thread at **Tuesday or Wednesday, 9-10am PT**. That's historically the highest-engagement window for `@aeyakovenko` and most of the Solana core dev account base.

Don't post on:
- Friday afternoon (end-of-week disengagement)
- Monday morning (inbox clearing)
- Any day an SOL price move exceeds 10% (finance twitter drowns everything else)
