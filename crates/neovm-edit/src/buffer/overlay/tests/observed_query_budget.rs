//! Cold overlay queries must not enroll distant property owners in a capture.
//!
//! Fixtures allocate every plist spine independently. A missing certificate is
//! a failure: capture overflow must never turn a large read set into "zero".
use super::*;
use neovm_value::tagged::collection_reads::{self, CollectionReads};

const QUERY_OWNER_BUDGET: usize = 8;

fn owner_count(reads: &CollectionReads) -> usize {
    let mut count = 0;
    assert!(reads.all_dependency_bits(|_| {
        count += 1;
        true
    }));
    count
}

fn observed_query<T>(label: &str, query: impl FnOnce() -> T) -> T {
    let (answer, reads) = collection_reads::capture(query);
    let reads = reads.unwrap_or_else(|| {
        panic!("{label}: no coherent certificate; overflow is not a zero-owner result")
    });
    assert!(
        reads.unchanged(),
        "{label}: query invalidated its own inputs"
    );
    let count = owner_count(&reads);
    assert!(
        count <= QUERY_OWNER_BUDGET,
        "{label}: observed {count} owners; budget {QUERY_OWNER_BUDGET} is independent of overlay count"
    );
    answer
}

fn overlay_with_plist(start: usize, end: usize, plist: Value) -> Value {
    Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist,
        buffer: Some(BufferId(1)),
        start,
        end,
        front_advance: false,
        rear_advance: false,
    })
}

fn unique_plist_overlays(count: usize) -> OverlayList {
    let face = Value::symbol("face");
    let priority = Value::symbol("priority");
    let default_face = Value::symbol("default");
    let mut list = OverlayList::new();
    for index in 0..count {
        // Allocate FOUR distinct cons owners per overlay, even when values
        // repeat. Sharing one fixture plist would hide the observed-read cost.
        let plist = Value::list(vec![
            face,
            default_face,
            priority,
            Value::fixnum((index % 3) as i64),
        ]);
        let start = 10 + index * 4;
        list.insert_overlay(overlay_with_plist(start, start + 2, plist));
    }
    list
}

#[derive(Clone, Copy, Debug)]
enum FirstQuery {
    CategoryPresence,
    NextBoundary,
    PreviousBoundary,
}

fn check_first_query(count: usize, query: FirstQuery) {
    crate::test_utils::init_test_tracing();
    check_observation_probe();
    let category = Value::symbol("category");
    let list = unique_plist_overlays(count);
    let start = 10 + (count / 2) * 4;
    let label = format!("first {query:?} with {count} unique plists");
    // No prior predicate or boundary query warms any derived index. This also
    // permits a design that maintains its index eagerly during attachment.
    match query {
        FirstQuery::CategoryPresence => {
            assert!(!observed_query(&label, || list.may_contain_property(category)));
        }
        FirstQuery::NextBoundary => {
            assert_eq!(
                observed_query(&label, || {
                    list.next_boundary_after_until_emacs_byte_pos(
                        emacs_byte_pos(start),
                        emacs_byte_pos(start + 3),
                    )
                }),
                Some(emacs_byte_pos(start + 2))
            );
        }
        FirstQuery::PreviousBoundary => {
            assert_eq!(
                observed_query(&label, || {
                    list.previous_boundary_before_since_emacs_byte_pos(
                        emacs_byte_pos(start + 3),
                        emacs_byte_pos(start),
                    )
                }),
                Some(emacs_byte_pos(start + 2))
            );
        }
    }
}

macro_rules! first_query_case {
    ($name:ident, $count:expr, $query:expr) => {
        #[test]
        fn $name() {
            check_first_query($count, $query);
        }
    };
}

first_query_case!(
    first_category_capture_budget_2000,
    2_000,
    FirstQuery::CategoryPresence
);
first_query_case!(
    first_category_capture_budget_16000,
    16_000,
    FirstQuery::CategoryPresence
);
first_query_case!(
    first_category_capture_budget_64000,
    64_000,
    FirstQuery::CategoryPresence
);
first_query_case!(
    first_next_boundary_capture_budget_2000,
    2_000,
    FirstQuery::NextBoundary
);
first_query_case!(
    first_next_boundary_capture_budget_16000,
    16_000,
    FirstQuery::NextBoundary
);
first_query_case!(
    first_next_boundary_capture_budget_64000,
    64_000,
    FirstQuery::NextBoundary
);
first_query_case!(
    first_previous_boundary_capture_budget_2000,
    2_000,
    FirstQuery::PreviousBoundary
);
first_query_case!(
    first_previous_boundary_capture_budget_16000,
    16_000,
    FirstQuery::PreviousBoundary
);
first_query_case!(
    first_previous_boundary_capture_budget_64000,
    64_000,
    FirstQuery::PreviousBoundary
);

