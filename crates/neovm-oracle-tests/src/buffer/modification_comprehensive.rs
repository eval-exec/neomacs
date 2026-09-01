//! Oracle parity tests for comprehensive buffer modification operations:
//! insert, insert-before-markers, insert-char, delete-region, delete-char,
//! delete-and-extract-region, erase-buffer, subst-char-in-region,
//! transpose-regions, combined insert+delete sequences, point tracking
//! through modifications, marker behavior, narrowing interactions, and
//! undo-list tracking.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// insert with various argument types and counts
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_insert_multi_strings_and_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // insert with no args, single string, multiple strings, chars, mixed
    let form = r##"(with-temp-buffer
  (insert)
  (let ((r1 (buffer-string)))
    (insert "hello")
    (let ((r2 (buffer-string)))
      (insert " " "world" " " "foo" " " "bar")
      (let ((r3 (buffer-string)))
        (erase-buffer)
        (insert ?A ?B ?C)
        (let ((r4 (buffer-string)))
          (erase-buffer)
          (insert "start" ?- ?> "end" ?! ?.)
          (let ((r5 (buffer-string)))
            (list r1 r2 r3 r4 r5)))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// insert-before-markers vs insert: marker movement differences
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_insert_before_markers_vs_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Demonstrate that insert-before-markers moves markers at insertion point
    // while insert does not
    let form = r#"(with-temp-buffer
  (insert "ABCDEF")
  (goto-char 4)
  (let ((m1 (point-marker))
        (m2 (copy-marker (point))))
    ;; Both markers at position 4
    (let ((before-m1 (marker-position m1))
          (before-m2 (marker-position m2)))
      ;; Regular insert at point (4): markers at 4 should NOT move
      (goto-char 4)
      (insert "xx")
      ;; m1 and m2 were at position 4 (insertion type nil by default)
      (let ((after-insert-m1 (marker-position m1))
            (after-insert-m2 (marker-position m2))
            (buf1 (buffer-string)))
        ;; Now insert-before-markers at current point (6, after "xx"):
        ;; first reset markers
        (set-marker m1 6)
        (set-marker m2 6)
        (goto-char 6)
        (insert-before-markers "YY")
        (let ((after-ibm-m1 (marker-position m1))
              (after-ibm-m2 (marker-position m2))
              (buf2 (buffer-string)))
          (list :before before-m1 before-m2
                :after-insert after-insert-m1 after-insert-m2 buf1
                :after-ibm after-ibm-m1 after-ibm-m2 buf2))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// insert-char: character repeated N times with optional inherit
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_insert_char_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Basic: insert char N times
  (insert-char ?A 5)
  (let ((r1 (buffer-string)))
    (erase-buffer)
    ;; Zero repetitions
    (insert-char ?B 0)
    (let ((r2 (buffer-string)))
      (erase-buffer)
      ;; Single repetition
      (insert-char ?C 1)
      (let ((r3 (buffer-string)))
        ;; Insert-char with unicode
        (erase-buffer)
        (insert-char #x03B1 3)  ;; Greek alpha
        (let ((r4 (buffer-string)))
          ;; Insert char in the middle of existing text
          (erase-buffer)
          (insert "Hello World")
          (goto-char 6)
          (insert-char ?_ 3)
          (let ((r5 (buffer-string)))
            (list r1 r2 r3 r4 r5 (point))))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// delete-region: various range combinations
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_delete_region_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  ;; Delete from beginning
  (delete-region 1 4)
  (let ((r1 (buffer-string)))
    ;; Delete from end
    (delete-region (- (point-max) 2) (point-max))
    (let ((r2 (buffer-string)))
      ;; Delete in middle
      (delete-region 3 6)
      (let ((r3 (buffer-string)))
        ;; Delete entire buffer
        (delete-region (point-min) (point-max))
        (let ((r4 (buffer-string)))
          ;; Delete empty region (no-op)
          (insert "test")
          (delete-region 2 2)
          (let ((r5 (buffer-string)))
            ;; Delete with reversed args (start > end is valid)
            (delete-region 4 2)
            (let ((r6 (buffer-string)))
              (list r1 r2 r3 r4 r5 r6))))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// delete-char: positive and negative counts, edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_delete_char_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "abcdefghij")
  (goto-char 1)
  ;; Delete 3 chars forward from beginning
  (delete-char 3)
  (let ((r1 (list (buffer-string) (point))))
    (erase-buffer)
    (insert "abcdefghij")
    (goto-char (point-max))
    ;; Delete 3 chars backward from end
    (delete-char -3)
    (let ((r2 (list (buffer-string) (point))))
      (erase-buffer)
      (insert "abcdefghij")
      (goto-char 5)
      ;; Delete 0 chars (no-op)
      (delete-char 0)
      (let ((r3 (list (buffer-string) (point))))
        (erase-buffer)
        (insert "abcdefghij")
        (goto-char 5)
        ;; Delete 2 forward then 2 backward
        (delete-char 2)
        (delete-char -2)
        (let ((r4 (list (buffer-string) (point))))
          (list r1 r2 r3 r4))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region: return value + buffer state
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_delete_and_extract_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog")
  ;; Extract "quick"
  (let* ((extracted1 (delete-and-extract-region 5 10))
         (buf1 (buffer-string)))
    ;; Extract from current state
    (let* ((extracted2 (delete-and-extract-region 1 5))
           (buf2 (buffer-string)))
      ;; Extract empty region
      (let* ((extracted3 (delete-and-extract-region 3 3))
             (buf3 (buffer-string)))
        ;; Extract with reversed args
        (let* ((extracted4 (delete-and-extract-region 10 5))
               (buf4 (buffer-string)))
          ;; Extract entire buffer
          (let* ((extracted5 (delete-and-extract-region (point-min) (point-max)))
                 (buf5 (buffer-string)))
            (list extracted1 buf1
                  extracted2 buf2
                  extracted3 buf3
                  extracted4 buf4
                  extracted5 buf5)))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// erase-buffer: various states
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_erase_buffer_states() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Erase empty buffer
  (erase-buffer)
  (let ((r1 (list (buffer-string) (point) (point-min) (point-max))))
    ;; Insert then erase
    (insert "Hello World\nSecond Line\nThird Line")
    (goto-char 15)
    (erase-buffer)
    (let ((r2 (list (buffer-string) (point) (point-min) (point-max))))
      ;; Insert, narrow, widen, erase
      (insert "0123456789")
      (narrow-to-region 3 8)
      (widen)
      (erase-buffer)
      (let ((r3 (list (buffer-string) (point) (point-min) (point-max))))
        ;; Double erase
        (insert "data")
        (erase-buffer)
        (erase-buffer)
        (let ((r4 (list (buffer-string) (point))))
          (list r1 r2 r3 r4))))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// subst-char-in-region: character substitution
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_subst_char_in_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "hello world hello")
  ;; Replace all 'l' with 'L' in entire buffer
  (subst-char-in-region (point-min) (point-max) ?l ?L)
  (let ((r1 (buffer-string)))
    (erase-buffer)
    (insert "aabbaabbccdd")
    ;; Replace 'a' with 'x' only in region 3..8
    (subst-char-in-region 3 8 ?a ?x)
    (let ((r2 (buffer-string)))
      (erase-buffer)
      (insert "no-match-here")
      ;; Replace char that doesn't exist (no-op)
      (subst-char-in-region (point-min) (point-max) ?Z ?Q)
      (let ((r3 (buffer-string)))
        (list r1 r2 r3)))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// transpose-regions: swapping buffer regions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_transpose_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "AAAbbbCCC")
  ;; Swap "AAA" (1-4) with "CCC" (7-10)
  (transpose-regions 1 4 7 10)
  (let ((r1 (buffer-string)))
    (erase-buffer)
    ;; Adjacent regions
    (insert "XXXYYY")
    (transpose-regions 1 4 4 7)
    (let ((r2 (buffer-string)))
      (erase-buffer)
      ;; Non-adjacent with gap
      (insert "11-22-33")
      (transpose-regions 1 3 6 8)
      (let ((r3 (buffer-string)))
        (list r1 r2 r3)))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Combined insert+delete sequences with point tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_insert_delete_sequence_point_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (let ((trace nil))
    (insert "ABCDEFGHIJ")
    (push (list :after-insert (buffer-string) (point)) trace)
    ;; Go to middle, delete 2 forward
    (goto-char 5)
    (delete-char 2)
    (push (list :after-del-fwd (buffer-string) (point)) trace)
    ;; Insert at current point
    (insert "XY")
    (push (list :after-ins (buffer-string) (point)) trace)
    ;; Delete backward
    (delete-char -3)
    (push (list :after-del-bwd (buffer-string) (point)) trace)
    ;; Go to beginning, insert
    (goto-char 1)
    (insert ">>")
    (push (list :after-prepend (buffer-string) (point)) trace)
    ;; Go to end, insert
    (goto-char (point-max))
    (insert "<<")
    (push (list :after-append (buffer-string) (point)) trace)
    ;; Delete from middle
    (delete-region 4 8)
    (push (list :after-del-region (buffer-string) (point)) trace)
    (nreverse trace)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Marker behavior during inserts and deletes
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_marker_behavior_during_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789")
  ;; Place markers at various positions
  (let ((m1 (copy-marker 3))   ;; before "2"
        (m2 (copy-marker 6))   ;; before "5"
        (m3 (copy-marker 9)))  ;; before "8"
    (let ((trace nil))
      ;; State 0: initial
      (push (list :init (marker-position m1) (marker-position m2) (marker-position m3)) trace)
      ;; Insert at position 5 (between m1 and m2)
      (goto-char 5)
      (insert "XX")
      (push (list :after-ins-5 (marker-position m1) (marker-position m2) (marker-position m3)
                  (buffer-string)) trace)
      ;; Delete region covering m2's original area
      (delete-region 7 10)
      (push (list :after-del-7-10 (marker-position m1) (marker-position m2) (marker-position m3)
                  (buffer-string)) trace)
      ;; Insert at beginning (should shift all markers)
      (goto-char 1)
      (insert ">>")
      (push (list :after-prepend (marker-position m1) (marker-position m2) (marker-position m3)
                  (buffer-string)) trace)
      (nreverse trace))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Narrowing interaction with modifications
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_narrowing_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  ;; Narrow to region 5-12
  (narrow-to-region 5 12)
  (let ((trace nil))
    ;; Can only see/modify narrowed region
    (push (list :narrowed (buffer-string) (point-min) (point-max)) trace)
    ;; Insert at beginning of narrowed region
    (goto-char (point-min))
    (insert "<<")
    (push (list :after-ins-min (buffer-string) (point-min) (point-max)) trace)
    ;; Insert at end of narrowed region
    (goto-char (point-max))
    (insert ">>")
    (push (list :after-ins-max (buffer-string) (point-min) (point-max)) trace)
    ;; Delete in narrowed region
    (delete-region 7 10)
    (push (list :after-del (buffer-string) (point-min) (point-max)) trace)
    ;; Widen and see full buffer
    (widen)
    (push (list :widened (buffer-string) (point-min) (point-max)) trace)
    (nreverse trace)))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// before/after-change-functions effects on global state (via let-binding)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_change_functions_side_effects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (let ((change-log nil))
    ;; Set up change tracking hooks via let-binding
    (let ((before-change-functions
           (list (lambda (beg end)
                   (push (list 'before beg end) change-log))))
          (after-change-functions
           (list (lambda (beg end len)
                   (push (list 'after beg end len) change-log)))))
      ;; Perform modifications
      (insert "hello")
      (goto-char 3)
      (insert " beautiful")
      (delete-region 1 3))
    ;; Change log should record the modifications
    (list :log (nreverse change-log)
          :final (buffer-string))))"#;
    assert_oracle_parity(form);
}

// ---------------------------------------------------------------------------
// Complex: build and modify a structured document
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_structured_document_building() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Build a simple key-value config file
  (let ((entries '(("host" . "localhost")
                   ("port" . "8080")
                   ("debug" . "true")
                   ("timeout" . "30"))))
    (dolist (entry entries)
      (insert (car entry) " = " (cdr entry) "\n"))
    (let ((r1 (buffer-string)))
      ;; Now modify: change port to 9090
      (goto-char (point-min))
      (search-forward "port = ")
      (let ((start (point)))
        (end-of-line)
        (delete-region start (point))
        (insert "9090"))
      (let ((r2 (buffer-string)))
        ;; Delete debug line entirely
        (goto-char (point-min))
        (search-forward "debug")
        (beginning-of-line)
        (let ((line-start (point)))
          (forward-line 1)
          (delete-region line-start (point)))
        (let ((r3 (buffer-string)))
          ;; Add a new entry at beginning
          (goto-char (point-min))
          (insert "# Config file\n")
          (let ((r4 (buffer-string)))
            (list r1 r2 r3 r4)))))))"##;
    let expect = expect_test::expect![[
        r##""OK (\"host = localhost\\nport = 8080\\ndebug = true\\ntimeout = 30\\n\" \"host = localhost\\nport = 9090\\ndebug = true\\ntimeout = 30\\n\" \"host = localhost\\nport = 9090\\ntimeout = 30\\n\" \"# Config file\\nhost = localhost\\nport = 9090\\ntimeout = 30\\n\")""##
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Multiple delete-and-extract in sequence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_extract_multiple_regions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "one:two:three:four:five")
  ;; Extract tokens separated by colons
  (let ((tokens nil))
    (goto-char (point-min))
    (let ((start (point)))
      (while (search-forward ":" nil t)
        (push (delete-and-extract-region start (1- (point))) tokens)
        (delete-char -1)  ;; delete the colon
        (setq start (point)))
      ;; Get last token
      (push (delete-and-extract-region start (point-max)) tokens))
    (list :tokens (nreverse tokens)
          :remaining (buffer-string))))"#;
    let expect = expect_test::expect![[
        r#""OK (:tokens (\"one\" \"two\" \"three\" \"four\" \"five\") :remaining \"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// insert and point-max interactions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_point_max_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (let ((trace nil))
    (push (list :empty (point-min) (point-max) (point)) trace)
    (insert "12345")
    (push (list :after-5 (point-min) (point-max) (point)) trace)
    (insert "67890")
    (push (list :after-10 (point-min) (point-max) (point)) trace)
    (goto-char 3)
    (delete-char 4)
    (push (list :after-del (point-min) (point-max) (point)) trace)
    (goto-char (point-max))
    (insert "END")
    (push (list :after-end (point-min) (point-max) (point)) trace)
    (erase-buffer)
    (push (list :erased (point-min) (point-max) (point)) trace)
    (nreverse trace)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:empty 1 1 1) (:after-5 1 6 6) (:after-10 1 11 11) (:after-del 1 7 3) (:after-end 1 10 10) (:erased 1 1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with modifications
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_save_excursion_with_mods() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (goto-char 5)
  (let ((p-before (point)))
    ;; save-excursion should restore point after modifications
    (save-excursion
      (goto-char 1)
      (insert ">>")
      (goto-char (point-max))
      (insert "<<")
      (delete-region 6 9))
    (list :point-before p-before
          :point-after (point)
          :buffer (buffer-string))))"#;
    let expect =
        expect_test::expect![[r#""OK (:point-before 5 :point-after 6 :buffer \">>ABCGHIJ<<\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: repeated search-and-replace using insert/delete
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_search_replace_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "the cat sat on the mat by the hat")
  ;; Replace all occurrences of "the" with "THE"
  (goto-char (point-min))
  (let ((count 0))
    (while (search-forward "the" nil t)
      (replace-match "THE")
      (setq count (1+ count)))
    (let ((r1 (list :count count :buf (buffer-string))))
      ;; Now replace "at" with "AT" but only as whole words using re
      (goto-char (point-min))
      (let ((count2 0))
        (while (re-search-forward "\\bat\\b" nil t)
          (replace-match "AT")
          (setq count2 (1+ count2)))
        (list r1 :re-count count2 :buf2 (buffer-string))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((:count 3 :buf \"THE cat sat on THE mat by THE hat\") :re-count 0 :buf2 \"THE cat sat on THE mat by THE hat\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Buffer modification with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_multibyte_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Insert multibyte text
  (insert "Hello" " " "World")
  (let ((r1 (list (buffer-string) (point) (buffer-size))))
    ;; Insert more multibyte
    (goto-char 6)
    (insert " Beautiful")
    (let ((r2 (list (buffer-string) (point))))
      ;; Delete a multibyte char region
      (erase-buffer)
      (insert "ABC")
      (goto-char 2)
      (delete-char 1)
      (let ((r3 (list (buffer-string) (point))))
        (list r1 r2 r3)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"Hello World\" 12 11) (\"Hello Beautiful World\" 16) (\"AC\" 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Undo-list tracking through modifications
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_mod_undo_list_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Enable undo
  (setq buffer-undo-list nil)
  (insert "Hello")
  (let ((after-insert-undo (not (null buffer-undo-list))))
    ;; Boundary
    (undo-boundary)
    (insert " World")
    (undo-boundary)
    (delete-region 1 6)
    (undo-boundary)
    (let ((has-undo-entries (not (null buffer-undo-list)))
          (current-buf (buffer-string)))
      (list :has-undo has-undo-entries
            :first-insert-tracked after-insert-undo
            :buffer current-buf))))"#;
    let expect =
        expect_test::expect![[r#""OK (:has-undo t :first-insert-tracked t :buffer \" World\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
