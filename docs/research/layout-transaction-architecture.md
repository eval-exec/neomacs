# NeoMacs layout architecture: current design and long-term target

Research date: 2026-07-13

Status: architectural analysis and recommendation. This document does not
describe an implemented migration.

## Executive conclusion

NeoMacs already has a good transaction boundary **after** layout: one
presentation identity groups visual primitives, hit-test data, source-position
geometry, cursor geometry, popup anchors, and frame placement; the evaluator
prepares it and the render runtime later activates or retires it.

The missing boundary is **inside** layout. A frame can currently be published
even when buffer rows were positioned from estimated chrome heights but its
tab-line/header-line/mode-line regions were built from different, measured
heights. The result is an atomically activated presentation whose projections
agree with one another while its glyph placement disagrees with them.

The long-term design should add a speculative, converging **layout
transaction** before the existing presentation transaction:

```text
logical Emacs state
    -> immutable layout inputs
    -> speculative frame layout attempt
       -> stable sealed frame layout
       -> or NeedsRelayout(measured intrinsic sizes)
    -> coherent visual/spatial projections
    -> prepared presentation
    -> active presentation
    -> retired presentation
```

The core rule is simple:

> A layout attempt may estimate intrinsic sizes, but it may not be published
> unless every estimate that affects geometry equals the size actually
> measured during that attempt.

This is GNU Emacs's semantic rule expressed as a typed Rust API. It also matches
the geometry-clean-before-paint boundary used by GTK, Flutter, and Blink.

## Scope and evidence

This analysis covers:

- the logical frame/window tree in `neovm-core`;
- the evaluator-to-layout bridge and retained layout in
  `neomacs-layout-engine`;
- glyph matrices, semantic regions, cursor/hit/source geometry, and popup
  anchors;
- the prepared/active/retired presentation lifecycle;
- GNU Emacs at local revision
  `0ee48ac4df205e0d915946b5db00e73a0cd21ae0`;
- primary GTK 4, Flutter, and Chromium/Blink documentation and source.

The current tab-line overlap is a useful tracer bug because it crosses every
important seam.

### Evidence from `/tmp/debug.log`

For presentation output around log line 405,840:

```text
treemacs leaf window 6: bounds=(0,24,144,1172), body y=24
main leaf window 1:    bounds=(144,24,1831,1172)
main body layout:      text_y=41, text_h=1138
main tab-line glyph:   y=24, height=21
main cursor:           y=41, height=17
cursor/body clip:      y=45, height=1133
```

The frame tab-bar occupies `[0, 24)` and the root window starts at y=24, which
is correct. Window 6 has no tab-line, so its body also starts at y=24. Window 1
does have a tab-line:

```text
estimated tab-line used by body = 41 - 24 = 17 px
measured tab-line                = 21 px
semantic body top               = 24 + 21 = 45 px
overlap                          = [41, 45) = 4 px
```

The cursor slot is emitted at y=41 while its clip starts at y=45. That is direct
evidence that one presentation contains two vertical partitions. It also
explains the visual feeling that the tab-line covers the first buffer row.

The tab-line is correctly owned by window 1, not by the frame. The bug is not
that a tab-line is global; it is that the owning leaf never stabilizes its
intrinsic height before its body geometry is accepted.

## Current NeoMacs pipeline

### 1. Logical frame and window tree

`neovm-core/src/window/mod.rs` owns the mutable Emacs model:

- a `Frame` owns a split-tree `root_window` and a separate minibuffer leaf;
- each leaf owns its logical outer `bounds`, buffer, point, window start,
  scroll state, margins, fringes, and display parameters;
- `Frame::sync_window_area_bounds` subtracts frame-owned chrome, redistributes
  the root split tree, synchronizes cell edges, and places the minibuffer;
- menu/tool/compact/tab-bar heights are frame fields and therefore affect the
  frame window area;
- tab-line, header-line, and mode-line are not subnodes in the logical split
  tree. They are internal bands of an individual leaf.

This ownership is fundamentally correct. A frame tab-bar moves the root window
area. A tab-line moves only the body inside its leaf. Sibling leaves may have
different tab-line/header-line/mode-line configurations.

