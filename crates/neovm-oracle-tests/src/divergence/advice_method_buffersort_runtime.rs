//! Advice (advice-add :around/:before/:after/:filter-return/:filter-args,
//! add-function, advice-member-p), cl-generic method combination
//! (:around/:before/:after ordering, eql/integer/number specializer chain,
//! cl-next-method-p), and buffer sort/flush (sort-lines/fields/numeric,
//! flush-lines, how-many, tabify/untabify, reverse-region) parity.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn add_function_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 25""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((fn (lambda (x) (* x x))))
  (add-function :filter-args (var fn) (lambda (args) (list (1+ (car args)))))
  (funcall fn 4))"##,
        expect,
    );
}

#[test]
fn advice_before_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((before . 7) (body . 7) (after . 7))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((log nil))
  (defun neo-adv2 (x) (push (cons 'body x) log) x)
  (advice-add 'neo-adv2 :before (lambda (x) (push (cons 'before x) log)))
  (advice-add 'neo-adv2 :after (lambda (x) (push (cons 'after x) log)))
  (neo-adv2 7)
  (nreverse log))"##,
        expect,
    );
}

#[test]
fn advice_combinators() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 110""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(defun neo-adv-base (x) (* x 2))
(advice-add 'neo-adv-base :around (lambda (orig x) (+ 1 (funcall orig x))))
(advice-add 'neo-adv-base :filter-return (lambda (r) (* r 10)))
(prog1 (neo-adv-base 5)
  (advice-remove 'neo-adv-base (lambda (orig x) (+ 1 (funcall orig x)))))"##,
        expect,
    );
}

#[test]
fn advice_member_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#[128 \"���\u{3}#�\" [#[(orig x) ((funcall orig (1+ x))) (t)] #[(x) (x) (t)] :around nil apply] 5 advice] 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(defun neo-adv3 (x) x)
(let ((f (lambda (orig x) (funcall orig (1+ x)))))
  (advice-add 'neo-adv3 :around f)
  (prog1 (list (advice-member-p f 'neo-adv3) (neo-adv3 10))
    (advice-remove 'neo-adv3 f)))"##,
        expect,
    );
}

#[test]
fn method_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (around-pre before primary after around-post)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((log nil))
  (cl-defgeneric neo-mc (x))
  (cl-defmethod neo-mc ((x integer)) (push 'primary log) x)
  (cl-defmethod neo-mc :before ((x integer)) (push 'before log))
  (cl-defmethod neo-mc :after ((x integer)) (push 'after log))
  (cl-defmethod neo-mc :around ((x integer)) (push 'around-pre log) (prog1 (cl-call-next-method) (push 'around-post log)))
  (neo-mc 5)
  (nreverse log))"##,
        expect,
    );
}

#[test]
fn method_next_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((int t) (num nil))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defgeneric neo-np (x))
(cl-defmethod neo-np ((x integer)) (list 'int (cl-next-method-p)))
(cl-defmethod neo-np ((x number)) (list 'num (cl-next-method-p)))
(list (neo-np 5) (neo-np 1.5))"##,
        expect,
    );
}

#[test]
fn method_specializers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK ((zero (integer number)) (integer number) number)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(cl-defgeneric neo-sp (x))
(cl-defmethod neo-sp ((x number)) 'number)
(cl-defmethod neo-sp ((x integer)) (list 'integer (cl-call-next-method)))
(cl-defmethod neo-sp ((x (eql 0))) (list 'zero (cl-call-next-method)))
(list (neo-sp 0) (neo-sp 5) (neo-sp 1.5))"##,
        expect,
    );
}

#[test]
fn flush_keep_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"bar2\\nbaz4\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "foo1\nbar2\nfoo3\nbaz4\nfoo5\n")
  (goto-char (point-min)) (flush-lines "foo")
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn how_many_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "aXbXcXd")
  (goto-char (point-min))
  (list (how-many "X") (count-matches "X" (point-min) (point-max))))"##,
        expect,
    );
}

#[test]
fn reverse_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"4\\n3\\n2\\n1\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "1\n2\n3\n4\n")
  (reverse-region (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn sort_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"x 3\\ny 2\\nz 1\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "z 1\nx 3\ny 2\n")
  (sort-fields 1 (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn sort_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"apple\\nbanana\\ncherry\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "banana\napple\ncherry\n")
  (sort-lines nil (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn sort_numeric_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"b 5\\na 30\\nc 200\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a 30\nb 5\nc 200\n")
  (sort-numeric-fields 2 (point-min) (point-max))
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn tabify_untabify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"\tx\" 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (setq tab-width 8)
  (insert "        x")
  (tabify (point-min) (point-max))
  (let ((tabbed (buffer-string)))
    (untabify (point-min) (point-max))
    (list tabbed (length (buffer-string)))))"##,
        expect,
    );
}
