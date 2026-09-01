//! Complex combo divergence probes batch 7 — deeper encoding/process edges.
//!
//! Extend encode-coding-string replacement-char across codings, process error
//! paths, call-process-region, process filter swap, coding round-trips with
//! non-UTF-8, process-buffer interaction.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx7_encode_coding_string_greek_unencodable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (225 226 227)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(append (encode-coding-string "αβγ" 'iso-8859-7) nil)
"##,
        expect,
    );
}

#[test]
fn div_cx7_encode_coding_string_cjk_unencodable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((228 184 173 230 150 135) (32 32) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append (encode-coding-string "中文" 'utf-8) nil)
      (append (encode-coding-string "中文" 'iso-8859-1) nil)
      (length (encode-coding-string "中文" 'iso-8859-1)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_encode_coding_string_emoji_unencodable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (32)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(append (encode-coding-string "😀" 'iso-8859-1) nil)
"##,
        expect,
    );
}

#[test]
fn div_cx7_call_process_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((input (get-buffer-create " *neo-cx7-in*"))
      (output (get-buffer-create " *neo-cx7-out*")))
  (with-current-buffer input (erase-buffer) (insert "5\n3\n"))
  (with-current-buffer output (erase-buffer))
  (call-process-region (point-min) (point-max) "sort" nil output nil)
  (prog1 (with-current-buffer output (buffer-string))
    (kill-buffer input) (kill-buffer output)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_process_filter_swap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (f1 f2)
  (let ((p (make-process :name "neo-cx7-fs" :command '("echo" "hello")
                         :buffer nil)))
    (set-process-filter p (lambda (proc msg) (push msg f1)))
    (accept-process-output p 1)
    (set-process-filter p (lambda (proc msg) (push :second-filter f2)))
    (delete-process p))
  (list (if f1 t nil) f2))
"##,
        expect,
    );
}

#[test]
fn div_cx7_process_buffer_set_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((b1 (get-buffer-create " *neo-cx7-b1*"))
      (b2 (get-buffer-create " *neo-cx7-b2*")))
  (let ((p (make-process :name "neo-cx7-pb" :command '("echo" "first")
                         :buffer b1)))
    (let ((attempts 0))
      (while (and (process-live-p p) (< attempts 100))
        (accept-process-output p 0.05)
        (setq attempts (1+ attempts)))
      (when (process-live-p p)
        (error "process did not exit")))
    (while (accept-process-output p 0.01))
    (set-process-buffer p b2)
    (list (eq (process-buffer p) b2)
          (with-current-buffer b1 (buffer-string))))
  (prog1 (with-current-buffer b2 (buffer-string))
    (kill-buffer b1) (kill-buffer b2)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_coding_roundtrip_latin9() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99 97 102 233 164) t 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café€")
       (e (encode-coding-string s 'iso-8859-15))
       (d (decode-coding-string e 'iso-8859-15)))
  (list (append e nil) (equal s d) (length e)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_decode_mixed_valid_invalid() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 (97 98 4194243 65 99 100) 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (decode-coding-string (unibyte-string 97 98 195 65 99 100) 'utf-8)))
  (list (length d) (append d nil) (string-bytes d)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_process_environment_inherit_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (car (split-string (shell-command-to-string "echo $HOME")))
      (length (> (length (shell-command-to-string "printenv")) 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_process_query_on_exit_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx7-qe" :command '("true"))))
  (prog1 (list (process-query-on-exit-flag p)
               (progn (set-process-query-on-exit-flag p nil)
                      (process-query-on-exit-flag p)))
    (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_call_process_stderr_capture() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument stringp #<buffer  *neo-cx7-err*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((stderr-buf (generate-new-buffer " *neo-cx7-err*")))
    (call-process "sh" nil (list t stderr-buf) nil "-c" "echo out; echo err 1>&2")
    (let ((out (buffer-string)))
      (prog1 (list out (with-current-buffer stderr-buf (buffer-string)))
        (kill-buffer stderr-buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx7_encode_then_decode_non_utf8_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 16 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "Hello 世界 €")
       (u8 (encode-coding-string s 'utf-8))
       (back (decode-coding-string u8 'utf-8)))
  (list (equal s back) (length u8) (string-bytes back)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_char_bytes_decode_encode_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((226 130 172) (8364) (226 130 172) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((bytes (unibyte-string 226 130 172))
       (char (decode-coding-string bytes 'utf-8))
       (re-enc (encode-coding-string char 'utf-8)))
  (list (append bytes nil) (append char nil) (append re-enc nil) (equal bytes re-enc)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_unibyte_string_vs_decode_equivalence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (233) (233))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((u (unibyte-string 195 169))
       (decoded (decode-coding-string u 'utf-8))
       (constructed (string 233)))
  (list (equal decoded constructed) (append decoded nil) (append constructed nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_process_list_after_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p1 (make-process :name "neo-cx7-pl1" :command '("true")))
      (p2 (make-process :name "neo-cx7-pl2" :command '("true"))))
  (let ((n (length (process-list))))
    (delete-process p1) (delete-process p2)
    n))
"##,
        expect,
    );
}

#[test]
fn div_cx7_format_escape_sequences_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (error \"Invalid format operation %'\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%-10s|" "left")
      (format "%10s|" "right")
      (format "%-+08d|" 42)
      (format "%08x|" 255)
      (format "%'d" 1234567))
"##,
        expect,
    );
}

#[test]
fn div_cx7_string_bytes_after_concat_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 6 6 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((a "café")
       (b "世界")
       (c (concat a b)))
  (list (string-bytes a) (string-bytes b) (length c) (string-bytes c)))
"##,
        expect,
    );
}

#[test]
fn div_cx7_buffer_string_vs_buffer_substring_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((face bold) (face italic) (face bold) (face italic))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAABBBBCCCC")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 5 8 'face 'italic)
  (let ((full (buffer-string))
        (sub (buffer-substring 3 10)))
    (list (text-properties-at 0 full) (text-properties-at 4 full)
          (text-properties-at 0 sub) (text-properties-at 2 sub))))
"##,
        expect,
    );
}

#[test]
fn div_cx7_overlay_modify_proto_textprop_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold underline underline nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 6 'face 'bold)
  (let ((ov (make-overlay 3 8)))
    (overlay-put ov 'face 'underline)
    (list (get-text-property 3 'face)
          (get-char-property 3 'face)
          (get-char-property 5 'face)
          (get-char-property 8 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx7_save_excursion_marker_point_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 2 \"abcdXef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (goto-char 2)
  (let ((m (point-marker)))
    (save-excursion
      (goto-char 5)
      (insert "X"))
    (list (point) (marker-position m) (buffer-string))))
"##,
        expect,
    );
}
