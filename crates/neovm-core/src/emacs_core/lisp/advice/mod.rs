//! Variable watchers for the Elisp VM.
//!
//! Provides callbacks invoked when a watched variable changes
//! (like Emacs `add-variable-watcher` / `remove-variable-watcher`).

use crate::emacs_core::error::expect_args;
use rustc_hash::FxHashMap;

use super::intern::SymId;
use super::value::{Value, ValueKind, VecLikeType};
use crate::gc_trace::GcTrace;

// ---------------------------------------------------------------------------
// Variable watcher system
// ---------------------------------------------------------------------------

/// A single variable watcher callback.
#[derive(Clone, Debug)]
pub struct VariableWatcher {
    /// The callback function to invoke on variable change.
    pub callback: Value,
}

/// Registry of variable watchers.
pub struct VariableWatcherList {
    /// Map from resolved variable symbol → list of watcher callbacks.
    /// FxHashMap (not the default SipHash): `has_watchers` is queried on every
    /// variable set/bind/unbind, and SymId keys hash far cheaper under FxHash.
    watchers: FxHashMap<SymId, Vec<VariableWatcher>>,
}

impl VariableWatcherList {
    pub fn new() -> Self {
        Self {
            watchers: FxHashMap::default(),
        }
    }

    /// Add a watcher callback for a variable.
    pub fn add_watcher(&mut self, var_id: SymId, callback: Value) {
        let entry = self.watchers.entry(var_id).or_default();
        // Don't add duplicate watchers.
        let already_exists = entry
            .iter()
            .any(|w| watcher_callback_matches(&w.callback, &callback));
        if !already_exists {
            entry.push(VariableWatcher { callback });
        }
    }

    /// Remove a watcher callback for a variable.
    pub fn remove_watcher(&mut self, var_id: SymId, callback: &Value) {
        if let Some(list) = self.watchers.get_mut(&var_id) {
            list.retain(|w| !watcher_callback_matches(&w.callback, callback));
            if list.is_empty() {
                self.watchers.remove(&var_id);
            }
        }
    }

    /// Remove all watcher callbacks for a variable.
    pub fn clear_watchers(&mut self, var_id: SymId) {
        self.watchers.remove(&var_id);
    }

    /// Check if a variable has any watchers.
    pub fn has_watchers(&self, var_id: SymId) -> bool {
        // Fast path: when nothing has called add-variable-watcher (the common
        // case, especially during loading) the map is empty, so skip hashing the
        // key entirely. `remove_watcher` deletes emptied entries, so an empty map
        // genuinely means "no watchers anywhere". This runs on every variable
        // write via run_variable_watchers_by_id_with_where.
        if self.watchers.is_empty() {
            return false;
        }
        self.watchers
            .get(&var_id)
            .is_some_and(|list| !list.is_empty())
    }

    /// Return registered watcher callbacks for a variable in insertion order.
    pub fn get_watchers(&self, var_id: SymId) -> Vec<Value> {
        self.watchers
            .get(&var_id)
            .map(|list| list.iter().map(|watcher| watcher.callback).collect())
            .unwrap_or_default()
    }

    /// Build a list of (callback, args) pairs to invoke for a variable change.
    ///
    /// Returns a Vec of (callback_value, argument_list) that the evaluator
    /// should call. The caller is responsible for actually invoking them
    /// (to avoid borrow issues with the evaluator).
    ///
    /// Each callback receives: (SYMBOL NEWVAL OPERATION WHERE)
    /// - SYMBOL: the variable name
    /// - NEWVAL: the new value
    /// - OPERATION: one of "set", "let", "unlet", "makunbound", "defvaralias"
    /// - WHERE: location designator (`nil` for global, buffer for buffer-local)
    pub fn notify_watchers(
        &self,
        var_id: SymId,
        new_val: &Value,
        _old_val: &Value,
        operation: &str,
        where_val: &Value,
    ) -> Vec<(Value, Vec<Value>)> {
        let mut calls = Vec::new();
        if let Some(list) = self.watchers.get(&var_id) {
            for watcher in list {
                let args = vec![
                    Value::from_sym_id(var_id),
                    *new_val,
                    Value::symbol(operation),
                    *where_val,
                ];
                calls.push((watcher.callback, args));
            }
        }
        calls
    }

