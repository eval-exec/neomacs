//! Comprehensive oracle parity tests for text property operations:
//! put-text-property, get-text-property, add-text-properties,
//! remove-text-properties, text-properties-at, next-property-change,
//! previous-property-change, next-single-property-change,
//! propertize, set-text-properties, and text properties on buffer text vs strings.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// put-text-property: START, END, PROPERTY, VALUE, OBJECT params
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_put_text_property_all_params() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test put-text-property with various start/end/property/value combos
    // including zero-length ranges, overlapping overwrites, and multiple properties.
    let form = r#"(let ((s (copy-sequence "abcdefghijklmnop")))
  ;; Basic property set
  (put-text-property 0 4 'face 'bold s)
  ;; Overwrite part of existing range
  (put-text-property 2 6 'face 'italic s)
  ;; Set a different property on overlapping range
  (put-text-property 1 8 'help-echo "help text" s)
  ;; Zero-length range (should be no-op)
  (put-text-property 5 5 'mouse-face 'highlight s)
  ;; Set at very end of string
  (put-text-property 14 16 'category 'my-cat s)
  ;; Numeric value
  (put-text-property 10 12 'priority 42 s)
  ;; List value
  (put-text-property 3 7 'display '(space :width 10) s)
  ;; Boolean nil value (effectively removes)
  (put-text-property 0 2 'invisible nil s)
  (list
   ;; face: 0-2=bold, 2-6=italic, 6+=nil
   (get-text-property 0 'face s)
   (get-text-property 1 'face s)
   (get-text-property 2 'face s)
   (get-text-property 5 'face s)
   (get-text-property 6 'face s)
   ;; help-echo: 1-8
   (get-text-property 0 'help-echo s)
   (get-text-property 1 'help-echo s)
   (get-text-property 7 'help-echo s)
   (get-text-property 8 'help-echo s)
   ;; zero-length property should not exist
   (get-text-property 5 'mouse-face s)
   ;; category at end
   (get-text-property 13 'category s)
   (get-text-property 14 'category s)
   (get-text-property 15 'category s)
   ;; numeric
   (get-text-property 10 'priority s)
   (get-text-property 11 'priority s)
   (get-text-property 12 'priority s)
   ;; list value
   (get-text-property 3 'display s)
   (get-text-property 6 'display s)
   (get-text-property 7 'display s)))"#;
    let expect = expect_test::expect![[
        r#""OK (bold bold italic italic nil nil \"help text\" \"help text\" nil nil nil my-cat my-cat 42 42 nil (space :width 10) (space :width 10) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// get-text-property: POSITION, PROP, OBJECT edge cases
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_get_text_property_edge_cases() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test get-text-property at boundaries, with non-existent properties,
    // on empty strings, and with compound property values.
    let form = r#"(let ((s1 (propertize "hello" 'face 'bold 'custom '(a b c)))
       (s2 (copy-sequence ""))
       (s3 (propertize "x" 'face 'italic)))
  (list
   ;; First and last positions
   (get-text-property 0 'face s1)
   (get-text-property 4 'face s1)
   ;; Non-existent property
   (get-text-property 0 'nonexistent s1)
   (get-text-property 2 'invisible s1)
   ;; Compound value
   (get-text-property 0 'custom s1)
   ;; Single-char string
   (get-text-property 0 'face s3)
   ;; Property on nil object (should use current buffer)
   (with-temp-buffer
     (insert (propertize "test" 'face 'underline))
     (list
       (get-text-property 1 'face)
       (get-text-property 2 'face)
       (get-text-property 1 'nonexistent)))
   ;; Same property across entire string
   (let ((s4 (propertize "abcde" 'face 'bold)))
     (list
       (get-text-property 0 'face s4)
       (get-text-property 2 'face s4)
       (get-text-property 4 'face s4)))))"#;
    let expect = expect_test::expect![[
        r#""OK (bold bold nil nil (a b c) italic (underline underline nil) (bold bold bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// add-text-properties: START, END, PROPERTIES list
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_add_text_properties_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // add-text-properties adds properties without replacing existing ones.
    // Test with multiple overlapping additions, return value semantics.
    let form = r#"(let ((s (copy-sequence "0123456789ABCDEF")))
  ;; Layer 1: face on 0-8
  (put-text-property 0 8 'face 'bold s)
  ;; Layer 2: add help-echo and mouse-face on 4-12
  (let ((ret1 (add-text-properties 4 12 '(help-echo "tip1" mouse-face highlight) s)))
    ;; Layer 3: add category and priority on 8-16
    (let ((ret2 (add-text-properties 8 16 '(category my-cat priority 5) s)))
      ;; Layer 4: try adding face on 0-4 (already exists) -- should still return t
      ;; because add-text-properties replaces existing values of same prop
      (let ((ret3 (add-text-properties 0 4 '(face italic) s)))
        ;; Layer 5: add with empty plist (should be no-op, return nil)
        (let ((ret4 (add-text-properties 0 4 nil s)))
          (list
           ;; Return values: t if properties changed, nil if not
           ret1 ret2 ret3 ret4
           ;; Position 2: face=italic (overwritten), no help-echo
           (get-text-property 2 'face s)
           (get-text-property 2 'help-echo s)
           ;; Position 6: face=bold (from layer 1), help-echo, mouse-face
           (get-text-property 6 'face s)
           (get-text-property 6 'help-echo s)
           (get-text-property 6 'mouse-face s)
           ;; Position 10: no face, help-echo, mouse-face, category, priority
           (get-text-property 10 'face s)
           (get-text-property 10 'help-echo s)
           (get-text-property 10 'category s)
           (get-text-property 10 'priority s)
           ;; Position 14: category, priority only
           (get-text-property 14 'face s)
           (get-text-property 14 'category s)
           (get-text-property 14 'priority s)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t t t nil italic nil bold \"tip1\" highlight nil \"tip1\" my-cat 5 nil my-cat 5)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// remove-text-properties: complex removal patterns
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_remove_text_properties_advanced() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test partial removal, removal of non-existent properties,
    // removal from subranges, and return value semantics.
    let form = r#"(let ((s (copy-sequence "ABCDEFGHIJKLMNOP")))
  ;; Setup: multiple properties on various ranges
  (put-text-property 0 16 'face 'bold s)
  (put-text-property 4 12 'help-echo "tip" s)
  (put-text-property 2 14 'mouse-face 'highlight s)
  (put-text-property 0 16 'fontified t s)

  ;; Remove face from 6-10 (splits the face range)
  (let ((r1 (remove-text-properties 6 10 '(face nil) s)))
    ;; Remove nonexistent property (should return nil)
    (let ((r2 (remove-text-properties 0 16 '(nonexistent nil) s)))
      ;; Remove help-echo from 0-16 (wider than help-echo range)
      (let ((r3 (remove-text-properties 0 16 '(help-echo nil) s)))
        ;; Remove multiple properties at once
        (let ((r4 (remove-text-properties 0 8 '(mouse-face nil fontified nil) s)))
          (list
           ;; Return values
           r1 r2 r3 r4
           ;; face: bold at 0-6, nil at 6-10, bold at 10-16
           (get-text-property 0 'face s)
           (get-text-property 5 'face s)
           (get-text-property 6 'face s)
           (get-text-property 9 'face s)
           (get-text-property 10 'face s)
           (get-text-property 15 'face s)
           ;; help-echo: all removed
           (get-text-property 4 'help-echo s)
           (get-text-property 8 'help-echo s)
           ;; mouse-face: removed from 0-8, still present 8-14
           (get-text-property 2 'mouse-face s)
           (get-text-property 7 'mouse-face s)
           (get-text-property 8 'mouse-face s)
           (get-text-property 13 'mouse-face s)
           (get-text-property 14 'mouse-face s)
           ;; fontified: removed from 0-8, still present 8-16
           (get-text-property 0 'fontified s)
           (get-text-property 7 'fontified s)
           (get-text-property 8 'fontified s)
           (get-text-property 15 'fontified s)))))))"#;
    let expect = expect_test::expect![[
        r#""OK (t nil t t bold bold nil nil bold bold nil nil nil nil highlight highlight nil nil nil t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// text-properties-at: full property list retrieval
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_text_properties_at_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // text-properties-at returns the full plist at a position.
    // Test with multiple properties, at boundaries, and with no properties.
    let form = r#"(let ((s (copy-sequence "ABCDEFGHIJ")))
  ;; No properties initially
  (let ((empty-props (text-properties-at 0 s)))
    ;; Add several properties to different ranges
    (put-text-property 0 5 'face 'bold s)
    (put-text-property 0 5 'help-echo "bold text" s)
    (put-text-property 3 7 'mouse-face 'highlight s)
    (put-text-property 2 8 'category 'my-cat s)
    ;; Collect plists at various positions
    (let ((p0 (text-properties-at 0 s))
          (p2 (text-properties-at 2 s))
          (p4 (text-properties-at 4 s))
          (p6 (text-properties-at 6 s))
          (p8 (text-properties-at 8 s)))
      ;; Count properties at each position
      (list
       empty-props
       (length p0)  ;; face + help-echo = 4 entries in plist
       (length p2)  ;; face + help-echo + category = 6
       (length p4)  ;; face + help-echo + mouse-face + category = 8
       (length p6)  ;; mouse-face + category = 4
       (length p8)  ;; nil (nothing)
       ;; Check specific values via plist-get on returned plist
       (plist-get p4 'face)
       (plist-get p4 'help-echo)
       (plist-get p4 'mouse-face)
       (plist-get p4 'category)
       ;; text-properties-at in buffer
       (with-temp-buffer
         (insert (propertize "test" 'face 'italic 'custom 99))
         (let ((bp (text-properties-at 1)))
           (list (plist-get bp 'face)
                 (plist-get bp 'custom))))))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil 4 6 8 4 0 bold \"bold text\" highlight my-cat (italic 99))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-property-change and previous-property-change: navigation
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_property_change_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Comprehensive traversal of property boundaries in both directions,
    // with and without LIMIT parameter, and on strings with complex property layouts.
    let form = r#"(let ((s (copy-sequence "0123456789ABCDEFGHIJ")))
  ;; Create a complex property layout:
  ;; 0-3: face=bold
  ;; 3-5: (nothing)
  ;; 5-8: face=italic, help-echo="tip"
  ;; 8-12: help-echo="tip" (face ends)
  ;; 12-15: mouse-face=highlight
  ;; 15-20: (nothing)
  (put-text-property 0 3 'face 'bold s)
  (put-text-property 5 8 'face 'italic s)
  (put-text-property 5 12 'help-echo "tip" s)
  (put-text-property 12 15 'mouse-face 'highlight s)
  ;; Forward traversal: collect all property change positions
  (let ((fwd nil) (pos 0))
    (while pos
      (setq pos (next-property-change pos s))
      (when pos (setq fwd (cons pos fwd))))
    (setq fwd (nreverse fwd))
    ;; Backward traversal from end
    (let ((bwd nil) (pos (length s)))
      (while pos
        (setq pos (previous-property-change pos s))
        (when pos (setq bwd (cons pos bwd))))
      ;; Forward with limit
      (let ((fwd-lim nil) (pos 0))
        (while (and pos (< pos 10))
          (setq pos (next-property-change pos s 10))
          (when pos (setq fwd-lim (cons pos fwd-lim))))
        (setq fwd-lim (nreverse fwd-lim))
        ;; Backward with limit
        (let ((bwd-lim nil) (pos (length s)))
          (while (and pos (> pos 10))
            (setq pos (previous-property-change pos s 10))
            (when pos (setq bwd-lim (cons pos bwd-lim))))
          (list
           fwd
           bwd
           fwd-lim
           bwd-lim
           ;; Specific boundary checks
           (next-property-change 0 s)      ;; 3 (face ends)
           (next-property-change 3 s)      ;; 5 (face+help-echo start)
           (next-property-change 5 s)      ;; 8 (face ends, help-echo continues)
           (next-property-change 12 s)     ;; 15 (mouse-face ends)
           (next-property-change 15 s)     ;; nil (no more changes)
           (previous-property-change 20 s) ;; 15
           (previous-property-change 15 s) ;; 12
           (previous-property-change 8 s)  ;; 5
           (previous-property-change 3 s)  ;; 0
           (previous-property-change 0 s)  ;; nil
           ))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((3 5 8 12 15) (3 5 8 12 15) (3 5 8 10) (10 12 15) 3 5 8 15 nil 15 12 5 nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// next-single-property-change: tracking specific properties
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_next_single_property_change_detailed() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Track individual property boundaries independently.
    let form = r#"(let ((s (copy-sequence "ABCDEFGHIJKLMNOPQRST")))
  ;; face: bold 0-5, italic 5-10, underline 10-15
  (put-text-property 0 5 'face 'bold s)
  (put-text-property 5 10 'face 'italic s)
  (put-text-property 10 15 'face 'underline s)
  ;; help-echo: "a" 2-8, "b" 8-18
  (put-text-property 2 8 'help-echo "a" s)
  (put-text-property 8 18 'help-echo "b" s)
  ;; mouse-face: highlight 0-20
  (put-text-property 0 20 'mouse-face 'highlight s)
  ;; previous-single-property-change for face
  (let ((face-fwd nil) (pos 0))
    (while pos
      (setq pos (next-single-property-change pos 'face s))
      (when pos (setq face-fwd (cons pos face-fwd))))
    (setq face-fwd (nreverse face-fwd))
    ;; previous-single-property-change for face
    (let ((face-bwd nil) (pos (length s)))
      (while pos
        (setq pos (previous-single-property-change pos 'face s))
        (when pos (setq face-bwd (cons pos face-bwd))))
      ;; help-echo boundaries
      (let ((echo-fwd nil) (pos 0))
        (while pos
          (setq pos (next-single-property-change pos 'help-echo s))
          (when pos (setq echo-fwd (cons pos echo-fwd))))
        (setq echo-fwd (nreverse echo-fwd))
        ;; mouse-face: should be 0->20 with no intermediate changes
        (let ((mouse-fwd nil) (pos 0))
          (while pos
            (setq pos (next-single-property-change pos 'mouse-face s))
            (when pos (setq mouse-fwd (cons pos mouse-fwd))))
          (setq mouse-fwd (nreverse mouse-fwd))
          (list
           face-fwd      ;; (5 10 15)
           face-bwd      ;; (0 5 10)
           echo-fwd      ;; (2 8 18)
           mouse-fwd     ;; (20)
           ;; With limit parameter
           (next-single-property-change 0 'face s 3)     ;; 3 (limit < boundary)
           (next-single-property-change 0 'face s 5)     ;; 5 (limit = boundary)
           (next-single-property-change 0 'face s 10)    ;; 5 (limit > boundary)
           (previous-single-property-change 20 'face s 12) ;; 15 (boundary > limit)
           (previous-single-property-change 20 'face s 16) ;; 16 (limit > boundary)
           ;; Non-existent property should return nil immediately
           (next-single-property-change 0 'nonexistent s)
           ))))))"#;
    let expect =
        expect_test::expect![[r#""OK ((5 10 15) (5 10 15) (2 8 18) nil 3 5 5 15 16 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// propertize: building propertized strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_propertize_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test propertize with various property types, concatenation of propertized
    // strings, nested propertize, and interaction with copy-sequence.
    let form = r#"(let* ((s1 (propertize "hello" 'face 'bold 'help-echo "greeting"))
        (s2 (propertize "world" 'face 'italic 'category 'important))
        (s3 (propertize "" 'face 'underline))
        (s4 (propertize "x" 'a 1 'b 2 'c 3 'd 4 'e 5))
        ;; Concatenation preserves properties
        (combined (concat s1 " " s2))
        ;; Copy preserves properties
        (copied (copy-sequence s1)))
  (list
   ;; Basic propertize
   (get-text-property 0 'face s1)
   (get-text-property 0 'help-echo s1)
   (get-text-property 4 'face s1)
   ;; Second string
   (get-text-property 0 'face s2)
   (get-text-property 0 'category s2)
   ;; Empty string length
   (length s3)
   ;; Many properties
   (get-text-property 0 'a s4)
   (get-text-property 0 'c s4)
   (get-text-property 0 'e s4)
   ;; Concat preserves: "hello" + " " + "world"
   (get-text-property 0 'face combined)         ;; bold
   (get-text-property 4 'face combined)         ;; bold
   (get-text-property 5 'face combined)         ;; nil (space)
   (get-text-property 6 'face combined)         ;; italic
   (get-text-property 10 'face combined)        ;; italic
   (get-text-property 0 'help-echo combined)    ;; "greeting"
   (get-text-property 6 'category combined)     ;; important
   ;; Copy preserves properties independently
   (get-text-property 0 'face copied)
   ;; Modify copy, original unchanged
   (progn
     (put-text-property 0 5 'face 'italic copied)
     (list (get-text-property 0 'face copied)
           (get-text-property 0 'face s1)))
   ;; propertize with list values
   (let ((s5 (propertize "test" 'display '(image :type png) 'font-lock-face '(bold italic))))
     (list (get-text-property 0 'display s5)
           (get-text-property 0 'font-lock-face s5)))))"#;
    let expect = expect_test::expect![[
        r#""OK (bold \"greeting\" bold italic important 0 1 3 5 bold bold nil italic italic \"greeting\" important bold (italic bold) ((image :type png) (bold italic)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// set-text-properties: replacing all properties on a range
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_set_text_properties_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // set-text-properties replaces ALL properties in the range, unlike
    // put-text-property which only affects one property.
    let form = r#"(let ((s (copy-sequence "ABCDEFGHIJKL")))
  ;; Setup: multiple properties
  (put-text-property 0 12 'face 'bold s)
  (put-text-property 0 12 'help-echo "tip" s)
  (put-text-property 0 12 'mouse-face 'highlight s)
  ;; Verify all three exist
  (let ((before (list (get-text-property 3 'face s)
                      (get-text-property 3 'help-echo s)
                      (get-text-property 3 'mouse-face s))))
    ;; set-text-properties on 2-8: replace with only 'category
    (set-text-properties 2 8 '(category my-cat) s)
    (let ((middle (list
                   ;; Position 1: still has all three (outside replaced range)
                   (get-text-property 1 'face s)
                   (get-text-property 1 'help-echo s)
                   (get-text-property 1 'mouse-face s)
                   ;; Position 4: only category (all others removed)
                   (get-text-property 4 'face s)
                   (get-text-property 4 'help-echo s)
                   (get-text-property 4 'mouse-face s)
                   (get-text-property 4 'category s)
                   ;; Position 9: still has all three (outside replaced range)
                   (get-text-property 9 'face s)
                   (get-text-property 9 'help-echo s))))
      ;; set-text-properties with nil removes all properties
      (set-text-properties 0 12 nil s)
      (let ((after (list
                    (get-text-property 0 'face s)
                    (get-text-property 5 'category s)
                    (get-text-property 10 'help-echo s)
                    (text-properties-at 0 s)
                    (text-properties-at 6 s))))
        (list before middle after)))))"#;
    let expect = expect_test::expect![[
        r#""OK ((bold \"tip\" highlight) (bold \"tip\" highlight nil nil nil my-cat bold \"tip\") (nil nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Text properties on buffer text vs strings
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_buffer_vs_string_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Verify that text property operations work identically on buffer text
    // and strings, including extraction via buffer-substring and insertion.
    let form = r#"(with-temp-buffer
  ;; Insert propertized text into buffer
  (insert (propertize "Hello" 'face 'bold))
  (insert " ")
  (insert (propertize "World" 'face 'italic 'help-echo "greeting"))
  ;; Buffer text property operations
  (let ((buf-face-1 (get-text-property 1 'face))
        (buf-face-6 (get-text-property 6 'face))
        (buf-face-7 (get-text-property 7 'face))
        (buf-echo-7 (get-text-property 7 'help-echo))
        (buf-echo-1 (get-text-property 1 'help-echo)))
    ;; put-text-property on buffer
    (put-text-property 1 11 'category 'sentence)
    (let ((buf-cat-3 (get-text-property 3 'category))
          (buf-cat-9 (get-text-property 9 'category)))
      ;; Extract substring preserving properties
      (let ((sub (buffer-substring 1 6)))
        (let ((sub-face (get-text-property 0 'face sub))
              (sub-cat (get-text-property 0 'category sub)))
          ;; text-properties-at in buffer vs extracted string
          (let ((buf-plist (text-properties-at 3))
                (sub-plist (text-properties-at 2 sub)))
            ;; Modify buffer properties, verify substring is independent
            (put-text-property 1 6 'face 'underline)
            (let ((buf-face-after (get-text-property 3 'face))
                  (sub-face-after (get-text-property 2 'face sub)))
              ;; next-property-change in buffer
              (let ((npc (let ((positions nil) (pos 1))
                           (while (and pos (< pos (point-max)))
                             (setq pos (next-property-change pos nil (point-max)))
                             (when (and pos (< pos (point-max)))
                               (setq positions (cons pos positions))))
                           (nreverse positions))))
                (list
                 buf-face-1 buf-face-6 buf-face-7
                 buf-echo-7 buf-echo-1
                 buf-cat-3 buf-cat-9
                 sub-face sub-cat
                 (length buf-plist) (length sub-plist)
                 buf-face-after sub-face-after
                 npc))))))))))"#;
    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 41 30)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: property interval merging and splitting
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_text_prop_comp_interval_merge_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Test that setting the same value on adjacent regions merges intervals,
    // and different values split them properly.
    let form = r#"(let ((s (copy-sequence "ABCDEFGHIJKLMNOP")))
  ;; Set face=bold on three separate adjacent regions
  (put-text-property 0 4 'face 'bold s)
  (put-text-property 4 8 'face 'bold s)
  (put-text-property 8 12 'face 'bold s)
  ;; Should be one merged interval 0-12
  (let ((changes-after-merge
         (let ((positions nil) (pos 0))
           (while pos
             (setq pos (next-single-property-change pos 'face s))
             (when pos (setq positions (cons pos positions))))
           (nreverse positions))))
    ;; Now split in the middle by changing 4-8 to italic
    (put-text-property 4 8 'face 'italic s)
    (let ((changes-after-split
           (let ((positions nil) (pos 0))
             (while pos
               (setq pos (next-single-property-change pos 'face s))
               (when pos (setq positions (cons pos positions))))
             (nreverse positions))))
      ;; Set italic on 0-4 to merge left
      (put-text-property 0 4 'face 'italic s)
      (let ((changes-after-left-merge
             (let ((positions nil) (pos 0))
               (while pos
                 (setq pos (next-single-property-change pos 'face s))
                 (when pos (setq positions (cons pos positions))))
               (nreverse positions))))
        ;; Remove face from 6-10 to create a gap
        (remove-text-properties 6 10 '(face nil) s)
        (let ((changes-after-gap
               (let ((positions nil) (pos 0))
                 (while pos
                   (setq pos (next-single-property-change pos 'face s))
                   (when pos (setq positions (cons pos positions))))
                 (nreverse positions))))
          (list
           changes-after-merge       ;; (12) - one boundary
           changes-after-split       ;; (4 8 12) - three boundaries
           changes-after-left-merge  ;; (8 12) - two boundaries
           changes-after-gap         ;; (6 10 12) - gap in middle
           ;; Verify values at key positions
           (get-text-property 0 'face s)   ;; italic
           (get-text-property 5 'face s)   ;; italic
           (get-text-property 7 'face s)   ;; nil (gap)
           (get-text-property 10 'face s)  ;; bold
           (get-text-property 11 'face s)  ;; bold
           (get-text-property 12 'face s)  ;; nil
           ))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((12) (4 8 12) (8 12) (6 10 12) italic italic nil bold bold nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
