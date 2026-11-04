//! Pure keymap lookup substrate (stage-4 W1/E4).
//!
//! GNU keymap.c's structural lookups: get_keyelt / get_keymap /
//! access_keymap, the map-keymap spine walks and the reverse-lookup helpers
//! that need no upper state. Moved out of the core command keymap module so
//! the evaluator can call it downward. The `&mut Context` resolution impl is
//! here because the trait and Context are same-crate in neovm-eval
//! (`&mut LocalType` is a fundamental-type local impl).

use super::*;
use crate::emacs_core::chartable::{
    builtin_char_table_range_with_style, builtin_set_char_table_range_with_style, is_char_table,
    make_char_table_value,
};
use crate::tagged::value::SymbolMode;
use neovm_value::keyboard_pure::{
    KEY_CHAR_ALT, KEY_CHAR_CODE_MASK, KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META,
    KEY_CHAR_MOD_MASK, KEY_CHAR_SHIFT, KEY_CHAR_SUPER, reorder_event_symbol_modifiers,
};
use strum::{EnumString, IntoStaticStr};

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

    pub fn from_value(value: Value, symbol_mode: SymbolMode) -> Option<Self> {
        let symbol = value.as_symbol_id(symbol_mode)?;
        [Self::Keymap, Self::MenuItem, Self::Remap]
            .into_iter()
            .find(|marker| symbol == marker.symbol_id())
    }

    pub fn is_value(self, value: Value, symbol_mode: SymbolMode) -> bool {
        value.as_symbol_id(symbol_mode) == Some(self.symbol_id())
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

    pub fn from_value(value: Value, symbol_mode: SymbolMode) -> Option<Self> {
        let id = value.as_symbol_id(symbol_mode)?;
        let keyword = Self::from_keyword(resolve_sym(id))?;
        (id == intern(keyword.keyword())).then_some(keyword)
    }

    pub fn is_value(self, value: Value, symbol_mode: SymbolMode) -> bool {
        Self::from_value(value, symbol_mode) == Some(self)
    }
}

/// Check if a value is a keymap: `(consp x) && (car x) == 'keymap`.
pub fn is_list_keymap(v: &Value, symbol_mode: SymbolMode) -> bool {
    match v.kind() {
        ValueKind::Cons => KeymapMarker::Keymap.is_value(v.cons_car(), symbol_mode),
        _ => false,
    }
}

pub fn keymap_symbol_id(value: &Value, symbol_mode: SymbolMode) -> Option<SymId> {
    value.as_symbol_id(symbol_mode)
}

pub fn resolve_indirect_function_by_id_in_obarray(
    obarray: &Obarray,
    symbol: SymId,
    symbol_mode: SymbolMode,
) -> Option<(SymId, Value)> {
    let mut current = symbol;
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(current) {
            return None;
        }
        let sym = obarray.get_by_id(current)?;
        if sym.function.get().is_nil() {
            return None;
        }
        let function = sym.function.get();
        if let Some(next_symbol) = keymap_symbol_id(&function, symbol_mode) {
            current = next_symbol;
            continue;
        }
        return Some((current, function));
    }
}

pub fn is_keymap_autoload_form(value: &Value, symbol_mode: SymbolMode) -> bool {
    if !crate::emacs_core::autoload::is_autoload_value(value, symbol_mode) {
        return false;
    }
    list_to_vec(value)
        .and_then(|items| items.get(4).copied())
        .is_some_and(|kind| KeymapMarker::Keymap.is_value(kind, symbol_mode))
}

