//! Oracle parity tests for advanced text properties patterns:
//! overlapping property ranges, selective removal, property traversal,
//! multi-property propertize, and complex rich-text construction.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// propertize with multiple properties at once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_propertize_multi_property_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Set multiple properties and verify each is independently accessible
    let form = r#"(let ((s (propertize "hello world" 'face 'bold
                                        'help-echo "tooltip"
                                        'mouse-face 'highlight
                                        'keymap 'some-map
                                        'category 'my-cat)))
                    (list
                     (get-text-property 0 'face s)
                     (get-text-property 0 'help-echo s)
                     (get-text-property 0 'mouse-face s)
                     (get-text-property 0 'keymap s)
                     (get-text-property 0 'category s)
                     ;; All properties at position 5 (same as 0)
                     (get-text-property 5 'face s)
                     (get-text-property 5 'help-echo s)
                     ;; Verify full plist
                     (length (text-properties-at 0 s))))"#;
    let expect = expect_test::expect![[
        r#""OK (bold \"tooltip\" highlight some-map my-cat bold \"tooltip\" 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// get-text-property at different positions in multi-region string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_property_position_boundaries() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string with different face properties in different regions
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold)
                                    (propertize "BBB" 'face 'italic)
                                    (propertize "CCC" 'face 'underline)
                                    "DDD")))
                    (list
                     ;; Region boundaries
                     (get-text-property 0 'face s)    ;; bold
                     (get-text-property 2 'face s)    ;; bold (last char)
                     (get-text-property 3 'face s)    ;; italic (first char)
                     (get-text-property 5 'face s)    ;; italic (last char)
                     (get-text-property 6 'face s)    ;; underline
                     (get-text-property 8 'face s)    ;; underline (last char)
                     (get-text-property 9 'face s)    ;; nil (unpropertized)
                     (get-text-property 11 'face s)   ;; nil
                     ;; Length verification
                     (length s)))"#;
    let expect =
        expect_test::expect![[r#""OK (bold bold italic italic underline underline nil nil 12)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// put-text-property on ranges with existing properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_put_text_property_overlapping_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Overlay properties on a string that already has properties
    let form = r#"(let ((s (copy-sequence (propertize "0123456789" 'face 'bold))))
                    ;; Add italic to middle region (overlaps with existing bold)
                    (put-text-property 3 7 'face 'italic s)
                    ;; Add a second property to a sub-region
                    (put-text-property 5 8 'help-echo "tip" s)
                    (list
                     ;; face at various positions
                     (get-text-property 0 'face s)       ;; bold
                     (get-text-property 2 'face s)       ;; bold
                     (get-text-property 3 'face s)       ;; italic (overwritten)
                     (get-text-property 6 'face s)       ;; italic
                     (get-text-property 7 'face s)       ;; bold (restored)
                     (get-text-property 9 'face s)       ;; bold
                     ;; help-echo only in 5..8
                     (get-text-property 4 'help-echo s)  ;; nil
                     (get-text-property 5 'help-echo s)  ;; "tip"
                     (get-text-property 7 'help-echo s)  ;; "tip"
                     (get-text-property 8 'help-echo s)));; nil"#;
    let expect = expect_test::expect![[
        r#""OK (bold bold italic italic bold bold nil \"tip\" \"tip\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// remove-text-properties: selective removal
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_selective() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Remove only specific properties, leaving others intact
    let form = r#"(let ((s (propertize "hello" 'face 'bold 'help-echo "tip"
                                        'mouse-face 'highlight)))
                    ;; Remove only face, leave help-echo and mouse-face
                    (remove-text-properties 0 5 '(face nil) s)
                    (let ((after-first
                           (list (get-text-property 0 'face s)
                                 (get-text-property 0 'help-echo s)
                                 (get-text-property 0 'mouse-face s))))
                      ;; Now remove help-echo too
                      (remove-text-properties 0 5 '(help-echo nil) s)
                      (let ((after-second
                             (list (get-text-property 0 'face s)
                                   (get-text-property 0 'help-echo s)
                                   (get-text-property 0 'mouse-face s))))
                        (list after-first after-second))))"#;
    let expect = expect_test::expect![[r#""OK ((nil \"tip\" highlight) (nil nil highlight))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// text-properties-at: full plist extraction
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_properties_at_full_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify text-properties-at returns a complete property list
    let form = r#"(let ((s (propertize "test" 'a 1 'b 2 'c 3)))
                    (let ((plist (text-properties-at 0 s)))
                      (list
                       (plist-get plist 'a)
                       (plist-get plist 'b)
                       (plist-get plist 'c)
                       ;; Verify it's a proper plist with correct length
                       (length plist)
                       ;; Empty position in unpropertized part
                       (text-properties-at 0 "plain"))))"#;
    let expect = expect_test::expect![[r#""OK (1 2 3 6 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change with all parameters
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_next_property_change_with_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test next-property-change with POS, OBJECT, and LIMIT parameters
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold)
                                    "BBB"
                                    (propertize "CCC" 'face 'italic)
                                    "DDD")))
                    (list
                     ;; Find each property boundary
                     (next-property-change 0 s)       ;; 3 (bold->none)
                     (next-property-change 3 s)       ;; 6 (none->italic)
                     (next-property-change 6 s)       ;; 9 (italic->none)
                     (next-property-change 9 s)       ;; nil (no more changes)
                     ;; With LIMIT: stop before actual boundary
                     (next-property-change 0 s 2)     ;; 2 (limited before 3)
                     (next-property-change 0 s 3)     ;; 3 (limit = boundary)
                     (next-property-change 0 s 5)     ;; 3 (limit past boundary)
                     ;; Walk all boundaries
                     (let ((boundaries nil)
                           (pos 0))
                       (while pos
                         (setq pos (next-property-change pos s))
                         (when pos
                           (setq boundaries (cons pos boundaries))))
                       (nreverse boundaries))))"#;
    let expect = expect_test::expect![[r#""OK (3 6 9 nil 2 3 3 (3 6 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: build rich text with overlapping property ranges
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_rich_text_overlapping_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Build a string and layer multiple properties across overlapping ranges
    let form = r#"(let ((s (copy-sequence "The quick brown fox jumps")))
                    ;; Layer 1: face on "quick brown"
                    (put-text-property 4 15 'face 'bold s)
                    ;; Layer 2: help-echo on "brown fox"
                    (put-text-property 10 19 'help-echo "animal" s)
                    ;; Layer 3: mouse-face on "fox jumps"
                    (put-text-property 16 25 'mouse-face 'highlight s)
                    ;; Verify the overlapping regions
                    (list
                     ;; "The " — no properties
                     (text-properties-at 0 s)
                     ;; "quic" — only face
                     (let ((p (text-properties-at 4 s)))
                       (list (plist-get p 'face)
                             (plist-get p 'help-echo)
                             (plist-get p 'mouse-face)))
                     ;; "brown" — face + help-echo
                     (let ((p (text-properties-at 10 s)))
                       (list (plist-get p 'face)
                             (plist-get p 'help-echo)
                             (plist-get p 'mouse-face)))
                     ;; "fox" — help-echo + mouse-face
                     (let ((p (text-properties-at 16 s)))
                       (list (plist-get p 'face)
                             (plist-get p 'help-echo)
                             (plist-get p 'mouse-face)))
                     ;; "jumps" — only mouse-face
                     (let ((p (text-properties-at 20 s)))
                       (list (plist-get p 'face)
                             (plist-get p 'help-echo)
                             (plist-get p 'mouse-face)))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil (bold nil nil) (bold \"animal\" nil) (nil \"animal\" highlight) (nil nil highlight))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property-based syntax highlighting simulation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_syntax_highlight_simulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Simulate a simple syntax highlighter that assigns face properties
    // based on pattern matching, then verify the property map
    let form = r#"(let ((code (copy-sequence "(defun add (a b) (+ a b))"))
                        (keywords '("defun" "let" "if" "cond" "lambda"))
                        (operators '("+" "-" "*" "/")))
                    ;; Highlight keyword "defun" at positions 1..6
                    (put-text-property 1 6 'face 'font-lock-keyword-face code)
                    ;; Highlight function name "add" at positions 7..10
                    (put-text-property 7 10 'face 'font-lock-function-name-face code)
                    ;; Highlight parameters in parens
                    (put-text-property 11 14 'face 'font-lock-variable-name-face code)
                    ;; Highlight operator "+"
                    (put-text-property 17 18 'face 'font-lock-builtin-face code)
                    ;; Now walk through and collect (position . face) for boundaries
                    (let ((result nil)
                          (pos 0))
                      (while (and pos (< pos (length code)))
                        (let ((face (get-text-property pos 'face code)))
                          (when face
                            (setq result (cons (cons pos face) result))))
                        (let ((next (next-property-change pos code)))
                          (setq pos next)))
                      (nreverse result)))"#;
    let expect = expect_test::expect![[
        r#""OK ((1 . font-lock-keyword-face) (7 . font-lock-function-name-face) (11 . font-lock-variable-name-face) (17 . font-lock-builtin-face))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
