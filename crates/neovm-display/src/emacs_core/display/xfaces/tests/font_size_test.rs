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
