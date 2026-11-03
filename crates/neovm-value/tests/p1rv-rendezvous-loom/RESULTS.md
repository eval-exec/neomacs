# A1 exploration receipts

These results concern the independent model, not production GC. Both runs used
Loom 0.7.2, Rust 1.96.1, test optimization level 1, one libtest thread,
and the harness CPU/TMPDIR/target setup documented in README.md. There was no
permutation/time limit or checkpoint pruning. A 512-branch path limit was
configured; neither completed run hit it. Positive counts exhaust schedules
at the named preemption bound; negative counts stop at the first exact witness.

| Preemptions | Passed / failed | Positive cases / iterations | Controls / witness-search iterations |
| --- | --- | --- | --- |
| 2 | 52 / 0 | 26 / 46,770 | 26 / 36 |
| 3 | 52 / 0 | 26 / 903,603 | 26 / 36 |

The four binaries passed 11 reader, 29 rendezvous, 7 visibility and 5 wait-graph
tests at each bound. Full logs and machine-readable JSON remain in
`tmp/peer-p1rv/a1/preempt{2,3}.log` and `results.json` in the lane worktree.
Only completed runs of the final fixture are counted. The native actor blocks
on a host-only latch through both epochs; no successful run was truncated.

## Per-test iterations and witnesses

An em dash denotes a positive case. Each control produced the same exact witness
at both bounds. The harness rejects an unrelated assertion, branch-limit panic
or Loom deadlock as a substitute.

