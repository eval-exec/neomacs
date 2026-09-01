use super::*;
use crate::buffer::BufferId;

fn alloc_overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(OverlayData {
        serial: 0,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
        position_handle: None,
        front_advance: false,
        rear_advance: false,
    })
}

fn emacs_byte_pos(byte: usize) -> EmacsBytePos {
    EmacsBytePos::new(byte)
}

fn emacs_byte_range(start: usize, end: usize) -> EmacsByteRange {
    EmacsByteRange::from_usize(start, end)
}

fn emacs_byte_len(len: usize) -> EmacsByteLen {
    EmacsByteLen::new(len)
}

fn overlay_start(list: &OverlayList, overlay: Value) -> Option<usize> {
    list.overlay_start_emacs_byte_pos(overlay)
        .map(EmacsBytePos::get)
}

fn overlay_end(list: &OverlayList, overlay: Value) -> Option<usize> {
    list.overlay_end_emacs_byte_pos(overlay)
        .map(EmacsBytePos::get)
}

fn overlays_at(list: &OverlayList, pos: usize) -> Vec<Value> {
    list.overlays_at_emacs_byte_pos(emacs_byte_pos(pos))
}

fn overlays_in_region(
    list: &OverlayList,
    start: usize,
    end: usize,
    accessible_end: usize,
) -> Vec<Value> {
    list.overlays_in_accessible_emacs_byte_range(
        emacs_byte_range(start, end),
        emacs_byte_pos(accessible_end),
    )
}

fn direct_overlay_property_extent(
    list: &OverlayList,
    pos: EmacsBytePos,
    property: Value,
    bounds: EmacsByteRange,
    window_id: Option<u64>,
) -> Option<OverlayPropertyExtent> {
    match list.resolve_overlay_property_at_emacs_byte_pos(pos, window_id, |overlay| {
        list.overlay_get_named(overlay, property)
    }) {
        OverlayPropertyAtPoint::Present(resolution) => resolution.extent(bounds),
        OverlayPropertyAtPoint::Vacant(_) => None,
    }
}

struct FilteredPropertyResolver<'a> {
    overlays: &'a OverlayList,
    lookup_order: &'a [Value],
}

impl OverlayPropertyResolver for FilteredPropertyResolver<'_> {
    fn value_for_overlay(&mut self, overlay: Value) -> Option<Value> {
        let (canonical, aliases) = self.lookup_order.split_first()?;
        if let Some(value) = self.overlays.overlay_get_named(overlay, *canonical) {
            return Some(value);
        }
        aliases.iter().find_map(|alias| {
            self.overlays
                .overlay_get_named(overlay, *alias)
                .filter(|value| !value.is_nil())
        })
    }

    fn endpoint_filter(&self) -> OverlayPropertyFilter {
        OverlayPropertyFilter::for_properties(
            self.lookup_order
                .iter()
                .copied()
                .chain(std::iter::once(Value::symbol("category"))),
        )
    }
}

#[test]
fn insert_and_delete_overlay_preserves_object_identity() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(2, 5);
    list.insert_overlay(overlay);
    assert_eq!(overlays_at(&list, 3), vec![overlay]);
    assert!(list.delete_overlay(overlay));
    assert!(overlays_at(&list, 3).is_empty());
    assert!(overlay_live_buffer(overlay).is_none());
}

#[test]
fn same_range_overlays_remain_distinct_objects() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    list.insert_overlay(first);
    list.insert_overlay(second);

    let overlays = overlays_at(&list, 3);
    assert_eq!(overlays.len(), 2);
    assert!(overlays.iter().any(|overlay| eq_value(overlay, &first)));
    assert!(overlays.iter().any(|overlay| eq_value(overlay, &second)));

    assert!(list.delete_overlay(first));
    let overlays = overlays_at(&list, 3);
    assert_eq!(overlays.len(), 1);
    assert!(eq_value(&overlays[0], &second));
}

#[test]
fn raw_overlays_at_matches_gnu_same_start_itree_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    let third = alloc_overlay(2, 5);
    list.insert_overlay(first);
    list.insert_overlay(second);
    list.insert_overlay(third);

    assert_eq!(overlays_at(&list, 3), vec![third, second, first]);

    assert!(list.delete_overlay(second));
    assert_eq!(overlays_at(&list, 3), vec![third, first]);
}

#[test]
fn dump_round_trip_preserves_gnu_same_start_itree_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    let third = alloc_overlay(2, 5);
    list.insert_overlay(first);
    list.insert_overlay(second);
    list.insert_overlay(third);

    let dumped = list.dump_overlays();
    drop(list);
    let restored = OverlayList::from_dump(dumped);

    assert_eq!(
        overlays_at(&restored, 3)
            .into_iter()
            .map(Value::bits)
            .collect::<Vec<_>>(),
        vec![third.bits(), second.bits(), first.bits()]
    );
}

#[test]
fn text_edit_relocation_preserves_same_start_itree_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    let third = alloc_overlay(2, 5);
    list.insert_overlay(first);
    list.insert_overlay(second);
    list.insert_overlay(third);

    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(1), emacs_byte_len(3), true);

    assert_eq!(overlays_at(&list, 6), vec![third, second, first]);
}

#[test]
fn overlays_at_prunes_right_subtree_when_all_starts_are_after_position() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let start = 10 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }

    reset_overlays_at_node_visit_count();
    assert!(overlays_at(&list, 0).is_empty());

    let visits = overlays_at_node_visit_count();
    assert!(
        visits < 8,
        "overlays_at should prune right subtrees that start after the queried position; visited {visits} nodes"
    );
}

