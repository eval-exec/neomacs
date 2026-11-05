//! Deterministic overlay mutation model and fixed GNU ordering checks.
//! The vector model checks semantics, not B+ shape, allocation sizes or time.
use super::*;

#[derive(Clone, Copy)]
struct ModelOverlay {
    value: Value,
    owner: usize,
    start: usize,
    end: usize,
    front: bool,
    rear: bool,
    priority: i64,
}

fn next_random(state: &mut u64) -> u64 {
    *state = state
        .wrapping_mul(6_364_136_223_846_793_005)
        .wrapping_add(1_442_695_040_888_963_407);
    *state
}

fn identity_set(values: impl IntoIterator<Item = Value>) -> Vec<usize> {
    let mut values: Vec<_> = values.into_iter().map(Value::bits).collect();
    values.sort_unstable();
    values
}

fn make_model_overlay(
    lists: &mut [OverlayList; 2],
    slot: usize,
    owner: usize,
    start: usize,
    end: usize,
    bits: u64,
) -> ModelOverlay {
    let value = alloc_overlay(start, end);
    let _ = value.with_overlay_data_mut(|data| {
        data.buffer = Some(BufferId(owner as u64 + 1));
    });
    let front = bits & 1 != 0;
    let rear = bits & 2 != 0;
    // Unique primary priorities make the independent numeric order total.
    // Mix explicit nil, integer and cons priorities without modeling GNU's
    // intentionally nontransitive equal-priority/nesting comparator here.
    let priority = if slot == 0 { 0 } else { slot as i64 * 2 - 127 };
    let property = if slot == 0 {
        Value::NIL
    } else if slot % 2 == 0 {
        Value::cons(Value::fixnum(priority), Value::fixnum(slot as i64))
    } else {
        Value::fixnum(priority)
    };
    lists[owner].insert_overlay(value);
    lists[owner].set_front_advance(value, front);
    lists[owner].set_rear_advance(value, rear);
    lists[owner]
        .overlay_put(value, Value::symbol("priority"), property)
        .unwrap();
    ModelOverlay {
        value,
        owner,
        start,
        end,
        front,
        rear,
        priority,
    }
}

fn check_model(lists: &[OverlayList; 2], model: &[Option<ModelOverlay>], step: usize) {
    for (owner, list) in lists.iter().enumerate() {
        let live: Vec<_> = model
            .iter()
            .flatten()
            .filter(|entry| entry.owner == owner)
            .collect();
        assert_eq!(
            list.len(),
            live.len(),
            "membership count: step {step}, owner {owner}"
        );
        list.index.assert_invariants();
        // This assertion is a membership set, not a fabricated GNU topology
        // oracle. Fixed GNU fixtures below retain their exact vector order.
        assert_eq!(
            identity_set(list.overlays_in_gnu_lists_order()),
            identity_set(live.iter().map(|entry| entry.value)),
            "lists membership: step {step}, owner {owner}"
        );
        for entry in &live {
            assert_eq!(
                overlay_start(list, entry.value),
                Some(entry.start),
                "start: step {step}"
            );
            assert_eq!(
                overlay_end(list, entry.value),
                Some(entry.end),
                "end: step {step}"
            );
            let data = entry.value.as_overlay_data().unwrap();
            assert_eq!(
                data.buffer,
                Some(BufferId(owner as u64 + 1)),
                "owner: step {step}"
            );
            assert_eq!(
                (data.front_advance, data.rear_advance),
                (entry.front, entry.rear)
            );
        }
        let points: Vec<usize> = if step % 257 == 0 {
            (0..=128).collect()
        } else {
            vec![0, 8, step.wrapping_mul(17) % 96, 64, 128]
        };
        for point in points {
            let expected: Vec<_> = live
                .iter()
                .filter(|entry| entry.start <= point && point < entry.end)
                .map(|entry| entry.value)
                .collect();
            let actual = overlays_at(list, point);
            assert_eq!(
                identity_set(actual.iter().copied()),
                identity_set(expected),
                "at: step {step}, owner {owner}, point {point}"
            );
            let mut sorted = actual;
            list.sort_overlay_ids_by_priority_desc(&mut sorted);
            let mut priority_expected: Vec<_> = live
                .iter()
                .filter(|entry| entry.start <= point && point < entry.end)
                .collect();
            priority_expected.sort_by_key(|entry| std::cmp::Reverse(entry.priority));
            assert_eq!(
                sorted.iter().copied().map(Value::bits).collect::<Vec<_>>(),
                priority_expected
                    .iter()
                    .map(|entry| entry.value.bits())
                    .collect::<Vec<_>>(),
                "priority: step {step}, owner {owner}, point {point}"
            );
            let next = live
                .iter()
                .flat_map(|entry| [entry.start, entry.end])
                .filter(|boundary| *boundary > point)
                .min();
            let previous = live
                .iter()
                .flat_map(|entry| [entry.start, entry.end])
                .filter(|boundary| *boundary < point)
                .max();
            assert_eq!(
                list.next_boundary_after_emacs_byte_pos(emacs_byte_pos(point))
                    .map(EmacsBytePos::get),
                next,
                "next: step {step}, point {point}"
            );
            assert_eq!(
                list.previous_boundary_before_emacs_byte_pos(emacs_byte_pos(point))
                    .map(EmacsBytePos::get),
                previous,
                "previous: step {step}, point {point}"
            );
        }
        // Independent half-open region model, with GNU's empty-overlay and
        // accessible-end exceptions. Do not call production overlap helpers.
        for (start, end, accessible_end) in
            [(0, 128, 128), (8, 8, 128), (16, 32, 128), (16, 32, 32)]
        {
            let expected = live
                .iter()
                .filter(|entry| {
                    if entry.start == entry.end {
                        entry.start == start
                            || (start < entry.start && entry.start < end)
                            || (entry.start == end && end == accessible_end)
                    } else if start == end {
                        entry.start < start && start < entry.end
                    } else {
                        entry.start < end && entry.end > start
                    }
                })
                .map(|entry| entry.value);
            assert_eq!(
                identity_set(overlays_in_region(list, start, end, accessible_end)),
                identity_set(expected),
                "in: step {step}, owner {owner}, range {start}..{end}"
            );
        }
        if step % 257 == 0 {
            let first = overlays_at(list, 8);
            let second = overlays_at(list, 32);
            let mut first_iter = list.iter_overlays_at_emacs_byte_pos(emacs_byte_pos(8));
            let mut second_iter = list.iter_overlays_at_emacs_byte_pos(emacs_byte_pos(32));
            for index in 0..=first.len().max(second.len()) {
                assert_eq!(
                    first_iter.next().map(Value::bits),
                    first.get(index).copied().map(Value::bits)
                );
                assert_eq!(
                    second_iter.next().map(Value::bits),
                    second.get(index).copied().map(Value::bits)
                );
            }
        }
    }
}

