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
    PropertySweep,
    PropertyExtent,
}

struct DirectFaceResolver<'a> {
    list: &'a OverlayList,
    property: Value,
}

impl OverlayPropertyResolver for DirectFaceResolver<'_> {
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
        self.list.overlay_get_named(overlay, self.property)
    }

    fn endpoint_filter(&self) -> OverlayPropertyFilter {
        OverlayPropertyFilter::for_properties([self.property])
    }
}

fn check_first_query(count: usize, query: FirstQuery) {
    crate::test_utils::init_test_tracing();
    check_observation_probe();
    let category = Value::symbol("category");
    let face = Value::symbol("face");
    let list = unique_plist_overlays(count);
    let start = 10 + (count / 2) * 4;
    let total_end = 10 + count * 4;
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
        FirstQuery::PropertySweep => {
            let run = observed_query(&label, || {
                let OverlayPropertyAtPoint::Present(resolution) = list
                    .resolve_overlay_property_at_emacs_byte_pos(
                        emacs_byte_pos(start),
                        None,
                        DirectFaceResolver {
                            list: &list,
                            property: face,
                        },
                    )
                else {
                    panic!("expected face overlay at {start}");
                };
                let mut sweep = resolution
                    .sweep(emacs_byte_range(0, total_end))
                    .expect("sweep covers midpoint");
                sweep
                    .partition_at(emacs_byte_pos(start))
                    .expect("partition covers midpoint")
            });
            assert_eq!(run.range(), emacs_byte_range(start, start + 2));
        }
        FirstQuery::PropertyExtent => {
            let extent = observed_query(&label, || {
                let OverlayPropertyAtPoint::Present(resolution) = list
                    .resolve_overlay_property_at_emacs_byte_pos(
                        emacs_byte_pos(start),
                        None,
                        DirectFaceResolver {
                            list: &list,
                            property: face,
                        },
                    )
                else {
                    panic!("expected face overlay at {start}");
                };
                resolution
                    .extent(emacs_byte_range(0, total_end))
                    .expect("extent covers midpoint")
            });
            assert_eq!(extent.range(), emacs_byte_range(start, start + 2));
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
first_query_case!(
    first_property_sweep_capture_budget_2000,
    2_000,
    FirstQuery::PropertySweep
);
first_query_case!(
    first_property_sweep_capture_budget_16000,
    16_000,
    FirstQuery::PropertySweep
);
first_query_case!(
    first_property_sweep_capture_budget_64000,
    64_000,
    FirstQuery::PropertySweep
);
first_query_case!(
    first_property_extent_capture_budget_2000,
    2_000,
    FirstQuery::PropertyExtent
);
first_query_case!(
    first_property_extent_capture_budget_16000,
    16_000,
    FirstQuery::PropertyExtent
);
first_query_case!(
    first_property_extent_capture_budget_64000,
    64_000,
    FirstQuery::PropertyExtent
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
        // Publishing endpoints must reuse cached per-overlay property masks
        // without re-walking Lisp plists.
        assert_eq!(property_index_plist_steps().1, 0);
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
    // Do this only after reading the work counter: an invariant check
    // legitimately traverses source plists to cross-check cached masks.
    list.assert_index_invariants_for_test();
    assert!(
        property_index_plist_steps().1 > 0,
        "invariant check must exercise the independent mask-verification walker"
    );
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

#[test]
fn cold_endpoint_publication_performs_zero_observed_reads_and_matches_live_masks() {
    use crate::buffer::overlay_index::{
        property_index_plist_steps, reset_property_index_plist_steps,
    };

    crate::test_utils::init_test_tracing();
    let face = Value::symbol("face");
    let mouse_face = Value::symbol("mouse-face");
    let nil_added = Value::symbol("cold-pub-nil-key");
    let detached_only = Value::symbol("cold-pub-detached-key");
    let evaporated_only = Value::symbol("cold-pub-evaporated-key");
    let evaporate = Value::symbol("evaporate");

    // 1. Live OverlayList mutated across every cold write path before publication.
    let mut list = OverlayList::new();
    let kept = overlay_with_plist(10, 20, Value::list(vec![face, Value::symbol("bold")]));
    let moved = overlay_with_plist(30, 40, Value::NIL);
    let detached = overlay_with_plist(
        50,
        60,
        Value::list(vec![detached_only, Value::symbol("gone")]),
    );
    let evaporating = overlay_with_plist(
        70,
        75,
        Value::list(vec![evaporate, Value::T, evaporated_only, Value::T]),
    );
    for ov in [kept, moved, detached, evaporating] {
        list.insert_overlay(ov);
    }
    // Cold overlay_put: existing key update, new non-nil key, new explicit-nil key.
    assert!(
        list.overlay_put(kept, face, Value::symbol("italic"))
            .unwrap()
    );
    assert!(
        list.overlay_put(kept, mouse_face, Value::symbol("highlight"))
            .unwrap()
    );
    assert!(!list.overlay_put(moved, nil_added, Value::NIL).unwrap());
    // Cold move (EndOnly then StartChanged).
    list.move_overlay_to_emacs_byte_range(moved, emacs_byte_range(30, 44));
    list.move_overlay_to_emacs_byte_range(moved, emacs_byte_range(32, 46));
    // Cold detach.
    assert!(list.delete_overlay(detached));
    // Cold text insert (shifts suffix) and delete (evaporates `evaporating`).
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(5), EmacsByteLen::new(2), false);
    // `evaporating` shifted from [70, 75] to [72, 77]; delete [72, 77] to evaporate it.
    let _ = list.adjust_for_delete_emacs_byte_range(emacs_byte_range(72, 77));

    let bounds = emacs_byte_range(0, 200);
    reset_property_index_plist_steps();
    let (all_endpoints, reads) = collection_reads::capture(|| {
        list.index
            .endpoint_records_strictly_within(bounds, OverlayPropertyFilter::unfiltered())
            .collect::<Vec<_>>()
    });
    let reads = reads.expect("cold publication must yield a valid certificate");
    assert!(
        reads.reads_empty_for_test(),
        "cold endpoint publication must perform zero observed Lisp reads"
    );
    assert_eq!(
        property_index_plist_steps(),
        (0, 0),
        "cold endpoint publication must perform zero Lisp plist walks"
    );
    assert_eq!(
        all_endpoints.len(),
        4,
        "only kept and moved remain attached"
    );

    // Filtered queries on the published tree must reflect every cold mutation
    // (no stale masks after zero-observed-read publication).
    let mouse_carriers: Vec<_> = list
        .index
        .endpoint_records_strictly_within(
            bounds,
            OverlayPropertyFilter::for_properties([mouse_face]),
        )
        .map(|ep| ep.overlay.bits())
        .collect();
    assert_eq!(mouse_carriers, vec![kept.bits(), kept.bits()]);

    let nil_key_carriers: Vec<_> = list
        .index
        .endpoint_records_strictly_within(
            bounds,
            OverlayPropertyFilter::for_properties([nil_added]),
        )
        .map(|ep| ep.overlay.bits())
        .collect();
    assert_eq!(nil_key_carriers, vec![moved.bits(), moved.bits()]);

    assert!(
        list.index
            .endpoint_records_strictly_within(
                bounds,
                OverlayPropertyFilter::for_properties([detached_only]),
            )
            .next()
            .is_none(),
        "detached overlay key must not survive in published endpoint masks"
    );
    assert!(
        list.index
            .endpoint_records_strictly_within(
                bounds,
                OverlayPropertyFilter::for_properties([evaporated_only]),
            )
            .next()
            .is_none(),
        "evaporated overlay key must not survive in published endpoint masks"
    );
    list.assert_index_invariants_for_test();

    // 2. Batch-attached (from_dump) + remapped cold index must also publish
    // with zero observed reads and up-to-date masks.
    let batch_a = overlay_with_plist(10, 14, Value::list(vec![mouse_face, Value::T]));
    let batch_b = overlay_with_plist(20, 24, Value::list(vec![face, Value::symbol("default")]));
    let mut restored = OverlayList::from_dump(vec![batch_a, batch_b]);
    restored.remap_overlay_positions(&[
        OverlayPositionRemap {
            overlay: batch_a,
            old_range: emacs_byte_range(10, 14),
            range: emacs_byte_range(110, 114),
            old_begin: CharPos0::new(10),
            new_begin: CharPos0::new(110),
        },
        OverlayPositionRemap {
            overlay: batch_b,
            old_range: emacs_byte_range(20, 24),
            range: emacs_byte_range(120, 124),
            old_begin: CharPos0::new(20),
            new_begin: CharPos0::new(120),
        },
    ]);
    reset_property_index_plist_steps();
    let (batch_mouse, batch_reads) = collection_reads::capture(|| {
        restored
            .index
            .endpoint_records_strictly_within(
                bounds,
                OverlayPropertyFilter::for_properties([mouse_face]),
            )
            .map(|ep| (ep.overlay.bits(), ep.position.get()))
            .collect::<Vec<_>>()
    });
    let batch_reads = batch_reads.expect("batch cold publication certificate");
    assert!(
        batch_reads.reads_empty_for_test(),
        "batch cold endpoint publication must perform zero observed Lisp reads"
    );
    assert_eq!(property_index_plist_steps(), (0, 0));
    assert_eq!(
        batch_mouse,
        vec![(batch_a.bits(), 110), (batch_a.bits(), 114)]
    );
    restored.assert_index_invariants_for_test();
}

#[test]
fn cold_live_view_endpoint_publication_performs_zero_observed_reads() {
    use crate::buffer::overlay_index::{
        property_index_plist_steps, reset_property_index_plist_steps,
    };

    crate::test_utils::init_test_tracing();
    let mouse_face = Value::symbol("mouse-face");
    let mut list = unique_plist_overlays(2_000);
    let target_start = 10 + 1_000 * 4;
    let target = list.overlays_at_emacs_byte_pos(emacs_byte_pos(target_start))[0];
    assert!(
        list.overlay_put(target, mouse_face, Value::symbol("highlight"))
            .unwrap()
    );

    struct LivePropertyResolver<'a> {
        view: &'a LiveOverlayView,
        property: Value,
    }
    impl OverlayPropertyResolver for LivePropertyResolver<'_> {
        fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
            self.view.overlay_get_named(overlay, self.property)
        }

        fn endpoint_filter(&self) -> OverlayPropertyFilter {
            OverlayPropertyFilter::for_properties([self.property])
        }
    }

    let live = list.live_view();
    let total_end = 10 + 2_000 * 4;
    reset_property_index_plist_steps();
    let (run, reads) = collection_reads::capture(|| {
        let OverlayPropertyAtPoint::Present(resolution) = live
            .resolve_overlay_property_at_emacs_byte_pos(
                emacs_byte_pos(target_start),
                None,
                LivePropertyResolver {
                    view: &live,
                    property: mouse_face,
                },
            )
        else {
            panic!("expected mouse-face overlay at {target_start}");
        };
        let mut sweep = resolution
            .sweep(emacs_byte_range(0, total_end))
            .expect("live view sweep covers target");
        sweep
            .partition_at(emacs_byte_pos(target_start))
            .expect("live view partition covers target")
    });
    let reads = reads.expect("cold live-view sweep must produce a coherent certificate");
    assert!(
        owner_count(&reads) <= QUERY_OWNER_BUDGET,
        "cold live-view publication must not enroll distant overlays; observed {}",
        owner_count(&reads)
    );
    assert_eq!(
        property_index_plist_steps(),
        (0, 0),
        "cold live-view endpoint publication must perform zero plist walks"
    );
    assert_eq!(
        run.range(),
        emacs_byte_range(target_start, target_start + 2)
    );
    assert_eq!(
        run.winner().map(|w| w.overlay().bits()),
        Some(target.bits())
    );
    assert!(!live.is_stale());
}

