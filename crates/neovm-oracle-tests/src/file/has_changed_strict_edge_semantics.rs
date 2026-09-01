//! Oracle parity tests for GNU `file-has-changed-p` semantics.
//!
//! GNU implements this in `lisp/files.el` with an internal hash table keyed by
//! `(symbol-name TAG) @ normalized-file-name`.  It compares file size and
//! integer modification time from `file-attributes`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_file_has_changed_cache_tags_missing_and_type_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-changed-" t))
       (file (expand-file-name "tracked.txt" dir))
       (missing (expand-file-name "missing.txt" dir)))
  (unwind-protect
      (progn
        (clrhash file-has-changed-p--hash-table)
        (write-region "a" nil file nil 'silent)
        (set-file-times file 100)
        (let ((initial
               (list
                ;; First call for an existing file records the current
                ;; attribute pair and returns non-nil.
                (file-has-changed-p file)
                ;; Second call with the same implicit nil tag is cached.
                (file-has-changed-p file)
                ;; A distinct symbol tag has an independent cache entry.
                (file-has-changed-p file 'alpha)
                (file-has-changed-p file 'alpha)
                ;; Missing files have nil attributes and no change from the
                ;; initial nil cache value.
                (file-has-changed-p missing)
                (file-has-changed-p missing 'alpha))))
          (write-region "abcdef" nil file nil 'silent)
          (set-file-times file 200)
          (append
           initial
           (list
            ;; Size changed, so both tag caches observe a change once.
            (file-has-changed-p file)
            (file-has-changed-p file)
            (file-has-changed-p file 'alpha)
            (file-has-changed-p file 'alpha)
            ;; Deleting an existing cached file changes its cached attr to nil,
            ;; but the function still returns nil because it stores nil.
            (progn (delete-file file) (file-has-changed-p file))
            (file-has-changed-p file)
            (condition-case err
                (file-has-changed-p 42)
              (error (list (car err) (cdr err))))
            (condition-case err
                (file-has-changed-p missing 42)
              (error (list (car err) (cdr err))))))))
    (ignore-errors (delete-file file))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((1 0 100 0 0) nil (1 0 100 0 0) nil nil nil (6 0 200 0 0) nil (6 0 200 0 0) nil nil nil (wrong-type-argument (stringp 42)) (wrong-type-argument (symbolp 42)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_file_has_changed_directory_file_name_cache_key_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let* ((dir (make-temp-file "neomacs-oracle-file-changed-slash-" t))
       (default-directory (file-name-as-directory dir))
       (file "tracked.txt")
       (slash "tracked.txt/"))
  (unwind-protect
      (progn
        (clrhash file-has-changed-p--hash-table)
        (write-region "a" nil file nil 'silent)
        (set-file-times file 100)
        (list
         ;; GNU strips the trailing slash with `directory-file-name` before
         ;; querying attributes and before building the cache key.
         (file-has-changed-p file)
         (file-has-changed-p slash)
         (hash-table-count file-has-changed-p--hash-table)
         (progn
           (write-region "abcdef" nil file nil 'silent)
           (set-file-times file 200)
           (file-has-changed-p slash))
         (file-has-changed-p file)
         (hash-table-count file-has-changed-p--hash-table)))
    (ignore-errors (delete-file (expand-file-name file dir)))
    (ignore-errors (delete-directory dir))))
"#;

    let expect = expect_test::expect![[r#""OK ((1 0 100 0 0) nil 1 (6 0 200 0 0) nil 1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
