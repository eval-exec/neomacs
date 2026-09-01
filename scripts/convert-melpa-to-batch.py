#!/usr/bin/env python3
"""Convert neomacs-melpa-tests parity suites to multi-probe batches (2a).

Transforms each test file that calls assert_*_parity / assert_*_signal_parity
into one (or few) batch tests, and injects assert_*_batch helpers into mod.rs.
"""

from __future__ import annotations

import re
import sys
from collections import defaultdict
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1] / "crates" / "neomacs-melpa-tests" / "src" / "parity_tests"
SKIP_PACKAGES = {
    "a",
    "aa_edit_mode",
    # harness / lifecycle modules are not package workflow corpora
}
SKIP_TOP_LEVEL = {
    "mod.rs",
    "batch_support.rs",
    "harness_contract.rs",
    "frozen_packages.rs",
    "package_lifecycle.rs",
    "package_vc.rs",
    "upstream_package_ert.rs",
}


def find_raw_string(text: str, start: int) -> tuple[str, int] | None:
    """Return (full_raw_literal, end_index) if text[start:] begins a raw string."""
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


def extract_expect_block(text: str, start: int) -> tuple[str, int] | None:
    """Extract expect![[ ... ]] or expect![ ... ] starting at 'expect'."""
    m = re.match(r"expect!\s*(\[\[|\[)", text[start:])
    if not m:
        return None
    opener = m.group(1)
    depth = len(opener)
    i = start + m.end()
    while i < len(text):
        # skip raw strings
        if text[i] == "r" and (raw := find_raw_string(text, i)):
            i = raw[1]
            continue
        # skip normal strings
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


def split_tests(text: str) -> list[tuple[str, str, str]]:
    """Return list of (fn_name, full_fn_source, body_inner)."""
    tests = []
    for m in re.finditer(r"#\[test\]\s*(?:#\[[^\]]+\]\s*)*fn\s+(\w+)\s*\(", text):
        name = m.group(1)
        # find opening brace of function
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
                    tests.append((name, full, body))
                    break
            i += 1
    return tests


def parse_string_literal(text: str, start: int) -> tuple[str, int] | None:
    """Parse a Rust string or raw string at start; return (literal, end)."""
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


def parse_assert_call(body: str) -> tuple[str, str, bool, str] | None:
    """Return (batch_key, probe_src, expect_value, expect_src) or None.

    batch_key is the assert_*_batch name to call. For source-file asserts it is
    ``assert_foo_source_batch::file.el`` so tests load the same source together.
    """
    # 3-arg: assert_foo_source_parity("file.el", form, expect)
    m3 = re.search(
        r"\b(assert_\w+)\s*\(\s*(\"[^\"]+\")\s*,\s*(\w+)\s*,\s*(\w+)\s*\)\s*;",
        body,
    )
    # 2-arg: assert_foo_parity(form, expect) / signal / missing_dependency_signal
    m2 = re.search(
        r"\b(assert_\w+)\s*\(\s*(\w+)\s*,\s*(\w+)\s*\)\s*;",
        body,
    )

    source_lit = None
    if m3 and (not m2 or m3.start() <= m2.start()):
        assert_fn, source_lit, form_var, expect_var = (
            m3.group(1),
            m3.group(2),
            m3.group(3),
            m3.group(4),
        )
    elif m2:
        assert_fn, form_var, expect_var = m2.group(1), m2.group(2), m2.group(3)
    else:
        return None

    if not assert_fn.startswith("assert_"):
        return None

    expect_value = "signal" not in assert_fn

    # find let form_var = ...
    form_pat = re.compile(
        rf"let\s+{re.escape(form_var)}\s*(?::\s*&?str\s*)?=\s*"
    )
    fm = form_pat.search(body)
    if not fm:
        return None
    pos = fm.end()
    while pos < len(body) and body[pos].isspace():
        pos += 1
    strlit = parse_string_literal(body, pos)
    if not strlit:
        return None
    probe_src, _ = strlit

    em = re.search(
        rf"let\s+{re.escape(expect_var)}\s*(?::\s*Expect\s*)?=\s*",
        body,
    )
    if not em:
        return None
    pos = em.end()
    while pos < len(body) and body[pos].isspace():
        pos += 1
    if not body.startswith("expect!", pos):
        return None
    exp = extract_expect_block(body, pos)
    if not exp:
        return None
    expect_src, _ = exp

    batch_fn = batch_name_for_assert(assert_fn)
    if source_lit is not None:
        # embed source file so grouping keeps one load target per batch
        source_file = source_lit.strip('"')
        batch_key = f"{batch_fn}::{source_file}"
    else:
        batch_key = batch_fn
    return batch_key, probe_src, expect_value, expect_src


