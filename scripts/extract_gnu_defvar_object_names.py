#!/usr/bin/env python3
"""Extract GNU Emacs `DEFVAR_LISP' / `DEFVAR_KBOARD' declarations into Rust.

Companion to `extract_gnu_defvar_docs.py', which takes the doc: text of every
DEFVAR.  This one takes the *other* half of the declaration: the fact that GNU
declared the name in C at all, and which `Lisp_Fwd' variant that gives the
symbol.

    DEFVAR_LISP      ("name", Vsym, doc: ...)  ->  Lisp_Fwd_Obj
    DEFVAR_LISP_NOPRO("name", Vsym, doc: ...)  ->  Lisp_Fwd_Obj
    DEFVAR_KBOARD    ("name", field, doc: ...) ->  Lisp_Fwd_Kboard_Obj

Both macros set `redirect = SYMBOL_FORWARDED' (`src/lread.c:5275' and
`src/lread.c:5296'), which is what `set_internal' consults when it refuses an
unbind (`src/data.c:1802-1809'), what `Fdefvaralias' consults when it refuses a
built-in variable as a new alias (`src/eval.c:665-668'), and what
`Fmake_local_variable' consults when it refuses a keyboard variable
(`src/data.c:2287-2290').

DEFVAR_INT / DEFVAR_BOOL / DEFVAR_PER_BUFFER are deliberately NOT emitted: this
port already declares those through `defvar_bool.rs',
`Obarray::define_int_variable' and `BUFFER_SLOT_INFO' respectively.

Usage:
    scripts/extract_gnu_defvar_object_names.py \\
        --gnu-src /path/to/emacs-mirror/emacs/src \\
        --output  crates/neovm-core/src/emacs_core/runtime/defvar_object/gnu_table.rs
"""

import argparse
import re
import sys
from pathlib import Path

DECL = re.compile(
    r'\bDEFVAR_(LISP_NOPRO|LISP|KBOARD)\s*\(\s*"([^"]+)"',
)

KIND = {
    "LISP": "Global",
    "LISP_NOPRO": "Global",
    "KBOARD": "Keyboard",
}

# GNU spells "and now take `declared_special' back off again" two different
# ways, and a generator that knows only one of them is how ledger 176's
# `features' divergence got in.  Both are matched here.
#
#   1. A call to the Lisp-visible primitive:
#        Fmake_var_non_special (Qfeatures);            src/fns.c:6823
#   2. A direct store through a symbol the line names:
#        XSYMBOL (Qtop_level)->u.s.declared_special = false;
#                                                      src/keyboard.c:13955
#        XBARE_SYMBOL (intern ("values"))->u.s.declared_special = false;
#                                                      src/lread.c:5596
NON_SPECIAL_CALL = re.compile(r"\bFmake_var_non_special\s*\(\s*([A-Za-z_]\w*)\s*\)")
NON_SPECIAL_STORE = re.compile(
    r"\bX(?:BARE_)?SYMBOL\s*\(\s*(.+?)\s*\)\s*->\s*u\.s\.declared_special\s*=\s*false",
)
# The subject forms a store site can name a symbol with.  Anything else is a
# C variable holding a symbol chosen at runtime, which by construction cannot
# be a per-name exception -- see `unresolved' handling in `collect_non_special'.
SUBJECT_QSYM = re.compile(r"^(Q\w+)$")
SUBJECT_INTERN = re.compile(r'^intern\s*\(\s*"([^"]+)"\s*\)$')
DEFSYM = re.compile(r'\bDEFSYM\s*\(\s*(Q\w+)\s*,\s*"([^"]+)"\s*\)')

IF_OPEN = re.compile(r"^\s*#\s*(?:if|ifdef|ifndef)\b")
IF_ZERO = re.compile(r"^\s*#\s*if\s+0\b")
IF_ELSE = re.compile(r"^\s*#\s*el(?:se|if)\b")
IF_CLOSE = re.compile(r"^\s*#\s*endif\b")


