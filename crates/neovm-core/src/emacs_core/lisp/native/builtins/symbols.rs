use super::*;
use crate::buffer::{CharLen, CharPos0, CharRange, LispCharPos1};
use crate::emacs_core::error::{
    expect_args, expect_args_range, expect_fixnum, expect_max_args, expect_min_args,
};
use crate::emacs_core::eval::{
    push_scratch_gc_root, restore_scratch_gc_roots, save_scratch_gc_roots,
};
use crate::emacs_core::fontset;
use crate::emacs_core::intern::{NIL_SYM_ID, T_SYM_ID, intern, is_canonical_id};
use crate::emacs_core::minibuffer;
use crate::emacs_core::symbol::{FunctionCellSnapshot, Obarray, SymbolPlistSnapshot};
use malachite::integer::Integer;

/// GNU `init_obarray_once` creates the initial obarray with size_bits = 15.
pub(crate) const GNU_INITIAL_OBARRAY_SIZE: usize = 1 << 15;

// ===========================================================================
// Symbol operations (need evaluator for obarray access)
// ===========================================================================

pub(crate) fn symbol_id(value: &Value) -> Option<SymId> {
    match value.kind() {
        ValueKind::Nil => Some(NIL_SYM_ID),
        ValueKind::T => Some(T_SYM_ID),
        ValueKind::Symbol(id) => Some(id),
        _ => {
            // Transparently unwrap symbol-with-pos → bare symbol.
            // The inner `.sym` is always a bare symbol, so one level
            // of unwrapping is sufficient and safe.
            if let Some(sym) = value.as_symbol_with_pos_sym() {
                symbol_id(&sym)
            } else {
                None
            }
        }
    }
}

pub(crate) fn symbol_id_checked(value: &Value, symbols_with_pos_enabled: bool) -> Option<SymId> {
    match value.kind() {
        ValueKind::Nil => Some(NIL_SYM_ID),
        ValueKind::T => Some(T_SYM_ID),
        ValueKind::Symbol(id) => Some(id),
        _ if symbols_with_pos_enabled => value
            .as_symbol_with_pos_sym()
            .and_then(|sym| symbol_id_checked(&sym, symbols_with_pos_enabled)),
        _ => None,
    }
}

pub(crate) fn expect_symbol_id_checked(
    value: &Value,
    symbols_with_pos_enabled: bool,
) -> Result<SymId, Flow> {
    symbol_id_checked(value, symbols_with_pos_enabled).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )
    })
}

fn overriding_plist_environment_symbol() -> SymId {
    static SYM: std::sync::OnceLock<SymId> = std::sync::OnceLock::new();
    *SYM.get_or_init(|| intern("overriding-plist-environment"))
}

fn assq_cdr_swp(key: &Value, alist: Value, symbols_with_pos_enabled: bool) -> Option<Value> {
    let mut cursor = alist;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        if entry.is_cons() && eq_value_swp(key, &entry.cons_car(), symbols_with_pos_enabled) {
            return Some(entry.cons_cdr());
        }
        cursor = cursor.cons_cdr();
    }
    None
}

fn value_from_symbol_id(id: SymId) -> Value {
    if is_canonical_id(id) {
        if id == NIL_SYM_ID {
            return Value::NIL;
        }
        if id == T_SYM_ID {
            return Value::T;
        }
        let name = resolve_sym(id);
        if name.starts_with(':') {
            return Value::from_kw_id(id);
        }
    }
    Value::from_sym_id(id)
}

pub(crate) trait MacroexpandRuntime {
    fn symbol_function_by_id(&self, symbol: SymId) -> Option<Value>;
    fn autoload_do_load_macro(&mut self, autoload: Value, head: Value) -> Result<(), Flow>;
    /// As autoload_do_load_macro, but keeps `rooted_form` GC-rooted for the
    /// load's duration: on macroexpand iterations >= 2 the in-flight
    /// expansion is a fresh unrooted cons structure that the load's GCs
    /// would otherwise free before the caller expands it further.
    fn autoload_do_load_macro_rooting(
        &mut self,
        autoload: Value,
        head: Value,
        rooted_form: Value,
    ) -> Result<(), Flow> {
        let _ = rooted_form;
        self.autoload_do_load_macro(autoload, head)
    }
    fn apply_macro_function(
        &mut self,
        form: Value,
        definition: Value,
        args: Vec<Value>,
        environment: Option<Value>,
    ) -> Result<Value, Flow>;
}

impl MacroexpandRuntime for super::eval::Context {
    fn symbol_function_by_id(&self, symbol: SymId) -> Option<Value> {
        symbol_function_cell_in_obarray(self.obarray(), symbol)
    }

    fn autoload_do_load_macro(&mut self, autoload: Value, head: Value) -> Result<(), Flow> {
        let _ = super::autoload::builtin_autoload_do_load(
            self,
            vec![autoload, head, Value::symbol("macro")],
        )?;
        Ok(())
    }

    fn autoload_do_load_macro_rooting(
        &mut self,
        autoload: Value,
        head: Value,
        rooted_form: Value,
    ) -> Result<(), Flow> {
        let root_scope = self.save_specpdl_roots();
        self.push_specpdl_root(rooted_form);
        let result = self.autoload_do_load_macro(autoload, head);
        self.restore_specpdl_roots(root_scope);
        result
    }

    fn apply_macro_function(
        &mut self,
        form: Value,
        definition: Value,
        args: Vec<Value>,
        environment: Option<Value>,
    ) -> Result<Value, Flow> {
        self.expand_macro_for_macroexpand(form, definition, args, environment)
    }
}

pub(crate) fn constant_set_outcome_in_obarray(
    obarray: &Obarray,
    symbol: SymId,
    symbol_arg: Value,
    new_value: Value,
) -> Option<EvalResult> {
    use crate::emacs_core::symbol::ConstantWrite;
    match obarray.classify_constant_write(symbol, new_value) {
        ConstantWrite::Writable => None,
        // GNU `set_internal` returns the new value without storing.
        ConstantWrite::KeywordSelfAssign => Some(Ok(new_value)),
        ConstantWrite::Refused => Some(Err(signal(
            LispCondition::SettingConstant,
            vec![symbol_arg],
        ))),
    }
}

pub(crate) fn expect_symbol_id(value: &Value) -> Result<SymId, Flow> {
    symbol_id(value).ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )
    })
}

pub(crate) fn is_canonical_symbol_id(id: SymId) -> bool {
    is_canonical_id(id)
}

pub(crate) fn resolve_variable_alias_id_in_obarray(
    obarray: &Obarray,
    symbol: SymId,
) -> Result<SymId, Flow> {
    // Phase 3 of the symbol-redirect refactor: walk via the new
    // `flags.redirect() == Varalias` + `val.alias` path through
    // `Obarray::indirect_variable_id`. Mirrors GNU's
    // `indirect_variable` (`src/data.c:1284-1301`) and the `goto
    // start` loop in `find_symbol_value` (`src/data.c:1593-1595`).
    //
    // Returns the chain terminus on success, or
    // `cyclic-variable-indirection` if a cycle is detected via Floyd's
    // tortoise/hare.
    obarray.indirect_variable_id(symbol).ok_or_else(|| {
        signal(
            LispCondition::CyclicVariableIndirection,
            vec![Value::from_sym_id(symbol)],
        )
    })
}

