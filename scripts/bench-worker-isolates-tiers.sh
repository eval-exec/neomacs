#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
neomacs_bin="${NEOMACS_BIN:-$repo_root/target/release/neomacs}"
elisp_file="$repo_root/scripts/bench-worker-isolates-tiers.el"
cores="$(nproc)"
asm_dump="${NEOVM_JIT_DUMP_ASM:-${TMPDIR:-/tmp}/neomacs-worker-isolates-jit-$$.asm}"

if [[ ! -x "$neomacs_bin" ]]; then
  printf 'Missing release binary: %s\n' "$neomacs_bin" >&2
  exit 1
fi

printf '| Tier | num-processors | Pool size | Serial (s) | Parallel (s) | Speedup |\n'
printf '| --- | ---: | ---: | ---: | ---: | ---: |\n'

for tier in interpreted bytecode jit; do
  if [[ "$tier" == bytecode ]]; then
    NEOMACS_WORKER_THREADS="$cores" \
      NEOMACS_ISOLATE_TIER="$tier" NEOVM_JIT=0 \
      "$neomacs_bin" -Q --batch -l "$elisp_file"
  elif [[ "$tier" == jit ]]; then
    NEOMACS_WORKER_THREADS="$cores" \
      NEOMACS_ISOLATE_TIER="$tier" NEOVM_JIT=1 NEOVM_JIT_DUMP_ASM="$asm_dump" \
      "$neomacs_bin" -Q --batch -l "$elisp_file"
  else
    NEOMACS_WORKER_THREADS="$cores" \
      NEOMACS_ISOLATE_TIER="$tier" NEOVM_JIT=0 \
      "$neomacs_bin" -Q --batch -l "$elisp_file"
  fi
done

if [[ -f "$asm_dump" ]]; then
  printf '\nJIT assembly evidence: %s\n' "$asm_dump" >&2
  grep -n -E 's6-worker-bench-loop|s6-worker-jit-witness|s6-worker-bench' "$asm_dump" >&2 || true
fi
