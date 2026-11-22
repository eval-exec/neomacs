//! Composition builtins (complex script rendering).
//!
//! GNU Emacs records explicit compositions as a `composition` text property.
//! The display engine later validates and registers those properties when it
//! needs glyph data.  The Lisp-visible mutation semantics live here.

use super::chartable::make_char_table_value;
use crate::emacs_core::text_host::TextHost;
use neomacs_display_protocol::glyph_matrix::{TerminalComposition, TerminalCompositionCell};
use neovm_edit::buffer::{CharLen, CharPos0, CharRange, EmacsByteRange};
use neovm_value::error::LispCondition;
use neovm_value::error::{EvalResult, Flow, signal};
use neovm_value::error::{expect_args, expect_args_range, expect_max_args};
use neovm_value::value::ValueKind;
use neovm_value::value::scratch_bridge::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use neovm_value::value::*;
use unicode_general_category::GeneralCategory;

pub use neovm_edit::buffer::composition::{
    AutomaticCompositionSpan, BYTES_SCANNED, MAX_AUTO_COMPOSITION_LOOKBACK, fast_path,
};

/// Install the substrate's static composition seams (stage-3 C7).  Idempotent;
/// called at evaluator construction and by the text-side entry points so tests
/// that scan buffers without a live evaluator still get the queries.
#[doc(hidden)]
pub fn install_substrate_seams() {
    // Fast path: these entry points run once per window scan, so avoid a
    // `OnceLock::set` RMW per call.  The lock contents are still read through
    // `OnceLock::get`, which synchronizes with the setter on its own.
    static INSTALLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if INSTALLED.load(std::sync::atomic::Ordering::Acquire) {
        return;
    }
    neovm_edit::buffer::composition::install(
        neovm_edit::buffer::composition::CompositionSeams::new(
            automatic_composition_spans_in_with_symbols,
            ascii_has_no_composition_rules_with_symbols,
            composition_prop_symbol,
            composition_width_for_property,
        ),
    );
    INSTALLED.store(true, std::sync::atomic::Ordering::Release);
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

fn expect_integer_or_marker_p(arg: &Value) -> Result<(), Flow> {
    match arg.kind() {
        ValueKind::Fixnum(_) => Ok(()),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("integer-or-marker-p"), *arg],
        )),
    }
}

fn integer_value(arg: &Value) -> i64 {
    match arg.kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    }
}

fn expect_composition_components(arg: Value) -> Result<(), Flow> {
    if arg.is_nil() || arg.is_fixnum() || arg.is_cons() || arg.is_string() || arg.is_vector() {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), arg],
        ))
    }
}

fn composition_property(
    start: i64,
    end: i64,
    components: Value,
    modification_func: Value,
) -> Value {
    Value::cons(
        Value::cons(Value::fixnum(end - start), components),
        modification_func,
    )
}

fn expect_string_value(arg: &Value) -> Result<&neovm_value::heap_types::LispString, Flow> {
    arg.as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *arg],
        )
    })
}

fn validate_subarray_indices(
    array: Value,
    from: Value,
    to: Value,
    size: i64,
) -> Result<(i64, i64), Flow> {
    fn normalize_index(value: Value, default: i64, size: i64) -> Result<i64, Flow> {
        if value.is_nil() {
            return Ok(default);
        }
        let raw = match value.kind() {
            ValueKind::Fixnum(n) => n,
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("integerp"), value],
                ));
            }
        };
        Ok(if raw < 0 { raw + size } else { raw })
    }

    let from_idx = normalize_index(from, 0, size)?;
    let to_idx = normalize_index(to, size, size)?;
    if !(0 <= from_idx && from_idx <= to_idx && to_idx <= size) {
        return Err(signal(LispCondition::ArgsOutOfRange, vec![array, from, to]));
    }
    Ok((from_idx, to_idx))
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// Context-backed `(compose-region-internal START END &optional COMPONENTS MODIFICATION-FUNC)`.
#[doc(hidden)]
pub fn compose_region_internal<H: TextHost>(ctx: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args_range("compose-region-internal", &args, 2, 4)?;
    // GNU `Fcompose_region_internal` runs `validate_region (&start, &end)`
    // (buffer.c), which resolves markers, swaps so the lower bound comes first,
    // then bounds-checks against `BEGV`/`ZV`.
    let mut beg = neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(
        ctx.buffers(),
        &args[0],
    )?;
    let mut end = neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(
        ctx.buffers(),
        &args[1],
    )?;
    if end < beg {
        std::mem::swap(&mut beg, &mut end);
    }
    let components = args.get(2).copied().unwrap_or(Value::NIL);
    let modification_func = args.get(3).copied().unwrap_or(Value::NIL);
    expect_composition_components(components)?;

    let (buffer_handle, point_min, point_max) = if let Some(buf) = ctx.buffers().current_buffer() {
        (
            Value::make_buffer(buf.id()),
            buf.point_min_lisp_char_pos().as_i64(),
            buf.point_max_lisp_char_pos().as_i64(),
        )
    } else {
        (Value::NIL, 1, 1)
    };
    if beg < point_min || end > point_max {
        // GNU `args_out_of_range_3` reports the original (un-swapped) arguments.
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![buffer_handle, args[0], args[1]],
        ));
    }

    let prop = composition_property(beg, end, components, modification_func);
    neovm_edit::emacs_core::textprop::builtin_put_text_property(
        ctx,
        vec![
            Value::fixnum(beg),
            Value::fixnum(end),
            Value::symbol("composition"),
            prop,
            Value::NIL,
        ],
    )?;

    Ok(Value::NIL)
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
/// `(compose-string-internal STRING START END &optional COMPONENTS MODIFICATION-FUNC)`
///
/// Compose text in STRING between indices START and END.
pub(crate) fn compose_string_internal(args: Vec<Value>) -> EvalResult {
    expect_args_range("compose-string-internal", &args, 3, 5)?;
    if !args[0].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        ));
    }
    let components = args.get(3).copied().unwrap_or(Value::NIL);
    let modification_func = args.get(4).copied().unwrap_or(Value::NIL);

    let len = expect_string_value(&args[0])?.schars() as i64;
    let (start, end) = validate_subarray_indices(args[0], args[1], args[2], len)?;
    let char_start = usize::try_from(start).expect("validated non-negative string start");
    let char_end = usize::try_from(end).expect("validated non-negative string end");
    let char_len = usize::try_from(len).expect("string character length fits usize");
    let prop = composition_property(start, end, components, modification_func);
    let mut table = get_string_text_properties_table_for_value(args[0]).unwrap_or_default();
    table.put_property_for_object_char_len(
        CharRange::new(CharPos0::new(char_start), CharPos0::new(char_end)),
        CharLen::new(char_len),
        Value::symbol("composition"),
        prop,
    );
    neovm_edit::emacs_core::textprop::save_string_props_for_value(args[0], table);

    Ok(args[0])
}

thread_local! {
    /// Mirrors GNU's `composition_hash_table` (dedup: component chars -> id) plus
    /// the `relative_p` slice of `composition_table` (id -> method). The id
    /// counter is GNU's `n_compositions`. Keyed on the component char codes so
    /// there is no GC interaction; `relative_by_id[id]` lets a later
    /// `find-composition`/decode of a registered (Form-B) property recover the
    /// `relative-p` it can no longer infer from the bare components vector.
    static COMPOSITION_REGISTRY: std::cell::RefCell<CompositionRegistry> =
        std::cell::RefCell::new(CompositionRegistry {
            next_id: 0,
            dedup: std::collections::HashMap::new(),
            relative_by_id: Vec::new(),
        });
}

struct CompositionRegistry {
    next_id: i64,
    dedup: std::collections::HashMap<Vec<i64>, i64>,
    relative_by_id: Vec<bool>,
}

/// The component char codes of a key vector, or None if any element is not a
/// fixnum (rule-based components carrying cons rules — not deduped, like a
/// distinct GNU registration).
fn composition_key_codes(key: &Value) -> Option<Vec<i64>> {
    let items = key.as_vector_data()?;
    let mut codes = Vec::with_capacity(items.len());
    for item in items.iter() {
        codes.push(item.as_fixnum()?);
    }
    Some(codes)
}

/// GNU `get_composition_id` id assignment: reuse the id of an identical
/// composition (same component chars) else allocate the next id, recording its
/// `relative-p` (method) for later decode.
fn composition_assign_id(key: &Value, relative_p: bool) -> i64 {
    let codes = composition_key_codes(key);
    COMPOSITION_REGISTRY.with(|reg| {
        let mut reg = reg.borrow_mut();
        if let Some(codes) = &codes
            && let Some(&id) = reg.dedup.get(codes)
        {
            return id;
        }
        let id = reg.next_id;
        reg.next_id += 1;
        if let Some(codes) = codes {
            reg.dedup.insert(codes, id);
        }
        reg.relative_by_id.push(relative_p);
        id
    })
}

fn composition_lookup_relative(id: i64) -> bool {
    COMPOSITION_REGISTRY.with(|reg| {
        reg.borrow()
            .relative_by_id
            .get(id as usize)
            .copied()
            .unwrap_or(true)
    })
}

