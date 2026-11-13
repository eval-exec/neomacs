//! Keymap system — key binding lookup and command dispatch.
//!
//! Provides an Emacs-compatible keymap system with:
//! - Sparse and full keymaps
//! - Parent (inheritance) chain lookup
//! - Key description parsing (`kbd` style: "C-x C-f", "M-x", "RET", etc.)
//! - Global and local (buffer) keymap support

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::eval::VariableView;
use crate::tagged::SymbolMode;
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;

use super::builtins::expect_integer_or_marker_in_buffers;
use super::chartable::{
    builtin_set_char_table_range_with_style, is_char_table, make_char_table_value,
};
use super::error::{EvalResult, Flow, expect_args, signal};
use super::eval::Context;
use super::intern::resolve_sym;
use super::intern::{SymId, intern};
use super::keyboard::pure::{
    KEY_CHAR_ALT, KEY_CHAR_CODE_MASK, KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META,
    KEY_CHAR_MOD_MASK, KEY_CHAR_SHIFT, KEY_CHAR_SUPER, reorder_event_symbol_modifiers,
};
use super::symbol::Obarray;
use super::value::{OrderedRuntimeBindingMap, Value, ValueKind, eq_value_swp, list_to_vec};
use strum::{EnumString, IntoStaticStr};

mod describe_bindings;
mod menu_bar_items;

pub(crate) use describe_bindings::{
    builtin_describe_buffer_bindings, builtin_help_describe_vector,
};
pub(crate) use menu_bar_items::{builtin_menu_bar_menu_at_x_y, menu_bar_top_level_items};

// Keymap lookup substrate moved to the eval manifest (stage-4 W1/E4).
// Core command modules keep the historical paths through this re-export.
pub use crate::emacs_core::eval::keymap_lookup::{
    ActiveKeyBindingResolution, DefaultBindingMode, KeyEvent, KeymapElement, KeymapLookupEvent,
    KeymapMarker, KeymapMutationEpoch, KeymapResolution, KeymapStateVariable, LocalMapProperty,
    MenuItemProperty, collect_map_entries_from_alist_in_state, collect_minor_mode_maps_in_state,
    command_remapping_command_name, command_remapping_in_active_maps,
    command_remapping_lookup_in_keymaps, command_remapping_lookup_in_lisp_keymap,
    command_remapping_normalize_target, current_active_maps_for_position,
    current_active_maps_for_position_read_only,
    dynamic_buffer_or_global_symbol_value_by_sym_id_in_state, emacs_event_to_key_event,
    expand_meta_prefix_char_events_in_obarray, for_each_keymap_element, get_keyelt,
    get_keyelt_runtime, get_keymap_in_obarray, get_keymap_in_runtime, is_keymap_autoload_form,
    is_list_keymap, key_binding_apply_remap_in_active_maps, key_event_to_emacs_event,
    keymap_binding_spine, keymap_mutation_epoch, keymap_symbol_id, list_keymap_access_with_event,
    list_keymap_accessible, list_keymap_copy, list_keymap_define, list_keymap_define_seq,
    list_keymap_define_seq_in_obarray_ex, list_keymap_define_seq_runtime_ex,
    list_keymap_for_each_binding, list_keymap_for_each_binding_recursive,
    list_keymap_inherits_from, list_keymap_lookup_one, list_keymap_lookup_one_in_obarray,
    list_keymap_lookup_one_noinherit, list_keymap_lookup_one_runtime,
    list_keymap_lookup_one_t_ok_in_obarray, list_keymap_lookup_one_unresolved,
    list_keymap_lookup_seq, list_keymap_parent, list_keymap_remove, list_keymap_set_parent,
    list_keymap_sibling_keymaps, local_map_property_at_buffer_point,
    lookup_key_in_keymaps_in_obarray, lookup_key_in_keymaps_in_obarray_runtime,
    lookup_key_in_obarray, lookup_key_in_obarray_runtime, lookup_keymap_with_partial,
    make_list_keymap, make_sparse_list_keymap, maybe_keymap_in_obarray, maybe_keymap_in_runtime,
    minor_mode_key_binding_in_context, note_keymap_mutation, resolve_active_key_binding,
    resolve_indirect_function_by_id_in_obarray, resolve_keymap_or_autoload,
    resolve_prefix_keymap_binding_in_obarray, where_is_keymaps_in_context,
};

