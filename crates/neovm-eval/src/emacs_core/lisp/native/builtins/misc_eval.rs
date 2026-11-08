use super::*;
use neovm_edit::buffer::text_props::PropertyPlistApplication;
use neovm_edit::buffer::{CharLen, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, LispCharPos1};
use neovm_edit::emacs_core::edit_host::EditVariableRead;
#[doc(hidden)]
pub use neovm_value::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use neovm_value::symbol::Obarray;
use neovm_value::tagged::value::SymbolMode;

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_get_pos_property(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("get-pos-property", &args, 2, 3)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_get_pos_property_3(eval, arg(0), arg(1), arg(2))
}
/// `get-pos-property` as registered: fixed arity 3, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a3` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_get_pos_property_3(
    eval: &mut super::eval::Context,
    position: Value,
    prop: Value,
    object: Value,
) -> EvalResult {
    let args: [Value; 3] = [position, prop, object];
    neovm_edit::emacs_core::textprop::builtin_get_pos_property_impl(
        eval.variables(),
        &[],
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frames,
        )),
        &eval.edit.buffers,
        &args,
    )
}

#[doc(hidden)]
pub fn builtin_next_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_next_char_property_change_in_buffers(&eval.edit.buffers, args)
}

pub(crate) fn builtin_next_char_property_change_in_buffers(
    buffers: &neovm_edit::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("next-char-property-change", &args, 1)?;
    expect_max_args("next-char-property-change", &args, 2)?;

    // GNU: temp = next-overlay-change(POS); if LIMIT < temp, temp = LIMIT;
    // return next-property-change(POS, nil, temp).
    let overlay_next = neovm_edit::emacs_core::buffer::builtin_next_overlay_change_in_buffers(
        buffers,
        vec![args[0]],
    )?;
    let mut temp = overlay_next;
    if let Some(limit) = args.get(1)
        && !limit.is_nil()
    {
        let lim_int =
            neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, limit)?;
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

/// `pos-bol` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
#[doc(hidden)]
pub fn builtin_pos_bol_1(eval: &mut super::eval::Context, n: Value) -> EvalResult {
    let args: [Value; 1] = [n];
    expect_max_args("pos-bol", &args, 1)?;
    // GNU `Fpos_bol` (editfns.c:684) returns the unconstrained line-beginning
    // position; only `Fline_beginning_position` adds field constraints.
    let scan_count = super::navigation::line_beginning_scan_count_arg(&args)?;
    let (bol_charpos, _orig, _count) =
        super::navigation::pos_bol_compute(&eval.edit.buffers, scan_count)?;
    Ok(Value::from_fixnum(bol_charpos.into()))
}

/// `pos-eol` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
#[doc(hidden)]
pub fn builtin_pos_eol_1(eval: &mut super::eval::Context, n: Value) -> EvalResult {
    let args: [Value; 1] = [n];
    expect_max_args("pos-eol", &args, 1)?;
    // GNU `Fpos_eol` (editfns.c:740) returns the unconstrained line-end
    // position; only `Fline_end_position` adds field constraints.
    let scan_count = super::navigation::line_end_scan_count_arg(&args)?;
    let (eol_charpos, _orig) = super::navigation::pos_eol_compute(&eval.edit.buffers, scan_count)?;
    Ok(Value::from_fixnum(eol_charpos.into()))
}

#[doc(hidden)]
pub fn builtin_previous_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_property_change_in_buffers(&eval.edit.buffers, args)
}

pub(crate) fn builtin_previous_property_change_in_buffers(
    buffers: &neovm_edit::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-property-change", &args, 1)?;
    expect_max_args("previous-property-change", &args, 3)?;

    let pos =
        neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, &args[0])?;

    // --- String OBJECT ---
    if let Some(str_val) = args.get(1).filter(|v| v.is_string()) {
        let s = str_val
            .as_lisp_string()
            .expect("string object must carry LispString payload");
        // A read-only walk: borrow the live table rather than copying it.
        // The clone made `previous-property-change` O(intervals) per call
        // where GNU is O(log n) -- 2,000 calls on a 3,200-interval string
        // took 18.8ms against GNU's 1.0ms.  Nothing below runs Lisp, so the
        // borrow cannot be invalidated under us.
        let empty_table;
        let table =
            match neovm_value::value::borrow_string_text_properties_table_for_value(*str_val) {
                Some(table) => table,
                None => {
                    empty_table = neovm_edit::buffer::text_props::TextPropertyTable::new();
                    &empty_table
                }
            };
        let char_pos = textprop::validate_string_point_raw(s, pos, args[0])?;
        let (limit_pos, limit_val) = match args.get(2) {
            Some(v) if !v.is_nil() => {
                let lim_int = neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(
                    buffers, v,
                )?;
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
            .map(|b| b.id())
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")])),
        Some(v) if v.is_nil() => buffers
            .current_buffer()
            .map(|b| b.id())
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
            let limit =
                neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, v)?;
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

#[doc(hidden)]
pub fn builtin_previous_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_previous_char_property_change_in_buffers(&eval.edit.buffers, args)
}

pub(crate) fn builtin_previous_char_property_change_in_buffers(
    buffers: &neovm_edit::buffer::BufferManager,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("previous-char-property-change", &args, 1)?;
    expect_max_args("previous-char-property-change", &args, 2)?;

    // GNU: temp = previous-overlay-change(POS); if LIMIT > temp, temp = LIMIT;
    // return previous-property-change(POS, nil, temp).
    let overlay_prev = neovm_edit::emacs_core::buffer::builtin_previous_overlay_change_in_buffers(
        buffers,
        vec![args[0]],
    )?;
    let mut temp = overlay_prev;
    if let Some(limit) = args.get(1)
        && !limit.is_nil()
    {
        let lim_int =
            neovm_edit::emacs_core::buffer::expect_integer_or_marker_in_buffers(buffers, limit)?;
        if let Some(temp_int) = temp.as_fixnum()
            && lim_int > temp_int
        {
            temp = Value::fixnum(lim_int);
        }
    }
    builtin_previous_property_change_in_buffers(buffers, vec![args[0], Value::NIL, temp])
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_next_single_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_value::error::expect_args_range("next-single-char-property-change", &args, 2, 4)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_next_single_char_property_change_4(eval, arg(0), arg(1), arg(2), arg(3))
}
/// `next-single-char-property-change` as registered: fixed arity 4, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a4` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
#[doc(hidden)]
pub fn builtin_next_single_char_property_change_4(
    eval: &mut super::eval::Context,
    position: Value,
    prop: Value,
    object: Value,
    limit: Value,
) -> EvalResult {
    let args: [Value; 4] = [position, prop, object, limit];
    neovm_edit::emacs_core::textprop::builtin_next_single_char_property_change_in_buffers_with_symbols(
        eval.variables(),
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(
            &eval.frames,
        )),
        &eval.edit.buffers,
        &args,
        eval.symbol_mode(),
    )
}

#[doc(hidden)]
pub fn builtin_previous_single_char_property_change(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    neovm_edit::emacs_core::textprop::builtin_previous_single_char_property_change_in_buffers_with_symbols(
        eval.variables(),
        crate::emacs_core::eval::data::window_state::char_property_window_lookup(Some(&eval.frames)),
        &eval.edit.buffers,
        args,
        eval.symbol_mode(),
    )
}

