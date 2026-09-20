# Native macOS GUI verification

The shared menu contracts use real desktop input, independently observed window
bounds, native-resolution screenshots, and fixture command effects on Linux and
macOS. They verify opening, placement, selection, cancellation, nested menus,
disabled items, keyboard navigation, restored typing, and tall submenus at both
bottom corners.

## Code layout

- `crates/neomacs-gui-tests/src/interaction/mod.rs`: typed desktop actions,
  observations, errors, window identities, and the `DesktopDriver` interface.
- `interaction/geometry.rs`: explicit desktop-point to capture-pixel mapping,
  including negative desktop origins and Retina scaling.
- `interaction/linux.rs`: isolated Weston/X11 input and capture. Its popup
  observer resolves accepted parent-relative rectangles through live lifetimes.
- `interaction/macos/mod.rs`: bounded Unix-socket requests to a persistent native
  helper. Missing prerequisites fail explicitly rather than silently skipping.
- `interaction/macos/keyboard.rs`: a scoped Core Foundation / Carbon input-source
  guard. It selects the enabled ABC layout for physical key events and restores
  the previous source when the session drops, including assertion unwinding.
- `interaction/macos/Driver.swift`: Quartz input, ScreenCaptureKit window
  discovery/capture, and Accessibility window fitting. Connection loss releases
  held input. Each observed window ID carries a lifetime generation.
- `tests/native_menus/contracts.rs`: shared behavior and pixel assertions.
  Capture timeouts preserve consecutive image regions and name the failing stage.
- `tests/macos_native_menus.rs`: owned editor processes and native test execution.

Swift keeps Apple's asynchronous desktop framework operations in the
permission-approved helper. Rust owns contracts, coordinate types, action enums,
and the scoped keyboard layout. No external keyboard-switching utility is
required by the harness.

## Setup and execution

Requires macOS 14 or later, Xcode command-line tools, a logged-in unlocked
desktop, and an enabled ABC keyboard layout. Use a dedicated desktop without
floating overlays that intercept input. Run only one verification invocation
per desktop; Nextest serializes the menu binary within an invocation.

Build the editor and helper from the repository root:

```sh
TMPDIR="$PWD/tmp" cargo xtask fresh-build --release
bash scripts/build-macos-gui-driver.sh
open -n "$PWD/target/gui-driver/NeomacsGuiDriver.app" --args "$PWD/tmp/gui-driver.sock"
```

Grant Accessibility and Screen Recording to this exact app, then restart it.
For explicit initial setup, append `--request-permissions` after the socket
argument. Normal test runs do not request permissions. macOS may additionally
ask to allow direct capture rather than its private window picker when the
first screenshot is taken; approve that before running unattended tests.

The helper uses bundle ID `org.neomacs.GuiDriver`. Its default ad-hoc signature
can invalidate saved grants after a rebuild. If macOS logs a code-requirement
mismatch despite an enabled switch, remove the old permission entry and add the
current app. `NEOMACS_GUI_DRIVER_SIGN_IDENTITY` selects a stable signing identity
when one is available. The tests never edit TCC records.

The socket has mode 0600 and must fit macOS's Unix socket pathname limit. Use a
shorter workspace-local socket and `NEOMACS_GUI_DRIVER_SOCKET` if necessary.
Remove a stale socket only after verifying that its owning helper has exited.
All temporary paths are under the checkout's `tmp` directory.

```sh
TMPDIR="$PWD/tmp" cargo nextest run -p neomacs-gui-tests \
  --test macos_native_menus --test interaction_geometry --test-threads 1
```

Screenshots preserve native capture resolution. The fixtures use undecorated
windows, so observed window bounds are also content bounds. Usable desktop
bounds exclude the Dock and OS menu bar. Input includes short human-like button
holds; rapid-release races are a separate contract.

## Failures diagnosed and fixed

