//! Minibuffer and completion system.
//!
//! Provides:
//! - `MinibufferManager` — owns all minibuffer state, history, and completion logic
//! - `CompletionTable` — what can be completed against (list, function, file names, etc.)
//! - `CompletionStyle` — matching strategy (prefix, substring, flex, basic)
//! - Builtin functions for Elisp: `read-from-minibuffer`, `completing-read`, `y-or-n-p`, etc.

use std::collections::HashMap;

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

// ---------------------------------------------------------------------------
// Argument helpers (local copies, same pattern as builtins.rs / builtins_extra.rs)
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

fn expect_max_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    if args.len() > max {
        Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol(name), Value::Int(args.len() as i64)],
        ))
    } else {
        Ok(())
    }
}

fn expect_string(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("stringp"), other.clone()],
        )),
    }
}

// ---------------------------------------------------------------------------
// CompletionTable
// ---------------------------------------------------------------------------

/// What can be completed against.
pub enum CompletionTable {
    /// Fixed list of completion candidates.
    List(Vec<String>),
    /// Dynamic completion function: given the current input, returns matching candidates.
    Function(Box<dyn Fn(&str) -> Vec<String>>),
    /// File name completion rooted at a directory.
    FileNames { directory: String },
    /// Buffer name completion (candidates supplied externally).
    BufferNames,
    /// Symbol name completion (candidates supplied externally).
    SymbolNames,
    /// Association list: each entry is (key, value).
    Alist(Vec<(String, Value)>),
}

impl CompletionTable {
    /// Extract the raw string candidates from the table.
    ///
    /// For `Function` tables the `input` is passed through; for static tables it
    /// is ignored (filtering happens later in the matching functions).
    fn candidates(&self, input: &str) -> Vec<String> {
        match self {
            CompletionTable::List(v) => v.clone(),
            CompletionTable::Function(f) => f(input),
            CompletionTable::FileNames { directory } => list_files_in_dir(directory),
            CompletionTable::BufferNames => Vec::new(),
            CompletionTable::SymbolNames => Vec::new(),
            CompletionTable::Alist(pairs) => pairs.iter().map(|(k, _)| k.clone()).collect(),
        }
    }
}

/// Best-effort listing of file names in `dir`.  Returns an empty vec on I/O error.
fn list_files_in_dir(dir: &str) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect(),
        Err(_) => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// CompletionStyle
// ---------------------------------------------------------------------------

/// Matching strategy for completions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompletionStyle {
    /// Standard prefix matching (case-insensitive).
    Prefix,
    /// Match anywhere in the candidate string.
    Substring,
    /// Fuzzy / flex matching: input characters must appear in order.
    Flex,
    /// Exact prefix (case-sensitive).
    Basic,
}

// ---------------------------------------------------------------------------
// CompletionResult
// ---------------------------------------------------------------------------

/// Result of a completion attempt.
pub struct CompletionResult {
    /// The candidates that matched.
    pub matches: Vec<String>,
    /// Longest common prefix of all matches (if any).
    pub common_prefix: Option<String>,
    /// Whether the match list is exhaustive (i.e. we know there are no more).
    pub exhaustive: bool,
}

// ---------------------------------------------------------------------------
// MinibufferState
// ---------------------------------------------------------------------------

/// Tracks one active minibuffer interaction (possibly recursive).
pub struct MinibufferState {
    pub prompt: String,
    pub initial_input: String,
    pub history: Vec<String>,
    pub history_position: Option<usize>,
    pub content: String,
    pub cursor_pos: usize,
    pub completion_table: Option<CompletionTable>,
    pub require_match: bool,
    pub default_value: Option<String>,
    pub active: bool,
    /// Recursive minibuffer depth at which this state was entered.
    pub depth: usize,
}

impl MinibufferState {
    fn new(prompt: String, initial: String, depth: usize) -> Self {
        let cursor_pos = initial.len();
        Self {
            prompt,
            initial_input: initial.clone(),
            history: Vec::new(),
            history_position: None,
            content: initial,
            cursor_pos,
            completion_table: None,
            require_match: false,
            default_value: None,
            active: true,
            depth,
        }
    }
}

// ---------------------------------------------------------------------------
// MinibufferHistory
// ---------------------------------------------------------------------------

/// Named history lists (e.g. "minibuffer-history", "file-name-history", ...).
pub struct MinibufferHistory {
    histories: HashMap<String, Vec<String>>,
    max_length: usize,
}

impl MinibufferHistory {
    pub fn new() -> Self {
        Self {
            histories: HashMap::new(),
            max_length: 100,
        }
    }

    pub fn get(&self, name: &str) -> &[String] {
        match self.histories.get(name) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }

    pub fn add(&mut self, name: &str, value: &str) {
        let list = self
            .histories
            .entry(name.to_string())
            .or_insert_with(Vec::new);
        // Avoid consecutive duplicates at the front.
        if list.first().map(|s| s.as_str()) != Some(value) {
            list.insert(0, value.to_string());
        }
        if list.len() > self.max_length {
            list.truncate(self.max_length);
        }
    }
}

impl Default for MinibufferHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// MinibufferManager
// ---------------------------------------------------------------------------

/// Owns all minibuffer state, including the recursive-edit stack.
pub struct MinibufferManager {
    state_stack: Vec<MinibufferState>,
    history: MinibufferHistory,
    completion_style: CompletionStyle,
    enable_recursive: bool,
    max_depth: usize,
}

