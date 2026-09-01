//! Oracle parity tests for `rename-buffer` with ALL parameter combinations:
//! NEWNAME (string), UNIQUE (nil vs t), return value, error on duplicate
//! without UNIQUE, batch rename, and unique suffix generation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic rename-buffer: NEWNAME only
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // rename-buffer returns the new name; verify buffer-name changes
    let form = r#"(with-temp-buffer
  (insert "content")
  (let ((old-name (buffer-name)))
    (rename-buffer "*neovm-oracle-rename-basic*")
    (let ((new-name (buffer-name)))
      (list old-name new-name
            (string= new-name "*neovm-oracle-rename-basic*")
            ;; buffer-substring should be preserved
            (buffer-string)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\" *temp*\" \"*neovm-oracle-rename-basic*\" t \"content\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer return value is the new name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (let ((result (rename-buffer "*neovm-oracle-rename-ret*")))
    (list result
          (stringp result)
          (string= result (buffer-name))
          (string= result "*neovm-oracle-rename-ret*"))))"#;
    let expect = expect_test::expect![[r#""OK (\"*neovm-oracle-rename-ret*\" t t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer with UNIQUE = nil (default) — error on duplicate
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_duplicate_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Renaming to an existing buffer name without UNIQUE should signal an error
    let form = r#"(let ((b1 (generate-new-buffer "*neovm-oracle-dup-a*"))
                        (b2 (generate-new-buffer "*neovm-oracle-dup-b*")))
  (unwind-protect
      (progn
        (with-current-buffer b1
          (rename-buffer "*neovm-oracle-dup-target*"))
        (with-current-buffer b2
          (condition-case err
              (progn
                (rename-buffer "*neovm-oracle-dup-target*")
                'no-error)
            (error (list 'got-error (car err))))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))"#;
    let expect = expect_test::expect![[r#""OK (got-error error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer with UNIQUE = t — auto-generates unique name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_unique_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When UNIQUE is t and the name already exists, Emacs appends <N>
    let form = r#"(let ((b1 (generate-new-buffer "*neovm-oracle-uniq*"))
                        (b2 (generate-new-buffer "*neovm-oracle-uniq-src*")))
  (unwind-protect
      (progn
        (with-current-buffer b1
          (rename-buffer "*neovm-oracle-uniq-target*"))
        (with-current-buffer b2
          (let ((result (rename-buffer "*neovm-oracle-uniq-target*" t)))
            (list result
                  (stringp result)
                  ;; Should have <2> or similar suffix
                  (not (string= result "*neovm-oracle-uniq-target*"))
                  ;; Should start with the requested name
                  (string-match-p "\\`\\*neovm-oracle-uniq-target\\*" result)
                  ;; buffer-name should match
                  (string= result (buffer-name))))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))))"#;
    let expect = expect_test::expect![[r#""OK (\"*neovm-oracle-uniq-target*<2>\" t t 0 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer with UNIQUE when no conflict — no suffix added
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_unique_no_conflict() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // UNIQUE = t but no conflict: name should be exactly as requested
    let form = r#"(with-temp-buffer
  (let ((result (rename-buffer "*neovm-oracle-unique-noconflict*" t)))
    (list result
          (string= result "*neovm-oracle-unique-noconflict*")
          (string= result (buffer-name)))))"#;
    let expect = expect_test::expect![[r#""OK (\"*neovm-oracle-unique-noconflict*\" t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple successive renames on the same buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_successive_renames() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "data")
  (let ((names nil))
    (rename-buffer "*neovm-oracle-rename-s1*")
    (setq names (cons (buffer-name) names))
    (rename-buffer "*neovm-oracle-rename-s2*")
    (setq names (cons (buffer-name) names))
    (rename-buffer "*neovm-oracle-rename-s3*")
    (setq names (cons (buffer-name) names))
    ;; Verify old names are no longer taken
    (let ((s1-exists (get-buffer "*neovm-oracle-rename-s1*"))
          (s2-exists (get-buffer "*neovm-oracle-rename-s2*"))
          (s3-exists (get-buffer "*neovm-oracle-rename-s3*")))
      (list (nreverse names)
            ;; Only the final name should still be live
            (null s1-exists)
            (null s2-exists)
            (bufferp s3-exists)
            (buffer-string)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"*neovm-oracle-rename-s1*\" \"*neovm-oracle-rename-s2*\" \"*neovm-oracle-rename-s3*\") t t t \"data\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Batch rename with unique suffix generation across multiple buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_batch_unique_suffixes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create several buffers and rename them all to the same base name with UNIQUE=t.
    // Each should get a different <N> suffix.
    let form = r#"(let ((buffers nil))
  (unwind-protect
      (progn
        ;; Create 5 buffers and rename each to the same target with UNIQUE=t
        (let ((i 0))
          (while (< i 5)
            (let ((b (generate-new-buffer (format "*neovm-oracle-batch-src-%d*" i))))
              (setq buffers (cons b buffers))
              (with-current-buffer b
                (rename-buffer "*neovm-oracle-batch-target*" t)))
            (setq i (1+ i))))
        ;; Collect all names — they should all be distinct
        (let ((names (mapcar (lambda (b) (buffer-name b))
                             (nreverse buffers))))
          (list names
                ;; All distinct
                (= (length names) (length (delete-dups (copy-sequence names))))
                ;; All start with the target base
                (cl-every (lambda (n)
                            (string-match-p "\\`\\*neovm-oracle-batch-target\\*" n))
                          names)
                ;; The count
                (length names))))
    (dolist (b buffers)
      (when (buffer-live-p b) (kill-buffer b)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer with empty string — should signal error
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_empty_string_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (condition-case err
      (progn
        (rename-buffer "")
        'no-error)
    (error (list 'got-error (car err)))))"#;
    let expect = expect_test::expect![[r#""OK (got-error error)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer preserves buffer content, point, and markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_preserves_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "hello world")
  (goto-char 6)
  (let ((pt-before (point))
        (content-before (buffer-string))
        (size-before (buffer-size)))
    (rename-buffer "*neovm-oracle-rename-state*")
    (list (= pt-before (point))
          (string= content-before (buffer-string))
          (= size-before (buffer-size))
          (buffer-name))))"#;
    let expect = expect_test::expect![[r#""OK (t t t \"*neovm-oracle-rename-state*\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
