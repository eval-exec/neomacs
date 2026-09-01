use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use crate::emacs_core::symbol::Obarray;

// ===========================================================================
// Keymap builtins
// ===========================================================================
use super::keymap::{
    KeyEvent, KeymapMarker, collect_minor_mode_map_entries_in_state,
    collect_minor_mode_maps_in_state, current_active_maps_for_position,
    expand_meta_prefix_char_events_in_obarray, get_keymap_in_obarray, get_keymap_in_runtime,
    is_list_keymap, key_event_to_emacs_event, list_keymap_accessible, list_keymap_copy,
    list_keymap_define_seq_in_obarray_ex, list_keymap_inherits_from, list_keymap_parent,
    list_keymap_set_parent, lookup_key_in_keymaps_in_obarray_runtime, make_list_keymap,
    make_sparse_list_keymap, maybe_keymap_in_obarray, maybe_keymap_in_runtime,
};
use super::symbols::cache_event_symbol_value_properties_in_obarray;

fn map_keymap_binding_value(binding: Value) -> Value {
    if binding == Value::T {
        Value::NIL
    } else {
        binding
    }
}

/// Validate that a value is a keymap, returning it if so.
/// Accepts:
/// - Cons cells starting with 'keymap
/// - Symbols whose function definition is a keymap
pub(crate) fn expect_keymap_in_obarray(obarray: &Obarray, value: &Value) -> Result<Value, Flow> {
    get_keymap_in_obarray(obarray, value, true)
}

fn expect_keymap(eval: &mut super::eval::Context, value: &Value) -> EvalResult {
    get_keymap_in_runtime(eval, value, true, true)
}

#[allow(clippy::too_many_arguments)] // mirrors the Lisp helper's positional argument contract
fn call_help_describe_map_tree(
    eval: &mut super::eval::Context,
    startmap: Value,
    partial: Value,
    shadow: Value,
    prefix: Value,
    title: Value,
    nomenu: Value,
    transl: Value,
    always_title: Value,
    mention_shadow: Value,
    buffer: Value,
) -> Result<Value, Flow> {
    eval.apply(
        Value::symbol("help--describe-map-tree"),
        vec![
            startmap,
            partial,
            shadow,
            prefix,
            title,
            nomenu,
            transl,
            always_title,
            mention_shadow,
            buffer,
        ],
    )
}

/// Parse a key description from a Value, returning emacs event values.
///
/// For vectors, integer and symbol elements are used directly as emacs event
/// codes (preserving all modifier bits including Alt and Hyper).  For strings,
/// each character is treated as a raw key event.
pub(crate) fn expect_key_events(value: &Value) -> Result<Vec<Value>, Flow> {
    match value.kind() {
        // Vectors: use elements directly — integers are already emacs event codes,
        // symbols are already event symbols.
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = value.as_vector_data().unwrap().clone();
            let mut events = Vec::with_capacity(items.len());
            for item in &items {
                match item.kind() {
                    // Integer event codes (character + modifier bits)
                    ValueKind::Fixnum(_) => events.push(*item),
                    // Symbol events (function keys, remap, etc.)
                    ValueKind::Symbol(_) => events.push(*item),
                    // nil and t can appear as events in vectors
                    ValueKind::Nil => events.push(Value::symbol("nil")),
                    ValueKind::T => events.push(Value::symbol("t")),
                    // GNU only treats a cons vector element as a Lucid-style
                    // event type list when every element is an integer or a
                    // symbol.  Real mouse events are lists like
                    // (mouse-movement POSITION), where POSITION is itself a
                    // list; those remain parameterized events and key lookup
                    // matches on their car.
                    ValueKind::Cons => {
                        if let Some(event) = convert_lucid_event_type_list(item) {
                            // GNU `Fdefine_key` converts a Lucid event type list
                            // such as `(shift tab)` via `Fevent_convert_list`
                            // (src/keymap.c:1156-1157, 1264-1265).  That routine
                            // keeps a multi-character symbol base (e.g. `tab`) as
                            // a SYMBOL and applies modifiers to produce `S-tab`,
                            // whereas the kbd-designator path coerces `tab` to the
                            // character 9 and yields the integer 33554441.  Use
                            // the same conversion as `event-convert-list' so the
                            // stored key matches GNU exactly.
                            events.push(event);
                        } else {
                            events.push(*item);
                        }
                    }
                    _other => {
                        return Err(signal(
                            LispCondition::WrongTypeArgument,
                            vec![Value::symbol("arrayp"), *value],
                        ));
                    }
                }
            }
            Ok(events)
        }
        // Strings and other forms: go through KeyEvent roundtrip
        _ => {
            let key_events = expect_key_description(value)?;
            Ok(key_events.iter().map(key_event_to_emacs_event).collect())
        }
    }
}

fn cache_key_event_symbol_properties(
    eval: &mut super::eval::Context,
    events: &[Value],
) -> EvalResult {
    for event in events {
        cache_event_symbol_value_properties_in_obarray(eval.obarray_mut(), *event)?;
    }
    Ok(Value::NIL)
}

fn lucid_event_type_list_p(value: &Value) -> bool {
    if !value.is_cons() {
        return false;
    }
    if let Some("help-echo" | "vertical-line" | "mode-line" | "tab-line" | "header-line") =
        value.cons_car().as_symbol_name()
    {
        return false;
    }

    let mut cursor = *value;
    while cursor.is_cons() {
        let elt = cursor.cons_car();
        if !matches!(
            elt.kind(),
            ValueKind::Fixnum(_) | ValueKind::Symbol(_) | ValueKind::Nil | ValueKind::T
        ) {
            return false;
        }
        cursor = cursor.cons_cdr();
    }
    cursor.is_nil()
}

fn convert_lucid_event_type_list(value: &Value) -> Option<Value> {
    if !lucid_event_type_list_p(value) {
        return None;
    }

    let mut items = Vec::new();
    let mut cursor = *value;
    while cursor.is_cons() {
        items.push(cursor.cons_car());
        cursor = cursor.cons_cdr();
    }
    crate::emacs_core::keyboard::pure::convert_lucid_event_list(&items)
}

