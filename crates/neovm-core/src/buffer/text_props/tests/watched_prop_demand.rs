use super::*;

fn query_with_mode(
    table: &TextPropertyTable,
    pos: usize,
    cap: usize,
    keys: &[Value],
    demand: WatchedPropDemandMode,
) -> CharPos0 {
    with_watched_prop_demand_mode_for_test(demand, || {
        table.next_watched_property_change(char_pos(pos), char_pos(cap), keys)
    })
}

// A character-by-character property observation is independent of interval
// traversal/advance policy. Beyond tree coverage every watched key is nil.
fn point_reference(table: &TextPropertyTable, pos: usize, cap: usize, keys: &[Value]) -> CharPos0 {
    if cap <= pos {
        return char_pos(cap);
    }
    let at = |position| {
        let plist = table
            .interval_plist_at_char_pos(char_pos(position))
            .unwrap_or(Value::NIL);
        keys.iter()
            .map(|key| plist_value_get(plist, *key).unwrap_or(Value::NIL))
            .collect::<Vec<_>>()
    };
    let initial = at(pos);
    let covered = table.intervals.len().get();
    for position in pos.saturating_add(1)..cap.min(covered.saturating_add(1)) {
        let candidate = at(position);
        if !initial
            .iter()
            .zip(candidate.iter())
            .all(|(a, b)| eq_value(a, b))
        {
            return char_pos(position);
        }
    }
    char_pos(cap)
}

fn assert_query_modes(table: &TextPropertyTable, pos: usize, cap: usize, keys: &[Value]) {
    let expected = point_reference(table, pos, cap, keys);
    let before = table.interval_plist_runs_for_test();
    let ticks = (table.mutation_tick, table.syntax_prop_tick);
    for demand in [WatchedPropDemandMode::Off, WatchedPropDemandMode::On] {
        assert_eq!(
            query_with_mode(table, pos, cap, keys, demand),
            expected,
            "pos={pos}, cap={cap}, demand={demand:?}, keys={keys:?}"
        );
    }
    assert_eq!(
        table.interval_plist_runs_for_test(),
        before,
        "query writes no plist"
    );
    assert_eq!(
        (table.mutation_tick, table.syntax_prop_tick),
        ticks,
        "query changes no tick"
    );
}

fn assert_boundary_queries(table: &TextPropertyTable, total: usize) {
    let face = Value::symbol("face");
    let invisible = Value::symbol("invisible");
    let display = Value::symbol("display");
    let absent = Value::symbol("watched-demand-absent");
    let keys: &[&[Value]] = &[
        &[],
        &[face],
        &[invisible],
        &[display],
        &[absent],
        &[face, invisible, display],
    ];
    for pos in 0..=total + 2 {
        for cap in [0, pos, pos + 1, total / 2, total, total + 3, usize::MAX] {
            for keys in keys {
                assert_query_modes(table, pos, cap, keys);
            }
        }
    }
}

#[test]
fn watched_prop_demand_knob_defaults_on_only_when_absent_and_forces_its_own_mode() {
    assert_eq!(
        parse_watched_prop_demand_mode(None),
        WatchedPropDemandMode::On
    );
    for value in [
        Some(""),
        Some("off"),
        Some("0"),
        Some("deferred"),
        Some("invalid"),
    ] {
        assert_eq!(
            parse_watched_prop_demand_mode(value),
            WatchedPropDemandMode::Off
        );
    }
    for value in [Some("on"), Some(" ON ")] {
        assert_eq!(
            parse_watched_prop_demand_mode(value),
            WatchedPropDemandMode::On
        );
    }
    with_watched_prop_demand_mode_for_test(WatchedPropDemandMode::Off, || {
        assert_eq!(watched_prop_demand_mode(), WatchedPropDemandMode::Off);
        with_watched_prop_demand_mode_for_test(WatchedPropDemandMode::On, || {
            assert_eq!(watched_prop_demand_mode(), WatchedPropDemandMode::On);
        });
        assert_eq!(watched_prop_demand_mode(), WatchedPropDemandMode::Off);
    });
}

#[test]
fn watched_prop_demand_matches_empty_nil_gaps_boundaries_and_unbounded_caps() {
    assert_boundary_queries(&TextPropertyTable::new(), 7);
    let face = Value::symbol("face");
    let invisible = Value::symbol("invisible");
    let table = TextPropertyTable::from_plist_runs(vec![
        TextPropertyPlistRun::new(char_range(0, 3), vec![]),
        TextPropertyPlistRun::new(char_range(3, 5), vec![(face, Value::symbol("bold"))]),
        TextPropertyPlistRun::new(char_range(5, 8), vec![(face, Value::symbol("italic"))]),
        TextPropertyPlistRun::new(char_range(8, 12), vec![(invisible, Value::T)]),
        TextPropertyPlistRun::new(char_range(12, 15), vec![]),
        TextPropertyPlistRun::new(char_range(15, 18), vec![(invisible, Value::T)]),
    ]);
    assert_boundary_queries(&table, 20);
}