pub fn get_keymap_in_obarray(
    obarray: &Obarray,
    value: &Value,
    error_if_not_keymap: bool,
    symbol_mode: SymbolMode,
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

    if is_list_keymap(value, symbol_mode) {
        return Ok(*value);
    }

    if let Some(symbol) = keymap_symbol_id(value, symbol_mode)
        && let Some((_, function)) =
            resolve_indirect_function_by_id_in_obarray(obarray, symbol, symbol_mode)
    {
        if is_list_keymap(&function, symbol_mode) {
            return Ok(function);
        }
        if is_keymap_autoload_form(&function, symbol_mode) && !error_if_not_keymap {
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

pub fn maybe_keymap_in_obarray(
    obarray: &Obarray,
    value: &Value,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    get_keymap_in_obarray(obarray, value, false, symbol_mode)
        .ok()
        .filter(|value| is_list_keymap(value, symbol_mode))
}

/// Strip menu-item wrappers from a keymap binding, mirroring `get_keyelt`
/// in official Emacs `keymap.c`.
///
/// - `(STRING . DEFN)` → `DEFN`  (menu label)
/// - `(STRING . (STRING . DEFN))` → `DEFN`  (menu label + help string)
/// - `(menu-item NAME DEFN ...)` → `DEFN`  (extended menu item)
/// - anything else → returned as-is
pub fn get_keyelt(binding: Value, symbol_mode: SymbolMode) -> Value {
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
        if KeymapMarker::MenuItem.is_value(pair_car, symbol_mode) {
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

pub fn nth_list_element(mut list: Value, mut index: usize) -> Option<Value> {
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

pub fn menu_item_filter(tail: Value, symbol_mode: SymbolMode) -> Option<Value> {
    let mut cursor = tail;
    while cursor.is_cons() {
        let key = cursor.cons_car();
        let rest = cursor.cons_cdr();
        if MenuItemProperty::Filter.is_value(key, symbol_mode) && rest.is_cons() {
            return Some(rest.cons_car());
        }
        cursor = rest;
    }
    None
}

pub fn get_keyelt_runtime(eval: &mut Context, binding: Value, autoload: bool) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
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

        if KeymapMarker::MenuItem.is_value(pair_car, symbol_mode) {
            if !pair_cdr.is_cons() {
                return Ok(object);
            }
            let tail = pair_cdr.cons_cdr();
            object = nth_list_element(pair_cdr, 1).unwrap_or(Value::NIL);
            if autoload && let Some(filter) = menu_item_filter(tail, symbol_mode) {
                // GNU applies the filter through `menu_item_eval_property`
                // (src/keyboard.c:8873-8884): `inhibit-redisplay` bound,
                // `error` caught to nil, quit still nonlocal.
                object = eval.eval_menu_item_property(filter, vec![object])?;
            }
            continue;
        }

        return Ok(object);
    }
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
pub fn keymap_binding_spine(keymap: &Value, symbol_mode: SymbolMode) -> Option<Value> {
    if !keymap.is_cons() {
        return None;
    }

    if KeymapMarker::Keymap.is_value(keymap.cons_car(), symbol_mode) {
        return Some(keymap.cons_cdr());
    }

    Some(*keymap)
}

pub fn maybe_resolve_keyelt(
    binding: Value,
    resolve_keyelt: bool,
    symbol_mode: SymbolMode,
) -> Value {
    if resolve_keyelt {
        get_keyelt(binding, symbol_mode)
    } else {
        binding
    }
}

/// How one keymap walk resolves a raw definition, mirroring the two depths of
/// GNU `get_keyelt`.
///
/// GNU `access_keymap_1` resolves every definition *before* deciding whether it
/// is a prefix keymap to compose with the parent's prefix, so the resolver must
/// run inside the walk.  Resolving after the walk loses the parent composition
/// of a menu-item's filter result: `evil-esc-mode` rebinds `\e` in
/// `input-decode-map` to `(menu-item "" CAPTURED :filter evil-esc)`, and only
/// the composed result still carries the terminal map's `\e[200~`.
///
/// The capability is encoded as the implementor rather than a flag: a walk
/// given `Option<&Obarray>` can only strip indirections, while a walk given
/// `&mut Context` may evaluate a menu-item `:filter` exactly as GNU's
/// `autoload` argument does.  Neither implementor needs a lifetime-carrying
/// adapter struct, so no lifetime annotations appear in signatures.
pub trait KeyeltResolution {
    /// The obarray whose function cells a symbol keymap tail resolves through.
    fn lookup_obarray(&self) -> Option<&Obarray>;
    /// GNU `get_keyelt (binding, autoload)`.  `resolve_keyelt` is Neo's
    /// existing "strip indirections at all" flag; runtime resolvers evaluate
    /// menu-item filters regardless of it.
    fn resolve(
        &mut self,
        binding: Value,
        resolve_keyelt: bool,
        symbol_mode: SymbolMode,
    ) -> Result<Value, Flow>;
}

/// Structural resolution for walks without an evaluator.
impl KeyeltResolution for Option<&Obarray> {
    fn lookup_obarray(&self) -> Option<&Obarray> {
        *self
    }

    fn resolve(
        &mut self,
        binding: Value,
        resolve_keyelt: bool,
        symbol_mode: SymbolMode,
    ) -> Result<Value, Flow> {
        Ok(maybe_resolve_keyelt(binding, resolve_keyelt, symbol_mode))
    }
}

/// Runtime resolution: the borrowed evaluator *is* the resolver.
impl KeyeltResolution for &mut Context {
    fn lookup_obarray(&self) -> Option<&Obarray> {
        Some((**self).obarray())
    }

    fn resolve(
        &mut self,
        binding: Value,
        _resolve_keyelt: bool,
        _symbol_mode: SymbolMode,
    ) -> Result<Value, Flow> {
        get_keyelt_runtime(*self, binding, true)
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
pub struct KeymapLookupEvent(Value);

impl KeymapLookupEvent {
    pub fn from_event(event: Value, symbol_mode: SymbolMode) -> Self {
        let head = if event.is_cons() {
            event.cons_car()
        } else {
            event
        };
        let head = symbol_mode.unwrap(head);
        let canonical = match head.kind() {
            ValueKind::Symbol(_) => reorder_event_symbol_modifiers(head, SymbolMode::Bare),
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

    fn matches_stored_key(self, stored: Value, symbol_mode: SymbolMode) -> bool {
        eq_value_swp(&stored, &self.0, symbol_mode == SymbolMode::Positioned)
    }
}

pub fn lookup_in_keymap_level_impl(
    keymap: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    resolution: &mut dyn KeyeltResolution,
    symbol_mode: SymbolMode,
) -> Result<Option<Value>, Flow> {
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
        return Ok(None);
    };
    let mut entries = 0;
    let mut t_binding: Option<Value> = None;
    let mut prefix_binding: Option<Value> = None;
    let mut nil_binding_found = false;
    while cursor.is_cons() {
        if is_list_keymap(&cursor, symbol_mode) {
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
                    let result = builtin_char_table_range_with_style(
                        vec![entry_car, event.value()],
                        None,
                        symbol_mode,
                    )
                    .unwrap_or(Value::NIL);
                    if !result.is_nil() {
                        // Qt in char-table means explicitly nil binding
                        // (shadows parent), matching GNU keymap.c:455-459
                        let val = if eq_value_swp(
                            &result,
                            &Value::T,
                            symbol_mode == SymbolMode::Positioned,
                        ) {
                            Value::NIL
                        } else {
                            result
                        };
                        let val = resolution.resolve(val, resolve_keyelt, symbol_mode)?;
                        if val.is_nil() {
                            nil_binding_found = true;
                        } else if is_list_keymap(&val, symbol_mode) {
                            prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                        } else if prefix_binding.is_some() {
                            break;
                        } else {
                            return Ok(Some(val));
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
                    let val = resolution.resolve(val, resolve_keyelt, symbol_mode)?;
                    if val.is_nil() {
                        nil_binding_found = true;
                    } else if is_list_keymap(&val, symbol_mode) {
                        prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                    } else if prefix_binding.is_some() {
                        break;
                    } else {
                        return Ok(Some(val));
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
        if entry_car.is_cons() && is_list_keymap(&entry_car, symbol_mode) {
            if let Some(found) = access_keymap_in_member(
                &entry_car,
                event,
                noinherit,
                t_ok,
                resolve_keyelt,
                resolution,
                symbol_mode,
            )? {
                if found.is_nil() {
                    nil_binding_found = true;
                } else if is_list_keymap(&found, symbol_mode) {
                    prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, found));
                } else if prefix_binding.is_some() {
                    break;
                } else {
                    return Ok(Some(found));
                }
            }
            cursor = entry_cdr;
            continue;
        }

        // Alist entry: (EVENT . DEF)
        if entry_car.is_cons() {
            let binding_car = entry_car.cons_car();
            let binding_cdr = entry_car.cons_cdr();
            if event.matches_stored_key(binding_car, symbol_mode) {
                let val = resolution.resolve(binding_cdr, resolve_keyelt, symbol_mode)?;
                if val.is_nil() {
                    nil_binding_found = true;
                } else if is_list_keymap(&val, symbol_mode) {
                    prefix_binding = Some(accumulate_prefix_keymap(prefix_binding, val));
                } else if prefix_binding.is_some() {
                    break;
                } else {
                    return Ok(Some(val));
                }
            }
            // Check for (t . COMMAND) default binding.
            // GNU keymap.c:425-429: when t_ok, record the first t binding
            // but keep scanning for a specific match.
            if t_ok
                && t_binding.is_none()
                && eq_value_swp(
                    &binding_car,
                    &Value::T,
                    symbol_mode == SymbolMode::Positioned,
                )
            {
                t_binding = Some(resolution.resolve(binding_cdr, resolve_keyelt, symbol_mode)?);
            }
        }

        cursor = entry_cdr;

        // GNU `access_keymap_1`'s loop condition retries `get_keymap` as soon as
        // the tail stops being a cons, so a tail that NAMES a keymap continues
        // this same scan -- its bindings belong to this keymap, not to a parent.
        if !cursor.is_cons()
            && !cursor.is_nil()
            && let Some(resolved) = resolve_keymap(cursor, resolution.lookup_obarray(), symbol_mode)
            && let Some(spine) = keymap_binding_spine(&resolved, symbol_mode)
        {
            cursor = spine;
        }
    }

    // If no specific binding found but we have a t default binding, use it.
    // Matches GNU keymap.c:486-487.
    if let Some(binding) = prefix_binding {
        Ok(Some(binding))
    } else if nil_binding_found {
        Ok(Some(Value::NIL))
    } else {
        Ok(t_binding)
    }
}

/// One member of a composed keymap, searched the way GNU searches it: with its
/// own parent chain, and reporting "no entry here" apart from "an entry that is
/// nil".
///
/// That distinction is why this cannot just call [`list_keymap_access_impl`],
/// which returns nil for both: an explicit nil entry SHADOWS the members and
/// parents that follow, while no entry at all lets them through.
pub fn access_keymap_in_member(
    member: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    resolution: &mut dyn KeyeltResolution,
    symbol_mode: SymbolMode,
) -> Result<Option<Value>, Flow> {
    let found = lookup_in_keymap_level_impl(
        member,
        event,
        noinherit,
        t_ok,
        resolve_keyelt,
        resolution,
        symbol_mode,
    )?;
    if noinherit {
        return Ok(found);
    }
    let parent = get_keymap_tail_parent(member, symbol_mode);
    match found {
        // A prefix keymap merges with whatever the member's parent binds for the
        // same event, exactly as it would if the member were looked up directly.
        Some(binding) if is_list_keymap(&binding, symbol_mode) && !parent.is_nil() => {
            let parent_binding = list_keymap_access_with_event(
                &parent,
                event,
                false,
                t_ok,
                resolve_keyelt,
                resolution,
                symbol_mode,
            )?;
            if is_list_keymap(&parent_binding, symbol_mode) {
                Ok(Some(compose_prefix_with_parent_keymap(
                    &binding,
                    &parent_binding,
                    symbol_mode,
                )))
            } else {
                Ok(Some(binding))
            }
        }
        Some(binding) => Ok(Some(binding)),
        // Nothing at the member's own level: its parent chain still applies.
        None if !parent.is_nil() => access_keymap_in_member(
            &parent,
            event,
            noinherit,
            t_ok,
            resolve_keyelt,
            resolution,
            symbol_mode,
        ),
        None => Ok(None),
    }
}

/// Get the parent keymap from a keymap (the tail after all alist entries).
pub fn get_keymap_tail_parent(keymap: &Value, symbol_mode: SymbolMode) -> Value {
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
        return Value::NIL;
    };
    while cursor.is_cons() {
        if is_list_keymap(&cursor, symbol_mode) {
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
pub fn list_keymap_access(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, true, None, symbol_mode)
}

/// [`list_keymap_access`] with the obarray GNU's `access_keymap_1` resolves a
/// symbol spine tail through (`get_keymap`), for the lookup paths that have one.
pub fn list_keymap_access_in_obarray(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    obarray: &Obarray,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_impl(
        keymap,
        event,
        noinherit,
        t_ok,
        true,
        Some(obarray),
        symbol_mode,
    )
}

pub fn list_keymap_access_unresolved(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_impl(keymap, event, noinherit, t_ok, false, None, symbol_mode)
}

/// Look up ONE event, following a spine tail that names a keymap -- the lookup
/// GNU's `access_keymap` performs. Used by the `lookup-key` paths, which have the
/// obarray in hand; the obarray-less variants above stay structural.
pub fn list_keymap_lookup_one_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_in_obarray(keymap, event, false, false, obarray, symbol_mode)
}

pub fn list_keymap_lookup_one_t_ok_in_obarray(
    keymap: &Value,
    event: &Value,
    obarray: &Obarray,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_in_obarray(keymap, event, false, true, obarray, symbol_mode)
}

pub fn list_keymap_access_with_event(
    keymap: &Value,
    event: KeymapLookupEvent,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    resolution: &mut dyn KeyeltResolution,
    symbol_mode: SymbolMode,
) -> Result<Value, Flow> {
    let mut current = *keymap;
    let mut depth = 0;
    const MAX_KEYMAP_DEPTH: usize = 50;

    loop {
        depth += 1;
        if depth > MAX_KEYMAP_DEPTH {
            tracing::warn!("list_keymap_access: depth limit reached, possible cycle");
            return Ok(Value::NIL);
        }

        // Look up the event in the current keymap level only.
        // Some(val) means "found" (val may be nil for explicit nil binding).
        // None means "not found at this level".
        match lookup_in_keymap_level_impl(
            &current,
            event,
            noinherit,
            t_ok,
            resolve_keyelt,
            resolution,
            symbol_mode,
        )? {
            Some(binding) => {
                if !noinherit && is_list_keymap(&binding, symbol_mode) {
                    // Found a prefix keymap at this level. Check if parent
                    // also has a prefix keymap for the same event. If so,
                    // create a composed keymap: (keymap child-sub . parent-sub)
                    let parent = get_keymap_tail_parent(&current, symbol_mode);
                    if !parent.is_nil() {
                        let parent_binding = list_keymap_access_with_event(
                            &parent,
                            event,
                            false,
                            t_ok,
                            resolve_keyelt,
                            resolution,
                            symbol_mode,
                        )?;
                        if is_list_keymap(&parent_binding, symbol_mode) {
                            return Ok(compose_prefix_with_parent_keymap(
                                &binding,
                                &parent_binding,
                                symbol_mode,
                            ));
                        }
                    }
                }
                // Return the found binding (even if nil — nil shadows parents)
                return Ok(binding);
            }
            None => {
                // No binding at this level. Follow parent chain if allowed.
                if noinherit {
                    return Ok(Value::NIL);
                }
                let parent = get_keymap_tail_parent(&current, symbol_mode);
                if parent.is_nil() {
                    return Ok(Value::NIL);
                }
                current = parent;
            }
        }
    }
}

pub fn list_keymap_access_impl(
    keymap: &Value,
    event: &Value,
    noinherit: bool,
    t_ok: bool,
    resolve_keyelt: bool,
    obarray: Option<&Obarray>,
    symbol_mode: SymbolMode,
) -> Value {
    let mut resolution = obarray;
    list_keymap_access_with_event(
        keymap,
        KeymapLookupEvent::from_event(*event, symbol_mode),
        noinherit,
        t_ok,
        resolve_keyelt,
        &mut resolution,
        symbol_mode,
    )
    .unwrap_or(Value::NIL)
}

/// Look up a single event in a keymap, following the parent chain.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=false, t_ok=false`.
/// When a prefix keymap is found, it is composed with parent prefix
/// keymaps to create a merged keymap that includes all bindings from
/// the entire inheritance chain.
///
/// Returns the binding or `Value::NIL` if not found.
pub fn list_keymap_lookup_one(keymap: &Value, event: &Value, symbol_mode: SymbolMode) -> Value {
    list_keymap_access(keymap, event, false, false, symbol_mode)
}

/// Look up a single event in a keymap, following the parent chain,
/// accepting `(t . COMMAND)` default bindings.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=false, t_ok=true`.
pub fn list_keymap_lookup_one_t_ok(
    keymap: &Value,
    event: &Value,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access(keymap, event, false, true, symbol_mode)
}

/// Look up a single event in a keymap without stripping menu-item wrappers.
///
/// This is used by `read-key-sequence` key translation maps.  GNU
/// `access_keymap_keyremap` calls `access_keymap` with autoloading enabled, so
/// menu-item `:filter` properties must be evaluated by the keyboard runtime
/// rather than discarded by the pure keymap lookup layer.
pub fn list_keymap_lookup_one_unresolved(
    keymap: &Value,
    event: &Value,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access_unresolved(keymap, event, false, false, symbol_mode)
}

/// Look up a single event in a keymap without following the parent chain.
///
/// This mirrors GNU Emacs `access_keymap` with `noinherit=true`.
/// Used by `define-key` to only check the current keymap level.
pub fn list_keymap_lookup_one_noinherit(
    keymap: &Value,
    event: &Value,
    symbol_mode: SymbolMode,
) -> Value {
    list_keymap_access(keymap, event, true, false, symbol_mode)
}

pub fn accumulate_prefix_keymap(existing: Option<Value>, next: Value) -> Value {
    match existing {
        Some(current) => compose_prefix_keymaps(&current, &next),
        None => next,
    }
}

/// Compose prefix keymaps found at the same keymap level.  GNU
/// `access_keymap_1` keeps those as separate embedded members:
/// `(keymap MAP1 MAP2 ...)`.
pub fn compose_prefix_keymaps(first: &Value, second: &Value) -> Value {
    Value::list(vec![KeymapMarker::Keymap.symbol_value(), *first, *second])
}

/// Compose a direct prefix map with an inherited parent prefix map.  GNU
/// `access_keymap_1` keeps the inherited keymap marker as the tail boundary,
/// just like `make-composed-keymap` in `subr.el`: `(keymap CHILD . PARENT)`.
///
/// This distinction matters for `define-key` on a composed prefix map:
/// `access_keymap(..., noinherit=true)` must stop at the parent boundary so the
/// write goes into CHILD rather than mutating an inherited map.
pub fn compose_prefix_with_parent_keymap(
    child: &Value,
    parent: &Value,
    symbol_mode: SymbolMode,
) -> Value {
    if is_list_keymap(parent, symbol_mode) {
        Value::cons(
            KeymapMarker::Keymap.symbol_value(),
            Value::cons(*child, *parent),
        )
    } else {
        compose_prefix_keymaps(child, parent)
    }
}

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
pub enum KeymapElement {
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
    IndirectTail(Value),
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
pub fn resolve_keymap(
    value: Value,
    obarray: Option<&Obarray>,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    match resolve_keymap_or_autoload(value, obarray, symbol_mode)? {
        KeymapResolution::Loaded(map) => Some(map),
        // GNU's spine loops retry `get_keymap` and then test CONSP on the
        // result, so an autoload symbol -- returned as itself -- ends the
        // walk exactly like a non-keymap does.
        KeymapResolution::UnloadedAutoload(_) => None,
    }
}

/// The two shapes GNU `get_keymap (OBJECT, 0, 0)` can answer with, kept apart
/// in the type instead of collapsed into one `Value` the caller re-inspects.
pub enum KeymapResolution {
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
pub fn resolve_keymap_or_autoload(
    value: Value,
    obarray: Option<&Obarray>,
    symbol_mode: SymbolMode,
) -> Option<KeymapResolution> {
    if value.is_nil() {
        return None;
    }
    if is_list_keymap(&value, symbol_mode) {
        return Some(KeymapResolution::Loaded(value));
    }
    let name = value.as_symbol_name(symbol_mode)?;
    let function = obarray?.indirect_function(name)?;
    if is_list_keymap(&function, symbol_mode) {
        return Some(KeymapResolution::Loaded(function));
    }
    is_keymap_autoload_form(&function, symbol_mode)
        .then_some(KeymapResolution::UnloadedAutoload(value))
}

/// GNU `map_keymap_item`: a `t` binding shadows lower-precedence keymaps exactly
/// like an explicit nil binding, so it is reported as nil.
pub fn normalized_binding_value(value: Value, symbol_mode: SymbolMode) -> Value {
    if eq_value_swp(&value, &Value::T, symbol_mode == SymbolMode::Positioned) {
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
pub fn for_each_keymap_element<F>(
    keymap: &Value,
    obarray: Option<&Obarray>,
    mut f: F,
    symbol_mode: SymbolMode,
) where
    F: FnMut(KeymapElement),
{
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
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
        if is_list_keymap(&cursor, symbol_mode) {
            f(KeymapElement::Submap(cursor));
            return;
        }

        let element = cursor.cons_car();
        let rest = cursor.cons_cdr();

        if is_list_keymap(&element, symbol_mode) {
            // A composed submap. GNU `map_keymap` recurses into it and then
            // continues with the rest of the spine, so do not stop here.
            f(KeymapElement::Submap(element));
        } else if crate::emacs_core::chartable::is_char_table(&element) {
            crate::emacs_core::chartable::for_each_non_nil_char_table_run_with_symbols(
                &element,
                |key, value| {
                    f(KeymapElement::Binding {
                        key,
                        value: normalized_binding_value(value, symbol_mode),
                    });
                },
                symbol_mode,
            );
        } else if element.is_vector() {
            // An inline vector indexes bindings by character code. GNU reports
            // every slot, empty ones included.
            if let Some(items) = element.as_vector_data() {
                for (index, binding) in items.iter().enumerate() {
                    f(KeymapElement::Binding {
                        key: Value::fixnum(index as i64),
                        value: normalized_binding_value(*binding, symbol_mode),
                    });
                }
            }
        } else if element.is_cons() {
            f(KeymapElement::Binding {
                key: element.cons_car(),
                value: normalized_binding_value(element.cons_cdr(), symbol_mode),
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
            && let Some(resolved) = resolve_keymap(cursor, obarray, symbol_mode)
            && let Some(spine) = keymap_binding_spine(&resolved, symbol_mode)
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
pub fn list_keymap_for_each_binding<F>(
    keymap: &Value,
    obarray: Option<&Obarray>,
    mut f: F,
    symbol_mode: SymbolMode,
) where
    F: FnMut(Value, Value),
{
    for_each_keymap_element(
        keymap,
        obarray,
        |element| match element {
            KeymapElement::Binding { key, value } => f(key, value),
            // Single-level by contract: descent, and the prefix bookkeeping it
            // needs, belong to the caller.
            KeymapElement::Submap(_)
            | KeymapElement::IndirectTail(_)
            | KeymapElement::Prompt(_) => {}
        },
        symbol_mode,
    );
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
    symbol_mode: SymbolMode,
) where
    F: FnMut(Value, Value),
{
    fn walk<F>(
        keymap: &Value,
        obarray: Option<&Obarray>,
        f: &mut F,
        depth: usize,
        symbol_mode: SymbolMode,
    ) where
        F: FnMut(Value, Value),
    {
        if depth > MAX_KEYMAP_WALK_DEPTH {
            return;
        }
        for_each_keymap_element(
            keymap,
            obarray,
            |element| match element {
                KeymapElement::Binding { key, value } => f(key, value),
                KeymapElement::Submap(submap) => walk(&submap, obarray, f, depth + 1, symbol_mode),
                KeymapElement::IndirectTail(_) | KeymapElement::Prompt(_) => {}
            },
            symbol_mode,
        );
    }

    walk(keymap, obarray, &mut f, 0, symbol_mode);
}

/// Guard against a circular keymap spine (a `setcdr`-built cycle).
pub const MAX_KEYMAP_SPINE_STEPS: usize = 100_000;

/// Guard against mutually-nested keymaps when a consumer recurses into submaps.
pub const MAX_KEYMAP_WALK_DEPTH: usize = 64;

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
pub fn note_keymap_mutation() {
    KEYMAP_MUTATION_EPOCH.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
}

/// Runtime selection installed by GNU's `use-global-map`.
///
/// This is intentionally distinct from the Lisp variable `global-map`.
/// Dynamically binding or assigning that variable does not select a new map;
/// only `use-global-map` changes the runtime selection.
#[derive(Clone, Copy, Debug, Default)]
pub enum GlobalMapSelection {
    #[default]
    Uninitialized,
    Selected(Value),
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SelectedGlobalMap {
    selection: GlobalMapSelection,
}

impl SelectedGlobalMap {
    pub fn from_dump(value: Value) -> Option<Self> {
        if value.is_nil() {
            Some(Self::default())
        } else if is_list_keymap(&value, SymbolMode::Bare) {
            Some(Self {
                selection: GlobalMapSelection::Selected(value),
            })
        } else {
            None
        }
    }

    pub fn value(self) -> Value {
        match self.selection {
            GlobalMapSelection::Uninitialized => Value::NIL,
            GlobalMapSelection::Selected(value) => value,
        }
    }

    pub fn select(&mut self, value: Value, symbol_mode: SymbolMode) {
        assert!(
            is_list_keymap(&value, symbol_mode),
            "SelectedGlobalMap requires a resolved keymap"
        );
        self.selection = GlobalMapSelection::Selected(value);
    }
}

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

// Conversion from keyboard::KeyEvent → keymap::KeyEvent
// ---------------------------------------------------------------------------

/// GNU carries `alt_modifier' as a distinct bit (src/event.h), and the NS
/// modifier policy cooks Option to either `meta' or `alt'
/// (`parse_solitary_modifier', src/keyboard.c:7917), so the frontend's alt
/// modifier must survive this conversion instead of being dropped.
impl From<super::keyboard::KeyEvent> for KeyEvent {
    fn from(ke: super::keyboard::KeyEvent) -> Self {
        use super::keyboard::{Key, NamedKey};
        match ke.key {
            Key::Char(c) => KeyEvent::Char {
                code: c,
                ctrl: ke.modifiers.ctrl,
                meta: ke.modifiers.meta,
                shift: ke.modifiers.shift,
                super_: ke.modifiers.super_,
                hyper: ke.modifiers.hyper,
                alt: ke.modifiers.alt,
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
                            alt: ke.modifiers.alt,
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
                    alt: ke.modifiers.alt,
                }
            }
            // A function key that already carries its symbol — the misc and
            // XF86 blocks, and the reserved identities for native keys with no
            // keysym.  GNU interns the same symbol (`modify_event_symbol`), so
            // `[undo]`, `[XF86Back]` and `[key-16777215]` are ordinary
            // bindable events.
            Key::Function(name) => KeyEvent::Function {
                name: intern(&name),
                ctrl: ke.modifiers.ctrl,
                meta: ke.modifiers.meta,
                shift: ke.modifiers.shift,
                super_: ke.modifiers.super_,
                hyper: ke.modifiers.hyper,
                alt: ke.modifiers.alt,
            },
        }
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum KeymapStateVariable {
    EmulationModeMapAlists,
    MetaPrefixChar,
    MinorModeMapAlist,
    MinorModeOverridingMapAlist,
    OverridingLocalMap,
    OverridingLocalMapMenuFlag,
    OverridingTerminalLocalMap,
}

impl KeymapStateVariable {
    pub fn symbol_id(self) -> SymId {
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

    pub fn global_value(self, vars: VariableView<'_>) -> Option<Value> {
        vars.value(self.symbol_id())
    }

    pub fn buffer_or_global_value(
        self,
        vars: VariableView<'_>,
        buffers: &crate::buffer::BufferManager,
        buffer_id: Option<crate::buffer::BufferId>,
    ) -> Option<Value> {
        dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
            vars,
            &[],
            buffers,
            buffer_id,
            self.symbol_id(),
        )
    }
}

/// GNU's buffer-local-or-global variable read for a keymap state variable.
///
/// Prefers the exact buffer's local slot, then the dynamic/global value. The
/// core active-map resolver above eval also reads through this helper.
pub fn dynamic_buffer_or_global_symbol_value_by_sym_id_in_state(
    vars: VariableView<'_>,
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
    vars.value(sym_id)
}

#[derive(Clone, Copy, Debug)]
pub struct ActiveKeyBindingResolution {
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
pub enum DefaultBindingMode {
    Ignore,
    Accept,
}

impl DefaultBindingMode {
    pub fn accepts_default(self) -> bool {
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

pub fn list_keymap_lookup_one_runtime(
    ctx: &mut Context,
    keymap: &Value,
    event: &Value,
    t_ok: bool,
) -> Result<Value, Flow> {
    let symbol_mode = ctx.symbol_mode();
    let mut resolution: &mut Context = ctx;
    list_keymap_access_with_event(
        keymap,
        KeymapLookupEvent::from_event(*event, symbol_mode),
        false,
        t_ok,
        true,
        &mut resolution,
        symbol_mode,
    )
}

pub fn expand_meta_prefix_char_events_in_obarray(
    vars: VariableView<'_>,
    events: &[Value],
    symbol_mode: SymbolMode,
) -> Option<Vec<Value>> {
    let meta_prefix = KeymapStateVariable::MetaPrefixChar
        .global_value(vars)
        .and_then(|v| v.as_fixnum())?;

    let mut changed = false;
    let mut expanded = Vec::with_capacity(events.len() + 1);
    for event in events {
        match symbol_mode.unwrap(*event).kind() {
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

pub fn resolve_prefix_keymap_binding_in_obarray(
    obarray: &Obarray,
    binding: &Value,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    if is_list_keymap(binding, symbol_mode) {
        return Some(*binding);
    }
    maybe_keymap_in_obarray(obarray, binding, symbol_mode)
}

pub fn lookup_key_in_obarray(
    obarray: &Obarray,
    keymap: &Value,
    events: &[Value],
    t_ok: bool,
    symbol_mode: SymbolMode,
) -> Value {
    if events.is_empty() {
        return *keymap;
    }

    let mut current_map = *keymap;
    for (i, event) in events.iter().enumerate() {
        let binding = if t_ok {
            list_keymap_lookup_one_t_ok_in_obarray(&current_map, event, obarray, symbol_mode)
        } else {
            list_keymap_lookup_one_in_obarray(&current_map, event, obarray, symbol_mode)
        };
        let is_last = i == events.len() - 1;

        if is_last {
            return binding;
        }

        if binding.is_nil() {
            return Value::fixnum((i + 1) as i64);
        }

        if let Some(prefix_keymap) =
            resolve_prefix_keymap_binding_in_obarray(obarray, &binding, symbol_mode)
        {
            current_map = prefix_keymap;
            continue;
        }

        return Value::fixnum((i + 1) as i64);
    }

    Value::NIL
}

pub fn lookup_key_in_keymaps_in_obarray(
    vars: VariableView<'_>,
    keymaps: &[Value],
    events: &[Value],
    t_ok: bool,
    symbol_mode: SymbolMode,
) -> Value {
    let obarray = vars.obarray();
    if events.is_empty() {
        return keymaps.first().copied().unwrap_or(Value::NIL);
    }

    let mut best = Value::NIL;
    for keymap in keymaps {
        let direct = lookup_key_in_obarray(obarray, keymap, events, t_ok, symbol_mode);
        if !direct.is_nil() && !direct.is_fixnum() {
            return direct;
        }

        if let Some(expanded) = expand_meta_prefix_char_events_in_obarray(vars, events, symbol_mode)
        {
            let expanded_result =
                lookup_key_in_obarray(obarray, keymap, &expanded, t_ok, symbol_mode);
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

pub fn lookup_key_in_obarray_runtime(
    ctx: &mut Context,
    keymap: Value,
    events: &[Value],
    t_ok: bool,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    if events.is_empty() {
        return Ok(keymap);
    }

    let mut current_map = keymap;
    for (i, event) in events.iter().enumerate() {
        // GNU access_keymap resolves `get_keyelt (def, autoload)` inside the
        // walk, so a menu-item's filter result is what the prefix decision
        // sees, and a filter that returns a prefix keymap still composes with
        // the parent chain's prefix for the same event.  The runtime step does
        // both; the old order (compose raw, resolve after) dropped the parent
        // branch every time a package rebinds a prefix through a filter.
        let is_last = i == events.len() - 1;
        let binding = list_keymap_lookup_one_runtime(ctx, &current_map, event, t_ok)?;

        if is_last {
            return Ok(binding);
        }

        if binding.is_nil() {
            return Ok(Value::fixnum((i + 1) as i64));
        }

        if let Some(prefix_keymap) =
            resolve_prefix_keymap_binding_in_obarray(&ctx.obarray, &binding, symbol_mode)
        {
            current_map = prefix_keymap;
            continue;
        }

        return Ok(Value::fixnum((i + 1) as i64));
    }

    Ok(Value::NIL)
}

pub fn lookup_key_in_keymaps_in_obarray_runtime(
    ctx: &mut Context,
    keymaps: &[Value],
    events: &[Value],
    t_ok: bool,
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    if events.is_empty() {
        return Ok(keymaps.first().copied().unwrap_or(Value::NIL));
    }

    let mut best = Value::NIL;
    for keymap in keymaps {
        let direct = lookup_key_in_obarray_runtime(ctx, *keymap, events, t_ok)?;
        if !direct.is_nil() && !direct.is_fixnum() {
            return Ok(direct);
        }

        if let Some(expanded) =
            expand_meta_prefix_char_events_in_obarray(ctx.variables(), events, symbol_mode)
        {
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
pub fn list_keymap_lookup_seq(keymap: &Value, events: &[Value], symbol_mode: SymbolMode) -> Value {
    list_keymap_lookup_seq_impl(keymap, events, true, symbol_mode)
}

fn list_keymap_lookup_seq_impl(
    keymap: &Value,
    events: &[Value],
    resolve_keyelt: bool,
    symbol_mode: SymbolMode,
) -> Value {
    if events.is_empty() {
        return *keymap;
    }

    if let Some(binding) =
        list_keymap_lookup_composed_seq(keymap, events, resolve_keyelt, symbol_mode)
    {
        return binding;
    }

    let mut current_map = *keymap;
    for (i, event) in events.iter().enumerate() {
        let binding = if resolve_keyelt {
            list_keymap_lookup_one(&current_map, event, symbol_mode)
        } else {
            list_keymap_lookup_one_unresolved(&current_map, event, symbol_mode)
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
        if is_list_keymap(&binding, symbol_mode) {
            current_map = binding;
        } else {
            // Check if it's a symbol whose function cell is a keymap
            if let Some(sym_name) = binding.as_symbol_name(symbol_mode) {
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
    symbol_mode: SymbolMode,
) -> Option<Value> {
    let Some(mut cursor) = keymap_binding_spine(keymap, symbol_mode) else {
        return None;
    };
    let mut saw_embedded_keymap = false;
    let mut parent = Value::NIL;

    while cursor.is_cons() {
        if is_list_keymap(&cursor, symbol_mode) {
            parent = cursor;
            break;
        }

        let entry_car = cursor.cons_car();
        let entry_cdr = cursor.cons_cdr();
        if is_list_keymap(&entry_car, symbol_mode) {
            saw_embedded_keymap = true;
            let binding =
                list_keymap_lookup_seq_impl(&entry_car, events, resolve_keyelt, symbol_mode);
            if !binding.is_nil() && !binding.is_fixnum() {
                return Some(binding);
            }
        } else if saw_embedded_keymap {
            break;
        } else {
            return None;
        }

        if is_list_keymap(&entry_cdr, symbol_mode) {
            parent = entry_cdr;
            break;
        }
        cursor = entry_cdr;
    }

    if !saw_embedded_keymap {
        if !parent.is_nil() {
            let binding = list_keymap_lookup_seq_impl(&parent, events, resolve_keyelt, symbol_mode);
            if !binding.is_nil() && !binding.is_fixnum() {
                return Some(binding);
            }
        }
        return None;
    }
    if !parent.is_nil() {
        let binding = list_keymap_lookup_seq_impl(&parent, events, resolve_keyelt, symbol_mode);
        if !binding.is_nil() && !binding.is_fixnum() {
            return Some(binding);
        }
    }
    None
}

/// Create a sparse list keymap: `(keymap)` — a single-element list.
pub fn make_sparse_list_keymap() -> Value {
    Value::list(vec![KeymapMarker::Keymap.symbol_value()])
}

pub fn list_keymap_define(keymap: Value, event: Value, def: Value, symbol_mode: SymbolMode) {
    note_keymap_mutation();
    store_in_keymap(keymap, event, def, false, symbol_mode);
}

/// Remove a binding from a keymap, matching GNU `define-key` with REMOVE arg.
pub fn list_keymap_remove(keymap: Value, event: Value, symbol_mode: SymbolMode) {
    // GNU `store_in_keymap` flushes the reverse keymap cache on every store,
    // including REMOVE (src/keymap.c:733-736). Filters can run arbitrary Lisp
    // (and rebuild that cache) between the caller's epoch bump and this
    // store, so the removal must invalidate it just like a definition does.
    note_keymap_mutation();
    store_in_keymap(keymap, event, Value::NIL, true, symbol_mode);
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

fn keymap_storage_event(event: Value, symbol_mode: SymbolMode) -> Value {
    if event.is_cons() && keymap_character_range(&event).is_none() {
        reorder_event_symbol_modifiers(symbol_mode.unwrap(event.cons_car()), SymbolMode::Bare)
    } else {
        reorder_event_symbol_modifiers(symbol_mode.unwrap(event), SymbolMode::Bare)
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

fn keymap_delq_after(insertion_point: Value, elt: Value, symbol_mode: SymbolMode) {
    let mut previous = insertion_point;
    let mut cursor = insertion_point.cons_cdr();
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        let next = cursor.cons_cdr();
        if eq_value_swp(&entry, &elt, symbol_mode == SymbolMode::Positioned) {
            previous.set_cdr(next);
            return;
        }
        previous = cursor;
        cursor = next;
    }
}

/// Core store/remove implementation matching GNU `store_in_keymap`.
fn store_in_keymap(keymap: Value, event: Value, def: Value, remove: bool, symbol_mode: SymbolMode) {
    if !keymap.is_cons() {
        return;
    };
    let root_car = keymap.cons_car();
    let root_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(root_car, symbol_mode) {
        return;
    }
    let event = keymap_storage_event(event, symbol_mode);

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
        if is_list_keymap(&cursor, symbol_mode) {
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
                        let _ = builtin_set_char_table_range_with_style(
                            vec![entry_car, event, store_val],
                            None,
                            symbol_mode,
                        );
                        return;
                    }
                }
            } else if event.is_cons() && keymap_character_range(&event).is_some() {
                let store_val = keymap_char_table_store_value(def, remove);
                let _ = builtin_set_char_table_range_with_style(
                    vec![entry_car, event, store_val],
                    None,
                    symbol_mode,
                );
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
        if entry_car.is_cons() && is_list_keymap(&entry_car, symbol_mode) {
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
                        keymap_delq_after(insertion_point, entry_car, symbol_mode);
                    } else {
                        entry_car.set_cdr(def);
                    }
                    if from == to {
                        return;
                    }
                }
            } else if eq_value_swp(&binding_car, &event, symbol_mode == SymbolMode::Positioned) {
                if remove {
                    // GNU uses `Fdelq (elt, insertion_point)`: remove exactly
                    // this binding cons while preserving earlier alist entries
                    // that insertion_point intentionally skips over.
                    keymap_delq_after(insertion_point, entry_car, symbol_mode);
                } else {
                    // Update in-place: set the cdr of the binding cons.
                    entry_car.set_cdr(def);
                }
                return;
            }
        }

        // Check for 'keymap symbol in spine (start of inherited keymap)
        // GNU keymap.c:871-876
        if KeymapMarker::Keymap.is_value(entry_car, symbol_mode) {
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
            let _ = builtin_set_char_table_range_with_style(
                vec![char_table, event, store_val],
                None,
                symbol_mode,
            );
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

/// Set the parent keymap: walk to the last alist cons cell, set its CDR.
pub fn list_keymap_set_parent(keymap: Value, parent: Value, symbol_mode: SymbolMode) {
    note_keymap_mutation();
    if !keymap.is_cons() {
        return;
    };
    let root_car = keymap.cons_car();
    let root_cdr = keymap.cons_cdr();
    if !KeymapMarker::Keymap.is_value(root_car, symbol_mode) {
        return;
    }

    // Find the last cons cell in the keymap list
    let mut prev_cell_value = keymap;
    let mut cursor = root_cdr;
    loop {
        if is_list_keymap(&cursor, symbol_mode) || cursor.is_nil() {
            prev_cell_value.set_cdr(parent);
            return;
        }
        match cursor.kind() {
            ValueKind::Cons => {
                let entry_cdr = cursor.cons_cdr();
                // If cdr is a keymap (existing parent) or nil, we replace it
                if is_list_keymap(&entry_cdr, symbol_mode) || entry_cdr.is_nil() {
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
