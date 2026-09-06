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
    resolved_frame_font_with_line_height(height_tenths, 18)
}

fn resolved_frame_font_with_line_height(height_tenths: i32, line_height: i32) -> ResolvedFrameFont {
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
                height: line_height,
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

#[test]
fn default_face_font_change_resizes_mini_window_to_one_line_of_the_new_font() {
    // A frame created with an 11px font keeps an 11px minibuffer window.
    // Switching the default face to a font with 18px lines must leave the
    // minibuffer one line of the NEW font tall, with the root window ending
    // where the minibuffer starts.  GNU: `ns_new_font` (src/nsterm.m:11424-
    // 11428, emacs-31.0.90) calls `adjust_frame_size`, whose
    // `resize_frame_windows` (src/window.c:5052-5055,5118-5128) gives the
    // mini-window `unit + decorations` pixels where `unit` is the new
    // `FRAME_LINE_HEIGHT`.  Without this the echo area shows only the top
    // 11px of an 18px text line (the clipped-echo-area bug on macOS).
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.width = 502;
        frame.height = 430;
        frame.char_width = 6.0;
        frame.char_height = 11.0;
        let mini = frame.minibuffer_leaf.as_mut().expect("own minibuffer");
        let mut bounds = *mini.bounds();
        bounds.height = 11.0;
        mini.set_bounds(bounds);
        frame.sync_window_area_bounds();
    }

    eval.set_display_host(Box::new(CapturingFrameFontDisplayHost {
        requested_size: Rc::new(Cell::new(FrameFontSize::Default)),
        // `resolved_frame_font` realizes a font whose line height is 18px.
        realized: resolved_frame_font(150),
    }));

    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            integer_font_spec(),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("realize the new default font");

    let frame = eval.frames.get(frame_id).expect("selected frame");
    assert_eq!(
        frame.char_height, 18.0,
        "frame line height follows the font"
    );
    let mini = frame
        .minibuffer_leaf
        .as_ref()
        .expect("own minibuffer")
        .bounds();
    let root = frame.root_window.bounds();
    assert_eq!(
        mini.height, 18.0,
        "minibuffer window is one line of the new font (was 11px)"
    );
    assert_eq!(
        mini.y + mini.height,
        frame.height as f32,
        "minibuffer ends at the frame bottom"
    );
    assert_eq!(
        root.y + root.height,
        mini.y,
        "root window ends where the minibuffer starts"
    );
}

/// Drive the selected GUI frame (11px lines, mini-window `mini_height`) through
/// a default-face font change whose realized line height is `line_height`, and
/// return the frame for assertions.
fn frame_after_default_font_change(
    mini_height: Option<f32>,
    line_height: i32,
) -> (Context, FrameId) {
    let mut eval = Context::new();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    {
        let frame = eval
            .frame_manager_mut()
            .get_mut(frame_id)
            .expect("selected frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.width = 502;
        frame.height = 430;
        frame.char_width = 6.0;
        frame.char_height = 11.0;
        match mini_height {
            Some(height) => {
                let mini = frame.minibuffer_leaf.as_mut().expect("own minibuffer");
                let mut bounds = *mini.bounds();
                bounds.height = height;
                mini.set_bounds(bounds);
            }
            // A frame whose minibuffer lives on another frame.
            None => frame.minibuffer_leaf = None,
        }
        frame.sync_window_area_bounds();
    }
    eval.set_display_host(Box::new(CapturingFrameFontDisplayHost {
        requested_size: Rc::new(Cell::new(FrameFontSize::Default)),
        realized: resolved_frame_font_with_line_height(150, line_height),
    }));
    builtin_internal_set_lisp_face_attribute(
        &mut eval,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            integer_font_spec(),
            Value::make_frame(frame_id.0),
        ],
    )
    .expect("realize the new default font");
    (eval, frame_id)
}

#[test]
fn default_face_font_change_resets_a_grown_mini_window_to_one_line() {
    // GNU `resize_frame_windows` gives the mini-window one line of the new
    // unit whatever it held before (window.c:5051-5053,5125-5128); a
    // three-line 33px mini-window under the old 11px font becomes 18px.
    let (eval, frame_id) = frame_after_default_font_change(Some(33.0), 18);
    let frame = eval.frames.get(frame_id).expect("selected frame");
    let mini = frame
        .minibuffer_leaf
        .as_ref()
        .expect("own minibuffer")
        .bounds();
    assert_eq!(mini.height, 18.0);
    assert_eq!(mini.y + mini.height, 430.0);
}

#[test]
fn default_face_font_change_with_the_same_line_height_keeps_the_mini_window() {
    // Same line height, different font: no `resize_frame_windows` vertical
    // pass, so a grown mini-window keeps its height and only the edges resync.
    let (eval, frame_id) = frame_after_default_font_change(Some(33.0), 11);
    let frame = eval.frames.get(frame_id).expect("selected frame");
    assert_eq!(frame.char_height, 11.0);
    let mini = frame
        .minibuffer_leaf
        .as_ref()
        .expect("own minibuffer")
        .bounds();
    assert_eq!(
        mini.height, 33.0,
        "unchanged line height leaves the mini-window alone"
    );
    assert_eq!(mini.y + mini.height, 430.0);
}

#[test]
fn default_face_font_change_resyncs_a_frame_without_its_own_mini_window() {
    // `FRAME_HAS_MINIBUF_P && !FRAME_MINIBUF_ONLY_P` is false for a frame
    // whose minibuffer lives elsewhere (window.c:5051): no mini-window rule,
    // but the root window still spans the frame in the new units.
    let (eval, frame_id) = frame_after_default_font_change(None, 18);
    let frame = eval.frames.get(frame_id).expect("selected frame");
    assert_eq!(frame.char_height, 18.0);
    assert!(frame.minibuffer_leaf.is_none());
    let root = frame.root_window.bounds();
    assert_eq!(
        root.y + root.height,
        430.0,
        "root window spans to the frame bottom"
    );
}
