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

#[test]
fn image_identity_filters_gnu_animation_bookkeeping_keys_only() {
    // GNU `filter_image_spec` (src/image.c:2259-2288) strips :animate-buffer,
    // :animate-tardiness, and :animate-position so `image-animate-timeout`
    // does not bust the image cache on every tick (#823).
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let base = eval
        .eval_str("(list 'image :type 'gif :file \"anim.gif\" :index 0)")
        .unwrap();
    let with_bookkeeping = eval
        .eval_str(
            "(list 'image :type 'gif :file \"anim.gif\" :index 0 \
                   :animate-buffer 'anim-buf \
                   :animate-tardiness 0.125 \
                   :animate-position (list 42))",
        )
        .unwrap();
    let mutated_tardiness = eval
        .eval_str(
            "(list 'image :type 'gif :file \"anim.gif\" :index 0 \
                   :animate-tardiness 1.75)",
        )
        .unwrap();
    let next_frame = eval
        .eval_str(
            "(list 'image :type 'gif :file \"anim.gif\" :index 1 \
                   :animate-tardiness 1.75)",
        )
        .unwrap();

    let base_id = ImageSpecIdentity::from_lisp_spec(&base).unwrap();
    assert_eq!(
        base_id,
        ImageSpecIdentity::from_lisp_spec(&with_bookkeeping).unwrap(),
        "animation bookkeeping keys must not change image cache identity"
    );
    assert_eq!(
        base_id,
        ImageSpecIdentity::from_lisp_spec(&mutated_tardiness).unwrap(),
        "mutating :animate-tardiness must keep the same cache identity"
    );
    assert_ne!(
        base_id,
        ImageSpecIdentity::from_lisp_spec(&next_frame).unwrap(),
        "changing :index must still select a distinct frame identity"
    );
}
