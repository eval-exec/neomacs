#!/usr/bin/env python3
"""
Mechanically transform old Value enum usage to new TaggedValue-based API.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Usage:
    python3 scripts/transform_tagged.py            # apply in-place
    python3 scripts/transform_tagged.py --dry-run  # preview changes only
"""

import argparse
import os
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

# All Value:: variant names that appear in pattern/match contexts
VALUE_VARIANTS = {
    "Nil", "True", "Int", "Float", "Symbol", "Keyword", "Char", "Subr",
    "Cons", "Str", "Vector", "Record", "HashTable", "Lambda", "Macro",
    "ByteCode", "Marker", "Overlay", "Buffer", "Window", "Frame", "Timer",
}

VARIANT_PATTERN = r'\bValue::(' + '|'.join(sorted(VALUE_VARIANTS)) + r')\b'

# Mapping from Value variant to VecLikeType enum variant name
VECLIKE_MAP = {
    "Vector": "Vector",
    "Record": "Record",
    "HashTable": "HashTable",
    "Lambda": "Lambda",
    "Macro": "Macro",
    "ByteCode": "ByteCode",
    "Marker": "Marker",
    "Overlay": "Overlay",
    "Buffer": "Buffer",
    "Window": "Window",
    "Frame": "Frame",
    "Timer": "Timer",
}

# Mapping for if-let accessor methods
IF_LET_ACCESSOR = {
    "Int": ("as_fixnum", True),
    "Symbol": ("as_symbol_id", True),
    "Keyword": ("as_keyword_id", True),
    "Char": ("as_char", True),
    "Subr": ("as_subr_id", True),
}

# Mapping for if-let predicate methods (heap objects that lose ObjId)
IF_LET_PREDICATE = {
    "Cons": "is_cons",
    "Str": "is_string",
    "Vector": "is_vector",
    "Record": "is_record",
    "HashTable": "is_hash_table",
    "Lambda": "is_lambda",
    "Macro": "is_macro",
    "ByteCode": "is_bytecode",
    "Buffer": "is_buffer",
    "Window": "is_window",
    "Frame": "is_frame",
    "Marker": "is_marker",
    "Overlay": "is_overlay",
    "Timer": "is_timer",
}

CONSTRUCTOR_TODO = {
    "Str", "Cons", "Vector", "Record", "HashTable", "Lambda", "Macro",
    "ByteCode", "Marker", "Overlay",
}


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------

def find_rs_files(root: Path) -> list:
    """Find all .rs files under root, excluding the tagged/ subdirectory."""
    tagged_dir = root / "tagged"
    result = []
    for dirpath, dirnames, filenames in os.walk(root):
        dp = Path(dirpath)
        if dp == tagged_dir or tagged_dir in dp.parents:
            continue
        for f in filenames:
            if f.endswith(".rs"):
                result.append(dp / f)
    return sorted(result)


def line_has_value_arm(line: str) -> bool:
    """Check if a line contains a Value:: variant in pattern position (before =>)."""
    # Only check the part before => (the pattern part)
    pattern_part, sep, _ = split_at_arrow(line)
    if not sep:
        # No => on this line -- might be a continuation of a multi-line pattern
        pattern_part = line

    stripped = pattern_part.strip()
    # Match Value:: with optional module path prefix (e.g., super::value::Value::)
    return bool(re.search(
        r'\bValue::(' + '|'.join(sorted(VALUE_VARIANTS)) + r')',
        stripped,
    ))


# ---------------------------------------------------------------------------
# Phase 1: if let / while let / let-else transformations
# ---------------------------------------------------------------------------