#[test]
fn first_previous_boundary_preserves_lower_limit_and_empty_endpoints() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    for (start, end, pos, limit, expected) in [
        (1, 10, 20, 10, Some(10)),
        (1, 15, 20, 10, Some(15)),
        (1, 9, 20, 10, None),
        (10, 10, 20, 10, Some(10)),
        (20, 20, 20, 10, None),
        (1, 15, 10, 10, None),
        (1, 15, 9, 10, None),
    ] {
        let mut list = OverlayList::new();
        list.insert_overlay(overlay_with_plist(
            start,
            end,
            Value::list(vec![face, Value::symbol("default")]),
        ));
        assert_eq!(
            observed_query("first bounded predecessor", || {
                list.previous_boundary_before_since_emacs_byte_pos(
                    emacs_byte_pos(pos),
                    emacs_byte_pos(limit),
                )
            }),
            expected.map(emacs_byte_pos),
            "overlay [{start},{end}], pos={pos}, lower limit={limit}"
        );
    }
}

#[test]
fn observation_probe_counts_independent_plist_spines() {
    crate::test_utils::init_test_tracing();
    check_observation_probe();
}

fn check_observation_probe() {
    let face = Value::symbol("face");
    let plists: Vec<_> = (0..8)
        .map(|index| Value::list(vec![face, Value::fixnum(index)]))
        .collect();
    let (keys, reads) = collection_reads::capture(|| {
        plists
            .iter()
            .map(|plist| {
                let key = plist.cons_car();
                let value_cell = plist.cons_cdr();
                let _ = value_cell.cons_car();
                assert!(value_cell.cons_cdr().is_nil());
                key
            })
            .collect::<Vec<_>>()
    });
    assert!(keys.iter().all(|key| *key == face));
    let reads = reads.expect("the small explicit projection must have a certificate");
    assert_eq!(owner_count(&reads), 16, "two unique cons owners per plist");
    for plist in plists {
        assert!(reads.contains_owner_for_test(plist.bits()));
    }
}

fn check_small_answers(
    list: &OverlayList,
    category: Value,
    present: bool,
    first: Option<usize>,
    last: Option<usize>,
) {
    assert_eq!(
        observed_query("small category presence", || list
            .may_contain_property(category)),
        present
    );
    assert_eq!(
        observed_query("small first boundary", || {
            list.next_boundary_after_emacs_byte_pos(emacs_byte_pos(0))
        }),
        first.map(emacs_byte_pos)
    );
    assert_eq!(
        observed_query("small last boundary", || {
            list.previous_boundary_before_emacs_byte_pos(emacs_byte_pos(1_000))
        }),
        last.map(emacs_byte_pos)
    );
    list.assert_index_invariants_for_test();
}

