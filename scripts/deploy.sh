#!/usr/bin/env bash
# Deploy a perc5ive artifact to a Solana cluster via `five deploy`.
#
# Usage:
#   scripts/deploy.sh <artifact_name> [--target <devnet|testnet|mainnet|local>] [--dry-run]
#
# `<artifact_name>` is one of: perc5ive, sov, pyth_race, lp_perp.
#
# For `perc5ive`: this script runs the `link-perc5ive` binary first to produce
# `target/perc5ive.linked.bin` — the DSL-compiled fbin with all 9 hand-written
# handler bodies appended and every sentinel rewritten. That linked binary is
# what ships on-chain. Deploying the raw `.fbin` would put sentinel-stubbed
# handlers on-chain, which cannot execute deposit / withdraw / settle / etc.
#
# For `sov`, `pyth_race`, `lp_perp`: these are pure DSL with no sentinel-
# stubbed handlers; the raw `.fbin` is shipped as-is, renamed to `.bin` so
# the @five-vm/cli accepts it (CLI whitelists .five/.bin/.v only).
#
# Defaults to the cluster set in `solana config get` if --target is omitted.

set -euo pipefail

ARTIFACT="${1:-}"
shift || true

if [[ -z "${ARTIFACT}" ]]; then
    echo "usage: scripts/deploy.sh <perc5ive|sov|pyth_race|lp_perp> [five deploy flags...]" >&2
    exit 2
fi

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

case "${ARTIFACT}" in
    perc5ive)
        # Regenerate everything and run the linker.
        if [[ ! -f "target/perc5ive.fbin" ]]; then
            echo "missing target/perc5ive.fbin — running \`cargo build\` to regenerate"
            cargo build
        fi
        echo "→ linking perc5ive (normalize_dsl_header + append 9 handler bodies + rewrite sentinels)"
        cargo run --quiet --bin link-perc5ive -- target/perc5ive.fbin target/perc5ive.linked.bin
        SHIPPED="${REPO_ROOT}/target/perc5ive.bin"
        cp "target/perc5ive.linked.bin" "${SHIPPED}"
        ;;
    sov|pyth_race|lp_perp)
        SOURCE="${REPO_ROOT}/target/${ARTIFACT}.fbin"
        SHIPPED="${REPO_ROOT}/target/${ARTIFACT}.bin"
        if [[ ! -f "${SOURCE}" ]]; then
            echo "missing ${SOURCE} — run \`cargo build\` first to regenerate it" >&2
            exit 1
        fi
        cp "${SOURCE}" "${SHIPPED}"
        ;;
    *)
        echo "unknown artifact '${ARTIFACT}' — expected perc5ive, sov, pyth_race, or lp_perp" >&2
        exit 2
        ;;
esac

echo "→ deploying ${ARTIFACT} ($(wc -c <"${SHIPPED}") bytes)"
exec five deploy "${SHIPPED}" "$@"
