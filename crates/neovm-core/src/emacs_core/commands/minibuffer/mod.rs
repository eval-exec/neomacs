//! Minibuffer and completion system.
//!
//! Provides:
//! - `MinibufferManager` — owns all minibuffer state, history, and completion logic
//! - `CompletionTable` — what can be completed against (list, function, file names, etc.)
//! - `CompletionStyle` — matching strategy (prefix, substring, flex, basic)
//! - Builtin functions for Elisp: `read-from-minibuffer`, `completing-read`, `y-or-n-p`, etc.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args, expect_min_args};
use std::collections::HashMap;
use std::num::NonZeroUsize;
use strum::IntoStaticStr;

use crate::buffer::{BufferId, BufferManager, EmacsBytePos, EmacsByteRange, LispCharPos1};
use crate::heap_types::LispString;

use super::error::{EvalResult, Flow, signal};
use super::hashtab::hash_key_to_visible_value;
use super::intern::{NIL_SYM_ID, SymId, T_SYM_ID, resolve_sym};
use super::reader::{KeyboardInputRuntime, MinibufferInputSource};
use super::symbol::Obarray;
use super::textprop::StickinessProperty;
use super::value::{Value, ValueKind, VecLikeType};

/// GNU completion state held in predeclared C variables/symbols rather than
/// rediscovered by name at every completion operation.
///
/// The closed enum makes the hot state domain exhaustive: adding another
/// variable requires assigning it a dedicated cache slot instead of silently
/// reintroducing runtime string interning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum CompletionStateVariable {
    CompletionIgnoreCase,
    CompletionRegexpList,
    Obarray,
}

impl CompletionStateVariable {
    #[inline(always)]
    fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static COMPLETION_IGNORE_CASE: OnceLock<SymId> = OnceLock::new();
        static COMPLETION_REGEXP_LIST: OnceLock<SymId> = OnceLock::new();
        static OBARRAY: OnceLock<SymId> = OnceLock::new();
        let name: &'static str = self.into();
        match self {
            Self::CompletionIgnoreCase => {
                *COMPLETION_IGNORE_CASE.get_or_init(|| super::intern::intern(name))
            }
            Self::CompletionRegexpList => {
                *COMPLETION_REGEXP_LIST.get_or_init(|| super::intern::intern(name))
            }
            Self::Obarray => *OBARRAY.get_or_init(|| super::intern::intern(name)),
        }
    }
}

// ---------------------------------------------------------------------------
// Argument helpers (local copies, same pattern as builtins.rs / builtins_extra.rs)
// ---------------------------------------------------------------------------

fn expect_lisp_string(value: &Value) -> Result<crate::heap_types::LispString, Flow> {
    value.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
}

fn first_default_value(default: Value) -> Value {
    match default.kind() {
        ValueKind::Cons => default.cons_car(),
        _other => default,
    }
}

fn normalize_symbol_reader_default(default: Value) -> Value {
    match first_default_value(default).kind() {
        ValueKind::Symbol(id) => Value::string(resolve_sym(id)),
        _other => first_default_value(default),
    }
}

fn normalize_buffer_reader_default(buffers: &BufferManager, default: Value) -> Value {
    let first = first_default_value(default);
    match first.kind() {
        ValueKind::Veclike(VecLikeType::Buffer) => first
            .as_buffer_id()
            .and_then(|id| buffers.get(id))
            .map(|buffer| buffer.name_value())
            .unwrap_or(first),
        _ => first,
    }
}

fn strip_read_buffer_prompt_suffix(prompt: &[u8]) -> &[u8] {
    if let Some(stripped) = prompt.strip_suffix(b": ") {
        stripped
    } else if let Some(stripped) = prompt.strip_suffix(b":") {
        stripped
    } else if let Some(stripped) = prompt.strip_suffix(b" ") {
        stripped
    } else {
        prompt
    }
}

fn prompt_lisp_from_bytes(bytes: Vec<u8>, multibyte: bool) -> LispString {
    if multibyte {
        LispString::from_emacs_bytes(bytes)
    } else {
        LispString::from_unibyte(bytes)
    }
}

/// Substitute `%s` (with DEFAULT) and `%%` (with `%`) in the
/// `minibuffer-default-prompt-format` string, over raw Emacs bytes so eight-bit
/// content in the default survives faithfully. `%s`/`%%` are ASCII, so matching
/// on bytes is safe even when the format string is multibyte.
fn format_default_prompt(format: &LispString, default: &LispString) -> LispString {
    let bytes = format.as_bytes();
    let fmt_multibyte = format.is_multibyte();
    let mut result = prompt_lisp_from_bytes(Vec::new(), false);
    let mut literal: Vec<u8> = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 1 < bytes.len() {
            match bytes[i + 1] {
                b's' => {
                    if !literal.is_empty() {
                        result = result.concat(&prompt_lisp_from_bytes(
                            std::mem::take(&mut literal),
                            fmt_multibyte,
                        ));
                    }
                    result = result.concat(default);
                    i += 2;
                    continue;
                }
                b'%' => {
                    literal.push(b'%');
                    i += 2;
                    continue;
                }
                _ => {}
            }
        }
        literal.push(bytes[i]);
        i += 1;
    }
    if !literal.is_empty() {
        result = result.concat(&prompt_lisp_from_bytes(literal, fmt_multibyte));
    }
    result
}

fn read_buffer_prompt(obarray: &Obarray, raw_prompt: Value, default: Value) -> Value {
    if default.is_nil() {
        return raw_prompt;
    }
    let Some(prompt) = raw_prompt.as_lisp_string() else {
        return raw_prompt;
    };
    let prompt_base = strip_read_buffer_prompt_suffix(prompt.as_bytes());
    let mut formatted = prompt_lisp_from_bytes(prompt_base.to_vec(), prompt.is_multibyte());
    if let Some(default_text) = default.as_lisp_string()
        && !default_text.as_bytes().is_empty()
    {
        let default_prompt_format = obarray
            .symbol_value("minibuffer-default-prompt-format")
            .and_then(|value| (*value).as_lisp_string().cloned())
            .unwrap_or_else(|| LispString::from_unibyte(b" (default %s)".to_vec()));
        formatted = formatted.concat(&format_default_prompt(&default_prompt_format, default_text));
    }
    formatted = formatted.concat(&LispString::from_unibyte(b": ".to_vec()));
    Value::heap_string(formatted)
}

// ---------------------------------------------------------------------------
// CompletionTable
// ---------------------------------------------------------------------------

/// What can be completed against.
pub enum CompletionTable {
    /// Fixed list of completion candidates.
    List(Vec<LispString>),
    /// Dynamic completion function: given the current input, returns matching candidates.
    #[allow(clippy::type_complexity)]
    // callable is the public dynamic-completion representation
    Function(Box<dyn Fn(&LispString) -> Vec<LispString>>),
    /// File name completion rooted at a directory.
    FileNames { directory: LispString },
    /// Buffer name completion (candidates supplied externally).
    BufferNames,
    /// Symbol name completion (candidates supplied externally).
    SymbolNames,
    /// Association list: each entry is (key, value).
    Alist(Vec<(LispString, Value)>),
}

impl CompletionTable {
    /// Extract the raw string candidates from the table.
    ///
    /// For `Function` tables the `input` is passed through; for static tables it
    /// is ignored (filtering happens later in the matching functions).
    fn candidates(&self, input: &LispString) -> Vec<LispString> {
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
fn list_files_in_dir(dir: &LispString) -> Vec<LispString> {
    let Some(dir) = dir.as_utf8_str() else {
        return Vec::new();
    };
    match std::fs::read_dir(dir) {
        Ok(entries) => entries
            .filter_map(|e| e.ok())
            .map(|e| LispString::from_utf8(&e.file_name().to_string_lossy()))
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
    pub matches: Vec<LispString>,
    /// Longest common prefix of all matches (if any).
    pub common_prefix: Option<LispString>,
    /// Whether the match list is exhaustive (i.e. we know there are no more).
    pub exhaustive: bool,
}

// ---------------------------------------------------------------------------
// MinibufferState
// ---------------------------------------------------------------------------

/// Tracks one active minibuffer interaction (possibly recursive).
pub struct MinibufferState {
    pub buffer_id: BufferId,
    pub prompt: LispString,
    pub prompt_end: usize,
    pub initial_input: LispString,
    pub history: Vec<LispString>,
    pub history_position: Option<usize>,
    pub content: LispString,
    pub cursor_pos: usize,
    pub completion_table: Option<CompletionTable>,
    /// The `require-match` argument from `completing-read`.
    ///
    /// Possible semantic values:
    /// - `nil` — no restriction, any input accepted
    /// - `t` (or any non-nil, non-`confirm`, non-`confirm-after-completion`)
    ///   — must match exactly
    /// - symbol `confirm` — may exit with non-match after a second RET
    /// - symbol `confirm-after-completion` — like `confirm` but only after
    ///   the user has triggered a completion at least once
    pub require_match: Value,
    pub default_value: Option<LispString>,
    pub active: bool,
    /// Recursive minibuffer depth at which this state was entered.
    pub depth: usize,
    /// Command-loop depth active when this minibuffer was entered.
    pub command_loop_depth: usize,
}

impl MinibufferState {
    fn new(buffer_id: BufferId, prompt: LispString, initial: LispString, depth: usize) -> Self {
        let cursor_pos = initial.byte_len();
        let prompt_end = prompt.sbytes();
        Self {
            buffer_id,
            prompt,
            prompt_end,
            initial_input: initial.clone(),
            history: Vec::new(),
            history_position: None,
            content: initial,
            cursor_pos,
            completion_table: None,
            require_match: Value::NIL,
            default_value: None,
            active: true,
            depth,
            command_loop_depth: 0,
        }
    }
}

pub(crate) fn install_minibuffer_buffer_text(
    buffers: &mut BufferManager,
    buffer_id: BufferId,
    prompt: &LispString,
    initial: Option<&LispString>,
    prompt_properties: Value,
) -> EmacsBytePos {
    // Match GNU `read_minibuf` / `erase-buffer`: clear the minibuffer through
    // the buffer edit pipeline so point, narrowing, and sibling state reset
    // together before we insert the new prompt and initial contents.
    let old_text_range = buffers
        .full_buffer_emacs_byte_range(buffer_id)
        .expect("minibuffer buffer");
    buffers
        .restore_buffer_emacs_byte_restriction(buffer_id, old_text_range)
        .expect("minibuffer buffer widen");
    if !old_text_range.is_empty() {
        buffers
            .delete_buffer_emacs_byte_range(buffer_id, old_text_range)
            .expect("minibuffer buffer delete");
    }
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, EmacsBytePos::new(0))
        .expect("minibuffer buffer goto");

    buffers
        .insert_lisp_string_into_buffer(buffer_id, prompt)
        .expect("minibuffer prompt insert");
    let prompt_end = buffers
        .get(buffer_id)
        .expect("minibuffer buffer")
        .full_emacs_byte_range()
        .end();
    if prompt_end.get() > 0 {
        let prompt_range = EmacsByteRange::new(EmacsBytePos::new(0), prompt_end);
        let buf = buffers.get_mut(buffer_id).expect("minibuffer buffer");
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            Value::symbol("field"),
            Value::T,
        );
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            StickinessProperty::FrontSticky.value(),
            Value::T,
        );
        buf.text_props_put_property_in_emacs_byte_range(
            prompt_range,
            StickinessProperty::RearNonsticky.value(),
            Value::T,
        );
        apply_minibuffer_prompt_properties(buf, prompt_end, prompt_properties);
    }

