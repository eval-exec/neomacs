//! Oracle parity tests for GNU symbol-with-position semantics.
//!
//! GNU implements `bare-symbol-p`, `symbol-with-pos-p`, `bare-symbol`,
//! `symbol-with-pos-pos`, `remove-pos-from-symbol`, and `position-symbol`
//! in `src/data.c`.  Reader integration is in `src/lread.c`, which wraps
//! symbols when `symbols-with-pos-enabled` is non-nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_position_symbol_basic_accessors_and_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((sp (position-symbol 'neomacs--oracle-sympos 42))
       (sp2 (position-symbol sp 77))
       (sp3 (position-symbol 'other sp)))
  (list
   (symbol-with-pos-p sp)
   (bare-symbol-p sp)
   (bare-symbol-p 'neomacs--oracle-sympos)
   (bare-symbol sp)
   (symbol-with-pos-pos sp)
   (bare-symbol sp2)
   (symbol-with-pos-pos sp2)
   (bare-symbol sp3)
   (symbol-with-pos-pos sp3)
   (remove-pos-from-symbol sp)
   (remove-pos-from-symbol "not-a-symbol")))
"#;

    let expect = expect_test::expect![[
        r#""OK (t nil t neomacs--oracle-sympos 42 neomacs--oracle-sympos 77 other 42 neomacs--oracle-sympos \"not-a-symbol\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_position_symbol_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (bare-symbol 1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (symbol-with-pos-pos 'plain)
   (error (list (car err) (cdr err))))
 (condition-case err
     (position-symbol 1 2)
   (error (list (car err) (cdr err))))
 (condition-case err
     (position-symbol 'ok "bad-pos")
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument ((symbolp symbol-with-pos-p) 1)) (wrong-type-argument (symbol-with-pos-p plain)) (wrong-type-argument ((symbolp symbol-with-pos-p) 1)) (wrong-type-argument (fixnum-or-symbol-with-pos-p \"bad-pos\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_position_symbol_accepts_negative_fixnum_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((neg (position-symbol 'neomacs--oracle-negative-pos -1))
       (copied (position-symbol neg (position-symbol 'other 9))))
  (list
   (bare-symbol neg)
   (symbol-with-pos-pos neg)
   (bare-symbol copied)
   (symbol-with-pos-pos copied)))
"#;

    let expect = expect_test::expect![[
        r#""OK (neomacs--oracle-negative-pos -1 neomacs--oracle-negative-pos 9)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_symbol_with_pos_enabled_controls_symbolp_and_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((sp-a (position-symbol 'neomacs--oracle-sympos-eq 10))
      (sp-b (position-symbol 'neomacs--oracle-sympos-eq 20)))
  (list
   (let ((symbols-with-pos-enabled nil))
     (list (symbolp sp-a)
           (eq sp-a 'neomacs--oracle-sympos-eq)
           (eq sp-a sp-b)))
   (let ((symbols-with-pos-enabled t))
     (list (symbolp sp-a)
           (eq sp-a 'neomacs--oracle-sympos-eq)
           (eq sp-a sp-b)
           (eq (bare-symbol sp-a) (bare-symbol sp-b)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 13 58)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_symbols_with_positions_records_source_offsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((symbols-with-pos-enabled t))
  (let ((form (read "(alpha beta (gamma . delta))")))
    (list
     (mapcar #'symbol-with-pos-p form)
     (mapcar #'bare-symbol form)
     (mapcar #'symbol-with-pos-pos form)
     (symbol-with-pos-p (car (nth 2 form)))
     (bare-symbol (car (nth 2 form)))
     (symbol-with-pos-pos (car (nth 2 form)))
     (symbol-with-pos-p (cdr (nth 2 form)))
     (bare-symbol (cdr (nth 2 form)))
     (symbol-with-pos-pos (cdr (nth 2 form)))))))
"#;

    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument (symbolp symbol-with-pos-p) (gamma . delta))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
