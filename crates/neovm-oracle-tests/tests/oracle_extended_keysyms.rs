//! Lisp binding controls for #398; native GUI transport is checked separately.

use crate::common;
use common::return_if_neovm_enable_oracle_proptest_not_set;

#[test]
fn reported_extended_keysyms_preserve_gnu_lisp_bindings() {
    return_if_neovm_enable_oracle_proptest_not_set!();
    common::assert_oracle_parity_expect(
        r#"(let ((names '(f13 f14 f15 f16 f17 f18 f19 f20 f21 f22 f23 f24
                            f25 f26 f27 f28 f29 f30 f31 f32 f33 f34 f35
                            undo redo menu XF86Back XF86Forward XF86Copy XF86Cut XF86Paste))
                  (map (make-sparse-keymap))
                  results)
              (dolist (name names)
                (let* ((description (format "<%s>" name))
                       (key (kbd description)))
                  (define-key map key 'ignore)
                  (push (list (equal key (vector name))
                              (equal (key-description key) description)
                              (eq (lookup-key map key) 'ignore)
                              (eq (event-basic-type (aref key 0)) name))
                        results)))
              (list (length results)
                    (equal results (make-list 31 '(t t t t)))
                    (equal (kbd "C-S-<f13>") [C-S-f13])))"#,
        expect_test::expect![[r#""OK (31 t t)""#]],
    );
}
