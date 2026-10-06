//! GNU-backed regressions for lane GDN string representation and equality.
#[path = "../src/common.rs"]
mod common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn gdn_delete_string_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let* ((raw (concat "é" (string-to-multibyte "\352")))
                         (a (delete ?é "abé"))
                         (b (delete ?b (string-to-multibyte "abc")))
                         (c (delete ?é raw))
                         (d (delete ?é "é"))
                         (u (delete ?b (unibyte-string 97 98 234)))
                         (p (propertize "abé" 'face 'bold)))
                    (list a (multibyte-string-p a) b (multibyte-string-p b)
                          (aref c 0) (multibyte-string-p c) (string-bytes c)
                          (multibyte-string-p d) (multibyte-string-p u)
                          (multibyte-string-p (remove ?é raw))
                          (multibyte-string-p (delete ?é p))
                          (text-properties-at 0 (delete ?é p))))"#;
    common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (\"ab\" t \"ac\" t 4194282 t 2 t nil t t nil)""#]],
    );
}

#[test]
fn gdn_reverse_string_representation() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(list (multibyte-string-p (reverse (string-to-multibyte "ab")))
                       (multibyte-string-p (nreverse (string-to-multibyte "ab")))
                       (multibyte-string-p (reverse (string-to-multibyte "")))
                       (multibyte-string-p (reverse (substring "éa" 1)))
                       (multibyte-string-p (reverse "éa"))
                       (string-to-list (reverse (concat "é" (string-to-multibyte "\352"))))
                       (string-to-list (reverse (unibyte-string 97 234)))
                       (text-properties-at 0 (reverse (propertize "ab" 'face 'bold))))"#;
    common::assert_oracle_parity_expect(
        form,
        expect_test::expect![[r#""OK (nil nil nil nil t (4194282 233) (234 97) nil)""#]],
    );
}

#[test]
fn gdn_equal_car_cycles() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r#"(let ((a (list 1)) (b (list 1)) (c (list 1 2)) (d (list 1 3)))
                    (setcar a a) (setcar b b) (setcar c c) (setcar d d)
                    (list (equal a b) (equal c d) (equal-including-properties a b)
                          (not (null (member a (list b))))
                          (let ((x (vector nil)) (y (vector nil)))
                            (aset x 0 (cons x a)) (aset y 0 (cons y b)) (equal x y))
                          (let ((x (list 1)) (y (list 1 1)))
                            (setcar x x) (setcar y y) (equal x y))))"#;
    common::assert_oracle_parity_expect(form, expect_test::expect![[r#""OK (t nil t t t nil)""#]]);
}
