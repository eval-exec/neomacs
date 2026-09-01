//! Oracle parity tests for comprehensive `with-current-buffer` patterns:
//! preserving/restoring current buffer, nested calls, interaction with
//! `set-buffer`, `with-temp-buffer` inside, buffer-local variable access,
//! point/mark preservation, narrowing context, error handling within
//! buffer switch, generated buffer names, buffer state isolation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Preserving and restoring current buffer across with-current-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_preserve_restore_current_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcbc-a*"))
      (buf-b (generate-new-buffer " *neovm-wcbc-b*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-a (insert "AAA"))
        (with-current-buffer buf-b (insert "BBB"))
        (let ((orig (current-buffer)))
          ;; Switch to buf-a, verify current buffer inside
          (let ((inside-a (with-current-buffer buf-a (current-buffer)))
                (after-a (current-buffer)))
            ;; Switch to buf-b, verify restoration
            (let ((inside-b (with-current-buffer buf-b (current-buffer)))
                  (after-b (current-buffer)))
              (list
               ;; Inside with-current-buffer, current-buffer is the target
               (eq inside-a buf-a)
               (eq inside-b buf-b)
               ;; After with-current-buffer, we return to original
               (eq after-a orig)
               (eq after-b orig)
               ;; Return value is the body's last expression
               (with-current-buffer buf-a (buffer-string))
               (with-current-buffer buf-b (buffer-string)))))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[r#""OK (t t t t \"AAA\" \"BBB\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Deeply nested with-current-buffer: 4 levels
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_deeply_nested_four_levels() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((b1 (generate-new-buffer " *neovm-wcbc-n1*"))
      (b2 (generate-new-buffer " *neovm-wcbc-n2*"))
      (b3 (generate-new-buffer " *neovm-wcbc-n3*"))
      (b4 (generate-new-buffer " *neovm-wcbc-n4*")))
  (unwind-protect
      (progn
        (with-current-buffer b1 (insert "L1"))
        (with-current-buffer b2 (insert "L2"))
        (with-current-buffer b3 (insert "L3"))
        (with-current-buffer b4 (insert "L4"))
        ;; Nest 4 levels deep, reading text from each
        (with-current-buffer b1
          (let ((t1 (buffer-string)))
            (with-current-buffer b2
              (let ((t2 (buffer-string)))
                (with-current-buffer b3
                  (let ((t3 (buffer-string)))
                    (with-current-buffer b4
                      (let ((t4 (buffer-string)))
                        ;; Build result from innermost
                        (list t1 t2 t3 t4
                              ;; Verify we are in b4
                              (eq (current-buffer) b4))))))
                ;; Back in b2
                (eq (current-buffer) b2)))
            ;; Back in b1
            (eq (current-buffer) b1))))
    (kill-buffer b1) (kill-buffer b2)
    (kill-buffer b3) (kill-buffer b4)))"#;
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with set-buffer inside with-current-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_interaction_with_set_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcbc-sb-a*"))
      (buf-b (generate-new-buffer " *neovm-wcbc-sb-b*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-a (insert "aaa"))
        (with-current-buffer buf-b (insert "bbb"))
        (let ((orig (current-buffer)))
          ;; Inside with-current-buffer, use set-buffer to switch away
          ;; with-current-buffer still restores the original on exit
          (let ((result
                 (with-current-buffer buf-a
                   (let ((before (current-buffer)))
                     (set-buffer buf-b)
                     (let ((during (current-buffer)))
                       (list (eq before buf-a)
                             (eq during buf-b)
                             (buffer-string)))))))
            ;; After with-current-buffer, we are back at orig
            ;; (not buf-b, even though set-buffer was called)
            (list result
                  (eq (current-buffer) orig)))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[r#""OK ((t t \"bbb\") t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-temp-buffer inside with-current-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_with_temp_buffer_inside() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcbc-outer*")))
  (unwind-protect
      (progn
        (with-current-buffer buf (insert "outer-content"))
        (let ((orig (current-buffer)))
          (with-current-buffer buf
            ;; Inside buf, create a temp buffer
            (let ((outer-text (buffer-string)))
              (let ((temp-result
                     (with-temp-buffer
                       (insert "temp-content")
                       (let ((inner-text (buffer-string)))
                         (list inner-text
                               ;; We are NOT in buf anymore
                               (eq (current-buffer) buf))))))
                ;; Back in buf after with-temp-buffer
                (list outer-text
                      temp-result
                      (eq (current-buffer) buf)
                      (buffer-string)
                      ;; Original is preserved after everything
                      ))))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"outer-content\" (\"temp-content\" nil) t \"outer-content\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer-local variable access across with-current-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_buffer_local_variable_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcbc-bl-a*"))
      (buf-b (generate-new-buffer " *neovm-wcbc-bl-b*")))
  (unwind-protect
      (progn
        ;; Set buffer-local variable in each buffer
        (with-current-buffer buf-a
          (setq-local neovm--test-var-wcbc 'value-a)
          (setq-local neovm--test-count-wcbc 100))
        (with-current-buffer buf-b
          (setq-local neovm--test-var-wcbc 'value-b)
          (setq-local neovm--test-count-wcbc 200))
        (list
         ;; Read buffer-local from outside via with-current-buffer
         (with-current-buffer buf-a neovm--test-var-wcbc)
         (with-current-buffer buf-b neovm--test-var-wcbc)
         (with-current-buffer buf-a neovm--test-count-wcbc)
         (with-current-buffer buf-b neovm--test-count-wcbc)
         ;; Modify in one buffer, verify other is unchanged
         (progn
           (with-current-buffer buf-a
             (setq neovm--test-count-wcbc 999))
           (list
            (with-current-buffer buf-a neovm--test-count-wcbc)
            (with-current-buffer buf-b neovm--test-count-wcbc)))
         ;; local-variable-p
         (with-current-buffer buf-a
           (local-variable-p 'neovm--test-var-wcbc))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[r#""OK (value-a value-b 100 200 (999 200) t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point and mark preservation across buffer switches
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_point_mark_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcbc-pm-a*"))
      (buf-b (generate-new-buffer " *neovm-wcbc-pm-b*")))
  (unwind-protect
      (progn
        ;; Setup: insert text and position point/mark in each buffer
        (with-current-buffer buf-a
          (insert "Hello World AAAA")
          (goto-char 6)
          (push-mark 12 t t))
        (with-current-buffer buf-b
          (insert "Goodbye World BBBB")
          (goto-char 8)
          (push-mark 14 t t))
        (list
         ;; Point is preserved per-buffer
         (with-current-buffer buf-a (point))
         (with-current-buffer buf-b (point))
         ;; Mark is preserved per-buffer
         (with-current-buffer buf-a (mark t))
         (with-current-buffer buf-b (mark t))
         ;; Move point in buf-a, doesn't affect buf-b
         (progn
           (with-current-buffer buf-a (goto-char 1))
           (list (with-current-buffer buf-a (point))
                 (with-current-buffer buf-b (point))))
         ;; Region text via point and mark
         (with-current-buffer buf-a
           (goto-char 1)
           (buffer-substring (point-min) 5))
         (with-current-buffer buf-b
           (buffer-substring 1 8))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[r#""OK (6 8 12 14 (1 8) \"Hell\" \"Goodbye\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing context: independent per buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_narrowing_context_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcbc-nr-a*"))
      (buf-b (generate-new-buffer " *neovm-wcbc-nr-b*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-a
          (insert "0123456789ABCDEF")
          ;; Narrow to middle portion
          (narrow-to-region 5 13))
        (with-current-buffer buf-b
          (insert "abcdefghijklmnop")
          ;; Narrow differently
          (narrow-to-region 3 8))
        (list
         ;; Each buffer sees only its narrowed region
         (with-current-buffer buf-a (buffer-string))
         (with-current-buffer buf-b (buffer-string))
         ;; point-min/point-max reflect narrowing
         (with-current-buffer buf-a (list (point-min) (point-max)))
         (with-current-buffer buf-b (list (point-min) (point-max)))
         ;; Widen one buffer, other stays narrowed
         (progn
           (with-current-buffer buf-a (widen))
           (list
            (with-current-buffer buf-a (buffer-string))
            (with-current-buffer buf-b (buffer-string))))
         ;; save-restriction within with-current-buffer
         (with-current-buffer buf-b
           (save-restriction
             (widen)
             (let ((wide (buffer-string)))
               (list wide (point-min) (point-max)))))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"456789AB\" \"cdefg\" (5 13) (3 8) (\"0123456789ABCDEF\" \"cdefg\") (\"abcdefghijklmnop\" 1 17))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling within with-current-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_error_handling_restores_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcbc-err*")))
  (unwind-protect
      (let ((orig (current-buffer)))
        (list
         ;; Error inside with-current-buffer: buffer is still restored
         (condition-case err
             (with-current-buffer buf
               (insert "before-error")
               (error "deliberate error")
               (insert "after-error"))  ;; never reached
           (error (list 'caught (cadr err))))
         ;; We are back to the original buffer
         (eq (current-buffer) orig)
         ;; The buffer retains what was inserted before the error
         (with-current-buffer buf (buffer-string))
         ;; Nested error: outer with-current-buffer still restores
         (let ((buf2 (generate-new-buffer " *neovm-wcbc-err2*")))
           (unwind-protect
               (condition-case nil
                   (with-current-buffer buf
                     (with-current-buffer buf2
                       (insert "inner")
                       (error "inner error")))
                 (error
                  (list (eq (current-buffer) orig)
                        (with-current-buffer buf2 (buffer-string)))))
             (kill-buffer buf2)))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught \"deliberate error\") t \"before-error\" (t \"inner\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Generated buffer names and uniqueness
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_generated_buffer_names() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((bufs nil))
  (unwind-protect
      (progn
        ;; Generate several buffers with same base name
        (dotimes (i 5)
          (setq bufs (cons (generate-new-buffer " *neovm-wcbc-gen*") bufs)))
        (setq bufs (nreverse bufs))
        ;; All buffer names are distinct
        (let ((names (mapcar #'buffer-name bufs)))
          (list
           ;; All are strings
           (cl-every #'stringp names)
           ;; Length is 5
           (length names)
           ;; Use with-current-buffer on each, insert index
           (progn
             (let ((i 0))
               (dolist (b bufs)
                 (with-current-buffer b
                   (insert (format "buf-%d" i)))
                 (setq i (1+ i))))
             ;; Read back from each
             (mapcar (lambda (b) (with-current-buffer b (buffer-string))) bufs))
           ;; No two names are equal
           (let ((all-unique t))
             (dotimes (i (length names))
               (dotimes (j (length names))
                 (when (and (/= i j) (string= (nth i names) (nth j names)))
                   (setq all-unique nil))))
             all-unique))))
    (dolist (b bufs) (when (buffer-live-p b) (kill-buffer b)))))"#;
    let expect = expect_test::expect![[r#""ERR (void-function cl-every)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer state isolation: insert in one doesn't affect another
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_buffer_state_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((b1 (generate-new-buffer " *neovm-wcbc-iso1*"))
      (b2 (generate-new-buffer " *neovm-wcbc-iso2*")))
  (unwind-protect
      (progn
        ;; Insert in b1
        (with-current-buffer b1 (insert "one"))
        ;; b2 is still empty
        (list
         (with-current-buffer b2 (buffer-string))
         (with-current-buffer b1 (buffer-string))
         ;; Insert in b2
         (progn
           (with-current-buffer b2 (insert "two"))
           (list
            (with-current-buffer b1 (buffer-string))
            (with-current-buffer b2 (buffer-string))))
         ;; Erase b1, b2 unchanged
         (progn
           (with-current-buffer b1 (erase-buffer))
           (list
            (with-current-buffer b1 (buffer-string))
            (with-current-buffer b2 (buffer-string))))
         ;; buffer-size
         (list
          (with-current-buffer b1 (buffer-size))
          (with-current-buffer b2 (buffer-size)))
         ;; buffer-modified-p isolation
         (progn
           (with-current-buffer b2 (set-buffer-modified-p nil))
           (with-current-buffer b1 (insert "new"))
           (list
            (with-current-buffer b1 (buffer-modified-p))
            (with-current-buffer b2 (buffer-modified-p))))))
    (kill-buffer b1)
    (kill-buffer b2)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"\" \"one\" (\"one\" \"two\") (\"\" \"two\") (0 3) (t nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer with save-excursion interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_wcb_comp_save_excursion_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcbc-se*")))
  (unwind-protect
      (progn
        (with-current-buffer buf (insert "abcdefghij"))
        (let ((orig (current-buffer)))
          (list
           ;; save-excursion inside with-current-buffer
           (with-current-buffer buf
             (goto-char 3)
             (let ((p1 (point)))
               (save-excursion
                 (goto-char 7)
                 (let ((p2 (point)))
                   (list p1 p2)))
               ;; Point is restored
               (point)))
           ;; with-current-buffer inside save-excursion
           (save-excursion
             (let ((inner-result (with-current-buffer buf
                                   (goto-char 5)
                                   (buffer-substring (point) (point-max)))))
               (list inner-result (eq (current-buffer) orig))))
           ;; Nested save-excursion + with-current-buffer combo
           (save-excursion
             (with-current-buffer buf
               (save-excursion
                 (goto-char 1)
                 (buffer-substring 1 4)))))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[r#""OK (3 (\"efghij\" t) \"abc\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