#[test]
fn overlays_at_midpoint_skips_disjoint_prefix_and_suffix_nodes() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let start = 10 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }

    reset_overlays_at_node_visit_count();
    reset_endpoint_search_summary_shift_count();
    reset_overlay_iterator_frame_push_count();
    assert_eq!(overlays_at(&list, 2_010).len(), 1);

    let visits = overlays_at_node_visit_count();
    assert!(
        visits <= 8,
        "overlays_at should descend directly to disjoint midpoint matches; visited {visits} nodes"
    );
    assert_eq!(
        endpoint_search_summary_shift_count(),
        0,
        "point search should shift only compared scalars, not rebuild node summaries"
    );
    assert_eq!(
        overlay_iterator_frame_push_count(),
        0,
        "materialized point queries should not construct a resumable iterator stack"
    );
}

#[test]
fn monotonic_overlay_insertion_keeps_interval_index_logarithmic() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay_count = 2_000usize;
    for index in 0..overlay_count {
        let start = 10 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }

    // GNU's red-black interval tree has height <= 2 * log2(n + 1).  This is
    // an interface performance guarantee, not a test of a particular balancing
    // algorithm: AVL, red-black, and a high-fanout tree all satisfy it.
    let logarithmic_height_bound = 2 * (usize::BITS - overlay_count.leading_zeros()) as usize;
    assert!(
        list.interval_index_height() <= logarithmic_height_bound,
        "sorted insertion produced interval-index height {}; expected at most {} for {} overlays",
        list.interval_index_height(),
        logarithmic_height_bound,
        overlay_count
    );
}

#[test]
fn inserted_position_property_lookup_inspects_only_nearby_overlays() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("face");
    let target = alloc_overlay(2, 8);
    list.insert_overlay(target);
    list.overlay_put(target, property, Value::symbol("bold"))
        .unwrap();
    for index in 0..2_000 {
        let start = 100 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }

    reset_best_overlay_candidate_inspection_count();
    assert_eq!(
        list.highest_priority_overlay_for_inserted_emacs_byte_pos(emacs_byte_pos(4), &property,),
        Some(target)
    );
    let inspections = best_overlay_candidate_inspection_count();
    assert!(
        inspections < 32,
        "an inserted-position lookup should inspect only interval-tree candidates; inspected {inspections} overlays"
    );
}

#[test]
fn tail_insertion_inspects_only_overlays_with_affected_endpoints() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let start = index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }
    let affected = alloc_overlay(5_000, 5_010);
    list.insert_overlay(affected);

    reset_overlay_edit_candidate_inspection_count();
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(5_000), emacs_byte_len(3), true);

    assert_eq!(overlay_start(&list, affected), Some(5_003));
    assert_eq!(overlay_end(&list, affected), Some(5_013));
    let inspections = overlay_edit_candidate_inspection_count();
    assert!(
        inspections < 32,
        "a tail insertion should inspect only overlays with affected endpoints; inspected {inspections} overlays"
    );
}

#[test]
fn tail_deletion_inspects_only_overlays_with_affected_endpoints() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let start = index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }
    let affected = alloc_overlay(5_000, 5_010);
    list.insert_overlay(affected);

    reset_overlay_edit_candidate_inspection_count();
    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(4_997, 5_000));

    assert_eq!(overlay_start(&list, affected), Some(4_997));
    assert_eq!(overlay_end(&list, affected), Some(5_007));
    let inspections = overlay_edit_candidate_inspection_count();
    assert!(
        inspections < 32,
        "a tail deletion should inspect only overlays with affected endpoints; inspected {inspections} overlays"
    );
}

#[test]
fn prefix_edit_shifts_large_suffix_without_visiting_each_overlay() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(100, 101);
    let last = alloc_overlay(4_098, 4_099);
    list.insert_overlay(first);
    for index in 1..1_999 {
        let start = 100 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }
    list.insert_overlay(last);

    reset_overlay_edit_candidate_inspection_count();
    reset_overlay_shift_node_visit_count();
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(1), emacs_byte_len(3), true);
    assert_eq!(overlay_start(&list, first), Some(103));
    assert_eq!(overlay_end(&list, last), Some(4_102));

    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(1, 4));
    assert_eq!(overlay_start(&list, first), Some(100));
    assert_eq!(overlay_end(&list, last), Some(4_099));

    let inspections = overlay_edit_candidate_inspection_count();
    assert!(
        inspections < 64,
        "a wholly affected suffix should be shifted lazily; inspected {inspections} overlays"
    );
    let shift_visits = overlay_shift_node_visit_count();
    assert_eq!(
        shift_visits, 2,
        "a prefix insert/delete pair should tag the authoritative interval root once per edit"
    );
}

#[test]
fn lazy_suffix_positions_are_authoritative_on_overlay_objects() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(100, 101);
    list.insert_overlay(overlay);

    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(1), emacs_byte_len(3), true);

    let data = overlay.as_overlay_data().expect("overlay object");
    assert_eq!(data.current_range(), (103, 104));
    let materialized = list.get(overlay).expect("live overlay");
    assert_eq!((materialized.start, materialized.end), (103, 104));
    let equal_at_current_range = alloc_overlay(103, 104);
    assert!(crate::emacs_core::value::equal_value(
        &overlay,
        &equal_at_current_range,
        0,
    ));
}