def transform_if_let_while_let(content: str) -> str:
    """Transform if let / while let / let-else patterns with Value:: variants."""

    # 1a. Float: if let Value::Float(f, _) = expr  -> if let Some(f) = expr.as_float()
    content = re.sub(
        r'\b(if\s+let|while\s+let)\s+Value::Float\(\s*(\w+)\s*,\s*\w+\s*\)\s*=\s*(&?)(.+?)(?=\s*\{)',
        lambda m: f'{m.group(1)} Some({m.group(2)}) = {m.group(3)}{m.group(4)}.as_float()',
        content,
    )
    content = re.sub(
        r'\blet\s+Value::Float\(\s*(\w+)\s*,\s*\w+\s*\)\s*=\s*(&?)(.+?)\s+else\b',
        lambda m: f'let Some({m.group(1)}) = {m.group(2)}{m.group(3)}.as_float() else',
        content,
    )

    # 1b. Variants with accessor methods (Int, Symbol, Keyword, Char, Subr)
    for variant, (method, _) in IF_LET_ACCESSOR.items():
        pattern = (
            r'\b(if\s+let|while\s+let)\s+Value::'
            + variant
            + r'\(\s*(\w+)\s*\)\s*=\s*(&?)(.+?)(?=\s*\{)'
        )
        content = re.sub(
            pattern,
            lambda m, meth=method: (
                f'{m.group(1)} Some({m.group(2)}) = {m.group(3)}{m.group(4)}.{meth}()'
            ),
            content,
        )
        pattern_else = (
            r'\blet\s+Value::'
            + variant
            + r'\(\s*(\w+)\s*\)\s*=\s*(&?)(.+?)\s+else\b'
        )
        content = re.sub(
            pattern_else,
            lambda m, meth=method: (
                f'let Some({m.group(1)}) = {m.group(2)}{m.group(3)}.{meth}() else'
            ),
            content,
        )

    # 1c. Variants that become predicate checks (heap objects that lose ObjId)
    for variant, predicate in IF_LET_PREDICATE.items():
        pattern = (
            r'\b(if\s+let|while\s+let)\s+Value::'
            + variant
            + r'\(\s*(\w+)\s*\)\s*=\s*(&?)(.+?)(?=\s*\{)'
        )
        def _make_predicate_replacement(m, pred=predicate, var_name=variant):
            keyword = m.group(1)  # "if let" or "while let"
            # Strip " let" to get "if" or "while"
            bare = keyword.replace(' let', '')
            var = m.group(2)
            ref = m.group(3)
            expr = m.group(4)
            return (
                f'{bare} {ref}{expr}.{pred}()'
                f' /* TODO(tagged): `{var}` was Value::{var_name}({var}), now use accessor */'
            )
        content = re.sub(pattern, _make_predicate_replacement, content)
        pattern_else = (
            r'\blet\s+Value::'
            + variant
            + r'\(\s*(\w+)\s*\)\s*=\s*(&?)(.+?)\s+else\b'
        )
        content = re.sub(
            pattern_else,
            lambda m, pred=predicate, var_name=variant: (
                f'if !{m.group(2)}{m.group(3)}.{pred}()'
                + f' /* TODO(tagged): `{m.group(1)}` was Value::{var_name}({m.group(1)}), rewrite let-else */'
            ),
            content,
        )

    return content


# ---------------------------------------------------------------------------
# Phase 2: Match block detection and transformation
# ---------------------------------------------------------------------------

def find_match_blocks(lines: list) -> list:
    """
    Find match blocks that contain Value:: variant patterns.
    Returns list of (match_line_idx, block_end, is_tuple) tuples.
    Handles nested match blocks by scanning all lines (not skipping consumed blocks).
    """
    blocks = []
    for i in range(len(lines)):
        line = lines[i]
        m = re.match(r'^(\s*)(let\s+\w+\s*(?::\s*\S+\s*)?=\s*)?match\s+(.+?)\s*\{\s*$', line)
        if not m:
            continue

        match_line = i
        expr = m.group(3).strip()
        is_tuple = expr.startswith('(') and ',' in expr

        # Track brace depth to find the end of this match block
        depth = 0
        has_value_arm = False
        j = match_line
        while j < len(lines):
            for ch in lines[j]:
                if ch == '{':
                    depth += 1
                elif ch == '}':
                    depth -= 1
                    if depth == 0:
                        break
            if line_has_value_arm(lines[j]):
                has_value_arm = True
            if depth == 0:
                break
            j += 1

        if has_value_arm:
            blocks.append((match_line, j, is_tuple))

    return blocks


