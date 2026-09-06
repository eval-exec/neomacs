//! Keymap system — key binding lookup and command dispatch.
//!
//! Provides an Emacs-compatible keymap system with:
//! - Sparse and full keymaps
//! - Parent (inheritance) chain lookup
//! - Key description parsing (`kbd` style: "C-x C-f", "M-x", "RET", etc.)
//! - Global and local (buffer) keymap support

use crate::emacs_core::error::LispCondition;
use std::collections::HashSet;

use super::builtins::expect_integer_or_marker_in_buffers;
use super::chartable::{
    builtin_char_table_range, builtin_set_char_table_range, char_table_ascii_cache_range,
    char_table_data_start, is_char_table, make_char_table_value,
};
use super::error::{EvalResult, Flow, signal};
use super::eval::Context;
use super::intern::resolve_sym;
use super::intern::{SymId, intern};
use super::keyboard::pure::{
    KEY_CHAR_ALT, KEY_CHAR_CODE_MASK, KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META,
    KEY_CHAR_MOD_MASK, KEY_CHAR_SHIFT, KEY_CHAR_SUPER, reorder_event_symbol_modifiers,
};
use super::symbol::Obarray;
use super::value::{OrderedRuntimeBindingMap, Value, ValueKind, eq_value, list_to_vec};
use strum::{EnumString, IntoStaticStr};

/// Global keymap content-mutation epoch.
///
/// Bumped at the `define-key` and `set-keymap-parent` chokepoints so caches
/// whose contract requires immediate keymap-content freshness (for example
/// the interactive-spec command cache) can reject an old projection.
///
/// Same caveat as `SYNTAX_TABLE_MUTATION_EPOCH`: keymaps are ordinary
/// conses, so raw `setcdr` surgery bypasses this. Consumers must opt into
/// this epoch only when their own freshness contract calls for eager mutation
/// tracking; GNU's frame menu-bar cache intentionally does not.
static KEYMAP_MUTATION_EPOCH: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// Generation of keymap contents observed by a derived-data cache.
///
/// Keeping this distinct from unrelated counters prevents a cache from being
/// accidentally keyed by (for example) a redisplay generation merely because
/// both happen to be represented by `u64`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct KeymapMutationEpoch(u64);

/// Current keymap mutation epoch (see [`KEYMAP_MUTATION_EPOCH`]).
pub fn keymap_mutation_epoch() -> KeymapMutationEpoch {
    KeymapMutationEpoch(KEYMAP_MUTATION_EPOCH.load(std::sync::atomic::Ordering::Relaxed))
}

/// Record a keymap content mutation. Over-invalidation is harmless, so
/// callers may bump before knowing whether the mutation succeeds.
pub(crate) fn note_keymap_mutation() {
    KEYMAP_MUTATION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum KeymapMarker {
    Keymap,
    MenuItem,
    Remap,
}

impl KeymapMarker {
    pub fn symbol_name(self) -> &'static str {
        self.into()
    }

    /// The exact canonical symbol object GNU names `Qkeymap`, `Qmenu_item`,
    /// or `Qremap`.
    ///
    /// GNU keymap.c tests these markers with `EQ`, not by comparing their
    /// printed names. Keeping one typed ID per enum variant both preserves
    /// that identity rule and avoids decoding a symbol name in hot keymap
    /// walks. The exhaustive match makes a newly added marker choose its own
    /// canonical-symbol cache at compile time.
    pub fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static KEYMAP: OnceLock<SymId> = OnceLock::new();
        static MENU_ITEM: OnceLock<SymId> = OnceLock::new();
        static REMAP: OnceLock<SymId> = OnceLock::new();

        match self {
            Self::Keymap => *KEYMAP.get_or_init(|| intern("keymap")),
            Self::MenuItem => *MENU_ITEM.get_or_init(|| intern("menu-item")),
            Self::Remap => *REMAP.get_or_init(|| intern("remap")),
        }
    }

    pub fn symbol_value(self) -> Value {
        Value::from_sym_id(self.symbol_id())
    }

    pub fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    pub fn from_value(value: Value) -> Option<Self> {
        let symbol = value.as_symbol_id()?;
        [Self::Keymap, Self::MenuItem, Self::Remap]
            .into_iter()
            .find(|marker| symbol == marker.symbol_id())
    }

    pub fn is_value(self, value: Value) -> bool {
        value.as_symbol_id() == Some(self.symbol_id())
    }
}

/// Lisp variables whose values are native inputs to GNU's active-keymap
/// collector.
///
/// GNU stores these as predeclared `Lisp_Object` globals (`V...` fields), so a
/// key lookup reads their identities directly.  Keeping the corresponding
/// Neomacs identities behind a closed enum prevents hot callers from falling
/// back to string-based lookup and makes every newly modeled variable choose a
/// cache slot at compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum KeymapStateVariable {
    EmulationModeMapAlists,
    MetaPrefixChar,
    MinorModeMapAlist,
    MinorModeOverridingMapAlist,
    OverridingLocalMap,
    OverridingLocalMapMenuFlag,
    OverridingTerminalLocalMap,
}

impl KeymapStateVariable {
    fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static EMULATION_MODE_MAP_ALISTS: OnceLock<SymId> = OnceLock::new();
        static META_PREFIX_CHAR: OnceLock<SymId> = OnceLock::new();
        static MINOR_MODE_MAP_ALIST: OnceLock<SymId> = OnceLock::new();
        static MINOR_MODE_OVERRIDING_MAP_ALIST: OnceLock<SymId> = OnceLock::new();
        static OVERRIDING_LOCAL_MAP: OnceLock<SymId> = OnceLock::new();
        static OVERRIDING_LOCAL_MAP_MENU_FLAG: OnceLock<SymId> = OnceLock::new();
        static OVERRIDING_TERMINAL_LOCAL_MAP: OnceLock<SymId> = OnceLock::new();

        match self {
            Self::EmulationModeMapAlists => {
                *EMULATION_MODE_MAP_ALISTS.get_or_init(|| intern(self.into()))
            }
            Self::MetaPrefixChar => *META_PREFIX_CHAR.get_or_init(|| intern(self.into())),
            Self::MinorModeMapAlist => *MINOR_MODE_MAP_ALIST.get_or_init(|| intern(self.into())),
            Self::MinorModeOverridingMapAlist => {
                *MINOR_MODE_OVERRIDING_MAP_ALIST.get_or_init(|| intern(self.into()))
            }
            Self::OverridingLocalMap => *OVERRIDING_LOCAL_MAP.get_or_init(|| intern(self.into())),
            Self::OverridingLocalMapMenuFlag => {
                *OVERRIDING_LOCAL_MAP_MENU_FLAG.get_or_init(|| intern(self.into()))
            }
            Self::OverridingTerminalLocalMap => {
                *OVERRIDING_TERMINAL_LOCAL_MAP.get_or_init(|| intern(self.into()))
            }
        }
    }

    fn global_value(self, obarray: &Obarray) -> Option<Value> {
        obarray.symbol_value_id(self.symbol_id()).copied()
    }

    fn buffer_or_global_value(
        self,
        obarray: &Obarray,
        buffers: &crate::buffer::BufferManager,
        buffer_id: Option<crate::buffer::BufferId>,
    ) -> Option<Value> {
        dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            obarray,
            &[],
            buffers,
            buffer_id,
            self.symbol_id(),
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, EnumString, IntoStaticStr)]
#[strum(prefix = ":", serialize_all = "kebab-case")]
pub enum MenuItemProperty {
    Enable,
    Visible,
    Help,
    Filter,
    Button,
    Keys,
    KeySequence,
    Image,
    Rtl,
    Wrap,
    Label,
    VertOnly,
}

impl MenuItemProperty {
    pub fn keyword(self) -> &'static str {
        self.into()
    }

    pub fn from_keyword(name: &str) -> Option<Self> {
        name.strip_prefix(':')?.parse().ok()
    }

    pub fn from_value(value: Value) -> Option<Self> {
        Self::from_keyword(value.as_symbol_name()?)
    }

    pub fn is_value(self, value: Value) -> bool {
        Self::from_value(value) == Some(self)
    }
}

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

    pub fn from_value(value: Value) -> Option<Self> {
        Self::from_keyword(value.as_symbol_name()?)
    }
}

// ---------------------------------------------------------------------------
// Key events
// ---------------------------------------------------------------------------

/// A key event — a single keystroke with optional modifiers.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum KeyEvent {
    /// A regular character with modifiers.
    Char {
        code: char,
        ctrl: bool,
        meta: bool,
        shift: bool,
        super_: bool,
        hyper: bool,
        alt: bool,
    },
    /// A named function/special key (e.g. "return", "backspace", "f1").
    Function {
        name: SymId,
        ctrl: bool,
        meta: bool,
        shift: bool,
        super_: bool,
        hyper: bool,
        alt: bool,
    },
}

// ---------------------------------------------------------------------------
// Conversion from keyboard::KeyEvent → keymap::KeyEvent
// ---------------------------------------------------------------------------

impl From<crate::keyboard::KeyEvent> for KeyEvent {
    fn from(ke: crate::keyboard::KeyEvent) -> Self {
        use crate::keyboard::{Key, NamedKey};
        match ke.key {
            Key::Char(c) => KeyEvent::Char {
                code: c,
                ctrl: ke.modifiers.ctrl,
                meta: ke.modifiers.meta,
                shift: ke.modifiers.shift,
                super_: ke.modifiers.super_,
                hyper: ke.modifiers.hyper,
                alt: false,
            },
            Key::Named(named) => {
                let name = match named {
                    NamedKey::Escape => "escape",
                    NamedKey::Return => "return",
                    NamedKey::Tab => "tab",
                    NamedKey::Backspace => "backspace",
                    NamedKey::Delete => "delete",
                    NamedKey::Insert => "insert",
                    NamedKey::Home => "home",
                    NamedKey::End => "end",
                    NamedKey::PageUp => "prior",
                    NamedKey::PageDown => "next",
                    NamedKey::Left => "left",
                    NamedKey::Right => "right",
                    NamedKey::Up => "up",
                    NamedKey::Down => "down",
                    NamedKey::F(n) => {
                        return KeyEvent::Function {
                            name: intern(&format!("f{}", n)),
                            ctrl: ke.modifiers.ctrl,
                            meta: ke.modifiers.meta,
                            shift: ke.modifiers.shift,
                            super_: ke.modifiers.super_,
                            hyper: ke.modifiers.hyper,
                            alt: false,
                        };
                    }
                };
                KeyEvent::Function {
                    name: intern(name),
                    ctrl: ke.modifiers.ctrl,
                    meta: ke.modifiers.meta,
                    shift: ke.modifiers.shift,
                    super_: ke.modifiers.super_,
                    hyper: ke.modifiers.hyper,
                    alt: false,
                }
            }
        }
    }
}

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

/// Create a sparse list keymap: `(keymap)` — a single-element list.
pub fn make_sparse_list_keymap() -> Value {
    Value::list(vec![KeymapMarker::Keymap.symbol_value()])
}

/// Check if a value is a keymap: `(consp x) && (car x) == 'keymap`.
pub fn is_list_keymap(v: &Value) -> bool {
    match v.kind() {
        ValueKind::Cons => KeymapMarker::Keymap.is_value(v.cons_car()),
        _ => false,
    }
}

fn keymap_symbol_id(value: &Value) -> Option<SymId> {
    match value.kind() {
        ValueKind::Nil => Some(intern("nil")),
        ValueKind::T => Some(intern("t")),
        ValueKind::Symbol(id) => Some(id),
        _ => None,
    }
}

fn resolve_indirect_function_by_id_in_obarray(
    obarray: &Obarray,
    symbol: SymId,
) -> Option<(SymId, Value)> {
    let mut current = symbol;
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(current) {
            return None;
        }
        let sym = obarray.get_by_id(current)?;
        if sym.function.is_nil() {
            return None;
        }
        let function = sym.function;
        if let Some(next_symbol) = keymap_symbol_id(&function) {
            current = next_symbol;
            continue;
        }
        return Some((current, function));
    }
}

pub(crate) fn is_keymap_autoload_form(value: &Value) -> bool {
    if !crate::emacs_core::autoload::is_autoload_value(value) {
        return false;
    }
    list_to_vec(value)
        .and_then(|items| items.get(4).copied())
        .is_some_and(|kind| KeymapMarker::Keymap.is_value(kind))
}