pub use crate::emacs_core::eval::keymap_lookup::MenuButtonKind;

/// Stage-5 display inversion: the extension trait lives with the eval-owned
/// enum; this historical command path stays as a re-export for the
/// layout-engine crate.
pub use crate::emacs_core::eval::keymap_lookup::MenuButtonKindExt;

// The default bodies above are the implementation; this binds them to the
// upper-layer type that owns the state.

// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// ---------------------------------------------------------------------------
// Key description parsing  ("kbd" style)
// ---------------------------------------------------------------------------

/// Parse a key description string into a sequence of `KeyEvent`s.
///
/// Supported syntax:
/// - `"C-x"` — Ctrl+x
/// - `"M-x"` — Meta(Alt)+x
/// - `"S-x"` — Shift+x
/// - `"s-x"` — Super+x
/// - `"C-M-x"` — Ctrl+Meta+x
/// - `"C-x C-f"` — sequence of Ctrl+x then Ctrl+f
/// - `"RET"`, `"TAB"`, `"SPC"`, `"ESC"`, `"DEL"`, `"BS"` — named keys
/// - `"f1"` .. `"f12"` — function keys
/// - `"a"`, `"b"`, `"1"`, `"!"` — plain characters
pub fn parse_key_description(desc: &str) -> Result<Vec<KeyEvent>, String> {
    let desc = desc.trim();
    if desc.is_empty() {
        return Err("empty key description".to_string());
    }

    let mut result = Vec::new();
    for part in desc.split_whitespace() {
        result.push(parse_single_key(part)?);
    }
    Ok(result)
}

/// Parse a single key token (e.g. "C-x", "M-RET", "a", "f1").
pub fn parse_single_key(token: &str) -> Result<KeyEvent, String> {
    let mut ctrl = false;
    let mut meta = false;
    let mut shift = false;
    let mut super_ = false;
    let mut hyper = false;
    let mut alt = false;

    let mut remainder = token;

    // Parse modifier prefixes: "C-", "M-", "S-", "s-", "H-", "A-"
    loop {
        if let Some(rest) = remainder.strip_prefix("C-") {
            ctrl = true;
            remainder = rest;
        } else if let Some(rest) = remainder.strip_prefix("M-") {
            meta = true;
            remainder = rest;
        } else if remainder.starts_with("S-") && remainder.len() > 2 {
            let rest = &remainder[2..];
            shift = true;
            remainder = rest;
        } else if remainder.starts_with("s-") && remainder.len() > 2 {
            let rest = &remainder[2..];
            super_ = true;
            remainder = rest;
        } else if remainder.starts_with("H-") && remainder.len() > 2 {
            let rest = &remainder[2..];
            hyper = true;
            remainder = rest;
        } else if remainder.starts_with("A-") && remainder.len() > 2 {
            let rest = &remainder[2..];
            alt = true;
            remainder = rest;
        } else {
            break;
        }
    }

    if remainder.is_empty() {
        return Err(format!("incomplete key description: {}", token));
    }

    // Helper to build a Function event with current modifiers
    let mk_func = |name: &str| -> KeyEvent {
        KeyEvent::Function {
            name: intern(name),
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        }
    };

    // Check for named special keys
    match remainder {
        "RET" | "return" => Ok(mk_func("return")),
        "TAB" | "tab" => Ok(mk_func("tab")),
        "SPC" | "space" => Ok(KeyEvent::Char {
            code: ' ',
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        }),
        "ESC" | "escape" => Ok(KeyEvent::Char {
            code: '\u{1b}',
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        }),
        "DEL" | "delete" => Ok(mk_func("delete")),
        "BS" | "backspace" => Ok(mk_func("backspace")),
        "up" => Ok(mk_func("up")),
        "down" => Ok(mk_func("down")),
        "left" => Ok(mk_func("left")),
        "right" => Ok(mk_func("right")),
        "home" => Ok(mk_func("home")),
        "end" => Ok(mk_func("end")),
        "prior" | "page-up" => Ok(mk_func("prior")),
        "next" | "page-down" => Ok(mk_func("next")),
        "insert" => Ok(mk_func("insert")),
        other => {
            // Check for function keys: f1 .. f20
            if let Some(stripped) = other.strip_prefix('f')
                && let Ok(n) = stripped.parse::<u32>()
                && (1..=20).contains(&n)
            {
                let fkey = format!("f{}", n);
                return Ok(mk_func(&fkey));
            }

            // Single character
            let mut chars = other.chars();
            let ch = chars
                .next()
                .ok_or_else(|| format!("empty key after modifiers: {}", token))?;
            if chars.next().is_some() {
                return Err(format!("unknown key name: {}", other));
            }
            Ok(KeyEvent::Char {
                code: ch,
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            })
        }
    }
}