#[doc(hidden)]
pub fn builtin_defalias(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let plan = plan_defalias_in_obarray(eval.obarray(), &args, eval.symbol_mode())?;
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
        result,
        definition,
        eval.symbol_mode(),
    ));
    eval.record_defalias_function_history(result);
    match action {
        DefaliasAction::SetFunction { symbol, definition } => {
            eval.obarray_mut().set_symbol_function_id_for(
                symbol,
                definition,
                neovm_value::symbol::FunctionEpochBump::Defalias,
            );
        }
        DefaliasAction::CallHook {
            hook,
            symbol_value,
            definition,
        } => {
            eval.apply(hook, vec![symbol_value, definition])?;
        }
    }
    if let Some(symbol) = result.as_symbol_id(eval.symbol_mode()) {
        let definition = eval
            .obarray
            .symbol_function_id(symbol)
            .unwrap_or(Value::NIL);
        let mode = eval.symbol_mode();
        crate::emacs_core::eval::interactive_registry::sync_interactive_registry_for_symbol_definition(
            &mut eval.interactive,
            symbol,
            definition,
            mode,
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
    mode: SymbolMode,
) -> Result<DefaliasPlan, Flow> {
    expect_args_range("defalias", args, 2, 3)?;
    // Unwrap symbol-with-pos transparently via symbol_id, which handles
    // bare symbols, nil, t, and symbol-with-pos objects.
    let symbol = super::symbols::expect_symbol_id(&args[0], mode)?;
    if symbol == intern("nil") {
        return Err(signal(
            LispCondition::SettingConstant,
            vec![Value::symbol("nil")],
        ));
    }
    let definition = args[1];
    if super::symbols::would_create_function_alias_cycle_in_obarray(
        obarray,
        symbol,
        &definition,
        mode,
    ) {
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

#[doc(hidden)]
pub fn builtin_provide(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("provide", &args, 1, 2)?;
    eval.provide_value(args[0], args.get(1).cloned())
}

#[doc(hidden)]
pub fn builtin_require(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
#[doc(hidden)]
pub fn builtin_garbage_collect(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("garbage-collect", &args, 0)?;
    if eval.gc_inhibit_depth > 0 && neovm_value::hashtab::hash_test_parity_enabled() {
        return Ok(Value::NIL);
    }
    eval.gc_collect_exact();
    // Return GC stats.
    super::builtins_extra::builtin_garbage_collect_stats()
}

#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_load(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("load", &args, 1)?;
    if let Some(result) =
        crate::emacs_core::eval::lifecycle::dispatch_file_handler(eval, "load", &args)?
    {
        return Ok(result);
    }
    match super::load::plan_load_in_state(
        eval.variables(),
        eval.edit.buffers.current_buffer(),
        args[0],
        args.get(1).copied(),
        args.get(3).copied(),
        args.get(4).copied(),
    )? {
        super::load::LoadPlan::Return(value) => Ok(value),
        super::load::LoadPlan::Load { requested, found } => {
            let path = neovm_edit::emacs_core::file_name::lisp_file_name_to_path_buf(&found);
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) fn builtin_eval(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("eval", &args, 1)?;
    expect_max_args("eval", &args, 2)?;
    eval.eval_value_with_lexical_arg(args[0], args.get(1).copied())
}

#[doc(hidden)]
pub fn builtin_eval_2(eval: &mut super::eval::Context, form: Value, lexical: Value) -> EvalResult {
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
#[doc(hidden)]
pub fn dynamic_or_global_symbol_value(eval: &super::eval::Context, name: &str) -> Option<Value> {
    let id = neovm_value::intern::intern(name);
    eval.eval_symbol_by_id(id).ok()
}

/// `(compare-buffer-substrings ...)`: resolve the case-fold policy and the
/// current buffer's canonical table (GNU editfns.c:1769-1771) here, then run
/// the editor-owned comparison with the table passed down.
#[doc(hidden)]
pub fn builtin_compare_buffer_substrings(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let case_fold = dynamic_or_global_symbol_value(eval, "case-fold-search")
        .map(|value| !value.is_nil())
        .unwrap_or(true);
    // GNU includes the standard equivalence classes (sigma/final sigma,
    // micro sign) through the current buffer's canonical case table.
    let canon = if case_fold {
        Some(crate::emacs_core::eval::case_tables::current_case_canon_table(eval)?)
    } else {
        None
    };
    neovm_edit::emacs_core::buffer::compare_buffer_substrings_with_canon(
        &eval.edit.buffers,
        args,
        canon,
    )
}

pub(super) fn dynamic_or_global_symbol_value_in_state<'a, Vars: EditVariableRead<'a>>(
    vars: Vars,
    _dynamic: &[OrderedRuntimeBindingMap],
    name: &str,
) -> Option<Value> {
    vars.value_by_name(name)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn buffer_read_only_active(
    eval: &super::eval::Context,
    buf: &neovm_edit::buffer::Buffer,
) -> bool {
    if let Some(value) = buf.get_buffer_local("inhibit-read-only")
        && value.is_truthy()
    {
        return false;
    }

    if eval
        .builtin_var_value_by_name("inhibit-read-only")
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

    eval.builtin_var_value_by_name("buffer-read-only")
        .is_some_and(|value| value.is_truthy())
}

#[doc(hidden)]
pub fn builtin_barf_if_buffer_read_only(
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

    let Some(buf) = ctx.edit.buffers.current_buffer() else {
        return Ok(Value::NIL);
    };
    let point_min = buf.point_min_lisp_char_pos().as_i64();
    let read_only = neovm_edit::emacs_core::editfns::buffer_read_only_active_in_state(
        ctx.variables(),
        &[],
        buf,
    );
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
        vec![Value::make_buffer(buf.id())],
    ))
}

#[doc(hidden)]
pub fn builtin_bury_buffer_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("bury-buffer-internal", &args, 1)?;
    let id = expect_buffer_id(&args[0])?;
    let mut moved = false;
    if eval.edit.buffers.get(id).is_some() {
        // Move to end of global buffer order (Vbuffer_alist equivalent).
        eval.edit.buffers.note_buffer_order_tail(id);
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
    if moved && !eval.edit.buffers.buffer_hooks_inhibited(id) {
        neovm_edit::emacs_core::buffer::run_buffer_list_update_hook(eval)?;
    }
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_cancel_kbd_macro_events(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("cancel-kbd-macro-events", &args, 0)?;
    eval.cancel_kbd_macro_runtime_events();
    Ok(Value::NIL)
}

#[doc(hidden)]
pub fn builtin_combine_after_change_execute(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("combine-after-change-execute", &args, 0)?;
    neovm_edit::emacs_core::editfns::execute_combined_after_change(eval)?;
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
            .builtin_var_value_by_name("standard-output")
            .unwrap_or(Value::T),
    }
}

/// The buffer a (resolved) print target writes into, or `None` when the target
/// is the echo area / `t` / a printer function.
fn print_target_buffer_id(
    _ctx: &crate::emacs_core::eval::Context,
    target: Value,
) -> Option<neovm_edit::buffer::BufferId> {
    match target.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => target.as_buffer_id(),
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
fn print_target_current_buffer(
    ctx: &crate::emacs_core::eval::Context,
    target: Value,
) -> Option<&neovm_edit::buffer::Buffer> {
    match print_target_buffer_id(ctx, target) {
        Some(id) => ctx.edit.buffers.get(id),
        None => ctx.edit.buffers.current_buffer(),
    }
}

fn print_settings_source(
    ctx: &super::eval::Context,
    target: Value,
) -> super::print::PrintSettingsSource {
    let buffer =
        print_target_buffer_id(ctx, target).or_else(|| ctx.edit.buffers.current_buffer_id());
    buffer.map_or(
        super::print::PrintSettingsSource::Globals,
        super::print::PrintSettingsSource::Buffer,
    )
}

/// Temporarily select the target to bind its localized escape cell. Explicit
/// completion restores runtime state; panic Drop restores native ownership.
/// The outer printer scope owns specbindings.
/// No state leaves the owning mutator or is published to another thread.
#[must_use = "dropping the guard restores the caller buffer"]
struct PrintBindingBufferGuard<'context> {
    context: &'context mut super::eval::Context,
    origin: Option<neovm_edit::buffer::BufferId>,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(PrintBindingBufferGuard<'static>: Send, Sync);
impl std::fmt::Debug for PrintBindingBufferGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintBindingBufferGuard")
            .field("origin", &self.origin)
            .finish()
    }
}
impl<'context> PrintBindingBufferGuard<'context> {
    #[doc(hidden)]
    pub fn new(context: &'context mut super::eval::Context) -> Self {
        let origin = context.edit.buffers.current_buffer_id();
        Self {
            context,
            origin,
            _mutator: std::marker::PhantomData,
        }
    }
    #[doc(hidden)]
    pub fn select(&mut self, target: neovm_edit::buffer::BufferId) -> Result<(), Flow> {
        if self.context.edit.buffers.current_buffer_id() == Some(target) {
            Ok(())
        } else {
            self.context.set_current_buffer_unrecorded(target)
        }
    }
    #[doc(hidden)]
    pub fn context(&mut self) -> &mut super::eval::Context {
        self.context
    }
    #[doc(hidden)]
    pub fn finish(mut self, result: Result<(), Flow>) -> Result<(), Flow> {
        if let Some(origin) = self.origin {
            self.context.restore_current_buffer_if_live(origin);
        }
        self.origin = None;
        result
    }
}
impl Drop for PrintBindingBufferGuard<'_> {
    fn drop(&mut self) {
        if let Some(origin) = self.origin.take() {
            self.context.restore_current_buffer_storage_if_live(origin);
        }
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
    ctx.edit.buffers.get(id).map(|buf| buf.get_multibyte())
}

/// Apply GNU `print_prepare`'s implicit binding of the `print-escape-*` flags to
/// `options` for a buffer/marker print target (print.c lines ~170-177): when the
/// destination buffer is unibyte and `print-escape-multibyte` is unset, bind it
/// to `t`; when it is multibyte and `print-escape-nonascii` is unset, bind that.
///
/// In particular, a unibyte string printed into a unibyte buffer keeps
/// `print-escape-nonascii` nil, so its high bytes are emitted raw (not octal),
/// while printing into a multibyte buffer escapes them.
#[derive(Clone, Copy, Debug, strum::AsRefStr)]
enum ImplicitPrintEscape {
    #[strum(serialize = "print-escape-nonascii")]
    Nonascii,
    #[strum(serialize = "print-escape-multibyte")]
    Multibyte,
}

fn apply_print_target_escape_bindings(
    ctx: &mut crate::emacs_core::eval::Context,
    target: Value,
) -> Result<(), Flow> {
    let Some(target_id) = print_target_buffer_id(ctx, target) else {
        return Ok(());
    };
    let Some(multibyte) = print_target_buffer_multibyte(ctx, target) else {
        return Ok(());
    };
    let binding = if multibyte {
        ImplicitPrintEscape::Nonascii
    } else {
        ImplicitPrintEscape::Multibyte
    };
    if ctx
        .variables()
        .value_in_buffer_by_name(ctx.edit.buffers.get(target_id), binding.as_ref())
        .is_some_and(|value| value.is_truthy())
    {
        return Ok(());
    }
    let mut buffer_scope = PrintBindingBufferGuard::new(ctx);
    let result = (|| {
        buffer_scope.select(target_id)?;
        let ctx = buffer_scope.context();
        let binding = intern(binding.as_ref());
        if ctx.specbind_cached(binding, Value::T) {
            Ok(())
        } else {
            ctx.try_specbind(binding, Value::T)
        }
    })();
    buffer_scope.finish(result)
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
    buffer_id: neovm_edit::buffer::BufferId,
    text: &neovm_value::heap_types::LispString,
    before_markers: bool,
) -> Result<(), Flow> {
    if text.is_empty() {
        return Ok(());
    }

    let (insert_pos, insert_char_pos) = ctx
        .edit
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
        &ctx.edit.buffers,
        buffer_id,
        insert_pos,
        super::editfns::lisp_string_text_extent(text),
    )?;

    super::editfns::signal_before_text_change(ctx, change)?;
    let inserted = if before_markers {
        ctx.edit
            .buffers
            .insert_lisp_string_into_buffer_before_markers(buffer_id, text)
    } else {
        ctx.edit
            .buffers
            .insert_lisp_string_into_buffer(buffer_id, text)
    };
    inserted.ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("Output buffer no longer exists")],
        )
    })?;
    let _ = ctx
        .edit
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
    buffer_id: neovm_edit::buffer::BufferId,
    text: &neovm_value::heap_types::LispString,
) -> Result<(), Flow> {
    if ctx.edit.buffers.get(buffer_id).is_none() {
        return Err(signal(
            "error",
            vec![Value::string("Output buffer no longer exists")],
        ));
    }

    let saved_current = ctx.edit.buffers.current_buffer_id();
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
    text: &neovm_value::heap_types::LispString,
) -> Result<(), Flow> {
    let Some((Some(buffer_id), _, _)) = super::marker::marker_logical_fields(&target) else {
        return Err(signal(
            "error",
            vec![Value::string("Marker does not point anywhere")],
        ));
    };
    let marker_pos =
        super::marker::marker_position_as_int_with_buffers(&ctx.edit.buffers, &target)?;
    let Some(buffer) = ctx.edit.buffers.get(buffer_id) else {
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
    let saved_current = ctx.edit.buffers.current_buffer_id();

    let result = (|| -> Result<(), Flow> {
        ctx.set_current_buffer_unrecorded(buffer_id)?;
        let _ = ctx
            .edit
            .buffers
            .goto_buffer_emacs_byte_pos(buffer_id, marker_byte);
        insert_print_lisp_string_with_hooks(ctx, buffer_id, text, false)?;

        let new_marker_pos = ctx
            .edit
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
            &mut ctx.edit.buffers,
            &[
                target,
                Value::fixnum(new_marker_pos),
                Value::make_buffer(buffer_id),
            ],
        )?;
        Ok(())
    })();

    if result.is_ok() && ctx.edit.buffers.get(buffer_id).is_some() {
        let inserted_len = EmacsByteLen::new(text.sbytes());
        let restore_point = if old_target_point >= marker_byte {
            old_target_point.add_len(inserted_len)
        } else {
            old_target_point
        };
        let _ = ctx
            .edit
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
    let text = neovm_value::heap_types::LispString::from_utf8(text);
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
    let ls = neovm_value::heap_types::LispString::from_emacs_bytes(bytes.to_vec());
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
        _other if super::marker::is_marker(&target) => {
            insert_print_lisp_string_to_marker_target(ctx, target, &ls)
        }
        _ => Ok(()),
    }
}

