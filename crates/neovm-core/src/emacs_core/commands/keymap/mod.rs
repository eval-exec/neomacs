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
    KeymapMarker, KeymapMutationEpoch, KeymapResolution, KeymapStateVariable, MenuItemProperty,
    dynamic_buffer_or_global_symbol_value_by_sym_id_in_state, emacs_event_to_key_event,
    expand_meta_prefix_char_events_in_obarray, for_each_keymap_element, get_keyelt,
    get_keyelt_runtime, get_keymap_in_obarray, is_keymap_autoload_form, is_list_keymap,
    key_event_to_emacs_event, keymap_binding_spine, keymap_mutation_epoch, keymap_symbol_id,
    list_keymap_access_with_event, list_keymap_define, list_keymap_for_each_binding,
    list_keymap_for_each_binding_recursive, list_keymap_lookup_one,
    list_keymap_lookup_one_in_obarray, list_keymap_lookup_one_noinherit,
    list_keymap_lookup_one_runtime, list_keymap_lookup_one_t_ok_in_obarray,
    list_keymap_lookup_one_unresolved, list_keymap_lookup_seq, list_keymap_remove,
    list_keymap_set_parent, lookup_key_in_keymaps_in_obarray,
    lookup_key_in_keymaps_in_obarray_runtime, lookup_key_in_obarray, lookup_key_in_obarray_runtime,
    make_sparse_list_keymap, maybe_keymap_in_obarray, note_keymap_mutation,
    resolve_indirect_function_by_id_in_obarray, resolve_keymap_or_autoload,
    resolve_prefix_keymap_binding_in_obarray,
};

/// Lisp variables whose values are native inputs to GNU's active-keymap
/// collector.
///
/// GNU stores these as predeclared `Lisp_Object` globals (`V...` fields), so a
/// key lookup reads their identities directly.  Keeping the corresponding
/// Neomacs identities behind a closed enum prevents hot callers from falling
/// back to string-based lookup and makes every newly modeled variable choose a
/// cache slot at compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum MenuButtonKind {
    Toggle,
    Radio,
}

impl MenuButtonKind {
    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub fn from_value(value: Value, symbol_mode: SymbolMode) -> Option<Self> {
        let id = value.as_symbol_id(symbol_mode)?;
        let keyword = Self::from_keyword(resolve_sym(id))?;
        (id == intern(keyword.keyword())).then_some(keyword)
    }
}

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

/// Create a full list keymap: `(keymap CHAR-TABLE)`
pub fn make_list_keymap() -> Value {
    let char_table = make_char_table_value(Value::NIL, Value::NIL);
    Value::list(vec![KeymapMarker::Keymap.symbol_value(), char_table])
}

pub(crate) fn get_keymap_in_runtime(
    eval: &mut Context,
    value: &Value,
    error_if_not_keymap: bool,
    autoload: bool,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    let original = *value;
    let mut current = original;

    loop {
        if current.is_nil() {
            break;
        }
        if is_list_keymap(&current, symbol_mode) {
            return Ok(current);
        }

        let Some(symbol) = keymap_symbol_id(&current, symbol_mode) else {
            break;
        };
        let Some((_, function)) =
            resolve_indirect_function_by_id_in_obarray(eval.obarray(), symbol, symbol_mode)
        else {
            break;
        };

        if is_list_keymap(&function, symbol_mode) {
            return Ok(function);
        }

        if is_keymap_autoload_form(&function, symbol_mode) {
            if autoload {
                current = crate::emacs_core::autoload::builtin_autoload_do_load(
                    eval,
                    vec![function, original, Value::NIL],
                )?;
                continue;
            }
            if !error_if_not_keymap {
                return Ok(original);
            }
        }

        break;
    }

    if error_if_not_keymap {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("keymapp"), original],
        ))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn maybe_keymap_in_runtime(
    eval: &mut Context,
    value: &Value,
    autoload: bool,
) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    let resolved = get_keymap_in_runtime(eval, value, false, autoload)?;
    if is_list_keymap(&resolved, symbol_mode) {
        Ok(resolved)
    } else {
        Ok(Value::NIL)
    }
}

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

/// Look up one event for a caller that holds the evaluator, resolving the
/// definition exactly as GNU `access_keymap` does with `autoload` set: a
/// menu-item `:filter` runs, and a filter result that is a prefix keymap is
/// composed with the parent chain's prefix for the same event before the walk
/// descends.
/// Get the parent keymap (last CDR that is itself a keymap).
pub fn list_keymap_parent(keymap: &Value, symbol_mode: SymbolMode) -> Value {
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
        return Value::NIL;
    };
    while cursor.is_cons() {
        // Check if cursor itself is a parent keymap before treating as alist entry
        if is_list_keymap(&cursor, symbol_mode) {
            return cursor;
        }
        let entry_cdr = cursor.cons_cdr();
        if entry_cdr.is_nil() {
            return Value::NIL;
        }
        cursor = entry_cdr;
    }
    Value::NIL
}

/// Enumerate the keymaps embedded at a keymap's own level: the inline
/// sub-keymaps of a composed keymap `(keymap SUBMAP... . PARENT)` (produced by
/// `make-composed-keymap`, e.g. evil/general active state maps) plus the parent
/// keymap in the spine tail. All of these share the containing keymap's prefix,
/// mirroring GNU `map_keymap` / `Faccessible_keymaps`, which descend into
/// composed submaps and parents alike. Bindings and `(key . binding)` conses at
/// this level are NOT returned (those are the containing map's own bindings).
pub(crate) fn list_keymap_sibling_keymaps(keymap: &Value, symbol_mode: SymbolMode) -> Vec<Value> {
    let mut siblings = Vec::new();
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
        return siblings;
    };
    while cursor.is_cons() {
        // The spine tail is a parent keymap `(keymap ...)`.
        if is_list_keymap(&cursor, symbol_mode) {
            siblings.push(cursor);
            break;
        }
        let entry_car = cursor.cons_car();
        // An inline element that is itself a keymap is a composed submap; a
        // `(KEY . BINDING)` cons is an ordinary binding (its car is the key, not
        // the `keymap` marker) and is left to the containing map's own scan.
        if is_list_keymap(&entry_car, symbol_mode) {
            siblings.push(entry_car);
        }
        cursor = cursor.cons_cdr();
    }
    siblings
}

/// Check whether `target` appears in `keymap`'s parent chain.
pub fn list_keymap_inherits_from(keymap: &Value, target: &Value, symbol_mode: SymbolMode) -> bool {
    let mut current = *keymap;
    while is_list_keymap(&current, symbol_mode) {
        // Use pointer identity (eq), not structural equality (equal),
        // to detect cycles. Two keymaps with the same content are NOT
        // the same keymap.
        if eq_value_swp(&current, target, symbol_mode == SymbolMode::Positioned) {
            return true;
        }
        current = list_keymap_parent(&current, symbol_mode);
    }
    false
}

pub(crate) fn minor_mode_map_entry(
    entry: &Value,
    symbol_mode: SymbolMode,
) -> Option<(SymId, Value)> {
    if !entry.is_cons() {
        return None;
    };

    let (mode, cdr) = {
        let pair_car = entry.cons_car();
        let pair_cdr = entry.cons_cdr();
        (pair_car, pair_cdr)
    };
    let mode_name = mode.as_symbol_id(symbol_mode)?;
    if cdr == Value::NIL {
        return None;
    }
    Some((mode_name, cdr))
}

