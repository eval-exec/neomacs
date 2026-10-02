use super::*;

fn assert_same_tables(actual: &TextPropertyTable, expected: &TextPropertyTable, context: &str) {
    actual.assert_tree_invariants_for_test();
    expected.assert_tree_invariants_for_test();
    assert_eq!(
        actual.interval_plist_runs_for_test(),
        expected.interval_plist_runs_for_test(),
        "{context}"
    );
    assert_eq!(
        actual.mutation_tick, expected.mutation_tick,
        "mutation tick: {context}"
    );
    assert_eq!(
        actual.syntax_prop_tick, expected.syntax_prop_tick,
        "syntax tick: {context}"
    );
    for name in [
        "face",
        "help-echo",
        "invisible",
        "display",
        "category",
        "syntax-table",
    ] {
        let name = Value::symbol(name);
        assert_eq!(
            actual.property_name_presence(name),
            expected.property_name_presence(name),
            "name summary: {context}"
        );
    }
}

fn compare_slice_graft(
    base: &TextPropertyTable,
    source: &TextPropertyTable,
    range: CharRange,
    offset: CharLen,
) {
    let mut expected = base.clone();
    let slice = source.slice_char_range(range);
    expected.append_shifted_at_char_offset(&slice, offset);
    let mut actual = base.clone();
    actual.append_source_slice_at_char_offset(source, range, offset);
    assert_same_tables(
        &actual,
        &expected,
        &format!("range={range:?}, offset={offset:?}"),
    );
}

#[test]
fn source_slice_graft_matches_boundaries_nil_gaps_and_empty_slices() {
    let face = Value::symbol("face");
    let invisible = Value::symbol("invisible");
    let source = TextPropertyTable::from_plist_runs(vec![
        TextPropertyPlistRun::new(char_range(0, 4), vec![]),
        TextPropertyPlistRun::new(char_range(4, 7), vec![(face, Value::symbol("bold"))]),
        TextPropertyPlistRun::new(char_range(7, 10), vec![]),
        TextPropertyPlistRun::new(char_range(10, 13), vec![(invisible, Value::T)]),
        TextPropertyPlistRun::new(char_range(13, 16), vec![]),
    ]);
    let mut base = TextPropertyTable::new();
    set_chars(&mut base, 0, 6, vec![(face, Value::symbol("italic"))]);
    set_chars(&mut base, 9, 20, vec![(invisible, Value::NIL)]);
    for start in 0..=19 {
        for end in start..=20 {
            for offset in [0, 3, 9, 17, 23] {
                compare_slice_graft(&base, &source, char_range(start, end), char_len(offset));
            }
        }
    }
    compare_slice_graft(
        &base,
        &TextPropertyTable::new(),
        char_range(0, 12),
        char_len(3),
    );
}

#[test]
fn source_slice_graft_preserves_duplicate_keys_order_and_malformed_tails() {
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    let composition = Value::symbol("composition");
    let repeated = Value::list(vec![
        face,
        Value::symbol("bold"),
        help,
        Value::string("first"),
        face,
        Value::symbol("italic"),
        composition,
        Value::NIL,
    ]);
    let dangling = Value::list(vec![face]);
    let odd = Value::list(vec![face, Value::symbol("underline"), help]);
    let dotted = Value::cons(
        face,
        Value::cons(Value::symbol("shadow"), Value::symbol("dotted-tail")),
    );
    let source = TextPropertyTable::with_runs_and_names(
        vec![
            IntervalRun::new_in_char_range(char_range(0, 3), repeated),
            IntervalRun::new_in_char_range(char_range(3, 5), Value::NIL),
            IntervalRun::new_in_char_range(char_range(5, 8), dangling),
            IntervalRun::new_in_char_range(char_range(8, 11), odd),
            IntervalRun::new_in_char_range(char_range(11, 14), dotted),
        ],
        ConservativePropertyNames::from_runs(&[
            IntervalRun::new_in_char_range(char_range(0, 3), repeated),
            IntervalRun::new_in_char_range(char_range(5, 8), dangling),
            IntervalRun::new_in_char_range(char_range(8, 11), odd),
            IntervalRun::new_in_char_range(char_range(11, 14), dotted),
        ]),
    );
    let mut base = TextPropertyTable::new();
    set_chars(
        &mut base,
        0,
        18,
        vec![
            (face, Value::symbol("default")),
            (help, Value::string("target")),
        ],
    );
    for start in 0..=15 {
        for end in start..=16 {
            for offset in [0, 2, 9, 18] {
                compare_slice_graft(&base, &source, char_range(start, end), char_len(offset));
            }
        }
    }
}

