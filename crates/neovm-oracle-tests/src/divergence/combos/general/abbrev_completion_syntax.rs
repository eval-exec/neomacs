//! Divergence tests: abbrev + completion + syntax + text-property + buffer combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_abbrev_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument obarrayp test-ato-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-abbrev-table 'test-ato-xxx nil)
  (define-abbrev 'test-ato-xxx "tst1" "test-one" nil)
  (define-abbrev 'test-ato-xxx "tst2" "test-two" nil)
  (define-abbrev 'test-ato-xxx "tst3" "test-three" nil)
  (list (abbrev-symbol "tst1" 'test-ato-xxx)
        (abbrev-symbol "tst2" 'test-ato-xxx)
        (abbrev-symbol "tst3" 'test-ato-xxx)
        (not (abbrev-symbol "tst4" 'test-ato-xxx))
        (abbrev-expansion "tst1" 'test-ato-xxx)
        (string= (abbrev-expansion "tst1" 'test-ato-xxx) "test-one")
        (abbrev-expansion "tst2" 'test-ato-xxx)
        (string= (abbrev-expansion "tst2" 'test-ato-xxx) "test-two")
        (abbrev-expansion "tst3" 'test-ato-xxx)
        (string= (abbrev-expansion "tst3" 'test-ato-xxx) "test-three"))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_table_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-syntax 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((st (copy-syntax-table (standard-syntax-table))))
    (modify-syntax-entry ?$ "'" st)
    (modify-syntax-entry ?@ "w" st)
    (modify-syntax-entry ?! "_" st)
    (list (char-syntax ?$ st)
          (eq (char-syntax ?$ st) ?')
          (char-syntax ?@ st)
          (eq (char-syntax ?@ st) ?w)
          (char-syntax ?! st)
          (eq (char-syntax ?! st) ?_)
          (syntax-table-p st)
          (standard-syntax-table)
          (syntax-table-p (standard-syntax-table))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_class_with_parse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (0 t nil t nil t 1 t 34 nil 0 t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(defun foo (bar)\n  \"docstring\"\n  (list bar 'baz))")
  (let ((ppss (syntax-ppss 1))
        (ppss2 (syntax-ppss 30))
        (ppss3 (syntax-ppss (point-max))))
    (list (car ppss)
          (= (car ppss) 0)
          (nth 3 ppss)
          (null (nth 3 ppss))
          (nth 8 ppss)
          (null (nth 8 ppss))
          (car ppss2)
          (= (car ppss2) 1)
          (nth 3 ppss2)
          (null (nth 3 ppss2))
          (car ppss3)
          (= (car ppss3) 0)
          (nth 3 ppss3)
          (null (nth 3 ppss3))))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_try_completion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"alp\" t \"alpha\" t \"beta\" t nil t \"\" nil (\"alpha\" \"alphabet\" \"alpine\") t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((coll '("alpha" "alphabet" "alpine" "beta" "gamma")))
    (list (try-completion "al" coll)
          (string= (try-completion "al" coll) "alp")
          (try-completion "alpha" coll)
          (string= (try-completion "alpha" coll) "alpha")
          (try-completion "b" coll)
          (string= (try-completion "b" coll) "beta")
          (try-completion "z" coll)
          (null (try-completion "z" coll))
          (try-completion "" coll)
          (eq (try-completion "" coll) t)
          (all-completions "al" coll)
          (equal (all-completions "al" coll)
                 '("alpha" "alphabet" "alpine"))
          (= (length (all-completions "al" coll)) 3)
          (= (length (all-completions "" coll)) 5)))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_obarray() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t (\"test-co-alpha-xxx\" \"test-co-beta-xxx\" \"test-co-charlie-xxx\") (\"test-co-beta-xxx\" \"test-co-charlie-xxx\") (\"test-co-charlie-xxx\") \"test-co-\" t test-co-alpha-xxx t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (intern "test-co-alpha-xxx")
  (intern "test-co-beta-xxx")
  (intern "test-co-charlie-xxx")
  (let ((matches (all-completions "test-co-" obarray)))
    (list (= (length matches) 3)
          (member "test-co-alpha-xxx" matches)
          (member "test-co-beta-xxx" matches)
          (member "test-co-charlie-xxx" matches)
          (try-completion "test-co-" obarray)
          (string= (try-completion "test-co-" obarray)
                   "test-co-")
          (intern-soft "test-co-alpha-xxx")
          (eq (intern-soft "test-co-alpha-xxx")
              'test-co-alpha-xxx)))) "#,
        expect,
    );
}

#[test]
fn divergence_abbrev_expand_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument obarrayp test-aeb-xxx)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (define-abbrev-table 'test-aeb-xxx nil)
  (define-abbrev 'test-aeb-xxx "hw" "Hello World" nil)
  (insert "hw ")
  (let ((abbrev-mode nil)
        (before (buffer-string)))
    (goto-char 1)
    (let ((expanded (expand-abbrev)))
      (let ((after (buffer-string)))
        (list before
              (string= before "hw ")
              expanded
              (or (null expanded) (stringp expanded))
              after
              (string= after "Hello World ")
              (= (point) 12)))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_forward_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (27 nil t t nil 1 42 41 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "(alpha (beta (gamma delta) epsilon) zeta)")
  (let ((len (buffer-size)))
    (goto-char 14)
    (let ((forward (condition-case err (scan-lists (point) 1 0) (scan-error nil)))
          (backward (condition-case err (scan-lists (point) -1 0) (scan-error nil))))
      (list forward backward
            (or (null forward) (> forward (point)))
            (or (null backward) (<= backward (point)))
            (= len 40)
            (goto-char 1)
            (condition-case err (scan-lists (point) 1 0) (scan-error nil))
            (goto-char 41)
            (condition-case err (scan-lists (point) -1 0) (scan-error nil)))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_properties_with_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((1) t nil t nil t (1) t #(\"AAA.BBB.CCC.DDD\" 3 4 (syntax-table (1)) 7 8 (syntax-table (1)) 11 12 (syntax-table (1))) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA.BBB.CCC.DDD")
  (put-text-property 4 5 'syntax-table '(1))
  (put-text-property 8 9 'syntax-table '(1))
  (put-text-property 12 13 'syntax-table '(1))
  (list (get-text-property 4 'syntax-table)
        (equal (get-text-property 4 'syntax-table) '(1))
        (get-text-property 5 'syntax-table)
        (null (get-text-property 5 'syntax-table))
        (get-text-property 1 'syntax-table)
        (null (get-text-property 1 'syntax-table))
        (get-text-property 8 'syntax-table)
        (equal (get-text-property 8 'syntax-table) '(1))
        (buffer-string)
        (= (buffer-size) 15))) "#,
        expect,
    );
}

#[test]
fn divergence_completion_regexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"ap\" t (\"apple\" \"application\" \"apricot\") t (\"apple\" \"application\" \"apricot\") (\"application\" \"apricot\") (\"apricot\") t t t t (\"banana\") t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((coll '("apple" "application" "apricot" "banana" "cherry")))
    (list (try-completion "ap" coll)
          (string= (try-completion "ap" coll) "ap")
          (all-completions "ap" coll)
          (= (length (all-completions "ap" coll)) 3)
          (member "apple" (all-completions "ap" coll))
          (member "application" (all-completions "ap" coll))
          (member "apricot" (all-completions "ap" coll))
          (test-completion "apple" coll)
          (test-completion "apricot" coll)
          (not (test-completion "grape" coll))
          (= (length (all-completions "" coll)) 5)
          (all-completions "ban" coll)
          (equal (all-completions "ban" coll) '("banana"))))) "#,
        expect,
    );
}

#[test]
fn divergence_syntax_comment_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (nil t nil t t nil 0 t 1 nil 0 t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert ";; This is a comment\n(defvar x 1)\n;; Another comment\n(setq x 2)")
  (let ((ppss1 (syntax-ppss 1))
        (ppss2 (syntax-ppss 24))
        (ppss3 (syntax-ppss 45)))
    (list (nth 4 ppss1)
          (or (null (nth 4 ppss1)) (integerp (nth 4 ppss1)))
          (nth 4 ppss2)
          (null (nth 4 ppss2))
          (nth 4 ppss3)
          (or (null (nth 4 ppss3)) (integerp (nth 4 ppss3)))
          (car ppss1)
          (= (car ppss1) 0)
          (car ppss2)
          (= (car ppss2) 0)
          (car ppss3)
          (= (car ppss3) 0)
          (= (buffer-size) 52)))) "#,
        expect,
    );
}
