# Cross-Platform GUI Test Architecture

**Date:** 2026-10-31
**Status:** Proposal
**Scope:** How `neomacs-gui-tests` runs the same observable contracts on
Linux, macOS, and Windows; how the Doom/Spacemacs config-boot tests fit;
what remains platform-specific by design.

## Goal

One GUI suite, one observation contract, and no silent platform skips. A test
states the capabilities it needs (a display, scenario-driven input, native OS
input, screen capture, a GNU oracle); each backend advertises which it has;
the harness either runs the test or reports exactly which capability is
missing. macOS is a first-class target for the scenario tier today and for
the native-input tier on hosts that can grant permissions.

## Current state (measured on 2026-10-31)

- `neomacs-gui-tests` has 68 integration-test files; 47 are
  `#![cfg(target_os = "linux")]`, 21 are not.
- `GuiBackend` already names `Macos` and `Windows`; `DisplayHarness` maps
  them to `CurrentDesktopSession`; `requested_backend()` defaults to the
  host platform when `NEOMACS_GUI_TEST_BACKEND` is unset.
- A real macOS driver exists (`interaction/macos/`): a signed Swift helper
  over a typed Unix-socket RPC with Quartz input, ScreenCaptureKit capture,
  Accessibility window fitting, and an explicit permission preflight. The
  native menu contracts already use it.
- Doom *install* compatibility already runs on macOS in CI. The Doom and
  Spacemacs GUI boot comparison (`config_env_boot.rs`) is Wayland-only.
- Linux CI runs the suite under Xvfb and Weston with a pinned GNU oracle.
  macOS CI runs core tests, Doom install, and a scratch GUI smoke workflow;
  it does not run the shared GUI suite.

## Why tests are Linux-only

The gating reasons fall into five groups, and only the last is inherently
platform-specific:

1. **Display acquisition is hard-coded** to `LinuxX11`/`LinuxWayland`,
   Xvfb, or Weston, even when the test only needs a window.
2. **Linux input tools are called directly** (`xdotool`, `wtype`) instead of
   going through `DesktopDriver`.
3. **Isolation relies on Linux mechanisms**: `/proc/<pid>/fd`, process
   groups, `XDG_RUNTIME_DIR`, X authority.
4. **Fixtures and goldens are Linux-font/pixel specific**: DejaVu/Ubuntu
   font pins, exact cell grids, pixel-exact sheets.
5. **The contract is the platform**: X11/Wayland protocol behavior, Linux
   process semantics. These should stay gated, with the reason stated.

## Design

### 1. Capabilities, not platforms

A backend advertises a capability set:

```rust
pub struct TestCapabilities {
    pub display: bool,             // a usable window-server session
    pub scenario_driver: bool,     // fixture + Elisp/control-socket driving
    pub native_input: bool,        // real OS input injection
    pub screen_capture: bool,      // real desktop pixels
    pub compositor_control: bool,  // Xvfb/Weston or an equivalent harness
    pub gnu_oracle: bool,          // a pinned GNU binary is available
    pub platform_semantics: PlatformSemantics, // X11, Wayland, AppKit, Win32
}
```

Tests declare what they need. The runner resolves that request against the
backend and returns either a run or an explicit
`Unsupported { missing: [...], backend }`. Unsupported reasons are written
to the run manifest and printed; they are never silently green. CI can
require "zero Unsupported on this platform for this filter" once the
porting work for a tier is complete.

### 2. Two drive tiers

- **Scenario tier (portable, the CI default).** The fixture drives the editor
  through the Elisp channel: `execute-kbd-macro`, `unread-command-events`,
  synthesized posn events, timers, and harness-owned control sockets (the
  `NEOMACS_GUI_SCROLL_CONTROL` pattern). It observes through the frame
  snapshot and in-process PNG readback. No Accessibility or Screen Recording
  permission is involved, and it works in any logged-in macOS session.
- **Native tier (permission-gated, serialized).** Real OS input and capture
  through `DesktopDriver`: xdotool/wtype under an owned compositor on Linux,
  the Swift helper on macOS, SendInput/UIA on Windows. Use it only when the
  contract is the native path itself: focus, menus, window-manager behavior,
  raw modifiers, IME, drag-and-drop, real pixel capture.

