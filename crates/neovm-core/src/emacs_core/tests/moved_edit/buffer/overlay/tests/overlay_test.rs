use super::*;

use neovm_value::error::Flow;
use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::OverlayData;
use neovm_value::plist;
use neovm_value::value::ValueKind;
use neovm_value::value::scratch_bridge::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BinaryHeap};
use std::hash::Hasher;
use std::rc::Rc;

use neovm_edit::buffer::BufferId;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::heap_types::byte_positions::EmacsByteLen;
use neovm_value::heap_types::byte_positions::EmacsBytePos;
use neovm_value::heap_types::byte_positions::EmacsByteRange;
use neovm_value::value::Value;

use neovm_edit::buffer::overlay::*;
use neovm_value::heap_types::overlay_storage::instrumentation::endpoint_search_node_visit_count;
use neovm_value::test_support::authority::reset_endpoint_search_node_visit_count;

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

        list.assert_index_invariants_for_test();
        for expected in &model {
            assert_eq!(
                list.index_range_for_test(expected.value),
                Some(emacs_byte_range(expected.start, expected.end)),
                "range diverged at edit step {step}, position {position}, length {length}, serial {}",
                expected.value.as_overlay_data().unwrap().serial.get(),
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
fn cached_overlay_snapshot_survives_exact_evaluator_gc() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    eval.eval_str("(progn (insert \"abcd\") (overlay-put (make-overlay 1 4) 'before-string (copy-sequence \"frozen\")))").unwrap();
    let buffer = eval.edit.buffers.current_buffer().unwrap().id();
    // No Rust-held Value roots the copied overlay. The cache belongs to the
    // live buffer and must keep its independent overlay/plist objects alive.
    drop(eval.edit.buffers.get(buffer).unwrap().overlays().snapshot());
    for _ in 0..3 {
        eval.gc_collect_exact();
        let snapshot = eval.edit.buffers.get(buffer).unwrap().overlays().snapshot();
        let overlay = overlays_at(&snapshot, 0)[0];
        assert_eq!(
            snapshot.overlay_get_named(overlay, Value::symbol("before-string")),
            Some(Value::string("frozen"))
        );
        assert_eq!(overlay_end(&snapshot, overlay), Some(3));
    }
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

fn alloc_overlay(start: usize, end: usize) -> Value {
    Value::make_overlay(neovm_value::heap_types::OverlayDataInit {
        serial: neovm_value::heap_types::OverlaySerialSource::Mint,
        plist: Value::NIL,
        buffer: Some(BufferId(1)),
        start,
        end,
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

#[cfg(test)]
#[path = "gnu_identity.rs"]
mod gnu_identity;

#[cfg(test)]
mod property_filter_collision;

#[cfg(test)]
#[path = "random_overlay_model_test.rs"]
mod random_overlay_model_test;
