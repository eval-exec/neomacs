//! Complex combo batch 44 — continued MEGA combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx44_subword_transpose_words_then_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . user-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (buffer-enable-undo)
      (subword-mode 1)
      (insert "camelCase firstWord")
      (goto-char 1)
      (transpose-words 1)
      (let ((after (buffer-string)))
        (undo)
        (list after (buffer-string))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_process_exit_code_sentinel_buffer_content_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx44-pe*")) (se nil))
  (let ((p (make-process :name "neo-cx44-pe" :command '("sh" "-c" "echo done; exit 9")
                         :buffer buf :sentinel (lambda (proc ev) (setq se ev))))
    (accept-process-output p 2))
  (list (with-current-buffer buf (string-trim (buffer-string)))
        se
        (if se (string-match "code 9" se) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx44_overlay_evaporate_undo_display_marker_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF0123456789")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 8 12 'display "XX")
  (let ((ov (make-overlay 5 10)) (m (set-marker (make-marker) 7)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 24)
    (undo-boundary)
    (delete-region 4 16)
    (let ((state (list (overlayp ov) (overlay-start ov)
                       (marker-position m) (text-properties-at 1) (current-column))))
      (undo)
      (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)
            (text-properties-at 7) (current-column))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_set_buffer_multibyte_overlay_text_prop_sticky_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"café世界hello\" 0 2 (face bold) 3 8 (face italic)) (face bold) 3) #(\"café世界hello\" 0 2 (face bold) 3 5 (face italic)) (face bold) nil 3)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界hello")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'face 'italic)
  (let ((ov (make-overlay 3 5)))
    (overlay-put ov 'face 'underline)
    (set-buffer-multibyte nil)
    (let ((nil-state (list (buffer-string) (text-properties-at 1)
                           (overlay-start ov))))
      (set-buffer-multibyte t)
      (list nil-state (buffer-string) (text-properties-at 1)
            (text-properties-at 3) (overlay-start ov)))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_json_xml_dom_filter_chain_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 \"div\" \"y\" (\"div\" \"p\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json) (require 'dom)
      (let* ((json-str "[{\"tag\":\"div\",\"class\":\"x\"},{\"tag\":\"p\",\"class\":\"y\"}]")
             (data (json-read-from-string json-str)))
        (list (length data)
              (cdr (assoc 'tag (aref data 0)))
              (cdr (assoc 'class (aref data 1)))
              (mapcar (lambda (item) (cdr (assoc 'tag item))) data))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_coding_encode_decode_region_no_conversion_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\0\u{1}A���\" \"\\0\u{1}A���\" \"\\0\u{1}A���\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 0 1 65 128 200 255)))
  (list (with-temp-buffer
          (set-buffer-multibyte nil) (insert raw)
          (encode-coding-region 1 (point-max) 'no-conversion)
          (buffer-string))
        (with-temp-buffer
          (set-buffer-multibyte nil) (insert raw)
          (decode-coding-region 1 (point-max) 'no-conversion)
          (buffer-string))
        (decode-coding-string raw 'no-conversion)))
"##,
        expect,
    );
}

#[test]
fn div_cx44_window_config_save_restore_split_marker_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx44-wc*")))
  (with-current-buffer buf
    (insert "0123456789")
    (set-marker (make-marker) 5))
  (set-window-buffer (selected-window) buf)
  (let ((cfg (current-window-configuration)))
    (split-window nil nil 'right)
    (let ((after-split (count-windows)))
      (set-window-configuration cfg)
      (prog1 (list after-split (count-windows)
                   (with-current-buffer buf (marker-position (cdar buffer-markers))))
        (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
        (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_cl_loop_hash_destructuring_for_in_collect_max_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 10 ht) (puthash "b" 20 ht) (puthash "c" 30 ht) (puthash "d" 40 ht)
  (list (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                       collect (cons k v))
              (lambda (x y) (string< (car x) (car y))))
        (cl-loop for v being the hash-values of ht maximize v)
        (cl-loop for v being the hash-values of ht sum v)
        (cl-loop for v being the hash-values of ht count (> v 15))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_process_send_region_filter_concat_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (received)
  (with-temp-buffer
    (insert "deterministic content for hashing\n")
    (let ((p (make-process :name "neo-cx44-sr" :command '("cat")
                           :buffer nil :connection-type 'pipe
                           :filter (lambda (proc str) (push str received))))
      (process-send-region p (point-min) (point-max))
      (process-send-eof p)
      (accept-process-output p 1)))
  (secure-hash 'sha256 (apply #'concat (nreverse received))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_print_circle_gensym_propertized_quoted_struct_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx44-rec a b)
  (let ((print-circle t) (print-gensym t) (print-quoted t))
    (let ((r (make-neo-cx44-rec :a 1 :b "café"))
          (gs (gensym))
          (circ (list 1)))
      (setcdr circ circ)
      (list (prin1-to-string r)
            (string-match "#s" (prin1-to-string r))
            (prin1-to-string `(',gs ',gs))
            (prin1-to-string circ)
            (prin1-to-string `(,circ ,circ))))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_timer_idle_cancel_process_env_exitcode_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (timer-fired idle-fired)
  (let ((t1 (run-with-timer 0 nil (lambda () (setq timer-fired :t))))
        (t2 (run-with-idle-timer 0 nil (lambda () (setq idle-fired :i)))))
    (sit-for 0.05)
    (cancel-timer t1)
    (cancel-timer t2))
  (let ((env-result
         (let ((process-environment (cons "NEO_CX44=env" process-environment)))
           (string-trim (shell-command-to-string "echo $NEO_CX44")))
        (exit-code
         (let ((p (make-process :name "neo-cx44-te" :command '("sh" "-c" "exit 11")))
           (accept-process-output p 2)
           (process-exit-status p))))
    (list timer-fired idle-fired env-result exit-code)))
"##,
        expect,
    );
}

#[test]
fn div_cx44_display_property_slice_image_align_column_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (16 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 1 3 'display '(space :align-to 10))
  (put-text-property 3 5 'display '(space :relative-width 4))
  (list (current-column) (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_abbrev_expansion_undo_count_hook_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tbl (make-abbrev-table)))
  (define-abbrev tbl "neox" "expanded" nil)
  (let (hook-fired)
    (with-temp-buffer
      (buffer-enable-undo)
      (set (make-local-variable 'local-abbrev-table) tbl)
      (abbrev-mode 1)
      (add-hook 'abbrev-expand-functions (lambda (a) (setq hook-fired :hook)) nil t)
      (insert "neox ") (expand-abbrev)
      (let ((after-exp (buffer-string)))
        (undo)
        (list after-exp (buffer-string) hook-fired)))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_read_eval_backquote_macro_multibyte_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable i)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t) (items '("café" "世界" "test")) (i 0))
  (list (mapcar (lambda (item)
                  (eval (car (read-from-string "`(:idx ,i :val ,item)")) t))
                items)
        (eval (car (read-from-string "`(1 ,@items 2)")) t)))
"##,
        expect,
    );
}

#[test]
fn div_cx44_coding_priority_prefer_utf16_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-16 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (car (coding-system-priority-list))))
  (prefer-coding-system 'utf-16)
  (let ((after (car (coding-system-priority-list))))
    (prefer-coding-system 'utf-8)
    (list before after (car (coding-system-priority-list)))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_overlay_invisible_narrow_buffer_substring_filter_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "vis1 INVIS vis2 INVIS vis3 INVIS vis4\n")
      (put-text-property 6 12 'invisible t)
      (put-text-property 18 24 'invisible t)
      (put-text-property 30 36 'invisible t)
      (narrow-to-region 1 40)
      (let ((full (buffer-string)))
        (widen)
        (list full (length full)
              (count-lines (point-min) (point-max)))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_superword_upcase_backward_word() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"snake_case_var CAMELCASE\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var camelCase")
      (goto-char 25)
      (upcase-word -1)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx44_weak_hash_key_eviction_marker_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 0""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :weakness 'key :test 'eq)))
  (with-temp-buffer
    (insert "x")
    (let ((m (set-marker (make-marker) 1)))
      (puthash m :marker-val ht)))
  (garbage-collect)
  (hash-table-count ht))
"##,
        expect,
    );
}

#[test]
fn div_cx44_format_escape_all_print_flags_multibyte_circle_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\"caf\\\\x00e9\\\\n\\\\x4e16\\\\x754c\\\"\" \"[1 2 3 ...]\" \"('#:g0)\" \"(#1=(0 . #1#) #1#)\" 11)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-circle t) (print-gensym t) (print-quoted t)
      (print-escape-newlines t) (print-escape-nonascii t)
      (print-escape-multibyte t) (print-length 3) (print-level 2))
  (let ((s "café\n世界")
        (v (vector 1 2 3 4 5 6 7 8 9 10))
        (gs (gensym))
        (circ (list 0)))
    (setcdr circ circ)
    (list (prin1-to-string s)
          (prin1-to-string v)
          (prin1-to-string `(',gs))
          (prin1-to-string (list circ circ))
          (length (prin1-to-string v)))))
"##,
        expect,
    );
}
