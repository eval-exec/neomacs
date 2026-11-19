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
    (ignore-errors (delete-directory dir)))"#;

    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

/// GNU's file-name-directory/nondirectory/as-directory/directory-file-name
/// allocate fresh strings (`make_specified_string`, src/fileio.c:505,572,671,
/// 762): text properties on the input never reach the result. Neomacs cloned
/// the input on several no-op branches, which leaked the `directory` property
/// company-c-headers attaches to its candidates into the location it builds
/// with `file-name-nondirectory` (the company_c_headers MELPA batch divergence).
/// `file-relative-name` then inherited the leaked property. This row pins the
/// property-drop contract and the company-shaped chain on both editors.
#[test]
fn oracle_file_name_primitives_drop_input_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir "/tmp/neomacs-oracle-fnprops/")
       (candidate (propertize "sub/concept.hpp" 'directory dir))
       (tail (substring candidate 4))
       (head (substring candidate 0 3))
       (location (concat (file-name-as-directory "/abs/include/")
                         (file-name-nondirectory tail)))
       (relative (file-relative-name
                  (concat "/tmp/neomacs-oracle-fnprops/"
                          (file-name-nondirectory tail))
                  dir)))
  (list
   (file-name-nondirectory tail)
   (text-properties-at 0 (file-name-nondirectory tail))
   (file-name-directory (concat "/tmp/" tail))
   (text-properties-at 0 (file-name-directory (concat "/tmp/" tail)))
   (file-name-as-directory (concat "/tmp/" tail "/"))
   (text-properties-at 0 (file-name-as-directory (concat "/tmp/" tail "/")))
   (directory-file-name (concat "/tmp/" tail "///"))
   (text-properties-at 0 (directory-file-name (concat "/tmp/" tail "///")))
   location
   (text-properties-at 0 location)
   relative
   (text-properties-at 0 relative)
   ;; The input candidate still carries its property; only the fresh
   ;; file-name-* results are plain.
   (text-properties-at 0 head)))
"#;

    let expect = expect_test::expect![[
        r#""OK (\"concept.hpp\" nil \"/tmp/\" nil \"/tmp/concept.hpp/\" nil \"/tmp/concept.hpp\" nil \"/abs/include/concept.hpp\" nil \"concept.hpp\" nil (directory \"/tmp/neomacs-oracle-fnprops/\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