#[test]
fn lazy_edit_index_matches_simple_overlay_reference_model() {
    crate::test_utils::init_test_tracing();

    #[derive(Clone, Copy)]
    struct ReferenceOverlay {
        value: Value,
        start: usize,
        end: usize,
        front_advance: bool,
        rear_advance: bool,
    }

    let mut list = OverlayList::new();
    let mut model = Vec::new();
    for index in 0..64 {
        let start = 5 + index * 3;
        let end = start + index % 7;
        let value = alloc_overlay(start, end);
        let front_advance = index % 3 == 0;
        let rear_advance = index % 4 == 0;
        list.insert_overlay(value);
        list.set_front_advance(value, front_advance);
        list.set_rear_advance(value, rear_advance);
        model.push(ReferenceOverlay {
            value,
            start,
            end,
            front_advance,
            rear_advance,
        });
    }

    let mut random = 0x7a5b_49d3_u64;
    for step in 0..300 {
        random = random
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        let position = 1 + (random as usize % 180);
        let length = 1 + ((random >> 16) as usize % 5);
        if step % 3 != 2 {
            let before_markers = step % 2 == 0;
            list.adjust_for_insert_at_emacs_byte_pos(
                emacs_byte_pos(position),
                emacs_byte_len(length),
                before_markers,
            );
            for overlay in &mut model {
                let empty = overlay.start == overlay.end;
                if before_markers {
                    if overlay.start >= position {
                        overlay.start += length;
                    }
                    if overlay.end >= position {
                        overlay.end += length;
                    }
                } else {
                    if overlay.start > position
                        || (overlay.start == position
                            && overlay.front_advance
                            && (!empty || overlay.rear_advance))
                    {
                        overlay.start += length;
                    }
                    if overlay.end > position || (overlay.end == position && overlay.rear_advance) {
                        overlay.end += length;
                    }
                }
            }
        } else {
            let end = position + length;
            list.adjust_for_delete_emacs_byte_range(emacs_byte_range(position, end));
            for overlay in &mut model {
                if overlay.start >= end {
                    overlay.start -= length;
                } else if overlay.start > position {
                    overlay.start = position;
                }
                if overlay.end >= end {
                    overlay.end -= length;
                } else if overlay.end > position {
                    overlay.end = position;
                }
            }
        }

        list.index.assert_invariants();
        for expected in &model {
            assert_eq!(
                list.index.range(expected.value),
                Some(emacs_byte_range(expected.start, expected.end)),
                "range diverged at edit step {step}, position {position}, length {length}, serial {}",
                expected.value.as_overlay_data().unwrap().serial,
            );
            assert_eq!(
                expected.value.as_overlay_data().unwrap().current_range(),
                (expected.start, expected.end),
                "object range diverged at edit step {step}"
            );
        }
        for point in [1, 17, 63, 127, 191] {
            let mut actual: Vec<usize> = overlays_at(&list, point)
                .into_iter()
                .map(Value::bits)
                .collect();
            actual.sort_unstable();
            let mut expected: Vec<usize> = model
                .iter()
                .filter(|overlay| overlay.start <= point && point < overlay.end)
                .map(|overlay| overlay.value.bits())
                .collect();
            expected.sort_unstable();
            assert_eq!(actual, expected, "point query diverged at edit step {step}");
        }
    }
}

#[test]
fn raw_overlays_in_matches_gnu_same_start_itree_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let start = alloc_overlay(1, 5);
    let end = alloc_overlay(48, 52);
    let full = alloc_overlay(1, 52);
    list.insert_overlay(start);
    list.insert_overlay(end);
    list.insert_overlay(full);

    assert_eq!(overlays_in_region(&list, 1, 52, 52), vec![full, start, end]);
}

#[test]
fn next_boundary_uses_one_logarithmic_endpoint_search() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    for index in 0..2_000 {
        let start = 100 + index * 2;
        list.insert_overlay(alloc_overlay(start, start + 1));
    }

    assert_eq!(
        list.next_boundary_after_emacs_byte_pos(emacs_byte_pos(0)),
        Some(emacs_byte_pos(100))
    );
    reset_endpoint_search_node_visit_count();
    reset_endpoint_search_summary_shift_count();
    assert_eq!(
        list.next_boundary_after_emacs_byte_pos(emacs_byte_pos(1_999)),
        Some(emacs_byte_pos(2_000))
    );
    let visits = endpoint_search_node_visit_count();
    assert!(
        visits <= 13,
        "next-boundary should traverse one balanced endpoint index; visited {visits} nodes"
    );
    assert_eq!(
        endpoint_search_summary_shift_count(),
        0,
        "boundary search should compare the needed shifted scalar, not rebuild summaries"
    );
}

#[test]
fn sorted_overlay_precedence_matches_gnu_same_range_identity_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let first = alloc_overlay(2, 5);
    let second = alloc_overlay(2, 5);
    list.insert_overlay(first);
    list.insert_overlay(second);

    let mut overlays = overlays_at(&list, 3);
    list.sort_overlay_ids_by_priority_desc(&mut overlays);
    assert_eq!(overlays, vec![second, first]);
}

#[test]
fn snapshot_clone_preserves_same_range_precedence_identity() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    for value in [
        Value::symbol("low-mouse"),
        Value::symbol("high-mouse"),
        Value::NIL,
    ] {
        let overlay = alloc_overlay(0, 2);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, property, value).unwrap();
        list.overlay_put(overlay, Value::symbol("priority"), Value::fixnum(7))
            .unwrap();
    }

    let snapshot = list.snapshot_clone();
    let winner = snapshot
        .highest_priority_overlay_at_emacs_byte_pos(emacs_byte_pos(0), property)
        .expect("snapshot should preserve the highest non-nil carrier");

    assert_eq!(
        snapshot.overlay_get_named(winner, property),
        Some(Value::symbol("high-mouse"))
    );
}

#[test]
fn snapshot_clone_preserves_raw_same_start_query_order() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let label = Value::symbol("snapshot-order");
    for name in ["first", "second", "third"] {
        let overlay = alloc_overlay(0, 2);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, label, Value::symbol(name))
            .unwrap();
    }

    let snapshot = list.snapshot_clone();
    let labels = overlays_at(&snapshot, 0)
        .into_iter()
        .map(|overlay| snapshot.overlay_get_named(overlay, label).unwrap())
        .collect::<Vec<_>>();

    assert_eq!(
        labels,
        vec![
            Value::symbol("third"),
            Value::symbol("second"),
            Value::symbol("first"),
        ]
    );
}

