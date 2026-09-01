//! Autoload and deferred after-load support.
//!
//! Provides:
//! - **Autoload system**: Deferred function loading — register a function as
//!   autoloaded so that its file is loaded on first use.
//! - **with-eval-after-load**: Deferred form execution after a file loads.
//! - **Obsolete aliases**: `define-obsolete-function-alias`,
//!   `define-obsolete-variable-alias`, `make-obsolete`, `make-obsolete-variable`.

use crate::emacs_core::error::LispCondition;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

use super::error::{EvalResult, Flow, signal};
use super::intern::{intern, resolve_sym};
use super::symbol::Obarray;
use super::value::*;
use crate::emacs_core::SymId;
use crate::emacs_core::builtins::symbols::expect_symbol_id;
use crate::emacs_core::value::ValueKind;
use crate::gc_trace::GcTrace;
use crate::heap_types::LispString;

type ObsoleteInfo = (LispString, LispString);
const UNSET_AUTOLOAD_SYMBOL_ID: u32 = u32::MAX;
static AUTOLOAD_SYMBOL_ID: AtomicU32 = AtomicU32::new(UNSET_AUTOLOAD_SYMBOL_ID);

#[inline]
fn autoload_symbol_id() -> SymId {
    let cached = AUTOLOAD_SYMBOL_ID.load(Ordering::Relaxed);
    if cached != UNSET_AUTOLOAD_SYMBOL_ID {
        return SymId(cached);
    }
    let id = intern("autoload");
    AUTOLOAD_SYMBOL_ID.store(id.0, Ordering::Relaxed);
    id
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
pub(crate) struct AfterLoadKey(LispString);

impl AfterLoadKey {
    pub(crate) fn from_runtime(text: &str) -> Self {
        Self(runtime_string_to_autoload_string(text))
    }

    pub(crate) fn from_lisp_string(text: &LispString) -> Self {
        // Normalize to multibyte so keys match `from_runtime` (which decodes to
        // a multibyte string) — without round-tripping through a storage String.
        Self(if text.is_multibyte() {
            text.clone()
        } else {
            LispString::from_emacs_bytes(crate::emacs_core::emacs_char::str_to_multibyte(
                text.as_bytes(),
            ))
        })
    }

    pub(crate) fn as_lisp_string(&self) -> &LispString {
        &self.0
    }
}

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
        if val.is_t() {
            return Self::Macro;
        }
        match val.as_symbol_name() {
            Some("macro") => Self::Macro,
            Some("keymap") => Self::Keymap,
            _ => Self::Function,
        }
    }

    /// Convert back to a symbol Value.
    pub fn to_value(&self) -> Value {
        match self {
            Self::Function => Value::NIL,
            Self::Macro => Value::symbol("macro"),
            Self::Keymap => Value::symbol("keymap"),
        }
    }
}

/// An entry in the autoload table.
#[derive(Clone, Debug)]
pub struct AutoloadEntry {
    /// The file to load when the function is first called.
    pub file: LispString,
    /// Optional documentation string.
    pub docstring: Option<LispString>,
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
    entries: HashMap<SymId, AutoloadEntry>,
    /// Map from file/feature name to list of forms to evaluate after loading.
    after_load: HashMap<AfterLoadKey, Vec<Value>>,
    /// Set of files that have already been loaded (for after-load tracking).
    loaded_files: Vec<LispString>,
    /// Obsolete function warnings: old-name -> (new-name, when).
    obsolete_functions: HashMap<SymId, ObsoleteInfo>,
    /// Obsolete variable warnings: old-name -> (new-name, when).
    obsolete_variables: HashMap<SymId, ObsoleteInfo>,
}

impl Default for AutoloadManager {
    fn default() -> Self {
        Self::new()
    }
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
    pub fn register(&mut self, name: &str, entry: AutoloadEntry) {
        self.register_symbol(intern(name), entry);
    }

    pub fn register_symbol(&mut self, name: SymId, entry: AutoloadEntry) {
        self.entries.insert(name, entry);
    }

