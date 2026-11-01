# S2 host-service protocol model

This isolated crate models the **linearized authority and lifetime contract** of
an outstanding host call. It does not import the engine or reproduce its queue
implementation. The registry mutex is an abstract linearization boundary; it is
not evidence that the real transport uses the right atomics or lock ordering.

`tests/protocol.rs` has four positive cases and ten named negative controls.
The concurrent case uses at most four actors: the requesting worker, main's
native service turn, native completion, and a second worker issuing control
metadata. Cancel, close and worker death race dispatch/completion. Blocking Loom
channels bound actor progress; there are no fairness-dependent spin loops.
Exploration is explicitly bounded to two preemptions and 256 branches per
execution, with no duration or permutation truncation. The single outstanding
call models the reservation's critical interval; it does not establish fairness
or multi-session FIFO order.

The checker is separate from the transition implementation. It replays an
observable event history and verifies:

- publication follows parked-root preparation;
- native execution has the sole Baton and commits at most once;
- terminal settlement is durable before notification;
- root retirement follows restored-owner acknowledgement or settled dead-owner
  teardown authority, including cancellation and host closure;
- main's original join/mutex/condition/sleep/input/process wait is preserved;
- the requester's service-only slice admits neither unrelated Lisp nor a target
  signal checkpoint;
- only a real blocking release permits that scheduling, followed by Baton
  reacquisition before native completion.

The native request may commit while cancellation is outstanding. Cancellation
selects the terminal cause; it does not roll back that native effect. Duplicate
completion cannot create another settlement or retirement authority.

Each negative changes a protocol transition and runs the independent checker.
The test catches only the diagnostic for the named invariant. A Loom deadlock,
branch-limit failure or unrelated panic cannot satisfy a negative control.
These are executable counterexample controls, not ignored or `should_panic`
tests.

The separate `tests/wake.rs` target adds four positive cases and four negative
controls for cut 1. **Execution status: 8/8 passed; see RESULTS.md.**
It is an abstract contract model, with its own Loom mutex and channels; it does
not import `host_effect/wake.rs` or substitute Loom primitives into production
`std` synchronization. Its independent history replay checks:

- registration and physical rearm precede the final durable predicate check;
  publication races before, between and after these steps;
- scheduler consumption of a physical hint preserves main's independent epoch;
- main and scheduler cannot simultaneously hold the physical polling lease;
- retired session allocation rejects late registration/publication while a
  replacement allocation remains independent.

The named mutations are `NoFinalPredicateRecheck`,
`SchedulerConsumesMainNudge`, `ConcurrentPollerWaiters` and
`IgnoreClosedSession`. Each must produce the same invariant diagnostic used by
the positive history checker. Deadlock, branch exhaustion and unrelated panics
are rejected as negative witnesses. Exploration uses at most four threads,
two preemptions and 256 branches per execution, with no duration/permutation
truncation. Blocking channels order the epoch-drain and overlapping-lease
witnesses; no spin loop or elapsed-time assumption supplies progress.

This wake model represents physical arm as a history event. It does not perform
an OS wait, model an actual condvar unlock, prove kernel/socket rearm or closure
behavior, validate production atomics, establish fairness or prove progress after
an arbitrary number of spurious wakes. Its durable predicate is monotonic within
one obligation per destination; it does not model consumption/reuse, epoch
overflow, generation reuse, cancellation policy, or pending native requests.
The production wait-path tests, failure controls, TSan and frozen census remain
separate gates. Run this new target with `--test wake` in the command below;
`--test protocol --test wake` selects both targets.

| Mutation | Required rejected history |
| --- | --- |
| `PublishBeforePark` | request visible before its parked roots |
| `WakeBeforeDurableReply` | notification before terminal settlement |
| `ReplayAfterWakeFailure` | a second native dispatch after accepted publication |
| `CancelDropsLiveRoots` | cancellation retires roots before acknowledgement |
| `CloseRetiresUnconsumedReplyRoots` | close retires an unconsumed native reply |
| `CompleteAfterCancelTwice` | late native completion creates a second terminal |
| `RetireBeforeRestore` | roots retired before owner restoration |
| `ServiceMakesMainRunnable` | service makes main's original wait runnable |
| `ArtificialHopYieldsToUnrelatedOwner` | RPC hop admits unrelated Lisp |
| `ArtificialHopDeliversSignal` | RPC hop creates a target signal checkpoint |

Run from the repository root, inside the lane's admitted build environment:

```sh
CARGO_TARGET_DIR=/home/exec/nvme-rust-target/v10x-wt-lead-ipc/host-service-loom \
  cargo nextest run \
  --manifest-path crates/neovm-core/tests/host-service-loom/Cargo.toml \
  --test protocol --test-threads 1 --retries 0
```

The first admitted build creates this standalone crate's lockfile. Loom is
pinned to the same 0.7.2 as the adjacent authority models. There is no engine
workspace feature or dependency change.

This model does **not** prove real RootBatch registration, heap backing
retention, quiescent collection, panic/fail-stop behaviour after a liveness
commit, real channel memory ordering, TSan cleanliness, hosted wakeup coverage,
GNU Lisp behaviour or OS host affinity. Those require the production
integration, mutation, GNU-oracle and instrumented-channel tests. In particular,
the abstract model has no raw Lisp Values and performs no GC. Its role is to
reject protocol histories that would invalidate those production proofs.

## Owned pending-native driver

`tests/pending_native.rs`, included in the existing protocol target, adds two
positive histories and five mutation controls. Two request reservations retain
independent keys and roots while B's native service unblocks A. Physical driver
acquisition is separate from keyed completion. Cancellation/close retain native
backing until actual settlement and root drain; the last ordinary owner cannot
drop a transferred Baton before the native driver's acquire.

An independent event audit detects ResumeWrongPendingKey,
NativeWaitStarvesSecondRequest, CompleteBeforeReacquire, FreeBackingBeforeDrain
and CloseDropsReleasedBaton. The starvation mutation makes a finite incorrect
rearm choice with B queued; a timeout or Loom deadlock is not an accepted witness.
The seven cases passed on the cut-2 source alongside the existing 28 histories.
These abstract models supplement the actual Green runtime and sanitizer tests.