#[test]
fn delete_overlay_removes_non_root_interval_entry() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let root = alloc_overlay(20, 30);
    let earlier = alloc_overlay(2, 10);
    list.insert_overlay(root);
    list.insert_overlay(earlier);

    assert_eq!(overlays_at(&list, 5), vec![earlier]);
    assert!(list.delete_overlay(earlier));
    assert!(overlays_at(&list, 5).is_empty());
    assert_eq!(overlays_at(&list, 25), vec![root]);
    assert!(overlay_live_buffer(earlier).is_none());
}

#[test]
fn overlay_put_prepends_new_properties_and_updates_in_place() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(0, 1);
    list.insert_overlay(overlay);
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    list.overlay_put(overlay, face, Value::symbol("bold"))
        .unwrap();
    list.overlay_put(overlay, help, Value::string("tip"))
        .unwrap();
    list.overlay_put(overlay, face, Value::symbol("italic"))
        .unwrap();
    let plist = list.overlay_plist(overlay).unwrap();
    assert_eq!(
        crate::emacs_core::print::print_value(&plist),
        "(help-echo \"tip\" face italic)"
    );
}

#[test]
fn move_overlay_updates_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(0, 2);
    list.insert_overlay(overlay);
    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(4, 7));
    assert_eq!(overlay_start(&list, overlay), Some(4));
    assert_eq!(overlay_end(&list, overlay), Some(7));
    assert_eq!(overlays_at(&list, 5), vec![overlay]);
}

#[test]
fn move_overlay_removes_old_non_root_interval_entry() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let root = alloc_overlay(20, 30);
    let earlier = alloc_overlay(2, 10);
    list.insert_overlay(root);
    list.insert_overlay(earlier);

    list.move_overlay_to_emacs_byte_range(earlier, emacs_byte_range(40, 45));
    assert!(overlays_at(&list, 5).is_empty());
    assert_eq!(overlays_at(&list, 25), vec![root]);
    assert_eq!(overlays_at(&list, 42), vec![earlier]);
    assert_eq!(overlay_start(&list, earlier), Some(40));
    assert_eq!(overlay_end(&list, earlier), Some(45));
}

#[test]
fn move_overlay_evaporates_zero_width_overlay() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(2, 5);
    list.insert_overlay(overlay);
    list.overlay_put(overlay, Value::symbol("evaporate"), Value::T)
        .unwrap();
    list.move_overlay_to_emacs_byte_range(overlay, emacs_byte_range(4, 4));
    assert!(list.is_empty());
    assert!(overlay_live_buffer(overlay).is_none());
}

#[test]
fn insert_adjusts_front_and_rear_advance() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(5, 10);
    list.insert_overlay(overlay);
    list.set_front_advance(overlay, true);
    list.set_rear_advance(overlay, true);
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(5), emacs_byte_len(2), false);
    assert_eq!(overlay_start(&list, overlay), Some(7));
    assert_eq!(overlay_end(&list, overlay), Some(12));
}

#[test]
fn empty_front_advance_overlay_does_not_invert_on_insert() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(5, 5);
    list.insert_overlay(overlay);
    list.set_front_advance(overlay, true);
    list.set_rear_advance(overlay, false);
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(5), emacs_byte_len(2), false);
    assert_eq!(overlay_start(&list, overlay), Some(5));
    assert_eq!(overlay_end(&list, overlay), Some(5));
}

#[test]
fn before_markers_insert_moves_overlay_boundaries_at_point() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let starts_here = alloc_overlay(5, 10);
    let ends_here = alloc_overlay(2, 5);
    let empty = alloc_overlay(5, 5);
    list.insert_overlay(starts_here);
    list.insert_overlay(ends_here);
    list.insert_overlay(empty);
    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(5), emacs_byte_len(2), true);
    assert_eq!(overlay_start(&list, starts_here), Some(7));
    assert_eq!(overlay_end(&list, starts_here), Some(12));
    assert_eq!(overlay_start(&list, ends_here), Some(2));
    assert_eq!(overlay_end(&list, ends_here), Some(7));
    assert_eq!(overlay_start(&list, empty), Some(7));
    assert_eq!(overlay_end(&list, empty), Some(7));
}

#[test]
fn replace_preserves_overlay_spanning_replaced_text() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(25, 32);
    list.insert_overlay(overlay);

    list.adjust_for_replace_at_emacs_byte_pos(
        emacs_byte_pos(26),
        emacs_byte_len(5),
        emacs_byte_len(5),
    );
    assert_eq!(overlay_start(&list, overlay), Some(25));
    assert_eq!(overlay_end(&list, overlay), Some(32));

    list.adjust_for_insert_at_emacs_byte_pos(emacs_byte_pos(10), emacs_byte_len(15), false);
    assert_eq!(overlay_start(&list, overlay), Some(40));
    assert_eq!(overlay_end(&list, overlay), Some(47));
}

#[test]
fn delete_evaporates_zero_width_overlay() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let overlay = alloc_overlay(5, 10);
    list.insert_overlay(overlay);
    list.overlay_put(overlay, Value::symbol("evaporate"), Value::T)
        .unwrap();
    list.adjust_for_delete_emacs_byte_range(emacs_byte_range(5, 10));
    assert!(list.is_empty());
    assert!(overlay_live_buffer(overlay).is_none());
}

