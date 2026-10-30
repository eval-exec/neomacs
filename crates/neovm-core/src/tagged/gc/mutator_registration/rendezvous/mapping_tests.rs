//! Every reviewed A1 control remains named, with an executable production
//! check where one exists. Fixed ordering/linearity are not runtime race detectors.
#![forbid(unsafe_code)]

use super::*;
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MappingClass {
    RuntimeRefusal,
    QueueOrWait,
    LinearOwnership,
    FixedOrdering,
    LockOrder,
}

struct Control {
    model: &'static str,
    witness: &'static str,
    class: MappingClass,
    check_name: &'static str,
    check: Option<fn()>,
    expectation: &'static str,
}

fn controls() -> [Control; 31] {
    use MappingClass::*;
    let stale: fn() = stale_ack_identity_generation_duplicate_and_missing_image_are_refused;
    let forced: fn() = two_owners_pause_before_ack_and_while_restoring_across_two_epochs;
    let native: fn() = native_owner_sleeps_two_epochs_latest_image_and_synthetic_work_once;
    let debt: fn() = native_idle_restore_debt_queues_request_before_any_new_epoch;
    let reader: fn() = failure::completion_requires_trace_and_every_explicit_reader_finish;
    let retired: fn() = retiring_image_and_starting_owner_keep_exact_frozen_membership;
    let frozen: fn() =
        failure::stale_registration_exhaustion_and_frozen_removal_refuse_before_trace;
    [
        Control {
            model: "negative_ack_before_roots",
            witness: "ROOT_COVERAGE",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "missing roots refuse Publication before AllStopped",
        },
        Control {
            model: "negative_ack_before_logs",
            witness: "LOG_COVERAGE",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "missing owned GC image refuses Publication; actual census completeness is separate",
        },
        Control {
            model: "negative_outstanding_loan",
            witness: "OUTSTANDING_LOAN",
            class: RuntimeRefusal,
            check_name: "worker_loan",
            check: Some(live_worker_loan_blocks_ack_and_preserves_running_owner),
            expectation: "Loan refusal preserves original running owner and loan",
        },
        Control {
            model: "negative_stale_registration",
            witness: "STALE_REGISTRATION",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "Identity or StaleRegistration before accessing peer image",
        },
        Control {
            model: "negative_stale_ack_epoch",
            witness: "STALE_ACK_EPOCH",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "wrong wanted/coverage epoch refuses Publication with original record retained",
        },
        Control {
            model: "negative_native_ack_before_roots",
            witness: "ROOT_COVERAGE",
            class: QueueOrWait,
            check_name: "native_publishing",
            check: Some(native_publication_is_not_stopped_until_root_and_image_commit),
            expectation: "Publishing cannot count as complete roots; keeper remains pending",
        },
        Control {
            model: "negative_stop_ignores_resume_debt",
            witness: "RESTORATION_DURING_STOP",
            class: QueueOrWait,
            check_name: "forced",
            check: Some(forced),
            expectation: "request Queued and no epoch advance during two paused Resuming owners",
        },
        Control {
            model: "negative_native_return_omits_idle_resume_debt",
            witness: "RESTORATION_DURING_STOP",
            class: QueueOrWait,
            check_name: "native_idle_debt",
            check: Some(debt),
            expectation: "native return from Idle reserves debt before racing request queues",
        },
        Control {
            model: "negative_native_image_log_refolded",
            witness: "DUPLICATE_LOG_FOLD",
            class: LinearOwnership,
            check_name: "native_two_epochs",
            check: Some(native),
            expectation: "same native image through two epochs; known immediate work consumed once, latest response taken once",
        },
        Control {
            model: "negative_retirement_drops_pending_log",
            witness: "LOG_COVERAGE",
            class: LinearOwnership,
            check_name: "retiring",
            check: Some(retired),
            expectation: "owned retiring image retains its seeded pending work in the canonical census; real GC folding is A5",
        },
        Control {
            model: "negative_retirement_removes_frozen_participant",
            witness: "FROZEN_OWNER_REMOVED",
            class: RuntimeRefusal,
            check_name: "frozen",
            check: Some(frozen),
            expectation: "retired frozen member refuses Identity without shrinking the frozen set",
        },
        Control {
            model: "negative_stale_publication_generation",
            witness: "STALE_PUBLICATION",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "root identity/publication generation mismatch refuses Identity before decoding",
        },
        Control {
            model: "negative_native_return_skips_active_stop_gate",
            witness: "NATIVE_RETURN_DURING_STOP",
            class: QueueOrWait,
            check_name: "native_active_gate",
            check: Some(native_return_waits_for_active_stop_before_restoration),
            expectation: "native return stays stopped until active epoch completes",
        },
        Control {
            model: "negative_collector_resumes_with_reader_alive",
            witness: "READER_STILL_ACTIVE",
            class: RuntimeRefusal,
            check_name: "reader_complete",
            check: Some(reader),
            expectation: "Readers refusal after zero or one of two explicit completions; completion remains uncommitted",
        },
        Control {
            model: "negative_resume_before_last_read_changes_observed_heap_word",
            witness: "collector read raced with resumed mutator",
            class: RuntimeRefusal,
            check_name: "reader_forget",
            check: Some(failure::forgotten_reader_refuses_resume_and_retains_roots),
            expectation: "actual outstanding transport reader prevents completion and keeps roots/image",
        },
        Control {
            model: "negative_ignored_abandonment_loses_required_root",
            witness: "unfinished reader lost rooted publication",
            class: RuntimeRefusal,
            check_name: "reader_drop",
            check: Some(failure::unfinished_reader_drop_refuses_resume_and_retains_roots),
            expectation: "abandonment refuses Readers and retains root publication/heap exclusion",
        },
        Control {
            model: "negative_index_only_reader_decodes_wrong_generation",
            witness: "stale reader decoded another generation",
            class: RuntimeRefusal,
            check_name: "stale",
            check: Some(stale),
            expectation: "stale key cannot admit a root reader; its callback never executes",
        },
        Control {
            model: "relaxed_ack_publication_has_a_stale_payload_witness",
            witness: "relaxed ACK publication exposed stale roots or logs",
            class: FixedOrdering,
            check_name: "A1 visibility",
            check: None,
            expectation: "payload before mutex ACK commit / Release publication; no runtime detector can substitute for this edge",
        },
        Control {
            model: "relaxed_ack_observation_has_a_stale_payload_witness",
            witness: "relaxed ACK observation exposed stale roots or logs",
            class: FixedOrdering,
            check_name: "A1 visibility",
            check: None,
            expectation: "Acquire/control gate before using root/image payload",
        },
        Control {
            model: "ack_before_payload_has_an_incomplete_roots_witness",
            witness: "ACK before publication exposed incomplete roots or logs",
            class: FixedOrdering,
            check_name: "forced",
            check: Some(forced),
            expectation: "forced preparation interval grants no stop authority before immutable publication commit",
        },
        Control {
            model: "relaxed_completion_publication_has_a_stale_resume_witness",
            witness: "relaxed completion publication exposed stale collector writes",
            class: FixedOrdering,
            check_name: "A1 visibility",
            check: None,
            expectation: "response/reader checks before Release completed_epoch; no runtime weak-atomic switch",
        },
        Control {
            model: "relaxed_resume_observation_has_a_stale_resume_witness",
            witness: "relaxed resume observation exposed stale collector writes",
            class: FixedOrdering,
            check_name: "A1 visibility",
            check: None,
            expectation: "Acquire completion before response application",
        },
        Control {
            model: "negative_ack_requires_escrowed_baton",
            witness: "WAIT_GRAPH_CYCLE",
            class: LockOrder,
            check_name: "forced",
            check: Some(forced),
            expectation: "two owners publish while keeper retains real Baton, with no Baton input on owner ACK",
        },
        Control {
            model: "negative_keeper_keeps_control_while_waiting_for_ack",
            witness: "WAIT_GRAPH_CYCLE",
            class: LockOrder,
            check_name: "forced",
            check: Some(forced),
            expectation: "owner ACK progresses through forced publication/keeper waits using released control gate",
        },
        Control {
            model: "negative_keeper_keeps_allocator_while_waiting_for_ack",
            witness: "WAIT_GRAPH_CYCLE",
            class: LockOrder,
            check_name: "forced",
            check: Some(forced),
            expectation: "synthetic run slice ends before keeper wait; no allocator or heap authority enters stop wait",
        },
        Control {
            model: "negative_owner_requests_and_waits_without_releasing_grant",
            witness: "WAIT_GRAPH_CYCLE",
            class: LockOrder,
            check_name: "worker_loan",
            check: Some(live_worker_loan_blocks_ack_and_preserves_running_owner),
            expectation: "outstanding transferable loan refuses ACK until explicitly returned; Rust run borrow separately prevents ACK",
        },
        Control {
            model: "negative_lock_free_poison_can_notify_before_wait_registration",
            witness: "POISON_NOTIFICATION_LOST_WITHOUT_GATE",
            class: LockOrder,
            check_name: "external_poison_gap",
            check: Some(failure::external_poison_between_predicate_and_wait_is_event_driven),
            expectation: "external poison obtains predicate gate and wakes a real untimed wait; no notify can pass atomic release/register",
        },
        Control {
            model: "negative_always_locking_poison_has_same_thread_control_cycle",
            witness: "POISON_SELF_LOCK_WITHOUT_OWNER_CHECK",
            class: LockOrder,
            check_name: "same_thread_poison_gap",
            check: Some(failure::lost_poison_notification_between_predicate_and_wait_fails_closed),
            expectation: "holder poison recognizes its tracked identity and wait rechecks it without recursive gate acquisition",
        },
        Control {
            model: "negative_missing_owner_fence_reorders_admission_past_the_scan",
            witness: "ADMISSION_OWNER_FENCE_REORDER_MISSES_LIVE_SLICE",
            class: FixedOrdering,
            check_name: "admitted_slice",
            check: None,
            expectation: "owner admission store precedes its SeqCst fence and the following phase load, so a published stop or block is observed before the slice body runs",
        },
        Control {
            model: "negative_unfenced_authority_scan_misses_live_slice",
            witness: "ADMISSION_UNFENCED_SCAN_MISSES_LIVE_SLICE",
            class: FixedOrdering,
            check_name: "admitted_slice",
            check: None,
            expectation: "authority publishes the phase word Release then fences SeqCst before scanning admission words; guard-drop publication alone does not order the scan",
        },
        Control {
            model: "negative_skipped_authority_scan_misses_live_slice",
            witness: "ADMISSION_SKIPPED_SCAN_MISSES_LIVE_SLICE",
            class: RuntimeRefusal,
            check_name: "admission_scan",
            check: Some(
                crate::tagged::gc::mutator_registration::admission_probe::in_slice_admission_word_refuses_catalog_transition,
            ),
            expectation: "an IN_SLICE admission word refuses the catalog transition even when every gated precheck passes",
        },
    ]
}

