#!/usr/bin/env python3
"""
Fifth-pass mechanical fixes for the NeoVM tagged pointer migration.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Fixes applied:
  A. Match-block with_heap elimination:
     Inside `match EXPR.kind() { ... ValueKind::Cons => { ... } }`:
     - with_heap(|h| h.cons_car(ID))      -> EXPR.cons_car()
     - with_heap(|h| h.cons_cdr(ID))      -> EXPR.cons_cdr()
     - with_heap_mut(|h| h.set_car(ID, V)) -> EXPR.set_car(V)
     - with_heap_mut(|h| h.set_cdr(ID, V)) -> EXPR.set_cdr(V)
     - read_cons(ID) -> inline car/cdr
     Similar for String/Vector/Lambda/ByteCode/HashTable/Marker/Overlay arms.

  B. read_cons(ID) outside match blocks:
     - let VAR = read_cons(ID);  // TODO(tagged)...
       -> let VAR_car = CONTEXT.cons_car(); let VAR_cdr = CONTEXT.cons_cdr();
       Then VAR.car -> VAR_car, VAR.cdr -> VAR_cdr

  C. with_heap patterns outside match blocks (in is_cons/is_string guards):
     - with_heap(|h| h.cons_car(ID))  -> GUARDED_EXPR.cons_car()
     - with_heap(|h| h.get_string(ID)) -> GUARDED_EXPR.as_str().unwrap()

  D. ValueKind::Nil / ValueKind::T used as values (E0308):
     - ValueKind::Nil -> Value::NIL
     - ValueKind::T   -> Value::T
     (only when NOT in a match arm pattern position)

  E. Match without .kind():
     - match EXPR { ValueKind::X => ... } -> match EXPR.kind() { ... }

  F. unwrap_or(ValueKind::Nil) -> unwrap_or(Value::NIL)

  G. with_heap(|h| h.get_marker(*id)...) -> v.as_marker_data().unwrap()...
     (outside match blocks, where v is the checked marker value)

  H. with_heap_mut(|h| h.set_car(*c, V)) -> c.set_car(V) (in if is_cons blocks)

Usage:
    python3 scripts/fix_pass5.py            # apply in-place
    python3 scripts/fix_pass5.py --dry-run  # preview changes only
"""

import argparse
import os
import re
import sys
from pathlib import Path


# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SRC_ROOT = Path("crates/neovm-core/src")
EXCLUDED_DIR = SRC_ROOT / "tagged"


# ---------------------------------------------------------------------------
# File discovery
# ---------------------------------------------------------------------------

def find_rs_files(root: Path) -> list:
    """Find all .rs files under root, excluding the tagged/ subdirectory."""
    result = []
    for dirpath, _dirnames, filenames in os.walk(root):
        dp = Path(dirpath)
        if dp == EXCLUDED_DIR or EXCLUDED_DIR in dp.parents:
            continue
        for f in filenames:
            if f.endswith(".rs"):
                result.append(dp / f)
    return sorted(result)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def count_braces(line: str) -> int:
    """Count net brace depth change in a line (ignoring strings/comments simplistically)."""
    depth = 0
    in_string = False
    in_char = False
    prev = ''
    for ch in line:
        if in_string:
            if ch == '"' and prev != '\\':
                in_string = False
        elif in_char:
            if ch == "'" and prev != '\\':
                in_char = False
        else:
            if ch == '"':
                in_string = True
            elif ch == '{':
                depth += 1
            elif ch == '}':
                depth -= 1
        prev = ch
    return depth


def is_match_arm_pattern(line: str) -> bool:
    """Check if a line looks like a match arm pattern (e.g., `ValueKind::Nil =>`)."""
    stripped = line.strip()
    # Match arm patterns end with => or => {
    return bool(re.search(r'^ValueKind::\w+.*=>', stripped))


def is_in_pattern_context(line: str, col: int) -> bool:
    """Check if position is in a match arm pattern (before =>)."""
    arrow_pos = line.find('=>')
    if arrow_pos < 0:
        return False
    return col < arrow_pos


# ---------------------------------------------------------------------------
# Fix A: Match block with_heap elimination
# ---------------------------------------------------------------------------

# Maps (arm_kind, heap_method) -> replacement template
# Template uses {EXPR} for match expression and {ARG1}, {ARG2}, etc. for args
ARM_REPLACEMENTS = {
    # Cons arm
    ('Cons', 'cons_car'): '{EXPR}.cons_car()',
    ('Cons', 'cons_cdr'): '{EXPR}.cons_cdr()',
    ('Cons', 'set_car'): '{EXPR}.set_car({ARG2})',
    ('Cons', 'set_cdr'): '{EXPR}.set_cdr({ARG2})',

    # String arm
    ('String', 'get_string'): '{EXPR}.as_str().unwrap()',
    ('String', 'get_lisp_string'): '{EXPR}.as_lisp_string().unwrap()',
    ('String', 'string_is_multibyte'): '{EXPR}.string_is_multibyte()',

    # Vector arm
    ('Vector', 'get_vector'): '{EXPR}.as_vector_data().unwrap()',
    ('Vector', 'get_vector_mut'): '{EXPR}.as_vector_data_mut().unwrap()',
    ('Vector', 'vector_len'): '{EXPR}.as_vector_data().unwrap().len()',
    ('Vector', 'vector_ref'): '{EXPR}.as_vector_data().unwrap()[{ARG2}]',

    # Record arm
    ('Record', 'get_vector'): '{EXPR}.as_record_data().unwrap()',
    ('Record', 'get_vector_mut'): '{EXPR}.as_record_data_mut().unwrap()',

    # Lambda arm
    ('Lambda', 'get_lambda'): '{EXPR}.get_lambda_data().unwrap()',

    # Macro arm
    ('Macro', 'get_macro_data'): '{EXPR}.get_lambda_data().unwrap()',
    ('Macro', 'get_lambda'): '{EXPR}.get_lambda_data().unwrap()',

    # ByteCode arm
    ('ByteCode', 'get_bytecode'): '{EXPR}.get_bytecode_data().unwrap()',

    # HashTable arm
    ('HashTable', 'get_hash_table'): '{EXPR}.as_hash_table().unwrap()',
    ('HashTable', 'get_hash_table_mut'): '{EXPR}.as_hash_table_mut().unwrap()',

    # Marker arm
    ('Marker', 'get_marker'): '{EXPR}.as_marker_data().unwrap()',

    # Overlay arm
    ('Overlay', 'get_overlay'): '{EXPR}.as_overlay_data().unwrap()',
}

