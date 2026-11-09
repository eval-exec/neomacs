//! Re-homed display popup/query host tests (P1 B1). Test bodies are the
//! display originals; only the constructor line and crate-qualified helper
//! paths changed.
use super::*;

#[test]
fn display_queries_default_to_selected_frame_window_system_surface() {
    crate::test_utils::init_test_tracing();
    let mut eval = crate::test_utils::new_context();
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    let frame = Value::make_frame(frame_id.0);

    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    eval.set_variable("initial-window-system", Value::NIL);
    eval.set_variable("window-system", Value::NIL);

    assert_eq!(
        builtin_display_graphic_p(&mut eval, vec![]).unwrap(),
        Value::T
    );
    // `display-color-cells' is lisp/frame.el:2966, not a subr (DIVERGENCES.md
    // 157).  Its body dispatches on `framep-on-display' and, for a `neo'
    // frame, calls the C `x-display-color-cells' (src/xfns.c:5714) -- which is
    // what this bare evaluator can ask.
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![frame]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        eval.eval_str("(xw-display-color-p)").unwrap(),
        Value::T,
        "public Lisp entry: xw-display-color-p with no frame"
    );
    assert_eq!(
        eval.eval_str("(xw-display-color-p (selected-frame))")
            .unwrap(),
        Value::T,
        "public Lisp entry: xw-display-color-p with the live selected frame"
    );
    assert_eq!(
        builtin_display_planes(&mut eval, vec![]).unwrap(),
        Value::fixnum(24)
    );
    assert_eq!(
        builtin_display_visual_class(&mut eval, vec![]).unwrap(),
        Value::symbol("true-color")
    );
    assert_eq!(
        builtin_x_display_color_cells(&mut eval, vec![Value::NIL]).unwrap(),
        Value::fixnum(16_777_216)
    );
    assert_eq!(
        builtin_x_display_visual_class(&mut eval, vec![frame]).unwrap(),
        Value::symbol("true-color")
    );
}

#[test]
fn monitor_attributes_geometry_uses_frame_pixel_units() {
    crate::test_utils::init_test_tracing();
    use neovm_eval::emacs_core::builtins::{NeomacsMonitorInfo, set_neomacs_monitor_info};
    let mut eval = crate::test_utils::new_context();
    crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    set_neomacs_monitor_info(vec![NeomacsMonitorInfo {
        x: 3024,
        y: 0,
        width: 3024,
        height: 1964,
        scale: 2.0,
        width_mm: 0,
        height_mm: 0,
        name: Some("Retina".into()),
    }]);
    let list = eval
        .eval_str("(neomacs-display-monitor-attributes-list)")
        .expect("public Lisp entry for the monitor attribute list");
    set_neomacs_monitor_info(vec![]);
    let expected = if cfg!(target_os = "macos") {
        "(geometry 1512 0 1512 982)"
    } else {
        "(geometry 3024 0 3024 1964)"
    };
    let printed = neovm_eval::emacs_core::print::print_value(&list);
    assert!(printed.contains(expected), "{printed}");
}

#[test]
fn monitor_attributes_report_scale_factor_only_for_wayland_frames() {
    crate::test_utils::init_test_tracing();
    use neovm_eval::emacs_core::builtins::{NeomacsMonitorInfo, set_neomacs_monitor_info};
    let mut eval = crate::test_utils::new_context();
    let selected = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    set_neomacs_monitor_info(vec![NeomacsMonitorInfo {
        x: 0,
        y: 0,
        width: 3024,
        height: 1964,
        scale: 2.0,
        width_mm: 0,
        height_mm: 0,
        name: None,
    }]);
    // The harness is an integration test, so reach the subr through its public
    // Lisp entry (the direct builtin is crate-private after the split).
    let printed = |eval: &mut neovm_eval::emacs_core::eval::Context, args: &str| {
        let src = format!("(neomacs-display-monitor-attributes-list{args})");
        let list = eval.eval_str(&src).expect("monitor attributes");
        neovm_eval::emacs_core::print::print_value(&list)
    };
    // GNU's NS and X builds omit it; only PGTK reports it.
    let without = printed(&mut eval, "");
    // The selected frame stays non-graphical (an emacsclient -t frame); a
    // Wayland frame elsewhere still makes the frontend's monitors PGTK-shaped.
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let wayland = eval
        .frame_manager_mut()
        .create_frame("wayland", 800, 600, scratch);
    eval.frame_manager_mut()
        .get_mut(wayland)
        .expect("Wayland frame")
        .set_display_identity(crate::window::FrameDisplayIdentity::wayland("wayland-0"));
    eval.frame_manager_mut().select_frame(selected);
    let with = printed(&mut eval, "");
    let with_frame_arg = printed(&mut eval, " (selected-frame)");
    set_neomacs_monitor_info(vec![]);
    assert!(!without.contains("scale-factor"), "{without}");
    for printed in [&with, &with_frame_arg] {
        // GNU's order: mm-size, scale-factor, frames.
        let scale = printed.find("(scale-factor . 2.0)").expect(printed);
        assert!(printed.find("(mm-size").unwrap() < scale, "{printed}");
        assert!(scale < printed.find("(frames").unwrap(), "{printed}");
    }
}

