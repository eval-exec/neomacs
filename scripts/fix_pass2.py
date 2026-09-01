#!/usr/bin/env python3
"""
Second-pass mechanical fixes for the NeoVM tagged pointer migration.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Usage:
    python3 scripts/fix_pass2.py            # apply in-place
    python3 scripts/fix_pass2.py --dry-run  # preview changes only
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
# Fix 1: matches!(expr, Value::symbol(id) if resolve_sym(*id) == "name")
#         -> expr.is_symbol_named("name")
#
# Also handles:
#   - assert!(matches!(...))
#   - &expr references
#   - *id and id dereference variants
#   - ref id binding variants
#   - Some(Value::symbol(id)) wrapper
# ---------------------------------------------------------------------------

def fix_matches_symbol(content: str) -> tuple:
    """Convert matches!(expr, Value::symbol(id) if resolve_sym(...) == "name")
    to expr.is_symbol_named("name").

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Pattern A: Simple single-name comparison
    # matches!(EXPR, Value::symbol(REF? ID) if resolve_sym(DEREF? ID) == "NAME")
    # Handles both `*id` and `id` dereferences, and `ref id` bindings
    def replace_simple_symbol(m):
        nonlocal count
        prefix = m.group("prefix") or ""  # e.g. "assert!(" or ""
        neg = m.group("neg") or ""        # e.g. "!" or ""
        expr = m.group("expr").strip()
        name = m.group("name")
        suffix = ")" if prefix else ""    # close assert!( if present

        # Strip leading & from expr for method call
        if expr.startswith("&"):
            expr = expr[1:].strip()

        count += 1
        if neg:
            return f"{prefix}!{expr}.is_symbol_named(\"{name}\"){suffix}"
        else:
            return f"{prefix}{expr}.is_symbol_named(\"{name}\"){suffix}"

    # Pattern: matches!(EXPR, Value::symbol(ref? ID) if resolve_sym(*?ID) == "NAME")
    # With optional assert!( wrapper, optional negation
    pattern_simple = re.compile(
        r'(?P<prefix>assert!\()?'
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'              # the expression being matched
        r',\s*Value::symbol\(\s*'
        r'(?:ref\s+)?'                    # optional `ref`
        r'(?P<id>\w+)\s*\)'              # binding variable
        r'\s+if\s+'
        r'resolve_sym\(\s*\*?'
        r'(?P=id)\s*\)'                   # resolve_sym(*id) or resolve_sym(id)
        r'\s*==\s*"(?P<name>[^"]+)"'      # == "name"
        r'\s*\)'                           # close matches!
        r'(?:\s*\))?'                      # optional close assert!
    )
    content = pattern_simple.sub(replace_simple_symbol, content)

    # Pattern B: matches!(EXPR, Value::symbol(ref? ID) if resolve_sym(*?ID) == "A" || resolve_sym(*?ID) == "B")
    # -> expr.is_symbol_named("A") || expr.is_symbol_named("B")
    # This is the "unspecified" || "relative" pattern in font.rs
    def replace_or_symbol(m):
        nonlocal count
        prefix = m.group("prefix") or ""
        neg = m.group("neg") or ""
        expr = m.group("expr").strip()
        guard_body = m.group("guard")
        suffix = ")" if prefix else ""

        if expr.startswith("&"):
            expr = expr[1:].strip()

        ident = m.group("id")
        # Parse out all `resolve_sym(*?id) == "name"` from the guard
        names = re.findall(
            r'resolve_sym\(\s*\*?' + re.escape(ident) + r'\s*\)\s*==\s*"([^"]+)"',
            guard_body
        )
        if not names:
            return m.group(0)  # no match, leave unchanged

        parts = " || ".join(f'{expr}.is_symbol_named("{n}")' for n in names)
        count += 1
        if neg:
            return f"{prefix}!({parts}){suffix}"
        else:
            return f"{prefix}({parts}){suffix}"

    pattern_or = re.compile(
        r'(?P<prefix>assert!\()?'
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::symbol\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s+if\s+'
        r'(?P<guard>resolve_sym\(\s*\*?\w+\s*\)\s*==\s*"[^"]+"\s*(?:\|\|\s*resolve_sym\(\s*\*?\w+\s*\)\s*==\s*"[^"]+"\s*)+)'
        r'\)'   # close matches!
        r'(?:\s*\))?'  # optional close assert!
    )
    content = pattern_or.sub(replace_or_symbol, content)

    # Pattern C: matches!(EXPR, Some(Value::symbol(id)) if resolve_sym(*?id) == "name")
    # -> EXPR.map_or(false, |v| v.is_symbol_named("name"))
    def replace_some_symbol(m):
        nonlocal count
        prefix = m.group("prefix") or ""
        expr = m.group("expr").strip()
        name = m.group("name")
        suffix = ")" if prefix else ""
        count += 1
        return f"{prefix}{expr}.map_or(false, |v| v.is_symbol_named(\"{name}\")){suffix}"

    pattern_some = re.compile(
        r'(?P<prefix>assert!\()?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Some\(\s*Value::symbol\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)\s*\)'
        r'\s+if\s+'
        r'resolve_sym\(\s*\*?(?P=id)\s*\)'
        r'\s*==\s*"(?P<name>[^"]+)"'
        r'\s*\)'
        r'(?:\s*\))?'
    )
    content = pattern_some.sub(replace_some_symbol, content)

    return content, count


