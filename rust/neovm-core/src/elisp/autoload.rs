//! Autoload, compile-time evaluation, obsolete function/variable support.
//!
//! Provides:
//! - **Autoload system**: Deferred function loading — register a function as
//!   autoloaded so that its file is loaded on first use.
//! - **eval-when-compile / eval-and-compile**: Compile-time evaluation stubs
//!   (in the interpreter they just evaluate normally).
//! - **with-eval-after-load**: Deferred form execution after a file loads.
//! - **Obsolete aliases**: `define-obsolete-function-alias`,
//!   `define-obsolete-variable-alias`, `make-obsolete`, `make-obsolete-variable`.

use std::collections::HashMap;

use super::error::{signal, EvalResult};
use super::value::*;

// ---------------------------------------------------------------------------
// Autoload types
// ---------------------------------------------------------------------------

/// The kind of definition an autoload stands for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AutoloadType {
    /// Normal function (default).
    Function,
    /// Macro.
    Macro,
    /// Keymap.
    Keymap,
}

impl AutoloadType {
    /// Parse a Value into an AutoloadType.
    pub fn from_value(val: &Value) -> Self {
        match val.as_symbol_name() {
            Some("macro") => Self::Macro,
            Some("keymap") => Self::Keymap,
            _ => Self::Function,
        }
    }

    /// Convert back to a symbol Value.
    pub fn to_value(&self) -> Value {
        match self {
            Self::Function => Value::Nil,
            Self::Macro => Value::symbol("macro"),
            Self::Keymap => Value::symbol("keymap"),
        }
    }
}

/// An entry in the autoload table.
#[derive(Clone, Debug)]
pub struct AutoloadEntry {
    /// The function name that is autoloaded.
    pub name: String,
    /// The file to load when the function is first called.
    pub file: String,
    /// Optional documentation string.
    pub docstring: Option<String>,
    /// Whether the function is interactive (a command).
    pub interactive: bool,
    /// The type of definition (function, macro, keymap).
    pub autoload_type: AutoloadType,
}

// ---------------------------------------------------------------------------
// AutoloadManager
// ---------------------------------------------------------------------------

/// Central registry of autoloaded functions and eval-after-load callbacks.
pub struct AutoloadManager {
    /// Map from function name to autoload entry.
    entries: HashMap<String, AutoloadEntry>,
    /// Map from file/feature name to list of forms to evaluate after loading.
    after_load: HashMap<String, Vec<Value>>,
    /// Set of files that have already been loaded (for after-load tracking).
    loaded_files: Vec<String>,
    /// Obsolete function warnings: old-name -> (new-name, when).
    obsolete_functions: HashMap<String, (String, String)>,
    /// Obsolete variable warnings: old-name -> (new-name, when).
    obsolete_variables: HashMap<String, (String, String)>,
}

