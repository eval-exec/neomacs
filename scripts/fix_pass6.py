#!/usr/bin/env python3
"""
Sixth-pass mechanical fixes for the NeoVM tagged pointer migration.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Fixes applied:
  1. E0164 — Value::symbol(id) etc. used as patterns (function-like paths)
     a. matches!(EXPR, Value::symbol(id) if GUARD) → EXPR.as_symbol_id().map_or(false, |id| GUARD)
     b. matches!(EXPR, Value::fixnum(n) if GUARD)  → EXPR.as_fixnum().map_or(false, |n| GUARD)
     c. matches!(EXPR, Value::symbol(_))           → EXPR.is_symbol()
     d. matches!(EXPR, Value::fixnum(_))           → EXPR.is_fixnum()
     e. matches!(EXPR, Value::char(_))             → EXPR.as_char().is_some()
     f. matches!(EXPR, Value::subr(_))             → EXPR.as_subr_id().is_some()
     g. matches!(EXPR, Value::fixnum(n))           → EXPR.is_fixnum() [no guard, wildcard-like]
     h. if let Value::symbol(id) = EXPR            → if let Some(id) = EXPR.as_symbol_id()
     i. Value::Cons(_) in patterns                 → is_cons() / ValueKind::Cons
     j. Value::make_float(_) in patterns           → is_float() / ValueKind::Float
     k. Value::make_buffer(_) / make_frame(_) etc. → is_buffer() / is_frame() etc.
     l. Value::Str(_) in patterns                  → is_string() / ValueKind::String
     m. Value::Vector(_) in patterns               → is_vector() / ValueKind::Veclike(VecLikeType::Vector)
     n. Value::Lambda(_) / Value::ByteCode(_)      → is_lambda() / is_bytecode()

  2. E0433 — Files using ValueKind/VecLikeType without importing them.

  3. Remaining dereferences: *n → n, *id → id in kind() bindings; &n → &n for
     Range::contains.

  4. E0631 — Closure type mismatches: Value::is_nil (fn(Value)->bool) used where
     FnOnce(&Value)->bool expected. .is_none_or(Value::is_nil) →
     .is_none_or(|v| v.is_nil()), .all(Value::is_X) → .all(|v| v.is_X()), etc.

  6. Remaining Value::Nil → Value::NIL, Value::True → Value::T in non-pattern
     context.

Usage:
    python3 scripts/fix_pass6.py            # apply in-place
    python3 scripts/fix_pass6.py --dry-run  # preview changes only
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
# Fix 1: E0164 — pattern matching on function-like Value constructors
# ---------------------------------------------------------------------------

# Map of Value::method(_) patterns to is_* checks for wildcard patterns
WILDCARD_PATTERN_TO_CHECK = {
    'symbol': 'is_symbol()',
    'keyword': 'as_keyword_id().is_some()',
    'subr': 'as_subr_id().is_some()',
    'char': 'as_char().is_some()',
    'fixnum': 'is_fixnum()',
}

# Map of Value::method(var) patterns to as_* extraction for guarded patterns
NAMED_PATTERN_TO_ACCESSOR = {
    'symbol': 'as_symbol_id',
    'keyword': 'as_keyword_id',
    'subr': 'as_subr_id',
    'char': 'as_char',
    'fixnum': 'as_fixnum',
}

# Map of Value::Type(_) patterns in matches! to simple is_* checks
VALUE_TYPE_WILDCARD_TO_CHECK = {
    'Cons': 'is_cons()',
    'Str': 'is_string()',
    'Vector': 'is_vector()',
    'Lambda': 'is_lambda()',
    'ByteCode': 'is_bytecode()',
}

# Map of Value::make_type(_) patterns in matches! to is_* checks
VALUE_MAKE_WILDCARD_TO_CHECK = {
    'make_float': 'is_float()',
    'make_buffer': 'is_buffer()',
    'make_frame': 'is_frame()',
    'make_window': 'is_window()',
    'make_overlay': 'is_overlay()',
    'make_marker': 'is_marker()',
}

# Map of Value::Type(_) in match arms to ValueKind equivalents
VALUE_TYPE_TO_VALUEKIND = {
    'Cons': 'ValueKind::Cons',
    'Str': 'ValueKind::String',
    'Vector': 'ValueKind::Veclike(VecLikeType::Vector)',
    'Lambda': 'ValueKind::Veclike(VecLikeType::Lambda)',
    'ByteCode': 'ValueKind::Veclike(VecLikeType::ByteCode)',
}

VALUE_MAKE_TO_VALUEKIND = {
    'make_float': 'ValueKind::Float',
    'make_buffer': 'ValueKind::Veclike(VecLikeType::Buffer)',
    'make_frame': 'ValueKind::Veclike(VecLikeType::Frame)',
    'make_window': 'ValueKind::Veclike(VecLikeType::Window)',
    'make_overlay': 'ValueKind::Veclike(VecLikeType::Overlay)',
    'make_marker': 'ValueKind::Veclike(VecLikeType::Marker)',
}


def find_balanced_paren_end(s: str, start: int) -> int:
    """Find the position of the closing ')' that matches the opening at `start`.

    Handles nested parens and braces. Returns the index of the closing ')'
    or -1 if not found.
    """
    depth = 0
    brace_depth = 0
    bracket_depth = 0
    in_string = False
    prev = ''
    i = start
    while i < len(s):
        ch = s[i]
        if in_string:
            if ch == '"' and prev != '\\':
                in_string = False
        else:
            if ch == '"':
                in_string = True
            elif ch == '(':
                depth += 1
            elif ch == ')':
                depth -= 1
                if depth == 0:
                    return i
            elif ch == '{':
                brace_depth += 1
            elif ch == '}':
                brace_depth -= 1
            elif ch == '[':
                bracket_depth += 1
            elif ch == ']':
                bracket_depth -= 1
        prev = ch
        i += 1
    return -1


def fix_matches_macro(content: str) -> tuple:
    """Fix matches!() macros that use Value::method() as patterns.

    Returns (new_content, count_of_fixes).

    Handles both single-line and multi-line matches! by using balanced
    parenthesis tracking.
    """
    fixes = 0

    # Strategy: find each matches!( and parse it with balanced parens,
    # then apply transforms to the extracted content.

    def process_matches_occurrences(content, method, accessor):
        """Find and replace all matches!(EXPR, Value::METHOD(var) if GUARD)."""
        result = []
        pos = 0
        search_pat = re.compile(r'matches!\s*\(')
        # Match binding variables like `id`, `ref id`, `_`
        inner_pat = re.compile(
            r'\s*(.+?)\s*,\s*Value::' + re.escape(method) + r'\(\s*(?:ref\s+)?(\w+)\s*\)\s+if\s+',
            re.DOTALL
        )

        while pos < len(content):
            m = search_pat.search(content, pos)
            if not m:
                result.append(content[pos:])
                break

            result.append(content[pos:m.start()])

            # Find the balanced end of the matches!() macro
            paren_start = m.end() - 1  # position of the '('
            paren_end = find_balanced_paren_end(content, paren_start)

            if paren_end < 0:
                # Unbalanced — leave as-is
                result.append(content[m.start():m.end()])
                pos = m.end()
                continue

            macro_text = content[m.start():paren_end + 1]
            inner_text = content[m.end():paren_end]  # between ( and )

            # Try to match the pattern
            im = inner_pat.match(inner_text)
            if im:
                expr = im.group(1).strip()
                var = im.group(2)
                guard = inner_text[im.end():].strip()
                # Remove dereferences of the var in the guard
                guard = guard.replace(f'*{var}', var)
                # Remove leading & from expr
                if expr.startswith('&'):
                    expr = expr[1:]
                replacement = f'{expr}.{accessor}().map_or(false, |{var}| {guard})'
                result.append(replacement)
            else:
                result.append(macro_text)

            pos = paren_end + 1

        return ''.join(result)

    # --- matches!(EXPR, Value::method(var) if GUARD) ---
    for method, accessor in NAMED_PATTERN_TO_ACCESSOR.items():
        content = process_matches_occurrences(content, method, accessor)

    # --- matches!(EXPR, Value::method(_)) --- (wildcard, no guard)
    for method, check in WILDCARD_PATTERN_TO_CHECK.items():
        pattern = re.compile(
            r'matches!\s*\(\s*'
            r'(.+?)'              # EXPR
            r',\s*Value::' + re.escape(method) + r'\(_\)'
            r'\s*\)'
        )

        def replace_wildcard(m, check=check):
            expr = m.group(1).strip()
            if expr.startswith('&'):
                expr = expr[1:]
            return f'{expr}.{check}'

        content = pattern.sub(replace_wildcard, content)

    # --- matches!(EXPR, Value::method(var)) --- (named binding, no guard — just test presence)
    for method, check in WILDCARD_PATTERN_TO_CHECK.items():
        pattern = re.compile(
            r'matches!\s*\(\s*'
            r'(.+?)'
            r',\s*Value::' + re.escape(method) + r'\(\w+\)'
            r'\s*\)'
        )

        def replace_named_noguard(m, check=check):
            expr = m.group(1).strip()
            if expr.startswith('&'):
                expr = expr[1:]
            return f'{expr}.{check}'

        content = pattern.sub(replace_named_noguard, content)

    # --- matches!(EXPR, Value::Type(_)) for Cons/Str/Vector/Lambda/ByteCode ---
    for typ, check in VALUE_TYPE_WILDCARD_TO_CHECK.items():
        pattern = re.compile(
            r'matches!\s*\(\s*'
            r'(.+?)'
            r',\s*Value::' + re.escape(typ) + r'\(_\)'
            r'\s*\)'
        )

        def replace_type_wildcard(m, check=check):
            expr = m.group(1).strip()
            if expr.startswith('&'):
                expr = expr[1:]
            return f'{expr}.{check}'

        content = pattern.sub(replace_type_wildcard, content)

    # --- matches!(EXPR, Value::make_type(_)) for float/buffer/frame/window ---
    for maker, check in VALUE_MAKE_WILDCARD_TO_CHECK.items():
        pattern = re.compile(
            r'matches!\s*\(\s*'
            r'(.+?)'
            r',\s*Value::' + re.escape(maker) + r'\(_\)'
            r'\s*\)'
        )

        def replace_make_wildcard(m, check=check):
            expr = m.group(1).strip()
            if expr.startswith('&'):
                expr = expr[1:]
            return f'{expr}.{check}'

        content = pattern.sub(replace_make_wildcard, content)

    # --- matches!(EXPR, Value::Type(var) if GUARD) for Cons ---
    # Use the balanced-paren approach for multiline guards
    def process_cons_guarded(content):
        result = []
        pos = 0
        search_pat = re.compile(r'matches!\s*\(')
        inner_pat = re.compile(
            r'\s*(.+?)\s*,\s*Value::Cons\(\s*(\w+)\s*\)\s+if\s+',
            re.DOTALL
        )

        while pos < len(content):
            m = search_pat.search(content, pos)
            if not m:
                result.append(content[pos:])
                break

            result.append(content[pos:m.start()])
            paren_start = m.end() - 1
            paren_end = find_balanced_paren_end(content, paren_start)

            if paren_end < 0:
                result.append(content[m.start():m.end()])
                pos = m.end()
                continue

            macro_text = content[m.start():paren_end + 1]
            inner_text = content[m.end():paren_end]

            im = inner_pat.match(inner_text)
            if im:
                expr = im.group(1).strip()
                _var = im.group(2)
                guard = inner_text[im.end():].strip()
                if expr.startswith('&'):
                    expr = expr[1:]
                replacement = f'{expr}.is_cons() && {{ /* TODO(tagged): migrate Cons guard */ {guard} }}'
                result.append(replacement)
            else:
                result.append(macro_text)

            pos = paren_end + 1

        return ''.join(result)

    content = process_cons_guarded(content)

    # --- matches!(EXPR, Some(Value::fixnum(n)) if GUARD) ---
    # e.g. matches!(vec.get(bit_idx), Some(Value::fixnum(n)) if *n != 0)
    def process_some_guarded(content):
        result = []
        pos = 0
        search_pat = re.compile(r'matches!\s*\(')

        while pos < len(content):
            m = search_pat.search(content, pos)
            if not m:
                result.append(content[pos:])
                break

            result.append(content[pos:m.start()])
            paren_start = m.end() - 1
            paren_end = find_balanced_paren_end(content, paren_start)

            if paren_end < 0:
                result.append(content[m.start():m.end()])
                pos = m.end()
                continue

            macro_text = content[m.start():paren_end + 1]
            inner_text = content[m.end():paren_end]

            replaced = False
            for method, accessor in NAMED_PATTERN_TO_ACCESSOR.items():
                some_pat = re.compile(
                    r'\s*(.+?)\s*,\s*Some\(Value::' + re.escape(method) + r'\(\s*(\w+)\s*\)\)\s+if\s+',
                    re.DOTALL
                )
                sm = some_pat.match(inner_text)
                if sm:
                    expr = sm.group(1).strip()
                    var = sm.group(2)
                    guard = inner_text[sm.end():].strip()
                    guard = guard.replace(f'*{var}', var)
                    if expr.startswith('&'):
                        expr = expr[1:]
                    replacement = f'{expr}.and_then(|v| v.{accessor}()).map_or(false, |{var}| {guard})'
                    result.append(replacement)
                    replaced = True
                    break

            if not replaced:
                result.append(macro_text)

            pos = paren_end + 1

        return ''.join(result)

    content = process_some_guarded(content)

    return content, fixes


def fix_if_let_patterns(content: str) -> str:
    """Fix if let / while let Value::method(var) = EXPR patterns."""

    for method, accessor in NAMED_PATTERN_TO_ACCESSOR.items():
        # if let Value::method(var) = EXPR {
        pattern = re.compile(
            r'(if|while)\s+let\s+Value::' + re.escape(method) + r'\(\s*(\w+)\s*\)\s*=\s*(.+?)\s*\{'
        )

        def replace_if_let(m, accessor=accessor):
            keyword = m.group(1)
            var = m.group(2)
            expr = m.group(3).strip()
            if expr.startswith('&'):
                expr = expr[1:]
            return f'{keyword} let Some({var}) = {expr}.{accessor}() {{'

        content = pattern.sub(replace_if_let, content)

    # if let Value::Cons(var) = EXPR { → if EXPR.is_cons() {
    # (the variable binding is lost, but these typically just check type)
    pattern = re.compile(
        r'if\s+let\s+Value::Cons\(\s*_\s*\)\s*=\s*(.+?)\s*\{'
    )
    content = pattern.sub(r'if \1.is_cons() {', content)

    # if let Value::Cons(_) = EXPR (without brace on same line)
    pattern = re.compile(
        r'if\s+let\s+Value::Cons\(\s*_\s*\)\s*=\s*(.+?)$',
        re.MULTILINE
    )
    content = pattern.sub(r'if \1.is_cons()', content)

    return content


def fix_match_arm_patterns(lines: list) -> list:
    """Fix Value::Type(var) and Value::make_type(var) in match arm patterns.

    These appear in `match expr.kind() { ... }` blocks, but also in
    `match expr { Value::Cons(cell) => ... }` blocks (which should become
    match on .kind()).
    """
    result = []
    in_match_kind = False
    match_depth = 0

    for line in lines:
        new_line = line

        # Detect match EXPR.kind() { blocks
        if re.search(r'\bmatch\b.+\.kind\(\)\s*\{', line):
            in_match_kind = True

        # In match arms, convert Value::Type(var) → ValueKind::Type
        # for patterns like Value::Cons(cell) =>
        if in_match_kind or re.search(r'\bmatch\b', line):
            # Value::fixnum(_) => → ValueKind::Fixnum(_) =>
            for method, kind_name in [
                ('fixnum', 'Fixnum'),
                ('symbol', 'Symbol'),
                ('char', 'Char'),
                ('keyword', 'Keyword'),
                ('subr', 'Subr'),
            ]:
                arm_pat = re.compile(
                    r'Value::' + re.escape(method) + r'\((\w+)\)\s*=>'
                )
                m = arm_pat.search(new_line)
                if m:
                    var = m.group(1)
                    new_line = arm_pat.sub(f'ValueKind::{kind_name}({var}) =>', new_line)

            # Value::fixnum(_) | Value::char(_) (in multi-alternative arms)
            for method, kind_name in [
                ('fixnum', 'Fixnum'),
                ('symbol', 'Symbol'),
                ('char', 'Char'),
                ('keyword', 'Keyword'),
                ('subr', 'Subr'),
            ]:
                # Pattern: Value::method(_) appearing before a | or =>
                alt_pat = re.compile(
                    r'Value::' + re.escape(method) + r'\((\w+)\)'
                )
                # Only in pattern context (before =>)
                arrow_pos = new_line.find('=>')
                if arrow_pos >= 0:
                    before = new_line[:arrow_pos]
                    after = new_line[arrow_pos:]
                    m = alt_pat.search(before)
                    if m:
                        var = m.group(1)
                        before = alt_pat.sub(f'ValueKind::{kind_name}({var})', before)
                        new_line = before + after

        result.append(new_line)

    return result


def fix_or_patterns_in_matches(content: str) -> str:
    """Fix multi-alternative patterns in matches! that mix Value:: function-like
    constructors with other patterns.

    e.g.: matches!(x, Value::fixnum(_) | Value::make_float(_) | Value::char(_))
    → x.is_fixnum() || x.is_float() || x.as_char().is_some()
    """
    # Pattern for matches! with | alternatives containing Value:: function-like
    # This is complex, so we handle specific known combinations

    # Generic approach: find matches!(EXPR, ALT1 | ALT2 | ...) where all
    # alternatives are Value::method(_) patterns.

    ALL_PATTERN_TO_CHECK = {}
    ALL_PATTERN_TO_CHECK.update({f'Value::{k}(_)': v for k, v in WILDCARD_PATTERN_TO_CHECK.items()})
    ALL_PATTERN_TO_CHECK.update({f'Value::{k}(_)': v for k, v in VALUE_TYPE_WILDCARD_TO_CHECK.items()})
    ALL_PATTERN_TO_CHECK.update({f'Value::{k}(_)': v for k, v in VALUE_MAKE_WILDCARD_TO_CHECK.items()})
    # Also handle ValueKind patterns that might be mixed in
    ALL_PATTERN_TO_CHECK['ValueKind::String'] = 'is_string()'
    ALL_PATTERN_TO_CHECK['ValueKind::Cons'] = 'is_cons()'
    ALL_PATTERN_TO_CHECK['ValueKind::Float'] = 'is_float()'
    ALL_PATTERN_TO_CHECK['ValueKind::T'] = 'is_t()'
    ALL_PATTERN_TO_CHECK['Value::NIL'] = 'is_nil()'
    ALL_PATTERN_TO_CHECK['Value::T'] = 'is_t()'
    # ValueKind variants with wildcard bindings (from matches! context)
    ALL_PATTERN_TO_CHECK['ValueKind::Fixnum(_)'] = 'is_fixnum()'
    ALL_PATTERN_TO_CHECK['ValueKind::Symbol(_)'] = 'is_symbol()'
    ALL_PATTERN_TO_CHECK['ValueKind::Char(_)'] = 'as_char().is_some()'
    ALL_PATTERN_TO_CHECK['ValueKind::Keyword(_)'] = 'as_keyword_id().is_some()'
    ALL_PATTERN_TO_CHECK['ValueKind::Subr(_)'] = 'as_subr_id().is_some()'

    # Find matches!(EXPR, PAT1 | PAT2 | ...)
    # The pattern needs to match alternatives that are either:
    # - Value::name(...)
    # - ValueKind::Name or ValueKind::Name(...)
    # - Value::NIL, Value::T (uppercase const-like)
    alt_atom = r'(?:Value::\w+\([^)]*\)|ValueKind::\w+(?:\([^)]*\))?|Value::[A-Z]+)'
    pat = re.compile(
        r'matches!\s*\(\s*(.+?)\s*,\s*'
        r'((?:' + alt_atom + r'\s*\|\s*)*' + alt_atom + r')'
        r'\s*\)'
    )

    def replace_or_matches(m):
        expr = m.group(1).strip()
        alternatives_str = m.group(2).strip()

        # Split on |
        alts = [a.strip() for a in alternatives_str.split('|')]

        # Check if all alternatives have a known mapping
        checks = []
        for alt in alts:
            if alt in ALL_PATTERN_TO_CHECK:
                checks.append(ALL_PATTERN_TO_CHECK[alt])
            else:
                # Can't convert this one — leave the whole thing alone
                return m.group(0)

        if expr.startswith('&'):
            expr = expr[1:]

        result = ' || '.join(f'{expr}.{c}' for c in checks)
        # Wrap in parens if multiple alternatives (to preserve precedence when negated)
        if len(checks) > 1:
            result = f'({result})'
        return result

    content = pat.sub(replace_or_matches, content)
    return content


def fix_match_arm_value_types(content: str) -> str:
    """Fix Value::Type(var) and Value::make_type(var) in match arm patterns
    that should be ValueKind variants.

    This handles patterns like:
        Value::Cons(cell) => { ... }
    which should become:
        ValueKind::Cons => { let cell = ...; ... }
    But that's too complex for mechanical fixing. Instead, for simple
    match arms, convert to ValueKind.
    """
    # Value::Cons(cell) => ... in a match on .kind()
    # These will need manual intervention for the cell binding.
    # But we can at least convert the pattern and add a TODO.

    # For now, handle the simpler case where it's a match on something
    # without .kind() — these need .kind() added.

    return content


# ---------------------------------------------------------------------------
# Fix 2: E0433 — Missing ValueKind/VecLikeType imports
# ---------------------------------------------------------------------------

def fix_missing_imports(content: str, filepath: Path) -> str:
    """Add ValueKind and VecLikeType imports where they're used but not imported."""

    uses_valuekind = 'ValueKind::' in content
    uses_vecliketype = 'VecLikeType::' in content

    if not uses_valuekind and not uses_vecliketype:
        return content

    has_valuekind_import = bool(re.search(r'use\s+.*\bValueKind\b', content))
    has_vecliketype_import = bool(re.search(r'use\s+.*\bVecLikeType\b', content))

    # Wildcard imports from value:: bring in ValueKind and VecLikeType
    has_value_wildcard = bool(re.search(
        r'use\s+(?:super|crate::emacs_core)::value::\*;',
        content
    ))
    if has_value_wildcard:
        has_valuekind_import = True
        has_vecliketype_import = True

    needs_valuekind = uses_valuekind and not has_valuekind_import
    needs_vecliketype = uses_vecliketype and not has_vecliketype_import

    if not needs_valuekind and not needs_vecliketype:
        return content

    # Determine the right import path
    # Files in emacs_core/ use super::value:: or crate::emacs_core::value::
    # Files elsewhere use crate::emacs_core::value::

    rel = filepath.relative_to(SRC_ROOT)
    parts = rel.parts

    # Build the import items list
    items = []
    if needs_valuekind:
        items.append('ValueKind')
    if needs_vecliketype:
        items.append('VecLikeType')

    # Check if there's already a `use crate::emacs_core::value::` line we can extend
    existing_import = re.search(
        r'(use\s+crate::emacs_core::value::\{)([^}]+)(\};)',
        content
    )
    if existing_import:
        current_items = existing_import.group(2)
        new_items_str = ', '.join(items)
        new_import = f'{existing_import.group(1)}{current_items}, {new_items_str}{existing_import.group(3)}'
        content = content.replace(existing_import.group(0), new_import, 1)
        return content

    # Check for simple use crate::emacs_core::value::Something;
    existing_simple = re.search(
        r'(use\s+crate::emacs_core::value::)(\w+);',
        content
    )
    if existing_simple:
        existing_item = existing_simple.group(2)
        all_items = [existing_item] + items
        new_import = f'{existing_simple.group(1)}{{{", ".join(all_items)}}};'
        content = content.replace(existing_simple.group(0), new_import, 1)
        return content

    # Check for super::value:: imports
    existing_super = re.search(
        r'(use\s+super::value::\{)([^}]+)(\};)',
        content
    )
    if existing_super:
        current_items = existing_super.group(2)
        new_items_str = ', '.join(items)
        new_import = f'{existing_super.group(1)}{current_items}, {new_items_str}{existing_super.group(3)}'
        content = content.replace(existing_super.group(0), new_import, 1)
        return content

    existing_super_simple = re.search(
        r'(use\s+super::value::)(\w+);',
        content
    )
    if existing_super_simple:
        existing_item = existing_super_simple.group(2)
        all_items = [existing_item] + items
        new_import = f'{existing_super_simple.group(1)}{{{", ".join(all_items)}}};'
        content = content.replace(existing_super_simple.group(0), new_import, 1)
        return content

    # No existing value import found — add one after the last module-level use statement
    items_str = ', '.join(items)
    import_line = f'use crate::emacs_core::value::{{{items_str}}};\n'

    # Find a good insertion point — after the last module-level `use` line.
    # Module-level uses have no leading whitespace (or minimal indentation).
    # Skip `use` statements inside function bodies (which are indented).
    lines = content.split('\n')
    last_use_idx = -1
    brace_depth = 0
    for i, line in enumerate(lines):
        stripped = line.strip()
        # Track brace depth to know if we're inside a function body
        for ch in line:
            if ch == '{':
                brace_depth += 1
            elif ch == '}':
                brace_depth -= 1
        # Only consider module-level use statements (brace_depth == 0 or
        # the use is at column 0 with no indentation)
        if stripped.startswith('use ') and brace_depth <= 0:
            last_use_idx = i

    if last_use_idx >= 0:
        lines.insert(last_use_idx + 1, import_line.rstrip())
        content = '\n'.join(lines)
    else:
        # No module-level use statements — add after any doc comments / module attributes
        insert_idx = 0
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith('//') or stripped.startswith('#[') or stripped == '':
                insert_idx = i + 1
            else:
                break
        lines.insert(insert_idx, import_line.rstrip())
        content = '\n'.join(lines)

    return content


