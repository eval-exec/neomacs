# N-607-ns-toolbar: native allocation remains divergent

The shared `frame_inhibit_resize` predicate and coherent settled height
reporting apply to every backend. The toolbar/menu/border consumer that
preserves the text grid applies to the Linux GTK oracle profile. It must not
be applied to NS without modeling its native toolbar allocation transition.

The pinned GNU Emacs source is
`a360712c9d272d950d8d8255ef74570f7e90b7d9`. Its NS setter mutates the external
toolbar before calling `adjust_frame_size(..., 2, ..., Qtool_bar_lines)`
(`src/nsfns.m:723–770`). The setter itself documents the inhibition limitation.
`src/nsmenu.m:1095–1115` removes it with `setVisible:NO` and `setToolbar:nil`;
the comment explains that the animation repeatedly resizes the window.
Showing it uses `src/nsmenu.m:1237–1255` and
`EmacsWindow::createToolbar` (`src/nsterm.m:9838–9860`).

`resizeWithOldSuperviewSize` (`src/nsterm.m:8343–8366`) reads actual Cocoa
content bounds and calls `change_frame_size` with delayed application. The
later update (`src/dispnew.c:6694–6728`) uses inhibit mode 5. This authoritative
native allocation can supersede the earlier core request. The native geometry
macros (`src/nsterm.h:1062–1078`) use live Cocoa dimensions. The source supplies
no fixed 32- or 50-pixel adjustment.

The attested `macos-ns` oracle, Menlo-13, 100x35, independently launched `-Q`
GUI, 20-second warmup and native event-loop settlement produced these values
for toolbar ON/OFF/ON. Both explicit policies, `(tab-bar-lines)` and `t`, gave
the same GNU transition:

| Query | GNU | Neo before the consumer correction |
| --- | --- | --- |
| `frame-height` | 35 / 32 / 35 | 39 / 39 / 39 |
| `frame-text-lines` | 35 / 32 / 35 | 39 / 42 / 39 |
| Native/pixel height | 529 / 497 / 529 | 597 / 631 / 597 |
| Text pixel height | 525 / 493 / 525 | 597 / 631 / 597 |
| Quartz window height | 569 / 519 / 569 | unchanged |

GNU's character height was 15 throughout. Its internal border accounted for
the four-pixel native/text difference. The 493-pixel text area contains 32
complete rows and a 13-pixel remainder; it is not a grid-rounded allocation.
This is an observed NS contract, separate from the GTK preserve-grid contract.

The strict NS twin in `scripts/macos-navigation-parity/toolbar-resize.py`
compares GNU and Neo transition deltas for both policies and reports the
remaining divergence. It also checks `frame-height == frame-text-lines` in
every settled state. The known NS allocation mismatch is not a passing row
and is not a blocker for the scoped Linux consumer correction.

The long-term seam is a host-owned native chrome transition with authoritative
content bounds: either real NSToolbar realization or a GPU toolbar adapter
that reproduces the Cocoa allocation sequence. The existing pending resize
request/acknowledgement machinery must retain ownership of size intent.
[Winit's resize API](https://docs.rs/winit/0.30.13/winit/window/struct.Window.html#method.request_inner_size)
likewise distinguishes requested size from the later native resize event;
that is an internal design comparison, not the GNU observable contract.
No scalar compensation, manual post-startup resize, or relaxed NS assertion
is part of this correction. The separate startup font-metric divergence must
also be resolved independently of toolbar policy.
