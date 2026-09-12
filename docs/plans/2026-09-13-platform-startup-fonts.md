# Platform startup-font policy

## Scope and source contract

Extend the Linux desktop-font fix to Cocoa and Windows without conflating a
desktop preference with an editor fallback. GNU source research is recorded in
the [Linux](../diagnostics/2026-09-12-gnu-desktop-font-settings.md),
[Cocoa](../diagnostics/2026-09-13-gnu-macos-default-font.md), and
[Windows](../diagnostics/2026-09-13-gnu-windows-default-font.md) notes.

This builds on the exact opened-font ownership contract in
[font realization](2026-07-05-font-realization-render-boundary-design.md).
Native discovery supplies requests; it does not invent renderer metrics or
independently choose a second font for a Lisp face.

## Module ownership

```text
neomacs-display-runtime/src/font_defaults/
  mod.rs       native discovery dispatch via cfg_select!
  policy.rs    owned platform defaults, ordered selection, typed candidates
  linux.rs     GSettings snapshot, no GTK initialization
  macos.rs     AppKit fixed-pitch name, native startup thread only

neomacs/src/startup_font.rs
  shared name parsing + logical sizing + native catalog opening

neomacs/src/startup_frame.rs
  prepared font/geometry ownership and one-shot native startup readiness

neomacs-display-runtime/src/render_thread/geometry_hints.rs
  complete native minimum/increment hints, with X11-specific base-size support

neovm-core/.../display_host/system_fonts.rs
  desktop preference names/roles exposed through the display host
```

Windows has no discovery adapter that fabricates a desktop editor preference.
Its ordered fallback is policy, encoded by `WindowsFontFallback` with strum
iteration. Android and Web explicitly select the portable policy; no other
desktop's discovery API is compiled for them.

`GuiFontDefaults` is an enum, not a collection of booleans or optional fields
whose combinations callers must validate. Only the desktop variant owns
`SystemFonts`. Thus Cocoa's fixed-pitch font and Windows's fallback names cannot
accidentally become values returned by the Linux-style system-font Lisp queries.
Backend dispatch is exhaustive over `GraphicalBackend`.

`GuiFontDefaults::select` owns fallback ordering and accepts the native opener.
Callers receive a typed candidate, not a platform name to interpret. Its family
matching requirement distinguishes opening an available named family from
permitting native substitution. This prevents a missing Windows candidate from
silently matching an unrelated font and stopping the fallback list early.

## Platform behavior

- Linux: effective monospace preference, then generic monospace at 10pt.
- Cocoa: `NSFont::userFixedPitchFontOfSize(-1.0)` supplies the display name,
  with GNU's trailing ` Regular` removal. The AppKit object's returned point
  size is deliberately not copied. An unspecified size uses the distinct
  `CocoaBackendDefault` policy, mapped to Core Text's documented 12pt default.
  AppKit thread checks and native objects remain inside the adapter.
- Windows: Courier New 10pt, Courier 13px, Fixedsys 12px, then unsized Fixedsys,
  preserving the GNU fallback order and size units through the shared parser.
- Portable: generic monospace 10pt.

Named-font defaults, Cocoa's backend default, and the portable monospace default
are distinct enum values. Positive point/pixel conversion uses existing
`FrameFontSize` and `FontSizing`; no native zero-size sentinel enters the opened
font representation. Device/backing scale is not part of these point requests.

Explicit Lisp/default-face changes remain authoritative after startup. The
policy is evaluated only during bootstrap, never during redisplay. It does not
reset fonts in response to frame redraws.

## Deliberate limits and follow-ups

- Keep Neomacs's generic last resort if all named platform candidates fail;
  GNU Windows instead errors when its entire fallback list fails. In
  particular, DirectWrite need not expose legacy GDI bitmap families such as
  Fixedsys. This is a documented Neomacs policy, not claimed full GNU parity.
- This does not implement GNU Windows's Emacs registry-resource lookup or
  Cocoa's resource database. Existing explicit Lisp configuration remains in
  its existing startup path.
- Live Linux desktop setting changes still need a typed event/subscription
  path, separate from installed-font catalog changes. Discovery refresh and
  the `font-use-system-font` opt-in must remain separate responsibilities.
- Initial geometry now follows evaluator-side font opening through the
  readiness protocol below. Explicit font changes made later by Lisp remain
  in the existing frame-update path; this is not a rewrite of every startup
  resource or user-init precedence rule.
- No native macOS or Windows GUI verification is possible on this machine.
  A source-level adapter check is not a native launch or a full cross-build.

## Verification

The pre-agreed public default-face/bootstrap test seam has a Cocoa sizing
regression: with a Cocoa observation, the previous code opened 10px and the
new policy opens 12px / face height 120. The failing run is captured in
`target/diagnostics/issue-360/startup-fonts/cocoa-red.log`; both that regression
and the existing Linux bootstrap realization test pass after the change.