pub(crate) fn print_target_is_direct(target: Value) -> bool {
    (target.is_t() || target.is_nil() || target.is_buffer()) || super::marker::is_marker(&target)
}

/// GNU `print_prepare`/`print_finish` around one buffer/marker-directed print,
/// backed by the World's one `print_buffer` accumulator
/// (`crate::emacs_core::eval::print_buffer`).
///
/// `prepare` runs BEFORE the value is rendered, so a nested print performed by
/// a `print-unreadable-function` callback (a `prin1-to-string` inside it) takes
/// the nested save path on the same accumulator. Its unwind restores the saved
/// bytes but leaves `pos`, exactly like GNU's static; the outer print then
/// appends its own bytes at that position, reproducing the documented
/// nested-print leak (U1/U10/B2/B3).
///
/// The guard is just the specpdl count captured before the accumulator entry.
/// `prepare` pushes GNU's `record_unwind_protect (print_unwind, ...)` as a
/// `PrintBufferUnwind` native-unwind entry, and `finish` runs GNU's
/// `unbind_to`; the partial restore therefore runs on normal finishes AND on
/// every non-local exit (signal/throw/panic replay), without a raw pointer or
/// a borrow held across a Lisp callback (design rev 2 A2).
pub(crate) struct PrintAccumulatorScope {
    /// `SPECPDL_INDEX ()` from GNU `print_prepare`, captured before its
    /// accumulator unwind entry is pushed; `print_finish` unbinds to it.
    base: usize,
    /// Type-level proof that this scope's prepare made the accumulator
    /// Active. Every accumulator operation requires it; the scope carries it
    /// across Lisp callbacks without borrowing the World.
    active: crate::emacs_core::eval::print_buffer::PrintActive,
}

