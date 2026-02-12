//! Advice system and variable watchers for the Elisp VM.
//!
//! Provides:
//! - **Advice system**: Before, After, Around, Override, FilterArgs, FilterReturn
//!   advice on functions (like Emacs `advice-add` / `advice-remove`).
//! - **Variable watchers**: Callbacks invoked when a watched variable changes
//!   (like Emacs `add-variable-watcher` / `remove-variable-watcher`).

use std::collections::HashMap;

use super::value::Value;

// ---------------------------------------------------------------------------
// Advice types
// ---------------------------------------------------------------------------

/// The kind of advice to apply around a target function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AdviceType {
    /// Called before the original function; receives same args.
    Before,
    /// Called after the original function; receives same args.
    After,
    /// Wraps the original function; receives the original as first arg.
    Around,
    /// Completely replaces the original function.
    Override,
    /// Filters the argument list before the original function sees it.
    FilterArgs,
    /// Filters the return value after the original function returns.
    FilterReturn,
}

impl AdviceType {
    /// Parse a keyword (e.g. `:before`) into an AdviceType.
    pub fn from_keyword(kw: &str) -> Option<Self> {
        match kw {
            ":before" => Some(Self::Before),
            ":after" => Some(Self::After),
            ":around" => Some(Self::Around),
            ":override" => Some(Self::Override),
            ":filter-args" => Some(Self::FilterArgs),
            ":filter-return" => Some(Self::FilterReturn),
            _ => None,
        }
    }

    /// Ordering key for sorting advice — lower values run first.
    fn sort_key(&self) -> u8 {
        match self {
            Self::FilterArgs => 0,
            Self::Before => 1,
            Self::Around => 2,
            Self::Override => 3,
            Self::After => 4,
            Self::FilterReturn => 5,
        }
    }
}

// ---------------------------------------------------------------------------
// Advice
// ---------------------------------------------------------------------------

/// A single piece of advice attached to a function.
#[derive(Clone, Debug)]
pub struct Advice {
    pub advice_type: AdviceType,
    /// The advice function — a lambda, symbol, or subr.
    pub function: Value,
    /// Optional name for identification / removal by name.
    pub name: Option<String>,
}

// ---------------------------------------------------------------------------
// AdviceManager
// ---------------------------------------------------------------------------

/// Central registry of all advice attached to named functions.
pub struct AdviceManager {
    /// Map from target function name → list of advice.
    advice_map: HashMap<String, Vec<Advice>>,
}

impl AdviceManager {
    pub fn new() -> Self {
        Self {
            advice_map: HashMap::new(),
        }
    }

    /// Add advice to a target function.
    pub fn add_advice(
        &mut self,
        target_fn: &str,
        advice_type: AdviceType,
        advice_fn: Value,
        name: Option<String>,
    ) {
        let entry = self.advice_map.entry(target_fn.to_string()).or_default();

        // If advice with the same name already exists, replace it
        if let Some(ref n) = name {
            if let Some(existing) = entry.iter_mut().find(|a| a.name.as_deref() == Some(n.as_str())) {
                existing.advice_type = advice_type;
                existing.function = advice_fn;
                return;
            }
        }

        entry.push(Advice {
            advice_type,
            function: advice_fn,
            name,
        });
    }

    /// Remove advice from a target function by function name or advice name.
    pub fn remove_advice(&mut self, target_fn: &str, advice_fn_or_name: &str) {
        if let Some(list) = self.advice_map.get_mut(target_fn) {
            list.retain(|a| {
                // Keep if neither the name nor the function symbol matches
                let name_matches = a.name.as_deref() == Some(advice_fn_or_name);
                let fn_matches = matches!(&a.function, Value::Symbol(s) if s == advice_fn_or_name);
                !name_matches && !fn_matches
            });
            if list.is_empty() {
                self.advice_map.remove(target_fn);
            }
        }
    }

