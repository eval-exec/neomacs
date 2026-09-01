//! Oracle parity tests for GNU `load-history` helper semantics.
//!
//! GNU implements `load-history-regexp` and
//! `load-history-filename-element` in `lisp/subr.el`.  They are part of the
//! `eval-after-load` path, so their exact file-name regexp behavior matters
//! during startup and package loading.

use crate::common::{assert_oracle_parity, return_if_neovm_enable_oracle_proptest_not_set};

#[test]
fn oracle_prop_gnu_load_history_regexp_matches_suffix_rules() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-suffixes '(".elc" ".el" ""))
      (jka-compr-load-suffixes '(".gz" ".bz2" ""))
      (default-directory "/tmp/"))
  (let ((relative-no-ext (load-history-regexp "foo/bar"))
        (relative-el (load-history-regexp "foo/bar.el"))
        (absolute-no-ext (load-history-regexp "/tmp/neomacs-oracle-load-history")))
    (list
     relative-no-ext
     relative-el
     absolute-no-ext
     (mapcar (lambda (name)
               (not (null (string-match relative-no-ext name))))
             '("foo/bar"
               "/usr/share/emacs/foo/bar"
               "/usr/share/emacs/foo/bar.el"
               "/usr/share/emacs/foo/bar.el.gz"
               "/usr/share/emacs/foo/bar.txt"
               "/usr/share/emacs/not-foo/bar.el"))
     (mapcar (lambda (name)
               (not (null (string-match relative-el name))))
             '("foo/bar.el"
               "/usr/share/emacs/foo/bar.el"
               "/usr/share/emacs/foo/bar.el.gz"
               "/usr/share/emacs/foo/bar.elc"
               "/usr/share/emacs/foo/bar"))
     (mapcar (lambda (name)
               (not (null (string-match absolute-no-ext name))))
             '("/tmp/neomacs-oracle-load-history"
               "/tmp/neomacs-oracle-load-history.el"
               "/tmp/neomacs-oracle-load-history.el.gz"
               "/var/tmp/neomacs-oracle-load-history.el")))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\(\\\\`\\\\|/\\\\)foo/bar\\\\(\\\\.elc\\\\|\\\\.el\\\\|\\\\)?\\\\(\\\\.gz\\\\|\\\\.bz2\\\\|\\\\)?\\\\'\" \"\\\\(\\\\`\\\\|/\\\\)foo/bar\\\\.el\\\\(\\\\.gz\\\\|\\\\.bz2\\\\|\\\\)?\\\\'\" \"\\\\`/tmp/neomacs-oracle-load-history\\\\(\\\\.elc\\\\|\\\\.el\\\\|\\\\)?\\\\(\\\\.gz\\\\|\\\\.bz2\\\\|\\\\)?\\\\'\" (t t t t nil nil) (t t t nil nil) (t t t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_load_history_filename_element_preserves_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((load-suffixes '(".elc" ".el" ""))
      (jka-compr-load-suffixes '(".gz" ""))
      (load-history
       '((nil ignored-entry)
         ("/tmp/neomacs-oracle-load-history/alpha.el" (provide . alpha))
         ("/tmp/neomacs-oracle-load-history/beta.el.gz" (provide . beta))
         ("/tmp/neomacs-oracle-load-history/beta.el" (provide . beta-uncompressed))
         ("/tmp/neomacs-oracle-load-history/gamma.txt" (provide . gamma)))))
  (string-match "\\(a\\)\\(b\\)" "zabz")
  (let ((before (match-data t))
        (alpha-regexp (load-history-regexp "/tmp/neomacs-oracle-load-history/alpha.el"))
        (beta-regexp (load-history-regexp "beta"))
        (missing-regexp (load-history-regexp "missing")))
    (let ((alpha (load-history-filename-element alpha-regexp))
          (beta (load-history-filename-element beta-regexp))
          (missing (load-history-filename-element missing-regexp))
          (after (match-data t)))
      (list
       alpha
       beta
       missing
       (equal before after)
       after))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((\"/tmp/neomacs-oracle-load-history/alpha.el\" (provide . alpha)) (\"/tmp/neomacs-oracle-load-history/beta.el.gz\" (provide . beta)) nil t (1 3 1 2 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
