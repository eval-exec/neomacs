#!/usr/bin/env bash
# Ledger 205 (ledger 204's discriminator).  A `--no-byte-compile` fresh build
# REGENERATES some `.el` files after the previous build byte-compiled them, so
# their `.elc` ends up older and ledger 202's refusal fires.  Lifting an mtime
# is only legitimate when the regenerated source is byte-identical to the one
# the `.elc` was compiled from -- otherwise it hides exactly the defect ledger
# 202 built that refusal for.
#
# The comparison tree is the main checkout, whose copies were produced by a
# DIFFERENT build at a different time: if the two agree byte for byte, the
# generator is deterministic and the `.elc` really does implement the `.el`.
#
#   scripts/l205-lift-generated-elc.sh [--dry-run]
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
compare_root="/home/exec/Projects/github.com/eval-exec/neomacs"
dry="${1:-}"
cd "$root" || exit 1
identical=0
different=0
missing=0
while read -r f; do
  el="${f%.elc}.el"
  [ -f "$el" ] || continue
  [ "$el" -nt "$f" ] || continue
  other="$compare_root/$el"
  if [ ! -f "$other" ]; then
    echo "NO COMPARISON COPY: $el"
    missing=$((missing+1))
    continue
  fi
  if cmp -s "$el" "$other"; then
    identical=$((identical+1))
    [ "$dry" = "--dry-run" ] || touch "$f"
  else
    echo "DIFFERENT: $el"
    different=$((different+1))
  fi
done < <(find lisp -name '*.elc')
echo "identical=$identical different=$different no_comparison=$missing dry_run=${dry:-no}"