pub(crate) fn get_keymap_in_obarray(
    obarray: &Obarray,
    value: &Value,
    error_if_not_keymap: bool,
) -> Result<Value, Flow> {
    if value.is_nil() {
        return if error_if_not_keymap {
            Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("keymapp"), *value],
            ))
        } else {
            Ok(Value::NIL)
        };
    }

    if is_list_keymap(value) {
        return Ok(*value);
    }

    if let Some(symbol) = keymap_symbol_id(value)
        && let Some((_, function)) = resolve_indirect_function_by_id_in_obarray(obarray, symbol)
    {
        if is_list_keymap(&function) {
            return Ok(function);
        }
        if is_keymap_autoload_form(&function) && !error_if_not_keymap {
            return Ok(*value);
        }
    }

    if error_if_not_keymap {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("keymapp"), *value],
        ))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn maybe_keymap_in_obarray(obarray: &Obarray, value: &Value) -> Option<Value> {
    get_keymap_in_obarray(obarray, value, false)
        .ok()
        .filter(is_list_keymap)
}

pub(crate) fn get_keymap_in_runtime(
    eval: &mut Context,
    value: &Value,
    error_if_not_keymap: bool,
    autoload: bool,
) -> EvalResult {
    let original = *value;
    let mut current = original;

    loop {
        if current.is_nil() {
            break;
        }
        if is_list_keymap(&current) {
            return Ok(current);
        }

        let Some(symbol) = keymap_symbol_id(&current) else {
            break;
        };
        let Some((_, function)) =
            resolve_indirect_function_by_id_in_obarray(eval.obarray(), symbol)
        else {
            break;
        };

        if is_list_keymap(&function) {
            return Ok(function);
        }

        if is_keymap_autoload_form(&function) {
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
    let resolved = get_keymap_in_runtime(eval, value, false, autoload)?;
    if is_list_keymap(&resolved) {
        Ok(resolved)
    } else {
        Ok(Value::NIL)
    }
}

/// Strip menu-item wrappers from a keymap binding, mirroring `get_keyelt`
/// in official Emacs `keymap.c`.
///
/// - `(STRING . DEFN)` → `DEFN`  (menu label)
/// - `(STRING . (STRING . DEFN))` → `DEFN`  (menu label + help string)
/// - `(menu-item NAME DEFN ...)` → `DEFN`  (extended menu item)
/// - anything else → returned as-is
pub(crate) fn get_keyelt(binding: Value) -> Value {
    let mut obj = binding;
    loop {
        if !obj.is_cons() {
            return obj;
        };
        let pair_car = obj.cons_car();
        let pair_cdr = obj.cons_cdr();
        if pair_car.is_string() {
            // (STRING . REST) — strip the menu label
            obj = pair_cdr;
            // Also strip a second string (help string)
            if obj.is_cons() {
                let p2_car = obj.cons_car();
                let p2_cdr = obj.cons_cdr();
                if p2_car.is_string() {
                    obj = p2_cdr;
                }
            }
            continue;
        }
        if KeymapMarker::MenuItem.is_value(pair_car) {
            // (menu-item NAME DEFN . PROPS) — extract DEFN (third element)
            if pair_cdr.is_cons() {
                let p1_cdr = pair_cdr.cons_cdr();
                if p1_cdr.is_cons() {
                    let p2_car = p1_cdr.cons_car();
                    return p2_car;
                }
            }
            return Value::NIL;
        }
        return obj;
    }
}

fn nth_list_element(mut list: Value, mut index: usize) -> Option<Value> {
    while list.is_cons() {
        let car = list.cons_car();
        if index == 0 {
            return Some(car);
        }
        index -= 1;
        list = list.cons_cdr();
    }
    None
}

fn menu_item_filter(tail: Value) -> Option<Value> {
    let mut cursor = tail;
    while cursor.is_cons() {
        let key = cursor.cons_car();
        let rest = cursor.cons_cdr();
        if MenuItemProperty::Filter.is_value(key) && rest.is_cons() {
            return Some(rest.cons_car());
        }
        cursor = rest;
    }
    None
}

pub(crate) fn get_keyelt_runtime(eval: &mut Context, binding: Value, autoload: bool) -> EvalResult {
    let mut object = binding;
    loop {
        if !object.is_cons() {
            return Ok(object);
        }

        let pair_car = object.cons_car();
        let pair_cdr = object.cons_cdr();
        if pair_car.is_string() {
            object = pair_cdr;
            if object.is_cons() && object.cons_car().is_string() {
                object = object.cons_cdr();
            }
            continue;
        }

        if KeymapMarker::MenuItem.is_value(pair_car) {
            if !pair_cdr.is_cons() {
                return Ok(object);
            }
            let tail = pair_cdr.cons_cdr();
            object = nth_list_element(pair_cdr, 1).unwrap_or(Value::NIL);
            if autoload && let Some(filter) = menu_item_filter(tail) {
                object = eval.apply(filter, vec![object])?;
            }
            continue;
        }

        return Ok(object);
    }
}

/// Look up a single event in a keymap, following the parent chain.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=false, t_ok=false`.
/// When a prefix keymap is found, it is composed with parent prefix
/// keymaps to create a merged keymap that includes all bindings from
/// the entire inheritance chain.
///
/// Returns the binding or `Value::NIL` if not found.
pub fn list_keymap_lookup_one(keymap: &Value, event: &Value) -> Value {
    list_keymap_access(keymap, event, false, false)
}

/// Look up a single event in a keymap, following the parent chain,
/// accepting `(t . COMMAND)` default bindings.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=false, t_ok=true`.
pub fn list_keymap_lookup_one_t_ok(keymap: &Value, event: &Value) -> Value {
    list_keymap_access(keymap, event, false, true)
}

/// Look up a single event in a keymap without stripping menu-item wrappers.
///
/// This is used by `read-key-sequence` key translation maps.  GNU
/// `access_keymap_keyremap` calls `access_keymap` with autoloading enabled, so
/// menu-item `:filter` properties must be evaluated by the keyboard runtime
/// rather than discarded by the pure keymap lookup layer.
pub fn list_keymap_lookup_one_unresolved(keymap: &Value, event: &Value) -> Value {
    list_keymap_access_unresolved(keymap, event, false, false)
}

/// Look up a single event in a keymap without following the parent chain.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=true`.
/// Used by `define-key` to only check the current keymap level.
pub fn list_keymap_lookup_one_noinherit(keymap: &Value, event: &Value) -> Value {
    list_keymap_access(keymap, event, true, false)
}

/// Look up a single event in one level of a keymap (no parent following).
///
/// Helper: scans only the entries in the given keymap (not parents).
/// Returns `Some(binding)` if found (even if binding is nil), or
/// `None` if not found. This distinction is critical: an explicit
/// nil binding shadows parent bindings, while "not found" falls through.
///
/// When `t_ok` is true, a `(t . COMMAND)` entry is accepted as a
/// default binding, matching GNU `access_keymap_1`'s `t_ok` parameter.
///
/// GNU `access_keymap_1` accepts both full `(keymap ...)` objects and
/// raw cons spines such as the running `tail` in `help.el:describe-map`.
/// Neomacs must do the same so `lookup-key` works on canonicalized
/// keymap tails during help rendering.
fn keymap_binding_spine(keymap: &Value) -> Option<Value> {
    if !keymap.is_cons() {
        return None;
    }

    if KeymapMarker::Keymap.is_value(keymap.cons_car()) {
        return Some(keymap.cons_cdr());
    }

    Some(*keymap)
}

fn maybe_resolve_keyelt(binding: Value, resolve_keyelt: bool) -> Value {
    if resolve_keyelt {
        get_keyelt(binding)
    } else {
        binding
    }
}

/// The single canonical event index GNU's `access_keymap_1` compares against
/// every entry in a keymap spine.
///
/// Keeping this as a distinct type makes it impossible for recursive member
/// and parent scans to accidentally repeat event-head extraction or modifier
/// canonicalization.  Stored alist keys are already canonicalized by
/// `store_in_keymap`; repairing arbitrary keys while reading would both cost a
/// parse per comparison and diverge from GNU.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct KeymapLookupEvent(Value);

impl KeymapLookupEvent {
    fn from_event(event: Value) -> Self {
        let head = if event.is_cons() {
            event.cons_car()
        } else {
            event
        };
        let canonical = match head.kind() {
            ValueKind::Symbol(_) => reorder_event_symbol_modifiers(head),
            // GNU clears bits above the event representation before scanning
            // (`CHAR_META | (CHAR_META - 1)`, src/keymap.c).
            ValueKind::Fixnum(code) => Value::fixnum(code & (KEY_CHAR_META | (KEY_CHAR_META - 1))),
            _ => head,
        };
        Self(canonical)
    }

    fn value(self) -> Value {
        self.0
    }

    fn matches_stored_key(self, stored: Value) -> bool {
        eq_value(&stored, &self.0)
    }
}

fn lookup_in_keymap_level_impl(
    keymap: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    obarray: Option<&Obarray>,
) -> Option<Value> {
    let mut cursor = keymap_binding_spine(keymap)?;
    let mut entries = 0;
    let mut t_binding: Option<Value> = None;
    let mut prefix_binding: Option<Value> = None;
    let mut nil_binding_found = false;
    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            break; // parent boundary
        }
        entries += 1;
        if entries > 100_000 {
            break;
        }
        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();

        // Char-table: only look up characters WITHOUT modifier bits.
        // GNU keymap.c:441-450: nil in char-table means unbound;
        // Qt means explicitly nil binding.
        if is_char_table(&entry_car) {
            if let Some(code) = event.value().as_fixnum()
                && (code & KEY_CHAR_MOD_MASK) == 0
            {
                let base = code & KEY_CHAR_CODE_MASK;
                if (0..=0x3FFFFF).contains(&base) {
                    let result = builtin_char_table_range(vec![entry_car, event.value()], None)
                        .unwrap_or(Value::NIL);
                    if !result.is_nil() {
                        // Qt in char-table means explicitly nil binding
                        // (shadows parent), matching GNU keymap.c:455-459
                        let val = if result == Value::T {
                            Value::NIL
                        } else {
                            result
                        };
                        let val = maybe_resolve_keyelt(val, resolve_keyelt);
                        if val.is_nil() {
                            nil_binding_found = true;
                        } else if is_list_keymap(&val) {
                            prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                        } else if prefix_binding.is_some() {
                            break;
                        } else {
                            return Some(val);
                        }
                    }
                    // nil in char-table means unbound — fall through
                }
            }
            cursor = entry_cdr;
            continue;
        }

        // Vector element in keymap spine: maps char codes 0..len to
        // bindings by index. Matches GNU keymap.c:431-434.
        if entry_car.is_vector() {
            if let Some(code) = event.value().as_fixnum()
                && code >= 0
            {
                let idx = code as usize;
                let items = entry_car.as_vector_data().unwrap();
                if idx < items.len() {
                    let val = items[idx];
                    let val = maybe_resolve_keyelt(val, resolve_keyelt);
                    if val.is_nil() {
                        nil_binding_found = true;
                    } else if is_list_keymap(&val) {
                        prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                    } else if prefix_binding.is_some() {
                        break;
                    } else {
                        return Some(val);
                    }
                }
            }
            cursor = entry_cdr;
            continue;
        }

        // Sub-keymap embedded in spine: composed keymaps created by
        // `make-composed-keymap` / `internal-push-keymap` / `set-transient-map`
        // look like (keymap <sub-keymap> ...).
        //
        // GNU `access_keymap_1` recurses into ITSELF here -- `access_keymap_1
        // (submap, idx, t_ok, noinherit, autoload)` -- so the member is searched
        // WITH its own parent chain, and a prefix it shares with that parent is
        // merged before the composed map's own parent is considered. Searching
        // the member one level deep instead loses everything it inherits, which
        // is why `M-q` in a swiper minibuffer -- bound in `swiper-map`, the
        // parent of the composed member `swiper-isearch-map` -- fell through to
        // the global `fill-paragraph`.
        if entry_car.is_cons() && is_list_keymap(&entry_car) {
            if let Some(found) =
                access_keymap_in_member(&entry_car, event, noinherit, t_ok, resolve_keyelt, obarray)
            {
                if found.is_nil() {
                    nil_binding_found = true;
                } else if is_list_keymap(&found) {
                    prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, found));
                } else if prefix_binding.is_some() {
                    break;
                } else {
                    return Some(found);
                }
            }
            cursor = entry_cdr;
            continue;
        }

        // Alist entry: (EVENT . DEF)
        if entry_car.is_cons() {
            let binding_car = entry_car.cons_car();
            let binding_cdr = entry_car.cons_cdr();
            if event.matches_stored_key(binding_car) {
                let val = maybe_resolve_keyelt(binding_cdr, resolve_keyelt);
                if val.is_nil() {
                    nil_binding_found = true;
                } else if is_list_keymap(&val) {
                    prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                } else if prefix_binding.is_some() {
                    break;
                } else {
                    return Some(val);
                }
            }
            // Check for (t . COMMAND) default binding.
            // GNU keymap.c:425-429: when t_ok, record the first t binding
            // but keep scanning for a specific match.
            if t_ok && t_binding.is_none() && binding_car == Value::T {
                t_binding = Some(maybe_resolve_keyelt(binding_cdr, resolve_keyelt));
            }
        }

        cursor = entry_cdr;

        // GNU `access_keymap_1`'s loop condition retries `get_keymap` as soon as
        // the tail stops being a cons, so a tail that NAMES a keymap continues
        // this same scan -- its bindings belong to this keymap, not to a parent.
        if !cursor.is_cons()
            && !cursor.is_nil()
            && let Some(resolved) = resolve_keymap(cursor, obarray)
            && let Some(spine) = keymap_binding_spine(&resolved)
        {
            cursor = spine;
        }
    }

    // If no specific binding found but we have a t default binding, use it.
    // Matches GNU keymap.c:486-487.
    if let Some(binding) = prefix_binding {
        Some(binding)
    } else if nil_binding_found {
        Some(Value::NIL)
    } else {
        t_binding
    }
}