fn collect_maps_from_alist_in_state(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    alist: &Value,
    skip_if_in: Option<&Value>,
    maps: &mut Vec<Value>,
    symbol_mode: SymbolMode,
) {
    let obarray = vars.obarray();
    let Some(entries) = list_to_vec(alist) else {
        return;
    };
    for entry in entries {
        if !entry.is_cons() {
            continue;
        };
        let (mode_var, keymap_val) = {
            let pair_car = entry.cons_car();
            let pair_cdr = entry.cons_cdr();
            (pair_car, pair_cdr)
        };
        let Some(mode_id) = mode_var.as_symbol_id(symbol_mode) else {
            continue;
        };

        if let Some(skip_alist) = skip_if_in
            && assq_in_alist(skip_alist, &mode_var, symbol_mode)
        {
            continue;
        }

        // GNU `current_minor_maps` resolves the exact symbol stored in the
        // alist.  Do not round-trip through its printed name: minor modes may
        // deliberately use an uninterned symbol (for example a `cl-gensym`)
        // whose value cell is distinct from an interned namesake.
        let mode_active = dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            vars, dynamic, buffers, buffer_id, mode_id,
        )
        .map(|value| value.is_truthy())
        .unwrap_or(false);
        if !mode_active {
            continue;
        }

        if let Some(resolved) = maybe_keymap_in_obarray(obarray, &keymap_val, symbol_mode) {
            maps.push(resolved);
        }
    }
}

fn collect_map_entries_from_alist_in_state(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    alist: &Value,
    skip_if_in: Option<&Value>,
    maps: &mut Vec<(SymId, Value)>,
    symbol_mode: SymbolMode,
) {
    let obarray = vars.obarray();
    let Some(entries) = list_to_vec(alist) else {
        return;
    };
    for entry in entries {
        if !entry.is_cons() {
            continue;
        };
        let (mode_var, keymap_val) = {
            let pair_car = entry.cons_car();
            let pair_cdr = entry.cons_cdr();
            (pair_car, pair_cdr)
        };
        let Some(mode_name) = mode_var.as_symbol_id(symbol_mode) else {
            continue;
        };

        if let Some(skip_alist) = skip_if_in
            && assq_in_alist(skip_alist, &mode_var, symbol_mode)
        {
            continue;
        }

        let mode_active = dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            vars, dynamic, buffers, buffer_id, mode_name,
        )
        .map(|value| value.is_truthy())
        .unwrap_or(false);
        if !mode_active {
            continue;
        }

        if let Some(resolved) = maybe_keymap_in_obarray(obarray, &keymap_val, symbol_mode) {
            maps.push((mode_name, resolved));
        }
    }
}

fn assq_in_alist(alist: &Value, key: &Value, symbol_mode: SymbolMode) -> bool {
    let Some(entries) = list_to_vec(alist) else {
        return false;
    };

    for entry in entries {
        if !entry.is_cons() {
            continue;
        };
        let pair_car = entry.cons_car();
        let _pair_cdr = entry.cons_cdr();
        if eq_value_swp(&pair_car, key, symbol_mode == SymbolMode::Positioned) {
            return true;
        }
    }

    false
}

