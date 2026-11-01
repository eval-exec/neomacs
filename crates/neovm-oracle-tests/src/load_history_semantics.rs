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
(let* ((root (file-truename "/tmp/neomacs-oracle-load-history"))
       (load-suffixes '(".elc" ".el" ""))
       (jka-compr-load-suffixes '(".gz" ".bz2" ""))
       (default-directory "/tmp/"))
  (let ((relative-no-ext (load-history-regexp "foo/bar"))
        (relative-el (load-history-regexp "foo/bar.el"))
        (absolute-no-ext (load-history-regexp root)))
    (list
     ;; The regexp TEXT embeds the host truename (/tmp vs /private/tmp), so
     ;; assert the suffix RULES through the names they match; the absolute
     ;; names are built from the same canonical root as the regexp.
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
             (list root
                   (concat root ".el")
                   (concat root ".el.gz")
                   "/var/tmp/neomacs-oracle-load-history.el")))))
"#;

    let expect = expect_test::expect![[r#""OK ((t t t t nil nil) (t t t nil nil) (t t t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_gnu_load_history_filename_element_preserves_match_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
;; load-history-regexp calls file-truename on absolute names
;; (lisp/subr.el:6296-6311), so build the fixture at the canonical root:
;; /tmp is its own truename on Linux, /private/tmp on macOS.  The answer
;; reports the matched entry's cdr, never the host path.
(let* ((root (file-truename "/tmp/neomacs-oracle-load-history"))
       (load-suffixes '(".elc" ".el" ""))
       (jka-compr-load-suffixes '(".gz" ""))
       (load-history
        (list
         (list nil 'ignored-entry)
         (list (expand-file-name "alpha.el" root) '(provide . alpha))
         (list (expand-file-name "beta.el.gz" root) '(provide . beta))
         (list (expand-file-name "beta.el" root) '(provide . beta-uncompressed))
         (list (expand-file-name "gamma.txt" root) '(provide . gamma)))))
  (string-match "\\(a\\)\\(b\\)" "zabz")
  (let ((before (match-data t))
        (alpha-regexp (load-history-regexp (expand-file-name "alpha.el" root)))
        (beta-regexp (load-history-regexp "beta"))
        (missing-regexp (load-history-regexp "missing")))
    (let ((alpha (load-history-filename-element alpha-regexp))
          (beta (load-history-filename-element beta-regexp))
          (missing (load-history-filename-element missing-regexp))
          (after (match-data t)))
      (list
       (and alpha (cdr alpha))
       (and beta (cdr beta))
       missing
       (equal before after)
       after))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((provide . alpha)) ((provide . beta)) nil t (1 3 1 2 2 3))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