#[cfg(any(debug_assertions, feature = "test-support"))]
#[test]
fn assert_no_observed_reads_rejects_plist_walk_in_debug() {
    crate::test_utils::init_test_tracing();
    let plist = Value::list(vec![Value::symbol("face"), Value::symbol("bold")]);
    let err = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        collection_reads::assert_no_observed_reads(|| {
            let _ = plist.cons_car();
        });
    }))
    .expect_err("assert_no_observed_reads must panic on an observed Lisp read");
    let msg = err
        .downcast_ref::<String>()
        .map(String::as_str)
        .or_else(|| err.downcast_ref::<&str>().copied())
        .unwrap_or("");
    assert!(
        msg.contains("assert_no_observed_reads"),
        "unexpected panic payload: {msg:?}"
    );
}

#[test]
fn highest_priority_overlay_property_absent_fast_path_respects_budget_and_underflow() {
    crate::test_utils::init_test_tracing();
    let invisible = Value::symbol("invisible");
    let list = unique_plist_overlays(16_000);
    // Query a position covered by an overlay whose plist carries `face` and
    // `priority`, but NOT `invisible`.
    let covered_pos = emacs_byte_pos(10 + 8_000 * 4);
    reset_overlays_at_node_visit_count();
    reset_endpoint_publication_interval_read_count();
    let (answer, reads) = collection_reads::capture(|| {
        list.highest_priority_overlay_property_value_at_emacs_byte_pos(covered_pos, invisible, None)
    });
    assert_eq!(answer, None);
    let reads = reads.expect("absent direct-key query must have a coherent certificate");
    assert_eq!(
        owner_count(&reads),
        0,
        "absent direct-key fast path must observe zero plist owners"
    );
    assert_eq!(
        overlays_at_node_visit_count(),
        0,
        "absent direct-key fast path must not descend the interval tree"
    );
    assert_eq!(
        endpoint_publication_interval_read_count(),
        0,
        "absent direct-key fast path must keep endpoint index cold"
    );

    // Range-view underflow must STILL be recorded on the absent-key fast path.
    let view = list.snapshot_in_range(emacs_byte_range(4_000, 8_000));
    assert!(!view.overlay_view_query_underflowed());
    assert_eq!(
        view.highest_priority_overlay_property_value_at_emacs_byte_pos(
            emacs_byte_pos(100),
            invisible,
            None,
        ),
        None
    );
    assert!(
        view.overlay_view_query_underflowed(),
        "absent direct-key fast path must still call note_overlay_view_query(pos)"
    );
}

