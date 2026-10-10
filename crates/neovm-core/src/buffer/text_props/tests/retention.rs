use super::*;

/// Set a property on the middle of a fixed 32-character object, then clear
/// the whole object: every cycle splits and merges intervals without changing
/// how many are needed at once.
fn churn(table: &mut TextPropertyTable, cycles: usize) {
    let name = Value::symbol("retention-probe");
    for _ in 0..cycles {
        set_chars_for_object_len(table, 8, 24, 32, vec![(name, Value::fixnum(1))]);
        set_chars_for_object_len(table, 0, 32, 32, Vec::new());
    }
}

#[test]
fn fixed_size_property_churn_reuses_detached_slots() {
    crate::test_utils::init_test_tracing();
    let mut table = TextPropertyTable::new();
    churn(&mut table, 20_000);
    assert_eq!(table.intervals.runs().len(), 1);
    let slots = table.intervals.nodes.len();
    let capacity = table.intervals.nodes.capacity();
    assert!(slots <= 3, "fixed-size churn retained {slots} slots");
    assert!(
        capacity <= 4,
        "fixed-size churn retained capacity {capacity}"
    );
    table.intervals.assert_invariants_for_test();
}

#[test]
fn recycled_slots_keep_cow_snapshots_independent() {
    use std::rc::Rc;
    crate::test_utils::init_test_tracing();
    let name = Value::symbol("retention-probe");
    let mut current = Rc::new(TextPropertyTable::new());
    churn(Rc::make_mut(&mut current), 100);
    // Both sides of the copy-on-write split start with the same free slots
    // and must reuse them without disturbing each other.
    let mut other = Rc::clone(&current);
    Rc::make_mut(&mut current).set_properties_for_object_char_len(
        char_range(4, 28),
        char_len(32),
        vec![(name, Value::fixnum(7))],
    );
    let frozen = current.clone();
    let frozen_runs = frozen.interval_plist_runs_for_test();
    churn(Rc::make_mut(&mut other), 2_000);
    churn(Rc::make_mut(&mut current), 2_000);
    assert_eq!(frozen.interval_plist_runs_for_test(), frozen_runs);
    assert_eq!(get_at_char(&frozen, 16, name), Some(Value::fixnum(7)));
    assert_eq!(get_at_char(&other, 16, name), None);
    assert_eq!(get_at_char(&current, 16, name), None);
    for table in [&current, &other, &frozen] {
        table.intervals.assert_invariants_for_test();
        assert!(table.intervals.nodes.len() <= 5);
    }
}

#[test]
fn trailing_prune_and_whole_delete_reuse_retired_slots() {
    crate::test_utils::init_test_tracing();
    let name = Value::symbol("retention-probe");
    let mut table = TextPropertyTable::new();
    for _ in 0..5_000 {
        set_chars(&mut table, 8, 24, vec![(name, Value::fixnum(1))]);
        // Seed the position and rightmost memos right before retirement.
        assert!(table.intervals.find_id(char_pos(16)).is_some());
        clear_chars(&mut table, 0, 32);
        assert!(table.is_empty());
        table.intervals.assert_invariants_for_test();
        assert!(table.intervals.nodes.len() <= 2);
        set_chars_for_object_len(&mut table, 4, 28, 32, vec![(name, Value::fixnum(2))]);
        delete_char_range(&mut table, 0, 32);
        assert_eq!(table.intervals.nodes.len(), 0);
        assert!(table.intervals.find_id(char_pos(0)).is_none());
        table.intervals.assert_invariants_for_test();
    }
    assert!(table.intervals.nodes.capacity() <= 4);
}

#[test]
fn recycling_matches_character_model_under_mixed_edits() {
    crate::test_utils::init_test_tracing();
    let name = Value::symbol("syntax-table");
    let mut table = TextPropertyTable::new();
    let mut expected = vec![None; 32];
    let mut seed = 0x81a3_769du64;
    let mut random = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        seed as usize
    };
    let mut peak = 0;
    for step in 0..5_000 {
        let start = random() % expected.len();
        let end = start + 1 + random() % (expected.len() - start);
        let value = Value::fixnum((step % 7 + 1) as i64);
        match random() % 6 {
            0 => {
                set_chars_for_object_len(
                    &mut table,
                    start,
                    end,
                    expected.len(),
                    vec![(name, value)],
                );
                expected[start..end].fill(Some(value));
            }
            1 => {
                put_chars_for_object_len(&mut table, start, end, expected.len(), name, value);
                expected[start..end].fill(Some(value));
            }
            2 => {
                remove_chars(&mut table, start, end, name);
                expected[start..end].fill(None);
            }
            3 if expected.len() < 64 => {
                insert_chars_at(&mut table, start, 1);
                expected.insert(start, None);
            }
            4 if expected.len() > 8 => {
                delete_char_range(&mut table, start, start + 1);
                expected.remove(start);
            }
            _ => {
                clear_chars(&mut table, start, end);
                expected[start..end].fill(None);
            }
        }
        table.intervals.assert_invariants_for_test();
        assert!(table.debug_syntax_caches_consistent().is_ok());
        peak = peak.max(table.intervals.runs().len());
        assert!(table.intervals.nodes.len() <= peak + 2);
        for (pos, value) in expected.iter().enumerate() {
            assert_eq!(
                get_at_char(&table, pos, name),
                *value,
                "step {step}, pos {pos}"
            );
            assert_eq!(
                table.intervals.find_id(char_pos(pos)),
                table.intervals.find_id_uncached(char_pos(pos))
            );
        }
    }
}
