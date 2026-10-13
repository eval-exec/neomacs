use super::*;
use crate::emacs_core::eval::Context;

#[test]
fn positioned_menu_keys_deduplicate_and_reorder_by_eq_policy() {
    let mut ctx = Context::new();
    let first = Value::symbol("issue554-menu-first");
    let second = Value::symbol("issue554-menu-second");
    let positioned = ctx
        .tagged_heap
        .alloc_symbol_with_pos(first, Value::fixnum(0));
    ctx.set_variable("menu-bar-final-items", Value::list(vec![positioned]));
    let original = vec![(first, "First".to_string()), (second, "Second".to_string())];
    let mut items = original.clone();
    ctx.set_variable("symbols-with-pos-enabled", Value::NIL);
    move_menu_bar_final_items(&ctx, &mut items);
    assert_eq!(items[0].0.bits(), first.bits());
    ctx.set_variable("symbols-with-pos-enabled", Value::T);
    move_menu_bar_final_items(&ctx, &mut items);
    assert_eq!(items[0].0.bits(), second.bits());

    let definition = |label| {
        Value::list(vec![
            Value::symbol("menu-item"),
            Value::string(label),
            Value::symbol("ignore"),
        ])
    };
    let menu = Value::list(vec![
        Value::symbol("keymap"),
        Value::cons(positioned, definition("First")),
        Value::cons(first, definition("Duplicate")),
    ]);
    let map = Value::list(vec![
        Value::symbol("keymap"),
        Value::cons(Value::symbol("menu-bar"), menu),
    ]);
    let mut collected = Vec::new();
    collect_menu_bar_items_from_map(&ctx, map, &mut collected);
    assert_eq!(collected.len(), 1);
    assert_eq!(collected[0].0.bits(), positioned.bits());
}
