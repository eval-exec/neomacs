use super::*;

/// Ordinary reconstruction needs observed cursor absence, not merely absence
/// of a currently winning candidate; relevant interval writes revoke it.
#[test]
fn ordinary_cursor_proof_observes_original_string_absence() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    for (expression, ordinary) in [
        ("(copy-sequence \"XY\")", true),
        ("(propertize \"XY\" 'face 'bold)", true),
        ("(propertize \"XY\" 'cursor nil)", true),
        ("(propertize \"XY\" 'cursor 1)", false),
        (
            "(propertize \"XY\" 'category 'neo-unproven-category)",
            false,
        ),
    ] {
        let source = context
            .eval_str(&format!("(setq neo-proof-source {expression})"))
            .unwrap();
        let scope = Capture::begin(&context);
        string_properties(
            source,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
        let original = scope.finish().unwrap();
        assert_eq!(
            original.ordinary_cursor_source_proof().is_some(),
            ordinary,
            "{expression}"
        );
        context
            .eval_str("(put-text-property 0 1 'cursor 2 neo-proof-source)")
            .unwrap();
        assert!(original.ordinary_cursor_source_proof().is_none());
    }
}

/// Flattened original authorities have a hard leaf budget, not a growing
/// historical chain, and repeating the same union does not consume more budget.
#[test]
fn original_authority_unions_are_flattened_deduplicated_and_bounded() {
    let context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let mut originals = Vec::new();
    for index in 0..33 {
        let source = Value::string(format!("source-{index}"));
        let scope = Capture::begin(&context);
        string_properties(
            source,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
        originals.push(scope.finish().unwrap());
    }
    let joined =
        CursorSourceDependencies::combine_originals(originals[..32].iter().cloned()).unwrap();
    let repeated = CursorSourceDependencies::combine_originals([joined.clone(), joined]).unwrap();
    let DependencyAuthority::PreparedUnion(parts) = repeated.0.as_ref() else {
        panic!("flat original union required")
    };
    assert_eq!(parts.len(), 32);
    assert!(repeated.unchanged());
    assert!(CursorSourceDependencies::combine_originals(originals).is_none());
}

/// A newly decoded payload is a new original operand even if its source string
/// was already observed. Detaching an observed descendant invalidates before GC.
#[test]
fn decoded_payload_domains_and_ancestor_mutations_keep_original_authority() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let first = context
        .eval_str("(setq neo-original-face (list :height 2.0))")
        .unwrap();
    let string = Value::string("same-source");
    let capture = |context: &Context, face: Value| {
        let scope = Capture::begin(context);
        string_properties(
            string,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
        observe_projection(face, || neovm_core::emacs_core::value::list_to_vec(&face));
        scope.finish().unwrap()
    };
    let original = capture(&context, first);
    context
        .eval_str("(setcar (cdr (list :height 1.0)) 0.5)")
        .unwrap();
    assert!(original.unchanged(), "unrelated payloads do not invalidate");
    let second = context.eval_str("(list :height 0.1)").unwrap();
    let expanded = original
        .after_partial_walk(capture(&context, second))
        .unwrap();
    assert!(
        !expanded.can_replay(),
        "the same string does not cover a new decoded operand"
    );
    context
        .eval_str("(setcdr neo-original-face (list 0.5))")
        .unwrap();
    context.gc_collect_exact();
    assert!(!original.unchanged());
    assert!(
        !expanded.unchanged(),
        "original ancestor writes remain invalid after descendant reclamation"
    );
}

/// Equal text is not object identity: each original string (including distinct
/// interval state) must survive exact GC for its retained read certificate.
#[test]
fn equal_content_sources_keep_distinct_roots() {
    use neovm_core::emacs_core::value::{mutate_string_text_properties, positions::*};
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    context.gc_collect_exact();
    let allocated =
        || unsafe { neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count()) };
    let baseline = allocated();
    let first = Value::string("same");
    let second = Value::string("same");
    mutate_string_text_properties(second, |props| {
        props.put_property_for_object_char_len(
            CharRange::new(CharPos0::new(0), CharPos0::new(1)),
            CharLen::new(4),
            Value::symbol("cursor"),
            Value::fixnum(1),
        );
    });
    assert_ne!(first.bits(), second.bits());
    assert_eq!(
        first, second,
        "Value equality intentionally ignores string properties"
    );
    // Establish the whole live graph, including the interval metadata, without
    // assuming how many managed objects the property representation needs.
    let setup_roots = context.retain_gc_roots([first, second]);
    context.gc_collect_exact();
    let expected = allocated();
    let capture = Capture::begin(&context);
    let _ = string_properties(
        first,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let _ = string_properties(
        second,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let sources = capture.finish().unwrap();
    drop(setup_roots);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        expected,
        "both original strings must remain rooted, not only one equal-content object"
    );
    let DependencyAuthority::Canonical {
        _roots, strings, ..
    } = sources.0.as_ref()
    else {
        panic!("canonical producer capture");
    };
    assert_eq!(_roots.len(), 2);
    assert_eq!(strings.len(), 2);
    drop(sources);
    context.gc_collect_exact();
    assert_eq!(allocated(), baseline);
}

/// A different original object expands a partial source domain even when its
/// text compares equal; the skipped rows' original authority cannot cover it.
#[test]
fn equal_content_new_source_expands_partial_domain() {
    let context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let first = Value::string("same");
    let second = Value::string("same");
    assert_ne!(first.bits(), second.bits());
    assert_eq!(first, second);
    let capture = |value| {
        let scope = Capture::begin(&context);
        let _ = string_properties(
            value,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
        scope.finish().unwrap()
    };
    let original = capture(first);
    let expanded = original.after_partial_walk(capture(second)).unwrap();
    assert!(
        !expanded.can_replay(),
        "equal text does not certify a different original object"
    );
    assert!(expanded.unchanged());
}

/// Repeated projection of one original string needs one immediate root lease,
/// while all original content/interval observations remain freshness evidence.
#[test]
fn repeated_string_projections_share_one_root_lease() {
    let context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let value = Value::string("repeated source");
    let capture = Capture::begin(&context);
    for _ in 0..100 {
        let _ = string_properties(
            value,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
    }
    let sources = capture.finish().unwrap();
    let DependencyAuthority::Canonical {
        _roots, strings, ..
    } = sources.0.as_ref()
    else {
        panic!("canonical producer capture");
    };
    assert_eq!(strings.len(), 1);
    assert_eq!(
        _roots.len(),
        1,
        "do not register a lease per repeated projection"
    );
}

/// Partial walks preserve original read origins without accumulating a chain
/// of removed source strings; new strings after an empty domain are complete.
#[test]
fn partial_source_domains_preserve_original_authority_and_bound_history() {
    for target in ["neo-source-first", "neo-source-second"] {
        let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
        let first = context
            .eval_str("(setq neo-source-first \"first\")")
            .unwrap();
        let second = context
            .eval_str("(setq neo-source-second \"second\")")
            .unwrap();
        let capture = |values: &[Value]| {
            let scope = Capture::begin(&context);
            for value in values {
                let _ = string_properties(
                    *value,
                    &StringSourceRootScope::Rooted(context.owned_root_retainer()),
                );
            }
            scope.finish().unwrap()
        };
        let empty = capture(&[]);
        let original = capture(&[first]);
        let introduced = empty.after_partial_walk(original.clone()).unwrap();
        assert!(introduced.can_replay());
        assert!(Rc::ptr_eq(&introduced.0, &original.0));
        let same = original.after_partial_walk(capture(&[first])).unwrap();
        assert!(
            Rc::ptr_eq(&same.0, &original.0),
            "never rebase skipped original inputs"
        );
        let expanded = original.after_partial_walk(capture(&[second])).unwrap();
        assert!(expanded.unchanged());
        assert!(
            !expanded.can_replay(),
            "reset expanded histories with a full walk"
        );
        assert!(expanded.after_partial_walk(capture(&[first])).is_none());
        context
            .eval_str(&format!("(put-text-property 0 1 'cursor 1 {target})"))
            .unwrap();
        assert!(
            !expanded.unchanged(),
            "mutation of either union arm must invalidate it: {target}"
        );
    }
}

fn allocated_interval_count() -> i64 {
    use neovm_value::tagged::gc::MemoryUseCountSlot;
    Value::memory_use_counts_snapshot()[MemoryUseCountSlot::Intervals.index()]
}

/// Exercise the read used by LispStringSourceFrame, not an owned-copy editing
/// API. Alternating faces prevent adjacent intervals from being normalized away.
fn assert_display_string_property_read_allocates_no_intervals(interval_count: usize) {
    use neovm_core::buffer::CharPos0;
    use neovm_core::emacs_core::value::{
        StringTextPropertyRun, borrow_string_text_properties_table_for_value,
    };
    use neovm_value::value::text_props::text_property_table_clones_for_test;

    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let face = Value::symbol("face");
    let source = Value::string_with_text_properties(
        "x".repeat(interval_count),
        (0..interval_count)
            .map(|index| StringTextPropertyRun {
                start: index,
                end: index + 1,
                plist: Value::list(vec![face, Value::fixnum((index & 1) as i64)]),
            })
            .collect(),
    );
    assert_eq!(
        borrow_string_text_properties_table_for_value(source)
            .expect("propertized display string")
            .interval_plist_runs_for_test()
            .len(),
        interval_count,
        "the generated fixture must retain all distant intervals"
    );

    let before_intervals = allocated_interval_count();
    let before_tables = text_property_table_clones_for_test();
    let snapshot = string_properties(
        source,
        &StringSourceRootScope::Rooted(_context.owned_root_retainer()),
    );
    let properties = snapshot.as_ref().expect("display source properties");
    for position in [0, interval_count / 2, interval_count - 1] {
        assert_eq!(
            properties.get_property_at_char_pos(CharPos0::new(position), face),
            Some(Value::fixnum((position & 1) as i64)),
            "the read still exposes the original property at {position}"
        );
    }
    let allocated_intervals = allocated_interval_count() - before_intervals;
    let table_clones = text_property_table_clones_for_test() - before_tables;
    assert_eq!(
        allocated_intervals, 0,
        "a display-source property read must not copy {interval_count} interval nodes; \
         observed {table_clones} table clones"
    );
}

#[test]
fn display_string_property_read_allocates_no_intervals_2k() {
    assert_display_string_property_read_allocates_no_intervals(2_048);
}

#[test]
fn display_string_property_read_allocates_no_intervals_16k() {
    assert_display_string_property_read_allocates_no_intervals(16_384);
}

#[test]
fn display_string_property_read_allocates_no_intervals_64k() {
    assert_display_string_property_read_allocates_no_intervals(65_536);
}

#[test]
fn propertyless_display_string_read_allocates_no_intervals() {
    let _context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let source = Value::string("plain margin");
    let before = allocated_interval_count();
    assert!(
        string_properties(
            source,
            &StringSourceRootScope::Rooted(_context.owned_root_retainer())
        )
        .is_none()
    );
    assert_eq!(allocated_interval_count() - before, 0);
}

/// Buffer snapshots already share their Rc<TextPropertyTable>. Keep this
/// passing control separate from the owned display-string read regression.
#[test]
fn buffer_text_snapshot_read_allocates_no_intervals() {
    use neovm_core::buffer::{EmacsBytePos, EmacsByteRange};
    use neovm_value::value::text_props::text_property_table_clones_for_test;

    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let id = context
        .buffer_manager()
        .current_buffer()
        .expect("current buffer")
        .id();
    let buffer = context.buffer_manager_mut().get_mut(id).expect("buffer");
    buffer.insert("abc");
    let face = Value::symbol("face");
    assert!(buffer.text_props_put_property_in_emacs_byte_range(
        EmacsByteRange::from_usize(0, 3),
        face,
        Value::T,
    ));
    let before_intervals = allocated_interval_count();
    let before_tables = text_property_table_clones_for_test();
    let snapshot = buffer.text_snapshot();
    assert_eq!(
        snapshot.text_prop_at_emacs_byte_pos(EmacsBytePos::new(1), face),
        Some(Value::T)
    );
    assert_eq!(allocated_interval_count() - before_intervals, 0);
    assert_eq!(text_property_table_clones_for_test() - before_tables, 0);
}

/// Completing the body transfers its roots and original read origin. Later
/// chrome writes can invalidate reuse without erasing the completed record.
#[test]
fn completed_body_seal_keeps_original_sources_rooted_and_detects_later_mutation() {
    use neovm_core::emacs_core::value::{mutate_string_text_properties, positions::*};

    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    context.gc_collect_exact();
    let allocated =
        || unsafe { neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count()) };
    let baseline = allocated();
    let source = Value::string("completed body source");
    let setup_roots = context.retain_gc_roots([source]);
    context.gc_collect_exact();
    let expected = allocated();
    let mut capture = Capture::begin(&context);
    string_properties(
        source,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let completed = capture
        .seal_completed_body()
        .expect("complete unchanged body");
    assert!(completed.unchanged());
    assert!(completed.ordinary_cursor_source_proof().is_some());
    assert!(
        capture.seal_completed_body().is_none(),
        "a body seals only once"
    );
    drop(setup_roots);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        expected,
        "the sealed body owns the original source"
    );
    assert_eq!(
        source.as_lisp_string().unwrap().as_utf8_str(),
        Some("completed body source")
    );
    drop(capture);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        expected,
        "capture teardown must not release the body's roots"
    );
    assert!(
        completed.unchanged(),
        "GC alone preserves the original read certificate"
    );
    mutate_string_text_properties(source, |properties| {
        properties.put_property_for_object_char_len(
            CharRange::new(CharPos0::ZERO, CharPos0::new(1)),
            CharLen::new(21),
            Value::symbol("cursor"),
            Value::fixnum(1),
        );
    });
    context.gc_collect_exact();
    assert!(
        !completed.unchanged(),
        "later chrome cannot refresh the body's read certificate"
    );
    assert!(completed.ordinary_cursor_source_proof().is_none());
    drop(completed);
    context.gc_collect_exact();
    assert_eq!(
        allocated(),
        baseline,
        "retiring the body releases all original roots"
    );
}

#[test]
fn completed_body_seal_rejects_body_mutation_and_finish_remains_strict() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    for seal_body in [true, false] {
        let source = context
            .eval_str("(setq neo-seal-source (copy-sequence \"body\"))")
            .unwrap();
        let mut capture = Capture::begin(&context);
        string_properties(
            source,
            &StringSourceRootScope::Rooted(context.owned_root_retainer()),
        );
        context
            .eval_str("(put-text-property 0 1 'cursor 1 neo-seal-source)")
            .unwrap();
        if seal_body {
            assert!(
                capture.seal_completed_body().is_none(),
                "body mutations remain rejection causes"
            );
        }
        assert!(
            capture.finish().is_none(),
            "strict finish never seals changed observations"
        );
    }
}

