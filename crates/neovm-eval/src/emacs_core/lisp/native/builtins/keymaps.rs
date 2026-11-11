use super::*;
#[doc(hidden)]
pub use neovm_value::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use neovm_value::symbol::Obarray;
use neovm_value::tagged::SymbolMode;

// ===========================================================================
// Keymap builtins
// ===========================================================================
use super::keymap::{
    KeyEvent, KeymapMarker, collect_minor_mode_maps_in_state, current_active_maps_for_position,
    expand_meta_prefix_char_events_in_obarray, get_keymap_in_obarray, get_keymap_in_runtime,
    is_list_keymap, key_event_to_emacs_event, list_keymap_accessible, list_keymap_copy,
    list_keymap_define_seq_runtime_ex, list_keymap_inherits_from, list_keymap_parent,
    list_keymap_set_parent, lookup_key_in_keymaps_in_obarray_runtime, make_list_keymap,
    make_sparse_list_keymap, maybe_keymap_in_runtime,
};
pub(crate) use super::symbols::cache_event_symbol_value_properties_in_obarray;

fn map_keymap_binding_value(binding: Value, symbol_mode: SymbolMode) -> Value {
    if eq_value_swp(&binding, &Value::T, symbol_mode == SymbolMode::Positioned) {
        Value::NIL
    } else {
        binding
    }
}

/// Validate that a value is a keymap, returning it if so.
/// Accepts:
/// - Cons cells starting with 'keymap
/// - Symbols whose function definition is a keymap
pub(crate) fn expect_keymap_in_obarray(
    obarray: &Obarray,
    value: &Value,
    symbol_mode: SymbolMode,
) -> Result<Value, Flow> {
    get_keymap_in_obarray(obarray, value, true, symbol_mode)
}

fn expect_keymap(eval: &mut super::eval::Context, value: &Value) -> EvalResult {
    get_keymap_in_runtime(eval, value, true, true)
}

