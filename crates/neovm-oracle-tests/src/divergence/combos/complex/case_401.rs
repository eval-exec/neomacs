//! Complex combo batch 401 — 20 fresh probes targeting additional divergence
//! themes: selective-display column accounting, char-width-table mutation
//! ignored, display :eval property, overlay category inheritance,
//! vertical-motion + display glyph, window-text-pixel-size, buffer-local
//! variables with overlays, substitute-command-keys, format-message deep
//! paths, encode-time float precision, hash-table equal+vectors,
//! decode-char eight-bit, category table + char property,
//! parse-partial-sexp + comment/string + narrowing, event basic-type, and
//! bool-vector count/op.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// selective-display: ^L (page-break) used as display separator;
/// column/line counting may differ from GNU.
#[test]
fn div_cx401_selective_display_column_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (3 3 3 17 \"aaa\\nbbb\\fccc\\fddd\\neee\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa\nbbb\014ccc\014ddd\neee")
  (let ((selective-display t))
    (list (count-lines (point-min) (point-max))
          (line-number-at-pos (point-max))
          (current-column)
          (progn (goto-char (point-min))
                 (forward-line 2)
                 (point))
          (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect,
    );
}

/// char-width-table mutation: Neomacs ignores custom char widths;
/// test via string-width + format with padding.
#[test]
fn div_cx401_char_width_table_ignored_pad() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 1 \"   a\" \"  ab\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'char-width-table 2)))
  (set-char-table-range ct ?a 5)
  (set-char-table-range ct ?b 3)
  (list (string-width "ab")
        (string-width "a")
        (format "%4s" "a")
        (format "%4s" "ab")))
"##,
        expect,
    );
}

/// Display :eval property — dynamic display evaluation where the
/// display string is computed at display time. Neomacs may evaluate
/// differently or not at all.
#[test]
fn div_cx401_display_eval_property() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((eval (prog1 (format \"[%d]\" counter) (setq counter (1+ counter)))) 0)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((counter 0))
  (with-temp-buffer
    (insert "abc")
    (put-text-property 2 3 'display '(eval (prog1 (format "[%d]" counter) (setq counter (1+ counter)))))
    (list (get-text-property 2 'display)
          counter)))
"##,
        expect,
    );
}

/// Overlay category inheritance: overlay with `category` property
/// should inherit other properties from the category symbol's plist.
#[test]
fn div_cx401_overlay_category_inherit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (bold highlight nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((my-cat (intern "neo-cx401-cat")))
  (setplist my-cat '(face bold mouse-face highlight))
  (with-temp-buffer
    (insert "abcdef")
    (let ((ov (make-overlay 2 5)))
      (overlay-put ov 'category my-cat)
      (list (overlay-get ov 'face)
            (overlay-get ov 'mouse-face)
            (overlay-get ov 'priority)))))
"##,
        expect,
    );
}

/// vertical-motion + display property + overlay: cursor motion
/// should account for display glyph widths.
#[test]
fn div_cx401_vertical_motion_display_glyph() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 8 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "x  y  z")
  (put-text-property 2 3 'display "ABCDEF")
  (let ((ov (make-overlay 5 6)))
    (overlay-put ov 'display "G"))
  (list (progn (vertical-motion 0) (current-column))
        (progn (goto-char 1) (vertical-motion 1) (point))
        (progn (goto-char 3) (vertical-motion 0) (current-column))))
"##,
        expect,
    );
}

/// window-text-pixel-size with display property: Neomacs may report
/// different pixel dimensions for text with display substitutions.
#[test]
fn div_cx401_window_text_pixel_size_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((0 . 0) 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc xyz")
  (put-text-property 2 3 'display "WWWW")
  (put-text-property 6 7 'display "ZZ")
  (list (window-text-pixel-size)
        (car (window-text-pixel-size))
        (cdr (window-text-pixel-size))))
"##,
        expect,
    );
}

/// buffer-local-variables with overlays and text properties active.
#[test]
fn div_cx401_buffer_local_vars_overlay_active() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((neo-cx401-local . 42) (buffer-file-name) 21)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (let ((local-sym (intern "neo-cx401-local")))
    (set (make-local-variable local-sym) 42)
    (insert "abcdef")
    (put-text-property 1 4 'face 'bold)
    (let ((ov (make-overlay 2 5))) (overlay-put ov 'face 'italic))
    (let ((locals (buffer-local-variables)))
      (list (assq local-sym locals)
            (assq 'buffer-file-name locals)
            (length locals)))))
"##,
        expect,
    );
}

/// substitute-command-keys with keymaps and keybindings:
/// may produce different formatted command descriptions.
#[test]
fn div_cx401_substitute_command_keys_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK \"\\nUses keymap ‘map’, which is not currently defined.\\n\"""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map (kbd "C-c C-f") 'forward-word)
  (define-key map (kbd "C-c C-b") 'backward-word)
  (substitute-command-keys "\\{map}"))
"##,
        expect,
    );
}

/// format-message quote style in deeper nested error paths:
/// wrong-number-of-arguments, invalid-function, etc.
#[test]
fn div_cx401_format_message_error_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 f car nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let (errors)
  (push (condition-case e (funcall 1 2 3) (error (cadr e))) errors)
  (push (condition-case e (let ((f 5)) (f 1)) (error (cadr e))) errors)
  (push (condition-case e (car 1 2 3) (error (cadr e))) errors)
  (push (condition-case e
            (with-temp-buffer
              (let ((parse-sexp-lookup-properties t))
                (insert "( )")
                (put-text-property 2 3 'syntax-table (string-to-syntax ")"))
                (forward-sexp 1)))
          (error (cadr e)))
        errors)
  (nreverse errors))
"##,
        expect,
    );
}

/// encode-time float precision: Neomacs float time handling may
/// differ from GNU's high-resolution time arithmetic.
#[test]
fn div_cx401_encode_time_float_seconds() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1781634630.5 1781634630.0 0.5 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((t1 (encode-time 30.5 30 14 16 6 2026 nil))
      (t2 (encode-time 30.0 30 14 16 6 2026 nil)))
  (list (float-time t1)
        (float-time t2)
        (- (float-time t1) (float-time t2))
        (time-equal-p t1 t2)))
"##,
        expect,
    );
}

