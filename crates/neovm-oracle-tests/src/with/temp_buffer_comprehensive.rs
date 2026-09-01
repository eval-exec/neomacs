//! Oracle parity tests for comprehensive `with-temp-buffer` usage:
//! basic insert/extract, multiple buffer operations, nested with-temp-buffer
//! (independent buffers), complex return values, buffer state isolation,
//! error handling inside temp buffers, narrowing, and text properties.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic insert and extract pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_insert_extract_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert multiple lines, extract substrings from different positions,
    // and verify point movement.
    let form = r####"(with-temp-buffer
  (insert "first line\n")
  (insert "second line\n")
  (insert "third line")
  (let ((full (buffer-string))
        (size (buffer-size)))
    (goto-char (point-min))
    (forward-line 1)
    (let ((line2-start (point)))
      (end-of-line)
      (let ((line2-end (point)))
        (list full size
              (buffer-substring line2-start line2-end)
              line2-start line2-end)))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"first line\\nsecond line\\nthird line\" 33 \"second line\" 12 23)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple buffer operations within with-temp-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_multiple_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform insert, delete, replace, and case operations all within
    // a single with-temp-buffer.
    let form = r####"(with-temp-buffer
  (insert "Hello, World! This is a test.")
  ;; Delete ", World"
  (goto-char 6)
  (delete-region 6 13)
  ;; Now buffer is "Hello! This is a test."
  (let ((after-delete (buffer-string)))
    ;; Upcase the first word
    (goto-char (point-min))
    (upcase-region 1 6)
    (let ((after-upcase (buffer-string)))
      ;; Insert at beginning
      (goto-char (point-min))
      (insert ">>> ")
      (let ((after-prefix (buffer-string)))
        ;; Insert at end
        (goto-char (point-max))
        (insert " <<<")
        (list after-delete after-upcase after-prefix (buffer-string))))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"Hello! This is a test.\" \"HELLO! This is a test.\" \">>> HELLO! This is a test.\" \">>> HELLO! This is a test. <<<\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Nested with-temp-buffer (independent buffers)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_nested_independent() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested with-temp-buffer forms create independent buffers.
    // Inner buffer operations do not affect the outer buffer.
    let form = r####"(with-temp-buffer
  (insert "outer-content")
  (let ((outer-before (buffer-string)))
    (let ((inner-result
           (with-temp-buffer
             (insert "inner-content")
             (let ((inner-str (buffer-string)))
               (upcase-region (point-min) (point-max))
               (list inner-str (buffer-string))))))
      ;; Outer buffer should be unchanged after inner with-temp-buffer
      (let ((outer-after (buffer-string)))
        (list outer-before inner-result outer-after
              (string= outer-before outer-after))))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"outer-content\" (\"inner-content\" \"INNER-CONTENT\") \"outer-content\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_triple_nested() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of nesting.  Each level inserts text, reads it, and
    // verifies isolation.
    let form = r####"(with-temp-buffer
  (insert "L1")
  (let ((l1-str (buffer-string)))
    (let ((l2-result
           (with-temp-buffer
             (insert "L2")
             (let ((l2-str (buffer-string)))
               (let ((l3-result
                      (with-temp-buffer
                        (insert "L3")
                        (buffer-string))))
                 ;; After L3 exits, L2 buffer still intact
                 (list l2-str l3-result (buffer-string)))))))
      ;; After L2 exits, L1 buffer still intact
      (list l1-str l2-result (buffer-string)))))"####;
    let expect = expect_test::expect![[r#""OK (\"L1\" (\"L2\" \"L3\" \"L2\") \"L1\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-temp-buffer returning complex values
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_complex_return() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build up a complex data structure inside with-temp-buffer and return it.
    let form = r####"(with-temp-buffer
  (insert "key1=value1\nkey2=value2\nkey3=value3\n")
  (goto-char (point-min))
  ;; Parse key=value pairs into an alist
  (let ((result nil))
    (while (not (eobp))
      (let ((line-start (point)))
        (end-of-line)
        (let* ((line (buffer-substring line-start (point)))
               (eq-pos (string-match "=" line)))
          (when eq-pos
            (let ((key (substring line 0 eq-pos))
                  (val (substring line (1+ eq-pos))))
              (setq result (cons (cons key val) result)))))
        (forward-line 1)))
    (let ((parsed (nreverse result)))
      (list parsed
            (length parsed)
            (assoc "key2" parsed)
            (cdr (assoc "key3" parsed))))))"####;
    let expect = expect_test::expect![[
        r#""OK (((\"key1\" . \"value1\") (\"key2\" . \"value2\") (\"key3\" . \"value3\")) 3 (\"key2\" . \"value2\") \"value3\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer state isolation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_state_isolation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that with-temp-buffer does not affect the current buffer's
    // point, mark, or contents.
    let form = r####"(with-temp-buffer
  (insert "main-buffer-content")
  (goto-char 5)
  (let ((main-point (point))
        (main-content (buffer-string)))
    ;; Enter a sub with-temp-buffer
    (let ((sub-result
           (with-temp-buffer
             (insert "completely different stuff!!")
             (goto-char (point-max))
             (list (point) (buffer-string) (buffer-size)))))
      ;; After returning, verify main buffer state unchanged
      (list
        (= (point) main-point)
        (string= (buffer-string) main-content)
        main-point
        sub-result))))"####;
    let expect = expect_test::expect![[r#""OK (t t 5 (29 \"completely different stuff!!\" 28))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error handling inside with-temp-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Errors inside with-temp-buffer are caught by condition-case.
    // The temp buffer is still cleaned up.
    let form = r####"(list
  ;; Successful case
  (with-temp-buffer
    (insert "safe content")
    (buffer-string))
  ;; Error caught inside
  (condition-case err
      (with-temp-buffer
        (insert "before error")
        (error "deliberate error: %s" (buffer-string))
        (insert "after error — never reached")
        (buffer-string))
    (error (list 'caught (cadr err))))
  ;; Verify we can still use with-temp-buffer after error
  (with-temp-buffer
    (insert "post-error recovery")
    (buffer-string)))"####;
    let expect = expect_test::expect![[
        r#""OK (\"safe content\" (caught \"deliberate error: before error\") \"post-error recovery\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_error_preserves_outer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Error in nested with-temp-buffer does not corrupt outer buffer.
    let form = r####"(with-temp-buffer
  (insert "outer intact")
  (let ((outer-str (buffer-string)))
    (condition-case _err
        (with-temp-buffer
          (insert "inner will fail")
          (error "boom"))
      (error nil))
    ;; Outer buffer must be unchanged
    (list (string= (buffer-string) outer-str)
          (buffer-string))))"####;
    let expect = expect_test::expect![[r#""OK (t \"outer intact\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-temp-buffer with narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow to a region inside with-temp-buffer, then operate on the
    // narrowed region.  Widen and verify full contents.
    let form = r####"(with-temp-buffer
  (insert "AAAA:important-data:BBBB")
  ;; Find the important data between colons
  (goto-char (point-min))
  (search-forward ":")
  (let ((start (point)))
    (search-forward ":")
    (let ((end (1- (point))))
      (narrow-to-region start end)
      (let ((narrowed-str (buffer-string))
            (narrowed-min (point-min))
            (narrowed-max (point-max))
            (narrowed-size (buffer-size)))
        ;; Upcase within the narrowed region
        (upcase-region (point-min) (point-max))
        (let ((upcased (buffer-string)))
          (widen)
          (list narrowed-str narrowed-min narrowed-max narrowed-size
                upcased (buffer-string)))))))"####;
    let expect = expect_test::expect![[
        r#""OK (\"important-data\" 6 20 24 \"IMPORTANT-DATA\" \"AAAA:IMPORTANT-DATA:BBBB\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-temp-buffer with text properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert text with properties, verify they are preserved within
    // with-temp-buffer.
    let form = r####"(with-temp-buffer
  (insert "plain ")
  (let ((start (point)))
    (insert "bold")
    (put-text-property start (point) 'face 'bold))
  (insert " ")
  (let ((start2 (point)))
    (insert "italic")
    (put-text-property start2 (point) 'face 'italic))
  (let ((full (buffer-string))
        (props-at-8 (get-text-property 8 'face))
        (props-at-13 (get-text-property 13 'face)))
    ;; buffer-substring-no-properties strips properties
    (let ((no-props (buffer-substring-no-properties (point-min) (point-max))))
      (list full no-props props-at-8 props-at-13
            (string= full no-props)))))"####;
    let expect = expect_test::expect![[
        r#""OK (#(\"plain bold italic\" 6 10 (face bold) 11 17 (face italic)) \"plain bold italic\" bold italic t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_with_temp_buffer_propertize_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use propertize with insert, then read back properties.
    let form = r####"(with-temp-buffer
  (insert (propertize "hello" 'font-lock-face 'font-lock-keyword-face
                               'custom-prop 42))
  (insert " ")
  (insert (propertize "world" 'font-lock-face 'font-lock-string-face
                               'custom-prop 99))
  (list
    (buffer-string)
    (get-text-property 1 'custom-prop)
    (get-text-property 3 'font-lock-face)
    (get-text-property 7 'custom-prop)
    (get-text-property 7 'font-lock-face)
    ;; Space between words has no custom-prop
    (get-text-property 6 'custom-prop)))"####;
    let expect = expect_test::expect![[
        r#""OK (#(\"hello world\" 0 5 (font-lock-face font-lock-keyword-face custom-prop 42) 6 11 (font-lock-face font-lock-string-face custom-prop 99)) 42 font-lock-keyword-face 99 font-lock-string-face nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// with-temp-buffer: search and replace
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_with_temp_buffer_search_replace_loop() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Perform a search-and-replace loop inside with-temp-buffer.
    let form = r####"(with-temp-buffer
  (insert "the cat sat on the mat by the cat")
  (goto-char (point-min))
  (let ((count 0))
    (while (search-forward "cat" nil t)
      (replace-match "dog" t t)
      (setq count (1+ count)))
    (list (buffer-string) count)))"####;
    let expect = expect_test::expect![[r#""OK (\"the dog sat on the mat by the dog\" 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
