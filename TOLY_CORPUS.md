# Toly Corpus — canonical quotes and links

All publicly verified statements from Anatoly Yakovenko and the Percolator ecosystem that ground the Perc5ive thesis.

---

## 2025-10-19 — GitHub publication

- Repo published at [`github.com/aeyakovenko/percolator`](https://github.com/aeyakovenko/percolator)
- Initial description: sharded perpetual exchange protocol with Router + Slab architecture
- Sister repo [`github.com/aeyakovenko/percolator-cli`](https://github.com/aeyakovenko/percolator-cli)

## 2025-10-20 — Toly's "steal this" tweet

> *"I am just messing around with Claude to see how well it can generate Pinocchio and test with surfpool. Pls steal the idea. I want to see if it's possible to replicate the same prop-amm competition for spot but for perps."*

— @aeyakovenko, reported by AMBCrypto, Cointelegraph, Bankless, Brave New Coin

Key phrases:
- *"messing around with Claude"* — AI-native workflow; 5ive's MCP aligns
- *"generate Pinocchio and test with surfpool"* — specific technical stack
- *"Pls steal the idea"* — explicit invitation
- *"prop-amm competition for spot but for perps"* — the deepest architectural intent

## 2025-10-21 — Press coverage headlines

- *"Solana Co-Founder Vibe Codes Hyperliquid Rival, Invites Devs to 'Steal Idea'"* — Yahoo Finance
- *"Solana's co-founder drops Percolator Perps DEX, dares devs to 'steal' it"* — AMBCrypto
- *"Solana's Yakovenko Teases 'Percolator' Perp DEX: Sharded Engines, Parallel Order Books"* — CryptoNinjas

## 2025-11-01 — Plan phase update

- `plan.md` master branch: Phases 1-3 complete (trading + funding + liquidation)
- Phases 4-5 pending feature implementation
- Explicit design constraint: *"Percolator does NOT move tokens — a wrapper program performs SPL transfers and calls into the engine"*

## April 2026 — Toly "feature complete" tweet (the thesis-correcting one)

Tweet ID `2019890554436481243`, URL `https://x.com/toly/status/2019890554436481243`

> *"🦞🦞🦞 Hey, percolator update. It's basically feature complete. What is it though? It's a small Open Source formally verified library for managing risk that devs can use to build their own markets. It's designed to let users bring their own matching programs to provide..."*

Key phrases:
- *"basically feature complete"* — liquidation engine is DONE, changes our race-strategy
- *"small Open Source formally verified library"* — Kani proofs confirmed
- *"devs can use to build their own markets"* — the BYOM invitation
- *"users bring their own matching programs"* — confirms the toolkit opportunity

## April 2026 — Sov retweet (the market-type validation)

Tweet ID `2020173049094619222`, visible via luc.sats retweet

> *"sov = percolator inverted market for the memecoin, so it's backed by the memecoin + burn the admin key. The insurance fund will..."*

Key phrases:
- *"sov = percolator inverted market for the memecoin"* — Toly defining the concept
- *"backed by the memecoin"* — inverted perp collateral mechanics
- *"burn the admin key"* — trust-minimization pattern
- *"insurance fund will..."* — truncated, but implies insurance-fund role in Sov

## Percolator_fun Twitter account

- Handle: [@percolator_fun](https://x.com/percolator_fun)
- Status: active April 2026
- Relationship to Anatoly: independent fan/ecosystem account tracking upstream
- Representative tweet: *"We have been able to fork the code present in @aeyakovenko's github repo to build a working perp dex. The code is not fully complete, resulting in a few broken & missing features. We will continue to update the website as he updates the codebase."*
- Web version live at `dex.percolator.site` (HaidarIDK/PERColator repo)
- **Signal:** they TRACK upstream, they don't lead. Natural ally, not competitor.

## Aster/Hyperliquid competitive framing (external context)

- Aster 24h volume hit $41.8B in April 2026, grabbing 70%+ of onchain perps share
- Solana perp volume: $699B March 2026 vs $1.36T October 2025 peak
- Hyperliquid captured 34% of DEX share
- Galaxy Research "Solana's Next Chapter: Internet Capital Markets" explicitly names perp-DEX as Solana's 2026 structural gap
- Galaxy Research Q4 2025: *"Diversifying Solana's leading fee-generating applications is a continued focus and challenge for the ecosystem"*

## Academic anchor

Tarun Chitra's autodeleveraging paper: [arXiv `2512.01112`](https://arxiv.org/abs/2512.01112) — referenced in Percolator repo as design foundation for the A/K side-index math.

## External mentions / lists

- Bankless: *"Solana's Anatoly Yakovenko Building 'Percolator' Perpetuals DEX"*
- The Block: *"Solana co-founder Anatoly Yakovenko is designing a perps DEX: GitHub"*
- CoinTelegraph: *"Solana Founder Shares Plans For New Perp DEX 'Percolator'"*
- DEXTools News: *"Percolator: The Solana Perp DEX and Pump.fun Rival Explained"*
- Bitget News: *"Solana Founder Launches Percolator, a New Perp DEX"*

## How to use this corpus

When you write the public README, pitch deck, tweet thread, or PR to Anatoly's repo:
1. Never paraphrase his quotes — quote them directly and link the tweet ID
2. The "steal this" framing is now historical — the current positioning is BYOM library + markets
3. Sov is the one concept you must mention early because it shows you've been tracking him recently
4. Acknowledge Tarun Chitra's paper if you write anything technical about the A/K indices
5. Reference Percolator_fun / HaidarIDK as allies, not competitors

## Monitoring plan

Someone on the team should:
- Watch [@aeyakovenko](https://x.com/aeyakovenko) daily during the sprint
- Watch [@percolator_fun](https://x.com/percolator_fun) daily
- Watch `aeyakovenko/percolator` commits daily (git clone locally + `git log --oneline -10` as cron)
- Flag any mention of "market", "BYOM", "Sov", "slab", "bring your own", or "5ive" in any Solana-ecosystem Twitter within 2 hours
