//! Oracle parity tests for GNU `subr.el' `suppress-keymap'.

use crate::common::assert_oracle_parity;

#[test]
fn oracle_suppress_keymap_mutates_digits_and_remap_contract() {
    let form = r#"
(let ((default-map (make-sparse-keymap))
      (nodigits-map (make-sparse-keymap)))
  (list
   ;; GNU subr.el returns nil when the digit exception branch runs.
   (suppress-keymap default-map)
   (lookup-key default-map [remap self-insert-command])
   (lookup-key default-map "-")
   (lookup-key default-map "0")
   (lookup-key default-map "9")
   (lookup-key default-map "a")
   ;; With NODIGITS non-nil, GNU skips the digit exception and returns NODIGITS.
   (suppress-keymap nodigits-map t)
   (lookup-key nodigits-map [remap self-insert-command])
   (lookup-key nodigits-map "-")
   (lookup-key nodigits-map "0")
   (lookup-key nodigits-map "9")
   (lookup-key nodigits-map "a")
   (condition-case e
       (suppress-keymap 42)
     (error (list (car e) (cadr e) (caddr e))))))"#;
    let expect = expect_test::expect![[
        r#""OK (nil undefined negative-argument digit-argument digit-argument nil t undefined nil nil nil nil (wrong-type-argument keymapp 42))""#
    ]];
    crate::common::assert_oracle_parity_expect(form, expect);
}
