#!/usr/bin/env python3
"""Enforce the display crate graph (docs/design/display-crate-layout.md).

The display protocol is the vocabulary every display crate names, so it
depends on nothing in the workspace but the serde-only identity leaf. The
session layer knows nothing about displays and depends on no workspace crate.
The display wire codec and the terminal rasterizer depend only on the protocol
and the leaf. Nothing on the frontend side of the seam (the display runtime,
the renderer, the terminal rasterizer, the session and wire crates) may reach
the Lisp machine or the layout engine, and the walk includes their
dev-dependencies: their tests build without the Lisp machine too. Every rule
counts build-dependencies with normal ones, because Cargo compiles them as
part of the crate. The display host is the evaluator side; its normal and
build dependencies are pinned to its charter, so its edge to the display
runtime stays a dev-dependency for its tests.

Edges listed in KNOWN_EDGES_TO_REMOVE are tolerated until the milestone that
deletes them; a listed edge that no longer exists fails the check, so the
allowlist cannot go stale. The crate cut (MILESTONES M4) emptied it.

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
TTY_RENDER = "neomacs-tty-render"
DISPLAY_HOST = "neomacs-display-host"
LAYOUT = "neomacs-layout-engine"

# Workspace normal and build dependencies each crate may have (a subset check).
ALLOWED_WORKSPACE_DEPENDENCIES = {
    PROTOCOL: {IDENTITY},
    SESSION: set(),
    WIRE: {PROTOCOL, IDENTITY},
    TTY_RENDER: {PROTOCOL, IDENTITY},
    # The display host's charter: the evaluator side of a display link. Its
    # tests reach the display runtime through a dev-dependency only.
    DISPLAY_HOST: {
        "neovm-core",
        LAYOUT,
        PROTOCOL,
        WIRE,
        SESSION,
        TTY_RENDER,
        "neomacs-media-cpu",
    },
}

# The frontend side of the seam, and what it must never reach, tests included.
FRONTEND_SIDE = {RUNTIME, RENDERER, TTY_RENDER, SESSION, WIRE}
LISP_MACHINE = {"neovm-core", "neovm-eval", LAYOUT}

# Edges that exist today and are removed by a later milestone. Every entry
# must still exist; delete it in the change that removes the edge.
KNOWN_EDGES_TO_REMOVE = set()


def workspace_graph(metadata, kinds):
    """Map each workspace package to the workspace packages it depends on
    through dependencies of a kind in KINDS (None for normal, "build", "dev"),
    on any target, optional dependencies included."""
    members = {package["name"] for package in metadata["packages"]}
    graph = {}
    for package in metadata["packages"]:
        edges = set()
        for dependency in package["dependencies"]:
            if dependency.get("kind") not in kinds:
                continue
            if dependency["name"] in members and dependency.get("path"):
                edges.add(dependency["name"])
        graph[package["name"]] = edges
    return graph


def with_dev_edges(graph, dev_graph, start):
    """GRAPH, with START's dev-dependencies added to its normal ones. A dev
    edge belongs to START's test build only; the crates it reaches are walked
    through their normal and build edges, as Cargo builds them."""
    combined = dict(graph)
    combined[start] = graph.get(start, set()) | (dev_graph.get(start, set()) - {start})
    return combined


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
    # What building a crate compiles: its normal and build dependencies.
    graph = workspace_graph(metadata, {None, "build"})
    dev_graph = workspace_graph(metadata, {"dev"})
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
        tests_graph = with_dev_edges(graph, dev_graph, source)
        reached = reachable(tests_graph, source, KNOWN_EDGES_TO_REMOVE)
        for forbidden in sorted(reached & LISP_MACHINE):
            path = path_to(tests_graph, source, forbidden, KNOWN_EDGES_TO_REMOVE)
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