/// Decode either composition form into `(length, components-or-vec, mod-func,
/// registered-id)`. `registered-id` is `Some` for Form-B (then the second value
/// is the components vector), `None` for Form-A (the raw components).
fn composition_parts_any(prop: Value) -> Option<(i64, Value, Value, Option<i64>)> {
    if !prop.is_cons() {
        return None;
    }
    let head = prop.cons_car();
    if let Some(id) = head.as_fixnum() {
        // Form-B: (ID . (LENGTH COMPONENTS-VEC . MOD)).
        let rest = prop.cons_cdr();
        if !rest.is_cons() {
            return None;
        }
        let length = rest.cons_car().as_fixnum()?;
        let after = rest.cons_cdr();
        if !after.is_cons() {
            return None;
        }
        Some((length, after.cons_car(), after.cons_cdr(), Some(id)))
    } else if head.is_cons() {
        // Form-A: ((LENGTH . COMPONENTS) . MOD).
        let length = head.cons_car().as_fixnum()?;
        Some((length, head.cons_cdr(), prop.cons_cdr(), None))
    } else {
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompositionDisplayText {
    text: String,
    char_len: usize,
}

impl CompositionDisplayText {
    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn char_len(&self) -> usize {
        self.char_len
    }
}

fn composition_char_from_code(code: i64) -> Option<char> {
    u32::try_from(code).ok().and_then(char::from_u32)
}

/// A component in GNU's static composition vector.
///
/// TAB is structural: it requests left/right padding around an adjacent
/// glyph, but is never itself emitted (`term.c:615`, `xdisp.c:30837`).  Keeping
/// it distinct from a drawable character prevents a later display consumer
/// from accidentally expanding it as an ordinary terminal tab.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CompositionDisplayComponent {
    Glyph(char),
    PaddingMarker,
}

impl CompositionDisplayComponent {
    fn from_code(code: i64) -> Option<Self> {
        let ch = composition_char_from_code(code)?;
        Some(if ch == '\t' {
            Self::PaddingMarker
        } else {
            Self::Glyph(ch)
        })
    }

    fn append_visible_text(self, text: &mut String) {
        if let Self::Glyph(ch) = self {
            text.push(ch);
        }
    }
}

fn composition_chars_display_text(chars: impl IntoIterator<Item = char>) -> String {
    let mut text = String::new();
    for ch in chars {
        let component = if ch == '\t' {
            CompositionDisplayComponent::PaddingMarker
        } else {
            CompositionDisplayComponent::Glyph(ch)
        };
        component.append_visible_text(&mut text);
    }
    text
}

fn composition_components_display_text(components: Value, relative_p: bool) -> Option<String> {
    match components.kind() {
        ValueKind::Fixnum(code) => CompositionDisplayComponent::from_code(code).map(|component| {
            let mut text = String::new();
            component.append_visible_text(&mut text);
            text
        }),
        ValueKind::String => components
            .as_runtime_string_owned()
            .map(|components| composition_chars_display_text(components.chars())),
        ValueKind::Cons => {
            let items = list_to_vec(&components)?;
            composition_items_display_text(&items, relative_p)
        }
        _ if components.is_vector() => {
            let items = components.as_vector_data()?;
            composition_items_display_text(items, relative_p)
        }
        _ => None,
    }
}

fn composition_items_display_text(items: &[Value], relative_p: bool) -> Option<String> {
    if items.is_empty() {
        return Some(String::new());
    }
    let glyphs: Vec<Value> = if relative_p {
        items.to_vec()
    } else {
        if items.len().is_multiple_of(2) {
            return None;
        }
        items.iter().step_by(2).copied().collect()
    };
    let mut text = String::new();
    for item in glyphs {
        let code = item.as_fixnum()?;
        CompositionDisplayComponent::from_code(code)?.append_visible_text(&mut text);
    }
    Some(text)
}

/// Decode the text a static `composition` property should display.
///
/// GNU redisplay drives this through `composition_it`/gstrings.  The Rust
/// display source uses this narrower value-level helper to avoid falling back
/// to the underlying raw buffer character for explicit replacement
/// compositions such as org-superstar and prettify-symbols.
pub fn composition_display_text_for_property(prop: Value) -> Option<CompositionDisplayText> {
    let (length, components, _, registered) = composition_parts_any(prop)?;
    if length <= 0 {
        return None;
    }
    let relative_p = registered
        .map(composition_lookup_relative)
        .unwrap_or_else(|| composition_relative_p(components));
    Some(CompositionDisplayText {
        text: composition_components_display_text(components, relative_p)?,
        char_len: usize::try_from(length).ok()?,
    })
}

/// GNU `get_composition_id`: register a Form-A composition and rewrite the
/// (shared) property cons in place to Form-B `(ID LENGTH COMPONENTS-VEC . MOD)`.
/// Direct car/cdr mutation matches GNU's `XSETCAR`/`XSETCDR` — it upgrades the
/// stored property without re-running put-text-property or touching
/// buffer-modified-p. Returns the composition id.
fn composition_register_prop(
    prop: Value,
    key: Value,
    length: i64,
    mod_func: Value,
    relative_p: bool,
) -> i64 {
    let id = composition_assign_id(&key, relative_p);
    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(key);
    push_scratch_gc_root(mod_func);
    let new_cdr = Value::cons(Value::fixnum(length), Value::cons(key, mod_func));
    prop.set_car(Value::fixnum(id));
    prop.set_cdr(new_cdr);
    restore_scratch_gc_roots(saved);
    id
}

/// The `composition` property symbol, interned once.
///
/// GNU keeps this as the staticpro'd `Qcomposition`. `composition_width_for_property`
/// runs once per display stop of every column/screen-line scan (via
/// `indent::composition_run_at`), so `Value::symbol("composition")` re-hashed
/// the name on each probe -- one of the two ~2M-per-ELB-`scroll` intern
/// sources. Caches the `SymId` rather than the `Value`, matching
/// `cached_symbol_id!` in `runtime/eval` and the `syntax-table` symbol in
/// `text/syntax`: an id is a plain index, so it cannot be invalidated by GC
/// the way a cached pointer could. Threading: the id resolves once from the
/// process-global symbol registry via `OnceLock` and reads lock-free; no
/// Lisp state is cached.
#[inline(always)]
fn composition_prop_symbol() -> Value {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    Value::symbol(*SYMBOL.get_or_init(|| neovm_value::intern::intern("composition")))
}

/// Display width and character length of the composition whose `composition`
/// text property begins at 1-based buffer position `charpos1`, or None if there
/// is no valid composition there. This is GNU's `get_composition_id` as called
/// from `current_column_1`: it returns the composed glyphs' width over the
/// covered characters AND, the first time the composition is seen, registers it
/// — rewriting the property from Form-A to Form-B in place.
pub(crate) fn composition_width_for_property(
    prop: Value,
    buffers: &neovm_edit::buffer::BufferManager,
    charpos1: i64,
) -> Option<(i64, i64)> {
    install_substrate_seams();
    let (length, components, mod_func, registered) = composition_parts_any(prop)?;
    if length <= 0 {
        return None;
    }
    if let Some(id) = registered {
        // Already Form-B: `components` is the registered components vector.
        let width = if composition_lookup_relative(id) {
            composition_relative_width(&components)
        } else {
            composition_rule_based_width(&components)
        };
        return Some((width, length));
    }
    // Form-A: compute the width via `get_composition_id`, then register
    // (rewrite in place to Form-B). An invalid composition yields id=-1 and is
    // not treated as a composition here.
    let key = composition_components_key(buffers, components, Value::NIL, charpos1, length);
    let width = composition_get_id_width(components, &key)?;
    let relative_p = composition_relative_p(components);
    composition_register_prop(prop, key, length, mod_func, relative_p);
    Some((width, length))
}

/// GNU `composition_valid_p` restricted to the unregistered form: PROP is a
/// well-formed composition property whose stored length equals `end - start`.
fn composition_valid_unregistered(start: i64, end: i64, prop: Value) -> bool {
    let Some((length, components, _, registered)) = composition_parts_any(prop) else {
        return false;
    };
    if length != end - start {
        return false;
    }
    if registered.is_some() {
        // Form-B: components is the registered components vector.
        return true;
    }
    components.is_nil()
        || components.is_string()
        || components.is_vector()
        || components.is_fixnum()
        || components.is_cons()
}

/// GNU `composition_method`: relative unless the components describe explicit
/// composition rules. `find-composition` reports `relative-p` as nil only for
/// `COMPOSITION_WITH_RULE_ALTCHARS` (vector/list components); nil/char/string
/// components are relative.
fn composition_relative_p(components: Value) -> bool {
    components.is_nil() || components.is_fixnum() || components.is_string()
}

/// GNU `get_composition_id` key derivation: the components vector returned by
/// `find-composition`. A single char becomes `[char]`; a string or list is
/// `vconcat`-ed into a char vector; a vector is used as-is; nil takes the chars
/// of the composed range from the buffer (or STRING).
fn composition_components_key(
    buffers: &neovm_edit::buffer::BufferManager,
    components: Value,
    string: Value,
    start: i64,
    nchars: i64,
) -> Value {
    match components.kind() {
        ValueKind::Fixnum(code) => Value::vector(vec![Value::fixnum(code)]),
        ValueKind::String => {
            let codes = neovm_value::strings_core::lisp_string_char_codes(
                components.as_lisp_string().expect("string"),
            );
            Value::vector(codes.into_iter().map(|c| Value::fixnum(c as i64)).collect())
        }
        ValueKind::Cons => Value::vector(list_to_vec(&components).unwrap_or_default()),
        _ if components.is_vector() => components,
        _ => {
            // nil components: take the chars of the composed range.
            let codes: Vec<u32> = if let Some(text) = string.as_lisp_string() {
                let all = neovm_value::strings_core::lisp_string_char_codes(text);
                let from = start.max(0) as usize;
                let to = ((start + nchars).max(0) as usize).min(all.len());
                all.get(from..to).map(|s| s.to_vec()).unwrap_or_default()
            } else if let Some(buf) = buffers.current_buffer() {
                let byte_start = buf
                    .char_pos_to_emacs_byte_pos_clamped(CharPos0::new((start - 1).max(0) as usize));
                let byte_end = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(
                    (start - 1 + nchars).max(0) as usize,
                ));
                let sub = buf
                    .buffer_substring_lisp_string_range(EmacsByteRange::new(byte_start, byte_end));
                neovm_value::strings_core::lisp_string_char_codes(&sub)
            } else {
                Vec::new()
            };
            Value::vector(codes.into_iter().map(|c| Value::fixnum(c as i64)).collect())
        }
    }
}

