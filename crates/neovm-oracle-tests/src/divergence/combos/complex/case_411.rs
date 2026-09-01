//! Complex combo batch 411 — 20 probes targeting deeper implementation
//! gaps: current-message, inhibit-message, combine-change-calls,
//! with-silent-modifications, define-globalized-minor-mode, derived-mode-p,
//! abbrev table operations, char-table-subtype/extra-slot, map-char-table,
//! char-table-range, syntax-table-p, copy-syntax-table, category-table-p,
//! define-prefix-command, global-set-key with vector, combine-after-change-calls,
//! header-line-format, tab-line-format, input-method functions,
//! and encode-char/decode-char with UCS and legacy charsets.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

/// current-message / inhibit-message: echo area message state.
#[test]
fn div_cx411_current_inhibit_message() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (message "test message")
  (let ((msg (current-message))
        (inhibit-message t))
    (message "suppressed")
    (list msg
          (current-message))))
"##,
        expect,
    );
}

/// combine-change-calls: batching buffer change notifications.
#[test]
fn div_cx411_combine_change_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"inalREPL\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "original")
  (combine-change-calls 1 (point-max)
    (delete-region 1 5)
    (insert "REPL"))
  (buffer-string))
"##,
        expect,
    );
}

/// with-silent-modifications: suppressing modification flags.
#[test]
fn div_cx411_with_silent_modifications() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t #(\"hello world\" 0 5 (face bold)) bold)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "hello")
  (with-silent-modifications
    (insert " world")
    (put-text-property 1 6 'face 'bold))
  (list (buffer-modified-p)
        (buffer-string)
        (get-text-property 1 'face)))
"##,
        expect,
    );
}

/// define-globalized-minor-mode: creating a global minor mode
/// that toggles a buffer-local mode in all buffers.
#[test]
fn div_cx411_define_globalized_minor_mode() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(progn
  (define-minor-mode neo-cx411-local-mode
    "test local mode" :lighter " T411")
  (define-globalized-minor-mode neo-cx411-global-mode
    neo-cx411-local-mode neo-cx411-local-mode-on)
  (defun neo-cx411-local-mode-on ()
    (neo-cx411-local-mode 1))
  (list (fboundp 'neo-cx411-local-mode)
        (fboundp 'neo-cx411-global-mode)))
"##,
        expect,
    );
}

/// derived-mode-p / provided-mode-derived-p: mode hierarchy.
#[test]
fn div_cx411_derived_mode_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (text-mode nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (text-mode)
  (list (derived-mode-p 'text-mode)
        (derived-mode-p 'fundamental-mode)
        (derived-mode-p 'emacs-lisp-mode)))
"##,
        expect,
    );
}

/// copy-abbrev-table / define-abbrev / abbrev-symbol / abbrev-expansion.
#[test]
fn div_cx411_abbrev_table_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (teh \"the\" dont \"don't\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((tab (make-abbrev-table)))
  (define-abbrev tab "teh" "the")
  (define-abbrev tab "dont" "don't" nil 1)
  (list (abbrev-symbol "teh" tab)
        (abbrev-expansion "teh" tab)
        (abbrev-symbol "dont" tab)
        (abbrev-expansion "dont" tab)))
"##,
        expect,
    );
}

/// char-table-subtype / char-table-extra-slot:
/// introspecting char table metadata.
#[test]
fn div_cx411_char_table_subtype_extra() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (category-table test-slot)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'category-table)))
  (set-char-table-extra-slot ct 0 'test-slot)
  (list (char-table-subtype ct)
        (char-table-extra-slot ct 0)))
"##,
        expect,
    );
}

/// map-char-table: iterating char table entries.
#[test]
fn div_cx411_map_char_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments set-char-table-range 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'syntax-table ?w))
      (count 0))
  (set-char-table-range ct ?a ?z ?x)
  (map-char-table (lambda (range val) (setq count (1+ count))) ct)
  count)
"##,
        expect,
    );
}

/// char-table-range: querying range values.
#[test]
fn div_cx411_char_table_range_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""ERR (wrong-number-of-arguments set-char-table-range 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-char-table 'syntax-table ?w)))
  (set-char-table-range ct ?a ?z ?x)
  (list (char-table-range ct ?a)
        (char-table-range ct ?m)
        (char-table-range ct ?A)))
