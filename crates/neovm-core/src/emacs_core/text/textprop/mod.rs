//! Text property and overlay builtins for the Elisp interpreter.
//!
//! Bridges the buffer's `TextPropertyTable` and `OverlayList` to Elisp
//! functions like `put-text-property`, `make-overlay`, etc.

use super::error::{EvalResult, Flow, signal};
use super::intern::{NIL_SYM_ID, T_SYM_ID};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_max_args, expect_min_args};
// storage imports removed — now using emacs_char directly
use super::symbol::Obarray;
use super::value::*;
use crate::buffer::text_props::{PropertyPlistApplication, TextPropertyTable};
use crate::buffer::{
    Buffer, BufferId, BufferManager, CharLen, CharPos0, CharRange, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
};
use crate::emacs_core::SymId;
use crate::window::{FrameManager, WindowId};
use strum::IntoStaticStr;

/// Lisp variables that control native text-property resolution.
///
/// GNU keeps these in predeclared `V...`/`Q...` globals.  A closed Rust enum
/// gives the corresponding Neomacs variables stable identities too: callers
/// cannot accidentally put a string-based lookup back on a per-character or
/// per-insertion path, and adding a modeled variable requires an explicit
/// cache slot in the exhaustive match below.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum TextPropertyControlVariable {
    CharPropertyAliasAlist,
    DefaultTextProperties,
    TextPropertyDefaultNonsticky,
}

impl TextPropertyControlVariable {
    pub(crate) fn name(self) -> &'static str {
        self.into()
    }

    pub(crate) fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static CHAR_PROPERTY_ALIAS_ALIST: OnceLock<SymId> = OnceLock::new();
        static DEFAULT_TEXT_PROPERTIES: OnceLock<SymId> = OnceLock::new();
        static TEXT_PROPERTY_DEFAULT_NONSTICKY: OnceLock<SymId> = OnceLock::new();

        match self {
            Self::CharPropertyAliasAlist => {
                *CHAR_PROPERTY_ALIAS_ALIST.get_or_init(|| super::intern::intern(self.into()))
            }
            Self::DefaultTextProperties => {
                *DEFAULT_TEXT_PROPERTIES.get_or_init(|| super::intern::intern(self.into()))
            }
            Self::TextPropertyDefaultNonsticky => {
                *TEXT_PROPERTY_DEFAULT_NONSTICKY.get_or_init(|| super::intern::intern(self.into()))
            }
        }
    }

    /// Read this variable for one explicit buffer, falling back to its global
    /// default exactly as GNU's buffer-local value machinery does.
    pub(crate) fn value_for_buffer(self, obarray: &Obarray, buf: &Buffer) -> Option<Value> {
        let symbol_id = self.symbol_id();
        let localized = obarray.is_localized(symbol_id);
        if let Some(binding) = buf.get_buffer_local_binding_by_sym_id_gated(symbol_id, localized) {
            return binding.as_value();
        }
        obarray.symbol_value_id_copied(symbol_id)
    }
}

/// The two properties that override default text-property stickiness.
///
/// GNU's interval code compares `Qfront_sticky` and `Qrear_nonsticky`
/// directly.  Carrying the distinction as an enum prevents arbitrary strings
/// from entering the insertion merge and makes every property-to-symbol
/// mapping exhaustive.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub(crate) enum StickinessProperty {
    FrontSticky,
    RearNonsticky,
}

impl StickinessProperty {
    pub(crate) fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static FRONT_STICKY: OnceLock<SymId> = OnceLock::new();
        static REAR_NONSTICKY: OnceLock<SymId> = OnceLock::new();

        match self {
            Self::FrontSticky => *FRONT_STICKY.get_or_init(|| super::intern::intern(self.into())),
            Self::RearNonsticky => {
                *REAR_NONSTICKY.get_or_init(|| super::intern::intern(self.into()))
            }
        }
    }

    pub(crate) fn value(self) -> Value {
        Value::from_sym_id(self.symbol_id())
    }

    pub(crate) fn is_value(self, value: Value) -> bool {
        value.as_symbol_id() == Some(self.symbol_id())
    }
}

pub(crate) fn init_textprop_vars(
    obarray: &mut crate::emacs_core::symbol::Obarray,
    _custom: &mut crate::emacs_core::custom::CustomManager,
) {
    let default_properties = TextPropertyControlVariable::DefaultTextProperties;
    obarray.set_symbol_value(default_properties.name(), Value::NIL);
    obarray.make_special(default_properties.name());

    let alias_alist = TextPropertyControlVariable::CharPropertyAliasAlist;
    obarray.set_symbol_value(alias_alist.name(), Value::NIL);
    obarray.make_special(alias_alist.name());

    obarray.set_symbol_value("inhibit-point-motion-hooks", Value::T);
    obarray.make_special("inhibit-point-motion-hooks");

    let default_nonsticky = TextPropertyControlVariable::TextPropertyDefaultNonsticky;
    let default_nonsticky_name = default_nonsticky.name();
    obarray.set_symbol_value(
        default_nonsticky_name,
        default_text_property_nonsticky_alist(),
    );
    obarray.make_special(default_nonsticky_name);
    // Mirrors GNU `Fmake_variable_buffer_local` (`data.c:2142-2207`):
    // flip the redirect tag to LOCALIZED, allocate a BLV, set
    // local_if_set = 1. Replaces the legacy `make_buffer_local`
    // helper which was destructive (set the redirect back to
    // PLAINVAL and orphaned the BLV).
    {
        let id = default_nonsticky.symbol_id();
        let default = obarray
            .find_symbol_value(id)
            .unwrap_or(crate::emacs_core::value::Value::NIL);
        obarray.make_symbol_localized(id, default);
        obarray.set_blv_local_if_set(id, true);
    }
}

// ---------------------------------------------------------------------------
// Helpers (local to this module)
// ---------------------------------------------------------------------------

#[inline]
fn buffer_char_to_emacs_byte_pos(buf: &Buffer, char_pos: CharPos0) -> EmacsBytePos {
    buf.char_pos_to_emacs_byte_pos_clamped(char_pos)
}

#[inline]
fn buffer_char_to_byte_pos(buf: &Buffer, char_pos: CharPos0) -> usize {
    buffer_char_to_emacs_byte_pos(buf, char_pos).get()
}

#[inline]
fn buffer_end_emacs_byte_pos(buf: &Buffer) -> EmacsBytePos {
    buf.total_emacs_byte_end_pos()
}

#[inline]
fn string_char_pos(pos: usize) -> CharPos0 {
    CharPos0::new(pos)
}

#[inline]
fn string_char_len(len: usize) -> CharLen {
    CharLen::new(len)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => super::marker::marker_position_as_int(value),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_int_eval(eval: &super::eval::Context, value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => {
            super::marker::marker_position_as_int_eval(eval, value)
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integerp"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_integer_or_marker(value: &Value) -> Result<i64, Flow> {
    match value.kind() {
        ValueKind::Fixnum(n) => Ok(n),
        _ if super::marker::is_marker(value) => super::marker::marker_position_as_int(value),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *value],
        )),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn expect_integer_or_marker_eval(eval: &super::eval::Context, value: &Value) -> Result<i64, Flow> {
    super::position::fix_position_eval(eval, value)
}

pub(crate) fn expect_integer_or_marker_in_buffers(
    buffers: &BufferManager,
    value: &Value,
) -> Result<i64, Flow> {
    super::position::fix_position_with_buffers(buffers, value)
}

/// Text property keys are Lisp objects and are compared by identity, matching
/// GNU Emacs interval plists.
pub(crate) fn expect_property_key(value: &Value) -> Result<Value, Flow> {
    Ok(*value)
}

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    obarray.set_symbol_value(
        TextPropertyControlVariable::DefaultTextProperties.name(),
        Value::NIL,
    );
    obarray.set_symbol_value(
        TextPropertyControlVariable::CharPropertyAliasAlist.name(),
        Value::NIL,
    );
    obarray.set_symbol_value("inhibit-point-motion-hooks", Value::T);
    obarray.set_symbol_value(
        TextPropertyControlVariable::TextPropertyDefaultNonsticky.name(),
        default_text_property_nonsticky_alist(),
    );
}

/// GNU's effective default for `text-property-default-nonsticky', which is
/// assembled in TWO C files and is therefore not what either one alone says:
/// `syms_of_textprop' seeds the alist with syntax-table and display
/// (src/textprop.c:2426-2429), and `syms_of_composite' conses `composition'
/// onto the FRONT of whatever is already there (src/composite.c:2212-2213).
///
/// This lives in one function because the value used to be spelled out at four
/// separate installation sites, so correcting one of them was silently undone
/// by the next one to run.
pub(crate) fn default_text_property_nonsticky_alist() -> Value {
    Value::list(vec![
        Value::cons(Value::symbol("composition"), Value::T),
        Value::cons(Value::symbol("syntax-table"), Value::T),
        Value::cons(Value::symbol("display"), Value::T),
    ])
}

fn current_textprop_variable_value(
    obarray: &Obarray,
    buffers: &BufferManager,
    variable: TextPropertyControlVariable,
) -> Option<Value> {
    // Text-property control vars (char-property-alias-alist, default-text-
    // properties, inhibit-*, ...) are almost always plain globals. A global
    // (non-Localized) symbol can never be in a buffer's local_var_alist, so
    // skip the per-buffer scan for it -- this runs during redisplay and was
    // ~3% of the layout profile. See `Obarray::is_localized`. The caller
    // passes a closed typed identity so the hot path cannot re-intern a name.
    let sym_id = variable.symbol_id();
    // Localized-first: a global (non-Localized) symbol can never have a
    // buffer-local binding, so skip the current-buffer probe (a map lookup +
    // a call) entirely — these reads run several times per char-property
    // lookup.
    if obarray.is_localized(sym_id)
        && let Some(buf) = buffers.current_buffer()
        && let Some(binding) = buf.get_buffer_local_binding_by_sym_id_gated(sym_id, true)
    {
        return binding.as_value();
    }
    obarray.symbol_value_id_copied(sym_id)
}

fn plist_get_value(plist: Value, prop: Value) -> Option<Value> {
    let mut tail = plist;
    loop {
        if !tail.is_cons() {
            return None;
        };
        let pair_car = tail.cons_car();
        let pair_cdr = tail.cons_cdr();
        if !pair_cdr.is_cons() {
            return None;
        };
        if eq_value(&pair_car, &prop) {
            return Some(pair_cdr.cons_car());
        }
        tail = pair_cdr.cons_cdr();
    }
}

fn plist_slice_get_value(plist: &[(Value, Value)], prop: Value) -> Option<Value> {
    plist
        .iter()
        .find_map(|(key, value)| eq_value(key, &prop).then_some(*value))
}

fn assq_rest(list: Value, prop: Value) -> Option<Value> {
    let mut cursor = list;
    while cursor.is_cons() {
        let pair_car = cursor.cons_car();
        let pair_cdr = cursor.cons_cdr();
        if pair_car.is_cons() {
            let entry_car = pair_car.cons_car();
            let entry_cdr = pair_car.cons_cdr();
            if eq_value(&entry_car, &prop) {
                return Some(entry_cdr);
            }
        }
        cursor = pair_cdr;
    }
    None
}

fn symbol_id_for_property_lookup(value: Value) -> Option<SymId> {
    match value.kind() {
        ValueKind::Nil => Some(NIL_SYM_ID),
        ValueKind::T => Some(T_SYM_ID),
        ValueKind::Symbol(id) => Some(id),
        _ => None,
    }
}

/// What one property list says directly about a property: its own value, and
/// the `category` symbol that may supply one on its behalf.
///
/// GNU's `lookup_char_property` reads both in a SINGLE pass over the plist
/// (`src/intervals.c`), and so must we: this runs on the syntax scanner's
/// run-refill path, where a second pass over a font-lock plist to ask "is there
/// a category here?" measured ~1.6% of a syntax-scan profile.
#[derive(Clone, Copy, Default)]
pub struct DirectCharProperties {
    /// The property's own entry. `Some(nil)` means present with value nil,
    /// which GNU honours over every fallback.
    pub value: Option<Value>,
    /// The `category` entry's value, whatever its type.
    pub category: Option<Value>,
}

impl DirectCharProperties {
    /// Read both entries through a caller's property getter, for sources that
    /// are not a walkable plist (overlays, layout buffer views). Two probes,
    /// where [`Self::from_plist`] needs one pass; none of these callers is on
    /// the per-character scanning path.
    pub fn from_getter<F>(mut get: F, prop: Value) -> Self
    where
        F: FnMut(Value) -> Option<Value>,
    {
        let value = get(prop);
        let category = value
            .is_none()
            .then(|| get(Value::from_sym_id(category_sym_id())))
            .flatten();
        Self { value, category }
    }

    /// Scan a Lisp plist once for `prop` and for `category`, as GNU's
    /// `lookup_char_property` loop does.
    #[inline]
    pub fn from_plist(plist: Value, prop: Value) -> Self {
        let category_key = Value::from_sym_id(category_sym_id());
        let mut found = Self::default();
        let mut tail = plist;
        while tail.is_cons() {
            let key = tail.cons_car();
            let rest = tail.cons_cdr();
            if !rest.is_cons() {
                break;
            }
            if eq_value(&key, &prop) {
                // GNU returns here: the direct entry wins outright, and a
                // later `category` cannot be consulted.
                found.value = Some(rest.cons_car());
                return found;
            }
            if eq_value(&key, &category_key) {
                found.category = Some(rest.cons_car());
            }
            tail = rest.cons_cdr();
        }
        found
    }
}