/// One member of a composed keymap, searched the way GNU searches it: with its
/// own parent chain, and reporting "no entry here" apart from "an entry that is
/// nil".
///
/// That distinction is why this cannot just call [`list_keymap_access_impl`],
/// which returns nil for both: an explicit nil entry SHADOWS the members and
/// parents that follow, while no entry at all lets them through.
fn access_keymap_in_member(
    member: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    obarray: Option<&Obarray>,
) -> Option<Value> {
    let found =
        lookup_in_keymap_level_impl(member, event, noinherit, t_ok, resolve_keyelt, obarray);
    if noinherit {
        return found;
    }
    let parent = get_keymap_tail_parent(member);
    match found {
        // A prefix keymap merges with whatever the member's parent binds for the
        // same event, exactly as it would if the member were looked up directly.
        Some(binding) if is_list_keymap(&binding) && !parent.is_nil() => {
            let parent_binding =
                list_keymap_access_with_event(&parent, event, false, t_ok, resolve_keyelt, obarray);
            if is_list_keymap(&parent_binding) {
                Some(compose_prefix_with_parent_keymap(&binding, &parent_binding))
            } else {
                Some(binding)
            }
        }
        Some(binding) => Some(binding),
        // Nothing at the member's own level: its parent chain still applies.
        None if !parent.is_nil() => {
            access_keymap_in_member(&parent, event, noinherit, t_ok, resolve_keyelt, obarray)
        }
        None => None,
    }
}

/// Get the parent keymap from a keymap (the tail after all alist entries).
fn get_keymap_tail_parent(keymap: &Value) -> Value {
    let Some(mut cursor) = keymap_binding_spine(keymap) else {
        return Value::NIL;
    };
    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            return cursor;
        }
        cursor = cursor.cons_cdr();
    }
    Value::NIL
}

/// Core event lookup in a keymap, optionally following the parent chain.
///
/// Mirrors GNU Emacs `access_keymap`:
/// - Walks the keymap list scanning bindings (char-tables, alist entries)
/// - When `noinherit` is false: follows parent keymap chain; if a prefix
///   keymap is found, it composes it with prefix keymaps from parent
///   levels to create a proper inheritance chain
/// - When `noinherit` is true: stops at the first parent boundary
/// - When `t_ok` is true: accepts `(t . COMMAND)` default bindings
///
/// An explicit nil binding (e.g. from `define-key m [?b] nil`) shadows
/// parent bindings, matching GNU Emacs behavior where nil != unbound.
fn list_keymap_access(keymap: &Value, event: &Value, noinherit: bool, t_ok: bool) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, true, None)
}

/// [`list_keymap_access`] with the obarray GNU's `access_keymap_1` resolves a
/// symbol spine tail through (`get_keymap`), for the lookup paths that have one.
fn list_keymap_access_in_obarray(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, true, Some(obarray))
}

fn list_keymap_access_unresolved(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, false, None)
}

/// [`list_keymap_access_unresolved`] with a symbol-tail-resolving obarray.
fn list_keymap_access_unresolved_in_obarray(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, false, Some(obarray))
}

/// Look up ONE event, following a spine tail that names a keymap -- the lookup
/// GNU's `access_keymap` performs. Used by the `lookup-key` paths, which have the
/// obarray in hand; the obarray-less variants above stay structural.
pub(crate) fn list_keymap_lookup_one_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_in_obarray(keymap, event, false, false, obarray)
}

pub(crate) fn list_keymap_lookup_one_t_ok_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_in_obarray(keymap, event, false, true, obarray)
}

pub(crate) fn list_keymap_lookup_one_unresolved_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_unresolved_in_obarray(keymap, event, false, false, obarray)
}

pub(crate) fn list_keymap_lookup_one_unresolved_t_ok_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
) -> Value {
    list_keymap_access_unresolved_in_obarray(keymap, event, false, true, obarray)
}

fn list_keymap_access_with_event(
    keymap: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    obarray: Option<&Obarray>,
) -> Value {
    let mut current = *keymap;
    let mut depth = 0;
    const MAX_KEYMAP_DEPTH: usize = 50;

    loop {
        depth += 1;
        if depth > MAX_KEYMAP_DEPTH {
            tracing::warn!("list_keymap_access: depth limit reached, possible cycle");
            return Value::NIL;
        }

        // Look up the event in the current keymap level only.
        // Some(val) means "found" (val may be nil for explicit nil binding).
        // None means "not found at this level".
        match lookup_in_keymap_level_impl(&current, event, noinherit, t_ok, resolve_keyelt, obarray)
        {
            Some(binding) => {
                if !noinherit && is_list_keymap(&binding) {
                    // Found a prefix keymap at this level. Check if parent
                    // also has a prefix keymap for the same event. If so,
                    // create a composed keymap: (keymap child-sub . parent-sub)
                    let parent = get_keymap_tail_parent(&current);
                    if !parent.is_nil() {
                        let parent_binding = list_keymap_access_with_event(
                            &parent,
                            event,
                            false,
                            t_ok,
                            resolve_keyelt,
                            obarray,
                        );
                        if is_list_keymap(&parent_binding) {
                            return compose_prefix_with_parent_keymap(&binding, &parent_binding);
                        }
                    }
                }
                // Return the found binding (even if nil — nil shadows parents)
                return binding;
            }
            None => {
                // No binding at this level. Follow parent chain if allowed.
                if noinherit {
                    return Value::NIL;
                }
                let parent = get_keymap_tail_parent(&current);
                if parent.is_nil() {
                    return Value::NIL;
                }
                current = parent;
            }
        }
    }
}

fn list_keymap_access_impl(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    obarray: Option<&Obarray>,
) -> Value {
    list_keymap_access_with_event(
        keymap,
        KeymapLookupEvent::from_event(*event),
        noinherit,
        t_ok,
        resolve_keyelt,
        obarray,
    )
}

fn accumulate_prefix_keymap(existing: Option<Value>, next: Value) -> Value {
    match existing {
        Some(current) => compose_prefix_keymaps(&current, &next),
        None => next,
    }
}

/// Compose prefix keymaps found at the same keymap level.  GNU
/// `access_keymap_1` keeps those as separate embedded members:
/// `(keymap MAP1 MAP2 ...)`.
fn compose_prefix_keymaps(first: &Value, second: &Value) -> Value {
    Value::list(vec![KeymapMarker::Keymap.symbol_value(), *first, *second])
}

/// Compose a direct prefix map with an inherited parent prefix map.  GNU
/// `access_keymap_1` keeps the inherited keymap marker as the tail boundary,
/// just like `make-composed-keymap` in `subr.el`: `(keymap CHILD . PARENT)`.
///
/// This distinction matters for `define-key` on a composed prefix map:
/// `access_keymap(..., noinherit=true)` must stop at the parent boundary so the
/// write goes into CHILD rather than mutating an inherited map.
fn compose_prefix_with_parent_keymap(child: &Value, parent: &Value) -> Value {
    if is_list_keymap(parent) {
        Value::cons(
            KeymapMarker::Keymap.symbol_value(),
            Value::cons(*child, *parent),
        )
    } else {
        compose_prefix_keymaps(child, parent)
    }
}

pub(crate) fn expand_meta_prefix_char_events_in_obarray(
    obarray: &Obarray,
    events: &[Value],
) -> Option<Vec<Value>> {
    let meta_prefix = KeymapStateVariable::MetaPrefixChar
        .global_value(obarray)
        .and_then(|v| v.as_fixnum())?;

    let mut changed = false;
    let mut expanded = Vec::with_capacity(events.len() + 1);
    for event in events {
        match event.kind() {
            ValueKind::Fixnum(code) if (code & KEY_CHAR_META) != 0 => {
                changed = true;
                expanded.push(Value::fixnum(meta_prefix));
                expanded.push(Value::fixnum(code & !KEY_CHAR_META));
            }
            _ => expanded.push(*event),
        }
    }

    changed.then_some(expanded)
}

pub(crate) fn resolve_prefix_keymap_binding_in_obarray(
    obarray: &Obarray,
    binding: &Value,
) -> Option<Value> {
    if is_list_keymap(binding) {
        return Some(*binding);
    }
    maybe_keymap_in_obarray(obarray, binding)
}

pub(crate) fn lookup_key_in_obarray(
    obarray: &Obarray,
    keymap: &Value,
    events: &[Value],
    t_ok: bool,
) -> Value {
    if events.is_empty() {
        return *keymap;
    }

    let mut current_map = *keymap;
    for (i, event) in events.iter().enumerate() {
        let binding = if t_ok {
            list_keymap_lookup_one_t_ok_in_obarray(&current_map, event, obarray)
        } else {
            list_keymap_lookup_one_in_obarray(&current_map, event, obarray)
        };
        let is_last = i == events.len() - 1;

        if is_last {
            return binding;
        }

        if binding.is_nil() {
            return Value::fixnum((i + 1) as i64);
        }

        if let Some(prefix_keymap) = resolve_prefix_keymap_binding_in_obarray(obarray, &binding) {
            current_map = prefix_keymap;
            continue;
        }

        return Value::fixnum((i + 1) as i64);
    }

    Value::NIL
}

pub(crate) fn lookup_key_in_keymaps_in_obarray(
    obarray: &Obarray,
    keymaps: &[Value],
    events: &[Value],
    t_ok: bool,
) -> Value {
    if events.is_empty() {
        return keymaps.first().copied().unwrap_or(Value::NIL);
    }

    let mut best = Value::NIL;
    for keymap in keymaps {
        let direct = lookup_key_in_obarray(obarray, keymap, events, t_ok);
        if !direct.is_nil() && !direct.is_fixnum() {
            return direct;
        }

        if let Some(expanded) = expand_meta_prefix_char_events_in_obarray(obarray, events) {
            let expanded_result = lookup_key_in_obarray(obarray, keymap, &expanded, t_ok);
            if !expanded_result.is_nil() && !expanded_result.is_fixnum() {
                return expanded_result;
            }
        }

        if best.is_nil() {
            best = direct;
        }
    }

    best
}

pub(crate) fn lookup_key_in_obarray_runtime(
    ctx: &mut Context,
    keymap: Value,
    events: &[Value],
    t_ok: bool,
) -> EvalResult {
    if events.is_empty() {
        return Ok(keymap);
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        let raw_binding = if t_ok {
            list_keymap_lookup_one_unresolved_t_ok_in_obarray(&current_map, event, ctx.obarray())
        } else {
            list_keymap_lookup_one_unresolved_in_obarray(&current_map, event, ctx.obarray())
        };
        let is_last = i == events.len() - 1;
        let binding = get_keyelt_runtime(ctx, raw_binding, true)?;

        if is_last {
            return Ok(binding);
        }

        if binding.is_nil() {
            return Ok(Value::fixnum((i + 1) as i64));
        }

        if let Some(prefix_keymap) =
            resolve_prefix_keymap_binding_in_obarray(&ctx.obarray, &binding)
        {
            current_map = prefix_keymap;
            continue;
        }

        return Ok(Value::fixnum((i + 1) as i64));
    }

    Ok(Value::NIL)
}

pub(crate) fn lookup_key_in_keymaps_in_obarray_runtime(
    ctx: &mut Context,
    keymaps: &[Value],
    events: &[Value],
    t_ok: bool,
) -> EvalResult {
    if events.is_empty() {
        return Ok(keymaps.first().copied().unwrap_or(Value::NIL));
    }

    let mut best = Value::NIL;
    for keymap in keymaps {
        let direct = lookup_key_in_obarray_runtime(ctx, *keymap, events, t_ok)?;
        if !direct.is_nil() && !direct.is_fixnum() {
            return Ok(direct);
        }

        if let Some(expanded) = expand_meta_prefix_char_events_in_obarray(&ctx.obarray, events) {
            let expanded_result = lookup_key_in_obarray_runtime(ctx, *keymap, &expanded, t_ok)?;
            if !expanded_result.is_nil() && !expanded_result.is_fixnum() {
                return Ok(expanded_result);
            }
        }

        if best.is_nil() {
            best = direct;
        }
    }

    Ok(best)
}

