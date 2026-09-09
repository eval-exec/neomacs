# Native tooltip ownership

Status: implemented; Linux Wayland release/pdump and test verification complete,
with the bytecode-artifact test exception recorded below.

## Contract

Tooltips are non-interactive native popups, not editor overlays or menu
sessions. Explicit echo-area help remains a Lisp policy. The renderer never
decides when to show help or owns native windows.

GNU Emacs has two producers: Lisp `tooltip.el` delegates to `x-show-tip`,
while PGTK menu widgets carry their own help strings into GTK's tooltip
machinery. Preserve that distinction: menu hover must not need a round trip
through a completed command before the runtime can display help.

## Ownership and layout

- `neomacs-display-protocol/tooltip`: owned requests, styles and identities;
  no Lisp values, platform handles, or renderer objects.
- `neovm-core/display`: validate the Lisp tooltip interface and delegate
  through `DisplayHost`. Preserve terminal errors and explicit echo-area help.
- `neomacs-display-runtime/tooltips`: tooltip lifetime, replacement,
  cancellation, deadlines and native presentation; separate from menu input.
- `neomacs-display-runtime/menus`: retain item help and identify its owning
  panel. Dismiss a panel's tooltip before destroying that panel.
- `neomacs-display-runtime/presentation`: shared native surface lifetime and
  acquire/present, with an explicit interactive-menu versus passive-tooltip
  role. Native parent relationships determine stacking, never a global z index.
- `neomacs-renderer-wgpu`: target-local tooltip measurement and painting;
  no frame-clamped placement and no shared mutable target dimensions.

The originating native surface owns the anchor coordinate space. A tooltip
for a submenu belongs to that submenu, not to a guessed root-frame position.
Passive surfaces neither activate nor grab keyboard/pointer input. Unsupported
platform support must be explicit; do not silently draw an editor overlay.

## Lifetime rules

Same-target motion preserves pending deadlines and visible surface identity.
A different target replaces pending help. Leave, activation, cancellation,
owner destruction, and device loss invalidate that target before teardown.
Late requests and hide operations must be correlated to their originating
intent so they cannot resurrect or dismiss another tooltip.

Lisp owns initial/recent-help scheduling for Lisp help. Runtime menu help owns
its equivalent hover scheduling locally; the two must not schedule the same
help twice. Native timeout and drawing use the runtime event-loop deadline,
not continuous redraw or polling.

## Verification

Use cargo nextest, never cargo test. Cover the Lisp/display-host interface,
runtime lifetime interface and Linux Wayland native surfaces. Reproduce the
reported error using GUI `x-show-tip`; verify the original menu interaction,
not only a standalone tooltip. Check parent-local placement, passive input,
same-target stability, timeout, replacement, teardown and scale changes.
Other platforms cannot be claimed verified on this machine.

## Rust representation and test layout

- `Presentation::{Hidden, Ready, Mapped}` owns each lifetime's resources;
  mapped child surfaces are dropped before retained parents.
- `TooltipTicket` correlates show/hide without blocking the evaluator on a
  GUI reply. A typed generation rejects help invalidated during callbacks or
  delayed Lisp timers. Atomic visibility uses a private `strum::FromRepr` enum.
- Lisp parameter names parse through a finite `strum::EnumString` enum and
  exhaustive matching. Positive row/column limits use `NonZeroU32`.
- Menu help belongs to its item snapshot, not a parallel index-coupled vector.
  Waiting/offered hover states distinguish a pending timer from an expired tip.
- Text runs own resolved face/font data; no Lisp values cross the GUI queue.
  Measurement and painting share glyph keys and advances. Native resize/scale
  changes remeasure and reflow against the configured surface width.
- Unit tests live in sibling `*_test.rs` files selected with `#[path]` under
  `#[cfg(test)]`; core's architecture rule places its tooltip tests under
  `display/display/tests/tooltip_test.rs`. GPU integration tests live under
  the renderer's `tests/`.

Verified so far with nextest: 2,423 protocol/runtime/renderer tests, four
frontend menu tests, and both explicitly enabled Linux Wayland native menu
smokes. GPU pixel tests cover tooltip target-scale independence and unchanged
editor pixels. The callback-generation regression was red before moving the
generation capture before Lisp evaluation.

Core verification totals 9,769 passed and one artifact failure across the
initial run and a sequential retry. Rebuilding a test executable during the
first run caused `Text file busy` and invalid subsequent launches; all 4,160
non-passing tests were rerun after rebuilding, yielding 4,159 passes and only
the artifact failure below. The new tests and core test-layout rule also
passed independently (six tests).

The remaining artifact check,
`no_lisp_source_ships_without_the_bytecode_gnu_would_have_built_it`, reports
missing `lisp/loaddefs.elc` and `lisp/neomacs-effects.elc`. Do not regenerate
those files to hide the failure: this work explicitly uses `--no-byte-compile`.

Native Wayland trace: the tooltip is a child of the hovered submenu surface,
has an empty input region, and does not request a popup grab. Repeated motion
retains the same surface. Active closure drains child destruction before exit.

`cargo xtask fresh-build --profile release --no-byte-compile` completed
successfully. A release GUI probe showed styled Unicode text, repositioned the
same tooltip after one second (one native popup creation total), and returned
`t` from `x-hide-tip` after three seconds before exiting cleanly. No tooltip
error or fingerprint mismatch was logged. Batch startup printed `PDUMP-OK`.
The existing user edit to `lisp/ldefs-boot.el` is unchanged and excluded.

Logs: `/tmp/neomacs-tooltip-release.log`, `/tmp/neomacs-tooltip-gui.log`,
`/tmp/neomacs-native-tooltip-final.log`, `/tmp/neomacs-tooltip-tests.log`,
`/tmp/neomacs-tooltip-core.log`, `/tmp/neomacs-tooltip-core-resumed.log`,
and `/tmp/neomacs-tooltip-core-focused.log`.

## Review

### Standards

The review identified late generation capture after Lisp help evaluation.
Capture now precedes evaluation; the regression failed before the change and
passes after it. Earlier parallel help vectors and blocking hide handshakes
were replaced by item-owned help and ticket-qualified asynchronous dismissal.
Unit tests are out of line; the core-specific `tests/` rule is verified.

### Spec

The review identified missing device-loss invalidation and constrained-width
reflow. Recovery now invalidates queued intents before teardown; configured
native widths reflow measured glyphs. Earlier correlation, target-scale,
styled-run, explicit unsupported-coordinate and reposition findings were
addressed and covered by the checks above.

Review result: one final-round Standards finding and two final-round Spec
findings addressed; cross-platform runtime verification remains out of scope
on this Linux Wayland machine.

## Explicit limits

No in-window fallback is retained. Android/web native presentation remains
explicitly unsupported by the current adapter; macOS/Windows are unverified.
Absolute desktop tooltip coordinates are rejected rather than silently
misinterpreted as parent-local Wayland coordinates. Styled Unicode text uses
the existing glyph-atlas rendering path; this is not a claim of full Emacs
display-property, bidi, or complex-script shaping parity. Content exceeding
the configured height is clipped; tooltips are passive, not scrollable menus.

## Existing failure

The release GUI probe on 2026-09-09 reproduced
`(error "Window system frame should be used")` from `x-show-tip` on the selected
graphical frame. The registered native function ignores its evaluator context
and unconditionally signals the no-window-system error. `tooltip-show`
catches it, waits one second and publishes the help using `message`.
The old runtime `ShowTooltip` command has no production sender and paints into
the main frame, so merely wiring that command would retain clipping and the
wrong native ownership.
