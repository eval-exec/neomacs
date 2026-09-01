#!/usr/bin/env python3
"""
Replace assert_eq! with assert_val_eq! in test files where the comparison
involves Value types.

NeoVM's Value type (TaggedValue) uses bitwise PartialEq (pointer identity for
heap objects).  Tests that compare structurally-equal Values (like
``assert_eq!(result, Value::cons(a, b))``) fail because each ``Value::cons()``
allocates a new heap object with a different pointer.

The ``assert_val_eq!`` macro uses structural comparison (``equal_value()``).

Usage:
    python3 scripts/fix_assert_eq.py            # apply in-place
    python3 scripts/fix_assert_eq.py --dry-run  # preview changes only
"""

import argparse
import re
import sys
from pathlib import Path

# ---------------------------------------------------------------------------
# Configuration
# ---------------------------------------------------------------------------

SRC_ROOT = Path("crates/neovm-core/src")

# ---------------------------------------------------------------------------
# File discovery
# ---------------------------------------------------------------------------

def find_target_files() -> list[Path]:
    """Return the list of files to process, sorted."""
    files: list[Path] = []

    # All *_test.rs files under crates/neovm-core/src/
    for p in SRC_ROOT.rglob("*_test.rs"):
        files.append(p)

    # All */tests.rs files under crates/neovm-core/src/
    for p in SRC_ROOT.rglob("tests.rs"):
        files.append(p)

    # Explicitly listed non-test files that contain test modules
    extras = [
        SRC_ROOT / "buffer" / "buffer.rs",
        SRC_ROOT / "emacs_core" / "alloc.rs",
    ]
    for p in extras:
        if p.exists():
            files.append(p)

    # Deduplicate and sort
    return sorted(set(files))

# ---------------------------------------------------------------------------
# Macro call extraction
# ---------------------------------------------------------------------------

def find_macro_call(text: str, start: int) -> tuple[int, int] | None:
    """Find the full extent of a macro invocation starting at *start*.

    *start* must point to the first character of ``assert_eq!``.
    Returns ``(start, end)`` where ``text[start:end]`` is the full call
    including the closing ``)`` (or ``]`` / ``}``).

    Handles nested parentheses, brackets, braces, and Rust string literals
    (including raw strings ``r#"..."#``).
    """
    # Skip past "assert_eq!"
    i = start
    while i < len(text) and text[i] != '(':
        i += 1
    if i >= len(text):
        return None

    opener = text[i]
    closer = {'(': ')', '[': ']', '{': '}'}[opener]
    depth = 1
    i += 1

    while i < len(text) and depth > 0:
        ch = text[i]

        # --- skip string literals ---
        if ch == '"':
            i += 1
            while i < len(text):
                if text[i] == '\\':
                    i += 2
                    continue
                if text[i] == '"':
                    i += 1
                    break
                i += 1
            continue

        # --- skip raw string literals  r#"..."# / r##"..."## etc. ---
        if ch == 'r' and i + 1 < len(text):
            j = i + 1
            hashes = 0
            while j < len(text) and text[j] == '#':
                hashes += 1
                j += 1
            if j < len(text) and text[j] == '"' and hashes > 0:
                # raw string: skip until closing  "###...
                j += 1  # skip opening "
                closing = '"' + '#' * hashes
                while j < len(text):
                    if text[j:j + len(closing)] == closing:
                        j += len(closing)
                        break
                    j += 1
                i = j
                continue

        # --- skip // line comments ---
        if ch == '/' and i + 1 < len(text) and text[i + 1] == '/':
            while i < len(text) and text[i] != '\n':
                i += 1
            continue

        # --- skip /* block comments --- (not nested)
        if ch == '/' and i + 1 < len(text) and text[i + 1] == '*':
            i += 2
            while i + 1 < len(text):
                if text[i] == '*' and text[i + 1] == '/':
                    i += 2
                    break
                i += 1
            continue

        # --- skip character literals ---
        if ch == "'" and i + 1 < len(text):
            # Could be a char literal like 'a', '\\', '\n', '\'' or a lifetime
            # A char literal: 'X' where X is a single char or escape
            j = i + 1
            if j < len(text) and text[j] == '\\':
                j += 1  # skip backslash
                if j < len(text):
                    j += 1  # skip escaped char
            elif j < len(text):
                j += 1  # skip the char
            if j < len(text) and text[j] == "'":
                i = j + 1
                continue
            # else: it's a lifetime or label, just advance normally

        if ch == '(' or ch == '[' or ch == '{':
            depth += 1
        elif ch == ')' or ch == ']' or ch == '}':
            depth -= 1
            if depth == 0:
                return (start, i + 1)
        i += 1

    return None


