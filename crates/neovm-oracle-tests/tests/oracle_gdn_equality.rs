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
