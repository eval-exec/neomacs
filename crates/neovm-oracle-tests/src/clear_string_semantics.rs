//! Oracle parity tests for GNU `clear-string` semantics.
//!
//! GNU implements `clear-string` in `src/fns.c`: it destructively removes text
//! properties, zeroes the existing bytes, makes the string unibyte, and sets
//! the character length to the previous byte length.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_clear_string_zeroes_bytes_removes_properties_and_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (propertize (copy-sequence "éx") 'face 'bold))
       (before (list s (length s) (string-bytes s)
                     (multibyte-string-p s)
                     (text-properties-at 0 s)))
       (ret (clear-string s)))
  (list ret
        before
        s
        (length s)
        (string-bytes s)
        (multibyte-string-p s)
        (text-properties-at 0 s)
        (mapcar (lambda (i) (aref s i))
                (number-sequence 0 (1- (length s))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil (\"\\0\\0\\0\" 2 3 t (face bold)) \"\\0\\0\\0\" 3 3 nil nil (0 0 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_clear_string_empty_and_wrong_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (let ((s "")) (list (clear-string s) s (length s) (string-bytes s)))
 (let ((s (string ?é)))
   (clear-string s)
   (list s (length s) (string-bytes s) (multibyte-string-p s)))
 (condition-case err
     (clear-string [1 2 3])
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((nil \"\" 0 0) (\"\\0\\0\" 2 2 nil) (wrong-type-argument (stringp [1 2 3])))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
