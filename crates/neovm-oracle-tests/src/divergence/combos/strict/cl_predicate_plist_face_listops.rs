//! Strict combo oracle probes, batch 26: cl-every/some/notany/notevery/reduce/
//! subsetp, plist-member/lax-plist-put, add-face-text-property + font-lock-face,
//! list tail ops (butlast/nthcdr/last/nbutlast/safe-length), and cl-find/
//! position-if/count/mismatch.
//!
//! Tests are parity locks unless annotated with a surfaced divergence.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_g1_cl_every_some_reduce() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-every #'numberp '(1 2 3))
      (cl-every #'numberp '(1 a 3))
      (cl-some #'oddp '(2 4 5))
      (cl-notevery #'numberp '(1 a))
      (cl-notany #'oddp '(2 4))
      (cl-reduce #'+ '(1 2 3 4))
      (cl-subsetp '(1 2) '(1 2 3)))
"##,
        expect,
    );
}

#[test]
fn div_g1_plist_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((:b 2) (a 1 b 2 c 3) (:a 1 :b 2) (:a 99 :b 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (plist-member '(:a 1 :b 2) :b)
      (lax-plist-put (copy-tree '(a 1 b 2)) 'c 3)
      (plist-put (copy-tree '(:a 1)) :b 2)
      (plist-put (copy-tree '(:a 1 :b 2)) :a 99))
"##,
        expect,
    );
}

#[test]
fn div_g1_add_face_text_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold bold italic (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (add-face-text-property 2 5 'bold)
  (add-text-properties 1 2 '(font-lock-face italic))
  (list (get-text-property 2 'face)
        (get-text-property 4 'face)
        (get-text-property 1 'font-lock-face)
        (text-properties-at 2)))
"##,
        expect,
    );
}

#[test]
fn div_g1_list_tail_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3) (3 4) (2 3) (1 2 3) 1000)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (butlast '(1 2 3 4))
      (nthcdr 2 '(1 2 3 4))
      (last '(1 2 3) 2)
      (nbutlast (list 1 2 3 4))
      (safe-length (make-list 1000 'x)))
"##,
        expect,
    );
}

#[test]
fn div_g1_cl_find_count_mismatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-find)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-find 3 '(1 2 3 4))
      (cl-position-if #'oddp '(2 4 5 6))
      (cl-find-if #'atom '((1) (2) 3))
      (cl-count 2 '(1 2 2 3 2))
      (cl-mismatch "abc" "abd" :end 2)
      (cl-mismatch "abc" "abd"))
"##,
        expect,
    );
}

#[test]
fn div_g1_assq_delq_member_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((a . 1) (c . 3)) (a c a) (a c a) (b c) (b c) (2 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (assq-delete-all 'b '((a . 1) (b . 2) (c . 3)))
      (delq 'b '(a b c b a))
      (remq 'b '(a b c b a))
      (member 'b '(a b c))
      (memq 'b '(a b c))
      (memql 2 '(1 2 3)))
"##,
        expect,
    );
}
