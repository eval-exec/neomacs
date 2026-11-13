# N1 immutable descriptor and grace models

This standalone workspace is a specification test crate, pinned to Loom 0.7.2.
It has no engine dependency, product feature, new TLS, unsafe memory or generated
code. The lockfile uses the reviewed p3sym model dependency versions. N1 changes
no production storage or admission API.

There are **26 cases: 15 positive histories and 11 negative controls**. A negative
case passes only when its exact invariant witness is found. The caught failure
is checked against the first panic line, so an unrelated setup/branch-limit
failure cannot qualify. No test is ignored. No try-acquisition is unwrapped.

## Runs and receipts

Run each invocation serially under the lane's prescribed nix/toolchain/CPU and
memory guards. For example, from the repository root, with a separate log per
bound (the orchestrator owns validation and the target directory):

```sh
taskset -c 0-3,14-31 nix develop -c bash -c '
  unset RUST_LOG
  export CARGO_BUILD_JOBS=6
  export CARGO_TARGET_DIR=/home/exec/nvme-rust-target/v10x-peer-p3sym
  export NMJIT_PREEMPTIONS=2
  cargo nextest run --manifest-path crates/neovm-core/tests/nmjit-publication-loom/Cargo.toml --locked --no-capture
'
```

Repeat at `NMJIT_PREEMPTIONS=3`. The helper overrides ambient
`LOOM_MAX_PREEMPTIONS`. There is no permutation/duration cap; `max_branches=4096`
is a fail-visible branch safety limit. All programs have finite operations and
no Busy spin. A Busy refusal is an allowed completed outcome; it is not an
assumption that another owner makes progress after a number of yields. Engine
contention fixtures use retry.rs Deadline; this specification explores finite
attempts rather than implementing an elapsed-time scheduler.

Each case prints `LOOM_RECEIPT <case> bound=<None|Some(2)|Some(3)>
entered=<n> completed=<n>`, including on a negative unwind. `None` is exhaustive
for that case's fixed finite actor/operation set. `Some` is exhaustive only
within the stated preemption bound. Every negative also prints
`LOOM_WITNESS <case> <exact-witness>`. Preserve both logs: those are the per-case
counts and outcomes, not an invented fixed schedule count. No runs were made by
the model author; the parent orchestrator produces qualification receipts.

## Case inventory

`publication` contains twelve cases:

| Case | Exploration | Obligation/witness |
| --- | --- | --- |
| release_acquire_initializes_whole_descriptor | exhaustive, max_threads=3 | descriptor publication orders bound and chunk arm |
| native_atomic_load_initializes_whole_descriptor | exhaustive, max_threads=3 | native SeqCst load establishes the required descriptor initialization edge |
| immutable_descriptor_cannot_mix_bound_and_table | exhaustive, max_threads=3 | one descriptor identity chooses bound and array |
| two_writers_publish_only_owned_initialized_tables | bounded, max_threads=4 | two serialized growth commits; current allocation always owned |
| capacity_move_and_clean_refusal_keep_published_table | exhaustive, max_threads=3 | serial spine move cannot change independent reader table; charge remains |
| retained_limit_refuses_without_publication | exhaustive, max_threads=3 | NeedGrace leaves publication and retained bytes unchanged |
| same_name_winner_and_gap_keep_existing_identities | bounded, max_threads=3 | one new registry issue; gap/soft miss; cached identity unchanged |
| negative_publication_before_arms | bounded, max_threads=3 | TABLE_VISIBLE_BEFORE_INITIALIZATION |
| negative_missing_release | bounded, max_threads=3 | TABLE_VISIBLE_WITHOUT_RELEASE |
| negative_missing_acquire | bounded, max_threads=3 | TABLE_VISIBLE_WITHOUT_ACQUIRE |
| negative_split_bound_and_pointer | bounded, max_threads=3 | NEW_BOUND_WITH_OLD_TABLE |
| negative_publication_without_ledger_owner | bounded, max_threads=3 | PUBLISHED_TABLE_WITHOUT_OWNER |