pub(crate) fn resolve_variable_alias_id(
    eval: &super::eval::Context,
    symbol: SymId,
) -> Result<SymId, Flow> {
    resolve_variable_alias_id_in_obarray(&eval.obarray, symbol)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn resolve_variable_alias_name(
    eval: &super::eval::Context,
    name: &str,
) -> Result<String, Flow> {
    resolve_variable_alias_name_in_obarray(&eval.obarray, name)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn resolve_variable_alias_name_in_obarray(
    obarray: &Obarray,
    name: &str,
) -> Result<String, Flow> {
    Ok(resolve_sym(resolve_variable_alias_id_in_obarray(obarray, intern(name))?).to_string())
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn would_create_variable_alias_cycle(eval: &super::eval::Context, new: &str, old: &str) -> bool {
    would_create_variable_alias_cycle_in_obarray(eval.obarray(), intern(new), intern(old))
}

pub(crate) fn would_create_variable_alias_cycle_in_obarray(
    obarray: &Obarray,
    new_symbol: SymId,
    old_symbol: SymId,
) -> bool {
    use crate::emacs_core::symbol::SymbolRedirect;

    // Phase 3: walk via the new redirect tag instead of the legacy
    // SymbolValue enum. Mirrors GNU `Fdefvaralias`'s base-chain walk
    // (`src/eval.c:631-726`).
    let mut current = old_symbol;
    loop {
        if current == new_symbol {
            return true;
        }
        match obarray.get_by_id(current) {
            Some(sym) if sym.flags.redirect() == SymbolRedirect::Varalias => {
                current = unsafe { sym.val.alias };
            }
            _ => return false,
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_boundp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("boundp", &args, 1)?;
    builtin_boundp_1(eval, args[0])
}

typed_subr! {
    pub(crate) fn builtin_boundp_1(eval, symbol: SymId) -> EvalResult {
        let obarray = eval.obarray();
        let resolved = resolve_variable_alias_id_in_obarray(obarray, symbol)?;
        // `boundp` runs constantly (font-lock/redisplay); a global (non-Localized)
        // symbol is never in any local_var_alist, so skip the per-buffer scan.
        let localized = obarray.is_localized(resolved);
        // specbind writes directly to obarray, so no dynamic stack lookup needed.
        if let Some(buf) = eval.buffers.current_buffer()
            && let Some(binding) = buf.get_buffer_local_binding_by_sym_id_gated(resolved, localized)
            {
                return Ok(Value::bool_val(binding.as_value().is_some()));
            }
        Ok(Value::bool_val(
            obarray.boundp_id(resolved) || obarray.is_constant_id(resolved),
        ))
    }
}

pub(crate) fn builtin_obarrayp(args: Vec<Value>) -> EvalResult {
    expect_args("obarrayp", &args, 1)?;
    Ok(Value::bool_val(is_obarray_value(args[0])))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_special_variable_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("special-variable-p", &args, 1)?;
    builtin_special_variable_p_1(eval, args[0])
}

typed_subr! {
    pub(crate) fn builtin_special_variable_p_1(eval, symbol: SymId) -> EvalResult {
        // Match GNU eval.c Fspecial_variable_p: this is a direct declared-special
        // bit test on the symbol itself, not an alias walk and not a constant
        // check.  Canonical keywords become special when materialized in the
        // initial obarray, mirroring lread.c intern_sym.
        eval.obarray_mut().ensure_interned_global_id(symbol);
        Ok(Value::bool_val(eval.obarray().is_special_id(symbol)))
    }
}

pub(crate) fn builtin_internal_define_uninitialized_variable(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("internal--define-uninitialized-variable", &args, 1, 2)?;
    let symbol = SymId::from_value(eval, args[0])?;
    let documentation = args.get(1).copied().unwrap_or(Value::NIL);

    if !eval.obarray().is_special_id(symbol) && eval.lexbound_p_in_specpdl(symbol) {
        return Err(signal(
            "error",
            vec![
                Value::string("Defining as dynamic an already lexical var"),
                args[0],
            ],
        ));
    }

    eval.note_macro_expansion_mutation();
    eval.obarray_mut().make_special_id(symbol);

    if !documentation.is_nil() {
        eval.obarray_mut().put_property_id(
            symbol,
            intern("variable-documentation"),
            documentation,
        )?;
    }

    // GNU `Finternal__define_uninitialized_variable` (eval.c:913) calls
    // LOADHIST_ATTACH(symbol), recording the bare defvar/defconst symbol on
    // current-load-list so it appears in the file's `load-history` entry.
    // The typed recorder self-guards on file-load context, so this is a no-op
    // outside a load.
    eval.record_load_history_entry(crate::emacs_core::eval::LoadHistoryEntry::Variable(symbol));

    Ok(Value::NIL)
}

pub(crate) fn builtin_defvaralias(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let state_change = defvaralias_impl(eval, args.clone())?;
    // GNU order (`eval.c:Fdefvaralias`): after validation and possible value
    // migration, notify watchers while NEW-ALIAS is still in its old state;
    // only then install the alias edge and write variable-documentation.
    eval.run_variable_watchers_by_id(
        state_change.previous_target_id,
        &state_change.base_variable,
        &Value::NIL,
        "defvaralias",
    )?;
    install_defvaralias_state(eval, &state_change);
    eval.watchers.clear_watchers(state_change.alias_id);
    // GNU `Fdefvaralias` treats the alias as a variable definition in its own
    // right: `LOADHIST_ATTACH (new_alias)` records the bare alias symbol after
    // the alias edge is installed and before its documentation is written.
    // This provenance is deliberately independent of the base variable.
    eval.record_load_history_entry(crate::emacs_core::eval::LoadHistoryEntry::Variable(
        state_change.alias_id,
    ));
    builtin_put(
        eval,
        vec![
            args[0],
            Value::symbol("variable-documentation"),
            state_change.docstring,
        ],
    )?;
    Ok(state_change.result)
}

/// GNU's signal for each way `Fdefvaralias` can refuse (`src/eval.c:647-679`).
///
/// `Cycle` is the one arm whose data is the BASE variable rather than the new
/// alias's name, which is why this is a match rather than one format string.
fn make_alias_error_signal(
    error: crate::emacs_core::symbol::MakeAliasError,
    new_name: &str,
    base_variable: Value,
) -> Flow {
    use crate::emacs_core::symbol::MakeAliasError;
    match error {
        MakeAliasError::Constant => signal(
            "error",
            vec![Value::string(format!(
                "Cannot make a constant an alias: {new_name}"
            ))],
        ),
        MakeAliasError::Cycle => signal(
            LispCondition::CyclicVariableIndirection,
            vec![base_variable],
        ),
        MakeAliasError::Forwarded => signal(
            "error",
            vec![Value::string(format!(
                "Cannot make a built-in variable an alias: {new_name}"
            ))],
        ),
        MakeAliasError::Localized => signal(
            "error",
            vec![Value::string(format!(
                // GNU's `error' runs its format string through `doprnt',
                // which applies `text-quoting-style' -- so the apostrophe in
                // the C source at `src/eval.c:672' reaches Lisp as U+2019.
                // Measured under GNU 31.0.90 `-Q --batch':
                //   "Don\u{2019}t know how to make a buffer-local variable an alias: l170z"
                "Don\u{2019}t know how to make a buffer-local variable an alias: {new_name}"
            ))],
        ),
        MakeAliasError::LetBound => signal(
            "error",
            vec![Value::string(format!(
                // `src/eval.c:709-710`, through the same `doprnt`
                // `text-quoting-style` path as the LOCALIZED message above.
                // Measured under GNU 31.0.90 `-Q --batch` (`tmp/l183-p6.el`):
                //   "Don\u{2019}t know how to make a let-bound variable an alias: l183p"
                "Don\u{2019}t know how to make a let-bound variable an alias: {new_name}"
            ))],
        ),
    }
}

pub(crate) struct DefvaraliasStateChange {
    pub(crate) alias_id: SymId,
    pub(crate) base_id: SymId,
    pub(crate) previous_target_id: SymId,
    pub(crate) base_variable: Value,
    pub(crate) docstring: Value,
    pub(crate) result: Value,
}

pub(crate) fn defvaralias_impl(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> Result<DefvaraliasStateChange, Flow> {
    expect_args_range("defvaralias", &args, 2, 3)?;
    let new_symbol = SymId::from_value(ctx, args[0])?;
    let old_symbol = SymId::from_value(ctx, args[1])?;
    let new_name = resolve_sym(new_symbol).to_string();
    // GNU's four refusals, from the one place that spells them
    // (`src/eval.c:647-679`).  Reaching them through the closed
    // [`MakeAliasError`] is what stops this subr from validating a subset
    // again: adding a fifth reason to the enum breaks this match.
    if let Err(error) = ctx.obarray.check_variable_alias(new_symbol, old_symbol) {
        return Err(make_alias_error_signal(error, &new_name, args[1]));
    }
    let previous_target_id = resolve_variable_alias_id_in_obarray(&ctx.obarray, new_symbol)?;
    // GNU's `if (NILP (Fboundp (base_variable)))` migration arm
    // (`src/eval.c:679-685`).  Its `else if` twin -- the "Overwriting value of
    // X by aliasing to Y" warning (`:686-701`) -- is measured missing and
    // recorded in ledger 183 rather than added: it calls `display-warning`,
    // which is Lisp, and 141 unit tests reach this primitive on a bare
    // `Context` where no Lisp is loaded.
    if ctx.obarray.find_symbol_value(old_symbol).is_none()
        && let Some(alias_value) = ctx.obarray.find_symbol_value(new_symbol)
    {
        let target = resolve_variable_alias_id_in_obarray(&ctx.obarray, old_symbol)?;
        ctx.obarray.set_symbol_value_id(target, alias_value);
        ctx.sync_cached_runtime_binding_by_id(target, alias_value);
        ctx.refresh_gc_runtime_settings_after_change_by_id(target);
    }
    // GNU's fifth refusal, and it is deliberately down here: the specpdl scan
    // runs AFTER the value migration above and after the "Overwriting value"
    // warning (`src/eval.c:702-711`), so a refused `defvaralias` has already
    // moved a value into an unbound BASE.  Measured in both editors
    // (`tmp/l183-p7.el`).
    if ctx.symbol_is_let_bound(new_symbol) {
        return Err(make_alias_error_signal(
            crate::emacs_core::symbol::MakeAliasError::LetBound,
            &new_name,
            args[1],
        ));
    }
    ctx.note_macro_expansion_mutation();
    let docstring = args.get(2).cloned().unwrap_or(Value::NIL);
    Ok(DefvaraliasStateChange {
        alias_id: new_symbol,
        base_id: old_symbol,
        previous_target_id,
        base_variable: args[1],
        docstring,
        result: args[1],
    })
}

pub(crate) fn install_defvaralias_state(
    ctx: &mut crate::emacs_core::eval::Context,
    state_change: &DefvaraliasStateChange,
) {
    ctx.obarray.make_special_id(state_change.alias_id);
    ctx.obarray
        .make_alias(state_change.alias_id, state_change.base_id);
    ctx.obarray.make_special_id(state_change.base_id);
    ctx.refresh_gc_runtime_settings_after_change_by_id(state_change.alias_id);
}

pub(crate) fn builtin_indirect_variable(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("indirect-variable", &args, 1)?;
    let obarray = eval.obarray();
    let Some(symbol) = symbol_id(&args[0]) else {
        return Ok(args[0]);
    };
    let resolved = resolve_variable_alias_id_in_obarray(obarray, symbol)?;
    Ok(value_from_symbol_id(resolved))
}

pub(crate) fn builtin_internal_delete_indirect_variable(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-delete-indirect-variable", &args, 1)?;
    let symbol = SymId::from_value(eval, args[0])?;
    if !eval.obarray().is_alias_id(symbol) {
        return Err(signal(
            "error",
            vec![
                Value::string("Cannot undeclare a variable that is not an alias"),
                args[0],
            ],
        ));
    }

    eval.note_macro_expansion_mutation();
    eval.obarray_mut().delete_variable_alias_id(symbol);
    eval.obarray_mut()
        .put_property_id(symbol, intern("variable-documentation"), Value::NIL)?;
    eval.makunbound_runtime_binding_by_id(symbol);
    Ok(args[0])
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_fboundp(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("fboundp", &args, 1)?;
    builtin_fboundp_1(eval, args[0])
}

typed_subr! {
    pub(crate) fn builtin_fboundp_1(eval, symbol: SymId) -> EvalResult {
        Ok(Value::bool_val(
            symbol_function_cell_in_obarray(eval.obarray(), symbol)
                .is_some_and(|function| !function.is_nil()),
        ))
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_symbol_value(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("symbol-value", &args, 1)?;
    builtin_symbol_value_1(eval, args[0])
}

pub(crate) fn builtin_symbol_value_1(
    eval: &mut super::eval::Context,
    symbol_value: Value,
) -> EvalResult {
    // The void-variable error reports the argument as given (a
    // symbol-with-pos keeps its wrapper), so extraction keeps the raw arg.
    let symbol = SymId::from_value(eval, symbol_value)?;
    match eval.visible_runtime_variable_value_by_id(symbol)? {
        Some(value) => Ok(value),
        None => Err(signal(LispCondition::VoidVariable, vec![symbol_value])),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_symbol_function(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("symbol-function", &args, 1)?;
    symbol_function_impl_1_checked(eval.obarray(), args[0], eval.symbols_with_pos_enabled)
}

typed_subr! {
    pub(crate) fn builtin_symbol_function_1(eval, symbol: SymId) -> EvalResult {
        symbol_function_by_id(eval.obarray(), symbol)
    }
}

/// Obarray-only implementation shared by `builtin_symbol_function` and doc.rs.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn symbol_function_impl(obarray: &Obarray, args: Vec<Value>) -> EvalResult {
    expect_args("symbol-function", &args, 1)?;
    symbol_function_impl_1(obarray, args[0])
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn symbol_function_impl_1(obarray: &Obarray, arg: Value) -> EvalResult {
    symbol_function_impl_1_checked(obarray, arg, false)
}

pub(crate) fn symbol_function_impl_1_checked(
    obarray: &Obarray,
    arg: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    let symbol = expect_symbol_id_checked(&arg, symbols_with_pos_enabled)?;
    symbol_function_by_id(obarray, symbol)
}

/// `symbol-function` after symbol extraction: the function cell by identity.
pub(crate) fn symbol_function_by_id(obarray: &Obarray, symbol: SymId) -> EvalResult {
    if obarray.is_function_unbound_id(symbol) {
        return Ok(Value::NIL);
    }

    if let Some(function) = obarray.symbol_function_id(symbol) {
        return Ok(function);
    }

    if !is_canonical_symbol_id(symbol) {
        return Ok(Value::NIL);
    }

    Ok(symbol_function_cell_in_obarray(obarray, symbol).unwrap_or(Value::NIL))
}

pub(crate) fn builtin_func_arity(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let obarray = eval.obarray();
    expect_args("func-arity", &args, 1)?;

    // Unwrap symbol-with-pos transparently for symbol name extraction.
    let arg0 = eval.unwrap_symbol(args[0]);
    if let Some(name) = arg0.as_symbol_name() {
        if let Some(function) =
            resolve_indirect_symbol_by_id_in_obarray(obarray, intern(name)).map(|(_, value)| value)
        {
            if function.is_nil() {
                return Err(signal(
                    LispCondition::VoidFunction,
                    vec![Value::symbol(name)],
                ));
            }
            if super::subr_info::subr_dispatch_kind_from_value(&function)
                .is_some_and(|kind| kind == crate::tagged::header::SubrDispatchKind::SpecialForm)
            {
                return super::subr_info::builtin_func_arity_ctx(eval, vec![function]);
            }
            if let Some(arity) =
                dispatch_symbol_func_arity_override_in_obarray(obarray, name, &function)
            {
                return Ok(arity);
            }
            return super::subr_info::builtin_func_arity_ctx(eval, vec![function]);
        }
        return Err(signal(
            LispCondition::VoidFunction,
            vec![Value::symbol(name)],
        ));
    }

    super::subr_info::builtin_func_arity_ctx(eval, vec![args[0]])
}

fn dispatch_symbol_func_arity_override_in_obarray(
    obarray: &Obarray,
    name: &str,
    function: &Value,
) -> Option<Value> {
    // Only applies to builtin functions (those with Subr function cells).
    if !obarray.symbol_function(name).is_some_and(|v| v.is_subr()) {
        return None;
    }

    if super::autoload::is_autoload_value(function) {
        return Some(super::subr_info::dispatch_subr_arity_value(name));
    }

    None
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_set(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("set", &args, 2)?;
    builtin_set_2(eval, args[0], args[1])
}

pub(crate) fn builtin_set_2(
    eval: &mut super::eval::Context,
    symbol_value: Value,
    value: Value,
) -> EvalResult {
    let symbol = SymId::from_value(eval, symbol_value)?;
    let resolved = resolve_variable_alias_id(eval, symbol)?;
    if let Some(result) =
        constant_set_outcome_in_obarray(eval.obarray(), resolved, symbol_value, value)
    {
        return result;
    }
    let where_value = eval.variable_watcher_where_for_set_by_id(resolved);
    eval.run_variable_watchers_by_id_with_where(
        resolved,
        &value,
        &Value::NIL,
        "set",
        &where_value,
    )?;
    eval.try_set_runtime_binding_by_id(resolved, value)?;
    Ok(value)
}

pub(crate) fn builtin_fset(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("fset", &args, 2)?;
    builtin_fset_2(eval, args[0], args[1])
}

pub(crate) fn builtin_fset_2(
    eval: &mut super::eval::Context,
    symbol_value: Value,
    def: Value,
) -> EvalResult {
    let symbol = SymId::from_value(eval, symbol_value)?;
    if symbol == intern("nil") && !def.is_nil() {
        return Err(signal(
            LispCondition::SettingConstant,
            vec![Value::symbol("nil")],
        ));
    }
    let would_cycle = {
        let obarray = eval.obarray_mut();
        would_create_function_alias_cycle_in_obarray(obarray, symbol, &def)
    };
    if would_cycle {
        return Err(signal(
            LispCondition::CyclicFunctionIndirection,
            vec![symbol_value],
        ));
    }
    eval.note_macro_expansion_mutation();
    eval.obarray_mut().set_symbol_function_id(symbol, def);
    crate::emacs_core::interactive::sync_interactive_registry_for_symbol_definition(
        &mut eval.interactive,
        symbol,
        def,
    );
    Ok(def)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn would_create_function_alias_cycle(
    eval: &super::eval::Context,
    target_symbol: SymId,
    def: &Value,
) -> bool {
    would_create_function_alias_cycle_in_obarray(eval.obarray(), target_symbol, def)
}

pub(crate) fn would_create_function_alias_cycle_in_obarray(
    obarray: &Obarray,
    target_symbol: SymId,
    def: &Value,
) -> bool {
    let mut current = match symbol_id(def) {
        Some(id) if id == intern("nil") => return false,
        Some(id) => id,
        None => return false,
    };
    let mut seen = HashSet::new();

    loop {
        if current == target_symbol {
            return true;
        }
        if !seen.insert(current) {
            return true;
        }

        let next = match obarray.symbol_function_id(current) {
            Some(function) => match symbol_id(&function) {
                Some(id) => id,
                None => return false,
            },
            None => return false,
        };
        current = next;
    }
}

pub(crate) fn builtin_makunbound(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("makunbound", &args, 1)?;
    let symbol = SymId::from_value(eval, args[0])?;
    let resolved = resolve_variable_alias_id(eval, symbol)?;
    if eval.obarray().is_constant_id(resolved) {
        return Err(signal(LispCondition::SettingConstant, vec![args[0]]));
    }
    crate::emacs_core::eval::check_forwarded_unbind(eval.obarray(), resolved, args[0])?;
    eval.note_macro_expansion_mutation();
    eval.run_variable_watchers_by_id(resolved, &Value::NIL, &Value::NIL, "makunbound")?;
    eval.makunbound_runtime_binding_by_id(resolved);
    Ok(args[0])
}

pub(crate) fn builtin_defvar_1(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("defvar-1", &args, 2, 3)?;
    let symbol = SymId::from_value(eval, args[0])?;
    let documentation = args.get(2).copied().unwrap_or(Value::NIL);
    let was_bound = super::super::data::default_boundp(eval, vec![args[0]])?.is_truthy();

    if documentation.is_nil() {
        builtin_internal_define_uninitialized_variable(eval, vec![args[0]])?;
    } else {
        builtin_internal_define_uninitialized_variable(eval, vec![args[0], documentation])?;
    }

    if !was_bound {
        crate::emacs_core::eval::set_default_toplevel_value(eval, vec![args[0], args[1]])?;
    }

    Ok(Value::from_sym_id(symbol))
}

pub(crate) fn builtin_defconst_1(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("defconst-1", &args, 2, 3)?;
    let symbol = SymId::from_value(eval, args[0])?;
    let documentation = args.get(2).copied().unwrap_or(Value::NIL);

    if documentation.is_nil() {
        builtin_internal_define_uninitialized_variable(eval, vec![args[0]])?;
    } else {
        builtin_internal_define_uninitialized_variable(eval, vec![args[0], documentation])?;
    }

    super::super::data::set_default(eval, vec![args[0], args[1]])?;
    let resolved = resolve_variable_alias_id(eval, symbol)?;
    eval.obarray_mut()
        .put_property_id(resolved, intern("risky-local-variable"), Value::T)?;

    Ok(Value::from_sym_id(symbol))
}

pub(crate) fn builtin_fmakunbound(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("fmakunbound", &args, 1)?;
    let symbol = SymId::from_value(eval, args[0])?;
    if symbol == intern("nil") || symbol == intern("t") {
        return Err(signal(LispCondition::SettingConstant, vec![args[0]]));
    }
    eval.note_macro_expansion_mutation();
    eval.obarray_mut().fmakunbound_id(symbol);
    Ok(args[0])
}

pub(crate) fn builtin_get(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("get", &args, 2)?;
    builtin_get_2(eval, args[0], args[1])
}

pub(crate) fn builtin_get_2(
    eval: &mut super::eval::Context,
    symbol_value: Value,
    prop: Value,
) -> EvalResult {
    Ok(symbol_property_get(eval, symbol_value, prop)?
        .1
        .unwrap_or(Value::NIL))
}

pub(crate) fn symbol_property_get(
    eval: &super::eval::Context,
    symbol_value: Value,
    prop: Value,
) -> Result<(SymId, Option<Value>), Flow> {
    let symbols_with_pos_enabled = eval.symbols_with_pos_enabled;
    let sym = expect_symbol_id_checked(&symbol_value, symbols_with_pos_enabled)?;

    // `overriding-plist-environment` is a special var (never lexically bound) and
    // is virtually always nil, so read its dynamic value directly and bail on the
    // common nil/non-cons case — matching GNU `Fget`'s single
    // `Voverriding_plist_environment` global load (fns.c). The old
    // `visible_variable_value_or_nil_by_id` ran a full lexenv-cache probe (which
    // thrashes during byte-compilation as the lexenv changes per closure/let) +
    // alias/redirect resolution on every `get`. `symbol_value_id_or_nil` still
    // follows defvaralias and reads the slot `specbind` writes, so a let-bound
    // override is honored.
    let overrides = eval
        .obarray()
        .symbol_value_id_or_nil(overriding_plist_environment_symbol());
    if overrides.is_cons()
        && let Some(plist) = assq_cdr_swp(&symbol_value, overrides, symbols_with_pos_enabled)
        && let Some(propval) =
            crate::emacs_core::plist::plist_get_swp(plist, &prop, symbols_with_pos_enabled)
        && !propval.is_nil()
    {
        return Ok((sym, Some(propval)));
    }

    let property = match eval.obarray().symbol_plist_snapshot_id(sym) {
        SymbolPlistSnapshot::NoEntries => None,
        SymbolPlistSnapshot::Entries(plist) => {
            crate::emacs_core::plist::plist_get_swp(plist, &prop, symbols_with_pos_enabled)
        }
    };
    Ok((sym, property))
}

pub(crate) fn builtin_put(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    ctx.note_macro_expansion_mutation();
    put_in_obarray(&mut ctx.obarray, args, ctx.symbols_with_pos_enabled)
}

pub(crate) fn builtin_put_3(
    ctx: &mut crate::emacs_core::eval::Context,
    symbol_value: Value,
    prop: Value,
    value: Value,
) -> EvalResult {
    ctx.note_macro_expansion_mutation();
    put_in_obarray_values(
        &mut ctx.obarray,
        symbol_value,
        prop,
        value,
        ctx.symbols_with_pos_enabled,
    )
}

pub(crate) fn put_in_obarray(
    obarray: &mut Obarray,
    args: Vec<Value>,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    expect_args("put", &args, 3)?;
    put_in_obarray_values(obarray, args[0], args[1], args[2], symbols_with_pos_enabled)
}

pub(crate) fn put_in_obarray_values(
    obarray: &mut Obarray,
    symbol_value: Value,
    prop: Value,
    value: Value,
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    let sym = expect_symbol_id_checked(&symbol_value, symbols_with_pos_enabled)?;

    let saved = save_scratch_gc_roots();
    push_scratch_gc_root(symbol_value);
    push_scratch_gc_root(prop);
    push_scratch_gc_root(value);
    let plist = obarray.symbol_plist_id(sym);
    let result =
        crate::emacs_core::plist::plist_put_swp(plist, prop, value, symbols_with_pos_enabled);
    if let Ok((new_plist, _changed)) = result {
        obarray.set_symbol_plist_id(sym, new_plist);
    }
    restore_scratch_gc_roots(saved);
    result?;
    Ok(value)
}

pub(crate) fn builtin_symbol_plist_fn(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("symbol-plist", &args, 1)?;
    let symbol = SymId::from_value(eval, args[0])?;
    Ok(eval.obarray().symbol_plist_id(symbol))
}

pub(super) fn builtin_register_code_conversion_map(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbols_with_pos_enabled = eval.symbols_with_pos_enabled;
    // Pre-validate the target symbol's plist shape BEFORE allocating a
    // map ID. If the plist is malformed, the subsequent `put` would
    // signal `(wrong-type-argument plistp …)` AFTER `register_code_
    // conversion_map_impl` has already consumed an ID — leaving the
    // counter in a non-GNU state. Shape-check first so the error path
    // is side-effect-free.
    if let Some(sym_id) = args
        .first()
        .and_then(|arg| symbol_id_checked(arg, symbols_with_pos_enabled))
    {
        let plist = eval
            .obarray()
            .get_by_id(sym_id)
            .map(|s| s.plist)
            .unwrap_or(Value::NIL);
        crate::emacs_core::plist::plist_check(plist)?;
    }

    let obarray = eval.obarray_mut();
    let map_id = super::ccl::builtin_register_code_conversion_map_impl(args.clone())?;

    let _ = put_in_obarray(
        obarray,
        vec![args[0], Value::symbol("code-conversion-map"), args[1]],
        symbols_with_pos_enabled,
    )?;
    let _ = put_in_obarray(
        obarray,
        vec![args[0], Value::symbol("code-conversion-map-id"), map_id],
        symbols_with_pos_enabled,
    )?;

    Ok(map_id)
}

fn symbol_has_valid_ccl_program_idx_in_obarray(
    obarray: &Obarray,
    symbol: &Value,
) -> Result<bool, Flow> {
    if !symbol.is_symbol() {
        return Ok(false);
    }
    let symbol = expect_symbol_id(symbol)?;
    let idx = obarray
        .get_property_id(symbol, intern("ccl-program-idx"))
        .unwrap_or(Value::NIL);
    Ok(idx.as_int().is_some_and(|n| n >= 0))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn symbol_has_valid_ccl_program_idx(
    eval: &mut super::eval::Context,
    symbol: &Value,
) -> Result<bool, Flow> {
    symbol_has_valid_ccl_program_idx_in_obarray(eval.obarray(), symbol)
}

pub(super) fn builtin_ccl_program_p(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let obarray = eval.obarray();
    if args.len() == 1 && args[0].is_symbol() {
        return Ok(Value::bool_val(
            symbol_has_valid_ccl_program_idx_in_obarray(obarray, &args[0])?,
        ));
    }
    super::ccl::builtin_ccl_program_p_impl(args)
}

pub(super) fn builtin_ccl_execute(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let obarray = eval.obarray();
    if args.first().is_some_and(|v| v.is_symbol())
        && !symbol_has_valid_ccl_program_idx_in_obarray(obarray, &args[0])?
    {
        let mut forced = args.clone();
        forced[0] = Value::fixnum(0);
        return super::ccl::builtin_ccl_execute_impl(forced);
    }
    super::ccl::builtin_ccl_execute_impl(args)
}

pub(super) fn builtin_ccl_execute_on_string(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let obarray = eval.obarray();
    if args.first().is_some_and(|v| v.is_symbol())
        && !symbol_has_valid_ccl_program_idx_in_obarray(obarray, &args[0])?
    {
        let mut forced = args.clone();
        forced[0] = Value::fixnum(0);
        return super::ccl::builtin_ccl_execute_on_string_impl(forced);
    }
    super::ccl::builtin_ccl_execute_on_string_impl(args)
}

pub(super) fn builtin_register_ccl_program(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let symbols_with_pos_enabled = eval.symbols_with_pos_enabled;
    let obarray = eval.obarray_mut();
    let was_registered = args
        .first()
        .and_then(|v| v.as_symbol_id())
        .is_some_and(super::ccl::is_registered_ccl_program);
    let program_id = super::ccl::builtin_register_ccl_program_impl(args.clone())?;

    if was_registered {
        return Ok(program_id);
    }

    let publish = put_in_obarray(
        obarray,
        vec![args[0], Value::symbol("ccl-program-idx"), program_id],
        symbols_with_pos_enabled,
    );
    if let Err(err) = publish {
        if let Some(name) = args[0].as_symbol_id() {
            super::ccl::unregister_registered_ccl_program(name);
        }
        return Err(err);
    }

    Ok(program_id)
}

pub(crate) fn builtin_setplist(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("setplist", &args, 2)?;
    let symbol = SymId::from_value(eval, args[0])?;
    let plist = args[1];
    eval.obarray_mut().set_symbol_plist_id(symbol, plist);
    Ok(plist)
}

fn macroexpand_environment_binding_by_id(env: &Value, target: SymId) -> Option<Value> {
    let mut cursor = *env;
    loop {
        match cursor.kind() {
            ValueKind::Nil => return None,
            ValueKind::Cons => {
                let entry = cursor.cons_car();
                cursor = cursor.cons_cdr();
                if !entry.is_cons() {
                    continue;
                };
                let entry_car = entry.cons_car();
                let entry_cdr = entry.cons_cdr();
                if matches!(symbol_id(&entry_car), Some(id) if id == target) {
                    return Some(entry_cdr);
                }
            }
            _ => return None,
        }
    }
}

fn macroexpand_environment_callable(binding: &Value) -> Result<Value, Flow> {
    Ok(*binding)
}

#[inline]
fn macroexpand_definition_is_macro(definition: &Value) -> bool {
    matches!(definition.kind(), ValueKind::Veclike(VecLikeType::Macro))
        || (definition.is_cons() && definition.cons_car().is_symbol_named("macro"))
}

/// Collect the elements of a list, signalling `(wrong-type-argument listp
/// BAD-CDR)` for an improper list — matching GNU's `list_length`
/// (`FOR_EACH_TAIL` + `CHECK_LIST_END (list, list)`), which reports only the
/// final non-nil cdr, not the whole improper tail.
fn collect_proper_list_args(list: Value) -> Result<Vec<Value>, Flow> {
    let mut items = Vec::new();
    let mut tail = list;
    while tail.is_cons() {
        items.push(tail.cons_car());
        tail = tail.cons_cdr();
    }
    if !tail.is_nil() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), tail],
        ));
    }
    Ok(items)
}

#[tracing::instrument(level = "trace", skip(runtime, environment), fields(head))]
fn macroexpand_once_with_environment<R: MacroexpandRuntime>(
    runtime: &mut R,
    form: Value,
    environment: Option<&Value>,
) -> Result<(Value, bool), Flow> {
    if !form.is_cons() {
        return Ok((form, false));
    };
    let form_pair_car = form.cons_car();
    let form_pair_cdr = form.cons_cdr();
    let head = form_pair_car;
    let tail = form_pair_cdr;
    let Some(head_id) = symbol_id(&head) else {
        return Ok((form, false));
    };
    if let Some(env) = environment
        && !env.is_nil()
        && !env.is_cons()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *env],
        ));
    }

    // Match GNU `eval.c:Fmacroexpand`: walk symbol aliases one hop at a time,
    // consulting ENVIRONMENT at each hop before following the function cell.
    let mut current_definition = head;
    let mut current_symbol = head_id;
    let mut environment_binding = None;
    while let Some(definition_symbol) = symbol_id(&current_definition) {
        current_symbol = definition_symbol;
        if let Some(env) = environment
            && let Some(binding) = macroexpand_environment_binding_by_id(env, definition_symbol)
        {
            environment_binding = Some(binding);
            break;
        }

        let Some(function) = runtime.symbol_function_by_id(definition_symbol) else {
            current_definition = Value::NIL;
            break;
        };
        current_definition = function;
        if !function.is_nil() {
            continue;
        }
        break;
    }

    let function = if let Some(binding) = environment_binding {
        if binding.is_nil() {
            None
        } else {
            Some(macroexpand_environment_callable(&binding)?)
        }
    } else {
        let mut global = current_definition;
        if super::autoload::is_autoload_value(&global) {
            // Root the in-flight expansion across the load (see the trait
            // method doc); autoload fires at most once per symbol, so the
            // single root is cheap.
            runtime.autoload_do_load_macro_rooting(
                global,
                value_from_symbol_id(current_symbol),
                form,
            )?;
            global = runtime
                .symbol_function_by_id(current_symbol)
                .unwrap_or(Value::NIL);
        }

        if macroexpand_definition_is_macro(&global) {
            Some(global)
        } else {
            None
        }
    };

    let Some(function) = function else {
        return Ok((form, false));
    };
    // GNU `apply1 (expander, XCDR (form))` calls `Fapply`, which runs
    // `list_length (spread_arg)`; that walks the list with `FOR_EACH_TAIL`
    // and ends with `CHECK_LIST_END (list, list)` — so an improper arglist
    // signals `(wrong-type-argument listp BAD-CDR)`, where BAD-CDR is the
    // final non-nil cdr, NOT the whole improper tail (fns.c:115/lisp.h:3332).
    let args = collect_proper_list_args(tail)?;
    let expanded = runtime.apply_macro_function(form, function, args, environment.copied())?;
    // Match real Emacs (eval.c line 1319): if the macro expander returned
    // the same form object (EQ), treat it as "no expansion occurred".
    let did_expand = !eq_value(&form, &expanded);
    Ok((expanded, did_expand))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_macroexpand_with_runtime<R: MacroexpandRuntime>(
    runtime: &mut R,
    args: Vec<Value>,
) -> EvalResult {
    builtin_macroexpand_slice_with_runtime(runtime, &args)
}

pub(crate) fn builtin_macroexpand_slice_with_runtime<R: MacroexpandRuntime>(
    runtime: &mut R,
    args: &[Value],
) -> EvalResult {
    expect_args_range("macroexpand", args, 1, 2)?;
    let mut form = args[0];
    let environment = args.get(1);
    loop {
        let (expanded, did_expand) = macroexpand_once_with_environment(runtime, form, environment)?;
        if !did_expand {
            return Ok(expanded);
        }
        form = expanded;
    }
}

pub(crate) fn builtin_macroexpand(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    builtin_macroexpand_slice(eval, &args)
}

pub(crate) fn builtin_macroexpand_slice(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    builtin_macroexpand_slice_with_runtime(eval, args)
}

pub(crate) fn builtin_indirect_function(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    crate::emacs_core::error::expect_args_range("indirect-function", &args, 1, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_indirect_function_2(eval, arg(0), arg(1))
}
/// `indirect-function` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_indirect_function_2(
    eval: &mut super::eval::Context,
    object: Value,
    noerror: Value,
) -> EvalResult {
    let args: [Value; 2] = [object, noerror];
    indirect_function_impl_checked(eval.obarray(), &args, eval.symbols_with_pos_enabled)
}

/// Obarray-only implementation shared by `builtin_indirect_function` and doc.rs.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn indirect_function_impl(obarray: &Obarray, args: Vec<Value>) -> EvalResult {
    indirect_function_impl_checked(obarray, &args, false)
}

pub(crate) fn indirect_function_impl_checked(
    obarray: &Obarray,
    args: &[Value],
    symbols_with_pos_enabled: bool,
) -> EvalResult {
    expect_min_args("indirect-function", &args, 1)?;
    expect_max_args("indirect-function", &args, 2)?;

    if let Some(symbol) = symbol_id_checked(&args[0], symbols_with_pos_enabled) {
        if let Some(function) = resolve_indirect_symbol_by_id_in_obarray_checked(
            obarray,
            symbol,
            symbols_with_pos_enabled,
        )
        .map(|(_, value)| value)
        {
            return Ok(function);
        }
        return Ok(Value::NIL);
    }

    Ok(args[0])
}

pub(crate) fn symbol_function_cell_in_obarray(obarray: &Obarray, symbol: SymId) -> Option<Value> {
    match obarray.function_cell_snapshot(symbol) {
        FunctionCellSnapshot::Bound(function) => Some(function),
        FunctionCellSnapshot::ExplicitlyUnbound | FunctionCellSnapshot::Empty => None,
    }
}

pub(crate) fn resolve_indirect_symbol_by_id_in_obarray(
    obarray: &Obarray,
    symbol: SymId,
) -> Option<(SymId, Value)> {
    resolve_indirect_symbol_by_id_in_obarray_checked(obarray, symbol, false)
}

pub(crate) fn resolve_indirect_symbol_by_id_in_obarray_checked(
    obarray: &Obarray,
    symbol: SymId,
    symbols_with_pos_enabled: bool,
) -> Option<(SymId, Value)> {
    let mut current = symbol;
    loop {
        let function = symbol_function_cell_in_obarray(obarray, current)?;
        if let Some(next) = symbol_id_checked(&function, symbols_with_pos_enabled) {
            if next == NIL_SYM_ID {
                return Some((next, Value::NIL));
            }
            current = next;
            continue;
        }
        return Some((current, function));
    }
}

pub(crate) fn resolve_indirect_symbol_by_id(
    eval: &super::eval::Context,
    symbol: SymId,
) -> Option<(SymId, Value)> {
    resolve_indirect_symbol_by_id_in_obarray(eval.obarray(), symbol)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn resolve_indirect_symbol_with_name(
    eval: &super::eval::Context,
    name: &str,
) -> Option<(String, Value)> {
    resolve_indirect_symbol_by_id(eval, intern(name))
        .map(|(resolved, value)| (resolve_sym(resolved).to_string(), value))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(super) fn resolve_indirect_symbol(eval: &super::eval::Context, name: &str) -> Option<Value> {
    resolve_indirect_symbol_with_name(eval, name).map(|(_, value)| value)
}

// `macrop' is not implemented here.  GNU has no DEFUN of that name: it is
// `(defun macrop (object) ...)' at lisp/subr.el:4793, built on
// `indirect-function' and `autoloadp'.  DIVERGENCES.md 148.

/// Hash a string for custom obarray bucket index.
pub(crate) fn obarray_hash_lisp_string(s: &crate::heap_types::LispString, len: usize) -> usize {
    let normalized = crate::emacs_core::intern::normalize_symbol_name_lisp_string(s);
    obarray_hash_bytes(normalized.as_bytes(), len)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn obarray_hash(s: &str, len: usize) -> usize {
    obarray_hash_bytes(s.as_bytes(), len)
}

fn obarray_hash_bytes(bytes: &[u8], len: usize) -> usize {
    if len == 0 {
        return 0;
    }

    let hash = crate::emacs_core::hashtab::reduce_emacs_uint_to_hash_hash(
        crate::emacs_core::hashtab::emacs_hash_char_array(bytes),
    );

    if len.is_power_of_two() {
        crate::emacs_core::hashtab::knuth_hash_index(hash, len.trailing_zeros())
    } else {
        (hash as usize) % len
    }
}

/// Immutable symbol-id snapshot in GNU obarray iteration order.
///
/// Completion consumes the ids directly, while Lisp APIs such as `mapatoms`
/// may materialize `Value`s from the same snapshot.  Keeping those two
/// ownership policies separate prevents the hot completion path from paying
/// for a parallel `Vec<Value>`.
pub(crate) fn global_obarray_symbol_ids_in_bucket_order(
    obarray: &Obarray,
    lisp_obarray: Value,
) -> std::sync::Arc<[crate::emacs_core::intern::SymId]> {
    let len = obarray_len(lisp_obarray)
        .filter(|len| *len > 1)
        .unwrap_or(GNU_INITIAL_OBARRAY_SIZE);
    // Avoid allocating `len` (~16K) bucket Vecs on every call -- that showed up
    // as ~28% of all-completions in `_int_malloc`.  Compute each symbol's
    // (bucket, insertion-order) key once and sort, reproducing the exact GNU
    // iteration order: bucket index ascending, then reverse insertion order
    // within a bucket (what `bucket.into_iter().rev()` produced). The order
    // is memoized per membership epoch: symbol names are immutable
    // (append-only interner) so it only changes on intern/unintern or an
    // obarray resize (the len cache key).
    obarray.completion_bucket_order_cached(len, || {
        // The order is a bucket index plus a position inside it, so it is a
        // counting sort, not a comparison sort: bucket every member once,
        // prefix-sum the counts, then scatter.  Scattering the members in
        // REVERSE membership order puts the newest first inside each bucket,
        // which is what the old `(bucket asc, order desc)` comparison
        // produced -- with ~18K symbols this replaced 6.2 M instructions of
        // `ipnsort` with two passes and two allocations.
        let members: Vec<(u32, crate::emacs_core::intern::SymId)> = obarray
            .global_member_ids()
            .map(|id| {
                let name = crate::emacs_core::intern::resolve_sym_lisp_string(id);
                (obarray_hash_lisp_string(name, len) as u32, id)
            })
            .collect();
        // `starts[b + 1]` counts bucket `b`, so the prefix sum leaves
        // `starts[b]` at bucket `b`'s first output index.
        let mut starts = vec![0u32; len + 1];
        for (bucket, _) in &members {
            starts[*bucket as usize + 1] += 1;
        }
        for index in 1..starts.len() {
            starts[index] += starts[index - 1];
        }
        let mut ordered = vec![crate::emacs_core::intern::NIL_SYM_ID; members.len()];
        for (bucket, id) in members.iter().rev() {
            let cursor = &mut starts[*bucket as usize];
            ordered[*cursor as usize] = *id;
            *cursor += 1;
        }
        ordered
    })
}

#[cfg(test)]
thread_local! {
    static GLOBAL_OBARRAY_SYMBOL_VALUE_MATERIALIZATIONS: std::cell::Cell<usize> =
        const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn reset_global_obarray_symbol_value_materializations() {
    GLOBAL_OBARRAY_SYMBOL_VALUE_MATERIALIZATIONS.set(0);
}

#[cfg(test)]
pub(crate) fn global_obarray_symbol_value_materializations() -> usize {
    GLOBAL_OBARRAY_SYMBOL_VALUE_MATERIALIZATIONS.get()
}

pub(crate) fn global_obarray_symbols_in_bucket_order(
    obarray: &Obarray,
    lisp_obarray: Value,
) -> Vec<Value> {
    #[cfg(test)]
    GLOBAL_OBARRAY_SYMBOL_VALUE_MATERIALIZATIONS
        .set(GLOBAL_OBARRAY_SYMBOL_VALUE_MATERIALIZATIONS.get() + 1);
    let ids = global_obarray_symbol_ids_in_bucket_order(obarray, lisp_obarray);
    ids.iter().copied().map(Value::from_sym_id).collect()
}

/// The name string a symbol answers to in an obarray.
///
/// GNU compares against the symbol's NAME OBJECT, which Lisp can mutate in
/// place -- `(aset name 0 ?X)` after `(intern name ob)` makes the symbol answer
/// to the new spelling and not the old one.
fn obarray_lookup_name(sym: Value) -> Option<crate::emacs_core::intern::LispVisibleSymbolName> {
    sym.as_symbol_id()
        .map(crate::emacs_core::intern::resolve_lisp_visible_symbol_name)
}

/// Search a bucket chain (cons list) for a symbol with the given name.
/// Returns the symbol Value if found.
pub(crate) fn obarray_bucket_find(
    bucket: Value,
    name: &crate::heap_types::LispString,
) -> Option<Value> {
    // Normalize BOTH sides: a symbol created from an ascii-only multibyte name
    // keeps that multibyte string as its name object (GNU `intern_driver`), so
    // the stored name can carry a representation the query does not. GNU's
    // `oblookup` never compares the multibyte flag, only chars and bytes.
    let normalized = crate::emacs_core::intern::normalize_symbol_name_lisp_string(name);
    let mut current = bucket;
    loop {
        match current.kind() {
            ValueKind::Nil => return None,
            ValueKind::Cons => {
                let car = current.cons_car();
                let cdr = current.cons_cdr();
                if let Some(sym_name) = obarray_lookup_name(car)
                    && crate::emacs_core::intern::normalize_symbol_name_lisp_string(sym_name.text())
                        .as_ref()
                        == normalized.as_ref()
                {
                    return Some(car);
                }
                current = cdr;
            }
            _ => return None,
        }
    }
}

fn obarray_bucket_symbols(mut bucket: Value) -> Vec<Value> {
    let mut symbols = Vec::new();
    while bucket.is_cons() {
        let sym = bucket.cons_car();
        if sym.as_symbol_lisp_string().is_some() {
            symbols.push(sym);
        }
        bucket = bucket.cons_cdr();
    }
    symbols
}

fn obarray_symbol_count(buckets: &[Value]) -> usize {
    buckets
        .iter()
        .map(|bucket| obarray_bucket_symbols(*bucket).len())
        .sum()
}

fn grow_obarray_vector_if_needed(obarray_val: Value) {
    let Some(buckets) = obarray_buckets(obarray_val) else {
        return;
    };
    let old_len = buckets.len();
    if old_len == 0 || obarray_symbol_count(buckets.as_slice()) <= old_len {
        return;
    }

    let mut new_buckets = vec![Value::NIL; old_len.saturating_mul(2).max(1)];
    for bucket in buckets.iter().copied() {
        for sym in obarray_bucket_symbols(bucket) {
            // Rehash on the same name a lookup will hash: the name object when
            // the symbol has one, so a grown obarray still finds its symbols.
            let Some(name) = obarray_lookup_name(sym) else {
                continue;
            };
            let idx = obarray_hash_lisp_string(name.text(), new_buckets.len());
            new_buckets[idx] = Value::cons(sym, new_buckets[idx]);
        }
    }
    let _ = replace_obarray_buckets(obarray_val, new_buckets);
}

pub(crate) fn is_global_obarray_proxy(eval: &super::eval::Context, value: &Value) -> bool {
    #[inline(always)]
    fn neovm_obarray_object_sym() -> crate::emacs_core::intern::SymId {
        static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
            std::sync::OnceLock::new();
        *SYMBOL.get_or_init(|| crate::emacs_core::intern::intern("neovm--obarray-object"))
    }
    eval.obarray()
        .symbol_value_id(neovm_obarray_object_sym())
        .is_some_and(|proxy| *proxy == *value)
}

fn current_lisp_obarray_value(eval: &super::eval::Context) -> Value {
    eval.obarray()
        .symbol_value("obarray")
        .copied()
        .unwrap_or_else(|| {
            eval.obarray()
                .symbol_value("neovm--obarray-object")
                .copied()
                .unwrap_or(Value::NIL)
        })
}

fn effective_obarray_arg(eval: &super::eval::Context, args: &[Value]) -> Value {
    args.get(1)
        .copied()
        .filter(|value| !value.is_nil())
        .unwrap_or_else(|| current_lisp_obarray_value(eval))
}

pub(crate) fn builtin_intern_fn(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_min_args("intern", &args, 1)?;
    expect_max_args("intern", &args, 2)?;
    // Debug: validate string arg before access
    if args[0].is_string() {
        let ptr = args[0].as_string_ptr().unwrap();
        let header = unsafe { &(*ptr).header };
        if !matches!(header.kind, crate::tagged::header::HeapObjectKind::String) {
            // Dump bc_buf state for debugging
            let bc_buf_len = eval.bc_buf.len();
            let _bc_frames_len = eval.bc_frames.len();
            let bc_frames_info: Vec<String> = eval
                .bc_frames
                .iter()
                .map(|f| format!("base={} fun={:#x}", f.base, f.fun.0))
                .collect();
            panic!(
                "INTERN BUG: string arg {:#x} (ptr {:?}) has header.kind={:?}\n\
                 bc_buf.len()={}, bc_frames={:?}\n\
                 All args: {:?}",
                args[0].0,
                ptr,
                header.kind,
                bc_buf_len,
                bc_frames_info,
                args.iter()
                    .map(|a| format!("{:#x}", a.0))
                    .collect::<Vec<_>>(),
            );
        }
    }
    let name = eval.expect_lisp_string(args[0])?;

    let effective_obarray = effective_obarray_arg(eval, &args);

    // Custom obarray path
    if !is_global_obarray_proxy(eval, &effective_obarray) {
        let obarray_val = check_obarray_value(effective_obarray)?;
        let vec_len = obarray_len(obarray_val).unwrap_or(0);
        if vec_len == 0 {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("obarrayp"), effective_obarray],
            ));
        }
        let bucket_idx = obarray_hash_lisp_string(name, vec_len);
        let bucket = obarray_bucket(obarray_val, bucket_idx).unwrap_or(Value::NIL);

        // Check if already interned
        if let Some(sym) = obarray_bucket_find(bucket, name) {
            return Ok(sym);
        }

        // Not found: create the symbol from the string we were handed and
        // prepend it to the bucket chain. GNU has ONE creation path for both
        // obarrays -- `intern_driver (string, ...)` -> `Fmake_symbol (string)`
        // (lread.c:4705-4708) -- so the argument becomes the name object here
        // exactly as it does globally, keeping its text properties and its
        // multibyteness. Lookup already normalizes an ascii-only multibyte
        // spelling to its unibyte bytes (`obarray_hash_lisp_string`,
        // `obarray_bucket_find`), so identity does not depend on the name
        // object's representation.
        let sym = Value::from_sym_id(
            crate::emacs_core::intern::make_uninterned_symbol_with_name_value(args[0]),
        );
        let new_bucket = Value::cons(sym, bucket);
        let _ = set_obarray_bucket(obarray_val, bucket_idx, new_bucket);
        note_obarray_symbol_added(obarray_val);
        grow_obarray_vector_if_needed(obarray_val);
        return Ok(sym);
    }

    // Global obarray path. Pass the string OBJECT, not just its bytes: when
    // this call creates the symbol that object becomes its name, so
    // `symbol-name` returns it with the text properties it was read from.
    let sym = eval.obarray_mut().intern_lisp_value(args[0]);
    Ok(Value::from_sym_id(sym))
}

pub(crate) fn builtin_intern_soft(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    crate::emacs_core::error::expect_args_range("intern-soft", &args, 1, 2)?;
    let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
    builtin_intern_soft_2(eval, arg(0), arg(1))
}
/// `intern-soft` as registered: fixed arity 2, called straight off the bytecode
/// stack like GNU `funcall_subr`'s `a2` case (absent optionals arrive as nil).
/// The `Vec` entry point above serves Rust callers.
pub(crate) fn builtin_intern_soft_2(
    eval: &mut super::eval::Context,
    name: Value,
    obarray: Value,
) -> EvalResult {
    let args: [Value; 2] = [name, obarray];
    intern_soft_impl(eval, &args)
}

pub(crate) fn intern_soft_impl(eval: &super::eval::Context, args: &[Value]) -> EvalResult {
    expect_min_args("intern-soft", &args, 1)?;
    expect_max_args("intern-soft", &args, 2)?;

    let effective_obarray = effective_obarray_arg(eval, &args);

    // Custom obarray path
    if !is_global_obarray_proxy(eval, &effective_obarray) {
        let obarray_val = check_obarray_value(effective_obarray)?;
        // GNU searches for `SYMBOL_NAME (name)` -- the name OBJECT, the same
        // string the bucket chain is compared on. The view has to outlive the
        // `Cow` that borrows out of it, so it is bound before the match rather
        // than produced inside one of its arms.
        let symbol_name_view = match args[0].kind() {
            ValueKind::Symbol(id) => Some(
                crate::emacs_core::intern::resolve_lisp_visible_symbol_name(id),
            ),
            ValueKind::Nil => Some(crate::emacs_core::intern::resolve_lisp_visible_symbol_name(
                NIL_SYM_ID,
            )),
            ValueKind::T => Some(crate::emacs_core::intern::resolve_lisp_visible_symbol_name(
                T_SYM_ID,
            )),
            _ => None,
        };
        let name = match (&symbol_name_view, args[0].kind()) {
            (Some(view), _) => std::borrow::Cow::Borrowed(view.text()),
            (None, ValueKind::String) => {
                std::borrow::Cow::Borrowed(args[0].as_lisp_string().unwrap())
            }
            (None, _other) => {
                // Transparently unwrap symbol-with-pos → bare symbol name.
                if let Some(id) = symbol_id(&args[0]) {
                    std::borrow::Cow::Borrowed(crate::emacs_core::intern::resolve_sym_lisp_string(
                        id,
                    ))
                } else {
                    return Err(signal(
                        LispCondition::WrongTypeArgument,
                        vec![Value::symbol("stringp"), args[0]],
                    ));
                }
            }
        };
        let vec_len = obarray_len(obarray_val).unwrap_or(0);
        if vec_len == 0 {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("obarrayp"), effective_obarray],
            ));
        }
        let bucket_idx = obarray_hash_lisp_string(name.as_ref(), vec_len);
        let bucket = obarray_bucket(obarray_val, bucket_idx).unwrap_or(Value::NIL);
        return Ok(obarray_bucket_find(bucket, name.as_ref()).unwrap_or(Value::NIL));
    }

    // Global obarray path
    let name = match args[0].kind() {
        ValueKind::String => std::borrow::Cow::Borrowed(args[0].as_lisp_string().unwrap()),
        ValueKind::Nil => std::borrow::Cow::Borrowed(
            crate::emacs_core::intern::resolve_sym_lisp_string(NIL_SYM_ID),
        ),
        ValueKind::T => {
            std::borrow::Cow::Borrowed(crate::emacs_core::intern::resolve_sym_lisp_string(T_SYM_ID))
        }
        ValueKind::Symbol(id) => {
            std::borrow::Cow::Borrowed(crate::emacs_core::intern::resolve_sym_lisp_string(id))
        }
        _other => {
            // Transparently unwrap symbol-with-pos → bare symbol name.
            if let Some(id) = symbol_id(&args[0]) {
                std::borrow::Cow::Borrowed(crate::emacs_core::intern::resolve_sym_lisp_string(id))
            } else {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), args[0]],
                ));
            }
        }
    };
    if let Some(id) = eval.obarray().intern_soft_lisp_string(name.as_ref()) {
        Ok(Value::from_sym_id(id))
    } else {
        Ok(Value::NIL)
    }
}