pub(crate) fn collect_minor_mode_maps_in_state(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    symbol_mode: SymbolMode,
) -> Vec<Value> {
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
            collect_maps_from_alist_in_state(
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
        collect_maps_from_alist_in_state(
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
        collect_maps_from_alist_in_state(
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
        .frames
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
    let Some(frame_id) = ctx.frames.selected_frame().map(|frame| frame.id) else {
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

#[derive(Clone, Copy, Debug)]
struct ActiveMapPosition {
    buffer_id: crate::buffer::BufferId,
    buffer_local_map: Value,
    char_pos: Option<i64>,
    displayed_string: Option<DisplayedStringPosition>,
}

/// The display area containing a string named by `POSN_STRING`.
///
/// GNU `current-active-maps` treats absent maps on mode/header-line strings
/// differently from absent maps on tab-line and ordinary displayed strings.
/// An enum keeps that semantic distinction out of raw symbol comparisons in
/// the map-precedence code.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DisplayedStringArea {
    ModeLine,
    HeaderLine,
    TabLine,
    Other,
}

impl DisplayedStringArea {
    fn from_position_area(value: Value, symbol_mode: SymbolMode) -> Self {
        match value.as_symbol_name(symbol_mode) {
            Some("mode-line") => Self::ModeLine,
            Some("header-line") => Self::HeaderLine,
            Some("tab-line") => Self::TabLine,
            _ => Self::Other,
        }
    }

    fn replaces_position_maps_when_absent(self) -> bool {
        matches!(self, Self::ModeLine | Self::HeaderLine)
    }
}

/// A validated `(STRING . CHARPOS)` from the fourth slot of a mouse position.
#[derive(Clone, Copy, Debug)]
struct DisplayedStringPosition {
    object: Value,
    char_pos: i64,
    area: DisplayedStringArea,
}

impl DisplayedStringPosition {
    fn from_position_slots(slots: &[Value], symbol_mode: SymbolMode) -> Option<Self> {
        let string_position = *slots.get(4)?;
        if !string_position.is_cons() {
            return None;
        }

        let object = string_position.cons_car();
        let string = object.as_lisp_string()?;
        let char_pos = string_position.cons_cdr().as_int()?;
        if char_pos < 0 || char_pos as usize >= string.schars() {
            return None;
        }

        Some(Self {
            object,
            char_pos,
            area: slots
                .get(1)
                .copied()
                .map(|area| DisplayedStringArea::from_position_area(area, symbol_mode))
                .unwrap_or(DisplayedStringArea::Other),
        })
    }
}

/// Whether a displayed string participates in one active-map layer.
///
/// `ReplaceWith(nil)` is intentionally distinct from `PreservePositionMap`:
/// GNU clears point-derived maps for mode/header-line strings even when those
/// strings have no corresponding property.
#[derive(Clone, Copy, Debug)]
enum DisplayedStringMapOverride {
    PreservePositionMap,
    ReplaceWith(Value),
}

impl DisplayedStringMapOverride {
    fn apply(self, position_map: Value) -> Value {
        match self {
            Self::PreservePositionMap => position_map,
            Self::ReplaceWith(string_map) => string_map,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct DisplayedStringMapOverrides {
    local_map: DisplayedStringMapOverride,
    keymap: DisplayedStringMapOverride,
}

#[derive(Clone, Copy, Debug)]
struct PositionMapLayers {
    local_map: Value,
    keymap: Value,
}

fn active_map_position(
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    position: Option<&Value>,
    symbol_mode: SymbolMode,
) -> Result<Option<ActiveMapPosition>, Flow> {
    let Some(buffer) = buffers.current_buffer() else {
        return Ok(None);
    };

    let default_position = ActiveMapPosition {
        buffer_id: buffer.id(),
        buffer_local_map: buffer.local_map(),
        char_pos: Some(buffer.point_lisp_char_pos().as_i64()),
        displayed_string: None,
    };

    let Some(position) = position else {
        return Ok(Some(default_position));
    };

    if position.is_window() {
        let window_id = crate::window::WindowId(position.as_window_id().unwrap());
        for frame_id in frames.frame_list() {
            let Some(frame) = frames.get(frame_id) else {
                continue;
            };
            let Some(window) = frame.find_window(window_id) else {
                continue;
            };
            let Some(buffer_id) = window.buffer_id() else {
                break;
            };
            let Some(target_buffer) = buffers.get(buffer_id) else {
                break;
            };

            return Ok(Some(ActiveMapPosition {
                buffer_id,
                buffer_local_map: target_buffer.local_map(),
                char_pos: Some(target_buffer.point_lisp_char_pos().as_i64()),
                displayed_string: None,
            }));
        }

        return Ok(Some(default_position));
    }

    if position.is_fixnum() || position.is_char() || position.is_marker() {
        let char_pos = expect_integer_or_marker_in_buffers(buffers, position)?;
        let point_min = buffer.point_min_lisp_char_pos().as_i64();
        let point_max = buffer.point_max_lisp_char_pos().as_i64();
        if char_pos < point_min || char_pos > point_max {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![Value::make_buffer(buffer.id()), *position],
            ));
        }

        return Ok(Some(ActiveMapPosition {
            buffer_id: buffer.id(),
            buffer_local_map: buffer.local_map(),
            char_pos: Some(char_pos),
            displayed_string: None,
        }));
    }

    let Some(slots) = list_to_vec(position) else {
        return Ok(Some(default_position));
    };
    if slots.len() < 6 {
        return Ok(Some(default_position));
    }

    let window_id = match slots[0].as_window_id() {
        Some(id) => crate::window::WindowId(id),
        None => return Ok(Some(default_position)),
    };
    let char_pos = slots[5].as_int().or_else(|| slots[1].as_int());
    let displayed_string = DisplayedStringPosition::from_position_slots(&slots, symbol_mode);

    for frame_id in frames.frame_list() {
        let Some(frame) = frames.get(frame_id) else {
            continue;
        };
        let Some(window) = frame.find_window(window_id) else {
            continue;
        };
        let Some(buffer_id) = window.buffer_id() else {
            continue;
        };
        let Some(target_buffer) = buffers.get(buffer_id) else {
            continue;
        };
        // GNU never signals for an event position.  `click_position`
        // (src/keymap.c:1639-1646) range-checks only a fixnum or a marker;
        // a cons falls back to `PT`, so its `args_out_of_range` cannot be
        // reached from a posn.  The posn branch (:1727-1740) instead uses its
        // range test only to decide whether to consult the `local-map` and
        // `keymap` text properties at that position:
        //
        //   if (FIXNUMP (buffer_posn)
        //       && XFIXNUM (buffer_posn) >= BEG && XFIXNUM (buffer_posn) <= Z)
        //     { ... get_local_map ... }
        //   if (NILP (local_map))
        //     local_map = BVAR (current_buffer, keymap);
        //
        // Dropping the position is that fallback: `position_map_layers` reads
        // `char_pos: None` as "no property lookup, use the buffer's own local
        // map".  An inactive mini-window draws the echo area's text while it
        // stays bound to the empty ` *Minibuf-0*`, so every mouse posn over a
        // displayed message names a position past that buffer's end; signalling
        // escaped `read_key_sequence` and reached the command loop once per
        // mouse event.
        let char_pos = char_pos.filter(|char_pos| {
            target_buffer
                .full_lisp_char_region()
                .contains(crate::buffer::LispCharPos1::new(*char_pos))
        });

        return Ok(Some(ActiveMapPosition {
            buffer_id,
            buffer_local_map: target_buffer.local_map(),
            char_pos,
            displayed_string,
        }));
    }

    Ok(Some(default_position))
}

fn keymap_property_at_position(
    vars: VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    buffer_id: crate::buffer::BufferId,
    char_pos: i64,
    property: LocalMapProperty,
) -> Result<Value, Flow> {
    let prop_symbol = Value::from_sym_id(property.symbol_id());
    let buffer = buffers
        .get(buffer_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    // GNU `get_local_map` clips before temporarily widening
    // (`src/intervals.c:2176-2208`).  The order matters: the full-buffer
    // range check in `current-active-maps` admits a renderer event outside a
    // narrowing, but its property evidence comes from the nearest accessible
    // boundary, never from inaccessible text and never from a signalling
    // Lisp property primitive.
    let position = crate::buffer::LispCharPos1::new(char_pos).clamp(
        buffer.point_min_lisp_char_pos(),
        buffer.point_max_lisp_char_pos(),
    );
    let char_property = super::textprop::buffer_char_property_at_full_lisp_pos(
        vars,
        buffers,
        buffer,
        position,
        prop_symbol,
    );
    if !char_property.is_nil() {
        return Ok(char_property);
    }

    super::textprop::buffer_pos_property_at_full_lisp_pos(
        vars,
        buffers,
        buffer,
        position,
        prop_symbol,
    )
}

/// The two text properties GNU's `get_local_map` (keymap.c) consults, and the
/// fallback each one carries when the property is absent or names no keymap.
///
/// Naming the property with a string instead let the two fallbacks drift apart:
/// `keymap` has none, `local-map` falls back to the buffer's own keymap, and a
/// caller that passes the wrong string gets the wrong fallback silently.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LocalMapProperty {
    /// `keymap`: consulted ahead of the minor-mode maps, with NO fallback.
    Keymap,
    /// `local-map`: consulted in place of the buffer's own keymap, which is its
    /// fallback.
    LocalMap,
}

impl LocalMapProperty {
    fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static LOCAL_MAP: OnceLock<SymId> = OnceLock::new();

        match self {
            Self::Keymap => KeymapMarker::Keymap.symbol_id(),
            Self::LocalMap => *LOCAL_MAP.get_or_init(|| intern("local-map")),
        }
    }
}

/// GNU `get_local_map (BUF_PT (b), b, PROP)`: the keymap named by PROP at
/// BUFFER's OWN point -- not the selected window's -- with PROP's fallback.
///
/// `describe-buffer-bindings` reports on a buffer that need not be current or
/// displayed, so the position-based helpers that resolve through the selected
/// window cannot answer this.
pub(crate) fn local_map_property_at_buffer_point(
    vars: VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    buffer_object: Value,
    buffer_point: i64,
    property: LocalMapProperty,
    buffer_keymap: Value,
    symbol_mode: SymbolMode,
) -> Result<Value, Flow> {
    let obarray = vars.obarray();
    let fallback = match property {
        LocalMapProperty::Keymap => Value::NIL,
        LocalMapProperty::LocalMap => buffer_keymap,
    };
    let buffer_id = buffer_object
        .as_buffer_id()
        .ok_or_else(|| signal(LispCondition::WrongTypeArgument, vec![buffer_object]))?;
    let found = keymap_property_at_position(vars, buffers, buffer_id, buffer_point, property)?;
    Ok(maybe_keymap_in_obarray(obarray, &found, symbol_mode).unwrap_or(fallback))
}

fn displayed_string_map_overrides(
    vars: VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    displayed: DisplayedStringPosition,
    symbol_mode: SymbolMode,
) -> Result<DisplayedStringMapOverrides, Flow> {
    let obarray = vars.obarray();
    let resolve = |property: LocalMapProperty| -> Result<DisplayedStringMapOverride, Flow> {
        let found = super::textprop::builtin_get_text_property_in_state(
            vars,
            buffers,
            &[
                Value::fixnum(displayed.char_pos),
                Value::from_sym_id(property.symbol_id()),
                displayed.object,
            ],
        )?;
        if found.is_nil() && !displayed.area.replaces_position_maps_when_absent() {
            return Ok(DisplayedStringMapOverride::PreservePositionMap);
        }

        Ok(DisplayedStringMapOverride::ReplaceWith(
            maybe_keymap_in_obarray(obarray, &found, symbol_mode).unwrap_or(Value::NIL),
        ))
    };

    Ok(DisplayedStringMapOverrides {
        local_map: resolve(LocalMapProperty::LocalMap)?,
        keymap: resolve(LocalMapProperty::Keymap)?,
    })
}

fn position_map_layers(
    vars: VariableView<'_>,
    buffers: &crate::buffer::BufferManager,
    active_position: Option<ActiveMapPosition>,
    fallback_local_map: Value,
    symbol_mode: SymbolMode,
) -> Result<PositionMapLayers, Flow> {
    let obarray = vars.obarray();
    let Some(active_position) = active_position else {
        return Ok(PositionMapLayers {
            local_map: fallback_local_map,
            keymap: Value::NIL,
        });
    };

    let (mut local_map, mut keymap) = if let Some(char_pos) = active_position.char_pos {
        let local_property = keymap_property_at_position(
            vars,
            buffers,
            active_position.buffer_id,
            char_pos,
            LocalMapProperty::LocalMap,
        )?;
        let keymap_property = keymap_property_at_position(
            vars,
            buffers,
            active_position.buffer_id,
            char_pos,
            LocalMapProperty::Keymap,
        )?;
        (
            maybe_keymap_in_obarray(obarray, &local_property, symbol_mode)
                .unwrap_or(active_position.buffer_local_map),
            maybe_keymap_in_obarray(obarray, &keymap_property, symbol_mode).unwrap_or(Value::NIL),
        )
    } else {
        (active_position.buffer_local_map, Value::NIL)
    };

    if let Some(displayed_string) = active_position.displayed_string {
        // Mirrors GNU keymap.c `current-active-maps`: a displayed string's
        // maps override the maps derived from the clicked buffer position.
        let overrides =
            displayed_string_map_overrides(vars, buffers, displayed_string, symbol_mode)?;
        local_map = overrides.local_map.apply(local_map);
        keymap = overrides.keymap.apply(keymap);
    }

    Ok(PositionMapLayers { local_map, keymap })
}

#[allow(clippy::too_many_arguments)] // explicit keymap layers preserve GNU precedence ordering
fn current_active_maps_from_parts(
    vars: VariableView<'_>,
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    current_local_map: Value,
    global_map: Value,
    minor_maps: Vec<Value>,
    overriding_local_map: Option<Value>,
    overriding_terminal_local_map: Option<Value>,
    obey_overriding_local_maps: bool,
    position: Option<&Value>,
    symbol_mode: SymbolMode,
) -> Result<Vec<Value>, Flow> {
    let active_position = active_map_position(frames, buffers, position, symbol_mode)?;
    let current_buffer_id = active_position.map(|pos| pos.buffer_id);

    if obey_overriding_local_maps
        && overriding_terminal_local_map.is_none()
        && let Some(overriding_local_map) = overriding_local_map
    {
        return Ok(vec![overriding_local_map, global_map]);
    }

    let mut maps = Vec::new();

    if obey_overriding_local_maps
        && let Some(overriding_terminal_local_map) = overriding_terminal_local_map
    {
        maps.push(overriding_terminal_local_map);
    }

    let position_maps = position_map_layers(
        vars,
        buffers,
        active_position,
        current_local_map,
        symbol_mode,
    )?;
    if !position_maps.keymap.is_nil() {
        maps.push(position_maps.keymap);
    }

    if minor_maps.is_empty() {
        maps.extend(collect_minor_mode_maps_in_state(
            vars,
            &[],
            buffers,
            current_buffer_id,
            symbol_mode,
        ));
    } else {
        maps.extend(minor_maps);
    }

    if !position_maps.local_map.is_nil() {
        maps.push(position_maps.local_map);
    }

    maps.push(global_map);
    Ok(maps)
}

pub(crate) fn current_active_maps_for_position(
    ctx: &mut Context,
    obey_overriding_local_maps: bool,
    position: Option<&Value>,
) -> Result<Vec<Value>, Flow> {
    let symbol_mode = ctx.symbol_mode();
    let global_map = ctx.current_global_map();
    let overriding_local_map = KeymapStateVariable::OverridingLocalMap
        .global_value(ctx.variables())
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value, symbol_mode));
    let overriding_terminal_local_map = KeymapStateVariable::OverridingTerminalLocalMap
        .global_value(ctx.variables())
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value, symbol_mode));

    current_active_maps_from_parts(
        ctx.variables(),
        &ctx.frames,
        &ctx.edit.buffers,
        ctx.edit.buffers.current_local_map(),
        global_map,
        Vec::new(),
        overriding_local_map,
        overriding_terminal_local_map,
        obey_overriding_local_maps,
        position,
        symbol_mode,
    )
}

