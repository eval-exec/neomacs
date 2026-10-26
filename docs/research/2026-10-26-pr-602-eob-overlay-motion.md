# PR #602: motion at end-of-buffer overlay boundaries

Research for [PR #602](https://github.com/eval-exec/neomacs/pull/602) and
[issue #601](https://github.com/eval-exec/neomacs/issues/601), 2026-10-26.
The reviewed PR head is `bd8921a6744b58b947f79b46a947ebc391b12651`.
GNU was studied first at local revision
`a360712c9d272d950d8d8255ef74570f7e90b7d9`; its checkout remained read-only.
These are source findings and design implications, not a claim that a native
macOS runtime or the full PR verification matrix was executed by this research
task.

## GNU's contract

`vertical-motion` moves through screen lines, with negative arguments moving
backward and zero reseating at the visual line start. The return value records
actual movement and may stop short at an accessible buffer boundary. Window
width, wrapping, horizontal scroll and display conventions affect the answer.
It uses the current buffer, even when the window argument displays another
buffer. A `(COLS . LINES)` argument adds a horizontal target after the vertical
walk; that target is relative to the visual line start and canonical font width.
[Screen Lines](https://www.gnu.org/software/emacs/manual/html_node/elisp/Screen-Lines.html),
[pinned implementation and docstring](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/indent.c#L2203-L2247).

Overlay `before-string` and `after-string` values add displayed content without
adding buffer characters. Several displayed rows may consequently retain one
source insertion boundary. The overlay's visible tail must not be assigned
invented buffer positions to make motion work.
[Overlay Properties](https://www.gnu.org/software/emacs/manual/html_node/elisp/Overlay-Properties.html),
[pinned manual source](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/doc/lispref/display.texi#L2042-L2056).

`ZV` denotes the accessible end, rather than necessarily the backing buffer's
physical end. Narrowing changes the legal motion bounds without renumbering
positions. An interior narrowed region with backing text on both sides is
therefore an important control for any EOB proof.
[Narrowing](https://www.gnu.org/software/emacs/manual/html_node/elisp/Narrowing.html),
[Point and point-max](https://www.gnu.org/software/emacs/manual/html_node/elisp/Point.html),
[GNU buffer bounds](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/buffer.h#L40-L55).

## Why the end-of-buffer case is special

GNU has separate batch and display-iterator engines, selected by
`noninteractive`. The batch branch calls `vmotion`; the interactive branch
performs the display-iterator walk. A plain batch probe is not sufficient
evidence for a defect in the latter.
[Engine selection](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/indent.c#L2280-L2287).

In the positive display-iterator path, GNU treats an origin at `ZV` separately.
After moving the iterator through displayed strings, it corrects the movement
count to zero when point has not advanced. For nonpositive requests it still
performs the backward/reseat operation. The later horizontal-target phase is
separate from this vertical correction and must also be considered for cons
arguments. This supports an EOB-specific correction, not a general claim that
all interior overlay rows are irrelevant to motion.
[Directional and EOB handling](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/indent.c#L2467-L2527),
[subsequent horizontal target](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/indent.c#L2529-L2559).

The lower-level iterator distinguishes overlay strings from replacing display
strings: leaving a display-property string may advance to the next buffer
position, while an overlay does not conceal its anchor. That distinction is
already expressed by NeoMacs's row-end provenance enum and should survive this
fix.
[GNU iterator exit](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L11517-L11533).

## NeoMacs abstraction and review implications

The existing shared motion resolver has the right ownership boundary. Its
`RowMotion` enum distinguishes settled motion from insufficient measurement;
`AccessibleCharRange`, `LispCharPos1`, `DisplayRowEndSource`, and the separate
`MotionEngine` variants keep the relevant coordinate and provenance choices
explicit. The PR changes this resolver rather than falsifying row production
or special-casing the Lisp builtin. This is a design assessment based on the
[motion implementation](../../crates/neovm-core/src/emacs_core/display/xdisp/motion/mod.rs)
and [PR diff](https://github.com/eval-exec/neomacs/pull/602/files).

An overlay-ended source row anchored at the known accessible end can settle a
coverage question even when no later buffer-source row exists. That does not
prove its movement count is zero. In contrast, an overlay anchor at the edge
of a partial query does not prove accessible EOB and must keep the
`NeedsMoreRows` outcome. A viewport edge alone is not a buffer-boundary proof.

The PR's unconditional trailing-anchor collapse and its zero-movement return
for a row ending at EOB are broader than GNU's origin-at-EOB correction.
An origin *before* `ZV` must still account for virtual display rows traversed
while reaching a valid source insertion boundary. Therefore source-position
reachability and physical row distance must remain separate. The source row's
real start and visible sourceless tail should remain intact for redisplay.
[PR implementation](https://github.com/eval-exec/neomacs/pull/602/files),
[GNU origin-sensitive branches](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/indent.c#L2488-L2527).

The review's live GNU terminal probes establish this distinction. The source
buffer `abc` has EOB 4; `abc\ndef` has EOB 8. Each request below is
`(vertical-motion 1)` with an `after-string` overlay anchored at EOB:

| Buffer | Overlay tail | Origin | Returned movement | Final point |
| --- | --- | ---: | ---: | ---: |
| `abc` | `\nX` | 1 | 1 | 4 |
| `abc` | `\nX\nY` | 1 | 2 | 4 |
| `abc` | `\nX\nY` | 4 | 0 | 4 |
| `abc\ndef` | `\nX\nY` | 5 | 2 | 8 |
| `abc\ndef` | `\nX\nY` | 1 | 1 | 5 |

These are runtime observations from `tmp/issue602/spec-gnu-tty-minimal.el` and
`spec-gnu-tty-minimal.txt`, not values inferred from the implementation under
review. The broader terminal matrix also records before-string and cons
requests in `spec-gnu-tty-probes.el` and `spec-gnu-tty-probes.txt`. It demonstrates
that collapsing repeated source anchors does not preserve the full GNU motion
contract. The original EOB-origin case is distinct from the additional
before-EOB cases; these probes alone do not establish that the latter worked
in the pre-PR implementation.

If this classification spreads to additional callers, a small private helper
or enum could centralize the combination of request origin, row-end provenance,
anchor and accessible range. A zero-distance EOB case must require an origin
at the accessible end, rather than merely a row whose end reaches it. Keep the
original physical row indices available when settling a source cursor; deriving
movement from a source-anchor-deduplicated vector loses required information.
There is no need to introduce an independent rendering model or a generalized
boundary framework. Existing enums and exhaustive `match` arms already provide
compile-time checks; the Rust compiler rejects missing variants in an
exhaustive match.
[Rust match and exhaustiveness](https://doc.rust-lang.org/book/ch06-02-match.html#matches-are-exhaustive).

## Verification questions

The reported `(vertical-motion 1)` at EOB must reach `(0 ZV)` through the real
row-query callback. Integer tests should cover before/after strings, repeated
newlines, zero/backward movement, actual final source-line starts, and narrowed
accessible ends. Partial-query controls must continue asking for more rows.

The cons-argument phase deserves a separate probe: for example, compare
`(vertical-motion '(0 . 1))`, `(1 . 1)`, and `(0 . 0)` from EOB with GNU.
An early return that correctly handles an integer request does not by itself
prove the horizontal phase is preserved. This is a verification question, not
an independently reproduced defect in the PR.

Empty buffers, a real final buffer newline, wrapped real source lines,
truncation/hscroll, and multiple overlay strings provide useful adjacent
controls. Soft-wrapped source rows must remain distinguishable from repeated
virtual anchors. These are candidates for bounded follow-ups if source-based
review or runtime probes identify a gap, rather than assumed merge blockers.

Use a live TUI/GUI session for the user-facing result. Binding `noninteractive`
to nil in a cold batch process selects the display-iterator branch, but did not
reproduce the terminal results above in this review. It is therefore not a
sufficient runtime oracle for these cases. The NeoMacs side must install a real
measured-row query, and differential tests should use an actual initialized
terminal or graphical session on the GNU side. State assertions should include
returned movement, final point and unchanged accessible bounds; GUI coverage
should also ensure the virtual overlay tail still renders. Research does not
substitute for the failed-test-first and before/after runtime checks.


## Merge decision and follow-up

PR #602 was merged by rebase as `5f87eff1de2b4dda52f06421c9063af99dcdc5fe`.
Its original EOB-origin crash was reproduced on main in native TUI and GUI,
and the PR fixed it in both X11 and Wayland. The earlier-origin mismatch was
also present as a coverage error before the PR: it was an incomplete fix,
rather than a demonstrated regression of previously working motion.

The follow-up preserves reachable source landings and transports physical
row distance separately. `EobOverlayMotionCoverage` distinguishes absence,
an unfinished stream, and a completely measured stream. A checked
`CompletedEobOverlayMotionExtent` stores the accessible anchor and relative
row distance. Relative distance survives prepared-query row rebasing; it also
counts a final empty row omitted from published snapshots. Completion is
published only after every ordered EOB string drains. Clipping and row-budget
exhaustion cannot manufacture that evidence.

The integer resolver returns zero for an origin already at accessible EOB.
For earlier origins it uses source-row distance plus the certified tail;
an unfinished matching tail expands the canonical query geometrically.
Rows with no buffer provenance retain their original representation.

A failed public-query test also exposed a premature renderer exit at the
right edge with `truncate-lines`: a seven-character source in an 80-pixel
fontless window left a finite overlay stream unfinished even at a 64-row
budget. The renderer now reaches its existing newline/discard/exhaustion
path. The regression checks lengths around the boundary for both overlay
properties. Another query regression requires all ordered strings to finish,
including a second stream that exceeds the initial row budget.

This follow-up is scoped to positive integer `LINES`. GNU applies a separate
horizontal phase for `(COLS . LINES)`, with additional distinctions between
explicit newline and soft-wrap traversal. Those arguments keep their existing
path; complete goal-column parity is not claimed.

## Failed-test-first evidence

Before the original production patch, the copied regression tests failed in
core and layout tests, and actual native TUI/X11 sessions reproduced the
reported coverage error. Before the follow-up production patch, actual TUI,
X11, and Wayland comparisons failed for earlier origins: NeoMacs returned
zero where initialized GNU returned one or two physical rows. The extended
TUI fixture also failed for a 70-row tail. Corrected layout expectations failed
before integration. The separate truncation regression failed before removal
of the premature right-edge exit.

The shared interactive fixture compares live GNU and NeoMacs state across
before/after strings, one/two/trailing-newline tails, a 70-row tail, accessible
EOB and final source-line origins, and narrowing with backing text on both
sides. A batch oracle control checks the distinct noninteractive engine; it
does not stand in for these initialized display comparisons.


## Follow-up verification results

All Rust test runs used `cargo nextest` in the existing Nix development
shell, with temporary artifacts under the repository's `tmp/issue602/`.

| Layer | Result |
| --- | --- |
| Relevant core xdisp/window commands/window tests | 920 passed |
| Full layout suite | 3,290 passed, one pre-existing Nerd Font cell-width failure, six skipped |
| Final focused layout tests | 16 passed, including the subsequent multiple-stream regression |
| Live GNU batch oracle control | One passed, two accessible-bound cases |
| Initialized GNU versus NeoMacs TUI | Two tests passed, 32 state comparisons |
| Initialized GNU versus NeoMacs X11 GUI | Two tests passed, 32 state comparisons |
| Initialized GNU versus NeoMacs Wayland GUI | Two tests passed, 32 state comparisons |

The full layout failure is
`layout_frame_rust_nerd_font_alias_icon_uses_resolved_monospace_cell_width`:
observed icon width six versus expected default cell width seven. The same
failure occurred before this follow-up. The final focused run includes the
last additional multiple-stream test and unused-binding cleanup after the
full run.

GUI checks require clean exits, live GNU state parity, nonempty screenshots,
and actual `X` glyphs with Lisp-string provenance in the frame snapshot. The
final X11 screenshot was also inspected: source text and virtual `X` rows
remain visible. GUI screenshot readback runs through the fixture rather than
accepting an initial empty startup frame as visual evidence.

The native development build, fresh runtime image generation, batch startup
smoke (`42`), formatting, and whitespace checks passed. Native macOS and
Windows were not executed on this Linux host. Cons/goal-column parity remains
outside this follow-up's scope.

The detailed logs remain local in `tmp/issue602/`, including baseline and
pre-follow-up RED runs, `core-followup-full.log`,
`layout-followup-full.log`, `layout-followup-final-focused.log`,
`tui-followup-green.log`, `gui-followup-{x11,wayland}-final.log`, and
`oracle-followup-green.log`.