/// Parse a key description from a Value, returning emacs event values.
///
/// For vectors, integer and symbol elements are used directly as emacs event
/// codes (preserving all modifier bits including Alt and Hyper).  For strings,
/// each character is treated as a raw key event.
#[doc(hidden)]
pub fn expect_key_events(value: &Value, symbol_mode: SymbolMode) -> Result<Vec<Value>, Flow> {
    match value.kind() {
        // Vectors: use elements directly — integers are already emacs event codes,
        // symbols are already event symbols.
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = value.as_vector_data().unwrap().clone();
            let mut events = Vec::with_capacity(items.len());
            for item in &items {
                match symbol_mode.unwrap(*item).kind() {
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
                        if let Some(event) = convert_lucid_event_type_list(item, symbol_mode) {
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
    let symbol_mode = eval.symbol_mode();
    // SAFETY: this loop only parses event designators and writes their cached
    // properties. It invokes no Lisp or canonical dispatch producer and leaves
    // no manager/interior reference outside its single cohesive phase.
    unsafe {
        eval.obarray.with_legacy_value_write(|obarray| {
            for event in events {
                cache_event_symbol_value_properties_in_obarray(obarray, *event, symbol_mode)?;
            }
            Ok(Value::NIL)
        })
    }
}

fn lucid_event_type_list_p(value: &Value, symbol_mode: SymbolMode) -> bool {
    if !value.is_cons() {
        return false;
    }
    if let Some("help-echo" | "vertical-line" | "mode-line" | "tab-line" | "header-line") =
        value.cons_car().as_symbol_name(symbol_mode)
    {
        return false;
    }

    let mut cursor = *value;
    while cursor.is_cons() {
        let elt = cursor.cons_car();
        if !matches!(
            symbol_mode.unwrap(elt).kind(),
            ValueKind::Fixnum(_) | ValueKind::Symbol(_) | ValueKind::Nil | ValueKind::T
        ) {
            return false;
        }
        cursor = cursor.cons_cdr();
    }
    cursor.is_nil()
}

fn convert_lucid_event_type_list(value: &Value, symbol_mode: SymbolMode) -> Option<Value> {
    if !lucid_event_type_list_p(value, symbol_mode) {
        return None;
    }

    let mut items = Vec::new();
    let mut cursor = *value;
    while cursor.is_cons() {
        items.push(symbol_mode.unwrap(cursor.cons_car()));
        cursor = cursor.cons_cdr();
    }
    neovm_value::keyboard_pure::convert_lucid_event_list(&items, SymbolMode::Bare)
}

/// GNU `Fdefine_key` treats a vector whose first element is a cons as an
/// XEmacs-style keyboard macro and canonicalizes each Lucid event list in it.
/// Keep that compatibility conversion at the `define-key` boundary so stored
/// definitions, lookup results, and command-loop execution share one shape.
fn normalize_keyboard_macro_definition(definition: Value, symbol_mode: SymbolMode) -> Value {
    let Some(items) = definition.as_vector_data() else {
        return definition;
    };
    if items.first().is_none_or(|item| !item.is_cons()) {
        return definition;
    }

    Value::vector(
        items
            .iter()
            .map(|item| convert_lucid_event_type_list(item, symbol_mode).unwrap_or(*item))
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
#[doc(hidden)]
pub fn builtin_accessible_keymaps(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    builtin_accessible_keymaps_impl(eval.variables(), &args, symbol_mode)
}

pub(crate) fn builtin_accessible_keymaps_impl(
    vars: crate::emacs_core::eval::VariableView<'_>,
    args: &[Value],
    symbol_mode: SymbolMode,
) -> EvalResult {
    let obarray = vars.obarray();
    use neovm_value::value::{ValueKind, VecLikeType};

    expect_min_args("accessible-keymaps", args, 1)?;
    expect_max_args("accessible-keymaps", args, 2)?;
    let keymap = expect_keymap_in_obarray(obarray, &args[0], symbol_mode)?;

    // GNU starts the walk AT the map PREFIX reaches rather than enumerating
    // everything and filtering: the two differ, because the walk metizes a key
    // that follows `meta-prefix-char` but never the one that follows a PREFIX
    // ending in it.
    let prefix_events: Vec<Value> = match args.get(1) {
        None => Vec::new(),
        Some(prefix_arg) if prefix_arg.is_nil() => Vec::new(),
        Some(prefix_arg) => match prefix_arg.kind() {
            ValueKind::String => expect_key_events(prefix_arg, symbol_mode)?,
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
    list_keymap_accessible(
        keymap,
        &prefix_events,
        Some(vars),
        &mut all_out,
        symbol_mode,
    );

    Ok(Value::list(all_out))
}

/// (make-keymap) -> keymap
#[doc(hidden)]
pub fn builtin_make_keymap(_eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn builtin_make_sparse_keymap(
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
#[doc(hidden)]
pub fn builtin_copy_keymap(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    builtin_copy_keymap_impl(eval.obarray(), &args, symbol_mode)
}

pub(crate) fn builtin_copy_keymap_impl(
    obarray: &Obarray,
    args: &[Value],
    symbol_mode: SymbolMode,
) -> EvalResult {
    expect_args("copy-keymap", args, 1)?;
    let keymap = expect_keymap_in_obarray(obarray, &args[0], symbol_mode)?;
    Ok(list_keymap_copy(&keymap, symbol_mode))
}

/// (define-key KEYMAP KEY DEF &optional REMOVE) -> DEF
#[doc(hidden)]
pub fn builtin_define_key(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_min_args("define-key", &args, 3)?;
    expect_max_args("define-key", &args, 4)?;
    let keymap = expect_keymap(eval, &args[0])?;
    // KEYMAP may be a symbol whose function cell held the map, and the key
    // translation runs Lisp (`key-valid-p', `key-parse'), so keep the
    // resolved map rooted until it is updated.
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(keymap);
    let result = define_key_in_rooted_keymap(eval, keymap, &args, symbol_mode);
    eval.restore_specpdl_roots(root_scope);
    result
}

fn define_key_in_rooted_keymap(
    eval: &mut super::eval::Context,
    keymap: Value,
    args: &[Value],
    symbol_mode: SymbolMode,
) -> EvalResult {
    let key = possibly_translate_key_sequence(eval, args[1])?;
    let mut events = expect_key_events(&key, symbol_mode)?;
    cache_key_event_symbol_properties(eval, &events)?;
    let def = normalize_keyboard_macro_definition(args[2], symbol_mode);
    let remove = args.get(3).is_some_and(|v| v.is_truthy());
    // Expand meta-prefixed events to ESC + base, matching GNU Emacs
    // Fdefine_key's metized handling.
    if let Some(expanded) =
        expand_meta_prefix_char_events_in_obarray(eval.variables(), &events, symbol_mode)
    {
        events = expanded;
    }
    list_keymap_define_seq_runtime_ex(eval, keymap, &events, def, remove)?;
    Ok(def)
}

/// GNU `possibly_translate_key_sequence' (src/keymap.c), shared by
/// `define-key' and `lookup-key': a vector holding one string, such as
/// ["C-c a"], is parsed with `key-parse' when `key-valid-p' accepts the
/// string, and is returned unchanged otherwise.
fn possibly_translate_key_sequence(eval: &mut super::eval::Context, key: Value) -> EvalResult {
    let Some(items) = key.as_vector_data() else {
        return Ok(key);
    };
    let [string] = items.as_slice() else {
        return Ok(key);
    };
    let string = *string;
    if !string.is_string() {
        return Ok(key);
    }
    if !eval.obarray().fboundp("key-valid-p") {
        return Err(signal(
            "error",
            vec![
                Value::string("`key-valid-p' is not defined, so this syntax can't be used: %s"),
                key,
            ],
        ));
    }
    if eval
        .funcall_general(Value::symbol("key-valid-p"), vec![string])?
        .is_nil()
    {
        return Ok(key);
    }
    let parsed = eval.funcall_general(Value::symbol("key-parse"), vec![string])?;
    let length = match parsed.kind() {
        ValueKind::String => parsed.as_lisp_string().map_or(0, |s| s.schars()),
        ValueKind::Veclike(VecLikeType::Vector) => parsed.as_vector_data().map_or(0, |v| v.len()),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("arrayp"), parsed],
            ));
        }
    };
    if length == 0 {
        return Err(signal(
            "error",
            vec![Value::string("Invalid `key-parse' syntax: %S"), parsed],
        ));
    }
    Ok(parsed)
}

/// (lookup-key KEYMAP KEY &optional ACCEPT-DEFAULTS) -> binding or nil
#[doc(hidden)]
pub fn builtin_lookup_key(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_min_args("lookup-key", &args, 2)?;
    expect_max_args("lookup-key", &args, 3)?;
    let t_ok = args.get(2).is_some_and(|v| v.is_truthy());
    let key = possibly_translate_key_sequence(eval, args[1])?;
    let events = expect_key_events(&key, symbol_mode)?;
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
    let symbol_mode = eval.symbol_mode();
    let found = lookup_key_in_keymaps_in_obarray_runtime(eval, keymaps, events, t_ok)?;
    if is_defined_lookup_result(&found) || !is_menu_bar_key(events, symbol_mode) {
        return Ok(found);
    }

    let lower_events: Vec<Value> = events
        .iter()
        .map(|event| {
            event
                .as_symbol_name(symbol_mode)
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
                .as_symbol_name(symbol_mode)
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

fn is_menu_bar_key(events: &[Value], symbol_mode: SymbolMode) -> bool {
    events
        .first()
        .and_then(|event| event.as_symbol_name(symbol_mode))
        .is_some_and(|name| name == "menu-bar")
}

fn resolve_lookup_keymaps_in_runtime(
    eval: &mut super::eval::Context,
    value: &Value,
) -> Result<Vec<Value>, Flow> {
    let symbol_mode = eval.symbol_mode();
    if is_list_keymap(value, symbol_mode) {
        return Ok(vec![*value]);
    }
    if value.is_nil() {
        return Ok(vec![Value::NIL]);
    }
    if value.is_cons()
        && is_list_keymap(
            &maybe_keymap_in_runtime(eval, &value.cons_car(), true)?,
            symbol_mode,
        )
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
#[doc(hidden)]
pub fn builtin_use_local_map(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("use-local-map", &args, 1)?;
    let keymap = if args[0].is_nil() {
        Value::NIL
    } else {
        expect_keymap(eval, &args[0])?
    };
    let _ = eval.edit.buffers.set_current_local_map(keymap);
    Ok(Value::NIL)
}

/// (use-global-map KEYMAP)
#[doc(hidden)]
pub fn builtin_use_global_map(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("use-global-map", &args, 1)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    eval.select_global_map(keymap);
    Ok(Value::NIL)
}

/// (current-local-map) -> keymap or nil
#[doc(hidden)]
pub fn builtin_current_local_map(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_current_local_map_impl(eval.edit.buffers.current_local_map(), &args)
}

pub(crate) fn builtin_current_local_map_impl(
    current_local_map: Value,
    args: &[Value],
) -> EvalResult {
    expect_args("current-local-map", args, 0)?;
    Ok(current_local_map)
}

/// (current-global-map) -> keymap
#[doc(hidden)]
pub fn builtin_current_global_map(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("current-global-map", &args, 0)?;
    Ok(eval.current_global_map())
}

/// `(current-active-maps &optional OLP POSITION)` -> list of active keymaps.
///
/// Returns list of currently active keymaps in priority order.
/// GNU Emacs order: minor-mode maps > local-map > global-map.
#[doc(hidden)]
pub fn builtin_current_active_maps(
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
#[doc(hidden)]
pub fn builtin_current_minor_mode_maps(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_current_minor_mode_maps_impl(eval, &args)
}

pub(crate) fn builtin_current_minor_mode_maps_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: &[Value],
) -> EvalResult {
    let symbol_mode = ctx.symbol_mode();
    expect_args("current-minor-mode-maps", args, 0)?;
    let maps = collect_minor_mode_maps_in_state(
        ctx.variables(),
        &[],
        &ctx.edit.buffers,
        ctx.edit.buffers.current_buffer_id(),
        symbol_mode,
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

pub(crate) fn plan_keymap_iteration(keymap: Value, symbol_mode: SymbolMode) -> KeymapIterationPlan {
    let mut bindings = Vec::new();
    let mut parent = Value::NIL;
    let mut cursor = if is_list_keymap(&keymap, symbol_mode) {
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

        if is_list_keymap(&cursor, symbol_mode) {
            parent = cursor;
            break;
        }

        let entry = cursor.cons_car();
        if is_list_keymap(&entry, symbol_mode) {
            parent = entry;
            break;
        }

        if neovm_text::emacs_core::chartable::is_char_table(&entry) {
            let _ = neovm_text::emacs_core::chartable::for_each_char_table_mapping_with_symbols(
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
                    bindings.push((event, map_keymap_binding_value(binding, symbol_mode)));
                    Ok(())
                },
                symbol_mode,
            );
        } else {
            match entry.kind() {
                ValueKind::Cons => {
                    let pair_car = entry.cons_car();
                    let pair_cdr = entry.cons_cdr();
                    bindings.push((pair_car, map_keymap_binding_value(pair_cdr, symbol_mode)));
                }
                ValueKind::Veclike(VecLikeType::Vector) => {
                    let items = entry.as_vector_data().unwrap().clone();
                    for (idx, binding) in items.iter().enumerate() {
                        bindings.push((
                            Value::fixnum(idx as i64),
                            map_keymap_binding_value(*binding, symbol_mode),
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
#[doc(hidden)]
pub fn builtin_map_keymap(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_min_args("map-keymap", &args, 2)?;
    expect_max_args("map-keymap", &args, 3)?;
    let function = args[0];
    let keymap = expect_keymap(eval, &args[1])?;

    // Unlike map-keymap-internal, map-keymap descends into each composed
    // map and then resumes the enclosing spine. Treating the first embedded
    // map as the sole parent loses later maps (and inherited submenu items).
    let mut bindings = Vec::new();
    super::keymap::list_keymap_for_each_binding_recursive(
        &keymap,
        Some(eval.obarray()),
        |event, binding| {
            let event = if event.is_cons() {
                Value::cons(event.cons_car(), event.cons_cdr())
            } else {
                event
            };
            bindings.push((event, binding));
        },
        symbol_mode,
    );
    let holder = Value::list(
        bindings
            .iter()
            .map(|(key, value)| Value::cons(*key, *value))
            .collect(),
    );
    let root_scope = eval.save_specpdl_roots();
    eval.push_specpdl_root(holder);
    let result = execute_keymap_iteration_callbacks(eval, function, &bindings);
    eval.restore_specpdl_roots(root_scope);
    result?;
    Ok(Value::NIL)
}

/// `(map-keymap-internal FUNCTION KEYMAP)` -> parent keymap or nil.
///
/// Call FUNCTION for each binding in KEYMAP (not its parents).
/// Returns the parent keymap if it has one.
#[doc(hidden)]
pub fn builtin_map_keymap_internal(
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
    let symbol_mode = eval.symbol_mode();
    let plan = plan_keymap_iteration(keymap, symbol_mode);
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
#[doc(hidden)]
pub fn builtin_keymap_parent(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_args("keymap-parent", &args, 1)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    Ok(list_keymap_parent(&keymap, symbol_mode))
}

/// (set-keymap-parent KEYMAP PARENT) -> PARENT
#[doc(hidden)]
pub fn builtin_set_keymap_parent(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    expect_args("set-keymap-parent", &args, 2)?;
    let keymap = get_keymap_in_runtime(eval, &args[0], true, true)?;
    let parent = if args[1].is_nil() {
        Value::NIL
    } else {
        get_keymap_in_runtime(eval, &args[1], true, false)?
    };
    if !parent.is_nil() && list_keymap_inherits_from(&parent, &keymap, symbol_mode) {
        return Err(signal(
            "error",
            vec![Value::string("Cyclic keymap inheritance")],
        ));
    }
    list_keymap_set_parent(keymap, parent, symbol_mode);
    Ok(parent)
}

/// (keymapp OBJ) -> t or nil
#[doc(hidden)]
pub fn builtin_keymapp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let symbol_mode = eval.symbol_mode();
    builtin_keymapp_impl(eval.obarray(), &args, symbol_mode)
}

pub(crate) fn builtin_keymapp_impl(
    obarray: &Obarray,
    args: &[Value],
    symbol_mode: SymbolMode,
) -> EvalResult {
    expect_args("keymapp", args, 1)?;
    // GNU Fkeymapp is `!NILP (get_keymap (object, false, false))`, and
    // get_keymap with error_if_not_keymap=false answers a symbol whose
    // function cell is an unloaded (autoload ... keymap) form with the symbol
    // ITSELF -- so keymapp is t for e.g. kmacro-keymap before kmacro.el
    // loads.  help.el's describe-map relies on that to suppress the bare
    // prefix row and descend via map-keymap instead (ledger entry 61).
    let resolved = get_keymap_in_obarray(obarray, &args[0], false, symbol_mode)?;
    Ok(if resolved.is_nil() {
        Value::NIL
    } else {
        Value::T
    })
}

/// `(event-convert-list EVENT-DESC)` -> event object or nil
#[doc(hidden)]
pub fn builtin_event_convert_list(args: Vec<Value>, symbol_mode: SymbolMode) -> EvalResult {
    expect_args("event-convert-list", &args, 1)?;
    let Some(items) = list_to_vec(&args[0]) else {
        return Ok(Value::NIL);
    };
    if items.is_empty() {
        return Ok(Value::NIL);
    }
    convert_lucid_event_list(&items, symbol_mode)
        .ok_or_else(|| signal("error", vec![Value::string("Invalid event description")]))
}

/// `(text-char-description CHARACTER)` -> printable text description.
#[doc(hidden)]
pub fn builtin_text_char_description(args: Vec<Value>) -> EvalResult {
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
            let Some(ch) = neovm_value::emacs_char::EmacsChar::from_code(code as u32) else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("characterp"), args[0]],
                ));
            };
            return Ok(Value::heap_string(
                neovm_value::heap_types::LispString::from_emacs_bytes(ch.to_emacs_bytes()),
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
    Value::heap_string(neovm_value::heap_types::LispString::from_emacs_bytes(bytes))
}

/// `(single-key-description KEY &optional NO-ANGLES)` -> string
#[doc(hidden)]
pub fn builtin_single_key_description(args: Vec<Value>, symbol_mode: SymbolMode) -> EvalResult {
    expect_args_range("single-key-description", &args, 1, 2)?;
    let no_angles = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(key_description_string(describe_single_key_value(
        &args[0],
        no_angles,
        symbol_mode,
    )?))
}

/// `(key-description KEYS &optional PREFIX)` -> string
#[doc(hidden)]
pub fn builtin_key_description(args: Vec<Value>, symbol_mode: SymbolMode) -> EvalResult {
    expect_args_range("key-description", &args, 1, 2)?;
    let mut events = if let Some(prefix) = args.get(1) {
        key_sequence_values(prefix, symbol_mode)?
    } else {
        vec![]
    };
    events.extend(key_sequence_values(&args[0], symbol_mode)?);

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
                        symbol_mode,
                    )?);
                    add_meta = false;
                    continue;
                }
                None => {
                    rendered.push(describe_single_key_value(
                        &Value::fixnum(META_PREFIX_CHAR),
                        false,
                        symbol_mode,
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
        rendered.push(describe_single_key_value(event, false, symbol_mode)?);
    }
    if add_meta {
        rendered.push(describe_single_key_value(
            &Value::fixnum(META_PREFIX_CHAR),
            false,
            symbol_mode,
        )?);
    }
    Ok(key_description_string(rendered.join(&b' ')))
}

/// `(recent-keys &optional INCLUDE-CMDS)` -> vector of recent input events.
#[doc(hidden)]
pub fn builtin_recent_keys(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
#[path = "tests/keymaps_test.rs"]
mod tests;
