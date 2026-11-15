//! VM replace-region test re-homed from neovm-eval's VM test module.
//!
//! The fixture drives `replace-region-contents`, a core-owned buffer command
//! (R0 placement: it drives buffer source/property state through the command
//! layer). Eval's unit runtime carries the upper entry as an ABSENT-LAYER
//! refusal, so the test belongs to the runtime that owns the primitive.
//! Core's `new_context` constructs that same surface with the canonical
//! hook-installation proof. The Lisp form and expected string are unchanged.

fn vm_eval_str(src: &str) -> String {
    let mut eval = crate::test_utils::new_context();
    let result = eval.eval_str(src);
    crate::emacs_core::format_eval_result(&result)
}

#[test]
fn vm_replace_region_contents_uses_shared_source_and_property_state() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        vm_eval_str(
            r#"(let ((dest (current-buffer))
                     (src (get-buffer-create "*rrc-src*"))
                     (s (propertize "CD" 'face 'bold)))
                 (set-buffer src)
                 (erase-buffer)
                 (insert "1234")
                 (put-text-property 2 4 'face 'italic)
                 (set-buffer dest)
                 (list
                  (progn
                    (erase-buffer)
                    (insert "abZZef")
                    (replace-region-contents 3 5 s)
                    (list
                     (buffer-substring-no-properties 1 (point-max))
                     (get-text-property 3 'face)))
                  (progn
                    (erase-buffer)
                    (insert "abZZef")
                    (replace-region-contents 3 5 (vector src 2 4))
                    (list
                     (buffer-substring-no-properties 1 (point-max))
                     (get-text-property 3 'face)
                     (get-text-property 4 'face)))
                  (condition-case err
                      (replace-region-contents 3 5 (current-buffer))
                    (error (list (car err) (car (cdr err)))))))"#
        ),
        r#"OK (("abCDef" bold) ("ab23ef" italic italic) (error "Cannot replace a buffer with itself"))"#
    );
}
