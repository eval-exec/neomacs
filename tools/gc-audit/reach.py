#!/usr/bin/env python3
"""Ledger 163: reverse-reachability to a GC safepoint over a name-level call
graph of neovm-core / neomacs-bin / neomacs-layout-engine.

Approximation, stated plainly: the graph is keyed by BARE FUNCTION NAME, so two
distinct `fn read` in different modules are one node.  That over-approximates
reachability (never under-approximates it, except for calls made through trait
objects / function pointers / macros, which are invisible here).  The point of
the run is to measure how LARGE the reachable set is, i.e. whether "can this
call reach a safepoint" is a discriminating question in an interpreter at all.
"""

import os
import re
import sys
import json
import collections

from gcaudit_root import ROOT, require_nonzero  # noqa: E402

FN_RE = re.compile(r'^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?'
                   r'(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?'
                   r'fn\s+([A-Za-z_][A-Za-z0-9_]*)')
CALL_RE = re.compile(r'([A-Za-z_][A-Za-z0-9_]*)\s*\(')

KEYWORDS = {
    'if', 'while', 'for', 'match', 'return', 'fn', 'let', 'else', 'loop', 'in',
    'as', 'move', 'ref', 'mut', 'impl', 'where', 'unsafe', 'dyn', 'self', 'Self',
    'assert', 'assert_eq', 'assert_ne', 'debug_assert', 'debug_assert_eq',
    'debug_assert_ne', 'panic', 'write', 'writeln', 'format', 'vec', 'println',
    'eprintln', 'todo', 'unimplemented', 'matches', 'Some', 'Ok', 'Err', 'None',
}

# Seeds: the functions whose BODY contains a safepoint (measured).
SEEDS = {
    'gc_safe_point', 'gc_safe_point_exact', 'gc_collect_from_current_roots',
    'gc_collect_from_current_roots_impl', 'maybe_gc_and_quit',
    'bytecode_branch_maybe_gc_and_quit',
    'eval_sub', 'apply_internal', 'apply_with_frame_function',
}


def build():
    defs = collections.defaultdict(list)   # name -> [(file, start, end)]
    calls = collections.defaultdict(set)   # name -> {callee names}
    fn_count = 0
    for crate in ('crates/neovm-core/src', 'crates/neomacs/src', 'crates/neomacs-layout-engine/src'):
        for dirpath, _dirs, files in os.walk(os.path.join(ROOT, crate)):
            for fname in sorted(files):
                if not fname.endswith('.rs'):
                    continue
                path = os.path.join(dirpath, fname)
                rel = os.path.relpath(path, ROOT)
                with open(path, encoding='utf-8') as fh:
                    lines = fh.read().split('\n')
                i = 0
                while i < len(lines):
                    m = FN_RE.match(lines[i])
                    if not m:
                        i += 1
                        continue
                    name = m.group(2)
                    depth = 0
                    started = False
                    end = i
                    for j in range(i, len(lines)):
                        for ch in lines[j]:
                            if ch == '{':
                                depth += 1
                                started = True
                            elif ch == '}':
                                depth -= 1
                        end = j
                        if started and depth <= 0:
                            break
                    body = '\n'.join(lines[i:end + 1])
                    fn_count += 1
                    defs[name].append((rel, i + 1, end + 1))
                    for c in CALL_RE.finditer(body):
                        cn = c.group(1)
                        if cn in KEYWORDS or cn == name:
                            continue
                        calls[name].add(cn)
                    i = end + 1
    return defs, calls, fn_count


def main():
    defs, calls, fn_count = build()
    # reverse edges
    rev = collections.defaultdict(set)
    for caller, callees in calls.items():
        for c in callees:
            rev[c].add(caller)
    reach = set(s for s in SEEDS if s in defs or s in rev)
    work = list(reach)
    while work:
        n = work.pop()
        for caller in rev.get(n, ()):
            if caller not in reach:
                reach.add(caller)
                work.append(caller)
    require_nonzero('fn definitions', fn_count)
    print(f"distinct fn names defined : {len(defs)}", file=sys.stderr)
    print(f"fn definitions total      : {fn_count}", file=sys.stderr)
    print(f"names reaching a safepoint: {len(reach)} "
          f"({100.0 * len(reach) / max(1, len(defs)):.1f}% of defined names)",
          file=sys.stderr)
    json.dump(sorted(reach), sys.stdout)


if __name__ == '__main__':
    main()