#[test]
fn overlay_make_move_delete_matches_bruteforce_10000_operations() {
    crate::test_utils::init_test_tracing();
    const SEED: u64 = 0x20a1_7e5d_91c3_6b47;
    const OPERATIONS: usize = 10_000;
    const SLOTS: usize = 128;
    tracing::info!(
        seed = SEED,
        operations = OPERATIONS,
        "overlay model started"
    );
    let mut random = SEED;
    let mut lists = [OverlayList::new(), OverlayList::new()];
    let mut model = vec![None; SLOTS];
    // Start multilevel with equal starts and empty records in both owners.
    for (slot, entry) in model.iter_mut().enumerate() {
        let start = slot % 16 * 4;
        let end = start + slot % 7;
        *entry = Some(make_model_overlay(
            &mut lists,
            slot,
            slot % 2,
            start,
            end,
            next_random(&mut random),
        ));
    }
    check_model(&lists, &model, 0);
    let mut counts = [0_usize; 4]; // make, move, delete, cross-owner move
    for step in 0..OPERATIONS {
        let bits = next_random(&mut random);
        let occupied: Vec<_> = model
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.map(|_| slot))
            .collect();
        let vacant: Vec<_> = model
            .iter()
            .enumerate()
            .filter_map(|(slot, entry)| entry.is_none().then_some(slot))
            .collect();
        let mut operation = ((bits >> 32) % 4) as usize;
        // Periodic draining exercises empty/refill without counting a no-op
        // delete as one of the requested ten thousand operations.
        if step % 1024 >= 896 && !occupied.is_empty() {
            operation = 2;
        }
        if occupied.is_empty() {
            operation = 0;
        }
        if operation == 0 && vacant.is_empty() {
            operation = 1;
        }
        let choices = if operation == 0 { &vacant } else { &occupied };
        let slot = choices[(next_random(&mut random) >> 32) as usize % choices.len()];
        let start = ((next_random(&mut random) >> 32) as usize % 16) * 4;
        let end = start + ((bits >> 16) as usize % 9);
        match operation {
            0 => {
                let owner = (bits >> 40) as usize % 2;
                model[slot] = Some(make_model_overlay(
                    &mut lists, slot, owner, start, end, bits,
                ));
            }
            1 => {
                let mut entry = model[slot].unwrap();
                // Some true same-range moves, not just start-changing moves.
                let (start, end) = if step % 7 == 0 {
                    (entry.start, entry.end)
                } else {
                    (start, end)
                };
                lists[entry.owner]
                    .move_overlay_to_emacs_byte_range(entry.value, emacs_byte_range(start, end));
                entry.start = start;
                entry.end = end;
                model[slot] = Some(entry);
            }
            2 => {
                let entry = model[slot].take().unwrap();
                assert!(
                    lists[entry.owner].delete_overlay(entry.value),
                    "delete: step {step}"
                );
                assert!(overlay_live_buffer(entry.value).is_none());
                assert_eq!(overlay_start(&lists[entry.owner], entry.value), None);
            }
            3 => {
                let mut entry = model[slot].unwrap();
                assert!(
                    lists[entry.owner].detach_overlay(entry.value),
                    "detach: step {step}"
                );
                entry.owner = 1 - entry.owner;
                entry.start = start;
                entry.end = end;
                let _ = entry.value.with_overlay_data_mut(|data| {
                    data.buffer = Some(BufferId(entry.owner as u64 + 1));
                    data.start = start;
                    data.end = end;
                });
                lists[entry.owner].insert_overlay(entry.value);
                model[slot] = Some(entry);
            }
            _ => unreachable!(),
        }
        counts[operation] += 1;
        check_model(&lists, &model, step + 1);
        // Additional text-edit checks exercise the stored front/rear flags;
        // these edits do not replace any of the ten thousand overlay ops.
        if step % 31 == 0 {
            let owner = step % 2;
            let position = (bits >> 24) as usize % 64;
            let before_markers = bits & 4 != 0;
            lists[owner].adjust_for_insert_at_emacs_byte_pos(
                emacs_byte_pos(position),
                emacs_byte_len(1),
                before_markers,
            );
            for entry in model
                .iter_mut()
                .flatten()
                .filter(|entry| entry.owner == owner)
            {
                let empty = entry.start == entry.end;
                if before_markers {
                    if entry.start >= position {
                        entry.start += 1;
                    }
                    if entry.end >= position {
                        entry.end += 1;
                    }
                } else {
                    if entry.start > position
                        || (entry.start == position && entry.front && (!empty || entry.rear))
                    {
                        entry.start += 1;
                    }
                    if entry.end > position || (entry.end == position && entry.rear) {
                        entry.end += 1;
                    }
                }
            }
            check_model(&lists, &model, step + 1);
            lists[owner]
                .adjust_for_delete_emacs_byte_range(emacs_byte_range(position, position + 1));
            for entry in model
                .iter_mut()
                .flatten()
                .filter(|entry| entry.owner == owner)
            {
                if entry.start > position {
                    entry.start -= 1;
                }
                if entry.end > position {
                    entry.end -= 1;
                }
            }
            check_model(&lists, &model, step + 1);
        }
    }
    assert_eq!(counts.iter().sum::<usize>(), OPERATIONS);
    assert!(
        counts.iter().all(|count| *count > 0),
        "operation counts: {counts:?}"
    );
    tracing::info!(seed = SEED, ?counts, "overlay model completed");
}

