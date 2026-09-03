use super::*;
use crate::buffer::text_props::PropertyPlistApplication;
use crate::buffer::{CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, LispCharPos1};
use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use crate::emacs_core::symbol::Obarray;
use crate::emacs_core::textprop::{StickinessProperty, TextPropertyControlVariable};

fn runtime_string_value(value: Value) -> String {
    value
        .as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
        .expect("ValueKind::String must carry LispString payload")
}

pub(crate) fn builtin_get_pos_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_get_pos_property_impl(&eval.obarray, &[], Some(&eval.frames), &eval.buffers, args)
}

pub(crate) fn builtin_get_pos_property_impl(
    obarray: &crate::emacs_core::symbol::Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    frames: Option<&crate::window::FrameManager>,
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("get-pos-property", &args, 2)?;
    expect_max_args("get-pos-property", &args, 3)?;
    let pos = crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = super::textprop::expect_property_key(&args[1])?;

    if let Some(str_val) = args.get(2).filter(|v| v.is_string()) {
        if get_string_text_properties_table_for_value(*str_val).is_some() {
            return super::textprop::builtin_get_text_property_in_state(
                obarray,
                buffers,
                vec![Value::fixnum(pos), prop, *str_val],
            );
        }
        return Ok(Value::NIL);
    }

    // GNU editfns.c `get-pos-property`: a WINDOW object resolves to its buffer
    // (XWINDOW(object)->contents), a buffer/nil resolves as usual.
    let buf_id =
        super::textprop::resolve_char_property_buffer_id_with_frames(frames, buffers, args.get(2))?;

    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    super::textprop::buffer_pos_property_at_accessible_lisp_pos(obarray, buffers, buf, pos, prop)
}

pub(crate) fn builtin_next_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_char_property_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_next_char_property_change_in_buffers(
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("next-char-property-change", &args, 1)?;
    expect_max_args("next-char-property-change", &args, 2)?;

    // GNU: temp = next-overlay-change(POS); if LIMIT < temp, temp = LIMIT;
    // return next-property-change(POS, nil, temp).
    let overlay_next =
        crate::emacs_core::buffer::builtin_next_overlay_change_in_buffers(buffers, vec![args[0]])?;
    let mut temp = overlay_next;
    if let Some(limit) = args.get(1)
        && !limit.is_nil()
    {
        let lim_int =
            crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, limit)?;
        if let Some(temp_int) = temp.as_fixnum()
            && lim_int < temp_int
        {
            temp = Value::fixnum(lim_int);
        }
    }
    super::textprop::builtin_next_property_change_in_buffers(
        buffers,
        vec![args[0], Value::NIL, temp],
    )
}

pub(crate) fn builtin_pos_bol(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("pos-bol", &args, 1)?;
    // GNU `Fpos_bol` (editfns.c:684) returns the unconstrained line-beginning
    // position; only `Fline_beginning_position` adds field constraints.
    let scan_count = super::navigation::line_beginning_scan_count_arg(&args)?;
    let (bol_charpos, _orig, _count) = super::navigation::pos_bol_compute(eval, scan_count)?;
    Ok(Value::fixnum(bol_charpos))
}

pub(crate) fn builtin_pos_eol(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_max_args("pos-eol", &args, 1)?;
    // GNU `Fpos_eol` (editfns.c:740) returns the unconstrained line-end
    // position; only `Fline_end_position` adds field constraints.
    let scan_count = super::navigation::line_end_scan_count_arg(&args)?;
    let (eol_charpos, _orig) = super::navigation::pos_eol_compute(eval, scan_count)?;
    Ok(Value::fixnum(eol_charpos))
}

pub(crate) fn builtin_previous_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_property_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_previous_property_change_in_buffers(
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-property-change", &args, 1)?;
    expect_max_args("previous-property-change", &args, 3)?;

    let pos = crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;

    // --- String OBJECT ---
    if let Some(str_val) = args.get(1).filter(|v| v.is_string()) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        let table = get_string_text_properties_table_for_value(*str_val).unwrap_or_default();
        let char_pos = textprop::validate_string_point_raw(s, pos, args[0])?;
        let (limit_pos, limit_val) = match args.get(2) {
            Some(v) if !v.is_nil() => {
                let lim_int =
                    crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, v)?;
                (Some(lim_int), Some(lim_int))
            }
            _ => (None, None),
        };

        let ref_char = CharPos0::new(char_pos).saturating_sub_len(CharLen::new(1));
        let current_props = table.get_properties_at_char_pos(ref_char);
        let mut cursor = CharPos0::new(char_pos);

        while let Some(prev) = table.previous_property_change_before_char_pos(cursor) {
            if let Some(lim) = limit_pos
                && (prev.get() as i64) <= lim
            {
                return Ok(match limit_val {
                    Some(lv) => Value::fixnum(lv),
                    None => Value::NIL,
                });
            }
            let check = prev.saturating_sub_len(CharLen::new(1));
            let new_props = table.get_properties_at_char_pos(check);
            if new_props != current_props {
                return Ok(Value::fixnum(textprop::string_char_to_elisp_pos(s, prev)));
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

    // --- Buffer OBJECT ---
    let buf_id = match args.get(1) {
        None => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| b.id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_buffer() => Ok(v.as_buffer_id().unwrap()),
        Some(other) => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("buffer-or-string-p"), *other],
        )),
    }?;

    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;

    // GNU `Fprevious_property_change' validates through
    // `validate_interval_range (object, &position, &position, soft)'
    // (src/textprop.c:1090), whose out-of-range signal carries the position
    // TWICE because a point call passes one pointer for both `begin' and `end'
    // (src/textprop.c:141, :158).  The string branch above already used that
    // shape; this branch used `get_char_property_and_overlay''s single-value
    // shape (src/textprop.c:642-644), which belongs to a different family of
    // builtins.  `previous-char-property-change' inherits the payload from here
    // because it delegates, exactly as GNU does (src/textprop.c:767).
    let byte_pos = textprop::validate_buffer_property_point_raw(buf, pos, args[0])?;

    let (limit_pos, limit_val) = match args.get(2) {
        Some(v) if !v.is_nil() => {
            let limit = crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, v)?;
            (Some(limit), Some(limit))
        }
        _ => (None, None),
    };

    let ref_byte = textprop::emacs_byte_pos_of_preceding_char(buf, EmacsBytePos::new(byte_pos));
    let current_props = buf.text_props_get_properties_at_emacs_byte_pos(ref_byte);
    let mut cursor = EmacsBytePos::new(byte_pos);

    while let Some(prev) = buf.text_props_previous_change_before_emacs_byte_pos(cursor) {
        if let (Some(lim), Some(lv)) = (limit_pos, limit_val)
            && textprop::byte_to_elisp_pos(buf, prev) <= lim
        {
            return Ok(Value::fixnum(lv));
        }

        let check = textprop::emacs_byte_pos_of_preceding_char(buf, prev);
        let new_props = buf.text_props_get_properties_at_emacs_byte_pos(check);
        if new_props != current_props {
            return Ok(Value::fixnum(textprop::byte_to_elisp_pos(buf, prev)));
        }

        if prev == EmacsBytePos::ZERO {
            break;
        }
        cursor = if prev < cursor {
            prev
        } else {
            textprop::emacs_byte_pos_of_preceding_char(buf, prev)
        };
    }

    match limit_val {
        Some(lv) => Ok(Value::fixnum(lv)),
        None => Ok(Value::NIL),
    }
}

pub(crate) fn builtin_previous_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_char_property_change_in_buffers(&eval.buffers, args)
}

pub(crate) fn builtin_previous_char_property_change_in_buffers(
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-char-property-change", &args, 1)?;
    expect_max_args("previous-char-property-change", &args, 2)?;

    // GNU: temp = previous-overlay-change(POS); if LIMIT > temp, temp = LIMIT;
    // return previous-property-change(POS, nil, temp).
    let overlay_prev = crate::emacs_core::buffer::builtin_previous_overlay_change_in_buffers(
        buffers,
        vec![args[0]],
    )?;
    let mut temp = overlay_prev;
    if let Some(limit) = args.get(1)
        && !limit.is_nil()
    {
        let lim_int =
            crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, limit)?;
        if let Some(temp_int) = temp.as_fixnum()
            && lim_int > temp_int
        {
            temp = Value::fixnum(lim_int);
        }
    }
    builtin_previous_property_change_in_buffers(buffers, vec![args[0], Value::NIL, temp])
}

pub(crate) fn builtin_next_single_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_single_char_property_change_in_buffers(
        &eval.obarray,
        Some(&eval.frames),
        &eval.buffers,
        args,
    )
}

fn char_property_buffer_id_for_object(
    frames: Option<&crate::window::FrameManager>,
    buffers: &crate::buffer::BufferManager,
    object: Option<&Value>,
) -> Result<crate::buffer::BufferId, Flow> {
    // GNU textprop.c `next/previous-single-char-property-change` defer object
    // resolution to `Fget_char_property`, which resolves a WINDOW object to its
    // buffer. Mirror that here so a window OBJECT works.
    super::textprop::resolve_char_property_buffer_id_with_frames(frames, buffers, object)
}

fn next_char_property_change_for_buffer(
    buf: &crate::buffer::buffer::Buffer,
    position: i64,
    limit: i64,
) -> Result<i64, Flow> {
    let byte_pos = textprop::validate_buffer_point(buf, position)?;
    let overlay_next = buf
        .overlays
        .next_boundary_after_emacs_byte_pos(EmacsBytePos::new(byte_pos));
    let accessible = buf.accessible_emacs_byte_region();
    let point_max = textprop::byte_to_elisp_pos(buf, accessible.end());
    let mut temp = overlay_next.map_or(point_max, |next| textprop::byte_to_elisp_pos(buf, next));
    if limit < temp {
        temp = limit;
    }

    if let Some(next) = buf.text_props_next_change_after_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    {
        let next_pos = textprop::byte_to_elisp_pos(buf, next);
        if next < accessible.end() && next_pos < temp {
            return Ok(next_pos);
        }
    }
    Ok(temp)
}