#[test]
fn x_popup_dialog_interactive_returns_selected_value() {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("dialog-owner", 800, 600, scratch);
    eval.frame_manager_mut().select_frame(frame_id);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    let hidden = Arc::clone(&host.hidden);
    eval.set_display_host(Box::new(host));

    let contents = Value::list(vec![
        Value::string("Confirm?"),
        Value::cons(Value::string("Yes"), Value::T),
        Value::cons(Value::string("No"), Value::symbol("declined")),
    ]);
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: 1,
            token: None,
        },
    )
    .unwrap();

    let result =
        super::builtin_x_popup_dialog(&mut eval, vec![Value::T, contents, Value::NIL]).unwrap();

    assert_eq!(result, Value::symbol("declined"));
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].frame_id, frame_id);
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::at(neomacs_display_protocol::Point::new(
            400.0, 300.0
        ),)
    );
    assert_eq!(shown[0].title.as_deref(), Some("Confirm?"));
    assert_eq!(shown[0].entries.len(), 2);
    assert_eq!(shown[0].entries[0].label, "Yes");
    assert_eq!(shown[0].entries[1].label, "No");
    assert_eq!(*hidden.lock().unwrap(), 1);
}

#[test]
fn x_popup_menu_interactive_cancel_returns_nil() {
    let mut eval = crate::test_utils::new_context();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();

    assert!(
        super::builtin_x_popup_menu(
            &mut eval,
            vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
        )
        .unwrap()
        .is_nil()
    );
}

#[test]
fn x_popup_menu_interactive_ignores_tty_mouse_navigation() {
    let mut eval = crate::test_utils::new_context();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let tty_menu_navigation_map =
        neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        tty_menu_navigation_map,
        Value::symbol("mouse-movement"),
        Value::symbol("tty-menu-select"),
        neovm_value::tagged::SymbolMode::Bare,
    );
    eval.set_variable("tty-menu-navigation-map", tty_menu_navigation_map);
    eval.set_variable("track-mouse", Value::T);

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("first"),
        Value::cons(Value::string("First"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MouseMove {
            x: 8.0,
            y: 18.0,
            modifiers: neovm_eval::emacs_core::eval::keyboard::Modifiers::none(),
            target_frame_id: 0,
        },
    )
    .unwrap();
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    assert!(
        result.is_nil(),
        "native popup hover must not run tty-menu-select"
    );
}