| Test | Bound 2 | Bound 3 | Required witness |
| --- | ---: | ---: | --- |
| `abandoned_reader_retains_roots_heap_and_exclusion` | 44 | 77 | — |
| `ack_before_payload_has_an_incomplete_roots_witness` | 3 | 3 | ACK before publication exposed incomplete roots or logs |
| `acknowledgement_needs_no_baton_and_wait_releases_control` | 8 | 10 | — |
| `admission_and_retirement_share_one_gate` | 7 | 9 | — |
| `busy_retry_preserves_exact_publication` | 1 | 1 | — |
| `closing_roots_allow_finished_collector_scan_but_refuse_new_escaping_reader` | 1 | 1 | — |
| `collector_finishes_every_reader_before_resume` | 1 | 1 | — |
| `concurrent_requesters_coalesce` | 20 | 34 | — |
| `early_and_redundant_notifications_are_hints` | 5,990 | 72,260 | — |
| `equal_key_with_different_control_is_refused` | 1 | 1 | — |
| `failed_publication_and_loans_preserve_ack_state` | 1 | 1 | — |
| `final_reader_finish_precedes_resume` | 22 | 33 | — |
| `finishing_one_of_two_readers_does_not_release_last_reader` | 1 | 1 | — |
| `foreign_identity_and_generation_exhaustion_are_atomic_refusals` | 1 | 1 | — |
| `keeper_two_allocating_owners_one_epoch` | 25,884 | 605,868 | — |
| `native_entry_races_request_without_polling_in_native` | 1,316 | 10,734 | — |
| `native_return_races_request_with_idle_resume_debt` | 4,796 | 112,603 | — |
| `native_sleeps_two_epochs_takes_latest_image_once` | 6,074 | 79,760 | — |
| `negative_ack_before_logs` | 1 | 1 | LOG_COVERAGE |
| `negative_ack_before_roots` | 1 | 1 | ROOT_COVERAGE |
| `negative_ack_requires_escrowed_baton` | 1 | 1 | WAIT_GRAPH_CYCLE |
| `negative_collector_resumes_with_reader_alive` | 1 | 1 | READER_STILL_ACTIVE |
| `negative_ignored_abandonment_loses_required_root` | 1 | 1 | unfinished reader lost rooted publication |
| `negative_index_only_reader_decodes_wrong_generation` | 1 | 1 | stale reader decoded another generation |
| `negative_keeper_keeps_allocator_while_waiting_for_ack` | 1 | 1 | WAIT_GRAPH_CYCLE |
| `negative_keeper_keeps_control_while_waiting_for_ack` | 1 | 1 | WAIT_GRAPH_CYCLE |
| `negative_native_ack_before_roots` | 1 | 1 | ROOT_COVERAGE |
| `negative_native_image_log_refolded` | 1 | 1 | DUPLICATE_LOG_FOLD |
| `negative_native_return_omits_idle_resume_debt` | 1 | 1 | RESTORATION_DURING_STOP |
| `negative_native_return_skips_active_stop_gate` | 1 | 1 | NATIVE_RETURN_DURING_STOP |
| `negative_outstanding_loan` | 1 | 1 | OUTSTANDING_LOAN |
| `negative_owner_requests_and_waits_without_releasing_grant` | 1 | 1 | WAIT_GRAPH_CYCLE |
| `negative_resume_before_last_read_changes_observed_heap_word` | 1 | 1 | collector read raced with resumed mutator |
| `negative_retirement_drops_pending_log` | 1 | 1 | LOG_COVERAGE |
| `negative_retirement_removes_frozen_participant` | 1 | 1 | FROZEN_OWNER_REMOVED |
| `negative_stale_ack_epoch` | 1 | 1 | STALE_ACK_EPOCH |
| `negative_stale_publication_generation` | 1 | 1 | STALE_PUBLICATION |
| `negative_stale_registration` | 1 | 1 | STALE_REGISTRATION |
| `negative_stop_ignores_resume_debt` | 1 | 1 | RESTORATION_DURING_STOP |
| `poison_wakes_keeper_and_owner_without_resuming` | 1,258 | 11,443 | — |
| `queued_stop_waits_for_every_resume_debt` | 1 | 1 | — |
| `registration_race_freezes_membership` | 4 | 4 | — |
| `relaxed_ack_observation_has_a_stale_payload_witness` | 3 | 3 | relaxed ACK observation exposed stale roots or logs |
| `relaxed_ack_publication_has_a_stale_payload_witness` | 3 | 3 | relaxed ACK publication exposed stale roots or logs |
| `relaxed_completion_publication_has_a_stale_resume_witness` | 3 | 3 | relaxed completion publication exposed stale collector writes |
| `relaxed_resume_observation_has_a_stale_resume_witness` | 3 | 3 | relaxed resume observation exposed stale collector writes |
| `release_ack_acquire_scan_exposes_roots_and_logs` | 6 | 6 | — |
| `release_completion_acquire_resume_exposes_collector_writes` | 6 | 6 | — |
| `renewed_generation_refuses_stale_admission_and_retirement` | 9 | 12 | — |
| `retired_slot_renewal_rejects_old_key` | 1 | 1 | — |
| `retiring_owner_remains_rooted_through_collection` | 1,316 | 10,734 | — |
| `unfinished_collector_poison_retains_images` | 1 | 1 | — |

## Tested source identity

SHA-256 hashes bind these receipts to the model, harness and locked build inputs.
README/results prose is excluded.

| Input | SHA-256 |
| --- | --- |
| `Cargo.lock` | `7769d3b6326e3c1bfce52b8dbffe5e7e70e66e0239bf1b12e533cf44b5b6ba5c` |
| `Cargo.toml` | `43a787920ea75b108e16cd6fe9ef9e9ff8afa529e7b3b1dc1ef7f7a4a81f87da` |
| `tests/model/mod.rs` | `3706488ea484d1ec8269b3efc532948bab7589041ac6b5aed31577a5c5f32f95` |
| `tests/readers.rs` | `cbefb5d8f95be6d81518c7484c9761e1f3293a8e3e401b7243f4150b17a77229` |
| `tests/rendezvous.rs` | `b9d928c8543bd467233d1ab1ee0da6a071a2d83065f3391a55fd8c9266b770b6` |
| `tests/support/mod.rs` | `cf98d5cb83d0e753a9991352555fa8d5ac777b5434dfcb9638cee2ea4560bf67` |
| `tests/visibility.rs` | `3285582e4cb1375caec17ee8d8fef7c42e419f41de80e3d2df20c17640ade786` |
| `tests/wait_graph.rs` | `b2519a3748f3cb65560e2a6b760e993a86a5bccf83fa977f135a02ef11a02747` |