fn previous_char_property_change_for_buffer(
    buf: &crate::buffer::buffer::Buffer,
    position: i64,
    limit: i64,
) -> Result<i64, Flow> {
    let byte_pos = textprop::validate_buffer_point(buf, position)?;
    let overlay_prev = buf
        .overlays
        .previous_boundary_before_emacs_byte_pos(EmacsBytePos::new(byte_pos));
    let accessible = buf.accessible_emacs_byte_region();
    let point_min = textprop::byte_to_elisp_pos(buf, accessible.start());
    let mut temp = overlay_prev.map_or(point_min, |prev| textprop::byte_to_elisp_pos(buf, prev));
    if limit > temp {
        temp = limit;
    }

    if let Some(prev) =
        buf.text_props_previous_change_before_emacs_byte_pos(EmacsBytePos::new(byte_pos))
    {
        let prev_pos = textprop::byte_to_elisp_pos(buf, prev);
        if prev > accessible.start() && prev_pos > temp {
            return Ok(prev_pos);
        }
    }
    Ok(temp)
}

pub(crate) fn builtin_next_single_char_property_change_in_buffers(
    obarray: &crate::emacs_core::symbol::Obarray,
    frames: Option<&crate::window::FrameManager>,
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("next-single-char-property-change", &args, 2)?;
    expect_max_args("next-single-char-property-change", &args, 4)?;

    if let Some(str_val) = args.get(2).filter(|v| v.is_string()) {
        let result = super::textprop::builtin_next_single_property_change_in_state(
            obarray,
            buffers,
            args.clone(),
        )?;
        if !result.is_nil() {
            return Ok(result);
        }
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        if let Some(limit) = args.get(3)
            && !limit.is_nil()
        {
            return Ok(Value::fixnum(expect_int(limit)?));
        }
        return Ok(Value::fixnum(s.schars() as i64));
    }

    let position =
        crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = super::textprop::expect_property_key(&args[1])?;
    let object = args.get(2);
    let buf_id = char_property_buffer_id_for_object(frames, buffers, object)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let accessible = buf.accessible_emacs_byte_region();
    let point_max = textprop::byte_to_elisp_pos(buf, accessible.end());
    let mut get_args = vec![Value::fixnum(position), prop];
    if let Some(object) = object {
        get_args.push(*object);
    }
    let initial_value =
        super::textprop::builtin_get_char_property_with_frames(obarray, buffers, frames, get_args)?;
    let limit = match args.get(3) {
        Some(v) if !v.is_nil() => {
            crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, v)?
        }
        _ => point_max,
    };

    if position >= limit {
        return Ok(Value::fixnum(limit.min(point_max)));
    }

    let mut cursor = position;
    loop {
        cursor = next_char_property_change_for_buffer(buf, cursor, limit)?;
        if cursor >= limit {
            return Ok(Value::fixnum(limit));
        }

        let mut value_args = vec![Value::fixnum(cursor), prop];
        if let Some(object) = object {
            value_args.push(*object);
        }
        let value = super::textprop::builtin_get_char_property_with_frames(
            obarray, buffers, frames, value_args,
        )?;
        if !eq_value(&value, &initial_value) || cursor >= point_max {
            return Ok(Value::fixnum(cursor));
        }
    }
}

pub(crate) fn builtin_previous_single_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_single_char_property_change_in_buffers(
        &eval.obarray,
        Some(&eval.frames),
        &eval.buffers,
        args,
    )
}

pub(crate) fn builtin_previous_single_char_property_change_in_buffers(
    obarray: &crate::emacs_core::symbol::Obarray,
    frames: Option<&crate::window::FrameManager>,
    buffers: &crate::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-single-char-property-change", &args, 2)?;
    expect_max_args("previous-single-char-property-change", &args, 4)?;

    if args.get(2).is_some_and(|v| v.is_string()) {
        let result = super::textprop::builtin_previous_single_property_change_in_state(
            obarray,
            buffers,
            args.clone(),
        )?;
        if !result.is_nil() {
            return Ok(result);
        }
        if let Some(limit) = args.get(3)
            && !limit.is_nil()
        {
            return Ok(Value::fixnum(expect_int(limit)?));
        }
        return Ok(Value::fixnum(0));
    }

    let position =
        crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;
    let prop = super::textprop::expect_property_key(&args[1])?;
    let object = args.get(2);
    let buf_id = char_property_buffer_id_for_object(frames, buffers, object)?;
    let buf = buffers
        .get(buf_id)
        .ok_or_else(|| signal("error", vec![Value::string("Buffer does not exist")]))?;
    let accessible = buf.accessible_emacs_byte_region();
    let point_min = textprop::byte_to_elisp_pos(buf, accessible.start());
    let limit = match args.get(3) {
        Some(v) if !v.is_nil() => {
            crate::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, v)?
        }
        _ => point_min,
    };

    if position <= limit {
        return Ok(Value::fixnum(limit.max(point_min)));
    }

    let initial_position = position - i64::from(position >= 0);
    let mut get_args = vec![Value::fixnum(initial_position), prop];
    if let Some(object) = object {
        get_args.push(*object);
    }
    let initial_value =
        super::textprop::builtin_get_char_property_with_frames(obarray, buffers, frames, get_args)?;

    let mut cursor = position;
    loop {
        cursor = previous_char_property_change_for_buffer(buf, cursor, limit)?;
        if cursor <= limit {
            return Ok(Value::fixnum(limit));
        }
        if cursor <= point_min {
            return Ok(Value::fixnum(cursor));
        }

        let mut value_args = vec![Value::fixnum(cursor - 1), prop];
        if let Some(object) = object {
            value_args.push(*object);
        }
        let value = super::textprop::builtin_get_char_property_with_frames(
            obarray, buffers, frames, value_args,
        )?;
        if !eq_value(&value, &initial_value) {
            return Ok(Value::fixnum(cursor));
        }
    }
}

pub(crate) fn builtin_defalias(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let plan = plan_defalias_in_obarray(eval.obarray(), &args)?;
    let DefaliasPlan {
        action,
        docstring,
        result,
    } = plan;
    let definition = match &action {
        DefaliasAction::SetFunction { definition, .. }
        | DefaliasAction::CallHook { definition, .. } => *definition,
    };
    eval.record_load_history_entry(crate::emacs_core::eval::LoadHistoryEntry::function(
        result, definition,
    ));
    eval.record_defalias_function_history(result);
    match action {
        DefaliasAction::SetFunction { symbol, definition } => {
            eval.obarray_mut()
                .set_symbol_function_id(symbol, definition);
        }
        DefaliasAction::CallHook {
            hook,
            symbol_value,
            definition,
        } => {
            eval.apply(hook, vec![symbol_value, definition])?;
        }
    }
    if let Some(symbol) = result.as_symbol_id() {
        let definition = eval
            .obarray
            .symbol_function_id(symbol)
            .unwrap_or(Value::NIL);
        crate::emacs_core::interactive::sync_interactive_registry_for_symbol_definition(
            &mut eval.interactive,
            symbol,
            definition,
        );
    }
    if let Some(docstring) = docstring {
        super::symbols::builtin_put(
            eval,
            vec![result, Value::symbol("function-documentation"), docstring],
        )?;
    }
    Ok(result)
}

pub(crate) enum DefaliasAction {
    SetFunction {
        symbol: SymId,
        definition: Value,
    },
    CallHook {
        hook: Value,
        symbol_value: Value,
        definition: Value,
    },
}

pub(crate) struct DefaliasPlan {
    pub(crate) action: DefaliasAction,
    pub(crate) docstring: Option<Value>,
    pub(crate) result: Value,
}

pub(crate) fn plan_defalias_in_obarray(
    obarray: &Obarray,
    args: &[Value],
) -> Result<DefaliasPlan, Flow> {
    expect_args_range("defalias", args, 2, 3)?;
    // Unwrap symbol-with-pos transparently via symbol_id, which handles
    // bare symbols, nil, t, and symbol-with-pos objects.
    let symbol = super::symbols::expect_symbol_id(&args[0])?;
    if symbol == intern("nil") {
        return Err(signal(
            LispCondition::SettingConstant,
            vec![Value::symbol("nil")],
        ));
    }
    let definition = args[1];
    if super::symbols::would_create_function_alias_cycle_in_obarray(obarray, symbol, &definition) {
        return Err(signal(
            LispCondition::CyclicFunctionIndirection,
            vec![args[0]],
        ));
    }
    let result = match args[0].kind() {
        ValueKind::Nil => Value::NIL,
        ValueKind::T => Value::T,
        ValueKind::Symbol(_) => args[0],
        _ => Value::from_sym_id(symbol),
    };
    let hook = obarray
        .get_property_id(symbol, intern("defalias-fset-function"))
        .unwrap_or(Value::NIL);
    let action = if hook.is_nil() {
        DefaliasAction::SetFunction { symbol, definition }
    } else {
        DefaliasAction::CallHook {
            hook,
            symbol_value: result,
            definition,
        }
    };
    let docstring = args.get(2).copied().filter(|value| !value.is_nil());
    Ok(DefaliasPlan {
        action,
        docstring,
        result,
    })
}

pub(crate) fn builtin_provide(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("provide", &args, 1, 2)?;
    eval.provide_value(args[0], args.get(1).cloned())
}

pub(crate) fn builtin_require(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("require", &args, 1, 3)?;
    eval.require_value(args[0], args.get(1).cloned(), args.get(2).cloned())
}

// ===========================================================================
// Loading / eval
// ===========================================================================

/// Convert an EvalError back to a Flow for builtins that call load_file.
fn eval_error_to_flow(e: super::error::EvalError) -> Flow {
    super::error::flow_from_eval_error(e)
}