def strip_if_zero(text: str) -> str:
    """Blank out every `#if 0' ... `#else'/`#endif' region, keeping offsets.

    A generator that scrapes C text sees declarations the compiler never
    does.  GNU has seven `DEFVAR_LISP' heads parked inside `#if 0', and one of
    them -- `echo-area-clear-hook' (`src/keyboard.c:14059') -- is in a file
    this port DOES have the variable for, so the spurious row made the symbol
    `SYMBOL_FORWARDED' and `special' here while GNU leaves it an ordinary
    plain Lisp variable.  Measured, `-Q --batch':
    `(list (boundp 'echo-area-clear-hook) (special-variable-p ...))'
    answers `(t nil)' in GNU and answered `(t t)' here, and `makunbound' was
    refused where GNU allows it (ledger 183).

    Same failure class as ledger 176's `features' and ledger 173's extractor
    bugs: the fix belongs in the generator, not in a hand-maintained list of
    exceptions beside it.  Replacement preserves every byte offset (only
    non-newline characters are blanked) so `file:line' comments stay true.

    `#ifdef'/`#ifndef'/`#if <cond>' are deliberately NOT evaluated -- those
    guard platform files whose names belong in the table (`w32fns.c`'s
    `x-pointer-shape` is also declared in `xfns.c`), and `adopt` reports a
    name this build lacks as `Absent` rather than acting on it.  `#if 0' is
    different in kind: it is dead in EVERY build.
    """
    out = []
    depth = 0
    dead_at = None
    for line in text.split("\n"):
        if IF_OPEN.match(line):
            depth += 1
            if dead_at is None and IF_ZERO.match(line):
                dead_at = depth
        elif IF_CLOSE.match(line):
            if dead_at is not None and depth == dead_at:
                dead_at = None
            depth = max(0, depth - 1)
        elif IF_ELSE.match(line):
            if dead_at is not None and depth == dead_at:
                dead_at = None
        out.append(" " * len(line) if dead_at is not None else line)
    return "\n".join(out)


def read_live_source(path: Path) -> str:
    """The C text a compiler would see, minus `#if 0' regions."""
    return strip_if_zero(path.read_text(encoding="utf-8", errors="replace"))


def collect(src: Path):
    """name -> (kind, "file:line"), keeping the first declaration seen."""
    found = {}
    for path in sorted(src.glob("*.c")):
        text = read_live_source(path)
        for match in DECL.finditer(text):
            macro, name = match.group(1), match.group(2)
            if name in found:
                continue
            line = text.count("\n", 0, match.start()) + 1
            found[name] = (KIND[macro], f"{path.name}:{line}")
    return found


def collect_defsyms(src: Path):
    """Qidentifier -> Lisp name, from GNU's own `DEFSYM' table."""
    syms = {}
    for path in sorted(src.glob("*.c")):
        text = read_live_source(path)
        for match in DEFSYM.finditer(text):
            syms.setdefault(match.group(1), match.group(2))
    return syms


def collect_non_special(src: Path, defsyms):
    """name -> "file:line" for every name GNU un-declares as special.

    Returns `(found, unresolved)`.  `unresolved` holds the store sites whose
    subject is a C variable rather than a symbol literal: `src/alloc.c`'s
    `p->u.s.declared_special = false' initialising a freshly allocated symbol,
    and `src/eval.c`'s `XSYMBOL (symbol)->u.s.declared_special = false' -- the
    body of `internal-make-var-non-special' itself, clearing the flag on
    whatever symbol it was handed at runtime.  Neither NAMES a symbol, which
    is exactly what disqualifies them: a per-name exception has to have a name.
    """
    found = {}
    unresolved = []
    for path in sorted(src.glob("*.c")):
        text = read_live_source(path)

        def site(match):
            return f"{path.name}:{text.count(chr(10), 0, match.start()) + 1}"

        for match in NON_SPECIAL_CALL.finditer(text):
            qsym = match.group(1)
            if qsym not in defsyms:
                raise SystemExit(
                    f"{site(match)}: Fmake_var_non_special ({qsym}) names a symbol "
                    f"with no DEFSYM; the Q-identifier map is incomplete."
                )
            found.setdefault(defsyms[qsym], site(match))

        for match in NON_SPECIAL_STORE.finditer(text):
            subject = match.group(1)
            if qsym := SUBJECT_QSYM.match(subject):
                if qsym.group(1) not in defsyms:
                    raise SystemExit(
                        f"{site(match)}: {subject} names a symbol with no DEFSYM; "
                        f"the Q-identifier map is incomplete."
                    )
                found.setdefault(defsyms[qsym.group(1)], site(match))
            elif literal := SUBJECT_INTERN.match(subject):
                found.setdefault(literal.group(1), site(match))
            else:
                unresolved.append((site(match), subject))
    return found, unresolved


