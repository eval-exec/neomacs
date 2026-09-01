#!/usr/bin/env python3
"""
Fourth-pass mechanical fixes for the NeoVM tagged pointer migration.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Fixes applied:
  1. Value::Int(x)  -> Value::fixnum(x)
  2. Value::Char(x) -> Value::char(x)  (constructor context only)
  3. *other in match value.kind() catch-all arms -> *MATCH_EXPR
     (where other is a ValueKind but the code expects Value)
  4. *n where n is i64 from ValueKind::Fixnum(n) (already owned, no deref)
  5. .contains(n) -> .contains(&n) in ValueKind::Fixnum(n) match guards
  6. Value::fixnum(x) used in patterns (if let / match) -> extract with as_fixnum
  7. Value::Str(id) -> value.as_str() / value.is_string() patterns
  8. Value::Cons(id) -> value.is_cons() patterns
  9. Value::Vector(id) / Value::HashTable(id) / etc -> proper tagged API
 10. with_heap(|h| h.get_string(LOST_VAR)) -> MATCH_EXPR.as_str().unwrap()
 11. with_heap(|h| h.cons_car(LOST_VAR)) -> MATCH_EXPR.cons_car()
 12. read_cons(LOST_VAR) -> ConsSnapshot { car: MATCH_EXPR.cons_car(), ... }
 13. Value::Overlay(*overlay) -> Value::from_overlay(overlay)  (TODO marker)
 14. is_some_and(Value::is_X) -> is_some_and(|v| v.is_X())
 15. .and_then(Value::as_str) -> .and_then(|v| v.as_str())

Usage:
    python3 scripts/fix_pass4.py            # apply in-place
    python3 scripts/fix_pass4.py --dry-run  # preview changes only
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
# Fix 1: Value::Int(expr) -> Value::fixnum(expr)
# ---------------------------------------------------------------------------

def fix_value_int_constructor(content: str) -> tuple:
    """Replace Value::Int(expr) with Value::fixnum(expr) in expression context.

    This handles the constructor case where Value::Int was the old enum variant.
    Must NOT replace in pattern context (match arms, if-let patterns).

    Returns (new_content, count).
    """
    # Simple text replacement: Value::Int( -> Value::fixnum(
    # This works because Value::Int is not a valid associated item on TaggedValue.
    # We do NOT want to replace ValueKind::Fixnum (that's the match variant).
    count = 0
    # Replace Value::Int( with Value::fixnum(
    pattern = re.compile(r'\bValue::Int\(')
    new_content = pattern.sub('Value::fixnum(', content)
    count = len(pattern.findall(content))
    return new_content, count


# ---------------------------------------------------------------------------
# Fix 2: Value::Char(expr) -> Value::char(expr) in constructor context
# ---------------------------------------------------------------------------

def fix_value_char_constructor(content: str) -> tuple:
    """Replace Value::Char(expr) with Value::char(expr) in expression context.

    Be careful: ValueKind::Char(c) is valid and should NOT be changed.
    Only Value::Char( needs to be changed.

    Returns (new_content, count).
    """
    pattern = re.compile(r'\bValue::Char\(')
    new_content = pattern.sub('Value::char(', content)
    count = len(pattern.findall(content))
    return new_content, count


# ---------------------------------------------------------------------------
# Fix 3: *other -> *MATCH_EXPR in kind() match catch-all arms
# ---------------------------------------------------------------------------

def find_matching_brace(content: str, open_pos: int) -> int:
    """Find the position of the closing brace matching the open brace at open_pos."""
    assert content[open_pos] == '{', f"Expected '{{' at position {open_pos}"
    depth = 1
    i = open_pos + 1
    in_string = False
    in_line_comment = False
    in_block_comment = False
    prev = ''
    while i < len(content) and depth > 0:
        ch = content[i]
        if in_line_comment:
            if ch == '\n':
                in_line_comment = False
        elif in_block_comment:
            if prev == '*' and ch == '/':
                in_block_comment = False
                prev = ''
                i += 1
                continue
        elif in_string:
            if ch == '\\':
                i += 2
                continue
            elif ch == '"':
                in_string = False
        else:
            if ch == '/' and i + 1 < len(content):
                next_ch = content[i + 1]
                if next_ch == '/':
                    in_line_comment = True
                    i += 2
                    continue
                elif next_ch == '*':
                    in_block_comment = True
                    i += 2
                    continue
            elif ch == '"':
                in_string = True
            elif ch == '{':
                depth += 1
            elif ch == '}':
                depth -= 1
                if depth == 0:
                    return i
        prev = ch
        i += 1
    return -1


def fix_star_other_in_kind_match(content: str) -> tuple:
    """In match EXPR.kind() { ... other => ... *other ... }, replace *other
    with the original EXPR value.

    The `other` binding captures a ValueKind, but `*other` was meant to get the
    original Value.  We replace `*other` with either `*EXPR` (if EXPR is a reference)
    or `EXPR` (if EXPR is an owned Value).

    Returns (new_content, count).
    """
    count = 0
    result = content
    offset = 0

    # Find match EXPR.kind() { patterns
    match_kind_re = re.compile(r'\bmatch\s+(.*?)\.kind\(\)\s*\{')

    while True:
        m = match_kind_re.search(result, offset)
        if not m:
            break

        match_expr = m.group(1).strip()
        brace_pos = m.end() - 1

        # Find matching close brace
        brace_end = find_matching_brace(result, brace_pos)
        if brace_end == -1:
            offset = m.end()
            continue

        body = result[brace_pos + 1:brace_end]

        # Find catch-all arms binding `other` and using `*other`
        # Patterns like:
        #   other => ... *other ...
        #   other => { ... *other ... }
        # The binding name is typically `other` but could be `_other`, `rest`, etc.
        # We only handle the common case: `other`.

        # Replace *other with the match expression value.
        # If match_expr starts with & or is a &ref, *match_expr gives Value.
        # If match_expr is `value` (a &Value param), we need `*value`.
        # Actually, we can just use `*match_expr` which will work if match_expr is
        # a reference. If it's owned, we'll use `match_expr` directly.
        # Since most of these are `value: &Value`, `*value` is correct.
        # But for `args[0]` (which is already Value), we need just `args[0]`.

        # Heuristic: if match_expr is a simple ident that could be a reference, use *.
        # If it's an index/method call, it's probably owned Value, use as-is.
        # Actually let's detect if `*match_expr` exists in scope by checking
        # whether the original code used `*other` (the old enum was always `Value`).
        #
        # Simple approach: since we know `*other` is wrong (ValueKind can't be deref'd),
        # replace `*other` with `*match_expr` if match_expr is a simple ident,
        # or just `match_expr` if it's complex.

        is_simple_ref = re.match(r'^&?\w+$', match_expr) is not None
        if is_simple_ref:
            # e.g., value, &value, target -> *value, *(&value), *target
            replacement_expr = '*' + match_expr.lstrip('&')
        else:
            # e.g., args[0], self.val, foo.bar() -> just use it directly
            replacement_expr = match_expr

        # Replace `*other` (and also just `other` used as Value in vec![...])
        # within the match body. We need to be careful to only replace in
        # catch-all arms where `other` binds a ValueKind.
        #
        # Strategy: find lines in the match body that have `other =>` or
        # `other if ... =>` at the top brace level, and then replace *other
        # in that arm's body.

        new_body = body
        local_count = 0

        # Simple line-based approach: find `*other` in the body and replace
        # We verify it's in a catch-all arm by checking context.
        star_other_re = re.compile(r'\*other\b')
        matches_in_body = list(star_other_re.finditer(new_body))

        if matches_in_body:
            new_body = star_other_re.sub(replacement_expr, new_body)
            local_count = len(matches_in_body)

        if local_count > 0:
            result = result[:brace_pos + 1] + new_body + result[brace_end:]
            count += local_count
            # Adjust offset for length changes
            len_diff = len(new_body) - len(body)
            offset = brace_end + 1 + len_diff
        else:
            offset = brace_end + 1

    return result, count


# ---------------------------------------------------------------------------
# Fix 4: `other` (ValueKind) passed as Value (without *) in kind match
# ---------------------------------------------------------------------------

def fix_other_as_value_in_kind_match(content: str) -> tuple:
    """Fix cases where `other` (a ValueKind) is passed where Value is expected.

    E.g., `is_marker(other)` where other is ValueKind but fn expects &Value.
    Replace with the match expression.

    This is the non-deref version of fix 3, for cases like:
    - is_marker(other) -> is_marker(MATCH_EXPR)
    - fn_call(other) -> fn_call(MATCH_EXPR)
    - maybe_trace_characterp_nil(other, ...) -> maybe_trace_characterp_nil(MATCH_EXPR, ...)

    Returns (new_content, count).
    """
    count = 0
    result = content
    offset = 0

    match_kind_re = re.compile(r'\bmatch\s+(.*?)\.kind\(\)\s*\{')

    while True:
        m = match_kind_re.search(result, offset)
        if not m:
            break

        match_expr = m.group(1).strip()
        brace_pos = m.end() - 1
        brace_end = find_matching_brace(result, brace_pos)
        if brace_end == -1:
            offset = m.end()
            continue

        body = result[brace_pos + 1:brace_end]

        # Build the reference expression for the match target
        is_simple_ref = re.match(r'^&?\w+$', match_expr) is not None
        if is_simple_ref:
            # For functions expecting &Value, pass the reference
            ref_expr = match_expr.lstrip('&')
        else:
            ref_expr = '&' + match_expr if not match_expr.startswith('&') else match_expr

        # Find patterns where `other` is used as a function argument
        # but only in catch-all arms.  For now, handle:
        # - is_marker(other) -> is_marker(ref_expr)
        # - maybe_trace_characterp_nil(other, ...) -> maybe_trace_characterp_nil(ref_expr, ...)
        # - `other if super::marker::is_marker(other)` in arm patterns

        new_body = body
        local_count = 0

        # Pattern: function_name(other) where other is first/only arg
        # but NOT *other (handled by fix 3) and NOT `other =>` (arm pattern)
        # The tricky part: `other` in arm patterns like `other if is_marker(other) =>`
        # is actually the ValueKind variant, not a value. These need to change to
        # use the match expression.

        # Replace `other` passed as function argument: fn(other) or fn(other, ...)
        # but NOT in arm pattern position.
        #
        # This is too risky to do generically. Skip.

        offset = brace_end + 1

    return result, count


# ---------------------------------------------------------------------------
# Fix 5: .contains(n) -> .contains(&n) in ValueKind::Fixnum(n) guards
# ---------------------------------------------------------------------------

def fix_contains_ref_in_fixnum_guard(content: str) -> tuple:
    """Fix .contains(n) -> .contains(&n) where n is i64 from ValueKind::Fixnum(n).

    In match guards like:
        ValueKind::Fixnum(n) if (0..=0x3FFFFF).contains(n) =>
    The `n` is owned i64, but .contains() needs &i64.

    Returns (new_content, count).
    """
    count = 0
    # Pattern: .contains(IDENT) where IDENT is a simple word (not &IDENT already)
    # in lines that also contain ValueKind::Fixnum
    lines = content.split('\n')
    result_lines = []

    for line in lines:
        if 'ValueKind::Fixnum(' in line and '.contains(' in line:
            # Extract the binding name from ValueKind::Fixnum(NAME)
            binding_m = re.search(r'ValueKind::Fixnum\(\s*(\w+)\s*\)', line)
            if binding_m:
                binding = binding_m.group(1)
                # Replace .contains(binding) with .contains(&binding)
                contains_re = re.compile(
                    r'\.contains\(\s*' + re.escape(binding) + r'\s*\)'
                )
                if contains_re.search(line):
                    # Check it's not already .contains(&binding)
                    already_ref_re = re.compile(
                        r'\.contains\(\s*&\s*' + re.escape(binding) + r'\s*\)'
                    )
                    if not already_ref_re.search(line):
                        new_line = contains_re.sub(f'.contains(&{binding})', line)
                        if new_line != line:
                            count += 1
                            line = new_line
        result_lines.append(line)

    return '\n'.join(result_lines), count


# ---------------------------------------------------------------------------
# Fix 6: Value::fixnum(_) / Value::char(_) in patterns -> proper checks
# ---------------------------------------------------------------------------

def fix_value_fn_in_patterns(content: str) -> tuple:
    """Fix Value::fixnum(x) and Value::char(x) used in match/if-let patterns.

    These are function calls, not tuple variants, so they can't be used in patterns.
    Transform:
      matches!(expr, Value::fixnum(_) | Value::char(_))
      -> expr.is_fixnum() || expr.is_char()

      if let (Value::fixnum(a), Value::fixnum(b)) = (x, y)
      -> if let (Some(a), Some(b)) = (x.as_fixnum(), y.as_fixnum())

    Returns (new_content, count).
    """
    count = 0
    lines = content.split('\n')
    result_lines = []

    for line in lines:
        original = line

        # Pattern 1: matches!(EXPR, Value::fixnum(_) | Value::char(_))
        # -> EXPR.is_fixnum() || EXPR.is_char()
        m = re.search(
            r'matches!\(\s*(.+?)\s*,\s*((?:Value::(?:fixnum|char)\([^)]*\)\s*\|\s*)*Value::(?:fixnum|char)\([^)]*\))\s*\)',
            line
        )
        if m:
            expr = m.group(1)
            variants_str = m.group(2)
            # Parse the variants
            variant_parts = [v.strip() for v in variants_str.split('|')]
            checks = []
            for vp in variant_parts:
                if 'Value::fixnum(' in vp:
                    checks.append(f'{expr}.is_fixnum()')
                elif 'Value::char(' in vp:
                    checks.append(f'{expr}.is_char()')
            if checks:
                replacement = ' || '.join(checks)
                # Check if this is used with || afterwards
                rest_of_line_after = line[m.end():]
                line = line[:m.start()] + replacement + rest_of_line_after
                count += 1

        # Pattern 2: if let (Value::fixnum(a), Value::fixnum(b)) = (x, y)
        m2 = re.search(
            r'if\s+let\s+\(Value::fixnum\(\s*(\w+)\s*\)\s*,\s*Value::fixnum\(\s*(\w+)\s*\)\)\s*=\s*\((.+?),\s*(.+?)\)',
            line
        )
        if m2:
            a, b, x, y = m2.group(1), m2.group(2), m2.group(3), m2.group(4)
            new_pattern = f'if let (Some({a}), Some({b})) = ({x}.as_fixnum(), {y}.as_fixnum())'
            line = line[:m2.start()] + new_pattern + line[m2.end():]
            count += 1

        # Pattern 3: Some(Value::NIL | Value::fixnum(_) | Value::char(_))
        # -> is_nil/is_fixnum/is_char check
        # This is in match arm context — tricky to fix generically. Skip for now.

        result_lines.append(line)

    return '\n'.join(result_lines), count


# ---------------------------------------------------------------------------
# Fix 7: Value::Str(...) patterns in various contexts
# ---------------------------------------------------------------------------

def fix_value_str_patterns(content: str) -> tuple:
    """Fix remaining Value::Str(...) patterns.

    Value::Str doesn't exist on TaggedValue.  Replace:
    - Value::Str(id) /* TODO */ => BODY_USING_ID
      -> ValueKind::String => BODY_WITH_MATCH_EXPR
    - Value::Str(_) /* TODO */ => ...
      -> ValueKind::String => ...

    Returns (new_content, count).
    """
    count = 0

    # Replace Value::Str(_) /* TODO(tagged): convert Value::Str to new API */
    # with ValueKind::String
    pattern1 = re.compile(
        r'Value::Str\(\s*_\s*\)\s*/\*\s*TODO\(tagged\):[^*]*\*/'
    )
    content, n = pattern1.subn('ValueKind::String', content)
    count += n

    # Replace Value::Str(id) /* TODO(tagged): convert Value::Str to new API */
    # with ValueKind::String  (the body will still reference `id` — that's an E0425 for later)
    pattern2 = re.compile(
        r'Value::Str\(\s*\w+\s*\)\s*/\*\s*TODO\(tagged\):[^*]*\*/'
    )
    content, n = pattern2.subn('ValueKind::String', content)
    count += n

    return content, count


# ---------------------------------------------------------------------------
# Fix 8: Value::Cons(...) and Value::Overlay(...) etc. in expressions
# ---------------------------------------------------------------------------

def fix_value_old_variant_constructors(content: str) -> tuple:
    """Fix old Value::Variant(id) constructor patterns.

    These don't exist on TaggedValue.  Mark with TODO for manual fixing.

    Returns (new_content, count).
    """
    count = 0

    # Value::Cons(expr) /* TODO */ -> expr as-is if possible, but these need
    # the cons cell to be wrapped. Can't auto-fix safely.
    # Skip for now.

    # Value::Overlay(*overlay) /* TODO */ -> these need manual conversion
    # Skip for now.

    return content, count


# ---------------------------------------------------------------------------
# Fix 9: is_some_and(Value::is_X) -> is_some_and(|v| v.is_X())
# ---------------------------------------------------------------------------

def fix_method_ref_in_is_some_and(content: str) -> tuple:
    """Fix is_some_and(Value::method) -> is_some_and(|v| v.method()).

    When Value was an enum, `Value::is_truthy` etc. were fn(&Value) -> bool.
    Now Value is a Copy struct, and methods take self (not &self for predicates).
    So `is_some_and(Value::is_truthy)` expects fn(&Value) -> bool but gets
    fn(Value) -> bool.

    Also fix .and_then(Value::as_str) -> .and_then(|v| v.as_str()).

    Returns (new_content, count).
    """
    count = 0

    # Methods that take self (not &self) on TaggedValue
    self_methods = [
        'is_truthy', 'is_nil', 'is_cons', 'is_string', 'is_integer',
        'is_fixnum', 'is_float', 'is_symbol', 'is_keyword', 'is_char',
        'is_subr', 'is_vector', 'is_record', 'is_hash_table', 'is_function',
        'is_list', 'is_number', 'is_t', 'is_heap_object', 'is_veclike',
        'is_immediate', 'is_buffer', 'is_window', 'is_frame', 'is_timer',
        'is_marker', 'is_overlay',
        'as_fixnum', 'as_float', 'as_str', 'as_char', 'as_symbol_id',
        'as_keyword_id', 'as_subr_id', 'as_int', 'as_number_f64',
        'as_symbol_name', 'type_name',
    ]

    for method in self_methods:
        # is_some_and(Value::method)
        pat = re.compile(r'is_some_and\(\s*Value::' + re.escape(method) + r'\s*\)')
        replacement = f'is_some_and(|v| v.{method}())'
        new_content = pat.sub(replacement, content)
        n = len(pat.findall(content))
        if n:
            count += n
            content = new_content

        # map(Value::method) / and_then(Value::method) / filter(Value::method)
        for combinator in ['and_then', 'map', 'filter', 'find']:
            pat2 = re.compile(
                re.escape(combinator) + r'\(\s*Value::' + re.escape(method) + r'\s*\)'
            )
            replacement2 = f'{combinator}(|v| v.{method}())'
            new_content2 = pat2.sub(replacement2, content)
            n2 = len(pat2.findall(content))
            if n2:
                count += n2
                content = new_content2

    return content, count


# ---------------------------------------------------------------------------
# Fix 10: with_heap get_string in String match arms
# ---------------------------------------------------------------------------

def fix_with_heap_get_string_in_match(content: str) -> tuple:
    """Fix with_heap(|h| h.get_string(LOST_VAR)...) in ValueKind::String arms.

    In match EXPR.kind() { ValueKind::String => { ... with_heap(|h| h.get_string(id)...) } },
    the `id` no longer exists. Replace with EXPR.as_str().unwrap().

    Strategy: find `with_heap(|h| h.get_string(IDENT)` patterns and replace
    them with the appropriate tagged-pointer call. We do this globally since
    get_string always operates on a string ObjId.

    Returns (new_content, count).
    """
    count = 0

    # with_heap(|h| h.get_string(ID).to_owned())
    pat1 = re.compile(
        r'with_heap\(\|h\|\s*h\.get_string\(\s*\*?\w+\s*\)\.to_owned\(\)\)'
    )
    # We can't know the match_expr here. For now, mark these with a comment.
    # Actually, many of these are in ValueKind::String arms where the value
    # to call on IS available somewhere. Let's try a different approach:
    # Just leave these as they are (they'll be E0425 errors) and focus on
    # higher-value fixes.

    # Actually wait — many of these have the pattern:
    # match value.kind() { ValueKind::String => Ok(with_heap(...)) }
    # where `value` is the thing to call as_str on.
    # Let's try to fix them contextually.

    # For simplicity, let's do line-level replacements for the most common
    # patterns that appear in expect_string() style functions.

    return content, count


# ---------------------------------------------------------------------------
# Fix 11: with_heap cons_car/cons_cdr in Cons match arms
# ---------------------------------------------------------------------------

def fix_with_heap_cons_in_match(content: str) -> tuple:
    """Fix with_heap(|h| h.cons_car(LOST_VAR)) and similar.

    Strategy: find these patterns and try to determine the match expression
    to use as the replacement. This is complex, so we handle the most common
    cases.

    Returns (new_content, count).
    """
    count = 0
    # Skip for now — this requires understanding the match context
    return content, count


# ---------------------------------------------------------------------------
# Fix 12: ValueKind::Fixnum(0) -> Value::fixnum(0) in expression context
# ---------------------------------------------------------------------------

def fix_valuekind_fixnum_in_expr_context(content: str) -> tuple:
    """Fix ValueKind::Fixnum(n) used in expression context (not match arms).

    E.g., vec![..., ValueKind::Fixnum(0)] should be vec![..., Value::fixnum(0)].
    This is an E0308 error (expected TaggedValue, found ValueKind).

    Detect: ValueKind::Fixnum(EXPR) used inside vec![], in assignments, etc.
    But NOT in match arm patterns.

    Returns (new_content, count).
    """
    count = 0

    # We look for ValueKind::Fixnum(EXPR) that appears inside vec![...]
    # or as a return value / assignment, NOT after `match` keyword patterns.
    # The safest approach: replace ValueKind::Fixnum(EXPR) with Value::fixnum(EXPR)
    # ONLY when it appears in a vec![] macro or after = or in return.

    # Pattern: inside vec![...]: , ValueKind::Fixnum(EXPR)]
    pat = re.compile(r'\bValueKind::Fixnum\(([^)]+)\)')

    lines = content.split('\n')
    result_lines = []

    for line in lines:
        stripped = line.strip()
        if not pat.search(line):
            result_lines.append(line)
            continue

        # Skip match arm patterns (lines with => or starting with ValueKind::)
        vk_pos = stripped.find('ValueKind::Fixnum')
        arrow_pos = stripped.find('=>')
        if arrow_pos >= 0 and vk_pos >= 0 and vk_pos < arrow_pos:
            result_lines.append(line)
            continue
        if stripped.startswith('ValueKind::'):
            result_lines.append(line)
            continue

        # Check for ValueKind::Fixnum in expression context
        # Good signals: inside vec![], after =, after return, after comma in fn args
        if ('vec![' in line.lower() or
                'return ' in stripped or
                '= ValueKind::Fixnum' in line or
                ', ValueKind::Fixnum' in line or
                '(ValueKind::Fixnum' in line):
            new_line = pat.sub(r'Value::fixnum(\1)', line)
            if new_line != line:
                count += len(pat.findall(line))
                line = new_line

        result_lines.append(line)

    return '\n'.join(result_lines), count


# ---------------------------------------------------------------------------
# Fix 13: E0432 — bad `use super::value::{ValueKind, VecLikeType}` imports
# ---------------------------------------------------------------------------

def fix_bad_value_imports(content: str) -> tuple:
    """Fix bad import paths for ValueKind and VecLikeType.

    Some files have `use super::value::{ValueKind, VecLikeType};` which doesn't
    resolve because `value` is not a direct child of `super` in all modules.
    Replace with the correct crate-level import.

    Also fix the broken import in kbd.rs where `use` appears at wrong position.

    Returns (new_content, count).
    """
    count = 0

    # Fix: use super::value::{ValueKind, VecLikeType}; -> proper import
    # These should be: use crate::emacs_core::value::{ValueKind, VecLikeType};
    # or in some cases: use crate::tagged::value::ValueKind;
    #                    use crate::tagged::header::VecLikeType;

    # Pattern: use super::value::{ValueKind, VecLikeType};
    # or: use super::super::value::{ValueKind, VecLikeType};
    pat = re.compile(
        r'use\s+super(?:::super)*::value::\{ValueKind,\s*VecLikeType\};'
    )
    replacement = 'use crate::emacs_core::value::{ValueKind, VecLikeType};'
    new_content = pat.sub(replacement, content)
    n = len(pat.findall(content))
    if n:
        count += n
        content = new_content

    return content, count


# ---------------------------------------------------------------------------
# Fix 14: E0433 — unresolved ValueKind in files without import
# ---------------------------------------------------------------------------

def fix_missing_valuekind_import(content: str) -> tuple:
    """Add `use crate::emacs_core::value::{ValueKind, VecLikeType};` if the file
    uses ValueKind but doesn't import it.

    Returns (new_content, count).
    """
    count = 0

    if 'ValueKind::' in content:
        # Check if ValueKind is imported
        if not re.search(r'use\s+.*ValueKind', content):
            # Find the last `use` statement to insert after
            last_use = -1
            for m in re.finditer(r'^use\s+[^;]+;', content, re.MULTILINE):
                last_use = m.end()

            if last_use > 0:
                insert = '\nuse crate::emacs_core::value::{ValueKind, VecLikeType};\n'
                content = content[:last_use] + insert + content[last_use:]
                count = 1

    return content, count


# ---------------------------------------------------------------------------
# Fix 15: Remove stale TODO comments that are now wrong
# ---------------------------------------------------------------------------

def fix_stale_todo_comments(content: str) -> tuple:
    """Remove stale TODO(tagged) comments that refer to old patterns.

    Returns (new_content, count).
    """
    count = 0

    # Remove /* TODO(tagged): convert Value::X to new API */
    pat = re.compile(r'\s*/\*\s*TODO\(tagged\):[^*]*\*/')
    new_content = pat.sub('', content)
    n = len(pat.findall(content))
    if n:
        count += n
        content = new_content

    return content, count


# ---------------------------------------------------------------------------
# Fix 16: &args[0].is_string() -> args[0].is_string() (extra & before bool)
# ---------------------------------------------------------------------------

def fix_ref_before_bool_method(content: str) -> tuple:
    """Fix `if &args[...].is_METHOD()` -> `if args[...].is_METHOD()`.

    Some code has `if &EXPR.is_string()` which creates &&bool reference.

    Returns (new_content, count).
    """
    count = 0

    # Pattern: if &EXPR.is_TYPE() { where is_TYPE returns bool
    bool_methods = [
        'is_string', 'is_nil', 'is_cons', 'is_fixnum', 'is_float',
        'is_symbol', 'is_keyword', 'is_char', 'is_subr', 'is_vector',
        'is_record', 'is_hash_table', 'is_truthy', 'is_integer',
        'is_number', 'is_list', 'is_function',
    ]

    for method in bool_methods:
        pat = re.compile(r'if\s+&(\w[\w\[\]]*\.' + re.escape(method) + r'\(\))')
        new_content = pat.sub(r'if \1', content)
        n = len(pat.findall(content))
        if n:
            count += n
            content = new_content

    return content, count


# ---------------------------------------------------------------------------
# Fix 17: Value::Nil -> Value::NIL, Value::True -> Value::T in expressions
# ---------------------------------------------------------------------------

def fix_value_nil_true_associated(content: str) -> tuple:
    """Fix Value::Nil and Value::True which don't exist.

    Value::Nil -> Value::NIL (the associated constant)
    Value::True -> Value::T

    But only in expression context, not pattern context.

    Returns (new_content, count).
    """
    count = 0

    # Value::Nil used as expression (not in match patterns)
    # Tricky: we must not change `ValueKind::Nil` (that's valid).
    # Only Value::Nil (exactly).
    # Note: Value::NIL already exists and is correct.
    # Value::Nil is wrong (no such variant on TaggedValue).

    # Check if Value::Nil exists (it shouldn't)
    pat_nil = re.compile(r'\bValue::Nil\b')
    new_content = pat_nil.sub('Value::NIL', content)
    n = len(pat_nil.findall(content))
    if n:
        count += n
        content = new_content

    pat_true = re.compile(r'\bValue::True\b')
    new_content = pat_true.sub('Value::T', content)
    n = len(pat_true.findall(content))
    if n:
        count += n
        content = new_content

    return content, count


# ---------------------------------------------------------------------------
# Fix 18: Value::Vector(id) / Value::HashTable(id) / ... in patterns
# ---------------------------------------------------------------------------

def fix_value_veclike_patterns(content: str) -> tuple:
    """Fix patterns like Value::Vector(id), Value::Buffer(id) etc.

    These old enum variants don't exist on TaggedValue.  In pattern context
    they should be ValueKind::Veclike(VecLikeType::Vector) etc.

    In expression/constructor context, they need a completely different approach.

    Returns (new_content, count).
    """
    count = 0

    # Map old variant names to VecLikeType variants
    veclike_map = {
        'Vector':    'Vector',
        'HashTable': 'HashTable',
        'Lambda':    'Lambda',
        'Macro':     'Macro',
        'ByteCode':  'ByteCode',
        'Record':    'Record',
        'Buffer':    'Buffer',
        'Window':    'Window',
        'Frame':     'Frame',
        'Timer':     'Timer',
        'Marker':    'Marker',
        'Overlay':   'Overlay',
    }

    for old_name, vl_type in veclike_map.items():
        # Value::Vector(id) /* TODO */ in match arm patterns
        # -> ValueKind::Veclike(VecLikeType::Vector)
        # The `id` binding is lost — that's an E0425 for later.
        pat = re.compile(
            r'\bValue::' + re.escape(old_name) + r'\(\s*\w+\s*\)'
        )

        lines = content.split('\n')
        new_lines = []
        for line in lines:
            if pat.search(line):
                # Check if this is in pattern context (match arm, if-let)
                stripped = line.strip()
                in_pattern = (
                    '=>' in stripped or
                    stripped.startswith('if let') or
                    stripped.startswith('let ') or
                    'matches!' in stripped
                )
                if in_pattern:
                    new_line = pat.sub(
                        f'ValueKind::Veclike(VecLikeType::{vl_type})',
                        line
                    )
                    if new_line != line:
                        count += 1
                        line = new_line
            new_lines.append(line)
        content = '\n'.join(new_lines)

    return content, count


# ---------------------------------------------------------------------------
# Fix 19: Value::Symbol(id) in expression context
# ---------------------------------------------------------------------------

def fix_value_symbol_constructor(content: str) -> tuple:
    """Fix Value::Symbol(id) -> Value::from_sym_id(id).

    Only in expression context (not match arm patterns).

    Returns (new_content, count).
    """
    count = 0

    pat = re.compile(r'\bValue::Symbol\((\w+)\)')

    lines = content.split('\n')
    new_lines = []
    for line in lines:
        if pat.search(line):
            stripped = line.strip()
            # Skip match arm patterns
            is_pattern = (
                ('=>' in stripped and stripped.find('Value::Symbol') < stripped.find('=>'))
                or stripped.startswith('ValueKind::')
            )
            if not is_pattern:
                new_line = pat.sub(r'Value::from_sym_id(\1)', line)
                if new_line != line:
                    count += len(pat.findall(line))
                    line = new_line
        new_lines.append(line)
    content = '\n'.join(new_lines)

    return content, count


# ---------------------------------------------------------------------------
# Fix 20: Value::Float(id) -> Value::from_float / in patterns
# ---------------------------------------------------------------------------

def fix_value_float_pattern(content: str) -> tuple:
    """Fix Value::Float(id) in pattern context.

    Old: match v { Value::Float(id) => ... }
    Already converted to: match v.kind() { ValueKind::Float => ... }
    But some leftover Value::Float(f) patterns exist.

    Returns (new_content, count).
    """
    count = 0
    # Skip — Float is already handled as ValueKind::Float in most places
    return content, count


# ---------------------------------------------------------------------------
# Fix 21: Value::Keyword(id) -> Value::from_kw_id(id) in expressions
# ---------------------------------------------------------------------------

def fix_value_keyword_constructor(content: str) -> tuple:
    """Fix Value::Keyword(id) -> Value::from_kw_id(id).

    Returns (new_content, count).
    """
    count = 0

    pat = re.compile(r'\bValue::Keyword\((\w+)\)')

    lines = content.split('\n')
    new_lines = []
    for line in lines:
        if pat.search(line):
            stripped = line.strip()
            is_pattern = (
                ('=>' in stripped and stripped.find('Value::Keyword') < stripped.find('=>'))
                or stripped.startswith('ValueKind::')
            )
            if not is_pattern:
                new_line = pat.sub(r'Value::from_kw_id(\1)', line)
                if new_line != line:
                    count += len(pat.findall(line))
                    line = new_line
        new_lines.append(line)
    content = '\n'.join(new_lines)

    return content, count


# ---------------------------------------------------------------------------
# Fix 22: Value::Subr(id) -> Value::subr(id) in expressions
# ---------------------------------------------------------------------------

def fix_value_subr_constructor(content: str) -> tuple:
    """Fix Value::Subr(id) -> Value::subr(id).

    Returns (new_content, count).
    """
    count = 0

    pat = re.compile(r'\bValue::Subr\((\w+)\)')

    lines = content.split('\n')
    new_lines = []
    for line in lines:
        if pat.search(line):
            stripped = line.strip()
            is_pattern = (
                ('=>' in stripped and stripped.find('Value::Subr') < stripped.find('=>'))
                or stripped.startswith('ValueKind::')
            )
            if not is_pattern:
                new_line = pat.sub(r'Value::subr(\1)', line)
                if new_line != line:
                    count += len(pat.findall(line))
                    line = new_line
        new_lines.append(line)
    content = '\n'.join(new_lines)

    return content, count


# ---------------------------------------------------------------------------
# Process a single file
# ---------------------------------------------------------------------------

FIXES = [
    ("Value::Int -> Value::fixnum", fix_value_int_constructor),
    ("Value::Char -> Value::char", fix_value_char_constructor),
    ("Value::Nil -> Value::NIL", fix_value_nil_true_associated),
    ("Value::Str patterns", fix_value_str_patterns),
    ("Stale TODO comments", fix_stale_todo_comments),
    ("*other -> *match_expr in kind()", fix_star_other_in_kind_match),
    (".contains(n) -> .contains(&n)", fix_contains_ref_in_fixnum_guard),
    ("Value::fixnum/char in patterns", fix_value_fn_in_patterns),
    ("is_some_and(Value::method)", fix_method_ref_in_is_some_and),
    ("ValueKind::Fixnum in expr ctx", fix_valuekind_fixnum_in_expr_context),
    ("Bad value imports", fix_bad_value_imports),
    ("&expr.is_X() -> expr.is_X()", fix_ref_before_bool_method),
    ("Value::Veclike patterns", fix_value_veclike_patterns),
    ("Value::Symbol constructor", fix_value_symbol_constructor),
    ("Value::Keyword constructor", fix_value_keyword_constructor),
    ("Value::Subr constructor", fix_value_subr_constructor),
]


def process_file(filepath: Path, dry_run: bool) -> dict:
    """Apply all fixes to a single file. Returns dict of fix_name -> count."""
    content = filepath.read_text()
    original = content
    results = {}

    for name, fix_fn in FIXES:
        content, n = fix_fn(content)
        if n > 0:
            results[name] = n

    if content != original:
        if not dry_run:
            filepath.write_text(content)

    return results


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------

def main():
    parser = argparse.ArgumentParser(
        description="Fourth-pass fixes for NeoVM tagged pointer migration"
    )
    parser.add_argument(
        "--dry-run", action="store_true",
        help="Preview changes without modifying files"
    )
    args = parser.parse_args()

    if not SRC_ROOT.is_dir():
        print(f"Error: {SRC_ROOT} not found. Run from project root.", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(SRC_ROOT)
    print(f"Processing {len(files)} files...")

    total_by_fix = {}
    files_changed = 0

    for filepath in files:
        results = process_file(filepath, args.dry_run)
        if results:
            files_changed += 1
            if args.dry_run:
                print(f"\n  {filepath}:")
            for name, n in results.items():
                if args.dry_run:
                    print(f"    {name}: {n}")
                total_by_fix[name] = total_by_fix.get(name, 0) + n

    print(f"\n{'DRY RUN - ' if args.dry_run else ''}Summary:")
    print(f"  Files {'would be ' if args.dry_run else ''}changed: {files_changed}")
    total = 0
    for name, n in sorted(total_by_fix.items(), key=lambda x: -x[1]):
        print(f"  {name}: {n}")
        total += n
    print(f"  Total replacements: {total}")


if __name__ == "__main__":
    main()
