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
  linux.rs     owned GSettings discovery/subscription thread, no GTK initialization
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

The initial selection policy runs only during bootstrap, never during
redisplay. Explicit Lisp/default-face changes remain in effect unless the user
opts into GNU's dynamic desktop-font policy with `font-use-system-font`.
Redraws do not reapply desktop preferences.

## Deliberate limits and follow-ups

- Keep Neomacs's generic last resort if all named platform candidates fail;
  GNU Windows instead errors when its entire fallback list fails. In
  particular, DirectWrite need not expose legacy GDI bitmap families such as
  Fixedsys. This is a documented Neomacs policy, not claimed full GNU parity.
- This does not implement GNU Windows's Emacs registry-resource lookup or
  Cocoa's resource database. Existing explicit Lisp configuration remains in
  its existing startup path.
- Linux live changes use the owned subscription described below, separate
  from installed-font catalog changes. Other native subscription adapters,
  GConf, and XSettings remain unsupported.
- Initial geometry now follows evaluator-side font opening through the
  readiness protocol below. Explicit font changes made later by Lisp remain
  in the existing frame-update path; this is not a rewrite of every startup
  resource or user-init precedence rule.
- No native macOS or Windows GUI verification is possible on this machine.
  A source-level adapter check is not a native launch or a full cross-build.

## Verification

### Live Linux preferences (continuation)

`observe_font_defaults` returns a `FontDefaultsObserver` with initial facts,
an owned preference receiver, and native subscription ownership. On Linux,
startup discovery and monitoring construct GSettings on the same private GIO
context: the default backend's file monitors also bind to their initial
construction context. The callback connects before watched keys are read.
It rereads preferences and suppresses identical snapshots. It supplies no
rendering metrics. AppKit startup discovery remains on the main thread;
unsupported subscriptions expose an inactive receiver explicitly.

The Linux display adapter enables core's `desktop-font-settings` build
capability. Its existing feature table then advertises `dynamic-setting` and
`system-font-setting`, allowing `loadup.el` to load the Lisp handler and Custom
to expose `font-use-system-font`. A bare evaluator does not enable this
capability. The receiver is transferred once to its input consumer.

The evaluator owns the observer through startup and its command loop. Its
input bridge transports `SystemFontsChanged` with the graphical display
identity through the existing input channel and wakeup mechanism. Servicing
that event first updates the display host's public system-font facts, even
when adoption is disabled. Changed roles then become the existing Lisp
`config-changed-event` forms, with the monospace event gated by
`font-use-system-font`. `dynamic-setting.el` checks the option again and uses
the existing `set-frame-font`/font-realization path. Application preference
changes do not choose document fonts.

Normal exit, startup failure, and unwinding release subscription ownership.
Shutdown sets an atomic stop flag, wakes the private context and joins the
worker; Settings and signal handlers are destroyed on that worker. This
stops subscription callbacks. GLib retains its default settings backend as
process-global state, so it is not a claim that all GLib monitors are freed.
No production timer, compositor delay, or new font measurement is involved.

The public native-settings/Lisp GUI seam was approved in the handoff. Tests
use separate `gsettings` processes with a private keyfile backend and config
root. GNU at 96 DPI supplies the expected realized metrics. The first
Neomacs red run, `live-red.log`, retained `Ubuntu Mono 13` in both its query
and frame after the writer changed the preference to `DejaVu Sans Mono 16`.
GNU passed with a 13×25 cell and default-face height 158. Further control
slices and final verification are recorded as they complete.

The first native subscription build refreshed the query but did not change
the face (`live-adoption-diagnostic.log`): its feature table still declared
`dynamic-setting` unavailable, leaving the special-event handler unbound.
A temporary explicit load made the same GUI regression pass
(`live-adoption-explicit-load.log`). That diagnostic load was removed; the
build capability above makes normal image construction load the handler.

The canonical startup regression now passes in both GNU X11 and Neomacs
Wayland, with no explicit handler load (`live-feature-gui.log`: 2 passed,
0 skipped). The existing GUI selection also passes unchanged
(`live-feature-baseline-gui.log`: 11 passed, 0 skipped). The full user-requested
`cargo xtask fresh-build --release` completed with matching executable/image
fingerprint `D0F966B8EF4F6317B99F914E90AC4EC695DD4ADDAE79D65B3890C99A35B32173`.
`lisp/ldefs-boot.el` retains its recorded SHA-256. The earlier no-byte-compile
build left stale generated bytecode; those test refusals were resolved by
the full build, without weakening the freshness check. Bare-core feature
coverage passes separately (11 passed), so Cargo feature unification cannot
hide accidentally advertising a native adapter in an evaluator-only build.
The combined runtime/core/layout selection passed 1,556 of 1,557 tests; its
one failure was the previous unconditional feature-absence expectation.
After correcting that expectation for linked native adapters, all eight
coupled-variable tests passed (`live-coupled-vars-green.log`). Standards and
spec reviews found no remaining first-slice defects; receiver ownership was
tightened to a single transfer after review. Remaining behavior controls are
still separate work below, not inferred from this opt-in result.

