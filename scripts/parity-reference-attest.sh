#!/usr/bin/env bash
# Ledger 214: check that a GNU Emacs is the one this project's parity numbers
# are measured against, and print the stamp a published number must carry.
#
#   scripts/parity-reference-attest.sh EDITOR [fingerprint|exhaustive]
#
# Prints ONE line on stdout -- the stamp -- and exits 0 when the editor is the
# pin.  On any disagreement it writes a named refusal to stderr and exits 3.
# `NEOMACS_PARITY_REFERENCE=none' declares that no pin is available: the stamp
# then says UNATTESTED and the exit is 0, so a number can still be taken, but
# it can never be mistaken for a comparable one.
#
# WHY THIS EXISTS ALONGSIDE THE RUST ONE.  Four harnesses need this check and
# two of them are shell.  Making the shell ones call Rust would put a cargo
# build in front of every sweep; making the Rust ones call this would put a
# subprocess inside a 38k-test run.  So there are two readers of ONE manifest,
# and `neomacs-parity-reference' has a test that runs THIS script over planted
# fixtures and requires it to agree with the Rust reader on every one of them.
# Two implementations that are not held together are how a guard rots.
#
# The manifest format is deliberately tiny (see parity-reference.toml).  This
# parser REFUSES a line it does not understand rather than skipping it: a pin
# that silently stops being read is the failure this whole file exists to
# prevent.
set -u

usage() {
  echo "usage: $0 [--if-gnu] EDITOR [fingerprint|exhaustive]" >&2
  echo "       $0 --port NEOMACS-BINARY" >&2
  exit 2
}

# --port: attest THIS PORT's binary, the other half of the pair.
#
# WHY THIS IS A DIFFERENT PREDICATE, NOT THE SAME ONE (ledger 214).  A parity
# number is a statement about a PAIR, and both halves should travel with it.
# But GNU and this port are pinned in opposite ways and must not share a
# predicate:
#
#   GNU is PINNED.  There is one right answer, parity-reference.toml records
#   it, and anything else is REFUSED.
#
#   This port is NOT pinnable.  It changes every commit; there is no constant
#   to record.  Recording a port hash in the manifest would mean re-pinning on
#   every commit, which is how a pin becomes noise people delete.
#
# So the port predicate is CORRESPONDENCE, not equality: can this binary be
# placed on the history of the tree being measured, and where?  The binary
# already knows -- crates/neomacs/build.rs embeds VERGEN_GIT_SHA and
# VERGEN_GIT_DIRTY, and `neomacs --version' prints both, for 8ms.
#
# The verdicts, and why only one of them refuses:
#
#   * built from HEAD, built clean, tree clean -- it corresponds.  Say so.
#   * built from an ANCESTOR of HEAD, or built dirty, or the tree is dirty --
#     BRANDED, not refused.  Building, measuring and then committing is the
#     normal order of work, and a harness that refuses it is a harness people
#     route around.  What was missing was never permission; it was the RECORD.
#   * built from a commit that is not on this tree's history at all -- REFUSED.
#     That binary cannot be talking about this tree.
#
# This closes the hole scripts/l205-provenance.sh leaves: that script proves
# what a binary was built FROM (the dos-codepage docstring, an empty *scratch*)
# and cannot say whether it matches the tree you are measuring.  Reproduced
# while writing this: the binary that took this entry's own sweep numbers
# reports `bfe815c13 (dirty)' against a HEAD of 5a12d613f, and provenance
# passes it without a word.

# --if-gnu: attest only when the editor IS a GNU Emacs, and say so otherwise.
#
# The single-editor runners are handed either peer and cannot know the role, so
# they ask this instead of guessing.  A GNU Emacs is identified by the dump it
# loads: src/emacs.c:1104-1120 looks for basename(argv0) + ".pdmp", and that
# file starts with the magic src/pdumper.c:116 writes.  This port's own dump is
# `neomacs.pdump' and is not one, so the two peers are told apart by an
# artifact GNU itself defines rather than by a name or a path.
#
# A GNU whose dump is MISSING is not quietly reclassified as "not GNU": it is
# refused below, and it could not have run anyway -- measured, a GNU with its
# dump removed exits 255 before evaluating anything, which is the failure
# ledger 211 section 10.1 already reports as "the EDITOR could not be RUN".
only_if_gnu=0
if [ "${1:-}" = --if-gnu ]; then
  only_if_gnu=1
  shift