    /// Check whether a function name has an autoload entry.
    pub fn is_autoloaded(&self, name: &str) -> bool {
        self.is_autoloaded_symbol(intern(name))
    }

    pub fn is_autoloaded_symbol(&self, name: SymId) -> bool {
        self.entries.contains_key(&name)
    }

    /// Get the autoload entry for a function name.
    pub fn get_entry(&self, name: &str) -> Option<&AutoloadEntry> {
        self.get_entry_symbol(intern(name))
    }

    pub fn get_entry_symbol(&self, name: SymId) -> Option<&AutoloadEntry> {
        self.entries.get(&name)
    }

    /// Snapshot current autoload entries for callers that need to rebuild
    /// function cells from the registered autoload metadata.
    pub fn entries_snapshot(&self) -> Vec<(String, AutoloadEntry)> {
        self.entries
            .iter()
            .map(|(name, entry)| (resolve_sym(*name).to_string(), entry.clone()))
            .collect()
    }

    /// Remove an autoload entry (used after the file has been loaded and the
    /// real definition is in place).
    pub fn remove(&mut self, name: &str) {
        self.remove_symbol(intern(name));
    }

    pub fn remove_symbol(&mut self, name: SymId) {
        self.entries.remove(&name);
    }

    /// Register a form to evaluate after a given file/feature is loaded.
    pub fn add_after_load(&mut self, file: &str, form: Value) {
        self.add_after_load_key(AfterLoadKey::from_runtime(file), form);
    }

    pub(crate) fn add_after_load_key(&mut self, file: AfterLoadKey, form: Value) {
        self.after_load.entry(file).or_default().push(form);
    }

    /// Get the after-load forms for a file (if any).
    pub fn take_after_load_forms(&mut self, file: &str) -> Vec<Value> {
        self.take_after_load_forms_key(&AfterLoadKey::from_runtime(file))
    }

    pub(crate) fn take_after_load_forms_key(&mut self, file: &AfterLoadKey) -> Vec<Value> {
        self.after_load.remove(file).unwrap_or_default()
    }

    /// Record that a file has been loaded.
    pub fn mark_loaded(&mut self, file: &str) {
        let file = runtime_string_to_autoload_string(file);
        if !self.loaded_files.contains(&file) {
            self.loaded_files.push(file);
        }
    }

    /// Check if a file has already been loaded.
    pub fn is_loaded(&self, file: &str) -> bool {
        let file = runtime_string_to_autoload_string(file);
        self.loaded_files.contains(&file)
    }

    /// Mark a function as obsolete.
    pub fn make_obsolete(&mut self, old_name: &str, new_name: &str, when: &str) {
        self.make_obsolete_symbol(
            intern(old_name),
            runtime_string_to_autoload_string(new_name),
            runtime_string_to_autoload_string(when),
        );
    }

    pub fn make_obsolete_symbol(
        &mut self,
        old_name: SymId,
        new_name: LispString,
        when: LispString,
    ) {
        self.obsolete_functions.insert(old_name, (new_name, when));
    }

    /// Check if a function is marked obsolete.
    pub fn is_function_obsolete(&self, name: &str) -> bool {
        self.is_function_obsolete_symbol(intern(name))
    }

    pub fn is_function_obsolete_symbol(&self, name: SymId) -> bool {
        self.obsolete_functions.contains_key(&name)
    }

    /// Get obsolete function info: (new-name, when).
    pub fn get_obsolete_function(&self, name: &str) -> Option<(LispString, LispString)> {
        self.get_obsolete_function_symbol(intern(name))
    }

    pub fn get_obsolete_function_symbol(&self, name: SymId) -> Option<(LispString, LispString)> {
        self.obsolete_functions
            .get(&name)
            .map(|(new_name, when)| (new_name.clone(), when.clone()))
    }

    /// Mark a variable as obsolete.
    pub fn make_variable_obsolete(&mut self, old_name: &str, new_name: &str, when: &str) {
        self.make_variable_obsolete_symbol(
            intern(old_name),
            runtime_string_to_autoload_string(new_name),
            runtime_string_to_autoload_string(when),
        );
    }

