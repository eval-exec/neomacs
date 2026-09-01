//! Complex combo batch 16 — secure-hash file FIXED probe (delete+rewrite),
//! with-temp-message deeper, process exit code via call-process return,
//! encode-coding-region consistency across codings, char-fold + case-fold
//! combined, overlay before/after-string + narrowing + point-motion,
//! cl-loop with multiple accumulators + conditionals, timer-list after
//! multiple timers, read-delimited, format-spec with multibyte values,
//! buffer-local face-remap + font-lock precedence, process send-string
//! + process-buffer + narrowing.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx16_secure_hash_file_rewritten() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"9ad8e8456982da1b7b45c7149528a1d72477656dfaca2aeb24da1e967ea936f2\" \"9ad8e8456982da1b7b45c7149528a1d72477656dfaca2aeb24da1e967ea936f2\" t \"d2399a270cda9e50b2f467d8b9e4e6b347e31e5ff04cc72e09d6f77c4da0cedf\" \"dfda11df060666bcc681d3c5086b4000b9153dda2f5d7918811e9bb571b88135\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f "/tmp/neo-cx16-sh-fixed"))
  (let ((path1 (secure-hash 'sha256 f))
        (content1 (secure-hash 'sha256 "ascii content")))
    (let ((path2 (secure-hash 'sha256 f))
          (content2 (secure-hash 'sha256 "café世界")))
      (list path1 path2 (equal path1 path2)
            content1 content2 (equal content1 content2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_call_process_return_exit_code() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 7 42 255)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (call-process "sh" nil nil nil "-c" "exit 0")
      (call-process "sh" nil nil nil "-c" "exit 7")
      (call-process "sh" nil nil nil "-c" "exit 42")
      (call-process "sh" nil nil nil "-c" "exit 255"))
"##,
        expect,
    );
}

#[test]
fn div_cx16_encode_coding_region_all_codings_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 4 (65 66) (65 66))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "AB"))
  (list (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-8) (length (buffer-string)))
        (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'latin-1) (length (buffer-string)))
        (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-16be) (length (buffer-string)))
        (append (encode-coding-string s 'utf-8) nil)
        (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-8) (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_char_fold_case_fold_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 101)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match (char-fold-to-regexp ?e) "É")
        (string-match (char-fold-to-regexp ?É) "e")
        (string-match (char-fold-to-regexp ?a) "Á")))
"##,
        expect,
    );
}

#[test]
fn div_cx16_overlay_before_after_narrow_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 14 \"23X456789AB\" 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 5 8)))
    (overlay-put ov 'before-string ">>")
    (overlay-put ov 'after-string "<<")
    (overlay-put ov 'face 'bold))
  (narrow-to-region 3 13)
  (goto-char 5)
  (insert "X")
  (list (point-min) (point-max) (buffer-string)
        (length (overlays-in (point-min) (point-max)))
        (get-char-property 4 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_cl_loop_multi_accumulator() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for i from 1 to 10
         if (cl-evenp i) collect i into evens
         else collect i into odds
         sum i into total
         finally (return (list evens odds total)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_timer_list_after_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t1 (run-with-timer 100 nil (lambda ())))
      (t2 (run-with-timer 200 nil (lambda ())))
      (t3 (run-with-idle-timer 100 nil (lambda ()))))
  (prog1 (list (length timer-list)
               (length timer-idle-list)
               (timerp t1) (timerp t2) (timerp t3))
    (cancel-timer t1) (cancel-timer t2) (cancel-timer t3)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_format_spec_multibyte_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café and 世界\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (format-spec "%a and %b"
                 '((97 . "café") (98 . "世界")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_face_remap_font_lock_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (font-lock-keyword-face bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(defun foo ())")
    (font-lock-fontify-buffer)
    (let ((cookie (face-remap-add-relative 'font-lock-keyword-face :weight 'bold)))
      (prog1 (list (get-text-property 2 'face)
                   (face-attribute 'font-lock-keyword-face :weight))
        (face-remap-remove-relative cookie)))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_process_send_buffer_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"before-middle-after\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "before-middle-after")
  (narrow-to-region 7 13)
  (let ((p (make-process :name "neo-cx16-ps" :command '("cat")
                         :buffer nil :connection-type 'pipe)))
    (process-send-region p (point-min) (point-max))
    (process-send-eof p)
    (accept-process-output p 1))
  (widen)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx16_decode_coding_region_then_aset_grow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abé\" 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 97 98 195 169))
  (decode-coding-region (point-min) (point-max) 'utf-8)
  (aset (buffer-string) 2 ?X)
  (list (buffer-string) (length (buffer-string)) (string-bytes (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_cl_lexical_closure_mutual_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 42 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((get-set nil))
    (let ((val 0))
      (setq get-set (cons (lambda () val)
                          (lambda (new) (setq val new)))))
    (list (funcall (car get-set))
          (funcall (cdr get-set) 42)
          (funcall (car get-set)))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_overlay_priority_mouse_face_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold secondary bold highlight)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((o1 (make-overlay 2 6)) (o2 (make-overlay 4 8)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'mouse-face 'highlight)
    (overlay-put o1 'mouse-face 'secondary)
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 5)
    (list (get-char-property 3 'face)
          (get-char-property 3 'mouse-face)
          (get-char-property 5 'face)
          (get-char-property 5 'mouse-face))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_read_from_string_multiple_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a) (b) (c) \"str\" 42 [vec])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input "(a) (b) (c) \"str\" 42 [vec]")
       (pos 0)
       (forms nil))
  (while (< pos (length input))
    (let ((r (read-from-string input pos)))
      (push (car r) forms)
      (setq pos (cdr r))))
  (nreverse forms))
"##,
        expect,
    );
}

#[test]
fn div_cx16_undo_text_prop_change_does_not_affect_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (6 7 (face nil) #(\"hello world\" 0 4 (face nil)))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (let ((m (set-marker (make-marker) 7)))
    (undo-boundary)
    (put-text-property 1 5 'face 'bold)
    (undo-boundary)
    (delete-region 6 11)
    (let ((after-delete (marker-position m)))
      (undo)
      (list after-delete (marker-position m)
            (text-properties-at 1) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx16_process_kill_then_status() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx16-pk" :command '("sleep" "30"))))
  (accept-process-output p 0.1)
  (delete-process p)
  (accept-process-output p 0.1)
  (list (process-status p) (process-live-p p)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_coding_system_mime_charset_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 iso-8859-1 iso-8859-7 big5 shift_jis)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :mime-charset)
      (coding-system-get 'latin-1 :mime-charset)
      (coding-system-get 'iso-8859-7 :mime-charset)
      (coding-system-get 'big5 :mime-charset)
      (coding-system-get 'shift_jis :mime-charset))
"##,
        expect,
    );
}

#[test]
fn div_cx16_string_lessp_with_raw_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (string-make-multibyte (unibyte-string 200)))
      (b (string-make-multibyte (unibyte-string 201))))
  (list (string-lessp a b)
        (string-lessp b a)
        (string-lessp a a)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_buffer_file_name_coding_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((dir (make-temp-file "neo-cx16-bfn-" t))
       (f (expand-file-name "café.txt" dir)))
  (condition-case e
      (progn
        (write-region "content" nil f nil 0)
        (list (file-exists-p f)
              (directory-files dir nil "^[^.]")))
    (error (cons 'errored (car e))))
  (ignore-errors (delete-directory dir t)))
"##,
        expect,
    );
}

#[test]
fn div_cx16_multiple_coding_systems_decode_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"café\" 0 4 (charset iso-8859-1)) \"caf\\351\" \"caf�\" (99 97 102 233) (99 97 102 233))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 99 97 102 233)))
  (list (decode-coding-string raw 'latin-1)
        (decode-coding-string raw 'utf-8)
        (decode-coding-string raw 'no-conversion)
        (append (decode-coding-string raw 'latin-1) nil)
        (append (decode-coding-string raw 'no-conversion) nil)))
"##,
        expect,
    );
}
