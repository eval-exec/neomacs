# pentry-l6-loom — the L6 rendezvous ring model over the real control source

Isolated Cargo workspace (no engine dependency, not a member of the main
workspace). It checks the L6 serial-request ring on the **product**
`rendezvous/control.rs`, copied read-only by `build.rs` into `OUT_DIR`:

* `build.rs` strips only the leading `//!` block and the trailing
  `#[cfg(test)]` control-tests module, rewrites the crate visibility to
  `pub(crate)`, and re-anchors the product `super::super::MutatorKey` import to
  the model parent. The model supplies `MutatorKey` and `RendezvousError` at
  its crate root.
* The copied control/ring names no `std::sync`, `crate::` or `super::super`.
  The L6.0 ring control itself uses no synchronization; the model drives the
  product `State` under the test's own Loom `Mutex`/`Condvar`. The product
  park/wake (`gate.rs`/`mod.rs` and the ring-predicate condvar) is **not**
  copied here; L6.7 carries that copy so L1/L3/L7 become product evidence
  under 6052/6091.
* `ring` — Loom rows:
  (a) N requesters + keeper: grants follow commit order and no committed
  requester is lost;
  (b) a queued requester cancelled by owner exit is retired exactly once, the
  surviving requester grants once, and no id duplicates;
  (c) two keepers racing during `DrainingResumes` produce exactly one grant.
* `confinement` — the copied source stands alone and keeps the L6 ring API.

## Run it

From the repo root (any cwd works; on this box, inside `nix develop`):

```sh
./crates/neovm-core/tests/pentry-l6-loom/run.sh
# or directly:
cargo nextest run --manifest-path crates/neovm-core/tests/pentry-l6-loom/Cargo.toml
```

Loom bounds: run at `LOOM_MAX_PREEMPTIONS=2` and `LOOM_MAX_PREEMPTIONS=3`;
both are witnessed for this model. Extra arguments pass through
(`run.sh --test ring`, `run.sh --no-capture`). A build.rs assertion fails the
model if the product file grows a direct `std::sync`/`crate::`/`super::super`
or loses the ring API, so re-anchoring is explicit rather than silent.
