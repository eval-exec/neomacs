use super::*;
use crate::emacs_core::display_host::{FrameFontRequest, FrameFontSize};
use crate::emacs_core::eval::{
    Context, DisplayHost, FontPxProbeResult, GuiFrameHostRequest, ResolvedFrameFont,
};
use crate::emacs_core::font::font_spec;
use crate::emacs_core::value::Value;
use crate::face::{FontSlant, FontWeight, FontWidth};
use crate::window::FrameId;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

fn resolved_frame_font(height_tenths: i32) -> ResolvedFrameFont {
    ResolvedFrameFont {
        font: crate::emacs_core::eval::test_resolved_opened_font(
            "Monospace",
            None,
            None,
            FontWeight::NORMAL,
            FontSlant::Normal,
            FontWidth::Normal,
            Some("Monospace-Regular"),
            FontPxProbeResult {
                pixel_size: 15,
                height: 18,
                ascent: 14,
                descent: 4,
                max_width: 9,
                space_width: 8,
                average_width: 8,
            },
            None,
        ),
        height_tenths,
    }
}

struct CapturingFrameFontDisplayHost {
    requested_size: Rc<Cell<FrameFontSize>>,
    realized: ResolvedFrameFont,
}

struct FrameSpecificFontDisplayHost {
    selected_frame: FrameId,
    requests: Rc<RefCell<Vec<FrameId>>>,
}

impl DisplayHost for FrameSpecificFontDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        assert_eq!(
            request.size(),
            FrameFontSize::pixels(15).expect("positive pixel size")
        );
        self.requests.borrow_mut().push(frame_id);
        let height_tenths = if frame_id == self.selected_frame {
            // Cocoa-like logical coordinates.
            150
        } else {
            // Windows/Wayland-like logical coordinates.
            113
        };
        Ok(Some(resolved_frame_font(height_tenths)))
    }
}

fn integer_font_spec() -> Value {
    font_spec(vec![
        Value::keyword("family"),
        Value::string("Monospace"),
        Value::keyword("size"),
        Value::fixnum(15),
    ])
    .expect("create font spec")
}

impl DisplayHost for CapturingFrameFontDisplayHost {
    fn realize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resize_gui_frame(&mut self, _request: GuiFrameHostRequest) -> Result<(), String> {
        Ok(())
    }

    fn resolve_frame_font(
        &mut self,
        _frame_id: FrameId,
        request: FrameFontRequest,
    ) -> Result<Option<ResolvedFrameFont>, String> {
        self.requested_size.set(request.size());
        Ok(Some(self.realized.clone()))
    }
}

#[test]
fn live_font_spec_keeps_integer_size_in_pixels_until_frame_realization() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = eval
        .frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame");
    frame.set_window_system(Some(Value::symbol("neo")));

    let requested_size = Rc::new(Cell::new(FrameFontSize::Default));
    eval.set_display_host(Box::new(CapturingFrameFontDisplayHost {
        requested_size: Rc::clone(&requested_size),
        // The host models GNU Cocoa: 15 logical pixels are 150 tenths of a
        // point at FRAME_RES == PT_PER_INCH.
        realized: resolved_frame_font(150),
    }));
    let spec = integer_font_spec();

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            spec,
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("realize integer font-spec size");

    assert_eq!(
        requested_size.get(),
        FrameFontSize::pixels(15).expect("positive pixel size")
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
        .expect("realized default face height")
        .as_int(),
        Some(150)
    );
}

#[test]
fn frame_zero_realizes_integer_font_size_for_each_live_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let selected_frame = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let buffer_id = eval.buffers.current_buffer_id().expect("current buffer");
    let other_frame = eval.frames.create_frame("other", 800, 600, buffer_id);
    for frame_id in [selected_frame, other_frame] {
        eval.frame_manager_mut()
            .get_mut(frame_id)
            .expect("live frame")
            .set_window_system(Some(Value::symbol("neo")));
    }

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
            Value::fixnum(0),
        ],
    )
    .expect("realize font on every frame");

    for (frame_id, expected_height) in [(selected_frame, 150), (other_frame, 113)] {
        assert_eq!(
            builtin_internal_get_lisp_face_attribute(
                &mut eval,
                vec![
                    Value::symbol("default"),
                    Value::keyword(":height"),
                    Value::make_frame(frame_id.0),
                ],
            )
            .expect("frame-local default face height")
            .as_int(),
            Some(expected_height)
        );
    }
    let requests = requests.borrow();
    assert!(requests.contains(&selected_frame));
    assert!(requests.contains(&other_frame));
}

#[test]
fn frame_t_realizes_new_frame_defaults_against_selected_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
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