/// GNU `CHARACTER_WIDTH` (buffer.h): display columns of a single character. A
/// TAB inside a composition counts as 1 (see `get_composition_id`).
fn composition_char_width(code: i64) -> i64 {
    if code == 9 {
        1
    } else {
        crate::encoding::char_width_for_code_with_display_table(code, None) as i64
    }
}

/// GNU relative/altchars width (`get_composition_id`, the
/// `method != COMPOSITION_WITH_RULE_ALTCHARS` branch): the maximum display
/// width over the component glyphs (TAB counts as 1), 0 for an empty
/// composition.
fn composition_relative_width(key: &Value) -> i64 {
    let Some(items) = key.as_vector_data() else {
        return 0;
    };
    let mut width = 0i64;
    for item in items.iter() {
        if let ValueKind::Fixnum(code) = item.kind() {
            let this = composition_char_width(code);
            if width < this {
                width = this;
            }
        }
    }
    width
}

/// GNU rule-based width (`get_composition_id`, the
/// `COMPOSITION_WITH_RULE_ALTCHARS` branch, composite.c ~L344-390): walk the
/// `char rule char rule ...` key computing the leftmost/rightmost overlap
/// geometry from each rule's decoded reference points, then take the ceiling of
/// `rightmost - leftmost`. `glyph_len = (ASIZE(key)+1)/2` and the loop bound is
/// `i < glyph_len` over the key indices, exactly as GNU writes it.
fn composition_rule_based_width(key: &Value) -> i64 {
    let Some(items) = key.as_vector_data() else {
        return 0;
    };
    let codes: Vec<i64> = items.iter().map(|v| v.as_fixnum().unwrap_or(0)).collect();
    if codes.is_empty() {
        return 0;
    }
    let glyph_len = codes.len().div_ceil(2);
    let mut leftmost = 0.0_f64;
    let ch0 = codes[0];
    let mut rightmost = if ch0 != b'\t' as i64 {
        composition_char_width(ch0) as f64
    } else {
        1.0
    };
    let mut i = 1usize;
    while i < glyph_len {
        let rule = codes[i];
        let ch = codes[i + 1];
        let this_width = if ch != b'\t' as i64 {
            composition_char_width(ch) as f64
        } else {
            1.0
        };
        // COMPOSITION_DECODE_REFS (composite.h): gref = (rule & 0xFF) / 12,
        // nref = (rule & 0xFF) % 12.
        let rule_code = rule & 0xFF;
        let mut gref = rule_code / 12;
        if gref > 12 {
            gref = 11;
        }
        let nref = rule_code % 12;
        let this_left = leftmost + (gref % 3) as f64 * (rightmost - leftmost) / 2.0
            - (nref % 3) as f64 * this_width / 2.0;
        if this_left < leftmost {
            leftmost = this_left;
        }
        if this_left + this_width > rightmost {
            rightmost = this_left + this_width;
        }
        i += 2;
    }
    // GNU truncates `rightmost - leftmost` to an int, then adds 1 if the
    // truncation lost a fractional part — i.e. the ceiling.
    (rightmost - leftmost).ceil() as i64
}

/// GNU `get_composition_id` validity + width. `components` is the raw Form-A
/// components used to derive the method; `key` is the derived component vector.
/// Returns the composition width, or `None` when `get_composition_id` would
/// return -1 (an invalid composition), in which case `Ffind_composition_internal`
/// reports only `(FROM TO)`.
fn composition_get_id_width(components: Value, key: &Value) -> Option<i64> {
    let items = key.as_vector_data()?;
    let rule_based = !composition_relative_p(components);
    // GNU `get_composition_id` validates COMPONENTS that are vectors or lists.
    // A glyph-string (a vector whose first element is itself a vector) takes a
    // separate branch with no odd-length requirement; everything else
    // (rule/altchars vectors and lists) requires an odd-length, all-fixnum key.
    let is_glyph_string = components
        .as_vector_data()
        .is_some_and(|c| c.len() >= 2 && c[0].is_vector());
    if is_glyph_string {
        // Each composed glyph element must be a vector.
        if items.iter().skip(1).any(|v| !v.is_vector()) {
            return None;
        }
    } else if components.is_vector() || components.is_cons() {
        if items.len() % 2 == 0 {
            return None;
        }
        if items.iter().any(|v| !v.is_fixnum()) {
            return None;
        }
    }
    Some(if rule_based {
        composition_rule_based_width(key)
    } else {
        composition_relative_width(key)
    })
}

/// GNU `find_composition`/`get_property_and_range` for a buffer: the
/// `composition` property covering `from` (1-based), else the nearest one
/// toward `to` (-1 = none). Returns `(start, end, prop)` in 1-based positions.
fn find_composition_in_buffer(
    buf: &neovm_edit::buffer::buffer::Buffer,
    begv: i64,
    zv: i64,
    from: i64,
    to: i64,
    comp: Value,
) -> Option<(i64, i64, Value)> {
    let run_at = |charpos: i64| -> Option<(i64, i64, Value)> {
        if charpos < begv || charpos >= zv {
            return None;
        }
        let (prop, s, e) =
            buf.get_property_run_at_char_pos(CharPos0::new((charpos - 1) as usize), comp);
        match prop {
            Some(p) if !p.is_nil() => Some((s.get() as i64 + 1, e.get() as i64 + 1, p)),
            _ => None,
        }
    };
    if let Some(found) = run_at(from) {
        return Some(found);
    }
    if to < 0 || to == from {
        return None;
    }
    if to > from {
        // Forward: jump run by run until a composition appears before `to`.
        let mut pos = from;
        while pos < to {
            let (_p, _s, e) =
                buf.get_property_run_at_char_pos(CharPos0::new((pos - 1) as usize), comp);
            let next = e.get() as i64 + 1;
            if next <= pos || next >= to {
                return None;
            }
            if let Some(found) = run_at(next) {
                return Some(found);
            }
            pos = next;
        }
        None
    } else {
        // Backward: GNU checks the char before `from`, then scans backward.
        if let Some(found) = run_at(from - 1) {
            return Some(found);
        }
        let mut pos = from - 1;
        while pos > to {
            let (_p, s, _e) =
                buf.get_property_run_at_char_pos(CharPos0::new((pos - 1) as usize), comp);
            let prev = s.get() as i64; // 1-based position one before this run's start
            if prev <= to || prev >= pos {
                return None;
            }
            if let Some(found) = run_at(prev) {
                return Some(found);
            }
            pos = prev;
        }
        None
    }
}

/// How a caller bounds GNU's automatic-composition search.
///
/// `find_automatic_composition' gives negative, forward, and backward limits
/// different meanings.  Naming those states keeps the signed sentinel out of
/// the range-selection logic below.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AutomaticCompositionQuery {
    Covering { pos: usize },
    Forward { pos: usize, limit: usize },
    Backward { pos: usize, limit: usize },
}

impl AutomaticCompositionQuery {
    fn new(pos: usize, limit: i64) -> Self {
        if limit < 0 || limit as usize == pos {
            Self::Covering { pos }
        } else if limit as usize > pos {
            Self::Forward {
                pos,
                limit: limit as usize,
            }
        } else {
            Self::Backward {
                pos,
                limit: limit as usize,
            }
        }
    }

    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    fn accepts(self, range: CharRange) -> bool {
        let start = range.start().get();
        let end = range.end().get();
        match self {
            Self::Covering { pos } => start <= pos && pos < end,
            Self::Forward { pos, limit } => pos < end && start < limit,
            Self::Backward { pos, limit } => limit < end && start < pos,
        }
    }

    fn select(self, ranges: Vec<CharRange>) -> Option<CharRange> {
        match self {
            Self::Backward { .. } => ranges.into_iter().rev().find(|range| self.accepts(*range)),
            _ => ranges.into_iter().find(|range| self.accepts(*range)),
        }
    }
}

/// Distinguish stored `composition' properties from display-driven automatic
/// compositions.  The former must pass Form-A/Form-B validation and may be
/// registered; the latter already carries the glyph string returned to Lisp.
enum LocatedComposition {
    Stored {
        start: i64,
        end: i64,
        property: Value,
    },
    Automatic {
        start: i64,
        end: i64,
        gstring: Value,
    },
}

/// Select the non-overlapping automatic-composition spans in `text` from the
/// live Lisp rule table.
///
/// This is the forward-search core of GNU `find_automatic_composition`
/// (`src/composite.c`).  Rules are attached to a trigger character, carry an
/// explicit lookback, and are tried in list order.  Once a rule matches, the
/// next search begins after the complete match so a later rule cannot form the
/// partially overlapping composition GNU rejects.
/// Take and reset [`BYTES_SCANNED`].
pub fn take_bytes_scanned() -> usize {
    BYTES_SCANNED.swap(0, std::sync::atomic::Ordering::Relaxed)
}

pub fn automatic_composition_spans(
    buffer: &neovm_edit::buffer::Buffer,
    composition_function_table: Value,
    text: &str,
) -> Vec<AutomaticCompositionSpan> {
    install_substrate_seams();
    automatic_composition_spans_in(buffer, composition_function_table, text, 0)
}

pub(crate) fn ascii_has_no_composition_rules_with_symbols(
    table: Value,
    mode: neovm_value::tagged::SymbolMode,
) -> bool {
    install_substrate_seams();
    if !super::chartable::is_char_table(&table) {
        return true;
    }
    (0..128).all(|ch| {
        super::chartable::ct_lookup_with_symbols(&table, ch, mode)
            .unwrap_or(Value::NIL)
            .is_nil()
    })
}

