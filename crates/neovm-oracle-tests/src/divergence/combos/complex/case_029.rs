//! Complex combo batch 29 — reader #-dispatch edge cases + remaining interactions.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx29_reader_hash_dispatch_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (invalid-read-syntax invalid-read-syntax invalid-read-syntax nil end-of-file #'sym sym (a))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "#+")) (error (car e)))
      (condition-case e (car (read-from-string "#-")) (error (car e)))
      (condition-case e (car (read-from-string "#~")) (error (car e)))
      (condition-case e (car (read-from-string "#$")) (error (car e)))
      (condition-case e (car (read-from-string "#@5abcdef")) (error (car e)))
      (condition-case e (car (read-from-string "#'sym")) (error (car e)))
      (condition-case e (car (read-from-string "#:sym")) (error (car e)))
      (condition-case e (car (read-from-string "#1=(a) #1#")) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_reader_backquote_complex() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-variable x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((x 10) (y 20) (lst '(1 2 3)))
  (list (eval (car (read-from-string "`(,x ,y)")) t)
        (eval (car (read-from-string "`(,x ,@lst ,y)")) t)
        (eval (car (read-from-string "`(,x (,@lst) ,y)")) t)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_format_spec_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((spec (format-spec-make ?a "café" ?b "世界" ?% "percent")))
      (list (format-spec "%a" spec)
            (format-spec "literal %% in spec" spec)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_char_table_predicate() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-table-p 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx29 nil)))
  (set-char-table-range ct ?a :yes)
  (set-char-table-range ct ?b nil)
  (list (char-table-p ct ?a)
        (char-table-p ct ?b)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_category_set_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments make-category-set 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((ct (standard-category-table))
       (cs1 (make-category-set "a" ct))
       (cs2 (make-category-set "b" ct)))
  (list (category-set-mnemonics cs1)
        (category-set-mnemonics cs2)
        (condition-case e (char-in-category-p ?a ?a ct) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_window_hscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 5 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig (window-hscroll)))
  (set-window-hscroll (selected-window) 5)
  (let ((after (window-hscroll)))
    (set-window-hscroll (selected-window) 0)
    (list orig after (window-hscroll))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_format_c_negative_and_large() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (wrong-type-argument \"\\0\" \"ÿ\" \"Ā\" \"\\377\" wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (format "%c" -1) (error (car e)))
      (condition-case e (format "%c" 0) (error (car e)))
      (format "%c" 255)
      (format "%c" 256)
      (format "%c" #x3FFFFF)
      (condition-case e (format "%c" #x400000) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_coding_system_translation_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :decode-translate-table)
      (coding-system-get 'utf-8 :encode-translate-table)
      (coding-system-get 'latin-1 :decode-translate-table)
      (condition-case e (coding-system-get 'iso-8859-7 :decode-translate-table) (error (cons 'err (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_process_send_string_empty_then_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"actual-data\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (got)
  (let ((p (make-process :name "neo-cx29-es" :command '("cat")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str got)))))
    (process-send-string p "")
    (process-send-string p "actual-data")
    (process-send-eof p)
    (accept-process-output p 1))
  (apply #'concat (nreverse got)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_set_buffer_multibyte_nil_then_marker() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 4 \"café世界\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((m (set-marker (make-marker) 4)))
    (set-buffer-multibyte nil)
    (let ((mpos-nil (marker-position m)))
      (set-buffer-multibyte t)
      (list mpos-nil (marker-position m) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_cl_defstruct_with_predicate_and_copier_named() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct (neo-cx29-box (:predicate neo-cx29-box-p?)
                              (:copier neo-cx29-copy-box)
                              (:constructor neo-cx29-new-box))
    (val 0 :read-only t))
  (let ((b (neo-cx29-new-box :val 42)))
    (list (neo-cx29-box-val b)
          (neo-cx29-box-p? b)
          (let ((c (neo-cx29-copy-box b)))
            (neo-cx29-box-val c))
          (condition-case e (setf (neo-cx29-box-val b) 99) (error (car e))))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_overlay_display_integer_height_no_width_change() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display '(height 3.0))
  (list (current-column)
        (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_decode_encode_region_no_conversion_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t (0 127 128 200 255 65))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 0 127 128 200 255 65))
  (let ((orig (buffer-string)))
    (encode-coding-region (point-min) (point-max) 'no-conversion)
    (decode-coding-region (point-min) (point-max) 'no-conversion)
    (list (equal orig (buffer-string))
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_cl_loop_destructuring_bind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(cl-loop for (a b) in '((1 2) (3 4) (5 6))
         collect (+ a b))
"##,
        expect,
    );
}

#[test]
fn div_cx29_buffer_hash_after_undo_match_original() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (let ((h1 (buffer-hash)))
    (goto-char 6) (insert "X")
    (undo)
    (list h1 (buffer-hash) (equal h1 (buffer-hash)))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_prin1_of_string_with_text_properties_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold 'mouse-face 'highlight))
       (print-circle t)
       (p (prin1-to-string (list s s s))
       (back (car (read-from-string p))))
  (list (aref back 0)
        (eq (aref back 0) (aref back 1))
        (eq (aref back 0) (aref back 2))
        (text-properties-at 0 (aref back 0))))
"##,
        expect,
    );
}

#[test]
fn div_cx29_set_window_start_then_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (15 71 #<buffer *scratch*>)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx29-ws*")))
  (with-current-buffer buf
    (dotimes (i 10) (insert (format "line %d\n" i))))
  (set-window-buffer (selected-window) buf)
  (set-window-start (selected-window) 15)
  (prog1 (list (window-start) (window-point) (current-buffer))
    (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_text_property_any_across_overlay_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 1 8 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 1 8 'face 'bold)
  (let ((ov (make-overlay 6 10))) (overlay-put ov 'face 'italic))
  (list (text-property-any 1 16 'face 'bold)
        (text-property-not-all 1 16 'face nil)
        (next-property-change 5)
        (next-single-property-change 5 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_string_lessp_raw_bytes_multibyte_mixed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((a (concat "x" (string-make-multibyte (unibyte-string 200))))
       (b (concat "x" (string-make-multibyte (unibyte-string 201))))
       (c "x"))
  (list (string-lessp a b) (string-lessp c a) (string-lessp a c)))
"##,
        expect,
    );
}

#[test]
fn div_cx29_coding_system_unibyte_alias_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t raw-text raw-text)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-p 'no-conversion)
      (coding-system-p 'raw-text)
      (coding-system-p 'binary)
      (coding-system-type 'no-conversion)
      (coding-system-type 'raw-text))
"##,
        expect,
    );
}