impl PrintAccumulatorScope {
    /// GNU `print_prepare` for a buffer/marker destination, or `None` for the
    /// echo area / `t` / a printer function (GNU's non-`Qnil` printcharfun
    /// path never touches `print_buffer`).
    pub(crate) fn prepare(
        ctx: &mut crate::emacs_core::eval::Context,
        target: Value,
    ) -> Option<Self> {
        if !print_target_is_direct(target) {
            return None;
        }
        if !(target.is_buffer() || super::marker::is_marker(&target)) {
            return None;
        }
        Some(Self::prepare_accumulator(ctx))
    }

    /// `print_prepare` for GNU's invisible `Vprin1_to_string_buffer`
    /// destination: the accumulator takes part exactly as for a buffer, but
    /// the destination buffer itself has no Lisp-visible identity (GNU's
    /// `" prin1"` buffer is outside `Vbuffer_alist`; `(get-buffer " prin1")`
    /// answers nil, and the name is free for a user buffer -- probe B1).
    pub(crate) fn prepare_prin1_to_string(context: &mut crate::emacs_core::eval::Context) -> Self {
        Self::prepare_accumulator(context)
    }

    fn prepare_accumulator(context: &mut crate::emacs_core::eval::Context) -> Self {
        // GNU print_prepare: specpdl_count BEFORE record_unwind_protect.
        let base = context.specpdl.len();
        let active = context.world_print().prepare();
        context.push_specpdl_with(|| crate::emacs_core::eval::SpecBinding::NativeUnwind {
            action: crate::emacs_core::eval::NativeUnwindAction::PrintBufferUnwind,
        });
        Self { base, active }
    }

    /// The prepare token, for the sink renderer (`PrintActive` is `Copy`).
    pub(crate) fn token(&self) -> crate::emacs_core::eval::print_buffer::PrintActive {
        self.active
    }

    /// GNU `print_buffer`'s byte append; `append` advances `pos` by the
    /// character count and `pos_byte` by the byte count. The Context borrow
    /// is re-taken per operation: the accumulator stays Active across a
    /// callback through its specpdl entry, never through a held borrow.
    pub(crate) fn append(&mut self, ctx: &mut crate::emacs_core::eval::Context, bytes: &[u8]) {
        ctx.world_print().append(self.active, bytes);
    }

    /// GNU `print_finish`: insert `buffer[0..pos]` into TARGET (when it has a
    /// real buffer/marker identity), then run `unbind_to`'s recorded unwind
    /// (the partial restore). Returns the inserted bytes -- the value
    /// Fprin1_to_string reads back for its virtual destination.
    pub(crate) fn finish(
        self,
        ctx: &mut crate::emacs_core::eval::Context,
        target: Option<Value>,
    ) -> Result<Vec<u8>, Flow> {
        // GNU inserts into the destination BEFORE unbind_to runs the unwind.
        let pending = ctx.world_print().pending(self.active);
        if let Some(target) = target {
            write_print_bytes_to_target(ctx, target, &pending)?;
        }
        // GNU print_finish's unbind_to(pc->specpdl_count, Qnil): runs the
        // PrintBufferUnwind entry pushed by prepare (partial restore).
        ctx.unbind_to_result(self.base)?;
        Ok(pending)
    }
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
        let (code, len) = neovm_value::emacs_char::string_char(&bytes[pos..]);
        emit_char(Value::fixnum(code.code() as i64))?;
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
            if eval.edit.buffers.get(id).is_none() {
                return Err(signal(
                    "error",
                    vec![Value::string("Output buffer no longer exists")],
                ));
            }
            let _ = eval.edit.buffers.insert_into_buffer(id, "\n");
            Ok(())
        }
        _other => {
            // Root the callable target across eval.apply().
            let mut scope = super::print::PrintEvaluationGuard::new(eval);
            if let Some(buffer_id) = scope.context().edit.buffers.current_buffer_id() {
                scope
                    .context()
                    .specpdl
                    .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
            }
            scope.context().push_specpdl_root(target);
            let result = scope
                .context()
                .apply(target, vec![Value::fixnum('\n' as i64)]);
            scope.finish_value(result)?;
            Ok(())
        }
    }
}

/// No-escape printing into a multibyte text stream, including error-data
/// printing. This shares object traversal with the public printer, so raw-byte
/// symbol names never pass through Rust's UTF-8-only symbol-name API.
pub(crate) fn print_value_princ_bytes_to_multibyte_buffer(
    ctx: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Vec<u8> {
    // GNU print.c:2376-2378 sends a no-escape string straight to
    // print_string_1 (472-495), ignoring its properties and other object
    // settings. Circle preprocessing (1329-1330) can still label strings,
    // so retain the full printer whenever print-circle is enabled.
    if let Some(string) = value.as_lisp_string()
        && !ctx
            .variables()
            .value_in_buffer_by_name(None, "print-circle")
            .is_some_and(|setting| setting.is_truthy())
    {
        // With no active circle table, entry resets numbering independently
        // of print-continuous-numbering. Keep the shared RAII lifecycle.
        let _print_guard = super::print::enter_print_call(&super::print::PrintOptions::default());
        return super::print::princ_string_bytes(
            string,
            super::print::PrincStringOutput::OctalByte8,
        )
        .into_owned();
    }
    let mut options = crate::emacs_core::print::print_options_from_state(ctx.variables(), None);
    options.print_noescape = true;
    options.print_escape_nonascii = true;
    crate::emacs_core::print::print_value_bytes_in_state_with_options(ctx, value, options)
}

/// One override-list cursor stays rooted while specbind invokes watchers.
/// The exclusive Context borrow is confined to the calling mutator. Only roots
/// are removed on Drop: print-variable bindings survive for the outer printer
/// unwind scope. The root-slot token is private and cannot outlive this scope.
#[must_use = "dropping the guard releases the override cursor root"]
struct PrintOverrideCursorGuard<'context> {
    context: &'context mut super::eval::Context,
    roots: Option<super::eval::SpecpdlRootScopeState>,
    cursor: super::eval::SpecpdlRootSlot,
    _mutator: std::marker::PhantomData<*const ()>,
}
static_assertions::assert_not_impl_any!(PrintOverrideCursorGuard<'static>: Send, Sync);
impl std::fmt::Debug for PrintOverrideCursorGuard<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PrintOverrideCursorGuard")
            .field("roots", &self.roots)
            .field("cursor", &self.cursor)
            .finish()
    }
}
impl<'context> PrintOverrideCursorGuard<'context> {
    #[doc(hidden)]
    pub fn new(context: &'context mut super::eval::Context, value: Value) -> Self {
        let roots = Some(context.save_specpdl_roots());
        let cursor = context.push_specpdl_root_slot(value);
        Self {
            context,
            roots,
            cursor,
            _mutator: std::marker::PhantomData,
        }
    }
    #[doc(hidden)]
    pub fn set_cursor(&mut self, value: Value) {
        self.context.set_specpdl_root_slot(&self.cursor, value);
    }
    #[doc(hidden)]
    pub fn context(&mut self) -> &mut super::eval::Context {
        self.context
    }
}
impl Drop for PrintOverrideCursorGuard<'_> {
    fn drop(&mut self) {
        if let Some(roots) = self.roots.take() {
            self.context.restore_specpdl_roots(roots);
        }
    }
}