#[test]
fn source_slice_graft_matches_random_overlapping_destinations() {
    let mut rng = 0x5_11ce_d5_u64;
    for case in 0..600 {
        let target_len = 1 + lcg(&mut rng) as usize % 120;
        let target_segments = lcg(&mut rng) as usize % 10;
        let base = random_table(&mut rng, target_len, target_segments);
        let source_len = 1 + lcg(&mut rng) as usize % 48;
        let source_segments = 1 + lcg(&mut rng) as usize % 10;
        let source = random_table(&mut rng, source_len, source_segments);
        let a = lcg(&mut rng) as usize % (source_len + 8);
        let b = lcg(&mut rng) as usize % (source_len + 8);
        let (start, end) = if a <= b { (a, b) } else { (b, a) };
        let offset = lcg(&mut rng) as usize % (target_len + 20);
        let mut expected = base.clone();
        expected.append_shifted_at_char_offset(
            &source.slice_char_range(char_range(start, end)),
            char_len(offset),
        );
        let mut actual = base.clone();
        actual.append_source_slice_at_char_offset(
            &source,
            char_range(start, end),
            char_len(offset),
        );
        assert_same_tables(
            &actual,
            &expected,
            &format!("case {case}, [{start}, {end}), offset {offset}"),
        );
    }
}

#[test]
fn source_slice_graft_detaches_plists_and_preserves_unchanged_remainder_identity() {
    let face = Value::symbol("face");
    let help = Value::symbol("help-echo");
    let nested_value = Value::list(vec![Value::symbol("nested")]);
    let mut source = TextPropertyTable::new();
    set_chars(
        &mut source,
        0,
        12,
        vec![(face, Value::symbol("italic")), (help, nested_value)],
    );
    let source_plist = source.raw_plist_at_for_test(char_pos(3)).unwrap();
    let mut target = TextPropertyTable::new();
    set_chars(&mut target, 0, 30, vec![(face, Value::symbol("bold"))]);
    let original = target.raw_plist_at_for_test(char_pos(5)).unwrap();
    target.append_source_slice_at_char_offset(&source, char_range(2, 8), char_len(10));
    let left = target.raw_plist_at_for_test(char_pos(5)).unwrap();
    let grafted = target.raw_plist_at_for_test(char_pos(12)).unwrap();
    let right = target.raw_plist_at_for_test(char_pos(25)).unwrap();
    assert_ne!(left.bits(), original.bits(), "left remainder is detached");
    assert_eq!(left, original);
    assert_ne!(
        grafted.bits(),
        source_plist.bits(),
        "grafted plist is detached from source"
    );
    assert_eq!(grafted, source_plist);
    assert_eq!(
        right.bits(),
        original.bits(),
        "right remainder retains its old plist identity"
    );
    assert_eq!(
        get_at_char(&target, 12, help).unwrap().bits(),
        nested_value.bits(),
        "property values remain shallow-shared"
    );
    let target_before_source_write = target.interval_plist_runs_for_test();
    put_chars(&mut source, 0, 12, face, Value::symbol("source-after"));
    put_chars(&mut source, 0, 12, help, Value::symbol("help-after"));
    assert_eq!(
        target.interval_plist_runs_for_test(),
        target_before_source_write,
        "source writes cannot mutate produced output"
    );
    let source_before_target_write = source.interval_plist_runs_for_test();
    put_chars(&mut target, 10, 16, face, Value::symbol("target-after"));
    put_chars(&mut target, 10, 16, help, Value::NIL);
    assert_eq!(
        source.interval_plist_runs_for_test(),
        source_before_target_write,
        "output writes cannot mutate source"
    );
}