The opt-out control now passes in both engines (`live-opt-out.log`: 4 live
tests passed, 0 skipped). It changes the real isolated monospace preference,
waits for the public query, and checks unchanged face attributes, font-object
identity and frame geometry. It then enables adoption and changes only the
application preference. That query refreshes without replaying the skipped
monospace update or changing the document font. No additional production
change was needed for these controls.

The explicit/current/future-frame oracle also passes in both engines
(`live-explicit-future.log`: 6 live tests passed, 0 skipped). Two existing
frames start with different explicit fonts; an enabled desktop update replaces
both. An ordinary newly created frame inherits the updated default, while a
new frame with its own explicit font keeps that request. The existing GNU
Lisp/Custom path already implements this precedence; no Rust policy override
was added.

The stronger geometry oracle exposed an existing missing implied resize:
after adopting the new 13×25 font, Neomacs still presented its old 745×432
allocation (`live-geometry-red-text-grid.log`). GNU retained an 80×24 text
grid by growing its window. The fixture uses `frame-text-lines` and exact
text pixels to measure this grid; the existing `frame-height` parameter can
undercount a grown minibuffer during setup. Menu/tool bars are disabled by
their public mode commands to keep this oracle independent of toolkit chrome.

The font-realization entry point now captures the existing text grid (or a
pending requested grid), installs the opened font, and sends the implied
resize through the existing frame resize interface. Native acknowledgement
still owns the allocation. Each axis respects `frame-inhibit-implied-resize`
and fullscreen state; child frames use the existing local resize fallback.
New-frame construction keeps its separate metrics-only entry point. No
preference adapter or renderer measures a second font. A Wayland pass requires
a later native Presented receipt, matching dimensions and carrying the
compositor timestamp; font attributes and requested frame parameters alone
cannot satisfy it. Full `xtask fresh-build --release` completed with matching
fingerprint `6427BEDA02BB31C4A20F87FAA89B797CE72368E66AE4509F7BB9284294CE8E4F`.
All eight live GUI/GNU checks pass (`live-geometry-gui.log`), including native
presentation at 1069×600 with an 80×24 text grid. The generated bootstrap Lisp
hash is unchanged.

The baseline selection passed nine tests while two concurrent Weston sessions
lost their peer connections, without a Lisp font/geometry assertion or a
logged Weston protocol error (`live-geometry-baseline-gui.log`). Their raw
artifacts are retained under `geometry-baseline-peer-reset/`. Both pass when
rerun separately (`live-geometry-baseline-isolated.log`). This is recorded as
an unexplained concurrent compositor failure, not silently relabeled a code
fix or a proven resource-exhaustion diagnosis.

The focused geometry selection passed 543 of 544 tests. The sole failure
expected no implied resize after a font change, which is the behavior this
slice intentionally adds. Its explicit-followup/deferred-ack assertions remain
intact, and the adjusted test passes (`live-geometry-deferred-green.log`).
Review confirmed the opened-font/native-resize boundary and removed an
incorrect blanket child-frame exclusion. This work does not establish full
GNU `adjust_frame_size` parity for split-window minimum-size overrides or
overlapping stale native acknowledgements; those are wider existing resize
constraints, not verified by the sequential live-font oracle.

See [subscription research](../diagnostics/2026-09-13-live-desktop-font-settings.md)
and the [completed post-rebase checkpoint](../diagnostics/2026-09-13-post-rebase-font-verification.md).

### Earlier startup verification

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
It runs on both the standard output and a high-resolution scale-2 Weston
output. (The original preset supplied 3840x2160 logical dimensions, producing
7680x4320 physical pixels; the follow-up below corrects the 4K preset.)
Before the completion fix both runs remained at 80 columns
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
- All 4 GUI regressions passed, including 80→91 columns at both 1× and 2×.
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

## Follow-up: resize intent and restoration

The completion/platform coverage above was committed as `cdd992960`.

The stronger GUI assertion exposed a width-only resize changing native height
from 688 to 612 pixels. GNU `frame.c:Fset_frame_width` passes the existing
`FRAME_TEXT_HEIGHT` unchanged to `adjust_frame_size`; its tracked GUI oracle
preserves 720 pixels when requesting 91 columns. Neomacs instead reconstructed
the unchanged height from rounded character rows and subtracted chrome.
`FrameResizeRequest::TextWidth` now expresses a width-only intent and preserves
the exact current text height, including partial rows. Public Lisp/core tests
and the real desktop-font fixture cover the unchanged-height contract.

