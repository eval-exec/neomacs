//! Divergence tests: plist + symbol property + face + font-lock combos.

use crate::common::assert_oracle_parity;
use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn divergence_plist_put_get_remove_cycle() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (1 2 3 nil t t t t 99 t 4 t 8 t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pl nil))
    (setq pl (plist-put pl :a 1))
    (setq pl (plist-put pl :b 2))
    (setq pl (plist-put pl :c 3))
    (let ((p1 (plist-get pl :a))
          (p2 (plist-get pl :b))
          (p3 (plist-get pl :c))
          (p4 (plist-get pl :d)))
      (setq pl (plist-put pl :a 99))
      (setq pl (plist-put pl :d 4))
      (list p1 p2 p3 p4
            (= p1 1) (= p2 2) (= p3 3) (null p4)
            (plist-get pl :a)
            (= (plist-get pl :a) 99)
            (plist-get pl :d)
            (= (plist-get pl :d) 4)
            (length pl)
            (= (length pl) 8))))) "#,
        expect,
    );
}

#[test]
fn divergence_symbol_plist_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 15 41)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((sym (make-symbol "test-sym-pl-xxx")))
    (put sym 'prop1 'val1)
    (put sym 'prop2 42)
    (put sym 'prop3 '(a b c))
    (list (get sym 'prop1)
          (eq (get sym 'prop1) 'val1)
          (get sym 'prop2)
          (= (get sym 'prop2) 42)
          (get sym 'prop3)
          (equal (get sym 'prop3) '(a b c))
          (get sym 'nonexistent)
          (null (get sym 'nonexistent))
          (symbol-plist sym)
          (listp (symbol-plist sym))))) #"#,
        expect,
    );
}

#[test]
fn divergence_face_attribute_operations() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[
        r#""OK (\"red\" t bold t [face unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified unspecified] t unspecified nil)""#
    ]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defface test-face-attr-xxx '((t :foreground "red" :weight bold))
    "Test face.")
  (list (face-attribute 'test-face-attr-xxx :foreground)
        (equal (face-attribute 'test-face-attr-xxx :foreground) "red")
        (face-attribute 'test-face-attr-xxx :weight)
        (equal (face-attribute 'test-face-attr-xxx :weight) 'bold)
        (facep 'test-face-attr-xxx)
        (not (facep 'nonexistent-face-xxx))
        (face-attribute 'test-face-attr-xxx :underline)
        (equal (face-attribute 'test-face-attr-xxx :underline) nil))) "#,
        expect,
    );
}

#[test]
fn divergence_plist_member_vs_get() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 12 48)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pl '(:a 1 :b nil :c 3)))
    (list (plist-get pl :a)
          (= (plist-get pl :a) 1)
          (plist-get pl :b)
          (null (plist-get pl :b))
          (plist-get pl :c)
          (= (plist-get pl :c) 3)
          (plist-member pl :b)
          (plist-member pl :d)
          (null (plist-member pl :d))
          (not (null (plist-member pl :b)))))) #"#,
        expect,
    );
}

#[test]
fn divergence_symbol_function_plist_interplay() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 14 68)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defun test-sfpi-xxx (x) (+ x 1))
  (put 'test-sfpi-xxx 'doc-string "test function")
  (put 'test-sfpi-xxx 'safe '(lambda (x) t))
  (list (fboundp 'test-sfpi-xxx)
        (functionp 'test-sfpi-xxx)
        (get 'test-sfpi-xxx 'doc-string)
        (string= (get 'test-sfpi-xxx 'doc-string) "test function")
        (get 'test-sfpi-xxx 'safe)
        (funcall (get 'test-sfpi-xxx 'safe) 42)
        (funcall 'test-sfpi-xxx 5)
        (= (funcall 'test-sfpi-xxx 5) 6)
        (documentation 'test-sfpi-xxx)
        (string= (documentation 'test-sfpi-xxx) "test function"))) #"#,
        expect,
    );
}

#[test]
fn divergence_face_remap_alist() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 11 56)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (list (facep 'default)
        (facep 'bold)
        (facep 'italic)
        (listp (face-attribute 'default :foreground))
        (or (stringp (face-attribute 'default :foreground))
            (eq (face-attribute 'default :foreground) 'unspecified))
        (face-attribute 'bold :weight)
        (eq (face-attribute 'bold :weight) 'bold)
        (face-attribute 'italic :slant)
        (eq (face-attribute 'italic :slant) 'italic))) #"#,
        expect,
    );
}

#[test]
fn divergence_set_plist_with_hash() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 21 52)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((sym (intern "test-sph-xxx"))
        (ht (make-hash-table :test 'equal)))
    (puthash 'a 1 ht)
    (puthash 'b 2 ht)
    (put sym 'hash ht)
    (put sym 'list '(x y z))
    (put sym 'num 42)
    (let ((stored-ht (get sym 'hash)))
      (list (hash-table-p stored-ht)
            (gethash 'a stored-ht)
            (= (gethash 'a stored-ht) 1)
            (get sym 'list)
            (equal (get sym 'list) '(x y z))
            (get sym 'num)
            (= (get sym 'num) 42)
            (puthash 'c 3 stored-ht)
            (hash-table-count stored-ht)
            (= (hash-table-count stored-ht) 3)
            (gethash 'c (get sym 'hash))
            (= (gethash 'c (get sym 'hash)) 3))))) #"#,
        expect,
    );
}

#[test]
fn divergence_plist_to_alist_conversion() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""ERR (void-function cl-loop)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((pl '(:name "Alice" :age 30 :roles (admin editor))))
    (let ((al (cl-loop for (k v) on pl by 'cddr collect (cons k v))))
      (list al
            (= (length al) 3)
            (assoc :name al)
            (equal (assoc :name al) '(:name . "Alice"))
            (assoc :age al)
            (= (cdr (assoc :age al)) 30)
            (assoc :roles al)
            (equal (cdr (assoc :roles al)) '(admin editor)))))) #"#,
        expect,
    );
}

#[test]
fn divergence_face_all_attributes() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r##""ERR (invalid-read-syntax \"#\" 11 35)""##]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (defface test-comp-xxx '((t :foreground "blue" :background "yellow" :underline t))
    "Composite face.")
  (let ((attrs (list (face-attribute 'test-comp-xxx :foreground)
                     (face-attribute 'test-comp-xxx :background)
                     (face-attribute 'test-comp-xxx :underline))))
    (list (equal (nth 0 attrs) "blue")
          (equal (nth 1 attrs) "yellow")
          (equal (nth 2 attrs) t)
          (facep 'test-comp-xxx)
          (= (length attrs) 3)))) #"#,
        expect,
    );
}

#[test]
fn divergence_symbol_name_intern_plist_chain() {
    return_if_neovm_enable_oracle_proptest_not_set!();

    let expect = expect_test::expect![[r#""OK (t t t t)""#]];
    crate::common::assert_oracle_parity_expect(
        r#"(progn
  (let ((names '("test-chain-a-xxx" "test-chain-b-xxx" "test-chain-c-xxx"))
        (result nil))
    (dolist (name names)
      (let ((sym (intern name)))
        (put sym 'index (length result))
        (put sym 'name name)
        (push (list (symbol-name sym)
                    (get sym 'index)
                    (= (get sym 'index) (length result))
                    (get sym 'name)
                    (string= (get sym 'name) name))
              result)))
    (let ((final (nreverse result)))
      (list (= (length final) 3)
            (equal (car (car final)) "test-chain-a-xxx")
            (= (nth 1 (cadr final)) 1)
            (= (nth 1 (caddr final)) 2))))) "#,
        expect,
    );
}
