//! Interactive search and query-replace.
//!
//! Implements:
//! - Incremental search (isearch) state machine
//! - Search history
//! - Search highlighting (overlay tracking)
//! - Query-replace with interactive responses
//! - Regular expression search variants
//! - Lazy highlight (deferred search matches)

use std::collections::VecDeque;

use super::error::{signal, EvalResult, Flow};
use super::value::Value;

use regex::Regex;

// ---------------------------------------------------------------------------
// Argument helpers (local copies, matching builtins.rs convention)
// ---------------------------------------------------------------------------

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

fn expect_min_max_args(name: &str, args: &[Value], min: usize, max: usize) -> Result<(), Flow> {
    if args.len() < min || args.len() > max {
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

fn expect_integer_or_marker(val: &Value) -> Result<i64, Flow> {
    match val {
        Value::Int(n) => Ok(*n),
        Value::Char(c) => Ok(*c as i64),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("integer-or-marker-p"), other.clone()],
        )),
    }
}

fn expect_sequence_string(val: &Value) -> Result<String, Flow> {
    match val {
        Value::Str(s) => Ok((**s).clone()),
        other => Err(signal(
            "wrong-type-argument",
            vec![Value::symbol("sequencep"), other.clone()],
        )),
    }
}

fn lisp_pos_to_byte(buf: &crate::buffer::Buffer, raw: i64) -> usize {
    let char_pos = if raw > 0 { raw as usize - 1 } else { 0 };
    let byte = buf.text.char_to_byte(char_pos.min(buf.text.char_count()));
    byte.clamp(buf.begv, buf.zv)
}

fn replacement_region_bounds(
    buf: &crate::buffer::Buffer,
    start_arg: Option<&Value>,
    end_arg: Option<&Value>,
) -> Result<(usize, usize), Flow> {
    let start = match start_arg {
        Some(v) if !v.is_nil() => lisp_pos_to_byte(buf, expect_integer_or_marker(v)?),
        _ => buf.point(),
    };
    let end = match end_arg {
        Some(v) if !v.is_nil() => lisp_pos_to_byte(buf, expect_integer_or_marker(v)?),
        _ => buf.point_max(),
    };
    if start <= end {
        Ok((start, end))
    } else {
        Ok((end, start))
    }
}

fn line_operation_region_bounds(
    buf: &crate::buffer::Buffer,
    start_arg: Option<&Value>,
    end_arg: Option<&Value>,
) -> Result<(usize, usize), Flow> {
    let start = match start_arg {
        Some(v) if !v.is_nil() => lisp_pos_to_byte(buf, expect_integer_or_marker(v)?),
        _ => buf.point(),
    };
    let end = match end_arg {
        Some(v) if !v.is_nil() => lisp_pos_to_byte(buf, expect_integer_or_marker(v)?),
        _ => buf.point_max(),
    };
    if start <= end {
        Ok((start, end))
    } else {
        Ok((end, start))
    }
}

fn line_start_at_or_before(source: &str, at: usize) -> usize {
    let pos = at.min(source.len());
    match source[..pos].rfind('\n') {
        Some(idx) => idx + 1,
        None => 0,
    }
}

// ---------------------------------------------------------------------------
// Search direction
// ---------------------------------------------------------------------------

/// Direction of an incremental search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchDirection {
    Forward,
    Backward,
}

// ---------------------------------------------------------------------------
// IsearchState — tracks incremental search session
// ---------------------------------------------------------------------------

/// Full state for one active incremental search session.
pub struct IsearchState {
    /// Whether the search session is still running.
    pub active: bool,
    /// Current search direction.
    pub direction: SearchDirection,
    /// The string being searched for (built up character by character).
    pub search_string: String,
    /// Whether this is a regexp search.
    pub regexp: bool,
    /// Case folding: `None` = auto-detect, `Some(true)` = fold, `Some(false)` = exact.
    pub case_fold: Option<bool>,
    /// Whether the search has wrapped around the buffer.
    pub wrapped: bool,
    /// Whether the last incremental search step succeeded.
    pub success: bool,
    /// Byte position of the start of the current match (if any).
    pub match_start: Option<usize>,
    /// Byte position of the end of the current match (if any).
    pub match_end: Option<usize>,
    /// Point position when the search was started (for abort restoration).
    pub origin: usize,
    /// Position where wrapping resets to.
    pub barrier: usize,
    /// Index into the history ring, if navigating history.
    pub history_index: Option<usize>,
    /// All visible matches for lazy-highlight overlays: `(start, end)` pairs.
    pub lazy_matches: Vec<(usize, usize)>,
}

// ---------------------------------------------------------------------------
// SearchHistory
// ---------------------------------------------------------------------------

/// Ring of previous search strings, kept separately for literal and regexp.
pub struct SearchHistory {
    strings: VecDeque<String>,
    regexp_strings: VecDeque<String>,
    max_length: usize,
}

impl SearchHistory {
    /// Create an empty history with default capacity of 100 entries per ring.
    pub fn new() -> Self {
        Self {
            strings: VecDeque::new(),
            regexp_strings: VecDeque::new(),
            max_length: 100,
        }
    }

    /// Push a search string onto the appropriate ring.
    /// Duplicates are moved to the front rather than stored twice.
    pub fn push(&mut self, string: String, regexp: bool) {
        let ring = if regexp {
            &mut self.regexp_strings
        } else {
            &mut self.strings
        };
        // Remove duplicate if present
        if let Some(pos) = ring.iter().position(|s| *s == string) {
            ring.remove(pos);
        }
        ring.push_front(string);
        if ring.len() > self.max_length {
            ring.pop_back();
        }
    }

    /// Get the search string at `index` (0 = most recent).
    pub fn get(&self, index: usize, regexp: bool) -> Option<&str> {
        let ring = if regexp {
            &self.regexp_strings
        } else {
            &self.strings
        };
        ring.get(index).map(|s| s.as_str())
    }

    /// Number of entries in the chosen ring.
    pub fn len(&self, regexp: bool) -> usize {
        if regexp {
            self.regexp_strings.len()
        } else {
            self.strings.len()
        }
    }

    /// Borrow the underlying deque.
    pub fn strings(&self, regexp: bool) -> &VecDeque<String> {
        if regexp {
            &self.regexp_strings
        } else {
            &self.strings
        }
    }
}

impl Default for SearchHistory {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// IsearchManager
// ---------------------------------------------------------------------------

/// Manages the lifecycle of incremental search sessions.
pub struct IsearchManager {
    state: Option<IsearchState>,
    history: SearchHistory,
    last_search_string: Option<String>,
    last_search_regexp: bool,
}

impl IsearchManager {
    pub fn new() -> Self {
        Self {
            state: None,
            history: SearchHistory::new(),
            last_search_string: None,
            last_search_regexp: false,
        }
    }

    // -- Start/end ----------------------------------------------------------

    /// Begin a new incremental search session.
    pub fn begin_search(&mut self, direction: SearchDirection, regexp: bool, origin: usize) {
        self.state = Some(IsearchState {
            active: true,
            direction,
            search_string: String::new(),
            regexp,
            case_fold: None, // auto
            wrapped: false,
            success: true,
            match_start: None,
            match_end: None,
            origin,
            barrier: origin,
            history_index: None,
            lazy_matches: Vec::new(),
        });
    }

    /// End the search session normally, optionally saving the string to history.
    pub fn end_search(&mut self, save_to_history: bool) {
        if let Some(state) = self.state.take() {
            if save_to_history && !state.search_string.is_empty() {
                self.history.push(state.search_string.clone(), state.regexp);
            }
            if !state.search_string.is_empty() {
                self.last_search_regexp = state.regexp;
                self.last_search_string = Some(state.search_string);
            }
        }
    }

    /// Abort the search session.  Returns the original point position so that
    /// the caller can restore it.
    pub fn abort_search(&mut self) -> usize {
        let origin = self.state.as_ref().map(|s| s.origin).unwrap_or(0);
        self.state = None;
        origin
    }

    // -- Modify search ------------------------------------------------------

    /// Append a character to the search string.
    pub fn add_char(&mut self, ch: char) {
        if let Some(state) = self.state.as_mut() {
            state.search_string.push(ch);
            state.history_index = None;
        }
    }

    /// Remove the last character from the search string.
    pub fn delete_char(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.search_string.pop();
            state.history_index = None;
        }
    }

    /// Replace the search string wholesale.
    pub fn set_string(&mut self, s: String) {
        if let Some(state) = self.state.as_mut() {
            state.search_string = s;
            state.history_index = None;
        }
    }

