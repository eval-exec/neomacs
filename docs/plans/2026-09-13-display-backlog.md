# Remaining display work

The user authorized all four remaining handoff tasks and the additional known
gaps on 2026-09-13, after startup commit `084aaec74` was pushed. This record
tracks the full scope across implementation slices; an unchecked item is not
a claim of completion.

## Work and evidence

- [x] Submission terminology: popup state, video surface evidence, internal
  frame counters; retain existing exported diagnostic/log fields.
- [ ] Platform validation: portable compile/policy checks and actual macOS and
  Windows execution using repository CI runners; strengthen native GUI controls.
- [ ] GNU platform resources: Windows registry precedence and Cocoa user
  defaults, including initial font policy and public resource lookup.
- [ ] Weston cached subsurface scale/detach diagnostic: reproduce on stock
  Weston, explain against official protocol/source, correct Neomacs only if
  the evidence identifies a client contract violation. Investigate independent
  unexplained peer resets separately.
- [ ] Asynchronous initial GPU adapter/device acquisition: native dispatch must
  remain responsive while acquisition is pending; preserve main-thread window
  ownership and failure propagation.
- [ ] macOS/Windows live-font behavior: establish GNU/platform capabilities.
  A user question is pending because GNU has no Linux-style system-monospace
  subscription on these backends; a Neomacs-specific preference would be a
  separate interface decision, not inferred from nonexistent OS settings.
- [ ] Grown-minibuffer frame-height accounting: GNU differential regression and
  correction through public Lisp/native-host geometry observations.
- [ ] Child-frame initial width with chrome: reproduce without suppressing
  chrome, compare GNU, correct initial geometry ownership.
- [ ] GNU split-window minimum-size overrides: pin the actual font-change
  contract and retain native presented geometry evidence.
- [ ] Overlapping resize acknowledgements: exercise multiple outstanding
  requests and stale native observations without losing newest intent.

## Working constraints

Use the supplied GNU checkout read-only. Tests use the already approved public
GUI/process, Lisp, and native-host interfaces; Rust tests stay in tracked test
files. Use cargo nextest, not cargo test. All Neomacs rebuilds use
`cargo xtask fresh-build`, including matching generated runtime artifacts.
Desktop-setting changes are confined to test-local settings or ephemeral CI.
Use stock compositors, without production delays, patched Weston, or invented
native presentation confirmations. Preserve exported log compatibility.
Commit independently reviewable slices and perform final integration checks.

## Investigations underway

The research skill runs independent primary-source investigations for native
resources/settings and the stock Weston diagnostic. Existing macOS fresh-build
workflow has been dispatched for a baseline. Current CI also has Windows
runners; native GUI validation will use an explicit display contract rather
than treating an installer or a cross-compile as display verification.