/// The rule table and buffer-owned regexp classification used by GNU's
/// `fast_looking_at`, even when the displayed object is a Lisp string.
///
/// Like other redisplay inputs these are references to live, rooted Lisp
/// tables, not a persistent cache. Keep this value within its evaluator's
/// redisplay transaction; it does not execute Lisp callbacks or own GC roots.
#[derive(Clone, Copy, Debug)]
pub struct AutomaticCompositionRules {
    table: Value,
    syntax: super::syntax::SyntaxTable,
    category: Option<Value>,
    symbol_mode: neovm_value::tagged::SymbolMode,
}

impl AutomaticCompositionRules {
    pub fn new(buffer: &neovm_edit::buffer::Buffer, table: Value) -> Option<Self> {
        Self::new_with_symbols(buffer, table, neovm_value::tagged::SymbolMode::Bare)
    }

    pub fn new_with_symbols(
        buffer: &neovm_edit::buffer::Buffer,
        table: Value,
        symbol_mode: neovm_value::tagged::SymbolMode,
    ) -> Option<Self> {
        super::chartable::is_char_table(&table).then(|| Self {
            table,
            symbol_mode,
            syntax: super::syntax::SyntaxTable::for_buffer(buffer),
            category: super::category::active_category_table_for_buffer(Some(buffer))
                .ok()
                .filter(|table| !table.is_nil()),
        })
    }

    /// Enumerate the original Lisp tables this view can read after a callback.
    /// The evaluator-side owner supplies the lifetime lease; enumeration does
    /// not consult the buffer again, execute Lisp, or traverse table contents.
    pub fn gc_roots(&self) -> impl Iterator<Item = Value> + '_ {
        [Some(self.table), Some(self.syntax.chartable), self.category]
            .into_iter()
            .flatten()
    }

    pub fn spans(self, text: &str) -> Vec<AutomaticCompositionSpan> {
        self.spans_in(text, 0)
    }

    fn spans_in(self, text: &str, char_offset: usize) -> Vec<AutomaticCompositionSpan> {
        let syntax = super::regex_emacs::BufferSyntaxLookup {
            syntax_table: self.syntax,
            category_table: self.category,
            word_boundary: super::regex_emacs::WordBoundaryLookup::default()
                .with_symbol_mode(self.symbol_mode),
        };
        select_automatic_composition_spans(self.table, &syntax, text, char_offset)
    }

    /// GNU `find_automatic_composition` over a text slice.
    ///
    /// `pos` and `limit` are character indices into `text`; `None` for the
    /// limit is GNU's negative LIMIT ("at POS").  `head` is the safe rewind
    /// bound (the first character after the previous newline, or 0 for a
    /// string).  `shape` builds and shapes one candidate composition;
    /// returning `None` is GNU's `autocmp_chars` nil.  The returned span is in
    /// slice character coordinates.
    ///
    /// The rewind/scan loop and its "last examined rule" result follow
    /// src/composite.c:1641-1875: a composable POS is rewound to the start of
    /// its composable run, rules are gated by `check_pos >= head` and by the
    /// LIMIT side, and a scan that finds no covering composition returns the
    /// result of the last rule it examined (nil when that rule failed).
    pub(crate) fn find_automatic_composition(
        self,
        text: &str,
        pos: usize,
        limit: Option<i64>,
        head: usize,
        shape: &mut dyn FnMut(usize, usize, Value) -> Option<Value>,
    ) -> Option<(usize, usize, Value)> {
        if text.is_empty() {
            return None;
        }
        let mut byte_offsets = Vec::with_capacity(text.len() + 1);
        byte_offsets.extend(text.char_indices().map(|(offset, _)| offset));
        byte_offsets.push(text.len());
        let char_count = byte_offsets.len() - 1;
        if pos >= char_count || head > pos {
            return None;
        }
        let char_at = |index: usize| {
            text[byte_offsets[index]..]
                .chars()
                .next()
                .expect("character index is inside the scanned text")
        };
        let syntax = super::regex_emacs::BufferSyntaxLookup {
            syntax_table: self.syntax,
            category_table: self.category,
            word_boundary: super::regex_emacs::WordBoundaryLookup::default()
                .with_symbol_mode(self.symbol_mode),
        };

        let limit = match limit {
            Some(limit) => limit.min(char_count as i64),
            None => pos as i64,
        };
        let tail = char_count;
        let mut fore_check_limit = if limit <= pos as i64 {
            (pos + 1 + MAX_AUTO_COMPOSITION_LOOKBACK).min(tail)
        } else {
            ((limit.max(0) as usize) + MAX_AUTO_COMPOSITION_LOOKBACK).min(tail)
        };
        let mut cur = pos;
        loop {
            let mut skip_rewind = false;
            if !char_composable_p(char_at(cur)) {
                if limit <= pos as i64 {
                    // Case (1): back up to the previous composable character.
                    loop {
                        if cur as i64 <= limit || cur == 0 {
                            return None;
                        }
                        cur -= 1;
                        if char_composable_p(char_at(cur)) {
                            break;
                        }
                    }
                    fore_check_limit = (cur + 1).min(tail);
                } else {
                    // Case (2): no rewinding; search forward from POS.
                    skip_rewind = true;
                }
            }
            if !skip_rewind {
                // Rewind to the start of the current composable run.
                while head < cur {
                    let previous = cur;
                    cur -= 1;
                    if !char_composable_p(char_at(cur)) {
                        cur = previous;
                        break;
                    }
                }
            }

            let scan_start = cur;
            let mut last: Option<(usize, usize, Value)> = None;
            while cur < fore_check_limit {
                let trigger = cur;
                let rules = super::chartable::ct_lookup_with_symbols(
                    &self.table,
                    i64::from(char_at(trigger) as u32),
                    self.symbol_mode,
                )
                .unwrap_or(Value::NIL);
                let mut advanced_to_end = false;
                if !rules.is_nil() {
                    for rule in neovm_value::value::list_iter(rules) {
                        let Some(fields) = rule.as_vector_data() else {
                            continue;
                        };
                        if fields.len() != 3 {
                            continue;
                        }
                        let Some(lookback) = fields[1]
                            .as_fixnum()
                            .and_then(|value| usize::try_from(value).ok())
                        else {
                            continue;
                        };
                        let Some(start) = trigger.checked_sub(lookback) else {
                            continue;
                        };
                        if start < head {
                            continue;
                        }
                        if limit <= pos as i64 {
                            if (pos as i64) < start as i64 {
                                continue;
                            }
                        } else if limit <= start as i64 {
                            continue;
                        }
                        // Every rule that passes the gates is examined: GNU
                        // assigns the rule function's result to *gstring even
                        // when that result is nil.
                        last = None;
                        let match_len = if fields[0].is_nil() {
                            1
                        } else {
                            let Some(pattern) = fields[0].as_lisp_string() else {
                                continue;
                            };
                            let suffix = &text[byte_offsets[start]..];
                            let Ok(Some(match_len)) =
                                super::regex::looking_at_lisp_pattern_length_with_syntax(
                                    pattern, suffix, &syntax,
                                )
                            else {
                                continue;
                            };
                            match_len
                        };
                        let end = start.saturating_add(match_len).min(tail);
                        if start >= end || trigger >= end {
                            continue;
                        }
                        // GNU passes the matched rule's FUNC to
                        // `auto-composition-function` as its first
                        // argument (`autocmp_chars`, src/composite.c).
                        let Some(gstring) = shape(start, end, fields[2]) else {
                            continue;
                        };
                        last = Some((start, end, gstring));
                        let target = if (pos as i64) < limit {
                            (pos as i64) < end as i64
                        } else {
                            start as i64 <= pos as i64 && (pos as i64) < end as i64
                        };
                        if target {
                            return last;
                        }
                        cur = end;
                        advanced_to_end = true;
                        break;
                    }
                }
                if !advanced_to_end {
                    cur += 1;
                }
            }

            if (pos as i64) < limit {
                return None;
            }
            if let Some(found) = last {
                return Some(found);
            }
            if scan_start == head {
                return None;
            }
            cur = scan_start;
            if cur == 0 {
                return None;
            }
            cur -= 1;
        }
    }
}

/// The scan over a SLICE of a buffer's text.
///
/// `text` is the slice to examine and `char_offset` is the char index of its
/// first character within the whole text, so the spans come back in absolute
/// coordinates. This exists so a caller can scan what a window shows instead
/// of what a buffer holds: GNU never sweeps a whole buffer for compositions
/// (`composition_compute_stop_pos` searches from the current position toward a
/// bounded stop), and sweeping one costs time proportional to buffer size on
/// every frame.
///
/// A slice is NOT equivalent to the whole text at its edges: a rule may look
/// back up to [`MAX_AUTO_COMPOSITION_LOOKBACK`] characters, and a match may
/// run past the end. A caller wanting every span that STARTS in `a..b` must
/// pass a slice covering `a - MAX_AUTO_COMPOSITION_LOOKBACK .. b + (longest
/// match)` and filter, which is what the equivalence pin checks.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn automatic_composition_spans_in(
    buffer: &neovm_edit::buffer::Buffer,
    composition_function_table: Value,
    text: &str,
    char_offset: usize,
) -> Vec<AutomaticCompositionSpan> {
    automatic_composition_spans_in_with_symbols(
        buffer,
        composition_function_table,
        text,
        char_offset,
        neovm_value::tagged::SymbolMode::Bare,
    )
}