#[test]
fn observed_overlay_queries_track_property_membership_and_coordinate_mutations() {
    crate::test_utils::init_test_tracing();
    let category = Value::symbol("category");
    let face = Value::symbol("face");
    let mut list = OverlayList::new();
    let first = overlay_with_plist(10, 20, Value::list(vec![face, Value::symbol("default")]));
    let second = overlay_with_plist(30, 35, Value::NIL);
    list.insert_overlay(first);
    list.insert_overlay(second);
    check_small_answers(&list, category, false, Some(10), Some(35));

    list.overlay_put(first, category, Value::symbol("query-budget-category"))
        .unwrap();
    check_small_answers(&list, category, true, Some(10), Some(35));
    list.overlay_put(first, category, Value::NIL).unwrap();
    // GNU overlay-put retains an existing key when its value becomes nil.
    assert_eq!(list.overlay_get_named(first, category), Some(Value::NIL));
    check_small_answers(&list, category, true, Some(10), Some(35));
    assert!(list.delete_overlay(first));
    check_small_answers(&list, category, false, Some(30), Some(35));
    list.move_overlay_to_emacs_byte_range(second, emacs_byte_range(40, 45));
    check_small_answers(&list, category, false, Some(40), Some(45));
    // Prepending an absent nil key replaces the plist head even though the
    // GNU display-change return from overlay_put is false.
    assert!(!list.overlay_put(second, category, Value::NIL).unwrap());
    check_small_answers(&list, category, true, Some(40), Some(45));
    list.delete_all_overlays();
    check_small_answers(&list, category, false, None, None);

    let third = overlay_with_plist(100, 104, Value::list(vec![category, Value::NIL]));
    let fourth = overlay_with_plist(110, 115, Value::list(vec![face, Value::symbol("default")]));
    // from_dump uses the production batch-attachment path.
    let mut restored = OverlayList::from_dump(vec![third, fourth]);
    check_small_answers(&restored, category, true, Some(100), Some(115));
    restored.remap_overlay_positions(&[
        OverlayPositionRemap {
            overlay: third,
            old_range: emacs_byte_range(100, 104),
            range: emacs_byte_range(200, 204),
            old_begin: CharPos0::new(100),
            new_begin: CharPos0::new(200),
        },
        OverlayPositionRemap {
            overlay: fourth,
            old_range: emacs_byte_range(110, 115),
            range: emacs_byte_range(210, 215),
            old_begin: CharPos0::new(110),
            new_begin: CharPos0::new(210),
        },
    ]);
    check_small_answers(&restored, category, true, Some(200), Some(215));
    assert!(restored.delete_overlay(third));
    check_small_answers(&restored, category, false, Some(210), Some(215));
    restored.delete_all_overlays();
    check_small_answers(&restored, category, false, None, None);
}

#[test]
fn shared_overlay_property_values_keep_original_observation_authority() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let payload = Value::list(vec![Value::symbol(":foreground"), Value::string("red")]);
    let first = overlay_with_plist(10, 20, Value::list(vec![face, payload]));
    let second = overlay_with_plist(30, 40, Value::list(vec![face, payload]));
    let mut list = OverlayList::new();
    list.insert_overlay(first);
    list.insert_overlay(second);
    let (_, reads) = collection_reads::capture(|| {
        for overlay in [first, second] {
            let value = list.overlay_get_named(overlay, face).unwrap();
            assert_eq!(value.bits(), payload.bits());
            assert_eq!(value.cons_car(), Value::symbol(":foreground"));
        }
    });
    let reads = reads.expect("the small shared-value projection must be coherent");
    assert!(reads.contains_owner_for_test(first.bits()));
    assert!(reads.contains_owner_for_test(second.bits()));
    assert!(reads.contains_owner_for_test(payload.bits()));
    assert!(reads.unchanged());
    payload.set_car(Value::symbol(":background"));
    assert!(
        !reads.unchanged(),
        "an aliased original payload mutation must invalidate"
    );
    list.overlay_put(first, face, Value::NIL).unwrap();
    assert_eq!(list.overlay_get_named(first, face), Some(Value::NIL));
    assert_eq!(
        list.overlay_get_named(second, face).unwrap().bits(),
        payload.bits()
    );
}

// A single overlay-put already knows whether it updated an existing pair or
// prepended one new pair. Neither case needs another traversal to discover the
// complete key set. Count physical helper work rather than distinct captured
// owners: repeating a walk of the same plist would deduplicate in a certificate.
#[derive(Clone, Copy, Debug)]
enum SinglePropertyWrite {
    Update,
    Noop,
    Insert,
    InsertNil,
}

