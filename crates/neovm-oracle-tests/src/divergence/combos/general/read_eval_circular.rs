//! Divergence tests: complex read/print + eval + circular structure combinations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_circular_list_print_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t (a b c a b . #2))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-circle t)
        (print-gensym t)
        (obj (list 'a 'b 'c)))
  (nconc obj obj)
  (let ((printed (prin1-to-string obj))
        (r (read-from-string \"#1=(a b c . #1#)\")))
    (list (string= printed \"#1=(a b c . #1#)\")
          (car (read-from-string printed))))) ",
        expect,
    );
}

#[test]
fn divergence_shared_substructure_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 12 ((1 2 3) (1 2 3) ((1 2 3))))""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((print-circle t)
        (shared (list 1 2 3)))
  (let ((obj (list shared shared (list shared))))
    (let ((printed (prin1-to-string obj)))
      (list (string-match \"#1=\" printed)
            (string-match \"#1#\" printed)
            (car (read-from-string printed)))))) ",
        expect,
    );
}

#[test]
fn divergence_eval_defun_closure_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable captured)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((lexical-binding t))
  (eval '(defun test-clo-fn-xxx (x)
           (let ((captured x))
             (lambda (y) (+ captured y)))))
  (let* ((fn (test-clo-fn-xxx 10))
         (printed (prin1-to-string fn)))
    (list (funcall fn 5)
          (stringp printed)
          (string-match \"closure\" printed)))) ",
        expect,
    );
}

#[test]
fn divergence_record_print_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t cl-struct-tag cl-struct-tag 1 1 (3) (3) 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((r (record 'cl-struct-tag 1 \"two\" (list 3) [4 5])))
  (let* ((printed (prin1-to-string r))
         (r2 (car (read-from-string printed))))
    (list (equal r r2)
          (aref r 0) (aref r2 0)
          (aref r 1) (aref r2 1)
          (aref r 3) (aref r2 3)
          (length r) (length r2)))) ",
        expect,
    );
}

#[test]
fn divergence_print_length_level_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"((... ...) (... ...))\" \"(... ...)\")""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((data '(((a b) (c d)) ((e f) (g h))))
        (print-length 3)
        (print-level 2))
  (list (prin1-to-string data)
        (let ((print-length 1) (print-level 1))
          (prin1-to-string data)))) ",
        expect,
    );
}

#[test]
fn divergence_string_with_special_chars_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((strings (list \"hello\\nworld\"
                            \"tab\\there\"
                            \"quote\\\"inside\"
                            \"back\\\\slash\"
                            \"ctrl\\001char\"))
        (printed (mapcar #'prin1-to-string strings))
        (read-back (mapcar (lambda (s) (car (read-from-string s))) printed)))
  (list (cl-every #'string= strings read-back)
        (length strings)
        (nth 0 read-back)
        (nth 1 read-back)
        (nth 2 read-back))) ",
        expect,
    );
}

#[test]
fn divergence_hash_table_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (32 47 t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((ht (make-hash-table :test 'equal)))
  (puthash \"key1\" '(1 2 3) ht)
  (puthash \"key2\" '(a b c) ht)
  (let ((printed (prin1-to-string ht)))
    (list (string-match \"key1\" printed)
          (string-match \"key2\" printed)
          (hash-table-p (car (read-from-string printed)))))) ",
        expect,
    );
}

#[test]
fn divergence_eval_nested_quote_backquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-variable \\`)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((x 42)
        (items '(a b c)))
  (list (eval '\\`(+ 1 2))
        (eval (list '+ 1 2))
        (eval (list 'quote items))
        (eval (list 'list x (1+ x)))
        (macroexpand-all '\\`(list ,@items)))) ",
        expect,
    );
}

#[test]
fn divergence_print_read_multibyte_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((syms (list 'hello 'world 'test-sym-xxx))
        (printed (prin1-to-string syms))
        (read-back (car (read-from-string printed))))
  (list (equal syms read-back)
        (= (length printed) (1- (length (substring printed 1))))
        (string= (symbol-name (nth 0 syms)) \"hello\"))) ",
        expect,
    );
}

#[test]
fn divergence_format_spec_with_eval() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"Name: Alice, Age: 30\" \"Score: 95.5%\" \"((name . \\\"Alice\\\") (age . 30) (score . 95.5))\" \"(:a 1 :b 2 :c 3)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let ((data '((name . \"Alice\") (age . 30) (score . 95.5))))
  (list (format \"Name: %s, Age: %d\" (cdr (assoc 'name data)) (cdr (assoc 'age data)))
        (format \"Score: %.1f%%\" (cdr (assoc 'score data)))
        (format \"%S\" data)
        (format \"%s\" (plist-put '(:a 1 :b 2) :c 3)))) ",
        expect,
    );
}
