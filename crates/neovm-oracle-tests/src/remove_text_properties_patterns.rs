//! Oracle parity tests for `remove-text-properties` with complex patterns:
//! START, END, PROPERTIES arguments, single vs multiple property removal,
//! return value semantics, interaction with propertize, stripping all
//! formatting, and selective property removal.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

// ---------------------------------------------------------------------------
// Basic: remove a single property
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_single() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "hello" 'face 'bold)))
  (let ((before (get-text-property 0 'face s)))
    (remove-text-properties 0 5 '(face nil) s)
    (let ((after (get-text-property 0 'face s)))
      (list before after))))"#;
    let expect = expect_test::expect![[r#""OK (bold nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Return value: t if any property was actually changed, nil otherwise
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_return_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s1 (propertize "abc" 'face 'bold))
                        (s2 (copy-sequence "abc")))
  ;; s1 has face, so removing it should return t
  (let ((r1 (remove-text-properties 0 3 '(face nil) s1))
        ;; s2 has no properties, so removing should return nil
        (r2 (remove-text-properties 0 3 '(face nil) s2))
        ;; Removing a property that doesn't exist on a propertized string
        (s3 (propertize "xyz" 'face 'bold)))
    (let ((r3 (remove-text-properties 0 3 '(help-echo nil) s3)))
      (list r1 r2 r3))))"#;
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Remove multiple properties at once
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "hello world" 'face 'bold 'help-echo "tip"
                                        'mouse-face 'highlight 'category 'my-cat)))
  ;; Remove face and help-echo in one call
  (remove-text-properties 0 11 '(face nil help-echo nil) s)
  (list
   (get-text-property 0 'face s)         ;; should be nil
   (get-text-property 0 'help-echo s)    ;; should be nil
   (get-text-property 0 'mouse-face s)   ;; should remain
   (get-text-property 0 'category s)     ;; should remain
   ;; Now remove the remaining two
   (let ((r (remove-text-properties 0 11 '(mouse-face nil category nil) s)))
     (list r
           (get-text-property 0 'mouse-face s)
           (get-text-property 0 'category s)))))"#;
    let expect = expect_test::expect![[r#""OK (nil nil highlight my-cat (t nil nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Partial range removal: only some positions affected
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_partial_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "0123456789" 'face 'bold)))
  ;; Remove face from positions 3..7 only
  (remove-text-properties 3 7 '(face nil) s)
  (list
   (get-text-property 0 'face s)   ;; bold (before range)
   (get-text-property 2 'face s)   ;; bold (just before range)
   (get-text-property 3 'face s)   ;; nil (start of removal)
   (get-text-property 5 'face s)   ;; nil (middle)
   (get-text-property 6 'face s)   ;; nil (last in range)
   (get-text-property 7 'face s)   ;; bold (just after range)
   (get-text-property 9 'face s)   ;; bold (well after)
   ;; property boundaries
   (next-property-change 0 s)      ;; 3
   (next-property-change 3 s)      ;; 7
   (next-property-change 7 s)))"#;
    let expect = expect_test::expect![[r#""OK (bold bold nil nil nil bold bold 3 7 nil)""#]];
    // nil (rest is uniform)
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Interaction with propertize: remove then re-add
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_then_readd() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "abcdef" 'face 'bold 'help-echo "tip")))
  ;; Remove all properties
  (remove-text-properties 0 6 '(face nil help-echo nil) s)
  (let ((after-remove (list (get-text-property 0 'face s)
                            (get-text-property 0 'help-echo s))))
    ;; Re-add different properties
    (put-text-property 0 6 'face 'italic s)
    (put-text-property 2 4 'help-echo "new-tip" s)
    (let ((after-readd
           (list (get-text-property 0 'face s)
                 (get-text-property 1 'help-echo s)
                 (get-text-property 2 'help-echo s)
                 (get-text-property 3 'help-echo s)
                 (get-text-property 4 'help-echo s))))
      (list after-remove after-readd))))"#;
    let expect =
        expect_test::expect![[r#""OK ((nil nil) (italic nil \"new-tip\" \"new-tip\" nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: strip ALL formatting from a region in a buffer
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_strip_all_in_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(with-temp-buffer
  ;; Insert richly formatted text
  (insert (propertize "bold" 'face 'bold))
  (insert " ")
  (insert (propertize "italic" 'face 'italic 'help-echo "hover"))
  (insert " ")
  (insert (propertize "colored" 'face '(:foreground "red") 'mouse-face 'highlight))
  ;; Capture property state before
  (let ((before (list (get-text-property 1 'face)
                      (get-text-property 6 'face)
                      (get-text-property 6 'help-echo)
                      (get-text-property 13 'face)
                      (get-text-property 13 'mouse-face))))
    ;; Strip everything
    (remove-text-properties (point-min) (point-max)
                            '(face nil help-echo nil mouse-face nil))
    ;; Verify all properties are gone
    (let ((after (list (get-text-property 1 'face)
                       (get-text-property 6 'face)
                       (get-text-property 6 'help-echo)
                       (get-text-property 13 'face)
                       (get-text-property 13 'mouse-face))))
      (list before after
            ;; Text content preserved
            (buffer-substring-no-properties (point-min) (point-max))))))"#;
    let expect = expect_test::expect![[
        r#""OK ((bold italic \"hover\" (:foreground \"red\") highlight) (nil nil nil nil nil) \"bold italic colored\")""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Complex: selective removal based on property walk
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_selective_walk() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Walk a string, remove 'face only where it's 'bold, leave 'italic alone
    let form = r#"(let ((s (concat (propertize "AAA" 'face 'bold)
                                    (propertize "BBB" 'face 'italic)
                                    (propertize "CCC" 'face 'bold)
                                    (propertize "DDD" 'face 'italic))))
  ;; Walk and selectively remove bold faces
  (let ((pos 0)
        (len (length s)))
    (while (< pos len)
      (let ((face (get-text-property pos 'face s))
            (next (or (next-property-change pos s) len)))
        (when (eq face 'bold)
          (remove-text-properties pos next '(face nil) s))
        (setq pos next))))
  ;; Verify: bold regions should be nil, italic should remain
  (list
   (get-text-property 0 'face s)   ;; nil (was bold)
   (get-text-property 1 'face s)   ;; nil (was bold)
   (get-text-property 3 'face s)   ;; italic
   (get-text-property 5 'face s)   ;; italic
   (get-text-property 6 'face s)   ;; nil (was bold)
   (get-text-property 8 'face s)   ;; nil (was bold)
   (get-text-property 9 'face s)   ;; italic
   (get-text-property 11 'face s)  ;; italic
   ;; Property boundary count
   (let ((boundaries nil) (p 0))
     (while p
       (setq p (next-property-change p s))
       (when p (setq boundaries (cons p boundaries))))
     (nreverse boundaries))))"#;
    let expect =
        expect_test::expect![[r#""OK (nil nil italic italic nil nil italic italic (3 6 9))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Double removal: removing already-removed property returns nil
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_double_removal() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "test" 'face 'bold)))
  (let ((r1 (remove-text-properties 0 4 '(face nil) s))
        (r2 (remove-text-properties 0 4 '(face nil) s)))
    (list r1 r2
          ;; First removal changed something (t), second did not (nil)
          (not (null r1))
          (null r2)
          ;; Property is definitely gone
          (get-text-property 0 'face s))))"#;
    let expect = expect_test::expect![[r#""OK (t nil t t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

// ---------------------------------------------------------------------------
// Edge cases: zero-length range, single char, full string
// ---------------------------------------------------------------------------

#[test]
fn oracle_prop_remove_text_properties_edge_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"(let ((s (propertize "abcdef" 'face 'bold)))
  ;; Zero-length range: START = END
  (let ((r-zero (remove-text-properties 3 3 '(face nil) s)))
    ;; Single character range
    (let ((s2 (propertize "x" 'face 'italic)))
      (let ((r-single (remove-text-properties 0 1 '(face nil) s2)))
        (list
         ;; Zero range should not remove anything
         r-zero
         (get-text-property 3 'face s) ;; still bold
         ;; Single char removal
         r-single
         (get-text-property 0 'face s2) ;; nil
         ;; Full string removal
         (let ((s3 (propertize "xyz" 'face 'underline 'help-echo "h")))
           (let ((r-full (remove-text-properties 0 (length s3) '(face nil help-echo nil) s3)))
             (list r-full
                   (text-properties-at 0 s3)))))))))"#;
    let expect = expect_test::expect![[r#""OK (nil bold t nil (t nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