    pub fn make_variable_obsolete_symbol(
        &mut self,
        old_name: SymId,
        new_name: LispString,
        when: LispString,
    ) {
        self.obsolete_variables.insert(old_name, (new_name, when));
    }

    /// Check if a variable is marked obsolete.
    pub fn is_variable_obsolete(&self, name: &str) -> bool {
        self.is_variable_obsolete_symbol(intern(name))
    }

    pub fn is_variable_obsolete_symbol(&self, name: SymId) -> bool {
        self.obsolete_variables.contains_key(&name)
    }

    /// Get obsolete variable info: (new-name, when).
    pub fn get_obsolete_variable(&self, name: &str) -> Option<(LispString, LispString)> {
        self.get_obsolete_variable_symbol(intern(name))
    }

    pub fn get_obsolete_variable_symbol(&self, name: SymId) -> Option<(LispString, LispString)> {
        self.obsolete_variables
            .get(&name)
            .map(|(new_name, when)| (new_name.clone(), when.clone()))
    }

    // pdump accessors
    pub(crate) fn dump_entries(&self) -> &HashMap<SymId, AutoloadEntry> {
        &self.entries
    }
    // `AfterLoadKey` stores Lisp strings whose interior mutability is managed
    // by the non-moving GC; pdump must expose the map without changing its key.
    #[allow(clippy::mutable_key_type)]
    pub(crate) fn dump_after_load(&self) -> &HashMap<AfterLoadKey, Vec<Value>> {
        &self.after_load
    }
    pub(crate) fn dump_loaded_files(&self) -> &[LispString] {
        &self.loaded_files
    }
    pub(crate) fn dump_obsolete_functions(&self) -> &HashMap<SymId, ObsoleteInfo> {
        &self.obsolete_functions
    }
    pub(crate) fn dump_obsolete_variables(&self) -> &HashMap<SymId, ObsoleteInfo> {
        &self.obsolete_variables
    }
    #[allow(clippy::mutable_key_type)] // restores the same GC-aware Lisp key representation
    pub(crate) fn from_dump(
        entries: HashMap<SymId, AutoloadEntry>,
        after_load: HashMap<AfterLoadKey, Vec<Value>>,
        loaded_files: Vec<LispString>,
        obsolete_functions: HashMap<SymId, ObsoleteInfo>,
        obsolete_variables: HashMap<SymId, ObsoleteInfo>,
    ) -> Self {
        Self {
            entries,
            after_load,
            loaded_files,
            obsolete_functions,
            obsolete_variables,
        }
    }
}

fn runtime_string_to_autoload_string(text: &str) -> LispString {
    super::builtins::plain_str_to_lisp_string(text, true)
}

// ---------------------------------------------------------------------------
// Builtins (pure — need evaluator access)
// ---------------------------------------------------------------------------

/// Check whether a value is an autoload form (autoload FILE ...).
pub(crate) fn is_autoload_value(val: &Value) -> bool {
    val.is_cons()
        && val
            .cons_car()
            .as_symbol_id()
            .is_some_and(|id| id == autoload_symbol_id())
}

/// `(autoload-do-load FUNDEF &optional FUNNAME MACRO-ONLY)` — trigger autoload.
/// If FUNDEF is an autoload form, load the file and return the new definition.
/// Otherwise return FUNDEF unchanged.
pub(crate) enum AutoloadDoLoadPlan {
    Return(Value),
    Load {
        file: LispString,
        funname: Option<SymId>,
    },
}