# ---------------------------------------------------------------------------
# Fix 2: matches!(expr, Value::keyword(id) if resolve_sym(*id) == "name")
#         -> expr.as_keyword_id().map_or(false, |k| resolve_sym(k) == "name")
# ---------------------------------------------------------------------------

def fix_matches_keyword(content: str) -> tuple:
    """Convert matches!(expr, Value::keyword(id) if resolve_sym(*id) == "name")
    to expr.as_keyword_id().map_or(false, |k| resolve_sym(k) == "name").

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Simple single-name comparison
    def replace_simple_kw(m):
        nonlocal count
        prefix = m.group("prefix") or ""
        neg = m.group("neg") or ""
        expr = m.group("expr").strip()
        name = m.group("name")
        suffix = ")" if prefix else ""

        if expr.startswith("&"):
            expr = expr[1:].strip()

        count += 1
        inner = f'{expr}.as_keyword_id().map_or(false, |k| resolve_sym(k) == "{name}")'
        if neg:
            return f"{prefix}!{inner}{suffix}"
        else:
            return f"{prefix}{inner}{suffix}"

    pattern_kw_simple = re.compile(
        r'(?P<prefix>assert!\()?'
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::keyword\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s+if\s+'
        r'resolve_sym\(\s*\*?(?P=id)\s*\)'
        r'\s*==\s*"(?P<name>[^"]+)"'
        r'\s*\)'
        r'(?:\s*\))?'
    )
    content = pattern_kw_simple.sub(replace_simple_kw, content)

    # Keyword with block guard: matches!(value, Value::keyword(k) if { let n = resolve_sym(*k); n == ":file" || n == "file" })
    def replace_block_kw(m):
        nonlocal count
        expr = m.group("expr").strip()
        guard_block = m.group("guard")

        if expr.startswith("&"):
            expr = expr[1:].strip()

        # Extract comparisons from guard block
        names = re.findall(r'==\s*"([^"]+)"', guard_block)
        if not names:
            return m.group(0)

        checks = " || ".join(f'n == "{n}"' for n in names)
        count += 1
        return f'{expr}.as_keyword_id().map_or(false, |k| {{ let n = resolve_sym(k); {checks} }})'

    pattern_kw_block = re.compile(
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::keyword\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s+if\s*\{(?P<guard>[^}]+)\}\s*'
        r'\)'
    )
    content = pattern_kw_block.sub(replace_block_kw, content)

    return content, count


# ---------------------------------------------------------------------------
# Fix 3: matches! with combined Value::symbol(id) | Value::keyword(id) guards
#
# Patterns like:
#   matches!(&expr, Value::symbol(id) | Value::keyword(id) if { ... })
#   matches!(&expr, Value::keyword(sym) | Value::symbol(sym) if { ... })
# -> expr.as_symbol_id().or_else(|| expr.as_keyword_id()).map_or(false, |id| { ... })
# ---------------------------------------------------------------------------

def fix_matches_symbol_or_keyword(content: str) -> tuple:
    """Convert matches with combined Value::symbol|keyword patterns.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Pattern: matches!(EXPR, Value::{symbol,keyword}(ID) | Value::{keyword,symbol}(ID) if { GUARD })
    # where GUARD uses resolve_sym(*id) or resolve_sym(id)
    def replace_combined_block(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = m.group("expr").strip()
        ident = m.group("id")
        guard = m.group("guard").strip()

        if expr.startswith("&"):
            expr = expr[1:].strip()

        # Rewrite guard: replace resolve_sym(*id) with resolve_sym(id_)
        # using a fresh variable name
        guard_rewritten = re.sub(
            r'resolve_sym\(\s*\*?' + re.escape(ident) + r'\s*\)',
            'resolve_sym(id_)',
            guard,
        )

        count += 1
        inner = f'{expr}.as_symbol_id().or_else(|| {expr}.as_keyword_id()).map_or(false, |id_| {{ {guard_rewritten} }})'
        if neg:
            return f"!{inner}"
        else:
            return inner

    # With block guard { ... }
    pattern_combined_block = re.compile(
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s*\|\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P=id)\s*\)'
        r'\s+if\s*\{(?P<guard>[^}]+)\}\s*'
        r'\)'
    )
    content = pattern_combined_block.sub(replace_combined_block, content)

    # With inline guard (no braces)
    def replace_combined_inline(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = m.group("expr").strip()
        ident = m.group("id")
        guard = m.group("guard").strip()

        if expr.startswith("&"):
            expr = expr[1:].strip()

        guard_rewritten = re.sub(
            r'resolve_sym\(\s*\*?' + re.escape(ident) + r'\s*\)',
            'resolve_sym(id_)',
            guard,
        )

        count += 1
        inner = f'{expr}.as_symbol_id().or_else(|| {expr}.as_keyword_id()).map_or(false, |id_| {guard_rewritten})'
        if neg:
            return f"!{inner}"
        else:
            return inner

    pattern_combined_inline = re.compile(
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s*\|\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P=id)\s*\)'
        r'\s+if\s+(?P<guard>resolve_sym[^)]+\)\s*==\s*"[^"]+"\s*(?:\|\|\s*resolve_sym[^)]+\)\s*==\s*"[^"]+"\s*)*)'
        r'\)'
    )
    content = pattern_combined_inline.sub(replace_combined_inline, content)

    return content, count


