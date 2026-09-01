//! Complex combo batch 203 — `ucs-grapheme-cluster` / `char-fold-table` /
//! `search-default-mode` / `char-script-table` queries.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx203_char_fold_table_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (boundp 'char-fold-table)
          (char-table-p (if (boundp 'char-fold-table) char-fold-table nil))
          (fboundp 'char-fold-make-table))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx203_search_default_mode_query() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t nil t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(list (boundp 'search-default-mode)
      search-default-mode
      (fboundp 'char-fold-to-regexp))
"##,
        expect,
    );
}

#[test]
fn div_cx203_ucs_grapheme_cluster_availability() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (list (fboundp 'ucs-grapheme-cluster)
          (fboundp 'grapheme-cluster)
          (fboundp 'find-grapheme-clusters)
          (boundp 'grapheme-cluster-function))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_fold_search_with_accents() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (with-temp-buffer
      (insert "café naïve résumé piñata")
      (let ((case-fold-search t))
        (goto-char 1)
        (list (search-forward "cafe" nil t)
              (search-forward "naive" nil t)
              (search-forward "resume" nil t)
              (search-forward "pinata" nil t))))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_fold_to_regexp_basic() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (t 0 0)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(condition-case e
    (let ((re (char-fold-to-regexp "cafe")))
      (list (stringp re)
            (string-match re "cafe")
            (string-match re "café")))
  (error (list :errored (car e))))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_script_table_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (char-script c)))
        '(?a ?A ?0 ?α ?β ?世 ?界 ?日 ?本 ?語
          ?À ?É ?Ñ ?Ü ?ß ?Ç ?Å ?Æ ?Ø))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_general_category_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 Ll) (65 Lu) (48 Nd) (32 Zs) (33 Po) (44 Po) (46 Po) (40 Ps) (41 Pe) (45 Pd) (945 Ll) (945 Ll) (233 Ll) (49 Nd) (95 Pc) (10 Cc) (9 Cc))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'general-category)))
        '(?a ?A ?0 ?  ?! ?, ?. ?( ?) ?-
          ?α ?α ?é ?1 ?_ ?\n ?\t))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_bidi_class_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((97 L) (65 L) (48 EN) (32 WS) (33 ON) (45 ES) (40 ON) (41 ON) (1488 R) (1489 R) (1490 R) (1491 R) (1575 AL) (1576 AL) (1580 AL) (1583 AL))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c) (list c (get-char-code-property c 'bidi-class)))
        '(?a ?A ?0 ?  ?! ?- ?( ?)
          ?א ?ב ?ג ?ד
          ?ا ?ب ?ج ?د))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_decomposition_matrix() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK ((224 (97 768)) (233 (101 769)) (252 (117 776)) (241 (110 771)) (196 (65 776)) (246 (111 776)) (199 (67 807)) (197 (65 778)) (198 (198)) (338 (338)) (339 (339)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"
(mapcar (lambda (c)
          (let ((decomp (get-char-code-property c 'decomposition)))
            (list c decomp)))
        '(?à ?é ?ü ?ñ ?Ä ?ö ?Ç ?Å ?Æ ?Œ ?œ))
"##,
        expect,
    );
}

#[test]
fn div_cx203_char_fold_with_marker_overlay_undo_narrow_mega() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function char-script)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"
(let ((scripts (mapcar (lambda (c) (char-script c))
                        '(?a ?α ?世 ?é ?0 ?!))))
  (with-temp-buffer
    (buffer-enable-undo)
    (insert "Char-fold mega café 世界 test")
    (put-text-property 1 6 'face 'bold)
    (let ((m (set-marker (make-marker) 10))
          (ov (make-overlay 4 18)))
      (overlay-put ov 'face 'italic)
      (overlay-put ov 'evaporate t)
      (narrow-to-region 2 22)
      (let ((state (list scripts
                         (boundp 'char-fold-table)
                         (boundp 'search-default-mode)
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
