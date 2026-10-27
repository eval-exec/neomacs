use super::*;
use crate::emacs_core::display_host::InitialGuiFrameDefaults;
use crate::window::{FrameDisplayIdentity, FrameId, FrameVisibility};
use neomacs_display_protocol::{GraphicalBackend, GraphicalDisplayIdentity};

fn initial_host() -> RecordingDisplayHost {
    RecordingDisplayHost {
        initial_gui_defaults: Some(InitialGuiFrameDefaults {
            display: GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11),
            font: resolved_frame_font(
                "Initial Mono",
                "InitialMono-Regular",
                130,
                FontPxProbeResult {
                    pixel_size: 17,
                    height: 18,
                    ascent: 14,
                    descent: 4,
                    max_width: 9,
                    space_width: 9,
                    average_width: 9,
                },
            ),
            size: GuiFrameHostSize {
                width: 745,
                height: 680,
            },
        }),
        ..RecordingDisplayHost::default()
    }
}

fn terminal_startup_context() -> Context {
    let mut eval = Context::new();
    let buffer = eval.buffers.create_buffer("*scratch*");
    eval.buffers.set_current(buffer);
    let terminal = eval.frames.create_frame("F1", 80, 25, buffer);
    let frame = eval.frames.get_mut(terminal).unwrap();
    frame.set_window_system(None);
    frame.char_width = 1.0;
    frame.char_height = 1.0;
    frame.displays_chrome = false;
    eval
}

#[test]
fn first_graphical_frame_uses_opened_host_font_instead_of_terminal_cells() {
    let mut eval = terminal_startup_context();
    let host = initial_host();
    let requests = host.realized.clone();
    eval.set_display_host(Box::new(host));

    let value = eval
        .eval_str("(x-create-frame '((name . \"Initial\")))")
        .unwrap();
    let frame = eval
        .frames
        .get(FrameId(value.as_frame_id().unwrap()))
        .unwrap();
    assert_eq!(frame.char_width, 9.0);
    assert_eq!(frame.char_height, 18.0);
    assert_eq!(frame.font_pixel_size, 17.0);
    assert_eq!(frame.font_ascent, 14.0);
    assert!(
        matches!(frame.display_identity(), FrameDisplayIdentity::Graphical(display)
        if display.backend() == GraphicalBackend::X11)
    );
    assert_eq!((frame.width, frame.height), (745, 680));
    assert_eq!(
        (requests.borrow()[0].width, requests.borrow()[0].height),
        (745, 680)
    );
    assert!(!frame.parameter("font-parameter").unwrap().is_nil());
}

#[test]
fn raw_visible_initial_frame_finishes_without_make_frame_wrapper() {
    let mut eval = terminal_startup_context();
    let host = initial_host();
    let finished = host.finished.clone();
    eval.set_display_host(Box::new(host));

    let value = eval.eval_str("(x-create-frame nil)").unwrap();
    let frame = eval
        .frames
        .get(FrameId(value.as_frame_id().unwrap()))
        .unwrap();
    let finished = finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].frame_id, frame.id);
    assert_eq!((finished[0].width, finished[0].height), (745, 680));
    assert_eq!(finished[0].geometry_hints, frame.gui_geometry_hints());
    assert_eq!(finished[0].visibility, FrameVisibility::Visible);
    assert!(finished[0].decorated);
    assert_eq!(frame.char_height, 18.0);
}

#[test]
fn raw_invisible_initial_frame_defers_finish_until_lisp_finalization() {
    let mut eval = terminal_startup_context();
    let host = initial_host();
    let finished = host.finished.clone();
    eval.set_display_host(Box::new(host));

    eval.eval_str("(x-create-frame '((visibility . nil)))")
        .unwrap();
    assert!(finished.borrow().is_empty());
}

#[test]
fn frame_creation_finish_observes_final_lisp_dimensions() {
    let mut eval = terminal_startup_context();
    let host = initial_host();
    let finished = host.finished.clone();
    eval.set_display_host(Box::new(host));
    let value = eval
        .eval_str("(x-create-frame '((name . \"Initial\") (visibility . nil)))")
        .unwrap();
    eval.set_variable("issue417-frame", value);
    assert!(finished.borrow().is_empty());

    eval.eval_str("(set-frame-size issue417-frame 81 27)")
        .unwrap();
    eval.eval_str("(frame-after-make-frame issue417-frame t)")
        .unwrap();
    let frame = eval
        .frames
        .get(FrameId(value.as_frame_id().unwrap()))
        .unwrap();
    let finished = finished.borrow();
    assert_eq!(finished.len(), 1);
    assert_eq!(finished[0].frame_id, frame.id);
    assert_eq!(
        (finished[0].width, finished[0].height),
        (frame.width, frame.height)
    );
    assert_eq!(finished[0].geometry_hints, frame.gui_geometry_hints());
    // Geometry completion must retain the original invisible request; it
    // does not authorize the native backend to expose this logical frame.
    assert_eq!(finished[0].visibility, FrameVisibility::Invisible);
    assert!(finished[0].decorated);
}

#[test]
fn initial_gui_default_face_starts_from_its_opened_font_before_lisp_finalization() {
    let mut eval = terminal_startup_context();
    eval.set_display_host(Box::new(initial_host()));
    eval.eval_str("(setq issue417-frame (x-create-frame nil))")
        .unwrap();

    let family = eval
        .eval_str("(internal-get-lisp-face-attribute 'default :family issue417-frame)")
        .unwrap();
    let height = eval
        .eval_str("(internal-get-lisp-face-attribute 'default :height issue417-frame)")
        .unwrap();
    assert_eq!(family.as_utf8_str(), Some("Initial Mono"));
    assert_eq!(height, Value::fixnum(130));
}

#[test]
fn initial_gui_font_seeding_preserves_early_init_new_frame_overrides() {
    let mut eval = terminal_startup_context();
    eval.eval_str("(internal-set-lisp-face-attribute 'default :family \"Configured Mono\" t)")
        .unwrap();
    eval.eval_str("(internal-set-lisp-face-attribute 'default :height 210 t)")
        .unwrap();
    eval.set_display_host(Box::new(initial_host()));
    eval.eval_str("(setq issue417-frame (x-create-frame nil))")
        .unwrap();

    // GNU's faces.el applies new-frame overrides after raw frame creation.
    // Seeding the opened default must not replace that later authority.
    eval.eval_str("(internal-merge-in-global-face 'default issue417-frame)")
        .unwrap();
    for frame in ["t", "issue417-frame"] {
        let family = eval
            .eval_str(&format!(
                "(internal-get-lisp-face-attribute 'default :family {frame})"
            ))
            .unwrap();
        let height = eval
            .eval_str(&format!(
                "(internal-get-lisp-face-attribute 'default :height {frame})"
            ))
            .unwrap();
        assert_eq!(family.as_utf8_str(), Some("Configured Mono"));
        assert_eq!(height, Value::fixnum(210));
    }
}