def transform_match_expression(line: str, is_tuple: bool) -> str:
    """Add .kind() to a match expression if it doesn't already have it."""
    if '.kind()' in line:
        return line

    def replacer(m):
        prefix = m.group(1) or ''
        expr = m.group(2).strip()

        if is_tuple:
            # match (a, b) { -> match (a.kind(), b.kind()) {
            # Parse the tuple contents
            inner = expr[1:-1].strip()  # strip parens
            parts = split_tuple_parts(inner)
            new_parts = []
            for part in parts:
                part = part.strip()
                if part.startswith('&'):
                    inner_p = part[1:].strip()
                    new_parts.append(f'{inner_p}.kind()')
                else:
                    new_parts.append(f'{part}.kind()')
            return f'{prefix}match ({", ".join(new_parts)}) {{'
        else:
            if expr.startswith('&'):
                inner = expr[1:].strip()
                if expr.startswith('&mut '):
                    inner = expr[5:].strip()
                return f'{prefix}match {inner}.kind() {{'
            else:
                return f'{prefix}match {expr}.kind() {{'

    return re.sub(
        r'^(\s*(?:let\s+\w+\s*(?::\s*\S+\s*)?=\s*)?)match\s+(.+?)\s*\{\s*$',
        replacer,
        line,
    )


def split_tuple_parts(s: str) -> list:
    """Split tuple parts respecting nesting."""
    parts = []
    depth = 0
    current = []
    for ch in s:
        if ch == '(' or ch == '<':
            depth += 1
            current.append(ch)
        elif ch == ')' or ch == '>':
            depth -= 1
            current.append(ch)
        elif ch == ',' and depth == 0:
            parts.append(''.join(current))
            current = []
        else:
            current.append(ch)
    if current:
        parts.append(''.join(current))
    return parts


def transform_match_arm(line: str) -> str:
    """Transform a match arm from Value:: to ValueKind:: patterns.

    ONLY transforms Value:: that appear BEFORE the => arrow (pattern position).
    Leaves Value:: in the arm body (after =>) untouched.
    """
    # Split the line at the => to separate pattern from body
    # Must be careful about => inside nested expressions
    pattern_part, sep, body_part = split_at_arrow(line)

    if not sep:
        # No => found; might be a continuation line or closing brace
        # Still transform if it looks like a pattern line
        if line_has_value_arm(line):
            pattern_part = _do_arm_replacements(pattern_part)
        return pattern_part

    # Only transform the pattern part
    pattern_part = _do_arm_replacements(pattern_part)

    return pattern_part + sep + body_part


def split_at_arrow(line: str) -> tuple:
    """Split line at the first `=>` that's not inside quotes/parens/brackets."""
    depth = 0
    in_string = False
    escape = False
    i = 0
    while i < len(line):
        c = line[i]
        if escape:
            escape = False
            i += 1
            continue
        if c == '\\':
            escape = True
            i += 1
            continue
        if c == '"':
            in_string = not in_string
            i += 1
            continue
        if in_string:
            i += 1
            continue
        if c in '({[':
            depth += 1
        elif c in ')}]':
            depth -= 1
        elif c == '=' and i + 1 < len(line) and line[i + 1] == '>' and depth == 0:
            return (line[:i], '=>', line[i + 2:])
        i += 1
    return (line, '', '')


