//! Complex combo batch 36 — occur, package-version, subword-mode, page
//! navigation, thingatpt deeper, which-function, imenu, table.el.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx36_occur_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "apple\nbanana\napple pie\ncherry\n")
      (let ((buf (get-buffer-create "*Occur*")))
        (occur "apple")
        (prog1 (with-current-buffer buf (count-lines (point-min) (point-max)))
          (kill-buffer buf))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_package_version_split_join() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'package)
      (list (package-version-join '(1 2 3))
            (package-version-split "1.2.3")
            (package-version-join (package-version-split "2.0.5"))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_subword_movement() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (subword-mode 1)
      (insert "camelCaseString PascalCase")
      (goto-char 1)
      (forward-word 1)
      (let ((p1 (point)))
        (subword-forward 1)
        (list p1 (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_page_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil \"page3\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((page-delimiter "\f"))
  (with-temp-buffer
    (insert "page1\n\fpage2\n\fpage3\n")
    (goto-char 1)
    (list (forward-page 1)
          (forward-page 1)
          (buffer-substring (point) (point-max)))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_thingatpt_list_sexp_deeper() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(a b)\" \"b\" (12 . 17) (15 . 16))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo (a b)\n  (list a b))\n")
  (goto-char 15)
  (list (thing-at-point 'list)
        (thing-at-point 'sexp)
        (bounds-of-thing-at-point 'list)
        (bounds-of-thing-at-point 'sexp)))
"##,
        expect,
    );
}

#[test]
fn div_cx36_which_function() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun my-func-1 ()\n  body)\n\n(defun my-func-2 ()\n  body)\n")
      (goto-char 30)
      (which-function))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_imenu_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun func-a () 1)\n(defun func-b () 2)\n(defvar my-var 0)\n")
      (let ((index (imenu--index-alist)))
        (list (consp index) (length index))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_add_log_current_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"neo-cx36-fn\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'add-log)
      (with-temp-buffer
        (emacs-lisp-mode)
        (insert "(defun neo-cx36-fn ()\n  body)\n")
        (goto-char 20)
        (add-log-current-defun)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_table_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK #(\"+-----+-----+-----+\\n|     |     |     |\\n+-----+-----+-----+\\n|     |     |     |\\n+-----+-----+-----+\\n\" 21 22 (table-valign nil table-justify nil keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 22 25 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 25 26 (table-valign nil table-justify nil keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 26 27 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t) 27 32 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 32 33 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t) 33 38 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 38 39 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t) 61 66 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 66 67 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t) 67 72 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 72 73 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t) 73 78 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) font-lock-face table-cell cursor-sensor-functions (table--point-entered/left-cell-function)) 78 79 (keymap table-cell-map table-cell t yank-handler (nil nil t nil) rear-nonsticky t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn (require 'table)
      (with-temp-buffer
        (table-insert 3 2)
        (buffer-string)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_char_syntax_in_syntax_table_override() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 95 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (make-syntax-table)))
  (modify-syntax-entry ?@ "_" st)
  (modify-syntax-entry ?% "." st)
  (with-temp-buffer
    (with-syntax-table st
      (insert "foo@bar %baz")
      (goto-char 1)
      (forward-word 1)
      (list (point) (char-syntax ?@) (char-syntax ?%)))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_overlay_evaporate_undo_text_prop_marker_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t 1 nil) t nil nil 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (put-text-property 1 4 'face 'bold)
  (let ((ov (make-overlay 2 6)) (m (set-marker (make-marker) 5)))
    (overlay-put ov 'face 'italic)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (delete-region 1 10)
    (let ((evap (list (overlayp ov) (marker-position m) (text-properties-at 1))))
      (undo)
      (list evap (overlayp ov) (overlay-start ov) (overlay-end ov)
            (marker-position m) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_coding_system_decode_string_then_re_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é\" 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((decoded (decode-coding-string (unibyte-string 99 97 102 195 169) 'utf-8)))
  (with-temp-buffer
    (insert decoded)
    (goto-char 1)
    (re-search-forward "caf\\(.\\)" nil t)
    (list (match-string 1) (match-end 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_process_output_buffer_with_markers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (3 \"header\\nappended\\n\\nProcess neo-cx36-pm finished\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx36-pm*")))
  (with-current-buffer buf
    (insert "header\n")
    (let ((m (set-marker (make-marker) 3)))
      (let ((p (make-process :name "neo-cx36-pm" :command '("echo" "appended")
                             :buffer buf)))
        ;; GNU's accept-process-output may return after reading output but
        ;; before publishing the child exit/default sentinel.  Wait for the
        ;; process lifecycle before asserting the final buffer text; this case
        ;; is about process output insertion before existing markers.
        (while (process-live-p p)
          (accept-process-output p 1))
        (while (accept-process-output p 0)))
      (prog1 (list (marker-position m)
                   (with-current-buffer buf (buffer-string)))
        (kill-buffer buf)))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_format_c_concat_then_string_bytes_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 9 (\"Hi\" \"あ中\") (2 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s1 (format "%c%c" ?H ?i))
       (s2 (format "%c%c" #x3042 #x4e2d))
       (cat (concat s1 "-" s2))
       (split (split-string cat "-")))
  (list (length cat) (string-bytes cat) split
        (mapcar #'length split)))
"##,
        expect,
    );
}

#[test]
fn div_cx36_set_buffer_multibyte_then_char_syntax_then_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 119 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((cs1 (char-syntax (aref (buffer-string) 3))))
    (set-buffer-multibyte nil)
    (let ((cs2 (char-syntax (aref (buffer-string) 3))))
      (set-buffer-multibyte t)
      (list cs1 cs2 (char-syntax (aref (buffer-string) 3))))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_cl_loop_for_across_vector_with_index() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for x across [10 20 30 40]
         for i from 0
         collect (cons i x))
"##,
        expect,
    );
}

#[test]
fn div_cx36_undo_redo_cycle_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"A\" \"AB\" \"ABC\" \"AB\" \"AB\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (let (states)
    (insert "A") (undo-boundary) (push (buffer-string) states)
    (insert "B") (undo-boundary) (push (buffer-string) states)
    (insert "C") (undo-boundary) (push (buffer-string) states)
    (undo) (push (buffer-string) states)
    (undo) (push (buffer-string) states)
    (condition-case nil (while t (redo) (push (buffer-string) states)) (error))
    (nreverse states)))
"##,
        expect,
    );
}

#[test]
fn div_cx36_coding_system_base_aliases_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 utf-8 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-base 'utf-8-unix)
      (coding-system-base 'utf-8-dos)
      (coding-system-base 'utf-8-mac)
      (eq (coding-system-base 'utf-8-unix) (coding-system-base 'utf-8-dos)))
"##,
        expect,
    );
}

#[test]
fn div_cx36_buffer_string_with_text_props_then_prin1_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (face italic) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'face 'italic)
  (let* ((sub (buffer-substring 1 6))
         (p (prin1-to-string sub))
         (back (car (read-from-string p))))
    (list (text-properties-at 0 back)
          (text-properties-at 3 back)
          (equal sub back))))
"##,
        expect,
    );
}

#[test]
fn div_cx36_process_exit_code_exit_zero_vs_nonzero_clean() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (call-process "sh" nil nil nil "-c" "exit 0")
      (call-process "sh" nil nil nil "-c" "exit 5")
      (let ((p (make-process :name "neo-cx36-ec" :command '("sh" "-c" "exit 5")))
        (accept-process-output p 2)
        (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx36_window_start_end_stable_after_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx36-ws*")))
  (with-current-buffer buf
    (dotimes (i 5) (insert (format "line %d\n" i))))
  (set-window-buffer (selected-window) buf)
  (let ((ws (window-start)) (we (window-end)))
    (with-current-buffer buf (goto-char 10) (insert "X"))
    (prog1 (list ws we (window-start) (window-end))
      (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
      (kill-buffer buf)))
"##,
        expect,
    );
}
