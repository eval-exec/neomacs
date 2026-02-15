//! Evaluator — special forms, function application, and dispatch.

use std::collections::HashMap;

use super::abbrev::AbbrevManager;
use super::advice::{AdviceManager, VariableWatcherList};
use super::autoload::AutoloadManager;
use super::bookmark::BookmarkManager;
use super::builtins;
use super::category::CategoryManager;
use super::coding::CodingSystemManager;
use super::custom::CustomManager;
use super::error::*;
use super::expr::Expr;
use super::interactive::InteractiveRegistry;
use super::keymap::KeymapManager;
use super::kill_ring::KillRing;
use super::kmacro::KmacroManager;
use super::mode::ModeRegistry;
use super::network::NetworkManager;
use super::process::ProcessManager;
use super::rect::RectangleState;
use super::regex::MatchData;
use super::register::RegisterManager;
use super::symbol::Obarray;
use super::threads::ThreadManager;
use super::timer::TimerManager;
use super::value::*;
use crate::buffer::BufferManager;
use crate::window::FrameManager;

#[derive(Clone, Debug)]
enum NamedCallTarget {
    Obarray(Value),
    EvaluatorCallable,
    Probe,
    Builtin,
    SpecialForm,
    Void,
}

#[derive(Clone, Debug)]
struct NamedCallCache {
    symbol: String,
    function_epoch: u64,
    target: NamedCallTarget,
}

/// The Elisp evaluator.
pub struct Evaluator {
    /// The obarray — unified symbol table with value cells, function cells, plists.
    pub(crate) obarray: Obarray,
    /// Dynamic binding stack (each frame is one `let`/function call scope).
    pub(crate) dynamic: Vec<HashMap<String, Value>>,
    /// Lexical environment stack (for lexical-binding mode).
    pub(crate) lexenv: Vec<HashMap<String, Value>>,
    /// Features list (for require/provide).
    pub(crate) features: Vec<String>,
    /// Features currently being resolved through `require`.
    require_stack: Vec<String>,
    /// Buffer manager — owns all live buffers and tracks current buffer.
    pub(crate) buffers: BufferManager,
    /// Match data from the last successful search/match operation.
    pub(crate) match_data: Option<MatchData>,
    /// Keymap manager — owns all keymaps.
    pub(crate) keymaps: KeymapManager,
    /// Process manager — owns all tracked processes.
    pub(crate) processes: ProcessManager,
    /// Network manager — owns network connections, filters, and sentinels.
    pub(crate) network: NetworkManager,
    /// Timer manager — owns all timers.
    pub(crate) timers: TimerManager,
    /// Advice manager — function advice (before/after/around/etc.).
    pub(crate) advice: AdviceManager,
    /// Variable watcher list — callbacks on variable changes.
    pub(crate) watchers: VariableWatcherList,
    /// Current buffer-local keymap id (set by `use-local-map`).
    pub(crate) current_local_map: Option<u64>,
    /// Register manager — quick storage and retrieval of text, positions, etc.
    pub(crate) registers: RegisterManager,
    /// Bookmark manager — persistent named positions.
    pub(crate) bookmarks: BookmarkManager,
    /// Abbreviation manager — text abbreviation expansion.
    pub(crate) abbrevs: AbbrevManager,
    /// Autoload manager — deferred function loading.
    pub(crate) autoloads: AutoloadManager,
    /// Custom variable manager — defcustom/defgroup system.
    pub(crate) custom: CustomManager,
    /// Kill ring — clipboard/kill ring for text editing.
    pub(crate) kill_ring: KillRing,
    /// Rectangle state — stores the last killed rectangle for yank-rectangle.
    pub(crate) rectangle: RectangleState,
    /// Interactive command registry — tracks interactive commands.
    pub(crate) interactive: InteractiveRegistry,
    /// Frame manager — owns all frames and windows.
    pub(crate) frames: FrameManager,
    /// Mode registry — major/minor modes.
    pub(crate) modes: ModeRegistry,
    /// Thread manager — cooperative threading primitives.
    pub(crate) threads: ThreadManager,
    /// Category manager — character category tables.
    pub(crate) category_manager: CategoryManager,
    /// Keyboard macro manager — recording, playback, macro ring.
    pub(crate) kmacro: KmacroManager,
    /// Coding system manager — encoding/decoding registry.
    pub(crate) coding_systems: CodingSystemManager,
    /// Recursion depth counter.
    depth: usize,
    /// Maximum recursion depth.
    max_depth: usize,
    /// Single-entry hot cache for named callable resolution in `funcall`/`apply`.
    named_call_cache: Option<NamedCallCache>,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let mut obarray = Obarray::new();
        let default_directory = std::env::current_dir()
            .ok()
            .and_then(|p| p.to_str().map(|s| s.to_string()))
            .map(|mut s| {
                if !s.ends_with('/') {
                    s.push('/');
                }
                s
            })
            .unwrap_or_else(|| "./".to_string());

        // Set up standard global variables
        obarray.set_symbol_value("most-positive-fixnum", Value::Int(i64::MAX));
        obarray.set_symbol_value("most-negative-fixnum", Value::Int(i64::MIN));
        obarray.set_symbol_value("emacs-version", Value::string("29.1"));
        obarray.set_symbol_value("system-type", Value::symbol("gnu/linux"));
        obarray.set_symbol_value("default-directory", Value::string(default_directory));
        obarray.set_symbol_value("load-path", Value::Nil);
        obarray.set_symbol_value("load-history", Value::Nil);
        obarray.set_symbol_value("features", Value::Nil);
        obarray.set_symbol_value("debug-on-error", Value::Nil);
        obarray.set_symbol_value("lexical-binding", Value::Nil);
        obarray.set_symbol_value("load-prefer-newer", Value::Nil);
        obarray.set_symbol_value("load-file-name", Value::Nil);
        obarray.set_symbol_value("noninteractive", Value::True);
        obarray.set_symbol_value("inhibit-quit", Value::Nil);
        obarray.set_symbol_value("print-length", Value::Nil);
        obarray.set_symbol_value("print-level", Value::Nil);
        obarray.set_symbol_value("standard-output", Value::True);
        obarray.set_symbol_value("buffer-read-only", Value::Nil);

        // Mark standard variables as special (dynamically bound)
        for name in &[
            "debug-on-error",
            "lexical-binding",
            "load-prefer-newer",
            "load-path",
            "load-history",
            "features",
            "default-directory",
            "load-file-name",
            "noninteractive",
            "inhibit-quit",
            "print-length",
            "print-level",
            "standard-output",
            "buffer-read-only",
        ] {
            obarray.make_special(name);
        }

        // Initialize the standard error hierarchy (error, user-error, etc.)
        super::errors::init_standard_errors(&mut obarray);

        // Initialize indentation variables (tab-width, indent-tabs-mode, etc.)
        super::indent::init_indent_vars(&mut obarray);

        let mut custom = CustomManager::new();
        custom.make_variable_buffer_local("buffer-read-only");

