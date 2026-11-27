# e5-thread-control-loom: E5.3 / E5.4 parallel thread-control protocols under Loom (E5.4b)

This is an isolated Cargo workspace. It has no engine dependency, is not a member of the main workspace, and introduces zero `cfg(loom)` into `neovm-eval` or `neovm-core` (matching `keeper-driver-loom`).

`build.rs` copies four real product files read-only into `OUT_DIR` plus one-edge mutated copies for negative rows (every mutation anchor must match exactly once):

* `crates/neovm-eval/src/emacs_core/runtime/eval/green/scheduler/wait_core.rs`: real `OwnerWake`, `SignalHint`, `ParallelParticipant`, and `Participant`, driven over `tests/sync.rs`.
* `crates/neovm-eval/src/emacs_core/runtime/eval/green/condition_wait.rs`: real `ValidatedWait`, `ConditionWait`, `ConditionSeat`, `WaitOwner`, and `wait_with_seat` state machine.
* `crates/neovm-eval/src/emacs_core/runtime/eval/parallel_entry/driver_core.rs`: real keeper-driver protocol (`Driver`, `KeeperOps`, `Batch`, `KeeperMsg::BatonRequest`, `await_return`).
* `crates/neovm-value/src/tagged/gc/mutator_registration/rendezvous/control.rs`: real FIFO serial ring (`enqueue_serial`, `enqueue_baton_group`, `grant_head_in_stop`).

## Models

Every positive model records cross-execution coverage counters (write-only `std::sync::atomic::AtomicUsize` observers that never influence scheduling) and asserts after `loom::model::Builder::check` finishes that Loom explored the real target races (wake-before-park, `ConditionSeat::park_condition`, `ConditionSeat::park_mutex_relock`, `SignalHint`, `coalesce_request_locked` idle-to-requested and duplicate-suppression, etc.) rather than passing vacuously.

| Model | Row | Product code exercised | Negative (mutation) and witness |
|---|---|---|---|
| (a) notify against wait registration | `model_a_notify_against_wait_registration` | `wait_core.rs` (`OwnerWake`, `Participant`) | `wait_core_drop_wake_flag.rs`: `OwnerWake::wake` omits `*woken = true`. Witness: `"OwnerWake lost wake-before-park"`. |
| (b) signal against wake and relock | `model_b_signal_against_wake_and_relock`, `model_b_condition_wait_wakes_mutex_waiter` | `condition_wait.rs` (`ValidatedWait`, `ConditionWait`, `ConditionSeat`, `WaitOwner`, `wait_with_seat`) + `wait_core.rs` (`OwnerWake`, `SignalHint`) | `condition_wait_signal_before_relock.rs`: `wait_with_seat` calls `deliver_signal` before relocking. Witness: `"signal delivered before saved recursive mutex depth restored"`. `condition_wait_drop_wake_mutex.rs`: `wait_with_seat` omits `wake_mutex_waiters`. Witness: `"condition-wait failed to wake mutex waiter"`. |
| (c) join against exit + pre-wait signal snapshot | `model_c_join_against_exit`, `model_c_join_pre_wait_signal_snapshot` | `wait_core.rs` (`OwnerWake`, `SignalHint`, `Participant`) | `negative_model_c_exit_without_join_wake_caught`: exit omits `wake_waiting_for(Wait::Join(..))`. Witness: `"exit failed to wake parked joiner"`. |
| (d) cross-kind wake into idle Baton group (Q1) | `model_d_cross_kind_wake_into_idle_baton_group`, `model_d_concurrent_wakers_coalesce_single_baton_request` | `driver_core.rs` + `control.rs` + `wait_core.rs` + `coalesce_request_locked` (`GroupTurn`) | `negative_model_d_missing_idle_guard_duplicates_group_entry`: coalescer omits `GroupTurn::Idle` guard. Witness: `"duplicate BatonRequest sent"`. `negative_model_d_dropped_coalesce_loses_baton_wake`: coalescer omitted. Witness: `"woken Baton thread never ran"`. |

## Run it

From the repo root (any cwd works; on this box, inside `nix develop`):

```sh
./crates/neovm-core/tests/e5-thread-control-loom/run.sh
LOOM_MAX_PREEMPTIONS=3 ./crates/neovm-core/tests/e5-thread-control-loom/run.sh
```

Without `LOOM_MAX_PREEMPTIONS`, the model bounds preemptions at 2.

CI discovers this crate through `scripts/ci/run-loom-models.sh` (glob `crates/*/tests/*loom*`). The harness reads `LOOM_MAX_PREEMPTIONS` explicitly so the script classes it as `bounded` and exports its `BOUND` here.
