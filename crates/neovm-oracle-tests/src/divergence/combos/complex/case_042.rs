//! Complex combo batch 42 — more MEGA combos + extend weak-hash + word-movement.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx42_weak_hash_value_eviction_marker_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'value :test 'eq)))
  (puthash 'key (cons 1 2) ht)
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx42_weak_hash_key_eviction_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq)))
  (puthash (vector 1 2) :val ht)
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx42_subword_downcase_then_upcase_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"camelCaseVar rest\" \"CAMELCaseVar rest\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseVar rest")
      (goto-char 1)
      (downcase-word 1)
      (let ((after-down (buffer-string)))
        (goto-char 1)
        (upcase-word 1)
        (list after-down (buffer-string))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_superword_downcase_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"snake_case_var rest\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "SNAKE_CASE_VAR rest")
      (goto-char 1)
      (downcase-word 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_process_sentinel_make_vs_call_exit_detail() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"exited abnormally with code 42\\n\" 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (sentinel-ev)
  (let ((p (make-process :name "neo-cx42-se" :command '("sh" "-c" "exit 42")
                         :sentinel (lambda (proc event) (setq sentinel-ev event)))))
    (accept-process-output p 2))
  (list sentinel-ev
        (call-process "sh" nil nil nil "-c" "exit 42")))
"##,
        expect,
    );
}

#[test]
fn div_cx42_overlay_evaporate_undo_narrow_marker_textprop_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 13 18 'mouse-face 'highlight)
  (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 10)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 24)
    (undo-boundary)
    (delete-region 5 15)
    (let ((state (list (overlayp ov) (overlay-start ov)
                       (marker-position m) (text-properties-at 1))))
      (undo)
      (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)
            (text-properties-at 12) (buffer-substring 1 5)))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_coding_priority_list_exact_contents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (utf-8 iso-2022-7bit iso-latin-1 iso-2022-7bit-lock iso-2022-8bit-ss2 emacs-mule raw-text iso-2022-jp in-is13194-devanagari chinese-iso-8bit utf-8-auto utf-8-with-signature utf-16 utf-16be-with-signature utf-16le-with-signature utf-16be utf-16le japanese-shift-jis chinese-big5 undecided)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(coding-system-priority-list)
"##,
        expect,
    );
}

#[test]
fn div_cx42_set_buffer_multibyte_corruption_various_patterns() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 5 4 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (bytes)
          (with-temp-buffer
            (set-buffer-multibyte nil)
            (apply #'insert bytes)
            (set-buffer-multibyte t)
            (length (buffer-string))))
        (list (list (unibyte-string 128 129 65))
              (list (unibyte-string 200 201 202 65 66))
              (list (unibyte-string 255 128 200 65))
              (list (unibyte-string 160 161 162 163 164 65 66 67 68))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_display_property_image_spec_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display '(image :type xpm :file "test.xpm" :width 10 :height 1))
  (current-column))
"##,
        expect,
    );
}

