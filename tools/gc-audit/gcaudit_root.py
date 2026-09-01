#!/usr/bin/env python3
"""Repository root for the gc-audit scripts, resolved and VALIDATED.

Entry 163 shipped these scripts "so the numbers in that entry can be
re-measured rather than believed".  As committed they could not be: each one
computed `ROOT` with two `os.path.dirname` calls from its own `__file__`,
which was correct while they lived in `tools/` and became `<repo>/tools` when
they were moved into `tools/gc-audit/`.  Six of the eight then walked a
directory tree that does not exist and reported a count of ZERO while exiting
0 -- `classify2.py` emitted well-formed JSON containing no sites, and
`reach.py` printed "names reaching a safepoint: 0 (0.0% of defined names)".

So the failure mode this module exists to prevent is not "wrong path"; it is
"an analysis that answers zero and looks like it ran".  `repo_root()` walks up
until it finds a directory that actually holds this workspace and raises
otherwise, and `require_nonzero` gives every script one line with which to
refuse to report an empty measurement.
"""

import os

MARKERS = ('Cargo.toml', 'crates/neovm-core', 'crates/neomacs-melpa-tests')


def repo_root():
    """Nearest ancestor directory holding this workspace. Raises if there is none."""
    here = os.path.dirname(os.path.abspath(__file__))
    cur = here
    while True:
        if all(os.path.exists(os.path.join(cur, m)) for m in MARKERS):
            return cur
        parent = os.path.dirname(cur)
        if parent == cur:
            raise RuntimeError(
                f"gc-audit: no workspace root above {here!r} "
                f"(looked for {', '.join(MARKERS)})"
            )
        cur = parent


ROOT = repo_root()


def require_nonzero(what, n):
    """Refuse to report a zero measurement, which is how a broken path looks."""
    if n <= 0:
        raise RuntimeError(
            f"gc-audit: {what} came out {n} under ROOT={ROOT!r}. "
            "A zero here means the scan found nothing to scan, not that the "
            "seam is empty -- check the paths before believing any number."
        )
    return n
