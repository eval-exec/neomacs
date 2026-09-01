//! Complex combo batch 45 — continued MEGA combos targeting new interaction patterns.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx45_process_filter_partial_chunk_reassembly_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (24 \"9dde3598e8ad5bf6069b10f5bb121a5e\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (chunks)
  (let ((p (make-process :name "neo-cx45-ch" :command '("printf" "%s" "chunk1-chunk2-chunk3-end")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str chunks)))))
    (dotimes (_ 5) (accept-process-output p 0.1)))
  (let ((full (apply #'concat (nreverse chunks))))
    (list (length full) (secure-hash 'md5 full))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_overlay_before_after_string_face_display_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 15 \"123Z456789ABC\" nil nil 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 4 8)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'before-string (propertize ">>" 'face 'italic 'display "XX"))
    (overlay-put ov 'after-string (propertize "<<" 'face 'underline 'display "YY")))
  (narrow-to-region 2 14)
  (goto-char 5) (insert "Z")
  (list (point-min) (point-max) (buffer-string)
        (get-char-property 3 'face)
        (get-char-property 3 'display)
        (length (overlays-in (point-min) (point-max)))
        (current-column)))
"##,
        expect,
    );
}

#[test]
fn div_cx45_cl_defmethod_before_after_around_primary_static_advice_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx45-fn: (:static)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx45-cls () ((val :initarg :val)))
  (let (log)
    (cl-defgeneric neo-cx45-fn (obj))
    (cl-defmethod neo-cx45-fn :static ((class neo-cx45-cls)) (push :static log) :s)
    (cl-defmethod neo-cx45-fn :before ((obj neo-cx45-cls)) (push :before log))
    (cl-defmethod neo-cx45-fn :around ((obj neo-cx45-cls))
      (push :around-in log) (let ((r (cl-call-next-method))) (push :around-out log) r))
    (cl-defmethod neo-cx45-fn :after ((obj neo-cx45-cls)) (push :after log))
    (cl-defmethod neo-cx45-fn ((obj neo-cx45-cls)) (push :primary log) (oref obj val))
    (advice-add 'neo-cx45-fn :before (lambda (&rest _) (push :advice log)))
    (list (neo-cx45-fn (neo-cx45-cls :val 42))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_undo_redo_text_prop_marker_overlay_evaporate_narrow_widen_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'mouse-face 'highlight)
  (let ((ov (make-overlay 11 16)) (m (set-marker (make-marker) 14)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 29)
    (undo-boundary)
    (goto-char 10) (insert "XYZ")
    (undo-boundary)
    (delete-region 5 20)
    (undo-boundary)
    (set-text-properties 4 10 '(face underline))
    (let ((state (list (point-min) (point-max) (marker-position m)
                       (overlayp ov) (overlay-start ov)
                       (buffer-string) (text-properties-at 1))))
      (undo)
      (undo)
      (undo)
      (widen)
      (list state (buffer-string) (marker-position m)
            (overlayp ov) (overlay-start ov) (overlay-end ov)
            (text-properties-at 1) (text-properties-at 5) (text-properties-at 10)))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_json_encode_decode_multibyte_hash_roundtrip_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café世界\" \"😀\" [1 \"é\" t nil] \"03c5f12f56de3f5c1dce2cf8090c9e5622ac74d8d4f84dc5c89723431d59841b\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let* ((data '((name . "café世界") (emoji . "😀") (items . [1 "é" t nil])))
             (enc (json-encode data))
             (dec (json-read-from-string enc)))
        (list (cdr (assoc 'name dec))
              (cdr (assoc 'emoji dec))
              (cdr (assoc 'items dec))
              (secure-hash 'sha256 enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_process_env_coding_narrow_output_filter_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"prop\" \"café世界\" \"PREoutput\\n\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx45-pe*")))
  (with-current-buffer buf (insert "PRE\n") (narrow-to-region 1 4))
  (let ((env-val
         (let ((process-environment (cons "NEO_CX45=prop" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX45"))))
        (coding-out
         (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix))
             (call-process "printf" nil t nil "café世界"))
           (buffer-string))))
    (let ((p (make-process :name "neo-cx45-pe" :command '("echo" "output")
                           :buffer buf :sentinel #'ignore)))
      (set-process-query-on-exit-flag p nil)
      (let ((i 0))
        (while (and (process-live-p p) (< i 20))
          (accept-process-output p 0.05)
          (setq i (1+ i)))))
    (prog1 (list env-val coding-out
                 (with-current-buffer buf (widen) (buffer-string)))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_window_buffer_point_marker_overlay_dedicated_hscroll_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-markers)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx45-wb*")))
  (with-current-buffer buf
    (insert (make-string 50 ?x))
    (let ((m (set-marker (make-marker) 10)) (ov (make-overlay 5 15)))
      (overlay-put ov 'face 'bold)))
  (set-window-buffer (selected-window) buf)
  (set-window-point (selected-window) 20)
  (set-window-hscroll (selected-window) 3)
  (set-window-dedicated-p (selected-window) t)
  (let ((wp (window-point)) (wh (window-hscroll)) (wd (window-dedicated-p)))
    (prog1 (list wp wh wd
                 (with-current-buffer buf (marker-position (cdar buffer-markers)))
                 (with-current-buffer buf (length (overlays-at 8))))
      (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_regex_casefold_multibyte_replace_match_data_save_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"CAFÉ RÉSUMÉ 世界 STRASSE\" ((\"Café\" . \"CAFÉ\") (\"RÉSUMÉ\" . \"CAFÉ\") (\"straße\" . \"CAFÉ\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "Café RÉSUMÉ 世界 straße")
    (goto-char 1)
    (let (results)
      (while (re-search-forward "\\([a-zéàüß]+\\)" nil t)
        (let ((match (match-string 1))
              (md (match-data)))
          (replace-match (upcase match))
          (push (cons match (buffer-substring-no-properties 1 5)) results)
          (set-match-data md)))
      (list (buffer-string) (nreverse results)))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_timer_multiple_cancel_count_idle_list_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 0 2 0 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((timers (list (run-with-timer 0 nil (lambda () (push :t1 fired)))
                      (run-with-timer 0 nil (lambda () (push :t2 fired)))
                      (run-with-idle-timer 0 nil (lambda () (push :i1 fired))))))
    (sit-for 0.05)
    (let ((tl-before (length timer-list))
          (il-before (length timer-idle-list)))
      (mapc #'cancel-timer timers)
      (list (length fired) tl-before il-before
            (length timer-list) (length timer-idle-list)))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_dom_xml_parse_attr_children_text_compare_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"val1\" \"val2\" 2 (\"text1\" \"text2\") root)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom) (require 'xml)
      (let ((dom (with-temp-buffer
                   (insert "<root attr1=\"val1\" attr2=\"val2\"><child>text1</child><child>text2</child></root>")
                   (car (xml-parse-region (point-min) (point-max))))))
        (list (dom-attr dom 'attr1)
              (dom-attr dom 'attr2)
              (length (dom-by-tag dom 'child))
              (mapcar #'dom-text (dom-by-tag dom 'child))
              (dom-tag dom))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_coding_system_change_eol_conversion_chain_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8-dos utf-8-mac utf-8-unix 1 2 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base 'utf-8))
  (list (coding-system-change-eol-conversion base 'dos)
        (coding-system-change-eol-conversion base 'mac)
        (coding-system-change-eol-conversion (coding-system-change-eol-conversion base 'dos) 'unix)
        (coding-system-eol-type 'utf-8-dos)
        (coding-system-eol-type 'utf-8-mac)
        (coding-system-eol-type 'utf-8-unix)))
"##,
        expect,
    );
}

#[test]
fn div_cx45_set_buffer_multibyte_nil_insert_multibyte_char_then_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (12 13 (104 101 108 108 111 99 97 102 4194281 4194248 4194249 65))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (set-buffer-multibyte nil)
  (condition-case e (insert "café") (error (push :err nil)))
  (insert (unibyte-string 200 201 65))
  (set-buffer-multibyte t)
  (list (length (buffer-string)) (point-max) (append (buffer-string) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx45_subword_forward_backward_cycle_point_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 8 12 15 15 15 12 8 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "myCamelCaseVar")
      (goto-char 1)
      (let (positions)
        (dotimes (_ 6)
          (forward-word 1)
          (push (point) positions))
        (dotimes (_ 3)
          (backward-word 1)
          (push (point) positions))
        (nreverse positions)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_hash_table_weak_key_rehash_after_eviction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 1 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq :size 2)))
  (dotimes (i 3) (puthash (cons i nil) i ht))
  (let ((before-gc (hash-table-count ht)))
    (garbage-collect)
    (puthash (cons 99 nil) 99 ht)
    (garbage-collect)
    (list before-gc (hash-table-count ht)
          (hash-table-size ht))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_cl_coerce_list_vector_string_char_cycle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café")
       (l (cl-coerce s 'list))
       (v (cl-coerce l 'vector))
       (s2 (cl-coerce v 'string))
       (c0 (cl-coerce (car l) 'char)))
  (list l v s2 c0
        (equal s s2)
        (append v nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx45_print_read_struct_vector_named_plist_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-cx45-named (:type vector) :named) field1 field2)
  (let ((obj (make-neo-cx45-named :field1 "café" :field2 42))
        (p (prin1-to-string (make-neo-cx45-named :field1 "x" :field2 1))))
    (let ((back (car (read-from-string p))))
      (list (aref back 0) (aref back 1) (aref back 2)
            (neo-cx45-named-p back)
            (neo-cx45-named-field1 back)
            (neo-cx45-named-field2 back)))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_overlay_modification_hooks_insert_delete_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (hooks)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "0123456789")
    (let ((ov (make-overlay 3 7)))
      (overlay-put ov 'face 'bold)
      (overlay-put ov 'modification-hooks
                   (list (lambda (o beg end &rest _) (push :mod hooks))))
      (undo-boundary)
      (goto-char 5) (insert "X")
      (undo-boundary)
      (delete-region 4 8)
      (let ((hook-count (length hooks)))
        (undo)
        (list hook-count (length hooks) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_superword_kill_word_then_forward_word_rest() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\" camelCase rest\" 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var camelCase rest")
      (goto-char 1)
      (kill-word 1)
      (let ((after-kill (buffer-string)))
        (forward-word 1)
        (list after-kill (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx45_coding_decode_string_then_char_width_then_string_width_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99 97 102 233 8364) (1 1 1 1 1) 5 8 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string 99 97 102 195 169 226 130 172))
       (decoded (decode-coding-string raw 'utf-8)))
  (list (append decoded nil)
        (mapcar #'char-width (append decoded nil))
        (string-width decoded)
        (string-bytes decoded)
        (length decoded)))
"##,
        expect,
    );
}