The 74 focused core font/remapping tests pass after rebasing onto remote main.
The actual AppKit adapter and its actual `SystemFonts` transport source compile
for `aarch64-apple-darwin` in an isolated dependency harness. This found a
required `objc2-core-foundation` feature gate, now explicitly enabled.

Full runtime cross-checks did not reach Neomacs source: macOS's C dependencies
encountered Linux headers during cross compilation; Windows's C dependencies
received Linux `-fPIC`, rejected for MSVC. These checks are not reported as
passing. The source harness, commands and logs are under
`target/diagnostics/issue-360/`.

A fake-selector seam was not added. The subsequent platform coverage instead
uses the approved public Lisp startup seam with process-local Fontconfig
catalogs (see below).

The Linux release build succeeded after the rebase, and a matching pdump was
regenerated without byte compilation or autoload regeneration. The tracked
Wayland desktop-font/SVG regression passes against that binary. The two-editor
desktop-preference reproduction again reports identical window font metrics
and identical SVG source. Logs are `platform-release-build.log`,
`platform-pdump.log`, `platform-gui-green.log`, and `platform-desktop.log` in
`target/diagnostics/issue-360/startup-fonts/`. `lisp/ldefs-boot.el` remains unchanged.

## Font-owned initial geometry (2026-09-13 follow-up)

The previous native-thread bootstrap measured generic monospace, while the
evaluator later selected the desktop font. Under the controlled Ubuntu Mono
13pt preference, GNU reported `(frame-width) = 80` but Neomacs reported 70.
The tracked desktop-font GUI fixture now checks this public observable; the
failure is captured in `geometry-red.log` and the independent GNU observation
in `geometry-gnu.log`. GNU source selects the font before
`gui_figure_window_size` (`nsfns.m:1322/1470`, `w32fns.c:6427`).

The native thread captures platform facts, starts the evaluator, and waits for
a one-shot readiness reply before creating the primary native window or GPU
resources. The evaluator initializes its TLS and preloaded Lisp, then opens
the initial font. `PreparedGuiFrame` privately owns that opened font, its
display configuration, and dimensions calculated from its metrics. Consuming
installation seeds the logical frame using the same font; it cannot silently
select another one. Only geometry crosses the readiness channel.

`BootstrapFont::{Gui(StartupFont), Tty}` replaces the ambiguous optional GUI
font. Graphical bootstrap requires an opened font; TTY bootstrap uses constant
cell metrics without scanning a native catalog. The old separate default-font
name/opening helpers were removed, and their name-format assertion now queries
the actual bootstrapped Lisp frame rather than a test-only parallel selector.

`StartupFrameReply` is consumed by either success or a typed failure. Unwinding
before readiness drops its sender, wakes the native receiver, and lets the
native thread join and propagate the original evaluator panic. The
`startup_failure` GUI integration test exercises this through an explicitly
missing pdump. It passed against the prior binary before the readiness change.

Preparation must stay independent of event-loop replies: install the display
host, perform monitor rendezvous, and run user startup only after publishing
readiness. This is a pre-window rendezvous, not a wait inserted into a running
GUI loop. It also removes the native thread's redundant font-catalog scan.
The owned preparation value can later travel through a nonblocking native
startup state machine without moving Lisp values or native font handles across
threads; that larger event-loop lifecycle change is not required here.

The initial focused bootstrap run had 24 passes and one previously observed
failure: `bootstrap_batch_startup_error_exits_nonzero_like_gnu` failed its
`*Messages*` assertion. The subsequent GNU investigation below found that
assertion was incorrect, rather than a font/geometry regression.

The first geometry rerun exposed a second, native-side mismatch: 745px was
requested using the correct font, then winit snapped it down to 740px (79 text
columns). Neomacs sent resize increments but left winit's default minimum in
place. Wayland winit uses that minimum as the increment origin. GNU's
`gtkutil.c:xg_wm_set_size_hint` supplies minimum and increments together, with the
minimum equal to the base size. The renamed `geometry_hints` module now applies
both through winit; only the additional X11 base-size properties use Xlib.
The existing typed `GuiFrameGeometryHints` remains the shared transport.

A process-isolated Fontconfig fixture with no installed faces also exercises
startup failure through the real GUI entry point. Before the fix, Cosmic's
buffer constructor panicked while shaping a default line. The font selector
now returns its existing no-match result before constructing that buffer when
the native catalog is empty. The GUI startup converts this into
`StartupFrameError::NoUsableFont` and exits without creating a primary window.
This test changes neither the user's font configuration nor installed fonts.

Final verification of the geometry/error-path follow-up:

- `cargo build -p neomacs --release`: passed (6m17s), followed by fingerprint
  sealing and pdump regeneration without byte compilation or autoload changes.