### 2. Evaluator-to-layout bridge

`neomacs-layout-engine/src/neovm_bridge.rs` walks every leaf and produces a
large `WindowParams` value.

The bridge resolves:

- logical window and buffer state;
- frame-absolute outer bounds;
- horizontal bands for scrollbars, fringes, margins, and text;
- face/font/cell metrics;
- whether each chrome row is wanted;
- a face-based estimated height for tab-line, header-line, and mode-line.

The horizontal text rect is partially resolved here, but its y/height initially
remain the leaf's outer y/height. The vertical partition is deferred.

`WindowParams` is currently both a logical input snapshot and a bag of
partially resolved physical geometry. It has many consumers (more than one
hundred references in the layout crate), so no module can easily tell which
fields are source facts, estimates, derived geometry, or cache keys.

### 3. Frame-level retry loop

`LayoutEngine::layout_frame_rust` runs on the evaluator thread. It currently:

1. evaluates menu, tab-bar, then tool-bar semantics once in GNU
   `prepare_menu_bars` order before any display line is filled;
2. realizes default metrics;
3. begins a presentation identity;
4. collects frame/window parameters;
5. snapshots pre-fontification damage and resets mutable output builders;
6. renders frame chrome and classifies retained-window fast paths;
7. processes each leaf sequentially in GNU order: resolve and publish its
   candidate start, run any scroll hook, fontify its visible range, revalidate
   canonical inputs, then produce body and chrome rows;
8. discards and recollects the speculative frame if leaf-local Lisp changed
   layout inputs;
9. checks minibuffer resize;
10. positions the already-evaluated GUI chrome and seals display/spatial output;
11. commits retained matrices and prepares evaluator geometry.

`FrameLayoutCoordinator` now owns this bounded convergence policy. Relayout
requests are typed by cause (frame chrome, leaf chrome, minibuffer allocation,
logical-input invalidation, or window-topology invalidation), and every
accepted request discards the complete speculative frame before recollection.
The remaining boundary is deliberately narrow: the coordinator decides retry
eligibility, while the specialized frame/window machinery still performs the
requested geometry mutation.

Retained matrices and buffer unchanged-region acknowledgements are committed
only after an iteration reaches the explicit accepted boundary.

### 4. Per-window layout

`display_buffer_window_render.rs` creates `WindowChromeRowsPlan`. The plan
realizes faces and computes estimated chrome heights. Those getters are passed
to `BufferWindowGeometryRequest`, which computes:

```text
body_y = outer_y + estimated_tab + estimated_header
body_h = outer_h - estimated_tab - estimated_header - estimated_mode
```

Body rows, window-start/end, source-position mappings, and cursor geometry are
then produced using that body rect.

Only after the body is complete does `WindowChromeRowsRenderRequest::render`
evaluate and shape `tab-line-format`, `header-line-format`, and
`mode-line-format`. This ordering is intentional: mode-line forms such as `%p`
and `%l` depend on the final body/window-start.

The renderer returns `WindowChromeMeasuredHeights`. Top chrome is anchored at
the leaf top and the mode-line is pinned to the bottom. A measured height may
therefore differ from the estimate already used by the body.

The design mistake is not “measure after body.” GNU does that too. The mistake
is treating measured height as metadata instead of as a possible layout
invalidation.

### 5. Output and post-layout geometry reconstruction

Window output is accumulated through `DisplayOutputBuilder`,
`WindowOutputEmitter`, and `FrameOutputOwner` into a `FrameDisplayState`.

At window completion, `WindowDisplaySnapshot` records body mappings, cursor,
points, scalar chrome heights, and placeholder/default regions. Back in
`engine.rs`, `PresentedWindowRegionRequest` reconstructs semantic window
regions using the **measured** chrome heights and mutates the snapshot.

This creates two authorities:

- glyph/body/cursor coordinates came from estimated heights;
- semantic regions and later hit/source geometry came from measured heights.

`WindowMatrixEntry::text_area_clip_rect` adds a third derivation: it scans
actual chrome rows in the glyph matrix to narrow the body clip. That is why the
dump has a cursor at y=41 with a clip beginning at y=45.

### 6. Spatial projection and presentation lifecycle