#[test]
fn priority_sort_uses_gnu_precedence_rules() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let low = alloc_overlay(2, 7);
    let high = alloc_overlay(4, 7);
    list.insert_overlay(low);
    list.insert_overlay(high);
    list.overlay_put(low, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    list.overlay_put(low, Value::symbol("priority"), Value::fixnum(1))
        .unwrap();
    list.overlay_put(high, Value::symbol("face"), Value::symbol("italic"))
        .unwrap();
    list.overlay_put(
        high,
        Value::symbol("priority"),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
    )
    .unwrap();
    let mut ids = overlays_at(&list, 4);
    list.sort_overlay_ids_by_priority_desc(&mut ids);
    assert_eq!(ids, vec![high, low]);
}

#[test]
fn highest_priority_property_value_wins_outright_over_lower_precedence_carriers() {
    // GNU `get_char_property_and_overlay`: the highest-precedence overlay carrying
    // the property wins OUTRIGHT. No lower-precedence overlay gets a say, even
    // when the winner's value means "inactive" downstream -- e.g. an `invisible`
    // value absent from `buffer-invisibility-spec` keeps the text VISIBLE, and a
    // lower-priority `invisible` must not hide it anyway. Scanning on past the
    // winner is what hid text GNU shows.
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("invisible");
    let high = alloc_overlay(2, 10);
    let low = alloc_overlay(2, 10);
    list.insert_overlay(high);
    list.insert_overlay(low);
    list.overlay_put(high, property, Value::symbol("not-in-spec"))
        .unwrap();
    list.overlay_put(high, Value::symbol("priority"), Value::fixnum(10))
        .unwrap();
    list.overlay_put(low, property, Value::T).unwrap();
    list.overlay_put(low, Value::symbol("priority"), Value::fixnum(1))
        .unwrap();

    let winner = list.highest_priority_overlay_property_value_at_emacs_byte_pos(
        emacs_byte_pos(4),
        property,
        None,
    );
    assert_eq!(winner, Some(Value::symbol("not-in-spec")));
}

#[test]
fn highest_priority_property_value_is_none_when_no_overlay_carries_it() {
    // `None` is the caller's signal to fall back to the TEXT property. An overlay
    // that does carry the property shadows the text property instead, so the
    // fallback must be keyed on this and nothing else -- consulting the text
    // property first is what hid text a covering overlay declared visible.
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("invisible");
    let unrelated = alloc_overlay(2, 10);
    list.insert_overlay(unrelated);
    list.overlay_put(unrelated, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    // An explicit nil value does not count as carrying the property (GNU skips
    // `NILP (tem)` candidates).
    let nil_valued = alloc_overlay(2, 10);
    list.insert_overlay(nil_valued);
    list.overlay_put(nil_valued, property, Value::NIL).unwrap();

    assert_eq!(
        list.highest_priority_overlay_property_value_at_emacs_byte_pos(
            emacs_byte_pos(4),
            property,
            None
        ),
        None
    );
}

#[test]
fn ascending_property_values_order_by_gnu_precedence_including_cons_priority() {
    // The merge policy (`face`) needs GNU `sort_overlays` order, not a bare
    // `priority` integer compare -- which reads a `(PRIMARY . SECONDARY)` priority
    // as 0 and drops the containment rule.
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("face");
    let low = alloc_overlay(2, 7);
    let high = alloc_overlay(4, 7);
    list.insert_overlay(low);
    list.insert_overlay(high);
    list.overlay_put(low, property, Value::symbol("bold"))
        .unwrap();
    list.overlay_put(low, Value::symbol("priority"), Value::fixnum(1))
        .unwrap();
    list.overlay_put(high, property, Value::symbol("italic"))
        .unwrap();
    list.overlay_put(
        high,
        Value::symbol("priority"),
        Value::cons(Value::fixnum(1), Value::fixnum(2)),
    )
    .unwrap();

    // Ascending precedence: the winner merges LAST. Same ordering as
    // `sort_overlay_ids_by_priority_desc`, reversed.
    assert_eq!(
        list.overlay_property_values_ascending_at_emacs_byte_pos(emacs_byte_pos(4), property, None),
        vec![Value::symbol("bold"), Value::symbol("italic")]
    );
}

#[test]
fn property_resolvers_filter_window_specific_overlays() {
    // Both policies honor the overlay `window` property (GNU
    // `overlay_matches_window`), so a per-window highlight cannot leak into
    // another window through either path.
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("face");
    let windowed = alloc_overlay(2, 10);
    list.insert_overlay(windowed);
    list.overlay_put(windowed, property, Value::symbol("hl-line"))
        .unwrap();
    list.overlay_put(windowed, Value::symbol("window"), Value::make_window(7))
        .unwrap();

    for window_id in [None, Some(7)] {
        assert_eq!(
            list.overlay_property_values_ascending_at_emacs_byte_pos(
                emacs_byte_pos(4),
                property,
                window_id
            ),
            vec![Value::symbol("hl-line")],
            "window_id={window_id:?} should see its own overlay"
        );
    }
    assert!(
        list.overlay_property_values_ascending_at_emacs_byte_pos(
            emacs_byte_pos(4),
            property,
            Some(9)
        )
        .is_empty(),
        "another window must not see a window-specific overlay"
    );
    assert_eq!(
        list.highest_priority_overlay_property_value_at_emacs_byte_pos(
            emacs_byte_pos(4),
            property,
            Some(9)
        ),
        None
    );
}

#[test]
fn property_extent_uses_gnu_winner_across_irrelevant_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let outer = alloc_overlay(2, 18);
    let nested_low = alloc_overlay(5, 8);
    let nested_high = alloc_overlay(10, 14);
    let nil_overlay = alloc_overlay(6, 12);
    for overlay in [outer, nested_low, nested_high, nil_overlay] {
        list.insert_overlay(overlay);
    }
    list.overlay_put(outer, property, Value::symbol("outer"))
        .unwrap();
    list.overlay_put(
        outer,
        Value::symbol("priority"),
        Value::cons(Value::fixnum(4), Value::fixnum(1)),
    )
    .unwrap();
    list.overlay_put(nested_low, property, Value::symbol("low"))
        .unwrap();
    list.overlay_put(nested_low, Value::symbol("priority"), Value::fixnum(3))
        .unwrap();
    list.overlay_put(nested_high, property, Value::symbol("high"))
        .unwrap();
    list.overlay_put(
        nested_high,
        Value::symbol("priority"),
        Value::cons(Value::fixnum(4), Value::fixnum(2)),
    )
    .unwrap();
    list.overlay_put(nil_overlay, property, Value::NIL).unwrap();
    list.overlay_put(nil_overlay, Value::symbol("priority"), Value::fixnum(99))
        .unwrap();

    let bounds = emacs_byte_range(0, 20);
    let outer_extent =
        direct_overlay_property_extent(&list, emacs_byte_pos(7), property, bounds, None).unwrap();
    assert_eq!(outer_extent.overlay(), outer);
    assert_eq!(outer_extent.value(), Value::symbol("outer"));
    assert_eq!(outer_extent.range(), emacs_byte_range(2, 10));

    let high_extent =
        direct_overlay_property_extent(&list, emacs_byte_pos(11), property, bounds, None).unwrap();
    assert_eq!(high_extent.overlay(), nested_high);
    assert_eq!(high_extent.value(), Value::symbol("high"));
    assert_eq!(high_extent.range(), emacs_byte_range(10, 14));
}