/// Resolve one GNU effective character property from what its property list
/// says directly.
///
/// This is the shared implementation of `lookup_char_property' precedence
/// from GNU `src/intervals.c`: a directly present canonical property wins even
/// when its value is nil; otherwise a non-nil category property wins; then
/// non-nil aliases are considered in order; finally the caller-supplied
/// optional `default-text-properties' value is used for text properties.
/// Overlay callers supply no default. Callers own the environment adapters
/// because evaluator lookups and immutable layout snapshots obtain
/// category/default values differently, and because only a caller holding a
/// real plist can read `direct` in one pass.
///
/// `alias_get` is consulted only for a non-empty `aliases` list, which
/// `char-property-alias-alist` leaves empty in every default configuration.
#[inline]
pub fn resolve_effective_char_property<C, A, G>(
    direct: DirectCharProperties,
    mut category_get: C,
    prop: Value,
    aliases: A,
    mut alias_get: G,
    default: Option<Value>,
) -> Option<Value>
where
    C: FnMut(Value, Value) -> Option<Value>,
    A: IntoIterator<Item = Value>,
    G: FnMut(Value) -> Option<Value>,
{
    if let Some(value) = direct.value {
        return Some(value);
    }

    if let Some(category) = direct.category
        && let Some(value) = category_get(category, prop)
        && !value.is_nil()
    {
        return Some(value);
    }

    for alias in aliases {
        if let Some(value) = alias_get(alias)
            && !value.is_nil()
        {
            return Some(value);
        }
    }

    default
}

/// Interned `category`, resolved once: [`resolve_effective_char_property`] runs
/// per property lookup, and re-interning the name there costs a registry lock
/// and a hash on the syntax scanner's run-refill path.
fn category_sym_id() -> SymId {
    static ID: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *ID.get_or_init(|| super::intern::intern("category"))
}

/// GNU `textget` (`src/intervals.c` `lookup_char_property` with
/// `textprop = true`) for ONE property, snapshotted for the duration of one
/// scan.
///
/// The syntax scanner cannot reach the property builtins per character: it runs
/// under a plain `&Buffer` and reads its property once per character scanned.
/// Without a shared resolver it read the property raw and disagreed with
/// `get-char-property` about the same character -- notably for the CC Mode
/// `category` indirection. This carries the same three fallbacks `textget`
/// applies, so scanner and property API resolve a character identically by
/// construction.
///
/// `char-property-alias-alist` and `default-text-properties` are read ONCE, at
/// snapshot time, rather than per lookup as in GNU. No Lisp runs during a scan
/// (`syntax-propertize` has already finished by then), which is the same
/// immutability invariant the scanner's property-run cache and its ASCII syntax
/// memo already depend on. Anything that does run Lisp mid-scan -- the
/// `find-word-boundary-function-table` callback -- takes a fresh snapshot for
/// each probe, exactly as it already rebuilds the run cache.
#[derive(Clone, Copy)]
pub(crate) struct CharPropertyResolver<'a> {
    obarray: &'a Obarray,
    prop: Value,
    /// `(cdr (assq PROP char-property-alias-alist))` at snapshot time.
    aliases: Value,
    /// `(plist-get default-text-properties PROP)` at snapshot time.
    default: Option<Value>,
}

impl<'a> CharPropertyResolver<'a> {
    pub(crate) fn snapshot(obarray: &'a Obarray, buffers: &BufferManager, prop: Value) -> Self {
        let aliases = current_textprop_variable_value(
            obarray,
            buffers,
            TextPropertyControlVariable::CharPropertyAliasAlist,
        )
        .and_then(|value| assq_rest(value, prop))
        .unwrap_or(Value::NIL);
        let default = current_textprop_variable_value(
            obarray,
            buffers,
            TextPropertyControlVariable::DefaultTextProperties,
        )
        .filter(|value| value.is_cons())
        .and_then(|defaults| plist_get_value(defaults, prop));
        Self {
            obarray,
            prop,
            aliases,
            default,
        }
    }

    /// Whether the snapshot carries `char-property-alias-alist` aliases for
    /// the property (rare) — the fast presence-bit coalescing is only sound
    /// without them.
    pub(crate) fn has_aliases(&self) -> bool {
        self.aliases.is_cons()
    }

    /// Whether run coalescing is sound for this snapshot: no aliases (they
    /// widen the watched-key set) and no `default-text-properties` fallback
    /// (a key-free INTERVAL resolves to the default while a GAP resolves to
    /// nothing, so merging across that edge would blur two values).
    pub(crate) fn supports_presence_coalescing(&self) -> bool {
        !self.aliases.is_cons() && self.default.is_none()
    }

    /// Every plist key whose value can influence [`Self::resolve_interval_plist`]:
    /// the property itself, `category` (a category symbol's plist can supply
    /// it), and each snapshot alias. Two intervals whose plists agree (`eq`)
    /// on all of these resolve identically — the run-coalescing scanners use
    /// this with `next_watched_property_change` to skip boundaries that only
    /// split other properties (font-lock `face` churn).
    pub(crate) fn watched_keys(&self) -> smallvec::SmallVec<[Value; 4]> {
        // Interned-once `category` id: this runs per coalescing scan, and
        // `Value::symbol(&str)` pays string compares plus a thread-local
        // intern probe per call.
        let mut keys = smallvec::SmallVec::new();
        keys.push(self.prop);
        keys.push(Value::symbol(category_sym_id()));
        let mut alias = self.aliases;
        while alias.is_cons() {
            keys.push(alias.cons_car());
            alias = alias.cons_cdr();
        }
        keys
    }

    /// Resolve the property from one interval's plist.
    ///
    /// Callers pass the plist of the interval covering the position, never a
    /// synthesized one: GNU's `update_syntax_table` returns early when
    /// `interval_of` finds no interval, so a position outside every interval
    /// gets no property at all -- not even the `default-text-properties`
    /// fallback.
    #[inline]
    pub(crate) fn resolve_interval_plist(&self, plist: Value) -> Option<Value> {
        let direct = DirectCharProperties::from_plist(plist, self.prop);
        if let Some(value) = direct.value {
            return Some(value);
        }
        // The overwhelmingly common shape: no direct entry, no category, and
        // both control variables nil. Kept inline and ahead of the fallback
        // machinery because the syntax scanner reaches here once per property
        // run, and once per character on the byte-addressed scanners.
        if direct.category.is_none() && !self.aliases.is_cons() && self.default.is_none() {
            return None;
        }
        self.resolve_fallbacks(direct, plist)
    }

    /// The `category` / alias / `default-text-properties` tail, outlined so the
    /// common answer above stays inlinable into the scanners.
    #[inline(never)]
    fn resolve_fallbacks(&self, direct: DirectCharProperties, plist: Value) -> Option<Value> {
        let mut aliases = self.aliases;
        let alias_iter = std::iter::from_fn(move || {
            if !aliases.is_cons() {
                return None;
            }
            let alias = aliases.cons_car();
            aliases = aliases.cons_cdr();
            Some(alias)
        });
        resolve_effective_char_property(
            direct,
            |category, property| {
                let category_id = symbol_id_for_property_lookup(category)?;
                let property_id = symbol_id_for_property_lookup(property)?;
                self.obarray.get_property_id(category_id, property_id)
            },
            self.prop,
            alias_iter,
            |name| plist_get_value(plist, name),
            self.default,
        )
    }
}

fn lookup_char_property_from_direct<F>(
    obarray: &Obarray,
    buffers: &BufferManager,
    mut direct_get: F,
    prop: Value,
    textprop: bool,
) -> Value
where
    F: FnMut(Value) -> Option<Value>,
{
    let mut aliases = current_textprop_variable_value(
        obarray,
        buffers,
        TextPropertyControlVariable::CharPropertyAliasAlist,
    )
    .and_then(|value| assq_rest(value, prop))
    .unwrap_or(Value::NIL);
    let alias_iter = std::iter::from_fn(move || {
        if !aliases.is_cons() {
            return None;
        }
        let alias = aliases.cons_car();
        aliases = aliases.cons_cdr();
        Some(alias)
    });
    let default = textprop
        .then(|| {
            current_textprop_variable_value(
                obarray,
                buffers,
                TextPropertyControlVariable::DefaultTextProperties,
            )
            .filter(|value| value.is_cons())
            .and_then(|defaults| plist_get_value(defaults, prop))
        })
        .flatten();

    let direct = DirectCharProperties::from_getter(&mut direct_get, prop);
    resolve_effective_char_property(
        direct,
        |category, property| {
            let category_id = symbol_id_for_property_lookup(category)?;
            let property_id = symbol_id_for_property_lookup(property)?;
            obarray.get_property_id(category_id, property_id)
        },
        prop,
        alias_iter,
        &mut direct_get,
        default,
    )
    .unwrap_or(Value::NIL)
}

/// Resolve a char/text property from an interval's plist (in slice form),
/// resolving through a `category` symbol just like GNU `textget`
/// (`lookup_char_property` with `textprop = true`).  Used when collecting
/// `modification-hooks` from text-property intervals so a `category' interval
/// contributes the category symbol's `modification-hooks' property.
pub(crate) fn lookup_text_property_from_plist_slice(
    obarray: &Obarray,
    buffers: &BufferManager,
    plist: &[(Value, Value)],
    prop: Value,
) -> Value {
    lookup_char_property_from_direct(
        obarray,
        buffers,
        |name| plist_slice_get_value(plist, name),
        prop,
        true,
    )
}

fn lookup_string_text_property(
    obarray: &Obarray,
    buffers: &BufferManager,
    table: &TextPropertyTable,
    char_pos: usize,
    prop: Value,
) -> Value {
    lookup_char_property_from_direct(
        obarray,
        buffers,
        |name| table.get_property_at_char_pos(string_char_pos(char_pos), name),
        prop,
        true,
    )
}

fn lookup_buffer_text_property_at_char_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::buffer::Buffer,
    char_pos: CharPos0,
    prop: Value,
) -> Value {
    lookup_char_property_from_direct(
        obarray,
        buffers,
        |name| buf.text_props_get_property_at_char_pos(char_pos, name),
        prop,
        true,
    )
}

pub(crate) fn lookup_buffer_text_property(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::buffer::Buffer,
    byte_pos: usize,
    prop: Value,
) -> Value {
    lookup_buffer_text_property_at_emacs_byte_pos(
        obarray,
        buffers,
        buf,
        EmacsBytePos::new(byte_pos),
        prop,
    )
}

pub(crate) fn lookup_buffer_text_property_at_emacs_byte_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::buffer::Buffer,
    byte_pos: EmacsBytePos,
    prop: Value,
) -> Value {
    // Convert byte->char only if some queried name (prop or an alias) may
    // actually be present: an absent name answers from the presence set
    // without paying the anchored conversion.
    let char_pos = std::cell::OnceCell::new();
    lookup_char_property_from_direct(
        obarray,
        buffers,
        |name| {
            if buf.text_props_property_name_presence(name)
                == crate::buffer::text_props::PropertyNamePresence::DefinitelyAbsent
            {
                return None;
            }
            let pos = *char_pos.get_or_init(|| buf.emacs_byte_pos_to_char_pos_clamped(byte_pos));
            buf.text_props_get_property_at_char_pos(pos, name)
        },
        prop,
        true,
    )
}

pub(crate) fn lookup_overlay_property(
    obarray: &Obarray,
    buffers: &BufferManager,
    overlay_val: Value,
    prop: Value,
) -> Value {
    let plist = overlay_val
        .as_overlay_data()
        .map_or(Value::NIL, |d| d.plist);
    lookup_char_property_from_direct(
        obarray,
        buffers,
        |name| plist_get_value(plist, name),
        prop,
        false,
    )
}

/// Convert a 1-based Elisp char position to a 0-based byte position.
///
/// This is only valid after GNU-style range validation.  Text-property
/// builtins must not clamp positions: GNU `validate_interval_range` signals
/// `args-out-of-range` for invalid positions.
fn elisp_pos_to_byte(buf: &crate::buffer::buffer::Buffer, pos: LispCharPos1) -> EmacsBytePos {
    debug_assert!(pos.as_i64() >= 1);
    buffer_char_to_emacs_byte_pos(buf, pos.to_char_pos())
}

fn validated_lisp_char_pos(pos: i64) -> LispCharPos1 {
    debug_assert!(pos >= 1);
    LispCharPos1::from_one_based_usize(usize::try_from(pos).expect("Lisp position fits usize"))
}

pub(crate) fn elisp_pos_to_byte_clipped_full(
    buf: &crate::buffer::buffer::Buffer,
    pos: LispCharPos1,
) -> EmacsBytePos {
    let max = buf.z_lisp_char_pos().as_i64();
    let clipped = validated_lisp_char_pos(pos.as_i64().clamp(1, max));
    elisp_pos_to_byte(buf, clipped)
}

pub(crate) fn elisp_range_to_byte_clipped_full(
    buf: &crate::buffer::buffer::Buffer,
    mut beg: i64,
    mut end: i64,
) -> EmacsByteRange {
    if beg > end {
        std::mem::swap(&mut beg, &mut end);
    }
    let max = buf.z_lisp_char_pos().as_i64();
    let clipped_beg = beg.clamp(1, max);
    let clipped_end = end.clamp(clipped_beg, max);
    EmacsByteRange::new(
        elisp_pos_to_byte(buf, validated_lisp_char_pos(clipped_beg)),
        elisp_pos_to_byte(buf, validated_lisp_char_pos(clipped_end)),
    )
}

fn args_out_of_range_point(pos: i64) -> Flow {
    signal(LispCondition::ArgsOutOfRange, vec![Value::fixnum(pos)])
}

fn args_out_of_range_point_pair(pos0: Value) -> Flow {
    signal(LispCondition::ArgsOutOfRange, vec![pos0, pos0])
}

fn args_out_of_range_range(begin0: Value, end0: Value) -> Flow {
    signal(LispCondition::ArgsOutOfRange, vec![begin0, end0])
}

pub(crate) fn validate_string_point_raw(
    s: &crate::heap_types::LispString,
    pos: i64,
    pos0: Value,
) -> Result<usize, Flow> {
    validate_string_char_pos_raw(s, pos, pos0).map(CharPos0::get)
}

