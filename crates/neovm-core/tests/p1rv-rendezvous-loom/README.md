# Proof 1 rendezvous models

This standalone test workspace models the proposed all-mutator stop protocol
from `tmp/peer-p1rv/design.md`, section 4. It pins Loom 0.7.2 and
static_assertions 1.1.0. It has no production dependency, feature, evaluator or
unsafe code. Its own `[workspace]`, manifest and lockfile isolate it from the
production workspace. Adding these tests changes no Sim runtime inputs, hot
paths, generated code or layouts.

## Model map

| Files | Modeled contract |
| --- | --- |
| `tests/model/mod.rs`, `tests/rendezvous.rs` | Keeper and owner control transitions; frozen identities; request/ACK/trace/resume; allocation loans; queued requests; native entry/return; registration, retirement and poisoning |
| `tests/visibility.rs` | Independent atomic publication of root/log payloads before ACK, and collector responses before resume |
| `tests/readers.rs` | Reader admission, explicit finish, Closing publications, Busy retry, abandonment, identity/generation checks and retirement |
| `tests/wait_graph.rs` | Declared keeper/owner/resource dependencies involving the Baton, control mutex and allocator |
| `tests/poison.rs` | Event-driven poison, gate holder identity, Condvar owner transfer and exact lost-wake/self-lock controls |
| `tests/cons_catalog.rs` | Keeper-side catalog detach/restore: transition exclusion of new run admissions and loan refusals around the cold window |
| `tests/cons_tails.rs` | Owner-exclusive cons tails: lent-range charges, tail publication back to the catalog, and slice-close hand-in ordering |
| `tests/admission.rs` | Lock-free slice admission: the Dekker store->load pair (owner admission word vs keeper phase word), no slice overlapping an AllStopped epoch, no lost request wake, poison reaching parked owners, and the catalog-transition fenced scan; exact witnesses for a missing owner fence, an unfenced authority scan and a skipped scan |
| `tests/support/mod.rs` | Exploration receipts and exact expected-panic witnesses |

The control model intentionally uses one mutex and a condvar. Its heap phases
are `Idle`, `Stopping(epoch)`, `Tracing(epoch)`, `DrainingResumes` and
`Poisoned`. Entries distinguish Running, Publishing, Stopped, NativeStopped,
Resuming, StartingStopped and Retired; retirement obligations are also recorded
on an entry. A key includes heap, World, owner slot and registration generation.
Each root record additionally has its publication generation and request epoch.
Roots and logs are abstract words/counters, not Lisp objects.
Publication construction is collapsed into a mutex-protected transition:
Publishing is vocabulary for the production seam, not a separately scheduled
state in this model. Pending log counters are keeper-owned after publication;
the immutable record retains its original log marker across native epochs.
The lost-log controls deliberately corrupt that extracted accounting. Reader
tests abstract one heap slot and use fresh controls on generation renewal.
The scalar trace oracle also updates counters under the gate; it does not model
actual collector work or authorize carrying that gate across runtime GC,
worker waits or callbacks.

The admission model (`tests/admission.rs`) mirrors the production phase word
(tag/blocked/epoch, plus a model-only transition counter in bits 16.. bumped by
the closing publish of every transition window) and the owner admission word
(OUT, IN_SLICE packing the observed epoch, EXITED). The transition counter is
what makes the Dekker witnesses exact by value: an admissible counter-0 word
is exactly the pre-transition initial word, so `missed && pre_transition_body`
identifies a slice that ran against a window the authority truly failed to
observe, without false-firing on a legal post-window admission. Stop-path
invariants in the model are self-healing by construction (every owner actor
ends in a gated settle/wait, mirroring the production driver's park between
slice batches), so no stop-path negative can fire; the load-bearing scan is
the catalog transition's, which the controls target directly.

The checked invariants include:

- Tracing requires complete root/log coverage of every frozen identity, matching
  generations and epoch, no outstanding run/worker loan and no restoration debt.
- A request cannot start another stop while owners are restoring coverage.
  Requests queue during `DrainingResumes`; the last restoration acknowledgement
  discharges the debt and permits activation.
- An owner cannot allocate during trace or before applying its resume image.
  Allocation finishes its grant and drops the allocator guard before ACK.
- A new registration during a stop remains StartingStopped and cannot allocate.
  Stale identities and exhausted generations refuse admission without a partial
  membership change.
- A retiring owner cannot disappear from frozen membership or lose pending
  roots/logs. Poisoning wakes waiters, refuses further successful collection or
  resume, and retains the stopped images.

The main scripted case has a keeper and two allocating owners for one epoch.
Separate cases cover two epochs of resume debt and two native-stopped epochs. Smaller cases isolate request coalescing, registration and retirement
races, publication failure, loans, stale records, notifications and poisoning.
Each logical owner has one scripted actor. The helper `Owner` factory and its
thread-confined wrapper are **not** a proof of unique production admission or
of a globally unique token for a key; the scripts assume that uniqueness.

## Native, visibility and retirement boundaries

Native entry publishes roots and a stopped image before it can count as
stopped. Native return checks the active-stop gate and reserves restoration debt
before transferring the image, including when the heap was Idle. A dedicated
case leaves one owner NativeStopped across two collections: its original log
is folded once, its entry-owned image advances each epoch, and return takes
the latest image exactly once. The native actor does not need to poll or wake
for those collections: it blocks on a separate host-only completion latch.
That latch orders its return after epoch two; the separate return/request race
and atomic visibility cases test the gate and memory edges.
This is a protocol abstraction, not a real syscall or
Lisp mutex implementation.