pub(crate) fn builtin_obarray_make(args: Vec<Value>) -> EvalResult {
    expect_args_range("obarray-make", &args, 0, 1)?;
    let size = if args.is_empty() || args[0].is_nil() {
        1511usize
    } else {
        expect_wholenump(&args[0])? as usize
    };
    Ok(Value::obarray(size))
}

fn is_legacy_obarray_vector(value: Value) -> bool {
    value.is_vector()
        && value
            .as_vector_data()
            .is_some_and(|items| items.iter().all(|slot| slot.is_nil() || slot.is_cons()))
}

fn is_obarray_value(value: Value) -> bool {
    value.is_obarray() || is_legacy_obarray_vector(value)
}

fn make_compat_obarray() -> Value {
    // GNU lread.c:check_obarray_slow calls make_obarray(0), producing a
    // one-bucket obarray for the legacy vector slot-zero compatibility path.
    Value::obarray(1)
}

pub(crate) fn check_obarray_value(value: Value) -> Result<Value, Flow> {
    if is_obarray_value(value) {
        return Ok(value);
    }

    if value.is_vector() {
        let slots = value.as_vector_data().unwrap();
        if let Some(slot0) = slots.first().copied() {
            if is_obarray_value(slot0) {
                return Ok(slot0);
            }
            if slot0 == Value::fixnum(0) {
                let obarray = make_compat_obarray();
                let _ = value.set_vector_slot(0, obarray);
                return Ok(obarray);
            }
        }
    }

    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("obarrayp"), value],
    ))
}

pub(crate) fn expect_obarray_vector_id(value: &Value) -> Result<Value, Flow> {
    check_obarray_value(*value)
}

pub(crate) fn obarray_buckets(value: Value) -> Option<Vec<Value>> {
    if value.is_obarray() {
        return value
            .as_obarray_obj()
            .map(|obj| obj.buckets.as_slice().to_vec());
    }
    value.as_vector_data().map(|items| items.to_vec())
}

pub(crate) fn obarray_bucket(value: Value, idx: usize) -> Option<Value> {
    if value.is_obarray() {
        return value
            .as_obarray_obj()
            .and_then(|obj| obj.buckets.get(idx).copied());
    }
    value
        .as_vector_data()
        .and_then(|items| items.get(idx).copied())
}

pub(crate) fn obarray_len(value: Value) -> Option<usize> {
    if value.is_obarray() {
        return value.as_obarray_obj().map(|obj| obj.buckets.len());
    }
    value.as_vector_data().map(|items| items.len())
}

pub(crate) fn set_obarray_bucket(value: Value, idx: usize, bucket: Value) -> bool {
    if value.is_obarray() {
        return value
            .with_obarray_mut(|obj| {
                if let Some(slot) = obj.buckets.get_mut(idx) {
                    *slot = bucket;
                    true
                } else {
                    false
                }
            })
            .unwrap_or(false);
    }
    value.set_vector_slot(idx, bucket)
}

pub(crate) fn replace_obarray_buckets(value: Value, buckets: Vec<Value>) -> bool {
    if value.is_obarray() {
        return value
            .with_obarray_mut(|obj| {
                obj.buckets = buckets.into();
                true
            })
            .unwrap_or(false);
    }
    value.replace_vector_data(buckets)
}

pub(crate) fn note_obarray_symbol_added(value: Value) {
    let _ = value.with_obarray_mut(|obj| {
        obj.count = obj.count.saturating_add(1);
    });
}

pub(crate) fn note_obarray_symbol_removed(value: Value) {
    let _ = value.with_obarray_mut(|obj| {
        obj.count = obj.count.saturating_sub(1);
    });
}

pub(crate) fn builtin_obarray_clear(args: Vec<Value>) -> EvalResult {
    expect_args("obarray-clear", &args, 1)?;
    let obarray_val = expect_obarray_vector_id(&args[0])?;
    let vec_len = obarray_len(obarray_val).unwrap_or(0);
    let _ = replace_obarray_buckets(obarray_val, vec![Value::NIL; vec_len]);
    let _ = obarray_val.with_obarray_mut(|obj| obj.count = 0);
    Ok(Value::NIL)
}

pub(crate) fn builtin_make_temp_file_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    super::fileio::builtin_make_temp_file_internal(eval, args)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_minibuffer_innermost_command_loop_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("minibuffer-innermost-command-loop-p", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_next_frame(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args_range("next-frame", &args, 0, 2)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
    {
        let _ = super::window_cmds::resolve_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            Some(frame),
            "frame-live-p",
        )?;
    }
    crate::emacs_core::frame::builtin_selected_frame(eval, Vec::new())
}

pub(crate) fn builtin_previous_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("previous-frame", &args, 0, 2)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
    {
        let _ = super::window_cmds::resolve_frame_id_in_state(
            &mut eval.frames,
            &mut eval.buffers,
            Some(frame),
            "frame-live-p",
        )?;
    }
    crate::emacs_core::frame::builtin_selected_frame(eval, Vec::new())
}

pub(crate) fn builtin_raise_frame(args: Vec<Value>) -> EvalResult {
    expect_args_range("raise-frame", &args, 0, 1)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_redisplay(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("redisplay", &args, 0, 1)?;
    if eval
        .eval_symbol("executing-kbd-macro")
        .is_ok_and(|value| !value.is_nil())
    {
        return Ok(Value::NIL);
    }
    let force = args.first().is_some_and(|value| value.is_truthy());
    eval.redisplay_with_force(force);
    Ok(Value::T)
}

pub(crate) fn builtin_suspend_emacs(args: Vec<Value>) -> EvalResult {
    expect_args_range("suspend-emacs", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_rename_buffer(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("rename-buffer", &args, 1, 2)?;
    let requested_name = args[0];
    let name = expect_string_lossy(&requested_name)?;

    if name.is_empty() {
        return Err(signal(
            "error",
            vec![Value::string("Empty string is invalid as a buffer name")],
        ));
    }

    let (current_id, old_name) = match eval.buffers.current_buffer() {
        Some(buf) => (buf.id, buf.name_value()),
        None => {
            return Err(signal("error", vec![Value::string("No current buffer")]));
        }
    };
    let old_name_text = expect_string_lossy(&old_name)?;
    let unique = args.get(1).copied().unwrap_or(Value::NIL);

    let new_name = match eval.buffers.find_buffer_by_name(&name) {
        Some(existing_id) if existing_id == current_id && unique.is_nil() => {
            // GNU returns the buffer's stored name object, not an equal string
            // supplied by the caller, and performs no rename side effects.
            return Ok(old_name);
        }
        Some(_other_id) => {
            if unique.is_nil() {
                return Err(signal(
                    "error",
                    vec![Value::string(format!("Buffer name `{}' is in use", name))],
                ));
            }
            super::super::buffer::generate_new_buffer_name_value_in_state(
                &eval.buffers,
                requested_name,
                Some(&old_name_text),
            )?
        }
        None => requested_name,
    };

    let _ = eval.buffers.rename_buffer(current_id, new_name);
    // GNU `Frename_buffer` calls `bset_update_mode_line` only after a real
    // rename (buffer.c:1690-1720).  That helper raises global
    // `update_mode_lines` even when CURRENT-BUFFER is offscreen, which makes
    // a subsequent file/Dired buffer switch rebuild the selected window's
    // menu as well as its chrome.
    eval.request_global_mode_line_update();

    // GNU `Frename_buffer` (buffer.c:1726) runs `buffer-list-update-hook'
    // after updating the buffer's name in `Vbuffer_alist', unless the buffer
    // has hooks inhibited.  This also makes `set-visited-file-name' (which
    // renames the buffer via `rename-buffer') fire the hook like GNU.
    if !eval.buffers.buffer_hooks_inhibited(current_id) {
        super::super::buffer::run_buffer_list_update_hook(eval)?;
    }

    // Hooks may themselves rename the buffer. GNU refetches the slot after
    // running them rather than returning a stale local copy.
    Ok(eval
        .buffers
        .get(current_id)
        .map(|buffer| buffer.name_value())
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_set_buffer_major_mode(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-major-mode", &args, 1)?;
    let buffer_id = expect_buffer_id(&args[0])?;
    let Some(target_buf) = eval.buffers.get(buffer_id) else {
        return Err(signal(
            "error",
            vec![Value::string("Attempt to set major mode for a dead buffer")],
        ));
    };

    let mut function = if target_buf.name_value() == Value::string("*scratch*") {
        eval.visible_variable_value_or_nil("initial-major-mode")
    } else {
        crate::buffer::buffer::lookup_buffer_slot("major-mode")
            .map(|info| eval.buffers.buffer_defaults[info.offset.index()])
            .unwrap_or(Value::NIL)
    };

    if function.is_nil() {
        let current_major_mode = eval.visible_variable_value_or_nil("major-mode");
        if !current_major_mode.is_nil() {
            let mode_class = eval.funcall_general(
                Value::symbol("get"),
                vec![current_major_mode, Value::symbol("mode-class")],
            )?;
            if mode_class.is_nil() {
                function = current_major_mode;
            }
        }
    }

    if function.is_nil() {
        return Ok(Value::NIL);
    }

    let saved_current = eval.buffers.current_buffer_id();
    let mode_result = (|| -> EvalResult {
        eval.switch_current_buffer(buffer_id)?;
        let _ = eval.funcall_general(function, vec![])?;
        Ok(Value::NIL)
    })();

    if let Some(prev_id) = saved_current {
        eval.restore_current_buffer_if_live(prev_id);
    }

    mode_result
}

pub(crate) fn builtin_set_buffer_redisplay(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-buffer-redisplay", &args, 4)?;
    eval.invalidate_redisplay();
    Ok(Value::NIL)
}

pub(crate) fn builtin_re_describe_compiled(args: Vec<Value>) -> EvalResult {
    expect_args_range("re--describe-compiled", &args, 1, 2)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_map_charset_chars(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("map-charset-chars", &args, 2, 5)?;
    let charset = args[1].as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("charsetp"), args[1]],
        )
    })?;
    let from_code = match args.get(3).copied().filter(|v| !v.is_nil()) {
        Some(value) => Some(expect_wholenump(&value)?),
        None => None,
    };
    let to_code = match args.get(4).copied().filter(|v| !v.is_nil()) {
        Some(value) => Some(expect_wholenump(&value)?),
        None => None,
    };
    let ranges = crate::emacs_core::charset::map_charset_char_ranges(charset, from_code, to_code)
        .ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("charsetp"), args[1]],
        )
    })?;
    let function = args[0];
    let arg = args.get(2).copied().unwrap_or(Value::NIL);
    for (from, to) in ranges {
        let range = Value::cons(Value::fixnum(i64::from(from)), Value::fixnum(i64::from(to)));
        eval.funcall_general(function, vec![range, arg])?;
    }
    Ok(Value::NIL)
}

