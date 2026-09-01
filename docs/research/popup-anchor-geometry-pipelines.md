# Popup anchor geometry pipelines

Research date: 2026-07-12

This note studies completion-popup placement as a coordinate-space and policy
problem.  The sources are GNU Emacs and Corfu source code plus first-party GTK,
Wayland, Qt, and Chromium documentation/source.

## Executive finding

The mature design is not “compute an `(x, y)` near the cursor.”  It is:

1. acquire an **anchor rectangle** from authoritative rendered geometry;
2. state the anchor rectangle's **coordinate space** and transform it exactly
   once into the popup parent's space;
3. apply an explicit **placement policy** (preferred side/alignment, gap, and
   overflow adjustments);
4. present one resolved popup rectangle, then retain the actual placement when
   the window system is allowed to adjust it;
5. invalidate and resolve again when the anchor, parent transform, popup size,
   scale, or available bounds change.

The `/tmp/log.log` vertical offset came from a concrete API-contract mix in
pre-fix Neomacs: its `posn-at-point` Y included top window chrome, while GNU's
`window-body-pixel-edges` contract already includes that chrome in the body
origin.  Corfu composes those two public values exactly as GNU intends, so
Neomacs counted top chrome twice.  In the captured frame, text clipping begins
at Y=45 while the window begins at Y=24 (21 pixels of top chrome); the popup
begins 22 pixels below the 17-pixel cursor's bottom, matching that duplicated
chrome plus the one-pixel child-frame edge/rounding.  A focused test initially
reproduced the contract failure directly: with 5 pixels of header and 17 pixels
of tab line, Neomacs returned the snapshot Y unchanged instead of subtracting
22.  The same test passes with the implemented contract fix.

The horizontal distance between the cursor and popup is not, by itself, a
bug.  Corfu deliberately anchors to the beginning of the completion base, not
necessarily to point.

## GNU Emacs: geometry contracts

