# Popup lifetime and post-event commit

## Contract

Native window lifetime is separate from menu/tooltip intent. Input callbacks
may detach a popup immediately from event routing, but retain its native/GPU
resources until the event batch is complete. Creation, repositioning, and
ordinary retirement occur during the real `about_to_wait` pass. Proxy wakeups
only wake the loop; they do not run that pass recursively.

`render_thread/lifecycle.rs` owns the private construction of `PopupCommit`.
Native creation and normal retirement require a borrowed capability. Menu and
tooltip event handlers cannot construct it from an `ActiveEventLoop`.
Native compositor tests have an explicitly test-only constructor, including
an adversarial test which bypasses application scheduling to test winit itself.

## Ownership and layout

- `presentation/host.rs`: active parent/submenu chain and detached native
  surfaces. Children retain their exact native parent; the host never exposes
  owned drawable images to callers.
- `presentation/retirement.rs`: child-first deferred resource release, with
  out-of-line resource-lifetime tests in `retirement_test.rs`.
- `menus/controller.rs`: logical menu session, panel reconciliation and
  item help. Its tooltip children retire before their menu parents.
- `tooltips.rs`: `Hidden`, `Ready`, and `Mapped` states; mapped resources move
  to retirement on hide/replacement. Visibility tickets invalidate immediately.
  Same-content repositioning is pending intent until commit.
- `render_thread/lifecycle.rs`: post-event capability issuance, frame-owner
  teardown ordering, device-loss teardown, and shutdown.

Detached surfaces are no longer addressable for input or painting. A canceled
intent cannot be revived by late native events. Multiple replacements before
commit coalesce to the latest request. Native stacking still follows popup
roles and parent relationships, not a numeric cross-window z-index.

Shutdown and device loss explicitly release all popup resources before the
renderer/device/frame resources. They cannot wait for a later event batch.
Pending frame destruction cancels and retires descendants before dropping the
frame's native/GPU state. No in-window fallback is introduced.

## Backend responsibility

Application scheduling does not excuse a backend lifetime panic. The winit
fork separately removes retired-window compositor/synthetic/input events before
`Destroyed` and filters late dispatch for absent windows. The fork starts at
the existing upstream pin, so no unrelated upstream changes are bundled.
Neomacs will use an immutable fork revision, not a local path dependency.
Per user direction, no upstream PR is submitted.

## Verification

Use `cargo nextest`, not `cargo test`. Native tests require Linux Wayland and
a GPU; other platforms are not runtime-verified on this machine.

- Queue retirement preserves live-window/device event ordering in winit.
- The GPU callback-replacement test exercises the original backend panic even
  though production reconciliation has moved out of event callbacks.
- The deferred-replacement test exercises Neomacs's normal commit policy.
- Stable menu and passive menu-tooltip tests verify surface identity and
  child-before-parent cleanup; their synthetic motion is not full manual grab QA.
- Protocol/runtime/renderer suites and the frontend suite run
  under nextest, followed by release plus matching pdump:
  `cargo xtask fresh-build --profile release --no-byte-compile`.

Final results and fork revision are recorded in the diagnostic note.

## Review

Standards: no hard violations found. The capability-construction and ordinary
retirement caveats were addressed by private lifecycle-module issuance and
requiring the capability for both operations. Explicit shutdown remains separate.

Spec: no correctness blocker found; dependency pin and final verification were
still pending at review time. No source changes to default media features or
the user's existing `lisp/ldefs-boot.el` edit belong to this work.
