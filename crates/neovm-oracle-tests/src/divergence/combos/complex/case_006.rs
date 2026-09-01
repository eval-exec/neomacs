//! Complex combo divergence probes batch 6 — encoding/process/coding edges.
//!
//! Extend the encode-coding-string replacement-char bug across codings,
//! set-process-coding-system + output, set-buffer-file-coding-system round-trip,
//! multiple-value-bind, register ops, insert-file-contents REPLACE, write-region
//! to process stdin, print-escape-newline, char-fold + case-fold combined.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx6_encode_unencodable_various_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((32) (164) (128))""#]];
    // € (U+20AC) unencodable in latin-1 but encodable in latin-9.
    crate::common::assert_oracle_parity_expect(
        r##"
(list (append (encode-coding-string "€" 'iso-8859-1) nil)
      (append (encode-coding-string "€" 'iso-8859-15) nil)
      (append (encode-coding-string "€" 'windows-1252) nil))
"##,
        expect,
    );
}

#[test]
fn div_cx6_encode_unencodable_multiple_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (97 32 98 32 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(append (encode-coding-string "a€b中c" 'iso-8859-1) nil)
"##,
        expect,
    );
}

#[test]
fn div_cx6_set_process_coding_system_output() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"hello\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx6-pc" :command '("echo" "hello")
                         :buffer (current-buffer))))
    ;; The default sentinel can be delivered in the same wait on GNU.  This
    ;; case only asserts explicit process-output decoding.
    (set-process-sentinel p #'ignore)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx6_set_buffer_file_coding_system_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx6-bfc-")))
  (with-temp-buffer
    (insert "café世界")
    (set-buffer-file-coding-system 'utf-8-unix)
    (write-region (buffer-string) nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix))
             (insert-file-contents f))
           (list (buffer-string) (buffer-file-coding-system)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx6_multiple_value_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-multiple-value-bind)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-multiple-value-bind (a b c) (values 1 2 3) (list a b c))
      (cl-multiple-value-list (values :x :y))
      (multiple-value-list (floor 17 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_register_ops_types() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 . 2) \"hello\" 42 [:vector :register] nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (set-register ?a (cons 1 2))
  (set-register ?b "hello")
  (set-register ?c 42)
  (set-register ?d [:vector :register])
  (list (get-register ?a) (get-register ?b) (get-register ?c) (get-register ?d)
        (get-register ?z)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_insert_file_contents_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"1234567890\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx6-ifr-")))
  (write-region "1234567890" nil f nil 0)
  (prog1 (with-temp-buffer
           (insert "ABCDE")
           (goto-char 2)
           (insert-file-contents f nil nil nil t)
           (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx6_write_region_to_process_stdin() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument stringp #<process neo-cx6-wp>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx6-wp" :command '("cat")
                         :buffer (current-buffer) :connection-type 'pipe)))
    (write-region "hello via cat\n" nil p)
    (process-send-eof p)
    (accept-process-output p 1))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx6_print_escape_newline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\\"line1\\\\nline2\ttab\\\"\" \"\\\"no-newline\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t))
  (list (prin1-to-string "line1\nline2\ttab")
        (prin1-to-string "no-newline")))
"##,
        expect,
    );
}

#[test]
fn div_cx6_char_fold_plus_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 101)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (string-match (char-fold-to-regexp ?e) "CAFÉ")
        (string-match (char-fold-to-regexp ?E) "café")))
"##,
        expect,
    );
}

#[test]
fn div_cx6_default_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((default-text-properties '(face default)))
    (put-text-property 2 4 'mouse-face 'highlight))
  (list (text-properties-at 0) (text-properties-at 1) (text-properties-at 2)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_text_property_any_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 3 6 'face 'bold)
  (narrow-to-region 2 8)
  (list (text-property-any (point-min) (point-max) 'face 'bold)
        (text-property-not-all (point-min) (point-max) 'face nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_narrow_excursion_stack() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 1 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDE")
  (narrow-to-region 1 10)
  (save-excursion
    (save-restriction
      (widen)
      (narrow-to-region 5 15)
      (goto-char 7)))
  (list (point) (point-min) (point-max)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_cl_letf_buffer_local() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-letf)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defvar neo-cx6-letf 5)
  (list (cl-letf (((default-value 'neo-cx6-letf) 99))
          (default-value 'neo-cx6-letf))
        (default-value 'neo-cx6-letf)))
"##,
        expect,
    );
}

#[test]
fn div_cx6_set_marker_insertion_type_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((m1 (set-marker (make-marker) 3))
        (m2 (set-marker (make-marker) 3)))
    (set-marker-insertion-type m2 t)
    (goto-char 3)
    (insert "X")
    (let ((p1 (marker-position m1)) (p2 (marker-position m2)))
      (undo)
      (list p1 p2 (marker-position m1) (marker-position m2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx6_compare_strings_case_fold_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t -1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (compare-strings "CAFÉ" nil nil "café" nil nil t)
      (compare-strings "CAFÉ" nil nil "café" nil nil nil)
      (compare-strings "世界" nil nil "世界" nil nil nil))
"##,
        expect,
    );
}

#[test]
fn div_cx6_buffer_local_hook_run_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:local)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (with-temp-buffer
    (add-hook 'neo-cx6-hook (lambda () (push :local fired)) nil t)
    (run-hooks 'neo-cx6-hook)
    (remove-hook 'neo-cx6-hook (car neo-cx6-hook) t)
    (run-hooks 'neo-cx6-hook))
  fired)
"##,
        expect,
    );
}

#[test]
fn div_cx6_format_message_quotes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a ‘sym’\" \"don’t\" \"‘nested’ ‘deeper’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format-message "a `%s'" 'sym)
      (format-message "don't")
      (format-message "`nested' `deeper'"))
"##,
        expect,
    );
}

#[test]
fn div_cx6_with_slots_oref_oset_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx6-obj () ((a :initarg :a) (b :initarg :b)))
  (let ((o (neo-cx6-obj :a 1 :b 2)))
    (with-slots (a b) o
      (oset o b (+ a b)))
    (list (oref o a) (oref o b))))
"##,
        expect,
    );
}

#[test]
fn div_cx6_process_connection_type_pipe_vs_pty() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (file-missing \"Unknown connection type\" \"No such file or directory\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p1 (make-process :name "neo-cx6-pty" :command '("echo" "pty")
                         :connection-type t))
      (p2 (make-process :name "neo-cx6-pipe" :command '("echo" "pipe")
                        :connection-type 'pipe)))
  (accept-process-output p1 1)
  (accept-process-output p2 1)
  (list (processp p1) (processp p2) (eq (process-status p1) 'exit)))
"##,
        expect,
    );
}
