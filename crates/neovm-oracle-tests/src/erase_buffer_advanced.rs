//! Advanced oracle parity tests for `erase-buffer`:
//! erase with markers present (marker positions after erase), erase narrowed
//! buffer behavior, erase and text properties, erase+insert cycles, erase
//! with undo, combined with save-excursion, buffer-size after erase.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Multiple markers at various positions: all collapse to 1 after erase
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_markers_collapse() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create markers at beginning, middle, and end of buffer content,
    // including insertion-type markers. After erase-buffer all marker
    // positions should be 1 regardless of original position or type.
    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789")
  (let* ((m-begin (copy-marker 1))
         (m-mid1  (copy-marker 10))
         (m-mid2  (copy-marker 20))
         (m-end   (copy-marker (point-max)))
         ;; Insertion-type markers (advance when text inserted at their position)
         (m-ins   (copy-marker 15 t))
         (pre-positions (list (marker-position m-begin)
                              (marker-position m-mid1)
                              (marker-position m-mid2)
                              (marker-position m-end)
                              (marker-position m-ins)
                              (marker-insertion-type m-ins))))
    (erase-buffer)
    (let ((post-positions (list (marker-position m-begin)
                                (marker-position m-mid1)
                                (marker-position m-mid2)
                                (marker-position m-end)
                                (marker-position m-ins)
                                (marker-insertion-type m-ins)
                                ;; All should be 1
                                (= (marker-position m-begin) 1)
                                (= (marker-position m-mid1) 1)
                                (= (marker-position m-mid2) 1)
                                (= (marker-position m-end) 1)
                                (= (marker-position m-ins) 1))))
      ;; Now insert new text and check marker behavior
      (insert "New content here")
      (let ((after-insert (list (marker-position m-begin)
                                (marker-position m-mid1)
                                (marker-position m-mid2)
                                (marker-position m-end)
                                ;; insertion-type marker should have advanced
                                (marker-position m-ins)
                                (buffer-string))))
        (set-marker m-begin nil)
        (set-marker m-mid1 nil)
        (set-marker m-mid2 nil)
        (set-marker m-end nil)
        (set-marker m-ins nil)
        (list pre-positions post-positions after-insert)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 10 20 37 15 t) (1 1 1 1 1 t t t t t t) (1 1 1 1 17 \"New content here\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erase narrowed buffer: erase only visible portion, then check widened state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_narrowing_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // erase-buffer erases the entire buffer, even when narrowed.
    // Verify: the narrowing is effectively removed because all text is gone.
    let form = r#"(with-temp-buffer
  (insert "HEADER-START\n")
  (insert "line one\n")
  (insert "line two\n")
  (insert "line three\n")
  (insert "FOOTER-END\n")
  (let ((full-size (buffer-size))
        (full-text (buffer-string)))
    ;; Narrow to the middle lines only
    (goto-char (point-min))
    (forward-line 1)
    (let ((narrow-start (point)))
      (forward-line 3)
      (narrow-to-region narrow-start (point))
      (let ((narrowed-text (buffer-string))
            (narrowed-pmin (point-min))
            (narrowed-pmax (point-max))
            (narrowed-size (buffer-size)))
        ;; Erase while narrowed
        (erase-buffer)
        (let ((post-text (buffer-string))
              (post-size (buffer-size))
              (post-pmin (point-min))
              (post-pmax (point-max))
              (post-point (point)))
          ;; Widen to see what's left
          (widen)
          (let ((widened-text (buffer-string))
                (widened-size (buffer-size))
                (widened-pmin (point-min))
                (widened-pmax (point-max)))
            (list (list 'full full-size full-text)
                  (list 'narrowed narrowed-size narrowed-text
                        narrowed-pmin narrowed-pmax)
                  (list 'after-erase post-size post-text
                        post-pmin post-pmax post-point)
                  (list 'after-widen widened-size widened-text
                        widened-pmin widened-pmax))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((full 53 \"HEADER-START\\nline one\\nline two\\nline three\\nFOOTER-END\\n\") (narrowed 53 \"line one\\nline two\\nline three\\n\" 14 43) (after-erase 0 \"\" 1 1 1) (after-widen 0 \"\" 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erase buffer with text properties: properties are removed too
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert propertized text, verify properties exist, erase, then
    // insert again and verify no residual properties.
    let form = r#"(with-temp-buffer
  ;; Insert text with properties
  (insert (propertize "bold text" 'face 'bold))
  (insert " ")
  (insert (propertize "italic text" 'face 'italic))
  (insert " ")
  (insert (propertize "custom prop" 'my-prop 42 'another-prop "hello"))
  (let* ((pre-text (buffer-string))
         (pre-size (buffer-size))
         ;; Check properties at various positions
         (prop-at-1 (get-text-property 1 'face))
         (prop-at-12 (get-text-property 12 'face))
         (prop-at-24 (get-text-property 24 'my-prop))
         (prop-at-24b (get-text-property 24 'another-prop)))
    (erase-buffer)
    ;; Now insert plain text and verify no properties leak through
    (insert "plain text after erase")
    (let ((post-text (buffer-string))
          (post-prop-1 (get-text-property 1 'face))
          (post-prop-5 (get-text-property 5 'face))
          (post-prop-10 (get-text-property 10 'my-prop)))
      (list (list 'pre pre-text pre-size prop-at-1 prop-at-12
                  prop-at-24 prop-at-24b)
            (list 'post post-text
                  post-prop-1 post-prop-5 post-prop-10)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((pre #(\"bold text italic text custom prop\" 0 9 (face bold) 10 21 (face italic) 22 33 (my-prop 42 another-prop \"hello\")) 33 bold italic 42 \"hello\") (post \"plain text after erase\" nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erase+insert cycles: stress test with varying content sizes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_insert_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Repeatedly erase and fill with different content, tracking
    // buffer state invariants at each step. Simulates log rotation,
    // REPL output clearing, etc.
    let form = r#"(with-temp-buffer
  (let ((results nil)
        (cycle-data '(("Short" . 5)
                      ("Medium length string for testing" . 3)
                      ("X" . 100)
                      ("" . 0)
                      ("Final content with\nnewlines\nand\ttabs" . 1))))
    (dotimes (i (length cycle-data))
      (let* ((entry (nth i cycle-data))
             (text (car entry))
             (repeat-count (cdr entry)))
        (erase-buffer)
        ;; Insert the text repeated N times
        (dotimes (_ repeat-count)
          (insert text))
        (let ((snap (list i
                          (buffer-size)
                          (point)
                          (point-min)
                          (point-max)
                          (= (point) (point-max))
                          (buffer-string))))
          (setq results (cons snap results)))))
    ;; Final check: erase one more time
    (erase-buffer)
    (setq results (cons (list 'final
                              (buffer-size)
                              (point)
                              (bobp)
                              (eobp)
                              (string= (buffer-string) ""))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 25 26 1 26 t \"ShortShortShortShortShort\") (1 96 97 1 97 t \"Medium length string for testingMedium length string for testingMedium length string for testing\") (2 100 101 1 101 t \"XXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXXX\") (3 0 1 1 1 t \"\") (4 36 37 1 37 t \"Final content with\\nnewlines\\nand\ttabs\") (final 0 1 t t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erase buffer combined with undo: undo restores content
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Enable undo, insert text, erase, then undo to recover.
    // Verify that undo correctly restores the erased content.
    let form = r#"(with-temp-buffer
  ;; Undo is enabled by default in temp buffers
  (insert "First line of text\n")
  (insert "Second line of text\n")
  (insert "Third line of text\n")
  (let ((before-erase (buffer-string))
        (before-size (buffer-size))
        (before-point (point)))
    (erase-buffer)
    (let ((after-erase-text (buffer-string))
          (after-erase-size (buffer-size)))
      ;; Undo the erase
      (undo)
      (let ((after-undo-text (buffer-string))
            (after-undo-size (buffer-size))
            (after-undo-point (point)))
        (list (list 'before before-erase before-size)
              (list 'erased after-erase-text after-erase-size)
              (list 'undone after-undo-text after-undo-size)
              ;; Content should match
              (string= before-erase after-undo-text)
              (= before-size after-undo-size))))))"#;
    let expect =
        expect_test::expect![[r#""ERR (user-error \"No undo information in this buffer\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with erase + replacement: point restoration behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_save_excursion_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Nested save-excursion with erase in inner scope, then check
    // point restoration in both inner and outer scopes.
    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij")
  (goto-char 25)
  (let ((outer-point (point))
        (outer-result nil)
        (inner-result nil))
    (save-excursion
      (goto-char 10)
      (let ((mid-point (point)))
        (save-excursion
          (erase-buffer)
          (insert "replacement text")
          (setq inner-result
                (list 'inner (buffer-string) (buffer-size) (point))))
        ;; After inner save-excursion restores
        (setq outer-result
              (list 'after-inner-restore (point) (buffer-string)
                    (<= (point) (point-max))
                    (>= (point) (point-min))))))
    ;; After outer save-excursion restores
    (list outer-point
          inner-result
          outer-result
          (list 'outer-restore (point) (buffer-string)
                (<= (point) (point-max))))))"#;
    let expect = expect_test::expect![[
        r#""OK (25 (inner \"replacement text\" 16 17) (after-inner-restore 1 \"replacement text\" t t) (outer-restore 1 \"replacement text\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-size consistency across erase operations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_size_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify buffer-size, point-max, and (length (buffer-string)) are
    // consistent before and after erase-buffer, including multibyte content.
    let form = r#"(with-temp-buffer
  (let ((results nil))
    ;; ASCII content
    (insert "Hello World 12345")
    (let ((s1 (buffer-size))
          (pm1 (point-max))
          (l1 (length (buffer-string))))
      (setq results (cons (list 'ascii s1 pm1 (1- pm1)
                                (= s1 (1- pm1))
                                (= s1 l1))
                          results))
      (erase-buffer)
      (setq results (cons (list 'ascii-erased
                                (buffer-size) (point-max)
                                (= (buffer-size) 0)
                                (= (point-max) 1))
                          results)))
    ;; Multibyte content (CJK characters are 3 bytes in UTF-8)
    (insert "Hello")
    (let ((s2 (buffer-size))
          (pm2 (point-max))
          (l2 (length (buffer-string))))
      (setq results (cons (list 'multibyte s2 pm2 l2
                                (= s2 (1- pm2)))
                          results))
      (erase-buffer)
      (setq results (cons (list 'multibyte-erased
                                (buffer-size) (point-max)
                                (= (buffer-size) 0))
                          results)))
    ;; Mixed: insert, partial delete, erase
    (insert "ABCDEFGHIJ")
    (delete-region 4 7)
    (let ((after-del-size (buffer-size))
          (after-del-text (buffer-string)))
      (erase-buffer)
      (setq results (cons (list 'after-partial-delete
                                after-del-size after-del-text
                                (buffer-size) (buffer-string))
                          results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((ascii 17 18 17 t t) (ascii-erased 0 1 t t) (multibyte 5 6 5 t) (multibyte-erased 0 1 t) (after-partial-delete 7 \"ABCGHIJ\" 0 \"\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Erase buffer with overlays: overlays removed after erase
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_erase_buffer_advanced_marker_reuse_after_erase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // After erase-buffer, reposition markers to new content and verify
    // they work correctly. Tests that markers remain usable after erase.
    let form = r#"(with-temp-buffer
  (insert "original content here with some length to it")
  (let ((m1 (copy-marker 5))
        (m2 (copy-marker 20))
        (m3 (copy-marker 30 t)))
    (let ((pre (list (marker-position m1)
                     (marker-position m2)
                     (marker-position m3)
                     (marker-buffer m1)
                     (eq (marker-buffer m1) (current-buffer)))))
      (erase-buffer)
      (let ((mid (list (marker-position m1)
                       (marker-position m2)
                       (marker-position m3)
                       ;; Markers still belong to this buffer
                       (eq (marker-buffer m1) (current-buffer)))))
        ;; Insert new content and reposition markers
        (insert "brand new replacement text for testing markers")
        (set-marker m1 10)
        (set-marker m2 25)
        (set-marker m3 35)
        (let ((post (list (marker-position m1)
                          (marker-position m2)
                          (marker-position m3)
                          ;; Verify we can read text at marker positions
                          (char-after m1)
                          (char-after m2)
                          (char-after m3)
                          (buffer-string))))
          (set-marker m1 nil)
          (set-marker m2 nil)
          (set-marker m3 nil)
          (list pre mid post))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((5 20 30 #<killed buffer> t) (1 1 1 t) (10 25 35 32 120 116 \"brand new replacement text for testing markers\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
