use super::*;
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::keymap::{
    list_keymap_define, list_keymap_set_parent, make_sparse_list_keymap,
};
use neovm_core::heap_types::LispString;

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

/// The per-frame item cache must never serve stale content: a `define-key`
/// that adds a menu-bar entry, or a switch to a buffer with a different
/// local map, must be visible on the very next collection. (GNU's frame
/// cache tolerates define-key staleness until the next broad redisplay
/// trigger; ours keys on the keymap mutation epoch, so it must be stricter.)
#[test]
fn menu_bar_item_cache_invalidates_on_keymap_mutation_and_buffer_switch() {
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

    // Interior mutation through the low-level store (the chokepoint that
    // bumps the keymap mutation epoch; builtin define-key funnels here too):
    // the new entry must appear immediately.
    list_keymap_define(
        local_menu,
        Value::symbol("second-menu"),
        Value::cons(Value::string("Second"), Value::symbol("ignore")),
    );
    // Sparse-keymap define PREPENDS, so the newer binding walks first --
    // the invariant under test is presence, immediately, not order.
    assert_eq!(
        labels(&eval),
        vec!["Second".to_string(), "First".to_string()]
    );

    // A different buffer with a different local map on ANOTHER frame: the
    // per-frame keying and the active-map identity bits must keep the two
    // frames' items independent, with no epoch or generation movement.
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
    let frame_b = eval
        .frame_manager_mut()
        .create_frame("menu-cache-b", 800, 600, buffer_b);
    let other_labels: Vec<String> = collect_tty_menu_bar_items_for_frame(&eval, frame_b)
        .into_iter()
        .map(|item| item.label)
        .collect();
    assert_eq!(other_labels, vec!["Other".to_string()]);
    // And frame A's cached items are untouched by frame B's collection.
    assert_eq!(
        labels(&eval),
        vec!["Second".to_string(), "First".to_string()]
    );
}
