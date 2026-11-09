//! Per-storage syntax parse invalidation state (stage-3 C7).
//!
//! The syntax parse cache's recorded runs are text-layer data: they hold parse
//! states, comment flavors and descriptor words, and only the syntax scanner
//! knows their keys.  What the buffer substrate owns is the *account of
//! change*: which text byte and which property character changed since the
//! cache last looked, whether everything must go, and the epoch/tick promises
//! the notes are checked against.  That account is plain integers and lives
//! here, next to the text it describes.
//!
//! The text layer keeps its runs in the storage's opaque `syntax_parse_body`
//! slot and downcasts them. This crate never names that cache type. A snapshot
//! starts with an empty slot, so a clone does not copy another storage's runs.
//! Nothing on the insert, delete or property paths calls into the text layer.

use std::cell::Cell;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU64, Ordering};

/// What changed since a syntax parse cache was last used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub enum Invalidation {
    /// Nothing a scan reads.
    Nothing,
    /// Text at or after `byte`, or syntax-relevant properties at or after
    /// `char` (either may be `usize::MAX`): everything a scan read before both
    /// is unchanged, and so are its coordinates.
    From { byte: usize, char: usize },
    /// Anything.
    All,
}

/// The invalidation notes a `BufferTextStorage` leaves for the syntax parse
/// cache.  Plain values: no `Value`, no borrow, nothing to trace.
#[derive(Clone, Copy, Debug, Default)]
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) struct SyntaxMutationNotes {
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

impl SyntaxMutationNotes {
    /// A text edit whose first changed byte is `at_byte` (every later byte may
    /// have moved). Called once per content-epoch bump.
    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn note_edit(&mut self, at_byte: usize) {
        self.dirty_byte = Some(self.dirty_byte.map_or(at_byte, |dirty| dirty.min(at_byte)));
        self.noted_edits = self.noted_edits.wrapping_add(1);
    }

    /// A property mutation that moved the syntax tick by `ticks` and can
    /// change syntax-relevant properties only at or after `at_char`.
    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn note_prop_change(&mut self, at_char: usize, ticks: u64) {
        self.dirty_char = Some(self.dirty_char.map_or(at_char, |dirty| dirty.min(at_char)));
        self.noted_prop_ticks = self.noted_prop_ticks.wrapping_add(ticks);
    }

    /// Everything goes: a change with no known extent.
    #[inline]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn clear(&mut self) {
        self.cleared = true;
    }

    /// Apply what changed since the last drain, given the storage's current
    /// text epoch and syntax tick, and reset the notes.
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn take_invalidation(&mut self, epoch: u64, prop_tick: u64) -> Invalidation {
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

/// Identity of one `BufferTextStorage`'s syntax parse cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[doc(hidden)]
pub struct SyntaxCacheId(u64);

impl SyntaxCacheId {
    #[inline]
    #[doc(hidden)]
    pub fn get(self) -> u64 {
        self.0
    }
}

static NEXT_CACHE_ID: AtomicU64 = AtomicU64::new(1);

/// The substrate half of the syntax parse cache: a storage's cache identity
/// and the invalidation notes its mutators leave. The recorded runs live in
/// the storage's opaque `syntax_parse_body` slot, which the text crate owns.
pub(crate) struct SyntaxCacheState {
    id: SyntaxCacheId,
    /// `Cell` rather than `RefCell`: the notes are `Copy` plain data and no
    /// borrow guard is needed on the insert path.
    notes: Cell<SyntaxMutationNotes>,
}

impl SyntaxCacheState {
    /// A fresh identity with nothing seen yet: a new storage (or a snapshot)
    /// reports everything changed on its first use, as P3.0 §3.9 requires.
    pub(crate) fn fresh() -> Self {
        Self {
            id: SyntaxCacheId(NEXT_CACHE_ID.fetch_add(1, Ordering::Relaxed)),
            notes: Cell::new(SyntaxMutationNotes::default()),
        }
    }

    #[inline]
    pub(crate) fn id(&self) -> SyntaxCacheId {
        self.id
    }

    #[inline]
    pub(crate) fn note_edit(&self, at_byte: usize) {
        let mut notes = self.notes.get();
        notes.note_edit(at_byte);
        self.notes.set(notes);
    }

    #[inline]
    pub(crate) fn note_prop_change(&self, at_char: usize, ticks: u64) {
        let mut notes = self.notes.get();
        notes.note_prop_change(at_char, ticks);
        self.notes.set(notes);
    }

    #[inline]
    pub(crate) fn clear(&self) {
        let mut notes = self.notes.get();
        notes.clear();
        self.notes.set(notes);
    }

    pub(crate) fn take_invalidation(&self, epoch: u64, prop_tick: u64) -> Invalidation {
        let mut notes = self.notes.get();
        let invalidation = notes.take_invalidation(epoch, prop_tick);
        self.notes.set(notes);
        invalidation
    }
}

/// Optional cold-drop hook. The runs live on the storage now, so dropping a
/// storage drops them. A caller that still keeps an out-of-line entry can
/// install a hook; nothing in this crate does.
type ForgetHook = fn(u64);

static FORGET_HOOK: OnceLock<ForgetHook> = OnceLock::new();

#[doc(hidden)]
pub fn install_forget_hook(hook: ForgetHook) {
    let _ = FORGET_HOOK.set(hook);
}

impl Drop for SyntaxCacheState {
    fn drop(&mut self) {
        if let Some(hook) = FORGET_HOOK.get() {
            hook(self.id.get());
        }
    }
}