pub(crate) fn automatic_composition_spans_in_with_symbols(
    buffer: &neovm_edit::buffer::Buffer,
    composition_function_table: Value,
    text: &str,
    char_offset: usize,
    mode: neovm_value::tagged::SymbolMode,
) -> Vec<AutomaticCompositionSpan> {
    install_substrate_seams();
    AutomaticCompositionRules::new_with_symbols(buffer, composition_function_table, mode)
        .map(|rules| rules.spans_in(text, char_offset))
        .unwrap_or_default()
}

fn select_automatic_composition_spans(
    composition_function_table: Value,
    syntax: &dyn super::regex_emacs::SyntaxLookup,
    text: &str,
    char_offset: usize,
) -> Vec<AutomaticCompositionSpan> {
    if text.is_empty() {
        return Vec::new();
    }

    // Byte offsets only: the scan jumps (`trigger = committed_end`), so it
    // needs random access, but a parallel `Vec<char>` of the whole buffer is
    // half a megabyte of pure duplication -- the character is one `chars()`
    // step from the offset we already keep.
    // `collect` trusts `CharIndices::size_hint`, whose lower bound is one
    // quarter of the byte length, so an ASCII scan grew and recopied this
    // table twice on the way up -- once per window per frame.
    let mut byte_offsets = Vec::with_capacity(text.len() + 1);
    byte_offsets.extend(text.char_indices().map(|(offset, _)| offset));
    byte_offsets.push(text.len());
    let char_count = byte_offsets.len() - 1;

    // GNU reads this table through `CHAR_TABLE_REF`, whose ASCII arm
    // (`CHAR_TABLE_REF_ASCII`) is a direct array index off `tbl->ascii`.
    // Asking the generic entry point once per character instead costs a call
    // frame, a `Result` channel and three repeated type checks -- 63 Ir a
    // character, for every character of every window on every frame. Nothing
    // in this scan runs Lisp, so the table cannot change under it: resolve its
    // ASCII row once and index it. Every entry stays reachable from the table
    // itself, which is a root, so holding them here roots nothing new.
    let ascii_rules: [Value; 128] = std::array::from_fn(|ch| {
        super::chartable::ct_lookup_with_symbols(
            &composition_function_table,
            ch as i64,
            syntax.symbol_mode(),
        )
        .unwrap_or(Value::NIL)
    });
    // Composition-rule tracing rides RUST_LOG like every other diagnostic:
    // `RUST_LOG=neovm_core::emacs_core::text::composite=debug`.
    tracing::debug!(
        text_len = text.len(),
        char_count = char_count,
        rules_for_dash = ascii_rules.get(0x2D).map(|rule| !rule.is_nil()),
        rules_non_nil = ascii_rules.iter().filter(|rule| !rule.is_nil()).count(),
        "select composition spans: scan start"
    );

    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut committed_end = 0usize;
    let mut trigger = 0usize;
    while trigger < char_count {
        let offset = byte_offsets[trigger];
        let lead = bytes[offset];
        let rules = if lead < 0x80 {
            ascii_rules[lead as usize]
        } else {
            let Some(trigger_char) = text[offset..].chars().next() else {
                break;
            };
            super::chartable::ct_lookup_with_symbols(
                &composition_function_table,
                i64::from(trigger_char as u32),
                syntax.symbol_mode(),
            )
            .unwrap_or(Value::NIL)
        };
        // Nearly every character in an ordinary buffer has NO composition
        // rule. `list_to_vec` opens with `Vec::with_capacity(16)` and so
        // allocates even for nil, which made this one malloc/free pair per
        // character of the whole buffer, per frame.
        if rules.is_nil() {
            trigger += 1;
            continue;
        }

        let mut matched = None;
        for rule in neovm_value::value::list_iter(rules) {
            let Some(fields) = rule.as_vector_data() else {
                continue;
            };
            if fields.len() != 3 {
                continue;
            }
            let Some(lookback) = fields[1]
                .as_fixnum()
                .and_then(|value| usize::try_from(value).ok())
            else {
                continue;
            };
            let Some(start) = trigger.checked_sub(lookback) else {
                continue;
            };
            if start < committed_end {
                continue;
            }

            let match_len = if fields[0].is_nil() {
                1
            } else {
                let Some(pattern) = fields[0].as_lisp_string() else {
                    continue;
                };
                let suffix = &text[byte_offsets[start]..];
                let Ok(Some(match_len)) = super::regex::looking_at_lisp_pattern_length_with_syntax(
                    pattern, suffix, syntax,
                ) else {
                    continue;
                };
                match_len
            };
            let end = start.saturating_add(match_len).min(char_count);
            if start < end && trigger < end {
                matched = Some(AutomaticCompositionSpan::new(
                    start + char_offset,
                    end + char_offset,
                ));
                break;
            }
        }

        if let Some(span) = matched {
            committed_end = span.end() - char_offset;
            trigger = committed_end;
            spans.push(span);
        } else {
            trigger += 1;
        }
    }
    spans
}

/// Lower one automatic composition exactly as GNU's terminal composer does.
///
/// `compose-gstring-for-terminal` (`lisp/composite.el`) gives every orphan
/// zero-width non-format character a space cell, replaces an orphan `Cf` with
/// a space, and attaches zero-width characters following a positive-width
/// character to that character's cell.  Neomacs terminals are UTF-8, so the
/// coding-system-unrepresentable branch of GNU's function is not applicable.
pub fn automatic_composition_for_terminal(text: &str) -> TerminalComposition {
    let chars = text.chars().collect::<Vec<_>>();
    let mut cells = Vec::new();
    let mut index = 0;
    let mut total_width = 0u16;

    while index < chars.len() {
        let ch = chars[index];
        let width = crate::encoding::char_width(ch);
        if width == 0 {
            let extenders =
                if unicode_general_category::get_general_category(ch) == GeneralCategory::Format {
                    Box::<str>::from("")
                } else {
                    Box::<str>::from(ch.to_string())
                };
            cells.push(TerminalCompositionCell {
                base: ' ',
                extenders,
                width_cols: 1,
                source_char_len: 1,
            });
            total_width = total_width.saturating_add(1);
            index += 1;
            continue;
        }

        let mut following = index + 1;
        while following < chars.len() && crate::encoding::char_width(chars[following]) == 0 {
            following += 1;
        }
        let extenders = chars[index + 1..following].iter().collect::<String>();
        let width_cols = u8::try_from(width).unwrap_or(u8::MAX);
        cells.push(TerminalCompositionCell {
            base: ch,
            extenders: extenders.into(),
            width_cols,
            source_char_len: u16::try_from(following - index).unwrap_or(u16::MAX),
        });
        total_width = total_width.saturating_add(u16::from(width_cols));
        index = following;
    }

    TerminalComposition {
        cells: cells.into_boxed_slice(),
        width_cols: total_width,
    }
}

/// GNU `char_composable_p` (src/composite.c:986-1005): characters of general
/// category C? (except ZWNJ, ZWJ and the tag characters) and the line and
/// paragraph separators are the only non-composable characters.  This is a
/// property of the character, not of whether the composition-function-table
/// currently has a rule for it.
fn char_composable_p(ch: char) -> bool {
    use neovm_value::emacs_char::{UnicodeCategory, char_general_category};

    let code = ch as u32;
    // GNU uses `c >= ' '` before consulting the category table.
    if code < ' ' as u32 {
        return false;
    }
    if code == 0x200C || code == 0x200D {
        return true;
    }
    if (0xE0020..=0xE007F).contains(&code) {
        return true;
    }
    char_general_category(code)
        .is_some_and(|category| category <= UnicodeCategory::SpaceSeparator as i64)
}

fn is_unicode_combining_mark(code: u32) -> bool {
    use neovm_value::emacs_char::UnicodeCategory;

    let Some(ch) = char::from_u32(code) else {
        return false;
    };
    matches!(
        UnicodeCategory::from(unicode_general_category::get_general_category(ch)),
        UnicodeCategory::NonspacingMark
            | UnicodeCategory::SpacingMark
            | UnicodeCategory::EnclosingMark
    )
}

/// Whether the active table still contains GNU's default look-behind-one rule
/// for graphic combining characters.
///
/// Consulting the table is important: users can disable or replace automatic
/// composition per character.  This recognizes the rule installed by
/// `lisp/composite.el`; general execution of arbitrary composition rules remains
/// the broader `find_automatic_composition' port.
fn has_default_combining_rule<H: TextHost>(ctx: &H, code: u32) -> bool {
    let symbol_mode = ctx.symbol_mode();
    let table = ctx
        .visible_variable_value_or_nil(neovm_value::intern::intern("composition-function-table"));
    let Ok(rules) = super::chartable::builtin_char_table_range_with_symbols(
        vec![table, Value::fixnum(code as i64)],
        Some(ctx.variables()),
        symbol_mode,
    ) else {
        return false;
    };
    let Some(rules) = list_to_vec(&rules) else {
        return false;
    };

    rules.into_iter().any(|rule| {
        let Some(fields) = rule.as_vector_data() else {
            return false;
        };
        fields.len() == 3
            && fields[0].as_utf8_str() == Some("\\c.\\c^+")
            && fields[1].as_fixnum() == Some(1)
            && fields[2].is_symbol_named("compose-gstring-for-graphic", symbol_mode)
    })
}

