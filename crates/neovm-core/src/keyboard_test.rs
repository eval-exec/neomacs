use super::*;

#[test]
fn presented_interactions_retain_exact_posn_string_until_retirement() {
    let mut interactions = PresentedInteractions::default();
    let presentation = interactions.begin();
    let caption = Value::string("tab");
    let posn_string = Value::cons(caption, Value::fixnum(0));
    let interaction = interactions.register_mouse_target(
        presentation,
        PresentedMouseTarget {
            area: PresentedMouseArea::TabBar,
            posn_string,
        },
    );

    assert_eq!(
        interactions.resolve(presentation, interaction),
        Some(PresentedMouseTarget {
            area: PresentedMouseArea::TabBar,
            posn_string,
        })
    );

    let mut roots = Vec::new();
    crate::gc_trace::GcTrace::trace_roots(&interactions, &mut roots);
    assert!(roots.contains(&posn_string));

    interactions.retire(presentation);
    assert_eq!(interactions.resolve(presentation, interaction), None);
}

#[test]
fn presented_pointer_preserves_phase_and_installs_snapshot_posn_string() {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("presented-pointer");
    let frame = eval
        .frame_manager_mut()
        .create_frame("presented-pointer", 200, 100, buffer);
    let presentation = eval.begin_interaction_presentation();
    let posn_string = Value::cons(Value::string("tab"), Value::fixnum(0));
    let interaction = eval.register_presented_mouse_target(
        presentation,
        PresentedMouseTarget {
            area: PresentedMouseArea::TabBar,
            posn_string,
        },
    );

    let press = eval
        .handle_read_char_input_event(InputEvent::PresentedPointer {
            presentation,
            interaction,
            pressed: true,
            button: 1,
            x: 24.0,
            y: 8.0,
            emacs_frame_id: frame.0,
        })
        .expect("press conversion")
        .expect("press event");
    let release = eval
        .handle_read_char_input_event(InputEvent::PresentedPointer {
            presentation,
            interaction,
            pressed: false,
            button: 1,
            x: 24.0,
            y: 8.0,
            emacs_frame_id: frame.0,
        })
        .expect("release conversion")
        .expect("release event");

    let press_parts = crate::emacs_core::value::list_to_vec(&press).expect("press list");
    let release_parts = crate::emacs_core::value::list_to_vec(&release).expect("release list");
    assert_eq!(press_parts[0].as_symbol_name(), Some("down-mouse-1"));
    assert_eq!(release_parts[0].as_symbol_name(), Some("mouse-1"));
    for event in [press, release] {
        let parts = crate::emacs_core::value::list_to_vec(&event).expect("mouse event");
        let position = crate::emacs_core::value::list_to_vec(&parts[1]).expect("position");
        assert_eq!(position[1].as_symbol_name(), Some("tab-bar"));
        assert_eq!(position[4], posn_string);
    }
}

#[test]
fn presented_tab_line_hit_joins_renderer_string_index_with_rooted_lisp_value() {
    use crate::window::{
        PresentedWindowChromeArea, PresentedWindowChromeString, PresentedWindowRegions,
        WindowDisplaySnapshot,
    };
    use neomacs_display_protocol::{
        DisplayWindowId, FrameRect, GlyphStringId, PresentationId, PresentedHitIndex,
        PresentedHitQuery, PresentedHitRegion, PresentedRegionKind, PresentedStringPosition, Rect,
    };

    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval
        .buffer_manager_mut()
        .create_buffer("presented-tab-line-hit");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("presented-tab-line-hit", 200, 100, buffer);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let presentation = crate::window::geometry::PresentationId::new(7);
    let protocol_presentation = PresentationId::new(7);
    let string_id = GlyphStringId::new(1);
    let text = Value::string("first second");
    let tab_bounds = FrameRect::new(0.0, 0.0, 200.0, 16.0).unwrap();
    let char_bounds = FrameRect::new(40.0, 0.0, 8.0, 16.0).unwrap();
    let hit_index = PresentedHitIndex::from_parts_with_strings(
        protocol_presentation,
        vec![PresentedHitRegion::new(
            Some(DisplayWindowId::new(window_id.0 as i64)),
            PresentedRegionKind::TabLine,
            tab_bounds,
            20,
        )],
        Vec::new(),
        vec![PresentedStringPosition::new(
            DisplayWindowId::new(window_id.0 as i64),
            PresentedWindowChromeArea::TabLine,
            char_bounds,
            string_id,
            6,
        )],
    )
    .unwrap();
    let hit = hit_index
        .resolve(PresentedHitQuery::new(protocol_presentation, 44.0, 8.0))
        .unwrap()
        .unwrap();
    let regions = PresentedWindowRegions {
        outer: Rect::new(0.0, 0.0, 200.0, 100.0),
        text_body: Rect::new(0.0, 16.0, 200.0, 84.0),
        tab_line: Some(Rect::new(0.0, 0.0, 200.0, 16.0)),
        ..Default::default()
    };
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                presentation,
                vec![WindowDisplaySnapshot {
                    window_id,
                    regions,
                    regions_materialized: true,
                    tab_line_height: 16,
                    chrome_strings: vec![PresentedWindowChromeString::new(
                        PresentedWindowChromeArea::TabLine,
                        string_id,
                        text,
                    )],
                    ..Default::default()
                }],
            )
            .unwrap();
    }
    eval.command_loop
        .keyboard
        .kboard
        .presented_mouse_observation = Some(PresentedMouseObservation {
        presentation: 7,
        hit: Some(hit),
        x: 44.0,
        y: 8.0,
        frame_id: frame_id.0,
    });

    let position = crate::emacs_core::Context::make_mouse_position(44.0, 8.0, frame_id.0, &eval);
    let parts = crate::emacs_core::value::list_to_vec(&position).expect("position");
    assert_eq!(parts[0], Value::make_window(window_id.0));
    assert_eq!(parts[1].as_symbol_name(), Some("tab-line"));
    assert_eq!(parts[4], Value::cons(text, Value::fixnum(6)));
}

