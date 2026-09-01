/// Batch 468: cl-loop complex, seq-map deep, subr-x edge, string-propertize display.
use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_cx468_cl_loop_hash_across() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equal)))
  (puthash "a" 1 ht) (puthash "b" 2 ht) (puthash "c" 3 ht)
  (cl-loop for k being the hash-keys of ht
           collect (cons k (gethash k ht))))"##,
        expect,
    );
}

#[test]
fn div_cx468_cl_loop_summing() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i from 1 to 10
           sum (* i i))"##,
        expect,
    );
}

#[test]
fn div_cx468_cl_loop_maximising() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(cl-loop for i in '(3 7 2 9 1 8)
           maximize i)"##,
        expect,
    );
}

#[test]
fn div_cx468_seq_map_into_deep() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((2 3 4) (2 3 4) (65 66 67))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (list (seq-map #'1+ '(1 2 3))
        (seq-map #'1+ [1 2 3])
        (seq-map #'upcase "abc")))"##,
        expect,
    );
}

#[test]
fn div_cx468_seq_drop_take() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((a b) (c d) (1 2) (a 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (list (seq-take '(a b c d) 2)
        (seq-drop '(a b c d) 2)
        (seq-take-while #'numberp '(1 2 a 3))
        (seq-drop-while #'numberp '(1 2 a 3))))"##,
        expect,
    );
}

#[test]
fn div_cx468_seq_chunk_split() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK nil""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'seq)
  (list (seq-split '(1 2 3 4 5) 2)
        (seq-do (lambda (i) (* 2 i)) '(1 2 3)))"##,
        expect,
    );
}

#[test]
fn div_cx468_thread_let() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function thread-let)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(progn (require 'subr-x)
  (thread-let ((x 1) (y 2)) (+ x y)))"##,
        expect,
    );
}

#[test]
fn div_cx468_propertize_display_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[
        r#""OK (#(\"hello\" 0 5 (display \"WORLD\" face bold)) #(\"  \" 0 2 (display (space :width 10))) #(\"abc\" 0 3 (face italic mouse-face highlight)))""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (propertize "hello" 'display "WORLD" 'face 'bold)
      (propertize "  " 'display '(space :width 10))
      (propertize "abc" 'face 'italic 'mouse-face 'highlight))"##,
        expect,
    );
}

#[test]
fn div_cx468_kbd_key_description() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect =
        expect_test::expect![[r#""OK (\"\u{3}\u{6}\" [134217848] [134217729] \"C-x C-c\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (kbd "C-c C-f") (kbd "M-x") (kbd "C-M-a")
      (key-description (kbd "C-x C-c")))"##,
        expect,
    );
}

#[test]
fn div_cx468_hash_table_custom_test() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (5 equal)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((ht (make-hash-table :test 'equal :size 10 :rehash-size 1.5)))
  (dotimes (i 5) (puthash i (* i i) ht))
  (list (hash-table-count ht) (hash-table-test ht)))"##,
        expect,
    );
}

#[test]
fn div_cx468_cl_assoc_if() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""ERR (void-function cl-assoc-if)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (cl-assoc-if #'numberp '((a . 1) (b . 2) (c . 3)))
      (cl-rassoc-if #'numberp '((1 . a) (2 . b) (3 . c)))
      (cl-member 3 '(1 2 3 4) :key #'1-))"##,
        expect,
    );
}

#[test]
fn div_cx468_list_keyword_args() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (\"john\" nil (:b 2 :c 3))""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (plist-get '(:name "john" :age 30 :city "nyc") :name)
      (plist-get '(:a 1 :b 2) :c)
      (plist-member '(:a 1 :b 2 :c 3) :b))"##,
        expect,
    );
}

#[test]
fn div_cx468_make_bool_vector() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (#&10\"\\377\u{3}\" #&5\"\\0\" #&5\"\u{15}\")""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(list (make-bool-vector 10 t)
      (make-bool-vector 5 nil)
      (bool-vector t nil t nil t))"##,
        expect,
    );
}

#[test]
fn div_cx468_char_syntax_string() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK (119 40 41 32 46)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(with-temp-buffer
  (insert "hello(world)test")
  (list (char-syntax ?h) (char-syntax ?\() (char-syntax ?\))
        (char-syntax ?\s) (char-syntax ?\;)))"##,
        expect,
    );
}

#[test]
fn div_cx468_set_plist_then_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let expect = expect_test::expect![[r#""OK ((prop1 val1 prop2 val2) val1 val2)""#]];
    crate::common::assert_oracle_parity_expect(
        r##"(let ((s (make-symbol "neo-cx468-ps")))
  (setplist s '(prop1 val1 prop2 val2))
  (list (symbol-plist s) (get s 'prop1) (get s 'prop2)))"##,
        expect,
    );
}
