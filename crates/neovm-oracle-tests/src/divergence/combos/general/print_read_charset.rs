//! Divergence tests: print/read/circular/charset/char-table combo.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_print_read_roundtrip_symbols() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t t t \"hello\" t \":keyword\" t \"42\" t \"\\\"str\\\"\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '(hello world :keyword test-sym-xxx 42 "string" t nil)))
    (let ((printed (prin1-to-string data)))
      (list (stringp printed)
            (string= printed "(hello world :keyword test-sym-xxx 42 \"string\" t nil)")
            (equal (read printed) data)
            (prin1-to-string 'hello)
            (string= (prin1-to-string 'hello) "hello")
            (prin1-to-string :keyword)
            (string= (prin1-to-string :keyword) ":keyword")
            (prin1-to-string 42)
            (string= (prin1-to-string 42) "42")
            (prin1-to-string "str")
            (string= (prin1-to-string "str") "\"str\""))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_read_nested_structures() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t t t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((data '((a . 1) (b . (2 3)) (c (d e) f) [g h i])))
    (let ((printed (prin1-to-string data)))
      (list (stringp printed)
            (equal (read printed) data)
            (listp data)
            (= (length data) 4)
            (consp (car data))
            (equal (car data) '(a . 1))
            (equal (nth 1 data) '(b 2 3))
            (equal (nth 2 data) '(c (d e) f))
            (equal (nth 3 data) '[g h i]))))) "#,
        expect,
    );
}

#[test]
fn divergence_circular_print() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t nil)""#]];
    crate::common::assert_oracle_parity_expect(
        "(progn\n\
          (let ((x (list 'a 'b)))\n\
            (setcar (cdr x) x)\n\
            (let ((printed (prin1-to-string x t)))\n\
              (list (stringp printed)\n\
                    (> (length printed) 5)\n\
                    (> (length printed) 8))))) ",
        expect,
    );
}

#[test]
fn divergence_charset_functions() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (invalid-read-syntax \")\" 16 56)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (charsetp 'ascii)
        (eq (charsetp 'ascii) t)
        (charsetp 'unicode)
        (eq (charsetp 'unicode) t)
        (not (charsetp 'nonexistent-xxx))
        (charsetp 'eight-bit)
        (eq (charsetp 'eight-bit) t)
        (encode-char ?A 'ascii)
        (= (encode-char ?A 'ascii) 65)
        (decode-char 'ascii 65)
        (= (decode-char 'ascii 65) 65)
        (char-charset ?A)
        (eq (char-charset ?A) 'ascii)
        (char-charset ?\x3B1)
        (memq (char-charset ?\x3B1) '(unicode greek))))) "#,
        expect,
    );
}

#[test]
fn divergence_char_table_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function char-table-type)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ct (make-char-table 'syntax-table nil)))
    (set-char-table-range ct ?\( '(4))
    (set-char-table-range ct ?\) '(5))
    (set-char-table-range ct '(?\[ . ?\]) '(2))
    (list (char-table-p ct)
          (char-table-type ct)
          (eq (char-table-type ct) 'syntax-table)
          (char-table-range ct ?\()
          (equal (char-table-range ct ?\() '(4))
          (char-table-range ct ?\))
          (equal (char-table-range ct ?\)) '(5))
          (char-table-range ct ?\[)
          (equal (char-table-range ct ?\[) '(2))
          (char-table-range ct ?a)
          (null (char-table-range ct ?a))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_with_text_props() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r##""OK (\"#(\\\"Hello World\\\" 0 5 (face bold) 6 11 (face italic))\" nil bold t italic t 11 t \"Hello\" t)""##
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((str (copy-sequence "Hello World")))
    (put-text-property 0 5 'face 'bold str)
    (put-text-property 6 11 'face 'italic str)
    (list (prin1-to-string str)
          (string= (prin1-to-string str) "\"Hello World\"")
          (get-text-property 0 'face str)
          (eq (get-text-property 0 'face str) 'bold)
          (get-text-property 6 'face str)
          (eq (get-text-property 6 'face str) 'italic)
          (length str)
          (= (length str) 11)
          (substring-no-properties str 0 5)
          (string= (substring-no-properties str 0 5) "Hello")))) "#,
        expect,
    );
}

#[test]
fn divergence_read_special_forms() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (wrong-number-of-arguments equal 3)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (equal (read "()") '())
        (equal (read "nil") '())
        (equal (read "'hello") '(quote hello))
        (equal (read "`(,a ,@b)") '\`(,(a) ,@(b)))
        (equal (read "[1 2 3]") '[1 2 3])
        (equal (read "(1 . 2)") '(1 . 2))
        (equal (read "(a b c)") '(a b c))
        (= (read "42") 42)
        (string= (read "\"hello\"") "hello")
        (equal (read "(a (b (c)))") '(a (b (c)))))) "#,
        expect,
    );
}

#[test]
fn divergence_format_with_objects() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"(a b c)\" t \"(a b c)\" t \"42\" t \"3.14\" t \"A\" t \"hello test-fwo-xxx world\" t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((obj '(a b c)))
    (list (format "%s" obj)
          (string= (format "%s" obj) "(a b c)")
          (format "%S" obj)
          (string= (format "%S" obj) "(a b c)")
          (format "%d" 42)
          (string= (format "%d" 42) "42")
          (format "%.2f" 3.14)
          (string= (format "%.2f" 3.14) "3.14")
          (format "%c" ?A)
          (string= (format "%c" ?A) "A")
          (format "hello %s world" 'test-fwo-xxx)
          (string= (format "hello %s world" 'test-fwo-xxx)
                   "hello test-fwo-xxx world")))) "#,
        expect,
    );
}

#[test]
fn divergence_char_table_parent_and_extra() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (t alpha t digit t override override t #^[nil nil category-table #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil digit nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil alpha nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] #^^[1 0 #^^[2 0 #^^[3 0 nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil digit nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil alpha nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil nil] t nil t)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((parent (make-char-table 'category-table nil))
        (child (make-char-table 'category-table nil)))
    (set-char-table-range parent ?a 'alpha)
    (set-char-table-range parent ?0 'digit)
    (set-char-table-parent child parent)
    (list (char-table-p child)
          (char-table-range child ?a)
          (eq (char-table-range child ?a) 'alpha)
          (char-table-range child ?0)
          (eq (char-table-range child ?0) 'digit)
          (set-char-table-range child ?a 'override)
          (char-table-range child ?a)
          (eq (char-table-range child ?a) 'override)
          (char-table-parent child)
          (eq (char-table-parent child) parent)
          (char-table-extra-slot child 0)
          (null (char-table-extra-slot child 0))))) "#,
        expect,
    );
}

#[test]
fn divergence_print_read_hash_table() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t val1 t val2 t 2 t equal t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((ht (make-hash-table :test 'equal)))
    (puthash 'key1 'val1 ht)
    (puthash 'key2 'val2 ht)
    (let ((printed (prin1-to-string ht)))
      (list (stringp printed)
            (> (length printed) 5)
            (hash-table-p ht)
            (gethash 'key1 ht)
            (eq (gethash 'key1 ht) 'val1)
            (gethash 'key2 ht)
            (eq (gethash 'key2 ht) 'val2)
            (hash-table-count ht)
            (= (hash-table-count ht) 2)
            (hash-table-test ht)
            (eq (hash-table-test ht) 'equal))))) "#,
        expect,
    );
}
