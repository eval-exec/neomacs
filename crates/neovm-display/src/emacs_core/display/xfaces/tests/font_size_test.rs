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

#[test]
fn frame_t_font_on_selected_tty_validates_without_changing_defaults() {
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    assert!(
        eval.frame_manager()
            .get(frame)
            .unwrap()
            .effective_window_system()
            .is_none()
    );
    let defaults = ensure_global_lisp_face_vector(&mut eval, "default").unwrap();
    let before = defaults.as_vector_data().unwrap().as_slice().to_vec();
    for font in [Value::string("Monospace-12"), integer_font_spec()] {
        builtin_internal_set_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword("font"),
                font,
                Value::T,
            ],
        )
        .expect("a valid font is ignored for TTY new-frame defaults");
        assert_eq!(
            defaults.as_vector_data().unwrap().as_slice(),
            before.as_slice()
        );
    }
    for font in [Value::fixnum(42), Value::NIL] {
        assert!(
            builtin_internal_set_lisp_face_attribute(
                &mut eval,
                vec![
                    Value::symbol("default"),
                    Value::keyword("font"),
                    font,
                    Value::T,
                ]
            )
            .is_err(),
            "validation must precede the TTY no-store gate"
        );
        assert_eq!(
            defaults.as_vector_data().unwrap().as_slice(),
            before.as_slice()
        );
    }
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::symbol("unspecified"),
            Value::T,
        ],
    )
    .expect("reset values are stored even with a selected TTY");
    assert_eq!(
        lisp_face_vector_attr(defaults, LFaceAttr::Font),
        Some(ignore_defface_symbol())
    );
}

#[test]
fn unavailable_numeric_font_name_retries_the_literal_family() {
    struct LiteralFamilyHost {
        calls: Rc<RefCell<Vec<(String, FrameFontSize)>>>,
        resolved: ResolvedFrameFont,
    }
    impl DisplayHost for LiteralFamilyHost {
        fn realize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resize_gui_frame(&mut self, _: GuiFrameHostRequest) -> Result<(), String> {
            Ok(())
        }
        fn resolve_frame_font(
            &mut self,
            _: FrameId,
            request: FrameFontRequest,
        ) -> Result<Option<ResolvedFrameFont>, String> {
            let family = request.face().family_runtime_string_owned().unwrap();
            self.calls
                .borrow_mut()
                .push((family.clone(), request.size()));
            Ok((family == "Missing Font-17").then(|| self.resolved.clone()))
        }
    }
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let frame = crate::test_utils::xfaces_upper_layer_helpers::ensure_selected_gui_frame(&mut eval);
    eval.frame_manager_mut()
        .get_mut(frame)
        .unwrap()
        .font_pixel_size = 15.0;
    let calls = Rc::new(RefCell::new(Vec::new()));
    eval.set_display_host(Box::new(LiteralFamilyHost {
        calls: Rc::clone(&calls),
        resolved: crate::test_utils::xfaces_upper_layer_helpers::resolved_frame_font(
            "Missing Font-17",
            "LiteralFamily-Regular",
            110,
            FontPxProbeResult {
                pixel_size: 15,
                height: 19,
                ascent: 15,
                descent: 4,
                max_width: 8,
                space_width: 8,
                average_width: 8,
            },
        ),
    }));
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Missing Font-17"),
            Value::make_frame(frame.0),
        ],
    )
    .expect("the literal family retry can supply an available font");
    assert_eq!(
        calls.borrow().as_slice(),
        &[
            (
                "Missing Font".to_owned(),
                FrameFontSize::points(17.0).unwrap()
            ),
            (
                "Missing Font-17".to_owned(),
                FrameFontSize::pixels(15).unwrap()
            ),
        ]
    );
    let font = eval
        .frame_manager()
        .get(frame)
        .unwrap()
        .fixed_parameter(crate::window::FrameQueryParam::FontParameter)
        .unwrap();
    assert_eq!(
        crate::emacs_core::font::font_get(vec![font, Value::keyword("family")])
            .unwrap()
            .as_bare_symbol_name(),
        Some("Missing Font-17")
    );
}
