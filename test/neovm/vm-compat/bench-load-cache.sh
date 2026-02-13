#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 || $# -gt 2 ]]; then
  echo "usage: $0 <source.el> [iterations]" >&2
  exit 2
fi

source_file="$1"
iterations="${2:-100}"

if [[ ! -f "$source_file" ]]; then
  echo "source file not found: $source_file" >&2
  exit 2
fi

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

cargo run \
  --manifest-path "$repo_root/rust/neovm-core/Cargo.toml" \
  --example load_cache_bench \
  -- "$source_file" "$iterations"