/// Define a binding in a keymap.
///
/// For integer events without modifier bits in full keymaps: stores in char-table.
/// Otherwise: updates existing alist entry in-place or prepends `(event . def)`.
///
/// When `remove` is true, removes the binding entry from the alist (or
/// stores nil in char-table), matching GNU keymap.c `store_in_keymap` with
/// `remove=true`.
pub fn list_keymap_define(keymap: Value, event: Value, def: Value) {
    note_keymap_mutation();
    store_in_keymap(keymap, event, def, false);
}

/// Remove a binding from a keymap, matching GNU `define-key` with REMOVE arg.
pub fn list_keymap_remove(keymap: Value, event: Value) {
    store_in_keymap(keymap, event, Value::NIL, true);
}

fn keymap_character_range(event: &Value) -> Option<(i64, i64)> {
    if !event.is_cons() {
        return None;
    }
    let from = event.cons_car().as_fixnum()?;
    let to = event.cons_cdr().as_fixnum()?;
    (0..=KEY_CHAR_CODE_MASK).contains(&from).then_some(())?;
    (0..=KEY_CHAR_CODE_MASK).contains(&to).then_some(())?;
    (from <= to).then_some((from, to))
}

fn keymap_storage_event(event: Value) -> Value {
    if event.is_cons() && keymap_character_range(&event).is_none() {
        reorder_event_symbol_modifiers(event.cons_car())
    } else {
        reorder_event_symbol_modifiers(event)
    }
}

fn keymap_char_table_store_value(def: Value, remove: bool) -> Value {
    if remove {
        Value::NIL
    } else if def.is_nil() {
        // nil has special meaning for char-tables (unbound), so use Qt for an
        // explicitly nil key binding.
        Value::T
    } else {
        def
    }
}

fn keymap_delq_after(insertion_point: Value, elt: Value) {
    let mut previous = insertion_point;
    let mut cursor = insertion_point.cons_cdr();
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        let next = cursor.cons_cdr();
        if eq_value(&entry, &elt) {
            previous.set_cdr(next);
            return;
        }
        previous = cursor;
        cursor = next;
    }
}

/// Core store/remove implementation matching GNU `store_in_keymap`.
fn store_in_keymap(keymap: Value, event: Value, def: Value, remove: bool) {
    if !keymap.is_cons() {
        return;
    };
    let root_car = keymap.cons_car();
    let root_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(root_car) {
        return;
    }
    let event = keymap_storage_event(event);

    // Scan the keymap for existing bindings, tracking insertion point.
    //
    // GNU `keymap.c:779-880`: `insertion_point` starts at `keymap`
    // (the head cons holding the `keymap` symbol). It is **only**
    // advanced when a vector or char-table element is encountered, so
    // that those high-density elements stay at the front of the alist
    // and character lookups stay fast. For ordinary alist bindings
    // `insertion_point` stays at the head, which means the final
    // `XSETCDR (insertion_point, Fcons (elt, XCDR (insertion_point)))`
    // **prepends** the new entry right after the `keymap` symbol —
    // i.e. newest binding first.
    //
    // The previous neomacs implementation advanced `insertion_point`
    // on every iteration (including alist entries), so new bindings
    // were appended at the **tail** instead. That divergence is
    // observable: it inverts the order produced by `map_keymap_canonical`
    // / `list_keymap_for_each_binding`, which surfaces e.g. as a
    // backwards menu bar (`menu-bar.el` calls `define-key global-map
    // [menu-bar tools]` then `[menu-bar buffer]` etc., and walking
    // the resulting keymap should yield them in reverse insertion
    // order — newest first — to match GNU's `display_menu_bar`).
    let mut insertion_point = keymap;
    let mut cursor = root_cdr;
    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            // Hit a parent keymap boundary — stop scanning
            break;
        }
        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();

        // Char-table: handle plain character events and character ranges.
        // GNU keymap.c:805-829
        if is_char_table(&entry_car) {
            if let Some(code) = event.as_fixnum() {
                let mods = code & KEY_CHAR_MOD_MASK;
                if mods == 0 {
                    let base = code & KEY_CHAR_CODE_MASK;
                    if (0..=0x3FFFFF).contains(&base) {
                        let store_val = keymap_char_table_store_value(def, remove);
                        let _ =
                            builtin_set_char_table_range(vec![entry_car, event, store_val], None);
                        return;
                    }
                }
            } else if event.is_cons() && keymap_character_range(&event).is_some() {
                let store_val = keymap_char_table_store_value(def, remove);
                let _ = builtin_set_char_table_range(vec![entry_car, event, store_val], None);
                return;
            }
            // GNU keymap.c:829: char-table found, advance insertion_point
            // so a future prepend lands AFTER the char-table.
            insertion_point = cursor;
            cursor = entry_cdr;
            continue;
        }

        // Vector element: check for matching index or update the covered
        // prefix of a character range.
        // GNU keymap.c:783-803
        if entry_car.is_vector() {
            if let Some(code) = event.as_fixnum() {
                let idx = code as usize;
                let updated = entry_car
                    .as_vector_data()
                    .is_some_and(|vec_data| idx < vec_data.len())
                    && entry_car.set_vector_slot(idx, def);
                if updated {
                    return;
                }
            } else if let Some((from, to)) = keymap_character_range(&event)
                && let Some(vec_data) = entry_car.as_vector_data()
                && from < vec_data.len() as i64
            {
                let last = to.min(vec_data.len() as i64 - 1);
                for code in from..=last {
                    entry_car.set_vector_slot(code as usize, def);
                }
                if to == last {
                    return;
                }
            }
            // GNU keymap.c:803: vector found, advance insertion_point
            // so a future prepend lands AFTER the vector.
            insertion_point = cursor;
            cursor = entry_cdr;
            continue;
        }

        // Embedded sub-keymap in a composed keymap.  GNU retargets the write
        // into this real component map because the containing map may be a
        // temporary keymap produced by access_keymap/make-composed-keymap.
        if entry_car.is_cons() && is_list_keymap(&entry_car) {
            insertion_point = entry_car;
            cursor = entry_car.cons_cdr();
            continue;
        }

        // Alist entry: (EVENT . DEF) — check for existing binding to update in-place.
        // GNU keymap.c:842-849
        if entry_car.is_cons() {
            let binding_car = entry_car.cons_car();
            if let Some((from, to)) = keymap_character_range(&event) {
                if let Some(code) = binding_car.as_fixnum()
                    && (from..=to).contains(&code)
                {
                    if remove {
                        keymap_delq_after(insertion_point, entry_car);
                    } else {
                        entry_car.set_cdr(def);
                    }
                    if from == to {
                        return;
                    }
                }
            } else if eq_value(&binding_car, &event) {
                if remove {
                    // GNU uses `Fdelq (elt, insertion_point)`: remove exactly
                    // this binding cons while preserving earlier alist entries
                    // that insertion_point intentionally skips over.
                    keymap_delq_after(insertion_point, entry_car);
                } else {
                    // Update in-place: set the cdr of the binding cons.
                    entry_car.set_cdr(def);
                }
                return;
            }
        }

        // Check for 'keymap symbol in spine (start of inherited keymap)
        // GNU keymap.c:871-876
        if KeymapMarker::Keymap.is_value(entry_car) {
            break;
        }

        // NOTE: deliberately do NOT advance `insertion_point` here.
        // GNU keeps it pointing at the keymap head (or the last
        // vector/char-table) for ordinary alist entries and prompt
        // strings, so
        // that the prepend at the end of the function inserts the
        // new binding at the front of the alist (but after any header
        // elements).
        cursor = entry_cdr;
    }

    // No existing binding found. Prepend the new entry right after
    // `insertion_point`, matching GNU `keymap.c:898`:
    //   XSETCDR (insertion_point, Fcons (elt, XCDR (insertion_point)));
    if !remove {
        let binding = if event.is_cons() && keymap_character_range(&event).is_some() {
            let char_table = make_char_table_value(KeymapMarker::Keymap.symbol_value(), Value::NIL);
            let store_val = keymap_char_table_store_value(def, false);
            let _ = builtin_set_char_table_range(vec![char_table, event, store_val], None);
            char_table
        } else {
            Value::cons(event, def)
        };
        let old_cdr = match insertion_point.kind() {
            ValueKind::Cons => insertion_point.cons_cdr(),
            _ => Value::NIL,
        };
        let new_cdr = Value::cons(binding, old_cdr);
        insertion_point.set_cdr(new_cdr);
    }
}

/// Get the parent keymap (last CDR that is itself a keymap).
pub fn list_keymap_parent(keymap: &Value) -> Value {
    let Some(mut cursor) = keymap_binding_spine(keymap) else {
        return Value::NIL;
    };
    while cursor.is_cons() {
        // Check if cursor itself is a parent keymap before treating as alist entry
        if is_list_keymap(&cursor) {
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
pub(crate) fn list_keymap_sibling_keymaps(keymap: &Value) -> Vec<Value> {
    let mut siblings = Vec::new();
    let Some(mut cursor) = keymap_binding_spine(keymap) else {
        return siblings;
    };
    while cursor.is_cons() {
        // The spine tail is a parent keymap `(keymap ...)`.
        if is_list_keymap(&cursor) {
            siblings.push(cursor);
            break;
        }
        let entry_car = cursor.cons_car();
        // An inline element that is itself a keymap is a composed submap; a
        // `(KEY . BINDING)` cons is an ordinary binding (its car is the key, not
        // the `keymap` marker) and is left to the containing map's own scan.
        if is_list_keymap(&entry_car) {
            siblings.push(entry_car);
        }
        cursor = cursor.cons_cdr();
    }
    siblings
}

/// Set the parent keymap: walk to the last alist cons cell, set its CDR.
pub fn list_keymap_set_parent(keymap: Value, parent: Value) {
    note_keymap_mutation();
    if !keymap.is_cons() {
        return;
    };
    let root_car = keymap.cons_car();
    let root_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(root_car) {
        return;
    }

    // Find the last cons cell in the keymap list
    let mut prev_cell_value = keymap;
    let mut cursor = root_cdr;
    loop {
        if is_list_keymap(&cursor) || cursor.is_nil() {
            prev_cell_value.set_cdr(parent);
            return;
        }
        match cursor.kind() {
            ValueKind::Cons => {
                let entry_cdr = cursor.cons_cdr();
                // If cdr is a keymap (existing parent) or nil, we replace it
                if is_list_keymap(&entry_cdr) || entry_cdr.is_nil() {
                    cursor.set_cdr(parent);
                    return;
                }
                prev_cell_value = cursor;
                cursor = entry_cdr;
            }
            _ => {
                // cursor is either nil or an existing parent keymap
                // Set previous cell's cdr to the new parent
                prev_cell_value.set_cdr(parent);
                return;
            }
        }
    }
}

/// Check whether `target` appears in `keymap`'s parent chain.
pub fn list_keymap_inherits_from(keymap: &Value, target: &Value) -> bool {
    let mut current = *keymap;
    while is_list_keymap(&current) {
        // Use pointer identity (eq), not structural equality (equal),
        // to detect cycles. Two keymaps with the same content are NOT
        // the same keymap.
        if eq_value(&current, target) {
            return true;
        }
        current = list_keymap_parent(&current);
    }
    false
}

/// Runtime selection installed by GNU's `use-global-map`.
///
/// This is intentionally distinct from the Lisp variable `global-map`.
/// Dynamically binding or assigning that variable does not select a new map;
/// only `use-global-map` changes the runtime selection.
#[derive(Clone, Copy, Debug, Default)]
enum GlobalMapSelection {
    #[default]
    Uninitialized,
    Selected(Value),
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SelectedGlobalMap {
    selection: GlobalMapSelection,
}

impl SelectedGlobalMap {
    pub(crate) fn from_dump(value: Value) -> Option<Self> {
        if value.is_nil() {
            Some(Self::default())
        } else if is_list_keymap(&value) {
            Some(Self {
                selection: GlobalMapSelection::Selected(value),
            })
        } else {
            None
        }
    }

    pub(crate) fn value(self) -> Value {
        match self.selection {
            GlobalMapSelection::Uninitialized => Value::NIL,
            GlobalMapSelection::Selected(value) => value,
        }
    }

    pub(crate) fn select(&mut self, value: Value) {
        assert!(
            is_list_keymap(&value),
            "SelectedGlobalMap requires a resolved keymap"
        );
        self.selection = GlobalMapSelection::Selected(value);
    }
}

fn dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
    obarray: &Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    sym_id: SymId,
) -> Option<Value> {
    if let Some(buffer_id) = buffer_id
        && let Some(buf) = buffers.get(buffer_id)
        && let Some(value) = buf.get_buffer_local_by_sym_id(sym_id)
    {
        return Some(value);
    }
    obarray.symbol_value_id(sym_id).copied()
}

pub(crate) fn minor_mode_map_entry(entry: &Value) -> Option<(SymId, Value)> {
    if !entry.is_cons() {
        return None;
    };

    let (mode, cdr) = {
        let pair_car = entry.cons_car();
        let pair_cdr = entry.cons_cdr();
        (pair_car, pair_cdr)
    };
    let mode_name = mode.as_symbol_id()?;
    if cdr == Value::NIL {
        return None;
    }
    Some((mode_name, cdr))
}

fn collect_maps_from_alist_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    alist: &Value,
    skip_if_in: Option<&Value>,
    maps: &mut Vec<Value>,
) {
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
        let Some(mode_id) = mode_var.as_symbol_id() else {
            continue;
        };

        if let Some(skip_alist) = skip_if_in
            && assq_in_alist(skip_alist, &mode_var)
        {
            continue;
        }

        // GNU `current_minor_maps` resolves the exact symbol stored in the
        // alist.  Do not round-trip through its printed name: minor modes may
        // deliberately use an uninterned symbol (for example a `cl-gensym`)
        // whose value cell is distinct from an interned namesake.
        let mode_active = dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            obarray, dynamic, buffers, buffer_id, mode_id,
        )
        .map(|value| value.is_truthy())
        .unwrap_or(false);
        if !mode_active {
            continue;
        }

        if let Some(resolved) = maybe_keymap_in_obarray(obarray, &keymap_val) {
            maps.push(resolved);
        }
    }
}