`grace` contains eleven cases:

| Case | Exploration | Obligation/witness |
| --- | --- | --- |
| replacement_waits_for_reader_and_stop_blocks_new_entries | bounded, max_threads=4 | register activation before descriptor load; close entries before quiescence/reclaim |
| resume_reloads_descriptor_after_grace | exhaustive, max_threads=2 | completed activation drops address; next entry loads new descriptor |
| starting_cancelled_native_and_retired_membership | bounded, max_threads=3 | Starting blocks receipt; cancellation/native state permits final stop |
| busy_entry_is_legal_and_never_unwrapped | bounded, max_threads=3 | competing entry can refuse Busy; completed views exit |
| native_handoff_and_terminal_shutdown_follow_activation_exit | exhaustive, max_threads=3 | native/terminal transition follows view destruction; current freed only terminally |
| poison_retains_published_and_retired_allocations | exhaustive, max_threads=3 | poison refuses reclamation and new entries, retains both allocations |
| rooted_old_call_survives_and_next_call_revalidates | exhaustive, max_threads=3 | root installed under cell authority; next selection sees new target |
| negative_reclaim_while_old_view_lives | bounded, max_threads=3 | OLD_TABLE_RECLAIMED_BEFORE_GRACE |
| negative_ack_with_live_activation | bounded, max_threads=3 | ACK_WITH_LIVE_JIT_VIEW |
| negative_resume_uses_saved_pre_grace_pointer | bounded, max_threads=2 | RESUME_USES_PRE_GRACE_POINTER |
| negative_unrooted_function_selection | bounded, max_threads=3 | UNROOTED_SELECTED_DEFINITION |

Thread limits include the calling actor. Cases use at most three spawned
actors; the configured bound also covers the model/harness thread.

## Abstractions and limits

Descriptor addresses are finite IDs, not raw pointers. Owned/Retired/Reclaimed
states model the ledger's permission to dereference, without invoking undefined
behavior. Descriptor bound/arm initialization uses separate Relaxed atomics;
Release/Acquire alone must establish their visibility. The separate missing
edge controls allow stale observation. Real immutable descriptor fields would
use ordinary accesses protected by that edge; provenance and actual allocation
lifetime need Miri and the eventual engine fixtures.

The split-publication control deliberately exposes a new bound before its
matching pointer. It demonstrates the rejected representation's mixed-pair
failure, not a claim that every existing pointer-before-bound store order has
that exact history. A single immutable-descriptor publication eliminates the
representation mismatch entirely.

The registry mutex models writer/entry/stop admission and finite membership.
It does not propose adding a mutex to generated walks. Publication ordering is
tested without that mutex in `publication.rs`; lifetime tests separately model
the ledger. Starting owners block a receipt; stop closes admissions; Active
begins before descriptor acquisition and lasts until run-to-return epilogue.
A receipt binds one runtime identity and stop generation, is consumed on reclaim,
and never frees the current allocation. These finite models do not prove actual
JIT register/spill liveness, generated atomic lowering, collector tracing,
unbounded-generation arithmetic or chunk provenance. Type contracts and emitted
code tests must prove the corresponding implementation boundaries.

The function-root case models the root-transfer/redefinition boundary and a
stopped sweep. It is not a JIT cache implementation or a concurrency oracle for
GNU Lisp scheduling. The shared symbol-authority snapshot/root rule remains
required. Same-name/gap and refusal cases compose with the previously qualified
C5 transaction model; they do not replace its arms/name/epoch tests.

`TableId`, `OwnerSlot`, `StopGeneration`, `RetainedBytes`, ownership/admission/
owner/refusal enums and must-use View/JitGracePeriod make modeled states explicit.
The integer atomic ID stands in for the eventual AtomicPtr ABI. This is a model
boundary, not a proposed raw-integer production interface. Compile-negative
tests, not Loom, establish private constructors, GEN1 admission, confinement and
non-conversion from proof/serial entry types.

