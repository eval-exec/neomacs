//! Oracle parity tests for `eval-buffer` and `eval-region`.
//!
//! GNU implements `eval-buffer` and `eval-region` in `src/lread.c`.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, eval_oracle_and_neovm};

#[test]
fn oracle_eval_buffer_returns_nil_for_empty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalbuf*"))
  (erase-buffer)
  (eval-buffer))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_eval_buffer_evaluates_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 42""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalbuf2*"))
  (erase-buffer)
  (insert "(setq neovm--test-evalbuf-result 42)")
  (eval-buffer)
  neovm--test-evalbuf-result)"#,
        expect,
    );
    assert_ok_eq("42", &oracle, &neovm);
}

#[test]
fn oracle_eval_buffer_interns_symbols_in_a_large_read_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's readevalloop reads through `read`, which interns every symbol
    // token into Vobarray as it reads (src/lread.c:4674-4709). Published
    // membership is observable through intern-soft/mapatoms. The generated
    // form also makes the intern pass the caller's data size.
    let form = r#"(let ((n 20000))
  (with-temp-buffer
    (insert "(quote ("
            (mapconcat (lambda (i) (format "zzdeep%d" i))
                       (number-sequence 1 n) " ")
            "))")
    (eval-buffer)
    (list (and (intern-soft "zzdeep1") t)
          (and (intern-soft "zzdeep12345") t)
          (and (intern-soft (format "zzdeep%d" n)) t))))"#;
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_load_interns_a_million_element_read_form_with_bounded_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The reader's obarray-intern pass is walked over the read form. A
    // million-element generated form pins that the walk is bounded by an
    // explicit worklist rather than Rust recursion depth (issue #590).
    let form = r#"(let* ((n 1000000)
                       (f (make-temp-file "zzdeep-load" nil ".el")))
  (unwind-protect
      (progn
        (with-temp-file f
          (insert "(quote ("
                  (mapconcat (lambda (i) (format "zzdeepload%d" i))
                             (number-sequence 1 n) " ")
                  "))"))
        (load f nil t)
        (list (and (intern-soft "zzdeepload1") t)
              (and (intern-soft (format "zzdeepload%d" n)) t)))
    (delete-file f)))"#;
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_read_and_eval_buffer_intern_into_the_current_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's reader interns into the CURRENT value of `obarray`
    // (lread.c:4673-4706), so a read/eval-buffer under a let-bound custom
    // obarray must not leak names into the global one, and the symbol the
    // reader returns must be the object that custom obarray holds.
    let form = r#"(list
 (progn (let ((obarray (obarray-make)))
          (read "peer-fresh-sym-1"))
        (and (intern-soft "peer-fresh-sym-1") t))
 (progn (with-temp-buffer
          (insert "peer-fresh-sym-2")
          (let ((obarray (obarray-make)))
            (condition-case nil (eval-buffer) (error nil))))
        (and (intern-soft "peer-fresh-sym-2") t))
 (let* ((custom (obarray-make))
        (sym (let ((obarray custom)) (read "peer-fresh-sym-3"))))
   (list (and (intern-soft "peer-fresh-sym-3") t)
         (eq sym (intern-soft "peer-fresh-sym-3" custom)))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil (nil t))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_eval_region_returns_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        r#"(progn
  (switch-to-buffer (get-buffer-create "*neovm-test-evalreg*"))
  (erase-buffer)
  (insert "99")
  (eval-region (point-min) (point-max)))"#,
        expect,
    );
    assert_ok_eq("nil", &oracle, &neovm);
}

#[test]
fn oracle_eval_buffer_wrong_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 42)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect("(eval-buffer 42)", expect);
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}