    /// Get all advice for a function, sorted by type (filter-args first,
    /// filter-return last).
    pub fn get_advice(&self, target_fn: &str) -> Vec<&Advice> {
        match self.advice_map.get(target_fn) {
            Some(list) => {
                let mut sorted: Vec<&Advice> = list.iter().collect();
                sorted.sort_by_key(|a| a.advice_type.sort_key());
                sorted
            }
            None => Vec::new(),
        }
    }

    /// Check if a function has any advice attached.
    pub fn has_advice(&self, target_fn: &str) -> bool {
        self.advice_map
            .get(target_fn)
            .is_some_and(|list| !list.is_empty())
    }

    /// Check if a specific function or name is advising a target.
    pub fn advice_member_p(&self, target_fn: &str, advice_fn_or_name: &str) -> bool {
        match self.advice_map.get(target_fn) {
            Some(list) => list.iter().any(|a| {
                let name_matches = a.name.as_deref() == Some(advice_fn_or_name);
                let fn_matches = matches!(&a.function, Value::Symbol(s) if s == advice_fn_or_name);
                name_matches || fn_matches
            }),
            None => false,
        }
    }
}

impl Default for AdviceManager {
    fn default() -> Self {
        Self::new()
    }
}

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
    /// Map from variable name → list of watcher callbacks.
    watchers: HashMap<String, Vec<VariableWatcher>>,
}

impl VariableWatcherList {
    pub fn new() -> Self {
        Self {
            watchers: HashMap::new(),
        }
    }

    /// Add a watcher callback for a variable.
    pub fn add_watcher(&mut self, var_name: &str, callback: Value) {
        let entry = self.watchers.entry(var_name.to_string()).or_default();
        // Don't add duplicate watchers (check by symbol name)
        let already_exists = entry.iter().any(|w| {
            match (&w.callback, &callback) {
                (Value::Symbol(a), Value::Symbol(b)) => a == b,
                _ => false,
            }
        });
        if !already_exists {
            entry.push(VariableWatcher { callback });
        }
    }

    /// Remove a watcher callback for a variable, identified by function name.
    pub fn remove_watcher(&mut self, var_name: &str, callback_name: &str) {
        if let Some(list) = self.watchers.get_mut(var_name) {
            list.retain(|w| {
                !matches!(&w.callback, Value::Symbol(s) if s == callback_name)
            });
            if list.is_empty() {
                self.watchers.remove(var_name);
            }
        }
    }

    /// Check if a variable has any watchers.
    pub fn has_watchers(&self, var_name: &str) -> bool {
        self.watchers
            .get(var_name)
            .is_some_and(|list| !list.is_empty())
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
    /// - WHERE: nil for global changes (buffer-local not yet supported)
    pub fn notify_watchers(
        &self,
        var_name: &str,
        new_val: &Value,
        _old_val: &Value,
        operation: &str,
    ) -> Vec<(Value, Vec<Value>)> {
        let mut calls = Vec::new();
        if let Some(list) = self.watchers.get(var_name) {
            for watcher in list {
                let args = vec![
                    Value::symbol(var_name),
                    new_val.clone(),
                    Value::symbol(operation),
                    Value::Nil, // WHERE — nil for global
                ];
                calls.push((watcher.callback.clone(), args));
            }
        }
        calls
    }
}

impl Default for VariableWatcherList {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Builtin functions (eval-dependent)
// ---------------------------------------------------------------------------

use super::error::{signal, EvalResult, Flow};

/// Expect at least N arguments.
fn expect_min_args(name: &str, args: &[Value], min: usize) -> Result<(), Flow> {
    if args.len() < min {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Expect exactly N arguments.
fn expect_args(name: &str, args: &[Value], n: usize) -> Result<(), Flow> {
    if args.len() != n {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

/// Extract a symbol name from a Value.
fn expect_symbol_name(value: &Value) -> Result<String, Flow> {
    match value {
        Value::Symbol(s) => Ok(s.clone()),
        Value::Nil => Ok("nil".to_string()),
        Value::True => Ok("t".to_string()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("symbolp"), other.clone()],
        )),
    }
}

/// `(advice-add SYMBOL WHERE FUNCTION &optional PROPS)`
///
/// WHERE is one of :before, :after, :around, :override, :filter-args, :filter-return.
/// PROPS is an optional plist; currently only :name is recognized.
pub(crate) fn builtin_advice_add(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("advice-add", &args, 3)?;

    let target = expect_symbol_name(&args[0])?;

    let where_kw = match &args[1] {
        Value::Keyword(k) => k.clone(),
        other => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("keywordp"), other.clone()],
            ));
        }
    };

