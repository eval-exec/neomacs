//! Complex combo batch 404 — 20 more divergence probes building on
//! the 16 confirmed divergences from 400-403, plus fresh interaction
//! surfaces: BOM on coding-system alias vs full name, encode-time
//! float rejection across all slots, overlay-lists after undo+insert,
//! set-buffer-multibyte with mixed raw/multibyte, string-collate case
//! ordering with mixed case lists, display :space with :relative-width,
//! vertical-motion column after display substitution, window-text-pixel-size
//! with :space display, process with :coding and :filter, search-backward
//! with case-fold at boundaries, buffer-local-variables missing entries,
//! face-attribute after face-remap with inheritance, regexp-opt with
//! multibyte, :stipple face attribute, line-move-visual with display,
//! substring with display property + copy, and character-fold search.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// BOM presence: both utf-8-with-signature and utf-8-sig should work.
/// (Neomacs accepts utf-8-sig but writes no BOM; GNU errors on it.)
#[test]
fn div_cx404_bom_utf8_sig_vs_full_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 4 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-temp-file "neo-bom3-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "x" nil f nil 0))
  (prog1 (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents f)
           (let* ((bytes (buffer-string))
                  (bom (and (>= (length bytes) 4)
                            (= (aref bytes 0) #xef)
                            (= (aref bytes 1) #xbb)
                            (= (aref bytes 2) #xbf))))
             (list bom (string-bytes bytes) (length bytes))))
    (ignore-errors (delete-file f))))
"##,
        expect,
    );
}

/// overlay-lists: after undo, verify before/after counts.
#[test]
fn div_cx404_overlay_lists_after_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (user-error \"No further undo information\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "abcdefgh")
  (let ((o1 (make-overlay 2 5)) (o2 (make-overlay 5 8)))
    (overlay-put o1 'face 'bold)
    (overlay-put o2 'face 'italic)
    (let ((before (list (length (car (overlay-lists)))
                        (length (cdr (overlay-lists))))))
      (undo)
      (let ((after (list (length (car (overlay-lists)))
                         (length (cdr (overlay-lists)))
                         (length (overlays-in 1 10)))))
        (list before after)))))
"##,
        expect,
    );
}

/// set-buffer-multibyte: mix of raw bytes and existing multibyte content.
#[test]
fn div_cx404_set_buf_multibyte_mixed_content() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"héllo \\310\\311A\" 9 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "héllo ")
  (set-buffer-multibyte nil)
  (insert (unibyte-string 200 201 65))
  (set-buffer-multibyte t)
  (list (buffer-string) (length (buffer-string)) (point-max)))
"##,
        expect,
    );
}

/// string-collate-lessp: ordering across mixed uppercase/lowercase
/// with and without case-fold.
#[test]
fn div_cx404_string_collate_mixed_case_sort() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((\"äpfel\" \"Äpfel\" \"apple\" \"Apple\" \"banana\" \"Banana\" \"zebra\" \"Zebra\") (\"Äpfel\" \"äpfel\" \"Apple\" \"apple\" \"Banana\" \"banana\" \"Zebra\" \"zebra\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((words '("Apple" "apple" "Banana" "banana" "Äpfel" "äpfel" "Zebra" "zebra")))
  (list (sort (copy-sequence words) #'string-collate-lessp)
        (sort (copy-sequence words) (lambda (a b) (string-collate-lessp a b nil t)))))
"##,
        expect,
    );
}

/// display :space with :relative-width: column accounting should
/// scale relative to the default font width.
#[test]
fn div_cx404_display_relative_width_column() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (7 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc")
  (put-text-property 2 3 'display '(space :relative-width 5))
  (list (current-column)
        (progn (goto-char 2) (current-column))
        (progn (move-to-column 10) (point))))
"##,
        expect,
    );
}

/// vertical-motion column after display substitution across line.
#[test]
fn div_cx404_vertical_motion_column_after_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 6 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a bc def")
  (put-text-property 2 3 'display "XXXXX")
  (goto-char 1)
  (vertical-motion 0)
  (list (current-column)
        (progn (forward-char 2) (current-column))
        (progn (vertical-motion 0) (current-column))))
"##,
        expect,
    );
}

/// window-text-pixel-size with :space display spec.
#[test]
fn div_cx404_window_pixel_size_space() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument window-live-p 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "a b c")
  (put-text-property 3 4 'display '(space :width 20))
  (list (car (window-text-pixel-size))
        (cdr (window-text-pixel-size))
        (car (window-text-pixel-size 1 4))
        (cdr (window-text-pixel-size 1 4))))
"##,
        expect,
    );
}

/// Process with :coding utf-8 and :filter receiving multibyte output.
#[test]
fn div_cx404_process_coding_filter_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    crate::common::assert_oracle_parity(
        r##"
(let ((buf (get-buffer-create " *neo-cx404-proc*")))
  (let ((proc (make-process :name "neo-cx404-proc"
                            :command '("sh" "-c" "printf 'café\n世界\n'")
                            :connection-type 'pipe :buffer buf
                            :coding 'utf-8-unix)))
    (set-process-sentinel proc #'ignore)
    (set-process-query-on-exit-flag proc nil)
    (neovm--oracle-settle-process proc))
  (prog1 (with-current-buffer buf
           (string-trim-right (buffer-string)))
    (kill-buffer buf)))
"##,
    );
}

/// search-backward with case-fold at buffer start boundary.
#[test]
fn div_cx404_search_backward_casefold_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 nil 9 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "ΑΒΓ ΔΕΖ ΗΘΙ")
    (list (progn (goto-char (point-max)) (search-backward "αβγ" nil t))
          (progn (search-backward "ΑΒΓ" nil t))
          (progn (goto-char (point-max)) (search-backward "ηθι" nil t))
          (progn (search-backward "ΗΘΙ" nil t)))))
