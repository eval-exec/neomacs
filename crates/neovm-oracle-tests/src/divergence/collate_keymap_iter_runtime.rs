//! string-collate-lessp/equalp with locale, map-keymap / where-is-internal /
//! keymap-set/keymap-lookup / prefix keymap, text-property-search-forward with
//! predicate + prop-match accessors, sort-coding-systems, char-fold of ü/ñ/œ,
//! map-into hash/alist, cl-loop hash-keys using hash-values.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn char_fold_more() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'char-fold)
(let ((case-fold-search t))
  (list (with-temp-buffer (insert "über") (goto-char 1) (and (re-search-forward (char-fold-to-regexp "uber") nil t) t))
        (with-temp-buffer (insert "señor") (goto-char 1) (and (re-search-forward (char-fold-to-regexp "senor") nil t) t))
        (with-temp-buffer (insert "œuvre") (goto-char 1) (and (re-search-forward (char-fold-to-regexp "oeuvre") nil t) t))))"##,
        expect,
    );
}

#[test]
fn cl_loop_collect_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK ((\"x\" . 1) (\"y\" . 2))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((h (make-hash-table :test 'equal)))
  (puthash "x" 1 h) (puthash "y" 2 h)
  (sort (cl-loop for k being the hash-keys of h using (hash-values v) collect (cons k v))
        (lambda (a b) (string< (car a) (car b)))))"##,
        expect,
    );
}

#[test]
fn cl_loop_keymap() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 2""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'cl-lib)
(let ((m (make-sparse-keymap)) (n 0))
  (define-key m "a" 'cmd-a) (define-key m "b" 'cmd-b)
  (map-keymap (lambda (_k _b) (setq n (1+ n))) m)
  n)"##,
        expect,
    );
}

#[test]
fn keymap_set_lookup() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (cmd-a nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (let ((m (make-sparse-keymap)))
  (keymap-set m "C-c a" 'cmd-a)
  (list (keymap-lookup m "C-c a") (keymap-lookup m "C-c b"))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn keymap_which_key() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t cmd-x)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((m (make-sparse-keymap)))
  (define-key m (kbd "C-c x") 'cmd-x)
  (list (keymapp (lookup-key m (kbd "C-c"))) (lookup-key m (kbd "C-c x"))))"##,
        expect,
    );
}

#[test]
fn map_into_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (#s(hash-table test equal data (a 1 b 2)) ((:a . 1) (:b . 2)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'map)
(list (map-into '((a . 1) (b . 2)) 'hash-table)
      (map-into '(:a 1 :b 2) 'alist))"##,
        expect,
    );
}

#[test]
fn sort_coding_systems() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK t""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (listp (sort-coding-systems (list 'utf-8 'latin-1))) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn string_collate_locale() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(condition-case e (list (and (string-collate-lessp "apple" "banana" "en_US.UTF-8") t)
        (and (string-collate-equalp "abc" "abc" "en_US.UTF-8") t)
        (and (string-collate-lessp "a" "B" "C") t)) (error (cons (quote ERR) (car e))))"##,
        expect,
    );
}

#[test]
fn text_prop_search_pred() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (3 5 1)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(require 'text-property-search)
(with-temp-buffer
  (insert "aaXXbbYYcc")
  (put-text-property 3 5 'p 1) (put-text-property 7 9 'p 2)
  (goto-char (point-min))
  (let ((m (text-property-search-forward 'p 1 t)))
    (list (prop-match-beginning m) (prop-match-end m) (prop-match-value m))))"##,
        expect,
    );
}

#[test]
fn where_is_multiple() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK 3""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((m (make-sparse-keymap)))
  (define-key m "x" 'foo) (define-key m "y" 'foo) (define-key m "z" 'foo)
  (length (where-is-internal 'foo m)))"##,
        expect,
    );
}