def _do_arm_replacements(text: str) -> str:
    """Apply Value:: -> ValueKind:: replacements in pattern text.

    Handles optional ``ref`` / ``ref mut`` before binding variables.
    Also handles optional module path prefix like ``super::value::Value::``.
    """
    BINDING = r'(?:ref\s+(?:mut\s+)?)?\w+'
    # Optional module path before Value:: (e.g., super::value::, crate::...)
    PATH = r'(?:\w+::)*'

    # Value::Nil -> ValueKind::Nil
    text = re.sub(PATH + r'\bValue::Nil\b', 'ValueKind::Nil', text)
    # Value::True -> ValueKind::T
    text = re.sub(PATH + r'\bValue::True\b', 'ValueKind::T', text)
    # Value::Int(n) -> ValueKind::Fixnum(n)
    text = re.sub(PATH + r'\bValue::Int\(\s*(?:ref\s+(?:mut\s+)?)?(\w+)\s*\)', r'ValueKind::Fixnum(\1)', text)
    # Value::Symbol(id) -> ValueKind::Symbol(id)
    text = re.sub(PATH + r'\bValue::Symbol\(\s*(?:ref\s+(?:mut\s+)?)?(\w+)\s*\)', r'ValueKind::Symbol(\1)', text)
    # Value::Keyword(id) -> ValueKind::Keyword(id)
    text = re.sub(PATH + r'\bValue::Keyword\(\s*(?:ref\s+(?:mut\s+)?)?(\w+)\s*\)', r'ValueKind::Keyword(\1)', text)
    # Value::Char(c) -> ValueKind::Char(c)
    text = re.sub(PATH + r'\bValue::Char\(\s*(?:ref\s+(?:mut\s+)?)?(\w+)\s*\)', r'ValueKind::Char(\1)', text)
    # Value::Subr(id) -> ValueKind::Subr(id)
    text = re.sub(PATH + r'\bValue::Subr\(\s*(?:ref\s+(?:mut\s+)?)?(\w+)\s*\)', r'ValueKind::Subr(\1)', text)

    # Value::Float(f, _) or Value::Float(f, id) -> ValueKind::Float
    text = re.sub(
        PATH + r'\bValue::Float\(\s*' + BINDING + r'\s*,\s*' + BINDING + r'\s*\)',
        'ValueKind::Float /* TODO(tagged): extract float via .xfloat() */',
        text,
    )

    # Value::Cons(id) or Value::Cons(_) -> ValueKind::Cons
    text = re.sub(PATH + r'\bValue::Cons\(\s*' + BINDING + r'\s*\)', 'ValueKind::Cons', text)

    # Value::Str(id) or Value::Str(_) -> ValueKind::String
    text = re.sub(PATH + r'\bValue::Str\(\s*' + BINDING + r'\s*\)', 'ValueKind::String', text)

    # Veclike types
    for variant, vtype in VECLIKE_MAP.items():
        if variant in ("Cons", "Str"):
            continue
        text = re.sub(
            PATH + r'\bValue::' + variant + r'\(\s*' + BINDING + r'\s*\)',
            f'ValueKind::Veclike(VecLikeType::{vtype})',
            text,
        )

    return text


def transform_match_blocks(content: str) -> str:
    """Find and transform all match blocks that use Value:: patterns."""
    lines = content.split('\n')
    blocks = find_match_blocks(lines)

    if not blocks:
        return content

    # Track which line ranges are inside match blocks (pattern regions)
    # so Phase 3 can skip them
    # Process blocks in reverse order to preserve line indices
    for match_line, block_end, is_tuple in reversed(blocks):
        # Transform the arms within the block
        for i in range(match_line + 1, block_end + 1):
            if line_has_value_arm(lines[i]):
                lines[i] = transform_match_arm(lines[i])

        # Add .kind() to the match expression
        lines[match_line] = transform_match_expression(lines[match_line], is_tuple)

    return '\n'.join(lines)


# ---------------------------------------------------------------------------
# Phase 3: Constructor replacements (non-pattern context)
# ---------------------------------------------------------------------------