# ---------------------------------------------------------------------------
# Fix 4: Value::Float(expr, next_float_id()) -> Value::make_float(expr)
#
# Handles both single-line and multi-line forms.
# ---------------------------------------------------------------------------

def fix_float_constructors(content: str) -> tuple:
    """Convert Value::Float(expr, next_float_id()) to Value::make_float(expr).

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Single-line: Value::Float(EXPR, next_float_id())  // TODO ...
    def replace_single_line_float(m):
        nonlocal count
        f_expr = m.group(1).strip()
        count += 1
        return f"Value::make_float({f_expr})"

    pattern_single = re.compile(
        r'Value::Float\(\s*'
        r'(.+?)'
        r',\s*next_float_id\(\)\s*\)'
        r'(?:\s*//[^\n]*)?'   # optional trailing comment
    )

    # We need to be careful with multi-line. Let's handle them line-by-line
    # with a state machine approach.

    lines = content.split('\n')
    new_lines = []
    i = 0
    while i < len(lines):
        line = lines[i]

        # Single-line case: everything on one line
        if 'Value::Float(' in line and 'next_float_id()' in line:
            new_line = pattern_single.sub(replace_single_line_float, line)
            if new_line != line:
                new_lines.append(new_line)
                i += 1
                continue

        # Multi-line case: Value::Float( on this line, closing ) later
        # Detect: line contains Value::Float( but NOT next_float_id()
        m_start = re.search(r'Value::Float\(\s*$', line.rstrip().rstrip(',').rstrip(')'))
        if m_start is None and 'Value::Float(' in line and 'next_float_id()' not in line:
            # Check if this is the start of a multi-line Value::Float(
            # Collect lines until we find next_float_id() or closing )
            block_lines = [line]
            j = i + 1
            found_end = False
            while j < len(lines) and j < i + 10:  # limit search
                block_lines.append(lines[j])
                if 'next_float_id()' in lines[j]:
                    found_end = True
                    break
                j += 1

            if found_end:
                block = '\n'.join(block_lines)
                # Try to match the multi-line pattern
                ml_pattern = re.compile(
                    r'Value::Float\(\s*\n'
                    r'(\s*)(.*?),\s*\n'                   # the float expression (line 2)
                    r'\s*next_float_id\(\),?\s*'          # next_float_id() line
                    r'(?://[^\n]*)?\s*\n?'                # optional comment
                    r'(\s*)\)',                            # closing )
                    re.DOTALL,
                )
                ml_match = ml_pattern.search(block)
                if ml_match:
                    indent1 = ml_match.group(1)
                    f_expr = ml_match.group(2).strip()
                    indent2 = ml_match.group(3)
                    # Reconstruct as Value::make_float(expr)
                    replacement = f'Value::make_float({f_expr})'
                    new_block = block[:ml_match.start()] + replacement + block[ml_match.end():]
                    # Adjust: figure out how many original lines to skip
                    new_lines.extend(new_block.split('\n'))
                    count += 1
                    i = j + 1
                    continue

                # Alternative multi-line pattern: Value::Float(\n  COMPLEX_EXPR,\n  next_float_id(), /...\n)
                # where the float expression might be on the same line as Value::Float(
                ml_pattern2 = re.compile(
                    r'Value::Float\(\s*\n?'
                    r'(.*?),\s*\n'
                    r'\s*next_float_id\(\),?\s*'
                    r'(?://[^\n]*)?\s*\n?'
                    r'(\s*)\)',
                    re.DOTALL,
                )
                ml_match2 = ml_pattern2.search(block)
                if ml_match2:
                    f_expr = ml_match2.group(1).strip()
                    replacement = f'Value::make_float({f_expr})'
                    new_block = block[:ml_match2.start()] + replacement + block[ml_match2.end():]
                    new_lines.extend(new_block.split('\n'))
                    count += 1
                    i = j + 1
                    continue

        new_lines.append(line)
        i += 1

    content = '\n'.join(new_lines)

    # Second pass: catch remaining single-line patterns we might have missed
    prev_count = count
    content = pattern_single.sub(replace_single_line_float, content)

    # Also handle Value::Float(expr, 0) or Value::Float(expr, some_id)
    # These are constructors that weren't tagged with next_float_id()
    def replace_float_other_id(m):
        nonlocal count
        f_expr = m.group(1).strip()
        count += 1
        return f"Value::make_float({f_expr})"

    # Value::Float(expr, 0) -- literal 0 as the id
    content = re.sub(
        r'Value::Float\(\s*(.+?)\s*,\s*0\s*\)',
        replace_float_other_id,
        content,
    )

    return content, count


# ---------------------------------------------------------------------------
# Fix 5: value.str_id() -> value.str_ptr_key() with TODO
# ---------------------------------------------------------------------------

def fix_str_id_calls(content: str) -> tuple:
    """Replace .str_id() calls with .str_ptr_key() + TODO comment.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    def replace_str_id(m):
        nonlocal count
        count += 1
        return f"{m.group(1)}.str_ptr_key() /* TODO(tagged): was .str_id(), verify semantics */"

    content = re.sub(
        r'(\w+(?:\.\w+\(\))*?)\.str_id\(\)',
        replace_str_id,
        content,
    )
    return content, count


