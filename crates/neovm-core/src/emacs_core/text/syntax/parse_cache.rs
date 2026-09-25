//! The syntax parse cache (P3.4): `parse-partial-sexp` answers from recorded
//! loop states of earlier scans, exactly.
//!
//! `parse-partial-sexp` is a pure function of the text, the syntax table, the
//! `syntax-table` properties, a few variables, FROM, the starting state, the
//! options and TO. Its scan loop is deterministic and resumable at any loop top
//! (`parse_loop`), so an earlier scan's loop state at `p` is the state a new
//! scan with the same inputs reaches at `p`, for any TO after `p`. The cache
//! keeps such states per starting key and never answers from one whose inputs
//! may have changed.
//!
//! This file holds the cache and its invalidation. Every change to what a scan
//! reads reaches it:
//!
//! * text: the four measured mutators note the edit's first byte
//!   ([`SyntaxParseCache::note_edit`]); wholesale replacements, multibyte and
//!   backend conversions clear it;
//! * `syntax-table` / `category` properties: every property-table mutation that
//!   moves the table's syntax tick notes the first character it can touch
//!   ([`SyntaxParseCache::note_prop_change`]); a swapped-in table clears it;
//! * everything else a scan reads (the syntax table and its char-table write
//!   tick, BEGV, `parse-sexp-lookup-properties`, `comment-end-can-be-escaped`,
//!   multibyteness) is part of each query's key.
//!
//! Notes are counted: before each use, a text epoch or syntax tick that moved
//! further than the notes account for (a mutation path that bypassed them)
//! clears everything, so a missed hook costs hits, never answers.
//!
//! The cache lives in `BufferTextStorage`, outside the copy-on-write text, and
//! a snapshot clone starts empty (P3.0 §3.9). It holds positions and parse
//! states only: no `Value`, nothing to trace.
//!
//! `NEOVM_SYNTAX_PARSE_CACHE`: `0`/`off` (default), `1`/`on`, or `verify`
//! (every cached answer is recomputed by a plain scan and compared).

// The invalidation plumbing lands before the memo that reads it (S3 -> S4).
#![allow(dead_code)]

use std::sync::atomic::{AtomicU8, Ordering};

/// `NEOVM_SYNTAX_PARSE_CACHE`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ParseCacheMode {
    /// Every scan runs from FROM, as before (the default until measured).
    Off,
    /// Cached answers are served.
    On,
    /// Cached answers are computed, recomputed by a plain scan and compared;
    /// a mismatch is reported and the plain answer returned.
    Verify,
}

const MODE_UNREAD: u8 = 0;
const MODE_OFF: u8 = 1;
const MODE_ON: u8 = 2;
const MODE_VERIFY: u8 = 3;

static MODE: AtomicU8 = AtomicU8::new(MODE_UNREAD);

#[cfg(test)]
thread_local! {
    /// Test override of `NEOVM_SYNTAX_PARSE_CACHE`.
    pub(crate) static MODE_OVERRIDE: std::cell::Cell<Option<ParseCacheMode>> =
        const { std::cell::Cell::new(None) };
}

/// The cache mode, read once from `NEOVM_SYNTAX_PARSE_CACHE`.
#[inline]
pub(crate) fn parse_cache_mode() -> ParseCacheMode {
    #[cfg(test)]
    if let Some(mode) = MODE_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    match MODE.load(Ordering::Relaxed) {
        MODE_OFF => ParseCacheMode::Off,
        MODE_ON => ParseCacheMode::On,
        MODE_VERIFY => ParseCacheMode::Verify,
        _ => read_parse_cache_knob(),
    }
}

#[cold]
#[inline(never)]
fn read_parse_cache_knob() -> ParseCacheMode {
    let mode = parse_parse_cache_knob(std::env::var("NEOVM_SYNTAX_PARSE_CACHE").ok().as_deref());
    MODE.store(
        match mode {
            ParseCacheMode::Off => MODE_OFF,
            ParseCacheMode::On => MODE_ON,
            ParseCacheMode::Verify => MODE_VERIFY,
        },
        Ordering::Relaxed,
    );
    tracing::debug!(?mode, "NEOVM_SYNTAX_PARSE_CACHE read");
    mode
}

