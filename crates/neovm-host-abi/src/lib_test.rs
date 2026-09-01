use super::*;
use std::collections::HashSet;

#[test]
fn isolate_id_is_hashable_and_equatable() {
    let a = IsolateId(1);
    let b = IsolateId(1);
    let c = IsolateId(2);
    assert_eq!(a, b);
    assert_ne!(a, c);
    let mut set = HashSet::new();
    set.insert(a);
    assert!(set.contains(&b));
    assert!(!set.contains(&c));
}

#[test]
fn primitive_id_and_channel_id_are_copy_and_hash() {
    let p = PrimitiveId(7);
    let p_copy = p;
    assert_eq!(p, p_copy);
    let c = ChannelId(42);
    let c_copy = c;
    assert_eq!(c, c_copy);
    let mut set: HashSet<ChannelId> = HashSet::new();
    set.insert(c);
    assert!(set.contains(&c_copy));
}

#[test]
fn affinity_variants_compare() {
    assert_eq!(Affinity::MainOnly, Affinity::MainOnly);
    assert_ne!(Affinity::MainOnly, Affinity::WorkerSafe);
}

#[test]
fn effect_class_distinguishes_four_categories() {
    let variants = [
        EffectClass::PureRead,
        EffectClass::StateRead,
        EffectClass::StateWrite,
        EffectClass::BlockingIo,
    ];
    // Each is equal to itself; no two of the four are equal.
    for (i, a) in variants.iter().enumerate() {
        for (j, b) in variants.iter().enumerate() {
            if i == j {
                assert_eq!(a, b);
            } else {
                assert_ne!(a, b);
            }
        }
    }
}

#[test]
fn primitive_descriptor_clone_and_compare() {
    let d = PrimitiveDescriptor {
        name: "test-primitive",
        affinity: Affinity::WorkerSafe,
        effect: EffectClass::PureRead,
        can_trigger_gc: false,
        can_reenter_elisp: false,
        deterministic: true,
    };
    let d2 = d.clone();
    assert_eq!(d, d2);
}

#[test]
fn lisp_value_default_is_empty() {
    let v = LispValue::default();
    assert!(v.bytes.is_empty());
    let v2 = LispValue {
        bytes: vec![1, 2, 3],
    };
    assert_ne!(v, v2);
}

#[test]
fn signal_round_trips_through_clone_and_compare() {
    let s = Signal {
        symbol: "wrong-type-argument".to_string(),
        data: Some("expected list".to_string()),
    };
    let s2 = s.clone();
    assert_eq!(s, s2);
    let s3 = Signal {
        symbol: "wrong-type-argument".to_string(),
        data: None,
    };
    assert_ne!(s, s3);
}

#[test]
fn host_error_display_writes_message() {
    let e = HostError {
        message: "handle 42 not found".to_string(),
    };
    assert_eq!(e.to_string(), "handle 42 not found");
    // HostError is also a std::error::Error so it can be
    // wrapped in Box<dyn Error> or returned from ?-propagating
    // functions.
    let boxed: Box<dyn std::error::Error> = Box::new(e.clone());
    assert_eq!(boxed.to_string(), "handle 42 not found");
}

#[test]
fn snapshot_request_carries_optional_revision_hint() {
    let r = SnapshotRequest {
        isolate: IsolateId(1),
        handle: 99,
        revision_hint: None,
    };
    assert!(r.revision_hint.is_none());
    let r_hinted = SnapshotRequest {
        revision_hint: Some(5),
        ..r.clone()
    };
    assert_eq!(r_hinted.revision_hint, Some(5));
}

#[test]
fn snapshot_blob_default_is_empty_and_revision_zero() {
    let b = SnapshotBlob::default();
    assert_eq!(b.revision, 0);
    assert!(b.bytes.is_empty());
}

#[test]
fn patch_result_variants_compare() {
    let applied = PatchResult::Applied { new_revision: 7 };
    let rejected = PatchResult::Rejected {
        current_revision: 5,
    };
    assert_ne!(applied, rejected);
    assert_eq!(applied, PatchResult::Applied { new_revision: 7 });
    assert_ne!(applied, PatchResult::Applied { new_revision: 8 });
}

#[test]
fn task_options_default_matches_documented_defaults() {
    let opts = TaskOptions::default();
    assert_eq!(opts.priority, TaskPriority::Default);
    assert_eq!(opts.affinity, Affinity::WorkerSafe);
    assert!(opts.name.is_none());
    assert!(opts.timeout.is_none());
}

#[test]
fn task_error_failed_carries_signal() {
    let err = TaskError::Failed(Signal {
        symbol: "args-out-of-range".to_string(),
        data: None,
    });
    let copy = err.clone();
    assert_eq!(err, copy);
    let timed_out = TaskError::TimedOut;
    assert_ne!(err, timed_out);
}

#[test]
fn select_op_send_carries_payload() {
    let payload = LispValue {
        bytes: vec![0xde, 0xad],
    };
    let op = SelectOp::Send(ChannelId(3), payload.clone());
    match op {
        SelectOp::Send(ch, value) => {
            assert_eq!(ch, ChannelId(3));
            assert_eq!(value, payload);
        }
        _ => panic!("expected Send"),
    }
}

#[test]
fn select_result_ready_reports_op_index_and_value() {
    let result = SelectResult::Ready {
        op_index: 2,
        value: Some(LispValue { bytes: vec![9] }),
    };
    match result {
        SelectResult::Ready { op_index, value } => {
            assert_eq!(op_index, 2);
            assert_eq!(value.unwrap().bytes, vec![9]);
        }
        _ => panic!("expected Ready"),
    }
}
