//! Oracle parity tests for GNU `object-intervals` semantics.
//!
//! GNU implements `Fobject_intervals` in `src/fns.c`: strings and buffers
//! return a copied interval list, including explicit nil-property runs around
//! non-empty property intervals; plain objects without intervals return nil.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn oracle_object_intervals_string_and_buffer_interval_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = r#"
(let ((s (copy-sequence "abcdef"))
      (b (get-buffer-create " *object-intervals-oracle*")))
  (put-text-property 1 3 'face 'bold s)
  (put-text-property 3 6 'help-echo "tail" s)
  (with-current-buffer b
    (erase-buffer)
    (insert "abcd")
    (put-text-property 2 4 'face 'italic))
  (list
   (object-intervals "plain")
   (object-intervals s)
   (object-intervals b)
   (condition-case err
       (object-intervals 42)
     (error (cons (car err) (cdr err))))))
"#;

    let expect = expect_test::expect![[
        r#""OK (nil ((0 1 nil) (1 3 (face bold)) (3 6 (help-echo \"tail\"))) ((0 1 nil) (1 3 (face italic)) (3 4 nil)) (wrong-type-argument buffer-or-string-p 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_object_intervals_preserves_adjacent_equal_property_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU compares text properties by effective interval values, but
    // `object-intervals` still exposes the concrete interval run shape.
    // This follows src/fns.c:Fequal_including_properties/internal_equal and
    // src/textprop.c interval mutation behavior.
    let form = r#"
(let ((split (copy-sequence "xy"))
      (merged (copy-sequence "xy")))
  (put-text-property 0 1 'face 'bold split)
  (put-text-property 1 2 'face 'bold split)
  (put-text-property 0 2 'face 'bold merged)
  (list
   (object-intervals split)
   (object-intervals merged)
   (equal split merged)
   (equal-including-properties split merged)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((0 1 (face bold)) (1 2 (face bold))) ((0 2 (face bold))) t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_object_intervals_set_text_properties_merges_replaced_runs() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's `set_text_properties_1` sets every interval in the target range,
    // then merges each later changed interval left into the first changed
    // interval.  The resulting raw interval shape is observable through
    // `object-intervals`.
    let form = r#"
(let ((with-props (copy-sequence "abcdef"))
      (without-props (copy-sequence "abcdef")))
  (dolist (s (list with-props without-props))
    (put-text-property 0 2 'face 'bold s)
    (put-text-property 2 4 'face 'italic s)
    (put-text-property 4 6 'face 'underline s))
  (set-text-properties 1 5 '(category t) with-props)
  (set-text-properties 1 5 nil without-props)
  (list
   (object-intervals with-props)
   (object-intervals without-props)))
"#;

    let expect = expect_test::expect![[
        r#""OK (((0 1 (face bold)) (1 5 (category t)) (5 6 (face underline))) ((0 1 (face bold)) (1 5 nil) (5 6 (face underline))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_object_intervals_after_insert_and_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Raw interval shape after buffer edits is maintained by GNU's
    // `offset_intervals`: plain insertion creates an unpropertied run, while
    // deletion shrinks/removes interval nodes and shifts later nodes left.
    let form = r#"
(with-temp-buffer
  (insert "abcdef")
  (dotimes (i 6)
    (put-text-property (1+ i) (+ i 2) 'slot i))
  (goto-char 3)
  (insert "XX")
  (let ((after-insert (object-intervals (current-buffer))))
    (delete-region 3 6)
    (list after-insert
          (object-intervals (current-buffer)))))
"#;

    let expect = expect_test::expect![[
        r#""OK (((0 1 (slot 0)) (1 2 (slot 1)) (2 4 nil) (4 5 (slot 2)) (5 6 (slot 3)) (6 7 (slot 4)) (7 8 (slot 5))) ((0 1 (slot 0)) (1 2 (slot 1)) (2 3 (slot 3)) (3 4 (slot 4)) (4 5 (slot 5))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_object_intervals_multibyte_positions_and_edit_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU src/fns.c:collect_interval exposes `interval->position` and
    // `position + LENGTH(interval)`, which are character positions.  This
    // covers multibyte strings and buffer edits where byte and character
    // offsets differ.
    let form = r#"
(let ((s (copy-sequence "aé🙂b")))
  (put-text-property 1 3 'face 'bold s)
  (list
   (length s)
   (string-bytes s)
   (object-intervals s)
   (with-temp-buffer
     (insert "aé🙂b")
     (put-text-property 2 4 'face 'bold)
     (goto-char 3)
     (insert "λ")
     (let ((after-insert (object-intervals (current-buffer))))
       (delete-region 3 4)
       (list
        (buffer-string)
        after-insert
        (object-intervals (current-buffer)))))))"#;

    let expect = expect_test::expect![[
        r#""OK (4 8 ((0 1 nil) (1 3 (face bold)) (3 4 nil)) (#(\"aé🙂b\" 1 2 (face bold) 2 3 (face bold)) ((0 1 nil) (1 2 (face bold)) (2 3 nil) (3 4 (face bold)) (4 5 nil)) ((0 1 nil) (1 2 (face bold)) (2 3 (face bold)) (3 4 nil))))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