**Window discovery:** the fixture's explicit frame title was replaced during
redisplay. Waiting six seconds did not help; setting `frame-title-format` to the
expected discovery title did. Shared fixtures now pin that format. Separately,
`frame_host_title` currently gives the formatted title precedence over its
explicit-title fallback, whereas GNU Emacs's `ns_set_name` in `src/nsfns.m`
preserves `f->title`. That production title-precedence discrepancy is not fixed
by this fixture adjustment.

**Cursor pixels:** full layout correctly resolved `cursor-type=nil` to
`NoCursor`, but incremental replay substituted `FilledBox` when retained row
cursor decoration was absent. The snapshot test captured a changing
14-by-28-pixel cursor rectangle. GNU `xdisp.c:get_window_cursor_type` explicitly
returns `NO_CURSOR` for this buffer setting. Incremental publication now uses
the same current `Option<CursorStyle>` policy as full layout. Redundant retained
style fields and their filled-box defaults were removed from cursor, scroll,
and edit replay plans. A regression test went red before the fix and now checks
both idle redraw and point movement. Pixel equality was not relaxed.

**Typing:** Rime consumed the physical `x` key as composition. The Rust session
now owns a temporary ABC selection and restores Rime afterward. The signed
Swift helper did not need rebuilding or renewed permissions for this change.

**Corner input:** a floating ChatGPT desktop-pet overlay intercepted the
bottom-right click. Hiding the overlay allowed both corner contracts to pass;
no click coordinate or menu assertion was weakened.

**Scrolling checks:** antialiased Org heading text on macOS overlapped the
orange banner detector's color range. The banner was absent in the failing
screenshots. The fixture now fixes heading foregrounds to black while retaining
its different font heights. The strict zero-banner-pixels assertion remains.

**Startup checks:** three portable startup tests incorrectly launched Weston
on macOS. They now use the native backend. The empty-Fontconfig-catalog test is
compiled only on Linux, where its fixture applies.

## Verification evidence

Local evidence is under `tmp/menu-driver/`, with copied Mac logs under
`tmp/menu-driver/macos/`. Remote logs are under
`/Users/chenyibin/neomacs/tmp/menu-verification/`.

- `cursor-hidden-red.log`: hidden-cursor regression fails before the fix.
- `cursor-hidden-green.log`: 32 related incremental-layout tests pass.
- `cursor-hidden-idle.log`: hidden cursor remains absent during idle and motion.
- `layout-all.log`: 2,382 layout tests pass; three pre-existing exclusions.
- `cursor-fixed-native.log`: both macOS menu contracts and coordinate test pass;
  Rime is selected both before and after the run.
- `portable-tests-fixed.log`: both scrolling and three startup tests pass.
- `resource-font.log`: native resource-font startup passes. The temporary global
  `Font` preference is restored immediately after that isolated test.

The native captures on yibie-et are 3024×1964 pixels, corresponding to a
1512×982-point Retina desktop. Full-suite verification distinguishes actual
macOS checks from existing X11/WPE-only test bodies that return early on macOS.

Final macOS run `5fc1567c-79f7-4a27-af3b-e4b858c18472` is recorded in
`final-macos-applicable.log`: **31 applicable checks passed**. The separately
provisioned resource-font check also passed, for **32/32 applicable checks**.
The filter excludes X11 resize capture and WPE/X11 oracle cases; the
Fontconfig-only test is compile-gated. Both native menu tests passed again in
this final run. `final-linux-driver.log` records **18/18** native Linux checks
after rebuilding with `cargo xtask fresh-build --release`. `portable-linux.log`
records another **5/5** passing scrolling and portable startup checks.

Copied full-resolution screenshots are `macos/nested-menu.png` and
`macos/bottom-right-submenu.png`. The previous keyboard source (Rime), the
original absent global `Font` preference, and ChatGPT's visibility were restored
after verification. ChatGPT was subsequently hidden again at the user's request
and remains hidden. No helper rebuild or TCC change was needed for the fixes.
