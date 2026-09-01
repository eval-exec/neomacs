//! Complex combo batch 41 — extend weak-hash eviction, more MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx41_weak_hash_value_eviction_after_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'value :test 'eq)))
  (let ((obj (cons 1 2)))
    (puthash 'key obj ht))
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx41_weak_hash_key_and_value_eviction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key-and-value :test 'eq)))
  (dotimes (i 3)
    (let ((k (cons i nil)) (v (cons (* i 10) nil)))
      (puthash k v ht)))
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx41_weak_hash_key_eviction_string_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq)))
  (puthash (string ?x) 1 ht)
  (puthash (string ?y) 2 ht)
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx41_weak_hash_non_weak_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((strong (make-hash-table :test 'eq))
      (weak (make-hash-table :weakness 'key :test 'eq)))
  (let ((obj (cons 1 2)))
    (puthash obj :val strong)
    (puthash obj :val weak))
  (garbage-collect)
  (list (hash-table-count strong) (hash-table-count weak)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_subword_backward_word_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 14""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "pre camelCaseVar post")
      (narrow-to-region 5 19)
      (goto-char 18)
      (backward-word 1)
      (point))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_superword_upcase_word_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"SNAKE_CASE_VAR REST\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var rest")
      (goto-char 1)
      (upcase-word 2)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_process_buffer_marker_undo_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-markers)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx41-pm*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "0123456789")
    (let ((m (set-marker (make-marker) 5))
          (ov (make-overlay 2 7)))
      (overlay-put ov 'face 'bold)
      (put-text-property 1 3 'face 'italic)
      (narrow-to-region 2 9)))
  (let ((p (make-process :name "neo-cx41-pm" :command '("echo" "out")
                         :buffer buf)))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (list (buffer-string) (point-min) (point-max)
                 (marker-position (cdar buffer-markers))
                 (length (overlays-in 1 10))
                 (text-properties-at 1)))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_coding_encode_decode_string_region_consistency_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界😀"))
  (list (equal (encode-coding-string s 'utf-8)
               (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-8) (buffer-string)))
        (equal (encode-coding-string s 'latin-1)
               (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'latin-1) (buffer-string)))
        (equal (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8) s)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_display_table_char_width_string_width_column_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((dt (make-display-table)))
    (aset dt ?a (vector ?X ?Y))
    (setq buffer-display-table dt))
  (list (current-column)
        (string-width "café")
        (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_cl_defmethod_combination_list_plus_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx41-fn: (list)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx41-cls () ())
  (let (log)
    (cl-defgeneric neo-cx41-fn (obj) (:method-combination list))
    (cl-defmethod neo-cx41-fn list ((obj neo-cx41-cls)) :a)
    (cl-defmethod neo-cx41-fn list ((obj neo-cx41-cls)) :b)
    (cl-defmethod neo-cx41-fn list ((obj neo-cx41-cls)) :c)
    (neo-cx41-fn (neo-cx41-cls))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_overlay_priority_invisible_textprop_fontlock_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (font-lock-keyword-face font-lock-keyword-face font-lock-keyword-face t highlight t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(defun foo (x) body)")
    (font-lock-fontify-buffer)
    (put-text-property 3 7 'invisible t)
    (let ((ov (make-overlay 5 10)))
      (overlay-put ov 'face 'highlight)
      (overlay-put ov 'priority 10))
    (list (get-text-property 2 'face)
          (get-char-property 2 'face)
          (get-char-property 3 'face)
          (get-char-property 3 'invisible)
          (get-char-property 6 'face)
          (get-char-property 6 'invisible))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_set_multibyte_undo_marker_overlay_textprop_display_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((16 6 4 (face bold)) 11 5 4 (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café世界hello")
  (put-text-property 1 3 'face 'bold)
  (let ((ov (make-overlay 4 7)) (m (set-marker (make-marker) 5)))
    (overlay-put ov 'face 'italic)
    (undo-boundary)
    (set-buffer-multibyte nil)
    (let ((nil-state (list (length (buffer-string)) (marker-position m)
                           (overlay-start ov) (text-properties-at 1))))
      (set-buffer-multibyte t)
      (list nil-state (length (buffer-string)) (marker-position m)
            (overlay-start ov) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_json_xml_dom_parse_compare_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" [1 2] 33)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom)
      (let* ((json-str "{\"name\": \"café\", \"items\": [1, 2]}")
             (json-data (json-read-from-string json-str)))
        (list (cdr (assoc 'name json-data))
              (cdr (assoc 'items json-data))
              (length json-str))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_timer_cancel_process_exit_sentinel_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:fired 23 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired sentinel-ev)
  (let ((tt (run-with-timer 0 nil (lambda () (setq timer-fired :fired)))))
    (let ((p (make-process :name "neo-cx41-tps" :command '("sh" "-c" "exit 7")
                           :sentinel (lambda (proc event) (setq sentinel-ev event)))))
      (accept-process-output p 2))
    (cancel-timer tt))
  (list timer-fired
        (if sentinel-ev (string-match "code 7" sentinel-ev) nil)
        (if sentinel-ev (string-match "code 1" sentinel-ev) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_process_send_string_filter_buffer_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (received)
  (let ((p (make-process :name "neo-cx41-sf" :command '("cat")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str received))))
    (process-send-string p "deterministic round trip content")
    (process-send-eof p)
    (accept-process-output p 1))
  (secure-hash 'sha256 (apply #'concat (nreverse received))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_format_escape_circle_gensym_quoted_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"(#1=#:g0 #1#)\" \"(#1=(1 . #1#) #1#)\" \"(lambda (x) \\\"café\\\\n\\\" x)\" 13)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-circle t) (print-gensym t) (print-quoted t)
      (print-escape-newlines t) (print-escape-nonascii t))
  (let ((gs (gensym))
        (circ (list 1)))
    (setcdr circ circ)
    (list (prin1-to-string `(,gs ,gs))
          (prin1-to-string `(,circ ,circ))
          (prin1-to-string '(lambda (x) "café\n" x))
          (length (prin1-to-string `(,gs ,gs))))))
"##,
        expect,
    );
}

#[test]
fn div_cx41_window_config_register_marker_overlay_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable buffer-markers)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx41-wc*")))
  (with-current-buffer buf
    (insert "0123456789ABCDEF")
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 3 7)))
      (overlay-put ov 'face 'bold)
      (narrow-to-region 2 14)))
  (set-window-buffer (selected-window) buf)
  (let ((cfg (current-window-configuration)))
    (set-register ?c cfg)
    (split-window)
    (set-window-configuration cfg))
  (prog1 (with-current-buffer buf
           (list (point-min) (point-max)
                 (marker-position (cdar buffer-markers))
                 (length (overlays-at 4))))
    (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_read_from_string_multiple_forms_position_tracking_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 (:keyword) 38)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input "(a) 42 \"str\" [vec] ?x (b . c) :keyword")
       (pos 0)
       (forms nil))
  (while (< pos (length input))
    (let ((r (read-from-string input pos)))
      (push (cons (car r) (cdr r)) forms)
      (setq pos (cdr r))))
  (list (length (nreverse forms))
        (mapcar #'car (nreverse forms))
        pos))
"##,
        expect,
    );
}

#[test]
fn div_cx41_cl_loop_hash_keys_values_sum_filter_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 20) (puthash (number-to-string i) (* i i) ht))
  (list (cl-loop for v being the hash-values of ht when (> v 100) count t)
        (cl-loop for v being the hash-values of ht when (> v 100) sum v)
        (cl-loop for k being the hash-keys of ht when (cl-evenp (string-to-number k))
                 count t)))
"##,
        expect,
    );
}

#[test]
fn div_cx41_undo_redo_marker_overlay_textprop_narrow_widen_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (((9 11 6) (5 5 5) ((font-lock-face nil face bold))) #(\"0123456X789ABCDEF0123456789\" 0 3 (face bold) 3 4 (font-lock-face nil face bold) 11 13 (font-lock-face nil) 13 14 (font-lock-face nil mouse-face highlight) 14 18 (mouse-face highlight)) 11 5 13 (face bold) (font-lock-face nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 10)))
    (overlay-put ov 'face 'italic)
    (put-text-property 13 18 'mouse-face 'highlight)
    (narrow-to-region 3 24)
    (let (states)
      (undo-boundary)
      (goto-char 8) (insert "X") (push (list (point) (marker-position m) (overlay-start ov)) states)
      (undo-boundary)
      (delete-region 5 12) (push (list (point) (marker-position m) (overlay-start ov)) states)
      (undo-boundary)
      (put-text-property 4 8 'font-lock-face 'keyword) (push (list (text-properties-at 4)) states)
      (undo) (undo) (undo)
      (widen)
      (list (nreverse states) (buffer-string) (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (text-properties-at 1) (text-properties-at 12)))))
"##,
        expect,
    );
}
