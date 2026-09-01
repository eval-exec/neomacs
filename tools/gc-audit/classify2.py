#!/usr/bin/env python3
"""Ledger 163: classify every `as_lisp_string` / `expect_lisp_string` borrow
site by whether the BORROW ITSELF is named and, if so, whether its live range
spans a call that can reach a GC safepoint or mutate the string.

Classes
  OWNED   the accessor's result is immediately converted to owned data
          (`.clone()`, `.to_vec()`, `.to_owned()`, a scalar like `.schars()`),
          so no borrow survives the expression.
  INLINE  the borrow is not bound to a name; it dies at the end of its
          statement.  At risk only if that one statement itself runs Lisp.
  BOUND   the borrow is bound to a name (`let`, `if let`, `&& let`, `match`
          arm, closure parameter).  Its live range is [site, last textual use
          of the name inside the enclosing fn]; that range is scanned.
"""

import os
import re
import sys
import json

from gcaudit_root import ROOT  # noqa: E402  (validated workspace root)

ACCESSOR = re.compile(r'\b(as_lisp_string|expect_lisp_string)\b')
FN_RE = re.compile(r'^(\s*)(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?'
                   r'(?:const\s+)?(?:async\s+)?(?:unsafe\s+)?(?:extern\s+"[^"]*"\s+)?'
                   r'fn\s+([A-Za-z_][A-Za-z0-9_]*)')

DANGER = re.compile(
    r'\b('
    r'eval|eval_sub|eval_sub_cons|eval_value|eval_body|eval_form|eval_string|eval_progn'
    r'|eval_value_with_lexical_arg|eval_and_dispatch'
    r'|apply|apply0|apply1|apply2|apply_internal|apply_untraced|apply_with_frame_function'
    r'|funcall|funcall_general|funcall_general_untraced|call_function|call0|call1|call2|call3|call4'
    r'|call_interactively|command_execute|execute_command|call_lisp'
    r'|macroexpand|macroexpand_all|macroexpand_1'
    r'|unbind_to|unbind_to_result|unbind_to_with_result'
    r'|run_hook[a-z_]*|run_window_[a-z_]*|safe_call[0-9]?|safe_funcall'
    r'|gc_safe_point[a-z_]*|garbage_collect[a-z_]*|maybe_gc[a-z_]*|gc_collect[a-z_]*'
    r'|dispatch_signal[a-z_]*|signal_hook[a-z_]*'
    r'|force_mode_line_update|redisplay[a-z_]*'
    r'|read_from_minibuffer|completing_read|read_minibuffer'
    r'|exec_byte_code|execute_bytecode|run_bytecode|exec_bytecode'
    r')\s*[(\[]'
)

MUTATE = re.compile(r'\b(with_lisp_string_mut|mutate_bytes|set_from_str'
                    r'|replace_owned_payload|builtin_aset|aset_string_replacement)\s*\(')

# Terminal conversions that yield owned data.
OWNING = re.compile(
    r'\.(clone|cloned|to_vec|to_owned|to_string|as_str_owned|as_runtime_string_owned'
    r'|schars|sbytes|len|is_multibyte|is_empty|is_ascii|char_at|byte_at'
    r'|as_utf8_str_owned)\s*\(\s*\)\s*[;,)?]?\s*$'
)

IDENT = r'[A-Za-z_][A-Za-z0-9_]*'


def fn_extent(lines, idx):
    for i in range(idx, -1, -1):
        m = FN_RE.match(lines[i])
        if not m:
            continue
        depth = 0
        started = False
        end = len(lines) - 1
        for j in range(i, len(lines)):
            for ch in lines[j]:
                if ch == '{':
                    depth += 1
                    started = True
                elif ch == '}':
                    depth -= 1
            if started and depth <= 0:
                end = j
                break
        if i <= idx <= end:
            return (m.group(2), i, end)
    return (None, 0, len(lines) - 1)


def statement_extent(lines, idx, fn_end):
    depth = 0
    out = []
    for j in range(idx, min(fn_end + 1, len(lines))):
        out.append(j)
        for ch in lines[j]:
            if ch in '([{':
                depth += 1
            elif ch in ')]}':
                depth -= 1
        s = lines[j].rstrip()
        if depth <= 0 and (s.endswith(';') or s.endswith('{') or s.endswith('}')
                           or s.endswith(',')):
            break
    return out


