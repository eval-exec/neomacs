//! Complex combo batch 28 — extend reader-error area + remaining process/timing
//! + print-flag + coding-change-eol edges.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx28_reader_hex_octal_binary_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (invalid-read-syntax invalid-read-syntax invalid-read-syntax invalid-read-syntax 3735928559 511 10)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "#xgg")) (error (car e)))
      (condition-case e (car (read-from-string "#o8")) (error (car e)))
      (condition-case e (car (read-from-string "#b2")) (error (car e)))
      (condition-case e (car (read-from-string "#x")) (error (car e)))
      (car (read-from-string "#xdeadbeef"))
      (car (read-from-string "#o777"))
      (car (read-from-string "#b1010")))
"##,
        expect,
    );
}

#[test]
fn div_cx28_reader_char_escape_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (end-of-file end-of-file 1 134217825 134217729 8 233 233 65)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "?\\C-")) (error (car e)))
      (condition-case e (car (read-from-string "?\\M-")) (error (car e)))
      (car (read-from-string "?\\C-a"))
      (car (read-from-string "?\\M-a"))
      (car (read-from-string "?\\C-\\M-a"))
      (car (read-from-string "?\\^H"))
      (car (read-from-string "?\\N{U+00E9}"))
      (car (read-from-string "?\\u00e9"))
      (car (read-from-string "?\\x41")))
"##,
        expect,
    );
}

#[test]
fn div_cx28_reader_vector_struct_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (invalid-read-syntax end-of-file invalid-read-syntax invalid-read-syntax invalid-read-syntax)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (car (read-from-string "#(a b)")) (error (car e)))
      (condition-case e (car (read-from-string "#(")) (error (car e)))
      (condition-case e (car (read-from-string "#&")) (error (car e)))
      (condition-case e (car (read-from-string "#s")) (error (car e)))
      (condition-case e (car (read-from-string "#^^")) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_process_sentinel_exit_zero_vs_nonzero() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 7)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (ev0 ev7)
  (let ((p0 (make-process :name "neo-cx28-e0" :command '("true")
                          :sentinel (lambda (proc event) (setq ev0 event))))
        (p7 (make-process :name "neo-cx28-e7" :command '("sh" "-c" "exit 7")
                          :sentinel (lambda (proc event) (setq ev7 event)))))
    (accept-process-output p0 2)
    (accept-process-output p7 2))
  (list (if ev0 (string-match "finished" ev0) nil)
        (if ev7 (string-match "abnormally" ev7) nil)))
"##,
        expect,
    );
}

