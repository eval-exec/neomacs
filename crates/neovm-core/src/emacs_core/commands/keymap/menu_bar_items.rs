//! Menu-bar item collection and the `menu-bar-menu-at-x-y` Lisp entry.
//!
//! The bodies bind frame/window state (the active keymaps of a frame, the
//! pending popup anchor), so they are R0 bodies in the core command layer
//! (STAGE4 design §5). They read evaluator state through `&Context` and the
//! keymap Value substrate from the parent module.

use crate::emacs_core::error::{EvalResult, expect_args_range};
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{Value, eq_value_swp};
use crate::tagged::SymbolMode;
use crate::window::DisplayFrameManagerExt;

pub(crate) fn builtin_menu_bar_menu_at_x_y(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("menu-bar-menu-at-x-y", &args, 2, 3)?;
    let x = args[0].as_fixnum().unwrap_or(0);
    let mut hpos = 0i64;
    let frame_id = args
        .get(2)
        .and_then(|frame| frame.as_frame_id().map(neovm_value::display_vocab::FrameId))
        .or_else(|| eval.frame_manager().selected_frame().map(|frame| frame.id));
    if let Some(anchor) = &eval.pending_menu_bar_popup_anchor
        && frame_id.is_none_or(|id| id == anchor.frame_id)
        && let Some(menu_key) = &anchor.menu_key
    {
        return Ok(Value::symbol(menu_key));
    }
    for (key, label) in menu_bar_top_level_items_for_frame(eval, frame_id) {
        let width = label.chars().count() as i64 + 1;
        if x >= hpos && x < hpos + width {
            return Ok(key);
        }
        hpos += width;
    }
    Ok(Value::NIL)
}

pub fn menu_bar_top_level_items(eval: &Context) -> Vec<(Value, String)> {
    menu_bar_top_level_items_for_frame(
        eval,
        eval.frame_manager().selected_frame().map(|frame| frame.id),
    )
}

pub(crate) fn menu_bar_top_level_items_for_frame(
    eval: &Context,
    frame_id: Option<neovm_value::display_vocab::FrameId>,
) -> Vec<(Value, String)> {
    let mut items = Vec::new();
    let active_maps = if let Some(frame_id) = frame_id {
        super::menu_bar_active_keymaps_for_frame_read_only(eval, frame_id)
    } else {
        let obey_overriding_local_maps = eval
            .builtin_var_value_by_name("overriding-local-map-menu-flag")
            .is_some_and(|value| value.is_truthy());
        let mut maps = super::current_active_maps_for_position_read_only(
            eval,
            obey_overriding_local_maps,
            None,
        )
        .unwrap_or_default();
        maps.reverse();
        maps
    };
    for keymap in active_maps {
        collect_menu_bar_items_from_map(eval, keymap, &mut items);
    }
    move_menu_bar_final_items(eval, &mut items);
    items
}

fn move_menu_bar_final_items(eval: &Context, items: &mut Vec<(Value, String)>) {
    let Some(mut final_items) = eval.builtin_var_value_by_name("menu-bar-final-items") else {
        return;
    };
    while final_items.is_cons() {
        let key = final_items.cons_car();
        if let Some(index) = items
            .iter()
            .position(|(item_key, _)| eq_value_swp(item_key, &key, eval.symbols_with_pos_enabled))
        {
            let item = items.remove(index);
            items.push(item);
        }
        final_items = final_items.cons_cdr();
    }
}

fn collect_menu_bar_items_from_map(
    eval: &Context,
    keymap: Value,
    items: &mut Vec<(Value, String)>,
) {
    let menu_bar = Value::symbol("menu-bar");
    let raw = super::list_keymap_lookup_one(&keymap, &menu_bar, eval.symbol_mode());
    let Some(menu_map) = super::maybe_keymap_in_obarray(eval.obarray(), &raw, eval.symbol_mode())
    else {
        return;
    };
    super::list_keymap_for_each_binding_recursive(
        &menu_map,
        Some(eval.obarray()),
        |key, def| {
            if items
                .iter()
                .any(|(seen, _)| eq_value_swp(seen, &key, eval.symbols_with_pos_enabled))
            {
                return;
            }
            if let Some(label) = menu_bar_label(def, eval.symbol_mode()) {
                items.push((key, label));
            }
        },
        eval.symbol_mode(),
    );
}

fn menu_bar_label(def: Value, symbol_mode: SymbolMode) -> Option<String> {
    if !def.is_cons() {
        return None;
    }
    let car = def.cons_car();
    let cdr = def.cons_cdr();
    if super::KeymapMarker::MenuItem.is_value(car, symbol_mode) && cdr.is_cons() {
        return cdr
            .cons_car()
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()));
    }
    car.as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

#[cfg(test)]
#[path = "tests/positioned_menu_item_identity_test.rs"]
mod positioned_menu_item_identity_tests;