def batch_name_for_assert(assert_fn: str) -> str:
    """Map assert helper name to batch helper name."""
    name = assert_fn
    if name.endswith("_signal_parity"):
        name = name[: -len("_signal_parity")] + "_parity"
    elif name.endswith("_signal"):
        name = name[: -len("_signal")] + "_parity"
    if name.endswith("_parity"):
        return name[: -len("_parity")] + "_batch"
    if name.endswith("_batch"):
        return name
    return name + "_batch"


def convert_test_file(path: Path, dry_run: bool = False) -> tuple[bool, str]:
    text = path.read_text()
    # Fully batched: only batch asserts, no remaining parity asserts in tests.
    if re.search(r"assert_\w+_batch\s*\(", text) and not re.search(
        r"assert_\w+_(?:source_)?parity\s*\(|assert_\w+_signal(?:_parity)?\s*\(", text
    ):
        return False, "already batched"

    tests = split_tests(text)
    if not tests:
        return False, "no tests"

    # Group cases by batch helper (+ optional source file)
    groups: dict[str, list[tuple[str, str, bool, str]]] = defaultdict(list)
    unconverted = []
    for fn_name, full, body in tests:
        parsed = parse_assert_call(body)
        if not parsed:
            unconverted.append(fn_name)
            continue
        batch_key, probe, expect_value, expect_src = parsed
        groups[batch_key].append((fn_name, probe, expect_value, expect_src))

    if not groups:
        return False, f"no parseable asserts ({len(unconverted)} tests)"

    # Preserve header (use lines, module docs) before first #[test]
    first_test = re.search(r"#\[test\]", text)
    header = text[: first_test.start()].rstrip() + "\n\n" if first_test else ""

    # Rewrite use super::{...} to include batch helpers; keep parity imports if
    # some tests remain unconverted.
    batch_fns = sorted({k.split("::", 1)[0] for k in groups})
    keep_parity = bool(unconverted)
    header = rewrite_super_imports(header, batch_fns, keep_parity=keep_parity)

    # Keep unconverted tests verbatim.
    unconverted_sources = []
    if unconverted:
        for fn_name, full, body in tests:
            if fn_name in unconverted:
                unconverted_sources.append(full)

    parts = [header.rstrip(), ""]
    stem = path.stem
    for batch_key, cases in groups.items():
        if "::" in batch_key:
            batch_fn, source_file = batch_key.split("::", 1)
            call = f'{batch_fn}("{source_file}",'
            suffix = source_file.replace(".el", "").replace("-", "_").replace(".", "_")
            test_fn = f"{stem}_{suffix}_batch"
        else:
            batch_fn = batch_key
            call = f"{batch_fn}("
            if len(groups) == 1:
                test_fn = f"{stem}_public_surface_batch"
            else:
                test_fn = f"{stem}_{batch_fn.removeprefix('assert_')}"

        parts.append("#[test]")
        parts.append(f"fn {test_fn}() {{")
        parts.append(f"    {call}&[")
        for fn_name, probe, expect_value, expect_src in cases:
            parts.append("        (")
            parts.append(f'            "{fn_name}",')
            # Raw-string interiors must keep original bytes (Elisp source).
            probe_lines = probe.splitlines()
            if len(probe_lines) == 1:
                parts.append(f"            {probe},")
            else:
                parts.append(f"            {probe_lines[0]}")
                for ln in probe_lines[1:]:
                    parts.append(ln)
                parts[-1] = parts[-1] + ","
            parts.append(f"            {'true' if expect_value else 'false'},")
            exp_lines = expect_src.splitlines()
            if len(exp_lines) == 1:
                parts.append(f"            {exp_lines[0]},")
            else:
                parts.append(f"            {exp_lines[0]}")
                for ln in exp_lines[1:]:
                    parts.append(ln)
                parts[-1] = parts[-1] + ","
            parts.append("        ),")
        parts.append("    ]);")
        parts.append("}")
        parts.append("")

    for full in unconverted_sources:
        parts.append(full)
        parts.append("")

    new_text = "\n".join(parts)
    if not dry_run:
        path.write_text(new_text)
    msg = f"{len(tests) - len(unconverted)} tests -> {len(groups)} batch(es)"
    if unconverted:
        msg += f"; kept {len(unconverted)} single-probe"
    return True, msg


