//! Oracle parity tests for GNU symbol property semantics.
//!
//! GNU implements `symbol-plist` and `setplist` in `src/data.c`, while `get`
//! and `put` are in `src/fns.c`.  These tests cover the symbol-specific layer
//! on top of plist handling, including `overriding-plist-environment`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_symbol_plist_returns_live_property_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs-oracle-live-plist")))
  (setplist sym (list 'a 1 'b 2))
  (let ((plist (symbol-plist sym)))
    (setcar (cdr plist) 11)
    (setcdr (cddr plist) (list 'c 3))
    (list
     (get sym 'a)
     (get sym 'b)
     (get sym 'c)
     (eq plist (symbol-plist sym))
     (symbol-plist sym))))
"#;

    let expect = expect_test::expect![[r#""OK (11 c nil t (a 11 b c 3))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_setplist_accepts_malformed_plist_and_put_validates_when_needed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs-oracle-malformed-plist")))
  (setplist sym '(a 1 b . bad-tail))
  (list
   (get sym 'a)
   (get sym 'b)
   (get sym 'missing)
   (put sym 'b 22)
   (get sym 'b)
   (condition-case err
       (put sym 'c 3)
     (error (list (car err) (cdr err))))
   (symbol-plist sym)))
"#;

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument plistp (a 1 b . bad-tail))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_get_uses_overriding_plist_environment_only_for_non_nil_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sym (make-symbol "neomacs-oracle-override")))
  (put sym 'a 1)
  (put sym 'b 2)
  (put sym 'c nil)
  (let ((overriding-plist-environment (list (list sym 'a 10 'b nil 'c 30))))
    (list
     (get sym 'a)
     (get sym 'b)
     (get sym 'c)
     (get sym 'missing)
     (symbol-plist sym))))
"#;

    let expect = expect_test::expect![[r#""OK (10 2 30 nil (a 1 b 2 c nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_property_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (symbol-plist "not-symbol")
   (error (list (car err) (cdr err))))
 (condition-case err
     (setplist 42 nil)
   (error (list (car err) (cdr err))))
 (condition-case err
     (get '(not . symbol) 'a)
   (error (list (car err) (cdr err))))
 (condition-case err
     (put nil 'a 1)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (symbolp \"not-symbol\")) (wrong-type-argument (symbolp 42)) (wrong-type-argument (symbolp (not . symbol))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_define_symbol_prop_updates_load_list_and_symbol_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((current-load-list nil))
  (unwind-protect
      (progn
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-a
          'neomacs-prop-one "first")
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-a
          'neomacs-prop-one "updated")
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-b
          'neomacs-prop-one "second")
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-a
          'neomacs-prop-two "third")
        (list
         current-load-list
         (get 'neomacs-oracle-define-symbol-prop-a 'neomacs-prop-one)
         (get 'neomacs-oracle-define-symbol-prop-b 'neomacs-prop-one)
         (get 'neomacs-oracle-define-symbol-prop-a 'neomacs-prop-two)))
    (setplist 'neomacs-oracle-define-symbol-prop-a nil)
    (setplist 'neomacs-oracle-define-symbol-prop-b nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((define-symbol-props (neomacs-prop-two neomacs-oracle-define-symbol-prop-a) (neomacs-prop-one neomacs-oracle-define-symbol-prop-b neomacs-oracle-define-symbol-prop-a))) \"updated\" \"second\" \"third\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_define_symbol_prop_preserves_existing_load_list_entries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((current-load-list
       '((define-symbol-props
          (neomacs-prop-one neomacs-oracle-define-symbol-prop-existing))
         (defun . neomacs-oracle-define-symbol-prop-function)
         neomacs-oracle-define-symbol-prop-variable)))
  (unwind-protect
      (progn
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-existing
          'neomacs-prop-one "existing")
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-new
          'neomacs-prop-one "new")
        (define-symbol-prop 'neomacs-oracle-define-symbol-prop-new
          'neomacs-prop-two "other")
        (list
         current-load-list
         (get 'neomacs-oracle-define-symbol-prop-existing 'neomacs-prop-one)
         (get 'neomacs-oracle-define-symbol-prop-new 'neomacs-prop-one)
         (get 'neomacs-oracle-define-symbol-prop-new 'neomacs-prop-two)))
    (setplist 'neomacs-oracle-define-symbol-prop-existing nil)
    (setplist 'neomacs-oracle-define-symbol-prop-new nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((define-symbol-props (neomacs-prop-two neomacs-oracle-define-symbol-prop-new) (neomacs-prop-one neomacs-oracle-define-symbol-prop-new neomacs-oracle-define-symbol-prop-existing)) (defun . neomacs-oracle-define-symbol-prop-function) neomacs-oracle-define-symbol-prop-variable) \"existing\" \"new\" \"other\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
