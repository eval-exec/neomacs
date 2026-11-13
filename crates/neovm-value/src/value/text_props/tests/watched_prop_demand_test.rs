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

#[test]
fn watched_state_change_distinguishes_explicit_nil_from_absent() {
    let face = Value::symbol("face");
    let other = Value::symbol("other");
    let table = TextPropertyTable::from_plist_runs(vec![
        TextPropertyPlistRun::new(char_range(0, 2), vec![]),
        TextPropertyPlistRun::new(char_range(2, 4), vec![(face, Value::NIL)]),
        TextPropertyPlistRun::new(
            char_range(4, 6),
            vec![(face, Value::NIL), (other, Value::T)],
        ),
        TextPropertyPlistRun::new(char_range(6, 8), vec![]),
    ]);
    let keys = [face];

    // Presence changes at 2 (absent -> explicit nil) and 6 (explicit nil ->
    // absent). The unrelated `other` interval at 4 is not a watched change.
    assert_eq!(
        table.next_watched_property_state_change(char_pos(0), char_pos(8), &keys),
        char_pos(2)
    );
    assert_eq!(
        table.next_watched_property_state_change(char_pos(2), char_pos(8), &keys),
        char_pos(6)
    );
    assert_eq!(
        table.next_watched_property_state_change(char_pos(6), char_pos(8), &keys),
        char_pos(8)
    );

    // Value-only scans see nil throughout, so they cannot report the
    // absent/explicit-nil transitions that shadow lower-priority sources.
    assert_eq!(
        table.next_watched_property_change(char_pos(0), char_pos(8), &keys),
        char_pos(8)
    );

    assert_eq!(
        table.previous_watched_property_state_change_before_char_pos(char_pos(8), &keys),
        Some(char_pos(6))
    );
    assert_eq!(
        table.previous_watched_property_state_change_before_char_pos(char_pos(6), &keys),
        Some(char_pos(6))
    );
    assert_eq!(
        table.previous_watched_property_state_change_before_char_pos(char_pos(4), &keys),
        Some(char_pos(2))
    );
    assert_eq!(
        table.previous_watched_property_state_change_before_char_pos(char_pos(2), &keys),
        Some(char_pos(2))
    );
    assert_eq!(
        table.previous_watched_property_state_change_before_char_pos(char_pos(0), &keys),
        None
    );
}

#[test]
fn syntax_state_runs_split_on_presence_and_walk_backward_from_query() {
    let syntax_table = Value::symbol("syntax-table");
    let category = Value::symbol("category");
    let table = TextPropertyTable::from_plist_runs(vec![
        TextPropertyPlistRun::new(char_range(0, 2), vec![(syntax_table, Value::T)]),
        TextPropertyPlistRun::new(
            char_range(2, 3),
            vec![(syntax_table, Value::T), (category, Value::NIL)],
        ),
        TextPropertyPlistRun::new(char_range(3, 5), vec![(syntax_table, Value::T)]),
        TextPropertyPlistRun::new(
            char_range(5, 6),
            vec![(syntax_table, Value::T), (category, Value::NIL)],
        ),
        TextPropertyPlistRun::new(char_range(6, 8), vec![(syntax_table, Value::T)]),
    ]);

    // Value-only runs would coalesce the explicit-nil category interval with
    // the absent neighbours; presence must split at 2 and 3.
    assert_eq!(
        table.syntax_prop_free_run_end(char_pos(1), char_pos(8)),
        char_pos(2)
    );
    assert_eq!(
        table.syntax_prop_free_run_end(char_pos(2), char_pos(8)),
        char_pos(3)
    );
    assert_eq!(
        table.syntax_prop_free_run_end(char_pos(3), char_pos(8)),
        char_pos(5)
    );

    // Backward run starts are resolved from the queried position, not from 0.
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(7)),
        char_pos(6)
    );
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(4)),
        char_pos(3)
    );
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(2)),
        char_pos(2)
    );
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(1)),
        char_pos(0)
    );
}

#[test]
fn syntax_backward_run_start_rechecks_stale_range_coverage() {
    let category = Value::symbol("category");
    let mut table = TextPropertyTable::from_plist_runs(vec![TextPropertyPlistRun::new(
        char_range(0, 8),
        vec![(category, Value::symbol("cat"))],
    )]);
    // Warm the cached syntax-range list over one wide covered range.
    assert_eq!(
        table.syntax_prop_free_run_end(char_pos(0), char_pos(8)),
        char_pos(8)
    );
    // Removal leaves the cached coverage wider than the actual prop interval.
    let _ = table.remove_property_in_char_range(char_range(2, 4), category);
    assert_eq!(
        table.syntax_prop_free_run_end(char_pos(0), char_pos(8)),
        char_pos(2)
    );
    // Present run [4,8) must start at 4, not at the stale coverage start 0.
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(6)),
        char_pos(4)
    );
    // The gap [2,4) starts after the real prop interval [0,2), not at 0.
    assert_eq!(
        table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(3)),
        char_pos(2)
    );
}

#[test]
fn syntax_backward_run_start_preserves_pruned_tail_boundary() {
    let category = Value::symbol("category");
    for warm_cache in [false, true] {
        for unrelated_tail in [false, true] {
            let mut table = TextPropertyTable::from_plist_runs(vec![TextPropertyPlistRun::new(
                char_range(0, 10),
                vec![(category, Value::symbol("cat"))],
            )]);
            if warm_cache {
                table.syntax_prop_free_run_end(char_pos(0), char_pos(10));
            }
            // Clearing all properties prunes the real intervals after 5.
            table.remove_all_properties_in_char_range(char_range(5, 10));
            if unrelated_tail {
                // The last real interval may itself be syntax-free: its
                // history property must not shorten the implicit gap.
                table.put_property_in_char_range(
                    char_range(5, 7),
                    Value::symbol("history-event"),
                    Value::T,
                );
            }
            for pos in 5..=10 {
                assert_eq!(
                    table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(pos)),
                    char_pos(5),
                    "warm_cache={warm_cache}, unrelated_tail={unrelated_tail}, pos={pos}"
                );
            }
            table.remove_all_properties_in_char_range(char_range(0, 10));
            assert_eq!(
                table.previous_syntax_prop_state_run_start_at_char_pos(char_pos(8)),
                char_pos(0),
                "an empty table still has a prop-free run starting at zero"
            );
        }
    }
}