# What arm kind each ValueKind variant maps to
VALUEKIND_TO_ARM = {
    'ValueKind::Cons': 'Cons',
    'ValueKind::String': 'String',
    'ValueKind::Veclike(VecLikeType::Vector)': 'Vector',
    'ValueKind::Veclike(VecLikeType::Record)': 'Record',
    'ValueKind::Veclike(VecLikeType::Lambda)': 'Lambda',
    'ValueKind::Veclike(VecLikeType::Macro)': 'Macro',
    'ValueKind::Veclike(VecLikeType::ByteCode)': 'ByteCode',
    'ValueKind::Veclike(VecLikeType::HashTable)': 'HashTable',
    'ValueKind::Veclike(VecLikeType::Marker)': 'Marker',
    'ValueKind::Veclike(VecLikeType::Overlay)': 'Overlay',
    'ValueKind::Veclike(VecLikeType::Buffer)': 'Buffer',
    'ValueKind::Veclike(VecLikeType::Window)': 'Window',
    'ValueKind::Veclike(VecLikeType::Frame)': 'Frame',
    'ValueKind::Veclike(VecLikeType::Timer)': 'Timer',
}


def extract_match_expr(line: str):
    """Extract the match expression from a 'match EXPR.kind() {' line.
    Returns the expression without .kind(), or None.
    """
    m = re.search(r'\bmatch\s+(.+?)\.kind\(\)\s*\{', line)
    if m:
        expr = m.group(1).strip()
        # Handle &value -> &value (keep the reference as-is in the expr)
        return expr
    return None


def detect_arm_kind(line: str):
    """Detect which ValueKind variant an arm matches.
    Returns arm kind string or None.
    """
    stripped = line.strip()
    for pattern, kind in VALUEKIND_TO_ARM.items():
        if stripped.startswith(pattern):
            return kind
    return None


# Regex for with_heap(|h| h.METHOD(ARGS)) patterns
# Handles both with_heap and with_heap_mut
RE_WITH_HEAP = re.compile(
    r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.(\w+)\(([^)]*)\)\s*(?:\.(\w+)\(\))?\)'
)
RE_WITH_HEAP_MUT = re.compile(
    r'(?:crate::emacs_core::value::)?with_heap_mut\(\|h\|\s*h\.(\w+)\(([^)]*)\)\)'
)

# Regex for with_heap(|h| h.METHOD(ARGS).clone())
RE_WITH_HEAP_CLONE = re.compile(
    r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.(\w+)\(([^)]*)\)\.clone\(\)\)'
)

# Regex for with_heap(|h| h.METHOD(ARGS).to_owned())
RE_WITH_HEAP_TO_OWNED = re.compile(
    r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.(\w+)\(([^)]*)\)\.to_owned\(\)\)'
)

# Regex for read_cons(ID) with TODO comment
RE_READ_CONS = re.compile(
    r'(?:crate::emacs_core::value::)?read_cons\(([^)]+)\)\s*;\s*//\s*TODO\(tagged\).*'
)

# Regex for read_cons(ID) without TODO (less common)
RE_READ_CONS_BARE = re.compile(
    r'(?:crate::emacs_core::value::)?read_cons\(([^)]+)\)'
)

# Regex for "let VAR = read_cons(...);"
RE_LET_READ_CONS = re.compile(
    r'(\s*)let\s+(\w+)\s*=\s*(?:crate::emacs_core::value::)?read_cons\(([^)]+)\)\s*;\s*(?://\s*TODO\(tagged\).*)?'
)


def replace_with_heap_in_arm(line: str, arm_kind: str, match_expr: str) -> str:
    """Replace with_heap/with_heap_mut patterns in a match arm body."""
    if not arm_kind or not match_expr:
        return line

    # Handle with_heap(|h| h.METHOD(ARGS).to_owned())
    def repl_to_owned(m):
        method = m.group(1)
        args = m.group(2).strip()
        key = (arm_kind, method)
        if key in ARM_REPLACEMENTS:
            base = ARM_REPLACEMENTS[key].format(EXPR=match_expr, ARG1=args, ARG2=args)
            return base + '.to_owned()'
        return m.group(0)

    line = RE_WITH_HEAP_TO_OWNED.sub(repl_to_owned, line)

    # Handle with_heap(|h| h.METHOD(ARGS).clone())
    def repl_clone(m):
        method = m.group(1)
        args = m.group(2).strip()
        key = (arm_kind, method)
        if key in ARM_REPLACEMENTS:
            base = ARM_REPLACEMENTS[key].format(EXPR=match_expr, ARG1=args, ARG2=args)
            return base + '.clone()'
        return m.group(0)

    line = RE_WITH_HEAP_CLONE.sub(repl_clone, line)

    # Handle with_heap(|h| h.METHOD(ARGS)) and with_heap(|h| h.METHOD(ARGS).CHAIN())
    def repl_heap(m):
        method = m.group(1)
        args_str = m.group(2).strip()
        chain = m.group(3)  # optional .chain() call
        key = (arm_kind, method)
        if key in ARM_REPLACEMENTS:
            # Parse args for set_car/set_cdr which have 2 args
            args = [a.strip() for a in args_str.split(',')]
            fmt_args = {'EXPR': match_expr}
            for i, a in enumerate(args):
                fmt_args[f'ARG{i+1}'] = a
            result = ARM_REPLACEMENTS[key].format(**fmt_args)
            if chain:
                result += f'.{chain}()'
            return result
        return m.group(0)

    line = RE_WITH_HEAP.sub(repl_heap, line)

    # Handle with_heap_mut(|h| h.METHOD(ARGS))
    def repl_heap_mut(m):
        method = m.group(1)
        args_str = m.group(2).strip()
        args = [a.strip() for a in args_str.split(',')]
        key = (arm_kind, method)
        # Map mut methods
        mut_map = {
            ('Cons', 'set_car'): '{EXPR}.set_car({ARG2})',
            ('Cons', 'set_cdr'): '{EXPR}.set_cdr({ARG2})',
            ('Vector', 'get_vector_mut'): '{EXPR}.as_vector_data_mut().unwrap()',
            ('Record', 'get_vector_mut'): '{EXPR}.as_record_data_mut().unwrap()',
            ('HashTable', 'get_hash_table_mut'): '{EXPR}.as_hash_table_mut().unwrap()',
        }
        if key in mut_map:
            fmt_args = {'EXPR': match_expr}
            for i, a in enumerate(args):
                fmt_args[f'ARG{i+1}'] = a
            return mut_map[key].format(**fmt_args)
        return m.group(0)

    line = RE_WITH_HEAP_MUT.sub(repl_heap_mut, line)

    return line


