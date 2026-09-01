//! Complex combo batch 21 — MEGA combos (5+ features) + remaining edges.
//!
//! coding-system-for-read/write let-binding propagation, set-buffer-multibyte +
//! text-prop + overlay + narrow + undo mega, process filter + buffer mod + undo,
//! cl-defmethod multi-arg dispatch, hash-table modify-during-maphash, char-table-
//! parent extra-slot inheritance, read-circle + print-gensym mega, cl-loop
//! hash-keys+values simultaneously.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx21_coding_for_read_write_let_propagation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx21-crw-")))
  (let ((coding-system-for-write 'utf-8-unix))
    (write-region "café" nil f nil 'silent))
  (prog1 (let ((coding-system-for-read 'utf-8-unix))
           (with-temp-buffer
             (insert-file-contents f)
             (list (buffer-string) (buffer-file-coding-system))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_mega_multibyte_textprop_overlay_narrow_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAAAABBBBBCCCCCDDDDDEEEEE")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (let ((ov1 (make-overlay 3 7)) (ov2 (make-overlay 11 15)))
    (overlay-put ov1 'face 'underline)
    (overlay-put ov2 'face 'default)
    (let ((m (set-marker (make-marker) 8)))
      (narrow-to-region 2 20)
      (undo-boundary)
      (goto-char 5) (insert "X")
      (undo-boundary)
      (delete-region 3 8)
      (let ((state (list (point-min) (point-max)
                         (marker-position m)
                         (overlay-start ov1) (overlay-end ov1)
                         (overlay-start ov2) (overlay-end ov2)
                         (buffer-string) (text-properties-at 1))))
        (undo) (undo) (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov1) (overlay-end ov1)
              (text-properties-at 1) (text-properties-at 5))))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_process_filter_buffer_mod_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx21-pf*")))
  (with-current-buffer buf
    (buffer-enable-undo)
    (insert "original"))
  (let ((p (make-process :name "neo-cx21-pf" :command '("echo" "process-output")
                         :buffer nil
                         :filter (lambda (proc str)
                                   (with-current-buffer buf
                                     (goto-char (point-max))
                                     (insert str))))))
    (accept-process-output p 1))
  (prog1 (with-current-buffer buf
           (let ((content (buffer-string)))
             (undo)
             (list content (buffer-string))))
    (kill-buffer buf)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_cl_defmethod_multi_arg_dispatch() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (cl-no-applicable-method neo-cx21-fn 42 42)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (defclass neo-cx21-a () ())
  (defclass neo-cx21-b () ())
  (let (log)
    (cl-defgeneric neo-cx21-fn (x y))
    (cl-defmethod neo-cx21-fn ((x neo-cx21-a) (y neo-cx21-b))
      (push :a-b log))
    (cl-defmethod neo-cx21-fn ((x neo-cx21-a) y)
      (push :a-any log))
    (cl-defmethod neo-cx21-fn (x (y neo-cx21-b))
      (push :any-b log))
    (let ((a (neo-cx21-a)) (b (neo-cx21-b)))
      (neo-cx21-fn a b)
      (neo-cx21-fn a 42)
      (neo-cx21-fn 42 b)
      (neo-cx21-fn 42 42))
    (nreverse log)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_hash_table_modify_during_maphash() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-evenp)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (dotimes (i 5) (puthash (number-to-string i) i ht))
  (let (removed)
    (maphash (lambda (k v)
               (when (cl-evenp v) (push k removed)))
             ht)
    (dolist (k removed) (remhash k ht))
    (list (hash-table-count ht)
          (sort (let (keys) (maphash (lambda (k v) (push k keys)) ht) keys)
                #'string<))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_char_table_parent_extra_slot_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments make-char-table 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-char-table 'cx21 nil 2)) (c (make-char-table 'cx21 nil 2)))
  (set-char-table-extra-slot p 0 :parent-slot0)
  (set-char-table-parent c p)
  (list (char-table-extra-slot c 0)
        (char-table-extra-slot p 0)
        (eq (char-table-parent c) p)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_read_circle_print_gensym_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 4 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((print-circle t) (print-gensym t)
       (gs (gensym))
       (x (list gs gs))
       (p (prin1-to-string x))
       (back (car (read-from-string p))))
  (list (string-match "#1=" p)
        (string-match "#:" p)
        (eq (car back) (cadr back))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_cl_loop_hash_keys_values_simultaneous() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht) (puthash "c" 3 ht)
  (sort (cl-loop for k being the hash-keys of ht using (hash-values v)
                 collect (format "%s=%d" k v))
        #'string<))
"##,
        expect,
    );
}

#[test]
fn div_cx21_coding_system_for_read_default_value() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function buffer-file-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx21-dr-")))
  (write-region "data" nil f nil 'silent)
  (prog1 (with-temp-buffer
           (let ((coding-system-for-read nil))
             (insert-file-contents f))
           (list (buffer-string) (buffer-file-coding-system)))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_unwind_protect_throw_across_multiple_unwind() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (:inner :mid :outer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (log)
  (catch 'tag
    (unwind-protect
        (unwind-protect
            (unwind-protect
                (throw 'tag :thrown)
              (push :inner log))
          (push :mid log))
      (push :outer log)))
  (nreverse log))
"##,
        expect,
    );
}

#[test]
fn div_cx21_marker_relocation_after_replace_match() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (11 \"foo BARBAR baz\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "foo bar baz")
  (let ((m (set-marker (make-marker) 8)))
    (goto-char 1)
    (re-search-forward "bar")
    (replace-match "BARBAR")
    (list (marker-position m) (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_overlay_evaporate_undo_propagated() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((t 3) t nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "0123456789")
  (let ((ov (make-overlay 3 6)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'evaporate t)
    (undo-boundary)
    (replace-string "345" "" nil 3 6)
    (let ((evaporated (list (overlayp ov) (overlay-start ov))))
      (undo)
      (list evaporated (overlayp ov) (overlay-start ov) (overlay-end ov)))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_decode_encode_region_roundtrip_utf16() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"AB\" 4 \"AB\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "AB")
  (let ((orig (buffer-string)))
    (encode-coding-region 1 (point-max) 'utf-16be)
    (let ((encoded-length (length (buffer-string))))
      (decode-coding-region 1 (point-max) 'utf-16be)
      (list orig encoded-length (buffer-string) (equal orig (buffer-string))))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_prin1_escape_newlines_multibyte_combined() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\\\"café\\\\nworld\ttab\\\"\" 13)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-escape-newlines t) (print-escape-nonascii t))
  (list (prin1-to-string "café\nworld\ttab")
        (length (prin1-to-string "café\nworld"))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_process_exit_code_via_call_process_vs_make_process() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (call-process "sh" nil nil nil "-c" "exit 7")
      (let ((p (make-process :name "neo-cx21-ec" :command '("sh" "-c" "exit 7"))))
        (accept-process-output p 2)
        (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_text_property_any_not_all_with_narrow_overlay_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDE")
  (put-text-property 1 5 'face 'bold)
  (put-text-property 6 10 'face 'italic)
  (let ((ov (make-overlay 8 12))) (overlay-put ov 'face 'underline))
  (narrow-to-region 3 13)
  (list (text-property-any (point-min) (point-max) 'face 'bold)
        (text-property-not-all (point-min) (point-max) 'face nil)
        (next-property-change (point-min))
        (next-single-property-change (point-min) 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_window_dedicated_set_buffer_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx21-wd*")))
  (set-window-buffer (selected-window) buf)
  (set-window-dedicated-p (selected-window) t)
  (let ((ded (window-dedicated-p)))
    (set-window-dedicated-p (selected-window) nil)
    (prog1 (list ded (window-dedicated-p) (eq (window-buffer) buf))
      (set-window-buffer (selected-window) (get-buffer-create "*scratch*"))
      (kill-buffer buf))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_cl_setf_on_plist_via_getf() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function \\(setf\\ cl-getf\\))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pl '(:a 1 :b 2)))
  (setf (cl-getf pl :c :default) 3)
  (setf (cl-getf pl :a) 99)
  (list pl (cl-getf pl :a) (cl-getf pl :c) (cl-getf pl :d :missing)))
"##,
        expect,
    );
}

#[test]
fn div_cx21_encode_coding_string_vs_region_consistency_all_codings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café"))
  (list (equal (encode-coding-string s 'utf-8)
               (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-8) (buffer-string)))
        (equal (encode-coding-string s 'latin-1)
               (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'latin-1) (buffer-string)))
        (equal (encode-coding-string s 'utf-16be)
               (with-temp-buffer (insert s) (encode-coding-region 1 (point-max) 'utf-16be) (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx21_buffer_hash_stability_after_identical_edits() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"aaf4c61ddcc5e8a2dabede0f3b482cd9aea9434d\" \"7c211433f02071597741e6ff5a8ea34789abbf43\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (with-temp-buffer (insert "hello") (buffer-hash))
      (with-temp-buffer (insert "hello") (buffer-hash))
      (with-temp-buffer (insert "world") (buffer-hash)))
"##,
        expect,
    );
}