#[test]
fn absent_property_cannot_be_turned_into_an_exact_extent() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let nil_overlay = alloc_overlay(2, 6);
    let value_overlay = alloc_overlay(8, 12);
    list.insert_overlay(nil_overlay);
    list.insert_overlay(value_overlay);
    list.overlay_put(nil_overlay, property, Value::NIL).unwrap();
    list.overlay_put(value_overlay, property, Value::symbol("highlight"))
        .unwrap();

    assert!(matches!(
        list.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(4), None, |overlay| {
            list.overlay_get_named(overlay, property)
        }),
        OverlayPropertyAtPoint::Vacant(_)
    ));
}

#[test]
fn property_winner_equality_uses_overlay_identity_not_structural_value_equality() {
    crate::test_utils::init_test_tracing();
    let property = Value::symbol("mouse-face");
    let value = Value::symbol("highlight");
    let mut first_list = OverlayList::new();
    let mut second_list = OverlayList::new();
    let first_overlay = alloc_overlay(2, 6);
    let second_overlay = alloc_overlay(2, 6);
    first_list.insert_overlay(first_overlay);
    second_list.insert_overlay(second_overlay);
    first_list
        .overlay_put(first_overlay, property, value)
        .unwrap();
    second_list
        .overlay_put(second_overlay, property, value)
        .unwrap();

    assert_eq!(
        first_overlay, second_overlay,
        "the fixture requires structurally equal overlay values"
    );
    assert!(!eq_value(&first_overlay, &second_overlay));
    let OverlayPropertyAtPoint::Present(first) = first_list
        .resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(4), None, |overlay| {
            first_list.overlay_get_named(overlay, property)
        })
    else {
        panic!("first overlay should win");
    };
    let OverlayPropertyAtPoint::Present(second) = second_list
        .resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(4), None, |overlay| {
            second_list.overlay_get_named(overlay, property)
        })
    else {
        panic!("second overlay should win");
    };

    assert_ne!(first.winner(), second.winner());
}

#[test]
fn filtered_endpoint_summary_tracks_overlay_property_mutation() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let alias = Value::symbol("alternate-mouse-face");
    let lookup_order = [property, alias];
    let overlay = alloc_overlay(2, 8);
    list.insert_overlay(overlay);
    list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();

    let before = list
        .overlay_property_sweep(
            emacs_byte_range(0, 10),
            None,
            FilteredPropertyResolver {
                overlays: &list,
                lookup_order: &lookup_order,
            },
        )
        .collect::<Vec<_>>();
    assert_eq!(before.len(), 1);
    assert_eq!(before[0].winner(), None);

    list.overlay_put(overlay, alias, Value::symbol("highlight"))
        .unwrap();
    let OverlayPropertyAtPoint::Present(resolution) = list
        .resolve_overlay_property_at_emacs_byte_pos(
            emacs_byte_pos(5),
            None,
            FilteredPropertyResolver {
                overlays: &list,
                lookup_order: &lookup_order,
            },
        )
    else {
        panic!("mutated property should be visible at point");
    };
    let run = resolution
        .sweep(emacs_byte_range(0, 10))
        .expect("filtered sweep")
        .partition_at(emacs_byte_pos(5))
        .expect("winner partition");

    assert_eq!(run.range(), emacs_byte_range(2, 8));
    assert_eq!(run.winner().unwrap().overlay(), overlay);
}

#[test]
fn filtered_property_sweep_prunes_unrelated_endpoint_subtrees() {
    crate::test_utils::init_test_tracing();
    const BUFFER_END: usize = 8_192;
    const CURSOR: usize = BUFFER_END / 2;
    const UNRELATED_OVERLAYS: usize = 2_000;

    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let lookup_order = [property];
    let winner = alloc_overlay(0, BUFFER_END);
    list.insert_overlay(winner);
    list.overlay_put(winner, property, Value::symbol("highlight"))
        .unwrap();
    for index in 0..UNRELATED_OVERLAYS {
        let start = index * 2;
        let overlay = alloc_overlay(start, start + 1);
        list.insert_overlay(overlay);
        list.overlay_put(overlay, Value::symbol("face"), Value::symbol("bold"))
            .unwrap();
    }

    let OverlayPropertyAtPoint::Present(_resolution) = list
        .resolve_overlay_property_at_emacs_byte_pos(
            emacs_byte_pos(CURSOR),
            None,
            FilteredPropertyResolver {
                overlays: &list,
                lookup_order: &lookup_order,
            },
        )
    else {
        panic!("whole-buffer mouse-face overlay should win at the cursor");
    };

    reset_endpoint_search_node_visit_count();
    let runs = list
        .overlay_property_sweep(
            emacs_byte_range(0, BUFFER_END),
            None,
            FilteredPropertyResolver {
                overlays: &list,
                lookup_order: &lookup_order,
            },
        )
        .collect::<Vec<_>>();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].range(), emacs_byte_range(0, BUFFER_END));
    assert_eq!(runs[0].winner().unwrap().overlay(), winner);
    let visits = endpoint_search_node_visit_count();
    assert!(
        visits < 128,
        "filtered sweep should prune {UNRELATED_OVERLAYS} unrelated overlay subtrees; visited {visits} endpoint-tree nodes"
    );
}

