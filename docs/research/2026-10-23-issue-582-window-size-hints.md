# Issue #582: font-dependent gaps at maximized screen edges

Investigated 2026-10-23. GNU Emacs was studied first, from the local checkout at `a360712c9d272d950d8d8255ef74570f7e90b7d9`; reference code was not modified. Source analysis is followed by the implementation's local verification record.

## GNU behavior and its limits

`gtkutil.c:xg_wm_set_size_hint` returns without updating hints for `maximized` and `fullboth` on an EWMH-capable X11 WM. Its comment identifies shrinking with KWin and GTK3. In normal state it supplies character increments, or unit increments when `frame-resize-pixelwise` is enabled, together with base/minimum sizes. This is evidence against republishing font-dependent grid constraints during maximization. It does **not** clear previously published constraints. [GNU GTK implementation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/gtkutil.c#L1985-L2046).

GNU separately observes native state: `x_net_wm_state` reads WM properties and stores the Lisp `fullscreen` parameter; ConfigureNotify processing calls it for the frame window. Consequently, user actions through the WM participate in state tracking, without requiring a Lisp fullscreen request. [GNU native-state reconciliation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xterm.c#L18005-L18032), [configure handling](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/xterm.c#L21598-L21608).

GNU documents that some WMs require `frame-resize-pixelwise` to maximize or enter fullscreen without rounding. Therefore, reproducing GNU's early return alone is not proof that the first WM-driven maximize will have no gap. [GNU option documentation](https://github.com/emacs-mirror/emacs/blob/a360712c9d272d950d8d8255ef74570f7e90b7d9/src/frame.c#L7624-L7635).

## Protocol and KWin evidence

ICCCM defines preferred dimensions as `base + integer * increment`; font-dependent increments therefore admit different remainders for a fixed screen size. It also distinguishes base size from minimum usable size. [ICCCM, section 4.1.2.3 WM_NORMAL_HINTS](https://xorg.freedesktop.org/archive/current/doc/xorg-docs/icccm/icccm.html).

EWMH distinguishes maximization, which uses the available workarea, from fullscreen, which fills the screen without decorations. The WM maintains `_NET_WM_STATE` as the current state; a client sends state-change requests. The report's drag-to-top trigger should thus be tested as native maximization, separately from true fullscreen. [EWMH, sections 2.1.1 and 5.7](https://specifications.freedesktop.org/wm/latest-single/).

KWin Plasma 5.24, relevant to the reported Ubuntu 22.04/Plasma 5 environment, floors dimensions using base and resize increments in `constrainClientSize`. `MaximizeFull` constrains the workarea and then positions the smaller rectangle; electric-border maximization has separate positioning logic. This explains both font-dependent leftover pixels and changing gap edges. True fullscreen directly uses `FullScreenArea`. [KWin constraints](https://github.com/KDE/kwin/blob/Plasma/5.24/src/x11client.cpp#L3415-L3458), [maximize placement](https://github.com/KDE/kwin/blob/Plasma/5.24/src/x11client.cpp#L4330-L4379), [fullscreen](https://github.com/KDE/kwin/blob/Plasma/5.24/src/x11client.cpp#L4431-L4438).

Clearing increments after an undersized maximize is insufficient evidence of recovery: KWin's `getWmNormalHints` normally reconstrains the **current** client size. It does not unconditionally recompute the maximized workarea. An actual KWin test must establish recovery, rather than checking only updated properties. KWin can ignore increments/aspect via its explicit StrictGeometry rule, but that is WM policy. [KWin hint changes](https://github.com/KDE/kwin/blob/Plasma/5.24/src/x11client.cpp#L3579-L3608), [WM rule](https://github.com/KDE/kwin/blob/Plasma/5.24/src/x11client.cpp#L3444-L3447).

## Pinned winit and Neomacs seam

At pinned winit revision `2290f91`, X11 `is_maximized()` queries `_NET_WM_STATE` and requires both maximized axes. X11 `fullscreen()` reads the library's requested/shared state instead; its property handler does not deliver a dedicated WM-state-change event. Sampling maximization during surface configure processing can observe native drag-to-top actions. Fullscreen initiated externally needs more careful observation than the cached accessor. [Pinned X11 state accessors](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-x11/src/window.rs#L1160-L1166), [maximization query](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-x11/src/window.rs#L1312-L1330), [property handling](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-x11/src/event_processor.rs#L887-L896).

Pinned Wayland already avoids increment snapping for maximized, fullscreen, and tiled configure states. Preserve its normal floating-window minimum-relative snapping behavior. [Pinned Wayland sizing policy](https://github.com/eval-exec/winit/blob/2290f91c602f8efdf102972457a25e8b6e36f90c/winit-wayland/src/window/state.rs#L467-L500).

Neomacs currently republishes increments unconditionally in `render_thread/geometry_hints.rs`, including direct X11 `PResizeInc` updates. Its `FrameWindowState::set_fullscreen_mode` issues native requests, while `WindowEvent::SurfaceResized` handles native geometry separately. A fix confined to the Lisp-request method misses the report's WM-driven trigger.

## Recommended model and acceptance evidence

The recommended fix is **pixelwise native X11 constraints, with editor-requested character-grid sizing retained independently**. This changes normal X11 border dragging to pixel increments; it avoids the first native maximization being rounded before Neomacs observes its state. ICCCM exposes one global progression, with no separate floating-only scope. A client cannot portably retain character increments for external border dragging while guaranteeing that the WM ignores them for external maximization. This is an inference from the protocol and KWin implementation, supported by GNU's documented pixelwise remedy.

Further recommendations inferred from the sources:

- Represent the native resize policy with enum variants such as `Pixelwise` and `CellGrid { increments }`. Centralize projection in one backend adapter. Use `Pixelwise` for X11 and retain Wayland's existing state-aware grid behavior. Separate this from the editor's character-based size intent rather than silently rewriting grid metrics to unit increments everywhere.
- Keep the latest editor grid geometry distinct from applied native constraints. Font changes while maximized should update retained grid metrics without reintroducing character increments. Restoring normal state should use the latest metrics.
- Reconcile requested and observed state. Native maximize, restore, startup fullscreen, and font/scale changes must all reach the same sizing policy. Avoid issuing surface resize requests in response to authoritative maximized geometry.
- Avoid a post-maximize workarea resize or unmaximize/remaximize bounce. Those approaches race user actions, risk overwriting restore geometry, and duplicate WM workarea policy. Observed state remains useful for display/chrome and Lisp state reconciliation, but it does not solve a constraint already consumed by the WM.
- Keep physical X11 pixels distinct from logical Wayland/Cocoa units. Reuse existing coordinate conversions and minimum/base distinctions.

Tests should cover a non-divisible workarea, several font heights, native drag/maximize, Lisp maximize, true fullscreen, font changes while maximized, restoration, and a second frame. Assert native outer/client geometry and WM state independently from rendered content. A framebuffer-only assertion cannot distinguish an undersized native window from incomplete painting inside a correctly sized window. Keep X11/KWin evidence separate from Wayland tests, where the dependency already handles compositor-constrained states.

## Implementation and local verification

The implemented `NativeResizeGrid` enum selects `Pixels` for Xlib/Xcb and `Characters` for other handles. Its exhaustive projection changes only native increments; the editor's grid, minimum/base sizes and existing coordinate conversions are retained. Both winit's stored hints and the direct X11 property receive the same projection, including font updates and secondary-window creation.

The baseline at `0f45b903a0` was rebuilt with default features plus `video`, with matching bootstrap/final dumps regenerated. Before the fix, the native maximization regression failed under **cargo nextest**: on a 1280×800 Xvfb display, KWin settled at an outer rectangle of 1276×790. A minimized direct probe at another font size measured a 1276×761 client below a 36-pixel titlebar, with character increments 9×24 and base 34×89. Removing hints and reasserting maximization afterward did not recover the missing pixels.

Verification of the fixed development executable and matching dumps:

- 72 runtime unit tests passed, selected by `test(geometry_hints) | test(frame_windows::)`. Three new tests cover Xlib/Xcb allocation, distinct base/minimum sizes, and preservation of Wayland's grid.
- One live GNU oracle test passed for `set-frame-size-and-position-pixelwise`.
- Three TUI comparisons passed: terminal resize geometry, content/modeline reflow, and frame-local bar geometry.
- Three new KWin GUI tests passed: native maximize, native fullscreen, and GNU fullscreen. Heights 100–140 were exercised; maximize also changed fonts in place and restored each window before an exact 91-column resize. Outer rectangles and native states were checked independently; geometry/hint records and root PNGs were retained.
- Three existing Wayland GUI tests passed: HiDPI native grid configuration, decorated fullscreen restoration, and live fullscreen font updates.
- Development build, workspace formatting, shell syntax and diff checks passed. All test invocations used `cargo nextest` after the user's runner preference was specified.

The live-font Wayland fixture initially failed because Ubuntu Mono was absent and Noto Sans was substituted. Supplying the existing test prerequisite through a private fontconfig configuration made its original 9×18 assertion pass; no assertion was relaxed. CI's ecosystem profile already installs Ubuntu fonts and now also installs the native KWin test tools.

Local native testing used KWin 6.6.6 and GNU Emacs 31.1. Times New Roman was unavailable and resolved to Liberation Serif; the actual font path is recorded in each scenario. Plasma 5.24 behavior was studied in source, rather than executed locally. Regression logs and scratch configuration are under repository `tmp/issue582-*`; GUI artifacts are under `target/neomacs-gui-tests/issue-582/`.