    if let Some(initial) = initial {
        buffers
            .insert_lisp_string_into_buffer(buffer_id, initial)
            .expect("minibuffer initial input insert");
    }

    let full_end = buffers
        .get(buffer_id)
        .expect("minibuffer buffer")
        .full_emacs_byte_range()
        .end();
    buffers
        .restore_buffer_emacs_byte_restriction(
            buffer_id,
            buffers
                .full_buffer_emacs_byte_range(buffer_id)
                .expect("minibuffer buffer"),
        )
        .expect("minibuffer buffer widen");
    buffers
        .goto_buffer_emacs_byte_pos(buffer_id, full_end)
        .expect("minibuffer buffer goto");
    prompt_end
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn default_minibuffer_prompt_properties() -> Value {
    Value::list(vec![Value::symbol("read-only"), Value::T])
}

fn apply_minibuffer_prompt_properties(
    buf: &mut crate::buffer::Buffer,
    prompt_end: EmacsBytePos,
    prompt_properties: Value,
) {
    let prompt_range = EmacsByteRange::new(EmacsBytePos::new(0), prompt_end);
    let mut cursor = prompt_properties;
    while cursor.is_cons() {
        let key = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !cursor.is_cons() {
            break;
        }
        let value = cursor.cons_car();
        cursor = cursor.cons_cdr();
        buf.text_props_put_property_in_emacs_byte_range(prompt_range, key, value);
    }
}

// ---------------------------------------------------------------------------
// MinibufferHistory
// ---------------------------------------------------------------------------

/// Named history lists (e.g. "minibuffer-history", "file-name-history", ...).
pub struct MinibufferHistory {
    histories: HashMap<SymId, Vec<LispString>>,
}

impl MinibufferHistory {
    pub fn new() -> Self {
        Self {
            histories: HashMap::new(),
        }
    }

    pub fn get(&self, name: SymId) -> &[LispString] {
        match self.histories.get(&name) {
            Some(v) => v.as_slice(),
            None => &[],
        }
    }

    /// Add a value to a named history list.
    ///
    /// `max_length` controls how many entries to keep.  Callers that have
    /// access to the obarray should read the `history-length` symbol and
    /// pass it here; the default in GNU Emacs is 100.
    pub fn add(&mut self, name: SymId, value: LispString, max_length: usize) {
        let list = self.histories.entry(name).or_default();
        // Avoid consecutive duplicates at the front.
        if list.first() != Some(&value) {
            list.insert(0, value);
        }
        if list.len() > max_length {
            list.truncate(max_length);
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

/// Whether a new minibuffer may be entered while another one is active.
///
/// Keeping the Lisp option out of `MinibufferManager` makes admission an
/// explicit operation for each read instead of mutable ambient state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RecursiveMinibufferPolicy {
    Allow,
    Reject,
}

/// The exhaustive reasons a minibuffer entry can be rejected before it has
/// changed any buffer, window, or command-loop state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MinibufferEntryRejection {
    RecursiveDisabled,
    #[cfg(test)]
    TestDepthLimit,
}

impl MinibufferEntryRejection {
    pub(crate) fn into_flow(self) -> Flow {
        let message = match self {
            Self::RecursiveDisabled => "Command attempted to use minibuffer while in minibuffer",
            #[cfg(test)]
            Self::TestDepthLimit => "Command attempted to use minibuffer while in minibuffer",
        };
        signal(LispCondition::UserError, vec![Value::string(message)])
    }
}

/// Single-use proof that entering the next minibuffer level is allowed.
///
/// The private fields prevent callers from manufacturing a permit. Consuming
/// it at entry and checking its parent depth keep admission and stack mutation
/// paired even as the setup path evolves.
#[must_use = "minibuffer admission must be consumed by enter_with_permit"]
#[derive(Debug)]
pub(crate) struct MinibufferEntryPermit {
    parent_depth: usize,
    depth: NonZeroUsize,
}

impl MinibufferEntryPermit {
    pub(crate) fn depth(&self) -> usize {
        self.depth.get()
    }
}

/// Owns all minibuffer state, including the recursive-edit stack.
pub struct MinibufferManager {
    state_stack: Vec<MinibufferState>,
    history: MinibufferHistory,
    completion_style: CompletionStyle,
    #[cfg(test)]
    max_depth: usize,
}

impl MinibufferManager {
    pub fn new() -> Self {
        Self {
            state_stack: Vec::new(),
            history: MinibufferHistory::new(),
            completion_style: CompletionStyle::Prefix,
            #[cfg(test)]
            max_depth: 10,
        }
    }

    /// Decide whether the next minibuffer level may be entered.
    ///
    /// This is deliberately side-effect free. GNU `read_minibuf` rejects a
    /// prohibited recursive read before selecting or clearing a minibuffer
    /// window; callers must obtain this permit before doing either.
    pub(crate) fn prepare_entry(
        &self,
        policy: RecursiveMinibufferPolicy,
    ) -> Result<MinibufferEntryPermit, MinibufferEntryRejection> {
        let parent_depth = self.state_stack.len();
        let depth = NonZeroUsize::new(
            parent_depth
                .checked_add(1)
                .expect("minibuffer depth overflow"),
        )
        .expect("next minibuffer depth is nonzero");
        #[cfg(test)]
        if depth.get() > self.max_depth {
            return Err(MinibufferEntryRejection::TestDepthLimit);
        }
        if policy == RecursiveMinibufferPolicy::Reject && parent_depth != 0 {
            return Err(MinibufferEntryRejection::RecursiveDisabled);
        }
        Ok(MinibufferEntryPermit {
            parent_depth,
            depth,
        })
    }

    pub(crate) fn enter_with_permit(
        &mut self,
        permit: MinibufferEntryPermit,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&LispString>,
        history_name: Option<SymId>,
    ) -> &mut MinibufferState {
        assert_eq!(
            self.state_stack.len(),
            permit.parent_depth,
            "minibuffer stack changed after entry was admitted"
        );
        let initial_string = initial
            .cloned()
            .unwrap_or_else(|| LispString::from_utf8(""));
        let mut state = MinibufferState::new(
            buffer_id,
            prompt.clone(),
            initial_string,
            permit.depth.get(),
        );

        if let Some(name) = history_name {
            state.history = self.history.get(name).to_vec();
        }

        self.state_stack.push(state);
        self.state_stack
            .last_mut()
            .expect("admitted minibuffer state was just pushed")
    }

