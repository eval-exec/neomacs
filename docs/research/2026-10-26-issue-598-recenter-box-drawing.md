# Issue #598: bottom recentering and box-drawing ink

Investigated 2026-10-26 against the reported NeoMacs revision
`38d6ca6bd216d26a803393aac4db779381c6b872`. GNU Emacs was studied first at
local revision `a360712c9d272d950d8d8255ef74570f7e90b7d9`. Its checkout was
read-only, respecting its `AGENTS.md` restriction on generated contributions.
This note records source findings and design inferences. Reproduction and final
test results belong to the implementation investigation; source differences
alone do not establish the reported failure's root cause.

## GNU command and display contracts

Consecutive `C-l` commands cycle the current display line through middle, top,
and bottom, with `scroll-margin` respected. This is a command-history cycle,
not a character-specific behavior. `recenter-top-bottom` selects `middle` on
the first call, compares `this-command` with `last-command` on subsequent calls,
and invokes `recenter` with nil, a nonnegative top margin, or a negative bottom
offset. [GNU manual](https://www.gnu.org/software/emacs/manual/html_mono/emacs.html#Recentering),
[pinned window.el:10855–10897](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/lisp/window.el#L10855-L10897).

Graphical `Frecenter` preserves three distinct motion meanings. Centering walks
backward by half the window's pixel height. Positive arguments walk a requested
number of display rows backward from the beginning of point's display row.
Negative arguments reserve the actual pixel height of point's row and requested
rows below it, then walk backward within the remaining pixel budget. If that
walk goes too far, GNU advances a row to keep point's row fully visible; its
comment explains that partial visibility would otherwise cause redisplay to
recenter it in the middle. End-of-buffer padding and final-row extra spacing
have explicit handling. GNU resets vertical scrolling after choosing the new
start. [Pinned window.c:7313–7454](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/window.c#L7313-L7454).

Character-unit window height is expressed in the frame's default font metrics;
it need not equal the number of actual displayed rows when fonts or sizes vary.
A partially visible bottom row is excluded from the character-unit body height.
Thus a default-font line count is not generally an interchangeable substitute
for a pixel budget. [GNU window-size documentation](https://www.gnu.org/software/emacs/manual/html_node/elisp/Window-Sizes.html).

## Logical row height and visible ink are separate

GNU's glyph row stores logical ascent/height, physical ascent/height, visible
height, and extra line spacing separately. Physical glyph ink can extend above
or below its logical row. [Pinned dispextern.h:944–962](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/dispextern.h#L944-L962).

The display iterator sets logical row height from logical ascent/descent and
tracks physical ink separately. It marks vertical overlap when physical metrics
exceed logical metrics. Rendering has an explicit overlap-repair pass, and
overlap clipping can span adjacent rows while remaining bounded by the window.
There is a specific first-row ascent adjustment, rather than a blanket rule
making every row as tall as its ink. [Line metrics](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L24047-L24102),
[glyph overlap flag](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L32252-L32258),
[clipping](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L2495-L2578),
[overlap repair](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L34865-L34913).

FreeType likewise distinguishes glyph origin/baseline, bearings, advances, ink
bounds, and typographic line spacing. Hinting can alter ink bounds and bearings.
An ink rectangle exceeding a layout cell by a fraction of a pixel is therefore
not sufficient evidence of incorrect row placement. Clipping every glyph to its
advance cell can discard legitimate ink. [FreeType glyph metrics](https://freetype.org/freetype2/docs/glyphs/glyphs-3.html).

## NeoMacs observations to test independently

At the reported revision, `recenter_backward_origin` converts a negative argument
to a top-relative count using character-unit `window_body_height_impl`, then
`builtin_recenter` walks that many screen rows. That loses the bottom-relative
pixel-budget meaning GNU retains. It predicts a position error with sufficiently
tall or varying row heights, but requires an executed public reproduction before
being called this issue's cause. [NeoMacs recenter implementation](../../crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs).

The reported two `│` cells occupy y=822..854 and y=854..886; their ink intersects
at y≈857.3..858.0. The current vertical-overlap classifier requires the
intersection to straddle the shared cell boundary, so it rejects this geometry:
the intersection is several pixels below y=854. This can explain the logged
classification even if the submitted ink matches its atlas bounds. The renderer
already retains the logical cell, submitted ink, and original raster bounds as
separate data. Its logging return count is ignored by the drawing call site, so
there is no direct viewport decision based on that count. [Classifier](../../crates/neomacs-renderer-wgpu/src/renderer/glyphs.rs),
[drawing call site](../../crates/neomacs-renderer-wgpu/src/renderer/layer_text.rs).

## Design inferences

Preserve recenter intent until the display-motion layer can interpret it. An
enum such as center, rows from top, and rows from bottom prevents premature
conversion of every request to a top-relative integer. Pixel extents and row
counts should have distinct types or explicit conversion boundaries. Reuse the
same measured display-row geometry used by redisplay and vertical motion, with
point unchanged and full-row visibility checked after the move.

Keep overlap classification independent from viewport motion. Validate raster
placement and logical ownership before classifying permitted ink overhang; retain
errors for intersecting advance cells, misplaced submitted bitmaps, and unrelated
windows or row roles. Avoid Unicode-specific exceptions, unconditional row
inflation, or clipping all glyph ink to its cell. Existing enum-based expected
versus unexpected overlap can express the result without weakening every check.
Exhaustive Rust matching makes new placement modes require explicit handling.
[Rust exhaustive matches](https://doc.rust-lang.org/book/ch06-02-match.html#matches-are-exhaustive).

Public regression coverage should compare each of the three C-l positions and
unchanged point against GNU, for ASCII, box drawing, varying line height,
wrapping, scroll margin, and nonzero vertical scroll. Geometry diagnostics need
a separate test using the reported rectangles plus nearby invalid placements.
The GUI case must observe the settled post-redisplay row position, not only the
requested `window-start`, because automatic visibility correction can undo an
incorrect initial recenter.

## Controlled reproduction and limits

The implementation investigation reproduced the third-C-l failure with Hack,
face height 210, box-drawing text, and explicit line spacing. On Wayland with
`line-spacing=3`, point stayed on buffer line 100. The bottom command initially
selected window-start line 67, but settled redisplay changed it to line 84,
putting point near the middle. The frame reported a canonical character height
of 33 pixels and body height of 1134 pixels, while actual rows advanced by 36
pixels. The nominal 33-row backward move therefore placed point outside the
body. Evidence: `tmp/issue598/neo-box-hack-wayland-spacing3-native.json`.

An X11 control with `line-spacing=1` kept its requested start but left the point
row extending below the text body: the measured cursor bottom was 1014 pixels,
versus a body bottom of 1012. A GNU control using actual 35-pixel rows kept the
point row fully visible. The public GUI regression also failed its explicit
full-row visibility assertion before the implementation change
(`tmp/issue598/gui-x11-red.log`). These observations establish the controlled
pixel-budget defect: default-font line counts cannot stand in for the height
of the rows recentering must fit.

These controlled failures emitted no `char_overlap` errors, so they do not
establish that the reported overlap diagnostics caused the scrolling failure.
At this stage the reporter's font configuration was unknown; Hack and explicit
line spacing were controlled inputs for the faulty placement path. The later
author-provided reproductions below establish the original failure without
adding line spacing.

GNU itself has a relevant edge case: the unspaced Hack box-drawing control can
also return to the middle. Its glyph matrix starts with a 35-pixel first row,
then uses 34-pixel rows, despite nominal `line-pixel-height` reporting 34. That
first-row physical-ascent correction can leave the otherwise bottom-aligned
point row one pixel short of full visibility. Evidence:
`tmp/issue598/gnu-control-testpaths/box-metrics.json`. Consequently the oracle
must use a case with an independently verified full-visible bottom result,
not assume every unspaced GNU box-drawing setup satisfies that condition.
Both digits and box glyphs resolved to the same Hack font in that control.
Disabling `make-cursor-line-fully-visible` preserved GNU's requested bottom
start and exposed a one-pixel-cropped point row, isolating the visibility
correction from font fallback (`box-relaxed-metrics.json` beside that file).

The focused unit RED uses a 100-pixel body, 10-pixel canonical character height,
and measured 20-pixel display rows. Calling public `recenter -1` at buffer line
50 selected line 41 rather than line 46, preserving point but fitting twice as
many rows as the pixel body can hold. Evidence:
`tmp/issue598/unit-bottom-recenter-red.log`.

## Public compatibility controls

The existing TUI C-l cycle test now shares its navigation and assertions with a
box-drawing variant. Both observe actual terminal display after each keypress,
check the unchanged point line/column, and require the final cursor row to sit
directly above the mode line. A batch oracle matrix covers plain ASCII, box
drawing, invisible rows, and wrapped continuations for center/top/bottom
arguments. Exact expected starts were executed twice with GNU directly and once
through the oracle harness's GNU normalizer; the latter evidence is
`tmp/issue598/gnu-oracle-control-expect.log`.

The batch controls protect the character-unit fallback and display-row
traversal; they do not substitute for native GUI pixel-budget verification.
Both terminal scenarios passed against the unchanged baseline product using
`cargo nextest` (2/2, `tmp/issue598/tui-recenter-baseline-200-lines.log`). The
batch oracle matrix also passed in live GNU verify mode (1/1,
`tmp/issue598/oracle-recenter-baseline.log`). These are compatibility controls,
not the graphical pixel-budget RED test.

The first terminal fixture had only 80 lines with point on line 40; its strict
bottom assertion encountered legitimate beginning-of-buffer clamping in a
48-row body. Expanding it to 200 lines with point on line 100 removes that
fixture limitation while preserving the GNU display and bottom-row assertions.
Final post-fix verification is recorded below.

## Author-provided reproduction

The [author's reply](https://github.com/eval-exec/neomacs/issues/598#issuecomment-6411265072)
provided Fira Code at `:height 160`, Noto Sans Mono CJK SC rules for `han`,
`kana`, `cjk-misc`, `bopomofo`, and `hangul`, and Noto Color Emoji for `emoji`.
Their standalone `-Q` case uses Adwaita Mono. The supplied tree contains Chinese
comments and is repeated more than twenty times.

Both cases were replayed with the supplied text repeated 25 times, point on
line 100, real native Wayland C-l keypresses, and no added line spacing. The
preserved baseline is the exact reported commit `38d6ca6bd2`.

| Configuration | Baseline NeoMacs | First fixed NeoMacs | GNU control |
| --- | --- | --- | --- |
| Fira Code 16 pt and author's fontset rules | Third C-l selects bottom, then redisplay returns point's line to the middle; bottom gap 521 px | Whole point row remains at bottom; gap 24 px | Passes; gap 28 px |
| Adwaita Mono 16 pt, no Lisp fontset rules | Same failure; gap 521 px | Whole point row remains at bottom; gap 24 px | Passes; gap 16 px |

All comparisons preserve buffer point and the middle/top/bottom command cycle.
The Adwaita case isolates the available font catalog to Adwaita/Fira/Noto CJK/
emoji files; allowing unrelated installed Sarasa fonts can select a fallback
with matching metrics and make the baseline pass. This is an explicit font
availability control, not an extra text property or Lisp fontset override.
Font versions and hashes are recorded under `tmp/issue598/font-sha256.txt`.

The original case exposes **27-pixel canonical default-font height versus
32-pixel rendered rows** with Noto CJK fallback. The old primitive converted
`recenter -1` into a number of canonical rows and moved backward that many
actual display rows. Too much text then preceded point for the pixel body;
redisplay's visibility correction returned it to the middle. Measuring the
actual display rows before selecting the start corrects this unit mismatch.

Local evidence is `tmp/issue598/author-fira16-{baseline-wayland,firstfixed-wayland,gnu-x11}.json`
and `author-adwaita16-isolated-{baseline-wayland,firstfixed-wayland,gnu-x11}.json`.
The Fira baseline also emits the reported overlap diagnostics. They remain
after successful recentering, while the isolated Adwaita failure has zero such
logs. The bitmap-overhang diagnostics are therefore independent of this
viewport-placement defect.

## Implementation and regression boundaries

`RecenterPlacement` retains center/top/bottom intent; bottom counts use a
nonzero unsigned type. Graphical center and bottom placement query the same
canonical row producer used by display motion. Top placement and the initial/
batch fallback retain screen-line semantics. Bottom selection corrects a
backward overshoot so the full requested row fits, with GNU's full-height
early-return order and trailing-spacing exclusion.

`DisplayRowSnapshot` now carries extra line spacing explicitly alongside the
row advance. Producers pass the existing typed `DisplayTextRowMetrics`, and
complete row constructors must supply spacing at compile time. This avoids a
second font/spacing calculator in recentering and preserves logical geometry
separately from bitmap ink.

The shared measurement transaction distinguishes a selected scroll target
from a fixed recenter target. It revalidates point, buffer-local policy and
layout identity across callback-bearing queries. Tests reproduce point movement
without a buffer edit, selection of another window/frame, and independent
window versus buffer points. Fixed-target queries preserve those independent
markers rather than treating each frame's selected window as globally selected.

Four focused public unit tests have test-first failure evidence: measured
pixel fitting, a point row whose advance consumes the body, a callback moving
point during the settling query, and callbacks changing the selected target.
Permanent GUI tests also failed on both backends before the fix: X11 clipped
point's row by 28 pixels and Wayland returned it to the middle. Their first
fixed runs pass on both backends with PNG inspection. Final post-refactor
verification is recorded below.

## Final verification

The fix was rebased onto `f1af8f47ef` after main advanced during investigation.
The two row-output conflicts retain upstream automatic-minibuffer row state and
typed row coordinates alongside this change's spacing metadata. The rebased
development executable and matching runtime image pass both author
replays on native Wayland: Fira Code with the supplied fontset and isolated
Adwaita without Lisp fontset rules. Each has `expected_cycle=true`,
`unchanged_point=true`, `expected_bottom=true`, and a 24-pixel bottom gap for a
32-pixel point row. Evidence: `tmp/issue598/author-fira16-rebased-wayland.json`
and `author-adwaita16-isolated-rebased-wayland.json`.

| Check | Final result | Evidence under `tmp/issue598/` |
| --- | --- | --- |
| Core display/window unit suites | 720 passed, including all four new public regressions | `rebased-core-final.log` |
| Complete layout-engine unit suite | 3,264 passed; 1 documented known failure; 6 skipped | `rebased-layout-final.log` |
| Live GNU oracle, new matrix and related scroll/window-state controls | 5 passed | `oracle-recenter-rebased.log` |
| TUI ASCII and box-drawing C-l cycle | 2 passed | `tui-recenter-rebased.log` |
| Native GUI regression on X11 and Wayland | Both passed; PNGs inspected with the point row fully visible | `gui-{x11,wayland}-rebased-green.log` |

All Rust test runs used `cargo nextest`. Generated-runtime writes finished
before the final test runs. An earlier broad core run overlapped Lisp generation
and encountered four stale-bytecode startup failures; all four passed when
rerun after generation. The complete 717-case pre-rebase and 720-case rebased
core runs were clean.

The remaining layout failure,
`layout_frame_rust_nerd_font_alias_icon_uses_resolved_monospace_cell_width`,
matches the existing project gate allowlist dated 2026-10-18
(`neomacs-main/tmp/v10x/layout-known-failures.txt`) and its known icon-cell width
6 versus 7 mismatch. `rebased-layout-failure-classification.json` records the
comparison and zero unexpected failures. Before integrating main, the full
layout run also had the three documented Fido/M-x special-character text-run
panics; all three pass on the new base. A separate baseline rebuild for the
remaining unrelated failure was not performed. No tests were skipped or
relaxed to accept it.

Verification used a development build, not release-PGO. The overlap diagnostics
remain outside this viewport fix. GNU's checkout was used read-only; temporary
fixtures, fonts, isolated state and logs remain under repository `./tmp/`.