def split_macro_args(text: str, body_start: int, body_end: int) -> list[str]:
    """Split the arguments of a macro call at top-level commas.

    *body_start* points to the character after the opening ``(``.
    *body_end* points to the closing ``)``.
    Returns a list of argument strings (untrimmed).
    """
    args = []
    depth = 0
    current_start = body_start
    i = body_start

    while i < body_end:
        ch = text[i]

        # --- skip string literals ---
        if ch == '"':
            i += 1
            while i < body_end:
                if text[i] == '\\':
                    i += 2
                    continue
                if text[i] == '"':
                    i += 1
                    break
                i += 1
            continue

        # --- skip raw string literals ---
        if ch == 'r' and i + 1 < body_end:
            j = i + 1
            hashes = 0
            while j < body_end and text[j] == '#':
                hashes += 1
                j += 1
            if j < body_end and text[j] == '"' and hashes > 0:
                j += 1
                closing = '"' + '#' * hashes
                while j < body_end:
                    if text[j:j + len(closing)] == closing:
                        j += len(closing)
                        break
                    j += 1
                i = j
                continue

        # --- skip // line comments ---
        if ch == '/' and i + 1 < body_end and text[i + 1] == '/':
            while i < body_end and text[i] != '\n':
                i += 1
            continue

        # --- skip /* block comments ---
        if ch == '/' and i + 1 < body_end and text[i + 1] == '*':
            i += 2
            while i + 1 < body_end:
                if text[i] == '*' and text[i + 1] == '/':
                    i += 2
                    break
                i += 1
            continue

        # --- skip character literals ---
        if ch == "'" and i + 1 < body_end:
            j = i + 1
            if j < body_end and text[j] == '\\':
                j += 1
                if j < body_end:
                    j += 1
            elif j < body_end:
                j += 1
            if j < body_end and text[j] == "'":
                i = j + 1
                continue

        if ch in ('(', '[', '{'):
            depth += 1
        elif ch in (')', ']', '}'):
            depth -= 1
        elif ch == ',' and depth == 0:
            args.append(text[current_start:i])
            current_start = i + 1
            i += 1
            continue

        i += 1

    # Last argument
    remainder = text[current_start:body_end]
    if remainder.strip():
        args.append(remainder)

    return args


def has_value_at_top_level(arg: str) -> bool:
    """Return True if ``Value::`` appears anywhere within *arg*,
    NOT inside a string literal. Checks at any depth (including
    inside ``vec![...]``, ``Some(...)``, etc.)."""
    depth = 0
    i = 0
    while i < len(arg):
        ch = arg[i]

        # --- skip string literals ---
        if ch == '"':
            i += 1
            while i < len(arg):
                if arg[i] == '\\':
                    i += 2
                    continue
                if arg[i] == '"':
                    i += 1
                    break
                i += 1
            continue

        # --- skip raw string literals ---
        if ch == 'r' and i + 1 < len(arg):
            j = i + 1
            hashes = 0
            while j < len(arg) and arg[j] == '#':
                hashes += 1
                j += 1
            if j < len(arg) and arg[j] == '"' and hashes > 0:
                j += 1
                closing = '"' + '#' * hashes
                while j < len(arg):
                    if arg[j:j + len(closing)] == closing:
                        j += len(closing)
                        break
                    j += 1
                i = j
                continue

        # --- skip character literals ---
        if ch == "'" and i + 1 < len(arg):
            j = i + 1
            if j < len(arg) and arg[j] == '\\':
                j += 1
                if j < len(arg):
                    j += 1
            elif j < len(arg):
                j += 1
            if j < len(arg) and arg[j] == "'":
                i = j + 1
                continue

        if ch in ('(', '[', '{'):
            depth += 1
            i += 1
            continue
        elif ch in (')', ']', '}'):
            depth -= 1
            i += 1
            continue

        # Check for Value:: at any depth (not inside strings)
        if arg[i:i+7] == 'Value::':
            return True

        i += 1

    return False


# ---------------------------------------------------------------------------
# Main transform
# ---------------------------------------------------------------------------

# Pattern to find assert_eq! (but not assert_val_eq! or assert_ne!)
ASSERT_EQ_RE = re.compile(r'\bassert_eq!\s*\(')


def process_file(path: Path, dry_run: bool) -> int:
    """Process a single file.  Returns number of replacements made."""
    text = path.read_text(encoding='utf-8')
    replacements: list[tuple[int, int, str]] = []  # (start, end, replacement)

    for m in ASSERT_EQ_RE.finditer(text):
        call_start = m.start()

        # Find the full macro call extent
        extent = find_macro_call(text, call_start)
        if extent is None:
            continue
        _, call_end = extent

        call_text = text[call_start:call_end]

        # Find the opening paren position within call_text
        paren_offset = call_text.index('(')
        body_start = call_start + paren_offset + 1
        body_end = call_end - 1  # just before closing paren

        # Split into arguments
        args = split_macro_args(text, body_start, body_end)
        if len(args) < 2:
            continue

        # Check if either of the first two arguments has Value:: at top level
        left = args[0]
        right = args[1]

        if has_value_at_top_level(left) or has_value_at_top_level(right):
            # Replace assert_eq! with assert_val_eq!
            new_call = 'assert_val_eq!' + call_text[paren_offset:]
            replacements.append((call_start, call_end, new_call))

    if not replacements:
        return 0

    # Apply replacements in reverse order to preserve offsets
    new_text = text
    for start, end, replacement in reversed(replacements):
        new_text = new_text[:start] + replacement + new_text[end:]

    if not dry_run:
        path.write_text(new_text, encoding='utf-8')

    return len(replacements)


def main():
    parser = argparse.ArgumentParser(
        description="Replace assert_eq! with assert_val_eq! for Value comparisons"
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="Preview changes without modifying files",
    )
    args = parser.parse_args()

    files = find_target_files()
    if not files:
        print("No target files found. Run from the project root.", file=sys.stderr)
        sys.exit(1)

    total_replacements = 0
    files_changed = 0

    print(f"{'[DRY RUN] ' if args.dry_run else ''}Scanning {len(files)} files...")
    print()

    for path in files:
        count = process_file(path, args.dry_run)
        if count > 0:
            files_changed += 1
            total_replacements += count
            rel = path.relative_to(Path(".")) if path.is_relative_to(Path(".")) else path
            print(f"  {rel}: {count} replacement(s)")

    print()
    print(f"{'[DRY RUN] ' if args.dry_run else ''}Summary:")
    print(f"  Files scanned:  {len(files)}")
    print(f"  Files changed:  {files_changed}")
    print(f"  Replacements:   {total_replacements}")


if __name__ == "__main__":
    main()
