//! Oracle parity tests for advanced buffer operations:
//! nested `with-temp-buffer`, `get-buffer-create`, `buffer-name`,
//! `buffer-live-p`, `rename-buffer`, `set-buffer`/`current-buffer`,
//! `buffer-size` vs `point-max` vs `(length (buffer-string))`,
//! `erase-buffer` + re-insertion, multi-buffer text processing,
//! and buffer-local variable simulation via alists.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Nested with-temp-buffer (buffer within buffer)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_with_temp_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Outer temp buffer inserts text, inner temp buffer processes it,
    // then outer buffer continues with its own state intact
    let form = r#"(with-temp-buffer
  (insert "outer-content")
  (let ((outer-size (buffer-size))
        (outer-point (point)))
    (let ((inner-result
           (with-temp-buffer
             (insert "inner-")
             (insert "content")
             (let ((inner-str (buffer-string)))
               (list (buffer-size) inner-str (point))))))
      ;; After inner with-temp-buffer, outer state is restored
      (list outer-size outer-point
            (buffer-size) (point)
            (buffer-string)
            inner-result))))"#;
    let expect =
        expect_test::expect![[r#""OK (13 14 13 14 \"outer-content\" (13 \"inner-content\" 14))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// get-buffer-create / buffer-name / buffer-live-p
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_get_buffer_create_and_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create named buffer, check its properties, kill it, verify buffer-live-p
    let form = r#"(let* ((buf (get-buffer-create " *neovm-test-buf-adv*"))
                         (name-before (buffer-name buf))
                         (live-before (buffer-live-p buf)))
  (unwind-protect
      (progn
        (with-current-buffer buf
          (insert "test data")
          (let ((content (buffer-string))
                (sz (buffer-size)))
            (list name-before live-before content sz))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[r#""OK (\" *neovm-test-buf-adv*\" t \"test data\" 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// rename-buffer and its effect on buffer-name
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rename_buffer_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create buffer, rename it, verify buffer-name changes,
    // also test that get-buffer finds it under the new name
    let form = r#"(let ((buf (get-buffer-create " *neovm-rename-test*")))
  (unwind-protect
      (progn
        (let ((old-name (buffer-name buf)))
          (with-current-buffer buf
            (rename-buffer " *neovm-renamed*" t)
            (let ((new-name (buffer-name)))
              (insert "renamed buffer content")
              (list old-name
                    new-name
                    (buffer-name buf)
                    (buffer-string)
                    (buffer-size))))))
    (when (buffer-live-p buf) (kill-buffer buf))))"#;
    let expect = expect_test::expect![[
        r#""OK (\" *neovm-rename-test*\" \" *neovm-renamed*\" \" *neovm-renamed*\" \"renamed buffer content\" 22)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-buffer / current-buffer switching patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_set_buffer_current_buffer_switching() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Switch between multiple buffers, track current-buffer identity
    let form = r#"(let ((buf-a (get-buffer-create " *neovm-switch-a*"))
                        (buf-b (get-buffer-create " *neovm-switch-b*")))
  (unwind-protect
      (with-temp-buffer
        (insert "temp-origin")
        (let ((temp-name (buffer-name)))
          ;; Switch to buf-a
          (with-current-buffer buf-a
            (insert "content-a")
            ;; Nested switch to buf-b
            (with-current-buffer buf-b
              (insert "content-b")))
          ;; Back in temp buffer
          (list temp-name
                (buffer-string)
                (with-current-buffer buf-a (buffer-string))
                (with-current-buffer buf-b (buffer-string))
                ;; Verify current-buffer is temp again
                (eq (current-buffer) (current-buffer)))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[
        r#""OK (\" *temp*\" \"temp-origin\" \"content-a\" \"content-b\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-size vs (point-max) vs (length (buffer-string))
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_size_vs_point_max_vs_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // All three should agree in an unrestricted buffer.
    // With narrowing, point-max changes but buffer-size stays the same.
    let form = r#"(with-temp-buffer
  (insert "abcdefghij")
  (let ((sz1 (buffer-size))
        (pm1 (point-max))
        (len1 (length (buffer-string))))
    ;; Narrow to region [3, 8)
    (narrow-to-region 3 8)
    (let ((sz2 (buffer-size))
          (pm2 (point-max))
          (pm-min2 (point-min))
          (len2 (length (buffer-string))))
      (widen)
      (let ((sz3 (buffer-size))
            (pm3 (point-max))
            (len3 (length (buffer-string))))
        (list
         ;; Before narrowing: all equal
         (list sz1 pm1 len1)
         ;; During narrowing: buffer-size = full, point-max = narrowed end
         (list sz2 pm2 pm-min2 len2)
         ;; After widen: restored
         (list sz3 pm3 len3))))))"#;
    let expect = expect_test::expect![r#""OK ((10 11 10) (10 8 3 5) (10 11 10))""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// erase-buffer followed by re-insertion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_then_reinsert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Erase buffer multiple times, reinserting different content each time,
    // verify buffer state is fully reset each time
    let form = r#"(with-temp-buffer
  (insert "first content")
  (let ((snap1 (list (buffer-size) (point) (buffer-string))))
    (erase-buffer)
    (let ((snap2 (list (buffer-size) (point) (point-min) (point-max)
                       (bobp) (eobp))))
      (insert "second")
      (insert " content here")
      (let ((snap3 (list (buffer-size) (point) (buffer-string))))
        (erase-buffer)
        (let ((snap4 (list (buffer-size) (point))))
          ;; Reinsert with newlines
          (insert "line1\nline2\nline3")
          (goto-char (point-min))
          (forward-line 1)
          (let ((snap5 (list (buffer-size) (point)
                             (buffer-substring (point) (line-end-position))
                             (buffer-string))))
            (list snap1 snap2 snap3 snap4 snap5)))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((13 14 \"first content\") (0 1 1 1 t t) (19 20 \"second content here\") (0 1) (17 7 \"line2\" \"line1\\nline2\\nline3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-buffer text processing (copy between buffers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multi_buffer_copy_processing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create source/dest buffers, extract lines matching a pattern from source,
    // transform them, insert into dest
    let form = r#"(let ((src (get-buffer-create " *neovm-src*"))
                        (dst (get-buffer-create " *neovm-dst*")))
  (unwind-protect
      (progn
        ;; Populate source
        (with-current-buffer src
          (insert "ERROR: disk full\n")
          (insert "INFO: started\n")
          (insert "ERROR: timeout\n")
          (insert "DEBUG: trace\n")
          (insert "ERROR: network\n")
          (insert "INFO: done\n"))
        ;; Process: copy ERROR lines to dst, uppercased
        (with-current-buffer src
          (goto-char (point-min))
          (let ((count 0))
            (while (re-search-forward "^ERROR: \\(.+\\)$" nil t)
              (let ((msg (match-string 1)))
                (with-current-buffer dst
                  (insert (upcase msg) "\n"))
                (setq count (1+ count))))
            ;; Return results
            (list count
                  (with-current-buffer dst (buffer-string))
                  (with-current-buffer src (buffer-size))
                  (with-current-buffer dst (buffer-size))))))
    (kill-buffer src)
    (kill-buffer dst)))"#;
    let expect = expect_test::expect![[r#""OK (3 \"DISK FULL\\nTIMEOUT\\nNETWORK\\n\" 85 26)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: buffer-local variable simulation with alist
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_simulation_with_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate buffer-local variables using an alist keyed by buffer name.
    // Each "buffer" gets its own set of variable bindings.
    let form = r#"(let ((buffer-vars nil))
  ;; Helper: set a variable for a buffer
  (fset 'neovm--bvar-set
    (lambda (bufname var val)
      (let ((entry (assoc bufname buffer-vars)))
        (if entry
            (let ((var-entry (assoc var (cdr entry))))
              (if var-entry
                  (setcdr var-entry val)
                (setcdr entry (cons (cons var val) (cdr entry)))))
          (setq buffer-vars
                (cons (cons bufname (list (cons var val)))
                      buffer-vars))))))
  ;; Helper: get a variable for a buffer
  (fset 'neovm--bvar-get
    (lambda (bufname var)
      (let ((entry (assoc bufname buffer-vars)))
        (when entry
          (cdr (assoc var (cdr entry)))))))
  (unwind-protect
      (progn
        ;; Set variables for two "buffers"
        (funcall 'neovm--bvar-set "buf-a" "mode" "text")
        (funcall 'neovm--bvar-set "buf-a" "count" 42)
        (funcall 'neovm--bvar-set "buf-b" "mode" "prog")
        (funcall 'neovm--bvar-set "buf-b" "count" 99)
        (funcall 'neovm--bvar-set "buf-a" "count" 43)
        ;; Query
        (list
         (funcall 'neovm--bvar-get "buf-a" "mode")
         (funcall 'neovm--bvar-get "buf-a" "count")
         (funcall 'neovm--bvar-get "buf-b" "mode")
         (funcall 'neovm--bvar-get "buf-b" "count")
         (funcall 'neovm--bvar-get "buf-c" "mode")
         (length buffer-vars)))
    (fmakunbound 'neovm--bvar-set)
    (fmakunbound 'neovm--bvar-get)))"#;
    let expect = expect_test::expect![[r#""OK (\"text\" 43 \"prog\" 99 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
