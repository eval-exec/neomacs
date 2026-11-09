//! display_when_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display_when::DisplayWhenContextExt;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::value::Value;

#[test]
fn form_sees_object_position_and_buffer_position_bound_dynamically() {
    let mut eval = crate::test_utils::new_context();
    let buffer_id = eval.edit.buffers.current_buffer_id().expect("buffer");
    let buffer = Value::make_buffer(buffer_id);
    let probe = eval
        .eval_str("'(and (bufferp object) (= position 7) (= buffer-position 42))")
        .expect("read");
    assert!(
        eval.display_when_form_holds(probe, buffer, 7, 42)
            .expect("no flow")
    );
    let other = eval.eval_str("'(= position 8)").expect("read");
    assert!(
        !eval
            .display_when_form_holds(other, buffer, 7, 42)
            .expect("no flow")
    );
    // The bindings are undone afterwards (GNU `unbind_to`).
    assert!(eval.eval_str("(boundp 'position)").expect("eval").is_nil());
    assert!(
        eval.eval_str("(boundp 'buffer-position)")
            .expect("eval")
            .is_nil()
    );
}

#[test]
fn sites_are_evaluated_once_per_site_with_the_buffer_current_and_survive_gc() {
    use crate::emacs_core::display_when::{DisplayWhenSite, DisplayWhenSiteKey};
    let mut eval = crate::test_utils::new_context();
    let original = eval.edit.buffers.current_buffer_id().expect("buffer");
    let other = eval.edit.buffers.create_buffer("other");
    let buffer = Value::make_buffer(other);
    // The first form collects; the second must still be intact afterwards.
    let collecting = eval.eval_str("'(progn (garbage-collect) t)").expect("read");
    let named = eval
        .eval_str("'(string-equal (buffer-name) \"other\")")
        .expect("read");
    let property = |form| Value::cons(Value::symbol("when"), Value::cons(form, Value::string("X")));
    let sites = [
        DisplayWhenSite {
            property: property(collecting),
            object: buffer,
            position: 1,
            buffer_position: 1,
        },
        DisplayWhenSite {
            property: property(named),
            object: buffer,
            position: 2,
            buffer_position: 2,
        },
        DisplayWhenSite {
            property: Value::list(vec![Value::symbol("disable-eval"), property(named)]),
            object: buffer,
            position: 3,
            buffer_position: 3,
        },
    ];
    let results = eval
        .evaluate_display_when_sites(other, None, &sites, |spec, _| spec.is_string())
        .expect("no flow");
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(collecting, &sites[0])),
        Some(&true)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(named, &sites[1])),
        Some(&true)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(named, &sites[2])),
        None,
        "a disable-eval site has no answer"
    );
    assert_eq!(results.len(), 2);
    assert_eq!(eval.edit.buffers.current_buffer_id(), Some(original));
}

#[test]
fn one_form_on_two_sites_is_evaluated_at_each_with_its_own_bindings() {
    use crate::emacs_core::display_when::{DisplayWhenSite, DisplayWhenSiteKey};
    let mut eval = crate::test_utils::new_context();
    let buf_id = eval.edit.buffers.current_buffer_id().expect("buffer");
    let buffer = Value::make_buffer(buf_id);
    eval.eval_str("(setq dw-seen nil)").expect("reset");
    let form = eval
        // A bare `Context` has no `push' macro: subrs only.
        .eval_str(
            "'(progn (setq dw-seen (cons (list position buffer-position) dw-seen)) (> buffer-position 30))",
        )
        .expect("read");
    let property = Value::cons(
        Value::symbol("when"),
        Value::cons(form, Value::string("LATE")),
    );
    let sites = [
        DisplayWhenSite {
            property,
            object: buffer,
            position: 1,
            buffer_position: 1,
        },
        DisplayWhenSite {
            property,
            object: buffer,
            position: 36,
            buffer_position: 36,
        },
    ];
    let results = eval
        .evaluate_display_when_sites(buf_id, None, &sites, |spec, _| spec.is_string())
        .expect("no flow");
    assert_eq!(results.len(), 2);
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(form, &sites[0])),
        Some(&false)
    );
    assert_eq!(
        results.get(&DisplayWhenSiteKey::at(form, &sites[1])),
        Some(&true)
    );
    let seen = eval.eval_str("(reverse dw-seen)").expect("seen");
    let expected = eval.eval_str("'((1 1) (36 36))").expect("read");
    assert!(
        eval.eval_str("(equal (reverse dw-seen) '((1 1) (36 36)))")
            .expect("compare")
            .is_truthy(),
        "each site evaluated once with its own bindings: {seen:?} vs {expected:?}"
    );
}
