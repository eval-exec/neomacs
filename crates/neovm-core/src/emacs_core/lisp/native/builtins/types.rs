use super::*;
use crate::emacs_core::error::{expect_args, expect_args_range};
use crate::emacs_core::value::{ValueKind, VecLikeType};

pub(crate) fn builtin_null_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_nil()))
}

pub(crate) fn builtin_atom_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(!arg.is_cons()))
}

pub(crate) fn builtin_consp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_cons()))
}

pub(crate) fn builtin_listp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_list()))
}

pub(crate) fn builtin_nlistp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(!arg.is_list()))
}

pub(crate) fn builtin_symbolp_1(eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    let is_sym = arg.is_symbol() || (eval.symbols_with_pos_enabled && arg.is_symbol_with_pos());
    Ok(Value::bool_val(is_sym))
}

pub(crate) fn builtin_numberp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_number()))
}

pub(crate) fn builtin_integerp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_integer()))
}

pub(crate) fn builtin_floatp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_float()))
}

pub(crate) fn builtin_stringp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_string()))
}

pub(crate) fn builtin_vectorp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    let is_vec = arg.is_vector()
        && !super::chartable::is_char_table(&arg)
        && !super::chartable::is_bool_vector(&arg);
    Ok(Value::bool_val(is_vec))
}

fn keywordp_swp(arg: Value, symbols_with_pos_enabled: bool) -> bool {
    let bare = if symbols_with_pos_enabled && arg.is_symbol_with_pos() {
        arg.as_symbol_with_pos_sym().unwrap_or(arg)
    } else {
        arg
    };
    bare.is_keyword()
}

pub(crate) fn builtin_keywordp_1(eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(keywordp_swp(
        arg,
        eval.symbols_with_pos_enabled,
    )))
}

pub(crate) fn builtin_eq_2(
    eval: &mut super::eval::Context,
    left: Value,
    right: Value,
) -> EvalResult {
    Ok(Value::bool_val(eq_value_swp(
        &left,
        &right,
        eval.symbols_with_pos_enabled,
    )))
}

// ===========================================================================
// Type predicates
// ===========================================================================

// `list-of-strings-p', `integer-or-null-p' and `string-or-null-p' are NOT
// here: GNU has no DEFUN for them, only `defun's at lisp/subr.el:4768, :4809
// and :4762.  DIVERGENCES.md 148.

pub(crate) fn builtin_integer_or_marker_p(args: Vec<Value>) -> EvalResult {
    expect_args("integer-or-marker-p", &args, 1)?;
    // Mirrors GNU `INTEGERP || MARKERP` (data.c). `INTEGERP`
    // covers both fixnums and bignums.
    let is_integer_or_marker =
        args[0].is_integer() || args[0].is_char() || super::marker::is_marker(&args[0]);
    Ok(Value::bool_val(is_integer_or_marker))
}

pub(crate) fn builtin_number_or_marker_p(args: Vec<Value>) -> EvalResult {
    expect_args("number-or-marker-p", &args, 1)?;
    // Mirrors GNU `NUMBERP || MARKERP` (data.c). `NUMBERP`
    // covers fixnums, bignums, and floats.
    let is_number_or_marker =
        args[0].is_number() || args[0].as_char().is_some() || super::marker::is_marker(&args[0]);
    Ok(Value::bool_val(is_number_or_marker))
}

pub(crate) fn builtin_vector_or_char_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("vector-or-char-table-p", &args, 1)?;
    let is_plain_vector = args[0].is_vector() && !super::chartable::is_bool_vector(&args[0]);
    Ok(Value::bool_val(
        is_plain_vector || super::chartable::is_char_table(&args[0]),
    ))
}

pub(crate) fn builtin_characterp(args: Vec<Value>) -> EvalResult {
    expect_args_range("characterp", &args, 1, 2)?;
    // Official Emacs: characterp accepts both Char values and integers
    // in the valid Unicode range (0..MAX_CHAR).  Its obsolete second
    // argument is accepted for compatibility and ignored.
    let is_char = match args[0].kind() {
        ValueKind::Fixnum(n) => (0..=0x3F_FFFF).contains(&n), // MAX_CHAR in Emacs
        _ => false,
    };
    Ok(Value::bool_val(is_char))
}

// `char-uppercase-p' is NOT here either: it is `(defun char-uppercase-p
// (char) ...)' at lisp/simple.el:6683, and it asks the Unicode `lowercase'
// property rather than the case table's downcase mapping -- the two disagree
// for U+0130.  DIVERGENCES.md 148.

pub(super) fn is_lambda_form_list(value: &Value, symbols_with_pos_enabled: bool) -> bool {
    match value.kind() {
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            crate::emacs_core::value::eq_value_swp(
                &pair_car,
                &Value::symbol("lambda"),
                symbols_with_pos_enabled,
            )
        }
        _ => false,
    }
}

