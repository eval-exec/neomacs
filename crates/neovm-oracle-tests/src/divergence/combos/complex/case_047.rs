//! Complex combo batch 47 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx47_process_filter_buffer_textprop_overlay_narrow_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx47-fp*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (let ((ov (make-overlay 3 5))) (overlay-put ov 'face 'italic))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx47-fp" :command '("echo" "out")
                         :buffer buf)))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (undo)
             (list content (buffer-string)
                   (text-properties-at 0) (text-properties-at 7)
                   (length (overlays-in 1 20))
                   (get-char-property 3 'face))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx47_overlay_priority_face_display_invisible_narrow_get_char_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 9 'display "XX")
  (put-text-property 9 13 'font-lock-face 'keyword)
  (let ((o1 (make-overlay 3 8)) (o2 (make-overlay 7 14)) (o3 (make-overlay 11 18)))
    (overlay-put o1 'face 'italic) (overlay-put o2 'face 'underline) (overlay-put o3 'face 'shadow)
    (overlay-put o1 'invisible t) (overlay-put o2 'display "YY")
    (overlay-put o1 'priority 1) (overlay-put o2 'priority 5) (overlay-put o3 'priority 3))
  (narrow-to-region 2 30)
  (let (result)
    (dotimes (i 25)
      (push (list i (get-char-property (+ i 1) 'face)
                  (get-char-property (+ i 1) 'display)
                  (get-char-property (+ i 1) 'invisible)
                  (get-char-property (+ i 1) 'font-lock-face))
            result))
    (list (current-column) (string-width (buffer-substring 1 10))
          (length (overlays-in (point-min) (point-max))))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_subword_superword_toggle_forward_backward_consistency_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 1) (3 1) (17 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "my_camelCase_var test_Word")
      (goto-char 1)
      (let ((default (list (progn (forward-word 1) (point))
                           (progn (backward-word 1) (point)))))
        (goto-char 1) (subword-mode 1)
        (let ((sub (list (progn (forward-word 1) (point))
                         (progn (backward-word 1) (point)))))
          (goto-char 1) (superword-mode 1)
          (let ((super (list (progn (forward-word 1) (point))
                            (progn (backward-word 1) (point)))))
            (list default sub super)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_coding_encode_decode_region_roundtrip_all_codings_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 15 t \"café世界😀\") (utf-16be 16 t \"café世界😀\") (utf-16le 16 t \"café世界😀\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界😀"))
  (mapcar (lambda (cs)
            (with-temp-buffer
              (insert s)
              (let ((orig (buffer-string)))
                (encode-coding-region 1 (point-max) cs)
                (let ((enc-len (length (buffer-string))))
                  (decode-coding-region 1 (point-max) cs)
                  (list cs enc-len (equal orig (buffer-string)) (buffer-string))))))
          '(utf-8 utf-16be utf-16le)))
"##,
        expect,
    );
}

#[test]
fn div_cx47_cl_defmethod_combination_list_and_max_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx47-list-fn: (list)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx47-cls () ())
  (let (log)
    (cl-defgeneric neo-cx47-list-fn (obj) (:method-combination list))
    (cl-defgeneric neo-cx47-max-fn (obj) (:method-combination max))
    (cl-defmethod neo-cx47-list-fn list ((obj neo-cx47-cls)) :a)
    (cl-defmethod neo-cx47-list-fn list ((obj neo-cx47-cls)) :b)
    (cl-defmethod neo-cx47-list-fn list ((obj neo-cx47-cls)) :c)
    (cl-defmethod neo-cx47-max-fn max ((obj neo-cx47-cls)) 10)
    (cl-defmethod neo-cx47-max-fn max ((obj neo-cx47-cls)) 30)
    (cl-defmethod neo-cx47-max-fn max ((obj neo-cx47-cls)) 20)
    (list (neo-cx47-list-fn (neo-cx47-cls))
          (neo-cx47-max-fn (neo-cx47-cls)))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_json_dom_xml_parse_serialize_hash_chain_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"café\" \"世界\") \"{\\\"items\\\":[\\\"café\\\",\\\"世界\\\"]}\" [\"café\" \"世界\"] nil \"72bc78b6e2c305f1bd6d53f51032661b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((xml-dom (with-temp-buffer
                        (insert "<data><item>café</item><item>世界</item></data>")
                        (car (xml-parse-region (point-min) (point-max)))))
             (items (mapcar #'dom-text (dom-by-tag xml-dom 'item)))
             (json-enc (json-encode `((items . ,items))))
             (json-dec (json-read-from-string json-enc))
             (back-items (cdr (assoc 'items json-dec))))
        (list items json-enc back-items
              (equal items back-items)
              (secure-hash 'md5 json-enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_set_buffer_multibyte_narrow_insert_raw_bytes_toggle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (narrow-to-region 3 8)
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 202 65))
  (let ((nil-len (length (buffer-string))))
    (set-buffer-multibyte t)
    (list nil-len (length (buffer-string)) (point-min) (point-max)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_process_env_coding_exitcode_sentinel_timer_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired se)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX47=val" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX47"))))
        (exit-code
         (let ((p (make-process :name "neo-cx47-te" :command '("sh" "-c" "exit 8")
                                :sentinel (lambda (proc ev) (setq se ev))))
           (accept-process-output p 2)
           (process-exit-status p))))
    (sit-for 0.01)
    (list timer-fired env-val exit-code
          (if se (string-match "code 8" se) nil)
          (if se (string-match "code 1" se) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_window_buffer_marker_overlay_dedicated_hscroll_split_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx47-wb*")))
  (with-current-buffer buf
    (insert (make-string 30 ?x))
    (let ((m (set-marker (make-marker) 10)) (ov (make-overlay 5 15)))
      (overlay-put ov 'face 'bold)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 15)
  (set-window-hscroll (selected-window) 5)
  (set-window-dedicated-p (selected-window) t)
  (let ((cfg (current-window-configuration)))
    (split-window nil nil 'right)
    (let ((split-count (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list split-count (count-windows)
                   (window-point) (window-hscroll) (window-dedicated-p)
                   (with-current-buffer buf (marker-position (cdar buffer-markers)))
                   (with-current-buffer buf (length (overlays-at 8))))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_undo_redo_marker_overlay_textprop_narrow_display_evaporate_full_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'display "XX")
  (put-text-property 11 15 'mouse-face 'highlight)
  (let ((ov (make-overlay 8 16)) (m (set-marker (make-marker) 12)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 30)
    (undo-boundary)
    (goto-char 10) (insert "ABC")
    (undo-boundary)
    (delete-region 5 18)
    (undo-boundary)
    (put-text-property 3 8 'font-lock-face 'keyword)
    (let ((state (list (point-min) (point-max) (marker-position m)
                       (overlayp ov) (overlay-start ov)
                       (buffer-string) (text-properties-at 1) (text-properties-at 5)
                       (current-column))))
      (undo) (undo) (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlayp ov) (overlay-start ov) (overlay-end ov)
            (text-properties-at 1) (text-properties-at 5) (text-properties-at 10)
            (current-column))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_print_circle_gensym_struct_vector_propertized_escape_length_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx47-rec a b c)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-length 3) (print-level 2))
    (let ((r (make-neo-cx47-rec :a "café" :b 42 :c t))
          (gs1 (gensym)) (gs2 (gensym))
          (s (propertize "world\n\t" 'face 'bold))
          (x (list 1)))
      (setcdr x x)
      (list (prin1-to-string r)
            (prin1-to-string `(',gs1 ',gs2))
            (prin1-to-string `[,s ,s])
            (prin1-to-string x)
            (length (prin1-to-string (vector r r r)))
            (length (prin1-to-string (list 1 2 3 4 5 6 7 8 9))))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_regex_casefold_multibyte_cyrillic_greek_replace_match_data_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abcXdef\" \"abcQdef\" \"ZZZZ test\" 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (replace-regexp-in-string "σ" "X" "abcΣdef")
        (replace-regexp-in-string "я" "Q" "abcЯdef")
        (replace-regexp-in-string "[фг]" "Z" "ФГфг test")
        (with-temp-buffer
          (insert "Café Συν κόσμο")
          (goto-char 1)
          (re-search-forward "συν" nil t))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_hash_weak_key_and_value_combined_eviction_gc_stress_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key-and-value :test 'eq))
      (counts nil))
  (dotimes (i 10)
    (puthash (cons i nil) (cons (* i 10) nil) ht))
  (push (hash-table-count ht) counts)
  (garbage-collect)
  (push (hash-table-count ht) counts)
  (puthash (cons 99 nil) (cons 990 nil) ht)
  (garbage-collect)
  (push (hash-table-count ht) counts)
  (nreverse counts))
"##,
        expect,
    );
}

#[test]
fn div_cx47_cl_coerce_list_vector_string_char_cycle_propertized_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold))
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string))
       (c0 (cl-coerce (car l) 'char))
       (i0 (cl-coerce c0 'integer)))
  (list (length l) (length v) (length s2)
        c0 i0
        (text-properties-at 0 s) (text-properties-at 0 s2)
        (equal s s2)))
"##,
        expect,
    );
}

#[test]
fn div_cx47_subword_capitalize_word_multiple_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CamelcaseVariable \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "pre camelCaseVariable rest")
      (narrow-to-region 5 23)
      (goto-char 5)
      (capitalize-word 2)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_coding_decode_encode_string_region_no_conversion_raw_text_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\0A���\" \"\\0A���\" \"\\0A���\" \"\\0A���\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 0 65 128 200 255)))
  (list (decode-coding-string raw 'no-conversion)
        (decode-coding-string raw 'raw-text)
        (encode-coding-string (decode-coding-string raw 'no-conversion) 'no-conversion)
        (with-temp-buffer (set-buffer-multibyte nil) (insert raw)
                          (decode-coding-region 1 (point-max) 'raw-text)
                          (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx47_read_from_string_backquote_unquote_splice_eval_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((x "café") (y "世界") (lst '(1 2 3)))
    (list (eval (car (read-from-string "`(,x ,y)")) t)
          (eval (car (read-from-string "`(,x ,@lst ,y)")) t)
          (eval (car (read-from-string "`(:a ,x :b ,(length y) :c ,@(mapcar #'1+ lst))")) t))))
"##,
        expect,
    );
}
