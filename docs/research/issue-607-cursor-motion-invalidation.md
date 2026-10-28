# Issue 607: cursor motion and scene invalidation

Investigated 2026-10-28. The report is [issue 607](https://github.com/eval-exec/neomacs/issues/607).
The source comparison began at NeoMACS `1eb0cb7911395edb15fc6503005fce82c10437f8`
and GNU Emacs `a360712c9d272d950d8d8255ef74570f7e90b7d9`.

## GNU's contract

GNU's `try_cursor_movement` reuses the current glyph matrix when text and
viewport remain valid and only point moved. It deliberately rejects the shortcut
when point-dependent content requires repainting: relative line numbers,
a distinct current-line number face, overlay arrows, or explicit mode-line/window
invalidation. Scrolling uses a separate reuse path. This is a validity proof,
not a rule that every `forward-line` is cursor-only. See the local reference
checkout's `src/xdisp.c`, introductory redisplay commentary and
`try_cursor_movement`, and the [pinned GNU implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xdisp.c#L19838).

`(redisplay t)` asks GNU to attempt redisplay despite pending input; it does not
require regenerating all frame content. `force-window-update` explicitly marks
windows for more extensive work. The benchmark's forced redisplay therefore
allows the ordinary damage optimizations. See the [GNU Lisp reference](https://www.gnu.org/software/emacs/manual/html_node/elisp/Forcing-Redisplay.html)
and `doc/lispref/display.texi` in the reference checkout.

## CPU cause found in NeoMACS

The existing layout counters show cursor-only row reuse during line navigation.
The downstream renderer nevertheless repaints the root scene at animation
cadence. Two independent invalidation paths caused that amplification:

1. `tick_cursor_animation` and `tick_cursor_size_animation` set the scene's
   content-dirty bit after updating compositor-owned cursor geometry.
2. `declare_frame_demands` included `CursorAnimation` among producers sharing
   `RepaintLayers { layers: all(), damage: FullLayer }`.

Thus changing either path alone leaves the other full-repaint request active.
These are source findings at the investigation base, in
[`frame_compositor/cursor.rs`](../../crates/neomacs-display-runtime/src/render_thread/frame_compositor/cursor.rs)
and [`lifecycle.rs`](../../crates/neomacs-display-runtime/src/render_thread/lifecycle.rs).
The native profile attributed roughly 44% of sampled CPU to glyph rendering;
the profile is useful for attribution, not for clean resource comparisons.

The existing retained-static composition path owns the cursorless scene texture
and filled-box inverse-text cells. Its cursor draw accepts sampled animated
geometry. It provides the correct seam for avoiding unchanged-scene work;
motion pixel equivalence must be checked, including intermediate positions,
size changes, and the destination text. See
[`retained_static.rs`](../../crates/neomacs-display-runtime/src/render_thread/render_pass/retained_static.rs).

## Typed design

Keep editor presentations authoritative. Cursor motion and size interpolation
change presentation-time visual state; they must request cursor composition
without pretending that text changed. Use the existing `Invalidation` enum's
`CompositeOnly { layers: CURSOR_EFFECTS }` variant. Real content damage remains
`RepaintLayers` or `RebuildScene`. The strongest-wins enum merge preserves a
simultaneous content repaint rather than letting cursor demand suppress it.
See [`frame_sched.rs`](../../crates/neomacs-display-runtime/src/render_thread/frame_sched.rs).

Motion and blink need distinct `DemandReason` variants. The coordinator keys
scheduled deadlines and phase anchors by reason, and resubmission replaces the
same reason's schedule. Motion's `MaxRate` demand must survive installation or
retraction of blink's independent `At` deadline. A reason enum also automatically
feeds the derived variant list, diagnostic names, and counter-array width;
parallel strings or numeric reason indices would weaken that guarantee.

Retained composition remains conditional: input scrolling, pane movement,
transitions, pointer paint damage, and active overlays can still require full
rendering. Decorative cursor motion keeps full scene damage in this fix: those
cursors are materialized into the frame and retained cursor cells have fixed geometry.
Extending composition to them needs independent pixel-equivalence tests. The
final animation sample must request a frame even if that sample ends the animation; already-recorded cursor damage carries
that obligation after continuous demand is withdrawn.

## Measurements and memory limits

The local reproduction runs 260 timer-driven `forward-line` moves at 100 ms,
forcing redisplay after each move and checking line 261 at completion. The clean
Linux release baseline consumed approximately 28.04% of one CPU core during
navigation. Its sampled RSS range was 312,104–319,192 KB. Timing is secondary to
structural evidence: root-glyph passes should follow content changes rather than
all cursor-animation frames. Reproduction artifacts are in `tmp/issue607/`.

This Linux/Wayland result does not reproduce the report's macOS physical-footprint
spike. An earlier local run with native profiling reached roughly 315–522 MB,
but instrumentation contaminated that comparison. Do not label it a leak or
claim a memory fix from it.

A plausible pressure mechanism deserves separate measurement: wgpu 30's native
`Queue::write_buffer` copies into newly allocated staging memory, released after
the following submission finishes. Its docs describe `StagingBelt` or explicitly
managed mapped buffers as alternatives. Eliminating unnecessary full glyph
uploads should reduce such traffic, but the connection to the reported macOS
spike remains an inference, not a demonstrated root cause. See the
[wgpu 30 Queue documentation](https://docs.rs/wgpu/30.0.0/wgpu/struct.Queue.html#method.write_buffer).
A staging allocator refactor requires its own allocation profile and bounded
in-flight submission evidence; it is not justified by this CPU fix alone.

## Verification obligations

A failing regression must first prove that a real cursor-motion tick wrongly
requests content damage. Scheduler tests must then show independent motion and
blink cadence, and that actual content damage outranks cursor composition.
Renderer readback must compare full and retained pixels during filled-box
motion and resizing. GNU oracle and real TUI navigation tests protect editor
semantics; a real GUI timer workload checks animation routing and final line.
Native macOS CPU and physical-footprint measurements remain necessary to assess
the complete original report.

## Reproduction commands

Build the matching executable and dump with `cargo xtask fresh-build --release`.
The GUI regression uses the issue's 260 timer-driven moves at 100 ms, verifies
line 261 and the EOF viewport, and checks root-glyph work against scene commits.
On Wayland it uses the existing GL benchmark compositor because the ordinary
Pixman smoke compositor exposes only software-adapter capabilities. Software
compatibility mode still runs navigation assertions but explicitly omits the
animation budget; hardware mode must demonstrate active interpolation.

```sh
cargo nextest run -p neomacs-display-runtime \
  -E 'test(frame_sched) or test(cursor_)'
cargo test -p neomacs-renderer-wgpu --test renderer \
  filled_box_composite_matches_full_render
NEOMACS_GUI_TEST_BACKEND=wayland cargo nextest run -p neomacs-gui-tests \
  -E 'test(timer_line_navigation_reuses_scene_during_cursor_motion)'
```

The Linux regression runs a separate `-Q` process in a private display. Its
renderer counters are the performance gate, not wall-clock thresholds. Native
macOS verification requires an isolated GUI session and the issue's physical-
memory sampling procedure; it was not performed on this Linux host.

Before the fix, the hardware GUI regression reached line 261 but failed the
retained-composition assertion: 319 scene commits, 1,330 root glyph passes,
39 retained-composite frames. The primary-motion unit regression also failed
before implementation with `cursor motion must not repaint the editor scene`.

## Final release verification

`cargo xtask fresh-build --release` completed successfully, including its matching
dump and Lisp byte-compilation. On that release, the hardware GUI regression
passed with the same 319 scene commits: root glyph passes fell from 1,330 to
497 (62.6% fewer), and retained-composite frames rose from 39 to 1,095. The
final Lisp line and displayed EOF viewport were both verified. These counters
are independent of CPU scheduling noise; this does not claim a complete GNU
performance match.

The focused verification passed: 87 runtime cursor/scheduler unit tests, two
GPU readback tests (settled and moving/resizing/spring-trail filled-box cursors),
two live GNU viewport oracle cases, one real TUI line-motion comparison, and
two real GUI tests (timer navigation and startup/readback). Formatting of the
changed Rust files and `git diff --check` also passed. Logs and before/after
artifacts are retained under `tmp/issue607/` and `target/neomacs-gui-tests/`.

## Private-display resource comparison

A final paired run used a private GL Weston compositor (separate socket and
runtime directory, with desktop `DISPLAY` cleared). Both editors were pinned
to CPU 8; external `perf stat` counted user-space instructions and cycles at
100% counter coverage across the whole process lifetime. Both completed line
261. CPU and RSS below are one-second samples from steady navigation after
four seconds. This is one paired resource measurement; the deterministic GUI
work counters above are the regression gate.

| Metric | Original | Fixed |
| --- | ---: | ---: |
| Instructions | 25,237,841,948 | 17,920,723,251 |
| Cycles | 14,025,964,653 | 10,468,457,641 |
| Sampled CPU, one core = 100% | 21.79% | 16.04% |
| RSS range, KiB | 289,348–300,284 | 295,492–303,012 |

Instructions fell 29.0% and cycles fell 25.4%. RSS stayed bounded in both runs;
there is no demonstrated memory reduction, leak diagnosis, or reproduction of
the macOS physical-footprint spike. The retained cursorless scene itself has
a texture cost. Scripts, counter CSVs, samples, executable hashes and logs are
in `tmp/issue607/`; `python3 tmp/issue607/private-bench.py` repeats this private-
display resource comparison. The compositor and both editors are terminated
when the script exits.
