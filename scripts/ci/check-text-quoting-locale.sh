#!/usr/bin/env bash
# Prove that a Neomacs binary resolves a nil `text-quoting-style` from the same
# two inputs GNU reads, agreeing with a pinned GNU byte for byte.
#
#     scripts/ci/check-text-quoting-locale.sh <neomacs> <gnu-emacs>
#
# GNU's `Ftext_quoting_style` (`src/doc.c:652-678`) asks
# `default_to_grave_quoting_style` (`src/doc.c:638-650`) when the variable is
# nil.  That function reads `text_quoting_flag`, which `main` sets from the
# process locale's charset (`src/emacs.c:1653-1656`, `using_utf8` at
# `src/emacs.c:414-436`), and then the standard display table's U+2018 entry
# (`src/doc.c:644-649`).  The cases below pin each input:
#
#   * LC_ALL=C in batch        -> flag nil  -> grave, `` `x' ``
#   * LC_ALL=en_US.UTF-8 batch -> flag set, no display table -> curve, ‘x’
#   * an explicit display table whose U+2018 entry is [?`] -> grave
#
# A TTY that cannot display curved quotes needs a pty and is proven separately
# (`startup--setup-quote-display` installs a homoglyph-faced glyph code, which
# doc.c does NOT treat as the backtick entry, so the interactive answer stays
# curve with the ASCII substitution in the display table).  This script is the
# batch half and runs unchanged on Linux and macOS.
set -euo pipefail

readonly usage="usage: $0 <neomacs> <gnu-emacs>"

if [[ $# -ne 2 ]]; then
    echo "$usage" >&2
    exit 2
fi

readonly neomacs=$1
readonly gnu=$2

fail() {
    echo "check-text-quoting-locale: $*" >&2
    exit 1
}

[[ -x $neomacs ]] || fail "$neomacs is not executable"
[[ -x $gnu ]] || fail "no GNU Emacs oracle at $gnu (the job must fail, not skip)"

# The default answer of `(text-quoting-style)' plus the message requoting every
# format-message performs (`src/editfns.c` Fformat_message -> Ftext_quoting_style).
# The message is built with (string 96) and (string 39) so this script needs no
# backtick or apostrophe of its own -- both are load-bearing shell characters.
readonly default_form='(prin1 (list (text-quoting-style) (format-message (concat (string 96) "x" (string 39)))))'
# The display-table input, with the flag set explicitly so the case does not
# depend on the ambient locale.
readonly table_form='(progn (setq standard-display-table (make-display-table)) (aset standard-display-table #x2018 (vector 96)) (setq text-quoting-style nil) (prin1 (list (text-quoting-style) (format-message (concat (string 96) "x" (string 39))))))'

# $1 = locale, $2 = editor, $3 = form; prints the editor's stdout.
run_case() {
    local locale=$1 editor=$2 form=$3
    env -u LANG -u LC_CTYPE LC_ALL="$locale" "$editor" -Q --batch --eval "$form"
}

check_case() {
    local name=$1 locale=$2 form=$3 want=$4
    local gnu_out neo_out
    gnu_out=$(run_case "$locale" "$gnu" "$form") || fail "$name: GNU Emacs failed"
    neo_out=$(run_case "$locale" "$neomacs" "$form") || fail "$name: Neomacs failed"
    [[ $gnu_out == "$want" ]] ||
        fail "$name: the pinned GNU did not answer '$want' (got '$gnu_out'); the oracle is not the characterized build"
    [[ $neo_out == "$gnu_out" ]] ||
        fail "$name: Neomacs answered '$neo_out' where GNU answered '$gnu_out'"
    echo "check-text-quoting-locale: $name ok: $neo_out"
}

# \140 is ` and \047 is '; printf writes the exact prin1 form to compare.
readonly grave_answer=$(printf '(grave "\140x\047")')
readonly curve_answer='(curve "‘x’")'

check_case "C-locale batch" C "$default_form" "$grave_answer"
check_case "UTF-8-locale batch" en_US.UTF-8 "$default_form" "$curve_answer"
check_case "display-table backtick" C "$table_form" "$grave_answer"

echo "check-text-quoting-locale: neomacs matches GNU in all 3 cases"
