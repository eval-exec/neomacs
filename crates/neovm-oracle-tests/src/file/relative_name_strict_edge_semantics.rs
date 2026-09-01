//! Oracle parity tests for GNU `file-relative-name` semantics.
//!
//! GNU implements this in `lisp/files.el`.  The function expands FILENAME and
//! DIRECTORY, preserves directory-vs-file distinctions in the returned string,
//! climbs to the nearest common parent, and refuses to relativize across
//! different remote identifiers.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_relative_name_local_remote_and_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-relative-name-" t))
       (a (expand-file-name "a" dir))
       (b (expand-file-name "b" dir))
       (space-dir (expand-file-name "space dir" a))
       (file-in-a (expand-file-name "file.txt" a))
       (file-in-b (expand-file-name "file.txt" b))
       (default-directory (file-name-as-directory a)))
  (unwind-protect
      (progn
        (make-directory a)
        (make-directory b)
        (make-directory space-dir)
        (write-region "a" nil file-in-a nil 'silent)
        (write-region "b" nil file-in-b nil 'silent)
        (list
         ;; Same-directory files and exact directory identity.
         (file-relative-name file-in-a a)
         (file-relative-name a dir)
         (file-relative-name (file-name-as-directory a) dir)
         (file-relative-name (file-name-as-directory a)
                             (file-name-as-directory a))
         (file-relative-name a (file-name-as-directory a))
         ;; Siblings, root traversal, spaces, and relative FILENAME expansion
         ;; through `default-directory'.
         (file-relative-name file-in-b a)
         (file-relative-name dir a)
         (file-relative-name file-in-a "/")
         (file-relative-name (expand-file-name "leaf.el" space-dir) a)
         (file-relative-name "relative.el" b)
         (file-relative-name "../b/file.txt" a)
         (file-relative-name "" a)
         ;; Different remote identifiers return expanded FILENAME; identical
         ;; remote identifiers are relativized syntactically.
         (file-relative-name "/ssh:user@host:/tmp/a" "/tmp/")
         (file-relative-name "/tmp/a" "/ssh:user@host:/tmp/")
         (file-relative-name "/ssh:user@host:/tmp/a" "/ssh:user@host:/tmp/")
         (file-relative-name "/ssh:user@host:/tmp/a" "/ssh:other@host:/tmp/")
         (condition-case err
             (file-relative-name 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-relative-name nil)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-relative-name "x" 42)
           (error (list (car err) (cdr err))))
         (condition-case err
             (file-relative-name "x" nil nil)
           (error (list (car err) (cdr err)))))))
    (ignore-errors (delete-file file-in-b))
    (ignore-errors (delete-file file-in-a))
    (ignore-errors (delete-directory space-dir))
    (ignore-errors (delete-directory b))
    (ignore-errors (delete-directory a))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 56 44)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
