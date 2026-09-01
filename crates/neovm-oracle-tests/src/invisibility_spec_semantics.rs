//! Oracle parity tests for GNU invisibility-spec helper semantics.
//!
//! GNU implements `add-to-invisibility-spec` and
//! `remove-from-invisibility-spec` in `lisp/subr.el`.  They mutate the
//! buffer-local `buffer-invisibility-spec` using exact `t`/list conversion and
//! `delete` semantics.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_invisibility_spec_helpers_preserve_exact_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((results nil))
  (dolist (initial '(t nil (t) (alpha beta alpha) ((outline . t) t)))
    (with-temp-buffer
      (setq buffer-invisibility-spec (copy-tree initial))
      (let ((add-ret (add-to-invisibility-spec 'alpha))
            (after-add buffer-invisibility-spec)
            (remove-ret (remove-from-invisibility-spec 'alpha))
            (after-remove buffer-invisibility-spec)
            (remove-missing-ret (remove-from-invisibility-spec 'missing))
            (after-remove-missing buffer-invisibility-spec))
        (push (list initial
                    add-ret
                    after-add
                    remove-ret
                    after-remove
                    remove-missing-ret
                    after-remove-missing)
              results))))
  (nreverse results))
"#;

    let expect = expect_test::expect![[
        r#""OK ((t (alpha t) (alpha t) (t) (t) (t) (t)) (nil (alpha) (alpha) nil nil (t) (t)) ((t) (alpha t) (alpha t) (t) (t) (t) (t)) ((alpha beta alpha) (alpha alpha beta) (alpha alpha beta) (beta) (beta) (beta) (beta)) (((outline . t) t) (alpha (outline . t) t) (alpha (outline . t) t) ((outline . t) t) ((outline . t) t) ((outline . t) t) ((outline . t) t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_remove_from_invisibility_spec_converts_non_lists_to_t_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((results nil))
  (dolist (initial '(t nil hidden 42 "hidden"))
    (with-temp-buffer
      (setq buffer-invisibility-spec initial)
      (let ((ret (remove-from-invisibility-spec 'hidden)))
        (push (list initial ret buffer-invisibility-spec) results))))
  (nreverse results))
"#;

    let expect = expect_test::expect![[
        r#""OK ((t (t) (t)) (nil (t) (t)) (hidden (t) (t)) (42 (t) (t)) (\"hidden\" (t) (t)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