`PresentationSpatialPlan::compile` consumes snapshots to build window metadata,
hit regions, source-position geometry, and pointer mappings. It seals those
projections into `FrameDisplayState`.

`neovm-core/src/window/geometry.rs` then provides a typed, immutable
`PresentationGeometry`. `FramePresentationState` distinguishes prepared,
active, and retired geometry. Runtime activation switches the evaluator's
active visual geometry only when the matching display presentation activates.

This is one of the strongest current abstractions. It solves the evaluator
thread/render thread timing problem: logical geometry and currently visible
geometry are allowed to differ, and interaction uses a presentation identity.

Its limitation is upstream. Atomic activation prevents projections from being
mixed across presentation identities; it cannot repair projections that were
already inconsistent within one identity.

### 7. Incremental layout

`RetainedWindowKey` declares many logical, font, buffer, face, and outer/text
geometry inputs. Cursor-only, scroll, and edit paths reuse body rows and
rerender chrome. Accepted output is retained only after the frame loop accepts
the iteration.

The missing cache concept is a canonical window-partition signature. A retained
body can be reused while newly shaped chrome requires a different body origin.
The fast path may faithfully preserve coordinates from a now-invalid partition.

Incremental layout should therefore depend on sealed layout geometry, not help
construct semantic geometry. Reuse is valid only when the old and new canonical
partition signatures match, or when a formally defined transform can update
all dependent artifacts together.

## Evaluation of the current abstractions

### What is good

1. **Correct high-level ownership.** Frame chrome and per-leaf chrome are
   conceptually separate; split-window outer bounds belong to the logical
   frame tree.
2. **Evaluator-thread semantic work.** Lisp evaluation and mutable Emacs state
   remain on the evaluator thread instead of leaking into the render runtime.
3. **Unified row shaping.** Body and chrome increasingly share display-row,
   face, property, image, and source-mapping machinery.
4. **Accepted-attempt cache commit.** Retained matrices are not overwritten by
   current frame-level retry attempts.
5. **Strong presentation lifecycle.** Prepared/active/retired identities make
   asynchronous frontend/evaluator interaction coherent.
6. **Typed presentation coordinates.** `PresentationGeometry` makes body,
   frame, and cell coordinate intent more explicit.
7. **Immutable renderer handoff.** `FrameDisplayState` and
   `PresentationSpatialPlan` are moving toward deep builder-to-sealed-output
   modules.

These should be retained. A redesign does not require abandoning the Rust
frontend/two-thread model.

### What is weak

1. **No layout-clean state.** “Output builder finished” currently means
   “objects were produced,” not “all intrinsic sizes and derived geometry
   converged.”
2. **Split geometry authority.** Estimated body geometry, measured chrome,
   matrix-derived clipping, and snapshot-derived semantic regions can disagree.
3. **Shallow chrome abstraction.** `WindowChromeRowsPlan` exposes height getters
   and later returns different heights; the caller must understand and repair
   the module's central invariant.
4. **Broad mixed-phase input.** `WindowParams` mixes logical facts, styles,
   estimates, derived rectangles, and cache inputs.
5. **Output mutation before acceptance.** Per-window layout writes directly
   into frame output and snapshot state before the frame knows whether the
   attempt is stable.
6. **Dummy-then-rewrite snapshots.** Regions begin as defaults and are later
   reconstructed/mutated. This makes invalid intermediate states representable.
7. **Parallel projection code.** Body geometry, semantic regions, and matrix
   clipping independently repeat band arithmetic.
8. **Incomplete validation.** Spatial validation checks that semantic
   projections agree, but not that every body glyph/cursor/media primitive is
   contained by the same body's region, or that chrome primitives belong to
   their bands.
9. **Cache keys do not name the true contract.** Many fields are compared, but
   there is no explicit `WindowPartitionSignature` representing the geometry
   on which retained rows depend.
10. **Ad hoc convergence.** Tab-bar and minibuffer retries are separate booleans
    rather than outcomes of one frame layout protocol.
11. **Migration residue.** Public legacy `LayoutOutput`, `WindowLayout`,
    `LayoutRow`, and `CursorLayout` types in `types.rs` have tests but no
    production consumers. Scalar snapshot chrome fields duplicate regions.