- `cargo nextest`: all 3 tracked Wayland GUI tests passed, including the
  80-column/SVG regression and both startup failure controls.
- `cargo nextest`: all 107 layout font-metrics tests passed; all 24 selected
  bootstrap tests passed. The separate broader run's known batch `*Messages*`
  failure remains as described above.
- GNU/Neomacs desktop-preference, explicit-font override, and no-schema
  fallback controls all matched in unremapped window metrics and SVG source.
- `cargo fmt --all` and `git diff --check` completed; `lisp/ldefs-boot.el` has
  unchanged SHA-256 `094b50fbd69032559bcac655d6368e16b8e69ae70b53989a99576ecd5c325138`.

Logs under `target/diagnostics/issue-360/startup-fonts/`:
`geometry-release-build2.log`, `geometry-pdump2.log`,
`geometry-gui-green2.log`, `geometry-font-metrics-tests.log`,
`geometry-bootstrap-green.log`, and `geometry-{desktop,explicit,fallback}.log`.
The rebuilt executable/pdump fingerprint is
`545C52570CE82336FD1326B9317168D5E29C0D20286400BDBDFDDBF082EA8DA6`.
Native macOS/Windows runtime validation remains unavailable; the GUI evidence
above is Linux Wayland at 96 logical DPI.

## Follow-up: native resize completion and platform coverage

The startup refactor was committed as `3da868a97` before these follow-ups.
The GUI fixture now also requests 91 columns and checks the settled result.
It runs on both the standard output and an actual 3840x2160, scale-2 Weston
output. Before the completion fix both runs remained at 80 columns
(`hidpi-red.log`); the high-DPI log confirmed native scale factor 2.

The local winit source contract (`winit-core/src/window.rs:request_surface_size`)
explicitly permits immediate completion without a later resize event.
Wayland's implementation returns `Some(applied_size)`. Neomacs discarded that
return value, leaving GPU/surface and evaluator geometry at the previous size.
GNU PGTK delegates requests through `xg_frame_set_char_size` and GTK allocation;
its logical dimensions follow applied native geometry, not merely the request.

`render_thread/surface_resize.rs` now owns a single completion path for native
events and immediate replies. `ResizeRequestOutcome` distinguishes `Applied`,
`AwaitingConfigure`, and `PendingRealization`, and is marked `must_use`.
The completion uses the returned physical size, even when that is an old size
because the platform rejected a request. It updates the surface, renderer, and
evaluator notification through the existing coordinate conversion. Requested
dimensions cannot be mistaken for a confirmed native result.

`platform_startup_test.rs` runs public Lisp startup in isolated child processes
against native Fontconfig catalogs with controlled family availability. Five
cases cover Courier New priority, missing-Courier-New advancement, Fixedsys's
12px fallback, Neomacs's generic last resort, and Cocoa's named fixed-pitch
12pt policy. Each catalog copies a locally available outline face into its
own workspace artifact directory and assigns native scan family names. No
production environment override or fake internal selector is introduced.
These verify shared policy, not actual AppKit/DirectWrite behavior, and do not
force every possible driver-opening failure (such as reaching unsized Fixedsys
after a same-family pixel-size failure).

The batch failure was an incorrect test expectation. GNU
`keyboard.c:command-error-default-function` prints noninteractive errors to
`external-debugging-output`, emits a newline, then calls `kill-emacs -1`.
Independent GNU runs with a shutdown hook showed `boom` on stderr and no
`boom` entry in `*Messages*`, including with noninteractive backtraces disabled.
The existing assertion now checks absence from the echo area/message log;
`tests/batch_startup.rs` separately checks the real executable's stderr and
exit status 255. No production error-reporting behavior was changed.

Follow-up verification completed:

- Release build and no-byte-compile pdump regeneration passed. Matching
  fingerprint: `95DF7D65A634705CEB537960E37A62FE7E1DBFFB766BF9740364623AE87FB25E`.
- All 4 GUI regressions passed, including 80→91 columns at both 1× and 4K/2×.
  The 2× trace confirms the applied resize returns 844 logical pixels with
  native scale factor 2; it no longer remains at the old 745-pixel width.
- All 30 selected bootstrap/platform tests passed, including the corrected
  batch assertion; all 19 runtime resize tests and 16 GUI harness tests passed.
- The real batch-process stderr/exit-status test passed.
- Formatting and diff checks passed. `lisp/ldefs-boot.el` remains unchanged.

Logs: `hidpi-release-build.log`, `hidpi-pdump.log`, `hidpi-gui-green.log`,
`followup-bootstrap-all.log`, `hidpi-runtime-tests.log`,
`hidpi-harness-tests.log`, and `batch-process-tests2.log` in the same diagnostic
directory. GNU batch controls are `batch-gnu{,-no-backtrace}.{stdout,stderr}.log`.
Native macOS/Windows execution is still not available on this machine.