fi

if [ "${1:-}" = --port ]; then
  shift
  [ "$#" -ge 1 ] || usage
  port_bin="$1"
  port_root="$(cd "$(dirname "$0")/.." && pwd)"
  [ -x "$port_bin" ] ||
    { echo "port reference: the PORT BINARY could not be run: $port_bin" >&2; exit 3; }
  port_version="$("$port_bin" --version 2> /dev/null)" ||
    { echo "port reference: $port_bin could not report its version" >&2; exit 3; }
  port_sha="$(printf '%s\n' "$port_version" |
    sed -n 's/^Git commit: \([0-9a-f]\{40\}\).*/\1/p')"
  if [ -z "$port_sha" ]; then
    echo "port reference: $port_bin reports no source revision, so a number taken" >&2
    echo "  with it cannot be tied to any tree.  Rebuild it inside the git checkout." >&2
    exit 3
  fi
  case "$port_version" in
    *"(dirty)"*) port_built=dirty ;;
    *"(worktree state unknown)"*) port_built=unknown ;;
    *) port_built=clean ;;
  esac
  port_head="$(git -C "$port_root" rev-parse HEAD 2> /dev/null)" || port_head=
  if [ -z "$port_head" ]; then
    printf 'neo=%s built=%s tree=no-git place=UNPLACEABLE\n' "${port_sha:0:11}" "$port_built"
    exit 0
  fi
  if [ -z "$(git -C "$port_root" status --porcelain 2> /dev/null)" ]; then
    port_tree=clean
  else
    port_tree=dirty
  fi
  if [ "$port_sha" = "$port_head" ]; then
    port_place=HEAD
  elif git -C "$port_root" merge-base --is-ancestor "$port_sha" "$port_head" 2> /dev/null; then
    port_place="behind-$(git -C "$port_root" rev-list --count "$port_sha".."$port_head")"
  else
    {
      echo "port reference REFUSED: $port_bin was built from $port_sha,"
      echo "  which is NOT on the history of this tree (HEAD $port_head)."
      echo "  That binary cannot be talking about the tree being measured, so a"
      echo "  number scored with it is not a number about this tree.  Rebuild with"
      echo "    cargo xtask fresh-build --release"
    } >&2
    exit 3
  fi
  printf 'neo=%s built=%s tree=%s place=%s\n' \
    "${port_sha:0:11}" "$port_built" "$port_tree" "$port_place"
  exit 0
fi

[ "$#" -ge 1 ] || usage
editor="$1"
depth="${2:-fingerprint}"
case "$depth" in
  fingerprint | exhaustive) ;;
  *)
    echo "parity reference: unknown depth ${depth}; expected fingerprint or exhaustive" >&2
    exit 2
    ;;
esac

root="$(cd "$(dirname "$0")/.." && pwd)"
manifest="${NEOMACS_PARITY_REFERENCE_FILE:-$root/parity-reference.toml}"

refuse() {
  echo "parity reference: $*" >&2
  exit 3
}

# The opt-out is exact.  A variable that disables a guard must not be able to
# be disabled and misspelled at the same time.
if [ -n "${NEOMACS_PARITY_REFERENCE:-}" ]; then
  if [ "$NEOMACS_PARITY_REFERENCE" = none ]; then
    echo "gnu=UNATTESTED (NEOMACS_PARITY_REFERENCE=none) attest=none"
    exit 0
  fi
  refuse "NEOMACS_PARITY_REFERENCE=\"$NEOMACS_PARITY_REFERENCE\" is not understood;" \
         "the only accepted value is \"none\", which brands every number produced as UNATTESTED"