pub(crate) fn current_active_maps_for_position_read_only(
    ctx: &Context,
    obey_overriding_local_maps: bool,
    position: Option<&Value>,
) -> Result<Vec<Value>, Flow> {
    let symbol_mode = ctx.symbol_mode();
    let global_map = ctx.current_global_map();
    let overriding_local_map = KeymapStateVariable::OverridingLocalMap
        .global_value(ctx.variables())
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value, symbol_mode));
    let overriding_terminal_local_map = KeymapStateVariable::OverridingTerminalLocalMap
        .global_value(ctx.variables())
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value, symbol_mode));

    current_active_maps_from_parts(
        ctx.variables(),
        &ctx.frames,
        &ctx.edit.buffers,
        ctx.edit.buffers.current_local_map(),
        global_map,
        Vec::new(),
        overriding_local_map,
        overriding_terminal_local_map,
        obey_overriding_local_maps,
        position,
        symbol_mode,
    )
}

pub(crate) fn is_plain_printable_emacs_event(event: &Value) -> bool {
    let Some(ch) = (match event.kind() {
        ValueKind::Fixnum(code) if (code & KEY_CHAR_MOD_MASK) == 0 => char::from_u32(code as u32),
        _ => None,
    }) else {
        return false;
    };

    !ch.is_control() && ch != '\u{7f}'
}

pub(crate) fn resolve_active_key_binding(
    ctx: &mut Context,
    events: &[Value],
    default_binding_mode: DefaultBindingMode,
    no_remap: bool,
    position: Option<&Value>,
) -> Result<ActiveKeyBindingResolution, Flow> {
    let active_maps = current_active_maps_for_position(ctx, true, position)?;
    // The collected active maps (and any heap event conses, e.g. mouse
    // events) live only in Rust storage across the lookup and the remap
    // pass, both of which can run Lisp; a keymap swapped out of its buffer
    // or symbol mid-lookup would be freed while later entries are probed.
    // One rooted holder spans both calls.
    let mut holder = Value::NIL;
    for value in active_maps.iter().chain(events.iter()).rev() {
        if value.is_heap_object() {
            holder = Value::cons(*value, holder);
        }
    }
    let root_scope = ctx.save_specpdl_roots();
    ctx.push_specpdl_root(holder);
    let result = (|| -> Result<ActiveKeyBindingResolution, Flow> {
        let lookup = lookup_key_in_keymaps_in_obarray_runtime(
            ctx,
            &active_maps,
            events,
            default_binding_mode.accepts_default(),
        )?;
        let binding = if !lookup.is_nil() && !lookup.is_fixnum() {
            key_binding_apply_remap_in_active_maps(ctx, &active_maps, lookup, no_remap)?
        } else if events.len() == 1 && is_plain_printable_emacs_event(&events[0]) {
            Value::symbol("self-insert-command")
        } else {
            Value::NIL
        };

        Ok(ActiveKeyBindingResolution { lookup, binding })
    })();
    ctx.restore_specpdl_roots(root_scope);
    result
}

fn lookup_minor_mode_binding_in_alist_in_obarray(
    vars: VariableView<'_>,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    events: &[Value],
    alist_value: &Value,
    symbol_mode: SymbolMode,
) -> Result<Option<(SymId, Value)>, Flow> {
    let obarray = vars.obarray();
    let Some(entries) = list_to_vec(alist_value) else {
        return Ok(None);
    };

    for entry in entries {
        let Some((mode_name, map_value)) = minor_mode_map_entry(&entry, symbol_mode) else {
            continue;
        };
        if !dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            vars, dynamic, buffers, buffer_id, mode_name,
        )
        .is_some_and(|v| v.is_truthy())
        {
            continue;
        }

        let keymap = if is_list_keymap(&map_value, symbol_mode) {
            map_value
        } else if map_value.as_symbol_name(symbol_mode).is_some() {
            match map_value
                .as_symbol_name(symbol_mode)
                .and_then(|name| vars.value_by_name(name))
            {
                Some(value) if is_list_keymap(&value, symbol_mode) => value,
                _ => match obarray.symbol_function_of_value(&map_value) {
                    Some(value) if is_list_keymap(&value, symbol_mode) => value,
                    _ => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("keymapp"), map_value],
                        ));
                    }
                },
            }
        } else {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("keymapp"), map_value],
            ));
        };

        let binding = lookup_keymap_with_partial(&keymap, events, symbol_mode);
        if binding.is_nil() {
            continue;
        }

        return Ok(Some((mode_name, binding)));
    }

    Ok(None)
}

