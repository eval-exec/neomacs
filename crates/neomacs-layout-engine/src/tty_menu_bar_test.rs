use super::*;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::keymap::{
    list_keymap_define, list_keymap_set_parent, make_sparse_list_keymap,
};
use neovm_core::heap_types::LispString;
use neovm_core::window::{SplitDirection, SplitPlacement};

#[test]
fn extract_menu_label_preserves_raw_unibyte_strings() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    let raw = Value::heap_string(LispString::from_unibyte(vec![0xFF]));
    let expected = raw
        .as_runtime_string_owned()
        .expect("runtime string for raw label");

    let plain = Value::cons(raw, Value::symbol("ignore"));
    assert_eq!(extract_menu_label(&plain), Some(expected.clone()));

    let menu_item = Value::list(vec![
        Value::symbol("menu-item"),
        raw,
        Value::symbol("ignore"),
    ]);
    assert_eq!(extract_menu_label(&menu_item), Some(expected));
}

#[test]
fn collect_from_keymap_includes_inherited_menu_bar_items() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let parent = make_sparse_list_keymap();
    let child = make_sparse_list_keymap();
    let parent_menu = make_sparse_list_keymap();
    let child_menu = make_sparse_list_keymap();

    list_keymap_define(
        parent_menu,
        Value::symbol("text"),
        Value::cons(Value::string("Text"), Value::symbol("ignore")),
    );
    list_keymap_define(
        child_menu,
        Value::symbol("org"),
        Value::cons(Value::string("Org"), Value::symbol("ignore")),
    );
    list_keymap_set_parent(child_menu, parent_menu);

    list_keymap_define(parent, Value::symbol("menu-bar"), parent_menu);
    list_keymap_define(child, Value::symbol("menu-bar"), child_menu);
    list_keymap_set_parent(child, parent);

    let mut items = Vec::new();
    collect_from_keymap(&eval, &child, &mut items);

    assert_eq!(items.len(), 2);
    assert_eq!(items[0].key, "org");
    assert_eq!(items[0].label, "Org");
    assert_eq!(items[1].key, "text");
    assert_eq!(items[1].label, "Text");
}

#[test]
fn collect_from_keymap_hides_inherited_undefined_menu_items() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let keymap = make_sparse_list_keymap();
    let parent_menu = make_sparse_list_keymap();
    let child_menu = make_sparse_list_keymap();

    for (key, label) in [
        ("headings", "Headings"),
        ("show", "Show"),
        ("hide", "Hide"),
        ("text", "Text"),
    ] {
        list_keymap_define(
            parent_menu,
            Value::symbol(key),
            Value::cons(Value::string(label), Value::symbol("ignore")),
        );
    }

    list_keymap_define(
        child_menu,
        Value::symbol("org"),
        Value::cons(Value::string("Org"), Value::symbol("ignore")),
    );
    for key in ["headings", "show", "hide"] {
        list_keymap_define(child_menu, Value::symbol(key), Value::symbol("undefined"));
    }
    list_keymap_set_parent(child_menu, parent_menu);

    list_keymap_define(keymap, Value::symbol("menu-bar"), child_menu);

    let mut items = Vec::new();
    collect_from_keymap(&eval, &keymap, &mut items);
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

    assert_eq!(labels, vec!["Org", "Text"]);
}

/// GNU builds the top-level menu from the global map first and then lets the
/// selected buffer's local map contribute.  `menu_bar_item` treats an
/// explicit `undefined` contribution as a tombstone: it removes an item that
/// an earlier active map already contributed.  Calendar uses exactly this to
/// suppress the global Edit menu.
#[test]
fn later_active_keymap_undefined_removes_earlier_menu_item() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let global_map = make_sparse_list_keymap();
    let global_menu = make_sparse_list_keymap();
    list_keymap_define(
        global_menu,
        Value::symbol("edit"),
        Value::cons(Value::string("Edit"), Value::symbol("ignore")),
    );
    list_keymap_define(global_map, Value::symbol("menu-bar"), global_menu);

    let local_map = make_sparse_list_keymap();
    let local_menu = make_sparse_list_keymap();
    list_keymap_define(
        local_menu,
        Value::symbol("edit"),
        Value::symbol("undefined"),
    );
    list_keymap_define(local_map, Value::symbol("menu-bar"), local_menu);

    let mut items = Vec::new();
    collect_from_keymap(&eval, &global_map, &mut items);
    collect_from_keymap(&eval, &local_map, &mut items);

    assert!(
        items.is_empty(),
        "local tombstone left global item: {items:?}"
    );
}

