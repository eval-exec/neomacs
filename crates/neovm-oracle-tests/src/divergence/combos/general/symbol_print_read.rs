//! Divergence tests: print/read roundtrip + obarray + symbol + intern combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn deficiency_symbol_function_plist_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (result-a t result-b t \"Function A\" t 1 t \"Function B\" t 2 t (doc \"Function A\" version 1) t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-sym-fn-a () 'result-a)
  (defun test-sym-fn-b () 'result-b)
  (put 'test-sym-fn-a 'doc "Function A")
  (put 'test-sym-fn-a 'version 1)
  (put 'test-sym-fn-b 'doc "Function B")
  (put 'test-sym-fn-b 'version 2)
  (list (funcall 'test-sym-fn-a) (eq (funcall 'test-sym-fn-a) 'result-a)
        (funcall 'test-sym-fn-b) (eq (funcall 'test-sym-fn-b) 'result-b)
        (get 'test-sym-fn-a 'doc) (string= (get 'test-sym-fn-a 'doc) "Function A")
        (get 'test-sym-fn-a 'version) (= (get 'test-sym-fn-a 'version) 1)
        (get 'test-sym-fn-b 'doc) (string= (get 'test-sym-fn-b 'doc) "Function B")
        (get 'test-sym-fn-b 'version) (= (get 'test-sym-fn-b 'version) 2)
        (symbol-plist 'test-sym-fn-a) (listp (symbol-plist 'test-sym-fn-a))
        (fboundp 'test-sym-fn-a)
        (fboundp 'test-sym-fn-b))) "#,
        expect,
    );
}

#[test]
fn deficiency_intern_soft_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (hello t world t foo t nil t hello t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((my-obarray (make-vector 13 0)))
    (intern "hello" my-obarray)
    (intern "world" my-obarray)
    (intern "foo" my-obarray)
    (list (intern-soft "hello" my-obarray) (symbolp (intern-soft "hello" my-obarray))
          (intern-soft "world" my-obarray) (symbolp (intern-soft "world" my-obarray))
          (intern-soft "foo" my-obarray) (symbolp (intern-soft "foo" my-obarray))
          (intern-soft "bar" my-obarray) (null (intern-soft "bar" my-obarray))
          (intern "hello" my-obarray) (eq (intern "hello" my-obarray)
                                          (intern-soft "hello" my-obarray))))) "#,
        expect,
    );
}

#[test]
fn deficiency_mapatoms_symbol_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t (test-mapatoms-alpha test-mapatoms-delta test-mapatoms-gamma) (test-mapatoms-delta test-mapatoms-gamma))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((syms nil))
    (mapatoms (lambda (s) (when (string-match "^test-mapatoms-" (symbol-name s))
                       (push s syms))))
    (intern "test-mapatoms-alpha")
    (intern "test-mapatoms-beta")
    (intern "test-mapatoms-gamma")
    (intern "test-mapatoms-delta")
    (let ((syms2 nil))
      (mapatoms (lambda (s) (when (string-match "^test-mapatoms-" (symbol-name s))
                         (push s syms2))))
      (list (= (length syms) 0)
            (= (length syms2) 4)
            (member (intern-soft "test-mapatoms-alpha") syms2)
            (member (intern-soft "test-mapatoms-delta") syms2))))) "#,
        expect,
    );
}

#[test]
fn deficiency_prin1_to_string_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let* ((data '(1 "hello" (a b c) [1 2 3] t nil (lambda (x) x)))
         (printed (prin1-to-string data))
         (re-read (read printed)))
    (list printed (stringp printed)
          (equal re-read data)
          (equal (car re-read) '(1 "hello" (a b c) [1 2 3] t nil))
          (length printed) (> (length printed) 10)
          (equal (read (prin1-to-string '(a . b))) '(a . b))
          (equal (read (prin1-to-string [1 2 3])) [1 2 3]
          (equal (read (prin1-to-string "hello")) "hello")))) "#,
        expect,
    );
}

#[test]
fn deficiency_obarray_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t sym-0 t sym-19 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ob (make-vector 7 0)))
    (dotimes (i 20)
      (intern (format "sym-%d" i) ob))
    (let ((count 0))
      (mapatoms (lambda (_s) (setq count (+ count 1))) ob)
      (list (= count 20)
            (intern-soft "sym-0" ob) (symbolp (intern-soft "sym-0" ob))
            (intern-soft "sym-19" ob) (symbolp (intern-soft "sym-19" ob))
            (intern-soft "sym-20" ob) (null (intern-soft "sym-20" ob)))))) "#,
        expect,
    );
}

