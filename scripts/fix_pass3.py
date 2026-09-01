#!/usr/bin/env python3
"""
Third-pass mechanical fixes for the NeoVM tagged pointer migration.

Processes all .rs files under crates/neovm-core/src/ (except crates/neovm-core/src/tagged/).
Run from the project root directory.

Usage:
    python3 scripts/fix_pass3.py            # apply in-place
    python3 scripts/fix_pass3.py --dry-run  # preview changes only
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

# ValueKind variants that carry owned scalars (not references)
# Pattern: ValueKind::Variant(binding) => ... *binding ... should become ... binding ...
OWNED_VARIANTS = {
    "Fixnum",   # i64
    "Symbol",   # SymId (Copy)
    "Keyword",  # SymId (Copy)
    "Char",     # char (Copy)
    "Subr",     # SymId (Copy)
}

# Non-ValueKind arm patterns that indicate .kind() was added to a non-Value type.
# These must appear in the PATTERN part of an arm (before `=>`), not in the body.
NON_VALUE_ARM_KEYWORDS = [
    'Some(',
    'None',
    'Ok(',
    'Err(',
    'true',
    'false',
    'Flow::',
    'PrefixArg::',
    'RuntimeBindingValue::',
    'Expr::',
    'Op::',
    'LispType::',
]


def is_non_valuekind_arm(line: str) -> bool:
    """Check if a line contains a non-ValueKind pattern in a match arm.

    Only checks the pattern part (before =>).  Returns True if the line
    has a non-ValueKind enum/literal used as a match arm pattern.
    """
    stripped = line.strip()
    if not stripped:
        return False

    # If the line has =>, split and only check the pattern part
    if '=>' in stripped:
        pattern_part = stripped.split('=>', 1)[0].strip()
    else:
        # Lines without => could be:
        # 1. Multi-line arm patterns (e.g., `Some(x)` on one line, `=> body` on next)
        # 2. Arm body continuation lines (e.g., `return Err(signal(`)
        #
        # Only treat as pattern if line starts with a pattern-like token:
        # - starts with an enum variant (Capital letter or |)
        # - starts with Some, None, Ok, Err, true, false
        # - starts with a string literal
        # - starts with _
        looks_like_pattern = (
            stripped.startswith('|')
            or stripped.startswith('_')
            or re.match(r'^(Some|None|Ok|Err|true|false|Flow|PrefixArg|RuntimeBindingValue)\b', stripped)
            or re.match(r'^"', stripped)
        )
        if not looks_like_pattern:
            return False
        pattern_part = stripped

    # Check if pattern part starts with or contains non-ValueKind keywords
    for kw in NON_VALUE_ARM_KEYWORDS:
        if kw in pattern_part:
            # Make sure it's not inside a string or comment
            idx = pattern_part.find(kw)
            prefix = pattern_part[:idx]
            if '//' not in prefix and prefix.count('"') % 2 == 0:
                return True

    # Check for string literal as arm pattern (e.g. "foreground" => ...)
    if re.match(r'^\s*"[^"]*"\s*(=>|\|)', stripped):
        return True

    return False

# Accessor method names that should NOT have .kind() before them
ACCESSOR_METHODS = [
    "as_fixnum",
    "as_symbol_id",
    "as_keyword_id",
    "as_char",
    "as_float",
    "as_subr_id",
    "as_str",
    "as_symbol_name",
    "is_cons",
    "is_string",
    "is_vector",
    "is_record",
    "is_hash_table",
    "is_lambda",
    "is_macro",
    "is_bytecode",
    "is_buffer",
    "is_window",
    "is_frame",
    "is_marker",
    "is_overlay",
    "is_timer",
    "is_nil",
    "is_t",
    "is_symbol",
    "is_keyword",
    "is_fixnum",
    "is_float",
    "is_char",
    "is_subr",
    "is_symbol_named",
    "is_keyword_named",
    "cons_car",
    "cons_cdr",
    "xfloat",
]


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
# Brace-matching helper
# ---------------------------------------------------------------------------

def find_matching_brace(content: str, open_pos: int) -> int:
    """Find the position of the closing brace matching the open brace at open_pos.
    Returns -1 if not found."""
    assert content[open_pos] == '{', f"Expected '{{' at position {open_pos}, got '{content[open_pos]}'"
    depth = 1
    i = open_pos + 1
    in_string = False
    in_char = False
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
                i += 2  # skip escaped char
                continue
            elif ch == '"':
                in_string = False
        elif in_char:
            if ch == '\\':
                i += 2  # skip escaped char
                continue
            elif ch == '\'':
                in_char = False
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
            elif ch == '\'' and i + 1 < len(content):
                # Distinguish char literals from lifetime annotations
                # Char literal: 'x', '\n', '\x41', '\u{1234}'
                # Lifetime: 'a (followed by identifier chars then non-quote)
                # Simple heuristic: if next is \ or next-next is ', it's a char
                next_ch = content[i + 1]
                if next_ch == '\\':
                    in_char = True
                elif i + 2 < len(content) and content[i + 2] == '\'':
                    in_char = True
                # else it's a lifetime, don't enter char mode
            elif ch == '{':
                depth += 1
            elif ch == '}':
                depth -= 1
                if depth == 0:
                    return i
        prev = ch
        i += 1
    return -1


# ---------------------------------------------------------------------------
# Fix 1: Remove spurious .kind() on non-Value types
# ---------------------------------------------------------------------------

def extract_arm_patterns(body: str) -> list:
    """Extract match arm pattern lines from a match block body.

    Returns only lines that are at the top-level (brace depth 0 AND paren
    depth 0 within the body) and contain arm patterns (identified by having
    `=>` at the top level).

    Returns list of pattern strings (the part before `=>`).
    """
    patterns = []
    brace_depth = 0
    paren_depth = 0
    in_string = False

    for line in body.split('\n'):
        stripped = line.strip()
        if not stripped:
            continue

        # Track depths across the line
        start_brace = brace_depth
        start_paren = paren_depth
        in_line_comment = False
        skip_next = False
        for ci, ch in enumerate(stripped):
            if skip_next:
                skip_next = False
                continue
            if in_line_comment:
                break
            if in_string:
                if ch == '\\':
                    skip_next = True
                    continue
                if ch == '"':
                    in_string = False
                continue
            if ch == '"':
                in_string = True
                continue
            if ch == '/' and ci + 1 < len(stripped) and stripped[ci + 1] == '/':
                in_line_comment = True
                break
            if ch == '{':
                brace_depth += 1
            elif ch == '}':
                brace_depth -= 1
            elif ch == '(':
                paren_depth += 1
            elif ch == ')':
                paren_depth -= 1

        # Only consider lines at top-level (brace=0 and paren=0 at line start)
        if start_brace == 0 and start_paren == 0 and '=>' in stripped:
            # Extract the pattern part (before =>)
            arrow_idx = stripped.index('=>')
            pattern = stripped[:arrow_idx].strip()
            if pattern:
                patterns.append(pattern)

    return patterns


def fix_spurious_kind(content: str) -> tuple:
    """Remove .kind() from match expressions where the arms show it's not a Value.

    Finds `match EXPR.kind() {` blocks and checks if arm patterns contain
    non-ValueKind patterns (Some, None, Ok, Err, Flow::, PrefixArg::, etc).
    If so, removes the `.kind()`.

    Returns (new_content, count_of_fixes).
    """
    count = 0
    result = content
    offset = 0

    # Search for `.kind() {` which marks the end of a match expression.
    kind_brace_re = re.compile(r'\.kind\(\)\s*\{')

    while True:
        m = kind_brace_re.search(result, offset)
        if not m:
            break

        kind_dot_pos = m.start()   # position of the '.' in '.kind()'
        brace_pos = m.end() - 1    # position of '{'

        # Walk backwards to find the 'match' keyword
        line_start = result.rfind('\n', 0, kind_dot_pos) + 1
        match_keyword_re = re.compile(r'\bmatch\b')
        match_pos = -1

        # Search current line
        for mm in match_keyword_re.finditer(result, line_start, kind_dot_pos):
            match_pos = mm.start()

        if match_pos == -1 and line_start > 0:
            # Search previous line
            prev_line_start = result.rfind('\n', 0, line_start - 1) + 1
            for mm in match_keyword_re.finditer(result, prev_line_start, kind_dot_pos):
                match_pos = mm.start()

        if match_pos == -1:
            offset = m.end()
            continue

        # Find matching closing brace
        brace_end = find_matching_brace(result, brace_pos)
        if brace_end == -1:
            offset = m.end()
            continue

        # Skip tuple matches: match (a.kind(), b.kind()) {
        expr_text = result[match_pos:brace_pos].strip()
        if re.search(r'match\s*\(', expr_text):
            offset = brace_end + 1
            continue

        # Extract arm patterns from the match body
        body = result[brace_pos + 1:brace_end]
        arm_patterns = extract_arm_patterns(body)

        # Check if any arm pattern is a non-ValueKind pattern
        has_non_valuekind = False
        for pattern in arm_patterns:
            for kw in NON_VALUE_ARM_KEYWORDS:
                if kw in pattern:
                    has_non_valuekind = True
                    break
            if has_non_valuekind:
                break
            # Check for string literal arm pattern
            if re.match(r'^"[^"]*"(\s*\|)?', pattern):
                has_non_valuekind = True
                break

        if has_non_valuekind:
            # Remove .kind() from the match expression
            result = result[:kind_dot_pos] + result[kind_dot_pos + len('.kind()'):]
            count += 1
            offset = kind_dot_pos
        else:
            offset = brace_end + 1

    return result, count


# ---------------------------------------------------------------------------
# Fix 2: Remove * dereference on owned ValueKind bindings in match arms
# ---------------------------------------------------------------------------

def fix_deref_in_match_arms(content: str) -> tuple:
    """Remove * dereference on bindings from ValueKind pattern matches.

    In `match expr.kind() { ValueKind::Fixnum(n) => ... *n ... }`,
    the `n` is now owned i64, so `*n` should become `n`.

    Returns (new_content, count_of_fixes).
    """
    count = 0
    lines = content.split('\n')
    result_lines = []

    # State tracking
    in_kind_match = False
    match_depth = 0  # brace depth within the match block
    current_bindings = set()  # bindings from ValueKind patterns in current arm
    arm_started = False  # whether we've seen => for current arm

    # Pre-compile patterns
    kind_match_re = re.compile(r'match\s+.*\.kind\(\)\s*\{')
    # Match ValueKind::Variant(binding) where binding can be compound
    # e.g. ValueKind::Fixnum(n), ValueKind::Symbol(id), ValueKind::Char(c)
    valuekind_binding_re = re.compile(
        r'ValueKind::(' + '|'.join(OWNED_VARIANTS) + r')\(\s*(\w+)\s*\)'
    )
    # Also handle Some(ValueKind::Variant(binding)) patterns
    some_valuekind_binding_re = re.compile(
        r'Some\(\s*ValueKind::(' + '|'.join(OWNED_VARIANTS) + r')\(\s*(\w+)\s*\)\s*\)'
    )

    i = 0
    while i < len(lines):
        line = lines[i]
        stripped = line.strip()

        # Track entering a .kind() match block
        if kind_match_re.search(line):
            in_kind_match = True
            match_depth = 0
            current_bindings = set()
            arm_started = False
            # Count braces in this line
            for ch in line:
                if ch == '{':
                    match_depth += 1
                elif ch == '}':
                    match_depth -= 1

            result_lines.append(line)
            i += 1
            continue

        if in_kind_match:
            # Count braces
            in_string = False
            in_line_comment = False
            for ci, ch in enumerate(line):
                if in_line_comment:
                    break
                if in_string:
                    if ch == '\\':
                        continue  # skip next
                    if ch == '"':
                        in_string = False
                    continue
                if ch == '"':
                    in_string = True
                    continue
                if ch == '/' and ci + 1 < len(line) and line[ci + 1] == '/':
                    in_line_comment = True
                    break
                if ch == '{':
                    match_depth += 1
                elif ch == '}':
                    match_depth -= 1

            if match_depth <= 0:
                in_kind_match = False
                current_bindings = set()
                arm_started = False
                result_lines.append(line)
                i += 1
                continue

            # Check if this line starts a new arm (has a pattern with =>)
            # A new arm resets bindings
            if '=>' in stripped:
                # Extract bindings from this arm's pattern
                # Find the pattern part (before =>)
                arrow_pos = line.index('=>')
                pattern_part = line[:arrow_pos]

                # Check for ValueKind bindings in pattern
                new_bindings = set()
                for m in valuekind_binding_re.finditer(pattern_part):
                    new_bindings.add(m.group(2))
                for m in some_valuekind_binding_re.finditer(pattern_part):
                    new_bindings.add(m.group(2))

                # Also check for multi-line arms from previous lines
                # by looking at pattern continuations (lines with | before =>)
                current_bindings = new_bindings
                arm_started = True

                # Fix dereferences in the guard (between `if` and `=>`)
                # The guard appears as: `ValueKind::Fixnum(n) if *n >= 0 =>`
                if current_bindings and ' if ' in pattern_part:
                    guard_start = pattern_part.index(' if ') + 4
                    guard_part = pattern_part[guard_start:]
                    new_guard = guard_part
                    for binding in current_bindings:
                        deref_re = re.compile(r'(?<!\*)\*(' + re.escape(binding) + r')(?!\w)')
                        new_guard_result = deref_re.sub(r'\1', new_guard)
                        if new_guard_result != new_guard:
                            count += len(deref_re.findall(guard_part))
                            new_guard = new_guard_result
                    if new_guard != guard_part:
                        line = line[:line.index(' if ') + 4] + new_guard + line[arrow_pos:]
                        # Recompute arrow_pos since line changed
                        arrow_pos = line.index('=>')

                # Fix dereferences in the arm body (part after =>)
                body_part = line[arrow_pos + 2:]
                if current_bindings and body_part.strip():
                    new_body = body_part
                    for binding in current_bindings:
                        # Replace *binding with binding, but not **binding, and not
                        # *binding_extended (i.e., binding must be followed by non-word char)
                        deref_re = re.compile(r'(?<!\*)\*(' + re.escape(binding) + r')(?!\w)')
                        new_body_result = deref_re.sub(r'\1', new_body)
                        if new_body_result != new_body:
                            count += len(deref_re.findall(body_part))
                            new_body = new_body_result
                    line = line[:arrow_pos + 2] + new_body

            elif arm_started and current_bindings:
                # We're in the body of an arm with bindings - fix dereferences
                for binding in current_bindings:
                    deref_re = re.compile(r'(?<!\*)\*(' + re.escape(binding) + r')(?!\w)')
                    new_line = deref_re.sub(r'\1', line)
                    if new_line != line:
                        # Count actual replacements
                        old_count = len(deref_re.findall(line))
                        count += old_count
                        line = new_line

            # Check for new arm starting on pattern-only line (no =>)
            # These are lines like `ValueKind::Fixnum(n)` followed by `| ValueKind::Char(c)`
            # then eventually `=> { ... }`
            if not arm_started or '=>' not in stripped:
                for m in valuekind_binding_re.finditer(stripped):
                    if '=>' not in stripped:
                        # This is a pattern line without =>, accumulate bindings
                        current_bindings.add(m.group(2))
                for m in some_valuekind_binding_re.finditer(stripped):
                    if '=>' not in stripped:
                        current_bindings.add(m.group(2))

        result_lines.append(line)
        i += 1

    return '\n'.join(result_lines), count


def fix_deref_in_tuple_match_arms(content: str) -> tuple:
    """Fix * dereferences in tuple match arms like:
    match (a.kind(), b.kind()) {
        (ValueKind::Fixnum(x), ValueKind::Fixnum(y)) => *x == *y,
    }

    Returns (new_content, count_of_fixes).
    """
    count = 0
    lines = content.split('\n')
    result_lines = []

    tuple_kind_match_re = re.compile(r'match\s+\(.*\.kind\(\).*\.kind\(\)\)\s*\{')
    valuekind_binding_re = re.compile(
        r'ValueKind::(' + '|'.join(OWNED_VARIANTS) + r')\(\s*(\w+)\s*\)'
    )

    in_tuple_match = False
    match_depth = 0
    current_bindings = set()

    for line in lines:
        stripped = line.strip()

        if tuple_kind_match_re.search(line):
            in_tuple_match = True
            match_depth = 0
            current_bindings = set()
            for ch in line:
                if ch == '{':
                    match_depth += 1
                elif ch == '}':
                    match_depth -= 1
            result_lines.append(line)
            continue

        if in_tuple_match:
            # Count braces
            in_string = False
            in_line_comment = False
            for ci, ch in enumerate(line):
                if in_line_comment:
                    break
                if in_string:
                    if ch == '\\':
                        continue
                    if ch == '"':
                        in_string = False
                    continue
                if ch == '"':
                    in_string = True
                    continue
                if ch == '/' and ci + 1 < len(line) and line[ci + 1] == '/':
                    in_line_comment = True
                    break
                if ch == '{':
                    match_depth += 1
                elif ch == '}':
                    match_depth -= 1

            if match_depth <= 0:
                in_tuple_match = False
                current_bindings = set()
                result_lines.append(line)
                continue

            if '=>' in stripped:
                arrow_pos = line.index('=>')
                pattern_part = line[:arrow_pos]

                new_bindings = set()
                for m in valuekind_binding_re.finditer(pattern_part):
                    new_bindings.add(m.group(2))

                current_bindings = new_bindings

                body_part = line[arrow_pos + 2:]
                if current_bindings and body_part.strip():
                    new_body = body_part
                    for binding in current_bindings:
                        deref_re = re.compile(r'(?<!\*)\*(' + re.escape(binding) + r')(?!\w)')
                        new_body_result = deref_re.sub(r'\1', new_body)
                        if new_body_result != new_body:
                            count += len(deref_re.findall(body_part))
                            new_body = new_body_result
                    line = line[:arrow_pos + 2] + new_body

            elif current_bindings:
                for binding in current_bindings:
                    deref_re = re.compile(r'(?<!\*)\*(' + re.escape(binding) + r')(?!\w)')
                    new_line = deref_re.sub(r'\1', line)
                    if new_line != line:
                        count += len(deref_re.findall(line))
                        line = new_line

        result_lines.append(line)

    return '\n'.join(result_lines), count


# ---------------------------------------------------------------------------
# Fix 3: Remove .kind() before accessor/predicate method calls
# ---------------------------------------------------------------------------

def fix_kind_before_accessor(content: str) -> tuple:
    """Remove .kind() before accessor method calls like .as_fixnum(), .is_cons(), etc.

    Pattern: expr.kind().as_fixnum() -> expr.as_fixnum()
    Pattern: expr.kind().is_cons() -> expr.is_cons()

    Returns (new_content, count_of_fixes).
    """
    count = 0
    result = content

    # Build pattern: .kind().METHOD(
    # Need to handle both .kind().method() and .kind().method(args)
    for method in ACCESSOR_METHODS:
        pattern = re.compile(r'\.kind\(\)\.' + re.escape(method) + r'(?=\s*\()')
        new_result = pattern.sub('.' + method, result)
        if new_result != result:
            count += len(pattern.findall(result))
            result = new_result

    return result, count


# ---------------------------------------------------------------------------
# Fix 3b: Remove *expr.kind() (dereference of kind result)
# ---------------------------------------------------------------------------

def fix_star_kind(content: str) -> tuple:
    """Remove `*expr.kind()` patterns where someone wrote `*target.kind()`.

    Pattern: *EXPR.kind() -> EXPR.kind()
    But only when the * is clearly dereferencing the .kind() result.

    Returns (new_content, count_of_fixes).
    """
    count = 0
    # Pattern: *SOMETHING.kind() where SOMETHING doesn't start with (
    # to avoid matching *(expr).kind()
    # Look for: *IDENT.kind() or *IDENT[n].kind() etc
    pattern = re.compile(r'\*(\w[\w.\[\]]*?)\.kind\(\)')

    def replace_star_kind(m):
        nonlocal count
        count += 1
        return m.group(1) + '.kind()'

    result = pattern.sub(replace_star_kind, content)
    return result, count


# ---------------------------------------------------------------------------
# Fix 4: Fix `if ... .kind()` guard patterns that were incorrectly inserted
# ---------------------------------------------------------------------------

def fix_kind_in_if_guard(content: str) -> tuple:
    """Remove .kind() in match arm guards where the pattern is `v if condition`.

    Pattern like: `v if is_marker(v.kind())` -> `v if is_marker(v)`
    This can happen when `.kind()` was added to the match expression but
    the guard references the bound variable with .kind() too.

    Returns (new_content, count_of_fixes).
    """
    # This is a lower-priority fix. For now, just return unchanged.
    return content, 0


# ---------------------------------------------------------------------------
# Fix 5: Fix Some(ValueKind::...) patterns wrongly produced
# ---------------------------------------------------------------------------

def fix_some_valuekind_in_match(content: str) -> tuple:
    """In match blocks on .kind(), fix arms that say `Some(ValueKind::Fixnum(n))`
    which should just be `ValueKind::Fixnum(n)` (since .kind() returns ValueKind,
    not Option<ValueKind>).

    But also: in non-.kind() match blocks, `Some(ValueKind::Fixnum(n))` indicates
    the match is on an Option<ValueKind> which might be wrong.

    For safety, only fix the case where the match is explicitly on `.kind()` and
    arms use `Some(ValueKind::...)` which is definitely wrong.

    Returns (new_content, count_of_fixes).
    """
    # This is complex and risky. Skip for now.
    return content, 0


# ---------------------------------------------------------------------------
# Fix 6: Handle ValueKind::Fixnum used in vec![] and value position
#         (not pattern position)
# ---------------------------------------------------------------------------

def fix_valuekind_in_value_position(content: str) -> tuple:
    """Replace ValueKind::Fixnum(N) in value positions with Value::fixnum(N).

    Examples:
        vec![..., ValueKind::Fixnum(val)] -> vec![..., Value::fixnum(val)]
        return ValueKind::Nil -> return Value::NIL
        return Ok(Some(ValueKind::Nil)) -> return Ok(Some(Value::NIL))

    This handles cases where ValueKind variants ended up in expression position
    rather than pattern position.

    Returns (new_content, count_of_fixes).
    """
    count = 0
    result = content

    # ValueKind::Nil in value position
    # Pattern: ValueKind::Nil NOT preceded by a pattern-context keyword
    # Detect value-position by looking for contexts like: =, return, vec![, (, ,
    # But it's hard to distinguish pattern from value position with regex alone.
    #
    # Safe approach: only fix specific known-wrong patterns

    # ValueKind::T in value position - should be Value::TRUE or Value::t()
    # Skip these as they need manual review

    return result, count


# ---------------------------------------------------------------------------
# Main processing
# ---------------------------------------------------------------------------

def process_file(filepath: Path, dry_run: bool) -> dict:
    """Process a single .rs file. Returns dict of fix counts."""
    content = filepath.read_text(encoding='utf-8')
    original = content
    stats = {}

    # Fix 3: Remove .kind() before accessor methods (do first, simple regex)
    content, n = fix_kind_before_accessor(content)
    if n:
        stats['kind_before_accessor'] = n

    # Fix 3b: Remove *expr.kind()
    content, n = fix_star_kind(content)
    if n:
        stats['star_kind'] = n

    # Fix 1: Remove spurious .kind() on non-Value types
    content, n = fix_spurious_kind(content)
    if n:
        stats['spurious_kind'] = n

    # Fix 2: Remove * dereference in match arms (single match)
    content, n = fix_deref_in_match_arms(content)
    if n:
        stats['deref_in_arms'] = n

    # Fix 2b: Remove * dereference in tuple match arms
    content, n = fix_deref_in_tuple_match_arms(content)
    if n:
        stats['deref_in_tuple_arms'] = n

    if content != original:
        if not dry_run:
            filepath.write_text(content, encoding='utf-8')

    return stats


def main():
    parser = argparse.ArgumentParser(
        description="Third-pass fixes for NeoVM tagged pointer migration"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview changes without modifying files",
    )
    args = parser.parse_args()

    if not SRC_ROOT.is_dir():
        print(f"Error: {SRC_ROOT} not found. Run from project root.", file=sys.stderr)
        sys.exit(1)

    files = find_rs_files(SRC_ROOT)
    print(f"Processing {len(files)} .rs files (excluding tagged/)...")

    total_stats = {}
    files_changed = 0

    for filepath in files:
        stats = process_file(filepath, args.dry_run)
        if stats:
            files_changed += 1
            rel = filepath.relative_to(SRC_ROOT)
            detail_parts = []
            for key, val in sorted(stats.items()):
                detail_parts.append(f"{key}={val}")
                total_stats[key] = total_stats.get(key, 0) + val
            print(f"  {rel}: {', '.join(detail_parts)}")

    print()
    print("=" * 60)
    if args.dry_run:
        print("DRY RUN (no files modified)")
    print(f"Files changed: {files_changed}")
    print("Fix summary:")
    for key, val in sorted(total_stats.items()):
        print(f"  {key}: {val}")
    total_fixes = sum(total_stats.values())
    print(f"Total fixes: {total_fixes}")


if __name__ == "__main__":
    main()
