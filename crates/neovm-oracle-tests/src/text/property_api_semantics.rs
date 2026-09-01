//! Oracle parity tests for GNU text property API edge semantics.
//!
//! GNU implements these primitives in `src/textprop.c`.  These tests focus on
//! return values, empty ranges, range validation, property-change limits, and
//! buffer/string indexing differences.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_replace_region_contents_does_not_inherit_adjoining_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU 31 editfns.c:Freplace_region_contents passes `false` as
    // replace_range's property-inheritance flag in both the minimal-diff and
    // fallback paths.  Its optional INHERIT argument is passed in the separate
    // adjust-match-data position.  Consequently it preserves properties owned
    // by inserted SOURCE text, but does not merge properties from adjoining
    // destination text.  This intentionally differs from insert-and-inherit.
    let form = r#"
(list
 (with-temp-buffer
   (insert "x")
   (put-text-property 1 2 'destination 'kept)
   (replace-region-contents
    1 2 (propertize "xy" 'source 'inserted) 0.1 nil 'inherit)
   (buffer-string))
 (with-temp-buffer
   (insert "x")
   (put-text-property 1 2 'destination 'kept)
   (replace-region-contents 1 2 "xy" 0.1 nil 'inherit)
   (buffer-string))
 (with-temp-buffer
   (insert "x")
   (put-text-property 1 2 'destination 'kept)
   (goto-char 2)
   (insert-and-inherit "y")
   (buffer-string)))
"#;

    assert_oracle_parity(form);
}

#[test]
fn oracle_text_property_mutator_return_values_and_empty_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "abcd")))
  (list
   (add-text-properties 0 2 '(a 1) s)
   (add-text-properties 0 2 '(a 1) s)
   (put-text-property 0 2 'b 2 s)
   (set-text-properties 2 2 '(c 3) s)
   (remove-text-properties 0 2 '(missing nil) s)
   (remove-text-properties 0 2 '(a nil) s)
   s
   (text-properties-at 0 s)
   (text-properties-at 2 s)))
"#;

    let expect =
        expect_test::expect![[r#""OK (t nil nil nil nil t #(\"abcd\" 0 2 (b 2)) (b 2) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_text_properties_at_end_and_range_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (propertize "abc" 'face 'bold)))
  (list
   (text-properties-at 0 s)
   (text-properties-at 3 s)
   (get-text-property 3 'face s)
   (condition-case err
       (text-properties-at 4 s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (text-properties-at -1 s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (put-text-property 2 1 'a 1 s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (add-text-properties 0 1 '(a) s)
     (error (list (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK ((face bold) nil nil (args-out-of-range (4 4)) (args-out-of-range (-1 -1)) nil (error (\"Odd length text property list\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_remove_text_properties_odd_plist_is_noop_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:Fremove_text_properties only uses property
    // names from PROPERTIES.  An odd trailing property name with no value is
    // ignored and returns nil if nothing was removed.
    let form = r#"
(let ((s (copy-sequence "abc")))
  (list
   (condition-case err
       (add-text-properties 0 1 '(face) s)
     (error (list (car err) (cdr err))))
   (condition-case err
       (remove-text-properties 0 1 '(face) s)
     (error (list (car err) (cdr err))))
   (text-properties-at 0 s)))
"#;

    let expect =
        expect_test::expect![[r#""OK ((error (\"Odd length text property list\")) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_remove_list_of_text_properties_allows_dotted_tail_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:Fremove_list_of_text_properties scans property
    // names with the interval helpers and does not require LIST-OF-PROPERTIES
    // to be a proper list when no listed property remains to remove.
    let form = r#"
(let ((s (propertize "abc" 'face 'bold 'help-echo "tip")))
  (list
   (remove-list-of-text-properties 0 1 '(face) s)
   (text-properties-at 0 s)
   (condition-case err
       (remove-list-of-text-properties 0 1 '(face . bold) s)
     (error (list (car err) (cdr err))))
   (text-properties-at 0 s)))
"#;

    let expect = expect_test::expect![[r#""OK (t (help-echo \"tip\") nil (help-echo \"tip\"))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_text_property_search_uses_eq_not_equal_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:Ftext_property_any and
    // Ftext_property_not_all compare property values with EQ, not equal.
    let form = r#"
(let* ((stored (copy-sequence "tip"))
       (needle (copy-sequence "tip"))
       (s (propertize "abc" 'help-echo stored)))
  (list
   (eq stored needle)
   (equal stored needle)
   (text-property-any 0 3 'help-echo needle s)
   (text-property-not-all 0 3 'help-echo needle s)))
"#;

    let expect = expect_test::expect![[r#""OK (nil t nil 0)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_set_text_properties_reports_noop_on_unpropertized_string_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:set_text_properties returns nil when removing
    // all properties from a whole string that has no intervals to remove.
    let form = r#"
(let ((plain (copy-sequence "abc"))
      (styled (propertize "abc" 'face 'bold)))
  (list
   (set-text-properties 0 3 nil plain)
   (text-properties-at 0 plain)
   (set-text-properties 0 3 nil styled)
   (text-properties-at 0 styled)))
"#;

    let expect = expect_test::expect![[r#""OK (nil nil t nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_face_text_property_preserves_dotted_face_list_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:add_properties treats a cons face value as a
    // face list unless it is an anonymous face plist, so prepending conses onto
    // even a dotted face list instead of wrapping that dotted list as one face.
    let form = r#"
(let ((s (copy-sequence "abc")))
  (set-text-properties 0 3 '(face (bold . italic)) s)
  (add-face-text-property 0 3 'underline nil s)
  (get-text-property 0 'face s))
"#;

    let expect = expect_test::expect![[r#""OK (underline bold . italic)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_face_text_property_append_rejects_dotted_face_list_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // The append branch in GNU Emacs src/textprop.c:add_properties calls
    // append on the existing face list.  Improper face lists therefore signal
    // a listp error instead of being wrapped as a single face value.
    let form = r#"
(let ((s (copy-sequence "abc")))
  (set-text-properties 0 3 '(face (bold . italic)) s)
  (condition-case err
      (add-face-text-property 0 3 'underline t s)
    (error (list (car err) (cadr err) (caddr err)))))
"#;

    let expect = expect_test::expect![[r#""OK (wrong-type-argument listp italic)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_add_face_text_property_same_face_is_noop_like_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c:add_properties checks EQ before merge-list
    // handling.  Adding the same face value again must not manufacture a
    // duplicate face list.
    let form = r#"
(let ((s (copy-sequence "abc")))
  (add-face-text-property 0 3 'bold nil s)
  (add-face-text-property 0 3 'bold t s)
  (with-temp-buffer
    (insert "abc")
    (add-face-text-property 1 4 'bold)
    (add-face-text-property 1 4 'bold t)
    (list (get-text-property 0 'face s)
          (get-text-property 1 'face))))
"#;

    let expect = expect_test::expect![[r#""OK (bold bold)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_next_previous_property_change_limit_edges() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (concat (propertize "ab" 'face 'bold)
                 (propertize "cd" 'face 'italic)
                 "ef")))
  (list
   (next-property-change 0 s)
   (next-property-change 0 s 1)
   (next-property-change 0 s 2)
   (next-property-change 2 s t)
   (next-property-change 4 s)
   (next-property-change 4 s 5)
   (previous-property-change 6 s)
   (previous-property-change 6 s 5)
   (previous-property-change 4 s t)
   (previous-property-change 2 s 1)))
"#;

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integer-or-marker-p t)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_buffer_text_property_positions_are_one_based() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(with-temp-buffer
  (insert "abcd")
  (add-text-properties 1 3 '(face bold) (current-buffer))
  (list
   (get-text-property 1 'face)
   (get-text-property 3 'face)
   (text-properties-at 1)
   (text-properties-at 5)
   (next-property-change 1 nil)
   (previous-property-change 5 nil)
   (condition-case err
       (text-properties-at 0)
     (error (list (car err) (cdr err))))))
"#;

    let expect =
        expect_test::expect![[r#""OK (bold nil (face bold) nil 3 3 (args-out-of-range (0 0)))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_text_property_search_positions_are_character_based_for_multibyte_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/textprop.c stores and returns interval positions in
    // character coordinates: string APIs use 0-based character indexes, while
    // buffer APIs use 1-based buffer positions.  These cases catch accidental
    // byte-offset exposure on multibyte text.
    let form = r#"
(let ((s (copy-sequence "aé🙂b")))
  (put-text-property 1 3 'face 'bold s)
  (list
   (length s)
   (string-bytes s)
   (next-property-change 0 s)
   (next-single-property-change 0 'face s)
   (next-property-change 1 s)
   (next-single-property-change 1 'face s)
   (previous-property-change 4 s)
   (previous-single-property-change 4 'face s)
   (text-property-any 0 4 'face 'bold s)
   (text-property-not-all 1 3 'face 'bold s)
   (text-property-not-all 0 4 'face 'bold s)
   (mapcar (lambda (i) (text-properties-at i s))
           (number-sequence 0 4))
   (with-temp-buffer
     (insert "aé🙂b")
     (put-text-property 2 4 'face 'bold)
     (list
      (point-min)
      (point-max)
      (buffer-size)
      (next-property-change 1)
      (next-single-property-change 1 'face)
      (next-property-change 2)
      (next-single-property-change 2 'face)
      (previous-property-change 5)
      (previous-single-property-change 5 'face)
      (text-property-any 1 5 'face 'bold)
      (text-property-not-all 2 4 'face 'bold)
      (text-property-not-all 1 5 'face 'bold)
      (mapcar (lambda (i) (text-properties-at i))
              (number-sequence 1 5))))))"#;

    let expect = expect_test::expect![[
        r#""OK (4 8 1 1 3 3 3 3 1 nil 0 (nil (face bold) (face bold) nil nil) (1 5 4 2 2 4 4 4 4 2 nil 1 (nil (face bold) (face bold) nil nil)))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_text_property_change_limits_accept_markers_and_return_character_positions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's next/previous property-change functions coerce marker LIMIT
    // arguments with CHECK_FIXNUM_COERCE_MARKER and still return character
    // positions.  Marker start/end arguments are likewise buffer positions,
    // not byte offsets.
    let form = r#"
(with-temp-buffer
  (insert "αβγδε")
  (put-text-property 2 5 'face 'bold)
  (let ((start (copy-marker 1))
        (middle (copy-marker 3))
        (limit-front (copy-marker 4))
        (limit-end (copy-marker 6)))
    (list
     (next-property-change start nil limit-front)
     (next-single-property-change start 'face nil limit-front)
     (next-property-change middle nil limit-end)
     (next-single-property-change middle 'face nil limit-end)
     (previous-property-change limit-end nil middle)
     (previous-single-property-change limit-end 'face nil middle)
     (text-property-any start limit-end 'face 'bold)
     (text-property-not-all middle limit-front 'face 'bold))))"#;

    let expect = expect_test::expect![[r#""OK (2 2 5 5 5 5 2 nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_insert_and_inherit_sticky_property_merge_matrix_matches_gnu() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/intervals.c:merge_properties_sticky documents the 16
    // combinations of left/right front/rear stickiness and merges properties
    // property-by-property.  Use the same matrix names as GNU's comment and
    // insert multibyte text at the boundary so interval positions and sticky
    // inheritance are both covered.
    let form = r#"
(let ((text-property-default-nonsticky nil))
  (with-temp-buffer
    (insert "LR")
    (set-text-properties
     1 2
     '(front-sticky (p8 p9 pa pb pc pd pe pf)
       rear-nonsticky (p4 p5 p6 p7 p8 p9 pa pb)
       p0 L p1 L p2 L p3 L p4 L p5 L p6 L p7 L
       p8 L p9 L pa L pb L pc L pd L pe L pf L))
    (set-text-properties
     2 3
     '(front-sticky (p2 p3 p6 p7 pa pb pe pf)
       rear-nonsticky (p1 p2 p5 p6 p9 pa pd pe)
       p0 R p1 R p2 R p3 R p4 R p5 R p6 R p7 R
       p8 R p9 R pa R pb R pc R pd R pe R pf R))
    (goto-char 2)
    (insert-and-inherit "λ")
    (list (buffer-string)
          (text-properties-at 2)
          (object-intervals (current-buffer)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"LλR\" 0 1 (front-sticky (p8 p9 pa pb pc pd pe pf) rear-nonsticky (p4 p5 p6 p7 p8 p9 pa pb) p0 L p1 L p2 L p3 L p4 L p5 L p6 L p7 L p8 L p9 L pa L pb L pc L pd L pe L pf L) 1 2 (front-sticky (p6 p7 pa pb pc pd pe pf) rear-nonsticky (p6 pa) p0 L p1 L p2 L p3 L p6 R p7 R pa R pb R pc L pd L pe L pf L) 2 3 (front-sticky (p2 p3 p6 p7 pa pb pe pf) rear-nonsticky (p1 p2 p5 p6 p9 pa pd pe) p0 R p1 R p2 R p3 R p4 R p5 R p6 R p7 R p8 R p9 R pa R pb R pc R pd R pe R pf R)) (front-sticky (p6 p7 pa pb pc pd pe pf) rear-nonsticky (p6 pa) p0 L p1 L p2 L p3 L p6 R p7 R pa R pb R pc L pd L pe L pf L) ((0 1 (front-sticky (p8 p9 pa pb pc pd pe pf) rear-nonsticky (p4 p5 p6 p7 p8 p9 pa pb) p0 L p1 L p2 L p3 L p4 L p5 L p6 L p7 L p8 L p9 L pa L pb L pc L pd L pe L pf L)) (1 2 (front-sticky (p6 p7 pa pb pc pd pe pf) rear-nonsticky (p6 pa) p0 L p1 L p2 L p3 L p6 R p7 R pa R pb R pc L pd L pe L pf L)) (2 3 (front-sticky (p2 p3 p6 p7 pa pb pe pf) rear-nonsticky (p1 p2 p5 p6 p9 pa pd pe) p0 R p1 R p2 R p3 R p4 R p5 R p6 R p7 R p8 R p9 R pa R pb R pc R pd R pe R pf R))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_insert_and_inherit_category_front_sticky_suppresses_explicit_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU Emacs src/intervals.c:merge_properties_sticky omits an explicit
    // front-sticky property in the merged plist when the inherited category
    // symbol itself has front-sticky=t.  The observable property values still
    // match, but object-intervals exposes the exact GNU interval plist shape.
    let form = r#"
(let ((text-property-default-nonsticky nil))
  (put 'oracle-sticky-category 'front-sticky t)
  (unwind-protect
      (with-temp-buffer
        (insert "LR")
        (set-text-properties 1 2 '(left L))
        (set-text-properties
         2 3
         '(category oracle-sticky-category
           front-sticky (category p)
           p R))
        (goto-char 2)
        (insert-and-inherit "λ")
        (list (buffer-string)
              (text-properties-at 2)
              (object-intervals (current-buffer))))
    (put 'oracle-sticky-category 'front-sticky nil)))
"#;

    let expect = expect_test::expect![[
        r#""OK (#(\"LλR\" 0 1 (left L) 1 2 (category oracle-sticky-category p R left L) 2 3 (category oracle-sticky-category front-sticky (category p) p R)) (category oracle-sticky-category p R left L) ((0 1 (left L)) (1 2 (category oracle-sticky-category p R left L)) (2 3 (category oracle-sticky-category front-sticky (category p) p R))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
