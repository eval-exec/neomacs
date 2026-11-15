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

/// GNU interns every compiled `DEFSYM` spelling at startup, so the initial
/// obarray's `"buffer-"` completion includes `buffer-or-string-p`
/// (`src/data.c:4084`; in 31.1 `DEFSYM` is only a make-docfile marker,
/// `src/lisp.h:2395`, and `init_obarray_once` interns `defsym_name[]`,
/// `src/lread.c:5193-5203`). The port used to complete 73 names here.
#[test]
fn oracle_prop_all_completions_of_the_global_obarray_include_defsym_only_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(all-completions "buffer-" obarray)"#;
    let expect = expect_test::expect![[
        r#""OK (\"buffer-face-mode\" \"buffer-file-coding-system-explicit\" \"buffer-menu-buffer\" \"buffer-face-set\" \"buffer-base-buffer\" \"buffer-read-only\" \"buffer-menu-open\" \"buffer-live-p\" \"buffer-match-p--past-warnings\" \"buffer-invisibility-spec\" \"buffer-file-format\" \"buffer-text-pixel-size\" \"buffer-access-fontified-property\" \"buffer-to-register\" \"buffer-line-statistics\" \"buffer-undo-list\" \"buffer-enable-undo\" \"buffer-display-table\" \"buffer-list-update-hook\" \"buffer-menu-other-window\" \"buffer-save-without-query\" \"buffer-file-name\" \"buffer-local-variables\" \"buffer-file-coding-system\" \"buffer-disable-undo\" \"buffer-end\" \"buffer-last-name\" \"buffer-face-toggle\" \"buffer-file-truename\" \"buffer-substring--filter\" \"buffer-chars-modified-tick\" \"buffer-stale-function\" \"buffer-predicate\" \"buffer-substring-no-properties\" \"buffer-quit-function\" \"buffer-display-time\" \"buffer-local-toplevel-value\" \"buffer-saved-size\" \"buffer-modified-p\" \"buffer-local-set-state\" \"buffer-local-boundp\" \"buffer-list\" \"buffer-name-history\" \"buffer-navigation-repeat-map\" \"buffer-size\" \"buffer-name\" \"buffer-face-mode-invoke\" \"buffer-narrowed-p\" \"buffer-auto-save-file-format\" \"buffer-offer-save\" \"buffer-substring\" \"buffer-file-read-only\" \"buffer-hash\" \"buffer-file-number\" \"buffer-string\" \"buffer-display-count\" \"buffer-substring-with-bidi-context\" \"buffer-local-set-state--get\" \"buffer-position\" \"buffer-auto-save-file-name\" \"buffer-local-restore-state\" \"buffer-match-p\" \"buffer-file-numbers-unique\" \"buffer-stale--default-function\" \"buffer-or-name\" \"buffer-access-fontify-functions\" \"buffer-menu--display-help\" \"buffer-swap-text\" \"buffer-modified-tick\" \"buffer-backed-up\" \"buffer-auto-revert-by-notification\" \"buffer-menu\" \"buffer-local-value\" \"buffer-or-string-p\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
