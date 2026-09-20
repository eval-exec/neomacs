# Native menu interaction and placement

The GUI suite drives pointer and keyboard input through an isolated X server
into Weston, then into Neomacs's native Wayland popup surfaces. Screenshots are
of the compositor output, not the main editor surface: native menus have their
own surfaces. The output is 1000×700 pixels; the backing X server is 1280×800.
The dropdown and nested-menu contracts also run at output scale 2.

## Contracts

- A dropdown touches the bottom of its heading and aligns horizontally when
  there is room. A wide CJK heading remains clickable at scale 1 and 2.
- A submenu touches the parent's right edge and aligns with its row. Pointer
  traversal across that boundary can select its command.
- At the right edge, the dropdown remains onscreen and the submenu flips left,
  retaining row alignment. Context menus and their children remain onscreen
  near each corner.
- Mouse and keyboard selection execute the intended Lisp command once.
- Escape and an outside click cancel without executing a command, and editor
  typing works after cancellation and selection. Cancellation requires a new
  configured popup, visible panel pixels, and restoration of the closed image.
- A tall child on the last parent row slides upward at both bottom corners;
  its last item remains visible and clickable.
- Disabled items cannot execute, keyboard traversal skips them, checkbox state
  changes visibly, and pointer motion can switch menu-bar headings.

`fixtures/menu-interaction.el` defines deterministic commands and records their
observable effects. `fixtures/menu-heading-edge.el` positions a wide heading
near the right edge; `fixtures/menu-corners.el` adds rows so bottom-edge
constraints are exercised. `tests/native_menus/interaction.rs` contains the behavioral
assertions; `tests/native_menus.rs` owns the existing Linux native-input harness
and earlier #381/#387 regressions.

`fixtures/menu-bottom-submenu.el` adds a tall child to the last parent row, so
the child itself must slide upward. `tests/native_menus/popup_trace.rs` tracks
typed popup events and object lifetimes: reconfiguration updates an existing
popup, while ID reuse after destruction creates a new lifetime.

The geometry checks compare rendered screenshot changes and compositor
`xdg_popup.configure` replies. A positioner request alone is not evidence of
where a popup appeared. Parent-relative compositor coordinates and screenshot
pixel rectangles have separate Rust types.

## Failure found by the edge test

Before the fix, a mouse-opened context menu near the right edge received
`configure(972, 9, 170, 88)` on the 1000-pixel-wide output. Its rightmost 142
pixels were offscreen. The trace showed `set_constraint_adjustment(0)`.

`popup-menu-normalize-position` converts mouse events to coordinate pairs.
`PopupMenuPosition::at` treated those pairs as unconstrained origins. Menu
origins now use the existing `AtAnchor` placement with
`PopupConstraintPolicy::Shift`, for both coordinate pairs and raw events. The
evaluator supplies placement intent; the native compositor applies it after the
menu has been measured. The generic `PopupPlacement::at` constructor and dialog
placement are unchanged.

GNU Emacs references inspected locally:

- `src/gtkutil.c`: `create_menus` creates the native menu bar and attaches child
  menus with `gtk_menu_item_set_submenu`.
- `src/pgtkmenu.c`: `create_and_show_popup_menu` uses
  `gtk_menu_popup_at_pointer` for clicks and `gtk_menu_popup_at_rect` for explicit
  positions. Native toolkit placement owns the resulting popup geometry.

## A second failure: row alignment after flipping left

After constraining the root menu, the right-edge case configured its parent at
`(749, 9, 251, 208)` and the child at parent-relative `(-150, 16, 150, 28)`.
The highlighted parent row started at screen y=33, but the child's top was y=25:
an 8-pixel misalignment, visible in the screenshot.

The native adapter translated `FlipAndShift` into permission to flip on both
axes. A compositor could therefore bottom-align a horizontally flipped child
with its parent row. This differed from the protocol's semantic resolver,
whose opposite of `Right` is only `Left`.

The adapter now exhaustively matches `PopupPreferredSide`: above/below permits
vertical flipping, left/right permits horizontal flipping, and an explicit
anchor has no opposite side. Sliding remains available on both axes, with
vertical resizing for menus that exceed the output height. This keeps policy
in the existing placement types and native constraint translation, rather than
adding compensating pixel offsets to menu painting.

Row alignment uses the highlighted row's rendered pixels. The detector requires
a change across most of a row, so movement of the native pointer does not count
as a highlight. Compositor positions independently detect overlap that a
screenshot crop might otherwise hide.

## Running

Build production changes through `cargo xtask fresh-build --release`, then:

```sh
TMPDIR="$PWD/tmp" cargo nextest run -p neomacs-gui-tests \
  --test native_menus --test-threads 1 --no-fail-fast
```

