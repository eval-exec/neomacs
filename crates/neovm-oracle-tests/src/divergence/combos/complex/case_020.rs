//! Complex combo batch 20 — milestone 20th. Remaining encoding/coding edges
//! (prefer-coding-system, file-coding-system-alist, process defaults),
//! window-parameter, char-fold-table modify, modify-syntax-entry + parse,
//! marker after kill-buffer, backtrace-frame, format-mode-line custom.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx20_prefer_coding_system_priority() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (20 20 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((before (length (coding-system-priority-list))))
  (prefer-coding-system 'utf-8)
  (list before
        (length (coding-system-priority-list))
        (car (coding-system-priority-list))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_file_coding_system_alist_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"\\\\.txt\\\\'\" . utf-8-unix) (utf-8-unix . utf-8-unix))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((file-coding-system-alist '(("\\.txt\\'" . utf-8-unix))))
  (list (assoc "\\.txt\\'" file-coding-system-alist)
        (find-operation-coding-system 'insert-file-contents "/tmp/test.txt")))
"##,
        expect,
    );
}

#[test]
fn div_cx20_insert_file_contents_coding_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments insert-file-contents 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx20-ic-")))
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region "café" nil f nil 'silent))
  (prog1 (with-temp-buffer
           (insert-file-contents f nil nil nil nil 'utf-8-unix)
           (list (buffer-string) (length (buffer-string))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_window_parameter_get_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:val nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (set-window-parameter w 'neo-param :val)
  (list (window-parameter w 'neo-param)
        (window-parameter w 'nonexistent)
        (consp (window-parameters w))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_char_fold_table_modify_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-fold-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cft (char-fold-table)))
  (set-char-table-range cft ?a (string ?ä ?á ?à))
  (list (string-match (char-fold-to-regexp ?a) "ä")
        (string-match (char-fold-to-regexp ?a) "x")))
"##,
        expect,
    );
}

#[test]
fn div_cx20_modify_syntax_entry_parse_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 46 39)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?_ "." st)
  (modify-syntax-entry ?# "'" st)
  (with-temp-buffer
    (with-syntax-table st
      (insert "foo_bar #baz")
      (goto-char 1)
      (list (progn (forward-word 1) (point))
            (char-syntax ?_)
            (char-syntax ?#)))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_marker_position_after_kill_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx20-mk*")))
  (with-current-buffer buf
    (insert "hello")
    (let ((m (set-marker (make-marker) 3 (current-buffer))))
      (let ((pos-before (marker-position m))
            (buf-before (marker-buffer m)))
        (kill-buffer buf)
        (list pos-before buf-before
              (marker-position m)
              (marker-buffer m)))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_backtrace_frame_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'backtrace-frame)
          (let ((debug-on-error nil))
            (condition-case err
                (car (backtrace-frame 0))
              (error :backtrace-error))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_format_mode_line_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((s (format-mode-line '("[" mode-name "] " "%l:%c"))))
      (list (stringp s) (> (length s) 2)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_process_default_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx20-dc" :command '("echo" "x"))))
  (let ((cs (process-coding-system p)))
    (prog1 (list (consp cs) (coding-system-p (car cs)) (coding-system-p (cdr cs)))
      (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_keyboard_terminal_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-p (keyboard-coding-system))
      (coding-system-p (terminal-coding-system)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_char_width_table_modify_move_to_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-width-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aéb")
  (let ((orig (char-width ?é)))
    (set-char-table-range (char-width-table) ?é 3)
    (prog1 (list orig (char-width ?é)
                 (current-column)
                 (progn (forward-char) (current-column))
                 (string-width "aéb"))
      (set-char-table-range (char-width-table) ?é orig))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_cl_defstruct_copier_predicate_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-cx20-box (:constructor neo-cx20-make (size))
                              (:copier neo-cx20-copy-box))
    size content)
  (let ((b (neo-cx20-make 42)))
    (setf (neo-cx20-box-content b) "data")
    (let ((c (neo-cx20-copy-box b)))
      (list (neo-cx20-box-size b) (neo-cx20-box-content b)
            (neo-cx20-box-size c) (neo-cx20-box-content c)
            (neo-cx20-box-p b)
            (eq b c)))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_set_buffer_multibyte_nil_then_insert_ascii() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function enable-multibyte-characters)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert "hello")
  (list (multibyte-string-p (buffer-string))
        (length (buffer-string))
        (enable-multibyte-characters)
        (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_hash_table_test_equal_including_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Invalid hash table test\" equal-including-properties)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal-including-properties)))
  (puthash #("x" 0 1 (face bold)) 1 ht)
  (list (gethash #("x" 0 1 (face bold)) ht)
        (gethash #("x" 0 1 (face italic)) ht)
        (gethash "x" ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_overlay_window_specific_visibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world")
  (let ((ov (make-overlay 1 5)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'window (selected-window)))
  (list (length (overlays-at 2))
        (eq (overlay-get (car (overlays-at 2)) 'window) (selected-window))))
"##,
        expect,
    );
}

#[test]
fn div_cx20_decode_coding_string_unibyte_result_flag() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function unibyte-string-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (decode-coding-string (unibyte-string 65 66 67) 'no-conversion)))
  (list (multibyte-string-p d) (unibyte-string-p d)
        (append d nil) (length d)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_cl_typecase_with_satisfies() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typecase)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typecase 5 (satisfies cl-evenp) :even (integer :odd))
      (cl-typecase 6 (satisfies cl-evenp) :even (integer :odd))
      (cl-typecase "x" (satisfies stringp) :string (t :other)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_string_match_data_after_replace_in_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 3) 0 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (replace-regexp-in-string "\\([a-z]+\\)" "\\1!" "abc def")
  (list (match-data)
        (match-beginning 0)
        (match-end 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx20_buffer_undo_list_after_set_text_properties_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (let ((u1 (length buffer-undo-list)))
    (undo-boundary)
    (set-text-properties 1 5 '(face bold))
    (let ((u2 (length buffer-undo-list)))
      (undo)
      (list (> u2 u1) (text-properties-at 1) (buffer-string))))
"##,
        expect,
    );
}