## R6 event-driven poison extension

The original 52-case receipts above remain the A1 baseline. R6 adds three
positive cases and two exact-witness controls in `tests/poison.rs`. Fresh
bounded runs passed all five binaries at bounds 2 and 3: 57 tests, 0 failures
each. Every original source body and all 52 original iteration/witness receipts
match the reviewed A1 baseline exactly; the manifest only adds the poison test
binary. Full receipts are `tmp/peer-p1rv/a5/loom/results.json` and
`preempt{2,3}.log` in the lane worktree. Bound 2 exhausted 46,800 positive
iterations and found 28 controls in 38 searches; bound 3 exhausted 903,644
positive iterations and found the same controls in 38 searches.

| Test | Bound 2 | Bound 3 | Required witness |
| --- | ---: | ---: | --- |
| `negative_lock_free_poison_can_notify_before_wait_registration` | 1 | 1 | POISON_NOTIFICATION_LOST_WITHOUT_GATE |
| `negative_always_locking_poison_has_same_thread_control_cycle` | 1 | 1 | POISON_SELF_LOCK_WITHOUT_OWNER_CHECK |
| `external_poison_in_predicate_wait_gap_acquires_gate_and_wakes` | 19 | 28 | — |
| `holder_poison_between_predicate_and_wait_is_rechecked_without_self_lock` | 1 | 1 | — |
| `ownership_changes_across_condvar_release_before_later_holder_poison` | 10 | 12 | — |

## Turn-3 admission extension

`tests/admission.rs` models the lock-free slice admission protocol (design:
`tmp/peer-constail/admission-design.md`). Bound 2 passed 8/8 in 72.35s; bound 3
passed 7/8 — the 4-thread deep positive was stopped at 29:08 with no violation
found and no completion in sight (bound 2 exhausts it in 1.7M iterations;
bound 3 is intractable for that one test), and the two remaining small tests
were rerun separately at bound 3. Logs: `tmp/peer-constail/loom/bound-2.log`,
`bound-3.log`, `bound-3-remainder.log`.

| Test | Bound 2 | Bound 3 | Required witness |
| --- | ---: | ---: | --- |
| `owner_slices_never_overlap_the_all_stopped_epoch` | 1,735,625 | stopped 29:08, no violation | — |
| `fenced_authority_scan_and_owner_admission_are_mutually_observable` | 51 | 77 | — |
| `parked_owner_loses_no_request_wake` | 28 | 60 | — |
| `poison_reaches_parked_owner_and_refuses_further_admissions` | 14 | 25 | — |
| `catalog_transition_refuses_inflight_slice_and_excludes_new_ones` | 1 | 1 | — |
| `negative_missing_owner_fence_reorders_admission_past_the_scan` | 1 | 1 | ADMISSION_OWNER_FENCE_REORDER_MISSES_LIVE_SLICE |
| `negative_unfenced_authority_scan_misses_live_slice` | 1 | 1 | ADMISSION_UNFENCED_SCAN_MISSES_LIVE_SLICE |
| `negative_skipped_authority_scan_misses_live_slice` | 1 | 1 | ADMISSION_SKIPPED_SCAN_MISSES_LIVE_SLICE |

The witness predicate is value-based: the model phase word carries a
transition counter (bits 16..) bumped by the closing publish of every
transition window, and a fault is `missed && pre_transition_body` — the
authority scan never observed the owner AND the owner's fenced load read the
pre-transition initial word. This distinguishes a legal post-window admission
from a genuinely missed slice, so the fenced Dekker pair positives stay clean
while each fault still produces its exact witness.
