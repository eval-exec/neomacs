//! Complex combo batch 227 — `char` properties deep: `get-char-code-property`
//! with `general-category`, `bidi-class`, `decomposition`, `decimal-digit-value`,
//! `digit-value`, `numeric-value`, `mirrored`, `old-name`, `iso-10646-comment`.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx227_char_code_property_general_category_full() {
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
    );
}

#[test]
fn div_cx227_char_numeric_value_and_digit() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((48 0 0 0) (49 1 1 1) (53 5 5 5) (57 9 9 9) (8551 8 nil nil) (8547 4 nil nil) (8555 12 nil nil) (189 0.5 nil nil) (188 0.25 nil nil) (190 0.75 nil nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c)
          (list c
                (get-char-code-property c 'numeric-value)
                (get-char-code-property c 'decimal-digit-value)
                (get-char-code-property c 'digit-value)))
        '(?0 ?1 ?5 ?9
          ?Ⅷ ?Ⅳ ?Ⅻ
          ?½ ?¼ ?¾))
"##,
        expect,
    );
}

#[test]
fn div_cx227_char_mirrored_property() {
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
    );
}

#[test]
fn div_cx227_char_bidi_class_full_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 L) (65 L) (48 EN) (49 EN) (32 WS) (33 ON) (45 ES) (40 ON) (41 ON) (1488 R) (1489 R) (1490 R) (1491 R) (1492 R) (1575 AL) (1576 AL) (1580 AL) (1583 AL) (1607 AL) (10 B) (9 S))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'bidi-class)))
        '(?a ?A ?0 ?1 ?  ?! ?- ?( ?)
          ?א ?ב ?ג ?ד ?ה
          ?ا ?ب ?ج ?د ?ه
          ?\n ?\t))
"##,
        expect,
    )
}

#[test]
fn div_cx227_char_decomposition_compatibility() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((224 (97 768)) (233 (101 769)) (252 (117 776)) (241 (110 771)) (196 (65 776)) (246 (111 776)) (199 (67 807)) (197 (65 778)) (198 (198)) (338 (338)) (339 (339)) (64257 (compat 102 105)) (64258 (compat 102 108)) (8460 (font 72)) (8451 (compat 176 67)) (12814 (compat 40 4352 4449 41)) (12815 (compat 40 4354 4449 41)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c)
          (let ((d (get-char-code-property c 'decomposition)))
            (list c d)))
        '(?à ?é ?ü ?ñ ?Ä ?ö ?Ç ?Å ?Æ ?Œ ?œ
          ?ﬁ ?ﬂ ?ℌ ?℃ ?㈎ ?㈏))
"##,
        expect,
    );
}

#[test]
fn div_cx227_char_old_name_and_comment() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((224 \"LATIN SMALL LETTER A GRAVE\" nil) (233 \"LATIN SMALL LETTER E ACUTE\" nil) (196 \"LATIN CAPITAL LETTER A DIAERESIS\" nil) (199 \"LATIN CAPITAL LETTER C CEDILLA\" nil) (198 \"LATIN CAPITAL LETTER A E\" nil) (338 \"LATIN CAPITAL LETTER O E\" nil) (8460 \"BLACK-LETTER H\" nil))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (mapcar (lambda (c)
              (list c
                    (get-char-code-property c 'old-name)
                    (get-char-code-property c 'iso-10646-comment)))
            '(?à ?é ?Ä ?Ç ?Æ ?Œ ?ℌ))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx227_char_name_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (\"LATIN SMALL LETTER A\" \"LATIN CAPITAL LETTER A\" \"DIGIT ZERO\" \"SPACE\" \"EXCLAMATION MARK\" \"LEFT PARENTHESIS\" \"RIGHT PARENTHESIS\" \"LATIN SMALL LETTER A WITH GRAVE\" \"LATIN SMALL LETTER E WITH ACUTE\" \"GREEK SMALL LETTER ALPHA\" \"GREEK SMALL LETTER BETA\" \"GREEK CAPITAL LETTER ALPHA\" \"GREEK CAPITAL LETTER BETA\" \"CJK IDEOGRAPH-4E16\" \"CJK IDEOGRAPH-754C\" \"CJK IDEOGRAPH-65E5\" \"GRINNING FACE\" \"PARTY POPPER\" \"EARTH GLOBE EUROPE-AFRICA\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (get-char-code-property c 'name))
        '(?a ?A ?0 ?  ?! ?( ?)
          ?à ?é ?α ?β ?Α ?Β
          ?世 ?界 ?日
          ?😀 ?🎉 ?🌍))
"##,
        expect,
    );
}

#[test]
fn div_cx227_char_script_full_matrix() {
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
          ?À ?É ?Ñ ?Ø ?Þ ?Ð ?Þ))
"##,
        expect,
    );
}

#[test]
fn div_cx227_unicode_property_value_aliases() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'unicode-property-table-internal)
          (char-table-p (category-table))
          (boundp 'char-script-table)
          (fboundp 'char-code-property-description))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx227_char_properties_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((cats (mapcar (lambda (c) (get-char-code-property c 'general-category))
                    '(?a ?A ?0 ?! ?( ?- ?à ?α ?世))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Char properties mega café 世界 test")
    (put-text-property 1 5 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 22)
      (let ((state (list cats
                         (mapcar #'char-script '(?a ?α ?世 ?é))
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