#[test]
fn presented_region_drives_exact_gnu_mouse_position_and_rejects_stale_observation() {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("presented-region");
    eval.buffer_manager_mut().set_current(buffer);
    eval.buffer_manager_mut()
        .get_mut(buffer)
        .unwrap()
        .insert("abcdefg");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(7),
            Value::fixnum(8),
            Value::symbol("help-echo"),
            Value::string("semantic tip"),
        ],
    )
    .unwrap();
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("presented-region", 200, 100, buffer);
    let window_id = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).unwrap();
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_and_activate_display_presentation_for_test(
                crate::window::geometry::PresentationId::new(1),
                vec![crate::window::WindowDisplaySnapshot {
                    window_id,
                    regions: crate::window::PresentedWindowRegions {
                        outer: neomacs_display_protocol::types::Rect::new(10.0, 5.0, 180.0, 90.0),
                        text_body: neomacs_display_protocol::types::Rect::new(
                            20.0, 10.0, 160.0, 70.0,
                        ),
                        mode_line: Some(neomacs_display_protocol::types::Rect::new(
                            10.0, 80.0, 180.0, 15.0,
                        )),
                        ..Default::default()
                    },
                    regions_materialized: true,
                    points: vec![crate::window::DisplayPointSnapshot {
                        role: crate::window::DisplayPointRole::Glyph,
                        // Poison the legacy coordinate lookup; the transported
                        // renderer hit below owns position 7.
                        buffer_pos: crate::buffer::LispCharPos1::new(1),
                        x: 0,
                        y: 5,
                        width: 8,
                        height: 16,
                        row: 3,
                        col: 4,
                    }],
                    body_rows: vec![crate::window::PresentedBodyRowSnapshot {
                        output_row: 3,
                        body_row: 0,
                        body_y: 0,
                    }],
                    ..Default::default()
                }],
            )
            .unwrap();
    }
    let protocol_window = neomacs_display_protocol::DisplayWindowId::new(window_id.0 as i64);
    let hit = neomacs_display_protocol::PresentedHitIndex::from_parts(
        neomacs_display_protocol::PresentationId::new(1),
        vec![neomacs_display_protocol::PresentedHitRegion::new(
            Some(protocol_window),
            neomacs_display_protocol::PresentedRegionKind::TextBody,
            neomacs_display_protocol::FrameRect::new(20.0, 10.0, 160.0, 70.0).unwrap(),
            0,
        )],
        vec![neomacs_display_protocol::PresentedTextPosition::new(
            protocol_window,
            neomacs_display_protocol::FrameRect::new(20.0, 10.0, 8.0, 16.0).unwrap(),
            7,
            3,
            4,
        )],
    )
    .unwrap()
    .resolve(neomacs_display_protocol::PresentedHitQuery::new(
        neomacs_display_protocol::PresentationId::new(1),
        22.0,
        12.0,
    ))
    .unwrap();

    assert_eq!(
        eval.handle_read_char_input_event(InputEvent::PresentedRegion {
            presentation: 1,
            hit,
            x: 22.0,
            y: 12.0,
            target_frame_id: frame_id.0,
        })
        .unwrap(),
        None
    );
    eval.handle_read_char_input_event(InputEvent::PresentedRegion {
        presentation: 0,
        hit: None,
        x: 22.0,
        y: 12.0,
        target_frame_id: frame_id.0,
    })
    .unwrap();
    assert_eq!(
        eval.command_loop
            .keyboard
            .kboard
            .presented_mouse_observation
            .unwrap()
            .presentation,
        1,
        "stale observation must not replace the accepted presentation"
    );

    let event = eval
        .handle_read_char_input_event(InputEvent::MousePress {
            button: MouseButton::Left,
            x: 22.0,
            y: 12.0,
            modifiers: Modifiers::none(),
            target_frame_id: frame_id.0,
        })
        .unwrap()
        .unwrap();
    let event = crate::emacs_core::value::list_to_vec(&event).unwrap();
    let position = crate::emacs_core::value::list_to_vec(&event[1]).unwrap();
    assert_eq!(position[0], Value::make_window(window_id.0));
    assert_eq!(position[1], Value::fixnum(7));
    assert_eq!(position[2].cons_car(), Value::fixnum(2));
    assert_eq!(position[2].cons_cdr(), Value::fixnum(2));
    assert_eq!(position[5], Value::fixnum(7));
    assert_eq!(position[6].cons_car(), Value::fixnum(4));
    assert_eq!(position[6].cons_cdr(), Value::fixnum(3));
    assert_eq!(position[9].cons_car(), Value::fixnum(8));
    assert_eq!(position[9].cons_cdr(), Value::fixnum(16));

    eval.queue_mouse_help_echo_update(Some(frame_id), 22, 12);
    let help = eval
        .command_loop
        .keyboard
        .kboard
        .last_help_echo_event
        .expect("semantic help echo");
    let help = crate::emacs_core::value::list_to_vec(&help).unwrap();
    assert_eq!(
        help[2].as_lisp_string().unwrap().as_utf8_str(),
        Some("semantic tip")
    );
    assert_eq!(help[5], Value::fixnum(7));

    let divider_hit = neomacs_display_protocol::PresentedHitIndex::from_parts(
        neomacs_display_protocol::PresentationId::new(1),
        vec![neomacs_display_protocol::PresentedHitRegion::new(
            Some(protocol_window),
            neomacs_display_protocol::PresentedRegionKind::RightDivider,
            neomacs_display_protocol::FrameRect::new(182.0, 5.0, 8.0, 75.0).unwrap(),
            30,
        )],
        vec![],
    )
    .unwrap()
    .resolve(neomacs_display_protocol::PresentedHitQuery::new(
        neomacs_display_protocol::PresentationId::new(1),
        185.0,
        40.0,
    ))
    .unwrap();
    eval.handle_read_char_input_event(InputEvent::PresentedRegion {
        presentation: 1,
        hit: divider_hit,
        x: 185.0,
        y: 40.0,
        target_frame_id: frame_id.0,
    })
    .unwrap();
    let divider_event = eval
        .handle_read_char_input_event(InputEvent::MousePress {
            button: MouseButton::Left,
            x: 185.0,
            y: 40.0,
            modifiers: Modifiers::none(),
            target_frame_id: frame_id.0,
        })
        .unwrap()
        .unwrap();
    let divider_event = crate::emacs_core::value::list_to_vec(&divider_event).unwrap();
    let divider_position = crate::emacs_core::value::list_to_vec(&divider_event[1]).unwrap();
    assert_eq!(
        divider_position[1].as_symbol_name(),
        Some("vertical-line"),
        "the typed divider hit must reach GNU's mouse-drag-vertical-line binding"
    );

    eval.command_loop
        .keyboard
        .kboard
        .presented_mouse_observation = None;
    let event = eval
        .handle_read_char_input_event(InputEvent::MousePress {
            button: MouseButton::Left,
            x: 22.0,
            y: 12.0,
            modifiers: Modifiers::none(),
            target_frame_id: frame_id.0,
        })
        .unwrap()
        .unwrap();
    let event = crate::emacs_core::value::list_to_vec(&event).unwrap();
    let position = crate::emacs_core::value::list_to_vec(&event[1]).unwrap();
    assert_eq!(position[0], Value::make_frame(frame_id.0));
    assert!(position[1].is_nil());
    assert!(
        eval.resolve_text_area_help_echo_event(frame_id, 22, 12)
            .is_none(),
        "GUI help must not fall back to poisoned live coordinate arithmetic"
    );
}

