use super::*;

#[test]
fn issue554_positioned_keymap_markers_follow_symbol_mode() {
    let mut ctx = Context::new();
    let marker = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::symbol("keymap"), Value::fixnum(0));
    let map = Value::list(vec![marker]);
    assert!(is_list_keymap(&map, SymbolMode::Positioned));
    assert!(!is_list_keymap(&map, SymbolMode::Bare));
    assert_eq!(
        KeymapMarker::from_value(marker, SymbolMode::Positioned),
        Some(KeymapMarker::Keymap)
    );
    assert_eq!(KeymapMarker::from_value(marker, SymbolMode::Bare), None);
    assert_eq!(
        get_keymap_in_obarray(&ctx.obarray, &map, true, SymbolMode::Positioned)
            .unwrap()
            .bits(),
        map.bits()
    );
    assert!(get_keymap_in_obarray(&ctx.obarray, &map, true, SymbolMode::Bare).is_err());
}

#[test]
fn issue554_positioned_keymap_events_match_and_update_canonical_symbols() {
    let mut ctx = Context::new();
    let event = Value::symbol("f5");
    let positioned = ctx
        .tagged_heap
        .alloc_symbol_with_pos(event, Value::fixnum(0));
    let first = Value::symbol("first-command");
    let second = Value::symbol("second-command");
    let map = make_sparse_list_keymap();
    list_keymap_define(map, event, first, SymbolMode::Bare);
    assert_eq!(
        list_keymap_lookup_one(&map, &positioned, SymbolMode::Positioned),
        first
    );
    assert!(list_keymap_lookup_one(&map, &positioned, SymbolMode::Bare).is_nil());
    list_keymap_define(map, positioned, second, SymbolMode::Positioned);
    assert_eq!(
        list_keymap_lookup_one(&map, &event, SymbolMode::Bare),
        second
    );
    list_keymap_remove(map, positioned, SymbolMode::Positioned);
    assert!(list_keymap_lookup_one(&map, &event, SymbolMode::Bare).is_nil());
}

#[test]
fn issue554_positioned_keymap_designators_resolve_function_cells() {
    let mut ctx = Context::new();
    let map = make_sparse_list_keymap();
    let alias = Value::symbol("issue554-keymap-alias");
    let positioned = ctx
        .tagged_heap
        .alloc_symbol_with_pos(alias, Value::fixnum(0));
    ctx.obarray
        .set_symbol_function("issue554-keymap-alias", map);
    assert_eq!(
        get_keymap_in_obarray(&ctx.obarray, &positioned, true, SymbolMode::Positioned)
            .unwrap()
            .bits(),
        map.bits()
    );
    assert!(get_keymap_in_obarray(&ctx.obarray, &positioned, true, SymbolMode::Bare).is_err());
}

#[test]
fn issue554_positioned_minor_mode_alists_use_assq_identity() {
    let mut ctx = Context::new();
    let key = Value::symbol("issue554-mode");
    let positioned = ctx.tagged_heap.alloc_symbol_with_pos(key, Value::fixnum(0));
    let map = make_sparse_list_keymap();
    let entry = Value::cons(positioned, map);
    let alist = Value::list(vec![entry]);
    assert!(assq_in_alist(&alist, &key, SymbolMode::Positioned));
    assert!(!assq_in_alist(&alist, &key, SymbolMode::Bare));
    assert_eq!(
        minor_mode_map_entry(&entry, SymbolMode::Positioned),
        Some((intern("issue554-mode"), map))
    );
    assert!(minor_mode_map_entry(&entry, SymbolMode::Bare).is_none());
}