pub(crate) fn validate_string_char_pos_raw(
    s: &crate::heap_types::LispString,
    pos: i64,
    pos0: Value,
) -> Result<CharPos0, Flow> {
    let len = s.schars() as i64;
    if !(0 <= pos && pos <= len) {
        return Err(args_out_of_range_point_pair(pos0));
    }
    Ok(string_char_pos(pos as usize))
}

fn validate_string_range(
    s: &crate::heap_types::LispString,
    beg: i64,
    end: i64,
    beg0: Value,
    end0: Value,
) -> Result<Option<CharRange>, Flow> {
    if beg == end {
        return Ok(None);
    }
    let (start, finish) = if beg > end { (end, beg) } else { (beg, end) };
    let len = s.schars() as i64;
    if !(0 <= start && start <= finish && finish <= len) {
        return Err(args_out_of_range_range(beg0, end0));
    }
    Ok(Some(CharRange::new(
        string_char_pos(start as usize),
        string_char_pos(finish as usize),
    )))
}

pub(crate) fn validate_buffer_point(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
) -> Result<usize, Flow> {
    validate_buffer_point_raw(buf, pos, Value::fixnum(pos))
}

pub(crate) fn validate_buffer_point_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    _pos0: Value,
) -> Result<usize, Flow> {
    validate_buffer_point_emacs_byte_pos_raw(buf, pos, _pos0).map(EmacsBytePos::get)
}

/// Char-native sibling of [`validate_buffer_point_emacs_byte_pos_raw`].
pub(crate) fn validate_buffer_point_char_pos_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    _pos0: Value,
) -> Result<crate::buffer::CharPos0, Flow> {
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= pos && pos <= point_max) {
        return Err(args_out_of_range_point(pos));
    }
    Ok(crate::buffer::CharPos0::from_lisp(validated_lisp_char_pos(
        pos,
    )))
}

pub(crate) fn validate_buffer_point_emacs_byte_pos_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    _pos0: Value,
) -> Result<EmacsBytePos, Flow> {
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= pos && pos <= point_max) {
        return Err(args_out_of_range_point(pos));
    }
    Ok(elisp_pos_to_byte(buf, validated_lisp_char_pos(pos)))
}

pub(crate) fn validate_buffer_property_point_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    pos0: Value,
) -> Result<usize, Flow> {
    validate_buffer_property_point_emacs_byte_pos_raw(buf, pos, pos0).map(EmacsBytePos::get)
}

/// Char-native sibling of
/// [`validate_buffer_property_point_emacs_byte_pos_raw`]: the same bounds
/// check without the char->byte conversion. GNU's textprop entry points
/// (`validate_interval_range`) work purely in character positions — the
/// interval tree is char-indexed — so a property read needs NO byte position
/// at all; converting to bytes and back was two anchored scans per lookup.
pub(crate) fn validate_buffer_property_point_char_pos_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    pos0: Value,
) -> Result<crate::buffer::CharPos0, Flow> {
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= pos && pos <= point_max) {
        return Err(args_out_of_range_point_pair(pos0));
    }
    Ok(crate::buffer::CharPos0::from_lisp(validated_lisp_char_pos(
        pos,
    )))
}

pub(crate) fn validate_buffer_property_point_emacs_byte_pos_raw(
    buf: &crate::buffer::buffer::Buffer,
    pos: i64,
    pos0: Value,
) -> Result<EmacsBytePos, Flow> {
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= pos && pos <= point_max) {
        return Err(args_out_of_range_point_pair(pos0));
    }
    Ok(elisp_pos_to_byte(buf, validated_lisp_char_pos(pos)))
}

fn validate_buffer_property_range(
    buf: &crate::buffer::buffer::Buffer,
    beg: i64,
    end: i64,
    beg0: Value,
    end0: Value,
) -> Result<Option<EmacsByteRange>, Flow> {
    validate_buffer_property_emacs_byte_range(buf, beg, end, beg0, end0)
}

fn validate_buffer_property_emacs_byte_range(
    buf: &crate::buffer::buffer::Buffer,
    beg: i64,
    end: i64,
    beg0: Value,
    end0: Value,
) -> Result<Option<EmacsByteRange>, Flow> {
    if beg == end {
        return Ok(None);
    }
    let (start, finish) = if beg > end { (end, beg) } else { (beg, end) };
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let point_max = buf.point_max_lisp_char_pos().as_i64();
    if !(point_min <= start && start <= finish && finish <= point_max) {
        return Err(args_out_of_range_range(beg0, end0));
    }
    Ok(Some(EmacsByteRange::new(
        elisp_pos_to_byte(buf, validated_lisp_char_pos(start)),
        elisp_pos_to_byte(buf, validated_lisp_char_pos(finish)),
    )))
}

/// Convert a 0-based byte position to a 1-based Elisp char position.
pub(crate) fn byte_to_elisp_pos(
    buf: &crate::buffer::buffer::Buffer,
    byte_pos: EmacsBytePos,
) -> i64 {
    buf.emacs_byte_pos_to_lisp_char_pos(byte_pos).as_i64()
}

pub(crate) fn resolve_buffer_id_in_buffers(
    buffers: &BufferManager,
    object: Option<&Value>,
) -> Result<BufferId, Flow> {
    match object {
        None => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_buffer() => v
            .as_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("Invalid buffer")])),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *other],
        )),
    }
}

fn resolve_text_property_buffer_id_in_buffers(
    buffers: &BufferManager,
    object: Option<&Value>,
) -> Result<BufferId, Flow> {
    match object {
        None => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_buffer() => v
            .as_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("Invalid buffer")])),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("buffer-or-string-p"), *other],
        )),
    }
}

fn resolve_char_property_target_in_state(
    frames: Option<&FrameManager>,
    buffers: &BufferManager,
    object: Option<&Value>,
) -> Result<(BufferId, Option<WindowId>), Flow> {
    match object {
        None => buffers
            .current_buffer()
            .map(|b| (b.id, None))
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| (b.id, None))
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_buffer() => v
            .as_buffer_id()
            .map(|id| (id, None))
            .ok_or_else(|| signal("error", vec![Value::string("Invalid buffer")])),
        Some(v) if v.is_window() => {
            let Some(frames) = frames else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("buffer-or-string-p"), *v],
                ));
            };
            let wid = WindowId(v.as_window_id().expect("window value has an id"));
            let window = frames.lookup_window(wid).ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), *v],
                )
            })?;
            let buffer_id = window.buffer_id().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("window-live-p"), *v],
                )
            })?;
            Ok((buffer_id, Some(wid)))
        }
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("buffer-or-string-p"), *other],
        )),
    }
}

/// Resolve a char-property OBJECT argument (None/nil/buffer/window) to a
/// `BufferId`, matching GNU `get_char_property_and_overlay`'s window handling:
/// a WINDOW object resolves to its buffer (requires `frames` to be `Some`).
/// The window-specific overlay matching is dropped here; callers that need the
/// `WindowId` for overlay matching use `resolve_char_property_target_in_state`
/// directly.
pub(crate) fn resolve_char_property_buffer_id_with_frames(
    frames: Option<&FrameManager>,
    buffers: &BufferManager,
    object: Option<&Value>,
) -> Result<BufferId, Flow> {
    resolve_char_property_target_in_state(frames, buffers, object).map(|(id, _wid)| id)
}

/// Interned-once ids for the read-only verification walk — it runs on
/// every text-property mutation builtin and re-hashed these names per
/// call.
#[inline(always)]
fn inhibit_read_only_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("inhibit-read-only"))
}

#[inline(always)]
fn read_only_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("read-only"))
}

#[inline(always)]
fn inhibit_modification_hooks_sym() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("inhibit-modification-hooks"))
}

pub(crate) fn current_buffer_id_in_buffers(buffers: &BufferManager) -> Result<BufferId, Flow> {
    buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))
}

pub(crate) fn expect_overlay(value: &Value) -> Result<Value, Flow> {
    if !value.is_overlay() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("overlayp"), *value],
        ));
    }
    Ok(*value)
}

pub(crate) fn resolve_overlay_buffer_id(overlay_val: Value) -> Option<BufferId> {
    overlay_val.as_overlay_data().and_then(|d| d.buffer)
}

pub(crate) fn ensure_marker_points_into_buffer(
    buffers: &BufferManager,
    value: &Value,
    buffer_id: BufferId,
) -> Result<(), Flow> {
    let Some((Some(marker_buffer_id), _, _)) = super::marker::marker_logical_fields(value) else {
        return Ok(());
    };
    if buffers.get(marker_buffer_id).is_none() {
        return Ok(());
    }
    if marker_buffer_id == buffer_id {
        return Ok(());
    }
    Err(signal(
        "error",
        vec![Value::string("Marker points into wrong buffer"), *value],
    ))
}

/// Check if the OBJECT argument is a string.  Returns Some(Value) if so.
pub(crate) fn is_string_object(object: Option<&Value>) -> Option<Value> {
    match object {
        Some(v) if v.is_string() => Some(*v),
        _ => None,
    }
}

pub(crate) fn string_char_to_elisp_pos(
    _s: &crate::heap_types::LispString,
    char_pos: CharPos0,
) -> i64 {
    char_pos.get() as i64
}

/// Write back a modified TextPropertyTable to string text properties.
pub(crate) fn save_string_props_for_value(value: Value, table: TextPropertyTable) {
    set_string_text_properties_table_for_value(value, table);
}

/// Iterate a plist (alternating key value key value ...) from a list or vec.
/// Returns pairs of (property-name, value).
fn plist_pairs(plist: &Value) -> Result<Vec<(Value, Value)>, Flow> {
    if plist.is_nil() {
        return Ok(Vec::new());
    }
    if !plist.is_cons() {
        return Ok(vec![(expect_property_key(plist)?, Value::NIL)]);
    }

    let mut pairs = Vec::new();
    let mut tail = *plist;
    loop {
        if !tail.is_cons() {
            break;
        }
        let name = tail.cons_car();
        let rest = tail.cons_cdr();
        if !rest.is_cons() {
            return Err(signal(
                "error",
                vec![Value::string("Odd length text property list")],
            ));
        }
        pairs.push((expect_property_key(&name)?, rest.cons_car()));
        tail = rest.cons_cdr();
    }
    Ok(pairs)
}

fn plist_names_for_remove(plist: Value) -> Vec<Value> {
    let mut names = Vec::new();
    let mut tail = plist;
    while tail.is_cons() {
        names.push(tail.cons_car());
        tail = tail.cons_cdr();
        if tail.is_cons() {
            tail = tail.cons_cdr();
        } else {
            break;
        }
    }
    names
}

fn list_names_for_remove(list: Value) -> Vec<Value> {
    let mut names = Vec::new();
    let mut tail = list;
    while tail.is_cons() {
        names.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    names
}

// ===========================================================================
// Text property builtins
// ===========================================================================

/// GNU `verify_interval_modification` (textprop.c:2184), restricted to the
/// read-only check.  Walks intervals overlapping `[byte_start, byte_end)`
/// in BUF_ID and signals `text-read-only` if any interval has a non-nil
/// `read-only` property that is not silenced by either the
/// `inhibit-read-only` interval property or the dynamic
/// `inhibit-read-only` variable.
pub(crate) fn verify_text_read_only_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf_id: BufferId,
    byte_start: usize,
    byte_end: usize,
) -> Result<(), Flow> {
    verify_text_read_only_emacs_byte_range_in_state(
        obarray,
        buffers,
        buf_id,
        EmacsByteRange::new(EmacsBytePos::new(byte_start), EmacsBytePos::new(byte_end)),
    )
}

pub(crate) fn verify_text_read_only_emacs_byte_range_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf_id: BufferId,
    byte_range: EmacsByteRange,
) -> Result<(), Flow> {
    if byte_range.is_empty() {
        return Ok(());
    }
    let Some(buf) = buffers.get(buf_id) else {
        return Ok(());
    };
    let iro = inhibit_read_only_sym();
    let inhibit = buf
        .get_buffer_local_by_sym_id_gated(iro, obarray.is_localized(iro))
        .unwrap_or_else(|| {
            obarray
                .symbol_value("inhibit-read-only")
                .copied()
                .unwrap_or(Value::NIL)
        });
    // INTERVAL_GENERALLY_WRITABLE_P: when inhibit-read-only is non-nil
    // and not a list, every interval is writable regardless of its
    // read-only property.  GNU intervals.h:210.
    if !inhibit.is_nil() && !inhibit.is_cons() {
        return Ok(());
    }
    let read_only_sym = Value::from_sym_id(read_only_sym());
    let inhibit_sym = Value::from_sym_id(inhibit_read_only_sym());
    // GNU `textget`'s control variables, resolved ONCE for the walk instead
    // of per interval: no Lisp runs between here and the last interval (the
    // same invariant `CharPropertyResolver` documents), and each
    // `current_textprop_variable_value` read pays a localized-gate probe —
    // per interval it dominated large-range `put-text-property` (25% of the
    // 20k-interval bench).
    let read_only_aliases = current_textprop_variable_value(
        obarray,
        buffers,
        TextPropertyControlVariable::CharPropertyAliasAlist,
    )
    .and_then(|value| assq_rest(value, read_only_sym))
    .unwrap_or(Value::NIL);
    let read_only_default = current_textprop_variable_value(
        obarray,
        buffers,
        TextPropertyControlVariable::DefaultTextProperties,
    )
    .filter(|value| value.is_cons())
    .and_then(|defaults| plist_get_value(defaults, read_only_sym));
    buf.text_props_try_for_each_interval_plist_in_emacs_byte_range(byte_range, |_range, plist| {
        let direct = DirectCharProperties::from_plist(plist, read_only_sym);
        let mut alias_tail = read_only_aliases;
        let alias_iter = std::iter::from_fn(move || {
            if !alias_tail.is_cons() {
                return None;
            }
            let alias = alias_tail.cons_car();
            alias_tail = alias_tail.cons_cdr();
            Some(alias)
        });
        let read_only = resolve_effective_char_property(
            direct,
            |category, property| {
                let category_id = symbol_id_for_property_lookup(category)?;
                let property_id = symbol_id_for_property_lookup(property)?;
                obarray.get_property_id(category_id, property_id)
            },
            read_only_sym,
            alias_iter,
            |name| plist_get_value(plist, name),
            read_only_default,
        )
        .unwrap_or(Value::NIL);
        if read_only.is_nil() {
            return Ok::<(), Flow>(());
        }
        // INTERVAL_EXPRESSLY_WRITABLE_P (intervals.h:217).
        let express_inhibit = plist_get_value(plist, inhibit_sym).unwrap_or(Value::NIL);
        if !express_inhibit.is_nil() {
            return Ok(());
        }
        if inhibit.is_cons() && value_in_list(read_only, inhibit) {
            return Ok(());
        }
        let args = if read_only.is_string() {
            vec![read_only]
        } else {
            vec![]
        };
        Err(signal(LispCondition::TextReadOnly, args))
    })?;
    Ok(())
}