pub(crate) fn plan_autoload_do_load_in_state(
    obarray: &Obarray,
    args: &[Value],
) -> Result<AutoloadDoLoadPlan, Flow> {
    if args.is_empty() || args.len() > 3 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("autoload-do-load"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }

    let fundef = &args[0];
    if !is_autoload_value(fundef) {
        return Ok(AutoloadDoLoadPlan::Return(*fundef));
    }

    let items = list_to_vec(fundef).unwrap_or_default();
    // items[0] = 'autoload, items[1] = file, ...
    let file = if items.len() > 1 {
        match items[1].kind() {
            ValueKind::String => items[1]
                .as_lisp_string()
                .cloned()
                .expect("ValueKind::String must carry LispString payload"),
            _ => return Ok(AutoloadDoLoadPlan::Return(*fundef)),
        }
    } else {
        return Ok(AutoloadDoLoadPlan::Return(*fundef));
    };

    // MACRO-ONLY check: if the third arg is `macro`, only autoload if the
    // autoload's TYPE field (5th element) is `t` or `macro`.
    // This matches GNU Emacs eval.c:Fautoload_do_load.
    let macro_only = args.get(2).copied().unwrap_or(Value::NIL);
    if macro_only.as_symbol_name() == Some("macro") {
        let kind = items.get(4).copied().unwrap_or(Value::NIL);
        let is_macro_type = kind.is_t() || (kind.as_symbol_name() == Some("macro"));
        if !is_macro_type {
            return Ok(AutoloadDoLoadPlan::Return(*fundef));
        }
    }

    let funname = if args.len() > 1 && !args[1].is_nil() {
        Some(expect_symbol_id(&args[1])?)
    } else {
        None
    };

    if let Some(name) = funname
        && let Some(override_value) =
            super::eval::compiler_function_override_in_obarray(obarray, name)
    {
        return Ok(AutoloadDoLoadPlan::Return(override_value));
    }

    Ok(AutoloadDoLoadPlan::Load { file, funname })
}

pub(crate) fn resolve_autoload_load_path(
    obarray: &Obarray,
    buf: Option<&crate::buffer::Buffer>,
    file: &LispString,
) -> Result<PathBuf, Flow> {
    super::load::resolve_autoload_load_path_in_state(obarray, buf, file)
}

/// After loading an autoload file, check whether the function was defined.
/// Matches GNU Emacs eval.c:Fautoload_do_load lines 2501-2508: if the
/// function cell is still the same autoload, signal an error.
pub(crate) fn finish_autoload_do_load_in_state(
    obarray: &Obarray,
    funname: Option<SymId>,
    original_autoload: Option<&Value>,
) -> EvalResult {
    if let Some(name) = funname
        && let Some(func) = obarray.symbol_function_id(name)
    {
        // GNU Emacs: if function is still the same autoload form after
        // loading, signal "Autoloading file failed to define function".
        // GNU Emacs eval.c: if (!NILP (Fequal (fun, fundef)))
        // Only error if the function cell is STILL the SAME autoload
        // form (by identity, not by content, to avoid stale reference).
        // A different autoload (re-registered by eval-after-load
        // callbacks) is fine — the function was redefined.
        if let Some(orig) = original_autoload {
            let same_identity = match (func.kind(), orig.kind()) {
                (ValueKind::Cons, ValueKind::Cons) => func.bits() == orig.bits(),
                _ => false,
            };
            if same_identity {
                return Err(signal(
                    "error",
                    vec![Value::string(format!(
                        "Autoloading failed to define function {}",
                        resolve_sym(name)
                    ))],
                ));
            }
        }
        return Ok(obarray.indirect_function_id(name).unwrap_or(func));
    }
    Ok(Value::NIL)
}

/// Run an implicit dependency load with GNU's caller-state isolation.
///
/// GNU funnels both `autoload-do-load` and `require` through
/// `load_with_autoload_queue`.  Besides its transactional definition queue,
/// that boundary calls `save_match_data_load`, so arbitrary searches performed
/// while reading and evaluating a dependency cannot overwrite the caller's
/// search registers.  Keep that shared semantic boundary explicit: direct
/// `(load ...)` is intentionally allowed to change match data, while implicit
/// loads are not.
///
/// Neomacs does not yet model GNU's complete transactional autoload queue.
/// Centralizing the implicit-load boundary here gives that future state one
/// home instead of duplicating unwind policy between `require` and autoload.
pub(crate) fn with_implicit_load_state<T>(
    eval: &mut super::eval::Context,
    operation: impl FnOnce(&mut super::eval::Context) -> Result<T, Flow>,
) -> Result<T, Flow> {
    super::builtins::search::with_preserved_match_data(eval, operation)
}

