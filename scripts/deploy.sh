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
    echo "usage: scripts/deploy.sh <perc5ive|meta|sov|pyth_race|lp_perp> [five deploy flags...]" >&2
    exit 2
fi

REPO_ROOT="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${REPO_ROOT}"

# Custom VM loader program ID — perc5ive runs against our own loader built
# from the latest five-solana + five-vm-mito sources (includes InitLargeProgram
# chunked-deploy support + all new u256/i128/i256/MULDIV_REM_U256/field-u128
# opcodes). Override by exporting PERC5IVE_VM_LOADER_ID.
#
# The stock loader at J99pDwVh1PqcxyBGKRvPKk8MUvW8V8KF6TmVEavKnzaF was last
# deployed 2025-08-20 and predates every opcode perc5ive uses, so the stock
# path doesn't work today.
DEFAULT_LOADER_ID="CTSPYe2YTciJr2oHqGZZr6H8GnaSNCNENZdcjTM85Dq9"
LOADER_ID="${PERC5IVE_VM_LOADER_ID:-${DEFAULT_LOADER_ID}}"

# Loader selection: perc5ive needs the custom loader (multi-precision
# opcodes + chunked deploy). Markets are pure DSL and deploy fine on the
# stock loader — both paths coexist cleanly on devnet.
STOCK_LOADER_ID="J99pDwVh1PqcxyBGKRvPKk8MUvW8V8KF6TmVEavKnzaF"

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
        TARGET_LOADER="${LOADER_ID}"
        ;;
    meta)
        # MetaGenesis (percolator-meta fair-launch layer) — same link-then-ship
        # pipeline as perc5ive: append the 12 genesis-lifecycle handler bodies
        # and rewrite their sentinels. The 5 Phase-3 governance sentinels stay
        # stubbed. Needs the custom loader (shared VM ABI).
        if [[ ! -f "target/meta.fbin" ]]; then
            echo "missing target/meta.fbin — running \`cargo build\` to regenerate"
            cargo build
        fi
        echo "→ linking meta (normalize_dsl_header + append 12 genesis bodies + rewrite sentinels)"
        cargo run --quiet --bin link-meta -- target/meta.fbin target/meta.linked.bin
        SHIPPED="${REPO_ROOT}/target/meta.bin"
        cp "target/meta.linked.bin" "${SHIPPED}"
        TARGET_LOADER="${LOADER_ID}"
        ;;
    sov|pyth_race|lp_perp)
        SOURCE="${REPO_ROOT}/target/${ARTIFACT}.fbin"
        SHIPPED="${REPO_ROOT}/target/${ARTIFACT}.bin"
        if [[ ! -f "${SOURCE}" ]]; then
            echo "missing ${SOURCE} — run \`cargo build\` first to regenerate it" >&2
            exit 1
        fi
        cp "${SOURCE}" "${SHIPPED}"
        TARGET_LOADER="${STOCK_LOADER_ID}"
        ;;
    *)
        echo "unknown artifact '${ARTIFACT}' — expected perc5ive, meta, sov, pyth_race, or lp_perp" >&2
        exit 2
        ;;
esac

echo "→ deploying ${ARTIFACT} ($(wc -c <"${SHIPPED}") bytes) against loader ${TARGET_LOADER}"
exec five deploy "${SHIPPED}" --program-id "${TARGET_LOADER}" "$@"