/// GNU `Fdefine_key` treats a vector whose first element is a cons as an
/// XEmacs-style keyboard macro and canonicalizes each Lucid event list in it.
/// Keep that compatibility conversion at the `define-key` boundary so stored
/// definitions, lookup results, and command-loop execution share one shape.
fn normalize_keyboard_macro_definition(definition: Value) -> Value {
    let Some(items) = definition.as_vector_data() else {
        return definition;
    };
    if items.first().is_none_or(|item| !item.is_cons()) {
        return definition;
    }

    Value::vector(
        items
            .iter()
            .map(|item| convert_lucid_event_type_list(item).unwrap_or(*item))
            .collect(),
    )
}

/// Parse a key description from a Value (must be a string or vector).
fn expect_key_description(value: &Value) -> Result<Vec<KeyEvent>, Flow> {
    match super::kbd::key_events_from_designator(value) {
        Ok(events) => Ok(events),
        Err(super::kbd::KeyDesignatorError::WrongType(other)) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("arrayp"), other],
        )),
        Err(super::kbd::KeyDesignatorError::Parse(msg)) => {
            Err(signal("error", vec![Value::string(msg)]))
        }
    }
}

/// `(accessible-keymaps KEYMAP &optional PREFIXES)` -> list of accessible keymaps.
pub(super) fn builtin_accessible_keymaps(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_accessible_keymaps_impl(eval.obarray(), &args)
}

pub(crate) fn builtin_accessible_keymaps_impl(obarray: &Obarray, args: &[Value]) -> EvalResult {
    use crate::emacs_core::value::{ValueKind, VecLikeType};

    expect_min_args("accessible-keymaps", args, 1)?;
    expect_max_args("accessible-keymaps", args, 2)?;
    let keymap = expect_keymap_in_obarray(obarray, &args[0])?;

    // GNU starts the walk AT the map PREFIX reaches rather than enumerating
    // everything and filtering: the two differ, because the walk metizes a key
    // that follows `meta-prefix-char` but never the one that follows a PREFIX
    // ending in it.
    let prefix_events: Vec<Value> = match args.get(1) {
        None => Vec::new(),
        Some(prefix_arg) if prefix_arg.is_nil() => Vec::new(),
        Some(prefix_arg) => match prefix_arg.kind() {
            ValueKind::String => expect_key_events(prefix_arg)?,
            ValueKind::Veclike(VecLikeType::Vector) => prefix_arg.as_vector_data().unwrap().clone(),
            // Lists are not valid as key sequences for prefix
            ValueKind::Cons => {
                return Err(super::error::signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("arrayp"), *prefix_arg],
                ));
            }
            _ => {
                return Err(super::error::signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("sequencep"), *prefix_arg],
                ));
            }
        },
    };

    let mut all_out = Vec::new();
    list_keymap_accessible(keymap, &prefix_events, Some(obarray), &mut all_out);

    Ok(Value::list(all_out))
}

/// (make-keymap) -> keymap
pub(super) fn builtin_make_keymap(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_make_keymap_pure(&args)
}

pub(crate) fn builtin_make_keymap_pure(args: &[Value]) -> EvalResult {
    expect_max_args("make-keymap", args, 1)?;
    let keymap = make_list_keymap();
    if let Some(prompt) = args.first()
        && !prompt.is_nil()
    {
        let tail = keymap.cons_cdr();
        if tail.is_cons() {
            tail.set_cdr(Value::cons(*prompt, Value::NIL));
        }
    }
    Ok(keymap)
}

/// (make-sparse-keymap &optional NAME) -> keymap
pub(super) fn builtin_make_sparse_keymap(
    _eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("make-sparse-keymap", &args, 1)?;
    // GNU keymap.c: (make-sparse-keymap "prompt") → (keymap "prompt")
    if let Some(prompt) = args.first()
        && prompt.is_string()
    {
        return Ok(Value::cons(
            KeymapMarker::Keymap.symbol_value(),
            Value::cons(*prompt, Value::NIL),
        ));
    }
    Ok(make_sparse_list_keymap())
}

/// `(copy-keymap KEYMAP)` -> keymap copy.
pub(super) fn builtin_copy_keymap(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_copy_keymap_impl(eval.obarray(), &args)
}

pub(crate) fn builtin_copy_keymap_impl(obarray: &Obarray, args: &[Value]) -> EvalResult {
    expect_args("copy-keymap", args, 1)?;
    let keymap = expect_keymap_in_obarray(obarray, &args[0])?;
    Ok(list_keymap_copy(&keymap))
}

/// (define-key KEYMAP KEY DEF &optional REMOVE) -> DEF
pub(super) fn builtin_define_key(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("define-key", &args, 3)?;
    expect_max_args("define-key", &args, 4)?;
    let keymap = expect_keymap(eval, &args[0])?;
    let mut events = expect_key_events(&args[1])?;
    cache_key_event_symbol_properties(eval, &events)?;
    let def = normalize_keyboard_macro_definition(args[2]);
    let remove = args.get(3).is_some_and(|v| v.is_truthy());
    // Expand meta-prefixed events to ESC + base, matching GNU Emacs
    // Fdefine_key's metized handling.
    if let Some(expanded) = expand_meta_prefix_char_events_in_obarray(eval.obarray(), &events) {
        events = expanded;
    }
    if let Err(msg) =
        list_keymap_define_seq_in_obarray_ex(eval.obarray(), keymap, &events, def, remove)
    {
        return Err(signal("error", vec![Value::string(msg)]));
    }
    Ok(def)
}