#[test]
fn watched_prop_demand_preserves_duplicate_keys_malformed_plists_and_eq_values() {
    let invisible = Value::symbol("invisible");
    let display = Value::symbol("display");
    let first = Value::list(vec![Value::fixnum(1)]);
    let equal_but_distinct = Value::list(vec![Value::fixnum(1)]);
    let runs = vec![
        IntervalRun::new_in_char_range(
            char_range(0, 3),
            Value::list(vec![invisible, Value::NIL, invisible, Value::T]),
        ),
        IntervalRun::new_in_char_range(char_range(3, 5), Value::NIL),
        IntervalRun::new_in_char_range(
            char_range(5, 8),
            Value::list(vec![display, first, invisible]),
        ),
        IntervalRun::new_in_char_range(
            char_range(8, 11),
            Value::cons(display, Value::cons(first, Value::symbol("tail"))),
        ),
        IntervalRun::new_in_char_range(
            char_range(11, 14),
            Value::list(vec![display, equal_but_distinct]),
        ),
        IntervalRun::new_in_char_range(char_range(14, 16), Value::list(vec![invisible])),
    ];
    let table = TextPropertyTable::with_runs_and_names(
        runs.clone(),
        ConservativePropertyNames::from_runs(&runs),
    );
    assert_boundary_queries(&table, 18);
    assert_eq!(
        query_with_mode(&table, 0, 5, &[invisible], WatchedPropDemandMode::On),
        char_pos(5)
    );
    assert_eq!(
        query_with_mode(&table, 5, 16, &[display], WatchedPropDemandMode::On),
        char_pos(11)
    );
}

#[test]
fn watched_prop_demand_queries_follow_property_plist_and_text_mutations() {
    let face = Value::symbol("face");
    let invisible = Value::symbol("invisible");
    let display = Value::symbol("display");
    let mut table = TextPropertyTable::new();
    let mut total = 40usize;
    for index in 0..10 {
        put_chars(
            &mut table,
            index * 4,
            index * 4 + 4,
            face,
            Value::fixnum(index as i64),
        );
    }
    put_chars(&mut table, 0, 8, invisible, Value::NIL);
    assert_boundary_queries(&table, total);
    let plist = table
        .raw_plist_at_for_test(char_pos(0))
        .expect("live plist");
    assert_eq!(plist.cons_car(), invisible);
    plist.cons_cdr().set_car(Value::T);
    assert_boundary_queries(&table, total);
    for step in 0..30 {
        let start = step * 13 % total;
        let end = (start + 3).min(total);
        match step % 6 {
            0 => {
                put_chars(&mut table, start, end, invisible, Value::T);
            }
            1 => {
                remove_chars(&mut table, start, end, invisible);
            }
            2 => {
                put_chars(&mut table, start, end, display, Value::fixnum(step as i64));
            }
            3 => {
                table.adjust_for_insert_at_char_pos(char_pos(start), char_len(2));
                total += 2;
            }
            4 => {
                table.adjust_for_delete_char_range(char_range(start, end));
                total -= end - start;
            }
            _ => {
                set_chars(
                    &mut table,
                    start,
                    end,
                    vec![(face, Value::fixnum(step as i64))],
                );
            }
        }
        table.assert_tree_invariants_for_test();
        assert_boundary_queries(&table, total);
    }
}

#[test]
fn watched_prop_demand_matches_randomized_queries_after_overlapping_edits() {
    let mut rng = 0xd5_b0_7eed_u64;
    let keys = [
        Value::symbol("face"),
        Value::symbol("invisible"),
        Value::symbol("display"),
    ];
    for _case in 0..120 {
        let mut total = 8 + lcg(&mut rng) as usize % 56;
        let mut table = TextPropertyTable::new();
        for step in 0..24 {
            let start = lcg(&mut rng) as usize % total;
            let end = (start + 1 + lcg(&mut rng) as usize % 9).min(total);
            let key = keys[lcg(&mut rng) as usize % keys.len()];
            match step % 6 {
                0 => {
                    put_chars(
                        &mut table,
                        start,
                        end,
                        key,
                        Value::fixnum(lcg(&mut rng) as i64 % 3),
                    );
                }
                1 => {
                    remove_chars(&mut table, start, end, key);
                }
                2 => {
                    set_chars(&mut table, start, end, vec![(key, Value::T)]);
                }
                3 => {
                    table.adjust_for_insert_at_char_pos(char_pos(start), char_len(1));
                    total += 1;
                }
                4 if total > 10 => {
                    table.adjust_for_delete_char_range(char_range(start, end));
                    total -= end - start;
                }
                _ => {
                    clear_chars(&mut table, start, end);
                }
            }
            table.assert_tree_invariants_for_test();
            for _query in 0..8 {
                let pos = lcg(&mut rng) as usize % (total + 4);
                let cap = lcg(&mut rng) as usize % (total + 6);
                let key = keys[lcg(&mut rng) as usize % keys.len()];
                for names in [&[][..], &[key][..], &keys[..]] {
                    assert_query_modes(&table, pos, cap, names);
                }
            }
        }
    }
}