#[test]
fn completed_body_seal_restores_nested_capture_without_merging_source_domains() {
    let mut context = neovm_core::emacs_core::eval_hook_installers::new_context();
    let first = context
        .eval_str("(setq neo-seal-outer (copy-sequence \"outer\"))")
        .unwrap();
    let second = context
        .eval_str("(setq neo-seal-inner (copy-sequence \"inner\"))")
        .unwrap();
    let mut outer = Capture::begin(&context);
    string_properties(
        first,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let mut inner = Capture::begin(&context);
    string_properties(
        second,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let inner_sources = inner.seal_completed_body().unwrap();
    context
        .eval_str("(put-text-property 0 1 'cursor 1 neo-seal-inner)")
        .unwrap();
    context.gc_collect_exact();
    assert!(!inner_sources.unchanged());
    drop(inner);
    assert!(
        outer.unchanged(),
        "inner chrome writes do not belong to the outer source domain"
    );
    string_properties(
        first,
        &StringSourceRootScope::Rooted(context.owned_root_retainer()),
    );
    let outer_sources = outer.seal_completed_body().unwrap();
    drop(outer);
    assert!(outer_sources.unchanged());
    context
        .eval_str("(put-text-property 0 1 'cursor 1 neo-seal-outer)")
        .unwrap();
    assert!(
        !outer_sources.unchanged(),
        "the restored outer observation retains its origin"
    );
}
