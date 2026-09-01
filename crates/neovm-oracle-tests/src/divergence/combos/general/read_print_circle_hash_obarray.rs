//! Divergence tests: read/print circularity + hash + obarray deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_print_circle_shared_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 2 1 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
  (let ((shared (list 1 2 3)))\n\
    (let ((tree (list shared shared)))\n\
      (let ((printed (let ((print-circle t)) (prin1-to-string tree))))\n\
        (list (stringp printed)\n\
              (> (length printed) 0)\n\
              (string-match \"1\" printed)\n\
              (string-match \"#\" printed)\n\
              (eq (car tree) (cadr tree))\n\
              (= (car (car tree)) 1)\n\
              (= (length (car tree)) 3)))))) ",
        expect,
    );
}

#[test]
fn divergence_hash_with_symbol_keys_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function hash-table-keys)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'eq)))
    (mapatoms (lambda (sym)
                (when (and (fboundp sym)
                           (string-match "^car\\|^cdr\\|^cons" (symbol-name sym)))
                  (puthash sym (symbol-name sym) ht))))
    (list (hash-table-count ht)
          (>= (hash-table-count ht) 3)
          (gethash 'car ht)
          (string= (gethash 'car ht) "car")
          (gethash 'cons ht)
          (string= (gethash 'cons ht) "cons")
          (memq 'car (hash-table-keys ht))
          (memq 'cons (hash-table-keys ht))))) "#,
        expect,
    );
}

#[test]
fn divergence_read_print_preserves_circularity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p a)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
  (let ((x (list 'a 'b)))\n\
    (nconc x x)\n\
    (let ((printed (let ((print-circle t) (print-level 5))\n\
                     (prin1-to-string x))))\n\
      (list (stringp printed)\n\
            (string-match \"a\" printed)\n\
            (string-match \"b\" printed)\n\
            (string-match \"#\" printed)\n\
            (> (length printed) 4)\n\
            (= (car x) 'a)\n\
            (= (cadr x) 'b))))) ",
        expect,
    );
}

#[test]
fn divergence_gensym_obarray_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable test-g-1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((g1 (gensym "test-g-"))
        (g2 (gensym "test-g-")))
    (list (not (eq g1 g2))
          (symbolp g1)
          (symbolp g2)
          (null (intern-soft (symbol-name g1)))
          (null (boundp g1))
          (null (fboundp g1))
          (set g1 42)
          (symbol-value g1)
          (= (symbol-value g1) 42)
          (null (symbol-value g2))))) "#,
        expect,
    );
}

#[test]
fn divergence_hash_table_with_equal_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 t duplicate t string-dup t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash '(1 2 3) 'list-value ht)
    (puthash '(1 2 3) 'duplicate ht)
    (puthash "hello" 'string-value ht)
    (puthash "hello" 'string-dup ht)
    (list (hash-table-count ht)
          (= (hash-table-count ht) 2)
          (gethash '(1 2 3) ht)
          (eq (gethash '(1 2 3) ht) 'duplicate)
          (gethash "hello" ht)
          (eq (gethash "hello" ht) 'string-dup)
          (null (gethash '(4 5 6) ht))
          (null (gethash "world" ht))))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_intern_unintern_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (first t t t t second t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (intern "test-cycle-xxx")
  (let ((s1 (intern-soft "test-cycle-xxx")))
    (set s1 'first)
    (let ((v1 (symbol-value s1)))
      (unintern "test-cycle-xxx" obarray)
      (let ((s2 (intern-soft "test-cycle-xxx")))
        (intern "test-cycle-xxx")
        (let ((s3 (intern-soft "test-cycle-xxx")))
          (set s3 'second)
          (list v1
                (eq v1 'first)
                (null s2)
                (symbolp s3)
                (not (eq s1 s3))
                (symbol-value s3)
                (eq (symbol-value s3) 'second))))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_gensym_read_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t 1 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
  (let ((g (gensym)))\n\
    (let ((printed (let ((print-gensym t)) (prin1-to-string g))))\n\
      (list (stringp printed)\n\
            (> (length printed) 0)\n\
            (string-match \":\" printed)\n\
            (symbolp g)\n\
            (not (eq g (intern (symbol-name g)))))))) ",
        expect,
    );
}

#[test]
fn divergence_hash_table_nested_structure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 t 2 t 1 t 2 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((outer (make-hash-table :test 'equal))
        (inner (make-hash-table :test 'equal)))
    (puthash 'x 1 inner)
    (puthash 'y 2 inner)
    (puthash 'inner inner outer)
    (list (gethash 'x (gethash 'inner outer))
          (= (gethash 'x (gethash 'inner outer)) 1)
          (gethash 'y (gethash 'inner outer))
          (= (gethash 'y (gethash 'inner outer)) 2)
          (hash-table-count outer)
          (= (hash-table-count outer) 1)
          (hash-table-count inner)
          (= (hash-table-count inner) 2)
          (hash-table-p (gethash 'inner outer))))) "#,
        expect,
    );
}

#[test]
fn divergence_mapatoms_count_classes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((count 0)
        (names nil))
    (mapatoms (lambda (sym)
                (when (string-match "^buffer-\\(list\\|name\\|size\\)$"
                                    (symbol-name sym))
                  (cl-incf count)
                  (push (symbol-name sym) names))))
    (list count
          (>= count 2)
          (= (length names) count)))) "#,
        expect,
    );
}

#[test]
fn divergence_print_readability_lists_vectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data (list '(1 2 3) [4 5 6] '(a . b) '(nil) '() t nil)))
    (let ((printed (prin1-to-string data))
          (re-read (read (prin1-to-string data))))
      (list (equal data re-read)
            (stringp printed)
            (> (length printed) 10)
            (equal (nth 0 data) '(1 2 3))
            (equal (nth 1 data) [4 5 6])
            (equal (nth 2 data) '(a . b))
            (equal (nth 3 data) '(nil))
            (null (nth 4 data))
            (eq (nth 5 data) t)
            (null (nth 6 data)))))) "#,
        expect,
    );
}
