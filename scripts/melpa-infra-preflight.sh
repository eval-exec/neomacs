#!/usr/bin/env bash
set -euo pipefail

workspace_root=${NEXTEST_WORKSPACE_ROOT:-}
if [[ -z "$workspace_root" || ! -d "$workspace_root" ]]; then
  echo "MELPA infrastructure preflight requires NEXTEST_WORKSPACE_ROOT" >&2
  exit 2
fi

scratch_parent="$workspace_root/tmp/melpa"
mkdir -p "$scratch_parent"
scratch_root=$(mktemp -d "$scratch_parent/preflight.XXXXXX")

cleanup() {
  case "$scratch_root" in
    "$scratch_parent"/preflight.*)
      rm -rf -- "$scratch_root"
      ;;
    *)
      echo "refusing to remove unexpected MELPA preflight path: $scratch_root" >&2
      ;;
  esac
}
trap cleanup EXIT

resolve_executable() {
  local label=$1
  local candidate=$2

  if [[ "$candidate" == */* ]]; then
    if [[ ! -x "$candidate" ]]; then
      echo "MELPA infrastructure preflight cannot execute $label: $candidate" >&2
      return 1
    fi
    printf '%s\n' "$candidate"
    return
  fi

  if ! command -v "$candidate" >/dev/null 2>&1; then
    echo "MELPA infrastructure preflight cannot find $label on PATH: $candidate" >&2
    return 1
  fi
  command -v "$candidate"
}

neomacs_candidate=${NEOMACS_BIN:-"$workspace_root/target/release/neomacs"}
neomacs_bin=$(resolve_executable Neomacs "$neomacs_candidate")

oracle_candidate=${NEOMACS_MELPA_ORACLE_EMACS:-}
if [[ -z "$oracle_candidate" ]]; then
  oracle_candidate=${NEOVM_ORACLE_EMACS:-}
fi
if [[ -z "$oracle_candidate" ]]; then
  oracle_candidate=${ORACLE_EMACS:-}
fi
if [[ -z "$oracle_candidate" ]]; then
  source_oracle=/home/exec/Projects/github.com/emacs-mirror/emacs/src/emacs
  if [[ -x "$source_oracle" ]]; then
    oracle_candidate=$source_oracle
  else
    oracle_candidate=emacs
  fi
fi
oracle_bin=$(resolve_executable "GNU Emacs oracle" "$oracle_candidate")

# WHICH GNU (ledger 214).  This preflight is the melpa suite's gate and the
# first thing in it that launches GNU, and the resolution above is FIVE rules
# deep -- three environment variables, a hard-coded checkout and PATH -- so it
# is exactly the place a different GNU could enter a published parity number
# unnoticed.  It runs once per suite, so it takes the exhaustive check.
if ! parity_reference=$(bash "$(dirname "$0")/parity-reference-attest.sh" "$oracle_bin" exhaustive); then
  echo "MELPA infrastructure preflight refused: the GNU reference did not attest" >&2
  exit 1
fi
echo "MELPA infrastructure preflight reference: $parity_reference"

git_bin=$(resolve_executable Git git)
"$git_bin" --version >/dev/null

probe='(progn
  (require '"'"'package)
  (unless (fboundp '"'"'package-install-file)
    (error "package-install-file is unavailable"))
  (princ "NEOMACS-MELPA-PREFLIGHT:ready"))'

run_probe() {
  local label=$1
  local executable=$2
  local runtime_root=$3
  local home="$scratch_root/$label/home"
  local editor_tmp="$scratch_root/$label/tmp"
  local output="$scratch_root/$label/stdout"
  local errors="$scratch_root/$label/stderr"

  mkdir -p \
    "$home" \
    "$editor_tmp" \
    "$scratch_root/$label/xdg/config" \
    "$scratch_root/$label/xdg/cache" \
    "$scratch_root/$label/xdg/data" \
    "$scratch_root/$label/xdg/state"

  if ! env \
    HOME="$home" \
    TMPDIR="$editor_tmp" \
    XDG_CONFIG_HOME="$scratch_root/$label/xdg/config" \
    XDG_CACHE_HOME="$scratch_root/$label/xdg/cache" \
    XDG_DATA_HOME="$scratch_root/$label/xdg/data" \
    XDG_STATE_HOME="$scratch_root/$label/xdg/state" \
    NEOMACS_RUNTIME_ROOT="$runtime_root" \
    TZ=UTC \
    "$executable" --batch --quick --eval "$probe" >"$output" 2>"$errors"
  then
    echo "MELPA infrastructure preflight failed to launch $label" >&2
    sed -n '1,160p' "$output" >&2
    sed -n '1,160p' "$errors" >&2
    return 1
  fi

  if ! grep -Fq "NEOMACS-MELPA-PREFLIGHT:ready" "$output"; then
    echo "MELPA infrastructure preflight received no ready marker from $label" >&2
    sed -n '1,160p' "$output" >&2
    sed -n '1,160p' "$errors" >&2
    return 1
  fi
}

run_probe gnu-emacs "$oracle_bin" "$workspace_root"
run_probe neomacs "$neomacs_bin" "$workspace_root"

if [[ -n "${NEXTEST_ENV:-}" ]]; then
  printf '%s\n' "NEOMACS_MELPA_INFRA_PREFLIGHT=ready" >>"$NEXTEST_ENV"
fi

echo "MELPA infrastructure preflight passed"
