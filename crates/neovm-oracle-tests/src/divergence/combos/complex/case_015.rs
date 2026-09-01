//! Complex combo batch 15 — extend secure-hash file divergence, with-temp-message
//! format, define-coding-system hooks; plus new edges: char-table-decode-char
//! per charset, read/print of circular compiled lambda, hash-table-test
//! custom, set-transient-map, window-scroll-functions, process-buffer-sentinel
//! interaction, cl-defmethod combination (:after across hierarchy), buffer
//! formatting (center-line), face-remap-window-local.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx15_secure_hash_file_various_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"979900ab4e372ab39cb7ec10869c26bf\" \"979900ab4e372ab39cb7ec10869c26bf\" \"979900ab4e372ab39cb7ec10869c26bf\" \"979900ab4e372ab39cb7ec10869c26bf\" \"979900ab4e372ab39cb7ec10869c26bf\" \"979900ab4e372ab39cb7ec10869c26bf\") (\"636ef9a74136a637d69c870b7eb3256c\" \"07117fe4a1ebd544965dc19573183da2\" \"c086b3008aca0efa8f2ded065d6afb50\" \"4fcc82a88ee38e0aa16c17f512c685c9\" \"d41d8cd98f00b204e9800998ecf8427e\" \"0cc175b9c0f1b6a831c399e269772661\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pathname "/tmp/neo-cx15-sh-fixed")
      (contents '("ascii only" "café" "世界" "line1\nline2\n" "" "a")))
  (list (mapcar (lambda (_c) (secure-hash 'md5 pathname)) contents)
        (mapcar (lambda (c) (secure-hash 'md5 c)) contents)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_with_temp_message_formats() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (msg1 msg2)
  (with-temp-message "formatted: %d %s" 42 "hello"
    (setq msg1 (current-message)))
  (with-temp-message (format "pre-formatted: %d" 99)
    (setq msg2 (current-message)))
  (list msg1 msg2))
"##,
        expect,
    );
}

#[test]
fn div_cx15_define_coding_system_uninterned() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"x-ucs\" 85)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-coding-system 'neo-cx15-ucs "Test UCS"
        :coding-type 'utf-8 :mnemonic ?U :charset-list '(unicode)
        :mime-charset "x-ucs")
      (list (coding-system-p 'neo-cx15-ucs)
            (coding-system-get 'neo-cx15-ucs :mime-charset)
            (coding-system-mnemonic 'neo-cx15-ucs)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_hash_table_custom_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Invalid hash table test\" equal-including-properties)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal-including-properties)))
  (puthash (propertize "key" 'face 'bold) 1 ht)
  (puthash (propertize "key" 'face 'italic) 2 ht)
  (list (hash-table-count ht)
        (gethash (propertize "key" 'face 'bold) ht)
        (gethash (propertize "key" 'face 'italic) ht)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_set_transient_map_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-sparse-keymap)))
  (define-key m "a" 'transient-action)
  (condition-case e
      (progn
        (set-transient-map m t)
        (list (key-binding "a" t)
              (lookup-key m "a")))
    (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_process_buffer_sentinel_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"pre:data\\n\" (:sentinel . \"finished\\n\"))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx15-pbs*"))
      (sentinel-data nil))
  (with-current-buffer buf (insert "pre:"))
  (let ((p (make-process :name "neo-cx15-pbs" :command '("echo" "data")
                         :buffer buf
                         :sentinel (lambda (proc event)
                                     (push (cons :sentinel event) sentinel-data)))))
    (let ((deadline (+ (float-time) 2.0)))
      (while (and (< (float-time) deadline)
                  (not sentinel-data))
        (accept-process-output p 0.05))))
  (list (with-current-buffer buf (buffer-string))
        (if sentinel-data (car sentinel-data) :none)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_cl_defmethod_after_hierarchy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx15-base () ())
  (defclass neo-cx15-sub (neo-cx15-base) ())
  (let (log)
    (cl-defgeneric neo-cx15-fn (obj))
    (cl-defmethod neo-cx15-fn ((obj neo-cx15-base))
      (push :base-primary log) :base)
    (cl-defmethod neo-cx15-fn :after ((obj neo-cx15-sub))
      (push :sub-after log))
    (let ((r (neo-cx15-fn (neo-cx15-sub))))
      (list r (nreverse log))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_center_line_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"café 世界\\n\t\t    \"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((fill-column 40))
    (insert "café 世界\n")
    (center-line)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_face_remap_window_local_effect() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 1 :removed)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((cookie (face-remap-add-relative 'default :height 2.0)))
    (list (consp cookie)
          (face-attribute 'default :height)
          (progn (face-remap-remove-relative cookie) :removed))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_window_scroll_functions_hook() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (fired)
  (add-hook 'window-scroll-functions
            (lambda (win start) (push :scrolled fired)))
  (remove-hook 'window-scroll-functions
               (car window-scroll-functions))
  (list (null window-scroll-functions) fired))
"##,
        expect,
    );
}

#[test]
fn div_cx15_print_circle_compiled_function_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((f (byte-compile (lambda (x) (* x 2))))
       (print-circle t))
  (string-match "#[0-9]" (prin1-to-string (vector f f))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_encode_char_charset_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (9250 12354 97 97)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((c-jis (make-char 'japanese-jisx0208 36 34))
      (c-ascii ?a))
  (list (encode-char c-jis 'japanese-jisx0208)
        (encode-char c-jis 'unicode)
        (encode-char c-ascii 'ascii)
        (encode-char c-ascii 'unicode)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_decode_coding_string_then_char_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((99 97 102 233) (ascii ascii ascii unicode-bmp))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (decode-coding-string (unibyte-string 99 97 102 233) 'latin-1)))
  (list (append d nil)
        (mapcar #'char-charset (append d nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_undo_boundary_buffer_undo_list_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-if-not)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello")
  (undo-boundary)
  (insert " world")
  (let ((entries (cl-remove-if-not #'consp buffer-undo-list)))
    (list (length entries)
          (car (car entries))
          (eq (car buffer-undo-list) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_marker_insertion_type_undo_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 4 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((m1 (set-marker (make-marker) 3))
        (m2 (set-marker (make-marker) 3)))
    (set-marker-insertion-type m2 t)
    (undo-boundary)
    (goto-char 3) (insert "X")
    (let ((a1 (marker-position m1)) (a2 (marker-position m2)))
      (undo)
      (list a1 a2 (marker-position m1) (marker-position m2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_cl_defstruct_inheritance_printers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx15-animal name sound)
  (cl-defstruct (neo-cx15-dog (:include neo-cx15-animal)) breed)
  (let ((d (make-neo-cx15-dog :name "Rex" :sound "Woof" :breed "Lab")))
    (list (neo-cx15-animal-name d)
          (neo-cx15-animal-sound d)
          (neo-cx15-dog-breed d)
          (neo-cx15-dog-p d)
          (neo-cx15-animal-p d))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_overlay_priority_face_precedence_with_invisible() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (let ((o1 (make-overlay 1 5)) (o2 (make-overlay 3 7)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o1 'priority 1)
    (overlay-put o2 'priority 10)
    (overlay-put o1 'invisible t)
    (list (get-char-property 1 'face)
          (get-char-property 3 'face)
          (get-char-property 1 'invisible)
          (get-char-property 3 'invisible))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_process_environment_nested_let_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"inner-value\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((result (shell-command-to-string "echo $NEO_CX15_TEST"))
       (inner (let ((process-environment (cons "NEO_CX15_TEST=inner-value" process-environment)))
                (shell-command-to-string "echo $NEO_CX15_TEST"))))
  (list (string-trim result) (string-trim inner)))
"##,
        expect,
    );
}

#[test]
fn div_cx15_narrow_undo_delete_insert_text_property_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAAAABBBBBCCCCC")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (put-text-property 11 15 'face 'underline)
  (narrow-to-region 3 13)
  (undo-boundary)
  (goto-char 6) (delete-char 3)
  (let ((after-delete (list (buffer-string) (text-properties-at 1) (text-properties-at 4))))
    (undo)
    (widen)
    (list after-delete (buffer-string)
          (text-properties-at 1) (text-properties-at 5) (text-properties-at 10))))
"##,
        expect,
    );
}

#[test]
fn div_cx15_format_escape_multibyte_props_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (#(\"café世界\" 0 6 (face bold)) (face bold) 27 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café世界" 'face 'bold))
       (fmt1 (format "%s" s))
       (fmt2 (format "%S" s))
       (fmt3 (format "%25s|" s)))
  (list fmt1 (text-properties-at 0 fmt1) (length fmt2) (length fmt3)))
"##,
        expect,
    );
}