fn value_in_list(needle: Value, list: Value) -> bool {
    let mut cursor = list;
    while cursor.is_cons() {
        if eq_value(&cursor.cons_car(), &needle) {
            return true;
        }
        cursor = cursor.cons_cdr();
    }
    false
}

/// GNU `TMEM(sym, set)` (intervals.h): if SET is a list, whether SYM is `memq`
/// it; otherwise whether SET is non-nil (`t` means "all properties").
fn text_prop_sticky_member(sym: Value, set: Value) -> bool {
    if set.is_cons() {
        value_in_list(sym, set)
    } else {
        !set.is_nil()
    }
}

/// `inhibit-read-only` silences this read-only value: never (nil), or when it
/// is a list containing the value. (The "non-nil non-list" blanket case is
/// handled by the caller before reaching here.)
fn read_only_silenced(read_only: Value, inhibit: Value) -> bool {
    inhibit.is_cons() && value_in_list(read_only, inhibit)
}

fn text_read_only_flow(read_only: Value) -> Flow {
    let args = if read_only.is_string() {
        vec![read_only]
    } else {
        vec![]
    };
    signal(LispCondition::TextReadOnly, args)
}

/// GNU `verify_interval_modification` (textprop.c:2184), the `start == end`
/// insertion case: signal `text-read-only` when inserting at `byte_pos` is
/// forbidden by the `read-only` property of the adjacent characters, honoring
/// stickiness. The char *after* blocks only when `read-only` is front-sticky;
/// the char *before* blocks unless `read-only` is rear-nonsticky (so a plain
/// `(put-text-property ... 'read-only t)` — rear-sticky by default — forbids
/// insertion right after it, while inserting before it stays allowed). This is
/// what lets minibuffer input through: the prompt is `rear-nonsticky t`.
pub(crate) fn verify_text_read_only_for_insert_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf_id: BufferId,
    byte_pos: EmacsBytePos,
) -> Result<(), Flow> {
    let Some(buf) = buffers.get(buf_id) else {
        return Ok(());
    };
    let iro = inhibit_read_only_sym();
    let inhibit = buf
        .get_buffer_local_by_sym_id_gated(iro, obarray.is_localized(iro))
        .unwrap_or_else(|| {
            obarray
                .symbol_value("inhibit-read-only")
                .copied()
                .unwrap_or(Value::NIL)
        });
    // inhibit-read-only non-nil and not a list: every modification is allowed.
    if !inhibit.is_nil() && !inhibit.is_cons() {
        return Ok(());
    }
    let read_only_sym = Value::symbol("read-only");
    let accessible = buf.accessible_emacs_byte_range();
    let begv = accessible.start().get();
    let zv = accessible.end().get();
    let pos = byte_pos.get();

    // Character after the insertion point: blocks only if `read-only` is
    // front-sticky there.
    if pos < zv {
        let after = lookup_buffer_text_property_at_emacs_byte_pos(
            obarray,
            buffers,
            buf,
            byte_pos,
            read_only_sym,
        );
        if !after.is_nil() && !read_only_silenced(after, inhibit) {
            let front_sticky = lookup_buffer_text_property_at_emacs_byte_pos(
                obarray,
                buffers,
                buf,
                byte_pos,
                StickinessProperty::FrontSticky.value(),
            );
            if text_prop_sticky_member(read_only_sym, front_sticky) {
                return Err(text_read_only_flow(after));
            }
        }
    }

    // Character before the insertion point: blocks unless `read-only` is
    // rear-nonsticky there (rear-sticky is the default).
    if pos > begv {
        let before_byte = EmacsBytePos::new(pos - 1);
        let before = lookup_buffer_text_property_at_emacs_byte_pos(
            obarray,
            buffers,
            buf,
            before_byte,
            read_only_sym,
        );
        if !before.is_nil() && !read_only_silenced(before, inhibit) {
            let rear_nonsticky = lookup_buffer_text_property_at_emacs_byte_pos(
                obarray,
                buffers,
                buf,
                before_byte,
                StickinessProperty::RearNonsticky.value(),
            );
            if !text_prop_sticky_member(read_only_sym, rear_nonsticky) {
                return Err(text_read_only_flow(before));
            }
        }
    }

    Ok(())
}

/// Resolve OBJECT-arg to a buffer and verify text-read-only over the
/// `[BEG, END)` byte range.  No-op if OBJECT is a string (text properties
/// on strings have no read-only enforcement in GNU either).
fn verify_property_change_read_only(
    eval: &mut super::eval::Context,
    args: &[Value],
    object_arg_idx: usize,
) -> Result<(), Flow> {
    if is_string_object(args.get(object_arg_idx)).is_some() {
        return Ok(());
    }
    if args.len() < 2 {
        return Ok(());
    }
    let beg = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let buf_id =
        resolve_text_property_buffer_id_in_buffers(&eval.buffers, args.get(object_arg_idx))?;
    let byte_range = {
        let buf = eval
            .buffers
            .get(buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
        let Some(range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
            return Ok(());
        };
        range
    };
    verify_text_read_only_emacs_byte_range_in_state(
        &eval.obarray,
        &eval.buffers,
        buf_id,
        byte_range,
    )
}

fn buffer_property_range_for_args(
    eval: &super::eval::Context,
    args: &[Value],
    object_arg_idx: usize,
) -> Result<Option<(BufferId, EmacsByteRange)>, Flow> {
    if is_string_object(args.get(object_arg_idx)).is_some() {
        return Ok(None);
    }
    if args.len() < 2 {
        return Ok(None);
    }
    let beg = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
    let buf_id =
        resolve_text_property_buffer_id_in_buffers(&eval.buffers, args.get(object_arg_idx))?;
    let Some(buf) = eval.buffers.get(buf_id) else {
        return Err(signal(
            "error",
            vec![Value::string("Buffer does not exist")],
        ));
    };
    validate_buffer_property_range(buf, beg, end, args[0], args[1])
        .map(|range| range.map(|byte_range| (buf_id, byte_range)))
}

fn begin_buffer_text_property_change(
    eval: &mut super::eval::Context,
    buf_id: BufferId,
    byte_range: EmacsByteRange,
) -> Result<(Option<BufferId>, crate::buffer::TextChange), Flow> {
    let saved_current = eval.buffers.current_buffer_id();
    if saved_current != Some(buf_id) {
        eval.set_current_buffer_unrecorded(buf_id)?;
    }
    let change = super::editfns::text_change_for_unchanged_extent_in_manager(
        &eval.buffers,
        buf_id,
        byte_range,
    )?;
    super::editfns::signal_before_property_change(eval, change)?;
    Ok((saved_current, change))
}

fn finish_buffer_text_property_change(
    eval: &mut super::eval::Context,
    saved_current: Option<BufferId>,
    change: crate::buffer::TextChange,
) -> Result<(), Flow> {
    let result = super::editfns::signal_after_property_change(eval, change);
    if let Some(saved) = saved_current {
        eval.restore_current_buffer_if_live(saved);
    }
    result
}

fn call_text_property_hook_lists(
    eval: &mut super::eval::Context,
    hook_lists: Vec<Value>,
    lisp_start: i64,
    lisp_end: i64,
) -> Result<(), Flow> {
    if hook_lists.is_empty() {
        return Ok(());
    }
    let start_v = Value::fixnum(lisp_start);
    let end_v = Value::fixnum(lisp_end);
    let specpdl_count = eval.specpdl.len();
    eval.try_specbind_or_unwind_to(specpdl_count, inhibit_modification_hooks_sym(), Value::T)?;
    // The collected hook chains live only in this Rust Vec, and each hook
    // can unlink its own chain from the interval plist (the one-shot-hook
    // idiom) and trigger GC — freeing the conses the walk still reads. Keep
    // every chain alive under ONE root by threading the list heads onto a
    // heap list; the moving cursor stays inside a rooted chain. GNU's C
    // locals survive this via conservative stack scanning (textprop.c
    // verify_interval_modification), which the precise GC does not scan.
    let mut hook_holder = Value::NIL;
    for hook_list in hook_lists.iter().rev() {
        hook_holder = Value::cons(*hook_list, hook_holder);
    }
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(hook_holder);
    // Rooting the WALK CURSOR (updated per step) additionally keeps the
    // remaining chain alive even if a hook setcdr's the chain mid-walk —
    // marking is transitive from the cursor, exactly the survival GNU gets
    // from its conservatively-scanned tail local.
    let cursor_slot = eval.push_specpdl_root_slot(Value::NIL);
    let result = (|| -> Result<(), Flow> {
        for hook_list in hook_lists {
            let mut cursor = hook_list;
            while cursor.is_cons() {
                eval.set_specpdl_root_slot(&cursor_slot, cursor);
                let fn_v = cursor.cons_car();
                eval.apply(fn_v, vec![start_v, end_v])?;
                cursor = cursor.cons_cdr();
            }
        }
        Ok(())
    })();
    eval.restore_specpdl_roots(root_scope);
    eval.unbind_to_with_result(specpdl_count, result.map(|()| Value::NIL))
        .map(|_| ())
}

/// GNU `verify_interval_modification` for buffer text changes.
///
/// This is the interval-hook part of `prepare_to_modify_buffer_1`: for
/// non-empty changes, call `modification-hooks` before the text is changed;
/// for insertions, record `insert-behind-hooks` and `insert-in-front-hooks`
/// so `signal_after_change` can replay them after the inserted text exists.
pub(crate) fn prepare_interval_modification_for_change(
    eval: &mut super::eval::Context,
    buf_id: BufferId,
    byte_start: EmacsBytePos,
    byte_end: EmacsBytePos,
) -> Result<(), Flow> {
    eval.interval_insert_behind_hooks = Value::NIL;
    eval.interval_insert_in_front_hooks = Value::NIL;

    if byte_start == byte_end {
        record_interval_insert_hooks(eval, buf_id, byte_start);
        return Ok(());
    }

    if super::editfns::inhibit_modification_hooks(eval) {
        return Ok(());
    }

    let (lisp_start, lisp_end, hook_lists) = {
        let obarray = &eval.obarray;
        let buffers = &eval.buffers;
        let Some(buf) = buffers.get(buf_id) else {
            return Ok(());
        };
        let byte_range = EmacsByteRange::ordered(byte_start, byte_end);
        let lisp_start = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.start())
            .as_i64();
        let lisp_end = buf
            .emacs_byte_pos_to_lisp_char_pos(byte_range.end())
            .as_i64();
        let mod_sym = Value::symbol("modification-hooks");
        let mut prev: Option<Value> = None;
        let mut hooks = Vec::new();
        let _ = buf.text_props_try_for_each_interval_in_emacs_byte_range(
            byte_range,
            |_range, plist| {
                // GNU `verify_interval_modification` reads `modification-hooks'
                // via `textget`, which resolves through a `category' symbol.
                let mh = lookup_text_property_from_plist_slice(obarray, buffers, plist, mod_sym);
                if mh.is_nil() {
                    return Ok::<(), ()>(());
                }
                if let Some(p) = prev
                    && eq_value(&p, &mh)
                {
                    return Ok(());
                }
                prev = Some(mh);
                hooks.push(mh);
                Ok(())
            },
        );
        (lisp_start, lisp_end, hooks)
    };

    call_text_property_hook_lists(eval, hook_lists, lisp_start, lisp_end)
}

fn record_interval_insert_hooks(
    eval: &mut super::eval::Context,
    buf_id: BufferId,
    byte_pos: EmacsBytePos,
) {
    let Some(buf) = eval.buffers.get(buf_id) else {
        return;
    };
    // With no text properties anywhere, no `insert-in-front-hooks' /
    // `insert-behind-hooks' can exist, so skip the per-insert property lookups
    // -- each of which does a byte->char conversion.  This is the hot path for
    // inserts into property-free buffers (byte-compilation output, batch work).
    // The hook fields were already reset to nil by the caller, so leaving them
    // is correct.
    if buf.text_props_is_empty() {
        return;
    }
    let behind_sym = Value::symbol("insert-behind-hooks");
    let front_sym = Value::symbol("insert-in-front-hooks");
    let accessible = buf.accessible_emacs_byte_region();

    if byte_pos > accessible.start()
        && let Some(prev_len) = buf.char_before_emacs_byte_len(byte_pos)
    {
        let prev_byte = byte_pos.saturating_sub_len(prev_len);
        if let Some(hooks) = buf.text_props_get_property_at_emacs_byte_pos(prev_byte, behind_sym)
            && !hooks.is_nil()
        {
            eval.interval_insert_behind_hooks = hooks;
        }
    }

    if byte_pos < accessible.end()
        && let Some(hooks) = buf.text_props_get_property_at_emacs_byte_pos(byte_pos, front_sym)
        && !hooks.is_nil()
    {
        eval.interval_insert_in_front_hooks = hooks;
    }
}

