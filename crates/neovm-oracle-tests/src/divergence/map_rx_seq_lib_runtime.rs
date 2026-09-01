//! Standard-library parity: map.el (elt/keys/values/merge/filter/apply),
//! rx (build + match + pcase rx pattern), seq advanced (group-by/difference/
//! intersection/mapn/into/seq-let), ring, generator (iter-defun), cl-print,
//! bindat pack/unpack, and pcase map pattern.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn map_elt_put() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 9 default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'map)
(let ((al '((a . 1) (b . 2))) (pl '(:x 1 :y 2)) (h (make-hash-table :test 'eq)))
  (puthash 'k 9 h)
  (list (map-elt al 'a) (map-elt pl :y) (map-elt h 'k) (map-elt al 'z 'default)))"##,
        expect,
    );
}

#[test]
fn map_filter_apply() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((a . 1)) ((1 . a) (2 . b)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'map)
(list (map-filter (lambda (k _v) (eq k 'a)) '((a . 1) (b . 2)))
      (sort (map-apply (lambda (k v) (cons v k)) '((a . 1) (b . 2))) (lambda (x y) (< (car x) (car y)))))"##,
        expect,
    );
}

#[test]
fn map_merge_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b) (1 2) ((a . 1) (b . 2)) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'map)
(list (map-keys '((a . 1) (b . 2))) (map-values '((a . 1) (b . 2)))
      (map-merge 'list '((a . 1)) '((b . 2))) (map-length '((a . 1) (b . 2))))"##,
        expect,
    );
}

#[test]
fn rx_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"[[:digit:]]+\" \"^\\\\([a-z]\\\\)$\" \"\\\\(?:bar\\\\|foo\\\\)\" \"\\\\(?:ab\\\\)\\\\{3\\\\}\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'rx)
(list (rx (+ digit)) (rx bol (group (any "a-z")) eol)
      (rx (or "foo" "bar")) (rx (= 3 "ab")))"##,
        expect,
    );
}

#[test]
fn rx_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 0 \"a_b_c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'rx)
(list (string-match (rx (+ digit)) "abc123") (string-match (rx word-boundary "the" word-boundary) "the cat")
      (replace-regexp-in-string (rx (+ space)) "_" "a  b   c"))"##,
        expect,
    );
}

#[test]
fn rx_pcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"2024\" \"01\" \"15\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'rx)
(pcase "2024-01-15"
  ((rx (let y (= 4 digit)) "-" (let m (= 2 digit)) "-" (let d (= 2 digit)))
   (list y m d)))"##,
        expect,
    );
}

#[test]
fn seq_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(list (seq-group-by #'cl-evenp '(1 2 3 4 5 6))
      (seq-difference '(1 2 3 4) '(2 4))
      (seq-intersection '(1 2 3) '(2 3 4))
      (seq-mapn #'+ '(1 2 3) '(10 20 30)))"##,
        expect,
    );
}

#[test]
fn seq_into_concat() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ([1 2 3] (1 2 3) (97 98 99) (1 2 3 4) [3 2 1])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(list (seq-into '(1 2 3) 'vector) (seq-into [1 2 3] 'list)
      (seq-into "abc" 'list) (seq-concatenate 'list '(1 2) '(3 4))
      (seq-reverse [1 2 3]))"##,
        expect,
    );
}

#[test]
fn seq_let_destructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 (3 4 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(seq-let (a b &rest c) '(1 2 3 4 5) (list a b c))"##,
        expect,
    );
}

#[test]
fn seq_more() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'seq)
(list (seq-position '(a b c) 'b) (seq-find #'cl-evenp '(1 3 4 5))
      (seq-count #'cl-evenp '(1 2 3 4)) (seq-sort-by #'- #'< '(1 2 3))
      (seq-min '(3 1 2)) (seq-max '(3 1 2)))"##,
        expect,
    );
}

#[test]
fn bindat_pack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 5) (258 5))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'bindat)
(let ((spec (bindat-type (n uint 16) (m uint 8))))
  (let ((packed (bindat-pack spec '((n . 258) (m . 5)))))
    (list (append (string-to-unibyte packed) nil)
          (let ((u (bindat-unpack spec packed))) (list (bindat-get-field u 'n) (bindat-get-field u 'm))))))"##,
        expect,
    );
}

#[test]
fn cl_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"(1 2 3)\" \"\\\"str\\\"\" \"[1 2 3]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-print)
(let ((print-circle t))
  (list (cl-prin1-to-string '(1 2 3)) (cl-prin1-to-string "str")
        (cl-prin1-to-string [1 2 3])))"##,
        expect,
    );
}

#[test]
fn generator_iter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 1 4 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'generator)
(iter-defun neo-range (n) (dotimes (i n) (iter-yield (* i i))))
(let ((acc nil) (it (neo-range 4)))
  (condition-case nil (while t (push (iter-next it) acc)) (iter-end-of-sequence nil))
  (nreverse acc))"##,
        expect,
    );
}

#[test]
fn pcase_rx_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"x\" 30)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'map)
(pcase '((name . "x") (age . 30))
  ((map name age) (list name age)))"##,
        expect,
    );
}

#[test]
fn ring_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 d c (d c b))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'ring)
(let ((r (make-ring 3)))
  (ring-insert r 'a) (ring-insert r 'b) (ring-insert r 'c) (ring-insert r 'd)
  (list (ring-length r) (ring-ref r 0) (ring-ref r 1) (ring-elements r)))"##,
        expect,
    );
}
