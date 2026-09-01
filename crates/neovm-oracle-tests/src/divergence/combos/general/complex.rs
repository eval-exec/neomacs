//! Complex combo divergence probes — multi-feature interaction edges.
//!
//! Each test combines 3+ features to surface divergences that focused tests
//! miss: set-buffer-multibyte + narrowing + markers, case-fold search + replace
//! over multibyte, process output + filter + multibyte insertion, file coding
//! round-trips, overlay stacks + editing + undo, text-property stickiness across
//! undo, font-lock + overlay precedence, composition + regex, cl-loop + hash.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx_set_buf_multibyte_narrow_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Changing multibyteness in a narrowed buffer\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界x")
  (let ((m (set-marker (make-marker) 3)))
    (narrow-to-region 2 6)
    (set-buffer-multibyte nil)
    (set-buffer-multibyte t)
    (list (point-min) (point-max) (marker-position m) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx_case_fold_replace_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abcXdef\"""#]];
    // case-fold σ should match Σ — surfaces the case-fold CF/D1 bug via replace.
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (replace-regexp-in-string "σ" "X" "abcΣdef"))
"##,
        expect,
    );
}

#[test]
fn div_cx_case_fold_search_cyrillic_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abcQdef\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (replace-regexp-in-string "я" "Q" "abcЯdef"))
"##,
        expect,
    );
}

#[test]
fn div_cx_write_region_coding_read_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é中😀\" 9 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cc-"))
      (s (string #xe9 #x4e2d #x1f600)))
  (let ((coding-system-for-write 'utf-8-unix)) (write-region s nil f nil 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix)) (insert-file-contents f))
           (list (buffer-string) (string-bytes (buffer-string)) (length (buffer-string))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx_process_filter_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café世界\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (received)
  (let ((proc (make-process :name "neo-pc" :command '("echo" "café世界")
                            :connection-type 'pipe :buffer nil
                            :filter (lambda (p s) (push s received)))))
    (accept-process-output proc 2))
  (apply #'concat (nreverse received)))
"##,
        expect,
    );
}

#[test]
fn div_cx_text_prop_stickiness_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAAAABBBBB")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (goto-char 5)
  (insert "X")
  (let ((p1 (list (get-text-property 5 'face) (get-text-property 6 'face))))
    (undo)
    (list p1 (get-text-property 5 'face) (get-text-property 6 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx_overlay_stack_edit_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "café世界")
  (let ((o1 (make-overlay 1 4)) (o2 (make-overlay 3 5)) (o3 (make-overlay 5 7)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 3)
    (overlay-put o3 'priority 2)
    (overlay-put o1 'before-string ">>")
    (overlay-put o3 'after-string "<<")
    (goto-char 3)
    (insert "X")
    (let ((s1 (overlay-start o1)) (e1 (overlay-end o1))
          (s2 (overlay-start o2)) (e2 (overlay-end o2)))
      (undo)
      (list s1 e1 s2 e2 (overlay-start o1) (overlay-end o1)
            (overlay-start o2) (overlay-end o2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx_buf_substring_props_print_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (face bold) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café")
  (put-text-property 1 3 'face 'bold)
  (let* ((sub (buffer-substring 1 4))
         (printed (prin1-to-string sub))
         (back (car (read-from-string printed))))
    (list (text-properties-at 0 sub) (text-properties-at 0 back) (equal sub back))))
"##,
        expect,
    );
}

#[test]
fn div_cx_format_high_codepoint_concat_aref() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (128512 128512 2 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((c1 (format "%c" #x1f600))
       (c2 (char-to-string #x1f600))
       (cat (concat c1 c2)))
  (list (aref cat 0) (aref cat 1) (length cat) (string-bytes cat)))
"##,
        expect,
    );
}

#[test]
fn div_cx_set_multibyte_raw_bytes_position() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 3 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65))
  (let ((b1 (position-bytes 1)))
    (set-buffer-multibyte t)
    (list b1 (position-bytes 1) (length (buffer-string)) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_cx_narrow_overlay_textprop_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 3 6 'face 'bold)
  (let ((ov (make-overlay 4 7))) (overlay-put ov 'face 'italic))
  (narrow-to-region 3 8)
  (let ((tp (get-text-property 1 'face))
        (cp (get-char-property 2 'face))
        (found (text-property-any (point-min) (point-max) 'face 'bold)))
    (list tp cp (if found t nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx_font_lock_overlay_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (font-lock-keyword-face highlight highlight)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'font-lock)
  (with-temp-buffer
    (emacs-lisp-mode)
    (insert "(defun foo (x) (* x 2))")
    (font-lock-fontify-buffer)
    (let ((ov (make-overlay 3 7)))
      (overlay-put ov 'face 'highlight)
      (list (get-text-property 3 'face)
            (get-char-property 3 'face)
            (get-char-property 5 'face)))))
"##,
        expect,
    );
}

#[test]
fn div_cx_compose_regex_char_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 128104 8205 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc\U0001F468‍\U0001F469‍\U0001F467def")
  (compose-region 4 7 "")
  (goto-char 1)
  (let ((m (re-search-forward "\U0001F468" nil t)))
    (list (if m t nil) (char-after 4) (char-after 5) (point))))
"##,
        expect,
    );
}

#[test]
fn div_cx_eieio_method_advice_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 11""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx-cls () ((x :initarg :x)))
  (cl-defgeneric neo-cx-fn (obj))
  (cl-defmethod neo-cx-fn ((obj neo-cx-cls)) (* (oref obj x) 2))
  (advice-add 'neo-cx-fn :around (lambda (fn &rest a) (1+ (apply fn a))))
  (let ((o (neo-cx-cls :x 5)))
    (condition-case e (neo-cx-fn o) (error (cons 'err (car e))))))
"##,
        expect,
    );
}

#[test]
fn div_cx_upcase_replace_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"CAFÉ RÉSUMÉ STRASSE\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "Café RÉSUMÉ straße")
  (goto-char 1)
  (while (re-search-forward "[a-zéàüß]+" nil t)
    (replace-match (upcase (match-string 0))))
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx_write_append_coding_read_offset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-wa-")))
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region "café" nil f nil 0)
    (write-region "世界" nil f 'append 0))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-unix))
             (insert-file-contents f nil 0 5))
           (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx_invisible_narrow_substring_lines() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "vis1\ninvis1\ninvis2\nvis2\n")
  (put-text-property 6 13 'invisible t)
  (narrow-to-region 1 19)
  (list (count-lines (point-min) (point-max))
        (length (buffer-substring-no-properties 1 19))))
"##,
        expect,
    );
}

#[test]
fn div_cx_undo_text_prop_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 (mouse-face nil face nil) (face nil) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((m (set-marker (make-marker) 3)))
    (undo-boundary)
    (set-text-properties 1 3 '(face bold))
    (undo-boundary)
    (put-text-property 1 2 'mouse-face 'highlight)
    (let ((ov (make-overlay 2 4))) (overlay-put ov 'face 'italic))
    (undo)
    (list (marker-position m)
          (text-properties-at 1) (text-properties-at 2)
          (length (overlays-at 2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx_cl_loop_hash_filter() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table)))
  (dotimes (i 5) (puthash i (* i i) ht))
  (cl-loop for k being the hash-keys of ht using (hash-values v)
           when (> v 4) collect (cons k v) into big
           else collect k into small
           finally (return (list (sort big (lambda (a b) (< (car a) (car b))))
                                 (sort small #'<)))))
"##,
        expect,
    );
}

#[test]
fn div_cx_multi_coding_decode_compare() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café\" #(\"cafÃ©\" 0 5 (charset iso-8859-1)) \"café\" (99 97 102 233))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((bytes (unibyte-string 99 97 102 195 169)))
  (list (decode-coding-string bytes 'utf-8)
        (decode-coding-string bytes 'latin-1)
        (decode-coding-string bytes 'utf-8-unix)
        (append (decode-coding-string bytes 'utf-8) nil)))
"##,
        expect,
    );
}
