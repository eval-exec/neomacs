//! Interactive command system and mode definition macros.
//!
//! Implements:
//! - `InteractiveSpec` and `InteractiveRegistry` for tracking which functions
//!   are interactive commands and their argument specifications.
//! - Built-in functions: `call-interactively`, `interactive-p`,
//!   `called-interactively-p`, `commandp`, `command-execute`,
//!   `execute-extended-command`, `key-binding`, `local-key-binding`,
//!   `global-key-binding`, `minor-mode-key-binding`, `where-is-internal`,
//!   `substitute-command-keys`, `describe-key-briefly`, `this-command-keys`,
//!   `this-command-keys-vector`, `thing-at-point`, `bounds-of-thing-at-point`,
//!   `word-at-point`, `symbol-at-point`.
//! - Special forms: `define-minor-mode`, `define-derived-mode`,
//!   `define-generic-mode`.

use std::collections::{HashMap, HashSet};

use super::error::{signal, EvalResult, Flow};
use super::eval::Evaluator;
use super::expr::Expr;
use super::keymap::{KeyBinding, KeymapManager};
use super::mode::{MajorMode, MinorMode};
use super::value::*;

// ---------------------------------------------------------------------------
// InteractiveSpec — describes how a command reads its arguments
// ---------------------------------------------------------------------------

/// Interactive argument specification for a command.
#[derive(Clone, Debug)]
pub struct InteractiveSpec {
    /// Code letter(s) describing argument types, e.g. "r" for region,
    /// "p" for prefix arg, "sPrompt: " for string prompt, etc.
    pub code: String,
    /// Optional prompt string (extracted from the code).
    pub prompt: Option<String>,
}

impl InteractiveSpec {
    /// Create a new interactive spec from a code string.
    pub fn new(code: impl Into<String>) -> Self {
        let code = code.into();
        // Extract prompt from code if it contains a prompt (e.g. "sEnter name: ")
        let prompt = if code.len() > 1 && code.starts_with(|c: char| c.is_ascii_lowercase()) {
            Some(code[1..].to_string())
        } else {
            None
        };
        Self { code, prompt }
    }

    /// Create a spec with no arguments (plain interactive command).
    pub fn no_args() -> Self {
        Self {
            code: String::new(),
            prompt: None,
        }
    }
}

// ---------------------------------------------------------------------------
// InteractiveRegistry — tracks which functions are interactive commands
// ---------------------------------------------------------------------------

/// Registry for interactive command specifications.
///
/// Tracks which named functions are interactive (i.e., can be called via
/// `M-x` or key bindings) and their argument specs.
pub struct InteractiveRegistry {
    /// Map from function name to its interactive spec.
    specs: HashMap<String, InteractiveSpec>,
    /// Stack tracking whether the current function was called interactively.
    interactive_call_stack: Vec<bool>,
    /// The key sequence that invoked the current command (if any).
    this_command_keys: Vec<String>,
}

impl InteractiveRegistry {
    pub fn new() -> Self {
        Self {
            specs: HashMap::new(),
            interactive_call_stack: Vec::new(),
            this_command_keys: Vec::new(),
        }
    }

    /// Register a function as interactive with the given spec.
    pub fn register_interactive(&mut self, name: &str, spec: InteractiveSpec) {
        self.specs.insert(name.to_string(), spec);
    }

    /// Check if a function is registered as interactive.
    pub fn is_interactive(&self, name: &str) -> bool {
        self.specs.contains_key(name)
    }

    /// Get the interactive spec for a function, if registered.
    pub fn get_spec(&self, name: &str) -> Option<&InteractiveSpec> {
        self.specs.get(name)
    }

    /// Push an interactive call frame.
    pub fn push_interactive_call(&mut self, is_interactive: bool) {
        self.interactive_call_stack.push(is_interactive);
    }

    /// Pop an interactive call frame.
    pub fn pop_interactive_call(&mut self) {
        self.interactive_call_stack.pop();
    }

    /// Check if the current function was called interactively.
    pub fn is_called_interactively(&self) -> bool {
        self.interactive_call_stack.last().copied().unwrap_or(false)
    }

    /// Set the key sequence that invoked the current command.
    pub fn set_this_command_keys(&mut self, keys: Vec<String>) {
        self.this_command_keys = keys;
    }

    /// Get the key sequence that invoked the current command.
    pub fn this_command_keys(&self) -> &[String] {
        &self.this_command_keys
    }
}

impl Default for InteractiveRegistry {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Expect helpers (local to this module)
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Built-in functions (evaluator-dependent)
// ---------------------------------------------------------------------------

/// `(call-interactively FUNCTION &optional RECORD-FLAG KEYS)`
/// Call FUNCTION interactively, reading arguments according to its
/// interactive spec.
pub(crate) fn builtin_call_interactively(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("call-interactively", &args, 1)?;

    let func_val = &args[0];
    if !command_designator_p(eval, func_val) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("commandp"), func_val.clone()],
        ));
    }
    let Some((resolved_name, func)) = resolve_command_target(eval, func_val) else {
        return Err(signal("void-function", vec![func_val.clone()]));
    };
    let call_args = default_call_interactively_args(eval, &resolved_name)?;

    // Mark as interactive call
    eval.interactive.push_interactive_call(true);

    // Call the function with no args (interactive arg reading is stubbed)
    let result = eval.apply(func, call_args);

    eval.interactive.pop_interactive_call();
    result
}

/// `(interactive-p)` -> t if the calling function was called interactively.
pub(crate) fn builtin_interactive_p(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("interactive-p", &args, 0)?;
    let _ = eval;
    // Emacs 30 keeps `interactive-p` obsolete; it effectively returns nil.
    Ok(Value::Nil)
}

/// `(called-interactively-p &optional KIND)`
/// Return t if the calling function was called interactively.
/// KIND can be 'interactive or 'any.
pub(crate) fn builtin_called_interactively_p(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    // Accept 0 or 1 args
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![
                Value::symbol("called-interactively-p"),
                Value::Int(args.len() as i64),
            ],
        ));
    }
    if !eval.interactive.is_called_interactively() {
        return Ok(Value::Nil);
    }

    // GNU Emacs semantics:
    // - KIND = 'interactive => nil
    // - KIND = nil / 'any / unknown => t (when called interactively)
    if args
        .first()
        .is_some_and(|v| matches!(v, Value::Symbol(s) if s == "interactive"))
    {
        Ok(Value::Nil)
    } else {
        Ok(Value::True)
    }
}

/// `(commandp FUNCTION &optional FOR-CALL-INTERACTIVELY)`
/// Return non-nil if FUNCTION is a command (i.e., can be called interactively).
pub(crate) fn builtin_commandp_interactive(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("commandp", &args, 1)?;
    let is_command = command_designator_p(eval, &args[0]);
    Ok(Value::bool(is_command))
}

fn builtin_command_name(name: &str) -> bool {
    matches!(
        name,
        "ignore"
            | "eval-expression"
            | "self-insert-command"
            | "newline"
            | "execute-extended-command"
            | "forward-char"
            | "backward-char"
            | "delete-char"
            | "next-line"
            | "previous-line"
            | "kill-line"
            | "kill-word"
            | "backward-kill-word"
            | "kill-region"
            | "kill-ring-save"
            | "kill-whole-line"
            | "copy-region-as-kill"
            | "yank"
            | "yank-pop"
            | "transpose-chars"
            | "transpose-lines"
            | "transpose-paragraphs"
            | "transpose-sentences"
            | "transpose-sexps"
            | "transpose-words"
            | "open-line"
            | "delete-horizontal-space"
            | "just-one-space"
            | "delete-indentation"
            | "indent-for-tab-command"
            | "upcase-word"
            | "downcase-word"
            | "capitalize-word"
            | "upcase-region"
            | "downcase-region"
            | "capitalize-region"
            | "upcase-initials-region"
            | "switch-to-buffer"
            | "find-file"
            | "save-buffer"
            | "set-mark-command"
            | "recenter-top-bottom"
            | "scroll-up-command"
            | "scroll-down-command"
            | "other-window"
            | "keyboard-quit"
            | "quoted-insert"
            | "universal-argument"
            | "beginning-of-line"
            | "end-of-line"
            | "move-beginning-of-line"
            | "move-end-of-line"
    )
}

fn expr_is_interactive_form(expr: &Expr) -> bool {
    match expr {
        Expr::List(items) => items
            .first()
            .is_some_and(|head| matches!(head, Expr::Symbol(sym) if sym == "interactive")),
        _ => false,
    }
}

fn lambda_body_has_interactive_form(body: &[Expr]) -> bool {
    let mut body_index = 0;
    if matches!(body.first(), Some(Expr::Str(_))) {
        body_index = 1;
    }
    body.get(body_index).is_some_and(expr_is_interactive_form)
}

fn value_list_to_vec(list: &Value) -> Option<Vec<Value>> {
    let mut values = Vec::new();
    let mut cursor = list.clone();
    loop {
        match cursor {
            Value::Nil => return Some(values),
            Value::Cons(cell) => {
                let pair = cell.lock().expect("poisoned");
                values.push(pair.car.clone());
                cursor = pair.cdr.clone();
            }
            _ => return None,
        }
    }
}

fn value_is_interactive_form(value: &Value) -> bool {
    match value {
        Value::Cons(cell) => {
            let pair = cell.lock().expect("poisoned");
            pair.car.as_symbol_name() == Some("interactive")
        }
        _ => false,
    }
}

fn quoted_lambda_has_interactive_form(value: &Value) -> bool {
    let Some(items) = value_list_to_vec(value) else {
        return false;
    };
    if items.first().and_then(Value::as_symbol_name) != Some("lambda") {
        return false;
    }

    let mut body_index = 2;
    if matches!(items.get(body_index), Some(Value::Str(_))) {
        body_index += 1;
    }

    items
        .get(body_index)
        .is_some_and(value_is_interactive_form)
}

