//! Complex combo batch 43 — more MEGA combos targeting known roots in
//! new interaction patterns + fresh subsystem edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx43_process_env_propagate_call_vs_make() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((process-environment (cons "NEO_CX43=propagated" process-environment)))
  (list (string-trim (shell-command-to-string "echo $NEO_CX43"))
        (let ((p (make-process :name "neo-cx43-env" :command '("sh" "-c" "echo $NEO_CX43"))))
          (accept-process-output p 1)
          (with-current-buffer (process-buffer p) (string-trim (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_set_buffer_multibyte_narrow_then_char_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (narrow-to-region 2 8)
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201))
  (set-buffer-multibyte t)
  (list (point-min) (point-max) (length (buffer-string))
        (char-after (1- (point-max)))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_encode_region_vs_string_latin1_world() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((99 97 102 233 32 32) (99 97 102 4194281 32 32))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界"))
  (list (append (encode-coding-string s 'latin-1) nil)
        (with-temp-buffer
          (insert s) (encode-coding-region 1 (point-max) 'latin-1)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_subword_kill_word_then_undo_then_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "camelCaseVariable rest")
      (let ((m (set-marker (make-marker) 10)))
        (goto-char 1)
        (kill-word 1)
        (let ((after-kill (list (buffer-string) (marker-position m))))
          (undo)
          (list after-kill (buffer-string) (marker-position m)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_weak_hash_key_eviction_after_multiple_gc() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq)))
  (dotimes (i 5) (puthash (cons i nil) i ht))
  (garbage-collect)
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx43_timer_run_zero_delay_fires_immediately() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :immediate""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (run-with-timer 0 nil (lambda () (setq fired :immediate)))
  (sit-for 0.01)
  fired)
"##,
        expect,
    );
}

#[test]
fn div_cx43_overlay_stack_priority_get_char_property_5_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK bold""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((o1 (make-overlay 1 10)) (o2 (make-overlay 1 10))
        (o3 (make-overlay 1 10)) (o4 (make-overlay 1 10))
        (o5 (make-overlay 1 10)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (overlay-put o4 'face 'shadow)
    (overlay-put o5 'face 'highlight)
    (overlay-put o1 'priority 10) (overlay-put o2 'priority 8)
    (overlay-put o3 'priority 6) (overlay-put o4 'priority 4) (overlay-put o5 'priority 2)
    (get-char-property 5 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx43_window_start_end_hscroll_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx43-ws*")))
  (with-current-buffer buf
    (dotimes (i 10) (insert (format "col%-3d" i))))
  (set-window-buffer (selected-window) buf)
  (set-window-hscroll (selected-window) 5)
  (let ((hs (window-hscroll)) (ws (window-start)) (we (window-end)))
    (set-window-hscroll (selected-window) 0)
    (prog1 (list hs ws we (window-hscroll))
      (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
      (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx43_json_read_complex_nested_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((users . [((name . \"café\") (tags . [\"a\" \"b\"])) ((name . \"世界\") (active . t))]) (count . 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (json-read-from-string
       "{\"users\": [{\"name\": \"café\", \"tags\": [\"a\", \"b\"]}, {\"name\": \"世界\", \"active\": true}], \"count\": 2}"))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_coding_system_get_decode_translate_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :decode-translate-table)
      (coding-system-get 'utf-8 :encode-translate-table)
      (coding-system-get 'euc-jp :decode-translate-table)
      (coding-system-get 'euc-jp :encode-translate-table))
"##,
        expect,
    );
}

#[test]
fn div_cx43_cl_defmethod_static_instance_advice_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (cl-no-applicable-method neo-cx43-fn neo-cx43-cls)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx43-cls () ((val :initarg :val)))
  (let (log)
    (cl-defgeneric neo-cx43-fn (obj))
    (cl-defmethod neo-cx43-fn :static ((class neo-cx43-cls))
      (push :static log) :cls)
    (cl-defmethod neo-cx43-fn :around ((obj neo-cx43-cls))
      (push :around log) (cl-call-next-method))
    (cl-defmethod neo-cx43-fn ((obj neo-cx43-cls))
      (push :primary log) (oref obj val))
    (advice-add 'neo-cx43-fn :before (lambda (&rest _) (push :advice log)))
    (list (neo-cx43-fn 'neo-cx43-cls)
          (neo-cx43-fn (neo-cx43-cls :val 99))
          (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_print_circle_shared_propertized_string_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold 'mouse-face 'highlight))
       (print-circle t))
  (let ((p (prin1-to-string (vector s s s))))
    (list (string-match "#1=" p)
          (> (length p) 20))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_process_output_multibyte_narrowed_buffer_coding_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"HEAcafé世界DER\\n\" 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx43-pm*")))
  (with-current-buffer buf
    (insert "HEADER\n")
    (narrow-to-region 1 4))
  (let ((p (make-process :name "neo-cx43-pm" :command '("printf" "%s" "café世界")
                         :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (widen)
           (list (buffer-string) (string-bytes (buffer-string))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx43_undo_marker_overlay_textprop_display_evaporate_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t 4 6 (face bold)) t nil nil 1 nil 0 \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 9 'display "XX")
  (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 8)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (delete-region 4 14)
    (let ((state (list (overlayp ov) (marker-position m)
                       (current-column) (text-properties-at 1))))
      (undo)
      (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)
            (current-column) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_fill_paragraph_narrow_overlay_textprop_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"AAAABBBBCCCC\\nlong word h\" 0 3 (face bold)) (face bold) nil 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 15))
    (insert "AAAABBBBCCCC long word here that wraps\n")
    (put-text-property 1 4 'face 'bold)
    (let ((ov (make-overlay 5 8))) (overlay-put ov 'face 'italic))
    (narrow-to-region 1 25)
    (fill-paragraph)
    (list (buffer-string) (text-properties-at 1)
          (get-char-property 4 'face) (current-column))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_read_from_string_error_recovery_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 (end))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((input "(valid) (also-valid) 42 end"))
  (let (forms pos)
    (condition-case nil
        (while t
          (let ((r (read-from-string input (or pos 0))))
            (push (cons (car r) (cdr r)) forms)
            (setq pos (cdr r))))
      (end-of-file))
    (list (length (nreverse forms))
          (mapcar #'car (nreverse forms)))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_superword_backward_kill_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"rest \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "rest snake_case_var")
      (goto-char 20)
      (backward-kill-word 1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_subword_mark_word_forward_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseVariable rest")
      (goto-char 1)
      (mark-word 3)
      (list (region-beginning) (region-end)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx43_hash_table_rehash_stress_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (50 625 2401 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal :size 4 :rehash-size 2.0)))
  (dotimes (i 50) (puthash (number-to-string i) (* i i) ht))
  (list (hash-table-count ht)
        (gethash "25" ht)
        (gethash "49" ht)
        (> (hash-table-size ht) 10)))
"##,
        expect,
    );
}
