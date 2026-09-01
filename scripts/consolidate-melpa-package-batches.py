#!/usr/bin/env python3
"""Consolidate compatible MELPA parity batches into one Rust test per package.

The input is the normalized constructor form produced by
`convert-melpa-case-constructors.py`: each Rust test builds a
`Vec<ParityBatchCase>` and passes it to one assertion helper. Tests that call
the same helper with the same non-case arguments share one generated package
test. Calls with different source files or setup arguments remain separate.
"""

from __future__ import annotations

import argparse
import hashlib
import re
from dataclasses import dataclass
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1] / "crates" / "neomacs-melpa-tests" / "src" / "parity_tests"
GENERATED_START = "// BEGIN generated package batch tests"
GENERATED_END = "// END generated package batch tests"


def find_raw_string(text: str, start: int) -> int | None:
    if start >= len(text) or text[start] != "r":
        return None
    cursor = start + 1
    hashes = 0
    while cursor < len(text) and text[cursor] == "#":
        hashes += 1
        cursor += 1
    if cursor >= len(text) or text[cursor] != '"':
        return None
    close = '"' + "#" * hashes
    end = text.find(close, cursor + 1)
    return None if end < 0 else end + len(close)


@dataclass
class RustTest:
    name: str
    body: str
    start: int
    end: int


def rust_tests(text: str) -> list[RustTest]:
    tests: list[RustTest] = []
    pattern = re.compile(r"#\[test\]\s*fn\s+(\w+)\s*\(")
    for match in pattern.finditer(text):
        brace = text.find("{", match.end())
        if brace < 0:
            continue
        depth = 0
        cursor = brace
        while cursor < len(text):
            raw_end = find_raw_string(text, cursor)
            if raw_end is not None:
                cursor = raw_end
                continue
            if text[cursor] == '"':
                cursor += 1
                while cursor < len(text):
                    if text[cursor] == "\\":
                        cursor += 2
                    elif text[cursor] == '"':
                        cursor += 1
                        break
                    else:
                        cursor += 1
                continue
            if text[cursor] == "{":
                depth += 1
            elif text[cursor] == "}":
                depth -= 1
                if depth == 0:
                    tests.append(
                        RustTest(
                            name=match.group(1),
                            body=text[brace + 1 : cursor],
                            start=match.start(),
                            end=cursor + 1,
                        )
                    )
                    break
            cursor += 1
    return tests


@dataclass
class Collector:
    module: str | None
    name: str
    call: str

    def invocation(self) -> str:
        prefix = f"{self.module}::" if self.module else ""
        return f"{prefix}{self.name}()"


SIMPLE_BATCH = re.compile(
    r"\s*let\s+cases:\s*Vec<ParityBatchCase>\s*=\s*"
    r"(?P<cases>.*);\s*"
    r"(?P<call>assert_[A-Za-z0-9_]+_batch\([^;]*&cases[^;]*\);)\s*",
    re.DOTALL,
)


def collector_for(test: RustTest, module: str | None) -> tuple[Collector, str] | None:
    match = SIMPLE_BATCH.fullmatch(test.body)
    if match is None:
        return None
    collector_name = f"{test.name}_cases"
    cases = match.group("cases").strip()
    rendered = (
        f"pub(super) fn {collector_name}() -> Vec<ParityBatchCase> {{\n"
        f"    {cases}\n"
        "}"
    )
    return Collector(module, collector_name, match.group("call").strip()), rendered


def generated_test_name(call: str, same_helper_calls: int) -> str:
    helper = re.match(r"(assert_[A-Za-z0-9_]+_batch)\(", call)
    assert helper is not None
    base = helper.group(1).removeprefix("assert_").removesuffix("_batch")
    if same_helper_calls == 1:
        return f"{base}_package_batch"
    digest = hashlib.sha256(call.encode()).hexdigest()[:8]
    return f"{base}_{digest}_package_batch"


def render_package_tests(groups: dict[str, list[Collector]]) -> str:
    helper_counts: dict[str, int] = {}
    for call in groups:
        helper = call.split("(", 1)[0]
        helper_counts[helper] = helper_counts.get(helper, 0) + 1

    blocks = [GENERATED_START]
    for call, collectors in sorted(groups.items()):
        helper = call.split("(", 1)[0]
        test_name = generated_test_name(call, helper_counts[helper])
        invocations = ",\n        ".join(item.invocation() for item in collectors)
        blocks.append(
            "\n#[test]\n"
            f"fn {test_name}() {{\n"
            "    let cases: Vec<ParityBatchCase> = [\n"
            f"        {invocations},\n"
            "    ]\n"
            "    .into_iter()\n"
            "    .flatten()\n"
            "    .collect();\n"
            f"    {call}\n"
            "}"
        )
    blocks.append(f"\n{GENERATED_END}")
    return "\n".join(blocks) + "\n"


def consolidate_package(package_dir: Path, write: bool) -> tuple[int, list[str]]:
    groups: dict[str, list[Collector]] = {}
    replacements: dict[Path, list[tuple[int, int, str]]] = {}
    unmatched: list[str] = []

    mod_path = package_dir / "mod.rs"
    mod_text = mod_path.read_text()
    has_generated_start = GENERATED_START in mod_text
    has_generated_end = GENERATED_END in mod_text
    if has_generated_start != has_generated_end:
        raise RuntimeError(f"incomplete generated package batch block in {mod_path}")
    if has_generated_start:
        return 0, unmatched

    for path in sorted(package_dir.glob("*.rs")):
        text = path.read_text()
        module = None if path.name == "mod.rs" else path.stem
        for test in rust_tests(text):
            converted = collector_for(test, module)
            if converted is None:
                unmatched.append(f"{path.relative_to(ROOT)}::{test.name}")
                continue
            collector, rendered = converted
            groups.setdefault(collector.call, []).append(collector)
            replacements.setdefault(path, []).append((test.start, test.end, rendered))

    if not groups:
        return 0, unmatched

    if write:
        for path, edits in replacements.items():
            text = path.read_text()
            for start, end, rendered in sorted(edits, reverse=True):
                text = text[:start] + rendered + text[end:]
            path.write_text(text)

        mod_path.write_text(mod_text.rstrip() + "\n\n" + render_package_tests(groups))

    return sum(len(items) for items in groups.values()), unmatched


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true")
    args = parser.parse_args()

    converted = 0
    unmatched: list[str] = []
    packages = 0
    for package_dir in sorted(path for path in ROOT.iterdir() if path.is_dir()):
        count, package_unmatched = consolidate_package(package_dir, args.write)
        if count:
            packages += 1
            converted += count
        unmatched.extend(package_unmatched)

    print(f"packages: {packages}")
    print(f"converted tests: {converted}")
    print(f"unmatched tests: {len(unmatched)}")
    for item in unmatched:
        print(f"  {item}")


if __name__ == "__main__":
    main()