impl AutoloadManager {
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            after_load: HashMap::new(),
            loaded_files: Vec::new(),
            obsolete_functions: HashMap::new(),
            obsolete_variables: HashMap::new(),
        }
    }

    /// Register an autoload entry.
    pub fn register(&mut self, entry: AutoloadEntry) {
        self.entries.insert(entry.name.clone(), entry);
    }

    /// Check whether a function name has an autoload entry.
    pub fn is_autoloaded(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// Get the autoload entry for a function name.
    pub fn get_entry(&self, name: &str) -> Option<&AutoloadEntry> {
        self.entries.get(name)
    }

    /// Remove an autoload entry (used after the file has been loaded and the
    /// real definition is in place).
    pub fn remove(&mut self, name: &str) {
        self.entries.remove(name);
    }

    /// Register a form to evaluate after a given file/feature is loaded.
    pub fn add_after_load(&mut self, file: &str, form: Value) {
        self.after_load
            .entry(file.to_string())
            .or_default()
            .push(form);
    }

    /// Get the after-load forms for a file (if any).
    pub fn take_after_load_forms(&mut self, file: &str) -> Vec<Value> {
        self.after_load.remove(file).unwrap_or_default()
    }

    /// Record that a file has been loaded.
    pub fn mark_loaded(&mut self, file: &str) {
        if !self.loaded_files.contains(&file.to_string()) {
            self.loaded_files.push(file.to_string());
        }
    }

    /// Check if a file has already been loaded.
    pub fn is_loaded(&self, file: &str) -> bool {
        self.loaded_files.contains(&file.to_string())
    }

    /// Mark a function as obsolete.
    pub fn make_obsolete(&mut self, old_name: &str, new_name: &str, when: &str) {
        self.obsolete_functions.insert(
            old_name.to_string(),
            (new_name.to_string(), when.to_string()),
        );
    }

    /// Check if a function is marked obsolete.
    pub fn is_function_obsolete(&self, name: &str) -> bool {
        self.obsolete_functions.contains_key(name)
    }

    /// Get obsolete function info: (new-name, when).
    pub fn get_obsolete_function(&self, name: &str) -> Option<&(String, String)> {
        self.obsolete_functions.get(name)
    }

    /// Mark a variable as obsolete.
    pub fn make_variable_obsolete(&mut self, old_name: &str, new_name: &str, when: &str) {
        self.obsolete_variables.insert(
            old_name.to_string(),
            (new_name.to_string(), when.to_string()),
        );
    }

    /// Check if a variable is marked obsolete.
    pub fn is_variable_obsolete(&self, name: &str) -> bool {
        self.obsolete_variables.contains_key(name)
    }

    /// Get obsolete variable info: (new-name, when).
    pub fn get_obsolete_variable(&self, name: &str) -> Option<&(String, String)> {
        self.obsolete_variables.get(name)
    }
}

// ---------------------------------------------------------------------------
// Builtins (pure — need evaluator access)
// ---------------------------------------------------------------------------

/// `(autoloadp OBJ)` — return t if OBJ is an autoload object.
/// In our implementation, autoload objects are stored as special list values
/// of the form (autoload FILE DOCSTRING INTERACTIVE TYPE).
pub(crate) fn builtin_autoloadp(args: Vec<Value>) -> EvalResult {
    if args.len() != 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("autoloadp"), Value::Int(args.len() as i64)],
        ));
    }
    // Check if the value is an autoload form: a list starting with 'autoload
    Ok(Value::bool(is_autoload_value(&args[0])))
}

/// Check whether a value is an autoload form (autoload FILE ...).
pub(crate) fn is_autoload_value(val: &Value) -> bool {
    if let Some(items) = list_to_vec(val) {
        if let Some(first) = items.first() {
            if let Some(name) = first.as_symbol_name() {
                return name == "autoload";
            }
        }
    }
    false
}

/// `(autoload-do-load FUNDEF &optional FUNNAME MACRO-ONLY)` — trigger autoload.
/// If FUNDEF is an autoload form, load the file and return the new definition.
/// Otherwise return FUNDEF unchanged.
pub(crate) fn builtin_autoload_do_load(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() || args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("autoload-do-load"),
                Value::Int(args.len() as i64),
            ],
        ));
    }

    let fundef = &args[0];
    if !is_autoload_value(fundef) {
        return Ok(fundef.clone());
    }

    let items = list_to_vec(fundef).unwrap_or_default();
    // items[0] = 'autoload, items[1] = file, ...
    let file = if items.len() > 1 {
        match &items[1] {
            Value::Str(s) => (**s).clone(),
            _ => return Ok(fundef.clone()),
        }
    } else {
        return Ok(fundef.clone());
    };

    let funname = if args.len() > 1 {
        args[1].as_symbol_name().map(|s| s.to_string())
    } else {
        None
    };

    // Load the file
    let load_path = super::load::get_load_path(&eval.obarray);
    match super::load::find_file_in_load_path(&file, &load_path) {
        Some(path) => {
            eval.load_file_internal(&path)?;
        }
        None => {
            return Err(signal(
                "file-missing",
                vec![Value::string(format!(
                    "Cannot open load file: no such file or directory, {}",
                    file
                ))],
            ));
        }
    }

    // Return the new definition if we know the function name
    if let Some(name) = funname {
        if let Some(func) = eval.obarray.symbol_function(&name).cloned() {
            return Ok(func);
        }
    }

    Ok(Value::Nil)
}

