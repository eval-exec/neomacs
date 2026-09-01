//! Complex combo batch 46 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx46_process_sentinel_filter_buffer_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx46-sf*")) (se nil))
  (with-current-buffer buf (buffer-enable-undo) (insert "ORIG"))
  (let ((p (make-process :name "neo-cx46-sf" :command '("echo" "output")
                         :buffer buf :sentinel (lambda (proc ev) (setq se ev)))))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (let ((content (buffer-string)))
             (undo)
             (list content (buffer-string) se
                   (if se (string-match "finished" se) nil))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_overlay_stack_priority_invisible_display_face_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'font-lock-face 'keyword)
  (let ((o1 (make-overlay 3 8)) (o2 (make-overlay 7 14)) (o3 (make-overlay 10 18)))
    (overlay-put o1 'face 'italic)
    (overlay-put o2 'face 'underline)
    (overlay-put o3 'face 'shadow)
    (overlay-put o1 'invisible t)
    (overlay-put o2 'display "XX")
    (overlay-put o1 'priority 1) (overlay-put o2 'priority 5) (overlay-put o3 'priority 3))
  (narrow-to-region 2 24)
  (list (get-char-property 1 'face)
        (get-char-property 3 'face)
        (get-char-property 3 'invisible)
        (get-char-property 6 'face)
        (get-char-property 6 'font-lock-face)
        (get-char-property 6 'display)
        (get-char-property 9 'face)
        (get-char-property 10 'face)
        (get-char-property 13 'face)
        (current-column)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_set_buffer_multibyte_overlay_marker_text_prop_display_evaporate_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café世界hello")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'display "XX")
  (let ((ov (make-overlay 5 8)) (m (set-marker (make-marker) 6)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((nil-state (list (length (buffer-string)) (marker-position m)
                           (overlay-start ov) (text-properties-at 1)
                           (current-column))))
      (set-buffer-multibyte t)
      (let ((back-state (list (length (buffer-string)) (marker-position m)
                              (overlay-start ov) (text-properties-at 1))))
        (undo)
        (list nil-state back-state (length (buffer-string))
              (marker-position m) (overlay-start ov) (overlayp ov)
              (text-properties-at 1) (text-properties-at 5)
              (current-column) (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_cl_defmethod_multiple_dispatch_eql_advice_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:ab :aany :eql (:advice :a-b :advice :a-any :advice :eql))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx46-a () ()) (defclass neo-cx46-b () ())
  (let (log)
    (cl-defgeneric neo-cx46-fn (x y))
    (cl-defmethod neo-cx46-fn ((x neo-cx46-a) (y neo-cx46-b))
      (push :a-b log) :ab)
    (cl-defmethod neo-cx46-fn ((x neo-cx46-a) y)
      (push :a-any log) :aany)
    (cl-defmethod neo-cx46-fn ((x (eql :special)) y)
      (push :eql log) :eql)
    (advice-add 'neo-cx46-fn :around (lambda (fn &rest a)
                                        (push :advice log) (apply fn a)))
    (list (neo-cx46-fn (neo-cx46-a) (neo-cx46-b))
          (neo-cx46-fn (neo-cx46-a) 42)
          (neo-cx46-fn :special 99)
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_coding_encode_decode_string_region_all_codings_consistency_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((utf-8 nil t) (latin-1 nil t) (utf-16be nil t))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café"))
  (mapcar (lambda (cs)
            (let ((str-enc (append (encode-coding-string s cs) nil))
                  (str-dec (decode-coding-string (encode-coding-string s cs) cs))
                  (reg-enc (with-temp-buffer
                             (insert s) (encode-coding-region 1 (point-max) cs)
                             (append (buffer-string) nil))))
              (list cs (equal str-enc reg-enc) (equal s str-dec))))
          '(utf-8 latin-1 utf-16be)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_json_xml_dom_nested_complex_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 (\"café\" \"世界\") \"1\" ((items . [((id . \"1\") (text . \"café\"))])))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom) (require 'xml)
      (let* ((xml-dom (with-temp-buffer
                        (insert "<root><list><item id=\"1\">café</item><item id=\"2\">世界</item></list></root>")
                        (car (xml-parse-region (point-min) (point-max)))))
             (json-str "{\"xml\":{\"items\":[{\"id\":\"1\",\"text\":\"café\"}]}}")
             (json-data (json-read-from-string json-str)))
        (list (length (dom-by-tag xml-dom 'item))
              (mapcar #'dom-text (dom-by-tag xml-dom 'item))
              (dom-attr (car (dom-by-tag xml-dom 'item)) 'id)
              (cdr (assoc 'xml json-data)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_timer_run_cancel_process_exit_coding_env_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired)
  (let ((tt (run-with-timer 0 nil (lambda () (setq timer-fired :fired)))))
    (let ((env-val
           (let ((process-environment (cons "NEO_CX46=env" process-environment)))
             (string-trim (shell-command-to-string "echo $NEO_CX46"))))
          (exit-code
           (let ((p (make-process :name "neo-cx46-te" :command '("sh" "-c" "exit 7")))
             (accept-process-output p 2)
             (process-exit-status p)))
          (coding-out
           (with-temp-buffer
             (let ((coding-system-for-read 'utf-8-unix))
               (call-process "printf" nil t nil "café"))
             (buffer-string))))
      (cancel-timer tt)
      (list timer-fired env-val exit-code coding-out))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_print_circle_gensym_quoted_struct_propertized_length_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx46-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t)
        (print-length 5) (print-level 2))
    (let ((r (make-neo-cx46-rec :a 1 :b "café"))
          (gs (gensym))
          (s (propertize "world" 'face 'bold))
          (circ (list 0)))
      (setcdr circ circ)
      (list (prin1-to-string r)
            (prin1-to-string `(',gs ',gs))
            (prin1-to-string `[,s ,s ,circ ,circ]))
            (length (prin1-to-string (list 1 2 3 4 5 6 7 8 9 10)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_window_config_register_buffer_switch_marker_overlay_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-markers)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx46-wc*")))
  (with-current-buffer buf
    (insert "0123456789ABCDEF")
    (let ((m (set-marker (make-marker) 8)) (ov (make-overlay 3 7)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 4 'face 'italic)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 6)
  (let ((cfg (current-window-configuration)))
    (set-register ?c cfg)
    (split-window nil nil 'below)
    (let ((after-split (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list after-split (count-windows)
                   (window-point)
                   (with-current-buffer buf (marker-position (cdar buffer-markers)))
                   (with-current-buffer buf (length (overlays-at 4)))
                   (with-current-buffer buf (text-properties-at 1)))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_subword_upcase_backward_word_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"camelCaseVAR p\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "pre camelCaseVar post")
      (narrow-to-region 5 19)
      (goto-char 18)
      (upcase-word -1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_weak_hash_value_eviction_after_gc_cons_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 :symbol-value)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'value :test 'eq)))
  (puthash 'key1 (cons 1 2) ht)
  (puthash 'key2 (cons 3 4) ht)
  (puthash 'key3 :symbol-value ht)
  (garbage-collect)
  (list (hash-table-count ht)
        (gethash 'key3 ht :gone)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_process_send_string_newline_filter_buffer_textprop_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx46-pf*")))
  (with-current-buffer buf
    (insert "HEADER\n")
    (put-text-property 1 7 'face 'bold))
  (let (filter-data)
    (let ((p (make-process :name "neo-cx46-pf" :command '("cat")
                           :buffer buf :connection-type 'pipe
                           :filter (lambda (proc str)
                                     (with-current-buffer buf (goto-char (point-max)) (insert str))
                                     (push str filter-data)))))
      (process-send-string p "line1\nline2\n")
      (process-send-eof p)
      (accept-process-output p 1)))
  (prog1 (with-current-buffer buf
           (widen)
           (list (buffer-string)
                 (text-properties-at 0)
                 (text-properties-at 7)
                 (count-lines 1 (point-max))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_read_eval_backquote_destructuring_macro_lexical_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\,)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((data '((name . "café") (val . 42) (items . (1 2 3)))))
    (list (eval (car (read-from-string "`(:name ,(cdr (assoc 'name ,data)))")) t)
          (eval (car (read-from-string "`(,@(cdr (assoc 'items ,data)) end)")) t)
          (mapcar (lambda (cell)
                    (eval (car (read-from-string "`(,(car ,cell) . ,(cdr ,cell)))")) t))
                  data))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_overlay_modification_hooks_before_change_functions_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((2 6 #(\"01456789\" 0 2 (face bold)) (face bold)) \"\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (before-log after-log)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "0123456789")
    (put-text-property 1 4 'face 'bold)
    (let ((ov (make-overlay 3 7)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'modification-hooks
                   (list (lambda (o beg end &rest _) (push :ov-mod after-log)))))
    (add-hook 'before-change-functions (lambda (b e) (push :before before-log)) nil t)
    (add-hook 'after-change-functions (lambda (b e l) (push :after after-log)) nil t)
    (goto-char 5) (insert "X")
    (undo-boundary)
    (delete-region 3 6)
    (let ((state (list (length before-log) (length after-log)
                       (buffer-string) (text-properties-at 1))))
      (undo)
      (list state (buffer-string) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_superword_backward_word_then_upcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 \"rest SNAKE_CASE_VAR\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "rest snake_case_var")
      (goto-char 20)
      (backward-word 1)
      (upcase-word 1)
      (list (point) (buffer-string)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_format_escape_all_multibyte_circle_gensym_propertized_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-circle t) (print-gensym t) (print-quoted t)
      (print-escape-newlines t) (print-escape-nonascii t)
      (print-escape-multibyte t) (print-length 4) (print-level 3))
  (let ((s1 (propertize "café\n" 'face 'bold))
        (s2 (propertize "世界" 'mouse-face 'highlight))
        (gs (gensym))
        (circ (vector 0)))
    (aset circ 0 circ)
    (list (length (prin1-to-string s1))
          (length (prin1-to-string s2))
          (string-match "#:" (prin1-to-string gs))
          (string-match "#[0-9]" (prin1-to-string circ))
          (prin1-to-string (list s1 s2 gs circ (vector 1 2 3 4 5 6 7)))))
"##,
        expect,
    );
}

#[test]
fn div_cx46_coding_priority_list_utf8_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-position)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((prio (coding-system-priority-list)))
  (list (length prio) (memq 'utf-8 prio) (cl-position 'utf-8 prio)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_process_kill_buffer_query_exit_flag_off_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx46-qk*")))
  (let ((p (make-process :name "neo-cx46-qk" :command '("sleep" "10") :buffer buf)))
    (accept-process-output p 0.1)
    (set-process-query-on-exit-flag p nil)
    (kill-buffer buf))
  (list (buffer-live-p buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx46_cl_coerce_cycle_with_text_properties_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold))
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string)))
  (list (length s) (length l) (length v) (length s2)
        (text-properties-at 0 s) (text-properties-at 0 s2)
        (equal s s2)))
"##,
        expect,
    );
}
