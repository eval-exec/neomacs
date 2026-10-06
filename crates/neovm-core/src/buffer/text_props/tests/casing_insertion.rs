use super::*;

fn controls(nonsticky: Value) -> CasingPropertyControls {
    CasingPropertyControls {
        equality: CasingPropertyEquality::Exact,
        nonsticky,
        aliases: Value::NIL,
        defaults: Value::NIL,
        mutator: std::marker::PhantomData,
    }
}

fn set_run(table: &mut TextPropertyTable, start: usize, end: usize, plist: Vec<(Value, Value)>) {
    table.set_properties_for_object_char_len(
        CharRange::from_usize(start, end),
        CharLen::new(3),
        plist,
    );
}

fn faces(table: &TextPropertyTable, len: usize) -> Vec<Option<Value>> {
    let face = Value::symbol("face");
    (0..len)
        .map(|pos| table.get_property_at_char_pos(CharPos0::new(pos), face))
        .collect()
}

// GNU intervals.c:837-958 distinguishes interior stretching from the sticky
// merger at real (or forced) boundaries. These assertions also guard that the
// ordinary property-free insertion API keeps its existing behavior.
#[test]
fn casing_insertion_preserves_interior_and_resolves_boundary_stickiness() {
    let roots = CasingPropertyRoots::new();
    let face = Value::symbol("face");
    let front = Value::symbol("front-sticky");
    let rear = Value::symbol("rear-nonsticky");
    let bold = Value::symbol("bold");
    let italic = Value::symbol("italic");
    let mut obarray = Obarray::new();
    obarray
        .put_property("gdm-casing-front-category", "front-sticky", Value::T)
        .expect("fixed category property");
    let context = CasingPropertyContext {
        obarray: &obarray,
        controls: controls(Value::NIL),
    };
    let insert =
        |table: &mut TextPropertyTable, pos: usize, context: &CasingPropertyContext<'_>| {
            table
                .adjust_for_casify_insertion(
                    CharPos0::new(pos),
                    CharLen::new(1),
                    CharLen::new(3),
                    context,
                    &roots,
                )
                .unwrap();
            table.assert_tree_invariants_for_test();
        };

    let mut interior = TextPropertyTable::new();
    set_run(&mut interior, 0, 3, vec![(face, bold)]);
    let original_plist = interior.interval_plist_at_char_pos(CharPos0::ZERO).unwrap();
    let mut ordinary = interior.clone();
    insert(&mut interior, 1, &context);
    assert_eq!(faces(&interior, 4), vec![Some(bold); 4]);
    assert!(crate::emacs_core::value::eq_value(
        &interior
            .interval_plist_at_char_pos(CharPos0::new(1))
            .unwrap(),
        &original_plist,
    ));
    ordinary.adjust_for_insert_raw(CharPos0::new(1), CharLen::new(1));
    assert_eq!(
        faces(&ordinary, 4),
        vec![Some(bold), None, Some(bold), Some(bold)]
    );

    let mut boundary = TextPropertyTable::new();
    set_run(&mut boundary, 0, 1, vec![(face, italic)]);
    set_run(&mut boundary, 1, 3, vec![(face, bold)]);
    insert(&mut boundary, 1, &context);
    assert_eq!(
        faces(&boundary, 4),
        vec![Some(italic), Some(italic), Some(bold), Some(bold)]
    );

    let mut equal_boundary = TextPropertyTable::new();
    set_run(&mut equal_boundary, 0, 1, vec![(face, bold)]);
    set_run(&mut equal_boundary, 1, 3, vec![(face, bold)]);
    insert(&mut equal_boundary, 1, &context);
    assert_eq!(
        equal_boundary
            .interval_plist_runs_for_test()
            .iter()
            .map(|(start, end, _)| (*start, *end))
            .collect::<Vec<_>>(),
        vec![(0, 2), (2, 4)],
        "GNU extends the left without removing the existing right boundary"
    );

    let mut beginning = TextPropertyTable::new();
    set_run(&mut beginning, 0, 3, vec![(face, bold)]);
    insert(&mut beginning, 0, &context);
    assert_eq!(
        faces(&beginning, 4),
        vec![None, Some(bold), Some(bold), Some(bold)]
    );

    let mut front_wins = TextPropertyTable::new();
    set_run(
        &mut front_wins,
        0,
        1,
        vec![(face, italic), (rear, Value::list(vec![face]))],
    );
    set_run(
        &mut front_wins,
        1,
        3,
        vec![(face, bold), (front, Value::list(vec![face]))],
    );
    insert(&mut front_wins, 1, &context);
    assert_eq!(
        faces(&front_wins, 4),
        vec![Some(italic), Some(bold), Some(bold), Some(bold)]
    );

    let mut categorized = TextPropertyTable::new();
    let category_key = Value::symbol("category");
    let category = Value::symbol("gdm-casing-front-category");
    set_run(
        &mut categorized,
        1,
        3,
        vec![(category_key, category), (face, bold)],
    );
    insert(&mut categorized, 1, &context);
    assert_eq!(
        faces(&categorized, 4),
        vec![None, Some(bold), Some(bold), Some(bold)]
    );
    assert_eq!(
        categorized.get_property_at_char_pos(CharPos0::new(1), category_key),
        Some(category)
    );
    assert_eq!(
        categorized.get_property_at_char_pos(CharPos0::new(1), front),
        None
    );

    let nonsticky = CasingPropertyContext {
        obarray: &obarray,
        controls: controls(Value::list(vec![Value::cons(face, Value::T)])),
    };
    let mut split = TextPropertyTable::new();
    set_run(&mut split, 0, 3, vec![(face, bold)]);
    insert(&mut split, 1, &nonsticky);
    assert_eq!(
        faces(&split, 4),
        vec![Some(bold), None, Some(bold), Some(bold)]
    );

    let default_front = CasingPropertyContext {
        obarray: &obarray,
        controls: controls(Value::list(vec![Value::cons(face, Value::NIL)])),
    };
    let mut default_start = TextPropertyTable::new();
    set_run(&mut default_start, 0, 3, vec![(face, bold)]);
    let successor_plist = default_start
        .interval_plist_at_char_pos(CharPos0::ZERO)
        .unwrap();
    insert(&mut default_start, 0, &default_front);
    assert_eq!(faces(&default_start, 4), vec![Some(bold); 4]);
    assert!(crate::emacs_core::value::eq_value(
        &default_start
            .interval_plist_at_char_pos(CharPos0::ZERO)
            .unwrap(),
        &successor_plist,
    ));

    let mut forced_boundary = TextPropertyTable::new();
    set_run(&mut forced_boundary, 0, 3, vec![(face, bold)]);
    let original = forced_boundary
        .interval_plist_at_char_pos(CharPos0::ZERO)
        .unwrap();
    insert(&mut forced_boundary, 1, &default_front);
    assert_eq!(forced_boundary.interval_plist_runs_for_test().len(), 2);
    assert!(crate::emacs_core::value::eq_value(
        &forced_boundary
            .interval_plist_at_char_pos(CharPos0::new(1))
            .unwrap(),
        &original,
    ));
    // GNU copy_properties (intervals.c:118-125,895) copies the right
    // plist's cons cells. Its property values are equal, its identity differs.
    let copied = forced_boundary
        .interval_plist_at_char_pos(CharPos0::new(2))
        .unwrap();
    assert_eq!(copied, original);
    assert!(!crate::emacs_core::value::eq_value(&copied, &original));

    let mut front_override = TextPropertyTable::new();
    set_run(
        &mut front_override,
        0,
        3,
        vec![
            (face, bold),
            (front, Value::T),
            (rear, Value::list(vec![face])),
        ],
    );
    insert(&mut front_override, 1, &nonsticky);
    assert_eq!(faces(&front_override, 4), vec![Some(bold); 4]);
    assert_eq!(
        front_override.get_property_at_char_pos(CharPos0::new(1), front),
        Some(Value::T)
    );

    // A detached casing plan invalidates populated syntax caches once. Both
    // boundary and interior growth keep them invalid until the first reader,
    // which reconstructs coherent positions from the final interval tree.
    for (pos, syntax_start) in [(1, 2), (2, 1)] {
        let mut cached = TextPropertyTable::new();
        set_run(&mut cached, 0, 1, vec![(face, italic)]);
        set_run(
            &mut cached,
            1,
            3,
            vec![(Value::symbol("syntax-table"), Value::fixnum(2))],
        );
        assert_eq!(
            cached.syntax_prop_free_run_end(CharPos0::ZERO, CharPos0::new(3)),
            CharPos0::new(1)
        );
        let mut detached = cached.clone();
        detached.invalidate_casify_syntax_caches();
        assert_eq!(detached.syntax_prop_ranges.lock().unwrap().0, 0);
        insert(&mut detached, pos, &context);
        assert_eq!(detached.syntax_prop_ranges.lock().unwrap().0, 0);
        assert_eq!(
            detached.syntax_prop_free_run_end(CharPos0::ZERO, CharPos0::new(4)),
            CharPos0::new(syntax_start)
        );
        assert_eq!(
            detached.syntax_prop_free_run_end(CharPos0::new(syntax_start), CharPos0::new(4)),
            CharPos0::new(4)
        );
        detached.debug_syntax_caches_consistent().unwrap();
        assert_eq!(
            cached.syntax_prop_free_run_end(CharPos0::ZERO, CharPos0::new(3)),
            CharPos0::new(1),
            "preparation does not invalidate the published source table"
        );
    }

    // Policy errors happen while preparing a detached table, before storage
    // and coordinate anchors change. A circular alist must also terminate.
    let mut buffers = crate::buffer::BufferManager::new();
    let buffer_id = buffers.current_buffer_id().unwrap();
    buffers.insert_into_buffer(buffer_id, "aßb").unwrap();
    let buffer = buffers.get(buffer_id).unwrap();
    assert!(
        buffer.text.text_props_put_property_in_emacs_byte_range(
            buffer
                .edit_range_for_char_range(CharRange::from_usize(0, 3))
                .byte_range(),
            face,
            bold,
        )
    );
    let cycle = Value::cons(
        Value::cons(Value::symbol("gdm-absent-key"), Value::T),
        Value::NIL,
    );
    cycle.set_cdr(cycle);
    let invalid = CasingPropertyMode::Inherit(CasingPropertyContext {
        obarray: &obarray,
        controls: controls(cycle),
    });
    let expansion = crate::buffer::CasifyExpansion::new(CharPos0::new(1), CharLen::new(2)).unwrap();
    let old_end = buffer.point_max_anchor();
    assert!(
        buffer
            .text
            .prepare_casify_properties(CharPos0::ZERO, &[expansion], &invalid)
            .is_err()
    );
    assert_eq!(buffer.buffer_string(), "aßb");
    assert_eq!(buffer.point_max_anchor(), old_end);
    assert_eq!(
        buffer
            .text
            .text_props_get_property_at_char_pos(CharPos0::new(1), face),
        Some(bold)
    );
}