/// Fast path for the default base-plus-combining-marks rule installed by GNU
/// `composite.el`.
///
/// This is deliberately modeled as an automatic composition, rather than as
/// unconditional Unicode grapheme segmentation: it observes
/// `auto-composition-mode`, multibyte-ness, and the live
/// `composition-function-table`, matching the control points used by GNU's
/// `find_automatic_composition`.
fn find_default_combining_composition_in_string<H: TextHost>(
    ctx: &H,
    string: &neovm_value::heap_types::LispString,
    pos: usize,
    limit: i64,
) -> Option<CharRange> {
    if !string.is_multibyte()
        || !ctx.current_buffer_is_displayed()
        || ctx
            .visible_variable_value_or_nil(neovm_value::intern::intern("auto-composition-mode"))
            .is_nil()
    {
        return None;
    }

    let codes = neovm_value::strings_core::lisp_string_char_codes(string);
    let query = AutomaticCompositionQuery::new(pos, limit);
    let mut ranges = Vec::new();
    let mut base = 0usize;
    while base < codes.len() {
        if is_unicode_combining_mark(codes[base]) {
            base += 1;
            continue;
        }

        let mut end = base + 1;
        while end < codes.len()
            && is_unicode_combining_mark(codes[end])
            && has_default_combining_rule(ctx, codes[end])
        {
            end += 1;
        }
        if end > base + 1 {
            ranges.push(CharRange::new(CharPos0::new(base), CharPos0::new(end)));
        }
        base = end;
    }
    query.select(ranges)
}

/// The shared gates of GNU `find_automatic_composition` as reached from
/// `Ffind_composition_internal` (src/composite.c): multibyte text,
/// `auto-composition-mode` enabled, and the current buffer displayed in some
/// window (`Fget_buffer_window (Fcurrent_buffer (), Qnil)`).
///
/// GNU's `inhibit_auto_composition` also disables composition when
/// `auto-composition-mode` is a string equal to the selected terminal's tty
/// type.  That case is left to the shaping gate: on a tty the evaluator below
/// resolves no font object and the scan yields nothing, matching GNU's
/// outcome through the window-system branch `autocmp_chars` skips.
fn automatic_composition_enabled<H: TextHost>(ctx: &H, multibyte: bool) -> bool {
    multibyte
        && ctx.current_buffer_is_displayed()
        && !ctx
            .visible_variable_value_or_nil(neovm_value::intern::intern("auto-composition-mode"))
            .is_nil()
}

/// Build and shape the gstring of one composition as GNU's `autocmp_chars`
/// does: `composition-get-gstring`, then the rule function — which for the
/// shaping rules of the default table (Arabic, the Indic scripts, and the
/// graphic default) bottoms out in `font-shape-gstring` — runs through the
/// evaluator against the displayed window's font.
///
/// A font or shaping failure yields `None`, matching `autocmp_chars`'s nil
/// return that keeps GNU's scan moving to the next candidate.  One
/// simplification against `autocmp_chars` is deliberate: the font comes from
/// `font-at` at the composition start rather than GNU's whole-range
/// `font_range` walk (equivalent whenever one face covers the composition).
/// Otherwise this mirrors `autocmp_chars`: an already-cached shaped gstring
/// (non-nil LGSTRING_ID) is returned without running the rule, and otherwise
/// the buffer-local `auto-composition-function` is called with the rule's
/// FUNC (src/composite.c:941-975).
fn shape_composition_gstring<H: TextHost>(
    ctx: &mut H,
    start: i64,
    end: i64,
    func: Value,
    string: Value,
) -> Option<Value> {
    let font_at = Value::symbol(neovm_value::intern::intern("font-at"));
    let font_form = if string.is_nil() {
        Value::list(vec![font_at.clone(), Value::fixnum(start)])
    } else {
        Value::list(vec![font_at, Value::fixnum(start), Value::NIL, string])
    };
    let font = ctx.eval_sub(font_form).ok()?;
    if !font.is_font_object() {
        return None;
    }
    let gstring = composition_get_gstring(
        ctx,
        vec![Value::fixnum(start), Value::fixnum(end), font, string],
    )
    .ok()?;
    // GNU `autocmp_chars` calls `auto-composition-function` only when the
    // gstring has no ID (src/composite.c:973); a cached shaped gstring is
    // reused as-is.
    if gstring
        .as_vector_data()
        .and_then(|slots| slots.get(1))
        .is_some_and(|id| !id.is_nil())
    {
        return Some(gstring);
    }
    // GNU `autocmp_chars` runs the default `auto-composition-function`
    // (`auto-compose-chars`) with the matched rule's FUNC and the metric-
    // filled gstring; that default builds/looks up the gstring and calls
    // FUNC, which decides whether to shape (compose-gstring-for-graphic's
    // lone-mark branch only sets an adjustment).  Calling
    // `font-shape-gstring` directly here would skip both the user's
    // buffer-local function and the rule's non-shaping branches.
    let auto_composition =
        ctx.visible_variable_value_or_nil(neovm_value::intern::intern("auto-composition-function"));
    tracing::debug!(
        target: "neovm_text::composition_search",
        start,
        font_object = font.is_font_object(),
        auto_composition_nil = auto_composition.is_nil(),
        auto_composition_kind = ?auto_composition.kind(),
        "autocmp shape"
    );
    if auto_composition.is_nil() {
        return None;
    }
    // GNU `autocmp_chars` uses `safe_calln`, not `eval`: the arguments are
    // VALUES (the rule's FUNC symbol, the font object) and must not be
    // evaluated as variables.  Its `record_unwind_save_match_data`
    // (composite.c:941) and the point save around the call are mirrored by
    // `save-match-data` + `save-excursion`, so a rule's `string-match` cannot
    // leak out of the composition search.
    //
    // DIRECTION is Qnil on purpose.  The `find-composition-internal` subr
    // matches GNU `Ffind_composition_internal`, which passes Qnil.  The
    // layout snapshot resolver also reaches this builder (Neo's display
    // path), and it passes Qnil too, whereas GNU's `composition_reseat_it`
    // derives the direction from the run's bidi level (even -> L2R, odd ->
    // R2L, below zero -> nil).  HarfBuzz normally guesses the same direction
    // from the script, so Arabic and Hebrew agree; Common/Inherited-script
    // clusters on odd levels diverge.  That pre-existing display-path gap is
    // named N-autocmp-direction.
    let quote = |value: Value| {
        Value::list(vec![
            Value::symbol(neovm_value::intern::intern("quote")),
            value,
        ])
    };
    let call = Value::list(vec![
        Value::symbol(neovm_value::intern::intern("save-match-data")),
        Value::list(vec![
            Value::symbol(neovm_value::intern::intern("save-excursion")),
            Value::list(vec![
                Value::symbol(neovm_value::intern::intern("funcall")),
                quote(auto_composition),
                quote(func),
                Value::fixnum(start),
                Value::fixnum(end),
                font,
                string,
                Value::NIL,
            ]),
        ]),
    ]);
    let shaped = ctx.eval_sub(call).ok()?;
    (!shaped.is_nil()).then_some(shaped)
}

/// Walk `spans` in the query's order and return the first accepting span
/// whose gstring shapes; GNU's forward search treats an unshapable
/// candidate as no composition and keeps scanning.
/// The string-arm automatic composition search beyond the default
/// combining-marks rule: GNU `find_automatic_composition` for
/// `Ffind_composition_internal`'s string case.  `pos` and `limit` are in the
/// string's 0-based coordinates, as the caller received them.
fn find_automatic_composition_in_string<H: TextHost>(
    ctx: &mut H,
    string: Value,
    text: &str,
    multibyte: bool,
    pos: usize,
    limit: i64,
) -> Option<(i64, i64, Value)> {
    if !automatic_composition_enabled(ctx, multibyte) {
        return None;
    }
    let table = ctx
        .visible_variable_value_or_nil(neovm_value::intern::intern("composition-function-table"));
    let rules = {
        let buf = ctx.buffers().current_buffer()?;
        AutomaticCompositionRules::new(&buf, table)?
    };
    let limit = (limit >= 0).then_some(limit);
    let mut shape = |start: usize, end: usize, func: Value| {
        shape_composition_gstring(ctx, start as i64, end as i64, func, string)
    };
    rules
        .find_automatic_composition(text, pos, limit, 0, &mut shape)
        .map(|(start, end, gstring)| (start as i64, end as i64, gstring))
}

/// The buffer-arm automatic composition search: GNU
/// `find_automatic_composition` for `Ffind_composition_internal`'s nil-string
/// case.  `pos` and `to` are buffer positions (1-based; `to` negative means
/// "at POS").
///
/// The scan is bounded to POS's line — "This assumes a newline can never be
/// composed" — and to GNU's forward check limit.  The returned span is
/// 1-based and end-exclusive, like the property-based search beside it.
fn find_automatic_composition_in_buffer<H: TextHost>(
    ctx: &mut H,
    pos: i64,
    to: i64,
) -> Option<(i64, i64, Value)> {
    let (begv, zv, multibyte) = {
        let buf = ctx.buffers().current_buffer()?;
        (
            buf.point_min_lisp_char_pos().as_i64(),
            buf.point_max_lisp_char_pos().as_i64(),
            buf.get_multibyte(),
        )
    };
    if !automatic_composition_enabled(ctx, multibyte) {
        return None;
    }
    // GNU scans from the start of POS's line; a lookback that crosses a
    // newline is as impossible as a match that spans one.
    let head = {
        let buf = ctx.buffers().current_buffer()?;
        let mut byte = buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::new(pos));
        let begv_byte = buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::new(begv));
        while byte > begv_byte {
            match buf.char_before_emacs_byte_pos(byte) {
                Some('\n') | None => break,
                Some(_) => {
                    let Some(len) = buf.char_before_emacs_byte_len(byte) else {
                        break;
                    };
                    byte = byte.saturating_sub_len(len);
                }
            }
        }
        buf.emacs_byte_pos_to_lisp_char_pos(byte).as_i64()
    };
    // The forward trigger limit past POS (or LIMIT): matches may run to the
    // end of their line, but no trigger beyond this limit starts one.
    let fore_check_limit = if to >= 0 && to > pos {
        (to + MAX_AUTO_COMPOSITION_LOOKBACK as i64).min(zv)
    } else {
        (pos + 1 + MAX_AUTO_COMPOSITION_LOOKBACK as i64).min(zv)
    };
    let scan_end = {
        let buf = ctx.buffers().current_buffer()?;
        let mut cursor = fore_check_limit.max(head).min(zv);
        while cursor < zv {
            let byte =
                buf.lisp_pos_to_emacs_byte_pos(neovm_edit::buffer::LispCharPos1::new(cursor));
            if buf.char_code_after_emacs_byte_pos(byte) == Some('\n' as u32) {
                break;
            }
            cursor += 1;
        }
        cursor
    };
    if scan_end <= head {
        return None;
    }
    let text = {
        let buf = ctx.buffers().current_buffer()?;
        let byte_range =
            neovm_edit::emacs_core::editfns::current_buffer_accessible_char_region_in_buffers(
                ctx.buffers(),
                &Value::fixnum(head),
                &Value::fixnum(scan_end),
            )
            .ok()??;
        buf.buffer_substring_lisp_string_range(byte_range)
            .as_utf8_str()?
            .to_owned()
    };
    let table = ctx
        .visible_variable_value_or_nil(neovm_value::intern::intern("composition-function-table"));
    let rules = {
        let buf = ctx.buffers().current_buffer()?;
        AutomaticCompositionRules::new(&buf, table)?
    };
    let pos_index = usize::try_from(pos - head).ok()?;
    let limit = (to >= 0).then_some(to - head);
    let mut shape = |start: usize, end: usize, func: Value| {
        shape_composition_gstring(
            ctx,
            head + start as i64,
            head + end as i64,
            func,
            Value::NIL,
        )
    };
    rules
        .find_automatic_composition(&text, pos_index, limit, 0, &mut shape)
        .map(|(start, end, gstring)| (head + start as i64, head + end as i64, gstring))
}

