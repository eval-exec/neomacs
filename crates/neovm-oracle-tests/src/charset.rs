//! Oracle parity tests for charset primitives.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

use crate::common::{assert_err_kind, assert_ok_eq, assert_oracle_parity, eval_oracle_and_neovm};

#[test]
fn oracle_prop_charset_basics() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form = "(list (char-charset ?A) (charsetp (char-charset ?A)) (encode-char ?A 'ucs) (decode-char 'ucs #x41) (encode-char ?😀 'ucs) (decode-char 'ucs #x1F600))";
    let expect = expect_test::expect![[r#""OK (ascii t 65 65 128512 128512)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ascii t 65 65 128512 128512)", &oracle, &neovm);
}

#[test]
fn oracle_prop_char_charset_classification() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let form =
        "(list (char-charset ?A) (char-charset ?é) (char-charset ?😀) (char-charset ?\\x80))";
    let expect = expect_test::expect![[r#""OK (ascii unicode-bmp unicode unicode-bmp)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(form, expect);
    assert_ok_eq("(ascii unicode-bmp unicode unicode-bmp)", &oracle, &neovm);
}

#[test]
fn oracle_prop_encode_char_unknown_charset_error() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""ERR (wrong-type-argument charsetp neovm-no-such-charset)""#]];
    let (oracle, neovm) = crate::common::eval_oracle_and_neovm_expect(
        "(encode-char ?A 'neovm-no-such-charset)",
        expect,
    );
    assert_err_kind(&oracle, &neovm, "wrong-type-argument");
}

#[test]
fn oracle_prop_decode_char_out_of_range_error_shape() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""ERR (error \"Not an in-range integer, integral float, or cons of integers\")""#
    ]];
    crate::common::assert_oracle_parity_expect("(decode-char 'ucs -1)", expect);
}