/// GNU `menu_bar_items` does not merely stable-partition final items.  It
/// walks `menu-bar-final-items` from left to right, moving each named item to
/// the end in that order.  This is observable in comint and bookmark buffers,
/// where the source keymap order differs from the requested final order.
#[test]
fn final_menu_items_follow_the_declared_order() {
    let mut eval = Context::new();
    eval.setup_thread_locals();
    eval.eval_str("(setq menu-bar-final-items '(completion inout signals help-menu))")
        .expect("set final menu order through the forwarded Lisp variable");
    assert_eq!(
        eval.obarray()
            .symbol_value("menu-bar-final-items")
            .copied()
            .expect("menu-bar-final-items is bound")
            .cons_car()
            .as_symbol_name(),
        Some("completion"),
    );
    let mut items = [
        ("file", "File"),
        ("help-menu", "Help"),
        ("completion", "Complete"),
        ("signals", "Signals"),
        ("inout", "In/Out"),
    ]
    .into_iter()
    .map(|(key, label)| TtyMenuBarItem {
        key: key.to_owned(),
        label: label.to_owned(),
        hpos: 0,
    })
    .collect();

    move_final_items_to_end(&eval, &mut items);

    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();
    assert_eq!(labels, ["File", "Complete", "In/Out", "Signals", "Help"]);
}

#[test]
fn collect_from_keymap_descends_embedded_menu_bar_keymaps() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let keymap = make_sparse_list_keymap();
    let org_group = make_sparse_list_keymap();
    let text_group = make_sparse_list_keymap();

    list_keymap_define(
        org_group,
        Value::symbol("org"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Org"),
            make_sparse_list_keymap(),
        ]),
    );
    list_keymap_define(
        org_group,
        Value::symbol("table"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Table"),
            make_sparse_list_keymap(),
        ]),
    );
    list_keymap_define(
        text_group,
        Value::symbol("text"),
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string("Text"),
            make_sparse_list_keymap(),
        ]),
    );

    let menu_bar = Value::list(vec![Value::symbol("keymap"), org_group, text_group]);
    list_keymap_define(keymap, Value::symbol("menu-bar"), menu_bar);

    let mut items = Vec::new();
    collect_from_keymap(&eval, &keymap, &mut items);
    let labels: Vec<_> = items.iter().map(|item| item.label.as_str()).collect();

    assert_eq!(labels, vec!["Table", "Org", "Text"]);
}

#[test]
fn collect_tty_menu_bar_items_uses_selected_window_local_map() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let selected_buffer = eval.buffer_manager_mut().create_buffer("selected");
    let frame_id =
        eval.frame_manager_mut()
            .create_frame("menu-local-map", 800, 600, selected_buffer);
    let local_map = make_sparse_list_keymap();
    let local_menu = make_sparse_list_keymap();
    list_keymap_define(
        local_menu,
        Value::symbol("mode-menu"),
        Value::cons(Value::string("Mode Menu"), Value::symbol("ignore")),
    );
    list_keymap_define(local_map, Value::symbol("menu-bar"), local_menu);
    eval.buffer_manager_mut()
        .set_buffer_local_map(selected_buffer, local_map)
        .expect("set selected buffer local map");

    let labels: Vec<_> = collect_tty_menu_bar_items_for_frame(&eval, frame_id)
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert!(
        labels.iter().any(|label| label == "Mode Menu"),
        "{labels:?}"
    );
}

