//! Complex combo batch 49 — continued fresh edges + MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx49_hi_lock_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world foo bar\n")
      (hi-lock-mode 1)
      (highlight-regexp "foo" 'hi-yellow)
      (prog1 (get-text-property 13 'font-lock-face)
        (hi-lock-mode -1)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_char_fold_search_default_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 101)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((search-default-mode 'char-fold-to-regexp))
  (list (string-match (char-fold-to-regexp ?e) "café")
        (string-match (char-fold-to-regexp ?a) "abc")
        (string-match (char-fold-to-regexp ?é) "cafe")))
"##,
        expect,
    );
}

#[test]
fn div_cx49_electric_layout_mode_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"()\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (electric-layout-mode 1)
      (insert "()")
      (goto-char 2)
      (let ((last-command-event ?\n))
        (electric-layout-post-self-insert-function))
      (buffer-string))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx49_subword_transpose_then_kill_then_undo_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "camelCase firstWord lastWord")
      (goto-char 1)
      (transpose-words 1)
      (let ((after-transpose (buffer-string)))
        (kill-word 1)
        (let ((after-kill (buffer-string)))
          (undo)
          (list after-transpose after-kill (buffer-string)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_process_output_decode_encode_roundtrip_through_buffer_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx49-rt*")))
  (let ((p (make-process :name "neo-cx49-rt" :command '("printf" "%s" "café世界")
                         :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (with-current-buffer buf
    (let ((content (buffer-string)))
      (encode-coding-region 1 (point-max) 'utf-8)
      (let ((enc-len (length (buffer-string))))
        (decode-coding-region 1 (point-max) 'utf-8)
        (list content enc-len (buffer-string) (equal content (buffer-string))))))
  (kill-buffer buf))
"##,
        expect,
    );
}

#[test]
fn div_cx49_overlay_evaporate_display_narrow_undo_marker_textprop_hook_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (hooks)
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "0123456789ABCDEF0123456789")
    (put-text-property 1 5 'face 'bold)
    (put-text-property 6 10 'display "XX")
    (let ((ov (make-overlay 8 14)) (m (set-marker (make-marker) 11)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (overlay-put ov 'modification-hooks (list (lambda (o b e &rest _) (push :hook hooks))))
      (narrow-to-region 3 24)
      (undo-boundary)
      (delete-region 5 16)
      (let ((state (list (overlayp ov) (marker-position m)
                         (text-properties-at 1) (current-column)
                         (length hooks))))
        (undo)
        (widen)
        (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
              (marker-position m) (text-properties-at 1)
              (text-properties-at 5) (text-properties-at 10)
              (current-column) (buffer-string) (length hooks)))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_cl_defmethod_call_next_method_no_next_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function next-method-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx49-cls () ())
  (let (log)
    (cl-defgeneric neo-cx49-fn (obj))
    (cl-defmethod neo-cx49-fn ((obj neo-cx49-cls))
      (list (next-method-p)
            (condition-case e (cl-call-next-method) (error (car e)))))
    (neo-cx49-fn (neo-cx49-cls))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_json_array_of_arrays_nested_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 3 1 \"世界\" \"😀\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let* ((data [[1 "café"] [2 "世界"] [3 "😀"]])
             (enc (json-encode data))
             (dec (json-read-from-string enc)))
        (list (vectorp dec)
              (length dec)
              (aref (aref dec 0) 0)
              (aref (aref dec 1) 1)
              (aref (aref dec 2) 1))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_dom_manipulation_add_remove_children_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'dom)
      (let ((dom '(div nil (p nil "text1") (p nil "text2"))))
        (dom-add-child dom '(p nil "text3"))
        (list (length (dom-by-tag dom 'p))
              (mapcar #'dom-text (dom-by-tag dom 'p))
              (dom-children dom))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_window_resize_hscroll_vscroll_dedicated_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((buf (get-buffer-create " *neo-cx49-wr*")))
      (with-current-buffer buf (dotimes (i 10) (insert (format "line%d\n" i))))
      (set-window-buffer (selected-window) buf)
      (set-window-hscroll (selected-window) 3)
      (set-window-vscroll (selected-window) 2)
      (set-window-dedicated-p (selected-window) t)
      (window-resize (selected-window) -1)
      (let ((h (window-total-height)) (hs (window-hscroll)) (vs (window-vscroll)) (d (window-dedicated-p)))
        (window-resize (selected-window) 1)
        (prog1 (list h hs vs d (window-total-height))
          (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
          (kill-buffer buf))))
  (error (list :errored)))
"##,
        expect,
    );
}

#[test]
fn div_cx49_coding_priority_list_after_prefer_utf16_check_first() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (car (coding-system-priority-list))))
  (prefer-coding-system 'utf-16)
  (let ((after (car (coding-system-priority-list))))
    (prefer-coding-system 'utf-8)
    (list before after)))
"##,
        expect,
    );
}

#[test]
fn div_cx49_timer_idle_list_after_run_idle_cancel() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3 2 ([nil 0 100 0 nil (closure (t) nil nil) nil idle 0 nil]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t1 (run-with-idle-timer 50 nil (lambda ())))
      (t2 (run-with-idle-timer 100 nil (lambda ()))))
  (let ((before (length timer-idle-list)))
    (cancel-timer t1)
    (list before (length timer-idle-list)
          (member t2 timer-idle-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_undo_after_format_replace_multibyte_marker_overlay_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((\"CAFÉ 世界 HELLO WORLD\\n\" 8 6 nil) \"\" 1 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café 世界 hello world\n")
  (put-text-property 1 3 'face 'bold)
  (let ((ov (make-overlay 6 10)) (m (set-marker (make-marker) 8)))
    (overlay-put ov 'face 'italic)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "[a-zé]+" nil t)
      (replace-match (upcase (match-string 0))))
    (let ((state (list (buffer-string) (marker-position m)
                       (overlay-start ov) (text-properties-at 1))))
      (undo)
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_print_read_circle_shared_hash_vector_struct_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx49-rec val)
  (let ((print-circle t) (print-gensym t))
    (let* ((v (vector 1 2 3))
           (r (make-neo-cx49-rec :val v))
           (p (prin1-to-string (list v v r r)))
           (back (car (read-from-string p))))
      (list (eq (car back) (cadr back))
            (eq (nth 2 back) (nth 3 back))
            (aref (car back) 0)
            (neo-cx49-rec-val (nth 2 back))))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_superword_downcase_backward_then_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"snake_case_var REST\" 20)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "SNAKE_CASE_VAR REST")
      (goto-char 15)
      (downcase-word -1)
      (let ((after-down (buffer-string)))
        (forward-word 1)
        (list after-down (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_set_buffer_multibyte_then_char_syntax_high_codepoint() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 32 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((mb (char-syntax #x4e16)))
    (set-buffer-multibyte nil)
    (let ((ub (char-syntax #x4e16)))
      (set-buffer-multibyte t)
      (list mb ub (char-syntax #x4e16)))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_process_send_region_filter_hash_coding_env_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (received env-val)
  (with-temp-buffer
    (insert "café世界 data for round trip")
    (let ((p (make-process :name "neo-cx49-sr" :command '("cat")
                           :buffer nil :connection-type 'pipe
                           :filter (lambda (proc str) (push str received))))
      (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
      (process-send-region p (point-min) (point-max))
      (process-send-eof p)
      (accept-process-output p 1)))
  (setq env-val
        (let ((process-environment (cons "NEO_CX49=prop" process-environment)))
          (string-trim (shell-command-to-string "echo $NEO_CX49"))))
  (list env-val (secure-hash 'sha256 (apply #'concat (nreverse received)))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_weak_hash_value_eviction_marker_as_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'value :test 'eq)))
  (with-temp-buffer
    (insert "x")
    (let ((m (point-marker)))
      (puthash 'key m ht)))
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx49_read_delimited_list_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(a b c) (d e) (f)")
      (goto-char 1)
      (let ((lst (read-delimited-list (string ?\)))
            lst))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx49_coding_encode_coding_string_then_decode_compare_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((99 97 102 195 169) (99 97 102 233) (ascii ascii ascii unicode-bmp) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café")
       (enc (encode-coding-string s 'utf-8))
       (dec (decode-coding-string enc 'utf-8)))
  (list (append enc nil)
        (append dec nil)
        (mapcar #'char-charset (append dec nil))
        (equal s dec)))
"##,
        expect,
    );
}