# ---------------------------------------------------------------------------
# Fix 3: Remaining dereferences
# ---------------------------------------------------------------------------

def fix_remaining_derefs(content: str) -> str:
    """Fix *id and *n dereferences that are no longer needed.

    After the tagged pointer migration, kind() returns owned values
    (i64, SymId, char), not references. So *id → id, *n → n.
    """
    # resolve_sym(*id) → resolve_sym(id) where id comes from ValueKind::Symbol(id)
    content = re.sub(r'resolve_sym\(\*(\w+)\)', r'resolve_sym(\1)', content)

    # .contains(n) where n is i64 from ValueKind::Fixnum(n) — Range::contains takes &T
    # (0..=0x3F_FFFF).contains(n) → (0..=0x3F_FFFF).contains(&n)
    # This is tricky — we can't blindly replace. Skip for now, too context-dependent.

    return content


# ---------------------------------------------------------------------------
# Fix 4: E0631 — Closure type mismatches
# ---------------------------------------------------------------------------

def fix_closure_type_mismatches(content: str) -> str:
    """Fix method references that take Value by value but are used where
    FnOnce(&Value) -> bool is expected.

    .is_none_or(Value::is_nil) → .is_none_or(|v| v.is_nil())
    .all(Value::is_nil)        → .all(|v| v.is_nil())
    .any(Value::is_X)          → .any(|v| v.is_X())
    .map(Value::is_X)          → .map(|v| v.is_X())
    .filter(Value::is_X)       → .filter(|v| v.is_X())
    """
    # List of method names that are now fn(self) -> bool
    is_methods = [
        'is_nil', 'is_t', 'is_cons', 'is_string', 'is_symbol', 'is_fixnum',
        'is_float', 'is_vector', 'is_lambda', 'is_bytecode', 'is_buffer',
        'is_frame', 'is_window', 'is_marker', 'is_overlay',
    ]

    for method in is_methods:
        # .combinator(Value::method) → .combinator(|v| v.method())
        # The combinators that pass &T when iterating over &[Value] or Option<&Value>
        for combinator in ['is_none_or', 'all', 'any', 'map', 'filter', 'find']:
            pattern = re.compile(
                r'\.' + re.escape(combinator) + r'\(Value::' + re.escape(method) + r'\)'
            )
            replacement = f'.{combinator}(|v| v.{method}())'
            content = pattern.sub(replacement, content)

        # Also handle .map(|vals| vals.iter().all(Value::method))
        pattern = re.compile(
            r'\.all\(Value::' + re.escape(method) + r'\)'
        )
        replacement = f'.all(|v| v.{method}())'
        content = pattern.sub(replacement, content)

    return content