#[test]
fn deficiency_symbol_name_equal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"test-sym-eq\" t \"test-sym-eq\" t nil t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((s1 (make-symbol "test-sym-eq"))
        (s2 (make-symbol "test-sym-eq")))
    (list (symbol-name s1) (string= (symbol-name s1) "test-sym-eq")
          (symbol-name s2) (string= (symbol-name s2) "test-sym-eq")
          (eq s1 s2) (null (eq s1 s2))
          (equal (symbol-name s1) (symbol-name s2))
          (not (eq s1 s2))))) "#,
        expect,
    );
}

#[test]
fn deficiency_keyword_symbol() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t \":hello\" t \"hello\" t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((kw :hello)
        (sym 'hello))
    (list (keywordp kw)
          (null (keywordp sym))
          (symbolp kw)
          (symbol-name kw) (string= (symbol-name kw) ":hello")
          (symbol-name sym) (string= (symbol-name sym) "hello")
          (eq :hello :hello)
          (null (eq :hello 'hello))))) "#,
        expect,
    );
}

#[test]
fn deficiency_read_string_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((42 . 2) t ((a b c) . 7) t (\"hello\" . 7) t (nil . 3) t (t . 1) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (read-from-string "42")
        (equal (read-from-string "42") '(42 . 2))
        (read-from-string "(a b c)")
        (equal (read-from-string "(a b c)") '((a b c) . 7))
        (read-from-string "\"hello\"")
        (equal (read-from-string "\"hello\"") '("hello" . 7))
        (read-from-string "nil")
        (equal (read-from-string "nil") '(nil . 3))
        (read-from-string "t")
        (equal (read-from-string "t") '(t . 1)))) "#,
        expect,
    );
}

#[test]
fn deficiency_format_encoded_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"(1 2 3 4 5 6 7 8 9 10)\" t \"(\\\"a\\\" \\\"bb\\\" \\\"ccc\\\" \\\"dddd\\\")\" t \"(alpha beta gamma delta)\" t \"1 + 2 = 3\" t 22 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((nums '(1 2 3 4 5 6 7 8 9 10))
        (strs '("a" "bb" "ccc" "dddd"))
        (syms '(alpha beta gamma delta)))
    (list (format "%S" nums) (string= (format "%S" nums) "(1 2 3 4 5 6 7 8 9 10)")
          (format "%S" strs) (string= (format "%S" strs) "(\"a\" \"bb\" \"ccc\" \"dddd\")")
          (format "%S" syms) (string= (format "%S" syms) "(alpha beta gamma delta)")
          (format "%d + %d = %d" 1 2 3) (string= (format "%d + %d = %d" 1 2 3) "1 + 2 = 3")
          (length (format "%S" nums)) (> (length (format "%S" nums)) 10)))) "#,
        expect,
    );
}

#[test]
fn deficiency_symbol_properties_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (1 t 2 t 3 t 4 t 5 t (a 1 b 2 c 3 d 4 e 5) t 1 t 5 t (c 3 d 4 e 5) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((sym (intern "test-prop-chain")))
    (put sym 'a 1)
    (put sym 'b 2)
    (put sym 'c 3)
    (put sym 'd 4)
    (put sym 'e 5)
    (let ((plist (symbol-plist sym)))
      (list (get sym 'a) (= (get sym 'a) 1)
            (get sym 'b) (= (get sym 'b) 2)
            (get sym 'c) (= (get sym 'c) 3)
            (get sym 'd) (= (get sym 'd) 4)
            (get sym 'e) (= (get sym 'e) 5)
            plist (listp plist)
            (plist-get plist 'a) (= (plist-get plist 'a) 1)
            (plist-get plist 'e) (= (plist-get plist 'e) 5)
            (plist-member plist 'c) (consp (plist-member plist 'c)))))) "#,
        expect,
    );
}
