/// Batch 455: thing-at-point, word-search, char-*, Unicode, narrow-defun.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx455_thing_at_point_various() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"foo\" \"foo\" \"foo\" \"(defun foo (x) (* x 2))\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun foo (x) (* x 2))")
  (goto-char 8)
  (list (thing-at-point 'sexp) (thing-at-point 'symbol) (thing-at-point 'word) (thing-at-point 'list)))"##,
        expect,
    );
}

#[test]
fn div_cx455_bounds_of_thing_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (1 . 6)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world")
  (goto-char 3)
  (bounds-of-thing-at-point 'word))"##,
        expect,
    );
}

#[test]
fn div_cx455_word_search_forward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 18""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world hello")
  (goto-char 1)
  (word-search-forward "hello" nil t 2)
  (point))"##,
        expect,
    );
}

#[test]
fn div_cx455_word_search_backward() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello world hello")
  (goto-char (point-max))
  (word-search-backward "hello" nil t 2)
  (point))"##,
        expect,
    );
}

#[test]
fn div_cx455_word_search_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"\\\\<hello\\\\W+world\\\\>\"""#]];
    crate::common::assert_oracle_parity_expect(r##"(word-search-regexp "hello world")"##, expect);
}

#[test]
fn div_cx455_count_words_buffer() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 5""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "one two three four five")
  (count-words (point-min) (point-max)))"##,
        expect,
    );
}

#[test]
fn div_cx455_sexp_at_point() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK a""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "(a b c)")
  (goto-char 2)
  (sexp-at-point))"##,
        expect,
    );
}

#[test]
fn div_cx455_narrow_to_defun_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (17 33 \"(defun b (y) y)\\n\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun a (x) x)\n(defun b (y) y)\n")
  (goto-char 20)
  (narrow-to-defun)
  (list (point-min) (point-max) (buffer-string)))"##,
        expect,
    );
}

#[test]
fn div_cx455_beginning_of_defun_regexp() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK 1""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (emacs-lisp-mode)
  (insert "(defun a (x) x)\n")
  (goto-char (point-max))
  (beginning-of-defun 1)
  (point))"##,
        expect,
    );
}

#[test]
fn div_cx455_unicode_category() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (Lu Nd Po)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'general-category)
      (get-char-code-property ?0 'general-category)
      (get-char-code-property ?! 'general-category))"##,
        expect,
    );
}

#[test]
fn div_cx455_unicode_uppercase_lowercase() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (nil nil 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (get-char-code-property ?A 'uppercase)
      (get-char-code-property ?a 'lowercase)
      (get-char-code-property ?1 'numeric-value))"##,
        expect,
    );
}

#[test]
fn div_cx455_char_after_char_before_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (99 99 97 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "a cafe world")
  (goto-char 4)
  (list (char-after 3) (char-before) (following-char) (preceding-char)))"##,
        expect,
    );
}

#[test]
fn div_cx455_insert_char_multibyte() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK \"aaa 😀\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert-char ?a 3)
  (insert-char ? 1)
  (insert-char #x1F600 1)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx455_write_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""helloOK \"\"""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (write-char ?h)
  (write-char ?e)
  (write-char ?l)
  (write-char ?l)
  (write-char ?o)
  (buffer-string))"##,
        expect,
    );
}

#[test]
fn div_cx455_preceding_char_following_char() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (98 99)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "abcde")
  (goto-char 3)
  (list (preceding-char) (following-char)))"##,
        expect,
    );
}
