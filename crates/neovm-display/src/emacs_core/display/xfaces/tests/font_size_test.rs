use super::*;
use crate::emacs_core::display_host::{FrameFontRequest, FrameFontSize};
use crate::emacs_core::font::font_spec;
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::test_utils::xfaces_upper_layer_helpers::{
    CapturingFrameFontDisplayHost, FrameSpecificFontDisplayHost, integer_font_spec,
};
use crate::window::FrameId;
use neovm_eval::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFrameFont,
};
use neovm_value::value::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[test]
fn frame_t_realizes_new_frame_defaults_against_selected_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let selected_frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frame_manager_mut()
        .get_mut(selected_frame)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol("neo")));

    let requests = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(FrameSpecificFontDisplayHost {
        selected_frame,
        requests: Rc::clone(&requests),
    }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            integer_font_spec(),
            Value::T,
        ],
    )
    .expect("realize new-frame defaults");

    let defaults = lookup_face_new_frame_defaults_vector(&eval, Value::symbol("default"))
        .expect("new-frame default face vector");
    assert_eq!(
        lisp_face_vector_attr(defaults, LFaceAttr::Height).and_then(|height| height.as_int()),
        Some(150)
    );
    assert_eq!(requests.borrow().as_slice(), &[selected_frame]);
}

#[test]
fn unavailable_explicit_font_preserves_face_and_frame_state() {
    use crate::test_utils::xfaces_upper_layer_helpers::LiveFrameFontDisplayHost;
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .set_window_system(Some(Value::symbol("neo")));
    let before = ensure_frame_lisp_face_vector_by_symbol(
        &mut eval,
        frame,
        Value::symbol("default"),
        "default",
        FrameFaceInitial::SelectedBase,
    )
    .unwrap();
    let before_attrs = before.as_vector_data().unwrap().as_slice().to_vec();
    let before_font = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .fixed_parameter(crate::window::FrameQueryParam::FontParameter);
    eval.set_display_host(Box::new(LiveFrameFontDisplayHost { realized: None }));
    let spec = integer_font_spec();
    let error = builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            spec,
            Value::NIL,
        ],
    )
    .expect_err("an explicit unavailable GUI font must signal");
    let neovm_value::error::FlowKind::Signal(signal) = error.into_kind() else {
        panic!("expected Font not available signal");
    };
    assert_eq!(signal.symbol_name(), "error");
    assert_eq!(signal.data[0].as_utf8_str(), Some("Font not available"));
    assert_eq!(signal.data[1], spec);
    assert_eq!(
        before.as_vector_data().unwrap().as_slice().to_vec(),
        before_attrs
    );
    assert_eq!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .fixed_parameter(crate::window::FrameQueryParam::FontParameter),
        before_font
    );
}
