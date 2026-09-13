# 8K startup: output readiness is not socket readiness

## Confirmed cause of the missing first callback

The installed, unmodified Weston 15 initially refused to repaint because its
output was not ready. In
[`weston_output_schedule_repaint`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/libweston/compositor.c),
`!output->ready` returns before scheduling. The documented source contract of
`weston_output_set_ready` requires the shell to install content such as its
background first. Desktop-shell's
[`background_committed`](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/desktop-shell/shell.c)
maps that content and marks the output ready.

With no config, the
[desktop-shell client](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/clients/desktop-shell.c)
uses Cairo and the default tiled pattern. On this machine its initial 8K
background upload occurred after the fullscreen fixture's two-second timer.
This was not evidence that Neomacs's frame scheduler lost a callback.

Using stock `weston --debug --logger-scopes=log,proto` and `weston-debug
scene-graph`, the captured server trace showed:

- Before background commit: mapped Neomacs parent, one committed buffer, no
  painted frames, output reporting `no repaint`.
- At 11:25:50.591: desktop-shell PID 224482 commits a 7680x4320 background
  buffer on surface 21.
- At 11:25:50.741: Neomacs PID 224489 receives its pending callback 126.
- Subsequent Neomacs parent commits and callbacks proceed normally.

Artifacts: `target/diagnostics/issue-360/startup-fonts/scene-weston.log`,
`scene-graph.log`, and `scene-neomacs.log`.

Disabling `startup-animation` alone did not help: removing the fade curtain
does not bypass output readiness. The earlier inference that the curtain
alone might explain the stall was tested and rejected.

## Small harness fix, verified independently of the observer

The [official Weston configuration manual](https://gitlab.freedesktop.org/wayland/weston/-/blob/15.0.0/man/weston.ini.man)
documents `background-color`. A nonzero solid background without an image
uses the single-pixel-buffer branch of `background_draw`, avoiding an
output-sized Cairo pattern before first repaint. Normal headless tests now
use the repository-owned `fixtures/weston-headless.ini` with this setting.
No startup sleep, compositor patch, or forced parent commit is involved.

The original 8K decorated fullscreen nextest failed in 2.64s with the default
pattern (`presentation-boundary-red.log`) and passed in 5.95s after only this
configuration change (`solid-background-control.log`), using the same old
release executable. Commit: `8ef588d82`.

This fixes the test harness's unnecessary startup dependency. It does not
claim to fix Weston's separate rejection of a cached synchronized child
detach. The standalone raw-protocol diagnostic still covers that case.

## Presentation-confirmed test boundary

An opt-in runtime observer uses the official
[`wp_presentation` protocol](https://gitlab.freedesktop.org/wayland/wayland-protocols/-/blob/main/stable/presentation-time/presentation-time.xml).
Feedback is associated with the upcoming surface commit. Only a `presented`
event advances the test receipt; a `discarded` event does not. A surface frame
callback, successful queue submission, or readback PNG is not substituted for
this confirmation. On headless Weston this means presentation to its virtual
output, not verification of physical monitor scanout.

`NEOMACS_GUI_PRESENTATION_RECEIPT` opts in to an atomically replaced Lisp-readable
receipt containing submission identity, frame identity, logical dimensions,
and scale. Disabled sessions do not bind the protocol. The observer owns its
guest queue on the render thread, dispatches nonblocking pending events, and
tears down before the winit display. It does not control rendering or pacing.

The new fixture advances only after each confirmed scale-2 geometry: startup
745px, resized 844px, fullscreen 3840px, restored 844px/91 columns. It has a
bounded failure deadline, not a fixed startup delay. A second test retains
Weston's slow default pattern to exercise output-readiness latency.

The observer is currently Wayland-specific diagnostic support. Other
platforms do not manufacture receipts or claim native verification.

## Verification checkpoints

- Receipt-gated GUI test was red before observer implementation: explicit
  missing-receipt timeout (`presentation-receipt-red.log`).
- Runtime nextest: 966 passed, 5 skipped (`presentation-runtime-tests.log`).
- Harness nextest: 16 passed (`presentation-harness-tests.log`).
- Release rebuild succeeded in 6m16s. Matching pdump regenerated without byte
  compilation or autoload regeneration. Fingerprint:
  `D7665460D2C20C8733A01C3CF706D0D6FD11BE4F75F7A71EE9C2B86DB2C99BFD`.
- Both presentation-gated cases passed: solid background in 1.58s, default
  patterned background in 4.33s (`presentation-receipt-green.log`).
- Broader final GUI run: 11 passed, including the legacy 8K fullscreen case,
  both receipt-gated cases, 1x/2x font/SVG startup, scale ordering, failure
  handling, and GNU Emacs's width/height oracle (`presentation-gui-all.log`).
- Final runtime nextest after all production changes: 966 passed, 5 skipped
  (`presentation-runtime-final.log`).
- Early-transition control with the same new executable, observer enabled,
  and the default patterned desktop still fails before any presentation
  receipt: only submission 1 at scale 1 is requested, then surface 24 is
  rejected with old 833x44 dimensions at scale 2. Artifacts are in
  `early-control-uUwj1s/`. This confirms the observer itself did not make the
  early case pass by perturbing initialization.
- The standalone raw cached-detach probe still reproduces even with the solid
  desktop (`presentation-raw-detach-control.log`). No compositor fix is claimed.
- `lisp/ldefs-boot.el` remains unchanged, SHA-256
  `094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

All log names above are under `target/diagnostics/issue-360/startup-fonts/`.
No GitHub comment or push was performed.
