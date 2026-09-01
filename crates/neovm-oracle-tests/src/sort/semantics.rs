//! Oracle parity tests for GNU `sort` keyword and mutation semantics.
//!
//! GNU implements `sort` in `src/fns.c`.  Old-style `(sort SEQ LESSP)` sorts
//! in place, but keyword invocation defaults to a sorted copy unless
//! `:in-place` is non-nil.  Equal keys retain input order.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_sort_old_style_list_is_destructive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((lst (list 3 1 2))
       (first lst)
       (sorted (sort lst #'<)))
  (list
   sorted
   lst
   first
   (eq sorted first)
   (memq first sorted)))
"#;

    let expect = expect_test::expect![[r#""OK ((1 2 3) (1 2 3) (1 2 3) t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_keyword_default_copies_list_and_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((lst (list 3 1 2))
       (vec (vector 3 1 2))
       (sorted-list (sort lst :lessp #'<))
       (sorted-vec (sort vec :lessp #'<)))
  (list
   sorted-list
   lst
   (eq sorted-list lst)
   sorted-vec
   vec
   (eq sorted-vec vec)))
"#;

    let expect = expect_test::expect![[r#""OK ((1 2 3) (3 1 2) nil [1 2 3] [3 1 2] nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_keyword_in_place_and_reverse_stability() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((vec (vector '(1 . a) '(2 . b) '(1 . c) '(2 . d) '(1 . e)))
       (sorted-vec (sort vec :key #'car :lessp #'< :in-place t))
       (rev-list (sort (list '(1 . a) '(2 . b) '(1 . c) '(2 . d) '(1 . e))
                       :key #'car :lessp #'< :reverse t)))
  (list
   sorted-vec
   (eq sorted-vec vec)
   rev-list))
"#;

    let expect = expect_test::expect![[
        r#""OK ([(1 . a) (1 . c) (1 . e) (2 . b) (2 . d)] t ((2 . b) (2 . d) (1 . a) (1 . c) (1 . e)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_default_value_lessp_and_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (sort nil :lessp #'<)
 (sort [3 1 2])
 (sort '(3 1 2))
 (sort '("b" "a" "c")))
"#;

    let expect = expect_test::expect![[r#""OK (nil [1 2 3] (1 2 3) (\"a\" \"b\" \"c\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_keyword_errors_and_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (sort '(3 1 2) :lessp)
   (error (list (car err) (cdr err))))
 (condition-case err
     (sort '(3 1 2) :unknown #'<)
   (error (list (car err) (cdr err))))
 (condition-case err
     (sort "abc" :lessp #'<)
   (error (list (car err) (cdr err))))
 (condition-case err
     (sort '(3 1 2) :lessp 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (sort '(3 1 2) :key 42 :lessp #'<)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((void-function (:lessp)) (error (\"Invalid keyword argument\" :unknown)) (wrong-type-argument (list-or-vector-p \"abc\")) (invalid-function (42)) (invalid-function (42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_rejects_bool_vector_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/fns.c:Fsort only accepts nil, cons lists, and vectors.
    // Bool-vectors are sequences for some primitives, but `sort` rejects them
    // before invoking the comparator/key machinery.
    let form = r#"
(list
 (condition-case err
     (sort (bool-vector t nil t) #'<)
   (error (list (car err) (cdr err))))
 (condition-case err
     (sort (bool-vector t nil t) :lessp #'<)
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (list-or-vector-p #&3\"\u{5}\")) (wrong-type-argument (list-or-vector-p #&3\"\u{5}\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_sort_validates_list_before_key_or_lessp_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:sort_list calls list_length before extracting elements or
    // invoking tim_sort, so malformed/circular lists signal before :key or
    // :lessp can run.
    let form = r#"
(list
 (let ((key-calls nil)
       (lessp-calls nil))
   (list
    (condition-case err
        (sort '(3 1 . tail)
              :key (lambda (x) (push x key-calls) x)
              :lessp (lambda (a b) (push (list a b) lessp-calls) (< a b)))
      (error (list (car err) (cdr err))))
    (nreverse key-calls)
    (nreverse lessp-calls)))
 (let ((key-calls nil)
       (lessp-calls nil)
       (cycle (list 3 1)))
   (setcdr (last cycle) cycle)
   (list
    (condition-case err
        (sort cycle
              :key (lambda (x) (push x key-calls) x)
              :lessp (lambda (a b) (push (list a b) lessp-calls) (< a b)))
      (error (list (car err) (cdr err))))
    (nreverse key-calls)
    (nreverse lessp-calls))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((wrong-type-argument (listp tail)) nil nil) ((circular-list ((3 1 3 1 . #2))) nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
