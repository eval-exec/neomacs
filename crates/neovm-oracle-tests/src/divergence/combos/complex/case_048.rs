//! Complex combo batch 48 — fresh subsystem edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx48_delete_duplicate_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"unique\\nduplicate\\nother\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "unique\nduplicate\nduplicate\nother\nunique\n")
  (delete-duplicate-lines (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx48_sort_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"apple paragraph\\n\\nmango paragraph\\n\\nzebra paragraph\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "zebra paragraph\n\napple paragraph\n\nmango paragraph\n")
  (sort-paragraphs nil (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx48_sort_fields() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1 alpha\\n2 bravo\\n3 charlie\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "3 charlie\n1 alpha\n2 bravo\n")
  (sort-fields 1 (point-min) (point-max))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx48_rx_to_string_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:prefix[a-z]+-?\\\\(?1:[[:digit:]]+\\\\)suffix\\\\)\" 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (rx-to-string '(seq "prefix" (+ (any "a-z")) (? "-") (group-n 1 (+ digit)) "suffix"))
      (string-match (rx-to-string '(seq bow (+ (any "a-z")) eow)) "hello world"))
"##,
        expect,
    );
}

#[test]
fn div_cx48_whitespace_cleanup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"  trailing spaces\\n\t\tindented\\nempty line above\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "  trailing spaces   \n\t\tindented\nempty line above\n")
      (whitespace-cleanup)
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_ansi_color_apply_on_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'ansi-color)
      (with-temp-buffer
        (insert "\e[31mred text\e[0m normal \e[1;32mgreen bold\e[0m")
        (ansi-color-apply-on-region (point-min) (point-max))
        (list (buffer-string) (text-properties-at 0))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_fill_individual_paragraphs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK \"> para one is\\n> long enough\\n> para two also\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 15))
    (insert "> para one is long enough\n> para two also\n")
    (fill-individual-paragraphs (point-min) (point-max))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx48_newline_and_indent() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"(foo\\n bar)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(foo bar)")
  (goto-char 6)
  (newline-and-indent)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx48_iso_char_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . file-missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'iso-conv)
      (list (iso-iso2sgml "café © ®")
            (iso-sgml2iso "caf&eacute; &copy;")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_highlight_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "hello world foo bar\n")
      (highlight-regexp "foo" 'hi-yellow)
      (prog1 (get-text-property 13 'font-lock-face)
        (unhighlight-regexp "foo")))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_syntax_propertize_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "(defun foo () \"doc\" body)")
      (syntax-propertize (point-max))
      (goto-char 15)
      (nth 3 (syntax-ppss)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_face_remap_add_relative_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((cookie (face-remap-add-relative 'default :weight 'bold :foreground "red")))
    (list (consp cookie)
          (face-attribute 'default :weight)
          (progn (face-remap-remove-relative cookie)
                 (face-attribute 'default :weight))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_cl_setf_on_buffer_local_variable() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-setf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx48-var :global)
  (with-temp-buffer
    (setq-local neo-cx48-var :local)
    (cl-setf (default-value 'neo-cx48-var) :new-default)
    (list neo-cx48-var
          (default-value 'neo-cx48-var)
          (let ((neo-cx48-var :shadowed))
            (setq neo-cx48-var :set-in-shadow)
            neo-cx48-var))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_process_filter_accumulate_secure_hash_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (all)
  (let ((p (make-process :name "neo-cx48-ha" :command '("printf" "%d %d %d %s" 1 2 3 "café")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str all))))
    (accept-process-output p 1)
    (accept-process-output p 0.5))
  (secure-hash 'sha256 (apply #'concat (nreverse all))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_overlay_evaporate_insert_delete_undo_marker_display_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 5 8 'display "XX")
  (let ((ov (make-overlay 6 12)) (m (set-marker (make-marker) 9)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (narrow-to-region 3 14)
    (undo-boundary)
    (goto-char 7) (insert "YZ")
    (undo-boundary)
    (delete-region 4 10)
    (let ((state (list (overlayp ov) (overlay-start ov)
                       (marker-position m) (text-properties-at 1)
                       (current-column))))
      (undo)
      (list state (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)
            (text-properties-at 5) (current-column))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_cl_defmethod_no_applicable_method_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:a :other-error)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx48-a () ())
  (defclass neo-cx48-b () ())
  (cl-defgeneric neo-cx48-fn (obj))
  (cl-defmethod neo-cx48-fn ((obj neo-cx48-a)) :a)
  (list (neo-cx48-fn (neo-cx48-a))
        (condition-case e (neo-cx48-fn (neo-cx48-b)) (no-applicable-method :no-method) (error :other-error))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_coding_system_category_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (coding-category-utf-8 coding-category-utf-8-auto coding-category-utf-8-sig coding-category-charset coding-category-charset coding-category-emacs-mule coding-category-utf-16-auto coding-category-utf-16-be-nosig coding-category-utf-16-le-nosig coding-category-big5)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar #'coding-system-category
        '(utf-8 utf-8-auto utf-8-with-signature latin-1 iso-8859-7 emacs-mule
          utf-16 utf-16be utf-16le big5))
"##,
        expect,
    );
}

#[test]
fn div_cx48_read_from_string_ignore_extra_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((first) 7 (second) 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((input "(first) (second) garbage")
       (r1 (read-from-string input 0))
       (r2 (read-from-string input (cdr r1))))
  (list (car r1) (cdr r1) (car r2) (cdr r2)))
"##,
        expect,
    );
}

#[test]
fn div_cx48_superword_mark_word_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (superword-mode 1)
      (insert "snake_case_var camelCase")
      (goto-char 1)
      (mark-word 1)
      (list (region-beginning) (region-end)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx48_json_deeply_nested_roundtrip_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 \"café\" \"世界\" (1 2 3) \"6ebd3534316e3335a0621c706a3654b2\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'json)
      (let* ((data '(((name . "café") (items . [1 2 3]))
                     ((name . "世界") (items . [4 5 6]))))
             (enc (json-encode data))
             (dec (json-read-from-string enc)))
        (list (length dec)
              (cdr (assoc 'name (aref dec 0)))
              (cdr (assoc 'name (aref dec 1)))
              (append (cdr (assoc 'items (aref dec 0))) nil)
              (secure-hash 'md5 enc))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}
