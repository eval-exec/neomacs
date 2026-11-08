use super::*;

/// Equal text is not object identity: each original string (including distinct
/// interval state) must survive exact GC for its retained read certificate.
#[test]
fn equal_content_sources_keep_distinct_roots() {
    use neovm_core::emacs_core::value::{mutate_string_text_properties, positions::*};
    let mut context = Context::new();
    context.gc_collect_exact();
    let allocated = || neovm_core::tagged::gc::with_tagged_heap(|heap| heap.allocated_count());
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
    let _ = string_properties(first);
    let _ = string_properties(second);
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
    let context = Context::new();
    let first = Value::string("same");
    let second = Value::string("same");
    assert_ne!(first.bits(), second.bits());
    assert_eq!(first, second);
    let capture = |value| {
        let scope = Capture::begin(&context);
        let _ = string_properties(value);
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
    let context = Context::new();
    let value = Value::string("repeated source");
    let capture = Capture::begin(&context);
    for _ in 0..100 {
        let _ = string_properties(value);
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
        let mut context = Context::new();
        let first = context
            .eval_str("(setq neo-source-first \"first\")")
            .unwrap();
        let second = context
            .eval_str("(setq neo-source-second \"second\")")
            .unwrap();
        let capture = |values: &[Value]| {
            let scope = Capture::begin(&context);
            for value in values {
                let _ = string_properties(*value);
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