def process_match_blocks(lines: list) -> tuple:
    """Process match EXPR.kind() blocks, fixing with_heap patterns in arms.

    Returns (new_lines, change_count).
    """
    result = []
    changes = 0
    i = 0

    # Stack for nested matches: [(match_expr, arm_kind, brace_depth_at_match_start)]
    match_stack = []

    # Track brace depth globally
    brace_depth = 0

    while i < len(lines):
        line = lines[i]
        orig_line = line

        # Check for match EXPR.kind() {
        match_expr = extract_match_expr(line)
        if match_expr is not None:
            # Starting a new match block
            depth_before = brace_depth
            brace_depth += count_braces(line)
            match_stack.append({
                'expr': match_expr,
                'arm_kind': None,
                'match_depth': depth_before + 1,  # depth of the match block's opening brace
            })
            result.append(line)
            i += 1
            continue

        # Track brace depth
        depth_change = count_braces(line)

        # Check if we're in a match block
        if match_stack:
            ctx = match_stack[-1]
            match_expr = ctx['expr']

            # Check if this line closes the match block
            new_depth = brace_depth + depth_change
            if new_depth < ctx['match_depth']:
                match_stack.pop()
                brace_depth = new_depth
                result.append(line)
                i += 1
                continue

            # Check for arm pattern
            arm_kind = detect_arm_kind(line)
            if arm_kind is not None:
                ctx['arm_kind'] = arm_kind

            # Apply replacements if we're in a known arm
            if ctx['arm_kind']:
                # Handle read_cons
                m_let_rc = RE_LET_READ_CONS.match(line)
                if m_let_rc:
                    indent = m_let_rc.group(1)
                    var = m_let_rc.group(2)
                    # Replace with inline car/cdr
                    if ctx['arm_kind'] == 'Cons':
                        new_line = (
                            f'{indent}let {var}_car = {match_expr}.cons_car();\n'
                            f'{indent}let {var}_cdr = {match_expr}.cons_cdr();'
                        )
                        result.append(new_line + '\n')
                        changes += 1
                        # Track variable name for subsequent .car/.cdr replacements
                        # Look ahead and replace pair.car -> pair_car, pair.cdr -> pair_cdr
                        i += 1
                        while i < len(lines):
                            next_line = lines[i]
                            next_line = next_line.replace(f'{var}.car', f'{var}_car')
                            next_line = next_line.replace(f'{var}.cdr', f'{var}_cdr')
                            # Check if we've left the arm
                            nd = count_braces(next_line)
                            if brace_depth + depth_change + nd < ctx['match_depth']:
                                break
                            # Check for new arm
                            if detect_arm_kind(next_line) is not None:
                                # Don't consume this line; push it back
                                break
                            # Also apply with_heap replacements
                            next_line = replace_with_heap_in_arm(next_line, ctx['arm_kind'], match_expr)
                            if next_line != lines[i]:
                                changes += 1
                            result.append(next_line)
                            depth_change += nd
                            i += 1
                        brace_depth += depth_change
                        continue
                    else:
                        # For non-Cons arms, read_cons doesn't make sense
                        pass

                # Apply with_heap replacements
                new_line = replace_with_heap_in_arm(line, ctx['arm_kind'], match_expr)
                if new_line != orig_line:
                    changes += 1
                    line = new_line

        brace_depth += depth_change
        result.append(line)
        i += 1

    return result, changes


# ---------------------------------------------------------------------------
# Fix B: read_cons outside match blocks
# ---------------------------------------------------------------------------

def fix_read_cons_outside_match(content: str) -> tuple:
    """Fix read_cons(VAR) patterns outside match blocks.

    Pattern: let VARNAME = read_cons(DEADVAR);  // TODO(tagged)...
    The DEADVAR is a dead ObjId. We need to find what Value it came from.

    Strategy: look for the loop/if context. If the line before has
    `if !EXPR.is_cons() { return ... }` or `EXPR.is_cons()`, use EXPR.

    Also handles:
      let VARNAME = read_cons(DEADVAR);  // TODO(tagged)
      ... VARNAME.car ...
      ... VARNAME.cdr ...
    """
    lines = content.split('\n')
    result = []
    changes = 0
    i = 0

    # Track pending read_cons replacements: var -> (car_name, cdr_name)
    active_replacements = {}

    while i < len(lines):
        line = lines[i]

        # Apply any active .car/.cdr replacements
        for var, (car_name, cdr_name) in list(active_replacements.items()):
            line = line.replace(f'{var}.car', car_name)
            line = line.replace(f'{var}.cdr', cdr_name)

        # Check for read_cons pattern
        m = RE_LET_READ_CONS.match(line)
        if m:
            indent = m.group(1)
            var = m.group(2)
            dead_var = m.group(3).strip()

            # Try to find the context variable by looking backwards
            context_expr = find_cons_context_expr(lines, i, dead_var)

            if context_expr:
                car_name = f'{var}_car'
                cdr_name = f'{var}_cdr'
                new_line = (
                    f'{indent}let {car_name} = {context_expr}.cons_car();\n'
                    f'{indent}let {cdr_name} = {context_expr}.cons_cdr();'
                )
                result.append(new_line)
                active_replacements[var] = (car_name, cdr_name)
                changes += 1
                i += 1
                continue
            # If we can't find context, leave it but fix the immediate line
            # Don't replace, leave as-is with TODO

        # Check for single-expression read_cons usage like: read_cons(*cell).car
        # This is a pattern like `crate::emacs_core::value::read_cons(cell).car`
        m2 = re.search(
            r'(?:crate::emacs_core::value::)?read_cons\(([^)]+)\)\.(car|cdr)',
            line
        )
        if m2 and '// TODO(tagged)' in line:
            dead_var = m2.group(1).strip()
            field = m2.group(2)
            context_expr = find_cons_context_expr(lines, i, dead_var)
            if context_expr:
                method = 'cons_car' if field == 'car' else 'cons_cdr'
                old = m2.group(0)
                new = f'{context_expr}.{method}()'
                line = line.replace(old, new)
                # Remove TODO comment
                line = re.sub(r'\s*//\s*TODO\(tagged\).*', '', line)
                changes += 1

        if line != lines[i]:
            changes += 1
        result.append(line)
        i += 1

    return '\n'.join(result), changes