impl MinibufferManager {
    pub fn new() -> Self {
        Self {
            state_stack: Vec::new(),
            history: MinibufferHistory::new(),
            completion_style: CompletionStyle::Prefix,
            enable_recursive: true,
            max_depth: 10,
        }
    }

    /// Enter the minibuffer with the given prompt and optional initial input / history name.
    ///
    /// Returns a fresh `MinibufferState` that has been pushed onto the stack.
    /// The caller can further configure it (completion table, require-match, default).
    pub(crate) fn read_from_minibuffer(
        &mut self,
        prompt: &str,
        initial: Option<&str>,
        history_name: Option<&str>,
    ) -> Result<&mut MinibufferState, Flow> {
        let new_depth = self.state_stack.len() + 1;
        if new_depth > self.max_depth {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Command attempted to use minibuffer while in minibuffer",
                )],
            ));
        }
        if !self.enable_recursive && !self.state_stack.is_empty() {
            return Err(signal(
                "error",
                vec![Value::string(
                    "Command attempted to use minibuffer while in minibuffer",
                )],
            ));
        }

        let initial_str = initial.unwrap_or("").to_string();
        let mut state = MinibufferState::new(prompt.to_string(), initial_str, new_depth);

        // Pre-populate history from the named list.
        if let Some(name) = history_name {
            state.history = self.history.get(name).to_vec();
        }

        self.state_stack.push(state);
        // Safety: we just pushed, so unwrap is fine.
        Ok(self.state_stack.last_mut().unwrap())
    }

    /// Attempt to complete the current minibuffer content.
    pub fn try_complete(&self, state: &MinibufferState) -> CompletionResult {
        match &state.completion_table {
            Some(table) => {
                let input = &state.content;
                let matches = self.all_completions(input, table);
                let common = compute_common_prefix(&matches);
                let exhaustive = !matches!(table, CompletionTable::Function(_));
                CompletionResult {
                    matches,
                    common_prefix: common,
                    exhaustive,
                }
            }
            None => CompletionResult {
                matches: Vec::new(),
                common_prefix: None,
                exhaustive: true,
            },
        }
    }

    /// Return all completions of `prefix` against `table`.
    pub fn all_completions(&self, prefix: &str, table: &CompletionTable) -> Vec<String> {
        let candidates = table.candidates(prefix);
        match self.completion_style {
            CompletionStyle::Prefix => prefix_match(prefix, &candidates),
            CompletionStyle::Substring => substring_match(prefix, &candidates),
            CompletionStyle::Flex => flex_match(prefix, &candidates),
            CompletionStyle::Basic => basic_match(prefix, &candidates),
        }
    }

    /// Try to complete `prefix` to the longest common prefix of all matches.
    /// Returns `None` if there are no matches.
    pub fn try_completion_string(&self, prefix: &str, table: &CompletionTable) -> Option<String> {
        let matches = self.all_completions(prefix, table);
        compute_common_prefix(&matches)
    }

    /// Test whether `string` is an exact match in `table`.
    pub fn test_completion(&self, string: &str, table: &CompletionTable) -> bool {
        let candidates = table.candidates(string);
        candidates.iter().any(|c| c == string)
    }

    /// Exit the current minibuffer, returning its content (or the default if empty).
    pub fn exit_minibuffer(&mut self) -> Option<String> {
        if let Some(mut state) = self.state_stack.pop() {
            state.active = false;
            let result = if state.content.is_empty() {
                state.default_value.unwrap_or_default()
            } else {
                state.content.clone()
            };
            Some(result)
        } else {
            None
        }
    }

    /// Abort the current minibuffer (like C-g).
    pub fn abort_minibuffer(&mut self) {
        if let Some(mut state) = self.state_stack.pop() {
            state.active = false;
        }
    }

    /// Navigate to the previous (older) history entry.
    pub fn history_previous(&mut self) -> Option<String> {
        let state = self.state_stack.last_mut()?;
        let history = &state.history;
        if history.is_empty() {
            return None;
        }
        let new_pos = match state.history_position {
            None => 0,
            Some(p) => {
                if p + 1 < history.len() {
                    p + 1
                } else {
                    return None; // already at oldest
                }
            }
        };
        state.history_position = Some(new_pos);
        let entry = history[new_pos].clone();
        state.content = entry.clone();
        state.cursor_pos = entry.len();
        Some(entry)
    }

    /// Navigate to the next (newer) history entry.
    pub fn history_next(&mut self) -> Option<String> {
        let state = self.state_stack.last_mut()?;
        match state.history_position {
            None => None,
            Some(0) => {
                // Back to the original input.
                state.history_position = None;
                state.content = state.initial_input.clone();
                state.cursor_pos = state.initial_input.len();
                Some(state.initial_input.clone())
            }
            Some(p) => {
                let new_pos = p - 1;
                state.history_position = Some(new_pos);
                let entry = state.history[new_pos].clone();
                state.content = entry.clone();
                state.cursor_pos = entry.len();
                Some(entry)
            }
        }
    }

    /// Add a value to a named history list.
    pub fn add_to_history(&mut self, name: &str, value: &str) {
        self.history.add(name, value);
    }

    /// Reference to the current (innermost) minibuffer state, if any.
    pub fn current(&self) -> Option<&MinibufferState> {
        self.state_stack.last()
    }

    /// Mutable reference to the current (innermost) minibuffer state.
    pub fn current_mut(&mut self) -> Option<&mut MinibufferState> {
        self.state_stack.last_mut()
    }

    /// Current recursive minibuffer depth (0 = not in minibuffer).
    pub fn depth(&self) -> usize {
        self.state_stack.len()
    }

    /// Whether any minibuffer is currently active.
    pub fn is_active(&self) -> bool {
        self.state_stack.last().map_or(false, |s| s.active)
    }

    /// Set the completion style.
    pub fn set_completion_style(&mut self, style: CompletionStyle) {
        self.completion_style = style;
    }

    /// Set whether recursive minibuffers are allowed.
    pub fn set_enable_recursive(&mut self, enable: bool) {
        self.enable_recursive = enable;
    }
}

