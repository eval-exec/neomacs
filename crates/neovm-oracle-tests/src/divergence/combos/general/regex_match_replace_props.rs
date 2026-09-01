//! Divergence tests: regex + match-data + replace + text-property + overlay combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_regex_replace_textprop_overlay_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"aaa-REPLACED-bbb-REPLACED-ccc-REPLACED-ddd\" 0 2 (group pre) 3 4 (group tok1) 12 15 (group mid) 16 17 (group tok2) 25 28 (group mid2) 29 30 (group tok3) 38 41 (group post)) ((1 43) (4 5) (17 18) (30 31)) t nil whole t first t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "aaa-FOO-bbb-BAR-ccc-BAZ-ddd")
  (let ((ov1 (make-overlay 1 31))
        (ov2 (make-overlay 4 7))
        (ov3 (make-overlay 12 15))
        (ov4 (make-overlay 20 23)))
    (overlay-put ov1 'scope 'whole)
    (overlay-put ov2 'match 'first)
    (overlay-put ov3 'match 'second)
    (overlay-put ov4 'match 'third)
    (put-text-property 1 3 'group 'pre)
    (put-text-property 4 7 'group 'tok1)
    (put-text-property 8 11 'group 'mid)
    (put-text-property 12 15 'group 'tok2)
    (put-text-property 16 19 'group 'mid2)
    (put-text-property 20 23 'group 'tok3)
    (put-text-property 24 27 'group 'post)
    (goto-char 1)
    (while (re-search-forward "FOO\\|BAR\\|BAZ" nil t)
      (replace-match "REPLACED"))
    (let ((s (buffer-string))
          (ov-s (mapcar (lambda (ov) (list (overlay-start ov) (overlay-end ov)))
                        (sort (overlays-in 1 50)
                              (lambda (a b) (< (overlay-start a)
                                               (overlay-start b)))))))
      (list s ov-s
            (string= s "aaa-REPLACED-bbb-REPLACED-ccc-REPLACED-ddd")
            (= (length s) 40)
            (overlay-get ov1 'scope)
            (eq (overlay-get ov1 'scope) 'whole)
            (overlay-get ov2 'match)
            (eq (overlay-get ov2 'match) 'first))))) "#,
        expect,
    );
}

#[test]
fn divergence_match_data_save_restore() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (search-failed \"beta\")""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha beta gamma delta epsilon")
  (re-search-forward "beta")
  (let ((saved-match (match-data)))
    (list (match-beginning 0) (match-end 0)
          (buffer-substring (match-beginning 0) (match-end 0))
          (string= (buffer-substring (match-beginning 0) (match-end 0)) "beta")
          (re-search-forward "delta")
          (match-beginning 0) (match-end 0)
          (buffer-substring (match-beginning 0) (match-end 0))
          (string= (buffer-substring (match-beginning 0) (match-end 0)) "delta")
          (set-match-data saved-match)
          (match-beginning 0) (match-end 0)
          (buffer-substring (match-beginning 0) (match-end 0))
          (string= (buffer-substring (match-beginning 0) (match-end 0))
                   "beta")))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_narrow_replace() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 1 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG")
  (put-text-property 1 3 'zone 'z1)
  (put-text-property 5 7 'zone 'z2)
  (put-text-property 9 11 'zone 'z3)
  (put-text-property 13 15 'zone 'z4)
  (put-text-property 17 19 'zone 'z5)
  (put-text-property 21 23 'zone 'z6)
  (put-text-property 25 27 'zone 'z7)
  (let ((ov (make-overlay 5 23)))
    (overlay-put ov 'scope 'middle)
    (narrow-to-region 5 23)
    (goto-char (point-min))
    (while (re-search-forward "[A-Z][A-Z][A-Z]" nil t)
      (replace-match "xxx"))
    (let ((narrowed (buffer-string))
          (props (list (get-text-property 1 'zone)
                       (get-text-property 5 'zone))))
      (widen)
      (list narrowed props
            (buffer-string)
            (= (length (buffer-string)) 31)
            (get-text-property 1 'zone)
            (eq (get-text-property 1 'zone) 'z1)
            (get-text-property 25 'zone)
            (eq (get-text-property 25 'zone) 'z7)
            (overlay-get ov 'scope)
            (eq (overlay-get ov 'scope) 'middle))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_groups_backreference() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((7 17 7 12 13 17 \"alpha-beta\" \"alpha\" \"beta\") (\"alpha-beta-alpha-beta\" \"alpha\" \"beta\")) t nil nil nil t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "start-alpha-beta-alpha-beta-end")
  (goto-char 1)
  (let ((result nil))
    (when (re-search-forward "\\(alpha\\)-\\(beta\\)" nil t)
      (push (list (match-beginning 0) (match-end 0)
                  (match-beginning 1) (match-end 1)
                  (match-beginning 2) (match-end 2)
                  (match-string 0) (match-string 1) (match-string 2))
            result))
    (goto-char 1)
    (when (re-search-forward "\\(\\w+\\)-\\(\\w+\\)-\\1-\\2" nil t)
      (push (list (match-string 0) (match-string 1) (match-string 2))
            result))
    (let ((r (nreverse result)))
      (list r
            (= (length r) 2)
            (string= (nth 7 (car r)) "alpha-beta")
            (string= (nth 8 (car r)) "alpha")
            (string= (nth 9 (car r)) "beta")
            (string= (nth 0 (cadr r))
                     "alpha-beta-alpha-beta")
            (string= (nth 1 (cadr r)) "alpha")
            (string= (nth 2 (cadr r)) "beta"))))) "#,
        expect,
    );
}

#[test]
fn divergence_replace_preserve_textprops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"XXXX-FACE-YYYY-FACE-ZZZZ\" 0 3 (style plain) 4 5 (style highlighted) 9 12 (style plain) 13 15 (style bold) 19 21 (style plain)) plain highlighted plain bold bold 5 16 styled t t nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "XXXX-face1-YYYY-face2-ZZZZ")
  (put-text-property 1 4 'style 'plain)
  (put-text-property 5 10 'style 'highlighted)
  (put-text-property 11 14 'style 'plain)
  (put-text-property 15 20 'style 'bold)
  (put-text-property 21 24 'style 'plain)
  (let ((ov (make-overlay 5 20)))
    (overlay-put ov 'tag 'styled)
    (goto-char 1)
    (re-search-forward "face1" nil t)
    (replace-match "FACE" nil t)
    (goto-char 1)
    (re-search-forward "face2" nil t)
    (replace-match "FACE" nil t)
    (let ((s (buffer-string))
          (p1 (get-text-property 1 'style))
          (p2 (get-text-property 5 'style))
          (p3 (get-text-property 10 'style))
          (p4 (get-text-property 14 'style))
          (p5 (get-text-property 15 'style)))
      (list s p1 p2 p3 p4 p5
            (overlay-start ov) (overlay-end ov)
            (overlay-get ov 'tag)
            (eq (overlay-get ov 'tag) 'styled)
            (eq p1 'plain)
            (eq p5 'plain))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_multiline_anchor() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((1 6 #(\"line1\" 0 4 (row 1)) 1) (7 12 #(\"line2\" 0 4 (row 2)) 2) (13 18 #(\"line3\" 0 4 (row 3)) 3) (19 24 #(\"line4\" 0 4 (row 4)) 4) (25 30 #(\"line5\" 0 4 (row 5)) 5)) t t t t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "line1\nline2\nline3\nline4\nline5")
  (put-text-property 1 5 'row 1)
  (put-text-property 7 11 'row 2)
  (put-text-property 13 17 'row 3)
  (put-text-property 19 23 'row 4)
  (put-text-property 25 29 'row 5)
  (goto-char 1)
  (let ((matches nil))
    (while (re-search-forward "^line[0-9]$" nil t)
      (push (list (match-beginning 0)
                  (match-end 0)
                  (match-string 0)
                  (get-text-property (match-beginning 0) 'row))
            matches))
    (let ((r (nreverse matches)))
      (list r
            (= (length r) 5)
            (string= (nth 2 (car r)) "line1")
            (string= (nth 2 (nth 4 r)) "line5")
            (= (nth 3 (car r)) 1)
            (= (nth 3 (nth 4 r)) 5))))) "#,
        expect,
    );
}

#[test]
fn divergence_match_data_interleaved_regex() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument stringp 5)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "cat dog bat cat dog bat")
  (let ((matches nil))
    (goto-char 1)
    (while (re-search-forward "cat\\|dog\\|bat" nil t)
      (push (cons (match-string 0)
                  (cons (match-beginning 0) (match-end 0)))
            matches))
    (let ((r (nreverse matches)))
      (list r
            (= (length r) 6)
            (string= (caar r) "cat")
            (string= (cadr (nth 1 r)) "dog")
            (string= (caar (nth 2 r)) "bat")
            (eq (aref (caar r) 0) ?c)
            (eq (aref (cadr (nth 1 r)) 0) ?d))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_case_fold_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK ((\"Hello\" \"WORLD\" \"foo\" \"BAR\") (\"foo\") t nil (\"Hello\" \"WORLD\" \"foo\" \"BAR\") (\"WORLD\" \"foo\" \"BAR\") (\"foo\" \"BAR\") (\"BAR\") (\"foo\") nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello WORLD foo BAR baz QUX")
  (let ((case-fold-search t)
        (matches nil))
    (goto-char 1)
    (while (re-search-forward "hello\\|world\\|foo\\|bar" nil t)
      (push (match-string 0) matches))
    (let ((r1 (nreverse matches)))
      (setq case-fold-search nil)
      (goto-char 1)
      (let ((matches2 nil))
        (while (re-search-forward "hello\\|world\\|foo\\|bar" nil t)
          (push (match-string 0) matches2))
        (let ((r2 (nreverse matches2)))
          (list r1 r2
                (= (length r1) 4)
                (= (length r2) 2)
                (member "Hello" r1)
                (member "WORLD" r1)
                (member "foo" r1)
                (member "BAR" r1)
                (member "foo" r2)
                (member "bar" r2))))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_word_boundary() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (((1 4 #(\"the\" 0 2 (word w1)) w1)) t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "the theme is theorem and other")
  (put-text-property 1 3 'word 'w1)
  (put-text-property 5 9 'word 'w2)
  (put-text-property 13 20 'word 'w3)
  (put-text-property 25 30 'word 'w4)
  (goto-char 1)
  (let ((matches nil))
    (while (re-search-forward "\\<the\\>" nil t)
      (push (list (match-beginning 0) (match-end 0)
                  (match-string 0)
                  (get-text-property (match-beginning 0) 'word))
            matches))
    (let ((r (nreverse matches)))
      (list r
            (string= (nth 2 (car r)) "the")
            (= (length r) 1)
            (= (car (car r)) 1)
            (= (cadr (car r)) 4)
            (eq (nth 3 (car r)) 'w1))))) "#,
        expect,
    );
}

#[test]
fn divergence_regex_replace_with_fixedcase() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"Greetings World GREETINGS WORLD greetings world\" 10 14 (case title2) 26 30 (case upper2) 42 46 (case lower2)) ((1 10 \"Greetings\") (17 26 \"GREETINGS\") (33 42 \"greetings\")) t nil t nil nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "Hello World HELLO WORLD hello world")
  (put-text-property 1 5 'case 'title)
  (put-text-property 7 11 'case 'title2)
  (put-text-property 13 17 'case 'upper)
  (put-text-property 19 23 'case 'upper2)
  (put-text-property 25 29 'case 'lower)
  (put-text-property 31 35 'case 'lower2)
  (goto-char 1)
  (while (re-search-forward "hello" nil t)
    (replace-match "greetings" nil nil))
  (let ((s (buffer-string)))
    (goto-char 1)
    (let ((matches nil))
      (while (re-search-forward "greetings" nil t)
        (push (list (match-beginning 0) (match-end 0)
                    (match-string 0))
              matches))
      (let ((r (nreverse matches)))
        (list s r
              (= (length r) 3)
              (string= (nth 2 (car r)) "greetings")
              (= (car (car r)) 1)
              (= (car (cadr r)) 13)
              (= (car (nth 2 r)) 25)))))) "#,
        expect,
    );
}