fn is_macro_marker_list(value: &Value, symbols_with_pos_enabled: bool) -> bool {
    match value.kind() {
        ValueKind::Cons => {
            let pair_car = value.cons_car();
            crate::emacs_core::value::eq_value_swp(
                &pair_car,
                &Value::symbol("macro"),
                symbols_with_pos_enabled,
            )
        }
        _ => false,
    }
}

fn is_runtime_function_object(value: &Value) -> bool {
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::ByteCode)
        | ValueKind::Veclike(VecLikeType::ModuleFunction) => true,
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => {
            super::subr_info::subr_is_callable_function_value(value)
        }
        _ => false,
    }
}

fn autoload_type_of(value: &Value) -> Option<super::autoload::AutoloadType> {
    if !super::autoload::is_autoload_value(value) {
        return None;
    }
    let items = list_to_vec(value)?;
    let type_value = items.get(4).cloned().unwrap_or(Value::NIL);
    Some(super::autoload::AutoloadType::from_value(&type_value))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_functionp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("functionp", &args, 1)?;
    builtin_functionp_1(eval, args[0])
}

pub(crate) fn value_is_function(eval: &super::eval::Context, arg: Value) -> bool {
    let object = eval.unwrap_symbol(arg);
    if let Some(symbol) = match object.kind() {
        ValueKind::Nil => Some(intern("nil")),
        ValueKind::T => Some(intern("t")),
        ValueKind::Symbol(id) => Some(id),
        _ => None,
    } {
        if let Some(function) =
            resolve_indirect_symbol_by_id_in_obarray(&eval.obarray, symbol).map(|(_, value)| value)
        {
            if let Some(autoload_type) = autoload_type_of(&function) {
                matches!(autoload_type, super::autoload::AutoloadType::Function)
            } else {
                is_runtime_function_object(&function)
                    || is_lambda_form_list(&function, eval.symbols_with_pos_enabled)
            }
        } else {
            false
        }
    } else {
        match object.kind() {
            ValueKind::Veclike(VecLikeType::Lambda)
            | ValueKind::Subr(_)
            | ValueKind::Veclike(VecLikeType::Subr)
            | ValueKind::Veclike(VecLikeType::ByteCode)
            | ValueKind::Veclike(VecLikeType::ModuleFunction) => {
                is_runtime_function_object(&object)
            }
            ValueKind::Cons => {
                !is_macro_marker_list(&object, eval.symbols_with_pos_enabled)
                    && is_lambda_form_list(&object, eval.symbols_with_pos_enabled)
            }
            _ => false,
        }
    }
}

pub(crate) fn builtin_functionp_1(eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(value_is_function(eval, arg)))
}

pub(crate) fn builtin_hash_table_p(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-p", &args, 1)?;
    Ok(Value::bool_val(args[0].is_hash_table()))
}

pub(crate) fn builtin_type_of(args: &[Value]) -> EvalResult {
    expect_args("type-of", &args, 1)?;
    // GNU Emacs `type-of` handles symbol, integer, subr directly,
    // then delegates to `cl-type-of` for everything else.
    match args[0].kind() {
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) => Ok(Value::symbol("symbol")),
        // GNU `type-of` returns `integer` for both fixnums and bignums; only
        // `cl-type-of` distinguishes `bignum`.
        ValueKind::Fixnum(_) | ValueKind::Veclike(VecLikeType::Bignum) => {
            Ok(Value::symbol("integer"))
        }
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => Ok(Value::symbol("subr")),
        _ => builtin_cl_type_of(&args),
    }
}

/// Context-aware type-of that dumps Lisp backtrace on stale reference.
pub(crate) fn builtin_type_of_with_ctx(
    ctx: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args("type-of", &args, 1)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_type_of_with_ctx_1(ctx, arg(0))
}
/// `type-of` as registered: fixed arity 1, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a1` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_type_of_with_ctx_1(
    ctx: &mut super::super::eval::Context,
    object: Value,
) -> EvalResult {
    let args: [Value; 1] = [object];
    // Stale tagged pointer detection is not applicable with tagged pointers —
    // the old generation-based check relied on tagged pointer indirection which
    // no longer exists.  Just delegate directly.
    let _ = ctx; // suppress unused warning
    builtin_type_of(&args)
}

