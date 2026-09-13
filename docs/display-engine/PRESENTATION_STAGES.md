# Editor snapshots, submission, and native presentation

These stages are different facts. None is an alias for another:

| Stage | Producer and witness | Consumers and policy |
| --- | --- | --- |
| Editor snapshot installed | `frame_ingest` emits `PresentationActivated`; `PresentationId` identifies immutable evaluator data | Evaluator snapshot lifetime, interaction identities, drawing source. Does not wait for rendering or native feedback. |
| Frame submitted | `queue.present(output)` returns; `SubmissionResult::Submitted` | Frame coordinator consumes the render plan. New demand proceeds independently of compositor confirmation. |
| Submitted interaction geometry | `publish_submitted_projection` after queue submission | Pointer hit transforms use the latest submitted composition. Failed/abandoned render attempts do not replace it. |
| Content confirmed on native output | Native `wp_presentation_feedback.presented` event | Opt-in GUI readiness receipts. Discarded content cannot advance readiness. |

## Type ownership

`render_thread/frame_sched::SubmissionResult` is the synchronous result of
servicing a frame plan. It has no `Presented` variant and is not a native
feedback message. Timeout, occlusion, loss, and awaiting-content branches
retain their existing scheduling behavior.

`presentation_feedback/wayland.rs` owns native protocol interpretation:

- `Submission` describes the feedback-associated submission metadata.
- `NativeFeedback` distinguishes `Presented(ConfirmedPresentation)` from
  `Discarded(Submission)`.
- `ConfirmedPresentation` combines that metadata with a `CompositorTimestamp`.
- `Receipts::publish` accepts only `ConfirmedPresentation`, not a raw submission
  or a boolean success flag.

The protocol adapter constructs a confirmation only on the native Presented
event. It records the compositor's declared clock ID and timestamp, rejecting
a missing clock domain or invalid nanosecond field. `CompositorTimestamp` is
not the scheduler's `EventTime` or predicted target time: there is no implicit
conversion between those clocks. The GUI receipt exposes the timestamp in
`:clock-id`, `:seconds`, and `:nanoseconds` fields.

No universal platform-acknowledgment interface is invented here. Wayland has
an implemented diagnostic adapter; other platforms do not manufacture
confirmation from queue submission. This observer neither drives pacing nor
delays evaluator input.

## Audited legacy terminology

Some names describe established editor or submission contracts and are not
renamed blindly:

| Existing name | Actual meaning / reason to retain |
| --- | --- |
| `PresentationId`, `PresentationActivated`, `PresentationRetired` | Immutable evaluator snapshot identity and lifetime. One snapshot can produce many animation submissions. Renaming these to native submission IDs would be incorrect. |
| `PresentedPointer`, `PresentedHitQuery`, `PresentMapping` | Geometry/identity witnesses used by the interaction model. Not native feedback receipts. Their coordinate-safety guarantees remain intact. |
| Popup `presented` flag in `presentation/host.rs` | Set immediately after popup queue submission; permits child popup creation. It must not become a wait for optional native confirmation. Local naming can be tightened in a separate popup-only change. |
| `frame_stats` fields containing `present` | Historical queue-submission counters and commit-to-submission measurements. Log fields are retained for existing tooling, with their semantics documented in the module. Not scanout latency. |
| `finish_presented_video_surface` / video `presented_frames` | Optional video adapter records surface submission evidence and local timing after `queue.present`. This audit does not claim those counters are native display acknowledgments or change their exported contract. |

The renderer's “compositor” is also distinct from the OS compositor. A
Neomacs composition transform can be valid without the OS having displayed
that frame yet.

## Regression and verification

The native GUI fixture now requires a confirmed receipt's compositor-clock
timestamp before it advances. Both 8K solid-background and slow-patterned
startup tests failed against the old receipt format. The tests remain at the
real GUI/native protocol seam; no private timestamp formatter test substitutes
for that path.

The Rust driver also compares the final receipt against independent native
`wp_presentation_feedback.presented` and `wp_presentation.clock_id` trace
events. A synthetic scheduler timestamp with the right field names is not
enough to satisfy this contract.

The submission-only rename passed the full runtime nextest suite: 966 passed,
5 skipped. This includes snapshot activation, pointer/projection, scheduler
follow-up demand, timeout, and lifecycle regressions. No input-publication or
render scheduling timing was changed.

Final verification after the typed-receipt change: 966 runtime tests, 16
harness tests, and all 11 selected GUI tests passed, including both independent
native timestamp comparisons and the GNU Emacs resize oracle. Logs are under
`target/diagnostics/issue-360/startup-fonts/submission-types-*`.

The release build succeeded and the matching pdump was regenerated without
byte compilation or autoload regeneration. Fingerprint:
`262444BEC54D0E4CF072F2889579DF50E2F6A331818DBE7A2E0F3AFF2ED3BB82`.
Native verification is Linux Wayland only; other platforms were not exercised.

See [the output-readiness investigation](../diagnostics/2026-09-13-weston-output-readiness.md)
for why a ready socket, frame callback, or readback cannot stand in for native
presentation confirmation. The separate Weston cached-detach diagnostic is
not a reason to add unsolicited parent commits or defer all input.
