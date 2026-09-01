//! Complex combo batch 22 — remaining deterministic edges: terminal-parameter,
//! deeper thingatpt, outline navigation, comment-region/uncomment-region,
//! align-regexp, syntax-pp, print-vector-length, search-whitespace-regexp,
//! word-search, keymap menu-item, current-indentation, read #@N.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx22_terminal_parameter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function terminalp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((term (frame-terminal (selected-frame))))
  (set-terminal-parameter term 'neo-param :val)
  (list (terminalp term)
        (terminal-parameter term 'neo-param)
        (terminal-parameter term 'nonexistent)))
"##,
        expect,
    );
}

#[test]
fn div_cx22_thingatpt_sentence_paragraph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"First sentence.\" 30 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "First sentence.  Second one.\n\nSecond paragraph.\n")
  (goto-char 10)
  (list (thing-at-point 'sentence)
        (save-excursion (forward-paragraph 1) (point))
        (save-excursion (backward-paragraph 1) (point))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_thingatpt_sexp_defun() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(x)\" 1 36)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo (x)\n  \"Doc.\"\n  (+ x 1))\n")
  (goto-char 15)
  (list (thing-at-point 'sexp)
        (save-excursion (beginning-of-defun) (point))
        (save-excursion (end-of-defun) (point))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_outline_navigation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (outline-mode)
      (insert "* Top\n** Sub1\n*** SubSub\n** Sub2\n* Top2\n")
      (goto-char 1)
      (list (outline-level)
            (progn (outline-next-heading) (point))
            (progn (outline-next-heading) (point))
            (progn (outline-forward-same-level 1) (point))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_comment_region_uncomment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\";; line one\\n;; line two\\n;; line t\\nhree\\n\" \"line one\\nline two\\nline t\\nhree\\n\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (emacs-lisp-mode)
  (insert "line one\nline two\nline three\n")
  (comment-region 1 25)
  (let ((commented (buffer-string)))
    (uncomment-region 1 (point-max))
    (list commented (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_align_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\t\t= 1\\nfoo\t\t= 2\\nlongname\t= 3\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "a = 1\nfoo = 2\nlongname = 3\n")
      (align-regexp 1 (point-max) "\\(\\s-*\\)=")
      (buffer-string))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_syntax_pp_cache() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 34)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (emacs-lisp-mode)
      (insert "(defun foo ()\n  \"string\"\n  (+ 1 2))\n")
      (goto-char 20)
      (list (consp (syntax-ppss))
            (nth 0 (syntax-ppss))
            (nth 3 (syntax-ppss))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_word_search_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (17 12 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello world café-world test")
  (goto-char 1)
  (list (word-search-forward "café" nil t)
        (progn (goto-char 1) (word-search-forward "world" nil t))
        (progn (goto-char 1) (word-search-forward "xyz" nil t))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_keymap_menu_item() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-action nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m [menu-bar neo] (cons "Neo" (make-sparse-keymap)))
  (define-key-after m [menu-bar neo item1]
    '(menu-item "Item 1" neo-action :help "Help text"))
  (list (lookup-key m [menu-bar neo item1])
        (get (lookup-key m [menu-bar neo item1]) 'menu-item-help)))
"##,
        expect,
    );
}

#[test]
fn div_cx22_current_indentation_indent_line() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "no-indent\n    indented\n  partial\n")
  (list (current-indentation)
        (progn (forward-line 1) (current-indentation))
        (progn (forward-line 1) (current-indentation))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_print_vector_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"[nil nil nil nil nil]\" 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-length 100) (print-vector-length t))
  (list (prin1-to-string (make-vector 5 nil))
        (length (prin1-to-string (make-bool-vector 8 t)))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_search_whitespace_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil 14 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello   world\tcafé")
  (goto-char 1)
  (let ((search-spaces-regexp "[ \t]+"))
    (list (search-forward "hello world" nil t)
          (progn (goto-char 1) (search-forward "hello   world" nil t))
          (progn (goto-char 1) (search-forward "world café" nil t)))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_char_table_range_vector_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([:foo :bar] :foo t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx22 nil)))
  (set-char-table-range ct ?a [:foo :bar])
  (list (char-table-range ct ?a)
        (aref (char-table-range ct ?a) 0)
        (vectorp (char-table-range ct ?a))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_frame_parameter_modification() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"neo-cx22-test\" \"F1\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (selected-frame)))
  (let ((orig-name (frame-parameter f 'name)))
    (modify-frame-parameters f '((name . "neo-cx22-test")))
    (prog1 (list (frame-parameter f 'name) orig-name)
      (modify-frame-parameters f (list (cons 'name orig-name))))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_read_skip_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK actual-symbol""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (car (read-from-string ";; comment\nactual-symbol"))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_format_escape_multibyte_in_prin1_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 12 11 t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café\t世界\n😀")
       (p1 (prin1-to-string s))
       (p2 (let ((print-escape-newlines t)) (prin1-to-string s)))
       (p3 (let ((print-escape-nonascii t)) (prin1-to-string s))))
  (list (length p1) (length p2) (length p3)
        (equal (car (read-from-string p1)) s)
        (equal (car (read-from-string p2)) s)))
"##,
        expect,
    );
}

#[test]
fn div_cx22_buffer_narrow_marker_undo_overlays_textprops_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'mouse-face 'highlight)
  (let ((ov (make-overlay 3 7)) (m (set-marker (make-marker) 8)))
    (overlay-put ov 'face 'italic)
    (narrow-to-region 2 15)
    (undo-boundary)
    (goto-char 5) (insert "X")
    (undo-boundary)
    (put-text-property 4 8 'font-lock-face 'keyword)
    (let ((state (list (point-min) (point-max)
                       (marker-position m)
                       (overlay-start ov) (overlay-end ov)
                       (buffer-string)
                       (text-properties-at 1) (text-properties-at 4)
                       (get-char-property 3 'face))))
      (undo)
      (widen)
      (list state
            (point-min) (point-max)
            (marker-position m)
            (overlay-start ov) (overlay-end ov)
            (buffer-string)
            (text-properties-at 1) (text-properties-at 5)
            (length (overlays-at 3)))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_coding_system_for_write_file_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments insert-file-contents 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx22-cw-")))
  (let ((coding-system-for-write 'latin-1-unix))
    (write-region "café" nil f nil 'silent))
  (prog1 (list (with-temp-buffer
                 (let ((coding-system-for-read 'latin-1-unix))
                   (insert-file-contents f))
                 (buffer-string))
               (with-temp-buffer
                 (insert-file-contents f nil nil nil nil 'utf-8-unix)
                 (buffer-string)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_cl_defstruct_print_object_inherited() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx22-animal name)
  (cl-defstruct (neo-cx22-cat (:include neo-cx22-animal)) color)
  (let ((c (make-neo-cx22-cat :name "Whiskers" :color "black")))
    (list (neo-cx22-animal-name c)
          (neo-cx22-cat-color c)
          (neo-cx22-cat-p c)
          (neo-cx22-animal-p c)
          (string-match "cat" (prin1-to-string c)))))
"##,
        expect,
    );
}

#[test]
fn div_cx22_process_list_filter_dead() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 0 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
  (let ((p (make-process :name "neo-cx22-pl" :command '("true"))))
    (accept-process-output p 2)
    (let ((live-before (process-live-p p)))
      (delete-process p)
      (let ((process-count 0))
        (dolist (candidate (process-list))
          (when (processp candidate)
            (setq process-count (1+ process-count))))
        (list live-before
              (process-live-p p)
              process-count
              (memq p (process-list))))))
"##,
        expect,
    );
}
