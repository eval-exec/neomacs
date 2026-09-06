//! Composition builtins (complex script rendering).
//!
//! GNU Emacs records explicit compositions as a `composition` text property.
//! The display engine later validates and registers those properties when it
//! needs glyph data.  The Lisp-visible mutation semantics live here.

mod subrs;
#[cfg(test)]
pub(crate) use subrs::SUBRS;
pub(crate) use subrs::register_subrs;

use super::chartable::make_char_table_value;
use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::buffer::{CharLen, CharPos0, CharRange, EmacsByteRange};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args};
use crate::emacs_core::value::ValueKind;
use neomacs_display_protocol::glyph_matrix::{TerminalComposition, TerminalCompositionCell};
use unicode_general_category::GeneralCategory;

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

fn expect_string_value(arg: &Value) -> Result<&crate::heap_types::LispString, Flow> {
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
pub(crate) fn compose_region_internal(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("compose-region-internal", &args, 2, 4)?;
    // GNU `Fcompose_region_internal` runs `validate_region (&start, &end)`
    // (buffer.c), which resolves markers, swaps so the lower bound comes first,
    // then bounds-checks against `BEGV`/`ZV`.
    let mut beg = super::builtins::expect_integer_or_marker_in_buffers(&ctx.buffers, &args[0])?;
    let mut end = super::builtins::expect_integer_or_marker_in_buffers(&ctx.buffers, &args[1])?;
    if end < beg {
        std::mem::swap(&mut beg, &mut end);
    }
    let components = args.get(2).copied().unwrap_or(Value::NIL);
    let modification_func = args.get(3).copied().unwrap_or(Value::NIL);
    expect_composition_components(components)?;

    let (buffer_handle, point_min, point_max) = if let Some(buf) = ctx.buffers.current_buffer() {
        (
            Value::make_buffer(buf.id),
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
    super::textprop::builtin_put_text_property(
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
    super::textprop::save_string_props_for_value(args[0], table);

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
    let saved = super::eval::save_scratch_gc_roots();
    super::eval::push_scratch_gc_root(key);
    super::eval::push_scratch_gc_root(mod_func);
    let new_cdr = Value::cons(Value::fixnum(length), Value::cons(key, mod_func));
    prop.set_car(Value::fixnum(id));
    prop.set_cdr(new_cdr);
    super::eval::restore_scratch_gc_roots(saved);
    id
}

/// Display width and character length of the composition whose `composition`
/// text property begins at 1-based buffer position `charpos1`, or None if there
/// is no valid composition there. This is GNU's `get_composition_id` as called
/// from `current_column_1`: it returns the composed glyphs' width over the
/// covered characters AND, the first time the composition is seen, registers it
/// — rewriting the property from Form-A to Form-B in place.
pub(crate) fn composition_width_at(
    ctx: &super::eval::Context,
    charpos1: i64,
) -> Option<(i64, i64)> {
    let prop = super::textprop::builtin_get_text_property_in_state(
        &ctx.obarray,
        &ctx.buffers,
        &[Value::fixnum(charpos1), Value::symbol("composition")],
    )
    .ok()?;
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
    let key = composition_components_key(ctx, components, Value::NIL, charpos1, length);
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
    ctx: &super::eval::Context,
    components: Value,
    string: Value,
    start: i64,
    nchars: i64,
) -> Value {
    match components.kind() {
        ValueKind::Fixnum(code) => Value::vector(vec![Value::fixnum(code)]),
        ValueKind::String => {
            let codes = crate::emacs_core::builtins::lisp_string_char_codes(
                components.as_lisp_string().expect("string"),
            );
            Value::vector(codes.into_iter().map(|c| Value::fixnum(c as i64)).collect())
        }
        ValueKind::Cons => Value::vector(list_to_vec(&components).unwrap_or_default()),
        _ if components.is_vector() => components,
        _ => {
            // nil components: take the chars of the composed range.
            let codes: Vec<u32> = if let Some(text) = string.as_lisp_string() {
                let all = crate::emacs_core::builtins::lisp_string_char_codes(text);
                let from = start.max(0) as usize;
                let to = ((start + nchars).max(0) as usize).min(all.len());
                all.get(from..to).map(|s| s.to_vec()).unwrap_or_default()
            } else if let Some(buf) = ctx.buffers.current_buffer() {
                let byte_start = buf
                    .char_pos_to_emacs_byte_pos_clamped(CharPos0::new((start - 1).max(0) as usize));
                let byte_end = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(
                    (start - 1 + nchars).max(0) as usize,
                ));
                let sub = buf
                    .buffer_substring_lisp_string_range(EmacsByteRange::new(byte_start, byte_end));
                crate::emacs_core::builtins::lisp_string_char_codes(&sub)
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
    buf: &crate::buffer::buffer::Buffer,
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

/// A half-open character range selected by GNU's automatic-composition rule
/// table.
///
/// This is deliberately distinct from a Unicode grapheme range: the Lisp
/// `composition-function-table` may compose spacing characters, look behind
/// the trigger character, or leave zero-width characters uncomposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomaticCompositionSpan {
    range: CharRange,
}

impl AutomaticCompositionSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            range: CharRange::new(CharPos0::new(start), CharPos0::new(end)),
        }
    }

    pub const fn start(self) -> usize {
        self.range.start().get()
    }

    pub const fn end(self) -> usize {
        self.range.end().get()
    }
}

/// Select the non-overlapping automatic-composition spans in `text` from the
/// live Lisp rule table.
///
/// This is the forward-search core of GNU `find_automatic_composition`
/// (`src/composite.c`).  Rules are attached to a trigger character, carry an
/// explicit lookback, and are tried in list order.  Once a rule matches, the
/// next search begins after the complete match so a later rule cannot form the
/// partially overlapping composition GNU rejects.
/// Bytes of buffer text this process has actually scanned for automatic
/// compositions. Bumped only when the memo misses, so it reports work DONE
/// rather than work asked for. Read once per accepted frame into
/// `LayoutStats::composition_bytes_scanned`.
pub static BYTES_SCANNED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Take and reset [`BYTES_SCANNED`].
pub fn take_bytes_scanned() -> usize {
    BYTES_SCANNED.swap(0, std::sync::atomic::Ordering::Relaxed)
}

pub fn automatic_composition_spans(
    buffer: &crate::buffer::Buffer,
    composition_function_table: Value,
    text: &str,
) -> Vec<AutomaticCompositionSpan> {
    automatic_composition_spans_in(buffer, composition_function_table, text, 0)
}

/// GNU's look-back bound for automatic compositions (composite.c:156).
///
/// A rule may claim up to this many characters BEFORE the one that triggered
/// it, so a scan that wants every composition starting in a range must begin
/// this far ahead of it. GNU calls the same number "a limitation imposed by
/// composition rules in composition-function-table" (composite.c:1597).
pub const MAX_AUTO_COMPOSITION_LOOKBACK: usize = 3;

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
pub fn automatic_composition_spans_in(
    buffer: &crate::buffer::Buffer,
    composition_function_table: Value,
    text: &str,
    char_offset: usize,
) -> Vec<AutomaticCompositionSpan> {
    if !super::chartable::is_char_table(&composition_function_table) || text.is_empty() {
        return Vec::new();
    }

    // Byte offsets only: the scan jumps (`trigger = committed_end`), so it
    // needs random access, but a parallel `Vec<char>` of the whole buffer is
    // half a megabyte of pure duplication -- the character is one `chars()`
    // step from the offset we already keep.
    let mut byte_offsets = text
        .char_indices()
        .map(|(offset, _)| offset)
        .collect::<Vec<_>>();
    byte_offsets.push(text.len());
    let char_count = byte_offsets.len() - 1;

    let mut spans = Vec::new();
    let mut committed_end = 0usize;
    let mut trigger = 0usize;
    while trigger < char_count {
        let Some(trigger_char) = text[byte_offsets[trigger]..].chars().next() else {
            break;
        };
        let rules = super::chartable::ct_lookup(
            &composition_function_table,
            i64::from(trigger_char as u32),
        )
        .unwrap_or(Value::NIL);
        // Nearly every character in an ordinary buffer has NO composition
        // rule. `list_to_vec` opens with `Vec::with_capacity(16)` and so
        // allocates even for nil, which made this one malloc/free pair per
        // character of the whole buffer, per frame.
        if rules.is_nil() {
            trigger += 1;
            continue;
        }

        let mut matched = None;
        for rule in crate::emacs_core::value::list_iter(rules) {
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
                let mut match_data = None;
                let Ok(true) = super::regex::looking_at_lisp_pattern_with_buffer_tables(
                    pattern,
                    suffix,
                    false,
                    buffer,
                    &mut match_data,
                ) else {
                    continue;
                };
                let Some(group) = match_data.and_then(|data| data.group(0)) else {
                    continue;
                };
                group.end()
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

fn is_unicode_combining_mark(code: u32) -> bool {
    use crate::emacs_core::emacs_char::UnicodeCategory;

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
fn has_default_combining_rule(ctx: &super::eval::Context, code: u32) -> bool {
    let table = ctx.visible_variable_value_or_nil("composition-function-table");
    let Ok(rules) = super::chartable::builtin_char_table_range(
        vec![table, Value::fixnum(code as i64)],
        Some(&ctx.obarray),
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
            && fields[2].is_symbol_named("compose-gstring-for-graphic")
    })
}

fn current_buffer_is_displayed(ctx: &super::eval::Context) -> bool {
    let Some(buffer_id) = ctx.buffers.current_buffer_id() else {
        return false;
    };
    ctx.frames.selected_frame().is_some_and(|frame| {
        frame.window_list().into_iter().any(|window_id| {
            frame
                .find_window(window_id)
                .and_then(|window| window.buffer_id())
                == Some(buffer_id)
        })
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
fn find_default_combining_composition_in_string(
    ctx: &super::eval::Context,
    string: &crate::heap_types::LispString,
    pos: usize,
    limit: i64,
) -> Option<CharRange> {
    if !string.is_multibyte()
        || !current_buffer_is_displayed(ctx)
        || ctx
            .visible_variable_value_or_nil("auto-composition-mode")
            .is_nil()
    {
        return None;
    }

    let codes = super::builtins::lisp_string_char_codes(string);
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

/// `(find-composition-internal POS LIMIT STRING DETAIL-P)`
///
/// GNU `Ffind_composition_internal` (composite.c): describe the composition at
/// or nearest to POS. With DETAIL-P nil, returns `(FROM TO VALID-P)`; otherwise
/// `(FROM TO COMPONENTS RELATIVE-P MOD-FUNC WIDTH)`.  The default
/// base-plus-combining-marks automatic rule is implemented here; arbitrary
/// font-driven rules still require the broader display shaper port.
pub(crate) fn find_composition_internal(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
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
    let comp = Value::symbol("composition");

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
            None
        }
    } else {
        let (begv, zv) = {
            let Some(buf) = ctx.buffers.current_buffer() else {
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
                .buffers
                .current_buffer()
                .map(|b| Value::make_buffer(b.id))
                .unwrap_or(Value::NIL);
            return Err(signal(
                LispCondition::ArgsOutOfRange,
                vec![handle, Value::fixnum(pos)],
            ));
        }
        let to = if limit < 0 { -1 } else { limit.clamp(begv, zv) };
        let buf = ctx
            .buffers
            .current_buffer()
            .expect("checked current buffer");
        find_composition_in_buffer(buf, begv, zv, pos, to, comp).map(|(start, end, property)| {
            LocatedComposition::Stored {
                start,
                end,
                property,
            }
        })
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
        let key = composition_components_key(ctx, components, args[2], start, end - start);
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
pub(crate) fn composition_get_gstring(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("composition-get-gstring", &args, 4)?;

    let codes = if args[3].is_nil() {
        let byte_range = super::editfns::current_buffer_accessible_char_region_in_buffers(
            &ctx.buffers,
            &args[0],
            &args[1],
        )?;
        let Some(buf) = ctx.buffers.current_buffer() else {
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
        crate::emacs_core::builtins::lisp_string_char_codes(&text)
    } else {
        let text = expect_string_value(&args[3])?;
        if !text.is_multibyte() && text.as_bytes().iter().any(|byte| *byte >= 0x80) {
            return Err(signal(
                "error",
                vec![Value::string("Attempt to shape unibyte text")],
            ));
        }
        let codes = crate::emacs_core::builtins::lisp_string_char_codes(text);
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
    let mut encoded = vec![if super::font::is_font_object(&args[2]) {
        args[2]
    } else {
        Value::symbol("utf-8-unix")
    }];
    encoded.extend(segment.iter().map(|code| Value::fixnum(*code as i64)));

    let mut gstring = vec![Value::vector(encoded), Value::NIL];
    for code in segment {
        let code = *code as i64;
        gstring.push(Value::vector(vec![
            Value::fixnum(0),
            Value::fixnum(0),
            Value::fixnum(code),
            Value::fixnum(code),
            Value::fixnum(1),
            Value::fixnum(0),
            Value::fixnum(1),
            Value::fixnum(1),
            Value::fixnum(0),
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
pub(crate) fn composition_gstring_p(ctx: &super::eval::Context, value: Value) -> bool {
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
        || super::font::is_font_object(&header[0])
        || header[0]
            .as_symbol_name()
            .is_some_and(|name| ctx.coding_systems.is_known_or_derived(name));
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

fn compose_string(_ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    compose_string_internal(args)
}

fn clear_cache(_ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    clear_composition_cache(args)
}

fn sort_rules(_ctx: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    composition_sort_rules(args)
}

// ---------------------------------------------------------------------------
// Bootstrap variables
// ---------------------------------------------------------------------------

pub fn register_bootstrap_vars(obarray: &mut crate::emacs_core::symbol::Obarray) {
    // Official Emacs leaves unicode-category-table as nil at C init time;
    // it is populated later by characters.el via unicode-property-table-internal.
    // character.c:1156 DEFVAR_LISP, init nil.
    obarray.define_special_variable("unicode-category-table", Value::NIL);
    // char-unify-table is created lazily by define_charset (charset.c:1364).
    // Initialize to nil so maybe_unify_char gracefully degrades.
    obarray.set_symbol_value("char-unify-table", Value::NIL);
    // composition-function-table must be a real char-table (composite.c:2289).
    obarray.set_symbol_value(
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
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