fn presented_pointer_fixture() -> (crate::emacs_core::Context, crate::window::FrameId, u64, u32) {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("presented-pointer");
    let frame = eval
        .frame_manager_mut()
        .create_frame("presented-pointer", 200, 100, buffer);
    let presentation = eval.begin_interaction_presentation();
    let interaction = eval.register_presented_mouse_target(
        presentation,
        PresentedMouseTarget {
            area: PresentedMouseArea::TabBar,
            posn_string: Value::cons(Value::string("tab"), Value::fixnum(0)),
        },
    );
    (eval, frame, presentation, interaction)
}

#[test]
fn pointer_before_retirement_resolves_before_snapshot_is_released() {
    let (mut eval, frame, presentation, interaction) = presented_pointer_fixture();
    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentedPointer {
            presentation,
            interaction,
            pressed: true,
            button: 1,
            x: 24.0,
            y: 8.0,
            emacs_frame_id: frame.0,
        });
    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentationRetired { presentation });

    let pointer = eval
        .read_char_with_timeout(Some(std::time::Duration::ZERO))
        .expect("read should succeed")
        .expect("pointer preceding retirement must remain readable");
    assert_eq!(
        crate::emacs_core::value::list_to_vec(&pointer)
            .expect("mouse event")
            .first()
            .and_then(|value| value.as_symbol_name()),
        Some("down-mouse-1")
    );
    assert!(
        eval.resolve_presented_mouse_target(presentation, interaction)
            .is_some(),
        "the later retirement must not overtake the pointer"
    );

    assert_eq!(
        crate::emacs_core::reader::builtin_input_pending_p(&mut eval, vec![])
            .expect("retirement service"),
        Value::NIL
    );
    assert_eq!(
        eval.resolve_presented_mouse_target(presentation, interaction),
        None
    );
}

#[test]
fn retirement_before_pointer_rejects_stale_hit_without_reordering_key() {
    let (mut eval, frame, presentation, interaction) = presented_pointer_fixture();
    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentationRetired { presentation });
    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentedPointer {
            presentation,
            interaction,
            pressed: true,
            button: 1,
            x: 24.0,
            y: 8.0,
            emacs_frame_id: frame.0,
        });
    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::key_press(KeyEvent::char('k')));

    let event = eval
        .read_char_with_timeout(Some(std::time::Duration::ZERO))
        .expect("read should succeed");

    assert_eq!(event, Some(Value::fixnum('k' as i64)));
    assert_eq!(
        eval.resolve_presented_mouse_target(presentation, interaction),
        None
    );
}

#[test]
fn layout_invalidation_forces_redisplay_when_evaluator_signature_is_unchanged() {
    let redisplays = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = std::rc::Rc::clone(&redisplays);
    let mut eval = crate::emacs_core::Context::new();
    eval.redisplay_fn = Some(Box::new(move |_| observed.set(observed.get() + 1)));

    eval.redisplay();
    eval.redisplay();
    assert_eq!(redisplays.get(), 1, "unchanged redisplay should be skipped");

    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::LayoutInvalidated);
    let effects = eval.service_leading_internal_frontend_events();
    assert!(effects.redisplay_needed);
    eval.redisplay();
    assert_eq!(redisplays.get(), 2);
}

#[test]
fn presentation_activation_event_atomically_exposes_prepared_geometry() {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("activation-event");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("activation-event", 200, 100, buffer);
    let presentation = crate::window::geometry::PresentationId::new(41);
    eval.frame_manager_mut()
        .get_mut(frame_id)
        .expect("frame")
        .prepare_live_window_presentation(presentation, Vec::new())
        .expect("prepare presentation");

    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentationActivated {
            presentation: presentation.get(),
            emacs_frame_id: frame_id.0,
        });
    eval.service_leading_internal_frontend_events();

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert_eq!(frame.active_presentation(), Some(presentation));
    assert!(!frame.is_display_presentation_prepared(presentation));

    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentationRetired {
            presentation: presentation.get(),
        });
    eval.service_leading_internal_frontend_events();
    assert_eq!(
        eval.frame_manager()
            .get(frame_id)
            .expect("frame")
            .active_presentation(),
        None
    );
}

#[test]
fn presentation_discard_event_removes_only_prepared_geometry() {
    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("discard-event");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("discard-event", 200, 100, buffer);
    let active = crate::window::geometry::PresentationId::new(41);
    let discarded = crate::window::geometry::PresentationId::new(42);
    let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
    frame
        .prepare_live_window_presentation(active, Vec::new())
        .expect("prepare active presentation");
    frame
        .activate_display_presentation(active)
        .expect("activate presentation");
    frame
        .prepare_live_window_presentation(discarded, Vec::new())
        .expect("prepare discarded presentation");
    let interaction = eval.register_presented_mouse_target(
        discarded.get(),
        PresentedMouseTarget {
            area: PresentedMouseArea::TabBar,
            posn_string: Value::string("discarded"),
        },
    );

    eval.command_loop
        .keyboard
        .pending_input_events
        .push_back(InputEvent::PresentationDiscarded {
            presentation: discarded.get(),
            emacs_frame_id: frame_id.0,
        });
    eval.service_leading_internal_frontend_events();

    let frame = eval.frame_manager().get(frame_id).expect("frame");
    assert_eq!(frame.active_presentation(), Some(active));
    assert!(!frame.is_display_presentation_prepared(discarded));
    assert!(
        eval.resolve_presented_mouse_target(discarded.get(), interaction)
            .is_none()
    );
}

fn reset_keyboard_test_terminals() {
    crate::emacs_core::terminal::pure::reset_terminal_thread_locals();
}

fn ensure_keyboard_test_terminal(id: u64) {
    crate::emacs_core::terminal::pure::ensure_terminal_runtime_owner(
        id,
        format!("tty-{id}"),
        crate::emacs_core::terminal::pure::TerminalRuntimeConfig::interactive(
            Some("xterm-256color".to_string()),
            neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities::full_with_color_cells(256),
        ),
    );
}

#[test]
fn key_event_description() {
    crate::test_utils::init_test_tracing();
    let e = KeyEvent::char('x');
    assert_eq!(e.to_description(), "x");

    let e = KeyEvent::char_with_mods('x', Modifiers::ctrl());
    assert_eq!(e.to_description(), "C-x");

    let e = KeyEvent::char_with_mods('f', Modifiers::meta());
    assert_eq!(e.to_description(), "M-f");

    let e = KeyEvent::char_with_mods('g', Modifiers::ctrl_meta());
    assert_eq!(e.to_description(), "C-M-g");

    // GNU distinguishes the <return> FUNCTION KEY from the RET character.
    // `(single-key-description 'return)` => "<return>" and
    // `(single-key-description ?\r)` => "RET" in real GNU Emacs. Since
    // 220938220 ("preserve named GUI key events"), `NamedKey::Return` models the
    // GUI `<return>` key (emacs symbol `return`), so it describes as "<return>";
    // the RET character is `Key::Char('\r')` and describes as "RET". (When an
    // unbound `<return>` is read, `read_key_sequence` falls back to ASCII 13 via
    // function-key-map -- see the read_key_sequence_* tests in eval_test.rs.)
    let e = KeyEvent::named(NamedKey::Return);
    assert_eq!(e.to_description(), "<return>");

    let e = KeyEvent::char('\r');
    assert_eq!(e.to_description(), "RET");
}