#[test]
fn menu_bar_item_cache_tracks_temporary_window_selection() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let buffer_a = eval.buffer_manager_mut().create_buffer("selection-a");
    let buffer_b = eval.buffer_manager_mut().create_buffer("selection-b");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("menu-selection", 800, 600, buffer_a);
    let window_a = eval
        .frame_manager()
        .get(frame_id)
        .expect("frame")
        .selected_window;
    let window_b = eval
        .frame_manager_mut()
        .split_window(
            frame_id,
            window_a,
            SplitDirection::Vertical,
            buffer_b,
            None,
            SplitPlacement::AfterTarget,
        )
        .expect("split window");

    for (buffer, key, label) in [
        (buffer_a, "selection-a-menu", "Selection A"),
        (buffer_b, "selection-b-menu", "Selection B"),
    ] {
        let local_map = make_sparse_list_keymap();
        let local_menu = make_sparse_list_keymap();
        list_keymap_define(
            local_menu,
            Value::symbol(key),
            Value::cons(Value::string(label), Value::symbol("ignore")),
        );
        list_keymap_define(local_map, Value::symbol("menu-bar"), local_menu);
        eval.buffer_manager_mut()
            .set_buffer_local_map(buffer, local_map)
            .expect("set buffer local map");
    }

    let labels = |eval: &Context| -> Vec<String> {
        collect_tty_menu_bar_items_for_frame(eval, frame_id)
            .into_iter()
            .map(|item| item.label)
            .collect()
    };
    assert_eq!(labels(&eval), vec!["Selection A".to_string()]);

    // GNU `select_window` marks the non-selected old or new window for
    // redisplay even when NORECORD is non-nil.  That raises
    // `windows_or_buffers_changed`, so a temporary `with-selected-window`
    // body and its restoration each rebuild the frame menu for the window
    // selected at the next redisplay.
    eval.eval_form(Value::list(vec![
        Value::symbol("select-window"),
        Value::make_window(window_b.0),
        Value::T,
    ]))
    .expect("temporarily select second window");
    assert_eq!(labels(&eval), vec!["Selection B".to_string()]);

    eval.eval_form(Value::list(vec![
        Value::symbol("select-window"),
        Value::make_window(window_a.0),
        Value::T,
    ]))
    .expect("restore first window");
    assert_eq!(labels(&eval), vec!["Selection A".to_string()]);
}

/// GNU's frame item cache observes redisplay invalidation, not the raw identity
/// or contents of the active maps.  A map mutation can therefore retain the
/// previous menu until an update-mode-lines / windows-or-buffers-changed
/// trigger asks `update_menu_bar` to rebuild it.  GNU's public
/// `set-window-buffer` records an ordinary window-buffer transition, but GNU
/// prepares menus before it promotes that frame flag during redisplay.  A
/// selected-window buffer switch therefore retains the prepared menu unless a
/// separate rebuild predicate is already pending.  A bare mutation of the
/// active keymap within one buffer is cached for the same reason.
#[test]
fn menu_bar_item_cache_rebuilds_only_at_the_redisplay_invalidation_boundary() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let buffer_a = eval.buffer_manager_mut().create_buffer("cache-a");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("menu-cache", 800, 600, buffer_a);

    let local_map = make_sparse_list_keymap();
    let local_menu = make_sparse_list_keymap();
    list_keymap_define(
        local_menu,
        Value::symbol("first-menu"),
        Value::cons(Value::string("First"), Value::symbol("ignore")),
    );
    list_keymap_define(local_map, Value::symbol("menu-bar"), local_menu);
    eval.buffer_manager_mut()
        .set_buffer_local_map(buffer_a, local_map)
        .expect("set local map");

    let labels = |eval: &Context| -> Vec<String> {
        collect_tty_menu_bar_items_for_frame(eval, frame_id)
            .into_iter()
            .map(|item| item.label)
            .collect()
    };

    assert_eq!(labels(&eval), vec!["First".to_string()]);
    // Second collection with nothing changed: the cached path must agree
    // with the walked path.
    assert_eq!(labels(&eval), vec!["First".to_string()]);

    // A bare keymap mutation does not cross GNU's menu-bar invalidation
    // boundary, even though the next rebuild must observe it.
    list_keymap_define(
        local_menu,
        Value::symbol("second-menu"),
        Value::cons(Value::string("Second"), Value::symbol("ignore")),
    );
    eval.eval_str("(message \"ordinary redisplay is not a menu rebuild\")")
        .expect("invalidate general redisplay without crossing the menu boundary");
    assert_eq!(labels(&eval), vec!["First".to_string()]);

    eval.eval_str("(force-mode-line-update)")
        .expect("an undisplayed current buffer cannot request a local update");
    assert_eq!(labels(&eval), vec!["First".to_string()]);

    // GNU buffer.c gates the local request on buffer_window_count of the
    // CURRENT buffer, not the selected window's buffer. Creating a frame via
    // the Rust manager above does not switch the evaluator out of *scratch*.
    eval.buffer_manager_mut().set_current(buffer_a);
    eval.eval_str("(force-mode-line-update)")
        .expect("cross the GNU update-mode-lines boundary");
    assert_eq!(
        labels(&eval),
        vec!["Second".to_string(), "First".to_string()]
    );

    // Buffer identity is not itself a cache key.  GNU's public
    // set-window-buffer operation records FRAME_WINDOW_CHANGE for an ordinary
    // window, but prepare_menu_bars runs before that flag is promoted.  The
    // selected window therefore keeps the already-prepared menu here.
    let buffer_b = eval.buffer_manager_mut().create_buffer("cache-b");
    let other_map = make_sparse_list_keymap();
    let other_menu = make_sparse_list_keymap();
    list_keymap_define(
        other_menu,
        Value::symbol("other-menu"),
        Value::cons(Value::string("Other"), Value::symbol("ignore")),
    );
    list_keymap_define(other_map, Value::symbol("menu-bar"), other_menu);
    eval.buffer_manager_mut()
        .set_buffer_local_map(buffer_b, other_map)
        .expect("set other local map");

    eval.eval_str("(set-window-buffer (selected-window) \"cache-b\")")
        .expect("switch selected window to other buffer");
    assert_eq!(
        labels(&eval),
        vec!["Second".to_string(), "First".to_string()]
    );

    eval.buffer_manager_mut().set_current(buffer_b);
    eval.eval_str("(force-mode-line-update)")
        .expect("cross a later GNU update-mode-lines boundary");
    assert_eq!(labels(&eval), vec!["Other".to_string()]);

    // GNU's third predicate is `window_buffer_changed`: whether the selected
    // buffer's modified-star state differs from what the window last showed.
    // Model that state explicitly, without turning buffer identity into an
    // eager cache key.
    eval.buffer_manager_mut()
        .get_mut(buffer_b)
        .expect("other buffer")
        .insert("modified");
    assert_eq!(labels(&eval), vec!["Other".to_string()]);
}

