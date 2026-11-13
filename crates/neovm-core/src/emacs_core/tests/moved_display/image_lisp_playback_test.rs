//! Re-homed FULL display image test (create_bootstrap_evaluator terminal).
//! Body is verbatim from the display source.

use neovm_value::value::Value;

#[test]
fn svg_animation_lisp_playback_behavior() {
    let mut eval = crate::emacs_core::load_boot::create_bootstrap_evaluator()
        .expect("image playback tests need the real Lisp image API");
    let fixture = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test/lisp/neomacs-image-tests.el");
    let form = format!(
        "(progn (load {:?} nil t t)\n\
         (ert-stats-completed-unexpected\n\
           (ert-run-tests-batch \"^neomacs-image-tests-\")))",
        fixture.to_string_lossy()
    );
    assert_eq!(
        eval.eval_str(&form)
            .expect("the real playback functions should satisfy the ERT scenarios"),
        Value::fixnum(0),
        "all image playback ERT cases must pass"
    );
}