// map-keymap and map-keymap-internal are now eval-backed in keymaps.rs

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_mapbacktrace(args: Vec<Value>) -> EvalResult {
    expect_args_range("mapbacktrace", &args, 1, 2)?;
    match args[0].kind() {
        ValueKind::Nil | ValueKind::T => {
            return Err(signal(LispCondition::VoidFunction, vec![args[0]]));
        }
        ValueKind::Symbol(_)
        | ValueKind::Subr(_)
        | ValueKind::Veclike(VecLikeType::Subr)
        | ValueKind::Veclike(VecLikeType::Lambda)
        | ValueKind::Veclike(VecLikeType::Macro)
        | ValueKind::Veclike(VecLikeType::ByteCode) => {}
        _ => {
            return Err(signal(LispCondition::InvalidFunction, vec![args[0]]));
        }
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_make_record(args: Vec<Value>) -> EvalResult {
    expect_args("make-record", &args, 3)?;
    let length = expect_wholenump(&args[1])? as usize;
    let mut items = Vec::with_capacity(length + 1);
    items.push(args[0]); // type tag
    for _ in 0..length {
        items.push(args[2]); // init value
    }
    Ok(Value::make_record(items))
}

pub(crate) fn builtin_marker_last_position(args: Vec<Value>) -> EvalResult {
    expect_args("marker-last-position", &args, 1)?;
    if !super::marker::is_marker(&args[0]) {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("markerp"), args[0]],
        ));
    }
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Marker) => {
            let marker = args[0].as_marker_data().unwrap();
            // GNU `Fmarker_last_position` (marker.c:458) returns the
            // marker's last known position even after detach.  Neomacs
            // tracks this with `last_position_valid`, set the first time
            // the marker is positioned and preserved across detach.
            let last = if marker.last_position_valid {
                CharPos0::new(marker.charpos).to_lisp().as_i64()
            } else {
                0
            };
            Ok(Value::fixnum(last))
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = args[0].as_vector_data().unwrap().clone();
            Ok(items
                .get(2)
                .and_then(|value| value.as_fixnum())
                .map(Value::fixnum)
                .unwrap_or_else(|| Value::fixnum(0)))
        }
        _ => unreachable!("markerp check above guarantees a tagged marker object"),
    }
}

pub(crate) fn builtin_newline_cache_check(args: Vec<Value>) -> EvalResult {
    expect_args_range("newline-cache-check", &args, 0, 1)?;
    if let Some(buffer) = args.first()
        && !buffer.is_nil()
        && !buffer.is_buffer()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *buffer],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_old_selected_frame(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("old-selected-frame", &args, 0)?;
    crate::emacs_core::frame::builtin_selected_frame(eval, Vec::new())
}

pub(crate) fn builtin_menu_bar_menu_at_x_y(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("menu-bar-menu-at-x-y", &args, 2, 3)?;
    let x = args[0].as_fixnum().unwrap_or(0);
    let mut hpos = 0i64;
    let frame_id = args
        .get(2)
        .and_then(|frame| frame.as_frame_id().map(crate::window::FrameId))
        .or_else(|| eval.frames.selected_frame().map(|frame| frame.id));
    if let Some(anchor) = &eval.pending_menu_bar_popup_anchor
        && frame_id.is_none_or(|id| id == anchor.frame_id)
        && let Some(menu_key) = &anchor.menu_key
    {
        return Ok(Value::symbol(menu_key));
    }
    for (key, label) in menu_bar_top_level_items_for_frame(eval, frame_id) {
        let width = label.chars().count() as i64 + 1;
        if x >= hpos && x < hpos + width {
            return Ok(key);
        }
        hpos += width;
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_menu_or_popup_active_p(args: Vec<Value>) -> EvalResult {
    expect_args("menu-or-popup-active-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn menu_bar_top_level_items(eval: &super::eval::Context) -> Vec<(Value, String)> {
    menu_bar_top_level_items_for_frame(eval, eval.frames.selected_frame().map(|frame| frame.id))
}

pub(crate) fn menu_bar_top_level_items_for_frame(
    eval: &super::eval::Context,
    frame_id: Option<crate::window::FrameId>,
) -> Vec<(Value, String)> {
    let mut items = Vec::new();
    let active_maps = if let Some(frame_id) = frame_id {
        crate::emacs_core::keymap::menu_bar_active_keymaps_for_frame_read_only(eval, frame_id)
    } else {
        let obey_overriding_local_maps = eval
            .obarray
            .symbol_value("overriding-local-map-menu-flag")
            .copied()
            .is_some_and(|value| value.is_truthy());
        let mut maps = crate::emacs_core::keymap::current_active_maps_for_position_read_only(
            eval,
            obey_overriding_local_maps,
            None,
        )
        .unwrap_or_default();
        maps.reverse();
        maps
    };
    for keymap in active_maps {
        collect_menu_bar_items_from_map(eval, keymap, &mut items);
    }
    move_menu_bar_final_items(eval, &mut items);
    items
}

fn move_menu_bar_final_items(eval: &super::eval::Context, items: &mut Vec<(Value, String)>) {
    let Some(mut final_items) = eval.obarray().symbol_value("menu-bar-final-items").copied() else {
        return;
    };
    while final_items.is_cons() {
        let key = final_items.cons_car();
        if let Some(index) = items
            .iter()
            .position(|(item_key, _)| item_key.bits() == key.bits())
        {
            let item = items.remove(index);
            items.push(item);
        }
        final_items = final_items.cons_cdr();
    }
}

fn collect_menu_bar_items_from_map(
    eval: &super::eval::Context,
    keymap: Value,
    items: &mut Vec<(Value, String)>,
) {
    let menu_bar = Value::symbol("menu-bar");
    let raw = crate::emacs_core::keymap::list_keymap_lookup_one(&keymap, &menu_bar);
    let Some(menu_map) = crate::emacs_core::keymap::maybe_keymap_in_obarray(eval.obarray(), &raw)
    else {
        return;
    };
    crate::emacs_core::keymap::list_keymap_for_each_binding_recursive(
        &menu_map,
        Some(eval.obarray()),
        |key, def| {
            if items.iter().any(|(seen, _)| seen.bits() == key.bits()) {
                return;
            }
            if let Some(label) = menu_bar_label(def) {
                items.push((key, label));
            }
        },
    );
}

fn menu_bar_label(def: Value) -> Option<String> {
    if !def.is_cons() {
        return None;
    }
    let car = def.cons_car();
    let cdr = def.cons_cdr();
    if crate::emacs_core::keymap::KeymapMarker::MenuItem.is_value(car) && cdr.is_cons() {
        return cdr
            .cons_car()
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()));
    }
    car.as_lisp_string()
        .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
}

fn selected_frame_value(eval: &mut super::eval::Context) -> Value {
    let fid =
        super::window_cmds::ensure_selected_frame_id_in_state(&mut eval.frames, &mut eval.buffers);
    Value::make_frame(fid.0)
}

fn maybe_transform_mouse_position(eval: &mut super::eval::Context, value: Value) -> EvalResult {
    let transform = eval
        .obarray
        .symbol_value("mouse-position-function")
        .copied()
        .unwrap_or(Value::NIL);
    if transform.is_truthy() {
        eval.apply(transform, vec![value])
    } else {
        Ok(value)
    }
}

fn pixel_to_char_mouse_position(
    eval: &super::eval::Context,
    frame_id: Option<crate::window::FrameId>,
    x: i64,
    y: i64,
) -> (Value, Value) {
    let Some(frame_id) = frame_id else {
        return (Value::NIL, Value::NIL);
    };
    let Some(frame) = eval.frames.get(frame_id) else {
        return (Value::NIL, Value::NIL);
    };
    let char_width = frame.char_width.max(1.0);
    let char_height = frame.char_height.max(1.0);
    (
        Value::fixnum((x as f32 / char_width).floor() as i64),
        Value::fixnum((y as f32 / char_height).floor() as i64),
    )
}

fn current_mouse_position_value(eval: &mut super::eval::Context, pixel_units: bool) -> EvalResult {
    let selected_frame = selected_frame_value(eval);
    let (frame_value, x, y) = match eval.command_loop.keyboard.mouse_pixel_position() {
        Some(state) => {
            let frame_value = state
                .frame_id
                .map(|frame_id| Value::make_frame(frame_id.0))
                .unwrap_or(Value::NIL);
            let (x, y) = if pixel_units {
                (Value::fixnum(state.x), Value::fixnum(state.y))
            } else {
                pixel_to_char_mouse_position(eval, state.frame_id, state.x, state.y)
            };
            (frame_value, x, y)
        }
        None => (selected_frame, Value::NIL, Value::NIL),
    };
    maybe_transform_mouse_position(eval, Value::cons(frame_value, Value::cons(x, y)))
}

pub(crate) fn builtin_mouse_pixel_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mouse-pixel-position", &args, 0)?;
    current_mouse_position_value(eval, true)
}

pub(crate) fn builtin_mouse_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("mouse-position", &args, 0)?;
    current_mouse_position_value(eval, false)
}

pub(crate) fn builtin_native_comp_available_p(args: Vec<Value>) -> EvalResult {
    expect_args("native-comp-available-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn fontset_alias_alist_startup_value() -> Value {
    fontset::fontset_alias_alist_startup_value()
}

pub(super) fn fontset_list_value() -> Value {
    fontset::fontset_list_value()
}

fn dynamic_or_global_symbol_value_in_state(
    obarray: &Obarray,
    _dynamic: &[OrderedRuntimeBindingMap],
    name: &str,
) -> Option<Value> {
    obarray.symbol_value(name).copied()
}

pub(crate) fn builtin_new_fontset(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("new-fontset", &args, 2)?;
    let obarray = eval.obarray();
    let name = expect_string_lossy(&args[0])?;
    let char_script_table =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "char-script-table");
    let charset_script_alist =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "charset-script-alist");
    let font_encoding_alist =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "font-encoding-alist");
    let registered = fontset::new_fontset(
        &name,
        &args[1],
        char_script_table.as_ref(),
        charset_script_alist.as_ref(),
        font_encoding_alist.as_ref(),
    )?;
    Ok(Value::string(registered))
}

pub(crate) fn builtin_open_font(args: Vec<Value>) -> EvalResult {
    expect_args_range("open-font", &args, 1, 3)?;
    let is_font_entity = match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => {
            let items = args[0].as_vector_data().unwrap().clone();
            items
                .first()
                .is_some_and(|v| v.as_symbol_name() == Some(":font-entity"))
        }
        _ => false,
    };
    if !is_font_entity {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("font-entity"), args[0]],
        ));
    }
    Ok(Value::NIL)
}

/// `(open-dribble-file FILE)` -> nil
///
/// Mirrors GNU `src/keyboard.c:12327-12367`. Opens FILE for
/// writing as the dribble file, where every input event will be
/// logged for debugging. Passing nil closes the current dribble
/// file. Keyboard audit Finding 11 in
/// `drafts/keyboard-command-loop-audit.md`: the previous body
/// validated the argument and silently dropped it.
///
/// The actual writes happen in the keyboard event-ingest path
/// (`KBoard::record_input_event`), which calls
/// `dribble_write_event` whenever the dribble file handle is
/// open.
pub(crate) fn builtin_open_dribble_file(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("open-dribble-file", &args, 1)?;
    if args[0].is_nil() {
        eval.command_loop.keyboard.kboard.close_dribble_file();
        return Ok(Value::NIL);
    }
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(eval.expect_lisp_string(args[0])?);
    if let Err(err) = eval.command_loop.keyboard.kboard.open_dribble_file(&path) {
        return Err(signal(
            LispCondition::FileError,
            vec![
                Value::string("Cannot open dribble file"),
                Value::string(err.to_string()),
            ],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_object_intervals(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("object-intervals", &args, 1)?;
    if args[0].is_string() {
        let len = args[0]
            .as_lisp_string()
            .expect("string value must carry LispString payload")
            .schars();
        let Some(table) =
            crate::emacs_core::value::get_string_text_properties_interval_table_for_value(args[0])
        else {
            return Ok(Value::NIL);
        };
        let intervals = table
            .object_interval_plist_runs_for_char_len(CharLen::new(len))
            .into_iter()
            .map(|run| {
                Value::list(vec![
                    Value::fixnum(run.start().get() as i64),
                    Value::fixnum(run.end().get() as i64),
                    run.plist(),
                ])
            })
            .collect();
        return Ok(Value::list(intervals));
    }

    if args[0].is_buffer() {
        let id = args[0].as_buffer_id().expect("buffer value has id");
        let Some(buf) = eval.buffers.get_any(id) else {
            return Ok(Value::NIL);
        };
        let intervals = buf
            .text_props_object_interval_runs()
            .into_iter()
            .map(|run| {
                let mut plist_values = Vec::with_capacity(run.properties().len() * 2);
                for (key, value) in run.properties() {
                    plist_values.push(*key);
                    plist_values.push(*value);
                }
                Value::list(vec![
                    Value::fixnum(run.start().get() as i64),
                    Value::fixnum(run.end().get() as i64),
                    Value::list(plist_values),
                ])
            })
            .collect();
        return Ok(Value::list(intervals));
    }

    Err(signal(
        LispCondition::WrongTypeArgument,
        vec![Value::symbol("buffer-or-string-p"), args[0]],
    ))
}

pub(crate) fn builtin_optimize_char_table(args: Vec<Value>) -> EvalResult {
    expect_args_range("optimize-char-table", &args, 1, 2)?;
    let test = match args.get(1) {
        None => super::chartable::OptimizeCharTableTest::Equal,
        Some(value) if value.is_nil() || value.is_symbol_named("equal") => {
            super::chartable::OptimizeCharTableTest::Equal
        }
        Some(value) if value.is_symbol_named("eq") => super::chartable::OptimizeCharTableTest::Eq,
        Some(_) => return Ok(Value::NIL),
    };
    super::chartable::optimize_char_table(&args[0], test)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_profiler_cpu_log(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-cpu-log", &args, 0)?;
    Ok(ctx.profiler_cpu_log().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_profiler_cpu_running_p(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-cpu-running-p", &args, 0)?;
    Ok(Value::bool(ctx.profiler_cpu_running()))
}

pub(crate) fn builtin_profiler_cpu_start(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-cpu-start", &args, 1)?;
    if ctx.profiler_cpu_running() {
        return Err(signal(
            "error",
            vec![Value::string("CPU profiler is already running")],
        ));
    }
    let Some(interval) = args[0].as_fixnum().filter(|interval| *interval > 0) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid sampling interval")],
        ));
    };
    // NOTE: the actual start MUST run in every build. Do not wrap it in
    // `debug_assert!`, which is stripped in release builds and would leave the
    // CPU profiler silently disabled in the shipped (release-only) runtime.
    let started = ctx.profiler_cpu_start(interval as u64);
    debug_assert!(
        started,
        "profiler-cpu-start must engage after the running/interval guards above"
    );
    Ok(Value::T)
}

pub(crate) fn builtin_profiler_cpu_stop(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-cpu-stop", &args, 0)?;
    Ok(Value::bool(ctx.profiler_cpu_stop()))
}

pub(crate) fn builtin_profiler_memory_log(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-memory-log", &args, 0)?;
    Ok(ctx.profiler_memory_log().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_profiler_memory_running_p(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-memory-running-p", &args, 0)?;
    Ok(Value::bool(ctx.profiler_memory_running()))
}

pub(crate) fn builtin_profiler_memory_start(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-memory-start", &args, 0)?;
    if !ctx.profiler_memory_start() {
        Err(signal(
            "error",
            vec![Value::string("Memory profiler is already running")],
        ))
    } else {
        Ok(Value::T)
    }
}

pub(crate) fn builtin_profiler_memory_stop(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("profiler-memory-stop", &args, 0)?;
    Ok(Value::bool(ctx.profiler_memory_stop()))
}

pub(crate) fn builtin_pdumper_stats(args: Vec<Value>) -> EvalResult {
    expect_args("pdumper-stats", &args, 0)?;
    Ok(crate::emacs_core::pdump::runtime::pdumper_stats_value().unwrap_or(Value::NIL))
}

pub(crate) fn builtin_position_symbol(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("position-symbol", &args, 2)?;
    let sym = if args[0].is_symbol() {
        args[0]
    } else if args[0].is_symbol_with_pos() {
        args[0].as_symbol_with_pos_sym().unwrap()
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![
                Value::list(vec![
                    Value::symbol("symbolp"),
                    Value::symbol("symbol-with-pos-p"),
                ]),
                args[0],
            ],
        ));
    };
    let pos_val = if let Some(n) = args[1].as_fixnum() {
        Value::fixnum(n)
    } else if let Some(p) = args[1].as_symbol_with_pos_pos() {
        Value::fixnum(p)
    } else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("fixnum-or-symbol-with-pos-p"), args[1]],
        ));
    };
    Ok(ctx.tagged_heap.alloc_symbol_with_pos(sym, pos_val))
}

pub(crate) fn builtin_record(args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("record"), Value::fixnum(0)],
        ));
    }
    Ok(Value::make_record(args))
}

pub(crate) fn builtin_recordp_1(_eval: &mut super::eval::Context, arg: Value) -> EvalResult {
    Ok(Value::bool_val(arg.is_record()))
}

pub(crate) fn builtin_query_fontset(args: Vec<Value>) -> EvalResult {
    expect_args_range("query-fontset", &args, 1, 2)?;
    let pattern = expect_string_lossy(&args[0])?;
    if pattern.is_empty() {
        return Ok(Value::NIL);
    }
    let regexpp = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(fontset::query_fontset_registry(&pattern, regexpp).map_or(Value::NIL, Value::string))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_recent_auto_save_p(args: Vec<Value>) -> EvalResult {
    expect_args("recent-auto-save-p", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_reconsider_frame_fonts(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("reconsider-frame-fonts", &args, 1)?;
    let frame_id = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    if eval
        .frames
        .get(frame_id)
        .and_then(|frame| frame.effective_window_system())
        .is_none()
    {
        return Err(signal(
            "error",
            vec![Value::string("Window system frame should be used")],
        ));
    }
    crate::emacs_core::font::seed_live_frame_default_face_from_font_parameter(eval, frame_id);
    Ok(Value::NIL)
}

pub(crate) fn builtin_redirect_debugging_output(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("redirect-debugging-output", &args, 1, 2)?;
    if args[0].is_nil() {
        eval.debugging_output_file = None;
        return Ok(Value::NIL);
    }
    let expanded =
        crate::emacs_core::fileio::builtin_expand_file_name(eval, vec![args[0], Value::NIL])?;
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(expect_lisp_string(&expanded)?);
    let append = args.get(1).is_some_and(|value| value.is_truthy());
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .open(&path)
        .map_err(|err| {
            signal(
                LispCondition::FileError,
                vec![Value::string(err.to_string())],
            )
        })?;
    eval.debugging_output_file = Some(file);
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_redirect_frame_focus(args: Vec<Value>) -> EvalResult {
    expect_args_range("redirect-frame-focus", &args, 1, 2)?;
    if !args[0].is_nil() && !args[0].is_frame() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("framep"), args[0]],
        ));
    }
    if let Some(focus_frame) = args.get(1)
        && !focus_frame.is_nil()
        && !focus_frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *focus_frame],
        ));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_remove_pos_from_symbol(args: Vec<Value>) -> EvalResult {
    expect_args("remove-pos-from-symbol", &args, 1)?;
    if args[0].is_symbol_with_pos() {
        Ok(args[0].as_symbol_with_pos_sym().unwrap())
    } else {
        Ok(args[0])
    }
}

pub(crate) fn builtin_resize_mini_window_internal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("resize-mini-window-internal", &args, 1)?;
    let wid = args[0].as_window_id().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("window-live-p"), args[0]],
        )
    })?;
    let window_id = crate::window::WindowId(wid);
    let fid = eval
        .frames
        .find_window_frame_id(window_id)
        .ok_or_else(|| signal("error", vec![Value::string("Window not found")]))?;
    let frame = eval
        .frames
        .get_mut(fid)
        .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
    if frame.minibuffer_window != Some(window_id) {
        return Err(signal(
            "error",
            vec![Value::string("Not a valid minibuffer window")],
        ));
    }

    let root_height = frame.root_window.bounds().height.max(0.0) as i64;
    let root_new = frame
        .root_window
        .new_pixel()
        .ok_or_else(|| signal("error", vec![Value::string("Cannot resize mini window")]))?;
    let (mini_height, mini_new) = {
        let mini = frame.minibuffer_leaf.as_ref().ok_or_else(|| {
            signal(
                "error",
                vec![Value::string("Cannot resize a minibuffer-only frame")],
            )
        })?;
        (
            mini.bounds().height.max(0.0) as i64,
            mini.new_pixel()
                .ok_or_else(|| signal("error", vec![Value::string("Cannot resize mini window")]))?,
        )
    };

    let old_total = root_height.saturating_add(mini_height);
    let new_total = root_new.saturating_add(mini_new);
    if !crate::window::window_resize_check(&frame.root_window, false)
        || mini_new <= 0
        || old_total != new_total
    {
        return Err(signal(
            "error",
            vec![Value::string("Cannot resize mini window")],
        ));
    }

    let char_width = frame.char_width;
    let char_height = frame.char_height;
    crate::window::window_resize_apply(&mut frame.root_window, false, char_width, char_height);
    let mini = frame
        .minibuffer_leaf
        .as_mut()
        .expect("minibuffer leaf validated before resize mutation");
    let bounds = *mini.bounds();
    mini.set_bounds(crate::window::Rect::new(
        bounds.x,
        bounds.y,
        bounds.width,
        mini_new as f32,
    ));
    mini.set_new_pixel(None);
    frame.recalculate_minibuffer_bounds();
    Ok(Value::T)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_restore_buffer_modified_p(args: Vec<Value>) -> EvalResult {
    expect_args("restore-buffer-modified-p", &args, 1)?;
    Ok(args[0])
}

pub(crate) fn builtin_set_this_command_keys(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set--this-command-keys", &args, 1)?;
    // Same shape as `getenv-internal`: the setter takes `&mut Context`, so the
    // borrow would span it. One key sequence per command at most — copy the
    // bytes out (DIVERGENCES.md 163).
    let keys = eval.expect_lisp_string(args[0])?.clone();
    eval.set_this_command_keys_from_string(&keys)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_set_buffer_auto_saved(args: Vec<Value>) -> EvalResult {
    expect_args("set-buffer-auto-saved", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_set_charset_plist(args: Vec<Value>) -> EvalResult {
    expect_args("set-charset-plist", &args, 2)?;
    let name = match args[0].kind() {
        ValueKind::Symbol(id) => id,
        _other => {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("charsetp"), args[0]],
            ));
        }
    };
    // Parse the plist argument into (key, value) pairs and store it.
    let mut plist_pairs = Vec::new();
    if let Some(items) = list_to_vec(&args[1]) {
        let mut i = 0;
        while i + 1 < items.len() {
            if let Some(key) = items[i].as_symbol_id() {
                plist_pairs.push((key, items[i + 1]));
            }
            i += 2;
        }
    }
    super::charset::set_charset_plist_registry(name, plist_pairs);
    Ok(args[1])
}

pub(crate) fn builtin_set_fontset_font(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-fontset-font", &args, 3, 5)?;
    let obarray = eval.obarray();
    let char_script_table =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "char-script-table");
    let charset_script_alist =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "charset-script-alist");
    let font_encoding_alist =
        dynamic_or_global_symbol_value_in_state(obarray, &[], "font-encoding-alist");
    fontset::set_fontset_font(
        &args[0],
        &args[1],
        &args[2],
        args.get(4),
        char_script_table.as_ref(),
        charset_script_alist.as_ref(),
        font_encoding_alist.as_ref(),
    )
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_set_frame_window_state_change(args: Vec<Value>) -> EvalResult {
    expect_args_range("set-frame-window-state-change", &args, 0, 2)?;
    if let Some(frame) = args.first()
        && !frame.is_nil()
        && !frame.is_frame()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), *frame],
        ));
    }
    Ok(Value::NIL)
}

fn is_known_fringe_bitmap(name: &str) -> bool {
    matches!(
        name,
        "empty-line"
            | "horizontal-bar"
            | "vertical-bar"
            | "hollow-square"
            | "filled-square"
            | "hollow-rectangle"
            | "filled-rectangle"
            | "right-bracket"
            | "left-bracket"
            | "bottom-right-angle"
            | "bottom-left-angle"
            | "top-right-angle"
            | "top-left-angle"
            | "right-triangle"
            | "left-triangle"
            | "large-circle"
            | "right-curly-arrow"
            | "left-curly-arrow"
            | "down-arrow"
            | "up-arrow"
            | "right-arrow"
            | "left-arrow"
            | "exclamation-mark"
            | "question-mark"
    )
}

pub(crate) fn builtin_set_fringe_bitmap_face(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-fringe-bitmap-face", &args, 1, 2)?;
    let bitmap = args[0].as_symbol_name();
    let has_fringe_property = symbol_property_get(ctx, args[0], Value::symbol("fringe"))?
        .1
        .is_some_and(|v| !v.is_nil());
    if !bitmap.is_some_and(is_known_fringe_bitmap) && !has_fringe_property {
        return Err(signal(
            "error",
            vec![Value::string("Undefined fringe bitmap")],
        ));
    }
    // Store the face override (by name, GC-safe) on the user-bitmap registry so
    // the display pipeline applies it over the spec's FACE. GNU records the FACE
    // symbol in `fringe_faces[n]`; we keep just the resolvable name.
    let symbols_with_pos_enabled = ctx.symbols_with_pos_enabled;
    if let Some(sym) = symbol_id_checked(&args[0], symbols_with_pos_enabled) {
        let face_name = args
            .get(1)
            .filter(|face| !face.is_nil())
            .and_then(|face| face.as_symbol_name())
            .map(|name| name.to_string());
        ctx.fringe_bitmaps.set_face(sym, face_name);
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_set_minibuffer_window(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-minibuffer-window", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::Window)
            if eval.frames.is_minibuffer_window_id(crate::window::WindowId(
                args[0].as_window_id().unwrap(),
            )) =>
        {
            Ok(Value::NIL)
        }
        ValueKind::Veclike(VecLikeType::Window) => Err(signal(
            "error",
            vec![Value::string("Window is not a minibuffer window")],
        )),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("windowp"), args[0]],
        )),
    }
}