def identify_match_arm_lines(content: str) -> set:
    """Return the set of line numbers that are inside match block arm regions."""
    lines = content.split('\n')
    arm_lines = set()
    blocks = find_match_blocks(lines)
    for match_line, block_end, _ in blocks:
        # Lines from match_line+1 to block_end are inside the match block
        # We conservatively mark all lines within the match block braces
        for i in range(match_line, block_end + 1):
            arm_lines.add(i)
    return arm_lines


def transform_constructors(content: str) -> str:
    """Transform Value:: constructor calls that are NOT in pattern position.

    Uses a line-by-line approach to skip lines that are inside match blocks.
    """
    lines = content.split('\n')

    # Identify lines that are inside match blocks
    arm_lines = identify_match_arm_lines(content)

    for i in range(len(lines)):
        # Skip lines inside match blocks - those were handled by Phase 2
        if i in arm_lines:
            # But still do transforms for code AFTER => in arm bodies
            line = lines[i]
            _, sep, body = split_at_arrow(line)
            if sep and body:
                new_body = _apply_constructor_transforms(body)
                if new_body != body:
                    pattern_part = line[:len(line) - len(sep) - len(body)]
                    lines[i] = pattern_part + sep + new_body
            continue

        lines[i] = _apply_constructor_transforms(lines[i])

    return '\n'.join(lines)


def _apply_constructor_transforms(text: str) -> str:
    """Apply constructor transformations to a piece of text.

    Splits on block comments to avoid transforming Value:: inside /* ... */
    and line comments // ...
    """
    # Split text into code segments and comment segments, transform only code
    parts = _split_code_comments(text)
    result = []
    for part, is_comment in parts:
        if is_comment:
            result.append(part)
        else:
            result.append(_apply_constructor_transforms_raw(part))
    return ''.join(result)


def _split_code_comments(text: str) -> list:
    """Split text into (segment, is_comment) pairs.

    Handles /* ... */ block comments and // line comments.
    """
    segments = []
    i = 0
    current = []
    while i < len(text):
        if text[i] == '/' and i + 1 < len(text) and text[i + 1] == '*':
            # Start of block comment
            if current:
                segments.append((''.join(current), False))
                current = []
            comment = ['/*']
            i += 2
            while i < len(text):
                if text[i] == '*' and i + 1 < len(text) and text[i + 1] == '/':
                    comment.append('*/')
                    i += 2
                    break
                comment.append(text[i])
                i += 1
            segments.append((''.join(comment), True))
        elif text[i] == '/' and i + 1 < len(text) and text[i + 1] == '/':
            # Start of line comment - rest of line is comment
            if current:
                segments.append((''.join(current), False))
                current = []
            comment = []
            while i < len(text) and text[i] != '\n':
                comment.append(text[i])
                i += 1
            segments.append((''.join(comment), True))
        elif text[i] == '"':
            # String literal - include in code segment
            current.append(text[i])
            i += 1
            while i < len(text) and text[i] != '"':
                if text[i] == '\\':
                    current.append(text[i])
                    i += 1
                    if i < len(text):
                        current.append(text[i])
                        i += 1
                else:
                    current.append(text[i])
                    i += 1
            if i < len(text):
                current.append(text[i])
                i += 1
        else:
            current.append(text[i])
            i += 1
    if current:
        segments.append((''.join(current), False))
    return segments