/// Format a key event back to a human-readable description string.
pub fn format_key_event(event: &KeyEvent) -> String {
    let mut parts = String::new();
    let (ctrl, meta, shift, super_, hyper, alt) = match event {
        KeyEvent::Char {
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
            ..
        } => (*ctrl, *meta, *shift, *super_, *hyper, *alt),
        KeyEvent::Function {
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
            ..
        } => (*ctrl, *meta, *shift, *super_, *hyper, *alt),
    };
    if alt {
        parts.push_str("A-");
    }
    if ctrl {
        parts.push_str("C-");
    }
    if hyper {
        parts.push_str("H-");
    }
    if meta {
        parts.push_str("M-");
    }
    if shift {
        parts.push_str("S-");
    }
    if super_ {
        parts.push_str("s-");
    }
    match event {
        KeyEvent::Char { code: ' ', .. } => {
            parts.push_str("SPC");
        }
        KeyEvent::Char { code: '\r', .. } => {
            parts.push_str("RET");
        }
        KeyEvent::Char { code: '\t', .. } => {
            parts.push_str("TAB");
        }
        KeyEvent::Char { code: '\u{7f}', .. } => {
            parts.push_str("DEL");
        }
        KeyEvent::Char { code: '\u{1b}', .. } => {
            parts.push_str("ESC");
        }
        KeyEvent::Char { code, .. } => {
            parts.push(*code);
        }
        KeyEvent::Function { name, .. } => match resolve_sym(*name) {
            "return" => parts.push_str("RET"),
            "tab" => parts.push_str("TAB"),
            "escape" => parts.push_str("ESC"),
            "delete" => parts.push_str("DEL"),
            "backspace" => parts.push_str("BS"),
            other => parts.push_str(other),
        },
    }
    parts
}

/// Format a full key sequence.
pub fn format_key_sequence(events: &[KeyEvent]) -> String {
    events
        .iter()
        .map(format_key_event)
        .collect::<Vec<_>>()
        .join(" ")
}

// ===========================================================================
// Emacs-compatible list keymaps
// ===========================================================================
//
// Official Emacs keymap format:
//   Full keymap:   (keymap CHAR-TABLE (EVENT . DEF) (EVENT . DEF) ...)
//   Sparse keymap: (keymap (EVENT . DEF) (EVENT . DEF) ...)
//   With parent:   (keymap (EVENT . DEF) ... . PARENT-KEYMAP)
//
// - `keymapp` checks `(consp x) && (car x) == 'keymap`
// - Char-table stores character bindings (0-MAX_CHAR)
// - Alist stores non-character bindings (function keys, mouse, remap, modified chars)
// - Events: integers (char code with modifier bits) or symbols (function keys)
// - Parent keymap: last CDR in the list, itself a `(keymap ...)` list

/// `(keymap--get-keyelt BINDING AUTOLOAD)` -> definition.
///
/// GNU `Fkeymap__get_keyelt` (keymap.c) delegates to get_keyelt: strip
/// `(menu-item NAME DEFN ...)` wrappers and `(STRING . DEFN)` menu labels
/// down to the definition.  Returning the element unreduced made help.el's
/// describe-map `(eq definition (lookup-key tail (vector event) t))` guard
/// fail for every wrapped menu binding, which silently emptied menu keymap
/// sections such as C-<down-mouse-2> facemenu-menu (ledger entry 61).
///
/// The body is a Lisp entry that follows the autoload filter path, so it is
/// an R0 body of the core command layer (STAGE4 design §5).
pub(crate) fn builtin_keymap_get_keyelt(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    expect_args("keymap--get-keyelt", &args, 2)?;
    get_keyelt_runtime(eval, args[0], !args[1].is_nil())
}

