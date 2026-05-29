# MetaGenesis — rent audit + watermelon pool (design)

**Date:** 2026-05-29
**Status:** Phase 1 specified, Phase 2 sketched (gets its own spec)
**Trigger:** two Toly tweets, 2026-05-29.

Tweet 1 (`x.com/toly/status/2060351571540484250`):
> "Do you want a whole single grape or a slice of the watermelon? The perfect
> way to bootstrap a network should offer every participant a slice of the
> watermelon, so they chose to participate in network as [opposed] to trying to
> build their own thing. You must have minimal rent."

Tweet 2 (`x.com/toly/status/2060380455707476303`), quoted by Colosseum as
*"Request for startup by the Solana godfather himself"*:
> "Hint hint. Instead of all building your own thing, get a bunch of startups
> together that are all building perps, prediction markets, new oracles, prop
> amms, etc… bootstrap a Futarchy together that run common protocols.
> Percolator (baring any bugs) has formally verified isolation between markets.
> Anyone can create additional markets under one roof with guarantees that they
> can't wreck anyone else.
> Reduce the rents in it to be marginal and return all the rents to users. Then
> the actual value creation can capture real roi."

Tweet 2 makes the watermelon literal: a **shared futarchy** where many teams
build isolated markets under one roof, on common protocols, with **marginal
rents returned to users**. Crucially, the answer to contagion is **Percolator's
formally-verified market isolation** — *not* cross-collateralization. This is the
economic thesis behind `percolator-meta`, which we already ported to the 5ive
DSL (`meta/src/main.v`). The tweet maps onto our build on three axes:

| Tweet | Meaning | Where it lives |
|---|---|---|
| "slice of the watermelon, not a grape" | bootstrap via a shared stake, not isolated launches | genesis lifecycle in `meta/src/main.v` |
| "choose to participate vs build their own" | anti-fragmentation / network pull | market-factory framing, `docs-internal/launch/METAGENESIS_THREAD.md` |
| "you must have minimal rent" | the bootstrap layer extracts ~nothing | `meta/src/main.v:9-13` — *"No yield."* |

Three deliverables, **built in order** (audit first, it earns the others):
1. **Phase 1 — rent audit.** Prove on the executed bytecode that the genesis
   lifecycle extracts zero rent to any non-protocol key.
2. **Phase 2 — shared-futarchy market factory.** Let an established MetaDAO
   authorize *additional isolated* markets under one shared futarchy, with rents
   returned to users. Built on the proven rent=0 baseline.
3. **Phase 3 — launch / positioning.** A Toly-tagged thread positioning Perc5ive
   publicly as the answer to the RFS, framed as a continuation of the Frontier
   submission. Posting is **gated** on Phases 1-2 being real and on the honesty
   bounds below (esp. meta is NOT devnet-live yet).

---

## "Minimal rent" — the definition this design asserts

Toly's tweet 2 sharpens "minimal rent" to **"reduce the rents to marginal and
return all the rents to users."** We assert the strongest form of that: across
the whole genesis lifecycle **no value leaves to any non-protocol key**.
Deposits are recoverable pro-rata, the only accrual is time-weighted voting
power, and *if* a backing fee ever exists it can only recirculate into the
shared insurance vault — i.e. returned to users, never out to an operator or
founder. Zero operator rent is the limiting case of "marginal rent returned to
users."

Grounding in the reference (`hello_slab/percolator-meta/spec.md`):
- `:56-57` — `insurance = floor(total/2)`, `backing = total − insurance`.
- `:100-102` — "optional backing DAO fee routed to the **main insurance token
  vault**. Insurance vaults cannot charge a DAO fee. Backing vaults may route a
  futarchy-configured fee **to the main insurance vault**."

Our 5ive port (`meta/src/main.v`) implements only the **genesis lifecycle**
(`init_genesis_bootstrap`, `genesis_deposit`, `genesis_withdraw`,
`kickstart_genesis_market`, `init/vote_genesis_distribution`,
`genesis_mint_reward`, `finalize_genesis`, `draw_genesis_surplus`,
`recover_genesis_market`). The backing-DAO-fee routing is a *post-genesis
risk-vault* feature (`spec.md:83-102`) we did **not** port. So the genesis layer
we shipped is genuinely zero-fee, and the recirculating-fee path becomes the
phase-2 bridge rather than a current liability.

**Headline claim the audit proves:** rent to any operator/founder key = 0; the
only value flows are recoverable depositor principal and the in-network
insurance/backing pools.

---

## Phase 1 — rent audit

### Goal

Prove, on the executed bytecode (not just the reference math), that the genesis
lifecycle is zero-extraction, and surface it as both a conformance property and
an MCP tool.

### Components

#### 1. `tests/e2e_rent_audit.rs` (new) — the proof

A ledger-conservation e2e on the real VM. Drives the full lifecycle:

```
init_genesis_bootstrap
  → genesis_deposit × N
  → kickstart_genesis_market
  → init_genesis_distribution / vote_genesis_distribution
  → genesis_mint_reward
  → finalize_genesis
exit paths: genesis_withdraw, draw_genesis_surplus, recover_genesis_market
```

After **every** handler, assert two **conservation invariants** on separate
ledgers (base units vs the minted COIN — `genesis_mint_reward` mints COIN, it
does not consume base units, so they are tracked independently):

```
base units:  total_deposited == sum(recoverable_principal) + insurance + backing
COIN supply: minted_supply == reward_supply        (matches meta_conformance.rs:245)
```

plus three **no-sink** assertions:
- (a) no handler moves base units to an address that isn't the genesis vault,
  the insurance vault, the backing market, or a depositor's recoverable claim;
- (b) `draw_genesis_surplus(amount)` reverts when `amount > vault_balance`;
- (c) `recover_genesis_market` only credits `genesis_vault`.

The **operator-rent figure** is computed as
`total_in − (recoverable + in_network_pools)` and asserted `== 0`.

Pattern: extend the existing `tests/e2e_meta_genesis.rs` path; reuse its VM
harness and account setup.

#### 2. `meta.rent_audit` MCP tool — the surface

Fifth simulation tool, alongside the existing four (vote weight, kickstart
split, COIN distribution, lifecycle) in `mcp/src/tools.rs` + `mcp/src/bin/server.rs`.

- **Input:** a lifecycle scenario — deposits `[u64]`, optional loss factor,
  optional surplus draw.
- **Output:** a per-step value-flow table + headline `operator_rent: 0` and an
  `in_network_recirculation` figure.

Mirrors the shape/registration of the existing four tools.

#### 3. `bench/src/meta_conformance.rs` (extend) — the report line

Add a `rent_zero_extraction` property to the `ConformanceReport` that runs the
conservation checks over a probe set, so `PercolatorBench` shows it green
alongside the existing four meta properties (vote weight, kickstart split,
quorum, recovery). Add a unit test mirroring `meta_conformance_run_is_all_green`.

### Data flow

The e2e is the **proof** (real VM, real bytecode). The bench property and the
MCP tool both reuse the same conservation arithmetic from
`src/bytecode/meta_math.rs` — add a `rent_breakdown(scenario)` helper there — so
all three agree by construction.

### Testing

- The e2e *is* the test; must be all-green.
- The bench property gets a unit test (`rent_audit_is_zero_extraction` style).
- MCP tool: a smoke test that the tool returns `operator_rent == 0` for a
  representative scenario.

---

## Phase 2 — shared-futarchy market factory

Built on the Phase-1 rent=0 baseline. **Revised after Toly tweet 2** — the
earlier "mutualized insurance pool" reading is rejected (see below).

### The turn

Today our build runs **one genesis → one COIN → one MetaDAO → one market** — the
"bigger grape." The watermelon, per tweet 2: a **shared futarchy** under which
*many* teams launch *isolated* markets (perps, prediction markets, oracles,
prop-amms) on common protocols. The "slice of the watermelon" is exposure to the
**shared futarchy / COIN / rent-return**, not to a shared risk pool. Teams choose
to launch under the common roof instead of building their own thing because the
roof gives them governance, common protocols, and marginal-rent economics they
can't bootstrap alone.

### Rejected: mutualized insurance pool

