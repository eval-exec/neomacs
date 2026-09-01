//! TTY menu bar item collection.
//!
//! Mirrors GNU Emacs `keyboard.c:menu_bar_items` (around line 8605):
//! walks the active keymaps' `[menu-bar]` prefix, collects each
//! top-level item label, and reorders so items in `menu-bar-final-items`
//! (typically `(help-menu)`) move to the end of the list. The result is
//! handed to `display_menu_bar` (`xdisp.c:27444`) for rasterization.
//!
//! Neomacs implementation differences from GNU:
//!
//! * GNU stores items in the frame's `menu_bar_items_vector` with four
//!   slots per item (key, string, def, hpos).  We carry only the
//!   user-visible fields (label + key) in a `Vec<TtyMenuBarItem>` since
//!   the TTY rasterizer doesn't need `def`, and `hpos` is computed on
//!   the way out by the rasterizer.
//! * Menu items whose label resolves to nil or whose definition is nil
//!   are skipped, mirroring `menu_bar_item`'s `STRINGP (string)` /
//!   `CONSP (def)` guards.  The key still counts as seen while walking
//!   parent keymaps, matching GNU's `keymap-canonicalize`: a child
//!   binding of `undefined` hides an inherited menu entry.
//! * The active maps are computed from the selected window's buffer, not
//!   the evaluator's current buffer.  GNU's menu-bar redisplay path
//!   (`xmenu.c`/`pgtkmenu.c`) temporarily selects the frame's selected
//!   window buffer before calling `menu_bar_items`.

use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyMenuBarItem {
    pub label: String,
    pub key: String,
    pub hpos: u16,
}
use neovm_core::emacs_core::Context;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::keymap::{
    KeymapMarker, list_keymap_for_each_binding_recursive, list_keymap_lookup_one_unresolved,
    list_keymap_parent, menu_bar_active_keymaps_for_frame_read_only,
    menu_bar_active_keymaps_read_only,
};
use neovm_core::window::FrameId;
use std::cell::RefCell;

/// Per-frame cache of collected menu-bar items.
///
/// A typing profile put `collect_tty_menu_bar_items_for_frame` at 22.8% of
/// the whole session: the full active-keymap walk ran on EVERY redisplay,
/// per keystroke, for a menu bar whose content essentially never changes.
/// GNU caches the result on the frame (`fset_menu_bar_items`, xdisp.c
/// `update_menu_bar`) and recomputes only on `windows_or_buffers_changed`
/// / `update_mode_lines` / `window_buffer_changed`.
///
/// Our key is those triggers translated, plus one GNU lacks:
///
/// * `redisplay_generation` — the `update_mode_lines` family
///   (`force-mode-line-update`, which `define-minor-mode` always calls;
///   display-variable writes; media changes);
/// * the identity bits of the ACTIVE maps in order — catches buffer and
///   window switches, `use-local-map`, global-map replacement, and
///   minor-mode toggles that change the active-map list, all without any
///   flag needing to fire;
/// * `keymap_mutation_epoch` — `define-key`/`set-keymap-parent` interior
///   mutations, which GNU's cache misses until the next broad trigger;
/// * `menu-bar-final-items` value bits, read by the reorder step;
/// * `context_instance_id` — refuses entries from a previous evaluator
///   (tests create many per thread; generations restart and heap
///   addresses recycle).
///
/// Remaining staleness (raw `setcdr` surgery on a keymap's interior)
/// matches GNU's own cache exactly and is resolved by the same triggers.
/// Items are plain data (`String`s + `u16`) — no Lisp values, so the
/// cache is invisible to GC by construction, not by accident.
struct MenuBarItemsCache {
    context_id: u64,
    frame_bits: u64,
    generation: u64,
    keymap_epoch: neovm_core::emacs_core::keymap::KeymapMutationEpoch,
    inputs_key: u64,
    items: Vec<TtyMenuBarItem>,
}

thread_local! {
    static MENU_BAR_ITEMS_CACHE: RefCell<Vec<MenuBarItemsCache>> =
        const { RefCell::new(Vec::new()) };
}

/// Fold the identity bits of every cache input into one key (FNV-1a).
fn menu_bar_inputs_key(eval: &Context, maps: &[Value]) -> u64 {
    let mut key: u64 = 0xcbf2_9ce4_8422_2325;
    let mut fold = |bits: u64| {
        key = (key ^ bits).wrapping_mul(0x0000_0100_0000_01b3);
    };
    for map in maps {
        fold(map.bits() as u64);
    }
    fold(
        eval.obarray()
            .symbol_value("menu-bar-final-items")
            .map(|v| v.bits() as u64)
            .unwrap_or(0),
    );
    key
}

