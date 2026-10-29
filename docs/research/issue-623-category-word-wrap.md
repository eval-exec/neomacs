# Category-aware visual word wrapping (#623)

GNU `src/xdisp.c:550–617` defines wrap opportunities from the displayed
character. With `word-wrap-by-category` nil, only space/tab allow a following
break, and a non-space/tab can start the next row. With it non-nil, category
`|` also allows a following break; `<` forbids an end-of-line position and `>`
forbids a beginning-of-line position. GNU swaps these restrictions for reversed
visual rows. Both `move_it_in_display_line_to` and `display_line` save a candidate
only when the preceding character allows a following break and the current
character allows a preceding one (`xdisp.c:10288–10300,26095–26115`).

This is a category-table rule, not a Unicode-range heuristic. Each buffer can
select and modify its own table. See the GNU manuals on
[categories](https://www.gnu.org/software/emacs/manual/html_node/elisp/Categories.html)
and [continuation lines](https://www.gnu.org/software/emacs/manual/html_node/emacs/Continuation-Lines.html).
`characters.el` installs the default CJK `|` membership; `kinsoku.el` installs
the punctuation restrictions.

The existing red regression in PR #626 tests the public `vertical-motion` seam
in a 24-column terminal frame. It returns `OK (24 2 15 15)` instead of GNU's
`OK (24 2 19 15)`. A private X11 GUI probe independently reproduced the symptom:
GNU starts the second row at 52 with category wrapping enabled and 15 with it
disabled; the existing pre-fix Neomacs release starts it at 15 in both cases.
The GUI baseline binary predates this PR; the PR-head unit regression provides
the source-matched red result.

Both Neomacs motion fallback and layout word-wrap checkpoint predicates were
hard-coded to whitespace. The fix shares native wrap opportunities between
these paths. Any category state captured for independent row workers must own
only native data, while retained geometry must also observe changes to the
option, the buffer-selected table and the categories beneath that table.

## Implementation

`WordWrapMode` and `WordWrapDirection` select the shared rule; an opaque
`WordWrapOpportunity` exposes the two independent boundary permissions.
`WordWrapPolicy` freezes only the relevant category bits in sorted, coalesced
native ranges. Its private representation preserves the lookup invariant and
its `Send + Sync` assertion prevents Lisp-owned state from entering workers.
The motion fallback reads the live buffer after callbacks, while live rows and
worker row programs share a captured policy. Canonical freshness and retained
keys compare that policy by value, including direct bool-vector mutations that
do not update a char-table epoch. Overflow decisions carry only the optional
break checkpoint, avoiding an `Arc` clone for each character.

Rows are built in logical order before bidi finalization, so these producers
use the forward rule. GNU's reversed-row rule is represented explicitly for
consumers that iterate in visual order; changing the engine's bidi algorithm is
outside this fix.

## Validation

The real `LayoutEngine` row test also failed before the row-producer change
(continuation position 15), then passed. After a full `cargo xtask fresh-build
--release`, the original private GUI probe reports position **52** with category
wrapping enabled and **15** when disabled in both editors at an 80-column body
width. The GNU reference is the local GNU Emacs 31.1 executable. These positions
are platform/font dependent; agreement at the same measured width is the check.

All test runs use `cargo nextest`:

- 467 core window-command, category and indent tests pass.
- Three focused layout tests pass: real row production, live option/table
  mutation, and worker replay after the Lisp category table changes.
- Two oracle tests pass in `NEOVM_ORACLE_MODE=verify`, comparing the rebuilt
  Neomacs with live GNU. They cover CJK, custom ASCII categories and both
  kinsoku restrictions. The harness honors snapshot, verify, refresh and live
  modes. It runs actual private `-nw` event loops on owned PTYs: GNU `--batch`
  remains inert for word wrapping even when a terminal frame is created.
- The TUI test passes, checking the live `(15 19)` motion result and the visible
  first/second rows containing four/eleven CJK characters. Exact screen parity
  is checked after the temporary split is closed. Full wrapped-screen parity
  still exposes a terminal continuation-marker gap at the right edge; this
  test does not claim that marker behavior is fixed.
- Two GUI tests pass on private displays: live GNU/Neomacs category and kinsoku
  parity, plus startup surface readback. No GUI was opened on the user's display.

The broader layout selection passes 381 of 382 tests. The remaining
`position_prefix_reuse_preserves_wrap_inserted_strings_and_hidden_target_points`
failure reproduces unchanged on #620 at `fff421a99e`: it expects a position-cache
hit although that PR disables position-cache reads. This fix leaves that
unrelated expectation untouched.
