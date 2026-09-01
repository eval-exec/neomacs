//! Complex combo batch 355 — `char-code-property` ultimate matrix:
//! general-category, bidi-class, decomposition, numeric-value, digit-value,
//! mirrored, name across Latin/Greek/CJK/emoji/RTL/combining chars.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx355_char_code_property_general_category_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 Ll) (65 Lu) (48 Nd) (49 Nd) (32 Zs) (33 Po) (44 Po) (46 Po) (63 Po) (40 Ps) (41 Pe) (45 Pd) (224 Ll) (233 Ll) (252 Ll) (241 Ll) (196 Lu) (231 Ll) (197 Lu) (198 Lu) (338 Lu) (945 Ll) (946 Ll) (947 Ll) (913 Lu) (914 Lu) (915 Lu) (19990 Lo) (30028 Lo) (26085 Lo) (26412 Lo) (35486 Lo) (1488 Lo) (1489 Lo) (1490 Lo) (1575 Lo) (1576 Lo) (1580 Lo) (10 Cc) (9 Cc) (95 Pc) (34 Po) (39 Po) (92 Po) (35 Po) (36 Sc) (37 Po) (38 Po) (42 Po) (43 Sm) (60 Sm) (62 Sm) (64 Po) (47 Po) (124 Sm) (126 Sm) (94 Sk))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'general-category)))
        '(?a ?A ?0 ?1 ?  ?! ?, ?. ?? ?( ?) ?-
          ?à ?é ?ü ?ñ ?Ä ?ç ?Å ?Æ ?Œ
          ?α ?β ?γ ?Α ?Β ?Γ
          ?世 ?界 ?日 ?本 ?語
          ?א ?ב ?ג ?ا ?ب ?ج
          ?\n ?\t ?_ ?" ?' ?\\ ?# ?$ ?% ?& ?* ?+ ?< ?> ?@ ?/ ?| ?~ ?^))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_code_property_numeric_and_digit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((48 0 0 0) (49 1 1 1) (53 5 5 5) (57 9 9 9))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c)
          (list c
                (get-char-code-property c 'numeric-value)
                (get-char-code-property c 'decimal-digit-value)
                (get-char-code-property c 'digit-value)))
        '(?0 ?1 ?5 ?9))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_code_property_mirrored() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((40 Y) (41 Y) (91 Y) (93 Y) (123 Y) (125 Y) (60 Y) (62 Y) (171 Y) (187 Y) (8249 Y) (8250 Y) (97 N) (65 N) (48 N) (32 N) (33 N))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'mirrored)))
        '(?( ?) ?[ ?] ?{ ?} ?< ?> ?« ?» ?‹ ?› ?a ?A ?0 ?  ?!))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_code_property_bidi_class_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 L) (65 L) (48 EN) (49 EN) (32 WS) (33 ON) (45 ES) (40 ON) (41 ON) (1488 R) (1489 R) (1490 R) (1491 R) (1492 R) (1575 AL) (1576 AL) (1580 AL) (1583 AL) (1607 AL))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'bidi-class)))
        '(?a ?A ?0 ?1 ?  ?! ?- ?( ?)
          ?א ?ב ?ג ?ד ?ה
          ?ا ?ب ?ج ?د ?ه))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_code_property_decomposition_full() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((224 (97 768)) (233 (101 769)) (252 (117 776)) (241 (110 771)) (196 (65 776)) (246 (111 776)) (199 (67 807)) (197 (65 778)) (198 (198)) (338 (338)) (339 (339)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c)
          (let ((d (get-char-code-property c 'decomposition)))
            (list c d)))
        '(?à ?é ?ü ?ñ ?Ä ?ö ?Ç ?Å ?Æ ?Œ ?œ))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_code_property_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"LATIN SMALL LETTER A\" \"LATIN CAPITAL LETTER A\" \"DIGIT ZERO\" \"SPACE\" \"EXCLAMATION MARK\" \"LEFT PARENTHESIS\" \"RIGHT PARENTHESIS\" \"LATIN SMALL LETTER A WITH GRAVE\" \"LATIN SMALL LETTER E WITH ACUTE\" \"GREEK SMALL LETTER ALPHA\" \"GREEK SMALL LETTER BETA\" \"GREEK CAPITAL LETTER ALPHA\" \"GREEK CAPITAL LETTER BETA\" \"CJK IDEOGRAPH-4E16\" \"CJK IDEOGRAPH-754C\" \"CJK IDEOGRAPH-65E5\" \"GRINNING FACE\" \"PARTY POPPER\" \"EARTH GLOBE EUROPE-AFRICA\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (get-char-code-property c 'name))
        '(?a ?A ?0 ?  ?! ?( ?)
          ?à ?é ?α ?β ?Α ?Β
          ?世 ?界 ?日 ?😀 ?🎉 ?🌍))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_script_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-script c)))
        '(?a ?A ?0 ?  ?! ?( ?-
          ?à ?é ?ñ ?Ç ?Æ
          ?α ?β ?Ω
          ?世 ?界 ?日 ?本 ?語 ?中 ?国 ?한 ?글
          ?א ?ב ?ا ?ب
          ?À ?É ?Ñ ?Ø ?Þ ?Ð))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_width_emoji_and_variation_selectors() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((65038 0) (65039 0) (8205 0) (8419 0) (97 1) (19990 2) (128512 2) (127881 2) (127757 2))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-width c)))
        '(#xFE0E #xFE0F #x200D #x20E3 ?a ?世 ?😀 ?🎉 ?🌍))
"##,
        expect,
    )
}

#[test]
fn div_cx355_string_width_with_emoji_and_zwj() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (2 4 14 12 0 1 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (string-width "😀")
      (string-width "🎉🌍")
      (string-width "hello 😀 world")
      (string-width "café 世界 😀")
      (string-width "")
      (length "😀")
      (string-bytes "😀"))
"##,
        expect,
    )
}

#[test]
fn div_cx355_char_props_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cats (mapcar (lambda (c) (get-char-code-property c 'general-category))
                    '(?a ?A ?0 ?! ?( ?- ?à ?α ?世))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Char-code-property mega café 世界 😀 test")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 22)
      (let ((state (list cats
                         (mapcar #'char-script '(?a ?α ?世 ?é))
                         (mapcar #'char-width '(?a ?世 ?😀))
                         (get-char-code-property ?à 'decomposition)
                         (get-char-code-property ?世 'name)
                         (buffer-string)
                         (marker-position m)
                         (overlay-start ov) (overlay-end ov)
                         (text-properties-at 1))))
        (undo)
        (widen()
        (list state (buffer-string) (marker-position m)
              (overlay-start ov) (overlay-end ov)
              (text-properties-at 1))))))
"##,
        expect,
    )
}