    let advice_type = AdviceType::from_keyword(&where_kw).ok_or_else(|| {
        signal(
            "error",
            vec![Value::string(format!(
                "advice-add: unknown advice type {}",
                where_kw
            ))],
        )
    })?;

    let advice_fn = args[2].clone();

    // Extract optional :name from PROPS plist
    let name = if args.len() > 3 {
        extract_plist_name(&args[3..])
    } else {
        // Use symbol name of advice function as default name
        match &advice_fn {
            Value::Symbol(s) => Some(s.clone()),
            _ => None,
        }
    };

    eval.advice.add_advice(&target, advice_type, advice_fn, name);
    Ok(Value::Nil)
}

/// Extract :name from a plist-style argument list.
fn extract_plist_name(props: &[Value]) -> Option<String> {
    let mut i = 0;
    while i + 1 < props.len() {
        if matches!(&props[i], Value::Keyword(k) if k == ":name") {
            return match &props[i + 1] {
                Value::Symbol(s) => Some(s.clone()),
                Value::Str(s) => Some((**s).clone()),
                _ => None,
            };
        }
        i += 2;
    }
    None
}

/// `(advice-remove SYMBOL FUNCTION)`
///
/// Remove advice identified by FUNCTION (a symbol) or by name from SYMBOL.
pub(crate) fn builtin_advice_remove(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("advice-remove", &args, 2)?;

    let target = expect_symbol_name(&args[0])?;
    let fn_or_name = expect_symbol_name(&args[1])?;

    eval.advice.remove_advice(&target, &fn_or_name);
    Ok(Value::Nil)
}

/// `(advice-member-p FUNCTION SYMBOL)`
///
/// Return t if FUNCTION is advising SYMBOL.
pub(crate) fn builtin_advice_member_p(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("advice-member-p", &args, 2)?;

    let function = expect_symbol_name(&args[0])?;
    let target = expect_symbol_name(&args[1])?;

    Ok(Value::bool(eval.advice.advice_member_p(&target, &function)))
}

/// `(add-variable-watcher SYMBOL WATCH-FUNCTION)`
///
/// Arrange to call WATCH-FUNCTION when SYMBOL is set.
pub(crate) fn builtin_add_variable_watcher(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("add-variable-watcher", &args, 2)?;

    let var_name = expect_symbol_name(&args[0])?;
    let callback = args[1].clone();

    eval.watchers.add_watcher(&var_name, callback);
    Ok(Value::Nil)
}