/// `(garbage-collect)` — run a full GC cycle and return memory statistics.
pub(super) fn builtin_garbage_collect(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("garbage-collect", &args, 0)?;
    eval.gc_collect_exact();
    // Return GC stats.
    super::builtins_extra::builtin_garbage_collect_stats()
}

pub(crate) fn builtin_load(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("load", &args, 1)?;
    if let Some(result) = super::fileio::dispatch_file_handler(eval, "load", &args)? {
        return Ok(result);
    }
    match super::load::plan_load_in_context(
        eval,
        args[0],
        args.get(1).copied(),
        args.get(3).copied(),
        args.get(4).copied(),
    )? {
        super::load::LoadPlan::Return(value) => Ok(value),
        super::load::LoadPlan::Load { requested, found } => {
            let path = super::fileio::lisp_file_name_to_path_buf(&found);
            let options = super::load::LoadOptions::from_lisp_flags(
                args.get(1).is_some_and(|v| v.is_truthy()),
                args.get(2).is_some_and(|v| v.is_truthy()),
            );
            super::load::load_file_with_requested_and_found_options(
                eval, &path, &requested, &found, options,
            )
            .map_err(eval_error_to_flow)
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_load_file(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("load-file", &args, 1)?;
    let file = crate::emacs_core::builtins::expect_lisp_string(&args[0])?.clone();
    let path = super::fileio::lisp_file_name_to_path_buf(&file);
    super::load::load_file_with_found_options(
        eval,
        &path,
        &file,
        super::load::LoadOptions::EXPLICIT,
    )
    .map_err(eval_error_to_flow)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_eval(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("eval", &args, 1)?;
    expect_max_args("eval", &args, 2)?;
    eval.eval_value_with_lexical_arg(args[0], args.get(1).copied())
}

pub(crate) fn builtin_eval_2(
    eval: &mut super::eval::Context,
    form: Value,
    lexical: Value,
) -> EvalResult {
    eval.eval_value_with_lexical_arg(form, Some(lexical))
}

// Misc builtins
// ===========================================================================

/// Resolve a symbol's current value in the current-buffer scope,
/// honoring lexical environment, LOCALIZED BLV state, FORWARDED
/// BUFFER_OBJFWD slots, and active specpdl let-bindings.
///
/// Mirrors GNU `find_symbol_value` at `src/data.c:1584-1609`, which
/// walks the symbol's redirect chain, dispatches LOCALIZED via
/// `swap_in_symval_forwarding`, and reads FORWARDED via
/// `do_symval_forwarding`. Previously this helper called
/// `obarray.symbol_value(name)` directly, which returns the
/// BLV default cell unconditionally for `SymbolValue::BufferLocal`
/// — silently ignoring `(setq-local VAR VAL)`, `(let ((VAR VAL)) …)`,
/// and any per-buffer override. That divergence was audit finding
/// #3 in `drafts/regex-search-audit.md` and caused `case-fold-search`,
/// `search-upper-case`, `case-replace`, and every other buffer-local
/// search variable to ignore user overrides.
///
/// This implementation routes through `Context::eval_symbol_by_id`,
/// which goes through the full GNU lookup: lexenv → alias resolve
/// → LOCALIZED `read_localized` → buffer-local-binding → FORWARDED
/// `buffer_defaults` → obarray `find_symbol_value`. Any `Err`
/// (void-variable) is normalized to `None` for the legacy
/// `Option`-returning callsites.
pub(crate) fn dynamic_or_global_symbol_value(
    eval: &super::eval::Context,
    name: &str,
) -> Option<Value> {
    let id = crate::emacs_core::intern::intern(name);
    eval.eval_symbol_by_id(id).ok()
}

pub(super) fn dynamic_or_global_symbol_value_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    name: &str,
) -> Option<Value> {
    obarray.symbol_value(name).cloned()
}

pub(crate) fn inherited_text_properties_for_inserted_range_in_state(
    obarray: &crate::emacs_core::symbol::Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    buffers: &crate::buffer::BufferManager,
    buf: &crate::buffer::Buffer,
    insert_start: usize,
    insert_len: usize,
) -> Vec<(Value, Value)> {
    let insert_start_char = buf.emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(insert_start));
    let left_props = if insert_start_char > buf.point_min_char_pos() {
        // GNU intervals are indexed by character positions (`PT`), not raw
        // bytes. Step to the previous character boundary before consulting
        // the left neighbor; `insert_start - 1` can land inside an Emacs
        // multibyte/non-Unicode storage sequence.
        let left_byte = buf.char_pos_to_emacs_byte_pos_clamped(
            insert_start_char.saturating_sub_len(CharLen::new(1)),
        );
        buf.text_props_get_properties_ordered_at_emacs_byte_pos(left_byte)
    } else {
        Vec::new()
    };
    let right_pos = insert_start.saturating_add(insert_len);
    let right_char = buf
        .emacs_byte_pos_to_char_pos_clamped(EmacsBytePos::new(right_pos))
        .get();
    let right_props = if right_char < buf.point_max_char_pos().get() {
        let right_byte = buf.char_pos_to_emacs_byte_pos_clamped(CharPos0::new(right_char));
        buf.text_props_get_properties_ordered_at_emacs_byte_pos(right_byte)
    } else {
        Vec::new()
    };

    let left_map: HashMap<Value, Value> = left_props.iter().cloned().collect();
    let right_map: HashMap<Value, Value> = right_props.iter().cloned().collect();
    // GNU `adjust_intervals_for_insertion` reads stickiness with `textget`,
    // not direct plist lookup.  In particular, text buttons carry
    // `rear-nonsticky t` on their category symbol rather than on every
    // interval.  Resolve these meta-properties through the same effective
    // text-property path used by GNU before deciding what padding inherits.
    let effective_stickiness = |props: &[(Value, Value)], property: StickinessProperty| {
        super::textprop::lookup_text_property_from_plist_slice(
            obarray,
            buffers,
            props,
            property.value(),
        )
    };
    let left_front = effective_stickiness(&left_props, StickinessProperty::FrontSticky);
    let left_rear = effective_stickiness(&left_props, StickinessProperty::RearNonsticky);
    let right_front = effective_stickiness(&right_props, StickinessProperty::FrontSticky);
    let right_rear = effective_stickiness(&right_props, StickinessProperty::RearNonsticky);
    let default_nonsticky =
        TextPropertyControlVariable::TextPropertyDefaultNonsticky.value_for_buffer(obarray, buf);

    // GNU `adjust_intervals_for_insertion` (src/intervals.c): inserting into the
    // MIDDLE of a single uniform interval extends that interval, so the inserted
    // text inherits its full plist verbatim -- including the front-sticky /
    // rear-nonsticky meta entries -- rather than the between-intervals sticky
    // merge below. This only applies strictly interior (both neighbors present,
    // never at point-min/point-max) and when the two neighbors carry the same
    // property set (our proxy for "one interval", since neomacs coalesces equal
    // adjacent intervals). A real property that is rear-nonsticky (by value or
    // by `text-property-default-nonsticky`) forces a split, so fall through to
    // the merge in that case.
    if !left_props.is_empty() && !right_props.is_empty() && left_map == right_map {
        let forces_split = left_props.iter().any(|(name, _)| {
            if StickinessProperty::FrontSticky.is_value(*name)
                || StickinessProperty::RearNonsticky.is_value(*name)
            {
                return false;
            }
            let default_rear_nonsticky = default_nonsticky
                .as_ref()
                .and_then(|value| assq_cdr(value, *name))
                .is_some_and(|v| v.is_truthy());
            matches_rear_nonsticky(left_rear, *name) || default_rear_nonsticky
        });
        if !forces_split {
            return left_props.clone();
        }
    }

    let mut merged_props = Vec::new();
    let mut front_sticky = Vec::new();
    let mut rear_nonsticky = Vec::new();
    let mut seen = HashSet::new();

    for (name, right_value) in &right_props {
        if StickinessProperty::FrontSticky.is_value(*name)
            || StickinessProperty::RearNonsticky.is_value(*name)
        {
            continue;
        }
        seen.insert(*name);

        let left_present = left_map.contains_key(name);
        let left_value = left_map.get(name).copied().unwrap_or(Value::NIL);
        let default_entry = default_nonsticky
            .as_ref()
            .and_then(|value| assq_cdr(value, *name));
        let default_rear_nonsticky = default_entry.as_ref().is_some_and(|v| v.is_truthy());
        let default_front_sticky = default_entry.is_some_and(|v| v.is_nil());

        let mut use_left =
            left_present && !(matches_rear_nonsticky(left_rear, *name) || default_rear_nonsticky);
        let mut use_right = matches_front_sticky(right_front, *name) || default_front_sticky;
        if use_left && use_right {
            if left_value.is_nil() {
                use_left = false;
            } else if right_value.is_nil() {
                use_right = false;
            }
        }

        if use_left {
            merged_props.push((*name, left_value));
            if matches_front_sticky(left_front, *name) {
                front_sticky.push(*name);
            }
            if matches_rear_nonsticky(left_rear, *name) {
                rear_nonsticky.push(*name);
            }
        } else if use_right {
            merged_props.push((*name, *right_value));
            if matches_front_sticky(right_front, *name) {
                front_sticky.push(*name);
            }
            if matches_rear_nonsticky(right_rear, *name) {
                rear_nonsticky.push(*name);
            }
        }
    }

    for (name, left_value) in &left_props {
        if StickinessProperty::FrontSticky.is_value(*name)
            || StickinessProperty::RearNonsticky.is_value(*name)
            || seen.contains(name)
        {
            continue;
        }

        let default_entry = default_nonsticky
            .as_ref()
            .and_then(|value| assq_cdr(value, *name));
        let default_rear_nonsticky = default_entry.as_ref().is_some_and(|v| v.is_truthy());
        let default_front_sticky = default_entry.is_some_and(|v| v.is_nil());
        let left_nonsticky = matches_rear_nonsticky(left_rear, *name);
        let right_sticky = matches_front_sticky(right_front, *name) || default_front_sticky;

        if !(left_nonsticky || default_rear_nonsticky) {
            merged_props.push((*name, *left_value));
            if matches_front_sticky(left_front, *name) {
                front_sticky.push(*name);
            }
        } else if right_sticky {
            front_sticky.push(*name);
            if matches_rear_nonsticky(right_rear, *name) {
                rear_nonsticky.push(*name);
            }
        }
    }

    if !rear_nonsticky.is_empty() {
        merged_props.insert(
            0,
            (
                StickinessProperty::RearNonsticky.value(),
                Value::list(rear_nonsticky),
            ),
        );
    }

    let category_front_sticky_t = merged_props
        .iter()
        .find_map(|(name, value)| {
            (*name == Value::symbol("category"))
                .then(|| value.as_symbol_id())
                .flatten()
        })
        .and_then(|category| {
            obarray.get_property_id(category, StickinessProperty::FrontSticky.symbol_id())
        })
        .is_some_and(|value| value == Value::T);

    if !front_sticky.is_empty() && !category_front_sticky_t {
        merged_props.insert(
            0,
            (
                StickinessProperty::FrontSticky.value(),
                Value::list(front_sticky),
            ),
        );
    }

    merged_props
}

fn matches_front_sticky(value: Value, prop: Value) -> bool {
    value.is_t() || value_list_contains(&value, prop)
}

fn matches_rear_nonsticky(value: Value, prop: Value) -> bool {
    if value.is_nil() {
        return false;
    }
    if value.is_cons() {
        return value_list_contains(&value, prop);
    }
    true
}

fn assq_cdr(list: &Value, prop: Value) -> Option<Value> {
    // GNU `assq`: EQ on the keys. `Value ==` is DEEP equal (the documented
    // footgun) — besides the per-element cost, it would MATCH an `equal`
    // (non-eq) string/float key GNU's assq skips.
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

fn value_list_contains(list: &Value, prop: Value) -> bool {
    // GNU `memq`: EQ on the members (see `assq_cdr` on the `==` footgun).
    let mut cursor = *list;
    while cursor.is_cons() {
        let item = cursor.cons_car();
        if item.bits() == prop.bits() {
            return true;
        }
        cursor = cursor.cons_cdr();
    }
    false
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn buffer_read_only_active(
    eval: &super::eval::Context,
    buf: &crate::buffer::Buffer,
) -> bool {
    if let Some(value) = buf.get_buffer_local("inhibit-read-only")
        && value.is_truthy()
    {
        return false;
    }

    if eval
        .obarray
        .symbol_value("inhibit-read-only")
        .is_some_and(|value| value.is_truthy())
    {
        return false;
    }

    if buf.get_read_only() {
        return true;
    }

    if let Some(value) = buf.get_buffer_local("buffer-read-only") {
        return value.is_truthy();
    }

    eval.obarray
        .symbol_value("buffer-read-only")
        .is_some_and(|value| value.is_truthy())
}

pub(crate) fn builtin_barf_if_buffer_read_only(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_barf_if_buffer_read_only_impl(eval, args)
}

pub(crate) fn builtin_barf_if_buffer_read_only_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("barf-if-buffer-read-only", &args, 1)?;
    let position = match args.first() {
        None => None,
        Some(v) if v.is_nil() => None,
        Some(value) => Some(expect_fixnum(value)?),
    };

    let Some(buf) = ctx.buffers.current_buffer() else {
        return Ok(Value::NIL);
    };
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let read_only =
        crate::emacs_core::editfns::buffer_read_only_active_in_state(&ctx.obarray, &[], buf);
    if !read_only {
        return Ok(Value::NIL);
    }
    let pos = position.unwrap_or_else(|| buf.point_lisp_char_pos().as_i64());
    if pos < point_min {
        return Err(signal(
            LispCondition::ArgsOutOfRange,
            vec![Value::fixnum(pos), Value::fixnum(pos)],
        ));
    }
    let prop_byte = buf.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(pos));
    if buf
        .text_props_get_property_at_emacs_byte_pos(prop_byte, Value::symbol("inhibit-read-only"))
        .is_some_and(|value| value.is_truthy())
    {
        return Ok(Value::NIL);
    }
    Err(signal(
        LispCondition::BufferReadOnly,
        vec![Value::make_buffer(buf.id)],
    ))
}

pub(crate) fn builtin_bury_buffer_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("bury-buffer-internal", &args, 1)?;
    let id = expect_buffer_id(&args[0])?;
    let mut moved = false;
    if eval.buffers.get(id).is_some() {
        // Move to end of global buffer order (Vbuffer_alist equivalent).
        eval.buffers.note_buffer_order_tail(id);
        // Update frame buffer lists (GNU buffer.c:2259-2262).
        if let Some(frame) = eval.frames.selected_frame_mut() {
            frame.buffer_list.retain(|bid| *bid != id);
            frame.buried_buffer_list.retain(|bid| *bid != id);
            frame.buried_buffer_list.insert(0, id);
        }
        moved = true;
    }
    // GNU `Fbury_buffer_internal` (buffer.c:2264) runs
    // `buffer-list-update-hook' after moving BUFFER to the end of the buffer
    // lists, unless that buffer has hooks inhibited.
    if moved && !eval.buffers.buffer_hooks_inhibited(id) {
        crate::emacs_core::buffer::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_cancel_kbd_macro_events(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("cancel-kbd-macro-events", &args, 0)?;
    eval.cancel_kbd_macro_runtime_events();
    Ok(Value::NIL)
}

pub(crate) fn builtin_combine_after_change_execute(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("combine-after-change-execute", &args, 0)?;
    crate::emacs_core::editfns::execute_combined_after_change(eval)?;
    Ok(Value::NIL)
}

fn resolve_print_target(eval: &super::eval::Context, printcharfun: Option<&Value>) -> Value {
    match printcharfun {
        Some(dest) if !dest.is_nil() => *dest,
        _ => dynamic_or_global_symbol_value(eval, "standard-output").unwrap_or(Value::T),
    }
}

pub(crate) fn resolve_print_target_in_state(
    ctx: &crate::emacs_core::eval::Context,
    printcharfun: Option<&Value>,
) -> Value {
    match printcharfun {
        Some(dest) if !dest.is_nil() => *dest,
        _ => ctx
            .obarray
            .symbol_value("standard-output")
            .cloned()
            .unwrap_or(Value::T),
    }
}

/// The buffer a (resolved) print target writes into, or `None` when the target
/// is the echo area / `t` / a printer function.
fn print_target_buffer_id(
    ctx: &crate::emacs_core::eval::Context,
    target: Value,
) -> Option<crate::buffer::BufferId> {
    match target.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => target.as_buffer_id(),
        ValueKind::String => {
            let name = runtime_string_value(target);
            ctx.buffers.find_buffer_by_name(&name)
        }
        _ if super::marker::is_marker(&target) => {
            let (buffer_id, _, _) = super::marker::marker_logical_fields(&target)?;
            buffer_id
        }
        _ => None,
    }
}

/// The buffer GNU has **current** while the printer dereferences its `print-*`
/// globals for this target.
///
/// `PRINTPREPARE` (`src/print.c`) does `set_buffer_internal` on a buffer stream
/// before printing into it, so for a buffer destination it is that buffer's
/// bindings that apply and the caller's buffer-local `print-level` is swapped
/// out; a function / `t` / echo-area stream performs no switch, so the caller's
/// buffer stays current and its bindings do apply. Ledger 196.
fn print_target_current_buffer<'a>(
    ctx: &'a crate::emacs_core::eval::Context,
    target: Value,
) -> Option<&'a crate::buffer::Buffer> {
    match print_target_buffer_id(ctx, target) {
        Some(id) => ctx.buffers.get(id),
        None => ctx.buffers.current_buffer(),
    }
}

