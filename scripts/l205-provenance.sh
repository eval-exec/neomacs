#!/usr/bin/env bash
# Ledger 205 provenance check on a release binary, per the brief:
#   (documentation-property 'dos-codepage 'variable-documentation) must be nil
#   *scratch* must be empty (point-max = 1)
#   the .pdump must be newer than the binary beside it
set -u
root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$root" || exit 1
export RUST_LOG=error
./target/release/neomacs --batch --no-site-file --no-site-lisp --eval \
  '(progn (princ (format "docprop=%S\n" (documentation-property (quote dos-codepage) (quote variable-documentation)))) (princ (format "scratch-pmax=%S\n" (with-current-buffer "*scratch*" (point-max)))))'
echo "binary mtime = $(stat -c %Y target/release/neomacs)"
echo "pdump  mtime = $(stat -c %Y target/release/neomacs.pdump)"