HEADER = '''\
// AUTO-GENERATED by scripts/extract_gnu_defvar_object_names.py -- DO NOT EDIT.
//
// Source: GNU Emacs `src/*.c` DEFVAR_LISP / DEFVAR_LISP_NOPRO / DEFVAR_KBOARD
// declarations.  Re-run the extractor against an updated GNU mirror to
// refresh.  Each row is a name GNU's C declares, so the symbol is
// `SYMBOL_FORWARDED` there; the trailing comment is the GNU `file:line`.
//
// The table is the DECLARATION, not a measurement of this port: a name whose
// C file this build does not compile is still listed, and
// `adopt_gnu_object_forwarders` simply finds no symbol to adopt.

// Each row also carries whether GNU KEEPS the symbol special.  `DEFVAR_*`
// sets `declared_special` unconditionally (`src/lread.c:5274`), and three
// names in GNU's `src/` have it taken straight back off again; the second
// column records that on the declaration itself so the two halves cannot
// drift apart.  A `NonSpecial` row's trailing comment carries both sites.

use super::{GnuObjectForward, GnuObjectVariable, GnuSpecialness};

use GnuObjectForward::Global as G;
use GnuObjectForward::Keyboard as K;
use GnuSpecialness::NonSpecial as N;
use GnuSpecialness::Special as S;

#[rustfmt::skip]
pub(crate) static GNU_OBJECT_VARIABLES: &[GnuObjectVariable] = &[
'''

FOOTER = "];\n"


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("--gnu-src", required=True, type=Path)
    ap.add_argument("--output", required=True, type=Path)
    args = ap.parse_args()

    found = collect(args.gnu_src)
    defsyms = collect_defsyms(args.gnu_src)
    non_special, unresolved = collect_non_special(args.gnu_src, defsyms)

    # A name GNU un-declares but never declared would mean this script's model
    # of GNU is wrong, not that GNU has an undeclared exception.  Say so rather
    # than dropping the row on the floor.
    orphans = sorted(set(non_special) - set(found))
    if orphans:
        raise SystemExit(
            "un-declared as special but never DEFVAR'd: "
            + ", ".join(f"{name} ({non_special[name]})" for name in orphans)
        )

    rows = []
    for name in sorted(found):
        kind, site = found[name]
        short = "G" if kind == "Global" else "K"
        if name in non_special:
            special, comment = "N", f"{site} non-special at {non_special[name]}"
        else:
            special, comment = "S", site
        rows.append(
            f'    GnuObjectVariable {{ name: r#"{name}"#, kind: {short},'
            f" special: {special} }}, // {comment}\n"
        )

    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(HEADER + "".join(rows) + FOOTER, encoding="utf-8")

    globals_n = sum(1 for k, _ in found.values() if k == "Global")
    kboard_n = sum(1 for k, _ in found.values() if k == "Keyboard")
    print(f"{len(found)} names ({globals_n} Lisp_Objfwd, {kboard_n} Lisp_Kboard_Objfwd)"
          f" -> {args.output}", file=sys.stderr)
    print(f"{len(non_special)} un-declared as special: "
          + ", ".join(f"{n} ({non_special[n]})" for n in sorted(non_special)),
          file=sys.stderr)
    for site, subject in unresolved:
        print(f"  skipped {site}: `{subject}' is not a symbol literal", file=sys.stderr)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