#[test]
fn x_popup_menu_interactive_keyboard_select_roots_selected_event() {
    let mut eval = crate::test_utils::new_context();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("second"),
        Value::cons(Value::string("Second"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("first"),
        Value::cons(Value::string("First"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::KeyPress {
            key: neovm_eval::emacs_core::eval::keyboard::KeyEvent::named(
                neovm_eval::emacs_core::eval::keyboard::NamedKey::Down,
            ),
            emacs_frame_id: 0,
        },
    )
    .unwrap();
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::KeyPress {
            key: neovm_eval::emacs_core::eval::keyboard::KeyEvent::named(
                neovm_eval::emacs_core::eval::keyboard::NamedKey::Return,
            ),
            emacs_frame_id: 0,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = neovm_value::value::list_to_vec(&result).expect("event list");
    assert_eq!(events, vec![Value::symbol("second")]);
}

#[test]
fn x_popup_menu_interactive_keymap_collapses_submenu_on_tty() {
    // A frame with no window system is a TTY frame (effective_window_system =
    // None). GNU `single_menu_item` (src/menu.c:407-433) renders a submenu on a
    // TTY frame as ONE collapsed line whose label gains the `" >"` suffix; the
    // child-recursion (`single_keymap_panes`) is inside the
    // `#if USE_X_TOOLKIT || USE_GTK || ...` block and is compiled out for TTY,
    // so the children are NOT inlined into the parent pane (they are opened on
    // demand by `tty_menu_activate`). This test previously asserted the GUI
    // toolkit behavior (children flattened into the same pane), which is wrong
    // for a TTY frame and ballooned panes like Help -> Describe past the
    // screen height.
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("popup-owner", 800, 600, scratch);
    eval.frame_manager_mut().select_frame(frame_id);
    assert!(
        eval.frame_manager()
            .get(frame_id)
            .and_then(|f| f.effective_window_system())
            .is_none(),
        "test fixture frame should be a TTY frame (no window system)"
    );
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));

    let submenu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        submenu,
        Value::symbol("visual-line-mode"),
        Value::cons(Value::string("Visual Line Mode"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("line-wrapping"),
        Value::cons(Value::string("Line Wrapping in this Buffer"), submenu),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    // Only the collapsed submenu header is pushed — its children are NOT
    // inlined — and the label gains the GNU `" >"` suffix.
    assert_eq!(shown[0].entries.len(), 1);
    assert_eq!(shown[0].entries[0].label, "Line Wrapping in this Buffer >");
    assert!(shown[0].entries[0].submenu());
    assert_eq!(shown[0].entries[0].depth, 0);
}

#[test]
fn x_popup_menu_interactive_keymap_returns_selected_event() {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("popup-owner", 800, 600, scratch);
    eval.frame_manager_mut().select_frame(frame_id);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    let hidden = Arc::clone(&host.hidden);
    eval.set_display_host(Box::new(host));

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::KeyPress {
            key: neovm_eval::emacs_core::eval::keyboard::KeyEvent::named(
                neovm_eval::emacs_core::eval::keyboard::NamedKey::Return,
            ),
            emacs_frame_id: 0,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = neovm_value::value::list_to_vec(&result).expect("event list");
    assert_eq!(events, vec![Value::symbol("open")]);
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(shown[0].frame_id, frame_id);
    assert_eq!(shown[0].entries.len(), 1);
    assert_eq!(shown[0].entries[0].label, "Open");
    assert_eq!(*hidden.lock().unwrap(), 1);
}

#[test]
fn x_popup_menu_interactive_menu_bar_position_anchors_below_menu_bar() {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("popup-owner", 800, 600, scratch);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .menu_bar_height = 24;
    eval.frame_manager_mut().select_frame(frame_id);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();

    let position = Value::list(vec![
        Value::symbol("file"),
        Value::list(vec![
            Value::make_frame(frame_id.0),
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(5), Value::fixnum(0)]),
            Value::fixnum(0),
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::NIL,
            Value::list(vec![Value::fixnum(96), Value::fixnum(4)]),
            Value::cons(Value::fixnum(64), Value::fixnum(24)),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, menu]).unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::new(
            neomacs_display_protocol::Rect::new(96.0, 0.0, 64.0, 24.0),
            neomacs_display_protocol::PopupPreferredSide::Below,
            neomacs_display_protocol::Point::ZERO,
            neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
        )
    );
}

#[test]
fn x_popup_menu_interactive_menu_bar_position_uses_pending_native_anchor() {
    let mut eval = crate::test_utils::new_context();
    let scratch = eval.edit.buffers.create_buffer("*scratch*");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("popup-owner", 800, 600, scratch);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .menu_bar_height = 24;
    eval.frame_manager_mut().select_frame(frame_id);
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    let host = RecordingPopupHost::default();
    let shown = Arc::clone(&host.shown);
    eval.set_display_host(Box::new(host));
    eval.pending_menu_bar_popup_anchor = Some(neovm_eval::emacs_core::MenuBarPopupAnchor {
        request_id: Some(neomacs_display_protocol::menu::MenuBarRequestId(42)),
        frame_id,
        menu_key: Some("tools".to_string()),
        menu_x: 26,
        x: 244,
        y: 0,
        width: 55,
        height: 18,
    });

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();

    let position = Value::list(vec![
        Value::symbol("tools"),
        Value::list(vec![
            Value::make_frame(frame_id.0),
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(16), Value::fixnum(0)]),
            Value::fixnum(0),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, menu]).unwrap();

    assert!(result.is_nil());
    let shown = shown.lock().unwrap();
    assert_eq!(shown.len(), 1);
    assert_eq!(
        shown[0].request_id,
        Some(neomacs_display_protocol::menu::MenuBarRequestId(42))
    );
    assert_eq!(
        shown[0].placement,
        neomacs_display_protocol::PopupPlacement::new(
            neomacs_display_protocol::Rect::new(244.0, 0.0, 55.0, 18.0),
            neomacs_display_protocol::PopupPreferredSide::Below,
            neomacs_display_protocol::Point::ZERO,
            neomacs_display_protocol::PopupConstraintPolicy::FlipAndShift { padding: 4.0 },
        )
    );
}

#[test]
fn x_popup_menu_interactive_menu_bar_right_returns_next_menu_position() {
    let mut eval = crate::test_utils::new_context();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let menu_bar = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    let file_menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    let help_menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    let global_map = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu_bar,
        Value::symbol("help-menu"),
        Value::cons(Value::string("Help"), help_menu),
        neovm_value::tagged::SymbolMode::Bare,
    );
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu_bar,
        Value::symbol("file"),
        Value::cons(Value::string("File"), file_menu),
        neovm_value::tagged::SymbolMode::Bare,
    );
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        global_map,
        Value::symbol("menu-bar"),
        menu_bar,
        neovm_value::tagged::SymbolMode::Bare,
    );
    eval.set_variable("global-map", global_map);
    eval.select_global_map(global_map);
    eval.set_variable(
        "menu-bar-final-items",
        Value::list(vec![Value::symbol("help-menu")]),
    );

    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        file_menu,
        Value::symbol("open"),
        Value::cons(Value::string("Open"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::KeyPress {
            key: neovm_eval::emacs_core::eval::keyboard::KeyEvent::named(
                neovm_eval::emacs_core::eval::keyboard::NamedKey::Right,
            ),
            emacs_frame_id: 0,
        },
    )
    .unwrap();

    let position = Value::list(vec![
        Value::symbol("file"),
        Value::list(vec![
            Value::NIL,
            Value::list(vec![Value::symbol("menu-bar")]),
            Value::list(vec![Value::fixnum(0), Value::fixnum(0)]),
            Value::fixnum(0),
        ]),
    ]);
    let result = super::builtin_x_popup_menu(&mut eval, vec![position, file_menu]).unwrap();

    assert!(
        result.is_cons(),
        "right from File should return menu-bar coordinates"
    );
    assert_eq!(result.cons_car(), Value::fixnum(5));
    assert_eq!(result.cons_cdr(), Value::fixnum(0));
}

#[test]
fn x_popup_menu_interactive_submenu_selection_returns_full_event_path() {
    let mut eval = crate::test_utils::new_context();
    // On a window-system (GUI) frame the toolkit renders nested submenu panes,
    // so GNU recurses into the submenu (`single_keymap_panes`, the
    // `#if USE_X_TOOLKIT || ...` branch in src/menu.c:422-433) and the child
    // entry is reachable. Selecting it returns the full nested event path.
    let frame_id = crate::emacs_core::window_cmds::ensure_selected_frame_id(&mut eval);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("selected frame")
        .set_window_system(Some(Value::symbol(gui_window_system_symbol())));
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));

    let submenu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        submenu,
        Value::symbol("visual-line-mode"),
        Value::cons(Value::string("Visual Line Mode"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("line-wrapping"),
        Value::cons(Value::string("Line Wrapping in this Buffer"), submenu),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: 1,
            token: None,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .unwrap();

    let events = neovm_value::value::list_to_vec(&result).expect("event list");
    assert_eq!(
        events,
        vec![
            Value::symbol("line-wrapping"),
            Value::symbol("visual-line-mode")
        ]
    );
}

#[test]
fn x_popup_menu_preserves_heading_click_queued_before_old_cancellation() {
    let mut eval = crate::test_utils::new_context();
    let buffer = eval.edit.buffers.create_buffer("*menu-switch*");
    let frame = eval
        .frame_manager_mut()
        .create_frame("menu-owner", 800, 600, buffer);
    eval.frame_manager_mut().select_frame(frame);
    eval.eval_str("(progn (setq global-map (make-sparse-keymap)) (use-global-map global-map) (define-key global-map [menu-bar mouse-1] 'ignore))").unwrap();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));
    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("entry"),
        Value::cons(Value::string("Entry"), Value::T),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuBarClick {
            request_id: Some(neomacs_display_protocol::menu::MenuBarRequestId(42)),
            index: 5,
            key: "lisp-interaction".into(),
            menu_x: 304.0,
            menu_y: 0.0,
            anchor_x: 304.0,
            anchor_y: 0.0,
            anchor_width: 144.0,
            anchor_height: 18.0,
            emacs_frame_id: frame.0,
        },
    )
    .unwrap();
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: -1,
            token: None,
        },
    )
    .unwrap();
    assert!(
        super::builtin_x_popup_menu(&mut eval, vec![Value::T, menu])
            .unwrap()
            .is_nil()
    );
    let unread = eval.eval_str("unread-command-events").unwrap();
    assert!(
        !unread.is_nil(),
        "old menu swallowed the newer heading request"
    );
    let (keys, _) = eval.read_key_sequence().unwrap();
    assert_eq!(keys[0], Value::symbol("menu-bar"));
    assert_eq!(
        eval.pending_menu_bar_popup_anchor
            .as_ref()
            .unwrap()
            .request_id,
        Some(neomacs_display_protocol::menu::MenuBarRequestId(42))
    );
}