#[test]
fn highest_priority_overlay_property_bloom_false_negative_mutation_is_caught() {
    crate::test_utils::init_test_tracing();
    let invisible = Value::symbol("invisible");
    let mut list = OverlayList::new();
    list.insert_overlay(overlay_with_plist(
        10,
        20,
        Value::list(vec![invisible, Value::T]),
    ));
    let pos = emacs_byte_pos(12);
    // Normal path: bloom bit is set, fast path falls through and finds `t`.
    assert_eq!(
        list.highest_priority_overlay_property_value_at_emacs_byte_pos(pos, invisible, None),
        Some(Value::T)
    );
    list.assert_index_invariants_for_test();

    // Mutation proof: clearing the bloom bit for a present property forces a
    // false negative on the fast path, diverging immediately from the
    // authoritative unfiltered scan and failing the index invariant check.
    list.index
        .clear_direct_property_key_bloom_bit_for_test(invisible);
    let fast_path_after_mutation =
        list.highest_priority_overlay_property_value_at_emacs_byte_pos(pos, invisible, None);
    let authoritative = match list.resolve_overlay_property_at_emacs_byte_pos(pos, None, |ov| {
        overlay_property_named(ov, invisible)
    }) {
        OverlayPropertyAtPoint::Present(res) => Some(res.value()),
        OverlayPropertyAtPoint::Vacant(_) => None,
    };
    assert_eq!(authoritative, Some(Value::T));
    assert_ne!(
        fast_path_after_mutation, authoritative,
        "clearing the bloom bit must expose a false negative, proving the guard is active"
    );
    assert!(
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            list.assert_index_invariants_for_test();
        }))
        .is_err(),
        "index invariant check must catch a missing bloom bit"
    );
}