fn check_single_property_write_key_index_work(published: bool, write: SinglePropertyWrite) {
    use crate::buffer::overlay_index::{
        property_index_plist_steps, reset_property_index_plist_steps,
    };

    crate::test_utils::init_test_tracing();
    const FILLER_PAIRS: usize = 64;
    let face = Value::symbol("face");
    let filler = Value::symbol("r1-write-work-filler");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let mut pairs = vec![face, bold];
    // Duplicate filler keys are legal plist data and keep the conservative
    // filter sparse while making redundant whole-plist traversals visible.
    for index in 0..FILLER_PAIRS {
        pairs.extend([filler, Value::fixnum(index as i64)]);
    }
    let overlay = overlay_with_plist(10, 20, Value::list(pairs));
    let mut list = OverlayList::new();
    reset_property_index_plist_steps();
    list.insert_overlay(overlay);
    let (direct, masks) = property_index_plist_steps();
    // Positive control: the counter still measures a real key extraction
    // after the optimized single-property route stops using that helper.
    assert!(direct >= FILLER_PAIRS + 1);
    assert_eq!(masks, 0, "attachment must leave the endpoint index cold");

    let bounds = EmacsByteRange::from_usize(0, 30);
    if published {
        let endpoints: Vec<_> = list
            .index
            .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::unfiltered())
            .collect();
        assert_eq!(endpoints.len(), 2);
        // A separate positive control covers the other physical plist walker.
        assert!(property_index_plist_steps().1 > 0);
    }

    let added = if published {
        (100_000..104_096)
            .map(Value::fixnum)
            .find(|property| {
                list.index
                    .endpoint_records_strictly_within(
                        bounds,
                        OverlayPropertyFilter::for_properties([*property]),
                    )
                    .next()
                    .is_none()
            })
            .expect("the two existing key classes cannot occupy every filter bit")
    } else {
        Value::fixnum(100_000)
    };
    assert!(!list.index.may_contain_property(added));
    let (property, value, changed) = match write {
        SinglePropertyWrite::Update => (face, italic, true),
        SinglePropertyWrite::Noop => (face, bold, false),
        SinglePropertyWrite::Insert => (added, Value::T, true),
        SinglePropertyWrite::InsertNil => (added, Value::NIL, false),
    };

    reset_property_index_plist_steps();
    assert_eq!(list.overlay_put(overlay, property, value).unwrap(), changed);
    let work = property_index_plist_steps();
    assert_eq!(
        work,
        (0, 0),
        "single-property {write:?}, published={published}: redundant key/mask plist walks"
    );

    assert_eq!(list.overlay_get(overlay, &property), Some(value));
    assert!(list.index.may_contain_property(face));
    assert!(list.index.may_contain_property(filler));
    assert_eq!(
        list.index.may_contain_property(added),
        matches!(
            write,
            SinglePropertyWrite::Insert | SinglePropertyWrite::InsertNil
        )
    );
    // Do this only after reading the work counter: an invariant check and a
    // cold endpoint publication legitimately traverse their source plists.
    list.assert_index_invariants_for_test();
    let carriers: Vec<_> = list
        .index
        .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::for_properties([property]))
        .collect();
    assert_eq!(
        carriers.len(),
        2,
        "both endpoint masks must retain the written key"
    );
    assert!(
        carriers
            .iter()
            .all(|endpoint| endpoint.overlay.bits() == overlay.bits())
    );
}

macro_rules! single_property_write_key_index_work_case {
    ($name:ident, $published:expr, $write:expr) => {
        #[test]
        fn $name() {
            check_single_property_write_key_index_work($published, $write);
        }
    };
}

single_property_write_key_index_work_case!(
    single_property_write_key_index_work_cold_update,
    false,
    SinglePropertyWrite::Update
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_cold_noop,
    false,
    SinglePropertyWrite::Noop
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_cold_insert,
    false,
    SinglePropertyWrite::Insert
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_cold_insert_nil,
    false,
    SinglePropertyWrite::InsertNil
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_published_update,
    true,
    SinglePropertyWrite::Update
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_published_noop,
    true,
    SinglePropertyWrite::Noop
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_published_insert,
    true,
    SinglePropertyWrite::Insert
);
single_property_write_key_index_work_case!(
    single_property_write_key_index_work_published_insert_nil,
    true,
    SinglePropertyWrite::InsertNil
);