/// Bind GNU printer override variables in list order, including resets.
/// The exclusive Context borrow belongs to the caller's PrintEvaluationGuard;
/// no Lisp state is cached outside that mutator or survives guard teardown.
fn bind_print_overrides(ctx: &mut super::eval::Context, mut overrides: Value) -> Result<(), Flow> {
    if overrides.is_nil() {
        return Ok(());
    }
    if overrides == Value::T {
        return bind_print_defaults(ctx);
    }
    let mut roots = PrintOverrideCursorGuard::new(ctx, overrides);
    while !overrides.is_nil() {
        roots.set_cursor(overrides);
        let ctx = roots.context();
        if !overrides.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("consp"), overrides],
            ));
        }
        let setting = overrides.cons_car();
        if setting == Value::T {
            bind_print_defaults(ctx)?;
        } else {
            if !setting.is_cons() {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("consp"), setting],
                ));
            }
            let key = PrintOverrideKey::try_from(setting.cons_car())?;
            ctx.try_specbind(key.variable(), setting.cons_cdr())?;
        }
        let tail = overrides.cons_cdr();
        if !tail.is_nil() && !tail.is_cons() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("consp"), overrides],
            ));
        }
        overrides = tail;
    }
    Ok(())
}
fn bind_print_defaults(ctx: &mut super::eval::Context) -> Result<(), Flow> {
    for &key in <PrintOverrideKey as strum::VariantArray>::VARIANTS {
        ctx.try_specbind(key.variable(), key.default_value())?;
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, strum::EnumString, strum::AsRefStr, strum::VariantArray)]
enum PrintOverrideKey {
    #[strum(serialize = "length")]
    Length,
    #[strum(serialize = "level")]
    Level,
    #[strum(serialize = "circle")]
    Circle,
    #[strum(serialize = "quoted")]
    Quoted,
    #[strum(serialize = "escape-newlines")]
    EscapeNewlines,
    #[strum(serialize = "escape-control-characters")]
    EscapeControlCharacters,
    #[strum(serialize = "escape-nonascii")]
    EscapeNonascii,
    #[strum(serialize = "escape-multibyte")]
    EscapeMultibyte,
    #[strum(serialize = "charset-text-property")]
    CharsetTextProperty,
    #[strum(serialize = "unreadable-function")]
    UnreadableFunction,
    #[strum(serialize = "unreadeable-function")]
    LegacyUnreadableFunction,
    #[strum(serialize = "gensym")]
    Gensym,
    #[strum(serialize = "continuous-numbering")]
    ContinuousNumbering,
    #[strum(serialize = "symbols-bare")]
    SymbolsBare,
    #[strum(serialize = "number-table")]
    NumberTable,
    #[strum(serialize = "float-format")]
    FloatFormat,
    #[strum(serialize = "integers-as-characters")]
    IntegersAsCharacters,
}
impl TryFrom<Value> for PrintOverrideKey {
    type Error = Flow;
    fn try_from(key: Value) -> Result<Self, Self::Error> {
        let parsed = key
            .as_bare_symbol_name()
            .and_then(|name| name.parse::<Self>().ok())
            .filter(|setting| key == Value::symbol(setting.as_ref()));
        parsed.ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), Value::NIL],
            )
        })
    }
}

impl PrintOverrideKey {
    #[doc(hidden)]
    pub fn variable(self) -> SymId {
        match self {
            Self::FloatFormat => intern("float-output-format"),
            Self::LegacyUnreadableFunction | Self::UnreadableFunction => {
                intern("print-unreadable-function")
            }
            Self::Length
            | Self::Level
            | Self::Circle
            | Self::Quoted
            | Self::EscapeNewlines
            | Self::EscapeControlCharacters
            | Self::EscapeNonascii
            | Self::EscapeMultibyte
            | Self::CharsetTextProperty
            | Self::Gensym
            | Self::ContinuousNumbering
            | Self::SymbolsBare
            | Self::NumberTable
            | Self::IntegersAsCharacters => intern(&format!("print-{}", self.as_ref())),
        }
    }
    #[doc(hidden)]
    pub fn default_value(self) -> Value {
        match self {
            Self::Quoted => Value::T,
            Self::Length
            | Self::Level
            | Self::Circle
            | Self::EscapeNewlines
            | Self::EscapeControlCharacters
            | Self::EscapeNonascii
            | Self::EscapeMultibyte
            | Self::CharsetTextProperty
            | Self::Gensym
            | Self::ContinuousNumbering
            | Self::SymbolsBare
            | Self::NumberTable
            | Self::IntegersAsCharacters
            | Self::FloatFormat
            | Self::LegacyUnreadableFunction
            | Self::UnreadableFunction => Value::NIL,
        }
    }
}

/// Escaping mode selected once at the Lisp optional-argument boundary.
/// This enum is per-call scalar metadata with no mutator-owned state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ObjectEscaping {
    Escaped,
    Unescaped,
}
impl From<Value> for ObjectEscaping {
    fn from(value: Value) -> Self {
        if value.is_truthy() {
            Self::Unescaped
        } else {
            Self::Escaped
        }
    }
}
impl ObjectEscaping {
    #[doc(hidden)]
    pub fn is_unescaped(self) -> bool {
        match self {
            Self::Escaped => false,
            Self::Unescaped => true,
        }
    }
}

/// Admission is confined to nil overrides and objects whose non-circle printer
/// cannot invoke Lisp or select a buffer. Keep this proof mutator-local like the
/// enclosing PrintEvaluationGuard; properties can require evaluator traversal.
struct CallbackFreePrintLeaf {
    _mutator: std::marker::PhantomData<std::rc::Rc<()>>,
}

impl CallbackFreePrintLeaf {
    #[doc(hidden)]
    pub fn validate(value: Value, overrides: Value) -> Option<Self> {
        if !overrides.is_nil() {
            return None;
        }
        let admitted = match value.kind() {
            ValueKind::Nil
            | ValueKind::T
            | ValueKind::Fixnum(_)
            | ValueKind::Float
            | ValueKind::Symbol(_) => true,
            ValueKind::String => !neovm_value::value::string_has_text_properties_for_value(value),
            ValueKind::Subr(_)
            | ValueKind::Cons
            | ValueKind::Veclike(_)
            | ValueKind::Unbound
            | ValueKind::Unknown => false,
        };
        admitted.then_some(Self {
            _mutator: std::marker::PhantomData,
        })
    }
}

enum Prin1StringPreparation {
    CallbackFreeLeaf {
        _proof: CallbackFreePrintLeaf,
        options: super::print::PrintOptions,
    },
    SavedBuffer(super::print::PrintOptions),
}
static_assertions::assert_not_impl_any!(Prin1StringPreparation: Send, Sync);

impl Prin1StringPreparation {
    #[doc(hidden)]
    pub fn into_options(self) -> super::print::PrintOptions {
        match self {
            Self::CallbackFreeLeaf { options, .. } | Self::SavedBuffer(options) => options,
        }
    }
}