12. **Interface pressure is visible in the code shape.** The layout crate
    allows `clippy::too_many_arguments` globally, `WindowParams` is widely
    threaded through the crate, and several production modules exceed one to
    three thousand lines. File size alone is not the problem; it is evidence
    that important policy is distributed rather than hidden behind a deep
    layout boundary.

## Why this happened

Commit `feeee1e72` correctly changed chrome rows to report the tallest measured
display element and correctly rejected an unconditional pre-measure pass that
would evaluate an expensive mode-line twice on every redisplay.

Its architectural assumption was wrong: it claimed GNU lets the body reserve
the estimate while a differently sized measured row covers the fractional
strip. GNU's source shows that a current/desired height mismatch invalidates
the desired attempt and triggers immediate thorough redisplay before backend
update.

The real choice is therefore not:

- evaluate chrome twice on every frame; or
- publish estimated body geometry with measured chrome.

It is:

- normal case: one evaluation because the retained/estimated height matches;
- mismatch case: reject the speculative attempt, seed the next attempt with
  the measured height, and reevaluate as part of the retry.

The retry is rare after the correct height is retained. It preserves exact
Emacs evaluation ordering and never publishes an internally inconsistent
frame.

The existing mode-line instrumentation should consequently assert one
evaluation per **attempt**, and one evaluation for an ordinary stable frame.
A frame whose first attempt discovers a new intrinsic height legitimately
evaluates again in the immediate retry, as GNU does.

## GNU Emacs semantic reference

GNU's code is stateful C, but its ownership and stabilization rules are sound.

### Per-leaf chrome ownership

Each `struct window` stores effective mode-line, header-line, and tab-line
heights. The window text-body height subtracts that leaf's own tab/header/mode,
horizontal scrollbar, and divider. The display iterator begins after the
current tab-line plus header-line height.

Sources:

- [`window.h` window fields](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/window.h#L374-L381)
- [`window.h` body-band macros](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/window.h#L1018-L1064)
- [`xdisp.c` iterator body origin](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L3520-L3523)

The frame tab-bar is a separate frame-owned special window and is explicitly
not an ordinary window's tab-line.

### Current, desired, compare, retry

GNU obtains current chrome height from retained/current matrices and falls back
to a face-based estimate. It shapes desired chrome after body layout and reads
the actual desired glyph-row height. It then compares every desired
tab/header/mode height with the current height.

On mismatch GNU seeds the new height, marks redisplay for a thorough retry, and
does not call the display backend with the inconsistent desired matrix. Frame
tab-bar resizing follows the same stabilization principle at frame scope.

Sources:

- [`dispextern.h` current/desired heights](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/dispextern.h#L1559-L1660)
- [`xdisp.c` face estimate](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L2288-L2314)
- [`xdisp.c` per-window height comparison and retry](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L21426-L21463)
- [`xdisp.c` chrome shaping](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L28023-L28303)
- [`xdisp.c` outer redisplay/backend update](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L17903-L17983)
- [`xdisp.c` frame tab-bar retry](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/xdisp.c#L14930-L15062)

NeoMacs should reproduce these semantics, not GNU's global flags and `goto`
control flow.

## Primary-source comparison with modern GUI systems

### GTK 4

GTK orders Events, Update, Layout, and Paint. Parents measure visible children,
allocate their geometry, and only then snapshot immutable GSK render nodes.
Widget-local coordinate systems are explicit. Geometry changes request layout;
paint-only changes need not.

Sources:

- [GTK drawing model](https://docs.gtk.org/gtk4/drawing-model.html)
- [`Gtk.Widget.measure`](https://docs.gtk.org/gtk4/vfunc.Widget.measure.html)
- [GTK coordinate systems](https://docs.gtk.org/gtk4/coordinates.html)
- [`Gsk.RenderNode`](https://docs.gtk.org/gsk4/class.RenderNode.html)

Lesson: an Emacs leaf is a specialized container. Its intrinsic chrome rows
must be measured and allocated before immutable render recording.

### Flutter

Flutter sends constraints down and sizes up. A parent declares when it depends
on a child's size, and that dependency propagates layout invalidation.
`flushLayout` reaches layout-clean before paint. Hit testing requires current
layout and uses the same child transforms.

Sources:

- [Flutter architectural overview](https://docs.flutter.dev/resources/architectural-overview)
- [`PipelineOwner.flushLayout`](https://api.flutter.dev/flutter/rendering/PipelineOwner/flushLayout.html)
- [`RenderObject.markNeedsLayout`](https://api.flutter.dev/flutter/rendering/RenderObject/markNeedsLayout.html)
- [`RenderBox.hitTest`](https://api.flutter.dev/flutter/rendering/RenderBox/hitTest.html)

Lesson: a measured tab-line height change invalidates its parent window
partition, not only tab-line paint.

### Chromium/Blink

Blink has explicit dirty/clean lifecycle states. Layout algorithms take
declared inputs such as a `ConstraintSpace` and produce physical fragments.
Those fragments carry child offsets and feed both paint and hit testing.
`LayoutResult` includes typed outcomes that require relayout; only a clean
lifecycle proceeds to paint/compositor publication.

Sources:

- [Blink layout lifecycle](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/third_party/blink/renderer/core/layout/README.md)
- [LayoutNG inputs and cache model](https://chromium.googlesource.com/chromium/src/third_party/+/refs/heads/main/blink/renderer/core/layout/layout_ng.md)
- [`PhysicalFragment`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/third_party/blink/renderer/core/layout/physical_fragment.h#51)
- [`LayoutResult`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/third_party/blink/renderer/core/layout/layout_result.h)
- [Blink paint pipeline](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/third_party/blink/renderer/core/paint/README.md)

Lesson: `Stable(fragment)` versus `NeedsRelayout(reason)` is an ordinary layout
interface. Paint and hit testing should consume one physical geometry output,
not separately reconstruct offsets.

### Common principle

The implementations differ, but all enforce:

```text
intrinsic measurement
    -> parent geometry invalidation when needed
    -> stable allocated/physical layout
    -> paint + hit test + publication
```

No reference lets paint discover a layout-affecting size, update only semantic
clips, and publish body positions calculated from the old size.

## Design alternatives

### A. Minimal GNU-style retry in the current engine

Add a per-window measured-versus-assumed comparison. If any leaf differs,
discard frame output, retain the newly measured heights, and continue the
existing outer loop.

Advantages:

- smallest path to fixing the overlap;
- exact GNU visible semantics;
- provides a testable tracer bullet for convergence.

Disadvantages:

- retains duplicate geometry derivations;
- keeps `WindowParams`, snapshots, and output builders phase-ambiguous;
- future intrinsic elements can repeat the same bug;
- ad hoc retries grow more complicated.

Verdict: correct first migration step, insufficient end state.

### B. Layout transaction plus sealed frame layout

Create a deep module that owns estimation, measurement, allocation,
stabilization, coordinate transforms, and spatial invariants. It returns either
a sealed layout or a typed relayout request. All visual and interaction
projections derive from the sealed layout.

Advantages:

- makes inconsistent output unrepresentable at the publication seam;
- directly matches GNU semantics and modern lifecycle design;
- centralizes leaf partition ownership;
- gives incremental layout a precise cache contract;
- fits the existing presentation transaction.

Disadvantages:

- moderate cross-module migration;
- requires careful sequencing to preserve Lisp evaluation order and fast-path
  performance;
- temporarily exposes old/new seams during small commits.

Verdict: recommended target.

### C. Generic widget/constraint scene graph

Represent all frame/window/chrome/body elements as generic layout nodes similar
to a full toolkit.

Advantages:

- uniform generic composition;
- could support broad non-Emacs UI in the future.

Disadvantages:

- introduces a second general UI model beside Emacs's window/glyph model;
- makes GNU redisplay semantics harder to recognize;
- buffer display rows, scrolling, bidi, overlays, and window-start convergence
  do not naturally become ordinary widgets;
- much larger change with no additional correctness needed for this problem.

Verdict: over-generalized. Use modern lifecycle ideas without cloning a browser
or toolkit object model.

## Recommended target architecture

### 1. A frame-wide layout transaction

```rust
enum FrameLayoutOutcome {
    Stable(SealedFrameLayout),
    NeedsRelayout(RelayoutRequest),
}

enum RelayoutRequest {
    FrameChrome {
        assumed: FrameChromeMetrics,
        measured: FrameChromeMetrics,
    },
    WindowChrome {
        window_id: WindowId,
        assumed: WindowChromeMetrics,
        measured: WindowChromeMetrics,
    },
    Minibuffer(MinibufferAllocationRequest),
    LogicalRevisionChanged {
        before: LayoutRevision,
        after: LayoutRevision,
    },
}
```

One speculative attempt follows GNU's leaf traversal and Lisp evaluation
order. A leaf shapes all of its tab/header/mode rows and can report their three
metrics together. On the first leaf that returns `NeedsRelayout`, the frame
discards the attempt and retries immediately; it does not evaluate later
windows merely to collect more changes. This matters because status-line Lisp
can have observable side effects. Pure, already-computed invalidations may be
coalesced, but compatibility must not change evaluation order for a
micro-optimization.

GNU's frame-menu preparation is the deliberate preflight exception:
`update_tool_bar` evaluates its captions and menu properties before
`redisplay_windows` fills any leaf rows, and physical convergence retries reuse
that one logical result. Window callbacks remain leaf-local: a window's scroll
hook, body, and chrome Lisp run before redisplay advances to its sibling. Do not
hoist those callbacks into a frame-wide preflight; that changes cross-window
observable order. Display
queries invoked by those callbacks use a disjoint, renderer-inert query engine
owned by the evaluator-thread redisplay runtime. That engine runs the canonical
target-only row producer but cannot prepare or mutate a renderer presentation,
so `window-end` is reentrant without aliasing the borrowed presentation engine.
This models GNU's stack-local display iterator as an ownership boundary rather
than a `RefCell` exception.

Inactive echo-area display is also a separate publication domain. Its
minibuffer-shaped snapshot remains part of sealed renderer and interaction
geometry, but a typed `GeometryOnly` publication prevents the temporary echo
source from replacing the live minibuffer's output, `window-end`, or retained
redisplay cache. This mirrors GNU's temporary `with_echo_area_buffer` swap
without making pointer input lose the minibuffer window.

Retries must be bounded. Oscillation should return a diagnostic
`LayoutConvergenceError` containing presentation identity, attempt number,
window, old/new metrics, relevant formats/faces, and logical revisions. An
unstable attempt must never be prepared for presentation.

“Transaction” here means speculative build followed by validation and commit;
it does not mean freezing all Emacs state. Fontification and status-line Lisp
can mutate redisplay inputs while a leaf runs. The coordinator captures the
complete live-window and semantic-source projection before that leaf's Lisp and
after its body/chrome production, and retries when that leaf-local identity
drifts. A later sibling may invalidate an already completed sibling; matching
GNU's observable traversal order, redisplay does not revisit the earlier leaf
in the same frame walk. The mutation remains live and is consumed by the next
redisplay. Structural topology changes are different because they invalidate
the remaining traversal routes, so they reject the whole physical attempt
immediately.

### 2. One canonical per-leaf partition

```rust
struct WindowChromeMetrics {
    tab_line: Px,
    header_line: Px,
    mode_line: Px,
}

struct WindowLayoutBox {
    outer: Rect<FrameSpace>,
    tab_line: Option<Rect<FrameSpace>>,
    header_line: Option<Rect<FrameSpace>>,
    body: Rect<FrameSpace>,
    horizontal_scroll_bar: Option<Rect<FrameSpace>>,
    mode_line: Option<Rect<FrameSpace>>,
    bottom_divider: Option<Rect<FrameSpace>>,
    horizontal: WindowHorizontalBands,
}
```

Frame layout owns leaf outer rectangles. Each leaf layout owns the complete
partition inside its outer rect. `WindowLayoutBox` is the sole authority for:

- body origin and extent;
- tab/header/mode/scrollbar/divider bounds;
- margin/fringe/vertical-scrollbar/text bands;
- body and chrome clips;
- row and cursor transforms;
- hit/source/popup-anchor geometry.

Code should ask this type for regions; it should not pass scalar heights to
several independent arithmetic implementations.

### 3. A stable window result, not a bag of side effects

```rust
enum WindowLayoutOutcome {
    Stable(SealedWindowLayout),
    NeedsRelayout(WindowChromeMetrics),
}

struct SealedWindowLayout {
    window_id: WindowId,
    layout_box: WindowLayoutBox,
    matrix: GlyphMatrix,
    cursor: Option<PresentedCursor>,
    source_positions: Vec<PresentedPosition>,
    hit_regions: Vec<HitRegion>,
    anchors: WindowAnchors,
    partition_signature: WindowPartitionSignature,
}
```

The window attempt may use assumed metrics to walk the body and then shape
chrome. If measured metrics differ, it returns only `NeedsRelayout`; no
publishable `SealedWindowLayout` exists. If they match, all emitted geometry is
known to use the same partition.

This interface is deep: it hides estimation, shaping, evaluation timing,
allocation, and validation. Callers do not need chrome height getters or
post-layout reconciliation.

### 4. Separate input phases

Replace the broad `WindowParams` gradually with explicit inputs:

```text
WindowLogicalInput
    ids, buffer span, point, scrolling, logical bounds, display flags/ticks

WindowStyleInput
    resolved faces, font metrics, line spacing, chrome formats/generations

WindowConstraintInput
    outer allocation, scale, frame metrics, scrollbar/fringe/margin policy

WindowLayoutBox
    physical result, not an input
```

This makes cache dependencies declared and prevents derived geometry from
circulating as if it were source state.

### 5. Local coordinates with typed transforms

Rows and child primitives should use a documented owner-local space (body-local
for body rows, chrome-band-local for chrome) and be transformed to frame space
once by the sealed layout. A generic “window-relative y” for both body and
chrome is too ambiguous because their origins differ.

Intentional overflow effects must be typed as effects with explicit clip
policy. Ordinary glyphs should not rely on later clipping to hide a geometry
disagreement.

Partition edges also need one rounding policy. Use a typed canonical
`LayoutUnit` (fixed fractional logical pixels, or an equivalently deterministic
quantization) for allocation and convergence comparisons. Quantize a measured
intrinsic height exactly once, derive adjacent edges from the same value, and
defer logical-to-device conversion to the surface boundary. Glyph shaping may
retain subpixel values, but scattered `.round()` calls and unrelated epsilon
tests must not decide whether two modules think a band is 17 or 18 pixels tall.

### 6. Projection after sealing

`SealedFrameLayout` should be the only input to projection builders:

```text
SealedFrameLayout
    +-> FrameDisplayState visual primitives
    +-> evaluator PresentationGeometry
    +-> renderer/pointer hit index
    +-> source-position map
    +-> popup/child-frame anchor map
    +-> accessibility/debug geometry
```

`PresentationSpatialPlan` remains useful, but it should compile a sealed
physical layout rather than reconstruct regions from mutable snapshots.
`WindowDisplaySnapshot` should become a compatibility/query projection, not a
second geometry owner.

The render thread may materialize fonts, textures, buffers, and GPU resources.
It must not reinterpret window layout or repair region placement.

### 7. Cache and invalidation model

Use explicit invalidation classes:

- **layout dirty:** any input that can alter sizes, row breaks, positions,
  partitions, transforms, or anchors;
- **paint dirty:** colors/effects that do not alter geometry;
- **composition dirty:** transforms/opacity/damage whose semantics permit
  renderer-side update;
- **presentation-only:** cursor animation phase or other visual time state that
  consumes unchanged sealed geometry.

Retained rows include `WindowPartitionSignature` in their key. A chrome-only
rerender that changes intrinsic metrics escalates to layout dirty. Cursor-only
or animation paths may reuse geometry only while the partition signature is
unchanged.

### 8. Seal-time invariants

For every leaf:

- bands are ordered, non-overlapping, and contained by `outer`;
- body top is exactly the bottom of tab-line/header-line;
- body bottom precedes horizontal scrollbar/mode-line/divider;
- horizontal bands follow the selected margin/fringe/scrollbar ordering and
  exactly account for available width;
- ordinary body glyphs, cursor cells, body hits, positions, and anchors are
  contained by the body region;
- chrome primitives and hits are contained by their owning band;
- all projections use the same frame/window/presentation generation;
- source identities are unique;
- TTY uses the same semantic partition with one-cell metrics.

These checks belong before publication. Debug builds can validate every
primitive; production can retain cheap structural checks.

## Migration plan in small, test-first commits

1. **Lock the failure.** Add a test where assumed tab-line height is 17 and
   shaped height is 21. Assert that the attempt is rejected and that no body at
   y=17 is publishable. Run with `cargo nextest`.
2. **Introduce pure partition types.** Add `WindowChromeMetrics`,
   `WindowLayoutBox`, ordering/containment tests, split-window tests, and TTY
   tests. Do not yet change rendering.
3. **Add typed chrome outcome.** Replace exposed height getters plus measured
   metadata with `Stable`/`NeedsRelayout` at the window seam.
4. **Generalize the frame coordinator.** Fold tab-bar, leaf chrome, and
   minibuffer allocation into one bounded convergence loop. Preserve GNU's
   traversal/evaluation order by retrying at the first window metric mismatch.
5. **Make output speculative.** Build each attempt in attempt-owned frame/window
   builders, including temporary asset/source identities. Only move results
   into retained/presentation state after stability; discarding an attempt must
   not leak IDs or resource requests.
6. **Use one partition everywhere.** Route body rows, chrome rows, clips,
   cursor, decorations, scrollbars/fringes/margins, hit/source maps, and popup
   anchors through `WindowLayoutBox`.
7. **Strengthen seal validation.** Add primitive-to-region containment and
   generation consistency checks to the display protocol.
8. **Harden incremental reuse.** Add the partition signature, exercise
   cursor/edit/scroll fast paths with changing tab/mode-line intrinsic heights,
   and commit caches only from sealed attempts.
9. **Delete duplicate/legacy paths.** Remove dummy regions and post-mutation,
   `PresentedWindowRegionRequest` arithmetic, derivable scalar chrome fields,
   dead legacy `LayoutOutput` types/tests, and obsolete broad `WindowParams`
   geometry fields.
10. **End-to-end verification.** Use `cargo nextest`, then Weston/ydotool GUI
    scenarios and glyph dumps for split leaves with unequal chrome, tall
    images/fonts, fringes/margins/scrollbars, minibuffer resizing, child-frame
    anchors, and retained fast paths.

The first commit is the minimal GNU fix. The later commits deepen the module so
the same class of bug cannot reappear for header-lines, mode-lines, scrollbars,
fringes, margins, or future intrinsic elements.

## What not to do

- Do not make tab-line a frame-global band; it belongs to one leaf.
- Do not merely clamp/clip the first body row. That hides pixels but leaves
  cursor, hit testing, source positions, scrolling, and anchors wrong.
- Do not move Lisp evaluation to the render thread.
- Do not unconditionally pre-evaluate every mode-line twice.
- Do not let the renderer recompute or correct evaluator geometry.
- Do not keep both “estimated physical layout” and “measured semantic layout”
  as equally authoritative.
- Do not build a generic widget framework to solve a specialized redisplay
  convergence problem.
- Do not introduce a permanent feature-flagged shadow architecture. Migrate
  one authority at a time and delete the old derivation as each seam moves.

## Final design decision

Adopt alternative B: a two-level `LayoutTransaction` with per-leaf canonical
partitions, typed relayout outcomes, a bounded frame convergence coordinator,
and one immutable `SealedFrameLayout` from which visual and interaction
projections are derived.

This preserves what is already strongest in NeoMacs—the logical Emacs window
model, evaluator ownership, retained rendering, and atomic presentation
lifecycle—while fixing the missing semantic boundary that currently permits
overlap and geometry drift.

The two-thread Rust frontend is not the cause of the bug. It makes a strong
publication contract more important, but the immediate failure occurs entirely
within one evaluator-thread layout attempt. The ideal architecture is therefore
two nested transactions:

```text
layout transaction:       speculative -> stable/sealed
presentation transaction: prepared -> active -> retired
```

Only the stable result of the first may enter the second.