pub(crate) fn builtin_set_mouse_pixel_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-mouse-pixel-position", &args, 3)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let x = expect_int(&args[1])?;
    let y = expect_int(&args[2])?;
    eval.command_loop
        .keyboard
        .set_mouse_pixel_position(Some(fid), x, y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_set_mouse_position(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-mouse-position", &args, 3)?;
    let fid = super::window_cmds::resolve_frame_id_in_state(
        &mut eval.frames,
        &mut eval.buffers,
        Some(&args[0]),
        "frame-live-p",
    )?;
    let x = expect_int(&args[1])?;
    let y = expect_int(&args[2])?;
    let Some(frame) = eval.frames.get(fid) else {
        return Err(signal("error", vec![Value::string("Frame not found")]));
    };
    let char_width = frame.char_width.max(1.0).round() as i64;
    let char_height = frame.char_height.max(1.0).round() as i64;
    let pixel_x = x.saturating_mul(char_width).saturating_add(char_width / 2);
    let pixel_y = y
        .saturating_mul(char_height)
        .saturating_add(char_height / 2);
    eval.command_loop
        .keyboard
        .set_mouse_pixel_position(Some(fid), pixel_x, pixel_y);
    Ok(Value::NIL)
}

pub(crate) fn builtin_set_window_new_normal(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-window-new-normal", &args, 1, 2)?;
    expect_window_valid_or_nil(&args[0])?;
    Ok(super::stubs::set_window_new_normal_value(
        eval,
        &args[0],
        args.get(1).cloned().unwrap_or(Value::NIL),
    ))
}

pub(crate) fn builtin_set_window_new_pixel(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-window-new-pixel", &args, 2, 3)?;
    expect_window_valid_or_nil(&args[0])?;
    let size = expect_int(&args[1])?;
    Ok(super::stubs::set_window_new_pixel_value(
        eval,
        &args[0],
        size,
        args.get(2).is_some_and(|v| v.is_truthy()),
    ))
}

pub(crate) fn builtin_set_window_new_total(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("set-window-new-total", &args, 2, 3)?;
    expect_window_valid_or_nil(&args[0])?;
    let size = expect_fixnum(&args[1])?;
    Ok(super::stubs::set_window_new_total_value(
        eval,
        &args[0],
        size,
        args.get(2).is_some_and(|v| v.is_truthy()),
    ))
}

pub(crate) fn builtin_sort_charsets(args: Vec<Value>) -> EvalResult {
    expect_args("sort-charsets", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_string_distance(args: Vec<Value>) -> EvalResult {
    expect_args_range("string-distance", &args, 2, 3)?;
    let s1 = args[0].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let s2 = args[1].as_lisp_string().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[1]],
        )
    })?;
    let bytecomp = args.get(2).is_some_and(|v| v.is_truthy());
    let use_byte_compare = bytecomp || (!s1.is_multibyte() && !s2.is_multibyte());

    if use_byte_compare {
        // Byte-level Levenshtein distance
        let b1 = s1.as_bytes();
        let b2 = s2.as_bytes();
        let dist = levenshtein_distance_bytes(b1, b2);
        Ok(Value::fixnum(dist as i64))
    } else {
        // Character-level Levenshtein distance
        let c1 = super::lisp_string_char_codes(s1);
        let c2 = super::lisp_string_char_codes(s2);
        let dist = levenshtein_distance_codes(&c1, &c2);
        Ok(Value::fixnum(dist as i64))
    }
}

fn levenshtein_distance_codes(a: &[u32], b: &[u32]) -> usize {
    let m = a.len();
    let n = b.len();
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];
    for (j, value) in prev.iter_mut().enumerate() {
        *value = j;
    }
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