def find_cons_context_expr(lines: list, line_idx: int, dead_var: str):
    """Look backwards from line_idx to find what Value expression the dead_var
    was extracted from. Returns the expression string or None.

    Looks for patterns like:
      - if EXPR.is_cons() { ... }
      - match EXPR.kind() { ValueKind::Cons => ...
      - while EXPR.is_cons() { ... }
      - if !EXPR.is_cons() { return ...; }  (on the line above the let)
    """
    # Also look for: the dead_var was from a destructure like Value::Cons(dead_var)
    # which got converted to ValueKind::Cons but the dead_var disappeared.

    # Common dead vars: cell, id, first_cell, cell_arc, etc.
    # Look backwards up to 20 lines for an is_cons() check
    for j in range(line_idx - 1, max(line_idx - 25, -1), -1):
        prev = lines[j].strip()

        # Pattern: if !EXPR.is_cons() { return ...
        m = re.search(r'if\s+!(\w[\w.\[\]()]*?)\.is_cons\(\)', prev)
        if m:
            return m.group(1)

        # Pattern: if EXPR.is_cons() {
        m = re.search(r'if\s+(\w[\w.\[\]()]*?)\.is_cons\(\)', prev)
        if m:
            return m.group(1)

        # Pattern: while EXPR.is_cons()
        m = re.search(r'while\s+(\w[\w.\[\]()]*?)\.is_cons\(\)', prev)
        if m:
            return m.group(1)

        # Pattern: match EXPR.kind() {
        m = re.search(r'match\s+(.+?)\.kind\(\)\s*\{', prev)
        if m:
            # We're inside a Cons arm
            return m.group(1).strip()

        # If we hit a function definition or a closing brace at column 0, stop
        if prev.startswith('fn ') or prev.startswith('pub fn ') or prev == '}':
            break

    return None


# ---------------------------------------------------------------------------
# Fix C: with_heap patterns outside match blocks
# ---------------------------------------------------------------------------

