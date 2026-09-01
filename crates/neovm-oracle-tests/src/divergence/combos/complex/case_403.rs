//! Complex combo batch 403 — 20 probes targeting the 11 freshly-confirmed
//! divergences from batches 400-402 plus new interaction surfaces: BOM on
//! coding-system aliases, overlay-lists after complex edits, set-buffer-
//! multibyte on large buffers, display property with string-width/truncation,
//! encode-time with mixed integer/float, search-backward edge ordering,
//! space :align-to display spec, vertical-motion over multiple lines,
//! string-collate-lessp with locale and case, buffer-local-variables
//! vs standard bindings, process filter with process-buffer, face
//! foreground inheritance chain, window-body-width with display prop,
//! regexp case-fold with char-range, compare-strings with multibyte,
//! char-before with display property, field property with motion,
//! next-single-char-property-change edge, and set-window-configuration
//! with overlay state.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// BOM presence when coding-system is specified via alias
/// (utf-8-with-signature vs utf-8-sig): both should emit BOM.
#[test]
fn div_cx403_bom_coding_system_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (coding-system-error utf-8-sig)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-bom2-")))
  (let ((coding-system-for-write 'utf-8-sig))
    (write-region "abc" nil f nil 0))
  (prog1 (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents f)
           (let* ((bytes (buffer-string))
                  (bom (and (>= (length bytes) 3)
                            (= (aref bytes 0) #xef)
                            (= (aref bytes 1) #xbb)
                            (= (aref bytes 2) #xbf))))
             (list bom (string-bytes bytes))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

/// overlay-lists: more complex edit patterns before querying.
#[test]
fn div_cx403_overlay_lists_complex_edit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 0 2 7 4 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee fff")
  (let ((o1 (make-overlay 2 8))
        (o2 (make-overlay 6 14))
        (o3 (make-overlay 12 20)))
    (goto-char 4)
    (delete-char 3)
    (insert "XX")
    (goto-char 8)
    (insert "YYY")
    (list (length (car (overlay-lists)))
          (length (cdr (overlay-lists)))
          (overlay-start o1) (overlay-end o1)
          (overlay-start o2) (overlay-end o2))))
"##,
        expect,
    );
}

/// set-buffer-multibyte data loss: larger buffer with raw bytes
/// interspersed with ASCII, check all chars survive.
#[test]
fn div_cx403_set_buf_multibyte_larger_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (8 8 12 \"\\310A\\311B\\312C\\313D\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 65 201 66 202 67 203 68))
  (let ((original (buffer-string)))
    (set-buffer-multibyte t)
    (list (length original)
          (length (buffer-string))
          (string-bytes (buffer-string))
          (buffer-string))))
"##,
        expect,
    );
}

/// Display property with string-width/truncate:
/// visual width should reflect display substitution.
#[test]
fn div_cx403_string_width_display_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 3 #(\"abc…\" 2 3 (display \"XXXXX\")))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "abcde"))
  (put-text-property 2 4 'display "XXXXX" s)
  (list (string-width s)
        (string-width (substring s 0 3))
        (truncate-string-to-width s 4 nil nil t)))
"##,
        expect,
    );
}

/// encode-time: mixed integer/float in different slots.
#[test]
fn div_cx403_encode_time_mixed_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((27185 38470) (501485565986148219617280 . 0) wrong-type-argument wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (encode-time 30 30 14 16 6 2026 nil) (error (car e)))
      (condition-case e (encode-time 30.0 30 14 16 6 2026 nil) (error (car e)))
      (condition-case e (encode-time 30 30 14.5 16 6 2026 nil) (error (car e)))
      (condition-case e (encode-time 30 30 14 16 6 2026.0 nil) (error (car e))))
"##,
        expect,
    );
}

/// search-backward / search-forward ordering with case-fold
/// and Greek text at word boundaries.
#[test]
fn div_cx403_search_casefold_greek_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 nil 15 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "alpha β gamma δ epsilon")
    (let (results)
      (goto-char (point-max))
      (push (search-backward "β" nil t) results)
      (push (search-backward "Β" nil t) results)
      (goto-char (point-max))
      (push (search-backward "δ" nil t) results)
      (push (search-backward "Δ" nil t) results)
      (nreverse results))))
"##,
        expect,
    );
}

/// display :align-to space spec: column alignment may differ.
#[test]
fn div_cx403_display_align_to_spec() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a")
  (put-text-property 1 2 'display '(space :align-to 20))
  (list (current-column)
        (progn (forward-char 1) (current-column))))
"##,
        expect,
    );
}

/// vertical-motion over multiple lines with display glyphs.
#[test]
fn div_cx403_vertical_motion_multi_line_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (13 25 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc def ghi\njkl mno pqr\nstu vwx yz")
  (put-text-property 1 2 'display "MMMM")
  (put-text-property 14 15 'display "NN")
  (list (progn (goto-char 1) (vertical-motion 1) (point))
        (progn (vertical-motion 1) (point))
        (progn (goto-char 1) (vertical-motion 2) (current-column))))
"##,
        expect,
    );
}

/// string-collate-lessp with case-fold and different locales.
#[test]
fn div_cx403_string_collate_case_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t (\"a\" \"A\" \"B\" \"c\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-collate-lessp "a" "B")
      (string-collate-lessp "B" "a")
      (string-collate-lessp "a" "B" nil t)
      (sort '("B" "a" "c" "A") #'string-collate-lessp))