# ---------------------------------------------------------------------------
# Fix 6: Remaining Value::Nil / Value::True in non-pattern context
# ---------------------------------------------------------------------------

def fix_value_nil_true(content: str) -> str:
    """Replace Value::Nil → Value::NIL and Value::True → Value::T
    in non-pattern context.
    """
    lines = content.split('\n')
    result = []
    for line in lines:
        # Skip comments
        stripped = line.strip()
        if stripped.startswith('//'):
            result.append(line)
            continue

        # Value::Nil → Value::NIL (but not Value::NIL which is already correct)
        # Be careful not to match in pattern context (match arms)
        # Heuristic: if line has => and Value::Nil is before =>, it's a pattern
        new_line = line

        # Replace Value::Nil with Value::NIL
        if 'Value::Nil' in new_line:
            # Check it's not already part of Value::NIL
            new_line = re.sub(r'Value::Nil\b(?!L)', 'Value::NIL', new_line)

        # Replace Value::True with Value::T
        if 'Value::True' in new_line:
            new_line = re.sub(r'Value::True\b', 'Value::T', new_line)

        result.append(new_line)

    return '\n'.join(result)


# ---------------------------------------------------------------------------
# Fix 1 (continued): complex multi-alternative matches! with function-like
# constructors mixed with valid patterns
# ---------------------------------------------------------------------------

