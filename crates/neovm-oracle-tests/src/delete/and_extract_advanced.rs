//! Advanced oracle parity tests for `delete-and-extract-region`:
//! extract various regions, extract with text properties, extract in
//! narrowed buffer, extract entire buffer, extract empty region,
//! combined with insert to move text, extract and compare with
//! buffer-substring.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Extract various regions: beginning, middle, end, single char, multiline
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_various_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract different sub-regions from a buffer and verify both
    // the extracted text and the remaining buffer content.
    let form = r#"(let ((results nil))
  ;; Extract from beginning
  (with-temp-buffer
    (insert "ABCDEFGHIJKLMNOP")
    (let ((extracted (delete-and-extract-region 1 5)))
      (setq results (cons (list 'beginning extracted (buffer-string)
                                (buffer-size) (point))
                          results))))
  ;; Extract from middle
  (with-temp-buffer
    (insert "ABCDEFGHIJKLMNOP")
    (let ((extracted (delete-and-extract-region 6 11)))
      (setq results (cons (list 'middle extracted (buffer-string)
                                (buffer-size))
                          results))))
  ;; Extract from end
  (with-temp-buffer
    (insert "ABCDEFGHIJKLMNOP")
    (let ((extracted (delete-and-extract-region 13 17)))
      (setq results (cons (list 'end extracted (buffer-string)
                                (buffer-size))
                          results))))
  ;; Extract single character
  (with-temp-buffer
    (insert "ABCDEFGHIJKLMNOP")
    (let ((extracted (delete-and-extract-region 8 9)))
      (setq results (cons (list 'single-char extracted (buffer-string))
                          results))))
  ;; Extract multiline region
  (with-temp-buffer
    (insert "line one\nline two\nline three\nline four\n")
    (let ((extracted (delete-and-extract-region 10 28)))
      (setq results (cons (list 'multiline extracted (buffer-string))
                          results))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((beginning \"ABCD\" \"EFGHIJKLMNOP\" 12 13) (middle \"FGHIJ\" \"ABCDEKLMNOP\" 11) (end \"MNOP\" \"ABCDEFGHIJKL\" 12) (single-char \"H\" \"ABCDEFGIJKLMNOP\") (multiline \"line two\\nline thre\" \"line one\\ne\\nline four\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract with text properties: extracted text retains properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert propertized text, extract a region spanning multiple
    // property runs, and verify the extracted string has the
    // correct properties while the buffer lost them.
    let form = r#"(with-temp-buffer
  (insert (propertize "BOLD" 'face 'bold))
  (insert " normal ")
  (insert (propertize "ITALIC" 'face 'italic))
  (insert " more ")
  (insert (propertize "CUSTOM" 'my-val 99))
  (let ((full-text (buffer-string))
        (full-size (buffer-size)))
    ;; Extract region spanning from BOLD through normal into ITALIC
    ;; "LD normal IT"
    (let ((extracted (delete-and-extract-region 3 15)))
      (list full-text
            full-size
            extracted
            (length extracted)
            ;; Check properties on extracted string
            (get-text-property 0 'face extracted)
            (get-text-property 3 'face extracted)
            (get-text-property 10 'face extracted)
            ;; Remaining buffer
            (buffer-string)
            (buffer-size)
            ;; Properties on remaining buffer
            (get-text-property 1 'face)
            (get-text-property (- (point-max) 2) 'my-val)))))"#;
    let expect = expect_test::expect![[
        r#""OK (#(\"BOLD normal ITALIC more CUSTOM\" 0 4 (face bold) 12 18 (face italic) 24 30 (my-val 99)) 30 #(\"LD normal IT\" 0 2 (face bold) 10 12 (face italic)) 12 bold nil italic #(\"BOALIC more CUSTOM\" 0 2 (face bold) 2 6 (face italic) 12 18 (my-val 99)) 18 bold 99)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract in narrowed buffer: respects restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow the buffer, extract from the narrowed region, then widen
    // to see the full picture. Positions are relative to narrowed region.
    let form = r#"(with-temp-buffer
  (insert "HEADER:alpha:beta:gamma:delta:FOOTER")
  (let ((full-before (buffer-string)))
    ;; Narrow to the middle portion "alpha:beta:gamma:delta"
    (narrow-to-region 8 30)
    (let ((narrowed-text (buffer-string))
          (narrowed-pmin (point-min))
          (narrowed-pmax (point-max)))
      ;; Extract "beta:gamma" from the narrowed view
      (let ((extracted (delete-and-extract-region 14 25)))
        (let ((narrowed-after (buffer-string))
              (narrowed-after-size (buffer-size)))
          ;; Widen to see full buffer
          (widen)
          (let ((full-after (buffer-string))
                (full-after-size (buffer-size)))
            (list (list 'full-before full-before)
                  (list 'narrowed narrowed-text narrowed-pmin narrowed-pmax)
                  (list 'extracted extracted)
                  (list 'narrowed-after narrowed-after narrowed-after-size)
                  (list 'full-after full-after full-after-size))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((full-before \"HEADER:alpha:beta:gamma:delta:FOOTER\") (narrowed \"alpha:beta:gamma:delta\" 8 30) (extracted \"beta:gamma:\") (narrowed-after \"alpha:delta\" 25) (full-after \"HEADER:alpha:delta:FOOTER\" 25))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract entire buffer: equivalent to buffer-string + erase-buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_entire_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extracting (point-min) to (point-max) should return the full
    // content and leave the buffer empty, similar to erase-buffer.
    let form = r#"(let ((content "The quick brown fox jumps over the lazy dog.\nSecond line.\n")
      (results nil))
  ;; Method 1: delete-and-extract-region on entire buffer
  (with-temp-buffer
    (insert content)
    (let ((extracted (delete-and-extract-region (point-min) (point-max))))
      (setq results (cons (list 'extract-all
                                extracted
                                (buffer-string)
                                (buffer-size)
                                (point)
                                (= (point) 1)
                                (bobp) (eobp))
                          results))))
  ;; Method 2: buffer-string + erase-buffer for comparison
  (with-temp-buffer
    (insert content)
    (let ((grabbed (buffer-string)))
      (erase-buffer)
      (setq results (cons (list 'grab-erase
                                grabbed
                                (buffer-string)
                                (buffer-size)
                                (point))
                          results))))
  ;; They should produce equivalent extracted text
  (let ((r1 (cadr (assq 'extract-all (mapcar (lambda (r) (cons (car r) r))
                                              (nreverse results))))))
    results))"#;
    let expect = expect_test::expect![[
        r#""OK ((grab-erase \"The quick brown fox jumps over the lazy dog.\\nSecond line.\\n\" \"\" 0 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract empty region: returns empty string, buffer unchanged
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_empty_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extracting a zero-length region (start = end) should return ""
    // and leave the buffer completely untouched.
    let form = r#"(with-temp-buffer
  (insert "unchanged content here")
  (let ((before-text (buffer-string))
        (before-size (buffer-size))
        (before-point (point)))
    ;; Extract empty region at various positions
    (let ((e1 (delete-and-extract-region 1 1))
          (e2 (delete-and-extract-region 10 10))
          (e3 (delete-and-extract-region (point-max) (point-max))))
      ;; Also test reversed args (start > end should be handled)
      (let ((e4 (delete-and-extract-region 5 5)))
        (list (list 'empty-extracts e1 e2 e3 e4)
              (list 'all-empty-strings
                    (string= e1 "")
                    (string= e2 "")
                    (string= e3 "")
                    (string= e4 ""))
              (list 'buffer-unchanged
                    (string= before-text (buffer-string))
                    (= before-size (buffer-size))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((empty-extracts \"\" \"\" \"\" \"\") (all-empty-strings t t t t) (buffer-unchanged t t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Use extract+insert to move text within buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_move_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Implement a "move text" operation: extract from one location,
    // insert at another. Test moving text forward and backward.
    let form = r#"(progn
  (fset 'neovm--test-move-text
    (lambda (start end target)
      "Move text from [START,END) to position TARGET.
Returns the buffer content after the move."
      (let ((text (delete-and-extract-region start end)))
        ;; After deletion, positions shift. If target was after the
        ;; deleted region, adjust by the length of deleted text.
        (let ((adjusted-target (if (> target end)
                                   (- target (- end start))
                                 (if (> target start)
                                     start
                                   target))))
          (goto-char adjusted-target)
          (insert text)
          (buffer-string)))))
  (unwind-protect
      (let ((results nil))
        ;; Move word from beginning to end
        (with-temp-buffer
          (insert "Hello World Goodbye")
          (setq results (cons (list 'move-to-end
                                    (funcall 'neovm--test-move-text 1 6 20))
                              results)))
        ;; Move word from end to beginning
        (with-temp-buffer
          (insert "World Hello Goodbye")
          (setq results (cons (list 'move-to-start
                                    (funcall 'neovm--test-move-text 13 20 1))
                              results)))
        ;; Move middle section forward
        (with-temp-buffer
          (insert "[A][B][C][D][E]")
          (setq results (cons (list 'move-mid-fwd
                                    (funcall 'neovm--test-move-text 4 7 13))
                              results)))
        ;; Move middle section backward
        (with-temp-buffer
          (insert "[A][B][C][D][E]")
          (setq results (cons (list 'move-mid-bwd
                                    (funcall 'neovm--test-move-text 10 13 4))
                              results)))
        (nreverse results))
    (fmakunbound 'neovm--test-move-text)))"#;
    let expect = expect_test::expect![[
        r#""OK ((move-to-end \" World GoodbyeHello\") (move-to-start \"GoodbyeWorld Hello \") (move-mid-fwd \"[A][C][D][B][E]\") (move-mid-bwd \"[A][D][B][C][E]\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Extract and compare with buffer-substring: should be identical
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_vs_buffer_substring() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // For various regions, compare the result of delete-and-extract-region
    // with what buffer-substring would have returned for the same region.
    // They should be string-equal (including text properties from
    // buffer-substring, though delete-and-extract-region also preserves them).
    let form = r#"(let ((content "Alpha Bravo Charlie Delta Echo Foxtrot Golf")
      (regions '((1 . 6) (7 . 12) (15 . 30) (1 . 44)))
      (results nil))
  (dolist (region regions)
    (let ((start (car region))
          (end (cdr region)))
      ;; Get buffer-substring from one buffer
      (let ((substring-result nil)
            (extract-result nil))
        (with-temp-buffer
          (insert content)
          (setq substring-result
                (buffer-substring-no-properties start end)))
        ;; Get delete-and-extract-region from another
        (with-temp-buffer
          (insert content)
          (setq extract-result
                (delete-and-extract-region start end)))
        (setq results
              (cons (list (cons start end)
                          substring-result
                          extract-result
                          (string= substring-result extract-result))
                    results)))))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK (((1 . 6) \"Alpha\" \"Alpha\" t) ((7 . 12) \"Bravo\" \"Bravo\" t) ((15 . 30) \"arlie Delta Ech\" \"arlie Delta Ech\" t) ((1 . 44) \"Alpha Bravo Charlie Delta Echo Foxtrot Golf\" \"Alpha Bravo Charlie Delta Echo Foxtrot Golf\" t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Chained extracts building a new document from pieces
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_chained_assembly() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use delete-and-extract-region repeatedly to disassemble a buffer
    // into parts, then reassemble them in a different order in a new buffer.
    let form = r#"(let ((src (get-buffer-create " *neovm-extract-src*"))
      (dst (get-buffer-create " *neovm-extract-dst*")))
  (unwind-protect
      (progn
        ;; Set up source with labeled sections
        (with-current-buffer src
          (erase-buffer)
          (insert "[INTRO]Hello there![/INTRO]")
          (insert "[BODY]This is the main content.[/BODY]")
          (insert "[FOOTER]Goodbye![/FOOTER]"))
        ;; Extract sections in reverse order and build destination
        (with-current-buffer dst
          (erase-buffer))
        ;; Extract footer first (it's at the end)
        (let ((footer nil) (body nil) (intro nil))
          (with-current-buffer src
            ;; Find and extract footer
            (goto-char (point-min))
            (search-forward "[FOOTER]")
            (let ((fstart (point)))
              (search-forward "[/FOOTER]")
              (setq footer (delete-and-extract-region
                            fstart (- (point) (length "[/FOOTER]")))))
            ;; Find and extract body
            (goto-char (point-min))
            (search-forward "[BODY]")
            (let ((bstart (point)))
              (search-forward "[/BODY]")
              (setq body (delete-and-extract-region
                          bstart (- (point) (length "[/BODY]")))))
            ;; Find and extract intro
            (goto-char (point-min))
            (search-forward "[INTRO]")
            (let ((istart (point)))
              (search-forward "[/INTRO]")
              (setq intro (delete-and-extract-region
                           istart (- (point) (length "[/INTRO]"))))))
          ;; Reassemble in new order: FOOTER, BODY, INTRO
          (with-current-buffer dst
            (insert "=== Reversed Document ===\n")
            (insert footer)
            (insert "\n---\n")
            (insert body)
            (insert "\n---\n")
            (insert intro)
            (insert "\n"))
          (list (list 'extracted intro body footer)
                (list 'source-remaining
                      (with-current-buffer src (buffer-string)))
                (list 'destination
                      (with-current-buffer dst (buffer-string))))))
    (kill-buffer src)
    (kill-buffer dst)))"#;
    let expect = expect_test::expect![[
        r#""OK ((extracted \"Hello there!\" \"This is the main content.\" \"Goodbye!\") (source-remaining \"[INTRO][/INTRO][BODY][/BODY][FOOTER][/FOOTER]\") (destination \"=== Reversed Document ===\\nGoodbye!\\n---\\nThis is the main content.\\n---\\nHello there!\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
