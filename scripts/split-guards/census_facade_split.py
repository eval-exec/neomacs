#!/usr/bin/env python3
"""Rename-aware facade-split census.

Proves that the two r2 facade splits are pure moves:

  runtime/eval/mod.rs      -> runtime/eval/context_api.rs
  tagged/gc.rs             -> tagged/gc/memory_registry.rs

For every function that moved, the signature+body text at the base is
compared with the tip text after undoing the r2 identifier renames
(`tagged_heap_identity`, `publish_finalized_root_batch`,
`tagged_heap_generational_enabled`). It also checks the interpreter
`run_loop` and every `carrier_tls_fingerprint` function (the TLS surface)
the same way.

Usage:
  census_facade_split.py --base <git-sha> [--tip-dir <worktree>]

Writes a TSV to stdout and exits 1 if any body differs or a moved
function is missing.
"""
import argparse
import hashlib
import re
import subprocess
import sys
from pathlib import Path

RENAMES = [
    (re.compile(r"\btagged_heap_identity\b"), "identity"),
    (re.compile(r"\bpublish_finalized_root_batch\b"), "publish"),
    (re.compile(r"\btagged_heap_generational_enabled\b"), "generational_enabled"),
    # The new file sits one module level deeper, so a verbatim move rewrites
    # `super::` to `super::super::` in paths. Collapse the run of `super::`
    # on BOTH sides: the moved body must otherwise be textually identical.
    (re.compile(r"(?:\bsuper::)+"), "super::"),
]

FN_RE = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?(?:default\s+)?(?:const\s+)?"
    r"(?:async\s+)?(?:unsafe\s+)?(?:extern\s+\"[^\"]*\"\s+)?fn\s+([A-Za-z0-9_]+)"
)

SPLITS = [
    (
        "crates/neovm-eval/src/emacs_core/runtime/eval/mod.rs",
        "crates/neovm-eval/src/emacs_core/runtime/eval/context_api.rs",
    ),
    (
        "crates/neovm-value/src/tagged/gc.rs",
        "crates/neovm-value/src/tagged/gc/memory_registry.rs",
    ),
]

RUN_LOOP = "crates/neovm-eval/src/emacs_core/runtime/bytecode/vm.rs"
TLS_EXPR = re.compile(
    r"(?m)^[ \t]*(?:pub(?:\([^)]*\))?\s+)?fn\s+carrier_tls_fingerprint\b"
)


def normalize(text: str) -> str:
    for pattern, replacement in RENAMES:
        text = pattern.sub(replacement, text)
    return text


def git_show(root: Path, sha: str, rel: str) -> str:
    return subprocess.run(
        ["git", "-C", str(root), "show", f"{sha}:{rel}"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout


def extract_fns(text: str):
    """name -> sorted list of signature+body texts (order by hash)."""
    out = {}
    for match in FN_RE.finditer(text):
        name = match.group(1)
        brace = text.find("{", match.end())
        if brace < 0:
            continue
        depth = 0
        end = brace
        in_str = False
        escape = False
        while end < len(text):
            ch = text[end]
            if in_str:
                if escape:
                    escape = False
                elif ch == "\\":
                    escape = True
                elif ch == '"':
                    in_str = False
            elif ch == '"':
                in_str = True
            elif ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                if depth == 0:
                    break
            end += 1
        body = text[match.start() : end + 1]
        out.setdefault(name, []).append(hashlib.sha256(body.encode()).hexdigest())
    for name in out:
        out[name].sort()
    return out


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base", required=True)
    parser.add_argument("--tip-dir", default=".")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    tip = Path(args.tip_dir).resolve()

    failures = []
    rows = []
    for old_rel, new_rel in SPLITS:
        base_text = normalize(git_show(root, args.base, old_rel))
        base_fns = extract_fns(base_text)
        tip_old = normalize((tip / old_rel).read_text())
        tip_old_fns = extract_fns(tip_old)
        tip_new = normalize((tip / new_rel).read_text())
        tip_new_fns = extract_fns(tip_new)
        moved = sorted(set(base_fns) - set(tip_old_fns))
        for name in moved:
            base_hashes = base_fns[name]
            tip_hashes = tip_new_fns.get(name)
            if tip_hashes is None:
                failures.append(f"MISSING {name} in {new_rel}")
                rows.append(f"{name}\t{old_rel}\t{new_rel}\tMISSING")
                continue
            identical = base_hashes == tip_hashes
            rows.append(
                f"{name}\t{old_rel}\t{new_rel}\t"
                f"{'identical' if identical else 'DIFFERENT'}"
            )
            if not identical:
                failures.append(f"DIFFERENT {name} ({old_rel} -> {new_rel})")
        extras = sorted(set(tip_new_fns) - set(moved))
        if extras:
            rows.append(f"(extras in {new_rel})\t-\t-\t{','.join(extras)}")

    # run_loop: the interpreter dispatch loop must be byte-identical after
    # undoing the renames.
    base_loop = extract_fns(normalize(git_show(root, args.base, RUN_LOOP))).get(
        "run_loop"
    )
    tip_loop = extract_fns(normalize((tip / RUN_LOOP).read_text())).get("run_loop")
    identical = base_loop is not None and base_loop == tip_loop
    rows.append(
        f"run_loop\t{RUN_LOOP}\t{RUN_LOOP}\t"
        f"{'identical' if identical else 'DIFFERENT'}"
    )
    if not identical:
        failures.append("DIFFERENT run_loop")

    # TLS: every carrier_tls_fingerprint body across the tree, compared as a
    # multiset (file moves do not matter; the bodies must be identical).
    def body_at(text, start):
        brace = text.find("{", start)
        if brace < 0:
            return None
        depth = 0
        end = brace
        while end < len(text):
            if text[end] == "{":
                depth += 1
            elif text[end] == "}":
                depth -= 1
                if depth == 0:
                    return text[start : end + 1]
            end += 1
        return None

    def tls_from_files(files):
        bodies = []
        for _rel, text in files:
            norm = normalize(text)
            for match in TLS_EXPR.finditer(norm):
                body = body_at(norm, match.start())
                if body is not None:
                    bodies.append(hashlib.sha256(body.encode()).hexdigest())
        return sorted(bodies)

    listed = subprocess.run(
        ["git", "-C", str(root), "grep", "-l", "fn carrier_tls_fingerprint",
         args.base, "--", "crates"],
        check=True,
        capture_output=True,
        text=True,
    ).stdout.splitlines()
    base_files = []
    for line in listed:
        rel = line.split(":", 1)[1]
        base_files.append((rel, git_show(root, args.base, rel)))
    tip_files = []
    for path in sorted((tip / "crates").rglob("*.rs")):
        text = path.read_text(errors="replace")
        if "fn carrier_tls_fingerprint" in text:
            tip_files.append((path.relative_to(tip).as_posix(), text))
    base_tls = tls_from_files(base_files)
    tip_tls = tls_from_files(tip_files)
    identical = base_tls == tip_tls
    rows.append(
        f"carrier_tls_fingerprint x{len(base_tls)}\t(tree)\t(tree)\t"
        f"{'identical' if identical else 'DIFFERENT'}"
    )
    if not identical:
        failures.append(
            f"DIFFERENT carrier_tls_fingerprint set: base={len(base_tls)} tip={len(tip_tls)}"
        )

    print("item\tbase_file\ttip_file\tverdict")
    for row in rows:
        print(row)
    print(f"# moved_bodies={sum(1 for r in rows if r.endswith('identical'))} failures={len(failures)}")
    for failure in failures:
        print(f"# FAIL {failure}", file=sys.stderr)
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