pub(crate) fn minor_mode_key_binding_in_context(
    ctx: &Context,
    events: &[Value],
) -> Result<Value, Flow> {
    let symbol_mode = ctx.symbol_mode();
    let current_buffer_id = ctx.edit.buffers.current_buffer_id();
    if let Some(emulation_raw) = KeymapStateVariable::EmulationModeMapAlists.buffer_or_global_value(
        ctx.variables(),
        &ctx.edit.buffers,
        current_buffer_id,
    ) && let Some(emulation_entries) = list_to_vec(&emulation_raw)
    {
        for emulation_entry in emulation_entries {
            let alist_value = match emulation_entry.as_symbol_id(symbol_mode) {
                Some(sym_id) => dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
                    ctx.variables(),
                    &[],
                    &ctx.edit.buffers,
                    current_buffer_id,
                    sym_id,
                )
                .unwrap_or(Value::NIL),
                None => emulation_entry,
            };
            if let Some((mode_name, binding)) = lookup_minor_mode_binding_in_alist_in_obarray(
                ctx.variables(),
                &[],
                &ctx.edit.buffers,
                current_buffer_id,
                events,
                &alist_value,
                symbol_mode,
            )? {
                return Ok(Value::list(vec![Value::cons(
                    Value::from_sym_id(mode_name),
                    binding,
                )]));
            }
        }
    }

    for variable in [
        KeymapStateVariable::MinorModeOverridingMapAlist,
        KeymapStateVariable::MinorModeMapAlist,
    ] {
        let Some(alist_value) =
            variable.buffer_or_global_value(ctx.variables(), &ctx.edit.buffers, current_buffer_id)
        else {
            continue;
        };
        if let Some((mode_name, binding)) = lookup_minor_mode_binding_in_alist_in_obarray(
            ctx.variables(),
            &[],
            &ctx.edit.buffers,
            current_buffer_id,
            events,
            &alist_value,
            symbol_mode,
        )? {
            return Ok(Value::list(vec![Value::cons(
                Value::from_sym_id(mode_name),
                binding,
            )]));
        }
    }

    Ok(Value::NIL)
}

fn where_is_expect_keymap_in_obarray(
    obarray: &Obarray,
    value: &Value,
    symbol_mode: SymbolMode,
) -> Result<Value, Flow> {
    get_keymap_in_obarray(obarray, value, true, symbol_mode)
}

fn where_is_explicit_keymaps_in_context(ctx: &Context, value: &Value) -> Result<Vec<Value>, Flow> {
    let symbol_mode = ctx.symbol_mode();
    if is_list_keymap(value, symbol_mode) {
        let keymap = where_is_expect_keymap_in_obarray(&ctx.obarray, value, symbol_mode)?;
        let mut keymaps = vec![keymap];
        let global_map = ctx.current_global_map();
        if is_list_keymap(&global_map, symbol_mode) && global_map != keymap {
            keymaps.push(global_map);
        }
        return Ok(keymaps);
    }

    if value.is_cons()
        && maybe_keymap_in_obarray(&ctx.obarray, &value.cons_car(), symbol_mode)
            .is_some_and(|keymap| is_list_keymap(&keymap, symbol_mode))
        && let Some(items) = list_to_vec(value)
    {
        let mut keymaps = Vec::with_capacity(items.len());
        for item in items {
            keymaps.push(where_is_expect_keymap_in_obarray(
                &ctx.obarray,
                &item,
                symbol_mode,
            )?);
        }
        return Ok(keymaps);
    }

    let keymap = if value.is_cons() {
        *value
    } else {
        where_is_expect_keymap_in_obarray(&ctx.obarray, value, symbol_mode)?
    };
    let mut keymaps = vec![keymap];
    let global_map = ctx.current_global_map();
    if is_list_keymap(&global_map, symbol_mode) && global_map != keymap {
        keymaps.push(global_map);
    }
    Ok(keymaps)
}

pub(crate) fn where_is_keymaps_in_context(
    ctx: &mut Context,
    value: Option<&Value>,
) -> Result<Vec<Value>, Flow> {
    // GNU `Fwhere_is_internal' with a nil KEYMAP uses
    // `Fcurrent_active_maps (Qnil, Qnil)' -- olp = nil, so the overriding maps
    // (`overriding-local-map' / `overriding-terminal-local-map') are EXCLUDED,
    // unlike `key-binding' which passes olp = t. So `where-is-internal' never
    // reports a key bound only in an overriding map.
    match value {
        Some(keymap_arg) if keymap_arg.is_nil() => {
            Ok(current_active_maps_for_position(ctx, false, None).unwrap_or_default())
        }
        Some(keymap_arg) => where_is_explicit_keymaps_in_context(ctx, keymap_arg),
        None => Ok(current_active_maps_for_position(ctx, false, None).unwrap_or_default()),
    }
}

fn command_remapping_list_tail(value: &Value, n: usize) -> Option<Value> {
    let mut cursor = *value;
    for _ in 0..n {
        match cursor.kind() {
            ValueKind::Cons => {
                cursor = cursor.cons_cdr();
            }
            _ => return None,
        }
    }
    Some(cursor)
}

fn command_remapping_nth_list_element(value: &Value, index: usize) -> Option<Value> {
    let tail = command_remapping_list_tail(value, index)?;
    match tail.kind() {
        ValueKind::Cons => Some(tail.cons_car()),
        _ => None,
    }
}

fn command_remapping_lookup_in_lisp_remap_entry(
    entry: &Value,
    command: SymId,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    if !KeymapMarker::Remap.is_value(command_remapping_nth_list_element(entry, 0)?, symbol_mode) {
        return None;
    }
    if !KeymapMarker::Keymap.is_value(command_remapping_nth_list_element(entry, 1)?, symbol_mode) {
        return None;
    }

    let mut bindings = command_remapping_list_tail(entry, 2)?;
    while bindings.is_cons() {
        let (binding_entry, rest) = {
            let pair_car = bindings.cons_car();
            let pair_cdr = bindings.cons_cdr();
            (pair_car, pair_cdr)
        };
        if binding_entry.is_cons() {
            let (binding_key, binding_target) = {
                let pair_car = binding_entry.cons_car();
                let pair_cdr = binding_entry.cons_cdr();
                (pair_car, pair_cdr)
            };
            if binding_key.as_symbol_id(symbol_mode) == Some(command) {
                return Some(binding_target);
            }
        }
        bindings = rest;
    }
    None
}

pub(crate) fn command_remapping_lookup_in_lisp_keymap(
    keymap: &Value,
    command: SymId,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    if !is_list_keymap(keymap, symbol_mode) {
        return None;
    }

    let mut cursor = if keymap.is_cons() {
        keymap.cons_cdr()
    } else {
        Value::NIL
    };

    while cursor.is_cons() {
        if is_list_keymap(&cursor, symbol_mode) {
            if let Some(parent) =
                command_remapping_lookup_in_lisp_keymap(&cursor, command, symbol_mode)
            {
                return Some(parent);
            }
            break;
        }

        let car = cursor.cons_car();
        let cdr = cursor.cons_cdr();
        if is_list_keymap(&car, symbol_mode) {
            if let Some(child) = command_remapping_lookup_in_lisp_keymap(&car, command, symbol_mode)
            {
                return Some(child);
            }
            cursor = cdr;
            continue;
        }
        if let Some(remap) =
            command_remapping_lookup_in_lisp_remap_entry(&car, command, symbol_mode)
        {
            return Some(remap);
        }
        cursor = cdr;
    }

    None
}