/// Execute the stateful half of GNU `autoload-do-load`.
///
/// Every evaluator adapter funnels through this boundary so autoloading cannot
/// accidentally become a plain `load`.  GNU's `load_with_autoload_queue`
/// preserves match data around path resolution and file loading, then checks
/// the resulting function cell after that state has been restored.
fn execute_autoload_load(
    eval: &mut super::eval::Context,
    file: &LispString,
    funname: Option<SymId>,
    original_fundef: Option<Value>,
) -> EvalResult {
    let roots = eval.save_specpdl_roots();
    if let Some(fundef) = original_fundef {
        eval.push_specpdl_root(fundef);
    }

    let load_result = with_implicit_load_state(eval, |eval| {
        let path = resolve_autoload_load_path(&eval.obarray, eval.buffers.current_buffer(), file)?;
        // GNU eval.c:Fautoload_do_load calls load_with_autoload_queue with
        // NOMESSAGE=t: autoloading is an implicit consequence of calling a
        // function, not an explicit user request to load a file.
        eval.load_file_internal_with_options(
            &path,
            super::load::LoadOptions::implicit_dependency(super::load::MissingFilePolicy::Signal),
        )
    });
    let result = match load_result {
        Ok(_) => finish_autoload_do_load_in_state(&eval.obarray, funname, original_fundef.as_ref()),
        Err(flow) => Err(flow),
    };

    eval.restore_specpdl_roots(roots);
    result
}

pub(crate) fn builtin_autoload_do_load(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let original_fundef = args.first().copied();
    match plan_autoload_do_load_in_state(&eval.obarray, &args)? {
        AutoloadDoLoadPlan::Return(value) => Ok(value),
        AutoloadDoLoadPlan::Load { file, funname } => {
            execute_autoload_load(eval, &file, funname, original_fundef)
        }
    }
}

pub(crate) fn builtin_autoload_do_load_3(
    eval: &mut super::eval::Context,
    fundef: Value,
    funname: Value,
    macro_only: Value,
) -> EvalResult {
    let args = [fundef, funname, macro_only];
    match plan_autoload_do_load_in_state(&eval.obarray, &args)? {
        AutoloadDoLoadPlan::Return(value) => Ok(value),
        AutoloadDoLoadPlan::Load { file, funname } => {
            execute_autoload_load(eval, &file, funname, Some(fundef))
        }
    }
}

pub(crate) fn builtin_autoload_do_load_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    let original_fundef = args.first().copied();
    match plan_autoload_do_load_in_state(&shared.obarray, args)? {
        AutoloadDoLoadPlan::Return(value) => Ok(value),
        AutoloadDoLoadPlan::Load { file, funname } => {
            execute_autoload_load(shared, &file, funname, original_fundef)
        }
    }
}

enum AutoloadDefinitionPlan {
    KeepExistingDefinition,
    Define {
        symbol: SymId,
        symbol_value: Value,
        definition: Value,
    },
}