#[test]
fn source_slice_graft_matches_syntax_cache_and_presence_updates() {
    let face = Value::symbol("face");
    let syntax = Value::symbol("syntax-table");
    let category = Value::symbol("category");
    let mut source = TextPropertyTable::new();
    set_chars(&mut source, 0, 18, vec![(face, Value::symbol("bold"))]);
    put_chars(&mut source, 4, 8, syntax, Value::fixnum(1));
    put_chars(
        &mut source,
        12,
        16,
        category,
        Value::symbol("slice-category"),
    );
    let mut base = TextPropertyTable::new();
    set_chars(&mut base, 0, 30, vec![(face, Value::symbol("italic"))]);
    put_chars(&mut base, 1, 4, category, Value::symbol("target-category"));
    let _ = base.syntax_prop_free_run_end(char_pos(0), char_pos(30));
    let _ = base.has_any_syntax_prop_interval();
    for range in [
        char_range(0, 3),
        char_range(3, 9),
        char_range(9, 18),
        char_range(0, 0),
        char_range(19, 23),
    ] {
        let mut expected = base.clone();
        expected.append_shifted_at_char_offset(&source.slice_char_range(range), char_len(6));
        let mut actual = base.clone();
        actual.append_source_slice_at_char_offset(&source, range, char_len(6));
        assert_same_tables(&actual, &expected, "syntax slice graft");
        assert_eq!(
            actual.has_any_syntax_prop_interval(),
            expected.has_any_syntax_prop_interval()
        );
        for pos in 0..=32 {
            assert_eq!(
                actual.syntax_prop_free_run_end(char_pos(pos), char_pos(33)),
                expected.syntax_prop_free_run_end(char_pos(pos), char_pos(33)),
                "syntax query at {pos}"
            );
        }
        actual
            .debug_syntax_caches_consistent()
            .expect("actual syntax cache");
        expected
            .debug_syntax_caches_consistent()
            .expect("legacy syntax cache");
    }
}

#[test]
fn source_slice_graft_avoids_the_intermediate_plist_copy_layer() {
    use crate::tagged::gc::MemoryUseCountSlot;
    let face = Value::symbol("face");
    let source = TextPropertyTable::from_plist_runs(vec![
        TextPropertyPlistRun::new(char_range(0, 3), vec![(face, Value::symbol("bold"))]),
        TextPropertyPlistRun::new(char_range(3, 7), vec![(face, Value::symbol("italic"))]),
        TextPropertyPlistRun::new(char_range(7, 10), vec![(face, Value::symbol("underline"))]),
    ]);
    let conses = || Value::memory_use_counts_snapshot()[MemoryUseCountSlot::ConsCells.index()];
    let mut legacy = TextPropertyTable::new();
    let before = conses();
    let slice = source.slice_char_range(char_range(2, 8));
    legacy.append_shifted_at_char_offset(&slice, char_len(0));
    let legacy_conses = conses() - before;
    let mut direct = TextPropertyTable::new();
    let before = conses();
    direct.append_source_slice_at_char_offset(&source, char_range(2, 8), char_len(0));
    let direct_conses = conses() - before;
    assert_same_tables(&direct, &legacy, "cons allocation proof");
    assert_eq!(
        legacy_conses, 12,
        "slice and graft each copy three two-cons plists"
    );
    assert_eq!(direct_conses, 6, "only the graft copies the three plists");
}