impl Default for MinibufferManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Completion matching functions
// ---------------------------------------------------------------------------

/// Case-insensitive prefix matching.
fn prefix_match(input: &str, candidates: &[String]) -> Vec<String> {
    let lower_input = input.to_lowercase();
    candidates
        .iter()
        .filter(|c| c.to_lowercase().starts_with(&lower_input))
        .cloned()
        .collect()
}

/// Substring matching (case-insensitive).
fn substring_match(input: &str, candidates: &[String]) -> Vec<String> {
    let lower_input = input.to_lowercase();
    candidates
        .iter()
        .filter(|c| c.to_lowercase().contains(&lower_input))
        .cloned()
        .collect()
}

/// Flex (fuzzy) matching: the input characters must appear in order within the candidate.
fn flex_match(input: &str, candidates: &[String]) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| is_flex_match(input, c))
        .cloned()
        .collect()
}

/// Check if all characters in `input` appear in order in `candidate` (case-insensitive).
fn is_flex_match(input: &str, candidate: &str) -> bool {
    let mut chars = candidate.chars().flat_map(|c| c.to_lowercase());
    for ic in input.chars().flat_map(|c| c.to_lowercase()) {
        loop {
            match chars.next() {
                Some(cc) if cc == ic => break,
                Some(_) => continue,
                None => return false,
            }
        }
    }
    true
}

/// Exact (case-sensitive) prefix matching.
fn basic_match(input: &str, candidates: &[String]) -> Vec<String> {
    candidates
        .iter()
        .filter(|c| c.starts_with(input))
        .cloned()
        .collect()
}

/// Compute the longest common prefix of a set of strings.
/// Returns `None` if the set is empty.
fn compute_common_prefix(strings: &[String]) -> Option<String> {
    if strings.is_empty() {
        return None;
    }
    let first = &strings[0];
    let mut prefix_len = first.len();
    for s in &strings[1..] {
        prefix_len = first
            .chars()
            .zip(s.chars())
            .take(prefix_len)
            .take_while(|(a, b)| a == b)
            .count();
        if prefix_len == 0 {
            return Some(String::new());
        }
    }
    // `prefix_len` is in *chars*; collect the first `prefix_len` chars.
    Some(first.chars().take(prefix_len).collect())
}

// ---------------------------------------------------------------------------
// Builtin functions for Elisp
// ---------------------------------------------------------------------------

/// `(read-from-minibuffer PROMPT &optional INITIAL KEYMAP READ HISTORY DEFAULT INHERIT)`
///
/// Stub: returns INITIAL (or DEFAULT or "").
pub(crate) fn builtin_read_from_minibuffer(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-from-minibuffer", &args, 1)?;
    let _prompt = expect_string(&args[0])?;

    // INITIAL (arg 1)
    if let Some(initial) = args.get(1) {
        if !initial.is_nil() {
            let s = expect_string(initial)?;
            return Ok(Value::string(s));
        }
    }

    // DEFAULT (arg 5)
    if let Some(default) = args.get(5) {
        if !default.is_nil() {
            let s = expect_string(default)?;
            return Ok(Value::string(s));
        }
    }

    Ok(Value::string(""))
}

/// `(read-string PROMPT &optional INITIAL HISTORY DEFAULT INHERIT)`
///
/// Stub: returns INITIAL or DEFAULT or "".
pub(crate) fn builtin_read_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-string", &args, 1)?;
    let _prompt = expect_string(&args[0])?;

    // INITIAL (arg 1)
    if let Some(initial) = args.get(1) {
        if !initial.is_nil() {
            let s = expect_string(initial)?;
            return Ok(Value::string(s));
        }
    }

    // DEFAULT (arg 3)
    if let Some(default) = args.get(3) {
        if !default.is_nil() {
            let s = expect_string(default)?;
            return Ok(Value::string(s));
        }
    }

    Ok(Value::string(""))
}

/// `(read-number PROMPT &optional DEFAULT)`
///
/// Stub: returns DEFAULT or 0.
pub(crate) fn builtin_read_number(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-number", &args, 1)?;
    let _prompt = expect_string(&args[0])?;

    if let Some(default) = args.get(1) {
        if !default.is_nil() {
            match default {
                Value::Int(n) => return Ok(Value::Int(*n)),
                Value::Float(f) => return Ok(Value::Float(*f)),
                _ => {}
            }
        }
    }

    Ok(Value::Int(0))
}

