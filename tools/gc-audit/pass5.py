#!/usr/bin/env python3
"""Ledger 163 pass 5 — the final at-risk list.

Refines pass 4 with two corrections the earlier passes got wrong:

  * `expect_lisp_string` is SIX different functions. Five of them
    (bookmark/dired/lread/minibuffer/reader, plus fileio's `_strict`) return an
    OWNED `LispString`; only `lisp/native/builtins/mod.rs`'s and
    `emacs_core/text/search/mod.rs`'s
    return a borrow. A call that reaches an owned one is not a borrow site.
  * `let x = <borrow>;` immediately followed by `let x = x.clone();` — the
    codebase's dominant idiom at any boundary that runs Lisp — leaves an owned
    value under the same name, so the borrow dies on the next line.
"""
import json
import os
import re

from gcaudit_root import ROOT  # noqa: E402  (validated workspace root)
sites = json.load(open(os.path.join(ROOT, 'tmp/sites2.json')))['sites']
ctxstr = {l.strip() for l in open(os.path.join(ROOT, 'tmp/ctxstr-names.txt')) if l.strip()}

OWNED_DEFS = {
    'crates/neovm-core/src/emacs_core/editing/bookmark/mod.rs',
    'crates/neovm-core/src/emacs_core/editing/dired/mod.rs',
    'crates/neovm-core/src/emacs_core/lisp/lread/mod.rs',
    'crates/neovm-core/src/emacs_core/commands/minibuffer/mod.rs',
    'crates/neovm-core/src/emacs_core/lisp/reader/mod.rs',
}
CTX_CALL = re.compile(
    r'\b[A-Za-z_]\w*\s*\(\s*(?:&mut\s+)?(ctx|eval|evaluator|context)\b'
    r'|\b(ctx|eval|evaluator|context)\s*\.\s*[A-Za-z_]\w*\s*\(')
CTXSTR_CALL = re.compile(r'\b(' + '|'.join(sorted(ctxstr)) + r')\s*\(')

cache = {}


def flines(rel):
    if rel not in cache:
        cache[rel] = open(os.path.join(ROOT, rel), encoding='utf-8').read().split('\n')
    return cache[rel]


def is_borrow(x):
    t = x['text']
    if 'as_lisp_string' in t:
        return True
    if 'expect_lisp_string_strict' in t or 'expect_lisp_filename_string_strict' in t:
        return False
    if 'builtins::expect_lisp_string' in t:
        return True
    return x['file'] not in OWNED_DEFS


def cloned_next_line(x):
    """`let n = <borrow>;` then `let n = n.clone();` — owned from line 2."""
    if not x['names']:
        return False
    lines = flines(x['file'])
    idx = x['line']          # 0-based index of the NEXT line
    if idx >= len(lines):
        return False
    nxt = lines[idx]
    for n in x['names']:
        if re.search(r'\blet\s+(?:mut\s+)?' + re.escape(n) + r'\s*=\s*' + re.escape(n)
                     + r'\s*\.\s*(clone|cloned|to_vec|to_owned)\s*\(', nxt):
            return True
    return False


bound = [x for x in sites if x['cls'] == 'BOUND' and not x['test']]
bound_borrow = [x for x in bound if is_borrow(x)]
bound_borrow_live = [x for x in bound_borrow if not cloned_next_line(x)]

rows = []
for x in bound_borrow_live:
    lines = flines(x['file'])
    body = '\n'.join(lines[x['line']:x['last']])
    ctx_calls = sorted({(m.group(1) or m.group(2)) for m in CTX_CALL.finditer(body)})
    ctxstr_calls = sorted({m.group(1) for m in CTXSTR_CALL.finditer(body)})
    inline_clone = bool(re.search(r'\.\s*(clone|cloned)\s*\(\s*\)', x['text']))
    if (ctx_calls or ctxstr_calls or x['danger'] or x['mutate']) and not inline_clone:
        rows.append({**x, 'ctx_calls': ctx_calls, 'ctxstr_calls': ctxstr_calls})

print(f"production BOUND sites                        : {len(bound)}")
print(f"  ... that really take a borrow               : {len(bound_borrow)}")
print(f"  ... whose borrow is not cloned on line 2    : {len(bound_borrow_live)}")
print(f"  ... and whose live range holds the evaluator: {len(rows)}")
print()
for r in rows:
    print(f"{r['file']}:{r['line']}-{r['last']} ({r['span']}L) fn={r['fn']} "
          f"names={r['names']} ctx={r['ctx_calls']} ctxstr={r['ctxstr_calls']} "
          f"danger={r['danger']} mutate={r['mutate']}")
    print('    ' + r['text'])
