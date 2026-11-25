#!/usr/bin/env python3
"""Enforce the display crate graph (docs/design/display-crate-layout.md).

The display protocol is the vocabulary every display crate names, so it
depends on nothing in the workspace but the serde-only identity leaf. The
session layer knows nothing about displays and depends on no workspace crate.
The display wire codec depends only on the protocol and the leaf. And nothing
on the frontend side of the seam (the display runtime, the renderer, the
session and wire crates) may reach the Lisp machine or the layout engine
through normal dependencies, except the edges listed in KNOWN_EDGES_TO_REMOVE,
which later milestones delete. A listed edge that no longer exists fails the
check, so the allowlist cannot go stale.

Reads cargo metadata JSON on stdin.

Usage:
    cargo metadata --no-deps --format-version 1 \
        | python3 -I scripts/ci/check-crate-graph.py
"""

import json
import sys

PROTOCOL = "neomacs-display-protocol"
IDENTITY = "neovm-display-identity"
SESSION = "neomacs-session"
WIRE = "neomacs-display-wire"
RUNTIME = "neomacs-display-runtime"
RENDERER = "neomacs-renderer-wgpu"
LAYOUT = "neomacs-layout-engine"

# Workspace normal dependencies each crate may have (a subset check).
ALLOWED_WORKSPACE_DEPENDENCIES = {
    PROTOCOL: {IDENTITY},
    SESSION: set(),
    WIRE: {PROTOCOL, IDENTITY},
}

# The frontend side of the seam, and what it must never reach.
FRONTEND_SIDE = {RUNTIME, RENDERER, SESSION, WIRE}
LISP_MACHINE = {"neovm-core", "neovm-eval", LAYOUT}

# Edges that exist today and are removed by the crate cut (MILESTONES M4).
# Every entry must still exist; delete it in the change that removes the edge.
KNOWN_EDGES_TO_REMOVE = {
    (RUNTIME, "neovm-core"),
    (RUNTIME, LAYOUT),
    (RENDERER, LAYOUT),
}


def normal_workspace_graph(metadata):
    """Map each workspace package to the workspace packages it depends on
    through normal (non-dev, non-build) dependencies, on any target,
    optional dependencies included."""
    members = {package["name"] for package in metadata["packages"]}
    graph = {}
    for package in metadata["packages"]:
        edges = set()
        for dependency in package["dependencies"]:
            if dependency.get("kind") is not None:
                continue
            if dependency["name"] in members and dependency.get("path"):
                edges.add(dependency["name"])
        graph[package["name"]] = edges
    return graph


def reachable(graph, start, removed):
    seen = set()
    pending = [start]
    while pending:
        node = pending.pop()
        for target in graph.get(node, ()):
            if (node, target) in removed or target in seen:
                continue
            seen.add(target)
            pending.append(target)
    return seen


def path_to(graph, start, goal, removed):
    """One dependency path from start to goal, for the error message."""
    previous = {start: None}
    pending = [start]
    while pending:
        node = pending.pop(0)
        if node == goal:
            path = []
            while node is not None:
                path.append(node)
                node = previous[node]
            return list(reversed(path))
        for target in sorted(graph.get(node, ())):
            if (node, target) in removed or target in previous:
                continue
            previous[target] = node
            pending.append(target)
    return []


def main() -> int:
    metadata = json.load(sys.stdin)
    graph = normal_workspace_graph(metadata)
    failures = []

    for crate, allowed in sorted(ALLOWED_WORKSPACE_DEPENDENCIES.items()):
        if crate not in graph:
            failures.append(f"{crate} is not a workspace member")
            continue
        extra = graph[crate] - allowed
        if extra:
            failures.append(
                f"{crate} depends on {sorted(extra)}; allowed: {sorted(allowed) or 'none'}"
            )

    for source, target in sorted(KNOWN_EDGES_TO_REMOVE):
        if target not in graph.get(source, set()):
            failures.append(
                f"the allowlisted edge {source} -> {target} no longer exists; "
                "remove it from KNOWN_EDGES_TO_REMOVE"
            )

    for source in sorted(FRONTEND_SIDE):
        if source not in graph:
            failures.append(f"{source} is not a workspace member")
            continue
        reached = reachable(graph, source, KNOWN_EDGES_TO_REMOVE)
        for forbidden in sorted(reached & LISP_MACHINE):
            path = path_to(graph, source, forbidden, KNOWN_EDGES_TO_REMOVE)
            failures.append(
                f"{source} reaches {forbidden}: {' -> '.join(path)}"
            )

    if failures:
        print("error: the display crate graph is violated:", file=sys.stderr)
        for failure in failures:
            print(f"  {failure}", file=sys.stderr)
        print(
            "See docs/design/display-crate-layout.md and scripts/ci/check-crate-graph.py.",
            file=sys.stderr,
        )
        return 1

    print(
        f"ok: checked {len(graph)} crates; "
        f"{len(KNOWN_EDGES_TO_REMOVE)} allowlisted edges still to remove"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