Source revision: GNU Emacs mirror commit
[`0ee48ac4`](https://github.com/emacs-mirror/emacs/commit/0ee48ac4df205e0d915946b5db00e73a0cd21ae0).

### `posn-at-point` returns rendered glyph geometry

GNU `posn-at-point` first asks `pos-visible-in-window-p` for the rendered
position and then feeds that position through `posn-at-x-y`; its documented
result includes `(X . Y)` and `(WIDTH . HEIGHT)` for the glyph at the requested
buffer position
([`src/keyboard.c:13055-13097`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/keyboard.c#L13055-L13097)).

The coordinate contract becomes explicit in `make_lispy_position`: for an
ordinary text glyph, X is relative to the text-area left edge, and Y subtracts
the window's tab-line and header-line heights
([`src/keyboard.c:5862-5884`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/keyboard.c#L5862-L5884)).
Thus a `posn-at-point` text position is local glyph geometry, not a global or
frame-absolute point.

### Window body edges own the chrome offset

GNU's `window-edges` calculation defines the body top as window top plus border,
header-line height, and tab-line height
([`lisp/window.el:3879-3895`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/lisp/window.el#L3879-L3895)).
`window-body-pixel-edges` is simply that calculation with `BODY` and
`PIXELWISE` enabled
([`lisp/window.el:3922-3935`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/lisp/window.el#L3922-L3935)).

This establishes a composable invariant:

```text
frame glyph top = window body top + glyph Y relative to body
```

Each contributor owns one part of the transform.  The glyph API does not leak
window chrome into its local Y, and the body-origin API does not omit it.

### Child-frame positions are in parent-frame coordinates

GNU documents positive `set-frame-position` coordinates for a child frame as
the child's outer-frame top-left relative to the parent frame's `(0, 0)`
([`src/frame.c:4662-4694`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/frame.c#L4662-L4694)).
Emacs 31 also exposes a compound pixelwise size-and-position operation so the
backend can move and resize atomically; when unsupported it falls back to two
operations
([`src/frame.c:4697-4719`](https://github.com/emacs-mirror/emacs/blob/0ee48ac4df205e0d915946b5db00e73a0cd21ae0/src/frame.c#L4697-L4719)).

This is the final coordinate boundary: popup layout must deliver exactly one
parent-frame-relative outer rectangle.

## Corfu: completion semantics and placement

Source revision: Corfu commit
[`4a9c67da`](https://github.com/minad/corfu/commit/4a9c67da16eb64cadaa4bfcc16713188145c83da).

Corfu chooses the anchor buffer position as
`beg + (length corfu--base)`, capped at `point-max`, then calls
`posn-at-point`
([`corfu.el:1159-1176`](https://github.com/minad/corfu/blob/4a9c67da16eb64cadaa4bfcc16713188145c83da/corfu.el#L1159-L1176)).
That is why the popup can remain horizontally at the start of the completion
base while the cursor advances to the right.  Corfu subsequently subtracts the
formatted prefix width (`off`) so candidate text aligns with that base
([`corfu.el:766-789`](https://github.com/minad/corfu/blob/4a9c67da16eb64cadaa4bfcc16713188145c83da/corfu.el#L766-L789)).

`corfu--popup-show` then:

- derives the line anchor height from the greater of default line height and
  the rendered object's height;
- converts `posn-at-point` to `(x . y)`;
- adds `window-inside-pixel-edges` exactly as the local-to-parent transform;
- subtracts margin, prefix offset, and border on X;
- puts the popup below the anchor line, with a pre-Emacs-31 tab-line
  compatibility term;
- flips above when the requested height would exceed the frame;
- clamps X to the frame.

See
[`corfu.el:1017-1063`](https://github.com/minad/corfu/blob/4a9c67da16eb64cadaa4bfcc16713188145c83da/corfu.el#L1017-L1063).
Finally, Corfu resizes and moves the frame in one operation when the Emacs 31
API exists, otherwise it uses `set-frame-size` and `set-frame-position`
([`corfu.el:494-509`](https://github.com/minad/corfu/blob/4a9c67da16eb64cadaa4bfcc16713188145c83da/corfu.el#L494-L509)).

Corfu is therefore not specifying “popup top-left equals cursor bottom.”  Its
semantic request is closer to:

```text
anchor = rendered rectangle at completion-base position
alignment = candidate text aligned with completion base
preferred side = below
fallback = above
horizontal policy = clamp to parent frame
```

## The pre-fix Neomacs contract divergence

The following findings describe the implementation before the geometry fix
developed from this investigation on 2026-07-12.

1. Layout snapshots store a display point's X relative to `text_x`, but store
   Y relative to `window_top`, not the text/body top
   ([`neomacs-layout-engine/src/window_output.rs:1202-1221`](../../crates/neomacs-layout-engine/src/window_output.rs)).
   Consequently `posn-at-point` exposes Y containing tab/header chrome through
   `make_text_area_position`
   ([`neovm-core/src/emacs_core/display/xdisp/mod.rs:4753-4776`](../../crates/neovm-core/src/emacs_core/display/xdisp/mod.rs),
   [`:4878-4890`](../../crates/neovm-core/src/emacs_core/display/xdisp/mod.rs)).

2. `window_body_edges_pixels` changes horizontal body offsets and removes the
   mode line, but leaves `body_top` equal to the raw window top; it does not add
   tab/header heights
   ([`neovm-core/src/emacs_core/display/window_cmds/mod.rs:1161-1182`](../../crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs)).

3. Neomacs separately reports the rendered tab-line height through
   `window-tab-line-height`
   ([`neovm-core/src/emacs_core/display/window_cmds/mod.rs:3058-3100`](../../crates/neovm-core/src/emacs_core/display/window_cmds/mod.rs)).

So the pre-fix Neomacs implementation had this mixed contract:

```text
body_top = window_top
posn_y   = top_chrome + glyph_y_in_body
```

where GNU's deep contract is:

```text
body_top = window_top + top_chrome
posn_y   = glyph_y_in_body
```

Both pairs can sum to the same glyph top in internal code that uses Neomacs's
Rust helpers together, but they are not API equivalent.  The loaded GNU Lisp
`window-edges` implementation follows GNU's public contract and includes top
chrome in the body origin.  Corfu combines that body origin with
`posn-at-point`, so Neomacs's leaked chrome is added a second time and becomes
a visible offset.  The pre-Emacs-31 compatibility term in Corfu is not involved
in this capture: the installed `corfu.elc` was compiled by Emacs 31.0.50.

The implemented fix makes the public contracts GNU-compatible without a Corfu
special case or a hard-coded child-frame offset.  `WindowDisplaySnapshot` now
owns conversion from its authoritative window-relative redisplay coordinates
to GNU text-body-relative pixel and row coordinates.  `posn-at-point` and
`posn-at-x-y` use that conversion, while `window-body-pixel-edges` adds the
same measured top-chrome height to the body origin.  Their composition is now:

```text
rendered glyph top = body_top + text_body_relative_posn_y
```

## Mature GUI designs

### GTK/GDK: typed anchor rectangle plus policy

GTK's `GtkPopover` points to a rectangle explicitly expressed in the parent
widget's coordinate space
([`gtk_popover_set_pointing_to`](https://docs.gtk.org/gtk4/method.Popover.set_pointing_to.html)).
The caller gives a preferred side, but GTK may choose the opposite side when
space is insufficient
([`gtk_popover_set_position`](https://docs.gtk.org/gtk4/method.Popover.set_position.html)).
Custom parent widgets must update popover positioning from their size-allocation
path, tying placement invalidation to layout
([`GtkPopover`](https://docs.gtk.org/gtk4/class.Popover.html)).

At the lower GDK layer, `GdkPopupLayout` separates:

- the anchor rectangle;
- the anchor point on that rectangle;
- the anchor point on the popup surface;
- an offset;
- constraint hints for flip, slide, and resize.

The window system can make the final decision, and GDK exposes the resulting
position and anchors so rendering (for example, an arrow) can reflect actual
placement
([`GdkPopupLayout`](https://docs.gtk.org/gdk4/struct.PopupLayout.html),
[`set_anchor_hints`](https://docs.gtk.org/gdk4/method.PopupLayout.set_anchor_hints.html)).

Design lesson: content code supplies semantic intent; a geometry resolver owns
coordinate conversion and constraints; rendering consumes the resolved result.

### Wayland: compositor-owned constrained placement

The official `xdg_positioner` protocol models popup placement as a set of rules
relative to a parent: non-zero popup size, an anchor rectangle, anchor,
gravity, offset, and constraint adjustments (`slide`, `flip`, `resize`).  The
compositor copies the rule set when the popup is created and resolves it
against visible constraints
([`xdg-shell.xml`, `xdg_positioner`](https://wayland.app/protocols/xdg-shell#xdg_positioner)).

This matters for Neomacs even if child frames are currently compositor-local
overlays: global coordinates are not a universally valid primitive.  Popup
geometry should remain parent-relative until the backend that owns the parent
surface resolves it.

### Qt: completion API accepts an anchor rectangle

Qt's `QCompleter::complete(const QRect&)` accepts a rectangle in the associated
widget's coordinates; the popup appears at its left edge, and omitting it means
“below the widget”
([`QCompleter::complete`](https://doc.qt.io/qt-6/qcompleter.html#complete)).
Qt provides explicit coordinate transforms such as `QWidget::mapToGlobal` and
`mapFromGlobal`
([`QWidget`](https://doc.qt.io/qt-6/qwidget.html#mapToGlobal)), while
`QScreen::availableGeometry` supplies the work area excluding reserved UI and
separately documents device-independent versus device-dependent pixels
([`QScreen`](https://doc.qt.io/qt-6/qscreen.html#availableGeometry-prop)).

Design lesson: the completion producer supplies a rectangle, not a guessed
screen point; transform APIs and screen constraints remain toolkit-owned.

### Chromium Views: anchor object, resolver, observation, tests

Chromium's bubble delegate obtains an anchor rectangle in screen coordinates
from the anchor view or tracked element and explicitly compensates for window
transforms
([`bubble_dialog_delegate_view.cc:703-737`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/views/bubble/bubble_dialog_delegate_view.cc#703)).
It observes visible anchor-bound changes and re-runs sizing/placement rather
than leaving a stale popup
([`bubble_dialog_delegate_view.cc:192-230,803-811`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/views/bubble/bubble_dialog_delegate_view.cc#192)).

Placement itself is delegated to `BubbleFrameView::GetUpdatedWindowBounds`,
which takes anchor rect, arrow preference, client size, and an adjustment flag;
it mirrors or offsets the arrow against anchor-window and screen bounds before
returning the final rectangle
([`bubble_frame_view.cc:819-873`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/views/bubble/bubble_frame_view.cc#819)).
Chromium also explicitly handles Ozone/Wayland platforms without global screen
coordinates instead of pretending global coordinates exist
([`bubble_frame_view.cc:884-920`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/views/bubble/bubble_frame_view.cc#884)).

The associated tests verify that moving an anchor view changes the anchor
rectangle and that deleting it preserves the last known rectangle
([`bubble_dialog_delegate_view_unittest.cc`](https://chromium.googlesource.com/chromium/src/+/refs/heads/main/ui/views/bubble/bubble_dialog_delegate_view_unittest.cc)).

Design lesson: placement is a testable pure-ish module around an explicit
anchor, while lifecycle code observes anchor changes and invokes it.

## Recommended Neomacs abstraction

### 1. Make coordinate spaces part of the types

Avoid passing unqualified `f32 x, y`.  Introduce lightweight wrappers such as:

```rust
struct WindowBodyPx;
struct ParentFramePx;

struct Point<Space> { x: f32, y: f32, _space: PhantomData<Space> }
struct Rect<Space>  { origin: Point<Space>, size: SizePx }
```

At minimum, names must carry the space (`glyph_rect_in_window_body`,
`popup_rect_in_parent_frame`).  Conversion functions should be few, explicit,
and testable.  Scale/device-pixel conversion belongs at the renderer/platform
edge, not mixed with Lisp geometry.

### 2. Preserve GNU public API contracts at the VM/display seam

The authoritative redisplay snapshot should expose a glyph rectangle relative
to the **window text/body origin**.  `window-body-pixel-edges` should expose the
body rectangle in frame coordinates, including top chrome and excluding the
mode line.  The one conversion is then:

```text
anchor_in_parent = window_body_origin_in_frame + glyph_rect_in_body
```

This both matches GNU and removes the compensating representation mix that
made the Corfu compatibility branch fail.

### 3. Model popup intent, not just frame coordinates

A reusable request can be:

```rust
struct PopupPlacementRequest {
    anchor: Rect<ParentFramePx>,
    popup_size: SizePx,
    anchor_edge: AnchorEdge,      // e.g. bottom-left
    popup_edge: AnchorEdge,       // e.g. top-left
    offset: VectorPx,
    constraints: ConstraintPolicy,
}

struct PopupPlacement {
    rect: Rect<ParentFramePx>,
    actual_side: Side,
    adjustments: AdjustmentFlags,
}
```

The same resolver can serve completion popups, tooltips, menus, hover cards,
signature help, and child-frame UI.  It should not know about Corfu.

### 4. Separate responsibilities

```text
redisplay snapshot
  owns glyph/body geometry
        |
        v
anchor acquisition
  selects buffer position / UI element
        |
        v
coordinate mapper
  converts once into parent-frame space
        |
        v
placement resolver
  preferred alignment + flip/slide/resize/clamp
        |
        v
child-frame compositor/backend
  applies resolved rect atomically and reports actual placement
```

The compositor should consume resolved geometry; it should not repair offsets
based on which package created the frame.

### 5. Invalidate from geometry dependencies

Re-resolve when any of these generations change:

- anchor glyph/row geometry;
- window layout or scrolling;
- tab/header/mode-line/fringe/margin geometry;
- parent frame size/scale/transform;
- popup preferred size;
- available work area.

This follows GTK's size-allocation ownership and Chromium's anchor observation.
It also avoids polling or tying popup movement to unrelated full redisplays.

## Tests that prevent recurrence

1. **GNU oracle contract tests**: with a tab line and header line, compare
   `posn-at-point`, `window-body-pixel-edges`, `window-tab-line-height`, and the
   derived anchor on GNU and Neomacs.
2. **Coordinate algebra tests**: assert
   `body_origin + posn_xy == rendered_glyph_origin` for split windows, fringes,
   margins, tab/header lines, scrolling, child frames, and scale factors.
3. **Corfu integration regression**: make a completion popup on a window with a
   non-zero tab line and assert its outer top equals anchor bottom (plus only
   the requested border/gap), not `+ tab_line_height` again.
4. **Placement policy table tests**: enough space below; flip above; slide at
   left/right; resize when neither side fits; RTL alignment; multi-monitor/work
   area; nested child frame.
5. **Metamorphic tests**: translating the parent and anchor by vector `d`
   translates the result by `d`; changing only popup content does not move the
   anchor; adding top chrome changes body origin but not local glyph Y.
6. **Trace diagnostics**: log the anchor rectangle and its named space, body
   origin, requested placement, adjustment decision, and final rectangle in one
   structured event.  A dump should make an extra transform obvious without
   reverse-engineering unrelated glyph records.

The key invariant is: **every rectangle has one declared owner and one declared
coordinate space; every transform happens once at a named boundary.**
