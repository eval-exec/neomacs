use super::*;

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
