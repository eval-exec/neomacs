use super::*;

fn make_table(runs: &[(usize, usize, Value)]) -> TextPropertyTable {
    TextPropertyTable::from_interval_runs_preserving_shape(
        runs.iter()
            .map(|&(start, end, plist)| {
                IntervalRun::new_in_char_range(CharRange::from_usize(start, end), plist)
            })
            .collect(),
    )
}

fn assert_same_queries(table: &TextPropertyTable, view: &TextPropertyReadView, keys: &[Value]) {
    let end = table.intervals.len().get();
    assert_eq!(view.is_empty(), table.is_empty());
    for raw in 0..=end + 2 {
        let pos = CharPos0::new(raw);
        assert_eq!(
            view.get_properties_at_char_pos(pos),
            table.get_properties_at_char_pos(pos)
        );
        assert_eq!(
            view.next_property_change_after_char_pos(pos),
            table.next_property_change_after_char_pos(pos),
            "whole property boundary at {raw}"
        );
        for &key in keys {
            assert_eq!(
                view.get_property_at_char_pos(pos, key)
                    .map(|value| value.bits()),
                table
                    .get_property_at_char_pos(pos, key)
                    .map(|value| value.bits()),
                "property identity at {raw}"
            );
            assert_eq!(
                view.next_single_property_change_after_char_pos(pos, key),
                table.next_single_property_change_after_char_pos(pos, key),
                "next single boundary at {raw}"
            );
            assert_eq!(
                view.previous_single_property_change_before_char_pos(pos, key),
                table.previous_single_property_change_before_char_pos(pos, key),
                "previous single boundary at {raw}"
            );
        }
        for limit in raw..=end + 2 {
            let range = CharRange::from_usize(raw, limit);
            for watched in [keys, &keys[..1], &keys[..0]] {
                assert_eq!(
                    view.has_any_non_nil_property_in_char_range(range, watched),
                    table.has_any_non_nil_property_in_char_range(range, watched),
                    "watched range {raw}..{limit}"
                );
            }
        }
    }
}

#[test]
fn read_view_queries_preserve_nil_gaps_duplicates_tied_boundaries_and_eq_values() {
    crate::test_utils::init_test_tracing();
    let p = Value::symbol("read-view-p");
    let q = Value::symbol("read-view-q");
    let absent = Value::symbol("read-view-absent");
    let left = Value::string("equal-value");
    let right = Value::string("equal-value");
    assert_ne!(left.bits(), right.bits());
    assert_eq!(left, right, "fixture values are equal but not EQ");
    let table = make_table(&[
        (0, 1, Value::NIL),
        (1, 2, Value::list(vec![p, Value::NIL])),
        (
            2,
            3,
            Value::list(vec![
                p,
                Value::fixnum(7),
                p,
                Value::fixnum(9),
                q,
                Value::NIL,
            ]),
        ),
        (
            3,
            4,
            Value::list(vec![
                q,
                Value::NIL,
                p,
                Value::fixnum(7),
                p,
                Value::fixnum(9),
            ]),
        ),
        (4, 5, Value::list(vec![p, left])),
        (5, 6, Value::list(vec![p, right])),
        (6, 7, Value::list(vec![p, right])),
        (7, 8, Value::list(vec![p, right, q, Value::T])),
        (8, 9, Value::NIL),
        (9, 10, Value::list(vec![p, Value::NIL])),
    ]);
    let view = table.read_view();
    assert_same_queries(&table, &view, &[p, q, absent]);
    assert_eq!(
        view.get_property_at_char_pos(CharPos0::new(1), p),
        Some(Value::NIL)
    );
    assert_eq!(view.get_property_at_char_pos(CharPos0::ZERO, p), None);
    assert_eq!(
        view.get_property_at_char_pos(CharPos0::new(2), p),
        Some(Value::fixnum(7))
    );
    assert_eq!(
        view.next_property_change_after_char_pos(CharPos0::new(2)),
        Some(CharPos0::new(4))
    );
    assert_eq!(
        view.next_single_property_change_after_char_pos(CharPos0::ZERO, p),
        Some(CharPos0::new(2))
    );
    assert_eq!(
        view.previous_single_property_change_before_char_pos(CharPos0::new(1), p),
        Some(CharPos0::ZERO)
    );
    assert_eq!(
        view.next_single_property_change_after_char_pos(CharPos0::new(4), p),
        Some(CharPos0::new(5))
    );
    assert_eq!(
        view.previous_single_property_change_before_char_pos(CharPos0::new(5), p),
        Some(CharPos0::new(5))
    );

    let heads = table
        .intervals
        .cursor_at(CharPos0::ZERO)
        .map(|(_, _, node)| node.plist.bits())
        .collect::<Vec<_>>();
    assert_eq!(
        view.root_operands()
            .map(|value| value.bits())
            .collect::<Vec<_>>(),
        heads
    );
    assert_same_queries(
        &TextPropertyTable::new(),
        &TextPropertyTable::new().read_view(),
        &[p, q, absent],
    );
}