/// `(find-composition-internal POS LIMIT STRING DETAIL-P)`
///
/// GNU `Ffind_composition_internal` (composite.c): describe the composition at
/// or nearest to POS. With DETAIL-P nil, returns `(FROM TO VALID-P)`; otherwise
/// `(FROM TO COMPONENTS RELATIVE-P MOD-FUNC WIDTH)`.  The default
/// base-plus-combining-marks automatic rule is implemented here; arbitrary
/// font-driven rules still require the broader display shaper port.
#[doc(hidden)]
pub fn find_composition_internal<H: TextHost>(ctx: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("find-composition-internal", &args, 4)?;
    expect_integer_or_marker_p(&args[0])?;
    if !args[1].is_nil() {
        expect_integer_or_marker_p(&args[1])?;
    }
    if !args[2].is_nil() && !args[2].is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[2]],
        ));
    }
    let detail = !args[3].is_nil();
    let pos = integer_value(&args[0]);
    let limit = if args[1].is_nil() {
        -1
    } else {
        integer_value(&args[1])
    };
    let comp = composition_prop_symbol();

    let found = if let Some(text) = ctx.lisp_string(args[2]) {
        let len = text.schars() as i64;
        if pos < 0 || pos > len {
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![args[2], Value::fixnum(pos)],
            ));
        }
        let table = get_string_text_properties_table_for_value(args[2]).unwrap_or_default();
        let run_at = |charpos: i64| -> Option<(i64, i64, Value)> {
            if charpos < 0 || charpos >= len {
                return None;
            }
            let (prop, s, e) = table.get_property_run_at_char_pos(
                CharPos0::new(charpos as usize),
                comp,
                len as usize,
            );
            match prop {
                Some(p) if !p.is_nil() => Some((s.get() as i64, e.get() as i64, p)),
                _ => None,
            }
        };
        // STRING positions are 0-based.  Stored composition properties take
        // precedence over automatic composition, as in GNU composite.c.
        if let Some((start, end, property)) = run_at(pos) {
            Some(LocatedComposition::Stored {
                start,
                end,
                property,
            })
        } else if let Some(range) =
            find_default_combining_composition_in_string(ctx, text, pos as usize, limit)
        {
            let start = range.start().get() as i64;
            let end = range.end().get() as i64;
            let gstring = composition_get_gstring(
                ctx,
                vec![
                    Value::fixnum(start),
                    Value::fixnum(end),
                    Value::NIL,
                    args[2],
                ],
            )?;
            Some(LocatedComposition::Automatic {
                start,
                end,
                gstring,
            })
        } else {
            let text_str = text.as_utf8_str().map(str::to_owned).unwrap_or_default();
            find_automatic_composition_in_string(
                ctx,
                args[2],
                &text_str,
                text.is_multibyte(),
                pos as usize,
                limit,
            )
            .map(|(start, end, gstring)| LocatedComposition::Automatic {
                start,
                end,
                gstring,
            })
        }
    } else {
        let (begv, zv) = {
            let Some(buf) = ctx.buffers().current_buffer() else {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::NIL, Value::fixnum(pos)],
                ));
            };
            (
                buf.point_min_lisp_char_pos().as_i64(),
                buf.point_max_lisp_char_pos().as_i64(),
            )
        };
        if pos < begv || pos > zv {
            let handle = ctx
                .buffers()
                .current_buffer()
                .map(|b| Value::make_buffer(b.id()))
                .unwrap_or(Value::NIL);
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![handle, Value::fixnum(pos)],
            ));
        }
        let to = if limit < 0 { -1 } else { limit.clamp(begv, zv) };
        let stored = {
            let buf = ctx
                .buffers()
                .current_buffer()
                .expect("checked current buffer");
            find_composition_in_buffer(buf, begv, zv, pos, to, comp)
        };
        match stored {
            // A stored run that does not cover POS still wins unless an
            // automatic composition sits strictly closer to POS (GNU
            // Ffind_composition_internal's second branch).
            Some((start, end, property)) if !(start <= pos && pos < end) => {
                match find_automatic_composition_in_buffer(ctx, pos, to) {
                    Some((s, e, gstring)) if (if e <= pos { e > end } else { s < start }) => {
                        Some(LocatedComposition::Automatic {
                            start: s,
                            end: e,
                            gstring,
                        })
                    }
                    _ => Some(LocatedComposition::Stored {
                        start,
                        end,
                        property,
                    }),
                }
            }
            found => {
                found.map(|(start, end, property)| LocatedComposition::Stored {
                    start,
                    end,
                    property,
                })
            }
            .or_else(|| {
                find_automatic_composition_in_buffer(ctx, pos, to).map(|(s, e, gstring)| {
                    LocatedComposition::Automatic {
                        start: s,
                        end: e,
                        gstring,
                    }
                })
            }),
        }
    };

    let Some(found) = found else {
        return Ok(Value::NIL);
    };
    let (start, end, prop) = match found {
        LocatedComposition::Automatic {
            start,
            end,
            gstring,
        } => {
            return Ok(Value::list(vec![
                Value::fixnum(start),
                Value::fixnum(end),
                gstring,
            ]));
        }
        LocatedComposition::Stored {
            start,
            end,
            property,
        } => (start, end, property),
    };

    if !composition_valid_unregistered(start, end, prop) {
        return Ok(Value::list(vec![
            Value::fixnum(start),
            Value::fixnum(end),
            Value::NIL,
        ]));
    }
    if !detail {
        return Ok(Value::list(vec![
            Value::fixnum(start),
            Value::fixnum(end),
            Value::T,
        ]));
    }

    // Requesting detail runs GNU `get_composition_id` (the detail branch of
    // `Ffind_composition_internal`), which registers the composition (rewriting
    // the property to Form-B) — unless the composition is invalid, in which case
    // it returns -1 and the property is left untouched.
    let (length, components, mod_func, registered) =
        composition_parts_any(prop).expect("valid composition decodes");
    let (key, relative_p, width) = if let Some(id) = registered {
        // Already Form-B: components is the registered vector; relative-p was
        // recorded at registration (it cannot be inferred from the bare vector).
        // GNU caches the width in `composition_table[id]`; recompute it from the
        // stored method + key, which is equivalent.
        let relative_p = composition_lookup_relative(id);
        let width = if relative_p {
            composition_relative_width(&components)
        } else {
            composition_rule_based_width(&components)
        };
        (components, relative_p, width)
    } else {
        let relative_p = composition_relative_p(components);
        let key =
            composition_components_key(ctx.buffers(), components, args[2], start, end - start);
        // GNU `get_composition_id` validates the components and returns -1 for an
        // invalid composition (e.g. an even-length rule vector). On -1 the detail
        // `tail` is nil, so only `(FROM TO)` is returned and PROP is not
        // registered.
        let Some(width) = composition_get_id_width(components, &key) else {
            return Ok(Value::list(vec![Value::fixnum(start), Value::fixnum(end)]));
        };
        composition_register_prop(prop, key, length, mod_func, relative_p);
        (key, relative_p, width)
    };
    Ok(Value::list(vec![
        Value::fixnum(start),
        Value::fixnum(end),
        key,
        if relative_p { Value::T } else { Value::NIL },
        mod_func,
        Value::fixnum(width),
    ]))
}