"##,
        expect,
    );
}

/// buffer-local-variables: count with various buffer-local settings
/// vs standard default bindings.
#[test]
fn div_cx403_buffer_local_vars_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (21 (neo-cx403-a . 1) (neo-cx403-b . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((s1 (make-local-variable 'neo-cx403-a))
        (s2 (make-local-variable 'neo-cx403-b)))
    (setq neo-cx403-a 1)
    (setq neo-cx403-b 2)
    (let ((locals (buffer-local-variables)))
      (list (length locals)
            (assq 'neo-cx403-a locals)
            (assq 'neo-cx403-b locals)))))
"##,
        expect,
    );
}

/// Process filter using process-buffer to insert: output collection.
#[test]
fn div_cx403_process_filter_insert_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx403-proc*"))
      (proc (make-process :name "neo-cx403-proc"
                          :command '("sh" "-c" "echo hello from proc")
                          :connection-type 'pipe :buffer buf)))
  (accept-process-output proc 2)
  (prog1 (with-current-buffer buf
           (string-trim-right (buffer-string)))
    (kill-buffer buf)))
"##,
        expect,
    );
}

/// face foreground inheritance chain: check inherited face
/// attribute propagation.
#[test]
fn div_cx403_face_inherit_foreground() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"red\" \"unspecified-bg\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((parent (make-face 'neo-cx403-parent))
      (child (make-face 'neo-cx403-child)))
  (set-face-attribute 'neo-cx403-parent nil :foreground "red")
  (set-face-attribute 'neo-cx403-child nil :inherit 'neo-cx403-parent)
  (list (face-attribute 'neo-cx403-child :foreground nil 'default)
        (face-attribute 'neo-cx403-child :background nil 'default)))
"##,
        expect,
    );
}

/// window-body-width with display property and invisible text:
/// body width affected by display glyphs.
#[test]
fn div_cx403_window_body_width_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (80 23)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a b c d e f g h i j")
  (put-text-property 3 4 'display "XXXXXXXXXXXX")
  (put-text-property 7 8 'invisible t)
  (list (window-body-width)
        (window-body-height)))
"##,
        expect,
    );
}

/// Regexp case-fold with character ranges and multibyte.
#[test]
fn div_cx403_regex_casefold_char_range() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<killed buffer> 0 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "alpha BETA gamma delta")
    (list (re-search-forward "[a-z]+" nil t)
          (match-string 0)
          (re-search-forward "[A-Z]+" nil t)
          (match-string 0))))
"##,
        expect,
    );
}

/// compare-strings with multibyte and case-fold: string comparison
/// across multibyte boundaries.
#[test]
fn div_cx403_compare_strings_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (compare-strings "café" 0 nil "CAFÉ" 0 nil)
        (compare-strings "straße" 0 nil "STRASSE" 0 nil)
        (compare-strings "abc" 0 nil "ABC" 0 nil t)))
"##,
        expect,
    );
}

/// char-before with display property: what char does point
/// see before it when display substitution is active.
#[test]
fn div_cx403_char_before_display_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 98 97)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcde")
  (put-text-property 2 3 'display "XXXX" )
  (list (progn (goto-char 4) (char-before (point)))
        (progn (goto-char 3) (char-before (point)))
        (progn (goto-char 2) (char-before (point)))))
"##,
        expect,
    );
}

/// field property with motion between fields:
/// forward/backward-char across field boundaries.
#[test]
fn div_cx403_field_property_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 5 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaaa bbbb cccc dddd")
  (put-text-property 1 5 'field 'a)
  (put-text-property 6 10 'field 'b)
  (put-text-property 11 15 'field 'c)
  (goto-char 1)
  (list (field-beginning)
        (field-end)
        (progn (goto-char 5) (field-end))
        (progn (goto-char 6) (field-beginning))))
"##,
        expect,
    );
}

/// next-single-char-property-change edge cases near
/// buffer boundaries with narrowing.
#[test]
fn div_cx403_next_single_char_prop_change_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 7 10 'face 'italic)
  (narrow-to-region 2 9)
  (list (next-single-char-property-change 1 'face)
        (next-single-char-property-change 4 'face)
        (next-single-char-property-change 8 'face)))
"##,
        expect,
    );
}

/// window-configuration roundtrip with overlay state:
/// save/restore window config preserves overlay visibility.
#[test]
fn div_cx403_window_config_overlay_state() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t \"visible text\\nmore text\\nfinal text\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "visible text\nmore text\nfinal text")
  (let ((ov (make-overlay 1 13)))
    (overlay-put ov 'invisible t)
    (let ((config (current-window-configuration)))
      (set-window-configuration config)
      (list (length (overlays-in 1 20))
            (overlay-get ov 'invisible)
            (buffer-string)))))
"##,
        expect,
    );
}

/// json-encode with different data types: list, vector, hash,
/// string with unicode, symbol, number.
#[test]
fn div_cx403_json_encode_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function json-encode)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((json-encoding-type 'json-object-type))
  (list (json-encode '(:a 1 :b 2))
        (json-encode [1 2 3])
        (json-encode "café")
        (json-encode 42)
        (json-encode t)
        (json-encode nil)))
"##,
        expect,
    );
}