#[test]
fn read_view_retains_old_ranges_and_original_plist_aliasing_like_table_clone() {
    crate::test_utils::init_test_tracing();
    let p = Value::symbol("read-view-shared");
    let q = Value::symbol("read-view-replacement");
    let mut source = make_table(&[(0, 4, Value::list(vec![p, Value::fixnum(1)]))]);
    let reference = source.clone();
    let view = source.read_view();
    source.put_property_in_char_range(CharRange::from_usize(0, 4), p, Value::fixnum(2));
    assert_same_queries(&reference, &view, &[p, q]);
    assert_eq!(
        view.get_property_at_char_pos(CharPos0::new(1), p),
        Some(Value::fixnum(2))
    );
    source.set_properties_for_object_char_len(
        CharRange::from_usize(0, 4),
        CharLen::new(4),
        vec![(q, Value::T)],
    );
    assert_same_queries(&reference, &view, &[p, q]);
    assert_eq!(source.get_property_at_char_pos(CharPos0::new(1), p), None);
    assert_eq!(
        view.get_property_at_char_pos(CharPos0::new(1), p),
        Some(Value::fixnum(2))
    );
    // This value-layer test deliberately performs NO GC. An upper-layer
    // owner must lease source + old plist operands before callback/GC tests.
}

#[test]
fn read_view_capture_and_queries_allocate_no_intervals_or_table_clones() {
    crate::test_utils::init_test_tracing();
    let p = Value::symbol("read-view-counter");
    let count = 2048;
    let source = make_table(
        &(0..count)
            .map(|index| {
                (
                    index,
                    index + 1,
                    Value::list(vec![p, Value::fixnum((index & 1) as i64)]),
                )
            })
            .collect::<Vec<_>>(),
    );
    let interval_count = || {
        Value::memory_use_counts_snapshot()
            [crate::tagged::gc::MemoryUseCountSlot::Intervals.index()]
    };
    let calibration_before = interval_count();
    let reference = source.clone();
    assert_eq!(
        interval_count() - calibration_before,
        count as i64,
        "deep-clone counter calibration"
    );
    let before_intervals = interval_count();
    let before_tables = text_property_table_clones_for_test();
    let view = source.read_view();
    for index in [0, count / 2, count - 1] {
        assert_eq!(
            view.get_property_at_char_pos(CharPos0::new(index), p),
            reference.get_property_at_char_pos(CharPos0::new(index), p),
        );
    }
    assert_eq!(interval_count() - before_intervals, 0);
    assert_eq!(text_property_table_clones_for_test() - before_tables, 0);
}