An earlier sketch proposed pointing every market's insurance leg at one shared
PDA pool. **Tweet 2 rules this out:** cross-collateralizing markets reintroduces
exactly the contagion Toly says to avoid ("guarantees they can't wreck anyone
else"). The contagion answer is **Percolator's formally-verified per-market
isolation**, not mutualization. Markets stay isolated; only the futarchy layer is
shared. Our current design already keeps markets in separate Percolator markets
under separate PDAs — isolation is preserved by construction.

### Likely changes (to be specified later)

- **Multi-market factory under one MetaDAO:** `init_percolator_market` /
  `approve_builder` extended so an established MetaDAO can authorize *additional*
  isolated markets under the same futarchy/COIN, not just the one born at
  kickstart.
- **Rent-return accounting:** any per-market fee routes back to COIN
  holders/users (Toly's "return all the rents to users"), with Phase 1's
  zero-operator-rent invariant extended to each new market.
- New e2e: two isolated markets under one shared futarchy, prove (a) a fault in
  market A cannot draw down market B (isolation), and (b) rents from both return
  to users, operator rent stays 0.

### Resolved decisions (defaults, confirm at plan review)

These were open questions; resolved with Toly-tweet-aligned defaults so Phase 2
is buildable now. Flagged for confirmation during plan review.

1. **What a "slice of the watermelon" entitles** → **governance weight in the
   shared futarchy + a share of aggregated rent-return.** Not cross-market risk
   exposure (rejected above — isolation, not mutualization).
2. **Builder onboarding** → an established MetaDAO admits a new builder via a
   **futarchy vote** that calls `approve_builder`; the approved builder then
   calls `init_percolator_market` for its own **isolated** market under the
   shared COIN. Reuses handlers we already ported (`approve_builder`,
   `init_percolator_market`, `percolator_admin`).
3. **Rent invariant per market** → Phase 1's zero-operator-rent proof is extended
   to *every* factory-created market; the Phase-2 e2e asserts it for a second
   market, not just the genesis-born one.

### Build surface

- Extend `approve_builder` / `init_percolator_market` so they work post-finalize
  under an existing MetaDAO (today they fire at/around kickstart for the single
  genesis market).
- Rent-return accounting: any per-market fee routes to COIN holders/users; no
  operator sink (extends `meta_math::rent_breakdown`).
- New e2e `tests/e2e_market_factory.rs`: one MetaDAO, two isolated markets, prove
  (a) a fault in market A cannot draw down market B, (b) operator rent stays 0
  across both.

---

## Phase 3 — launch / positioning (Toly-tagged RFS-answer thread)

Position Perc5ive publicly as the answer to the RFS, framed as a continuation of
the Frontier hackathon submission, tagging Toly as a direct answer to today's
tweets. **Posting is gated** on Phases 1-2 landing and on the honesty bounds.

### Honesty bounds (hard) — what we may and may not claim

| May claim (true) | May NOT claim |
|---|---|
| percolator-meta genesis lifecycle ported to 5ive DSL | meta is "live on devnet" (it is not — `DEVNET.md:36`, ID pending mono wave) |
| conformance: vote-weight bit-exact, kickstart split, quorum, recovery | any market is running real user funds |
| full lifecycle passes e2e against the **real linked binary** | that we built Percolator / percolator-meta (we built the *implementation*) |
| **rent audit: operator_rent = 0, proven on the VM** | "formally verified" of *our* code (that is Toly's claim about upstream Percolator) |
| shared-futarchy factory: 2 isolated markets, rent-return, proven in e2e | devnet-live for the factory |

The engine + 3 markets *do* have devnet IDs (pre-mono, 2026-04-17) and may be
linked as prior evidence, labeled honestly as pre-mono.

### Thread shape (draft — final copy written after the build, saved to `docs-internal/launch/RFS_ANSWER_THREAD.md`)

Voice per `METAGENESIS_THREAD.md`: technical, concise, Perc5ive = implementation,
🦞 only emoji, tag Toly once (this is the milestone). Sketch:

1. Quote Toly's RFS tweet. "We've been building this since Frontier — percolator
   and percolator-meta, ported to the 5ive DSL. Here's where it is."
2. The fair-launch/genesis lifecycle in 5ive: bond → time-weighted vote → mint →
   MetaDAO. No yield. (link repo)
3. "Reduce rents to marginal, return to users" — we measured it: **operator rent
   = 0**, proven on the executed VM bytecode, exposed as an MCP audit tool.
4. "Markets under one roof that can't wreck each other" — isolated markets under
   one shared futarchy; e2e proves a fault in A can't draw down B.
5. Conformance gift: vote-weight bit-exact vs reference across every log2 bucket;
   kickstart split / quorum / recovery all green in PercolatorBench.
6. Honest status: ported + conformance + e2e against the real linked binary;
   devnet deploy of meta is pending our mono redeploy wave. Engine + 3 markets
   already on devnet (pre-mono).
7. Credit: Percolator + percolator-meta are @aeyakovenko's design; @MetaDAOProject
   for the futarchy. Perc5ive is the 5ive-DSL implementation. Repo: <link>. 🦞

### Process gate

I **draft** the thread; I do **not** post it. Posting is the user's explicit
action (X posting is outward-facing and irreversible). Best window per
`TOLY_STRATEGY`: Tue/Wed 9-11am PT.

## Strategy fit

- **This is now an RFS.** Colosseum quoted tweet 2 as "Request for startup by
  the Solana godfather himself." Perc5ive (a Colosseum Frontier submission) is
  already a `percolator-meta`-on-5ive port — i.e. we are building the thing the
  RFS describes. The rent audit + shared-futarchy factory are the two pieces
  that close the gap between "we ported it" and "we built the RFS."
- **Toly strategy** (`docs-internal/TOLY_STRATEGY.md`): Tier-1 "direct proof
  rather than appeals." The rent audit answers "reduce rents to marginal, return
  to users" with a measured `operator_rent = 0`, not a claim. No overclaim — the
  audit covers exactly the lifecycle we ported. Outreach still follows the
  playbook (tag at most once per milestone, proof over appeals).
- **Honesty protocol** (`docs-internal/CLAUDE.md`): Phase 2 is labeled a
  Perc5ive extension, never as upstream design. Percolator's "formally verified
  isolation" is Toly's claim about upstream, not ours to assert.
- **Story arc:** "rents are marginal and returned to users today (operator rent
  = 0, proven on the VM) — here's how many teams build isolated markets under
  one shared futarchy." Phase 1 earns the right to make the Phase 2 claim.
