//! Entry-layer minibuffer vocabulary (stage-4 K2 S1): the manager/state
//! extension traits, their admission types, and the completion matching
//! helpers.  Everything here names only eval-data state and lower-crate
//! types, so the evaluator no longer reaches up into the core minibuffer
//! command module for `MinibufferManagerExt`.

use super::minibuffer::{
    CompletionStyle, CompletionTable, MinibufferHistory, MinibufferManager, MinibufferState,
};
use crate::emacs_core::eval::VariableView;
use neovm_edit::buffer::{BufferId, CharPos0, EmacsByteLen};
use neovm_value::error::{Flow, LispCondition, signal};
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;
use neovm_value::minibuffer_state::MinibufferEntryLevel;
use neovm_value::value::Value;
use std::num::NonZeroUsize;

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