fn levenshtein_distance_bytes(a: &[u8], b: &[u8]) -> usize {
    let m = a.len();
    let n = b.len();
    let mut prev = vec![0usize; n + 1];
    let mut curr = vec![0usize; n + 1];
    for (j, value) in prev.iter_mut().enumerate() {
        *value = j;
    }
    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

pub(crate) fn builtin_subr_native_lambda_list(args: Vec<Value>) -> EvalResult {
    expect_args("subr-native-lambda-list", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_subr_type(args: Vec<Value>) -> EvalResult {
    expect_args("subr-type", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_tool_bar_get_system_style(args: Vec<Value>) -> EvalResult {
    expect_args("tool-bar-get-system-style", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_tool_bar_pixel_width(args: Vec<Value>) -> EvalResult {
    expect_args_range("tool-bar-pixel-width", &args, 0, 1)?;
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_transpose_regions(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("transpose-regions", &args, 4, 5)?;
    let current_id = eval
        .buffers
        .current_buffer_id()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let (mut first, mut second) = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let point_min = buf.point_min_lisp_char_pos().as_i64();
        let point_max = buf.point_max_lisp_char_pos().as_i64();
        let raw_start1 = expect_integer_or_marker_in_buffers(&eval.buffers, &args[0])?;
        let raw_end1 = expect_integer_or_marker_in_buffers(&eval.buffers, &args[1])?;
        let raw_start2 = expect_integer_or_marker_in_buffers(&eval.buffers, &args[2])?;
        let raw_end2 = expect_integer_or_marker_in_buffers(&eval.buffers, &args[3])?;
        for (start, end, start_arg, end_arg) in [
            (raw_start1, raw_end1, args[0], args[1]),
            (raw_start2, raw_end2, args[2], args[3]),
        ] {
            if start < point_min || start > point_max || end < point_min || end > point_max {
                return Err(signal(
                    LispCondition::ArgsOutOfRange,
                    vec![Value::make_buffer(buf.id), start_arg, end_arg],
                ));
            }
        }
        (
            CharRange::new(
                LispCharPos1::new(raw_start1.min(raw_end1)).to_char_pos(),
                LispCharPos1::new(raw_start1.max(raw_end1)).to_char_pos(),
            ),
            CharRange::new(
                LispCharPos1::new(raw_start2.min(raw_end2)).to_char_pos(),
                LispCharPos1::new(raw_start2.max(raw_end2)).to_char_pos(),
            ),
        )
    };

    if second.start().get() < first.end().get() {
        std::mem::swap(&mut first, &mut second);
    }
    if second.start().get() < first.end().get() {
        return Err(signal(
            "error",
            vec![Value::string("Transposed regions overlap")],
        ));
    }
    if (first.is_empty() || second.is_empty()) && first.end().get() == second.start().get() {
        return Ok(Value::NIL);
    }

    let transposition = {
        let buf = eval
            .buffers
            .get(current_id)
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.text_transposition_for_char_ranges(first, second)
    };
    let changed_byte_span = transposition.byte_span();
    let changed_start_byte = changed_byte_span.start().get();
    let changed_end_byte = changed_byte_span.end().get();

    let read_only = eval.buffers.get(current_id).is_some_and(|buf| {
        crate::emacs_core::editfns::buffer_read_only_active_in_state(&eval.obarray, &[], buf)
    });
    if read_only {
        return Err(signal(
            LispCondition::BufferReadOnly,
            vec![Value::make_buffer(current_id)],
        ));
    }
    crate::emacs_core::textprop::verify_text_read_only_in_state(
        &eval.obarray,
        &eval.buffers,
        current_id,
        changed_start_byte,
        changed_end_byte,
    )?;

    let change = crate::emacs_core::editfns::text_change_for_unchanged_extent_in_manager(
        &eval.buffers,
        current_id,
        changed_byte_span,
    )?;
    crate::emacs_core::editfns::signal_before_text_change(eval, change)?;
    let leave_markers = args.get(4).is_some_and(|value| !value.is_nil());
    let _ = eval
        .buffers
        .transpose_buffer_regions(current_id, transposition, leave_markers);
    crate::emacs_core::editfns::signal_after_text_change(eval, change)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_tty_output_buffer_size(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty--output-buffer-size", &args, 0, 1)?;
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_tty_set_output_buffer_size(args: Vec<Value>) -> EvalResult {
    expect_args_range("tty--set-output-buffer-size", &args, 1, 2)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_tty_suppress_bold_inverse_default_colors(args: Vec<Value>) -> EvalResult {
    expect_args("tty-suppress-bold-inverse-default-colors", &args, 1)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_unicode_property_table_internal(args: Vec<Value>) -> EvalResult {
    expect_args("unicode-property-table-internal", &args, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_unix_sync(args: Vec<Value>) -> EvalResult {
    expect_args("unix-sync", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_value_lt(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("value<", &args, 2)?;
    match compare_value_lt(eval, &args[0], &args[1])? {
        std::cmp::Ordering::Less => Ok(Value::T),
        _ => Ok(Value::NIL),
    }
}

pub(crate) fn compare_value_lt(
    eval: &crate::emacs_core::eval::Context,
    lhs: &Value,
    rhs: &Value,
) -> Result<std::cmp::Ordering, Flow> {
    compare_value_lt_inner(eval, lhs, rhs, 200)
}

fn compare_value_lt_inner(
    eval: &crate::emacs_core::eval::Context,
    lhs: &Value,
    rhs: &Value,
    maxdepth: i32,
) -> Result<std::cmp::Ordering, Flow> {
    use std::cmp::Ordering;

    if maxdepth < 0 {
        return Err(signal(
            "error",
            vec![Value::string("Maximum depth exceeded in comparison")],
        ));
    }

    if lhs.bits() == rhs.bits() {
        return Ok(Ordering::Equal);
    }

    if let Some(ordering) = compare_number_values_for_value_lt(lhs, rhs) {
        return Ok(ordering);
    }

    if lhs.is_nil() && rhs.is_cons() {
        return Ok(Ordering::Less);
    }

    if lhs.is_cons() && rhs.is_nil() {
        return Ok(Ordering::Greater);
    }

    if let (Some(left), Some(right)) =
        (symbol_name_for_value_lt(lhs), symbol_name_for_value_lt(rhs))
    {
        return Ok(compare_lisp_strings(left.as_ref(), right.as_ref()));
    }

    match (lhs.kind(), rhs.kind()) {
        (ValueKind::String, ValueKind::String) => Ok(compare_lisp_strings(
            lhs.as_lisp_string().expect("string"),
            rhs.as_lisp_string().expect("string"),
        )),
        (ValueKind::Cons, ValueKind::Cons) => {
            let left_car = lhs.cons_car();
            let left_cdr = lhs.cons_cdr();
            let right_car = rhs.cons_car();
            let right_cdr = rhs.cons_cdr();

            let car_cmp = compare_value_lt_inner(eval, &left_car, &right_car, maxdepth - 1)?;
            if car_cmp != Ordering::Equal {
                return Ok(car_cmp);
            }

            match (left_cdr.kind(), right_cdr.kind()) {
                (ValueKind::Nil, ValueKind::Cons) => Ok(Ordering::Less),
                (ValueKind::Cons, ValueKind::Nil) => Ok(Ordering::Greater),
                _ => compare_value_lt_inner(eval, &left_cdr, &right_cdr, maxdepth - 1),
            }
        }
        (ValueKind::Veclike(left_ty), ValueKind::Veclike(right_ty)) => {
            if left_ty != right_ty {
                return Err(signal_value_lt_type_mismatch(lhs, rhs));
            }

            match left_ty {
                VecLikeType::Vector => match (vector_value_lt_kind(lhs), vector_value_lt_kind(rhs))
                {
                    (VectorValueLtKind::PlainVector, VectorValueLtKind::PlainVector) => {
                        compare_value_sequences(eval, lhs, rhs, maxdepth - 1)
                    }
                    (VectorValueLtKind::BoolVector, VectorValueLtKind::BoolVector) => {
                        compare_bool_vectors_for_value_lt(lhs, rhs)
                    }
                    (VectorValueLtKind::CharTable, VectorValueLtKind::CharTable) => {
                        Ok(Ordering::Equal)
                    }
                    _ => Err(signal_value_lt_type_mismatch(lhs, rhs)),
                },
                VecLikeType::Record => compare_value_sequences(eval, lhs, rhs, maxdepth - 1),
                VecLikeType::Marker => compare_markers_for_value_lt(eval, lhs, rhs),
                VecLikeType::Buffer => Ok(compare_buffers_for_value_lt(eval, lhs, rhs)),
                VecLikeType::Bignum => unreachable!("bignums are handled in compare_number_values"),
                _ => Ok(Ordering::Equal),
            }
        }
        (ValueKind::Unbound, ValueKind::Unbound) | (ValueKind::Unknown, ValueKind::Unknown) => {
            Ok(Ordering::Equal)
        }
        _ => Err(signal_value_lt_type_mismatch(lhs, rhs)),
    }
}

fn signal_value_lt_type_mismatch(lhs: &Value, rhs: &Value) -> Flow {
    signal(LispCondition::TypeMismatch, vec![*lhs, *rhs])
}

fn compare_value_sequences(
    eval: &crate::emacs_core::eval::Context,
    lhs: &Value,
    rhs: &Value,
    maxdepth: i32,
) -> Result<std::cmp::Ordering, Flow> {
    use std::cmp::Ordering;

    let left_values = if lhs.is_vector() {
        lhs.as_vector_data().expect("vector")
    } else {
        lhs.as_record_data().expect("record")
    };
    let right_values = if rhs.is_vector() {
        rhs.as_vector_data().expect("vector")
    } else {
        rhs.as_record_data().expect("record")
    };

    for (left, right) in left_values.iter().zip(right_values.iter()) {
        let cmp = compare_value_lt_inner(eval, left, right, maxdepth)?;
        if cmp != Ordering::Equal {
            return Ok(cmp);
        }
    }

    Ok(left_values.len().cmp(&right_values.len()))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum VectorValueLtKind {
    PlainVector,
    BoolVector,
    CharTable,
}

fn vector_value_lt_kind(value: &Value) -> VectorValueLtKind {
    if crate::emacs_core::chartable::is_bool_vector(value) {
        VectorValueLtKind::BoolVector
    } else if crate::emacs_core::chartable::is_char_table(value) {
        VectorValueLtKind::CharTable
    } else {
        VectorValueLtKind::PlainVector
    }
}

fn compare_bool_vectors_for_value_lt(lhs: &Value, rhs: &Value) -> Result<std::cmp::Ordering, Flow> {
    let left_len = crate::emacs_core::chartable::bool_vector_length(lhs)
        .ok_or_else(|| signal_value_lt_type_mismatch(lhs, rhs))? as usize;
    let right_len = crate::emacs_core::chartable::bool_vector_length(rhs)
        .ok_or_else(|| signal_value_lt_type_mismatch(lhs, rhs))? as usize;
    let left_values = lhs.as_vector_data().expect("bool-vector");
    let right_values = rhs.as_vector_data().expect("bool-vector");
    let min_len = left_len.min(right_len);

    for idx in 0..min_len {
        let left_bit = left_values[2 + idx]
            .as_fixnum()
            .map(|n| n != 0)
            .unwrap_or(false);
        let right_bit = right_values[2 + idx]
            .as_fixnum()
            .map(|n| n != 0)
            .unwrap_or(false);
        if left_bit != right_bit {
            return Ok(left_bit.cmp(&right_bit));
        }
    }

    Ok(left_len.cmp(&right_len))
}

fn compare_markers_for_value_lt(
    eval: &crate::emacs_core::eval::Context,
    lhs: &Value,
    rhs: &Value,
) -> Result<std::cmp::Ordering, Flow> {
    use std::cmp::Ordering;

    let left_buffer = marker_live_buffer_for_value_lt(eval, lhs);
    let right_buffer = marker_live_buffer_for_value_lt(eval, rhs);
    match (left_buffer, right_buffer) {
        (None, Some(_)) => return Ok(Ordering::Less),
        (Some(_), None) => return Ok(Ordering::Greater),
        (Some(left), Some(right)) => {
            let buffer_cmp = compare_buffer_ids_for_value_lt(eval, left, right);
            if buffer_cmp != Ordering::Equal {
                return Ok(buffer_cmp);
            }
        }
        (None, None) => return Ok(Ordering::Equal),
    }

    let left_pos =
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&eval.buffers, lhs)?;
    let right_pos =
        crate::emacs_core::marker::marker_position_as_int_with_buffers(&eval.buffers, rhs)?;
    Ok(left_pos.cmp(&right_pos))
}

fn marker_live_buffer_for_value_lt(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Option<crate::buffer::BufferId> {
    let buffer_id = value.as_marker_data()?.buffer?;
    eval.buffers.get(buffer_id)?;
    Some(buffer_id)
}

fn compare_buffers_for_value_lt(
    eval: &crate::emacs_core::eval::Context,
    lhs: &Value,
    rhs: &Value,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    match (lhs.as_buffer_id(), rhs.as_buffer_id()) {
        (Some(left), Some(right)) => compare_buffer_ids_for_value_lt(eval, left, right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_buffer_ids_for_value_lt(
    eval: &crate::emacs_core::eval::Context,
    lhs: crate::buffer::BufferId,
    rhs: crate::buffer::BufferId,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let left_name = eval
        .buffers
        .get(lhs)
        .map(|buffer| buffer.name.as_utf8_str());
    let right_name = eval
        .buffers
        .get(rhs)
        .map(|buffer| buffer.name.as_utf8_str());
    match (left_name, right_name) {
        (Some(left), Some(right)) => left.cmp(&right),
        (None, Some(_)) => Ordering::Less,
        (Some(_), None) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

fn compare_lisp_strings(
    lhs: &crate::heap_types::LispString,
    rhs: &crate::heap_types::LispString,
) -> std::cmp::Ordering {
    use std::cmp::Ordering;

    let mut left_pos = 0;
    let mut right_pos = 0;
    loop {
        match (
            next_lisp_string_char_for_value_lt(lhs, &mut left_pos),
            next_lisp_string_char_for_value_lt(rhs, &mut right_pos),
        ) {
            (Some(left), Some(right)) if left != right => return left.cmp(&right),
            (Some(_), Some(_)) => {}
            (None, Some(_)) => return Ordering::Less,
            (Some(_), None) => return Ordering::Greater,
            (None, None) => return Ordering::Equal,
        }
    }
}

fn next_lisp_string_char_for_value_lt(
    string: &crate::heap_types::LispString,
    pos: &mut usize,
) -> Option<u32> {
    let bytes = string.as_bytes();
    if *pos >= bytes.len() {
        return None;
    }

    if string.is_multibyte() {
        let (cp, len) = crate::emacs_core::emacs_char::string_char(&bytes[*pos..]);
        *pos += len;
        Some(cp)
    } else {
        let byte = bytes[*pos] as u32;
        *pos += 1;
        Some(byte)
    }
}

fn compare_number_values_for_value_lt(lhs: &Value, rhs: &Value) -> Option<std::cmp::Ordering> {
    use std::cmp::Ordering;

    if !lhs.is_number() || !rhs.is_number() {
        return None;
    }

    if lhs.is_float() || rhs.is_float() {
        if let Some(big) = lhs.as_bignum() {
            let right = match rhs.kind() {
                ValueKind::Fixnum(n) => n as f64,
                ValueKind::Float => rhs.xfloat(),
                _ => return None,
            };
            return Some(big.partial_cmp(&right).unwrap_or(Ordering::Equal));
        }
        if let Some(big) = rhs.as_bignum() {
            let left = match lhs.kind() {
                ValueKind::Fixnum(n) => n as f64,
                ValueKind::Float => lhs.xfloat(),
                _ => return None,
            };
            return Some(
                big.partial_cmp(&left)
                    .map(|ordering| ordering.reverse())
                    .unwrap_or(Ordering::Equal),
            );
        }
        let left = match lhs.kind() {
            ValueKind::Fixnum(n) => n as f64,
            ValueKind::Float => lhs.xfloat(),
            _ => return None,
        };
        let right = match rhs.kind() {
            ValueKind::Fixnum(n) => n as f64,
            ValueKind::Float => rhs.xfloat(),
            _ => return None,
        };
        return Some(left.partial_cmp(&right).unwrap_or(Ordering::Equal));
    }

    if !lhs.is_bignum() && !rhs.is_bignum() {
        return match (lhs.kind(), rhs.kind()) {
            (ValueKind::Fixnum(left), ValueKind::Fixnum(right)) => Some(left.cmp(&right)),
            _ => None,
        };
    }

    let left = match lhs.kind() {
        ValueKind::Fixnum(n) => Integer::from(n),
        ValueKind::Veclike(VecLikeType::Bignum) => lhs.as_bignum().expect("bignum").clone(),
        _ => return None,
    };
    let right = match rhs.kind() {
        ValueKind::Fixnum(n) => Integer::from(n),
        ValueKind::Veclike(VecLikeType::Bignum) => rhs.as_bignum().expect("bignum").clone(),
        _ => return None,
    };
    Some(left.cmp(&right))
}

fn symbol_name_for_value_lt(
    value: &Value,
) -> Option<std::borrow::Cow<'static, crate::heap_types::LispString>> {
    match value.kind() {
        ValueKind::Nil => Some(std::borrow::Cow::Borrowed(
            crate::emacs_core::intern::resolve_sym_lisp_string(NIL_SYM_ID),
        )),
        ValueKind::T => Some(std::borrow::Cow::Borrowed(
            crate::emacs_core::intern::resolve_sym_lisp_string(T_SYM_ID),
        )),
        ValueKind::Symbol(id) => Some(std::borrow::Cow::Borrowed(
            crate::emacs_core::intern::resolve_sym_lisp_string(id),
        )),
        _ => None,
    }
}

pub(crate) fn builtin_variable_binding_locus(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("variable-binding-locus", &args, 1)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    let resolved = resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;

    use crate::emacs_core::symbol::SymbolRedirect;
    if let Some(sym) = ctx.obarray.get_by_id(resolved) {
        match sym.redirect() {
            SymbolRedirect::Localized => {
                if let Some(buf) = ctx.buffers.current_buffer() {
                    let target_buf = Value::make_buffer(buf.id);
                    if buf.has_buffer_local_by_sym_id(resolved) {
                        return Ok(target_buf);
                    }
                }
                if let Some(blv) = ctx.obarray.blv(resolved)
                    && blv.found
                    && !blv.where_buf.is_nil()
                {
                    return Ok(blv.where_buf);
                }
            }
            SymbolRedirect::Forwarded => {
                // GNU answers the TERMINAL, not a buffer, for a keyboard
                // variable: `Fmake_local_variable`'s SYMBOL_FORWARDED arm
                // returns `Fframe_terminal (selected_frame)`
                // (`src/data.c:2519-2521`).  It is never nil and never a
                // buffer, so it is checked before the per-buffer question.
                {
                    use crate::emacs_core::forward::LispFwdType;
                    let fwd = unsafe { &*sym.val.fwd };
                    if matches!(fwd.ty, LispFwdType::KboardObj) {
                        return crate::emacs_core::terminal::pure::builtin_frame_terminal(
                            ctx,
                            vec![],
                        );
                    }
                }
                if let Some(buf) = ctx.buffers.current_buffer() {
                    let is_local = {
                        use crate::emacs_core::forward::{LispBufferObjFwd, LispFwdType};
                        let fwd = unsafe { &*sym.val.fwd };
                        if matches!(fwd.ty, LispFwdType::BufferObj) {
                            let buf_fwd = unsafe { &*(fwd as *const _ as *const LispBufferObjFwd) };
                            let Some(slot) =
                                crate::buffer::buffer::BufferSlot::from_u16(buf_fwd.offset)
                            else {
                                return Ok(Value::NIL);
                            };
                            let flags_idx = buf_fwd.local_flags_idx;
                            flags_idx == -1 || buf.slot_local_flag(slot)
                        } else {
                            false
                        }
                    };
                    if is_local {
                        return Ok(Value::make_buffer(buf.id));
                    }
                }
            }
            SymbolRedirect::Plainval | SymbolRedirect::Varalias => {}
        }
    }

    if is_canonical_symbol_id(resolved)
        && let Some(buf) = ctx.buffers.current_buffer()
        && buf.has_buffer_local_by_sym_id(resolved)
    {
        return Ok(Value::make_buffer(buf.id));
    }
    Ok(Value::NIL)
}

pub(crate) fn builtin_x_begin_drag(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-begin-drag", &args, 1, 6)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_x_double_buffered_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-double-buffered-p", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_x_menu_bar_open_internal(args: Vec<Value>) -> EvalResult {
    expect_args_range("x-menu-bar-open-internal", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_xw_display_color_p_ctx(
    ctx: &crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("xw-display-color-p", &args, 0, 1)?;
    if let Some(display) = args.first() {
        super::super::display::expect_display_designator_in_state(&ctx.frames, display)?;
    }
    if super::super::display::display_window_system_symbol_in_state(
        &ctx.frames,
        &ctx.obarray,
        &[],
        args.first(),
    )?
    .is_some_and(super::super::display::gui_window_system_active_value)
    {
        Ok(Value::T)
    } else {
        Ok(Value::NIL)
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_innermost_minibuffer_p(args: Vec<Value>) -> EvalResult {
    expect_args_range("innermost-minibuffer-p", &args, 0, 1)?;
    Ok(Value::NIL)
}

fn stored_interactive_spec_value(spec: Value) -> Value {
    if let Some(items) = spec.as_vector_data()
        && let Some(first) = items.first()
    {
        return *first;
    }
    spec
}

fn interactive_form_from_stored_closure_spec(spec: Value) -> Value {
    let spec = stored_interactive_spec_value(spec);
    Value::list(vec![Value::symbol("interactive"), spec])
}

fn interactive_form_from_quoted_interactive_form(form: &Value) -> Result<Option<Value>, Flow> {
    if !form.is_cons() {
        return Ok(None);
    };
    let pair_car = form.cons_car();
    let pair_cdr = form.cons_cdr();
    if pair_car.as_symbol_name() != Some("interactive") {
        return Ok(None);
    }

    match pair_cdr.kind() {
        ValueKind::Nil => Ok(Some(Value::list(vec![
            Value::symbol("interactive"),
            Value::NIL,
        ]))),
        ValueKind::Cons => {
            let arg_pair_car = pair_cdr.cons_car();
            let _arg_pair_cdr = pair_cdr.cons_cdr();
            Ok(Some(Value::list(vec![
                Value::symbol("interactive"),
                arg_pair_car,
            ])))
        }
        _tail => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), pair_cdr],
        )),
    }
}

fn interactive_form_from_quoted_lambda(value: &Value) -> Result<Option<Value>, Flow> {
    if !value.is_cons() {
        return Ok(None);
    };
    let lambda_pair_car = value.cons_car();
    let lambda_pair_cdr = value.cons_cdr();
    if lambda_pair_car.as_symbol_name() != Some("lambda") {
        return Ok(None);
    }
    if !lambda_pair_cdr.is_cons() {
        return Ok(None);
    };
    let _params = lambda_pair_cdr.cons_car();
    let body = lambda_pair_cdr.cons_cdr();
    let mut cursor = body;
    let mut can_skip_doc = true;

    loop {
        match cursor.kind() {
            ValueKind::Nil => return Ok(None),
            ValueKind::Cons => {
                let pair_car = cursor.cons_car();
                let pair_cdr = cursor.cons_cdr();
                if can_skip_doc && pair_car.is_string() {
                    can_skip_doc = false;
                    cursor = pair_cdr;
                    continue;
                }
                can_skip_doc = false;
                if let Some(interactive) = interactive_form_from_quoted_interactive_form(&pair_car)?
                {
                    return Ok(Some(interactive));
                }
                cursor = pair_cdr;
            }
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("listp"), body],
                ));
            }
        }
    }
}

fn interactive_form_from_bytecode_value(function: Value) -> Option<Value> {
    let probe = function.bytecode_interactive_probe()?;
    if probe.slot_count <= 5 {
        return None;
    }
    let spec_val = stored_interactive_spec_value(probe.interactive.unwrap_or(Value::NIL));
    Some(Value::list(vec![Value::symbol("interactive"), spec_val]))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) enum InteractiveFormPlan {
    Return(Value),
    Autoload { fundef: Value, funname: Value },
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn plan_interactive_form_in_state(
    obarray: &Obarray,
    cmd: Value,
) -> Result<InteractiveFormPlan, Flow> {
    let mut function = cmd;

    if let Some(mut current) = cmd.as_symbol_id() {
        let Some((_, indirect_function)) =
            resolve_indirect_symbol_by_id_in_obarray(obarray, current)
        else {
            return Ok(InteractiveFormPlan::Return(Value::NIL));
        };
        if indirect_function.is_nil() {
            return Ok(InteractiveFormPlan::Return(Value::NIL));
        }

        loop {
            if let Some(property) = obarray
                .get_property_id(
                    current,
                    crate::emacs_core::interactive::InteractiveFormSymbol::id(),
                )
                .filter(|value| !value.is_nil())
            {
                return Ok(InteractiveFormPlan::Return(property));
            }
            let Some(next_function) = symbol_function_cell_in_obarray(obarray, current) else {
                return Ok(InteractiveFormPlan::Return(Value::NIL));
            };
            function = next_function;
            let Some(next_symbol) = function.as_symbol_id() else {
                break;
            };
            current = next_symbol;
        }
    }

    match function.kind() {
        ValueKind::Subr(id) => Ok(InteractiveFormPlan::Return(
            crate::emacs_core::interactive::registered_builtin_interactive_form(id)
                .unwrap_or(Value::NIL),
        )),
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = function.as_subr_id().unwrap();
            Ok(InteractiveFormPlan::Return(
                crate::emacs_core::interactive::registered_builtin_interactive_form(id)
                    .unwrap_or(Value::NIL),
            ))
        }
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
            // GNU data.c:Finteractive_form uses the observable closure slot;
            // it does not reconstruct one by scanning a short closure's body.
            Ok(InteractiveFormPlan::Return(
                function
                    .closure_interactive()
                    .map(interactive_form_from_stored_closure_spec)
                    .unwrap_or(Value::NIL),
            ))
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => Ok(InteractiveFormPlan::Return(
            interactive_form_from_bytecode_value(function).unwrap_or(Value::NIL),
        )),
        ValueKind::Cons if super::autoload::is_autoload_value(&function) => {
            Ok(InteractiveFormPlan::Autoload {
                fundef: function,
                funname: if cmd.as_symbol_id().is_some() {
                    cmd
                } else {
                    Value::NIL
                },
            })
        }
        ValueKind::Cons => Ok(InteractiveFormPlan::Return(
            interactive_form_from_quoted_lambda(&function)?.unwrap_or(Value::NIL),
        )),
        _ => Ok(InteractiveFormPlan::Return(Value::NIL)),
    }
}

/// `(interactive-form CMD)` — matching GNU Emacs data.c:1127-1209 exactly.
///
/// Returns (interactive SPEC) or nil.
/// Handles: symbols (with `interactive-form` property), subrs, closures
/// (including oclosures via genfun dispatch), bytecode, autoloads, and
/// quoted lambda forms.
pub(crate) fn builtin_interactive_form(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("interactive-form", &args, 1)?;
    let cmd = args[0];

    // GNU (data.c:1133): Check indirect-function first for nil.
    if let Some(cmd_id) = cmd.as_symbol_id() {
        match resolve_indirect_symbol_by_id(eval, cmd_id) {
            Some((_, indirect)) if !indirect.is_nil() => {}
            _ => return Ok(Value::NIL),
        }
    }

    // GNU (data.c:1141-1149): Walk the original symbol chain, by symbol
    // identity, checking `interactive-form` on each symbol in the chain.
    let mut fun = cmd;
    let mut genfun = false;
    while let Some(symbol) = fun.as_symbol_id() {
        if let Some(prop) = eval
            .obarray
            .get_property_id(
                symbol,
                crate::emacs_core::interactive::InteractiveFormSymbol::id(),
            )
            .filter(|value| !value.is_nil())
        {
            return Ok(prop);
        }
        match symbol_function_cell_in_obarray(&eval.obarray, symbol) {
            Some(next) => {
                // An autoload's boolean INTERACTIVE flag is only a cheap
                // `commandp` hint.  GNU `interactive-form` always loads it to
                // obtain the real spec, so do not let registry metadata turn
                // that hint into a no-argument spec.
                if !super::autoload::is_autoload_value(&next)
                    && let Some(form) = crate::emacs_core::interactive::registry_interactive_form(
                        &eval.interactive,
                        symbol,
                    )
                {
                    return Ok(form);
                }
                fun = next;
            }
            None => return Ok(Value::NIL),
        }
    }

    // Now `fun` is the resolved function value (not a symbol).
    match fun.kind() {
        // GNU (data.c:1151-1161): SUBRP
        ValueKind::Subr(id) => {
            let result = crate::emacs_core::interactive::registered_builtin_interactive_form(id)
                .unwrap_or(Value::NIL);
            Ok(result)
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            let id = fun.as_subr_id().unwrap();
            let result = crate::emacs_core::interactive::registered_builtin_interactive_form(id)
                .unwrap_or(Value::NIL);
            Ok(result)
        }

        // GNU (data.c:1162-1177): CLOSUREP — check slot 5, then genfun
        ValueKind::Veclike(VecLikeType::Lambda) | ValueKind::Veclike(VecLikeType::Macro) => {
            if let Some(iform_val) = fun.closure_interactive() {
                return Ok(interactive_form_from_stored_closure_spec(iform_val));
            }

            // GNU (data.c:1172-1177): Check for oclosure (non-docstring doc_form)
            if fun.closure_doc_form().flatten().is_some() {
                genfun = true;
            }

            // Fall through to genfun check below
            if genfun {
                // GNU (data.c:1203-1206): Call (oclosure-interactive-form fun)
                // if available (avoid burping during bootstrap).
                // GNU (data.c:1205): "Avoid burping during bootstrap"
                if !eval
                    .obarray
                    .is_function_unbound("oclosure-interactive-form")
                    && let Ok(result) =
                        eval.apply(Value::symbol("oclosure-interactive-form"), vec![fun])
                    && !result.is_nil()
                {
                    return Ok(result);
                }
            }
            Ok(Value::NIL)
        }

        // GNU (data.c:1162-1177 for COMPILED_FUNCTION_P): bytecode.
        // First check the COMPILED_INTERACTIVE slot. If absent, check
        // the COMPILED_DOC_STRING slot — if it isn't a valid docstring
        // (i.e. not nil and not a plain string), set `genfun = true`
        // and fall through to `oclosure-interactive-form`. nadvice's
        // `:around` / `:before` / `:after` wrappers go through this
        // path: they're bytecode objects whose doc_form holds the
        // `advice` oclosure tag, and `oclosure-interactive-form`
        // dispatches to the cl-defmethod in nadvice.el.
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            if let Some(iform) = interactive_form_from_bytecode_value(fun) {
                return Ok(iform);
            }
            // Bytecode has no interactive slot. Check for an oclosure
            // tag in the doc slot.
            if let Some(probe) = fun.bytecode_interactive_probe()
                && probe.doc_form.is_some()
            {
                genfun = true;
            }
            if genfun
                && !eval
                    .obarray
                    .is_function_unbound("oclosure-interactive-form")
                && let Ok(result) =
                    eval.apply(Value::symbol("oclosure-interactive-form"), vec![fun])
                && !result.is_nil()
            {
                return Ok(result);
            }
            Ok(Value::NIL)
        }

        // GNU (data.c:1188-1189): autoload → load then retry
        ValueKind::Cons if super::autoload::is_autoload_value(&fun) => {
            let funname = if cmd.as_symbol_id().is_some() {
                cmd
            } else {
                Value::NIL
            };
            let loaded = super::autoload::builtin_autoload_do_load(eval, vec![fun, funname])?;
            // Retry with the loaded definition
            builtin_interactive_form(eval, vec![loaded])
        }

        // GNU (data.c:1190-1202): lambda list (cons starting with `lambda`)
        ValueKind::Cons => Ok(interactive_form_from_quoted_lambda(&fun)?.unwrap_or(Value::NIL)),

        _ => Ok(Value::NIL),
    }
}

/// `(local-variable-if-set-p VARIABLE &optional BUFFER)` — non-nil
/// if VARIABLE either already has a local binding in BUFFER (the
/// `local-variable-p` test) or is automatically buffer-local
/// (`local_if_set` flag set on its BLV).
///
/// Mirrors GNU `src/data.c:2429-2462`. The two non-trivial cases:
///
/// - SYMBOL_LOCALIZED: if `blv->local_if_set` is set, return `t`.
///   Otherwise fall through to `Flocal_variable_p(variable, buffer)`,
///   which checks for an actual binding in BUFFER. Buffer-local
///   audit Medium 5 in `drafts/buffer-local-variables-audit.md`
///   flagged that the BUFFER argument was previously dropped on
///   the floor here, so a per-buffer check always answered against
///   the current buffer.
///
/// - SYMBOL_FORWARDED with BUFFER_OBJFWD: always return `t` per
///   GNU `data.c:2459`, since BUFFER_OBJFWD slots become local
///   automatically when set.
pub(crate) fn builtin_local_variable_if_set_p(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("local-variable-if-set-p", &args, 1, 2)?;
    let symbol = SymId::from_value(ctx, args[0])?;
    let resolved_id = resolve_variable_alias_id_in_obarray(&ctx.obarray, symbol)?;
    // Mirror the GNU switch on `sym->u.s.redirect` at
    // src/data.c:2445-2461 exactly. PLAINVAL short-circuits to nil
    // *before* the BUFFER argument is validated, which is what
    // makes `(local-variable-if-set-p 'plain-var (some bad buffer))`
    // legitimately return nil rather than signaling
    // wrong-type-argument.
    use crate::emacs_core::symbol::SymbolRedirect;
    let Some(sym) = ctx.obarray.get_by_id(resolved_id) else {
        return Ok(Value::NIL);
    };
    match sym.redirect() {
        SymbolRedirect::Plainval => Ok(Value::NIL),
        SymbolRedirect::Localized => {
            // GNU `if (blv->local_if_set) return Qt;` short circuit.
            // Read the BLV's own local_if_set flag — mirrors GNU
            // `local-variable-if-set-p` SYMBOL_LOCALIZED arm at
            // `data.c:2450-2454`.
            if ctx.obarray.blv(resolved_id).is_some_and(|b| b.local_if_set) {
                return Ok(Value::T);
            }
            // Otherwise defer to local-variable-p with BUFFER
            // forwarded so a per-buffer check answers against the
            // requested buffer rather than the current one.
            crate::emacs_core::custom::builtin_local_variable_p(ctx, args)
        }
        SymbolRedirect::Forwarded => {
            // GNU `local-variable-if-set-p` returns t for BUFFER_OBJFWD
            // symbols because setting them auto-localizes the per-buffer slot
            // (`data.c:2458-2460`). Other forwarder kinds are not
            // buffer-local variables.
            let is_buffer_objfwd = {
                use crate::emacs_core::forward::LispFwdType;
                let fwd = unsafe { &*sym.val.fwd };
                matches!(fwd.ty, LispFwdType::BufferObj)
            };
            Ok(Value::bool_val(is_buffer_objfwd))
        }
        _ => Ok(Value::NIL),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_lock_buffer(args: Vec<Value>) -> EvalResult {
    expect_args_range("lock-buffer", &args, 0, 1)?;
    if let Some(filename) = args.first()
        && !filename.is_nil()
    {
        let _ = expect_lisp_string(filename)?;
    }
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_lock_file(args: Vec<Value>) -> EvalResult {
    expect_args("lock-file", &args, 1)?;
    let _ = expect_lisp_string(&args[0])?;
    Ok(Value::NIL)
}

thread_local! {
    static LOSSAGE_SIZE: RefCell<i64> = const { RefCell::new(300) };
}

pub(super) fn reset_symbols_thread_locals() {
    fontset::reset_fontset_registry();
    LOSSAGE_SIZE.with(|slot| *slot.borrow_mut() = 300);
}

pub(crate) fn builtin_lossage_size(args: Vec<Value>) -> EvalResult {
    expect_args_range("lossage-size", &args, 0, 1)?;

    if let Some(value) = args.first()
        && !value.is_nil()
    {
        let n = match value.kind() {
            ValueKind::Fixnum(n) => n,
            _ => {
                return Err(signal(
                    LispCondition::UserError,
                    vec![Value::string("Value must be a positive integer")],
                ));
            }
        };
        if n < 0 {
            return Err(signal(
                LispCondition::UserError,
                vec![Value::string("Value must be a positive integer")],
            ));
        }
        if n < 100 {
            return Err(signal(
                LispCondition::UserError,
                vec![Value::string("Value must be >= 100")],
            ));
        }
        LOSSAGE_SIZE.with(|slot| *slot.borrow_mut() = n);
    }

    Ok(Value::fixnum(LOSSAGE_SIZE.with(|slot| *slot.borrow())))
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_unlock_buffer(args: Vec<Value>) -> EvalResult {
    expect_args("unlock-buffer", &args, 0)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_unlock_file(args: Vec<Value>) -> EvalResult {
    expect_args("unlock-file", &args, 1)?;
    let _ = expect_lisp_string(&args[0])?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_internal_track_mouse(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal--track-mouse", &args, 1)?;
    let specpdl_count = ctx.specpdl.len();
    ctx.try_specbind_or_unwind_to(specpdl_count, intern("track-mouse"), Value::T)?;
    let result = ctx.apply(args[0], vec![]);
    ctx.unbind_to_with_result(specpdl_count, result)
}

fn internal_complete_buffer_alist(ctx: &super::eval::Context) -> Value {
    let entries: Vec<Value> = ctx
        .buffers
        .buffer_list()
        .into_iter()
        .filter_map(|id| {
            ctx.buffers
                .get(id)
                .map(|buf| Value::cons(buf.name, Value::make_buffer(id)))
        })
        .collect();
    Value::list(entries)
}

fn completion_string_starts_with_space(value: &Value) -> bool {
    value
        .as_lisp_string()
        .and_then(|string| string.as_bytes().first().copied())
        == Some(b' ')
}

fn strip_internal_buffer_completions(completions: Value, total_buffers: usize) -> Value {
    let Some(items) = super::value::list_to_vec(&completions) else {
        return completions;
    };

    let Some(first_non_internal) = items
        .iter()
        .position(|item| !completion_string_starts_with_space(item))
    else {
        return if items.len() == total_buffers {
            completions
        } else {
            Value::NIL
        };
    };

    Value::list(
        items
            .into_iter()
            .skip(first_non_internal)
            .filter(|item| !completion_string_starts_with_space(item))
            .collect(),
    )
}

pub(crate) fn builtin_internal_complete_buffer(
    ctx: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-complete-buffer", &args, 3)?;
    let string = args[0].as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[0]],
        )
    })?;
    let predicate = args[1];
    let flag = args[2];
    let buffer_alist = internal_complete_buffer_alist(ctx);

    if flag.is_nil() {
        return minibuffer::builtin_try_completion(ctx, vec![args[0], buffer_alist, predicate]);
    }

    if flag.is_t() {
        let completions =
            minibuffer::builtin_all_completions(ctx, vec![args[0], buffer_alist, predicate])?;
        if string.schars() > 0 {
            return Ok(completions);
        }
        return Ok(strip_internal_buffer_completions(
            completions,
            ctx.buffers.buffer_list().len(),
        ));
    }

    if eq_value(&flag, &Value::symbol("lambda")) {
        return minibuffer::builtin_test_completion(ctx, vec![args[0], buffer_alist, predicate]);
    }

    if eq_value(&flag, &Value::symbol("metadata")) {
        return Ok(Value::list(vec![
            Value::symbol("metadata"),
            Value::cons(Value::symbol("category"), Value::symbol("buffer")),
            Value::cons(
                Value::symbol("cycle-sort-function"),
                Value::symbol("identity"),
            ),
        ]));
    }

    Ok(Value::NIL)
}

fn syntax_description_word(class: crate::emacs_core::syntax::SyntaxClass) -> &'static str {
    match class {
        crate::emacs_core::syntax::SyntaxClass::Whitespace => "whitespace",
        crate::emacs_core::syntax::SyntaxClass::Punctuation => "punctuation",
        crate::emacs_core::syntax::SyntaxClass::Word => "word",
        crate::emacs_core::syntax::SyntaxClass::Symbol => "symbol",
        crate::emacs_core::syntax::SyntaxClass::Open => "open",
        crate::emacs_core::syntax::SyntaxClass::Close => "close",
        crate::emacs_core::syntax::SyntaxClass::Quote => "prefix",
        crate::emacs_core::syntax::SyntaxClass::StringDelim => "string",
        crate::emacs_core::syntax::SyntaxClass::Math => "math",
        crate::emacs_core::syntax::SyntaxClass::Escape => "escape",
        crate::emacs_core::syntax::SyntaxClass::CharQuote => "charquote",
        crate::emacs_core::syntax::SyntaxClass::Comment => "comment",
        crate::emacs_core::syntax::SyntaxClass::EndComment => "endcomment",
        crate::emacs_core::syntax::SyntaxClass::InheritStd => "inherit",
        crate::emacs_core::syntax::SyntaxClass::CommentFence => "comment fence",
        crate::emacs_core::syntax::SyntaxClass::StringFence => "string fence",
    }
}

fn syntax_descriptor_parts(value: Value) -> Option<(i64, Value)> {
    if !value.is_cons() {
        return None;
    }
    let first = value.cons_car();
    match first.kind() {
        ValueKind::Fixnum(code) => {
            let matching = value.cons_cdr();
            if matching.is_nil()
                || matching
                    .as_fixnum()
                    .and_then(super::character_code_to_rust_char)
                    .is_some()
            {
                Some((code, matching))
            } else {
                None
            }
        }
        _ => None,
    }
}

pub(crate) fn builtin_internal_describe_syntax_value(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-describe-syntax-value", &args, 1)?;
    let syntax = args[0];
    let text = if syntax.is_nil() {
        "default".to_string()
    } else if super::chartable::builtin_char_table_p(vec![syntax])?.is_truthy() {
        "deeper char-table ...".to_string()
    } else if let Some((syntax_code, matching)) = syntax_descriptor_parts(syntax) {
        let Some(class) = crate::emacs_core::syntax::SyntaxClass::from_code(syntax_code) else {
            super::super::buffer::builtin_insert(eval, vec![Value::string("invalid")])?;
            return Ok(syntax);
        };
        let flags = crate::emacs_core::syntax::SyntaxFlags::new(((syntax_code >> 16) & 0xff) as u8);
        let mut out = String::new();
        out.push(class.to_char());
        if matching.is_nil() {
            out.push(' ');
        } else if let Some(ch) = matching.as_fixnum().and_then(|n| char::from_u32(n as u32)) {
            out.push(ch);
        } else {
            super::super::buffer::builtin_insert(eval, vec![Value::string("invalid")])?;
            return Ok(syntax);
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_START_FIRST) {
            out.push('1');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_START_SECOND) {
            out.push('2');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_END_FIRST) {
            out.push('3');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_END_SECOND) {
            out.push('4');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::PREFIX) {
            out.push('p');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_STYLE_B) {
            out.push('b');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_STYLE_C) {
            out.push('c');
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_NESTABLE) {
            out.push('n');
        }
        out.push_str("\twhich means: ");
        out.push_str(syntax_description_word(class));
        if !matching.is_nil()
            && let Some(ch) = matching.as_fixnum().and_then(|n| char::from_u32(n as u32))
        {
            out.push_str(", matches ");
            out.push(ch);
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_START_FIRST) {
            out.push_str(",\n\t  is the first character of a comment-start sequence");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_START_SECOND) {
            out.push_str(",\n\t  is the second character of a comment-start sequence");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_END_FIRST) {
            out.push_str(",\n\t  is the first character of a comment-end sequence");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_END_SECOND) {
            out.push_str(",\n\t  is the second character of a comment-end sequence");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_STYLE_B) {
            out.push_str(" (comment style b)");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_STYLE_C) {
            out.push_str(" (comment style c)");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::COMMENT_NESTABLE) {
            out.push_str(" (nestable)");
        }
        if flags.contains(crate::emacs_core::syntax::SyntaxFlags::PREFIX) {
            let prefix_doc =
                Value::string(",\n\t  is a prefix character for `backward-prefix-chars'");
            let quoted =
                eval.funcall_general(Value::symbol("substitute-command-keys"), vec![prefix_doc])?;
            out.push_str(quoted.as_utf8_str().ok_or_else(|| {
                signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("stringp"), quoted],
                )
            })?);
        }
        out
    } else {
        "invalid".to_string()
    };
    super::super::buffer::builtin_insert(eval, vec![Value::string(text)])?;
    Ok(syntax)
}

pub(crate) fn builtin_internal_event_symbol_parse_modifiers(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-event-symbol-parse-modifiers", &args, 1)?;
    let symbol = SymId::from_value(eval, args[0])?;
    cache_event_symbol_properties_in_obarray(eval.obarray_mut(), symbol)?;
    Ok(eval
        .obarray()
        .get_property_id(symbol, intern("event-symbol-elements"))
        .unwrap_or(Value::NIL))
}

pub(crate) fn cache_event_symbol_value_properties_in_obarray(
    obarray: &mut Obarray,
    value: Value,
) -> EvalResult {
    if let Some(symbol) = symbol_id(&value) {
        cache_event_symbol_properties_in_obarray(obarray, symbol)?;
    } else if value.is_cons()
        && let Some(symbol) = symbol_id(&value.cons_car())
    {
        cache_event_symbol_properties_in_obarray(obarray, symbol)?;
    }
    Ok(Value::NIL)
}

pub(crate) fn cache_event_symbol_properties_in_obarray(
    obarray: &mut Obarray,
    symbol: SymId,
) -> EvalResult {
    let symbol_value = Value::from_sym_id(symbol);
    let name = symbol_value.as_symbol_name().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), symbol_value],
        )
    })?;

    let (modifiers_bits, base) = parse_event_modifiers_gnu(name);
    let base = Value::symbol(base);
    let elements = event_symbol_elements(base, modifiers_bits);
    let mask = Value::list(vec![base, Value::fixnum(modifiers_bits as i64)]);

    // GNU `parse_modifiers' caches both properties before returning
    // `event-symbol-elements' (`src/keyboard.c:7523-7578`).  Lisp
    // `event-basic-type' reads the latter directly.
    obarray.put_property_id(symbol, intern("event-symbol-element-mask"), mask)?;
    obarray.put_property_id(symbol, intern("event-symbol-elements"), elements)?;

    Ok(Value::NIL)
}

pub(crate) fn init_event_symbol_properties(obarray: &mut Obarray) {
    // GNU `syms_of_keyboard' initializes fixed event heads with
    // `(EVENT)' elements.  During loadup, `modify_event_symbol' also
    // fills properties for the preloaded keymap mouse/wheel/function-key
    // symbols.  These properties are what Lisp `event-basic-type' reads.
    for (symbol, kind) in [
        ("mouse-movement", "mouse-movement"),
        ("scroll-bar-movement", "mouse-movement"),
        ("switch-frame", "switch-frame"),
        ("focus-in", "focus-in"),
        ("focus-out", "focus-out"),
        ("move-frame", "move-frame"),
        ("delete-frame", "delete-frame"),
        ("iconify-frame", "iconify-frame"),
        ("make-frame-visible", "make-frame-visible"),
        ("select-window", "switch-frame"),
        ("touchscreen-begin", "touchscreen"),
        ("touchscreen-end", "touchscreen"),
    ] {
        let sym = intern(symbol);
        let value = Value::from_sym_id(sym);
        let _ = obarray.put_property_id(sym, intern("event-kind"), Value::symbol(kind));
        let _ = obarray.put_property_id(
            sym,
            intern("event-symbol-elements"),
            Value::list(vec![value]),
        );
    }

    for name in [
        "backspace",
        "tab",
        "linefeed",
        "clear",
        "return",
        "pause",
        "escape",
        "home",
        "left",
        "up",
        "right",
        "down",
        "prior",
        "next",
        "end",
        "begin",
        "select",
        "print",
        "execute",
        "insert",
        "undo",
        "redo",
        "menu",
        "find",
        "cancel",
        "help",
        "break",
        "backtab",
        "delete",
        "kp-space",
        "kp-tab",
        "kp-enter",
        "kp-f1",
        "kp-f2",
        "kp-f3",
        "kp-f4",
        "kp-home",
        "kp-left",
        "kp-up",
        "kp-right",
        "kp-down",
        "kp-prior",
        "kp-next",
        "kp-end",
        "kp-begin",
        "kp-insert",
        "kp-delete",
        "kp-multiply",
        "kp-add",
        "kp-separator",
        "kp-subtract",
        "kp-decimal",
        "kp-divide",
        "kp-0",
        "kp-1",
        "kp-2",
        "kp-3",
        "kp-4",
        "kp-5",
        "kp-6",
        "kp-7",
        "kp-8",
        "kp-9",
        "kp-equal",
    ] {
        init_standard_event_symbol(obarray, name, Some("function-key"));
    }

    for n in 1..=35 {
        init_standard_event_symbol(obarray, &format!("f{n}"), Some("function-key"));
    }

    for n in 1..=5 {
        init_standard_event_symbol(obarray, &format!("mouse-{n}"), None);
        init_standard_event_symbol(obarray, &format!("down-mouse-{n}"), None);
        init_standard_event_symbol(obarray, &format!("drag-mouse-{n}"), None);
    }

    for name in [
        "M-drag-mouse-1",
        "C-M-drag-mouse-1",
        "S-drag-mouse-1",
        "C-M-mouse-1",
        "C-M-down-mouse-1",
        "S-mouse-1",
        "wheel-up",
        "wheel-down",
        "wheel-left",
        "wheel-right",
        "S-wheel-up",
        "M-wheel-up",
    ] {
        init_standard_event_symbol(obarray, name, None);
    }

    // Modifier-prefixed *function-key* symbols that GNU's dump seeds with an
    // `event-symbol-elements' property because they appear in preloaded
    // keymaps/menus (e.g. `M-RET', `C-<left>', `S-<tab>').  `event-basic-type'
    // reads this property directly and returns nil for a symbol that lacks it,
    // so without these entries `(event-basic-type 'M-return)' wrongly yielded
    // nil instead of `return' (oracle test cx124).  The exact set mirrors the
    // non-keypad, non-dead-key modified function keys present in a fresh `-Q'
    // GNU build; element values are computed name-wise by
    // `cache_event_symbol_properties_in_obarray', matching GNU's
    // `apply_modifiers'.  Keypad (`kp-*') and dead-key variants are display
    // specific and intentionally omitted.
    for name in [
        "C-M-backspace",
        "C-M-delete",
        "C-M-down",
        "C-M-end",
        "C-M-home",
        "C-M-left",
        "C-M-right",
        "C-M-up",
        "C-S-backspace",
        "C-backspace",
        "C-delete",
        "C-down",
        "C-end",
        "C-f10",
        "C-home",
        "C-insert",
        "C-insertchar",
        "C-left",
        "C-next",
        "C-prior",
        "C-right",
        "C-tab",
        "C-up",
        "M-backspace",
        "M-begin",
        "M-clear",
        "M-delete",
        "M-down",
        "M-end",
        "M-escape",
        "M-f10",
        "M-home",
        "M-left",
        "M-linefeed",
        "M-next",
        "M-prior",
        "M-return",
        "M-right",
        "M-tab",
        "M-up",
        "S-delete",
        "S-f10",
        "S-insert",
        "S-insertchar",
        "S-iso-lefttab",
        "S-left",
        "S-right",
        "S-tab",
    ] {
        // GNU leaves `event-kind' nil on these modified function keys in a
        // fresh `-Q' session, so seed only `event-symbol-elements'.
        init_standard_event_symbol(obarray, name, None);
    }
}

fn init_standard_event_symbol(obarray: &mut Obarray, name: &str, kind: Option<&str>) {
    let sym = intern(name);
    let _ = cache_event_symbol_properties_in_obarray(obarray, sym);
    if let Some(kind) = kind {
        let _ = obarray.put_property_id(sym, intern("event-kind"), Value::symbol(kind));
    }
}

/// Parse event symbol modifiers matching GNU keyboard.c logic.
/// Returns (modifier_bitmask, base_event_name).
fn parse_event_modifiers_gnu(name: &str) -> (u32, &str) {
    const UP: u32 = 1 << 0;
    const DOWN: u32 = 1 << 1;
    const DRAG: u32 = 1 << 2;
    const CLICK: u32 = 1 << 3;
    const DOUBLE: u32 = 1 << 4;
    const TRIPLE: u32 = 1 << 5;
    const ALT: u32 = 1 << 22;
    const SUPER: u32 = 1 << 23;
    const HYPER: u32 = 1 << 24;
    const SHIFT: u32 = 1 << 25;
    const CONTROL: u32 = 1 << 26;
    const META: u32 = 1 << 27;

    let mut bits: u32 = 0;
    let mut rest = name;

    loop {
        if let Some(r) = rest.strip_prefix("M-") {
            bits |= META;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("C-") {
            bits |= CONTROL;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("S-") {
            bits |= SHIFT;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("H-") {
            bits |= HYPER;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("s-") {
            bits |= SUPER;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("A-") {
            bits |= ALT;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("down-") {
            bits |= DOWN;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("drag-") {
            bits |= DRAG;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("double-") {
            bits |= DOUBLE;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("triple-") {
            bits |= TRIPLE;
            rest = r;
        } else if let Some(r) = rest.strip_prefix("up-") {
            bits |= UP;
            rest = r;
        } else {
            break;
        }
    }

    if bits & (DOWN | DRAG | DOUBLE | TRIPLE) == 0
        && rest.len() == 7
        && rest.starts_with("mouse-")
        && rest.as_bytes()[6].is_ascii_digit()
    {
        bits |= CLICK;
    }

    if bits & (DOUBLE | TRIPLE) == 0 && rest.len() > 6 && rest.starts_with("wheel-") {
        bits |= CLICK;
    }

    (bits, rest)
}

fn event_symbol_elements(base: Value, modifiers_bits: u32) -> Value {
    let mut out = vec![base];
    for (bit, sym) in [
        (1 << 27, "meta"),
        (1 << 26, "control"),
        (1 << 25, "shift"),
        (1 << 24, "hyper"),
        (1 << 23, "super"),
        (1 << 22, "alt"),
        (1 << 5, "triple"),
        (1 << 4, "double"),
        (1 << 3, "click"),
        (1 << 2, "drag"),
        (1 << 1, "down"),
        (1 << 0, "up"),
    ] {
        if modifiers_bits & bit != 0 {
            out.push(Value::symbol(sym));
        }
    }
    Value::list(out)
}

pub(crate) fn builtin_internal_handle_focus_in(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-handle-focus-in", &args, 1)?;
    if !args[0].is_cons() {
        return Err(signal(
            "error",
            vec![Value::string("invalid focus-in event")],
        ));
    };
    let pair_car = args[0].cons_car();
    let pair_cdr = args[0].cons_cdr();
    if pair_car.as_symbol_name() != Some("focus-in") {
        return Err(signal(
            "error",
            vec![Value::string("invalid focus-in event")],
        ));
    }
    if !pair_cdr.is_cons() {
        return Err(signal(
            "error",
            vec![Value::string("invalid focus-in event")],
        ));
    };
    let frame_value = pair_cdr.cons_car();
    if !frame_value.is_frame() {
        return Err(signal(
            "error",
            vec![Value::string("invalid focus-in event")],
        ));
    };

    let frame_id = crate::window::FrameId(frame_value.as_frame_id().unwrap());
    // GNU `kbd_buffer_get_event` (`src/keyboard.c:4206-4210, 4364-4371`)
    // routes every framed keyboard event through the frame's kboard, records
    // Vlast_event_frame, and synthesizes `switch-frame' when necessary.
    eval.observe_keyboard_input_frame(frame_id);

    Ok(Value::NIL)
}

pub(crate) fn builtin_internal_make_var_non_special(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("internal-make-var-non-special", &args, 1)?;
    let obarray = eval.obarray_mut();
    let symbol = expect_symbol_id(&args[0])?;
    obarray.make_non_special_id(symbol);
    Ok(Value::NIL)
}

fn face_boolean_x_resource_value(
    value: &str,
    signal_on_invalid: bool,
) -> Result<Option<Value>, Flow> {
    if value.eq_ignore_ascii_case("on") || value.eq_ignore_ascii_case("true") {
        Ok(Some(Value::T))
    } else if value.eq_ignore_ascii_case("off") || value.eq_ignore_ascii_case("false") {
        Ok(Some(Value::NIL))
    } else if value.eq_ignore_ascii_case("unspecified") {
        Ok(Some(Value::symbol("unspecified")))
    } else if signal_on_invalid {
        Err(signal(
            "error",
            vec![
                Value::string("Invalid face attribute value from X resource"),
                Value::string(value),
            ],
        ))
    } else {
        Ok(None)
    }
}

pub(crate) fn builtin_internal_set_lisp_face_attribute_from_resource(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range(
        "internal-set-lisp-face-attribute-from-resource",
        &args,
        3,
        4,
    )?;
    if symbol_id(&args[0]).is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    let resource_value = expect_string_lossy(&args[2])?;

    let Some(attr_id) = symbol_id(&args[1]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[1]],
        ));
    };
    let attr_name = resolve_sym(attr_id);

    let converted_value = if resource_value.eq_ignore_ascii_case("unspecified") {
        Value::symbol("unspecified")
    } else {
        match attr_name {
            ":height" => {
                let number =
                    builtin_string_to_number(eval, Value::string(&resource_value), Value::NIL)?;
                match number.kind() {
                    ValueKind::Fixnum(height) if height > 0 => number,
                    _ => {
                        return Err(signal(
                            "error",
                            vec![
                                Value::string("Invalid face height from X resource"),
                                Value::string(resource_value),
                            ],
                        ));
                    }
                }
            }
            ":bold" | ":italic" => {
                face_boolean_x_resource_value(&resource_value, true)?.expect("signal=true")
            }
            ":weight" | ":slant" | ":width" => Value::symbol(&resource_value),
            ":inverse-video" | ":extend" => {
                face_boolean_x_resource_value(&resource_value, true)?.expect("signal=true")
            }
            ":underline" | ":overline" | ":strike-through" => {
                face_boolean_x_resource_value(&resource_value, false)?
                    .unwrap_or_else(|| Value::string(&resource_value))
            }
            ":box" | ":inherit" => {
                let read_result = crate::emacs_core::reader::builtin_read_from_string(
                    eval,
                    vec![Value::string(&resource_value)],
                )?;
                read_result.cons_car()
            }
            _ => Value::string(&resource_value),
        }
    };

    let mut setter_args = vec![args[0], args[1], converted_value];
    if let Some(frame) = args.get(3) {
        setter_args.push(*frame);
    }
    crate::emacs_core::xfaces::builtin_internal_set_lisp_face_attribute(eval, setter_args)
}

pub(crate) fn builtin_internal_stack_stats(args: Vec<Value>) -> EvalResult {
    expect_args("internal-stack-stats", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_internal_subr_documentation(args: Vec<Value>) -> EvalResult {
    expect_args("internal-subr-documentation", &args, 1)?;
    // Mirrors GNU `Fsubr_documentation' (`src/doc.c:383-400'). GNU
    // returns a fixnum byte offset into etc/DOC; neomacs stores docs
    // inline in `subr_docs::GNU_SUBR_DOCS' so we return the literal
    // string. Returns `t' (the GNU sentinel for "invalid function")
    // when the value isn't a subr at all -- the cl-defgeneric
    // `function-documentation' caller checks for `t' and signals
    // `invalid-function'.
    let func = args[0];
    let Some(subr) = super::super::subr_docs::SnarfedSubr::of(func) else {
        return Ok(Value::T);
    };
    Ok(super::super::subr_docs::lookup(&subr)
        .map(Value::string)
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_malloc_info(args: Vec<Value>) -> EvalResult {
    expect_args("malloc-info", &args, 0)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_malloc_trim(args: Vec<Value>) -> EvalResult {
    expect_args_range("malloc-trim", &args, 0, 1)?;
    if let Some(pad) = args.first()
        && !pad.is_nil()
    {
        let _ = expect_wholenump(pad)?;
    }
    Ok(Value::T)
}

pub(crate) fn builtin_memory_info(args: Vec<Value>) -> EvalResult {
    expect_args("memory-info", &args, 0)?;
    let counts = Value::memory_use_counts_snapshot();
    Ok(Value::list(vec![
        Value::fixnum(counts[0]),
        Value::fixnum(counts[1]),
        Value::fixnum(counts[2]),
        Value::fixnum(counts[3]),
    ]))
}

// `memory-limit' is not here.  GNU dropped the C `Fmemory_limit' after
// Emacs 27 (etc/NEWS.27:2965); it is now a `defun' at lisp/subr.el:3574 --
// `(or (cdr (assq 'vsize (process-attributes (emacs-pid)))) 0)', the VIRTUAL
// size reported by `process-attributes', which IS a subr here.  The Rust
// version returned VmHWM, the peak RESIDENT size, from a different file
// (DIVERGENCES.md 152).

pub(crate) fn builtin_module_load(
    ctx: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("module-load", &args, 1)?;
    let path =
        crate::emacs_core::fileio::lisp_file_name_to_path_buf(ctx.expect_lisp_string(args[0])?);
    super::super::dynamic_module::load_module(ctx, path)
}

pub(crate) fn builtin_dump_emacs_portable(
    ctx: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("dump-emacs-portable", &args, 1, 2)?;

    if !ctx.noninteractive() {
        return Err(signal(
            "error",
            vec![Value::string(
                "Dumping Emacs currently works only in batch mode.  If you'd like it to work interactively, please consider contributing a patch to Emacs.",
            )],
        ));
    }
    if ctx.threads.current_thread_id() != 0 {
        return Err(signal(
            "error",
            vec![Value::string(
                "This function can be called only in the main thread",
            )],
        ));
    }
    if ctx.threads.all_thread_ids().into_iter().any(|id| id != 0) {
        return Err(signal(
            "error",
            vec![Value::string(
                "No other Lisp threads can be running when this function is called",
            )],
        ));
    }

    let expanded_lisp = crate::emacs_core::fileio::expand_file_name_lisp(
        expect_lisp_string(&args[0])?,
        crate::emacs_core::fileio::default_directory_lisp_in_state(&ctx.obarray, &[], &ctx.buffers)
            .as_ref(),
    );
    let dump_path_buf = crate::emacs_core::fileio::lisp_file_name_to_path_buf(&expanded_lisp);
    let dump_path = dump_path_buf.as_path();
    let expanded_path = dump_path.display().to_string();
    let is_final_dump = dump_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| matches!(name, "emacs.pdmp" | "neomacs.pdump"));
    if is_final_dump {
        crate::emacs_core::load::normalize_final_dump_runtime_surface(ctx)
            .map_err(crate::emacs_core::error::flow_from_eval_error)?;
    }
    let saved_post_gc_hook = ctx
        .obarray()
        .symbol_value("post-gc-hook")
        .copied()
        .unwrap_or(Value::NIL);
    let saved_command_line_processed = ctx
        .obarray()
        .symbol_value("command-line-processed")
        .copied()
        .unwrap_or(Value::NIL);
    // `process-environment` is a `DEFVAR_LISP` (`src/callproc.c:2144`) that
    // `lisp/eshell/esh-var.el` and `lisp/progmodes/compile.el` localise, so it
    // is in ledger 191's class -- but this read is deliberately buffer-less and
    // the restore below is its matching global write, so the localized arm is
    // the right answer here and is accepted by name rather than by silence.
    // GNU's own `Fdump_emacs_portable` saves nothing of the sort. Ledger 196.
    let saved_process_environment = ctx
        .obarray()
        .value_without_buffer("process-environment")
        .any_arm()
        .unwrap_or(Value::NIL);
    ctx.set_variable("post-gc-hook", Value::NIL);
    ctx.gc_collect_exact();
    // A portable dump cannot represent finalizer objects (the pdump writer
    // arms refuse them; see `pdump/convert.rs`). Pending finalizers are no
    // obstacle: the collection above doomed them and ran their functions
    // (GNU likewise runs pending finalizers before dumping), and each run can
    // drop the last reference to further finalizers, so keep collecting until
    // the live-finalizer registry stops shrinking.
    let mut live_finalizers = ctx.tagged_heap.live_finalizer_count();
    while live_finalizers > 0 {
        ctx.gc_collect_exact();
        let remaining = ctx.tagged_heap.live_finalizer_count();
        if remaining >= live_finalizers {
            break; // fixpoint: the survivors are genuinely reachable
        }
        live_finalizers = remaining;
    }
    debug_assert!(
        !ctx.tagged_heap.has_pending_doomed_finalizers(),
        "gc_collect_exact drains and runs the doomed-finalizer queue"
    );
    if live_finalizers > 0 {
        // Refuse up front with an ordinary elisp error instead of reaching the
        // writer's panic backstop. (GNU pdumper instead dumps a reachable
        // finalizer as an inert object whose function never runs in the child;
        // neomacs deliberately refuses rather than silently reviving a
        // finalizer that will never fire.)
        ctx.set_variable("post-gc-hook", saved_post_gc_hook);
        return Err(signal(
            "error",
            vec![Value::string("Cannot dump Emacs with a finalizer object")],
        ));
    }
    ctx.set_variable("command-line-processed", Value::NIL);
    ctx.set_variable("process-environment", Value::NIL);
    let dump_result = crate::emacs_core::pdump::dump_to_file(ctx, dump_path);
    ctx.set_variable("post-gc-hook", saved_post_gc_hook);
    ctx.set_variable("command-line-processed", saved_command_line_processed);
    ctx.set_variable("process-environment", saved_process_environment);

    dump_result.map_err(|err| {
        signal(
            LispCondition::FileError,
            vec![
                Value::string("dump-emacs-portable"),
                Value::string(expanded_path),
                Value::string(err.to_string()),
            ],
        )
    })?;

    // R2-B1 (resolution B): dump-time AOT preload. Only on the FINAL production
    // dump and only when the producer is explicitly enabled by env (so ordinary
    // dump-emacs-portable calls — tests, plain dumps — pay nothing). Runs IN this
    // process, which owns the patched pdump fingerprint slot + the live obarray,
    // so the emitted `.so` matches the runtime by construction. Failures are
    // logged + swallowed inside the hook (an additive miss → runtime JITs).
    #[cfg(feature = "jit")]
    if is_final_dump {
        let dump_dir = dump_path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        crate::emacs_core::jit::aot::run_dump_time_preload(ctx, dump_dir);
    }

    Ok(Value::NIL)
}

pub(crate) fn builtin_dump_emacs_portable_sort_predicate(args: Vec<Value>) -> EvalResult {
    expect_args("dump-emacs-portable--sort-predicate", &args, 2)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_dump_emacs_portable_sort_predicate_copied(args: Vec<Value>) -> EvalResult {
    expect_args("dump-emacs-portable--sort-predicate-copied", &args, 2)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_decode_coding_region(args: Vec<Value>) -> EvalResult {
    expect_args_range("decode-coding-region", &args, 3, 4)?;
    Ok(Value::NIL)
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_encode_coding_region(args: Vec<Value>) -> EvalResult {
    expect_args_range("encode-coding-region", &args, 3, 4)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_find_operation_coding_system(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("find-operation-coding-system"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    if args.len() < 2 {
        return Err(signal("error", vec![Value::string("Too few arguments")]));
    }

    let operation = args[0];
    let Some(operation_id) = symbol_id_checked(&operation, eval.symbols_with_pos_enabled) else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid first argument")],
        ));
    };
    let Some(target_idx_value) = eval
        .obarray()
        .get_property_id(operation_id, intern("target-idx"))
    else {
        return Err(signal(
            "error",
            vec![Value::string("Invalid first argument")],
        ));
    };
    let target_idx = match expect_fixnum(&target_idx_value) {
        Ok(n) if n >= 0 => n as usize,
        _ => {
            return Err(signal(
                "error",
                vec![Value::string("Invalid first argument")],
            ));
        }
    };

    if args.len() <= target_idx + 1 {
        let op_name = operation.as_symbol_name().unwrap_or("<unknown>");
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Too few arguments for operation ‘{op_name}’"
            ))],
        ));
    }

    let mut target = args[target_idx + 1];
    let valid_target = if target.is_string() {
        true
    } else if operation_id == intern("insert-file-contents")
        && target.is_cons()
        && target.cons_car().is_string()
        && target.cons_cdr().is_buffer()
    {
        target = target.cons_car();
        true
    } else {
        operation_id == intern("open-network-stream") && (target.is_fixnum() || target.is_t())
    };

    if !valid_target {
        let op_name = operation.as_symbol_name().unwrap_or("<unknown>");
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "Invalid argument {} of operation ‘{}’",
                target_idx + 1,
                op_name
            ))],
        ));
    }

    let chain_symbol = if operation_id == intern("insert-file-contents")
        || operation_id == intern("write-region")
    {
        "file-coding-system-alist"
    } else if operation_id == intern("open-network-stream") {
        "network-coding-system-alist"
    } else {
        "process-coding-system-alist"
    };

    let chain = eval.visible_variable_value_or_nil(chain_symbol);
    if chain.is_nil() {
        return Ok(Value::NIL);
    }

    let mut cursor = chain;
    while cursor.is_cons() {
        let elt = cursor.cons_car();
        if elt.is_cons() {
            let key = elt.cons_car();
            let matched = if target.is_string() && key.is_string() {
                let result =
                    super::search::builtin_string_match(eval, vec![key, target, Value::fixnum(0)])?;
                !result.is_nil()
            } else {
                target.is_fixnum() && crate::emacs_core::value::eq_value(&target, &key)
            };

            if matched {
                let val = elt.cons_cdr();
                if val.is_cons() {
                    return Ok(val);
                }
                if !val.is_symbol() {
                    return Ok(Value::NIL);
                }
                if !crate::emacs_core::coding::builtin_coding_system_p(
                    &eval.coding_systems,
                    vec![val],
                )?
                .is_nil()
                {
                    return Ok(Value::cons(val, val));
                }
                if !builtin_fboundp_1(eval, val)?.is_nil() {
                    let callback_args = Value::list(args.clone());
                    let returned = eval.apply(val, vec![callback_args])?;
                    if returned.is_cons() {
                        return Ok(returned);
                    }
                    if returned.is_symbol()
                        && !crate::emacs_core::coding::builtin_coding_system_p(
                            &eval.coding_systems,
                            vec![returned],
                        )?
                        .is_nil()
                    {
                        return Ok(Value::cons(returned, returned));
                    }
                }
                return Ok(Value::NIL);
            }
        }
        cursor = cursor.cons_cdr();
    }

    Ok(Value::NIL)
}

pub(crate) fn builtin_handler_bind_1(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("handler-bind-1"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if args.len().is_multiple_of(2) {
        let message = super::strings::builtin_format_message_slice(
            eval,
            &[Value::string(
                "Trailing CONDITIONS without HANDLER in `handler-bind`",
            )],
        )?;
        return Err(signal("error", vec![message]));
    }

    let scope = eval.save_specpdl_roots();
    for value in &args {
        eval.push_specpdl_root(*value);
    }

    let bodyfun = args[0];
    let handlers: Vec<(Value, Value)> = args[1..]
        .chunks_exact(2)
        .filter_map(|pair| (!pair[0].is_nil()).then_some((pair[0], pair[1])))
        .collect();

    let condition_stack_base = eval.condition_stack_len();
    for (mute_span, (conditions, handler)) in handlers.iter().rev().enumerate() {
        eval.push_condition_frame(super::eval::ConditionFrame::HandlerBind {
            conditions: *conditions,
            handler: *handler,
            mute_span,
        });
    }

    let body_result = match eval.apply(bodyfun, vec![]) {
        Ok(value) => Ok(value),
        Err(Flow::Signal(sig)) => match eval.dispatch_signal_if_needed(sig) {
            Ok(dispatched) => Err(Flow::Signal(dispatched)),
            Err(flow) => Err(flow),
        },
        Err(flow) => Err(flow),
    };
    eval.truncate_condition_stack(condition_stack_base);
    eval.restore_specpdl_roots(scope);
    body_result
}

pub(crate) fn builtin_iso_charset(args: Vec<Value>) -> EvalResult {
    expect_args("iso-charset", &args, 3)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_keymap_get_keyelt(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("keymap--get-keyelt", &args, 2)?;
    // GNU Fkeymap__get_keyelt (keymap.c) delegates to get_keyelt: strip
    // `(menu-item NAME DEFN ...)` wrappers and `(STRING . DEFN)` menu labels
    // down to the definition.  Returning the element unreduced made help.el's
    // describe-map `(eq definition (lookup-key tail (vector event) t))` guard
    // fail for every wrapped menu binding, which silently emptied menu keymap
    // sections such as C-<down-mouse-2> facemenu-menu (ledger entry 61).
    crate::emacs_core::keymap::get_keyelt_runtime(eval, args[0], !args[1].is_nil())
}

pub(crate) fn builtin_keymap_prompt(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("keymap-prompt", &args, 1)?;
    let map = crate::emacs_core::keymap::get_keymap_in_obarray(eval.obarray(), &args[0], false)?;
    Ok(keymap_prompt_scan(eval.obarray(), map))
}

fn keymap_prompt_scan(obarray: &Obarray, map: Value) -> Value {
    keymap_prompt_scan_at_depth(obarray, map, 0)
}

/// The first prompt string found in MAP's spine, descending into composed
/// submaps and the parent. Built on the shared keymap-spine taxonomy so this
/// scan cannot drift from the other spine walkers (see
/// `keymap::for_each_keymap_element`).
fn keymap_prompt_scan_at_depth(obarray: &Obarray, map: Value, depth: usize) -> Value {
    use crate::emacs_core::keymap::KeymapElement;
    if depth > 64 {
        return Value::NIL;
    }
    let mut found = Value::NIL;
    crate::emacs_core::keymap::for_each_keymap_element(&map, Some(obarray), |element| {
        if !found.is_nil() {
            return; // the first prompt in spine order wins
        }
        match element {
            KeymapElement::Prompt(prompt) => found = prompt,
            KeymapElement::Submap(submap) => {
                found = keymap_prompt_scan_at_depth(obarray, submap, depth + 1);
            }
            KeymapElement::Binding { .. } | KeymapElement::IndirectTail(_) => {}
        }
    });
    found
}

pub(crate) fn plan_kill_emacs_request(
    args: &[Value],
) -> Result<super::eval::ShutdownRequest, Flow> {
    expect_args_range("kill-emacs", args, 0, 2)?;
    let exit_code = match args.first().copied().unwrap_or(Value::NIL).kind() {
        ValueKind::Fixnum(n) => n as i32,
        ValueKind::Nil | ValueKind::T => 0,
        _ => 0,
    };
    let restart = args.get(1).is_some_and(|v| v.is_truthy());
    Ok(super::eval::ShutdownRequest { exit_code, restart })
}

pub(crate) fn builtin_kill_emacs(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    let request = plan_kill_emacs_request(&args)?;
    let _ = eval.run_hook_if_bound("kill-emacs-hook");
    eval.request_shutdown(request.exit_code, request.restart);
    Err(Flow::Shutdown(request))
}

pub(crate) fn builtin_lower_frame(args: Vec<Value>) -> EvalResult {
    expect_args_range("lower-frame", &args, 0, 1)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_lread_substitute_object_in_subtree(args: Vec<Value>) -> EvalResult {
    expect_args("lread--substitute-object-in-subtree", &args, 3)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_make_byte_code(args: Vec<Value>) -> EvalResult {
    if args.len() < 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-byte-code"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    make_byte_code_from_slots(&args)
}

pub(crate) fn make_byte_code_from_slots(slots: &[Value]) -> EvalResult {
    if slots.len() < 4 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-byte-code"),
                Value::fixnum(slots.len() as i64),
            ],
        ));
    }

    make_byte_code_from_parts_with_slots(
        &slots[0],
        &slots[1],
        &slots[2],
        &slots[3],
        slots.get(4),
        slots.get(5),
        slots.len(),
        slots.get(6..).unwrap_or(&[]),
    )
}

fn valid_closure_arglist(value: Value) -> bool {
    value.is_fixnum() || value.is_cons() || value.is_nil()
}

fn valid_bytecode_stack_depth(value: Value) -> bool {
    value.as_fixnum().is_some_and(|n| n >= 0)
}

fn check_interpreted_closure_args(params_value: &Value) -> EvalResult {
    if !params_value.is_nil() && !params_value.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), *params_value],
        ));
    }
    Ok(Value::NIL)
}

fn check_interpreted_closure_body(body_value: &Value) -> EvalResult {
    if !body_value.is_cons() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("consp"), *body_value],
        ));
    }
    Ok(Value::NIL)
}

fn make_interpreted_closure_from_gnu_slots(slots: &[Value]) -> EvalResult {
    check_interpreted_closure_args(&slots[0])?;
    check_interpreted_closure_body(&slots[1])?;
    Ok(Value::make_lambda_with_slots(slots.to_vec()))
}

pub(crate) fn closure_from_reader_literal_slots(slots: &[Value]) -> EvalResult {
    if !(3..=6).contains(&slots.len()) || !valid_closure_arglist(slots[0]) {
        return Err(signal(
            LispCondition::InvalidReadSyntax,
            vec![Value::string("Invalid byte-code object")],
        ));
    }

    if slots[1].is_string() {
        if slots.len() <= 3 || !slots[2].is_vector() || !valid_bytecode_stack_depth(slots[3]) {
            return Err(signal(
                LispCondition::InvalidReadSyntax,
                vec![Value::string("Invalid byte-code object")],
            ));
        }
        return make_byte_code_from_slots(slots).map_err(|_| {
            signal(
                LispCondition::InvalidReadSyntax,
                vec![Value::string("Invalid byte-code object")],
            )
        });
    }

    if slots[1].is_cons() && (slots[2].is_cons() || slots[2].is_nil()) {
        return make_interpreted_closure_from_gnu_slots(slots).map_err(|_| {
            signal(
                LispCondition::InvalidReadSyntax,
                vec![Value::string("Invalid byte-code object")],
            )
        });
    }

    Err(signal(
        LispCondition::InvalidReadSyntax,
        vec![Value::string("Invalid byte-code object")],
    ))
}

/// Core logic for constructing a `Value::ByteCode` from GNU-style parts.
/// Used by both `make-byte-code` builtin and `sf_byte_code_literal`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn make_byte_code_from_parts(
    arglist: &Value,
    bytecode_str: &Value,
    constants_vec: &Value,
    maxdepth: &Value,
    docstring: Option<&Value>,
    interactive: Option<&Value>,
) -> EvalResult {
    let mut closure_slot_count = 4;
    if docstring.is_some() {
        closure_slot_count = 5;
    }
    if interactive.is_some() {
        closure_slot_count = 6;
    }

    make_byte_code_from_parts_with_slots(
        arglist,
        bytecode_str,
        constants_vec,
        maxdepth,
        docstring,
        interactive,
        closure_slot_count,
        &[],
    )
}

#[allow(clippy::too_many_arguments)] // preserves the observable GNU closure-slot layout
fn make_byte_code_from_parts_with_slots(
    arglist: &Value,
    bytecode_str: &Value,
    constants_vec: &Value,
    maxdepth: &Value,
    docstring: Option<&Value>,
    interactive: Option<&Value>,
    closure_slot_count: usize,
    extra_slots: &[Value],
) -> EvalResult {
    use crate::emacs_core::bytecode::ByteCodeFunction;
    use crate::emacs_core::bytecode::decode::{
        decode_gnu_bytecode_with_offset_map, parse_arglist_value,
    };

    if !valid_closure_arglist(*arglist)
        || !bytecode_str.is_string()
        || bytecode_str.string_is_multibyte()
        || !constants_vec.is_vector()
        || !valid_bytecode_stack_depth(*maxdepth)
    {
        return Err(signal(
            "error",
            vec![Value::string("Invalid byte-code object")],
        ));
    }

    // 1. Parse arglist
    let params = parse_arglist_value(arglist);

    // 2. Extract raw bytes from bytecode string.
    // Bytecode strings are unibyte and may contain arbitrary byte values
    // (including non-UTF-8), so we must access the raw bytes directly
    // rather than going through as_str() which requires valid UTF-8.
    let raw_bytes = bytecode_str
        .as_lisp_string()
        .expect("validated bytecode string")
        .as_bytes()
        .to_vec();
    let gnu_bytecode_bytes = Some(crate::tagged::header::LispByteVec::owned(raw_bytes.clone()));
    let _ = bytecode_str.with_lisp_string_mut(|string| string.pin_immovable());

    // 3. Extract constants from vector
    let mut constants: Vec<Value> = match constants_vec.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => constants_vec.as_vector_data().unwrap().clone(),
        _ => Vec::new(),
    };

    // 3b. Reify compiled literals embedded in the constants vector.
    // GNU `.elc` constants may contain nested `#[...]` bytecode objects or
    // `#s(hash-table ...)` literals. Convert them into real runtime objects
    // before decoding/executing the bytecode.
    for constant in &mut constants {
        *constant = try_convert_nested_compiled_literal(*constant);
    }

    // 4. Decode GNU bytecodes
    let (ops, gnu_byte_offset_map) =
        decode_gnu_bytecode_with_offset_map(&raw_bytes, &mut constants).map_err(|e| {
            signal(
                "error",
                vec![Value::string(format!("bytecode decode error: {}", e))],
            )
        })?;

    // 5. Extract maxdepth
    let max_stack = match maxdepth.kind() {
        ValueKind::Fixnum(n) => n as u16,
        _ => 16, // fallback
    };

    // 6. Extract closure slot 4.
    // GNU byte-code objects use this slot for either a docstring or an
    // arbitrary documentation form, notably the oclosure type symbol.
    let (doc, doc_form) = match docstring.copied() {
        Some(v) if v.is_string() => (
            Some(
                v.as_lisp_string()
                    .expect("ValueKind::String must carry LispString payload")
                    .clone(),
            ),
            None,
        ),
        Some(v) if !v.is_nil() => (None, Some(v)),
        _ => (None, None),
    };

    // 7. Build ByteCodeFunction
    let mut bc = ByteCodeFunction {
        source_id: crate::emacs_core::bytecode::fresh_bytecode_source_id(),
        ops,
        // The instructions above came straight from the sealing decoder;
        // the stack proof is recomputed below once every shape field
        // (params/lexical/arglist/env/max_stack) is in place.
        ops_sealed: true,
        stack_verified: false,
        constants: constants.into(),
        max_stack,
        params,
        arglist: *arglist,
        // GNU byte-code functions use an integer arg descriptor for lexical
        // bytecode and a list arglist for old dynamically-bound bytecode.
        lexical: matches!(arglist.kind(), ValueKind::Fixnum(_)),
        env: None,
        gnu_byte_offset_map: Some(gnu_byte_offset_map),
        // Preserve original GNU-format bytes so `(aref FN 1)` returns the
        // bytecode string.  Required for `byte-compile-make-closure` which
        // reads the bytes via aref and passes them back to `make-byte-code`
        // when generating closure prototypes.
        gnu_bytecode_bytes,
        docstring: doc,
        doc_form,
        // GNU Emacs (eval.c:2301-2303): "Bytecode objects are interactive if
        // they are long enough to have an element where the interactive spec
        // is stored."  The mere PRESENCE of the slot (even if nil) means the
        // function is interactive.  We mirror this: if the caller provided an
        // interactive argument at all (even nil), store Some(value).
        interactive: interactive.copied(),
        closure_slot_count,
        extra_slots: extra_slots.to_vec(),
        #[cfg(feature = "jit")]
        runtime: Some(crate::emacs_core::jit::Runtime::new()),
        lazy_gnu_code: None,
    };
    bc.defer_gnu_decode();
    if !bc.ops.is_empty() {
        // Eager decode policy kept the instructions resident; prove them now
        // that every shape field is final. (The lazy path proves at decode.)
        bc.refresh_stack_verification();
    }

    Ok(Value::make_bytecode(bc))
}

pub(crate) fn make_interpreted_closure_from_parts(
    params_value: &Value,
    body_value: &Value,
    env_value: &Value,
    docstring: Option<&Value>,
    interactive: Option<&Value>,
) -> EvalResult {
    let _docstring_value = docstring.copied().unwrap_or(Value::NIL);
    let iform = interactive.copied().unwrap_or(Value::NIL);

    check_interpreted_closure_args(params_value)?;
    check_interpreted_closure_body(body_value)?;
    if list_length(&iform).is_none() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("listp"), iform],
        ));
    }

    Ok(make_interpreted_closure_from_parts_unchecked(
        params_value,
        body_value,
        env_value,
        docstring,
        interactive,
    ))
}