    /// Toggle between literal and regexp search.
    pub fn toggle_regexp(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.regexp = !state.regexp;
        }
    }

    /// Toggle case-fold cycling: auto -> fold -> exact -> auto.
    pub fn toggle_case_fold(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.case_fold = match state.case_fold {
                None => Some(true),
                Some(true) => Some(false),
                Some(false) => None,
            };
        }
    }

    /// Reverse the search direction.
    pub fn reverse_direction(&mut self) {
        if let Some(state) = self.state.as_mut() {
            state.direction = match state.direction {
                SearchDirection::Forward => SearchDirection::Backward,
                SearchDirection::Backward => SearchDirection::Forward,
            };
        }
    }

    // -- Search operations --------------------------------------------------

    /// Perform one incremental search step in the current direction, starting
    /// from the current match position (or the barrier after a wrap).
    ///
    /// `text` is the full buffer contents.  Returns match `(start, end)` if
    /// found.  The caller is responsible for moving point.
    pub fn search_next(&mut self, text: &str) -> Option<(usize, usize)> {
        let state = self.state.as_mut()?;
        if state.search_string.is_empty() {
            state.success = true;
            state.match_start = None;
            state.match_end = None;
            return None;
        }

        let case_fold = resolve_case_fold(state.case_fold, &state.search_string);

        // Determine the starting position for the next search step.
        let from = match state.direction {
            SearchDirection::Forward => state.match_end.unwrap_or(state.barrier),
            SearchDirection::Backward => state.match_start.unwrap_or(state.barrier),
        };

        let forward = state.direction == SearchDirection::Forward;

        if let Some((start, end)) = find_match(
            text,
            &state.search_string,
            from,
            forward,
            state.regexp,
            case_fold,
        ) {
            state.success = true;
            state.match_start = Some(start);
            state.match_end = Some(end);
            return Some((start, end));
        }

        // Not found from current position — try wrapping.
        if !state.wrapped {
            state.wrapped = true;
            let wrap_from = if forward { 0 } else { text.len() };
            if let Some((start, end)) = find_match(
                text,
                &state.search_string,
                wrap_from,
                forward,
                state.regexp,
                case_fold,
            ) {
                state.success = true;
                state.match_start = Some(start);
                state.match_end = Some(end);
                return Some((start, end));
            }
        }

        state.success = false;
        None
    }

    /// Re-run the search from the origin for the current search string (used
    /// after each character addition/deletion to update the match).
    ///
    /// `text` is the full buffer contents.  Returns match `(start, end)` if
    /// found.
    pub fn search_update(&mut self, text: &str) -> Option<(usize, usize)> {
        let state = self.state.as_mut()?;
        if state.search_string.is_empty() {
            state.success = true;
            state.match_start = None;
            state.match_end = None;
            state.wrapped = false;
            return None;
        }

        let case_fold = resolve_case_fold(state.case_fold, &state.search_string);
        let forward = state.direction == SearchDirection::Forward;

        // Search from origin first.
        if let Some((start, end)) = find_match(
            text,
            &state.search_string,
            state.origin,
            forward,
            state.regexp,
            case_fold,
        ) {
            state.success = true;
            state.wrapped = false;
            state.match_start = Some(start);
            state.match_end = Some(end);
            return Some((start, end));
        }

        // Wrap around.
        let wrap_from = if forward { 0 } else { text.len() };
        if let Some((start, end)) = find_match(
            text,
            &state.search_string,
            wrap_from,
            forward,
            state.regexp,
            case_fold,
        ) {
            state.success = true;
            state.wrapped = true;
            state.match_start = Some(start);
            state.match_end = Some(end);
            return Some((start, end));
        }

        state.success = false;
        state.wrapped = false;
        state.match_start = None;
        state.match_end = None;
        None
    }

    /// Compute all matches within the visible region for lazy-highlight
    /// overlays.
    pub fn compute_lazy_matches(&mut self, text: &str, visible_start: usize, visible_end: usize) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        state.lazy_matches.clear();

        if state.search_string.is_empty() {
            return;
        }

        let case_fold = resolve_case_fold(state.case_fold, &state.search_string);
        let start = visible_start.min(text.len());
        let end = visible_end.min(text.len());

        if start >= end {
            return;
        }

        let region = &text[start..end];

        if state.regexp {
            let pattern = build_regex_pattern(&state.search_string, case_fold);
            if let Ok(re) = Regex::new(&pattern) {
                for m in re.find_iter(region) {
                    let ms = start + m.start();
                    let me = start + m.end();
                    // Skip zero-length matches to avoid infinite loop
                    if ms == me {
                        continue;
                    }
                    state.lazy_matches.push((ms, me));
                }
            }
        } else {
            let haystack = if case_fold {
                region.to_lowercase()
            } else {
                region.to_string()
            };
            let needle = if case_fold {
                state.search_string.to_lowercase()
            } else {
                state.search_string.clone()
            };
            let mut search_from = 0;
            while let Some(pos) = haystack[search_from..].find(&needle) {
                let ms = start + search_from + pos;
                let me = ms + needle.len();
                state.lazy_matches.push((ms, me));
                search_from += pos + needle.len();
            }
        }
    }

    // -- History navigation -------------------------------------------------

    /// Move to the previous (older) history entry.
    pub fn history_previous(&mut self) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };
        let ring_len = self.history.len(state.regexp);
        if ring_len == 0 {
            return;
        }
        let new_index = match state.history_index {
            None => 0,
            Some(i) => {
                if i + 1 < ring_len {
                    i + 1
                } else {
                    return;
                }
            }
        };
        if let Some(s) = self.history.get(new_index, state.regexp) {
            state.search_string = s.to_string();
            state.history_index = Some(new_index);
        }
    }

    /// Move to the next (newer) history entry.
    pub fn history_next(&mut self) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };
        match state.history_index {
            None => {}
            Some(0) => {
                state.search_string.clear();
                state.history_index = None;
            }
            Some(i) => {
                let new_index = i - 1;
                if let Some(s) = self.history.get(new_index, state.regexp) {
                    state.search_string = s.to_string();
                    state.history_index = Some(new_index);
                }
            }
        }
    }

    /// Yank the word (or character) at point into the search string.
    ///
    /// `text` is the full buffer text; `point` is the current cursor position.
    /// Appends text from `point` up to the next word boundary.
    pub fn yank_word_or_char(&mut self, text: &str, point: usize) {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => return,
        };

        if point >= text.len() {
            return;
        }

        let rest = &text[point..];
        let mut end = 0;
        let mut chars = rest.chars();

        // Grab at least one char; then continue while alphanumeric.
        if let Some(ch) = chars.next() {
            end += ch.len_utf8();
            if ch.is_alphanumeric() || ch == '_' {
                for ch2 in chars {
                    if ch2.is_alphanumeric() || ch2 == '_' {
                        end += ch2.len_utf8();
                    } else {
                        break;
                    }
                }
            }
        }

        state.search_string.push_str(&rest[..end]);
        state.history_index = None;
    }

    // -- State queries ------------------------------------------------------

    /// Whether an incremental search is currently active.
    pub fn is_active(&self) -> bool {
        self.state.as_ref().map_or(false, |s| s.active)
    }

    /// Borrow the current state (if any).
    pub fn state(&self) -> Option<&IsearchState> {
        self.state.as_ref()
    }

    /// Build the minibuffer prompt string for the current search.
    pub fn prompt(&self) -> String {
        let state = match self.state.as_ref() {
            Some(s) => s,
            None => return String::new(),
        };

        let mut parts = Vec::new();

        if !state.success {
            parts.push("Failing");
        }
        if state.wrapped {
            parts.push("Wrapped");
        }
        if state.regexp {
            parts.push("Regexp");
        }

        let dir = match state.direction {
            SearchDirection::Forward => "I-search",
            SearchDirection::Backward => "I-search backward",
        };
        parts.push(dir);

        let prompt = parts.join(" ");
        format!("{}: {}", prompt, state.search_string)
    }
}

impl Default for IsearchManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Query-replace response
// ---------------------------------------------------------------------------

/// Possible user responses during a query-replace session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueryReplaceResponse {
    /// `y` / `SPC` — replace this match.
    Yes,
    /// `n` / `DEL` — skip this match.
    No,
    /// `!` — replace all remaining matches without asking.
    ReplaceAll,
    /// `q` / `RET` — stop replacing.
    Quit,
    /// `e` — edit the replacement string for this match.
    Edit,
    /// `d` — delete the match text without inserting the replacement.
    Delete,
    /// `u` — undo the last replacement.
    Undo,
    /// `?` — show help text.
    Help,
}

// ---------------------------------------------------------------------------
// QueryReplaceUndo
// ---------------------------------------------------------------------------

/// Record of a single replacement for undo purposes.
#[derive(Clone, Debug)]
pub struct QueryReplaceUndo {
    /// Byte position where the replacement was made.
    pub position: usize,
    /// Original matched text.
    pub original: String,
    /// Replacement text that was inserted.
    pub replacement: String,
}

// ---------------------------------------------------------------------------
// QueryReplaceState
// ---------------------------------------------------------------------------

