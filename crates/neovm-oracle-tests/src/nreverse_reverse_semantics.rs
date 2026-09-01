//! Oracle parity tests for GNU `nreverse`/`reverse` edge semantics.
//!
//! GNU implements these in `src/fns.c`: `nreverse` destructively reverses
//! lists, vectors, and bool-vectors, but returns `reverse` for strings; `reverse`
//! returns fresh sequence storage and uses a different non-sequence error
//! predicate from `nreverse`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_nreverse_mutates_list_spine_and_vector_storage() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((list (list 'a 'b 'c))
       (first-cons list)
       (reversed-list (nreverse list))
       (vec (vector 1 2 3 4))
       (same-vec (nreverse vec)))
  (list reversed-list
        (eq (car (last reversed-list)) first-cons)
        first-cons
        vec
        same-vec
        (eq vec same-vec)))
"#;

    let expect = expect_test::expect![[r#""OK ((c b a) nil (a) [4 3 2 1] [4 3 2 1] t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_nreverse_bool_vector_mutates_and_string_does_not() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((bv (make-bool-vector 6 nil))
       (_ (aset bv 0 t))
       (_ (aset bv 2 t))
       (_ (aset bv 5 t))
       (same-bv (nreverse bv))
       (s (copy-sequence "abcd"))
       (rs (nreverse s)))
  (list (eq bv same-bv)
        (mapcar (lambda (i) (aref bv i)) '(0 1 2 3 4 5))
        s
        rs
        (eq s rs)))
"#;

    let expect = expect_test::expect![[r#""OK (t (t nil nil t nil t) \"abcd\" \"dcba\" nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_is_shallow_and_does_not_mutate_inputs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((shared (list 'cell))
       (list (list 'a shared 'c))
       (rev-list (reverse list))
       (vec (vector shared 2 3))
       (rev-vec (reverse vec)))
  (aset rev-vec 0 'changed)
  (list list
        rev-list
        (eq (cadr rev-list) shared)
        vec
        rev-vec
        (eq vec rev-vec)
        (eq (aref vec 0) (aref rev-vec 2))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((a (cell) c) (c (cell) a) t [(cell) 2 3] [changed 2 (cell)] nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_string_properties_and_multibyte_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((s (copy-sequence (concat "a" (char-to-string #x4e2d) "b")))
       (_ (put-text-property 0 2 'face 'bold s))
       (r (reverse s)))
  (list s
        r
        (substring-no-properties r)
        (multibyte-string-p r)
        (text-properties-at 0 r)
        (text-properties-at 1 r)
        (text-properties-at 2 r)))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"a中b\" 0 2 (face bold)) \"b中a\" \"b中a\" t nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_and_nreverse_error_payloads() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(list
 (condition-case err
     (reverse 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (nreverse 42)
   (error (list (car err) (cdr err))))
 (condition-case err
     (reverse '(a b . c))
   (error (list (car err) (cdr err))))
 (condition-case err
     (nreverse '(a b . c))
   (error (list (car err) (cdr err)))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((wrong-type-argument (sequencep 42)) (wrong-type-argument (arrayp 42)) (wrong-type-argument (listp c)) (wrong-type-argument (listp (a))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_reverse_and_nreverse_circular_list_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((r (list 'a 'b 'c))
       (_ (setcdr (last r) r))
       (n (list 'x 'y 'z))
       (_ (setcdr (last n) n)))
  (list
   (condition-case err
       (reverse r)
     (error (let ((arg (cadr err)))
              (list (car err)
                    (length (cdr err))
                    (eq arg r)
                    (listp arg)
                    (car arg)))))
   (condition-case err
       (nreverse n)
     (error (let ((arg (cadr err)))
              (list (car err)
                    (length (cdr err))
                    (eq arg n)
                    (listp arg)
                    (car arg)))))))
"#;

    let expect =
        expect_test::expect![[r#""OK ((circular-list 1 nil t c) (circular-list 1 t t x))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