/// (lookup-key KEYMAP KEY &optional ACCEPT-DEFAULTS) -> binding or nil
pub(super) fn builtin_lookup_key(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("lookup-key", &args, 2)?;
    expect_max_args("lookup-key", &args, 3)?;
    let t_ok = args.get(2).is_some_and(|v| v.is_truthy());
    let events = expect_key_events(&args[1])?;
    cache_key_event_symbol_properties(eval, &events)?;
    let keymaps = resolve_lookup_keymaps_in_runtime(eval, &args[0])?;

    if events.is_empty() {
        return Ok(keymaps.first().copied().unwrap_or(Value::NIL));
    }

    // The resolved keymaps (possibly read from function cells, not the
    // rooted KEYMAP argument) and heap event conses live only in Rust Vecs
    // across the lookups, which can run Lisp (keymap autoloads, translation
    // functions); thread them onto one rooted holder for the span.
    let mut holder = Value::NIL;
    for value in keymaps.iter().chain(events.iter()).rev() {
        if value.is_heap_object() {
            holder = Value::cons(*value, holder);
        }
    }
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(holder);
    let result = lookup_key_with_menu_compat_runtime(eval, &keymaps, &events, t_ok);
    eval.restore_specpdl_roots(root_scope);
    result
}

fn lookup_key_with_menu_compat_runtime(
    eval: &mut super::eval::Context,
    keymaps: &[Value],
    events: &[Value],
    t_ok: bool,
) -> EvalResult {
    let found = lookup_key_in_keymaps_in_obarray_runtime(eval, keymaps, events, t_ok)?;
    if is_defined_lookup_result(&found) || !is_menu_bar_key(events) {
        return Ok(found);
    }

    let lower_events: Vec<Value> = events
        .iter()
        .map(|event| {
            event
                .as_symbol_name()
                .map(|name| Value::symbol(name.to_lowercase()))
                .unwrap_or(*event)
        })
        .collect();
    let lower_found = lookup_key_in_keymaps_in_obarray_runtime(eval, keymaps, &lower_events, t_ok)?;
    if is_defined_lookup_result(&lower_found) {
        return Ok(lower_found);
    }

    let dash_events: Vec<Value> = lower_events
        .iter()
        .map(|event| {
            event
                .as_symbol_name()
                .filter(|name| name.contains(' '))
                .map(|name| Value::symbol(name.replace(' ', "-")))
                .unwrap_or(*event)
        })
        .collect();
    let dash_found = lookup_key_in_keymaps_in_obarray_runtime(eval, keymaps, &dash_events, t_ok)?;
    if is_defined_lookup_result(&dash_found) {
        return Ok(dash_found);
    }

    Ok(found)
}

fn is_defined_lookup_result(value: &Value) -> bool {
    !value.is_nil() && !value.is_fixnum()
}

fn is_menu_bar_key(events: &[Value]) -> bool {
    events
        .first()
        .and_then(|event| event.as_symbol_name())
        .is_some_and(|name| name == "menu-bar")
}

fn resolve_lookup_keymaps_in_runtime(
    eval: &mut super::eval::Context,
    value: &Value,
) -> Result<Vec<Value>, Flow> {
    if is_list_keymap(value) {
        return Ok(vec![*value]);
    }
    if value.is_nil() {
        return Ok(vec![Value::NIL]);
    }
    if value.is_cons()
        && is_list_keymap(&maybe_keymap_in_runtime(eval, &value.cons_car(), true)?)
        && let Some(items) = list_to_vec(value)
    {
        if items.is_empty() {
            return Ok(vec![Value::NIL]);
        }
        let mut resolved = Vec::with_capacity(items.len());
        for item in &items {
            if item.is_nil() {
                resolved.push(Value::NIL);
                continue;
            }
            let keymap = maybe_keymap_in_runtime(eval, item, true)?;
            if keymap.is_nil() {
                resolved.clear();
                break;
            }
            resolved.push(keymap);
        }
        if !resolved.is_empty() {
            return Ok(resolved);
        }
    }
    if value.is_cons() {
        return Ok(vec![*value]);
    }

    Ok(vec![get_keymap_in_runtime(eval, value, true, true)?])
}

// `global-set-key' and `local-set-key' are NOT here.  GNU has no C version
// of either: `(defun global-set-key (key command) ...)' is lisp/subr.el:1545
// and `(defun local-set-key (key command) ...)' is :1569, both three lines
// over `define-key' and `current-global-map' / `current-local-map', which are
// the C primitives (src/keymap.c) and stay registered.  The Rust versions
// checked the keymap BEFORE the key, so a non-array KEY reported
// `(wrong-type-argument keymapp nil)' where GNU signals
// `(wrong-type-argument arrayp KEY)' (DIVERGENCES.md 152).

/// (use-local-map KEYMAP)
pub(super) fn builtin_use_local_map(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("use-local-map", &args, 1)?;
    let keymap = if args[0].is_nil() {
        Value::NIL
    } else {
        expect_keymap(eval, &args[0])?
    };
    let _ = eval.buffers.set_current_local_map(keymap);
    Ok(Value::NIL)
}

/// (use-global-map KEYMAP)
pub(super) fn builtin_use_global_map(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("use-global-map", &args, 1)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    eval.select_global_map(keymap);
    Ok(Value::NIL)
}

/// (current-local-map) -> keymap or nil
pub(super) fn builtin_current_local_map(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_current_local_map_impl(eval.buffers.current_local_map(), &args)
}

pub(crate) fn builtin_current_local_map_impl(
    current_local_map: Value,
    args: &[Value],
) -> EvalResult {
    expect_args("current-local-map", args, 0)?;
    Ok(current_local_map)
}

/// (current-global-map) -> keymap
pub(super) fn builtin_current_global_map(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-global-map", &args, 0)?;
    Ok(eval.current_global_map())
}