Prefer the scenario tier for semantics and rendering; the native tier exists
to test the parts of the platform the Elisp channel cannot represent.

### 3. One observation contract

Both tiers publish the same artifacts:

- **Semantic truth**: `neomacs--frame-snapshot` JSON/text — what redisplay
  produced, with faces, geometry, chrome, cursors. Platform-neutral, and
  available even when the window cannot be presented.
- **Paint truth**: in-process PNG readback
  (`NEOMACS_DEBUG_SURFACE_READBACK`) — the GPU painted it. Platform-neutral,
  but it needs a drawable surface: on macOS a locked console or a sleeping
  display makes AppKit report the window non-visible, and wgpu returns
  `Occluded` instead of a drawable (the Mac test host is normally locked, so
  this is a real case, not a hypothetical). The `surface-present` host
  capability names that requirement; semantic tests do not require it, paint
  tests do, and a host that lacks it reports `unsupported` rather than a
  rendering failure. A future offscreen render target would remove the
  requirement for paint proofs; it must be an explicit second proof alongside
  surface acquisition, not a fake `AcquiredSurface`.
- **Desktop truth**: native capture only in the native tier.

Assertions default to text, geometry, and resolved colors. Pixel-exact
goldens are reserved for contracts where the pixel is the point, and are
keyed by `(platform, renderer, scale, font set)` rather than shared across
platforms: CoreText and FreeType produce different antialiasing, and a
shared golden would encode one platform's rasterizer as the oracle.

### 4. Session and isolation

A `SessionPlan` describes what a test needs isolated (home, temp dirs,
runtime dirs, locale, display) and each backend implements it:

- Linux: owned Xvfb/Weston session with pinned `HOME`, XDG dirs, and
  `XAUTHORITY`; nothing the editor writes reaches the operator's home.
- macOS: the current desktop session; pin `HOME`/`TMPDIR` and any XDG
  variables for config-environment tests; run one native-tier invocation
  per desktop; never assume the editor is the frontmost app.
- Windows (future): the current desktop session, with the same pinning.

### 5. Artifact manifest

Every run records backend, OS/arch, scale, renderer/GPU, font set,
capabilities, the resolved backend, the test binary and git revision, and
any Unsupported reason. A failure on one host is reproducible from its
manifest without guessing which platform produced it.

### 6. GNU oracle availability

GNU comparisons declare the `gnu_oracle` capability. Linux CI provides the
pinned GTK3 oracle. macOS CI builds the same pinned commit, but its default
profile is `--without-x --without-ns`, a batch reference: it can answer batch
parity questions, not open a GUI frame. GUI comparisons on macOS therefore
need one of two explicit oracle profiles:

- `macos-x11` (already implemented in `scripts/ci/build-gnu-oracle.sh`):
  window-system Lisp surface without a toolkit, run under XQuartz's Xvfb.
  This is a GNU-X11-vs-Neomacs-AppKit comparison, acceptable for
  Lisp-visible state but not for window-system pixels.
- a future `macos-ns` profile (`--with-ns`) run in the desktop session,
  which compares GNU AppKit with Neomacs winit/AppKit and is the faithful
  choice for Doom/Spacemacs GUI boot parity.

Until one of those profiles is wired, macOS GUI tests that need GNU are
marked Unsupported rather than comparing against an unpinned editor.

### 7. Doom and Spacemacs config environments

The sealed config fixtures are platform-neutral (clone + `doom sync`). The
GUI boot splits into two tests with different contracts:

- **Host-backend boot (all scenario-tier backends).** Boot the config on the
  requested backend, require the fixture's finish line, and require the frame
  snapshot to contain a rendered window. This needs no GNU and no presented
  surface, so it runs on Linux X11/Wayland and on the macOS desktop session,
  including a locked console.
