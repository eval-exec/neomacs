//! Divergence tests: string+regexp+textprop+undo+marker deep combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_regexp_replace_propagated_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 32 40)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "foo123bar456baz789qux000")
  (put-text-property 1 3 'kind 'word)
  (put-text-property 4 6 'kind 'num)
  (put-text-property 7 9 'kind 'word)
  (put-text-property 10 12 'kind 'num)
  (put-text-property 13 15 'kind 'word)
  (put-text-property 16 18 'kind 'num)
  (put-text-property 19 21 'kind 'word)
  (put-text-property 22 24 'kind 'num)
  (let ((ov (make-overlay 1 24))
        (m (copy-marker 4 t)))
    (overlay-put ov 'whole t)
    (undo-boundary)
    (goto-char 1)
    (while (re-search-forward "[0-9][0-9][0-9]" nil t)
      (replace-match "NUM"))
    (let ((s (buffer-string)))
      (primitive-undo 1 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "foo123bar456baz789qux000")
            (= (marker-position m) 4)
            (get-text-property 1 'kind) (eq (get-text-property 1 'kind) 'word)
            (get-text-property 4 'kind) (eq (get-text-property 4 'kind) 'num)
            (get-text-property 7 'kind) (eq (get-text-property 7 'kind) 'word)
            (get-text-property 10 'kind) (eq (get-text-property 10 'kind) 'num)
            (get-text-property 13 'kind) (eq (get-text-property 13 'kind) 'word)
            (get-text-property 16 'kind) (eq (get-text-property 16 'kind) 'num)
            (get-text-property 19 'kind) (eq (get-text-property 19 'kind) 'word)
            (get-text-property 22 'kind) (eq (get-text-property 22 'kind) 'num)
            (overlay-get ov 'whole)))))) "#,
        expect,
    );
}

#[test]
fn divergence_string_match_data_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"alpha charlie echo foxtrot\" 0 4 (pos 1) 6 12 (pos 3) 14 17 (pos 5) 19 26 (pos 6)) #(\"alpha bravo charlie delta echo foxtrot\" 0 4 (pos 1) 6 10 (pos 2) 12 18 (pos 3) 20 24 (pos 4) 26 29 (pos 5) 31 38 (pos 6)) t t 1 t 2 t 3 t 4 t 5 t all)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "alpha bravo charlie delta echo foxtrot")
  (put-text-property 1 5 'pos 1)
  (put-text-property 7 11 'pos 2)
  (put-text-property 13 19 'pos 3)
  (put-text-property 21 25 'pos 4)
  (put-text-property 27 30 'pos 5)
  (put-text-property 32 39 'pos 6)
  (let ((m (copy-marker 7 t))
        (ov (make-overlay 1 39)))
    (overlay-put ov 'scope 'all)
    (undo-boundary)
    (string-match "bravo" (buffer-string))
    (let ((ms (match-beginning 0))
          (me (match-end 0)))
      (delete-region ms (+ me 1))
      (undo-boundary)
      (string-match "delta" (buffer-string))
      (let ((ms2 (match-beginning 0))
            (me2 (match-end 0)))
        (delete-region ms2 (+ me2 1))
        (let ((s (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list s
                (buffer-string)
                (string= (buffer-string)
                         "alpha bravo charlie delta echo foxtrot")
                (= (marker-position m) 7)
                (get-text-property 1 'pos) (= (get-text-property 1 'pos) 1)
                (get-text-property 7 'pos) (= (get-text-property 7 'pos) 2)
                (get-text-property 13 'pos) (= (get-text-property 13 'pos) 3)
                (get-text-property 21 'pos) (= (get-text-property 21 'pos) 4)
                (get-text-property 27 'pos) (= (get-text-property 27 'pos) 5)
                (overlay-get ov 'scope))))))) "#,
        expect,
    );
}

#[test]
fn divergence_substring_props_with_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"CDEF\" 0 1 (idx 2) 1 2 (idx 3) 2 3 (idx 4) 3 4 (idx 5)) #(\"ABCDEFGHIJ\" 0 1 (idx 0) 1 2 (idx 1) 2 3 (idx 2) 3 4 (idx 3) 4 5 (idx 4) 5 6 (idx 5) 6 7 (idx 6) 7 8 (idx 7) 8 9 (idx 8) 9 10 (idx 9)) #(\"ABCDEFGHIJ\" 0 1 (idx 0) 1 2 (idx 1) 2 3 (idx 2) 3 4 (idx 3) 4 5 (idx 4) 5 6 (idx 5) 6 7 (idx 6) 7 8 (idx 7) 8 9 (idx 8) 9 10 (idx 9)) t t 0 t 2 t 4 t 7 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((s (propertize "ABCDEFGHIJ" 'idx 0)))
    (dotimes (i 9)
      (put-text-property (+ i 1) (+ i 2) 'idx (+ i 1) s))
    (insert s)
    (let ((ov (make-overlay 1 10))
          (m (copy-marker 5 t)))
      (overlay-put ov 'block t)
      (undo-boundary)
      (let ((sub (buffer-substring 3 7)))
        (delete-region 3 7)
        (undo-boundary)
        (goto-char 3)
        (insert sub)
        (let ((s2 (buffer-string)))
          (primitive-undo 2 buffer-undo-list)
          (list sub
                s2
                (buffer-string)
                (string= (buffer-string) "ABCDEFGHIJ")
                (= (marker-position m) 5)
                (get-text-property 1 'idx) (= (get-text-property 1 'idx) 0)
                (get-text-property 3 'idx) (= (get-text-property 3 'idx) 2)
                (get-text-property 5 'idx) (= (get-text-property 5 'idx) 4)
                (get-text-property 8 'idx) (= (get-text-property 8 'idx) 7)
                (overlay-get ov 'block))))))) "#,
        expect,
    );
}

#[test]
fn divergence_insert_buffer_substring_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (args-out-of-range 19 24)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "SOURCE-TEXT-WITH-PROPS")
  (put-text-property 1 6 'origin 'src)
  (put-text-property 8 11 'origin 'src)
  (put-text-property 13 17 'origin 'src)
  (put-text-property 19 24 'origin 'src)
  (let ((m (copy-marker 1 t))
        (ov (make-overlay 1 24)))
    (overlay-put ov 'source t)
    (undo-boundary)
    (goto-char 25)
    (insert-buffer-substring (current-buffer) 1 11)
    (let ((s (buffer-string))
          (p (get-text-property 26 'origin)))
      (primitive-undo 1 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "SOURCE-TEXT-WITH-PROPS")
            (= (marker-position m) 1)
            (= (buffer-size) 24)
            (get-text-property 1 'origin) (eq (get-text-property 1 'origin) 'src)
            (get-text-property 8 'origin) (eq (get-text-property 8 'origin) 'src)
            (overlay-get ov 'source)))))) "#,
        expect,
    );
}

#[test]
fn divergence_format_propertize_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"apple BLUEBERRY cherry DRAGONFRUIT elderberry \" 0 5 (val 1) 16 22 (val 3) 35 45 (val 5)) #(\"apple banana cherry date elderberry \" 0 5 (val 1) 6 12 (val 2) 13 19 (val 3) 20 24 (val 4) 25 35 (val 5)) t 1 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((items '(("apple" . 1) ("banana" . 2) ("cherry" . 3)
                 ("date" . 4) ("elderberry" . 5))))
    (dolist (item items)
      (let ((s (propertize (car item) 'val (cdr item))))
        (insert s " ")))
    (let ((ov (make-overlay 1 (point)))
          (m (copy-marker 1 t)))
      (overlay-put ov 'list t)
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "banana" nil t)
      (replace-match "BLUEBERRY")
      (undo-boundary)
      (goto-char 1)
      (re-search-forward "date" nil t)
      (replace-match "DRAGONFRUIT")
      (let ((s (buffer-string)))
        (primitive-undo 2 buffer-undo-list)
        (list s
              (buffer-string)
              (= (marker-position m) 1)
              (get-text-property 1 'val) (= (get-text-property 1 'val) 1)
              (overlay-get ov 'list)))))) "#,
        expect,
    );
}

#[test]
fn divergence_re_search_backward_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 40)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG")
  (put-text-property 1 3 'g 1)
  (put-text-property 5 7 'g 2)
  (put-text-property 9 11 'g 3)
  (put-text-property 13 15 'g 4)
  (put-text-property 17 19 'g 5)
  (put-text-property 21 23 'g 6)
  (put-text-property 25 27 'g 7)
  (let ((m (copy-marker 21 t))
        (ov (make-overlay 1 27)))
    (overlay-put ov 'chain t)
    (undo-boundary)
    (goto-char (point-max))
    (re-search-backward "EEE" nil t)
    (replace-match "XXX")
    (undo-boundary)
    (goto-char (point-max))
    (re-search-backward "CCC" nil t)
    (replace-match "YYY")
    (let ((s (buffer-string)))
      (primitive-undo 2 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string) "AAA-BBB-CCC-DDD-EEE-FFF-GGG")
            (= (marker-position m) 21)
            (get-text-property 1 'g) (= (get-text-property 1 'g) 1)
            (get-text-property 5 'g) (= (get-text-property 5 'g) 2)
            (get-text-property 9 'g) (= (get-text-property 9 'g) 3)
            (get-text-property 13 'g) (= (get-text-property 13 'g) 4)
            (get-text-property 17 'g) (= (get-text-property 17 'g) 5)
            (get-text-property 21 'g) (= (get-text-property 21 'g) 6)
            (get-text-property 25 'g) (= (get-text-property 25 'g) 7)
            (overlay-get ov 'chain)))))) "#,
        expect,
    );
}

#[test]
fn divergence_word_bounds_replace_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 34 43)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "the quick brown fox jumps over the lazy dog")
  (let ((words (split-string (buffer-string)))
        (i 0))
    (goto-char 1)
    (while (re-search-forward "\\b\\w+\\b" nil t)
      (put-text-property (match-beginning 0) (match-end 0) 'word-num i)
      (setq i (+ i 1))))
  (let ((ov (make-overlay 1 (buffer-size)))
        (m (copy-marker 11 t)))
    (overlay-put ov 'sentence t)
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "\\<quick\\>" nil t)
    (replace-match "slow")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "\\<brown\\>" nil t)
    (replace-match "red")
    (undo-boundary)
    (goto-char 1)
    (re-search-forward "\\<lazy\\>" nil t)
    (replace-match "energetic")
    (let ((s (buffer-string)))
      (primitive-undo 3 buffer-undo-list)
      (list s
            (buffer-string)
            (string= (buffer-string)
                     "the quick brown fox jumps over the lazy dog")
            (= (marker-position m) 11)
            (get-text-property 1 'word-num) (= (get-text-property 1 'word-num) 0)
            (get-text-property 5 'word-num) (= (get-text-property 5 'word-num) 1)
            (get-text-property 11 'word-num) (= (get-text-property 11 'word-num) 2)
            (overlay-get ov 'sentence)))))) "#,
        expect,
    );
}

#[test]
fn divergence_split_string_join_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"one-two-three-four-five-six-seven\" 0 3 (num 0) 4 7 (num 1) 8 13 (num 2) 14 18 (num 3) 19 23 (num 4) 24 27 (num 5) 28 33 (num 6)) #(\"one,two,three,four,five,six,seven\" 0 3 (num 0) 4 7 (num 1) 8 13 (num 2) 14 18 (num 3) 19 23 (num 4) 24 27 (num 5) 28 33 (num 6)) t t 0 t 1 t 2 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "one,two,three,four,five,six,seven")
  (let ((i 0))
    (goto-char 1)
    (while (re-search-forward "[a-z]+" nil t)
      (put-text-property (match-beginning 0) (match-end 0) 'num i)
      (setq i (+ i 1))))
  (let ((ov (make-overlay 1 (buffer-size)))
        (m (copy-marker 1 t)))
    (overlay-put ov 'csv t)
    (undo-boundary)
    (let ((parts (split-string (buffer-string) ",")))
      (erase-buffer)
      (insert (mapconcat #'identity parts "-"))
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (list s
              (buffer-string)
              (string= (buffer-string) "one,two,three,four,five,six,seven")
              (= (marker-position m) 1)
              (get-text-property 1 'num) (= (get-text-property 1 'num) 0)
              (get-text-property 5 'num) (= (get-text-property 5 'num) 1)
              (get-text-property 9 'num) (= (get-text-property 9 'num) 2)
              (overlay-get ov 'csv)))))) "#,
        expect,
    );
}

#[test]
fn divergence_apply_maps_to_buffer_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (((#(\"a\" 0 1 (idx 0)) . 1) (#(\"b\" 0 1 (idx 1)) . 2) (#(\"c\" 0 1 (idx 2)) . 3) (#(\"d\" 0 1 (idx 3)) . 4) (#(\"e\" 0 1 (idx 4)) . 5) (#(\"f\" 0 1 (idx 5)) . 6) (#(\"g\" 0 1 (idx 6)) . 7) (#(\"h\" 0 1 (idx 7)) . 8) (#(\"i\" 0 1 (idx 8)) . 9) (#(\"j\" 0 1 (idx 9)) . 10)) 10 t #(\"a=1 b=2 c=3 d=4 e=5 f=6 g=7 h=8 i=9 j=10\" 0 1 (idx 0) 4 5 (idx 1) 8 9 (idx 2) 12 13 (idx 3) 16 17 (idx 4) 20 21 (idx 5) 24 25 (idx 6) 28 29 (idx 7) 32 33 (idx 8) 36 37 (idx 9)) #(\"a1 b2 c3 d4 e5 f6 g7 h8 i9 j10\" 0 2 (idx 0) 3 5 (idx 1) 6 8 (idx 2) 9 11 (idx 3) 12 14 (idx 4) 15 17 (idx 5) 18 20 (idx 6) 21 23 (idx 7) 24 26 (idx 8) 27 30 (idx 9)) t t 0 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "a1 b2 c3 d4 e5 f6 g7 h8 i9 j10")
  (let ((i 0))
    (goto-char 1)
    (while (re-search-forward "[a-z][0-9]+" nil t)
      (put-text-property (match-beginning 0) (match-end 0) 'idx i)
      (setq i (+ i 1))))
  (let ((ov (make-overlay 1 (buffer-size)))
        (m (copy-marker 1 t)))
    (overlay-put ov 'data t)
    (undo-boundary)
    (let ((pairs (mapcar (lambda (s)
                           (cons (substring s 0 1)
                                 (string-to-number (substring s 1))))
                         (split-string (buffer-string)))))
      (erase-buffer)
      (insert (mapconcat (lambda (p) (format "%s=%d" (car p) (cdr p))) pairs " "))
      (let ((s (buffer-string)))
        (primitive-undo 1 buffer-undo-list)
        (list pairs (length pairs) (= (length pairs) 10)
              s
              (buffer-string)
              (string= (buffer-string) "a1 b2 c3 d4 e5 f6 g7 h8 i9 j10")
              (= (marker-position m) 1)
              (get-text-property 1 'idx) (= (get-text-property 1 'idx) 0)
              (overlay-get ov 'data)))))) "#,
        expect,
    );
}

#[test]
fn divergence_regexp_narrow_widen_undo() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#(\"FFF-GGG-YYY-III-JJ\" 0 3 (block 6) 4 7 (block 7) 12 15 (block 9) 16 18 (block 10)) #(\"AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ\" 0 3 (block 1) 4 7 (block 2) 8 11 (block 3) 12 15 (block 4) 16 19 (block 5) 20 23 (block 6) 24 27 (block 7) 28 31 (block 8) 32 35 (block 9) 36 39 (block 10)) t nil 1 t 2 t 3 t 4 t 5 t 6 t 7 t t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (insert "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ")
  (dotimes (i 10)
    (let ((start (+ 1 (* i 4)))
          (end (+ 3 (* i 4))))
      (put-text-property start (+ end 1) 'block (+ i 1))))
  (let ((m (copy-marker 5 t))
        (ov (make-overlay 1 39)))
    (overlay-put ov 'all-blocks t)
    (undo-boundary)
    (narrow-to-region 5 20)
    (goto-char 5)
    (re-search-forward "BBB" nil t)
    (replace-match "XXX")
    (undo-boundary)
    (widen)
    (narrow-to-region 21 39)
    (goto-char 21)
    (re-search-forward "HHH" nil t)
    (replace-match "YYY")
    (let ((narrow-s (buffer-string)))
      (widen)
      (primitive-undo 2 buffer-undo-list)
      (list narrow-s
            (buffer-string)
            (string= (buffer-string)
                     "AAA-BBB-CCC-DDD-EEE-FFF-GGG-HHH-III-JJJ")
            (= (marker-position m) 5)
            (get-text-property 1 'block) (= (get-text-property 1 'block) 1)
            (get-text-property 5 'block) (= (get-text-property 5 'block) 2)
            (get-text-property 9 'block) (= (get-text-property 9 'block) 3)
            (get-text-property 13 'block) (= (get-text-property 13 'block) 4)
            (get-text-property 17 'block) (= (get-text-property 17 'block) 5)
            (get-text-property 21 'block) (= (get-text-property 21 'block) 6)
            (get-text-property 25 'block) (= (get-text-property 25 'block) 7)
            (overlay-get ov 'all-blocks))))) "#,
        expect,
    );
}