pub(crate) fn builtin_cl_type_of(args: &[Value]) -> EvalResult {
    expect_args("cl-type-of", &args, 1)?;
    // Stale tagged pointer detection is not applicable with tagged pointers.
    // Records: return the type tag (slot 0).
    // GNU data.c:269-277: if slot 0 is itself a record with len > 1,
    // return slot 1 of that inner record (the class name symbol).
    // This is how EIEIO objects work: slot 0 is the eieio--class
    // record, and slot 1 of that record is the class name.
    if args[0].is_record() {
        let tag = args[0].as_record_data().and_then(|v| v.first().copied());
        if let Some(tag_val) = tag
            && tag_val.is_record()
        {
            let tag_vec = tag_val.as_record_data();
            if let Some(tv) = tag_vec
                && tv.len() > 1
            {
                return Ok(tv[1]);
            }
        }
        return Ok(tag.unwrap_or_else(|| Value::symbol("record")));
    }
    // Char-tables and bool-vectors are tagged vectors
    if chartable::is_char_table(&args[0]) {
        return Ok(Value::symbol("char-table"));
    }
    if chartable::is_bool_vector(&args[0]) {
        return Ok(Value::symbol("bool-vector"));
    }
    // GNU's PVEC_FONT reports font-spec/font-entity/font-object from type-of
    // and cl-type-of. Neomacs specs/entities are public vectors while opened
    // font objects carry the opaque PVEC_FONT tag.
    if let Some(name) = crate::emacs_core::font::font_value_type_symbol(&args[0]) {
        return Ok(Value::symbol(name));
    }
    let name = match args[0].kind() {
        ValueKind::Nil => "null",
        ValueKind::T => "boolean",
        ValueKind::Fixnum(_) => "fixnum",
        ValueKind::Float => "float",
        ValueKind::String => "string",
        ValueKind::Symbol(_) => "symbol",
        ValueKind::Cons => "cons",
        ValueKind::Veclike(VecLikeType::Vector) => "vector",
        ValueKind::Veclike(VecLikeType::CharTable) => "char-table",
        ValueKind::Veclike(VecLikeType::SubCharTable) => "sub-char-table",
        ValueKind::Veclike(VecLikeType::Record) => unreachable!(),
        ValueKind::Veclike(VecLikeType::Font) => "font-object",
        ValueKind::Veclike(VecLikeType::WindowConfiguration) => "window-configuration",
        ValueKind::Veclike(VecLikeType::HashTable) => "hash-table",
        ValueKind::Veclike(VecLikeType::Obarray) => "obarray",
        ValueKind::Subr(_) | ValueKind::Veclike(VecLikeType::Subr) => "primitive-function",
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
            "interpreted-function"
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => "byte-code-function",
        ValueKind::Veclike(VecLikeType::Marker) => "marker",
        ValueKind::Veclike(VecLikeType::Buffer) => "buffer",
        ValueKind::Veclike(VecLikeType::Overlay) => "overlay",
        ValueKind::Veclike(VecLikeType::Window) => "window",
        ValueKind::Veclike(VecLikeType::Frame) => "frame",
        ValueKind::Veclike(VecLikeType::Timer) => "timer",
        ValueKind::Veclike(VecLikeType::Process) => "process",
        ValueKind::Veclike(VecLikeType::Terminal) => "terminal",
        ValueKind::Veclike(VecLikeType::Xwidget) => "xwidget",
        ValueKind::Veclike(VecLikeType::XwidgetView) => "xwidget-view",
        // NeoMacs-only GC-managed shader-surface handle.
        ValueKind::Veclike(VecLikeType::SurfaceHandle) => "neomacs-surface",
        // NeoMacs-only GC-managed video-session handle.
        ValueKind::Veclike(VecLikeType::VideoHandle) => "neomacs-video",
        // GNU `Fcl_type_of` reports bignums as `bignum`.
        ValueKind::Veclike(VecLikeType::Bignum) => "bignum",
        // GNU `Fcl_type_of` reports symbol-with-pos as `symbol-with-pos`.
        ValueKind::Veclike(VecLikeType::SymbolWithPos) => "symbol-with-pos",
        ValueKind::Veclike(VecLikeType::Finalizer) => "finalizer",
        ValueKind::Veclike(VecLikeType::Sqlite) => "sqlite",
        ValueKind::Veclike(VecLikeType::UserPtr) => "user-ptr",
        ValueKind::Veclike(VecLikeType::ModuleFunction) => "module-function",
        // `Qunbound` is internal and should never reach `type-of`
        // from Lisp; treat it as `unknown` if it somehow leaks.
        ValueKind::Unbound | ValueKind::Unknown => "unknown",
    };
    Ok(Value::symbol(name))
}

pub(crate) fn builtin_sequencep(args: Vec<Value>) -> EvalResult {
    expect_args("sequencep", &args, 1)?;
    // GNU: sequences are lists, vectors, strings, bool-vectors, char-tables.
    // Lambdas and records are NOT sequences.
    let is_seq = args[0].is_list()
        || args[0].is_vector()
        || args[0].is_string()
        || chartable::is_char_table(&args[0])
        || chartable::is_bool_vector(&args[0]);
    Ok(Value::bool_val(is_seq))
}