#[test]
fn non_nil_fallback_can_promote_a_vacancy_to_a_forward_sweep() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let nil_overlay = alloc_overlay(2, 6);
    let value_overlay = alloc_overlay(8, 12);
    list.insert_overlay(nil_overlay);
    list.insert_overlay(value_overlay);
    list.overlay_put(nil_overlay, property, Value::NIL).unwrap();
    list.overlay_put(value_overlay, property, Value::symbol("highlight"))
        .unwrap();

    let OverlayPropertyAtPoint::Vacant(vacancy) =
        list.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(4), None, |overlay| {
            list.overlay_get_named(overlay, property)
        })
    else {
        panic!("expected an at-point vacancy");
    };
    let fallback =
        NonNilPropertyValue::new(Value::symbol("highlight")).expect("active fallback is non-nil");
    let runs = vacancy
        .with_fallback(fallback)
        .sweep(emacs_byte_range(0, 20))
        .expect("forward sweep")
        .map(|run| run.range())
        .collect::<Vec<_>>();
    assert_eq!(
        runs,
        vec![
            emacs_byte_range(0, 8),
            emacs_byte_range(8, 12),
            emacs_byte_range(12, 20)
        ]
    );
}

#[test]
fn positive_resolution_reuses_its_at_point_frontier_for_extent() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let winner = alloc_overlay(0, 10);
    let unrelated = alloc_overlay(4, 6);
    list.insert_overlay(winner);
    list.insert_overlay(unrelated);
    list.overlay_put(winner, property, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(unrelated, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();

    let resolver_calls = std::cell::Cell::new(0);
    let property_value = |overlay| {
        resolver_calls.set(resolver_calls.get() + 1);
        list.overlay_get_named(overlay, property)
    };
    let OverlayPropertyAtPoint::Present(resolution) =
        list.resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(5), None, property_value)
    else {
        panic!("expected a positive resolution");
    };
    assert_eq!(resolver_calls.get(), 2);

    let extent = resolution
        .extent(emacs_byte_range(0, 10))
        .expect("resolved extent");

    assert_eq!(extent.overlay(), winner);
    assert_eq!(extent.range(), emacs_byte_range(0, 10));
    assert_eq!(
        resolver_calls.get(),
        2,
        "the extent sweep must reuse the active property frontier"
    );
}

#[test]
fn positive_resolution_becomes_a_forward_sweep_without_repeating_at_point_lookup() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let winner = alloc_overlay(0, 10);
    let irrelevant = alloc_overlay(0, 6);
    list.insert_overlay(winner);
    list.insert_overlay(irrelevant);
    list.overlay_put(winner, property, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(irrelevant, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();

    let resolver_calls = std::cell::Cell::new(0);
    let OverlayPropertyAtPoint::Present(resolution) = list
        .resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(0), None, |overlay| {
            resolver_calls.set(resolver_calls.get() + 1);
            list.overlay_get_named(overlay, property)
        })
    else {
        panic!("expected a positive resolution");
    };
    assert_eq!(resolver_calls.get(), 2);

    let runs = resolution
        .sweep(emacs_byte_range(0, 10))
        .expect("forward sweep")
        .collect::<Vec<_>>();

    assert_eq!(runs.len(), 1);
    assert_eq!(runs[0].range(), emacs_byte_range(0, 10));
    assert_eq!(runs[0].winner().unwrap().overlay(), winner);
    assert_eq!(
        resolver_calls.get(),
        2,
        "consuming the at-point proof must reuse its active frontier"
    );
}

#[test]
fn positive_sweep_started_mid_run_preserves_the_semantic_start() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let property = Value::symbol("mouse-face");
    let winner = alloc_overlay(1, 9);
    let earlier_irrelevant = alloc_overlay(2, 3);
    for overlay in [winner, earlier_irrelevant] {
        list.insert_overlay(overlay);
    }
    list.overlay_put(winner, property, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(
        earlier_irrelevant,
        Value::symbol("face"),
        Value::symbol("bold"),
    )
    .unwrap();

    let OverlayPropertyAtPoint::Present(resolution) = list
        .resolve_overlay_property_at_emacs_byte_pos(emacs_byte_pos(5), None, |overlay| {
            list.overlay_get_named(overlay, property)
        })
    else {
        panic!("expected a positive resolution");
    };
    let run = resolution
        .sweep(emacs_byte_range(0, 10))
        .expect("bounded sweep")
        .partition_at(emacs_byte_pos(5))
        .expect("at-point partition");

    assert_eq!(run.range(), emacs_byte_range(1, 9));
    assert_eq!(run.winner().unwrap().overlay(), winner);
}

