//! Strict combo oracle probes, batch 380: cl-sort/cl-stable-sort deep +
//! cl-merge with :key. cl-sort/stable-sort with key function, cl-merge
//! with :key, and destructuring sort.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_cl_sort_stable_key_test_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((data '((3 . c) (1 . a) (4 . d) (1 . b) (5 . e) (9 . f) (2 . g))))
  (list (cl-sort (copy-sequence data) #'< :key #'car)
        (cl-stable-sort (copy-sequence data) #'< :key #'car)
        (cl-sort (copy-sequence data) #'string< :key #'cdr)
        (cl-stable-sort (copy-sequence data) #'string< :key #'cdr)))
"##;
    let expect = expect_test::expect![[
        r#""OK (((1 . a) (1 . b) (2 . g) (3 . c) (4 . d) (5 . e) (9 . f)) ((1 . a) (1 . b) (2 . g) (3 . c) (4 . d) (5 . e) (9 . f)) ((1 . a) (1 . b) (3 . c) (4 . d) (5 . e) (9 . f) (2 . g)) ((1 . a) (1 . b) (3 . c) (4 . d) (5 . e) (9 . f) (2 . g)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_merge_key_vectors_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(let ((left '((1 . a) (3 . c) (5 . e)))
      (right '((2 . b) (4 . d) (6 . f))))
  (list (cl-merge (copy-sequence left) (copy-sequence right) #'< :key #'car)
        (cl-merge '(1 1 1) '(1 1 1) #'<)
        (cl-merge '("c") '("a" "b") #'string<)))
"##;
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep <)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_cl_sort_vectors_strings_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(require 'cl-lib)
(list (cl-sort (copy-sequence [3 1 4 1 5 9 2 6]) #'<)
      (cl-sort (copy-sequence ["banana" "apple" "cherry"]) #'string<)
      (cl-sort (copy-sequence '(3 1 4 1 5)) #'>)
      (cl-stable-sort (copy-sequence [3 1 4]) #'<))
"##;
    let expect = expect_test::expect![[
        r#""OK ([1 1 2 3 4 5 6 9] [\"apple\" \"banana\" \"cherry\"] (5 4 3 1 1) [1 3 4])""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
