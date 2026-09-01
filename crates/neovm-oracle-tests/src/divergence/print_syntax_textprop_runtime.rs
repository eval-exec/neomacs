//! Printer/reader (floats, print-level/length, print-circle/shared,
//! special chars, roundtrip), syntax parsing (syntax-ppss, forward-sexp,
//! parse-partial-sexp, scan-lists), and text-property/marker/overlay ops.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn number_format_edge() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1.0\" \"4611686018427387904\" \"-2305843009213693952\" 0.0 3 3.5 2 -1)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (number-to-string 1.0) (number-to-string (expt 2 62))
        (format "%S" most-negative-fixnum) (abs -0.0) (/ 7 2) (/ 7.0 2) (mod -7 3) (% -7 3))"##,
        expect,
    );
}

#[test]
fn print_circle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK \"#1=(1 2 3 . #1#)\"""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-circle t) (l (list 1 2 3)))
  (setcdr (cddr l) l)
  (prin1-to-string l))"##,
        expect,
    );
}

#[test]
fn print_floats() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"1.0\" \"0.1\" \"1e+20\" \"-0.0\" \"100.0\" \"1.5e-10\" \"0.3333333333333333\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string 1.0) (prin1-to-string 0.1) (prin1-to-string 1e20)
        (prin1-to-string -0.0) (prin1-to-string 100.0) (prin1-to-string 1.5e-10)
        (prin1-to-string (/ 1.0 3.0)))"##,
        expect,
    );
}

#[test]
fn print_hash_record() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (eq 1 \"#s(foo 1 2)\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((h (make-hash-table :test 'eq :size 4)))
  (puthash 'a 1 h)
  (list (hash-table-test h) (hash-table-count h)
        (prin1-to-string (record 'foo 1 2))))"##,
        expect,
    );
}

#[test]
fn print_level_length() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect =
        expect_test::expect![[r#""OK (\"(1 (2 ...))\" \"(a b c ...)\" \"[1 2 3 ...]\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((print-level 2) (print-length 3))
  (list (prin1-to-string '(1 (2 (3 (4)))))
        (prin1-to-string '(a b c d e f))
        (prin1-to-string [1 2 3 4 5])))"##,
        expect,
    );
}

#[test]
fn print_quoted_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""OK (\"'x\" \"`(a ,b ,@c)\" \"#'fn\")""##]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string ''x) (prin1-to-string '`(a ,b ,@c))
        (prin1-to-string '#'fn))"##,
        expect,
    );
}

#[test]
fn print_read_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((objs '((1 2 3) "str" [1 2] (a . b) 3.14 ?x sym)))
  (equal objs (car (read-from-string (prin1-to-string objs)))))"##,
        expect,
    );
}

#[test]
fn print_shared() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK \"(#1=(x) #1#)\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let* ((print-circle t) (shared (list 'x)) (l (list shared shared)))
  (prin1-to-string l))"##,
        expect,
    );
}

#[test]
fn print_special_chars() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"\\\"a\tb\\nc\\\\\\\"d\\\\\\\\e\\\"\" \"10\" \"9\" \"1\" \"foo\\\\ bar\")""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (prin1-to-string "a\tb\nc\"d\\e")
        (prin1-to-string ?\n) (prin1-to-string ?\t) (prin1-to-string ?\C-a)
        (prin1-to-string 'foo\ bar))"##,
        expect,
    );
}

#[test]
fn forward_sexp_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (12 17 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a (b c) d) next")
  (goto-char (point-min)) (forward-sexp)
  (let ((p1 (point))) (forward-sexp)
    (list p1 (point) (progn (goto-char 1) (forward-char 1) (forward-sexp) (point)))))"##,
        expect,
    );
}

#[test]
fn parse_partial_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (2 6 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(1 2 (3 4) 5)")
  (let ((s (parse-partial-sexp (point-min) 8)))
    (list (nth 0 s) (nth 1 s) (numberp (nth 2 s)))))"##,
        expect,
    );
}