/// The multibyteness of the buffer that a (resolved) print target writes into,
/// or `None` when the target is the echo area / `t` / a printer function (where
/// GNU's `print_prepare` leaves the `print-escape-*` variables untouched).
fn print_target_buffer_multibyte(
    ctx: &crate::emacs_core::eval::Context,
    target: Value,
) -> Option<bool> {
    let id = print_target_buffer_id(ctx, target)?;
    ctx.buffers.get(id).map(|buf| buf.get_multibyte())
}

/// Apply GNU `print_prepare`'s implicit binding of the `print-escape-*` flags to
/// `options` for a buffer/marker print target (print.c lines ~170-177): when the
/// destination buffer is unibyte and `print-escape-multibyte` is unset, bind it
/// to `t`; when it is multibyte and `print-escape-nonascii` is unset, bind that.
///
/// In particular, a unibyte string printed into a unibyte buffer keeps
/// `print-escape-nonascii` nil, so its high bytes are emitted raw (not octal),
/// while printing into a multibyte buffer escapes them.
fn apply_print_target_escape_bindings(
    ctx: &crate::emacs_core::eval::Context,
    target: Value,
    options: &mut super::print::PrintOptions,
) {
    let Some(target_multibyte) = print_target_buffer_multibyte(ctx, target) else {
        return;
    };
    if target_multibyte {
        if !options.print_escape_nonascii {
            options.print_escape_nonascii = true;
        }
    } else if !options.print_escape_multibyte {
        options.print_escape_multibyte = true;
    }
}

