# Native scale observations must not fabricate logical resizes

Follow-up: [the separate 8K output-readiness investigation](2026-09-13-weston-output-readiness.md)
identifies the startup stall and verifies presentation-gated fullscreen tests.

## Reproduction and failing regression

`cargo nextest run -p neomacs-gui-tests --test desktop_font_startup -E
'test(hidpi_scale_change)' --run-ignored all` reproduced the startup bug on
installed Weston 15, physical 3840x2160 at scale 2. Before production changes,
the real GUI-to-evaluator input stream reported width 373 at scale 2, followed
by the correct width 745. The expected 80-column Ubuntu Mono grid needs 745
logical pixels, regardless of backing scale.

The GUI regression includes the input-bridge trace because output scale
discovery can precede loading the Lisp fixture. A Lisp window-size hook alone
missed that interval. Its final Lisp assertion independently checks 80 columns.
The red log is `target/diagnostics/issue-360/startup-fonts/scale-order-red.log`.

## Root cause and GNU Emacs comparison

Neomacs's `ScaleFactorChanged` handler replaced the scale and then divided
cached physical content dimensions by that new scale. Those dimensions still
belonged to the previous scale. It published the mixed observation as an
ordinary resize, exposing a transient half-size frame to the evaluator.

GNU Emacs's PGTK backend separates these responsibilities:

- `src/pgtkfns.c:update_watched_scale_factor` refreshes backing surface size
  using the existing desired logical dimensions.
- `src/pgtkterm.c:size_allocate` accepts GTK's allocation and calls
  `xg_frame_resized` with its logical width and height.

Source inspected in `/home/exec/Projects/github.com/emacs-mirror/emacs`.

Pinned winit `dc7af19` also separates notification from completion. Wayland
always emits `SurfaceResized` after `ScaleFactorChanged`; AppKit calls
`surface_resized` after handling the scale callback; Win32 applies its native
`SetWindowPos` after delivering that callback. Querying or reinterpreting
cached dimensions inside the scale callback is therefore the wrong seam.

## Ownership after the fix

Each frame window retains an optional pending scale observation. This does
not mutate committed geometry, glyph scale, or renderer scale. Native resize
completion consumes it and applies size and scale together before publishing
one evaluator resize. The scale setter is private to the frame-window module.

If no resize event arrives, the event-batch boundary samples the native
window's final surface size and completes through the same resize path.
Redraw also settles a pending observation before rendering. This is shared
code, not a Linux-only conditional. Pending state belongs to the window and
is discarded with it; there is no separate window-ID queue to outlive it.

`Option` models the pending/committed distinction directly; there is no string
dispatch or boolean flag paired with an independently stored scale value.

## Validation

- Focused display-runtime resize/scale nextest: 35 passed.
- Full display-runtime nextest: 966 passed, 5 skipped.
- GUI harness nextest: 16 passed.
- `cargo build -p neomacs --release`: succeeded in 6m10s. The only reported
  warning was the existing unused `HashMap` import in `mock-display`.
- Matching pdump regenerated using `loadup --temacs=pdump`, without byte
  compilation or autoload regeneration. Executable fingerprint:
  `9CB3481D8E01E66D842755C31BF29D78DCBFD6981C5353300FAC05FE2B089EE9`.
- Real GUI nextest: all seven selected startup, scale, font/SVG, resize and
  fullscreen cases passed. The scale regression now sees 745x689 at scale 1
  followed directly by 745x688 at scale 2, with no half-size event.
  Log: `target/diagnostics/issue-360/startup-fonts/scale-order-gui-green.log`.
- The separately rerun 8K decorated fullscreen test still fails with the
  cached-decoration scale error. Log: `scale-order-8k-result.log` in the same
  directory. The scale fix does **not** resolve that separate failure.
- No macOS or Windows runtime claim; their pinned backend source was inspected.
- `lisp/ldefs-boot.el` SHA-256 remains
  `094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

## Separate 8K investigation: do not conflate it with scale ordering

The installed `weston-debug scene-graph` tool showed a mapped Neomacs window
under an opaque desktop-shell startup fade surface, with one committed parent
buffer but no painted frames. A longer, five-second startup probe eventually
received frame callbacks and presented repeatedly.

The [official Weston configuration
manual](https://cgit.freedesktop.org/wayland/weston/tree/man/weston.ini.man)
documents `startup-animation=none`. The installed version's manual and
[Weston 15 shell implementation](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/desktop-shell/shell.c)
were checked. A controlled run with that setting removed the fade surface,
but the two-second fullscreen sequence still failed. Thus the curtain alone
does **not** explain the missing early repaint. That unsuccessful config
experiment was removed from the harness; it is not shipped as a fix.

The 8K failure still has its separate cached-subsurface/null-detach protocol
reproducer, described in the preceding Weston audit. Neither the absence of
an early callback nor that protocol failure justifies patching Weston or
forcing unsolicited parent commits. A socket-ready check does not establish
presentation readiness. Further work should observe presentation explicitly
before attributing startup scheduling to Neomacs or the compositor.

Logs under `target/diagnostics/issue-360/startup-fonts/`: `scene-graph.log`,
`scene-neomacs.log`, `scene-weston.log`, `startup-curtain-red.log`, and
`startup-curtain-green.log` (the latter records the **failed** no-fade probe,
despite its initial prospective filename). No compositor was built or patched.