The atomic visibility tests are separate because the control mutex could
otherwise hide a missing publication edge. They use Relaxed atomic payload
words, a Release ACK and an Acquire observation, without a mutex or join before
the observation. Separate tests do the same for completion/resume. Weakening
one edge or publishing ACK before the payload must expose its specific stale
payload witness. Passing the mutex model alone does not establish those edges.

Reader models retain immutable publications through active reads, require
explicit finish before resume, preserve roots on Busy, and retain heap/exclusion
obligations after abandonment. They check both generation/key identity and
distinct control identity. Separate negative controls permit premature resume,
ignore abandonment or accept an obsolete generation and require the resulting
specific violation.

The wait graph checks declared lock ordering only. ACK has no dependency on
the keeper's escrowed Baton; a condvar wait releases control ownership; and an
allocation grant ends before ACK. Negative controls restore a forbidden
dependency and require exactly `WAIT_GRAPH_CYCLE`. They detect a finite graph
cycle without making Loom threads actually deadlock. These checks cannot find
an undeclared production dependency.

R6 adds a separate poison protocol model. A guard records its actor identity
while holding the real mutex, clears it before Condvar's atomic release/wait,
and restores it after reacquisition. External poison takes the predicate gate;
holder poison uses the exact identity bypass and wait rechecks terminal poison.
No timer supplies progress. Negative controls use finite notification/lock-edge
oracles to expose a lost wake or same-holder blocking cycle, rather than
accepting a Loom deadlock as the expected witness. Actor identities are unique
script constants; production assigns monotone thread-local identifiers. OS
mutex poisoning/recovery is covered by production tests, not Loom's mutex.

## Reproduction and receipts

Run from the lane worktree root. Keep build/test work separate from performance
measurements. These commands use the lane's CPU set, Nix environment, owned
temporary directory and separate Loom target directory. They run all test
targets with one libtest thread, first at preemption bound 2 and then 3:

```sh
mkdir -p tmp/peer-p1rv/a1/loom-tmp tmp/peer-p1rv/a1/loom
TMPDIR="$PWD/tmp/peer-p1rv/a1/loom-tmp" \
  taskset -c 0-3,14-31 nix develop -c bash -euo pipefail -c '
  unset RUST_LOG
  unset LOOM_MAX_BRANCHES LOOM_MAX_DURATION LOOM_MAX_PERMUTATIONS
  unset LOOM_CHECKPOINT_FILE LOOM_CHECKPOINT_INTERVAL LOOM_LOG
  export TMPDIR="$PWD/tmp/peer-p1rv/a1/loom-tmp"
  export CARGO_TARGET_DIR=/home/exec/nvme-rust-target/v10x-peer-p1rv-loom
  export RUSTFLAGS="--cfg loom"
  for bound in 2 3; do
    export LOOM_MAX_PREEMPTIONS="$bound"
    cargo nextest run --locked \
      --manifest-path crates/neovm-core/tests/p1rv-rendezvous-loom/Cargo.toml \
      --tests --no-capture \
      > "tmp/peer-p1rv/a1/loom/bound-$bound.log" 2>&1
  done
'
```

Inspect the nextest summary and preserve the complete logs,
including receipts and expected panic messages. `support::check` preserves the
configured preemption bound, uses a 512-branch safety bound, and applies no
permutation limit, duration limit or checkpoint truncation. A branch-limit
failure is a failed run, not successful bounded exhaustion.

`LOOM_ITERATIONS <test> <count>` records model closure invocations. For positive
tests, a successful run exhausts the explored schedules at that configured
bound. Negative controls stop at their first required counterexample, so their
iteration counts describe search to a witness, not exhaustion. Their matching
`LOOM_WITNESS <test> <message>` receipt confirms the exact expected assertion.
An unrelated panic, branch-limit failure or Loom deadlock cannot satisfy the
negative harness. Expected-panic output in a successful negative test is
intentional; preserve it as evidence.

Verified bounds, per-test iteration counts and witness names are recorded in
`RESULTS.md`; full logs and the handoff report are retained under the lane's
`tmp/peer-p1rv/a1/` directory.

## Limits

These are independent finite models. Passing them does not establish their
end-to-end composition into one production implementation. In particular, the
mutex control model, isolated atomic visibility experiments, reader model and
declared wait graph have separate assumptions and interfaces that still need
an implementation correspondence review.

They do not execute the VM, allocation regions, World managers, actual root
enumeration, GC tracing/sweeping, TLS transport, native stacks or JIT stack maps.
Abstract loan bits do not prove Rust borrowing, pointer provenance or the
absence of unsafe aliasing. Abstract root words do not prove root census
completeness. Concurrent marking is excluded by construction; this is not a
proof of the eventual runtime capability boundary. The models do not establish
OS scheduling fairness, wall-clock
rendezvous bounds, arbitrary native-call progress or production deadlock
freedom. Loom exploration is bounded and is not a complete hardware memory
model proof.

No test here establishes a TSan pass or arm64 correctness. Those gates, Miri/API
checks, real concurrent allocation/collection tests, the dedicated projected
owner census, zero Sim census and performance measurements remain separate
implementation and qualification obligations.
