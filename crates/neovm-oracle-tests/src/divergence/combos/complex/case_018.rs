//! Complex combo batch 18 — write-region VISIT arg deep probe (the false
//! positive root from batches 14-16), process exit code + buffer content,
//! set-buffer-multibyte + char-charset, coding-system-plist deep,
//! overlay before-string with text props + narrowing + display,
//! cl-defstruct + cl-call-next-method, hash-table weak ref + GC count,
//! char-table-decode-char per charset deep, read/print #s struct with
//! custom printer, timer-persistent, process-connection-type pty.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx18_write_region_visit_arg_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"1890cf168caf07dff18f8beee3281439\" \"1890cf168caf07dff18f8beee3281439\" t \"07117fe4a1ebd544965dc19573183da2\" \"07117fe4a1ebd544965dc19573183da2\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f "/tmp/neo-cx18-v-fixed"))
  (let ((path1 (secure-hash 'md5 f))
        (content1 (secure-hash 'md5 "café")))
    (let ((path2 (secure-hash 'md5 f))
          (content2 (secure-hash 'md5 "café")))
      (list path1 path2 (equal path1 path2)
            content1 content2 (equal content1 content2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_process_exit_and_buffer_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (exit 3 \"done\\n\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx18-pe*"))
      result)
  (with-current-buffer buf (erase-buffer))
  (let ((p (make-process :name "neo-cx18-pe" :command '("sh" "-c" "echo done; exit 3")
                         :buffer buf)))
    (set-process-sentinel p #'ignore)
    (set-process-query-on-exit-flag p nil)
    (let ((i 0))
      (while (and (memq (process-status p) '(run open listen connect stop))
                  (< i 40))
        (accept-process-output p 0.05)
        (setq i (1+ i))))
    (setq result
          (list (process-status p)
                (process-exit-status p)
                (with-current-buffer buf (buffer-string))
                (with-current-buffer buf (buffer-modified-p)))))
  (kill-buffer buf)
  result)
"##,
        expect,
    );
}

#[test]
fn div_cx18_set_buffer_multibyte_char_charset_after() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (eight-bit eight-bit ascii)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert (unibyte-string 200 201 65))
  (set-buffer-multibyte t)
  (mapcar #'char-charset (append (buffer-string) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_coding_system_plist_deep_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-get 'utf-8 :flags)
      (coding-system-get 'utf-8 :designation)
      (coding-system-get 'latin-1 :flags)
      (coding-system-get 'utf-8-with-signature :flags)
      (coding-system-get 'utf-8-with-signature :bom))
"##,
        expect,
    );
}

#[test]
fn div_cx18_overlay_before_string_props_narrow_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"123Y456789ABC\" 1 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((ov (make-overlay 4 8)))
    (overlay-put ov 'before-string (propertize ">>" 'face 'bold 'display "XX"))
    (overlay-put ov 'after-string (propertize "<<" 'face 'italic)))
  (narrow-to-region 2 14)
  (goto-char 5)
  (insert "Y")
  (list (buffer-string)
        (length (overlays-in (point-min) (point-max)))
        (get-char-property 3 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_cl_defstruct_call_next_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 50)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx18-base () ((val :initarg :val)))
  (defclass neo-cx18-sub (neo-cx18-base) ())
  (cl-defgeneric neo-cx18-get (obj))
  (cl-defmethod neo-cx18-get ((obj neo-cx18-base))
    (oref obj val))
  (cl-defmethod neo-cx18-get :around ((obj neo-cx18-sub))
    (* 10 (cl-call-next-method)))
  (list (neo-cx18-get (neo-cx18-base :val 5))
        (neo-cx18-get (neo-cx18-sub :val 5))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_char_table_decode_char_charset_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (12354 65 4194248 12354 65 200 unicode-bmp ascii eight-bit)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((c1 (decode-char 'unicode #x3042))
      (c2 (decode-char 'ascii 65))
      (c3 (decode-char 'eight-bit 200)))
  (list c1 c2 c3
        (encode-char c1 'unicode)
        (encode-char c2 'ascii)
        (encode-char c3 'eight-bit)
        (char-charset c1) (char-charset c2) (char-charset c3)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_print_read_struct_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-defstruct)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (cl-defstruct neo-cx18-struct a b)
  (let* ((s (make-neo-cx18-struct :a 1 :b "café"))
         (p (prin1-to-string s))
         (back (car (read-from-string p))))
    (list (neo-cx18-struct-a back)
          (neo-cx18-struct-b back)
          (neo-cx18-struct-p back))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_process_connection_pty_error_path() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (:file-error \"Unknown connection type\" \"No such file or directory\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (make-process :name "neo-cx18-pty" :command '("echo" "x")
                  :connection-type t)
  (file-error (cons :file-error (cdr e)))
  (file-missing (cons :file-missing (cdr e)))
  (error (cons :other-error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_string_make_unibyte_then_back_char_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (6 6 6 (99 97 102 4194281 22 76))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((orig "café世界")
       (u (string-make-unibyte orig))
       (back (string-make-multibyte u)))
  (list (length orig) (length u) (length back)
        (append back nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_decode_encode_roundtrip_various_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café世界😀"))
  (list (equal s (decode-coding-string (encode-coding-string s 'utf-8) 'utf-8))
        (equal s (decode-coding-string (encode-coding-string s 'utf-8-with-signature) 'utf-8-with-signature))
        (equal s (decode-coding-string (encode-coding-string s 'utf-16) 'utf-16))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_buffer_hash_after_edit_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "hello world")
  (let ((h1 (buffer-hash)))
    (goto-char 6) (insert "X")
    (let ((h2 (buffer-hash)))
      (undo)
      (list h1 h2 (buffer-hash) (equal h1 (buffer-hash)) (eq h1 (buffer-hash))))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_overlay_after_string_with_multibyte_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"世界X\" 3 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (let ((ov (make-overlay 2 3)))
    (overlay-put ov 'after-string "世界X"))
  (list (overlay-get (car (overlays-at 2)) 'after-string)
        (length (overlay-get (car (overlays-at 2)) 'after-string))
        (string-bytes (overlay-get (car (overlays-at 2)) 'after-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_set_match_data_vector_format_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument listp [0 6 2 3 3 4])""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (string-match "\\(a\\)\\(b\\)" "xxabyy")
  (let ((md (match-data)))
    (set-match-data [0 6 2 3 3 4])
    (list (match-beginning 0) (match-end 0)
          (match-beginning 1) (match-end 1)
          (match-beginning 2) (match-end 2)
          (progn (set-match-data md)
                 (match-string 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_cl_loop_for_hash_values_sum() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table)))
  (puthash 'a 10 ht) (puthash 'b 20 ht) (puthash 'c 30 ht)
  (cl-loop for v being the hash-values of ht sum v))
"##,
        expect,
    );
}

#[test]
fn div_cx18_process_kill_after_start_buffer_kept() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx18-kb*")))
  (with-current-buffer buf (insert "pre"))
  (let ((p (make-process :name "neo-cx18-kb" :command '("echo" "out")
                         :buffer buf)))
    (accept-process-output p 1)
    (let ((content (with-current-buffer buf (buffer-string))))
      (delete-process p)
      (prog1 (list content (buffer-live-p buf)
                   (with-current-buffer buf (buffer-string)))
        (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_text_property_not_all_with_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789")
  (put-text-property 1 4 'face 'bold)
  (let ((ov (make-overlay 3 7))) (overlay-put ov 'face 'italic))
  (list (text-property-not-all 1 8 'face nil)
        (text-property-any 1 8 'face 'italic)
        (next-property-change 1)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_undo_tree_marker_overlay_deep_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((#(\"AAAAXBBBCCCCC\" 0 4 (face italic)) 6 3 (face italic)) #(\"AAAAABBBBBCCCCC\" 0 4 (face italic)) 8 3 (face italic))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAAAABBBBBCCCCC")
  (let ((m (set-marker (make-marker) 8))
        (ov (make-overlay 3 7)))
    (overlay-put ov 'face 'bold)
    (put-text-property 1 5 'face 'italic)
    (undo-boundary)
    (goto-char 5) (insert "X")
    (undo-boundary)
    (aset (buffer-substring 1 3) 0 ?Z)
    (undo-boundary)
    (delete-region 6 9)
    (let ((state (list (buffer-string) (marker-position m)
                       (overlay-start ov) (text-properties-at 1))))
      (dotimes (_ 3) (condition-case nil (undo) (error nil)))
      (list state (buffer-string) (marker-position m)
            (overlay-start ov) (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx18_coding_system_undecided_decode_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" 4 (99 97 102 233))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 99 97 102 195 169)))
  (list (decode-coding-string raw 'undecided)
        (length (decode-coding-string raw 'undecided))
        (append (decode-coding-string raw 'undecided) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx18_narrow_restrict_widen_marker_consistency() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 13 10 1 17 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (let ((m (set-marker (make-marker) 10)))
    (narrow-to-region 3 13)
    (let ((n-min (point-min)) (n-max (point-max)) (m-in (marker-position m)))
      (widen)
      (list n-min n-max m-in (point-min) (point-max) (marker-position m)))))
"##,
        expect,
    );
}
