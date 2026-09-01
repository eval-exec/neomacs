//! Complex combo batch 14 — coding conversion hooks, define-coding-system,
//! read #& bool-vector, char-table parent+map, secure-hash file,
//! text-property-search-backward, font-lock-add-keywords, window sizing,
//! md5 with coding, princ edge types.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx14_coding_system_post_read_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let (hook-called)
      (define-coding-system 'neo-cx14-cs
        "Test coding" :coding-type 'utf-8 :mnemonic ?T
        :post-read-conversion (lambda (len) (setq hook-called :post-read) len)
        :pre-write-conversion (lambda (from to) (setq hook-called :pre-write)))
      (let ((decoded (decode-coding-string "abc" 'neo-cx14-cs)))
        (list decoded hook-called)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_define_coding_system_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t utf-8 84)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-coding-system 'neo-cx14-dcs "Test"
        :coding-type 'utf-8 :mnemonic ?T :charset-list '(unicode))
      (list (coding-system-p 'neo-cx14-dcs)
            (coding-system-type 'neo-cx14-dcs)
            (coding-system-mnemonic 'neo-cx14-dcs)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_read_bool_vector_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bv (car (read-from-string "#&8\"\\0\""))))
  (list (bool-vector-p bv) (length bv)))
"##,
        expect,
    );
}

#[test]
fn div_cx14_char_table_parent_map_iteration() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-char-table 'cx14 nil)) (c (make-char-table 'cx14 nil))
      (count 0))
  (set-char-table-range p '(?a . ?e) :parent)
  (aset p ?z :parent-z)
  (set-char-table-parent c p)
  (aset c ?f :child-f)
  (map-char-table (lambda (k v) (when v (setq count (1+ count)))) c)
  count)
"##,
        expect,
    );
}

