//! Divergence tests: print/read + eval + obarray deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_eval_read_from_string_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((a b c) (d e f) (g h i))""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((input "(a b c) (d e f) (g h i)")
        (pos 0)
        (results nil))
  (while (< pos (length input))
    (let ((pair (read-from-string input pos)))
      (push (car pair) results)
      (setq pos (cdr pair))))
  (nreverse results)) "#,
        expect,
    );
}

#[test]
fn divergence_intern_soft_after_unintern_obarray_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t 42 t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((sym (intern "test-intern-cycle-xxx" obarray))
        (present1 (intern-soft "test-intern-cycle-xxx" obarray)))
  (set sym 42)
  (let ((val1 (symbol-value sym)))
    (unintern "test-intern-cycle-xxx" obarray)
    (let ((present2 (intern-soft "test-intern-cycle-xxx" obarray)))
      (intern "test-intern-cycle-xxx" obarray)
      (let ((present3 (intern-soft "test-intern-cycle-xxx" obarray)))
        (list (eq sym present1)
              val1
              (null present2)
              (not (eq sym present3))
              (boundp present3)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_mapatoms_collect_and_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (intern "test-ma-1-xxx" obarray)
  (intern "test-ma-2-xxx" obarray)
  (intern "test-ma-3-xxx" obarray)
  (let ((collected nil))
    (mapatoms (lambda (s)
                (when (string-prefix-p "test-ma-" (symbol-name s))
                  (push s collected))))
    (list (length collected)
          (= (length collected) 3)
          (cl-every #'symbolp collected)
          (member (intern "test-ma-2-xxx" obarray) collected))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_read_backquote_form() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((form '(a (b c) d))
        (printed (prin1-to-string form))
        (read-back (car (read-from-string printed))))
  (list (equal form read-back)
        (string= printed "(a (b c) d)")
        (= (length read-back) 3)
        (equal (cadr read-back) '(b c)))) "#,
        expect,
    );
}

#[test]
fn divergence_eval_lambda_then_funcall() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (25 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let* ((fn (eval '(lambda (x y) (+ (* x x) (* y y)))))
        (result (funcall fn 3 4)))
  (list result
        (= result 25)
        (compiled-function-p fn)
        (functionp fn))) "#,
        expect,
    );
}

#[test]
fn divergence_read_with_standard_input() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((1 2 3) (4 5 6) \"hello\" 65 t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(with-temp-buffer
  (insert "(1 2 3) (4 5 6) \"hello\" ?A")
  (goto-char 1)
  (let ((a (read (current-buffer)))
        (b (read (current-buffer)))
        (c (read (current-buffer)))
        (d (read (current-buffer))))
    (list a b c d
          (equal a '(1 2 3))
          (equal b '(4 5 6))
          (string= c "hello")
          (= d 65)))) "#,
        expect,
    );
}

#[test]
fn divergence_obarray_hash_collision_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((syms (mapcar (lambda (i) (intern (format "test-hash-%04d-xxx" i) obarray))
                          (number-sequence 0 99))))
  (list (length syms)
        (= (length syms) 100)
        (cl-every #'symbolp syms)
        (= (length (cl-remove-duplicates syms)) 100)
        (eq (nth 0 syms) (intern-soft "test-hash-0000-xxx" obarray)))) "#,
        expect,
    );
}

#[test]
fn divergence_print_circle_shared_substructure() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function match-strings-all)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-circle t)
        (shared (list 1 2 3)))
  (let ((obj (list shared (list shared) shared)))
    (let ((printed (prin1-to-string obj)))
      (list (string-match \"#1=\" printed)
            (string-match \"#1#\" printed)
            (>= (length (match-strings-all printed)) 0)
            (stringp printed))))) ",
        expect,
    );
}

#[test]
fn divergence_eval_nested_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function \\,)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((x 42)
        (items '(a b c)))
  (list (eval `\`(+ ,x ,@(mapcar #'1+ '(1 2 3))))
        (macroexpand-all '\`(list ,x ,@items))
        (eval '\`(list ,x ,@items))
        (equal (eval '\`(list ,x ,@items)) '(42 a b c)))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_plist_obarray_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 4 8 8 nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(let ((sym (intern "test-plist-ob-xxx" obarray)))
  (setplist sym '(a 1 b 2 c 3))
  (let ((p1 (symbol-plist sym)))
    (put sym 'd 4)
    (let ((p2 (symbol-plist sym)))
      (list (get sym 'a) (get sym 'b) (get sym 'c) (get sym 'd)
            (length p1) (length p2)
            (> (length p2) (length p1))
            (eq sym (intern-soft "test-plist-ob-xxx" obarray)))))) "#,
        expect,
    );
}
