//! Complex combo batch 24 — process output to narrowed buffer, write-region to
//! buffer, insert-file-contents under narrowing, char-table extra-slot on
//! standard tables, print-readably/read-eval, set-multibyte access patterns,
//! format %c encoding ranges, map-char-table range+parent.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx24_process_output_to_narrowed_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"PREoutput\\n\\nProcess neo-cx24-pn finished\\n\" \"PREoutput\\n\\nProcess neo-cx24-pn finished\\n-\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx24-pn*")))
  (with-current-buffer buf
    (insert "PRE-")
    (narrow-to-region 1 4))
  (let ((p (make-process :name "neo-cx24-pn" :command '("echo" "output")
                         :buffer buf)))
    (let ((attempts 0))
      (while (and (process-live-p p) (< attempts 100))
        (accept-process-output p 0.05)
        (setq attempts (1+ attempts)))
      (when (process-live-p p)
        (error "process did not exit")))
    (while (accept-process-output p 0.01)))
  (prog1 (with-current-buffer buf
           (let ((narrowed (buffer-string)))
             (widen)
             (list narrowed (buffer-string))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_insert_file_contents_under_narrowing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AAfile-contentB\" 3 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx24-if-")))
  (write-region "file-content" nil f nil 'silent)
  (prog1 (with-temp-buffer
           (insert "AAAABBBB")
           (narrow-to-region 3 6)
           (goto-char 5)
           (insert-file-contents f)
           (list (buffer-string) (point-min) (point-max)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_char_table_extra_slot_standard_syntax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194304 args-out-of-range args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (standard-syntax-table)))
  (list (length st)
        (condition-case e (char-table-extra-slot st 0) (error (car e)))
        (condition-case e (char-table-extra-slot st 1) (error (car e)))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_read_eval_interactions() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((a b c) invalid-read-syntax invalid-read-syntax)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((read-circle t) (read-circle nil))
  (list (car (read-from-string "(a b c)"))
        (condition-case e (car (read-from-string "#.(+ 1 2)")) (error (car e)))
        (let ((read-eval nil))
          (condition-case e (car (read-from-string "#.(+ 1 2)")) (error (car e))))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_format_c_encoding_range_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (128 127 255 256 4194303 57344 63743)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (aref (format "%c" #x80) 0)
      (aref (format "%c" #x7F) 0)
      (aref (format "%c" #xFF) 0)
      (aref (format "%c" #x100) 0)
      (aref (format "%c" #x3FFFFF) 0)
      (aref (format "%c" #xE000) 0)
      (aref (format "%c" #xF8FF) 0))
"##,
        expect,
    );
}

#[test]
fn div_cx24_set_multibyte_aref_buffer_string_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 4194208 4194209 4194248 eight-bit eight-bit)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 160 161 200 201 202 65))
  (set-buffer-multibyte t)
  (let* ((bs (buffer-string))
         (a0 (aref bs 0)) (a1 (aref bs 1)) (a2 (aref bs 2)))
    (list (length bs) a0 a1 a2
          (char-charset a0) (char-charset a2))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_map_char_table_range_with_parent_filtered() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((99 . :child-override) (122 . :parent-single))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-char-table 'cx24 nil)) (c (make-char-table 'cx24 nil))
      (ranges nil))
  (set-char-table-range p '(?a . ?e) :parent-range)
  (aset p ?z :parent-single)
  (set-char-table-parent c p)
  (aset c ?c :child-override)
  (map-char-table (lambda (k v)
                     (when (and (integerp k) v)
                       (push (cons k v) ranges)))
                   c)
  (sort ranges (lambda (a b) (< (car a) (car b)))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_coding_system_decode_then_char_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((1 1) 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((d (decode-coding-string (unibyte-string 195 169 226 130 172) 'utf-8)))
  (list (mapcar #'char-width (append d nil))
        (string-width d)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_overlay_before_string_with_display_glyph_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 13 \"23456789AB\" nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 5 8)))
    (overlay-put ov 'before-string (propertize ">>" 'display "XXXX"))
    (overlay-put ov 'face 'bold))
  (narrow-to-region 3 13)
  (goto-char 6)
  (list (point-min) (point-max)
        (buffer-string)
        (get-char-property 4 'face)
        (length (overlays-in (point-min) (point-max)))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_cl_defgeneric_argument_precedence() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK :b-first""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx24-cls-a () ())
  (defclass neo-cx24-cls-b () ())
  (let (log)
    (cl-defgeneric neo-cx24-fn (a b)
      (:argument-precedence-order b a))
    (cl-defmethod neo-cx24-fn ((a neo-cx24-cls-a) b)
      (push :a-first log))
    (cl-defmethod neo-cx24-fn (a (b neo-cx24-cls-b))
      (push :b-first log))
    (neo-cx24-fn (neo-cx24-cls-a) (neo-cx24-cls-b))
    (car log)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_buffer_local_variable_persistence_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (defvar neo-cx24-bl 0)
  (setq-local neo-cx24-bl 42)
  (undo-boundary)
  (insert "hello")
  (let ((v1 neo-cx24-bl))
    (undo)
    (list v1 neo-cx24-bl (local-variable-p 'neo-cx24-bl))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_print_readably_default() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"(lambda (x) (* x 2))\" \"(lambda (x) (* x 2))\" \"#:g0\" \"#s(hash-table test eq)\")""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (prin1-to-string '(lambda (x) (* x 2)))
      (let ((print-quoted t)) (prin1-to-string '(lambda (x) (* x 2))))
      (let ((print-gensym t)) (prin1-to-string (gensym)))
      (prin1-to-string (make-hash-table :test 'eq)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_process_send_region_narrowed() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"send-meB\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (got)
  (with-temp-buffer
    (insert "AAAAsend-meBBBB")
    (narrow-to-region 5 13)
    (let ((p (make-process :name "neo-cx24-sr" :command '("cat")
                           :buffer nil :connection-type 'pipe
                           :filter (lambda (proc str) (push str got)))))
      (process-send-region p (point-min) (point-max))
      (process-send-eof p)
      (accept-process-output p 1)))
  (apply #'concat (nreverse got)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_decode_coding_string_coding_detection() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 iso-latin-1 emacs-mule in-is13194-devanagari utf-8-auto utf-8-with-signature japanese-shift-jis chinese-big5 iso-2022-8bit-ss2) \"\u{feff}café\" (65279 99 97 102 233))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 239 187 191 99 97 102 195 169)))
  (list (detect-coding-string raw)
        (decode-coding-string raw 'undecided)
        (append (decode-coding-string raw 'undecided) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_text_property_sticky_after_multiple_inserts() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil italic italic 18)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AAAAABBBBBCCCCC")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (put-text-property 11 15 'face 'underline)
  (goto-char 5) (insert "X")
  (goto-char 8) (insert "Y")
  (goto-char 11) (insert "Z")
  (list (get-text-property 5 'face)
        (get-text-property 6 'face)
        (get-text-property 9 'face)
        (get-text-property 12 'face)
        (length (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx24_cl_loop_maximize_minimize_into() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-loop for x in '(3 1 4 1 5 9 2 6) maximize x into max
               minimize x into min
               finally (return (list max min)))
      (cl-loop for x in '(3 1 4 1 5 9 2 6) count (cl-oddp x))
      (cl-loop for x across [10 20 30] sum x))
"##,
        expect,
    );
}

#[test]
fn div_cx24_string_bytes_vs_length_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((strs '("ascii" "café" "世界" "😀" "aéb"c" "mixéd中文😀")))
  (mapcar (lambda (s) (list (length s) (string-bytes s))) strs))
"##,
        expect,
    );
}

#[test]
fn div_cx24_overlay_invisible_narrow_buffer_substring_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"visible1\\nhidden1\\nhidden2\\nv\" 9 17 (invisible t)) \"visible1\\nhidden1\\nhidden2\\nv\" 4 nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "visible1\nhidden1\nhidden2\nvisible2\n")
  (put-text-property 10 18 'invisible t)
  (let ((ov (make-overlay 10 18))) (overlay-put ov 'face 'bold))
  (narrow-to-region 1 27)
  (list (buffer-string)
        (buffer-substring-no-properties (point-min) (point-max))
        (count-lines (point-min) (point-max))
        (get-char-property 3 'invisible)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_window_parameter_window_combination() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (:window-val ((neo-cx24-wp . :window-val)) nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((w (selected-window)))
  (set-window-parameter w 'neo-cx24-wp :window-val)
  (list (window-parameter w 'neo-cx24-wp)
        (window-parameters w)
        (window-combined-p w)
        (window-combined-p w 'vertical)))
"##,
        expect,
    );
}

#[test]
fn div_cx24_process_coding_system_round_trip_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"café世界😀\" 7 15 \"c330814a9a47c252806bdcf58f1a7365165dcd21e8c9e73eea2515e8721fc769\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((coding-system-for-read 'utf-8-unix))
    (call-process "printf" nil t nil "%s" "café世界😀"))
  (list (buffer-string)
        (length (buffer-string))
        (string-bytes (buffer-string))
        (secure-hash 'sha256 (buffer-string))))
"##,
        expect,
    );
}
