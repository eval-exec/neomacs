//! Complex combo batch 10 — process exit code deeper (signal kill, process
//! contact), write-region MUSTBENEW variants, coding-system-equal-type,
//! find-operation-coding-system, process-adaptive-read-buffering,
//! set-buffer-multibyte with overlays+narrowing+undo chain, encode-coding-region
//! with eol variants, text-property search across narrowed+overlaid region,
//! copy-to-register/insert-register round-trip, cl-coerce over multibyte
//! with text properties, prin1-to-string of overlay before/after-strings.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx10_process_exit_via_signal() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (signal 9)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx10-sig" :command '("sleep" "10"))))
  (accept-process-output p 0.1)
  (signal-process p 9)
  (accept-process-output p 1)
  (list (process-status p) (process-exit-status p)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_process_contact_info() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((p (make-process :name "neo-cx10-ci" :command '("echo" "x"))))
  (prog1 (list (consp (process-contact p))
               (process-contact p :local)
               (process-contact p t))
    (delete-process p)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_write_region_mustbenew_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"overwrite\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-cx10-mn-")))
  (condition-case e
      (progn (write-region "overwrite" nil f nil 0 nil nil) :ok)
    (file-already-exists :blocked))
  (prog1 (with-temp-buffer (insert-file-contents f) (buffer-string))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_coding_system_equal_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function coding-system-equal-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-equal-type 'utf-8 'utf-8-unix)
      (coding-system-equal-type 'utf-8 'latin-1)
      (coding-system-equal-type 'utf-8-unix 'utf-8-dos))
"##,
        expect,
    );
}

#[test]
fn div_cx10_find_operation_coding_system() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((undecided) (undecided))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (find-operation-coding-system 'write-region "data" nil "/tmp/x" nil 0)
      (find-operation-coding-system 'insert-file-contents "/tmp/x"))
"##,
        expect,
    );
}

