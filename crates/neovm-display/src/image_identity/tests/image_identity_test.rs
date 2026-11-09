use crate::emacs_core::image_catalog::ImageSpecIdentity;
use neovm_eval::emacs_core::eval::Context;

#[test]
fn image_identity_tracks_deep_list_and_vector_payloads() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    for wrapper in ["(list nested)", "(vector nested)"] {
        let setup = format!(
            "(progn (setq image-payload (list 1) nested image-payload n 0)
              (while (< n 512) (setq nested {wrapper} n (+ n 1)))
              (setq image-spec (list 'image :metadata nested)))"
        );
        let spec = eval.eval_str(&setup).unwrap();
        let before = ImageSpecIdentity::from_lisp_spec(&spec).unwrap();
        eval.eval_str("(setcar image-payload 2)").unwrap();
        let after = ImageSpecIdentity::from_lisp_spec(&spec).unwrap();
        assert_ne!(before, after, "deep mutation must change image identity");
        let independent = eval.eval_str(&setup).unwrap();
        let independent = ImageSpecIdentity::from_lisp_spec(&independent).unwrap();
        let cache = std::collections::HashMap::from([(before, true)]);
        assert_eq!(cache.get(&independent), Some(&true));
    }
}

#[test]
fn image_identity_ignores_acyclic_sharing_and_tracks_nested_cycles() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let shared = eval
        .eval_str("(progn (setq part (list 1 2)) (list 'image :metadata (vector part part)))")
        .unwrap();
    let shared = ImageSpecIdentity::from_lisp_spec(&shared).unwrap();
    let copied = eval
        .eval_str("(list 'image :metadata (vector (list 1 2) (list 1 2)))")
        .unwrap();
    assert_eq!(shared, ImageSpecIdentity::from_lisp_spec(&copied).unwrap());

    let setup = "(progn (setq cycle (cons 1 nil)) (setcdr cycle cycle)
                 (setq image-spec (list 'image :metadata cycle)))";
    let spec = eval.eval_str(setup).unwrap();
    let before = ImageSpecIdentity::from_lisp_spec(&spec).unwrap();
    eval.eval_str("(setcar cycle 2)").unwrap();
    assert_ne!(before, ImageSpecIdentity::from_lisp_spec(&spec).unwrap());
    let independent = eval.eval_str(setup).unwrap();
    assert_eq!(
        before,
        ImageSpecIdentity::from_lisp_spec(&independent).unwrap()
    );
}
