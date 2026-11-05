use super::*;
use crate::emacs_core::print::print_value;

#[test]
fn sxhash_positioned_context_modes_cover_code_constants_and_properties() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::new_context();
    let result = context
        .eval_str(
            r#"
(let* ((s (read "sxhash-positioned-context"))
       (w (read-positioning-symbols "sxhash-positioned-context"))
       (plain-code (make-byte-code 0 "\300\207" (vector s) 1))
       (wrapped-code (make-byte-code 0 "\300\207" (vector w) 1))
       (plain-string (propertize "x" 'category s))
       (wrapped-string (propertize "x" 'category w)))
  (list
   (let ((symbols-with-pos-enabled t))
     (list (= (sxhash-equal plain-code) (sxhash-equal wrapped-code))
           (= (sxhash-equal-including-properties plain-string)
              (sxhash-equal-including-properties wrapped-string))))
   (let ((symbols-with-pos-enabled nil))
     (= (sxhash-equal s) (sxhash-equal w)))
   (let ((symbols-with-pos-enabled t))
     (= (sxhash-equal s) (sxhash-equal w)))))
"#,
        )
        .unwrap();
    assert_eq!(print_value(&result), "((t t) nil t)");
}

#[test]
fn sxhash_positioned_user_hash_noninteger_fallback_uses_owner_mode() {
    crate::test_utils::init_test_tracing();
    let mut context = crate::test_utils::new_context();
    let result = context
        .eval_str(
            r#"
(let ((symbols-with-pos-enabled t))
  (define-hash-table-test 'sxhash-positioned-user
    #'equal
    (lambda (_key) (read-positioning-symbols "sxhash-positioned-user-hash")))
  (let ((table (make-hash-table :test 'sxhash-positioned-user)))
    (puthash (copy-sequence "key") 'found table)
    (gethash (copy-sequence "key") table)))
"#,
        )
        .unwrap();
    assert_eq!(result, Value::symbol("found"));
}
