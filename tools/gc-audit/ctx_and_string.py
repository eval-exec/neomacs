#!/usr/bin/env python3
"""Ledger 163: the sharpest syntactic signature of "a string borrow is live
while Lisp may run" is a function that takes BOTH `&mut Context` and a
`&LispString`: the callee owns a safepoint-capable receiver and a borrow into
the heap at the same time."""
import os
import re

from gcaudit_root import ROOT  # noqa: E402  (validated workspace root)
FN = re.compile(r'^\s*(?:pub(?:\([^)]*\))?\s+)?(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?'
                r'fn\s+([A-Za-z_]\w*)')
CTX = re.compile(r'&\s*mut\s+(?:[A-Za-z_:]*::)?Context\b')
STR = re.compile(r'&\s*(?:\'\w+\s+)?(?:crate::heap_types::|heap_types::)?LispString\b')

hits = []
for crate in ('crates/neovm-core/src', 'crates/neomacs/src', 'crates/neomacs-layout-engine/src'):
    for dp, _d, fs in os.walk(os.path.join(ROOT, crate)):
        for f in sorted(fs):
            if not f.endswith('.rs'):
                continue
            p = os.path.join(dp, f)
            rel = os.path.relpath(p, ROOT)
            lines = open(p, encoding='utf-8').read().split('\n')
            for i, line in enumerate(lines):
                m = FN.match(line)
                if not m:
                    continue
                sig, depth, started = [], 0, False
                for j in range(i, min(i + 30, len(lines))):
                    sig.append(lines[j])
                    for ch in lines[j]:
                        if ch == '(':
                            depth += 1
                            started = True
                        elif ch == ')':
                            depth -= 1
                    if started and depth <= 0:
                        break
                s = '\n'.join(sig)
                if CTX.search(s) and STR.search(s):
                    hits.append((rel, i + 1, m.group(1)))
print(len(hits))
for h in hits:
    print(f"{h[0]}:{h[1]} {h[2]}")