pub(super) fn builtin_describe_buffer_bindings(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("describe-buffer-bindings", &args, 1, 3)?;
    if !args[0].is_buffer() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), args[0]],
        ));
    }
    if let Some(prefixes) = args.get(1)
        && !prefixes.is_nil()
        && !(prefixes.is_cons()
            || prefixes.is_vector()
            || prefixes.is_string()
            || prefixes.is_nil())
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("sequencep"), *prefixes],
        ));
    }

    let buffer = args[0];
    let prefix = args.get(1).copied().unwrap_or(Value::NIL);
    let nomenu = if args.get(2).is_some_and(|v| !v.is_nil()) {
        Value::NIL
    } else {
        Value::T
    };

    let Some(buffer_id) = buffer.as_buffer_id() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), buffer],
        ));
    };
    let Some(buf) = eval.buffers.get(buffer_id) else {
        return Err(signal(
            "error",
            vec![Value::string("Selecting deleted buffer")],
        ));
    };

    let buffer_keymap = buf.local_map();
    let buffer_point = buf.point_lisp_char_pos().as_i64();
    let major_mode_name = buf
        .get_buffer_local("major-mode")
        .and_then(|value| value.as_symbol_name())
        .unwrap_or("fundamental-mode")
        .to_string();

    let sections = describe_buffer_binding_sections(
        eval,
        buffer,
        buffer_id,
        buffer_keymap,
        buffer_point,
        &major_mode_name,
    )?;

    // Every keymap and shadow cons below is held in a Rust local across
    // help--describe-map-tree (arbitrary Lisp, can GC); root them, or a
    // collection mid-describe frees them and the later passes walk freed
    // keymaps. GNU's C locals survive via conservative stack scanning. The
    // scope unwinds with the specpdl on nonlocal exit.
    let root_scope = eval.save_specpdl_roots();
    for section in &sections {
        eval.push_specpdl_root(section.map);
    }

    let mut shadow = Value::NIL;
    for section in sections {
        let (partial, transl, always_title, section_shadow) = match section.kind {
            BindingSectionKind::Bindings { always_title } => (
                Value::T,
                Value::NIL,
                if always_title { Value::T } else { Value::NIL },
                shadow,
            ),
            // GNU passes nil shadow to every translation map and never adds one
            // to the accumulator: a translation is not a binding, so it neither
            // hides nor is hidden by one.
            BindingSectionKind::Translations => (Value::NIL, Value::T, Value::NIL, Value::NIL),
        };
        call_help_describe_map_tree(
            eval,
            section.map,
            partial,
            section_shadow,
            prefix,
            Value::string(section.title),
            nomenu,
            transl,
            always_title,
            Value::NIL,
            buffer,
        )?;
        if matches!(section.kind, BindingSectionKind::Bindings { .. }) {
            shadow = Value::cons(section.map, shadow);
            eval.push_specpdl_root(shadow);
        }
    }

    eval.restore_specpdl_roots(root_scope);
    Ok(Value::NIL)
}

/// GNU `MAX_5_BYTE_CHAR + 1` (character.h): the boundary `describe_vector` walks
/// a char-table in two passes around, so that ordinary characters are described
/// before the raw 8-bit ones.
const DESCRIBE_VECTOR_CHAR_TABLE_STOP: i64 = 0x3F_FF80;
/// GNU `MAX_CHAR + 1`.
const DESCRIBE_VECTOR_CHAR_TABLE_END: i64 = 0x40_0000;

/// One row of `describe_vector` output: a key or key RANGE that shares a single
/// definition, together with whether an outer keymap shadows it.
///
/// Deciding "is this row shadowed, and does that mean skip it or annotate it?"
/// in the middle of the emit loop is how GNU's own bug#9293 arose (a range was
/// printed whose members were not all shadowed alike). Naming the three outcomes
/// keeps the emit loop unable to forget one.
enum RowShadowing {
    /// Nothing shadows this row; print it plainly.
    None,
    /// Shadowed, and MENTION-SHADOW is off: GNU drops the row entirely.
    Suppressed,
    /// Shadowed, and MENTION-SHADOW is on: print the row and say what shadows it.
    Mentioned { shadowed_by: Value },
}