    /// Enter the minibuffer with the given prompt and optional initial input / history name.
    ///
    /// Returns a fresh `MinibufferState` that has been pushed onto the stack.
    /// The caller can further configure it (completion table, require-match, default).
    pub(crate) fn read_from_minibuffer_lisp(
        &mut self,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&LispString>,
        history_name: Option<SymId>,
    ) -> Result<&mut MinibufferState, Flow> {
        let permit = self
            .prepare_entry(RecursiveMinibufferPolicy::Allow)
            .map_err(MinibufferEntryRejection::into_flow)?;
        Ok(self.enter_with_permit(permit, buffer_id, prompt, initial, history_name))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn read_from_minibuffer(
        &mut self,
        buffer_id: BufferId,
        prompt: &str,
        initial: Option<&str>,
        history_name: Option<SymId>,
    ) -> Result<&mut MinibufferState, Flow> {
        let prompt = LispString::from_utf8(prompt);
        let initial = initial.map(LispString::from_utf8);
        self.read_from_minibuffer_lisp(buffer_id, &prompt, initial.as_ref(), history_name)
    }

    /// Attempt to complete the current minibuffer content.
    pub fn try_complete(&self, state: &MinibufferState) -> CompletionResult {
        match &state.completion_table {
            Some(table) => {
                let matches = self.all_completions(&state.content, table);
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
    pub fn all_completions(&self, prefix: &LispString, table: &CompletionTable) -> Vec<LispString> {
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
    pub fn try_completion_string(
        &self,
        prefix: &LispString,
        table: &CompletionTable,
    ) -> Option<LispString> {
        let matches = self.all_completions(prefix, table);
        compute_common_prefix(&matches)
    }

    /// Test whether `string` is an exact match in `table`.
    pub fn test_completion(&self, string: &LispString, table: &CompletionTable) -> bool {
        let candidates = table.candidates(string);
        candidates.iter().any(|candidate| candidate == string)
    }

    /// Exit the current minibuffer, returning its content (or the default if empty).
    pub fn exit_minibuffer(&mut self) -> Option<LispString> {
        if let Some(mut state) = self.state_stack.pop() {
            state.active = false;
            let result = if state.content.is_empty() {
                state
                    .default_value
                    .clone()
                    .unwrap_or_else(|| LispString::from_utf8(""))
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
    pub fn history_previous(&mut self) -> Option<LispString> {
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
        state.cursor_pos = state.content.byte_len();
        Some(entry)
    }

    /// Navigate to the next (newer) history entry.
    pub fn history_next(&mut self) -> Option<LispString> {
        let state = self.state_stack.last_mut()?;
        match state.history_position {
            None => None,
            Some(0) => {
                // Back to the original input.
                state.history_position = None;
                state.content = state.initial_input.clone();
                state.cursor_pos = state.content.byte_len();
                Some(state.content.clone())
            }
            Some(p) => {
                let new_pos = p - 1;
                state.history_position = Some(new_pos);
                state.content = state.history[new_pos].clone();
                state.cursor_pos = state.content.byte_len();
                Some(state.content.clone())
            }
        }
    }

    /// Add a value to a named history list.
    ///
    /// `max_length` controls how many entries to keep.  Callers should read
    /// the `history-length` symbol from the obarray (default 100).
    pub fn add_to_history(&mut self, name: SymId, value: &str, max_length: usize) {
        self.add_to_history_lisp(name, LispString::from_utf8(value), max_length);
    }

    pub fn add_to_history_lisp(&mut self, name: SymId, value: LispString, max_length: usize) {
        self.history.add(name, value, max_length);
    }

    /// Read the effective `history-length` from the obarray, defaulting to 100.
    pub fn history_length_from_obarray(obarray: &Obarray) -> usize {
        match obarray.symbol_value("history-length") {
            Some(v) if v.is_fixnum() && v.xfixnum() > 0 => v.xfixnum() as usize,
            _ => 100,
        }
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
        self.state_stack.last().is_some_and(|s| s.active)
    }

    pub fn has_buffer(&self, buffer_id: BufferId) -> bool {
        self.state_stack
            .iter()
            .any(|state| state.buffer_id == buffer_id)
    }

    /// Set the completion style.
    pub fn set_completion_style(&mut self, style: CompletionStyle) {
        self.completion_style = style;
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
fn prefix_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_prefix(input, candidate, true))
        .cloned()
        .collect()
}

/// Substring matching (case-insensitive).
fn substring_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_substring(input, candidate, true))
        .cloned()
        .collect()
}

/// Flex (fuzzy) matching: the input characters must appear in order within the candidate.
fn flex_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| is_flex_match(input, candidate))
        .cloned()
        .collect()
}

/// Check if all characters in `input` appear in order in `candidate` (case-insensitive).
fn is_flex_match(input: &LispString, candidate: &LispString) -> bool {
    let input_codes = completion_char_codes(input);
    let candidate_codes = completion_char_codes(candidate);
    let mut cursor = 0usize;
    for code in input_codes {
        let folded = completion_fold_char(code, true);
        while cursor < candidate_codes.len()
            && completion_fold_char(candidate_codes[cursor], true) != folded
        {
            cursor += 1;
        }
        if cursor == candidate_codes.len() {
            return false;
        }
        cursor += 1;
    }
    true
}

/// Exact (case-sensitive) prefix matching.
fn basic_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_prefix(input, candidate, false))
        .cloned()
        .collect()
}

fn lisp_string_prefix_chars(string: &LispString, chars: usize) -> LispString {
    let end_byte = if string.is_multibyte() {
        crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), chars)
    } else {
        chars.min(string.byte_len())
    };
    string
        .slice(0, end_byte)
        .unwrap_or_else(|| LispString::from_utf8(""))
}

fn lisp_string_matches_prefix(
    input: &LispString,
    candidate: &LispString,
    ignore_case: bool,
) -> bool {
    let input_codes = completion_char_codes(input);
    let candidate_codes = completion_char_codes(candidate);
    if input_codes.len() > candidate_codes.len() {
        return false;
    }
    input_codes
        .iter()
        .zip(candidate_codes.iter())
        .all(|(left, right)| {
            completion_fold_char(*left, ignore_case) == completion_fold_char(*right, ignore_case)
        })
}

fn lisp_string_matches_substring(
    input: &LispString,
    candidate: &LispString,
    ignore_case: bool,
) -> bool {
    let needle = completion_char_codes(input);
    let haystack = completion_char_codes(candidate);
    if needle.is_empty() {
        return true;
    }
    if needle.len() > haystack.len() {
        return false;
    }
    haystack.windows(needle.len()).any(|window| {
        window.iter().zip(needle.iter()).all(|(left, right)| {
            completion_fold_char(*left, ignore_case) == completion_fold_char(*right, ignore_case)
        })
    })
}

/// Compute the longest common prefix of a set of strings.
/// Returns `None` if the set is empty.
fn compute_common_prefix(strings: &[LispString]) -> Option<LispString> {
    if strings.is_empty() {
        return None;
    }
    let first = &strings[0];
    let mut prefix = completion_char_codes(first);
    for string in &strings[1..] {
        let other = completion_char_codes(string);
        let max = prefix.len().min(other.len());
        let mut common = 0;
        while common < max && prefix[common] == other[common] {
            common += 1;
        }
        prefix.truncate(common);
        if prefix.is_empty() {
            return Some(LispString::from_utf8(""));
        }
    }
    Some(lisp_string_prefix_chars(first, prefix.len()))
}

// ---------------------------------------------------------------------------
// Builtin functions for Elisp
// ---------------------------------------------------------------------------

/// `(read-file-name PROMPT &optional DIR DEFAULT MUSTMATCH INITIAL PREDICATE)`
///
/// Read a file name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer with initial directory context.
pub(crate) fn builtin_read_file_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_file_name_in_runtime(eval, &args)?;
    finish_read_file_name_in_eval(eval, &args)
}

/// `(read-directory-name PROMPT &optional DIR DEFAULT MUSTMATCH INITIAL)`
///
/// Read a directory name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer with initial directory context.
pub(crate) fn builtin_read_directory_name(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_directory_name_in_runtime(eval, &args)?;
    finish_read_directory_name_in_eval(eval, &args)
}

fn validate_file_name_reader_args(name: &str, args: &[Value], max: usize) -> Result<(), Flow> {
    expect_min_args(name, args, 1)?;
    expect_max_args(name, args, max)?;
    let _prompt = expect_lisp_string(&args[0])?;
    if let Some(dir) = args.get(1)
        && !dir.is_nil()
    {
        let _ = expect_lisp_string(dir)?;
    }
    if let Some(default) = args.get(2)
        && !default.is_nil()
    {
        let _ = expect_lisp_string(default)?;
    }
    if let Some(initial) = args.get(4)
        && !initial.is_nil()
    {
        let _ = expect_lisp_string(initial)?;
    }
    Ok(())
}

fn file_name_reader_minibuffer_args(args: &[Value]) -> [Value; 6] {
    let prompt = args[0];
    let initial = args.get(4).copied().unwrap_or(Value::NIL);
    let default = args.get(2).copied().unwrap_or(Value::NIL);
    let effective_initial = if initial.is_nil() {
        args.get(1).copied().unwrap_or(Value::NIL)
    } else {
        initial
    };
    [
        prompt,
        effective_initial,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        default,
    ]
}

pub(crate) fn builtin_read_file_name_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    validate_file_name_reader_args("read-file-name", args, 6)?;
    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(()),
        MinibufferInputSource::StandardInput => Err(end_of_file_stdin_error()),
    }
}

pub(crate) fn finish_read_file_name_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let minibuffer_args = file_name_reader_minibuffer_args(args);
    read_from_minibuffer(&minibuffer_args)
}

pub(crate) fn finish_read_file_name_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_file_name_with_minibuffer(args, |minibuffer_args| {
        super::reader::finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    })
}

pub(crate) fn finish_read_file_name_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    builtin_read_file_name_in_runtime(shared, args)?;
    finish_read_file_name_with_minibuffer(args, |minibuffer_args| {
        super::reader::finish_read_from_minibuffer_in_vm_runtime(shared, minibuffer_args)
    })
}

pub(crate) fn builtin_read_directory_name_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    validate_file_name_reader_args("read-directory-name", args, 5)?;
    match runtime.minibuffer_input_source() {
        MinibufferInputSource::CommandLoop => Ok(()),
        MinibufferInputSource::StandardInput => Err(end_of_file_stdin_error()),
    }
}

pub(crate) fn finish_read_directory_name_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_file_name_with_minibuffer(args, |minibuffer_args| {
        super::reader::finish_read_from_minibuffer_in_eval(eval, minibuffer_args)
    })
}

pub(crate) fn finish_read_directory_name_in_vm_runtime(
    shared: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    builtin_read_directory_name_in_runtime(shared, args)?;
    finish_read_file_name_with_minibuffer(args, |minibuffer_args| {
        super::reader::finish_read_from_minibuffer_in_vm_runtime(shared, minibuffer_args)
    })
}

/// `(read-buffer PROMPT &optional DEFAULT REQUIRE-MATCH PREDICATE)`
///
/// Read a buffer name from the minibuffer with completion.
/// In interactive mode, delegates to completing-read with buffer name candidates.
pub(crate) fn builtin_read_buffer(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    execute_read_buffer_plan(eval, &args, |eval, completing_args| {
        super::reader::builtin_completing_read(eval, completing_args.to_vec())
    })
}

/// The two GNU `Fread_buffer` dispatch paths.
///
/// Keeping this choice explicit prevents callers such as interactive control
/// letters from bypassing `read-buffer-function` by constructing completion
/// arguments directly.
#[derive(Clone, Debug)]
pub(crate) enum ReadBufferPlan {
    Override {
        function: Value,
        arguments: Vec<Value>,
    },
    Complete {
        arguments: [Value; 7],
    },
}

fn plan_read_buffer(eval: &mut super::eval::Context, args: &[Value]) -> ReadBufferPlan {
    let default = normalize_buffer_reader_default(
        eval.buffer_manager(),
        args.get(1).copied().unwrap_or(Value::NIL),
    );
    let function = eval.visible_variable_value_or_nil("read-buffer-function");
    if !function.is_nil() {
        let require_match = args.get(2).copied().unwrap_or(Value::NIL);
        let predicate = args.get(3).copied().unwrap_or(Value::NIL);
        let mut arguments = vec![args[0], default, require_match];
        // GNU keeps backward compatibility with older three-argument reader
        // functions when no predicate was supplied.
        if !predicate.is_nil() {
            arguments.push(predicate);
        }
        ReadBufferPlan::Override {
            function,
            arguments,
        }
    } else {
        ReadBufferPlan::Complete {
            arguments: read_buffer_completing_args(eval.obarray(), eval.buffer_manager(), args),
        }
    }
}

fn execute_read_buffer_plan(
    eval: &mut super::eval::Context,
    args: &[Value],
    complete: impl FnOnce(&mut super::eval::Context, &[Value]) -> EvalResult,
) -> EvalResult {
    builtin_read_buffer_in_runtime(eval, args)?;

    // GNU binds this around both the override and completion paths.
    let specpdl_count = eval.specpdl.len();
    let ignore_case = eval.visible_variable_value_or_nil("read-buffer-completion-ignore-case");
    eval.try_specbind_or_unwind_to(
        specpdl_count,
        CompletionStateVariable::CompletionIgnoreCase.symbol_id(),
        ignore_case,
    )?;

    let result = match plan_read_buffer(eval, args) {
        ReadBufferPlan::Override {
            function,
            arguments,
        } => eval.funcall_general(function, arguments),
        ReadBufferPlan::Complete { arguments } => complete(eval, &arguments),
    };
    eval.unbind_to_with_result(specpdl_count, result)
}

pub(crate) fn finish_read_buffer_in_vm_runtime(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    execute_read_buffer_plan(eval, args, |eval, completing_args| {
        super::reader::finish_completing_read_in_vm_runtime(eval, completing_args)
    })
}

pub(crate) fn builtin_read_buffer_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only; batch prompt/stdin handling lives in the shared
    // `completing-read` path reached from `finish_read_buffer_*`.
    let _ = runtime;
    expect_min_args("read-buffer", args, 1)?;
    expect_max_args("read-buffer", args, 4)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