#[test]
fn oracle_prop_iso_charset_designations_and_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Fiso_charset` (charset.c:2089): lookup in `iso_charset_table`,
    // where a later definition wins (`thai-tis620` over `thai-iso8859-11`),
    // plus the `check_iso_charset_parameter` errors.
    let form = "(list (iso-charset 1 94 ?B) (iso-charset 2 94 ?B) (iso-charset 1 96 ?A) \
        (iso-charset 2 94 ?A) (iso-charset 1 94 ?Z) (iso-charset 1 96 ?T) \
        (condition-case e (iso-charset 0 94 ?B) (error e)) \
        (condition-case e (iso-charset 1 1 ?A) (error e)) \
        (condition-case e (iso-charset 1 94 ?/) (error e)) \
        (condition-case e (iso-charset nil 94 ?B) (error e)) \
        (condition-case e (iso-charset 1 94 -1) (error e)))";
    let expect = expect_test::expect![[
        r#""OK (ascii japanese-jisx0208 latin-iso8859-1 chinese-gb2312 nil thai-tis620 (error \"Invalid DIMENSION 0, it should be 1, 2, or 3\") (error \"Invalid CHARS 1, it should be 94 or 96\") (error \"Invalid FINAL-CHAR ‘/’, it should be ‘0’..‘~’\") (wrong-type-argument fixnump nil) (wrong-type-argument characterp -1))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_declare_equiv_charset_and_alias_designations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Fdeclare_equiv_charset` (charset.c:1428) stores CHARSET's id in
    // `iso_charset_table`, so an alias stands for its target. GNU
    // `Fdefine_charset_alias` (charset.c:1289) only rebinds a name, so a
    // charset whose name becomes an alias keeps its designation. CHARSET is
    // checked before the ISO parameters.
    let form = "(list (progn (declare-equiv-charset 1 94 ?B 'unicode) (iso-charset 1 94 ?B)) \
        (progn (define-charset-alias 'oracle-equiv-alias 'latin-iso8859-1) \
        (declare-equiv-charset 1 94 ?B 'oracle-equiv-alias) (iso-charset 1 94 ?B)) \
        (progn (define-charset-alias 'latin-iso8859-1 'ascii) (iso-charset 1 96 ?A)) \
        (condition-case e (declare-equiv-charset 0 94 ?B 'nope) (error e)) \
        (condition-case e (declare-equiv-charset 0 94 ?B 'ascii) (error e)))";
    let expect = expect_test::expect![[
        r#""OK (unicode latin-iso8859-1 latin-iso8859-1 (wrong-type-argument charsetp nope) (error \"Invalid DIMENSION 0, it should be 1, 2, or 3\"))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_charset_full_designation_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Every designation of the dumped `iso_charset_table` (charset.c:103,
    // dumped by `PDUMPER_REMEMBER_SCALAR` at charset.c:2340), walked through
    // `iso-charset` over every valid (DIMENSION, CHARS, FINAL-CHAR).
    let form = "(let (acc) (dolist (d '(1 2 3)) (dolist (c '(94 96)) (let ((f ?0)) \
        (while (<= f ?~) (let ((cs (iso-charset d c f))) (when cs (push (list d c f cs) acc))) \
        (setq f (1+ f)))))) (nreverse acc))";
    let expect = expect_test::expect![[
        r#""OK ((1 94 48 chinese-sisheng) (1 94 49 lao) (1 94 50 arabic-digit) (1 94 51 arabic-1-column) (1 94 52 arabic-2-column) (1 94 53 indian-is13194) (1 94 66 ascii) (1 94 73 katakana-jisx0201) (1 94 74 latin-jisx0201) (1 96 48 ipa) (1 96 49 vietnamese-viscii-lower) (1 96 50 vietnamese-viscii-upper) (1 96 65 latin-iso8859-1) (1 96 66 latin-iso8859-2) (1 96 67 latin-iso8859-3) (1 96 68 latin-iso8859-4) (1 96 70 greek-iso8859-7) (1 96 71 arabic-iso8859-6) (1 96 72 hebrew-iso8859-8) (1 96 76 cyrillic-iso8859-5) (1 96 77 latin-iso8859-9) (1 96 84 thai-tis620) (1 96 86 latin-iso8859-10) (1 96 89 latin-iso8859-13) (1 96 95 latin-iso8859-14) (1 96 98 latin-iso8859-15) (1 96 102 latin-iso8859-16) (2 94 48 chinese-big5-1) (2 94 49 chinese-big5-2) (2 94 51 ethiopic) (2 94 53 indian-2-column) (2 94 54 indian-1-column) (2 94 55 tibetan) (2 94 56 tibetan-1-column) (2 94 64 japanese-jisx0208-1978) (2 94 65 chinese-gb2312) (2 94 66 japanese-jisx0208) (2 94 67 korean-ksc5601) (2 94 68 japanese-jisx0212) (2 94 71 chinese-cns11643-1) (2 94 72 chinese-cns11643-2) (2 94 73 chinese-cns11643-3) (2 94 74 chinese-cns11643-4) (2 94 75 chinese-cns11643-5) (2 94 76 chinese-cns11643-6) (2 94 77 chinese-cns11643-7) (2 94 79 japanese-jisx0213-1) (2 94 80 japanese-jisx0213-2) (2 94 81 japanese-jisx0213.2004-1) (2 96 49 mule-unicode-0100-24ff) (2 96 50 mule-unicode-2500-33ff) (2 96 51 mule-unicode-e000-ffff) (2 96 52 indian-glyph))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_charset_final_char_boundaries_and_type_errors() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `check_iso_charset_parameter` (charset.c:1385-1404): CHECK_FIXNUM on
    // DIMENSION and CHARS, CHECK_CHARACTER on FINAL-CHAR, then the DIMENSION,
    // CHARS and FINAL-CHAR ('0'..'~') range errors in that order.
    let form = "(mapcar (lambda (args) (condition-case e (apply #'iso-charset args) (error e))) \
        '((1 94 ?0) (1 94 ?~) (1 94 127) (1 94 32) (1 94 233) (1.0 94 ?B) (1 nil ?B) \
        (4 94 ?B) (1 95 ?B) (0 1 nil) (1 94 4194304) (3 96 ?B)))";
    let expect = expect_test::expect![[
        r#""OK (chinese-sisheng nil (error \"Invalid FINAL-CHAR ‘\u{7f}’, it should be ‘0’..‘~’\") (error \"Invalid FINAL-CHAR ‘ ’, it should be ‘0’..‘~’\") (error \"Invalid FINAL-CHAR ‘é’, it should be ‘0’..‘~’\") (wrong-type-argument fixnump 1.0) (wrong-type-argument fixnump nil) (error \"Invalid DIMENSION 4, it should be 1, 2, or 3\") (error \"Invalid CHARS 95, it should be 94 or 96\") (wrong-type-argument characterp nil) (wrong-type-argument characterp 4194304) nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_get_unused_iso_final_char_scans_the_designation_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU `Fget_unused_iso_final_char` (charset.c:1407-1425) returns the first
    // private final '0'..'?' whose `iso_charset_table` slot is empty, and nil
    // once all sixteen are taken; `declare-equiv-charset` fills slots.
    let form = "(list (mapcar (lambda (dc) (apply #'get-unused-iso-final-char dc)) \
        '((1 94) (1 96) (2 94) (2 96) (3 94) (3 96))) \
        (condition-case e (get-unused-iso-final-char 4 nil) (error e)) \
        (condition-case e (get-unused-iso-final-char 1 95) (error e)) \
        (progn (declare-equiv-charset 1 94 ?6 'ascii) (get-unused-iso-final-char 1 94)) \
        (progn (dolist (c '(?7 ?8 ?9 ?: 59 ?< ?= ?> ??)) (declare-equiv-charset 1 94 c 'ascii)) \
        (get-unused-iso-final-char 1 94)) \
        (get-unused-iso-final-char 1 96))";
    let expect = expect_test::expect![[
        r#""OK ((54 51 50 48 48 48) (wrong-type-argument fixnump nil) (error \"Invalid CHARS 95, it should be 94 or 96\") 55 nil 51)""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_designation_follows_declare_equiv_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // GNU's ISO-2022 decoder resolves `ESC ( F` through `iso_charset_table`
    // (coding.c:3193 `DECODE_DESIGNATION`), the table `declare-equiv-charset`
    // writes (charset.c:1441), so a declared equivalent takes over the
    // designation.
    let form = r#"(list (let ((s "\e(Z\\")) (progn (declare-equiv-charset 1 94 ?Z 'latin-jisx0201) (let ((d (decode-coding-string s 'iso-2022-7bit))) (list (string-to-list d) (get-text-property 0 'charset d))))) (let ((s "\e(J\\")) (progn (declare-equiv-charset 1 94 ?J 'ascii) (let ((d (decode-coding-string s 'iso-2022-7bit))) (list (string-to-list d) (get-text-property 0 'charset d))))))"#;
    let expect = expect_test::expect![[r#""OK (((165) latin-jisx0201) ((92) nil))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_charset_alias_keeps_captured_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `Fdefine_charset_alias` (charset.c:1289-1300) stores the target's
    // current attribute vector. Rebinding that name replaces only its slot.
    let form = "(progn (define-charset-alias 'probe 'latin-iso8859-1) \
        (define-charset-alias 'latin-iso8859-1 'ascii) \
        (list (charset-id-internal 'probe) (plist-get (charset-plist 'probe) :name) \
          (charset-id-internal 'latin-iso8859-1) (plist-get (charset-plist 'latin-iso8859-1) :name)))";
    let expect = expect_test::expect![[r#""OK (5 latin-iso8859-1 0 ascii)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_alias_keeps_designated_charset() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Declare, then alias. The table holds the id captured at declare time
    // (charset.c:1441), so the data byte still decodes through latin-iso8859-1.
    let form = "(progn (declare-equiv-charset 1 94 ?Z 'latin-iso8859-1) \
        (define-charset-alias 'latin-iso8859-1 'ascii) \
        (let ((d (decode-coding-string \"\\e(ZA\" 'iso-2022-7bit))) \
        (list (string-to-list d) (get-text-property 0 'charset d) (iso-charset 1 94 ?Z))))";
    let expect = expect_test::expect![[r#""OK ((193) latin-iso8859-1 latin-iso8859-1)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_unknown_designation_replays_bytes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `DECODE_DESIGNATION` failure takes `invalid_code` (coding.c:3946):
    // one source byte per output character, not a consumed escape.
    let form = "(string-to-list (decode-coding-string \"\\e(Z\\\\\" 'iso-2022-7bit))";
    let expect = expect_test::expect![[r#""OK (27 40 90 92)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_equiv_without_final_replays_designation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // `declare-equiv-charset` always writes the table (charset.c:1428-1442).
    // A charset with no ISO final is not in `Viso_2022_charset_list`
    // (charset.c:1163-1168), so iso-2022-7bit replays the escape.
    let form = "(progn (declare-equiv-charset 1 94 ?Z 'unicode) \
        (let ((d (decode-coding-string \"\\e(ZA\" 'iso-2022-7bit))) \
        (list (string-to-list d) (get-text-property 0 'charset d) (iso-charset 1 94 ?Z))))";
    let expect = expect_test::expect![[r#""OK ((27 40 90 65) nil unicode)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_encode_ignores_public_alias() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // ISO encode selects from `Viso_2022_charset_list` by id. Aliasing
    // latin-iso8859-1 does not change the bytes for U+00C1.
    let form = "(let ((before (string-to-list (encode-coding-string (string #xC1) 'iso-2022-7bit)))) \
        (define-charset-alias 'latin-iso8859-1 'ascii) \
        (list before (string-to-list (encode-coding-string (string #xC1) 'iso-2022-7bit))))";
    let expect = expect_test::expect![[r#""OK ((27 44 65 65 27 40 66) (27 44 65 65 27 40 66))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_redefine_splits_alias_plist_from_table_method() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // charset.c:1114-1154: redefining a name keeps the id, overwrites
    // `charset_table[id]`, and replaces only that name's attribute vector.
    // decode-char / encode-char use the id. charset-plist uses the vector.
    // Old ISO finals stay in the table (charset.c:1163-1168 only inserts).
    let form = "(progn \
        (define-charset 'redef-x \"first\" :dimension 1 :code-space [33 126] :code-offset 500 :iso-final-char ?9) \
        (define-charset-alias 'redef-probe 'redef-x) \
        (set-charset-plist 'redef-x (append (charset-plist 'redef-x) '(:probe-mark 1))) \
        (let ((shared (list \
            (eq (charset-id-internal 'redef-probe) (charset-id-internal 'redef-x)) \
            (eq (charset-plist 'redef-probe) (charset-plist 'redef-x)) \
            (plist-get (charset-plist 'redef-probe) :probe-mark) \
            (plist-get (charset-plist 'redef-probe) :code-offset) \
            (decode-char 'redef-probe 65) \
            (decode-char 'redef-x 65) \
            (eq (iso-charset 1 94 ?9) 'redef-x)))) \
          (define-charset 'redef-x \"second\" :dimension 1 :code-space [33 126] :code-offset 800 :iso-final-char ?8) \
          (list shared \
            (eq (charset-id-internal 'redef-probe) (charset-id-internal 'redef-x)) \
            (eq (charset-plist 'redef-probe) (charset-plist 'redef-x)) \
            (plist-get (charset-plist 'redef-probe) :code-offset) \
            (plist-get (charset-plist 'redef-x) :code-offset) \
            (plist-get (charset-plist 'redef-probe) :iso-final-char) \
            (plist-get (charset-plist 'redef-x) :iso-final-char) \
            (plist-get (charset-plist 'redef-probe) :probe-mark) \
            (plist-get (charset-plist 'redef-x) :probe-mark) \
            (decode-char 'redef-probe 65) \
            (decode-char 'redef-x 65) \
            (encode-char 532 'redef-probe) \
            (encode-char 532 'redef-x) \
            (encode-char 832 'redef-probe) \
            (encode-char 832 'redef-x) \
            (eq (iso-charset 1 94 ?9) 'redef-x) \
            (eq (iso-charset 1 94 ?8) 'redef-x) \
            (progn \
              (set-charset-plist 'redef-x (append (charset-plist 'redef-x) '(:after 2))) \
              (list (plist-get (charset-plist 'redef-probe) :after) \
                (plist-get (charset-plist 'redef-x) :after) \
                (plist-get (charset-plist 'redef-probe) :code-offset) \
                (decode-char 'redef-probe 65))))))";
    let expect = expect_test::expect![[
        r#""OK ((t t 1 500 532 532 t) t nil 500 800 57 56 1 nil 832 832 nil nil 65 65 t t (nil 2 500 832))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_redefine_through_alias_publishes_the_defining_name() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // Fdefine_charset_internal keeps the id and replaces the hash slot
    // (charset.c:1114-1154). Fiso_charset returns CHARSET_NAME of that id
    // (charset.c:2089-2103). Ids differ between the two engines, so the form
    // checks that the two names share one id and both finals publish the
    // defining symbol.
    let form = "(progn \
        (define-charset 'cr-base \"base\" :dimension 1 :code-space [33 126] \
          :iso-final-char ?9 :code-offset 200) \
        (define-charset-alias 'cr-probe 'cr-base) \
        (define-charset 'cr-probe \"probe\" :dimension 1 :code-space [33 126] \
          :iso-final-char ?8 :code-offset 300) \
        (list (iso-charset 1 94 ?9) (iso-charset 1 94 ?8) \
          (eq (charset-id-internal 'cr-probe) (charset-id-internal 'cr-base)) \
          (plist-get (charset-plist 'cr-probe) :name) \
          (plist-get (charset-plist 'cr-base) :name)))";
    let expect = expect_test::expect![[r#""OK (cr-probe cr-probe t cr-probe cr-base)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_emits_96_set_from_code_space_width() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // charset.c:912: iso_chars_96 is the first code-space width. [33 128]
    // is a 96-set, so the designation is ESC , rather than ESC (.
    let form = r#"(progn
        (define-charset 'cr-wide "wide"
          :dimension 1 :code-space [33 128]
          :iso-final-char ?W :code-offset #xE000 :ascii-compatible-p nil)
        (define-coding-system 'cr-iso-wide "wide probe"
          :coding-type 'iso-2022 :mnemonic ?W :charset-list '(ascii cr-wide)
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (let ((ch (decode-char 'cr-wide 33)))
          (list ch (char-charset ch)
            (iso-charset 1 96 ?W) (iso-charset 1 94 ?W)
            (string-to-list (encode-coding-string (char-to-string ch) 'cr-iso-wide)))))"#;
    let expect =
        expect_test::expect![[r#""OK (57344 unicode-bmp cr-wide nil (27 44 87 33 27 40 66))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_rejects_charset_absent_from_the_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // setup_iso_safe_charsets (coding.c:2856-2917). iso-2022-jp does not list
    // latin-iso8859-1, so ESC ( Z is replayed. iso-2022-7bit is full support
    // and decodes the byte.
    let form = r#"(progn
        (declare-equiv-charset 1 94 ?Z 'latin-iso8859-1)
        (list (string-to-list (decode-coding-string "\e(ZA" 'iso-2022-jp))
          (string-to-list (decode-coding-string "\e(ZA" 'iso-2022-7bit))))"#;
    let expect = expect_test::expect![[r#""OK ((27 40 90 65) (193))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_dimension_three_uses_c2_twice() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // coding.c:3911 validates the third byte and assembles c2 twice.
    // ESC ( Y ! ! ! and ESC ( Y ! ! A are the same character.
    let form = r#"(progn
        (define-charset 'cr-3d "three"
          :dimension 3 :code-space [33 126 33 126 33 126]
          :iso-final-char ?Y :code-offset #x300000)
        (declare-equiv-charset 1 94 ?Y 'cr-3d)
        (define-coding-system 'cr-iso-3d "3d probe"
          :coding-type 'iso-2022 :mnemonic ?Y :charset-list 'iso-2022
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (mapcar (lambda (s) (list (string-to-list s) (length s)))
          (list (decode-coding-string "\e(Y!!!" 'cr-iso-3d)
            (decode-coding-string "\e(Y!!A" 'cr-iso-3d))))"#;
    let expect = expect_test::expect![[r#""OK (((3145728) 1) ((3145728) 1))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_rejects_control_continuation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // coding.c:3898-3910 rejects a continuation below 0x20 before the
    // position code is assembled. The first graphic byte is replayed and
    // ESC ( B is decoded on its own.
    let form = r#"(progn
        (define-charset 'cr-open "open"
          :dimension 2 :code-space [0 255 0 255]
          :iso-final-char ?X :code-offset 0)
        (declare-equiv-charset 1 94 ?X 'cr-open)
        (define-coding-system 'cr-iso-open "open probe"
          :coding-type 'iso-2022 :mnemonic ?X :charset-list '(ascii cr-open)
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (list (string-to-list (decode-coding-string "\e(XA\e(B" 'cr-iso-open))
          (string-to-list (decode-coding-string "\e(X!!" 'cr-iso-open))))"#;
    let expect = expect_test::expect![[r#""OK ((65) (8481))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_define_keeps_alias_identity() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // coding.c:11048-11057 copies the charset id at define time. Rebinding
    // the alias does not change a later decode of that coding system.
    let form = r#"(progn
        (define-charset-alias 'latin-probe 'latin-iso8859-1)
        (define-coding-system 'cr-iso-alias "alias probe"
          :coding-type 'iso-2022 :mnemonic ?A
          :charset-list '(ascii latin-probe)
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (let ((before (string-to-list (decode-coding-string "\e,AA" 'cr-iso-alias))))
          (define-charset-alias 'latin-probe 'ascii)
          (list before
            (string-to-list (decode-coding-string "\e,AA" 'cr-iso-alias))
            (eq (charset-id-internal 'latin-probe)
                (charset-id-internal 'latin-iso8859-1)))))"#;
    let expect = expect_test::expect![[r#""OK ((193) (193) nil)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_keeps_membership_after_final_is_cleared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // charset.c:1163-1168 appends to Viso_2022_charset_list only for a new
    // definition that already has a final. Clearing the final leaves the
    // old table slot and the list membership.
    let form = r#"(progn
        (define-charset 'cr-z "z"
          :dimension 1 :code-space [33 126] :iso-final-char ?Z :code-offset 200)
        (define-coding-system 'cr-iso-z "z probe"
          :coding-type 'iso-2022 :mnemonic ?Z :charset-list 'iso-2022
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (define-charset 'cr-z "z2"
          :dimension 1 :code-space [33 126] :code-offset 500)
        (list (iso-charset 1 94 ?Z)
          (charset-iso-final-char 'cr-z)
          (string-to-list (decode-coding-string "\e(ZA" 'cr-iso-z))
          (decode-char 'cr-z 65)))"#;
    let expect = expect_test::expect![[r#""OK (cr-z -1 (532) 532)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_rejects_a_charset_that_never_had_a_final() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // declare-equiv-charset writes the table (charset.c:1428-1442) without
    // appending to Viso_2022_charset_list. Full support still replays.
    let form = r#"(progn
        (define-charset 'cr-never "never"
          :dimension 1 :code-space [33 126] :code-offset 700)
        (declare-equiv-charset 1 94 ?Y 'cr-never)
        (define-coding-system 'cr-iso-never "never probe"
          :coding-type 'iso-2022 :mnemonic ?Y :charset-list 'iso-2022
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (list (iso-charset 1 94 ?Y)
          (string-to-list (decode-coding-string "\e(YA" 'cr-iso-never))))"#;
    let expect = expect_test::expect![[r#""OK (cr-never (27 40 89 65))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_rejects_a_final_added_by_redefinition() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // A later definition that adds a final does not join
    // Viso_2022_charset_list (charset.c:1167-1168).
    let form = r#"(progn
        (define-charset 'cr-late "late"
          :dimension 1 :code-space [33 126] :code-offset 800)
        (define-charset 'cr-late "late"
          :dimension 1 :code-space [33 126] :iso-final-char ?W :code-offset 800)
        (define-coding-system 'cr-iso-late "late probe"
          :coding-type 'iso-2022 :mnemonic ?W :charset-list 'iso-2022
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (list (iso-charset 1 94 ?W)
          (string-to-list (decode-coding-string "\e(WA" 'cr-iso-late))))"#;
    let expect = expect_test::expect![[r#""OK (cr-late (27 40 87 65))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_resets_g0_after_a_rejected_continuation() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // invalid_code (coding.c:3946-3961) emits one byte and puts ASCII back
    // in G0. The two bangs are not a character of the rejected charset.
    let form = r#"(progn
        (define-charset 'cr-open "open"
          :dimension 2 :code-space [0 255 0 255]
          :iso-final-char ?X :code-offset 0)
        (declare-equiv-charset 1 94 ?X 'cr-open)
        (define-coding-system 'cr-iso-open "open probe"
          :coding-type 'iso-2022 :mnemonic ?X :charset-list '(ascii cr-open)
          :designation [(ascii t) nil nil nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation))
        (list (string-to-list (decode-coding-string "\e(XA\n!!" 'cr-iso-open))
          (string-to-list (decode-coding-string "\e(XA\e(B" 'cr-iso-open))
          (string-to-list (decode-coding-string "\e(X!!" 'cr-iso-open))))"#;
    let expect = expect_test::expect![[r#""OK ((65 10 33 33) (65) (8481))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn oracle_prop_iso_2022_single_shift_is_one_character() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    // coding.c:3711-3726 reads the SS2 graphic bytes in the same character
    // as the escape. A finished string that ends inside that character emits
    // the unconsumed bytes.
    let form = r#"(progn
        (define-charset 'cr-3d "three"
          :dimension 3 :code-space [33 126 33 126 33 126]
          :iso-final-char ?Y :code-offset #x300000)
        (define-coding-system 'cr-iso-ss "ss probe"
          :coding-type 'iso-2022 :mnemonic ?S :charset-list 'iso-2022
          :designation [(ascii t) nil cr-3d nil]
          :flags '(short ascii-at-eol ascii-at-cntl 7-bit designation single-shift))
        (mapcar (lambda (s) (string-to-list (decode-coding-string s 'cr-iso-ss)))
          '("\eN!!!" "\eN!!")))"#;
    let expect = expect_test::expect![[r#""OK ((3145728) (27 78 33 33))""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
