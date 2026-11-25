# keeper-driver-loom: the L6.7 keeper driver's protocol under Loom (K9)

This is an isolated Cargo workspace. It has no engine dependency and is not a member of the main workspace. It checks the keeper driver's real protocol code. `build.rs` copies two product files read-only into `OUT_DIR`:

* `crates/neovm-eval/.../parallel_entry/driver_core.rs` holds the driver's whole protocol. It is generic over `KeeperOps` (the era) and `Batch` (custody between two lends), and it reaches synchronization only through `super::sync`.
* `crates/neovm-value/.../rendezvous/control.rs` is the serial ring, re-anchored as in `pentry-l6-loom`.

`tests/model.rs` is the model era.
* It drives the product ring under a Loom `Mutex`, and the keeper waits for a model change on the seam's one-waiter `Signal`.
* A stop attempt that the members have not met waits for one model change and reports Pending. The request stands, as the product's condvar-bounded attempt leaves it.

`tests/sync.rs` is the model's side of the sync seam.
* Its channels are built on a Loom `Mutex` and `park`/`unpark`, with std semantics: `sync_channel` capacity, and a receiver disconnected when the last sender drops. A blocking send into a full grant channel is a model panic, since the protocol never needs one.
* Loom's own `mpsc` has neither, and the driver's `Drained` end and L9 depend on both.
* The backoff's clock is virtual: `Instant` reads a per-execution model counter (a Loom lazy static) that only `sleep` advances, and `sleep` then yields to Loom. So the contention bound is reached after a fixed number of backoff rounds on every path, and no host time enters a replay.
* Every wait registers its thread under the lock and parks, and its waker takes that registration under the same lock, so an unpark only meets a thread that is parked or about to park. Loom's `Condvar` can leave a stale waiter that later wakes a thread blocked on a mutex ("expected to be able to acquire lock"). Its `Notify` explores a spurious wake at every wait, which multiplied `two_owners` at bound 3 past eight minutes.

Loom replays a path only if the model is deterministic. On a replay it takes the recorded schedule's thread without checking that the thread can run. The driver's owner tables are `BTreeMap`s for that reason: the poison fan-out walks them, and a `HashMap`'s per-instance seed changed the send order between replays. Under the `Notify` seam that read "Reached unexpected exploration state". Under this seam a parked owner resumes unwoken and re-registers, which trips "model: a second waiter".

## Rows

Each row has a one-edge negative. build.rs mutates one anchored line of a copy, and the anchor must match exactly once. The witness is the exact assertion the mutation trips. The one-turn-out probe is the exception: no mutated core that lends before a return compiles, so its negatives move the model's return ahead of the turn's end instead, and compile-fail fixtures pin the core's side.

| Row | Property | Negative (mutation) and witness |
|---|---|---|
| `two_owners_one_turn_each_in_commit_order` | L1: grants follow commit order. L3/L4/L5: one ticket out, no lend before return (in-turn probe). L7: each owner is woken only by its grant. | `control_lifo.rs`: the ring commits at the head. Witness: "grants follow commit order". `driver_core_misrouted_grant.rs` (L7): the grant goes to another enrolled owner. Witness: "a grant woke an owner it was not cut for". L3/L4/L5: no copy of the core that lends before the return compiles, because a `Batch` lends once and only `settle`, which takes the returned ticket, makes the next one. `custody_batch_lends_once.rs` and `custody_batch_lends_nothing_while_the_world_is_out.rs` in `crates/neovm-core/tests/pentry_e2_loan_ui` pin that (E0382). The in-turn probe's own negative, `an_early_give_back_overlaps_two_turns`, sends each ticket back while its owner is still in its turn, the earliest a next lend can start. Witness: "two serial turns at once". |
| `an_abandoned_lend_poisons_and_wakes_the_rest` | L8: a lend guard dropped armed poisons, and the queued owner reads Poisoned. | `driver_core_silent_poison.rs`: the poison fan-out sends nothing. Witness: "the queued owner reads Poisoned". |
| `a_dead_keeper_disconnects_every_owner` | L9: once a keeper dies unanswered, the owner's grant channel reads disconnected. | A grant sender leaked past the keeper. Witness: "the owner learns the keeper is gone". |
| `a_slow_owner_delays_the_turn_and_poisons_nothing` | K8 r2: a pending stop keeps its request, and the turn is lent once the slow owner stops. | `driver_core_pending_poisons.rs`: a pending stop poisons, the r1 shape. Witness: "a slow owner poisons nothing". |
| `a_group_and_an_owner_alternate_in_commit_order` | L10 (L6.7b b3): the Baton group's World turn and an owner's ticket are lent in commit order across the two kinds, one turn out at a time (the in-turn probe is shared). | `control_lifo.rs`. Witness: "grants follow commit order". The plan's b3 negative, `an_early_world_return_overlaps_the_owner`: the World goes back while the group still holds it, so an owner lent next overlaps the interval. Witness: "two serial turns at once". The `custody_batch_lends_nothing_while_the_world_is_out.rs` fixture pins that the core itself cannot lend before the World's return. |
| `an_abandoned_world_turn_poisons_the_owners` | L11: the group's lend guard dropped armed poisons, and the waiting owner reads Poisoned. | `driver_core_silent_poison.rs`. Witness: "the queued owner reads Poisoned". |
| `an_abandoned_lend_poisons_the_queued_group` | L12: an owner's abandoned lend poisons, and the queued group reads Poisoned on its own channel. | `driver_core_silent_poison.rs` (its third anchor drops the group's fan-out). Witness: "the queued group reads Poisoned". |
| `a_collecting_group_resumes_before_any_other_turn` | L13 (L6.7b b5, QB3): the group's turn comes back asking for a collection while an owner is queued behind it; the next stop collects first and lends the group the World again before the owner, in that same stop. | `control_requeue_tail.rs`: the group is re-queued at the tail. Witness: "the collecting group resumes next". |

## Run it

From the repo root (any cwd works; on this box, inside `nix develop`):

```sh
./crates/neovm-core/tests/keeper-driver-loom/run.sh
LOOM_MAX_PREEMPTIONS=3 ./crates/neovm-core/tests/keeper-driver-loom/run.sh
```

Without `LOOM_MAX_PREEMPTIONS`, the model bounds preemptions at 2.

CI finds this crate through `scripts/ci/run-loom-models.sh` (glob `crates/*/tests/*loom*`). The harness reads `LOOM_MAX_PREEMPTIONS` itself, so the script classes it as bounded and exports its `BOUND` here: 2 on push and pull requests, 3 on the Sunday run.