// GNU intervals.c:1143-1149 suppresses explicit front metadata only when
// category front-stickiness is EQ to t, using lisp.h:1316-1324 equality.
#[test]
fn category_front_stickiness_respects_positioned_symbol_equality() {
    let mut ctx = crate::emacs_core::eval::Context::new();
    let roots = CasingPropertyRoots::new();
    let positioned_t = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::T, Value::fixnum(77));
    roots.root(positioned_t);
    let face = Value::symbol("face");
    let bold = Value::symbol("bold");
    let category_key = Value::symbol("category");
    let category = Value::symbol("gdm-casing-positioned-front-category");
    let front = Value::symbol("front-sticky");
    for (equality, category_front, expected_boundaries) in [
        (
            CasingPropertyEquality::Exact,
            Value::T,
            vec![(0, 1), (1, 4)],
        ),
        (
            CasingPropertyEquality::PositionedSymbolsTransparent,
            Value::T,
            vec![(0, 1), (1, 4)],
        ),
        (
            CasingPropertyEquality::Exact,
            positioned_t,
            vec![(0, 1), (1, 2), (2, 4)],
        ),
        (
            CasingPropertyEquality::PositionedSymbolsTransparent,
            positioned_t,
            vec![(0, 1), (1, 4)],
        ),
    ] {
        ctx.obarray
            .put_property(
                "gdm-casing-positioned-front-category",
                "front-sticky",
                category_front,
            )
            .unwrap();
        let context = CasingPropertyContext {
            obarray: &ctx.obarray,
            controls: CasingPropertyControls {
                equality,
                ..controls(Value::NIL)
            },
        };
        let mut table = TextPropertyTable::new();
        set_run(
            &mut table,
            1,
            3,
            vec![(category_key, category), (face, bold)],
        );
        table
            .adjust_for_casify_insertion(
                CharPos0::new(1),
                CharLen::new(1),
                CharLen::new(3),
                &context,
                &roots,
            )
            .unwrap();
        table.assert_tree_invariants_for_test();
        assert_eq!(
            faces(&table, 4),
            vec![None, Some(bold), Some(bold), Some(bold)]
        );
        assert_eq!(
            table.get_property_at_char_pos(CharPos0::new(1), category_key),
            Some(category)
        );
        assert_eq!(
            table
                .interval_plist_runs_for_test()
                .into_iter()
                .map(|(start, end, _)| (start, end))
                .collect::<Vec<_>>(),
            expected_boundaries
        );
        let expects_front_metadata = match equality {
            CasingPropertyEquality::Exact => category_front == positioned_t,
            CasingPropertyEquality::PositionedSymbolsTransparent => false,
        };
        if expects_front_metadata {
            let explicit = table
                .get_property_at_char_pos(CharPos0::new(1), front)
                .unwrap();
            assert_eq!(explicit.cons_car(), category_key);
            assert_eq!(explicit.cons_cdr().cons_car(), face);
            assert!(explicit.cons_cdr().cons_cdr().is_nil());
        } else {
            assert_eq!(
                table.get_property_at_char_pos(CharPos0::new(1), front),
                None
            );
        }
    }
}
