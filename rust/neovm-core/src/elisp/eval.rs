//! Evaluator — special forms, function application, and dispatch.

use std::collections::HashMap;

use super::abbrev::AbbrevManager;
use super::advice::{AdviceManager, VariableWatcherList};
use super::autoload::AutoloadManager;
use super::bookmark::BookmarkManager;
use super::builtins;
use super::custom::CustomManager;
use super::error::*;
use super::expr::Expr;
use super::interactive::InteractiveRegistry;
use super::mode::ModeRegistry;
use super::keymap::KeymapManager;
use super::kill_ring::KillRing;
use super::network::NetworkManager;
use super::process::ProcessManager;
use super::regex::MatchData;
use super::register::RegisterManager;
use super::symbol::Obarray;
use super::timer::TimerManager;
use super::value::*;
use crate::buffer::BufferManager;
use crate::window::FrameManager;

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
    /// Interactive command registry — tracks interactive commands.
    pub(crate) interactive: InteractiveRegistry,
    /// Frame manager — owns all frames and windows.
    pub(crate) frames: FrameManager,
    /// Mode registry — major/minor modes.
    pub(crate) modes: ModeRegistry,
    /// Recursion depth counter.
    depth: usize,
    /// Maximum recursion depth.
    max_depth: usize,
}

impl Default for Evaluator {
    fn default() -> Self {
        Self::new()
    }
}

impl Evaluator {
    pub fn new() -> Self {
        let mut obarray = Obarray::new();

        // Set up standard global variables
        obarray.set_symbol_value("most-positive-fixnum", Value::Int(i64::MAX));
        obarray.set_symbol_value("most-negative-fixnum", Value::Int(i64::MIN));
        obarray.set_symbol_value("emacs-version", Value::string("29.1"));
        obarray.set_symbol_value("system-type", Value::symbol("gnu/linux"));
        obarray.set_symbol_value("load-path", Value::Nil);
        obarray.set_symbol_value("features", Value::Nil);
        obarray.set_symbol_value("debug-on-error", Value::Nil);
        obarray.set_symbol_value("lexical-binding", Value::Nil);
        obarray.set_symbol_value("load-file-name", Value::Nil);
        obarray.set_symbol_value("noninteractive", Value::True);
        obarray.set_symbol_value("inhibit-quit", Value::Nil);
        obarray.set_symbol_value("print-length", Value::Nil);
        obarray.set_symbol_value("print-level", Value::Nil);

        // Mark standard variables as special (dynamically bound)
        for name in &["debug-on-error", "lexical-binding", "load-path", "features",
                      "load-file-name", "noninteractive", "inhibit-quit",
                      "print-length", "print-level"] {
            obarray.make_special(name);
        }

        // Initialize the standard error hierarchy (error, user-error, etc.)
        super::errors::init_standard_errors(&mut obarray);

        Self {
            obarray,
            dynamic: Vec::new(),
            lexenv: Vec::new(),
            features: Vec::new(),
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
            custom: CustomManager::new(),
            kill_ring: KillRing::new(),
            interactive: InteractiveRegistry::new(),
            frames: FrameManager::new(),
            modes: ModeRegistry::new(),
            depth: 0,
            max_depth: 200,
        }
    }

    /// Whether lexical-binding is currently enabled.
    pub fn lexical_binding(&self) -> bool {
        self.obarray.symbol_value("lexical-binding")
            .is_some_and(|v| v.is_truthy())
    }

    /// Enable or disable lexical binding.
    pub fn set_lexical_binding(&mut self, enabled: bool) {
        self.obarray.set_symbol_value("lexical-binding", Value::bool(enabled));
    }

    /// Load a file, converting EvalError back to Flow for use in special forms.
    pub(crate) fn load_file_internal(&mut self, path: &std::path::Path) -> EvalResult {
        super::load::load_file(self, path).map_err(|e| match e {
            EvalError::Signal { symbol, data } => {
                signal(&symbol, data)
            }
            EvalError::UncaughtThrow { tag, value } => {
                Flow::Throw { tag, value }
            }
        })
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
            return Err(signal("excessive-lisp-nesting", vec![Value::Int(self.max_depth as i64)]));
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
                if let Value::Macro(_) = &func {
                    let expanded = self.expand_macro(func, tail)?;
                    return self.eval(&expanded);
                }
            }

