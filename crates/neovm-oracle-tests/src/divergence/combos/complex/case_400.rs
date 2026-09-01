//! Complex combo batch 400 — 20 new integration probes targeting known
//! divergence themes with fresh combinations: CF/D1 case-fold asymmetry
//! in narrow+replace+multibyte, more read-only enforcement gaps, display
//! property + column + overlay accounting, eight-bit width through coding
//! roundtrips, BOM encoding+append, overlay-lists at point under edit,
//! set-buffer-multibyte data loss with markers, error quote style in
//! deep signal paths, composition+find-composition+overlay,
//! bidi auto-detection with RTL, charset-plist missing keys,
//! weak-table+gc+print, and multi-way face/color matrix combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// CF/D1 case-fold asymmetry (lower→upper fails for Greek π–ω and Cyrillic
/// р–я) via replace-regexp-in-string with narrow+multibyte context.
#[test]
fn div_cx400_casefold_cyrillic_narrow_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"БЕТА gamma Дelt\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "alpha Бета gamma Дelta epsilon")
    (narrow-to-region 7 22)
    (goto-char (point-min))
    (while (re-search-forward "[а-я]+" nil t)
      (replace-match (upcase (match-string 0))))
    (buffer-string)))
"##,
        expect,
    );
}

/// CF/D1 with Greek sigma/omega/pi replace across word boundaries.
#[test]
fn div_cx400_casefold_greek_multi_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"ΠΡΩΤ_ΟΣ δευτ_ερ_ος ΤΡΙΤ_ΟΣ\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (replace-regexp-in-string
   "\\([πρστυφχψω]\\)\\([αεο]\\)"
   "\\1_\\2"
   "ΠΡΩΤΟΣ δευτερος ΤΡΙΤΟΣ"))
"##,
        expect,
    );
}

/// Read-only text property blocking: forward-word, transpose-words,
/// kill-region, and fill-paragraph on a read-only span.
#[test]
fn div_cx400_read_only_textprop_multi_op() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t text-read-only text-read-only \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa bbb ccc ddd eee fff ggg")
  (put-text-property 5 11 'read-only t)
  (let ((results (list)))
    (goto-char 1)
    (push (condition-case e (forward-word 3) (error (car e))) results)
    (push (condition-case e (transpose-words 1) (error (car e))) results)
    (push (condition-case e (kill-region 5 8) (error (car e))) results)
    (push (condition-case e (fill-paragraph nil) (error (car e))) results)
    (nreverse results)))
"##,
        expect,
    );
}

/// Display property + current-column + move-to-column + overlay face.
/// Neomacs ignores the display glyph width in column accounting.
#[test]
fn div_cx400_display_property_column_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (10 4 6 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "x  y  z")
  (put-text-property 2 3 'display "ABC")
  (let ((ov (make-overlay 4 5)))
    (overlay-put ov 'face 'bold)
    (overlay-put ov 'display "DE"))
  (list (current-column)
        (progn (move-to-column 5) (point))
        (progn (move-to-column 8) (point))
        (progn (move-to-column 10) (point))))
"##,
        expect,
    );
}

/// Eight-bit recovered bytes: decode-invalid UTF-8 followed by
/// string-bytes, =, and prin1-to-string — probes 2vs3 byte width.
#[test]
fn div_cx400_eightbit_recovered_width_combo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (6 6 3 3 t t \"\\\"\\\\200\\\\201\\\\377\\\"\" \"\\\"\\\\200\\\\201\\\\377\\\"\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((raw (unibyte-string #x80 #x81 #xff))
       (decoded (decode-coding-string raw 'utf-8))
       (constructed (string-make-multibyte raw)))
  (list (string-bytes decoded) (string-bytes constructed)
        (length decoded) (length constructed)
        (string= decoded constructed)
        (equal decoded constructed)
        (prin1-to-string decoded)
        (prin1-to-string constructed)))
"##,
        expect,
    );
}

