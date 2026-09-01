//! Advanced oracle parity tests for `propertize`:
//! multiple property pairs, nested propertize, combining with
//! `get-text-property`, `text-properties-at`, property inheritance
//! patterns, face properties, complex multi-property strings used
//! in buffer operations.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Nested propertize: inner propertize overridden by outer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_nested_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // When propertize wraps another propertize, the outer properties
    // should override the inner ones for the same keys, and non-overlapping
    // keys from the inner call should be preserved.
    let form = r#"(let* ((inner (propertize "hello" 'face 'bold 'help-echo "inner-tip"))
                         (outer (propertize inner 'face 'italic 'mouse-face 'highlight)))
                    (list
                     ;; face should be overridden to italic
                     (get-text-property 0 'face outer)
                     ;; help-echo should be preserved from inner
                     (get-text-property 0 'help-echo outer)
                     ;; mouse-face added by outer
                     (get-text-property 0 'mouse-face outer)
                     ;; check at multiple positions
                     (get-text-property 2 'face outer)
                     (get-text-property 4 'help-echo outer)
                     (get-text-property 4 'mouse-face outer)
                     ;; total property count
                     (length (text-properties-at 0 outer))))"#;
    let expect = expect_test::expect![[
        r#""OK (italic \"inner-tip\" highlight italic \"inner-tip\" highlight 6)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Propertize concatenated segments with get-text-property boundary checks
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_concat_boundary_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string from multiple propertized segments via concat,
    // then verify properties at exact boundary positions.
    let form = r#"(let* ((s1 (propertize "AB" 'face 'bold 'category 'cat-a))
                         (s2 (propertize "CD" 'face 'italic 'priority 1))
                         (s3 (propertize "EF" 'face 'underline 'priority 2 'category 'cat-c))
                         (s4 "GH")
                         (combined (concat s1 s2 s3 s4)))
                    (list
                     ;; Segment 1 boundaries
                     (get-text-property 0 'face combined)
                     (get-text-property 1 'face combined)
                     (get-text-property 0 'category combined)
                     ;; Segment 2 boundaries
                     (get-text-property 2 'face combined)
                     (get-text-property 3 'face combined)
                     (get-text-property 2 'priority combined)
                     (get-text-property 2 'category combined)
                     ;; Segment 3 boundaries
                     (get-text-property 4 'face combined)
                     (get-text-property 5 'priority combined)
                     (get-text-property 4 'category combined)
                     ;; Segment 4 -- no properties
                     (get-text-property 6 'face combined)
                     (get-text-property 7 'priority combined)
                     ;; Total length
                     (length combined)))"#;
    let expect = expect_test::expect![[
        r#""OK (bold bold cat-a italic italic 1 nil underline 2 cat-c nil nil 8)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Propertize + substring preserves properties within range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_substring_property_preservation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Propertize a long string, take substrings, verify properties are
    // carried along correctly and boundaries are adjusted.
    let form = r#"(let* ((full (concat (propertize "AAA" 'x 1)
                                       (propertize "BBB" 'x 2)
                                       (propertize "CCC" 'x 3)))
                         ;; Substring that spans two property regions
                         (sub1 (substring full 2 7))
                         ;; Substring entirely within one region
                         (sub2 (substring full 3 6))
                         ;; Substring starting in middle, going to end
                         (sub3 (substring full 5)))
                    (list
                     ;; sub1 = "ABBBC" -> pos 0 should be x=1, pos 1 should be x=2
                     (get-text-property 0 'x sub1)
                     (get-text-property 1 'x sub1)
                     (get-text-property 4 'x sub1)
                     (length sub1)
                     ;; sub2 = "BBB" -> all x=2
                     (get-text-property 0 'x sub2)
                     (get-text-property 2 'x sub2)
                     ;; sub3 = "BCCC" -> pos 0 x=2, pos 1..3 x=3
                     (get-text-property 0 'x sub3)
                     (get-text-property 1 'x sub3)
                     (get-text-property 3 'x sub3)
                     (length sub3)))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 5 2 2 2 3 3 4)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex multi-property propertize with plist extraction and manipulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_plist_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Propertize with many properties, extract plist, manipulate it,
    // use plist-get/plist-put/plist-member to query.
    let form = r#"(let* ((s (propertize "test-string"
                                        'face '(:foreground "red" :background "blue")
                                        'display '(space :width 10)
                                        'invisible t
                                        'intangible t
                                        'rear-nonsticky '(face)
                                        'front-sticky '(invisible)))
                         (plist (text-properties-at 0 s))
                         (plist-mid (text-properties-at 5 s)))
                    (list
                     ;; Face is a plist itself
                     (get-text-property 0 'face s)
                     ;; Extract face sub-properties
                     (plist-get (get-text-property 0 'face s) :foreground)
                     (plist-get (get-text-property 0 'face s) :background)
                     ;; display property
                     (get-text-property 0 'display s)
                     ;; boolean properties
                     (get-text-property 0 'invisible s)
                     (get-text-property 0 'intangible s)
                     ;; list properties
                     (get-text-property 0 'rear-nonsticky s)
                     (get-text-property 0 'front-sticky s)
                     ;; plist at middle position should be same
                     (equal plist plist-mid)
                     ;; plist-member checks
                     (not (null (plist-member plist 'face)))
                     (not (null (plist-member plist 'invisible)))
                     (null (plist-member plist 'nonexistent))
                     ;; property count (6 keys * 2 = 12 entries in plist)
                     (length plist)))"#;
    let expect = expect_test::expect![[
        r#""OK ((:foreground \"red\" :background \"blue\") \"red\" \"blue\" (space :width 10) t t (face) (invisible) t t t t 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Propertize in buffer context: insert propertized text and read back
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_buffer_insert_and_readback() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Insert propertized text into a temp buffer, read properties back,
    // then modify properties in-buffer and verify.
    let form = r#"(with-temp-buffer
                    (insert (propertize "Hello " 'face 'bold))
                    (insert (propertize "World" 'face 'italic 'help-echo "greet"))
                    (insert "!")
                    (let* ((full (buffer-string))
                           (p0 (get-text-property 1 'face full))
                           (p6 (get-text-property 7 'face full))
                           (p6h (get-text-property 7 'help-echo full))
                           (p11 (get-text-property 12 'face full)))
                      ;; Now modify properties in-buffer
                      (put-text-property 1 12 'category 'greeting)
                      (let ((full2 (buffer-string)))
                        (list
                         ;; Original properties
                         p0 p6 p6h p11
                         ;; After modification: face preserved, category added
                         (get-text-property 1 'face full2)
                         (get-text-property 7 'face full2)
                         (get-text-property 3 'category full2)
                         (get-text-property 8 'category full2)
                         ;; Exclamation mark should also have category now
                         (get-text-property 12 'category full2)
                         ;; Length
                         (length full2)))))"#;
    let expect = expect_test::expect![[
        r#""OK (bold italic \"greet\" nil bold italic greeting greeting nil 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Propertize with add-text-properties and set-text-properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_add_and_set_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test add-text-properties (merges) vs set-text-properties (replaces)
    // and their interaction with propertize.
    let form = r#"(let ((s (copy-sequence (propertize "abcdefgh" 'face 'bold 'x 1))))
                    ;; add-text-properties: merge new properties, keep existing
                    (add-text-properties 0 4 '(y 2 z 3) s)
                    (let ((after-add
                           (list (get-text-property 0 'face s)
                                 (get-text-property 0 'x s)
                                 (get-text-property 0 'y s)
                                 (get-text-property 0 'z s)
                                 ;; positions 4-7 should still only have face+x
                                 (get-text-property 4 'y s)
                                 (get-text-property 4 'face s))))
                      ;; set-text-properties: replace ALL properties in range
                      (set-text-properties 2 6 '(q 99) s)
                      (let ((after-set
                             (list
                              ;; pos 0-1: unchanged (face, x, y, z)
                              (get-text-property 0 'face s)
                              (get-text-property 0 'y s)
                              ;; pos 2-5: only q=99, no face/x/y/z
                              (get-text-property 2 'face s)
                              (get-text-property 2 'x s)
                              (get-text-property 2 'q s)
                              ;; pos 6-7: original face+x
                              (get-text-property 6 'face s)
                              (get-text-property 6 'q s))))
                        (list after-add after-set))))"#;
    let expect =
        expect_test::expect![[r#""OK ((bold 1 2 3 nil bold) (bold 2 nil nil 99 bold nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build syntax-highlighted string with property walking
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_syntax_highlight_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate building a syntax-highlighted code snippet with multiple
    // face properties, then walk the string collecting property change
    // boundaries and the face at each segment.
    let form = r#"(let* ((code (concat
                                (propertize "def " 'face 'font-lock-keyword-face)
                                (propertize "fibonacci" 'face 'font-lock-function-name-face)
                                (propertize "(" 'face 'font-lock-bracket-face)
                                (propertize "n" 'face 'font-lock-variable-name-face)
                                (propertize "):" 'face 'font-lock-bracket-face)
                                " "
                                (propertize "return" 'face 'font-lock-keyword-face)
                                " "
                                (propertize "n" 'face 'font-lock-variable-name-face)))
                         ;; Walk all property boundaries, collecting (start face) pairs
                         (segments nil)
                         (pos 0)
                         (len (length code)))
                    (while (< pos len)
                      (let ((face (get-text-property pos 'face code))
                            (next (next-property-change pos code)))
                        (setq segments (cons (list pos (or next len) face) segments))
                        (setq pos (or next len))))
                    (let ((result (nreverse segments)))
                      (list
                       ;; Number of segments
                       (length result)
                       ;; The segments themselves
                       result
                       ;; Verify first segment is keyword
                       (nth 2 (car result))
                       ;; Verify total coverage equals string length
                       (= (nth 1 (car (last result))) len)
                       ;; Count keyword faces
                       (length (seq-filter
                                (lambda (seg) (eq (nth 2 seg) 'font-lock-keyword-face))
                                result))
                       ;; Total string length
                       len)))"#;
    let expect = expect_test::expect![[
        r#""OK (9 ((0 4 font-lock-keyword-face) (4 13 font-lock-function-name-face) (13 14 font-lock-bracket-face) (14 15 font-lock-variable-name-face) (15 17 font-lock-bracket-face) (17 18 nil) (18 24 font-lock-keyword-face) (24 25 nil) (25 26 font-lock-variable-name-face)) font-lock-keyword-face t 2 26)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Propertize: property comparison and equality semantics
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_equality_semantics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test how propertized strings interact with equal, string=,
    // and equal-including-properties.
    let form = r#"(let ((s1 (propertize "hello" 'face 'bold))
                        (s2 (propertize "hello" 'face 'bold))
                        (s3 (propertize "hello" 'face 'italic))
                        (s4 "hello")
                        (s5 (propertize "hello" 'face 'bold 'extra 1)))
                    (list
                     ;; string= ignores properties
                     (string= s1 s2)
                     (string= s1 s3)
                     (string= s1 s4)
                     ;; equal on strings compares only text in Emacs
                     (equal s1 s4)
                     (equal s1 s2)
                     ;; equal-including-properties checks properties too
                     (equal-including-properties s1 s2)
                     (equal-including-properties s1 s3)
                     (equal-including-properties s1 s4)
                     (equal-including-properties s1 s5)
                     ;; Verify copy-sequence preserves properties for equality
                     (equal-including-properties s1 (copy-sequence s1))
                     ;; substring preserves properties
                     (equal-including-properties
                      (substring s1 0 3)
                      (propertize "hel" 'face 'bold))))"#;
    let expect = expect_test::expect![[r#""OK (t t t t t t nil nil nil t t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
