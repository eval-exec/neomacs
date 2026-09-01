//! Oracle parity tests for advanced narrow/widen patterns:
//! nested narrowing with multiple levels, narrowing with markers,
//! narrowing + search operations, narrowing + point operations,
//! save-restriction + widen patterns, narrowing in multiple buffers,
//! and buffer-size in narrowed region.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Triple-nested narrowing with progressive restriction and widen at each level
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_triple_nested_progressive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop")
  (let ((results nil))
    ;; Level 0: full buffer
    (setq results (cons (list 'L0 (point-min) (point-max)
                               (buffer-size) (buffer-string)) results))
    (save-restriction
      ;; Level 1: narrow to positions 6..40
      (narrow-to-region 6 40)
      (setq results (cons (list 'L1 (point-min) (point-max)
                                 (buffer-size)
                                 (buffer-substring (point-min) (min (+ (point-min) 10) (point-max))))
                          results))
      (save-restriction
        ;; Level 2: narrow to positions 10..30 (within L1)
        (narrow-to-region 10 30)
        (setq results (cons (list 'L2 (point-min) (point-max)
                                   (buffer-size)
                                   (buffer-string)) results))
        (save-restriction
          ;; Level 3: narrow to positions 15..25 (within L2)
          (narrow-to-region 15 25)
          (setq results (cons (list 'L3 (point-min) (point-max)
                                     (buffer-size)
                                     (buffer-string)) results))
          ;; Widen inside L3's save-restriction: goes back to full buffer
          (widen)
          (setq results (cons (list 'L3-widen (point-min) (point-max)
                                     (buffer-size)) results)))
        ;; After L3's save-restriction exits: back to L2 restriction
        (setq results (cons (list 'L2-restored (point-min) (point-max)
                                   (buffer-string)) results)))
      ;; After L2's save-restriction exits: back to L1 restriction
      (setq results (cons (list 'L1-restored (point-min) (point-max)
                                 (buffer-size)) results)))
    ;; After L1's save-restriction exits: full buffer
    (setq results (cons (list 'L0-restored (point-min) (point-max)
                               (buffer-size)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((L0 1 53 52 \"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnop\") (L1 6 40 52 \"56789ABCDE\") (L2 10 30 52 \"9ABCDEFGHIJKLMNOPQRS\") (L3 15 25 52 \"EFGHIJKLMN\") (L3-widen 1 53 52) (L2-restored 10 30 \"9ABCDEFGHIJKLMNOPQRS\") (L1-restored 6 40 52) (L0-restored 1 53 52))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Markers track positions across narrowing, insertion, and widening
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_markers_across_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "ABCDEFGHIJKLMNOPQRSTUVWXYZ")
  (let ((m1 (copy-marker 5))
        (m2 (copy-marker 10))
        (m3 (copy-marker 20))
        (m4 (copy-marker 25))
        (results nil))
    ;; Narrow to region around m2
    (save-restriction
      (narrow-to-region 8 22)
      ;; All markers still have their absolute positions
      (setq results (cons (list 'markers-in-narrow
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4))
                          results))
      ;; Insert text at m2's position - this shifts m3 and m4
      (goto-char (marker-position m2))
      (insert "***INSERTED***")
      (setq results (cons (list 'after-insert
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4)
                                 (point-min) (point-max)
                                 (buffer-size))
                          results))
      ;; Delete text between m2 and its shifted position
      (goto-char (marker-position m2))
      (delete-char 5)
      (setq results (cons (list 'after-delete
                                 (marker-position m1)
                                 (marker-position m2)
                                 (marker-position m3)
                                 (marker-position m4))
                          results)))
    ;; Widened: check full buffer and marker positions
    (setq results (cons (list 'widened
                               (marker-position m1)
                               (marker-position m2)
                               (marker-position m3)
                               (marker-position m4)
                               (buffer-string))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((markers-in-narrow 5 10 20 25) (after-insert 5 10 34 39 8 36 40) (after-delete 5 10 29 34) (widened 5 10 29 34 \"ABCDEFGHISERTED***JKLMNOPQRSTUVWXYZ\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing + search: search-forward, re-search-forward confined to region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_search_confinement() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "cat dog cat fish cat bird cat mouse cat snake")
  (let ((results nil))
    ;; Count "cat" in full buffer
    (goto-char (point-min))
    (let ((full-count 0))
      (while (search-forward "cat" nil t)
        (setq full-count (1+ full-count)))
      (setq results (cons (list 'full-count full-count) results)))
    ;; Narrow to middle portion and count "cat" there
    (save-restriction
      (narrow-to-region 10 35)
      (goto-char (point-min))
      (let ((narrow-count 0)
            (narrow-positions nil))
        (while (search-forward "cat" nil t)
          (setq narrow-count (1+ narrow-count))
          (setq narrow-positions (cons (point) narrow-positions)))
        (setq results (cons (list 'narrow-count narrow-count
                                   'positions (nreverse narrow-positions)
                                   'visible (buffer-string))
                            results)))
      ;; re-search-forward with groups inside narrow region
      (goto-char (point-min))
      (let ((re-matches nil))
        (while (re-search-forward "\\([a-z]+\\)" nil t)
          (setq re-matches (cons (match-string 1) re-matches)))
        (setq results (cons (list 're-matches (nreverse re-matches)) results)))
      ;; Verify search cannot find text outside narrow region
      (goto-char (point-min))
      (let ((found-outside (search-forward "snake" nil t)))
        (setq results (cons (list 'snake-outside-narrow found-outside) results))))
    ;; After widening, search finds everything again
    (goto-char (point-min))
    (let ((after-widen-found (search-forward "snake" nil t)))
      (setq results (cons (list 'snake-after-widen (not (null after-widen-found))) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((full-count 5) (narrow-count 2 positions (21 30) visible \"at fish cat bird cat mous\") (re-matches (\"at\" \"fish\" \"cat\" \"bird\" \"cat\" \"mous\")) (snake-outside-narrow nil) (snake-after-widen t))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Point clamping, goto-char, beginning/end-of-line under narrowing
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_point_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Line-1\nLine-2\nLine-3\nLine-4\nLine-5\nLine-6\nLine-7\n")
  (let ((results nil))
    ;; Place point at beginning of Line-4
    (goto-char (point-min))
    (forward-line 3)
    (let ((line4-start (point)))
      ;; Narrow to lines 3-5 (roughly)
      (goto-char (point-min))
      (forward-line 2)
      (let ((narrow-start (point)))
        (forward-line 3)
        (let ((narrow-end (point)))
          (narrow-to-region narrow-start narrow-end)
          ;; Point was at line4-start, which is inside the region
          (setq results (cons (list 'point-after-narrow (point)
                                     'pmin (point-min) 'pmax (point-max))
                              results))
          ;; goto-char below point-min clamps
          (goto-char 1)
          (setq results (cons (list 'clamped-low (point) (= (point) (point-min))) results))
          ;; goto-char above point-max clamps
          (goto-char 9999)
          (setq results (cons (list 'clamped-high (point) (= (point) (point-max))) results))
          ;; beginning-of-line / end-of-line within narrow region
          (goto-char (point-min))
          (forward-line 1)
          (let ((mid-point (point)))
            (beginning-of-line)
            (let ((bol (point)))
              (end-of-line)
              (let ((eol (point)))
                (setq results (cons (list 'bol-eol mid-point bol eol
                                           (buffer-substring bol eol))
                                    results)))))
          ;; forward-line return value at boundary
          (goto-char (point-max))
          (let ((fl-result (forward-line 1)))
            (setq results (cons (list 'forward-line-at-end fl-result (point)) results)))
          (goto-char (point-min))
          (let ((fl-result (forward-line -1)))
            (setq results (cons (list 'forward-line-at-start fl-result (point)) results)))
          ;; buffer-substring within narrow bounds
          (setq results (cons (list 'narrow-content (buffer-string)) results))
          (widen)
          (setq results (cons (list 'full-content-len (buffer-size)) results)))))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((point-after-narrow 36 pmin 15 pmax 36) (clamped-low 15 t) (clamped-high 36 t) (bol-eol 22 22 28 \"Line-4\") (forward-line-at-end 1 36) (forward-line-at-start -1 15) (narrow-content \"Line-3\\nLine-4\\nLine-5\\n\") (full-content-len 49))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// save-restriction + widen: ensures widen is temporary within save-restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_save_restriction_widen_pattern() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "Alpha Beta Gamma Delta Epsilon Zeta Eta Theta")
  (let ((results nil))
    ;; First narrow to "Gamma Delta Epsilon"
    (narrow-to-region 12 31)
    (setq results (cons (list 'initial-narrow (buffer-string)) results))
    ;; Use save-restriction + widen to peek at full buffer
    (let ((full-text nil)
          (full-size nil))
      (save-restriction
        (widen)
        (setq full-text (buffer-string))
        (setq full-size (buffer-size))
        ;; Do some operation on full buffer
        (goto-char (point-min))
        (let ((found (search-forward "Theta" nil t)))
          (setq results (cons (list 'found-theta-in-widen (not (null found))) results))))
      ;; After save-restriction: back to narrow
      (setq results (cons (list 'back-to-narrow (buffer-string)
                                 'same (equal (buffer-string) "Gamma Delta Epsilon"))
                          results))
      (setq results (cons (list 'full-peek full-text full-size) results)))
    ;; Nested: narrow further, then save-restriction+widen
    (narrow-to-region (+ (point-min) 6) (- (point-max) 8))
    (setq results (cons (list 'double-narrow (buffer-string)) results))
    (save-restriction
      (widen)
      ;; widen goes to FULL buffer, not just the outer narrow
      (setq results (cons (list 'widen-from-double (buffer-size)
                                 (= (point-min) 1))
                          results)))
    ;; Back to double-narrow
    (setq results (cons (list 'double-narrow-restored (buffer-string)) results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((initial-narrow \"Gamma Delta Epsilon\") (found-theta-in-widen t) (back-to-narrow \"Gamma Delta Epsilon\" same t) (full-peek \"Alpha Beta Gamma Delta Epsilon Zeta Eta Theta\" 45) (double-narrow \"Delta\") (widen-from-double 45 t) (double-narrow-restored \"Delta\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing in multiple buffers: each buffer has independent restriction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_multiple_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((buf-a (generate-new-buffer " *test-narrow-a*"))
      (buf-b (generate-new-buffer " *test-narrow-b*"))
      (results nil))
  (unwind-protect
      (progn
        ;; Set up buffer A
        (with-current-buffer buf-a
          (insert "AAAAA-BBBBB-CCCCC-DDDDD-EEEEE")
          (narrow-to-region 7 18))
        ;; Set up buffer B
        (with-current-buffer buf-b
          (insert "11111-22222-33333-44444-55555")
          (narrow-to-region 13 24))
        ;; Read from buffer A
        (with-current-buffer buf-a
          (setq results (cons (list 'buf-a-narrow
                                     (buffer-string)
                                     (point-min) (point-max)
                                     (buffer-size))
                              results)))
        ;; Read from buffer B - its narrowing is independent
        (with-current-buffer buf-b
          (setq results (cons (list 'buf-b-narrow
                                     (buffer-string)
                                     (point-min) (point-max)
                                     (buffer-size))
                              results)))
        ;; Widen buffer A, buffer B stays narrowed
        (with-current-buffer buf-a
          (widen)
          (setq results (cons (list 'buf-a-widened
                                     (buffer-string)
                                     (point-min) (point-max))
                              results)))
        (with-current-buffer buf-b
          (setq results (cons (list 'buf-b-still-narrow
                                     (buffer-string)
                                     (point-min) (point-max))
                              results)))
        ;; Modify in narrowed buffer B, check buffer A unaffected
        (with-current-buffer buf-b
          (goto-char (point-min))
          (insert "XX")
          (setq results (cons (list 'buf-b-after-insert
                                     (buffer-string)
                                     (buffer-size))
                              results)))
        (with-current-buffer buf-a
          (setq results (cons (list 'buf-a-unchanged
                                     (buffer-string))
                              results)))
        (nreverse results))
    (kill-buffer buf-a)
    (kill-buffer buf-b)))"#;
    let expect = expect_test::expect![[
        r#""OK ((buf-a-narrow \"BBBBB-CCCCC\" 7 18 29) (buf-b-narrow \"33333-44444\" 13 24 29) (buf-a-widened \"AAAAA-BBBBB-CCCCC-DDDDD-EEEEE\" 1 30) (buf-b-still-narrow \"33333-44444\" 13 24) (buf-b-after-insert \"XX33333-44444\" 31) (buf-a-unchanged \"AAAAA-BBBBB-CCCCC-DDDDD-EEEEE\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-size vs region size under narrowing, with insert/delete tracking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_buffer_size_tracking() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "The quick brown fox jumps over the lazy dog and runs away fast")
  (let ((results nil)
        (full-size (buffer-size)))
    (setq results (cons (list 'full-size full-size
                               'full-pmin (point-min)
                               'full-pmax (point-max)
                               'size-eq-pmax-minus-pmin
                               (= full-size (1- (point-max))))
                        results))
    (save-restriction
      ;; Narrow to "brown fox jumps over"
      (narrow-to-region 11 30)
      (let ((narrow-size (buffer-size))
            (narrow-region (- (point-max) (point-min))))
        (setq results (cons (list 'narrow-size narrow-size
                                   'narrow-region narrow-region
                                   'size-eq-region (= narrow-size narrow-region))
                            results))
        ;; Insert text: both buffer-size and region grow
        (goto-char (point-min))
        (insert "***")
        (let ((after-ins-size (buffer-size))
              (after-ins-region (- (point-max) (point-min))))
          (setq results (cons (list 'after-insert
                                     'buf-size after-ins-size
                                     'region after-ins-region
                                     'grew-by-3 (= after-ins-size (+ narrow-size 3)))
                              results)))
        ;; Delete text: both shrink
        (goto-char (point-min))
        (delete-char 6)
        (let ((after-del-size (buffer-size))
              (after-del-region (- (point-max) (point-min))))
          (setq results (cons (list 'after-delete
                                     'buf-size after-del-size
                                     'region after-del-region)
                              results)))
        ;; Widen inside save-restriction to check total buffer size changed
        (let ((current-narrow-content (buffer-string)))
          (save-restriction
            (widen)
            (setq results (cons (list 'widen-peek
                                       'total-size (buffer-size)
                                       'original-total full-size
                                       'delta (- (buffer-size) full-size))
                                results)))
          (setq results (cons (list 'narrow-content current-narrow-content) results)))))
    ;; After outer save-restriction
    (setq results (cons (list 'final-size (buffer-size)
                               'final-content (buffer-string))
                        results))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((full-size 62 full-pmin 1 full-pmax 63 size-eq-pmax-minus-pmin t) (narrow-size 62 narrow-region 19 size-eq-region nil) (after-insert buf-size 65 region 22 grew-by-3 t) (after-delete buf-size 59 region 16) (widen-peek total-size 59 original-total 62 delta -3) (narrow-content \"wn fox jumps ove\") (final-size 59 final-content \"The quick wn fox jumps over the lazy dog and runs away fast\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Narrowing with replace-regexp-in-string simulation: search + replace cycles
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_narrow_widen_search_replace_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  (insert "foo=1;bar=2;baz=3;qux=4;foo=5;bar=6;baz=7;qux=8")
  (let ((results nil))
    ;; Process first half: replace values with doubled values
    (save-restriction
      (narrow-to-region 1 25)
      (setq results (cons (list 'first-half-before (buffer-string)) results))
      (goto-char (point-min))
      (while (re-search-forward "=\\([0-9]+\\)" nil t)
        (let ((val (string-to-number (match-string 1))))
          (replace-match (concat "=" (number-to-string (* val 2))))))
      (setq results (cons (list 'first-half-after (buffer-string)) results)))
    ;; Process second half: replace values with tripled values
    (save-restriction
      (narrow-to-region 25 (point-max))
      (setq results (cons (list 'second-half-before (buffer-string)) results))
      (goto-char (point-min))
      (while (re-search-forward "=\\([0-9]+\\)" nil t)
        (let ((val (string-to-number (match-string 1))))
          (replace-match (concat "=" (number-to-string (* val 3))))))
      (setq results (cons (list 'second-half-after (buffer-string)) results)))
    ;; Full buffer: first half doubled, second half tripled
    (setq results (cons (list 'full-result (buffer-string)) results))
    ;; Extract all key=value pairs from full buffer
    (goto-char (point-min))
    (let ((pairs nil))
      (while (re-search-forward "\\([a-z]+\\)=\\([0-9]+\\)" nil t)
        (setq pairs (cons (cons (match-string 1)
                                (string-to-number (match-string 2)))
                          pairs)))
      (setq results (cons (list 'all-pairs (nreverse pairs)) results)))
    (nreverse results)))"#;
    let expect = expect_test::expect![[
        r#""OK ((first-half-before \"foo=1;bar=2;baz=3;qux=4;\") (first-half-after \"foo=2;bar=4;baz=6;qux=8;\") (second-half-before \"foo=5;bar=6;baz=7;qux=8\") (second-half-after \"foo=15;bar=18;baz=21;qux=24\") (full-result \"foo=2;bar=4;baz=6;qux=8;foo=15;bar=18;baz=21;qux=24\") (all-pairs ((\"foo\" . 2) (\"bar\" . 4) (\"baz\" . 6) (\"qux\" . 8) (\"foo\" . 15) (\"bar\" . 18) (\"baz\" . 21) (\"qux\" . 24))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