#[test]
fn key_event_parse() {
    crate::test_utils::init_test_tracing();
    let e = KeyEvent::from_description("C-x").unwrap();
    assert_eq!(e.key, Key::Char('x'));
    assert!(e.modifiers.ctrl);
    assert!(!e.modifiers.meta);

    let e = KeyEvent::from_description("M-f").unwrap();
    assert_eq!(e.key, Key::Char('f'));
    assert!(e.modifiers.meta);

    let e = KeyEvent::from_description("RET").unwrap();
    assert_eq!(e.key, Key::Named(NamedKey::Return));

    let e = KeyEvent::from_description("C-M-g").unwrap();
    assert!(e.modifiers.ctrl);
    assert!(e.modifiers.meta);
}

#[test]
fn key_sequence_description() {
    crate::test_utils::init_test_tracing();
    let seq = KeySequence::from_description("C-x C-f").unwrap();
    assert_eq!(seq.len(), 2);
    assert_eq!(seq.to_description(), "C-x C-f");
}

#[test]
fn prefix_arg_values() {
    crate::test_utils::init_test_tracing();
    assert_eq!(PrefixArg::None.numeric_value(), 1);
    assert_eq!(PrefixArg::Numeric(5).numeric_value(), 5);
    assert_eq!(PrefixArg::Raw(1).numeric_value(), 4);
    assert_eq!(PrefixArg::Raw(2).numeric_value(), 16);
}

#[test]
fn command_loop_enqueue_read() {
    crate::test_utils::init_test_tracing();
    let mut cl = CommandLoop::new();
    cl.enqueue_event(InputEvent::key_press(KeyEvent::char('a')));
    cl.enqueue_event(InputEvent::key_press(KeyEvent::char('b')));

    let e = cl.read_key_event().unwrap();
    assert_eq!(e, Value::fixnum('a' as i64));
    let e = cl.read_key_event().unwrap();
    assert_eq!(e, Value::fixnum('b' as i64));
    assert!(cl.read_key_event().is_none());
}

#[test]
fn unread_events_have_priority() {
    crate::test_utils::init_test_tracing();
    let mut cl = CommandLoop::new();
    cl.enqueue_event(InputEvent::key_press(KeyEvent::char('a')));
    cl.unread_key(KeyEvent::char('z'));

    let e = cl.read_key_event().unwrap();
    assert_eq!(e, Value::fixnum('z' as i64)); // unread first
    let e = cl.read_key_event().unwrap();
    assert_eq!(e, Value::fixnum('a' as i64)); // then queue
}

#[test]
fn keyboard_runtime_preserves_kboard_state_per_terminal() {
    crate::test_utils::init_test_tracing();
    reset_keyboard_test_terminals();
    ensure_keyboard_test_terminal(7);
    let mut runtime = KeyboardRuntime::new();
    runtime.set_input_decode_map(Value::symbol("primary-map"));
    runtime.unread_event(Value::fixnum(1));

    runtime.select_terminal(7);
    assert_eq!(runtime.active_terminal_id(), 7);
    assert_eq!(runtime.input_decode_map(), Value::NIL);
    assert!(runtime.kboard.unread_events.is_empty());

    runtime.set_input_decode_map(Value::symbol("secondary-map"));
    runtime.unread_event(Value::fixnum(2));

    runtime.select_terminal(crate::emacs_core::terminal::pure::TERMINAL_ID);
    assert_eq!(
        runtime.input_decode_map(),
        Value::symbol("primary-map"),
        "switching back should restore the original terminal kboard state"
    );
    assert_eq!(
        runtime.kboard.unread_events.pop_front(),
        Some(Value::fixnum(1)),
        "unread events should be terminal-local"
    );

    runtime.select_terminal(7);
    assert_eq!(runtime.input_decode_map(), Value::symbol("secondary-map"));
    assert_eq!(
        runtime.kboard.unread_events.pop_front(),
        Some(Value::fixnum(2))
    );
}

#[test]
fn keyboard_runtime_polls_parked_kboards_after_active_one() {
    crate::test_utils::init_test_tracing();
    reset_keyboard_test_terminals();
    ensure_keyboard_test_terminal(7);
    ensure_keyboard_test_terminal(9);
    let mut runtime = KeyboardRuntime::new();
    runtime.unread_event(Value::fixnum(1));
    runtime.select_terminal(7);
    runtime.unread_event(Value::fixnum(2));
    runtime.select_terminal(9);
    runtime.unread_event(Value::fixnum(3));
    runtime.select_terminal(crate::emacs_core::terminal::pure::TERMINAL_ID);

    assert_eq!(runtime.read_key_event(), Some(Value::fixnum(1)));
    assert_eq!(
        runtime.read_key_event(),
        Some(Value::fixnum(3)),
        "after the active kboard drains, parked terminal input should be read in GNU terminal-list order"
    );
    assert_eq!(
        runtime.read_key_event(),
        Some(Value::fixnum(2)),
        "older parked terminal input should be read after newer terminals"
    );
    assert_eq!(runtime.active_terminal_id(), 7);
}

#[test]
fn keyboard_runtime_reports_pending_input_across_parked_kboards() {
    crate::test_utils::init_test_tracing();
    reset_keyboard_test_terminals();
    ensure_keyboard_test_terminal(9);
    let mut runtime = KeyboardRuntime::new();
    runtime.select_terminal(9);
    runtime.unread_event(Value::fixnum(99));
    runtime.select_terminal(crate::emacs_core::terminal::pure::TERMINAL_ID);

    assert!(
        runtime.has_pending_kboard_input(),
        "parked terminal unread input should still count as pending"
    );
}

#[test]
fn keyboard_macro_recording() {
    crate::test_utils::init_test_tracing();
    let mut cl = CommandLoop::new();
    cl.start_kbd_macro();

    cl.enqueue_event(InputEvent::key_press(KeyEvent::char('h')));
    cl.enqueue_event(InputEvent::key_press(KeyEvent::char('i')));

    cl.read_key_event(); // 'h' — recorded
    cl.read_key_event(); // 'i' — recorded

    cl.finalize_kbd_macro_chars();
    let recorded = cl.end_kbd_macro();
    assert_eq!(recorded.len(), 2);
    assert_eq!(cl.keyboard.kboard.kbd_macro_end, 2);

    // Replay.
    cl.begin_executing_kbd_macro(recorded);
    let e1 = cl.read_key_event().unwrap();
    assert_eq!(e1, Value::fixnum('h' as i64));
    let e2 = cl.read_key_event().unwrap();
    assert_eq!(e2, Value::fixnum('i' as i64));
}