/// GNU `Fhelp__describe_vector` / `describe_vector` (keymap.c).
///
/// Describes one VECTOR or char-table element of a keymap into the current
/// buffer. `describe-map` (help.el) reaches every dense keymap element through
/// here, so while this was a no-op stub every character bound in a char-table --
/// `self-insert-command` across the printable range, every `C-x 8` composition,
/// the whole of `key-translation-map` -- was silently missing from
/// `describe-bindings` and `describe-mode`.
pub(super) fn builtin_help_describe_vector(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("help--describe-vector", &args, 7)?;
    let vector = args[0];
    let prefix = args[1];
    let describer = args[2];
    let partial = args[3].is_truthy();
    let shadow = args[4];
    let entire_map = args[5];
    let mention_shadow = args[6].is_truthy();

    let is_char_table = crate::emacs_core::chartable::is_char_table(&vector);
    if !is_char_table && !vector.is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vector-or-char-table-p"), vector],
        ));
    }

    // GNU's `elt_prefix` stays nil here: `help--describe-vector` always describes
    // a keymap element, so the prefix is rendered by `key-description` together
    // with the key rather than pasted in front of it.
    let (mut stop, to) = if is_char_table {
        (
            DESCRIBE_VECTOR_CHAR_TABLE_STOP,
            DESCRIBE_VECTOR_CHAR_TABLE_END,
        )
    } else {
        let len = vector.as_vector_data().map_or(0, |data| data.len() as i64);
        (len, len)
    };

    let mut first = true;
    let mut i: i64 = 0;
    loop {
        if i == stop {
            if i == to {
                break;
            }
            stop = to;
        }
        let starting_i = i;

        let raw = if is_char_table {
            let (val, _from, run_end) =
                crate::emacs_core::chartable::char_table_ref_and_range(&vector, starting_i)?;
            i = run_end.min(stop - 1).max(starting_i);
            val
        } else {
            vector.as_vector_data().unwrap()[starting_i as usize]
        };
        let definition = crate::emacs_core::keymap::get_keyelt(raw);
        if definition.is_nil() {
            i += 1;
            continue;
        }

        // Don't mention suppressed commands.
        if partial
            && definition.is_symbol()
            && let Some(name) = definition.as_symbol_name()
            && eval
                .obarray
                .get_property(name, "suppress-keymap")
                .is_some_and(|value| value.is_truthy())
        {
            i += 1;
            continue;
        }

        let key_vector = |index: i64| Value::vector(vec![Value::fixnum(index)]);
        let shadowing = if shadow.is_nil() {
            RowShadowing::None
        } else {
            let shadowed_by = shadow_lookup_for_describe(eval, shadow, key_vector(starting_i))?;
            if shadowed_by.is_nil() || shadowed_by == definition {
                RowShadowing::None
            } else if mention_shadow {
                RowShadowing::Mentioned { shadowed_by }
            } else {
                RowShadowing::Suppressed
            }
        };
        if matches!(shadowing, RowShadowing::Suppressed) {
            i += 1;
            continue;
        }

        // Ignore a definition shadowed by an earlier one in the same keymap.
        if !entire_map.is_nil() {
            let in_whole_map = eval.apply(
                Value::symbol("lookup-key"),
                vec![entire_map, key_vector(starting_i), Value::T],
            )?;
            if in_whole_map != definition {
                i += 1;
                continue;
            }
        }

        if first {
            insert_describe_text(eval, "\n")?;
            first = false;
        }
        insert_fontified_key(eval, key_vector(starting_i), prefix)?;

        // Find all consecutive keys that share this definition. A char-table
        // already reported its run above.
        if !is_char_table {
            let data = vector.as_vector_data().unwrap();
            while i + 1 < stop {
                let next = crate::emacs_core::keymap::get_keyelt(data[(i + 1) as usize]);
                if next.is_nil() || !equal_value(&next, &definition, 0) {
                    break;
                }
                i += 1;
            }
        }

        // A range is only honest if every key in it is shadowed the same way, so
        // GNU truncates it at the first member that is not (bug#9293).
        let check_ranges = eval
            .obarray
            .symbol_value("describe-bindings-check-shadowing-in-ranges")
            .copied()
            .unwrap_or(Value::NIL);
        let skip_self_insert = check_ranges.as_symbol_name() == Some("ignore-self-insert")
            && definition.as_symbol_name() == Some("self-insert-command");
        if check_ranges.is_truthy() && is_char_table && i != starting_i && !skip_self_insert {
            let shadowed_by = match shadowing {
                RowShadowing::Mentioned { shadowed_by } => shadowed_by,
                RowShadowing::None | RowShadowing::Suppressed => Value::NIL,
            };
            for j in (starting_i + 1)..=i {
                let at_j = shadow_lookup_for_describe(eval, shadow, key_vector(j))?;
                if !equal_value(&at_j, &shadowed_by, 0) {
                    i = j - 1;
                    break;
                }
            }
        }

        if i != starting_i {
            insert_describe_text(eval, " .. ")?;
            insert_fontified_key(eval, key_vector(i), prefix)?;
        }

        // DESCRIBER inserts the definition, including its own alignment.
        eval.apply(describer, vec![definition])?;

        if let RowShadowing::Mentioned { shadowed_by } = shadowing {
            // GNU steps back over the newline DESCRIBER just wrote so the note
            // lands on the same line, then steps forward again.
            let point = eval.apply(Value::symbol("point"), vec![])?;
            let before_newline = Value::fixnum(point.as_fixnum().unwrap_or(1) - 1);
            eval.apply(Value::symbol("goto-char"), vec![before_newline])?;
            let note = match shadowed_by.as_symbol_name() {
                Some(name) => format!("  (currently shadowed by `{name}')"),
                // Could be a keymap, a lambda, or a keyboard macro.
                None => "  (currently shadowed)".to_string(),
            };
            insert_describe_text(eval, &note)?;
            eval.apply(
                Value::symbol("goto-char"),
                vec![Value::fixnum(
                    before_newline.as_fixnum().unwrap_or(1) + note.chars().count() as i64 + 1,
                )],
            )?;
        }

        i += 1;
    }

    if is_char_table {
        let default = crate::emacs_core::chartable::char_table_default(&vector);
        if !default.is_nil() {
            insert_describe_text(eval, "default")?;
            eval.apply(describer, vec![default])?;
        }
    }

    Ok(Value::NIL)
}

/// GNU `shadow_lookup (SHADOW, KEY, Qt, 0)` (keymap.c): the binding KEY has in
/// the shadowing maps, with a "key too long" answer reported as no binding.
fn shadow_lookup_for_describe(
    eval: &mut super::eval::Context,
    shadow: Value,
    key: Value,
) -> EvalResult {
    let value = eval.apply(Value::symbol("lookup-key"), vec![shadow, key, Value::T])?;
    if value.as_fixnum().is_some_and(|n| n >= 0) {
        return Ok(Value::NIL);
    }
    Ok(value)
}

fn insert_describe_text(eval: &mut super::eval::Context, text: &str) -> EvalResult {
    crate::emacs_core::buffer::builtin_insert(eval, vec![Value::string(text)])
}

/// GNU `describe_key_maybe_fontify` with `keymap_p` true: the key description,
/// carrying the `help-key-binding` face that `*Help*` renders keys in.
fn insert_fontified_key(eval: &mut super::eval::Context, key: Value, prefix: Value) -> EvalResult {
    let description = builtin_key_description(vec![key, prefix])?;
    let length = eval.apply(Value::symbol("length"), vec![description])?;
    eval.apply(
        Value::symbol("add-text-properties"),
        vec![
            Value::fixnum(0),
            length,
            Value::list(vec![
                Value::symbol("font-lock-face"),
                Value::symbol("help-key-binding"),
            ]),
            description,
        ],
    )?;
    crate::emacs_core::buffer::builtin_insert(eval, vec![description])
}

/// How one `describe-bindings` section participates in shadowing, which is the
/// only axis on which GNU's ten-argument `help--describe-map-tree` calls actually
/// differ.
///
/// Spelling it as an enum keeps the rule in one place: every real keymap is
/// described against the maps already described (and shadows the ones after it),
/// while `key-translation-map`, `local-function-key-map` and `input-decode-map`
/// are described against nothing and shadow nothing. Passing the accumulated
/// shadow list to a translation map -- which is what this function used to do
/// with `key-translation-map` -- silently drops translations that happen to
/// collide with a binding.
#[derive(Clone, Copy)]
enum BindingSectionKind {
    /// A real keymap. GNU: PARTIAL=t, TRANSL=nil, SHADOW=the accumulator.
    Bindings {
        /// GNU's ALWAYS-TITLE: print the heading even when nothing is under it.
        /// Only the global section sets it.
        always_title: bool,
    },
    /// A translation map. GNU: PARTIAL=nil, TRANSL=t, SHADOW=nil.
    Translations,
}

struct BindingSection {
    title: String,
    map: Value,
    kind: BindingSectionKind,
}