/// `text` is read AFTER `signal_before_text_change`, i.e. after arbitrary
/// Lisp. Sound today because every caller passes a borrow of a local it owns
/// outright -- the printer's freshly built output -- which no hook can reach.
/// DIVERGENCES.md 163 §10 named this a latent trap because the signature does
/// not say so; 164 shows the shape that does, at the `insert` door: carry the
/// `Value`, root it on the specpdl, and take the borrow past the safepoint
/// (`PendingInsert` in `emacs_core/editing/buffer/mod.rs`).
fn insert_print_lisp_string_with_hooks(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    text: &crate::heap_types::LispString,
    before_markers: bool,
) -> Result<(), Flow> {
    if text.is_empty() {
        return Ok(());
    }

    let (insert_pos, insert_char_pos) = ctx
        .buffers
        .get(buffer_id)
        .map(|buf| (buf.point_emacs_byte_pos(), buf.point_char_pos()))
        .ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Output buffer no longer exists")],
            )
        })?;
    let change = super::editfns::text_change_for_empty_insertion_at_emacs_byte_pos(
        &ctx.buffers,
        buffer_id,
        insert_pos,
        super::editfns::lisp_string_text_extent(text),
    )?;

    super::editfns::signal_before_text_change(ctx, change)?;
    let inserted = if before_markers {
        ctx.buffers
            .insert_lisp_string_into_buffer_before_markers(buffer_id, text)
    } else {
        ctx.buffers.insert_lisp_string_into_buffer(buffer_id, text)
    };
    inserted.ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Output buffer no longer exists")],
        )
    })?;
    let _ = ctx
        .buffers
        .clear_inserted_plain_text_properties_in_char_range(
            buffer_id,
            CharRange::from_start_len(insert_char_pos, CharLen::new(text.schars())),
        );
    super::editfns::signal_after_text_change(ctx, change)?;
    Ok(())
}

fn insert_print_lisp_string_to_buffer_target(
    ctx: &mut crate::emacs_core::eval::Context,
    buffer_id: crate::buffer::BufferId,
    text: &crate::heap_types::LispString,
) -> Result<(), Flow> {
    if ctx.buffers.get(buffer_id).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Output buffer no longer exists")],
        ));
    }

    let saved_current = ctx.buffers.current_buffer_id();
    let result = (|| -> Result<(), Flow> {
        ctx.set_current_buffer_unrecorded(buffer_id)?;
        insert_print_lisp_string_with_hooks(ctx, buffer_id, text, false)
    })();
    if let Some(saved_id) = saved_current {
        ctx.restore_current_buffer_if_live(saved_id);
    }
    result
}

fn insert_print_lisp_string_to_marker_target(
    ctx: &mut crate::emacs_core::eval::Context,
    target: Value,
    text: &crate::heap_types::LispString,
) -> Result<(), Flow> {
    let Some((Some(buffer_id), _, _)) = super::marker::marker_logical_fields(&target) else {
        return Err(signal(
            "error",
            vec![Value::string("Marker does not point anywhere")],
        ));
    };
    let marker_pos = super::marker::marker_position_as_int_with_buffers(&ctx.buffers, &target)?;
    let Some(buffer) = ctx.buffers.get(buffer_id) else {
        return Err(signal(
            "error",
            vec![Value::string("Output buffer no longer exists")],
        ));
    };
    let min_pos = buffer.point_min_lisp_char_pos().as_i64();
    let max_pos = buffer.point_max_lisp_char_pos().as_i64();
    if marker_pos < min_pos || marker_pos > max_pos {
        return Err(signal(
            "error",
            vec![Value::string(
                "Marker is outside the accessible part of the buffer",
            )],
        ));
    }

    let marker_byte = buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(marker_pos));
    let old_target_point = buffer.point_emacs_byte_pos();
    let saved_current = ctx.buffers.current_buffer_id();

    let result = (|| -> Result<(), Flow> {
        ctx.set_current_buffer_unrecorded(buffer_id)?;
        let _ = ctx
            .buffers
            .goto_buffer_emacs_byte_pos(buffer_id, marker_byte);
        insert_print_lisp_string_with_hooks(ctx, buffer_id, text, false)?;

        let new_marker_pos = ctx
            .buffers
            .get(buffer_id)
            .map(|buf| buf.point_lisp_char_pos().as_i64())
            .ok_or_else(|| {
                signal(
                    "error",
                    vec![Value::string("Output buffer no longer exists")],
                )
            })?;
        let _ = super::marker::builtin_set_marker_in_buffers(
            &mut ctx.buffers,
            vec![
                target,
                Value::fixnum(new_marker_pos),
                Value::make_buffer(buffer_id),
            ],
        )?;
        Ok(())
    })();

    if result.is_ok() && ctx.buffers.get(buffer_id).is_some() {
        let inserted_len = EmacsByteLen::new(text.sbytes());
        let restore_point = if old_target_point >= marker_byte {
            old_target_point.add_len(inserted_len)
        } else {
            old_target_point
        };
        let _ = ctx
            .buffers
            .goto_buffer_emacs_byte_pos(buffer_id, restore_point);
    }
    if let Some(saved_id) = saved_current {
        ctx.restore_current_buffer_if_live(saved_id);
    }
    result
}

fn write_print_output_to_target(
    ctx: &mut crate::emacs_core::eval::Context,
    target: Value,
    text: &str,
) -> Result<(), Flow> {
    let text = crate::heap_types::LispString::from_utf8(text);
    match target.kind() {
        // GNU print.c: when printcharfun is t, output goes through
        // setup_echo_area_for_printing.  A preceding message resets
        // message_buf_print, so the next print starts with a fresh echo buffer;
        // later print calls append to that buffer.
        ValueKind::T | ValueKind::Nil => {
            ctx.append_to_log_fragment(&text);
            ctx.append_echo_area_print_lisp_string(&text);
            Ok(())
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = target.as_buffer_id().unwrap();
            insert_print_lisp_string_to_buffer_target(ctx, id, &text)
        }
        ValueKind::String => {
            let name = runtime_string_value(target);
            let Some(id) = ctx.buffers.find_buffer_by_name(&name) else {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("No buffer named {name}"))],
                ));
            };
            insert_print_lisp_string_to_buffer_target(ctx, id, &text)
        }
        _other if super::marker::is_marker(&target) => {
            insert_print_lisp_string_to_marker_target(ctx, target, &text)
        }
        _ => Ok(()),
    }
}

/// Emacs-bytes print sink — the byte-faithful sibling of
/// [`write_print_output_to_target`].
///
/// Issue #131: `prin1`/`print`/`write-char` produce their output in the
/// canonical Emacs internal encoding (`CHAR_STRING`), where eight-bit raw bytes
/// (0x3FFF80..) and non-Unicode codes (>0x10FFFF) are *disjoint* extended
/// sequences — never an in-Unicode Private-Use sentinel.  Inserting those bytes
/// straight through the `LispString`/`insert_lisp_string` path keeps a real PUA
/// glyph (e.g. a nerd-font icon U+E0A0 → \xee\x82\xa0) distinct from a raw byte
/// 0xA0, which the legacy storage-string sink (`write_print_output_to_target`,
/// still used by `princ`/`terpri`) cannot disambiguate.  `from_emacs_bytes`
/// preserves the byte sequence verbatim, so `bytes.len()` is exactly the number
/// of bytes inserted.
fn write_print_bytes_to_target(
    ctx: &mut crate::emacs_core::eval::Context,
    target: Value,
    bytes: &[u8],
) -> Result<(), Flow> {
    let ls = crate::heap_types::LispString::from_emacs_bytes(bytes.to_vec());
    match target.kind() {
        ValueKind::T | ValueKind::Nil => {
            ctx.append_to_log_fragment(&ls);
            ctx.append_echo_area_print_lisp_string(&ls);
            Ok(())
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = target.as_buffer_id().unwrap();
            insert_print_lisp_string_to_buffer_target(ctx, id, &ls)
        }
        ValueKind::String => {
            let name = runtime_string_value(target);
            let Some(id) = ctx.buffers.find_buffer_by_name(&name) else {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("No buffer named {name}"))],
                ));
            };
            insert_print_lisp_string_to_buffer_target(ctx, id, &ls)
        }
        _other if super::marker::is_marker(&target) => {
            insert_print_lisp_string_to_marker_target(ctx, target, &ls)
        }
        _ => Ok(()),
    }
}

pub(crate) fn print_target_is_direct(target: Value) -> bool {
    (target.is_t() || target.is_nil() || target.is_buffer() || target.is_string())
        || super::marker::is_marker(&target)
}

/// Walk canonical Emacs internal-encoding bytes one character at a time and
/// invoke a callable print target with each code. Eight-bit raw bytes and
/// non-Unicode codes therefore surface as their real Emacs character codes,
/// never as lossy Rust Unicode replacements. Shared by `princ`, `prin1`, and
/// `print` when their target is a function.
pub(crate) fn dispatch_print_callback_emacs_chars(
    bytes: &[u8],
    mut emit_char: impl FnMut(Value) -> Result<(), Flow>,
) -> Result<(), Flow> {
    let mut pos = 0usize;
    while pos < bytes.len() {
        let (code, len) = crate::emacs_core::emacs_char::string_char(&bytes[pos..]);
        emit_char(Value::fixnum(code as i64))?;
        pos += len;
    }
    Ok(())
}

fn write_print_output_from_ctx(
    ctx: &mut crate::emacs_core::eval::Context,
    printcharfun: Option<&Value>,
    text: &str,
) -> Result<(), Flow> {
    let target = resolve_print_target_in_state(ctx, printcharfun);
    // GNU print.c: in batch mode, printcharfun=t writes to stdout
    if ctx.noninteractive() && (target.is_t() || target.is_nil()) {
        use std::io::Write;
        let _ = std::io::stdout().write_all(text.as_bytes());
        let _ = std::io::stdout().flush();
        return Ok(());
    }
    write_print_output_to_target(ctx, target, text)
}

/// Emacs-bytes variant of [`write_print_output_from_ctx`] used by
/// `prin1`/`print`/`write-char`, whose output is already canonical Emacs
/// internal encoding (see [`write_print_bytes_to_target`]).
fn write_print_bytes_from_ctx(
    ctx: &mut crate::emacs_core::eval::Context,
    printcharfun: Option<&Value>,
    bytes: &[u8],
) -> Result<(), Flow> {
    let target = resolve_print_target_in_state(ctx, printcharfun);
    // GNU print.c: in batch mode, printcharfun=t writes to stdout.
    if ctx.noninteractive() && (target.is_t() || target.is_nil()) {
        use std::io::Write;
        let _ = std::io::stdout().write_all(bytes);
        let _ = std::io::stdout().flush();
        return Ok(());
    }
    write_print_bytes_to_target(ctx, target, bytes)
}