/// `(remove-variable-watcher SYMBOL WATCH-FUNCTION)`
///
/// Remove WATCH-FUNCTION from the watchers of SYMBOL.
pub(crate) fn builtin_remove_variable_watcher(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("remove-variable-watcher", &args, 2)?;

    let var_name = expect_symbol_name(&args[0])?;
    let callback_name = expect_symbol_name(&args[1])?;

    eval.watchers.remove_watcher(&var_name, &callback_name);
    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // AdviceType tests
    // -----------------------------------------------------------------------

    #[test]
    fn advice_type_from_keyword() {
        assert_eq!(AdviceType::from_keyword(":before"), Some(AdviceType::Before));
        assert_eq!(AdviceType::from_keyword(":after"), Some(AdviceType::After));
        assert_eq!(AdviceType::from_keyword(":around"), Some(AdviceType::Around));
        assert_eq!(AdviceType::from_keyword(":override"), Some(AdviceType::Override));
        assert_eq!(AdviceType::from_keyword(":filter-args"), Some(AdviceType::FilterArgs));
        assert_eq!(AdviceType::from_keyword(":filter-return"), Some(AdviceType::FilterReturn));
        assert_eq!(AdviceType::from_keyword(":bogus"), None);
    }

    // -----------------------------------------------------------------------
    // AdviceManager tests
    // -----------------------------------------------------------------------

    #[test]
    fn add_and_get_advice() {
        let mut mgr = AdviceManager::new();
        assert!(!mgr.has_advice("my-fn"));

        mgr.add_advice(
            "my-fn",
            AdviceType::Before,
            Value::symbol("my-before-fn"),
            Some("my-before".to_string()),
        );

        assert!(mgr.has_advice("my-fn"));
        let advice_list = mgr.get_advice("my-fn");
        assert_eq!(advice_list.len(), 1);
        assert_eq!(advice_list[0].advice_type, AdviceType::Before);
        assert_eq!(advice_list[0].name.as_deref(), Some("my-before"));
    }

    #[test]
    fn add_multiple_advice_sorted_by_type() {
        let mut mgr = AdviceManager::new();

        mgr.add_advice("fn", AdviceType::After, Value::symbol("after-fn"), None);
        mgr.add_advice("fn", AdviceType::Before, Value::symbol("before-fn"), None);
        mgr.add_advice("fn", AdviceType::FilterArgs, Value::symbol("filter-fn"), None);
        mgr.add_advice("fn", AdviceType::Around, Value::symbol("around-fn"), None);

        let advice_list = mgr.get_advice("fn");
        assert_eq!(advice_list.len(), 4);
        assert_eq!(advice_list[0].advice_type, AdviceType::FilterArgs);
        assert_eq!(advice_list[1].advice_type, AdviceType::Before);
        assert_eq!(advice_list[2].advice_type, AdviceType::Around);
        assert_eq!(advice_list[3].advice_type, AdviceType::After);
    }

    #[test]
    fn remove_advice_by_name() {
        let mut mgr = AdviceManager::new();

        mgr.add_advice(
            "fn",
            AdviceType::Before,
            Value::symbol("adv1"),
            Some("first".to_string()),
        );
        mgr.add_advice(
            "fn",
            AdviceType::After,
            Value::symbol("adv2"),
            Some("second".to_string()),
        );

        assert_eq!(mgr.get_advice("fn").len(), 2);

        mgr.remove_advice("fn", "first");
        assert_eq!(mgr.get_advice("fn").len(), 1);
        assert_eq!(mgr.get_advice("fn")[0].name.as_deref(), Some("second"));
    }

    #[test]
    fn remove_advice_by_symbol_name() {
        let mut mgr = AdviceManager::new();

        mgr.add_advice("fn", AdviceType::Before, Value::symbol("adv-fn"), None);
        assert!(mgr.has_advice("fn"));

        mgr.remove_advice("fn", "adv-fn");
        assert!(!mgr.has_advice("fn"));
    }

    #[test]
    fn replace_advice_by_name() {
        let mut mgr = AdviceManager::new();

        mgr.add_advice(
            "fn",
            AdviceType::Before,
            Value::symbol("old-fn"),
            Some("my-advice".to_string()),
        );
        mgr.add_advice(
            "fn",
            AdviceType::After,
            Value::symbol("new-fn"),
            Some("my-advice".to_string()),
        );

        // Should have replaced, not added
        assert_eq!(mgr.get_advice("fn").len(), 1);
        assert_eq!(mgr.get_advice("fn")[0].advice_type, AdviceType::After);
    }

    #[test]
    fn advice_member_p_works() {
        let mut mgr = AdviceManager::new();

        mgr.add_advice(
            "fn",
            AdviceType::Before,
            Value::symbol("my-adv"),
            Some("named-adv".to_string()),
        );

        assert!(mgr.advice_member_p("fn", "my-adv"));
        assert!(mgr.advice_member_p("fn", "named-adv"));
        assert!(!mgr.advice_member_p("fn", "nonexistent"));
        assert!(!mgr.advice_member_p("other-fn", "my-adv"));
    }

    #[test]
    fn get_advice_empty() {
        let mgr = AdviceManager::new();
        assert!(mgr.get_advice("nonexistent").is_empty());
        assert!(!mgr.has_advice("nonexistent"));
    }

    // -----------------------------------------------------------------------
    // VariableWatcherList tests
    // -----------------------------------------------------------------------

    #[test]
    fn add_and_notify_watcher() {
        let mut wl = VariableWatcherList::new();
        assert!(!wl.has_watchers("my-var"));

        wl.add_watcher("my-var", Value::symbol("my-watcher"));
        assert!(wl.has_watchers("my-var"));

        let calls = wl.notify_watchers(
            "my-var",
            &Value::Int(42),
            &Value::Int(0),
            "set",
        );
        assert_eq!(calls.len(), 1);

        let (callback, args) = &calls[0];
        assert!(matches!(callback, Value::Symbol(s) if s == "my-watcher"));
        assert_eq!(args.len(), 4);
        // arg 0: symbol name
        assert!(matches!(&args[0], Value::Symbol(s) if s == "my-var"));
        // arg 1: new value
        assert!(matches!(&args[1], Value::Int(42)));
        // arg 2: operation
        assert!(matches!(&args[2], Value::Symbol(s) if s == "set"));
        // arg 3: where (nil)
        assert!(matches!(&args[3], Value::Nil));
    }

    #[test]
    fn remove_watcher() {
        let mut wl = VariableWatcherList::new();
        wl.add_watcher("my-var", Value::symbol("watcher1"));
        wl.add_watcher("my-var", Value::symbol("watcher2"));
        assert!(wl.has_watchers("my-var"));

        wl.remove_watcher("my-var", "watcher1");
        let calls = wl.notify_watchers("my-var", &Value::Int(1), &Value::Int(0), "set");
        assert_eq!(calls.len(), 1);
        assert!(matches!(&calls[0].0, Value::Symbol(s) if s == "watcher2"));
    }

    #[test]
    fn remove_all_watchers_cleans_up() {
        let mut wl = VariableWatcherList::new();
        wl.add_watcher("my-var", Value::symbol("w1"));

        wl.remove_watcher("my-var", "w1");
        assert!(!wl.has_watchers("my-var"));
    }

    #[test]
    fn no_duplicate_watchers() {
        let mut wl = VariableWatcherList::new();
        wl.add_watcher("my-var", Value::symbol("w"));
        wl.add_watcher("my-var", Value::symbol("w"));

        let calls = wl.notify_watchers("my-var", &Value::Int(1), &Value::Int(0), "set");
        assert_eq!(calls.len(), 1);
    }

    #[test]
    fn notify_no_watchers_returns_empty() {
        let wl = VariableWatcherList::new();
        let calls = wl.notify_watchers("no-var", &Value::Int(1), &Value::Int(0), "set");
        assert!(calls.is_empty());
    }

    #[test]
    fn multiple_watchers_all_notified() {
        let mut wl = VariableWatcherList::new();
        wl.add_watcher("v", Value::symbol("w1"));
        wl.add_watcher("v", Value::symbol("w2"));
        wl.add_watcher("v", Value::symbol("w3"));

        let calls = wl.notify_watchers("v", &Value::Int(99), &Value::Int(0), "set");
        assert_eq!(calls.len(), 3);
    }
}