A real fullscreen/rejected-resize/restoration cycle then exposed a second
issue: Lisp `fullscreen = nil` never reached the display host. GNU
`frame.c:gui_set_fullscreen` maps nil to `FULLSCREEN_NONE` and invokes the
native fullscreen hook. `FrameFullscreen::Windowed` now represents this
explicit transition, distinct from an absent or unrecognized request; the
exhaustive renderer mapping handles it. The GUI failure before this fix is
recorded in `increments-undecorated-red.log`: restoration remained at the
fullscreen width, 3840 pixels / 423 columns, instead of 844 / 91.

The approved native-host test seam also covers rejected requests followed by
duplicate applied-size notifications: public frame geometry remains the actual
size, and the next width-only request retains that actual height. Tests live
in `window_cmds/tests/frame_resize_test.rs`, not temporary directories.

The Weston 2× preset now uses 1920x1080 logical dimensions, producing a true
3840x2160 output; `HiDpi8k` preserves the original 7680x4320 environment under
an accurate name. The resize fixture normally disables decorations to isolate
content geometry. `native-resize-decorations.el` retains the decorated control
at both resolutions: an early run failed entering fullscreen with a Wayland
buffer-scale error on a client-side decoration subsurface, not the main GPU
surface. Its root cause has not yet been established. This error did not recur
in the first rebuilt 4K decorated run; it must not be inferred resolved merely
from an undecorated run.

With width preservation and fullscreen clearing fixed, both decorated and
undecorated 4K runs restore 90 columns / 842 pixels rather than the requested
91 / 844 (`resize-gui-first-build.log`). The resize hints had been labeled
physical even on a logical-coordinate backend, halving the minimum and
increments at 2×. Winit snaps configure sizes against that minimum/increment
grid. GNU `gtkutil.c:xg_wm_set_size_hint` likewise treats toolkit hint units
explicitly, converting its frame base/increments for the toolkit scale.

`geometry_hints.rs` now uses `window_size_from_emacs_pixels`, exactly as the
native resize request does. The existing typed coordinate policy selects
logical sizes for the logical-coordinate backends and physical sizes for X11.
There is no second platform/DPI policy in the hint adapter, and the X11 native
base-size hint remains physical. This change is shared across platforms;
only Linux Wayland has been exercised natively here.

### Remaining 8K decoration failure

**Diagnosis update:** the standalone protocol replay now isolates this to
Weston 15's cached-subsurface detach validation, and succeeds unchanged on the
current KDE Wayland desktop. See the
[reproduction and source analysis](../diagnostics/2026-09-13-weston-subsurface-scale-detach.md).
The original observations below are retained as investigation history.

The dedicated 8K decorated control reproduced the protocol error again with
the rebuilt width/fullscreen fixes (`resize-8k-before-hints.log`). Weston is
15.0.0. The error identifies `wl_surface@24`, a CSD subsurface, and dimensions
833x44 at scale 2. The earlier protocol trace showed a valid scale-2 decoration
buffer followed by an explicit null-buffer attach while entering fullscreen.

[Weston's `surface_commit`](https://cgit.freedesktop.org/wayland/weston/tree/libweston/compositor.c)
checks the previous surface dimensions against pending scale when no pending
buffer exists. This makes cached-subsurface/null-attach validation a candidate,
not a confirmed diagnosis. The next isolation should replay that protocol
sequence without Neomacs/wgpu, then distinguish compositor validation from CSD
lifecycle and delayed parent commits. No winit, sctk-adwaita, or compositor
patch has been applied on the basis of this hypothesis.

### Regression coverage

- `resize-window-suite.log`: all 328 core window-command tests passed,
  including the three new public-Lisp/native-host regressions.
- `resize-hints-runtime-tests.log`: all 27 selected runtime resize/scale tests
  passed; `resize-harness-tests.log`: all 16 harness tests passed.
- `resize-adoption-tests.log`: both main-binary frame-adoption tests passed.
- `width-height-gnu-final.log`: the real GNU GUI width-only oracle passed.
- `resize-gui-first-build.log`: width preservation passed at 1× and 4K/2×;
  both 4K fullscreen controls reached restoration and exposed the hint-unit
  bug before its fix (844 pixels becoming 842).

Final release verification (`resize-hints-release-build.log`,
`resize-hints-pdump.log`, `resize-gui-final.log`): the release build and matching
pdump succeeded, without byte compilation or autoload regeneration. The
fingerprint is
`9F24DFCF91DC6CB0EBFF175F15997D5C1452F72209ED9DADC71D30293267FFCE`.
Six of seven native GUI tests passed, including both startup-failure controls,
width preservation at 1×/4K2×, and both decorated/undecorated 4K fullscreen
cycles. Both cycles retained fullscreen size for duplicate rejected requests
and restored exactly 91 columns / 844 pixels. The 8K decorated regression
still fails with the CSD protocol error above; it is explicitly marked as a
known failure, not reported green. Formatting/diff checks passed and
`lisp/ldefs-boot.el` is unchanged.