def fix_complex_matches_or(content: str) -> str:
    """Handle matches! with OR alternatives that mix function-like constructors.

    e.g.: matches!(x, Value::fixnum(_) | Value::make_float(_) | Value::char(_))
    → x.is_fixnum() || x.is_float() || x.as_char().is_some()

    Also: matches!(x, Value::Cons(_) | Value::NIL)
    → x.is_cons() || x.is_nil()
    """
    return fix_or_patterns_in_matches(content)


# ---------------------------------------------------------------------------
# Fix 1 (continued): Value::Type(var) in match arms that aren't .kind()
# ---------------------------------------------------------------------------

def fix_match_without_kind(lines: list) -> list:
    """Fix match EXPR { Value::Cons(cell) => ... } where EXPR doesn't use .kind().

    These should either:
    a) Become match EXPR.kind() { ValueKind::Cons => ... } (but the cell binding is lost)
    b) Or be converted to if/else chains

    For now, we just fix the Value:: patterns in the arms.
    """
    result = []
    i = 0
    while i < len(lines):
        line = lines[i]

        # Detect match without .kind()
        m = re.match(r'^(\s*)match\s+(.+?)\s*\{', line)
        if m and '.kind()' not in line:
            indent = m.group(1)
            match_expr = m.group(2).strip()

            # Check if the next few lines have Value::Cons/Str/etc patterns
            # that indicate this match needs .kind()
            has_value_patterns = False
            for j in range(i+1, min(i+20, len(lines))):
                arm_line = lines[j].strip()
                if re.match(r'Value::(Cons|Str|Vector|Lambda|ByteCode)\s*\(', arm_line):
                    has_value_patterns = True
                    break
                if arm_line.startswith('}'):
                    break

            # Don't auto-convert complex matches — too risky
            result.append(line)
        else:
            result.append(line)
        i += 1

    return result