def fix_with_heap_outside_match(content: str) -> tuple:
    """Fix with_heap calls that reference dead ObjId variables outside match blocks.

    Patterns:
    - with_heap(|h| h.cons_car(DEAD))  where there's a nearby is_cons check
    - with_heap(|h| h.cons_cdr(DEAD))
    - with_heap(|h| h.get_string(DEAD)) where there's a nearby is_string check
    - with_heap(|h| h.get_string(DEAD).to_owned())
    - with_heap(|h| h.get_marker(DEAD)...) where there's a nearby is_marker check
    """
    lines = content.split('\n')
    result = []
    changes = 0

    for i, line in enumerate(lines):
        orig_line = line

        # with_heap(|h| h.cons_car(DEAD))
        for method, replacement_method in [('cons_car', 'cons_car'), ('cons_cdr', 'cons_cdr')]:
            pattern = re.compile(
                rf'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.{method}\((\w[\w.*]*)\)\)'
            )
            m = pattern.search(line)
            if m:
                dead_var = m.group(1)
                ctx = find_value_context(lines, i, 'cons', dead_var)
                if ctx:
                    line = line[:m.start()] + f'{ctx}.{replacement_method}()' + line[m.end():]
                    changes += 1

        # with_heap(|h| h.get_string(DEAD).to_owned())
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_string\((\w[\w.*]*)\)\.to_owned\(\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'string', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_str().unwrap().to_owned()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_string(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_string\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'string', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_str().unwrap()' + line[m.end():]
                changes += 1

        # with_heap_mut(|h| h.set_car(DEAD, VAL))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap_mut\(\|h\|\s*h\.set_car\((\w[\w.*]*),\s*([^)]+)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            val = m.group(2).strip()
            ctx = find_value_context(lines, i, 'cons', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.set_car({val})' + line[m.end():]
                changes += 1

        # with_heap_mut(|h| h.set_cdr(DEAD, VAL))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap_mut\(\|h\|\s*h\.set_cdr\((\w[\w.*]*),\s*([^)]+)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            val = m.group(2).strip()
            ctx = find_value_context(lines, i, 'cons', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.set_cdr({val})' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_vector(DEAD).clone())
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_vector\((\w[\w.*]*)\)\.clone\(\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'vector', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_vector_data().unwrap().clone()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_vector(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_vector\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'vector', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_vector_data().unwrap()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_lambda(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_lambda\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'lambda', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.get_lambda_data().unwrap()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_lambda(DEAD).clone())
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_lambda\((\w[\w.*]*)\)\.clone\(\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'lambda', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.get_lambda_data().unwrap().clone()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_bytecode(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_bytecode\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'bytecode', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.get_bytecode_data().unwrap()' + line[m.end():]
                changes += 1

        # with_heap(|h| h.get_hash_table(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|h\|\s*h\.get_hash_table\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'hash_table', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_hash_table().unwrap()' + line[m.end():]
                changes += 1

        # with_heap_mut(|h| h.get_hash_table_mut(DEAD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap_mut\(\|h\|\s*h\.get_hash_table_mut\((\w[\w.*]*)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            ctx = find_value_context(lines, i, 'hash_table', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_hash_table_mut().unwrap()' + line[m.end():]
                changes += 1

        if line != orig_line:
            # Don't double count
            pass
        result.append(line)

    return '\n'.join(result), changes


def find_value_context(lines: list, line_idx: int, type_hint: str, dead_var: str):
    """Look backwards to find the Value expression that the dead ObjId was extracted from.

    type_hint: 'cons', 'string', 'vector', 'lambda', 'bytecode', 'hash_table', 'marker'
    """
    check_methods = {
        'cons': ['is_cons'],
        'string': ['is_string'],
        'vector': ['is_vector'],
        'lambda': ['is_lambda', 'is_function'],
        'bytecode': ['is_bytecode'],
        'hash_table': ['is_hash_table'],
        'marker': ['is_marker'],
        'overlay': ['is_overlay'],
    }
    methods = check_methods.get(type_hint, [])

    for j in range(line_idx - 1, max(line_idx - 30, -1), -1):
        prev = lines[j].strip()

        # Check for is_TYPE() checks
        for method in methods:
            # Pattern: if !EXPR.is_X() { return ...
            m = re.search(rf'if\s+!(\w[\w.\[\]()]*?)\.{method}\(\)', prev)
            if m:
                return m.group(1)

            # Pattern: if EXPR.is_X()
            m = re.search(rf'if\s+(\w[\w.\[\]()]*?)\.{method}\(\)', prev)
            if m:
                return m.group(1)

            # Pattern: while EXPR.is_X()
            m = re.search(rf'while\s+(\w[\w.\[\]()]*?)\.{method}\(\)', prev)
            if m:
                return m.group(1)

        # Check for match EXPR.kind() (we're probably in an arm)
        m = re.search(r'match\s+(.+?)\.kind\(\)\s*\{', prev)
        if m:
            return m.group(1).strip()

        # If we hit a function definition, stop
        if prev.startswith('fn ') or prev.startswith('pub fn ') or (prev == '}' and j < line_idx - 3):
            break

    return None


# ---------------------------------------------------------------------------
# Fix D: ValueKind::Nil/T used as values (not in patterns)
# ---------------------------------------------------------------------------

def fix_valuekind_as_value(content: str) -> tuple:
    """Replace ValueKind::Nil and ValueKind::T used as VALUES (not match patterns)
    with Value::NIL and Value::T.

    E.g.:
    - return Ok(ValueKind::Nil)  -> return Ok(Value::NIL)
    - unwrap_or(ValueKind::Nil)  -> unwrap_or(Value::NIL)
    - RuntimeBindingValue::Bound(ValueKind::Nil) -> RuntimeBindingValue::Bound(Value::NIL)
    - buf.set_buffer_local("...", ValueKind::T)  -> buf.set_buffer_local("...", Value::T)
    - vec![..., ValueKind::T]    -> vec![..., Value::T]
    - == ValueKind::T            -> == Value::T

    Must NOT replace in match arm patterns (before =>).
    """
    lines = content.split('\n')
    result = []
    changes = 0

    for line in lines:
        orig_line = line

        # Skip lines that look like match arm patterns
        stripped = line.strip()
        if re.match(r'^ValueKind::(Nil|T)\b.*=>', stripped):
            result.append(line)
            continue

        # Also skip if it's a match-arm-continuation pattern
        # Be conservative: only replace in known value contexts

        # Replace ValueKind::Nil in value positions
        # Strategy: replace all ValueKind::Nil that are NOT before =>
        arrow_pos = line.find('=>')
        if arrow_pos >= 0:
            # Only replace after the =>
            before = line[:arrow_pos + 2]
            after = line[arrow_pos + 2:]
            after = after.replace('ValueKind::Nil', 'Value::NIL')
            after = after.replace('ValueKind::T', 'Value::T')
            line = before + after
        else:
            line = line.replace('ValueKind::Nil', 'Value::NIL')
            line = line.replace('ValueKind::T', 'Value::T')

        if line != orig_line:
            changes += 1
        result.append(line)

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix E: match EXPR { ValueKind::X => ... } without .kind()
# ---------------------------------------------------------------------------

def fix_match_without_kind(content: str) -> tuple:
    """Fix cases where match is on a Value but arms use ValueKind patterns.

    Pattern: match EXPR {
                 ValueKind::Fixnum(n) => ...
    This is wrong because EXPR is a TaggedValue, not a ValueKind.
    Fix: match EXPR.kind() {

    Also handles: match &EXPR { ValueKind::... => ... }
    Fix: match EXPR.kind() {
    """
    lines = content.split('\n')
    result = []
    changes = 0
    i = 0

    while i < len(lines):
        line = lines[i]

        # Look for: match EXPR {
        m = re.match(r'^(\s*)(.*)match\s+(.+?)\s*\{(.*)$', line)
        if m and '.kind()' not in line:
            indent = m.group(1)
            prefix = m.group(2)
            expr = m.group(3).strip()
            suffix = m.group(4)

            # Only add .kind() if the expression looks like a Value.
            # Be conservative: only handle patterns we're confident about.
            clean = expr.lstrip('&*')
            is_value_expr = False

            # Simple variable: value, car, cdr, arg, etc.
            if re.match(r'^[a-zA-Z_]\w*$', clean):
                is_value_expr = True

            # Array/slice index: args[0], slots[1], etc.
            if re.match(r'^[a-zA-Z_]\w*\[', clean):
                is_value_expr = True

            # Dereference of indexing: *slots.first()?
            if re.match(r'^\*?[a-zA-Z_]\w*\.(first|last)\(\)\??$', clean):
                is_value_expr = True

            # Method chain ending with known Value-returning method
            if re.search(r'\.(cons_car|cons_cdr|car|cdr)\(\)$', clean):
                is_value_expr = True

            # Tuple of expressions: (car.kind(), cdr.kind()) - already has .kind()
            # but (car, cdr) without .kind() might need it
            if clean.startswith('(') and clean.endswith(')') and ',' in clean:
                # Tuple match - skip, too complex
                is_value_expr = False

            if is_value_expr:
                # Check if the next non-empty line starts with ValueKind::
                for j in range(i + 1, min(i + 5, len(lines))):
                    next_stripped = lines[j].strip()
                    if not next_stripped:
                        continue
                    if next_stripped.startswith('ValueKind::'):
                        # This match needs .kind()
                        # Strip & from the expression if present
                        if expr.startswith('&'):
                            expr = expr[1:]
                        elif expr.startswith('*'):
                            # *expr -> (*expr) -- keep deref
                            pass
                        line = f'{indent}{prefix}match {expr}.kind() {{{suffix}'
                        changes += 1
                    break

        result.append(line)
        i += 1

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix E2: Value::NIL / Value::T used in match arm patterns (before =>)
# ---------------------------------------------------------------------------

def fix_value_nil_t_in_patterns(content: str) -> tuple:
    """Fix Value::NIL and Value::T used as match arm patterns.

    Inside a `match EXPR.kind() { ... }` block, patterns must be ValueKind
    variants, not Value constants. So:
      Value::NIL =>  ->  ValueKind::Nil =>
      Value::T =>    ->  ValueKind::T =>

    We detect these by checking if a line with Value::NIL/Value::T before =>
    is inside a match .kind() block.
    """
    lines = content.split('\n')
    result = []
    changes = 0
    in_kind_match = 0  # depth counter for nested match .kind() blocks
    brace_depth = 0
    kind_match_depths = []  # stack of brace depths at which .kind() matches started

    for line in lines:
        orig_line = line

        # Track match .kind() { starts
        if re.search(r'\bmatch\s+.+\.kind\(\)\s*\{', line):
            in_kind_match += 1
            kind_match_depths.append(brace_depth + count_braces(line))

        depth_change = count_braces(line)

        # Check if we've closed the current kind match
        if kind_match_depths:
            new_depth = brace_depth + depth_change
            while kind_match_depths and new_depth < kind_match_depths[-1]:
                kind_match_depths.pop()
                in_kind_match -= 1

        # If we're inside a match .kind() block, fix Value::NIL/T in pattern positions
        if in_kind_match > 0:
            stripped = line.strip()
            # Check for Value::NIL before =>
            if re.match(r'Value::NIL\s*=>', stripped):
                line = line.replace('Value::NIL', 'ValueKind::Nil', 1)
                changes += 1
            elif re.match(r'Value::T\s*=>', stripped):
                line = line.replace('Value::T', 'ValueKind::T', 1)
                changes += 1
            # Also handle: (ValueKind::X, Value::NIL) => patterns in tuple matches
            elif '=>' in stripped:
                arrow_pos = line.find('=>')
                before_arrow = line[:arrow_pos]
                if 'Value::NIL' in before_arrow:
                    line = before_arrow.replace('Value::NIL', 'ValueKind::Nil') + line[arrow_pos:]
                    changes += 1
                if 'Value::T' in before_arrow:
                    arrow_pos2 = line.find('=>')
                    before_arrow2 = line[:arrow_pos2]
                    line = before_arrow2.replace('Value::T', 'ValueKind::T') + line[arrow_pos2:]
                    changes += 1

        brace_depth += depth_change
        result.append(line)

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix F: unwrap_or(ValueKind::Nil) and similar
# (Now handled by Fix D, but let's be explicit)
# ---------------------------------------------------------------------------


# ---------------------------------------------------------------------------
# Fix G: with_heap marker patterns
# ---------------------------------------------------------------------------

def fix_marker_with_heap(content: str) -> tuple:
    """Fix with_heap(|heap| heap.get_marker(*id)...) patterns.

    These appear in marker.rs where functions take &Value but used to destructure
    the ObjId. Now *id is a dead variable.

    Pattern: with_heap(|heap| heap.get_marker(*id).FIELD)
    -> v.as_marker_data().unwrap().FIELD

    where v is the function parameter (usually named `v`).
    """
    lines = content.split('\n')
    result = []
    changes = 0
    pending_close_paren = False  # Track when we need to remove a closing })

    for i, line in enumerate(lines):
        orig_line = line

        # If we're looking for the closing }) from a multi-line with_heap match
        if pending_close_paren:
            stripped = line.strip()
            if stripped == '})':
                line = line.replace('})', '}')
                pending_close_paren = False
                changes += 1
                result.append(line)
                continue

        # with_heap(|heap| heap.get_marker(*id).clone())
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|heap\|\s*heap\.get_marker\(\*id\)\.clone\(\)\)',
            line
        )
        if m:
            ctx = find_marker_context_param(lines, i)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_marker_data().unwrap().clone()' + line[m.end():]
                changes += 1

        # with_heap(|heap| heap.get_marker(*id).FIELD)
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|heap\|\s*heap\.get_marker\(\*id\)\.(\w+)\)',
            line
        )
        if m:
            field = m.group(1)
            ctx = find_marker_context_param(lines, i)
            if ctx:
                line = line[:m.start()] + f'{ctx}.as_marker_data().unwrap().{field}' + line[m.end():]
                changes += 1

        # with_heap(|heap| match heap.get_marker(*id).FIELD { ... })
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|heap\|\s*match\s+heap\.get_marker\(\*id\)\.(\w+)',
            line
        )
        if m:
            field = m.group(1)
            ctx = find_marker_context_param(lines, i)
            if ctx:
                old = f'with_heap(|heap| match heap.get_marker(*id).{field}'
                new = f'match {ctx}.as_marker_data().unwrap().{field}'
                line = line.replace(old, new)
                # Check if closing }) is on this line
                if '})' in line:
                    line = line.replace('})', '}', 1)
                else:
                    # The closing }) is on a later line
                    pending_close_paren = True
                changes += 1

        # with_heap(|heap| Value::bool_val(heap.get_marker(*id).FIELD))
        m = re.search(
            r'(?:crate::emacs_core::value::)?with_heap\(\|heap\|\s*Value::bool_val\(heap\.get_marker\(\*id\)\.(\w+)\)\)',
            line
        )
        if m:
            field = m.group(1)
            ctx = find_marker_context_param(lines, i)
            if ctx:
                line = line[:m.start()] + f'Value::bool_val({ctx}.as_marker_data().unwrap().{field})' + line[m.end():]
                changes += 1

        # with_heap(|heap| heap.get_marker(*id).buffer)
        # Already covered by the generic .FIELD pattern above

        # with_heap_mut(|heap| { heap.get_marker_mut(*id).FIELD = VAL; })
        # This is multi-line, skip for now

        if line != orig_line:
            pass  # already counted
        result.append(line)

    return '\n'.join(result), changes


def find_marker_context_param(lines: list, line_idx: int):
    """Find the marker Value parameter name by looking at the function signature."""
    for j in range(line_idx - 1, max(line_idx - 30, -1), -1):
        prev = lines[j].strip()
        # Look for function parameter like: v: &Value
        m = re.search(r'fn\s+\w+\([^)]*(\w+)\s*:\s*&Value', prev)
        if m:
            return m.group(1)
        # Also check for is_marker check
        m = re.search(r'if\s+!?(\w+)\.is_marker\(\)', prev)
        if m:
            return m.group(1)
        if prev.startswith('fn ') or prev.startswith('pub fn '):
            break
    return None


# ---------------------------------------------------------------------------
# Fix H: with_heap_mut(|h| h.set_car/set_cdr(*c, V)) in is_cons blocks
# ---------------------------------------------------------------------------

def fix_set_car_cdr_in_if_blocks(content: str) -> tuple:
    """Fix with_heap_mut(|h| h.set_car(*VAR, VAL)) where VAR is a dead ObjId.

    These appear in if VAR.is_cons() blocks in the bytecode VM.
    The check variable is 'cell' but the dead var is '*c'.

    Pattern: with_heap_mut(|h| h.set_car(*c, newcar))
    -> cell.set_car(newcar)  (where cell is the is_cons-checked variable)
    """
    lines = content.split('\n')
    result = []
    changes = 0

    for i, line in enumerate(lines):
        orig_line = line

        # with_heap_mut(|h| h.set_car(DEAD, VAL))
        m = re.search(
            r'with_heap_mut\(\|h\|\s*h\.set_car\((\*?\w+),\s*([^)]+)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            val = m.group(2).strip()
            ctx = find_value_context(lines, i, 'cons', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.set_car({val})' + line[m.end():]
                changes += 1

        # with_heap_mut(|h| h.set_cdr(DEAD, VAL))
        m = re.search(
            r'with_heap_mut\(\|h\|\s*h\.set_cdr\((\*?\w+),\s*([^)]+)\)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            val = m.group(2).strip()
            ctx = find_value_context(lines, i, 'cons', dead_var)
            if ctx:
                line = line[:m.start()] + f'{ctx}.set_cdr({val})' + line[m.end():]
                changes += 1

        if line != orig_line:
            pass
        result.append(line)

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix I: with_heap(|h| !eq_value(&h.cons_car(DEAD), &value))
# ---------------------------------------------------------------------------

def fix_eq_value_with_heap(content: str) -> tuple:
    """Fix with_heap patterns that wrap eq_value with cons access.

    Pattern: with_heap(|h| !eq_value(&h.cons_car(DEAD), &value))
    -> !eq_value(&EXPR.cons_car(), &value)
    """
    lines = content.split('\n')
    result = []
    changes = 0

    for i, line in enumerate(lines):
        orig_line = line

        # with_heap(|h| !eq_value(&h.cons_car(DEAD), &EXPR))
        m = re.search(
            r'with_heap\(\|h\|\s*(!?)eq_value\(&h\.(cons_car|cons_cdr)\((\w+)\),\s*(&[^)]+)\)\)',
            line
        )
        if m:
            neg = m.group(1)
            method = m.group(2)
            dead_var = m.group(3)
            other_arg = m.group(4)
            ctx = find_value_context(lines, i, 'cons', dead_var)
            if ctx:
                line = line[:m.start()] + f'{neg}eq_value(&{ctx}.{method}(), {other_arg})' + line[m.end():]
                changes += 1

        if line != orig_line:
            pass
        result.append(line)

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix J: with_heap(|h| h.get_string(*sid).to_owned()) in match String arms
# where *sid is actually a dead SymId dereference
# ---------------------------------------------------------------------------

def fix_string_deref_patterns(content: str) -> tuple:
    """Fix patterns like with_heap(|h| h.get_string(*sid).to_owned()) where
    *sid is a dead variable from the old pattern match.

    This handles the case in marker.rs where the code does:
    match args[2].kind() {
        ValueKind::String => {
            let name = with_heap(|h| h.get_string(*sid).to_owned());
    """
    # This is already handled by fix_with_heap_outside_match and the match block processor
    return content, 0


# ---------------------------------------------------------------------------
# Fix K: with_heap_mut(|h| *h.get_vector_mut(V) = values)
# ---------------------------------------------------------------------------

def fix_vector_mut_assignment(content: str) -> tuple:
    """Fix with_heap_mut(|h| *h.get_vector_mut(DEAD) = values).

    -> *EXPR.as_vector_data_mut().unwrap() = values
    """
    lines = content.split('\n')
    result = []
    changes = 0

    for i, line in enumerate(lines):
        orig_line = line

        m = re.search(
            r'with_heap_mut\(\|h\|\s*\*h\.get_vector_mut\((\*?\w+)\)\s*=\s*([^)]+)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            rhs = m.group(2).strip()
            ctx = find_value_context(lines, i, 'vector', dead_var)
            if ctx:
                line = line[:m.start()] + f'*{ctx}.as_vector_data_mut().unwrap() = {rhs}' + line[m.end():]
                changes += 1

        # Similarly for hash tables: with_heap_mut(|h| *h.get_hash_table_mut(DEAD) = ht)
        m = re.search(
            r'with_heap_mut\(\|h\|\s*\*h\.get_hash_table_mut\((\*?\w+)\)\s*=\s*([^)]+)\)',
            line
        )
        if m:
            dead_var = m.group(1)
            rhs = m.group(2).strip()
            ctx = find_value_context(lines, i, 'hash_table', dead_var)
            if ctx:
                line = line[:m.start()] + f'*{ctx}.as_hash_table_mut().unwrap() = {rhs}' + line[m.end():]
                changes += 1

        if line != orig_line:
            pass
        result.append(line)

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Fix L: match &value { ValueKind::String => ... } (E0308 from matching &TaggedValue)
# This is where code does: match &value { ValueKind::X => ... }
# The & makes it &TaggedValue but pattern is ValueKind
# ---------------------------------------------------------------------------

def fix_match_ref_value(content: str) -> tuple:
    """Fix match &EXPR { ValueKind::... => ... }.

    The & produces &TaggedValue but arms expect ValueKind.
    Fix: match EXPR.kind() { ... }
    """
    # This is already handled by fix_match_without_kind
    return content, 0


# ---------------------------------------------------------------------------
# Fix M: with_heap multiline patterns (overlay, etc.)
# These involve with_heap(|h| { ... }) spanning multiple lines
# ---------------------------------------------------------------------------

# Too complex for mechanical fixing. Skip.


# ---------------------------------------------------------------------------
# Fix N: *other in catch-all arms where other binds to ValueKind
# but code uses it as a Value
# ---------------------------------------------------------------------------

def _replace_catch_all_var(text: str, var: str, match_expr: str) -> str:
    """Replace a catch-all variable with the match expression in a line fragment.

    Handles: &var, *var, and bare var (when not in format strings).
    """
    # Replace &var
    text = re.sub(rf'&{re.escape(var)}\b', f'&{match_expr}', text)
    # Replace *var
    text = re.sub(rf'\*{re.escape(var)}\b', match_expr, text)
    # Replace bare var in non-format contexts
    if (f'{{{var}' not in text and f'{var}:?' not in text
            and f'"{var}"' not in text):
        text = re.sub(rf'\b{re.escape(var)}\b', match_expr, text)
    return text


def fix_catch_all_other_as_value(content: str) -> tuple:
    """In match EXPR.kind() { ... other => ... *other ... },
    `other` is a ValueKind, not a Value. Using *other or &other is wrong.

    Replace *other with EXPR (the match expression).
    Replace &other with &EXPR.
    Replace other where a Value is expected with EXPR.

    This was partially done in pass4 but there are remaining cases.
    """
    # This is complex. Let's only handle the specific case of:
    # match EXPR.kind() { ... other => { ... &other ... } }
    # where &other should be &EXPR or EXPR
    lines = content.split('\n')
    result = []
    changes = 0
    i = 0

    match_stack = []
    brace_depth = 0

    while i < len(lines):
        line = lines[i]
        orig_line = line

        # Check for match EXPR.kind() {
        match_expr = extract_match_expr(line)
        if match_expr is not None:
            depth_before = brace_depth
            brace_depth += count_braces(line)
            match_stack.append({
                'expr': match_expr,
                'match_depth': depth_before + 1,
                'in_catch_all': False,
                'catch_all_var': None,
            })
            result.append(line)
            i += 1
            continue

        depth_change = count_braces(line)

        if match_stack:
            ctx = match_stack[-1]
            new_depth = brace_depth + depth_change
            if new_depth < ctx['match_depth']:
                match_stack.pop()
                brace_depth = new_depth
                result.append(line)
                i += 1
                continue

            # Check for catch-all arm: `other => {` or `_ => {`
            stripped = line.strip()
            m = re.match(r'^(\w+)\s*=>\s*\{?', stripped)
            if m and not stripped.startswith('ValueKind::'):
                var = m.group(1)
                if var != '_':
                    ctx['in_catch_all'] = True
                    ctx['catch_all_var'] = var

            # Check for named ValueKind arm pattern that also binds
            # like: `other => { ... }` where other matches all remaining
            if ctx['in_catch_all'] and ctx['catch_all_var']:
                var = ctx['catch_all_var']
                match_expr = ctx['expr']

                # For the arm pattern line (e.g., `other => { body }`),
                # only replace in the body part (after =>).
                # For non-pattern lines, replace everywhere.
                arm_pattern_m = re.match(rf'^(\s*{re.escape(var)}\s*=>)(.*)', line)
                if arm_pattern_m:
                    # This is the arm pattern line; only fix the body part
                    pattern_part = arm_pattern_m.group(1)
                    body_part = arm_pattern_m.group(2)
                    body_part = _replace_catch_all_var(body_part, var, match_expr)
                    line = pattern_part + body_part
                else:
                    line = _replace_catch_all_var(line, var, match_expr)

            # Reset catch-all tracking on new arm
            if stripped.startswith('ValueKind::'):
                ctx['in_catch_all'] = False
                ctx['catch_all_var'] = None

        if line != orig_line:
            changes += 1

        brace_depth += depth_change
        result.append(line)
        i += 1

    return '\n'.join(result), changes


# ---------------------------------------------------------------------------
# Master processing pipeline
# ---------------------------------------------------------------------------

def process_file(filepath: Path, dry_run: bool) -> int:
    """Process a single file through all fix passes. Returns change count."""
    content = filepath.read_text()
    original = content
    total_changes = 0

    # Fix E: match without .kind() (must come before match-block processing)
    content, n = fix_match_without_kind(content)
    total_changes += n

    # Fix D: ValueKind::Nil/T as values
    content, n = fix_valuekind_as_value(content)
    total_changes += n

    # Fix A: Match block with_heap elimination
    lines = content.split('\n')
    # Need to add newlines back since process_match_blocks works on lines with \n
    lines_with_nl = [l + '\n' for l in lines]
    if lines_with_nl:
        lines_with_nl[-1] = lines[-1]  # last line might not have trailing newline
        if content.endswith('\n'):
            lines_with_nl[-1] = lines[-1] + '\n'
    new_lines, n = process_match_blocks(lines_with_nl)
    content = ''.join(new_lines)
    total_changes += n

    # Fix B: read_cons outside match blocks
    content, n = fix_read_cons_outside_match(content)
    total_changes += n

    # Fix C: with_heap outside match blocks
    content, n = fix_with_heap_outside_match(content)
    total_changes += n

    # Fix G: marker with_heap patterns
    content, n = fix_marker_with_heap(content)
    total_changes += n

    # Fix H: set_car/set_cdr in if blocks
    content, n = fix_set_car_cdr_in_if_blocks(content)
    total_changes += n

    # Fix I: eq_value with cons access
    content, n = fix_eq_value_with_heap(content)
    total_changes += n

    # Fix K: vector/hash_table mut assignment
    content, n = fix_vector_mut_assignment(content)
    total_changes += n

    # Fix N: catch-all other as value
    content, n = fix_catch_all_other_as_value(content)
    total_changes += n

    # Fix E2: Value::NIL / Value::T in match arm patterns
    content, n = fix_value_nil_t_in_patterns(content)
    total_changes += n

    if content != original:
        if dry_run:
            print(f"  [DRY RUN] Would modify: {filepath}")
        else:
            filepath.write_text(content)
            print(f"  Modified: {filepath}")
        return total_changes
    return 0


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Fix E0425/E0308 errors from tagged pointer migration (pass 5)"
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Preview changes without modifying files',
    )
    args = parser.parse_args()

    if not SRC_ROOT.exists():
        print(f"Error: {SRC_ROOT} not found. Run from the project root.", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(SRC_ROOT)
    print(f"Scanning {len(files)} .rs files...")

    total_changes = 0
    modified_files = 0

    for filepath in files:
        n = process_file(filepath, args.dry_run)
        if n > 0:
            total_changes += n
            modified_files += 1

    action = "Would modify" if args.dry_run else "Modified"
    print(f"\nDone. {action} {modified_files} files with {total_changes} changes.")


if __name__ == "__main__":
    main()