"##,
        expect,
    );
}

/// buffer-local-variables: count difference with various
/// mode-specific locals set.
#[test]
fn div_cx404_buffer_local_vars_mode_diff() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (33 nil (major-mode . text-mode))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (text-mode)
  (let ((locals (buffer-local-variables)))
    (list (length locals)
          (assq 'text-mode locals)
          (assq 'major-mode locals))))
"##,
        expect,
    );
}

/// face-attribute after face-remap with inheritance chain.
#[test]
fn div_cx404_face_remap_inherit_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"blue\" \"yellow\" \"blue\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((base (make-face 'neo-cx404-base))
      (child (make-face 'neo-cx404-child)))
  (set-face-attribute 'neo-cx404-base nil :foreground "blue")
  (set-face-attribute 'neo-cx404-child nil :inherit 'neo-cx404-base :background "yellow")
  (let ((remap (face-remap-add-relative 'neo-cx404-child '((:foreground "red")))))
    (unwind-protect
        (list (face-attribute 'neo-cx404-child :foreground nil 'default)
              (face-attribute 'neo-cx404-child :background nil 'default)
              (face-attribute 'neo-cx404-base :foreground nil 'default))
      (face-remap-remove-relative remap))))
"##,
        expect,
    );
}

/// regexp-opt with multibyte characters.
#[test]
fn div_cx404_regexp_opt_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\\\\(?:café\\\\|straße\\\\|über\\\\)\" \"[αβγ]\" 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (regexp-opt '("café" "straße" "über"))
        (regexp-opt '("α" "β" "γ"))
        (string-match (regexp-opt '("café" "café世界")) "café世界")))
"##,
        expect,
    );
}

/// :stipple face attribute may be handled differently.
#[test]
fn div_cx404_face_stipple_attribute() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"gray1\" \"gray1\" nil \"gray3\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((f (make-face 'neo-cx404-stipple)))
  (set-face-attribute f nil :stipple "gray1")
  (list (face-attribute f :stipple nil 'default)
        (face-stipple f)
        (set-face-stipple f "gray3")
        (face-attribute f :stipple nil 'default)))
"##,
        expect,
    );
}

/// line-move-visual with display property: visual line movement
/// should account for display glyph width.
#[test]
fn div_cx404_line_move_visual_display() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (end-of-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "abc def ghi")
  (put-text-property 3 4 'display "XXXXXXXX")
  (list (line-move-visual 1)
        (point)
        (current-column)))
"##,
        expect,
    );
}

/// substring with display property + copy: display property
/// should be preserved in substring.
#[test]
fn div_cx404_substring_display_prop_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"bcde\" 1 3 (display \"XXXX\")) nil (display \"XXXX\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((s "abcdef"))
  (put-text-property 2 4 'display "XXXX" s)
  (let ((sub (substring s 1 5)))
    (list sub
          (text-properties-at 0 sub)
          (text-properties-at 1 sub))))
"##,
        expect,
    );
}

/// character-fold search with multibyte: char-fold-to-regexp
/// and search with character folding.
#[test]
fn div_cx404_char_fold_search_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"\\\\(?:c[\u{301}\u{302}\u{307}\u{30c}\u{327}]\\\\|[cçćĉċčᶜḉⅽⓒｃ𝐜𝑐𝒄𝒸𝓬𝔠𝕔𝖈𝖼𝗰𝘤𝙘𝚌]\\\\)\\\\(?:a[\u{300}-\u{304}\u{306}-\u{30a}\u{30c}\u{30f}\u{311}\u{323}\u{325}\u{328}]\\\\|[aªà-åāăąǎǟǡǻȁȃȧᵃḁạảấầẩẫậắằẳẵặₐⓐａ𝐚𝑎𝒂𝒶𝓪𝔞𝕒𝖆𝖺𝗮𝘢𝙖𝚊]\\\\)\\\\(?:f\u{307}\\\\|[fᶠḟⓕｆ𝐟𝑓𝒇𝒻𝓯𝔣𝕗𝖋𝖿𝗳𝘧𝙛𝚏]\\\\)\\\\(?:e[\u{300}-\u{304}\u{306}-\u{309}\u{30c}\u{30f}\u{311}\u{323}\u{327}\u{328}\u{32d}\u{330}]\\\\|[eè-ëēĕėęěȅȇȩᵉḕḗḙḛḝẹẻẽếềểễệₑℯⅇⓔｅ𝐞𝑒𝒆𝓮𝔢𝕖𝖊𝖾𝗲𝘦𝙚𝚎]\\\\)\" nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (with-temp-buffer
    (insert "café")
    (list (char-fold-to-regexp "cafe")
          (re-search-forward (char-fold-to-regexp "cafe") nil t))))
"##,
        expect,
    );
}

/// encode-time with nil slots and partial lists.
#[test]
fn div_cx404_encode_time_partial_slots() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK ((26965 65360) wrong-type-argument wrong-type-argument)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (condition-case e (encode-time '(0 0 0 1 1 2026 nil nil nil)) (error (car e)))
      (condition-case e (encode-time nil) (error (car e)))
      (condition-case e (encode-time '()) (error (car e))))
"##,
        expect,
    );
}

/// regexp-replace with multibyte and case-fold across
/// string boundaries.
#[test]
fn div_cx404_replace_regexp_multibyte_casefold() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"CAFÉ CAFÉ\" \"STRASSE STRASSE\" \"A a\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((case-fold-search t))
  (list (replace-regexp-in-string "café" "CAFÉ" "Café café")
        (replace-regexp-in-string "straße" "STRASSE" "Straße straße")
        (replace-regexp-in-string "α" "a" "Α α")))
"##,
        expect,
    );
}
