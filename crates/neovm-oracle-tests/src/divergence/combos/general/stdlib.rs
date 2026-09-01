//! Divergence tests: Elisp stdlib function interaction combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_string_props_with_replace_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"TODO: fix bug #issue-123\" 0 14 (face bold) 21 24 (face bold)) bold bold #(\"123\" 0 3 (face bold)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (let ((s (propertize \"TODO: fix bug #123\" 'face 'bold)))
    (insert s)
    (goto-char 1)
    (re-search-forward \"#\\\\([0-9]+\\\\)\")
    (let ((num (match-string 1))
          (has-face-before (get-text-property 1 'face)))
      (replace-match (format \"#issue-%s\" num) t)
      (list (buffer-string)
            has-face-before
            (get-text-property 1 'face)
            num
            (string= num \"123\"))))) ",
        expect,
    );
}

#[test]
fn deficiency_map_with_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((result nil))
  (mapc (lambda (x) (push (* x x) result)) '(1 2 3 4 5))
  (list (nreverse result)
        (= (length result) 5)
        (= (nth 2 (nreverse result)) 9))) ",
        expect,
    );
}

#[test]
fn deficiency_cl_loop_with_into_and_finally() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (cl-loop for x from 1 to 10
           sum x into total
           count (cl-oddp x) into odds
           finally return (list total odds))
  (cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
           when (cl-evenp x) sum x into evens
           finally return evens)
  (cl-loop for i from 0
           for x in '(a b c d e)
           collect (cons i x))) ",
        expect,
    );
}

#[test]
fn divergence_rx_pcase_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((re (rx bos (group (one-or-more digit)) \".\"
                     (group (one-or-more digit)) \".\"
                     (group (one-or-more digit)) eos)))
  (list re
        (string-match re \"2.1.5\")
        (when (string-match re \"2.1.5\")
          (list (match-string 1) (match-string 2) (match-string 3))))) ",
        expect,
    );
}

#[test]
fn deficiency_pcase_destructuring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (pcase '(1 2 3)
    ((pred listp) (list 'list))
    (_ 'other))
  (pcase '(1 2 3)
    ((\\`(,a ,b ,c) (list a b c)))
  (pcase '(1 2 3)
    ((\\`(,a . ,rest) (list a rest))))
  (pcase 42
    ((pred numberp) 'number)
    ((pred stringp) 'string))
  (pcase \"hello\"
    ((pred numberp) 'number)
    ((pred stringp) 'string))) ",
        expect,
    );
}

#[test]
fn deficiency_thread_first_last() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function thread-first)""#]];
    crate::common::assert_oracle_parity_expect(
        "(list
  (thread-first 5
    (+ 3)
    (* 2)
    (- 1))
  (thread-last '(1 2 3 4 5)
    (mapcar (lambda (x) (* x x)))
    (seq-filter (lambda (x) (> x 10)))
    (seq-reduce #'+ 0))) ",
        expect,
    );
}

#[test]
fn divergence_string_properties_manipulation_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold bold heavy italic italic t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let ((s (copy-sequence \"ABCDEFGHIJ\")))
  (put-text-property 0 5 'face 'bold s)
  (put-text-property 5 10 'face 'italic s)
  (add-text-properties 2 8 '(weight heavy invisible nil) s)
  (list (get-text-property 0 'face s)
        (get-text-property 3 'face s)
        (get-text-property 3 'weight s)
        (get-text-property 7 'face s)
        (get-text-property 9 'face s)
        (remove-text-properties 0 10 '(face nil weight nil) s)
        (get-text-property 3 'face s)
        (get-text-property 3 'weight s))) ",
        expect,
    );
}

#[test]
fn deficiency_seq_group_by() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 ((a 1) (a 3)) ((b 2) (b 5)) ((c 4)) t)""#]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((data '((a 1) (b 2) (a 3) (c 4) (b 5)))
        (grouped (seq-group-by #'car data)))
  (list (length grouped)
        (alist-get 'a grouped)
        (alist-get 'b grouped)
        (alist-get 'c grouped)
        (= (length (alist-get 'a grouped)) 2))) ",
        expect,
    );
}

#[test]
fn deficiency_cl_values_multiple_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn
  (defun test-divmod-xxx (a b)
    (cl-values (floor a b) (mod a b)))
  (multiple-value-bind (q r)
      (test-divmod-xxx 17 5)
    (list q r (= q 3) (= r 2)))) ",
        expect,
    );
}

#[test]
fn divergence_combine_and_eval_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((3 12 7 5) \"(+ 1 2), (* 3 4), (- 10 3), (/ 20 4)\" 27 nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        "(let* ((exprs (list '(+ 1 2) '(* 3 4) '(- 10 3) '(/ 20 4)))
        (results (mapcar #'eval exprs))
        (expr-str (mapconcat (lambda (e) (format \"%S\" e)) exprs \", \"))
        (sum (seq-reduce #'+ results 0)))
  (list results
        expr-str
        sum
        (= sum 21)
        (string= expr-str \"(+ 1 2), (* 3 4), (- 10 3), (/ 20 4)\"))) ",
        expect,
    );
}