Requires Weston with X11/kiosk-shell support, Xvfb, xdotool, and ImageMagick
`import`. Screenshots and protocol logs are kept under
`tmp/neomacs-gui-tests/native-menu-*`.

These are Linux-native tests. On `yibie-et`, an SSH-launched process reported
both `AXIsProcessTrusted()` and `CGPreflightScreenCaptureAccess()` as false.
The new suite has not been verified on macOS; its native input/capture driver
and OS permissions remain follow-up work.

## Verification

After `cargo xtask fresh-build --release` completed successfully:

- Native GUI suite: **12 passed**, including nine new tests and the three
  existing #381/#387 regressions. Run ID
  `6978bd05-3446-43d9-9fc6-fa655bc2c471`; serial run, 29.1 seconds.
- Core popup-menu tests: **14 passed** (`test(x_popup_menu)`).
- Display protocol placement tests: **3 passed** (`test(popup_placement)`).
- Formatting checks and `git diff --check` passed.

The corrected right-edge child is configured at parent-relative
`(-150, 24, 150, 28)`, instead of `(-150, 16, 150, 28)`. The bottom-right
parent is `(749, 492, 251, 208)`: its right and bottom edges coincide with the
1000×700 output, and its child remains fully visible.

Local evidence is retained under `tmp/menu-interaction/`:

- `edges-second.log`: original offscreen failure.
- `gui-final.log`: row-alignment failure after constraining the root.
- `gui-verified.log`: final 12/12 GUI result.
- `core-final.log`, `placement-tests.log`: supporting test results.
- `fresh-build-axis.log`: successful final fresh build.
- `submenu-before.png`, `submenu-after.png`: unmodified compositor screenshots
  of the right-edge row-alignment regression, both 1280×800.

### Review follow-up

The strengthened suite passed **14/14** tests (13 native GUI tests and one wire
trace regression), serially in 33.4 seconds. Run ID:
`c0ad5108-27ea-452f-b9ce-801df89d02c9`. This follow-up changes only tests and
fixtures; it uses the production binary from the fresh build above.

Evidence under `tmp/menu-review/`:

- `cancel-negative-control.log`: the original cancellation test incorrectly
  passed with menu opening disabled.
- `cancel-fixed-negative.log`: the strengthened test fails with that same
  disabled binding; `cancel-fixed-green.log` passes with the real binding.
- `trace-red.log`: repeated configuration incorrectly counted as another popup.
  `trace-green.log`: lifetime tracking passes the regression.
- `tall-short-negative.log`: replacing the tall child with one row fails the
  fixture-height assertion, proving the short-child coverage gap is closed.
- `tall-bottom-second.log`: the tall child slides upward at both bottom corners
  and its last command is clickable. The test holds Right until the native
  popup grab is accepted; rapid key-release timing is outside this contract.
- `gui-verified.log`: final 14/14 result, including the expanded trace case
  interleaving parent reconfiguration, child creation, destruction, and ID reuse.

An additional requested verification run passed all 14 native-menu target tests
in 40.4 seconds (run `e36dcf31-f15b-4ff7-92d5-97211da317c8`), all 14 core
popup-menu tests, and all 3 protocol placement tests. Logs are
`tmp/menu-review/reverify-gui.log`, `reverify-core.log`, and
`reverify-protocol.log`. Workspace formatting and `git diff --check` also passed.

### macOS verification (2026-09-20)

Applied the current uncommitted changes to the clean `yibie-et` checkout at
`/Users/chenyibin/neomacs`, based on
`0f861a295240dabe73537c59b096ac3331741637` (two commits ahead of the Linux base).
`cargo xtask fresh-build --release` succeeded, including Lisp runtime preparation.

- Seven selected native macOS GUI checks passed in 37.9 seconds: child-frame
  positioning and sizing, native startup/resize, imageless initialization,
  primary selection, surface readback, and window-chrome theme changes.
  Run ID: `f00f7014-6545-4c38-87d7-8d516e95343f`.
- All 14 core popup-menu tests and all 3 protocol placement tests passed.
- The native-menu target contains zero tests on macOS because it is Linux-gated.
  These GUI smoke results do **not** verify menu interaction or placement.
- Both `AXIsProcessTrusted()` and `CGPreflightScreenCaptureAccess()` remained
  false for the SSH-launched process. A macOS native menu input/capture driver
  and desktop permissions are still required for equivalent interaction coverage.
- Remote changes remain uncommitted; no push was performed. No test-owned
  release Neomacs process remained after completion.

Remote logs: `tmp/menu-verification/`. Local copies:
`tmp/menu-review/macos/{fresh-build,gui,core,protocol,native-menu-run}.log`.
