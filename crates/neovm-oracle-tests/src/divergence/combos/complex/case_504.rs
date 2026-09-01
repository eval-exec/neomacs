/// Batch 504: BOM/coding-system characterization — all utf-8 variants.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx504_bom_utf8_with_sig() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (4 239 187 191)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((f (make-temp-file "cx504-sig-")))
  (let ((coding-system-for-write 'utf-8-with-signature))
    (write-region "x" nil f nil 0))
  (prog1 (with-temp-buffer
           (set-buffer-multibyte nil)
           (insert-file-contents f)
           (let ((b (buffer-string)))
             (list (string-bytes b) (aref b 0) (aref b 1) (aref b 2))))
    (delete-file f)))
"##,
        expect,
    );
}

#[test]
fn div_cx504_bom_utf8_sig_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function find-coding-system)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-p 'utf-8-sig)
      (find-coding-system 'utf-8-sig))
"##,
        expect,
    );
}

#[test]
fn div_cx504_detect_utf8_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((undecided) (undecided) (utf-8 utf-8-auto iso-2022-7bit))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (detect-coding-string "hello")
      (detect-coding-string "cafe")
      (detect-coding-string (string #xef #xbb #xbf 65)))
"##,
        expect,
    );
}

#[test]
fn div_cx504_detect_utf16_bom() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((no-conversion) (no-conversion))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (detect-coding-string (string #xff #xfe 65 0))
      (detect-coding-string (string #xfe #xff 0 65)))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 mule-utf-8 cp65001) (iso-latin-1 iso-8859-1 latin-1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-aliases 'utf-8)
      (coding-system-aliases 'latin-1))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 charset raw-text)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-type 'utf-8)
      (coding-system-type 'latin-1)
      (coding-system-type 'raw-text))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_mnemonic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (85 49)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-mnemonic 'utf-8)
      (coding-system-mnemonic 'latin-1))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_base() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (utf-8 utf-8 utf-8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-base 'utf-8-unix)
      (coding-system-base 'utf-8-dos)
      (coding-system-base 'utf-8-mac))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_eol() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (0 1 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-eol-type 'utf-8-unix)
      (coding-system-eol-type 'utf-8-dos)
      (coding-system-eol-type 'utf-8-mac))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (coding-category-utf-8 coding-category-charset coding-system-error)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (coding-system-category 'utf-8)
      (coding-system-category 'latin-1)
      (condition-case e (coding-system-category 'nonexistent) (error (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx504_coding_system_plist() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (coding-category-utf-8 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (plist-get (coding-system-plist 'utf-8) :category)
      (plist-get (coding-system-plist 'utf-8) :ascii-compatible-p))
"##,
        expect,
    );
}

#[test]
fn div_cx504_encode_coding_string_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" \"cafe\" 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (encode-coding-string "hello" 'utf-8)
      (encode-coding-string "cafe" 'utf-8)
      (string-bytes (encode-coding-string "cafe" 'utf-8)))
"##,
        expect,
    );
}

#[test]
fn div_cx504_decode_coding_string_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"hello\" t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((enc (encode-coding-string "hello" 'utf-8)))
  (list (decode-coding-string enc 'utf-8)
        (string= (decode-coding-string enc 'utf-8) "hello")))
"##,
        expect,
    );
}

#[test]
fn div_cx504_prefer_utf8() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prefer-coding-system 'utf-8)
      (prefer-coding-system 'latin-1))
"##,
        expect,
    );
}

#[test]
fn div_cx504_set_terminal_coding() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e
    (set-terminal-coding-system 'utf-8)
  (error (car e)))
"##,
        expect,
    );
}
