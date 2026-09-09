# Native window chrome — issue #365

Baseline: `4ef24a04a63e3ffba11c4af764e450a2aaf534f2`.

## Accepted scope

Extend the macOS frame background beneath the native titlebar while retaining
native controls. Use a shared primary/secondary native-window path; separate
native window chrome from editor menu/tool/tab bars and native popup ownership.
Keep Linux, Windows and other existing backends on their existing decoration
behavior. No new GUI framework, video/webview defaults, or winit fork changes.

## Interface and invariants

- `window_chrome` protocol: enums for native titlebar style, decoration policy,
  and supported native capabilities. Unsupported style requests are distinct
  from success. This change selects platform defaults; it does not add new Lisp
  customization variables or implement the previously inert `ns-appearance`
  frame parameter.
- Runtime controller: per-window acknowledged appearance; only a successful
  native application updates it. External style changes invalidate it.
- macOS adapter: `cfg_select!` dispatch, public typed objc2/AppKit calls, checked
  main-thread access and a narrowly documented borrowed-handle unsafe seam.
- `DrawableSurface`: physical native content insets, clamped against observed
  dimensions. A fully obscured content viewport is not drawable.
- `PresentMapping`: preserve logical content size; translate between editor and
  native surface; clip stale content rather than stretching it.
- Draw the editor, child frames, overlays and effects into an unobscured content
  target, then place that target into the native surface without scaling or
  another alpha blend. Paint the remaining native area with the same resolved
  frame background. Only overlay chrome needs the additional pooled texture.
- Pointer conversion, editor-bar hit testing, popup anchors and IME placement
  must agree with this transform. Native titlebar controls are not editor input.
- Resize/scale/fullscreen observations update viewport geometry. GPU recovery
  drops content-target leases with the existing per-frame GPU state.

## Tests and verification

Use `cargo nextest`, with unit tests in `*_test.rs` and integration tests under
`tests/`. Protocol tests cover mapping, fractional scale, clipping and unsupported
policies. Runtime tests cover native application retry/reset and frame isolation.
The renderer integration test reads actual GPU pixels to verify the reserved
background and unchanged translucent editor pixels. Native GUI acceptance must
cover traffic lights, titlebar dragging, focus, theme changes, multiple frames,
fullscreen and Retina scaling.

The available machine is Linux Wayland, not macOS. Linux checks do not establish
AppKit compilation or native visual correctness. Record those limitations; do not
report a macOS reproduction or successful native acceptance based on Linux tests.

References: [source-backed research](../research/2026-09-09-macos-titlebar-rust-crates.md).

## Linux verification record

- `cargo fmt --all`, then `cargo fmt --all -- --check`: passed. The requested
  workspace formatting also reordered two module declarations in
  `neovm-core/src/emacs_core/display/terminal/mod.rs`.
- Default-feature protocol/runtime/renderer nextest run: 2,448 passed, 5 skipped.
- The same three crates with `neomacs-display-runtime/webview`: 2,459 passed,
  5 skipped. This is an explicit test feature, not a default-feature change.
- The native-content GPU test ran on Intel Iris Xe / Vulkan. It verified the
  reserved native background and unchanged premultiplied-alpha content pixels.
- Additional regressions cover editor menu hits beneath native chrome,
  suspension surviving a scale refresh, and native WebView placement/cache
  invalidation when only the content inset changes. The WebView test first
  failed with native y=16 instead of y=44, then passed after mapping integration.
- Standards review findings (responder preservation, retry tests, repeated
  geometry queries, checked placement construction) were addressed. Spec review
  findings (input mapping, fullscreen safe area, suspension and native WebViews)
  were addressed; the focused follow-up found no further serious issue.
- A broader GUI-crate run against the pre-rebuild executable had four GNU
  font/image oracle failures, including a GNU timeout. These are recorded
  separately from the passing affected-crate suites, not claimed as fixed.

Detailed local logs: `/tmp/neomacs-365-regression-final.log`,
`/tmp/neomacs-365-verified-suite.log`, `/tmp/neomacs-365-gpu.log`, and
`/tmp/neomacs-365-suite-final2.log`.

### Runnable release verification

- `cargo build -p neomacs --release`: passed with unchanged default features.
- `fresh-build --skip-build --no-byte-compile` refused that non-video developer
  executable because the Linux packaging contract requires GStreamer. The
  packaging contract was not changed. Instead, the executable was fingerprinted
  using xtask's exact normalized-SHA256 record procedure and copied to its two
  build-role names; `neomacs-temacs --batch -l loadup --temacs=pbootstrap` and
  `--temacs=pdump` regenerated matching dumps using the existing Lisp sources.
  No byte compilation or Lisp source generation was performed in this step.
- The rebuilt `neomacs --batch -Q --eval` startup check passed.
- `NEOMACS_GUI_TEST_BACKEND=wayland cargo nextest run -p neomacs-gui-tests
  --test window_chrome --run-ignored all`: passed under headless Weston. Both
  frames remained alive across the primary theme change, with surface PNG
  readback. This is a Linux regression smoke, not AppKit visual acceptance.
- The pre-existing `lisp/ldefs-boot.el` edits remained byte-for-byte unchanged
  and are excluded from this implementation commit.

Logs: `/tmp/neomacs-365-release-final.log`,
`/tmp/neomacs-365-direct-bootstrap.log`, `/tmp/neomacs-365-direct-pdump.log`,
`/tmp/neomacs-365-startup.log`, `/tmp/neomacs-365-wayland-gui.log`.
