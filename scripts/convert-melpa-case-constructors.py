#!/usr/bin/env python3
"""Refactor MELPA parity batch tests to named ParityBatchCase constructors + Vec.

Transforms:
  assert_foo_batch(&[
      ("case_id", r##"..."##, true, expect![[...]]),
  ]);

Into:
  fn case_id() -> ParityBatchCase { ParityBatchCase::value(...) }

  #[test]
  fn workflows_public_surface_batch() {
      let cases: Vec<ParityBatchCase> = vec![case_id(), ...];
      assert_foo_batch(&cases);
  }

Also updates mod.rs helpers to take &[ParityBatchCase] and use
assert_oracle_batch_cases.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "neomacs-melpa-tests" / "src" / "parity_tests"
SKIP_PKGS = {"aa_edit_mode"}  # already on case-constructor style


def find_raw_string(text: str, start: int) -> tuple[str, int] | None:
    if start >= len(text) or text[start] != "r":
        return None
    i = start + 1
    hashes = 0
    while i < len(text) and text[i] == "#":
        hashes += 1
        i += 1
    if i >= len(text) or text[i] != '"':
        return None
    i += 1
    close = '"' + ("#" * hashes)
    j = text.find(close, i)
    if j < 0:
        return None
    end = j + len(close)
    return text[start:end], end


def parse_string_literal(text: str, start: int) -> tuple[str, int] | None:
    raw = find_raw_string(text, start)
    if raw:
        return raw
    if start >= len(text) or text[start] != '"':
        return None
    i = start + 1
    while i < len(text):
        if text[i] == "\\":
            i += 2
            continue
        if text[i] == '"':
            return text[start : i + 1], i + 1
        i += 1
    return None


def extract_expect_block(text: str, start: int) -> tuple[str, int] | None:
    m = re.match(r"expect!\s*(\[\[|\[)", text[start:])
    if not m:
        return None
    depth = len(m.group(1))
    i = start + m.end()
    while i < len(text):
        if text[i] == "r" and (raw := find_raw_string(text, i)):
            i = raw[1]
            continue
        if text[i] == '"':
            i += 1
            while i < len(text):
                if text[i] == "\\":
                    i += 2
                    continue
                if text[i] == '"':
                    i += 1
                    break
                i += 1
            continue
        if text[i] == "[":
            depth += 1
            i += 1
            continue
        if text[i] == "]":
            depth -= 1
            i += 1
            if depth == 0:
                return text[start:i], i
            continue
        i += 1
    return None


def skip_ws(text: str, i: int) -> int:
    while i < len(text) and text[i].isspace():
        i += 1
    return i


def parse_case_tuple(text: str, start: int) -> tuple[dict, int] | None:
    """Parse one (id, probe, bool, expect) starting at '('."""
    i = skip_ws(text, start)
    if i >= len(text) or text[i] != "(":
        return None
    i = skip_ws(text, i + 1)

    # id string
    id_lit = parse_string_literal(text, i)
    if not id_lit:
        return None
    id_src, i = id_lit
    case_id = id_src.strip('"')
    i = skip_ws(text, i)
    if i >= len(text) or text[i] != ",":
        return None
    i = skip_ws(text, i + 1)

    # probe
    probe_lit = parse_string_literal(text, i)
    if not probe_lit:
        return None
    probe, i = probe_lit
    i = skip_ws(text, i)
    if i >= len(text) or text[i] != ",":
        return None
    i = skip_ws(text, i + 1)

    # bool
    if text.startswith("true", i):
        expect_value = True
        i += 4
    elif text.startswith("false", i):
        expect_value = False
        i += 5
    else:
        return None
    i = skip_ws(text, i)
    if i >= len(text) or text[i] != ",":
        return None
    i = skip_ws(text, i + 1)

    # expect
    if not text.startswith("expect!", i):
        return None
    exp = extract_expect_block(text, i)
    if not exp:
        return None
    expect_src, i = exp
    i = skip_ws(text, i)
    if i < len(text) and text[i] == ",":
        i += 1
    i = skip_ws(text, i)
    if i >= len(text) or text[i] != ")":
        return None
    i += 1
    i = skip_ws(text, i)
    if i < len(text) and text[i] == ",":
        i += 1
    return {
        "id": case_id,
        "probe": probe,
        "expect_value": expect_value,
        "expect": expect_src,
    }, i


def parse_batch_call(body: str) -> dict | None:
    """Parse assert_*_batch(...) from a test body."""
    m = re.search(r"\b(assert_\w+_batch)\s*\(", body)
    if not m:
        return None
    assert_fn = m.group(1)
    i = m.end()
    i = skip_ws(body, i)

    source = None
    # optional source file string first
    if i < len(body) and body[i] == '"':
        lit = parse_string_literal(body, i)
        if not lit:
            return None
        source, i = lit
        i = skip_ws(body, i)
        if i < len(body) and body[i] == ",":
            i += 1
        i = skip_ws(body, i)

    # expect &[ or just [
    if body.startswith("&[", i):
        i += 2
    elif body.startswith("[", i):
        i += 1
    else:
        return None

    cases = []
    while True:
        i = skip_ws(body, i)
        if i < len(body) and body[i] == "]":
            i += 1
            break
        case = parse_case_tuple(body, i)
        if not case:
            return None
        c, i = case
        cases.append(c)
        i = skip_ws(body, i)

    i = skip_ws(body, i)
    if i < len(body) and body[i] == ")":
        i += 1
    i = skip_ws(body, i)
    if i < len(body) and body[i] == ";":
        i += 1

    return {
        "assert_fn": assert_fn,
        "source": source,  # includes quotes if present
        "cases": cases,
        "call_end": i,
        "call_start": m.start(),
    }


def split_tests(text: str) -> list[tuple[str, str, str, int, int]]:
    """Return (fn_name, full_src, body, start, end)."""
    tests = []
    for m in re.finditer(r"#\[test\]\s*(?:#\[[^\]]+\]\s*)*fn\s+(\w+)\s*\(", text):
        name = m.group(1)
        brace = text.find("{", m.end())
        if brace < 0:
            continue
        depth = 0
        i = brace
        while i < len(text):
            ch = text[i]
            if ch == "r" and find_raw_string(text, i):
                _, end = find_raw_string(text, i)  # type: ignore
                i = end
                continue
            if ch == '"':
                i += 1
                while i < len(text):
                    if text[i] == "\\":
                        i += 2
                        continue
                    if text[i] == '"':
                        i += 1
                        break
                    i += 1
                continue
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    full = text[m.start() : i + 1]
                    body = text[brace + 1 : i]
                    tests.append((name, full, body, m.start(), i + 1))
                    break
            i += 1
    return tests


def sanitize_fn_name(case_id: str, used: set[str]) -> str:
    name = re.sub(r"[^a-zA-Z0-9_]", "_", case_id)
    if name and name[0].isdigit():
        name = "case_" + name
    if not name:
        name = "case"
    base = name
    n = 2
    while name in used:
        name = f"{base}_{n}"
        n += 1
    used.add(name)
    return name


def render_case_fn(fn_name: str, case: dict) -> str:
    probe = case["probe"]
    expect_src = case["expect"]
    # preserve multi-line probe/expect interiors exactly
    probe_lines = probe.splitlines()
    if len(probe_lines) == 1:
        probe_block = f"        {probe},"
    else:
        probe_block = "        " + probe_lines[0] + "\n"
        probe_block += "\n".join(probe_lines[1:])
        probe_block += ","

    exp_lines = expect_src.splitlines()
    if len(exp_lines) == 1:
        exp_block = f"        {exp_lines[0]},"
    else:
        exp_block = "        " + exp_lines[0] + "\n"
        exp_block += "\n".join(exp_lines[1:])
        exp_block += ","

    constructor = "value" if case["expect_value"] else "signal"
    return f'''fn {fn_name}() -> ParityBatchCase {{
    ParityBatchCase::{constructor}(
        "{case["id"]}",
{probe_block}
{exp_block}
    )
}}
'''


def convert_test_file(path: Path, dry_run: bool = False) -> tuple[bool, str]:
    text = path.read_text()
    if "fn " in text and "-> ParityBatchCase" in text:
        return False, "already case-constructors"

    # skip pure single-probe files with no batch
    if not re.search(r"assert_\w+_batch\s*\(", text):
        return False, "no batch asserts"

    tests = split_tests(text)
    if not tests:
        return False, "no tests"

    # Header: everything before first #[test]
    first = re.search(r"#\[test\]", text)
    header = text[: first.start()] if first else text

    # Preserve non-test items after header that aren't tests (const, etc.)
    # For simplicity: keep original header (imports + any consts before first test)
    # and also keep any non-test content between tests that appears before first test only.

    # Collect content that is not a test (module docs, consts, helpers) from original
    # by taking header only - SETUP constants in astyle are before tests.

    used_names: set[str] = set()
    case_fns: list[str] = []
    test_blocks: list[str] = []
    failures = []

    for test_name, full, body, _s, _e in tests:
        parsed = parse_batch_call(body)
        if not parsed:
            # keep as-is if not a tuple batch (or single-probe)
            test_blocks.append(full)
            if "assert_" in body and "_batch" in body:
                failures.append(test_name)
            continue

        fn_names = []
        for case in parsed["cases"]:
            fn = sanitize_fn_name(case["id"], used_names)
            case_fns.append(render_case_fn(fn, case))
            fn_names.append(fn)

        assert_fn = parsed["assert_fn"]
        source = parsed["source"]
        if source:
            call = f"{assert_fn}({source}, &cases)"
        else:
            call = f"{assert_fn}(&cases)"

        case_list = ",\n        ".join(f"{n}()" for n in fn_names)
        test_blocks.append(
            f"""#[test]