fn command_remapping_menu_item_target(value: &Value, symbol_mode: SymbolMode) -> Option<Value> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    if !KeymapMarker::MenuItem.is_value(pair_car, symbol_mode) {
        return None;
    }

    let tail = pair_cdr;
    let title = command_remapping_nth_list_element(&tail, 0)?;
    if title.is_nil() {
        return None;
    }
    command_remapping_nth_list_element(&tail, 1)
}

pub(crate) fn command_remapping_normalize_target(raw: Value, symbol_mode: SymbolMode) -> Value {
    if let Some(menu_target) = command_remapping_menu_item_target(&raw, symbol_mode) {
        return if menu_target.is_integer() {
            Value::NIL
        } else {
            menu_target
        };
    }
    if eq_value_swp(&raw, &Value::T, symbol_mode == SymbolMode::Positioned) || raw.is_fixnum() {
        return Value::NIL;
    }
    raw
}

fn command_remapping_lookup_in_keymap_value(
    keymap: &Value,
    command: SymId,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    command_remapping_lookup_in_lisp_keymap(keymap, command, symbol_mode)
        .map(|value| command_remapping_normalize_target(value, symbol_mode))
}

pub(crate) fn command_remapping_lookup_in_keymaps(
    keymaps: &[Value],
    command: SymId,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    for keymap in keymaps {
        if !is_list_keymap(keymap, symbol_mode) {
            continue;
        }
        if let Some(value) = command_remapping_lookup_in_keymap_value(keymap, command, symbol_mode)
        {
            return Some(value);
        }
    }
    None
}

/// Runtime twin of [`command_remapping_lookup_in_keymaps`] that resolves a
/// menu-item `:filter` on the remap target via [`get_keyelt_runtime`] (GNU
/// `get_keyelt` with autoload=true). Without this a `(menu-item "" nil :filter
/// FN)` remap -- e.g. Doom's `cmds!` -- normalizes to its (nil) DEFN and the
/// remap is dropped (so `[remap evil-record-macro]` -> `q` never reaches the
/// real command).
pub(crate) fn command_remapping_lookup_in_keymaps_runtime(
    ctx: &mut Context,
    keymaps: &[Value],
    command: SymId,
) -> Result<Option<Value>, Flow> {
    let symbol_mode = ctx.symbol_mode();
    for keymap in keymaps {
        if !is_list_keymap(keymap, symbol_mode) {
            continue;
        }
        let Some(raw) = command_remapping_lookup_in_lisp_keymap(keymap, command, symbol_mode)
        else {
            continue;
        };
        let resolved = get_keyelt_runtime(ctx, raw, true)?;
        let normalized = command_remapping_normalize_target(resolved, symbol_mode);
        if !normalized.is_nil() {
            return Ok(Some(normalized));
        }
    }
    Ok(None)
}

pub(crate) fn command_remapping_command_name(
    command: &Value,
    symbol_mode: SymbolMode,
) -> Option<SymId> {
    command.as_symbol_id(symbol_mode)
}

/// GNU `Fcommand_remapping (COMMAND, Qnil, Qnil)`: the one-level remapping of
/// COMMAND in the currently active keymaps, or nil.
///
/// `read_key_sequence` asks this once for every command it returns
/// (keyboard.c:11863-11868) and `command_loop_1` reuses the answer
/// (keyboard.c:1464-1495).  It is the raw active-map answer, without the
/// menu-item resolution `key-binding` performs, because that is what GNU's
/// `Fcommand_remapping` returns.
pub(crate) fn command_remapping_in_active_maps(
    ctx: &Context,
    command: Value,
) -> Result<Value, Flow> {
    let symbol_mode = ctx.symbol_mode();
    let Some(command_name) = command.as_symbol_id(symbol_mode) else {
        return Ok(Value::NIL);
    };
    let active_maps = current_active_maps_for_position_read_only(ctx, true, None)?;
    Ok(
        command_remapping_lookup_in_keymaps(&active_maps, command_name, symbol_mode)
            .unwrap_or(Value::NIL),
    )
}

pub(crate) fn key_binding_apply_remap_in_active_maps(
    ctx: &mut Context,
    active_maps: &[Value],
    binding: Value,
    no_remap: bool,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    if no_remap {
        return Ok(binding);
    }
    let Some(command_name) = binding.as_symbol_id(symbol_mode) else {
        return Ok(binding);
    };
    match command_remapping_lookup_in_keymaps_runtime(ctx, active_maps, command_name)? {
        Some(remapped) if !remapped.is_nil() => Ok(remapped),
        _ => Ok(binding),
    }
}

/// Look up a key sequence in a keymap, following prefix keymaps and parent chains.
/// Returns the binding Value, or the number of keys matched (as `Value::Int`)
/// when the sequence resolves through a non-keymap binding.
pub(crate) fn lookup_keymap_with_partial(
    keymap: &Value,
    emacs_events: &[Value],
    symbol_mode: SymbolMode,
) -> Value {
    if emacs_events.is_empty() {
        return *keymap;
    }
    list_keymap_lookup_seq(keymap, emacs_events, symbol_mode)
}

/// Define a key in a keymap, auto-creating prefix maps for multi-key sequences.
///
/// Returns `Err` if an intermediate key is already bound to a non-prefix
/// command (matching GNU Emacs behavior which signals an error).
pub fn list_keymap_define_seq(
    keymap: Value,
    events: &[Value],
    def: Value,
    symbol_mode: SymbolMode,
) -> Result<(), String> {
    note_keymap_mutation();
    if events.is_empty() {
        return Ok(());
    }
    if events.len() == 1 {
        list_keymap_define(keymap, events[0], def, symbol_mode);
        return Ok(());
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        if i == events.len() - 1 {
            list_keymap_define(current_map, *event, def, symbol_mode);
            return Ok(());
        }
        // Use noinherit: only look in current keymap level for prefix,
        // matching GNU Emacs define-key which uses access_keymap(noinherit=1)
        let binding = list_keymap_lookup_one_noinherit(&current_map, event, symbol_mode);
        if is_list_keymap(&binding, symbol_mode) {
            current_map = binding;
        } else if binding.is_nil() {
            // No binding at this level, create a new prefix keymap
            let prefix_map = make_sparse_list_keymap();
            list_keymap_define(current_map, *event, prefix_map, symbol_mode);
            current_map = prefix_map;
        } else {
            // Non-prefix binding found — error (matching GNU Emacs)
            let full = describe_event_sequence(events, symbol_mode);
            let prefix = describe_event_sequence(&events[..=i], symbol_mode);
            return Err(format!(
                "Key sequence {full} starts with non-prefix key {prefix}"
            ));
        }
    }
    Ok(())
}