#[test]
fn quit_flag() {
    crate::test_utils::init_test_tracing();
    let mut cl = CommandLoop::new();
    assert!(!cl.check_quit());

    cl.signal_quit();
    assert!(cl.check_quit());
    assert!(!cl.check_quit()); // cleared
}

#[test]
fn interactive_spec_parsing() {
    crate::test_utils::init_test_tracing();
    let codes = parse_interactive_spec("sSearch for: \nnCount: ");
    assert_eq!(codes.len(), 2);
    assert!(
        matches!(&codes[0], InteractiveCode::StringArg(p) if p.as_utf8_str() == Some("Search for: "))
    );
    assert!(
        matches!(&codes[1], InteractiveCode::NumberArg(p) if p.as_utf8_str() == Some("Count: "))
    );
}

#[test]
fn modifier_bits_round_trip() {
    crate::test_utils::init_test_tracing();
    let m = Modifiers {
        ctrl: true,
        meta: true,
        shift: false,
        super_: false,
        hyper: false,
    };
    let bits = m.to_bits();
    let m2 = Modifiers::from_bits(bits);
    assert_eq!(m, m2);
}

#[test]
fn modifier_bits_round_trip_all_combinations() {
    crate::test_utils::init_test_tracing();
    // Test each individual modifier
    for (field, expected_bit) in [
        ("ctrl", 1u32 << 26),
        ("meta", 1u32 << 27),
        ("shift", 1u32 << 25),
        ("super", 1u32 << 23),
        ("hyper", 1u32 << 24),
    ] {
        let m = match field {
            "ctrl" => Modifiers {
                ctrl: true,
                ..Modifiers::none()
            },
            "meta" => Modifiers {
                meta: true,
                ..Modifiers::none()
            },
            "shift" => Modifiers {
                shift: true,
                ..Modifiers::none()
            },
            "super" => Modifiers {
                super_: true,
                ..Modifiers::none()
            },
            "hyper" => Modifiers {
                hyper: true,
                ..Modifiers::none()
            },
            _ => unreachable!(),
        };
        assert_eq!(m.to_bits(), expected_bit, "bit mismatch for {}", field);
        assert_eq!(
            Modifiers::from_bits(m.to_bits()),
            m,
            "round-trip failed for {}",
            field
        );
    }

    // All modifiers set
    let all = Modifiers {
        ctrl: true,
        meta: true,
        shift: true,
        super_: true,
        hyper: true,
    };
    assert_eq!(Modifiers::from_bits(all.to_bits()), all);

    // No modifiers
    assert_eq!(Modifiers::none().to_bits(), 0);
    assert_eq!(Modifiers::from_bits(0), Modifiers::none());
}

#[test]
fn prefix_string_various() {
    crate::test_utils::init_test_tracing();
    assert_eq!(Modifiers::none().prefix_string(), "");
    assert_eq!(Modifiers::ctrl().prefix_string(), "C-");
    assert_eq!(Modifiers::meta().prefix_string(), "M-");
    assert_eq!(Modifiers::ctrl_meta().prefix_string(), "C-M-");

    let all = Modifiers {
        ctrl: true,
        meta: true,
        shift: true,
        super_: true,
        hyper: true,
    };
    // Order: H- s- C- M- S-
    assert_eq!(all.prefix_string(), "H-s-C-M-S-");
}

#[test]
fn modifiers_is_empty() {
    crate::test_utils::init_test_tracing();
    assert!(Modifiers::none().is_empty());
    assert!(!Modifiers::ctrl().is_empty());
    assert!(!Modifiers::meta().is_empty());
}

#[test]
fn key_event_from_description_all_named_keys() {
    crate::test_utils::init_test_tracing();
    let cases = [
        ("RET", Key::Named(NamedKey::Return)),
        ("TAB", Key::Named(NamedKey::Tab)),
        ("ESC", Key::Named(NamedKey::Escape)),
        ("DEL", Key::Named(NamedKey::Backspace)),
        ("SPC", Key::Char(' ')),
        ("<delete>", Key::Named(NamedKey::Delete)),
        ("<insert>", Key::Named(NamedKey::Insert)),
        ("<home>", Key::Named(NamedKey::Home)),
        ("<end>", Key::Named(NamedKey::End)),
        ("<prior>", Key::Named(NamedKey::PageUp)),
        ("<next>", Key::Named(NamedKey::PageDown)),
        ("<left>", Key::Named(NamedKey::Left)),
        ("<right>", Key::Named(NamedKey::Right)),
        ("<up>", Key::Named(NamedKey::Up)),
        ("<down>", Key::Named(NamedKey::Down)),
        ("<f1>", Key::Named(NamedKey::F(1))),
        ("<f12>", Key::Named(NamedKey::F(12))),
    ];
    for (desc, expected_key) in cases {
        let e =
            KeyEvent::from_description(desc).unwrap_or_else(|| panic!("failed to parse: {}", desc));
        assert_eq!(e.key, expected_key, "mismatch for {}", desc);
        assert!(e.modifiers.is_empty(), "unexpected modifiers for {}", desc);
    }
}

#[test]
fn key_event_description_round_trip() {
    crate::test_utils::init_test_tracing();
    let descriptions = [
        "C-x", "M-f", "C-M-g", "S-<f1>", "H-s-a", "RET", "TAB", "SPC", "<left>",
    ];
    for desc in descriptions {
        let event = KeyEvent::from_description(desc).unwrap();
        let back = event.to_description();
        let reparsed = KeyEvent::from_description(&back).unwrap();
        assert_eq!(event, reparsed, "round-trip failed for {}", desc);
    }
}

#[test]
fn prefix_arg_to_value() {
    crate::test_utils::init_test_tracing();
    assert_eq!(PrefixArg::None.to_value(), Value::NIL);
    assert_eq!(PrefixArg::Numeric(3).to_value(), Value::fixnum(3));
    // Raw(1) = C-u once = (4)
    let raw1 = PrefixArg::Raw(1).to_value();
    assert!(raw1.is_cons());
}

#[test]
fn key_sequence_from_description_multi() {
    crate::test_utils::init_test_tracing();
    let seq = KeySequence::from_description("C-x C-s").unwrap();
    assert_eq!(seq.len(), 2);
    assert_eq!(seq.events[0], KeyEvent::from_description("C-x").unwrap());
    assert_eq!(seq.events[1], KeyEvent::from_description("C-s").unwrap());
}

#[test]
fn key_sequence_empty() {
    crate::test_utils::init_test_tracing();
    let seq = KeySequence::new();
    assert!(seq.is_empty());
    assert_eq!(seq.to_description(), "");
}