/// BOM encoding roundtrip: write with utf-8-with-signature, append,
/// read back, check for BOM prefix.
#[test]
fn div_cx400_bom_encode_append_read() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 12 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-bom-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "abc" nil f nil 0)
    (write-region "def" nil f 'append 0))
  (prog1 (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents f)
           (let* ((bytes (buffer-string))
                  (bom (and (>= (length bytes) 3)
                            (= (aref bytes 0) #xef)
                            (= (aref bytes 1) #xbb)
                            (= (aref bytes 2) #xbf))))
             (list bom (string-bytes bytes) (length bytes))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

/// Overlay-lists before/after point categorization under
/// insert+delete at varying points.
#[test]
fn div_cx400_overlay_lists_insert_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((3 0 1) (3 0 1) (3 0 1))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abcdefghij")
  (let ((o1 (make-overlay 2 4)) (o2 (make-overlay 5 7)) (o3 (make-overlay 8 10)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (overlay-put o3 'face 'underline)
    (let ((r1 (list (length (car (overlay-lists)))
                    (length (cdr (overlay-lists)))
                    (length (overlays-at 6)))))
      (goto-char 3)
      (insert "XXX")
      (let ((r2 (list (length (car (overlay-lists)))
                      (length (cdr (overlay-lists)))
                      (length (overlays-at 6)))))
        (delete-region 6 9)
        (let ((r3 (list (length (car (overlay-lists)))
                        (length (cdr (overlay-lists)))
                        (length (overlays-at 6)))))
          (list r1 r2 r3))))))
"##,
        expect,
    );
}

/// set-buffer-multibyte toggle with raw bytes + markers + overlay:
/// probes data-loss off-by-one in raw-byte promotion.
#[test]
fn div_cx400_set_buf_multibyte_raw_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 #<killed buffer> \"\\310\\311ABC\" 5 6 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65 66 67))
  (let ((m (set-marker (make-marker) 2))
        (ov (make-overlay 1 4)))
    (overlay-put ov 'face 'bold)
    (set-buffer-multibyte t)
    (list (marker-position m)
          (marker-buffer m)
          (buffer-string)
          (length (buffer-string))
          (point-max)
          (overlay-start ov)
          (overlay-end ov))))
"##,
        expect,
    );
}

/// Error-message quote style (curly vs straight) through deep signal paths:
/// replace-regexp-in-string with invalid backref, wrong-type-argument,
/// and void-variable inside unwind-protect.
#[test]
fn div_cx400_error_quote_style_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"abc\" \"abc\" does-not-exist)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (errors)
  (push (condition-case e
            (replace-regexp-in-string "x" "\\1" "abc")
          (error (cadr e)))
        errors)
  (push (condition-case e
            (replace-regexp-in-string "x" "\\g<foo>" "abc")
          (error (cadr e)))
        errors)
  (push (condition-case e
            (let ((f (lambda () (signal 'void-variable '(does-not-exist)))))
              (unwind-protect (funcall f)
                (message "cleanup")))
          (error (cadr e)))
        errors)
  (nreverse errors))
"##,
        expect,
    );
}

/// Composition: compose-region then find-composition after
/// insert/delete + overlay addition.
#[test]
fn div_cx400_composition_insert_delete_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((3 5 t) (3 4 nil) nil #(\"ab😀Y😁😂cd\" 2 3 (composition ((2 . \"X\"))) 4 5 (composition ((2 . \"X\")))) 1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "ab\U0001F600\U0001F601\U0001F602cd")
  (compose-region 3 5 "X")
  (let ((ov (make-overlay 2 6)))
    (overlay-put ov 'face 'italic))
  (let ((comp-before (find-composition 3)))
    (goto-char 4)
    (insert "Y")
    (let ((comp-after (find-composition 3))
          (comp-moved (find-composition 6)))
      (list comp-before comp-after comp-moved
            (buffer-string)
            (length (overlays-at 3))))))
"##,
        expect,
    );
}

/// Bidi auto-detection with mixed RTL/LTR content + string operations:
/// probes the known RTL auto-detection breakage.
#[test]
fn div_cx400_bidi_rtl_auto_string_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (left-to-right 15 12 16)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc العربية 123")
  (let ((dir (current-bidi-paragraph-direction)))
    (list dir
          (current-column)
          (progn (goto-char 1) (forward-word 2) (point))
          (progn (goto-char 1) (forward-word 3) (point)))))
"##,
        expect,
    );
}

/// Charset-plist: verify built-in charset plist keys match GNU
/// (Neomacs returns incomplete plist — missing :name :code-space
/// :iso-final-char :emacs-mule-id :ascii-compatible-p :code-offset).
#[test]
fn div_cx400_charset_plist_completeness() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument charsetp utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((pl (charset-plist cs)))
            (list cs
                  (plist-get pl :name)
                  (plist-get pl :code-space)
                  (plist-get pl :iso-final-char)
                  (plist-get pl :emacs-mule-id)
                  (plist-get pl :ascii-compatible-p))))
        '(ascii latin-iso8859-1 eight-bit utf-8))
"##,
        expect,
    );
}

/// Weak hash-table + garbage-collect + print + read roundtrip.
#[test]
fn div_cx400_weak_hash_gc_print_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 1 t 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((ht (make-hash-table :weakness 'key :test 'eq))
       (k1 (cons 1 nil))
       (k2 (cons 2 nil)))
  (puthash k1 :val1 ht)
  (puthash k2 :val2 ht)
  (garbage-collect)
  (let ((count-before (hash-table-count ht)))
    (setq k1 nil)
    (garbage-collect)
    (let ((count-after (hash-table-count ht))
          (printed (prin1-to-string ht)))
      (list count-before count-after
            (> (length printed) 10)
            (hash-table-count ht)))))
"##,
        expect,
    );
}

/// Syntax-table override + case-fold + forward-word: probes
/// case-symbols-as-words interaction with syntax.
#[test]
fn div_cx400_syntax_case_fold_word_motion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t 12)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-symbols-as-words t))
  (with-temp-buffer
    (insert "foo_bar baz_qux hello_world")
    (let ((results (list)))
      (goto-char 1)
      (push (forward-word 1) results)
      (push (forward-word 1) results)
      (push (forward-word 1) results)
      (push (point) results)
      (nreverse results))))
