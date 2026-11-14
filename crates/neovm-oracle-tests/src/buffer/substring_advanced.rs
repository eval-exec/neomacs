//! Advanced oracle parity tests for `buffer-substring` and related operations.
//!
//! Tests buffer-substring with properties, narrowing, multibyte chars,
//! delete-and-extract-region, boundary conditions, diff-like comparison,
//! region reconstruction, and buffer-string equivalence.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// buffer-substring with properties vs without
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_properties_vs_no_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert text with properties, then compare substring with and without them
    let form = r#"(with-temp-buffer
                    (insert "hello world")
                    (put-text-property 3 8 'face 'bold)
                    (put-text-property 1 5 'custom-prop 'value1)
                    (let ((with-props (buffer-substring 1 12))
                          (no-props (buffer-substring-no-properties 1 12)))
                      ;; Text content should be the same
                      (list (string= with-props no-props)
                            ;; no-props should have no text properties
                            (null (text-properties-at 0 no-props))
                            ;; with-props should preserve properties
                            (not (null (text-properties-at 0 with-props)))
                            (get-text-property 3 'face with-props)
                            (get-text-property 0 'custom-prop with-props)
                            ;; Verify specific property ranges survived
                            (get-text-property 5 'face with-props)
                            (null (get-text-property 0 'face with-props)))))"#;
    let expect = expect_test::expect![r#""OK (t t t bold value1 bold t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-substring on narrowed buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_narrowed_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Narrow buffer and verify buffer-substring works within narrowed bounds
    let form = r#"(with-temp-buffer
                    (insert "ABCDEFGHIJKLMNOP")
                    ;; Narrow to "EFGHIJKL" (positions 5-12)
                    (narrow-to-region 5 13)
                    (let ((narrowed-str (buffer-substring (point-min) (point-max)))
                          (sub (buffer-substring 2 5))
                          (pmin (point-min))
                          (pmax (point-max)))
                      ;; Widen and compare
                      (widen)
                      (let ((full (buffer-substring 1 17)))
                        (list narrowed-str
                              sub
                              pmin
                              pmax
                              full
                              ;; Verify narrowed substring is a slice of the full
                              (string= narrowed-str
                                       (substring full 4 12))))))"#;
    let expect = expect_test::expect![r#""ERR (args-out-of-range #<killed buffer> 2 5)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-substring with multibyte characters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_multibyte_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Multibyte characters: positions are character-based, not byte-based
    let form = r#"(with-temp-buffer
                    (insert "café日本語xyz")
                    (let ((full (buffer-string))
                          (cafe (buffer-substring 1 5))
                          (nihon (buffer-substring 6 9))
                          (mixed (buffer-substring 4 10))
                          (len (- (point-max) (point-min))))
                      (list full
                            cafe
                            nihon
                            mixed
                            len
                            ;; Verify character counts
                            (length full)
                            (length cafe)
                            (length nihon)
                            ;; Verify string-bytes differs from string length for multibyte
                            (> (string-bytes full) (length full)))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"café日本語xyz\" \"café\" \"本語x\" \"é日本語xy\" 10 10 4 3 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// delete-and-extract-region vs buffer-substring + delete-region
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_delete_and_extract_vs_manual() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify delete-and-extract-region is equivalent to
    // buffer-substring followed by delete-region
    let form = r#"(let ((text "the quick brown fox jumps over the lazy dog"))
                    ;; Method 1: delete-and-extract-region
                    (let (extracted1 remaining1)
                      (with-temp-buffer
                        (insert text)
                        (setq extracted1 (delete-and-extract-region 5 15))
                        (setq remaining1 (buffer-string)))
                      ;; Method 2: buffer-substring + delete-region
                      (let (extracted2 remaining2)
                        (with-temp-buffer
                          (insert text)
                          (setq extracted2 (buffer-substring 5 15))
                          (delete-region 5 15)
                          (setq remaining2 (buffer-string)))
                        (list (string= extracted1 extracted2)
                              (string= remaining1 remaining2)
                              extracted1
                              remaining1))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t \"quick brow\" \"the n fox jumps over the lazy dog\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-substring at boundary conditions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_boundary_conditions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test edge cases: point-min, point-max, equal positions, reversed args
    let form = r#"(with-temp-buffer
                    (insert "abcdef")
                    (let ((full (buffer-substring (point-min) (point-max)))
                          ;; Equal start and end → empty string
                          (empty1 (buffer-substring 3 3))
                          (empty2 (buffer-substring (point-min) (point-min)))
                          (empty3 (buffer-substring (point-max) (point-max)))
                          ;; Reversed args: buffer-substring handles start > end
                          (reversed (buffer-substring 5 2))
                          (normal (buffer-substring 2 5))
                          ;; Single char at boundaries
                          (first-char (buffer-substring 1 2))
                          (last-char (buffer-substring 6 7)))
                      (list full
                            empty1
                            empty2
                            empty3
                            ;; Reversed should equal normal
                            (string= reversed normal)
                            reversed
                            first-char
                            last-char)))"#;
    let expect =
        expect_test::expect![[r#""OK (\"abcdef\" \"\" \"\" \"\" t \"bcd\" \"a\" \"f\")""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: line-by-line diff using buffer-substring
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_line_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Split two texts into lines, compare line-by-line, report differences
    let form = r#"(let ((split-lines
                         (lambda (text)
                           (let ((lines nil)
                                 (start 0)
                                 (i 0))
                             (while (< i (length text))
                               (when (= (aref text i) ?\n)
                                 (setq lines (cons (substring text start i) lines))
                                 (setq start (1+ i)))
                               (setq i (1+ i)))
                             ;; Last line (no trailing newline)
                             (when (< start (length text))
                               (setq lines (cons (substring text start) lines)))
                             (nreverse lines))))
                        (text-a "line one\nline two\nline three\nline four\nline five")
                        (text-b "line one\nLINE TWO\nline three\nline 4\nline five\nline six"))
                    (let ((lines-a (funcall split-lines text-a))
                          (lines-b (funcall split-lines text-b))
                          (diffs nil)
                          (i 0))
                      ;; Compare corresponding lines
                      (let ((a lines-a)
                            (b lines-b))
                        (while (or a b)
                          (let ((la (car a))
                                (lb (car b)))
                            (cond
                             ((and la lb (string= la lb))
                              nil) ;; same, skip
                             ((and la lb)
                              (setq diffs (cons (list 'changed i la lb) diffs)))
                             ((and la (null lb))
                              (setq diffs (cons (list 'deleted i la) diffs)))
                             ((and (null la) lb)
                              (setq diffs (cons (list 'added i lb) diffs)))))
                          (setq a (cdr a) b (cdr b) i (1+ i))))
                      (nreverse diffs)))"#;
    let expect = expect_test::expect![[
        r#""OK ((changed 1 \"line two\" \"LINE TWO\") (changed 3 \"line four\" \"line 4\") (added 5 \"line six\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: extract and reconstruct buffer regions
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_substring_extract_reconstruct() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Extract non-overlapping regions, reverse their order, reconstruct buffer
    let form = r#"(with-temp-buffer
                    (insert "AABBCCDDEE")
                    ;; Extract 3 regions: [1,3), [4,6), [7,9)
                    (let ((r1 (buffer-substring 1 3))
                          (r2 (buffer-substring 4 6))
                          (r3 (buffer-substring 7 9)))
                      ;; Build reversed concatenation: r3 + gap3 + r2 + gap2 + r1 + rest
                      (let ((gap1 (buffer-substring 3 4))
                            (gap2 (buffer-substring 6 7))
                            (tail (buffer-substring 9 11)))
                        ;; Reconstruct in reverse region order
                        (let ((reversed (concat r3 gap2 r2 gap1 r1 tail))
                              (original (buffer-string)))
                          (list r1 r2 r3
                                gap1 gap2 tail
                                reversed
                                original
                                ;; Verify lengths match
                                (= (length reversed) (length original)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (\"AA\" \"BC\" \"DD\" \"B\" \"C\" \"EE\" \"DDCBCBAAEE\" \"AABBCCDDEE\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// buffer-string vs buffer-substring full range equivalence
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_buffer_string_vs_substring_full_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // buffer-string should equal buffer-substring of the full range,
    // tested in various buffer states
    let form = r#"(let ((results nil))
                    ;; Test 1: normal text
                    (with-temp-buffer
                      (insert "hello world")
                      (setq results
                            (cons (string= (buffer-string)
                                           (buffer-substring (point-min) (point-max)))
                                  results)))
                    ;; Test 2: empty buffer
                    (with-temp-buffer
                      (setq results
                            (cons (string= (buffer-string)
                                           (buffer-substring (point-min) (point-max)))
                                  results)))
                    ;; Test 3: multibyte content
                    (with-temp-buffer
                      (insert "日本語テスト")
                      (setq results
                            (cons (string= (buffer-string)
                                           (buffer-substring (point-min) (point-max)))
                                  results)))
                    ;; Test 4: after modifications
                    (with-temp-buffer
                      (insert "abcdef")
                      (goto-char 4)
                      (insert "XYZ")
                      (delete-region 1 3)
                      (setq results
                            (cons (string= (buffer-string)
                                           (buffer-substring (point-min) (point-max)))
                                  results)))
                    ;; Test 5: narrowed buffer — buffer-string respects narrowing
                    (with-temp-buffer
                      (insert "0123456789")
                      (narrow-to-region 3 8)
                      (setq results
                            (cons (string= (buffer-string)
                                           (buffer-substring (point-min) (point-max)))
                                  results)))
                    ;; All should be t
                    (nreverse results))"#;
    let expect = expect_test::expect![r#""OK (t t t t t)""#];
    crate::common::assert_oracle_parity_expect(form, expect);
}