#[test]
fn read_key_sequence_state_tracks_raw_and_translated_events() {
    crate::test_utils::init_test_tracing();
    let mut state = ReadKeySequenceState::new();
    state.push_input_event(Value::fixnum('A' as i64));
    state.push_input_event(Value::fixnum('B' as i64));
    state.replace_translated_events(vec![Value::fixnum('a' as i64)]);

    let (translated, raw) = state.snapshot();
    assert_eq!(translated, vec![Value::fixnum('a' as i64)]);
    assert_eq!(
        raw,
        vec![Value::fixnum('A' as i64), Value::fixnum('B' as i64)]
    );
}

#[test]
fn key_sequence_translation_events_normalizes_vector_string_and_scalar() {
    crate::test_utils::init_test_tracing();
    let vector = Value::vector(vec![Value::fixnum('x' as i64), Value::fixnum('y' as i64)]);
    assert_eq!(
        key_sequence_translation_events(vector),
        Some(vec![Value::fixnum('x' as i64), Value::fixnum('y' as i64)])
    );
    assert_eq!(
        key_sequence_translation_events(Value::string("ab")),
        Some(vec![Value::fixnum('a' as i64), Value::fixnum('b' as i64)])
    );
    assert_eq!(
        key_sequence_translation_events(Value::symbol("f1")),
        Some(vec![Value::symbol("f1")])
    );
    assert_eq!(key_sequence_translation_events(Value::NIL), None);
}

#[test]
fn parse_interactive_spec_all_codes() {
    crate::test_utils::init_test_tracing();
    let codes = parse_interactive_spec("d");
    assert!(matches!(&codes[0], InteractiveCode::Point));

    let codes = parse_interactive_spec("m");
    assert!(matches!(&codes[0], InteractiveCode::Mark));

    let codes = parse_interactive_spec("r");
    assert!(matches!(&codes[0], InteractiveCode::Region));

    let codes = parse_interactive_spec("p");
    assert!(matches!(&codes[0], InteractiveCode::PrefixNumeric));

    let codes = parse_interactive_spec("P");
    assert!(matches!(&codes[0], InteractiveCode::PrefixRaw));

    let codes = parse_interactive_spec("fFile: ");
    assert!(matches!(&codes[0], InteractiveCode::FileName(p) if p.as_utf8_str() == Some("File: ")));

    let codes = parse_interactive_spec("DDirectory: ");
    assert!(
        matches!(&codes[0], InteractiveCode::DirectoryName(p) if p.as_utf8_str() == Some("Directory: "))
    );
}

#[test]
fn parse_interactive_spec_empty() {
    crate::test_utils::init_test_tracing();
    let codes = parse_interactive_spec("");
    assert_eq!(codes.len(), 1);
    assert!(matches!(&codes[0], InteractiveCode::None));
}

#[test]
fn inhibit_quit_blocks_signal() {
    crate::test_utils::init_test_tracing();
    let mut cl = CommandLoop::new();
    cl.inhibit_quit = true;
    cl.signal_quit();
    assert!(!cl.quit_flag); // should not be set when inhibited
}

// ===================================================================
// keysym_to_key_event — control characters
// ===================================================================

#[test]
fn keysym_ctrl_x_from_control_char() {
    crate::test_utils::init_test_tracing();
    // Ctrl+x → winit gives keysym 0x18 (control character)
    let event = keysym_to_key_event(0x18, RENDER_CTRL_MASK).unwrap();
    assert_eq!(event.key, Key::Char('x'));
    assert!(event.modifiers.ctrl);
}

#[test]
fn keysym_ctrl_a_from_control_char() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x01, RENDER_CTRL_MASK).unwrap();
    assert_eq!(event.key, Key::Char('a'));
    assert!(event.modifiers.ctrl);
}

#[test]
fn keysym_ctrl_z_from_control_char() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x1A, RENDER_CTRL_MASK).unwrap();
    assert_eq!(event.key, Key::Char('z'));
    assert!(event.modifiers.ctrl);
}

#[test]
fn keysym_ctrl_g_from_control_char_no_modifier() {
    crate::test_utils::init_test_tracing();
    // Even without explicit ctrl modifier bit, control char implies ctrl
    let event = keysym_to_key_event(0x07, 0).unwrap();
    assert_eq!(event.key, Key::Char('g'));
    assert!(event.modifiers.ctrl);
}

#[test]
fn keysym_raw_tty_escape_is_meta_prefix_char() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x1B, 0).unwrap();
    assert_eq!(event.key, Key::Char('\u{1b}'));
    assert!(event.modifiers.is_empty());
    assert_eq!(event.to_emacs_event_value(), Value::fixnum(27));
}

#[test]
fn keysym_raw_tty_delete_is_del_char() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x7F, RENDER_META_MASK).unwrap();
    assert_eq!(event.key, Key::Char('\u{7f}'));
    assert!(event.modifiers.meta);
    assert_eq!(
        event.to_emacs_event_value(),
        Value::fixnum(0x7F | (1 << 27))
    );
}

#[test]
fn keysym_gui_backspace_is_function_key_not_c_h() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(XK_BACKSPACE, 0).unwrap();
    assert_eq!(event.key, Key::Named(NamedKey::Backspace));
    assert_eq!(event.to_emacs_event_value(), Value::symbol("backspace"));
}

#[test]
fn keysym_raw_control_h_stays_help_char() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x08, 0).unwrap();
    assert_eq!(event.key, Key::Char('h'));
    assert!(event.modifiers.ctrl);
    assert_eq!(event.to_emacs_event_value(), Value::fixnum(8));
}

#[test]
fn keysym_ctrl_x_from_printable_with_modifier() {
    crate::test_utils::init_test_tracing();
    // Ctrl+x when winit gives keysym 0x78 ('x') with ctrl modifier
    let event = keysym_to_key_event(0x78, RENDER_CTRL_MASK).unwrap();
    assert_eq!(event.key, Key::Char('x'));
    assert!(event.modifiers.ctrl);
}

#[test]
fn keysym_shifted_uppercase_char_drops_shift_modifier() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event('A' as u32, RENDER_SHIFT_MASK).unwrap();
    assert_eq!(event.key, Key::Char('A'));
    assert!(!event.modifiers.shift);
}

#[test]
fn keysym_unicode_scalar_maps_to_character_event() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event('中' as u32, 0).unwrap();
    assert_eq!(event.key, Key::Char('中'));
    assert!(event.modifiers.is_empty());
}

#[test]
fn keysym_ctrl_shift_letter_preserves_shift_for_command_chord() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event('f' as u32, RENDER_CTRL_MASK | RENDER_SHIFT_MASK).unwrap();
    assert_eq!(event.key, Key::Char('f'));
    assert!(event.modifiers.ctrl);
    assert!(event.modifiers.shift);
    assert_eq!(event.to_description(), "C-S-f");
    assert_eq!(event.to_emacs_event_value(), Value::fixnum((1 << 25) | 6));
}

