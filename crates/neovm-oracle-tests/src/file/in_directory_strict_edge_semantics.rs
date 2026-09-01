//! Oracle parity tests for GNU `file-in-directory-p` semantics.
//!
//! GNU implements this in `lisp/files.el`: `DIR` must be an existing
//! directory, then both arguments are resolved through `file-truename`, split
//! into path components, and the common root is confirmed with `file-equal-p`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_in_directory_symlink_missing_and_self_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-in-dir-" t))
       (root (expand-file-name "root" dir))
       (child (expand-file-name "child" root))
       (other (expand-file-name "other" dir))
       (file (expand-file-name "file.txt" child))
       (link-root (expand-file-name "link-root" dir))
       (link-child-file (expand-file-name "link-child-file" dir))
       (missing-under-link (expand-file-name "child/missing.txt" link-root))
       (missing-dir (expand-file-name "missing-dir" dir)))
  (unwind-protect
      (progn
        (make-directory child t)
        (make-directory other)
        (write-region "x" nil file nil 'silent)
        (make-symbolic-link "root" link-root)
        (make-symbolic-link "root/child/file.txt" link-child-file)
        (list
         ;; A directory is considered a parent of itself.
         (file-in-directory-p root root)
         (file-in-directory-p (file-name-as-directory root) root)
         ;; Direct descendants and symlinked parents resolve through
         ;; `file-truename`.
         (file-in-directory-p file root)
         (file-in-directory-p file link-root)
         (file-in-directory-p link-child-file root)
         (file-in-directory-p missing-under-link root)
         ;; Existing sibling directories and missing directory arguments are
         ;; rejected.
         (file-in-directory-p file other)
         (file-in-directory-p file missing-dir)
         (file-in-directory-p missing-under-link missing-dir)
         ;; A missing file can still be inside an existing directory because
         ;; `file-truename` preserves the missing suffix.
         (file-in-directory-p (expand-file-name "missing.txt" child) root)
         (condition-case err
             (file-in-directory-p 42 root)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-in-directory-p file 42)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file link-child-file))
    (ignore-errors (delete-file link-root))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory child))
    (ignore-errors (delete-directory root))
    (ignore-errors (delete-directory other))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 48 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_in_directory_relative_prefix_and_escape_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-in-dir-prefix-" t))
       (default-directory (file-name-as-directory dir))
       (root (expand-file-name "root" dir))
       (child (expand-file-name "child" root))
       (root2 (expand-file-name "root2" dir))
       (escape (expand-file-name "escape" dir))
       (file (expand-file-name "file.txt" child))
       (root2-file (expand-file-name "file.txt" root2))
       (escape-file (expand-file-name "file.txt" escape))
       (regular-dir-arg (expand-file-name "regular-dir-arg.txt" dir))
       (inside-link-to-outside (expand-file-name "outside-link.txt" child)))
  (unwind-protect
      (progn
        (make-directory child t)
        (make-directory root2)
        (make-directory escape)
        (write-region "inside" nil file nil 'silent)
        (write-region "root2" nil root2-file nil 'silent)
        (write-region "escape" nil escape-file nil 'silent)
        (write-region "not a directory" nil regular-dir-arg nil 'silent)
        (make-symbolic-link "../escape/file.txt" inside-link-to-outside)
        (list
         ;; Relative names are expanded by `file-truename` from
         ;; `default-directory`, and . / .. components are normalized before
         ;; the component-wise parent comparison.
         (file-in-directory-p "root/child/./file.txt" "root")
         (file-in-directory-p "root/child/../child/file.txt" "root")
         (file-in-directory-p "./root/child/file.txt" "./root/child/..")
         ;; Textual path prefixes are not enough: root2 is a sibling, not a
         ;; descendant of root.
         (file-in-directory-p "root2/file.txt" "root")
         (file-in-directory-p "root/../root2/file.txt" "root")
         ;; Parent relation is directional.
         (file-in-directory-p "root" "root/child")
         ;; DIR must exist and must be a directory.
         (file-in-directory-p "root/child/file.txt" regular-dir-arg)
         ;; FILE is resolved through `file-truename`, so a symlink located
         ;; under DIR but pointing outside DIR is rejected.
         (file-in-directory-p inside-link-to-outside root)
         ;; But the symlink target is accepted under its real parent.
         (file-in-directory-p inside-link-to-outside escape))))
    (ignore-errors (delete-file inside-link-to-outside))
    (ignore-errors (delete-file regular-dir-arg))
    (ignore-errors (delete-file escape-file))
    (ignore-errors (delete-file root2-file))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory escape))
    (ignore-errors (delete-directory root2))
    (ignore-errors (delete-directory child))
    (ignore-errors (delete-directory root))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 52 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_in_directory_empty_root_and_normalized_file_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-in-dir-root-" t))
       (default-directory (file-name-as-directory dir))
       (root (expand-file-name "root" dir))
       (child (expand-file-name "child" root))
       (file (expand-file-name "file.txt" child)))
  (unwind-protect
      (progn
        (make-directory child t)
        (write-region "x" nil file nil 'silent)
        (list
         ;; Empty names are expanded by `file-truename` relative to
         ;; `default-directory`, so these are true when DIR is that default.
         (file-in-directory-p "" dir)
         (file-in-directory-p dir "")
         (file-in-directory-p "." dir)
         (file-in-directory-p dir ".")
         ;; Root is a parent of itself and every absolute path below it, but
         ;; string-prefix lookalikes are still rejected component-wise.
         (file-in-directory-p "/" "/")
         (file-in-directory-p "/tmp" "/")
         (file-in-directory-p "/tmp2" "/tmp")
         ;; FILE is resolved by `file-truename`, so trailing slash and parent
         ;; components are normalized before comparing against DIR.
         (file-in-directory-p (concat file "/") root)
         (file-in-directory-p (concat child "/../child/file.txt") root)
         (file-in-directory-p (concat child "/../../root/child/file.txt") root))))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory child))
    (ignore-errors (delete-directory root))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 31 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_in_directory_type_null_and_missing_dir_order_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-in-dir-order-" t))
       (root (expand-file-name "root" dir))
       (file (expand-file-name "file.txt" root))
       (missing-dir (expand-file-name "missing-dir" dir))
       (nul-name (string ?a 0 ?b)))
  (unwind-protect
      (progn
        (make-directory root)
        (write-region "x" nil file nil 'silent)
        (list
         ;; GNU asks `find-file-name-handler' about FILE first, so a non-string
         ;; FILE signals even when DIR is missing.
         (condition-case err
             (file-in-directory-p 42 missing-dir)
           (error (list (car err) (cdr err))))
         ;; But a string FILE containing a null byte reaches the DIR existence
         ;; check.  When DIR is missing, GNU returns nil before `file-truename'
         ;; validates FILE as a filename.
         (condition-case err
             (file-in-directory-p nul-name missing-dir)
           (error (list (car err) (cdr err))))
         ;; With an existing DIR, FILE is passed to `file-truename' and the
         ;; null-byte filename check must signal.
         (condition-case err
             (file-in-directory-p nul-name root)
           (error (list (car err) (cdr err))))
         ;; DIR validation is performed by `file-directory-p' before the
         ;; truename/component comparison.
         (condition-case err
             (file-in-directory-p file nul-name)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-in-directory-p file nil)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory root))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 38 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_in_directory_file_name_handler_dispatch_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(progn
  (setq neomacs--oracle-file-in-dir-calls nil)
  (defun neomacs--oracle-file-in-dir-file-handler (operation &rest args)
    (push (list 'file operation args) neomacs--oracle-file-in-dir-calls)
    (list 'file-handler operation args))
  (defun neomacs--oracle-file-in-dir-dir-handler (operation &rest args)
    (push (list 'dir operation args) neomacs--oracle-file-in-dir-calls)
    (list 'dir-handler operation args))
  (unwind-protect
      (let ((file-name-handler-alist
             '(("\\`/oracle-file:" . neomacs--oracle-file-in-dir-file-handler)
               ("\\`/oracle-dir:" . neomacs--oracle-file-in-dir-dir-handler))))
        (list
         (file-in-directory-p "/oracle-file:child" "/plain-dir")
         neomacs--oracle-file-in-dir-calls
         (setq neomacs--oracle-file-in-dir-calls nil)
         (file-in-directory-p "/plain-file" "/oracle-dir:root")
         neomacs--oracle-file-in-dir-calls
         (setq neomacs--oracle-file-in-dir-calls nil)
         ;; GNU probes FILE first, so this must dispatch to the file handler
         ;; even when DIR has its own handler.
         (file-in-directory-p "/oracle-file:child" "/oracle-dir:root")
         neomacs--oracle-file-in-dir-calls))
    (fmakunbound 'neomacs--oracle-file-in-dir-file-handler)
    (fmakunbound 'neomacs--oracle-file-in-dir-dir-handler)
    (makunbound 'neomacs--oracle-file-in-dir-calls)))
"#;

    let expect = expect_test::expect![[
        r#""OK ((file-handler file-in-directory-p (\"/oracle-file:child\" \"/plain-dir\")) ((file file-in-directory-p (\"/oracle-file:child\" \"/plain-dir\"))) nil (dir-handler file-in-directory-p (\"/plain-file\" \"/oracle-dir:root\")) ((dir file-in-directory-p (\"/plain-file\" \"/oracle-dir:root\"))) nil (file-handler file-in-directory-p (\"/oracle-file:child\" \"/oracle-dir:root\")) ((file file-in-directory-p (\"/oracle-file:child\" \"/oracle-dir:root\"))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
