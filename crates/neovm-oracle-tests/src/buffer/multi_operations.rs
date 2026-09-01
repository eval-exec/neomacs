//! Oracle parity tests for complex multi-buffer operations.
//!
//! Tests `get-buffer-create`, `set-buffer`, `with-current-buffer`,
//! `buffer-name`, `buffer-list`, `rename-buffer`, `kill-buffer`,
//! buffer-local variables, operations across multiple buffers,
//! and buffer lifecycle patterns.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::assert_oracle_parity;

// ---------------------------------------------------------------------------
// Multi-buffer creation, switching, and content isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_multi_buffer_create_switch_isolate() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create several named buffers, insert different content, switch between
    // them, and verify content isolation.
    let form = r#"(let ((buf-a (get-buffer-create " *neovm-test-buf-a*"))
                        (buf-b (get-buffer-create " *neovm-test-buf-b*"))
                        (buf-c (get-buffer-create " *neovm-test-buf-c*")))
  (unwind-protect
      (progn
        ;; Insert content into each buffer
        (with-current-buffer buf-a
          (erase-buffer)
          (insert "Alpha content line 1\n")
          (insert "Alpha content line 2\n"))
        (with-current-buffer buf-b
          (erase-buffer)
          (insert "Bravo: ")
          (dotimes (i 5)
            (insert (number-to-string (* i i)) " ")))
        (with-current-buffer buf-c
          (erase-buffer)
          (insert "Charlie"))
        ;; Read back and verify isolation
        (let ((a-str (with-current-buffer buf-a (buffer-string)))
              (b-str (with-current-buffer buf-b (buffer-string)))
              (c-str (with-current-buffer buf-c (buffer-string)))
              (a-size (with-current-buffer buf-a (buffer-size)))
              (b-size (with-current-buffer buf-b (buffer-size)))
              (c-size (with-current-buffer buf-c (buffer-size))))
          ;; Modify buf-a and verify b/c unchanged
          (with-current-buffer buf-a
            (goto-char (point-max))
            (insert "Alpha line 3\n"))
          (let ((a-str2 (with-current-buffer buf-a (buffer-string)))
                (b-str2 (with-current-buffer buf-b (buffer-string)))
                (c-str2 (with-current-buffer buf-c (buffer-string))))
            (list
              a-str b-str c-str
              a-size b-size c-size
              ;; b and c should be unchanged
              (string= b-str b-str2)
              (string= c-str c-str2)
              ;; a should have grown
              (> (length a-str2) (length a-str))))))
    (when (buffer-live-p buf-a) (kill-buffer buf-a))
    (when (buffer-live-p buf-b) (kill-buffer buf-b))
    (when (buffer-live-p buf-c) (kill-buffer buf-c))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"Alpha content line 1\\nAlpha content line 2\\n\" \"Bravo: 0 1 4 9 16 \" \"Charlie\" 42 18 7 t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer rename and name uniquification
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_rename_and_uniquify() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create buffers, rename them, check that names update correctly,
    // and that get-buffer finds them by new name.
    let form = r#"(let ((buf1 (get-buffer-create " *neovm-rename-test-1*"))
                        (buf2 (get-buffer-create " *neovm-rename-test-2*")))
  (unwind-protect
      (progn
        (let ((name1-before (buffer-name buf1))
              (name2-before (buffer-name buf2)))
          ;; Rename buf1
          (with-current-buffer buf1
            (rename-buffer " *neovm-renamed-alpha*"))
          (let ((name1-after (buffer-name buf1))
                ;; buf2 name should be unchanged
                (name2-still (buffer-name buf2)))
            ;; get-buffer should find by new name
            (let ((found-by-new (get-buffer " *neovm-renamed-alpha*"))
                  ;; Old name should not find anything
                  (found-by-old (get-buffer " *neovm-rename-test-1*")))
              ;; Rename buf2 with unique flag
              (with-current-buffer buf2
                (rename-buffer " *neovm-renamed-alpha*" t))
              (let ((name2-after (buffer-name buf2)))
                (list
                  name1-before name2-before
                  name1-after name2-still
                  (eq found-by-new buf1)
                  (null found-by-old)
                  ;; buf2 should have gotten a uniquified name
                  ;; (not the same as buf1's name)
                  (not (string= name1-after name2-after))
                  ;; Both should still be live
                  (buffer-live-p buf1)
                  (buffer-live-p buf2)))))))
    (when (buffer-live-p buf1) (kill-buffer buf1))
    (when (buffer-live-p buf2) (kill-buffer buf2))))"#;
    let expect = expect_test::expect![[
        r#""OK (\" *neovm-rename-test-1*\" \" *neovm-rename-test-2*\" \" *neovm-renamed-alpha*\" \" *neovm-rename-test-2*\" t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer kill and buffer-live-p lifecycle
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_kill_lifecycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create buffers, verify they are live, kill some, check dead status,
    // and verify killed buffers vanish from buffer-list.
    let form = r#"(let ((b1 (get-buffer-create " *neovm-lifecycle-1*"))
                        (b2 (get-buffer-create " *neovm-lifecycle-2*"))
                        (b3 (get-buffer-create " *neovm-lifecycle-3*")))
  (unwind-protect
      (progn
        ;; All should be live
        (let ((all-live (list (buffer-live-p b1) (buffer-live-p b2) (buffer-live-p b3))))
          ;; Insert content
          (with-current-buffer b1 (erase-buffer) (insert "content-1"))
          (with-current-buffer b2 (erase-buffer) (insert "content-2"))
          (with-current-buffer b3 (erase-buffer) (insert "content-3"))
          ;; Kill b2
          (kill-buffer b2)
          (let ((after-kill (list (buffer-live-p b1) (buffer-live-p b2) (buffer-live-p b3)))
                ;; b2 should not be in buffer-list
                (b2-in-list (memq b2 (buffer-list)))
                ;; b1 and b3 should still be in buffer-list
                (b1-in-list (not (null (memq b1 (buffer-list)))))
                (b3-in-list (not (null (memq b3 (buffer-list))))))
            ;; b1 and b3 content should be fine
            (let ((b1-str (with-current-buffer b1 (buffer-string)))
                  (b3-str (with-current-buffer b3 (buffer-string))))
              ;; Kill b3
              (kill-buffer b3)
              (list
                all-live
                after-kill
                (null b2-in-list)
                b1-in-list
                b1-str b3-str
                (buffer-live-p b1)
                (buffer-live-p b3))))))
    (when (buffer-live-p b1) (kill-buffer b1))
    (when (buffer-live-p b2) (kill-buffer b2))
    (when (buffer-live-p b3) (kill-buffer b3))))"#;
    let expect =
        expect_test::expect![[r#""OK ((t t t) (t nil t) t t \"content-1\" \"content-3\" t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local variables across multiple buffers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_variables() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use make-local-variable to create buffer-local bindings,
    // verify they are independent per buffer.
    let form = r#"(progn
  (defvar neovm--bmo-test-var 'global-default)
  (let ((buf-x (get-buffer-create " *neovm-local-x*"))
        (buf-y (get-buffer-create " *neovm-local-y*")))
    (unwind-protect
        (progn
          ;; Set buffer-local values
          (with-current-buffer buf-x
            (make-local-variable 'neovm--bmo-test-var)
            (setq neovm--bmo-test-var 'x-value))
          (with-current-buffer buf-y
            (make-local-variable 'neovm--bmo-test-var)
            (setq neovm--bmo-test-var 'y-value))
          ;; Read values from each buffer
          (let ((x-val (with-current-buffer buf-x neovm--bmo-test-var))
                (y-val (with-current-buffer buf-y neovm--bmo-test-var))
                ;; Default in a fresh temp buffer
                (default-val (with-temp-buffer neovm--bmo-test-var)))
            ;; Modify x, verify y unchanged
            (with-current-buffer buf-x
              (setq neovm--bmo-test-var 'x-modified))
            (let ((x-val2 (with-current-buffer buf-x neovm--bmo-test-var))
                  (y-val2 (with-current-buffer buf-y neovm--bmo-test-var)))
              ;; Check local-variable-p
              (let ((x-local (with-current-buffer buf-x
                               (local-variable-p 'neovm--bmo-test-var)))
                    (temp-local (with-temp-buffer
                                  (local-variable-p 'neovm--bmo-test-var))))
                (list
                  x-val y-val default-val
                  x-val2 y-val2
                  x-local temp-local)))))
      (when (buffer-live-p buf-x) (kill-buffer buf-x))
      (when (buffer-live-p buf-y) (kill-buffer buf-y))
      (makunbound 'neovm--bmo-test-var))))"#;
    let expect =
        expect_test::expect![r#""OK (x-value y-value global-default x-modified y-value t nil)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cross-buffer text aggregation pipeline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_cross_buffer_text_aggregation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create multiple source buffers, each with lines of data.
    // Aggregate all lines into a destination buffer, sort them, deduplicate,
    // and produce a summary.
    let form = r#"(let ((src1 (get-buffer-create " *neovm-agg-src1*"))
                        (src2 (get-buffer-create " *neovm-agg-src2*"))
                        (dest (get-buffer-create " *neovm-agg-dest*")))
  (unwind-protect
      (progn
        ;; Populate source buffers
        (with-current-buffer src1
          (erase-buffer)
          (insert "cherry\napple\nbanana\napple\n"))
        (with-current-buffer src2
          (erase-buffer)
          (insert "banana\ndate\ncherry\nfig\n"))
        ;; Collect all lines from src buffers into dest
        (with-current-buffer dest
          (erase-buffer))
        (dolist (src (list src1 src2))
          (with-current-buffer src
            (goto-char (point-min))
            (while (not (eobp))
              (let ((line-start (point)))
                (end-of-line)
                (let ((line (buffer-substring-no-properties line-start (point))))
                  (when (> (length line) 0)
                    (with-current-buffer dest
                      (goto-char (point-max))
                      (insert line "\n"))))
                (forward-line 1)))))
        ;; Now read all lines from dest, sort and deduplicate
        (let ((all-lines nil))
          (with-current-buffer dest
            (goto-char (point-min))
            (while (not (eobp))
              (let ((ls (point)))
                (end-of-line)
                (let ((l (buffer-substring-no-properties ls (point))))
                  (when (> (length l) 0)
                    (setq all-lines (cons l all-lines))))
                (forward-line 1))))
          (let* ((sorted (sort all-lines #'string<))
                 ;; Deduplicate
                 (deduped nil)
                 (prev nil))
            (dolist (item sorted)
              (unless (equal item prev)
                (setq deduped (cons item deduped))
                (setq prev item)))
            (let ((unique (nreverse deduped)))
              (list
                (length all-lines)
                (length unique)
                unique)))))
    (when (buffer-live-p src1) (kill-buffer src1))
    (when (buffer-live-p src2) (kill-buffer src2))
    (when (buffer-live-p dest) (kill-buffer dest))))"#;
    let expect =
        expect_test::expect![[r#""OK (8 5 (\"apple\" \"banana\" \"cherry\" \"date\" \"fig\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer nesting and save-excursion interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_nested_with_current_buffer_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deeply nest with-current-buffer and save-excursion to verify
    // that point, buffer context, and restriction are all properly
    // saved and restored.
    let form = r#"(let ((buf-p (get-buffer-create " *neovm-nest-p*"))
                        (buf-q (get-buffer-create " *neovm-nest-q*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-p
          (erase-buffer) (insert "PPPPPPPPPP"))
        (with-current-buffer buf-q
          (erase-buffer) (insert "QQQQQQQQQQ"))
        ;; Start in buf-p at position 5
        (with-current-buffer buf-p
          (goto-char 5)
          (let ((p-point-before (point))
                (p-buf-before (buffer-name (current-buffer))))
            ;; Switch to buf-q, move to position 3
            (let ((result-from-q
                   (with-current-buffer buf-q
                     (goto-char 3)
                     (let ((q-point (point))
                           (q-buf (buffer-name (current-buffer))))
                       ;; Nest back into buf-p via save-excursion
                       (let ((inner-p-result
                              (save-excursion
                                (with-current-buffer buf-p
                                  (goto-char 8)
                                  (list (point) (buffer-name (current-buffer)))))))
                         ;; After save-excursion, we should still be in q at pos 3
                         (list q-point q-buf inner-p-result
                               (point) (buffer-name (current-buffer))))))))
              ;; After with-current-buffer buf-q, we should be back in buf-p
              (let ((p-point-after (point))
                    (p-buf-after (buffer-name (current-buffer))))
                (list
                  p-point-before p-buf-before
                  result-from-q
                  p-point-after p-buf-after))))))
    (when (buffer-live-p buf-p) (kill-buffer buf-p))
    (when (buffer-live-p buf-q) (kill-buffer buf-q))))"#;
    let expect = expect_test::expect![[
        r#""OK (5 \" *neovm-nest-p*\" (3 \" *neovm-nest-q*\" (8 \" *neovm-nest-p*\") 3 \" *neovm-nest-q*\") 8 \" *neovm-nest-p*\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local variable default value and kill-local-variable
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_local_default_and_kill_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that killing a buffer-local variable reverts to default,
    // and make-variable-buffer-local vs make-local-variable semantics.
    let form = r#"(progn
  (defvar neovm--bmo-kill-var 'the-default)
  (let ((buf (get-buffer-create " *neovm-kill-local-test*")))
    (unwind-protect
        (progn
          ;; Set a buffer-local override
          (with-current-buffer buf
            (make-local-variable 'neovm--bmo-kill-var)
            (setq neovm--bmo-kill-var 'overridden))
          (let ((val-local (with-current-buffer buf neovm--bmo-kill-var))
                (val-default (default-value 'neovm--bmo-kill-var)))
            ;; Kill the local variable
            (with-current-buffer buf
              (kill-local-variable 'neovm--bmo-kill-var))
            (let ((val-after-kill (with-current-buffer buf neovm--bmo-kill-var))
                  (still-local (with-current-buffer buf
                                 (local-variable-p 'neovm--bmo-kill-var))))
              ;; Re-create local
              (with-current-buffer buf
                (make-local-variable 'neovm--bmo-kill-var)
                (setq neovm--bmo-kill-var 'second-override))
              (let ((val-recreated (with-current-buffer buf neovm--bmo-kill-var)))
                (list
                  val-local
                  val-default
                  val-after-kill
                  still-local
                  val-recreated
                  ;; Default unchanged throughout
                  (default-value 'neovm--bmo-kill-var))))))
      (when (buffer-live-p buf) (kill-buffer buf))
      (makunbound 'neovm--bmo-kill-var))))"#;
    let expect = expect_test::expect![
        r#""OK (overridden the-default the-default nil second-override the-default)""#
    ];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer list filtering and batch operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_list_batch_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a set of buffers with a naming convention, use buffer-list
    // to find them, perform batch operations, and verify results.
    let form = r#"(let ((bufs nil))
  (unwind-protect
      (progn
        ;; Create 5 named buffers with prefix
        (dotimes (i 5)
          (let ((b (get-buffer-create
                     (format " *neovm-batch-%d*" i))))
            (setq bufs (cons b bufs))
            (with-current-buffer b
              (erase-buffer)
              (dotimes (j (1+ i))
                (insert (format "line-%d\n" j))))))
        (setq bufs (nreverse bufs))
        ;; Gather info from all our buffers
        (let ((info
               (mapcar (lambda (b)
                         (with-current-buffer b
                           (list (buffer-name b)
                                 (buffer-size)
                                 (count-lines (point-min) (point-max)))))
                       bufs)))
          ;; Compute total size and total lines
          (let ((total-size 0) (total-lines 0))
            (dolist (entry info)
              (setq total-size (+ total-size (nth 1 entry)))
              (setq total-lines (+ total-lines (nth 2 entry))))
            ;; Erase the first two buffers
            (with-current-buffer (nth 0 bufs) (erase-buffer))
            (with-current-buffer (nth 1 bufs) (erase-buffer))
            (let ((sizes-after (mapcar (lambda (b)
                                         (with-current-buffer b (buffer-size)))
                                       bufs)))
              (list
                info
                total-size
                total-lines
                sizes-after)))))
    (dolist (b bufs)
      (when (buffer-live-p b) (kill-buffer b)))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\" *neovm-batch-0*\" 7 1) (\" *neovm-batch-1*\" 14 2) (\" *neovm-batch-2*\" 21 3) (\" *neovm-batch-3*\" 28 4) (\" *neovm-batch-4*\" 35 5)) 105 15 (0 0 21 28 35))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
