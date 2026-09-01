//! Strict combo oracle probes, batch 292: plist deep. plist-get/-put/-member,
//! lax-plist-get/-put (dotted pairs), plist-to-alist / alist-to-plist, and
//! setf on plist-get.
//! Uses assert_oracle_parity_expect format.

use crate::common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn div_v8_plist_get_member_put_lax() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((p (copy-sequence '(a 1 b 2 c 3))))
  (list (plist-get p 'b)
        (plist-get p 'z)
        (plist-member p 'b)
        (plist-member p 'z)
        (progn (plist-put p 'd 4) p)
        (progn (plist-put p 'a 99) (plist-get p 'a))))
"##;
    let expect = expect_test::expect![[r#""OK (2 nil (b 2 c 3 d 4) nil (a 99 b 2 c 3 d 4) 99)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_lax_plist_get_put_dotted() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let ((lp (copy-sequence '((a . 1) (b . 2) (c . 3)))))
  (list (lax-plist-get lp 'b)
        (lax-plist-get lp 'z)
        (lax-plist-member lp 'c)
        (progn (lax-plist-put lp 'd 4) lp)
        (progn (lax-plist-put lp 'a 99) (lax-plist-get lp 'a))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function lax-plist-member)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}

#[test]
fn div_v8_plist_to_alist_alist_to_plist_roundtrip() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    let form = r##"
(let* ((p '(a 1 b 2 c 3))
       (al (plist-to-alist p))
       (p2 (alist-to-plist al)))
  (list al
        p2
        (lax-plist-get (alist-to-plist al) 'b)
        (plist-get p2 'c)
        (sort (mapcar #'car al)
              (lambda (a b) (string< (symbol-name a) (symbol-name b))))))
"##;
    let expect = expect_test::expect![[r#""ERR (void-function plist-to-alist)""#]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