/// Hash-table with `equal` test and vector keys: vectors should
/// compare by contents for hash lookup.
#[test]
fn div_cx401_hash_equal_vector_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 \"xyz\" \"de\" missing)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ht (make-hash-table :test 'equal)))
  (puthash [1 2 3] "abc" ht)
  (puthash [4 5] "de" ht)
  (puthash (vector 1 2 3) "xyz" ht)
  (list (hash-table-count ht)
        (gethash [1 2 3] ht)
        (gethash (vector 4 5) ht)
        (gethash [99] ht 'missing)))
"##,
        expect,
    );
}

/// decode-char eight-bit charset: known divergence where
/// Neomacs doesn't recognize the eight-bit charset properly.
#[test]
fn div_cx401_decode_char_eight_bit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((4194176 128) (4194208 160) (4194248 200) (4194303 255))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (code)
          (list (decode-char 'eight-bit code)
                (encode-char (decode-char 'eight-bit code) 'eight-bit)))
        '(128 160 200 255))
"##,
        expect,
    );
}

/// Category table + char property: define-category and
/// char-category-set may differ.
#[test]
fn div_cx401_category_table_char_set() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-category-set 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (copy-category-table)))
  (define-category ?x "test category" ct)
  (modify-category-entry ?a ?x ct)
  (modify-category-entry ?b ?x ct)
  (list (char-category-set ?a ct)
        (char-category-set ?c ct)
        (category-docstring ?x ct)
        (char-category-set ?b ct)))
"##,
        expect,
    );
}

/// parse-partial-sexp with comment/string syntax + narrowing +
/// syntax-table text properties.
#[test]
fn div_cx401_parse_partial_sexp_comment_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((0 nil 1 nil nil nil 0 nil nil nil nil) (0 nil 10 nil nil nil 0 nil nil nil nil) (0 nil 18 nil nil nil 0 nil nil nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "alpha /* beta */ gamma (delta) // epsilon")
  (put-text-property 7 8 'syntax-table (string-to-syntax "< b"))
  (put-text-property 17 18 'syntax-table (string-to-syntax "> b"))
  (put-text-property 30 31 'syntax-table (string-to-syntax "< bn"))
  (narrow-to-region 1 25)
  (list (parse-partial-sexp 1 7)
        (parse-partial-sexp 1 12)
        (parse-partial-sexp 1 20)))
"##,
        expect,
    );
}

/// event-basic-type + event-modifiers: event symbol decomposition
/// may differ between Neomacs and GNU.
#[test]
fn div_cx401_event_basic_type_modifiers() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((nil (meta control) t) (nil (control) t) (nil (meta) t) (mouse-1 (shift click) t) (nil (hyper) t) (nil (alt) t))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (e)
          (list (event-basic-type e)
                (event-modifiers e)
                (eventp e)))
        '(C-M-a C-x M-RET S-mouse-1 H-return A-tab))
