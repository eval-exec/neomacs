//! Complex combo batch 17 — narrow secure-hash (string vs file), process
//! output hash, format-encode/decode, multibyte+char-width+column, triple
//! face precedence, max-char formatting, circular cons read, cl-remf,
//! window-start/end tracking.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx17_secure_hash_string_vs_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"e25dd806d495b413931f4eea50b677a7a5c02d00460924661283f211a37f7e7f\" \"8b7e2d8a46af5ce0067ce16bcd2a055358a95cb47d3d8b3d0011331edeebd6f0\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((content "test content for hashing")
       (str-hash (secure-hash 'sha256 content))
       (f "/tmp/neo-cx17-sh-fixed")
       (path-hash (secure-hash 'sha256 f)))
  (list str-hash path-hash (equal str-hash path-hash)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_process_output_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"68c3a571100a3ad74879be7e8b035d4ad2e5fa9d385a81c4b6e1c5e8c09f9dbb\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (call-process "printf" nil t nil "deterministic output")
  (secure-hash 'sha256 (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_format_encode_decode_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let* ((s "café")
           (enc (format-encode-string s 'utf-8))
           (dec (format-decode-string enc nil 'utf-8)))
      (list enc dec (equal s dec)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_set_multibyte_char_width_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 2 2 \"ab世cd\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab世cd")
  (goto-char 3)
  (let ((col-before (current-column))
        (w (char-width ?\x4e16)))
    (set-buffer-multibyte nil)
    (set-buffer-multibyte t)
    (list col-before w (char-width ?\x4e16) (current-column) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_triple_face_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (default font-lock-type-face default bold default)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (put-text-property 1 10 'face 'default)
  (put-text-property 1 10 'font-lock-face 'font-lock-type-face)
  (let ((ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'priority 10))
  (list (get-text-property 1 'face)
        (get-text-property 1 'font-lock-face)
        (get-char-property 1 'face)
        (get-char-property 4 'face)
        (get-char-property 8 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_max_emacs_char_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-valid-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((mc #x3FFFFF))
  (list (char-to-string mc)
        (length (char-to-string mc))
        (string-bytes (char-to-string mc))
        (aref (char-to-string mc) 0)
        (characterp mc)
        (char-valid-p mc)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_read_circle_circular_cons() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t a a t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((x (car (read-from-string "#1=(a . #1#)")))
       (tail (cdr x)))
  (list (eq tail x)
        (car x)
        (car tail)
        (eq (cdr tail) x)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_cl_remf_plist_modify() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pl (copy-tree '(:a 1 :b 2 :c 3))))
  (cl-remf (cl-getf pl :b) :b)
  (list pl (cl-getf pl :b :gone) (cl-getf pl :a)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_window_start_end_after_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 1 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\nline3\nline4\nline5\n")
  (goto-char 1)
  (list (window-start)
        (window-end)
        (progn (forward-line 2) (window-start))
        (count-lines (window-start) (window-end))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_set_buffer_file_coding_system_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café")
  (list (buffer-file-coding-system)
        (progn (set-buffer-file-coding-system 'utf-8-unix)
               (buffer-file-coding-system))
        (progn (set-buffer-file-coding-system 'latin-1-unix)
               (buffer-file-coding-system))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_process_sentinel_with_buffer_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"output\\n\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx17-sl*"))
      (sentinel-ev nil))
  (let ((p (make-process :name "neo-cx17-sl" :command '("echo" "output")
                         :buffer buf
                         :sentinel (lambda (proc event) (setq sentinel-ev event)))))
    (while (accept-process-output p 1)))
  (list (with-current-buffer buf (buffer-string))
        (if sentinel-ev (string-match "finished" sentinel-ev) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_overlay_evaporate_zero_length_via_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil t 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (let ((ov (make-overlay 5 10)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'bold)
    (delete-region 5 10)
    (list (overlay-start ov) (overlay-end ov) (overlayp ov)
          (length (overlays-in 1 20)))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_string_to_multibyte_vs_make_multibyte_utf8_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((4194243 4194217 4194274 4194178 4194220) (4194243 4194217 4194274 4194178 4194220) 5 5 t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string 195 169 226 130 172))
       (s2m (string-to-multibyte raw))
       (smk (string-make-multibyte raw)))
  (list (append s2m nil) (append smk nil)
        (length s2m) (length smk)
        (equal s2m smk)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_hash_table_iterate_maphash_order() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"b\" \"c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht) (puthash "c" 3 ht)
  (let (keys)
    (maphash (lambda (k v) (push k keys)) ht)
    (sort keys #'string<)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_condition_case_error_data_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((error \"msg 42\") (wrong-type-argument stringp 5) (arith-error))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (error "msg %d" 42) (error (cons (car e) (cdr e))))
      (condition-case e (signal 'wrong-type-argument '(stringp 5)) (error e))
      (condition-case e (/ 1 0) (arith-error (cons (car e) (cdr e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_cl_coerce_number_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-coerce 65 'string)
      (cl-coerce 233 'char)
      (cl-coerce ?a 'char)
      (cl-coerce '(97 98 99) 'string))
"##,
        expect,
    );
}

#[test]
fn div_cx17_process_output_multibyte_correct_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café世界\" 6 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((coding-system-for-read 'utf-8-unix))
    (call-process "printf" nil t nil "%s" "café世界"))
  (list (buffer-string) (length (buffer-string)) (string-bytes (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_undo_after_multiple_edit_text_prop_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"X23456789\" 1 3 (face bold)) 4 nil 1) #(\"0123456789\" 0 2 (face bold) 2 4 (face bold)) 5 (face bold) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (let ((m (set-marker (make-marker) 5)))
    (put-text-property 1 5 'face 'bold)
    (let ((ov (make-overlay 3 7))) (overlay-put ov 'face 'italic))
    (undo-boundary)
    (goto-char 3) (insert "X")
    (undo-boundary)
    (delete-region 1 3)
    (let ((state (list (buffer-string) (marker-position m)
                       (text-properties-at 1) (length (overlays-at 3)))))
      (undo) (undo) (undo)
      (list state (buffer-string) (marker-position m)
            (text-properties-at 1) (length (overlays-at 3))))))
"##,
        expect,
    );
}

#[test]
fn div_cx17_print_read_vector_of_symbols_eq() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((v [a b c])
       (p (prin1-to-string v))
       (back (car (read-from-string p))))
  (list (eq (aref back 0) 'a)
        (eq (aref back 1) 'b)
        (vectorp back)
        (length back)))
"##,
        expect,
    );
}

#[test]
fn div_cx17_decode_encode_region_roundtrip_latin1_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café\" \"caf\\351\" #(\"café\" 0 4 (charset iso-8859-1)) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café")
  (let ((orig (buffer-string)))
    (encode-coding-region (point-min) (point-max) 'latin-1)
    (let ((encoded (buffer-string)))
      (decode-coding-region (point-min) (point-max) 'latin-1)
      (list orig encoded (buffer-string) (equal orig (buffer-string))))))
"##,
        expect,
    );
}
