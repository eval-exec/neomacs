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
use neovm_core::emacs_core::MenuBarRebuildGeneration;
use neovm_core::emacs_core::Value;
use neovm_core::emacs_core::keymap::{
    KeymapMarker, list_keymap_for_each_binding_recursive, list_keymap_lookup_one_unresolved,
    list_keymap_parent, menu_bar_active_keymaps_for_frame_read_only,
    menu_bar_active_keymaps_read_only,
};
use neovm_core::window::FrameId;
use std::cell::RefCell;

/// Identity of the evaluator that owns a frame menu cache.
///
/// Test processes construct many evaluators on one thread.  Giving the raw
/// process-unique number a domain type prevents it from being confused with
/// the independently restarting menu-rebuild generation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuBarContextId(u64);

/// GNU `update_menu_bar`'s rebuild predicate, sampled as a cache key.
///
/// `rebuild` models `windows_or_buffers_changed || update_mode_lines`;
/// `modified_indicator` models `window_buffer_changed (w)`. `update_tool_bar`
/// tests the same predicate plus `w->update_mode_line`, so the tool bar keys
/// its cache on this value too (`gui_chrome.rs`).
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct MenuBarCacheKey {
    pub(crate) context: MenuBarContextId,
    pub(crate) frame: FrameId,
    rebuild: MenuBarRebuildGeneration,
    modified_indicator: MenuBarModifiedIndicator,
}

impl MenuBarCacheKey {
    pub(crate) fn capture(eval: &Context, frame: FrameId) -> Self {
        Self {
            context: MenuBarContextId(eval.context_instance_id()),
            frame,
            rebuild: eval.menu_bar_rebuild_generation(),
            modified_indicator: MenuBarModifiedIndicator::for_frame(eval, frame),
        }
    }
}

/// The selected window's mode-line `*` state sampled by GNU
/// `window_buffer_changed` (`xdisp.c:13820-13827`).
///
/// This is intentionally not buffer identity: switching between two saved
/// buffers does not by itself cross this particular menu-rebuild boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum MenuBarModifiedIndicator {
    Saved,
    Modified,
}

impl MenuBarModifiedIndicator {
    fn for_frame(eval: &Context, frame: FrameId) -> Self {
        let modified = eval
            .frame_manager()
            .get(frame)
            .and_then(|frame| frame.selected_window())
            .and_then(|window| window.buffer_id())
            .and_then(|buffer| eval.buffer_manager().get(buffer))
            .is_some_and(|buffer| buffer.is_modified());
        if modified {
            Self::Modified
        } else {
            Self::Saved
        }
    }
}

/// Per-frame cache of collected menu-bar items.
///
/// A typing profile put the recursive active-keymap walk at 22.8% of a typing
/// session.  GNU avoids that work by retaining `menu_bar_items_vector` on the
/// frame until `update_menu_bar` crosses its invalidation boundary.  This
/// cache has the same observable boundary: the dedicated rebuild generation
/// models `windows_or_buffers_changed || update_mode_lines`, while the typed
/// modified indicator models `window_buffer_changed`. Keymap identities,
/// mutations and `menu-bar-final-items` are sampled only while rebuilding,
/// never used as eager cache keys.
///
/// Items are plain owned data (`String`s + `u16`), so the cache is invisible
/// to Lisp GC by construction.
struct MenuBarItemsCache {
    key: MenuBarCacheKey,
    items: Vec<TtyMenuBarItem>,
}

thread_local! {
    static MENU_BAR_ITEMS_CACHE: RefCell<Vec<MenuBarItemsCache>> =
        const { RefCell::new(Vec::new()) };
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
    let key = MenuBarCacheKey::capture(eval, frame_id);

    let cached = MENU_BAR_ITEMS_CACHE.with(|cache| {
        cache
            .borrow()
            .iter()
            .find_map(|entry| (entry.key == key).then(|| entry.items.clone()))
    });
    if let Some(items) = cached {
        return items;
    }

    let maps = menu_bar_active_keymaps_for_frame_read_only(eval, frame_id);
    let mut items: Vec<TtyMenuBarItem> = Vec::new();
    for keymap in &maps {
        collect_from_keymap(eval, keymap, &mut items);
    }
    move_final_items_to_end(eval, &mut items);

    MENU_BAR_ITEMS_CACHE.with(|cache| {
        let mut cache = cache.borrow_mut();
        cache.retain(|entry| entry.key.context == key.context && entry.key.frame != key.frame);
        cache.push(MenuBarItemsCache {
            key,
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
        if !seen_keys.insert(key_str.clone()) {
            return;
        }

        match MenuBarBindingContribution::from_definition(&def) {
            MenuBarBindingContribution::Suppress => {
                // GNU `menu_bar_item` removes a contribution from an earlier
                // active map when a later map explicitly binds the same key
                // to `undefined` (keyboard.c).  Calendar relies on this to
                // hide the global Edit menu.
                items.retain(|item| item.key != key_str);
            }
            MenuBarBindingContribution::Item { label } => {
                // Active maps are walked global-first, like GNU.  A later
                // valid definition augments the existing menu's map list but
                // retains the first user-visible label.
                if !items.iter().any(|item| item.key == key_str) {
                    items.push(TtyMenuBarItem {
                        label,
                        key: key_str,
                        hpos: 0,
                    });
                }
            }
            MenuBarBindingContribution::NoItem => {}
        }
    });
}

/// The three semantically distinct outcomes of GNU's `menu_bar_item` parser.
///
/// Keeping suppression separate from an unparseable/non-menu binding matters:
/// only the literal `undefined` tombstone removes an item contributed by an
/// earlier active map.  An ordinary non-menu binding merely contributes no
/// item of its own.
#[derive(Debug, Clone, PartialEq, Eq)]
enum MenuBarBindingContribution {
    Suppress,
    Item { label: String },
    NoItem,
}

impl MenuBarBindingContribution {
    fn from_definition(definition: &Value) -> Self {
        if definition.as_symbol_name() == Some("undefined") {
            Self::Suppress
        } else if let Some(label) = extract_menu_label(definition) {
            Self::Item { label }
        } else {
            Self::NoItem
        }
    }
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

/// Reorder `items` exactly as GNU's `menu_bar_items` does: walk
/// `menu-bar-final-items` from left to right and move each matching item to
/// the current end.  The declared list therefore defines the final items'
/// order; this is not a stable partition of their source-keymap order.
fn move_final_items_to_end(eval: &Context, items: &mut Vec<TtyMenuBarItem>) {
    let final_items = match eval.obarray().symbol_value("menu-bar-final-items") {
        Some(value) => *value,
        None => return,
    };
    if final_items.is_nil() {
        return;
    }

    let mut tail = final_items;
    while tail.is_cons() {
        let head = tail.cons_car();
        if let Some(name) = head.as_symbol_name() {
            if let Some(index) = items.iter().position(|item| item.key == name) {
                let item = items.remove(index);
                items.push(item);
            }
        }
        tail = tail.cons_cdr();
    }
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
