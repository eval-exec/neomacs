//! Minibuffer state owned by the evaluator layer (stage-4 W2/E5).

use neovm_edit::buffer::BufferId;
use neovm_value::gc_trace::GcTrace;
use neovm_value::heap_types::LispString;
use neovm_value::intern::SymId;
use neovm_value::value::Value;
use std::collections::HashMap;
use std::convert::AsRef;

/// What can be completed against.
pub enum CompletionTable {
    /// Fixed list of completion candidates.
    List(Vec<LispString>),
    /// Dynamic completion function: given the current input, returns matching candidates.
    #[allow(clippy::type_complexity)]
    // callable is the public dynamic-completion representation
    Function(Box<dyn Fn(&LispString) -> Vec<LispString>>),
    /// File name completion rooted at a directory.
    FileNames { directory: CompletionDirectory },
    /// Buffer name completion (candidates supplied externally).
    BufferNames,
    /// Symbol name completion (candidates supplied externally).
    SymbolNames,
    /// Association list: each entry is (key, value).
    Alist(Vec<(LispString, Value)>),
}

/// Native directory spelling: filesystem lookup cannot retain text properties.
/// The private payload and shared-only view preserve this constructor invariant.
#[derive(Clone, Debug)]
#[repr(transparent)]
pub struct CompletionDirectory(pub(crate) LispString);

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

/// Named history lists (e.g. "minibuffer-history", "file-name-history", ...).
pub struct MinibufferHistory {
    pub(crate) histories: HashMap<SymId, Vec<LispString>>,
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

/// Owns all minibuffer state, including the recursive-edit stack.
pub struct MinibufferManager {
    pub state_stack: Vec<MinibufferState>,
    pub(crate) history: MinibufferHistory,
    pub(crate) completion_style: CompletionStyle,
    #[cfg(test)]
    pub(crate) max_depth: usize,
}

impl GcTrace for MinibufferManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.trace_roots_with(&mut |value| roots.push(value));
    }

    fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        let Self {
            state_stack,
            history,
            completion_style: _,
            #[cfg(test)]
                max_depth: _,
        } = self;
        for state in state_stack {
            // No rest pattern: a newly retained field needs a tracing decision.
            let MinibufferState {
                buffer_id: _,
                prompt,
                prompt_end: _,
                initial_input,
                history,
                history_position: _,
                content,
                cursor_pos: _,
                completion_table,
                require_match,
                default_value,
                active: _,
                depth: _,
                command_loop_depth: _,
            } = state;
            for text in [prompt, initial_input, content]
                .into_iter()
                .chain(history)
                .chain(default_value)
            {
                text.trace_roots_with(visit);
            }
            visit(*require_match);
            if let Some(table) = completion_table {
                match table {
                    CompletionTable::List(candidates) => {
                        for candidate in candidates {
                            candidate.trace_roots_with(visit);
                        }
                    }
                    CompletionTable::Alist(entries) => {
                        for (key, value) in entries {
                            key.trace_roots_with(visit);
                            visit(*value);
                        }
                    }
                    // The native directory type cannot carry interval edges.
                    CompletionTable::FileNames { directory: _ }
                    | CompletionTable::BufferNames
                    | CompletionTable::SymbolNames => {}
                    // An opaque native callback has no enumerable capture
                    // record; this walk covers the manager's declared fields.
                    CompletionTable::Function(_) => {}
                }
            }
        }
        let MinibufferHistory { histories } = history;
        for entries in histories.values() {
            for text in entries {
                text.trace_roots_with(visit);
            }
        }
    }
}

// --- E5-FM-6c1 data impls (stage-4 W2/E5) ---

impl From<&LispString> for CompletionDirectory {
    fn from(directory: &LispString) -> Self {
        Self(directory.without_properties())
    }
}

impl AsRef<LispString> for CompletionDirectory {
    fn as_ref(&self) -> &LispString {
        &self.0
    }
}

impl Default for MinibufferManager {
    fn default() -> Self {
        Self {
            state_stack: Vec::new(),
            history: MinibufferHistory::new(),
            completion_style: CompletionStyle::Prefix,
            #[cfg(test)]
            max_depth: 10,
        }
    }
}

impl MinibufferManager {
    /// Construct an empty manager (the command-layer entry constructor).
    pub fn new() -> Self {
        Self {
            state_stack: Vec::new(),
            history: MinibufferHistory::new(),
            completion_style: CompletionStyle::Prefix,
            #[cfg(test)]
            max_depth: 10,
        }
    }

    /// Current recursive minibuffer depth (0 = not in minibuffer).
    pub fn depth(&self) -> usize {
        self.state_stack.len()
    }

    /// Append one history entry from a Lisp string.
    pub fn add_to_history_lisp(&mut self, name: SymId, value: LispString, max_length: usize) {
        self.history.add(name, value, max_length);
    }

    /// Borrowed minibuffer history ring (peer #1005 field remediation): the
    /// field is `pub(crate)` in every arm and this is the read surface for
    /// the moved minibuffer oracle.
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(any(test, feature = "test-support"), visibility::make(pub))]
    pub(crate) fn history_for_test(&self) -> &MinibufferHistory {
        &self.history
    }
}

/// Whether the minibuffer entry keeps or replaces the selected window.
///
/// Pure data: the evaluator's entry/exit paths and the core minibuffer window
/// session both compute it, so it lives with the minibuffer state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MinibufferSelectedWindowUpdate {
    Preserve,
    Replace(neovm_edit::emacs_core::display_metrics::WindowId),
}

impl MinibufferSelectedWindowUpdate {
    pub fn for_entry(
        level: neovm_value::minibuffer_state::MinibufferEntryLevel,
        selected_window: neovm_edit::emacs_core::display_metrics::WindowId,
        minibuffer_window: neovm_edit::emacs_core::display_metrics::WindowId,
    ) -> Self {
        match level {
            neovm_value::minibuffer_state::MinibufferEntryLevel::Outermost => {
                Self::Replace(selected_window)
            }
            neovm_value::minibuffer_state::MinibufferEntryLevel::Recursive
                if selected_window == minibuffer_window =>
            {
                Self::Preserve
            }
            neovm_value::minibuffer_state::MinibufferEntryLevel::Recursive => {
                Self::Replace(selected_window)
            }
        }
    }

    pub fn apply(self, target: &mut Option<neovm_edit::emacs_core::display_metrics::WindowId>) {
        if let Self::Replace(window) = self {
            *target = Some(window);
        }
    }
}