fi

[ -f "$manifest" ] || refuse "cannot read the pin at $manifest: no such file"

# ---------------------------------------------------------------------------
# Parse the manifest, strictly
# ---------------------------------------------------------------------------
m_schema= m_emacs_version= m_mirror_commit= m_build_time=
m_fingerprint= m_executable_sha256= m_executable_size=
m_pdmp_sha256= m_pdmp_size=
lineno=0
while IFS= read -r line || [ -n "$line" ]; do
  lineno=$((lineno + 1))
  line="${line%$'\r'}"
  trimmed="${line#"${line%%[![:space:]]*}"}"
  case "$trimmed" in
    '' | '#'*) continue ;;
  esac
  case "$line" in
    *' = "'*'"') ;;
    *) refuse "$manifest line $lineno: expected 'key = \"value\"', got: $line" ;;
  esac
  key="${line%% = *}"
  value="${line#* = }"
  value="${value#\"}"
  value="${value%\"}"
  case "$key" in
    *[!a-z0-9_]* | '') refuse "$manifest line $lineno: '$key' is not a lower-snake key" ;;
  esac
  case "$value" in
    *'"'* | *'\'*) refuse "$manifest line $lineno: value has a quote or backslash; the format has no escapes" ;;
  esac
  case "$key" in
    schema | emacs_version | mirror_commit | build_time | fingerprint | \
      executable_sha256 | executable_size | pdmp_sha256 | pdmp_size) ;;
    *) refuse "$manifest line $lineno: unknown key '$key' for schema 1" ;;
  esac
  eval "current=\${m_$key}"
  [ -z "$current" ] || refuse "$manifest line $lineno: duplicate key '$key'"
  eval "m_$key=\$value"
done < "$manifest"

for key in schema emacs_version mirror_commit build_time fingerprint \
  executable_sha256 executable_size pdmp_sha256 pdmp_size; do
  eval "value=\${m_$key}"
  [ -n "$value" ] || refuse "$manifest: missing key '$key'"
done
[ "$m_schema" = 1 ] || refuse "$manifest: schema '$m_schema' is not understood by this script; expected 1"

check_hex() {
  # $1 = key name, $2 = value, $3 = required length
  case "$2" in
    *[!0-9a-f]*) refuse "$manifest: key '$1' must be lowercase hex, got '$2'" ;;
  esac
  [ "${#2}" -eq "$3" ] || refuse "$manifest: key '$1' must be $3 hex digits, got ${#2}"
}
check_hex mirror_commit "$m_mirror_commit" 40
check_hex fingerprint "$m_fingerprint" 64
check_hex executable_sha256 "$m_executable_sha256" 64
check_hex pdmp_sha256 "$m_pdmp_sha256" 64
case "$m_executable_size" in *[!0-9]* | '') refuse "$manifest: executable_size is not an integer" ;; esac
case "$m_pdmp_size" in *[!0-9]* | '') refuse "$manifest: pdmp_size is not an integer" ;; esac

# ---------------------------------------------------------------------------
# Resolve the editor the way a shell would, then canonicalize it
# ---------------------------------------------------------------------------
# The pin is reached through a symlink (~/.local/bin/emacs), and it was a
# BROKEN one of those that caused the incident behind ledger 211 section 10.1.
# In --if-gnu mode an editor that cannot be resolved is NOT this script's
# finding.  Ledger 211 section 10.1 bought the distinction between "the EDITOR
# could not be RUN" (exit 127, reported by the runner) and "it ran and wrote
# nothing", and an attestation that refused first would take that diagnosis
# away.  The sweep, which KNOWS which side is GNU, uses the strict mode and
# still refuses -- the guarantee lives where the role is known.
not_the_reference() {
  if [ "$only_if_gnu" = 1 ]; then
    echo "gnu=n/a (peer is not the pinned reference) attest=n/a"
    exit 0
  fi
  refuse "$@"
}