/// The sections GNU `Fdescribe_buffer_bindings` (keymap.c) emits, in its order.
///
/// Collecting them before describing any of them keeps the order and the
/// overriding-map exclusion readable: GNU describes an overriding map INSTEAD OF
/// the `keymap` property, minor-mode and local maps, never alongside them.
fn describe_buffer_binding_sections(
    eval: &mut super::eval::Context,
    buffer: Value,
    buffer_id: crate::buffer::BufferId,
    buffer_keymap: Value,
    buffer_point: i64,
    major_mode_name: &str,
) -> Result<Vec<BindingSection>, Flow> {
    let mut sections = Vec::new();
    let named_map = |name: &str| -> Option<Value> {
        eval.obarray
            .symbol_value(name)
            .copied()
            .filter(|map| !map.is_nil())
    };

    if let Some(key_translation_map) = named_map("key-translation-map") {
        sections.push(BindingSection {
            title: "Key translations".to_string(),
            map: key_translation_map,
            kind: BindingSectionKind::Translations,
        });
    }

    // GNU prefers `overriding-terminal-local-map`, falls back to
    // `overriding-local-map`, and when either is in force describes ONLY it.
    let overriding =
        named_map("overriding-terminal-local-map").or_else(|| named_map("overriding-local-map"));
    if let Some(overriding) = overriding {
        sections.push(BindingSection {
            title: "\u{c}\nOverriding Bindings".to_string(),
            map: overriding,
            kind: BindingSectionKind::Bindings {
                always_title: false,
            },
        });
    } else {
        let keymap_property = crate::emacs_core::keymap::local_map_property_at_buffer_point(
            &eval.obarray,
            &eval.buffers,
            buffer,
            buffer_point,
            crate::emacs_core::keymap::LocalMapProperty::Keymap,
            buffer_keymap,
        )?;
        if !keymap_property.is_nil() {
            sections.push(BindingSection {
                title: "\u{c}\n`keymap' Property Bindings".to_string(),
                map: keymap_property,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }

        for (mode, keymap) in collect_minor_mode_map_entries_in_state(
            &eval.obarray,
            &[],
            &eval.buffers,
            Some(buffer_id),
        ) {
            sections.push(BindingSection {
                title: format!("\u{c}\n`{}' Minor Mode Bindings", resolve_sym(mode)),
                map: keymap,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }

        let local_map = crate::emacs_core::keymap::local_map_property_at_buffer_point(
            &eval.obarray,
            &eval.buffers,
            buffer,
            buffer_point,
            crate::emacs_core::keymap::LocalMapProperty::LocalMap,
            buffer_keymap,
        )?;
        if !local_map.is_nil() {
            // A `local-map' property REPLACES the buffer's own keymap, so the
            // heading names the property rather than the major mode.
            let title = if local_map == buffer_keymap {
                format!("\u{c}\n`{major_mode_name}' Major Mode Bindings")
            } else {
                "\u{c}\n`local-map' Property Bindings".to_string()
            };
            sections.push(BindingSection {
                title,
                map: local_map,
                kind: BindingSectionKind::Bindings {
                    always_title: false,
                },
            });
        }
    }

    sections.push(BindingSection {
        title: "\u{c}\nGlobal Bindings".to_string(),
        map: eval.current_global_map(),
        kind: BindingSectionKind::Bindings { always_title: true },
    });

    if let Some(function_key_map) = named_map("local-function-key-map") {
        sections.push(BindingSection {
            title: "\u{c}\nFunction key map translations".to_string(),
            map: function_key_map,
            kind: BindingSectionKind::Translations,
        });
    }

    if let Some(input_decode_map) = named_map("input-decode-map") {
        sections.push(BindingSection {
            title: "\u{c}\nInput decoding map translations".to_string(),
            map: input_decode_map,
            kind: BindingSectionKind::Translations,
        });
    }

    Ok(sections)
}

/// `(current-active-maps &optional OLP POSITION)` -> list of active keymaps.
///
/// Returns list of currently active keymaps in priority order.
/// GNU Emacs order: minor-mode maps > local-map > global-map.
pub(super) fn builtin_current_active_maps(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_current_active_maps_impl(eval, &args)
}

pub(crate) fn builtin_current_active_maps_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_max_args("current-active-maps", args, 2)?;
    let obey_overriding_local_maps = args.first().is_some_and(|v| v.is_truthy());
    let maps = current_active_maps_for_position(ctx, obey_overriding_local_maps, args.get(1))?;
    Ok(Value::list(maps))
}

/// `(current-minor-mode-maps)` -> list of active minor mode keymaps.
pub(super) fn builtin_current_minor_mode_maps(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_current_minor_mode_maps_impl(eval, &args)
}

pub(crate) fn builtin_current_minor_mode_maps_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_args("current-minor-mode-maps", args, 0)?;
    let maps = collect_minor_mode_maps_in_state(
        &ctx.obarray,
        &[],
        &ctx.buffers,
        ctx.buffers.current_buffer_id(),
    );
    if maps.is_empty() {
        Ok(Value::NIL)
    } else {
        Ok(Value::list(maps))
    }
}

pub(crate) struct KeymapIterationPlan {
    pub(crate) bindings: Vec<(Value, Value)>,
    pub(crate) parent: Value,
}

pub(crate) fn plan_keymap_iteration(keymap: Value) -> KeymapIterationPlan {
    let mut bindings = Vec::new();
    let mut parent = Value::NIL;
    let mut cursor = if is_list_keymap(&keymap) {
        keymap.cons_cdr()
    } else {
        keymap
    };
    let mut steps = 0usize;

    while cursor.is_cons() {
        steps += 1;
        if steps > 100_000 {
            break;
        }

        if is_list_keymap(&cursor) {
            parent = cursor;
            break;
        }

        let entry = cursor.cons_car();
        if is_list_keymap(&entry) {
            parent = entry;
            break;
        }

        if crate::emacs_core::chartable::is_char_table(&entry) {
            let _ = crate::emacs_core::chartable::for_each_char_table_mapping(
                &entry,
                |event, binding| {
                    // GNU `map_keymap_char_table_item`: "make a copy since
                    // map_char_table modifies it in place". The range cons the
                    // char-table walk yields is ONE reused cell, so planning the
                    // pairs before running FUNCTION would hand every range the
                    // same cell -- holding only its final, post-walk value.
                    let event = if event.is_cons() {
                        Value::cons(event.cons_car(), event.cons_cdr())
                    } else {
                        event
                    };
                    bindings.push((event, map_keymap_binding_value(binding)));
                    Ok(())
                },
            );
        } else {
            match entry.kind() {
                ValueKind::Cons => {
                    let pair_car = entry.cons_car();
                    let pair_cdr = entry.cons_cdr();
                    bindings.push((pair_car, map_keymap_binding_value(pair_cdr)));
                }
                ValueKind::Veclike(VecLikeType::Vector) => {
                    let items = entry.as_vector_data().unwrap().clone();
                    for (idx, binding) in items.iter().enumerate() {
                        bindings.push((
                            Value::fixnum(idx as i64),
                            map_keymap_binding_value(*binding),
                        ));
                    }
                }
                _ => {}
            }
        }

        cursor = cursor.cons_cdr();
    }

    KeymapIterationPlan { bindings, parent }
}

pub(crate) fn execute_keymap_iteration_callbacks(
    eval: &mut super::eval::Context,
    function: Value,
    bindings: &[(Value, Value)],
) -> Result<(), Flow> {
    for (event, binding) in bindings {
        eval.apply(function, vec![*event, *binding])?;
    }
    Ok(())
}

/// `(map-keymap FUNCTION KEYMAP &optional SORT-FIRST)` -> nil.
///
/// Call FUNCTION for each binding in KEYMAP and its parents.
/// FUNCTION receives two arguments: the event and the binding definition.
pub(super) fn builtin_map_keymap(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("map-keymap", &args, 2)?;
    expect_max_args("map-keymap", &args, 3)?;
    let function = args[0];
    let mut keymap = expect_keymap(eval, &args[1])?;

    // Traverse this keymap and all parents.
    loop {
        keymap = map_keymap_internal_impl(eval, function, keymap)?;
        if keymap.is_nil() {
            break;
        }
        // keymap is the parent; continue if it's a valid keymap.
        if !is_list_keymap(&keymap) {
            break;
        }
    }
    Ok(Value::NIL)
}

/// `(map-keymap-internal FUNCTION KEYMAP)` -> parent keymap or nil.
///
/// Call FUNCTION for each binding in KEYMAP (not its parents).
/// Returns the parent keymap if it has one.
pub(super) fn builtin_map_keymap_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("map-keymap-internal", &args, 2)?;
    let function = args[0];
    let keymap = expect_keymap(eval, &args[1])?;
    map_keymap_internal_impl(eval, function, keymap)
}

/// Core implementation: iterate over one level of keymap entries,
/// calling `function(event, binding)` for each. Returns the parent
/// keymap (or nil if none).
fn map_keymap_internal_impl(
    eval: &mut super::eval::Context,
    function: Value,
    keymap: Value,
) -> EvalResult {
    let plan = plan_keymap_iteration(keymap);
    // The planned (event, binding) pairs and the parent keymap live in a
    // Rust Vec while FUNCTION runs per entry — arbitrary Lisp that can GC.
    // Unrooted, the first callback's collection frees the remaining
    // entries and the iteration walks freed objects. GNU's map_keymap
    // keeps everything on the conservatively-scanned C stack (keymap.c).
    // Keep every planned value alive under a SINGLE root by threading the
    // pairs onto a heap list — the GC marks the list transitively as part
    // of the ordinary heap walk. (Per-entry specpdl roots would add
    // O(bindings) root-seed work to EVERY collection, which exact-GC
    // stress mode turns into minutes.) No safe point can run inside the
    // build loop, so the partially-built list needs no interim rooting.
    let mut entry_holder = plan.parent;
    for (event, binding) in &plan.bindings {
        entry_holder = Value::cons(Value::cons(*event, *binding), entry_holder);
    }
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(entry_holder);
    let result = execute_keymap_iteration_callbacks(eval, function, &plan.bindings);
    eval.restore_specpdl_roots(root_scope);
    result?;
    Ok(plan.parent)
}

/// (keymap-parent KEYMAP) -> keymap or nil
pub(super) fn builtin_keymap_parent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("keymap-parent", &args, 1)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    Ok(list_keymap_parent(&keymap))
}