#[test]
fn div_cx42_print_circle_gensym_quoted_struct_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx42-pt x y)
  (let ((print-circle t) (print-gensym t) (print-quoted t))
    (let ((p (make-neo-cx42-pt :x 1 :y 2))
          (gs (gensym)))
      (list (string-match "#s" (prin1-to-string p))
            (string-match "#:" (prin1-to-string gs))
            (prin1-to-string '(quote neo-cx42-pt)))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_process_output_buffer_overlay_textprop_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx42-po*")))
  (with-current-buffer buf
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (let ((ov (make-overlay 3 5)))
      (overlay-put ov 'face 'italic)))
  (let ((p (make-process :name "neo-cx42-po" :command '("echo" "output-line")
                         :buffer buf)))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (list (buffer-string)
                 (text-properties-at 0)
                 (text-properties-at 3)
                 (text-properties-at 7)
                 (get-char-property 3 'face)
                 (length (overlays-in 1 20))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx42_regex_match_data_save_replace_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range #<buffer *scratch*> 2 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (string-match "\\(café\\).*\\(世界\\)" "x café y 世界 z")
  (let ((md (match-data))
        (m1 (match-string 1))
        (m2 (match-string 2)))
    (replace-regexp-in-string "café" "CAFÉ" "le café ici")
    (set-match-data md)
    (list m1 m2 (match-string 1) (match-string 2)
          (match-beginning 1) (match-end 2))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_timer_run_idle_cancel_count_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 0 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t1 (run-with-timer 100 nil (lambda ())))
      (t2 (run-with-idle-timer 50 nil (lambda ())))
      (t3 (run-with-timer 200 nil (lambda ()))))
  (let ((active (length timer-list))
        (idle (length timer-idle-list)))
    (cancel-timer t1)
    (cancel-timer t3)
    (list active idle
          (length timer-list)
          (length timer-idle-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_window_dedicated_buffer_switch_overlay_marker_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx42-wd*")))
  (with-current-buffer buf
    (insert "0123456789")
    (let ((m (set-marker (make-marker) 5))
          (ov (make-overlay 2 6)))
      (overlay-put ov 'face 'bold)))
  (set-window-buffer (selected-window) buf)
  (set-window-dedicated-p (selected-window) t)
  (let ((ded (window-dedicated-p)))
    (set-window-dedicated-p (selected-window) nil)
    (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
    (prog1 (list ded
                 (buffer-live-p buf)
                 (with-current-buffer buf
                   (marker-position (cdar buffer-markers))
                   (length (overlays-at 3))))
      (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx42_json_encode_hashtable_plist_alist_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"{\\\"x\\\":1,\\\"y\\\":2}\" \"{\\\"a\\\":1,\\\"b\\\":\\\"café\\\"}\" \"{\\\"a\\\":1,\\\"b\\\":2}\" 13)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let ((ht (make-hash-table :test 'equal))
            (pl '(:a 1 :b "café"))
            (al '((a . 1) (b . 2))))
        (puthash "x" 1 ht) (puthash "y" 2 ht)
        (list (json-encode ht)
              (json-encode pl)
              (json-encode al)
              (length (json-encode ht)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_dom_operations_recursive_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom)
      (let ((dom '(html nil
                     (head nil (title nil "Title"))
                     (body nil
                      (div ((class . "container"))
                       (p nil "Para 1")
                       (p ((id . "p2")) "Para 2")
                       (ul nil
                        (li nil "Item 1")
                        (li nil "Item 2"))))))
        (list (length (dom-by-tag dom 'p))
              (length (dom-by-tag dom 'li))
              (dom-attr (nth 1 (dom-by-tag dom 'p)) 'id)
              (dom-text (car (dom-by-tag dom 'title)))
              (mapcar #'dom-text (dom-by-tag dom 'li))
              (dom-by-tag dom 'div))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_coding_decode_encode_string_latin_vs_utf8_compare_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 0 4 (charset iso-8859-1)) \"caf\\351\" (99 97 102 233) (99 97 102 4194281) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 99 97 102 233)))
  (list (decode-coding-string raw 'latin-1)
        (decode-coding-string raw 'utf-8)
        (append (decode-coding-string raw 'latin-1) nil)
        (condition-case e (append (decode-coding-string raw 'utf-8) nil) (error :error))
        (equal (decode-coding-string raw 'latin-1) (decode-coding-string raw 'utf-8))))
"##,
        expect,
    );
}

#[test]
fn div_cx42_set_buffer_multibyte_then_char_charset_mapping() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (eight-bit eight-bit ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65))
  (set-buffer-multibyte t)
  (mapcar #'char-charset (append (buffer-string) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx42_overlay_priority_face_invisible_display_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'display "XX")
  (let ((o1 (make-overlay 3 8)) (o2 (make-overlay 10 15)))
    (overlay-put o1 'face 'italic)
    (overlay-put o2 'face 'underline)
    (overlay-put o1 'invisible t)
    (overlay-put o1 'priority 5)
    (overlay-put o2 'priority 3))
  (narrow-to-region 2 24)
  (list (get-char-property 1 'face)
        (get-char-property 3 'face)
        (get-char-property 3 'invisible)
        (get-char-property 5 'face)
        (get-char-property 8 'display)
        (get-char-property 9 'face)
        (get-char-property 10 'face)
        (current-column)))
"##,
        expect,
    );
}
