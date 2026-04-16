# RESOURCES — external references, tools, and links

Everything you need pointers to, all in one place.

---

## Anatoly's Percolator ecosystem

### Primary repo
- [`github.com/aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator) — 530 stars, 154 forks, Apache-2.0, 851 commits as of 2026-04-15
- [`github.com/aeyakovenko/percolator/blob/master/spec.md`](https://github.com/aeyakovenko/percolator/blob/master/spec.md) — Percolator risk-engine spec v12.17.0
- [`github.com/aeyakovenko/percolator/blob/master/plan.md`](https://github.com/aeyakovenko/percolator/blob/master/plan.md) — phase tracking
- [`github.com/aeyakovenko/percolator/blob/master/KITCHEN_SINK_TEST.md`](https://github.com/aeyakovenko/percolator/blob/master/KITCHEN_SINK_TEST.md) — comprehensive E2E test
- [`github.com/aeyakovenko/percolator/issues`](https://github.com/aeyakovenko/percolator/issues) — open issues + PRs
- [`github.com/aeyakovenko/percolator-cli`](https://github.com/aeyakovenko/percolator-cli) — separate CLI tool

### Ecosystem forks and related
- [`github.com/HaidarIDK/PERColator`](https://github.com/HaidarIDK/PERColator) — Web Version + CLI wrapper, tracked as of 2026-04-15
- `dex.percolator.site` — HaidarIDK's live web version
- [`x.com/percolator_fun`](https://x.com/percolator_fun) — active Twitter account

### Academic foundation
- Tarun Chitra autodeleveraging paper: [arXiv `2512.01112`](https://arxiv.org/abs/2512.01112)

### Anatoly's Twitter
- [`x.com/aeyakovenko`](https://x.com/aeyakovenko) — main account (he uses @toly alias occasionally)
- Tweet `2019890554436481243` — "basically feature complete" (April 2026) — the thesis-correcting update
- Tweet `2020173049094619222` — sov concept retweet

---

## 5ive ecosystem (internal)

### Main repos (in `/home/marche/5iveVM/`)
- `five-dsl-compiler/` — the DSL compiler
- `five-vm-mito/` — the VM implementation
- `five-mcp/` — MCP server for AI-native contract authoring
- `five-lsp/` — language server protocol for IDE integration
- `five-cli/` — command-line tooling
- `five-sdk/` — developer SDK
- `five-templates/` — project scaffolds
- `five-docs/` — Docusaurus-based documentation
- `five-solana/` — Solana-specific primitives + CPI support
- `five-dex-protocol/` — Five DEX Protocol (mainnet-deployed, 8,810 bytes)
- `5ive-amm/`, `5ive-cfd/`, `5ive-escrow/`, `5ive-lending/`, `5ive-payment-channel/`, etc. — 15+ primitive implementations

### Key internal docs
- `/home/marche/5iveVM/PROPOSAL_MULTICHAIN_EXPANSION.md` — multichain expansion strategy
- `/home/marche/5iveVM/SVM_CHAINS_ANALYSIS.md` — SVM-chain analysis
- `/home/marche/5iveVM/five-dsl-compiler/POTENTIAL_ISSUES.md` — known DSL compiler issues

### Deploy5 (status: awaiting launch per proposal)
- One-click deployment UI for 5ive-compiled programs

---

## Solana ecosystem references

### Foundation + core
- [`solana.com/news/solana-ecosystem-security`](https://solana.com/news/solana-ecosystem-security) — STRIDE / SIRN launch (April 7, 2026)
- [`solana.com/news/solana-attestation-service`](https://solana.com/news/solana-attestation-service) — SAS launch
- [`solana.com/news/solana-fireblocks-institutional-treasury-infrastructure`](https://solana.com/news/solana-fireblocks-institutional-treasury-infrastructure) — Fireblocks native integration
- [`github.com/solana-foundation/solana-improvement-documents`](https://github.com/solana-foundation/solana-improvement-documents) — SIMDs
  - SIMD-0297 — invalid nonces become transaction failures
  - SIMD-0301 — async execution activation switch
  - SIMD-415 — Nonce Payload Transaction Format
  - SIMD-456 — Nonce Replacement Meta-Proposal
  - SIMD-513 — Calldata/EIP-712 digest equivalent (Cyfrin)
  - SIMD-0296 — larger transactions (4KB)
- [`forum.solana.com/t/post-deployment-monitoring-tooling/1031`](https://forum.solana.com/t/post-deployment-monitoring-tooling/1031) — Foundation RFP for observability

### Infrastructure + tooling
- [`helius.dev`](https://helius.dev) — RPC + indexing + developer tools
- [`anza.xyz`](https://anza.xyz) — Jump Crypto's Solana client (Firedancer)
- [`jito.network`](https://jito.network) — MEV + bundle infrastructure
- [`pyth.network`](https://pyth.network) — oracle (primary)
- [`switchboard.xyz`](https://switchboard.xyz) — oracle (alternative)
- [`magicblock.gg`](https://magicblock.gg) — ephemeral rollups
- [`surfpool.run`](https://surfpool.run) — local Solana development environment (Anatoly uses this)
- [`anchor-lang.com`](https://anchor-lang.com) — competing smart contract framework

### Privacy + confidential computing
- [`arcium.com`](https://arcium.com) — MPC on Solana (Fortress DePIN vertical)
- [`docs.arcium.com`](https://docs.arcium.com) — Arcium integration docs

### Multisig + treasury
- [`squads.so`](https://squads.so) — dominant Solana multisig ($10B+ secured)
- [`docs.squads.so`](https://docs.squads.so) — Squads v5 advanced security docs
- [`realms.today`](https://realms.today) — Realms DAO tooling

### Competing perp DEXs
- [`drift.trade`](https://drift.trade) — Drift (recent hack victim)
- [`docs.drift.trade`](https://docs.drift.trade) — Drift documentation
- [`jupiter.exchange`](https://jupiter.exchange) — Jupiter
- [`zeta.markets`](https://zeta.markets) — Zeta
- [`pacifica.fi`](https://pacifica.fi) — Pacifica
- Hyperliquid (competing chain) — [`hyperliquid.xyz`](https://hyperliquid.xyz)
- Aster (competing chain) — [`asterdex.com`](https://asterdex.com)

---

## Colosseum hackathon

### Frontier 2026
- [`colosseum.com/frontier`](https://colosseum.com/frontier) — event page
- [`colosseum.com/hackathon`](https://colosseum.com/hackathon) — hackathon overview
- Dates: Apr 6 – May 11, 2026
- Prizes: $30K Grand Champion, 20× $10K runners-up, $10K university, $10K public goods
- Accelerator: ~11 teams receive $250K pre-seed

### Past winners to study
- Unruggable (Breakout, 3rd Infrastructure PRIZE + Accelerator C4) — hardware wallet with transaction introspection
- Umbra (Breakout PRIZE) — confidential computing on Solana
- Gecko Fuzz (Renaissance PRIZE) — decentralized fuzzing
- Excalead (Cypherpunk PRIZE) — AI audits + formal verification
- VitalFi (Cypherpunk PRIZE) — BR medical receivables RWA
- MCPay (Cypherpunk PRIZE + Accelerator) — x402 MCP tools monetization
- CORBITS.DEV (Cypherpunk PRIZE) — x402 merchant dashboard
- Cambrian (Renaissance PRIZE) — restaking layer
- Repl (Renaissance Honorable Mention) — DePIN trust layer
- Autonom (Cypherpunk PRIZE) — specialized RWA oracle
- Decal (Breakout PRIZE) — payments + loyalty

### Colosseum Copilot
- [`colosseum.com/copilot`](https://colosseum.com/copilot) — research assistant
- [`docs.colosseum.com/copilot/introduction`](https://docs.colosseum.com/copilot/introduction) — overview
- [`docs.colosseum.com/copilot/api-reference.md`](https://docs.colosseum.com/copilot/api-reference.md) — API reference
- API base: `copilot.colosseum.com/api/v1`
- Endpoints: `/status`, `/filters`, `/search/projects`, `/search/archives`, `/archives/:id`, `/projects/by-slug/:slug`, `/analyze`, `/compare`, `/clusters/:key`, `/source-suggestions`, `/feedback`

---

## Tools + services (non-Solana)

### Development
- [`github.com/anza-xyz/pinocchio`](https://github.com/anza-xyz/pinocchio) — Pinocchio SDK for Solana
- [`modelcontextprotocol.io`](https://modelcontextprotocol.io) — MCP specification
- [`github.com/model-checker/kani`](https://github.com/model-checker/kani) — Kani Rust verifier

### Monitoring + analytics
- [`rpcfast.com`](https://rpcfast.com) — RPC benchmarking + HFT playbook
- [`triton.one`](https://triton.one) — Yellowstone / Fumarole data infrastructure

---

## Research + investor sources

### Primary research
- [Galaxy Research "Solana's Next Chapter: Internet Capital Markets"](https://www.galaxy.com/insights/research/solana-firedancer-anza-alpenglow-internet-capital-markets) — 2025-10-28
- [Galaxy Research "Solana Q4 2025 Update: Weathering the Downturn"](https://www.galaxy.com/insights/research/solana-q4-2025-update) — Q4 recap
- [Galaxy Research "Weekly Top Stories 12/12/25"](https://www.galaxy.com/insights/research/weekly-top-stories-12-12-25) — cancel-priority primitives mention
- [Helius Ecosystem Report H1 2025](https://www.helius.dev/blog/solana-ecosystem-report-h1-2025) — ecosystem health snapshot
- [Helius Stablecoin Landscape](https://www.helius.dev/blog/solanas-stablecoin-landscape) — stablecoin ecosystem
- [Helius Proprietary AMM Revolution](https://www.helius.dev/blog/solana-proprietary-amm-revolution) — prop AMM thesis
- [Superteam "What To Build for Solana DeFi?"](https://blog.superteam.fun/p/what-to-build-for-solana-defi) — builder recommendations
- [Superteam "Solana Need L2s And Appchains?"](https://blog.superteam.fun/p/solana-need-l2s-and-appchains) — architecture thesis

### VC thought leaders
- a16z Crypto blog
- Multicoin Capital writings
- Paradigm research
- Placeholder VC blog
- Variant blog
- Dragonfly

---

## Security researchers

### SIRN founding members (April 2026)
- [`asymmetric.re`](https://blog.asymmetric.re) — STRIDE program lead
- [`ottersec.io`](https://ottersec.io) — security audits
- [`neodyme.io`](https://neodyme.io) — security research (`neodyme_blog` published "Nonce Upon a Time")
- Squads Protocol — multisig
- ZeroShadow — DeFi defense

### Other major security firms
- [`cyfrin.io`](https://cyfrin.io) — Cyfrin (published Drift hack learnings + opened SIMD-513)
- [`sec3.dev`](https://sec3.dev) — Sec3 Security
- Rugg Sec
- Immunefi (bug bounty platform)

---

## Community channels

### Solana-wide
- Solana Tech Discord
- Superteam Discord
- Colosseum Discord
- `r/solana`

### LATAM / Spanish-speaking
- Superteam LATAM
- `r/argentina` / `r/devsargentina`
- [`t.me/solanalatam`](https://t.me/solanalatam) (if exists)

### Twitter lists
- Solana Foundation official
- Colosseum ecosystem partners
- Major validators + stakeweight holders
- Percolator-adjacent (Anatoly, percolator_fun, HaidarIDK, etc.)

---

## Perc5ive internal artifacts

### This folder
- `README.md` — strategic anchor
- `START_HERE.md` — fresh-session bootstrap
- `TOLY_CORPUS.md` — canonical quotes + tweets
- `COMPETITIVE_LANDSCAPE.md` — Colosseum Copilot + Twitter snapshot
- `SPEC.md` — Percolator spec v12.17.0 derivative
- `SOV_SPRINT.md` — 48-hour decision gate plan
- `HELLO_SLAB.md` — technical feasibility spike
- `BUILD_PLAN.md` — 5-week expanded plan
- `TOLY_STRATEGY.md` — outreach playbook (private)
- `VC_NARRATIVE.md` — post-hackathon fundraise plan
- `PERCOLATOR_BENCH.md` — conformance + fuzz suite spec
- `MCP_PERCOLATOR.md` — MCP tool catalog
- `markets/SOV.md` — Sov market spec
- `markets/PYTH_RACE.md` — PythRaceMarket spec
- `markets/LP_PERP.md` — LPPerp spec
- `RESOURCES.md` — this file

### Backing data (not in this folder)
- Pain-scout research: `/home/marche/architect/data/pain-scout/`
- Hackathon experiment raw data: `/home/marche/architect/data/pain-scout/2026-04-15-hackathon-experiment/`
  - `copilot-raw/` — 90+ Copilot API responses across 9 research waves
  - `00-experiment-report.md` through `05-FINAL-grand-champion-candidates.md` — wave-by-wave results

---

## Key Colosseum Copilot credential

Stored at: `/home/marche/colosseumcopilot`

Contains PAT that expires 2026-07-02. Renew via Colosseum Copilot settings before expiration.

API usage pattern:
```bash
source /home/marche/colosseumcopilot
curl -s "$COLOSSEUM_COPILOT_API_BASE/search/projects" \
  -H "Authorization: Bearer $COLOSSEUM_COPILOT_PAT" \
  -H "Content-Type: application/json" \
  -d '{"query":"..","limit":10}'
```

---

## Maintenance

Check freshness of external links periodically. Anatoly iterates Percolator weekly; re-fetch spec.md before any architecture decision.

Last verified: 2026-04-15.