def rewrite_super_imports(
    header: str, batch_fns: list[str], keep_parity: bool = False
) -> str:
    """Replace assert_*_parity imports with batch helpers."""
    m = re.search(r"use super::\{([^}]+)\};", header)
    if m:
        items = [x.strip() for x in m.group(1).split(",") if x.strip()]
        new_items = []
        for it in items:
            if it.endswith("_signal_parity") and not keep_parity:
                continue
            if (
                it.endswith("_parity")
                and not it.endswith("_batch")
                and not keep_parity
            ):
                batch = batch_name_for_assert(it)
                if batch in batch_fns and batch not in new_items:
                    new_items.append(batch)
            elif it not in new_items:
                new_items.append(it)
        for bf in batch_fns:
            if bf not in new_items:
                new_items.append(bf)
        return (
            header[: m.start()]
            + "use super::{"
            + ", ".join(new_items)
            + "};"
            + header[m.end() :]
        )

    m = re.search(r"use super::(assert_\w+);", header)
    if m:
        old = m.group(1)
        batch = batch_name_for_assert(old)
        if keep_parity:
            return (
                header[: m.start()]
                + f"use super::{{{old}, {batch}}};"
                + header[m.end() :]
            )
        return header[: m.start()] + f"use super::{batch};" + header[m.end() :]
    # no import — add one
    return f"use super::{{{', '.join(batch_fns)}}};\n\n" + header


def inject_mod_batch_helpers(mod_path: Path, needed_batches: set[str], dry_run: bool = False) -> str:
    """Add assert_*_batch helpers derived from existing assert helpers."""
    text = mod_path.read_text()
    if "assert_oracle_batch" not in text:
        if "use expect_test::Expect;" in text:
            text = text.replace(
                "use expect_test::Expect;",
                "use expect_test::Expect;\n\nuse super::batch_support::assert_oracle_batch;",
                1,
            )
        else:
            text = "use super::batch_support::assert_oracle_batch;\n" + text

    added = []
    for batch_fn in sorted(needed_batches):
        if re.search(rf"\bfn {batch_fn}\s*\(", text):
            continue

        # Prefer source-file batch form when a source_parity helper exists.
        source_fn = batch_fn[: -len("_batch")] + "_source_parity"
        if batch_fn.endswith("_source_batch"):
            source_fn = batch_fn[: -len("_batch")] + "_parity"
        # assert_foo_source_batch from assert_foo_source_parity
        if re.search(rf"\bfn {re.escape(source_fn)}\s*\(", text) and "source_file" in text:
            # source-param batch
            sm = re.search(
                rf"fn {re.escape(source_fn)}\s*\([^)]*source_file[^)]*\)\s*\{{(.*?)\n\}}",
                text,
                re.S,
            )
            if sm:
                body = sm.group(1)
                om = re.search(r"((?:\w+_oracle)\s*\(\s*source_file\s*(?:,[^)]*)?\))", body)
                if not om:
                    om = re.search(r"((?:\w+_oracle)\s*\([^;]*?\))", body)
                if om:
                    oracle_call = om.group(1)
                    # replace source_file param usage
                    if "source_file" in oracle_call:
                        helper = f'''/// Multi-probe batch loading one source file (2a).
pub(crate) fn {batch_fn}(source_file: &str, cases: &[(&str, &str, bool, Expect)]) {{
    let name = current_test_name();
    assert_oracle_batch(
        {oracle_call},
        &name,
        "{batch_fn.removeprefix('assert_')}",
        cases,
    );
}}'''
                        text = text.rstrip() + "\n\n" + helper + "\n"
                        added.append(batch_fn)
                        continue

        # Standard: assert_foo_batch from assert_foo_parity / signal
        candidates = [
            batch_fn[: -len("_batch")] + "_parity",
            batch_fn[: -len("_batch")] + "_signal_parity",
            batch_fn[: -len("_batch")] + "_signal",
        ]
        found = False
        for parity_fn in candidates:
            m = re.search(
                rf"(?:pub\(crate\)\s+)?fn {re.escape(parity_fn)}\s*\([^)]*\)\s*\{{(.*?)\n\}}",
                text,
                re.S,
            )
            if not m:
                continue
            body = m.group(1)
            # chained source call
            cm = re.search(
                r"(\w+)\s*\(\s*\"([^\"]+)\"\s*(?:,\s*[^,\)]+)*\s*,\s*\w+\s*,\s*\w+\s*\)",
                body,
            )
            if cm and "source" in cm.group(1):
                source_file = cm.group(2)
                source_helper = cm.group(1)
                sm = re.search(
                    rf"fn {re.escape(source_helper)}\s*\([^)]*source_file[^)]*\).*?(\w+_oracle)\s*\(\s*source_file",
                    text,
                    re.S,
                )
                if sm:
                    oracle_expr = f'{sm.group(1)}("{source_file}")'
                else:
                    # try with_prelude style
                    oracle_expr = None
                    om = re.search(r"((?:\w+_oracle)\s*\([^;]*?\))", body)
                    if om:
                        oracle_expr = om.group(1)
            else:
                om = re.search(r"((?:\w+_oracle)\s*\([^;]*?\))", body)
                oracle_expr = om.group(1) if om else None

            if not oracle_expr:
                continue

            label_m = re.search(r'panic!\s*\(\s*"(.*?)\s+(?:parity|signal)', body)
            label = label_m.group(1) if label_m else parity_fn.removeprefix("assert_")
            helper = f'''/// Multi-probe batch for `{parity_fn}` cases (2a).
#[allow(dead_code)]
pub(crate) fn {batch_fn}(cases: &[(&str, &str, bool, Expect)]) {{
    let name = current_test_name();
    assert_oracle_batch(
        {oracle_expr},
        &name,
        "{label}",
        cases,
    );
}}'''
            text = text.rstrip() + "\n\n" + helper + "\n"
            added.append(batch_fn)
            found = True
            break
        if not found:
            added.append(f"{batch_fn}:MISSING")

    if not dry_run:
        mod_path.write_text(text)
    return ", ".join(added) if added else "no helpers added"