"##,
        expect,
    );
}

/// syntax-table-p / copy-syntax-table:
/// type checking and copying syntax tables.
#[test]
fn div_cx411_syntax_table_p_copy() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments char-syntax 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((st (make-syntax-table))
      (st2 (copy-syntax-table (syntax-table))))
  (list (syntax-table-p st)
        (syntax-table-p '(not a syntax table))
        (eq st2 (syntax-table))
        (char-syntax ?a st2)))
"##,
        expect,
    );
}

/// category-table-p: type checking for category tables.
#[test]
fn div_cx411_category_table_p() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((ct (make-category-table)))
  (list (category-table-p ct)
        (category-table-p (category-table))
        (category-table-p 'not-a-cat-table)))
"##,
        expect,
    );
}

/// define-prefix-command: creating prefix command symbols.
#[test]
fn div_cx411_define_prefix_command() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t (keymap))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((sym (make-symbol "neo-cx411-prefix")))
  (define-prefix-command sym)
  (list (commandp sym)
        (keymapp (symbol-value sym))
        (symbol-function sym)))
"##,
        expect,
    );
}

/// global-set-key with vector argument.
#[test]
fn div_cx411_global_set_key_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil elisp-byte-compile-buffer)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((map (make-sparse-keymap)))
  (define-key map [?\C-c ?\C-f] 'forward-char)
  (define-key map [?\C-c ?\C-b] 'backward-char)
  (list (key-binding [?\C-c ?\C-f] nil nil map)
        (key-binding [?\C-c ?\C-b] nil nil map)))
"##,
        expect,
    );
}

/// combine-after-change-calls: change notification batching.
#[test]
fn div_cx411_combine_after_change_calls() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"oreAFT\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (buffer-enable-undo)
  (insert "before")
  (combine-after-change-calls
    (delete-region 1 4)
    (insert "AFT"))
  (buffer-string))
"##,
        expect,
    );
}

/// header-line-format: formatting the header line.
#[test]
fn div_cx411_header_line_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "body text")
  (let ((header-line-format "HEADER: %b"))
    (list (stringp (format-mode-line header-line-format))
          (format-mode-line header-line-format))))
"##,
        expect,
    );
}

/// tab-line-format: tab line formatting.
#[test]
fn div_cx411_tab_line_format() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t \"\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "body")
  (let ((tab-line-format "TAB: %b"))
    (list (stringp (format-mode-line tab-line-format))
          (format-mode-line tab-line-format))))
"##,
        expect,
    );
}

/// input-method functions: current-input-method / activate.
#[test]
fn div_cx411_input_method_queries() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function current-input-method)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (current-input-method)
      (input-method-name)
      (input-method-after-insert-chunk-hook))
"##,
        expect,
    );
}

/// encode-char / decode-char with UCS and legacy charsets.
#[test]
fn div_cx411_encode_decode_char_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (65 105 65 nil void-variable 128512)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (encode-char ?A 'ascii)
      (encode-char ?é 'latin-iso8859-1)
      (decode-char 'ascii 65)
      (decode-char 'latin-iso8859-1 233)
      (condition-case e (decode-char 'ucs 0x1F600) (error (car e)))
      (condition-case e (encode-char #x1F600 'ucs) (error (car e))))
"##,
        expect,
    );
}

/// syntax-after / syntax-class / syntax-describe:
/// querying syntax at position.
#[test]
fn div_cx411_syntax_after_class() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((4 . 41) (2) (0) (4 . 93) 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(with-temp-buffer
  (insert "(a b) [c d]")
  (list (syntax-after 1)
        (syntax-after 2)
        (syntax-after 6)
        (syntax-after 7)
        (syntax-class (syntax-after 1))))
"##,
        expect,
    );
}

/// upcase-initials / capitalize with multibyte edge cases.
#[test]
fn div_cx411_upcase_initial_capitalize_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"Hello World\" \"Café World\" \"Café Straße Über\" \"Αβγ Δέ\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (upcase-initials "hello world")
      (upcase-initials "café world")
      (capitalize "café straße über")
      (upcase-initials "αβγ δέ"))
"##,
        expect,
    );
}