/// `(completing-read PROMPT COLLECTION &optional PREDICATE REQUIRE-MATCH INITIAL HIST DEF INHERIT)`
///
/// Stub: returns INITIAL or DEF or "".
pub(crate) fn builtin_completing_read(args: Vec<Value>) -> EvalResult {
    expect_min_args("completing-read", &args, 2)?;
    let _prompt = expect_string(&args[0])?;
    // COLLECTION (arg 1) is ignored in stub

    // INITIAL (arg 4)
    if let Some(initial) = args.get(4) {
        if !initial.is_nil() {
            let s = expect_string(initial)?;
            return Ok(Value::string(s));
        }
    }

    // DEF (arg 6)
    if let Some(def) = args.get(6) {
        if !def.is_nil() {
            if let Ok(s) = expect_string(def) {
                return Ok(Value::string(s));
            }
        }
    }

    Ok(Value::string(""))
}

/// `(read-file-name PROMPT &optional DIR DEFAULT MUSTMATCH INITIAL PREDICATE)`
///
/// Stub: returns INITIAL or DEFAULT or "".
pub(crate) fn builtin_read_file_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-file-name", &args, 1)?;
    expect_max_args("read-file-name", &args, 6)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

/// `(read-directory-name PROMPT &optional DIR DEFAULT MUSTMATCH INITIAL)`
///
/// Stub: returns INITIAL or DEFAULT or "".
pub(crate) fn builtin_read_directory_name(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-directory-name", &args, 1)?;
    expect_max_args("read-directory-name", &args, 5)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

/// `(read-buffer PROMPT &optional DEFAULT REQUIRE-MATCH PREDICATE)`
///
/// Stub: returns DEFAULT or "".
pub(crate) fn builtin_read_buffer(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-buffer", &args, 1)?;
    expect_max_args("read-buffer", &args, 4)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

/// `(read-command PROMPT &optional DEFAULT)`
///
/// Stub: returns DEFAULT or "".
pub(crate) fn builtin_read_command(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-command", &args, 1)?;
    expect_max_args("read-command", &args, 2)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

/// `(read-variable PROMPT &optional DEFAULT)`
///
/// Stub: returns DEFAULT or "".
pub(crate) fn builtin_read_variable(args: Vec<Value>) -> EvalResult {
    expect_min_args("read-variable", &args, 1)?;
    expect_max_args("read-variable", &args, 2)?;
    let _prompt = expect_string(&args[0])?;
    Err(signal("end-of-file", vec![]))
}

/// `(try-completion STRING COLLECTION &optional PREDICATE)`
///
/// Returns:
/// - `t` if STRING is an exact and unique match
/// - a string (the longest common prefix) if there are matches
/// - `nil` if no matches
pub(crate) fn builtin_try_completion(args: Vec<Value>) -> EvalResult {
    expect_min_args("try-completion", &args, 2)?;
    expect_max_args("try-completion", &args, 3)?;
    let string = expect_string(&args[0])?;
    let candidates = value_to_string_list(&args[1]);

    let matches: Vec<String> = candidates
        .iter()
        .filter(|c| c.starts_with(&string))
        .cloned()
        .collect();

    if matches.is_empty() {
        return Ok(Value::Nil);
    }

    // Exact unique match?
    if matches.len() == 1 && matches[0] == string {
        return Ok(Value::True);
    }

    // Compute longest common prefix.
    match compute_common_prefix(&matches) {
        Some(prefix) => Ok(Value::string(prefix)),
        None => Ok(Value::Nil),
    }
}

/// `(all-completions STRING COLLECTION &optional PREDICATE)`
///
/// Returns a list of all completions of STRING in COLLECTION.
pub(crate) fn builtin_all_completions(args: Vec<Value>) -> EvalResult {
    expect_min_args("all-completions", &args, 2)?;
    expect_max_args("all-completions", &args, 4)?;
    let string = expect_string(&args[0])?;
    let candidates = value_to_string_list(&args[1]);

    let matches: Vec<Value> = candidates
        .iter()
        .filter(|c| c.starts_with(&string))
        .map(|c| Value::string(c.clone()))
        .collect();

    Ok(Value::list(matches))
}

/// `(test-completion STRING COLLECTION &optional PREDICATE)`
///
/// Returns t if STRING is an exact match in COLLECTION, nil otherwise.
pub(crate) fn builtin_test_completion(args: Vec<Value>) -> EvalResult {
    expect_min_args("test-completion", &args, 2)?;
    expect_max_args("test-completion", &args, 3)?;
    let string = expect_string(&args[0])?;
    let candidates = value_to_string_list(&args[1]);
    Ok(Value::bool(candidates.iter().any(|c| c == &string)))
}

/// `(minibuffer-prompt)` — returns the current minibuffer prompt or nil.
///
/// Stub: returns nil (no active minibuffer in non-interactive mode).
pub(crate) fn builtin_minibuffer_prompt(args: Vec<Value>) -> EvalResult {
    expect_args("minibuffer-prompt", &args, 0)?;
    Ok(Value::Nil)
}

/// `(minibuffer-contents)` — returns the current minibuffer contents.
///
/// Stub: returns "".
pub(crate) fn builtin_minibuffer_contents(args: Vec<Value>) -> EvalResult {
    expect_args("minibuffer-contents", &args, 0)?;
    Ok(Value::string(""))
}

/// `(minibuffer-depth)` — returns the current recursive minibuffer depth.
///
/// Stub: returns 0.
pub(crate) fn builtin_minibuffer_depth(args: Vec<Value>) -> EvalResult {
    expect_args("minibuffer-depth", &args, 0)?;
    Ok(Value::Int(0))
}

/// `(minibufferp &optional BUFFER)` — returns t if BUFFER is a minibuffer.
///
/// Stub: always returns nil.
pub(crate) fn builtin_minibufferp(args: Vec<Value>) -> EvalResult {
    // 0 or 1 arg
    if args.len() > 1 {
        return Err(signal(
            "wrong-number-of-arguments",
            vec![Value::symbol("minibufferp"), Value::Int(args.len() as i64)],
        ));
    }
    Ok(Value::Nil)
}

/// `(exit-minibuffer)` — exit the active minibuffer.
///
/// Stub: signals an error if not in a minibuffer.
pub(crate) fn builtin_exit_minibuffer(args: Vec<Value>) -> EvalResult {
    expect_args("exit-minibuffer", &args, 0)?;
    // In a real interactive session this would exit the minibuffer.
    // Stub: signal an error since we're non-interactive.
    Err(signal("error", vec![Value::string("Not in a minibuffer")]))
}

/// `(abort-recursive-edit)` — abort the innermost recursive edit.
///
/// Stub: signals 'quit.
pub(crate) fn builtin_abort_recursive_edit(args: Vec<Value>) -> EvalResult {
    expect_args("abort-recursive-edit", &args, 0)?;
    Err(signal("quit", vec![]))
}

// ---------------------------------------------------------------------------
// Value-to-string-list conversion helper
// ---------------------------------------------------------------------------

/// Extract a list of strings from a Value.
///
/// Handles:
/// - Proper list of strings
/// - Alist of (string . _) pairs
/// - Vector of strings
/// - nil → empty
fn value_to_string_list(val: &Value) -> Vec<String> {
    match val {
        Value::Nil => Vec::new(),
        Value::Cons(_) => {
            let items = match super::value::list_to_vec(val) {
                Some(v) => v,
                None => return Vec::new(),
            };
            items
                .iter()
                .filter_map(|item| match item {
                    Value::Str(s) => Some((**s).clone()),
                    Value::Symbol(s) => Some(s.clone()),
                    // Alist entry: (STRING . _)
                    Value::Cons(cell) => {
                        let pair = cell.lock().ok()?;
                        match &pair.car {
                            Value::Str(s) => Some((**s).clone()),
                            Value::Symbol(s) => Some(s.clone()),
                            _ => None,
                        }
                    }
                    _ => None,
                })
                .collect()
        }
        Value::Vector(v) => {
            let vec = v.lock().expect("poisoned");
            vec.iter()
                .filter_map(|item| match item {
                    Value::Str(s) => Some((**s).clone()),
                    Value::Symbol(s) => Some(s.clone()),
                    _ => None,
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -- Completion matching --------------------------------------------------

    #[test]
    fn prefix_match_basic() {
        let candidates = vec![
            "apple".into(),
            "application".into(),
            "banana".into(),
            "apply".into(),
        ];
        let result = prefix_match("app", &candidates);
        assert_eq!(result.len(), 3);
        assert!(result.contains(&"apple".to_string()));
        assert!(result.contains(&"application".to_string()));
        assert!(result.contains(&"apply".to_string()));
    }

    #[test]
    fn prefix_match_case_insensitive() {
        let candidates = vec!["Apple".into(), "APPLY".into(), "banana".into()];
        let result = prefix_match("app", &candidates);
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn prefix_match_empty_input() {
        let candidates = vec!["a".into(), "b".into(), "c".into()];
        let result = prefix_match("", &candidates);
        assert_eq!(result.len(), 3);
    }

    #[test]
    fn prefix_match_no_matches() {
        let candidates = vec!["apple".into(), "banana".into()];
        let result = prefix_match("zz", &candidates);
        assert!(result.is_empty());
    }

    #[test]
    fn substring_match_basic() {
        let candidates = vec![
            "foobar".into(),
            "bazfoo".into(),
            "hello".into(),
            "food".into(),
        ];
        let result = substring_match("foo", &candidates);
        assert_eq!(result.len(), 3);
        assert!(result.contains(&"foobar".to_string()));
        assert!(result.contains(&"bazfoo".to_string()));
        assert!(result.contains(&"food".to_string()));
    }

    #[test]
    fn flex_match_basic() {
        let candidates = vec![
            "find-file".into(),
            "flycheck".into(),
            "first-foo".into(),
            "hello".into(),
        ];
        // "ff" should match strings where 'f' appears twice in order.
        let result = flex_match("ff", &candidates);
        assert!(result.contains(&"find-file".to_string()));
        assert!(result.contains(&"first-foo".to_string()));
        assert!(!result.contains(&"hello".to_string()));
    }

    #[test]
    fn flex_match_all_chars_in_order() {
        let candidates = vec!["abcdef".into(), "axbycz".into(), "zzz".into()];
        let result = flex_match("abc", &candidates);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"abcdef".to_string()));
        assert!(result.contains(&"axbycz".to_string()));
    }

    #[test]
    fn flex_match_case_insensitive() {
        let candidates = vec!["FindFile".into()];
        let result = flex_match("ff", &candidates);
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn basic_match_case_sensitive() {
        let candidates = vec!["Apple".into(), "apple".into(), "application".into()];
        let result = basic_match("app", &candidates);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"apple".to_string()));
        assert!(result.contains(&"application".to_string()));
        assert!(!result.contains(&"Apple".to_string()));
    }

    // -- Common prefix --------------------------------------------------------

    #[test]
    fn common_prefix_empty() {
        assert!(compute_common_prefix(&[]).is_none());
    }

    #[test]
    fn common_prefix_single() {
        let strings = vec!["hello".to_string()];
        assert_eq!(compute_common_prefix(&strings), Some("hello".to_string()));
    }

    #[test]
    fn common_prefix_multiple() {
        let strings = vec![
            "application".to_string(),
            "apple".to_string(),
            "apply".to_string(),
        ];
        assert_eq!(compute_common_prefix(&strings), Some("appl".to_string()));
    }

    #[test]
    fn common_prefix_no_overlap() {
        let strings = vec!["abc".to_string(), "xyz".to_string()];
        assert_eq!(compute_common_prefix(&strings), Some(String::new()));
    }

    #[test]
    fn common_prefix_identical() {
        let strings = vec!["test".to_string(), "test".to_string()];
        assert_eq!(compute_common_prefix(&strings), Some("test".to_string()));
    }

    // -- History navigation ---------------------------------------------------

    #[test]
    fn history_navigation() {
        let mut mgr = MinibufferManager::new();
        mgr.add_to_history("test-history", "first");
        mgr.add_to_history("test-history", "second");
        mgr.add_to_history("test-history", "third");

        // Enter minibuffer with history.
        mgr.read_from_minibuffer("prompt: ", None, Some("test-history"))
            .unwrap();

        // Go back in history: should get "third" (most recent).
        let prev = mgr.history_previous();
        assert_eq!(prev, Some("third".to_string()));

        // Go back again: "second".
        let prev = mgr.history_previous();
        assert_eq!(prev, Some("second".to_string()));

        // Go forward: back to "third".
        let next = mgr.history_next();
        assert_eq!(next, Some("third".to_string()));

        // Go forward again: back to original input (empty string).
        let next = mgr.history_next();
        assert_eq!(next, Some(String::new()));

        // Go forward past the start: None.
        let next = mgr.history_next();
        assert_eq!(next, None);

        // Clean up.
        mgr.exit_minibuffer();
    }

    #[test]
    fn history_dedup() {
        let mut mgr = MinibufferManager::new();
        mgr.add_to_history("h", "same");
        mgr.add_to_history("h", "same");
        mgr.add_to_history("h", "same");
        assert_eq!(mgr.history.get("h").len(), 1);

        mgr.add_to_history("h", "different");
        assert_eq!(mgr.history.get("h").len(), 2);
        assert_eq!(mgr.history.get("h")[0], "different");
        assert_eq!(mgr.history.get("h")[1], "same");
    }

    // -- Recursive minibuffer depth -------------------------------------------

    #[test]
    fn recursive_depth() {
        let mut mgr = MinibufferManager::new();
        assert_eq!(mgr.depth(), 0);
        assert!(!mgr.is_active());

        mgr.read_from_minibuffer("1: ", None, None).unwrap();
        assert_eq!(mgr.depth(), 1);
        assert!(mgr.is_active());

        mgr.read_from_minibuffer("2: ", None, None).unwrap();
        assert_eq!(mgr.depth(), 2);

        mgr.exit_minibuffer();
        assert_eq!(mgr.depth(), 1);

        mgr.exit_minibuffer();
        assert_eq!(mgr.depth(), 0);
        assert!(!mgr.is_active());
    }

    #[test]
    fn recursive_depth_limit() {
        let mut mgr = MinibufferManager::new();
        mgr.max_depth = 2;

        mgr.read_from_minibuffer("1: ", None, None).unwrap();
        mgr.read_from_minibuffer("2: ", None, None).unwrap();
        let result = mgr.read_from_minibuffer("3: ", None, None);
        assert!(result.is_err());
    }

    #[test]
    fn recursive_disabled() {
        let mut mgr = MinibufferManager::new();
        mgr.set_enable_recursive(false);

        mgr.read_from_minibuffer("1: ", None, None).unwrap();
        let result = mgr.read_from_minibuffer("2: ", None, None);
        assert!(result.is_err());
    }

    // -- Minibuffer enter/exit lifecycle --------------------------------------

    #[test]
    fn enter_exit_lifecycle() {
        let mut mgr = MinibufferManager::new();

        {
            let state = mgr
                .read_from_minibuffer("Enter: ", Some("init"), None)
                .unwrap();
            assert_eq!(state.prompt, "Enter: ");
            assert_eq!(state.content, "init");
            assert!(state.active);
            assert_eq!(state.depth, 1);
        }

        // Modify content
        {
            let state = mgr.current_mut().unwrap();
            state.content = "modified".to_string();
        }

        let result = mgr.exit_minibuffer();
        assert_eq!(result, Some("modified".to_string()));
        assert_eq!(mgr.depth(), 0);
    }

    #[test]
    fn exit_with_default() {
        let mut mgr = MinibufferManager::new();
        {
            let state = mgr.read_from_minibuffer("Enter: ", None, None).unwrap();
            state.default_value = Some("fallback".to_string());
            // Content is empty, so default should be used.
        }
        let result = mgr.exit_minibuffer();
        assert_eq!(result, Some("fallback".to_string()));
    }

    #[test]
    fn abort_minibuffer_clears_state() {
        let mut mgr = MinibufferManager::new();
        mgr.read_from_minibuffer("Enter: ", None, None).unwrap();
        assert_eq!(mgr.depth(), 1);
        mgr.abort_minibuffer();
        assert_eq!(mgr.depth(), 0);
        assert!(!mgr.is_active());
    }

    #[test]
    fn exit_empty_stack() {
        let mut mgr = MinibufferManager::new();
        assert_eq!(mgr.exit_minibuffer(), None);
    }

    // -- MinibufferManager completion -----------------------------------------

    #[test]
    fn try_complete_with_table() {
        let mut mgr = MinibufferManager::new();
        {
            let state = mgr
                .read_from_minibuffer("M-x ", Some("find"), None)
                .unwrap();
            state.completion_table = Some(CompletionTable::List(vec![
                "find-file".into(),
                "find-file-other-window".into(),
                "find-tag".into(),
                "forward-char".into(),
            ]));
        }
        let state = mgr.current().unwrap();
        let result = mgr.try_complete(state);
        assert_eq!(result.matches.len(), 3); // find-file, find-file-other-window, find-tag
        assert_eq!(result.common_prefix, Some("find-".to_string()));
        mgr.exit_minibuffer();
    }

    #[test]
    fn test_completion_exact_match() {
        let mgr = MinibufferManager::new();
        let table = CompletionTable::List(vec!["apple".into(), "banana".into(), "cherry".into()]);
        assert!(mgr.test_completion("apple", &table));
        assert!(mgr.test_completion("banana", &table));
        assert!(!mgr.test_completion("app", &table));
        assert!(!mgr.test_completion("APPLE", &table));
    }

    #[test]
    fn try_completion_string_result() {
        let mgr = MinibufferManager::new();
        let table =
            CompletionTable::List(vec!["application".into(), "apple".into(), "apply".into()]);
        let result = mgr.try_completion_string("app", &table);
        assert_eq!(result, Some("appl".to_string()));
    }

    #[test]
    fn all_completions_empty() {
        let mgr = MinibufferManager::new();
        let table = CompletionTable::List(vec!["foo".into(), "bar".into()]);
        let result = mgr.all_completions("zzz", &table);
        assert!(result.is_empty());
    }

    // -- Completion with different styles -------------------------------------

    #[test]
    fn completion_style_substring() {
        let mut mgr = MinibufferManager::new();
        mgr.set_completion_style(CompletionStyle::Substring);
        let table = CompletionTable::List(vec![
            "find-file".into(),
            "describe-file".into(),
            "file-name".into(),
        ]);
        let result = mgr.all_completions("file", &table);
        assert_eq!(result.len(), 3); // All contain "file"
    }

    #[test]
    fn completion_style_flex() {
        let mut mgr = MinibufferManager::new();
        mgr.set_completion_style(CompletionStyle::Flex);
        let table = CompletionTable::List(vec![
            "find-file".into(),
            "forward-char".into(),
            "flycheck".into(),
        ]);
        // "ff" should flex-match "find-file" and "flycheck" (f...f? no, flycheck has no second f)
        // Actually: "find-file" has f...f, "flycheck" has f but only one f total.
        let result = mgr.all_completions("ff", &table);
        assert!(result.contains(&"find-file".to_string()));
        // "flycheck" has only one 'f', so "ff" won't match it.
        assert!(!result.contains(&"flycheck".to_string()));
    }

    #[test]
    fn completion_style_basic_case_sensitive() {
        let mut mgr = MinibufferManager::new();
        mgr.set_completion_style(CompletionStyle::Basic);
        let table =
            CompletionTable::List(vec!["Apple".into(), "apple".into(), "application".into()]);
        let result = mgr.all_completions("app", &table);
        assert_eq!(result.len(), 2);
        assert!(result.contains(&"apple".to_string()));
        assert!(result.contains(&"application".to_string()));
    }

    // -- Alist completion table -----------------------------------------------

    #[test]
    fn alist_completion() {
        let mgr = MinibufferManager::new();
        let table = CompletionTable::Alist(vec![
            ("alpha".into(), Value::Int(1)),
            ("beta".into(), Value::Int(2)),
            ("alphabetical".into(), Value::Int(3)),
        ]);
        let result = mgr.all_completions("alph", &table);
        assert_eq!(result.len(), 2);
    }

    // -- Builtin stubs --------------------------------------------------------

    #[test]
    fn builtin_read_from_minibuffer_returns_initial() {
        let result =
            builtin_read_from_minibuffer(vec![Value::string("prompt: "), Value::string("init")])
                .unwrap();
        assert!(matches!(result, Value::Str(ref s) if &**s == "init"));
    }

    #[test]
    fn builtin_read_from_minibuffer_returns_empty() {
        let result = builtin_read_from_minibuffer(vec![Value::string("prompt: ")]).unwrap();
        assert!(matches!(result, Value::Str(ref s) if s.is_empty()));
    }

    #[test]
    fn builtin_read_string_with_default() {
        let result = builtin_read_string(vec![
            Value::string("Name: "),
            Value::Nil,
            Value::Nil,
            Value::string("default-val"),
        ])
        .unwrap();
        assert!(matches!(result, Value::Str(ref s) if &**s == "default-val"));
    }

    #[test]
    fn builtin_read_number_default() {
        let result = builtin_read_number(vec![Value::string("N: "), Value::Int(42)]).unwrap();
        assert!(matches!(result, Value::Int(42)));
    }

    #[test]
    fn builtin_read_number_no_default() {
        let result = builtin_read_number(vec![Value::string("N: ")]).unwrap();
        assert!(matches!(result, Value::Int(0)));
    }

    #[test]
    fn builtin_try_completion_unique_exact() {
        // Exact unique match should return t.
        let coll = Value::list(vec![Value::string("unique"), Value::string("other")]);
        let result = builtin_try_completion(vec![Value::string("unique"), coll]).unwrap();
        assert!(matches!(result, Value::True));
    }

    #[test]
    fn builtin_try_completion_common_prefix() {
        let coll = Value::list(vec![Value::string("application"), Value::string("apple")]);
        let result = builtin_try_completion(vec![Value::string("app"), coll]).unwrap();
        assert!(matches!(result, Value::Str(ref s) if &**s == "appl"));
    }

    #[test]
    fn builtin_try_completion_no_match() {
        let coll = Value::list(vec![Value::string("foo"), Value::string("bar")]);
        let result = builtin_try_completion(vec![Value::string("zzz"), coll]).unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn builtin_try_completion_rejects_more_than_three_args() {
        let coll = Value::list(vec![Value::string("a")]);
        let result =
            builtin_try_completion(vec![Value::string(""), coll, Value::Nil, Value::Nil]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_all_completions_returns_list() {
        let coll = Value::list(vec![
            Value::string("apple"),
            Value::string("application"),
            Value::string("banana"),
        ]);
        let result = builtin_all_completions(vec![Value::string("app"), coll]).unwrap();
        let items = super::super::value::list_to_vec(&result).unwrap();
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn builtin_all_completions_rejects_more_than_four_args() {
        let coll = Value::list(vec![Value::string("a")]);
        let result = builtin_all_completions(vec![
            Value::string(""),
            coll,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_test_completion_match() {
        let coll = Value::list(vec![Value::string("alpha"), Value::string("beta")]);
        let result = builtin_test_completion(vec![Value::string("alpha"), coll]).unwrap();
        assert!(matches!(result, Value::True));
    }

    #[test]
    fn builtin_test_completion_no_match() {
        let coll = Value::list(vec![Value::string("alpha"), Value::string("beta")]);
        let result = builtin_test_completion(vec![Value::string("alp"), coll]).unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn builtin_test_completion_rejects_more_than_three_args() {
        let coll = Value::list(vec![Value::string("a")]);
        let result =
            builtin_test_completion(vec![Value::string(""), coll, Value::Nil, Value::Nil]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_minibuffer_depth_returns_zero() {
        let result = builtin_minibuffer_depth(vec![]).unwrap();
        assert!(matches!(result, Value::Int(0)));
    }

    #[test]
    fn builtin_minibufferp_returns_nil() {
        let result = builtin_minibufferp(vec![]).unwrap();
        assert!(matches!(result, Value::Nil));
    }

    #[test]
    fn builtin_abort_recursive_edit_signals_quit() {
        let result = builtin_abort_recursive_edit(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_completing_read_returns_default() {
        let coll = Value::list(vec![Value::string("a"), Value::string("b")]);
        let result = builtin_completing_read(vec![
            Value::string("Choose: "),
            coll,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::string("default-choice"),
        ])
        .unwrap();
        assert!(matches!(result, Value::Str(ref s) if &**s == "default-choice"));
    }

    #[test]
    fn builtin_read_file_name_signals_end_of_file() {
        let result = builtin_read_file_name(vec![
            Value::string("File: "),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::string("/tmp/test.txt"),
        ]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "end-of-file"
        ));
    }

    #[test]
    fn builtin_read_file_name_rejects_more_than_six_args() {
        let result = builtin_read_file_name(vec![
            Value::string("File: "),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_read_buffer_signals_end_of_file() {
        let result =
            builtin_read_buffer(vec![Value::string("Buffer: "), Value::string("*scratch*")]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "end-of-file"
        ));
    }

    #[test]
    fn builtin_read_directory_name_rejects_more_than_five_args() {
        let result = builtin_read_directory_name(vec![
            Value::string("Directory: "),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_read_buffer_rejects_more_than_four_args() {
        let result = builtin_read_buffer(vec![
            Value::string("Buffer: "),
            Value::Nil,
            Value::Nil,
            Value::Nil,
            Value::Nil,
        ]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_read_command_rejects_more_than_two_args() {
        let result = builtin_read_command(vec![Value::string("Command: "), Value::Nil, Value::Nil]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    #[test]
    fn builtin_read_variable_rejects_more_than_two_args() {
        let result = builtin_read_variable(vec![Value::string("Variable: "), Value::Nil, Value::Nil]);
        assert!(matches!(
            result,
            Err(Flow::Signal(sig)) if sig.symbol == "wrong-number-of-arguments"
        ));
    }

    // -- value_to_string_list -------------------------------------------------

    #[test]
    fn value_to_string_list_from_list() {
        let list = Value::list(vec![
            Value::string("foo"),
            Value::string("bar"),
            Value::string("baz"),
        ]);
        let result = value_to_string_list(&list);
        assert_eq!(result, vec!["foo", "bar", "baz"]);
    }

    #[test]
    fn value_to_string_list_from_alist() {
        let alist = Value::list(vec![
            Value::cons(Value::string("key1"), Value::Int(1)),
            Value::cons(Value::string("key2"), Value::Int(2)),
        ]);
        let result = value_to_string_list(&alist);
        assert_eq!(result, vec!["key1", "key2"]);
    }

    #[test]
    fn value_to_string_list_from_nil() {
        let result = value_to_string_list(&Value::Nil);
        assert!(result.is_empty());
    }

    #[test]
    fn value_to_string_list_from_vector() {
        let vec = Value::vector(vec![Value::string("a"), Value::string("b")]);
        let result = value_to_string_list(&vec);
        assert_eq!(result, vec!["a", "b"]);
    }
}
