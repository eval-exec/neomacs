//! Oracle parity tests for GNU `subst-char-in-string` branch semantics.
//!
//! GNU implements this in `lisp/subr.el`.  The non-inplace multibyte path uses
//! `string-replace` and then repairs copied string properties, while the
//! inplace path mutates with `aset`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_subst_char_preserves_properties_on_multibyte_copy_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "aéaé")))
  (put-text-property 0 1 'face 'bold s)
  (put-text-property 1 3 'help-echo "mid" s)
  (put-text-property 3 4 'mouse-face 'highlight s)
  (let ((r (subst-char-in-string ?é ?e s)))
    (list r
          s
          (eq r s)
          (substring-no-properties r)
          (text-properties-at 0 r)
          (text-properties-at 1 r)
          (text-properties-at 3 r))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"aeae\" 0 1 (face bold) 1 3 (help-echo \"mid\") 3 4 (mouse-face highlight)) #(\"aéaé\" 0 1 (face bold) 1 3 (help-echo \"mid\") 3 4 (mouse-face highlight)) nil \"aeae\" (face bold) (help-echo \"mid\") (mouse-face highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_subst_char_no_match_multibyte_copy_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "café")))
  (put-text-property 0 4 'face 'bold s)
  (let ((r (subst-char-in-string ?x ?é s)))
    (list r
          s
          (eq r s)
          (equal r s)
          (text-properties-at 1 r))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 0 4 (face bold)) #(\"café\" 0 4 (face bold)) t t (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_subst_char_ascii_copy_path_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "banana")))
  (put-text-property 0 6 'face 'bold s)
  (let ((r (subst-char-in-string ?a ?o s)))
    (list r
          s
          (eq r s)
          (substring-no-properties r)
          (text-properties-at 1 r)
          (text-properties-at 5 r))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"bonono\" 0 6 (face bold)) #(\"banana\" 0 6 (face bold)) nil \"bonono\" (face bold) (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_subst_char_inplace_preserves_object_and_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "aéaé")))
  (put-text-property 0 1 'face 'bold s)
  (put-text-property 1 4 'help-echo "tail" s)
  (let ((r (subst-char-in-string ?é ?e s t)))
    (list r
          s
          (eq r s)
          (substring-no-properties s)
          (text-properties-at 0 s)
          (text-properties-at 1 s)
          (text-properties-at 3 s))))
"#;

    let expect = expect_test::expect![[
        r#""ERR (error \"Attempt to replace non-ASCII char in multibyte string\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_subst_char_unibyte_to_multibyte_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (string-as-unibyte (string 65 200 66 200)))
       (r1 (subst-char-in-string 200 ?é s))
       (r2 (let ((copy (copy-sequence s)))
             (subst-char-in-string 200 ?é copy t))))
  (list
   (multibyte-string-p s)
   (string-bytes s)
   r1
   (multibyte-string-p r1)
   (string-bytes r1)
   r2
   (multibyte-string-p r2)
   (string-bytes r2)))
"#;

    let expect = expect_test::expect![[r#""OK (nil 6 \"AÈBÈ\" nil 6 \"AÈBÈ\" nil 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_subst_char_argument_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((cases
       (list
        (lambda () (subst-char-in-string "a" ?b "abc"))
        (lambda () (subst-char-in-string ?a "b" "abc"))
        (lambda () (subst-char-in-string ?a ?b 123))
        (lambda () (subst-char-in-string #x400000 ?b "abc"))
        (lambda () (subst-char-in-string ?a #x400000 "abc")))))
  (mapcar
   (lambda (fn)
     (condition-case err
         (funcall fn)
       (error (list (car err) (cadr err)))))
   cases))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"abc\" (wrong-type-argument number-or-marker-p) (wrong-type-argument sequencep) \"abc\" (wrong-type-argument characterp))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
