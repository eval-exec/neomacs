use super::*;
use crate::window::DisplayFrameManagerExt;
use neovm_eval::emacs_core::eval::Context;
use neovm_value::intern::{intern, intern_call_names, reset_intern_calls};

fn context() -> (Context, crate::window::FrameId) {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.buffer_manager_mut().create_buffer("fixed-image-keys");
    let frame = eval
        .frame_manager_mut()
        .create_frame("fixed-image-keys", 800, 600, buffer);
    (eval, frame)
}

fn environment(eval: &Context, frame: crate::window::FrameId) -> ImageScaleEnvironment {
    image_scale_environment(
        eval.frame_manager().get(frame).expect("frame"),
        eval.variables(),
    )
}

#[test]
fn fixed_image_keys_keep_dynamic_values_and_unwind() {
    let (mut eval, frame) = context();
    eval.obarray_mut()
        .set_symbol_value("image-scaling-factor", Value::make_float(1.25));
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::fixnum(72));
    eval.project_thread_variable_for_test(ImageVariable::ScalingFactor.sym_id())
        .expect("plain scaling variable projects");
    eval.project_thread_variable_for_test(ImageVariable::SizeLimit.sym_id())
        .expect("plain size variable projects");
    let original = environment(&eval, frame);
    assert_eq!(
        original.default_scale,
        ImageDefaultScale::Explicit(ImageScaleFactor::try_from(1.25).expect("scale"))
    );
    assert_eq!(original.size_limit().maximum().dimensions(), (72, 72));
    let count = eval.specpdl().len();
    eval.try_specbind(
        ImageVariable::ScalingFactor.sym_id(),
        Value::make_float(2.0),
    )
    .expect("bind scale");
    eval.try_specbind(ImageVariable::SizeLimit.sym_id(), Value::fixnum(96))
        .expect("bind limit");
    let bound = environment(&eval, frame);
    assert_eq!(
        bound.default_scale,
        ImageDefaultScale::Explicit(ImageScaleFactor::try_from(2.0).expect("scale"))
    );
    assert_eq!(bound.size_limit().maximum().dimensions(), (96, 96));
    eval.unbind_to(count);
    assert_eq!(environment(&eval, frame), original);
}

#[test]
fn fixed_image_keys_match_by_name_for_void_and_non_numeric_values() {
    let (mut eval, frame) = context();
    let scale = ImageVariable::ScalingFactor.sym_id();
    let limit = ImageVariable::SizeLimit.sym_id();
    for value in [
        Value::NIL,
        Value::fixnum(3),
        Value::make_float(0.5),
        Value::symbol("auto"),
    ] {
        eval.obarray_mut().set_symbol_value_id(scale, value);
        eval.obarray_mut().set_symbol_value_id(limit, value);
        assert_eq!(
            eval.variables().value(scale),
            eval.variables().value_by_name("image-scaling-factor")
        );
        assert_eq!(
            eval.variables().value(limit),
            eval.variables().value_by_name("max-image-size")
        );
        let result = environment(&eval, frame);
        assert_eq!(
            result.size_limit(),
            image_size_limit(
                eval.frame_manager().get(frame).expect("frame"),
                eval.variables()
            )
        );
    }
    let empty = neovm_value::symbol::Obarray::new();
    let view = neovm_eval::emacs_core::eval::VariableView::canonical(
        neovm_value::symbol::CanonicalAccess::TEST,
        &empty,
    );
    assert_eq!(
        view.value(scale),
        view.value_by_name("image-scaling-factor")
    );
    assert_eq!(view.value(limit), view.value_by_name("max-image-size"));
    let absent = image_scale_environment(eval.frame_manager().get(frame).expect("frame"), view);
    assert_eq!(absent.default_scale, ImageDefaultScale::Auto);
    assert_eq!(absent.size_limit(), ImageSizeLimit::UNLIMITED);
    // Canonical IDs are fixed keys, not a memo of any resulting variable value.
    assert_eq!(scale, intern("image-scaling-factor"));
    assert_eq!(limit, intern("max-image-size"));
}

#[test]
fn fixed_image_warm_queries_do_not_intern_variable_spellings() {
    let (mut eval, frame) = context();
    eval.obarray_mut()
        .set_symbol_value("image-scaling-factor", Value::make_float(1.5));
    eval.obarray_mut()
        .set_symbol_value("max-image-size", Value::fixnum(64));
    let expected = environment(&eval, frame);
    reset_intern_calls();
    for _ in 0..128 {
        assert_eq!(environment(&eval, frame), expected);
    }
    assert!(intern_call_names().is_empty());
}