"##,
        expect,
    );
}

/// Buffer-display-table glyph substitution + current-column +
/// display property combo.
#[test]
fn div_cx400_display_table_column_display_prop() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((dt (make-display-table)))
    (aset dt ?a (vector (make-glyph-code ?X 'bold)))
    (set-window-display-table (selected-window) dt))
  (insert "abc abc")
  (put-text-property 5 6 'display "YY")
  (list (current-column)
        (progn (forward-char 2) (current-column))
        (buffer-string)))
"##,
        expect,
    );
}

/// Process + filter + multibyte + coding system: receives multibyte
/// output through filter, compares with expected.
#[test]
fn div_cx400_process_filter_multibyte_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""ERR (error \"Cannot convert character at index 3 to unibyte\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((received nil)
      (sent (string-to-unibyte "café世界\n")))
  (let ((proc (make-process :name "neo-cx400-p"
                            :command '("sh" "-c" "printf 'café世界\n'")
                            :connection-type 'pipe :buffer nil
                            :coding 'utf-8-unix
                            :filter (lambda (p s) (push s received)))))
    (accept-process-output proc 2))
  (let ((output (apply #'concat (nreverse received))))
    (list (string-bytes output) (length output)
          (string= output "café世界\n"))))
"##,
        expect,
    );
}

/// face-id + face-attribute + foreground/background across
/// multiple faces: probes the face-id offset + attribute diff.
#[test]
fn div_cx400_face_id_attribute_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 \"unspecified-fg\" \"unspecified-bg\") (1 \"unspecified-fg\" \"unspecified-bg\") (2 \"unspecified-fg\" \"unspecified-bg\") (13 \"unspecified-fg\" \"unspecified-bg\") (25 \"unspecified-fg\" \"unspecified-bg\") (43 \"unspecified-fg\" \"gray\") (31 \"unspecified-fg\" \"unspecified-bg\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (f)
          (list (face-id f)
                (face-attribute f :foreground nil 'default)
                (face-attribute f :background nil 'default)))
        '(default bold italic region mode-line fringe header-line))
"##,
        expect,
    );
}

/// unwind-protect + save-excursion + with-temp-buffer + marker
/// lifetime: marker position after deep nesting.
#[test]
fn div_cx400_unwind_excursion_marker_depth() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 \"abcdef\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((m (make-marker)))
  (set-marker m 5)
  (unwind-protect
      (save-excursion
        (with-temp-buffer
          (insert "abcdef")
          (set-marker m 3)
          (list (marker-position m) (buffer-string))))
    (list (marker-position m) (marker-buffer m))))
"##,
        expect,
    );
}

/// Advice + apply-partially + closure + recursion: deep
/// advice chain with partial application.
#[test]
fn div_cx400_advice_apply_partially_recursion() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (273 5 (5 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((lexical-binding t)
      (calls ()))
  (defun neo-cx400-fact (n acc)
    (push (list n acc) calls)
    (if (<= n 1) acc
      (neo-cx400-fact (1- n) (* n acc))))
  (advice-add 'neo-cx400-fact :around
              (lambda (fn n acc &rest _)
                (apply fn (list n (1+ acc)))))
  (let ((partial (apply-partially 'neo-cx400-fact 5)))
    (list (funcall partial 1)
          (length calls)
          (car (last calls)))))
"##,
        expect,
    );
}

/// cl-loop with hash-table accumulate destructure:
/// multiple accumulation clauses with sorting.
#[test]
fn div_cx400_cl_loop_hash_accumulate_multi() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash "alpha" 3 ht)
  (puthash "beta" 7 ht)
  (puthash "gamma" 1 ht)
  (puthash "delta" 5 ht)
  (cl-loop for k being the hash-keys of ht using (hash-values v)
           if (oddp v) collect (cons k v) into odd
           else collect (cons k v) into even
           do (message "kv: %s %s" k v)
           finally (return (list (sort odd (lambda (a b) (string< (car a) (car b))))
                                 (sort even (lambda (a b) (string< (car a) (car b))))))))
"##,
        expect,
    );
}

/// Narrowing + invisible + intangible + field + motion:
/// probes complex property navigation divergence.
#[test]
fn div_cx400_narrow_invisible_intangible_field() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aa bb cc dd ee ff")
  (put-text-property 4 7 'invisible t)
  (put-text-property 10 13 'intangible t)
  (let ((field-bounds (list 1 4 7 10 13 16)))
    (put-text-property (nth 0 field-bounds) (nth 1 field-bounds) 'field 'a)
    (put-text-property (nth 2 field-bounds) (nth 3 field-bounds) 'field 'b)
    (put-text-property (nth 4 field-bounds) (nth 5 field-bounds) 'field 'c))
  (narrow-to-region 2 15)
  (goto-char 1)
  (let ((results (list)))
    (push (condition-case e (forward-word 1) (error (car e))) results)
    (push (condition-case e (forward-word 1) (error (car e))) results)
    (push (point) results)
    (nreverse results)))
"##,
        expect,
    );
}