#[test]
fn every_a1_negative_control_has_an_explicit_production_correspondence() {
    let reviewed = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/p1rv-rendezvous-loom/RESULTS.md"
    ));
    let rows = controls();
    let mut names = BTreeSet::new();
    for row in &rows {
        assert!(names.insert(row.model), "duplicate model control mapping");
        assert!(
            reviewed.contains(&format!("`{}`", row.model)),
            "mapping must name a reviewed model control"
        );
        assert!(
            reviewed.contains(row.witness),
            "mapping must preserve the reviewed witness"
        );
        assert!(!row.expectation.is_empty());
        if row.class != MappingClass::FixedOrdering {
            assert!(
                row.check.is_some(),
                "runtime/ownership/lock correspondence needs an executable check"
            );
        }
    }
    assert_eq!(names.len(), 31);
    assert_eq!(
        rows.iter()
            .filter(|row| row.class == MappingClass::FixedOrdering)
            .count(),
        7
    );
    assert_eq!(
        rows.iter()
            .filter(|row| row.class == MappingClass::LockOrder)
            .count(),
        6
    );
    // Guard against omitting a reviewed case while merely retaining 28 rows.
    let reviewed_names: BTreeSet<_> = reviewed
        .lines()
        .filter_map(|line| {
            let name = line.strip_prefix("| `")?.split('`').next()?;
            let cells: Vec<_> = line.split('|').collect();
            (cells.get(4)?.trim() != "—").then_some(name)
        })
        .collect();
    assert_eq!(names, reviewed_names);
}

#[test]
fn mapped_refusals_queues_and_linear_handoffs_execute_real_protocol_checks() {
    // Several independent model corruptions are rejected at the same sealed
    // production seam. Execute each complete production regression once; every
    // table row still names its own exact model witness and expected behavior.
    let mut executed = BTreeSet::new();
    for row in controls() {
        if let Some(check) = row.check {
            if executed.insert(row.check_name) {
                check();
            }
        }
    }
    assert!(
        executed.len() >= 10,
        "a string table is insufficient runtime coverage"
    );
}