fn write_terpri_output(eval: &mut super::eval::Context, target: Value) -> Result<(), Flow> {
    match target.kind() {
        ValueKind::T | ValueKind::Nil => {
            eval.append_echo_area_print_runtime_text("\n");
            Ok(())
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = target.as_buffer_id().unwrap();
            if eval.buffers.get(id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Output buffer no longer exists")],
                ));
            }
            let _ = eval.buffers.insert_into_buffer(id, "\n");
            Ok(())
        }
        ValueKind::String => {
            let name = runtime_string_value(target);
            let Some(id) = eval.buffers.find_buffer_by_name(&name) else {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("No buffer named {name}"))],
                ));
            };
            if eval.buffers.get(id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Output buffer no longer exists")],
                ));
            }
            let _ = eval.buffers.insert_into_buffer(id, "\n");
            Ok(())
        }
        _other => {
            // Root the callable target across eval.apply().
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(target);
            let call_result = eval.apply(target, vec![Value::fixnum('\n' as i64)]);
            eval.restore_specpdl_roots(roots);
            call_result?;
            Ok(())
        }
    }
}

/// Issue #131: render the `princ` form of `value` as canonical Emacs
/// internal-encoding bytes — a string emits its bytes verbatim (a real
/// Private-Use glyph survives instead of being decoded as a raw byte), symbol /
/// buffer names emit their name bytes, and cons / vector / record recurse with
/// `princ` semantics. Opaque handles fall back to the byte `prin1` sink. This is
/// the sole `princ` producer (the former storage-string `print_value_princ*`
/// functions have been retired).
pub(crate) fn print_value_princ_bytes(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Vec<u8> {
    // Top-level `%s`/`princ`: a string ARGUMENT is inserted by `Fformat` /
    // `princ` directly, without `print_object`, so its raw bytes pass through
    // verbatim. Non-string arguments are printed via `print_object`, which
    // octal-escapes any eight-bit bytes in nested strings.
    print_value_princ_bytes_inner(ctx, value, false)
}

/// Render `value` as GNU `Fprinc` does when its print target is a multibyte
/// buffer, notably `Vprin1_to_string_buffer` in `error-message-string`.
///
/// A top-level unibyte string is the one destination-sensitive case: `%s`
/// returns its raw bytes, while `Fprinc` into a multibyte buffer octal-escapes
/// eight-bit bytes.  Every non-string object follows the ordinary recursive
/// `princ` printer, which already gives live buffers their names and applies
/// nested no-escape semantics.
pub(crate) fn print_value_princ_bytes_to_multibyte_buffer(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Vec<u8> {
    if let Some(string) = value.as_lisp_string() {
        return if string.is_multibyte() {
            string.as_bytes().to_vec()
        } else {
            crate::emacs_core::string_escape::octal_escape_unibyte_eight_bit(string.as_bytes())
        };
    }
    print_value_princ_bytes_inner(ctx, value, false)
}

/// `nested` is true for elements reached through an aggregate (list, vector,
/// record, byte-code object, …), i.e. printed by GNU's
/// `print_object (…, escapeflag=false)`. In that path, GNU's `print_string`
/// octal-escapes eight-bit bytes (`\NNN`) even though it omits the surrounding
/// quotes; only a TOP-LEVEL string argument to `%s`/`princ` is emitted raw.
fn print_value_princ_bytes_inner(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    nested: bool,
) -> Vec<u8> {
    let print_quoted = ctx
        .obarray
        .symbol_value("print-quoted")
        .is_none_or(|v| v.is_truthy());
    let prin1_bytes = |v: &Value| {
        super::error::print_value_bytes_in_state(
            &ctx.obarray,
            &ctx.buffers,
            &ctx.frames,
            &ctx.threads,
            v,
        )
    };
    let recurse = |v: &Value| print_value_princ_bytes_inner(ctx, v, true);
    if super::terminal::pure::print_terminal_handle(value).is_some()
        || ctx.threads.thread_id_from_handle(value).is_some()
        || ctx.threads.mutex_id_from_handle(value).is_some()
        || ctx
            .threads
            .condition_variable_id_from_handle(value)
            .is_some()
    {
        return prin1_bytes(value);
    }
    match value.kind() {
        ValueKind::String => value
            .as_lisp_string()
            .map(|ls| {
                if ls.is_multibyte() {
                    // Already canonical Emacs internal encoding — a real
                    // Private-Use glyph survives verbatim (issue #131).
                    ls.as_bytes().to_vec()
                } else if nested {
                    // Nested under `print_object` (escapeflag=false): GNU's
                    // `print_string` octal-escapes the raw eight-bit bytes
                    // (`\NNN`) while still omitting the surrounding quotes. This
                    // is what makes `(format "%s" (byte-compile …))` render the
                    // code string as `\211\300…` rather than the raw bytes.
                    crate::emacs_core::string_escape::octal_escape_unibyte_eight_bit(ls.as_bytes())
                } else {
                    // A unibyte string's raw bytes are not a valid multibyte
                    // sequence; promote high bytes to eight-bit characters so
                    // the canonical-bytes consumer gets well-formed Emacs bytes.
                    crate::emacs_core::emacs_char::str_to_multibyte(ls.as_bytes())
                }
            })
            .unwrap_or_default(),
        ValueKind::Symbol(id) => resolve_sym(id).as_bytes().to_vec(),
        ValueKind::Veclike(VecLikeType::Buffer) => {
            let id = value.as_buffer_id().unwrap();
            if let Some(buf) = ctx.buffers.get(id) {
                return buf.name_runtime_string_owned().into_bytes();
            }
            if ctx.buffers.dead_buffer_last_name_value(id).is_some() {
                return b"#<killed buffer>".to_vec();
            }
            prin1_bytes(value)
        }
        ValueKind::Cons => {
            if let Some(shorthand) =
                print_value_princ_bytes_list_shorthand(value, print_quoted, &recurse)
            {
                return shorthand;
            }
            let mut out = vec![b'('];
            let mut cursor = *value;
            let mut first = true;
            loop {
                match cursor.kind() {
                    ValueKind::Cons => {
                        if !first {
                            out.push(b' ');
                        }
                        let pair_car = cursor.cons_car();
                        let pair_cdr = cursor.cons_cdr();
                        out.extend_from_slice(&recurse(&pair_car));
                        cursor = pair_cdr;
                        first = false;
                    }
                    ValueKind::Nil => break,
                    _other => {
                        if !first {
                            out.extend_from_slice(b" . ");
                        }
                        out.extend_from_slice(&recurse(&cursor));
                        break;
                    }
                }
            }
            out.push(b')');
            out
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            if super::chartable::bool_vector_length(value).is_some() {
                return prin1_bytes(value);
            }
            let items = value.as_vector_data().unwrap().clone();
            let mut out = vec![b'['];
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.extend_from_slice(&recurse(item));
            }
            out.push(b']');
            out
        }
        ValueKind::Veclike(VecLikeType::Record) => {
            let items = value.as_record_data().unwrap().clone();
            let mut out = b"#s(".to_vec();
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b' ');
                }
                out.extend_from_slice(&recurse(item));
            }
            out.push(b')');
            out
        }
        // Interpreted-function closures are PVEC_CLOSURE in GNU and use the
        // same readable `#[...]` traversal as vectors, with `escapeflag=false`
        // propagated to every slot (src/print.c:2599-2614).
        ValueKind::Veclike(VecLikeType::Lambda) => {
            let mut out = b"#[".to_vec();
            if let Some(slots) = value.closure_slots() {
                for (i, item) in slots.iter().enumerate() {
                    if i > 0 {
                        out.push(b' ');
                    }
                    out.extend_from_slice(&recurse(item));
                }
            }
            out.push(b']');
            out
        }
        // Byte-code objects are printed by GNU's `princ` through
        // `print_object (obj, printcharfun, escapeflag=false)`
        // (`src/print.c`), recursing into the slots WITHOUT the escape flag, so
        // nested strings drop their surrounding quotes — but eight-bit bytes are
        // still octal-escaped by `print_string` (handled by the `nested` String
        // arm above). Previously these fell through to `prin1_bytes`, which kept
        // the quotes: `(format "%s" (byte-compile …))` printed `#[257 "\211…"]`
        // where GNU prints `#[257 \211…]`. Recurse over the byte-code literal
        // slots in princ mode to match GNU byte-for-byte.
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            let mut out = b"#[".to_vec();
            if let Some(slot_bytes) =
                super::print::with_bytecode_literal_slots_public(value, |slots| {
                    let mut inner = Vec::new();
                    for (i, item) in slots.iter().enumerate() {
                        if i > 0 {
                            inner.push(b' ');
                        }
                        inner.extend_from_slice(&recurse(item));
                    }
                    inner
                })
            {
                out.extend_from_slice(&slot_bytes);
                out.push(b']');
                out
            } else {
                prin1_bytes(value)
            }
        }
        _other => prin1_bytes(value),
    }
}

/// Recognise the `'x` / `#'x` / `` `x `` / `,x` / `,@x` two-element-list
/// shorthands for [`print_value_princ_bytes`] and render them as Emacs bytes.
fn print_value_princ_bytes_list_shorthand(
    value: &Value,
    print_quoted: bool,
    render: &dyn Fn(&Value) -> Vec<u8>,
) -> Option<Vec<u8>> {
    if !print_quoted {
        return None;
    }

    let items = super::value::list_to_vec(value)?;
    if items.len() != 2 {
        return None;
    }

    let head = match items[0].kind() {
        ValueKind::Symbol(id) => resolve_sym(id),
        _ => return None,
    };
    let prefix: &[u8] = match head {
        "quote" => b"'",
        "function" => b"#'",
        "`" => b"`",
        "," => b",",
        ",@" => b",@",
        _ => return None,
    };
    let mut out = prefix.to_vec();
    out.extend_from_slice(&render(&items[1]));
    Some(out)
}

