# Char-table slot publication model

This standalone workspace models the ctrace fix's private, thread-confined
write capability. It inherits the P7.4 model's exact dependency versions and
lockfile graph, without adding dependencies or features to production.

The required protocol is:

1. Construct any newly reachable child's header and payload.
2. Queue the overwritten child before changing the slot (SATB).
3. Execute a Release fence, then the single Relaxed atomic slot store.
4. The marker loads the slot with Acquire before reading its child's fields.

The concurrent reads are `tagged/header.rs::load_value_atomic` and
`VectorScanEntry::scan_values`. Production `Value::with_char_table_mut`
and `with_sub_char_table_mut` delegate to the barriered capability constructors
in `tagged/mutate/chartable.rs`; their private `SlotWrite::publish` executes
exactly the Release fence followed by a Relaxed store modeled here.
`RequiredOrdering` models that ordering protocol, not the production type or
its pointer/lifetime implementation.

`SlotWriter`, `SlotWrite` and `PreparedWord` are private and non-Send/non-Sync;
the capability exclusively borrows its writer and is consumed by the one
`publish` entry. Only the mock atomic storage crosses threads. The writer
remains on the model's original mutator thread; the marker endpoint crosses
to its reading thread. Adjacent static assertions pin these contracts;
the coded mock word also has size/alignment pins. There is no unsafe code,
real Value, pointer dereference, collector or Lisp execution.

The pending SATB preimage is one Relaxed atomic word: this abstracts queue
membership and deliberately does not model the queue implementation or add a
mutex synchronization edge. The constructor write occurs on the mutator after
marker spawn, so spawn cannot publish it. Only the writer yields after the
slot store; yielding the marker before its payload read would let Loom's
fairness rules force a newer payload observation and mask a missing fence.
There are no release/acquire readiness flags. The marker's original property
panic is propagated across join without substituting a join-error message.

Host-only counters report explored iterations and old/replaced slot
observations before join. Every positive check must exercise at least one
replacement before join. The counters use ordinary host atomics rather than
Loom primitives and never affect modeled memory or scheduling. Select
nextest's success-output option to retain their output for passing cases.

The initial version compiled, but its first run had three positive passes
and two controls that failed to panic. That exploratory run is invalid
model evidence. The corrected run on 2026-10-10 passed all five tests; each
positive explored 32 iterations, including six replaced-slot observations
before join. Both controls reached their exact property failures (five and
seven explored iterations). The retained final log is
`tmp/codex/ctrace/loom.log`. The tests are:

- `release_fence_relaxed_slot_publishes_constructor_before_marker_load`
- `satb_preimage_precedes_replacement_with_an_immediate`
- `preinitialized_child_remains_visible_with_the_required_slot_protocol`
- `negative_control_missing_release_fence_exposes_uninitialized_child`
- `negative_control_barrier_after_store_loses_preimage`

The negative controls require their specific contract assertion messages;
unrelated panics or the branch limit cannot satisfy them. The preinitialized
child case keeps the required slot protocol, since its fence also publishes
pending SATB membership in the queue abstraction.

One slot, one mutation, one marker attempt and two threads are modeled, with
no permutation, duration, preemption or checkpoint truncation and a 128-branch
safety bound. Final post-join state is checked. The model retains all mock
storage until the reader completes. It does **not** prove pointer alignment,
atomic casts, Rust aliasing, COW backing publication/retirement, complete SATB
root sets, queue implementation, mark termination, repeated mutations,
multiple mutators or heap lifetimes. Constructor fields use Relaxed atomics as a visibility oracle; the
model does not certify a plain memory access or repair an actual race by itself.
Passing Loom is a bounded regression check, not a complete weak-memory proof.

After the coordinator's source review, run the independent manifest through
the prescribed Nix shell with a lane-owned Kioxia target, nextest16 and direct
log files. The coordinator ran the initial version; no additional test or
build was run while correcting these files.