case "$editor" in
  */*) candidate="$editor" ;;
  *)
    candidate="$(command -v -- "$editor" 2> /dev/null)" ||
      not_the_reference "the EDITOR could not be resolved: $editor -- not found on PATH"
    ;;
esac
resolved="$(readlink -f -- "$candidate" 2> /dev/null)" || resolved=
[ -n "$resolved" ] && [ -f "$resolved" ] ||
  not_the_reference "the EDITOR could not be resolved: $candidate -- not found, not a file, or a broken symlink"

# src/emacs.c:1104-1120 falls back to basename(argv0) + ".pdmp"; an strace of
# the pinned build confirms it opens exactly <canonical executable>.pdmp.
pdmp="$resolved.pdmp"
[ -f "$pdmp" ] ||
  not_the_reference "the editor resolved but its dump is missing at $pdmp"

mismatch() {
  # $1 = field, $2 = path, $3 = pinned, $4 = found
  {
    echo "parity reference MISMATCH on $1 for $2"
    echo "  pinned: $3"
    echo "  found:  $4"
    echo "  This GNU is NOT the one this project's parity numbers are measured"
    echo "  against, so a number scored against it is not comparable with any"
    echo "  published one.  A rebuild of the GNU mirror is its own change with"
    echo "  its own re-baselining: run"
    echo "    cargo run -p xtask -- pin-reference --emacs PATH --reason \"...\""
    echo "  to re-pin deliberately, or set NEOMACS_PARITY_REFERENCE=none to"
    echo "  measure without a pin and have every number branded UNATTESTED."
  } >&2
  exit 3
}

# struct dump_header: char magic[16] then unsigned char fingerprint[32]
# (src/pdumper.c:361-367); the magic itself is src/pdumper.c:116.  The magic is
# checked FIRST so that --if-gnu can tell a non-GNU peer from a wrong GNU
# before any size is compared.
magic="$(head -c 14 -- "$pdmp")"
[ "$magic" = DUMPEDGNUEMACS ] ||
  not_the_reference "$pdmp is not a GNU dump file: magic is '$magic', expected 'DUMPEDGNUEMACS'"

actual_fingerprint="$(od -An -tx1 -j16 -N32 -- "$pdmp" | tr -d ' \n')"
[ "${#actual_fingerprint}" -eq 64 ] ||
  refuse "$pdmp: cannot read the 32-byte build fingerprint from the dump header"
[ "$actual_fingerprint" = "$m_fingerprint" ] ||
  mismatch "build fingerprint" "$pdmp" "$m_fingerprint" "$actual_fingerprint"

actual_size="$(stat -c %s -- "$resolved")" || refuse "cannot stat $resolved"
[ "$actual_size" = "$m_executable_size" ] ||
  mismatch "executable size" "$resolved" "$m_executable_size" "$actual_size"

actual_size="$(stat -c %s -- "$pdmp")" || refuse "cannot stat $pdmp"
[ "$actual_size" = "$m_pdmp_size" ] ||
  mismatch "dump size" "$pdmp" "$m_pdmp_size" "$actual_size"

if [ "$depth" = exhaustive ]; then
  actual="$(sha256sum -- "$resolved")" || refuse "cannot hash $resolved"
  actual="${actual%% *}"
  [ "$actual" = "$m_executable_sha256" ] ||
    mismatch "executable sha256" "$resolved" "$m_executable_sha256" "$actual"
  actual="$(sha256sum -- "$pdmp")" || refuse "cannot hash $pdmp"
  actual="${actual%% *}"
  [ "$actual" = "$m_pdmp_sha256" ] ||
    mismatch "dump sha256" "$pdmp" "$m_pdmp_sha256" "$actual"
fi

# The stamp.  Ledger 210 made every count carry the geometry it was measured
# in; this is the same rule applied to the reference.
printf 'gnu=%s fingerprint=%s mirror=%s attest=%s\n' \
  "$m_emacs_version" "${m_fingerprint:0:12}" "${m_mirror_commit:0:11}" "$depth"