## N4 HOF selection extension

The separate `hof` target adds seven bounded cases, using the same per-case
receipt/witness helpers and `NMJIT_PREEMPTIONS=2/3` qualification:

| Case | Obligation/witness |
| --- | --- |
| same_counted_reader_covers_mapper_callback_and_private_root | two readers may select coherent old/new callback snapshots; one counted lease covers canonical mapper identity, callback version/definition and private-root installation |
| negative_split_readers_mix_mapper_guard_and_callback_selection | HOF_MIXED_BUILTIN_AND_CALLBACK |
| negative_reader_release_precedes_private_root_contract | HOF_UNROOTED_CALLBACK_SELECTION |
| registration_clock_change_refuses_stale_prepared_builtin | one Acquire snapshot of the existing native registration clock while the symbol reader holds both cells; stale receipts refuse |
| negative_ignored_stamp_accepts_stale_native_registration | HOF_STALE_NATIVE_REGISTRATION |
| bracketed_preparation_refuses_changed_registration_generation | stamp the original verified generation; changed bracketing loads refuse |
| negative_postload_only_blesses_mixed_preparation_generation | HOF_MIXED_PREPARATION_GENERATION |

The mapper and callback are different symbol cells. The writer performs two
separate try-acquired WRITER primitives, releasing between mapper redefinition
and callback redefinition. Either operation can independently return Busy, as
can reader acquisition; each actor attempts finitely many program operations
without retries or progress assumptions. Thus the positive admits old/new
callback selections only when the mapper remains canonical at the exact
callback-selection instant. A new callback with a still-canonical mapper is
legal when the mapper write refused Busy. The split-lease negative can instead
combine an earlier canonical guard with a callback snapshot selected while the
mapper is already noncanonical; no atomic multi-symbol writer is invented.

A mutex models individual counted-word operations and protected snapshot/root
accesses; it is not held across the reader lease. Callback version and
definition update together under their one symbol primitive. Each activation
stays IN_SLICE throughout selection and has its own private home, independently
of other owners' roots. The root negative checks the stronger prescribed
root-before-reader-release contract at release, before installing that home.
It does **not** model a stop, sweep or reclamation during the gap: IN_SLICE
still blocks stop-all, and a real function journal can retain old definitions.
Actual tracing/reclamation obligations remain with the grace/engine fixtures.
These models do not prove the engine's authority-word implementation, memory
ordering, generated HOF lowering or source/definition provenance. Frozen-chain qualification owns the actual case counts and witnesses.

Native registration is a separate semantic publisher, not a symbol writer.
Preparation alone copies its metadata under its existing guard; runtime reads
only the publication clock. Registration writes metadata before its Release
clock update, the semantic commit. An old selection can linearize at its
Acquire observation before a later commit, while a fresh selection after a
joined completed registration must refuse the stale receipt. Neither model
permits a running native-metadata dereference or asserts that symbol authority
synchronizes native registration. The complete suite has 30 cases (17 positive,
13 negative controls); qualification records each case at bounds 2 and 3.

The native writer releases its metadata guard before publishing the clock,
matching the engine. Preparation reads the clock before its guarded copy and
compares the clock again afterward, including refused verification paths.
A postload-only receipt can otherwise bless old verified metadata with a new
registration generation. The mixed-preparation negative names that witness.

HOF negative controls record typed selection violations at the offending
boundary, then complete the same activation/reader teardown and actor joins.
They drop the modeled runtime before asserting the exact named witness. This
keeps a deliberate witness from unwinding live Loom RAII state; it does not
move the observation, remove a contender or change the violated invariant.
Preparation controls likewise join and drop their runtime before asserting.

Writer acquisition, protected update and release are separate modeled mutex
operations and therefore provide optional interleavings. They contain no forced
yield while WRITER is held: such a yield would force the gap reader to observe
Busy before the writer can finish, suppressing the split-reader counterexample.
Busy remains an allowed one-attempt outcome with no retry or progress assumption.
