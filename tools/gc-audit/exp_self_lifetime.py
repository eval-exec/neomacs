#!/usr/bin/env python3
"""The red measurement for DIVERGENCES.md 167: LOOSEN `Value::as_lisp_string`
back to `(self) -> Option<&'static LispString>` and count what stops being
checked.

Entry 163 ran this in the other direction, to price the tightening before
landing it: 20 compile errors, 7 genuine escapes and 13 temporaries. 167 landed
it, so the tree now holds the honest signature and this script is the way to
re-measure the cost of giving it up.

    python3 tools/gc-audit/exp_self_lifetime.py     # loosen; then cargo check
    python3 tools/gc-audit/exp_self_lifetime.py --revert

With the `'static` restored, `cargo check --workspace --all-targets` is clean
again -- every escape the honest signature rejects typechecks -- and the
`compile_fail` doctest on `Value::as_lisp_string` stops failing. That pair is
the property this entry bought: not a bug fixed, a class of mistake that stops
compiling.
"""
import os
import sys

from gcaudit_root import ROOT  # noqa: E402  (validated workspace root)

P = os.path.join(ROOT, 'crates/neovm-core/src/emacs_core/runtime/value/mod.rs')
HONEST = "    pub fn as_lisp_string(&self) -> Option<&LispString> {"
LOOSE = "    pub fn as_lisp_string(self) -> Option<&'static LispString> {"

s = open(P, encoding='utf-8').read()
revert = len(sys.argv) > 1 and sys.argv[1] == '--revert'
a, b = (LOOSE, HONEST) if revert else (HONEST, LOOSE)
if a not in s:
    print(f"MISS: {a!r} not found")
    sys.exit(1)
open(P, 'w', encoding='utf-8').write(s.replace(a, b, 1))
print('restored the honest signature' if revert else 'loosened to &\'static')