#[test]
fn windowed_overlay_property_extent_is_restricted_to_its_window() {
    // GNU restricts an overlay carrying a `window` property (e.g. hl-line with a
    // non-sticky flag) to that window: its `mouse-face` must not win in another
    // window. Same rule as the overlay's face / display / invisible.
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mouse_face = Value::symbol("mouse-face");
    let ov = alloc_overlay(0, 10);
    list.insert_overlay(ov);
    list.overlay_put(ov, mouse_face, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(ov, Value::symbol("window"), Value::make_window(7))
        .unwrap();

    let bounds = emacs_byte_range(0, 20);
    let at = emacs_byte_pos(5);
    let winner = |window_id| {
        direct_overlay_property_extent(&list, at, mouse_face, bounds, window_id)
            .map(|extent| extent.overlay())
    };
    // The overlay's own window (or no window context) -> it wins.
    assert_eq!(winner(Some(7)), Some(ov));
    assert_eq!(winner(None), Some(ov));
    // A different window -> filtered out, so there is no winning overlay.
    assert_eq!(winner(Some(8)), None);
}

#[test]
fn property_extent_inspects_each_unrelated_overlay_only_once_per_sweep() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mouse_face = Value::symbol("mouse-face");
    let winner = alloc_overlay(0, 4_100);
    list.insert_overlay(winner);
    list.overlay_put(winner, mouse_face, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(winner, Value::symbol("priority"), Value::fixnum(10))
        .unwrap();

    for index in 0..2_000 {
        let start = 2 + index * 2;
        let unrelated = alloc_overlay(start, start + 1);
        list.insert_overlay(unrelated);
        list.overlay_put(unrelated, Value::symbol("face"), Value::symbol("bold"))
            .unwrap();
    }

    reset_overlay_property_extent_inspection_count();
    let extent = direct_overlay_property_extent(
        &list,
        emacs_byte_pos(2_001),
        mouse_face,
        emacs_byte_range(0, 4_100),
        None,
    )
    .unwrap();
    assert_eq!(extent.overlay(), winner);
    assert_eq!(extent.range(), emacs_byte_range(0, 4_100));
    let inspections = overlay_property_extent_inspection_count();
    assert!(
        inspections <= 2_001,
        "one extent query should inspect each candidate at most once; inspected {inspections} overlays"
    );
}

#[test]
fn property_sweep_streams_effective_winner_runs_across_irrelevant_boundaries() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mouse_face = Value::symbol("mouse-face");
    let outer = alloc_overlay(1, 9);
    let irrelevant = alloc_overlay(2, 3);
    let inner = alloc_overlay(4, 6);
    for overlay in [outer, irrelevant, inner] {
        list.insert_overlay(overlay);
    }
    list.overlay_put(outer, mouse_face, Value::symbol("outer"))
        .unwrap();
    list.overlay_put(irrelevant, Value::symbol("face"), Value::symbol("bold"))
        .unwrap();
    list.overlay_put(inner, mouse_face, Value::symbol("inner"))
        .unwrap();
    list.overlay_put(inner, Value::symbol("priority"), Value::fixnum(1))
        .unwrap();

    let resolver_calls = std::cell::Cell::new(0);
    let runs = list
        .overlay_property_sweep(emacs_byte_range(0, 10), None, |overlay| {
            resolver_calls.set(resolver_calls.get() + 1);
            list.overlay_get_named(overlay, mouse_face)
        })
        .map(|run| {
            (
                run.range(),
                run.winner().map(OverlayPropertyWinner::overlay),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        runs,
        vec![
            (emacs_byte_range(0, 1), None),
            (emacs_byte_range(1, 4), Some(outer)),
            (emacs_byte_range(4, 6), Some(inner)),
            (emacs_byte_range(6, 9), Some(outer)),
            (emacs_byte_range(9, 10), None),
        ]
    );
    assert_eq!(
        resolver_calls.get(),
        3,
        "a monotonic sweep resolves each entering overlay once"
    );
}

#[test]
fn property_sweep_filters_window_overlays_before_forming_partitions() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mouse_face = Value::symbol("mouse-face");
    let windowed = alloc_overlay(2, 8);
    list.insert_overlay(windowed);
    list.overlay_put(windowed, mouse_face, Value::symbol("highlight"))
        .unwrap();
    list.overlay_put(windowed, Value::symbol("window"), Value::make_window(7))
        .unwrap();

    let winners_for = |window_id| {
        list.overlay_property_sweep(emacs_byte_range(0, 10), window_id, |overlay| {
            list.overlay_get_named(overlay, mouse_face)
        })
        .map(|run| {
            (
                run.range(),
                run.winner().map(OverlayPropertyWinner::overlay),
            )
        })
        .collect::<Vec<_>>()
    };

    assert_eq!(
        winners_for(Some(7)),
        vec![
            (emacs_byte_range(0, 2), None),
            (emacs_byte_range(2, 8), Some(windowed)),
            (emacs_byte_range(8, 10), None),
        ]
    );
    assert_eq!(winners_for(Some(8)), vec![(emacs_byte_range(0, 10), None)]);
}

#[test]
fn property_sweep_restarts_only_when_partition_lookup_moves_backwards() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let mouse_face = Value::symbol("mouse-face");
    let left = alloc_overlay(1, 4);
    let right = alloc_overlay(6, 9);
    for overlay in [left, right] {
        list.insert_overlay(overlay);
        list.overlay_put(overlay, mouse_face, Value::symbol("highlight"))
            .unwrap();
    }

    let mut sweep = list.overlay_property_sweep(emacs_byte_range(0, 10), None, |overlay| {
        list.overlay_get_named(overlay, mouse_face)
    });
    assert_eq!(
        sweep.partition_at(emacs_byte_pos(7)).unwrap().range(),
        emacs_byte_range(6, 9)
    );
    assert_eq!(
        sweep.partition_at(emacs_byte_pos(2)).unwrap().range(),
        emacs_byte_range(1, 4)
    );
}