            // Special forms
            if let Some(result) = self.try_special_form(name, tail) {
                return result;
            }

            // Regular function call — evaluate args then dispatch
            let mut args = Vec::with_capacity(tail.len());
            for expr in tail {
                args.push(self.eval(expr)?);
            }

            // Try builtin dispatch first
            if let Some(result) = builtins::dispatch_builtin(self, name, args.clone()) {
                return result;
            }

            // Try obarray function cell (including defalias chains)
            if let Some(func) = self.obarray.symbol_function(name).cloned() {
                return self.apply(func, args);
            }

            // Try indirect function resolution (defalias)
            if let Some(func) = self.obarray.indirect_function(name) {
                return self.apply(func, args);
            }

            return Err(signal("void-function", vec![Value::symbol(name.clone())]));
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
            "define-obsolete-function-alias" => super::autoload::sf_define_obsolete_function_alias(self, tail),
            "define-obsolete-variable-alias" => super::autoload::sf_define_obsolete_variable_alias(self, tail),
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
            // Interactive / mode definition special forms
            "define-minor-mode" => super::interactive::sf_define_minor_mode(self, tail),
            "define-derived-mode" => super::interactive::sf_define_derived_mode(self, tail),
            "define-generic-mode" => super::interactive::sf_define_generic_mode(self, tail),
            _ => return None,
        })
    }

    fn sf_quote(&self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        Ok(quote_to_value(&tail[0]))
    }

    fn sf_function(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() != 1 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        match &tail[0] {
            Expr::Symbol(name) => {
                // #'symbol — look up in function namespace (obarray function cell)
                if let Some(func) = self.obarray.symbol_function(name).cloned() {
                    Ok(func)
                } else {
                    // Assume it's a builtin (Subr)
                    Ok(Value::Subr(name.clone()))
                }
            }
            Expr::List(items) => {
                // #'(lambda ...) — create closure
                if let Some(Expr::Symbol(s)) = items.first() {
                    if s == "lambda" {
                        return self.eval_lambda(&items[1..]);
                    }
                }
                Err(signal("invalid-function", vec![]))
            }
            _ => Err(signal("invalid-function", vec![])),
        }
    }

    fn sf_let(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
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
                                return Err(signal("wrong-type-argument", vec![]));
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
            _ => return Err(signal("wrong-type-argument", vec![])),
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let entries = match &tail[0] {
            Expr::List(entries) => entries.clone(),
            Expr::Symbol(s) if s == "nil" => Vec::new(),
            _ => return Err(signal("wrong-type-argument", vec![])),
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
                        if pushed_lex { self.lexenv.pop(); }
                        self.dynamic.pop();
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    let value = if pair.len() > 1 {
                        match self.eval(&pair[1]) {
                            Ok(v) => v,
                            Err(e) => {
                                if pushed_lex { self.lexenv.pop(); }
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
                    if pushed_lex { self.lexenv.pop(); }
                    self.dynamic.pop();
                    return Err(signal("wrong-type-argument", vec![]));
                }
            }
        }

        let result = self.sf_progn(&tail[1..]);
        if pushed_dyn { self.dynamic.pop(); }
        if pushed_lex { self.lexenv.pop(); }
        result
    }

    fn sf_setq(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Ok(Value::Nil);
        }
        if tail.len() % 2 != 0 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let mut last = Value::Nil;
        let mut i = 0;
        while i < tail.len() {
            let Expr::Symbol(name) = &tail[i] else {
                return Err(signal("wrong-type-argument", vec![]));
            };
            let value = self.eval(&tail[i + 1])?;
            self.assign(name, value.clone());
            last = value;
            i += 2;
        }
        Ok(last)
    }

    fn sf_if(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
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
                return Err(signal("wrong-type-argument", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let first = self.eval(&tail[0])?;
        for form in &tail[1..] {
            self.eval(form)?;
        }
        Ok(first)
    }

    fn sf_when(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let cond = self.eval(&tail[0])?;
        if cond.is_nil() {
            self.sf_progn(&tail[1..])
        } else {
            Ok(Value::Nil)
        }
    }

    fn sf_defun(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 3 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        let lambda = self.eval_lambda(&tail[1..])?;
        self.obarray.set_symbol_function(name, lambda);
        Ok(Value::symbol(name.clone()))
    }

    fn sf_defvar(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        let value = self.eval(&tail[1])?;
        self.obarray.set_symbol_value(name, value);
        let sym = self.obarray.get_or_intern(name);
        sym.constant = true;
        sym.special = true;
        Ok(Value::symbol(name.clone()))
    }

    fn sf_defmacro(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 3 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let Expr::Symbol(name) = &tail[0] else {
            return Err(signal("wrong-type-argument", vec![]));
        };
        let params = self.parse_lambda_params(&tail[1])?;
        let body = tail[2..].to_vec();
        let macro_val = Value::Macro(std::sync::Arc::new(LambdaData {
            params,
            body,
            env: None,
            docstring: None,
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
            return Err(signal("wrong-number-of-arguments", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let tag = self.eval(&tail[0])?;
        let value = self.eval(&tail[1])?;
        Err(Flow::Throw { tag, value })
    }

    fn sf_unwind_protect(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
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
            return Err(signal("wrong-number-of-arguments", vec![]));
        }

        let var = match &tail[0] {
            Expr::Symbol(name) => name.clone(),
            _ => return Err(signal("wrong-type-argument", vec![])),
        };
        let body = &tail[1];
        let handlers = &tail[2..];

        match self.eval(body) {
            Ok(value) => Ok(value),
            Err(Flow::Signal(sig)) => {
                for handler in handlers {
                    let Expr::List(handler_items) = handler else {
                        return Err(signal("wrong-type-argument", vec![]));
                    };
                    if handler_items.is_empty() {
                        return Err(signal("wrong-type-argument", vec![]));
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
            Err(flow) => Err(flow),
        }
    }

    fn sf_defalias(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.len() < 2 {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let sym = self.eval(&tail[0])?;
        let def = self.eval(&tail[1])?;
        let name = match &sym {
            Value::Symbol(s) => s.clone(),
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("symbolp"), sym])),
        };
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
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("symbolp"), feature])),
        };
        if !self.features.contains(&name) {
            self.features.push(name);
        }
        Ok(feature)
    }

    fn sf_require(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let feature = self.eval(&tail[0])?;
        let name = match &feature {
            Value::Symbol(s) => s.clone(),
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("symbolp"), feature])),
        };
        if self.features.contains(&name) {
            return Ok(Value::symbol(name));
        }

        // Try to find and load the file
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
                if self.features.contains(&name) {
                    Ok(Value::symbol(name))
                } else {
                    Err(signal("file-missing", vec![
                        Value::string(format!("Required feature '{}' was not provided", name))
                    ]))
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
                Err(signal("file-missing", vec![
                    Value::string(format!("Cannot open load file: no such file or directory, {}", name))
                ]))
            }
        }
    }

    fn sf_with_current_buffer(&mut self, tail: &[Expr]) -> EvalResult {
        if tail.is_empty() {
            return Err(signal("wrong-number-of-arguments", vec![]));
        }
        let buf_val = self.eval(&tail[0])?;
        let target_id = match &buf_val {
            Value::Buffer(id) => *id,
            Value::Str(s) => {
                self.buffers.find_buffer_by_name(s)
                    .ok_or_else(|| signal("error", vec![Value::string(format!("No buffer named {s}"))]))?
            }
            other => return Err(signal("wrong-type-argument",
                vec![Value::symbol("bufferp"), other.clone()])),
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
            _ => return Err(signal("wrong-type-argument", vec![Value::symbol("integerp"), count])),
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

        // Skip docstring if present
        let body_start = if tail.len() > 2 {
            if let Expr::Str(_) = &tail[1] { 2 } else { 1 }
        } else {
            1
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
            docstring: None,
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
                        "&optional" => { mode = 1; continue; }
                        "&rest" => { mode = 2; continue; }
                        _ => {}
                    }
                    match mode {
                        0 => required.push(name.clone()),
                        1 => optional.push(name.clone()),
                        2 => { rest = Some(name.clone()); break; }
                        _ => unreachable!(),
                    }
                }

                Ok(LambdaParams { required, optional, rest })
            }
            _ => Err(signal("wrong-type-argument", vec![])),
        }
    }

    /// Apply a function value to evaluated arguments.
    pub(crate) fn apply(&mut self, function: Value, args: Vec<Value>) -> EvalResult {
        match function {
            Value::ByteCode(bc) => {
                let mut vm = super::bytecode::Vm::new(
                    &mut self.obarray,
                    &mut self.dynamic,
                    &mut self.lexenv,
                    &mut self.features,
                );
                vm.execute(&bc, args)
            }
            Value::Lambda(lambda) | Value::Macro(lambda) => {
                self.apply_lambda(&lambda, args)
            }
            Value::Subr(name) => {
                // Try obarray function cell first
                if let Some(func) = self.obarray.symbol_function(&name).cloned() {
                    return self.apply(func, args);
                }
                if let Some(result) = builtins::dispatch_builtin(self, &name, args) {
                    result
                } else {
                    Err(signal("void-function", vec![Value::symbol(name)]))
                }
            }
            Value::Symbol(name) => {
                // Symbol used as function — look up in obarray function cell
                if let Some(func) = self.obarray.symbol_function(&name).cloned() {
                    self.apply(func, args)
                } else if let Some(result) = builtins::dispatch_builtin(self, &name, args) {
                    result
                } else {
                    Err(signal("void-function", vec![Value::symbol(name)]))
                }
            }
            _ => Err(signal("invalid-function", vec![function])),
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

        let result = self.sf_progn(&lambda.body);

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
        // Fall through to obarray value cell
        self.obarray.set_symbol_value(name, value);
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
            head_vals.into_iter().rev().fold(tail_val, |acc, item| {
                Value::cons(item, acc)
            })
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
    use crate::elisp::{parse_forms, format_eval_result};

    fn eval_one(src: &str) -> String {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        let result = ev.eval_expr(&forms[0]);
        format_eval_result(&result)
    }

    fn eval_all(src: &str) -> Vec<String> {
        let forms = parse_forms(src).expect("parse");
        let mut ev = Evaluator::new();
        ev.eval_forms(&forms).iter().map(format_eval_result).collect()
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
        assert_eq!(eval_one(r#"(concat "hello" " " "world")"#), r#"OK "hello world""#);
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
    fn defmacro_works() {
        let result = eval_all(
            "(defmacro my-when (cond &rest body)
               (list 'if cond (cons 'progn body)))
             (my-when t 1 2 3)",
        );
        assert_eq!(result[1], "OK 3");
    }

    #[test]
    fn optional_and_rest_params() {
        let results = eval_all(
            "(defun f (a &optional b &rest c) (list a b c))
             (f 1)
             (f 1 2)
             (f 1 2 3 4)"
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
               (gethash \"key\" ht))"
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
        assert_eq!(eval_one(r#"(format "hello %s, %d" "world" 42)"#), r#"OK "hello world, 42""#);
    }

    #[test]
    fn prog1() {
        assert_eq!(eval_one("(prog1 1 2 3)"), "OK 1");
    }

    #[test]
    fn function_special_form() {
        let results = eval_all(
            "(defun add1 (x) (+ x 1))
             (funcall #'add1 5)"
        );
        assert_eq!(results[1], "OK 6");
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
        let rendered: Vec<String> = ev.eval_forms(&forms).iter().map(format_eval_result).collect();

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
        let forms = parse_forms(r#"
            (let ((x 1))
              (let ((f (lambda () x)))
                (let ((x 2))
                  (funcall f))))
        "#).expect("parse");
        let mut ev = Evaluator::new();
        ev.set_lexical_binding(true);
        let result = format_eval_result(&ev.eval_expr(&forms[0]));
        // In lexical binding, the closure captures x=1, not x=2
        assert_eq!(result, "OK 1");
    }

    #[test]
    fn dynamic_binding_closure() {
        // Without lexical binding (default), closures see dynamic scope
        let forms = parse_forms(r#"
            (let ((x 1))
              (let ((f (lambda () x)))
                (let ((x 2))
                  (funcall f))))
        "#).expect("parse");
        let mut ev = Evaluator::new();
        let result = format_eval_result(&ev.eval_expr(&forms[0]));
        // In dynamic binding, the lambda sees x=2 (innermost dynamic binding)
        assert_eq!(result, "OK 2");
    }

    #[test]
    fn lexical_binding_special_var_stays_dynamic() {
        // defvar makes a variable special — it stays dynamically scoped
        let forms = parse_forms(r#"
            (defvar my-special 10)
            (let ((my-special 20))
              (let ((f (lambda () my-special)))
                (let ((my-special 30))
                  (funcall f))))
        "#).expect("parse");
        let mut ev = Evaluator::new();
        ev.set_lexical_binding(true);
        let results: Vec<String> = ev.eval_forms(&forms).iter().map(format_eval_result).collect();
        // my-special is declared special, so even in lexical mode it's dynamic
        assert_eq!(results[1], "OK 30");
    }

    #[test]
    fn defalias_works() {
        let results = eval_all(
            "(defun my-add (a b) (+ a b))
             (defalias 'my-plus 'my-add)
             (my-plus 3 4)"
        );
        assert_eq!(results[2], "OK 7");
    }

    #[test]
    fn provide_require() {
        let forms = parse_forms("(provide 'my-feature) (featurep 'my-feature)").expect("parse");
        let mut ev = Evaluator::new();
        let results: Vec<String> = ev.eval_forms(&forms).iter().map(format_eval_result).collect();
        assert_eq!(results[0], "OK my-feature");
        assert_eq!(results[1], "OK t");
    }

    #[test]
    fn dotimes_loop() {
        let result = eval_one(
            "(let ((sum 0)) (dotimes (i 5) (setq sum (+ sum i))) sum)"
        );
        assert_eq!(result, "OK 10"); // 0+1+2+3+4 = 10
    }

    #[test]
    fn dolist_loop() {
        let result = eval_one(
            "(let ((result nil)) (dolist (x '(a b c)) (setq result (cons x result))) result)"
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
             (run-hooks 'my-hook)"
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
             (get 'x 'doc)"
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
             (bufferp (current-buffer))"
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
             (point-max)"
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
             (buffer-string)"
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
             (buffer-size)"
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
             (buffer-string)"
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
             (buffer-modified-p)"
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
             (mark)"
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
             (buffer-string)"
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
             (point)"
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
             (char-before)"
        );
        assert_eq!(results[4], "OK 98"); // ?b = 98
        assert_eq!(results[5], "OK 97"); // ?a = 97
    }

    #[test]
    fn buffer_list_and_kill() {
        let results = eval_all(
            "(get-buffer-create \"kill-me\")
             (kill-buffer \"kill-me\")
             (get-buffer \"kill-me\")"
        );
        assert_eq!(results[1], "OK t");
        assert_eq!(results[2], "OK nil");
    }

    #[test]
    fn buffer_generate_new_buffer() {
        let results = eval_all(
            "(buffer-name (generate-new-buffer \"test\"))
             (buffer-name (generate-new-buffer \"test\"))"
        );
        assert_eq!(results[0], r#"OK "test""#);
        assert_eq!(results[1], r#"OK "test<2>""#);
    }
}