fn prin1_string_options(
    ctx: &crate::emacs_core::eval::Context,
    escaping: ObjectEscaping,
) -> super::print::PrintOptions {
    let mut options = crate::emacs_core::print::print_options_from_state(ctx.variables(), None);
    options.print_noescape = escaping.is_unescaped();
    // GNU's multibyte Vprin1_to_string_buffer implicitly binds this flag to t.
    // Retain the old rendering fallback if a watcher changes the real binding.
    options.print_escape_nonascii = true;
    options
}

fn prepare_prin1_string(
    ctx: &mut crate::emacs_core::eval::Context,
    value: Value,
    escaping: ObjectEscaping,
    overrides: Value,
) -> Result<Prin1StringPreparation, Flow> {
    // Overrides precede PRINTPREPARE's saved buffer (GNU print.c:814-825).
    bind_print_overrides(ctx, overrides)?;
    let escape_nonascii = intern("print-escape-nonascii");
    let needs_binding = !ctx
        .builtin_var_value(escape_nonascii)
        .is_some_and(|value| value.is_truthy());

    if let Some(proof) = CallbackFreePrintLeaf::validate(value, overrides) {
        let binding_depth = ctx.specpdl.len();
        // A successful cached bind is a real Let, including forwarded BOOL
        // storage. Refusal changes neither storage nor specpdl and runs no Lisp.
        if !needs_binding || ctx.specbind_cached(escape_nonascii, Value::T) {
            // Binding can change aliased options, so capture only afterward.
            let options = prin1_string_options(ctx, escaping);
            if !options.print_circle {
                return Ok(Prin1StringPreparation::CallbackFreeLeaf {
                    _proof: proof,
                    options,
                });
            }
            if let Some(buffer_id) = ctx.edit.buffers.current_buffer_id() {
                ctx.specpdl
                    .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
                if needs_binding {
                    // No Lisp/safepoint has occurred since the single cached
                    // Let. Restore [Save, Let] before circle traversal can call
                    // Lisp, preserving the buffer seen by normal unlet cleanup.
                    debug_assert_eq!(ctx.specpdl.len(), binding_depth + 2);
                    ctx.specpdl.swap(binding_depth, binding_depth + 1);
                }
            }
            return Ok(Prin1StringPreparation::SavedBuffer(options));
        }
    }

    // Preserve Save-before-try_specbind on every refusal/general path: watched
    // binding may select a buffer, and unlet must precede buffer restoration.
    if let Some(buffer_id) = ctx.edit.buffers.current_buffer_id() {
        ctx.specpdl
            .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
    }
    if needs_binding {
        ctx.try_specbind(escape_nonascii, Value::T)?;
    }
    Ok(Prin1StringPreparation::SavedBuffer(prin1_string_options(
        ctx, escaping,
    )))
}

/// Prepare format's object bytes inside its already-armed print guard.
/// GNU format %s/%S uses prin1-to-string without overrides (editfns.c:3670-3685).
/// Share its scratch escaping option while retaining the native real implicit
/// binding, after-bind option capture, and circle/refusal cleanup ordering.
pub(super) fn prepare_format_object_options(
    ctx: &mut crate::emacs_core::eval::Context,
    value: Value,
    escaping: ObjectEscaping,
) -> Result<super::print::PrintOptions, Flow> {
    prepare_prin1_string(ctx, value, escaping, Value::NIL).map(Prin1StringPreparation::into_options)
}

fn prin1_to_lisp_string_value_in_state_with_overrides(
    ctx: &mut crate::emacs_core::eval::Context,
    value: &Value,
    escaping: ObjectEscaping,
    overrides: Option<&Value>,
) -> Result<neovm_value::heap_types::LispString, Flow> {
    let mut scope = super::print::PrintEvaluationGuard::new(ctx);
    let result = (|| {
        let ctx = scope.context();

        // GNU's `Fprin1_to_string` prints into `Vprin1_to_string_buffer`, so
        // `PRINTPREPARE` makes THAT buffer current and the caller's buffer-local
        // `print-level` / `print-length` never apply (ledger 196).
        let options = prepare_prin1_string(
            ctx,
            *value,
            escaping,
            overrides.copied().unwrap_or(Value::NIL),
        )?
        .into_options();
        // GNU Fprin1_to_string: print_prepare(Vprin1_to_string_buffer) before
        // the print, print_finish after it. The destination buffer is
        // Lisp-invisible (probe B1: (get-buffer " prin1") is nil), so the
        // World accumulator carries the GNU state and its pending bytes are
        // the string Fbuffer_string would read back.
        let accumulator = PrintAccumulatorScope::prepare_prin1_to_string(ctx);
        super::print::print_value_bytes_sinking_to_accumulator(
            ctx,
            value,
            options,
            super::print::PrintSettingsSource::Globals,
            accumulator.token(),
        )?;
        let bytes = accumulator.finish(ctx, None)?;
        Ok(if bytes.is_ascii() {
            neovm_value::heap_types::LispString::from_unibyte(bytes)
        } else {
            neovm_value::heap_types::LispString::from_emacs_bytes(bytes)
        })
    })();
    scope.finish(result)
}

#[doc(hidden)]
pub fn builtin_princ(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("princ", &args, 1)?;
    let target = resolve_print_target(eval, args.get(1));
    if print_target_is_direct(target) {
        return builtin_princ_impl(eval, args);
    }
    let settings_source = print_settings_source(eval, target);
    let mut scope = super::print::PrintEvaluationGuard::new(eval);
    let result = (|| {
        let eval = scope.context();
        eval.push_specpdl_root(target);
        if let Some(buffer_id) = eval.edit.buffers.current_buffer_id() {
            eval.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }

        let mut options = crate::emacs_core::print::print_options_from_state(
            eval.variables(),
            print_target_current_buffer(eval, target),
        );
        options.print_noescape = true;
        options.print_string_destination = super::print::PrincStringDestination::Callable;
        let bytes =
            super::print::print_value_bytes_in_context(eval, &args[0], options, settings_source)?;
        let princ_result = dispatch_print_callback_emacs_chars(&bytes, |ch| {
            eval.apply(target, vec![ch]).map(|_| ())
        });
        princ_result?;
        Ok(args[0])
    })();
    scope.finish_value(result)
}

pub(crate) fn builtin_princ_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mut scope = super::print::PrintEvaluationGuard::new(ctx);
    let result = (|| {
        let ctx = scope.context();

        expect_min_args("princ", &args, 1)?;
        // Issue #131: emit canonical Emacs bytes directly. A real Private-Use glyph
        // is inserted as itself, while genuine eight-bit / non-Unicode content is
        // carried as its disjoint extended encoding — neither is ever mistaken for
        // the other, retiring the storage-string sink princ used to fall back to.
        let target = resolve_print_target_in_state(ctx, args.get(1));
        ctx.push_specpdl_root(target);
        let output_target = if target.is_nil() { Value::T } else { target };
        let settings_source = print_settings_source(ctx, target);
        if let Some(buffer_id) = ctx.edit.buffers.current_buffer_id() {
            ctx.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }
        apply_print_target_escape_bindings(ctx, target)?;
        let mut options = crate::emacs_core::print::print_options_from_state(
            ctx.variables(),
            print_target_current_buffer(ctx, target),
        );
        options.print_noescape = true;
        // GNU print_prepare BEFORE the render: a print-unreadable callback's
        // nested print takes the nested save path on the World accumulator.
        let accumulator = PrintAccumulatorScope::prepare(ctx, target);
        if let Some(accumulator) = accumulator {
            super::print::print_value_bytes_sinking_to_accumulator(
                ctx,
                &args[0],
                options,
                settings_source,
                accumulator.token(),
            )?;
            accumulator.finish(ctx, Some(target))?;
        } else {
            let bytes = super::print::print_value_bytes_in_context(
                ctx,
                &args[0],
                options,
                settings_source,
            )?;
            write_print_bytes_from_ctx(ctx, Some(&output_target), &bytes)?;
        }
        Ok(args[0])
    })();
    scope.finish_value(result)
}