#[test]
fn keysym_ctrl_shift_control_text_preserves_shift_for_command_chord() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(0x06, RENDER_CTRL_MASK | RENDER_SHIFT_MASK).unwrap();
    assert_eq!(event.key, Key::Char('f'));
    assert!(event.modifiers.ctrl);
    assert!(event.modifiers.shift);
    assert_eq!(event.to_description(), "C-S-f");
}

#[test]
fn keysym_meta_shift_letter_consumes_shift_into_uppercase() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event('f' as u32, RENDER_META_MASK | RENDER_SHIFT_MASK).unwrap();
    assert_eq!(event.key, Key::Char('F'));
    assert!(event.modifiers.meta);
    assert!(!event.modifiers.shift);
    assert_eq!(event.to_description(), "M-F");
}

#[test]
fn keysym_ctrl_uppercase_without_shift_treats_caps_lock_as_unshifted() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event('F' as u32, RENDER_CTRL_MASK).unwrap();
    assert_eq!(event.key, Key::Char('f'));
    assert!(event.modifiers.ctrl);
    assert!(!event.modifiers.shift);
    assert_eq!(event.to_description(), "C-f");
}

#[test]
fn keysym_shift_space_preserves_gnu_distinction() {
    crate::test_utils::init_test_tracing();
    let event = keysym_to_key_event(' ' as u32, RENDER_SHIFT_MASK).unwrap();
    assert_eq!(event.key, Key::Char(' '));
    assert!(event.modifiers.shift);
    assert_eq!(event.to_description(), "S-SPC");
}

#[test]
fn render_modifiers_helper_matches_transport_bit_layout() {
    crate::test_utils::init_test_tracing();
    let mods =
        render_modifiers_to_modifiers(RENDER_SHIFT_MASK | RENDER_CTRL_MASK | RENDER_META_MASK);
    assert!(mods.shift);
    assert!(mods.ctrl);
    assert!(mods.meta);
    assert!(!mods.super_);
    assert!(!mods.hyper);
}

#[test]
fn render_key_transport_drops_key_releases() {
    crate::test_utils::init_test_tracing();
    assert!(render_key_transport_to_input_event(XK_RETURN, 0, false, 0).is_none());
}

#[test]
fn read_key_sequence_with_timeout_returns_nil_like_gnu() {
    // GNU parity (oracle divergence cx429): in batch, with no input
    // arriving, a with-timeout timer must fire during read-key-sequence's
    // wait and throw out of it, so the form returns nil - not an empty key
    // sequence string. The evaluator here has no input receiver, which is
    // exactly the batch shape.
    crate::test_utils::init_test_tracing();
    // with-timeout is timer.el lisp, so this needs the bootstrapped runtime.
    let mut eval =
        crate::emacs_core::load::create_bootstrap_evaluator_cached().expect("bootstrap evaluator");
    let result = eval
        .eval_str_each(
            "(condition-case e
                 (with-timeout (0.01) (read-key-sequence \"test: \"))
               (error (car e)))",
        )
        .pop()
        .expect("one form")
        .expect("evaluation succeeds");
    assert!(
        result.is_nil(),
        "expected nil from timed-out read-key-sequence, got {}",
        crate::emacs_core::print::print_value(&result)
    );
}

#[test]
fn fresh_character_events_go_through_keyboard_translate_table_like_gnu() {
    // GNU `read_char' translates a freshly read character through
    // `keyboard-translate-table' (src/keyboard.c:3149-3163) before the key
    // sequence layer sees it. That is the mechanism
    // `normal-erase-is-backspace-mode' uses on a ^H-erase terminal: it
    // key-translates C-h to DEL (lisp/simple.el:11178), so the 0x08 the
    // Backspace key sends must arrive as 127 (DIVERGENCES.md entry 67).
    crate::test_utils::init_test_tracing();
    let mut eval = crate::emacs_core::Context::new();

    // No table: the event passes through unchanged.
    assert_eq!(
        eval.translate_fresh_character_event(Value::fixnum(8)),
        Value::fixnum(8)
    );

    eval.eval_str_each(
        "(progn (setq keyboard-translate-table
                      (make-char-table 'keyboard-translate-table nil))
                (aset keyboard-translate-table 8 127)
                (aset keyboard-translate-table 127 4))",
    )
    .pop()
    .expect("one form")
    .expect("table setup succeeds");

    assert_eq!(
        eval.translate_fresh_character_event(Value::fixnum(8)),
        Value::fixnum(127),
        "C-h must translate to DEL"
    );
    assert_eq!(
        eval.translate_fresh_character_event(Value::fixnum(127)),
        Value::fixnum(4),
        "DEL must translate to C-d"
    );
    // nil entries mean no translation, and non-character events are left
    // alone entirely.
    assert_eq!(
        eval.translate_fresh_character_event(Value::fixnum(i64::from(b'a'))),
        Value::fixnum(i64::from(b'a'))
    );
    assert_eq!(
        eval.translate_fresh_character_event(Value::symbol("f1")),
        Value::symbol("f1")
    );
}

/// GNU's `buffer_posn_from_coords` opens with `Fset_buffer (w->contents)` and
/// walks from `w->start` (src/dispnew.c), so a mouse posn's buffer position is
/// always a position in the window's *own* buffer.
///
/// An inactive mini-window is the one place where those can come apart here:
/// GNU displays the echo area by temporarily swapping `w->contents` for
/// ` *Echo Area 0*` inside `with_echo_area_buffer` (src/xdisp.c) and restoring
/// it on unwind, while this port renders the echo buffer through the live
/// mini-window and publishes the resulting rows as geometry only -- "its
/// geometry must remain available to rendering and hit testing, but it must
/// never become evidence about the live minibuffer"
/// ([`crate::window::WindowPresentationSnapshot`]).  The renderer's hit index
/// is built from that geometry, so the text position it reports for a hover
/// over a displayed message is a position in the echo buffer, and the window
/// it names is still bound to the empty ` *Minibuf-0*`.
#[test]
fn presented_mouse_position_over_the_inactive_echo_area_reports_the_mini_windows_own_point() {
    use crate::window::{
        PresentedWindowRegions, WindowDisplaySnapshot, WindowPresentationSnapshot,
    };
    use neomacs_display_protocol::{
        DisplayWindowId, FrameRect, PresentationId, PresentedHitIndex, PresentedHitQuery,
        PresentedHitRegion, PresentedRegionKind, PresentedTextPosition, Rect,
    };

    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("echo-area-posn");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("echo-area-posn", 2560, 600, buffer);
    let minibuffer_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .minibuffer_window
        .expect("frame minibuffer window");
    // The inactive mini-window stays bound to the empty ` *Minibuf-0*`.
    let minibuffer_buffer = eval.buffer_manager_mut().create_buffer(" *Minibuf-0*");
    let minibuffer_point_max = eval
        .buffer_manager()
        .get(minibuffer_buffer)
        .expect("minibuffer buffer")
        .point_max_lisp_char_pos()
        .as_i64();
    assert_eq!(
        minibuffer_point_max, 1,
        "an inactive minibuffer holds no text"
    );

    let presentation = PresentationId::new(3);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame
            .find_window_mut(minibuffer_window)
            .expect("minibuffer window")
            .set_buffer(minibuffer_buffer);
        frame.set_window_system(Some(Value::symbol("neo")));
        // The echo-area walk publishes its rows as geometry only, exactly as
        // `mark_inactive_echo_snapshot_geometry_only` does in redisplay.
        frame
            .prepare_display_presentation(
                crate::window::geometry::PresentationId::new(3),
                vec![WindowPresentationSnapshot::GeometryOnly(
                    WindowDisplaySnapshot {
                        window_id: minibuffer_window,
                        regions: PresentedWindowRegions {
                            outer: Rect::new(0.0, 584.0, 2560.0, 16.0),
                            text_body: Rect::new(0.0, 584.0, 2560.0, 16.0),
                            ..Default::default()
                        },
                        regions_materialized: true,
                        ..Default::default()
                    },
                )],
            )
            .expect("prepare echo-area geometry");
        frame
            .activate_display_presentation(crate::window::geometry::PresentationId::new(3))
            .expect("activate echo-area geometry");
    }

    // The renderer's hit for the end of a 120-column echo-area message.
    let protocol_window = DisplayWindowId::new(minibuffer_window.0 as i64);
    let hit = PresentedHitIndex::from_parts(
        presentation,
        vec![PresentedHitRegion::new(
            Some(protocol_window),
            PresentedRegionKind::TextBody,
            FrameRect::new(0.0, 584.0, 2560.0, 16.0).unwrap(),
            0,
        )],
        vec![PresentedTextPosition::new(
            protocol_window,
            FrameRect::new(1230.0, 586.0, 2524.0, 22.0).unwrap(),
            121,
            0,
            120,
        )],
    )
    .unwrap()
    .resolve(PresentedHitQuery::new(presentation, 1230.0, 586.0))
    .unwrap();
    eval.command_loop
        .keyboard
        .kboard
        .presented_mouse_observation = Some(PresentedMouseObservation {
        presentation: 3,
        hit,
        x: 1230.0,
        y: 586.0,
        frame_id: frame_id.0,
    });

    let position =
        crate::emacs_core::Context::make_mouse_position(1230.0, 586.0, frame_id.0, &eval);
    let parts = crate::emacs_core::value::list_to_vec(&position).expect("position");

    assert_eq!(parts[0], Value::make_window(minibuffer_window.0));
    assert_eq!(
        parts[5],
        Value::fixnum(1),
        "the posn must name a position in the mini-window's own buffer"
    );
}

