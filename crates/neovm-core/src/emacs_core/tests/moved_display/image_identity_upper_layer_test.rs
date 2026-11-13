//! image_identity_test tests re-homed to the core upper-layer harness.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::image_catalog::ImageSpecIdentity;
use neovm_eval::emacs_core::eval::Context;

#[test]
fn image_identity_owns_binary_data_across_mutation() {
    let mut eval = crate::test_utils::new_context();
    let spec = eval
        .eval_str("(progn (setq image-payload (unibyte-string 255 0)) (setq image-spec (list 'image :type 'png :data image-payload)))")
        .unwrap();
    let before = ImageSpecIdentity::from_lisp_spec(&spec).unwrap();
    eval.eval_str("(aset image-payload 1 1)").unwrap();
    let after = ImageSpecIdentity::from_lisp_spec(&spec).unwrap();
    assert_ne!(before, after, "binary mutation changes the catalog key");
    let original = eval
        .eval_str("(list 'image :type 'png :data (unibyte-string 255 0))")
        .unwrap();
    assert_eq!(
        before,
        ImageSpecIdentity::from_lisp_spec(&original).unwrap(),
        "the old identity owns its bytes independently of Lisp storage"
    );
    let cache = std::collections::HashMap::from([(before, "original"), (after, "changed")]);
    assert_eq!(
        cache.get(&ImageSpecIdentity::from_lisp_spec(&original).unwrap()),
        Some(&"original")
    );
}
