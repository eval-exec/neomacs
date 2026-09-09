# Popup replacement dispatches a scale update to a destroyed window

Status: fixed in the pinned winit fork and Neomacs's post-event popup lifecycle;
release/pdump rebuild and native GUI startup probe passed.

## Reproducer

```sh
RUST_LOG=warn cargo nextest run --locked -p neomacs-display-runtime \
  --run-ignored only \
  -E 'test(linux_wayland_native_menu_replacement_stress)' \
  --success-output immediate
```

The opt-in native test replaces a popup during its `SurfaceResized` callback.
It reproduces the user's exact panic in about 0.6 seconds:

```text
winit-wayland/src/event_loop/mod.rs:386:58
called `Option::unwrap()` on a `None` value
```

The pinned winit revision is `a98b2b217c901f8776a2bdb4ebc35ea7611998ce`.
The local compositor is KWin Wayland, with fractional scaling on eDP-1.
Replacement in `about_to_wait` or redraw callbacks did not reproduce it in
the initial trials. Adding resize-callback replacement made it reproducible.
The previous stable-menu and tooltip smoke tests did not cover this transition.

## Confirmed queue-lifetime failure

1. Winit takes the current compositor-update batch for dispatch.
2. An application event callback replaces a popup and creates its successor.
3. Window creation performs a Wayland roundtrip. Scale updates for the old
   popup can be appended to `state.window_compositor_updates` during that call.
4. Deferred window teardown removes the old popup from `state.windows` later
   in the same iteration, without retiring those newly queued updates.
5. The next iteration drains that queue and unconditionally looks up the old
   window ID. The entry is absent, causing the reported unwrap panic.

Relevant pinned sources: `winit-wayland/src/window/mod.rs` (creation roundtrip),
`state.rs` (`scale_factor_changed`), and `event_loop/mod.rs` (update dispatch and
deferred window removal).

## Counterfactual experiment

A temporary copy of the pinned winit source was made at
`/tmp/neomacs-winit-panic.CxWH1y/winit`; the Cargo cache was not modified.
The only source change removes compositor updates for `w` immediately after
its native window entry is removed:

```rust
state.window_compositor_updates.retain(|update| update.window_id != w);
```

The same test passed with that backend change, including three consecutive
repeat runs. The override was supplied to `cargo nextest run --config`, not
installed in the project configuration. `Cargo.lock` was restored afterward.

Logs:

- `/tmp/neomacs-menu-panic-minimal.log`: exact pinned-backend failure.
- `/tmp/neomacs-menu-panic-patched-backend.log`: temporary backend pass.
- `/tmp/neomacs-menu-panic-patched-repeat.log`: three consecutive passes.

## Implemented repair

The backend retires compositor, synthetic, and input event queues with their
window lifetime, before dispatching `Destroyed`. It also filters dispatch to
absent windows, including events produced by handles after native parent-driven
closure. Device events and live-window event order are preserved.

Fork: <https://github.com/eval-exec/winit>

Commit: `dc7af19d15de375f980687f650174989f4b668a8`, based directly on the old
upstream pin. It is pushed on `neomacs/wayland-popup-lifetime` and pinned in
Neomacs's `Cargo.toml` and `Cargo.lock`. No local path override is committed.
No upstream PR was submitted, per user direction.

Neomacs now commits menu/tooltip native changes after the event batch. Only the
lifecycle module can issue the `PopupCommit` capability in production. Ordinary
creation and retirement require it. Logical detachment immediately stops input
routing, while a typed retirement queue retains native/GPU resources until
child-first release. Native tooltip repositioning is also deferred. Proxy
wakeups no longer run the reconciliation pass. Shutdown, device loss and owner
frame removal drain descendants before releasing parent resources.

See `docs/plans/2026-09-09-popup-lifetime-commit.md` for module ownership.

The exact reason the user's interaction repeatedly replaced the root popup
is not established from the supplied log alone. That is separate from the
now-reproduced fatal queue-lifetime error.

## Regression evidence after the application refactor

The adversarial GPU test still deliberately commits inside a resize callback,
using a test-only capability constructor. Reverting only the backend change
after the Neomacs refactor reproduces the same panic in 0.63 seconds; restoring
the fix passes in 6.08 seconds. Thus application deferral is not masking the
backend regression test.

- `/tmp/neomacs-popup-gated-red.log`: unpatched backend, exact panic.
- `/tmp/neomacs-popup-gated-green.log`: patched backend, same test passes.
- `/tmp/neomacs-popup-native-final.log`: four native checks pass (stable menu,
  passive menu tooltip, adversarial replacement, post-event replacement).
- `/tmp/winit-popup-committed-tests.log`: 13 winit tests pass, including the
  queue-order unit test and supplementary native softbuffer stress test.

The standalone softbuffer test also passes on the old backend. It is not the
red-capable reproducer; the fork documents that distinction explicitly.

## Final pinned dependency checks

`cargo nextest run -p neomacs-display-runtime -p neomacs-display-protocol
-p neomacs-renderer-wgpu -p neomacs --no-fail-fast` (no local override) ran
2,732 tests: 2,727 passed, five failed, six skipped. All protocol, runtime and
renderer tests passed. The frontend failures are:

- `primary_display_host_request_video_preserves_uri_source`
- `primary_display_host_request_video_queues_create_once_with_stable_id`
- `primary_display_host_routes_one_typed_video_session_lifecycle`

These expect video handles/sessions despite `video` not being compiled into
the default build. Default features remain unchanged, per user direction.

- `bootstrap_batch_startup_error_exits_nonzero_like_gnu`: expected the error
  string in captured messages, but captured messages were empty.
- `gnu_startup_next_line_moves_point_on_live_gui_frame`: bootstrap evaluator
  reports unsupported window system `neo`, then `(wrong-type-argument stringp
  nil)`. This test initializes evaluator GUI frame state, not a native winit
  event loop.

Those frontend failures remain unresolved; this is not a green whole-suite
claim. Log: `/tmp/neomacs-popup-final-tests.log`.

All four opt-in Linux Wayland native tests were repeated with `--locked` and
the published git dependency, without any local override: four passed in
24.4 seconds. Log: `/tmp/neomacs-popup-pinned-native.log`.

## Release and pdump

`cargo xtask fresh-build --profile release --no-byte-compile` completed
successfully. Release compilation took 13m 43s; the subsequent generated-Lisp
and final-pdump stages also succeeded. No byte-compilation was requested.

The existing Linux product policy in `xtask` explicitly passes `--features
video`; this is separate from Cargo defaults. Neither that policy nor the
default feature list was changed. `video` and `webview` are still not Cargo
default features. The rebuilt Linux product therefore includes video, matching
the existing fresh-build contract.

The resulting `target/release/neomacs` loaded its matching final image and
printed `PDUMP-OK` in batch mode. A timed native GUI probe showed styled Unicode
text, repositioned the same tooltip, replaced its content, and successfully hid
it before clean exit. Exactly two popup creations were logged: the initial
tooltip and its replacement. No panic or fingerprint error was logged.

- `/tmp/neomacs-popup-release.log`: successful build and pdump generation.
- `/tmp/neomacs-popup-pdump-check.log`: batch image loading.
- `/tmp/neomacs-popup-release-probe.el`: disposable GUI probe.
- `/tmp/neomacs-popup-release-probe.log`: native lifecycle probe and clean exit.

The user's four-line `lisp/ldefs-boot.el` edit remains unchanged and is excluded
from the implementation commit. Winit is pushed to the fork; no upstream PR is
created. Neomacs changes are committed locally, not pushed.