fn {test_name}() {{
    let cases: Vec<ParityBatchCase> = vec![
        {case_list},
    ];
    {call};
}}
"""
        )

    if failures and not case_fns:
        return False, f"parse failed: {failures[:5]}"

    # Rewrite imports
    header = rewrite_imports(header)

    parts = [header.rstrip(), "", *case_fns, *test_blocks]
    new_text = "\n".join(parts)
    if not new_text.endswith("\n"):
        new_text += "\n"

    if not dry_run:
        path.write_text(new_text)
    msg = f"{len(case_fns)} case fns, {len(test_blocks)} tests"
    if failures:
        msg += f"; kept unparsed {failures}"
    return True, msg


def rewrite_imports(header: str) -> str:
    """Ensure expect + ParityBatchCase + batch asserts are imported."""
    # use super::{...}
    m = re.search(r"use super::\{([^}]+)\};", header)
    if m:
        items = [x.strip() for x in m.group(1).split(",") if x.strip()]
        if "ParityBatchCase" not in items:
            items.insert(0, "ParityBatchCase")
        header = (
            header[: m.start()]
            + "use super::{"
            + ", ".join(items)
            + "};"
            + header[m.end() :]
        )
    else:
        m2 = re.search(r"use super::(assert_\w+);", header)
        if m2:
            name = m2.group(1)
            header = (
                header[: m2.start()]
                + f"use super::{{ParityBatchCase, {name}}};"
                + header[m2.end() :]
            )
        elif "use super::" not in header:
            # insert after expect import if present
            if "use expect_test::expect;" in header:
                header = header.replace(
                    "use expect_test::expect;",
                    "use expect_test::expect;\n\nuse super::ParityBatchCase;",
                    1,
                )
            else:
                header = "use super::ParityBatchCase;\n" + header

    if "use expect_test::expect;" not in header and "expect_test" not in header:
        header = "use expect_test::expect;\n\n" + header

    return header


def update_mod_rs(mod_path: Path, dry_run: bool = False) -> str:
    text = mod_path.read_text()
    original = text
    notes = []

    # Ensure imports for batch_support
    if "assert_oracle_batch" in text or "assert_oracle_batch_cases" in text or re.search(
        r"fn assert_\w+_batch", text
    ):
        if "assert_oracle_batch_cases" not in text and "assert_oracle_batch(" in text:
            text = text.replace("assert_oracle_batch(", "assert_oracle_batch_cases(")
            notes.append("oracle_batch->cases")

        # import line
        if "use super::batch_support::" in text:
            text = re.sub(
                r"use super::batch_support::\{?([^;}\n]+)\}?;",
                lambda m: "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};"
                if "ParityBatchCase" not in m.group(0)
                or "assert_oracle_batch_cases" not in m.group(0)
                else (
                    "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};"
                    if "assert_oracle_batch_cases" not in m.group(0)
                    else m.group(0)
                ),
                text,
                count=1,
            )
            # normalize if still only assert_oracle_batch
            text = text.replace(
                "use super::batch_support::assert_oracle_batch;",
                "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};",
            )
            text = text.replace(
                "use super::batch_support::{assert_oracle_batch};",
                "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};",
            )
            text = text.replace(
                "use super::batch_support::{assert_oracle_batch_cases};",
                "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};",
            )
            if "ParityBatchCase" not in text.split("mod ")[0]:
                # still missing
                if "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};" not in text:
                    text = re.sub(
                        r"use super::batch_support::[^;]+;",
                        "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};",
                        text,
                        count=1,
                    )
        elif re.search(r"fn assert_\w+_batch", text):
            if "use expect_test::Expect;" in text:
                text = text.replace(
                    "use expect_test::Expect;",
                    "use expect_test::Expect;\n\nuse super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};",
                    1,
                )
            else:
                text = (
                    "use super::batch_support::{ParityBatchCase, assert_oracle_batch_cases};\n"
                    + text
                )

        # re-export ParityBatchCase for workflows (once)
        if text.count("pub(crate) use super::batch_support::ParityBatchCase;") == 0:
            text = re.sub(
                r"(use super::batch_support::\{ParityBatchCase, assert_oracle_batch_cases\};)",
                r"\1\n\n/// Case constructors in child modules use this via `super::ParityBatchCase`.\npub(crate) use super::batch_support::ParityBatchCase;",
                text,
                count=1,
            )

        # Fix signatures
        text2, n = re.subn(
            r"fn (assert_\w+_batch)\(cases: &\[\(&str, &str, bool, Expect\)\]\)",
            r"fn \1(cases: &[ParityBatchCase])",
            text,
        )
        text = text2
        if n:
            notes.append(f"sig cases {n}")

        text2, n = re.subn(
            r"fn (assert_\w+_batch)\(source_file: &str, cases: &\[\(&str, &str, bool, Expect\)\]\)",
            r"fn \1(source_file: &str, cases: &[ParityBatchCase])",
            text,
        )
        text = text2
        if n:
            notes.append(f"sig source {n}")

        # leftover assert_oracle_batch( without _cases
        text = re.sub(
            r"\bassert_oracle_batch\(",
            "assert_oracle_batch_cases(",
            text,
        )
        # fix double _cases_cases
        text = text.replace("assert_oracle_batch_cases_cases(", "assert_oracle_batch_cases(")

    if text != original and not dry_run:
        if not text.endswith("\n"):
            text += "\n"
        mod_path.write_text(text)
    return ", ".join(notes) if notes else ("changed" if text != original else "unchanged")


def process_package(pkg: Path, dry_run: bool = False) -> list[str]:
    notes = []
    if pkg.name in SKIP_PKGS:
        return [f"skip {pkg.name}"]

    mod = pkg / "mod.rs"
    if mod.exists():
        notes.append(f"mod.rs: {update_mod_rs(mod, dry_run=dry_run)}")

    for tf in sorted(pkg.rglob("*.rs")):
        if tf.name == "mod.rs":
            continue
        t = tf.read_text(errors="replace")
        if "assert_" not in t or "_batch" not in t:
            continue
        if "-> ParityBatchCase" in t:
            notes.append(f"{tf.name}: already")
            continue
        ok, msg = convert_test_file(tf, dry_run=dry_run)
        notes.append(f"{tf.name}: {msg}" + ("" if ok else " [skip]"))
    return notes


def main() -> int:
    dry = "--dry-run" in sys.argv
    only = [a for a in sys.argv[1:] if not a.startswith("--")]
    packages = sorted(p for p in ROOT.iterdir() if p.is_dir())
    if only:
        packages = [p for p in packages if p.name in only]

    ok_n = 0
    for pkg in packages:
        notes = process_package(pkg, dry_run=dry)
        status = "\n  ".join(notes)
        if any("case fns" in n for n in notes):
            ok_n += 1
            print(f"OK {pkg.name}\n  {status}")
        elif any("skip" in n for n in notes) and len(notes) == 1:
            print(f"SKIP {pkg.name}")
        else:
            print(f"… {pkg.name}\n  {status}")

    print(f"\npackages_with_case_fns={ok_n} dry_run={dry}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