pub(crate) fn make_interpreted_closure_from_parts_unchecked(
    params_value: &Value,
    body_value: &Value,
    env_value: &Value,
    docstring: Option<&Value>,
    interactive: Option<&Value>,
) -> Value {
    let docstring_value = docstring.copied().unwrap_or(Value::NIL);
    let iform = interactive.copied().unwrap_or(Value::NIL);

    // GNU Emacs (eval.c:535-555): Fmake_interpreted_closure stores the
    // interactive spec in slot 5 of the closure vector.  The vector length is
    // observable: nil IFORM means slot 5 is absent; `(interactive)' and
    // `(interactive nil)' mean slot 5 is present with nil.
    //
    // GNU processes iform by CDR only.  When modes follow the spec,
    // eval.c constructs `(vector SPEC MODES)`, where MODES is the remaining
    // list as one value rather than a sequence of additional vector slots:
    //   nil                                  → no slot
    //   (interactive)                        → nil slot
    //   (interactive SPEC)                   → SPEC
    //   (interactive SPEC MODE1 MODE2 ...)   → [SPEC (MODE1 MODE2 ...)]
    let interactive_spec = if iform.is_nil() {
        None
    } else {
        let ifcdr = iform.cons_cdr();
        if ifcdr.is_nil() {
            Some(Value::NIL)
        } else if ifcdr.cons_cdr().is_nil() {
            Some(ifcdr.cons_car())
        } else {
            Some(Value::vector(vec![ifcdr.cons_car(), ifcdr.cons_cdr()]))
        }
    };

    let mut slots = vec![*params_value, *body_value, *env_value];
    if interactive_spec.is_some() || !docstring_value.is_nil() {
        slots.push(Value::NIL);
        slots.push(docstring_value);
        if let Some(spec) = interactive_spec {
            slots.push(spec);
        }
    }

    Value::make_lambda_with_slots(slots)
}