def process_package(pkg: Path, dry_run: bool = False) -> list[str]:
    notes = []
    if pkg.name in SKIP_PACKAGES:
        return [f"skip {pkg.name}"]

    mod = pkg / "mod.rs"
    if not mod.exists():
        return [f"{pkg.name}: no mod.rs"]

    test_files = [
        p
        for p in pkg.rglob("*.rs")
        if p.name != "mod.rs" and "#[test]" in p.read_text(errors="replace")
    ]
    if not test_files:
        return [f"{pkg.name}: no test files"]

    needed_batches: set[str] = set()
    for tf in test_files:
        ok, msg = convert_test_file(tf, dry_run=dry_run)
        notes.append(f"  {tf.relative_to(pkg)}: {msg}")
        if ok and not dry_run:
            # re-read to find batch fns used
            t = tf.read_text()
            for m in re.finditer(r"\b(assert_\w+_batch)\s*\(", t):
                needed_batches.add(m.group(1))
        elif ok and dry_run:
            # estimate
            tests = split_tests(tf.read_text())
            for _, _, body in tests:
                p = parse_assert_call(body)
                if p:
                    needed_batches.add(batch_name_for_assert(p[0]))

    if needed_batches:
        h = inject_mod_batch_helpers(mod, needed_batches, dry_run=dry_run)
        notes.append(f"  mod.rs: {h}")
    return notes


def main() -> int:
    dry = "--dry-run" in sys.argv
    only = [a for a in sys.argv[1:] if not a.startswith("--")]
    packages = sorted(p for p in ROOT.iterdir() if p.is_dir())
    if only:
        packages = [p for p in packages if p.name in only]

    converted = 0
    failed = 0
    for pkg in packages:
        if pkg.name in SKIP_PACKAGES:
            continue
        notes = process_package(pkg, dry_run=dry)
        status = "\n".join(notes)
        if any("tests ->" in n for n in notes):
            converted += 1
            print(f"OK {pkg.name}\n{status}")
        elif any("already" in n for n in notes):
            print(f"SKIP {pkg.name}\n{status}")
        else:
            failed += 1
            print(f"FAIL {pkg.name}\n{status}")

    print(f"\nconverted={converted} failed_or_empty={failed} dry_run={dry}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