/// Extended version of define-seq that supports the REMOVE flag.
pub fn list_keymap_define_seq_in_obarray_ex(
    obarray: &Obarray,
    keymap: Value,
    events: &[Value],
    def: Value,
    remove: bool,
    symbol_mode: SymbolMode,
) -> Result<(), String> {
    note_keymap_mutation();
    if events.is_empty() {
        return Ok(());
    }
    if events.len() == 1 {
        if remove {
            list_keymap_remove(keymap, events[0], symbol_mode);
        } else {
            list_keymap_define(keymap, events[0], def, symbol_mode);
        }
        return Ok(());
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        if i == events.len() - 1 {
            if remove {
                list_keymap_remove(current_map, *event, symbol_mode);
            } else {
                list_keymap_define(current_map, *event, def, symbol_mode);
            }
            return Ok(());
        }
        // Use noinherit: only look in current keymap level for prefix,
        // matching GNU Emacs define-key which uses access_keymap(noinherit=1)
        let binding = list_keymap_lookup_one_noinherit(&current_map, event, symbol_mode);
        if let Some(prefix_map) =
            resolve_prefix_keymap_binding_in_obarray(obarray, &binding, symbol_mode)
        {
            current_map = prefix_map;
        } else if binding.is_nil() {
            // No binding, create a new prefix keymap.
            // Matches GNU `define_as_prefix` (keymap.c:1446-1452).
            let prefix_map = make_sparse_list_keymap();
            list_keymap_define(current_map, *event, prefix_map, symbol_mode);
            current_map = prefix_map;
        } else {
            // GNU `Fdefine_key` only calls `define_as_prefix` after
            // `access_keymap` returns nil.  Existing non-prefix bindings are
            // errors, even though a fresh undefined key is promoted to a new
            // prefix keymap.
            let full = describe_event_sequence(events, symbol_mode);
            let prefix = describe_event_sequence(&events[..=i], symbol_mode);
            return Err(format!(
                "Key sequence {full} starts with non-prefix key {prefix}"
            ));
        }
    }
    Ok(())
}

/// Define a sequence with runtime prefix resolution and the REMOVE flag.
/// Menu-item filters must run before deciding whether a binding is a prefix,
/// just as in GNU `access_keymap(..., noinherit=true, autoload=true)`.
pub(crate) fn list_keymap_define_seq_runtime_ex(
    ctx: &mut Context,
    keymap: Value,
    events: &[Value],
    def: Value,
    remove: bool,
) -> Result<(), Flow> {
    let symbol_mode = ctx.symbol_mode();
    note_keymap_mutation();
    if events.is_empty() {
        return Ok(());
    }
    if events.len() == 1 {
        if remove {
            list_keymap_remove(keymap, events[0], symbol_mode);
        } else {
            list_keymap_define(keymap, events[0], def, symbol_mode);
        }
        return Ok(());
    }

    // Filters can run arbitrary Lisp, including GC and replacing the original
    // bindings. Keep resolved maps, the definition and heap events alive for
    // the entire walk, including maps returned only by a filter.
    let root_scope = ctx.save_specpdl_roots();
    ctx.push_specpdl_root(keymap);
    ctx.push_specpdl_root(def);
    for event in events {
        ctx.push_specpdl_root(*event);
    }
    let result = (|| {
        let mut current_map = keymap;
        for (i, event) in events.iter().enumerate() {
            if i == events.len() - 1 {
                if remove {
                    list_keymap_remove(current_map, *event, symbol_mode);
                } else {
                    list_keymap_define(current_map, *event, def, symbol_mode);
                }
                return Ok(());
            }
            // Use noinherit: only look in current keymap level for prefix,
            // matching GNU Emacs define-key which uses access_keymap(noinherit=1)
            let binding = list_keymap_access_with_event(
                &current_map,
                KeymapLookupEvent::from_event(*event, symbol_mode),
                true,
                false,
                true,
                &mut &mut *ctx,
                symbol_mode,
            )?;
            if let Some(prefix_map) =
                resolve_prefix_keymap_binding_in_obarray(ctx.obarray(), &binding, symbol_mode)
            {
                current_map = prefix_map;
            } else if binding.is_nil() {
                // No binding, create a new prefix keymap.
                // Matches GNU `define_as_prefix` (keymap.c:1446-1452).
                let prefix_map = make_sparse_list_keymap();
                list_keymap_define(current_map, *event, prefix_map, symbol_mode);
                current_map = prefix_map;
            } else {
                // GNU `Fdefine_key` only calls `define_as_prefix` after
                // `access_keymap` returns nil.  Existing non-prefix bindings are
                // errors, even though a fresh undefined key is promoted to a new
                // prefix keymap.
                let full = describe_event_sequence(events, symbol_mode);
                let prefix = describe_event_sequence(&events[..=i], symbol_mode);
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Key sequence {full} starts with non-prefix key {prefix}"
                    ))],
                ));
            }
            ctx.push_specpdl_root(current_map);
        }
        Ok(())
    })();
    ctx.restore_specpdl_roots(root_scope);
    result
}