#[test]
fn next_property_boundary_skips_non_carriers_without_publishing_endpoints() {
    crate::test_utils::init_test_tracing();
    let invisible = Value::symbol("invisible");
    let display = Value::symbol("display");
    let mut list = unique_plist_overlays(16_000);

    // Absent properties: O(1) None, 0 observed owners, 0 endpoint publications.
    reset_endpoint_publication_interval_read_count();
    let (next, reads) = collection_reads::capture(|| {
        list.next_property_boundary_after_until_emacs_byte_pos(
            emacs_byte_pos(0),
            emacs_byte_pos(100_000),
            &[invisible, display],
        )
    });
    assert_eq!(next, None);
    assert_eq!(owner_count(&reads.expect("coherent certificate")), 0);
    assert_eq!(endpoint_publication_interval_read_count(), 0);

    // Insert one `invisible` overlay deep in the buffer. Cold index must STILL
    // avoid endpoint publication; once published, OrderedFilterMask skips all
    // 16,000 non-carrier overlays directly to the carrier's start.
    list.insert_overlay(overlay_with_plist(
        50_000,
        50_020,
        Value::list(vec![invisible, Value::T]),
    ));
    reset_endpoint_publication_interval_read_count();
    let cold_next = list.next_property_boundary_after_until_emacs_byte_pos(
        emacs_byte_pos(0),
        emacs_byte_pos(100_000),
        &[invisible, display],
    );
    assert!(cold_next.is_some());
    assert_eq!(
        endpoint_publication_interval_read_count(),
        0,
        "cold filtered boundary query must not trigger O(N) endpoint publication"
    );

    // Explicitly publish endpoints, then verify OrderedFilterMask jumps straight
    // past all non-carrier subtrees to byte 50,000.
    let _ = list
        .index
        .endpoint_records_strictly_within(
            emacs_byte_range(0, 100_000),
            OverlayPropertyFilter::unfiltered(),
        )
        .next();
    reset_endpoint_search_node_visit_count();
    let warm_next = list.next_property_boundary_after_until_emacs_byte_pos(
        emacs_byte_pos(0),
        emacs_byte_pos(100_000),
        &[invisible, display],
    );
    assert_eq!(warm_next, Some(emacs_byte_pos(50_000)));
    assert!(
        endpoint_search_node_visit_count() < 64,
        "warm OrderedFilterMask jump visited {} nodes (expected O(log N))",
        endpoint_search_node_visit_count()
    );

    // P2: Range-view underflow must be recorded by
    // `OverlayList::next_property_boundary_after_until_emacs_byte_pos` even on
    // the absent-property fast path.
    let underflow_view = list.snapshot_in_range(emacs_byte_range(4_000, 8_000));
    assert!(!underflow_view.overlay_view_query_underflowed());
    assert_eq!(
        underflow_view.next_property_boundary_after_until_emacs_byte_pos(
            emacs_byte_pos(100),
            emacs_byte_pos(8_000),
            &[invisible, display],
        ),
        None
    );
    assert!(
        underflow_view.overlay_view_query_underflowed(),
        "next_property_boundary_after_until_emacs_byte_pos must record underflow below capture floor"
    );

    // P2: `LiveOverlayView` fast paths must still latch `is_stale()` when the
    // owner list mutates before an absent-property short-circuit.
    let stale_prop = Value::symbol("never-present-prop-829");
    let live_highest = list.live_view();
    let live_boundary = list.live_view();
    assert!(!live_highest.is_stale());
    assert!(!live_boundary.is_stale());
    list.insert_overlay(overlay_with_plist(
        60_000,
        60_010,
        Value::list(vec![Value::symbol("face"), Value::symbol("bold")]),
    ));
    assert_eq!(
        live_highest.highest_priority_overlay_property_value_at_emacs_byte_pos(
            emacs_byte_pos(12),
            stale_prop,
            None,
        ),
        None
    );
    assert!(
        live_highest.is_stale(),
        "LiveOverlayView::highest_priority_overlay_property_value_at_emacs_byte_pos must latch stale on short-circuit"
    );
    assert_eq!(
        live_boundary.next_property_boundary_after_until_emacs_byte_pos(
            emacs_byte_pos(0),
            emacs_byte_pos(100_000),
            &[stale_prop],
        ),
        None
    );
    assert!(
        live_boundary.is_stale(),
        "LiveOverlayView::next_property_boundary_after_until_emacs_byte_pos must latch stale on short-circuit"
    );

    // Reusing memoized copies across a second `snapshot_in_range` must NOT bump
    // `LispCollectionRevision` on the shared read-only copies and invalidate an
    // earlier `CollectionReads` certificate that observed the first range view.
    let first_view = list.snapshot_in_range(emacs_byte_range(0, 1_000));
    let (observed_overlays, reads) = collection_reads::capture(|| {
        first_view
            .overlays_at_emacs_byte_pos(emacs_byte_pos(10))
            .into_iter()
            .filter_map(|ov| first_view.overlay_get_named(ov, Value::symbol("face")))
            .collect::<Vec<_>>()
    });
    assert!(!observed_overlays.is_empty());
    let reads = reads.expect("first range-view read must produce a coherent certificate");
    assert!(reads.unchanged());
    let second_view = list.snapshot_in_range(emacs_byte_range(0, 1_000));
    let _ = second_view.overlays_at_emacs_byte_pos(emacs_byte_pos(10));
    assert!(
        reads.unchanged(),
        "second snapshot_in_range reusing memoized copies must not invalidate prior CollectionReads"
    );
}
