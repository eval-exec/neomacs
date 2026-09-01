/// Batch 511: display table, glyph, character display edge cases.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx511_display_table_standard() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function copy-display-table)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (copy-display-table (standard-display-table))))
  (display-table-p dt))
"##,
        expect,
    );
}

#[test]
fn div_cx511_display_table_truncation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"a\tb\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((dt (make-display-table)))
  (aset dt ?\^I (vector ?\s ?\s ?\s ?\s))
  (set-window-display-table (selected-window) dt)
  (with-temp-buffer
    (insert "a\tb")
    (buffer-string)))
"##,
        expect,
    );
}

#[test]
fn div_cx511_glyph_code_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4194369 66 88 nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (make-glyph-code ?A 'bold)
      (make-glyph-code ?B nil)
      (glyph-char (make-glyph-code ?X 'italic))
      (glyph-face (make-glyph-code ?Z 'default)))
"##,
        expect,
    );
}

#[test]
fn div_cx511_char_to_string_all() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (wrong-type-argument characterp 134217825)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (char-to-string ?a)
      (char-to-string ?\C-a)
      (char-to-string ?\M-a)
      (char-to-string ?\S-a))
"##,
        expect,
    );
}

#[test]
fn div_cx511_single_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"a\" \"C-x\" \"M-x\" \"S-x\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (single-key-description ?a)
      (single-key-description ?\C-x)
      (single-key-description ?\M-x)
      (single-key-description ?\S-x))
"##,
        expect,
    );
}

#[test]
fn div_cx511_text_char_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"^A\" \"^J\" \"^I\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (text-char-description ?\C-a)
      (text-char-description ?\n)
      (text-char-description ?\t))
"##,
        expect,
    );
}

#[test]
fn div_cx511_key_description_simple() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"C-x C-f\" \"M-x\" \"C-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (key-description "\C-x\C-f")
      (key-description "\M-x")
      (key-description "\C-c"))
"##,
        expect,
    );
}

#[test]
fn div_cx511_lookup_key_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK forward-char""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'forward-char)
  (lookup-key map "a"))
"##,
        expect,
    );
}

#[test]
fn div_cx511_accessible_keymaps_count() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'forward-char)
  (define-key map "b" 'backward-char)
  (length (accessible-keymaps map)))
"##,
        expect,
    );
}

#[test]
fn div_cx511_copy_keymap_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK fn1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((map (make-sparse-keymap)))
  (define-key map "a" 'fn1)
  (define-key map "b" 'fn2)
  (let ((copy (copy-keymap map)))
    (define-key map "a" 'fn3)
    (lookup-key copy "a")))
"##,
        expect,
    );
}

#[test]
fn div_cx511_current_minor_mode_maps() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((maps (current-minor-mode-maps)))
  (list (listp maps)))
"##,
        expect,
    );
}

#[test]
fn div_cx511_minor_mode_key_binding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (fboundp 'minor-mode-key-binding)
      (fboundp 'global-key-binding)
      (fboundp 'local-key-binding))
"##,
        expect,
    );
}

#[test]
fn div_cx511_define_prefix_command_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "cx511-prefix")))
  (define-prefix-command s)
  (list (commandp s) (keymapp (symbol-value s))))
"##,
        expect,
    );
}

#[test]
fn div_cx511_describe_keys_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'help)
  (list (fboundp 'describe-key)
        (fboundp 'describe-bindings)))
"##,
        expect,
    );
}

#[test]
fn div_cx511_use_local_map() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (keymap (97 . forward-word))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (let ((m (make-sparse-keymap)))
    (define-key m "a" 'forward-word)
    (use-local-map m)
    (current-local-map)))
"##,
        expect,
    );
}