def bound_names(line, lines, idx, fn_end):
    """Identifiers the accessor's result is bound to on this line."""
    names = []
    # `let PAT = ... accessor ...`  (covers plain let, if let, while let, && let)
    for m in re.finditer(r'\blet\s+([^=]{0,80}?)\s*=', line):
        pat = m.group(1)
        for nm in re.findall(IDENT, pat):
            if nm in ('mut', 'Some', 'Ok', 'None', 'Err', 'ref', 'else'):
                continue
            names.append(nm)
    # closure parameter on the same line: `.map(|s| ...)`
    for m in re.finditer(r'\|\s*(' + IDENT + r')\s*\|', line):
        names.append(m.group(1))
    # `match X.as_lisp_string() {` -> Some(name) arms in the following block
    if re.search(r'\bmatch\b', line) and line.rstrip().endswith('{'):
        depth = 0
        for j in range(idx, min(fn_end + 1, len(lines))):
            for ch in lines[j]:
                if ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
            if j > idx:
                for m in re.finditer(r'\b(?:Some|Ok)\s*\(\s*(' + IDENT + r')\s*\)\s*=>', lines[j]):
                    names.append(m.group(1))
            if depth <= 0 and j > idx:
                break
    return [n for n in dict.fromkeys(names)]


def main():
    results = []
    total_grep_lines = 0
    for crate in ('crates/neovm-core/src', 'crates/neomacs/src', 'crates/neomacs-layout-engine/src'):
        for dirpath, _dirs, files in os.walk(os.path.join(ROOT, crate)):
            for fname in sorted(files):
                if not fname.endswith('.rs'):
                    continue
                path = os.path.join(dirpath, fname)
                rel = os.path.relpath(path, ROOT)
                with open(path, encoding='utf-8') as fh:
                    lines = fh.read().split('\n')
                in_test_mod = False
                test_mod_depth = 0
                depth = 0
                for idx, line in enumerate(lines):
                    stripped = line.strip()
                    if re.match(r'#\[cfg\(test\)\]', stripped):
                        in_test_mod = True
                        test_mod_depth = depth
                    depth += line.count('{') - line.count('}')
                    if in_test_mod and depth <= test_mod_depth and '}' in line:
                        in_test_mod = False
                    if not ACCESSOR.search(line):
                        continue
                    total_grep_lines += 1
                    if stripped.startswith('//'):
                        results.append({'file': rel, 'line': idx + 1, 'cls': 'COMMENT'})
                        continue
                    if re.search(r'\bfn\s+(as_lisp_string|expect_lisp_string)\b', line):
                        results.append({'file': rel, 'line': idx + 1, 'cls': 'DEFN'})
                        continue
                    fn_name, _fn_start, fn_end = fn_extent(lines, idx)
                    is_test = (bool(re.search(r'(_test\.rs|/tests\.rs|_tests\.rs)', rel))
                               or in_test_mod)
                    names = bound_names(line, lines, idx, fn_end)
                    owning = bool(OWNING.search(line.rstrip()))
                    if owning and names:
                        cls = 'OWNED'
                        span = [idx]
                    elif names:
                        cls = 'BOUND'
                        last = idx
                        for nm in names:
                            pat = re.compile(r'\b' + re.escape(nm) + r'\b')
                            for j in range(idx + 1, fn_end + 1):
                                if pat.search(lines[j]):
                                    last = max(last, j)
                        span = list(range(idx, last + 1))
                    else:
                        cls = 'INLINE'
                        span = statement_extent(lines, idx, fn_end)
                    body = '\n'.join(lines[j] for j in span)
                    danger = sorted({m.group(1) for m in DANGER.finditer(body)})
                    mutate = sorted({m.group(1) for m in MUTATE.finditer(body)})
                    results.append({
                        'file': rel, 'line': idx + 1, 'fn': fn_name, 'cls': cls,
                        'names': names, 'span': len(span), 'last': span[-1] + 1,
                        'danger': danger, 'mutate': mutate, 'test': is_test,
                        'text': stripped[:170],
                    })
    json.dump({'grep_lines': total_grep_lines, 'sites': results}, sys.stdout, indent=1)


if __name__ == '__main__':
    main()