pub(crate) fn read_buffer_completing_args(
    obarray: &Obarray,
    buffers: &BufferManager,
    args: &[Value],
) -> [Value; 7] {
    let default =
        normalize_buffer_reader_default(buffers, args.get(1).copied().unwrap_or(Value::NIL));
    let prompt = read_buffer_prompt(obarray, args[0], default);
    let require_match = args.get(2).copied().unwrap_or(Value::NIL);
    let predicate = args.get(3).copied().unwrap_or(Value::NIL);

    let buf_ids = buffers.buffer_list();
    let buffer_names: Vec<Value> = buf_ids
        .iter()
        .filter_map(|id| buffers.get(*id))
        .map(|b| b.name_value())
        .collect();
    let collection = Value::list(buffer_names);

    [
        prompt,
        collection,
        predicate,
        require_match,
        Value::NIL,
        Value::NIL,
        default,
    ]
}

/// `(read-command PROMPT &optional DEFAULT)`
///
/// Read a command name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer and interns the result.
pub(crate) fn builtin_read_command(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_command_in_runtime(eval, &args)?;
    finish_read_command_in_eval(eval, &args)
}

pub(crate) fn finish_read_command_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_command_with_minibuffer(args, |minibuffer_args| {
        super::reader::builtin_read_from_minibuffer(eval, minibuffer_args.to_vec())
    })
}

pub(crate) fn builtin_read_command_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only.  GNU's `Fread_command` routes through `Fcompleting_read`
    // -> `read_minibuf` -> `read_minibuf_noninteractive` in batch mode, which
    // writes the prompt to stdout before reading stdin; the actual batch
    // prompt/stdin handling happens in the shared `read-from-minibuffer` path
    // reached from `finish_read_command_*`.
    let _ = runtime;
    expect_min_args("read-command", args, 1)?;
    expect_max_args("read-command", args, 2)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

fn symbol_reader_minibuffer_args(args: &[Value]) -> [Value; 6] {
    let prompt = args[0];
    let default = normalize_symbol_reader_default(args.get(1).copied().unwrap_or(Value::NIL));
    [
        prompt,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        Value::NIL,
        default,
    ]
}

fn intern_symbol_reader_result(result: Value) -> Value {
    if let Some(name) = result.as_lisp_string() {
        // read-symbol results are interned symbol names (ASCII / decoded text);
        // decode lossily and reuse Value::symbol's nil/t/keyword canonicalization.
        return Value::symbol(crate::emacs_core::emacs_char::to_utf8_lossy(
            name.as_bytes(),
        ));
    }
    result
}

fn finish_symbol_reader_with_minibuffer(
    args: &[Value],
    mut read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    let minibuffer_args = symbol_reader_minibuffer_args(args);
    let result = read_from_minibuffer(&minibuffer_args)?;
    Ok(intern_symbol_reader_result(result))
}

pub(crate) fn finish_read_command_with_minibuffer(
    args: &[Value],
    read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    finish_symbol_reader_with_minibuffer(args, read_from_minibuffer)
}

/// `(read-variable PROMPT &optional DEFAULT)`
///
/// Read a variable name from the minibuffer.
/// In interactive mode, uses read-from-minibuffer and interns the result.
pub(crate) fn builtin_read_variable(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    builtin_read_variable_in_runtime(eval, &args)?;
    finish_read_variable_in_eval(eval, &args)
}

pub(crate) fn finish_read_variable_in_eval(
    eval: &mut super::eval::Context,
    args: &[Value],
) -> EvalResult {
    finish_read_variable_with_minibuffer(args, |minibuffer_args| {
        super::reader::builtin_read_from_minibuffer(eval, minibuffer_args.to_vec())
    })
}

pub(crate) fn builtin_read_variable_in_runtime(
    runtime: &impl KeyboardInputRuntime,
    args: &[Value],
) -> Result<(), Flow> {
    // Validation only; batch prompt/stdin handling lives in the shared
    // `read-from-minibuffer` path reached from `finish_read_variable_*`.
    let _ = runtime;
    expect_min_args("read-variable", args, 1)?;
    expect_max_args("read-variable", args, 2)?;
    let _prompt = expect_lisp_string(&args[0])?;
    Ok(())
}

pub(crate) fn finish_read_variable_with_minibuffer(
    args: &[Value],
    read_from_minibuffer: impl FnMut(&[Value]) -> EvalResult,
) -> EvalResult {
    finish_symbol_reader_with_minibuffer(args, read_from_minibuffer)
}

/// `(minibuffer-prompt)` — returns the current minibuffer prompt or nil.
///
/// Stub: returns nil (no active minibuffer in non-interactive mode).
pub(crate) fn builtin_minibuffer_prompt_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-prompt", &args, 0)?;
    Ok(eval
        .minibuffers
        .current()
        .map(|state| Value::heap_string(state.prompt.clone()))
        .unwrap_or(Value::NIL))
}

pub(crate) fn builtin_minibuffer_prompt_end_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-prompt-end", &args, 0)?;

    Ok(Value::fixnum(
        minibuffer_prompt_end_in_state(&eval.obarray, &eval.buffers, &eval.minibuffers)?.as_i64(),
    ))
}

fn minibuffer_prompt_end_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    minibuffers: &MinibufferManager,
) -> Result<LispCharPos1, Flow> {
    let Some((current_id, point_min, point_max)) = buffers.current_buffer().map(|buffer| {
        (
            buffer.id,
            buffer.point_min_lisp_char_pos().as_i64(),
            buffer.point_max_lisp_char_pos().as_i64(),
        )
    }) else {
        return Ok(LispCharPos1::new(1));
    };

    let Some(state) = minibuffers.current() else {
        return Ok(LispCharPos1::new(point_min));
    };
    if state.buffer_id != current_id {
        return Ok(LispCharPos1::new(point_min));
    }

    let (_, prompt_end_pos) = super::buffer::find_field_bounds_in_state(
        obarray,
        &[],
        buffers,
        Some(&Value::fixnum(point_min)),
        false,
        None,
        None,
    )?;

    // GNU `Fminibuffer_prompt_end` falls back to point-min when the active
    // minibuffer has no prompt field at BEGV, even if `field-end` reaches ZV.
    if prompt_end_pos == point_max {
        let buffer = buffers
            .get(current_id)
            .expect("current minibuffer must remain available");
        let point_min_byte =
            buffer.lisp_pos_to_accessible_emacs_byte_pos(LispCharPos1::new(point_min));
        if buffer
            .text_props_get_property_at_emacs_byte_pos(point_min_byte, Value::symbol("field"))
            .is_none()
        {
            return Ok(LispCharPos1::new(point_min));
        }
    }

    Ok(LispCharPos1::new(prompt_end_pos))
}

/// `(minibuffer-contents)` — returns the current minibuffer contents.
///
/// In non-interactive batch mode, Emacs exposes current buffer contents.
pub(crate) fn builtin_minibuffer_contents_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-contents", &args, 0)?;
    Ok(Value::heap_string(minibuffer_contents_lisp_string(
        eval, true,
    )?))
}

/// `(minibuffer-contents-no-properties)` — returns minibuffer contents
/// without text properties.
pub(crate) fn builtin_minibuffer_contents_no_properties_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-contents-no-properties", &args, 0)?;
    Ok(Value::heap_string(minibuffer_contents_lisp_string(
        eval, false,
    )?))
}

/// `(minibuffer-depth)` — returns the current recursive minibuffer depth.
///
/// Stub: returns 0.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_minibuffer_depth(args: Vec<Value>) -> EvalResult {
    expect_args("minibuffer-depth", &args, 0)?;
    Ok(Value::fixnum(0))
}

pub(crate) fn builtin_minibuffer_depth_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("minibuffer-depth", &args, 0)?;
    Ok(Value::fixnum(eval.minibuffers.depth() as i64))
}

/// `(minibufferp &optional BUFFER)` — returns t if BUFFER is a minibuffer.
///
/// Batch-compatible behavior: accepts 0..=2 args, validates BUFFER-like first
/// arg shape, and returns nil (no active minibuffer).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_minibufferp(args: Vec<Value>) -> EvalResult {
    validate_minibufferp_args(&args)?;
    Ok(Value::NIL)
}

pub(crate) fn builtin_minibufferp_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    validate_minibufferp_args(&args)?;
    let live_only = args.get(1).is_some_and(|v| v.is_truthy());
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    let is_live = eval.minibuffers.has_buffer(buffer_id);
    let is_minibuffer = is_live
        || eval
            .buffers
            .get(buffer_id)
            .is_some_and(|buffer| is_minibuffer_buffer_name(&buffer.name_runtime_string_owned()));
    Ok(Value::bool_val(if live_only {
        is_live
    } else {
        is_minibuffer
    }))
}

pub(crate) fn builtin_minibuffer_innermost_command_loop_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("minibuffer-innermost-command-loop-p", &args, 0, 1)?;
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    let recursive_depth = eval.recursive_command_loop_depth();
    let command_loop_depth = eval
        .minibuffers
        .state_stack
        .iter()
        .find(|state| state.buffer_id == buffer_id)
        .map(|state| state.command_loop_depth);
    Ok(Value::bool_val(
        command_loop_depth.is_some_and(|depth| depth == recursive_depth),
    ))
}

pub(crate) fn builtin_innermost_minibuffer_p_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("innermost-minibuffer-p", &args, 0, 1)?;
    let Some(buffer_id) = resolve_minibuffer_buffer_arg(&eval.buffers, args.first())? else {
        return Ok(Value::NIL);
    };
    Ok(Value::bool_val(
        eval.minibuffers
            .current()
            .is_some_and(|state| state.buffer_id == buffer_id),
    ))
}

fn validate_minibufferp_args(args: &[Value]) -> Result<(), Flow> {
    if args.len() > 2 {
        return Err(signal(
            LispCondition::WrongNumberOfArguments,
            vec![
                Value::symbol("minibufferp"),
                Value::fixnum(args.len() as i64),
            ],
        ));
    }
    if let Some(bufferish) = args.first() {
        match bufferish.kind() {
            ValueKind::Nil | ValueKind::String | ValueKind::Veclike(VecLikeType::Buffer) => {}
            _ => {
                return Err(signal(
                    LispCondition::WrongTypeArgument,
                    vec![Value::symbol("bufferp"), *bufferish],
                ));
            }
        }
    }
    Ok(())
}

/// Eval-aware `(recursive-edit)` — enters the command loop.
///
/// Mirrors GNU Emacs keyboard.c:772 `Frecursive_edit`.
pub(crate) fn builtin_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("recursive-edit", &args, 0)?;
    let restore = RecursiveEditBuffer::record(eval);
    let result = eval.recursive_edit_inner();
    restore.unwind(eval);
    result
}