/// GNU `set-buffer-modified-p` delegates to `force-mode-line-update`, but
/// `Fforce_mode_line_update` only raises `update_mode_lines` for its local
/// form when the current buffer is already displayed (`buffer.c`).  The About
/// screen is constructed in an undisplayed buffer before `switch-to-buffer`;
/// that construction must therefore not discard the frame's prepared menu.
#[test]
fn offscreen_buffer_modified_state_does_not_rebuild_the_prepared_menu() {
    let mut eval = Context::new();
    eval.setup_thread_locals();

    let visible = eval.buffer_manager_mut().create_buffer("visible-menu");
    let frame_id = eval
        .frame_manager_mut()
        .create_frame("menu-cache", 800, 600, visible);
    let visible_map = make_sparse_list_keymap();
    let visible_menu = make_sparse_list_keymap();
    list_keymap_define(
        visible_menu,
        Value::symbol("visible-menu"),
        Value::cons(Value::string("Visible"), Value::symbol("ignore")),
    );
    list_keymap_define(visible_map, Value::symbol("menu-bar"), visible_menu);
    eval.buffer_manager_mut()
        .set_buffer_local_map(visible, visible_map)
        .expect("set visible local map");

    let labels = |eval: &Context| -> Vec<String> {
        collect_tty_menu_bar_items_for_frame(eval, frame_id)
            .into_iter()
            .map(|item| item.label)
            .collect()
    };
    assert_eq!(labels(&eval), vec!["Visible".to_string()]);

    let offscreen = eval.buffer_manager_mut().create_buffer("offscreen-about");
    let offscreen_map = make_sparse_list_keymap();
    let offscreen_menu = make_sparse_list_keymap();
    list_keymap_define(
        offscreen_menu,
        Value::symbol("offscreen-menu"),
        Value::cons(Value::string("Offscreen"), Value::symbol("ignore")),
    );
    list_keymap_define(offscreen_map, Value::symbol("menu-bar"), offscreen_menu);
    eval.buffer_manager_mut()
        .set_buffer_local_map(offscreen, offscreen_map)
        .expect("set offscreen local map");

    eval.eval_str(
        r##"(progn
               (set-buffer "offscreen-about")
               (set-buffer-modified-p nil)
               (set-window-buffer (selected-window) (current-buffer)))"##,
    )
    .expect("construct offscreen buffer, then display it");

    assert_eq!(labels(&eval), vec!["Visible".to_string()]);
}