/// (set-keymap-parent KEYMAP PARENT) -> PARENT
pub(super) fn builtin_set_keymap_parent(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-keymap-parent", &args, 2)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    let parent = if args[1].is_nil() {
        Value::NIL
    } else {
        get_keymap_in_runtime(eval, &args[1], true, false)?
    };
    if !parent.is_nil() && list_keymap_inherits_from(&parent, &keymap) {
        return Err(signal(
            "error",
            vec![Value::string("Cyclic keymap inheritance")],
        ));
    }
    list_keymap_set_parent(keymap, parent);
    Ok(parent)
}

/// (keymapp OBJ) -> t or nil
pub(super) fn builtin_keymapp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_keymapp_impl(eval.obarray(), &args)
}

pub(crate) fn builtin_keymapp_impl(obarray: &Obarray, args: &[Value]) -> EvalResult {
    expect_args("keymapp", args, 1)?;
    // GNU Fkeymapp is `!NILP (get_keymap (object, false, false))`, and
    // get_keymap with error_if_not_keymap=false answers a symbol whose
    // function cell is an unloaded (autoload ... keymap) form with the symbol
    // ITSELF -- so keymapp is t for e.g. kmacro-keymap before kmacro.el
    // loads.  help.el's describe-map relies on that to suppress the bare
    // prefix row and descend via map-keymap instead (ledger entry 61).
    let resolved = get_keymap_in_obarray(obarray, &args[0], false)?;
    Ok(if resolved.is_nil() {
        Value::NIL
    } else {
        Value::T
    })
}