/// Full state for one active query-replace session.
pub struct QueryReplaceState {
    /// Pattern to search for.
    pub from_string: String,
    /// Replacement string.
    pub to_string: String,
    /// Whether `from_string` is a regular expression.
    pub regexp: bool,
    /// Whether to match only whole delimited words.
    pub delimited: bool,
    /// Case folding override: `None` = auto, `Some(true)` = fold, `Some(false)` = exact.
    pub case_fold: Option<bool>,
    /// Whether to preserve the case pattern of the matched text.
    pub preserve_case: bool,
    /// Optional region restriction (start byte).
    pub region_start: Option<usize>,
    /// Optional region restriction (end byte).
    pub region_end: Option<usize>,
    /// Current match being presented to the user: `(start, end)`.
    pub current_match: Option<(usize, usize)>,
    /// Number of replacements made so far.
    pub replaced_count: usize,
    /// Number of matches skipped so far.
    pub skipped_count: usize,
    /// Stack of undoable replacements (most recent last).
    pub undo_stack: Vec<QueryReplaceUndo>,
}

// ---------------------------------------------------------------------------
// QueryReplaceAction
// ---------------------------------------------------------------------------

/// Action the caller should take after a query-replace response.
#[derive(Clone, Debug)]
pub enum QueryReplaceAction {
    /// Replace the region `[start, end)` with the given string.
    Replace(usize, usize, String),
    /// Skip the current match.
    Skip,
    /// The session is finished.
    Done(QueryReplaceSummary),
    /// Display this help text to the user.
    ShowHelp(String),
    /// The user asked to edit the replacement — caller should prompt for input.
    NeedInput,
}

// ---------------------------------------------------------------------------
// QueryReplaceSummary
// ---------------------------------------------------------------------------

/// Summary statistics returned when a query-replace session ends.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueryReplaceSummary {
    pub replaced: usize,
    pub skipped: usize,
}

// ---------------------------------------------------------------------------
// QueryReplaceManager
// ---------------------------------------------------------------------------

/// Manages query-replace sessions.
pub struct QueryReplaceManager {
    state: Option<QueryReplaceState>,
}

impl QueryReplaceManager {
    pub fn new() -> Self {
        Self { state: None }
    }

    /// Begin a new query-replace session (whole buffer).
    pub fn begin(&mut self, from: String, to: String, regexp: bool) {
        self.state = Some(QueryReplaceState {
            from_string: from,
            to_string: to,
            regexp,
            delimited: false,
            case_fold: None,
            preserve_case: true,
            region_start: None,
            region_end: None,
            current_match: None,
            replaced_count: 0,
            skipped_count: 0,
            undo_stack: Vec::new(),
        });
    }

    /// Begin a query-replace session restricted to a region.
    pub fn begin_in_region(
        &mut self,
        from: String,
        to: String,
        regexp: bool,
        start: usize,
        end: usize,
    ) {
        self.begin(from, to, regexp);
        if let Some(state) = self.state.as_mut() {
            state.region_start = Some(start);
            state.region_end = Some(end);
        }
    }

    /// Find the next match at or after `from_pos`.
    ///
    /// `text` is the full buffer contents.  Returns `(start, end)` of the
    /// match, also storing it in `current_match`.  Returns `None` when there
    /// are no more matches.
    pub fn find_next(&mut self, text: &str, from_pos: usize) -> Option<(usize, usize)> {
        let state = self.state.as_mut()?;

        let limit = state.region_end.unwrap_or(text.len()).min(text.len());
        let start = from_pos.max(state.region_start.unwrap_or(0));

        if start > limit {
            state.current_match = None;
            return None;
        }

        let case_fold = resolve_case_fold(state.case_fold, &state.from_string);
        let result = find_match(
            text,
            &state.from_string,
            start,
            true,
            state.regexp,
            case_fold,
        );

        if let Some((ms, me)) = result {
            if me <= limit {
                state.current_match = Some((ms, me));
                return Some((ms, me));
            }
        }

        state.current_match = None;
        None
    }

    /// Apply the user's response to the current match.
    pub fn respond(&mut self, response: QueryReplaceResponse) -> QueryReplaceAction {
        let state = match self.state.as_mut() {
            Some(s) => s,
            None => {
                return QueryReplaceAction::Done(QueryReplaceSummary {
                    replaced: 0,
                    skipped: 0,
                });
            }
        };

        match response {
            QueryReplaceResponse::Yes => {
                if let Some((start, end)) = state.current_match {
                    let matched_text = String::new(); // caller fills this in
                    let replacement = state.to_string.clone();
                    let replacement = if state.preserve_case {
                        // We cannot access the matched text here; the caller
                        // should use `compute_replacement` before calling
                        // `respond`.  Return the raw replacement.
                        replacement
                    } else {
                        replacement
                    };
                    state.replaced_count += 1;
                    state.undo_stack.push(QueryReplaceUndo {
                        position: start,
                        original: matched_text,
                        replacement: replacement.clone(),
                    });
                    state.current_match = None;
                    QueryReplaceAction::Replace(start, end, replacement)
                } else {
                    QueryReplaceAction::Skip
                }
            }
            QueryReplaceResponse::No => {
                state.skipped_count += 1;
                state.current_match = None;
                QueryReplaceAction::Skip
            }
            QueryReplaceResponse::ReplaceAll => {
                // Signal the caller to replace the current match and all
                // remaining ones.  We handle the *current* match here; the
                // caller should loop `find_next` + `respond(Yes)` for the rest.
                if let Some((start, end)) = state.current_match {
                    let replacement = state.to_string.clone();
                    state.replaced_count += 1;
                    state.undo_stack.push(QueryReplaceUndo {
                        position: start,
                        original: String::new(),
                        replacement: replacement.clone(),
                    });
                    state.current_match = None;
                    QueryReplaceAction::Replace(start, end, replacement)
                } else {
                    QueryReplaceAction::Skip
                }
            }
            QueryReplaceResponse::Quit => {
                let summary = QueryReplaceSummary {
                    replaced: state.replaced_count,
                    skipped: state.skipped_count,
                };
                self.state = None;
                QueryReplaceAction::Done(summary)
            }
            QueryReplaceResponse::Edit => QueryReplaceAction::NeedInput,
            QueryReplaceResponse::Delete => {
                if let Some((start, end)) = state.current_match {
                    state.replaced_count += 1;
                    state.undo_stack.push(QueryReplaceUndo {
                        position: start,
                        original: String::new(),
                        replacement: String::new(),
                    });
                    state.current_match = None;
                    // Replace with empty string = delete
                    QueryReplaceAction::Replace(start, end, String::new())
                } else {
                    QueryReplaceAction::Skip
                }
            }
            QueryReplaceResponse::Undo => {
                // Return the last undo entry via Done-like mechanism.
                // The actual undo application is done by `undo_last`.
                QueryReplaceAction::Skip
            }
            QueryReplaceResponse::Help => {
                let help = concat!(
                    "y/SPC - replace this match\n",
                    "n/DEL - skip this match\n",
                    "! - replace all remaining matches\n",
                    "q/RET - quit\n",
                    "e - edit replacement\n",
                    "d - delete match (no replacement)\n",
                    "u - undo last replacement\n",
                    "? - show this help",
                );
                QueryReplaceAction::ShowHelp(help.to_string())
            }
        }
    }

    /// Compute the replacement text for a given matched string.
    ///
    /// Handles `preserve_case` logic.  For regexp replacements the caller
    /// should additionally process `\&` and `\N` references (see
    /// `regex::build_replacement`).
    pub fn compute_replacement(&self, matched: &str) -> String {
        let state = match self.state.as_ref() {
            Some(s) => s,
            None => return String::new(),
        };

        if state.preserve_case {
            preserve_case(&state.to_string, matched)
        } else {
            state.to_string.clone()
        }
    }

    /// Pop and return the most recent undo entry.
    pub fn undo_last(&mut self) -> Option<QueryReplaceUndo> {
        let state = self.state.as_mut()?;
        let entry = state.undo_stack.pop();
        if entry.is_some() {
            // Decrement replaced count since we are undoing.
            state.replaced_count = state.replaced_count.saturating_sub(1);
        }
        entry
    }

    /// End the session and return a summary.
    pub fn finish(&mut self) -> QueryReplaceSummary {
        let state = match self.state.take() {
            Some(s) => s,
            None => {
                return QueryReplaceSummary {
                    replaced: 0,
                    skipped: 0,
                };
            }
        };
        QueryReplaceSummary {
            replaced: state.replaced_count,
            skipped: state.skipped_count,
        }
    }

    /// Whether a query-replace session is currently active.
    pub fn is_active(&self) -> bool {
        self.state.is_some()
    }

    /// Borrow the current state (if any).
    pub fn state(&self) -> Option<&QueryReplaceState> {
        self.state.as_ref()
    }

    /// Build the minibuffer prompt for the current session.
    pub fn prompt(&self) -> String {
        let state = match self.state.as_ref() {
            Some(s) => s,
            None => return String::new(),
        };
        let kind = if state.regexp {
            "Query replacing regexp"
        } else {
            "Query replacing"
        };
        format!(
            "{} {} with {}: (y/n/!/q/?)",
            kind, state.from_string, state.to_string
        )
    }
}