/// Walk the active `[menu-bar]` keymap(s) and return the items to draw.
///
/// Returns an empty vec if there is no menu bar (e.g. `global-map` has
/// no `[menu-bar]` binding) or if the binding doesn't resolve to a
/// keymap.  The returned items are in display order (left-to-right),
/// with `menu-bar-final-items` (default: `help-menu`) moved to the end
/// like GNU `keyboard.c:8697-8716`.
///
/// Walks the selected window's active menu-bar maps in GNU's display
/// collection order.
pub fn collect_tty_menu_bar_items(eval: &Context) -> Vec<TtyMenuBarItem> {
    let mut items: Vec<TtyMenuBarItem> = Vec::new();

    for keymap in menu_bar_active_keymaps_read_only(eval) {
        collect_from_keymap(eval, &keymap, &mut items);
    }

    move_final_items_to_end(eval, &mut items);
    items
}

pub fn collect_tty_menu_bar_items_for_frame(
    eval: &Context,
    frame_id: FrameId,
) -> Vec<TtyMenuBarItem> {
    // Resolving the ACTIVE maps is cheap; the recursive walk below is what
    // cost 22.8% of a typing session when run per redisplay. Key the cache
    // on the walk's inputs (see `MenuBarItemsCache`) and skip the walk when
    // none have changed.
    let maps = menu_bar_active_keymaps_for_frame_read_only(eval, frame_id);
    let context_id = eval.context_instance_id();
    let frame_bits = frame_id.0;
    let generation = eval.redisplay_generation();
    let keymap_epoch = neovm_core::emacs_core::keymap::keymap_mutation_epoch();
    let inputs_key = menu_bar_inputs_key(eval, &maps);

    let cached = MENU_BAR_ITEMS_CACHE.with(|cache| {
        cache.borrow().iter().find_map(|entry| {
            (entry.context_id == context_id
                && entry.frame_bits == frame_bits
                && entry.generation == generation
                && entry.keymap_epoch == keymap_epoch
                && entry.inputs_key == inputs_key)
                .then(|| entry.items.clone())
        })
    });
    if let Some(items) = cached {
        return items;
    }

    let mut items: Vec<TtyMenuBarItem> = Vec::new();
    for keymap in &maps {
        collect_from_keymap(eval, keymap, &mut items);
    }
    move_final_items_to_end(eval, &mut items);

    MENU_BAR_ITEMS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|entry| entry.context_id == context_id && entry.frame_bits != frame_bits);
        cache.push(MenuBarItemsCache {
            context_id,
            frame_bits,
            generation,
            keymap_epoch,
            inputs_key,
            items: items.clone(),
        });
    });
    items
}

/// Walk a single keymap looking for its `[menu-bar]` sub-keymap and
/// append any new items into `items` (deduping by key).
fn collect_from_keymap(eval: &Context, keymap: &Value, items: &mut Vec<TtyMenuBarItem>) {
    let menu_bar_sym = Value::symbol("menu-bar");
    let raw_binding = list_keymap_lookup_one_unresolved(keymap, &menu_bar_sym);
    if raw_binding.is_nil() {
        return;
    }

    let menu_bar_keymap = match resolve_keymap(eval, &raw_binding) {
        Some(km) => km,
        None => return,
    };

    let mut current = menu_bar_keymap;
    let mut seen_keys = HashSet::new();
    for _ in 0..64 {
        collect_menu_bar_keymap_bindings(&current, items, &mut seen_keys);
        let parent = list_keymap_parent(&current);
        if !is_keymap(&parent) {
            break;
        }
        current = parent;
    }
}

