//! Oracle parity tests for `with-current-buffer` with complex patterns:
//! basic switching, nested calls, buffer-local variables, string vs buffer
//! object, cross-buffer text copying, multi-buffer state management,
//! interaction with save-excursion, and error handling.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic: switch to a named buffer, operate, return to original
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_basic_switch() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create a temp buffer, insert text, use with-current-buffer to read it
    // from the original buffer context, verify we return to the original.
    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcb-test-1*")))
  (unwind-protect
      (progn
        ;; Insert text into the new buffer
        (with-current-buffer buf
          (insert "hello from buf"))
        ;; From original context, read text from buf
        (let ((text (with-current-buffer buf (buffer-string))))
          ;; Verify current buffer is NOT buf
          (list text
                (eq (current-buffer) buf)
                (buffer-name buf))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[r#""OK (\"hello from buf\" nil \" *neovm-wcb-test-1*\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer using buffer name (string) vs buffer object
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_string_vs_object() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // with-current-buffer accepts both a buffer object and a buffer name string
    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcb-test-2*")))
  (unwind-protect
      (progn
        (with-current-buffer buf
          (insert "content-A"))
        ;; Access via buffer object
        (let ((via-obj (with-current-buffer buf (buffer-string))))
          ;; Access via buffer name string
          (let ((via-str (with-current-buffer (buffer-name buf) (buffer-string))))
            (list via-obj via-str (string= via-obj via-str)))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[r#""OK (\"content-A\" \"content-A\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested with-current-buffer calls
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three buffers: A, B, C. Nested with-current-buffer switching between them.
    let form = r#"(let ((buf-a (generate-new-buffer " *neovm-wcb-a*"))
      (buf-b (generate-new-buffer " *neovm-wcb-b*"))
      (buf-c (generate-new-buffer " *neovm-wcb-c*")))
  (unwind-protect
      (progn
        (with-current-buffer buf-a (insert "AAA"))
        (with-current-buffer buf-b (insert "BBB"))
        (with-current-buffer buf-c (insert "CCC"))
        ;; Nested: from original, enter A, then B, then C, read all
        (with-current-buffer buf-a
          (let ((a-text (buffer-string)))
            (with-current-buffer buf-b
              (let ((b-text (buffer-string)))
                (with-current-buffer buf-c
                  (let ((c-text (buffer-string)))
                    ;; Inside C, verify current-buffer
                    (let ((in-c (eq (current-buffer) buf-c)))
                      (list a-text b-text c-text in-c)))))))))
    (kill-buffer buf-a)
    (kill-buffer buf-b)
    (kill-buffer buf-c)))"#;
    let expect = expect_test::expect![[r#""OK (\"AAA\" \"BBB\" \"CCC\" t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with buffer-local variables
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_buffer_local_vars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Buffer-local variables should be visible inside with-current-buffer
    let form = r#"(let ((buf-x (generate-new-buffer " *neovm-wcb-local-x*"))
      (buf-y (generate-new-buffer " *neovm-wcb-local-y*")))
  (unwind-protect
      (progn
        ;; Set buffer-local variable in each buffer
        (with-current-buffer buf-x
          (setq-local neovm--wcb-local-test 'value-x)
          (insert "X content"))
        (with-current-buffer buf-y
          (setq-local neovm--wcb-local-test 'value-y)
          (insert "Y content"))
        ;; Read buffer-local from each buffer
        (let ((x-val (with-current-buffer buf-x
                       (list (buffer-local-value 'neovm--wcb-local-test (current-buffer))
                             (buffer-string))))
              (y-val (with-current-buffer buf-y
                       (list (buffer-local-value 'neovm--wcb-local-test (current-buffer))
                             (buffer-string)))))
          (list x-val y-val)))
    (kill-buffer buf-x)
    (kill-buffer buf-y)))"#;
    let expect =
        expect_test::expect![[r#""OK ((value-x \"X content\") (value-y \"Y content\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: cross-buffer text copying
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_cross_buffer_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Copy text from one buffer to another using with-current-buffer
    let form = r#"(let ((src (generate-new-buffer " *neovm-wcb-src*"))
      (dst (generate-new-buffer " *neovm-wcb-dst*")))
  (unwind-protect
      (progn
        ;; Populate source buffer with multi-line text
        (with-current-buffer src
          (insert "line-1\nline-2\nline-3\nline-4\nline-5"))
        ;; Copy lines 2-4 from src to dst
        (let ((extracted (with-current-buffer src
                           (goto-char (point-min))
                           (forward-line 1)
                           (let ((start (point)))
                             (forward-line 3)
                             (buffer-substring start (point))))))
          (with-current-buffer dst
            (insert extracted)))
        ;; Verify dst content and src unchanged
        (list (with-current-buffer dst (buffer-string))
              (with-current-buffer src (buffer-string))
              (with-current-buffer src
                (goto-char (point-min))
                (let ((count 0))
                  (while (not (eobp))
                    (forward-line 1)
                    (setq count (1+ count)))
                  count))))
    (kill-buffer src)
    (kill-buffer dst)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"line-2\\nline-3\\nline-4\\n\" \"line-1\\nline-2\\nline-3\\nline-4\\nline-5\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: multi-buffer state management with accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_multi_buffer_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create multiple buffers, insert numbered content, then aggregate
    let form = r#"(let ((bufs nil)
      (names '(" *neovm-ms-1*" " *neovm-ms-2*" " *neovm-ms-3*" " *neovm-ms-4*")))
  (unwind-protect
      (progn
        ;; Create and populate buffers
        (let ((i 1))
          (dolist (name names)
            (let ((b (generate-new-buffer name)))
              (setq bufs (cons b bufs))
              (with-current-buffer b
                (insert (format "Buffer %d has %d items" i (* i 10)))
                (goto-char (point-min)))
              (setq i (1+ i)))))
        (setq bufs (nreverse bufs))
        ;; Collect all buffer contents into a list
        (let ((contents nil))
          (dolist (b bufs)
            (setq contents
                  (cons (with-current-buffer b
                          (list (buffer-name)
                                (buffer-string)
                                (buffer-size)
                                (point)))
                        contents)))
          ;; Return in order
          (nreverse contents)))
    (dolist (b bufs)
      (when (buffer-live-p b) (kill-buffer b)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\" *neovm-ms-1*\" \"Buffer 1 has 10 items\" 21 1) (\" *neovm-ms-2*\" \"Buffer 2 has 20 items\" 21 1) (\" *neovm-ms-3*\" \"Buffer 3 has 30 items\" 21 1) (\" *neovm-ms-4*\" \"Buffer 4 has 40 items\" 21 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: with-current-buffer inside save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_inside_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion should preserve point and buffer around with-current-buffer
    let form = r#"(with-temp-buffer
  (insert "original buffer text here")
  (goto-char 9)
  (let ((original-point (point))
        (original-buf (current-buffer)))
    (let ((buf2 (generate-new-buffer " *neovm-wcb-se*")))
      (unwind-protect
          (progn
            (with-current-buffer buf2
              (insert "secondary buffer"))
            ;; save-excursion around with-current-buffer
            (save-excursion
              (goto-char (point-max))  ;; move point in original
              (with-current-buffer buf2
                (goto-char (point-min))
                (insert "PREFIX: ")
                (buffer-string)))
            ;; After save-excursion, point should be restored
            (list (point)
                  (= (point) original-point)
                  (eq (current-buffer) original-buf)
                  (with-current-buffer buf2 (buffer-string))
                  (buffer-string)))
        (kill-buffer buf2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (9 t t \"PREFIX: secondary buffer\" \"original buffer text here\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer return value and side effects
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_return_and_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // with-current-buffer should return the value of the last form in body
    // and any modifications should persist in the target buffer
    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcb-ret*")))
  (unwind-protect
      (let ((retval (with-current-buffer buf
                      (insert "hello")
                      (goto-char (point-min))
                      (insert "say ")
                      ;; Return a computed value, not the buffer content
                      (+ 10 20 12))))
        ;; retval should be 42, and buffer should have modified content
        (list retval
              (with-current-buffer buf (buffer-string))
              (with-current-buffer buf (point-min))
              (with-current-buffer buf (point-max))
              (with-current-buffer buf (buffer-size))))
    (kill-buffer buf)))"#;
    let expect = expect_test::expect![[r#""OK (42 \"say hello\" 1 10 9)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer with multiple body forms
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_multiple_body_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multiple forms in body, last one determines return value
    let form = r#"(let ((buf (generate-new-buffer " *neovm-wcb-multi*"))
      (log nil))
  (unwind-protect
      (progn
        (let ((result (with-current-buffer buf
                        (setq log (cons 'step1 log))
                        (insert "first")
                        (setq log (cons 'step2 log))
                        (insert " second")
                        (setq log (cons 'step3 log))
                        (buffer-string))))
          (list result (nreverse log)
                (with-current-buffer buf (buffer-string)))))
    (kill-buffer buf)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"first second\" (step1 step2 step3) \"first second\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-current-buffer error propagation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_current_buffer_error_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // If an error occurs inside with-current-buffer, it should propagate
    // and the current buffer should be restored
    let form = r#"(with-temp-buffer
  (insert "main buffer")
  (let ((buf (generate-new-buffer " *neovm-wcb-err*"))
        (original-buf (current-buffer)))
    (unwind-protect
        (let ((caught (condition-case err
                          (with-current-buffer buf
                            (insert "some text")
                            (/ 1 0)
                            (insert "unreachable"))
                        (arith-error
                         (list 'caught
                               ;; After error, current-buffer should be restored
                               (eq (current-buffer) original-buf)
                               (car err))))))
          (list caught
                ;; The text before the error should still be in buf
                (with-current-buffer buf (buffer-string))
                ;; Main buffer should be unchanged
                (buffer-string)))
      (kill-buffer buf))))"#;
    let expect =
        expect_test::expect![[r#""OK ((caught t arith-error) \"some text\" \"main buffer\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