- **GNU differential (Linux, until a macOS GUI oracle exists).** Boot GNU and
  Neomacs in X11 sessions and compare the visible home-buffer text and frame
  geometry. GNU comes from `NEOMACS_GUI_TEST_GNU_EMACS` (the attested pinned
  oracle), never from whatever `emacs` happens to be on PATH. GNU and
  Neomacs write distinct artifact variants; sharing one artifact path made
  the comparison read the Neomacs file twice.

Isolation is the per-platform `SessionPlan`, not a Wayland assumption.

### 8. Suite governance

- A checked-in manifest enumerates every GUI integration-test target and
  the capabilities it requires. A harness contract test fails when a test
  file is missing from the manifest or silently hard-codes a backend.
- Bare `#![cfg(target_os = "linux")]` is retired in favour of capability
  declarations. The platform-specific contracts keep an explicit gate and
  a one-line reason.

## macOS specifics

- Tier 1 needs no TCC: a window in the logged-in desktop, snapshot and
  readback artifacts, fixtures that self-terminate. This is what a
  hosted macOS CI runner can execute.
- Paint proofs need an unlocked console and an awake display. The host
  probe reads `IOConsoleLocked` / `CGSSessionScreenIsLocked` from
  `ioreg`; only a positive "locked" answer removes `surface-present`, so
  an unreadable probe never skips a test by accident. Semantic tests
  (snapshot/geometry assertions) keep running on a locked host.
- Tier 2 uses the existing helper: `scripts/build-macos-gui-driver.sh`
  builds and signs it; one-time Accessibility and Screen Recording grants
  are required for that exact identity. Hosted runners cannot grant them,
  so those tests report `Unsupported { native_input, screen_capture }`
  there and run locally or on a self-hosted macOS runner.
- Window activation is not guaranteed; assertions must not depend on
  frontmost status, and tests that do need it run one at a time.
- A Unix-socket control channel must fit macOS's 104-byte `sun_path`
  (the same limit the GUI driver documents). Derive it from a short
  per-run directory, not from an artifact path below a deep checkout:
  the scrolling fixtures' `NEOMACS_GUI_SCROLL_CONTROL` path is short on
  CI but over the limit in a long local worktree.

## Migration plan

0. **Baseline (this branch).** Run the scenario-tier subset on macOS and
   record what fails for reasons other than fonts.
1. **Capability model and manifest.** Add the capability declarations, the
   Unsupported reporting, and the governance contract test.
2. **macOS scenario-tier CI.** A macOS job that builds the release runtime
   and runs the portable GUI tests with
   `NEOMACS_GUI_TEST_BACKEND=macos`.
3. **Port the shared tests.** Remove hard-coded backends and Linux tool
   calls one file at a time, starting with `real_gui_smoke`,
   `window_chrome`, the child-frame tests, and `config_env_boot`.
4. **Native tier on capable hosts.** Wire the remaining native-input tests
   to `DesktopDriver` and run them on a self-hosted macOS runner.
5. **Windows scenario tier.**

## Implementation status (2026-10-31, branch `gui-tests-cross-platform`)

- Capability contract, `Unsupported` reporting, and the manifest fields are
  in (`875510d0da`, `d7da72eb09`).
- A scenario's status no longer requires a PNG unless it declared
  `surface-present`; semantic scenarios pass on a host whose windows cannot
  be presented.
- Verified on this Mac with the console locked: five semantic scenarios pass
  (`child_frame_position`, `child_frame_sizing`, `compact_frame_geometry`,
  `display_window_geometry::pixelwise_window_resize`, `integer_width`), and
  the paint canary reports
  `backend macos lacks required capabilities: surface-present`. The
  `neomacs-gui-tests-macos` CI job runs that same set.
- Not yet verified: an actual paint readback on macOS (needs an unlocked,
  awake console), the macOS CI runner's session state, Doom/Spacemacs GUI
  boot (needs a macOS GUI GNU oracle profile), and the broader port of the
  47 Linux-gated files.

## What stays platform-specific

X11 and Wayland protocol contracts, Linux process/proc semantics, and font
fixtures that pin a Linux font package. These keep explicit gates and stay
out of the portability target.