pub(crate) fn builtin_arrayp(args: Vec<Value>) -> EvalResult {
    expect_args("arrayp", &args, 1)?;
    // GNU: arrays are vectors, strings, char-tables, bool-vectors.
    // Records are NOT arrays.
    let is_arr = args[0].is_vector()
        || args[0].is_string()
        || chartable::is_char_table(&args[0])
        || chartable::is_bool_vector(&args[0]);
    Ok(Value::bool_val(is_arr))
}

// ===========================================================================
// Equality
// ===========================================================================

pub(crate) fn builtin_eql_2(eval: &mut super::eval::Context, a: Value, b: Value) -> EvalResult {
    Ok(Value::bool_val(eql_value_swp(
        &a,
        &b,
        eval.symbols_with_pos_enabled,
    )))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_equal(args: Vec<Value>) -> EvalResult {
    expect_args("equal", &args, 2)?;
    Ok(Value::bool_val(try_equal_value_swp(
        &args[0], &args[1], 0, false,
    )?))
}

pub(crate) fn builtin_equal_2(eval: &mut super::eval::Context, a: Value, b: Value) -> EvalResult {
    Ok(Value::bool_val(try_equal_value_swp(
        &a,
        &b,
        0,
        eval.symbols_with_pos_enabled,
    )?))
}

pub(crate) fn builtin_function_equal(args: Vec<Value>) -> EvalResult {
    expect_args("function-equal", &args, 2)?;
    Ok(Value::bool_val(args[0].function_equal(args[1])))
}

pub(crate) fn builtin_module_function_p(args: Vec<Value>) -> EvalResult {
    expect_args("module-function-p", &args, 1)?;
    Ok(Value::bool_val(args[0].is_module_function()))
}

pub(crate) fn builtin_user_ptrp(args: Vec<Value>) -> EvalResult {
    expect_args("user-ptrp", &args, 1)?;
    Ok(Value::bool_val(args[0].is_user_ptr()))
}

pub(crate) fn builtin_symbol_with_pos_p_1(
    _eval: &mut super::eval::Context,
    arg: Value,
) -> EvalResult {
    Ok(Value::bool_val(arg.is_symbol_with_pos()))
}

pub(crate) fn builtin_symbol_with_pos_pos_1(
    _eval: &mut super::eval::Context,
    arg: Value,
) -> EvalResult {
    builtin_symbol_with_pos_pos_1_value(arg)
}

fn builtin_symbol_with_pos_pos_1_value(arg: Value) -> EvalResult {
    if let Some(pos) = arg.as_symbol_with_pos_pos() {
        Ok(Value::fixnum(pos))
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbol-with-pos-p"), arg],
        ))
    }
}

pub(crate) fn builtin_char_equal(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    crate::emacs_core::error::expect_args("char-equal", &args, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_char_equal_2(eval, arg(0), arg(1))
}
/// `char-equal` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_char_equal_2(
    eval: &mut super::eval::Context,
    c1: Value,
    c2: Value,
) -> EvalResult {
    let args: [Value; 2] = [c1, c2];
    expect_args("char-equal", &args, 2)?;
    let left = expect_char_equal_code(&args[0])?;
    let right = expect_char_equal_code(&args[1])?;
    // GNU `Fchar_equal` (`editfns.c:4406`): equal characters always match; with
    // `case-fold-search` nil, only exact equality counts.
    if left == right {
        return Ok(Value::bool_val(true));
    }
    let case_fold = super::misc_eval::dynamic_or_global_symbol_value_in_state(
        &eval.obarray,
        &[],
        "case-fold-search",
    )
    .map(|v| !v.is_nil())
    .unwrap_or(true);
    if !case_fold {
        return Ok(Value::bool_val(false));
    }
    // GNU compares `downcase (i1) == downcase (i2)` using the current buffer's
    // downcase table (`editfns.c:4440`). Consult a custom case table first, then
    // fall through to the hardwired Unicode case-fold mapping.
    let casetab = super::super::casetab::CaseTableOverride::for_current_buffer(eval)?;
    if casetab.is_custom() {
        let down_left = casetab
            .map(super::super::casetab::CaseMap::Down, left)
            .unwrap_or_else(|| downcase_char_code_emacs_compat(left));
        let down_right = casetab
            .map(super::super::casetab::CaseMap::Down, right)
            .unwrap_or_else(|| downcase_char_code_emacs_compat(right));
        return Ok(Value::bool_val(down_left == down_right));
    }
    match (char_equal_folded(left), char_equal_folded(right)) {
        (Some(a), Some(b)) => Ok(Value::bool_val(a == b)),
        _ => Ok(Value::bool_val(left == right)),
    }
}
