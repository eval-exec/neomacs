//! Re-homed window-command host tests (P1 B1): bodies are the display originals;
//! only the constructor line changed.
use super::*;
use neovm_value::display_vocab::WindowSystemKind;

#[test]
fn modify_frame_parameters_after_live_font_change_defers_gui_resize_until_geometry_query() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let host = RecordingDisplayHost::with_resolved_frame_font(resolved_frame_font(
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
    ));
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 800, 600, buf);
    {
        let frame = ev.frame_manager_mut().get_mut(fid).expect("frame");
        frame.set_window_system(Some(Value::symbol("neo")));
        frame.char_width = 8.0;
        frame.char_height = 16.0;
    }

    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(
        &mut ev,
        vec![
            Value::symbol("default"),
            Value::keyword("font"),
            Value::string("Noto Sans Mono-16"),
            Value::make_frame(fid.0),
        ],
    )
    .expect("set live default face font");

    // GNU's new-font hook now requests an implied resize immediately. The
    // explicit width/height override below must still remain deferred until
    // the geometry query, and the allocation still awaits a native reply.
    assert_eq!(resized.borrow().len(), 1);
    resized.borrow_mut().clear();

    let out = ev
        .eval_str_each("(modify-frame-parameters (selected-frame) '((width . 80) (height . 25)))");
    assert!(
        out[0].is_ok(),
        "modify-frame-parameters failed: {:?}",
        out[0]
    );

    assert!(
        resized.borrow().is_empty(),
        "font-followup resize should stay deferred until geometry query"
    );

    let expected_width = {
        let frame = ev.frame_manager().get(fid).expect("frame should exist");
        assert_eq!(frame.width, 800);
        assert_eq!(frame.height, 600);
        assert_eq!(frame.parameter("width"), Some(Value::fixnum(80)));
        assert_eq!(frame.parameter("height"), Some(Value::fixnum(25)));
        assert_eq!(frame.char_width, 13.0);
        assert_eq!(frame.char_height, 31.0);
        80 * 13 + frame.horizontal_non_text_width()
    };

    let tx = crate::host_input::HostInputPublisher::install(&mut ev);

    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
        width: expected_width as u32,
        height: 775,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str("(list (frame-native-width) (frame-native-height))")
        .expect("geometry query should flush deferred resize");
    let values = neovm_value::value::list_to_vec(&result).expect("result list");
    assert_eq!(
        values,
        vec![Value::fixnum(expected_width), Value::fixnum(775)]
    );

    let resize_requests = resized.borrow();
    assert_eq!(resize_requests.len(), 1);
    assert_eq!(resize_requests[0].frame_id, fid);
    assert_eq!(resize_requests[0].width, expected_width as u32);
    assert_eq!(resize_requests[0].height, 775);
    assert_eq!(
        resize_requests[0].geometry_hints,
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn set_frame_size_keeps_resize_pending_until_geometry_queries_force_sync() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 1300, 1188, buf);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("frame should exist");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
        frame.char_width = 16.0;
        frame.char_height = 33.0;
        frame.set_parameter(Value::symbol("width"), Value::fixnum(79));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(36));
        frame.set_parameter(Value::symbol("neovm--frame-text-lines"), Value::fixnum(35));
    }

    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));

    let tx = crate::host_input::HostInputPublisher::install(&mut ev);

    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
        width: 2144,
        height: 1386,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str(
            "(progn
               (set-frame-size (selected-frame) 132 42)
               (list (frame-parameter nil 'width) (frame-parameter nil 'height)))",
        )
        .expect("set-frame-size without geometry query should succeed");
    let values = neovm_value::value::list_to_vec(&result).expect("result list");
    assert_eq!(values, vec![Value::fixnum(79), Value::fixnum(36)]);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, fid);
    assert_eq!(requests[0].width, 2144);
    assert_eq!(requests[0].height, 1386);
}

#[test]
fn set_frame_size_syncs_resize_event_before_followup_frame_width_queries() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let buf = ev.edit.buffers.create_buffer("*scratch*");
    ev.edit.buffers.set_current(buf);
    let fid = ev.frame_manager_mut().create_frame("F1", 1300, 1188, buf);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("frame should exist");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
        frame.char_width = 16.0;
        frame.char_height = 33.0;
        frame.set_parameter(Value::symbol("width"), Value::fixnum(79));
        frame.set_parameter(Value::symbol("height"), Value::fixnum(36));
        frame.set_parameter(Value::symbol("neovm--frame-text-lines"), Value::fixnum(35));
    }

    let host = RecordingDisplayHost::new();
    let resized = host.resized.clone();
    ev.set_display_host(Box::new(host));

    let tx = crate::host_input::HostInputPublisher::install(&mut ev);

    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
        width: 2144,
        height: 1386,
        scale_factor: 1.0,
        emacs_frame_id: fid.0,
    })
    .expect("queue resize ack");

    let result = ev
        .eval_str(
            "(progn
               (set-frame-size (selected-frame) 132 42)
               (list (frame-total-cols) (frame-native-width)))",
        )
        .expect("set-frame-size and followup queries should succeed");
    let values = neovm_value::value::list_to_vec(&result).expect("result list");
    assert_eq!(values, vec![Value::fixnum(132), Value::fixnum(2144)]);

    let requests = resized.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, fid);
    assert_eq!(requests[0].width, 2144);
    assert_eq!(requests[0].height, 1386);
    assert_eq!(
        requests[0].geometry_hints,
        ev.frame_manager()
            .get(fid)
            .expect("frame should exist")
            .gui_geometry_hints()
    );
}

