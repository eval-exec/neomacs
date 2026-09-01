//! Oracle parity tests for symbol property-list identity.
//!
//! GNU implements `symbol-plist`/`setplist` in `src/data.c` and `get`/`put`
//! in `src/fns.c`. Those paths use the exact Lisp symbol object via
//! `XSYMBOL`; an uninterned symbol and an interned symbol with the same print
//! name must never share a plist.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_put_get_keep_uninterned_symbol_plists_separate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((uninterned (make-symbol "neomacs--oracle-plist-id"))
       (interned (intern "neomacs--oracle-plist-id"))
       (prop (make-symbol "neomacs--oracle-prop-id")))
  (unwind-protect
      (progn
        (put uninterned prop 'uninterned-value)
        (put interned prop 'interned-value)
        (put uninterned 'shared 'uninterned-shared)
        (put interned 'shared 'interned-shared)
        (list
         (eq uninterned interned)
         (get uninterned prop)
         (get interned prop)
         (get uninterned 'shared)
         (get interned 'shared)
         (symbol-plist uninterned)
         (symbol-plist interned)))
    (setplist uninterned nil)
    (setplist interned nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil uninterned-value interned-value uninterned-shared interned-shared (neomacs--oracle-prop-id uninterned-value shared uninterned-shared) (neomacs--oracle-prop-id interned-value shared interned-shared))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_setplist_is_verbatim_and_identity_based() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((uninterned (make-symbol "neomacs--oracle-setplist-id"))
       (interned (intern "neomacs--oracle-setplist-id"))
       (prop-a (make-symbol "neomacs--oracle-setplist-prop"))
       (prop-b (copy-sequence "prop")))
  (unwind-protect
      (progn
        (setplist uninterned (list prop-a 1 prop-b 2))
        (setplist interned (list prop-a 3 prop-b 4))
        (list
         (symbol-plist uninterned)
         (symbol-plist interned)
         (get uninterned prop-a)
         (get interned prop-a)
         (get uninterned prop-b)
         (get interned prop-b)
         (get uninterned (copy-sequence "prop"))
         (get interned (copy-sequence "prop"))))
    (setplist uninterned nil)
    (setplist interned nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((neomacs--oracle-setplist-prop 1 \"prop\" 2) (neomacs--oracle-setplist-prop 3 \"prop\" 4) 1 3 2 4 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_get_honors_non_nil_overriding_plist_environment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((sym (make-symbol "neomacs--oracle-override-plist"))
       (prop (make-symbol "neomacs--oracle-override-prop"))
       (overriding-plist-environment
        (list (cons sym (list prop 'override 'nil-prop nil)))))
  (unwind-protect
      (progn
        (put sym prop 'real)
        (put sym 'nil-prop 'real-nil-prop)
        (list
         (get sym prop)
         (get sym 'nil-prop)
         (symbol-plist sym)))
    (setplist sym nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (override real-nil-prop (neomacs--oracle-override-prop real nil-prop real-nil-prop))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_plist_malformed_get_put_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs--oracle-bad-plist")))
  (unwind-protect
      (progn
        (setplist sym '(a 1 b . bad-tail))
        (list
         (get sym 'a)
         (get sym 'b)
         (get sym 'missing)
         (condition-case err
             (put sym 'c 3)
           (error (list (car err) (cadr err))))
         (symbol-plist sym)))
    (setplist sym nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (1 nil nil (wrong-type-argument plistp) (a 1 b . bad-tail))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_plist_nil_and_t_are_mutable() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((nil-old (symbol-plist nil))
      (t-old (symbol-plist t)))
  (unwind-protect
      (let ((nil-plist (list :nil 1))
            (t-plist (list :t 3)))
        (setplist nil nil)
        (setplist t nil)
        (let ((nil-set (setplist nil nil-plist))
              (t-set (setplist t t-plist)))
          (list
           (eq nil-set nil-plist)
           nil-set
           (put nil :nil 2)
           (get nil :nil)
           (symbol-plist nil)
           (eq t-set t-plist)
           t-set
           (put t :t 4)
           (get t :t)
           (symbol-plist t))))
    (setplist nil nil-old)
    (setplist t t-old)))
"#;

    let expect = expect_test::expect![[r#""OK (t (:nil 2) 2 2 (:nil 2) t (:t 4) 4 4 (:t 4))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