        Self {
            obarray,
            dynamic: Vec::new(),
            lexenv: Vec::new(),
            features: Vec::new(),
            require_stack: Vec::new(),
            buffers: BufferManager::new(),
            match_data: None,
            keymaps: KeymapManager::new(),
            processes: ProcessManager::new(),
            network: NetworkManager::new(),
            timers: TimerManager::new(),
            advice: AdviceManager::new(),
            watchers: VariableWatcherList::new(),
            current_local_map: None,
            registers: RegisterManager::new(),
            bookmarks: BookmarkManager::new(),
            abbrevs: AbbrevManager::new(),
            autoloads: AutoloadManager::new(),
            custom,
            kill_ring: KillRing::new(),
            rectangle: RectangleState::new(),
            interactive: InteractiveRegistry::new(),
            frames: FrameManager::new(),
            modes: ModeRegistry::new(),
            threads: ThreadManager::new(),
            category_manager: CategoryManager::new(),
            kmacro: KmacroManager::new(),
            coding_systems: CodingSystemManager::new(),
            depth: 0,
            max_depth: 200,
            named_call_cache: None,
        }
    }

    /// Whether lexical-binding is currently enabled.
    pub fn lexical_binding(&self) -> bool {
        self.obarray
            .symbol_value("lexical-binding")
            .is_some_and(|v| v.is_truthy())
    }

    /// Enable or disable lexical binding.
    pub fn set_lexical_binding(&mut self, enabled: bool) {
        self.obarray
            .set_symbol_value("lexical-binding", Value::bool(enabled));
    }

    /// Load a file, converting EvalError back to Flow for use in special forms.
    pub(crate) fn load_file_internal(&mut self, path: &std::path::Path) -> EvalResult {
        super::load::load_file(self, path).map_err(|e| match e {
            EvalError::Signal { symbol, data } => signal(&symbol, data),
            EvalError::UncaughtThrow { tag, value } => Flow::Throw { tag, value },
        })
    }

    /// Keep the Lisp-visible `features` variable in sync with the evaluator's
    /// internal feature set.
    fn sync_features_variable(&mut self) {
        let values: Vec<Value> = self
            .features
            .iter()
            .map(|name| Value::symbol(name.clone()))
            .collect();
        self.obarray.set_symbol_value("features", Value::list(values));
    }

    fn refresh_features_from_variable(&mut self) {
        let current = self
            .obarray
            .symbol_value("features")
            .cloned()
            .unwrap_or(Value::Nil);
        let mut parsed = Vec::new();
        if let Some(items) = list_to_vec(&current) {
            for item in items {
                if let Some(name) = item.as_symbol_name() {
                    parsed.push(name.to_string());
                }
            }
        }
        self.features = parsed;
    }

    fn has_feature(&mut self, name: &str) -> bool {
        self.refresh_features_from_variable();
        self.features.iter().any(|f| f == name)
    }

    fn add_feature(&mut self, name: &str) {
        self.refresh_features_from_variable();
        if self.features.iter().any(|f| f == name) {
            return;
        }
        // Emacs pushes newly-provided features at the front.
        self.features.insert(0, name.to_string());
        self.sync_features_variable();
    }

    pub(crate) fn feature_present(&mut self, name: &str) -> bool {
        self.has_feature(name)
    }

    /// Access the obarray (for builtins that need it).
    pub fn obarray(&self) -> &Obarray {
        &self.obarray
    }

    /// Access the obarray mutably.
    pub fn obarray_mut(&mut self) -> &mut Obarray {
        &mut self.obarray
    }

    // -----------------------------------------------------------------------
    // Public API
    // -----------------------------------------------------------------------

    pub fn eval_expr(&mut self, expr: &Expr) -> Result<Value, EvalError> {
        self.eval(expr).map_err(map_flow)
    }

    pub fn eval_forms(&mut self, forms: &[Expr]) -> Vec<Result<Value, EvalError>> {
        forms.iter().map(|form| self.eval_expr(form)).collect()
    }

    /// Set a global variable.
    pub fn set_variable(&mut self, name: &str, value: Value) {
        self.obarray.set_symbol_value(name, value);
    }

    /// Set a function binding.
    pub fn set_function(&mut self, name: &str, value: Value) {
        self.obarray.set_symbol_function(name, value);
    }

    // -----------------------------------------------------------------------
    // Core eval
    // -----------------------------------------------------------------------

    pub(crate) fn eval(&mut self, expr: &Expr) -> EvalResult {
        self.depth += 1;
        if self.depth > self.max_depth {
            self.depth -= 1;
            return Err(signal(
                "excessive-lisp-nesting",
                vec![Value::Int(self.max_depth as i64)],
            ));
        }
        let result = self.eval_inner(expr);
        self.depth -= 1;
        result
    }

    fn eval_inner(&mut self, expr: &Expr) -> EvalResult {
        match expr {
            Expr::Int(v) => Ok(Value::Int(*v)),
            Expr::Float(v) => Ok(Value::Float(*v)),
            Expr::Str(s) => Ok(Value::string(s.clone())),
            Expr::Char(c) => Ok(Value::Char(*c)),
            Expr::Keyword(s) => Ok(Value::Keyword(s.clone())),
            Expr::Bool(true) => Ok(Value::True),
            Expr::Bool(false) => Ok(Value::Nil),
            Expr::Vector(items) => {
                let mut vals = Vec::with_capacity(items.len());
                for item in items {
                    vals.push(self.eval(item)?);
                }
                Ok(Value::vector(vals))
            }
            Expr::Symbol(symbol) => self.eval_symbol(symbol),
            Expr::List(items) => self.eval_list(items),
            Expr::DottedList(items, last) => {
                // Evaluate as a list call, ignoring dotted cdr
                // (This is for `(func a b . rest)` style, which in practice
                //  means the dotted pair is rarely used in function calls)
                let _ = last;
                self.eval_list(items)
            }
        }
    }

    fn eval_symbol(&self, symbol: &str) -> EvalResult {
        if symbol == "nil" {
            return Ok(Value::Nil);
        }
        if symbol == "t" {
            return Ok(Value::True);
        }
        // Keywords evaluate to themselves
        if symbol.starts_with(':') {
            return Ok(Value::Keyword(symbol.to_string()));
        }

        // If lexical binding is on and symbol is NOT special, check lexenv first
        if self.lexical_binding() && !self.obarray.is_special(symbol) {
            for frame in self.lexenv.iter().rev() {
                if let Some(value) = frame.get(symbol) {
                    return Ok(value.clone());
                }
            }
        }

        // Dynamic scope lookup (inner to outer)
        for frame in self.dynamic.iter().rev() {
            if let Some(value) = frame.get(symbol) {
                return Ok(value.clone());
            }
        }

        // Buffer-local binding on current buffer.
        if let Some(buf) = self.buffers.current_buffer() {
            if let Some(value) = buf.get_buffer_local(symbol) {
                return Ok(value.clone());
            }
        }

        // Obarray value cell
        if let Some(value) = self.obarray.symbol_value(symbol) {
            return Ok(value.clone());
        }

        Err(signal("void-variable", vec![Value::symbol(symbol)]))
    }

    fn eval_list(&mut self, items: &[Expr]) -> EvalResult {
        let Some((head, tail)) = items.split_first() else {
            return Ok(Value::Nil);
        };

        if let Expr::Symbol(name) = head {
            // Check for macro expansion first (from obarray function cell)
            if let Some(func) = self.obarray.symbol_function(name).cloned() {
                if func.is_nil() {
                    return Err(signal("void-function", vec![Value::symbol(name)]));
                }
                if let Value::Macro(_) = &func {
                    let expanded = self.expand_macro(func, tail)?;
                    return self.eval(&expanded);
                }

                if let Value::Subr(bound_name) = &func {
                    if bound_name == name && super::subr_info::is_special_form(name) {
                        if let Some(result) = self.try_special_form(name, tail) {
                            return result;
                        }
                    }
                }

                // Explicit function-cell bindings override special-form fallback.
                let mut args = Vec::with_capacity(tail.len());
                for expr in tail {
                    args.push(self.eval(expr)?);
                }
                return match self.apply(func, args) {
                    Err(Flow::Signal(sig)) if sig.symbol == "invalid-function" => {
                        Err(signal("invalid-function", vec![Value::symbol(name)]))
                    }
                    other => other,
                };
            }

            // Special forms
            if !self.obarray.is_function_unbound(name) {
                if let Some(result) = self.try_special_form(name, tail) {
                    return result;
                }
            }

            // Regular function call — evaluate args then dispatch
            let mut args = Vec::with_capacity(tail.len());
            for expr in tail {
                args.push(self.eval(expr)?);
            }

            return self.apply_named_callable(name, args, Value::Subr(name.clone()), false);
        }

        // Head is a list (possibly a lambda expression)
        if let Expr::List(lambda_form) = head {
            if let Some(Expr::Symbol(s)) = lambda_form.first() {
                if s == "lambda" {
                    let func = self.eval_lambda(&lambda_form[1..])?;
                    let mut args = Vec::with_capacity(tail.len());
                    for expr in tail {
                        args.push(self.eval(expr)?);
                    }
                    return self.apply(func, args);
                }
            }
        }

        Err(signal("invalid-function", vec![quote_to_value(head)]))
    }

    // -----------------------------------------------------------------------
    // Special forms
    // -----------------------------------------------------------------------

    fn try_special_form(&mut self, name: &str, tail: &[Expr]) -> Option<EvalResult> {
        Some(match name {
            "quote" => self.sf_quote(tail),
            "function" => self.sf_function(tail),
            "let" => self.sf_let(tail),
            "let*" => self.sf_let_star(tail),
            "setq" => self.sf_setq(tail),
            "setq-local" => self.sf_setq_local(tail),
            "if" => self.sf_if(tail),
            "and" => self.sf_and(tail),
            "or" => self.sf_or(tail),
            "cond" => self.sf_cond(tail),
            "while" => self.sf_while(tail),
            "progn" => self.sf_progn(tail),
            "prog1" => self.sf_prog1(tail),
            "lambda" => self.eval_lambda(tail),
            "defun" => self.sf_defun(tail),
            "defvar" => self.sf_defvar(tail),
            "defconst" => self.sf_defconst(tail),
            "defmacro" => self.sf_defmacro(tail),
            "funcall" => self.sf_funcall(tail),
            "catch" => self.sf_catch(tail),
            "throw" => self.sf_throw(tail),
            "unwind-protect" => self.sf_unwind_protect(tail),
            "condition-case" => self.sf_condition_case(tail),
            "byte-code-literal" => self.sf_byte_code_literal(tail),
            "interactive" => Ok(Value::Nil), // Stub: ignored for now
            "declare" => Ok(Value::Nil),     // Stub: ignored for now
            "when" => self.sf_when(tail),
            "unless" => self.sf_unless(tail),
            "defalias" => self.sf_defalias(tail),
            "provide" => self.sf_provide(tail),
            "require" => self.sf_require(tail),
            "save-excursion" => self.sf_save_excursion(tail),
            "save-restriction" => self.sf_save_restriction(tail),
            "with-current-buffer" => self.sf_with_current_buffer(tail),
            "ignore-errors" => self.sf_ignore_errors(tail),
            "dotimes" => self.sf_dotimes(tail),
            "dolist" => self.sf_dolist(tail),
            // Custom system special forms
            "defcustom" => super::custom::sf_defcustom(self, tail),
            "defgroup" => super::custom::sf_defgroup(self, tail),
            "setq-default" => super::custom::sf_setq_default(self, tail),
            "defvar-local" => super::custom::sf_defvar_local(self, tail),
            // Autoload special forms
            "autoload" => super::autoload::sf_autoload(self, tail),
            "eval-when-compile" => super::autoload::sf_eval_when_compile(self, tail),
            "eval-and-compile" => super::autoload::sf_eval_and_compile(self, tail),
            "declare-function" => super::autoload::sf_declare_function(self, tail),
            "define-obsolete-function-alias" => {
                super::autoload::sf_define_obsolete_function_alias(self, tail)
            }
            "define-obsolete-variable-alias" => {
                super::autoload::sf_define_obsolete_variable_alias(self, tail)
            }
            "make-obsolete" => super::autoload::sf_make_obsolete(self, tail),
            "make-obsolete-variable" => super::autoload::sf_make_obsolete_variable(self, tail),
            "with-eval-after-load" => super::autoload::sf_with_eval_after_load(self, tail),
            // Error hierarchy
            "define-error" => super::errors::sf_define_error(self, tail),
            // Pattern matching (pcase)
            "pcase" => super::pcase::sf_pcase(self, tail),
            "pcase-let" => super::pcase::sf_pcase_let(self, tail),
            "pcase-let*" => super::pcase::sf_pcase_let_star(self, tail),
            "pcase-dolist" => super::pcase::sf_pcase_dolist(self, tail),
            // Generalized variables (setf)
            "setf" => super::setf::sf_setf(self, tail),
            "push" => super::setf::sf_push(self, tail),
            "pop" => super::setf::sf_pop(self, tail),
            "cl-incf" => super::setf::sf_cl_incf(self, tail),
            "cl-decf" => super::setf::sf_cl_decf(self, tail),
            "gv-define-simple-setter" => super::setf::sf_gv_define_simple_setter(self, tail),
            "gv-define-setter" => super::setf::sf_gv_define_setter(self, tail),
            // CL extended special forms
            "cl-defstruct" => super::cl_extra::sf_cl_defstruct(self, tail),
            "cl-loop" => super::cl_extra::sf_cl_loop(self, tail),
            "cl-destructuring-bind" => super::cl_extra::sf_cl_destructuring_bind(self, tail),
            "cl-push" => super::cl_extra::sf_cl_push(self, tail),
            "cl-pop" => super::cl_extra::sf_cl_pop(self, tail),
            "cl-pushnew" => super::cl_extra::sf_cl_pushnew(self, tail),
            "cl-assert" => super::cl_extra::sf_cl_assert(self, tail),
            "cl-check-type" => super::cl_extra::sf_cl_check_type(self, tail),
            "cl-case" => super::cl_extra::sf_cl_case(self, tail),
            "cl-ecase" => super::cl_extra::sf_cl_ecase(self, tail),
            "cl-typecase" => super::cl_extra::sf_cl_typecase(self, tail),
            "cl-etypecase" => super::cl_extra::sf_cl_etypecase(self, tail),
            "cl-block" => super::cl_extra::sf_cl_block(self, tail),
            "cl-return-from" => super::cl_extra::sf_cl_return_from(self, tail),
            "cl-dotimes" => super::cl_extra::sf_cl_dotimes(self, tail),
            "cl-dolist" => super::cl_extra::sf_cl_dolist(self, tail),
            "cl-flet" => super::cl_extra::sf_cl_flet(self, tail),
            "cl-labels" => super::cl_extra::sf_cl_labels(self, tail),
            "cl-progv" => super::cl_extra::sf_cl_progv(self, tail),
            // Reader/printer special forms
            "with-output-to-string" => super::reader::sf_with_output_to_string(self, tail),
            // Threading
            "with-mutex" => super::threads::sf_with_mutex(self, tail),
            // Misc special forms
            "prog2" => super::misc::sf_prog2(self, tail),
            "with-temp-buffer" => super::misc::sf_with_temp_buffer(self, tail),
            "save-current-buffer" => super::misc::sf_save_current_buffer(self, tail),
            "track-mouse" => super::misc::sf_track_mouse(self, tail),
            "with-syntax-table" => super::misc::sf_with_syntax_table(self, tail),
            // Interactive / mode definition special forms
            "define-minor-mode" => super::interactive::sf_define_minor_mode(self, tail),
            "define-derived-mode" => super::interactive::sf_define_derived_mode(self, tail),
            "define-generic-mode" => super::interactive::sf_define_generic_mode(self, tail),
            _ => return None,
        })
    }

    fn sf_quote(&self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("quote"), Value::Int(tail.len() as i64)],
            ));
        }
        Ok(quote_to_value(&tail[0]))
    }

    fn sf_function(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("function"), Value::Int(tail.len() as i64)],
            ));
        }
        match &tail[0] {
            Expr::List(items) => {
                // #'(lambda ...) — create closure
                if let Some(Expr::Symbol(s)) = items.first() {
                    if s == "lambda" {
                        return self.eval_lambda(&items[1..]);
                    }
                }
                Ok(quote_to_value(&tail[0]))
            }
            _ => Ok(quote_to_value(&tail[0])),
        }
    }

    fn sf_let(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("let"), Value::Int(tail.len() as i64)],
            ));
        }

        let mut lexical_bindings = HashMap::new();
        let mut dynamic_bindings = HashMap::new();
        let use_lexical = self.lexical_binding();

        match &tail[0] {
            Expr::List(entries) => {
                for binding in entries {
                    match binding {
                        Expr::Symbol(name) => {
                            if use_lexical && !self.obarray.is_special(name) {
                                lexical_bindings.insert(name.clone(), Value::Nil);
                            } else {
                                dynamic_bindings.insert(name.clone(), Value::Nil);
                            }
                        }
                        Expr::List(pair) if !pair.is_empty() => {
                            let Expr::Symbol(name) = &pair[0] else {
                                return Err(signal(
                                    "wrong-type-argument",
                                    vec![Value::symbol("symbolp"), quote_to_value(&pair[0])],
                                ));
                            };
                            let value = if pair.len() > 1 {
                                self.eval(&pair[1])?
                            } else {
                                Value::Nil
                            };
                            if use_lexical && !self.obarray.is_special(name) {
                                lexical_bindings.insert(name.clone(), value);
                            } else {
                                dynamic_bindings.insert(name.clone(), value);
                            }
                        }
                        _ => return Err(signal("wrong-type-argument", vec![])),
                    }
                }
            }
            Expr::Symbol(s) if s == "nil" => {} // (let nil ...)
            Expr::DottedList(_, last) => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), quote_to_value(last)],
                ))
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), quote_to_value(other)],
                ))
            }
        }

        let pushed_lex = !lexical_bindings.is_empty();
        let pushed_dyn = !dynamic_bindings.is_empty();
        if pushed_lex {
            self.lexenv.push(lexical_bindings);
        }
        if pushed_dyn {
            self.dynamic.push(dynamic_bindings);
        }
        let result = self.sf_progn(&tail[1..]);
        if pushed_dyn {
            self.dynamic.pop();
        }
        if pushed_lex {
            self.lexenv.pop();
        }
        result
    }

    fn sf_let_star(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("let*"), Value::Int(tail.len() as i64)],
            ));
        }

        let entries = match &tail[0] {
            Expr::List(entries) => entries.clone(),
            Expr::Symbol(s) if s == "nil" => Vec::new(),
            Expr::DottedList(_, last) => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), quote_to_value(last)],
                ))
            }
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), quote_to_value(other)],
                ))
            }
        };

        let use_lexical = self.lexical_binding();
        let pushed_lex = use_lexical; // Always push a frame for let* in lexical mode
        let pushed_dyn = true; // Always push a dynamic frame too (for special vars or dynamic mode)

        self.dynamic.push(HashMap::new());
        if use_lexical {
            self.lexenv.push(HashMap::new());
        }

        for binding in &entries {
            match binding {
                Expr::Symbol(name) => {
                    if use_lexical && !self.obarray.is_special(name) {
                        if let Some(frame) = self.lexenv.last_mut() {
                            frame.insert(name.clone(), Value::Nil);
                        }
                    } else if let Some(frame) = self.dynamic.last_mut() {
                        frame.insert(name.clone(), Value::Nil);
                    }
                }
                Expr::List(pair) if !pair.is_empty() => {
                    let Expr::Symbol(name) = &pair[0] else {
                        if pushed_lex {
                            self.lexenv.pop();
                        }
                        self.dynamic.pop();
                        return Err(signal(
                            "wrong-type-argument",
                            vec![Value::symbol("symbolp"), quote_to_value(&pair[0])],
                        ));
                    };
                    let value = if pair.len() > 1 {
                        match self.eval(&pair[1]) {
                            Ok(v) => v,
                            Err(e) => {
                                if pushed_lex {
                                    self.lexenv.pop();
                                }
                                self.dynamic.pop();
                                return Err(e);
                            }
                        }
                    } else {
                        Value::Nil
                    };
                    if use_lexical && !self.obarray.is_special(name) {
                        if let Some(frame) = self.lexenv.last_mut() {
                            frame.insert(name.clone(), value);
                        }
                    } else if let Some(frame) = self.dynamic.last_mut() {
                        frame.insert(name.clone(), value);
                    }
                }
                _ => {
                    if pushed_lex {
                        self.lexenv.pop();
                    }
                    self.dynamic.pop();
                    return Err(signal("wrong-type-argument", vec![]));
                }
            }
        }

        let result = self.sf_progn(&tail[1..]);
        if pushed_dyn {
            self.dynamic.pop();
        }
        if pushed_lex {
            self.lexenv.pop();
        }
        result
    }

    fn sf_setq(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Ok(Value::Nil);
        }
        if tail.len() % 2 != 0 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("setq"), Value::Int(tail.len() as i64)],
            ));
        }

        let mut last = Value::Nil;
        let mut i = 0;
        while i < tail.len() {
            let Expr::Symbol(name) = &tail[i] else {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), quote_to_value(&tail[i])],
                ));
            };
            let value = self.eval(&tail[i + 1])?;
            self.assign(name, value.clone());
            last = value;
            i += 2;
        }
        Ok(last)
    }

    fn sf_setq_local(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Ok(Value::Nil);
        }
        if tail.len() % 2 != 0 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::symbol("setq-local"),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }

        let mut last = Value::Nil;
        let mut i = 0;
        while i < tail.len() {
            let Expr::Symbol(name) = &tail[i] else {
                return Err(signal("wrong-type-argument", vec![]));
            };

            if name == "nil" || name == "t" {
                return Err(signal("setting-constant", vec![Value::symbol(name)]));
            }

            let value = self.eval(&tail[i + 1])?;
            if let Some(buf) = self.buffers.current_buffer_mut() {
                buf.set_buffer_local(name, value.clone());
            } else {
                self.assign(name, value.clone());
            }
            last = value;
            i += 2;
        }
        Ok(last)
    }

    fn sf_if(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("if"), Value::Int(tail.len() as i64)],
            ));
        }
        let cond = self.eval(&tail[0])?;
        if cond.is_truthy() {
            self.eval(&tail[1])
        } else {
            self.sf_progn(&tail[2..])
        }
    }

    fn sf_and(&mut self, tail: &[Expr]) -> EvalResult {
        let mut last = Value::True;
        for expr in tail {
            last = self.eval(expr)?;
            if last.is_nil() {
                return Ok(Value::Nil);
            }
        }
        Ok(last)
    }

    fn sf_or(&mut self, tail: &[Expr]) -> EvalResult {
        for expr in tail {
            let val = self.eval(expr)?;
            if val.is_truthy() {
                return Ok(val);
            }
        }
        Ok(Value::Nil)
    }

    fn sf_cond(&mut self, tail: &[Expr]) -> EvalResult {
        for clause in tail {
            let Expr::List(items) = clause else {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("listp"), quote_to_value(clause)],
                ));
            };
            if items.is_empty() {
                continue;
            }
            let test = self.eval(&items[0])?;
            if test.is_truthy() {
                if items.len() == 1 {
                    return Ok(test);
                }
                return self.sf_progn(&items[1..]);
            }
        }
        Ok(Value::Nil)
    }

    fn sf_while(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("while"), Value::Int(tail.len() as i64)],
            ));
        }
        loop {
            let cond = self.eval(&tail[0])?;
            if cond.is_nil() {
                return Ok(Value::Nil);
            }
            self.sf_progn(&tail[1..])?;
        }
    }

    pub(crate) fn sf_progn(&mut self, forms: &[Expr]) -> EvalResult {
        let mut last = Value::Nil;
        for form in forms {
            last = self.eval(form)?;
        }
        Ok(last)
    }

    fn sf_prog1(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("prog1"), Value::Int(tail.len() as i64)],
            ));
        }
        let first = self.eval(&tail[0])?;
        for form in &tail[1..] {
            self.eval(form)?;
        }
        Ok(first)
    }

    fn sf_when(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::cons(Value::Int(1), Value::Int(1)),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }
        let cond = self.eval(&tail[0])?;
        if cond.is_truthy() {
            self.sf_progn(&tail[1..])
        } else {
            Ok(Value::Nil)
        }
    }

    fn sf_unless(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::cons(Value::Int(1), Value::Int(1)),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }
        let cond = self.eval(&tail[0])?;
        if cond.is_nil() {
            self.sf_progn(&tail[1..])
        } else {
            Ok(Value::Nil)
        }
    }

    fn sf_defun(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::cons(Value::Int(2), Value::Int(2)),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), quote_to_value(&tail[0])],
            ));
        };
        let lambda = self.eval_lambda(&tail[1..])?;
        self.obarray.set_symbol_function(name, lambda);
        Ok(Value::symbol(name.clone()))
    }

    fn sf_defvar(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("defvar"), Value::Int(tail.len() as i64)],
            ));
        }
        if tail.len() > 3 {
            return Err(signal(
                "error",
                vec![Value::string("Too many arguments")],
            ));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), quote_to_value(&tail[0])],
            ));
        };
        // Only set if not already bound. defvar always marks as special.
        if !self.obarray.boundp(name) {
            let value = if tail.len() > 1 {
                self.eval(&tail[1])?
            } else {
                Value::Nil
            };
            self.obarray.set_symbol_value(name, value);
        }
        self.obarray.make_special(name);
        Ok(Value::symbol(name.clone()))
    }

    fn sf_defconst(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("defconst"), Value::Int(tail.len() as i64)],
            ));
        }
        if tail.len() > 3 {
            return Err(signal(
                "error",
                vec![Value::string("Too many arguments")],
            ));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), quote_to_value(&tail[0])],
            ));
        };
        let value = self.eval(&tail[1])?;
        self.obarray.set_symbol_value(name, value);
        let sym = self.obarray.get_or_intern(name);
        sym.constant = true;
        sym.special = true;
        Ok(Value::symbol(name.clone()))
    }

    fn sf_defmacro(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::cons(Value::Int(2), Value::Int(2)),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), quote_to_value(&tail[0])],
            ));
        };
        let params = self.parse_lambda_params(&tail[1])?;
        let (docstring, body_start) = match tail.get(2) {
            Some(Expr::Str(s)) => (Some(s.clone()), 3),
            _ => (None, 2),
        };
        let body = tail[body_start..].to_vec();
        let macro_val = Value::Macro(std::sync::Arc::new(LambdaData {
            params,
            body,
            env: None,
            docstring,
        }));
        self.obarray.set_symbol_function(name, macro_val);
        Ok(Value::symbol(name.clone()))
    }

    fn sf_funcall(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let function = self.eval(&tail[0])?;
        let mut args = Vec::with_capacity(tail.len().saturating_sub(1));
        for expr in &tail[1..] {
            args.push(self.eval(expr)?);
        }
        self.apply(function, args)
    }

    fn sf_catch(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("catch"), Value::Int(tail.len() as i64)],
            ));
        }
        let tag = self.eval(&tail[0])?;
        match self.sf_progn(&tail[1..]) {
            Ok(value) => Ok(value),
            Err(Flow::Throw {
                tag: thrown_tag,
                value,
            }) if eq_value(&tag, &thrown_tag) => Ok(value),
            Err(flow) => Err(flow),
        }
    }

    fn sf_throw(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 2 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("throw"), Value::Int(tail.len() as i64)],
            ));
        }
        let tag = self.eval(&tail[0])?;
        let value = self.eval(&tail[1])?;
        Err(Flow::Throw { tag, value })
    }

    fn sf_unwind_protect(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("unwind-protect"), Value::Int(tail.len() as i64)],
            ));
        }
        let primary = self.eval(&tail[0]);
        let cleanup = self.sf_progn(&tail[1..]);
        match cleanup {
            Ok(_) => primary,
            Err(flow) => Err(flow),
        }
    }

    fn sf_condition_case(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 3 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::symbol("condition-case"),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }

        let var = match &tail[0] {
            Expr::Symbol(name) => name.clone(),
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), quote_to_value(other)],
                ))
            }
        };
        let body = &tail[1];
        let handlers = &tail[2..];

        // Emacs validates handler shape even when BODY exits normally.
        for handler in handlers {
            match handler {
                Expr::List(_) => {}
                Expr::Symbol(name) if name == "nil" => {}
                _ => {
                    return Err(signal(
                        "error",
                        vec![Value::string(format!(
                            "Invalid condition handler: {}",
                            super::expr::print_expr(handler)
                        ))],
                    ))
                }
            }
        }

        match self.eval(body) {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => {
                for handler in handlers {
                    if matches!(handler, Expr::Symbol(name) if name == "nil") {
                        continue;
                    }
                    let Expr::List(handler_items) = handler else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    if handler_items.is_empty() {
                        continue;
                    }

                    if signal_matches(&handler_items[0], &sig.symbol) {
                        let mut frame = HashMap::new();
                        if var != "nil" {
                            frame.insert(var.clone(), make_signal_binding_value(&sig));
                        }
                        self.dynamic.push(frame);
                        let result = self.sf_progn(&handler_items[1..]);
                        self.dynamic.pop();
                        return result;
                    }
                }
                Err(Flow::Signal(sig))
            }
            Err(Flow::Throw { tag, value }) => {
                let no_catch = SignalData {
                    symbol: "no-catch".to_string(),
                    data: vec![tag.clone(), value.clone()],
                    raw_data: None,
                };

                for handler in handlers {
                    if matches!(handler, Expr::Symbol(name) if name == "nil") {
                        continue;
                    }
                    let Expr::List(handler_items) = handler else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    if handler_items.is_empty() {
                        continue;
                    }

                    if signal_matches(&handler_items[0], &no_catch.symbol) {
                        let mut frame = HashMap::new();
                        if var != "nil" {
                            frame.insert(var.clone(), make_signal_binding_value(&no_catch));
                        }
                        self.dynamic.push(frame);
                        let result = self.sf_progn(&handler_items[1..]);
                        self.dynamic.pop();
                        return result;
                    }
                }

                Err(Flow::Throw { tag, value })
            }
        }
    }

    fn sf_byte_code_literal(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![
                    Value::symbol("byte-code-literal"),
                    Value::Int(tail.len() as i64),
                ],
            ));
        }

        let Expr::Vector(items) = &tail[0] else {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("vectorp"), quote_to_value(&tail[0])],
            ));
        };

        let values = items.iter().map(quote_to_value).collect::<Vec<_>>();
        Ok(super::compiled_literal::maybe_coerce_compiled_literal_function(Value::vector(values)))
    }

    fn sf_defalias(&mut self, tail: &[Expr]) -> EvalResult {
        if !(2..=3).contains(&tail.len()) {
            return Err(signal(
                "wrong-number-of-arguments",
                vec![Value::symbol("defalias"), Value::Int(tail.len() as i64)],
            ));
        }
        let sym = self.eval(&tail[0])?;
        let def = super::compiled_literal::maybe_coerce_compiled_literal_function(
            self.eval(&tail[1])?,
        );
        let name = sym
            .as_symbol_name()
            .map(str::to_string)
            .ok_or_else(|| {
                signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), sym.clone()],
                )
            })?;
        if name == "nil" {
            return Err(signal("setting-constant", vec![Value::symbol("nil")]));
        }
        if builtins::would_create_function_alias_cycle(self, &name, &def) {
            return Err(signal(
                "cyclic-function-indirection",
                vec![Value::symbol(name.clone())],
            ));
        }
        self.obarray.set_symbol_function(&name, def);
        Ok(sym)
    }

    fn sf_provide(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let feature = self.eval(&tail[0])?;
        let name = match &feature {
            Value::Symbol(s) => s.clone(),
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), feature],
                ))
            }
        };
        self.add_feature(&name);
        Ok(feature)
    }

    fn sf_require(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let feature = self.eval(&tail[0])?;
        let name = match &feature {
            Value::Symbol(s) => s.clone(),
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("symbolp"), feature],
                ))
            }
        };
        if self.has_feature(&name) {
            return Ok(Value::symbol(name));
        }

        if self.require_stack.iter().any(|feature| feature == &name) {
            return Err(signal(
                "error",
                vec![Value::string(format!(
                    "Recursive require for feature '{}'",
                    name
                ))],
            ));
        }
        self.require_stack.push(name.clone());

        // Try to find and load the file
        let result = (|| -> EvalResult {
            let filename = if tail.len() > 1 {
                match &self.eval(&tail[1])? {
                    Value::Str(s) => (**s).clone(),
                    _ => name.clone(),
                }
            } else {
                name.clone()
            };

            let load_path = super::load::get_load_path(&self.obarray);
            match super::load::find_file_in_load_path(&filename, &load_path) {
                Some(path) => {
                    self.load_file_internal(&path)?;
                    // After loading, check if feature was provided
                    if self.has_feature(&name) {
                        Ok(Value::symbol(name))
                    } else {
                        Err(signal(
                            "error",
                            vec![Value::string(format!(
                                "Required feature '{}' was not provided",
                                name
                            ))],
                        ))
                    }
                }
                None => {
                    // Check if no-error flag is set (3rd argument)
                    if tail.len() > 2 {
                        let noerror = self.eval(&tail[2])?;
                        if noerror.is_truthy() {
                            return Ok(Value::Nil);
                        }
                    }
                    Err(signal(
                        "file-missing",
                        vec![Value::string(format!(
                            "Cannot open load file: no such file or directory, {}",
                            name
                        ))],
                    ))
                }
            }
        })();
        let _ = self.require_stack.pop();
        result
    }

    fn sf_with_current_buffer(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let buf_val = self.eval(&tail[0])?;
        let target_id = match &buf_val {
            Value::Buffer(id) => *id,
            Value::Str(s) => self.buffers.find_buffer_by_name(s).ok_or_else(|| {
                signal("error", vec![Value::string(format!("No buffer named {s}"))])
            })?,
            other => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("bufferp"), other.clone()],
                ))
            }
        };
        // Save current buffer, switch, run body, restore
        let saved = self.buffers.current_buffer().map(|b| b.id);
        self.buffers.set_current(target_id);
        let result = self.sf_progn(&tail[1..]);
        if let Some(saved_id) = saved {
            self.buffers.set_current(saved_id);
        }
        result
    }

    fn sf_save_excursion(&mut self, tail: &[Expr]) -> EvalResult {
        // Save current buffer, point, and mark; restore after body
        let saved_buf = self.buffers.current_buffer().map(|b| b.id);
        let (saved_pt, saved_mark) = match self.buffers.current_buffer() {
            Some(b) => (b.pt, b.mark),
            None => (0, None),
        };
        let result = self.sf_progn(tail);
        // Restore
        if let Some(buf_id) = saved_buf {
            self.buffers.set_current(buf_id);
            if let Some(buf) = self.buffers.get_mut(buf_id) {
                buf.pt = saved_pt;
                buf.mark = saved_mark;
            }
        }
        result
    }

    fn sf_save_restriction(&mut self, tail: &[Expr]) -> EvalResult {
        // Save narrowing boundaries; restore after body
        let (saved_begv, saved_zv) = match self.buffers.current_buffer() {
            Some(b) => (b.begv, b.zv),
            None => (0, 0),
        };
        let result = self.sf_progn(tail);
        if let Some(buf) = self.buffers.current_buffer_mut() {
            buf.begv = saved_begv;
            buf.zv = saved_zv;
            buf.pt = buf.pt.clamp(buf.begv, buf.zv);
        }
        result
    }

    fn sf_ignore_errors(&mut self, tail: &[Expr]) -> EvalResult {
        match self.sf_progn(tail) {
            Ok(val) => Ok(val),
            Err(Flow::Signal(_)) => Ok(Value::Nil),
            Err(flow) => Err(flow),
        }
    }

    fn sf_dotimes(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::List(spec) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if spec.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(var) = &spec[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        let count = self.eval(&spec[1])?;
        let count = match &count {
            Value::Int(n) => *n,
            _ => {
                return Err(signal(
                    "wrong-type-argument",
                    vec![Value::symbol("integerp"), count],
                ))
            }
        };

        self.dynamic.push(HashMap::new());
        for i in 0..count {
            if let Some(frame) = self.dynamic.last_mut() {
                frame.insert(var.clone(), Value::Int(i));
            }
            self.sf_progn(&tail[1..])?;
        }
        // Result value (third element of spec, or nil)
        let result = if spec.len() > 2 {
            if let Some(frame) = self.dynamic.last_mut() {
                frame.insert(var.clone(), Value::Int(count));
            }
            self.eval(&spec[2])?
        } else {
            Value::Nil
        };
        self.dynamic.pop();
        Ok(result)
    }

    fn sf_dolist(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::List(spec) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        if spec.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(var) = &spec[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        let list_val = self.eval(&spec[1])?;
        let items = list_to_vec(&list_val).unwrap_or_default();

        self.dynamic.push(HashMap::new());
        for item in items {
            if let Some(frame) = self.dynamic.last_mut() {
                frame.insert(var.clone(), item);
            }
            self.sf_progn(&tail[1..])?;
        }
        let result = if spec.len() > 2 {
            if let Some(frame) = self.dynamic.last_mut() {
                frame.insert(var.clone(), Value::Nil);
            }
            self.eval(&spec[2])?
        } else {
            Value::Nil
        };
        self.dynamic.pop();
        Ok(result)
    }

    // -----------------------------------------------------------------------
    // Lambda / Function application
    // -----------------------------------------------------------------------

    pub(crate) fn eval_lambda(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let params = self.parse_lambda_params(&tail[0])?;

        // Extract docstring if present as the first body element.
        let (docstring, body_start) = match tail.get(1) {
            Some(Expr::Str(s)) => (Some(s.clone()), 2),
            _ => (None, 1),
        };

        // Capture lexical environment for closures (when lexical-binding is on)
        let env = if self.lexical_binding() && !self.lexenv.is_empty() {
            Some(self.lexenv.clone())
        } else {
            None
        };

        Ok(Value::Lambda(std::sync::Arc::new(LambdaData {
            params,
            body: tail[body_start..].to_vec(),
            env,
            docstring,
        })))
    }

    fn parse_lambda_params(&self, expr: &Expr) -> Result<LambdaParams, Flow> {
        match expr {
            Expr::Symbol(s) if s == "nil" => Ok(LambdaParams::simple(vec![])),
            Expr::List(items) => {
                let mut required = Vec::new();
                let mut optional = Vec::new();
                let mut rest = None;
                let mut mode = 0; // 0=required, 1=optional, 2=rest

                for item in items {
                    let Expr::Symbol(name) = item else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    match name.as_str() {
                        "&optional" => {
                            mode = 1;
                            continue;
                        }
                        "&rest" => {
                            mode = 2;
                            continue;
                        }
                        _ => {}
                    }
                    match mode {
                        0 => required.push(name.clone()),
                        1 => optional.push(name.clone()),
                        2 => {
                            rest = Some(name.clone());
                            break;
                        }
                        _ => unreachable!(),
                    }
                }

                Ok(LambdaParams {
                    required,
                    optional,
                    rest,
                })
            }
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    /// Apply a function value to evaluated arguments.
    pub(crate) fn apply(&mut self, function: Value, args: Vec<Value>) -> EvalResult {
        match function {
            Value::ByteCode(bc) => {
                self.refresh_features_from_variable();
                let mut vm = super::bytecode::Vm::new(
                    &mut self.obarray,
                    &mut self.dynamic,
                    &mut self.lexenv,
                    &mut self.features,
                );
                let result = vm.execute(&bc, args);
                self.sync_features_variable();
                result
            }
            Value::Lambda(lambda) | Value::Macro(lambda) => self.apply_lambda(&lambda, args),
            Value::Subr(name) => {
                self.apply_named_callable(&name, args, Value::Subr(name.clone()), true)
            }
            Value::Symbol(name) => {
                self.apply_named_callable(&name, args, Value::Subr(name.clone()), true)
            }
            Value::True => {
                self.apply_named_callable("t", args, Value::Subr("t".to_string()), true)
            }
            Value::Keyword(name) => {
                self.apply_named_callable(&name, args, Value::Subr(name.clone()), true)
            }
            Value::Nil => Err(signal("void-function", vec![Value::symbol("nil")])),
            _ => Err(signal("invalid-function", vec![function])),
        }
    }

    #[inline]
    fn resolve_named_call_target(&mut self, name: &str) -> NamedCallTarget {
        let function_epoch = self.obarray.function_epoch();
        if let Some(cache) = &self.named_call_cache {
            if cache.symbol == name && cache.function_epoch == function_epoch {
                return cache.target.clone();
            }
        }

        let target = if let Some(func) = self.obarray.symbol_function(name).cloned() {
            match &func {
                Value::Nil => NamedCallTarget::Void,
                // `(fset 'foo (symbol-function 'foo))` writes `#<subr foo>` into
                // the function cell. Treat this as a direct builtin/special-form
                // callable, not an obarray indirection cycle.
                Value::Subr(bound_name) if bound_name == name => {
                    if super::subr_info::is_evaluator_callable_name(name) {
                        NamedCallTarget::EvaluatorCallable
                    } else if super::subr_info::is_special_form(name) {
                        NamedCallTarget::SpecialForm
                    } else {
                        NamedCallTarget::Probe
                    }
                }
                _ => NamedCallTarget::Obarray(func),
            }
        } else if self.obarray.is_function_unbound(name) {
            NamedCallTarget::Void
        } else if super::subr_info::is_evaluator_callable_name(name) {
            NamedCallTarget::EvaluatorCallable
        } else if super::subr_info::is_special_form(name) {
            NamedCallTarget::SpecialForm
        } else {
            NamedCallTarget::Probe
        };

        self.named_call_cache = Some(NamedCallCache {
            symbol: name.to_string(),
            function_epoch,
            target: target.clone(),
        });

        target
    }

    #[inline]
    fn apply_named_callable(
        &mut self,
        name: &str,
        args: Vec<Value>,
        invalid_fn: Value,
        rewrite_builtin_wrong_arity: bool,
    ) -> EvalResult {
        match self.resolve_named_call_target(name) {
            NamedCallTarget::Obarray(func) => match self.apply(func, args) {
                Err(Flow::Signal(sig)) if sig.symbol == "invalid-function" => {
                    Err(signal("invalid-function", vec![Value::symbol(name)]))
                }
                other => other,
            },
            NamedCallTarget::EvaluatorCallable => self.apply_evaluator_callable(name, args),
            NamedCallTarget::Probe => {
                if let Some(result) = builtins::dispatch_builtin(self, name, args) {
                    self.named_call_cache = Some(NamedCallCache {
                        symbol: name.to_string(),
                        function_epoch: self.obarray.function_epoch(),
                        target: NamedCallTarget::Builtin,
                    });
                    if rewrite_builtin_wrong_arity {
                        result.map_err(|flow| rewrite_wrong_arity_function_object(flow, name))
                    } else {
                        result
                    }
                } else {
                    self.named_call_cache = Some(NamedCallCache {
                        symbol: name.to_string(),
                        function_epoch: self.obarray.function_epoch(),
                        target: NamedCallTarget::Void,
                    });
                    Err(signal("void-function", vec![Value::symbol(name)]))
                }
            }
            NamedCallTarget::Builtin => {
                if let Some(result) = builtins::dispatch_builtin(self, name, args) {
                    if rewrite_builtin_wrong_arity {
                        result.map_err(|flow| rewrite_wrong_arity_function_object(flow, name))
                    } else {
                        result
                    }
                } else {
                    self.named_call_cache = Some(NamedCallCache {
                        symbol: name.to_string(),
                        function_epoch: self.obarray.function_epoch(),
                        target: NamedCallTarget::Void,
                    });
                    Err(signal("void-function", vec![Value::symbol(name)]))
                }
            }
            NamedCallTarget::SpecialForm => Err(signal("invalid-function", vec![invalid_fn])),
            NamedCallTarget::Void => Err(signal("void-function", vec![Value::symbol(name)])),
        }
    }

    fn apply_evaluator_callable(&mut self, name: &str, args: Vec<Value>) -> EvalResult {
        match name {
            "throw" => {
                if args.len() != 2 {
                    return Err(signal(
                        "wrong-number-of-arguments",
                        vec![Value::Subr("throw".to_string()), Value::Int(args.len() as i64)],
                    ));
                }
                Err(Flow::Throw {
                    tag: args[0].clone(),
                    value: args[1].clone(),
                })
            }
            _ => Err(signal("void-function", vec![Value::symbol(name)])),
        }
    }

    fn apply_lambda(&mut self, lambda: &LambdaData, args: Vec<Value>) -> EvalResult {
        let params = &lambda.params;

        // Arity check
        if args.len() < params.min_arity() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        if let Some(max) = params.max_arity() {
            if args.len() > max {
                return Err(signal("wrong-number-of-arguments", vec![]));
            }
        }

        let mut frame = HashMap::new();
        let mut arg_idx = 0;

        // Required params
        for param in &params.required {
            frame.insert(param.clone(), args[arg_idx].clone());
            arg_idx += 1;
        }

        // Optional params
        for param in &params.optional {
            if arg_idx < args.len() {
                frame.insert(param.clone(), args[arg_idx].clone());
                arg_idx += 1;
            } else {
                frame.insert(param.clone(), Value::Nil);
            }
        }

        // Rest param
        if let Some(ref rest_name) = params.rest {
            let rest_args: Vec<Value> = args[arg_idx..].to_vec();
            frame.insert(rest_name.clone(), Value::list(rest_args));
        }

        // If closure has a captured lexenv, restore it
        let saved_lexenv = if let Some(ref env) = lambda.env {
            let old = std::mem::replace(&mut self.lexenv, env.clone());
            // Push param bindings as a new lexical frame on top of captured env
            self.lexenv.push(frame);
            Some(old)
        } else {
            // Dynamic binding (no captured lexenv)
            self.dynamic.push(frame);
            None
        };
        let saved_lexical_mode = if lambda.env.is_some() {
            let old = self.lexical_binding();
            self.set_lexical_binding(true);
            Some(old)
        } else {
            None
        };

        let result = self.sf_progn(&lambda.body);

        if let Some(old_mode) = saved_lexical_mode {
            self.set_lexical_binding(old_mode);
        }
        if let Some(old_lexenv) = saved_lexenv {
            self.lexenv = old_lexenv;
        } else {
            self.dynamic.pop();
        }
        result
    }

    // -----------------------------------------------------------------------
    // Macro expansion
    // -----------------------------------------------------------------------

    fn expand_macro(&mut self, macro_val: Value, args: &[Expr]) -> Result<Expr, Flow> {
        let Value::Macro(lambda) = macro_val else {
            return Err(signal("invalid-macro", vec![]));
        };

        // Convert unevaluated args to values (quoted forms)
        let arg_values: Vec<Value> = args.iter().map(quote_to_value).collect();

        // Apply the macro body
        let expanded_value = self.apply_lambda(&lambda, arg_values)?;

        // Convert value back to expr for re-evaluation
        Ok(value_to_expr(&expanded_value))
    }

    // -----------------------------------------------------------------------
    // Variable assignment
    // -----------------------------------------------------------------------

    pub(crate) fn assign(&mut self, name: &str, value: Value) {
        // If lexical binding and not special, check lexenv first
        if self.lexical_binding() && !self.obarray.is_special(name) {
            for frame in self.lexenv.iter_mut().rev() {
                if frame.contains_key(name) {
                    frame.insert(name.to_string(), value);
                    return;
                }
            }
        }

        // Search dynamic frames (inner to outer)
        for frame in self.dynamic.iter_mut().rev() {
            if frame.contains_key(name) {
                frame.insert(name.to_string(), value);
                return;
            }
        }

        // Update existing buffer-local binding if present.
        if let Some(buf) = self.buffers.current_buffer_mut() {
            if buf.get_buffer_local(name).is_some() {
                buf.set_buffer_local(name, value);
                return;
            }
        }

        // Auto-local variables become local upon assignment.
        if self.custom.is_auto_buffer_local(name) {
            if let Some(buf) = self.buffers.current_buffer_mut() {
                buf.set_buffer_local(name, value);
                return;
            }
        }

        // Fall through to obarray value cell
        self.obarray.set_symbol_value(name, value);
    }
}

fn rewrite_wrong_arity_function_object(flow: Flow, name: &str) -> Flow {
    match flow {
        Flow::Signal(mut sig) => {
            if sig.symbol == "wrong-number-of-arguments"
                && sig.raw_data.is_none()
                && !sig.data.is_empty()
                && sig.data[0].as_symbol_name() == Some(name)
            {
                sig.data[0] = Value::Subr(name.to_string());
            }
            Flow::Signal(sig)
        }
        other => other,
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert an Expr AST node to a Value (for quote).
pub fn quote_to_value(expr: &Expr) -> Value {
    match expr {
        Expr::Int(v) => Value::Int(*v),
        Expr::Float(v) => Value::Float(*v),
        Expr::Str(s) => Value::string(s.clone()),
        Expr::Char(c) => Value::Char(*c),
        Expr::Keyword(s) => Value::Keyword(s.clone()),
        Expr::Bool(true) => Value::True,
        Expr::Bool(false) => Value::Nil,
        Expr::Symbol(s) if s == "nil" => Value::Nil,
        Expr::Symbol(s) if s == "t" => Value::True,
        Expr::Symbol(s) => Value::Symbol(s.clone()),
        Expr::List(items) => {
            let quoted = items.iter().map(quote_to_value).collect::<Vec<_>>();
            Value::list(quoted)
        }
        Expr::DottedList(items, last) => {
            let head_vals: Vec<Value> = items.iter().map(quote_to_value).collect();
            let tail_val = quote_to_value(last);
            head_vals
                .into_iter()
                .rev()
                .fold(tail_val, |acc, item| Value::cons(item, acc))
        }
        Expr::Vector(items) => {
            let vals = items.iter().map(quote_to_value).collect();
            Value::vector(vals)
        }
    }
}

/// Public wrapper for value_to_expr (used by builtins::eval).
pub(crate) fn value_to_expr_pub(value: &Value) -> Expr {
    value_to_expr(value)
}

/// Convert a Value back to an Expr (for macro expansion).
fn value_to_expr(value: &Value) -> Expr {
    match value {
        Value::Nil => Expr::Symbol("nil".into()),
        Value::True => Expr::Symbol("t".into()),
        Value::Int(n) => Expr::Int(*n),
        Value::Float(f) => Expr::Float(*f),
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::Keyword(s) => Expr::Keyword(s.clone()),
        Value::Str(s) => Expr::Str((**s).clone()),
        Value::Char(c) => Expr::Char(*c),
        Value::Cons(_) => {
            if let Some(items) = list_to_vec(value) {
                Expr::List(items.iter().map(value_to_expr).collect())
            } else {
                // Improper list — best effort
                Expr::Symbol(format!("{}", value))
            }
        }
        Value::Vector(v) => {
            let items = v.lock().expect("poisoned");
            Expr::Vector(items.iter().map(value_to_expr).collect())
        }
        _ => Expr::Symbol(format!("{}", value)),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms};

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

    #[test]
    fn basic_arithmetic() {
        assert_eq!(eval_one("(+ 1 2)"), "OK 3");
        assert_eq!(eval_one("(- 10 3)"), "OK 7");
        assert_eq!(eval_one("(* 4 5)"), "OK 20");
        assert_eq!(eval_one("(/ 10 3)"), "OK 3");
        assert_eq!(eval_one("(% 10 3)"), "OK 1");
        assert_eq!(eval_one("(1+ 5)"), "OK 6");
        assert_eq!(eval_one("(1- 5)"), "OK 4");
    }

    #[test]
    fn float_arithmetic() {
        assert_eq!(eval_one("(+ 1.0 2.0)"), "OK 3.0");
        assert_eq!(eval_one("(+ 1 2.0)"), "OK 3.0"); // int promoted to float
        assert_eq!(eval_one("(/ 10.0 3.0)"), "OK 3.3333333333333335");
    }

    #[test]
    fn comparisons() {
        assert_eq!(eval_one("(< 1 2)"), "OK t");
        assert_eq!(eval_one("(> 1 2)"), "OK nil");
        assert_eq!(eval_one("(= 3 3)"), "OK t");
        assert_eq!(eval_one("(<= 3 3)"), "OK t");
        assert_eq!(eval_one("(>= 5 3)"), "OK t");
        assert_eq!(eval_one("(/= 1 2)"), "OK t");
    }

    #[test]
    fn type_predicates() {
        assert_eq!(eval_one("(integerp 42)"), "OK t");
        assert_eq!(eval_one("(floatp 3.14)"), "OK t");
        assert_eq!(eval_one("(stringp \"hello\")"), "OK t");
        assert_eq!(eval_one("(symbolp 'foo)"), "OK t");
        assert_eq!(eval_one("(consp '(1 2))"), "OK t");
        assert_eq!(eval_one("(null nil)"), "OK t");
        assert_eq!(eval_one("(null t)"), "OK nil");
        assert_eq!(eval_one("(listp nil)"), "OK t");
    }

    #[test]
    fn string_operations() {
        assert_eq!(
            eval_one(r#"(concat "hello" " " "world")"#),
            r#"OK "hello world""#
        );
        assert_eq!(eval_one(r#"(substring "hello" 1 3)"#), r#"OK "el""#);
        assert_eq!(eval_one(r#"(length "hello")"#), "OK 5");
        assert_eq!(eval_one(r#"(upcase "hello")"#), r#"OK "HELLO""#);
        assert_eq!(eval_one(r#"(string-equal "abc" "abc")"#), "OK t");
    }

    #[test]
    fn and_or_cond() {
        assert_eq!(eval_one("(and 1 2 3)"), "OK 3");
        assert_eq!(eval_one("(and 1 nil 3)"), "OK nil");
        assert_eq!(eval_one("(or nil nil 3)"), "OK 3");
        assert_eq!(eval_one("(or nil nil nil)"), "OK nil");
        assert_eq!(eval_one("(cond (nil 1) (t 2))"), "OK 2");
    }

    #[test]
    fn while_loop() {
        assert_eq!(
            eval_one("(let ((x 0)) (while (< x 5) (setq x (1+ x))) x)"),
            "OK 5"
        );
    }

    #[test]
    fn defvar_only_sets_if_unbound() {
        let results = eval_all("(defvar x 42) x (defvar x 99) x");
        assert_eq!(results, vec!["OK x", "OK 42", "OK x", "OK 42"]);
    }

    #[test]
    fn defvar_and_defconst_error_payloads_match_oracle_edges() {
        let results = eval_all(
            "(condition-case err (defvar) (error err))
             (condition-case err (defvar 1) (error err))
             (condition-case err (defvar 'vm-dv) (error err))
             (condition-case err (defvar vm-dv 1 \"doc\" t) (error err))
             (condition-case err (defconst) (error err))
             (condition-case err (defconst vm-dc) (error err))
             (condition-case err (defconst 1 2) (error err))
             (condition-case err (defconst 'vm-dc 1) (error err))
             (condition-case err (defconst vm-dc 1 \"doc\" t) (error err))",
        );
        assert_eq!(results[0], "OK (wrong-number-of-arguments defvar 0)");
        assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[2], "OK (wrong-type-argument symbolp 'vm-dv)");
        assert_eq!(results[3], "OK (error \"Too many arguments\")");
        assert_eq!(results[4], "OK (wrong-number-of-arguments defconst 0)");
        assert_eq!(results[5], "OK (wrong-number-of-arguments defconst 1)");
        assert_eq!(results[6], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[7], "OK (wrong-type-argument symbolp 'vm-dc)");
        assert_eq!(results[8], "OK (error \"Too many arguments\")");
    }

    #[test]
    fn setq_local_makes_binding_buffer_local() {
        let result = eval_one("(with-temp-buffer (setq-local vm-x 7) vm-x)");
        assert_eq!(result, "OK 7");
    }

    #[test]
    fn defmacro_works() {
        let result = eval_all(
            "(defmacro my-when (cond &rest body)
               (list 'if cond (cons 'progn body)))
             (my-when t 1 2 3)",
        );
        assert_eq!(result[1], "OK 3");
    }

    #[test]
    fn defun_and_defmacro_allow_empty_body() {
        let results = eval_all(
            "(defun vm-empty-f nil)
             (vm-empty-f)
             (defmacro vm-empty-m nil)
             (vm-empty-m)",
        );
        assert_eq!(results[0], "OK vm-empty-f");
        assert_eq!(results[1], "OK nil");
        assert_eq!(results[2], "OK vm-empty-m");
        assert_eq!(results[3], "OK nil");
    }

    #[test]
    fn defun_and_defmacro_error_payloads_match_oracle_edges() {
        let results = eval_all(
            "(condition-case err (defun) (error err))
             (condition-case err (defun 1 nil) (error err))
             (condition-case err (defun 'vm-df nil 1) (error err))
             (condition-case err (defmacro) (error err))
             (condition-case err (defmacro 1 nil) (error err))
             (condition-case err (defmacro 'vm-dm nil 1) (error err))",
        );
        assert_eq!(results[0], "OK (wrong-number-of-arguments (2 . 2) 0)");
        assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[2], "OK (wrong-type-argument symbolp 'vm-df)");
        assert_eq!(results[3], "OK (wrong-number-of-arguments (2 . 2) 0)");
        assert_eq!(results[4], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[5], "OK (wrong-type-argument symbolp 'vm-dm)");
    }

    #[test]
    fn optional_and_rest_params() {
        let results = eval_all(
            "(defun f (a &optional b &rest c) (list a b c))
             (f 1)
             (f 1 2)
             (f 1 2 3 4)",
        );
        assert_eq!(results[1], "OK (1 nil nil)");
        assert_eq!(results[2], "OK (1 2 nil)");
        assert_eq!(results[3], "OK (1 2 (3 4))");
    }

    #[test]
    fn when_unless() {
        assert_eq!(eval_one("(when t 1 2 3)"), "OK 3");
        assert_eq!(eval_one("(when nil 1 2 3)"), "OK nil");
        assert_eq!(eval_one("(unless nil 1 2 3)"), "OK 3");
        assert_eq!(eval_one("(unless t 1 2 3)"), "OK nil");
    }

    #[test]
    fn hash_table_ops() {
        let results = eval_all(
            "(let ((ht (make-hash-table :test 'equal)))
               (puthash \"key\" 42 ht)
               (gethash \"key\" ht))",
        );
        assert_eq!(results[0], "OK 42");
    }

    #[test]
    fn vector_ops() {
        assert_eq!(eval_one("(aref [10 20 30] 1)"), "OK 20");
        assert_eq!(eval_one("(length [1 2 3])"), "OK 3");
    }

    #[test]
    fn format_function() {
        assert_eq!(
            eval_one(r#"(format "hello %s, %d" "world" 42)"#),
            r#"OK "hello world, 42""#
        );
    }

    #[test]
    fn prog1() {
        assert_eq!(eval_one("(prog1 1 2 3)"), "OK 1");
    }

    #[test]
    fn function_special_form() {
        let results = eval_all(
            "(defun add1 (x) (+ x 1))
             (funcall #'add1 5)",
        );
        assert_eq!(results[1], "OK 6");
    }

    #[test]
    fn function_special_form_symbol_and_literal_payloads() {
        assert_eq!(eval_one("#'car"), "OK car");
        assert_eq!(eval_one("#'definitely-missing"), "OK definitely-missing");
        assert_eq!(
            eval_one("(condition-case err #'1 (error (car err)))"),
            "OK 1"
        );
        assert_eq!(
            eval_one("(equal #''(lambda) ''(lambda))"),
            "OK t"
        );
    }

    #[test]
    fn lambda_captures_docstring_metadata() {
        let forms = parse_forms("(lambda nil \"lambda-doc\" nil)").expect("parse");
        let mut ev = Evaluator::new();
        let value = ev.eval_expr(&forms[0]).expect("eval");
        let Value::Lambda(data) = value else {
            panic!("expected lambda value");
        };
        assert_eq!(data.docstring.as_deref(), Some("lambda-doc"));
    }

    #[test]
    fn defmacro_captures_docstring_metadata() {
        let forms = parse_forms("(defmacro vm-doc-macro (x) \"macro-doc\" x)").expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_expr(&forms[0]).expect("eval defmacro");
        let macro_val = ev
            .obarray
            .symbol_function("vm-doc-macro")
            .cloned()
            .expect("macro function cell");
        let Value::Macro(data) = macro_val else {
            panic!("expected macro value");
        };
        assert_eq!(data.docstring.as_deref(), Some("macro-doc"));
    }

    #[test]
    fn function_special_form_wrong_arity_signals() {
        assert_eq!(
            eval_one("(condition-case err (function) (error (car err)))"),
            "OK wrong-number-of-arguments"
        );
        assert_eq!(
            eval_one("(condition-case err (function 1 2) (error (car err)))"),
            "OK wrong-number-of-arguments"
        );
    }

    #[test]
    fn special_form_arity_payloads_match_oracle_edges() {
        let results = eval_all(
            "(condition-case err (if) (error err))
             (condition-case err (if t) (error err))
             (condition-case err (when) (error err))
             (condition-case err (unless) (error err))
             (condition-case err (quote) (error err))
             (condition-case err (quote 1 2) (error err))
             (condition-case err (function) (error err))
             (condition-case err (function 1 2) (error err))
             (condition-case err (prog1) (error err))
             (condition-case err (catch) (error err))
             (condition-case err (throw) (error err))
             (condition-case err (condition-case) (error err))
             (condition-case err (let) (error err))
             (condition-case err (let*) (error err))
             (condition-case err (while) (error err))
             (condition-case err (unwind-protect) (error err))",
        );
        assert_eq!(results[0], "OK (wrong-number-of-arguments if 0)");
        assert_eq!(results[1], "OK (wrong-number-of-arguments if 1)");
        assert_eq!(results[2], "OK (wrong-number-of-arguments (1 . 1) 0)");
        assert_eq!(results[3], "OK (wrong-number-of-arguments (1 . 1) 0)");
        assert_eq!(results[4], "OK (wrong-number-of-arguments quote 0)");
        assert_eq!(results[5], "OK (wrong-number-of-arguments quote 2)");
        assert_eq!(results[6], "OK (wrong-number-of-arguments function 0)");
        assert_eq!(results[7], "OK (wrong-number-of-arguments function 2)");
        assert_eq!(results[8], "OK (wrong-number-of-arguments prog1 0)");
        assert_eq!(results[9], "OK (wrong-number-of-arguments catch 0)");
        assert_eq!(results[10], "OK (wrong-number-of-arguments throw 0)");
        assert_eq!(
            results[11],
            "OK (wrong-number-of-arguments condition-case 0)"
        );
        assert_eq!(results[12], "OK (wrong-number-of-arguments let 0)");
        assert_eq!(results[13], "OK (wrong-number-of-arguments let* 0)");
        assert_eq!(results[14], "OK (wrong-number-of-arguments while 0)");
        assert_eq!(
            results[15],
            "OK (wrong-number-of-arguments unwind-protect 0)"
        );
    }

    #[test]
    fn let_dotted_binding_list_reports_listp_tail_payload() {
        assert_eq!(
            eval_one("(condition-case err (let ((x 1) . 2) x) (error err))"),
            "OK (wrong-type-argument listp 2)"
        );
        assert_eq!(
            eval_one("(condition-case err (let* ((x 1) . 2) x) (error err))"),
            "OK (wrong-type-argument listp 2)"
        );
    }

    #[test]
    fn special_form_type_payloads_match_oracle_edges() {
        let results = eval_all(
            "(condition-case err (setq x) (error err))
             (condition-case err (setq 1 2) (error err))
             (condition-case err (let ((1 2)) nil) (error err))
             (condition-case err (let* ((1 2)) nil) (error err))
             (condition-case err (cond 1) (error err))
             (condition-case err (condition-case 1 2 (error 3)) (error err))
             (condition-case err (condition-case err 2 3) (error err))
             (condition-case err (condition-case err 2 ()) (error err))",
        );
        assert_eq!(results[0], "OK (wrong-number-of-arguments setq 1)");
        assert_eq!(results[1], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[2], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[3], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[4], "OK (wrong-type-argument listp 1)");
        assert_eq!(results[5], "OK (wrong-type-argument symbolp 1)");
        assert_eq!(results[6], "OK (error \"Invalid condition handler: 3\")");
        assert_eq!(results[7], "OK 2");
    }

    #[test]
    fn mapcar_works() {
        assert_eq!(eval_one("(mapcar #'1+ '(1 2 3))"), "OK (2 3 4)");
    }

    #[test]
    fn apply_works() {
        assert_eq!(eval_one("(apply #'+ '(1 2 3))"), "OK 6");
        assert_eq!(eval_one("(apply #'+ 1 2 '(3))"), "OK 6");
    }

    #[test]
    fn apply_improper_tail_signals_wrong_type_argument() {
        assert_eq!(
            eval_one(
                "(condition-case err
                     (apply 'list '(1 . 2))
                   (error (list (car err) (nth 2 err))))"
            ),
            "OK (wrong-type-argument 2)"
        );
    }

    #[test]
    fn funcall_and_apply_nil_signal_void_function() {
        let funcall_result = eval_one(
            "(condition-case err
                 (funcall nil)
               (void-function (car err)))",
        );
        assert_eq!(funcall_result, "OK void-function");

        let apply_result = eval_one(
            "(condition-case err
                 (apply nil nil)
               (void-function (car err)))",
        );
        assert_eq!(apply_result, "OK void-function");
    }

    #[test]
    fn funcall_and_apply_non_callable_symbol_edges() {
        assert_eq!(
            eval_one("(condition-case err (funcall t) (error (car err)))"),
            "OK void-function"
        );
        assert_eq!(
            eval_one("(condition-case err (funcall :vm-matrix-keyword) (error (car err)))"),
            "OK void-function"
        );
        assert_eq!(
            eval_one("(condition-case err (funcall 'if) (error (car err)))"),
            "OK invalid-function"
        );
        assert_eq!(
            eval_one("(condition-case err (funcall (symbol-function 'if) t 1 2) (error (car err)))"),
            "OK invalid-function"
        );
        assert_eq!(
            eval_one("(condition-case err (apply t nil) (error (car err)))"),
            "OK void-function"
        );
        assert_eq!(
            eval_one("(condition-case err (apply :vm-matrix-keyword nil) (error (car err)))"),
            "OK void-function"
        );
        assert_eq!(
            eval_one("(condition-case err (apply 'if '(t 1 2)) (error (car err)))"),
            "OK invalid-function"
        );
    }

    #[test]
    fn funcall_throw_is_callable_and_preserves_throw_semantics() {
        assert_eq!(eval_one("(catch 'tag (funcall 'throw 'tag 42))"), "OK 42");
        assert_eq!(
            eval_one("(condition-case err (funcall 'throw 'tag 42) (error err))"),
            "OK (no-catch tag 42)"
        );
        assert_eq!(
            eval_one("(condition-case err (funcall 'throw) (error err))"),
            "OK (wrong-number-of-arguments #<subr throw> 0)"
        );
    }

    #[test]
    fn fmakunbound_masks_builtin_special_and_evaluator_callable_fallbacks() {
        let results = eval_all(
            "(fmakunbound 'car)
             (fboundp 'car)
             (symbol-function 'car)
             (condition-case err (car '(1 2)) (void-function 'void-function))
             (fmakunbound 'if)
             (fboundp 'if)
             (symbol-function 'if)
             (condition-case err (if t 1 2) (void-function 'void-function))
             (fmakunbound 'throw)
             (fboundp 'throw)
             (symbol-function 'throw)
             (condition-case err (throw 'tag 1) (void-function 'void-function))",
        );
        assert_eq!(results[1], "OK nil");
        assert_eq!(results[2], "OK nil");
        assert_eq!(results[3], "OK void-function");
        assert_eq!(results[5], "OK nil");
        assert_eq!(results[6], "OK nil");
        assert_eq!(results[7], "OK void-function");
        assert_eq!(results[9], "OK nil");
        assert_eq!(results[10], "OK nil");
        assert_eq!(results[11], "OK void-function");
    }

    #[test]
    fn fset_can_override_special_form_name_for_direct_calls() {
        let result = eval_one(
            "(let ((orig (symbol-function 'if)))
               (unwind-protect
                   (progn
                     (fset 'if (lambda (&rest _args) 'ov))
                     (if t 1 2))
                 (fset 'if orig)))",
        );
        assert_eq!(result, "OK ov");
    }

    #[test]
    fn fset_restoring_subr_object_keeps_callability() {
        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function 'car)))
                   (fset 'car orig)
                   (car '(1 2)))"
            ),
            "OK 1"
        );

        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function 'if)))
                   (fset 'if orig)
                   (if t 1 2))"
            ),
            "OK 1"
        );
    }

    #[test]
    fn fset_nil_reports_symbol_payload_for_void_function_calls() {
        let results = eval_all(
            "(fset 'vm-fsetnil nil)
             (fboundp 'vm-fsetnil)
             (condition-case err (vm-fsetnil) (error err))
             (condition-case err (funcall 'vm-fsetnil) (error err))
             (condition-case err (apply 'vm-fsetnil nil) (error err))
             (fset 'length nil)
             (fboundp 'length)
             (condition-case err (length '(1 2)) (error err))",
        );

        assert_eq!(results[0], "OK nil");
        assert_eq!(results[1], "OK nil");
        assert_eq!(results[2], "OK (void-function vm-fsetnil)");
        assert_eq!(results[3], "OK (void-function vm-fsetnil)");
        assert_eq!(results[4], "OK (void-function vm-fsetnil)");
        assert_eq!(results[5], "OK nil");
        assert_eq!(results[6], "OK nil");
        assert_eq!(results[7], "OK (void-function length)");
    }

    #[test]
    fn fset_noncallable_reports_symbol_payload_for_invalid_function_calls() {
        let results = eval_all(
            "(fset 'vm-fsetint 1)
             (fboundp 'vm-fsetint)
             (symbol-function 'vm-fsetint)
             (condition-case err (vm-fsetint) (error err))
             (condition-case err (funcall 'vm-fsetint) (error err))
             (condition-case err (apply 'vm-fsetint nil) (error err))",
        );

        assert_eq!(results[0], "OK 1");
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK 1");
        assert_eq!(results[3], "OK (invalid-function vm-fsetint)");
        assert_eq!(results[4], "OK (invalid-function vm-fsetint)");
        assert_eq!(results[5], "OK (invalid-function vm-fsetint)");
    }

    #[test]
    fn fset_t_function_cell_controls_funcall_and_apply_behavior() {
        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function 't)))
                   (unwind-protect
                       (progn
                         (fset 't 'car)
                         (funcall t '(1 2)))
                     (fset 't orig)))"
            ),
            "OK 1"
        );

        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function 't)))
                   (unwind-protect
                       (progn
                         (fset 't 1)
                         (condition-case err (funcall t) (error err)))
                     (fset 't orig)))"
            ),
            "OK (invalid-function t)"
        );
    }

    #[test]
    fn fset_keyword_function_cell_controls_funcall_and_apply_behavior() {
        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function :k)))
                   (unwind-protect
                       (progn
                         (fset :k 'car)
                         (funcall :k '(1 2)))
                     (fset :k orig)))"
            ),
            "OK 1"
        );

        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function :k)))
                   (unwind-protect
                       (progn
                         (fset :k 'car)
                         (apply :k '((1 2))))
                     (fset :k orig)))"
            ),
            "OK 1"
        );

        assert_eq!(
            eval_one(
                "(let ((orig (symbol-function :k)))
                   (unwind-protect
                       (progn
                         (fset :k 1)
                         (condition-case err (funcall :k) (error err)))
                     (fset :k orig)))"
            ),
            "OK (invalid-function :k)"
        );
    }

    #[test]
    fn named_call_cache_invalidates_on_function_cell_mutation() {
        let results = eval_all(
            "(condition-case err
                 (funcall 'vm-cache-target)
               (error (car err)))
             (fset 'vm-cache-target (lambda () 9))
             (funcall 'vm-cache-target)
             (fset 'vm-cache-target (lambda () 11))
             (funcall 'vm-cache-target)",
        );
        assert_eq!(results[0], "OK void-function");
        assert_eq!(results[2], "OK 9");
        assert_eq!(results[4], "OK 11");
    }

    #[test]
    fn funcall_builtin_wrong_arity_uses_subr_object_payload() {
        assert_eq!(
            eval_one("(condition-case err (car) (error (subrp (nth 1 err))))"),
            "OK nil"
        );
        assert_eq!(
            eval_one("(condition-case err (funcall 'car) (error (subrp (nth 1 err))))"),
            "OK t"
        );
    }

    #[test]
    fn condition_case_catches_uncaught_throw_as_no_catch() {
        assert_eq!(
            eval_one("(condition-case err (throw 'tag 42) (error (car err)))"),
            "OK no-catch"
        );
        assert_eq!(
            eval_one("(condition-case err (exit-minibuffer) (error (car err)))"),
            "OK no-catch"
        );
        assert_eq!(
            eval_one("(condition-case err (exit-minibuffer) (no-catch err))"),
            "OK (no-catch exit nil)"
        );
    }

    #[test]
    fn backward_compat_core_forms() {
        // Same tests as original elisp.rs
        let source = r#"
        (+ 1 2)
        (let ((x 1)) (setq x (+ x 2)) x)
        (let ((lst '(1 2))) (setcar lst 9) lst)
        (catch 'tag (throw 'tag 42))
        (condition-case e (/ 1 0) (arith-error 'div-zero))
        (let ((x 1))
          (let ((f (lambda () x)))
            (let ((x 2))
              (funcall f))))
        "#;

        let forms = parse_forms(source).expect("parse");
        let mut ev = Evaluator::new();
        let rendered: Vec<String> = ev
            .eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect();

        assert_eq!(
            rendered,
            vec!["OK 3", "OK 3", "OK (9 2)", "OK 42", "OK div-zero", "OK 2"]
        );
    }

    #[test]
    fn excessive_recursion_detected() {
        let results = eval_all("(defun inf () (inf))\n(inf)");
        // Second form should trigger excessive nesting
        assert!(results[1].contains("excessive-lisp-nesting"));
    }

    #[test]
    fn lexical_binding_closure() {
        // With lexical binding, closures capture the lexical environment
        let forms = parse_forms(
            r#"
            (let ((x 1))
              (let ((f (lambda () x)))
                (let ((x 2))
                  (funcall f))))
        "#,
        )
        .expect("parse");
        let mut ev = Evaluator::new();
        ev.set_lexical_binding(true);
        let result = format_eval_result(&ev.eval_expr(&forms[0]));
        // In lexical binding, the closure captures x=1, not x=2
        assert_eq!(result, "OK 1");
    }

    #[test]
    fn dynamic_binding_closure() {
        // Without lexical binding (default), closures see dynamic scope
        let forms = parse_forms(
            r#"
            (let ((x 1))
              (let ((f (lambda () x)))
                (let ((x 2))
                  (funcall f))))
        "#,
        )
        .expect("parse");
        let mut ev = Evaluator::new();
        let result = format_eval_result(&ev.eval_expr(&forms[0]));
        // In dynamic binding, the lambda sees x=2 (innermost dynamic binding)
        assert_eq!(result, "OK 2");
    }

    #[test]
    fn lexical_binding_special_var_stays_dynamic() {
        // defvar makes a variable special — it stays dynamically scoped
        let forms = parse_forms(
            r#"
            (defvar my-special 10)
            (let ((my-special 20))
              (let ((f (lambda () my-special)))
                (let ((my-special 30))
                  (funcall f))))
        "#,
        )
        .expect("parse");
        let mut ev = Evaluator::new();
        ev.set_lexical_binding(true);
        let results: Vec<String> = ev
            .eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect();
        // my-special is declared special, so even in lexical mode it's dynamic
        assert_eq!(results[1], "OK 30");
    }

    #[test]
    fn defalias_works() {
        let results = eval_all(
            "(defun my-add (a b) (+ a b))
             (defalias 'my-plus 'my-add)
             (my-plus 3 4)",
        );
        assert_eq!(results[2], "OK 7");
    }

    #[test]
    fn defalias_rejects_self_alias_cycle() {
        let result = eval_one(
            "(condition-case err
                 (defalias 'vm-da-self 'vm-da-self)
               (error err))",
        );
        assert_eq!(result, "OK (cyclic-function-indirection vm-da-self)");
    }

    #[test]
    fn defalias_rejects_two_node_alias_cycle() {
        let results = eval_all(
            "(defalias 'vm-da-a 'vm-da-b)
             (condition-case err
                 (defalias 'vm-da-b 'vm-da-a)
               (error err))",
        );
        assert_eq!(results[0], "OK vm-da-a");
        assert_eq!(results[1], "OK (cyclic-function-indirection vm-da-b)");
    }

    #[test]
    fn defalias_nil_signals_setting_constant() {
        let result = eval_one(
            "(condition-case err
                 (defalias nil 'car)
               (error err))",
        );
        assert_eq!(result, "OK (setting-constant nil)");
    }

    #[test]
    fn defalias_t_accepts_symbol_cell_updates() {
        let results = eval_all(
            "(defalias t 'car)
             (symbol-function t)",
        );
        assert_eq!(results[0], "OK t");
        assert_eq!(results[1], "OK car");
    }

    #[test]
    fn defalias_enforces_argument_count() {
        let results = eval_all(
            "(condition-case err (defalias) (error err))
             (condition-case err (defalias 'vm-da-too-few) (error err))
             (condition-case err (defalias 'vm-da-too-many 'car \"doc\" t) (error err))",
        );
        assert_eq!(results[0], "OK (wrong-number-of-arguments defalias 0)");
        assert_eq!(results[1], "OK (wrong-number-of-arguments defalias 1)");
        assert_eq!(results[2], "OK (wrong-number-of-arguments defalias 4)");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn defalias_compiled_literal_coerces_to_compiled_function() {
        let results = eval_all(
            "(defalias 'vm-elc-placeholder #[(x) \"\\bT\\207\" [x] 1 (#$ . 83)])
             (compiled-function-p (symbol-function 'vm-elc-placeholder))
             (functionp (symbol-function 'vm-elc-placeholder))",
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK t");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn fset_compiled_literal_coerces_to_compiled_function() {
        let results = eval_all(
            "(fset 'vm-elc-fset-placeholder #[(x) \"\\bT\\207\" [x] 1 (#$ . 83)])
             (compiled-function-p (symbol-function 'vm-elc-fset-placeholder))
             (functionp (symbol-function 'vm-elc-fset-placeholder))",
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK t");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn calling_simple_compiled_literal_executes() {
        let results = eval_all(
            "(funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\207\\\" [42] 1]\")))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\207\\\" [x] 1]\")) 'vm-x)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\bT\\\\207\\\" [x] 1]\")) 77)
             (condition-case err
                 (funcall (car (read-from-string \"#[(x) \\\"\\\\bT\\\\207\\\" [x] 1]\")) 'vm-x)
               (error err))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\123\\\\207\\\" [x] 1]\")) 77)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\134\\\\207\\\" [x y] 2]\")) 7 35)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\132\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\137\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\125\\\\207\\\" [x y] 2]\")) 7 7)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\125\\\\207\\\" [x y] 2]\")) 7 8)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\100\\\\207\\\" [x] 1]\")) '(1 2))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\101\\\\207\\\" [x] 1]\")) '(1 2))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\203\\\\6\\\\0\\\\301\\\\207\\\\302\\\\207\\\" [x 1 2] 1]\")) t)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\203\\\\6\\\\0\\\\301\\\\207\\\\302\\\\207\\\" [x 1 2] 1]\")) nil)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\205\\\\5\\\\0\\\\301\\\\207\\\" [x 2] 1]\")) t)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\205\\\\5\\\\0\\\\301\\\\207\\\" [x 2] 1]\")) nil)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\206\\\\5\\\\0\\\\301\\\\207\\\" [x 2] 1]\")) t)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\206\\\\5\\\\0\\\\301\\\\207\\\" [x 2] 1]\")) nil)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\204\\\\6\\\\0\\\\301\\\\207\\\\302\\\\207\\\" [x 1 2] 1]\")) nil)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\204\\\\6\\\\0\\\\301\\\\207\\\\302\\\\207\\\" [x 1 2] 1]\")) t)",
        );
        assert_eq!(results[0], "OK 42");
        assert_eq!(results[1], "OK vm-x");
        assert_eq!(results[2], "OK 78");
        assert_eq!(results[3], "OK (wrong-type-argument number-or-marker-p vm-x)");
        assert_eq!(results[4], "OK 76");
        assert_eq!(results[5], "OK 42");
        assert_eq!(results[6], "OK 5");
        assert_eq!(results[7], "OK 14");
        assert_eq!(results[8], "OK t");
        assert_eq!(results[9], "OK nil");
        assert_eq!(results[10], "OK 1");
        assert_eq!(results[11], "OK (2)");
        assert_eq!(results[12], "OK 1");
        assert_eq!(results[13], "OK 2");
        assert_eq!(results[14], "OK 2");
        assert_eq!(results[15], "OK nil");
        assert_eq!(results[16], "OK t");
        assert_eq!(results[17], "OK 2");
        assert_eq!(results[18], "OK 1");
        assert_eq!(results[19], "OK 2");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn calling_extended_compiled_literal_opcodes_executes() {
        let results = eval_all(
            "(funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\127\\\\207\\\" [x y] 2]\")) 1 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\127\\\\207\\\" [x y] 2]\")) 2 1)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\126\\\\207\\\" [x y] 2]\")) 2 1)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\130\\\\207\\\" [x y] 2]\")) 2 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\131\\\\207\\\" [x y] 2]\")) 1 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\135\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\136\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\102\\\\207\\\" [x y] 2]\")) 1 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\104\\\\207\\\" [x y] 2]\")) 1 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\75\\\\207\\\" [x y] 2]\")) 'a 'a)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\232\\\\207\\\" [x y] 2]\")) '(1 2) '(1 2))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\76\\\\207\\\" [x y] 2]\")) 'b '(a b c))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\236\\\\207\\\" [x y] 2]\")) 'b '((a . 1) (b . 2)))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\71\\\\207\\\" [x] 1]\")) 'x)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\72\\\\207\\\" [x] 1]\")) '(1 . 2))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\77\\\\207\\\" [x] 1]\")) nil)
             (let ((c (cons 1 2)))
               (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\240\\\\207\\\" [x y] 2]\")) c 9)
               c)
             (let ((c (cons 1 2)))
               (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\241\\\\207\\\" [x y] 2]\")) c 9)
               c)",
        );
        assert_eq!(results[0], "OK t");
        assert_eq!(results[1], "OK nil");
        assert_eq!(results[2], "OK t");
        assert_eq!(results[3], "OK t");
        assert_eq!(results[4], "OK nil");
        assert_eq!(results[5], "OK 7");
        assert_eq!(results[6], "OK 2");
        assert_eq!(results[7], "OK (1 . 2)");
        assert_eq!(results[8], "OK (1 2)");
        assert_eq!(results[9], "OK t");
        assert_eq!(results[10], "OK t");
        assert_eq!(results[11], "OK (b c)");
        assert_eq!(results[12], "OK (b . 2)");
        assert_eq!(results[13], "OK t");
        assert_eq!(results[14], "OK t");
        assert_eq!(results[15], "OK t");
        assert_eq!(results[16], "OK (9 . 2)");
        assert_eq!(results[17], "OK (1 . 9)");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn calling_sequence_compiled_literal_opcodes_executes() {
        let results = eval_all(
            "(funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\73\\\\207\\\" [x] 1]\")) \"abc\")
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\74\\\\207\\\" [x] 1]\")) '(1 2))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\250\\\\207\\\" [x] 1]\")) 7)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\247\\\\207\\\" [x] 1]\")) 7.5)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\120\\\\207\\\" [x y] 2]\")) \"a\" \"b\")
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\302\\\\117\\\\207\\\" [x y nil] 3]\")) \"abcd\" 1)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\107\\\\207\\\" [x] 1]\")) '(1 2 3))
             (funcall (car (read-from-string \"#[(n x) \\\"\\\\10\\\\11\\\\70\\\\207\\\" [n x] 2]\")) 1 '(a b c))
             (funcall (car (read-from-string \"#[(n x) \\\"\\\\10\\\\11\\\\233\\\\207\\\" [n x] 2]\")) 1 '(a b c))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\230\\\\207\\\" [x y] 2]\")) \"ab\" \"ab\")
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\231\\\\207\\\" [x y] 2]\")) \"ab\" \"ac\")
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\110\\\\207\\\" [x y] 2]\")) [1 2 3] 1)
             (let ((v [1 2 3]))
               (funcall (car (read-from-string \"#[(x y z) \\\"\\\\10\\\\11\\\\12\\\\111\\\\207\\\" [x y z] 3]\")) v 1 9)
               v)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\245\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\246\\\\207\\\" [x y] 2]\")) 7 2)
             (funcall (car (read-from-string \"#[(x y z) \\\"\\\\10\\\\11\\\\12\\\\303\\\\304\\\\305\\\\257\\\\6\\\\207\\\" [x y z 1 2 3] 6]\")) 'a 'b 'c)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\235\\\\207\\\" [x y] 2]\")) 'b '(a b c))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\237\\\\207\\\" [x] 1]\")) '(1 2 3))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\40\\\\207\\\" [list] 1]\")))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x reverse] 2]\")) '(1 2 3))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y append] 3]\")) '(1) '(2))
             (funcall (car (read-from-string \"#[(x y z) \\\"\\\\303\\\\10\\\\11\\\\12\\\\43\\\\207\\\" [x y z append] 4]\")) '(1) '(2) '(3))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y logand] 3]\")) 6 3)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x floor] 2]\")) 3.7)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y vector] 3]\")) 1 2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y assoc] 3]\")) 'b '((a . 1) (b . 2)))
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\244\\\\207\\\" [x y] 2]\")) '(1) '(2))
             (funcall (car (read-from-string \"#[(x y z) \\\"\\\\10\\\\11\\\\244\\\\12\\\\244\\\\207\\\" [x y z] 3]\")) '(1) '(2) '(3))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x string-to-number] 2]\")) \"42.5\")
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x number-to-string] 2]\")) 42)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x char-to-string] 2]\")) 65)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x string-to-char] 2]\")) \"Az\")
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x float] 2]\")) 7)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x truncate] 2]\")) -3.7)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x ceiling] 2]\")) 3.2)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y logior] 3]\")) 6 3)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y logxor] 3]\")) 6 3)
             (funcall (car (read-from-string \"#[(x) \\\"\\\\301\\\\10\\\\41\\\\207\\\" [x lognot] 2]\")) 0)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\302\\\\10\\\\11\\\\42\\\\207\\\" [x y ash] 3]\")) 3 2)
             (funcall (car (read-from-string \"#[(a b c d e f g h) \\\"\\\\10\\\\11\\\\12\\\\13\\\\14\\\\15\\\\16\\\\6\\\\16\\\\7\\\\257\\\\10\\\\207\\\" [a b c d e f g h] 8]\")) 1 2 3 4 5 6 7 8)
             (funcall (car (read-from-string \"#[(a b c d e f g h) \\\"\\\\11\\\\20\\\\10\\\\16\\\\7\\\\26\\\\6\\\\16\\\\6\\\\104\\\\207\\\" [a b c d e f g h] 2]\")) 1 2 3 4 5 6 7 8)
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\1\\\\207\\\" [1 2] 3]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\211\\\\134\\\\207\\\" [7] 2]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\210\\\\207\\\" [1 2] 2]\")))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\133\\\\207\\\" [x] 1]\")) 7)
             (progn
               (set 'x 99)
               (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\112\\\\207\\\" [x] 1]\"))))
             (progn
               (put 'x 'p 42)
               (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\116\\\\207\\\" [x p] 2]\"))))
             (funcall (car (read-from-string \"#[nil \\\"\\\\201\\\\100\\\\0\\\\207\\\" [0 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30 31 32 33 34 35 36 37 38 39 40 41 42 43 44 45 46 47 48 49 50 51 52 53 54 55 56 57 58 59 60 61 62 63 64] 1]\")))
             (funcall (car (read-from-string \"#[(a b c d e) \\\"\\\\10\\\\11\\\\12\\\\13\\\\14\\\\260\\\\5\\\\207\\\" [a b c d e] 5]\")) \"a\" \"b\" \"c\" \"d\" \"e\")
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\302\\\\262\\\\1\\\\104\\\\207\\\" [1 2 3] 3]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\302\\\\303\\\\263\\\\2\\\\0\\\\105\\\\207\\\" [1 2 3 4] 4]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\302\\\\266\\\\2\\\\207\\\" [1 2 3] 3]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\301\\\\302\\\\266\\\\202\\\\207\\\" [1 2 3] 3]\")))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\242\\\\10\\\\243\\\\104\\\\207\\\" [x] 2]\")) '(1 2 3))
             (funcall (car (read-from-string \"#[(x) \\\"\\\\10\\\\242\\\\10\\\\243\\\\104\\\\207\\\" [x] 2]\")) 'a)
             (funcall (car (read-from-string \"#[(x y) \\\"\\\\10\\\\11\\\\234\\\\207\\\" [x y] 2]\")) [10 20 30] 1)
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\202\\\\5\\\\0\\\\301\\\\207\\\" [10 20] 1]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\61\\\\6\\\\0\\\\301\\\\60\\\\207\\\" [0 99] 2]\")))
             (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\62\\\\6\\\\0\\\\301\\\\60\\\\207\\\" [0 77] 2]\")))",
        );
        assert_eq!(results[0], "OK t");
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK t");
        assert_eq!(results[3], "OK t");
        assert_eq!(results[4], "OK \"ab\"");
        assert_eq!(results[5], "OK \"bcd\"");
        assert_eq!(results[6], "OK 3");
        assert_eq!(results[7], "OK b");
        assert_eq!(results[8], "OK (b c)");
        assert_eq!(results[9], "OK t");
        assert_eq!(results[10], "OK t");
        assert_eq!(results[11], "OK 2");
        assert_eq!(results[12], "OK [1 9 3]");
        assert_eq!(results[13], "OK 3");
        assert_eq!(results[14], "OK 1");
        assert_eq!(results[15], "OK (a b c 1 2 3)");
        assert_eq!(results[16], "OK (b c)");
        assert_eq!(results[17], "OK (3 2 1)");
        assert_eq!(results[18], "OK nil");
        assert_eq!(results[19], "OK (3 2 1)");
        assert_eq!(results[20], "OK (1 2)");
        assert_eq!(results[21], "OK (1 2 3)");
        assert_eq!(results[22], "OK 2");
        assert_eq!(results[23], "OK 3");
        assert_eq!(results[24], "OK [1 2]");
        assert_eq!(results[25], "OK (b . 2)");
        assert_eq!(results[26], "OK (1 2)");
        assert_eq!(results[27], "OK (1 2 3)");
        assert_eq!(results[28], "OK 42.5");
        assert_eq!(results[29], "OK \"42\"");
        assert_eq!(results[30], "OK \"A\"");
        assert_eq!(results[31], "OK 65");
        assert_eq!(results[32], "OK 7.0");
        assert_eq!(results[33], "OK -3");
        assert_eq!(results[34], "OK 4");
        assert_eq!(results[35], "OK 7");
        assert_eq!(results[36], "OK 5");
        assert_eq!(results[37], "OK -1");
        assert_eq!(results[38], "OK 12");
        assert_eq!(results[39], "OK (1 2 3 4 5 6 7 8)");
        assert_eq!(results[40], "OK (2 8)");
        assert_eq!(results[41], "OK 1");
        assert_eq!(results[42], "OK 14");
        assert_eq!(results[43], "OK 1");
        assert_eq!(results[44], "OK -7");
        assert_eq!(results[45], "OK 99");
        assert_eq!(results[46], "OK 42");
        assert_eq!(results[47], "OK 64");
        assert_eq!(results[48], "OK \"abcde\"");
        assert_eq!(results[49], "OK (1 3)");
        assert_eq!(results[50], "OK (1 4 3)");
        assert_eq!(results[51], "OK 1");
        assert_eq!(results[52], "OK 3");
        assert_eq!(results[53], "OK (1 (2 3))");
        assert_eq!(results[54], "OK (nil nil)");
        assert_eq!(results[55], "OK 20");
        assert_eq!(results[56], "OK 10");
        assert_eq!(results[57], "OK 99");
        assert_eq!(results[58], "OK 77");
    }

    #[cfg(feature = "legacy-elc-literal")]
    #[test]
    fn calling_compiled_literal_placeholder_signals_error() {
        let result = eval_one(
            "(progn
               (defalias 'vm-elc-placeholder #[(x) \"\\377\\207\" [x] 1 (#$ . 83)])
               (condition-case err
                   (vm-elc-placeholder 1)
                 (error (car err))))",
        );
        assert_eq!(result, "OK error");
    }

    #[cfg(not(feature = "legacy-elc-literal"))]
    #[test]
    fn compiled_literal_reader_form_is_not_callable_by_default() {
        let result = eval_one(
            "(condition-case err
                 (funcall (car (read-from-string \"#[nil \\\"\\\\300\\\\207\\\" [42] 1]\")))
               (error (car err)))",
        );
        assert_eq!(result, "OK invalid-function");
    }

    #[test]
    fn provide_require() {
        let forms = parse_forms("(provide 'my-feature) (featurep 'my-feature)").expect("parse");
        let mut ev = Evaluator::new();
        let results: Vec<String> = ev
            .eval_forms(&forms)
            .iter()
            .map(format_eval_result)
            .collect();
        assert_eq!(results[0], "OK my-feature");
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn default_directory_is_bound_to_directory_path() {
        let results = eval_all(
            "(stringp default-directory)
             (file-directory-p default-directory)
             (string-suffix-p \"/\" default-directory)",
        );
        assert_eq!(results[0], "OK t");
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK t");
    }

    #[test]
    fn features_variable_controls_featurep_and_require() {
        let results = eval_all(
            "(setq features '(vm-existing))
             (featurep 'vm-existing)
             (require 'vm-existing)",
        );
        assert_eq!(results[0], "OK (vm-existing)");
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK vm-existing");
    }

    #[test]
    fn provide_preserves_features_variable_entries() {
        let results = eval_all(
            "(setq features '(vm-existing))
             (provide 'vm-new)
             features",
        );
        assert_eq!(results[0], "OK (vm-existing)");
        assert_eq!(results[1], "OK vm-new");
        assert_eq!(results[2], "OK (vm-new vm-existing)");
    }

    #[test]
    fn require_recursive_cycle_signals_error() {
        use std::fs;
        use std::time::{SystemTime, UNIX_EPOCH};

        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock before epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("neovm-require-recursive-{unique}"));
        fs::create_dir_all(&dir).expect("create fixture dir");
        fs::write(
            dir.join("vm-rec-a.el"),
            "(require 'vm-rec-b)\n(provide 'vm-rec-a)\n",
        )
        .expect("write vm-rec-a");
        fs::write(
            dir.join("vm-rec-b.el"),
            "(require 'vm-rec-a)\n(provide 'vm-rec-b)\n",
        )
        .expect("write vm-rec-b");

        let escaped = dir.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\"");
        let script = format!(
            "(progn (setq load-path (cons \"{}\" load-path)) 'ok)\n\
             (condition-case err (require 'vm-rec-a) (error (car err)))\n\
             (featurep 'vm-rec-a)\n\
             (featurep 'vm-rec-b)",
            escaped
        );
        let results = eval_all(&script);
        assert_eq!(results[1], "OK error");
        assert_eq!(results[2], "OK nil");
        assert_eq!(results[3], "OK nil");

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn dotimes_loop() {
        let result = eval_one("(let ((sum 0)) (dotimes (i 5) (setq sum (+ sum i))) sum)");
        assert_eq!(result, "OK 10"); // 0+1+2+3+4 = 10
    }

    #[test]
    fn dolist_loop() {
        let result = eval_one(
            "(let ((result nil)) (dolist (x '(a b c)) (setq result (cons x result))) result)",
        );
        assert_eq!(result, "OK (c b a)");
    }

    #[test]
    fn ignore_errors_catches_signal() {
        let result = eval_one("(ignore-errors (/ 1 0) 42)");
        assert_eq!(result, "OK nil"); // error caught, returns nil
    }

    #[test]
    fn math_functions() {
        assert_eq!(eval_one("(expt 2 10)"), "OK 1024");
        assert_eq!(eval_one("(sqrt 4.0)"), "OK 2.0");
    }

    #[test]
    fn hook_system() {
        let results = eval_all(
            "(defvar my-hook nil)
             (defun hook-fn () 42)
             (add-hook 'my-hook 'hook-fn)
             (run-hooks 'my-hook)",
        );
        assert_eq!(results[3], "OK nil"); // run-hooks returns nil
    }

    #[test]
    fn symbol_operations() {
        let results = eval_all(
            "(defvar x 42)
             (boundp 'x)
             (symbol-value 'x)
             (put 'x 'doc \"A variable\")
             (get 'x 'doc)",
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK 42");
        assert_eq!(results[4], r#"OK "A variable""#);
    }

    // -- Buffer operations -------------------------------------------------

    #[test]
    fn buffer_create_and_switch() {
        let results = eval_all(
            "(get-buffer-create \"test-buf\")
             (set-buffer \"test-buf\")
             (buffer-name)
             (bufferp (current-buffer))",
        );
        assert!(results[0].starts_with("OK #<buffer"));
        assert!(results[1].starts_with("OK #<buffer"));
        assert_eq!(results[2], r#"OK "test-buf""#);
        assert_eq!(results[3], "OK t");
    }

    #[test]
    fn buffer_insert_and_point() {
        let results = eval_all(
            "(get-buffer-create \"ed\")
             (set-buffer \"ed\")
             (insert \"hello\")
             (point)
             (goto-char 1)
             (point)
             (buffer-string)
             (point-min)
             (point-max)",
        );
        assert_eq!(results[3], "OK 6"); // after inserting "hello", point is 6 (1-based)
        assert_eq!(results[5], "OK 1"); // after goto-char 1
        assert_eq!(results[6], r#"OK "hello""#);
        assert_eq!(results[7], "OK 1"); // point-min
        assert_eq!(results[8], "OK 6"); // point-max
    }

    #[test]
    fn buffer_delete_region() {
        let results = eval_all(
            "(get-buffer-create \"del\")
             (set-buffer \"del\")
             (insert \"abcdef\")
             (delete-region 2 5)
             (buffer-string)",
        );
        assert_eq!(results[4], r#"OK "aef""#);
    }

    #[test]
    fn buffer_erase() {
        let results = eval_all(
            "(get-buffer-create \"era\")
             (set-buffer \"era\")
             (insert \"stuff\")
             (erase-buffer)
             (buffer-string)
             (buffer-size)",
        );
        assert_eq!(results[4], r#"OK """#);
        assert_eq!(results[5], "OK 0");
    }

    #[test]
    fn buffer_narrowing() {
        let results = eval_all(
            "(get-buffer-create \"nar\")
             (set-buffer \"nar\")
             (insert \"hello world\")
             (narrow-to-region 7 12)
             (buffer-string)
             (widen)
             (buffer-string)",
        );
        assert_eq!(results[4], r#"OK "world""#);
        assert_eq!(results[6], r#"OK "hello world""#);
    }

    #[test]
    fn buffer_modified_p() {
        let results = eval_all(
            "(get-buffer-create \"mod\")
             (set-buffer \"mod\")
             (buffer-modified-p)
             (insert \"x\")
             (buffer-modified-p)
             (set-buffer-modified-p nil)
             (buffer-modified-p)",
        );
        assert_eq!(results[2], "OK nil");
        assert_eq!(results[4], "OK t");
        assert_eq!(results[6], "OK nil");
    }

    #[test]
    fn buffer_mark() {
        let results = eval_all(
            "(get-buffer-create \"mk\")
             (set-buffer \"mk\")
             (insert \"hello\")
             (set-mark 3)
             (mark)",
        );
        assert_eq!(results[4], "OK 3");
    }

    #[test]
    fn buffer_with_current_buffer() {
        let results = eval_all(
            "(get-buffer-create \"a\")
             (get-buffer-create \"b\")
             (set-buffer \"a\")
             (insert \"in-a\")
             (with-current-buffer \"b\"
               (insert \"in-b\")
               (buffer-string))
             (buffer-name)
             (buffer-string)",
        );
        // with-current-buffer should switch to b, insert, get string, then restore a
        assert_eq!(results[4], r#"OK "in-b""#);
        assert_eq!(results[5], r#"OK "a""#); // current buffer restored
        assert_eq!(results[6], r#"OK "in-a""#); // a's content unchanged
    }

    #[test]
    fn buffer_save_excursion() {
        let results = eval_all(
            "(get-buffer-create \"se\")
             (set-buffer \"se\")
             (insert \"abcdef\")
             (goto-char 3)
             (save-excursion
               (goto-char 1)
               (insert \"X\"))
             (point)",
        );
        // save-excursion restores point to 3
        assert_eq!(results[5], "OK 3");
    }

    #[test]
    fn buffer_char_after_before() {
        let results = eval_all(
            "(get-buffer-create \"cb\")
             (set-buffer \"cb\")
             (insert \"abc\")
             (goto-char 2)
             (char-after)
             (char-before)",
        );
        assert_eq!(results[4], "OK 98"); // ?b = 98
        assert_eq!(results[5], "OK 97"); // ?a = 97
    }

    #[test]
    fn buffer_list_and_kill() {
        let results = eval_all(
            "(get-buffer-create \"kill-me\")
             (kill-buffer \"kill-me\")
             (get-buffer \"kill-me\")",
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn buffer_generate_new_buffer() {
        let results = eval_all(
            "(buffer-name (generate-new-buffer \"test\"))
             (buffer-name (generate-new-buffer \"test\"))",
        );
        assert_eq!(results[0], r#"OK "test""#);
        assert_eq!(results[1], r#"OK "test<2>""#);
    }
}
