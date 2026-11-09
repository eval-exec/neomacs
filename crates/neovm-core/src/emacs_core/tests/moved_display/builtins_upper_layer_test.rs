//! Xfaces builtin tests that need the core upper layers.
//!
//! Re-homed from neovm-display (display upper-layer re-home).
use crate::emacs_core::display_host::FontResolveRequest;
use crate::emacs_core::font::*;
use crate::emacs_core::xfaces::*;
use crate::face::{Color, FaceAttrValue, FaceHeight};
use crate::window::DisplayFrameExt;
use crate::window::DisplayWindowExt;
use crate::window::{FRAME_ID_BASE, FrameId, FrameParam};
use neovm_display::test_support::authority::xfaces_ensure_selected_gui_frame_for_test as ensure_selected_gui_frame;
use neovm_display::test_utils::xfaces_upper_layer_helpers::resolved_frame_font;
use neovm_edit::buffer::{Buffer, CharPos0, CharRange};
use neovm_eval::emacs_core::eval::data::face_registry::FIRST_DYNAMIC_FACE_ID;
use neovm_eval::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFontMatch,
    ResolvedFrameFont,
};
use neovm_value::error::{FlowKind, FlowResultExt as _};
use neovm_value::value::Value;
use neovm_value::value::{ValueKind, VecLikeType};

#[test]
fn internal_set_lisp_face_attribute_eval_realizes_string_font_requests_for_live_default_face() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }
    eval.set_display_host(Box::new(
        neovm_display::test_support::authority::live_frame_font_display_host_for_test(Some(
            resolved_frame_font(
                "Noto Sans Mono",
                "NotoSansMono-Regular",
                160,
                FontPxProbeResult {
                    pixel_size: 22,
                    height: 31,
                    ascent: 23,
                    descent: 8,
                    max_width: 13,
                    space_width: 13,
                    average_width: 13,
                },
            ),
        )),
    ));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set live default face font from string");

    let frame = eval
        .frame_manager()
        .get(frame_id)
        .expect("selected frame after font change");
    let public_font_name = frame.parameter("font").expect("public frame font name");
    let font_parameter = frame
        .parameter("font-parameter")
        .expect("font-parameter should be set");
    assert!(
        fontp(vec![font_parameter, Value::symbol("font-object")])
            .expect("font-object check")
            .is_truthy()
    );
    assert_eq!(frame.char_width, 13.0);
    assert_eq!(frame.char_height, 31.0);
    assert_eq!(frame.font_pixel_size, 22.0);

    let face_font_name = builtin_face_font(
        &mut eval,
        &vec![Value::symbol("default"), Value::make_frame(frame_id.0)],
    )
    .expect("default face font name");
    assert_eq!(
        public_font_name.as_utf8_str(),
        face_font_name.as_utf8_str(),
        "GNU commits the realized font name so default-font-height can use frame metrics"
    );

    let default_font = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":font"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("default face font");
    let opened_font = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
        .expect("font-parameter should retain the realized font object");
    assert!(
        fontp(vec![default_font, Value::symbol("font-object")])
            .expect("live default face :font type check")
            .is_truthy(),
        "GNU exposes a realized font object from a live graphical face"
    );
    assert_eq!(default_font, opened_font);
    assert_eq!(
        font_get(vec![opened_font, Value::keyword(":family")])
            .expect("default font family")
            .as_bare_symbol_name(),
        Some("Noto Sans Mono")
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":height"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face height")
        .as_int(),
        Some(160)
    );
}

#[test]
fn internal_set_lisp_face_attribute_eval_uses_resolved_point_height_when_font_request_has_no_size()
{
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.window_system = Some(Value::symbol("neo"));
    }
    eval.set_display_host(Box::new(
        neovm_display::test_support::authority::live_frame_font_display_host_for_test(Some(
            resolved_frame_font(
                "Noto Sans Mono",
                "NotoSansMono-Regular",
                102,
                FontPxProbeResult {
                    pixel_size: 22,
                    height: 31,
                    ascent: 23,
                    descent: 8,
                    max_width: 13,
                    space_width: 13,
                    average_width: 13,
                },
            ),
        )),
    ));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set live default face font without size");

    let default_font = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword(":font"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("default face font");
    let opened_font = eval
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.parameter("font-parameter"))
        .expect("font-parameter should retain the realized font object");
    assert_eq!(default_font, opened_font);
    assert_eq!(
        font_get(vec![opened_font, Value::keyword(":size")])
            .expect("default font size")
            .as_int(),
        Some(22)
    );
    assert_eq!(
        builtin_internal_get_lisp_face_attribute(
            &mut eval,
            vec![
                Value::symbol("default"),
                Value::keyword(":height"),
                Value::make_frame(frame_id.0),
            ],
        )
        .expect("default face height")
        .as_int(),
        Some(102)
    );
}

#[test]
fn internal_set_face_height_accepts_function_height_like_gnu() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("neo-height-face"),
            Value::keyword(":height"),
            Value::symbol("identity"),
            Value::NIL,
        ],
    )
    .expect("GNU accepts function-valued non-default face height");

    let stored = builtin_internal_get_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("neo-height-face"),
            Value::keyword(":height"),
            Value::NIL,
        ],
    )
    .expect("read stored height");
    assert_eq!(stored, Value::symbol("identity"));
}

#[test]
fn default_face_font_attr_update_refreshes_live_frame_font_state() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let frame_id = ensure_selected_gui_frame(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.install_gnu_gui_default_parameters();
        frame.set_known_parameter(FrameParam::Font, Value::string("JetBrains Mono-10"));
        frame.char_width = 7.8;
        frame.char_height = 18.0;
        frame.font_pixel_size = 14.0;
    }
    eval.set_display_host(Box::new(
        neovm_display::test_support::authority::live_frame_font_display_host_for_test(Some(
            resolved_frame_font(
                "JetBrains Mono",
                "JetBrainsMono-Regular",
                90,
                FontPxProbeResult {
                    pixel_size: 13,
                    height: 17,
                    ascent: 13,
                    descent: 4,
                    max_width: 7,
                    space_width: 7,
                    average_width: 7,
                },
            ),
        )),
    ));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("family"),
            Value::string("JetBrains Mono"),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set default family");
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("height"),
            Value::fixnum(90),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("set default height");

    let frame = eval.frame_manager().get(frame_id).expect("selected frame");
    assert!(
        frame
            .parameter("font-parameter")
            .is_some_and(|value| value.is_font_object()),
        "default face font attr changes should refresh internal font-parameter"
    );
    assert_eq!(frame.char_width, 7.0);
    assert_eq!(frame.char_height, 17.0);
    assert_eq!(frame.font_pixel_size, 13.0);
    assert_ne!(
        frame.known_parameter(FrameParam::Font),
        Some(Value::string("JetBrains Mono-10")),
        "frame font parameter must not remain at the stale pre-face-change font"
    );
}