/// GNU's native TTY menu path carries a menu item's `:help` property from
/// `parse_menu_item` through `MENU_ITEMS_ITEM_HELP` and publishes it with
/// `tty_menu_help_callback`/`show_help_echo` (`keyboard.c`, `menu.c`, and
/// `term.c`).  Destroying the popup clears only the terminal menu's internal
/// help pointer; it does not retract the help that was published.
#[test]
fn x_popup_menu_publishes_selected_item_help_and_keeps_it_after_teardown() {
    let mut eval = crate::test_utils::new_context();
    let tx = crate::host_input::HostInputPublisher::install(&mut eval);

    eval.set_display_host(Box::new(RecordingPopupHost::default()));
    eval.eval_str(
        r##"(progn
               (defvar neomacs-test-menu-help nil)
               (defvar neomacs-test-menu-help-inhibit-redisplay nil)
               (defvar neomacs-test-menu-previous-message nil)
               (setq show-help-function
                     (lambda (help)
                       (setq neomacs-test-menu-help help
                             neomacs-test-menu-previous-message
                             (current-message)
                             neomacs-test-menu-help-inhibit-redisplay
                             inhibit-redisplay))))"##,
    )
    .expect("install a recording show-help-function");
    eval.set_current_message(Some(neovm_value::heap_types::LispString::from_utf8(
        "Previous menu help",
    )));

    let menu = neovm_eval::emacs_core::eval::keymap_lookup::make_sparse_list_keymap();
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("open"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Open"),
            Value::T,
            Value::symbol(":help"),
            Value::string("Open the selected document"),
        ]),
        neovm_value::tagged::SymbolMode::Bare,
    );
    neovm_eval::emacs_core::eval::keymap_lookup::list_keymap_define(
        menu,
        Value::symbol("plain"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Plain"),
            Value::T,
        ]),
        neovm_value::tagged::SymbolMode::Bare,
    );
    tx.send(
        neovm_eval::emacs_core::eval::keyboard::InputEvent::MenuSelection {
            index: 1,
            token: None,
        },
    )
    .unwrap();

    let result = super::builtin_x_popup_menu(
        &mut eval,
        vec![Value::list(vec![Value::NIL, Value::NIL]), menu],
    )
    .expect("select helped popup item");

    let events = neovm_value::value::list_to_vec(&result).expect("event list");
    assert_eq!(events, vec![Value::symbol("open")]);
    assert_eq!(
        eval.eval_symbol("neomacs-test-menu-help")
            .expect("read published menu help")
            .as_utf8_str(),
        Some("Open the selected document")
    );
    assert_eq!(
        eval.eval_symbol("neomacs-test-menu-help-inhibit-redisplay")
            .expect("read native menu redisplay guard"),
        Value::T,
        "GNU tty_menu_activate inhibits ordinary redisplay while menu help runs"
    );
    assert_eq!(
        eval.eval_symbol("neomacs-test-menu-previous-message")
            .expect("read previous message observed by menu help"),
        Value::NIL,
        "GNU read_menu_command still clears the prior logical echo message"
    );
}
