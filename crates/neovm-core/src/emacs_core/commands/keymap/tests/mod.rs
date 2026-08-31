use super::*;
use crate::emacs_core::intern::{intern, resolve_sym};

#[test]
fn displayed_string_keymap_precedes_buffer_position_maps() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let frame_id = eval
        .frames
        .create_frame("displayed-string-keymap", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;

    let displayed = Value::string(" tab-one  tab-two ");
    let string_keymap = make_sparse_list_keymap();
    let command = Value::symbol("tab-line-select-tab");
    list_keymap_define_seq(
        string_keymap,
        &[Value::symbol("tab-line"), Value::symbol("down-mouse-1")],
        command,
    )
    .expect("define displayed string mouse binding");
    crate::emacs_core::textprop::builtin_put_text_property(
        &mut eval,
        vec![
            Value::fixnum(10),
            Value::fixnum(17),
            Value::symbol("keymap"),
            string_keymap,
            displayed,
        ],
    )
    .expect("put displayed string keymap");

    // GNU mouse positions carry the displayed string identity and its
    // zero-based character offset in slot 4 as (STRING . CHARPOS).
    let position = Value::list(vec![
        Value::make_window(window_id.0),
        Value::symbol("tab-line"),
        Value::cons(Value::fixnum(82), Value::fixnum(9)),
        Value::fixnum(0),
        Value::cons(displayed, Value::fixnum(14)),
        Value::NIL,
    ]);

    let maps = current_active_maps_for_position(&mut eval, true, Some(&position))
        .expect("resolve active maps for displayed string");

    assert_eq!(maps.first(), Some(&string_keymap));

    let mouse_event = Value::list(vec![Value::symbol("down-mouse-1"), position]);
    let resolved = resolve_active_key_binding(
        &mut eval,
        &[Value::symbol("tab-line"), mouse_event],
        DefaultBindingMode::Accept,
        true,
        Some(&position),
    )
    .expect("resolve displayed string mouse binding");
    assert_eq!(resolved.binding, command);
}

#[test]
fn chrome_string_without_maps_preserves_tab_line_but_clears_mode_line_maps() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let buffer_local_map = make_sparse_list_keymap();
    eval.buffers
        .set_current_local_map(buffer_local_map)
        .expect("set buffer local map");
    let frame_id = eval
        .frames
        .create_frame("chrome-string-map-fallback", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;
    let displayed = Value::string("caption");

    let position_for = |area: &str| {
        Value::list(vec![
            Value::make_window(window_id.0),
            Value::symbol(area),
            Value::cons(Value::fixnum(1), Value::fixnum(1)),
            Value::fixnum(0),
            Value::cons(displayed, Value::fixnum(0)),
            Value::NIL,
        ])
    };
    let tab_line_position = position_for("tab-line");
    let mode_line_position = position_for("mode-line");

    let tab_line_maps = current_active_maps_for_position(&mut eval, true, Some(&tab_line_position))
        .expect("resolve tab-line maps");
    let mode_line_maps =
        current_active_maps_for_position(&mut eval, true, Some(&mode_line_position))
            .expect("resolve mode-line maps");

    assert!(tab_line_maps.contains(&buffer_local_map));
    assert!(!mode_line_maps.contains(&buffer_local_map));
}

#[test]
fn keymap_marker_and_menu_item_property_domains_match_gnu_symbols() {
    assert_eq!(
        KeymapMarker::from_symbol_name("keymap"),
        Some(KeymapMarker::Keymap)
    );
    assert_eq!(
        KeymapMarker::from_symbol_name("menu-item"),
        Some(KeymapMarker::MenuItem)
    );
    assert_eq!(
        KeymapMarker::from_symbol_name("remap"),
        Some(KeymapMarker::Remap)
    );
    assert_eq!(KeymapMarker::from_symbol_name(":keymap"), None);

    for (keyword, property) in [
        (":enable", MenuItemProperty::Enable),
        (":visible", MenuItemProperty::Visible),
        (":help", MenuItemProperty::Help),
        (":filter", MenuItemProperty::Filter),
        (":button", MenuItemProperty::Button),
        (":keys", MenuItemProperty::Keys),
        (":key-sequence", MenuItemProperty::KeySequence),
        (":image", MenuItemProperty::Image),
        (":rtl", MenuItemProperty::Rtl),
        (":wrap", MenuItemProperty::Wrap),
        (":label", MenuItemProperty::Label),
        (":vert-only", MenuItemProperty::VertOnly),
    ] {
        assert_eq!(MenuItemProperty::from_keyword(keyword), Some(property));
        assert_eq!(property.keyword(), keyword);
    }

    assert_eq!(
        MenuButtonKind::from_keyword(":toggle"),
        Some(MenuButtonKind::Toggle)
    );
    assert_eq!(
        MenuButtonKind::from_keyword(":radio"),
        Some(MenuButtonKind::Radio)
    );
    assert_eq!(MenuButtonKind::from_keyword("toggle"), None);
    assert_eq!(MenuButtonKind::Toggle.keyword(), ":toggle");
    assert_eq!(MenuButtonKind::Radio.keyword(), ":radio");
}