    // pdump accessors
    pub(crate) fn dump_watchers(&self) -> &FxHashMap<SymId, Vec<VariableWatcher>> {
        &self.watchers
    }
    pub(crate) fn from_dump(watchers: FxHashMap<SymId, Vec<VariableWatcher>>) -> Self {
        Self { watchers }
    }
}

fn watcher_callback_matches(registered: &Value, candidate: &Value) -> bool {
    if registered == candidate {
        return true;
    }
    match (registered.kind(), candidate.kind()) {
        (ValueKind::Veclike(VecLikeType::Lambda), ValueKind::Veclike(VecLikeType::Lambda))
        | (ValueKind::Veclike(VecLikeType::Macro), ValueKind::Veclike(VecLikeType::Macro)) => {
            lambda_data_matches(registered, candidate)
        }
        _ => false,
    }
}

fn lambda_data_matches(left: &Value, right: &Value) -> bool {
    let (Some(left_params), Some(right_params)) = (left.closure_params(), right.closure_params())
    else {
        return false;
    };
    left_params.required == right_params.required
        && left_params.optional == right_params.optional
        && left_params.rest == right_params.rest
        && left
            .closure_body_value()
            .zip(right.closure_body_value())
            .is_some_and(|(left_body, right_body)| {
                super::value::equal_value(&left_body, &right_body, 0)
            })
        && lex_envs_equal(
            &left.closure_env().unwrap_or(None),
            &right.closure_env().unwrap_or(None),
        )
        && left.closure_docstring().flatten() == right.closure_docstring().flatten()
}

/// Equality for lexical environments (Option<Value>).
fn lex_envs_equal(a: &Option<super::value::Value>, b: &Option<super::value::Value>) -> bool {
    a == b
}

impl Default for VariableWatcherList {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for VariableWatcherList {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for watcher_list in self.watchers.values() {
            for watcher in watcher_list {
                roots.push(watcher.callback);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Builtin functions (eval-dependent)
// ---------------------------------------------------------------------------

use super::error::EvalResult;

/// `(add-variable-watcher SYMBOL WATCH-FUNCTION)`
///
/// Arrange to call WATCH-FUNCTION when SYMBOL is set.
pub(crate) fn builtin_add_variable_watcher(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("add-variable-watcher", &args, 2)?;

    let symbol = super::builtins::symbols::expect_symbol_id(&args[0])?;
    let resolved =
        super::builtins::symbols::resolve_variable_alias_id_in_obarray(&eval.obarray, symbol)?;
    let callback = args[1];

    eval.watchers.add_watcher(resolved, callback);
    Ok(Value::NIL)
}

/// `(remove-variable-watcher SYMBOL WATCH-FUNCTION)`
///
/// Remove WATCH-FUNCTION from the watchers of SYMBOL.
pub(crate) fn builtin_remove_variable_watcher(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("remove-variable-watcher", &args, 2)?;

    let symbol = super::builtins::symbols::expect_symbol_id(&args[0])?;
    let resolved =
        super::builtins::symbols::resolve_variable_alias_id_in_obarray(&eval.obarray, symbol)?;
    let callback = args[1];

    eval.watchers.remove_watcher(resolved, &callback);
    Ok(Value::NIL)
}

/// `(get-variable-watchers SYMBOL)`
///
/// Return a list of watcher callbacks registered for SYMBOL.
pub(crate) fn builtin_get_variable_watchers(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("get-variable-watchers", &args, 1)?;

    let symbol = super::builtins::symbols::expect_symbol_id(&args[0])?;
    let resolved =
        super::builtins::symbols::resolve_variable_alias_id_in_obarray(&eval.obarray, symbol)?;
    Ok(Value::list(eval.watchers.get_watchers(resolved)))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