#[test]
fn timer_deferred_make_frame_invisible_removes_gui_child_overlay_from_display_host() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let parent_id = ev
        .frame_manager_mut()
        .create_frame("parent", 960, 640, scratch);
    {
        let parent = ev
            .frame_manager_mut()
            .get_mut(parent_id)
            .expect("parent frame");
        parent.set_window_system(Some(Value::symbol("neo")));
        parent.char_width = 10.0;
        parent.char_height = 20.0;
        parent.font_pixel_size = 20.0;
    }
    ev.frame_manager_mut().select_frame(parent_id);
    let host = RecordingDisplayHost::new();
    let removed_child_frames = host.removed_child_frames.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(
            Value::symbol("parent-frame"),
            Value::make_frame(parent_id.0),
        ),
        Value::cons(Value::symbol("width"), Value::fixnum(20)),
        Value::cons(Value::symbol("height"), Value::fixnum(5)),
        Value::cons(Value::symbol("minibuffer"), Value::NIL),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");
    let child_id = crate::window::FrameId(created.as_frame_id().expect("child frame id"));

    ev.eval_str(
        r#"(progn
             (fset 'neomacs-test-hide-frame
                   (lambda (frame) (make-frame-invisible frame)))
             (fset 'timer-event-handler
                   (lambda (timer)
                     (setq timer-list (delq timer timer-list))
                     (apply (aref timer 5) (aref timer 6)))))"#,
    )
    .expect("install timer handler");
    ev.set_variable(
        "timer-list",
        Value::list(vec![due_gnu_timer(
            Value::symbol("neomacs-test-hide-frame"),
            Value::list(vec![created]),
        )]),
    );

    ev.fire_pending_timers();

    let child = ev
        .frame_manager()
        .get(child_id)
        .expect("child frame remains live");
    assert_eq!(child.visibility, crate::window::FrameVisibility::Invisible);
    assert_eq!(*removed_child_frames.borrow(), vec![child_id]);
}

#[test]
fn x_create_frame_syncs_pending_resize_before_adopting_opening_gui_frame() {
    crate::test_utils::init_test_tracing();
    let mut ev = crate::test_utils::new_context();
    let scratch = ev.edit.buffers.create_buffer("*scratch*");
    let fid = ev
        .frame_manager_mut()
        .create_frame("bootstrap", 960, 640, scratch);
    {
        let frame = ev
            .frame_manager_mut()
            .get_mut(fid)
            .expect("bootstrap frame");
        frame.install_backend_less_gui_kind(WindowSystemKind::X);
        frame.char_width = 10.0;
        frame.char_height = 20.0;
        if let Some(mini_leaf) = frame.minibuffer_leaf.as_mut() {
            mini_leaf.set_bounds(crate::window::Rect::new(0.0, 600.0, 960.0, 40.0));
        }
    }
    ev.set_variable("terminal-frame", Value::make_frame(fid.0));

    let tx = crate::host_input::HostInputPublisher::install(&mut ev);

    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Focus {
        focused: true,
        emacs_frame_id: 0,
    })
    .expect("queue focus");
    tx.send(neovm_eval::emacs_core::eval::keyboard::InputEvent::Resize {
        width: 1500,
        height: 1900,
        scale_factor: 1.0,
        emacs_frame_id: 0,
    })
    .expect("queue resize");

    let host = RecordingDisplayHost::new();
    let requests = host.realized.clone();
    ev.set_display_host(Box::new(host));

    let params = Value::list(vec![
        Value::cons(Value::symbol("name"), Value::string("Neomacs")),
        Value::cons(Value::symbol("title"), Value::string("Neomacs")),
    ]);
    let created = super::builtin_x_create_frame(&mut ev, vec![params]).expect("x-create-frame");

    let created_id = crate::window::FrameId(
        created
            .as_frame_id()
            .unwrap_or_else(|| panic!("expected frame object, got {:?}", created)),
    );
    let frame = ev
        .frame_manager()
        .get(created_id)
        .expect("created opening frame");
    assert_eq!(frame.width, 1500);
    assert_eq!(frame.height, 1900);

    let requests = requests.borrow();
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].frame_id, created_id);
    assert_eq!(requests[0].width, 1500);
    assert_eq!(requests[0].height, 1900);
}
