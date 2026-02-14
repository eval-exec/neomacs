#!/usr/bin/env bash
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "usage: $0 <forms-file>" >&2
  exit 2
fi

forms_file="$1"
if [[ ! -f "$forms_file" ]]; then
  echo "forms file not found: $forms_file" >&2
  exit 2
fi

forms_dir="$(cd "$(dirname "$forms_file")" && pwd)"
forms_file_abs="$forms_dir/$(basename "$forms_file")"

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
worker_manifest="$repo_root/rust/neovm-worker/Cargo.toml"
worker_binary="$repo_root/rust/neovm-worker/target/debug/examples/elisp_compat_runner"

needs_build=0
if [[ ! -x "$worker_binary" ]]; then
  needs_build=1
elif find \
  "$repo_root/rust/neovm-core" \
  "$repo_root/rust/neovm-host-abi" \
  "$repo_root/rust/neovm-worker" \
  -type f \
  \( -name '*.rs' -o -name 'Cargo.toml' \) \
  -newer "$worker_binary" \
  -print -quit | grep -q .; then
  needs_build=1
fi

if [[ "$needs_build" -eq 1 ]]; then
  cargo build \
    --manifest-path "$worker_manifest" \
    --example elisp_compat_runner
fi

NEOVM_FORMS_FILE="$forms_file_abs" \
NEOVM_DISABLE_LOAD_CACHE_WRITE=1 \
"$worker_binary" "$forms_file_abs"
