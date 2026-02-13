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

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
oracle_el="$script_dir/oracle_eval.el"
emacs_bin="${NEOVM_ORACLE_EMACS:-}"

if [[ -z "$emacs_bin" ]]; then
  if ! command -v emacs >/dev/null 2>&1; then
    echo "emacs binary not found in PATH (or set NEOVM_ORACLE_EMACS)" >&2
    exit 127
  fi
  emacs_bin="emacs"
fi

if [[ ! -x "$emacs_bin" ]]; then
  echo "oracle emacs binary is not executable: $emacs_bin" >&2
  exit 127
fi

NEOVM_FORMS_FILE="$forms_file" "$emacs_bin" --batch -Q -l "$oracle_el" 2>/dev/null
