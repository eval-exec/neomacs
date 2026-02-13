#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 1 ]]; then
  echo "usage: $0 <allowlist-file> [load-file ...]" >&2
  exit 2
fi

allowlist_file="$1"
shift
if [[ ! -f "$allowlist_file" ]]; then
  echo "allowlist file not found: $allowlist_file" >&2
  exit 2
fi

allowlist_dir="$(cd "$(dirname "$allowlist_file")" && pwd)"
allowlist_abs="$allowlist_dir/$(basename "$allowlist_file")"

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
runner_el="$script_dir/ert_allowlist_eval.el"
emacs_bin="${NEOVM_ORACLE_EMACS:-${ORACLE_EMACS:-}}"

if [[ -z "$emacs_bin" ]]; then
  if ! command -v emacs >/dev/null 2>&1; then
    echo "emacs binary not found in PATH (or set NEOVM_ORACLE_EMACS/ORACLE_EMACS)" >&2
    exit 127
  fi
  emacs_bin="emacs"
fi

if [[ ! -x "$emacs_bin" ]]; then
  echo "oracle emacs binary is not executable: $emacs_bin" >&2
  exit 127
fi

load_files_abs=()
for path in "$@"; do
  if [[ ! -f "$path" ]]; then
    echo "load file not found: $path" >&2
    exit 2
  fi
  dir="$(cd "$(dirname "$path")" && pwd)"
  load_files_abs+=("$dir/$(basename "$path")")
done

load_env=""
for path in "${load_files_abs[@]}"; do
  if [[ -z "$load_env" ]]; then
    load_env="$path"
  else
    load_env="$load_env:$path"
  fi
done

NEOVM_ERT_ALLOWLIST_FILE="$allowlist_abs" \
NEOVM_ERT_LOAD_FILES="$load_env" \
"$emacs_bin" --batch -Q -l "$runner_el" 2>/dev/null
