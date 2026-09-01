//! Complex combo batch 178 — `charset` / `coding-system` registry deep
//! dive: all coding-system predicates, charset dimension, code-space,
//! priority list ordering.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx178_coding_system_p_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 t) (utf-8-unix t) (utf-8-with-signature t) (latin-1 t) (iso-8859-1 t) (iso-8859-9 t) (utf-16 t) (utf-16le t) (utf-16be t) (big5 t) (gb2312 t) (no-conversion t) (undecided t) (binary t) (invalid-cs nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (coding-system-p cs)))
        '(utf-8 utf-8-unix utf-8-with-signature
          latin-1 iso-8859-1 iso-8859-9
          utf-16 utf-16le utf-16be
          big5 gb2312 no-conversion
          undecided binary invalid-cs))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_type_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 utf-8) (latin-1 charset) (iso-8859-9 charset) (utf-16 utf-16) (big5 big5) (gb2312 iso-2022))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (coding-system-type cs) (error :err))))
        '(utf-8 latin-1 iso-8859-9 utf-16 big5 gb2312))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_mnemonic_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 85) (utf-8-unix 85) (latin-1 49) (iso-8859-9 57) (utf-16 85) (big5 66))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (coding-system-mnemonic cs) (error :err))))
        '(utf-8 utf-8-unix latin-1 iso-8859-9 utf-16 big5))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_category_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 coding-category-utf-8) (utf-8-with-signature coding-category-utf-8-sig) (latin-1 coding-category-charset) (iso-8859-7 coding-category-charset) (emacs-mule coding-category-emacs-mule) (utf-16 coding-category-utf-16-auto) (utf-16be coding-category-utf-16-be-nosig) (utf-16le coding-category-utf-16-le-nosig) (big5 coding-category-big5) (no-conversion coding-category-raw-text) (raw-text coding-category-raw-text) (undecided coding-category-undecided) (binary coding-category-raw-text))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (coding-system-category cs) (error :err))))
        '(utf-8 utf-8-with-signature latin-1 iso-8859-7
          emacs-mule utf-16 utf-16be utf-16le big5
          no-conversion raw-text undecided binary))
"##,
        expect,
    );
}

#[test]
fn div_cx178_charset_plist_complete_keys() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii 1 ascii \"ASCII\" \"ASCII (ISO646 IRV)\" \"ASCII (ISO646 IRV)\" [0 127 0 0 0 0 0 0]) (unicode 3 unicode \"Unicode\" \"Unicode (ISO10646)\" \"Unicode (ISO10646)\" [0 255 0 255 0 16 0 0]) (eight-bit 1 eight-bit \"Raw bytes\" nil \"Raw bytes 128-255\" [128 255 0 0 0 0 0 0]) (iso-8859-1 1 iso-8859-1 \"Latin-1\" \"Latin-1\" \"Latin-1 (ISO/IEC 8859-1)\" [0 255 0 0 0 0 0 0]) (latin-iso8859-1 1 latin-iso8859-1 \"RHP of Latin-1\" \"RHP of ISO/IEC 8859/1 (Latin-1): ISO-IR-100\" \"Right-Hand Part of ISO/IEC 8859/1 (Latin-1): ISO-IR-100\" [32 127]) (mule-unicode-0100-24ff 2 mule-unicode-0100-24ff \"Unicode subset\" \"Unicode subset (U+0100..U+24FF)\" \"Unicode characters of the range U+0100..U+24FF.\" [32 127 32 127]))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (let ((p (condition-case e (charset-plist cs) (error nil))))
            (list cs
                  (plist-get p :dimension)
                  (plist-get p :name)
                  (plist-get p :short-name)
                  (plist-get p :long-name)
                  (plist-get p :docstring)
                  (plist-get p :code-space))))
        '(ascii unicode eight-bit iso-8859-1
          latin-iso8859-1 mule-unicode-0100-24ff))
"##,
        expect,
    );
}

#[test]
fn div_cx178_charset_dimension_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((ascii 1) (unicode 3) (eight-bit 1) (iso-8859-1 1) (latin-iso8859-1 1))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (charset-dimension cs) (error :err))))
        '(ascii unicode eight-bit iso-8859-1 latin-iso8859-1))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_priority_list_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'coding-category-list)
      (consp coding-category-list)
      (fboundp 'set-coding-priority))
"##,
        expect,
    );
}

#[test]
fn div_cx178_charset_chars_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((ascii 128) (unicode 256) (iso-8859-1 256))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (cs)
              (list cs (condition-case e (charset-chars cs) (error :err))))
            '(ascii unicode iso-8859-1))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_aliases_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((utf-8 (utf-8 mule-utf-8 cp65001)) (latin-1 (iso-latin-1 iso-8859-1 latin-1)) (iso-8859-9 (iso-latin-5 iso-8859-9 latin-5)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (cs)
          (list cs (condition-case e (coding-system-aliases cs) (error :err))))
        '(utf-8 latin-1 iso-8859-9))
"##,
        expect,
    );
}

#[test]
fn div_cx178_coding_system_put_get_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((:ascii-compatible-p t :category coding-category-utf-8 :name utf-8 :docstring \"UTF-8 (no signature (BOM))\" :coding-type utf-8 :mnemonic 85 :charset-list (unicode) :mime-charset utf-8) utf-8 utf-8)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (coding-system-plist 'utf-8)
          (plist-get (coding-system-plist 'utf-8) :name)
          (plist-get (coding-system-plist 'utf-8) :mime-charset))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx178_charset_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cs 'utf-8)
      (charset 'ascii))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert (format "Charset/coding mega: %s/%s" charset cs))
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 8))
          (ov (make-overlay 4 14)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 18)
      (let ((state (list (coding-system-p cs)
                         (coding-system-type cs)
                         (charset-dimension charset)
                         (charset-plist charset)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen)
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    );
}