/// GNU `report_interval_modification`: run insert text-property hooks after
/// insertion, passing the inserted character range.
pub(crate) fn report_interval_modification(
    eval: &mut super::eval::Context,
    lisp_start: i64,
    lisp_end: i64,
) -> Result<(), Flow> {
    let behind = eval.interval_insert_behind_hooks;
    let front = eval.interval_insert_in_front_hooks;
    if !behind.is_nil() {
        call_text_property_hook_lists(eval, vec![behind], lisp_start, lisp_end)?;
    }
    if !front.is_nil() && !eq_value(&front, &behind) {
        call_text_property_hook_lists(eval, vec![front], lisp_start, lisp_end)?;
    }
    Ok(())
}

/// (put-text-property BEG END PROP VAL &optional OBJECT)
pub(crate) fn builtin_put_text_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("put-text-property", &args, 4)?;
    expect_max_args("put-text-property", &args, 5)?;
    // GNU `add_text_properties_1` order: argument validation first
    // (`validate_interval_range`), then the has-all skip, and ONLY when some
    // interval will actually change does `modify_text_properties` run —
    // read-only verification and the modification hooks included. A no-op
    // put returns nil having done ONE early-exiting walk: no text-read-only
    // signal, no hooks, no undo entry, no apply walk.
    if let Some((buf_id, byte_range)) = buffer_property_range_for_args(eval, &args, 4)? {
        let unchanged = eval.buffers.get(buf_id).is_some_and(|buf| {
            let properties = [(args[2], args[3])];
            buf.text_props_range_has_all_properties_in_emacs_byte_range(byte_range, &properties)
        });
        if unchanged {
            return Ok(Value::NIL);
        }
        verify_property_change_read_only(eval, &args, 4)?;
        let (saved_current, change) = begin_buffer_text_property_change(eval, buf_id, byte_range)?;
        let result = builtin_put_text_property_in_buffers(&mut eval.buffers, args.clone())?;
        finish_buffer_text_property_change(eval, saved_current, change)?;
        Ok(result)
    } else {
        // Strings (and degenerate arg shapes): no buffer hooks, no buffer
        // read-only semantics — GNU's modify_text_properties is buffer-only.
        builtin_put_text_property_in_buffers(&mut eval.buffers, args)
    }
}

pub(crate) fn builtin_put_text_property_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("put-text-property", &args, 4)?;
    expect_max_args("put-text-property", &args, 5)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let prop = expect_property_key(&args[2])?;
    let val = args[3];

    if let Some(str_val) = is_string_object(args.get(4)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        table.put_property_for_object_char_len(char_range, string_char_len(s.schars()), prop, val);
        save_string_props_for_value(str_val, table);
        return Ok(Value::NIL);
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(4))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
        return Ok(Value::NIL);
    };
    if buffers
        .put_buffer_text_property_in_emacs_byte_range(buf_id, byte_range, prop, val)
        .unwrap_or(false)
    {
        let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    }
    Ok(Value::NIL)
}

/// (get-text-property POS PROP &optional OBJECT)
pub(crate) fn builtin_get_text_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_text_property_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_get_text_property_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-text-property", &args, 2)?;
    expect_max_args("get-text-property", &args, 3)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = expect_property_key(&args[1])?;

    if let Some(str_val) = is_string_object(args.get(2)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let char_pos = validate_string_char_pos_raw(s, pos, args[0])?;
        if char_pos.get() == s.schars() {
            return Ok(Value::NIL);
        }
        if let Some(table) = get_string_text_properties_table_for_value(str_val) {
            return Ok(lookup_string_text_property(
                obarray,
                buffers,
                &table,
                char_pos.get(),
                prop,
            ));
        }
        return Ok(lookup_char_property_from_direct(
            obarray,
            buffers,
            |_| None,
            prop,
            true,
        ));
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(2))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let char_pos = validate_buffer_property_point_char_pos_raw(buf, pos, args[0])?;
    if char_pos >= buf.total_char_end_pos() {
        return Ok(Value::NIL);
    }
    Ok(lookup_buffer_text_property_at_char_pos(
        obarray, buffers, buf, char_pos, prop,
    ))
}

pub(crate) fn buffer_overlay_property_at_byte_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::buffer::Buffer,
    byte_pos: usize,
    prop: Value,
    window_id: Option<WindowId>,
) -> Option<(Value, Value)> {
    let mut overlays = buf
        .overlays
        .overlays_at_emacs_byte_pos(EmacsBytePos::new(byte_pos));
    buf.overlays
        .sort_overlay_ids_by_priority_desc(&mut overlays);
    for overlay in overlays {
        if let Some(wid) = window_id {
            let window_prop =
                lookup_overlay_property(obarray, buffers, overlay, Value::symbol("window"));
            if window_prop
                .as_window_id()
                .is_some_and(|overlay_wid| overlay_wid != wid.0)
            {
                continue;
            }
        }
        let value = lookup_overlay_property(obarray, buffers, overlay, prop);
        if !value.is_nil() {
            return Some((value, overlay));
        }
    }
    None
}

pub(crate) fn buffer_overlay_property_for_inserted_char_at_byte_pos(
    buf: &crate::buffer::buffer::Buffer,
    byte_pos: usize,
    prop: Value,
) -> Option<(Value, Value)> {
    let overlay_id = buf
        .overlays
        .highest_priority_overlay_for_inserted_emacs_byte_pos(EmacsBytePos::new(byte_pos), &prop)?;
    let value = buf.overlays.overlay_get(overlay_id, &prop)?;
    Some((value, overlay_id))
}

/// The direction from which GNU's `text_property_stickiness` inherits a
/// property. Naming all three integer return values makes every caller handle
/// the full decision at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyStickiness {
    FromFollowing,
    FromPreceding,
    Neither,
}

/// The buffer bounds used by one internal property lookup.
///
/// Lisp-visible property primitives validate against `BEGV..ZV`. GNU's
/// `get_local_map` is deliberately different: it clips while narrowed, then
/// temporarily widens, so both validation and stickiness use full `BEG..Z`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BufferPropertyLookupDomain {
    Accessible,
    FullBuffer,
}

impl BufferPropertyLookupDomain {
    fn beginning(self, buf: &Buffer) -> LispCharPos1 {
        match self {
            Self::Accessible => buf.point_min_lisp_char_pos(),
            Self::FullBuffer => buf.full_lisp_char_region().beg(),
        }
    }

    fn contains(self, buf: &Buffer, pos: LispCharPos1) -> bool {
        match self {
            Self::Accessible => {
                buf.point_min_lisp_char_pos() <= pos && pos <= buf.point_max_lisp_char_pos()
            }
            Self::FullBuffer => buf.full_lisp_char_region().contains(pos),
        }
    }
}

pub(crate) fn buffer_pos_property_at_accessible_lisp_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: i64,
    prop: Value,
) -> Result<Value, Flow> {
    buffer_pos_property_in_domain(
        obarray,
        buffers,
        buf,
        pos,
        prop,
        BufferPropertyLookupDomain::Accessible,
    )
}

/// GNU `get_local_map` temporarily widens before calling
/// `Fget_pos_property`. This entry point exposes only that internal policy;
/// ordinary Lisp property primitives retain accessible-range validation.
pub(crate) fn buffer_pos_property_at_full_lisp_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: LispCharPos1,
    prop: Value,
) -> Result<Value, Flow> {
    debug_assert!(buf.full_lisp_char_region().contains(pos));
    buffer_pos_property_in_domain(
        obarray,
        buffers,
        buf,
        pos.as_i64(),
        prop,
        BufferPropertyLookupDomain::FullBuffer,
    )
}

fn buffer_pos_property_in_domain(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: i64,
    prop: Value,
    domain: BufferPropertyLookupDomain,
) -> Result<Value, Flow> {
    let byte_pos = buf.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(pos)).get();
    if let Some((value, _overlay_id)) =
        buffer_overlay_property_for_inserted_char_at_byte_pos(buf, byte_pos, prop)
    {
        return Ok(value);
    }

    // GNU src/editfns.c:339-349. The lower-bound guard must use the selected
    // domain: after `get_local_map` widens, a value immediately before the old
    // BEGV can be inherited at the clipped position.
    match text_property_stickiness_in_domain(obarray, buffers, buf, pos, prop, domain)? {
        PropertyStickiness::FromFollowing => Ok(text_property_value_at_char_pos(
            obarray,
            buffers,
            buf,
            LispCharPos1::new(pos),
            prop,
        )),
        PropertyStickiness::FromPreceding if pos > domain.beginning(buf).as_i64() => {
            Ok(text_property_value_at_char_pos(
                obarray,
                buffers,
                buf,
                LispCharPos1::new(pos - 1),
                prop,
            ))
        }
        PropertyStickiness::FromPreceding | PropertyStickiness::Neither => Ok(Value::NIL),
    }
}

/// GNU's `text_property_stickiness` (src/textprop.c:1901) validates each
/// delegated property read. The lookup domain makes the exceptional widened
/// caller explicit instead of weakening the Lisp-visible primitive.
fn text_property_stickiness_in_domain(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: i64,
    prop: Value,
    domain: BufferPropertyLookupDomain,
) -> Result<PropertyStickiness, Flow> {
    let ignore_previous_character = pos <= domain.beginning(buf).as_i64();
    let default_nonsticky =
        TextPropertyControlVariable::TextPropertyDefaultNonsticky.value_for_buffer(obarray, buf);
    let mut rear_sticky = !(ignore_previous_character
        || default_nonsticky
            .and_then(|value| assq_cdr_eq(&value, prop))
            .is_some_and(|value| value.is_truthy()));

    if rear_sticky && !ignore_previous_character {
        let previous_props = get_text_property_at_validated_char_pos(
            obarray,
            buffers,
            buf,
            pos - 1,
            StickinessProperty::RearNonsticky.value(),
            domain,
        )?;
        if rear_nonsticky_matches(previous_props, prop) {
            rear_sticky = false;
        }
    }

    let front_sticky = front_sticky_matches(
        get_text_property_at_validated_char_pos(
            obarray,
            buffers,
            buf,
            pos,
            StickinessProperty::FrontSticky.value(),
            domain,
        )?,
        prop,
    );

    match (rear_sticky, front_sticky) {
        (true, false) => Ok(PropertyStickiness::FromPreceding),
        (false, true) => Ok(PropertyStickiness::FromFollowing),
        (false, false) => Ok(PropertyStickiness::Neither),
        (true, true) => {
            if ignore_previous_character
                || get_text_property_at_validated_char_pos(
                    obarray,
                    buffers,
                    buf,
                    pos - 1,
                    prop,
                    domain,
                )?
                .is_nil()
            {
                Ok(PropertyStickiness::FromFollowing)
            } else {
                Ok(PropertyStickiness::FromPreceding)
            }
        }
    }
}

fn get_text_property_at_validated_char_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: i64,
    prop: Value,
    domain: BufferPropertyLookupDomain,
) -> Result<Value, Flow> {
    let pos = LispCharPos1::new(pos);
    if !domain.contains(buf, pos) {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(pos.as_i64()), Value::fixnum(pos.as_i64())],
        ));
    }
    Ok(text_property_value_at_char_pos(
        obarray, buffers, buf, pos, prop,
    ))
}

fn text_property_value_at_char_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &Buffer,
    pos: LispCharPos1,
    prop: Value,
) -> Value {
    lookup_buffer_text_property_at_char_pos(obarray, buffers, buf, pos.to_char_pos(), prop)
}

fn front_sticky_matches(value: Value, prop: Value) -> bool {
    value.is_t() || eq_member(&value, prop)
}

fn rear_nonsticky_matches(value: Value, prop: Value) -> bool {
    if value.is_nil() {
        return false;
    }
    if value.is_cons() {
        return eq_member(&value, prop);
    }
    true
}

fn assq_cdr_eq(list: &Value, prop: Value) -> Option<Value> {
    let mut cursor = *list;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        if entry.is_cons() && entry.cons_car().bits() == prop.bits() {
            return Some(entry.cons_cdr());
        }
        cursor = cursor.cons_cdr();
    }
    None
}

fn eq_member(list: &Value, prop: Value) -> bool {
    let mut cursor = *list;
    while cursor.is_cons() {
        if cursor.cons_car().bits() == prop.bits() {
            return true;
        }
        cursor = cursor.cons_cdr();
    }
    false
}

/// (get-char-property POS PROP &optional OBJECT)
/// For strings, same as get-text-property (no overlays).
pub(crate) fn builtin_get_char_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_char_property_with_frames(&eval.obarray, &eval.buffers, Some(&eval.frames), args)
}

pub(crate) fn builtin_get_char_property_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_char_property_with_frames(obarray, buffers, None, args)
}

/// Read a character property at a position already proven to be in the
/// buffer's full `BEG..Z` range, deliberately ignoring narrowing.
///
/// GNU's `get_local_map` temporarily widens the target buffer around its
/// `Fget_char_property` call (`src/intervals.c`).  Keeping that policy behind
/// a named internal entry point prevents ordinary Lisp property primitives
/// from accidentally losing their `BEGV..ZV` validation.
pub(crate) fn buffer_char_property_at_full_lisp_pos(
    obarray: &Obarray,
    buffers: &BufferManager,
    buf: &crate::buffer::buffer::Buffer,
    pos: LispCharPos1,
    prop: Value,
) -> Value {
    debug_assert!(buf.full_lisp_char_region().contains(pos));
    let char_pos = pos.to_char_pos();
    if char_pos >= buf.total_char_end_pos() {
        return Value::NIL;
    }
    if !buf.overlays.is_empty() {
        let byte_pos = buf.lisp_pos_to_emacs_byte_pos(pos);
        if let Some((value, _overlay_id)) =
            buffer_overlay_property_at_byte_pos(obarray, buffers, buf, byte_pos.get(), prop, None)
        {
            return value;
        }
    }
    lookup_buffer_text_property_at_char_pos(obarray, buffers, buf, char_pos, prop)
}