/// `(symbol-file SYMBOL &optional TYPE)` — return the file that defined SYMBOL.
/// Stub: always returns nil for now.
pub(crate) fn builtin_symbol_file(args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("symbol-file"), Value::Int(args.len() as i64)],
        ));
    }
    // Stub: we don't track symbol origins yet.
    Ok(Value::Nil)
}

/// Evaluator-aware `(symbol-file SYMBOL &optional TYPE)`.
///
/// NeoVM currently tracks symbol origin only for autoloaded function symbols.
/// This matches GNU Emacs behavior for the currently supported subset:
/// - non-symbol SYMBOL returns nil
/// - TYPE nil/missing/`defun` queries function definition origin
/// - other TYPE values return nil
pub(crate) fn builtin_symbol_file_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() || args.len() > 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("symbol-file"), Value::Int(args.len() as i64)],
        ));
    }

    let symbol_name = match args[0].as_symbol_name() {
        Some(name) => name,
        None => return Ok(Value::Nil),
    };

    let function_origin_requested = if args.len() == 1 || matches!(args[1], Value::Nil) {
        true
    } else {
        matches!(args[1].as_symbol_name(), Some("defun"))
    };
    if !function_origin_requested {
        return Ok(Value::Nil);
    }

    if let Some(entry) = eval.autoloads.get_entry(symbol_name) {
        return Ok(Value::string(entry.file.clone()));
    }

    if let Some(fndef) = eval.obarray.symbol_function(symbol_name).cloned() {
        if is_autoload_value(&fndef) {
            if let Some(items) = list_to_vec(&fndef) {
                if let Some(Value::Str(file)) = items.get(1) {
                    return Ok(Value::string((**file).clone()));
                }
            }
        }
    }

    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Special form handlers (called from eval.rs try_special_form dispatch)
// ---------------------------------------------------------------------------

/// `(autoload FUNCTION FILE &optional DOCSTRING INTERACTIVE TYPE)`
///
/// Register FUNCTION to be autoloaded from FILE.  Creates an autoload form
/// `(autoload FILE DOCSTRING INTERACTIVE TYPE)` and stores it as the function
/// cell of the symbol.  Also registers an [`AutoloadEntry`] with the
/// evaluator's [`AutoloadManager`].  Returns the function name symbol.
pub(crate) fn sf_autoload(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    if tail.len() < 2 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("autoload"), Value::Int(tail.len() as i64)],
        ));
    }

    // FUNCTION is evaluated (typically a quoted symbol)
    let func_val = eval.eval(&tail[0])?;
    let name = match &func_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), func_val],
            ));
        }
    };

    // FILE is evaluated (should be a string)
    let file_val = eval.eval(&tail[1])?;
    let file = match &file_val {
        Value::Str(s) => (**s).clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), file_val],
            ));
        }
    };

    // Optional: DOCSTRING (index 2)
    let docstring_val = if tail.len() > 2 {
        eval.eval(&tail[2])?
    } else {
        Value::Nil
    };
    let docstring = match &docstring_val {
        Value::Str(s) => Some((**s).clone()),
        _ => None,
    };

    // Optional: INTERACTIVE (index 3)
    let interactive_val = if tail.len() > 3 {
        eval.eval(&tail[3])?
    } else {
        Value::Nil
    };
    let interactive = !matches!(interactive_val, Value::Nil);

    // Optional: TYPE (index 4)
    let type_val = if tail.len() > 4 {
        eval.eval(&tail[4])?
    } else {
        Value::Nil
    };
    let autoload_type = AutoloadType::from_value(&type_val);

    // Build the autoload form: (autoload FILE DOCSTRING INTERACTIVE TYPE)
    let autoload_form = Value::list(vec![
        Value::symbol("autoload"),
        Value::string(file.clone()),
        docstring_val,
        interactive_val,
        type_val,
    ]);

    // Set function cell
    eval.obarray.set_symbol_function(&name, autoload_form);

    // Register in autoload manager
    eval.autoloads.register(AutoloadEntry {
        name: name.clone(),
        file,
        docstring,
        interactive,
        autoload_type,
    });

    Ok(Value::Symbol(name))
}