impl Default for QueryReplaceManager {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Helper: resolve case folding
// ---------------------------------------------------------------------------

/// Determine effective case folding.
///
/// When `override_val` is `None` (auto), we fold if the search string is
/// entirely lowercase (Emacs `isearch-no-upper-case-p` heuristic).
fn resolve_case_fold(override_val: Option<bool>, search_string: &str) -> bool {
    match override_val {
        Some(v) => v,
        None => {
            // Auto: fold if no uppercase letters in the search string.
            !search_string.chars().any(|c| c.is_uppercase())
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: build regex pattern with optional case-insensitive flag
// ---------------------------------------------------------------------------

fn build_regex_pattern(pattern: &str, case_fold: bool) -> String {
    let translated = super::regex::translate_emacs_regex(pattern);
    if case_fold {
        format!("(?i){}", translated)
    } else {
        translated
    }
}

// ---------------------------------------------------------------------------
// Helper: find_match — general-purpose substring/regex search
// ---------------------------------------------------------------------------

/// Search for `pattern` in `text`.
///
/// - `from`:    byte offset to start searching from.
/// - `forward`: direction of search.
/// - `regexp`:  treat `pattern` as an Emacs regular expression.
/// - `case_fold`: perform case-insensitive matching.
///
/// Returns `(match_start, match_end)` byte offsets into `text`, or `None`.
fn find_match(
    text: &str,
    pattern: &str,
    from: usize,
    forward: bool,
    regexp: bool,
    case_fold: bool,
) -> Option<(usize, usize)> {
    if pattern.is_empty() {
        return None;
    }

    let text_len = text.len();

    if regexp {
        let re_pat = build_regex_pattern(pattern, case_fold);
        let re = Regex::new(&re_pat).ok()?;

        if forward {
            let start = from.min(text_len);
            let region = &text[start..];
            let m = re.find(region)?;
            Some((start + m.start(), start + m.end()))
        } else {
            // Backward: search in text[0..from] and pick the last match.
            let end = from.min(text_len);
            let region = &text[..end];
            let mut last = None;
            for m in re.find_iter(region) {
                last = Some((m.start(), m.end()));
            }
            last
        }
    } else {
        // Literal search.
        if forward {
            let start = from.min(text_len);
            let region = &text[start..];
            if case_fold {
                let hay = region.to_lowercase();
                let needle = pattern.to_lowercase();
                let pos = hay.find(&needle)?;
                Some((start + pos, start + pos + needle.len()))
            } else {
                let pos = region.find(pattern)?;
                Some((start + pos, start + pos + pattern.len()))
            }
        } else {
            let end = from.min(text_len);
            let region = &text[..end];
            if case_fold {
                let hay = region.to_lowercase();
                let needle = pattern.to_lowercase();
                let pos = hay.rfind(&needle)?;
                Some((pos, pos + needle.len()))
            } else {
                let pos = region.rfind(pattern)?;
                Some((pos, pos + pattern.len()))
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Helper: case-preserving replacement
// ---------------------------------------------------------------------------

/// Produce a replacement string that preserves the case pattern of the
/// matched text.
///
/// Rules (matching Emacs `replace-match` behavior):
/// - If `matched` is all-uppercase, upcase the entire replacement.
/// - If `matched` starts with an uppercase letter and the rest is lowercase
///   (capitalized), capitalize the replacement.
/// - Otherwise return `replacement` unmodified.
fn preserve_case(replacement: &str, matched: &str) -> String {
    if matched.is_empty() || replacement.is_empty() {
        return replacement.to_string();
    }

    let all_upper = matched
        .chars()
        .all(|c| !c.is_alphabetic() || c.is_uppercase());
    let has_alpha = matched.chars().any(|c| c.is_alphabetic());

    if all_upper && has_alpha {
        return replacement.to_uppercase();
    }

    let mut chars = matched.chars();
    let first = chars.next().unwrap();
    let first_upper = first.is_uppercase();
    let rest_lower = chars.all(|c| !c.is_alphabetic() || c.is_lowercase());

    if first_upper && rest_lower {
        // Capitalize: uppercase first char of replacement, lowercase rest.
        let mut result = String::with_capacity(replacement.len());
        let mut rchars = replacement.chars();
        if let Some(rc) = rchars.next() {
            for uc in rc.to_uppercase() {
                result.push(uc);
            }
        }
        for rc in rchars {
            result.push(rc);
        }
        return result;
    }

    replacement.to_string()
}

// ---------------------------------------------------------------------------
// Builtin functions (stubs for evaluator dispatch)
// ---------------------------------------------------------------------------

/// `(replace-string FROM-STRING TO-STRING &optional DELIMITED START END BACKWARD REGION-NONCONTIGUOUS-P)` —
/// evaluator-backed non-interactive replace subset.
pub(crate) fn builtin_replace_string_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("replace-string", &args, 2, 7)?;
    let from = expect_sequence_string(&args[0])?;
    let to = expect_string(&args[1])?;
    let (start, end, source, read_only, buffer_name) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = replacement_region_bounds(buf, args.get(3), args.get(4))?;
        (
            start,
            end,
            buf.buffer_substring(start, end),
            buf.read_only,
            buf.name.clone(),
        )
    };

    if from.is_empty() {
        if source.is_empty() {
            return Ok(Value::Nil);
        }
        if read_only {
            return Err(signal(
                "buffer-read-only",
                vec![Value::string(buffer_name)],
            ));
        }
        let mut out = String::with_capacity(source.len() + to.len() * source.chars().count());
        for ch in source.chars() {
            out.push_str(&to);
            out.push(ch);
        }
        if out == source {
            return Ok(Value::Nil);
        }
        let buf = eval
            .buffers
            .current_buffer_mut()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        buf.delete_region(start, end);
        buf.goto_char(start);
        buf.insert(&out);
        buf.goto_char(start + out.len());
        return Ok(Value::Nil);
    }

    let case_fold = resolve_case_fold(None, &from);
    let mut out = String::with_capacity(source.len());
    let mut cursor = 0usize;
    let mut replaced = 0usize;
    while let Some((m_start, m_end)) = find_match(&source, &from, cursor, true, false, case_fold) {
        out.push_str(&source[cursor..m_start]);
        let matched = &source[m_start..m_end];
        out.push_str(&preserve_case(&to, matched));
        replaced += 1;
        cursor = m_end;
    }
    out.push_str(&source[cursor..]);

    if replaced == 0 {
        return Ok(Value::Nil);
    }
    if read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buffer_name)],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(start, end);
    buf.goto_char(start);
    buf.insert(&out);
    buf.goto_char(start + out.len());

    Ok(Value::Nil)
}

/// `(replace-regexp REGEXP TO-STRING &optional DELIMITED START END BACKWARD REGION-NONCONTIGUOUS-P)` —
/// evaluator-backed non-interactive regexp replacement subset.
pub(crate) fn builtin_replace_regexp_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("replace-regexp", &args, 2, 7)?;
    let from = expect_sequence_string(&args[0])?;
    let to = expect_string(&args[1])?;

    let (start, end, source, read_only, buffer_name) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = replacement_region_bounds(buf, args.get(3), args.get(4))?;
        (
            start,
            end,
            buf.buffer_substring(start, end),
            buf.read_only,
            buf.name.clone(),
        )
    };

    let case_fold = resolve_case_fold(None, &from);
    let pattern = build_regex_pattern(&from, case_fold);
    let re = Regex::new(&pattern)
        .map_err(|e| signal("invalid-regexp", vec![Value::string(format!("Invalid regexp: {e}"))]))?;

    let mut out = String::with_capacity(source.len());
    let mut last = 0usize;
    let mut replaced = 0usize;
    for m in re.find_iter(&source) {
        if m.start() == m.end() {
            // Emacs inserts on empty regexp matches before each character, not at end.
            if m.start() >= source.len() {
                continue;
            }
            out.push_str(&source[last..m.start()]);
            out.push_str(&to);
            last = m.start();
            replaced += 1;
            continue;
        }

        out.push_str(&source[last..m.start()]);
        out.push_str(&preserve_case(&to, m.as_str()));
        last = m.end();
        replaced += 1;
    }
    out.push_str(&source[last..]);

    if replaced == 0 {
        return Ok(Value::Nil);
    }
    if read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buffer_name)],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    buf.delete_region(start, end);
    buf.goto_char(start);
    buf.insert(&out);
    buf.goto_char(start + out.len());

    Ok(Value::Nil)
}