#[doc(hidden)]
pub fn builtin_prin1(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("prin1", &args, 1)?;
    let target = resolve_print_target(eval, args.get(1));
    if print_target_is_direct(target) {
        return builtin_prin1_impl(eval, args);
    }

    let mut scope = super::print::PrintEvaluationGuard::new(eval);
    let result = (|| {
        let eval = scope.context();

        expect_min_args("prin1", &args, 1)?;
        let target = resolve_print_target(eval, args.get(1));
        eval.push_specpdl_root(target);
        bind_print_overrides(eval, args.get(2).copied().unwrap_or(Value::NIL))?;
        if let Some(buffer_id) = eval.edit.buffers.current_buffer_id() {
            eval.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }
        let settings_source = print_settings_source(eval, target);

        let options = crate::emacs_core::print::print_options_from_state(
            eval.variables(),
            print_target_current_buffer(eval, target),
        );

        // GNU's callable print target receives Emacs character codes one at a time.
        // Keep the printer's canonical Emacs bytes intact until that boundary: a
        // Rust String cannot represent byte8 or non-Unicode Emacs characters and
        // would silently replace them before `external-debugging-output` (or any
        // other character sink) sees them.
        let bytes =
            super::print::print_value_bytes_in_context(eval, &args[0], options, settings_source)?;
        let prin1_result = dispatch_print_callback_emacs_chars(&bytes, |ch| {
            eval.apply(target, vec![ch]).map(|_| ())
        });
        prin1_result?;
        Ok(args[0])
    })();
    scope.finish_value(result)
}

pub(crate) fn builtin_prin1_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mut scope = super::print::PrintEvaluationGuard::new(ctx);
    let result = (|| {
        let ctx = scope.context();

        expect_min_args("prin1", &args, 1)?;
        let target = resolve_print_target_in_state(ctx, args.get(1));
        ctx.push_specpdl_root(target);
        let output_target = if target.is_nil() { Value::T } else { target };
        bind_print_overrides(ctx, args.get(2).copied().unwrap_or(Value::NIL))?;
        let settings_source = print_settings_source(ctx, target);
        if let Some(buffer_id) = ctx.edit.buffers.current_buffer_id() {
            ctx.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }
        apply_print_target_escape_bindings(ctx, target)?;
        let options = crate::emacs_core::print::print_options_from_state(
            ctx.variables(),
            print_target_current_buffer(ctx, target),
        );
        // GNU `print_prepare' implicitly binds `print-escape-nonascii' /
        // `print-escape-multibyte' based on the destination buffer's multibyteness,
        // so e.g. a unibyte string's high bytes print raw into a unibyte buffer but
        // octal-escaped into a multibyte buffer.
        // Issue #131: emit canonical Emacs bytes so a real Private-Use glyph in the
        // printed output is inserted as itself, not decoded as a raw byte.
        // GNU print_prepare BEFORE the render (see PrintAccumulatorScope).
        let accumulator = PrintAccumulatorScope::prepare(ctx, target);
        if let Some(accumulator) = accumulator {
            super::print::print_value_bytes_sinking_to_accumulator(
                ctx,
                &args[0],
                options,
                settings_source,
                accumulator.token(),
            )?;
            accumulator.finish(ctx, Some(target))?;
        } else {
            let bytes = super::print::print_value_bytes_in_context(
                ctx,
                &args[0],
                options,
                settings_source,
            )?;
            write_print_bytes_from_ctx(ctx, Some(&output_target), &bytes)?;
        }
        Ok(args[0])
    })();
    scope.finish_value(result)
}

#[doc(hidden)]
pub fn builtin_prin1_to_string(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_prin1_to_string_impl(eval, args)
}

pub(crate) fn builtin_prin1_to_string_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("prin1-to-string", &args, 1)?;
    let escaping = ObjectEscaping::from(args.get(1).copied().unwrap_or(Value::NIL));
    Ok(Value::heap_string(
        prin1_to_lisp_string_value_in_state_with_overrides(ctx, &args[0], escaping, args.get(2))?,
    ))
}

#[doc(hidden)]
pub fn builtin_print(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("print", &args, 1)?;
    let target = resolve_print_target(eval, args.get(1));
    if print_target_is_direct(target) {
        return builtin_print_impl(eval, args);
    }
    let settings_source = print_settings_source(eval, target);
    let mut scope = super::print::PrintEvaluationGuard::new(eval);
    let result = (|| {
        let eval = scope.context();
        eval.push_specpdl_root(target);
        if let Some(buffer_id) = eval.edit.buffers.current_buffer_id() {
            eval.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }

        // A function stream performs no `set_buffer_internal`, so GNU reads its
        // `print-*` globals with the CALLER's buffer current -- the same rule
        // `builtin_prin1`'s function path follows. Ledger 196.
        let options = crate::emacs_core::print::print_options_from_state(
            eval.variables(),
            print_target_current_buffer(eval, target),
        );
        let mut bytes = Vec::new();
        bytes.push(b'\n');
        bytes.extend_from_slice(&super::print::print_value_bytes_in_context(
            eval,
            &args[0],
            options,
            settings_source,
        )?);
        bytes.push(b'\n');
        let print_result = dispatch_print_callback_emacs_chars(&bytes, |ch| {
            eval.apply(target, vec![ch]).map(|_| ())
        });
        print_result?;
        Ok(args[0])
    })();
    scope.finish_value(result)
}

pub(crate) fn builtin_print_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let mut scope = super::print::PrintEvaluationGuard::new(ctx);
    let result = (|| {
        let ctx = scope.context();

        expect_min_args("print", &args, 1)?;
        // GNU `print_prepare' binds the `print-escape-*' flags from the destination
        // buffer's multibyteness (see `builtin_prin1_impl`).
        let target = resolve_print_target_in_state(ctx, args.get(1));
        ctx.push_specpdl_root(target);
        let output_target = if target.is_nil() { Value::T } else { target };
        let settings_source = print_settings_source(ctx, target);
        if let Some(buffer_id) = ctx.edit.buffers.current_buffer_id() {
            ctx.specpdl
                .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
        }
        apply_print_target_escape_bindings(ctx, target)?;
        let options = crate::emacs_core::print::print_options_from_state(
            ctx.variables(),
            print_target_current_buffer(ctx, target),
        );
        // Issue #131: emit canonical Emacs bytes (see `builtin_prin1_impl`).
        // GNU print_prepare BEFORE the render (see PrintAccumulatorScope).
        let mut accumulator = PrintAccumulatorScope::prepare(ctx, target);
        let rendered = if let Some(scope) = accumulator.as_mut() {
            // GNU Fprint: printchar ('\n') BEFORE the object print().
            scope.append(ctx, b"\n");
            super::print::print_value_bytes_sinking_to_accumulator(
                ctx,
                &args[0],
                options,
                settings_source,
                scope.token(),
            )?
        } else {
            super::print::print_value_bytes_in_context(ctx, &args[0], options, settings_source)?
        };
        if let Some(mut accumulator) = accumulator {
            accumulator.append(ctx, b"\n");
            accumulator.finish(ctx, Some(target))?;
        } else {
            let mut bytes = Vec::new();
            bytes.push(b'\n');
            bytes.extend_from_slice(&rendered);
            bytes.push(b'\n');
            write_print_bytes_from_ctx(ctx, Some(&output_target), &bytes)?;
        }
        Ok(args[0])
    })();
    scope.finish_value(result)
}

