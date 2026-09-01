//! Complex combo batch 52 — continued MEGA combos + fresh edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx52_dom_serialize_to_xml_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom) (require 'xml)
      (let ((dom '(html nil (body nil (p nil "café世界") (div ((class . "x")) "data"))))
        (with-temp-buffer
          (dom-print dom (current-buffer))
          (buffer-string))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_subword_backward_word_then_delete_region_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "pre camelCaseVar rest more")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 18)) (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'italic)
        (narrow-to-region 3 29)
        (goto-char 18)
        (backward-word 1)
        (let ((pos (point)))
          (delete-region pos 18)
          (let ((state (list (buffer-string) (marker-position m) (overlay-start ov))))
            (undo)
            (list state pos (buffer-string) (marker-position m)
                  (overlay-start ov) (text-properties-at 1))))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_process_env_coding_sentinel_exitcode_timer_window_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired se)
  (run-with-timer 0 nil (lambda () (setq timer-fired :t)))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX52=val" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX52"))))
        (exit-code
         (let ((p (make-process :name "neo-cx52-te" :command '("sh" "-c" "exit 9")
                                :sentinel (lambda (proc ev) (setq se ev))))
           (accept-process-output p 2)
           (process-exit-status p)))
        (coding-val
         (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix))
             (call-process "printf" nil t nil "%s" "café世界"))
           (buffer-string)))
        (win-val
         (let ((buf (get-buffer-create " *neo-cx52-w*")))
           (with-current-buffer buf (insert "data"))
           (set-window-buffer (selected-window) buf)
           (narrow-to-region 1 3)
           (let ((r (list (point-min) (point-max) (buffer-string)))
             (with-current-buffer buf (widen) r))))
    (sit-for 0.01)
    (list timer-fired env-val exit-code coding-val win-val
          (if se (string-match "code 9" se) nil)
          (if se (string-match "code 1" se) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_overlay_priority_face_invisible_display_evaporate_undo_narrow_display_column_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 9 'display "XX")
  (let ((o1 (make-overlay 3 8)) (o2 (make-overlay 10 16)))
    (overlay-put o1 'face 'italic) (overlay-put o2 'face 'underline)
    (overlay-put o1 'invisible t) (overlay-put o1 'priority 5) (overlay-put o2 'priority 3)
    (overlay-put o1 'evaporate t))
  (narrow-to-region 2 24)
  (let ((col1 (current-column)))
    (goto-char 7) (insert "Z")
    (let ((state (list (buffer-string) (current-column)
                       (get-char-property 3 'face)
                       (get-char-property 3 'invisible)
                       (get-char-property 8 'display)
                       (length (overlays-in (point-min) (point-max))))))
      (undo)
      (list col1 state (buffer-string) (current-column)
            (get-char-property 3 'face)
            (length (overlays-in (point-min) (point-max))))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_json_deeply_nested_with_hashtable_and_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let ((ht (make-hash-table :test 'equal)))
        (puthash "key" "café" ht) (puthash "val" 42 ht)
        (let* ((data `((nested . ((deep . ,ht) (arr . [1 "世界" t])))
                       (list . ("a" "b" "😀"))))
               (enc (json-encode data))
               (dec (json-read-from-string enc)))
          (list (length enc)
                (secure-hash 'md5 enc)
                (cdr (assoc 'deep (cdr (assoc 'nested dec)))))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_cl_defmethod_around_before_after_primary_static_eql_advice_error_call_next_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx52-fn: (:static)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx52-cls () ((val :initarg :val)))
  (let (log)
    (cl-defgeneric neo-cx52-fn (obj))
    (cl-defmethod neo-cx52-fn :static ((c neo-cx52-cls)) (push :static log) :cls)
    (cl-defmethod neo-cx52-fn :before ((obj neo-cx52-cls)) (push :before log))
    (cl-defmethod neo-cx52-fn :around ((obj neo-cx52-cls))
      (push :around-in log)
      (let ((r (if (> (oref obj val) 0) (cl-call-next-method) :skipped)))
        (push :around-out log) r))
    (cl-defmethod neo-cx52-fn ((obj (eql :special))) (push :eql log) :eql-result)
    (cl-defmethod neo-cx52-fn ((obj neo-cx52-cls)) (push :primary log) (oref obj val))
    (cl-defmethod neo-cx52-fn :after ((obj neo-cx52-cls)) (push :after log))
    (advice-add 'neo-cx52-fn :around (lambda (fn &rest a)
                                        (push :advice log) (apply fn a)))
    (list (neo-cx52-fn (neo-cx52-cls :val 42))
          (neo-cx52-fn (neo-cx52-cls :val -1))
          (neo-cx52-fn :special)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_read_from_string_multiple_forms_backquote_comma_splice_quote_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t) (x 10) (y 20) (lst '(1 2 3)))
  (let ((input "`(,x ,y) `(,@lst end) '(a . b) #'sym #'(lambda (x) x)")
        (pos 0)
        (forms nil))
    (while (< pos (length input))
      (let ((r (read-from-string input pos)))
        (push (car r) forms)
        (setq pos (cdr r))))
    (let ((parsed (nreverse forms)))
      (list (length parsed)
            (eval (nth 0 parsed) t)
            (eval (nth 1 parsed) t)
            (nth 2 parsed)
            (nth 3 parsed)))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_print_circle_gensym_struct_vector_propertized_escape_length_level_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx52-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-escape-newlines t) (print-escape-nonascii t)
        (print-escape-multibyte t) (print-length 2) (print-level 2))
    (let ((r1 (make-neo-cx52-rec :a "café" :b 1))
          (r2 (make-neo-cx52-rec :a "世界" :b 2))
          (gs (gensym))
          (s (propertize "data\n" 'face 'bold))
          (circ (list 0)))
      (setcdr circ circ)
      (list (prin1-to-string (vector r1 r2 r1 r2))
            (prin1-to-string `(',gs ',gs))
            (prin1-to-string s)
            (length (prin1-to-string (vector r1 r2 r1 r2)))
            (length (prin1-to-string (list (list 1 2 3) (list 4 5) (list 6)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_set_buffer_multibyte_narrow_marker_overlay_textprop_display_evaporate_undo_full_chain_mega()
 {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 9 'display "XX")
  (let ((ov (make-overlay 10 18)) (m (set-marker (make-marker) 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 30)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((nil-1 (list (length (buffer-string)) (marker-position m) (overlayp ov)
                     (text-properties-at 1))))
      (set-buffer-multibyte t)
      (let ((back-1 (list (length (buffer-string)) (marker-position m) (overlayp ov)
                         (text-properties-at 1))))
        (undo-boundary)
        (delete-region 8 22)
        (let ((del-1 (list (length (buffer-string)) (marker-position m) (overlayp ov)
                          (text-properties-at 1))))
          (undo) (undo)
          (widen)
          (list nil-1 back-1 del-1 (length (buffer-string))
                (marker-position m) (overlayp ov) (overlay-start ov)
                (text-properties-at 1) (text-properties-at 5)
                (current-column) (buffer-string)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_superword_subword_mode_off_default_word_behavior_restored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "snake_case_var camelCase")
      (goto-char 1)
      (subword-mode 1)
      (forward-word 1)
      (let ((sub-pos (point)))
        (subword-mode -1)
        (goto-char 1)
        (forward-word 1)
        (list sub-pos (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx52_weak_hash_all_weakness_types_eviction_gc_stress_2_rounds_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((key . 0) (value . 0) (key-and-value . 0) (key-or-value . 0))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((results nil))
  (dolist (weak '(key value key-and-value key-or-value))
    (let ((ht (make-hash-table :weakness weak :test 'eq)))
      (dotimes (i 5)
        (puthash (cons i nil) (cons (* i 10) nil) ht))
      (garbage-collect)
      (push (cons weak (hash-table-count ht)) results)))
  (nreverse results))
"##,
        expect,
    );
}

#[test]
fn div_cx52_process_send_string_newline_filter_buffer_textprop_overlay_narrow_undo_evaporate_mega()
{
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument processp 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx52-sf*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold)
    (let ((ov (make-overlay 3 5))) (overlay-put ov 'face 'italic) (overlay-put ov 'evaporate t))
    (narrow-to-region 1 7))
  (let ((p (make-process :name "neo-cx52-sf" :command '("cat")
                         :buffer buf :connection-type 'pipe)))
    (process-send-string buf "café\n")
    (accept-process-output 1))
  (prog1 (with-current-buffer buf
           (widen)
           (let ((content (buffer-string)))
             (undo)
             (list content (buffer-string)
                   (text-properties-at 0) (text-properties-at 7)
                   (length (overlays-in 1 20)))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx52_coding_priority_list_utf8_auto_utf16_big5_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((prio (coding-system-priority-list)))
  (list (length prio)
        (cl-position 'utf-8 prio)
        (cl-position 'utf-8-auto prio)
        (cl-position 'utf-16 prio)
        (cl-position 'big5 prio)))
"#,
        expect,
    );
}

#[test]
fn div_cx52_window_config_register_split_merge_marker_overlay_dedicated_hscroll_vscroll_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((buf (get-buffer-create " *neo-cx52-wc*")))
  (with-current-buffer buf
    (insert (make-string 60 ?x))
    (let ((m (set-marker (make-marker) 20)) (ov (make-overlay 10 30)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 5 'face 'italic)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 25)
  (set-window-hscroll (selected-window) 5)
  (set-window-vscroll (selected-window) 2)
  (set-window-dedicated-p (selected-window) t)
  (let ((cfg (current-window-configuration)))
    (set-register ?w cfg)
    (split-window nil nil 'right)
    (split-window nil nil 'below)
    (let ((split3 (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list split3 (count-windows)
                   (window-point) (window-hscroll) (window-vscroll) (window-dedicated-p)
                   (with-current-buffer buf (marker-position (cdar buffer-markers)))
                   (with-current-buffer buf (length (overlays-at 15)))
                   (with-current-buffer buf (text-properties-at 1)))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf))))
"#,
        expect,
    );
}

#[test]
fn div_cx52_regex_casefold_replace_then_undo_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let ((case-fold-search t))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Café Résumé straße 世界")
    (put-text-property 1 4 'face 'bold)
    (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 8)))
      (overlay-put ov 'face 'italic)
      (narrow-to-region 1 22)
      (goto-char 1)
      (while (re-search-forward "[a-zéàüß]+" nil t)
        (replace-match (upcase (match-string 0))))
      (let ((state (list (buffer-string) (marker-position m)
                         (overlay-start ov) (text-properties-at 1))))
        (undo)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (text-properties-at 1))))))
"#,
        expect,
    );
}

#[test]
fn div_cx52_cl_coerce_list_vector_string_char_multibyte_propertized_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(let* ((s (propertize "café世界" 'face 'bold))
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string))
       (ht (make-hash-table :test 'equal)))
  (puthash s2 1 ht)
  (puthash s 2 ht)
  (list (length s) (length l) (length v) (length s2)
        (text-properties-at 0 s) (text-properties-at 0 s2)
        (equal s s2)
        (hash-table-count ht)))
"#,
        expect,
    );
}

#[test]
fn div_cx52_json_xml_dom_chain_with_secure_hash_and_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"café\" \"世界\" \"😀\") [\"café\" \"世界\" \"😀\"] nil \"d8ede49ad9c8f687a4a5d20caf364cdd63b0d804838675be9d1f1f4bd093e30f\" nil ((i nil \"café\") (i nil \"世界\") (i nil \"😀\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((xml-dom (with-temp-buffer
                        (insert "<root><items><i>café</i><i>世界</i><i>😀</i></items></root>")
                        (car (xml-parse-region (point-min) (point-max)))))
             (items (mapcar #'dom-text (dom-by-tag xml-dom 'i)))
             (json-str (json-encode `((data . ,items))))
             (json-dec (json-read-from-string json-str))
             (back (cdr (assoc 'data json-dec))))
        (list items back (equal items back)
              (secure-hash 'sha256 json-str)
              (dom-attr (car (dom-by-tag xml-dom 'items)) nil)
              (dom-children (car (dom-by-tag xml-dom 'items))))))
  (error (cons 'errored (car e))))
"#,
        expect,
    );
}

#[test]
fn div_cx52_subword_kill_word_then_capitalize_then_undo_marker_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "pre camelCaseVar rest more")
      (put-text-property 1 3 'face 'bold)
      (let ((ov (make-overlay 5 18)) (m (set-marker (make-marker) 10)))
        (overlay-put ov 'face 'italic)
        (narrow-to-region 3 29)
        (goto-char 5) (kill-word 1)
        (let ((after-kill (buffer-string)))
          (goto-char 5) (capitalize-word 1)
          (let ((after-cap (buffer-string)))
            (undo) (undo)
            (list after-kill after-cap (buffer-string)
                  (marker-position m) (overlay-start ov)
                  (text-properties-at 1))))))
  (error (cons 'errored (car e))))
"#,
        expect,
    );
}
