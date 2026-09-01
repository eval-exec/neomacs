//! Oracle parity tests for GNU directory abbreviation helpers.
//!
//! GNU implements `directory-abbrev-make-regexp` and
//! `directory-abbrev-apply` in `lisp/files.el`.  The behavior is intentionally
//! regexp based: generated abbreviations match only the exact directory or a
//! slash boundary, and `directory-abbrev-apply` honors the caller's
//! `case-fold-search` binding while walking `directory-abbrev-alist` in order.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_directory_abbrev_regexp_boundaries_and_apply_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((regexp (directory-abbrev-make-regexp "/tmp/a.b"))
       (boundary-probes
        (mapcar (lambda (name)
                  (list name (and (string-match regexp name)
                                  (match-string 0 name))))
                '("/tmp/a.b" "/tmp/a.b/" "/tmp/a.b/file"
                  "/tmp/a.b-extra" "/tmp/aXb" "prefix/tmp/a.b")))
       (ordered-alist '(("\\`/tmp/root/long/" . "/one/")
                        ("\\`/tmp/root/" . "/two/")))
       (case-alist '(("\\`/tmp/case/" . "/lower/"))))
  (list
   regexp
   boundary-probes
   (let ((directory-abbrev-alist ordered-alist)
         (case-fold-search nil))
     (mapcar #'directory-abbrev-apply
             '("/tmp/root/long/file.el"
               "/tmp/root/other.el"
               "/tmp/root/longer/file.el"
               "relative/tmp/root/long/file.el")))
   (let ((directory-abbrev-alist case-alist)
         (case-fold-search nil))
     (directory-abbrev-apply "/TMP/CASE/file.el"))
   (let ((directory-abbrev-alist case-alist)
         (case-fold-search t))
     (directory-abbrev-apply "/TMP/CASE/file.el"))
   (let ((directory-abbrev-alist nil))
     (directory-abbrev-apply "/tmp/root/file.el"))
   (condition-case err
       (directory-abbrev-make-regexp 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (directory-abbrev-apply 42)
     (error (list (car err) (cdr err))))
   (condition-case err
       (directory-abbrev-apply)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"\\\\`/tmp/a\\\\.b\\\\(/\\\\|\\\\'\\\\)\" ((\"/tmp/a.b\" \"/tmp/a.b\") (\"/tmp/a.b/\" \"/tmp/a.b/\") (\"/tmp/a.b/file\" \"/tmp/a.b/\") (\"/tmp/a.b-extra\" nil) (\"/tmp/aXb\" nil) (\"prefix/tmp/a.b\" nil)) (\"/one/file.el\" \"/two/other.el\" \"/two/longer/file.el\" \"relative/tmp/root/long/file.el\") \"/TMP/CASE/file.el\" \"/lower/file.el\" \"/tmp/root/file.el\" (wrong-type-argument (stringp 42)) 42 (wrong-number-of-arguments ((1 . 1) 0)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
