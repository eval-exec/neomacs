//! Oracle parity tests for GNU `elt`, `aref`, and `aset` edge semantics.
//!
//! GNU routes list/nil `elt` through `nthcdr` in `src/fns.c`, while array
//! access and mutation are implemented by `aref`/`aset` in `src/data.c`.
//! These tests focus on type predicates, index errors, string byte/character
//! rules, bool-vector truth coercion, and char-table indexing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_elt_list_and_array_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (elt '(a b c) 0)
 (elt '(a b c) 3)
 (elt nil 0)
 (condition-case err
     (elt '(a b c) 'bad)
   (error (list (car err) (cdr err))))
 (condition-case err
     (elt 42 0)
   (error (list (car err) (cdr err))))
 (condition-case err
     (elt [a b c] 'bad)
   (error (list (car err) (cdr err))))
 (condition-case err
     (elt [a b c] -1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (elt "abc" 3)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (a nil nil (wrong-type-argument (integerp bad)) (wrong-type-argument (sequencep 42)) (wrong-type-argument (fixnump bad)) (args-out-of-range ([a b c] -1)) (args-out-of-range (\"abc\" 3)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_elt_lambda_is_not_sequence_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU fns.c:Felt accepts cons/nil specially, otherwise CHECK_ARRAY
    // with the sequencep predicate before delegating to Faref.  A lambda
    // closure is not an `elt` sequence, even though GNU Faref handles
    // closures directly for lower-level closure-slot access.
    let form = r#"(elt (lambda (x) x) 0)"#;
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument sequencep (closure (t) (x) x))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_elt_arraylike_acceptance_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:Felt accepts list/nil specially, then CHECK_ARRAY with
    // `sequencep' before delegating to src/data.c:Faref.  That admits
    // bool-vectors and char-tables, but not records or byte-code objects.
    let form = r#"
(let ((bits (make-bool-vector 3 nil))
      (table (make-char-table 'generic 'default)))
  (aset bits 1 t)
  (aset table ?A 'letter-a)
  (list
   (elt bits 0)
   (elt bits 1)
   (elt table ?A)
   (elt table ?B)
   (condition-case err
       (elt (record 'neovm--elt-record 1 2) 0)
     (error (list (car err) (cdr err))))
   (condition-case err
       (elt #[257 "\300\207" [42] 1] 0)
     (error (list (car err) (cdr err))))
   (condition-case err
       (elt bits 3)
     (error (list (car err) (cdr err))))
   (condition-case err
       (elt table #x400000)
     (error (list (car err) (cdr err))))))"#;

    let expect = expect_test::expect![[
        r#""OK (nil t letter-a default (wrong-type-argument (sequencep #s(neovm--elt-record 1 2))) (wrong-type-argument (sequencep #[257 \"��\" [42] 1])) (args-out-of-range (#&3\"\u{2}\" 3)) (wrong-type-argument (characterp 4194304)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_aref_type_index_and_bounds_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (aref '(a b c) 0)
   (error (list (car err) (cdr err))))
 (condition-case err
     (aref [a b c] 'bad)
   (error (list (car err) (cdr err))))
 (condition-case err
     (aref [a b c] -1)
   (error (list (car err) (cdr err))))
 (condition-case err
     (aref [a b c] 3)
   (error (list (car err) (cdr err))))
 (condition-case err
     (aref "abc" 3)
   (error (list (car err) (cdr err))))
 (condition-case err
     (aref (make-bool-vector 2 nil) 2)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (arrayp (a b c))) (wrong-type-argument (fixnump bad)) (args-out-of-range ([a b c] -1)) (args-out-of-range ([a b c] 3)) (args-out-of-range (\"abc\" 3)) (args-out-of-range (#&2\"\\0\" 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_aset_vector_bool_and_record_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((v (vector 'a 'b))
      (bv (make-bool-vector 4 nil))
      (rec (record 'tag 'a 'b)))
  (list
   (aset v 1 'changed)
   v
   (aset bv 0 nil)
   (aset bv 1 0)
   (aset bv 2 "")
   (list (aref bv 0) (aref bv 1) (aref bv 2) (aref bv 3))
   (aset rec 2 'new)
   rec
   (condition-case err
       (aset v -1 'x)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset bv 4 t)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset '(a b) 0 'x)
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 22 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_aset_string_ascii_and_multibyte_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((ascii (copy-sequence "abc"))
      (uni (copy-sequence "aéb"))
      (uni-ascii (copy-sequence "abc")))
  (list
   (aset ascii 1 ?Z)
   ascii
   (aset uni-ascii 1 ?Z)
   uni-ascii
   (condition-case err
       (aset ascii 0 256)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset uni-ascii 0 ?é)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset uni 1 ?E)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset ascii 0 'bad)
     (error (list (car err) (cdr err)))))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 21 43)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_char_table_aref_aset_index_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((ct (make-char-table 'test 'default)))
  (list
   (aref ct ?A)
   (aset ct ?A 'alpha)
   (aref ct ?A)
   (aref ct ?B)
   (condition-case err
       (aref ct -1)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aref ct 4194304)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aref ct 'bad)
     (error (list (car err) (cdr err))))
   (condition-case err
       (aset ct 'bad 'x)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (default alpha alpha default (wrong-type-argument (characterp -1)) (wrong-type-argument (characterp 4194304)) (wrong-type-argument (fixnump bad)) (wrong-type-argument (fixnump bad)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