// -- Key description parsing tests --

#[test]
fn parse_plain_char() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("a").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: 'a',
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_ctrl_x() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("C-x").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: 'x',
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_meta_x() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("M-x").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: 'x',
            ctrl: false,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_ctrl_x_ctrl_f_sequence() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("C-x C-f").unwrap();
    assert_eq!(keys.len(), 2);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: 'x',
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
    assert_eq!(
        keys[1],
        KeyEvent::Char {
            code: 'f',
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_ret() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("RET").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Function {
            name: intern("return"),
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_tab() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("TAB").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Function {
            name: intern("tab"),
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_esc_as_literal_escape_char() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("ESC").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: '\u{1b}',
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_spc() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("SPC").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: ' ',
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_combined_modifiers() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("C-M-s").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Char {
            code: 's',
            ctrl: true,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_function_key() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("f1").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Function {
            name: intern("f1"),
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_ctrl_function_key() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("C-f12").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Function {
            name: intern("f12"),
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

#[test]
fn parse_error_empty() {
    crate::test_utils::init_test_tracing();
    assert!(parse_key_description("").is_err());
}

#[test]
fn parse_error_unknown_name() {
    crate::test_utils::init_test_tracing();
    assert!(parse_key_description("foobar").is_err());
}

#[test]
fn format_key_event_roundtrip() {
    crate::test_utils::init_test_tracing();
    let cases = vec![
        "C-x", "M-x", "C-M-s", "a", "SPC", "RET", "TAB", "ESC", "f1", "C-f12",
    ];
    for desc in cases {
        let keys = parse_key_description(desc).unwrap();
        assert_eq!(keys.len(), 1, "expected single key for {}", desc);
        let formatted = format_key_event(&keys[0]);
        let reparsed = parse_key_description(&formatted).unwrap();
        assert_eq!(
            keys[0], reparsed[0],
            "roundtrip mismatch for {}: formatted as {}, reparsed as {:?}",
            desc, formatted, reparsed[0]
        );
    }
}

#[test]
fn keyboard_escape_encodes_to_emacs_escape_symbol() {
    crate::test_utils::init_test_tracing();
    let event = KeyEvent::from(crate::keyboard::KeyEvent::named(
        crate::keyboard::NamedKey::Escape,
    ));
    assert_eq!(
        key_event_to_emacs_event(&event),
        Value::symbol("escape"),
        "a named GUI Escape must remain distinct from the ASCII ESC character"
    );
}

#[test]
fn keyboard_escape_preserves_non_ctrl_modifiers_when_encoded() {
    crate::test_utils::init_test_tracing();
    let event = KeyEvent::from(crate::keyboard::KeyEvent::named_with_mods(
        crate::keyboard::NamedKey::Escape,
        crate::keyboard::Modifiers {
            shift: true,
            hyper: true,
            ..crate::keyboard::Modifiers::none()
        },
    ));
    assert_eq!(
        key_event_to_emacs_event(&event),
        Value::symbol("H-S-escape")
    );
}

#[test]
fn keyboard_return_encodes_to_emacs_return_symbol() {
    crate::test_utils::init_test_tracing();
    let event = KeyEvent::from(crate::keyboard::KeyEvent::named(
        crate::keyboard::NamedKey::Return,
    ));
    assert_eq!(
        key_event_to_emacs_event(&event),
        Value::symbol("return"),
        "physical Return should retain GNU's named GUI event identity"
    );
}

#[test]
fn keyboard_meta_return_encodes_to_emacs_modified_return_symbol() {
    crate::test_utils::init_test_tracing();
    let event = KeyEvent::from(crate::keyboard::KeyEvent::named_with_mods(
        crate::keyboard::NamedKey::Return,
        crate::keyboard::Modifiers::meta(),
    ));
    assert_eq!(
        key_event_to_emacs_event(&event),
        Value::symbol("M-return"),
        "Meta+Return should retain a modified named GUI event"
    );
}

#[test]
fn keyboard_tab_encodes_to_emacs_tab_symbol() {
    crate::test_utils::init_test_tracing();
    let event = KeyEvent::from(crate::keyboard::KeyEvent::named(
        crate::keyboard::NamedKey::Tab,
    ));
    assert_eq!(
        key_event_to_emacs_event(&event),
        Value::symbol("tab"),
        "physical Tab should retain GNU's named GUI event identity"
    );
}

#[test]
fn format_key_event_renders_gnu_control_char_names() {
    crate::test_utils::init_test_tracing();
    assert_eq!(
        format_key_event(&KeyEvent::Char {
            code: '\r',
            ctrl: false,
            meta: true,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }),
        "M-RET"
    );
    assert_eq!(
        format_key_event(&KeyEvent::Char {
            code: '\t',
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }),
        "TAB"
    );
    assert_eq!(
        format_key_event(&KeyEvent::Char {
            code: '\u{7f}',
            ctrl: false,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }),
        "DEL"
    );
}

#[test]
fn format_key_sequence_roundtrip() {
    crate::test_utils::init_test_tracing();
    let desc = "C-x C-f";
    let keys = parse_key_description(desc).unwrap();
    let formatted = format_key_sequence(&keys);
    assert_eq!(formatted, "C-x C-f");
}

#[test]
fn parse_arrow_keys() {
    crate::test_utils::init_test_tracing();
    for name in &["up", "down", "left", "right"] {
        let keys = parse_key_description(name).unwrap();
        assert_eq!(keys.len(), 1);
        match &keys[0] {
            KeyEvent::Function { name: n, .. } => assert_eq!(resolve_sym(*n), *name),
            other => panic!("expected Function for {}, got {:?}", name, other),
        }
    }
}

#[test]
fn parse_modifier_with_named_key() {
    crate::test_utils::init_test_tracing();
    let keys = parse_key_description("C-RET").unwrap();
    assert_eq!(keys.len(), 1);
    assert_eq!(
        keys[0],
        KeyEvent::Function {
            name: intern("return"),
            ctrl: true,
            meta: false,
            shift: false,
            super_: false,
            hyper: false,
            alt: false,
        }
    );
}

// -- List keymap tests --

#[test]
fn list_keymap_create_and_check() {
    crate::test_utils::init_test_tracing();
    let km = make_list_keymap();
    assert!(is_list_keymap(&km));
    let sparse = make_sparse_list_keymap();
    assert!(is_list_keymap(&sparse));
    assert!(!is_list_keymap(&Value::NIL));
    assert!(!is_list_keymap(&Value::fixnum(42)));
}

/// GNU `get_keymap` recognizes the marker with `EQ (XCAR (object),
/// Qkeymap)` (`src/keymap.c`), so an uninterned symbol that merely has the
/// same printed name is not a keymap marker.
#[test]
fn list_keymap_marker_requires_the_canonical_symbol_object() {
    crate::test_utils::init_test_tracing();
    let canonical = Value::list(vec![Value::symbol("keymap")]);
    let same_name = Value::list(vec![Value::from_sym_id(
        crate::emacs_core::intern::intern_uninterned("keymap"),
    )]);

    assert!(is_list_keymap(&canonical));
    assert!(!is_list_keymap(&same_name));
}

#[test]
fn list_keymap_define_and_lookup() {
    crate::test_utils::init_test_tracing();
    let km = make_sparse_list_keymap();
    let event = Value::symbol("return");
    list_keymap_define(km, event, Value::symbol("newline"));
    let result = list_keymap_lookup_one(&km, &event);
    assert_eq!(result.as_symbol_name(), Some("newline"));
}

/// GNU `access_keymap_1` canonicalizes the lookup index once before scanning,
/// while `store_in_keymap` canonicalizes keys when they enter a keymap.  It
/// does not repair a noncanonical alist entry manufactured directly by Lisp.
/// Consequently both written orders miss `(M-C-f1 . hit)`, while `define-key`
/// would have stored the key as `C-M-f1`.
#[test]
fn lookup_does_not_canonicalize_each_stored_alist_key() {
    crate::test_utils::init_test_tracing();
    let manual = Value::list(vec![
        Value::symbol("keymap"),
        Value::cons(Value::symbol("M-C-f1"), Value::symbol("hit")),
    ]);

    assert!(
        list_keymap_lookup_one(&manual, &Value::symbol("C-M-f1")).is_nil(),
        "GNU canonicalizes the lookup event, not a manually stored alist key"
    );
    assert!(
        list_keymap_lookup_one(&manual, &Value::symbol("M-C-f1")).is_nil(),
        "the noncanonical lookup spelling is canonicalized before the scan"
    );
}

#[test]
fn list_keymap_define_inserts_bindings_before_prompt_like_gnu() {
    crate::test_utils::init_test_tracing();
    let prompt = Value::string("Test Menu");
    let km = Value::cons(Value::symbol("keymap"), Value::cons(prompt, Value::NIL));

    list_keymap_define(km, Value::fixnum('a' as i64), Value::symbol("cmd-a"));
    list_keymap_define(km, Value::fixnum('b' as i64), Value::symbol("cmd-b"));
    list_keymap_define(km, Value::fixnum(1), Value::symbol("cmd-c"));

    let first = km.cons_cdr().cons_car();
    let second = km.cons_cdr().cons_cdr().cons_car();
    let fourth = km.cons_cdr().cons_cdr().cons_cdr().cons_cdr().cons_car();

    assert_eq!(first.cons_car(), Value::fixnum(1));
    assert_eq!(first.cons_cdr().as_symbol_name(), Some("cmd-c"));
    assert_eq!(second.cons_car(), Value::fixnum('b' as i64));
    assert_eq!(
        fourth.as_lisp_string().unwrap().as_utf8_str(),
        Some("Test Menu")
    );
}

#[test]
fn list_keymap_parent_chain() {
    crate::test_utils::init_test_tracing();
    let parent = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();
    list_keymap_set_parent(child, parent);
    assert!(is_list_keymap(&list_keymap_parent(&child)));

    // Binding in parent is found via child
    let event = Value::fixnum(97); // 'a'
    list_keymap_define(parent, event, Value::symbol("cmd-a"));
    let result = list_keymap_lookup_one(&child, &event);
    assert_eq!(result.as_symbol_name(), Some("cmd-a"));
}

#[test]
fn list_keymap_child_overrides_parent() {
    crate::test_utils::init_test_tracing();
    let parent = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();
    list_keymap_set_parent(child, parent);

    let event = Value::fixnum(120); // 'x'
    list_keymap_define(parent, event, Value::symbol("parent-cmd"));
    list_keymap_define(child, event, Value::symbol("child-cmd"));
    let result = list_keymap_lookup_one(&child, &event);
    assert_eq!(result.as_symbol_name(), Some("child-cmd"));
}

#[test]
fn list_keymap_set_parent_replaces_direct_sparse_parent_without_mutating_old_parent() {
    crate::test_utils::init_test_tracing();
    let parent_one = make_sparse_list_keymap();
    let parent_two = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();

    list_keymap_define(
        parent_one,
        Value::fixnum('a' as i64),
        Value::symbol("parent-one"),
    );
    list_keymap_define(
        parent_two,
        Value::fixnum('b' as i64),
        Value::symbol("parent-two"),
    );

    list_keymap_set_parent(child, parent_one);
    assert!(keymap_value_eq(&list_keymap_parent(&child), &parent_one));

    list_keymap_set_parent(child, parent_two);
    assert!(keymap_value_eq(&list_keymap_parent(&child), &parent_two));
    assert!(list_keymap_parent(&parent_one).is_nil());
    assert_eq!(
        list_keymap_lookup_one(&parent_one, &Value::fixnum('a' as i64)).as_symbol_name(),
        Some("parent-one")
    );
    assert!(list_keymap_lookup_one(&child, &Value::fixnum('a' as i64)).is_nil());
    assert_eq!(
        list_keymap_lookup_one(&child, &Value::fixnum('b' as i64)).as_symbol_name(),
        Some("parent-two")
    );
}

#[test]
fn list_keymap_for_each_binding_stops_before_direct_sparse_parent() {
    crate::test_utils::init_test_tracing();
    let parent = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();

    list_keymap_define(
        parent,
        Value::fixnum('a' as i64),
        Value::symbol("parent-cmd"),
    );
    list_keymap_define(child, Value::fixnum('x' as i64), Value::symbol("child-cmd"));
    list_keymap_set_parent(child, parent);

    let mut seen = Vec::new();
    list_keymap_for_each_binding(&child, None, |event, def| seen.push((event, def)));

    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Value::fixnum('x' as i64));
    assert_eq!(seen[0].1.as_symbol_name(), Some("child-cmd"));
}

#[test]
fn list_keymap_for_each_binding_reports_inline_vector_slots() {
    // GNU `map_keymap_internal` iterates an inline vector's slots as
    // char-code -> binding pairs. This scan silently lacked that arm while
    // `list_keymap_for_each_binding_recursive` -- the other copy of the same
    // taxonomy, in this very file -- had it, so a vector-stored command was
    // invisible to `where-is-internal` even though `lookup-key` found it.
    crate::test_utils::init_test_tracing();
    let mut slots = vec![Value::NIL; 128];
    slots['a' as usize] = Value::symbol("vec-cmd");
    let keymap = Value::list(vec![Value::symbol("keymap"), Value::vector(slots)]);

    let mut seen = Vec::new();
    list_keymap_for_each_binding(&keymap, None, |event, def| {
        if !def.is_nil() {
            seen.push((event, def));
        }
    });

    assert_eq!(seen.len(), 1, "seen={seen:?}");
    assert_eq!(seen[0].0, Value::fixnum('a' as i64));
    assert_eq!(seen[0].1.as_symbol_name(), Some("vec-cmd"));
}

#[test]
fn list_keymap_for_each_binding_normalizes_a_t_binding_to_nil() {
    // GNU `map_keymap_item`: a `t` binding shadows lower-precedence keymaps just
    // like an explicit nil binding, so it must be reported as nil rather than as
    // a binding of the symbol `t`.
    crate::test_utils::init_test_tracing();
    let keymap = make_sparse_list_keymap();
    list_keymap_define(keymap, Value::fixnum('u' as i64), Value::T);

    let mut seen = Vec::new();
    list_keymap_for_each_binding(&keymap, None, |event, def| seen.push((event, def)));

    assert_eq!(seen.len(), 1, "seen={seen:?}");
    assert_eq!(seen[0].0, Value::fixnum('u' as i64));
    assert!(seen[0].1.is_nil(), "t binding must arrive as nil");
}

#[test]
fn for_each_keymap_element_classifies_every_gnu_element_kind() {
    // One decode of the spine union, so no consumer can silently miss a shape:
    // prompt string, `(KEY . BINDING)` cons, inline vector, composed submap.
    crate::test_utils::init_test_tracing();
    let submap = make_sparse_list_keymap();
    list_keymap_define(submap, Value::fixnum('s' as i64), Value::symbol("sub-cmd"));
    let mut slots = vec![Value::NIL; 4];
    slots[2] = Value::symbol("vec-cmd");
    let keymap = Value::list(vec![
        Value::symbol("keymap"),
        Value::string("THE PROMPT"),
        Value::cons(Value::fixnum('c' as i64), Value::symbol("cons-cmd")),
        Value::vector(slots),
        submap,
    ]);

    let mut prompts = Vec::new();
    let mut bindings = Vec::new();
    let mut submaps = 0usize;
    for_each_keymap_element(&keymap, None, |element| match element {
        KeymapElement::Prompt(prompt) => prompts.push(prompt),
        KeymapElement::Binding { key, value } => {
            if !value.is_nil() {
                bindings.push((key, value));
            }
        }
        KeymapElement::Submap(_) => submaps += 1,
        KeymapElement::IndirectTail(_) => {}
    });

    assert_eq!(prompts.len(), 1);
    assert_eq!(
        prompts[0].as_lisp_string().unwrap().as_utf8_str(),
        Some("THE PROMPT")
    );
    assert_eq!(
        submaps, 1,
        "the composed submap must be yielded, not misread"
    );
    // The cons binding and the populated vector slot -- the submap's own binding
    // belongs to the submap, not to this level.
    let names: Vec<Option<&str>> = bindings.iter().map(|(_, v)| v.as_symbol_name()).collect();
    assert_eq!(
        names,
        vec![Some("cons-cmd"), Some("vec-cmd")],
        "{bindings:?}"
    );
}

#[test]
fn list_keymap_accessible_descends_into_direct_sparse_parent() {
    crate::test_utils::init_test_tracing();
    let parent = make_sparse_list_keymap();
    let prefix_map = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();

    list_keymap_define(parent, Value::fixnum('a' as i64), prefix_map);
    list_keymap_set_parent(child, parent);

    let mut out = Vec::new();
    list_keymap_accessible(child, &[], None, &mut out);

    // GNU `accessible-keymaps` follows the parent (via map_keymap), so the
    // parent's `a` prefix map is listed under [?a]. GNU prints ([] [97]).
    assert_eq!(out.len(), 2);
    assert_eq!(out[0].cons_car().as_vector_data().unwrap().len(), 0);
    let second_prefix = out[1].cons_car();
    let second_prefix = second_prefix.as_vector_data().unwrap();
    assert_eq!(second_prefix.len(), 1);
    assert_eq!(second_prefix[0], Value::fixnum('a' as i64));
    assert!(keymap_value_eq(&out[1].cons_cdr(), &prefix_map));
}

#[test]
fn list_keymap_copy_preserves_direct_sparse_parent_without_inlining_parent_bindings() {
    crate::test_utils::init_test_tracing();
    let parent = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();

    list_keymap_define(
        parent,
        Value::fixnum('a' as i64),
        Value::symbol("parent-cmd"),
    );
    list_keymap_define(child, Value::fixnum('x' as i64), Value::symbol("child-cmd"));
    list_keymap_set_parent(child, parent);

    let copy = list_keymap_copy(&child);

    assert!(keymap_value_eq(&list_keymap_parent(&copy), &parent));
    assert_eq!(
        list_keymap_lookup_one(&copy, &Value::fixnum('x' as i64)).as_symbol_name(),
        Some("child-cmd")
    );
    assert_eq!(
        list_keymap_lookup_one(&copy, &Value::fixnum('a' as i64)).as_symbol_name(),
        Some("parent-cmd")
    );

    let mut seen = Vec::new();
    list_keymap_for_each_binding(&copy, None, |event, def| seen.push((event, def)));
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].0, Value::fixnum('x' as i64));
}

#[test]
fn store_in_keymap_keeps_prompt_reachable_after_gnu_ordered_prepend() {
    crate::test_utils::init_test_tracing();
    let prompt = Value::string("Auxiliary keymap for Normal state");
    let map = Value::list(vec![Value::symbol("keymap"), prompt]);

    list_keymap_define(map, Value::fixnum('x' as i64), Value::symbol("foo"));

    // GNU `store_in_keymap` prepends ordinary bindings before prompt strings;
    // `keymap-prompt` still finds the prompt by scanning the keymap spine.
    let cdr = map.cons_cdr();
    assert!(cdr.is_cons(), "expected non-empty cdr after define-key");
    let head = cdr.cons_car();
    assert_eq!(head.cons_car(), Value::fixnum('x' as i64));
    assert_eq!(head.cons_cdr().as_symbol_name(), Some("foo"));
    let prompt_tail = cdr.cons_cdr();
    assert!(prompt_tail.is_cons(), "expected prompt after binding");
    assert_eq!(
        prompt_tail.cons_car().as_utf8_str(),
        Some("Auxiliary keymap for Normal state"),
        "prompt string was clobbered or replaced"
    );

    // The new binding must still exist and be reachable.
    let bound = list_keymap_lookup_one(&map, &Value::fixnum('x' as i64));
    assert_eq!(bound.as_symbol_name(), Some("foo"));
}

#[test]
fn list_keymap_event_conversion_roundtrip() {
    crate::test_utils::init_test_tracing();
    let key = KeyEvent::Char {
        code: 'x',
        ctrl: true,
        meta: false,
        shift: false,
        super_: false,
        hyper: false,
        alt: false,
    };
    let emacs_event = key_event_to_emacs_event(&key);
    let roundtrip = emacs_event_to_key_event(&emacs_event).unwrap();
    assert_eq!(key, roundtrip);
}

#[test]
fn list_keymap_lookup_seq_searches_composed_keymap_members_by_full_sequence() {
    crate::test_utils::init_test_tracing();
    let child = make_sparse_list_keymap();
    let events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &events, Value::vector(vec![Value::symbol("right")])).unwrap();

    let composed = Value::list(vec![Value::symbol("keymap"), child]);
    let binding = list_keymap_lookup_seq(&composed, &events);

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn lookup_key_in_obarray_searches_composed_keymap_members_by_full_sequence() {
    crate::test_utils::init_test_tracing();
    let obarray = crate::emacs_core::symbol::Obarray::new();
    let child = make_sparse_list_keymap();
    let events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &events, Value::vector(vec![Value::symbol("right")])).unwrap();

    let composed = Value::list(vec![Value::symbol("keymap"), child]);
    let binding = lookup_key_in_obarray(&obarray, &composed, &events, false);

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn lookup_key_in_obarray_searches_parent_composed_keymap_by_full_sequence() {
    crate::test_utils::init_test_tracing();
    let obarray = crate::emacs_core::symbol::Obarray::new();
    let child = make_sparse_list_keymap();
    let events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &events, Value::vector(vec![Value::symbol("right")])).unwrap();

    let composed_parent = Value::list(vec![Value::symbol("keymap"), child]);
    let map = Value::cons(Value::symbol("keymap"), composed_parent);
    let binding = lookup_key_in_obarray(&obarray, &map, &events, false);

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn lookup_key_in_obarray_composes_child_and_parent_prefix_maps() {
    crate::test_utils::init_test_tracing();
    let obarray = crate::emacs_core::symbol::Obarray::new();
    let child = make_sparse_list_keymap();
    let parent_member = make_sparse_list_keymap();
    let child_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('A' as i64),
    ];
    let parent_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &child_events, Value::symbol("child-up")).unwrap();
    list_keymap_define_seq(
        parent_member,
        &parent_events,
        Value::vector(vec![Value::symbol("right")]),
    )
    .unwrap();

    let composed_parent = Value::list(vec![Value::symbol("keymap"), parent_member]);
    list_keymap_set_parent(child, composed_parent);
    let binding = lookup_key_in_obarray(&obarray, &child, &parent_events, false);

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn composed_prefix_map_keeps_parent_prefix_at_next_level() {
    crate::test_utils::init_test_tracing();
    let child = make_sparse_list_keymap();
    let parent_member = make_sparse_list_keymap();
    let child_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('A' as i64),
    ];
    let parent_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &child_events, Value::symbol("child-up")).unwrap();
    list_keymap_define_seq(
        parent_member,
        &parent_events,
        Value::vector(vec![Value::symbol("right")]),
    )
    .unwrap();

    let composed_parent = Value::list(vec![Value::symbol("keymap"), parent_member]);
    list_keymap_set_parent(child, composed_parent);
    let esc_prefix = list_keymap_lookup_one(&child, &Value::fixnum(27));
    let bracket_prefix = list_keymap_lookup_one(&esc_prefix, &Value::fixnum('[' as i64));
    let binding = list_keymap_lookup_one(&bracket_prefix, &Value::fixnum('C' as i64));

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn composed_keymap_accumulates_prefix_maps_from_multiple_members() {
    crate::test_utils::init_test_tracing();
    let first_member = make_sparse_list_keymap();
    let second_member = make_sparse_list_keymap();
    let first_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('A' as i64),
    ];
    let second_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(first_member, &first_events, Value::symbol("first-up")).unwrap();
    list_keymap_define_seq(
        second_member,
        &second_events,
        Value::vector(vec![Value::symbol("right")]),
    )
    .unwrap();

    let composed = Value::list(vec![Value::symbol("keymap"), first_member, second_member]);
    let esc_prefix = list_keymap_lookup_one(&composed, &Value::fixnum(27));
    let bracket_prefix = list_keymap_lookup_one(&esc_prefix, &Value::fixnum('[' as i64));
    let binding = list_keymap_lookup_one(&bracket_prefix, &Value::fixnum('C' as i64));

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn composed_keymap_child_prefix_shadows_parent_command() {
    crate::test_utils::init_test_tracing();
    let child_member = make_sparse_list_keymap();
    let parent_member = make_sparse_list_keymap();
    let prefix_events = [Value::fixnum(3), Value::fixnum(22)];
    let child_events = [
        Value::fixnum(3),
        Value::fixnum(22),
        Value::fixnum('n' as i64),
    ];

    list_keymap_define_seq(child_member, &child_events, Value::symbol("child-command")).unwrap();
    list_keymap_define_seq(
        parent_member,
        &prefix_events,
        Value::symbol("parent-command"),
    )
    .unwrap();

    let composed = Value::list(vec![Value::symbol("keymap"), child_member, parent_member]);
    let prefix = list_keymap_lookup_seq(&composed, &prefix_events);
    assert!(is_list_keymap(&prefix));

    let binding = list_keymap_lookup_seq(&composed, &child_events);
    assert_eq!(binding.as_symbol_name(), Some("child-command"));
}

#[test]
fn lookup_key_in_obarray_composes_child_prefix_with_multi_member_parent() {
    crate::test_utils::init_test_tracing();
    let obarray = crate::emacs_core::symbol::Obarray::new();
    let child = make_sparse_list_keymap();
    let first_parent = make_sparse_list_keymap();
    let second_parent = make_sparse_list_keymap();
    let child_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('A' as i64),
    ];
    let parent_events = [
        Value::fixnum(27),
        Value::fixnum('[' as i64),
        Value::fixnum('C' as i64),
    ];
    list_keymap_define_seq(child, &child_events, Value::symbol("child-up")).unwrap();
    list_keymap_define_seq(first_parent, &child_events, Value::symbol("parent-up")).unwrap();
    list_keymap_define_seq(
        second_parent,
        &parent_events,
        Value::vector(vec![Value::symbol("right")]),
    )
    .unwrap();

    let composed_parent = Value::list(vec![Value::symbol("keymap"), first_parent, second_parent]);
    list_keymap_set_parent(child, composed_parent);
    let binding = lookup_key_in_obarray(&obarray, &child, &parent_events, false);

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn composed_prefix_map_searches_spliced_parent_tail() {
    crate::test_utils::init_test_tracing();
    let first_member = make_sparse_list_keymap();
    let parent_member = make_sparse_list_keymap();
    list_keymap_define(
        first_member,
        Value::fixnum('A' as i64),
        Value::symbol("first-up"),
    );
    list_keymap_define(
        parent_member,
        Value::fixnum('C' as i64),
        Value::vector(vec![Value::symbol("right")]),
    );

    let parent = Value::list(vec![Value::symbol("keymap"), parent_member]);
    let composed = Value::cons(
        Value::symbol("keymap"),
        Value::cons(first_member, parent.cons_cdr()),
    );
    let binding = list_keymap_lookup_one(&composed, &Value::fixnum('C' as i64));

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

#[test]
fn composed_keymap_nil_member_does_not_shadow_later_member() {
    crate::test_utils::init_test_tracing();
    let first_member = make_sparse_list_keymap();
    let second_member = make_sparse_list_keymap();
    list_keymap_define(first_member, Value::fixnum('C' as i64), Value::NIL);
    list_keymap_define(
        second_member,
        Value::fixnum('C' as i64),
        Value::vector(vec![Value::symbol("right")]),
    );

    let composed = Value::list(vec![Value::symbol("keymap"), first_member, second_member]);
    let binding = list_keymap_lookup_one(&composed, &Value::fixnum('C' as i64));

    assert_eq!(
        binding
            .as_vector_data()
            .map(|items| items[0].as_symbol_name()),
        Some(Some("right"))
    );
}

/// GNU never signals for an event position that names a buffer position
/// outside the accessible portion.
///
/// `click_position` (src/keymap.c:1639-1646) range-checks only a fixnum or a
/// marker; a cons -- an event posn -- falls back to `PT`, which cannot be out
/// of range, so its `args_out_of_range (Fcurrent_buffer (), position)` is
/// unreachable from a posn.  The posn branch of `Fcurrent_active_maps`
/// (:1727-1740) then uses `BEG <= posn-point <= Z` only to decide whether to
/// consult the `local-map`/`keymap` text properties at that position; an
/// out-of-range position simply skips them and falls back to the buffer's own
/// local map.
///
/// An inactive mini-window draws the echo area's text while it stays bound to
/// the empty ` *Minibuf-0*`, so a mouse posn over a displayed message names a
/// position past that buffer's `point-max`.  Signalling here escapes
/// `read_key_sequence` and reaches the command loop once per mouse event.
#[test]
fn event_position_past_point_max_falls_back_to_the_buffer_local_map() {
    crate::test_utils::init_test_tracing();
    let mut eval = Context::new();
    let buffer_id = eval.buffers.current_buffer().expect("current buffer").id;
    let point_max = eval
        .buffers
        .get(buffer_id)
        .expect("current buffer")
        .point_max_lisp_char_pos()
        .as_i64();
    assert!(
        point_max < 121,
        "the posn below must name a position past point-max, got point-max {point_max}"
    );
    let buffer_local_map = make_sparse_list_keymap();
    eval.buffers
        .set_current_local_map(buffer_local_map)
        .expect("set buffer local map");
    let frame_id = eval
        .frames
        .create_frame("event-position-past-point-max", 800, 600, buffer_id);
    let window_id = eval.frames.get(frame_id).expect("frame").selected_window;

    // The posn an echo-area message produces: text position 121 at column 120.
    let position = Value::list(vec![
        Value::make_window(window_id.0),
        Value::fixnum(121),
        Value::cons(Value::fixnum(1230), Value::fixnum(2)),
        Value::fixnum(0),
        Value::NIL,
        Value::fixnum(121),
        Value::cons(Value::fixnum(120), Value::fixnum(0)),
        Value::NIL,
        Value::cons(Value::fixnum(0), Value::fixnum(0)),
        Value::cons(Value::fixnum(2524), Value::fixnum(22)),
    ]);

    let maps = current_active_maps_for_position(&mut eval, true, Some(&position))
        .expect("an out-of-range event position must not signal");

    assert!(
        maps.contains(&buffer_local_map),
        "GNU falls back to the buffer's local map"
    );
}