/// The buffer `recursive-edit` has to put back when its command loop unwinds.
///
/// GNU `Frecursive_edit` (src/keyboard.c:811-816) records the current buffer
/// only when it is NOT the selected window's buffer, and
/// `recursive_edit_unwind` (src/keyboard.c:837-844) makes it current again --
/// on every exit, including the `(throw 'exit ...)` that
/// `exit-recursive-edit' uses.  Without it a `recursive-edit' entered inside
/// `with-temp-buffer' returns with the window's buffer current, so everything
/// the caller then does to "its" buffer silently lands elsewhere.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RecursiveEditBuffer {
    /// The current buffer is already the selected window's, so the command
    /// loop leaves it current by itself and GNU records nothing.
    SelectedWindows,
    /// A buffer only this Lisp frame knows about; make it current on unwind.
    Restore(crate::buffer::BufferId),
}

impl RecursiveEditBuffer {
    fn record(eval: &mut super::eval::Context) -> Self {
        let Some(current) = eval.buffers.current_buffer_id() else {
            return Self::SelectedWindows;
        };
        let window_buffer = super::window_cmds::builtin_window_buffer(eval, Vec::new())
            .ok()
            .and_then(|value| value.as_buffer_id());
        if window_buffer == Some(current) {
            Self::SelectedWindows
        } else {
            Self::Restore(current)
        }
    }

    fn unwind(self, eval: &mut super::eval::Context) {
        if let Self::Restore(buffer_id) = self
            && eval.buffers.get(buffer_id).is_some()
        {
            let _ = eval.buffers.switch_current(buffer_id);
        }
    }
}

/// `(top-level)` — exit all recursive edits.
///
/// Mirrors GNU Emacs keyboard.c:1187 `Ftop_level`.
/// Throws to the `top-level` tag to unwind all recursive edits.
pub(crate) fn builtin_top_level(args: Vec<Value>) -> EvalResult {
    expect_args("top-level", &args, 0)?;
    Err(Flow::throw(Value::symbol("top-level"), Value::NIL))
}

/// `(exit-recursive-edit)` — exit innermost recursive edit.
///
/// Mirrors GNU Emacs keyboard.c:1211 `Fexit_recursive_edit`.
/// Throws to the `exit` tag with nil value.
pub(crate) fn builtin_exit_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("exit-recursive-edit", &args, 0)?;
    // GNU Emacs checks: command_loop_level > 0 || minibuf_level > 0
    if eval.recursive_command_loop_depth() == 0 && eval.minibuffers.depth() == 0 {
        return Err(signal(
            LispCondition::UserError,
            vec![Value::string("No recursive edit is in progress")],
        ));
    }
    Err(Flow::throw(Value::symbol("exit"), Value::NIL))
}

/// `(exit-minibuffer)` — exit the active minibuffer.
///
/// Emacs exits by throwing to the `exit` tag; without a catch this
/// surfaces as `no-catch`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_exit_minibuffer(args: Vec<Value>) -> EvalResult {
    expect_args("exit-minibuffer", &args, 0)?;
    Err(Flow::throw(Value::symbol("exit"), Value::NIL))
}

/// `(abort-minibuffers)` — abort active minibuffer sessions.
///
/// Batch/non-interactive mode has no active minibuffer, so this matches GNU
/// Emacs by signaling a plain `error`.
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn builtin_abort_minibuffers(args: Vec<Value>) -> EvalResult {
    expect_args("abort-minibuffers", &args, 0)?;
    Err(signal("error", vec![Value::string("Not in a minibuffer")]))
}

/// GNU minibuf.c `this_minibuffer_depth`: the 1-based minibuffer level of the
/// current buffer, or 0 when it is not one of the active minibuffers.
fn this_minibuffer_depth(eval: &super::eval::Context) -> usize {
    let Some(buffer_id) = eval.buffers.current_buffer_id() else {
        return 0;
    };
    eval.minibuffers
        .state_stack
        .iter()
        .position(|state| state.buffer_id == buffer_id)
        .map_or(0, |index| index + 1)
}

/// Mirrors GNU minibuf.c `Fabort_minibuffers`.
///
/// It deliberately does not throw to `exit` itself: that is
/// `abort-recursive-edit`, whose `t` means a plain `quit`.  Aborting a
/// minibuffer instead delegates to `minibuffer-quit-recursive-edit`, which
/// throws a thunk signaling the distinguishable `minibuffer-quit`.
pub(crate) fn builtin_abort_minibuffers_ctx(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abort-minibuffers", &args, 0)?;
    let minibuf_depth = this_minibuffer_depth(eval);
    if minibuf_depth == 0 {
        return Err(signal("error", vec![Value::string("Not in a minibuffer")]));
    }
    if builtin_minibuffer_innermost_command_loop_p_ctx(eval, vec![])?.is_nil() {
        return Err(signal(
            "error",
            vec![Value::string("Not in most nested command loop")],
        ));
    }

    let minibuf_level = eval.minibuffers.depth();
    let quit_recursive_edit = Value::symbol("minibuffer-quit-recursive-edit");
    if minibuf_depth < minibuf_level {
        // Aborting this minibuffer also aborts every one nested inside it, so
        // GNU confirms first and then quits that many recursive edits at once.
        let levels = minibuf_level - minibuf_depth + 1;
        let prompt = Value::string(&format!("Abort {levels} minibuffer levels? "));
        if eval
            .apply(Value::symbol("yes-or-no-p"), vec![prompt])?
            .is_truthy()
        {
            eval.apply(quit_recursive_edit, vec![Value::fixnum(levels as i64)])?;
        }
    } else {
        eval.apply(quit_recursive_edit, vec![])?;
    }
    Ok(Value::NIL)
}

pub(crate) fn minibuffer_contents_lisp_string_in_state(
    obarray: &Obarray,
    buffers: &BufferManager,
    minibuffers: &MinibufferManager,
    preserve_properties: bool,
) -> Result<LispString, Flow> {
    let Some((point_max, current_id)) = buffers
        .current_buffer()
        .map(|buffer| (buffer.accessible_emacs_byte_region().end(), buffer.id))
    else {
        return Ok(LispString::from_utf8(""));
    };
    let prompt_end = minibuffer_prompt_end_in_state(obarray, buffers, minibuffers)?;
    let buffer = buffers
        .get(current_id)
        .expect("current buffer must remain available");
    let start = buffer.lisp_pos_to_accessible_emacs_byte_pos(prompt_end);
    let range = EmacsByteRange::new(start, point_max);
    Ok(if preserve_properties {
        buffer.buffer_substring_lisp_string_range(range)
    } else {
        buffer.buffer_substring_lisp_string_no_properties_range(range)
    })
}

fn minibuffer_contents_lisp_string(
    eval: &mut super::eval::Context,
    preserve_properties: bool,
) -> Result<LispString, Flow> {
    minibuffer_contents_lisp_string_in_state(
        &eval.obarray,
        &eval.buffers,
        &eval.minibuffers,
        preserve_properties,
    )
}

fn resolve_minibuffer_buffer_arg(
    buffers: &BufferManager,
    bufferish: Option<&Value>,
) -> Result<Option<BufferId>, Flow> {
    let Some(val) = bufferish else {
        return Ok(buffers.current_buffer_id());
    };
    match val.kind() {
        ValueKind::Nil => Ok(buffers.current_buffer_id()),
        ValueKind::Veclike(VecLikeType::Buffer) => Ok(val.as_buffer_id()),
        ValueKind::String => Ok(val
            .as_utf8_str()
            .and_then(|name| buffers.find_buffer_by_name(name))),
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("bufferp"), *val],
        )),
    }
}

fn is_minibuffer_buffer_name(name: &str) -> bool {
    name.starts_with(" *Minibuf-") && name.ends_with('*')
}

