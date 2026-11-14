//! Entry-layer minibuffer vocabulary (stage-4 K2 S1): the manager/state
//! extension traits, their admission types, and the completion matching
//! helpers. Everything here names only evaluator state and lower-crate
//! types, so the evaluator no longer reaches up into the core minibuffer
//! command module for `MinibufferManagerExt`.

use super::minibuffer::{
    CompletionStyle, CompletionTable, MinibufferHistory, MinibufferManager, MinibufferState,
};
use crate::emacs_core::eval::VariableView;
use neovm_edit::buffer::{BufferId, BufferManager, CharPos0, EmacsByteLen};
use neovm_value::error::{
    EvalResult, Flow, LispCondition, expect_max_args, expect_min_args, signal,
};
use neovm_value::heap_types::LispString;
use neovm_value::intern::{NIL_SYM_ID, SymId, T_SYM_ID, resolve_sym};
use neovm_value::minibuffer_state::MinibufferEntryLevel;
use neovm_value::symbol::Obarray;
use neovm_value::value::{Value, ValueKind, VecLikeType};
use std::num::NonZeroUsize;
use strum::IntoStaticStr;

pub trait CompletionTableExt {
    fn candidates(&self, input: &LispString) -> Vec<LispString>;
}

impl CompletionTableExt for CompletionTable {
    /// Extract the raw string candidates from the table.
    ///
    /// For `Function` tables the `input` is passed through; for static tables it
    /// is ignored (filtering happens later in the matching functions).
    fn candidates(&self, input: &LispString) -> Vec<LispString> {
        match self {
            CompletionTable::List(v) => v.clone(),
            CompletionTable::Function(f) => f(input),
            CompletionTable::FileNames { directory } => list_files_in_dir(directory.as_ref()),
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

/// Initial minibuffer text paired with a valid character cursor.
///
/// GNU's public reader APIs express this cursor with subtly different Lisp
/// coordinate conventions.  Those conventions are normalized at the reader
/// boundary; the minibuffer core only accepts this checked, zero-based
/// character coordinate, so it cannot confuse it with a byte offset or lose
/// it while passing the text through setup.
#[derive(Clone, Debug)]
#[doc(hidden)]
pub struct MinibufferInitialContents {
    text: LispString,
    cursor: CharPos0,
}

impl MinibufferInitialContents {
    #[doc(hidden)]
    pub fn at_end(text: LispString) -> Self {
        let cursor = CharPos0::new(text.schars());
        Self { text, cursor }
    }

    #[doc(hidden)]
    pub fn at_character_offset(text: LispString, cursor: CharPos0) -> Option<Self> {
        (cursor.get() <= text.schars()).then_some(Self { text, cursor })
    }

    #[doc(hidden)]
    pub fn text(&self) -> &LispString {
        &self.text
    }

    #[doc(hidden)]
    pub fn cursor_byte_offset(&self) -> EmacsByteLen {
        EmacsByteLen::new(self.text.char_to_byte_pos(self.cursor.get()))
    }
}

pub trait MinibufferStateExt {
    fn new(
        buffer_id: BufferId,
        prompt: LispString,
        initial: Option<&MinibufferInitialContents>,
        depth: usize,
    ) -> Self;
}

impl MinibufferStateExt for MinibufferState {
    fn new(
        buffer_id: BufferId,
        prompt: LispString,
        initial: Option<&MinibufferInitialContents>,
        depth: usize,
    ) -> Self {
        let initial_text = initial
            .map(|contents| contents.text.clone())
            .unwrap_or_else(|| LispString::from_utf8(""));
        let cursor_pos = initial
            .map(MinibufferInitialContents::cursor_byte_offset)
            .unwrap_or(EmacsByteLen::ZERO)
            .get();
        let prompt_end = prompt.sbytes();
        Self {
            buffer_id,
            prompt,
            prompt_end,
            initial_input: initial_text.clone(),
            history: Vec::new(),
            history_position: None,
            content: initial_text,
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

/// Whether a new minibuffer may be entered while another one is active.
///
/// Keeping the Lisp option out of `MinibufferManager` makes admission an
/// explicit operation for each read instead of mutable ambient state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum RecursiveMinibufferPolicy {
    Allow,
    Reject,
}

/// The exhaustive reasons a minibuffer entry can be rejected before it has
/// changed any buffer, window, or command-loop state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum MinibufferEntryRejection {
    RecursiveDisabled,
    #[cfg(test)]
    TestDepthLimit,
}

impl MinibufferEntryRejection {
    #[doc(hidden)]
    pub fn into_flow(self) -> Flow {
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
#[doc(hidden)]
pub struct MinibufferEntryPermit {
    parent_depth: usize,
    depth: NonZeroUsize,
}

impl MinibufferEntryPermit {
    #[doc(hidden)]
    pub fn depth(&self) -> usize {
        self.depth.get()
    }

    #[doc(hidden)]
    pub fn level(&self) -> MinibufferEntryLevel {
        MinibufferEntryLevel::from_depth(self.depth)
    }
}

pub trait MinibufferManagerExt {
    fn new() -> Self;

    fn prepare_entry(
        &self,
        policy: RecursiveMinibufferPolicy,
    ) -> Result<MinibufferEntryPermit, MinibufferEntryRejection>;

    fn enter_with_permit(
        &mut self,
        permit: MinibufferEntryPermit,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&MinibufferInitialContents>,
        history_name: Option<SymId>,
    ) -> &mut MinibufferState;

    fn read_from_minibuffer_lisp(
        &mut self,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&LispString>,
        history_name: Option<SymId>,
    ) -> Result<&mut MinibufferState, Flow>;

    fn read_from_minibuffer(
        &mut self,
        buffer_id: BufferId,
        prompt: &str,
        initial: Option<&str>,
        history_name: Option<SymId>,
    ) -> Result<&mut MinibufferState, Flow>;

    fn try_complete(&self, state: &MinibufferState) -> CompletionResult;

    fn all_completions(&self, prefix: &LispString, table: &CompletionTable) -> Vec<LispString>;

    fn try_completion_string(
        &self,
        prefix: &LispString,
        table: &CompletionTable,
    ) -> Option<LispString>;

    fn test_completion(&self, string: &LispString, table: &CompletionTable) -> bool;

    fn exit_minibuffer(&mut self) -> Option<LispString>;

    fn abort_minibuffer(&mut self);

    fn history_previous(&mut self) -> Option<LispString>;

    fn history_next(&mut self) -> Option<LispString>;

    fn add_to_history(&mut self, name: SymId, value: &str, max_length: usize);

    fn add_to_history_lisp(&mut self, name: SymId, value: LispString, max_length: usize);

    fn history_length_from_obarray(vars: VariableView<'_>) -> usize;

    fn current(&self) -> Option<&MinibufferState>;

    fn current_mut(&mut self) -> Option<&mut MinibufferState>;

    fn is_active(&self) -> bool;

    fn has_buffer(&self, buffer_id: BufferId) -> bool;

    fn set_completion_style(&mut self, style: CompletionStyle);
}

impl MinibufferManagerExt for MinibufferManager {
    fn new() -> Self {
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
    fn prepare_entry(
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

    fn enter_with_permit(
        &mut self,
        permit: MinibufferEntryPermit,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&MinibufferInitialContents>,
        history_name: Option<SymId>,
    ) -> &mut MinibufferState {
        assert_eq!(
            self.state_stack.len(),
            permit.parent_depth,
            "minibuffer stack changed after entry was admitted"
        );
        let mut state =
            MinibufferState::new(buffer_id, prompt.clone(), initial, permit.depth.get());

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
    fn read_from_minibuffer_lisp(
        &mut self,
        buffer_id: BufferId,
        prompt: &LispString,
        initial: Option<&LispString>,
        history_name: Option<SymId>,
    ) -> Result<&mut MinibufferState, Flow> {
        let permit = self
            .prepare_entry(RecursiveMinibufferPolicy::Allow)
            .map_err(MinibufferEntryRejection::into_flow)?;
        let initial = initial.cloned().map(MinibufferInitialContents::at_end);
        Ok(self.enter_with_permit(permit, buffer_id, prompt, initial.as_ref(), history_name))
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    fn read_from_minibuffer(
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
    fn try_complete(&self, state: &MinibufferState) -> CompletionResult {
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
    fn all_completions(&self, prefix: &LispString, table: &CompletionTable) -> Vec<LispString> {
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
    fn try_completion_string(
        &self,
        prefix: &LispString,
        table: &CompletionTable,
    ) -> Option<LispString> {
        let matches = self.all_completions(prefix, table);
        compute_common_prefix(&matches)
    }

    /// Test whether `string` is an exact match in `table`.
    fn test_completion(&self, string: &LispString, table: &CompletionTable) -> bool {
        let candidates = table.candidates(string);
        candidates.iter().any(|candidate| candidate == string)
    }

    /// Exit the current minibuffer, returning its content (or the default if empty).
    fn exit_minibuffer(&mut self) -> Option<LispString> {
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
    fn abort_minibuffer(&mut self) {
        if let Some(mut state) = self.state_stack.pop() {
            state.active = false;
        }
    }

    /// Navigate to the previous (older) history entry.
    fn history_previous(&mut self) -> Option<LispString> {
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
    fn history_next(&mut self) -> Option<LispString> {
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
    fn add_to_history(&mut self, name: SymId, value: &str, max_length: usize) {
        self.add_to_history_lisp(name, LispString::from_utf8(value), max_length);
    }

    fn add_to_history_lisp(&mut self, name: SymId, value: LispString, max_length: usize) {
        self.history.add(name, value, max_length);
    }

    /// Read the effective `history-length` from the obarray, defaulting to 100.
    fn history_length_from_obarray(vars: VariableView<'_>) -> usize {
        match vars.value_by_name("history-length") {
            Some(v) if v.is_fixnum() && v.xfixnum() > 0 => v.xfixnum() as usize,
            _ => 100,
        }
    }

    /// Reference to the current (innermost) minibuffer state, if any.
    fn current(&self) -> Option<&MinibufferState> {
        self.state_stack.last()
    }

    /// Mutable reference to the current (innermost) minibuffer state.
    fn current_mut(&mut self) -> Option<&mut MinibufferState> {
        self.state_stack.last_mut()
    }

    /// Whether any minibuffer is currently active.
    fn is_active(&self) -> bool {
        self.state_stack.last().is_some_and(|s| s.active)
    }

    fn has_buffer(&self, buffer_id: BufferId) -> bool {
        self.state_stack
            .iter()
            .any(|state| state.buffer_id == buffer_id)
    }

    /// Set the completion style.
    fn set_completion_style(&mut self, style: CompletionStyle) {
        self.completion_style = style;
    }
}

// ---------------------------------------------------------------------------
// Completion matching functions
// ---------------------------------------------------------------------------

/// Case-insensitive prefix matching.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn prefix_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_prefix(input, candidate, true))
        .cloned()
        .collect()
}

/// Substring matching (case-insensitive).
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn substring_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_substring(input, candidate, true))
        .cloned()
        .collect()
}

/// Flex (fuzzy) matching: the input characters must appear in order within the candidate.
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
fn basic_match(input: &LispString, candidates: &[LispString]) -> Vec<LispString> {
    candidates
        .iter()
        .filter(|candidate| lisp_string_matches_prefix(input, candidate, false))
        .cloned()
        .collect()
}

fn lisp_string_prefix_chars(string: &LispString, chars: usize) -> LispString {
    let end_byte = if string.is_multibyte() {
        neovm_value::emacs_char::char_to_byte_pos(string.as_bytes(), chars)
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
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

#[doc(hidden)]
pub fn completion_char_codes(string: &neovm_value::heap_types::LispString) -> Vec<u32> {
    lisp_string_chars(string).collect()
}

#[doc(hidden)]
pub fn lisp_string_chars(
    string: &neovm_value::heap_types::LispString,
) -> impl Iterator<Item = u32> + use<'_> {
    let bytes = string.as_bytes();
    let multibyte = string.is_multibyte();
    let mut pos = 0usize;
    // GNU minibuf.c:1719-1723 compares a declared character prefix. A
    // shorthand name can have SCHARS independent of payload decoding.
    (0..string.schars()).map(move |index| {
        if !multibyte {
            return u32::from(bytes[index]);
        }
        let tail = bytes.get(pos..).unwrap_or_default();
        let (code, width) = neovm_value::heap_types::decode_declared_multibyte_char(tail);
        pos += width;
        code.code()
    })
}

#[doc(hidden)]
pub fn completion_fold_char(code: u32, ignore_case: bool) -> u32 {
    if !ignore_case {
        return code;
    }
    neovm_value::emacs_char::downcase_char_code_emacs_compat(code as i64) as u32
}

#[cfg(test)]
mod depth_limit_tests {
    use super::*;

    #[test]
    fn recursive_depth_limit() {
        crate::test_utils::init_test_tracing();
        let mut mgr = MinibufferManager::new();
        mgr.max_depth = 2;

        mgr.read_from_minibuffer(BufferId(1), "1: ", None, None)
            .unwrap();
        mgr.read_from_minibuffer(BufferId(2), "2: ", None, None)
            .unwrap();
        let result = mgr.read_from_minibuffer(BufferId(3), "3: ", None, None);
        assert!(result.is_err());
    }
}

// GNU scanning completion entries and their one candidate/matcher implementation.
// Core minibuffer readers reuse these entries; no reader/session code lives here.

/// GNU completion state held in predeclared C variables/symbols rather than
/// rediscovered by name at every completion operation.
///
/// The closed enum makes the hot state domain exhaustive: adding another
/// variable requires assigning it a dedicated cache slot instead of silently
/// reintroducing runtime string interning.
#[derive(Clone, Copy, Debug, Eq, PartialEq, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
pub enum CompletionStateVariable {
    CompletionIgnoreCase,
    CompletionRegexpList,
    Obarray,
}

impl CompletionStateVariable {
    #[inline(always)]
    pub fn symbol_id(self) -> SymId {
        use std::sync::OnceLock;

        static COMPLETION_IGNORE_CASE: OnceLock<SymId> = OnceLock::new();
        static COMPLETION_REGEXP_LIST: OnceLock<SymId> = OnceLock::new();
        static OBARRAY: OnceLock<SymId> = OnceLock::new();
        let name: &'static str = self.into();
        match self {
            Self::CompletionIgnoreCase => {
                *COMPLETION_IGNORE_CASE.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::CompletionRegexpList => {
                *COMPLETION_REGEXP_LIST.get_or_init(|| neovm_value::intern::intern(name))
            }
            Self::Obarray => *OBARRAY.get_or_init(|| neovm_value::intern::intern(name)),
        }
    }
}

pub fn expect_lisp_string(value: &Value) -> Result<neovm_value::heap_types::LispString, Flow> {
    value.as_lisp_string().cloned().ok_or_else(|| {
        signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), *value],
        )
    })
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
        string: &'static neovm_value::heap_types::LispString,
    },
}

impl CompletionText {
    fn lisp_string(&self) -> &neovm_value::heap_types::LispString {
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
                neovm_value::intern::materialize_symbol_name_value(*symbol)
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
            neovm_value::emacs_char::char_to_byte_pos(string.as_bytes(), end_chars)
        } else {
            end_chars.min(string.byte_len())
        };
        let sliced = string
            .slice(0, end_byte)
            .expect("validated completion prefix slice");
        Value::heap_string(sliced)
    }

    fn searched_string(&self) -> neovm_text::emacs_core::regex::SearchedString {
        match self {
            Self::LispObject(value) => neovm_text::emacs_core::regex::SearchedString::Heap(*value),
            Self::Atom { string, .. } => {
                neovm_text::emacs_core::regex::SearchedString::Owned((*string).clone())
            }
        }
    }
}

fn completion_text_from_value(value: &Value) -> Option<CompletionText> {
    match value.kind() {
        ValueKind::String => Some(CompletionText::LispObject(*value)),
        ValueKind::Symbol(id) => Some(completion_text_from_symbol_name(
            neovm_value::intern::resolve_lisp_visible_symbol_name(id),
        )),
        ValueKind::Nil => Some(completion_text_from_symbol_name(
            neovm_value::intern::resolve_lisp_visible_symbol_name(NIL_SYM_ID),
        )),
        ValueKind::T => Some(completion_text_from_symbol_name(
            neovm_value::intern::resolve_lisp_visible_symbol_name(T_SYM_ID),
        )),
        _ => None,
    }
}

fn completion_text_from_symbol_name(
    name: neovm_value::intern::LispVisibleSymbolName,
) -> CompletionText {
    match name {
        neovm_value::intern::LispVisibleSymbolName::LispObject(value) => {
            CompletionText::LispObject(value)
        }
        neovm_value::intern::LispVisibleSymbolName::Atom { symbol, string } => {
            CompletionText::Atom { symbol, string }
        }
    }
}

#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub fn completion_display_string_from_value(value: &Value) -> Option<String> {
    let completion = completion_text_from_value(value)?;
    let string = completion.lisp_string();
    Some(
        string
            .as_utf8_str()
            .map(|text| text.to_owned())
            .unwrap_or_else(|| neovm_value::emacs_char::to_utf8_lossy(string.as_bytes())),
    )
}

fn completion_candidates_from_list_value(collection: &Value) -> Vec<CompletionCandidate> {
    let items = match neovm_value::value::list_to_vec(collection) {
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
    fn from_lisp_string(string: &neovm_value::heap_types::LispString) -> Self {
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

#[inline]
fn completion_prefix_matches_with_parity(
    ctx: &mut crate::emacs_core::eval::Context,
    string: &neovm_value::heap_types::LispString,
    prefix: &CompletionPrefix,
    completion: &CompletionText,
    ignore_case: bool,
    parity: bool,
) -> Result<bool, Flow> {
    if !ignore_case || !parity {
        return Ok(prefix.matches(completion, ignore_case));
    }
    if string.schars() > completion.lisp_string().schars() {
        return Ok(false);
    }
    Ok(crate::emacs_core::fns::compare_string_prefixes_in_state(
        ctx,
        string,
        completion.lisp_string(),
        string.schars(),
    )?
    .is_t())
}

#[inline]
fn completion_equals_with_parity(
    ctx: &mut crate::emacs_core::eval::Context,
    string: &neovm_value::heap_types::LispString,
    prefix: &CompletionPrefix,
    completion: &CompletionText,
    ignore_case: bool,
    parity: bool,
) -> Result<bool, Flow> {
    if !ignore_case || !parity {
        return Ok(completion_text_equals_string(
            completion,
            prefix.characters(),
            ignore_case,
        ));
    }
    if string.schars() != completion.lisp_string().schars() {
        return Ok(false);
    }
    Ok(crate::emacs_core::fns::compare_string_prefixes_in_state(
        ctx,
        completion.lisp_string(),
        string,
        string.schars(),
    )?
    .is_t())
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

fn is_global_obarray_proxy_in_state(vars: VariableView<'_>, value: &Value) -> bool {
    vars.value(CompletionStateVariable::Obarray.symbol_id())
        .is_some_and(|proxy| proxy == *value)
}

fn completion_candidates_from_global_obarray_in_state(
    obarray: &Obarray,
    lisp_obarray: Value,
) -> Vec<CompletionCandidate> {
    let ids = crate::emacs_core::builtins::symbols::global_obarray_symbol_ids_in_bucket_order(
        obarray,
        lisp_obarray,
    );
    neovm_value::intern::map_lisp_visible_symbol_names(&ids, |id, name| CompletionCandidate {
        completion: completion_text_from_symbol_name(name),
        predicate_arg: Value::from_sym_id(id),
        predicate_extra_arg: None,
    })
}

pub(crate) fn completion_candidates_from_collection_in_state(
    ctx: &crate::emacs_core::eval::Context,
    collection: &Value,
) -> Result<Option<Vec<CompletionCandidate>>, Flow> {
    let obarray = &ctx.obarray();
    let vars = ctx.variables();
    Ok(match collection.kind() {
        ValueKind::Nil | ValueKind::Cons => Some(completion_candidates_from_list_value(collection)),
        ValueKind::Veclike(VecLikeType::HashTable) => {
            Some(completion_candidates_from_hash_table(*collection))
        }
        ValueKind::Veclike(VecLikeType::Vector)
            if is_global_obarray_proxy_in_state(vars, collection) =>
        {
            Some(completion_candidates_from_global_obarray_in_state(
                obarray,
                *collection,
            ))
        }
        ValueKind::Veclike(VecLikeType::Vector) => {
            let obarray = crate::emacs_core::builtins::symbols::check_obarray_value(*collection)?;
            Some(completion_candidates_from_custom_obarray(obarray))
        }
        ValueKind::Veclike(VecLikeType::Obarray) => {
            Some(completion_candidates_from_custom_obarray(*collection))
        }
        _ => None,
    })
}

fn completion_candidates_from_collection(
    eval: &crate::emacs_core::eval::Context,
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
        } else if predicate.as_bare_symbol_id()
            == Some(crate::emacs_core::eval::interactive_registry::CommandpSymbol::id())
        {
            Self::CommandpPrimitive
        } else {
            Self::Callable(predicate)
        }
    }

    fn matches(
        self,
        eval: &mut crate::emacs_core::eval::Context,
        candidate: &CompletionCandidate,
    ) -> Result<bool, Flow> {
        match self {
            Self::AcceptAll => Ok(true),
            Self::CommandpPrimitive => {
                crate::emacs_core::eval::interactive_registry::builtin_commandp_interactive(
                    eval,
                    std::slice::from_ref(&candidate.predicate_arg),
                )
                .map(|result| result.is_truthy())
            }
            Self::Callable(function) => ordinary_completion_predicate_matches_with(
                function,
                candidate,
                |function, call_args| eval.apply(function, call_args),
            ),
        }
    }
}

pub(crate) fn builtin_try_completion_with_candidates(
    eval: &mut crate::emacs_core::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[neovm_value::heap_types::LispString],
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
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
    let case_table_parity = ignore_case && crate::emacs_core::fns::compare_strings_parity_enabled();
    let string_codes = prefix.characters();
    let string_schars = string_codes.len();

    let mut bestmatch: Option<&CompletionCandidate> = None;
    let mut best_codes: Vec<u32> = Vec::new();
    let mut bestmatchsize = 0usize;
    let mut matchcount = 0i32;

    for candidate in &candidates {
        if !completion_prefix_matches_with_parity(
            eval,
            &string,
            &prefix,
            &candidate.completion,
            ignore_case,
            case_table_parity,
        )? {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                eval.variables(),
                &eval.edit.buffers,
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
        let cmp = if case_table_parity {
            let result = crate::emacs_core::fns::compare_string_prefixes_in_state(
                eval,
                bestmatch
                    .expect("best match was initialized")
                    .completion
                    .lisp_string(),
                candidate.completion.lisp_string(),
                compare,
            )?;
            match result.as_int() {
                Some(pos) if pos < 0 => StringCompare::Less((-pos - 1) as usize),
                Some(pos) => StringCompare::Greater((pos - 1) as usize),
                None => StringCompare::Equal,
            }
        } else {
            gnu_compare_strings(&best_codes, &elt_codes, compare, ignore_case)
        };
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
    eval: &mut crate::emacs_core::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[neovm_value::heap_types::LispString],
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
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
    let case_table_parity = ignore_case && crate::emacs_core::fns::compare_strings_parity_enabled();
    let mut matching_completions: Vec<CompletionText> = Vec::new();
    for candidate in &candidates {
        if !completion_prefix_matches_with_parity(
            eval,
            &string,
            &prefix,
            &candidate.completion,
            ignore_case,
            case_table_parity,
        )? {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                eval.variables(),
                &eval.edit.buffers,
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
    eval: &mut crate::emacs_core::eval::Context,
    args: &[Value],
    candidates: Option<Vec<CompletionCandidate>>,
    ignore_case: bool,
    regexps: &[neovm_value::heap_types::LispString],
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
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
    let case_table_parity = ignore_case && crate::emacs_core::fns::compare_strings_parity_enabled();
    for candidate in &candidates {
        if !completion_equals_with_parity(
            eval,
            &string,
            &prefix,
            &candidate.completion,
            ignore_case,
            case_table_parity,
        )? {
            continue;
        }
        if !regexps.is_empty()
            && !matches_completion_regexps(
                syntax,
                eval.variables(),
                &eval.edit.buffers,
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
    let slots =
        crate::emacs_core::builtins::symbols::obarray_buckets(collection).unwrap_or_default();
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

/// What a single `oblookup` settled about an obarray collection.
///
/// GNU's `Ftest_completion` obarray branch (src/minibuf.c) is exactly three
/// outcomes, and naming them keeps the caller from inventing a fourth:
///
/// ```c
/// tem = oblookup (collection, SSDATA (string), SCHARS (string), SBYTES (string));
/// if (completion_ignore_case && !BARE_SYMBOL_P (tem))
///   DOOBARRAY (...)   /* the walk, comparing with Fcompare_strings */
/// if (!BARE_SYMBOL_P (tem)) return Qnil;
/// ```
///
/// The walk is not merely the slower road: it materialises a
/// `CompletionCandidate` for every symbol in the obarray -- ~17K of them for
/// the global one -- on every `M-x` keystroke.
enum ObarrayProbe {
    /// `oblookup` found the symbol. GNU commits to it: the regexps and the
    /// predicate then decide, and a rejection is nil rather than a reason to
    /// keep looking.
    Found(CompletionCandidate),
    /// `oblookup` missed and `completion-ignore-case` is off, so the walk
    /// compares the same way the lookup just did and cannot find more.
    Absent,
    /// Not an obarray, or a case-insensitive walk is still owed.
    WalkRequired,
}

/// Run GNU's `oblookup` against an obarray collection.
///
/// Both arms build the candidate from the SAME source the corresponding walk
/// would have used -- `resolve_lisp_visible_symbol_name` for the global
/// obarray, `completion_text_from_value` on the bucket entry for a custom one
/// -- so the shortcut cannot answer with a differently spelled completion than
/// the slow path would have.
fn probe_obarray_completion(
    vars: VariableView<'_>,
    collection: Value,
    string: &neovm_value::heap_types::LispString,
    ignore_case: bool,
) -> ObarrayProbe {
    let obarray = vars.obarray();
    use crate::emacs_core::builtins::symbols as sym;
    let found = match collection.kind() {
        ValueKind::Veclike(VecLikeType::Vector)
            if is_global_obarray_proxy_in_state(vars, &collection) =>
        {
            obarray
                .intern_soft_lisp_string(string)
                .map(|id| CompletionCandidate {
                    completion: completion_text_from_symbol_name(
                        neovm_value::intern::resolve_lisp_visible_symbol_name(id),
                    ),
                    predicate_arg: Value::from_sym_id(id),
                    predicate_extra_arg: None,
                })
        }
        ValueKind::Veclike(VecLikeType::Vector | VecLikeType::Obarray) => {
            let Ok(value) = sym::check_obarray_value(collection) else {
                return ObarrayProbe::WalkRequired;
            };
            // A zero-length obarray is `intern-soft`'s wrong-type signal, not
            // something to answer here; let the ordinary path raise it.
            let len = sym::obarray_len(value).unwrap_or(0);
            if len == 0 {
                return ObarrayProbe::WalkRequired;
            }
            let bucket = sym::obarray_bucket(value, sym::obarray_hash_lisp_string(string, len))
                .unwrap_or(Value::NIL);
            sym::obarray_bucket_find(bucket, string).and_then(|symbol| {
                Some(CompletionCandidate {
                    completion: completion_text_from_value(&symbol)?,
                    predicate_arg: symbol,
                    predicate_extra_arg: None,
                })
            })
        }
        _ => return ObarrayProbe::WalkRequired,
    };
    match found {
        Some(candidate) => ObarrayProbe::Found(candidate),
        None if ignore_case => ObarrayProbe::WalkRequired,
        None => ObarrayProbe::Absent,
    }
}

/// Look `string` up DIRECTLY in a hash-table collection, the way GNU does
/// before it considers walking (`hash_find` in `Ftest_completion`,
/// src/minibuf.c, with `DOHASH` only as the fallback).
///
/// Materialising the whole collection first is what made `test-completion`
/// over a 20,000-entry table 213ms against GNU's 0.1ms -- on a path that runs
/// on every minibuffer keystroke.
///
/// `None` means "no direct answer, walk the collection". That covers a real
/// miss, which GNU also falls back on so `completion-ignore-case` can still
/// find a differently cased key.
///
/// The user-defined-test bail is DEFENSIVE, and deliberately labelled as such:
/// `get_by_value` would use `table.test`, which for a `define-hash-table-test`
/// table is `Equal` and not the user's actual predicate, so the lookup would
/// not mean what the table means. I could not construct a case where it
/// changes an answer -- GNU's own fallback walk compares keys with
/// `Fcompare_strings` rather than the table's test, so an `eq`-strict user
/// table still answers `t` for an equal-but-not-eq string in both engines --
/// and a mutation test removing this bail stayed green. It is kept because
/// relying on a lookup whose semantics differ from the table's is fragile,
/// not because a test pins it.
fn direct_hash_completion_candidate(
    collection: Value,
    string: &neovm_value::heap_types::LispString,
    symbols_with_pos_enabled: bool,
) -> Option<CompletionCandidate> {
    let table = collection.as_hash_table()?;
    if table.user_cmp_function.is_some() {
        return None;
    }
    let probe = Value::heap_string(string.clone());
    let value = *table
        .data
        .get_by_value(probe, table.test, symbols_with_pos_enabled)?;
    let completion = completion_text_from_value(&probe)?;
    Some(CompletionCandidate {
        completion,
        predicate_arg: probe,
        predicate_extra_arg: Some(value),
    })
}

fn completion_candidates_from_hash_table(collection: Value) -> Vec<CompletionCandidate> {
    let table = collection.as_hash_table().unwrap().clone();
    let mut candidates = Vec::new();
    for (_, entry) in table.data.keyed_entries_in_slot_order() {
        let value = entry.value;
        let visible_key = entry.key;
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

/// GNU's forwarded completion state follows the current buffer binding
/// (`src/data.c:swap_in_symval_forwarding`), including a local `let` binding.
pub fn completion_ignore_case(vars: VariableView<'_>, buffers: &BufferManager) -> bool {
    vars.value_in_buffer(
        buffers.current_buffer(),
        CompletionStateVariable::CompletionIgnoreCase.symbol_id(),
    )
    .is_some_and(|v| v.is_truthy())
}

pub fn completion_regexp_lisp_list(
    vars: VariableView<'_>,
    buffers: &BufferManager,
) -> Vec<neovm_value::heap_types::LispString> {
    let Some(val) = vars.value_in_buffer(
        buffers.current_buffer(),
        CompletionStateVariable::CompletionRegexpList.symbol_id(),
    ) else {
        return Vec::new();
    };
    let Some(items) = neovm_value::value::list_to_vec(&val) else {
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
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
    vars: VariableView<'_>,
    buffers: &neovm_edit::buffer::BufferManager,
    candidate: &CompletionText,
    regexps: &[neovm_value::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    completion_string_matches_regexps(
        syntax,
        vars,
        buffers,
        candidate.lisp_string(),
        candidate.searched_string(),
        regexps,
        ignore_case,
    )
}

pub fn lisp_string_matches_completion_regexps(
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
    vars: VariableView<'_>,
    buffers: &neovm_edit::buffer::BufferManager,
    candidate: &neovm_value::heap_types::LispString,
    regexps: &[neovm_value::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    completion_string_matches_regexps(
        syntax,
        vars,
        buffers,
        candidate,
        neovm_text::emacs_core::regex::SearchedString::Owned(candidate.clone()),
        regexps,
        ignore_case,
    )
}

#[allow(clippy::too_many_arguments)] // match-time state stays explicit at the GNU-regexp boundary
fn completion_string_matches_regexps(
    syntax: crate::emacs_core::builtins::search::FastStringMatchSyntax,
    vars: VariableView<'_>,
    buffers: &neovm_edit::buffer::BufferManager,
    candidate: &neovm_value::heap_types::LispString,
    searched_string: neovm_text::emacs_core::regex::SearchedString,
    regexps: &[neovm_value::heap_types::LispString],
    ignore_case: bool,
) -> Result<bool, Flow> {
    for re in regexps {
        match syntax.search(
            vars,
            buffers,
            re,
            candidate,
            searched_string.clone(),
            0,
            ignore_case,
        ) {
            Ok(Some(_)) => {} // matched — continue checking remaining regexps
            Ok(None) => return Ok(false),
            Err(crate::emacs_core::builtins::search::FastStringMatchError::Regexp(message)) => {
                return Err(signal(
                    LispCondition::InvalidRegexp,
                    vec![Value::string(message)],
                ));
            }
            Err(error) => return Err(error.into_signal()),
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

pub fn builtin_try_completion(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(eval.variables(), &eval.edit.buffers);
    let regexps = completion_regexp_lisp_list(eval.variables(), &eval.edit.buffers);
    let syntax =
        crate::emacs_core::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
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

pub fn builtin_all_completions(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(eval.variables(), &eval.edit.buffers);
    let regexps = completion_regexp_lisp_list(eval.variables(), &eval.edit.buffers);
    let syntax =
        crate::emacs_core::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
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

pub fn builtin_test_completion(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    // GNU answers a hash-table collection with one `hash_find' and only walks
    // the table when that misses. Taking the same shortcut keeps this off the
    // whole-collection materialisation on every minibuffer keystroke.
    //
    // A direct HIT is committed to, exactly as GNU's `goto found_matching_key'
    // does: the regexps and the predicate then decide, and a rejection is nil
    // rather than a reason to keep looking.
    if args.len() >= 2
        && let Ok(string) = expect_lisp_string(&args[0])
    {
        let ignore_case = completion_ignore_case(eval.variables(), &eval.edit.buffers);
        let probe =
            match direct_hash_completion_candidate(args[1], &string, eval.symbols_with_pos_enabled)
            {
                Some(candidate) => ObarrayProbe::Found(candidate),
                None => probe_obarray_completion(eval.variables(), args[1], &string, ignore_case),
            };
        match probe {
            ObarrayProbe::Found(candidate) => {
                let regexps = completion_regexp_lisp_list(eval.variables(), &eval.edit.buffers);
                let syntax =
                    crate::emacs_core::builtins::search::FastStringMatchSyntax::for_current_buffer(
                        eval,
                    );
                return builtin_test_completion_with_candidates(
                    eval,
                    &args,
                    Some(vec![candidate]),
                    ignore_case,
                    &regexps,
                    syntax,
                );
            }
            // GNU returns straight from the missed `oblookup': with no
            // candidate, neither the regexps nor the predicate can say `t'.
            ObarrayProbe::Absent => return Ok(Value::NIL),
            ObarrayProbe::WalkRequired => {}
        }
    }
    let candidates = completion_candidates_from_collection(eval, &args[1])?;
    let ignore_case = completion_ignore_case(eval.variables(), &eval.edit.buffers);
    let regexps = completion_regexp_lisp_list(eval.variables(), &eval.edit.buffers);
    let syntax =
        crate::emacs_core::builtins::search::FastStringMatchSyntax::for_current_buffer(eval);
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

#[cfg(test)]
#[path = "tests/completion_matcher_test.rs"]
mod completion_matcher_tests;

#[cfg(test)]
#[path = "tests/completion_case_table_test.rs"]
mod completion_case_table_tests;
