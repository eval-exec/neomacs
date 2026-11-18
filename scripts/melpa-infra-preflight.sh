#!/usr/bin/env bash
set -euo pipefail

workspace_root=${NEXTEST_WORKSPACE_ROOT:-}
if [[ -z "$workspace_root" || ! -d "$workspace_root" ]]; then
  echo "MELPA infrastructure preflight requires NEXTEST_WORKSPACE_ROOT" >&2
  exit 2
fi

# The melpa caches live under the target directory, with the rest of the
# build output; keep this in step with neomacs_infra::melpa_cache_root().
target_root="${CARGO_TARGET_DIR:-$workspace_root/target}"
scratch_parent="$target_root/melpa"

# PEER 5832: the approved MELPA cache root must live inside a checkout.
#
# GNU `normal-backup-enable-predicate' (files.el) disables backups below the
# temporary directory and /tmp, and checkout fixtures resolve their root by
# walking ABOVE the sandbox for the outermost .git/.svn/.cvs.  A relocated
# CARGO_TARGET_DIR outside the checkout therefore turns path-independent
# contracts into host-dependent state -- the same skew, one layer earlier than
# MelpaSandbox::new's require_checkout_root.  Refuse once here, before either
# editor is launched, rather than failing per package with GNU==Neomacs
# mismatches.  Match require_checkout_root's marker set and outermost-wins rule
# (crates/neomacs-melpa-test-support/src/lib.rs).  The walk is lexical
# (`dirname`, not realpath): a `<worktree>/target` symlink to another volume
# still resolves to the checkout that owns the link.  An absolute
# CARGO_TARGET_DIR outside the checkout does not.
checkout_dir="$scratch_parent"
checkout_root=
while :; do
  if [[ -e "$checkout_dir/.git" || -e "$checkout_dir/.svn" || -e "$checkout_dir/.cvs" ]]; then
    checkout_root=$checkout_dir
  fi
  parent=$(dirname -- "$checkout_dir")
  if [[ "$parent" == "$checkout_dir" ]]; then
    break
  fi
  checkout_dir=$parent
done
if [[ -z "$checkout_root" ]]; then
  echo "MELPA infrastructure preflight refused: sandbox $scratch_parent is not inside a checkout (no .git/.svn/.cvs above it); run the MELPA suite with CARGO_TARGET_DIR unset so the MELPA cache root, and every case root below it, lives inside the checkout. If build output must live on another volume, leave CARGO_TARGET_DIR unset and point <worktree>/target at that directory with a symlink; the checkout walk uses that logical path. An absolute CARGO_TARGET_DIR outside the checkout is refused. GNU backup policy and the checkout fixtures derive observable state from that path." >&2
  exit 2
fi
echo "MELPA infrastructure preflight checkout: $checkout_root"
# The checkout-walk tests set this and stop before either editor is launched.
if [[ -n "${NEOMACS_MELPA_PREFLIGHT_CHECK_ONLY:-}" ]]; then
  exit 0
fi

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