/// `(abort-recursive-edit)` — abort the innermost recursive edit.
///
/// Mirrors GNU Emacs keyboard.c:1222 `Fabort_recursive_edit`.
/// Throws to the `exit` tag with `t` value (signals quit on catch).
pub(crate) fn builtin_abort_recursive_edit(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("abort-recursive-edit", &args, 0)?;
    // GNU Emacs checks: command_loop_level > 0 || minibuf_level > 0
    if eval.recursive_command_loop_depth() == 0 && eval.minibuffers.depth() == 0 {
        return Err(signal(
            LispCondition::UserError,
            vec![Value::string("No recursive edit is in progress")],
        ));
    }
    Err(Flow::throw(Value::symbol("exit"), Value::T))
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
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn value_to_string_list(val: &Value) -> Vec<String> {
    match val.kind() {
        ValueKind::Nil => Vec::new(),
        ValueKind::Cons => {
            let items = match super::value::list_to_vec(val) {
                Some(v) => v,
                None => return Vec::new(),
            };
            items
                .iter()
                .filter_map(|item| match item.kind() {
                    ValueKind::String => completion_display_string_from_value(item),
                    ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
                    // Alist entry: (STRING . _)
                    ValueKind::Cons => {
                        let pair_car = item.cons_car();
                        completion_display_string_from_value(&pair_car)
                    }
                    _ => None,
                })
                .collect()
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let vec = val.as_vector_data().unwrap().clone();
            vec.iter()
                .filter_map(|item| match item.kind() {
                    ValueKind::String => completion_display_string_from_value(item),
                    ValueKind::Symbol(id) => Some(resolve_sym(id).to_owned()),
                    _ => None,
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

#[derive(Clone)]
pub(crate) struct CompletionCandidate {
    completion: CompletionText,
    predicate_arg: Value,
    predicate_extra_arg: Option<Value>,
}

#[derive(Clone)]
enum CompletionText {
    /// An exact GC-owned Lisp string. This covers literal collection entries
    /// and Lisp-created symbol names; both must preserve object identity,
    /// mutation, text properties, and multibyteness like GNU.
    LispObject(Value),
    /// Process-lifetime atom for a symbol whose name has not yet been
    /// materialized on this heap.  Kept with its typed symbol identity so
    /// filtering stays allocation-free and a surviving result can acquire the
    /// symbol's one cached Lisp name object.
    Atom {
        symbol: SymId,
        string: &'static crate::heap_types::LispString,
    },
}

impl CompletionText {
    fn lisp_string(&self) -> &crate::heap_types::LispString {
        match self {
            Self::LispObject(value) => value
                .as_lisp_string()
                .expect("a rooted completion string remains a string"),
            Self::Atom { string, .. } => string,
        }
    }

    fn as_result_value(&self) -> Value {
        match self {
            Self::LispObject(value) => *value,
            Self::Atom { symbol, .. } => {
                crate::emacs_core::intern::materialize_symbol_name_value(*symbol)
            }
        }
    }

    fn substring_value(&self, end_chars: usize) -> Value {
        let string = self.lisp_string();
        if let Self::LispObject(value) = self
            && end_chars >= string.schars()
        {
            return *value;
        }

        let end_chars = end_chars.min(string.schars());
        let end_byte = if string.is_multibyte() {
            crate::emacs_core::emacs_char::char_to_byte_pos(string.as_bytes(), end_chars)
        } else {
            end_chars.min(string.byte_len())
        };
        let sliced = string
            .slice(0, end_byte)
            .expect("validated completion prefix slice");
        Value::heap_string(sliced)
    }

    fn searched_string(&self) -> super::regex::SearchedString {
        match self {
            Self::LispObject(value) => super::regex::SearchedString::Heap(*value),
            Self::Atom { string, .. } => super::regex::SearchedString::Owned((*string).clone()),
        }
    }
}

fn completion_text_from_value(value: &Value) -> Option<CompletionText> {
    match value.kind() {
        ValueKind::String => Some(CompletionText::LispObject(*value)),
        ValueKind::Symbol(id) => Some(completion_text_from_symbol_name(
            crate::emacs_core::intern::resolve_lisp_visible_symbol_name(id),
        )),
        ValueKind::Nil => Some(completion_text_from_symbol_name(
            crate::emacs_core::intern::resolve_lisp_visible_symbol_name(NIL_SYM_ID),
        )),
        ValueKind::T => Some(completion_text_from_symbol_name(
            crate::emacs_core::intern::resolve_lisp_visible_symbol_name(T_SYM_ID),
        )),
        _ => None,
    }
}

fn completion_text_from_symbol_name(
    name: crate::emacs_core::intern::LispVisibleSymbolName,
) -> CompletionText {
    match name {
        crate::emacs_core::intern::LispVisibleSymbolName::LispObject(value) => {
            CompletionText::LispObject(value)
        }
        crate::emacs_core::intern::LispVisibleSymbolName::Atom { symbol, string } => {
            CompletionText::Atom { symbol, string }
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
fn completion_display_string_from_value(value: &Value) -> Option<String> {
    let completion = completion_text_from_value(value)?;
    let string = completion.lisp_string();
    Some(
        string
            .as_utf8_str()
            .map(|text| text.to_owned())
            .unwrap_or_else(|| crate::emacs_core::emacs_char::to_utf8_lossy(string.as_bytes())),
    )
}

fn completion_candidates_from_list_value(collection: &Value) -> Vec<CompletionCandidate> {
    let items = match super::value::list_to_vec(collection) {
        Some(items) => items,
        None => return Vec::new(),
    };
    items
        .into_iter()
        .filter_map(|item| {
            let key = match item.kind() {
                ValueKind::Cons => item.cons_car(),
                _other => item,
            };
            completion_text_from_value(&key).map(|completion| CompletionCandidate {
                completion,
                predicate_arg: item,
                predicate_extra_arg: None,
            })
        })
        .collect()
}

fn completion_char_codes(string: &crate::heap_types::LispString) -> Vec<u32> {
    super::builtins::lisp_string_char_codes(string)
}

/// The prefix state shared by GNU's three scanning completion primitives.
///
/// Empty input is not merely a zero-length character buffer: for prefix
/// matching it accepts every string without inspecting that candidate at all.
/// Keeping that state as a distinct variant prevents the hot obarray scan from
/// constructing a decoder for every symbol only to discover that `all` over an
/// empty iterator is true.
#[derive(Clone, Debug, Eq, PartialEq)]
enum CompletionPrefix {
    Empty,
    Characters(Vec<u32>),
}

impl CompletionPrefix {
    fn from_lisp_string(string: &crate::heap_types::LispString) -> Self {
        let characters = completion_char_codes(string);
        if characters.is_empty() {
            Self::Empty
        } else {
            Self::Characters(characters)
        }
    }

    fn characters(&self) -> &[u32] {
        match self {
            Self::Empty => &[],
            Self::Characters(characters) => characters,
        }
    }

    fn matches(&self, completion: &CompletionText, ignore_case: bool) -> bool {
        match self {
            Self::Empty => true,
            Self::Characters(characters) => {
                completion_text_matches_nonempty_prefix(characters, completion, ignore_case)
            }
        }
    }
}

/// Lazy char-code iterator over a LispString, decoding exactly like
/// lisp_string_char_codes but without materializing a Vec. Prefix matching
/// over large candidate sets (a whole obarray) usually rejects on the first
/// character; allocating the full decode per candidate dominated bootstrap.
fn lisp_string_chars(
    string: &crate::heap_types::LispString,
) -> impl Iterator<Item = u32> + use<'_> {
    let bytes = string.as_bytes();
    let multibyte = string.is_multibyte();
    let mut pos = 0usize;
    std::iter::from_fn(move || {
        if pos >= bytes.len() {
            return None;
        }
        if !multibyte {
            let code = bytes[pos] as u32;
            pos += 1;
            return Some(code);
        }
        let byte = bytes[pos];
        if byte < 0x80 {
            pos += 1;
            return Some(byte as u32);
        }
        let (cp, len) = crate::emacs_core::emacs_char::string_char_unchecked(&bytes[pos..]);
        pos += len;
        Some(cp)
    })
}

fn completion_fold_char(code: u32, ignore_case: bool) -> u32 {
    if !ignore_case {
        return code;
    }
    crate::emacs_core::builtins::downcase_char_code_emacs_compat(code as i64) as u32
}

fn completion_text_matches_nonempty_prefix(
    prefix_codes: &[u32],
    completion: &CompletionText,
    ignore_case: bool,
) -> bool {
    debug_assert!(!prefix_codes.is_empty());
    let string = completion.lisp_string();
    if prefix_codes.len() > string.schars() {
        return false;
    }
    let mut chars = lisp_string_chars(string);
    prefix_codes.iter().all(|&left| match chars.next() {
        Some(right) => {
            completion_fold_char(left, ignore_case) == completion_fold_char(right, ignore_case)
        }
        None => false,
    })
}

fn completion_text_equals_string(
    completion: &CompletionText,
    string_codes: &[u32],
    ignore_case: bool,
) -> bool {
    let string = completion.lisp_string();
    if string.schars() != string_codes.len() {
        return false;
    }
    let mut chars = lisp_string_chars(string);
    string_codes.iter().all(|&right| match chars.next() {
        Some(left) => {
            completion_fold_char(left, ignore_case) == completion_fold_char(right, ignore_case)
        }
        None => false,
    })
}

/// GNU-faithful result of `Fcompare_strings` over the first `len` chars of two
/// char-code slices (each compared from offset 0).  Mirrors `fns.c`
/// `Fcompare_strings`: returns [`StringCompare::Equal`] when the compared
/// portions match, otherwise the (1-based) count of leading characters that
/// matched, tagged with which side was "less".  When `ignore_case` is set,
/// characters are upcased before comparison, exactly like GNU (which uses
/// `Fupcase`, not downcase).
#[derive(Clone, Copy, PartialEq, Eq)]
enum StringCompare {
    /// The two compared portions are equal.
    Equal,
    /// First string is "less"; `n` leading chars matched.
    Less(usize),
    /// First string is "greater"; `n` leading chars matched.
    Greater(usize),
}

impl StringCompare {
    /// GNU computes `matchsize` as `EQ (tem, Qt) ? compare : eabs (XFIXNUM (tem)) - 1`,
    /// where `compare` is the number of chars that were compared.  `eabs(N)-1`
    /// is the count of leading matching characters in both the less/greater
    /// cases (`-1 - N` for less, `N - 1` for greater both reduce to the leading
    /// match count).
    fn match_size(self, compare: usize) -> usize {
        match self {
            StringCompare::Equal => compare,
            StringCompare::Less(n) | StringCompare::Greater(n) => n,
        }
    }
}

fn upcase_code(code: u32) -> u32 {
    crate::emacs_core::builtins::upcase_char_code_emacs_compat(code as i64) as u32
}

/// Compare the first `len` characters of `a` and `b` (each starting at offset 0),
/// matching GNU `Fcompare_strings` semantics.  `len` is the number of chars to
/// compare; callers pass `min(SCHARS(a), SCHARS(b))` as in `Ftry_completion`.
fn gnu_compare_strings(a: &[u32], b: &[u32], len: usize, ignore_case: bool) -> StringCompare {
    let to1 = len.min(a.len());
    let to2 = len.min(b.len());
    let mut i = 0;
    while i < to1 && i < to2 {
        let mut c1 = a[i];
        let mut c2 = b[i];
        if c1 != c2 {
            if ignore_case {
                c1 = upcase_code(c1);
                c2 = upcase_code(c2);
            }
            if c1 != c2 {
                // GNU returns -(i+1) / (i+1) with `i` already advanced past the
                // mismatch; both encode `i` leading matching characters.
                return if c1 < c2 {
                    StringCompare::Less(i)
                } else {
                    StringCompare::Greater(i)
                };
            }
        }
        i += 1;
    }
    // One side ran out of compared range before the other.
    if i < to1 {
        StringCompare::Greater(i)
    } else if i < to2 {
        StringCompare::Less(i)
    } else {
        StringCompare::Equal
    }
}

fn is_global_obarray_proxy_in_state(obarray: &Obarray, value: &Value) -> bool {
    obarray
        .symbol_value_id_copied(CompletionStateVariable::Obarray.symbol_id())
        .is_some_and(|proxy| proxy == *value)
}

fn completion_candidates_from_global_obarray_in_state(
    obarray: &Obarray,
    lisp_obarray: Value,
) -> Vec<CompletionCandidate> {
    let ids =
        super::builtins::symbols::global_obarray_symbol_ids_in_bucket_order(obarray, lisp_obarray);
    crate::emacs_core::intern::map_lisp_visible_symbol_names(&ids, |id, name| CompletionCandidate {
        completion: completion_text_from_symbol_name(name),
        predicate_arg: Value::from_sym_id(id),
        predicate_extra_arg: None,
    })
}

pub(crate) fn completion_candidates_from_collection_in_state(
    ctx: &crate::emacs_core::eval::Context,
    collection: &Value,
) -> Result<Option<Vec<CompletionCandidate>>, Flow> {
    let obarray = &ctx.obarray;
    Ok(match collection.kind() {
        ValueKind::Nil | ValueKind::Cons => Some(completion_candidates_from_list_value(collection)),
        ValueKind::Veclike(VecLikeType::HashTable) => {
            Some(completion_candidates_from_hash_table(*collection))
        }
        ValueKind::Veclike(VecLikeType::Vector)
            if is_global_obarray_proxy_in_state(obarray, collection) =>
        {
            Some(completion_candidates_from_global_obarray_in_state(
                obarray,
                *collection,
            ))
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let obarray = super::builtins::symbols::check_obarray_value(*collection)?;
            Some(completion_candidates_from_custom_obarray(obarray))
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            Some(completion_candidates_from_custom_obarray(*collection))
        }
        _ => None,
    })
}

fn completion_candidates_from_collection(
    eval: &super::eval::Context,
    collection: &Value,
) -> Result<Option<Vec<CompletionCandidate>>, Flow> {
    completion_candidates_from_collection_in_state(eval, collection)
}

fn ordinary_completion_predicate_matches_with(
    predicate: Value,
    candidate: &CompletionCandidate,
    mut apply: impl FnMut(Value, Vec<Value>) -> EvalResult,
) -> Result<bool, Flow> {
    if predicate.is_nil() {
        return Ok(true);
    }
    let result = match candidate.predicate_extra_arg {
        Some(extra) => apply(predicate, vec![candidate.predicate_arg, extra])?,
        None => apply(predicate, vec![candidate.predicate_arg])?,
    };
    Ok(result.is_truthy())
}

/// Predicate dispatch used only by GNU's scanning completion primitives,
/// `try-completion` and `all-completions`.
///
/// GNU `minibuf.c` recognizes the canonical `Qcommandp` identity and calls
/// `Fcommandp` directly for every obarray candidate.  The closed enum snapshots
/// that decision once per completion scan, preventing the hot loop from
/// repeatedly resolving and invoking the symbol's function cell.  Other Lisp
/// predicates retain ordinary callable dispatch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ScanningCompletionPredicate {
    AcceptAll,
    CommandpPrimitive,
    Callable(Value),
}

impl ScanningCompletionPredicate {
    fn classify(predicate: Value) -> Self {
        if predicate.is_nil() {
            Self::AcceptAll
        } else if predicate.as_symbol_id() == Some(super::interactive::CommandpSymbol::id()) {
            Self::CommandpPrimitive
        } else {
            Self::Callable(predicate)
        }
    }

    fn matches(
        self,
        eval: &mut super::eval::Context,
        candidate: &CompletionCandidate,
    ) -> Result<bool, Flow> {
        match self {
            Self::AcceptAll => Ok(true),
            Self::CommandpPrimitive => super::interactive::builtin_commandp_interactive(
                eval,
                std::slice::from_ref(&candidate.predicate_arg),
            )
            .map(|result| result.is_truthy()),
            Self::Callable(function) => ordinary_completion_predicate_matches_with(
                function,
                candidate,
                |function, call_args| eval.apply(function, call_args),
            ),
        }
    }
}

pub(crate) fn builtin_try_completion_with_candidates(
    eval: &mut super::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[crate::heap_types::LispString],
    syntax: super::builtins::search::FastStringMatchSyntax,
) -> EvalResult {
    expect_min_args("try-completion", args, 2)?;
    expect_max_args("try-completion", args, 3)?;
    let string = expect_lisp_string(&args[0])?;
    let predicate_value = args.get(2).copied().unwrap_or(Value::NIL);
    let predicate = ScanningCompletionPredicate::classify(predicate_value);
    let collection = args[1];

    let Some(candidates) = candidates else {
        return eval.apply(collection, vec![args[0], predicate_value, Value::NIL]);
    };

    // Faithful port of GNU `Ftry_completion` (src/minibuf.c).  We iterate over
    // the candidates maintaining a running `bestmatch` whose leading
    // `bestmatchsize` characters are common to every accepted completion.  When
    // `completion_ignore_case` is set the *identity* of `bestmatch` can switch
    // to a later candidate so the returned case pattern matches GNU exactly.
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let string_codes = prefix.characters();
    let string_schars = string_codes.len();

    let mut bestmatch: Option<&CompletionCandidate> = None;
    let mut best_codes: Vec<u32> = Vec::new();
    let mut bestmatchsize = 0usize;
    let mut matchcount = 0i32;

    for candidate in &candidates {
        if !prefix.matches(&candidate.completion, ignore_case) {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                &eval.obarray,
                &eval.buffers,
                &candidate.completion,
                regexps,
                ignore_case,
            )?
        {
            continue;
        }
        if !predicate.matches(eval, candidate)? {
            continue;
        }

        let elt_codes = completion_char_codes(candidate.completion.lisp_string());
        let elt_schars = elt_codes.len();

        if bestmatch.is_none() {
            matchcount = 1;
            bestmatch = Some(candidate);
            bestmatchsize = elt_schars;
            best_codes = elt_codes;
            continue;
        }

        let best_schars = best_codes.len();
        let compare = bestmatchsize.min(elt_schars);
        let cmp = gnu_compare_strings(&best_codes, &elt_codes, compare, ignore_case);
        let matchsize = cmp.match_size(compare);

        // Whether the previous bestmatch (case-sensitively) prefix-matched the
        // first `compare` chars of this element — used for the matchcount bump.
        let old_best_prefix_eq =
            gnu_compare_strings(&best_codes, &elt_codes, compare, false) == StringCompare::Equal;

        let mut switched = false;
        if ignore_case {
            // If this is an exact match except for case, prefer it so the
            // returned value carries the actual match's case pattern.
            let elt_exact = matchsize == elt_schars;
            let best_exact = matchsize == best_schars;
            let cond1 = elt_exact && matchsize < best_schars;
            // Otherwise: when both (or neither) are exact, prefer the candidate
            // whose case agrees with the input over the current bestmatch which
            // does not.
            let elt_matches_input_case =
                gnu_compare_strings(&elt_codes, string_codes, string_schars, false)
                    == StringCompare::Equal;
            let best_matches_input_case =
                gnu_compare_strings(&best_codes, string_codes, string_schars, false)
                    == StringCompare::Equal;
            let cond2 =
                (elt_exact == best_exact) && elt_matches_input_case && !best_matches_input_case;
            if cond1 || cond2 {
                bestmatch = Some(candidate);
                switched = true;
            }
        }

        if best_schars != elt_schars
            || bestmatchsize != matchsize
            || (ignore_case && !old_best_prefix_eq)
        {
            // Don't count the same string multiple times.
            if matchcount <= 1 {
                matchcount += 1;
            }
        }

        bestmatchsize = matchsize;
        if switched {
            best_codes = elt_codes;
        }

        if matchsize <= string_schars && !ignore_case && matchcount > 1 {
            // No need to look any further.
            break;
        }
    }

    let Some(best) = bestmatch else {
        return Ok(Value::NIL);
    };

    // Return t if the supplied string is an exact (case-sensitive) match.
    if matchcount == 1
        && best_codes.len() == string_schars
        && best_codes
            .iter()
            .zip(string_codes.iter())
            .all(|(l, r)| l == r)
    {
        return Ok(Value::T);
    }

    Ok(best.completion.substring_value(bestmatchsize))
}

pub(crate) fn builtin_all_completions_with_candidates(
    eval: &mut super::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[crate::heap_types::LispString],
    syntax: super::builtins::search::FastStringMatchSyntax,
) -> EvalResult {
    expect_min_args("all-completions", args, 2)?;
    expect_max_args("all-completions", args, 3)?;
    let string = expect_lisp_string(&args[0])?;
    let predicate_value = args.get(2).copied().unwrap_or(Value::NIL);
    let predicate = ScanningCompletionPredicate::classify(predicate_value);
    let collection = args[1];

    let Some(candidates) = candidates else {
        return eval.apply(collection, vec![args[0], predicate_value, Value::T]);
    };

    // Two-pass approach: first filter candidates using the predicate
    // (which may trigger GC via apply), then create string Values.
    // This avoids holding unrooted Value strings across GC-triggering
    // predicate calls.
    let prefix = CompletionPrefix::from_lisp_string(&string);
    let mut matching_completions: Vec<CompletionText> = Vec::new();
    for candidate in &candidates {
        if !prefix.matches(&candidate.completion, ignore_case) {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                &eval.obarray,
                &eval.buffers,
                &candidate.completion,
                regexps,
                ignore_case,
            )?
        {
            continue;
        }
        if predicate.matches(eval, candidate)? {
            matching_completions.push(candidate.completion.clone());
        }
    }
    // Now create Values — no GC can trigger between creation and list building
    let matches: Vec<Value> = matching_completions
        .into_iter()
        .map(|completion| completion.as_result_value())
        .collect();
    Ok(Value::list(matches))
}

pub(crate) fn builtin_test_completion_with_candidates(
    eval: &mut super::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[crate::heap_types::LispString],
    syntax: super::builtins::search::FastStringMatchSyntax,
) -> EvalResult {
    expect_min_args("test-completion", args, 2)?;
    expect_max_args("test-completion", args, 3)?;
    let string = expect_lisp_string(&args[0])?;
    let predicate = args.get(2).copied().unwrap_or(Value::NIL);
    let collection = args[1];

    let Some(candidates) = candidates else {
        return eval.apply(
            collection,
            vec![args[0], predicate, Value::symbol("lambda")],
        );
    };

    let prefix = CompletionPrefix::from_lisp_string(&string);
    for candidate in &candidates {
        if !completion_text_equals_string(&candidate.completion, prefix.characters(), ignore_case) {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                &eval.obarray,
                &eval.buffers,
                &candidate.completion,
                regexps,
                ignore_case,
            )?
        {
            continue;
        }
        if ordinary_completion_predicate_matches_with(
            predicate,
            candidate,
            |function, call_args| eval.apply(function, call_args),
        )? {
            return Ok(Value::T);
        }
    }
    Ok(Value::NIL)
}

fn completion_candidates_from_custom_obarray(collection: Value) -> Vec<CompletionCandidate> {
    let slots = super::builtins::symbols::obarray_buckets(collection).unwrap_or_default();
    let mut candidates = Vec::new();
    for slot in slots {
        let mut current = slot;
        loop {
            match current.kind() {
                ValueKind::Nil => break,
                ValueKind::Cons => {
                    let pair_car = current.cons_car();
                    let pair_cdr = current.cons_cdr();
                    if let Some(completion) = completion_text_from_value(&pair_car) {
                        candidates.push(CompletionCandidate {
                            completion,
                            predicate_arg: pair_car,
                            predicate_extra_arg: None,
                        });
                    }
                    current = pair_cdr;
                }
                _ => break,
            }
        }
    }
    candidates
}

fn completion_candidates_from_hash_table(collection: Value) -> Vec<CompletionCandidate> {
    let table = collection.as_hash_table().unwrap().clone();
    let mut candidates = Vec::new();
    for key in table.live_hash_keys_in_slot_order() {
        let Some(value) = table.data.get(key).copied() else {
            continue;
        };
        let visible_key = hash_key_to_visible_value(&table, key);
        if let Some(completion) = completion_text_from_value(&visible_key) {
            candidates.push(CompletionCandidate {
                completion,
                predicate_arg: visible_key,
                predicate_extra_arg: Some(value),
            });
        }
    }
    candidates
}

/// Read the `completion-ignore-case` symbol from the obarray.
fn completion_ignore_case(obarray: &Obarray) -> bool {
    obarray
        .symbol_value_id_copied(CompletionStateVariable::CompletionIgnoreCase.symbol_id())
        .is_some_and(|v| v.is_truthy())
}

pub(crate) fn completion_regexp_lisp_list_from_obarray(
    obarray: &Obarray,
) -> Vec<crate::heap_types::LispString> {
    let Some(val) =
        obarray.symbol_value_id_copied(CompletionStateVariable::CompletionRegexpList.symbol_id())
    else {
        return Vec::new();
    };
    let Some(items) = super::value::list_to_vec(&val) else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(|item| item.as_lisp_string().cloned())
        .collect()
}

/// Return `true` when `candidate` matches **all** regexps in `regexps`.
///
/// GNU `match_regexps` / `Ftry_completion` (`src/minibuf.c:1592`,
/// `src/minibuf.c` candidate loop) run `fast_string_match_internal`, so the
/// match carries the current buffer's syntax state via `syntax`.
fn matches_completion_regexps(
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    candidate: &CompletionText,
    regexps: &[crate::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    completion_string_matches_regexps(
        syntax,
        obarray,
        buffers,
        candidate.lisp_string(),
        candidate.searched_string(),
        regexps,
        ignore_case,
    )
}

pub(crate) fn lisp_string_matches_completion_regexps(
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    candidate: &crate::heap_types::LispString,
    regexps: &[crate::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    completion_string_matches_regexps(
        syntax,
        obarray,
        buffers,
        candidate,
        super::regex::SearchedString::Owned(candidate.clone()),
        regexps,
        ignore_case,
    )
}

#[allow(clippy::too_many_arguments)] // match-time state stays explicit at the GNU-regexp boundary
fn completion_string_matches_regexps(
    syntax: super::builtins::search::FastStringMatchSyntax,
    obarray: &super::symbol::Obarray,
    buffers: &crate::buffer::BufferManager,
    candidate: &crate::heap_types::LispString,
    searched_string: super::regex::SearchedString,
    regexps: &[crate::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    for re in regexps {
        match syntax.search(
            obarray,
            buffers,
            re,
            candidate,
            searched_string.clone(),
            0,
            ignore_case,
        ) {
            Ok(Some(_)) => {} // matched — continue checking remaining regexps
            Ok(None) => return Ok(false),
            Err(message) => {
                return Err(signal(
                    LispCondition::InvalidRegexp,
                    vec![Value::string(message)],
                ));
            }
        }
    }
    Ok(true)
}

/// Thread every heap Value a completion-candidate set holds (original string
/// objects, predicate args) onto one heap list: a SINGLE root keeps the set
/// alive while the completion PREDICATE runs arbitrary Lisp that can unlink
/// candidates from the (rooted) collection they were copied from.
fn completion_candidates_root_holder(candidates: &[CompletionCandidate]) -> Value {
    let mut holder = Value::NIL;
    for candidate in candidates.iter().rev() {
        if let CompletionText::LispObject(value) = &candidate.completion {
            holder = Value::cons(*value, holder);
        }
        if candidate.predicate_arg.is_heap_object() {
            holder = Value::cons(candidate.predicate_arg, holder);
        }
        if let Some(extra) = candidate.predicate_extra_arg
            && extra.is_heap_object()
        {
            holder = Value::cons(extra, holder);
        }
    }
    holder
}

pub(crate) fn builtin_try_completion(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(&eval.obarray);
    let regexps = completion_regexp_lisp_list_from_obarray(&eval.obarray);
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    // Root the candidate set across the predicate calls (see the holder doc).
    let root_scope = eval.save_specpdl_roots();
    if let Some(candidates) = &candidates {
        eval.push_specpdl_root(completion_candidates_root_holder(candidates));
    }
    let result = builtin_try_completion_with_candidates(
        eval,
        &args,
        candidates,
        ignore_case,
        &regexps,
        syntax,
    );
    eval.restore_specpdl_roots(root_scope);
    result
}

pub(crate) fn builtin_all_completions(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(&eval.obarray);
    let regexps = completion_regexp_lisp_list_from_obarray(&eval.obarray);
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    // Root the candidate set across the predicate calls (see the holder doc).
    let root_scope = eval.save_specpdl_roots();
    if let Some(candidates) = &candidates {
        eval.push_specpdl_root(completion_candidates_root_holder(candidates));
    }
    let result = builtin_all_completions_with_candidates(
        eval,
        &args,
        candidates,
        ignore_case,
        &regexps,
        syntax,
    );
    eval.restore_specpdl_roots(root_scope);
    result
}

pub(crate) fn builtin_test_completion(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(&eval.obarray);
    let regexps = completion_regexp_lisp_list_from_obarray(&eval.obarray);
    let syntax = super::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
    // Root the candidate set across the predicate calls (see the holder doc).
    let root_scope = eval.save_specpdl_roots();
    if let Some(candidates) = &candidates {
        eval.push_specpdl_root(completion_candidates_root_holder(candidates));
    }
    let result = builtin_test_completion_with_candidates(
        eval,
        &args,
        candidates,
        ignore_case,
        &regexps,
        syntax,
    );
    eval.restore_specpdl_roots(root_scope);
    result
}

/// `(completion--flex-cost-gotoh PAT STR)`
///
/// Compute the cost of PAT matching STR using a modified Gotoh affine-gap
/// algorithm.  Returns nil when there is no match, else `(COST . MATCHES)`
/// where COST is a fixnum (lower is better) and MATCHES is a list, the same
/// length as PAT, whose i-th element is the position in STR where PAT's i-th
/// character matched.
///
/// Faithful port of GNU Emacs 31.0.90 `Fcompletion__flex_cost_gotoh`
/// (src/minibuf.c:2334).
pub(crate) fn builtin_flex_cost_gotoh(
    eval: &mut super::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // Pre-allocated matrix size limits, mirroring the C macros.
    const FLEX_MAX_STR_SIZE: usize = 512;
    const FLEX_MAX_PAT_SIZE: usize = 128;
    const FLEX_MAX_MATRIX_SIZE: usize = FLEX_MAX_PAT_SIZE * FLEX_MAX_STR_SIZE;

    let pat = expect_lisp_string(&args[0])?;
    let str = expect_lisp_string(&args[1])?;

    // Operate on character vectors (GNU uses SCHARS / fetch_string_char).
    // Completion text is UTF-8; fall back to a lossy decode for raw bytes so
    // we never panic, matching GNU's "process anyway" intent.
    let pat_chars: Vec<char> = match pat.as_utf8_str() {
        Some(s) => s.chars().collect(),
        None => crate::emacs_core::emacs_char::to_utf8_lossy(pat.as_bytes())
            .chars()
            .collect(),
    };
    let str_chars: Vec<char> = match str.as_utf8_str() {
        Some(s) => s.chars().collect(),
        None => crate::emacs_core::emacs_char::to_utf8_lossy(str.as_bytes())
            .chars()
            .collect(),
    };

    let patlen = pat_chars.len();
    let strlen = str_chars.len();
    let width = strlen + 1;
    let size = (patlen + 1) * width;

    const GAP_OPEN_COST: i32 = 10;
    const GAP_EXTEND_COST: i32 = 1;
    const POS_INF: i32 = i32::MAX / 2;

    // Bail if strings are empty or matrix too large.
    if patlen == 0 || strlen == 0 || size > FLEX_MAX_MATRIX_SIZE {
        return Ok(Value::NIL);
    }

    let ignore_case = completion_ignore_case(&eval.obarray);

    // Cheap subsequence pre-filter for the common case-sensitive case: if PAT
    // is not a subsequence of STR there can be no match, so bail before the
    // O(N*M) DP below.
    if !ignore_case {
        let mut pi = 0;
        for &sc in &str_chars {
            if pi >= patlen {
                break;
            }
            if sc == pat_chars[pi] {
                pi += 1;
            }
        }
        if pi < patlen {
            return Ok(Value::NIL);
        }
    }

    // Flat (patlen+1) x width matrices, indexed via MAT(i, j) for i in
    // -1..patlen-1 and j in -1..strlen-1.  Initialize to +inf...
    let mut m = vec![POS_INF; size];
    let mut d = vec![POS_INF; size];
    // ...except the first row of D, which gets gap_open_cost/2 for cheaper
    // leading gaps, and D[-1,-1] = 0 to promote matches at the beginning.
    d[..width].fill(GAP_OPEN_COST / 2);
    d[0] = 0;

    let idx = |i: isize, j: isize| -> usize { ((i + 1) as usize) * width + (j + 1) as usize };

    let downcased =
        |c: char| -> i64 { crate::emacs_core::builtins::downcase_char_code_emacs_compat(c as i64) };

    // Position (column index) of the first match found in the previous row, to
    // save iterations.
    let mut prev_match: usize = 0;

    // Forward pass.
    for (i, &pat_char) in pat_chars.iter().enumerate() {
        let mut match_seen = false;
        let start = prev_match;
        for (j, &str_char) in str_chars.iter().enumerate().skip(start) {
            let cmatch = if ignore_case {
                downcased(pat_char) == downcased(str_char)
            } else {
                pat_char == str_char
            };

            let ii = i as isize;
            let jj = j as isize;

            if cmatch {
                if !match_seen {
                    match_seen = true;
                    prev_match = j;
                }
                // Best of "previous char also matched" (M[i-1,j-1]) and
                // "arrive at this match from a gap" (D[i-1,j-1]).
                m[idx(ii, jj)] = m[idx(ii - 1, jj - 1)].min(d[idx(ii - 1, jj - 1)]);
            }
            // Best accumulated gapping cost: open a gap from a match on this
            // row, or extend a gap started earlier.
            d[idx(ii, jj)] =
                (m[idx(ii, jj - 1)] + GAP_OPEN_COST).min(d[idx(ii, jj - 1)] + GAP_EXTEND_COST);
        }
    }

    // Find lowest cost in last row.
    let mut best_cost = POS_INF;
    let mut lastcol: isize = -1;
    for j in 0..strlen {
        let cost = m[idx(patlen as isize - 1, j as isize)];
        if cost < best_cost {
            best_cost = cost;
            lastcol = j as isize;
        }
    }

    // Return early if no match.
    if lastcol < 0 || best_cost >= POS_INF {
        return Ok(Value::NIL);
    }

    // Go backwards to build the match-positions list.
    let mut positions: Vec<i64> = Vec::with_capacity(patlen);
    positions.push(lastcol as i64);
    let mut l: isize = lastcol;
    let mut i: isize = patlen as isize - 2;
    while i >= 0 {
        // do { --l } while (l >= 0 && M[i,l] >= D[i,l]);
        loop {
            l -= 1;
            if !(l >= 0 && m[idx(i, l)] >= d[idx(i, l)]) {
                break;
            }
        }
        positions.push(l as i64);
        i -= 1;
    }
    // positions was built last-to-first; reverse to PAT order.
    positions.reverse();

    let mut matches = Value::NIL;
    for &pos in positions.iter().rev() {
        matches = Value::cons(Value::fixnum(pos), matches);
    }
    Ok(Value::cons(Value::fixnum(best_cost as i64), matches))
}

fn end_of_file_stdin_error() -> Flow {
    signal(
        LispCondition::EndOfFile,
        vec![Value::string("Error reading from stdin")],
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