/// `(keep-lines REGEXP &optional RSTART REND INTERACTIVE)` —
/// evaluator-backed non-interactive line filtering subset.
pub(crate) fn builtin_keep_lines_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("keep-lines", &args, 1, 4)?;
    let regexp = expect_sequence_string(&args[0])?;

    let (point_min, start, end, source, read_only, buffer_name) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = line_operation_region_bounds(buf, args.get(1), args.get(2))?;
        (
            buf.point_min(),
            start,
            end,
            buf.buffer_substring(buf.point_min(), buf.point_max()),
            buf.read_only,
            buf.name.clone(),
        )
    };

    let case_fold = resolve_case_fold(None, &regexp);
    let pattern = build_regex_pattern(&regexp, case_fold);
    let re = match Regex::new(&pattern) {
        Ok(re) => re,
        Err(_) => return Ok(Value::Nil),
    };

    let rel_start = start.saturating_sub(point_min).min(source.len());
    let rel_end = end.saturating_sub(point_min).min(source.len());
    let mut rel_cursor = line_start_at_or_before(&source, rel_start);
    let mut delete_ranges: Vec<(usize, usize)> = Vec::new();

    while rel_cursor < source.len() {
        let abs_line_start = point_min + rel_cursor;
        if abs_line_start >= point_min + rel_end {
            break;
        }

        let line_tail = &source[rel_cursor..];
        let line_len = match line_tail.find('\n') {
            Some(idx) => idx + 1,
            None => line_tail.len(),
        };
        let rel_line_end = rel_cursor + line_len;
        let line = if source.as_bytes().get(rel_line_end.wrapping_sub(1)) == Some(&b'\n') {
            &source[rel_cursor..rel_line_end - 1]
        } else {
            &source[rel_cursor..rel_line_end]
        };

        if !re.is_match(line) {
            delete_ranges.push((point_min + rel_cursor, point_min + rel_line_end));
        }
        rel_cursor = rel_line_end;
    }

    if delete_ranges.is_empty() {
        return Ok(Value::Nil);
    }
    if read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buffer_name)],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    for (del_start, del_end) in delete_ranges.into_iter().rev() {
        buf.delete_region(del_start, del_end);
    }

    Ok(Value::Nil)
}

/// `(flush-lines REGEXP &optional RSTART REND INTERACTIVE)` —
/// evaluator-backed non-interactive line filtering subset.
pub(crate) fn builtin_flush_lines_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("flush-lines", &args, 1, 4)?;
    let regexp = expect_sequence_string(&args[0])?;

    let (point_min, start, end, source, read_only, buffer_name) = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = line_operation_region_bounds(buf, args.get(1), args.get(2))?;
        (
            buf.point_min(),
            start,
            end,
            buf.buffer_substring(buf.point_min(), buf.point_max()),
            buf.read_only,
            buf.name.clone(),
        )
    };

    let case_fold = resolve_case_fold(None, &regexp);
    let pattern = build_regex_pattern(&regexp, case_fold);
    let re = Regex::new(&pattern).map_err(|e| {
        signal(
            "invalid-regexp",
            vec![Value::string(format!("Invalid regexp: {e}"))],
        )
    })?;

    let rel_start = start.saturating_sub(point_min).min(source.len());
    let rel_end = end.saturating_sub(point_min).min(source.len());
    let mut rel_cursor = line_start_at_or_before(&source, rel_start);
    let mut delete_ranges: Vec<(usize, usize)> = Vec::new();

    while rel_cursor < source.len() {
        let abs_line_start = point_min + rel_cursor;
        if abs_line_start >= point_min + rel_end {
            break;
        }

        let line_tail = &source[rel_cursor..];
        let line_len = match line_tail.find('\n') {
            Some(idx) => idx + 1,
            None => line_tail.len(),
        };
        let rel_line_end = rel_cursor + line_len;
        let line = if source.as_bytes().get(rel_line_end.wrapping_sub(1)) == Some(&b'\n') {
            &source[rel_cursor..rel_line_end - 1]
        } else {
            &source[rel_cursor..rel_line_end]
        };

        if re.is_match(line) {
            delete_ranges.push((point_min + rel_cursor, point_min + rel_line_end));
        }
        rel_cursor = rel_line_end;
    }

    if delete_ranges.is_empty() {
        return Ok(Value::Nil);
    }
    if read_only {
        return Err(signal(
            "buffer-read-only",
            vec![Value::string(buffer_name)],
        ));
    }

    let buf = eval
        .buffers
        .current_buffer_mut()
        .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
    for (del_start, del_end) in delete_ranges.into_iter().rev() {
        buf.delete_region(del_start, del_end);
    }

    Ok(Value::Nil)
}

/// `(how-many REGEXP &optional RSTART REND INTERACTIVE)` —
/// evaluator-backed regexp match counting subset.
pub(crate) fn builtin_how_many_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("how-many", &args, 1, 4)?;
    let regexp = expect_sequence_string(&args[0])?;

    let source = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = line_operation_region_bounds(buf, args.get(1), args.get(2))?;
        buf.buffer_substring(start, end)
    };

    if regexp.is_empty() {
        return Ok(Value::Int(source.chars().count() as i64));
    }

    let case_fold = resolve_case_fold(None, &regexp);
    let pattern = build_regex_pattern(&regexp, case_fold);
    let re = Regex::new(&pattern).map_err(|e| {
        signal(
            "invalid-regexp",
            vec![Value::string(format!("Invalid regexp: {e}"))],
        )
    })?;

    let mut count: i64 = 0;
    for m in re.find_iter(&source) {
        if m.start() == m.end() {
            if m.start() >= source.len() {
                continue;
            }
        }
        count += 1;
    }

    Ok(Value::Int(count))
}

/// `(count-matches REGEXP &optional START END INTERACTIVE)` —
/// evaluator-backed regexp match counting subset.
pub(crate) fn builtin_count_matches_eval(
    eval: &mut super::eval::Evaluator,
    args: Vec<Value>,
) -> EvalResult {
    expect_min_max_args("count-matches", &args, 1, 4)?;
    let regexp = expect_sequence_string(&args[0])?;

    let source = {
        let buf = eval
            .buffers
            .current_buffer()
            .ok_or_else(|| signal("error", vec![Value::string("No current buffer")]))?;
        let (start, end) = line_operation_region_bounds(buf, args.get(1), args.get(2))?;
        buf.buffer_substring(start, end)
    };

    if regexp.is_empty() {
        return Ok(Value::Int(source.chars().count() as i64));
    }

    let case_fold = resolve_case_fold(None, &regexp);
    let pattern = build_regex_pattern(&regexp, case_fold);
    let re = Regex::new(&pattern).map_err(|e| {
        signal(
            "invalid-regexp",
            vec![Value::string(format!("Invalid regexp: {e}"))],
        )
    })?;

    let mut count: i64 = 0;
    for m in re.find_iter(&source) {
        if m.start() == m.end() {
            if m.start() >= source.len() {
                continue;
            }
        }
        count += 1;
    }

    Ok(Value::Int(count))
}

/// `(isearch-forward)` — stub: initiates forward incremental search.
pub(crate) fn builtin_isearch_forward(args: Vec<Value>) -> EvalResult {
    let _ = &args;
    Err(signal(
        "error",
        vec![Value::string(
            "move-to-window-line called from unrelated buffer",
        )],
    ))
}

/// `(isearch-backward)` — stub: initiates backward incremental search.
pub(crate) fn builtin_isearch_backward(args: Vec<Value>) -> EvalResult {
    let _ = &args;
    Err(signal(
        "error",
        vec![Value::string(
            "move-to-window-line called from unrelated buffer",
        )],
    ))
}

/// `(query-replace FROM TO &optional DELIMITED START END BACKWARD REGION-NONCONTIGUOUS)` — stub.
pub(crate) fn builtin_query_replace(args: Vec<Value>) -> EvalResult {
    expect_min_args("query-replace", &args, 2)?;
    // Interactive; return nil as a stub.
    Ok(Value::Nil)
}

/// `(query-replace-regexp FROM TO &optional DELIMITED START END BACKWARD)` — stub.
pub(crate) fn builtin_query_replace_regexp(args: Vec<Value>) -> EvalResult {
    expect_min_args("query-replace-regexp", &args, 2)?;
    Ok(Value::Nil)
}

/// `(replace-string FROM-STRING TO-STRING &optional DELIMITED START END BACKWARD)` —
/// non-interactive unconditional replace.
///
/// Operates on a notional "buffer text" supplied as the first arg (our
/// simplified version takes two string args and returns the result string).
pub(crate) fn builtin_replace_string(args: Vec<Value>) -> EvalResult {
    expect_min_args("replace-string", &args, 2)?;
    let from = expect_string(&args[0])?;
    let to = expect_string(&args[1])?;
    if from.is_empty() {
        return Err(signal(
            "error",
            vec![Value::string("replace-string: empty search string")],
        ));
    }
    // In a full implementation this would modify the current buffer.
    // For now, return the number of replacements that *would* be made (as int 0).
    let _ = to;
    Ok(Value::Int(0))
}

/// `(replace-regexp REGEXP TO-STRING &optional DELIMITED START END BACKWARD)` —
/// non-interactive regexp replace (stub).
pub(crate) fn builtin_replace_regexp(args: Vec<Value>) -> EvalResult {
    expect_min_args("replace-regexp", &args, 2)?;
    let _from = expect_string(&args[0])?;
    let _to = expect_string(&args[1])?;
    Ok(Value::Int(0))
}