fn resolve_function_designator_symbol(eval: &Evaluator, name: &str) -> Option<(String, Value)> {
    let mut current = name.to_string();
    let mut seen = HashSet::new();

    loop {
        if !seen.insert(current.clone()) {
            return None;
        }

        if eval.obarray.is_function_unbound(&current) {
            return None;
        }

        if let Some(function) = eval.obarray.symbol_function(&current) {
            if let Some(next) = function.as_symbol_name() {
                if next == "nil" {
                    return Some((current, Value::Nil));
                }
                current = next.to_string();
                continue;
            }
            return Some((current, function.clone()));
        }

        if let Some(function) = super::subr_info::fallback_macro_value(&current) {
            return Some((current, function));
        }

        if super::subr_info::is_special_form(&current)
            || super::subr_info::is_evaluator_callable_name(&current)
            || super::builtin_registry::is_dispatch_builtin_name(&current)
        {
            return Some((current.clone(), Value::Subr(current)));
        }

        return None;
    }
}

fn command_object_p(eval: &Evaluator, resolved_name: Option<&str>, value: &Value) -> bool {
    if let Some(name) = resolved_name {
        if eval.interactive.is_interactive(name) || builtin_command_name(name) {
            return true;
        }
    }

    match value {
        Value::Lambda(lambda) => lambda_body_has_interactive_form(&lambda.body),
        Value::Cons(_) => quoted_lambda_has_interactive_form(value),
        Value::Subr(name) => eval.interactive.is_interactive(name) || builtin_command_name(name),
        _ => false,
    }
}

fn command_designator_p(eval: &Evaluator, designator: &Value) -> bool {
    if let Some(name) = designator.as_symbol_name() {
        if eval.obarray.is_function_unbound(name) {
            return false;
        }
        if let Some((resolved_name, resolved_value)) = resolve_function_designator_symbol(eval, name) {
            return command_object_p(eval, Some(&resolved_name), &resolved_value);
        }
        return eval.interactive.is_interactive(name) || builtin_command_name(name);
    }
    command_object_p(eval, None, designator)
}

fn interactive_region_args(eval: &Evaluator, missing_mark_signal: &str) -> Result<Vec<Value>, Flow> {
    let buf = eval
        .buffers
        .current_buffer()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    let mark = buf.mark().ok_or_else(|| {
        signal(
            missing_mark_signal,
            vec![Value::string("The mark is not set now, so there is no region")],
        )
    })?;
    let pt = buf.point();
    let beg = pt.min(mark);
    let end = pt.max(mark);
    Ok(vec![Value::Int(beg as i64), Value::Int(end as i64)])
}

fn default_command_execute_args(eval: &Evaluator, name: &str) -> Result<Vec<Value>, Flow> {
    match name {
        "self-insert-command"
        | "delete-char"
        | "kill-word"
        | "backward-kill-word"
        | "downcase-word"
        | "upcase-word"
        | "capitalize-word"
        | "transpose-lines"
        | "transpose-paragraphs"
        | "transpose-sentences"
        | "transpose-sexps"
        | "transpose-words" => Ok(vec![Value::Int(1)]),
        "kill-region" => interactive_region_args(eval, "user-error"),
        "kill-ring-save" => interactive_region_args(eval, "error"),
        "copy-region-as-kill" => interactive_region_args(eval, "error"),
        "capitalize-region" => interactive_region_args(eval, "error"),
        "upcase-initials-region" => interactive_region_args(eval, "error"),
        "upcase-region" | "downcase-region" => {
            Err(signal("args-out-of-range", vec![Value::string(""), Value::Int(0)]))
        }
        _ => Ok(Vec::new()),
    }
}

fn default_call_interactively_args(eval: &Evaluator, name: &str) -> Result<Vec<Value>, Flow> {
    match name {
        "upcase-region" | "downcase-region" | "capitalize-region" => {
            interactive_region_args(eval, "error")
        }
        _ => default_command_execute_args(eval, name),
    }
}

fn resolve_command_target(eval: &Evaluator, designator: &Value) -> Option<(String, Value)> {
    if let Some(name) = designator.as_symbol_name() {
        if let Some((resolved_name, value)) = resolve_function_designator_symbol(eval, name) {
            return Some((resolved_name, value));
        }
        if builtin_command_name(name) {
            return Some((name.to_string(), Value::Subr(name.to_string())));
        }
        return None;
    }
    match designator {
        Value::Subr(name) => Some((name.clone(), designator.clone())),
        Value::True => Some(("t".to_string(), designator.clone())),
        Value::Keyword(name) => Some((name.clone(), designator.clone())),
        _ => Some(("<anonymous>".to_string(), designator.clone())),
    }
}

/// `(command-execute CMD &optional RECORD-FLAG KEYS SPECIAL)`
/// Execute CMD as an editor command.
pub(crate) fn builtin_command_execute(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("command-execute", &args, 1)?;

    let cmd = &args[0];
    if !command_designator_p(eval, cmd) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("commandp"), cmd.clone()],
        ));
    }
    let Some((resolved_name, func)) = resolve_command_target(eval, cmd) else {
        return Err(signal("void-function", vec![cmd.clone()]));
    };
    let call_args = default_command_execute_args(eval, &resolved_name)?;

    eval.interactive.push_interactive_call(true);
    let result = eval.apply(func, call_args);
    eval.interactive.pop_interactive_call();
    result
}

/// `(eval-expression EXPRESSION &optional INSERT-VALUE NO-TRUNCATE LEXICAL)` -- evaluate and
/// return EXPRESSION.
pub(crate) fn builtin_eval_expression(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        if eval.interactive.is_called_interactively() {
            return Err(signal(
                "end-of-file",
                vec![Value::string("Error reading from stdin")],
            ));
        }
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("eval-expression"), Value::Int(0)],
        ));
    }

    let expr = super::eval::value_to_expr_pub(&args[0]);
    eval.eval(&expr)
}

/// `(self-insert-command N)` -- insert the last typed character N times.
///
/// NeoVM currently does not track `last-command-event`; when it is unavailable
/// this command acts as a no-op and returns nil.
pub(crate) fn builtin_self_insert_command(_eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("self-insert-command"), Value::Int(0)],
        ));
    }
    if !matches!(&args[0], Value::Int(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fixnump"), args[0].clone()],
        ));
    }
    Ok(Value::Nil)
}

/// `(keyboard-quit)` -- cancel the current command sequence.
pub(crate) fn builtin_keyboard_quit(_eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("keyboard-quit", &args, 0)?;
    Err(signal("quit", vec![]))
}

/// `(find-file &optional FILENAME WILDCARDS)` -- visit FILENAME.
///
/// In batch mode interactive invocation without FILENAME signals EOF.
pub(crate) fn builtin_find_file_command(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args[0].is_nil() {
        return Err(signal(
            "end-of-file",
            vec![Value::string("Error reading from stdin")],
        ));
    }
    super::fileio::builtin_find_file_noselect(eval, vec![args[0].clone()])
}

/// `(save-buffer &optional ARG)` -- save current buffer.
///
/// In batch mode interactive invocation prompts for a file name and hits EOF.
pub(crate) fn builtin_save_buffer_command(_eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    if args.is_empty() || args[0].is_nil() {
        return Err(signal(
            "end-of-file",
            vec![Value::string("Error reading from stdin")],
        ));
    }
    Ok(Value::Nil)
}

/// `(set-mark-command ARG)` -- set mark and activate region.
pub(crate) fn builtin_set_mark_command(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    let mut push_args = vec![Value::Nil, Value::Nil, Value::True];
    if let Some(arg) = args.first() {
        push_args[0] = arg.clone();
    }
    super::navigation::builtin_push_mark(eval, push_args)
}

/// `(quoted-insert &optional ARG)` -- read a character and insert it.
///
/// In batch mode interactive invocation hits EOF while reading input.
pub(crate) fn builtin_quoted_insert_command(
    _eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    if args.is_empty() || args[0].is_nil() {
        return Err(signal(
            "end-of-file",
            vec![Value::string("Error reading from stdin")],
        ));
    }
    if !matches!(&args[0], Value::Int(_)) {
        return Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("fixnump"), args[0].clone()],
        ));
    }
    Ok(Value::Nil)
}

/// `(universal-argument)` -- initialize a prefix argument command state.
pub(crate) fn builtin_universal_argument_command(
    _eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("universal-argument", &args, 0)?;
    Ok(Value::Lambda(std::sync::Arc::new(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![Expr::Symbol("nil".to_string())],
        env: None,
        docstring: None,
    })))
}

/// `(execute-extended-command PREFIXARG &optional COMMAND-NAME TYPED)`
/// Read a command name and execute it. This is the M-x equivalent.
/// In our stub implementation, COMMAND-NAME must be provided.
pub(crate) fn builtin_execute_extended_command(
    eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("execute-extended-command", &args, 1)?;

    // Batch mode prompt path: M-x reads from stdin and hits EOF.
    if args.len() < 2 {
        return Err(signal(
            "end-of-file",
            vec![Value::string("Error reading from stdin")],
        ));
    }

    let command_name = if let Some(name) = args[1].as_str() {
        name.to_string()
    } else {
        let name = command_name_display(&args[1]);
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "\u{2018}{name}\u{2019} is not a valid command name"
            ))],
        ));
    };

    let command_designator = Value::symbol(command_name.clone());
    if !command_designator_p(eval, &command_designator) {
        return Err(signal(
            "error",
            vec![Value::string(format!(
                "\u{2018}{command_name}\u{2019} is not a valid command name"
            ))],
        ));
    }

    builtin_command_execute(eval, vec![command_designator])
}

fn command_name_display(value: &Value) -> String {
    if let Some(name) = value.as_symbol_name() {
        return name.to_string();
    }
    if let Some(text) = value.as_str() {
        return text.to_string();
    }
    if let Value::Int(n) = value {
        return n.to_string();
    }
    if let Value::Float(n) = value {
        return n.to_string();
    }
    value.type_name().to_string()
}

