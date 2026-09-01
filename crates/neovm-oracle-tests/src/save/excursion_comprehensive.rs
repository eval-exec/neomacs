//! Comprehensive oracle parity tests for `save-excursion`:
//! nested save-excursion preserving point independently, buffer switches,
//! interaction with narrowing/save-restriction, insert/delete point adjustment,
//! marker interaction, error recovery, and multi-buffer scenarios.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Nested save-excursion preserving point independently at different depths
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_nested_independent_point_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Three levels of nesting: each save-excursion restores its own saved
    // point independently. Inner modifications should not corrupt outer
    // saved positions.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz0123456789")
      (goto-char 5)
      (let ((p0 (point)))
        (save-excursion
          (goto-char 10)
          (let ((p1 (point)))
            (save-excursion
              (goto-char 20)
              (let ((p2 (point)))
                (save-excursion
                  (goto-char 30)
                  (let ((p3 (point)))
                    ;; innermost returns all captured points
                    (list p0 p1 p2 p3 (point))))
                ;; after innermost restore, back to 20
                (list (point) (= (point) p2))))
            ;; after middle restore, back to 10
            (list (point) (= (point) p1))))
        ;; after outermost restore, back to 5
        (list (point) (= (point) p0))))"#;
    let expect = expect_test::expect![[r#""OK (5 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with set-buffer inside (buffer switch and restore)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_buffer_switch_set_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion saves and restores current-buffer. Using set-buffer
    // inside should be undone upon exit.
    let form = r#"(let ((buf-a (generate-new-buffer "neovm--se-comp-A"))
                        (buf-b (generate-new-buffer "neovm--se-comp-B"))
                        (buf-c (generate-new-buffer "neovm--se-comp-C")))
      (unwind-protect
          (progn
            (with-current-buffer buf-a (insert "AAAA") (goto-char 3))
            (with-current-buffer buf-b (insert "BBBBBB") (goto-char 4))
            (with-current-buffer buf-c (insert "CC") (goto-char 1))
            ;; Start in buf-a
            (set-buffer buf-a)
            (let ((start-buf (buffer-name (current-buffer)))
                  (start-point (point)))
              (save-excursion
                ;; Switch to buf-b, move around
                (set-buffer buf-b)
                (goto-char (point-max))
                (let ((in-b-buf (buffer-name (current-buffer)))
                      (in-b-point (point)))
                  ;; Switch to buf-c
                  (set-buffer buf-c)
                  (goto-char (point-min))
                  (let ((in-c-buf (buffer-name (current-buffer)))
                        (in-c-point (point)))
                    (list in-b-buf in-b-point in-c-buf in-c-point))))
              ;; After save-excursion: back in buf-a at original point
              (list start-buf start-point
                    (buffer-name (current-buffer))
                    (point)
                    (equal start-buf (buffer-name (current-buffer)))
                    (= start-point (point)))))
        (kill-buffer buf-a)
        (kill-buffer buf-b)
        (kill-buffer buf-c)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"neovm--se-comp-A\" 3 \"neovm--se-comp-A\" 3 t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion + save-restriction: narrowing preserved independently
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_with_save_restriction_interleaved() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Interleave save-excursion and save-restriction in multiple orderings
    // to ensure each restores its own state without interfering.
    let form = r#"(with-temp-buffer
      (insert "0123456789abcdefghijklmnopqrstuvwxyz")
      (narrow-to-region 5 25)
      (goto-char 10)
      (let ((outer-min (point-min))
            (outer-max (point-max))
            (outer-pt (point)))
        ;; save-restriction wrapping save-excursion
        (save-restriction
          (widen)
          (let ((wide-min (point-min))
                (wide-max (point-max)))
            (save-excursion
              (goto-char (point-max))
              (narrow-to-region 1 10)
              (let ((inner-pt (point))
                    (inner-min (point-min))
                    (inner-max (point-max)))
                (list wide-min wide-max inner-pt inner-min inner-max)))
            ;; save-excursion restored point but save-restriction NOT yet restored
            (list (point) (point-min) (point-max))))
        ;; save-restriction restored narrowing
        (list (point) (= (point) outer-pt)
              (point-min) (= (point-min) outer-min)
              (point-max) (= (point-max) outer-max))))"#;
    let expect = expect_test::expect![[r#""OK (10 t 5 t 25 t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion + insert: point adjustment via markers
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_insert_point_adjustment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // save-excursion uses markers to track point. Insertions before the
    // saved point shift it forward; insertions after leave it unchanged.
    // Insertions AT the saved point: marker insertion type matters.
    let form = r#"(with-temp-buffer
      (insert "0123456789")
      (goto-char 6)
      (let ((original (point)))
        ;; Insert BEFORE saved point
        (let ((result-before
               (save-excursion
                 (goto-char 3)
                 (insert "XXX")
                 (buffer-string))))
          (let ((after-before (point)))
            ;; Reset: delete the inserted text
            (delete-region 3 6)
            (goto-char 6)
            ;; Insert AFTER saved point
            (let ((result-after
                   (save-excursion
                     (goto-char 8)
                     (insert "YYY")
                     (buffer-string))))
              (let ((after-after (point)))
                ;; Reset
                (delete-region 8 11)
                (goto-char 6)
                ;; Insert AT saved point
                (let ((result-at
                       (save-excursion
                         (goto-char 6)
                         (insert "ZZZ")
                         (buffer-string))))
                  (let ((after-at (point)))
                    (list original
                          result-before after-before
                          result-after after-after
                          result-at after-at)))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (6 \"01XXX23456789\" 9 \"0123456YYY789\" 6 \"01234ZZZ56789\" 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion + delete-region: point clamping behavior
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_delete_region_clamp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Deleting a region that encompasses the saved point should cause the
    // restored point to be clamped to the deletion boundary.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrst")
      ;; Case 1: delete region before saved point (point shifts left)
      (goto-char 10)
      (let ((r1 (progn
                  (save-excursion
                    (delete-region 3 7)
                    (buffer-string))
                  (point))))
        ;; Restore buffer
        (erase-buffer)
        (insert "abcdefghijklmnopqrst")
        ;; Case 2: delete region after saved point (point unchanged)
        (goto-char 5)
        (let ((r2 (progn
                    (save-excursion
                      (delete-region 10 15)
                      (buffer-string))
                    (point))))
          ;; Restore buffer
          (erase-buffer)
          (insert "abcdefghijklmnopqrst")
          ;; Case 3: delete region that contains saved point
          (goto-char 10)
          (let ((r3 (progn
                      (save-excursion
                        (delete-region 5 15)
                        (buffer-string))
                      (point))))
            (list r1 r2 r3)))))"#;
    let expect = expect_test::expect![[r#""OK (6 5 5)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with marker interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_marker_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Markers created inside save-excursion persist after it exits.
    // The saved point uses a marker internally. Test that user-created
    // markers and the internal point marker don't interfere.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz")
      (goto-char 10)
      (let ((m1 (point-marker))
            (m2 (copy-marker 15))
            (m3 (copy-marker 20)))
        ;; save-excursion with insertions that shift markers
        (save-excursion
          (goto-char 5)
          (insert "123")   ;; shifts m1, m2, m3 all forward by 3
          (let ((m1-inside (marker-position m1))
                (m2-inside (marker-position m2))
                (m3-inside (marker-position m3)))
            (list m1-inside m2-inside m3-inside)))
        ;; After restore: point is restored via marker (shifted)
        (let ((pt-after (point))
              (m1-after (marker-position m1))
              (m2-after (marker-position m2))
              (m3-after (marker-position m3)))
          ;; Create marker inside save-excursion, check it persists
          (let ((m4 nil))
            (save-excursion
              (goto-char 8)
              (setq m4 (point-marker))
              (goto-char 25))
            (list pt-after m1-after m2-after m3-after
                  (marker-position m4)
                  (marker-buffer m4)
                  (= (marker-position m4) 8))))))"#;
    let expect = expect_test::expect![[r#""OK (13 13 18 23 8 #<killed buffer> t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Error recovery: condition-case inside save-excursion
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_error_recovery() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // If an error occurs inside save-excursion and is caught by
    // condition-case (also inside), point should still be restored
    // when save-excursion exits normally.
    // Also test error thrown inside save-excursion caught outside.
    let form = r#"(with-temp-buffer
      (insert "abcdefghijklmnopqrstuvwxyz")
      (goto-char 10)
      ;; Case 1: error caught INSIDE save-excursion
      (let ((r1
             (save-excursion
               (goto-char 20)
               (condition-case err
                   (progn
                     (goto-char 5)
                     (signal 'error '("test error inside"))
                     'unreachable)
                 (error
                  (list 'caught (cadr err) (point)))))))
        ;; Point should be restored to 10
        (let ((pt1 (point)))
          ;; Case 2: error thrown inside save-excursion, caught OUTSIDE
          (goto-char 10)
          (let ((r2
                 (condition-case err
                     (save-excursion
                       (goto-char 25)
                       (signal 'error '("outer catch"))
                       'unreachable)
                   (error
                    (list 'outer-caught (cadr err))))))
            ;; Point should be restored even though error unwound save-excursion
            (let ((pt2 (point)))
              ;; Case 3: nested save-excursion, inner errors
              (goto-char 10)
              (let ((r3
                     (save-excursion
                       (goto-char 15)
                       (condition-case _
                           (save-excursion
                             (goto-char 25)
                             (signal 'error '("inner nested")))
                         (error (list 'inner-caught (point))))
                       ;; outer save-excursion body continues
                       (list (point)))))
                (list r1 pt1 r2 pt2 r3 (point))))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((caught \"test error inside\" 5) 10 (outer-caught \"outer catch\") 10 (15) 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion across multiple buffers with complex modifications
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_multi_buffer_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create three buffers, use save-excursion in each one with
    // cross-buffer operations (reading from one, writing to another),
    // and verify all points and buffers are correctly restored.
    let form = r#"(let ((buf-src (generate-new-buffer "neovm--se-src"))
                        (buf-dst (generate-new-buffer "neovm--se-dst"))
                        (buf-log (generate-new-buffer "neovm--se-log")))
      (unwind-protect
          (progn
            (with-current-buffer buf-src
              (insert "word1 word2 word3 word4 word5"))
            (with-current-buffer buf-dst
              (insert "DESTINATION: "))
            (with-current-buffer buf-log
              (insert "LOG:"))
            ;; From buf-src: extract words, write to buf-dst, log to buf-log
            (set-buffer buf-src)
            (goto-char 1)
            (let ((words nil))
              (save-excursion
                (goto-char (point-min))
                (while (re-search-forward "\\(word[0-9]+\\)" nil t)
                  (let ((w (match-string 1)))
                    (setq words (cons w words))
                    ;; Write to dst
                    (save-excursion
                      (set-buffer buf-dst)
                      (goto-char (point-max))
                      (insert " " w))
                    ;; Log to log buffer
                    (save-excursion
                      (set-buffer buf-log)
                      (goto-char (point-max))
                      (insert (format " found:%s" w))))))
              ;; After all save-excursions: back in buf-src at point 1
              (list
               (buffer-name (current-buffer))
               (point)
               (nreverse words)
               (with-current-buffer buf-dst (buffer-string))
               (with-current-buffer buf-log (buffer-string)))))
        (kill-buffer buf-src)
        (kill-buffer buf-dst)
        (kill-buffer buf-log)))"#;
    let expect = expect_test::expect![[
        r#""OK (\"neovm--se-src\" 1 (\"word1\" \"word2\" \"word3\" \"word4\" \"word5\") \"DESTINATION:  word1 word2 word3 word4 word5\" \"LOG: found:word1 found:word2 found:word3 found:word4 found:word5\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with repeated entry (loop) preserving point each iteration
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_loop_repeated_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Use save-excursion in a while loop where each iteration modifies
    // the buffer. Verify point is correctly restored each time despite
    // cumulative buffer changes shifting positions.
    let form = r#"(with-temp-buffer
      (insert "aaa bbb ccc ddd eee fff")
      (goto-char 5)
      (let ((positions nil)
            (count 0))
        (while (and (< count 5)
                    (save-excursion
                      (goto-char (point-min))
                      (re-search-forward "\\b[a-z]\\{3\\}\\b" nil t)))
          (setq count (1+ count))
          (setq positions (cons (point) positions))
          ;; Replace first 3-letter word with a 5-letter word (shifts positions)
          (save-excursion
            (goto-char (point-min))
            (when (re-search-forward "\\b[a-z]\\{3\\}\\b" nil t)
              (replace-match "XXXXX"))))
        ;; point should track via marker through all replacements
        (list (point)
              count
              (nreverse positions)
              (buffer-string))))"#;
    let expect =
        expect_test::expect![[r#""OK (7 5 (5 7 7 7 7) \"XXXXX XXXXX XXXXX XXXXX XXXXX fff\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-excursion with widen inside narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_save_excursion_widen_inside_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When narrowed, save-excursion + widen should allow accessing the
    // full buffer, but on exit point is restored relative to the
    // (still-narrowed) buffer.
    let form = r#"(with-temp-buffer
      (insert "LINE1\nLINE2\nLINE3\nLINE4\nLINE5\n")
      (goto-char 7)  ;; start of LINE2
      (narrow-to-region 7 19)  ;; LINE2 and LINE3
      (goto-char 8)
      (let ((narrow-pt (point))
            (narrow-min (point-min))
            (narrow-max (point-max)))
        (save-excursion
          (save-restriction
            (widen)
            ;; Now can access entire buffer
            (goto-char (point-max))
            (let ((wide-max-pt (point))
                  (full-text (buffer-string)))
              (goto-char 1)
              (list wide-max-pt (length full-text)))))
        ;; After save-excursion: point restored, narrowing intact
        (list (point) (= (point) narrow-pt)
              (point-min) (= (point-min) narrow-min)
              (point-max) (= (point-max) narrow-max)
              (buffer-substring (point-min) (point-max)))))"#;
    let expect = expect_test::expect![[r#""OK (8 t 7 t 19 t \"LINE2\\nLINE3\\n\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