# ---------------------------------------------------------------------------
# Fix 6: ValueKind::... | Value::Float(..) mixed match arms
#
# The first pass sometimes left mixed patterns like:
#   ValueKind::Fixnum(_) | Value::Float(..) => ...
# Convert Value::Float(..) to ValueKind::Float(_) in these contexts.
# ---------------------------------------------------------------------------

def fix_mixed_valuekind_value_float(content: str) -> tuple:
    """Fix mixed ValueKind/Value patterns containing Value::Float.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    def replace_mixed(m):
        nonlocal count
        count += 1
        return m.group(0).replace('Value::Float(..)', 'ValueKind::Float(_)')

    # Pattern: anything | Value::Float(..) in a match arm context
    content = re.sub(
        r'ValueKind::\w+\([^)]*\)\s*\|\s*Value::Float\(\.\.\)',
        replace_mixed,
        content,
    )
    # Also the reverse order
    content = re.sub(
        r'Value::Float\(\.\.\)\s*\|\s*ValueKind::\w+\([^)]*\)',
        lambda m: (count := count) or m.group(0).replace('Value::Float(..)', 'ValueKind::Float(_)'),
        content,
    )
    # Actually that lambda trick won't work. Do it properly:
    lines = content.split('\n')
    new_lines = []
    for line in lines:
        if 'Value::Float(..)' in line and 'ValueKind::' in line:
            new_line = line.replace('Value::Float(..)', 'ValueKind::Float(_)')
            if new_line != line:
                count += 1
            new_lines.append(new_line)
        else:
            new_lines.append(line)
    content = '\n'.join(new_lines)

    return content, count


# ---------------------------------------------------------------------------
# Fix 7: matches! with Value::symbol only (no guard) - type-only checks
#
# matches!(expr, Value::symbol(_))  ->  expr.is_symbol()
# matches!(expr, Value::keyword(_)) ->  expr.is_keyword()
# matches!(expr, Value::keyword(_) | Value::symbol(_)) ->  expr.is_symbol() || expr.is_keyword()
# matches!(expr, Value::keyword(_) | Value::symbol(_) | Value::NIL) -> ...
# ---------------------------------------------------------------------------

def fix_matches_type_only(content: str) -> tuple:
    """Convert matches! with Value:: type checks (no guard) to method calls.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Helper: strip & prefix and return clean expr
    def clean_expr(expr):
        expr = expr.strip()
        if expr.startswith("&"):
            expr = expr[1:].strip()
        return expr

    # matches!(expr, Value::symbol(_)) -> expr.is_symbol()
    # Also handles !matches!(...)
    def replace_is_symbol(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = clean_expr(m.group("expr"))
        count += 1
        return f'{neg}{expr}.is_symbol()'

    content = re.sub(
        r'(?P<neg>!)?matches!\(\s*(?P<expr>[^,]+?)\s*,\s*Value::symbol\(\s*_\s*\)\s*\)',
        replace_is_symbol,
        content,
    )

    # matches!(expr, Value::keyword(_)) -> expr.is_keyword()
    def replace_is_keyword(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = clean_expr(m.group("expr"))
        count += 1
        return f'{neg}{expr}.is_keyword()'

    content = re.sub(
        r'(?P<neg>!)?matches!\(\s*(?P<expr>[^,]+?)\s*,\s*Value::keyword\(\s*_\s*\)\s*\)',
        replace_is_keyword,
        content,
    )

    # matches!(expr, Value::Str(_) /* TODO ... */) -> expr.is_string()
    def replace_is_string(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = clean_expr(m.group("expr"))
        count += 1
        return f'{neg}{expr}.is_string()'

    content = re.sub(
        r'(?P<neg>!)?matches!\(\s*(?P<expr>[^,]+?)\s*,\s*Value::Str\(\s*_\s*\)\s*/\*[^*]*\*/\s*\)',
        replace_is_string,
        content,
    )

    # matches!(key, Value::keyword(_) | Value::symbol(_) | Value::NIL)
    # -> key.is_keyword() || key.is_symbol() || key.is_nil()
    # !matches!(...) -> !(key.is_keyword() || key.is_symbol())
    def replace_multi_type(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = clean_expr(m.group("expr"))
        alts = m.group("alts")
        # Parse alternatives
        parts = [p.strip() for p in alts.split('|')]
        method_parts = []
        for p in parts:
            p = p.strip()
            if re.match(r'Value::keyword\(\s*_\s*\)', p):
                method_parts.append(f'{expr}.is_keyword()')
            elif re.match(r'Value::symbol\(\s*_\s*\)', p):
                method_parts.append(f'{expr}.is_symbol()')
            elif p == 'Value::NIL':
                method_parts.append(f'{expr}.is_nil()')
            else:
                return m.group(0)  # can't convert, leave as-is
        count += 1
        inner = ' || '.join(method_parts)
        if neg:
            return f'!({inner})'
        return inner

    content = re.sub(
        r'(?P<neg>!)?matches!\(\s*(?P<expr>[^,]+?)\s*,\s*(?P<alts>(?:Value::(?:keyword|symbol)\(\s*_\s*\)|Value::NIL)(?:\s*\|\s*(?:Value::(?:keyword|symbol)\(\s*_\s*\)|Value::NIL))+)\s*\)',
        replace_multi_type,
        content,
    )

    return content, count


# ---------------------------------------------------------------------------
# Fix 8: matches! with Value::symbol guard using || logic inside block
#
# matches!(&value, Value::symbol(id) if resolve_sym(*id) == "a" || resolve_sym(*id) == "b")
# -> value.is_symbol_named("a") || value.is_symbol_named("b")
#
# (This catches the font.rs patterns like == "unspecified" || == "relative")
# ---------------------------------------------------------------------------

def fix_matches_symbol_multi_or(content: str) -> tuple:
    """Convert matches with Value::symbol and multi-name || guards.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Pattern: matches!(&EXPR, Value::symbol(ref? ID) if resolve_sym(*?ID) == "A" || resolve_sym(*?ID) == "B")
    def replace_symbol_or(m):
        nonlocal count
        neg = m.group("neg") or ""
        expr = m.group("expr").strip()
        ident = m.group("id")
        guard = m.group("guard")

        if expr.startswith("&"):
            expr = expr[1:].strip()

        names = re.findall(
            r'resolve_sym\(\s*\*?' + re.escape(ident) + r'\s*\)\s*==\s*"([^"]+)"',
            guard
        )
        if not names:
            return m.group(0)

        parts = " || ".join(f'{expr}.is_symbol_named("{n}")' for n in names)
        count += 1
        if neg:
            return f"!({parts})"
        else:
            return f"({parts})" if len(names) > 1 else parts

    pattern = re.compile(
        r'(?P<neg>!)?'
        r'matches!\(\s*'
        r'(?P<expr>[^,]+?)'
        r',\s*Value::symbol\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
        r'\s+if\s+'
        r'(?P<guard>resolve_sym\(\s*\*?\w+\s*\)\s*==\s*"[^"]+"\s*\|\|\s*resolve_sym\(\s*\*?\w+\s*\)\s*==\s*"[^"]+"'
        r'(?:\s*\|\|\s*resolve_sym\(\s*\*?\w+\s*\)\s*==\s*"[^"]*")*'
        r')\s*\)'
    )
    content = pattern.sub(replace_symbol_or, content)

    return content, count


# ---------------------------------------------------------------------------
# Fix 9: matches! with Value::symbol guard using matches! inner macro
#
# matches!(&args[1], Value::symbol(id) | Value::keyword(id) if {
#     matches!(resolve_sym(*id), "unspecified" | ":ignore-defface" | "ignore-defface")
# })
# -> args[1].as_symbol_id().or_else(|| args[1].as_keyword_id()).map_or(false, |id_| {
#     matches!(resolve_sym(id_), "unspecified" | ":ignore-defface" | "ignore-defface")
# })
# ---------------------------------------------------------------------------

def fix_matches_with_inner_matches(content: str) -> tuple:
    """Convert combined symbol|keyword patterns with inner matches! guard.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # We need to find patterns that span multiple lines, with inner matches! macro
    # Pattern: matches!(EXPR, Value::{sym|kw}(ID) | Value::{kw|sym}(ID) if {
    #     matches!(resolve_sym(*id), "a" | "b" | "c")
    # })
    # The inner matches!() contains parens, so we can't use [^)]+ for it.
    # Instead, use a function to find the balanced braces for the guard block.

    def find_and_replace(content):
        """Scan for combined symbol|keyword patterns with inner matches! guard blocks."""
        nonlocal count
        # Regex to find the start of the pattern, up to the `if {` part
        start_re = re.compile(
            r'matches!\(\s*'
            r'(?P<expr>[^,]+?)'
            r',\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P<id>\w+)\s*\)'
            r'\s*\|\s*Value::(?:symbol|keyword)\(\s*(?:ref\s+)?(?P=id)\s*\)'
            r'\s+if\s*\{',
            re.MULTILINE,
        )

        result = []
        pos = 0
        for m in start_re.finditer(content):
            # Found start of pattern. Now find matching closing `})`
            brace_start = m.end() - 1  # position of the `{`
            depth = 1
            i = m.end()
            while i < len(content) and depth > 0:
                if content[i] == '{':
                    depth += 1
                elif content[i] == '}':
                    depth -= 1
                i += 1

            if depth != 0:
                continue  # unbalanced, skip

            brace_end = i  # position after the closing `}`
            # Now expect `)` or `);` after the closing `}`
            rest = content[brace_end:brace_end + 10].lstrip()
            if not rest.startswith(')'):
                continue

            # Find the closing `)` of matches!
            close_paren = content.index(')', brace_end)
            full_end = close_paren + 1

            guard_body = content[m.end():brace_end - 1].strip()  # content inside { ... }
            expr = m.group("expr").strip()
            ident = m.group("id")

            if expr.startswith("&"):
                expr = expr[1:].strip()

            # Rewrite guard: replace resolve_sym(*id) with resolve_sym(id_)
            guard_rewritten = re.sub(
                r'resolve_sym\(\s*\*?' + re.escape(ident) + r'\s*\)',
                'resolve_sym(id_)',
                guard_body,
            )

            replacement = (
                f'{expr}.as_symbol_id().or_else(|| {expr}.as_keyword_id())'
                f'.map_or(false, |id_| {{\n        {guard_rewritten}\n    }})'
            )

            result.append(content[pos:m.start()])
            result.append(replacement)
            pos = full_end
            count += 1

        result.append(content[pos:])
        return ''.join(result)

    content = find_and_replace(content)
    return content, count


# ---------------------------------------------------------------------------
# Fix 10: Multi-line Value::Float constructors that weren't caught
#
# Handle remaining patterns where Value::Float( is on one line and
# the expression + next_float_id() span multiple lines
# ---------------------------------------------------------------------------

def fix_multiline_float_remaining(content: str) -> tuple:
    """Catch remaining multi-line Value::Float constructors.

    Returns (new_content, count_of_fixes).
    """
    count = 0

    # Pattern: Value::Float(\n  EXPR,\n  next_float_id(), // comment\n)
    # Greedy multi-line
    def replace_ml_float(m):
        nonlocal count
        f_expr = m.group(1).strip().rstrip(',')
        count += 1
        return f"Value::make_float({f_expr})"

    content = re.sub(
        r'Value::Float\(\s*\n'
        r'((?:.*?\n)*?)'                         # capture all lines of float expr
        r'\s*next_float_id\(\)\s*,?\s*'
        r'(?://[^\n]*)?\s*\n?'                    # optional comment
        r'\s*\)',
        replace_ml_float,
        content,
    )

    # Also: Value::Float(\n  complex_if_expr,\n  next_float_id(), / comment\n)
    # Note the "/" instead of "//" -- the first pass mangled some comments
    content = re.sub(
        r'Value::Float\(\s*\n'
        r'((?:.*?\n)*?)'
        r'\s*next_float_id\(\)\s*,?\s*'
        r'(?:/[^\n]*)?\s*\n?'
        r'\s*\)',
        lambda m: (count := count) or m.group(0),  # only match for logging
        content,
    )
    # Actually re-do properly:
    def replace_ml_float2(m):
        nonlocal count
        f_expr = m.group(1).strip().rstrip(',')
        count += 1
        return f"Value::make_float({f_expr})"

    content = re.sub(
        r'Value::Float\(\s*\n'
        r'((?:.*?\n)*?)'
        r'\s*next_float_id\(\)\s*,?\s*'
        r'/[^\n]*\n'                              # "/" comment (mangled)
        r'\s*\)',
        replace_ml_float2,
        content,
    )

    return content, count


# ---------------------------------------------------------------------------
# Main processing
# ---------------------------------------------------------------------------

def process_file(filepath: Path, dry_run: bool) -> dict:
    """Process a single .rs file. Returns dict of fix counts."""
    content = filepath.read_text(encoding='utf-8')
    original = content
    stats = {}

    # Apply fixes in order.
    # Note: combined symbol|keyword patterns must be processed BEFORE
    # individual symbol/keyword patterns, otherwise partial matches occur.

    # Fix 9: matches with inner matches! (most specific, do first)
    content, n = fix_matches_with_inner_matches(content)
    if n:
        stats['matches_inner_matches'] = n

    # Fix 3: combined symbol | keyword guards
    content, n = fix_matches_symbol_or_keyword(content)
    if n:
        stats['matches_symbol_or_keyword'] = n

    # Fix 8: symbol with || multi-name guards
    content, n = fix_matches_symbol_multi_or(content)
    if n:
        stats['matches_symbol_multi_or'] = n

    # Fix 1: matches with Value::symbol
    content, n = fix_matches_symbol(content)
    if n:
        stats['matches_symbol'] = n

    # Fix 2: matches with Value::keyword
    content, n = fix_matches_keyword(content)
    if n:
        stats['matches_keyword'] = n

    # Fix 7: type-only matches (no guard)
    content, n = fix_matches_type_only(content)
    if n:
        stats['matches_type_only'] = n

    # Fix 4 + 10: Value::Float constructors (single-line first, then multi-line)
    content, n = fix_float_constructors(content)
    if n:
        stats['float_constructors'] = n

    content, n = fix_multiline_float_remaining(content)
    if n:
        stats['float_multiline'] = n

    # Fix 5: .str_id() calls
    content, n = fix_str_id_calls(content)
    if n:
        stats['str_id_calls'] = n

    # Fix 6: mixed ValueKind/Value::Float patterns
    content, n = fix_mixed_valuekind_value_float(content)
    if n:
        stats['mixed_valuekind_float'] = n

    if content != original:
        if not dry_run:
            filepath.write_text(content, encoding='utf-8')

    return stats


def main():
    parser = argparse.ArgumentParser(
        description="Second-pass mechanical fixes for tagged pointer migration"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview changes without modifying files",
    )
    args = parser.parse_args()

    if not SRC_ROOT.exists():
        print(f"Error: {SRC_ROOT} not found. Run from the project root.", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(SRC_ROOT)
    print(f"Scanning {len(files)} .rs files under {SRC_ROOT}/...")
    if args.dry_run:
        print("(DRY RUN -- no files will be modified)\n")
    else:
        print()

    total_stats = {}
    files_changed = 0

    for filepath in files:
        stats = process_file(filepath, dry_run=args.dry_run)
        if stats:
            files_changed += 1
            rel = filepath.relative_to(SRC_ROOT)
            detail = ", ".join(f"{k}: {v}" for k, v in sorted(stats.items()))
            print(f"  {rel}: {detail}")
            for k, v in stats.items():
                total_stats[k] = total_stats.get(k, 0) + v

    print(f"\n{'=' * 60}")
    print(f"Files scanned:  {len(files)}")
    print(f"Files changed:  {files_changed}")
    print(f"\nFix summary:")
    grand_total = 0
    for k, v in sorted(total_stats.items()):
        print(f"  {k:35s} {v:5d}")
        grand_total += v
    print(f"  {'TOTAL':35s} {grand_total:5d}")

    if args.dry_run:
        print("\n(DRY RUN -- no files were modified)")


if __name__ == "__main__":
    main()