/// Return the active keymaps GNU's menu-bar builder would consult for the
/// selected window's buffer, in display collection order.
///
/// GNU `keyboard.c:menu_bar_items` sets the current buffer to the selected
/// window's buffer before walking the active maps.  It then scans the global
/// map first, followed by selected-buffer local/minor maps.  Redisplay may run
/// while `current-buffer` still names some other buffer, so the menu-bar path
/// must not use `BufferManager::current_local_map` as a proxy for the selected
/// window.
pub fn menu_bar_active_keymaps_for_frame_read_only(
    ctx: &Context,
    frame_id: crate::window::FrameId,
) -> Vec<Value> {
    let selected_window = ctx
        .frame_manager()
        .get(frame_id)
        .and_then(|frame| frame.selected_window())
        .map(|window| Value::make_window(window.id().0));
    let obey_overriding_local_maps = KeymapStateVariable::OverridingLocalMapMenuFlag
        .global_value(ctx.variables())
        .is_some_and(|value| value.is_truthy());

    let Ok(mut maps) = current_active_maps_for_position_read_only(
        ctx,
        obey_overriding_local_maps,
        selected_window.as_ref(),
    ) else {
        return Vec::new();
    };

    maps.reverse();
    maps
}

pub fn menu_bar_active_keymaps_read_only(ctx: &Context) -> Vec<Value> {
    let Some(frame_id) = ctx.frame_manager().selected_frame().map(|frame| frame.id) else {
        return Vec::new();
    };
    menu_bar_active_keymaps_for_frame_read_only(ctx, frame_id)
}

pub(crate) fn collect_minor_mode_map_entries_in_state(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    symbol_mode: SymbolMode,
) -> Vec<(SymId, Value)> {
    let mut maps = Vec::new();

    if let Some(emulation_raw) =
        KeymapStateVariable::EmulationModeMapAlists.buffer_or_global_value(vars, buffers, buffer_id)
        && let Some(emulation_entries) = list_to_vec(&emulation_raw)
    {
        for entry in emulation_entries {
            let alist_value = match entry.as_symbol_id(symbol_mode) {
                Some(sym_id) => dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
                    vars, dynamic, buffers, buffer_id, sym_id,
                )
                .unwrap_or(Value::NIL),
                None => entry,
            };
            collect_map_entries_from_alist_in_state(
                vars,
                dynamic,
                buffers,
                buffer_id,
                &alist_value,
                None,
                &mut maps,
                symbol_mode,
            );
        }
    }

    let overriding = KeymapStateVariable::MinorModeOverridingMapAlist
        .buffer_or_global_value(vars, buffers, buffer_id);
    if let Some(ref overriding_maps) = overriding {
        collect_map_entries_from_alist_in_state(
            vars,
            dynamic,
            buffers,
            buffer_id,
            overriding_maps,
            None,
            &mut maps,
            symbol_mode,
        );
    }

    if let Some(regular) =
        KeymapStateVariable::MinorModeMapAlist.buffer_or_global_value(vars, buffers, buffer_id)
    {
        collect_map_entries_from_alist_in_state(
            vars,
            dynamic,
            buffers,
            buffer_id,
            &regular,
            overriding.as_ref(),
            &mut maps,
            symbol_mode,
        );
    }

    maps
}

/// Check if two keymap values are the same object (by cons cell identity).
/// The accessible walk itself now compares through
/// [`KeymapResolution::as_value`] bits (GNU Frassq EQ, which must also match
/// autoload symbols); this cons-only helper remains for tests.
#[cfg(test)]
fn keymap_value_eq(a: &Value, b: &Value) -> bool {
    match (a.kind(), b.kind()) {
        (ValueKind::Cons, ValueKind::Cons) => *a == *b,
        _ => false,
    }
}

// ---------------------------------------------------------------------------
// Keymap spine taxonomy
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/keymap_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/positioned_symbols.rs"]
mod positioned_symbols;