fn print_options_from_overrides(
    ctx: &super::eval::Context,
    buf: Option<&crate::buffer::Buffer>,
    overrides: Option<&Value>,
) -> Result<super::print::PrintOptions, Flow> {
    let mut options = super::error::print_options_from_state(&ctx.obarray, buf);
    if let Some(overrides) = overrides.filter(|v| !v.is_nil()) {
        apply_print_overrides(&mut options, *overrides)?;
    }
    Ok(options)
}

fn reset_print_options(options: &mut super::print::PrintOptions) {
    *options = super::print::PrintOptions::default();
}

fn apply_print_overrides(
    options: &mut super::print::PrintOptions,
    mut overrides: Value,
) -> Result<(), Flow> {
    if overrides == Value::T {
        reset_print_options(options);
        return Ok(());
    }

    while !overrides.is_nil() {
        if !overrides.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("consp"), overrides],
            ));
        }

        let setting = overrides.cons_car();
        if setting == Value::T {
            reset_print_options(options);
        } else {
            if !setting.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), setting],
                ));
            }
            apply_print_override_setting(options, setting.cons_car(), setting.cons_cdr())?;
        }

        overrides = overrides.cons_cdr();
    }

    Ok(())
}

fn apply_print_override_setting(
    options: &mut super::print::PrintOptions,
    key: Value,
    value: Value,
) -> Result<(), Flow> {
    let ValueKind::Symbol(id) = key.kind() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), key],
        ));
    };

    match resolve_sym(id) {
        "length" => {
            options.print_length = value.as_fixnum().filter(|n| *n >= 0);
        }
        "level" => {
            options.print_level = value.as_fixnum().filter(|n| *n >= 0);
        }
        "circle" => {
            options.print_circle = value.is_truthy();
        }
        "escape-newlines" => {
            options.print_escape_newlines = value.is_truthy();
        }
        "escape-control-characters" => {
            options.print_escape_control_characters = value.is_truthy();
        }
        "escape-nonascii" => {
            options.print_escape_nonascii = value.is_truthy();
        }
        "escape-multibyte" => {
            options.print_escape_multibyte = value.is_truthy();
        }
        "gensym" => {
            options.print_gensym = value.is_truthy();
        }
        "quoted" => {
            options.print_quoted = value.is_truthy();
        }
        "continuous-numbering" => {
            options.print_continuous_numbering = value.is_truthy();
            if !options.print_continuous_numbering {
                options.print_number_table = None;
            }
        }
        "symbols-bare" => {
            options.print_symbols_bare = value.is_truthy();
        }
        "number-table" => {
            options.print_number_table = value.is_hash_table().then_some(value);
        }
        "float-format" => {
            options.float_output_format = value.is_string().then_some(value);
        }
        // GNU accepts these override keys by dynamically binding print
        // variables that Neomacs does not yet model in PrintOptions.
        "charset-text-property"
        | "unreadable-function"
        | "unreadeable-function"
        | "integers-as-characters" => {}
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), key],
            ));
        }
    }

    Ok(())
}

fn prin1_to_lisp_string_value_in_state_with_overrides(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
    noescape: bool,
    overrides: Option<&Value>,
) -> Result<crate::heap_types::LispString, Flow> {
    // GNU's `Fprin1_to_string` prints into `Vprin1_to_string_buffer`, so
    // `PRINTPREPARE` makes THAT buffer current and the caller's buffer-local
    // `print-level` / `print-length` never apply (ledger 196).
    let mut options = print_options_from_overrides(ctx, None, overrides)?;
    options.print_noescape = noescape;
    // GNU `prin1-to-string' prints into `Vprin1_to_string_buffer', which is a
    // multibyte buffer, so `print_prepare' binds `print-escape-nonascii' to t
    // (print.c).  A unibyte string's high bytes are therefore octal-escaped in
    // the resulting (multibyte) string, matching `(prin1 ... multibyte-buffer)'.
    if !options.print_escape_nonascii {
        options.print_escape_nonascii = true;
    }
    Ok(crate::heap_types::LispString::from_emacs_bytes(
        super::error::format_value_bytes_in_state_with_options(
            &ctx.obarray,
            &ctx.buffers,
            &ctx.frames,
            &ctx.threads,
            value,
            options,
        ),
    ))
}

pub(crate) fn builtin_princ(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("princ", &args, 1)?;
    let target = resolve_print_target(eval, args.get(1));
    if print_target_is_direct(target) {
        return builtin_princ_impl(eval, args);
    }

    let bytes = print_value_princ_bytes(eval, &args[0]);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(target);
    let princ_result =
        dispatch_print_callback_emacs_chars(&bytes, |ch| eval.apply(target, vec![ch]).map(|_| ()));
    eval.restore_specpdl_roots(roots);
    princ_result?;
    Ok(args[0])
}

pub(crate) fn builtin_princ_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("princ", &args, 1)?;
    // Issue #131: emit canonical Emacs bytes directly. A real Private-Use glyph
    // is inserted as itself, while genuine eight-bit / non-Unicode content is
    // carried as its disjoint extended encoding — neither is ever mistaken for
    // the other, retiring the storage-string sink princ used to fall back to.
    let bytes = print_value_princ_bytes(ctx, &args[0]);
    write_print_bytes_from_ctx(ctx, args.get(1), &bytes)?;
    Ok(args[0])
}

/// GNU `print_object`/`print_preprocess` build `Vprint_number_table` (a real
/// `eq` hash table) inside the printer when `print-circle' is set, and -- when
/// `print-continuous-numbering' is also set -- keep it (and `print_number_index`)
/// alive across successive print calls so shared structure retains its `#N='
/// labels (src/print.c:1296-1322, 1444-1445).  The neomacs printer only takes
/// the persistent (Lisp-variable) path when `print-number-table' is already a
/// hash table; otherwise it uses a throwaway internal table.  To match GNU,
/// materialize and bind the table here -- before printing -- whenever both
/// variables are active and the variable does not yet hold a hash table.  The
/// table's *contents* then persist automatically (it is a shared heap object),
/// so the next call in the same `print-continuous-numbering' scope reuses the
/// existing `#N#' references instead of re-emitting `#N=' prefixes.
pub(crate) fn ensure_continuous_print_number_table(ctx: &mut crate::emacs_core::eval::Context) {
    let continuous = ctx
        .obarray
        .symbol_value("print-continuous-numbering")
        .is_some_and(|v| v.is_truthy());
    let circle = ctx
        .obarray
        .symbol_value("print-circle")
        .is_some_and(|v| v.is_truthy());
    if !(continuous && circle) {
        return;
    }
    let already_table = ctx
        .obarray
        .symbol_value("print-number-table")
        .is_some_and(|v| v.is_hash_table());
    if already_table {
        return;
    }
    ctx.set_variable(
        "print-number-table",
        Value::hash_table(crate::emacs_core::value::HashTableTest::Eq),
    );
    // GNU `print_object' resets `print_number_index' to 0 whenever it (re)builds
    // a nil `Vprint_number_table' (src/print.c:1300-1305).  Mirror that so the
    // first shared object in this continuous-numbering scope is labelled `#1='.
    crate::emacs_core::print::reset_print_number_index();
}

pub(crate) fn builtin_prin1(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("prin1", &args, 1)?;
    ensure_continuous_print_number_table(eval);
    let target = resolve_print_target(eval, args.get(1));
    let options =
        print_options_from_overrides(eval, print_target_current_buffer(eval, target), args.get(2))?;
    if print_target_is_direct(target) {
        return builtin_prin1_impl(eval, args);
    }

    // GNU's callable print target receives Emacs character codes one at a time.
    // Keep the printer's canonical Emacs bytes intact until that boundary: a
    // Rust String cannot represent byte8 or non-Unicode Emacs characters and
    // would silently replace them before `external-debugging-output` (or any
    // other character sink) sees them.
    let bytes = super::error::print_value_bytes_in_state_with_options(eval, &args[0], options);
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(target);
    let prin1_result =
        dispatch_print_callback_emacs_chars(&bytes, |ch| eval.apply(target, vec![ch]).map(|_| ()));
    eval.restore_specpdl_roots(roots);
    prin1_result?;
    Ok(args[0])
}

pub(crate) fn builtin_prin1_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("prin1", &args, 1)?;
    ensure_continuous_print_number_table(ctx);
    let target = resolve_print_target_in_state(ctx, args.get(1));
    let mut options =
        print_options_from_overrides(ctx, print_target_current_buffer(ctx, target), args.get(2))?;
    // GNU `print_prepare' implicitly binds `print-escape-nonascii' /
    // `print-escape-multibyte' based on the destination buffer's multibyteness,
    // so e.g. a unibyte string's high bytes print raw into a unibyte buffer but
    // octal-escaped into a multibyte buffer.
    apply_print_target_escape_bindings(ctx, target, &mut options);
    // Issue #131: emit canonical Emacs bytes so a real Private-Use glyph in the
    // printed output is inserted as itself, not decoded as a raw byte.
    let bytes = super::error::print_value_bytes_in_state_with_options(ctx, &args[0], options);
    write_print_bytes_from_ctx(ctx, args.get(1), &bytes)?;
    Ok(args[0])
}

pub(crate) fn builtin_prin1_to_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    ensure_continuous_print_number_table(eval);
    builtin_prin1_to_string_impl(eval, args)
}

pub(crate) fn builtin_prin1_to_string_impl(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("prin1-to-string", &args, 1)?;
    let noescape = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(Value::heap_string(
        prin1_to_lisp_string_value_in_state_with_overrides(ctx, &args[0], noescape, args.get(2))?,
    ))
}