/// `(key-binding KEY &optional ACCEPT-DEFAULTS NO-REMAP POSITION)`
/// Return the binding for KEY in the current keymaps.
pub(crate) fn builtin_key_binding(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("key-binding", &args, 1)?;

    let key_desc = match args[0].as_str() {
        Some(s) => s.to_string(),
        None => {
            // Try vector of key events
            return Ok(Value::Nil);
        }
    };

    // Try local map first, then global
    if let Some(local_id) = eval.current_local_map {
        let events = match KeymapManager::parse_key_description(&key_desc) {
            Ok(e) => e,
            Err(_) => return Ok(Value::Nil),
        };
        if events.len() == 1 {
            if let Some(binding) = eval.keymaps.lookup_key(local_id, &events[0]) {
                return Ok(key_binding_to_value(binding));
            }
        } else if let Some(binding) = eval.keymaps.lookup_key_sequence(local_id, &events) {
            return Ok(key_binding_to_value(binding));
        }
    }

    if let Some(global_id) = eval.keymaps.global_map() {
        let events = match KeymapManager::parse_key_description(&key_desc) {
            Ok(e) => e,
            Err(_) => return Ok(Value::Nil),
        };
        if events.len() == 1 {
            if let Some(binding) = eval.keymaps.lookup_key(global_id, &events[0]) {
                return Ok(key_binding_to_value(binding));
            }
        } else if let Some(binding) = eval.keymaps.lookup_key_sequence(global_id, &events) {
            return Ok(key_binding_to_value(binding));
        }
    }

    Ok(Value::Nil)
}

/// `(local-key-binding KEY &optional ACCEPT-DEFAULTS)`
pub(crate) fn builtin_local_key_binding(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("local-key-binding", &args, 1)?;

    let key_desc = match args[0].as_str() {
        Some(s) => s.to_string(),
        None => return Ok(Value::Nil),
    };

    if let Some(local_id) = eval.current_local_map {
        let events = match KeymapManager::parse_key_description(&key_desc) {
            Ok(e) => e,
            Err(_) => return Ok(Value::Nil),
        };
        if events.len() == 1 {
            if let Some(binding) = eval.keymaps.lookup_key(local_id, &events[0]) {
                return Ok(key_binding_to_value(binding));
            }
        } else if let Some(binding) = eval.keymaps.lookup_key_sequence(local_id, &events) {
            return Ok(key_binding_to_value(binding));
        }
    }

    Ok(Value::Nil)
}

/// `(global-key-binding KEY &optional ACCEPT-DEFAULTS)`
pub(crate) fn builtin_global_key_binding(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("global-key-binding", &args, 1)?;

    let key_desc = match args[0].as_str() {
        Some(s) => s.to_string(),
        None => return Ok(Value::Nil),
    };

    if let Some(global_id) = eval.keymaps.global_map() {
        let events = match KeymapManager::parse_key_description(&key_desc) {
            Ok(e) => e,
            Err(_) => return Ok(Value::Nil),
        };
        if events.len() == 1 {
            if let Some(binding) = eval.keymaps.lookup_key(global_id, &events[0]) {
                return Ok(key_binding_to_value(binding));
            }
        } else if let Some(binding) = eval.keymaps.lookup_key_sequence(global_id, &events) {
            return Ok(key_binding_to_value(binding));
        }
    }

    Ok(Value::Nil)
}

/// `(minor-mode-key-binding KEY &optional ACCEPT-DEFAULTS)`
/// Look up KEY in active minor mode keymaps.
/// Stub: returns nil since minor mode keymaps are not yet wired.
pub(crate) fn builtin_minor_mode_key_binding(
    _eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_args("minor-mode-key-binding", &args, 1)?;
    Ok(Value::Nil)
}

/// `(where-is-internal DEFINITION &optional KEYMAP FIRSTONLY NOINDIRECT NO-REMAP)`
/// Return list of key sequences that invoke DEFINITION.
/// Stub: returns nil.
pub(crate) fn builtin_where_is_internal(_eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("where-is-internal", &args, 1)?;
    Ok(Value::Nil)
}

/// `(substitute-command-keys STRING)`
/// Replace \\[COMMAND], \\{KEYMAP}, and \\<KEYMAP> sequences in STRING.
pub(crate) fn builtin_substitute_command_keys(
    eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("substitute-command-keys", &args, 1)?;
    let s = match args[0].as_str() {
        Some(s) => s.to_string(),
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("stringp"), args[0].clone()],
            ))
        }
    };

    // Simple substitution: replace \\[command] with "M-x command"
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
                if next == '[' {
                    chars.next(); // consume '['
                    let mut cmd = String::new();
                    for c in chars.by_ref() {
                        if c == ']' {
                            break;
                        }
                        cmd.push(c);
                    }
                    // Try to find a key binding for the command
                    let key_desc = find_key_for_command(eval, &cmd);
                    result.push_str(&key_desc);
                    continue;
                } else if next == '\\' {
                    chars.next();
                    result.push('\\');
                    continue;
                }
            }
        }
        result.push(ch);
    }

    Ok(Value::string(result))
}

/// `(describe-key-briefly KEY &optional INSERT UNTRANSLATED)`
/// Print the command bound to KEY.
pub(crate) fn builtin_describe_key_briefly(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("describe-key-briefly", &args, 1)?;

    let key_desc = match args[0].as_str() {
        Some(s) => s.to_string(),
        None => return Ok(Value::Nil),
    };

    // Look up the binding
    let binding_val = builtin_key_binding(eval, vec![args[0].clone()])?;
    let description = if binding_val.is_nil() {
        format!("{} is undefined", key_desc)
    } else if let Some(name) = binding_val.as_symbol_name() {
        format!("{} runs the command {}", key_desc, name)
    } else {
        format!("{} is bound to {}", key_desc, binding_val)
    };

    Ok(Value::string(description))
}

/// `(this-command-keys)` -> string of keys that invoked current command.
pub(crate) fn builtin_this_command_keys(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_args("this-command-keys", &args, 0)?;
    let keys = eval.interactive.this_command_keys();
    Ok(Value::string(keys.join(" ")))
}

/// `(this-command-keys-vector)` -> vector of keys that invoked current command.
pub(crate) fn builtin_this_command_keys_vector(
    eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("this-command-keys-vector", &args, 0)?;
    let keys = eval.interactive.this_command_keys();
    let vals: Vec<Value> = keys.iter().map(|k| Value::string(k.clone())).collect();
    Ok(Value::vector(vals))
}

// ---------------------------------------------------------------------------
// Thing-at-point functions
// ---------------------------------------------------------------------------

/// `(thing-at-point THING &optional NO-PROPERTIES)` -> the THING at point.
pub(crate) fn builtin_thing_at_point(eval: &mut Evaluator, args: Vec<Value>) -> EvalResult {
    expect_min_args("thing-at-point", &args, 1)?;

    let thing = match args[0].as_symbol_name() {
        Some(s) => s.to_string(),
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), args[0].clone()],
            ))
        }
    };

    let buf = match eval.buffers.current_buffer() {
        Some(b) => b,
        None => return Ok(Value::Nil),
    };

    let text = buf.buffer_string();
    let byte_offset = buf.pt.saturating_sub(buf.begv);
    // pt is a 0-based byte position; convert to a 0-based char index
    let idx = text[..byte_offset.min(text.len())].chars().count();

    match thing.as_str() {
        "word" => Ok(extract_thing_word(&text, idx)),
        "symbol" => Ok(extract_thing_symbol(&text, idx)),
        "line" => Ok(extract_thing_line(&text, idx)),
        "sentence" => Ok(extract_thing_line(&text, idx)), // simplified
        "sexp" => Ok(extract_thing_symbol(&text, idx)),   // simplified
        "whitespace" => Ok(extract_thing_whitespace(&text, idx)),
        "number" => Ok(extract_thing_number(&text, idx)),
        "url" => Ok(Value::Nil),   // stub
        "email" => Ok(Value::Nil), // stub
        "filename" => Ok(extract_thing_filename(&text, idx)),
        _ => Ok(Value::Nil),
    }
}

/// `(bounds-of-thing-at-point THING)` -> (START . END) or nil.
pub(crate) fn builtin_bounds_of_thing_at_point(
    eval: &mut Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("bounds-of-thing-at-point", &args, 1)?;

    let thing = match args[0].as_symbol_name() {
        Some(s) => s.to_string(),
        None => {
            return Err(signal(
                "wrong-type-argument",
                vec![Value::symbol("symbolp"), args[0].clone()],
            ))
        }
    };

    let buf = match eval.buffers.current_buffer() {
        Some(b) => b,
        None => return Ok(Value::Nil),
    };

    let text = buf.buffer_string();
    let byte_offset = buf.pt.saturating_sub(buf.begv);
    // pt is a 0-based byte position; convert to a 0-based char index
    let idx = text[..byte_offset.min(text.len())].chars().count();

    let bounds = match thing.as_str() {
        "word" => bounds_word(&text, idx),
        "symbol" => bounds_symbol(&text, idx),
        "line" => bounds_line(&text, idx),
        "whitespace" => bounds_whitespace(&text, idx),
        "number" => bounds_number(&text, idx),
        _ => None,
    };

    match bounds {
        Some((start, end)) => {
            // Convert from 0-based to 1-based
            Ok(Value::cons(
                Value::Int((start + 1) as i64),
                Value::Int((end + 1) as i64),
            ))
        }
        None => Ok(Value::Nil),
    }
}

/// `(word-at-point &optional NO-PROPERTIES)` -> word at point or nil.
pub(crate) fn builtin_word_at_point(eval: &mut Evaluator, _args: Vec<Value>) -> EvalResult {
    builtin_thing_at_point(eval, vec![Value::symbol("word")])
}

/// `(symbol-at-point)` -> symbol at point or nil.
pub(crate) fn builtin_symbol_at_point(eval: &mut Evaluator, _args: Vec<Value>) -> EvalResult {
    let thing = builtin_thing_at_point(eval, vec![Value::symbol("symbol")])?;
    match thing {
        Value::Str(s) => Ok(Value::symbol((*s).clone())),
        _ => Ok(Value::Nil),
    }
}

// ---------------------------------------------------------------------------
// Special forms for mode definition (called from eval.rs)
// ---------------------------------------------------------------------------

