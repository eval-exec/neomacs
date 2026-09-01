//! Complex combo batch 13 — deeper process lifecycle, coding-system auto-detect,
//! buffer narrowing+marker+text-property undo stack, cl-loop with hash-table,
//! window configuration comparison, timer cancel race, overlay priority+invisible
//! interaction, read/eval of backquoted forms, char-fold + replacement chain,
//! process send+receive binary data, format "%c" modifier chars.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx13_process_send_receive_binary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 127 4194176 4194303 65)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (got)
  (let ((p (make-process :name "neo-cx13-bin" :command '("cat")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str got))))
        (data (unibyte-string 0 1 127 128 255 65)))
    (process-send-string p data)
    (process-send-eof p)
    (accept-process-output p 1))
  (append (apply #'concat (nreverse got)) nil))
"##,
        expect,
    );
}

#[test]
fn div_cx13_buffer_narrow_marker_undo_stack_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (let ((m1 (set-marker (make-marker) 4))
        (m2 (set-marker (make-marker) 12)))
    (put-text-property 1 8 'face 'bold)
    (put-text-property 9 16 'face 'italic)
    (narrow-to-region 3 14)
    (undo-boundary)
    (goto-char 7) (insert "X")
    (undo-boundary)
    (delete-region 5 10)
    (let ((state (list (point-min) (point-max) (marker-position m1) (marker-position m2)
                       (buffer-string) (text-properties-at 1) (text-properties-at 5))))
      (undo) (undo)
      (list state (point-min) (point-max)
            (marker-position m1) (marker-position m2)
            (buffer-string) (text-properties-at 1))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_cl_loop_hash_table_collect_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash 'b 2 ht) (puthash 'a 1 ht) (puthash 'd 4 ht) (puthash 'c 3 ht)
  (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                 collect (cons k v))
        (lambda (x y) (string< (symbol-name (car x)) (symbol-name (car y))))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_window_configuration_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((c1 (current-window-configuration)))
  (split-window)
  (let ((c2 (current-window-configuration)))
    (list (window-configuration-p c1) (window-configuration-p c2)
          (equal c1 c2)
          (progn (set-window-configuration c1) (count-windows)))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_timer_cancel_during_execution() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:fired nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (to-cancel fired)
  (setq to-cancel (run-with-timer 0 nil
                    (lambda () (setq fired :fired)
                      (cancel-timer to-cancel))))
  (sit-for 0.05)
  (list fired (memq to-cancel timer-list)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_overlay_priority_invisible_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t bold bold \"0123456\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((o1 (make-overlay 2 5)) (o2 (make-overlay 4 7)))
    (overlay-put o1 'priority 5) (overlay-put o2 'priority 1)
    (overlay-put o1 'invisible t)
    (overlay-put o2 'face 'bold)
    (list (get-char-property 3 'invisible)
          (get-char-property 4 'invisible)
          (get-char-property 5 'face)
          (get-char-property 6 'face)
          (buffer-substring 1 8))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_read_eval_backquote_unquote() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 10) (y 20))
  (list (eval (car (read-from-string "`(+ ,x ,y)")) t)
        (eval (car (read-from-string "`(a ,(+ x y) b)")) t)
        (eval (car (read-from-string "`(a ,@(list x y) b)")) t)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_char_fold_replace_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument sequencep 233)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café résumé naïve"))
  (replace-regexp-in-string (char-fold-to-regexp ?é) "E" s))
"##,
        expect,
    );
}

#[test]
fn div_cx13_coding_system_auto_detect_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx13-auto-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "café" nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'undecided))
             (insert-file-contents f))
           (list (buffer-string) (buffer-file-coding-system)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_format_c_modifier_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217825)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%c" 1)
      (format "%c" ?\C-a)
      (format "%c" ?\M-a)
      (format "%c" ?\C-\M-a)
      (format "%c" ?\S-a)
      (format "%c" 0))
"##,
        expect,
    );
}

#[test]
fn div_cx13_set_process_filter_receive_multiline() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (got)
  (let ((p (make-process :name "neo-cx13-ml" :command '("printf" "line1\\nline2\\nline3")
                         :buffer nil
                         :filter (lambda (proc str) (push str got))))
    (accept-process-output p 0.5))
  (apply #'concat (nreverse got)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_undo_boundary_gap_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument listp (((\"ABCD\" . \"AB\") . \"AB\")))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "A") (undo-boundary)
  (insert "B") (undo-boundary)
  (insert "C") (undo-boundary)
  (insert "D")
  (let ((s1 (buffer-string)))
    (undo) (setq s1 (cons s1 (buffer-string)))
    (undo) (setq s1 (cons s1 (buffer-string)))
    (undo) (setq s1 (cons s1 (buffer-string)))
    (nreverse s1)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_marker_point_eq_after_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 #<marker in no buffer> 5 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (goto-char 5)
  (let ((pt (point)) (mk (copy-marker (point))))
    (narrow-to-region 2 8)
    (list pt mk (point)
          (save-restriction (widen) (point))
          (marker-position mk))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_cl_typecase_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typecase)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typecase 5 (string :str) (integer :int) (t :other))
      (cl-typecase "x" (string :str) (integer :int) (t :other))
      (cl-typecase [1 2] (vector :vec) (t :other))
      (cl-typecase '(1 2) (cons :cons) (vector :vec) (t :other))
      (cl-typecase nil (null :null) (cons :cons) (t :other)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_overlay_after_string_with_props_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK ((face bold mouse-face highlight font-lock-face keyword) \"#(\\\"X\\\" 0 1 (face bold mouse-face highlight font-lock-face keyword))\" 88)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'after-string
                 (propertize "X" 'face 'bold 'mouse-face 'highlight 'font-lock-face 'keyword))
    (let ((as (overlay-get ov 'after-string)))
      (list (text-properties-at 0 as)
            (prin1-to-string as)
            (aref as 0)))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_conditional_search_replace_preserve_case() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"FOO BAR FOO BAR\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (with-temp-buffer
    (insert "Foo Bar FOO bar")
    (goto-char 1)
    (while (re-search-forward "foo\\|bar" nil t)
      (replace-match (upcase (match-string 0))))
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_buffer_modified_undo_modified_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello")
  (let ((m1 (buffer-modified-p)))
    (undo-boundary)
    (insert " world")
    (let ((m2 (buffer-modified-p)))
      (undo)
      (list m1 m2 (buffer-modified-p) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx13_process_adaptive_read_buffering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (void-function process-adaptive-read-buffering-p)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx13-arb" :command '("echo" "x")))
      (orig process-adaptive-read-buffering))
  (prog1 (list process-adaptive-read-buffering
               (progn (process-adaptive-read-buffering-p p)))
    (set-process-adaptive-read-buffering p nil)
    (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_decode_coding_string_latin1_vs_utf8_then_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((99 97 102 233) (99 97 102 195 169) (99 97 102 233) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((latin-bytes (unibyte-string 99 97 102 233))
       (from-latin1 (decode-coding-string latin-bytes 'latin-1))
       (utf8-bytes (encode-coding-string from-latin1 'utf-8))
       (from-utf8 (decode-coding-string utf8-bytes 'utf-8)))
  (list (append from-latin1 nil)
        (append utf8-bytes nil)
        (append from-utf8 nil)
        (equal from-latin1 from-utf8)))
"##,
        expect,
    );
}

#[test]
fn div_cx13_multiple_text_props_change_search_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 12 12 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 1 4 'face 'bold)
  (put-text-property 4 8 'mouse-face 'highlight)
  (put-text-property 8 12 'font-lock-face 'keyword)
  (put-text-property 12 16 'invisible t)
  (list (next-property-change 1)
        (next-single-property-change 1 'face)
        (next-single-property-change 1 'invisible)
        (previous-property-change 16)
        (text-property-any 1 16 'invisible t)))
"##,
        expect,
    );
}
