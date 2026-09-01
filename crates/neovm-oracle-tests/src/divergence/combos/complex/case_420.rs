//! Complex combo batch 420 — 20 probes into remaining niche areas:
//! make-byte-code, copy-tree circular, define-hash-table-test,
//! cl-destructuring-bind, cl-loop across/ref, rx minimal-match,
//! pcase cl-struct, cl-subst/sublis, key-description/single-key,
//! string-to-sequence, make-record deep, merge/merge-vector,
//! cl-position/find/count, cl-sort/stable-sort, concat/vconcat
//! edge cases, char-to-string edge, and seq-to-string.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// make-byte-code: constructing byte-code function objects.
#[test]
fn div_cx420_make_byte_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bc (make-byte-code 1 "\300\207" [nil] 0)))
  (byte-code-function-p bc))
"##,
        expect,
    );
}

/// copy-tree with circular list / alist structures.
#[test]
fn div_cx420_copy_tree_circular() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t a b)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((lst '(a b))
       (copy (copy-tree lst)))
  (list (equal lst copy)
        (car copy)
        (cadr copy)))
"##,
        expect,
    );
}

/// define-hash-table-test: custom hash function.
#[test]
fn div_cx420_define_hash_table_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx420-test 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(define-hash-table-test 'neo-cx420-test #'equal
  (lambda (x) (secure-hash 'sha256 (prin1-to-string x))))
(let ((ht (make-hash-table :test 'neo-cx420-test)))
  (puthash "hello" 1 ht)
  (puthash "world" 2 ht)
  (list (hash-table-test ht)
        (gethash "hello" ht)))
"##,
        expect,
    );
}

/// cl-destructuring-bind with nested patterns.
#[test]
fn div_cx420_cl_destructuring_bind_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-destructuring-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-destructuring-bind ((a &rest b) c d &key e) '((1 2 3) 4 5 :e 6)
  (list a b c d e))
"##,
        expect,
    );
}

/// cl-loop with for ... across and for ... in-ref.
#[test]
fn div_cx420_cl_loop_across_in_ref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((vec [10 20 30 40])
      (lst '(1 2 3 4)))
  (list (cl-loop for v across vec collect v)
        (cl-loop for v in lst collect (* v 2))))
"##,
        expect,
    );
}

/// rx with minimal-match / maximal-match.
#[test]
fn div_cx420_rx_minimal_maximal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 \"aab\" 0 \"aabb\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-match (rx (minimal-match (0+ any)) "b") "aabb")
      (match-string 0 "aabb")
      (string-match (rx (maximal-match (0+ any)) "b") "aabb")
      (match-string 0 "aabb"))
"##,
        expect,
    );
}

/// pcase with cl-struct pattern.
#[test]
fn div_cx420_pcase_cl_struct() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx420-point x y)
  (let ((p (make-neo-cx420-point :x 10 :y 20)))
    (pcase p
      ((cl-struct neo-cx420-point x y) (list x y))
      (_ nil))))
"##,
        expect,
    );
}

/// cl-subst / cl-sublis: tree substitution.
#[test]
fn div_cx420_cl_subst_sublis() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-subst)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-subst 'new 'old '(old a b (old c)))
      (cl-sublis '((old . new) (a . b)) '(old a b (old c))))
"##,
        expect,
    );
}

/// key-description / single-key-description.
#[test]
fn div_cx420_key_description_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C-c C-f\" \"a\" \"C-a\" \"^A\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (key-description (kbd "C-c C-f"))
      (single-key-description ?a)
      (single-key-description ?\C-a)
      (text-char-description ?\C-a))
"##,
        expect,
    );
}

/// string-to-sequence / seq-to-string.
#[test]
fn div_cx420_string_to_sequence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function string-to-sequence)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-sequence "abc" 'list)
      (string-to-sequence "abc" 'vector)
      (seq-to-string '(?a ?b ?c))
      (concat '(?a ?b ?c)))
"##,
        expect,
    );
}

/// make-record / record extended features.
#[test]
fn div_cx420_make_record_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t neo-cx420-type 4 neo-cx420-type a c)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((r (record 'neo-cx420-type 'a 'b 'c)))
  (list (recordp r)
        (type-of r)
        (length r)
        (aref r 0)
        (aref r 1)
        (aref r 3)))
"##,
        expect,
    );
}

/// merge / merge-vector: merging sorted sequences.
#[test]
fn div_cx420_merge_sequences() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function merge)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'cl-lib)
  (list (merge 'list '(1 3 5) '(2 4 6) #'<)
        (merge 'vector [1 3] [2 4] #'<)))
"##,
        expect,
    );
}

/// cl-position / cl-find / cl-count with keyword args.
#[test]
fn div_cx420_cl_position_find_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '(a b c b a)))
  (list (cl-position 'b lst)
        (cl-position 'b lst :from-end t)
        (cl-find 'c lst)
        (cl-count 'a lst)))
"##,
        expect,
    );
}

/// cl-sort / cl-stable-sort with different preds.
#[test]
fn div_cx420_cl_sort_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-sort)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lst '(3 1 4 1 5 9 2 6)))
  (list (cl-sort (copy-sequence lst) #'<)
        (cl-stable-sort (copy-sequence '(3 1 4 1 5)) #'<)))
"##,
        expect,
    );
}

/// concat / vconcat with mixed argument types.
#[test]
fn div_cx420_concat_vconcat_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp a)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (concat '(a b) [c d] "ef")
      (vconcat '(1 2) [3 4] "56"))
"##,
        expect,
    );
}

/// char-to-string with multibyte and char-byte values.
#[test]
fn div_cx420_char_to_string_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"é\" \"世\" \"😀\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-to-string ?a)
      (char-to-string ?é)
      (char-to-string ?世)
      (char-to-string #x1F600))
"##,
        expect,
    );
}

/// append / nconc with varying list lengths.
#[test]
fn div_cx420_append_nconc_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) (a b c) (1 2 3 4))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append '(1 2) '(3 4) '(5 6))
      (append '(a) nil '(b) nil '(c))
      (nconc (list 1 2) (list 3 4)))
"##,
        expect,
    );
}

/// number-to-string with various numeric types.
#[test]
fn div_cx420_number_to_string_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"0\" \"-42\" \"3.14159\" \"2305843009213693951\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (number-to-string 0)
      (number-to-string -42)
      (number-to-string 3.14159)
      (number-to-string most-positive-fixnum))
"##,
        expect,
    );
}

/// string-to-number with edge inputs.
#[test]
fn div_cx420_string_to_number_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 42 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "")
      (string-to-number "  42  ")
      (string-to-number "abc")
      (string-to-number "0x1A"))
"##,
        expect,
    );
}