#[test]
fn div_cx10_set_multibyte_overlay_narrow_undo_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Changes to be undone are outside visible portion of buffer\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "AAAAABBBBBCCCCCDDDDD")
  (let ((ov (make-overlay 6 10)) (m (set-marker (make-marker) 8)))
    (overlay-put ov 'face 'bold)
    (narrow-to-region 3 18)
    (undo-boundary)
    (goto-char (marker-position m))
    (insert "X")
    (let ((after (list (marker-position m) (overlay-start ov) (overlay-end ov))))
      (undo)
      (list after (marker-position m) (overlay-start ov) (overlay-end ov)
            (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_encode_region_with_eol_variants() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((99 97 102 4194243 4194217 13 10) (99 97 102 4194243 4194217 13))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "café\n"))
  (list (with-temp-buffer
          (insert s) (encode-coding-region 1 (point-max) 'utf-8-dos)
          (append (buffer-string) nil))
        (with-temp-buffer
          (insert s) (encode-coding-region 1 (point-max) 'utf-8-mac)
          (append (buffer-string) nil))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_text_property_search_narrowed_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument prop-match (0 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDEF")
  (put-text-property 3 7 'face 'bold)
  (put-text-property 10 14 'face 'italic)
  (let ((ov (make-overlay 5 12))) (overlay-put ov 'mouse-face 'highlight))
  (narrow-to-region 2 15)
  (goto-char (point-min))
  (let (results)
    (while (text-property-search-forward 'face nil t)
      (push (prop-match-beginning (match-data t)) results))
    (nreverse results)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_copy_register_insert_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (wrong-type-argument integer-or-marker-p #<buffer  *neo-cx10-reg*>)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((buf (get-buffer-create " *neo-cx10-reg*")))
  (with-current-buffer buf (erase-buffer) (insert "register content"))
  (copy-to-register ?r buf 1 17)
  (with-temp-buffer
    (insert-register ?r)
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_cl_coerce_multibyte_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-coerce)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((s (propertize "café" 'face 'bold))
       (lst (cl-coerce s 'list))
       (back (cl-coerce lst 'string)))
  (list (length lst) (text-properties-at 0 back)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_prin1_overlay_before_after_strings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r##""OK (\"#(\\\">>\\\" 0 2 (face bold))\" \"#(\\\"<<\\\" 0 2 (face italic))\" (face bold))""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((ov (make-overlay 2 4)))
    (overlay-put ov 'before-string (propertize ">>" 'face 'bold))
    (overlay-put ov 'after-string (propertize "<<" 'face 'italic))
    (list (prin1-to-string (overlay-get ov 'before-string))
          (prin1-to-string (overlay-get ov 'after-string))
          (text-properties-at 0 (overlay-get ov 'before-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_coding_system_charset_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((unicode) (iso-8859-1) 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (coding-system-charset-list 'utf-8)
      (coding-system-charset-list 'latin-1)
      (length (coding-system-charset-list 'utf-8)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_buffer_undo_list_markers_overlays() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdef")
  (let ((m (point-marker)))
    (undo-boundary)
    (goto-char 3) (insert "X")
    (undo-boundary)
    (let ((ov (make-overlay 2 5))) (overlay-put ov 'face 'bold))
    (let ((entry (car buffer-undo-list)))
      (list (consp entry) (consp buffer-undo-list)
            (> (length buffer-undo-list) 2)))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_decode_coding_string_with_offset_limit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"é€é😀\" 4 11)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((raw (unibyte-string 195 169 226 130 172 195 169 240 159 152 128)))
  (list (decode-coding-string raw 'utf-8)
        (length (decode-coding-string raw 'utf-8))
        (string-bytes (decode-coding-string raw 'utf-8))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_process_filter_partial_data() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"chunk1chunk2chunk3\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (chunks)
  (let ((p (make-process :name "neo-cx10-pd" :command '("printf" "%s" "chunk1chunk2chunk3")
                         :buffer nil
                         :filter (lambda (proc str) (push str chunks)))))
    (accept-process-output p 0.1)
    (accept-process-output p 0.1))
  (apply #'concat (nreverse chunks)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_set_process_filter_to_nil() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"FILTER:data\\n\" \"nofilter\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx10-nf" :command '("echo" "data")
                         :buffer (current-buffer))))
    ;; Silence the default sentinel (its "Process ... finished" buffer message
    ;; is incidental noise here) and drain to completion before reading, so the
    ;; filter-vs-buffer observable is deterministic on both engines.
    (set-process-sentinel p #'ignore)
    (set-process-filter p (lambda (proc msg) (insert "FILTER:" msg)))
    (while (process-live-p p) (accept-process-output p 1))
    (while (accept-process-output p 0))
    (let ((with-filter (buffer-string)))
      (erase-buffer)
      (set-process-filter p nil)
      (let ((p2 (make-process :name "neo-cx10-nf2" :command '("echo" "nofilter")
                              :buffer (current-buffer))))
        (set-process-sentinel p2 #'ignore)
        (while (process-live-p p2) (accept-process-output p2 1))
        (while (accept-process-output p2 0)))
      (list with-filter (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_encode_coding_string_empty_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"\" \"\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (encode-coding-string "" 'utf-8)
      (decode-coding-string "" 'utf-8)
      (length (encode-coding-string "" 'utf-8)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_string_as_multibyte_vs_make_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((4194248 4194249 65) (4194248 4194249 65) 3 3 5 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string 200 201 65))
       (as-m (string-as-multibyte raw))
       (make-m (string-make-multibyte raw)))
  (list (append as-m nil) (append make-m nil)
        (length as-m) (length make-m)
        (string-bytes as-m) (string-bytes make-m)))
"##,
        expect,
    );
}

#[test]
fn div_cx10_multiple_overlay_priority_char_property_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold italic italic bold bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ABCDEFGHIJ")
  (let ((o1 (make-overlay 1 10)) (o2 (make-overlay 3 7)) (o3 (make-overlay 5 8)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (overlay-put o1 'priority 1) (overlay-put o2 'priority 5) (overlay-put o3 'priority 3)
    (list (get-char-property 1 'face)
          (get-char-property 4 'face)
          (get-char-property 6 'face)
          (get-char-property 8 'face)
          (get-char-property 9 'face))))
"##,
        expect,
    );
}

#[test]
fn div_cx10_format_mode_line_fragment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((s (format-mode-line "%b %m")))
      (list (stringp s) (> (length s) 0)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}
