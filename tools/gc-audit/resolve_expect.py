#!/usr/bin/env python3
"""Ledger 163: not every `expect_lisp_string` call is a BORROW.

Six modules define a local `expect_lisp_string` that returns an OWNED
`LispString` (a clone). Only `lisp/native/builtins/mod.rs`'s and
`emacs_core/text/search/mod.rs`'s
return a reference into the heap. Resolve each call site to the definition it
actually reaches, so the audit counts borrows rather than spellings.
"""
import json
import os
import re

from gcaudit_root import ROOT  # noqa: E402  (validated workspace root)
sites = json.load(open(os.path.join(ROOT, 'tmp/sites2.json')))['sites']

DEF = re.compile(r'fn\s+(expect_lisp_string\w*)\s*\(')
RET_BORROW = re.compile(r'->\s*(Option|Result)\s*<\s*&')

# Discover every definition and whether it hands back a borrow.
defs = {}   # file -> {name: borrows?}
for dp, _d, fs in os.walk(os.path.join(ROOT, 'crates/neovm-core/src')):
    for f in sorted(fs):
        if not f.endswith('.rs'):
            continue
        p = os.path.join(dp, f)
        rel = os.path.relpath(p, ROOT)
        lines = open(p, encoding='utf-8').read().split('\n')
        for i, line in enumerate(lines):
            m = DEF.search(line)
            if not m:
                continue
            sig = '\n'.join(lines[i:i + 8])
            defs.setdefault(rel, {})[m.group(1)] = bool(RET_BORROW.search(sig))

print("definitions:")
for rel, names in sorted(defs.items()):
    for n, borrows in sorted(names.items()):
        print(f"  {'BORROW' if borrows else 'OWNED '}  {rel}  {n}")

borrow_sites = 0
owned_sites = 0
per_class = {}
for x in sites:
    if x['cls'] in ('COMMENT', 'DEFN') or x['test']:
        continue
    text = x['text']
    if 'as_lisp_string' in text:
        borrow_sites += 1
        per_class[x['cls']] = per_class.get(x['cls'], [0, 0])
        per_class[x['cls']][0] += 1
        continue
    # an expect_lisp_string* call: which definition does it reach?
    name = 'expect_lisp_string'
    m = re.search(r'\b(expect_lisp_\w+)\s*\(', text)
    if m:
        name = m.group(1)
    local = defs.get(x['file'], {})
    if name in local:
        borrows = local[name]
    elif 'builtins::expect_lisp_string' in text or 'builtins::expect' in text:
        borrows = True
    else:
        # imported: `builtins/mod.rs`'s is the only importable one
        borrows = True
    per_class.setdefault(x['cls'], [0, 0])
    if borrows:
        borrow_sites += 1
        per_class[x['cls']][0] += 1
    else:
        owned_sites += 1
        per_class[x['cls']][1] += 1

print()
print(f"production sites that really take a BORROW : {borrow_sites}")
print(f"production sites that get an OWNED clone   : {owned_sites}")
print("by class [borrow, owned]:", per_class)
