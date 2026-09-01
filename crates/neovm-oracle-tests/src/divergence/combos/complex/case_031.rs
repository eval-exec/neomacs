//! Complex combo batch 31 — fresh edges: hash-table read-back, char-table
//! printing, print-numbering, default-text-properties, char-fold-suffix,
//! map/seq extensions, window-vscroll/fringes, modifier-bit format.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx31_read_back_printed_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 2 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((ht (make-hash-table :test 'equal))
       (_ (puthash "a" 1 ht))
       (_ (puthash "b" 2 ht))
       (p (prin1-to-string ht))
       (back (car (read-from-string p))))
  (list (hash-table-p back)
        (hash-table-count back)
        (gethash "a" back)
        (gethash "b" back)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_print_char_table_to_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 11 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx31 nil)))
  (aset ct ?a :val-a)
  (let ((p (prin1-to-string ct)))
    (list (string-match "#\\^" p)
          (string-match "cx31" p)
          (> (length p) 10))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_print_continuous_numbering() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"(1 2 3)\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-circle t) (print-continuous-numbering t))
  (let ((s1 (prin1-to-string '(1 2 3)))
        (x (list 1)))
    (setcdr x x)
    (let ((s2 (prin1-to-string x)))
      (list s1 (string-match "#1=" s2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_default_text_properties_extraction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((default-text-properties '(face default)))
    (insert "abcdef")
    (put-text-property 2 4 'mouse-face 'highlight))
  (list (text-properties-at 0)
        (text-properties-at 1)
        (text-properties-at 2)
        (text-properties-at 5)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_char_fold_suffix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:errored wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (let ((char-fold-suffix t))
            (string-match (char-fold-to-regexp ?e) "caféx"))
          (let ((search-default-mode nil))
            (string-match (char-fold-to-regexp ?a) "abc")))
  (void-variable (list :not-available))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_map_keys_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a b c) (1 2 3) 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'map)
  (let ((al '((a . 1) (b . 2) (c . 3))))
    (list (sort (map-keys al) #'string<)
          (map-values al)
          (map-length al))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_seq_into_conversions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ([1 2 3] (1 2 3) (97 98 99))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn (require 'seq)
  (list (seq-into '(1 2 3) 'vector)
        (seq-into [1 2 3] 'list)
        (seq-into "abc" 'list)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_window_vscroll() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((orig (window-vscroll)))
  (set-window-vscroll (selected-window) 3)
  (let ((after (window-vscroll)))
    (set-window-vscroll (selected-window) 0)
    (list orig after (window-vscroll))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_set_window_fringes() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((orig (window-fringes)))
      (set-window-fringes (selected-window) 8 8 nil)
      (let ((after (window-fringes)))
        (apply #'set-window-fringes (selected-window) orig)
        (list (consp orig) (consp after))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_format_c_modifier_bit_ranges() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\u{40061}\" wrong-type-argument wrong-type-argument wrong-type-argument wrong-type-argument)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (format "%c" (+ ?a 262144)) (error (car e)))
      (condition-case e (format "%c" (+ ?a 134217728)) (error (car e)))
      (condition-case e (format "%c" (+ ?a 262144 134217728)) (error (car e)))
      (condition-case e (format "%c" (+ ?a 67108864)) (error (car e)))
      (condition-case e (format "%c" (+ ?a 536870912)) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_cl_defgeneric_method_combination_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Unsupported qualifiers in function neo-cx31-fn: (+)\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx31-base () ())
  (let (log)
    (cl-defgeneric neo-cx31-fn (obj) (:method-combination +))
    (cl-defmethod neo-cx31-fn + ((obj neo-cx31-base)) 1)
    (cl-defmethod neo-cx31-fn + ((obj neo-cx31-base)) 2)
    (neo-cx31-fn (neo-cx31-base))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_coding_system_define_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (errored . void-function)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (coding-system-define-aliases 'neo-cx31-alias 'utf-8)
      (list (coding-system-p 'neo-cx31-alias)
            (eq (coding-system-base 'neo-cx31-alias)
                (coding-system-base 'utf-8))))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_set_buffer_multibyte_then_char_table_range_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 32 119)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (let ((syntax-before (char-syntax ?\x4e16)))
    (set-buffer-multibyte nil)
    (let ((syntax-after (char-syntax ?\x4e16)))
      (set-buffer-multibyte t)
      (list syntax-before syntax-after (char-syntax ?\x4e16)))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_process_output_to_buffer_with_text_properties() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx31-ot*")))
  (with-current-buffer buf
    (insert "PRE:")
    (put-text-property 1 4 'face 'bold))
  (let ((p (make-process :name "neo-cx31-ot" :command '("echo" "output")
                         :buffer buf)))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (list (buffer-string)
                 (text-properties-at 0)
                 (text-properties-at 3)
                 (text-properties-at 4)))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_overlay_display_space_spec_column_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (put-text-property 2 4 'display '(space :width 5))
  (list (current-column)
        (string-width (buffer-substring 1 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_cl_coerce_from_hash_table_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK void-function""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table)))
  (condition-case e (cl-coerce ht 'list) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_coding_system_base_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (utf-8 utf-8 utf-8-with-signature iso-latin-1 (utf-8-unix mule-utf-8-unix cp65001-unix))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-base 'utf-8-unix)
      (coding-system-base 'utf-8-dos)
      (coding-system-base 'utf-8-with-signature)
      (coding-system-base 'latin-1-unix)
      (coding-system-aliases 'utf-8-unix))
"##,
        expect,
    );
}

#[test]
fn div_cx31_undo_after_multiple_set_text_properties_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil (mouse-face nil face bold) (mouse-face nil)) (face bold) (mouse-face nil face bold) (mouse-face nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (undo-boundary)
  (set-text-properties 1 4 '(face bold))
  (undo-boundary)
  (add-text-properties 2 6 '(mouse-face highlight))
  (undo-boundary)
  (remove-text-properties 1 3 '(face))
  (let ((state (list (text-properties-at 1) (text-properties-at 3) (text-properties-at 5))))
    (undo)
    (list state (text-properties-at 1) (text-properties-at 3) (text-properties-at 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx31_buffer_hash_vs_secure_hash_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"35fa158495d259492bc1495b5d29e842226ed7be\" \"d00f6ed04f1e898e4158850a30ea936c\" \"15bbe85aac4518db7da507997bd8b9baa07ddea5d0a08d098f85f1bf08c02521\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx31-hb*")))
  (with-current-buffer buf (erase-buffer) (insert "identical content"))
  (prog1 (list (with-current-buffer buf (buffer-hash))
               (with-current-buffer buf (secure-hash 'md5 (buffer-string)))
               (with-current-buffer buf (secure-hash 'sha256 (buffer-string))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_format_percent_c_then_aref_back() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"A\" \"é\" \"あ\" \"😀\" \"\u{e0a0}\") (65 233 12354 128512 57504) t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((codepoints (list 65 233 #x3042 #x1f600 #xe0a0))
       (strings (mapcar (lambda (cp) (format "%c" cp)) codepoints))
       (back (mapcar (lambda (s) (aref s 0)) strings)))
  (list strings back (equal codepoints back)))
"##,
        expect,
    );
}

#[test]
fn div_cx31_coding_system_for_write_inhibit_eol_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"line1\\nline2\\n\" 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx31-eol-")))
  (let ((coding-system-for-write 'utf-8-dos)
        (select-safe-coding-system-allow-other-codings t))
    (write-region "line1\nline2\n" nil f nil 'silent))
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read 'utf-8-dos))
             (insert-file-contents f))
           (list (buffer-string) (string-bytes (buffer-string))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}