def _apply_constructor_transforms_raw(text: str) -> str:
    """Apply constructor transformations to raw code text (no comments)."""

    # Value::Nil -> Value::NIL
    text = re.sub(r'\bValue::Nil\b', 'Value::NIL', text)

    # Value::True -> Value::T
    text = re.sub(r'\bValue::True\b', 'Value::T', text)

    # Value::Int(expr) -> Value::fixnum(expr)
    text = re.sub(r'\bValue::Int\(', 'Value::fixnum(', text)

    # Value::Float(expr, next_float_id()) -> Value::make_float(expr)
    text = re.sub(
        r'\bValue::Float\(\s*(.+?)\s*,\s*next_float_id\(\)\s*\)',
        r'Value::make_float(\1)',
        text,
    )
    # Value::Float(expr, id_expr) -> Value::make_float(expr) (remaining)
    text = re.sub(
        r'\bValue::Float\(\s*(.+?)\s*,\s*(.+?)\s*\)',
        r'Value::make_float(\1) /* TODO(tagged): dropped float id `\2` */',
        text,
    )

    # Value::Char(expr) -> Value::char(expr)
    text = re.sub(r'\bValue::Char\(', 'Value::char(', text)

    # Value::Symbol(expr) -> Value::symbol(expr)
    text = re.sub(r'\bValue::Symbol\(', 'Value::symbol(', text)

    # Value::Keyword(expr) -> Value::keyword(expr)
    text = re.sub(r'\bValue::Keyword\(', 'Value::keyword(', text)

    # Value::Subr(expr) -> Value::subr(expr)
    text = re.sub(r'\bValue::Subr\(', 'Value::subr(', text)

    # Value::Buffer(expr) -> Value::make_buffer(expr)
    text = re.sub(r'\bValue::Buffer\(', 'Value::make_buffer(', text)

    # Value::Window(expr) -> Value::make_window(expr)
    text = re.sub(r'\bValue::Window\(', 'Value::make_window(', text)

    # Value::Frame(expr) -> Value::make_frame(expr)
    text = re.sub(r'\bValue::Frame\(', 'Value::make_frame(', text)

    # Value::Timer(expr) -> Value::make_timer(expr)
    text = re.sub(r'\bValue::Timer\(', 'Value::make_timer(', text)

    # Heap-allocated types that need manual conversion - add TODO
    for variant in CONSTRUCTOR_TODO:
        pattern = r'\bValue::' + variant + r'\('
        if re.search(pattern, text):
            # Negative lookahead: don't add TODO if one is already there
            # (either directly or the comment was split off leaving trailing whitespace)
            text = re.sub(
                r'(\bValue::' + variant + r'\([^)]*\))(?!\s*/\*\s*TODO)(?!\s*$)',
                r'\1 /* TODO(tagged): convert Value::' + variant + r' to new API */',
                text,
            )

    return text


# ---------------------------------------------------------------------------
# Phase 4: Simple replacements
# ---------------------------------------------------------------------------

def transform_simple(content: str) -> str:
    """Apply simple find-replace transformations."""
    content = re.sub(r'\bValue::t\(\)', 'Value::T', content)
    content = re.sub(r'\bValue::bool\(', 'Value::bool_val(', content)
    return content


# ---------------------------------------------------------------------------
# Phase 5: next_float_id() cleanup
# ---------------------------------------------------------------------------

def transform_next_float_id(content: str) -> str:
    """Remove or comment out standalone next_float_id() calls."""
    lines = content.split('\n')
    result = []
    for line in lines:
        if 'next_float_id()' in line and 'TODO' not in line:
            if 'fn next_float_id' in line or 'pub fn next_float_id' in line:
                result.append(line)
            else:
                result.append(line + '  // TODO(tagged): remove next_float_id()')
        else:
            result.append(line)
    return '\n'.join(result)


# ---------------------------------------------------------------------------
# Phase 6: read_cons TODO markers
# ---------------------------------------------------------------------------

def transform_read_cons(content: str) -> str:
    """Add TODO markers to read_cons usage."""
    lines = content.split('\n')
    result = []
    for line in lines:
        if 'read_cons(' in line and 'TODO' not in line:
            if 'fn read_cons' in line or 'pub fn read_cons' in line:
                result.append(line)
            else:
                result.append(line + '  // TODO(tagged): replace read_cons with cons accessors')
        else:
            result.append(line)
    return '\n'.join(result)


# ---------------------------------------------------------------------------
# Phase 7: matches!() macro transformations
# ---------------------------------------------------------------------------

