//! Complex combo batch 34 — final niche edges: window-margins, display-table,
//! seq set ops, map-do, cl-defmethod :extra, read #s with numeric slots,
//! line-prefix + fill, char-table-decode for more charsets.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx34_window_margins_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((nil) (4 . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig (window-margins)))
  (set-window-margins (selected-window) 4 2)
  (let ((after (window-margins)))
    (apply #'set-window-margins (selected-window) orig)
    (list orig after)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_buffer_display_table_char_width_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (let ((dt (make-display-table)))
    (aset dt ?a (vector ?X ?Y ?Z))
    (setq buffer-display-table dt))
  (list (current-column) (string-width "abc")))
"##,
        expect,
    );
}

#[test]
fn div_cx34_seq_union_intersection_difference() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 2 3 4 5 6) (3 4) (1 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'seq)
  (let ((a '(1 2 3 4)) (b '(3 4 5 6)))
    (list (sort (seq-union a b) #'<)
          (sort (seq-intersection a b) #'<)
          (sort (seq-difference a b) #'<))))
"##,
        expect,
    );
}

#[test]
fn div_cx34_map_do_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:not-available)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((al '((a . 1) (b . 2))) (results nil))
      (map-do (lambda (k v) (push (format "%s=%d" k v) results)) al)
      (sort results #'string<))
  (void-function (list :not-available))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx34_cl_defmethod_extra_qualifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (42 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx34-cls () ((val :initarg :val)))
  (let (extra-log)
    (cl-defgeneric neo-cx34-fn (obj))
    (cl-defmethod neo-cx34-fn :extra "my-tag" ((obj neo-cx34-cls))
      (push :extra-fired extra-log))
    (cl-defmethod neo-cx34-fn ((obj neo-cx34-cls))
      (oref obj val))
    (list (neo-cx34-fn (neo-cx34-cls :val 42))
          extra-log)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_read_s_with_numeric_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (foo 1 bar \"x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((p "(#s(foo 1 2 3) #s(bar \"x\" #(\"y\" 0 1 (face bold))))")
       (back (car (read-from-string p))))
  (list (aref (car back) 0) (aref (car back) 1)
        (aref (cadr back) 0) (aref (cadr back) 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_line_prefix_wrap_prefix_fill_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"alpha bravo\\ncharlie delta\\necho\\n\" 0 11 (line-prefix \"> \") 11 12 (line-prefix \"> \") 12 25 (line-prefix \"> \") 25 26 (line-prefix \"> \") 26 31 (line-prefix \"> \"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 15))
    (insert "alpha bravo charlie delta echo\n")
    (put-text-property 1 (point-max) 'line-prefix "> ")
    (fill-region (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_char_table_decode_char_latin_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((c1 (decode-char 'latin-iso8859-1 41))
      (c2 (decode-char 'latin-iso8859-1 45))
      (c3 (decode-char 'katakana-jisx0201 161)))
  (list c1 c2 c3
        (encode-char c1 'latin-iso8859-1)
        (encode-char c3 'katakana-jisx0201)
        (char-charset c1) (char-charset c3)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_process_sentinel_exit_code_exit_signal_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (exit 5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx34-sc" :command '("sh" "-c" "exit 5"))))
  (accept-process-output p 2)
  (list (process-status p) (process-exit-status p) (process-live-p p)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_decode_coding_string_charset_prop_text() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99 97 102 233) (charset iso-8859-1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (decode-coding-string (unibyte-string 99 97 102 233) 'latin-1)))
  (list (append d nil) (text-properties-at 0 d)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_cl_setf_plist_put_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-setf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pl nil))
  (cl-setf (plist-get pl :a) 1)
  (cl-setf (plist-get pl :b) 2)
  (cl-setf (plist-get pl :a) 99)
  (list pl (plist-get pl :a) (plist-get pl :b) (plist-get pl :c)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_overlay_stack_priority_invisible_face_get_char_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (italic italic underline t underline shadow)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (let ((o1 (make-overlay 1 16)) (o2 (make-overlay 3 8)) (o3 (make-overlay 5 12)))
    (overlay-put o1 'face 'italic)
    (overlay-put o2 'face 'underline)
    (overlay-put o3 'face 'shadow)
    (overlay-put o3 'invisible t)
    (overlay-put o1 'priority 1) (overlay-put o2 'priority 5) (overlay-put o3 'priority 3))
  (list (get-char-property 1 'face)
        (get-char-property 2 'face)
        (get-char-property 4 'face)
        (get-char-property 5 'invisible)
        (get-char-property 6 'face)
        (get-char-property 9 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_string_bytes_of_char_to_string_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 0x7ff)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cp) (string-bytes (char-to-string cp)))
        '(0 65 127 128 255 256 0x7ff 0x800 0xffff 0x10000 0x10ffff 0x3fffff))
"##,
        expect,
    );
}

#[test]
fn div_cx34_set_buffer_multibyte_then_char_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (102 102 102)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café")
  (let ((before (char-after 3)))
    (set-buffer-multibyte nil)
    (let ((unibyte-char (char-after 3)))
      (set-buffer-multibyte t)
      (list before unibyte-char (char-after 3)))))
"##,
        expect,
    );
}

#[test]
fn div_cx34_undo_after_format_replace_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"CAFé 世界 HELLO\\n\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café 世界 hello\n")
  (undo-boundary)
  (goto-char 1)
  (while (re-search-forward "[a-z]+" nil t)
    (replace-match (upcase (match-string 0))))
  (let ((after (buffer-string)))
    (undo)
    (list after (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx34_window_total_size_width_stable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h (window-total-height)) (w (window-total-width)))
  (list (> h 0) (> w 0) (>= h 1) (>= w 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_coding_system_get_all_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 85 t coding-category-utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pl (coding-system-plist 'utf-8)))
  (list (plist-get pl :name)
        (plist-get pl :coding-type)
        (plist-get pl :mnemonic)
        (plist-get pl :ascii-compatible-p)
        (plist-get pl :category)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_cl_loop_sum_maximize_count_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for x in '(1 2 3 4 5 6 7 8 9 10)
         when (cl-evenp x) sum x into even-sum
         and count t into even-count
         when (cl-oddp x) maximize x into max-odd
         finally (return (list even-sum even-count max-odd)))
"##,
        expect,
    );
}

#[test]
fn div_cx34_buffer_hash_consistent_across_same_content_buffers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h1 (with-temp-buffer (insert "same content here") (buffer-hash)))
      (h2 (with-temp-buffer (insert "same content here") (buffer-hash)))
      (h3 (with-temp-buffer (insert "same content here ") (buffer-hash))))
  (list (equal h1 h2) (not (equal h1 h3))))
"##,
        expect,
    );
}

#[test]
fn div_cx34_marker_relocation_after_store_substring_on_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 \"abcdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((m (set-marker (make-marker) 4)))
    (store-substring (buffer-string) 2 ?X)
    (list (marker-position m) (buffer-string))))
"##,
        expect,
    );
}