fn collect_map_entries_from_alist_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    alist: &Value,
    skip_if_in: Option<&Value>,
    maps: &mut Vec<(SymId, Value)>,
) {
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
        let Some(mode_name) = mode_var.as_symbol_id() else {
            continue;
        };

        if let Some(skip_alist) = skip_if_in
            && assq_in_alist(skip_alist, &mode_var)
        {
            continue;
        }

        let mode_active = dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            obarray, dynamic, buffers, buffer_id, mode_name,
        )
        .map(|value| value.is_truthy())
        .unwrap_or(false);
        if !mode_active {
            continue;
        }

        if let Some(resolved) = maybe_keymap_in_obarray(obarray, &keymap_val) {
            maps.push((mode_name, resolved));
        }
    }
}

fn assq_in_alist(alist: &Value, key: &Value) -> bool {
    let Some(entries) = list_to_vec(alist) else {
        return false;
    };

    for entry in entries {
        if !entry.is_cons() {
            continue;
        };
        let pair_car = entry.cons_car();
        let _pair_cdr = entry.cons_cdr();
        if pair_car == *key {
            return true;
        }
    }

    false
}

pub(crate) fn collect_minor_mode_maps_in_state(
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
) -> Vec<Value> {
    let mut maps = Vec::new();

    if let Some(emulation_raw) = KeymapStateVariable::EmulationModeMapAlists
        .buffer_or_global_value(obarray, buffers, buffer_id)
        && let Some(emulation_entries) = list_to_vec(&emulation_raw)
    {
        for entry in emulation_entries {
            let alist_value = match entry.as_symbol_id() {
                Some(sym_id) => dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
                    obarray, dynamic, buffers, buffer_id, sym_id,
                )
                .unwrap_or(Value::NIL),
                None => entry,
            };
            collect_maps_from_alist_in_state(
                obarray,
                dynamic,
                buffers,
                buffer_id,
                &alist_value,
                None,
                &mut maps,
            );
        }
    }

    let overriding = KeymapStateVariable::MinorModeOverridingMapAlist
        .buffer_or_global_value(obarray, buffers, buffer_id);
    if let Some(ref overriding_maps) = overriding {
        collect_maps_from_alist_in_state(
            obarray,
            dynamic,
            buffers,
            buffer_id,
            overriding_maps,
            None,
            &mut maps,
        );
    }

    if let Some(regular) =
        KeymapStateVariable::MinorModeMapAlist.buffer_or_global_value(obarray, buffers, buffer_id)
    {
        collect_maps_from_alist_in_state(
            obarray,
            dynamic,
            buffers,
            buffer_id,
            &regular,
            overriding.as_ref(),
            &mut maps,
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
        .global_value(&ctx.obarray)
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
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
) -> Vec<(SymId, Value)> {
    let mut maps = Vec::new();

    if let Some(emulation_raw) = KeymapStateVariable::EmulationModeMapAlists
        .buffer_or_global_value(obarray, buffers, buffer_id)
        && let Some(emulation_entries) = list_to_vec(&emulation_raw)
    {
        for entry in emulation_entries {
            let alist_value = match entry.as_symbol_id() {
                Some(sym_id) => dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
                    obarray, dynamic, buffers, buffer_id, sym_id,
                )
                .unwrap_or(Value::NIL),
                None => entry,
            };
            collect_map_entries_from_alist_in_state(
                obarray,
                dynamic,
                buffers,
                buffer_id,
                &alist_value,
                None,
                &mut maps,
            );
        }
    }

    let overriding = KeymapStateVariable::MinorModeOverridingMapAlist
        .buffer_or_global_value(obarray, buffers, buffer_id);
    if let Some(ref overriding_maps) = overriding {
        collect_map_entries_from_alist_in_state(
            obarray,
            dynamic,
            buffers,
            buffer_id,
            overriding_maps,
            None,
            &mut maps,
        );
    }

    if let Some(regular) =
        KeymapStateVariable::MinorModeMapAlist.buffer_or_global_value(obarray, buffers, buffer_id)
    {
        collect_map_entries_from_alist_in_state(
            obarray,
            dynamic,
            buffers,
            buffer_id,
            &regular,
            overriding.as_ref(),
            &mut maps,
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
    fn from_position_area(value: Value) -> Self {
        match value.as_symbol_name() {
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
    fn from_position_slots(slots: &[Value]) -> Option<Self> {
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
                .map(DisplayedStringArea::from_position_area)
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
) -> Result<Option<ActiveMapPosition>, Flow> {
    let Some(buffer) = buffers.current_buffer() else {
        return Ok(None);
    };

    let default_position = ActiveMapPosition {
        buffer_id: buffer.id,
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
                vec![Value::make_buffer(buffer.id), *position],
            ));
        }

        return Ok(Some(ActiveMapPosition {
            buffer_id: buffer.id,
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
    let displayed_string = DisplayedStringPosition::from_position_slots(&slots);

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
    obarray: &Obarray,
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
        obarray,
        buffers,
        buffer,
        position,
        prop_symbol,
    );
    if !char_property.is_nil() {
        return Ok(char_property);
    }

    super::textprop::buffer_pos_property_at_full_lisp_pos(
        obarray,
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
    obarray: &Obarray,
    buffers: &crate::buffer::BufferManager,
    buffer_object: Value,
    buffer_point: i64,
    property: LocalMapProperty,
    buffer_keymap: Value,
) -> Result<Value, Flow> {
    let fallback = match property {
        LocalMapProperty::Keymap => Value::NIL,
        LocalMapProperty::LocalMap => buffer_keymap,
    };
    let buffer_id = buffer_object
        .as_buffer_id()
        .ok_or_else(|| signal(LispCondition::WrongTypeArgument, vec![buffer_object]))?;
    let found = keymap_property_at_position(obarray, buffers, buffer_id, buffer_point, property)?;
    Ok(maybe_keymap_in_obarray(obarray, &found).unwrap_or(fallback))
}

fn displayed_string_map_overrides(
    obarray: &Obarray,
    buffers: &crate::buffer::BufferManager,
    displayed: DisplayedStringPosition,
) -> Result<DisplayedStringMapOverrides, Flow> {
    let resolve = |property: LocalMapProperty| -> Result<DisplayedStringMapOverride, Flow> {
        let found = super::textprop::builtin_get_text_property_in_state(
            obarray,
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
            maybe_keymap_in_obarray(obarray, &found).unwrap_or(Value::NIL),
        ))
    };

    Ok(DisplayedStringMapOverrides {
        local_map: resolve(LocalMapProperty::LocalMap)?,
        keymap: resolve(LocalMapProperty::Keymap)?,
    })
}

fn position_map_layers(
    obarray: &Obarray,
    buffers: &crate::buffer::BufferManager,
    active_position: Option<ActiveMapPosition>,
    fallback_local_map: Value,
) -> Result<PositionMapLayers, Flow> {
    let Some(active_position) = active_position else {
        return Ok(PositionMapLayers {
            local_map: fallback_local_map,
            keymap: Value::NIL,
        });
    };

    let (mut local_map, mut keymap) = if let Some(char_pos) = active_position.char_pos {
        let local_property = keymap_property_at_position(
            obarray,
            buffers,
            active_position.buffer_id,
            char_pos,
            LocalMapProperty::LocalMap,
        )?;
        let keymap_property = keymap_property_at_position(
            obarray,
            buffers,
            active_position.buffer_id,
            char_pos,
            LocalMapProperty::Keymap,
        )?;
        (
            maybe_keymap_in_obarray(obarray, &local_property)
                .unwrap_or(active_position.buffer_local_map),
            maybe_keymap_in_obarray(obarray, &keymap_property).unwrap_or(Value::NIL),
        )
    } else {
        (active_position.buffer_local_map, Value::NIL)
    };

    if let Some(displayed_string) = active_position.displayed_string {
        // Mirrors GNU keymap.c `current-active-maps`: a displayed string's
        // maps override the maps derived from the clicked buffer position.
        let overrides = displayed_string_map_overrides(obarray, buffers, displayed_string)?;
        local_map = overrides.local_map.apply(local_map);
        keymap = overrides.keymap.apply(keymap);
    }

    Ok(PositionMapLayers { local_map, keymap })
}

#[allow(clippy::too_many_arguments)] // explicit keymap layers preserve GNU precedence ordering
fn current_active_maps_from_parts(
    obarray: &Obarray,
    frames: &crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    current_local_map: Value,
    global_map: Value,
    minor_maps: Vec<Value>,
    overriding_local_map: Option<Value>,
    overriding_terminal_local_map: Option<Value>,
    obey_overriding_local_maps: bool,
    position: Option<&Value>,
) -> Result<Vec<Value>, Flow> {
    let active_position = active_map_position(frames, buffers, position)?;
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

    let position_maps = position_map_layers(obarray, buffers, active_position, current_local_map)?;
    if !position_maps.keymap.is_nil() {
        maps.push(position_maps.keymap);
    }

    if minor_maps.is_empty() {
        maps.extend(collect_minor_mode_maps_in_state(
            obarray,
            &[],
            buffers,
            current_buffer_id,
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
    let global_map = ctx.current_global_map();
    let overriding_local_map = KeymapStateVariable::OverridingLocalMap
        .global_value(&ctx.obarray)
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value));
    let overriding_terminal_local_map = KeymapStateVariable::OverridingTerminalLocalMap
        .global_value(&ctx.obarray)
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value));

    current_active_maps_from_parts(
        &ctx.obarray,
        &ctx.frames,
        &ctx.buffers,
        ctx.buffers.current_local_map(),
        global_map,
        Vec::new(),
        overriding_local_map,
        overriding_terminal_local_map,
        obey_overriding_local_maps,
        position,
    )
}

pub(crate) fn current_active_maps_for_position_read_only(
    ctx: &Context,
    obey_overriding_local_maps: bool,
    position: Option<&Value>,
) -> Result<Vec<Value>, Flow> {
    let global_map = ctx.current_global_map();
    let overriding_local_map = KeymapStateVariable::OverridingLocalMap
        .global_value(&ctx.obarray)
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value));
    let overriding_terminal_local_map = KeymapStateVariable::OverridingTerminalLocalMap
        .global_value(&ctx.obarray)
        .and_then(|value| maybe_keymap_in_obarray(&ctx.obarray, &value));

    current_active_maps_from_parts(
        &ctx.obarray,
        &ctx.frames,
        &ctx.buffers,
        ctx.buffers.current_local_map(),
        global_map,
        Vec::new(),
        overriding_local_map,
        overriding_terminal_local_map,
        obey_overriding_local_maps,
        position,
    )
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ActiveKeyBindingResolution {
    pub lookup: Value,
    pub binding: Value,
}

/// Whether `(t . BINDING)` entries participate in active keymap lookup.
///
/// GNU's command reader always accepts these catch-all bindings, while Lisp
/// inspection APIs such as `key-binding` expose an `ACCEPT-DEFAULT` argument.
/// Keeping those two modes distinct in the type system prevents command
/// dispatch from accidentally inheriting the inspection API's default.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DefaultBindingMode {
    Ignore,
    Accept,
}

impl DefaultBindingMode {
    fn accepts_default(self) -> bool {
        matches!(self, Self::Accept)
    }
}

impl From<bool> for DefaultBindingMode {
    fn from(accept_default: bool) -> Self {
        if accept_default {
            Self::Accept
        } else {
            Self::Ignore
        }
    }
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
    obarray: &Obarray,
    dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buffer_id: Option<crate::buffer::BufferId>,
    events: &[Value],
    alist_value: &Value,
) -> Result<Option<(SymId, Value)>, Flow> {
    let Some(entries) = list_to_vec(alist_value) else {
        return Ok(None);
    };

    for entry in entries {
        let Some((mode_name, map_value)) = minor_mode_map_entry(&entry) else {
            continue;
        };
        if !dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            obarray, dynamic, buffers, buffer_id, mode_name,
        )
        .is_some_and(|v| v.is_truthy())
        {
            continue;
        }

        let keymap = if is_list_keymap(&map_value) {
            map_value
        } else if map_value.as_symbol_name().is_some() {
            match map_value
                .as_symbol_name()
                .and_then(|name| obarray.symbol_value(name).copied())
            {
                Some(value) if is_list_keymap(&value) => value,
                _ => match obarray.symbol_function_of_value(&map_value) {
                    Some(value) if is_list_keymap(&value) => value,
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

        let binding = lookup_keymap_with_partial(&keymap, events);
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
    let current_buffer_id = ctx.buffers.current_buffer_id();
    if let Some(emulation_raw) = KeymapStateVariable::EmulationModeMapAlists.buffer_or_global_value(
        &ctx.obarray,
        &ctx.buffers,
        current_buffer_id,
    ) && let Some(emulation_entries) = list_to_vec(&emulation_raw)
    {
        for emulation_entry in emulation_entries {
            let alist_value = match emulation_entry.as_symbol_id() {
                Some(sym_id) => dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
                    &ctx.obarray,
                    &[],
                    &ctx.buffers,
                    current_buffer_id,
                    sym_id,
                )
                .unwrap_or(Value::NIL),
                None => emulation_entry,
            };
            if let Some((mode_name, binding)) = lookup_minor_mode_binding_in_alist_in_obarray(
                &ctx.obarray,
                &[],
                &ctx.buffers,
                current_buffer_id,
                events,
                &alist_value,
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
            variable.buffer_or_global_value(&ctx.obarray, &ctx.buffers, current_buffer_id)
        else {
            continue;
        };
        if let Some((mode_name, binding)) = lookup_minor_mode_binding_in_alist_in_obarray(
            &ctx.obarray,
            &[],
            &ctx.buffers,
            current_buffer_id,
            events,
            &alist_value,
        )? {
            return Ok(Value::list(vec![Value::cons(
                Value::from_sym_id(mode_name),
                binding,
            )]));
        }
    }

    Ok(Value::NIL)
}

fn where_is_expect_keymap_in_obarray(obarray: &Obarray, value: &Value) -> Result<Value, Flow> {
    get_keymap_in_obarray(obarray, value, true)
}

fn where_is_explicit_keymaps_in_context(ctx: &Context, value: &Value) -> Result<Vec<Value>, Flow> {
    if is_list_keymap(value) {
        let keymap = where_is_expect_keymap_in_obarray(&ctx.obarray, value)?;
        let mut keymaps = vec![keymap];
        let global_map = ctx.current_global_map();
        if is_list_keymap(&global_map) && global_map != keymap {
            keymaps.push(global_map);
        }
        return Ok(keymaps);
    }

    if value.is_cons()
        && maybe_keymap_in_obarray(&ctx.obarray, &value.cons_car())
            .is_some_and(|keymap| is_list_keymap(&keymap))
        && let Some(items) = list_to_vec(value)
    {
        let mut keymaps = Vec::with_capacity(items.len());
        for item in items {
            keymaps.push(where_is_expect_keymap_in_obarray(&ctx.obarray, &item)?);
        }
        return Ok(keymaps);
    }

    let keymap = if value.is_cons() {
        *value
    } else {
        where_is_expect_keymap_in_obarray(&ctx.obarray, value)?
    };
    let mut keymaps = vec![keymap];
    let global_map = ctx.current_global_map();
    if is_list_keymap(&global_map) && global_map != keymap {
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

fn command_remapping_lookup_in_lisp_remap_entry(entry: &Value, command: SymId) -> Option<Value> {
    if !KeymapMarker::Remap.is_value(command_remapping_nth_list_element(entry, 0)?) {
        return None;
    }
    if !KeymapMarker::Keymap.is_value(command_remapping_nth_list_element(entry, 1)?) {
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
            if binding_key.as_symbol_id() == Some(command) {
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
) -> Option<Value> {
    if !is_list_keymap(keymap) {
        return None;
    }

    let mut cursor = if keymap.is_cons() {
        keymap.cons_cdr()
    } else {
        Value::NIL
    };

    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            if let Some(parent) = command_remapping_lookup_in_lisp_keymap(&cursor, command) {
                return Some(parent);
            }
            break;
        }

        let car = cursor.cons_car();
        let cdr = cursor.cons_cdr();
        if is_list_keymap(&car) {
            if let Some(child) = command_remapping_lookup_in_lisp_keymap(&car, command) {
                return Some(child);
            }
            cursor = cdr;
            continue;
        }
        if let Some(remap) = command_remapping_lookup_in_lisp_remap_entry(&car, command) {
            return Some(remap);
        }
        cursor = cdr;
    }

    None
}

fn command_remapping_menu_item_target(value: &Value) -> Option<Value> {
    if !value.is_cons() {
        return None;
    };
    let pair_car = value.cons_car();
    let pair_cdr = value.cons_cdr();
    if !KeymapMarker::MenuItem.is_value(pair_car) {
        return None;
    }

    let tail = pair_cdr;
    let title = command_remapping_nth_list_element(&tail, 0)?;
    if title.is_nil() {
        return None;
    }
    command_remapping_nth_list_element(&tail, 1)
}

pub(crate) fn command_remapping_normalize_target(raw: Value) -> Value {
    if let Some(menu_target) = command_remapping_menu_item_target(&raw) {
        return if menu_target.is_integer() {
            Value::NIL
        } else {
            menu_target
        };
    }
    if raw == Value::T || raw.is_fixnum() {
        return Value::NIL;
    }
    raw
}

fn command_remapping_lookup_in_keymap_value(keymap: &Value, command: SymId) -> Option<Value> {
    command_remapping_lookup_in_lisp_keymap(keymap, command).map(command_remapping_normalize_target)
}

pub(crate) fn command_remapping_lookup_in_keymaps(
    keymaps: &[Value],
    command: SymId,
) -> Option<Value> {
    for keymap in keymaps {
        if !is_list_keymap(keymap) {
            continue;
        }
        if let Some(value) = command_remapping_lookup_in_keymap_value(keymap, command) {
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
    for keymap in keymaps {
        if !is_list_keymap(keymap) {
            continue;
        }
        let Some(raw) = command_remapping_lookup_in_lisp_keymap(keymap, command) else {
            continue;
        };
        let resolved = get_keyelt_runtime(ctx, raw, true)?;
        let normalized = command_remapping_normalize_target(resolved);
        if !normalized.is_nil() {
            return Ok(Some(normalized));
        }
    }
    Ok(None)
}

pub(crate) fn command_remapping_command_name(command: &Value) -> Option<SymId> {
    command.as_symbol_id()
}

pub(crate) fn key_binding_apply_remap_in_active_maps(
    ctx: &mut Context,
    active_maps: &[Value],
    binding: Value,
    no_remap: bool,
) -> EvalResult {
    if no_remap {
        return Ok(binding);
    }
    let Some(command_name) = binding.as_symbol_id() else {
        return Ok(binding);
    };
    match command_remapping_lookup_in_keymaps_runtime(ctx, active_maps, command_name)? {
        Some(remapped) if !remapped.is_nil() => Ok(remapped),
        _ => Ok(binding),
    }
}

/// Convert a `KeyEvent` to an Emacs event value (integer with modifier bits, or symbol).
///
/// For Ctrl + ASCII letter, produce the control character code (1-26)
/// instead of using the CTRL modifier bit.  This matches GNU Emacs
/// `MAKE_CTRL_CHAR` normalization: C-a=1, C-b=2, ..., C-z=26,
/// C-@=0, C-[=27, C-\=28, C-]=29, C-^=30, C-_=31.
pub fn key_event_to_emacs_event(event: &KeyEvent) -> Value {
    match event {
        KeyEvent::Char {
            code,
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        } => {
            let mut bits: i64;
            if *ctrl {
                let c = *code as u32;
                // GNU Emacs MAKE_CTRL_CHAR normalization: for characters
                // that have a natural control character, fold into 0-31
                // without the CTRL modifier bit.
                let ctrl_char = match c {
                    // a-z → 1-26
                    0x61..=0x7A => Some(c - 0x60),
                    // A-Z → 1-26  (same as lowercase)
                    0x41..=0x5A => Some(c - 0x40),
                    // @ → 0 (NUL)
                    0x40 => Some(0),
                    // [ → 27 (ESC)
                    0x5B => Some(27),
                    // \ → 28
                    0x5C => Some(28),
                    // ] → 29
                    0x5D => Some(29),
                    // ^ → 30
                    0x5E => Some(30),
                    // _ → 31
                    0x5F => Some(31),
                    // Space/? → 0 (NUL) — Emacs convention
                    0x20 => Some(0),
                    _ => None,
                };
                if let Some(cc) = ctrl_char {
                    bits = cc as i64;
                } else {
                    bits = *code as i64;
                    bits |= KEY_CHAR_CTRL;
                }
            } else {
                bits = *code as i64;
            }
            if *meta {
                bits |= KEY_CHAR_META;
            }
            if *shift {
                bits |= KEY_CHAR_SHIFT;
            }
            if *super_ {
                bits |= KEY_CHAR_SUPER;
            }
            if *hyper {
                bits |= KEY_CHAR_HYPER;
            }
            if *alt {
                bits |= KEY_CHAR_ALT;
            }
            Value::fixnum(bits)
        }
        KeyEvent::Function {
            name,
            ctrl,
            meta,
            shift,
            super_,
            hyper,
            alt,
        } => {
            let mut prefix = String::new();
            if *alt {
                prefix.push_str("A-");
            }
            if *ctrl {
                prefix.push_str("C-");
            }
            if *hyper {
                prefix.push_str("H-");
            }
            if *meta {
                prefix.push_str("M-");
            }
            if *shift {
                prefix.push_str("S-");
            }
            if *super_ {
                prefix.push_str("s-");
            }
            Value::symbol(format!("{}{}", prefix, resolve_sym(*name)))
        }
    }
}

/// Convert an Emacs event value to a `KeyEvent`.
///
/// Recognizes control characters (0-31) and decomposes them into
/// the corresponding letter with ctrl=true.
pub fn emacs_event_to_key_event(event: &Value) -> Option<KeyEvent> {
    match event.kind() {
        ValueKind::Fixnum(code) => {
            let base = code & KEY_CHAR_CODE_MASK;
            let has_ctrl_bit = (code & KEY_CHAR_CTRL) != 0;
            let meta = (code & KEY_CHAR_META) != 0;
            let shift = (code & KEY_CHAR_SHIFT) != 0;
            let super_ = (code & KEY_CHAR_SUPER) != 0;
            let hyper = (code & KEY_CHAR_HYPER) != 0;
            let alt = (code & KEY_CHAR_ALT) != 0;

            // Decompose control characters (0-31) back to letter + ctrl
            if !has_ctrl_bit && (0..=31).contains(&base) {
                let (ch, ctrl) = match base {
                    0 => ('@', true), // NUL → C-@
                    1..=26 => {
                        // 1-26 → C-a through C-z
                        let c = char::from_u32((base + 0x60) as u32)?;
                        (c, true)
                    }
                    27 => ('\u{1b}', false), // ESC → literal escape prefix char
                    28 => ('\\', true),      // C-\
                    29 => (']', true),       // C-]
                    30 => ('^', true),       // C-^
                    31 => ('_', true),       // C-_
                    _ => unreachable!(),
                };
                Some(KeyEvent::Char {
                    code: ch,
                    ctrl,
                    meta,
                    shift,
                    super_,
                    hyper,
                    alt,
                })
            } else {
                let ch = char::from_u32(base as u32)?;
                Some(KeyEvent::Char {
                    code: ch,
                    ctrl: has_ctrl_bit,
                    meta,
                    shift,
                    super_,
                    hyper,
                    alt,
                })
            }
        }
        ValueKind::Symbol(id) => {
            let name = resolve_sym(id);
            // Parse modifier prefixes
            let mut rest = name;
            let mut ctrl = false;
            let mut meta = false;
            let mut shift = false;
            let mut super_ = false;
            let mut hyper = false;
            let mut alt = false;
            loop {
                if let Some(r) = rest.strip_prefix("C-") {
                    ctrl = true;
                    rest = r;
                    continue;
                }
                if let Some(r) = rest.strip_prefix("M-") {
                    meta = true;
                    rest = r;
                    continue;
                }
                if let Some(r) = rest.strip_prefix("S-") {
                    shift = true;
                    rest = r;
                    continue;
                }
                if let Some(r) = rest.strip_prefix("s-") {
                    super_ = true;
                    rest = r;
                    continue;
                }
                if let Some(r) = rest.strip_prefix("H-") {
                    hyper = true;
                    rest = r;
                    continue;
                }
                if let Some(r) = rest.strip_prefix("A-") {
                    alt = true;
                    rest = r;
                    continue;
                }
                break;
            }
            // If single char, return Char event
            let mut chars = rest.chars();
            if let Some(ch) = chars.next()
                && chars.next().is_none()
            {
                return Some(KeyEvent::Char {
                    code: ch,
                    ctrl,
                    meta,
                    shift,
                    super_,
                    hyper,
                    alt,
                });
            }
            // Otherwise it's a function key
            Some(KeyEvent::Function {
                name: intern(rest),
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            })
        }
        _ => None,
    }
}

/// Look up a key sequence in a keymap, following prefix keymaps and parent chains.
/// Returns the binding Value, or the number of keys matched (as `Value::Int`)
/// when the sequence resolves through a non-keymap binding.
pub fn list_keymap_lookup_seq(keymap: &Value, events: &[Value]) -> Value {
    list_keymap_lookup_seq_impl(keymap, events, true)
}

pub(crate) fn list_keymap_lookup_seq_unresolved(keymap: &Value, events: &[Value]) -> Value {
    list_keymap_lookup_seq_impl(keymap, events, false)
}

fn list_keymap_lookup_seq_impl(keymap: &Value, events: &[Value], resolve_keyelt: bool) -> Value {
    if events.is_empty() {
        return *keymap;
    }

    if let Some(binding) = list_keymap_lookup_composed_seq(keymap, events, resolve_keyelt) {
        return binding;
    }

    let mut current_map = *keymap;
    for (i, event) in events.iter().enumerate() {
        let binding = if resolve_keyelt {
            list_keymap_lookup_one(&current_map, event)
        } else {
            list_keymap_lookup_one_unresolved(&current_map, event)
        };
        let is_last = i == events.len() - 1;
        if is_last {
            // GNU: for the last key, return binding directly (even nil)
            return binding;
        }
        if binding.is_nil() {
            // No binding for a non-last event → return the number of keys
            // consumed (matching GNU which returns make_fixnum(idx) where
            // idx is already post-incremented).
            return Value::fixnum((i + 1) as i64);
        }
        // Must be a prefix keymap to continue
        if is_list_keymap(&binding) {
            current_map = binding;
        } else {
            // Check if it's a symbol whose function cell is a keymap
            if let Some(sym_name) = binding.as_symbol_name() {
                // We can't resolve symbol function cells from keymap.rs —
                // caller must handle this case. For now treat as non-prefix.
                let _ = sym_name;
            }
            return Value::fixnum((i + 1) as i64);
        }
    }
    Value::NIL
}

fn list_keymap_lookup_composed_seq(
    keymap: &Value,
    events: &[Value],
    resolve_keyelt: bool,
) -> Option<Value> {
    let mut cursor = keymap_binding_spine(keymap)?;
    let mut saw_embedded_keymap = false;
    let mut parent = Value::NIL;

    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            parent = cursor;
            break;
        }

        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();
        if is_list_keymap(&entry_car) {
            saw_embedded_keymap = true;
            let binding = list_keymap_lookup_seq_impl(&entry_car, events, resolve_keyelt);
            if !binding.is_nil() && !binding.is_fixnum() {
                return Some(binding);
            }
        } else if saw_embedded_keymap {
            break;
        } else {
            return None;
        }

        if is_list_keymap(&entry_cdr) {
            parent = entry_cdr;
            break;
        }
        cursor = entry_cdr;
    }

    if !saw_embedded_keymap {
        if !parent.is_nil() {
            let binding = list_keymap_lookup_seq_impl(&parent, events, resolve_keyelt);
            if !binding.is_nil() && !binding.is_fixnum() {
                return Some(binding);
            }
        }
        return None;
    }
    if !parent.is_nil() {
        let binding = list_keymap_lookup_seq_impl(&parent, events, resolve_keyelt);
        if !binding.is_nil() && !binding.is_fixnum() {
            return Some(binding);
        }
    }
    None
}

pub(crate) fn lookup_keymap_with_partial(keymap: &Value, emacs_events: &[Value]) -> Value {
    if emacs_events.is_empty() {
        return *keymap;
    }
    list_keymap_lookup_seq(keymap, emacs_events)
}

/// Define a key in a keymap, auto-creating prefix maps for multi-key sequences.
///
/// Returns `Err` if an intermediate key is already bound to a non-prefix
/// command (matching GNU Emacs behavior which signals an error).
pub fn list_keymap_define_seq(keymap: Value, events: &[Value], def: Value) -> Result<(), String> {
    note_keymap_mutation();
    if events.is_empty() {
        return Ok(());
    }
    if events.len() == 1 {
        list_keymap_define(keymap, events[0], def);
        return Ok(());
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        if i == events.len() - 1 {
            list_keymap_define(current_map, *event, def);
            return Ok(());
        }
        // Use noinherit: only look in current keymap level for prefix,
        // matching GNU Emacs define-key which uses access_keymap(noinherit=1)
        let binding = list_keymap_lookup_one_noinherit(&current_map, event);
        if is_list_keymap(&binding) {
            current_map = binding;
        } else if binding.is_nil() {
            // No binding at this level, create a new prefix keymap
            let prefix_map = make_sparse_list_keymap();
            list_keymap_define(current_map, *event, prefix_map);
            current_map = prefix_map;
        } else {
            // Non-prefix binding found — error (matching GNU Emacs)
            let full = describe_event_sequence(events);
            let prefix = describe_event_sequence(&events[..=i]);
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
) -> Result<(), String> {
    note_keymap_mutation();
    if events.is_empty() {
        return Ok(());
    }
    if events.len() == 1 {
        if remove {
            list_keymap_remove(keymap, events[0]);
        } else {
            list_keymap_define(keymap, events[0], def);
        }
        return Ok(());
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        if i == events.len() - 1 {
            if remove {
                list_keymap_remove(current_map, *event);
            } else {
                list_keymap_define(current_map, *event, def);
            }
            return Ok(());
        }
        // Use noinherit: only look in current keymap level for prefix,
        // matching GNU Emacs define-key which uses access_keymap(noinherit=1)
        let binding = list_keymap_lookup_one_noinherit(&current_map, event);
        if let Some(prefix_map) = resolve_prefix_keymap_binding_in_obarray(obarray, &binding) {
            current_map = prefix_map;
        } else if binding.is_nil() {
            // No binding, create a new prefix keymap.
            // Matches GNU `define_as_prefix` (keymap.c:1446-1452).
            let prefix_map = make_sparse_list_keymap();
            list_keymap_define(current_map, *event, prefix_map);
            current_map = prefix_map;
        } else {
            // GNU `Fdefine_key` only calls `define_as_prefix` after
            // `access_keymap` returns nil.  Existing non-prefix bindings are
            // errors, even though a fresh undefined key is promoted to a new
            // prefix keymap.
            let full = describe_event_sequence(events);
            let prefix = describe_event_sequence(&events[..=i]);
            return Err(format!(
                "Key sequence {full} starts with non-prefix key {prefix}"
            ));
        }
    }
    Ok(())
}

/// Generate a human-readable description of an event sequence for error messages.
/// Uses the same format as GNU Emacs `key-description`: function keys use
/// angle brackets (e.g., `<f1>`), characters use their standard description.
fn describe_event_sequence(events: &[Value]) -> String {
    use super::keyboard::pure::describe_single_key_value;
    events
        .iter()
        .map(|e| {
            // An error message is Rust text, so decode here, where the loss is
            // visible and harmless; the Lisp-facing builders keep the bytes.
            describe_single_key_value(e, false)
                .map(|bytes| crate::emacs_core::emacs_char::to_utf8_lossy(&bytes))
                .unwrap_or_else(|_| {
                    if let Some(name) = e.as_symbol_name() {
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
pub fn list_keymap_copy(keymap: &Value) -> Value {
    list_keymap_copy_impl(keymap, 0)
}

fn list_keymap_copy_impl(keymap: &Value, depth: usize) -> Value {
    if depth > 100 {
        tracing::warn!("list_keymap_copy: recursion depth limit, possible infinite loop");
        return *keymap;
    }

    if !keymap.is_cons() {
        return *keymap;
    };
    let pair_car = keymap.cons_car();
    let pair_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(pair_car) {
        return *keymap;
    }

    let mut elements = vec![KeymapMarker::Keymap.symbol_value()];
    let mut cursor = pair_cdr;
    let mut tail_parent = Value::NIL;

    while cursor.is_cons() {
        if is_list_keymap(&cursor) {
            // Parent keymap: keep shared (don't recursively copy parent chain)
            tail_parent = cursor;
            break;
        }
        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();

        if is_char_table(&entry_car) {
            // Deep-copy char-table: clone the vector, then recursively copy
            // any keymap entries within it.
            elements.push(copy_char_table_for_keymap(&entry_car, depth));
        } else if is_list_keymap(&entry_car) {
            // Nested keymap element — recursively copy
            elements.push(list_keymap_copy_impl(&entry_car, depth + 1));
        } else if entry_car.is_cons() {
            // Alist entry (EVENT . DEF) — copy the cons, recurse if DEF is a keymap
            let binding_car = entry_car.cons_car();
            let binding_cdr = entry_car.cons_cdr();
            let copied_def = copy_keymap_item(&binding_cdr, depth);
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
fn copy_keymap_item(item: &Value, depth: usize) -> Value {
    if is_list_keymap(item) {
        return list_keymap_copy_impl(item, depth + 1);
    }
    // Handle menu items etc. — for now, just return as-is for non-keymaps
    *item
}

/// Deep-copy a char-table used in a keymap.
///
/// GNU `copy_keymap_1` first calls `copy-sequence` on the char-table, then
/// walks the copied table with `map_char_table` and replaces each binding with
/// `copy_keymap_item`.  Keep that shape here so real char-table objects and
/// legacy vector-backed tables follow the same semantics.
fn copy_char_table_for_keymap(ct: &Value, depth: usize) -> Value {
    let Some(copied) = super::chartable::copy_char_table(*ct) else {
        return *ct;
    };

    let Ok(entries) = super::chartable::char_table_local_entries(&copied) else {
        return copied;
    };
    for (range, value) in entries {
        let copied_value = copy_keymap_item(&value, depth + 1);
        let _ =
            super::chartable::builtin_set_char_table_range(vec![copied, range, copied_value], None);
    }

    if copied.is_vector() {
        let Some(mut new_vec) = copied.as_vector_data().map(|data| data.to_vec()) else {
            return copied;
        };
        if let Some(cache_range) = char_table_ascii_cache_range(&new_vec) {
            for i in cache_range {
                let val = new_vec[i];
                new_vec[i] = copy_keymap_item(&val, depth + 1);
            }
        }

        let data_start = char_table_data_start(&new_vec);
        let mut i = data_start;
        while i + 1 < new_vec.len() {
            let val = new_vec[i + 1];
            new_vec[i + 1] = copy_keymap_item(&val, depth + 1);
            i += 2;
        }
        return Value::vector(new_vec);
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
    obarray: Option<&Obarray>,
    out: &mut Vec<Value>,
) {
    let meta_prefix_char = obarray
        .and_then(|o| KeymapStateVariable::MetaPrefixChar.global_value(o))
        .and_then(|v| v.as_fixnum())
        .unwrap_or(KEY_META_PREFIX_CHAR_DEFAULT);

    let start = if prefix.is_empty() {
        KeymapResolution::Loaded(keymap)
    } else {
        let reached = match obarray {
            Some(obarray) => lookup_key_in_obarray(obarray, &keymap, prefix, true),
            None => Value::NIL,
        };
        // GNU keeps an autoload symbol the prefix reaches: "If the keymap is
        // autoloaded `tem' is not a cons-cell, but we still want to return it."
        match resolve_keymap_or_autoload(reached, obarray) {
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
        list_keymap_for_each_binding_recursive(&thismap, obarray, |key, def| {
            let Some(submap) = resolve_keymap_or_autoload(get_keyelt(def), obarray) else {
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
        });

        for (newseq, submap, metized) in found {
            let is_cycle = maps.iter().any(|(seen_seq, seen_map)| {
                // GNU's cycle test is Frassq, i.e. EQ on the reached map --
                // which may be a cons OR an autoload symbol.
                seen_map.as_value().bits() == submap.as_value().bits()
                    && seen_seq.len() <= thisseq.len()
                    && seen_seq
                        .iter()
                        .zip(thisseq.iter())
                        .all(|(a, b)| a.bits() == b.bits())
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

/// Guard against a circular keymap spine (a `setcdr`-built cycle).
const MAX_KEYMAP_SPINE_STEPS: usize = 100_000;
/// Guard against mutually-nested keymaps when a consumer recurses into submaps.
const MAX_KEYMAP_WALK_DEPTH: usize = 64;

/// One element of a keymap's own spine, classified exactly as GNU
/// `map_keymap_internal` / `access_keymap_1` (keymap.c) classify it.
///
/// A keymap is `(keymap ELEMENT... . TAIL)`, and "element" is an untyped union in
/// Lisp. Re-decoding that union ad-hoc at each call site is how the shapes
/// drift: this scan silently lacked the inline-vector arm that
/// [`list_keymap_for_each_binding_recursive`] had, so a command bound through a
/// vector was invisible to `where-is-internal` even though `lookup-key` found
/// it. Decode the union once, here, and let `match` exhaustiveness oblige every
/// consumer to face every shape.
pub(crate) enum KeymapElement {
    /// A key -> binding pair. Sources: a `(KEY . BINDING)` cons, one slot of an
    /// inline vector (key = slot index), or one entry of a char-table (key = a
    /// character, or a `(FROM . TO)` range).
    ///
    /// `value` is normalized like GNU `map_keymap_item`: a `t` value is an
    /// explicit unbinding and is reported as nil.
    Binding { key: Value, value: Value },
    /// A keymap embedded in the spine: a composed submap
    /// (`make-composed-keymap`) or the parent. GNU `map_keymap` treats the two
    /// identically -- recurse into it, then continue with the rest of the spine
    /// -- and both share the enclosing keymap's prefix.
    Submap(Value),
    /// A spine tail that is not a cons and does NOT name a keymap.
    ///
    /// GNU's spine loops retry `get_keymap` on a non-cons tail
    /// (`access_keymap_1`: `(CONSP (tail) || (tail = get_keymap (tail, 0,
    /// autoload), CONSP (tail)))`; `map_keymap`: `if (!CONSP (map)) map =
    /// get_keymap (map, ...)`), so a symbol whose function cell is a keymap
    /// CONTINUES the spine. [`resolve_keymap`] performs exactly that, and the walk
    /// keeps going; this variant reports only what could not be resolved -- a
    /// symbol naming no keymap, or a walk with no obarray to resolve through.
    IndirectTail(#[expect(dead_code, reason = "reported for completeness")] Value),
    /// The keymap's prompt string (`make-sparse-keymap` PROMPT).
    Prompt(Value),
}

/// GNU `get_keymap (OBJECT, 0, autoload)` (src/keymap.c): OBJECT as a keymap, or
/// `None` if it is not one.
///
/// A `(keymap . ...)` cons is a keymap outright; anything else is passed through
/// function indirection first, so a SYMBOL whose function cell holds a keymap
/// (`(fset 'my-map (make-sparse-keymap))`, a `defalias` chain, a
/// `define-prefix-command` symbol) resolves to that keymap. This is the one place
/// that question is answered, because GNU asks it from every spine walk --
/// forward lookup, reverse `where-is` scan, `map_keymap` -- and answering it in
/// only some of them makes lookup directions disagree.
///
/// `obarray` is `None` for a purely structural walk; a symbol then stays
/// unresolved rather than being resolved through some other binding of the name.
/// Autoloading (GNU's third argument) is deliberately not performed here: it
/// evaluates Lisp, so it belongs to a context-taking caller.
pub(crate) fn resolve_keymap(value: Value, obarray: Option<&Obarray>) -> Option<Value> {
    match resolve_keymap_or_autoload(value, obarray)? {
        KeymapResolution::Loaded(map) => Some(map),
        // GNU's spine loops retry `get_keymap` and then test CONSP on the
        // result, so an autoload symbol -- returned as itself -- ends the
        // walk exactly like a non-keymap does.
        KeymapResolution::UnloadedAutoload(_) => None,
    }
}

/// The two shapes GNU `get_keymap (OBJECT, 0, 0)` can answer with, kept apart
/// in the type instead of collapsed into one `Value` the caller re-inspects.
pub(crate) enum KeymapResolution {
    /// A loaded `(keymap . ...)` list keymap: scannable.
    Loaded(Value),
    /// A SYMBOL whose function cell is an `(autoload FILE DOC INTERACTIVE
    /// keymap)` form that has not been loaded.  GNU's `get_keymap` returns the
    /// symbol itself here: it IS a keymap for listing purposes -- `keymapp`
    /// answers t and `Faccessible_keymaps` reports it as the map a prefix
    /// reaches -- but its bindings cannot be scanned without running Lisp
    /// ("Since we can't run lisp code, we can't scan autoloaded maps.",
    /// keymap.c Faccessible_keymaps).  Only a Context-taking caller such as
    /// `map-keymap` (GNU `map_keymap`, autoload=1) may load and descend, which
    /// is how help.el's describe-map expands e.g. the `C-x C-k` kmacro-keymap
    /// section in a batch session where kmacro.el is not yet loaded.
    UnloadedAutoload(Value),
}

impl KeymapResolution {
    /// The Value GNU's `get_keymap (OBJECT, 0, 0)` returns for this answer:
    /// the list keymap itself, or the autoload symbol standing in for one.
    pub(crate) fn as_value(&self) -> Value {
        match self {
            Self::Loaded(value) | Self::UnloadedAutoload(value) => *value,
        }
    }
}

/// GNU `get_keymap (OBJECT, 0, 0)` with both possible keymap answers: a loaded
/// list keymap, or the unloaded-autoload symbol standing for one.  Callers that
/// only ever scan use [`resolve_keymap`]; callers that LIST keymaps the way GNU
/// does (`accessible-keymaps`, `keymapp`) must face both arms.
pub(crate) fn resolve_keymap_or_autoload(
    value: Value,
    obarray: Option<&Obarray>,
) -> Option<KeymapResolution> {
    if value.is_nil() {
        return None;
    }
    if is_list_keymap(&value) {
        return Some(KeymapResolution::Loaded(value));
    }
    let name = value.as_symbol_name()?;
    let function = obarray?.indirect_function(name)?;
    if is_list_keymap(&function) {
        return Some(KeymapResolution::Loaded(function));
    }
    is_keymap_autoload_form(&function).then_some(KeymapResolution::UnloadedAutoload(value))
}

/// GNU `map_keymap_item`: a `t` binding shadows lower-precedence keymaps exactly
/// like an explicit nil binding, so it is reported as nil.
fn normalized_binding_value(value: Value) -> Value {
    if matches!(value.kind(), ValueKind::T) {
        Value::NIL
    } else {
        value
    }
}

/// Visit the elements of ONE keymap's spine, mirroring GNU
/// `map_keymap_internal`: every binding at this level, in spine order.
///
/// Descent is deliberately *not* performed here -- embedded keymaps are yielded
/// as [`KeymapElement::Submap`] so each consumer keeps its own policy. A
/// single-level scan ignores them; a `map_keymap`-style walk recurses into them
/// at the same prefix (they share this keymap's prefix). Elements that match none
/// of GNU's cases are skipped, as GNU skips them.
pub(crate) fn for_each_keymap_element<F>(keymap: &Value, obarray: Option<&Obarray>, mut f: F)
where
    F: FnMut(KeymapElement),
{
    let Some(mut cursor) = keymap_binding_spine(keymap) else {
        return;
    };
    let mut steps = 0usize;
    while cursor.is_cons() {
        steps += 1;
        if steps > MAX_KEYMAP_SPINE_STEPS {
            return;
        }
        // The spine tail is itself a keymap (the classic parent). Everything
        // remaining lives inside it, so hand it over and stop walking this level.
        if is_list_keymap(&cursor) {
            f(KeymapElement::Submap(cursor));
            return;
        }

        let element = cursor.cons_car();
        let rest = cursor.cons_cdr();

        if is_list_keymap(&element) {
            // A composed submap. GNU `map_keymap` recurses into it and then
            // continues with the rest of the spine, so do not stop here.
            f(KeymapElement::Submap(element));
        } else if super::chartable::is_char_table(&element) {
            super::chartable::for_each_non_nil_char_table_run(&element, |key, value| {
                f(KeymapElement::Binding {
                    key,
                    value: normalized_binding_value(value),
                });
            });
        } else if element.is_vector() {
            // An inline vector indexes bindings by character code. GNU reports
            // every slot, empty ones included.
            if let Some(items) = element.as_vector_data() {
                for (index, binding) in items.iter().enumerate() {
                    f(KeymapElement::Binding {
                        key: Value::fixnum(index as i64),
                        value: normalized_binding_value(*binding),
                    });
                }
            }
        } else if element.is_cons() {
            f(KeymapElement::Binding {
                key: element.cons_car(),
                value: normalized_binding_value(element.cons_cdr()),
            });
        } else if element.is_string() {
            f(KeymapElement::Prompt(element));
        }

        cursor = rest;

        // GNU's spine loops retry `get_keymap` whenever the tail stops being a
        // cons, so a tail that NAMES a keymap continues this same walk -- its
        // bindings belong to this keymap, at this prefix.
        if !cursor.is_cons()
            && !cursor.is_nil()
            && let Some(resolved) = resolve_keymap(cursor, obarray)
            && let Some(spine) = keymap_binding_spine(&resolved)
        {
            cursor = spine;
        }
    }

    // A non-nil, non-cons tail that names no keymap.
    if !cursor.is_nil() {
        f(KeymapElement::IndirectTail(cursor));
    }
}

/// Iterate over all bindings in a keymap (not following parent or submaps).
/// Calls `f(event, def)` for each binding.
///
/// `obarray` resolves a spine tail that names a keymap (GNU `get_keymap`); pass
/// `None` for a purely structural walk, which then stops at such a tail.
pub fn list_keymap_for_each_binding<F>(keymap: &Value, obarray: Option<&Obarray>, mut f: F)
where
    F: FnMut(Value, Value),
{
    for_each_keymap_element(keymap, obarray, |element| match element {
        KeymapElement::Binding { key, value } => f(key, value),
        // Single-level by contract: descent, and the prefix bookkeeping it
        // needs, belong to the caller.
        KeymapElement::Submap(_) | KeymapElement::IndirectTail(_) | KeymapElement::Prompt(_) => {}
    });
}

/// Iterate over all bindings in a keymap and its embedded/parent keymaps.
///
/// This mirrors GNU `map_keymap`, which descends into embedded keymaps before
/// continuing with the rest of the spine, then follows parent keymaps.  It is
/// the shape used by `map_keymap_canonical` after `keymap-canonicalize` when
/// building menu bars, where major modes such as Org store their menu-bar
/// prefix as `(keymap (keymap ...) (keymap ...))`.
pub fn list_keymap_for_each_binding_recursive<F>(
    keymap: &Value,
    obarray: Option<&Obarray>,
    mut f: F,
) where
    F: FnMut(Value, Value),
{
    fn walk<F>(keymap: &Value, obarray: Option<&Obarray>, f: &mut F, depth: usize)
    where
        F: FnMut(Value, Value),
    {
        if depth > MAX_KEYMAP_WALK_DEPTH {
            return;
        }
        for_each_keymap_element(keymap, obarray, |element| match element {
            KeymapElement::Binding { key, value } => f(key, value),
            // Composed submaps and the parent share this keymap's prefix, so
            // their bindings belong to this traversal (GNU `map_keymap`).
            KeymapElement::Submap(submap) => walk(&submap, obarray, f, depth + 1),
            // A tail naming no keymap ends this spine, as it does for GNU.
            KeymapElement::IndirectTail(_) | KeymapElement::Prompt(_) => {}
        });
    }

    walk(keymap, obarray, &mut f, 0);
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