#[test]
fn single_property_write_key_index_work_multilevel_pending_shifts() {
    use crate::buffer::overlay_index::{
        property_index_plist_steps, reset_property_index_plist_steps,
    };

    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlays: Vec<_> = (0..600)
        .map(|index| overlay_with_plist(index * 4 + 1, index * 4 + 3, Value::NIL))
        .collect();
    for overlay in &overlays {
        list.insert_overlay(*overlay);
    }
    let bounds = EmacsByteRange::from_usize(0, 100_000);
    // 1,200 endpoints span leaves and two branch levels. Publish before
    // leaving lazy position shifts pending across the suffix.
    assert_eq!(
        list.index
            .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::unfiltered())
            .count(),
        1_200
    );
    list.adjust_for_insert_at_emacs_byte_pos(EmacsBytePos::new(600), EmacsByteLen::new(5), false);

    let property = Value::symbol("r1-multilevel-added-key");
    assert!(
        list.index
            .endpoint_records_strictly_within(
                bounds,
                OverlayPropertyFilter::for_properties([property]),
            )
            .next()
            .is_none()
    );
    let carriers = [100, 150, 300, 599];
    reset_property_index_plist_steps();
    for index in carriers {
        assert!(
            !list
                .overlay_put(overlays[index], property, Value::NIL)
                .unwrap()
        );
    }
    assert_eq!(property_index_plist_steps(), (0, 0));

    let mut actual: Vec<_> = list
        .index
        .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::for_properties([property]))
        .map(|endpoint| (endpoint.overlay.bits(), endpoint.position.get()))
        .collect();
    let mut expected: Vec<_> = carriers
        .into_iter()
        .flat_map(|index| {
            [index * 4 + 1, index * 4 + 3].map(|position| {
                (
                    overlays[index].bits(),
                    position + if position >= 600 { 5 } else { 0 },
                )
            })
        })
        .collect();
    actual.sort_unstable();
    expected.sort_unstable();
    assert_eq!(actual, expected);
    list.assert_index_invariants_for_test();
}

// The live owner's membership and stable range are a single operation-local
// fact. Count actual range/contains entry points, excluding snapshot indices.
fn check_single_property_write_owner_resolution(cached_snapshot: bool, write: SinglePropertyWrite) {
    use crate::buffer::overlay_index::{
        finish_owner_query_count_for_test, start_owner_query_count_for_test,
    };
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let duplicate = Value::symbol("r1-owner-loan-duplicate");
    let added = Value::symbol("priority");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let shadow = Value::symbol("shadow");
    let overlay = overlay_with_plist(
        10,
        20,
        Value::list(vec![
            face,
            bold,
            face,
            shadow,
            duplicate,
            Value::T,
            duplicate,
            Value::NIL,
        ]),
    );
    let mut list = OverlayList::new();
    list.insert_overlay(overlay);
    let digest_before = list.content_digest();
    if cached_snapshot {
        // Leave only the cache's own strong reference, so the ordinary put
        // must mirror into the existing copy rather than drop/rebuild it.
        drop(list.snapshot());
    }
    reset_overlay_snapshot_copy_count();
    let (property, value, changed) = match write {
        SinglePropertyWrite::Noop => (face, bold, false),
        SinglePropertyWrite::Update => (face, italic, true),
        SinglePropertyWrite::Insert => (added, Value::fixnum(7), true),
        SinglePropertyWrite::InsertNil => (added, Value::NIL, false),
    };
    // The live list is fully constructed and stays at this address until
    // finish. No query made by a different snapshot index is charged here.
    start_owner_query_count_for_test(&list.index);
    let actual_changed = list.overlay_put(overlay, property, value).unwrap();
    let queries = finish_owner_query_count_for_test(&list.index);
    assert_eq!(
        queries, 1,
        "single-property {write:?}, cached_snapshot={cached_snapshot}: resolve the live owner once"
    );

    // Keep all semantic checks outside the measured scope. A missing put,
    // removed duplicate pair, lost explicit nil, stale cache, or broken hash
    // must not become an optimization that satisfies only the work counter.
    assert_eq!(actual_changed, changed);
    assert_eq!(list.overlay_get(overlay, &property), Some(value));
    assert_eq!(
        list.index.range(overlay),
        Some(EmacsByteRange::from_usize(10, 20))
    );
    assert!(list.index.contains(overlay));
    assert!(list.index.may_contain_property(face));
    assert!(list.index.may_contain_property(duplicate));
    assert_eq!(
        list.index.may_contain_property(added),
        matches!(
            write,
            SinglePropertyWrite::Insert | SinglePropertyWrite::InsertNil
        )
    );
    let plist = list
        .overlay_plist(overlay)
        .expect("member plist remains visible");
    let mut cursor = plist;
    let mut face_values = Vec::new();
    let mut duplicate_values = Vec::new();
    while cursor.is_cons() {
        let key = cursor.cons_car();
        let rest = cursor.cons_cdr();
        assert!(rest.is_cons(), "fixture remains a complete property list");
        if key.bits() == face.bits() {
            face_values.push(rest.cons_car());
        }
        if key.bits() == duplicate.bits() {
            duplicate_values.push(rest.cons_car());
        }
        cursor = rest.cons_cdr();
    }
    assert!(cursor.is_nil());
    assert_eq!(
        face_values,
        [
            if matches!(write, SinglePropertyWrite::Update) {
                italic
            } else {
                bold
            },
            shadow
        ]
    );
    assert_eq!(duplicate_values, [Value::T, Value::NIL]);
    assert_eq!(
        list.content_digest(),
        list.content_digest_from_scratch_for_test()
    );
    if matches!(write, SinglePropertyWrite::Noop) {
        assert_eq!(list.content_digest(), digest_before);
    }
    list.assert_index_invariants_for_test();
    if cached_snapshot {
        let snapshot = list.snapshot();
        assert_eq!(
            overlay_snapshot_copy_count(),
            0,
            "retain and mirror the unshared cache"
        );
        let copies = snapshot.overlays_at_emacs_byte_pos(EmacsBytePos::new(10));
        assert_eq!(copies.len(), 1);
        assert_ne!(copies[0].bits(), overlay.bits());
        assert_eq!(snapshot.overlay_get(copies[0], &property), Some(value));
        assert_eq!(
            snapshot.index.may_contain_property(added),
            list.index.may_contain_property(added)
        );
        snapshot.assert_index_invariants_for_test();
    }
    // Removing the actual owner must balance all duplicated key counts.
    list.delete_overlay(overlay);
    assert!(!list.index.may_contain_property(face));
    assert!(!list.index.may_contain_property(duplicate));
    assert!(!list.index.may_contain_property(added));
    list.assert_index_invariants_for_test();
}