/// `(eval-when-compile &rest BODY)`
///
/// In the interpreter, simply evaluates BODY sequentially and returns the last
/// result (identical to `progn`).
pub(crate) fn sf_eval_when_compile(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    eval.sf_progn(tail)
}

/// `(eval-and-compile &rest BODY)`
///
/// In the interpreter, simply evaluates BODY sequentially and returns the last
/// result (identical to `progn`).
pub(crate) fn sf_eval_and_compile(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    eval.sf_progn(tail)
}

/// `(declare-function FN FILE &rest ARGLIST)`
///
/// Stub: tells the byte-compiler that FN is defined in FILE.  The interpreter
/// ignores it and returns nil.
pub(crate) fn sf_declare_function(
    _eval: &mut super::eval::Evaluator,
    _tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    Ok(Value::Nil)
}

/// `(define-obsolete-function-alias OLD NEW WHEN)`
///
/// Creates a function alias from OLD to NEW (like `defalias`) and records the
/// obsolescence information.  Returns the OLD name symbol.
pub(crate) fn sf_define_obsolete_function_alias(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("define-obsolete-function-alias"),
                Value::Int(tail.len() as i64),
            ],
        ));
    }

    let old_val = eval.eval(&tail[0])?;
    let old_name = match &old_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), old_val],
            ));
        }
    };

    let new_val = eval.eval(&tail[1])?;
    let new_name = match &new_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), new_val],
            ));
        }
    };

    let when_val = eval.eval(&tail[2])?;
    let when = match &when_val {
        Value::Str(s) => (**s).clone(),
        _ => format!("{}", when_val),
    };

    // Create the alias: old-fn -> new-fn's symbol
    eval.obarray
        .set_symbol_function(&old_name, Value::Symbol(new_name.clone()));

    // Register obsolete info
    eval.autoloads.make_obsolete(&old_name, &new_name, &when);

    Ok(Value::Symbol(old_name))
}

/// `(define-obsolete-variable-alias OLD NEW WHEN)`
///
/// Copies the value of NEW to OLD (if NEW is bound) and records the
/// obsolescence information.  Returns the OLD name symbol.
pub(crate) fn sf_define_obsolete_variable_alias(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("define-obsolete-variable-alias"),
                Value::Int(tail.len() as i64),
            ],
        ));
    }

    let old_val = eval.eval(&tail[0])?;
    let old_name = match &old_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), old_val],
            ));
        }
    };

    let new_val = eval.eval(&tail[1])?;
    let new_name = match &new_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), new_val],
            ));
        }
    };

    let when_val = eval.eval(&tail[2])?;
    let when = match &when_val {
        Value::Str(s) => (**s).clone(),
        _ => format!("{}", when_val),
    };

    // Copy value: if the new variable is bound, set old variable to the same value
    if let Some(val) = eval.obarray.symbol_value(&new_name).cloned() {
        eval.obarray.set_symbol_value(&old_name, val);
    }

    // Register obsolete variable info
    eval.autoloads
        .make_variable_obsolete(&old_name, &new_name, &when);

    Ok(Value::Symbol(old_name))
}

/// `(make-obsolete OLD NEW WHEN)`
///
/// Mark function OLD as obsolete in favour of NEW since version WHEN.
/// Returns the OLD name symbol.
pub(crate) fn sf_make_obsolete(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-obsolete"),
                Value::Int(tail.len() as i64),
            ],
        ));
    }

    let old_val = eval.eval(&tail[0])?;
    let old_name = match &old_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), old_val],
            ));
        }
    };

    let new_val = eval.eval(&tail[1])?;
    let new_name = match &new_val {
        Value::Symbol(s) => s.clone(),
        _ => format!("{}", new_val),
    };

    let when_val = eval.eval(&tail[2])?;
    let when = match &when_val {
        Value::Str(s) => (**s).clone(),
        _ => format!("{}", when_val),
    };

    eval.autoloads.make_obsolete(&old_name, &new_name, &when);

    Ok(Value::Symbol(old_name))
}