pub(crate) fn builtin_get_char_property_with_frames(
    obarray: &Obarray,
    buffers: &BufferManager,
    frames: Option<&FrameManager>,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-char-property", &args, 2)?;
    expect_max_args("get-char-property", &args, 3)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = expect_property_key(&args[1])?;

    if is_string_object(args.get(2)).is_some() {
        return builtin_get_text_property_in_state(obarray, buffers, args);
    }

    let (buf_id, window_id) = resolve_char_property_target_in_state(frames, buffers, args.get(2))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    // Overlay-free buffer: no byte position is needed anywhere — validate in
    // chars and answer from the char-indexed interval tree directly (see
    // `validate_buffer_property_point_char_pos_raw`).
    if buf.overlays.is_empty() {
        let char_pos = validate_buffer_point_char_pos_raw(buf, pos, args[0])?;
        if char_pos >= buf.total_char_end_pos() {
            return Ok(Value::NIL);
        }
        return Ok(lookup_buffer_text_property_at_char_pos(
            obarray, buffers, buf, char_pos, prop,
        ));
    }
    let byte_pos = validate_buffer_point_emacs_byte_pos_raw(buf, pos, args[0])?;
    if byte_pos == buffer_end_emacs_byte_pos(buf) {
        return Ok(Value::NIL);
    }

    if let Some((value, _overlay_id)) =
        buffer_overlay_property_at_byte_pos(obarray, buffers, buf, byte_pos.get(), prop, window_id)
    {
        return Ok(value);
    }

    Ok(lookup_buffer_text_property(
        obarray,
        buffers,
        buf,
        byte_pos.get(),
        prop,
    ))
}

/// (add-text-properties BEG END PROPS &optional OBJECT)
pub(crate) fn builtin_add_text_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("add-text-properties", &args, 3)?;
    expect_max_args("add-text-properties", &args, 4)?;
    // Same GNU `add_text_properties_1` order as `put-text-property`: the
    // has-all skip runs first, and a no-op add returns nil with no
    // read-only verification, hooks, undo entry, or apply walk.
    let pairs_for_probe = plist_pairs(&args[2])?;
    if let Some((buf_id, byte_range)) = buffer_property_range_for_args(eval, &args, 3)? {
        let unchanged = eval.buffers.get(buf_id).is_some_and(|buf| {
            buf.text_props_range_has_all_properties_in_emacs_byte_range(
                byte_range,
                &pairs_for_probe,
            )
        });
        if unchanged {
            return Ok(Value::NIL);
        }
        verify_property_change_read_only(eval, &args, 3)?;
        let (saved_current, change) = begin_buffer_text_property_change(eval, buf_id, byte_range)?;
        let result = builtin_add_text_properties_in_buffers(&mut eval.buffers, args.clone())?;
        finish_buffer_text_property_change(eval, saved_current, change)?;
        Ok(result)
    } else {
        builtin_add_text_properties_in_buffers(&mut eval.buffers, args)
    }
}

pub(crate) fn builtin_add_text_properties_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("add-text-properties", &args, 3)?;
    expect_max_args("add-text-properties", &args, 4)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let pairs = plist_pairs(&args[2])?;

    if let Some(str_val) = is_string_object(args.get(3)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let any_changed = table.apply_property_plist_for_object_char_len(
            char_range,
            string_char_len(s.schars()),
            &pairs,
            PropertyPlistApplication::AddProperties,
        );
        save_string_props_for_value(str_val, table);
        return Ok(if any_changed { Value::T } else { Value::NIL });
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(3))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
        return Ok(Value::NIL);
    };
    let mut any_changed = false;
    for (name, val) in pairs {
        if buffers
            .put_buffer_text_property_in_emacs_byte_range(buf_id, byte_range, name, val)
            .unwrap_or(false)
        {
            any_changed = true;
        }
    }
    if any_changed {
        let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    }
    Ok(if any_changed { Value::T } else { Value::NIL })
}

fn is_anonymous_face_plist(v: &Value) -> bool {
    // GNU treats a cons whose car is a keyword as an anonymous face plist
    // (e.g. (:foreground "red")), not a list of faces.
    v.is_cons() && v.cons_car().is_keyword()
}

fn improper_list_tail(list: Value) -> Value {
    let mut tail = list;
    let mut tortoise = list;
    let mut step = 0u64;
    while tail.is_cons() {
        tail = tail.cons_cdr();
        step += 1;
        if step.is_multiple_of(2) {
            if tortoise.is_cons() {
                tortoise = tortoise.cons_cdr();
            }
            if tortoise.bits() == tail.bits() {
                return list;
            }
        }
    }
    tail
}

fn merge_face_property(
    existing: Option<Value>,
    new_face: Value,
    append: bool,
) -> Result<Value, Flow> {
    let Some(existing_value) = existing else {
        return Ok(new_face);
    };
    if existing_value.is_nil() {
        return Ok(new_face);
    }
    if eq_value(&existing_value, &new_face) {
        return Ok(existing_value);
    }

    if existing_value.is_cons() && !is_anonymous_face_plist(&existing_value) {
        if append {
            if let Some(mut items) = list_to_vec(&existing_value) {
                items.push(new_face);
                return Ok(Value::list(items));
            }
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("listp"), improper_list_tail(existing_value)],
            ));
        }
        return Ok(Value::cons(new_face, existing_value));
    }

    Ok(if append {
        Value::list(vec![existing_value, new_face])
    } else {
        Value::list(vec![new_face, existing_value])
    })
}

/// `(add-face-text-property START END FACE &optional APPENDP OBJECT)`
pub(crate) fn builtin_add_face_text_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("add-face-text-property", &args, 3)?;
    expect_max_args("add-face-text-property", &args, 5)?;
    // GNU backs this with the same `add_text_properties_1` (textprop.c:1368):
    // the has-all skip precedes `modify_text_properties`, so a no-op add
    // (every interval's `face` already eq the new value) does no read-only
    // verification, hooks, undo entry, or apply walk.
    if let Some((buf_id, byte_range)) = buffer_property_range_for_args(eval, &args, 4)? {
        let new_face = args[2];
        let unchanged = eval.buffers.get(buf_id).is_some_and(|buf| {
            buf.text_props_range_has_all_properties_in_emacs_byte_range(
                byte_range,
                &[(Value::symbol("face"), new_face)],
            )
        });
        if unchanged {
            return Ok(Value::NIL);
        }
        verify_property_change_read_only(eval, &args, 4)?;
        let (saved_current, change) = begin_buffer_text_property_change(eval, buf_id, byte_range)?;
        let result = builtin_add_face_text_property_in_buffers(&mut eval.buffers, args.clone())?;
        finish_buffer_text_property_change(eval, saved_current, change)?;
        Ok(result)
    } else {
        builtin_add_face_text_property_in_buffers(&mut eval.buffers, args)
    }
}

pub(crate) fn builtin_add_face_text_property_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("add-face-text-property", &args, 3)?;
    expect_max_args("add-face-text-property", &args, 5)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let new_face = args[2];
    let append = args.get(3).is_some_and(|v| v.is_truthy());

    let object = args.get(4);

    if let Some(str_val) = is_string_object(object) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let char_beg = char_range.start().get();
        let char_end = char_range.end().get();
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        // GNU iterates intervals in [beg, end); per interval, fetch its existing
        // face value and merge. Walk the range segment-by-segment.
        let mut seg_start = char_beg;
        while seg_start < char_end {
            let seg_end =
                match table.next_property_change_after_char_pos(string_char_pos(seg_start)) {
                    Some(p) if p.get() < char_end => p.get(),
                    _ => char_end,
                };
            let existing =
                table.get_property_at_char_pos(string_char_pos(seg_start), Value::symbol("face"));
            let merged = merge_face_property(existing, new_face, append)?;
            table.put_property_for_object_char_len(
                CharRange::new(CharPos0::new(seg_start), CharPos0::new(seg_end)),
                string_char_len(s.schars()),
                Value::symbol("face"),
                merged,
            );
            seg_start = seg_end;
        }
        save_string_props_for_value(str_val, table);
        return Ok(Value::NIL);
    }

    let buf_id = match object {
        None => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_buffer() => v
            .as_buffer_id()
            .ok_or_else(|| signal("error", vec![Value::string("Invalid buffer")])),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("buffer-or-string-p"), *other],
        )),
    }?;

    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let Some(byte_range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
        return Ok(Value::NIL);
    };
    // GNU iterates intervals in [beg, end); per interval, fetch its existing
    // face value and merge. Walk the range segment-by-segment to preserve any
    // heterogeneous face properties already present.
    let mut segments: Vec<(EmacsByteRange, Value)> = Vec::new();
    let byte_end_pos = byte_range.end();
    let mut seg_start = byte_range.start();
    while seg_start < byte_end_pos {
        let seg_end = match buf.text_props_next_change_after_emacs_byte_pos(seg_start) {
            Some(p) if p < byte_end_pos => p,
            _ => byte_end_pos,
        };
        let existing =
            buf.text_props_get_property_at_emacs_byte_pos(seg_start, Value::symbol("face"));
        let merged = merge_face_property(existing, new_face, append)?;
        segments.push((EmacsByteRange::new(seg_start, seg_end), merged));
        seg_start = seg_end;
    }
    let mut any_changed = false;
    for (byte_range, merged) in segments {
        if buffers
            .put_buffer_text_property_in_emacs_byte_range(
                buf_id,
                byte_range,
                Value::symbol("face"),
                merged,
            )
            .unwrap_or(false)
        {
            any_changed = true;
        }
    }
    if any_changed {
        let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    }
    Ok(Value::NIL)
}

/// (remove-text-properties BEG END PROPS &optional OBJECT)
pub(crate) fn builtin_remove_text_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("remove-text-properties", &args, 3)?;
    expect_max_args("remove-text-properties", &args, 4)?;
    verify_property_change_read_only(eval, &args, 3)?;
    let names_for_probe = plist_names_for_remove(args[2]);
    let change =
        buffer_property_range_for_args(eval, &args, 3)?.and_then(|(buf_id, byte_range)| {
            let buf = eval.buffers.get(buf_id)?;
            buf.text_props_range_has_any_property_named_in_emacs_byte_range(
                byte_range,
                &names_for_probe,
            )
            .then_some((buf_id, byte_range))
        });
    let before = if let Some((buf_id, byte_range)) = change {
        Some(begin_buffer_text_property_change(eval, buf_id, byte_range)?)
    } else {
        None
    };
    let result = builtin_remove_text_properties_in_buffers(&mut eval.buffers, args.clone())?;
    if let Some((saved_current, change)) = before {
        finish_buffer_text_property_change(eval, saved_current, change)?;
    }
    Ok(result)
}

pub(crate) fn builtin_remove_text_properties_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("remove-text-properties", &args, 3)?;
    expect_max_args("remove-text-properties", &args, 4)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let names = plist_names_for_remove(args[2]);

    if let Some(str_val) = is_string_object(args.get(3)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let mut any_removed = false;
        for name in names {
            if table.remove_property_in_char_range(char_range, name) {
                any_removed = true;
            }
        }
        save_string_props_for_value(str_val, table);
        return Ok(if any_removed { Value::T } else { Value::NIL });
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(3))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
        return Ok(Value::NIL);
    };
    // GNU Fremove_text_properties first looks for an interval in the range
    // that holds one of the names and returns nil, tree untouched, when there
    // is none -- the common case for `syntax-propertize`'s per-chunk
    // `(remove-text-properties start end '(syntax-table nil syntax-multiline
    // nil))` in a buffer where those properties are rare.  The removal walk
    // below would otherwise split the intervals at both range edges.
    let present = buffers.get(buf_id).is_some_and(|buf| {
        buf.text_props_range_has_any_property_named_in_emacs_byte_range(byte_range, &names)
    });
    if !present {
        return Ok(Value::NIL);
    }
    // One split+collect interval walk (and one undo-run walk) for every
    // name, like `remove-list-of-text-properties`; GNU's `remove_properties`
    // strips all of PROPERTIES from each interval in a single pass.
    let any_removed = buffers
        .remove_buffer_text_properties_in_emacs_byte_range(buf_id, byte_range, &names)
        .unwrap_or(false);
    if any_removed {
        let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    }
    Ok(if any_removed { Value::T } else { Value::NIL })
}

/// (set-text-properties BEG END PROPS &optional OBJECT)
pub(crate) fn builtin_set_text_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-text-properties", &args, 3)?;
    expect_max_args("set-text-properties", &args, 4)?;
    verify_property_change_read_only(eval, &args, 3)?;
    let pairs_for_probe = if args[2].is_nil() {
        Vec::new()
    } else {
        plist_pairs(&args[2])?
    };
    let change =
        buffer_property_range_for_args(eval, &args, 3)?.and_then(|(buf_id, byte_range)| {
            let buf = eval.buffers.get(buf_id)?;
            (!pairs_for_probe.is_empty()
                || buf.text_props_range_has_any_interval_in_emacs_byte_range(byte_range))
            .then_some((buf_id, byte_range))
        });
    let before = if let Some((buf_id, byte_range)) = change {
        Some(begin_buffer_text_property_change(eval, buf_id, byte_range)?)
    } else {
        None
    };
    let result = builtin_set_text_properties_in_buffers(&mut eval.buffers, args.clone())?;
    if let Some((saved_current, change)) = before {
        finish_buffer_text_property_change(eval, saved_current, change)?;
    }
    Ok(result)
}