#[test]
fn read_view_fixed_seed_generated_layouts_match_all_table_queries() {
    crate::test_utils::init_test_tracing();
    let p = Value::symbol("read-view-generated-p");
    let q = Value::symbol("read-view-generated-q");
    let r = Value::symbol("read-view-generated-r");
    let absent = Value::symbol("read-view-generated-absent");
    let keys = [p, q, r, absent];
    let mut seed = 0x5245_4144_5649_4557_u64;
    let mut next = || {
        seed = seed
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (seed >> 32) as usize
    };
    let mut generated_counts = std::collections::BTreeSet::new();
    let mut generated_lengths = std::collections::BTreeSet::new();
    let mut witnessed_zero_edges = 0;
    let mut witnessed_nil_gaps = 0;
    let mut witnessed_duplicates = 0;
    let mut witnessed_reordered_boundaries = 0;
    let mut witnessed_shared_heads = 0;
    let mut witnessed_trailing_extents = 0;

    for case in 0..32 {
        // The first two layouts force the count/length extremes. Remaining
        // layouts vary them with a reproducible native generator; required
        // semantic categories below are constructed, never left to chance.
        let extra_runs = match case {
            0 => 0,
            1 => 6,
            _ => next() % 7,
        };
        let first_length = match case {
            0 => 1,
            1 => 3,
            _ => 1 + next() % 3,
        };
        generated_counts.insert(extra_runs);
        generated_lengths.insert(first_length);
        let first_number = 1 + (next() % 17) as i64;
        let first = Value::fixnum(first_number);
        let second = Value::fixnum(first_number + 1);
        let shared = Value::list(vec![p, first, p, second, q, Value::NIL]);
        let reordered = Value::list(vec![q, Value::NIL, p, first, p, second]);
        assert_ne!(shared.bits(), reordered.bits());
        assert_eq!(
            plist_pairs(shared)
                .iter()
                .filter(|(key, _)| key.bits() == p.bits())
                .count(),
            2,
            "generated duplicate-key witness"
        );
        assert!(plists_equal_eq(
            &plist_pairs(shared),
            &plist_pairs(reordered)
        ));
        witnessed_duplicates += 1;

        let mut cursor = 1 + next() % 2;
        let first_start = cursor;
        let mut runs = vec![(0, 0, shared)];
        runs.push((cursor, cursor + first_length, shared));
        cursor += first_length;
        let reordered_start = cursor;
        let reordered_length = 1 + next() % 3;
        runs.push((cursor, cursor + reordered_length, reordered));
        cursor += reordered_length;
        let shared_length = 1 + next() % 3;
        runs.push((cursor, cursor + shared_length, shared));
        cursor += shared_length;
        let prefix_end = cursor;
        runs.push((cursor, cursor, reordered));
        // Omit this span to exercise a normalized implicit nil gap, in
        // addition to the explicit nil runs selected by the random tail.
        cursor += 1 + next() % 2;
        for index in 0..extra_runs {
            cursor += next() % 2;
            let length = if case == 1 && index == 0 {
                0
            } else {
                next() % 4
            };
            generated_lengths.insert(length);
            let plist = match next() % 6 {
                0 => Value::NIL,
                1 => Value::list(vec![r, Value::NIL]),
                2 => shared,
                3 => reordered,
                4 => Value::list(vec![q, Value::fixnum((next() % 5) as i64)]),
                _ => Value::list(vec![r, Value::T, p, first]),
            };
            runs.push((cursor, cursor + length, plist));
            cursor += length;
        }
        let trailing_start = cursor;
        cursor += 1 + next() % 3;
        let extent = cursor;
        runs.push((trailing_start, extent, Value::NIL));
        let outside_zero = extent + 1 + next() % 3;
        runs.push((outside_zero, outside_zero, shared));
        assert!(runs.iter().filter(|(start, end, _)| start == end).count() >= 3);
        witnessed_zero_edges += 1;

        let table = make_table(&runs);
        let view = table.read_view();
        assert_eq!(table.intervals.len().get(), extent);
        assert_eq!(view.get_property_at_char_pos(CharPos0::ZERO, p), None);
        assert_eq!(
            view.get_property_at_char_pos(CharPos0::new(prefix_end), p),
            None
        );
        witnessed_nil_gaps += 1;
        assert_eq!(
            table
                .intervals
                .cursor_at(CharPos0::new(reordered_start))
                .next()
                .unwrap()
                .2
                .plist
                .bits(),
            reordered.bits(),
            "the reordered plist must survive as an actual tied run boundary"
        );
        assert_eq!(
            view.next_property_change_after_char_pos(CharPos0::new(first_start)),
            Some(CharPos0::new(prefix_end)),
            "EQ-equivalent reordered/shared boundaries must be skipped"
        );
        witnessed_reordered_boundaries += 1;
        assert!(
            view.root_operands()
                .filter(|head| head.bits() == shared.bits())
                .count()
                >= 2,
            "distinct native runs must retain the same original plist identity"
        );
        witnessed_shared_heads += 1;
        for key in keys {
            assert_eq!(
                view.get_property_at_char_pos(CharPos0::new(extent - 1), key),
                None
            );
            assert_eq!(
                view.get_property_at_char_pos(CharPos0::new(extent), key),
                None
            );
        }
        witnessed_trailing_extents += 1;
        // Existing comparison checks all six queries, every key including the
        // absent one, every position through extent+2, and every bounded range.
        assert_same_queries(&table, &view, &keys);
    }

    assert!(generated_counts.contains(&0) && generated_counts.contains(&6));
    assert!(generated_lengths.contains(&0));
    assert!(generated_lengths.contains(&1) && generated_lengths.contains(&3));
    assert_eq!(witnessed_zero_edges, 32);
    assert_eq!(witnessed_nil_gaps, 32);
    assert_eq!(witnessed_duplicates, 32);
    assert_eq!(witnessed_reordered_boundaries, 32);
    assert_eq!(witnessed_shared_heads, 32);
    assert_eq!(witnessed_trailing_extents, 32);
}