/// `(composition-get-gstring FROM TO FONT-OBJECT STRING)`
///
/// Return a gstring (grapheme cluster string) for composing characters
/// between FROM and TO with FONT-OBJECT in STRING.
///
/// Stub: return nil (let the display engine handle shaping).
#[doc(hidden)]
pub fn composition_get_gstring<H: TextHost>(ctx: &mut H, args: Vec<Value>) -> EvalResult {
    expect_args("composition-get-gstring", &args, 4)?;

    let codes = if args[3].is_nil() {
        let byte_range =
            neovm_edit::emacs_core::editfns::current_buffer_accessible_char_region_in_buffers(
                ctx.buffers(),
                &args[0],
                &args[1],
            )?;
        let Some(buf) = ctx.buffers().current_buffer() else {
            return Err(signal("error", vec![Value::string("No current buffer")]));
        };
        if !buf.get_multibyte() {
            return Err(signal(
                "error",
                vec![Value::string("Attempt to shape unibyte text")],
            ));
        }
        let Some(byte_range) = byte_range else {
            return Err(signal("error", vec![Value::string("No current buffer")]));
        };
        let text = buf.buffer_substring_lisp_string_range(byte_range);
        neovm_value::strings_core::lisp_string_char_codes(&text)
    } else {
        let text = expect_string_value(&args[3])?;
        if !text.is_multibyte() && text.as_bytes().iter().any(|byte| *byte >= 0x80) {
            return Err(signal(
                "error",
                vec![Value::string("Attempt to shape unibyte text")],
            ));
        }
        let codes = neovm_value::strings_core::lisp_string_char_codes(text);
        let len = codes.len() as i64;
        let (from, to) = validate_subarray_indices(args[3], args[0], args[1], len)?;
        codes[from as usize..to as usize].to_vec()
    };

    if codes.is_empty() {
        return Err(signal(
            "error",
            vec![Value::string("Attempt to shape zero-length text")],
        ));
    }

    let segment = &codes;
    let font_object = args[2];
    let mut encoded = vec![if font_object.is_font_object() {
        font_object
    } else {
        Value::symbol("utf-8-unix")
    }];
    encoded.extend(segment.iter().map(|code| Value::fixnum(*code as i64)));

    // GNU `Fcomposition_get_gstring` consults `gstring_hash_table` by the
    // header before building the body (src/composite.c:1981-1988).  A hit is
    // the previously SHAPED gstring with its ID set; returning it lets the
    // automatic-composition search skip the rule call entirely on re-display.
    let header_value = Value::vector(encoded.clone());
    if let Some(cached) = ctx.composition_gstring_cache_lookup(header_value) {
        return Ok(cached);
    }
    let mut gstring = vec![Value::vector(encoded), Value::NIL];
    // GNU `fill_gstring_body` sets each glyph's FROM and TO to the
    // character's index in the gstring (src/composite.c:889-891).  With a
    // font object it fills the driver's encoded code and text extents
    // (`font_fill_lglyph_metrics`); a character the font cannot encode, and
    // the terminal path, take the char-width-table fallback
    // (src/composite.c:907-916).  Composition rules read those metrics
    // (`compose-gstring-for-graphic` derives a lone mark's spacing
    // adjustment from lbearing/rbearing/width), so a placeholder metric here
    // changes the displayed advance.
    let width_table = if font_object.is_font_object() {
        None
    } else {
        crate::encoding::char_width_table_value(ctx)
    };
    for (i, code) in segment.iter().enumerate() {
        let code = *code as i64;
        let index = Value::fixnum(i as i64);
        let (code_slot, width, lbearing, rbearing, ascent, descent) =
            match (font_object.is_font_object(), u32::try_from(code).ok()) {
                (true, Some(ch)) => match ctx.font_glyph_text_metrics(font_object, ch) {
                    Some(metrics) => (
                        Value::fixnum(i64::from(metrics.code)),
                        metrics.width,
                        metrics.lbearing,
                        metrics.rbearing,
                        metrics.ascent,
                        metrics.descent,
                    ),
                    None => {
                        let width = crate::encoding::char_width_in_table(width_table, ch);
                        (Value::fixnum(code), width as i64, 0, width as i64, 1, 0)
                    }
                },
                (false, Some(ch)) => {
                    let width = crate::encoding::char_width_in_table(width_table, ch);
                    (Value::fixnum(code), width as i64, 0, width as i64, 1, 0)
                }
                _ => (Value::fixnum(code), 1, 0, 1, 1, 0),
            };
        gstring.push(Value::vector(vec![
            index,
            index,
            Value::fixnum(code),
            code_slot,
            Value::fixnum(width),
            Value::fixnum(lbearing),
            Value::fixnum(rbearing),
            Value::fixnum(ascent),
            Value::fixnum(descent),
            Value::NIL,
        ]));
    }
    while gstring.len() < 10 {
        gstring.push(Value::NIL);
    }

    Ok(Value::vector(gstring))
}

/// GNU `composition_gstring_p`: validate the public glyph-string shape before
/// a font driver sees it.  A glyph vector after the first nil slot is unused,
/// exactly as in `src/composite.c`.
#[doc(hidden)]
pub fn composition_gstring_p<H: TextHost>(ctx: &H, value: Value) -> bool {
    let symbol_mode = ctx.symbol_mode();
    let Some(gstring) = value.as_vector_data() else {
        return false;
    };
    if gstring.len() < 2 {
        return false;
    }
    let Some(header) = gstring[0].as_vector_data() else {
        return false;
    };
    if header.len() < 2 {
        return false;
    }
    let valid_header_font = header[0].is_nil()
        || header[0].is_font_object()
        || header[0]
            .as_symbol_name(symbol_mode)
            .is_some_and(|name| ctx.coding_systems().is_known_or_derived(name));
    if !valid_header_font
        || !header[1..]
            .iter()
            .all(|value| value.as_int().is_some_and(|code| code >= 0))
    {
        return false;
    }
    if !(gstring[1].is_nil() || gstring[1].as_int().is_some_and(|id| id >= 0)) {
        return false;
    }
    gstring[2..]
        .iter()
        .take_while(|glyph| !glyph.is_nil())
        .all(|glyph| {
            glyph
                .as_vector_data()
                .is_some_and(|slots| slots.len() == 10)
        })
}

/// `(clear-composition-cache)`
///
/// Clear the internal composition cache.
///
/// Stub: no cache to clear, return nil.
pub(crate) fn clear_composition_cache(args: Vec<Value>) -> EvalResult {
    expect_max_args("clear-composition-cache", &args, 0)?;
    Ok(Value::NIL)
}

/// `(composition-sort-rules RULES)`
///
/// Sort composition rules by priority.
///
/// Batch-compatible subset:
/// - nil RULES => nil
/// - non-list RULES => `(wrong-type-argument listp RULES)`
/// - list entries that are not composition rules => generic invalid-rule error
/// - otherwise return RULES unchanged
pub(crate) fn composition_sort_rules(args: Vec<Value>) -> EvalResult {
    expect_args("composition-sort-rules", &args, 1)?;
    if args[0].is_nil() {
        return Ok(Value::NIL);
    }

    let items = list_to_vec(&args[0]).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), args[0]],
        )
    })?;

    for item in items {
        if !item.is_cons() {
            return Err(signal(
                "error",
                vec![Value::string("Invalid composition rule in RULES argument")],
            ));
        }
    }

    Ok(args[0])
}

#[doc(hidden)]
pub fn compose_string<H: crate::emacs_core::text_host::TextHost>(
    _ctx: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    compose_string_internal(args)
}

#[doc(hidden)]
pub fn clear_cache<H: crate::emacs_core::text_host::TextHost>(
    ctx: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("clear-composition-cache", &args, 0)?;
    ctx.composition_gstring_cache_clear()
}

#[doc(hidden)]
pub fn sort_rules<H: crate::emacs_core::text_host::TextHost>(
    _ctx: &mut H,
    args: Vec<Value>,
) -> EvalResult {
    composition_sort_rules(args)
}

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

pub fn register_bootstrap_vars(obarray: &mut neovm_value::symbol::Obarray) {
    // Official Emacs leaves unicode-category-table as nil at C init time;
    // it is populated later by characters.el via unicode-property-table-internal.
    // character.c:1156 DEFVAR_LISP, init nil.
    obarray.define_special_variable("unicode-category-table", Value::NIL);
    // `char-unify-table` is deliberately NOT seeded.  GNU creates it lazily --
    // `define_charset` (charset.c:1364) is what gives the symbol a value, and
    // `(boundp 'char-unify-table)` is nil until then, so a nil seed here would
    // make the name bound where GNU's is not.  The unified-char table this port
    // uses internally is Rust-side storage, not this symbol; nothing reads it.
    // composition-function-table must be a real char-table (composite.c:2289).
    obarray.seed_value(
        "composition-function-table",
        make_char_table_value(Value::NIL, Value::NIL),
    );
    // composite.c:2231 DEFVAR_LISP, init Qt.
    obarray.define_special_variable("auto-composition-mode", Value::T);
    // composite.c:2215 DEFVAR_LISP,
    // `Vcompose_chars_after_function = intern_c_string ("compose-chars-after")'.
    // The initializer is the SYMBOL, not a function object: `Fcompose_region'
    // funcalls whatever the variable names, and `lisp/composite.el:212' defines
    // `compose-chars-after' itself.  Seeding nil here would have been a
    // different default from GNU's, not a milder one.
    obarray.define_special_variable(
        "compose-chars-after-function",
        Value::symbol("compose-chars-after"),
    );
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
/// Round 3w (plan-3w.md section 7, C1): this World state lives in a
/// thread-local while its World's Baton holder runs, and in the World while
/// it is parked. The Green runtime takes it at a park and restores it at the
/// next resume, so every Lisp thread sees one value, as in GNU. (N-mutator
/// mode needs shared storage instead: a recorded obligation.)
#[cold]
#[doc(hidden)]
pub fn world_state_placeholder() -> Box<dyn std::any::Any> {
    Box::new(CompositionRegistry {
        next_id: 0,
        dedup: std::collections::HashMap::new(),
        relative_by_id: Vec::new(),
    })
}

#[cold]
#[doc(hidden)]
pub fn swap_world_state(slot: &mut dyn std::any::Any) {
    let slot = slot
        .downcast_mut::<CompositionRegistry>()
        .expect("carried World state");
    COMPOSITION_REGISTRY.with(|registry| std::mem::swap(&mut *registry.borrow_mut(), slot));
}

#[cfg(test)]
#[path = "tests/composite_test.rs"]
mod tests;