pub(crate) fn parse_parse_cache_knob(value: Option<&str>) -> ParseCacheMode {
    match value.map(str::trim).map(str::to_ascii_lowercase).as_deref() {
        Some("1" | "on" | "yes" | "true" | "t") => ParseCacheMode::On,
        Some("verify") => ParseCacheMode::Verify,
        _ => ParseCacheMode::Off,
    }
}

/// What changed since the cache was last used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Invalidation {
    /// Nothing a scan reads.
    Nothing,
    /// Text at or after `byte`, or syntax-relevant properties at or after
    /// `char` (either may be `usize::MAX`): everything a scan read before both
    /// is unchanged, and so are its coordinates.
    From { byte: usize, char: usize },
    /// Anything.
    All,
}

/// Per-storage syntax parse cache. See the module documentation.
#[derive(Debug, Default)]
pub(crate) struct SyntaxParseCache {
    /// Lowest Emacs byte position whose text changed since the last drain.
    dirty_byte: Option<usize>,
    /// Lowest char position whose syntax-relevant properties changed since
    /// the last drain.
    dirty_char: Option<usize>,
    /// A wholesale change since the last drain.
    cleared: bool,
    /// The text epoch and the property table's syntax tick at the last drain.
    seen_epoch: u64,
    seen_prop_tick: u64,
    /// How far the notes since the last drain account for each of them.
    noted_edits: u64,
    noted_prop_ticks: u64,
}

impl SyntaxParseCache {
    /// A text edit whose first changed byte is `at_byte` (every later byte may
    /// have moved). Called once per content-epoch bump.
    #[inline]
    pub(crate) fn note_edit(&mut self, at_byte: usize) {
        self.dirty_byte = Some(self.dirty_byte.map_or(at_byte, |dirty| dirty.min(at_byte)));
        self.noted_edits = self.noted_edits.wrapping_add(1);
    }

    /// A property mutation that moved the syntax tick by `ticks` and can
    /// change syntax-relevant properties only at or after `at_char`.
    #[inline]
    pub(crate) fn note_prop_change(&mut self, at_char: usize, ticks: u64) {
        self.dirty_char = Some(self.dirty_char.map_or(at_char, |dirty| dirty.min(at_char)));
        self.noted_prop_ticks = self.noted_prop_ticks.wrapping_add(ticks);
    }

    /// Everything goes: a change with no known extent.
    #[inline]
    pub(crate) fn clear(&mut self) {
        self.cleared = true;
    }

    /// What changed since the last drain, given the storage's current text
    /// epoch and syntax tick; resets the notes.
    pub(crate) fn drain(&mut self, epoch: u64, prop_tick: u64) -> Invalidation {
        let unaccounted_text = epoch.wrapping_sub(self.seen_epoch) != self.noted_edits;
        let unaccounted_props =
            prop_tick.wrapping_sub(self.seen_prop_tick) != self.noted_prop_ticks;
        let outcome = if self.cleared || unaccounted_text || unaccounted_props {
            if !self.cleared && (unaccounted_text || unaccounted_props) {
                tracing::trace!(
                    unaccounted_text,
                    unaccounted_props,
                    "syntax parse cache: a change bypassed the notes; clearing"
                );
            }
            Invalidation::All
        } else if self.dirty_byte.is_none() && self.dirty_char.is_none() {
            Invalidation::Nothing
        } else {
            Invalidation::From {
                byte: self.dirty_byte.unwrap_or(usize::MAX),
                char: self.dirty_char.unwrap_or(usize::MAX),
            }
        };
        self.dirty_byte = None;
        self.dirty_char = None;
        self.cleared = false;
        self.seen_epoch = epoch;
        self.seen_prop_tick = prop_tick;
        self.noted_edits = 0;
        self.noted_prop_ticks = 0;
        outcome
    }
}

#[cfg(test)]
#[path = "tests/parse_cache_invalidation.rs"]
mod invalidation_tests;