/// `(define-minor-mode MODE DOC &rest BODY)` with keyword args
/// :lighter :keymap :global
///
/// Expands to:
///   - defvar MODE (the toggle variable)
///   - defun MODE (the toggle function)
///   - registers the minor mode in ModeRegistry
pub(crate) fn sf_define_minor_mode(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 2 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }

    let Expr::Symbol(mode_name) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };

    // Parse keyword arguments from the tail
    let mut lighter: Option<String> = None;
    let mut keymap_name: Option<String> = None;
    let mut global = false;
    let mut body_start = 2; // skip mode name and docstring

    // Skip docstring if present
    if tail.len() > 1 {
        if let Expr::Str(_) = &tail[1] {
            body_start = 2;
        } else {
            body_start = 1; // no docstring
        }
    }

    // Parse keyword args
    let mut i = body_start;
    while i + 1 < tail.len() {
        match &tail[i] {
            Expr::Keyword(k) if k == ":lighter" => {
                if let Expr::Str(s) = &tail[i + 1] {
                    lighter = Some(s.clone());
                }
                i += 2;
            }
            Expr::Keyword(k) if k == ":keymap" => {
                if let Expr::Symbol(s) = &tail[i + 1] {
                    keymap_name = Some(s.clone());
                }
                i += 2;
            }
            Expr::Keyword(k) if k == ":global" => {
                match &tail[i + 1] {
                    Expr::Bool(true) => {
                        global = true;
                    }
                    Expr::Symbol(s) if s == "t" => {
                        global = true;
                    }
                    _ => {}
                }
                i += 2;
            }
            _ => break,
        }
    }

    let body_forms = &tail[i..];

    // 1. Create the toggle variable (defvar MODE nil)
    eval.obarray.set_symbol_value(mode_name, Value::Nil);
    eval.obarray.make_special(mode_name);

    // 2. Register with ModeRegistry
    let mode = MinorMode {
        name: mode_name.clone(),
        lighter: lighter.clone(),
        keymap_name: keymap_name.clone(),
        global,
        body: None,
    };
    eval.modes.register_minor_mode(mode);

    // 3. Register as an interactive command
    eval.interactive
        .register_interactive(mode_name, InteractiveSpec::no_args());

    // 4. Create toggle function that:
    //    - Toggles the variable
    //    - Runs the body forms
    //    - Returns the new value
    let mode_name_owned = mode_name.clone();
    let _global_flag = global;

    // Build a lambda body for the toggle function
    // We synthesize: (progn (setq MODE (not MODE)) BODY...)
    let toggle_body_exprs: Vec<Expr> = {
        let mut exprs = Vec::new();
        // (setq MODE (not MODE))
        exprs.push(Expr::List(vec![
            Expr::Symbol("setq".to_string()),
            Expr::Symbol(mode_name_owned.clone()),
            Expr::List(vec![
                Expr::Symbol("not".to_string()),
                Expr::Symbol(mode_name_owned.clone()),
            ]),
        ]));
        // Append body forms
        for form in body_forms {
            exprs.push(form.clone());
        }
        // Return the mode variable value
        exprs.push(Expr::Symbol(mode_name_owned.clone()));
        exprs
    };

    let lambda = Value::Lambda(std::sync::Arc::new(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: toggle_body_exprs,
        env: None,
        docstring: None,
    }));

    eval.obarray.set_symbol_function(mode_name, lambda);

    Ok(Value::symbol(mode_name.clone()))
}

/// `(define-derived-mode MODE PARENT NAME DOC &rest BODY)` with keyword args
/// :syntax-table :abbrev-table
///
/// Creates a major mode that derives from PARENT.
pub(crate) fn sf_define_derived_mode(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 3 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }

    let Expr::Symbol(mode_name) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };

    // Parent can be nil or a symbol
    let parent = match &tail[1] {
        Expr::Symbol(s) if s == "nil" => None,
        Expr::Symbol(s) => Some(s.clone()),
        _ => None,
    };

    // Pretty name (3rd arg) - evaluate it
    let pretty_name = match &tail[2] {
        Expr::Str(s) => s.clone(),
        _ => {
            let val = eval.eval(&tail[2])?;
            match val.as_str() {
                Some(s) => s.to_string(),
                None => mode_name.clone(),
            }
        }
    };

    // Parse optional keyword args and body
    let mut syntax_table_name: Option<String> = None;
    let mut abbrev_table_name: Option<String> = None;
    let mut body_start = 3;

    // Skip docstring if present
    if tail.len() > 3 {
        if let Expr::Str(_) = &tail[3] {
            body_start = 4;
        }
    }

    // Parse keyword args
    let mut i = body_start;
    while i + 1 < tail.len() {
        match &tail[i] {
            Expr::Keyword(k) if k == ":syntax-table" => {
                if let Expr::Symbol(s) = &tail[i + 1] {
                    syntax_table_name = Some(s.clone());
                }
                i += 2;
            }
            Expr::Keyword(k) if k == ":abbrev-table" => {
                if let Expr::Symbol(s) = &tail[i + 1] {
                    abbrev_table_name = Some(s.clone());
                }
                i += 2;
            }
            _ => break,
        }
    }

    let body_forms = &tail[i..];

    // Derive hook name and keymap name
    let hook_name = format!("{}-hook", mode_name);
    let keymap_name = format!("{}-map", mode_name);

    // 1. Register the major mode
    let mode = MajorMode {
        name: mode_name.clone(),
        pretty_name: pretty_name.clone(),
        parent: parent.clone(),
        mode_hook: hook_name.clone(),
        keymap_name: Some(keymap_name.clone()),
        syntax_table_name: syntax_table_name.clone(),
        abbrev_table_name: abbrev_table_name.clone(),
        font_lock: None,
        body: None,
    };
    eval.modes.register_major_mode(mode);

    // 2. Create the hook variable
    eval.obarray.set_symbol_value(&hook_name, Value::Nil);
    eval.obarray.make_special(&hook_name);

    // 3. Register as interactive command
    eval.interactive
        .register_interactive(mode_name, InteractiveSpec::no_args());

    // 4. Create mode function that:
    //    - Calls parent mode first (if any)
    //    - Runs body
    //    - Sets major-mode variable
    //    - Runs mode hook
    let mut func_body: Vec<Expr> = Vec::new();

    // Call parent mode if it exists
    if let Some(ref par) = parent {
        func_body.push(Expr::List(vec![Expr::Symbol(par.clone())]));
    }

    // (setq major-mode 'MODE)
    func_body.push(Expr::List(vec![
        Expr::Symbol("setq".to_string()),
        Expr::Symbol("major-mode".to_string()),
        Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol(mode_name.clone()),
        ]),
    ]));

    // (setq mode-name PRETTY-NAME)
    func_body.push(Expr::List(vec![
        Expr::Symbol("setq".to_string()),
        Expr::Symbol("mode-name".to_string()),
        Expr::Str(pretty_name),
    ]));

    // Body forms
    for form in body_forms {
        func_body.push(form.clone());
    }

    // (run-hooks 'MODE-hook)
    func_body.push(Expr::List(vec![
        Expr::Symbol("run-hooks".to_string()),
        Expr::List(vec![
            Expr::Symbol("quote".to_string()),
            Expr::Symbol(hook_name),
        ]),
    ]));

    let lambda = Value::Lambda(std::sync::Arc::new(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: func_body,
        env: None,
        docstring: None,
    }));

    eval.obarray.set_symbol_function(mode_name, lambda);

    // Set up major-mode and mode-name as special variables
    if !eval.obarray.boundp("major-mode") {
        eval.obarray
            .set_symbol_value("major-mode", Value::symbol("fundamental-mode"));
    }
    eval.obarray.make_special("major-mode");
    if !eval.obarray.boundp("mode-name") {
        eval.obarray
            .set_symbol_value("mode-name", Value::string("Fundamental"));
    }
    eval.obarray.make_special("mode-name");

    Ok(Value::symbol(mode_name.clone()))
}

