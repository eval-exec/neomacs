Cut-1 wake compiler contracts are prepared but **NOT RUN**. No stderr snapshots
have been generated or blessed. The existing `host_effect_compile_contracts`
runner discovers these fixtures through its existing pass/fail globs; no runner
or production fixture export was added.

Expected primary diagnostics:

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

Parent integration must run the existing thin compiler-contract target locally
with rustup first on PATH, without rust-src, then inspect diagnostics and create
the exact stderr snapshots. Build/test/census receipts remain the parent's gates.
