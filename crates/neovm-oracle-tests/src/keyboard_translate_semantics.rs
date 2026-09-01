//! Oracle parity tests for GNU `subr.el` `keyboard-translate`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;
use crate::common::{assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_keyboard_translate_creates_table_and_sets_character_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((keyboard-translate-table nil))
   (list (keyboard-translate ?a ?b)
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?a)
         (aref keyboard-translate-table ?b)
         (char-table-range keyboard-translate-table ?a)
         (char-table-range keyboard-translate-table ?b)))
 (let ((keyboard-translate-table 'stale))
   (list (keyboard-translate ?x ?y)
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?x)
         (condition-case e
             (keyboard-translate "x" ?z)
           (error (list (car e) (cadr e)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((98 t keyboard-translate-table 98 nil 98 nil) (121 t keyboard-translate-table 121 (wrong-type-argument fixnump)))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        "((98 t keyboard-translate-table 98 nil 98 nil) (121 t keyboard-translate-table 121 (wrong-type-argument fixnump)))",
        &oracle,
        &neovm,
    );
}

#[test]
fn oracle_key_translate_parses_keys_mutates_table_and_reports_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((keyboard-translate-table nil))
   (list (key-translate "a" "b")
         (char-table-p keyboard-translate-table)
         (char-table-subtype keyboard-translate-table)
         (aref keyboard-translate-table ?a)
         (aref keyboard-translate-table ?b)))
 (let ((keyboard-translate-table nil))
   (key-translate "a" nil)
   (list (char-table-p keyboard-translate-table)
         (aref keyboard-translate-table ?a)))
 (condition-case e
     (key-translate "" "a")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate "a b" "c")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate "a" "b c")
   (error (list (car e) (cadr e))))
 (condition-case e
     (key-translate 42 "a")
   (error (list (car e) (cadr e) (caddr e)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((98 t keyboard-translate-table 98 nil) (t nil) (error \"\\\"\\\" is not a valid key definition; see ‘key-valid-p’\") (error \"FROM key a b is not a single key\") (error \"TO key b c is not a single key\") (error \"42 is not a valid key definition; see ‘key-valid-p’\" nil))""#
    ]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq(
        r#"((98 t keyboard-translate-table 98 nil) (t nil) (error "\"\" is not a valid key definition; see ‘key-valid-p’") (error "FROM key a b is not a single key") (error "TO key b c is not a single key") (error "42 is not a valid key definition; see ‘key-valid-p’" nil))"#,
        &oracle,
        &neovm,
    );
}