macro_rules! single_property_write_owner_resolution_case {
    ($name:ident, $snapshot:expr, $write:expr) => {
        #[test]
        fn $name() {
            check_single_property_write_owner_resolution($snapshot, $write);
        }
    };
}

single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cold_noop,
    false,
    SinglePropertyWrite::Noop
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cold_update,
    false,
    SinglePropertyWrite::Update
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cold_insert,
    false,
    SinglePropertyWrite::Insert
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cold_insert_nil,
    false,
    SinglePropertyWrite::InsertNil
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cached_noop,
    true,
    SinglePropertyWrite::Noop
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cached_update,
    true,
    SinglePropertyWrite::Update
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cached_insert,
    true,
    SinglePropertyWrite::Insert
);
single_property_write_owner_resolution_case!(
    single_property_write_owner_resolution_cached_insert_nil,
    true,
    SinglePropertyWrite::InsertNil
);

#[test]
fn single_property_write_owner_resolution_counter_positive_control() {
    use crate::buffer::overlay_index::{
        finish_owner_query_count_for_test, start_owner_query_count_for_test,
    };
    crate::test_utils::init_test_tracing();
    let overlay = overlay_with_plist(10, 20, Value::NIL);
    let mut list = OverlayList::new();
    list.insert_overlay(overlay);
    let snapshot = list.snapshot();
    let copies = snapshot.overlays_at_emacs_byte_pos(EmacsBytePos::new(10));
    assert_eq!(copies.len(), 1);
    start_owner_query_count_for_test(&list.index);
    assert!(list.index.contains(overlay));
    assert_eq!(
        list.index.range(overlay),
        Some(EmacsByteRange::from_usize(10, 20))
    );
    // These are real owner queries too, but this different index is outside
    // the selected live-owner budget. The exact positive result must be 2.
    assert!(snapshot.index.contains(copies[0]));
    assert_eq!(
        snapshot.index.range(copies[0]),
        Some(EmacsByteRange::from_usize(10, 20))
    );
    assert_eq!(finish_owner_query_count_for_test(&list.index), 2);
}

#[test]
fn single_property_write_owner_resolution_plist_visibility_control() {
    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let plist = Value::list(vec![face, Value::symbol("bold")]);
    let overlay = overlay_with_plist(10, 20, plist);
    let mut owner = OverlayList::new();
    owner.insert_overlay(overlay);
    let foreign = OverlayList::new();
    assert_eq!(owner.overlay_plist(overlay).unwrap().bits(), plist.bits());
    assert!(
        foreign.overlay_plist(overlay).is_none(),
        "a live foreign overlay has no visible plist here"
    );
    owner.delete_overlay(overlay);
    assert!(overlay_live_buffer(overlay).is_none());
    assert_eq!(owner.overlay_plist(overlay).unwrap().bits(), plist.bits());
    assert_eq!(
        foreign.overlay_plist(overlay).unwrap().bits(),
        plist.bits(),
        "a detached overlay's plist remains observable through either list"
    );
}
