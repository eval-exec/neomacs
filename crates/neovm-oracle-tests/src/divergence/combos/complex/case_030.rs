//! Complex combo batch 30 (MILESTONE) — reader read-syntax deep, modifier
//! chars, key-binding interaction, nested hash-table print.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx30_reader_position_past_end() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a) 3 (b) 7 args-out-of-range)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((input "(a) (b)"))
  (let ((r1 (read-from-string input 0)))
    (let ((r2 (read-from-string input (cdr r1))))
      (list (car r1) (cdr r1) (car r2) (cdr r2)
            (condition-case e (read-from-string input 100) (error (car e)))))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_multiple_shared_refs() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((x (car (read-from-string "#1=(a) #2=(b) (#1# #2# #1# #2#)")))
       (third (nth 2 x)))
  (list (eq (nth 0 third) (nth 2 third))
        (eq (nth 1 third) (nth 3 third))
        (not (eq (nth 0 third) (nth 1 third)))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_char_to_string_modifier() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217825)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (char-to-string ?\C-a)
      (char-to-string ?\M-a)
      (char-to-string ?\C-\M-a)
      (length (char-to-string ?\C-a))
      (string-bytes (char-to-string ?\C-\M-a)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_modifier_bit_arithmetic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (262241 134217825 134479969 0 134217728)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ctrl-mask 262144) (meta-mask 134217728))
  (list (+ ?a ctrl-mask)
        (+ ?a meta-mask)
        (+ ?a ctrl-mask meta-mask)
        (logand ?\C-a ctrl-mask)
        (logand ?\M-a meta-mask)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_local_global_key_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (local-action local-action global-action)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((gm (make-sparse-keymap)) (lm (make-sparse-keymap)))
  (define-key gm "a" 'global-action)
  (define-key lm "a" 'local-action)
  (use-global-map gm)
  (with-temp-buffer
    (use-local-map lm)
    (list (key-binding "a")
          (lookup-key (current-local-map) "a")
          (lookup-key (current-global-map) "a"))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_prin1_nested_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((inner (make-hash-table :test 'eq))
       (outer (make-hash-table :test 'eq)))
  (puthash 'key :inner-val inner)
  (puthash 'inner inner outer)
  (let ((p (prin1-to-string outer)))
    (list (string-match "#s(hash-table" p)
          (> (length p) 20))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_format_c_modifier_bits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217825)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (format "%c" ?\C-a)
      (format "%c" ?\M-a)
      (format "%c" ?\C-\M-a)
      (format "%c" ?\S-a)
      (format "%c" ?\H-a)
      (format "%c" ?\s-a)
      (length (format "%c" ?\C-a)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_coding_system_for_read_let_effect_file() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx30-cr-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "café" nil f nil 'silent))
  (prog1 (let ((coding-system-for-read 'utf-8-with-signature))
           (with-temp-buffer
             (insert-file-contents f)
             (list (buffer-string) (buffer-file-coding-system))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_decode_encode_string_with_coding_system_arg() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 15 15)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s "café世界😀")
       (enc (encode-coding-string s 'utf-8 'utf-8-unix))
       (dec (decode-coding-string enc 'utf-8)))
  (list (equal s dec) (length enc) (string-bytes enc)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_undo_after_insert_delete_text_prop_overlay_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"0156789ABCDEF\" 0 2 (face bold)) 5 3 (face bold)) #(\"0123456789ABCDEF\" 0 2 (face bold) 2 3 (face bold) 3 4 (face bold)) 8 3 (face bold))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (let ((ov (make-overlay 6 10)))
    (overlay-put ov 'face 'italic)
    (let ((m (set-marker (make-marker) 8)))
      (undo-boundary)
      (goto-char 4) (insert "X")
      (undo-boundary)
      (delete-region 3 7)
      (let ((state (list (buffer-string) (marker-position m)
                         (overlay-start ov) (text-properties-at 1))))
        (undo)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (text-properties-at 1))))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_process_send_string_newline_terminated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"line1\\nline2\\n\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (got)
  (let ((p (make-process :name "neo-cx30-sn" :command '("cat")
                         :buffer nil :connection-type 'pipe
                         :filter (lambda (proc str) (push str got)))))
    (process-send-string p "line1\n")
    (accept-process-output p 0.3)
    (process-send-string p "line2\n")
    (accept-process-output p 0.3)
    (process-send-eof p))
  (apply #'concat (nreverse got)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_cl_mapcar_mapcan_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-mapcar)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-mapcar #'+ '(1 2 3) '(10 20 30))
      (cl-mapcan (lambda (x) (list x (* x x))) '(1 2 3))
      (cl-remove-duplicates (cl-mapcan #'identity '((1 2) (2 3) (3 4)))
                            :test #'=))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_window_scroll_step_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 36 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx30-ss*")))
  (with-current-buffer buf
    (dotimes (i 5) (insert (format "line %d\n" i))))
  (set-window-buffer (selected-window) buf)
  (goto-char 1)
  (prog1 (list (window-start) (window-end) (point))
    (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_set_buffer_multibyte_text_prop_sticky_after_toggle() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (((face bold) (face italic)) (face bold) (face italic))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'face 'italic)
  (let ((before (list (text-properties-at 1) (text-properties-at 4))))
    (set-buffer-multibyte nil)
    (set-buffer-multibyte t)
    (list before (text-properties-at 1) (text-properties-at 4))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_cl_setf_on_buffer_substring_accessor() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"abcdef\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (store-substring (buffer-string) 2 ?X)
  (buffer-string))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_overlay_priority_face_invisible_combo_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic t italic t underline)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 1 5 'face 'bold)
  (let ((o1 (make-overlay 3 7)) (o2 (make-overlay 5 10)))
    (overlay-put o1 'face 'italic)
    (overlay-put o2 'face 'underline)
    (overlay-put o1 'invisible t)
    (overlay-put o1 'priority 5)
    (overlay-put o2 'priority 1))
  (list (get-char-property 1 'face)
        (get-char-property 3 'face)
        (get-char-property 3 'invisible)
        (get-char-property 6 'face)
        (get-char-property 6 'invisible)
        (get-char-property 8 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_string_bytes_vs_length_comprehensive() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp (string 128))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (s) (cons (length s) (string-bytes s)))
        '("" "a" "ab" "é" "aé" "世" "a世b"
          (string #x80) (string #x800) (string #x10000) (string #x10FFFF)
          (make-string 0 ?a) (make-string 1 ?a) (make-string 3 ?a)))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_coding_system_get_bom_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (nil t (utf-16le-with-signature . utf-16be-with-signature) nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :bom)
      (coding-system-get 'utf-8-with-signature :bom)
      (coding-system-get 'utf-16 :bom)
      (coding-system-get 'utf-16le :bom)
      (coding-system-get 'utf-16be-with-signature :bom))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_marker_point_after_set_buffer_multibyte_insert() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 \"aXbcdef\" 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((m (set-marker (make-marker) 4)))
    (set-buffer-multibyte nil)
    (goto-char 2) (insert "X")
    (set-buffer-multibyte t)
    (list (marker-position m) (buffer-string) (point-max))))
"##,
        expect,
    );
}

#[test]
fn div_cx30_reader_buffer_hash_after_various_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((h1 (with-temp-buffer (insert "hello") (buffer-hash)))
      (h2 (with-temp-buffer (insert "hello") (put-text-property 1 3 'face 'bold) (buffer-hash)))
      (h3 (with-temp-buffer (insert "hello") (buffer-hash))))
  (list (equal h1 h3) (not (equal h1 h2))))
"##,
        expect,
    );
}