/// Reify nested compiled literals embedded in `.elc` constant vectors.
///
/// The value reader already turns `#[...]` into closure objects, just as GNU's
/// reader does.  Hash-table literals still arrive as
/// `(make-hash-table-from-literal '(...))` forms, so this pass reifies those
/// without guessing that ordinary vectors are closures.
pub(crate) fn try_convert_nested_compiled_literal(val: Value) -> Value {
    if let Some(table) = try_convert_hash_table_literal(val) {
        return table;
    }

    val
}

fn try_convert_hash_table_literal(val: Value) -> Option<Value> {
    let form = list_to_vec(&val)?;
    if form.len() != 2 {
        return None;
    }
    let head = form[0].as_symbol_name()?;
    if head != "make-hash-table-from-literal" {
        return None;
    }

    let payload = quote_payload_value(form[1])?;
    let spec = list_to_vec(&payload)?;
    if spec.first()?.as_symbol_name()? != "hash-table" {
        return None;
    }

    let mut test = HashTableTest::Eql;
    let mut test_name: Option<SymId> = None;
    let mut size = 0_i64;
    let mut weakness: Option<HashTableWeakness> = None;
    let mut rehash_size = 1.5_f64;
    let mut rehash_threshold = 0.8125_f64;
    let mut data_value: Option<Value> = None;

    let mut i = 1_usize;
    while i + 1 < spec.len() {
        let key = spec[i].as_symbol_name()?;
        let value = spec[i + 1];
        let Some(key) = HashTableLiteralKey::from_symbol_name(key) else {
            i += 2;
            continue;
        };
        match key {
            HashTableLiteralKey::Size => size = value.as_int()?,
            HashTableLiteralKey::Test => {
                let name = value.as_symbol_name()?;
                test = HashTableTest::from_symbol_name(name)?;
                test_name = Some(intern(name));
            }
            HashTableLiteralKey::Weakness => {
                weakness = match value.as_symbol_name() {
                    Some("nil") | None => None,
                    Some(name) => Some(HashTableWeakness::from_symbol_name(name)?),
                };
            }
            HashTableLiteralKey::RehashSize => {
                rehash_size = value.as_float().unwrap_or(value.as_int()? as f64);
            }
            HashTableLiteralKey::RehashThreshold => {
                rehash_threshold = value.as_float().unwrap_or(value.as_int()? as f64);
            }
            HashTableLiteralKey::Data => data_value = Some(value),
            HashTableLiteralKey::Purecopy => {}
        }
        i += 2;
    }

    let table_value =
        Value::hash_table_with_options(test, size, weakness, rehash_size, rehash_threshold);
    if !table_value.is_hash_table() {
        return None;
    };

    {
        let _ = table_value.with_hash_table_mut(|table| {
            table.test_name = test_name;
            if let Some(data) = data_value.and_then(|value| list_to_vec(&value)) {
                let mut idx = 0_usize;
                while idx + 1 < data.len() {
                    let key_value = try_convert_nested_compiled_literal(data[idx]);
                    let val_value = try_convert_nested_compiled_literal(data[idx + 1]);
                    let key = key_value.to_hash_key(&table.test);
                    table.insert(key, key_value, val_value);
                    idx += 2;
                }
            }
        });
    }

    Some(table_value)
}

fn quote_payload_value(value: Value) -> Option<Value> {
    let items = list_to_vec(&value)?;
    if items.len() != 2 {
        return None;
    }
    match items[0].as_symbol_name() {
        Some("quote") => Some(items[1]),
        _ => None,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_make_char(args: Vec<Value>) -> EvalResult {
    expect_args_range("make-char", &args, 1, 5)?;
    let Some(charset) = args[0].as_symbol_name() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("charsetp"), args[0]],
        ));
    };
    let code1 = match args.get(1) {
        Some(value) => value.as_fixnum().ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("integerp"), *value],
            )
        })?,
        None => default_charset_code(charset).ok_or_else(invalid_make_char_code)?,
    };

    match make_char_code(charset, code1) {
        Some(code) => Ok(Value::fixnum(code as i64)),
        None => Err(invalid_make_char_code()),
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn invalid_make_char_code() -> Flow {
    signal("error", vec![Value::string("Invalid code(s)")])
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn default_charset_code(charset: &str) -> Option<i64> {
    match charset {
        "ascii" => Some(0),
        "latin-iso8859-1" => Some(32),
        "latin-jisx0201" | "katakana-jisx0201" => Some(33),
        _ => None,
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn make_char_code(charset: &str, code1: i64) -> Option<u32> {
    if code1 < 0 {
        return None;
    }
    let code = (code1 as u32) & 0x7f;
    match charset {
        "ascii" => Some(code),
        "latin-iso8859-1" if (32..=127).contains(&code) => Some(0x80 + code),
        "latin-jisx0201" if (33..=126).contains(&code) => Some(match code {
            0x5c => 0x00a5,
            0x7e => 0x203e,
            _ => code,
        }),
        "katakana-jisx0201" if (33..=95).contains(&code) => Some(0xff61 + (code - 33)),
        _ => None,
    }
}

pub(crate) fn builtin_make_closure(args: Vec<Value>) -> EvalResult {
    // (make-closure PROTOTYPE &rest CLOSURE-VARS)
    if args.is_empty() {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("make-closure"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let prototype = &args[0];
    let closure_vars = &args[1..];

    let bc = prototype
        .get_bytecode_data()
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("byte-code-function-p"), args[0]],
            )
        })?
        .clone();

    let mut new_bc = bc;

    if let Some(env_val) = new_bc.env {
        // NeoVM-compiled: replace first N values in env alist
        new_bc.env = Some(replace_env_alist_values(env_val, closure_vars));
    } else {
        // GNU .elc: replace first N entries in constants vector
        if closure_vars.len() > new_bc.constants.len() {
            return Err(signal(
                "error",
                vec![Value::string("Closure vars do not fit in constvec")],
            ));
        }
        for (i, var) in closure_vars.iter().enumerate() {
            new_bc.constants[i] = *var;
        }
        // The instance shares the prototype's tiering state (jit::Runtime);
        // record how many leading constant slots are per-instance so a shared
        // native leaf reads them through the executing callee instead of
        // baking the prototype's `V0..Vn` placeholders — and drop any leaf
        // compiled under a narrower prefix.
        #[cfg(feature = "jit")]
        if let Some(id) = new_bc.jit_runtime().note_patched_prefix(closure_vars.len()) {
            crate::emacs_core::jit::cache::evict_compiled(id);
        }
    }

    Ok(Value::make_bytecode(new_bc))
}

/// Replace the first N values in a cons alist with closure_vars.
/// Walk env alist and closure_vars in parallel. For the first N entries,
/// create new (sym . new_val) cons pairs. Share the remaining tail unchanged.
fn replace_env_alist_values(env: Value, closure_vars: &[Value]) -> Value {
    if closure_vars.is_empty() {
        return env;
    }

    // Collect alist entries
    let entries = match list_to_vec(&env) {
        Some(v) => v,
        None => return env,
    };

    let mut result_entries = Vec::with_capacity(entries.len());
    for (i, entry) in entries.iter().enumerate() {
        if i < closure_vars.len() {
            // Replace value: get the key from (key . old_val), make (key . new_val)
            let key = match entry.kind() {
                ValueKind::Cons => entry.cons_car(),
                _ => *entry, // shouldn't happen in well-formed alist
            };
            result_entries.push(Value::cons(key, closure_vars[i]));
        } else {
            // Share remaining entries unchanged
            result_entries.push(*entry);
        }
    }

    Value::list(result_entries)
}

pub(crate) fn builtin_make_finalizer(
    ctx: &mut super::super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("make-finalizer", &args, 1)?;
    // GNU `Fmake_finalizer` accepts any object as FUNCTION; it is only
    // funcall'd (errors ignored) once the finalizer becomes unreachable.
    Ok(ctx.tagged_heap.alloc_finalizer(args[0]))
}

pub(crate) fn builtin_make_interpreted_closure(args: Vec<Value>) -> EvalResult {
    expect_args_range("make-interpreted-closure", &args, 3, 5)?;
    make_interpreted_closure_from_parts(&args[0], &args[1], &args[2], args.get(3), args.get(4))
}