pub(crate) fn builtin_set_text_properties_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("set-text-properties", &args, 3)?;
    expect_max_args("set-text-properties", &args, 4)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    // set-text-properties accepts nil for PROPS (= remove all)
    let pairs = if args[2].is_nil() {
        Vec::new()
    } else {
        plist_pairs(&args[2])?
    };

    if let Some(str_val) = is_string_object(args.get(3)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let full_string = beg == 0 && end == s.schars() as i64;
        let had_intervals = string_has_text_property_interval_tree(str_val);
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        if pairs.is_empty() && !had_intervals {
            return Ok(Value::NIL);
        }
        if pairs.is_empty() && full_string {
            clear_string_text_properties_for_value(str_val);
            return Ok(Value::T);
        }
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        table.set_properties_for_object_char_len(char_range, string_char_len(s.schars()), pairs);
        save_string_props_for_value(str_val, table);
        return Ok(Value::T);
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(3))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
        return Ok(Value::NIL);
    };
    let _ = buffers.set_buffer_text_properties_in_emacs_byte_range(buf_id, byte_range, pairs);
    let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    Ok(Value::T)
}

/// (remove-list-of-text-properties BEG END LIST &optional OBJECT)
pub(crate) fn builtin_remove_list_of_text_properties(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("remove-list-of-text-properties", &args, 3)?;
    expect_max_args("remove-list-of-text-properties", &args, 4)?;
    verify_property_change_read_only(eval, &args, 3)?;
    let names_for_probe = list_names_for_remove(args[2]);
    let change =
        buffer_property_range_for_args(eval, &args, 3)?.and_then(|(buf_id, byte_range)| {
            let buf = eval.buffers.get(buf_id)?;
            buf.text_props_range_has_any_property_named_in_emacs_byte_range(
                byte_range,
                &names_for_probe,
            )
            .then_some((buf_id, byte_range))
        });
    let before = if let Some((buf_id, byte_range)) = change {
        Some(begin_buffer_text_property_change(eval, buf_id, byte_range)?)
    } else {
        None
    };
    let result =
        builtin_remove_list_of_text_properties_in_buffers(&mut eval.buffers, args.clone())?;
    if let Some((saved_current, change)) = before {
        finish_buffer_text_property_change(eval, saved_current, change)?;
    }
    Ok(result)
}

pub(crate) fn builtin_remove_list_of_text_properties_in_buffers(
    buffers: &mut BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("remove-list-of-text-properties", &args, 3)?;
    expect_max_args("remove-list-of-text-properties", &args, 4)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let names = list_names_for_remove(args[2]);

    if let Some(str_val) = is_string_object(args.get(3)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let mut table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let mut changed = false;
        for name in names {
            if table.remove_property_in_char_range(char_range, name) {
                changed = true;
            }
        }
        save_string_props_for_value(str_val, table);
        return Ok(if changed { Value::T } else { Value::NIL });
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(3))?;
    let byte_range = {
        let buf = buffers
            .get(buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
        let Some(range) = validate_buffer_property_range(buf, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        range
    };

    // One char-converted interval walk answers "does any of the names
    // occur in the range" for every name at once. The per-name byte
    // cursor walk this replaces paid two byte<->char conversions plus two
    // tree queries PER interval boundary PER name — font-lock unfontify
    // (`face`+`font-lock-face` over a freshly fontified region) made it
    // the single hottest path in a kill/yank loop.
    let changed = buffers.get(buf_id).is_some_and(|buf| {
        buf.text_props_range_has_any_property_named_in_emacs_byte_range(byte_range, &names)
    });
    if !changed {
        // GNU Fremove_list_of_text_properties: no interval in the range holds
        // any of the names, so return nil without touching the tree (the
        // removal walk would still split the intervals at both range edges).
        return Ok(Value::NIL);
    }
    let _ = buffers.remove_buffer_text_properties_in_emacs_byte_range(buf_id, byte_range, &names);
    let _ = buffers.record_buffer_text_property_modification(buf_id, byte_range);
    Ok(Value::T)
}

/// (text-properties-at POS &optional OBJECT)
pub(crate) fn builtin_text_properties_at(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_text_properties_at_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_text_properties_at_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("text-properties-at", &args, 1)?;
    expect_max_args("text-properties-at", &args, 2)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;

    if let Some(str_val) = is_string_object(args.get(1)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let char_pos = validate_string_char_pos_raw(s, pos, args[0])?;
        if char_pos.get() == s.schars() {
            return Ok(Value::NIL);
        }
        if let Some(table) = get_string_text_properties_table_for_value(str_val) {
            return Ok(table.get_properties_plist_value_at_char_pos(char_pos));
        }
        return Ok(Value::NIL);
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(1))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = validate_buffer_property_point_emacs_byte_pos_raw(buf, pos, args[0])?;
    if byte_pos == buffer_end_emacs_byte_pos(buf) {
        return Ok(Value::NIL);
    }
    Ok(buf.text_props_get_properties_plist_value_at_emacs_byte_pos(byte_pos))
}

/// (next-single-property-change POS PROP &optional OBJECT LIMIT)
pub(crate) fn builtin_next_single_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_single_property_change_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_next_single_property_change_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("next-single-property-change", &args, 2)?;
    expect_max_args("next-single-property-change", &args, 4)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = expect_property_key(&args[1])?;
    let limit = match args.get(3) {
        Some(v) if !v.is_nil() => Some(expect_integer_or_marker_in_buffers(buffers, v)?),
        _ => None,
    };
    // GNU `Fnext_single_property_change`: `here_val = textget (i->plist,
    // prop)`, then `next = next_interval (i)` while `EQ (textget (next->plist,
    // prop), here_val)`.  One `textget` snapshot serves the whole walk
    // (`char-property-alias-alist` / `default-text-properties` read once, not
    // per interval), the intervals are walked in tree order without a
    // per-step descent, and positions are converted once at the ends.
    let resolver = CharPropertyResolver::snapshot(obarray, buffers, prop);
    if let Some(str_val) = is_string_object(args.get(2)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let char_pos = validate_string_char_pos_raw(s, pos, args[0])?;
        let len = CharPos0::new(s.schars());
        let outcome = walk_single_property_change(
            &resolver,
            char_pos,
            len,
            limit,
            |pos| string_char_to_elisp_pos(s, pos),
            |pos, f| table.for_each_interval_from_char_pos(pos, f),
        );
        return Ok(outcome.into_value(limit));
    }
    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(2))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let byte_pos = validate_buffer_property_point_emacs_byte_pos_raw(buf, pos, args[0])?;
    let char_pos = buf.emacs_byte_pos_to_char_pos_clamped(byte_pos);
    let buf_end = buf.emacs_byte_pos_to_char_pos_clamped(buf.accessible_emacs_byte_region().end());
    let outcome = walk_single_property_change(
        &resolver,
        char_pos,
        buf_end,
        limit,
        |pos| pos.to_lisp().as_i64(),
        |pos, f| buf.text_props_for_each_interval_from_char_pos(pos, f),
    );
    Ok(outcome.into_value(limit))
}

/// Where a forward single-property walk stopped.
enum SinglePropertyWalk {
    /// The property changed at this Lisp position.
    ChangedAt(i64),
    /// The walk reached LIMIT (or the object's end / the last interval)
    /// without a change: LIMIT if given, else nil.
    Exhausted,
}

impl SinglePropertyWalk {
    fn into_value(self, limit: Option<i64>) -> Value {
        match self {
            Self::ChangedAt(pos) => Value::fixnum(pos),
            Self::Exhausted => limit.map(Value::fixnum).unwrap_or(Value::NIL),
        }
    }
}

/// The shared walk of `next-single-property-change` over a string's or a
/// buffer's intervals (char positions; `to_elisp` renders one for Lisp).
///
/// The first interval seeds `here_val`; every later interval's START is a
/// boundary: reaching LIMIT or `object_end` there ends the walk, otherwise
/// its resolved value is compared with `eq`.  Text past the last interval has
/// no node but still holds the nil plist GNU's trailing interval would, so it
/// is compared once as a virtual interval starting at the last end.
fn walk_single_property_change<T, W>(
    resolver: &CharPropertyResolver<'_>,
    start_pos: CharPos0,
    object_end: CharPos0,
    limit: Option<i64>,
    to_elisp: T,
    walk: W,
) -> SinglePropertyWalk
where
    T: Fn(CharPos0) -> i64,
    W: FnOnce(CharPos0, &mut dyn FnMut(CharPos0, CharPos0, Value) -> bool),
{
    let resolve = |plist: Value| resolver.resolve_interval_plist(plist).unwrap_or(Value::NIL);
    let mut here_val: Option<Value> = None;
    let mut last_end = start_pos;
    let mut outcome: Option<SinglePropertyWalk> = None;
    // Compare the interval starting at `start` against `here_val`; `Some`
    // ends the walk.
    let step = |start: CharPos0, plist: Value, here_val: Value| -> Option<SinglePropertyWalk> {
        let lisp_pos = to_elisp(start);
        if limit.is_some_and(|lim| lisp_pos >= lim) || start >= object_end {
            return Some(SinglePropertyWalk::Exhausted);
        }
        (!eq_value(&here_val, &resolve(plist))).then_some(SinglePropertyWalk::ChangedAt(lisp_pos))
    };
    walk(start_pos, &mut |start, end, plist| {
        last_end = end;
        match here_val {
            None => {
                here_val = Some(resolve(plist));
                true
            }
            Some(current) => match step(start, plist, current) {
                Some(result) => {
                    outcome = Some(result);
                    false
                }
                None => true,
            },
        }
    });
    if let Some(outcome) = outcome {
        return outcome;
    }
    match here_val {
        Some(current) if last_end < object_end => {
            step(last_end, Value::NIL, current).unwrap_or(SinglePropertyWalk::Exhausted)
        }
        _ => SinglePropertyWalk::Exhausted,
    }
}

/// Byte position of the character immediately preceding `byte_pos`.
///
/// GNU's `previous-single-property-change` inspects the property of the
/// character *before* a position/boundary using a one-*character* step
/// (`position - 1`, since GNU works in character positions).  In a multibyte
/// buffer that character can be several bytes back, so a one-*byte* decrement
/// would land mid-character and trip the Emacs-char-boundary assertion in
/// `emacs_byte_pos_to_char_pos`.  `byte_pos` must already be a character
/// boundary (validated points and interval boundaries always are).
pub(crate) fn emacs_byte_pos_of_preceding_char(
    buf: &Buffer,
    byte_pos: EmacsBytePos,
) -> EmacsBytePos {
    if byte_pos <= EmacsBytePos::ZERO {
        return EmacsBytePos::ZERO;
    }
    let char_pos = buf.emacs_byte_pos_to_char_pos_clamped(byte_pos);
    let prev_char = char_pos.saturating_sub_len(CharLen::new(1));
    EmacsBytePos::new(buffer_char_to_byte_pos(buf, prev_char))
}

/// (previous-single-property-change POS PROP &optional OBJECT LIMIT)
pub(crate) fn builtin_previous_single_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_single_property_change_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_previous_single_property_change_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-single-property-change", &args, 2)?;
    expect_max_args("previous-single-property-change", &args, 4)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = expect_property_key(&args[1])?;

    if let Some(str_val) = is_string_object(args.get(2)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let char_pos = validate_string_char_pos_raw(s, pos, args[0])?;
        let (limit_pos, limit_val) = match args.get(3) {
            Some(v) if !v.is_nil() => {
                let lim_int = expect_integer_or_marker_in_buffers(buffers, v)?;
                (Some(lim_int), Some(lim_int))
            }
            _ => (None, None),
        };
        let ref_char = char_pos.saturating_sub_len(CharLen::new(1));
        let current_val =
            lookup_string_text_property(obarray, buffers, &table, ref_char.get(), prop);
        let mut cursor = char_pos;
        while let Some(prev) = table.previous_interval_boundary_before_char_pos(cursor) {
            if let Some(lim) = limit_pos
                && (prev.get() as i64) <= lim
            {
                return Ok(match limit_val {
                    Some(lv) => Value::fixnum(lv),
                    None => Value::NIL,
                });
            }
            let check = prev.saturating_sub_len(CharLen::new(1));
            let new_val = lookup_string_text_property(obarray, buffers, &table, check.get(), prop);
            let changed = !eq_value(&current_val, &new_val);
            if changed {
                return Ok(Value::fixnum(string_char_to_elisp_pos(s, prev)));
            }
            if prev == CharPos0::ZERO {
                break;
            }
            cursor = if prev < cursor {
                prev
            } else {
                prev.saturating_sub_len(CharLen::new(1))
            };
        }
        return Ok(match limit_val {
            Some(lv) => Value::fixnum(lv),
            None => Value::NIL,
        });
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(2))?;

    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = validate_buffer_property_point_emacs_byte_pos_raw(buf, pos, args[0])?;
    let (limit_pos, limit_val) = match args.get(3) {
        Some(v) if !v.is_nil() => {
            let lim_int = expect_integer_or_marker_in_buffers(buffers, v)?;
            (Some(lim_int), Some(lim_int))
        }
        _ => (Some(buf.point_min_lisp_char_pos().as_i64()), None),
    };

    let ref_byte = emacs_byte_pos_of_preceding_char(buf, byte_pos);
    let current_val = lookup_buffer_text_property(obarray, buffers, buf, ref_byte.get(), prop);
    let mut cursor = byte_pos;

    while let Some(prev) = buf.text_props_previous_interval_boundary_before_emacs_byte_pos(cursor) {
        if let Some(lim) = limit_pos
            && byte_to_elisp_pos(buf, prev) <= lim
        {
            return Ok(match limit_val {
                Some(lv) => Value::fixnum(lv),
                None => Value::NIL,
            });
        }
        let check = emacs_byte_pos_of_preceding_char(buf, prev);
        let new_val = lookup_buffer_text_property(obarray, buffers, buf, check.get(), prop);
        let changed = !eq_value(&current_val, &new_val);
        if changed {
            return Ok(Value::fixnum(byte_to_elisp_pos(buf, prev)));
        }
        if prev == EmacsBytePos::ZERO {
            break;
        }
        cursor = if prev < cursor {
            prev
        } else {
            emacs_byte_pos_of_preceding_char(buf, prev)
        };
    }

    Ok(match limit_val {
        Some(lv) => Value::fixnum(lv),
        None => Value::NIL,
    })
}