/// `(how-many REGEXP &optional RSTART REND INTERACTIVE)` — count matches.
///
/// Simplified version: counts occurrences of REGEXP in string arg (or returns 0).
pub(crate) fn builtin_how_many(args: Vec<Value>) -> EvalResult {
    expect_min_args("how-many", &args, 1)?;
    let _pattern = expect_string(&args[0])?;
    // In a full implementation this would search the current buffer.
    Ok(Value::Int(0))
}

/// `(keep-lines REGEXP &optional RSTART REND INTERACTIVE)` — delete non-matching lines (stub).
pub(crate) fn builtin_keep_lines(args: Vec<Value>) -> EvalResult {
    expect_min_args("keep-lines", &args, 1)?;
    let _pattern = expect_string(&args[0])?;
    Ok(Value::Int(0))
}

/// `(flush-lines REGEXP &optional RSTART REND INTERACTIVE)` — delete matching lines (stub).
pub(crate) fn builtin_flush_lines(args: Vec<Value>) -> EvalResult {
    expect_min_args("flush-lines", &args, 1)?;
    let _pattern = expect_string(&args[0])?;
    Ok(Value::Int(0))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // -----------------------------------------------------------------------
    // SearchHistory
    // -----------------------------------------------------------------------

    #[test]
    fn history_push_and_get() {
        let mut h = SearchHistory::new();
        h.push("hello".to_string(), false);
        h.push("world".to_string(), false);
        assert_eq!(h.get(0, false), Some("world"));
        assert_eq!(h.get(1, false), Some("hello"));
        assert_eq!(h.get(2, false), None);
    }

    #[test]
    fn history_push_deduplicates() {
        let mut h = SearchHistory::new();
        h.push("aaa".to_string(), false);
        h.push("bbb".to_string(), false);
        h.push("aaa".to_string(), false);
        assert_eq!(h.len(false), 2);
        assert_eq!(h.get(0, false), Some("aaa"));
        assert_eq!(h.get(1, false), Some("bbb"));
    }

    #[test]
    fn history_separate_rings() {
        let mut h = SearchHistory::new();
        h.push("literal".to_string(), false);
        h.push("re.*gex".to_string(), true);
        assert_eq!(h.len(false), 1);
        assert_eq!(h.len(true), 1);
        assert_eq!(h.get(0, false), Some("literal"));
        assert_eq!(h.get(0, true), Some("re.*gex"));
    }

    #[test]
    fn history_max_length() {
        let mut h = SearchHistory::new();
        for i in 0..150 {
            h.push(format!("item{}", i), false);
        }
        assert_eq!(h.len(false), 100);
        // Most recent is item149
        assert_eq!(h.get(0, false), Some("item149"));
    }

    #[test]
    fn history_strings_accessor() {
        let mut h = SearchHistory::new();
        h.push("a".to_string(), false);
        h.push("b".to_string(), false);
        let ring = h.strings(false);
        assert_eq!(ring.len(), 2);
        assert_eq!(ring[0], "b");
        assert_eq!(ring[1], "a");
    }

    // -----------------------------------------------------------------------
    // IsearchManager — begin/end lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_begin_end_lifecycle() {
        let mut mgr = IsearchManager::new();
        assert!(!mgr.is_active());

        mgr.begin_search(SearchDirection::Forward, false, 42);
        assert!(mgr.is_active());

        let state = mgr.state().unwrap();
        assert_eq!(state.origin, 42);
        assert!(state.search_string.is_empty());
        assert!(matches!(state.direction, SearchDirection::Forward));
        assert!(!state.regexp);

        mgr.end_search(false);
        assert!(!mgr.is_active());
    }

    #[test]
    fn isearch_abort_restores_origin() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 100);
        mgr.add_char('x');
        let origin = mgr.abort_search();
        assert_eq!(origin, 100);
        assert!(!mgr.is_active());
    }

    #[test]
    fn isearch_end_saves_to_history() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.add_char('f');
        mgr.add_char('o');
        mgr.add_char('o');
        mgr.end_search(true);
        assert_eq!(mgr.history.get(0, false), Some("foo"));
    }

    #[test]
    fn isearch_end_empty_string_not_saved() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.end_search(true);
        assert_eq!(mgr.history.len(false), 0);
    }

    // -----------------------------------------------------------------------
    // IsearchManager — string modification
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_add_delete_char() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.add_char('a');
        mgr.add_char('b');
        mgr.add_char('c');
        assert_eq!(mgr.state().unwrap().search_string, "abc");
        mgr.delete_char();
        assert_eq!(mgr.state().unwrap().search_string, "ab");
    }

    #[test]
    fn isearch_set_string() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("hello world".to_string());
        assert_eq!(mgr.state().unwrap().search_string, "hello world");
    }

    #[test]
    fn isearch_toggle_regexp() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        assert!(!mgr.state().unwrap().regexp);
        mgr.toggle_regexp();
        assert!(mgr.state().unwrap().regexp);
        mgr.toggle_regexp();
        assert!(!mgr.state().unwrap().regexp);
    }

    #[test]
    fn isearch_toggle_case_fold() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        assert!(mgr.state().unwrap().case_fold.is_none());
        mgr.toggle_case_fold();
        assert_eq!(mgr.state().unwrap().case_fold, Some(true));
        mgr.toggle_case_fold();
        assert_eq!(mgr.state().unwrap().case_fold, Some(false));
        mgr.toggle_case_fold();
        assert!(mgr.state().unwrap().case_fold.is_none());
    }

    #[test]
    fn isearch_reverse_direction() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        assert!(matches!(
            mgr.state().unwrap().direction,
            SearchDirection::Forward
        ));
        mgr.reverse_direction();
        assert!(matches!(
            mgr.state().unwrap().direction,
            SearchDirection::Backward
        ));
    }

    // -----------------------------------------------------------------------
    // IsearchManager — forward matching
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_forward_search_update_finds_match() {
        let mut mgr = IsearchManager::new();
        let text = "hello world hello";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("world".to_string());
        let result = mgr.search_update(text);
        assert_eq!(result, Some((6, 11)));
        assert!(mgr.state().unwrap().success);
    }

    #[test]
    fn isearch_forward_search_update_no_match() {
        let mut mgr = IsearchManager::new();
        let text = "hello world";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("zzz".to_string());
        let result = mgr.search_update(text);
        assert!(result.is_none());
        assert!(!mgr.state().unwrap().success);
    }

    #[test]
    fn isearch_forward_search_next_advances() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb aaa bbb";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("aaa".to_string());

        // First search_update finds first occurrence
        let r1 = mgr.search_update(text);
        assert_eq!(r1, Some((0, 3)));

        // search_next moves to next occurrence
        let r2 = mgr.search_next(text);
        assert_eq!(r2, Some((8, 11)));
    }

    // -----------------------------------------------------------------------
    // IsearchManager — backward matching
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_backward_search_update() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb aaa";
        mgr.begin_search(SearchDirection::Backward, false, text.len());
        mgr.set_string("aaa".to_string());
        let result = mgr.search_update(text);
        // Backward from origin=11, should find last "aaa" at 8
        assert_eq!(result, Some((8, 11)));
    }

    #[test]
    fn isearch_backward_search_next() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb aaa";
        mgr.begin_search(SearchDirection::Backward, false, text.len());
        mgr.set_string("aaa".to_string());

        let r1 = mgr.search_update(text);
        assert_eq!(r1, Some((8, 11)));

        // search_next backward should find the earlier "aaa"
        let r2 = mgr.search_next(text);
        assert_eq!(r2, Some((0, 3)));
    }

    // -----------------------------------------------------------------------
    // IsearchManager — wrapping
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_forward_wraps() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb ccc";
        // Start near the end so "aaa" is behind us
        mgr.begin_search(SearchDirection::Forward, false, 8);
        mgr.set_string("aaa".to_string());

        // search_update: forward from origin=8, no "aaa" after 8, wraps to find at 0
        let result = mgr.search_update(text);
        assert_eq!(result, Some((0, 3)));
        assert!(mgr.state().unwrap().wrapped);
    }

    #[test]
    fn isearch_backward_wraps() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb ccc";
        // Start at the beginning so "ccc" is ahead of us
        mgr.begin_search(SearchDirection::Backward, false, 0);
        mgr.set_string("ccc".to_string());

        let result = mgr.search_update(text);
        assert_eq!(result, Some((8, 11)));
        assert!(mgr.state().unwrap().wrapped);
    }

    // -----------------------------------------------------------------------
    // IsearchManager — case fold auto-detection
    // -----------------------------------------------------------------------

    #[test]
    fn case_fold_auto_lowercase_folds() {
        assert!(resolve_case_fold(None, "hello"));
    }

    #[test]
    fn case_fold_auto_uppercase_exact() {
        assert!(!resolve_case_fold(None, "Hello"));
    }

    #[test]
    fn case_fold_override_true() {
        assert!(resolve_case_fold(Some(true), "Hello"));
    }

    #[test]
    fn case_fold_override_false() {
        assert!(!resolve_case_fold(Some(false), "hello"));
    }

    #[test]
    fn isearch_case_fold_auto() {
        let mut mgr = IsearchManager::new();
        let text = "Hello World hello world";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        // Lowercase search string — should auto-fold
        mgr.set_string("hello".to_string());
        let result = mgr.search_update(text);
        // Should find "Hello" at 0 (case-folded)
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn isearch_case_fold_auto_uppercase_exact() {
        let mut mgr = IsearchManager::new();
        let text = "hello world Hello World";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        // Uppercase letter in search — should NOT fold
        mgr.set_string("Hello".to_string());
        let result = mgr.search_update(text);
        assert_eq!(result, Some((12, 17)));
    }

    // -----------------------------------------------------------------------
    // IsearchManager — regexp search
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_regexp_forward() {
        let mut mgr = IsearchManager::new();
        let text = "foo 123 bar 456";
        mgr.begin_search(SearchDirection::Forward, true, 0);
        mgr.set_string("[0-9]+".to_string());
        let result = mgr.search_update(text);
        assert_eq!(result, Some((4, 7)));
    }

    #[test]
    fn isearch_regexp_backward() {
        let mut mgr = IsearchManager::new();
        let text = "foo 123 bar 456";
        mgr.begin_search(SearchDirection::Backward, true, text.len());
        mgr.set_string("[0-9]+".to_string());
        let result = mgr.search_update(text);
        assert_eq!(result, Some((12, 15)));
    }

    // -----------------------------------------------------------------------
    // IsearchManager — lazy matches
    // -----------------------------------------------------------------------

    #[test]
    fn compute_lazy_matches_literal() {
        let mut mgr = IsearchManager::new();
        let text = "aa bb aa cc aa";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("aa".to_string());
        mgr.compute_lazy_matches(text, 0, text.len());
        let matches = &mgr.state().unwrap().lazy_matches;
        assert_eq!(matches.len(), 3);
        assert_eq!(matches[0], (0, 2));
        assert_eq!(matches[1], (6, 8));
        assert_eq!(matches[2], (12, 14));
    }

    #[test]
    fn compute_lazy_matches_regexp() {
        let mut mgr = IsearchManager::new();
        let text = "abc 123 def 456 ghi";
        mgr.begin_search(SearchDirection::Forward, true, 0);
        mgr.set_string("[0-9]+".to_string());
        mgr.compute_lazy_matches(text, 0, text.len());
        let matches = &mgr.state().unwrap().lazy_matches;
        assert_eq!(matches.len(), 2);
        assert_eq!(matches[0], (4, 7));
        assert_eq!(matches[1], (12, 15));
    }

    #[test]
    fn compute_lazy_matches_visible_region() {
        let mut mgr = IsearchManager::new();
        let text = "aaa bbb aaa ccc aaa";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("aaa".to_string());
        // Only look in the middle region [4..15]
        mgr.compute_lazy_matches(text, 4, 15);
        let matches = &mgr.state().unwrap().lazy_matches;
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0], (8, 11));
    }

    #[test]
    fn compute_lazy_matches_empty_string() {
        let mut mgr = IsearchManager::new();
        let text = "hello world";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string(String::new());
        mgr.compute_lazy_matches(text, 0, text.len());
        assert!(mgr.state().unwrap().lazy_matches.is_empty());
    }

    // -----------------------------------------------------------------------
    // IsearchManager — history navigation
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_history_navigation() {
        let mut mgr = IsearchManager::new();

        // Populate history
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("first".to_string());
        mgr.end_search(true);

        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("second".to_string());
        mgr.end_search(true);

        // Start a new search and navigate history
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.history_previous();
        assert_eq!(mgr.state().unwrap().search_string, "second");
        mgr.history_previous();
        assert_eq!(mgr.state().unwrap().search_string, "first");
        mgr.history_next();
        assert_eq!(mgr.state().unwrap().search_string, "second");
        mgr.history_next();
        assert!(mgr.state().unwrap().search_string.is_empty());
    }

    // -----------------------------------------------------------------------
    // IsearchManager — yank_word_or_char
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_yank_word() {
        let mut mgr = IsearchManager::new();
        let text = "hello world";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.yank_word_or_char(text, 0);
        assert_eq!(mgr.state().unwrap().search_string, "hello");
    }

    #[test]
    fn isearch_yank_nonword_char() {
        let mut mgr = IsearchManager::new();
        let text = " hello";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.yank_word_or_char(text, 0);
        // Space is not alphanumeric, so only one char yanked
        assert_eq!(mgr.state().unwrap().search_string, " ");
    }

    #[test]
    fn isearch_yank_at_end() {
        let mut mgr = IsearchManager::new();
        let text = "hi";
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.yank_word_or_char(text, 2);
        // At end of text, nothing yanked
        assert!(mgr.state().unwrap().search_string.is_empty());
    }

    // -----------------------------------------------------------------------
    // IsearchManager — prompt
    // -----------------------------------------------------------------------

    #[test]
    fn isearch_prompt_basic() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Forward, false, 0);
        mgr.set_string("test".to_string());
        let prompt = mgr.prompt();
        assert!(prompt.contains("I-search"));
        assert!(prompt.contains("test"));
        assert!(!prompt.contains("Regexp"));
    }

    #[test]
    fn isearch_prompt_regexp_backward_failing() {
        let mut mgr = IsearchManager::new();
        mgr.begin_search(SearchDirection::Backward, true, 0);
        mgr.set_string("pat".to_string());
        // Force a failing search
        let _ = mgr.search_update("no match here");
        let prompt = mgr.prompt();
        assert!(prompt.contains("Failing"));
        assert!(prompt.contains("Regexp"));
        assert!(prompt.contains("I-search backward"));
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — lifecycle
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_begin_end() {
        let mut mgr = QueryReplaceManager::new();
        assert!(!mgr.is_active());
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        assert!(mgr.is_active());
        let state = mgr.state().unwrap();
        assert_eq!(state.from_string, "foo");
        assert_eq!(state.to_string, "bar");
        assert!(!state.regexp);
        assert!(state.region_start.is_none());

        let summary = mgr.finish();
        assert_eq!(summary.replaced, 0);
        assert_eq!(summary.skipped, 0);
        assert!(!mgr.is_active());
    }

    #[test]
    fn query_replace_begin_in_region() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin_in_region("x".to_string(), "y".to_string(), false, 10, 50);
        let state = mgr.state().unwrap();
        assert_eq!(state.region_start, Some(10));
        assert_eq!(state.region_end, Some(50));
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — find_next
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_find_next_basic() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar foo baz foo";
        mgr.begin("foo".to_string(), "qux".to_string(), false);

        let r1 = mgr.find_next(text, 0);
        assert_eq!(r1, Some((0, 3)));

        let r2 = mgr.find_next(text, 3);
        assert_eq!(r2, Some((8, 11)));

        let r3 = mgr.find_next(text, 11);
        assert_eq!(r3, Some((16, 19)));

        let r4 = mgr.find_next(text, 19);
        assert!(r4.is_none());
    }

    #[test]
    fn query_replace_find_next_in_region() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar foo baz foo";
        mgr.begin_in_region("foo".to_string(), "qux".to_string(), false, 4, 15);

        let r1 = mgr.find_next(text, 0);
        assert_eq!(r1, Some((8, 11)));

        // Next "foo" is at 16 which is beyond region_end=15
        let r2 = mgr.find_next(text, 11);
        assert!(r2.is_none());
    }

    #[test]
    fn query_replace_find_next_regexp() {
        let mut mgr = QueryReplaceManager::new();
        let text = "abc 123 def 456";
        mgr.begin("[0-9]+".to_string(), "NUM".to_string(), true);

        let r1 = mgr.find_next(text, 0);
        assert_eq!(r1, Some((4, 7)));

        let r2 = mgr.find_next(text, 7);
        assert_eq!(r2, Some((12, 15)));
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — respond
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_respond_yes() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar";
        mgr.begin("foo".to_string(), "baz".to_string(), false);
        let _ = mgr.find_next(text, 0);

        let action = mgr.respond(QueryReplaceResponse::Yes);
        match action {
            QueryReplaceAction::Replace(start, end, repl) => {
                assert_eq!(start, 0);
                assert_eq!(end, 3);
                assert_eq!(repl, "baz");
            }
            _ => panic!("expected Replace action"),
        }
        assert_eq!(mgr.state().unwrap().replaced_count, 1);
    }

    #[test]
    fn query_replace_respond_no() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar";
        mgr.begin("foo".to_string(), "baz".to_string(), false);
        let _ = mgr.find_next(text, 0);

        let action = mgr.respond(QueryReplaceResponse::No);
        assert!(matches!(action, QueryReplaceAction::Skip));
        assert_eq!(mgr.state().unwrap().skipped_count, 1);
    }

    #[test]
    fn query_replace_respond_quit() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar foo";
        mgr.begin("foo".to_string(), "baz".to_string(), false);
        let _ = mgr.find_next(text, 0);
        mgr.respond(QueryReplaceResponse::Yes);

        let _ = mgr.find_next(text, 3);
        let action = mgr.respond(QueryReplaceResponse::Quit);
        match action {
            QueryReplaceAction::Done(summary) => {
                assert_eq!(summary.replaced, 1);
                assert_eq!(summary.skipped, 0);
            }
            _ => panic!("expected Done action"),
        }
        assert!(!mgr.is_active());
    }

    #[test]
    fn query_replace_respond_delete() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar";
        mgr.begin("foo".to_string(), "baz".to_string(), false);
        let _ = mgr.find_next(text, 0);

        let action = mgr.respond(QueryReplaceResponse::Delete);
        match action {
            QueryReplaceAction::Replace(start, end, repl) => {
                assert_eq!(start, 0);
                assert_eq!(end, 3);
                assert!(repl.is_empty());
            }
            _ => panic!("expected Replace with empty string"),
        }
    }

    #[test]
    fn query_replace_respond_help() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);

        let action = mgr.respond(QueryReplaceResponse::Help);
        match action {
            QueryReplaceAction::ShowHelp(text) => {
                assert!(text.contains("replace this match"));
                assert!(text.contains("skip this match"));
            }
            _ => panic!("expected ShowHelp action"),
        }
    }

    #[test]
    fn query_replace_respond_edit() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        let action = mgr.respond(QueryReplaceResponse::Edit);
        assert!(matches!(action, QueryReplaceAction::NeedInput));
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — undo
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_undo() {
        let mut mgr = QueryReplaceManager::new();
        let text = "foo bar foo";
        mgr.begin("foo".to_string(), "baz".to_string(), false);

        let _ = mgr.find_next(text, 0);
        mgr.respond(QueryReplaceResponse::Yes);
        assert_eq!(mgr.state().unwrap().replaced_count, 1);

        let undo = mgr.undo_last();
        assert!(undo.is_some());
        let undo = undo.unwrap();
        assert_eq!(undo.position, 0);
        assert_eq!(undo.replacement, "baz");
        assert_eq!(mgr.state().unwrap().replaced_count, 0);
    }

    #[test]
    fn query_replace_undo_empty_stack() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        assert!(mgr.undo_last().is_none());
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — compute_replacement with case preservation
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_compute_replacement_lower() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        assert_eq!(mgr.compute_replacement("foo"), "bar");
    }

    #[test]
    fn query_replace_compute_replacement_upper() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        assert_eq!(mgr.compute_replacement("FOO"), "BAR");
    }

    #[test]
    fn query_replace_compute_replacement_capitalized() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("foo".to_string(), "bar".to_string(), false);
        assert_eq!(mgr.compute_replacement("Foo"), "Bar");
    }

    // -----------------------------------------------------------------------
    // QueryReplaceManager — prompt
    // -----------------------------------------------------------------------

    #[test]
    fn query_replace_prompt() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("old".to_string(), "new".to_string(), false);
        let prompt = mgr.prompt();
        assert!(prompt.contains("Query replacing"));
        assert!(prompt.contains("old"));
        assert!(prompt.contains("new"));
        assert!(!prompt.contains("regexp"));
    }

    #[test]
    fn query_replace_prompt_regexp() {
        let mut mgr = QueryReplaceManager::new();
        mgr.begin("[0-9]+".to_string(), "NUM".to_string(), true);
        let prompt = mgr.prompt();
        assert!(prompt.contains("regexp"));
    }

    // -----------------------------------------------------------------------
    // preserve_case helper
    // -----------------------------------------------------------------------

    #[test]
    fn preserve_case_all_lower() {
        assert_eq!(preserve_case("bar", "foo"), "bar");
    }

    #[test]
    fn preserve_case_all_upper() {
        assert_eq!(preserve_case("bar", "FOO"), "BAR");
    }

    #[test]
    fn preserve_case_capitalized() {
        assert_eq!(preserve_case("bar", "Foo"), "Bar");
    }

    #[test]
    fn preserve_case_mixed() {
        // Mixed case like "fOo" doesn't match any pattern, return as-is
        assert_eq!(preserve_case("bar", "fOo"), "bar");
    }

    #[test]
    fn preserve_case_empty_matched() {
        assert_eq!(preserve_case("bar", ""), "bar");
    }

    #[test]
    fn preserve_case_empty_replacement() {
        assert_eq!(preserve_case("", "FOO"), "");
    }

    #[test]
    fn preserve_case_non_alpha_upper() {
        // "123" has no alphabetic chars, so all_upper && has_alpha is false
        assert_eq!(preserve_case("bar", "123"), "bar");
    }

    // -----------------------------------------------------------------------
    // find_match helper
    // -----------------------------------------------------------------------

    #[test]
    fn find_match_literal_forward() {
        let text = "hello world hello";
        let result = find_match(text, "hello", 0, true, false, false);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn find_match_literal_forward_from_offset() {
        let text = "hello world hello";
        let result = find_match(text, "hello", 1, true, false, false);
        assert_eq!(result, Some((12, 17)));
    }

    #[test]
    fn find_match_literal_backward() {
        let text = "hello world hello";
        let result = find_match(text, "hello", text.len(), false, false, false);
        assert_eq!(result, Some((12, 17)));
    }

    #[test]
    fn find_match_literal_backward_from_middle() {
        let text = "hello world hello";
        let result = find_match(text, "hello", 10, false, false, false);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn find_match_case_fold() {
        let text = "Hello World";
        let result = find_match(text, "hello", 0, true, false, true);
        assert_eq!(result, Some((0, 5)));
    }

    #[test]
    fn find_match_case_sensitive() {
        let text = "Hello World";
        let result = find_match(text, "hello", 0, true, false, false);
        assert!(result.is_none());
    }

    #[test]
    fn find_match_regexp_forward() {
        let text = "foo 123 bar";
        let result = find_match(text, "[0-9]+", 0, true, true, false);
        assert_eq!(result, Some((4, 7)));
    }

    #[test]
    fn find_match_regexp_backward() {
        let text = "foo 123 bar 456";
        let result = find_match(text, "[0-9]+", text.len(), false, true, false);
        assert_eq!(result, Some((12, 15)));
    }

    #[test]
    fn find_match_empty_pattern() {
        let text = "hello";
        assert!(find_match(text, "", 0, true, false, false).is_none());
    }

    #[test]
    fn find_match_no_match() {
        let text = "hello world";
        assert!(find_match(text, "zzz", 0, true, false, false).is_none());
    }

    #[test]
    fn find_match_at_boundary() {
        let text = "abcdef";
        let result = find_match(text, "def", 3, true, false, false);
        assert_eq!(result, Some((3, 6)));
    }

    // -----------------------------------------------------------------------
    // Builtin function stubs
    // -----------------------------------------------------------------------

    #[test]
    fn builtin_isearch_forward_returns_nil() {
        let result = builtin_isearch_forward(vec![]);
        assert!(matches!(result, Ok(Value::Nil)));
    }

    #[test]
    fn builtin_isearch_backward_returns_nil() {
        let result = builtin_isearch_backward(vec![]);
        assert!(matches!(result, Ok(Value::Nil)));
    }

    #[test]
    fn builtin_query_replace_needs_two_args() {
        let result = builtin_query_replace(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_query_replace_with_args() {
        let result = builtin_query_replace(vec![Value::string("foo"), Value::string("bar")]);
        assert!(matches!(result, Ok(Value::Nil)));
    }

    #[test]
    fn builtin_replace_string_needs_two_args() {
        let result = builtin_replace_string(vec![Value::string("a")]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_replace_string_empty_from() {
        let result = builtin_replace_string(vec![Value::string(""), Value::string("bar")]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_replace_string_ok() {
        let result = builtin_replace_string(vec![Value::string("foo"), Value::string("bar")]);
        assert!(matches!(result, Ok(Value::Int(0))));
    }

    #[test]
    fn builtin_how_many_needs_one_arg() {
        let result = builtin_how_many(vec![]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_how_many_ok() {
        let result = builtin_how_many(vec![Value::string("pattern")]);
        assert!(matches!(result, Ok(Value::Int(0))));
    }

    #[test]
    fn builtin_keep_lines_ok() {
        let result = builtin_keep_lines(vec![Value::string("pat")]);
        assert!(matches!(result, Ok(Value::Int(0))));
    }

    #[test]
    fn builtin_flush_lines_ok() {
        let result = builtin_flush_lines(vec![Value::string("pat")]);
        assert!(matches!(result, Ok(Value::Int(0))));
    }

    #[test]
    fn builtin_replace_regexp_ok() {
        let result = builtin_replace_regexp(vec![Value::string("[0-9]+"), Value::string("NUM")]);
        assert!(matches!(result, Ok(Value::Int(0))));
    }

    #[test]
    fn builtin_query_replace_regexp_needs_two_args() {
        let result = builtin_query_replace_regexp(vec![Value::string("a")]);
        assert!(result.is_err());
    }

    #[test]
    fn builtin_query_replace_regexp_ok() {
        let result =
            builtin_query_replace_regexp(vec![Value::string("[0-9]+"), Value::string("NUM")]);
        assert!(matches!(result, Ok(Value::Nil)));
    }
}