#[test]
fn div_cx14_secure_hash_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"0614fd732131f1cde1e8b8e1fbd62bbb923ae6e9505d8db59010d2e51334656b\" \"e25dd806d495b413931f4eea50b677a7a5c02d00460924661283f211a37f7e7f\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((f "/tmp/neo-cx14-shf-fixed")
       (content "test content for hashing")
       (path-hash (secure-hash 'sha256 f))
       (content-hash (secure-hash 'sha256 content)))
  (ignore-errors (write-region content nil f nil 'silent))
  (prog1 (list path-hash content-hash (equal path-hash content-hash))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_md5_with_explicit_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument integerp utf-8-unix)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (md5 "café世界" nil 'utf-8-unix)
      (md5 "café世界" nil 'latin-1-unix)
      (md5 "abc" nil 'utf-8))
"##,
        expect,
    );
}

#[test]
fn div_cx14_text_property_search_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:not-available)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "aaaBBBBcccDDDDeee")
      (put-text-property 4 8 'face 'bold)
      (put-text-property 11 15 'face 'italic)
      (goto-char 16)
      (let ((m (text-property-search-backward 'face 'italic t)))
        (list (if m t nil)
              (prop-match-beginning m)
              (prop-match-end m))))
  (void-function (list :not-available))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_font_lock_add_keywords_dynamic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (font-lock-keyword-face nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (font-lock-add-keywords nil '(( "\\bneo_kw\\b" . font-lock-keyword-face)))
    (insert "(neo_kw other)")
    (font-lock-fontify-buffer)
    (list (get-text-property 2 'face)
          (get-text-property 8 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_window_total_size_after_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (24 80 (t 40) 80)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((orig-h (window-total-height))
          (orig-w (window-total-width)))
      (split-window nil nil 'right)
      (let ((after-split (list (> orig-w (window-total-width))
                               (window-total-width))))
        (delete-other-windows)
        (list orig-h orig-w after-split (window-total-width))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_princ_edge_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""niltsymbol423.14string(a b c)OK (nil t symbol 42 3.14 \"string\" (a b c))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (princ nil)
      (princ t)
      (princ 'symbol)
      (princ 42)
      (princ 3.14)
      (princ "string")
      (princ '(a b c)))
"##,
        expect,
    );
}

#[test]
fn div_cx14_decode_encode_char_many_charsets() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (233 233 9008 97)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (encode-char ?é 'unicode) (error (cons 'err (car e))))
      (condition-case e (decode-char 'unicode 233) (error (cons 'err (car e))))
      (condition-case e (encode-char (make-char 'japanese-jisx0208 35 48) 'japanese-jisx0208) (error (cons 'err (car e))))
      (condition-case e (encode-char ?a 'ascii) (error (cons 'err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_with_temp_message_and_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (msg)
  (with-temp-message "temp msg %d" 42
    (setq msg (current-message)))
  msg)
"##,
        expect,
    );
}

#[test]
fn div_cx14_buffer_string_with_text_props_overlap() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((face bold) (mouse-face highlight face bold) (mouse-face highlight) \"AABBBBC\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAABBBBCCCCDDDD")
  (put-text-property 1 8 'face 'bold)
  (put-text-property 5 12 'mouse-face 'highlight)
  (let ((s1 (buffer-substring 3 10))
        (s2 (buffer-substring-no-properties 3 10)))
    (list (text-properties-at 0 s1)
          (text-properties-at 4 s1)
          (text-properties-at 6 s1)
          s2)))
"##,
        expect,
    );
}

#[test]
fn div_cx14_coding_system_get_safe_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((unicode) (iso-8859-1) nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :charset-list)
      (coding-system-get 'iso-8859-1 :charset-list)
      (coding-system-get 'emacs-mule :safe-charsets))
"##,
        expect,
    );
}

#[test]
fn div_cx14_process_kill_buffer_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (let ((buf (get-buffer-create " *neo-cx14-pk*")))
    (with-current-buffer buf (insert "preexisting"))
    (let ((p (make-process :name "neo-cx14-pk" :command '("echo" "output")
                           :buffer buf
                           :filter (lambda (proc msg) (setq fired :filter-fired)
                                     (with-current-buffer (process-buffer proc) (insert msg))))))
      (accept-process-output p 1)
      (let ((content (with-current-buffer buf (buffer-string))))
        (kill-buffer buf)
        (list fired content (process-buffer p)))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_cl_lex_default_vs_dynamic_scope() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t))
  (let ((x 1))
    (let ((f (lambda () x)))
      (let ((x 2))
        (list (funcall f) x)))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_unwind_protect_nested_throw_catch_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:body-start :inner-clean :outer-clean)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (log)
  (catch 'tag
    (unwind-protect
        (progn
          (push :body-start log)
          (unwind-protect
              (throw 'tag :thrown)
            (push :inner-clean log))
          (push :body-end log))
      (push :outer-clean log)))
  (nreverse log))
"##,
        expect,
    );
}

#[test]
fn div_cx14_decode_coding_region_latin1_in_multibyte_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"caféâ\u{82}¬\" 0 7 (charset iso-8859-1)) (99 97 102 195 169 195 162 194 130 194 172) 12)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 99 97 102 233 226 130 172))
  (decode-coding-region (point-min) (point-max) 'latin-1)
  (list (buffer-string) (append (buffer-string) nil) (point-max)))
"##,
        expect,
    );
}

#[test]
fn div_cx14_overlay_buffer_after_kill() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx14-ob*")))
  (with-current-buffer buf
    (insert "hello")
    (let ((ov (make-overlay 1 3)))
      (overlay-put ov 'face 'bold)
      (let ((ov-start (overlay-start ov))
            (ov-buf (overlay-buffer ov)))
        (kill-buffer buf)
        (list ov-start ov-buf (overlay-buffer ov) (overlay-start ov)))))
"##,
        expect,
    );
}

#[test]
fn div_cx14_string_to_number_edge_bases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (123 255 511 10 1000.0 0 42 3.14)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-to-number "123")
      (string-to-number "ff" 16)
      (string-to-number "777" 8)
      (string-to-number "1010" 2)
      (string-to-number "1e3" 10)
      (string-to-number "0x1f" 16)
      (string-to-number "  42  ")
      (string-to-number "3.14"))
"##,
        expect,
    );
}