fn collect_menu_bar_keymap_bindings(
    menu_bar_keymap: &Value,
    items: &mut Vec<TtyMenuBarItem>,
    seen_keys: &mut HashSet<String>,
) {
    // No obarray in the layout engine: a spine tail that merely NAMES a keymap
    // stays unresolved here (GNU would follow it). Menu-bar keymaps are built by
    // `define-key` and do not use that shape.
    list_keymap_for_each_binding_recursive(menu_bar_keymap, None, |key, def| {
        let key_str = key_symbol_name(&key);
        if seen_keys.insert(key_str.clone())
            && let Some(label) = extract_menu_label(&def)
        {
            // Dedup-by-key: GNU's `menu_bar_item` calls `Fmemq (key,
            // menu_bar_one_keymap_changed_items)` to skip a key it has
            // already seen for the *current* keymap. Here we apply the
            // same idea across the union of keymaps so that a major
            // mode that re-binds an existing top-level menu (rare)
            // doesn't produce a duplicate label. The first occurrence
            // wins, mirroring the natural reverse-insertion-order walk
            // (newest binding first within each map).
            if !items.iter().any(|item| item.key == key_str) {
                items.push(TtyMenuBarItem {
                    label,
                    key: key_str,
                    hpos: 0,
                });
            }
        }
    });
}

/// Resolve a keymap reference: either a `(keymap ...)` cons or a symbol
/// whose value is such a cons.
fn resolve_keymap(eval: &Context, value: &Value) -> Option<Value> {
    if is_keymap(value) {
        return Some(*value);
    }
    if let Some(name) = value.as_symbol_name()
        && let Some(symbol_value) = eval.obarray().symbol_value(name)
        && is_keymap(symbol_value)
    {
        return Some(*symbol_value);
    }
    None
}

/// Reorder `items` so that any whose key matches an entry in
/// `menu-bar-final-items` is moved to the end of the list, preserving
/// relative order. Mirrors `keyboard.c:8697-8716`.
fn move_final_items_to_end(eval: &Context, items: &mut Vec<TtyMenuBarItem>) {
    let final_items = match eval.obarray().symbol_value("menu-bar-final-items") {
        Some(value) => *value,
        None => return,
    };
    if final_items.is_nil() {
        return;
    }

    // Collect the symbol names referenced by `menu-bar-final-items`.
    let mut tail = final_items;
    let mut final_keys: Vec<String> = Vec::new();
    while tail.is_cons() {
        let head = tail.cons_car();
        if let Some(name) = head.as_symbol_name() {
            final_keys.push(name.to_string());
        }
        tail = tail.cons_cdr();
    }
    if final_keys.is_empty() {
        return;
    }

    // Stable partition: keep non-final items first, then final items.
    let mut non_final: Vec<TtyMenuBarItem> = Vec::with_capacity(items.len());
    let mut moved: Vec<TtyMenuBarItem> = Vec::new();
    for item in items.drain(..) {
        if final_keys.iter().any(|k| k == &item.key) {
            moved.push(item);
        } else {
            non_final.push(item);
        }
    }
    *items = non_final;
    items.extend(moved);
}

/// Extract the user-visible label from a menu-bar binding.
///
/// Handles the two shapes GNU's menu_bar_item recognises:
///
/// * `(STRING . CMD-OR-SUBMAP)` — simple binding from
///   `(define-key map [menu-bar foo] (cons "Foo" cmd))`. Label is `STRING`.
/// * `(menu-item LABEL CMD …)` — extended menu-item form. Label is `LABEL`,
///   which can be a string or a Lisp form that evaluates to a string. We
///   only honour string labels for the MVP; deferred Lisp evaluation of
///   dynamic labels is a TODO matching GNU's `Feval (label, ...)` path.
fn extract_menu_label(def: &Value) -> Option<String> {
    if !def.is_cons() {
        return None;
    }
    let car = def.cons_car();
    let cdr = def.cons_cdr();

    // (menu-item LABEL ...)
    if KeymapMarker::MenuItem.is_value(car) && cdr.is_cons() {
        let label = cdr.cons_car();
        if let Some(s) = label.as_runtime_string_owned() {
            return Some(s);
        }
        return None;
    }

    // (STRING . CMD-OR-SUBMAP)
    if let Some(s) = car.as_runtime_string_owned() {
        return Some(s);
    }

    None
}

/// True if `value` looks like a keymap (`(keymap ...)`).
fn is_keymap(value: &Value) -> bool {
    if !value.is_cons() {
        return false;
    }
    KeymapMarker::Keymap.is_value(value.cons_car())
}

/// Render a menu-bar key value as a printable identifier.
fn key_symbol_name(key: &Value) -> String {
    if let Some(name) = key.as_symbol_name() {
        return name.to_string();
    }
    format!("{:?}", key)
}

#[cfg(test)]
#[path = "tty_menu_bar_test.rs"]
mod tests;