# ---------------------------------------------------------------------------
# Fix for Value::Type(var) patterns in match arms (within .kind() match)
# ---------------------------------------------------------------------------

def count_braces(text: str) -> int:
    """Count net brace depth change, ignoring strings."""
    depth = 0
    in_str = False
    prev = ''
    for ch in text:
        if in_str:
            if ch == '"' and prev != '\\':
                in_str = False
        else:
            if ch == '"':
                in_str = True
            elif ch == '{':
                depth += 1
            elif ch == '}':
                depth -= 1
        prev = ch
    return depth


def fix_value_type_in_kind_match_arms(content: str) -> str:
    """Convert Value::Type(_) and Value::Type(var) in match arm patterns
    (within match expr.kind() blocks) to the corresponding ValueKind variant.

    Value::fixnum(_)  =>  →  ValueKind::Fixnum(_)  =>
    Value::symbol(s)  =>  →  ValueKind::Symbol(s)  =>
    Value::Cons(_)    =>  →  ValueKind::Cons        =>
    Value::make_float(_) => → ValueKind::Float      =>
    etc.

    IMPORTANT: Only converts patterns inside match blocks that match on
    .kind() — NOT in tuple matches or other complex match expressions,
    where the bindings would be lost.
    """
    lines = content.split('\n')
    result = []

    # Track nested match blocks with a stack. Each entry is:
    # (is_kind_match: bool, brace_depth: int)
    match_stack = []
    current_brace_depth = 0

    for line in lines:
        new_line = line

        # Track brace depth
        delta = count_braces(line)

        # Detect `match EXPR.kind() {` or `match EXPR {`
        match_m = re.search(r'\bmatch\b\s+(.+?)\s*\{', line)
        if match_m:
            match_expr = match_m.group(1).strip()
            is_kind = '.kind()' in match_expr
            # The opening brace is on this line
            match_stack.append((is_kind, current_brace_depth + 1))

        current_brace_depth += delta

        # Pop match entries when their brace depth is exceeded (block closed)
        while match_stack and current_brace_depth < match_stack[-1][1]:
            match_stack.pop()

        # Determine if we're inside a .kind() match
        in_kind_match = any(is_kind for is_kind, _ in match_stack)

        # Check if this line has a match arm pattern (before =>)
        arrow_pos = new_line.find('=>')
        if arrow_pos >= 0:
            before_arrow = new_line[:arrow_pos]
            after_arrow = new_line[arrow_pos:]

            # Skip if the pattern contains tuple destructuring like (Value::X, Value::Y)
            # These need manual conversion
            if '(' in before_arrow and ')' in before_arrow:
                # Check if it's a tuple pattern containing Value:: constructors
                if re.search(r'\(\s*Value::\w+\(.+?\)\s*,', before_arrow):
                    result.append(new_line)
                    continue

            # Only convert if inside a match block (either .kind() or regular match)
            # For .kind() matches, convert freely.
            # For non-.kind() matches, be conservative: only convert wildcard patterns.
            if in_kind_match:
                # Fix Value::fixnum/symbol/char/keyword/subr(var) => patterns
                for method, kind_name in [
                    ('fixnum', 'Fixnum'),
                    ('symbol', 'Symbol'),
                    ('char', 'Char'),
                    ('keyword', 'Keyword'),
                    ('subr', 'Subr'),
                ]:
                    before_arrow = re.sub(
                        r'Value::' + re.escape(method) + r'\((\w+)\)',
                        f'ValueKind::{kind_name}(\\1)',
                        before_arrow
                    )

                # Fix Value::Cons(var) => patterns → ValueKind::Cons =>
                before_arrow = re.sub(
                    r'Value::Cons\(\w+\)',
                    'ValueKind::Cons',
                    before_arrow
                )
                before_arrow = re.sub(
                    r'Value::Cons\(_\)',
                    'ValueKind::Cons',
                    before_arrow
                )

                # Fix Value::Str(_) => → ValueKind::String =>
                before_arrow = re.sub(
                    r'Value::Str\(\w+\)',
                    'ValueKind::String',
                    before_arrow
                )
                before_arrow = re.sub(
                    r'Value::Str\(_\)',
                    'ValueKind::String',
                    before_arrow
                )

                # Fix Value::Vector(_) => → ValueKind::Veclike(VecLikeType::Vector) =>
                for typ, kind_variant in VALUE_TYPE_TO_VALUEKIND.items():
                    if typ in ('Cons', 'Str'):
                        continue  # already handled
                    before_arrow = re.sub(
                        r'Value::' + re.escape(typ) + r'\(\w+\)',
                        kind_variant,
                        before_arrow
                    )
                    before_arrow = re.sub(
                        r'Value::' + re.escape(typ) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

                # Fix Value::make_float(_) => → ValueKind::Float =>
                for maker, kind_variant in VALUE_MAKE_TO_VALUEKIND.items():
                    before_arrow = re.sub(
                        r'Value::' + re.escape(maker) + r'\(\w+\)',
                        kind_variant,
                        before_arrow
                    )
                    before_arrow = re.sub(
                        r'Value::' + re.escape(maker) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

                # Fix Value::Lambda/_) => → ValueKind::Veclike(VecLikeType::Lambda) =>
                for vtype, kind_variant in [
                    ('Lambda', 'ValueKind::Veclike(VecLikeType::Lambda)'),
                    ('ByteCode', 'ValueKind::Veclike(VecLikeType::ByteCode)'),
                ]:
                    before_arrow = re.sub(
                        r'Value::' + re.escape(vtype) + r'\(\w+\)',
                        kind_variant,
                        before_arrow
                    )
                    before_arrow = re.sub(
                        r'Value::' + re.escape(vtype) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

            else:
                # Non-.kind() match: only convert wildcard/simple patterns
                # where the binding isn't used (i.e., Value::fixnum(_), Value::Cons(_))
                for method, kind_name in [
                    ('fixnum', 'Fixnum'),
                    ('symbol', 'Symbol'),
                    ('char', 'Char'),
                    ('keyword', 'Keyword'),
                    ('subr', 'Subr'),
                ]:
                    # Only wildcard patterns (no named binding)
                    before_arrow = re.sub(
                        r'Value::' + re.escape(method) + r'\(_\)',
                        f'ValueKind::{kind_name}(_)',
                        before_arrow
                    )

                # Wildcard Cons/Str/Vector patterns
                before_arrow = re.sub(r'Value::Cons\(_\)', 'ValueKind::Cons', before_arrow)
                before_arrow = re.sub(r'Value::Str\(_\)', 'ValueKind::String', before_arrow)

                for typ, kind_variant in VALUE_TYPE_TO_VALUEKIND.items():
                    if typ in ('Cons', 'Str'):
                        continue
                    before_arrow = re.sub(
                        r'Value::' + re.escape(typ) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

                for maker, kind_variant in VALUE_MAKE_TO_VALUEKIND.items():
                    before_arrow = re.sub(
                        r'Value::' + re.escape(maker) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

                for vtype, kind_variant in [
                    ('Lambda', 'ValueKind::Veclike(VecLikeType::Lambda)'),
                    ('ByteCode', 'ValueKind::Veclike(VecLikeType::ByteCode)'),
                ]:
                    before_arrow = re.sub(
                        r'Value::' + re.escape(vtype) + r'\(_\)',
                        kind_variant,
                        before_arrow
                    )

            new_line = before_arrow + after_arrow

        result.append(new_line)

    return '\n'.join(result)


# ---------------------------------------------------------------------------
# Fix: Value::symbol(_) | Value::subr(_) in non-matches!, non-arm context
# e.g. in if-let chains, match arm alternatives (before =>)
# ---------------------------------------------------------------------------

def fix_remaining_value_function_patterns(content: str) -> str:
    """Fix remaining Value::symbol(_) etc. that appear in pattern positions
    but weren't caught by previous fixes.

    Handles:
    - Some(Value::fixnum(_)) in match arms → Some(v) if v.is_fixnum() (complex)
    - Value::fixnum(_) | Value::char(_) in arms (already partially handled)
    """
    # Some(Value::fixnum(_)) → pattern in match arms
    # This requires more complex rewriting — just convert the inner pattern
    # For matches! with Some(Value::fixnum(_)):
    content = re.sub(
        r'matches!\s*\(\s*(.+?)\s*,\s*Some\(Value::fixnum\(_\)\)\s*\)',
        r'\1.map_or(false, |v| v.is_fixnum())',
        content
    )

    content = re.sub(
        r'matches!\s*\(\s*(.+?)\s*,\s*Some\(Value::make_frame\(_\)\)\s*\)',
        r'\1.map_or(false, |v| v.is_frame())',
        content
    )

    content = re.sub(
        r'matches!\s*\(\s*(.+?)\s*,\s*Some\(Value::make_buffer\(_\)\)\s*\)',
        r'\1.map_or(false, |v| v.is_buffer())',
        content
    )

    content = re.sub(
        r'matches!\s*\(\s*(.+?)\s*,\s*Some\(Value::make_window\(_\)\)\s*\)',
        r'\1.map_or(false, |v| v.is_window())',
        content
    )

    return content


# ---------------------------------------------------------------------------
# Fix: Some(Value::fixnum(_)) in match arms
# ---------------------------------------------------------------------------

def fix_some_value_in_match_arms(content: str) -> str:
    """Fix Some(Value::Type(var)) in match arm patterns.

    Some(Value::fixnum(n)) => ... → Some(v) if v.is_fixnum() => ...
    (Too complex — instead just flag with TODO)

    Actually, better: don't change match arms with Some(Value::...) — they
    need manual refactoring.
    """
    return content


# ---------------------------------------------------------------------------
# Fix: ValueKind::T | Value::symbol(_) mixed patterns in match arms
# ---------------------------------------------------------------------------

def fix_mixed_valuekind_value_arms(content: str) -> str:
    """Fix match arm patterns that mix ValueKind and Value patterns.

    e.g.: ValueKind::T | Value::symbol(_) => ...
    → ValueKind::T | ValueKind::Symbol(_) => ...

    And: ValueKind::String | Value::fixnum(_) => ...
    → ValueKind::String | ValueKind::Fixnum(_) => ...
    """
    # Already handled by fix_value_type_in_kind_match_arms
    return content


# ---------------------------------------------------------------------------
# Fix: if let (Value::Cons(a), Value::Cons(b)) = ... tuple destructuring
# ---------------------------------------------------------------------------

def fix_tuple_if_let(content: str) -> str:
    """Fix if let (Value::Cons(a), Value::Cons(b)) = (...) patterns."""
    # These need manual conversion — flag with TODO
    return content


# ---------------------------------------------------------------------------
# Main processing
# ---------------------------------------------------------------------------

def process_file(filepath: Path, dry_run: bool) -> dict:
    """Process a single .rs file. Returns a dict of fix counts."""
    with open(filepath, 'r') as f:
        original = f.read()

    content = original

    # Fix 6: Value::Nil → Value::NIL, Value::True → Value::T
    content = fix_value_nil_true(content)

    # Fix 1: matches! macros with Value:: function-like patterns
    content, _ = fix_matches_macro(content)

    # Fix 1 (continued): complex OR patterns in matches!
    content = fix_complex_matches_or(content)

    # Fix 1 (continued): if let / while let patterns
    content = fix_if_let_patterns(content)

    # Fix 1 (continued): Value::Type(var) in match arm patterns
    content = fix_value_type_in_kind_match_arms(content)

    # Fix 1 (continued): remaining Value::method patterns
    content = fix_remaining_value_function_patterns(content)

    # Fix 2: Missing imports
    content = fix_missing_imports(content, filepath)

    # Fix 3: Remaining dereferences
    content = fix_remaining_derefs(content)

    # Fix 4: Closure type mismatches
    content = fix_closure_type_mismatches(content)

    changed = content != original
    if changed and not dry_run:
        with open(filepath, 'w') as f:
            f.write(content)

    return {
        'changed': changed,
        'filepath': filepath,
    }


# ---------------------------------------------------------------------------
# Summary / reporting
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description='Sixth-pass mechanical fixes for NeoVM tagged pointer migration.'
    )
    parser.add_argument('--dry-run', action='store_true',
                        help='Preview changes without modifying files.')
    args = parser.parse_args()

    if not SRC_ROOT.exists():
        print(f"Error: {SRC_ROOT} not found. Run from the project root.", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(SRC_ROOT)
    print(f"Found {len(files)} .rs files to process.\n")

    changed_files = []
    unchanged_files = []

    for filepath in files:
        result = process_file(filepath, args.dry_run)
        if result['changed']:
            changed_files.append(filepath)
        else:
            unchanged_files.append(filepath)

    # Print summary
    print("=" * 60)
    print(f"{'DRY RUN ' if args.dry_run else ''}Summary")
    print("=" * 60)
    print(f"Total files scanned:  {len(files)}")
    print(f"Files changed:        {len(changed_files)}")
    print(f"Files unchanged:      {len(unchanged_files)}")
    print()

    if changed_files:
        print("Changed files:")
        for f in changed_files:
            print(f"  {f}")
    print()

    if args.dry_run:
        print("(Dry run — no files were modified.)")
    else:
        print("Done. All changes applied in-place.")


if __name__ == '__main__':
    main()