/// `(make-obsolete-variable OLD NEW WHEN)`
///
/// Mark variable OLD as obsolete in favour of NEW since version WHEN.
/// Returns the OLD name symbol.
pub(crate) fn sf_make_obsolete_variable(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    if tail.len() < 3 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("make-obsolete-variable"),
                Value::Int(tail.len() as i64),
            ],
        ));
    }

    let old_val = eval.eval(&tail[0])?;
    let old_name = match &old_val {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), old_val],
            ));
        }
    };

    let new_val = eval.eval(&tail[1])?;
    let new_name = match &new_val {
        Value::Symbol(s) => s.clone(),
        _ => format!("{}", new_val),
    };

    let when_val = eval.eval(&tail[2])?;
    let when = match &when_val {
        Value::Str(s) => (**s).clone(),
        _ => format!("{}", when_val),
    };

    eval.autoloads
        .make_variable_obsolete(&old_name, &new_name, &when);

    Ok(Value::Symbol(old_name))
}

/// `(with-eval-after-load FILE &rest BODY)`
///
/// Arrange to evaluate BODY forms after FILE is loaded.  The body is NOT
/// evaluated immediately — it is stored and will be run when the file is
/// eventually loaded via `load` or `require`.  Returns nil.
pub(crate) fn sf_with_eval_after_load(
    eval: &mut super::eval::Evaluator,
    tail: &[super::expr::Expr],
) -> super::error::EvalResult {
    use super::eval::quote_to_value;

    if tail.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("with-eval-after-load"),
                Value::Int(tail.len() as i64),
            ],
        ));
    }

    // FILE is evaluated (should be a string)
    let file_val = eval.eval(&tail[0])?;
    let file = match &file_val {
        Value::Str(s) => (**s).clone(),
        _ => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), file_val],
            ));
        }
    };

    // Store body forms (unevaluated) as a progn form for later execution
    let body_forms: Vec<Value> = tail[1..].iter().map(quote_to_value).collect();
    let progn_form = if body_forms.len() == 1 {
        body_forms.into_iter().next().unwrap()
    } else {
        let mut items = vec![Value::symbol("progn")];
        items.extend(body_forms);
        Value::list(items)
    };

    eval.autoloads.add_after_load(&file, progn_form);

    Ok(Value::Nil)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

    fn eval_one(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let result = ev.eval_expr(&forms[0]);
        format_eval_result(&result)
    }

    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    fn eval_all_with(ev: &mut Evaluator, src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        ev.eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect()
    }

    // -----------------------------------------------------------------------
    // AutoloadManager unit tests
    // -----------------------------------------------------------------------

    #[test]
    fn autoload_manager_register_and_lookup() {
        let mut mgr = AutoloadManager::new();
        assert!(!mgr.is_autoloaded("foo"));

        mgr.register(AutoloadEntry {
            name: "foo".into(),
            file: "foo-lib".into(),
            docstring: Some("Do foo things.".into()),
            interactive: false,
            autoload_type: AutoloadType::Function,
        });

        assert!(mgr.is_autoloaded("foo"));
        let entry = mgr.get_entry("foo").unwrap();
        assert_eq!(entry.file, "foo-lib");
        assert_eq!(entry.docstring.as_deref(), Some("Do foo things."));
        assert!(!entry.interactive);
        assert_eq!(entry.autoload_type, AutoloadType::Function);
    }

    #[test]
    fn autoload_manager_remove() {
        let mut mgr = AutoloadManager::new();
        mgr.register(AutoloadEntry {
            name: "bar".into(),
            file: "bar-lib".into(),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Macro,
        });
        assert!(mgr.is_autoloaded("bar"));
        mgr.remove("bar");
        assert!(!mgr.is_autoloaded("bar"));
    }

    #[test]
    fn autoload_manager_multiple_entries() {
        let mut mgr = AutoloadManager::new();
        mgr.register(AutoloadEntry {
            name: "a".into(),
            file: "file-a".into(),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Function,
        });
        mgr.register(AutoloadEntry {
            name: "b".into(),
            file: "file-b".into(),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Keymap,
        });
        assert!(mgr.is_autoloaded("a"));
        assert!(mgr.is_autoloaded("b"));
        assert!(!mgr.is_autoloaded("c"));
    }

    #[test]
    fn autoload_type_from_value() {
        assert_eq!(
            AutoloadType::from_value(&Value::Nil),
            AutoloadType::Function
        );
        assert_eq!(
            AutoloadType::from_value(&Value::symbol("macro")),
            AutoloadType::Macro
        );
        assert_eq!(
            AutoloadType::from_value(&Value::symbol("keymap")),
            AutoloadType::Keymap
        );
        assert_eq!(
            AutoloadType::from_value(&Value::symbol("unknown")),
            AutoloadType::Function
        );
    }

    #[test]
    fn autoload_type_roundtrip() {
        let types = [
            AutoloadType::Function,
            AutoloadType::Macro,
            AutoloadType::Keymap,
        ];
        for ty in &types {
            let val = ty.to_value();
            let back = AutoloadType::from_value(&val);
            assert_eq!(&back, ty);
        }
    }

    #[test]
    fn after_load_add_and_take() {
        let mut mgr = AutoloadManager::new();
        mgr.add_after_load("my-file", Value::Int(1));
        mgr.add_after_load("my-file", Value::Int(2));
        mgr.add_after_load("other-file", Value::Int(3));

        let forms = mgr.take_after_load_forms("my-file");
        assert_eq!(forms.len(), 2);

        // After taking, should be empty
        let forms2 = mgr.take_after_load_forms("my-file");
        assert!(forms2.is_empty());

        // Other file still has its form
        let forms3 = mgr.take_after_load_forms("other-file");
        assert_eq!(forms3.len(), 1);
    }

    #[test]
    fn loaded_files_tracking() {
        let mut mgr = AutoloadManager::new();
        assert!(!mgr.is_loaded("foo.el"));
        mgr.mark_loaded("foo.el");
        assert!(mgr.is_loaded("foo.el"));
        // Duplicate mark is harmless
        mgr.mark_loaded("foo.el");
        assert!(mgr.is_loaded("foo.el"));
    }

    #[test]
    fn obsolete_function_tracking() {
        let mut mgr = AutoloadManager::new();
        assert!(!mgr.is_function_obsolete("old-fn"));
        mgr.make_obsolete("old-fn", "new-fn", "28.1");
        assert!(mgr.is_function_obsolete("old-fn"));
        let info = mgr.get_obsolete_function("old-fn").unwrap();
        assert_eq!(info.0, "new-fn");
        assert_eq!(info.1, "28.1");
    }

    #[test]
    fn obsolete_variable_tracking() {
        let mut mgr = AutoloadManager::new();
        assert!(!mgr.is_variable_obsolete("old-var"));
        mgr.make_variable_obsolete("old-var", "new-var", "27.1");
        assert!(mgr.is_variable_obsolete("old-var"));
        let info = mgr.get_obsolete_variable("old-var").unwrap();
        assert_eq!(info.0, "new-var");
        assert_eq!(info.1, "27.1");
    }

    // -----------------------------------------------------------------------
    // is_autoload_value tests
    // -----------------------------------------------------------------------

    #[test]
    fn is_autoload_value_positive() {
        let val = Value::list(vec![Value::symbol("autoload"), Value::string("my-file")]);
        assert!(is_autoload_value(&val));
    }

    #[test]
    fn is_autoload_value_negative() {
        assert!(!is_autoload_value(&Value::Nil));
        assert!(!is_autoload_value(&Value::Int(42)));
        assert!(!is_autoload_value(&Value::list(vec![
            Value::symbol("lambda"),
            Value::Nil,
        ])));
    }

    // -----------------------------------------------------------------------
    // Special form tests (eval-level)
    // -----------------------------------------------------------------------

    #[test]
    fn autoload_special_form_registers() {
        let results = eval_all(
            r#"(autoload 'my-func "my-file" "A function." t)
               (autoloadp (symbol-function 'my-func))"#,
        );
        // autoload should return the function name as a symbol
        assert_eq!(results[0], "OK my-func");
        // autoloadp should recognize the autoload form
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn autoload_minimal_form() {
        // Minimal autoload: just function name and file
        let results = eval_all(
            r#"(autoload 'minimal-fn "min-file")
               (autoloadp (symbol-function 'minimal-fn))"#,
        );
        assert_eq!(results[0], "OK minimal-fn");
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn autoload_with_type() {
        let results = eval_all(
            r#"(autoload 'my-macro "macro-file" nil nil 'macro)
               (autoloadp (symbol-function 'my-macro))"#,
        );
        assert_eq!(results[0], "OK my-macro");
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn eval_when_compile_evaluates_body() {
        let result = eval_one("(eval-when-compile (+ 1 2))");
        assert_eq!(result, "OK 3");
    }

    #[test]
    fn eval_when_compile_multiple_forms() {
        let result = eval_one("(eval-when-compile 1 2 (+ 3 4))");
        assert_eq!(result, "OK 7");
    }

    #[test]
    fn eval_and_compile_evaluates_body() {
        let result = eval_one("(eval-and-compile (+ 10 20))");
        assert_eq!(result, "OK 30");
    }

    #[test]
    fn eval_and_compile_multiple_forms() {
        // Should return the last form's value
        let result = eval_one("(eval-and-compile (setq x 1) (setq y 2) (+ x y))");
        assert_eq!(result, "OK 3");
    }

    #[test]
    fn declare_function_returns_nil() {
        let result = eval_one(r#"(declare-function foo "bar.el")"#);
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn declare_function_multiple_args() {
        let result = eval_one(r#"(declare-function baz "quux.el" (x y))"#);
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn symbol_file_returns_nil() {
        let result = eval_one("(symbol-file 'cons)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn symbol_file_returns_autoload_file_for_function() {
        let result = eval_one(
            r#"(progn (autoload 'sym-file-probe "sym-file-probe-file") (symbol-file 'sym-file-probe))"#,
        );
        assert_eq!(result, r#"OK "sym-file-probe-file""#);
    }

    #[test]
    fn symbol_file_type_gate_matches_defun_only() {
        let results = eval_all(
            r#"(autoload 'sym-file-type-probe "sym-file-type-probe-file")
               (symbol-file 'sym-file-type-probe 'defun)
               (symbol-file 'sym-file-type-probe 'var)
               (symbol-file 'sym-file-type-probe 'function)"#,
        );
        assert_eq!(results[1], r#"OK "sym-file-type-probe-file""#);
        assert_eq!(results[2], "OK nil");
        assert_eq!(results[3], "OK nil");
    }

    #[test]
    fn symbol_file_non_symbol_returns_nil() {
        let results = eval_all(
            r#"(symbol-file 1)
               (symbol-file "x")
               (symbol-file 'car 1)"#,
        );
        assert_eq!(results[0], "OK nil");
        assert_eq!(results[1], "OK nil");
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn symbol_file_accepts_third_arg_but_not_fourth() {
        let results = eval_all(
            r#"(autoload 'sym-file-arity-probe "sym-file-arity-probe-file")
               (symbol-file 'sym-file-arity-probe 'defun t)
               (condition-case err
                   (symbol-file 'sym-file-arity-probe 'defun t :extra)
                 (error err))"#,
        );
        assert_eq!(results[1], r#"OK "sym-file-arity-probe-file""#);
        assert_eq!(results[2], "OK (wrong-number-of-arguments symbol-file 4)");
    }

    #[test]
    fn autoloadp_non_autoload() {
        let result = eval_one("(autoloadp 42)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn autoloadp_nil() {
        let result = eval_one("(autoloadp nil)");
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn define_obsolete_function_alias_creates_alias() {
        let results = eval_all(
            r#"(defun new-fn () 42)
               (define-obsolete-function-alias 'old-fn 'new-fn "28.1")
               (old-fn)"#,
        );
        assert_eq!(results[2], "OK 42");
    }

    #[test]
    fn make_obsolete_returns_name() {
        let results = eval_all(
            r#"(defun test-fn () nil)
               (make-obsolete 'test-fn 'better-fn "29.1")"#,
        );
        assert_eq!(results[1], "OK test-fn");
    }

    #[test]
    fn make_obsolete_variable_returns_name() {
        let results = eval_all(
            r#"(defvar old-var 1)
               (make-obsolete-variable 'old-var 'new-var "29.1")"#,
        );
        assert_eq!(results[1], "OK old-var");
    }

    #[test]
    fn define_obsolete_variable_alias_creates_alias() {
        let results = eval_all(
            r#"(defvar new-var 100)
               (define-obsolete-variable-alias 'legacy-var 'new-var "28.1")
               legacy-var"#,
        );
        // The old variable should get the value of the new one
        assert_eq!(results[2], "OK 100");
    }

    #[test]
    fn with_eval_after_load_stores_form() {
        // When the file hasn't been loaded yet, the form should be stored
        // and not evaluated immediately.
        let results = eval_all(
            r#"(setq test-counter 0)
               (with-eval-after-load "nonexistent-file"
                 (setq test-counter (1+ test-counter)))
               test-counter"#,
        );
        // The form should NOT have been evaluated since the file hasn't been loaded
        assert_eq!(results[2], "OK 0");
    }

    #[test]
    fn with_eval_after_load_returns_nil() {
        let result = eval_one(r#"(with-eval-after-load "some-file" (+ 1 2))"#);
        // with-eval-after-load always returns nil
        assert_eq!(result, "OK nil");
    }

    #[test]
    fn autoload_entry_interactive_flag() {
        let mut mgr = AutoloadManager::new();
        mgr.register(AutoloadEntry {
            name: "cmd".into(),
            file: "cmd-file".into(),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Function,
        });
        let entry = mgr.get_entry("cmd").unwrap();
        assert!(entry.interactive);
    }

    #[test]
    fn autoload_entry_keymap_type() {
        let mut mgr = AutoloadManager::new();
        mgr.register(AutoloadEntry {
            name: "my-map".into(),
            file: "map-file".into(),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Keymap,
        });
        let entry = mgr.get_entry("my-map").unwrap();
        assert_eq!(entry.autoload_type, AutoloadType::Keymap);
    }

    #[test]
    fn autoload_overwrites_previous() {
        let mut mgr = AutoloadManager::new();
        mgr.register(AutoloadEntry {
            name: "f".into(),
            file: "old-file".into(),
            docstring: None,
            interactive: false,
            autoload_type: AutoloadType::Function,
        });
        mgr.register(AutoloadEntry {
            name: "f".into(),
            file: "new-file".into(),
            docstring: None,
            interactive: true,
            autoload_type: AutoloadType::Macro,
        });
        let entry = mgr.get_entry("f").unwrap();
        assert_eq!(entry.file, "new-file");
        assert!(entry.interactive);
        assert_eq!(entry.autoload_type, AutoloadType::Macro);
    }

    #[test]
    fn autoload_registers_in_autoload_manager() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(autoload 'test-auto-fn "test-auto-file" "Test doc" t 'macro)"#,
        );
        assert_eq!(results[0], "OK test-auto-fn");
        assert!(ev.autoloads.is_autoloaded("test-auto-fn"));
        let entry = ev.autoloads.get_entry("test-auto-fn").unwrap();
        assert_eq!(entry.file, "test-auto-file");
        assert_eq!(entry.docstring.as_deref(), Some("Test doc"));
        assert!(entry.interactive);
        assert_eq!(entry.autoload_type, AutoloadType::Macro);
    }
}
