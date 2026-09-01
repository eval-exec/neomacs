//! Complex combo batch 26 — read malformed input, format %s complex objects,
//! cl-coerce edges, window-resize effect, syntax-pp after mod, hash-table
//! custom test prin1, seq/map extensions, process thread combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx26_read_malformed_error_handling() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (end-of-file end-of-file end-of-file invalid-read-syntax)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (read-from-string "(") (error (car e)))
      (condition-case e (read-from-string "\"unterminated") (error (car e)))
      (condition-case e (read-from-string "#(") (error (car e)))
      (condition-case e (read-from-string ".") (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_format_s_hash_table_compiled() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#s(hash-table test equal data (a 1))\" 3 \"#[257 \\\\211\\\\300_\\\\207 [2] 3 \\n\\n(fn X)]\" nil)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash 'a 1 ht)
  (let ((cf (byte-compile (lambda (x) (* x 2)))))
    (list (format "%s" ht)
          (string-match "hash-table" (format "%s" ht))
          (format "%s" cf)
          (string-match "#<compiled\\|#<closure\\|lambda" (format "%s" cf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_hash_table_custom_test_prin1() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Invalid hash table test\" equal-including-properties)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht-eq (make-hash-table :test 'eq))
      (ht-equal (make-hash-table :test 'equal))
      (ht-uniq (make-hash-table :test 'equal-including-properties)))
  (list (string-match ":test eq" (prin1-to-string ht-eq))
        (string-match ":test equal" (prin1-to-string ht-equal))
        (string-match "equal-including-properties" (prin1-to-string ht-uniq))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_cl_coerce_edge_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-coerce 65 'char)
      (cl-coerce ?a 'integer)
      (cl-coerce '(1 2 3) 'vector)
      (cl-coerce [1 2 3] 'list)
      (cl-coerce "abc" 'list)
      (cl-coerce '(97 98 99) 'string)
      (condition-case e (cl-coerce "abc" 'integer) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_window_resize_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((orig-h (window-total-height)))
      (window-resize (selected-window) -2)
      (let ((smaller (window-total-height)))
        (window-resize (selected-window) 2)
        (list orig-h smaller (window-total-height)
              (>= (window-total-height) orig-h))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_syntax_pp_after_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo () \"str\")")
  (goto-char 15)
  (let ((pp1 (syntax-ppss)))
    (insert "X")
    (let ((pp2 (syntax-ppss)))
      (list (nth 0 pp1) (nth 0 pp2)
            (nth 3 pp1) (nth 3 pp2)
            (nth 8 pp1) (nth 8 pp2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_seq_reduce_max_extensions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'seq)
  (list (seq-reduce #'+ '(1 2 3 4) 0)
        (seq-max '(3 1 4 1 5 9))
        (seq-min '(3 1 4 1 5 9))
        (seq-find #'cl-evenp '(1 3 4 5))
        (seq-count #'cl-oddp '(1 2 3 4 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_process_thread_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (exit t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx26-pt" :command '("echo" "x")))
      (t1 (make-thread (lambda () (sleep-for 0.01)))))
  ;; Drain the process to completion (no-op the incidental sentinel) so the
  ;; status read does not race the child's exit on either engine.
  (set-process-sentinel p #'ignore)
  (while (process-live-p p) (accept-process-output p 1))
  (while (accept-process-output p 0))
  (let ((p-status (process-status p)))
    (thread-join t1)
    (list p-status (eq (process-status p) 'exit)
          (null (thread-live-p t1))
          (threadp t1))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_condition_case_no_debug() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function condition-case-no-debug)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((debug-on-error t))
  (list (condition-case-no-debug
            (error "boom")
          (error :caught-no-debug))
        (condition-case
            (error "boom")
          (error :caught-normal))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_decode_encode_string_no_conversion_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\0\u{1}\u{7f}���\" \"\\0\u{1}\u{7f}���\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 0 1 127 128 200 255)))
  (list (decode-coding-string raw 'no-conversion)
        (encode-coding-string (decode-coding-string raw 'no-conversion) 'no-conversion)
        (equal raw (encode-coding-string (decode-coding-string raw 'no-conversion) 'no-conversion))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_overlay_after_string_then_buffer_substring_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((face bold mouse-face highlight) (face bold mouse-face highlight) \"bcd\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'after-string (propertize "XY" 'face 'bold 'mouse-face 'highlight)))
  (let* ((ov (car (overlays-at 2)))
         (as (overlay-get ov 'after-string))
         (sub (buffer-substring 2 5)))
    (list (text-properties-at 0 as)
          (text-properties-at 1 as)
          sub
          (text-properties-at 0 sub))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_cl_setf_on_aref_vector_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-rotatef)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((v (vector 1 2 3 4 5)))
  (setf (aref v 2) 99)
  (cl-rotatef (aref v 0) (aref v 4))
  (cl-shiftf (aref v 1) (aref v 3) 0)
  v)
"##,
        expect,
    );
}

#[test]
fn div_cx26_char_table_default_value_access() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Invalid RANGE argument to ‘char-table-range’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx26 :default-val)))
  (list (char-table-range ct ?a)
        (char-table-range ct ?z)
        (char-table-range ct t)
        (progn (set-char-table-default-slot ct :new-default)
               (char-table-range ct ?a))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_buffer_undo_list_format_overlay_entry() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello")
  (let ((before (copy-sequence buffer-undo-list)))
    (let ((ov (make-overlay 1 3)))
      (overlay-put ov 'face 'bold))
    (undo-boundary)
    (delete-region 1 2)
    (list (length buffer-undo-list)
          (> (length buffer-undo-list) (length before))
          (consp (car buffer-undo-list)))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_format_escape_combined_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"\\\"caf\\\\x00e9\\\\n\t\\\\x4e16\\\\x754c\\\"\" 26 \"#1=(1 . #1#)\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t)
      (print-escape-nonascii t)
      (print-escape-multibyte t)
      (print-circle t))
  (list (prin1-to-string "café\n\t世界")
        (length (prin1-to-string "café\n\t世界"))
        (let ((x (list 1))) (setcdr x x) (prin1-to-string x))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_process_environment_set_then_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"test-value\" nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig-env (copy-sequence process-environment)))
  (setenv "NEO_CX26_ENV" "test-value")
  (let ((direct (getenv "NEO_CX26_ENV")))
    (setq process-environment orig-env)
    (list direct (getenv "NEO_CX26_ENV"))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_marker_buffer_after_kill_buffer_marker_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil #<killed buffer> 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf1 (get-buffer-create " *neo-cx26-m1*"))
      (buf2 (get-buffer-create " *neo-cx26-m2*")))
  (let ((m1 (set-marker (make-marker) 3 buf1))
        (m2 (set-marker (make-marker) 5 buf2)))
    (kill-buffer buf1)
    (prog1 (list (marker-buffer m1) (marker-position m1)
                 (marker-buffer m2) (marker-position m2))
      (kill-buffer buf2))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_string_bytes_of_format_c_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"aéあ\" 3 6 (97 233 12354) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s1 (format "%c%c%c" ?a ?é #x3042)))
  (list s1 (length s1) (string-bytes s1)
        (append s1 nil)
        (multibyte-string-p s1)))
"##,
        expect,
    );
}

#[test]
fn div_cx26_overlay_evaporate_after_delete_undo_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'bold)
    (undo-boundary)
    (delete-region 3 6)
    (let ((after (list (overlayp ov) (overlay-start ov))))
      (undo)
      (list after (overlayp ov) (overlay-start ov) (overlay-end ov)
            (get-char-property 3 'face)))))
"##,
        expect,
    );
}

#[test]
fn div_cx26_coding_system_mime_charset_broad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-16 utf-16be utf-16le iso-8859-1 iso-8859-15 iso-8859-7 big5 gb2312 shift_jis euc-jp nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs) (coding-system-get cs :mime-charset))
        '(utf-8 utf-16 utf-16be utf-16le latin-1 iso-8859-15
          iso-8859-7 big5 gb2312 shift_jis euc-jp emacs-mule))
"##,
        expect,
    );
}