/// `(event-convert-list EVENT-DESC)` -> event object or nil
pub(crate) fn builtin_event_convert_list(args: Vec<Value>) -> EvalResult {
    expect_args("event-convert-list", &args, 1)?;
    let Some(items) = list_to_vec(&args[0]) else {
        return Ok(Value::NIL);
    };
    if items.is_empty() {
        return Ok(Value::NIL);
    }
    convert_lucid_event_list(&items)
        .ok_or_else(|| signal("error", vec![Value::string("Invalid event description")]))
}

/// `(text-char-description CHARACTER)` -> printable text description.
pub(super) fn builtin_text_char_description(args: Vec<Value>) -> EvalResult {
    expect_args("text-char-description", &args, 1)?;
    let code = match args[0].kind() {
        ValueKind::Fixnum(n) if (0..=KEY_CHAR_CODE_MASK).contains(&n) => n,
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("characterp"), args[0]],
            ));
        }
    };
    if (code & !KEY_CHAR_CODE_MASK) != 0 {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("characterp"), args[0]],
        ));
    }

    let rendered = match code {
        0 => "^@".to_string(),
        1..=26 => format!(
            "^{}",
            char::from_u32((code as u32) + 64).expect("control-letter rendering must be ASCII")
        ),
        27 => "^[".to_string(),
        28 => "^\\\\".to_string(),
        29 => "^]".to_string(),
        30 => "^^".to_string(),
        31 => "^_".to_string(),
        127 => "^?".to_string(),
        // GNU splits on `ASCII_CHAR_P (c)` (keymap.c:2406): a printable ASCII
        // character takes the `make_string` arm and is UNIBYTE.
        32..=126 => (code as u8 as char).to_string(),
        // GNU's non-ASCII arm: `CHAR_STRING (c, str)` then
        // `make_multibyte_string (str, 1, len)` (keymap.c:2406-2411) -- one
        // character, encoded, whether or not it is a Unicode scalar value. A
        // raw byte therefore comes back as itself, not as U+FFFD.
        _ => {
            let Some(ch) = crate::emacs_core::emacs_char::EmacsChar::from_code(code as u32) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), args[0]],
                ));
            };
            return Ok(Value::heap_string(
                crate::heap_types::LispString::from_emacs_bytes(ch.to_emacs_bytes()),
            ));
        }
    };
    // The ASCII control arm: GNU returns `make_string (desc, len)`, a UNIBYTE
    // string, so `(multibyte-string-p (text-char-description ?a))` is nil --
    // unlike `key-description`, which is always multibyte.
    Ok(Value::string(rendered))
}

/// A key description as Lisp text.
///
/// GNU builds every key description with
/// `make_specified_string (tem, -1, p - tem, 1)` (keymap.c:2339), so the result
/// is MULTIBYTE whatever it contains -- `(multibyte-string-p (key-description
/// [?a]))` is `t` on GNU 31.0.90. That matters beyond the flag: only a
/// multibyte string can carry the eight-bit raw-byte character
/// `push_key_description` emits for a raw byte.
fn key_description_string(bytes: Vec<u8>) -> Value {
    Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes))
}

/// `(single-key-description KEY &optional NO-ANGLES)` -> string
pub(super) fn builtin_single_key_description(args: Vec<Value>) -> EvalResult {
    expect_args_range("single-key-description", &args, 1, 2)?;
    let no_angles = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(key_description_string(describe_single_key_value(
        &args[0], no_angles,
    )?))
}

/// `(key-description KEYS &optional PREFIX)` -> string
pub(crate) fn builtin_key_description(args: Vec<Value>) -> EvalResult {
    expect_args_range("key-description", &args, 1, 2)?;
    let mut events = if let Some(prefix) = args.get(1) {
        key_sequence_values(prefix)?
    } else {
        vec![]
    };
    events.extend(key_sequence_values(&args[0])?);

    // Mirror GNU `Fkey_description`: a lone `meta_prefix_char` (ESC, 27) folds
    // the meta bit onto the FOLLOWING event, so e.g. [27 97] -> "M-a".  When the
    // following event cannot absorb the meta bit (a non-fixnum, another ESC, or
    // an already-meta key), the ESC is rendered literally instead.
    const META_PREFIX_CHAR: i64 = 27;
    let mut rendered: Vec<Vec<u8>> = Vec::with_capacity(events.len());
    let mut add_meta = false;
    for event in &events {
        let event_fixnum = event.as_fixnum();
        if add_meta {
            let absorbs_meta = match event_fixnum {
                Some(code) if code != META_PREFIX_CHAR && (code & KEY_CHAR_META) == 0 => Some(code),
                _ => None,
            };
            match absorbs_meta {
                Some(code) => {
                    rendered.push(describe_single_key_value(
                        &Value::fixnum(code | KEY_CHAR_META),
                        false,
                    )?);
                    add_meta = false;
                    continue;
                }
                None => {
                    rendered.push(describe_single_key_value(
                        &Value::fixnum(META_PREFIX_CHAR),
                        false,
                    )?);
                    if event_fixnum == Some(META_PREFIX_CHAR) {
                        // Leave `add_meta` set: the next event still folds.
                        continue;
                    }
                    add_meta = false;
                }
            }
        } else if event_fixnum == Some(META_PREFIX_CHAR) {
            add_meta = true;
            continue;
        }
        rendered.push(describe_single_key_value(event, false)?);
    }
    if add_meta {
        rendered.push(describe_single_key_value(
            &Value::fixnum(META_PREFIX_CHAR),
            false,
        )?);
    }
    Ok(key_description_string(rendered.join(&b' ')))
}

/// `(recent-keys &optional INCLUDE-CMDS)` -> vector of recent input events.
pub(crate) fn builtin_recent_keys(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_recent_keys_impl(eval, args)
}

pub(crate) fn builtin_recent_keys_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("recent-keys", &args, 1)?;
    let include_commands = args.first().is_some_and(|arg| arg.is_truthy());
    let events = ctx
        .recent_input_events()
        .iter()
        .copied()
        .filter(|event| include_commands || !(event.is_cons() && event.cons_car().is_nil()))
        .collect::<Vec<_>>();
    Ok(Value::vector(events))
}

#[cfg(test)]
#[path = "tests/keymaps.rs"]
mod tests;