fn plan_autoload_definition(
    obarray: &Obarray,
    args: &[Value],
    symbols_with_pos_enabled: bool,
) -> Result<AutoloadDefinitionPlan, Flow> {
    if args.len() < 2 || args.len() > 5 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![Value::symbol("autoload"), Value::fixnum(args.len() as i64)],
        ));
    }

    let func_val = args[0];
    let name = func_val
        .as_symbol_id()
        .or_else(|| {
            if symbols_with_pos_enabled {
                func_val
                    .as_symbol_with_pos_sym()
                    .and_then(|sym| sym.as_symbol_id())
            } else {
                None
            }
        })
        .ok_or_else(|| {
            signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("symbolp"), func_val],
            )
        })?;

    let file_val = args[1];
    if !file_val.is_string() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), file_val],
        ));
    }

    // GNU Emacs eval.c:Fautoload — "If function is defined and not as an
    // autoload, don't override."  If the symbol already has a real (non-
    // autoload) function definition, return nil without touching it.
    if let Some(current) = obarray.symbol_function_id(name)
        && !is_autoload_value(&current)
    {
        return Ok(AutoloadDefinitionPlan::KeepExistingDefinition);
    }

    let docstring_val = args.get(2).cloned().unwrap_or(Value::NIL);
    let interactive_val = args.get(3).cloned().unwrap_or(Value::NIL);
    let type_val = args.get(4).cloned().unwrap_or(Value::NIL);
    let definition = Value::list(vec![
        Value::symbol("autoload"),
        file_val,
        docstring_val,
        interactive_val,
        type_val,
    ]);

    Ok(AutoloadDefinitionPlan::Define {
        symbol: name,
        symbol_value: Value::from_sym_id(name),
        definition,
    })
}

fn autoload_entry_from_function_cell(function: Value) -> Option<AutoloadEntry> {
    if !is_autoload_value(&function) {
        return None;
    }
    let fields = list_to_vec(&function)?;
    let file = fields.get(1)?.as_lisp_string()?.clone();
    let docstring = fields
        .get(2)
        .and_then(|value| value.as_lisp_string())
        .cloned();
    let interactive = fields.get(3).is_some_and(|value| value.is_truthy());
    let autoload_type = fields
        .get(4)
        .map(AutoloadType::from_value)
        .unwrap_or(AutoloadType::Function);
    Some(AutoloadEntry {
        file,
        docstring,
        interactive,
        autoload_type,
    })
}

/// `(autoload FUNCTION FILE &optional DOCSTRING INTERACTIVE TYPE)`
///
/// Callable builtin form used by `funcall`/`apply` and direct function calls.
/// Arguments are already evaluated.
pub(crate) fn builtin_autoload(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    match plan_autoload_definition(&eval.obarray, &args, eval.symbols_with_pos_enabled)? {
        AutoloadDefinitionPlan::KeepExistingDefinition => Ok(Value::NIL),
        AutoloadDefinitionPlan::Define {
            symbol,
            symbol_value,
            definition,
        } => {
            // GNU Fautoload delegates to Fdefalias.  Keep that semantic
            // boundary intact so autoload definitions inherit load-history,
            // function-history, advice hooks, and interactive-registry
            // handling instead of maintaining a parallel definition path.
            let roots = eval.save_specpdl_roots();
            eval.push_specpdl_root(symbol_value);
            eval.push_specpdl_root(definition);
            let result =
                super::builtins::misc_eval::builtin_defalias(eval, vec![symbol_value, definition]);

            if result.is_ok() {
                // AutoloadManager is an acceleration index, not a second
                // source of truth.  Rebuild its entry from the function cell
                // after the defalias hook has run, or remove a stale entry if
                // the hook installed a different kind of definition.
                let entry = eval
                    .obarray
                    .symbol_function_id(symbol)
                    .and_then(autoload_entry_from_function_cell);
                match entry {
                    Some(entry) => eval.autoloads.register_symbol(symbol, entry),
                    None => eval.autoloads.remove_symbol(symbol),
                }
            }
            eval.restore_specpdl_roots(roots);
            result
        }
    }
}

// `symbol-file' is not here.  GNU has no C version: it is
// `(defun symbol-file (symbol &optional type native-p) ...)' at
// lisp/subr.el:3351, which walks `load-history' for every kind of
// definition and, for an autoload, answers `(locate-library (nth 1
// (symbol-function symbol)))'.  The Rust version knew only about autoloads
// and returned the RAW autoload file string, never consulting
// `load-history' at all (DIVERGENCES.md 152).

// ---------------------------------------------------------------------------
// Special form handlers (called from eval.rs try_special_form dispatch)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// GcTrace
// ---------------------------------------------------------------------------

impl GcTrace for AutoloadManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for values in self.after_load.values() {
            for value in values {
                roots.push(*value);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