/// (next-property-change POS &optional OBJECT LIMIT)
pub(crate) fn builtin_next_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_property_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_next_property_change_in_buffers(
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("next-property-change", &args, 1)?;
    expect_max_args("next-property-change", &args, 3)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;

    if let Some(str_val) = is_string_object(args.get(1)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let table = get_string_text_properties_table_for_value(str_val).unwrap_or_default();
        let char_pos = validate_string_char_pos_raw(s, pos, args[0])?;
        let limit_arg = args.get(2);
        if limit_arg.is_some_and(|v| v.is_t()) {
            let next = table
                .next_interval_boundary_after_char_pos(char_pos)
                .map(|pos| pos.get())
                .unwrap_or_else(|| s.schars());
            return Ok(Value::fixnum(next as i64));
        }
        let (limit_pos, limit_val) = match limit_arg {
            Some(v) if !v.is_nil() => {
                let lim_int = expect_integer_or_marker_in_buffers(buffers, v)?;
                (Some(lim_int), Some(Value::fixnum(lim_int)))
            }
            _ => (None, None),
        };
        let str_char_len = s.schars();
        return match table.next_property_change_after_char_pos(char_pos) {
            Some(next) => {
                let next = next.get();
                if let Some(lim) = limit_pos
                    && (next as i64) >= lim
                {
                    return Ok(limit_val.unwrap_or(Value::NIL));
                }
                // If the change is at or past the end of the string, treat as no change
                if next >= str_char_len {
                    return Ok(limit_val.unwrap_or(Value::NIL));
                }
                Ok(Value::fixnum(string_char_to_elisp_pos(
                    s,
                    string_char_pos(next),
                )))
            }
            None => Ok(limit_val.unwrap_or(Value::NIL)),
        };
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(1))?;
    let limit_arg = args.get(2);

    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let byte_pos = validate_buffer_property_point_emacs_byte_pos_raw(buf, pos, args[0])?;
    if limit_arg.is_some_and(|v| v.is_t()) {
        let next = buf
            .text_props_next_interval_boundary_after_emacs_byte_pos(byte_pos)
            .unwrap_or_else(|| buf.accessible_emacs_byte_region().end());
        return Ok(Value::fixnum(byte_to_elisp_pos(buf, next)));
    }
    let (limit_pos, limit_val) = match limit_arg {
        Some(v) if !v.is_nil() => {
            let lim_int = expect_integer_or_marker_in_buffers(buffers, v)?;
            (Some(lim_int), Some(Value::fixnum(lim_int)))
        }
        _ => (None, None),
    };
    let buf_end = buf.accessible_emacs_byte_region().end();

    match buf.text_props_next_change_after_emacs_byte_pos(byte_pos) {
        Some(next) => {
            if let Some(lim) = limit_pos
                && byte_to_elisp_pos(buf, next) >= lim
            {
                return Ok(limit_val.unwrap_or(Value::NIL));
            }
            // If the change is at or past buffer end, treat as no change
            if next >= buf_end {
                return Ok(limit_val.unwrap_or(Value::NIL));
            }
            Ok(Value::fixnum(byte_to_elisp_pos(buf, next)))
        }
        None => Ok(limit_val.unwrap_or(Value::NIL)),
    }
}

/// (text-property-any BEG END PROP VAL &optional OBJECT)
pub(crate) fn builtin_text_property_any(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_text_property_any_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_text_property_any_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("text-property-any", &args, 4)?;
    expect_max_args("text-property-any", &args, 5)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let prop = expect_property_key(&args[2])?;
    let val = &args[3];

    if let Some(str_val) = is_string_object(args.get(4)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let char_beg = char_range.start().get();
        let char_end = char_range.end().get();
        let Some(table) = get_string_text_properties_interval_table_for_value(str_val) else {
            return Ok(if val.is_nil() {
                if char_beg < char_end {
                    Value::fixnum(string_char_to_elisp_pos(s, string_char_pos(char_beg)))
                } else {
                    Value::NIL
                }
            } else {
                Value::NIL
            });
        };
        if val.is_nil() {
            let mut cursor = char_beg;
            while cursor < char_end {
                let found = lookup_string_text_property(obarray, buffers, &table, cursor, prop);
                if found.is_nil() {
                    return Ok(Value::fixnum(string_char_to_elisp_pos(
                        s,
                        string_char_pos(cursor),
                    )));
                }
                match table.next_interval_boundary_after_char_pos(string_char_pos(cursor)) {
                    Some(next) if next.get() <= char_end => cursor = next.get(),
                    _ => break,
                }
            }
            return Ok(Value::NIL);
        }
        let mut cursor = char_beg;
        while cursor < char_end {
            let found = lookup_string_text_property(obarray, buffers, &table, cursor, prop);
            if eq_value(&found, val) {
                return Ok(Value::fixnum(string_char_to_elisp_pos(
                    s,
                    string_char_pos(cursor),
                )));
            }
            match table.next_interval_boundary_after_char_pos(string_char_pos(cursor)) {
                Some(next) if next.get() > cursor && next.get() <= char_end => cursor = next.get(),
                _ => break,
            }
        }
        return Ok(Value::NIL);
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(4))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) =
        validate_buffer_property_emacs_byte_range(buf, beg, end, args[0], args[1])?
    else {
        return Ok(Value::NIL);
    };
    let byte_beg = byte_range.start();
    let byte_end = byte_range.end();

    if buf.text_props_is_empty() {
        return Ok(if val.is_nil() {
            if byte_beg < byte_end {
                Value::fixnum(byte_to_elisp_pos(buf, byte_beg))
            } else {
                Value::NIL
            }
        } else {
            Value::NIL
        });
    }

    if val.is_nil() {
        let mut cursor = byte_beg;
        while cursor < byte_end {
            let found =
                lookup_buffer_text_property_at_emacs_byte_pos(obarray, buffers, buf, cursor, prop);
            if found.is_nil() {
                return Ok(Value::fixnum(byte_to_elisp_pos(buf, cursor)));
            }
            match buf.text_props_next_interval_boundary_after_emacs_byte_pos(cursor) {
                Some(next) if next <= byte_end => {
                    cursor = next;
                }
                _ => break,
            }
        }
        return Ok(Value::NIL);
    }
    let mut cursor = byte_beg;
    while cursor < byte_end {
        let found =
            lookup_buffer_text_property_at_emacs_byte_pos(obarray, buffers, buf, cursor, prop);
        if eq_value(&found, val) {
            return Ok(Value::fixnum(byte_to_elisp_pos(buf, cursor)));
        }
        match buf.text_props_next_interval_boundary_after_emacs_byte_pos(cursor) {
            Some(next) if next > cursor && next <= byte_end => cursor = next,
            _ => break,
        }
    }
    Ok(Value::NIL)
}

/// (text-property-not-all BEG END PROP VAL &optional OBJECT)
pub(crate) fn builtin_text_property_not_all(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_text_property_not_all_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_text_property_not_all_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("text-property-not-all", &args, 4)?;
    expect_max_args("text-property-not-all", &args, 5)?;
    let beg = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let end = expect_integer_or_marker_in_buffers(buffers, &args[1])?;
    let prop = expect_property_key(&args[2])?;
    let val = &args[3];

    if let Some(str_val) = is_string_object(args.get(4)) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let Some(char_range) = validate_string_range(s, beg, end, args[0], args[1])? else {
            return Ok(Value::NIL);
        };
        let char_beg = char_range.start().get();
        let char_end = char_range.end().get();
        let Some(table) = get_string_text_properties_interval_table_for_value(str_val) else {
            return Ok(if val.is_nil() {
                Value::NIL
            } else if char_beg < char_end {
                Value::fixnum(string_char_to_elisp_pos(s, string_char_pos(char_beg)))
            } else {
                Value::NIL
            });
        };
        let mut cursor = char_beg;
        while cursor < char_end {
            let found = lookup_string_text_property(obarray, buffers, &table, cursor, prop);
            let matches = eq_value(&found, val);
            if !matches {
                return Ok(Value::fixnum(string_char_to_elisp_pos(
                    s,
                    string_char_pos(cursor),
                )));
            }
            match table.next_property_change_after_char_pos(string_char_pos(cursor)) {
                Some(next) if next.get() > cursor && next.get() < char_end => cursor = next.get(),
                _ => break,
            }
        }
        return Ok(Value::NIL);
    }

    let buf_id = resolve_text_property_buffer_id_in_buffers(buffers, args.get(4))?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let Some(byte_range) =
        validate_buffer_property_emacs_byte_range(buf, beg, end, args[0], args[1])?
    else {
        return Ok(Value::NIL);
    };
    let byte_beg = byte_range.start();
    let byte_end = byte_range.end();

    if buf.text_props_is_empty() {
        return Ok(if val.is_nil() {
            Value::NIL
        } else if byte_beg < byte_end {
            Value::fixnum(byte_to_elisp_pos(buf, byte_beg))
        } else {
            Value::NIL
        });
    }

    let mut cursor = byte_beg;

    while cursor < byte_end {
        let found =
            lookup_buffer_text_property_at_emacs_byte_pos(obarray, buffers, buf, cursor, prop);
        let matches = eq_value(&found, val);
        if !matches {
            return Ok(Value::fixnum(byte_to_elisp_pos(buf, cursor)));
        }

        match buf.text_props_next_change_after_emacs_byte_pos(cursor) {
            Some(next) if next > cursor && next < byte_end => cursor = next,
            _ => break,
        }
    }

    Ok(Value::NIL)
}

/// (get-char-property-and-overlay POS PROP &optional OBJECT)
pub(crate) fn builtin_get_char_property_and_overlay(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_char_property_and_overlay_with_frames(
        &eval.obarray,
        &eval.buffers,
        Some(&eval.frames),
        args,
    )
}

pub(crate) fn builtin_get_char_property_and_overlay_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_char_property_and_overlay_with_frames(obarray, buffers, None, args)
}

fn builtin_get_char_property_and_overlay_with_frames(
    obarray: &Obarray,
    buffers: &BufferManager,
    frames: Option<&FrameManager>,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-char-property-and-overlay", &args, 2)?;
    expect_max_args("get-char-property-and-overlay", &args, 3)?;
    let pos = expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = expect_property_key(&args[1])?;

    // For strings, no overlays — just return (text-prop-value . nil)
    if is_string_object(args.get(2)).is_some() {
        let value = builtin_get_text_property_in_state(obarray, buffers, args)?;
        return Ok(Value::cons(value, Value::NIL));
    }

    let (buf_id, window_id) = resolve_char_property_target_in_state(frames, buffers, args.get(2))?;

    if let Some(buf) = buffers.get(buf_id) {
        let byte_pos = validate_buffer_point_emacs_byte_pos_raw(buf, pos, args[0])?;
        if byte_pos == buffer_end_emacs_byte_pos(buf) {
            return Ok(Value::cons(Value::NIL, Value::NIL));
        }
        if let Some((value, ov_val)) = buffer_overlay_property_at_byte_pos(
            obarray,
            buffers,
            buf,
            byte_pos.get(),
            prop,
            window_id,
        ) {
            return Ok(Value::cons(value, ov_val));
        }
    }

    let value = builtin_get_char_property_in_state(obarray, buffers, args)?;
    Ok(Value::cons(value, Value::NIL))
}

/// (get-display-property POS PROP &optional OBJECT PROPERTIES)
pub(crate) fn builtin_get_display_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_display_property_in_state(&eval.obarray, &eval.buffers, args)
}

pub(crate) fn builtin_get_display_property_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-display-property", &args, 2)?;
    expect_max_args("get-display-property", &args, 4)?;
    let prop = expect_property_key(&args[1])?;
    if prop != Value::symbol("display") {
        return Ok(Value::NIL);
    }
    let mut forwarded = vec![args[0], args[1]];
    if let Some(object) = args.get(2) {
        forwarded.push(*object);
    }
    builtin_get_char_property_in_state(obarray, buffers, forwarded)
}

// ===========================================================================
// Overlay builtins
// ===========================================================================

/// (remove-overlays &optional BEG END NAME VAL)
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_remove_overlays(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("remove-overlays", &args, 4)?;
    let buf_id = eval
        .buffers
        .current_buffer()
        .map(|b| b.id)
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;

    let (start_pos, end_pos) = {
        let buf = eval
            .buffers
            .get(buf_id)
            .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
        let start = if args.is_empty() || args[0].is_nil() {
            buf.point_min_emacs_byte_pos()
        } else {
            elisp_pos_to_byte_clipped_full(buf, LispCharPos1::new(expect_int_eval(eval, &args[0])?))
        };
        let end = if args.len() < 2 || args[1].is_nil() {
            buf.point_max_emacs_byte_pos()
        } else {
            elisp_pos_to_byte_clipped_full(buf, LispCharPos1::new(expect_int_eval(eval, &args[1])?))
        };
        (start, end)
    };

    let buf = eval
        .buffers
        .get_mut(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    let filter_name = if args.len() >= 3 && !args[2].is_nil() {
        Some(expect_property_key(&args[2])?)
    } else {
        None
    };

    let filter_val = if args.len() >= 4 && !args[3].is_nil() {
        Some(args[3])
    } else {
        None
    };

    // Collect overlay ids in range.
    let accessible = buf.accessible_emacs_byte_region();
    let ids = buf.overlays.overlays_in_accessible_emacs_byte_range(
        EmacsByteRange::new(start_pos, end_pos),
        accessible.end(),
    );

    // Filter and delete.
    for overlay in ids {
        let should_delete = match (&filter_name, &filter_val) {
            (Some(name), Some(val)) => buf
                .overlays
                .overlay_get(overlay, name)
                .is_some_and(|v| equal_value(&v, val, 0)),
            (Some(name), None) => buf.overlays.overlay_get(overlay, name).is_some(),
            _ => true,
        };
        if should_delete {
            buf.overlays.delete_overlay(overlay);
        }
    }

    Ok(Value::NIL)
}

// ===========================================================================
// Tests
// ===========================================================================
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