def transform_matches_macro(content: str) -> str:
    """Transform matches!(self, Value::Variant(...)) patterns."""

    # Simple single-variant matches
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Nil\s*\)',
        r'\1.is_nil()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::True\s*\)',
        r'\1.is_t()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Cons\(\s*_\s*\)\s*\)',
        r'\1.is_cons()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Int\(\s*_\s*\)\s*\)',
        r'\1.is_fixnum()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Float\(\s*_\s*,\s*_\s*\)\s*\)',
        r'\1.is_float()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Char\(\s*_\s*\)\s*\)',
        r'\1.is_char()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Str\(\s*_\s*\)\s*\)',
        r'\1.is_string()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Keyword\(\s*_\s*\)\s*\)',
        r'\1.is_keyword()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Vector\(\s*_\s*\)\s*\)',
        r'\1.is_vector()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Record\(\s*_\s*\)\s*\)',
        r'\1.is_record()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::HashTable\(\s*_\s*\)\s*\)',
        r'\1.is_hash_table()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Symbol\(\s*_\s*\)\s*\)',
        r'\1.is_symbol()',
        content,
    )

    # Common compound patterns
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Nil\s*\|\s*Value::Cons\(\s*_\s*\)\s*\)',
        r'\1.is_nil() || \1.is_cons()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Int\(\s*_\s*\)\s*\|\s*Value::Char\(\s*_\s*\)\s*\|\s*Value::Float\(\s*_\s*,\s*_\s*\)\s*\)',
        r'\1.is_fixnum() || \1.is_char() || \1.is_float()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Int\(\s*_\s*\)\s*\|\s*Value::Char\(\s*_\s*\)\s*\)',
        r'\1.is_fixnum() || \1.is_char()',
        content,
    )
    content = re.sub(
        r'matches!\(\s*(\w+)\s*,\s*Value::Lambda\(\s*_\s*\)\s*\|\s*Value::Subr\(\s*_\s*\)\s*\|\s*Value::ByteCode\(\s*_\s*\)\s*\)',
        r'\1.is_function()',
        content,
    )

    return content


# ---------------------------------------------------------------------------
# Phase 8: Import additions
# ---------------------------------------------------------------------------

def add_imports(content: str, file_path: Path) -> str:
    """Add ValueKind and VecLikeType imports if needed."""

    needs_value_kind = 'ValueKind::' in content
    needs_veclike_type = 'VecLikeType::' in content

    if not needs_value_kind and not needs_veclike_type:
        return content

    has_value_kind = bool(re.search(r'\buse\b.*\bValueKind\b', content))
    has_veclike_type = bool(re.search(r'\buse\b.*\bVecLikeType\b', content))

    if needs_value_kind and has_value_kind:
        needs_value_kind = False
    if needs_veclike_type and has_veclike_type:
        needs_veclike_type = False

    if not needs_value_kind and not needs_veclike_type:
        return content

    to_add = []
    if needs_value_kind:
        to_add.append('ValueKind')
    if needs_veclike_type:
        to_add.append('VecLikeType')

    lines = content.split('\n')
    inserted = False

    # Try: find a use line that imports from value-related paths with braces
    for i, line in enumerate(lines):
        if re.search(r'use\s+.*value::\{.*\}\s*;', line):
            line_new = re.sub(
                r'(\}\s*;)',
                ', ' + ', '.join(to_add) + r'\1',
                line,
            )
            lines[i] = line_new
            inserted = True
            break

    if not inserted:
        # Try: find `use super::value::Value;` or similar single imports
        for i, line in enumerate(lines):
            if re.search(r'use\s+.*value::Value\s*;', line):
                line_new = re.sub(
                    r'(use\s+.*value::)Value\s*;',
                    r'\1{Value, ' + ', '.join(to_add) + '};',
                    line,
                )
                lines[i] = line_new
                inserted = True
                break

    if not inserted:
        # Add after the last use statement
        last_use = -1
        for i, line in enumerate(lines):
            if line.strip().startswith('use '):
                last_use = i
        if last_use >= 0:
            import_line = 'use crate::emacs_core::value::{' + ', '.join(to_add) + '};'
            lines.insert(last_use + 1, import_line)
            inserted = True

    if not inserted:
        import_line = 'use crate::emacs_core::value::{' + ', '.join(to_add) + '};'
        lines.insert(0, import_line)

    return '\n'.join(lines)


