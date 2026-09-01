//! Advanced oracle parity tests for `delete-region` and region manipulation:
//! boundary deletions, narrowed buffer deletions, marker adjustment,
//! `delete-and-extract-region`, progressive shrinking, selective pattern
//! deletion, non-overlapping reverse-order deletions, and cut-and-paste
//! simulation.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// delete-region at various boundary positions (beginning, middle, end)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_boundary_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test deletion at beginning, middle, and end of buffer; verify point,
    // buffer-size, and remaining content after each operation.
    let form = r#"(let ((results nil))
  ;; Delete from beginning
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 5)
    (delete-region 1 4)
    (setq results (cons (list 'begin
                               (buffer-string)
                               (point)
                               (buffer-size))
                        results)))
  ;; Delete from middle
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 1)
    (delete-region 4 8)
    (setq results (cons (list 'middle
                               (buffer-string)
                               (point)
                               (buffer-size))
                        results)))
  ;; Delete from end
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 3)
    (delete-region 8 11)
    (setq results (cons (list 'end
                               (buffer-string)
                               (point)
                               (buffer-size))
                        results)))
  ;; Delete entire buffer
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (goto-char 5)
    (delete-region (point-min) (point-max))
    (setq results (cons (list 'all
                               (buffer-string)
                               (point)
                               (buffer-size))
                        results)))
  ;; Delete empty range (no-op)
  (with-temp-buffer
    (insert "ABCDEFGHIJ")
    (delete-region 5 5)
    (setq results (cons (list 'empty
                               (buffer-string)
                               (buffer-size))
                        results)))
  (nreverse results))"#;
    let expect = expect_test::expect![[
        r#""OK ((begin \"DEFGHIJ\" 2 7) (middle \"ABCHIJ\" 1 6) (end \"ABCDEFG\" 3 7) (all \"\" 1 0) (empty \"ABCDEFGHIJ\" 10))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-region on a narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_narrowed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow buffer, delete within narrowed region, verify that the full
    // buffer reflects the deletion after widening.
    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  (narrow-to-region 5 13)
  (let ((narrowed-before (buffer-string))
        (pmin (point-min))
        (pmax (point-max)))
    ;; Delete within narrowed region (positions relative to narrowed view)
    (delete-region (point-min) (+ (point-min) 4))
    (let ((narrowed-after (buffer-string))
          (new-pmin (point-min))
          (new-pmax (point-max)))
      (widen)
      (let ((full-after (buffer-string)))
        (list narrowed-before
              pmin pmax
              narrowed-after
              new-pmin new-pmax
              full-after
              (buffer-size))))))"#;
    let expect =
        expect_test::expect![[r#""OK (\"456789AB\" 5 13 \"89AB\" 5 9 \"012389ABCDEF\" 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-region interacting with markers (marker adjustment)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_marker_adjustment() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Create markers at various positions, delete a region, verify that
    // markers before the deletion stay put, markers inside get moved to
    // the deletion start, and markers after get adjusted.
    let form = r#"(with-temp-buffer
  (insert "AABBCCDDEE")
  (let ((m-before (copy-marker 2))
        (m-inside (copy-marker 5))
        (m-inside2 (copy-marker 7))
        (m-after (copy-marker 9))
        (m-end (copy-marker 11)))
    ;; Delete region [4, 8) which covers chars "BCCD"
    (delete-region 4 8)
    (let ((result (list
                   (marker-position m-before)
                   (marker-position m-inside)
                   (marker-position m-inside2)
                   (marker-position m-after)
                   (marker-position m-end)
                   (buffer-string)
                   (buffer-size))))
      ;; Cleanup markers
      (set-marker m-before nil)
      (set-marker m-inside nil)
      (set-marker m-inside2 nil)
      (set-marker m-after nil)
      (set-marker m-end nil)
      result)))"#;
    let expect = expect_test::expect![[r#""OK (2 4 4 5 7 \"AABDEE\" 6)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region preserving deleted text
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_and_extract_region_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract multiple regions sequentially, building a collection of
    // extracted fragments while the buffer shrinks.
    let form = r#"(with-temp-buffer
  (insert "[HEADER]body-content[FOOTER]")
  (let ((fragments nil))
    ;; Extract header tag
    (let ((h (delete-and-extract-region 1 9)))
      (setq fragments (cons (cons 'header h) fragments)))
    ;; Buffer is now: body-content[FOOTER]
    ;; Extract footer tag (now at different position)
    (let ((f (delete-and-extract-region
              (- (point-max) 8) (point-max))))
      (setq fragments (cons (cons 'footer f) fragments)))
    ;; Buffer is now: body-content
    (let ((body (buffer-string)))
      (setq fragments (cons (cons 'body body) fragments)))
    ;; Also test extract of empty range
    (let ((empty-extract (delete-and-extract-region 1 1)))
      (setq fragments (cons (cons 'empty empty-extract) fragments)))
    (list (nreverse fragments)
          (buffer-string)
          (buffer-size))))"#;
    let expect = expect_test::expect![[
        r#""OK (((header . \"[HEADER]\") (footer . \"[FOOTER]\") (body . \"body-content\") (empty . \"\")) \"body-content\" 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Repeated delete-region shrinking buffer progressively
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_progressive_shrink() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Delete 2 chars from the front repeatedly until the buffer is empty
    // or too small, collecting snapshots at each step.
    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOP")
  (let ((snapshots nil)
        (iteration 0))
    (while (and (> (buffer-size) 0) (< iteration 20))
      (let ((to-delete (min 2 (buffer-size))))
        (delete-region 1 (1+ to-delete))
        (setq snapshots
              (cons (list iteration
                         (buffer-string)
                         (buffer-size)
                         (point))
                    snapshots))
        (setq iteration (1+ iteration))))
    (nreverse snapshots)))"#;
    let expect = expect_test::expect![[
        r#""OK ((0 \"CDEFGHIJKLMNOP\" 14 15) (1 \"EFGHIJKLMNOP\" 12 13) (2 \"GHIJKLMNOP\" 10 11) (3 \"IJKLMNOP\" 8 9) (4 \"KLMNOP\" 6 7) (5 \"MNOP\" 4 5) (6 \"OP\" 2 3) (7 \"\" 0 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Selective deletion: delete all lines matching a pattern
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_selective_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert structured text, delete all lines starting with "DEBUG:",
    // keep a tally of deleted lines and their content.
    let form = r#"(with-temp-buffer
  (insert "INFO: Application started\n")
  (insert "DEBUG: Loading module A\n")
  (insert "INFO: Module A ready\n")
  (insert "DEBUG: Memory usage: 128MB\n")
  (insert "WARN: Disk space low\n")
  (insert "DEBUG: Cache hit ratio: 0.95\n")
  (insert "INFO: Processing request\n")
  (insert "DEBUG: Query took 12ms\n")
  (insert "ERROR: Connection refused\n")
  (let ((deleted-lines nil)
        (delete-count 0))
    (goto-char (point-min))
    (while (not (eobp))
      (if (looking-at "^DEBUG: ")
          (let ((line-text (buffer-substring
                            (line-beginning-position)
                            (line-end-position))))
            (setq deleted-lines (cons line-text deleted-lines))
            (setq delete-count (1+ delete-count))
            ;; Delete the line including its newline
            (delete-region (line-beginning-position)
                           (min (1+ (line-end-position)) (point-max))))
        (forward-line 1)))
    (list delete-count
          (nreverse deleted-lines)
          (buffer-string)
          (buffer-size))))"#;
    let expect = expect_test::expect![[
        r#""OK (4 (\"DEBUG: Loading module A\" \"DEBUG: Memory usage: 128MB\" \"DEBUG: Cache hit ratio: 0.95\" \"DEBUG: Query took 12ms\") \"INFO: Application started\\nINFO: Module A ready\\nWARN: Disk space low\\nINFO: Processing request\\nERROR: Connection refused\\n\" 119)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Delete non-overlapping regions in reverse order
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_reverse_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Define multiple non-overlapping regions to delete, process them from
    // rightmost to leftmost so positions remain valid.
    let form = r#"(with-temp-buffer
  (insert "AAA-BBB-CCC-DDD-EEE-FFF")
  (let ((original (buffer-string))
        ;; Regions to delete (1-indexed, inclusive start, exclusive end):
        ;; Remove "BBB" (5-8), "DDD" (13-16), "FFF" (21-24)
        ;; Process in reverse order to preserve earlier positions.
        (regions '((21 . 24) (13 . 16) (5 . 8)))
        (extracted nil))
    (dolist (reg regions)
      (let ((text (buffer-substring (car reg) (cdr reg))))
        (delete-region (car reg) (cdr reg))
        (setq extracted (cons text extracted))))
    (list original
          (buffer-string)
          (nreverse extracted)
          (buffer-size)
          ;; Verify dashes are now adjacent or at edges
          (let ((dash-count 0))
            (goto-char (point-min))
            (while (search-forward "-" nil t)
              (setq dash-count (1+ dash-count)))
            dash-count))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"AAA-BBB-CCC-DDD-EEE-FFF\" \"AAA--CCC--EEE-\" (\"FFF\" \"DDD\" \"BBB\") 14 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Cut-and-paste simulation (extract + insert elsewhere)
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_delete_region_cut_paste_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a cut-and-paste: extract text from one location and insert
    // it at another, multiple times, building a reordered document.
    let form = r#"(with-temp-buffer
  (insert "line-1: Alpha\n")
  (insert "line-2: Bravo\n")
  (insert "line-3: Charlie\n")
  (insert "line-4: Delta\n")
  (insert "line-5: Echo\n")
  (let ((original (buffer-string)))
    ;; Move line-3 to after line-1 (cut line-3, paste after line-1)
    ;; First find line-3
    (goto-char (point-min))
    (forward-line 2) ;; now at start of line-3
    (let ((cut-start (point)))
      (forward-line 1) ;; now at start of line-4
      (let ((cut-text (delete-and-extract-region cut-start (point))))
        ;; Now paste after line-1
        (goto-char (point-min))
        (forward-line 1) ;; end of line-1
        (insert cut-text)))
    (let ((after-first-move (buffer-string)))
      ;; Now move the last line to the very beginning
      (goto-char (point-max))
      (forward-line -1) ;; start of last line
      (let ((cut-start (point)))
        (let ((cut-text (delete-and-extract-region cut-start (point-max))))
          (goto-char (point-min))
          (insert cut-text)))
      (list original
            after-first-move
            (buffer-string)
            ;; Verify line count is preserved
            (let ((count 0))
              (goto-char (point-min))
              (while (not (eobp))
                (setq count (1+ count))
                (forward-line 1))
              count)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"line-1: Alpha\\nline-2: Bravo\\nline-3: Charlie\\nline-4: Delta\\nline-5: Echo\\n\" \"line-1: Alpha\\nline-3: Charlie\\nline-2: Bravo\\nline-4: Delta\\nline-5: Echo\\n\" \"line-5: Echo\\nline-1: Alpha\\nline-3: Charlie\\nline-2: Bravo\\nline-4: Delta\\n\" 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
