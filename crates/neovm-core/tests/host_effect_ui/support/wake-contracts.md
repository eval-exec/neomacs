Cut-1 wake compiler contracts **passed** locally with rustup Rust 1.96.1
first on PATH and no build-std. The containing thin integration target passed
8/8 runners. Its host-effect glob now covers two pass cases and 25 intentional
rejections, including the three new wake rejections below and their positive
actual-source control. Only these three stderr snapshots were created; no
existing snapshot changed. The diagnostics were inspected for their intended
privacy/linearity errors. No runner or production fixture export was added.

Observed primary diagnostics:

| Fixture | Required rejection |
| --- | --- |
| `wake_source_private.rs` | E0603: production module `wake` is private; callers cannot name or construct its bare `ControlWake` owner. |
| `wake_lease_no_clone.rs` | E0277: actual `NativePollLease` does not implement `Clone`. |
| `wake_session_constructor_private.rs` | E0451: actual `HostSessionControl` fields are private; struct update cannot retarget an existing attachment to another backend. |

`host_effect_pass/wake_owner.rs` imports the same actual source and constructs
legitimate paired owners, a session, a registration and a lease without waiting.
It must pass, so missing polling/static_assertions/ReadDeadline dependencies or
source-import breakage cannot count as a negative witness. These dependencies
already exist in the core thin integration target; no Cargo dependency changed.

The two inner-owner fixtures import the actual `src/host_effect/wake.rs`, with
its production field visibility and trait implementations. They do not copy
those definitions or expose a new production constructor. They do not establish
native readiness, host provenance, OS wake behavior or hosted admission.

The new stderr snapshots were blessed with the pinned rustup compiler and
reviewed for exactly these diagnostics. Missing imports, unsupported APIs or
unrelated failures did not count as rejection witnesses. Frozen protected and
explicit hot-owner census, full core correctness, Loom and actual-wake TSan
are separate gates; a compiler rejection does not establish runtime progress.