# ---------------------------------------------------------------------------
# Main processing pipeline
# ---------------------------------------------------------------------------

def process_file(file_path: Path, dry_run: bool) -> dict:
    """Process a single file. Returns a summary dict."""
    original = file_path.read_text(encoding='utf-8')

    if not re.search(VARIANT_PATTERN, original) \
       and 'next_float_id()' not in original \
       and 'read_cons(' not in original \
       and 'Value::t()' not in original \
       and 'Value::bool(' not in original:
        return {'path': str(file_path), 'changed': False, 'changes': []}

    content = original
    changes = []

    # Phase 1: if let / while let / let-else
    new_content = transform_if_let_while_let(content)
    if new_content != content:
        changes.append('if-let/while-let/let-else patterns')
        content = new_content

    # Phase 7 (before match blocks): matches!() macro
    new_content = transform_matches_macro(content)
    if new_content != content:
        changes.append('matches!() macro patterns')
        content = new_content

    # Phase 2: Match blocks
    new_content = transform_match_blocks(content)
    if new_content != content:
        changes.append('match block patterns')
        content = new_content

    # Phase 3: Constructors
    new_content = transform_constructors(content)
    if new_content != content:
        changes.append('constructor calls')
        content = new_content

    # Phase 4: Simple replacements
    new_content = transform_simple(content)
    if new_content != content:
        changes.append('simple replacements (Value::t(), Value::bool())')
        content = new_content

    # Phase 5: next_float_id()
    new_content = transform_next_float_id(content)
    if new_content != content:
        changes.append('next_float_id() markers')
        content = new_content

    # Phase 6: read_cons
    new_content = transform_read_cons(content)
    if new_content != content:
        changes.append('read_cons() TODO markers')
        content = new_content

    # Phase 8: Imports
    new_content = add_imports(content, file_path)
    if new_content != content:
        changes.append('added imports (ValueKind/VecLikeType)')
        content = new_content

    changed = content != original
    if changed and not dry_run:
        file_path.write_text(content, encoding='utf-8')

    return {'path': str(file_path), 'changed': changed, 'changes': changes}


def main():
    parser = argparse.ArgumentParser(
        description='Transform old Value enum usage to new TaggedValue API'
    )
    parser.add_argument(
        '--dry-run',
        action='store_true',
        help='Preview changes without writing files',
    )
    parser.add_argument(
        '--verbose', '-v',
        action='store_true',
        help='Show unchanged files too',
    )
    args = parser.parse_args()

    script_dir = Path(__file__).resolve().parent
    project_root = script_dir.parent
    src_dir = project_root / 'crates' / 'neovm-core' / 'src'

    if not src_dir.exists():
        print(f"Error: source directory not found: {src_dir}", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(src_dir)
    print(f"Found {len(files)} .rs files to process")
    if args.dry_run:
        print("DRY RUN - no files will be modified\n")
    else:
        print()

    changed_count = 0
    total_changes = 0

    for fpath in files:
        result = process_file(fpath, args.dry_run)
        if result['changed']:
            changed_count += 1
            total_changes += len(result['changes'])
            rel = os.path.relpath(result['path'], project_root)
            print(f"  CHANGED  {rel}")
            for change in result['changes']:
                print(f"           - {change}")
        elif args.verbose:
            rel = os.path.relpath(result['path'], project_root)
            print(f"  (skip)   {rel}")

    print(f"\nSummary: {changed_count} files changed, "
          f"{len(files) - changed_count} unchanged, "
          f"{total_changes} transformation categories applied")
    if args.dry_run:
        print("(dry run - no files were written)")


if __name__ == '__main__':
    main()