/// Scalar line state derived from the resolved stream's validated point.
/// It holds no heap references and is safe to copy between threads.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerpriLineState {
    BeginningOfLine,
    NeedsNewline,
}
static_assertions::assert_impl_all!(TerpriLineState: Send, Sync);

/// Destination-relative line state; marker printing temporarily moves point
/// in GNU, so testing the caller's buffer point would be incorrect.
fn terpri_line_state(
    ctx: &crate::emacs_core::eval::Context,
    target: Value,
) -> Result<TerpriLineState, Flow> {
    let buffer_id = if super::marker::is_marker(&target) {
        Some(print_target_buffer_id(ctx, target).ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("Marker does not point anywhere")],
            )
        })?)
    } else {
        print_target_buffer_id(ctx, target).or_else(|| ctx.edit.buffers.current_buffer_id())
    };
    let Some(buffer_id) = buffer_id else {
        return Ok(TerpriLineState::BeginningOfLine);
    };
    let Some(buffer) = ctx.edit.buffers.get(buffer_id) else {
        return Err(signal(
            LispCondition::Error,
            vec![Value::string("Output buffer no longer exists")],
        ));
    };
    let point = if super::marker::is_marker(&target) {
        let pos = super::marker::marker_position_as_int_with_buffers(&ctx.edit.buffers, &target)?;
        let min = buffer.point_min_lisp_char_pos().as_i64();
        let max = buffer.point_max_lisp_char_pos().as_i64();
        if pos < min || pos > max {
            return Err(signal(
                LispCondition::Error,
                vec![
                    Value::string("Marker is outside the accessible part of the buffer"),
                    target,
                ],
            ));
        }
        buffer.lisp_pos_to_emacs_byte_pos(LispCharPos1::new(pos))
    } else {
        buffer.point_emacs_byte_pos()
    };
    Ok(
        if point == buffer.point_min_emacs_byte_pos()
            || buffer.char_code_before_emacs_byte_pos(point) == Some(u32::from(b'\n'))
        {
            TerpriLineState::BeginningOfLine
        } else {
            TerpriLineState::NeedsNewline
        },
    )
}

#[doc(hidden)]
pub fn builtin_terpri(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
        if target.is_buffer() || super::marker::is_marker(&target) {
            // GNU print.c:638 Fterpri runs entirely inside
            // print_prepare/print_finish, so a buffer/marker destination
            // writes through the shared accumulator (and a nested call
            // leaves GNU's `pos` quirk behind).
            let mut accumulator = PrintAccumulatorScope::prepare(ctx, target)
                .expect("buffer/marker destination takes print_prepare");
            let print_newline = if args.get(1).copied().is_some_and(Value::is_truthy) {
                match terpri_line_state(ctx, target)? {
                    TerpriLineState::BeginningOfLine => false,
                    TerpriLineState::NeedsNewline => true,
                }
            } else {
                true
            };
            if print_newline {
                accumulator.append(ctx, b"\n");
            }
            accumulator.finish(ctx, Some(target))?;
            return Ok(Some(if print_newline { Value::T } else { Value::NIL }));
        }
        if args.get(1).copied().is_some_and(Value::is_truthy)
            && !(ctx.noninteractive() && (target.is_t() || target.is_nil()))
        {
            match terpri_line_state(ctx, target)? {
                TerpriLineState::BeginningOfLine => return Ok(Some(Value::NIL)),
                TerpriLineState::NeedsNewline => {}
            }
        }
        write_print_output_from_ctx(ctx, args.first(), "\n")?;
        return Ok(Some(Value::T));
    }
    Ok(None)
}

pub(crate) fn finish_terpri_in_eval(eval: &mut super::eval::Context, args: &[Value]) -> EvalResult {
    expect_max_args("terpri", args, 2)?;
    let target = resolve_print_target(eval, args.first());
    if args.get(1).copied().is_some_and(Value::is_truthy) {
        if super::types::builtin_functionp_1(eval, target)?.is_truthy() {
            return Err(signal(
                LispCondition::Error,
                vec![Value::string("Unsupported function argument"), target],
            ));
        }
        match terpri_line_state(eval, target)? {
            TerpriLineState::BeginningOfLine => return Ok(Value::NIL),
            TerpriLineState::NeedsNewline => {}
        }
    }
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
        .and_then(neovm_value::emacs_char::EmacsChar::from_code)
        .map(|c| c.to_emacs_bytes())
}

#[doc(hidden)]
pub fn builtin_write_char(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
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
                if eval.edit.buffers.get(id).is_none() {
                    return Err(signal(
                        "error",
                        vec![Value::string("Output buffer no longer exists")],
                    ));
                }
                let ls = neovm_value::heap_types::LispString::from_emacs_bytes(bytes);
                let _ = eval.edit.buffers.insert_lisp_string_into_buffer(id, &ls);
            }
        }
        _other => {
            let mut scope = super::print::PrintEvaluationGuard::new(eval);
            if let Some(buffer_id) = scope.context().edit.buffers.current_buffer_id() {
                scope
                    .context()
                    .specpdl
                    .push(crate::emacs_core::eval::SpecBinding::SaveCurrentBuffer { buffer_id });
            }
            scope.context().push_specpdl_root(target);
            let result = scope
                .context()
                .apply(target, vec![Value::fixnum(char_code)]);
            scope.finish_value(result)?;
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
            if target.is_buffer() || super::marker::is_marker(&target) {
                // GNU print.c:561 Fwrite_char runs inside
                // print_prepare/print_finish: the byte goes through the
                // shared accumulator for a buffer/marker destination.
                let mut accumulator = PrintAccumulatorScope::prepare(ctx, target)
                    .expect("buffer/marker destination takes print_prepare");
                accumulator.append(ctx, &bytes);
                accumulator.finish(ctx, Some(target))?;
            } else {
                write_print_bytes_from_ctx(ctx, args.get(1), &bytes)?;
            }
        }
        return Ok(Some(Value::fixnum(char_code)));
    }

    Ok(None)
}

#[doc(hidden)]
pub fn builtin_propertize(args: Vec<Value>) -> EvalResult {
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
    // GNU copies SCHARS and SBYTES independently (editfns.c:3314,
    // fns.c:789-794). Recounting bytes loses supplied shorthand metadata.
    let new_str = Value::heap_string(string.without_properties());

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

#[doc(hidden)]
pub fn builtin_current_cpu_time(args: Vec<Value>) -> EvalResult {
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
            use windows_sys::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};

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

#[doc(hidden)]
pub fn builtin_current_idle_time(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("current-idle-time", &args, 0)?;
    Ok(eval.current_idle_time_value())
}

#[cfg(test)]
#[path = "tests/print_override_roots.rs"]
mod print_override_roots;

#[cfg(test)]
#[path = "tests/terpri_units.rs"]
mod terpri_units;

#[cfg(test)]
#[path = "tests/printer_target_units.rs"]
mod printer_target_units;

#[cfg(test)]
#[path = "tests/print_binding_scope_test.rs"]
mod print_binding_scope_test;

#[cfg(test)]
#[path = "tests/prin1_string_leaf_test.rs"]
mod prin1_string_leaf_test;

#[cfg(test)]
#[path = "misc_eval/tests/declared_propertize.rs"]
mod declared_propertize_tests;