#[test]
fn div_cx28_print_flags_interaction() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (0 \"(#1=#:g0 #1#)\" \"\\\"line1\\\\nline2\\\"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((print-gensym t) (print-circle t) (print-escape-newlines t))
  (let ((gs (gensym)))
    (list (string-match "#:" (prin1-to-string gs))
          (prin1-to-string (list gs gs))
          (prin1-to-string "line1\nline2"))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_coding_system_change_eol_on_custom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (neo-cx28-cs-dos neo-cx28-cs-mac)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (progn
      (define-coding-system 'neo-cx28-cs "Test"
        :coding-type 'utf-8 :mnemonic ?T :charset-list '(unicode))
      (list (coding-system-change-eol-conversion 'neo-cx28-cs 'dos)
            (coding-system-change-eol-conversion 'neo-cx28-cs 'mac)))
  (error (cons 'errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_equal_markers_comparison() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdef")
  (let ((m1 (set-marker (make-marker) 3))
        (m2 (set-marker (make-marker) 3))
        (m3 (set-marker (make-marker) 4)))
    (list (equal m1 m2) (eq m1 m2) (equal m1 m3) (eq m1 m3))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_plist_deep_manipulation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (2 (:c 3 :d 4) (:a 1 :b 99 :c 3 :d 4) (:a 1 :b 99 :c 3 :d 4) 2)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((pl '(:a 1 :b 2 :c 3)))
  (list (plist-get pl :b)
        (plist-member pl :c)
        (plist-put pl :d 4)
        (plist-put pl :b 99)
        (lax-plist-get '("a" 1 "b" 2) "b")))
"##,
        expect,
    );
}

#[test]
fn div_cx28_buffer_local_variables_listing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-remove-if-not)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (defvar neo-cx28-bl1 0)
  (defvar neo-cx28-bl2 0)
  (setq-local neo-cx28-bl1 :local1)
  (setq-local neo-cx28-bl2 :local2)
  (let ((locals (cl-remove-if-not
                  (lambda (pair) (string-match "neo-cx28" (symbol-name (car pair))))
                  (buffer-local-variables))))
    (sort (mapcar #'car locals)
          (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_cl_typep_hierarchy_predicates() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-typep)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (cl-typep 5 'integer)
      (cl-typep "x" 'string)
      (cl-typep '(1 2) 'cons)
      (cl-typep [1 2] 'vector)
      (cl-typep nil 'null)
      (cl-typep 'x 'symbol)
      (cl-typep ?a 'integer)
      (cl-typep 3.14 'float)
      (cl-typep (make-hash-table) 'hash-table))
"##,
        expect,
    );
}

#[test]
fn div_cx28_decode_encode_region_unicode_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 15 \"café世界😀\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界😀")
  (let ((orig (buffer-string)))
    (encode-coding-region (point-min) (point-max) 'utf-8)
    (let ((encoded-len (length (buffer-string))))
      (decode-coding-region (point-min) (point-max) 'utf-8)
      (list (equal orig (buffer-string)) encoded-len (buffer-string)))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_undo_after_format_replace_region() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"foo = 10\\nbar = 20\\nbaz = 30\\n\" \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "foo = 1\nbar = 2\nbaz = 3\n")
  (undo-boundary)
  (goto-char 1)
  (while (re-search-forward "= \\([0-9]+\\)" nil t)
    (replace-match (format "= %d" (* 10 (string-to-number (match-string 1))))))
  (let ((after (buffer-string)))
    (undo)
    (list after (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_set_buffer_multibyte_nil_text_property_preserve() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (face bold) (face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (put-text-property 1 3 'face 'bold)
  (let ((before (text-properties-at 1)))
    (set-buffer-multibyte nil)
    (let ((unibyte-props (text-properties-at 1)))
      (set-buffer-multibyte t)
      (list before unibyte-props (text-properties-at 1)))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_window_text_height_body_height() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (> (window-total-height) 0)
      (> (window-body-height) 0)
      (>= (window-total-height) (window-body-height))
      (> (window-total-width) 0)
      (> (window-body-width) 0))
"##,
        expect,
    );
}

#[test]
fn div_cx28_char_table_range_nil_empty_check() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (error \"Invalid RANGE argument to ‘char-table-range’\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'cx28 nil)))
  (list (char-table-range ct ?a)
        (char-table-range ct t)
        (progn (set-char-table-range ct nil :nil-range)
               (char-table-range ct ?z))
        (char-table-range ct t)))
"##,
        expect,
    );
}

#[test]
fn div_cx28_map_concat_over_hash_values() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument number-or-marker-p \"2\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht) (puthash "c" 3 ht)
  (let (vals)
    (maphash (lambda (k v) (push (number-to-string v) vals)) ht)
    (mapconcat #'identity (sort vals #'<) ",")))
"##,
        expect,
    );
}

#[test]
fn div_cx28_overlay_evaporate_narrow_widen() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "0123456789ABCDE")
  (let ((ov (make-overlay 5 10)))
    (overlay-put ov 'evaporate t)
    (overlay-put ov 'face 'bold)
    (narrow-to-region 3 8)
    (list (overlay-start ov) (overlay-end ov)
          (point-min) (point-max)
          (get-char-property 4 'face)))
"##,
        expect,
    );
}

#[test]
fn div_cx28_process_output_coding_system_after_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"café\" 4 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((p (make-process :name "neo-cx28-oc" :command '("printf" "caf\\303\\251")
                         :buffer (current-buffer))))
    ;; The default sentinel is a process-status timing artifact; this case is
    ;; about decoding after `set-process-coding-system`.
    (set-process-sentinel p #'ignore)
    (set-process-coding-system p 'utf-8-unix 'utf-8-unix)
    (accept-process-output p 1))
  (list (buffer-string) (length (buffer-string)) (string-bytes (buffer-string))))
"##,
        expect,
    );
}

#[test]
fn div_cx28_coding_system_get_designation_flags_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument arrayp nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((desig (coding-system-get 'utf-8 :designation)))
  (list (vectorp desig)
        (length desig)
        (aref desig 0)))
"##,
        expect,
    );
}

#[test]
fn div_cx28_prin1_to_string_of_buffer_substring_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((face bold) (font-lock-face keyword) t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "café世界")
  (put-text-property 1 3 'face 'bold)
  (put-text-property 4 6 'font-lock-face 'keyword)
  (let* ((sub (buffer-substring 1 6))
         (p (prin1-to-string sub))
         (back (car (read-from-string p))))
    (list (text-properties-at 0 back)
          (text-properties-at 3 back)
          (equal sub back))))
"##,
        expect,
    );
}
