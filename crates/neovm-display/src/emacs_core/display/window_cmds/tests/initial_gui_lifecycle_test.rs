use super::*;
use crate::emacs_core::display_host::InitialGuiFrameDefaults;
use crate::window::{DisplayFrameExt, DisplayFrameManagerExt};
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
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    let buffer = eval.edit.buffers.create_buffer("*scratch*");
    eval.edit.buffers.set_current(buffer);
    let terminal = eval.frame_manager_mut().create_frame("F1", 80, 25, buffer);
    let frame = eval.frame_manager_mut().get_mut(terminal).unwrap();
    frame.set_window_system(None);
    frame.char_width = 1.0;
    frame.char_height = 1.0;
    frame.displays_chrome = false;
    eval
}

#[test]
fn window_system_variable_follows_initial_frame_selection_and_dynamic_binding() {
    // GNU dispnew.c DEFVAR_KBOARD forwards window-system through the
    // selected frame's keyboard. Initial terminal state must not become a
    // permanent global nil after Lisp selects its first graphical frame.
    let mut eval = terminal_startup_context();
    let terminal = eval.frame_manager().selected_frame().unwrap().id;
    crate::emacs_core::terminal::pure::configure_terminal_runtime(
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::window_system(
            GraphicalDisplayIdentity::anonymous_connection(GraphicalBackend::X11),
        ),
    );
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        1,
        "initial_terminal",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::inactive(),
    );
    eval.frame_manager_mut()
        .get_mut(terminal)
        .unwrap()
        .terminal_id = 1;
    eval.set_variable("window-system", Value::NIL);
    eval.set_variable("initial-window-system", Value::symbol("neo"));
    eval.set_variable("issue417-terminal", Value::make_frame(terminal.0));
    eval.set_display_host(Box::new(initial_host()));
    let graphical = eval
        .eval_str("(x-create-frame '((visibility . nil)))")
        .unwrap();
    eval.set_variable("issue417-graphical", graphical);

    let observed: Vec<_> = eval
        .eval_str_each(
            "(progn
               (select-frame issue417-terminal)
               (list window-system (symbol-value 'window-system) (window-system)))
             (progn
               (select-frame issue417-graphical)
               (list window-system (symbol-value 'window-system) (window-system)))
             (let ((window-system 'w32))
               (select-frame issue417-graphical)
               (list window-system
                     (progn (select-frame issue417-terminal) window-system)
                     (progn (select-frame issue417-graphical) window-system)
                     (symbol-value 'window-system)
                     (window-system)))
             window-system
             (progn
               (select-frame issue417-terminal)
               (list window-system (symbol-value 'window-system) (window-system)))
             (progn
               (select-frame issue417-graphical)
               (list window-system (symbol-value 'window-system) (window-system)))",
        )
        .iter()
        .map(format_eval_result)
        .collect();
    assert_eq!(
        observed,
        [
            "OK (nil nil nil)",
            "OK (neo neo neo)",
            "OK (w32 nil w32 w32 neo)",
            "OK neo",
            "OK (nil nil nil)",
            "OK (neo neo neo)",
        ]
    );
}

#[test]
fn graphical_terminal_initialization_replaces_cached_tty_keyboard_lifetime() {
    let mut eval = terminal_startup_context();
    let terminal = eval.frame_manager().selected_frame().unwrap().id;
    eval.set_variable("issue417-terminal", Value::make_frame(terminal.0));
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(progn (select-frame issue417-terminal) window-system)")
        ),
        "OK nil",
    );
    let depth = eval.specpdl().len();
    eval.try_specbind(
        neovm_value::intern::intern("window-system"),
        Value::symbol("w32"),
    )
    .unwrap();

    eval.initialize_graphical_terminal(GraphicalDisplayIdentity::anonymous_connection(
        GraphicalBackend::X11,
    ));
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        1,
        "initial_terminal",
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::inactive(),
    );
    eval.frame_manager_mut()
        .get_mut(terminal)
        .unwrap()
        .terminal_id = 1;
    // The host reassigned this frame in place. Lisp's same-frame selection
    // is a no-op, so explicitly publish the new terminal ownership.
    eval.sync_keyboard_terminal_owner();
    eval.set_display_host(Box::new(initial_host()));
    eval.eval_str("(select-frame issue417-terminal)").unwrap();
    assert_eq!(
        format_eval_result(&eval.eval_str("window-system")),
        "OK nil"
    );
    let graphical = eval
        .eval_str("(x-create-frame '((visibility . nil)))")
        .unwrap();
    eval.set_variable("issue417-graphical", graphical);
    assert_eq!(
        format_eval_result(
            &eval.eval_str("(progn (select-frame issue417-graphical) window-system)")
        ),
        "OK neo",
    );

    // A binding from the cached TTY lifetime must not restore through the
    // same terminal number after the graphical connection replaced it.
    eval.unbind_to(depth);
    assert_eq!(
        format_eval_result(&eval.eval_str("(list window-system (window-system))")),
        "OK (neo neo)",
    );
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
        .frame_manager()
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
        .frame_manager()
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
/// A reopened GUI owns an opened default font independently of terminal faces
/// and of the renderer's one-shot primary-window adoption.
fn reopened_gui_default_face_starts_from_its_opened_font_after_primary_adoption() {
    let mut eval = terminal_startup_context();
    eval.eval_str(
        "(setq issue688-terminal (selected-frame))
         (internal-set-lisp-face-attribute 'default :family \"default\" issue688-terminal)
         (internal-set-lisp-face-attribute 'default :height 1 issue688-terminal)",
    )
    .unwrap();
    let host = initial_host();
    let realized = host.realized.clone();
    eval.set_display_host(Box::new(host));
    eval.eval_str(
        "(setq issue688-first (x-create-frame nil))
         (delete-frame issue688-first t)
         (select-frame issue688-terminal)
         (setq issue688-reopened (x-create-frame nil))",
    )
    .unwrap();
    assert_eq!(realized.borrow().len(), 2);
    let frame_id = FrameId(
        eval.eval_str("issue688-reopened")
            .unwrap()
            .as_frame_id()
            .unwrap(),
    );
    let frame = eval.frame_manager().get(frame_id).unwrap();
    assert_eq!((frame.char_width, frame.char_height), (9.0, 18.0));

    // GNU initializes each new graphical frame's default face from its
    // opened font (xfns.c:init_frame_faces, xfaces.c:realize_default_face).
    // The retained daemon terminal's synthetic default/1 face is unrelated
    // to the GUI font, including after the first native window was adopted.
    let family = eval
        .eval_str("(internal-get-lisp-face-attribute 'default :family issue688-reopened)")
        .unwrap();
    let height = eval
        .eval_str("(internal-get-lisp-face-attribute 'default :height issue688-reopened)")
        .unwrap();
    assert_eq!(family.as_utf8_str(), Some("Initial Mono"));
    assert_eq!(height, Value::fixnum(130));
    assert_eq!(
        eval.eval_str(
            "(fontp (internal-get-lisp-face-attribute 'default :font issue688-reopened))",
        )
        .unwrap(),
        Value::T,
        "a reopened GUI default face must retain its opened font object",
    );
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

#[test]
fn initial_default_face_keeps_opened_font_after_user_font_parameter_is_cleared() {
    // GNU xfaces.c:set_lface_from_font stores the opened object in LFACE_FONT.
    // faces.el:x-create-frame-with-faces subsequently clears font-parameter
    // when no user font was requested; that request marker is not font storage.
    let mut eval = terminal_startup_context();
    eval.set_display_host(Box::new(initial_host()));
    eval.eval_str(
        "(setq issue417-frame (x-create-frame '((visibility . nil))))
         (select-frame issue417-frame)
         (setq issue417-opened-font
               (internal-get-lisp-face-attribute 'default :font issue417-frame))
         (setq issue417-opened-font-info (font-info issue417-opened-font))",
    )
    .unwrap();
    assert_eq!(
        eval.eval_str("(fontp issue417-opened-font)").unwrap(),
        Value::T,
        "the raw initial frame must already own an opened font",
    );
    eval.eval_str("(modify-frame-parameters issue417-frame '((font-parameter . nil)))")
        .unwrap();
    assert_eq!(
        eval.eval_str("(fontp (internal-get-lisp-face-attribute 'default :font issue417-frame))")
            .unwrap(),
        Value::T,
        "clearing the user request must preserve the frame-local opened font",
    );
    assert_eq!(
        eval.eval_str(
            "(let ((font (internal-get-lisp-face-attribute 'default :font issue417-frame)))
               (and (eq font issue417-opened-font)
                    (equal (font-info font) issue417-opened-font-info)))"
        )
        .unwrap(),
        Value::T,
        "the opened font and its public metrics must survive Lisp finalization",
    );
}