/// Generate a human-readable description of an event sequence for error messages.
/// Uses the same format as GNU Emacs `key-description`: function keys use
/// angle brackets (e.g., `<f1>`), characters use their standard description.
fn describe_event_sequence(events: &[Value], symbol_mode: SymbolMode) -> String {
    use super::keyboard::pure::describe_single_key_value;
    events
        .iter()
        .map(|e| {
            // An error message is Rust text, so decode here, where the loss is
            // visible and harmless; the Lisp-facing builders keep the bytes.
            describe_single_key_value(e, false, symbol_mode)
                .map(|bytes| crate::emacs_core::emacs_char::to_utf8_lossy(&bytes))
                .unwrap_or_else(|_| {
                    if let Some(name) = e.as_symbol_name(symbol_mode) {
                        format!("<{}>", name)
                    } else {
                        format!("{:?}", e)
                    }
                })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Deep-copy a keymap cons-list structure.
///
/// Mirrors GNU Emacs `copy_keymap_1`:
/// - Copies the cons-list structure
/// - Deep-copies char-tables (via vector clone + recursive entry copy)
/// - Recursively copies sub-keymaps (prefix key maps)
/// - Copies alist bindings whose values are keymaps
/// - Preserves parent keymap as shared (not recursively copied)
pub fn list_keymap_copy(keymap: &Value, symbol_mode: SymbolMode) -> Value {
    list_keymap_copy_impl(keymap, 0, symbol_mode)
}

fn list_keymap_copy_impl(keymap: &Value, depth: usize, symbol_mode: SymbolMode) -> Value {
    if depth > 100 {
        tracing::warn!("list_keymap_copy: recursion depth limit, possible infinite loop");
        return *keymap;
    }

    if !keymap.is_cons() {
        return *keymap;
    };
    let pair_car = keymap.cons_car();
    let pair_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(pair_car, symbol_mode) {
        return *keymap;
    }

    let mut elements = vec![KeymapMarker::Keymap.symbol_value()];
    let mut cursor = pair_cdr;
    let mut tail_parent = Value::NIL;

    while cursor.is_cons() {
        if is_list_keymap(&cursor, symbol_mode) {
            // Parent keymap: keep shared (don't recursively copy parent chain)
            tail_parent = cursor;
            break;
        }
        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();

        if is_char_table(&entry_car) {
            // Deep-copy char-table: clone the vector, then recursively copy
            // any keymap entries within it.
            elements.push(copy_char_table_for_keymap(&entry_car, depth, symbol_mode));
        } else if is_list_keymap(&entry_car, symbol_mode) {
            // Nested keymap element — recursively copy
            elements.push(list_keymap_copy_impl(&entry_car, depth + 1, symbol_mode));
        } else if entry_car.is_cons() {
            // Alist entry (EVENT . DEF) — copy the cons, recurse if DEF is a keymap
            let binding_car = entry_car.cons_car();
            let binding_cdr = entry_car.cons_cdr();
            let copied_def = copy_keymap_item(&binding_cdr, depth, symbol_mode);
            elements.push(Value::cons(binding_car, copied_def));
        } else {
            elements.push(entry_car);
        }

        cursor = entry_cdr;
    }

    // Build the new list
    let mut result = tail_parent;
    for elem in elements.into_iter().rev() {
        result = Value::cons(elem, result);
    }
    result
}

/// Copy a keymap item (the DEF part of an alist entry).
/// If it's a keymap, recursively copy it. Otherwise return as-is.
/// Mirrors GNU `copy_keymap_item`.
fn copy_keymap_item(item: &Value, depth: usize, symbol_mode: SymbolMode) -> Value {
    if is_list_keymap(item, symbol_mode) {
        return list_keymap_copy_impl(item, depth + 1, symbol_mode);
    }
    // Handle menu items etc. — for now, just return as-is for non-keymaps
    *item
}

/// Deep-copy a char-table used in a keymap.
///
/// GNU `copy_keymap_1` first calls `copy-sequence` on the char-table, then
/// walks the copied table with `map_char_table` and replaces each binding with
/// `copy_keymap_item`.
fn copy_char_table_for_keymap(ct: &Value, depth: usize, symbol_mode: SymbolMode) -> Value {
    let Some(copied) = super::chartable::copy_char_table_with_symbols(*ct, symbol_mode) else {
        return *ct;
    };

    let Ok(entries) = super::chartable::char_table_local_entries_with_symbols(&copied, symbol_mode)
    else {
        return copied;
    };
    for (range, value) in entries {
        let copied_value = copy_keymap_item(&value, depth + 1, symbol_mode);
        let _ = super::chartable::builtin_set_char_table_range_with_style(
            vec![copied, range, copied_value],
            None,
            symbol_mode,
        );
    }

    copied
}

/// GNU's initial `meta-prefix-char` (ESC), used when no obarray is available to
/// read the variable from -- a purely structural walk still has to know which
/// event metizes the one after it.
const KEY_META_PREFIX_CHAR_DEFAULT: i64 = 27;

/// Collect all accessible sub-keymaps with their key sequences, mirroring GNU
/// `Faccessible_keymaps` / `accessible_keymaps_1` (keymap.c).
///
/// Breadth-first over a growing queue of `(sequence, map)` pairs: each queued map
/// is scanned with GNU `map_keymap` semantics (embedded submaps and the parent
/// chain share the map's own sequence) and every binding that names a keymap is
/// enqueued. Three GNU rules this walk must not drop:
///
/// * A binding is a prefix iff `get_keymap (get_keyelt (cmd, 0), 0, 0)` yields a
///   keymap, so a SYMBOL whose function cell is a keymap counts. That is what
///   `define-prefix-command` builds and how the global map stores every one of
///   its own prefixes -- `C-x` as `Control-X-prefix`, `C-c` as
///   `mode-specific-command-prefix`, `ESC` as `ESC-prefix`. Testing the raw
///   binding for keymap-ness instead made the entire global keymap tree
///   invisible, which is what emptied `describe-bindings`' global section.
/// * When the sequence so far ends in `meta-prefix-char`, a character key
///   REPLACES that ESC and gains the meta bit instead of extending the sequence,
///   so `ESC s` is reported as `[M-s]`. The metized entry keeps its parent's
///   length, so GNU splices it in directly after the map being scanned rather
///   than appending it, and successive metized finds land in reverse discovery
///   order.
/// * A map already reached by a sequence that is a prefix of the current one is
///   a cycle and is not re-enqueued; reaching it by an unrelated sequence is a
///   legitimate second listing.
///
/// `prefix` restricts the walk the way GNU's PREFIX argument does -- the walk
/// STARTS at the map that sequence reaches, rather than enumerating everything
/// and filtering, because the metization rule makes those two differ: GNU
/// deliberately does not metize the key after a PREFIX that itself ends in ESC.
/// An empty `prefix` walks from KEYMAP itself. Yields nothing when PREFIX reaches
/// no keymap, as GNU returns nil.
pub fn list_keymap_accessible(
    keymap: Value,
    prefix: &[Value],
    obarray: Option<VariableView<'_>>,
    out: &mut Vec<Value>,
    symbol_mode: SymbolMode,
) {
    let meta_prefix_char = obarray
        .and_then(|o| KeymapStateVariable::MetaPrefixChar.global_value(o))
        .and_then(|v| v.as_fixnum())
        .unwrap_or(KEY_META_PREFIX_CHAR_DEFAULT);

    let start = if prefix.is_empty() {
        KeymapResolution::Loaded(keymap)
    } else {
        let reached = match obarray {
            Some(vars) => lookup_key_in_obarray(vars.obarray(), &keymap, prefix, true, symbol_mode),
            None => Value::NIL,
        };
        // GNU keeps an autoload symbol the prefix reaches: "If the keymap is
        // autoloaded `tem' is not a cons-cell, but we still want to return it."
        match resolve_keymap_or_autoload(reached, obarray.map(|vars| vars.obarray()), symbol_mode) {
            Some(resolution) => resolution,
            None => return,
        }
    };

    let prefixlen = prefix.len();
    let mut maps: Vec<(Vec<Value>, KeymapResolution)> = vec![(prefix.to_vec(), start)];
    let mut i = 0;
    while i < maps.len() {
        let thisseq = maps[i].0.clone();
        // GNU scans only loaded maps: "Since we can't run lisp code, we can't
        // scan autoloaded maps." (`if (CONSP (thismap))`).  An unloaded
        // autoload symbol stays LISTED but contributes no descent.
        let KeymapResolution::Loaded(thismap) = maps[i].1 else {
            i += 1;
            continue;
        };
        // GNU's insertion point for a metized find: directly after the map being
        // scanned, so the new same-length sequence stays in breadth order.
        let splice_at = i + 1;
        i += 1;

        // "Does the current sequence end in the meta-prefix-char?", minus the
        // last character of PREFIX itself, which GNU refuses to metize.
        let seq_ends_in_meta_prefix = thisseq.len() > prefixlen
            && thisseq.last().and_then(|event| event.as_fixnum()) == Some(meta_prefix_char);

        let mut found: Vec<(Vec<Value>, KeymapResolution, bool)> = Vec::new();
        list_keymap_for_each_binding_recursive(
            &thismap,
            obarray.map(|vars| vars.obarray()),
            |key, def| {
                let Some(submap) = resolve_keymap_or_autoload(
                    get_keyelt(def, symbol_mode),
                    obarray.map(|vars| vars.obarray()),
                    symbol_mode,
                ) else {
                    return;
                };
                let mut newseq = thisseq.clone();
                let metized = match (seq_ends_in_meta_prefix, key.as_fixnum()) {
                    (true, Some(code)) => {
                        *newseq
                            .last_mut()
                            .expect("non-empty by seq_ends_in_meta_prefix") =
                            Value::fixnum(code | KEY_CHAR_META);
                        true
                    }
                    _ => {
                        newseq.push(key);
                        false
                    }
                };
                found.push((newseq, submap, metized));
            },
            symbol_mode,
        );

        for (newseq, submap, metized) in found {
            let is_cycle = maps.iter().any(|(seen_seq, seen_map)| {
                // GNU's cycle test is Frassq, i.e. EQ on the reached map --
                // which may be a cons OR an autoload symbol.
                eq_value_swp(
                    &seen_map.as_value(),
                    &submap.as_value(),
                    symbol_mode == SymbolMode::Positioned,
                ) && seen_seq.len() <= thisseq.len()
                    && seen_seq
                        .iter()
                        .zip(thisseq.iter())
                        .all(|(a, b)| eq_value_swp(a, b, symbol_mode == SymbolMode::Positioned))
            });
            if is_cycle {
                continue;
            }
            if metized {
                maps.insert(splice_at, (newseq, submap));
            } else {
                maps.push((newseq, submap));
            }
        }
    }

    for (sequence, map) in &maps {
        out.push(Value::cons(Value::vector(sequence.clone()), map.as_value()));
    }
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
