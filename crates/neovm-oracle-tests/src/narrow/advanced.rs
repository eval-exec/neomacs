//! Oracle parity tests for advanced narrowing patterns:
//! nested `save-restriction`/`widen`, marker behavior outside narrowed regions,
//! point clamping, `re-search-forward` within narrowed region,
//! section parsing via narrowing, and sequential narrow/widen cycles
//! with buffer modifications.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// nested save-restriction/widen: inner widen does not escape outer narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_nested_save_restriction_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (let ((results nil))
    ;; Outer narrowing: chars 5..15 = "EFGHIJKLMNO"
    (save-restriction
      (narrow-to-region 5 16)
      (setq results (cons (list 'outer-narrow (buffer-string)) results))
      ;; Inner narrowing: relative 3..7 within outer = "GHIJ"
      (save-restriction
        (narrow-to-region 7 11)
        (setq results (cons (list 'inner-narrow (buffer-string)) results))
        ;; Widen inside inner save-restriction — restores to outer narrowing
        (widen)
        (setq results (cons (list 'after-inner-widen (buffer-string)) results)))
      ;; Back to outer narrowing after inner save-restriction exits
      (setq results (cons (list 'after-inner-restore (buffer-string)) results)))
    ;; Fully widened
    (setq results (cons (list 'fully-wide (buffer-string)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((outer-narrow \"EFGHIJKLMNO\") (inner-narrow \"GHIJ\") (after-inner-widen \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\") (after-inner-restore \"EFGHIJKLMNO\") (fully-wide \"ABCDEFGHIJKLMNOPQRSTUVWXYZ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Marker behavior: markers outside narrowed region remain valid
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_marker_outside_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEF")
  ;; Place markers at various positions
  (let ((m1 (copy-marker 3))
        (m2 (copy-marker 8))
        (m3 (copy-marker 14)))
    ;; Narrow to middle: positions 5..11 = "56789A"
    (save-restriction
      (narrow-to-region 5 11)
      ;; Markers outside the narrowed region still have their absolute positions
      (let ((m1-pos (marker-position m1))
            (m2-pos (marker-position m2))
            (m3-pos (marker-position m3))
            (pmin (point-min))
            (pmax (point-max)))
        (list m1-pos m2-pos m3-pos pmin pmax
              ;; buffer-string only shows narrowed region
              (buffer-string))))))"#;
    let expect = expect_test::expect![[r#""OK (3 8 14 5 11 \"456789\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_narrow_to_region_bignum_start_saturates_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs editfns.c:Fnarrow_to_region validates START and END through
    // buffer.c:fix_position, so bignums saturate before the range check.
    let form = r#"(with-temp-buffer
  (insert "abc")
  (narrow-to-region 1000000000000000000000000000000 2))"#;
    let expect =
        expect_test::expect![[r#""ERR (args-out-of-range 1000000000000000000000000000000 2)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point clamping: goto-char outside narrowed region clamps to boundaries
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_point_clamping() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOP")
  ;; Point starts at end (17)
  (let ((before-narrow-point (point)))
    ;; Narrow to 5..10 = "EFGHI"
    (narrow-to-region 5 10)
    ;; Point was 17, outside the narrowed region — it gets clamped
    (let ((after-narrow-point (point))
          (pmin (point-min))
          (pmax (point-max)))
      ;; Try to goto-char beyond boundaries
      (goto-char 1)
      (let ((clamped-low (point)))
        (goto-char 100)
        (let ((clamped-high (point)))
          ;; goto-char within region works normally
          (goto-char 7)
          (let ((within (point)))
            (widen)
            (list before-narrow-point
                  after-narrow-point pmin pmax
                  clamped-low clamped-high within)))))))"#;
    let expect = expect_test::expect![[r#""OK (17 10 5 10 5 10 7)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// narrow-to-region + re-search-forward within narrowed region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_re_search_confined() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "apple:10 banana:20 cherry:30 date:40 elderberry:50")
  ;; Narrow to a substring that contains "banana:20 cherry:30"
  (save-restriction
    (narrow-to-region 10 30)
    (goto-char (point-min))
    (let ((matches nil))
      ;; Find all word:number patterns within the narrowed region
      (while (re-search-forward "\\([a-z]+\\):\\([0-9]+\\)" nil t)
        (setq matches
              (cons (list (match-string 1) (match-string 2))
                    matches)))
      ;; Verify that matches outside the region are NOT found
      (goto-char (point-min))
      (let ((apple-found (re-search-forward "apple" nil t))
            (elder-found (progn (goto-char (point-min))
                                (re-search-forward "elderberry" nil t))))
        (list (nreverse matches) apple-found elder-found)))))"#;
    let expect =
        expect_test::expect![[r#""OK (((\"banana\" \"20\") (\"cherry\" \"30\")) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Parse sections of a document by narrowing to each section
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_parse_document_sections() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "[HEADER]\n")
  (insert "title=My Document\n")
  (insert "author=Alice\n")
  (insert "[BODY]\n")
  (insert "Line one of body.\n")
  (insert "Line two of body.\n")
  (insert "Line three of body.\n")
  (insert "[FOOTER]\n")
  (insert "copyright=2026\n")
  (goto-char (point-min))
  ;; Collect section boundaries
  (let ((sections nil)
        (boundaries nil))
    ;; First pass: find all section headers and their positions
    (while (re-search-forward "^\\[\\([A-Z]+\\)\\]$" nil t)
      (setq boundaries
            (cons (cons (match-string 1) (1+ (match-end 0)))
                  boundaries)))
    (setq boundaries (nreverse boundaries))
    ;; Second pass: narrow to each section and extract key-value pairs or lines
    (let ((i 0))
      (while (< i (length boundaries))
        (let* ((entry (nth i boundaries))
               (name (car entry))
               (start (cdr entry))
               (end (if (< (1+ i) (length boundaries))
                        (save-excursion
                          (goto-char (cdr (nth (1+ i) boundaries)))
                          (forward-line -1)
                          (line-beginning-position))
                      (point-max))))
          (save-restriction
            (narrow-to-region start end)
            (goto-char (point-min))
            (let ((content nil))
              (while (not (eobp))
                (let ((line (buffer-substring
                             (line-beginning-position)
                             (line-end-position))))
                  (unless (string= line "")
                    (setq content (cons line content))))
                (forward-line 1))
              (setq sections
                    (cons (list name (nreverse content)) sections)))))
        (setq i (1+ i))))
    (nreverse sections)))"#;
    let expect = expect_test::expect![[
        r#""OK ((\"HEADER\" (\"title=My Document\" \"author=Alice\")) (\"BODY\" (\"Line one of body.\" \"Line two of body.\" \"Line three of body.\")) (\"FOOTER\" (\"copyright=2026\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Sequential narrow/widen cycles with buffer modifications between them
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_sequential_modify_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "aaaa|bbbb|cccc|dddd")
  (let ((snapshots nil))
    ;; Cycle 1: narrow to first segment, upcase it
    (save-restriction
      (narrow-to-region 1 5)
      (goto-char (point-min))
      (while (not (eobp))
        (let ((c (char-after)))
          (delete-char 1)
          (insert (upcase (string c)))))
      (setq snapshots (cons (list 'cycle1-narrow (buffer-string)) snapshots)))
    (setq snapshots (cons (list 'cycle1-wide (buffer-string)) snapshots))
    ;; Cycle 2: narrow to third segment (positions shifted if modified),
    ;; replace content entirely
    (save-restriction
      (narrow-to-region 11 15)
      (delete-region (point-min) (point-max))
      (insert "ZZZZ")
      (setq snapshots (cons (list 'cycle2-narrow (buffer-string)) snapshots)))
    (setq snapshots (cons (list 'cycle2-wide (buffer-string)) snapshots))
    ;; Cycle 3: narrow to second segment, reverse the characters
    (save-restriction
      (narrow-to-region 6 10)
      (let ((chars nil))
        (goto-char (point-min))
        (while (not (eobp))
          (setq chars (cons (char-after) chars))
          (forward-char 1))
        (delete-region (point-min) (point-max))
        (dolist (c chars)
          (insert (string c))))
      (setq snapshots (cons (list 'cycle3-narrow (buffer-string)) snapshots)))
    (setq snapshots (cons (list 'cycle3-wide (buffer-string)) snapshots))
    (nreverse snapshots)))"#;
    let expect = expect_test::expect![[
        r#""OK ((cycle1-narrow \"AAAA\") (cycle1-wide \"AAAA|bbbb|cccc|dddd\") (cycle2-narrow \"ZZZZ\") (cycle2-wide \"AAAA|bbbb|ZZZZ|dddd\") (cycle3-narrow \"bbbb\") (cycle3-wide \"AAAA|bbbb|ZZZZ|dddd\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing with deletion that shrinks the region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_delete_within_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog")
  (save-restriction
    ;; Narrow to "quick brown fox" (positions 5..19)
    (narrow-to-region 5 19)
    (let ((before (buffer-string))
          (before-size (- (point-max) (point-min))))
      ;; Delete "brown " from within the narrowed region
      (goto-char (point-min))
      (when (re-search-forward "brown " nil t)
        (replace-match ""))
      (let ((after (buffer-string))
            (after-size (- (point-max) (point-min))))
        ;; Widen and check full buffer
        (widen)
        (list before before-size
              after after-size
              (buffer-string))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"quick brown fo\" 14 \"quick fo\" 8 \"The quick fox jumps over the lazy dog\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: tabulate data from narrowed sub-regions with accumulator
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_tabulate_subregions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Name: Alice\nAge: 30\nCity: Paris\n---\n")
  (insert "Name: Bob\nAge: 25\nCity: London\n---\n")
  (insert "Name: Carol\nAge: 35\nCity: Tokyo\n---\n")
  (goto-char (point-min))
  ;; Find each record delimited by "---" and extract fields
  (let ((records nil)
        (record-start (point-min)))
    (while (search-forward "---" nil t)
      (let ((record-end (match-beginning 0)))
        (save-restriction
          (narrow-to-region record-start record-end)
          (goto-char (point-min))
          (let ((fields nil))
            (while (re-search-forward "^\\([A-Za-z]+\\): \\(.+\\)$" nil t)
              (setq fields
                    (cons (cons (match-string 1) (match-string 2))
                          fields)))
            (setq records (cons (nreverse fields) records))))
        ;; Move past the "---\n"
        (forward-line 1)
        (setq record-start (point))))
    ;; Return records sorted by Name
    (sort (nreverse records)
          (lambda (a b)
            (string-lessp (cdr (assoc "Name" a))
                          (cdr (assoc "Name" b)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (((\"Name\" . \"Alice\") (\"Age\" . \"30\") (\"City\" . \"Paris\")) ((\"Name\" . \"Bob\") (\"Age\" . \"25\") (\"City\" . \"London\")) ((\"Name\" . \"Carol\") (\"Age\" . \"35\") (\"City\" . \"Tokyo\")) nil nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing with save-excursion interaction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_with_save_excursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Line-1\nLine-2\nLine-3\nLine-4\nLine-5\n")
  (goto-char 1)
  ;; save-excursion inside narrowing: point restored relative to full buffer
  (let ((results nil))
    (save-restriction
      (narrow-to-region 8 22)  ;; "Line-2\nLine-3\n"
      (setq results (cons (list 'narrowed (buffer-string) (point)) results))
      (save-excursion
        (goto-char (point-max))
        (setq results (cons (list 'excursion-at-max (point)) results)))
      ;; After save-excursion, point is restored
      (setq results (cons (list 'after-excursion (point)) results))
      ;; Insert something to test save-excursion + narrowing combo
      (goto-char (point-min))
      (insert ">>")
      (setq results (cons (list 'after-insert (buffer-string) (point)) results)))
    ;; After save-restriction, widened again
    (setq results (cons (list 'widened (buffer-string)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((narrowed \"Line-2\\nLine-3\\n\" 8) (excursion-at-max 22) (after-excursion 8) (after-insert \">>Line-2\\nLine-3\\n\" 10) (widened \"Line-1\\n>>Line-2\\nLine-3\\nLine-4\\nLine-5\\n\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
