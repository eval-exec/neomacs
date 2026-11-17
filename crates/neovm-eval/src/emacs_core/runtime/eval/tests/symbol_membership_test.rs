//! PEER 5786 W1: Lisp-visible startup symbols must be obarray members.
//!
//! Neomacs interns names and marks obarray membership separately, so a Rust
//! registry can publish a `Value::symbol` that `intern-soft` cannot find. GNU
//! `intern_c_string` always interns into the current obarray; the boot path
//! must give `features` and every `coding-system-list` entry the same
//! membership. This row mirrors the attested-oracle membership form.

use crate::emacs_core::eval::Context;

#[test]
fn features_and_coding_systems_are_all_obarray_members() {
    let mut context = Context::new(crate::test_utils::unit_runtime());
    let features = context.eval_str("(length features)").expect("features");
    assert!(
        matches!(features.kind(), neovm_value::value::ValueKind::Fixnum(n) if n > 0),
        "features should be a non-empty list"
    );

    let non_member_features = context
        .eval_str(
            "(delq t (mapcar (lambda (sym) (eq sym (intern-soft (symbol-name sym))))
                             features))",
        )
        .expect("feature membership probe");
    assert!(
        non_member_features.is_nil(),
        "features contains non-obarray members: {}",
        crate::emacs_core::print::print_value(&non_member_features)
    );

    let coding_names = context.coding_systems().lisp_visible_names();
    assert!(
        !coding_names.is_empty(),
        "the coding-system registry should not be empty"
    );
    let non_member_coding: Vec<&str> = coding_names
        .iter()
        .copied()
        .filter(|name| {
            let id = neovm_value::intern::intern(name);
            !context.obarray_view().is_global_member(id)
        })
        .collect();
    assert!(
        non_member_coding.is_empty(),
        "coding systems contains non-obarray members: {non_member_coding:?}"
    );

    // The probe must be able to see a non-member if a registry ever regresses.
    let control = context
        .eval_str("(eq (make-symbol \"peer-5786-control\") (intern-soft \"peer-5786-control\"))")
        .expect("uninterned control");
    assert!(control.is_nil(), "make-symbol must stay uninterned");
}