/// Geometry-only echo-area rows describe a transient echo buffer, not the
/// live buffer owned by the minibuffer window.  A renderer text hit therefore
/// cannot be used to look up semantic help in that live buffer, even when the
/// numeric position happens to be valid there.
#[test]
fn geometry_only_echo_area_hit_cannot_publish_live_minibuffer_help_echo() {
    use crate::window::{
        PresentedWindowRegions, WindowDisplaySnapshot, WindowPresentationSnapshot,
    };
    use neomacs_display_protocol::{
        DisplayWindowId, FrameRect, PresentationId, PresentedHitIndex, PresentedHitQuery,
        PresentedHitRegion, PresentedRegionKind, PresentedTextPosition, Rect,
    };

    let mut eval = crate::emacs_core::Context::new();
    let buffer = eval.buffer_manager_mut().create_buffer("echo-help-frame");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("echo-help-frame", 400, 120, buffer);
    let minibuffer_window = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .minibuffer_window
        .expect("frame minibuffer window");
    let minibuffer_buffer = eval
        .buffer_manager_mut()
        .create_buffer(" *Minibuf-help-evidence*");
    eval.buffer_manager_mut().set_current(minibuffer_buffer);
    eval.buffer_manager_mut()
        .get_mut(minibuffer_buffer)
        .expect("minibuffer buffer")
        .insert("abcdefghij");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(7),
            Value::fixnum(8),
            Value::symbol("help-echo"),
            Value::string("unrelated live minibuffer help"),
        ],
    )
    .expect("seed unrelated live-buffer help");

    let presentation = PresentationId::new(4);
    {
        let frame = eval.frame_manager_mut().get_mut(frame_id).expect("frame");
        frame
            .find_window_mut(minibuffer_window)
            .expect("minibuffer window")
            .set_buffer(minibuffer_buffer);
        frame.set_window_system(Some(Value::symbol("neo")));
        frame
            .prepare_display_presentation(
                crate::window::geometry::PresentationId::new(4),
                vec![WindowPresentationSnapshot::GeometryOnly(
                    WindowDisplaySnapshot {
                        window_id: minibuffer_window,
                        regions: PresentedWindowRegions {
                            outer: Rect::new(0.0, 104.0, 400.0, 16.0),
                            text_body: Rect::new(0.0, 104.0, 400.0, 16.0),
                            ..Default::default()
                        },
                        regions_materialized: true,
                        ..Default::default()
                    },
                )],
            )
            .expect("prepare echo-area geometry");
        frame
            .activate_display_presentation(crate::window::geometry::PresentationId::new(4))
            .expect("activate echo-area geometry");
    }

    let protocol_window = DisplayWindowId::new(minibuffer_window.0 as i64);
    let hit = PresentedHitIndex::from_parts(
        presentation,
        vec![PresentedHitRegion::new(
            Some(protocol_window),
            PresentedRegionKind::TextBody,
            FrameRect::new(0.0, 104.0, 400.0, 16.0).unwrap(),
            0,
        )],
        vec![PresentedTextPosition::new(
            protocol_window,
            FrameRect::new(48.0, 104.0, 8.0, 16.0).unwrap(),
            7,
            0,
            6,
        )],
    )
    .unwrap()
    .resolve(PresentedHitQuery::new(presentation, 52.0, 112.0))
    .unwrap();
    eval.command_loop
        .keyboard
        .kboard
        .presented_mouse_observation = Some(PresentedMouseObservation {
        presentation: 4,
        hit,
        x: 52.0,
        y: 112.0,
        frame_id: frame_id.0,
    });

    assert!(
        eval.resolve_text_area_help_echo_event(frame_id, 52, 112)
            .is_none(),
        "geometry-only positions must never become live-buffer help evidence"
    );
}