#[test]
fn overlay_equal_start_forced_order_survives_move_delete_and_refill() {
    crate::test_utils::init_test_tracing();
    let mut list = OverlayList::new();
    let a = alloc_overlay(2, 5);
    let b = alloc_overlay(2, 5);
    let c = alloc_overlay(2, 5);
    for value in [a, b, c] {
        list.insert_overlay(value);
    }
    // Exact checked-in GNU observation already used by the overlay family;
    // do not sort these vectors or derive expected order from the tree.
    assert_eq!(overlays_at(&list, 3), vec![c, b, a]);
    assert!(list.delete_overlay(b));
    assert_eq!(overlays_at(&list, 3), vec![c, a]);
    list.move_overlay_to_emacs_byte_range(c, emacs_byte_range(9, 12));
    assert_eq!(overlays_at(&list, 3), vec![a]);
    assert_eq!(overlays_at(&list, 10), vec![c]);
    list.move_overlay_to_emacs_byte_range(c, emacs_byte_range(2, 5));
    assert_eq!(overlays_at(&list, 3), vec![c, a]);
    assert!(list.delete_overlay(c));
    assert!(list.delete_overlay(a));
    assert!(list.is_empty());
    let replacement = alloc_overlay(2, 5);
    list.insert_overlay(replacement);
    assert_eq!(overlays_at(&list, 3), vec![replacement]);
    list.index.assert_invariants();
}