#[test]
fn scan_lists_sexps() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 4 8)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a) (b) (c)")
  (list (scan-lists 1 1 0) (scan-sexps 1 1) (scan-lists 1 2 0)))"##,
        expect,
    );
}

#[test]
fn syntax_class_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-type-argument fixnump nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (list (char-syntax ?\() (char-syntax ?-) (char-syntax ?\;)
        (string (char-syntax ?a)) (syntax-class-to-char (car (syntax-after (point-min))))))"##,
        expect,
    );
}

#[test]
fn syntax_ppss() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 34 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(foo (bar \"str\") baz)")
  (list (nth 0 (syntax-ppss 6)) (nth 0 (syntax-ppss 11))
        (nth 3 (syntax-ppss 13)) (nth 0 (syntax-ppss 21))))"##,
        expect,
    );
}

#[test]
fn thing_at_point_sexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (\"world\" \"world\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello (world foo) bar")
  (goto-char 9) (list (thing-at-point 'word t) (thing-at-point 'symbol t)))"##,
        expect,
    );
}

#[test]
fn up_down_list() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 10)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a (b (c) d) e)")
  (goto-char 8) (backward-up-list)
  (let ((p1 (point))) (goto-char 8) (up-list) (list p1 (point))))"##,
        expect,
    );
}

#[test]
fn insert_with_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold nil #(\"ABCD\" 0 2 (face bold)) \"ABCD\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert (propertize "AB" 'face 'bold) "CD")
  (list (get-text-property 1 'face) (get-text-property 3 'face)
        (buffer-substring 1 5) (substring-no-properties (buffer-string))))"##,
        expect,
    );
}

#[test]
fn marker_arith() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (7 5 t 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "0123456789")
  (let ((m (copy-marker 5)))
    (list (+ m 2) (marker-position m) (= m 5) (- (point-max) m))))"##,
        expect,
    );
}

#[test]
fn marker_insertion_type() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 5 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello")
  (let ((m1 (copy-marker 3 nil)) (m2 (copy-marker 3 t)))
    (goto-char 3) (insert "XX")
    (list (marker-position m1) (marker-position m2) (marker-insertion-type m2))))"##,
        expect,
    );
}

#[test]
fn overlay_move_delete() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (6 nil nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdefghij")
  (let ((o (make-overlay 2 5)))
    (move-overlay o 6 9)
    (let ((s (overlay-start o)))
      (delete-overlay o)
      (list s (overlay-start o) (overlay-buffer o)))))"##,
        expect,
    );
}

#[test]
fn overlay_ops() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 6 bold 2 (#<overlay in no buffer>))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world test")
  (let ((o (make-overlay 1 6)) (o2 (make-overlay 7 12)))
    (overlay-put o 'face 'bold)
    (list (overlay-start o) (overlay-end o) (overlay-get o 'face)
          (length (overlays-in 1 16)) (overlays-at 3))))"##,
        expect,
    );
}

#[test]
fn textprop_add_remove() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 nil 2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcdefgh")
  (add-text-properties 2 6 '(p1 1 p2 2))
  (remove-text-properties 3 5 '(p1 nil))
  (list (get-text-property 2 'p1) (get-text-property 4 'p1) (get-text-property 4 'p2)))"##,
        expect,
    );
}

#[test]
fn textprop_put_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (bold 42 3 (x 42 face bold))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (put-text-property 1 6 'face 'bold)
  (put-text-property 3 9 'x 42)
  (list (get-text-property 1 'face) (get-text-property 3 'x)
        (next-property-change 1) (text-properties-at 4)))"##,
        expect,
    );
}

#[test]
fn textprop_search() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (4 nil 4)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "aaabbbccc")
  (put-text-property 4 7 'tag 'mid)
  (list (text-property-any 1 10 'tag 'mid)
        (text-property-not-all 4 7 'tag 'mid)
        (next-single-property-change 1 'tag)))"##,
        expect,
    );
}