/// `(define-generic-mode MODE COMMENT-LIST KEYWORD-LIST FONT-LOCK-LIST
///    AUTO-MODE-LIST FUNCTION-LIST &optional DOCSTRING)`
/// Simplified generic mode definition.
pub(crate) fn sf_define_generic_mode(eval: &mut Evaluator, tail: &[Expr]) -> EvalResult {
    if tail.len() < 5 {
        return Err(signal("wrong-number-of-arguments", vec![]));
    }

    let Expr::Symbol(mode_name) = &tail[0] else {
        return Err(signal("wrong-type-argument", vec![]));
    };

    // Register as a basic major mode with no parent
    let mode = MajorMode {
        name: mode_name.clone(),
        pretty_name: mode_name.replace('-', " "),
        parent: None,
        mode_hook: format!("{}-hook", mode_name),
        keymap_name: None,
        syntax_table_name: None,
        abbrev_table_name: None,
        font_lock: None,
        body: None,
    };
    eval.modes.register_major_mode(mode);

    // Register as interactive
    eval.interactive
        .register_interactive(mode_name, InteractiveSpec::no_args());

    // Create a simple mode function
    let lambda = Value::Lambda(std::sync::Arc::new(LambdaData {
        params: LambdaParams::simple(vec![]),
        body: vec![Expr::List(vec![
            Expr::Symbol("setq".to_string()),
            Expr::Symbol("major-mode".to_string()),
            Expr::List(vec![
                Expr::Symbol("quote".to_string()),
                Expr::Symbol(mode_name.clone()),
            ]),
        ])],
        env: None,
        docstring: None,
    }));

    eval.obarray.set_symbol_function(mode_name, lambda);

    Ok(Value::symbol(mode_name.clone()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Convert a KeyBinding to a Value.
fn key_binding_to_value(binding: &KeyBinding) -> Value {
    match binding {
        KeyBinding::Command(name) => Value::symbol(name.clone()),
        KeyBinding::LispValue(v) => v.clone(),
        KeyBinding::Prefix(id) => Value::symbol(format!("keymap-{}", id)),
    }
}

/// Find the key binding description for a command name.
fn find_key_for_command(eval: &Evaluator, command: &str) -> String {
    // Search global map for the command
    if let Some(global_id) = eval.keymaps.global_map() {
        if let Some(km) = eval.keymaps.get(global_id) {
            for (event, binding) in &km.bindings {
                if let KeyBinding::Command(name) = binding {
                    if name == command {
                        return KeymapManager::format_key_event(event);
                    }
                }
            }
        }
    }

    // If local map is set, search it too
    if let Some(local_id) = eval.current_local_map {
        if let Some(km) = eval.keymaps.get(local_id) {
            for (event, binding) in &km.bindings {
                if let KeyBinding::Command(name) = binding {
                    if name == command {
                        return KeymapManager::format_key_event(event);
                    }
                }
            }
        }
    }

    // Fallback: "M-x command"
    format!("M-x {}", command)
}

// ---------------------------------------------------------------------------
// Thing-at-point extraction helpers
// ---------------------------------------------------------------------------

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn is_symbol_char(c: char) -> bool {
    c.is_alphanumeric()
        || matches!(
            c,
            '_' | '-' | '.' | '+' | '*' | '/' | '?' | '!' | '<' | '>' | '=' | ':'
        )
}

fn is_filename_char(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | '~')
}

fn extract_thing_word(text: &str, idx: usize) -> Value {
    match bounds_word(text, idx) {
        Some((start, end)) => Value::string(&text[start..end]),
        None => Value::Nil,
    }
}

fn extract_thing_symbol(text: &str, idx: usize) -> Value {
    match bounds_symbol(text, idx) {
        Some((start, end)) => Value::string(&text[start..end]),
        None => Value::Nil,
    }
}

fn extract_thing_line(text: &str, idx: usize) -> Value {
    match bounds_line(text, idx) {
        Some((start, end)) => Value::string(&text[start..end]),
        None => Value::Nil,
    }
}

fn extract_thing_whitespace(text: &str, idx: usize) -> Value {
    match bounds_whitespace(text, idx) {
        Some((start, end)) => Value::string(&text[start..end]),
        None => Value::Nil,
    }
}

fn extract_thing_number(text: &str, idx: usize) -> Value {
    match bounds_number(text, idx) {
        Some((start, end)) => Value::string(&text[start..end]),
        None => Value::Nil,
    }
}

fn extract_thing_filename(text: &str, idx: usize) -> Value {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() {
        return Value::Nil;
    }
    if !is_filename_char(chars[idx]) {
        return Value::Nil;
    }

    let mut start = idx;
    while start > 0 && is_filename_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && is_filename_char(chars[end]) {
        end += 1;
    }
    let s: String = chars[start..end].iter().collect();
    Value::string(s)
}

fn bounds_word(text: &str, idx: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() || !is_word_char(chars[idx]) {
        return None;
    }

    let mut start = idx;
    while start > 0 && is_word_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && is_word_char(chars[end]) {
        end += 1;
    }

    // Convert char indices back to byte indices
    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
    Some((byte_start, byte_end))
}

fn bounds_symbol(text: &str, idx: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() || !is_symbol_char(chars[idx]) {
        return None;
    }

    let mut start = idx;
    while start > 0 && is_symbol_char(chars[start - 1]) {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && is_symbol_char(chars[end]) {
        end += 1;
    }

    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
    Some((byte_start, byte_end))
}

fn bounds_line(text: &str, idx: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() {
        return None;
    }

    let mut start = idx;
    while start > 0 && chars[start - 1] != '\n' {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && chars[end] != '\n' {
        end += 1;
    }
    // Include the newline if present
    if end < chars.len() {
        end += 1;
    }

    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
    Some((byte_start, byte_end))
}

fn bounds_whitespace(text: &str, idx: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() || !chars[idx].is_whitespace() {
        return None;
    }

    let mut start = idx;
    while start > 0 && chars[start - 1].is_whitespace() {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && chars[end].is_whitespace() {
        end += 1;
    }

    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
    Some((byte_start, byte_end))
}

fn bounds_number(text: &str, idx: usize) -> Option<(usize, usize)> {
    let chars: Vec<char> = text.chars().collect();
    if idx >= chars.len() || !chars[idx].is_ascii_digit() {
        return None;
    }

    let mut start = idx;
    while start > 0 && (chars[start - 1].is_ascii_digit() || chars[start - 1] == '.') {
        start -= 1;
    }
    let mut end = idx;
    while end < chars.len() && (chars[end].is_ascii_digit() || chars[end] == '.') {
        end += 1;
    }

    let byte_start: usize = chars[..start].iter().map(|c| c.len_utf8()).sum();
    let byte_end: usize = chars[..end].iter().map(|c| c.len_utf8()).sum();
    Some((byte_start, byte_end))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::elisp::{format_eval_result, parse_forms, Evaluator};

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

    // -------------------------------------------------------------------
    // InteractiveSpec
    // -------------------------------------------------------------------

    #[test]
    fn interactive_spec_no_args() {
        let spec = InteractiveSpec::no_args();
        assert!(spec.code.is_empty());
        assert!(spec.prompt.is_none());
    }

    #[test]
    fn interactive_spec_with_code() {
        let spec = InteractiveSpec::new("p");
        assert_eq!(spec.code, "p");
    }

    #[test]
    fn interactive_spec_with_prompt() {
        let spec = InteractiveSpec::new("sEnter name: ");
        assert_eq!(spec.code, "sEnter name: ");
        assert_eq!(spec.prompt.as_deref(), Some("Enter name: "));
    }

    // -------------------------------------------------------------------
    // InteractiveRegistry
    // -------------------------------------------------------------------

    #[test]
    fn registry_register_and_query() {
        let mut reg = InteractiveRegistry::new();
        reg.register_interactive("forward-char", InteractiveSpec::new("p"));
        assert!(reg.is_interactive("forward-char"));
        assert!(!reg.is_interactive("nonexistent"));
    }

    #[test]
    fn registry_get_spec() {
        let mut reg = InteractiveRegistry::new();
        reg.register_interactive("find-file", InteractiveSpec::new("FFind file: "));
        let spec = reg.get_spec("find-file").unwrap();
        assert_eq!(spec.code, "FFind file: ");
    }

    #[test]
    fn registry_interactive_call_stack() {
        let mut reg = InteractiveRegistry::new();
        assert!(!reg.is_called_interactively());

        reg.push_interactive_call(true);
        assert!(reg.is_called_interactively());

        reg.push_interactive_call(false);
        assert!(!reg.is_called_interactively());

        reg.pop_interactive_call();
        assert!(reg.is_called_interactively());

        reg.pop_interactive_call();
        assert!(!reg.is_called_interactively());
    }

    #[test]
    fn registry_this_command_keys() {
        let mut reg = InteractiveRegistry::new();
        assert!(reg.this_command_keys().is_empty());

        reg.set_this_command_keys(vec!["C-x".to_string(), "C-f".to_string()]);
        assert_eq!(reg.this_command_keys(), &["C-x", "C-f"]);
    }

    #[test]
    fn registry_default() {
        let reg = InteractiveRegistry::default();
        assert!(!reg.is_called_interactively());
    }

    // -------------------------------------------------------------------
    // define-minor-mode special form
    // -------------------------------------------------------------------

    #[test]
    fn define_minor_mode_creates_variable() {
        let results = eval_all(
            r#"(define-minor-mode test-mode "Test mode" :lighter " Test")
               test-mode"#,
        );
        assert_eq!(results[0], "OK test-mode");
        assert_eq!(results[1], "OK nil"); // initially off
    }

    #[test]
    fn define_minor_mode_creates_toggle_function() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(define-minor-mode my-mode "My mode" :lighter " My")
               (my-mode)
               my-mode"#,
        );
        assert_eq!(results[0], "OK my-mode");
        // After toggling, mode should be on (t)
        assert_eq!(results[2], "OK t");
    }

    #[test]
    fn define_minor_mode_toggle_off() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(define-minor-mode tog-mode "Toggle mode")
               (tog-mode)
               tog-mode
               (tog-mode)
               tog-mode"#,
        );
        // First toggle: on
        assert_eq!(results[2], "OK t");
        // Second toggle: off
        assert_eq!(results[4], "OK nil");
    }

    #[test]
    fn define_minor_mode_registers_interactive() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(define-minor-mode int-mode "Interactive mode")"#,
        );
        assert!(ev.interactive.is_interactive("int-mode"));
    }

    #[test]
    fn define_minor_mode_with_body() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(defvar body-ran nil)
               (define-minor-mode body-mode "Body mode"
                 (setq body-ran t))
               (body-mode)
               body-ran"#,
        );
        assert_eq!(results[3], "OK t");
    }

    #[test]
    fn define_minor_mode_global() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(define-minor-mode glob-mode "Global mode" :global t)"#,
        );
        assert_eq!(results[0], "OK glob-mode");
    }

    // -------------------------------------------------------------------
    // define-derived-mode special form
    // -------------------------------------------------------------------

    #[test]
    fn define_derived_mode_creates_mode() {
        let results = eval_all(
            r#"(define-derived-mode my-text-mode nil "MyText"
                 "A text mode.")"#,
        );
        assert_eq!(results[0], "OK my-text-mode");
    }

    #[test]
    fn define_derived_mode_with_parent() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(defvar parent-ran nil)
               (defun parent-mode ()
                 (setq parent-ran t))
               (define-derived-mode child-mode parent-mode "Child"
                 "A child mode.")"#,
        );
        assert_eq!(results[2], "OK child-mode");
    }

    #[test]
    fn define_derived_mode_sets_major_mode() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(define-derived-mode custom-mode nil "Custom")
               (custom-mode)
               major-mode"#,
        );
        assert_eq!(results[2], "OK custom-mode");
    }

    #[test]
    fn define_derived_mode_sets_mode_name() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(define-derived-mode fancy-mode nil "Fancy")
               (fancy-mode)
               mode-name"#,
        );
        assert_eq!(results[2], r#"OK "Fancy""#);
    }

    #[test]
    fn define_derived_mode_runs_body() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(defvar derived-body-ran nil)
               (define-derived-mode body-derived-mode nil "BodyDerived"
                 "Mode with body."
                 (setq derived-body-ran t))
               (body-derived-mode)
               derived-body-ran"#,
        );
        assert_eq!(results[3], "OK t");
    }

    #[test]
    fn define_derived_mode_registers_interactive() {
        let mut ev = Evaluator::new();
        eval_all_with(&mut ev, r#"(define-derived-mode ireg-mode nil "IReg")"#);
        assert!(ev.interactive.is_interactive("ireg-mode"));
    }

    // -------------------------------------------------------------------
    // define-generic-mode special form
    // -------------------------------------------------------------------

    #[test]
    fn define_generic_mode_creates_mode() {
        let results = eval_all(r#"(define-generic-mode my-generic-mode nil nil nil nil nil)"#);
        assert_eq!(results[0], "OK my-generic-mode");
    }

    // -------------------------------------------------------------------
    // commandp (interactive-aware version)
    // -------------------------------------------------------------------

    #[test]
    fn commandp_with_interactive_registration() {
        let mut ev = Evaluator::new();
        eval_all_with(&mut ev, r#"(define-minor-mode cmd-test-mode "Test")"#);
        // cmd-test-mode should now be a command
        let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("cmd-test-mode")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_non_interactive() {
        let mut ev = Evaluator::new();
        eval_all_with(&mut ev, r#"(defun my-plain-fn () 42)"#);
        let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("my-plain-fn")]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn commandp_true_for_builtin_ignore() {
        let mut ev = Evaluator::new();
        let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("ignore")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_true_for_builtin_execute_extended_command() {
        let mut ev = Evaluator::new();
        let result =
            builtin_commandp_interactive(&mut ev, vec![Value::symbol("execute-extended-command")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_true_for_builtin_forward_char() {
        let mut ev = Evaluator::new();
        let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("forward-char")]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_true_for_builtin_editing_commands() {
        let mut ev = Evaluator::new();
        for name in [
            "backward-char",
            "delete-char",
            "yank",
            "yank-pop",
            "transpose-chars",
            "transpose-lines",
            "transpose-paragraphs",
            "transpose-sentences",
            "transpose-sexps",
            "upcase-word",
            "downcase-word",
            "capitalize-word",
            "upcase-region",
            "downcase-region",
            "capitalize-region",
            "upcase-initials-region",
            "kill-word",
            "backward-kill-word",
            "kill-region",
            "kill-ring-save",
            "kill-whole-line",
            "copy-region-as-kill",
            "open-line",
            "delete-horizontal-space",
            "just-one-space",
            "delete-indentation",
            "indent-for-tab-command",
            "transpose-words",
            "scroll-up-command",
            "scroll-down-command",
            "recenter-top-bottom",
            "move-beginning-of-line",
            "move-end-of-line",
        ] {
            let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol(name)])
                .expect("commandp call");
            assert!(result.is_truthy(), "expected commandp true for {name}");
        }
    }

    #[test]
    fn commandp_false_for_noninteractive_builtin() {
        let mut ev = Evaluator::new();
        let result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("car")]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn commandp_resolves_aliases_and_symbol_designators() {
        let mut ev = Evaluator::new();
        ev.obarray
            .set_symbol_function("t", Value::symbol("ignore"));
        ev.obarray
            .set_symbol_function(":vm-command-alias-keyword", Value::symbol("ignore"));
        ev.obarray
            .set_symbol_function("vm-command-alias", Value::True);
        ev.obarray.set_symbol_function(
            "vm-command-alias-keyword",
            Value::keyword(":vm-command-alias-keyword"),
        );

        let t_result = builtin_commandp_interactive(&mut ev, vec![Value::True]);
        assert!(t_result.unwrap().is_truthy());
        let keyword_result =
            builtin_commandp_interactive(&mut ev, vec![Value::keyword(":vm-command-alias-keyword")]);
        assert!(keyword_result.unwrap().is_truthy());
        let alias_result = builtin_commandp_interactive(&mut ev, vec![Value::symbol("vm-command-alias")]);
        assert!(alias_result.unwrap().is_truthy());
        let keyword_alias_result =
            builtin_commandp_interactive(&mut ev, vec![Value::symbol("vm-command-alias-keyword")]);
        assert!(keyword_alias_result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_true_for_lambda_with_interactive_form() {
        let mut ev = Evaluator::new();
        let lambda = eval_all_with(&mut ev, "(lambda () (interactive) 1)");
        let parsed = super::super::parser::parse_forms("(lambda () (interactive) 1)")
            .expect("lambda form should parse");
        let value = ev.eval(&parsed[0]).expect("lambda form should evaluate");
        assert_eq!(lambda[0], "OK (lambda nil (interactive) 1)");
        let result = builtin_commandp_interactive(&mut ev, vec![value]);
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn commandp_true_for_quoted_lambda_with_interactive_form() {
        let mut ev = Evaluator::new();
        let forms = super::super::parser::parse_forms("'(lambda () \"doc\" (interactive) 1)")
            .expect("quoted lambda form should parse");
        let quoted_lambda = ev.eval(&forms[0]).expect("quoted lambda should evaluate");
        let result = builtin_commandp_interactive(&mut ev, vec![quoted_lambda]);
        assert!(result.unwrap().is_truthy());
    }

    // -------------------------------------------------------------------
    // interactive-p / called-interactively-p
    // -------------------------------------------------------------------

    #[test]
    fn interactive_p_false_by_default() {
        let mut ev = Evaluator::new();
        let result = builtin_interactive_p(&mut ev, vec![]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn interactive_p_nil_when_interactive() {
        let mut ev = Evaluator::new();
        ev.interactive.push_interactive_call(true);
        let result = builtin_interactive_p(&mut ev, vec![]);
        ev.interactive.pop_interactive_call();
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn called_interactively_p_false_by_default() {
        let mut ev = Evaluator::new();
        let result = builtin_called_interactively_p(&mut ev, vec![]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn called_interactively_p_with_kind() {
        let mut ev = Evaluator::new();
        let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("any")]);
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn called_interactively_p_kind_interactive_is_nil_when_interactive() {
        let mut ev = Evaluator::new();
        ev.interactive.push_interactive_call(true);
        let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("interactive")]);
        ev.interactive.pop_interactive_call();
        assert!(result.unwrap().is_nil());
    }

    #[test]
    fn called_interactively_p_kind_any_is_t_when_interactive() {
        let mut ev = Evaluator::new();
        ev.interactive.push_interactive_call(true);
        let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("any")]);
        ev.interactive.pop_interactive_call();
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn called_interactively_p_unknown_kind_is_t_when_interactive() {
        let mut ev = Evaluator::new();
        ev.interactive.push_interactive_call(true);
        let result = builtin_called_interactively_p(&mut ev, vec![Value::symbol("foo")]);
        ev.interactive.pop_interactive_call();
        assert!(result.unwrap().is_truthy());
    }

    #[test]
    fn called_interactively_p_too_many_args() {
        let mut ev = Evaluator::new();
        let result = builtin_called_interactively_p(
            &mut ev,
            vec![Value::symbol("any"), Value::symbol("extra")],
        );
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // this-command-keys / this-command-keys-vector
    // -------------------------------------------------------------------

    #[test]
    fn this_command_keys_empty() {
        let mut ev = Evaluator::new();
        let result = builtin_this_command_keys(&mut ev, vec![]).unwrap();
        assert_eq!(result.as_str(), Some(""));
    }

    #[test]
    fn this_command_keys_after_set() {
        let mut ev = Evaluator::new();
        ev.interactive
            .set_this_command_keys(vec!["C-x".to_string(), "C-f".to_string()]);
        let result = builtin_this_command_keys(&mut ev, vec![]).unwrap();
        assert_eq!(result.as_str(), Some("C-x C-f"));
    }

    #[test]
    fn this_command_keys_vector_empty() {
        let mut ev = Evaluator::new();
        let result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
        assert!(matches!(result, Value::Vector(_)));
    }

    #[test]
    fn this_command_keys_vector_after_set() {
        let mut ev = Evaluator::new();
        ev.interactive
            .set_this_command_keys(vec!["M-x".to_string()]);
        let result = builtin_this_command_keys_vector(&mut ev, vec![]).unwrap();
        if let Value::Vector(v) = result {
            let v = v.lock().unwrap();
            assert_eq!(v.len(), 1);
        } else {
            panic!("expected vector");
        }
    }

    // -------------------------------------------------------------------
    // key-binding / local-key-binding / global-key-binding
    // -------------------------------------------------------------------

    #[test]
    fn key_binding_global() {
        let mut ev = Evaluator::new();
        let map_id = ev.keymaps.make_keymap();
        ev.keymaps.set_global_map(map_id);
        let events = KeymapManager::parse_key_description("C-f").unwrap();
        ev.keymaps.define_key(
            map_id,
            events[0].clone(),
            KeyBinding::Command("forward-char".to_string()),
        );

        let result = builtin_key_binding(&mut ev, vec![Value::string("C-f")]).unwrap();
        assert_eq!(result.as_symbol_name(), Some("forward-char"));
    }

    #[test]
    fn key_binding_unbound() {
        let mut ev = Evaluator::new();
        let result = builtin_key_binding(&mut ev, vec![Value::string("C-z")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn global_key_binding_returns_binding() {
        let mut ev = Evaluator::new();
        let map_id = ev.keymaps.make_keymap();
        ev.keymaps.set_global_map(map_id);
        let events = KeymapManager::parse_key_description("M-x").unwrap();
        ev.keymaps.define_key(
            map_id,
            events[0].clone(),
            KeyBinding::Command("execute-extended-command".to_string()),
        );

        let result = builtin_global_key_binding(&mut ev, vec![Value::string("M-x")]).unwrap();
        assert_eq!(result.as_symbol_name(), Some("execute-extended-command"));
    }

    #[test]
    fn local_key_binding_nil_when_no_local_map() {
        let mut ev = Evaluator::new();
        let result = builtin_local_key_binding(&mut ev, vec![Value::string("C-c")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn minor_mode_key_binding_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_minor_mode_key_binding(&mut ev, vec![Value::string("C-c")]).unwrap();
        assert!(result.is_nil());
    }

    // -------------------------------------------------------------------
    // substitute-command-keys
    // -------------------------------------------------------------------

    #[test]
    fn substitute_command_keys_plain() {
        let mut ev = Evaluator::new();
        let result =
            builtin_substitute_command_keys(&mut ev, vec![Value::string("Press C-x to save")])
                .unwrap();
        assert_eq!(result.as_str(), Some("Press C-x to save"));
    }

    #[test]
    fn substitute_command_keys_with_command() {
        let mut ev = Evaluator::new();
        let result = builtin_substitute_command_keys(
            &mut ev,
            vec![Value::string("Press \\[save-buffer] to save")],
        )
        .unwrap();
        // Should substitute with "M-x save-buffer" since no key is bound
        let s = result.as_str().unwrap();
        assert!(s.contains("save-buffer"));
    }

    #[test]
    fn substitute_command_keys_with_bound_key() {
        let mut ev = Evaluator::new();
        let map_id = ev.keymaps.make_keymap();
        ev.keymaps.set_global_map(map_id);
        let events = KeymapManager::parse_key_description("C-s").unwrap();
        ev.keymaps.define_key(
            map_id,
            events[0].clone(),
            KeyBinding::Command("save-buffer".to_string()),
        );

        let result = builtin_substitute_command_keys(
            &mut ev,
            vec![Value::string("Press \\[save-buffer] to save")],
        )
        .unwrap();
        let s = result.as_str().unwrap();
        assert!(s.contains("C-s"));
    }

    #[test]
    fn substitute_command_keys_not_string() {
        let mut ev = Evaluator::new();
        let result = builtin_substitute_command_keys(&mut ev, vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // describe-key-briefly
    // -------------------------------------------------------------------

    #[test]
    fn describe_key_briefly_unbound() {
        let mut ev = Evaluator::new();
        let result = builtin_describe_key_briefly(&mut ev, vec![Value::string("C-z")]).unwrap();
        let s = result.as_str().unwrap();
        assert!(s.contains("undefined"));
    }

    #[test]
    fn describe_key_briefly_bound() {
        let mut ev = Evaluator::new();
        let map_id = ev.keymaps.make_keymap();
        ev.keymaps.set_global_map(map_id);
        let events = KeymapManager::parse_key_description("C-f").unwrap();
        ev.keymaps.define_key(
            map_id,
            events[0].clone(),
            KeyBinding::Command("forward-char".to_string()),
        );

        let result = builtin_describe_key_briefly(&mut ev, vec![Value::string("C-f")]).unwrap();
        let s = result.as_str().unwrap();
        assert!(s.contains("forward-char"));
    }

    // -------------------------------------------------------------------
    // thing-at-point
    // -------------------------------------------------------------------

    #[test]
    fn thing_at_point_word() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "tap")
               (set-buffer "tap")
               (insert "hello world")
               (goto-char 2)"#,
        );
        let result = builtin_thing_at_point(&mut ev, vec![Value::symbol("word")]).unwrap();
        assert_eq!(result.as_str(), Some("hello"));
    }

    #[test]
    fn thing_at_point_symbol() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "tap2")
               (set-buffer "tap2")
               (insert "foo-bar baz")
               (goto-char 3)"#,
        );
        let result = builtin_thing_at_point(&mut ev, vec![Value::symbol("symbol")]).unwrap();
        assert_eq!(result.as_str(), Some("foo-bar"));
    }

    #[test]
    fn thing_at_point_line() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "tap3")
               (set-buffer "tap3")
               (insert "line1\nline2\n")
               (goto-char 1)"#,
        );
        let result = builtin_thing_at_point(&mut ev, vec![Value::symbol("line")]).unwrap();
        let s = result.as_str().unwrap();
        assert!(s.starts_with("line1"));
    }

    #[test]
    fn thing_at_point_no_buffer() {
        let mut ev = Evaluator::new();
        // No buffer set
        let result = builtin_thing_at_point(&mut ev, vec![Value::symbol("word")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn thing_at_point_not_symbol() {
        let mut ev = Evaluator::new();
        let result = builtin_thing_at_point(&mut ev, vec![Value::Int(42)]);
        assert!(result.is_err());
    }

    // -------------------------------------------------------------------
    // bounds-of-thing-at-point
    // -------------------------------------------------------------------

    #[test]
    fn bounds_of_thing_at_point_word() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "bnd")
               (set-buffer "bnd")
               (insert "hello world")
               (goto-char 3)"#,
        );
        let result =
            builtin_bounds_of_thing_at_point(&mut ev, vec![Value::symbol("word")]).unwrap();
        // Should be (1 . 6) for "hello"
        if let Value::Cons(cell) = &result {
            let cell = cell.lock().unwrap();
            assert_eq!(cell.car.as_int(), Some(1));
            assert_eq!(cell.cdr.as_int(), Some(6));
        } else {
            panic!("expected cons, got {:?}", result);
        }
    }

    #[test]
    fn bounds_of_thing_at_point_nil() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "bnd2")
               (set-buffer "bnd2")
               (insert "   ")
               (goto-char 2)"#,
        );
        let result =
            builtin_bounds_of_thing_at_point(&mut ev, vec![Value::symbol("word")]).unwrap();
        assert!(result.is_nil());
    }

    // -------------------------------------------------------------------
    // word-at-point / symbol-at-point
    // -------------------------------------------------------------------

    #[test]
    fn word_at_point_basic() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "wap")
               (set-buffer "wap")
               (insert "testing words")
               (goto-char 3)"#,
        );
        let result = builtin_word_at_point(&mut ev, vec![]).unwrap();
        assert_eq!(result.as_str(), Some("testing"));
    }

    #[test]
    fn symbol_at_point_basic() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(get-buffer-create "sap")
               (set-buffer "sap")
               (insert "my-symbol other")
               (goto-char 3)"#,
        );
        let result = builtin_symbol_at_point(&mut ev, vec![]).unwrap();
        assert_eq!(result.as_symbol_name(), Some("my-symbol"));
    }

    // -------------------------------------------------------------------
    // command-execute
    // -------------------------------------------------------------------

    #[test]
    fn command_execute_builtin_ignore() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("ignore")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn command_execute_builtin_eval_expression_reads_stdin_in_batch() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("eval-expression")])
            .expect_err("command-execute eval-expression should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn command_execute_builtin_self_insert_command_is_noop() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("self-insert-command")])
            .expect("self-insert-command should execute");
        assert!(result.is_nil());
    }

    #[test]
    fn command_execute_builtin_delete_char_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (command-execute 'delete-char)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"bc\"");
    }

    #[test]
    fn call_interactively_builtin_delete_char_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (call-interactively 'delete-char)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"bc\"");
    }

    #[test]
    fn command_execute_builtin_upcase_word_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc def")
                 (goto-char 1)
                 (command-execute 'upcase-word)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"ABC def\"");
    }

    #[test]
    fn call_interactively_builtin_capitalize_word_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc def")
                 (goto-char 1)
                 (call-interactively 'capitalize-word)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Abc def\"");
    }

    #[test]
    fn command_execute_builtin_transpose_words_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "aa bb")
                 (goto-char 1)
                 (command-execute 'transpose-words)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"bb aa\"");
    }

    #[test]
    fn call_interactively_builtin_transpose_words_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "aa bb")
                 (goto-char 1)
                 (call-interactively 'transpose-words)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"bb aa\"");
    }

    #[test]
    fn command_execute_builtin_transpose_sexps_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "(aa) (bb)")
                 (goto-char 5)
                 (command-execute 'transpose-sexps)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"(bb) (aa)\"");
    }

    #[test]
    fn call_interactively_builtin_transpose_sexps_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "(aa) (bb)")
                 (goto-char 5)
                 (call-interactively 'transpose-sexps)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"(bb) (aa)\"");
    }

    #[test]
    fn command_execute_builtin_transpose_sentences_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "One.  Two.")
                 (goto-char 1)
                 (command-execute 'transpose-sentences)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Two.  One.\"");
    }

    #[test]
    fn call_interactively_builtin_transpose_sentences_uses_default_prefix_arg() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "One.  Two.")
                 (goto-char 1)
                 (call-interactively 'transpose-sentences)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Two.  One.\"");
    }

    #[test]
    fn command_execute_builtin_transpose_paragraphs_swaps_paragraphs() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "A\n\nB")
                 (goto-char 1)
                 (command-execute 'transpose-paragraphs)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"\\nBA\\n\"");
    }

    #[test]
    fn command_execute_builtin_kill_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (command-execute 'kill-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"c\"");
    }

    #[test]
    fn call_interactively_builtin_kill_ring_save_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (call-interactively 'kill-ring-save)
                 (current-kill 0 t))"#,
        );
        assert_eq!(results[0], "OK \"ab\"");
    }

    #[test]
    fn command_execute_builtin_copy_region_as_kill_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(let ((kill-ring nil))
                 (with-temp-buffer
                   (insert "abc")
                   (goto-char 1)
                   (set-mark 3)
                   (command-execute 'copy-region-as-kill)
                   (current-kill 0 t)))"#,
        );
        assert_eq!(results[0], "OK \"ab\"");
    }

    #[test]
    fn call_interactively_builtin_copy_region_as_kill_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(let ((kill-ring nil))
                 (with-temp-buffer
                   (insert "abc")
                   (goto-char 1)
                   (set-mark 3)
                   (call-interactively 'copy-region-as-kill)
                   (current-kill 0 t)))"#,
        );
        assert_eq!(results[0], "OK \"ab\"");
    }

    #[test]
    fn call_interactively_builtin_upcase_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (call-interactively 'upcase-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"ABc\"");
    }

    #[test]
    fn call_interactively_builtin_downcase_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "ABC")
                 (goto-char 1)
                 (set-mark 3)
                 (call-interactively 'downcase-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"abC\"");
    }

    #[test]
    fn call_interactively_builtin_capitalize_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (call-interactively 'capitalize-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Abc\"");
    }

    #[test]
    fn command_execute_builtin_upcase_region_signals_args_out_of_range() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (condition-case err
                     (command-execute 'upcase-region)
                   (error err)))"#,
        );
        assert_eq!(results[0], "OK (args-out-of-range \"\" 0)");
    }

    #[test]
    fn command_execute_builtin_downcase_region_signals_args_out_of_range() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "ABC")
                 (goto-char 1)
                 (set-mark 3)
                 (condition-case err
                     (command-execute 'downcase-region)
                   (error err)))"#,
        );
        assert_eq!(results[0], "OK (args-out-of-range \"\" 0)");
    }

    #[test]
    fn command_execute_builtin_capitalize_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (command-execute 'capitalize-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Abc\"");
    }

    #[test]
    fn command_execute_builtin_capitalize_region_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (command-execute 'capitalize-region)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn call_interactively_builtin_upcase_initials_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (call-interactively 'upcase-initials-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Abc\"");
    }

    #[test]
    fn command_execute_builtin_upcase_initials_region_uses_marked_region() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (set-mark 3)
                 (command-execute 'upcase-initials-region)
                 (buffer-string))"#,
        );
        assert_eq!(results[0], "OK \"Abc\"");
    }

    #[test]
    fn command_execute_builtin_upcase_initials_region_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (command-execute 'upcase-initials-region)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn command_execute_builtin_kill_region_without_mark_signals_user_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (command-execute 'kill-region)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (user-error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn command_execute_builtin_kill_ring_save_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (command-execute 'kill-ring-save)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn call_interactively_builtin_kill_ring_save_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (call-interactively 'kill-ring-save)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn command_execute_builtin_copy_region_as_kill_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (command-execute 'copy-region-as-kill)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn call_interactively_builtin_copy_region_as_kill_without_mark_signals_error() {
        let mut ev = Evaluator::new();
        let results = eval_all_with(
            &mut ev,
            r#"(with-temp-buffer
                 (insert "abc")
                 (goto-char 1)
                 (condition-case err
                     (call-interactively 'copy-region-as-kill)
                   (error err)))"#,
        );
        assert_eq!(
            results[0],
            "OK (error \"The mark is not set now, so there is no region\")"
        );
    }

    #[test]
    fn command_execute_builtin_find_file_reads_stdin_in_batch() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("find-file")])
            .expect_err("command-execute find-file should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn command_execute_builtin_save_buffer_reads_stdin_in_batch() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("save-buffer")])
            .expect_err("command-execute save-buffer should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn command_execute_builtin_set_mark_command_returns_nil() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("set-mark-command")])
            .expect("set-mark-command should execute");
        assert!(result.is_nil());
    }

    #[test]
    fn command_execute_builtin_quoted_insert_reads_stdin_in_batch() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("quoted-insert")])
            .expect_err("command-execute quoted-insert should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn command_execute_builtin_universal_argument_returns_noninteractive_function() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("universal-argument")])
            .expect("universal-argument should execute");
        assert!(matches!(result, Value::Lambda(_)));
        let as_command = builtin_commandp_interactive(&mut ev, vec![result]).expect("commandp call");
        assert!(as_command.is_nil());
    }

    #[test]
    fn command_execute_calls_function() {
        let mut ev = Evaluator::new();
        eval_all_with(
            &mut ev,
            r#"(defvar exec-ran nil)
               (defun test-cmd () (setq exec-ran t))"#,
        );
        ev.interactive
            .register_interactive("test-cmd", InteractiveSpec::no_args());

        let result = builtin_command_execute(&mut ev, vec![Value::symbol("test-cmd")]).unwrap();
        assert!(result.is_truthy());

        let ran = ev.obarray.symbol_value("exec-ran").unwrap().clone();
        assert!(ran.is_truthy());
    }

    #[test]
    fn command_execute_non_command_signals_commandp_error() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("car")])
            .expect_err("command-execute should reject non-command symbols");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("commandp"), Value::symbol("car")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn call_interactively_builtin_ignore() {
        let mut ev = Evaluator::new();
        let result = builtin_call_interactively(&mut ev, vec![Value::symbol("ignore")]).unwrap();
        assert!(result.is_nil());
    }

    #[test]
    fn call_interactively_non_command_signals_commandp_error() {
        let mut ev = Evaluator::new();
        let result = builtin_call_interactively(&mut ev, vec![Value::symbol("car")])
            .expect_err("call-interactively should reject non-command symbols");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("commandp"), Value::symbol("car")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn call_interactively_eval_expression_reads_stdin_in_batch() {
        let mut ev = Evaluator::new();
        let result = builtin_call_interactively(&mut ev, vec![Value::symbol("eval-expression")])
            .expect_err("call-interactively eval-expression should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn self_insert_command_argument_validation() {
        let mut ev = Evaluator::new();

        let missing = builtin_self_insert_command(&mut ev, vec![])
            .expect_err("self-insert-command should require one arg");
        match missing {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-number-of-arguments");
                assert_eq!(
                    sig.data,
                    vec![Value::symbol("self-insert-command"), Value::Int(0)]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }

        let wrong_type = builtin_self_insert_command(&mut ev, vec![Value::symbol("x")])
            .expect_err("self-insert-command should type check arg");
        match wrong_type {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "wrong-type-argument");
                assert_eq!(sig.data, vec![Value::symbol("fixnump"), Value::symbol("x")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn keyboard_quit_signals_quit() {
        let mut ev = Evaluator::new();
        let result = builtin_command_execute(&mut ev, vec![Value::symbol("keyboard-quit")])
            .expect_err("keyboard-quit should signal quit");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "quit");
                assert!(sig.data.is_empty());
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // execute-extended-command
    // -------------------------------------------------------------------

    #[test]
    fn execute_extended_command_with_command_name() {
        let mut ev = Evaluator::new();
        let result = builtin_execute_extended_command(
            &mut ev,
            vec![Value::Nil, Value::string("ignore")],
        )
        .expect("execute-extended-command should run command names");
        assert!(result.is_nil());
    }

    #[test]
    fn execute_extended_command_no_name_signals_end_of_file() {
        let mut ev = Evaluator::new();
        let result = builtin_execute_extended_command(&mut ev, vec![Value::Nil])
            .expect_err("execute-extended-command should signal end-of-file in batch");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "end-of-file");
                assert_eq!(sig.data, vec![Value::string("Error reading from stdin")]);
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn execute_extended_command_rejects_symbol_name_payload() {
        let mut ev = Evaluator::new();
        let result = builtin_execute_extended_command(
            &mut ev,
            vec![Value::Nil, Value::symbol("ignore")],
        )
        .expect_err("symbol payload should not be accepted as a command name");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("\u{2018}ignore\u{2019} is not a valid command name")]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn execute_extended_command_rejects_non_command_name() {
        let mut ev = Evaluator::new();
        let result =
            builtin_execute_extended_command(&mut ev, vec![Value::Nil, Value::string("car")])
                .expect_err("non-command names should be rejected");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("\u{2018}car\u{2019} is not a valid command name")]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    #[test]
    fn execute_extended_command_rejects_non_string_name_payload() {
        let mut ev = Evaluator::new();
        let result = builtin_execute_extended_command(&mut ev, vec![Value::Nil, Value::Int(1)])
            .expect_err("non-string command names should be rejected");
        match result {
            Flow::Signal(sig) => {
                assert_eq!(sig.symbol, "error");
                assert_eq!(
                    sig.data,
                    vec![Value::string("\u{2018}1\u{2019} is not a valid command name")]
                );
            }
            other => panic!("unexpected flow: {other:?}"),
        }
    }

    // -------------------------------------------------------------------
    // where-is-internal (stub)
    // -------------------------------------------------------------------

    #[test]
    fn where_is_internal_returns_nil() {
        let mut ev = Evaluator::new();
        let result =
            builtin_where_is_internal(&mut ev, vec![Value::symbol("forward-char")]).unwrap();
        assert!(result.is_nil());
    }

    // -------------------------------------------------------------------
    // Extractors (unit tests for internal functions)
    // -------------------------------------------------------------------

    #[test]
    fn bounds_word_middle() {
        let text = "hello world";
        let bounds = bounds_word(text, 2).unwrap(); // 'l' in "hello"
        assert_eq!(&text[bounds.0..bounds.1], "hello");
    }

    #[test]
    fn bounds_word_at_start() {
        let text = "abc def";
        let bounds = bounds_word(text, 0).unwrap();
        assert_eq!(&text[bounds.0..bounds.1], "abc");
    }

    #[test]
    fn bounds_word_at_space() {
        let text = "abc def";
        assert!(bounds_word(text, 3).is_none()); // space
    }

    #[test]
    fn bounds_symbol_with_hyphen() {
        let text = "foo-bar baz";
        let bounds = bounds_symbol(text, 1).unwrap();
        assert_eq!(&text[bounds.0..bounds.1], "foo-bar");
    }

    #[test]
    fn bounds_line_first_line() {
        let text = "first\nsecond\n";
        let bounds = bounds_line(text, 2).unwrap();
        assert_eq!(&text[bounds.0..bounds.1], "first\n");
    }

    #[test]
    fn bounds_whitespace_basic() {
        let text = "hello   world";
        let bounds = bounds_whitespace(text, 6).unwrap(); // middle space
        assert_eq!(&text[bounds.0..bounds.1], "   ");
    }

    #[test]
    fn bounds_number_basic() {
        let text = "val=42.5ok";
        let bounds = bounds_number(text, 4).unwrap();
        assert_eq!(&text[bounds.0..bounds.1], "42.5");
    }

    #[test]
    fn bounds_number_no_number() {
        let text = "hello";
        assert!(bounds_number(text, 0).is_none());
    }

    // -------------------------------------------------------------------
    // Integration: via Elisp eval
    // -------------------------------------------------------------------

    #[test]
    fn eval_define_minor_mode() {
        let results =
            eval_all(r#"(define-minor-mode my-test-mode "A test minor mode" :lighter " T")"#);
        assert_eq!(results[0], "OK my-test-mode");
    }

    #[test]
    fn eval_define_derived_mode() {
        let results =
            eval_all(r#"(define-derived-mode my-custom-mode nil "MyCustom" "Custom mode.")"#);
        assert_eq!(results[0], "OK my-custom-mode");
    }

    #[test]
    fn eval_define_derived_mode_and_activate() {
        let results = eval_all(
            r#"(define-derived-mode act-mode nil "Activated")
               (act-mode)
               major-mode"#,
        );
        assert_eq!(results[2], "OK act-mode");
    }

    #[test]
    fn eval_minor_mode_full_cycle() {
        let results = eval_all(
            r#"(define-minor-mode cycle-mode "Cycle mode" :lighter " C")
               cycle-mode
               (cycle-mode)
               cycle-mode
               (cycle-mode)
               cycle-mode"#,
        );
        assert_eq!(results[1], "OK nil"); // initially off
        assert_eq!(results[3], "OK t"); // toggled on
        assert_eq!(results[5], "OK nil"); // toggled off
    }
}