"##,
        expect,
    );
}

/// bool-vector count of set bits + op with different lengths.
#[test]
fn div_cx401_bool_vector_count_op() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-length-argument 5 5 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let* ((bv1 (bool-vector t nil t nil t))
       (bv2 (bool-vector nil t nil t nil))
       (bv3 (bool-vector t t nil nil t t)))
  (list (bool-vector-count-population bv1)
        (bool-vector-count-consecutive bv1 1 5)
        (bool-vector-count-consecutive bv1 0 5)
        (bool-vector-union bv1 bv2 bv3)
        (bool-vector-intersection bv1 bv2 bv3)
        (bool-vector-subsetp bv1 bv2)
        (bool-vector-subsetp bv3 bv1)))
"##,
        expect,
    );
}

/// buffer-swap-text with markers and overlays: swap text between
/// two buffers and verify marker/overlay attachment.
#[test]
fn div_cx401_buffer_swap_text_marker_overlay() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments buffer-swap-text 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((a (get-buffer-create " *neo-cx401-a*"))
      (b (get-buffer-create " *neo-cx401-b*")))
  (with-current-buffer a
    (insert "AAAAAAAA")
    (let ((m (set-marker (make-marker) 4))
          (ov (make-overlay 2 6)))
      (overlay-put ov 'face 'bold)
      (with-current-buffer b
        (insert "BBBBBBBB"))
      (buffer-swap-text a b)
      (list (with-current-buffer a (buffer-string))
            (with-current-buffer b (buffer-string))
            (marker-position m)
            (marker-buffer m)
            (with-current-buffer a (length (overlays-in 1 10)))
            (with-current-buffer b (length (overlays-in 1 10)))))))
"##,
        expect,
    );
}

/// replace-match with numeric subexpression and string replacement:
/// edge case with backreferences.
#[test]
fn div_cx401_replace_match_numeric_subexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"123\" \"456\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "aaa 123 bbb 456 ccc")
  (goto-char 1)
  (let (results)
    (while (re-search-forward "\\([a-z]+\\) \\([0-9]+\\)" nil t)
      (push (match-string 2) results)
      (replace-match (concat (match-string 1) "=" (match-string 2) )))
    (nreverse results)))
"##,
        expect,
    );
}

/// looking-at + search-backward + case-fold + multibyte chars:
/// regex search with case-folding over Greek text.
#[test]
fn div_cx401_looking_at_search_backward_casefold_greek() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 17 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "ΠΡΩΤΟΣ δευτερος ΤΡΙΤΟΣ")
    (let (results)
      (goto-char 15)
      (push (looking-at "δευτερος") results)
      (push (looking-at "ΔΕΥΤΕΡΟΣ") results)
      (goto-char (point-max))
      (push (search-backward "τριτος" nil t) results)
      (push (search-backward "ΤΡΙΤΟΣ" nil t) results)
      (nreverse results))))
"##,
        expect,
    );
}

/// line-number-at-pos with selective-display + narrowing:
/// edge case for line counting with page breaks.
#[test]
fn div_cx401_line_number_selective_narrow() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 3 3 \"ne1\\nline2\\fline3\\nline4\\flin\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "line1\nline2\014line3\nline4\014line5")
  (narrow-to-region 3 28)
  (let ((selective-display t))
    (list (line-number-at-pos (point-min))
          (line-number-at-pos (point-max))
          (count-lines (point-min) (point-max))
          (buffer-substring-no-properties (point-min) (point-max)))))
"##,
        expect,
    );
}

/// Image/display property :space alignment + column: Neomacs may
/// handle :space display spec differently in column accounting.
#[test]
fn div_cx401_display_space_spec_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (14 2 4 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a b c")
  (put-text-property 3 4 'display '(space :width 10))
  (list (current-column)
        (progn (goto-char 3) (current-column))
        (progn (move-to-column 5) (point))
        (progn (move-to-column 15) (point))))
"##,
        expect,
    );
}

/// string-collate-lessp with locale: sort order may differ in
/// locale-sensitive comparison.
#[test]
fn div_cx401_string_collate_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"a\" \"ä\" \"o\" \"ö\" \"ß\" \"u\" \"ü\" \"z\") (\"a\" \"o\" \"u\" \"z\" \"ß\" \"ä\" \"ö\" \"ü\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((words '("ä" "ö" "ü" "a" "o" "u" "z" "ß")))
  (list (sort (copy-sequence words) #'string-collate-lessp)
        (sort (copy-sequence words) #'string<)))
"##,
        expect,
    );
}