pub(crate) fn builtin_print(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("print", &args, 1)?;
    ensure_continuous_print_number_table(eval);
    let target = resolve_print_target(eval, args.get(1));
    if print_target_is_direct(target) {
        return builtin_print_impl(eval, args);
    }

    // A function stream performs no `set_buffer_internal`, so GNU reads its
    // `print-*` globals with the CALLER's buffer current -- the same rule
    // `builtin_prin1`'s function path follows. Ledger 196.
    let options = super::error::print_options_from_state(
        &eval.obarray,
        print_target_current_buffer(eval, target),
    );
    let mut bytes = Vec::new();
    bytes.push(b'\n');
    bytes.extend_from_slice(&super::error::print_value_bytes_in_state_with_options(
        eval, &args[0], options,
    ));
    bytes.push(b'\n');
    let roots = eval.save_specpdl_roots();
    eval.push_specpdl_root(target);
    let print_result =
        dispatch_print_callback_emacs_chars(&bytes, |ch| eval.apply(target, vec![ch]).map(|_| ()));
    eval.restore_specpdl_roots(roots);
    print_result?;
    Ok(args[0])
}

pub(crate) fn builtin_print_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("print", &args, 1)?;
    ensure_continuous_print_number_table(ctx);
    // GNU `print_prepare' binds the `print-escape-*' flags from the destination
    // buffer's multibyteness (see `builtin_prin1_impl`).
    let target = resolve_print_target_in_state(ctx, args.get(1));
    let mut options = super::error::print_options_from_state(
        &ctx.obarray,
        print_target_current_buffer(ctx, target),
    );
    apply_print_target_escape_bindings(ctx, target, &mut options);
    // Issue #131: emit canonical Emacs bytes (see `builtin_prin1_impl`).
    let mut bytes = Vec::new();
    bytes.push(b'\n');
    bytes.extend_from_slice(&super::error::print_value_bytes_in_state_with_options(
        ctx, &args[0], options,
    ));
    bytes.push(b'\n');
    write_print_bytes_from_ctx(ctx, args.get(1), &bytes)?;
    Ok(args[0])
}

pub(crate) fn builtin_terpri(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = builtin_terpri_impl(eval, args.clone())? {
        return Ok(result);
    }
    finish_terpri_in_eval(eval, &args)
}

pub(crate) fn builtin_terpri_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> Result<Option<Value>, Flow> {
    expect_max_args("terpri", &args, 2)?;
    let target = resolve_print_target_in_state(ctx, args.first());
    if print_target_is_direct(target) {
        write_print_output_from_ctx(ctx, args.first(), "\n")?;
        return Ok(Some(Value::T));
    }
    Ok(None)
}

pub(crate) fn finish_terpri_in_eval(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_max_args("terpri", args, 2)?;
    let target = resolve_print_target(eval, args.first());
    write_terpri_output(eval, target)?;
    Ok(Value::T)
}

/// `write-char`'s output character as canonical Emacs internal-encoding bytes
/// (`CHAR_STRING`).  Returns `None` for codes outside `0..=MAX_CHAR`.
///
/// Issue #131: produces the disjoint extended encoding directly via `EmacsChar` —
/// eight-bit raw bytes and non-Unicode codes never collide with a real Private-Use
/// glyph, so writing a nerd-font icon stores the glyph itself rather than packing
/// it into an in-Unicode storage sentinel.
fn write_char_emacs_bytes(char_code: i64) -> Option<Vec<u8>> {
    u32::try_from(char_code)
        .ok()
        .and_then(crate::emacs_core::emacs_char::EmacsChar::from_code)
        .map(|c| c.to_emacs_bytes())
}

pub(crate) fn builtin_write_char(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    if let Some(result) = builtin_write_char_impl(eval, args.clone())? {
        return Ok(result);
    }
    finish_write_char_in_eval(eval, &args)
}

pub(crate) fn finish_write_char_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    expect_args_range("write-char", args, 1, 2)?;
    let char_code = expect_fixnum(&args[0])?;
    let target = resolve_print_target(eval, args.get(1));

    match target.kind() {
        ValueKind::T | ValueKind::Nil => {}
        ValueKind::Veclike(VecLikeType::Buffer) => {
            if let Some(bytes) = write_char_emacs_bytes(char_code) {
                let id = target.as_buffer_id().unwrap();
                if eval.buffers.get(id).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Output buffer no longer exists")],
                    ));
                }
                let ls = crate::heap_types::LispString::from_emacs_bytes(bytes);
                let _ = eval.buffers.insert_lisp_string_into_buffer(id, &ls);
            }
        }
        ValueKind::String => {
            if let Some(bytes) = write_char_emacs_bytes(char_code) {
                let name = runtime_string_value(target);
                let Some(id) = eval.buffers.find_buffer_by_name(&name) else {
                    return Err(signal(
                        "error",
                        vec![Value::string(format!("No buffer named {name}"))],
                    ));
                };
                if eval.buffers.get(id).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Output buffer no longer exists")],
                    ));
                }
                let ls = crate::heap_types::LispString::from_emacs_bytes(bytes);
                let _ = eval.buffers.insert_lisp_string_into_buffer(id, &ls);
            }
        }
        _other => {
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(target);
            let call_result = eval.apply(target, vec![Value::fixnum(char_code)]);
            eval.restore_specpdl_roots(roots);
            call_result?;
        }
    }

    Ok(Value::fixnum(char_code))
}

pub(crate) fn builtin_write_char_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> Result<Option<Value>, Flow> {
    expect_args_range("write-char", &args, 1, 2)?;
    let char_code = expect_fixnum(&args[0])?;
    let target = resolve_print_target_in_state(ctx, args.get(1));

    if print_target_is_direct(target) {
        if let Some(bytes) = write_char_emacs_bytes(char_code) {
            write_print_bytes_from_ctx(ctx, args.get(1), &bytes)?;
        }
        return Ok(Some(Value::fixnum(char_code)));
    }

    Ok(None)
}

pub(crate) fn builtin_propertize(args: Vec<Value>) -> EvalResult {
    expect_min_args("propertize", &args, 1)?;

    let string = match args[0].kind() {
        ValueKind::String => args[0].as_lisp_string().expect("propertize string arg"),
        _ => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("stringp"), args[0]],
            ));
        }
    };

    // `propertize` requires an odd argument count: 1 string + plist pairs.
    if args.len().is_multiple_of(2) {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("propertize"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    // Issue #131: copy the source string's Emacs bytes verbatim rather than
    // round-tripping through the lossy storage-string form, which would decode a
    // real Private-Use glyph as a raw byte.
    let new_str = Value::heap_string(if string.is_multibyte() {
        crate::heap_types::LispString::from_emacs_bytes(string.as_bytes().to_vec())
    } else {
        crate::heap_types::LispString::from_unibyte(string.as_bytes().to_vec())
    });

    // Copy existing text properties from source string
    if args[0].is_string()
        && let Some(src_table) = get_string_text_properties_table_for_value(args[0])
    {
        // GNU `Fpropertize' starts with `Fcopy_sequence', whose string branch
        // copies the interval tree and each interval plist spine.  Sharing the
        // plist cons cells lets the properties added below mutate STRING.
        set_string_text_properties_table_for_value(new_str, src_table.copy_interval_plist_spines());
    }

    // Parse and apply plist properties.  GNU `propertize` reverses this plist
    // before calling `add-text-properties`, so select that typed application
    // policy rather than encoding the semantic distinction as iterator
    // direction at this call site.
    if args.len() > 1 {
        let char_len = new_str
            .as_lisp_string()
            .expect("new string must carry LispString payload")
            .schars();
        let mut table = get_string_text_properties_table_for_value(new_str).unwrap_or_default();
        let properties: Vec<_> = args[1..]
            .chunks_exact(2)
            .map(|chunk| (chunk[0], chunk[1]))
            .collect();
        table.apply_property_plist_for_object_char_len(
            CharRange::new(CharPos0::new(0), CharPos0::new(char_len)),
            CharLen::new(char_len),
            &properties,
            PropertyPlistApplication::PreserveSuppliedOrder,
        );
        set_string_text_properties_table_for_value(new_str, table);
    }

    Ok(new_str)
}

pub(crate) fn builtin_current_cpu_time(args: Vec<Value>) -> EvalResult {
    expect_args("current-cpu-time", &args, 0)?;

    // GNU timefns.c Fcurrent_cpu_time: (clock() . CLOCKS_PER_SEC) — CPU time
    // consumed by the process (all threads), not wall time, so a sleeping or
    // descheduled process accrues nothing.
    Ok(Value::cons(
        Value::fixnum(process_cpu_time_micros()),
        Value::fixnum(1_000_000),
    ))
}

fn process_cpu_time_micros() -> i64 {
    std::cfg_select! {
        unix => {
            // glibc clock() reads CLOCK_PROCESS_CPUTIME_ID truncated to
            // CLOCKS_PER_SEC (1e6) ticks; do the same directly.
            let mut ts = libc::timespec {
                tv_sec: 0,
                tv_nsec: 0,
            };
            if unsafe {
                libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut ts)
            } == 0 {
                ts.tv_sec * 1_000_000 + ts.tv_nsec / 1_000
            } else {
                0
            }
        }
        windows => {
            use windows_sys::Win32::Foundation::FILETIME;
            use windows_sys::Win32::System::Threading::{
                GetCurrentProcess, GetProcessTimes,
            };

            let mut creation = FILETIME::default();
            let mut exit = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            let ok = unsafe {
                GetProcessTimes(
                    GetCurrentProcess(),
                    &mut creation,
                    &mut exit,
                    &mut kernel,
                    &mut user,
                )
            };
            if ok == 0 {
                return 0;
            }

            let filetime_ticks = |time: FILETIME| {
                ((time.dwHighDateTime as u64) << 32) | time.dwLowDateTime as u64
            };
            // FILETIME counts 100 ns intervals. GNU-compatible ticks are
            // microseconds.
            let micros = filetime_ticks(kernel).saturating_add(filetime_ticks(user)) / 10;
            micros.min(i64::MAX as u64) as i64
        }
        _ => {
            0
        }
    }
}

pub(crate) fn builtin_current_idle_time(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-idle-time", &args, 0)?;
    Ok(eval.current_idle_time_value())
}
