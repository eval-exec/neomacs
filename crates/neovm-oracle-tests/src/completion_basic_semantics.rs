//! Oracle parity tests for GNU completion table primitives.
//!
//! `try-completion` and `all-completions` are implemented in GNU C
//! (`src/minibuf.c`), while metadata and boundary adapters live in
//! `lisp/minibuffer.el`.  These tests cover list/alist/hash/function table
//! behavior without replacing GNU's matching rules with local expectations.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_prop_try_completion_list_and_alist_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((completion-ignore-case nil)
      (completion-regexp-list nil))
  (list
   (try-completion "fo" '("foo" "foobar" "frob" "bar"))
   (try-completion "foo" '("foo" "foobar" "frob" "bar"))
   (try-completion "foo" '("foo" "bar"))
   (try-completion "z" '("foo" "bar"))
   (try-completion "al" '((alpha . 1) ("alpine" . 2) ("beta" . 3)))
   (try-completion "alp" '((alpha . 1) ("alpine" . 2) ("beta" . 3)))
   (try-completion "alpha" '((alpha . 1) ("alpine" . 2) ("beta" . 3)))))
"#;

    let expect = expect_test::expect![[r#""OK (\"foo\" \"foo\" t nil \"alp\" \"alp\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_all_completions_predicate_and_regexp_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((completion-ignore-case nil)
      (collection '(("apple" . fruit)
                    ("apricot" . fruit)
                    ("ape" . animal)
                    ("banana" . fruit)
                    ("application" . software))))
  (list
   (sort (copy-sequence
          (all-completions "ap" collection
                           (lambda (entry) (eq (cdr entry) 'fruit))))
         #'string<)
   (let ((completion-regexp-list '("e\\'")))
     (sort (copy-sequence (all-completions "ap" collection)) #'string<))
   (let ((completion-regexp-list '("i")))
     (sort (copy-sequence (all-completions "ap" collection)) #'string<))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"apple\" \"apricot\") (\"ape\" \"apple\") (\"application\" \"apricot\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_hash_table_completion_predicate_gets_key_and_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((completion-ignore-case nil)
      (completion-regexp-list nil)
      (table (make-hash-table :test 'equal)))
  (puthash "alpha" 1 table)
  (puthash "alpine" 2 table)
  (puthash "beta" 3 table)
  (puthash 'alias 4 table)
  (puthash 42 5 table)
  (list
   (try-completion "al" table)
   (sort (copy-sequence (all-completions "al" table)) #'string<)
   (sort (copy-sequence
          (all-completions "al" table
                           (lambda (key value)
                             (and (stringp key) (= value 2)))))
         #'string<)
   (try-completion "ali" table)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"al\" (\"alias\" \"alpha\" \"alpine\") (\"alpine\") \"alias\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_function_completion_table_actions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((calls nil)
      (table (lambda (string pred action)
               (push (list string (not (null pred)) action) calls)
               (cond
                ((eq action nil) "computed")
                ((eq action t) '("computed" "compact"))
                ((eq (car-safe action) 'boundaries) '(boundaries 1 . 2))
                ((eq action 'metadata) '(metadata (category . custom)))))))
  (list
   (try-completion "co" table #'identity)
   (all-completions "co" table #'identity)
   (completion-boundaries "prefix" table #'identity "suffix")
   (completion-metadata "co" table #'identity)
   (nreverse calls)))
"#;

    let expect = expect_test::expect![[r#""ERR (void-variable calls)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
