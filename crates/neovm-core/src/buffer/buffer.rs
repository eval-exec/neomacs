//! Buffer and BufferManager — the core text container for the Elisp VM.
//!
//! A `Buffer` wraps a [`BufferText`] with Emacs-style point, mark, narrowing,
//! markers, and buffer-local variables.  `BufferManager` owns all live buffers
//! and tracks the current buffer.

#[path = "insdel.rs"]
mod insdel;

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::mem;
use std::sync::OnceLock;

use super::buffer_text::BufferText;
use super::marker_data::{marker_data_anchor, positioned_marker_data, set_marker_data_anchor};
use super::position::{
    AccessibleCharRange, AccessibleEmacsByteRange, CharLen, CharPos0, CharRange, EmacsByteLen,
    EmacsBytePos, EmacsByteRange, FullBufferLispCharRange, LispCharPos1, TextPositionAnchor,
};
#[cfg(test)]
use super::text::BufferTextBytesSnapshot;
use super::text::{BufferTextBackendKind, ImplementedBufferTextBackendKind};
// Phase 10F: BufferLocals is gone. Per-buffer Lisp bindings now live
// in `Buffer::local_var_alist` (for LOCALIZED), `Buffer::slots[]`
// (for FORWARDED BUFFER_OBJFWD), and `Buffer::keymap` / the
// `SharedUndoState` (for the two always-present slots that don't
// match either pattern). Mirrors GNU's struct buffer layout in
// buffer.h:330-462.
use super::overlay::OverlayList;
use super::shared::{SavedPointBeforeCommand, SharedUndoState};
use super::text_props::{ObjectIntervalRun, TextPropertyTable};
use super::undo;
#[cfg(test)]
use super::visited_file_modtime::BaseVisitedFileModtime;
use super::visited_file_modtime::{FirstChangeModtime, VisitedFileModtime, VisitedFileModtimeSlot};
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::symbol::SetInternalAlist;
use crate::emacs_core::value::{RuntimeBindingValue, Value, ValueKind, eq_value};
use crate::gc_trace::GcTrace;
use crate::window::WindowId;
use rustc_hash::FxHashMap;

#[cfg(test)]
thread_local! {
    static BUFFER_LOCAL_VALUE_LOOKUP_PROBES: std::cell::Cell<u64> = const {
        std::cell::Cell::new(0)
    };
    static LOCAL_VAR_ALIST_ENTRY_PROBES: std::cell::Cell<u64> = const {
        std::cell::Cell::new(0)
    };
}

#[cfg(test)]
pub(crate) fn reset_buffer_local_value_lookup_probes() {
    BUFFER_LOCAL_VALUE_LOOKUP_PROBES.set(0);
}

#[cfg(test)]
pub(crate) fn buffer_local_value_lookup_probes() -> u64 {
    BUFFER_LOCAL_VALUE_LOOKUP_PROBES.get()
}

#[cfg(test)]
fn note_local_var_alist_entry_probe() {
    LOCAL_VAR_ALIST_ENTRY_PROBES.set(LOCAL_VAR_ALIST_ENTRY_PROBES.get().saturating_add(1));
}

#[cfg(not(test))]
#[inline(always)]
fn note_local_var_alist_entry_probe() {}

#[cfg(test)]
pub(crate) fn reset_local_var_alist_entry_probes() {
    LOCAL_VAR_ALIST_ENTRY_PROBES.set(0);
}

#[cfg(test)]
pub(crate) fn local_var_alist_entry_probes() -> u64 {
    LOCAL_VAR_ALIST_ENTRY_PROBES.get()
}

// ---------------------------------------------------------------------------
// BUFFER_SLOT_COUNT — sized to mirror GNU's `MAX_PER_BUFFER_VARS = 50`.
// ---------------------------------------------------------------------------

/// Number of `BUFFER_OBJFWD` slots in [`Buffer::slots`]. Mirrors GNU's
/// `MAX_PER_BUFFER_VARS = 50` limit on per-buffer C-side variables
/// (`buffer.h:311`). Bumped to 64 in Phase 10D so the conditional
/// `BUFFER_OBJFWD` slots (mode-line-format, fill-column, …) have room
/// alongside the always-local set already migrated in Phase 10A-C.
/// Sized to a power of two so [`Buffer::local_flags`] (a `u64`
/// bitmap) covers exactly one bit per slot. Bump again only after a
/// careful audit — the number bounds every Buffer's memory footprint.
pub const BUFFER_SLOT_COUNT: usize = 64;

/// Index into [`Buffer::slots`].  This is a compact Rust representation of
/// GNU's per-buffer slot domain: GNU stores byte offsets into `struct buffer`,
/// while Neomacs stores dense slot indices into a fixed `[Value; 64]`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct BufferSlot(u8);

impl BufferSlot {
    pub const fn new(index: usize) -> Self {
        assert!(index < BUFFER_SLOT_COUNT);
        Self(index as u8)
    }

    pub const fn index(self) -> usize {
        self.0 as usize
    }

    pub const fn as_u16(self) -> u16 {
        self.0 as u16
    }

    pub const fn local_flags_idx(self) -> i16 {
        self.0 as i16
    }

    pub fn from_u16(index: u16) -> Option<Self> {
        if usize::from(index) < BUFFER_SLOT_COUNT {
            Some(Self(index as u8))
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Phase 8b slot offset constants for the four hardcoded Buffer fields
// that will migrate from direct struct fields to slot accessors in
// follow-up commits. Mirrors GNU `buffer.c:5056-5500` where each
// `DEFVAR_PER_BUFFER` assigns a stable slot index.
// ---------------------------------------------------------------------------

/// Slot index for `buffer-file-name`. Mirrors GNU's slot for the
/// `file_name_` field in `struct buffer` (`buffer.h:319`).
pub const BUFFER_SLOT_FILE_NAME: BufferSlot = BufferSlot::new(0);
/// Slot index for `buffer-auto-save-file-name`. Mirrors GNU's
/// `auto_save_file_name_` (`buffer.h:323`).
pub const BUFFER_SLOT_AUTO_SAVE_FILE_NAME: BufferSlot = BufferSlot::new(1);
/// Slot index for `buffer-read-only`. Mirrors GNU's `read_only_`
/// (`buffer.h:338`).
pub const BUFFER_SLOT_READ_ONLY: BufferSlot = BufferSlot::new(2);
/// Slot index for `enable-multibyte-characters`. Mirrors GNU's
/// `enable_multibyte_characters_` (`buffer.h:346`).
pub const BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS: BufferSlot = BufferSlot::new(3);
/// Slot index for `buffer-file-truename`. Mirrors GNU's
/// `file_truename_` (`buffer.h:325`).
pub const BUFFER_SLOT_FILE_TRUENAME: BufferSlot = BufferSlot::new(4);
/// Slot index for `default-directory`. Mirrors GNU's
/// `directory_` (`buffer.h:321`).
pub const BUFFER_SLOT_DEFAULT_DIRECTORY: BufferSlot = BufferSlot::new(5);
/// Slot index for `buffer-saved-size`. Mirrors GNU's `save_length_`
/// (`buffer.h:340`).
pub const BUFFER_SLOT_SAVED_SIZE: BufferSlot = BufferSlot::new(6);
/// Slot index for `buffer-backed-up`. Mirrors GNU's `backed_up_`
/// (`buffer.h:341`).
pub const BUFFER_SLOT_BACKED_UP: BufferSlot = BufferSlot::new(7);
/// Slot index for `buffer-file-format`. Mirrors GNU's
/// `file_format_` (`buffer.h:342`).
pub const BUFFER_SLOT_FILE_FORMAT: BufferSlot = BufferSlot::new(8);
/// Slot index for `buffer-auto-save-file-format`. Mirrors GNU's
/// `auto_save_file_format_` (`buffer.h:343`).
pub const BUFFER_SLOT_AUTO_SAVE_FILE_FORMAT: BufferSlot = BufferSlot::new(9);
/// Slot index for `major-mode`. Mirrors GNU's `major_mode_`
/// (`buffer.h:347`).
pub const BUFFER_SLOT_MAJOR_MODE: BufferSlot = BufferSlot::new(10);
/// Slot index for `local-minor-modes`. Mirrors GNU's
/// `local_minor_modes_` (`buffer.h:349`).
pub const BUFFER_SLOT_LOCAL_MINOR_MODES: BufferSlot = BufferSlot::new(11);
/// Slot index for `mode-name`. Mirrors GNU's `mode_name_`
/// (`buffer.h:351`).
pub const BUFFER_SLOT_MODE_NAME: BufferSlot = BufferSlot::new(12);
/// Slot index for `mark-active`. Mirrors GNU's `mark_active_`
/// (`buffer.h:381`).
pub const BUFFER_SLOT_MARK_ACTIVE: BufferSlot = BufferSlot::new(13);
/// Slot index for `point-before-scroll`. Mirrors GNU's
/// `point_before_scroll_` (`buffer.h:413`).
pub const BUFFER_SLOT_POINT_BEFORE_SCROLL: BufferSlot = BufferSlot::new(14);
/// Slot index for `buffer-display-count`. Mirrors GNU's
/// `display_count_` (`buffer.h:418`).
pub const BUFFER_SLOT_DISPLAY_COUNT: BufferSlot = BufferSlot::new(15);
/// Slot index for `buffer-display-time`. Mirrors GNU's
/// `display_time_` (`buffer.h:432`).
pub const BUFFER_SLOT_DISPLAY_TIME: BufferSlot = BufferSlot::new(16);
/// Slot index for `buffer-invisibility-spec`. Mirrors GNU's
/// `invisibility_spec_` (`buffer.h:411`).
pub const BUFFER_SLOT_INVISIBILITY_SPEC: BufferSlot = BufferSlot::new(17);

// ---------------------------------------------------------------------------
// Phase 10D conditional slot offsets. These are BUFFER_OBJFWD slots
// with `local_flags_idx >= 0`: a fresh buffer's slot mirrors the
// global default in `BufferManager::buffer_defaults` until
// `make-local-variable` or a write through the slot flips the
// per-buffer `Buffer::local_flags` bit.
//
// Mirrors GNU `buffer.c:4742-4791` where each conditional `BVAR` slot
// gets a positive index assigned in `buffer_local_flags`.
// ---------------------------------------------------------------------------

/// Slot index for `fill-column`. Mirrors GNU's `fill_column_`
/// (`buffer.h:387`). First conditional slot migrated by Phase 10D
/// step 3 — picked because the value is a simple integer with a
/// non-trivial default (70) and dense test coverage.
pub const BUFFER_SLOT_FILL_COLUMN: BufferSlot = BufferSlot::new(18);
/// Slot index for `tab-width`. Mirrors GNU's `tab_width_`
/// (`buffer.h:386`). Default 8 (`buffer.c:4848`).
pub const BUFFER_SLOT_TAB_WIDTH: BufferSlot = BufferSlot::new(19);
/// Slot index for `left-margin`. Mirrors GNU's `left_margin_`
/// (`buffer.h:388`). Default 0 (`buffer.c:4867`).
pub const BUFFER_SLOT_LEFT_MARGIN: BufferSlot = BufferSlot::new(20);
/// Slot index for `abbrev-mode`. Mirrors GNU's `abbrev_mode_`
/// (`buffer.h:368`). Default nil (`buffer.c:4835`).
pub const BUFFER_SLOT_ABBREV_MODE: BufferSlot = BufferSlot::new(21);
/// Slot index for `overwrite-mode`. Mirrors GNU's `overwrite_mode_`
/// (`buffer.h:369`). Default nil (`buffer.c:4836`).
pub const BUFFER_SLOT_OVERWRITE_MODE: BufferSlot = BufferSlot::new(22);
/// Slot index for `selective-display`. Mirrors GNU's
/// `selective_display_` (`buffer.h:373`). Default nil (`buffer.c:4838`).
pub const BUFFER_SLOT_SELECTIVE_DISPLAY: BufferSlot = BufferSlot::new(23);
/// Slot index for `selective-display-ellipses`. Mirrors GNU's
/// `selective_display_ellipses_` (`buffer.h:374`). Default t
/// (`buffer.c:4839`).
pub const BUFFER_SLOT_SELECTIVE_DISPLAY_ELLIPSES: BufferSlot = BufferSlot::new(24);
/// Slot index for `truncate-lines`. Mirrors GNU's `truncate_lines_`
/// (`buffer.h:355`). Default nil (`buffer.c:4849`).
pub const BUFFER_SLOT_TRUNCATE_LINES: BufferSlot = BufferSlot::new(25);
/// Slot index for `word-wrap`. Mirrors GNU's `word_wrap_`
/// (`buffer.h:357`). Default nil (`buffer.c:4850`).
pub const BUFFER_SLOT_WORD_WRAP: BufferSlot = BufferSlot::new(26);
/// Slot index for `ctl-arrow`. Mirrors GNU's `ctl_arrow_`
/// (`buffer.h:359`). Default t (`buffer.c:4851`).
pub const BUFFER_SLOT_CTL_ARROW: BufferSlot = BufferSlot::new(27);
/// Slot index for `auto-fill-function`. Mirrors GNU's
/// `auto_fill_function_` (`buffer.h:367`). Default nil
/// (`buffer.c:4837`).
pub const BUFFER_SLOT_AUTO_FILL_FUNCTION: BufferSlot = BufferSlot::new(28);
/// Slot index for `mode-line-format`. Default `"%-"`.
pub const BUFFER_SLOT_MODE_LINE_FORMAT: BufferSlot = BufferSlot::new(29);
/// Slot index for `header-line-format`. Default nil.
pub const BUFFER_SLOT_HEADER_LINE_FORMAT: BufferSlot = BufferSlot::new(30);
/// Slot index for `tab-line-format`. Default nil.
pub const BUFFER_SLOT_TAB_LINE_FORMAT: BufferSlot = BufferSlot::new(31);
//
// Phase 10D step 5 batch 2 — display/bidi/fringe/scroll-bar slots.
/// Slot index for `bidi-display-reordering`. Default t.
pub const BUFFER_SLOT_BIDI_DISPLAY_REORDERING: BufferSlot = BufferSlot::new(32);
/// Slot index for `bidi-paragraph-direction`. Default nil.
pub const BUFFER_SLOT_BIDI_PARAGRAPH_DIRECTION: BufferSlot = BufferSlot::new(33);
/// Slot index for `bidi-paragraph-start-re`. Default nil.
pub const BUFFER_SLOT_BIDI_PARAGRAPH_START_RE: BufferSlot = BufferSlot::new(34);
/// Slot index for `bidi-paragraph-separate-re`. Default nil.
pub const BUFFER_SLOT_BIDI_PARAGRAPH_SEPARATE_RE: BufferSlot = BufferSlot::new(35);
/// Slot index for `cursor-type`. Default t.
pub const BUFFER_SLOT_CURSOR_TYPE: BufferSlot = BufferSlot::new(36);
/// Slot index for `line-spacing`. Default nil.
pub const BUFFER_SLOT_LINE_SPACING: BufferSlot = BufferSlot::new(37);
/// Slot index for `text-conversion-style`. Default nil.
pub const BUFFER_SLOT_TEXT_CONVERSION_STYLE: BufferSlot = BufferSlot::new(38);
/// Slot index for `cursor-in-non-selected-windows`. Default t.
pub const BUFFER_SLOT_CURSOR_IN_NON_SELECTED_WINDOWS: BufferSlot = BufferSlot::new(39);
/// Slot index for `left-margin-width`. Default nil.
pub const BUFFER_SLOT_LEFT_MARGIN_WIDTH: BufferSlot = BufferSlot::new(40);
/// Slot index for `right-margin-width`. Default nil.
pub const BUFFER_SLOT_RIGHT_MARGIN_WIDTH: BufferSlot = BufferSlot::new(41);
/// Slot index for `left-fringe-width`. Default nil.
pub const BUFFER_SLOT_LEFT_FRINGE_WIDTH: BufferSlot = BufferSlot::new(42);
/// Slot index for `right-fringe-width`. Default nil.
pub const BUFFER_SLOT_RIGHT_FRINGE_WIDTH: BufferSlot = BufferSlot::new(43);
/// Slot index for `fringes-outside-margins`. Default nil.
pub const BUFFER_SLOT_FRINGES_OUTSIDE_MARGINS: BufferSlot = BufferSlot::new(44);
/// Slot index for `scroll-bar-width`. Default nil.
pub const BUFFER_SLOT_SCROLL_BAR_WIDTH: BufferSlot = BufferSlot::new(45);
/// Slot index for `scroll-bar-height`. Default nil.
pub const BUFFER_SLOT_SCROLL_BAR_HEIGHT: BufferSlot = BufferSlot::new(46);
/// Slot index for `vertical-scroll-bar`. Default t.
pub const BUFFER_SLOT_VERTICAL_SCROLL_BAR: BufferSlot = BufferSlot::new(47);
/// Slot index for `horizontal-scroll-bar`. Default t.
pub const BUFFER_SLOT_HORIZONTAL_SCROLL_BAR: BufferSlot = BufferSlot::new(48);
/// Slot index for `indicate-empty-lines`. Default nil.
pub const BUFFER_SLOT_INDICATE_EMPTY_LINES: BufferSlot = BufferSlot::new(49);
/// Slot index for `indicate-buffer-boundaries`. Default nil.
pub const BUFFER_SLOT_INDICATE_BUFFER_BOUNDARIES: BufferSlot = BufferSlot::new(50);
/// Slot index for `fringe-indicator-alist`. Default nil.
pub const BUFFER_SLOT_FRINGE_INDICATOR_ALIST: BufferSlot = BufferSlot::new(51);
/// Slot index for `fringe-cursor-alist`. Default nil.
///
/// Cursor audit Finding 14 in `drafts/cursor-audit.md`: this
/// buffer-local slot exists and is registered as a forwarder, but
/// nothing in the layout engine or wgpu renderer reads it. GNU
/// uses it in `draw_fringe_bitmap_1` /
/// `get_logical_cursor_bitmap` to map fringe indicator types to
/// cursor bitmaps. Wiring it requires the fringe bitmap resolver,
/// which is itself still mostly stubbed.
pub const BUFFER_SLOT_FRINGE_CURSOR_ALIST: BufferSlot = BufferSlot::new(52);
/// Slot index for `scroll-up-aggressively`. Default nil.
pub const BUFFER_SLOT_SCROLL_UP_AGGRESSIVELY: BufferSlot = BufferSlot::new(53);
/// Slot index for `scroll-down-aggressively`. Default nil.
pub const BUFFER_SLOT_SCROLL_DOWN_AGGRESSIVELY: BufferSlot = BufferSlot::new(54);
/// Slot index for `cache-long-scans`. Default t.
pub const BUFFER_SLOT_CACHE_LONG_SCANS: BufferSlot = BufferSlot::new(55);
/// Slot index for `local-abbrev-table`. Default nil.
pub const BUFFER_SLOT_LOCAL_ABBREV_TABLE: BufferSlot = BufferSlot::new(56);
/// Slot index for `buffer-display-table`. Default nil.
pub const BUFFER_SLOT_BUFFER_DISPLAY_TABLE: BufferSlot = BufferSlot::new(57);
/// Slot index for `buffer-file-coding-system`. Default nil
/// (permanent).
pub const BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM: BufferSlot = BufferSlot::new(58);
/// Slot index for the buffer's syntax table (`BVAR(buf, syntax_table)`
/// in GNU `buffer.h:391`). Not exposed as a Lisp variable in GNU —
/// accessed only via `(syntax-table)` / `(set-syntax-table)`. Conditional
/// per GNU `buffer.c:4758` (`PER_BUFFER_VAR_IDX(syntax_table)`).
pub const BUFFER_SLOT_SYNTAX_TABLE: BufferSlot = BufferSlot::new(59);
/// Slot index for the buffer's category table (`BVAR(buf, category_table)`
/// in GNU `buffer.h:394`). Not exposed as a Lisp variable in GNU —
/// accessed only via `(category-table)` / `(set-category-table)`. Conditional
/// per GNU `buffer.c:4760`.
pub const BUFFER_SLOT_CATEGORY_TABLE: BufferSlot = BufferSlot::new(60);
/// Slot index for the buffer's case table (combined downcase/upcase/
/// canonicalize/equivalence as extras of a single char-table —
/// NeoMacs's collapse of GNU's 4-slot design in `buffer.h:408-417`).
/// Not exposed as a Lisp variable; accessed via `(current-case-table)`
/// / `(set-case-table)`. Always-local per GNU `buffer.c:4731-4734`
/// (flag=0 means every buffer has its own value, no conditional gate).
pub const BUFFER_SLOT_CASE_TABLE: BufferSlot = BufferSlot::new(61);

// ---------------------------------------------------------------------------
// BUFFER_SLOT_INFO table — declarative metadata for every BUFFER_OBJFWD
// slot. Mirrors GNU's `buffer_local_flags` + `defvar_per_buffer` table
// in `buffer.c:5056-5500`. Used by Phase 10C dispatch in `set_buffer_local`,
// `get_buffer_local`, `get_buffer_local_binding`, `ordered_buffer_local_*`
// and by the install loop in `Context::new` that flips the symbols to
// `SymbolRedirect::Forwarded`.
// ---------------------------------------------------------------------------

/// Default-value descriptor. Stored in the const table because
/// `Value::string` and `Value::fixnum` aren't `const`-friendly.
/// Materialised once at startup via [`SlotDefault::to_value`].
#[derive(Copy, Clone, Debug)]
pub enum SlotDefault {
    /// Use a const `Value` (NIL, T).
    Const(crate::emacs_core::value::Value),
    /// Encode an integer fixnum at install time.
    LazyFixnum(i64),
    /// Allocate a multibyte Lisp string at install time.
    LazyString(&'static str),
    /// Allocate a unibyte Lisp string at install time. Mirrors GNU's
    /// `make_unibyte_string` for `default-directory` during dump.
    LazyUnibyte(&'static str),
    /// Resolve to an interned symbol at install time.
    LazySymbol(&'static str),
    /// Resolve to the process's current working directory as a
    /// unibyte string with a trailing slash. Mirrors GNU
    /// `init_buffer_once`'s setup of `default-directory`
    /// (`buffer.c:5381`).
    LazyCwd,
}

impl SlotDefault {
    /// Materialise the default into a runtime [`Value`]. Called once at
    /// startup; the produced Value is GC-rooted by the buffer slot
    /// table from then on.
    pub fn to_value(self) -> crate::emacs_core::value::Value {
        use crate::emacs_core::value::Value;
        match self {
            SlotDefault::Const(v) => v,
            SlotDefault::LazyFixnum(n) => Value::fixnum(n),
            SlotDefault::LazyString(s) => Value::string(s),
            SlotDefault::LazyUnibyte(s) => Value::unibyte_string(s),
            SlotDefault::LazySymbol(s) => Value::symbol(s),
            SlotDefault::LazyCwd => {
                let mut s = std::env::current_dir()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_else(|_| "/".to_string());
                if !s.ends_with('/') {
                    s.push('/');
                }
                Value::unibyte_string(s)
            }
        }
    }
}

/// The closed predicate domain accepted by GNU's `DEFVAR_PER_BUFFER`.
///
/// GNU stores this as `enum Lisp_Fwd_Predicate`; keeping the same domain as a
/// Rust enum prevents misspelled predicate names and makes new upstream
/// variants an explicit exhaustiveness failure instead of a silent no-op.
#[repr(u8)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BufferSlotPredicate {
    Unrestricted,
    String,
    Symbol,
    Integer,
    Number,
    Fraction,
    VerticalScrollBar,
    OverwriteMode,
}

/// A predicate failure independent of the evaluator's non-local-control-flow
/// representation.  The evaluator maps this typed result to GNU-compatible
/// Lisp signal data at the storage boundary.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum BufferSlotPredicateError {
    WrongType(&'static str),
    Choice(&'static str),
    Range(&'static str),
}

impl BufferSlotPredicate {
    /// Check a value before it is stored in a live buffer slot.
    ///
    /// GNU's `store_symval_forwarding` bypasses every predicate for nil, even
    /// predicates such as `stringp` and `integerp` that would reject nil when
    /// called as ordinary Lisp functions.
    pub fn check(self, value: Value) -> Result<(), BufferSlotPredicateError> {
        use BufferSlotPredicateError::{Choice, Range, WrongType};

        if value.is_nil() || self == Self::Unrestricted {
            return Ok(());
        }

        match self {
            Self::Unrestricted => Ok(()),
            Self::String if value.is_string() => Ok(()),
            Self::String => Err(WrongType("stringp")),
            Self::Symbol if value.is_symbol() => Ok(()),
            Self::Symbol => Err(WrongType("symbolp")),
            Self::Integer if value.is_integer() => Ok(()),
            Self::Integer => Err(WrongType("integerp")),
            Self::Number if value.is_number() => Ok(()),
            Self::Number => Err(WrongType("numberp")),
            Self::Fraction if !value.is_number() => Err(WrongType("numberp")),
            Self::Fraction => {
                let in_range = match value.kind() {
                    ValueKind::Fixnum(number) => (0..=1).contains(&number),
                    ValueKind::Float => (0.0..=1.0).contains(&value.xfloat()),
                    ValueKind::Veclike(crate::tagged::header::VecLikeType::Bignum) => value
                        .as_bignum()
                        .is_some_and(|number| (&0..=&1).contains(&number)),
                    _ => unreachable!("numberp was checked above"),
                };
                if in_range {
                    Ok(())
                } else {
                    Err(Range("Value should be from 0.0 to 1.0"))
                }
            }
            Self::VerticalScrollBar
                if value.is_t()
                    || value.is_symbol_named("left")
                    || value.is_symbol_named("right") =>
            {
                Ok(())
            }
            Self::VerticalScrollBar => {
                Err(Choice("One of nil, t, left or right should be specified"))
            }
            Self::OverwriteMode
                if value.is_symbol_named("overwrite-mode-textual")
                    || value.is_symbol_named("overwrite-mode-binary") =>
            {
                Ok(())
            }
            Self::OverwriteMode => Err(Choice(
                "One of nil, overwrite-mode-textual or overwrite-mode-binary should be specified",
            )),
        }
    }
}

/// Per-slot metadata. Mirrors a GNU `defvar_per_buffer` entry.
#[derive(Copy, Clone, Debug)]
pub struct BufferSlotInfo {
    /// Lisp variable name (also used as the obarray symbol name).
    pub name: &'static str,
    /// Index into [`Buffer::slots`].
    pub offset: BufferSlot,
    /// Default value installed into every fresh buffer's slot.
    pub default: SlotDefault,
    /// Predicate checked by `store_symval_forwarding` on writes to a live
    /// buffer slot.
    pub predicate: BufferSlotPredicate,
    /// Whether `kill-all-local-variables` resets this *always-local*
    /// slot back to its default. Mirrors the explicit reset block at
    /// the top of GNU's `reset_buffer_local_variables`
    /// (`buffer.c:1143-1158`), which sets `bset_major_mode`,
    /// `bset_mode_name`, `bset_invisibility_spec`, the case tables,
    /// and the keymap. Other always-local slots
    /// (`buffer-file-name`, `default-directory`, etc.) are NOT
    /// reset and this flag stays `false` for them.
    ///
    /// **For conditional slots (`local_flags_idx >= 0`), use
    /// `permanent_local` instead** — conditional slots are reset by
    /// default and `permanent_local: true` opts them out.
    pub reset_on_kill: bool,
    /// Whether this *conditional* slot should be preserved across
    /// `kill-all-local-variables`. Mirrors GNU's
    /// `buffer_permanent_local_flags[idx]` table
    /// (`buffer.c:109,4751,4767`). Only `truncate-lines` and
    /// `buffer-file-coding-system` are marked permanent in upstream
    /// GNU; both survive the major-mode change.
    ///
    /// For always-local slots this field is ignored — always-local
    /// slots are governed by `reset_on_kill`.
    pub permanent_local: bool,
    /// GNU `buffer_local_flags` index. Mirrors `buffer.c:4703-4791`:
    /// - `-1`: always-local — every buffer has its own value, the
    ///   slot is authoritative without consulting `local_flags`.
    /// - `>= 0`: conditional — the slot only holds a per-buffer
    ///   value when the corresponding bit in
    ///   [`Buffer::local_flags`] is set; otherwise reads fall
    ///   through to [`BufferManager::buffer_defaults`].
    ///
    /// Phase 10A-C only used the always-local arm; Phase 10D adds
    /// conditional slots. The numeric index also serves as the bit
    /// position in `Buffer::local_flags` (NeoMacs collapses GNU's
    /// separate offset and `local_flags_idx` to keep dispatch a
    /// single bit shift).
    pub local_flags_idx: i16,
    /// Whether `install_buffer_objfwd` should install a FORWARDED
    /// symbol for this slot's `name`. GNU's DEFVAR_PER_BUFFER entries
    /// all become forwarded symbols (`syntax-table` / `category-table`
    /// / case tables are NOT DEFVAR_PER_BUFFER — they live in the
    /// BVAR slot block but are only accessible through builtins like
    /// `Fsyntax_table`). Setting this to `false` keeps the slot in
    /// the BVAR block (same storage, GC tracing, pdump round-trip)
    /// but leaves the symbol of that name untouched, matching GNU.
    pub install_as_forwarder: bool,
}

/// The complete table of `BUFFER_OBJFWD`-style slots. Phase 10C started
/// with the four names that Phase 8b moved to slots; subsequent Phase
/// 10C commits will add more entries as additional always-local
/// variables migrate from BufferLocals into the slot table.
pub const BUFFER_SLOT_INFO: &[BufferSlotInfo] = &[
    BufferSlotInfo {
        name: "buffer-file-name",
        offset: BUFFER_SLOT_FILE_NAME,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::String,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-auto-save-file-name",
        offset: BUFFER_SLOT_AUTO_SAVE_FILE_NAME,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::String,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-read-only",
        offset: BUFFER_SLOT_READ_ONLY,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "enable-multibyte-characters",
        offset: BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-file-truename",
        offset: BUFFER_SLOT_FILE_TRUENAME,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::String,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU buffer.c:5381 — default-directory defaults to the
        // process cwd resolved at startup. The slot table can't
        // compute that at const time so we use SlotDefault::LazyCwd
        // which calls std::env::current_dir() at install time.
        name: "default-directory",
        offset: BUFFER_SLOT_DEFAULT_DIRECTORY,
        default: SlotDefault::LazyCwd,
        predicate: BufferSlotPredicate::String,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-saved-size",
        offset: BUFFER_SLOT_SAVED_SIZE,
        default: SlotDefault::LazyFixnum(0),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-backed-up",
        offset: BUFFER_SLOT_BACKED_UP,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-file-format",
        offset: BUFFER_SLOT_FILE_FORMAT,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-auto-save-file-format",
        offset: BUFFER_SLOT_AUTO_SAVE_FILE_FORMAT,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "major-mode",
        offset: BUFFER_SLOT_MAJOR_MODE,
        default: SlotDefault::LazySymbol("fundamental-mode"),
        predicate: BufferSlotPredicate::Symbol,
        reset_on_kill: true,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "local-minor-modes",
        offset: BUFFER_SLOT_LOCAL_MINOR_MODES,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "mode-name",
        offset: BUFFER_SLOT_MODE_NAME,
        default: SlotDefault::LazyString("Fundamental"),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: true,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "mark-active",
        offset: BUFFER_SLOT_MARK_ACTIVE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "point-before-scroll",
        offset: BUFFER_SLOT_POINT_BEFORE_SCROLL,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-display-count",
        offset: BUFFER_SLOT_DISPLAY_COUNT,
        default: SlotDefault::LazyFixnum(0),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        name: "buffer-display-time",
        offset: BUFFER_SLOT_DISPLAY_TIME,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU sets this to t (a magic-bag value), not nil. The
        // legacy ALWAYS_LOCAL_BUFFER_LOCAL_NAMES table also used
        // Value::T, matching `init_buffer_once`.
        name: "buffer-invisibility-spec",
        offset: BUFFER_SLOT_INVISIBILITY_SPEC,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: true,
        local_flags_idx: -1,
        install_as_forwarder: true,
        permanent_local: false,
    },
    // Phase 10D conditional slots --------------------------------
    BufferSlotInfo {
        // GNU `buffer.c:4866` — fill_column defaults to 70.
        // GNU `buffer.c:4754` assigns this slot a positive index
        // in `buffer_local_flags`. NeoMacs reuses `offset` as the
        // bit index in `Buffer::local_flags`.
        name: "fill-column",
        offset: BUFFER_SLOT_FILL_COLUMN,
        default: SlotDefault::LazyFixnum(70),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_FILL_COLUMN.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4848` — tab-width defaults to 8.
        name: "tab-width",
        offset: BUFFER_SLOT_TAB_WIDTH,
        default: SlotDefault::LazyFixnum(8),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_TAB_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4867` — left-margin defaults to 0.
        name: "left-margin",
        offset: BUFFER_SLOT_LEFT_MARGIN,
        default: SlotDefault::LazyFixnum(0),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_LEFT_MARGIN.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4835` — abbrev-mode defaults to nil.
        name: "abbrev-mode",
        offset: BUFFER_SLOT_ABBREV_MODE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_ABBREV_MODE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4836` — overwrite-mode defaults to nil.
        name: "overwrite-mode",
        offset: BUFFER_SLOT_OVERWRITE_MODE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::OverwriteMode,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_OVERWRITE_MODE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4838` — selective-display defaults to nil.
        name: "selective-display",
        offset: BUFFER_SLOT_SELECTIVE_DISPLAY,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SELECTIVE_DISPLAY.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4839` — selective-display-ellipses defaults to t.
        name: "selective-display-ellipses",
        offset: BUFFER_SLOT_SELECTIVE_DISPLAY_ELLIPSES,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SELECTIVE_DISPLAY_ELLIPSES.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4849` — truncate-lines defaults to nil.
        // GNU `buffer.c:4751` flags this as `permanent_local`; the
        // `permanent_local` semantics aren't yet wired (Phase 10D
        // step 5+ will add a dedicated field), so for now we leave
        // `reset_on_kill` false to mirror the most common path.
        name: "truncate-lines",
        offset: BUFFER_SLOT_TRUNCATE_LINES,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_TRUNCATE_LINES.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: true,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4850` — word-wrap defaults to nil.
        name: "word-wrap",
        offset: BUFFER_SLOT_WORD_WRAP,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_WORD_WRAP.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4851` — ctl-arrow defaults to t.
        name: "ctl-arrow",
        offset: BUFFER_SLOT_CTL_ARROW,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_CTL_ARROW.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4837` — auto-fill-function defaults to nil.
        name: "auto-fill-function",
        offset: BUFFER_SLOT_AUTO_FILL_FUNCTION,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_AUTO_FILL_FUNCTION.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4832` — mode-line-format defaults to "%-".
        // Layout engine reads via `effective_buffer_value`, which
        // was updated to consult the slot table directly.
        name: "mode-line-format",
        offset: BUFFER_SLOT_MODE_LINE_FORMAT,
        default: SlotDefault::LazyString("%-"),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_MODE_LINE_FORMAT.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4833` — header-line-format defaults to nil.
        name: "header-line-format",
        offset: BUFFER_SLOT_HEADER_LINE_FORMAT,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_HEADER_LINE_FORMAT.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4834` — tab-line-format defaults to nil.
        name: "tab-line-format",
        offset: BUFFER_SLOT_TAB_LINE_FORMAT,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_TAB_LINE_FORMAT.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    //
    // Phase 10D step 5 batch 2 — display/bidi/fringe/scroll-bar slots.
    BufferSlotInfo {
        // GNU `buffer.c:4852` — bidi-display-reordering defaults to t.
        name: "bidi-display-reordering",
        offset: BUFFER_SLOT_BIDI_DISPLAY_REORDERING,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BIDI_DISPLAY_REORDERING.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4853` — bidi-paragraph-direction defaults to nil.
        name: "bidi-paragraph-direction",
        offset: BUFFER_SLOT_BIDI_PARAGRAPH_DIRECTION,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BIDI_PARAGRAPH_DIRECTION.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4854` — bidi-paragraph-start-re defaults to nil.
        name: "bidi-paragraph-start-re",
        offset: BUFFER_SLOT_BIDI_PARAGRAPH_START_RE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BIDI_PARAGRAPH_START_RE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4855` — bidi-paragraph-separate-re defaults to nil.
        name: "bidi-paragraph-separate-re",
        offset: BUFFER_SLOT_BIDI_PARAGRAPH_SEPARATE_RE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BIDI_PARAGRAPH_SEPARATE_RE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4856` — cursor-type defaults to t.
        name: "cursor-type",
        offset: BUFFER_SLOT_CURSOR_TYPE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_CURSOR_TYPE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4857` — extra-line-spacing defaults to nil.
        name: "line-spacing",
        offset: BUFFER_SLOT_LINE_SPACING,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_LINE_SPACING.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4861` — text-conversion-style defaults to nil.
        name: "text-conversion-style",
        offset: BUFFER_SLOT_TEXT_CONVERSION_STYLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_TEXT_CONVERSION_STYLE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4862` — cursor-in-non-selected-windows defaults to t.
        name: "cursor-in-non-selected-windows",
        offset: BUFFER_SLOT_CURSOR_IN_NON_SELECTED_WINDOWS,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_CURSOR_IN_NON_SELECTED_WINDOWS.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4871` — left-margin-cols defaults to 0.
        name: "left-margin-width",
        offset: BUFFER_SLOT_LEFT_MARGIN_WIDTH,
        default: SlotDefault::LazyFixnum(0),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_LEFT_MARGIN_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4872` — right-margin-cols defaults to 0.
        name: "right-margin-width",
        offset: BUFFER_SLOT_RIGHT_MARGIN_WIDTH,
        default: SlotDefault::LazyFixnum(0),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_RIGHT_MARGIN_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4873` — left-fringe-width defaults to nil.
        name: "left-fringe-width",
        offset: BUFFER_SLOT_LEFT_FRINGE_WIDTH,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_LEFT_FRINGE_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4874` — right-fringe-width defaults to nil.
        name: "right-fringe-width",
        offset: BUFFER_SLOT_RIGHT_FRINGE_WIDTH,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_RIGHT_FRINGE_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4875` — fringes-outside-margins defaults to nil.
        name: "fringes-outside-margins",
        offset: BUFFER_SLOT_FRINGES_OUTSIDE_MARGINS,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_FRINGES_OUTSIDE_MARGINS.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4876` — scroll-bar-width defaults to nil.
        name: "scroll-bar-width",
        offset: BUFFER_SLOT_SCROLL_BAR_WIDTH,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SCROLL_BAR_WIDTH.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4877` — scroll-bar-height defaults to nil.
        name: "scroll-bar-height",
        offset: BUFFER_SLOT_SCROLL_BAR_HEIGHT,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Integer,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SCROLL_BAR_HEIGHT.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4878` — vertical-scroll-bar defaults to t.
        name: "vertical-scroll-bar",
        offset: BUFFER_SLOT_VERTICAL_SCROLL_BAR,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::VerticalScrollBar,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_VERTICAL_SCROLL_BAR.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4879` — horizontal-scroll-bar defaults to t.
        name: "horizontal-scroll-bar",
        offset: BUFFER_SLOT_HORIZONTAL_SCROLL_BAR,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_HORIZONTAL_SCROLL_BAR.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4880` — indicate-empty-lines defaults to nil.
        name: "indicate-empty-lines",
        offset: BUFFER_SLOT_INDICATE_EMPTY_LINES,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_INDICATE_EMPTY_LINES.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4881` — indicate-buffer-boundaries defaults to nil.
        name: "indicate-buffer-boundaries",
        offset: BUFFER_SLOT_INDICATE_BUFFER_BOUNDARIES,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_INDICATE_BUFFER_BOUNDARIES.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4882` — fringe-indicator-alist defaults to nil.
        name: "fringe-indicator-alist",
        offset: BUFFER_SLOT_FRINGE_INDICATOR_ALIST,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_FRINGE_INDICATOR_ALIST.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4883` — fringe-cursor-alist defaults to nil.
        name: "fringe-cursor-alist",
        offset: BUFFER_SLOT_FRINGE_CURSOR_ALIST,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_FRINGE_CURSOR_ALIST.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4884` — scroll-up-aggressively defaults to nil.
        name: "scroll-up-aggressively",
        offset: BUFFER_SLOT_SCROLL_UP_AGGRESSIVELY,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Fraction,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SCROLL_UP_AGGRESSIVELY.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4885` — scroll-down-aggressively defaults to nil.
        name: "scroll-down-aggressively",
        offset: BUFFER_SLOT_SCROLL_DOWN_AGGRESSIVELY,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Fraction,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SCROLL_DOWN_AGGRESSIVELY.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4868` — cache-long-scans defaults to t.
        name: "cache-long-scans",
        offset: BUFFER_SLOT_CACHE_LONG_SCANS,
        default: SlotDefault::Const(crate::emacs_core::value::Value::T),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_CACHE_LONG_SCANS.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4840` — abbrev-table defaults to nil.
        name: "local-abbrev-table",
        offset: BUFFER_SLOT_LOCAL_ABBREV_TABLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_LOCAL_ABBREV_TABLE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4841` — display-table defaults to nil.
        name: "buffer-display-table",
        offset: BUFFER_SLOT_BUFFER_DISPLAY_TABLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BUFFER_DISPLAY_TABLE.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.c:4865` — buffer-file-coding-system defaults to nil.
        // GNU buffer.c:4767 flags this as `permanent_local`; the
        // permanent semantics are deferred until step 5+ adds the
        // dedicated field.
        name: "buffer-file-coding-system",
        offset: BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM.local_flags_idx(),
        install_as_forwarder: true,
        permanent_local: true,
    },
    // ---------- Internal-only slots (not exposed as Lisp variables) ----------
    //
    // These slots mirror GNU BVAR fields that are NOT `DEFVAR_PER_BUFFER`'d.
    // The slot offset lives in `Buffer::slots[]` so the storage, GC tracing,
    // pdump round-trip, and local_flags machinery all work uniformly, but the
    // `install_as_forwarder: false` flag tells the install loop to leave the
    // corresponding symbol alone — matching GNU where `(symbol-value
    // 'syntax-table)` signals void-variable.
    BufferSlotInfo {
        // GNU `buffer.h:391` `syntax_table_` + `buffer.c:4758` conditional
        // local_flags entry. Read via `Fsyntax_table` / written via
        // `Fset_syntax_table` (which also `SET_PER_BUFFER_VALUE_P`).
        name: "syntax-table",
        offset: BUFFER_SLOT_SYNTAX_TABLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_SYNTAX_TABLE.local_flags_idx(),
        install_as_forwarder: false,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.h:394` `category_table_` + `buffer.c:4760` conditional
        // local_flags entry. Read via `Fcategory_table` / written via
        // `Fset_category_table`.
        name: "category-table",
        offset: BUFFER_SLOT_CATEGORY_TABLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: BUFFER_SLOT_CATEGORY_TABLE.local_flags_idx(),
        install_as_forwarder: false,
        permanent_local: false,
    },
    BufferSlotInfo {
        // GNU `buffer.h:408-417` `downcase_table_` / `upcase_table_` /
        // `case_canon_table_` / `case_eqv_table_` + `buffer.c:4731-4734`
        // always-local (flag=0) entries. NeoMacs collapses the four GNU
        // slots into a single downcase char-table whose extras[0..2] hold
        // the upcase / canonicalize / equivalence subsidiary tables —
        // the same value shape `Fcurrent_case_table` returns. Read via
        // `Fcurrent_case_table` / written via `Fset_case_table`.
        name: "case-table",
        offset: BUFFER_SLOT_CASE_TABLE,
        default: SlotDefault::Const(crate::emacs_core::value::Value::NIL),
        predicate: BufferSlotPredicate::Unrestricted,
        reset_on_kill: false,
        local_flags_idx: -1,
        install_as_forwarder: false,
        permanent_local: false,
    },
];

/// Look up a [`BufferSlotInfo`] by Lisp variable name. Returns `None`
/// for non-slot-backed names.
///
/// Only returns entries with `install_as_forwarder: true` — the
/// Lisp-visible slots. Internal BVAR-only slots (`syntax-table`,
/// `category-table`, `case-table`) are addressed by their
/// dedicated slot offset constants instead of by name, matching
/// GNU where those symbols signal void-variable if read as Lisp
/// variables.
pub fn lookup_buffer_slot(name: &str) -> Option<&'static BufferSlotInfo> {
    static BUFFER_SLOT_NAME_MAP: OnceLock<FxHashMap<&'static str, &'static BufferSlotInfo>> =
        OnceLock::new();
    BUFFER_SLOT_NAME_MAP
        .get_or_init(|| {
            let mut map = FxHashMap::default();
            for info in BUFFER_SLOT_INFO {
                if info.install_as_forwarder {
                    map.insert(info.name, info);
                }
            }
            map
        })
        .get(name)
        .copied()
}

fn buffer_slot_sym_map() -> &'static [Option<&'static BufferSlotInfo>] {
    static BUFFER_SLOT_SYM_MAP: OnceLock<Box<[Option<&'static BufferSlotInfo>]>> = OnceLock::new();
    BUFFER_SLOT_SYM_MAP
        .get_or_init(|| {
            let mut entries: Vec<Option<&'static BufferSlotInfo>> = Vec::new();
            for info in BUFFER_SLOT_INFO {
                if !info.install_as_forwarder {
                    continue;
                }
                let sym_id = intern(info.name);
                let index = sym_id.0 as usize;
                if entries.len() <= index {
                    entries.resize(index + 1, None);
                }
                entries[index] = Some(info);
            }
            entries.into_boxed_slice()
        })
        .as_ref()
}

pub fn lookup_buffer_slot_by_sym_id(sym_id: SymId) -> Option<&'static BufferSlotInfo> {
    buffer_slot_sym_map()
        .get(sym_id.0 as usize)
        .and_then(|slot| *slot)
}

/// The curated set of per-buffer (`DEFVAR_PER_BUFFER`) variables whose
/// value affects how a window is laid out / painted, mirroring GNU's
/// `struct buffer` display slots (`src/buffer.c:5117-5899`).
///
/// In GNU Emacs there is *no* per-variable "redisplay" flag fired inside
/// the `setq`/`set` store path; instead `redisplay_window` re-reads every
/// live buffer-local display slot each cycle and the current-matrix diff
/// repaints any change (`src/xdisp.c:20535-20566`). Neomacs adds an
/// aggressive optimization GNU lacks — it short-circuits redisplay on an
/// unchanged `RedisplaySignature` — so to remain faithful it must mark
/// redisplay dirty (the analogue of GNU's `bset_redisplay` /
/// `windows_or_buffers_changed`) whenever one of these slots is written.
///
/// This is the per-buffer half of the display-affecting variable set
/// consulted by the variable-set chokepoint. Names match the GNU Lisp
/// names exactly. `enable-multibyte-characters` is deliberately excluded:
/// it is set through a dedicated path (`set-buffer-multibyte`) that
/// already nudges redisplay, and a plain `setq` of it is rejected by GNU.
pub const DISPLAY_AFFECTING_BUFFER_SLOTS: &[&str] = &[
    // Mode / header / tab lines — change the chrome rows.
    "mode-line-format",
    "header-line-format",
    "tab-line-format",
    // Text layout.
    "tab-width",
    "left-margin",
    "ctl-arrow",
    "truncate-lines",
    "word-wrap",
    "selective-display",
    "selective-display-ellipses",
    "buffer-display-table",
    "line-spacing",
    // NOTE: `fill-column` is `DEFVAR_PER_BUFFER` in GNU but is NOT read by
    // `redisplay_window`/the iterator when laying out text — it only drives
    // fill commands and `display-fill-column-indicator` (which reads it
    // live). GNU does not repaint on a `fill-column` change, so it is
    // excluded here to avoid spurious redisplays (e.g. enriched-mode sets it
    // during HELLO-buffer setup; an extra redisplay then painted the mode
    // line at a transient narrower width).
    // Bidi reordering parameters.
    "bidi-display-reordering",
    "bidi-paragraph-direction",
    "bidi-paragraph-start-re",
    "bidi-paragraph-separate-re",
    // Margins, fringes, scroll bars.
    "left-margin-width",
    "right-margin-width",
    "left-fringe-width",
    "right-fringe-width",
    "fringes-outside-margins",
    "scroll-bar-width",
    "scroll-bar-height",
    "vertical-scroll-bar",
    "horizontal-scroll-bar",
    "indicate-empty-lines",
    "indicate-buffer-boundaries",
    "fringe-indicator-alist",
    "fringe-cursor-alist",
    "scroll-up-aggressively",
    "scroll-down-aggressively",
    // Cursor appearance.
    "cursor-type",
    "cursor-in-non-selected-windows",
    // Invisibility spec changes which text is shown.
    "buffer-invisibility-spec",
];

/// The curated set of *global* (`DEFVAR_LISP`) variables that affect
/// display and so must mark redisplay dirty when set, mirroring GNU.
/// Unlike the per-buffer slots above these are not `struct buffer`
/// fields; GNU reads them directly during layout. `redisplay_window`
/// has no signature short-circuit so GNU needs no flag — but neomacs
/// does. Kept conservative: only variables read by the layout/iterator
/// path are listed, to avoid over-triggering redisplay.
pub const DISPLAY_AFFECTING_GLOBAL_VARS: &[&str] = &[
    // Layout/iterator inputs that change how currently-displayed text is laid
    // out the moment they change.
    "truncate-partial-width-windows",
    "line-prefix",
    "wrap-prefix",
    "default-text-properties",
    // Buffer-local face remapping (`face-remap-add-relative`, e.g. per-buffer
    // background). Changing it re-realizes every face for the buffer's
    // windows; without invalidation here a retained window keeps its stale
    // cached faces, so the remap never takes effect and split windows can
    // disagree (issue #142).
    "face-remapping-alist",
    "display-line-numbers",
    "display-line-numbers-width",
    "display-line-numbers-widen",
    "display-fill-column-indicator",
    "display-fill-column-indicator-column",
    "display-fill-column-indicator-character",
    "show-trailing-whitespace",
    "indicate-empty-lines",
    "overlay-arrow-position",
    "overlay-arrow-string",
    "ctl-arrow",
    "glyphless-char-display",
    "nobreak-char-display",
    // The `*-format` and per-buffer-mirrored display names below are also
    // settable as globals (the buffer default); marking on the default keeps
    // windows reading that default in sync.
    "mode-line-format",
    "header-line-format",
    "tab-line-format",
    "tab-width",
    "truncate-lines",
    "word-wrap",
    "bidi-display-reordering",
    "bidi-paragraph-direction",
    "fringe-indicator-alist",
    "fringe-cursor-alist",
    "cursor-in-non-selected-windows",
    // NOTE: the `scroll-*`/`hscroll-*` variables (scroll-margin,
    // scroll-conservatively, scroll-step, hscroll-margin, hscroll-step) are
    // deliberately EXCLUDED. GNU does not repaint a window when they change —
    // they only influence where the *next* scroll lands, not the layout of
    // the text currently on screen. `blink-cursor-mode` and
    // `void-text-area-pointer` are likewise excluded: the cursor blink is a
    // timer-driven minor mode and the void-area pointer is a mouse-cursor
    // shape, neither of which alters text layout.
];

/// Whether setting the named variable should mark redisplay dirty.
///
/// Returns `true` for the curated per-buffer display slots and the
/// curated global display variables. This is the single source of
/// truth queried by the `set`/`set-default`/buffer-local-set chokepoint
/// so the answer is identical no matter which write path is taken
/// (the tree-walk interpreter, the bytecode VM, `set-default`, custom).
pub fn variable_affects_display(name: &str) -> bool {
    static DISPLAY_VAR_SET: OnceLock<FxHashMap<&'static str, ()>> = OnceLock::new();
    DISPLAY_VAR_SET
        .get_or_init(|| {
            let mut set = FxHashMap::default();
            for &name in DISPLAY_AFFECTING_BUFFER_SLOTS {
                set.insert(name, ());
            }
            for &name in DISPLAY_AFFECTING_GLOBAL_VARS {
                set.insert(name, ());
            }
            set
        })
        .contains_key(name)
}

/// `variable_affects_display` keyed by `SymId`, for the hot variable-set
/// path. Resolves the symbol's name once into a dense `SymId -> bool`
/// table so the chokepoint avoids a string hash on every assignment.
/// The three chrome formats. GNU reaches these through the same
/// `add-variable-watcher` list as the other display variables
/// (lisp/frame.el:3752-3779), but their effect is narrower: changing one
/// invalidates a window's mode / header / tab line specifically, which is the
/// flag redisplay's chrome skip consults.
const CHROME_FORMAT_VARS: &[&str] = &["mode-line-format", "header-line-format", "tab-line-format"];

/// Whether this NAME is one of the chrome formats. The by-name form exists for
/// the window-parameter path, which carries a symbol name rather than the
/// `SymId` the hot variable-set chokepoint has.
pub fn variable_affects_chrome(name: &str) -> bool {
    CHROME_FORMAT_VARS.contains(&name)
}

/// Whether setting this variable invalidates chrome (mode / header / tab
/// line). A subset of [`variable_affects_display_by_sym_id`].
pub fn variable_affects_chrome_by_sym_id(sym_id: SymId) -> bool {
    static CHROME_VAR_BY_SYM: OnceLock<Box<[bool]>> = OnceLock::new();
    CHROME_VAR_BY_SYM
        .get_or_init(|| {
            let mut entries: Vec<bool> = Vec::new();
            for &name in CHROME_FORMAT_VARS {
                let id = intern(name);
                let index = id.0 as usize;
                if entries.len() <= index {
                    entries.resize(index + 1, false);
                }
                entries[index] = true;
            }
            entries.into_boxed_slice()
        })
        .get(sym_id.0 as usize)
        .copied()
        .unwrap_or(false)
}

pub fn variable_affects_display_by_sym_id(sym_id: SymId) -> bool {
    static DISPLAY_VAR_BY_SYM: OnceLock<Box<[bool]>> = OnceLock::new();
    DISPLAY_VAR_BY_SYM
        .get_or_init(|| {
            let mut entries: Vec<bool> = Vec::new();
            let mut mark = |name: &str| {
                let id = intern(name);
                let index = id.0 as usize;
                if entries.len() <= index {
                    entries.resize(index + 1, false);
                }
                entries[index] = true;
            };
            for &name in DISPLAY_AFFECTING_BUFFER_SLOTS {
                mark(name);
            }
            for &name in DISPLAY_AFFECTING_GLOBAL_VARS {
                mark(name);
            }
            entries.into_boxed_slice()
        })
        .get(sym_id.0 as usize)
        .copied()
        .unwrap_or(false)
}

/// Neomacs stores per-buffer slots in a compact Rust table, but GNU exposes
/// their C `struct buffer` order through `buffer-local-variables`: it walks
/// from `name_` through `cursor_in_non_selected_windows_` and prepends each
/// visible slot.  Keep that externally visible walk independent from the
/// internal slot-offset numbering.
const GNU_STRUCT_BUFFER_SLOT_ORDER: &[BufferSlot] = &[
    BUFFER_SLOT_FILE_NAME,
    BUFFER_SLOT_DEFAULT_DIRECTORY,
    BUFFER_SLOT_BACKED_UP,
    BUFFER_SLOT_SAVED_SIZE,
    BUFFER_SLOT_AUTO_SAVE_FILE_NAME,
    BUFFER_SLOT_READ_ONLY,
    BUFFER_SLOT_MAJOR_MODE,
    BUFFER_SLOT_LOCAL_MINOR_MODES,
    BUFFER_SLOT_MODE_NAME,
    BUFFER_SLOT_MODE_LINE_FORMAT,
    BUFFER_SLOT_HEADER_LINE_FORMAT,
    BUFFER_SLOT_TAB_LINE_FORMAT,
    BUFFER_SLOT_LOCAL_ABBREV_TABLE,
    BUFFER_SLOT_SYNTAX_TABLE,
    BUFFER_SLOT_CATEGORY_TABLE,
    BUFFER_SLOT_TAB_WIDTH,
    BUFFER_SLOT_FILL_COLUMN,
    BUFFER_SLOT_LEFT_MARGIN,
    BUFFER_SLOT_AUTO_FILL_FUNCTION,
    BUFFER_SLOT_CASE_TABLE,
    BUFFER_SLOT_TRUNCATE_LINES,
    BUFFER_SLOT_WORD_WRAP,
    BUFFER_SLOT_CTL_ARROW,
    BUFFER_SLOT_BIDI_DISPLAY_REORDERING,
    BUFFER_SLOT_BIDI_PARAGRAPH_DIRECTION,
    BUFFER_SLOT_BIDI_PARAGRAPH_SEPARATE_RE,
    BUFFER_SLOT_BIDI_PARAGRAPH_START_RE,
    BUFFER_SLOT_SELECTIVE_DISPLAY,
    BUFFER_SLOT_SELECTIVE_DISPLAY_ELLIPSES,
    BUFFER_SLOT_OVERWRITE_MODE,
    BUFFER_SLOT_ABBREV_MODE,
    BUFFER_SLOT_BUFFER_DISPLAY_TABLE,
    BUFFER_SLOT_MARK_ACTIVE,
    BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS,
    BUFFER_SLOT_BUFFER_FILE_CODING_SYSTEM,
    BUFFER_SLOT_FILE_FORMAT,
    BUFFER_SLOT_AUTO_SAVE_FILE_FORMAT,
    BUFFER_SLOT_CACHE_LONG_SCANS,
    BUFFER_SLOT_POINT_BEFORE_SCROLL,
    BUFFER_SLOT_FILE_TRUENAME,
    BUFFER_SLOT_INVISIBILITY_SPEC,
    BUFFER_SLOT_DISPLAY_COUNT,
    BUFFER_SLOT_LEFT_MARGIN_WIDTH,
    BUFFER_SLOT_RIGHT_MARGIN_WIDTH,
    BUFFER_SLOT_LEFT_FRINGE_WIDTH,
    BUFFER_SLOT_RIGHT_FRINGE_WIDTH,
    BUFFER_SLOT_FRINGES_OUTSIDE_MARGINS,
    BUFFER_SLOT_SCROLL_BAR_WIDTH,
    BUFFER_SLOT_SCROLL_BAR_HEIGHT,
    BUFFER_SLOT_VERTICAL_SCROLL_BAR,
    BUFFER_SLOT_HORIZONTAL_SCROLL_BAR,
    BUFFER_SLOT_INDICATE_EMPTY_LINES,
    BUFFER_SLOT_INDICATE_BUFFER_BOUNDARIES,
    BUFFER_SLOT_FRINGE_INDICATOR_ALIST,
    BUFFER_SLOT_FRINGE_CURSOR_ALIST,
    BUFFER_SLOT_DISPLAY_TIME,
    BUFFER_SLOT_SCROLL_UP_AGGRESSIVELY,
    BUFFER_SLOT_SCROLL_DOWN_AGGRESSIVELY,
    BUFFER_SLOT_CURSOR_TYPE,
    BUFFER_SLOT_LINE_SPACING,
    BUFFER_SLOT_TEXT_CONVERSION_STYLE,
    BUFFER_SLOT_CURSOR_IN_NON_SELECTED_WINDOWS,
];

fn buffer_slot_info_by_offset(offset: BufferSlot) -> Option<&'static BufferSlotInfo> {
    BUFFER_SLOT_INFO.iter().find(|info| info.offset == offset)
}

/// Lisp name of the undo-list variable; reads and writes route through
/// [`SharedUndoState`], not the plain slot array (see `buffer_local_value`).
pub(crate) const BUFFER_UNDO_LIST_NAME: &str = "buffer-undo-list";

fn buffer_undo_list_sym() -> SymId {
    static SYM: OnceLock<SymId> = OnceLock::new();
    *SYM.get_or_init(|| intern(BUFFER_UNDO_LIST_NAME))
}

/// A Lisp-visible buffer local whose backing store is neither a generic
/// `Buffer::slots` entry nor `local_var_alist`.
///
/// GNU represents `buffer-undo-list` as a `DEFVAR_PER_BUFFER` forwarder. In
/// Neomacs its value must instead live in [`SharedUndoState`] so indirect
/// buffers share one undo history. Keeping that exception in a closed enum
/// lets hot variable-read paths distinguish it by symbol identity without
/// probing every ordinary nil-valued global through the generic buffer-local
/// lookup machinery.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DedicatedBufferLocal {
    UndoList,
}

impl DedicatedBufferLocal {
    #[inline]
    pub(crate) fn from_sym_id(sym_id: SymId) -> Option<Self> {
        (sym_id == buffer_undo_list_sym()).then_some(Self::UndoList)
    }

    #[inline]
    pub(crate) fn read(self, buffer: &Buffer) -> Value {
        match self {
            Self::UndoList => buffer.get_undo_list(),
        }
    }
}

/// Verdict for one entry during [`LocalVariableBindings::retain_bindings`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BindingRetention {
    /// Leave the binding cons linked into the list.
    Keep,
    /// Unlink the binding cons.
    Drop,
}

/// Lisp-visible per-buffer bindings plus a derived identity index.
///
/// GNU keeps these bindings solely in `buffer.local_var_alist` and resolves
/// them with `assq_no_quit` (`data.c:2409`). The Lisp alist remains Neomacs's
/// only source of truth as well; the map only remembers each symbol's first
/// binding cons. Reading the cons cdr preserves GNU's in-place update
/// semantics, including the `Qunbound` marker, without copying binding values
/// into a second store.
///
/// Keeping the alist private to this type makes cache coherence a compile-time
/// property: in-place value changes retain the indexed cons, while every
/// structural replacement invalidates the derived map. Structural edits that
/// unlink interior entries must go through [`Self::retain_bindings`], which
/// splices and invalidates on one path -- [`Self::replace_alist`]'s
/// head-identity fast path cannot see such an edit.
struct LocalVariableBindings {
    alist: Value,
    index: RefCell<Option<FxHashMap<SymId, Value>>>,
}

impl Clone for LocalVariableBindings {
    fn clone(&self) -> Self {
        Self::from_alist(self.alist)
    }
}

impl LocalVariableBindings {
    fn from_alist(alist: Value) -> Self {
        Self {
            alist,
            index: RefCell::new(None),
        }
    }

    fn as_lisp_alist(&self) -> Value {
        self.alist
    }

    fn replace_alist(&mut self, alist: Value) {
        // Identity guard: every LOCALIZED Bind/Set routes through
        // `set_internal_localized`, which either writes an existing entry's
        // cdr IN PLACE (alist head unchanged - the overwhelmingly common
        // case, dozens of times per keystroke via let-bound buffer-locals)
        // or prepends a new cons (new head). It never splices interior
        // entries, so an identical head proves identical structure and the
        // index's SymId -> entry-cons mapping is still live (values are read
        // through cons_cdr at lookup time). Unconditional invalidation here
        // made `has_buffer_local_by_sym_id` rebuild the whole index ~28x per
        // keystroke (~3.2M Ir of the 133M type-sim steady window).
        //
        // A caller that unlinks an INTERIOR entry breaks that premise: the
        // head cons is unchanged, so the guard would keep an index still
        // mapping the unlinked symbol to its orphaned cons. Such callers must
        // not come through here at all -- [`Self::retain_bindings`] performs
        // the whole filter inside this type so the invalidation cannot be
        // separated from the splice.
        if self.alist.bits() == alist.bits() {
            return;
        }
        self.alist = alist;
        *self.index.get_mut() = None;
    }

    /// Filter the binding list in place, keeping the entries for which
    /// `decide` returns [`BindingRetention::Keep`].
    ///
    /// This is GNU's `reset_buffer_local_variables' splice
    /// (`src/buffer.c:1168-1225'): a `last' cursor walks the list and each
    /// dropped entry is unlinked with `XSETCDR (last, XCDR (tmp))', so a
    /// retained entry keeps its original cons cell and any BLV valcell
    /// pointing at it stays live.
    ///
    /// `decide` receives the binding cons `(SYMBOL . VALUE)` itself, so a
    /// caller may also rewrite a retained entry's cdr in place (GNU's
    /// `permanent-local-hook' partial preserve) before returning `Keep`.
    ///
    /// Filtering is the one structural edit that can leave the head cons
    /// identical while unlinking interior entries, which is precisely what
    /// [`Self::replace_alist`]'s identity guard cannot detect. Owning the walk
    /// here makes that hazard unrepresentable: the index is dropped on the
    /// same code path that performs the splice.
    fn retain_bindings(&mut self, mut decide: impl FnMut(Value) -> BindingRetention) {
        let mut head = Value::NIL;
        let mut last: Option<Value> = None;
        let mut cursor = self.alist;
        while cursor.is_cons() {
            let next = cursor.cons_cdr();
            let entry = cursor.cons_car();
            // A non-cons element cannot name a symbol, so it can never be
            // reached through the index; GNU's loop would fault on it. Drop it.
            let keep = entry.is_cons() && matches!(decide(entry), BindingRetention::Keep);
            if keep {
                match last {
                    Some(tail) => tail.set_cdr(cursor),
                    None => head = cursor,
                }
                last = Some(cursor);
            }
            cursor = next;
        }
        if let Some(tail) = last {
            tail.set_cdr(Value::NIL);
        }
        self.alist = head;
        *self.index.get_mut() = None;
    }

    fn binding_cons(&self, sym_id: SymId) -> Option<Value> {
        let mut index = self.index.borrow_mut();
        if index.is_none() {
            let mut rebuilt = FxHashMap::default();
            let mut cursor = self.alist;
            while cursor.is_cons() {
                note_local_var_alist_entry_probe();
                let entry = cursor.cons_car();
                cursor = cursor.cons_cdr();
                if !entry.is_cons() {
                    continue;
                }
                if let Some(id) = entry.cons_car().as_symbol_id() {
                    // `assq` observes the first duplicate entry. Preserve that
                    // behavior even though ordinary mutation APIs never create
                    // duplicates.
                    rebuilt.entry(id).or_insert(entry);
                }
            }
            *index = Some(rebuilt);
        }
        index.as_ref()?.get(&sym_id).copied()
    }

    fn value(&self, sym_id: SymId) -> Option<Value> {
        self.binding_cons(sym_id).map(Value::cons_cdr)
    }

    fn set(&mut self, sym_id: SymId, value: Value) {
        let before = self.alist;
        set_local_var_alist_entry(&mut self.alist, Value::from_sym_id(sym_id), value);
        if self.alist.bits() != before.bits() {
            *self.index.get_mut() = None;
        }
    }

    fn remove(&mut self, sym_id: SymId) {
        remove_local_var_alist_entry(&mut self.alist, Value::from_sym_id(sym_id));
        // Removing a non-head entry leaves the head identity unchanged, so the
        // requested symbol alone cannot prove that an existing map is valid.
        *self.index.get_mut() = None;
    }
}

/// Set `key` to `value` in a buffer-local alist. If `key` already
/// has an entry, mutate its cdr in place so any BLV valcell
/// pointing at the cell sees the new value without re-swapping.
/// Otherwise prepend a fresh `(key . value)` cons to the alist.
/// Mirrors the SYMBOL_LOCALIZED arm of GNU `set_internal` at
/// `data.c:1687-1762`.
fn set_local_var_alist_entry(
    alist: &mut crate::emacs_core::value::Value,
    key: crate::emacs_core::value::Value,
    value: crate::emacs_core::value::Value,
) {
    use crate::emacs_core::value::{Value, eq_value};
    let mut cursor = *alist;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if entry.is_cons() && eq_value(&entry.cons_car(), &key) {
            // In-place cdr write: a BLV valcell pointing at this cons
            // observes the new value directly — no epoch bump needed.
            entry.set_cdr(value);
            return;
        }
    }
    let cell = Value::cons(key, value);
    *alist = Value::cons(cell, *alist);
    // A binding now exists that a same-buffer BLV cache loaded before this
    // prepend cannot see (it may hold defcell/found=false for this buffer).
    crate::emacs_core::symbol::note_blv_alist_structural_mutation();
}

/// Copy Lisp-level buffer-local bindings for a cloned indirect buffer.
///
/// GNU `clone_per_buffer_values` calls `buffer_lisp_local_variables(from, 1)`,
/// which allocates fresh alist cells by walking `from->local_var_alist` and
/// prepending `(SYMBOL . VALUE)` for each entry. Values themselves are not
/// deep-copied, but the alist structure and each binding cons are distinct.
pub(crate) fn clone_lisp_local_variables(
    alist: crate::emacs_core::value::Value,
) -> crate::emacs_core::value::Value {
    use crate::emacs_core::value::Value;

    let mut result = Value::NIL;
    let mut cursor = alist;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if entry.is_cons() {
            result = Value::cons(Value::cons(entry.cons_car(), entry.cons_cdr()), result);
        }
    }
    result
}

/// Remove `key` from a buffer-local alist in place. Mirrors GNU's
/// `Fdelq`-over-`Fassq` pattern in `Fkill_local_variable`
/// (`data.c:2349-2378`).
fn remove_local_var_alist_entry(
    alist: &mut crate::emacs_core::value::Value,
    key: crate::emacs_core::value::Value,
) {
    use crate::emacs_core::value::{Value, eq_value};
    let mut head = *alist;
    let mut prev: Option<Value> = None;
    let mut cursor = head;
    let mut removed = false;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        let next = cursor.cons_cdr();
        if entry.is_cons() && eq_value(&entry.cons_car(), &key) {
            match prev {
                Some(p) => p.set_cdr(next),
                None => head = next,
            }
            removed = true;
        } else {
            prev = Some(cursor);
        }
        cursor = next;
    }
    *alist = head;
    if removed {
        // The removed cons may be some BLV's cached valcell (for this or
        // ANOTHER buffer) — force every cached localized read to re-swap.
        crate::emacs_core::symbol::note_blv_alist_structural_mutation();
    }
}

/// Filter a `(perm-hook ...)` value to keep only entries that
/// are themselves `permanent-local-hook` per their symbol property,
/// plus the `t` element if present. Mirrors GNU
/// `reset_buffer_local_variables`'s permanent-local-hook handling
/// at `buffer.c:1308-1335`. Used by [`Buffer::kill_all_local_variables`]
/// when walking `local_var_alist` for LOCALIZED hook bindings.
pub(crate) fn preserve_partial_permanent_local_hook_value(
    obarray: &crate::emacs_core::symbol::Obarray,
    value: crate::emacs_core::value::Value,
) -> crate::emacs_core::value::Value {
    use crate::emacs_core::value::Value;
    if !value.is_cons() {
        return value;
    }
    let mut preserved = Vec::new();
    let mut cursor = value;
    while cursor.is_cons() {
        let elt = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if elt.is_symbol_named("t")
            || elt.as_symbol_name().is_some_and(|name| {
                obarray
                    .get_property(name, "permanent-local-hook")
                    .is_some_and(|prop| !prop.is_nil())
            })
        {
            preserved.push(elt);
        }
    }
    Value::list(preserved)
}

// ---------------------------------------------------------------------------
// BufferId
// ---------------------------------------------------------------------------

/// Opaque, cheaply-copyable identifier for a buffer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct BufferId(pub u64);

// ---------------------------------------------------------------------------
// InsertionType
// ---------------------------------------------------------------------------

/// Controls whether a marker advances when text is inserted exactly at its
/// position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InsertionType {
    /// Marker stays before the new text (does NOT advance).
    Before,
    /// Marker moves after the new text (advances).
    After,
}

// ---------------------------------------------------------------------------
// BufferStateMarkers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug)]
pub struct BufferStateMarkers {
    pub pt_marker: u64,
    pub begv_marker: u64,
    pub zv_marker: u64,
    /// Non-Lisp-visible MarkerObj pointers for the three state markers.
    /// Allocated once per buffer in `ensure_buffer_state_markers` and
    /// reused on every `record_buffer_state_markers` re-registration
    /// (via `chain_unlink` + `register_marker`) so the intrusive chain
    /// precondition is upheld.
    ///
    /// These MarkerObjs stay alive because
    /// `BufferManager::<GcTrace>::trace_roots` synthesises a tagged
    /// `Value` for each of the three pointers and seeds them into the
    /// GC's root set. Mirrors GNU `mark_buffer` marking the per-buffer
    /// `pt_marker` / `begv_marker` / `zv_marker` BVAR `Lisp_Object`
    /// slots in `alloc.c`.
    ///
    /// The intrusive marker chain by itself is NOT a GC root:
    /// `unchain_dead_markers` splices out any MarkerObj whose mark bit
    /// is clear between `mark_all` and `sweep_objects`. Without the
    /// explicit root above, an unmarked state marker would be unlinked
    /// and freed, leaving `BufferStateMarkers` with a dangling pointer.
    pub pt_marker_ptr: *mut crate::tagged::header::MarkerObj,
    pub begv_marker_ptr: *mut crate::tagged::header::MarkerObj,
    pub zv_marker_ptr: *mut crate::tagged::header::MarkerObj,
}

impl PartialEq for BufferStateMarkers {
    fn eq(&self, other: &Self) -> bool {
        self.pt_marker == other.pt_marker
            && self.begv_marker == other.begv_marker
            && self.zv_marker == other.zv_marker
    }
}
impl Eq for BufferStateMarkers {}

/// Read-only snapshot of buffer text for layout and rendering.
///
/// A snapshot is intentionally separate from [`BufferText`]: callers can read
/// text and text properties, but cannot observe or mutate the concrete text
/// backend. `BufferText::Clone` is a deep snapshot, so this is safe to move
/// across the display boundary.
#[derive(Clone)]
pub struct BufferTextSnapshot {
    text: BufferText,
}

impl BufferTextSnapshot {
    pub fn emacs_byte_len(&self) -> EmacsByteLen {
        self.text.emacs_byte_len()
    }

    pub fn char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos {
        self.text.char_pos_to_emacs_byte_pos(charpos)
    }

    pub fn emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0 {
        self.text.emacs_byte_pos_to_char_pos(bytepos)
    }

    pub fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.text.copy_emacs_byte_range_to(range, out);
    }

    pub fn try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.text.for_each_emacs_byte_range_chunk(range, f)
    }

    pub fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.text.emacs_byte_at_pos(pos)
    }

    pub fn text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value> {
        self.text
            .text_props_get_property_at_emacs_byte_pos(pos, name)
    }

    pub fn next_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text.text_props_next_change_after_emacs_byte_pos(pos)
    }

    pub fn next_single_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_single_change_after_emacs_byte_pos(pos, name)
    }

    /// Display-engine bounded variant; see
    /// [`BufferText::text_props_next_single_change_after_emacs_byte_pos_bounded`].
    pub fn next_single_text_prop_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_single_change_after_emacs_byte_pos_bounded(pos, name, limit)
    }

    pub fn previous_single_text_prop_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_previous_single_change_before_emacs_byte_pos(pos, name)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AccessibleBufferRegionSnapshot {
    start_char: CharPos0,
    start_emacs_byte: EmacsBytePos,
    end_char: CharPos0,
    end_emacs_byte: EmacsBytePos,
}

impl AccessibleBufferRegionSnapshot {
    fn start_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.start_char, self.start_emacs_byte)
    }

    fn end_anchor(self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.end_char, self.end_emacs_byte)
    }

    pub fn start_emacs_byte(self) -> EmacsBytePos {
        self.start_emacs_byte
    }

    pub fn end_emacs_byte(self) -> EmacsBytePos {
        self.end_emacs_byte
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LabeledRestrictionLabel {
    Outermost,
    User(Value),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LabeledRestriction {
    pub label: LabeledRestrictionLabel,
    pub beg_marker: u64,
    pub end_marker: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SavedRestrictionKind {
    None,
    Markers { beg_marker: u64, end_marker: u64 },
}

#[derive(Clone, Debug, PartialEq)]
pub struct SavedRestrictionState {
    pub buffer_id: BufferId,
    pub restriction: SavedRestrictionKind,
    pub labeled_restrictions: Option<Vec<LabeledRestriction>>,
}

impl SavedRestrictionState {
    pub fn trace_roots(&self, roots: &mut Vec<Value>) {
        if let Some(restrictions) = &self.labeled_restrictions {
            for restriction in restrictions {
                if let LabeledRestrictionLabel::User(label) = restriction.label {
                    roots.push(label);
                }
            }
        }
    }

    /// Root the restriction markers themselves. They are referenced only by
    /// u64 ids here, and an unmarked marker is spliced out of its buffer's
    /// chain and freed by `unchain_dead_markers` — after which the saved
    /// bounds silently resolve to the full buffer. GNU keeps these markers
    /// alive through the specpdl's Lisp save_restriction data (editfns.c);
    /// this is the precise-GC equivalent. Called at seed time, before the
    /// mark phase, so every marker is still findable in its chain.
    pub fn trace_marker_roots(&self, buffers: &BufferManager, roots: &mut Vec<Value>) {
        let Some(buffer) = buffers.get(self.buffer_id) else {
            return;
        };
        if let SavedRestrictionKind::Markers {
            beg_marker,
            end_marker,
        } = self.restriction
        {
            if let Some(value) = buffer.marker_value_by_id(beg_marker) {
                roots.push(value);
            }
            if let Some(value) = buffer.marker_value_by_id(end_marker) {
                roots.push(value);
            }
        }
        if let Some(restrictions) = &self.labeled_restrictions {
            for restriction in restrictions {
                if let Some(value) = buffer.marker_value_by_id(restriction.beg_marker) {
                    roots.push(value);
                }
                if let Some(value) = buffer.marker_value_by_id(restriction.end_marker) {
                    roots.push(value);
                }
            }
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct OutermostRestrictionResetState {
    pub affected_buffers: Vec<BufferId>,
}

/// Complete buffer state reconstructed from a pdump image.
///
/// This is intentionally crate-private: pdump loading needs to restore the
/// exact serialized state, but ordinary runtime code should construct buffers
/// through [`Buffer::new`] and mutate text through the Buffer APIs.
pub(crate) struct BufferDumpParts {
    pub id: BufferId,
    pub name: Value,
    pub last_name: Value,
    pub base_buffer: Option<BufferId>,
    pub text: BufferText,
    pub point: TextPositionAnchor,
    pub mark_marker_id: Option<u64>,
    pub mark_marker_ptr: *mut crate::tagged::header::MarkerObj,
    pub accessible_start: TextPositionAnchor,
    pub accessible_end: TextPositionAnchor,
    pub autosave_modified_tick: i64,
    pub last_window_start: LispCharPos1,
    pub last_selected_window: Option<WindowId>,
    pub inhibit_buffer_hooks: bool,
    pub state_markers: Option<BufferStateMarkers>,
    pub local_var_alist: Value,
    pub keymap: Value,
    pub modtime: VisitedFileModtime,
    pub modtime_size: Option<i64>,
    pub slots: [Value; BUFFER_SLOT_COUNT],
    pub local_flags: u64,
    pub overlays: OverlayList,
    pub overlay_modified_tick: i64,
    pub undo_state: SharedUndoState,
    /// The editor-global saved-point cell every restored buffer must share.
    /// The dump does not carry it: GNU's globals are plain statics that start
    /// over on each startup, and there is no command to have run yet.
    pub saved_point_before_command: SavedPointBeforeCommand,
}

// ---------------------------------------------------------------------------
// Buffer
// ---------------------------------------------------------------------------

/// A single text buffer with point, mark, narrowing, markers, and local vars.
#[derive(Clone)]
pub struct Buffer {
    /// Mode-line line-number anchor (GNU w->base_line_pos/base_line_number,
    /// xdisp.c:29486-29620, held per BUFFER here): `(bol_byte, line)` of a
    /// recently displayed point's line start, so %l counts newlines only
    /// from the anchor instead of from the buffer start. `line == 0` means
    /// no anchor. Validity is checked against the unchanged-region
    /// accumulator (the BEG_UNCHANGED analog) at each use.
    pub(crate) line_number_anchor: std::cell::Cell<(usize, i64)>,
    /// Unique identifier.
    pub(crate) id: BufferId,
    /// Buffer name (e.g. `"*scratch*"`). Mirrors GNU `struct buffer.name_`.
    pub(crate) name: Value,
    /// Buffer name before the last rename, or before death after kill.
    /// Mirrors GNU `struct buffer.last_name_` and `BVAR (buf, last_name)`.
    pub(crate) last_name: Value,
    /// Base buffer when this is an indirect buffer.
    pub(crate) base_buffer: Option<BufferId>,
    /// The underlying text storage.
    pub(in crate::buffer) text: BufferText,
    /// Point — paired character and Emacs-byte cursor position.
    pub(crate) point: TextPositionAnchor,
    /// GNU `BVAR(buf, mark)` — a real Lisp_Marker.  This IS the mark;
    /// there are no separate mark position fields (matching GNU's design).
    /// The marker tracks its own position through the chain; read it via
    /// `mark_byte()` / `mark_char()` / `mark()`.
    pub(crate) mark_marker_id: Option<u64>,
    /// Cached raw pointer to the mark MarkerObj, for fast position reads
    /// and GC root tracing (mirrors BufferStateMarkers pointers).
    pub(crate) mark_marker_ptr: *mut crate::tagged::header::MarkerObj,
    /// Beginning of accessible (narrowed) portion, inclusive.
    pub(crate) accessible_start: TextPositionAnchor,
    /// End of accessible (narrowed) portion, exclusive.
    pub(crate) accessible_end: TextPositionAnchor,
    /// GNU `BUF_AUTOSAVE_MODIFF`: recent auto-save state is
    /// `save_modiff < autosave_modified_tick`.
    pub(crate) autosave_modified_tick: i64,
    /// GNU `last_window_start`: start position of the most recently
    /// disconnected window that showed this buffer.
    pub(crate) last_window_start: LispCharPos1,
    /// GNU `last_selected_window`: most recently selected live window showing
    /// this buffer, when known.
    pub(crate) last_selected_window: Option<WindowId>,
    /// GNU `inhibit_buffer_hooks`: suppress buffer lifecycle hooks for
    /// temporary/internal buffers.
    pub(crate) inhibit_buffer_hooks: bool,
    /// GNU-style noncurrent PT/BEGV/ZV markers for buffers that share text.
    pub(crate) state_markers: Option<BufferStateMarkers>,
    /// `local_var_alist` — list of `(SYMBOL . VALUE)` per-buffer
    /// bindings for `SYMBOL_LOCALIZED` variables. Mirrors GNU
    /// `BVAR(buffer, local_var_alist)` (`buffer.h:362`). This is
    /// the single source of truth for all Lisp-side per-buffer
    /// bindings that are not slot-backed (FORWARDED) and not the
    /// special buffer-undo-list (which has its own SharedUndoState).
    local_var_alist: LocalVariableBindings,
    /// `BVAR(buffer, keymap)` — the buffer's local keymap
    /// (`buffer.h:385`). `Value::NIL` when no local keymap is set.
    pub(crate) keymap: crate::emacs_core::value::Value,
    /// GNU `b->modtime` (`src/buffer.h:645-655`) plus the `b->base_buffer`
    /// link `record_first_change` follows to read it (`src/undo.c:213-214`).
    /// Reached only through [`Self::visited_file_modtime`],
    /// [`Self::set_visited_file_modtime`] and
    /// [`Self::first_change_modtime`], which are GNU's three readers.
    modtime: VisitedFileModtimeSlot,
    /// File size in bytes when `modtime` was captured.
    pub(crate) modtime_size: Option<i64>,
    /// `BUFFER_OBJFWD` slot table — per-buffer storage for variables
    /// that are forwarded into the C-side `struct buffer` in GNU.
    /// Mirrors the union of GNU's `Lisp_Object` slot fields in
    /// `buffer.h:319-462`. Indexed by [`crate::emacs_core::forward::LispBufferObjFwd::offset`].
    ///
    /// Phase 8a of the symbol-redirect refactor adds the slot table.
    /// Phase 8b will migrate the hardcoded fields ([`Self::file_name`],
    /// [`Self::auto_save_file_name`], [`Self::read_only`],
    /// [`Self::multibyte`]) into slots and remove the duplicates.
    pub(crate) slots: [crate::emacs_core::value::Value; BUFFER_SLOT_COUNT],
    /// Per-slot "is buffer-local in this buffer" bitmap. Bit `N` is
    /// set when this buffer has its own local value for the slot at
    /// offset `N`. Mirrors GNU `b->local_flags[]` (`buffer.h:646`,
    /// `char[MAX_PER_BUFFER_VARS]`); we use a `u64` bitmap because
    /// `BUFFER_SLOT_COUNT == 64`.
    ///
    /// **Semantics** (mirrors `set_internal` SYMBOL_FORWARDED arm at
    /// `data.c:1764-1791`):
    /// - Always-local slots (`local_flags_idx == -1`) ignore this
    ///   bitmap entirely; the slot is authoritative.
    /// - Conditional slots (`local_flags_idx >= 0`): a read returns
    ///   `slots[N]` iff bit `N` is set, otherwise the global default
    ///   from `Context::buffer_defaults[N]`. A write sets the bit
    ///   and writes the slot.
    ///
    /// Phase 10D wires the bitmap up; Phase 10A-C only used the
    /// always-local arm.
    pub(crate) local_flags: u64,
    /// Overlays attached to the buffer.
    pub(crate) overlays: OverlayList,
    /// GNU `BUF_OVERLAY_MODIFF`: incremented when live overlay ranges or
    /// properties change so redisplay observes overlay-only UI updates.
    pub(crate) overlay_modified_tick: i64,
    /// Shared undo owner for this text.
    pub(crate) undo_state: SharedUndoState,
    /// Handle on the editor's ONE saved point-before-command-or-undo, GNU's
    /// `point_before_last_command_or_undo` / `buffer_before_last_command_or_undo`
    /// globals (src/keyboard.c:232-233).  Every buffer a `BufferManager` owns
    /// holds a clone of the same cell, so a command-loop iteration in one
    /// buffer supersedes the point saved for every other one -- see
    /// [`SavedPointBeforeCommand`].
    pub(crate) saved_point_before_command: SavedPointBeforeCommand,
}

impl Buffer {
    /// Return the chartable Value stored in this buffer's syntax-table
    /// slot. Mirrors GNU `BVAR (buf, syntax_table)` — reading directly
    /// from `buffer->syntax_table` without any compiled shadow form.
    /// Falls back to `Value::NIL` for fresh buffers; callers that need
    /// the standard defaults should go through
    /// `current_buffer_syntax_table_object_in_buffers`, which seeds
    /// the slot on first access.
    pub fn syntax_chartable(&self) -> Value {
        self.slots[BUFFER_SLOT_SYNTAX_TABLE.index()]
    }

    fn swap_slot_with(&mut self, other: &mut Self, slot: BufferSlot) {
        mem::swap(
            &mut self.slots[slot.index()],
            &mut other.slots[slot.index()],
        );
    }

    fn swap_owned_text_state_with(&mut self, other: &mut Self) {
        mem::swap(&mut self.text, &mut other.text);
        mem::swap(&mut self.point, &mut other.point);
        mem::swap(&mut self.accessible_start, &mut other.accessible_start);
        mem::swap(&mut self.accessible_end, &mut other.accessible_end);
        mem::swap(&mut self.mark_marker_id, &mut other.mark_marker_id);
        mem::swap(&mut self.mark_marker_ptr, &mut other.mark_marker_ptr);
        mem::swap(&mut self.state_markers, &mut other.state_markers);
        mem::swap(&mut self.undo_state, &mut other.undo_state);
        mem::swap(&mut self.overlays, &mut other.overlays);
        self.swap_slot_with(other, BUFFER_SLOT_MARK_ACTIVE);
        self.swap_slot_with(other, BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS);
        self.swap_slot_with(other, BUFFER_SLOT_BIDI_DISPLAY_REORDERING);
        self.swap_slot_with(other, BUFFER_SLOT_BIDI_PARAGRAPH_DIRECTION);
        self.swap_slot_with(other, BUFFER_SLOT_BIDI_PARAGRAPH_SEPARATE_RE);
        self.swap_slot_with(other, BUFFER_SLOT_BIDI_PARAGRAPH_START_RE);
        self.slots[BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] = Value::NIL;
        other.slots[BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] = Value::NIL;
    }

    fn note_buffer_swap_text_self(&mut self) {
        self.slots[BUFFER_SLOT_POINT_BEFORE_SCROLL.index()] = Value::NIL;
        self.text.record_char_modification(2);
        self.increment_overlay_modified_tick();
        self.increment_overlay_modified_tick();
    }
}

impl Buffer {
    // -- Construction --------------------------------------------------------

    /// Create a new, empty buffer.
    ///
    /// `saved_point_before_command` is the editor's ONE saved-point cell (GNU's
    /// `point_before_last_command_or_undo` pair, src/keyboard.c:232-233).  It
    /// is a required argument rather than something a buffer mints for itself
    /// precisely because a private cell is the bug this models away: a buffer
    /// with its own saved point can never observe that a command in another
    /// buffer superseded it.  Production callers pass
    /// `BufferManager::saved_point_before_command`.
    pub(crate) fn new(
        id: BufferId,
        name: Value,
        saved_point_before_command: SavedPointBeforeCommand,
    ) -> Self {
        Self::new_with_text_backend_kind(
            id,
            name,
            ImplementedBufferTextBackendKind::GAP_BUFFER,
            saved_point_before_command,
        )
    }

    pub fn try_new_with_text_backend_kind(
        id: BufferId,
        name: Value,
        text_backend_kind: BufferTextBackendKind,
        saved_point_before_command: SavedPointBeforeCommand,
    ) -> Result<Self, BufferTextBackendKind> {
        let implemented_kind = text_backend_kind.implemented().ok_or(text_backend_kind)?;
        Ok(Self::new_with_text_backend_kind(
            id,
            name,
            implemented_kind,
            saved_point_before_command,
        ))
    }

    /// Create a buffer that belongs to no editor, minting a saved-point cell
    /// for it alone.
    ///
    /// This exists for STANDALONE buffers: layout, rendering and bridge tests
    /// in other crates that need a `Buffer` without a `BufferManager` around
    /// it.  It is named for that and kept separate from [`Buffer::new`] on
    /// purpose, because a buffer holding a private cell is exactly the defect
    /// ledger 122 removed -- the saved point-before-command is editor-global
    /// (GNU's `point_before_last_command_or_undo`, src/keyboard.c:232-233), and
    /// a buffer with its own cell can never observe that a command in another
    /// buffer superseded it.  A buffer an editor owns must come from
    /// `BufferManager`, which hands out clones of the one cell.
    pub fn new_standalone(id: BufferId, name: Value) -> Self {
        Self::new(id, name, SavedPointBeforeCommand::new_editor_global())
    }

    /// [`Buffer::new_standalone`] with an explicit text backend.
    pub fn try_new_standalone_with_text_backend_kind(
        id: BufferId,
        name: Value,
        text_backend_kind: BufferTextBackendKind,
    ) -> Result<Self, BufferTextBackendKind> {
        Self::try_new_with_text_backend_kind(
            id,
            name,
            text_backend_kind,
            SavedPointBeforeCommand::new_editor_global(),
        )
    }

    pub(crate) fn new_with_text_backend_kind(
        id: BufferId,
        name: Value,
        text_backend_kind: ImplementedBufferTextBackendKind,
        saved_point_before_command: SavedPointBeforeCommand,
    ) -> Self {
        assert!(name.is_string(), "buffer name must be a Lisp string");
        Self {
            line_number_anchor: std::cell::Cell::new((0, 0)),
            id,
            name,
            last_name: Value::NIL,
            base_buffer: None,
            text: BufferText::new_with_backend_kind(text_backend_kind),
            point: TextPositionAnchor::new(CharPos0::ZERO, EmacsBytePos::ZERO),
            mark_marker_id: None,
            mark_marker_ptr: std::ptr::null_mut(),
            accessible_start: TextPositionAnchor::new(CharPos0::ZERO, EmacsBytePos::ZERO),
            accessible_end: TextPositionAnchor::new(CharPos0::ZERO, EmacsBytePos::ZERO),
            autosave_modified_tick: 1,
            last_window_start: LispCharPos1::ONE,
            last_selected_window: None,
            inhibit_buffer_hooks: false,
            state_markers: None,
            local_var_alist: LocalVariableBindings::from_alist(Value::NIL),
            keymap: crate::emacs_core::value::Value::NIL,
            // GNU `reset_buffer`: `b->modtime = make_timespec (0,
            // UNKNOWN_MODTIME_NSECS); b->modtime_size = -1;`
            // (`src/buffer.c:1092-1093`).
            modtime: VisitedFileModtimeSlot::default(),
            modtime_size: None,
            slots: {
                // Phase 10C: seed every slot from BUFFER_SLOT_INFO.
                // Mirrors GNU's `reset_buffer` (`buffer.c:1188`)
                // copying `buffer_defaults` into a fresh buffer.
                let mut s = [crate::emacs_core::value::Value::NIL; BUFFER_SLOT_COUNT];
                for info in BUFFER_SLOT_INFO {
                    s[info.offset.index()] = info.default.to_value();
                }
                s
            },
            // Phase 10D: every fresh buffer starts with no conditional
            // local-flag bits set. Reads of conditional slots fall
            // through to `Context::buffer_defaults` until a write or
            // `make-local-variable` flips the bit.
            local_flags: 0,
            overlays: OverlayList::new(),
            overlay_modified_tick: 1,
            undo_state: SharedUndoState::new(),
            saved_point_before_command,
        }
    }

    pub(crate) fn from_dump_parts(parts: BufferDumpParts) -> Self {
        assert!(parts.name.is_string(), "buffer name must be a Lisp string");
        Self {
            line_number_anchor: std::cell::Cell::new((0, 0)),
            id: parts.id,
            name: parts.name,
            last_name: parts.last_name,
            base_buffer: parts.base_buffer,
            text: parts.text,
            point: parts.point,
            mark_marker_id: parts.mark_marker_id,
            mark_marker_ptr: parts.mark_marker_ptr,
            accessible_start: parts.accessible_start,
            accessible_end: parts.accessible_end,
            autosave_modified_tick: parts.autosave_modified_tick,
            last_window_start: parts.last_window_start,
            last_selected_window: parts.last_selected_window,
            inhibit_buffer_hooks: parts.inhibit_buffer_hooks,
            state_markers: parts.state_markers,
            local_var_alist: LocalVariableBindings::from_alist(parts.local_var_alist),
            keymap: parts.keymap,
            modtime: VisitedFileModtimeSlot::new(parts.modtime),
            modtime_size: parts.modtime_size,
            slots: parts.slots,
            local_flags: parts.local_flags,
            overlays: parts.overlays,
            overlay_modified_tick: parts.overlay_modified_tick,
            undo_state: parts.undo_state,
            saved_point_before_command: parts.saved_point_before_command,
        }
    }

    pub fn name_value(&self) -> Value {
        self.name
    }

    pub fn id(&self) -> BufferId {
        self.id
    }

    pub fn local_var_alist_value(&self) -> Value {
        self.local_var_alist.as_lisp_alist()
    }

    /// Store the binding list returned by `Obarray::set_internal_localized`.
    ///
    /// The [`SetInternalAlist`] witness is the point: only that function can
    /// mint one, and it only ever rewrites a binding cdr in place or prepends
    /// a new head. A caller that unlinks interior entries has no way to reach
    /// this method and must use [`LocalVariableBindings::retain_bindings`],
    /// which cannot leave the derived index stale.
    pub(crate) fn replace_local_var_alist(&mut self, alist: SetInternalAlist) {
        self.local_var_alist.replace_alist(alist.into_value());
    }

    pub fn slot_values_snapshot(&self) -> [Value; BUFFER_SLOT_COUNT] {
        self.slots
    }

    pub fn overlays(&self) -> &OverlayList {
        &self.overlays
    }

    pub fn overlays_mut(&mut self) -> &mut OverlayList {
        &mut self.overlays
    }

    pub fn last_name_value(&self) -> Value {
        self.last_name
    }

    pub fn name_runtime_string_owned(&self) -> String {
        // Buffer names are decoded text (ASCII/Unicode); to_utf8_lossy is exact
        // for those and only differs for pathological raw-eight-bit names.
        self.name
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .expect("buffer name must be a Lisp string")
    }

    pub fn has_name(&self, name: &str) -> bool {
        self.name_runtime_string_owned() == name
    }

    pub fn name_starts_with_space(&self) -> bool {
        self.name_runtime_string_owned().starts_with(' ')
    }

    pub fn set_name_value(&mut self, name: Value) {
        assert!(name.is_string(), "buffer name must be a Lisp string");
        self.name = name;
    }

    /// Mirror the final object-state mutations in GNU `Fkill_buffer`:
    /// reset local state first, then move `name` to `last_name` and leave
    /// the buffer object addressable but not live.
    pub fn mark_killed_after_local_reset(&mut self) {
        let old_name = self.name;
        debug_assert!(old_name.is_string());
        self.last_name = old_name;
        self.name = Value::NIL;
        self.local_var_alist.replace_alist(Value::NIL);
        self.local_flags = 0;
        self.keymap = Value::NIL;
        self.overlays.delete_all_overlays();
        self.text = BufferText::new();
        self.mark_marker_id = None;
        self.mark_marker_ptr = std::ptr::null_mut();
        self.state_markers = None;
        self.undo_state = SharedUndoState::new();
        self.narrow_to_emacs_byte_range(EmacsByteRange::EMPTY);
        self.goto_emacs_byte_pos(EmacsBytePos::new(0));
    }

    pub fn set_last_name_value(&mut self, name: Value) {
        debug_assert!(name.is_nil() || name.is_string());
        self.last_name = name;
    }

    pub fn set_name_runtime_string(&mut self, name: impl Into<String>) {
        self.name = Value::string(name.into());
    }

    // -- Phase 10D: per-slot local-flag bitmap accessors. Conditional
    // -- BUFFER_OBJFWD slots (those with `local_flags_idx >= 0`) only
    // -- hold buffer-local values when their bit is set in
    // -- [`Self::local_flags`]. Always-local slots ignore the bitmap.
    // -- Mirrors GNU's `PER_BUFFER_VALUE_P` / `SET_PER_BUFFER_VALUE_P`
    // -- (`buffer.h:1640-1645`).

    /// Test whether the conditional slot at `offset` has a per-buffer
    /// local value installed in this buffer. Mirrors GNU
    /// `PER_BUFFER_VALUE_P` (`buffer.h:1640`).
    #[inline]
    pub fn slot_local_flag(&self, slot: BufferSlot) -> bool {
        (self.local_flags >> (slot.index() as u32)) & 1 != 0
    }

    /// Set or clear the conditional-local flag for the slot at
    /// `offset`. Mirrors GNU `SET_PER_BUFFER_VALUE_P` (`buffer.h:1645`).
    #[inline]
    pub fn set_slot_local_flag(&mut self, slot: BufferSlot, on: bool) {
        let bit = 1u64 << (slot.index() as u32);
        if on {
            self.local_flags |= bit;
        } else {
            self.local_flags &= !bit;
        }
    }

    // -- Slot accessors for the four hardcoded fields targeted by
    // -- Phase 8b of the symbol-redirect refactor. `file_name` now
    // -- lives in [`Self::slots`] at [`BUFFER_SLOT_FILE_NAME`],
    // -- mirroring GNU's `BVAR(buffer, filename)`. The other three
    // -- (`auto_save_file_name` / `read_only` / `multibyte`) still
    // -- have struct fields during the staggered migration.

    /// Read `buffer-file-name` as the underlying Lisp value, mirroring GNU
    /// `BVAR(buf, filename)` (`buffer.h:319`).
    pub fn file_name_value(&self) -> Value {
        self.slots[BUFFER_SLOT_FILE_NAME.index()]
    }

    /// Clone `buffer-file-name` as an owned runtime string.
    /// This is a boundary helper for filesystem-facing code.
    pub fn file_name_runtime_string_owned(&self) -> Option<String> {
        self.file_name_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    }

    /// Borrow `buffer-file-name`'s payload, tied to the BUFFER's borrow.
    ///
    /// DIVERGENCES.md 163: returning `&'static` here laundered a `&self`
    /// borrow of a buffer — itself obtained from `&ctx.buffers` — into a
    /// reference with no owner, so a caller could keep reading the name after
    /// the slot had been overwritten and the old string collected. Eliding to
    /// `&self` makes that a borrow error and costs nothing: every caller
    /// either clones immediately or uses the name inside the same `&`-borrow.
    pub fn file_name_lisp_string(&self) -> Option<&crate::heap_types::LispString> {
        self.slots[BUFFER_SLOT_FILE_NAME.index()].as_lisp_string()
    }

    /// Write `buffer-file-name`. Mirrors GNU `bset_filename`
    /// (`buffer.c`). The slot stores either a Lisp string or `nil`.
    pub fn set_file_name_value(&mut self, v: Value) {
        assert!(
            v.is_nil() || v.is_string(),
            "buffer-file-name must be nil or a Lisp string"
        );
        self.slots[BUFFER_SLOT_FILE_NAME.index()] = v;
    }

    /// Read `buffer-auto-save-file-name` as the underlying Lisp value,
    /// mirroring GNU `BVAR(buf, auto_save_file_name)` (`buffer.h:323`).
    pub fn auto_save_file_name_value(&self) -> Value {
        self.slots[BUFFER_SLOT_AUTO_SAVE_FILE_NAME.index()]
    }

    /// Clone `buffer-auto-save-file-name` as an owned runtime string.
    /// This is a boundary helper for filesystem-facing code.
    pub fn auto_save_file_name_runtime_string_owned(&self) -> Option<String> {
        self.auto_save_file_name_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    }

    /// Tied to the buffer's borrow, not `'static` — see
    /// `file_name_lisp_string` (DIVERGENCES.md 163).
    pub fn auto_save_file_name_lisp_string(&self) -> Option<&crate::heap_types::LispString> {
        self.slots[BUFFER_SLOT_AUTO_SAVE_FILE_NAME.index()].as_lisp_string()
    }

    /// Write `buffer-auto-save-file-name`. Mirrors GNU
    /// `bset_auto_save_file_name`. The slot stores either a Lisp string or
    /// `nil`.
    pub fn set_auto_save_file_name_value(&mut self, v: Value) {
        assert!(
            v.is_nil() || v.is_string(),
            "buffer-auto-save-file-name must be nil or a Lisp string"
        );
        self.slots[BUFFER_SLOT_AUTO_SAVE_FILE_NAME.index()] = v;
    }

    /// Read `buffer-read-only`, mirroring GNU
    /// `BVAR(buf, read_only)`. A non-nil slot maps to `true`.
    pub fn get_read_only(&self) -> bool {
        self.slots[BUFFER_SLOT_READ_ONLY.index()].is_truthy()
    }

    /// Write `buffer-read-only`. `true` stores `Value::T`, `false`
    /// stores `Value::NIL`.
    pub fn set_read_only_value(&mut self, v: bool) {
        self.slots[BUFFER_SLOT_READ_ONLY.index()] = if v { Value::T } else { Value::NIL };
    }

    /// Read `enable-multibyte-characters`, mirroring GNU
    /// `BVAR(buf, enable_multibyte_characters)`. A non-nil slot
    /// maps to `true`.
    pub fn get_multibyte(&self) -> bool {
        self.slots[BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS.index()].is_truthy()
    }

    /// Write `enable-multibyte-characters`. `true` stores
    /// `Value::T`, `false` stores `Value::NIL`.
    pub fn set_multibyte_value(&mut self, v: bool) {
        self.text.set_multibyte(v);
        self.slots[BUFFER_SLOT_ENABLE_MULTIBYTE_CHARACTERS.index()] =
            if v { Value::T } else { Value::NIL };
    }

    // -- Point queries -------------------------------------------------------

    /// Current point as an Emacs byte position.
    pub fn point_emacs_byte_pos(&self) -> EmacsBytePos {
        self.point.emacs_byte_pos()
    }

    /// Current point as a paired character/byte anchor.
    pub fn point_anchor(&self) -> TextPositionAnchor {
        self.point
    }

    /// Beginning of the accessible portion as a paired character/byte anchor.
    pub fn point_min_anchor(&self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.point_min_char_pos(), self.point_min_emacs_byte_pos())
    }

    /// End of the accessible portion as a paired character/byte anchor.
    pub fn point_max_anchor(&self) -> TextPositionAnchor {
        TextPositionAnchor::new(self.point_max_char_pos(), self.point_max_emacs_byte_pos())
    }

    /// Restore point from a paired character/byte anchor.
    ///
    /// This is the Rust-side equivalent of GNU's `SET_PT_BOTH`/
    /// `TEMP_SET_PT_BOTH`: callers that already preserved both coordinates
    /// can restore them atomically.  The active text backend still recomputes
    /// the authoritative character coordinate from the restored byte
    /// coordinate, and debug builds verify that the saved pair was coherent.
    pub fn set_point_anchor(&mut self, point: TextPositionAnchor) {
        let requested_byte = point.emacs_byte_pos();
        let restored = self.accessible_anchor_for_emacs_byte_pos(requested_byte);
        self.set_point_anchor_unchecked(restored);
        if restored.emacs_byte_pos() == requested_byte {
            debug_assert_eq!(
                restored.char_pos(),
                point.char_pos().min(self.total_char_end_pos())
            );
        }
    }

    /// Current point converted to a character position.
    pub fn point_char_pos(&self) -> CharPos0 {
        self.point.char_pos()
    }

    /// Current point as a 1-based Lisp character position.
    pub fn point_lisp_char_pos(&self) -> LispCharPos1 {
        self.point_char_pos().to_lisp()
    }

    /// Beginning of the accessible portion (Emacs byte position).
    pub fn point_min_emacs_byte_pos(&self) -> EmacsBytePos {
        self.accessible_start.emacs_byte_pos()
    }

    /// Beginning of the accessible portion (character position).
    pub fn point_min_char_pos(&self) -> CharPos0 {
        self.accessible_start.char_pos()
    }

    /// Beginning of the accessible portion as a 1-based Lisp character position.
    pub fn point_min_lisp_char_pos(&self) -> LispCharPos1 {
        self.point_min_char_pos().to_lisp()
    }

    /// End of the accessible portion (Emacs byte position).
    pub fn point_max_emacs_byte_pos(&self) -> EmacsBytePos {
        self.accessible_end.emacs_byte_pos()
    }

    /// End of the accessible portion (character position).
    pub fn point_max_char_pos(&self) -> CharPos0 {
        self.accessible_end.char_pos()
    }

    /// End of the accessible portion as a 1-based Lisp character position.
    pub fn point_max_lisp_char_pos(&self) -> LispCharPos1 {
        self.point_max_char_pos().to_lisp()
    }

    /// Total number of characters in the buffer text.
    pub fn total_char_len(&self) -> CharLen {
        self.text.char_count()
    }

    /// Exclusive full-buffer end position in internal character coordinates.
    pub fn total_char_end_pos(&self) -> CharPos0 {
        CharPos0::ZERO.add_len(self.total_char_len())
    }

    /// GNU `Z` as a 1-based Lisp character position.
    pub fn z_lisp_char_pos(&self) -> LispCharPos1 {
        self.total_char_end_pos().to_lisp()
    }

    /// Full buffer bounds in Lisp character positions, ignoring narrowing.
    pub fn full_lisp_char_region(&self) -> FullBufferLispCharRange {
        FullBufferLispCharRange::new(self.z_lisp_char_pos())
    }

    /// Total number of Emacs bytes in the buffer text.
    pub fn total_emacs_byte_len(&self) -> EmacsByteLen {
        self.text.emacs_byte_len()
    }

    /// Exclusive full-buffer end position in Emacs byte coordinates.
    pub fn total_emacs_byte_end_pos(&self) -> EmacsBytePos {
        EmacsBytePos::ZERO.add_len(self.total_emacs_byte_len())
    }

    pub fn is_text_empty(&self) -> bool {
        self.text.is_empty()
    }

    pub fn full_text_string(&self) -> String {
        self.text.full_text_string()
    }

    pub fn text_snapshot(&self) -> BufferTextSnapshot {
        BufferTextSnapshot {
            text: self.text.clone(),
        }
    }

    pub fn text_backend_kind(&self) -> BufferTextBackendKind {
        self.text.backend_kind()
    }

    #[cfg(test)]
    pub(crate) fn shares_text_storage_with(&self, other: &Self) -> bool {
        self.text.shares_storage_with(&other.text)
    }

    #[cfg(test)]
    pub(crate) fn replace_text_snapshot_for_test(
        &mut self,
        snapshot: BufferTextBytesSnapshot,
        kind: ImplementedBufferTextBackendKind,
    ) {
        self.text = BufferText::from_snapshot_with_backend_kind(snapshot, kind);
    }

    pub(crate) fn dump_text_backend_kind(&self) -> ImplementedBufferTextBackendKind {
        self.text.implemented_backend_kind()
    }

    pub(crate) fn dump_text_bytes(&self) -> Vec<u8> {
        self.text.dump_text()
    }

    pub(crate) fn convert_text_backend_kind(&mut self, kind: ImplementedBufferTextBackendKind) {
        self.text.convert_backend_kind(kind);
    }

    pub fn gap_position_lisp(&self) -> i64 {
        self.text.gap_position_lisp()
    }

    pub fn gap_size_lisp(&self) -> i64 {
        self.text.gap_size_lisp()
    }

    /// Full buffer range in Emacs bytes, ignoring narrowing.
    pub fn full_emacs_byte_range(&self) -> EmacsByteRange {
        EmacsByteRange::from_start_len(EmacsBytePos::ZERO, self.total_emacs_byte_len())
    }

    /// Accessible buffer range in Emacs bytes, respecting narrowing.
    pub fn accessible_emacs_byte_range(&self) -> EmacsByteRange {
        EmacsByteRange::new(
            self.point_min_emacs_byte_pos(),
            self.point_max_emacs_byte_pos(),
        )
    }

    /// Accessible buffer bounds in Emacs bytes, preserving the narrowing
    /// meaning in the type for motion/search code.
    pub fn accessible_emacs_byte_region(&self) -> AccessibleEmacsByteRange {
        AccessibleEmacsByteRange::new(self.accessible_emacs_byte_range())
    }

    /// Accessible buffer bounds in internal character positions.
    pub fn accessible_char_region(&self) -> AccessibleCharRange {
        AccessibleCharRange::new(CharRange::new(
            self.point_min_char_pos(),
            self.point_max_char_pos(),
        ))
    }

    /// Character length of the accessible portion, respecting narrowing.
    pub fn accessible_char_len(&self) -> CharLen {
        self.point_max_char_pos()
            .saturating_offset_from(self.point_min_char_pos())
    }

    pub fn is_narrowed(&self) -> bool {
        let accessible = self.accessible_emacs_byte_region();
        accessible.start() > EmacsBytePos::ZERO
            || accessible.end() < self.total_emacs_byte_end_pos()
    }

    /// Convert a 0-based character position to an Emacs byte position,
    /// clamping to the buffer text length.
    pub fn char_pos_to_emacs_byte_pos_clamped(&self, char_pos: CharPos0) -> EmacsBytePos {
        let char_pos = char_pos.min(self.total_char_end_pos());
        // Point is a maintained (char, byte) pair, and scans resume AT
        // point constantly (parse-partial-sexp moves point to its stop and
        // the next call starts there) — the text layer would otherwise walk
        // from its nearest stride anchor for the same answer.
        if char_pos == self.point.char_pos() {
            return self.point.emacs_byte_pos();
        }
        self.text.char_pos_to_emacs_byte_pos(char_pos)
    }

    /// Convert an Emacs byte position to a 0-based character position,
    /// clamping to the buffer text length.
    pub fn emacs_byte_pos_to_char_pos_clamped(&self, byte_pos: EmacsBytePos) -> CharPos0 {
        let byte_pos = byte_pos.min(self.total_emacs_byte_end_pos());
        if byte_pos == self.point.emacs_byte_pos() {
            return self.point.char_pos();
        }
        self.text.emacs_byte_pos_to_char_pos(byte_pos)
    }

    pub fn emacs_byte_pos_to_lisp_char_pos(&self, byte_pos: EmacsBytePos) -> LispCharPos1 {
        self.emacs_byte_pos_to_char_pos_clamped(byte_pos).to_lisp()
    }

    /// Convert a 1-based Lisp character position to a byte position, clamping
    /// to the full buffer.
    pub fn lisp_pos_to_emacs_byte_pos(&self, lisp_pos: LispCharPos1) -> EmacsBytePos {
        let char_pos = lisp_pos.to_char_pos();
        self.char_pos_to_emacs_byte_pos_clamped(char_pos)
    }

    /// Convert a 1-based Lisp character position to a byte position, clamping
    /// to the accessible region.
    pub fn lisp_pos_to_accessible_emacs_byte_pos(&self, lisp_pos: LispCharPos1) -> EmacsBytePos {
        let char_pos = lisp_pos.to_char_pos();
        let clamped_char = char_pos.clamp(self.point_min_char_pos(), self.point_max_char_pos());
        self.text.char_pos_to_emacs_byte_pos(clamped_char)
    }

    /// Convert a 1-based Lisp character position to a byte position, clamping
    /// to the *full* buffer range (ignoring narrowing).
    ///
    /// GNU Emacs: `set-marker` clamps to the full buffer, not the narrowed
    /// region, so markers can be placed outside the accessible range.
    pub fn lisp_pos_to_full_buffer_emacs_byte_pos(&self, lisp_pos: LispCharPos1) -> EmacsBytePos {
        let char_pos = lisp_pos.to_char_pos();
        self.text
            .char_pos_to_emacs_byte_pos(char_pos.min(self.total_char_end_pos()))
    }

    // -- Point movement ------------------------------------------------------

    /// Set point in Emacs bytes, clamping to the accessible region `[begv, zv]`.
    pub fn goto_emacs_byte_pos(&mut self, pos: EmacsBytePos) {
        self.set_point_anchor_unchecked(self.accessible_anchor_for_emacs_byte_pos(pos));
    }

    // -- Undo helpers --------------------------------------------------------

    /// Get the current `buffer-undo-list` value from buffer-local properties.
    pub fn get_undo_list(&self) -> Value {
        self.undo_state.list()
    }

    /// Store the `buffer-undo-list` value into the shared undo
    /// state. The SharedUndoState is the single source of truth —
    /// reads of `buffer-undo-list` route through
    /// [`Self::get_undo_list`] regardless of which Buffer in an
    /// indirect-buffer chain is queried.
    pub fn set_undo_list(&mut self, value: Value) {
        self.undo_state.set_list(value);
    }

    // -- Text queries --------------------------------------------------------

    fn clamped_emacs_byte_pos(&self, pos: EmacsBytePos) -> EmacsBytePos {
        pos.min(self.total_emacs_byte_end_pos())
    }

    fn clamped_emacs_byte_range(&self, range: EmacsByteRange) -> EmacsByteRange {
        let total = self.total_emacs_byte_end_pos();
        let start = range.start().min(total);
        let end = range.end().max(start).min(total);
        EmacsByteRange::new(start, end)
    }

    pub fn copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.text
            .copy_emacs_byte_range_to(self.clamped_emacs_byte_range(range), out);
    }

    pub fn emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.text
            .emacs_byte_at_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn char_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.text
            .char_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    /// Emacs character *code* (u32, including codes outside the Unicode range
    /// for raw bytes) at a byte position.  Unlike `char_at_emacs_byte_pos` this
    /// does not lose raw-byte chars to Rust's `char` range.
    pub fn char_code_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        self.text
            .char_code_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    /// See `BufferText::contiguous_window_at`: the gap half (or storage
    /// chunk) containing logical byte `pos`, for borrow-free sequential
    /// scans. Valid only until the next text mutation.
    pub fn contiguous_window_at(&self, pos: usize) -> Option<(usize, *const u8, usize)> {
        self.text.contiguous_window_at(pos)
    }

    /// The interval plist covering char `pos`, or `None` when no interval
    /// covers it. See [`BufferText::interval_plist_at_char_pos`].
    pub fn interval_plist_at_char_pos(&self, pos: CharPos0) -> Option<Value> {
        self.text.interval_plist_at_char_pos(pos)
    }

    /// The interval plist covering an Emacs byte position, or `None` when no
    /// interval covers it. See [`BufferText::interval_plist_at_emacs_byte_pos`].
    pub fn interval_plist_at_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<Value> {
        self.text
            .interval_plist_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn text_props_get_property_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<Value> {
        self.text
            .text_props_get_property_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos), name)
    }

    /// Char-addressed property read: callers that already hold a char
    /// position must use this instead of converting to bytes just for the
    /// callee to convert straight back (a measured full round trip per
    /// property lookup on the font-lock path).
    pub fn text_props_get_property_at_char_pos(&self, pos: CharPos0, name: Value) -> Option<Value> {
        self.text
            .text_props_get_property_at_char_pos(pos.min(self.total_char_end_pos()), name)
    }

    /// Conservative whole-buffer presence of text property `name`.
    pub fn text_props_property_name_presence(
        &self,
        name: Value,
    ) -> super::text_props::PropertyNamePresence {
        self.text.text_props_property_name_presence(name)
    }

    /// Property `name` at char `pos` plus the `[start, end)` char run over
    /// which it is constant.  See
    /// [`BufferText::get_property_run_at_char_pos`].
    pub fn get_property_run_at_char_pos(
        &self,
        pos: CharPos0,
        name: Value,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        self.text.get_property_run_at_char_pos(pos, name)
    }

    /// The interval plist covering char `pos` plus its `[start, end)` char run.
    /// See [`BufferText::interval_plist_run_at_char_pos`].
    pub fn interval_plist_run_at_char_pos(
        &self,
        pos: CharPos0,
    ) -> (Option<Value>, CharPos0, CharPos0) {
        self.text.interval_plist_run_at_char_pos(pos)
    }

    /// Next char position in `(pos, cap)` where any of `keys` changes value.
    /// See [`text_props::TextPropertyTable::next_watched_property_change`].
    pub fn next_watched_property_change_at_char_pos(
        &self,
        pos: CharPos0,
        cap: CharPos0,
        keys: &[Value],
    ) -> CharPos0 {
        self.text
            .next_watched_property_change_at_char_pos(pos, cap, keys)
    }

    /// See [`text_props::TextPropertyTable::syntax_prop_free_run_end`].
    pub fn syntax_prop_free_run_end_at_char_pos(&self, pos: CharPos0, cap: CharPos0) -> CharPos0 {
        self.text.syntax_prop_free_run_end_at_char_pos(pos, cap)
    }

    /// See [`crate::buffer::buffer_text::BufferText::syntax_byte_run_memo_lookup`].
    pub fn syntax_byte_run_memo_lookup(
        &self,
        byte_pos: EmacsBytePos,
    ) -> Option<(u64, u64, Option<Value>)> {
        self.text.syntax_byte_run_memo_lookup(byte_pos)
    }

    /// See [`crate::buffer::buffer_text::BufferText::syntax_byte_run_memo_store`].
    pub fn syntax_byte_run_memo_store(&self, start: u64, end: u64, value: Option<Value>) {
        self.text.syntax_byte_run_memo_store(start, end, value);
    }

    /// See [`crate::buffer::buffer_text::BufferText::syntax_char_run_memo_lookup`].
    pub fn syntax_char_run_memo_lookup(&self, pos: usize) -> Option<(u64, u64, Option<Value>)> {
        self.text.syntax_char_run_memo_lookup(pos)
    }

    /// See [`crate::buffer::buffer_text::BufferText::syntax_char_run_memo_store`].
    pub fn syntax_char_run_memo_store(&self, start: u64, end: u64, value: Option<Value>) {
        self.text.syntax_char_run_memo_store(start, end, value);
    }

    /// Whether any property in `keys` is non-nil inside the bounded character
    /// `range`.  See
    /// [`TextPropertyTable::has_any_non_nil_property_in_char_range`].
    pub fn has_any_non_nil_property_in_char_range(&self, range: CharRange, keys: &[Value]) -> bool {
        self.text
            .has_any_non_nil_property_in_char_range(range, keys)
    }

    pub fn text_props_get_properties_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> HashMap<Value, Value> {
        self.text
            .text_props_get_properties_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn text_props_get_properties_ordered_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Vec<(Value, Value)> {
        self.text
            .text_props_get_properties_ordered_at_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn text_props_get_properties_plist_value_at_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Value {
        self.text
            .text_props_get_properties_plist_value_at_emacs_byte_pos(
                self.clamped_emacs_byte_pos(pos),
            )
    }

    pub fn text_props_put_property_in_emacs_byte_range(
        &mut self,
        range: EmacsByteRange,
        name: Value,
        value: Value,
    ) -> bool {
        self.text.text_props_put_property_in_emacs_byte_range(
            self.clamped_emacs_byte_range(range),
            name,
            value,
        )
    }

    pub fn text_props_remove_properties_in_emacs_byte_range(
        &mut self,
        range: EmacsByteRange,
        names: &[Value],
    ) -> bool {
        self.text.text_props_remove_properties_in_emacs_byte_range(
            self.clamped_emacs_byte_range(range),
            names,
        )
    }

    pub fn text_props_remove_property_in_emacs_byte_range(
        &mut self,
        range: EmacsByteRange,
        name: Value,
    ) -> bool {
        self.text.text_props_remove_property_in_emacs_byte_range(
            self.clamped_emacs_byte_range(range),
            name,
        )
    }

    pub fn text_props_remove_all_in_emacs_byte_range(&mut self, range: EmacsByteRange) {
        self.text
            .text_props_remove_all_in_emacs_byte_range(self.clamped_emacs_byte_range(range));
    }

    pub fn text_props_next_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_change_after_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn text_props_next_single_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_single_change_after_emacs_byte_pos(
                self.clamped_emacs_byte_pos(pos),
                name,
            )
    }

    /// Display-engine bounded variant; see
    /// [`BufferText::text_props_next_single_change_after_emacs_byte_pos_bounded`].
    pub fn text_props_next_single_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_single_change_after_emacs_byte_pos_bounded(
                self.clamped_emacs_byte_pos(pos),
                name,
                self.clamped_emacs_byte_pos(limit),
            )
    }

    pub fn text_props_previous_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_previous_change_before_emacs_byte_pos(self.clamped_emacs_byte_pos(pos))
    }

    pub fn text_props_previous_single_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_previous_single_change_before_emacs_byte_pos(
                self.clamped_emacs_byte_pos(pos),
                name,
            )
    }

    /// See `TextPropertyTable::for_each_interval_from_char_pos`.
    pub fn text_props_for_each_interval_from_char_pos<F>(&self, pos: CharPos0, f: F)
    where
        F: FnMut(CharPos0, CharPos0, Value) -> bool,
    {
        self.text
            .text_props_for_each_interval_from_char_pos(pos.min(self.total_char_end_pos()), f)
    }

    pub fn text_props_next_interval_boundary_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_next_interval_boundary_after_emacs_byte_pos(
                self.clamped_emacs_byte_pos(pos),
            )
    }

    pub fn text_props_previous_interval_boundary_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text
            .text_props_previous_interval_boundary_before_emacs_byte_pos(
                self.clamped_emacs_byte_pos(pos),
            )
    }

    pub fn text_props_is_empty(&self) -> bool {
        self.text.text_props_is_empty()
    }

    pub fn text_props_snapshot(&self) -> TextPropertyTable {
        self.text.text_props_snapshot()
    }

    pub(crate) fn text_props_object_interval_runs(&self) -> Vec<ObjectIntervalRun> {
        self.text
            .text_props_object_interval_runs(self.total_char_len())
    }

    #[cfg(test)]
    pub(crate) fn text_props_intervals_snapshot_for_test(
        &self,
    ) -> Vec<super::text_props::PropertyInterval> {
        self.text.text_props_intervals_snapshot()
    }

    #[cfg(test)]
    pub(crate) fn replace_text_props_for_test(&mut self, props: TextPropertyTable) {
        self.text.text_props_replace(props);
    }

    pub fn text_props_slice_emacs_byte_range(&self, range: EmacsByteRange) -> TextPropertyTable {
        let clamped = self.clamped_emacs_byte_range(range);
        self.text.text_props_slice_emacs_byte_range(clamped)
    }

    pub fn replace_lisp_string_with_text_props(
        &mut self,
        text: &crate::heap_types::LispString,
        text_props: TextPropertyTable,
    ) {
        self.text.replace_lisp_string(text, text_props);
    }

    pub fn remap_text_marker_anchors<F>(&mut self, f: F)
    where
        F: FnMut(TextPositionAnchor) -> TextPositionAnchor,
    {
        self.text.remap_marker_anchors(f);
    }

    pub(crate) fn walk_marker_data_for_dump<F: FnMut(&crate::heap_types::LispMarker)>(&self, f: F) {
        self.text.chain_walk_data(f);
    }

    pub fn emacs_byte_range_contains_char_code(&self, range: EmacsByteRange, code: u32) -> bool {
        self.text
            .emacs_byte_range_contains_char_code(self.clamped_emacs_byte_range(range), code)
    }

    pub fn text_props_range_has_any_property_named_in_emacs_byte_range(
        &self,
        range: EmacsByteRange,
        names: &[Value],
    ) -> bool {
        self.text
            .text_props_range_has_any_property_named_in_emacs_byte_range(
                self.clamped_emacs_byte_range(range),
                names,
            )
    }

    pub fn text_props_range_has_all_properties_in_emacs_byte_range(
        &self,
        range: EmacsByteRange,
        properties: &[(Value, Value)],
    ) -> bool {
        self.text
            .text_props_range_has_all_properties_in_emacs_byte_range(
                self.clamped_emacs_byte_range(range),
                properties,
            )
    }

    pub fn text_props_range_has_any_interval_in_emacs_byte_range(
        &self,
        range: EmacsByteRange,
    ) -> bool {
        self.text
            .text_props_range_has_any_interval_in_emacs_byte_range(
                self.clamped_emacs_byte_range(range),
            )
    }

    pub(crate) fn text_props_try_for_each_interval_in_emacs_byte_range<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(CharRange, &[(Value, Value)]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.text
            .text_props_try_for_each_interval_in_emacs_byte_range(
                self.clamped_emacs_byte_range(range),
                f,
            )
    }

    /// Plist-Value twin: no per-interval pair-slice materialization.
    pub(crate) fn text_props_try_for_each_interval_plist_in_emacs_byte_range<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(CharRange, Value) -> Result<(), E>,
    ) -> Result<(), E> {
        self.text
            .text_props_try_for_each_interval_plist_in_emacs_byte_range(
                self.clamped_emacs_byte_range(range),
                f,
            )
    }

    pub fn try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.text
            .for_each_emacs_byte_range_chunk(self.clamped_emacs_byte_range(range), f)
    }

    pub(crate) fn has_contiguous_emacs_byte_range(&self, range: EmacsByteRange) -> bool {
        self.text
            .has_contiguous_emacs_byte_range(self.clamped_emacs_byte_range(range))
    }

    /// See [`BufferText::try_make_emacs_byte_range_contiguous`].
    pub(crate) fn try_make_emacs_byte_range_contiguous(&self, range: EmacsByteRange) -> bool {
        self.text
            .try_make_emacs_byte_range_contiguous(self.clamped_emacs_byte_range(range))
    }

    pub(crate) fn with_contiguous_emacs_byte_range<R>(
        &self,
        range: EmacsByteRange,
        f: impl FnOnce(&[u8]) -> R,
    ) -> Option<R> {
        self.text
            .with_contiguous_emacs_byte_range(self.clamped_emacs_byte_range(range), f)
    }

    /// First `\n` in logical emacs-byte range `[from, limit)`, scanned in
    /// place.  See [`BufferText::next_newline_emacs_byte`].
    pub(crate) fn next_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text.next_newline_emacs_byte(from, limit)
    }

    /// Last `\n` in logical emacs-byte range `[floor, from)`, scanned in
    /// place.  See [`BufferText::prev_newline_emacs_byte`].
    pub(crate) fn prev_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        floor: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text.prev_newline_emacs_byte(from, floor)
    }

    /// Number of `\n` in logical emacs-byte range `[from, limit)`.
    /// See [`BufferText::count_newlines_emacs_byte`].
    pub(crate) fn count_newlines_emacs_byte(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
    ) -> usize {
        self.text.count_newlines_emacs_byte(from, limit)
    }

    /// Return a raw Emacs-byte copy of the range `[start, end)`.
    pub fn buffer_substring_bytes_range(&self, range: EmacsByteRange) -> Vec<u8> {
        let mut out = Vec::new();
        self.copy_emacs_byte_range_to(range, &mut out);
        out
    }

    /// Return the range `[start, end)` as a Lisp string preserving the
    /// buffer's multibyte/unibyte semantics.
    pub fn buffer_substring_lisp_string_range(
        &self,
        range: EmacsByteRange,
    ) -> crate::heap_types::LispString {
        let mut string = self.buffer_substring_lisp_string_no_properties_range(range);
        let props = self
            .text
            .text_props_slice_emacs_byte_range(self.clamped_emacs_byte_range(range));
        if !props.is_empty() {
            *string.intervals_mut() = props;
        }
        string
    }

    /// Return the range `[start, end)` as a Lisp string without copying the
    /// buffer's text properties, preserving its multibyte/unibyte semantics.
    pub fn buffer_substring_lisp_string_no_properties_range(
        &self,
        range: EmacsByteRange,
    ) -> crate::heap_types::LispString {
        let bytes = self.buffer_substring_bytes_range(range);
        if self.get_multibyte() {
            crate::heap_types::LispString::from_emacs_bytes(bytes)
        } else {
            crate::heap_types::LispString::from_unibyte(bytes)
        }
    }

    /// Return the range `[start, end)` as a Lisp value string.
    pub fn buffer_substring_value_range(&self, range: EmacsByteRange) -> Value {
        Value::heap_string(self.buffer_substring_lisp_string_range(range))
    }

    /// Return a `String` copy of the Emacs-byte range `[start, end)`.
    pub fn buffer_substring_range(&self, range: EmacsByteRange) -> String {
        let bytes = self.buffer_substring_bytes_range(range);
        crate::emacs_core::emacs_char::emacs_bytes_to_lossy_string(&bytes, self.get_multibyte())
    }

    /// Return the entire accessible portion of the buffer as a `String`.
    pub fn buffer_string(&self) -> String {
        self.buffer_substring_range(self.accessible_emacs_byte_range())
    }

    /// Character at Emacs byte position `pos`, or `None` if out of range.
    pub fn char_after_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.char_code_after_emacs_byte_pos(pos)
            .and_then(char::from_u32)
    }

    /// Emacs character code at Emacs byte position `pos`, or `None` if out of range.
    pub fn char_code_after_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        if pos >= self.total_emacs_byte_end_pos() {
            return None;
        }
        self.text.char_code_at_emacs_byte_pos(pos)
    }

    /// Character immediately before Emacs byte position `pos`, or `None`.
    pub fn char_before_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<char> {
        self.char_code_before_emacs_byte_pos(pos)
            .and_then(char::from_u32)
    }

    /// Emacs character code immediately before Emacs byte position `pos`, or `None`.
    pub fn char_code_before_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<u32> {
        let prior_byte = self.prior_char_start_emacs_byte_pos(pos)?;
        self.text.char_code_at_emacs_byte_pos(prior_byte)
    }

    /// Byte position where the character ending at `pos` starts: step back
    /// over continuation bytes, at most 4 (the 5-byte internal encoding's
    /// worst case). The internal encoding is self-synchronizing like UTF-8,
    /// the same invariant the syntax scanner's backward step relies on —
    /// the previous implementation did a byte->char AND a char->byte
    /// conversion (two anchored scans) per backward peek, and regexp
    /// word-boundary checks peek backward per candidate position.
    fn prior_char_start_emacs_byte_pos(&self, pos: EmacsBytePos) -> Option<EmacsBytePos> {
        if pos == EmacsBytePos::ZERO || pos > self.total_emacs_byte_end_pos() {
            return None;
        }
        let mut prior = pos.get() - 1;
        if self.get_multibyte() {
            let mut steps = 0;
            while prior > 0
                && steps < 4
                && self
                    .emacs_byte_at_pos(EmacsBytePos::new(prior))
                    .is_some_and(|b| (b & 0xC0) == 0x80)
            {
                prior -= 1;
                steps += 1;
            }
        }
        Some(EmacsBytePos::new(prior))
    }

    /// Emacs-byte width of the character starting at `pos`.
    pub fn char_after_emacs_byte_len(&self, pos: EmacsBytePos) -> Option<EmacsByteLen> {
        let end = self.total_emacs_byte_end_pos();
        if pos >= end {
            return None;
        }
        // GNU `BYTES_BY_CHAR_HEAD`: at a character boundary the lead byte
        // alone gives the width — no byte->char->byte round trip (two
        // anchored scans per call, which dominated skip-chars-forward and
        // scan_for_column on multibyte buffers). A continuation byte means
        // the caller handed a mid-character position; keep the exact old
        // answer (distance to the next character start) for that case.
        if self.get_multibyte()
            && let Some(lead) = self.text.emacs_byte_at_pos(pos)
            && !(0x80..=0xBF).contains(&lead)
        {
            let len = crate::emacs_core::emacs_char::bytes_by_char_head(lead);
            let remaining = end.get().saturating_sub(pos.get());
            return Some(EmacsByteLen::new(len.min(remaining).max(1)));
        }
        if !self.get_multibyte() {
            return Some(EmacsByteLen::new(1));
        }
        let char_idx = self.text.emacs_byte_pos_to_char_pos(pos);
        let next_byte = self
            .text
            .char_pos_to_emacs_byte_pos(char_idx.add_len(CharLen::new(1)));
        Some(next_byte.saturating_offset_from(pos))
    }

    /// Emacs-byte width of the character ending at `pos`.
    pub fn char_before_emacs_byte_len(&self, pos: EmacsBytePos) -> Option<EmacsByteLen> {
        let prior_byte = self.prior_char_start_emacs_byte_pos(pos)?;
        Some(pos.saturating_offset_from(prior_byte))
    }

    // -- Narrowing -----------------------------------------------------------

    /// Restrict the accessible portion to the Emacs-byte range.
    pub fn narrow_to_emacs_byte_range(&mut self, range: EmacsByteRange) {
        let total = self.total_emacs_byte_end_pos();
        let start_byte = range.start().min(total);
        let end_byte = range.end().clamp(start_byte, total);
        let start = self.text_anchor_for_emacs_byte_pos(start_byte);
        let end = self.text_anchor_for_emacs_byte_pos(end_byte);
        self.set_accessible_region_anchors_unchecked(start, end);
        // Clamp point into the new accessible region.
        self.goto_emacs_byte_pos(self.point_emacs_byte_pos());
    }

    /// Set the accessible byte range and point as one coherent buffer state.
    ///
    /// This mirrors GNU's invariant that `PT`/`PT_BYTE`, `BEGV`/`BEGV_BYTE`
    /// and `ZV`/`ZV_BYTE` are paired coordinates.  Callers provide byte
    /// positions; the active text backend recomputes the matching character
    /// positions.
    pub fn set_accessible_region_and_point_from_emacs_bytes(
        &mut self,
        range: EmacsByteRange,
        point: EmacsBytePos,
    ) {
        self.narrow_to_emacs_byte_range(range);
        self.goto_emacs_byte_pos(point);
    }

    /// Remove narrowing — make the entire buffer accessible again.
    pub fn widen(&mut self) {
        self.narrow_to_emacs_byte_range(self.full_emacs_byte_range());
    }

    pub fn accessible_region_snapshot(&self) -> AccessibleBufferRegionSnapshot {
        AccessibleBufferRegionSnapshot {
            start_char: self.point_min_char_pos(),
            start_emacs_byte: self.point_min_emacs_byte_pos(),
            end_char: self.point_max_char_pos(),
            end_emacs_byte: self.point_max_emacs_byte_pos(),
        }
    }

    pub fn restore_accessible_region(&mut self, snapshot: AccessibleBufferRegionSnapshot) {
        self.set_accessible_region_anchors_unchecked(
            snapshot.start_anchor(),
            snapshot.end_anchor(),
        );
        self.goto_emacs_byte_pos(self.point_emacs_byte_pos());
    }

    pub fn restore_accessible_region_with_current_full_end(
        &mut self,
        snapshot: AccessibleBufferRegionSnapshot,
    ) {
        self.set_accessible_region_anchors_unchecked(
            snapshot.start_anchor(),
            self.text_anchor_for_emacs_byte_pos(self.total_emacs_byte_end_pos()),
        );
        self.goto_emacs_byte_pos(self.point_emacs_byte_pos());
    }

    pub fn register_marker_at_emacs_byte_pos(
        &mut self,
        marker_ptr: *mut crate::tagged::header::MarkerObj,
        marker_id: u64,
        pos: EmacsBytePos,
        insertion_type: InsertionType,
    ) {
        let position = self.marker_anchor_for_emacs_byte_pos(pos);
        self.register_marker_at_anchor(marker_ptr, marker_id, position, insertion_type);
    }

    pub fn register_marker_at_anchor(
        &mut self,
        marker_ptr: *mut crate::tagged::header::MarkerObj,
        marker_id: u64,
        position: TextPositionAnchor,
        insertion_type: InsertionType,
    ) {
        let position = self.canonical_marker_anchor(position);
        self.text
            .register_marker(marker_ptr, self.id, marker_id, position, insertion_type);
    }

    fn marker_anchor_for_emacs_byte_pos(&self, pos: EmacsBytePos) -> TextPositionAnchor {
        self.text_anchor_for_emacs_byte_pos(pos)
    }

    fn text_anchor_for_emacs_byte_pos(&self, pos: EmacsBytePos) -> TextPositionAnchor {
        let total_byte_end = self.total_emacs_byte_end_pos();
        let clamped = pos.min(total_byte_end);
        let char_pos = if clamped == total_byte_end {
            self.total_char_end_pos()
        } else {
            self.text.emacs_byte_pos_to_char_pos(clamped)
        };
        TextPositionAnchor::new(char_pos, clamped)
    }

    fn accessible_anchor_for_emacs_byte_pos(&self, pos: EmacsBytePos) -> TextPositionAnchor {
        let clamped = pos.clamp(
            self.point_min_emacs_byte_pos(),
            self.point_max_emacs_byte_pos(),
        );
        if clamped == self.point_min_emacs_byte_pos() {
            self.point_min_anchor()
        } else if clamped == self.point_max_emacs_byte_pos() {
            self.point_max_anchor()
        } else {
            self.text_anchor_for_emacs_byte_pos(clamped)
        }
    }

    pub(in crate::buffer) fn set_point_anchor_unchecked(&mut self, anchor: TextPositionAnchor) {
        self.point = anchor;
    }

    pub(in crate::buffer) fn set_accessible_region_anchors_unchecked(
        &mut self,
        start: TextPositionAnchor,
        end: TextPositionAnchor,
    ) {
        self.accessible_start = start;
        self.accessible_end = end;
    }

    fn canonical_marker_anchor(&self, position: TextPositionAnchor) -> TextPositionAnchor {
        self.marker_anchor_for_emacs_byte_pos(position.emacs_byte_pos())
    }

    pub fn remove_marker_entry(&mut self, marker_id: u64) {
        self.text.remove_marker(marker_id);
    }

    pub fn has_marker(&self, marker_id: u64) -> bool {
        self.text.has_marker(marker_id)
    }

    pub fn marker_chain_anchor_lookup(
        &self,
        marker_id: u64,
    ) -> Option<(TextPositionAnchor, InsertionType)> {
        self.text.marker_chain_anchor_lookup(marker_id)
    }

    #[cfg(test)]
    pub(crate) fn marker_chain_len(&self) -> usize {
        self.text.chain_walk_collect().len()
    }

    #[cfg(test)]
    pub(crate) unsafe fn marker_chain_contains_raw_for_test(
        &self,
        targets: [*mut crate::tagged::header::MarkerObj; 3],
    ) -> [bool; 3] {
        let head_slot = unsafe { self.text.markers_head_slot_raw() };
        let mut contains = [false; 3];
        let mut curr = unsafe { *head_slot };
        let mut guard = 0usize;
        while !curr.is_null() && guard < 4096 {
            for (index, target) in targets.iter().enumerate() {
                if curr == *target {
                    contains[index] = true;
                }
            }
            curr = unsafe { (*curr).data.next_marker };
            guard += 1;
        }
        contains
    }

    pub fn marker_value_by_id(&self, marker_id: u64) -> Option<Value> {
        let ptr = self.text.chain_find_by_id(marker_id);
        if ptr.is_null() {
            None
        } else {
            // SAFETY: `ptr` was found by walking this buffer's live marker
            // chain. The returned Value is only a Lisp handle to that live
            // MarkerObj; callers do not retain the raw pointer.
            unsafe {
                Some(Value::from_veclike_ptr(
                    ptr as *const crate::tagged::header::VecLikeHeader,
                ))
            }
        }
    }

    pub fn unlink_marker_ptr(&self, ptr: *mut crate::tagged::header::MarkerObj) {
        self.text.chain_unlink(ptr);
    }

    pub fn move_marker_to_anchor(&self, marker_id: u64, position: TextPositionAnchor) {
        self.text.move_marker_to_anchor(marker_id, position);
    }

    pub fn update_marker_insertion_type(&mut self, marker_id: u64, insertion_type: InsertionType) {
        self.text
            .update_marker_insertion_type(marker_id, insertion_type);
    }

    // -- Mark ----------------------------------------------------------------
    // GNU: the mark IS a Lisp_Marker (BVAR(buf, mark)).  There are no
    // separate position fields.  The marker tracks its own position
    // through the buffer's marker chain and auto-adjusts on edits.

    /// Set the mark to Emacs-byte position `pos`, creating the marker if needed.
    pub fn set_mark_emacs_byte_pos(&mut self, pos: EmacsBytePos) {
        let position = self.marker_anchor_for_emacs_byte_pos(pos);
        if self.mark_marker_ptr.is_null() {
            // Create the marker eagerly and register in the chain so it
            // auto-adjusts on edits.
            let marker = crate::emacs_core::value::Value::make_marker(positioned_marker_data(
                self.id,
                crate::emacs_core::marker::MARK_MARKER_ID,
                position,
                super::InsertionType::Before,
            ));
            let ptr = marker
                .as_veclike_ptr()
                .expect("freshly allocated marker should have veclike ptr")
                as *mut crate::tagged::header::MarkerObj;
            self.text.register_marker(
                ptr,
                self.id,
                crate::emacs_core::marker::MARK_MARKER_ID,
                position,
                super::InsertionType::Before,
            );
            self.mark_marker_ptr = ptr;
        } else {
            // Update existing marker position via the chain
            let ptr = self.mark_marker_ptr;
            unsafe {
                set_marker_data_anchor(&mut (*ptr).data, position);
            }
        }
    }

    /// Return the mark as an Emacs byte position, None if mark inactive.
    pub fn mark_emacs_byte_pos(&self) -> Option<EmacsBytePos> {
        if self.mark_marker_ptr.is_null() {
            None
        } else {
            unsafe { Some(marker_data_anchor(&(*self.mark_marker_ptr).data).emacs_byte_pos()) }
        }
    }

    /// Return the mark character position, None if mark inactive.
    pub fn mark_char_pos(&self) -> Option<CharPos0> {
        if self.mark_marker_ptr.is_null() {
            None
        } else {
            unsafe { Some(marker_data_anchor(&(*self.mark_marker_ptr).data).char_pos()) }
        }
    }

    /// Deactivate the mark.
    pub fn clear_mark(&mut self) {
        self.mark_marker_id = None;
        self.mark_marker_ptr = std::ptr::null_mut();
    }

    /// GNU `buffer_visited_file_modtime (current_buffer)`
    /// (`src/fileio.c:6156-6175`): THIS buffer's recorded visited-file
    /// modification time, which is what `visited-file-modtime` returns.
    ///
    /// An indirect buffer's own modtime is the unknown sentinel
    /// `reset_buffer` left there -- it visits no file -- so this is NOT the
    /// value the first-change undo entry records; see
    /// [`Self::first_change_modtime`].
    pub fn visited_file_modtime(&self) -> VisitedFileModtime {
        self.modtime.own()
    }

    /// The Lisp value `visited-file-modtime` returns for this buffer.
    pub fn visited_file_modtime_value(&self) -> Value {
        self.visited_file_modtime().to_lisp_value()
    }

    /// Record a visited-file modtime for THIS buffer. GNU's
    /// `Fset_visited_file_modtime`, `insert-file-contents` and `write-region`
    /// all write `current_buffer->modtime`, never a base buffer's.
    pub fn set_visited_file_modtime(&mut self, modtime: VisitedFileModtime) {
        self.modtime.set_own(modtime);
    }

    /// GNU `record_first_change` (`src/undo.c:209-223`): the modtime the
    /// `(t . TIME)` undo entry records -- the BASE buffer's when this is an
    /// indirect buffer, because the entry names the save that the shared TEXT
    /// would be returned to and only the base visits the file.
    ///
    /// The returned [`FirstChangeModtime`] is the only thing the undo recorder
    /// accepts, and this is its only source, so no call site can record the
    /// current buffer's own modtime instead.
    pub(in crate::buffer) fn first_change_modtime(&self) -> FirstChangeModtime {
        self.modtime.for_first_change()
    }

    /// Test-facing: this buffer's own modtime cell, to assert an indirect
    /// buffer follows the base the manager owns rather than a copy.
    #[cfg(test)]
    pub(in crate::buffer) fn share_modtime_cell(&self) -> BaseVisitedFileModtime {
        self.modtime.share_own()
    }

    /// Test-facing counterpart of [`Self::share_modtime_cell`].
    #[cfg(test)]
    pub(in crate::buffer) fn follows_modtime_cell_of(&self, base: &BaseVisitedFileModtime) -> bool {
        self.modtime.follows(base)
    }

    // -- Modified flag -------------------------------------------------------

    pub fn modified_tick(&self) -> i64 {
        self.text.modified_tick()
    }

    pub fn chars_modified_tick(&self) -> i64 {
        self.text.chars_modified_tick()
    }

    /// The accumulated dirty char range `[beg, end)` since the last redisplay
    /// ack, or `None` if nothing changed (incremental-layout Phase 3, spec §6).
    /// Buffer-absolute char positions; the window side intersects with the
    /// accessible/visible region.
    pub fn changed_char_range(&self) -> Option<(i64, i64)> {
        let current_z = self.text.char_count().get() as i64;
        self.text.changed_char_range(current_z)
    }

    /// Reset the unchanged-region accumulator — the redisplay ack, performed at
    /// the committed (accepted) layout break.
    pub fn reset_unchanged_region(&self) {
        self.text.reset_unchanged_region();
    }

    /// Tick that advances on every text-PROPERTY change to this buffer
    /// (face/display/invisible/composition props), without moving the chars
    /// tick. Source-of-truth signal for incremental redisplay invalidation.
    pub fn props_modified_tick(&self) -> i64 {
        self.text.props_modified_tick()
    }

    /// Record a text-property modification on this buffer (advances
    /// `props_modified_tick` + `modified_tick`, not `chars_modified_tick`).
    pub fn record_text_property_modification(&mut self) {
        self.text.record_text_property_modification();
    }

    /// Fold a text-property change over `byte_range` into the unchanged-region
    /// accumulator. GNU `modify_text_properties` (textprop.c) runs
    /// `BUF_COMPUTE_UNCHANGED (buf, start, end)` for every property write,
    /// exactly like a char edit; mirroring that lets incremental layout treat
    /// font-lock's per-keystroke refontification as a bounded dirty span
    /// instead of escalating the whole window to a Full rebuild. A property
    /// change never moves characters, so `old_z` is the current char count.
    pub fn note_changed_property_region(&self, byte_range: EmacsByteRange) {
        let range = self.text.byte_range_to_char_range(byte_range);
        let z = self.text.char_count().get() as i64;
        self.text
            .note_changed_char_region(range.start().get() as i64, range.end().get() as i64, z);
    }

    pub fn save_modified_tick(&self) -> i64 {
        self.text.save_modified_tick()
    }

    pub fn overlay_modified_tick(&self) -> i64 {
        self.overlay_modified_tick
    }

    pub fn increment_overlay_modified_tick(&mut self) {
        self.overlay_modified_tick = self.overlay_modified_tick.wrapping_add(1);
    }

    pub fn is_modified(&self) -> bool {
        self.save_modified_tick() < self.modified_tick()
    }

    pub fn modified_state_value(&self) -> Value {
        if self.save_modified_tick() < self.modified_tick() {
            if self.autosave_modified_tick == self.modified_tick() {
                Value::symbol("autosaved")
            } else {
                Value::T
            }
        } else {
            Value::NIL
        }
    }

    pub fn recent_auto_save_p(&self) -> bool {
        self.save_modified_tick() < self.autosave_modified_tick
    }

    pub fn set_modified(&mut self, flag: bool) {
        if flag {
            if self.save_modified_tick() >= self.modified_tick() {
                self.text.increment_modified_tick(1);
            }
        } else {
            self.text.set_save_modified_tick(self.modified_tick());
        }
    }

    pub fn restore_modified_state(&mut self, flag: Value) -> Value {
        if flag.is_nil() {
            self.text.set_save_modified_tick(self.modified_tick());
        } else {
            if self.save_modified_tick() >= self.modified_tick() {
                self.text.increment_modified_tick(1);
            }
            if flag == Value::symbol("autosaved") {
                self.autosave_modified_tick = self.modified_tick();
            }
        }
        flag
    }

    pub fn mark_auto_saved(&mut self) {
        self.autosave_modified_tick = self.modified_tick();
    }

    // -- Buffer-local variables ----------------------------------------------

    /// Write a per-buffer binding. Mirrors GNU `set_internal`
    /// SYMBOL_FORWARDED arm (`data.c:1774-1786`) for slot-backed
    /// names and the SYMBOL_LOCALIZED arm for everything else:
    ///
    /// * Slot-backed (BUFFER_OBJFWD) — write
    ///   `slots[offset]`, setting the per-buffer local-flags bit
    ///   for conditional slots (`SET_PER_BUFFER_VALUE_P`).
    /// * `buffer-undo-list` — writes to [`SharedUndoState`], which
    ///   is the single source of truth shared across indirect
    ///   buffers.
    /// * Everything else — intern `name` to a SymId and store the
    ///   binding in [`Self::local_var_alist`]. Existing entries
    ///   are mutated in place so any [`LispBufferLocalValue`]
    ///   `valcell` still points at the same cons. New entries are
    ///   prepended to the alist.
    pub fn set_buffer_local(&mut self, name: &str, value: Value) {
        self.set_buffer_local_by_sym_id(intern(name), value);
    }

    pub fn set_buffer_local_by_sym_id(&mut self, sym_id: SymId, value: Value) {
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            self.slots[info.offset.index()] = value;
            if info.local_flags_idx >= 0 {
                self.set_slot_local_flag(info.offset, true);
            }
            return;
        }
        if sym_id == buffer_undo_list_sym() {
            self.undo_state.set_list(value);
            if value.is_nil() {
                self.undo_state.set_recorded_first_change(false);
            }
            return;
        }
        self.local_var_alist.set(sym_id, value);
    }

    /// Mark a per-buffer binding as void. Slot-backed names reset
    /// to nil; `buffer-undo-list` clears the undo state; all other
    /// names drop their entry from `local_var_alist` entirely.
    /// GNU doesn't have a true "void per-buffer binding" — removing
    /// the alist entry is the closest equivalent.
    pub fn set_buffer_local_void(&mut self, name: &str) {
        self.set_buffer_local_void_by_sym_id(intern(name));
    }

    pub fn set_buffer_local_void_by_sym_id(&mut self, sym_id: SymId) {
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            self.slots[info.offset.index()] = Value::NIL;
            return;
        }
        if sym_id == buffer_undo_list_sym() {
            self.undo_state.set_list(Value::NIL);
            self.undo_state.set_recorded_first_change(false);
            return;
        }
        self.local_var_alist.remove(sym_id);
    }

    /// Drop a per-buffer binding. Returns the previous binding if
    /// one existed. Mirrors the non-special path of GNU
    /// `Fkill_local_variable` (`data.c:2314-2378`).
    pub fn kill_buffer_local(&mut self, name: &str) -> Option<RuntimeBindingValue> {
        self.kill_buffer_local_by_sym_id(intern(name))
    }

    pub fn kill_buffer_local_by_sym_id(&mut self, sym_id: SymId) -> Option<RuntimeBindingValue> {
        if sym_id == buffer_undo_list_sym() {
            return None;
        }
        let existing = self.local_var_alist.value(sym_id)?;
        self.local_var_alist.remove(sym_id);
        Some(RuntimeBindingValue::Bound(existing))
    }

    pub fn kill_all_local_variables(
        &mut self,
        obarray: &mut crate::emacs_core::symbol::Obarray,
        kill_permanent: bool,
        buffer_defaults: &[crate::emacs_core::value::Value; BUFFER_SLOT_COUNT],
    ) {
        // Mirrors GNU `reset_buffer_local_variables'
        // (`buffer.c:1135-1234'). Three things happen:
        //
        //   1. Specific always-local slots get reset (major-mode,
        //      mode-name, invisibility-spec, the case tables, the
        //      keymap). GNU does these explicitly at the top of
        //      `reset_buffer_local_variables'. Neomacs encodes them
        //      via `BufferSlotInfo.reset_on_kill = true' for the
        //      slot-backed ones; the keymap is reset at the end.
        //
        //   2. Conditional slots are reset by clearing
        //      `local_flags[idx]', UNLESS `permanent_local' is set
        //      (matches GNU's `buffer_permanent_local_flags' table
        //      at `buffer.c:109,4751,4767'). Permanent conditional
        //      slots in upstream GNU are `truncate-lines' and
        //      `buffer-file-coding-system' -- both survive
        //      kill-all-local-variables.
        //
        //   3. The LOCALIZED `local_var_alist' is walked and
        //      non-`permanent-local' entries are spliced out
        //      (`buffer.c:1163-1228'). The `permanent-local-hook'
        //      partial-preserve filter runs in-place. See the
        //      walking loop below.
        //
        // Always-local slots that GNU does NOT explicitly reset
        // (`buffer-file-name', `default-directory', `mark-active',
        // `point-before-scroll', `buffer-display-count',
        // `buffer-display-time', `buffer-read-only', etc.) are
        // left untouched here. They have `reset_on_kill: false'.
        for info in BUFFER_SLOT_INFO {
            if info.local_flags_idx >= 0 {
                // Conditional slot. Skip if permanent (matches
                // GNU's `buffer_permanent_local_flags[idx] != 0'
                // gate at `buffer.c:1232'). The `kill_permanent'
                // flag overrides permanence -- it's used by
                // internal callers like `reset_buffer_local_variables(b, 1)'
                // for buffer creation/deletion. Ordinary
                // `kill-all-local-variables' calls pass
                // `kill_permanent = false' so permanent slots
                // survive.
                if info.permanent_local && !kill_permanent {
                    continue;
                }
                self.set_slot_local_flag(info.offset, false);
                // GNU `buffer.c:1242` — `set_per_buffer_value(b, offset,
                // per_buffer_default(offset))`. The reset target is the
                // CURRENT runtime buffer-defaults slot, NOT the
                // install-time `BufferSlotInfo::default` seed. The
                // distinction matters for any slot whose default got
                // updated by `setq-default` (e.g. bindings.el sets the
                // rich `mode-line-format` list — before this fix, the
                // reset here would clobber it back to the install-time
                // "%-" seed after any kill-all-local-variables call,
                // leaving the layout engine to render only the buffer
                // name).
                self.slots[info.offset.index()] = buffer_defaults[info.offset.index()];
            } else if info.reset_on_kill {
                // Always-local slot in GNU's explicit reset list
                // (major-mode, mode-name, invisibility-spec). These are
                // hardcoded resets in GNU (Qfundamental_mode, QSFundamental,
                // Qt) that don't participate in buffer-defaults, so the
                // install-time seed is the right value here.
                self.slots[info.offset.index()] = info.default.to_value();
            }
        }

        // Phase 10E: walk `local_var_alist` and remove non-permanent
        // entries IN PLACE. Mirrors GNU `reset_buffer_local_variables`
        // at `buffer.c:1168-1225`, whose `last'-cursor loop unlinks each
        // dropped entry with `XSETCDR (last, XCDR (tmp))'.
        //
        // Permanent locals (`(get sym 'permanent-local)`) survive
        // unconditionally. Permanent-local-hook variables get their
        // hook list filtered to keep only the permanent entries.
        // The filter MUTATES the existing cell's cdr in place so
        // any BLV whose valcell points at the cell still observes
        // the filtered value without needing a re-swap.
        //
        // Removed entries trigger a BLV cache reset so the next
        // read for that LOCALIZED variable falls through to the
        // global default.
        //
        // The splice runs inside `LocalVariableBindings::retain_bindings` so
        // the derived SymId -> binding-cons index is invalidated on the same
        // path. It has to be: when the newest local is the permanent one it
        // sits at the alist HEAD and survives, so the filtered list starts at
        // the very cons it started at before and only interior entries are
        // unlinked. Head identity therefore proves nothing here.
        self.local_var_alist.retain_bindings(|entry| {
            let Some(name) = entry.cons_car().as_symbol_name() else {
                return BindingRetention::Drop;
            };
            if !kill_permanent
                && let Some(prop) = obarray
                    .get_property(name, "permanent-local")
                    .filter(|v| !v.is_nil())
            {
                if prop.is_symbol_named("permanent-local-hook") {
                    // Partial-preserve: filter the value and mutate the
                    // existing cell's cdr in place, so a BLV valcell aimed at
                    // this cons observes the filtered list without re-swapping.
                    let preserved =
                        preserve_partial_permanent_local_hook_value(obarray, entry.cons_cdr());
                    entry.set_cdr(preserved);
                }
                return BindingRetention::Keep;
            }
            // Dropped. Reset the BLV cache for this LOCALIZED variable so
            // subsequent reads re-swap to the global default. Mirrors GNU's
            // `swap_in_global_binding' call at `buffer.c:1185'.
            let id = crate::emacs_core::intern::intern(name);
            if let Some(blv) = obarray.blv_mut(id) {
                blv.where_buf = Value::NIL;
                blv.found = false;
                blv.valcell = blv.defcell;
            }
            BindingRetention::Drop
        });

        // GNU `reset_buffer_local_variables` also clears the
        // buffer's local keymap (`buffer.c:1337`).
        self.keymap = Value::NIL;
    }

    pub fn get_buffer_local(&self, name: &str) -> Option<Value> {
        self.get_buffer_local_by_sym_id(intern(name))
    }

    pub fn get_buffer_local_by_sym_id(&self, sym_id: SymId) -> Option<Value> {
        self.get_buffer_local_by_sym_id_gated(sym_id, true)
    }

    /// Same as [`Self::get_buffer_local_by_sym_id`], but the caller passes
    /// whether `sym_id` is a `Localized` symbol (via `Obarray::is_localized`).
    /// When it is not, the `local_var_alist` walk is skipped: a non-localized
    /// symbol can never have an alist entry (see `Obarray::is_localized`), so
    /// the scan would only ever return `None` after walking the whole list.
    /// This is the hot path for display/VM variable resolution, where most
    /// referenced specials are global. Slot-backed and `buffer-undo-list`
    /// names are always resolved (they are per-buffer without being localized).
    pub(crate) fn get_buffer_local_by_sym_id_gated(
        &self,
        sym_id: SymId,
        localized: bool,
    ) -> Option<Value> {
        #[cfg(test)]
        BUFFER_LOCAL_VALUE_LOOKUP_PROBES
            .set(BUFFER_LOCAL_VALUE_LOOKUP_PROBES.get().saturating_add(1));
        // Slot-backed names resolve to the live slot value, mirroring
        // GNU's `BVAR(buf, …)` accessor. Conditional slots only
        // report a per-buffer binding when the local-flags bit is
        // set; the caller falls through to the global default at a
        // higher layer that has access to `BufferManager::buffer_defaults`.
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            if info.local_flags_idx >= 0 && !self.slot_local_flag(info.offset) {
                return None;
            }
            return Some(self.slots[info.offset.index()]);
        }
        // `buffer-undo-list` reads through `SharedUndoState` so
        // indirect buffers see the root buffer's undo state.
        if let Some(dedicated) = DedicatedBufferLocal::from_sym_id(sym_id) {
            return Some(dedicated.read(self));
        }
        if !localized {
            return None;
        }
        // Everything else: identity-lookup the binding cons derived from
        // `local_var_alist`. This has the same first-entry semantics as GNU's
        // `assq_no_quit (var, BVAR (buf, local_var_alist))` at `data.c:2409`,
        // while repeated reads do not rewalk the entire list. A `Qunbound` cdr
        // is a "local but void" marker — report it as absent for this
        // read-style API. Use `get_buffer_local_binding` when the
        // Bound/Void/absent distinction matters.
        self.local_var_alist
            .value(sym_id)
            .filter(|v| !v.is_unbound())
    }

    /// Walk this buffer's `local_var_alist` for an `(sym . val)`
    /// pair whose car matches `key`. Returns the cdr if found.
    /// Mirrors GNU's `assq_no_quit (variable, BVAR (buf, local_var_alist))`
    /// at `data.c:2409`.
    ///
    /// Used by Phase 10E callers that need to look up per-buffer
    /// values for LOCALIZED symbols without going through the
    /// obarray's BLV swap-in.
    pub fn find_in_local_var_alist(&self, key: Value) -> Option<Value> {
        if let Some(sym_id) = key.as_symbol_id() {
            return self.local_var_alist.value(sym_id);
        }
        let mut alist = self.local_var_alist.as_lisp_alist();
        while alist.is_cons() {
            let entry = alist.cons_car();
            if entry.is_cons() && crate::emacs_core::value::eq_value(&entry.cons_car(), &key) {
                return Some(entry.cons_cdr());
            }
            alist = alist.cons_cdr();
        }
        None
    }

    pub fn get_buffer_local_binding(&self, name: &str) -> Option<RuntimeBindingValue> {
        self.get_buffer_local_binding_by_sym_id(intern(name))
    }

    pub fn get_buffer_local_binding_by_sym_id(&self, sym_id: SymId) -> Option<RuntimeBindingValue> {
        self.get_buffer_local_binding_by_sym_id_gated(sym_id, true)
    }

    /// Gated form of [`Self::get_buffer_local_binding_by_sym_id`]; see
    /// [`Self::get_buffer_local_by_sym_id_gated`] for the `localized` contract.
    pub(crate) fn get_buffer_local_binding_by_sym_id_gated(
        &self,
        sym_id: SymId,
        localized: bool,
    ) -> Option<RuntimeBindingValue> {
        // BUFFER_OBJFWD slots are always live and bypass any
        // "present/absent" short-circuit. They never go void in
        // GNU — a nil slot still resolves as Bound(nil).
        // Conditional slots (`local_flags_idx >= 0`) only report a
        // per-buffer binding when the local-flag bit is set;
        // otherwise the caller falls through to the global default.
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            if info.local_flags_idx >= 0 && !self.slot_local_flag(info.offset) {
                return None;
            }
            return Some(RuntimeBindingValue::Bound(self.slots[info.offset.index()]));
        }
        if let Some(dedicated) = DedicatedBufferLocal::from_sym_id(sym_id) {
            return Some(RuntimeBindingValue::Bound(dedicated.read(self)));
        }
        if !localized {
            return None;
        }
        // An UNBOUND cdr in the alist marks a void per-buffer
        // binding — the variable IS local (Some) but has no
        // value (Void). Mirrors GNU's `(var . Qunbound)` alist
        // entries created by `Fmake_local_variable` on a void
        // symbol at `data.c:2285-2289`.
        self.local_var_alist.value(sym_id).map(|v| {
            if v.is_unbound() {
                RuntimeBindingValue::Void
            } else {
                RuntimeBindingValue::Bound(v)
            }
        })
    }

    pub fn has_buffer_local(&self, name: &str) -> bool {
        self.has_buffer_local_by_sym_id(intern(name))
    }

    pub fn has_buffer_local_by_sym_id(&self, sym_id: SymId) -> bool {
        self.has_buffer_local_by_sym_id_gated(sym_id, true)
    }

    /// Gated form of [`Self::has_buffer_local_by_sym_id`]; the caller passes
    /// whether `sym_id` is `Localized` (via `Obarray::is_localized`). A
    /// non-localized symbol can never have an alist entry (see
    /// `Obarray::is_localized`), so the alist walk is skipped for it. Slot and
    /// `buffer-undo-list` names are still resolved (always per-buffer).
    pub(crate) fn has_buffer_local_by_sym_id_gated(&self, sym_id: SymId, localized: bool) -> bool {
        // BUFFER_OBJFWD-style names are conceptually always
        // per-buffer (mirrors GNU's `local-variable-p` returning t
        // for DEFVAR_PER_BUFFER variables regardless of whether the
        // user explicitly called `make-local-variable`).
        // Conditional slots only count as local when the per-buffer
        // flag bit is set — mirrors GNU `local-variable-p`
        // dispatching through `PER_BUFFER_VALUE_P` at
        // `data.c:2347-2380`.
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            if info.local_flags_idx >= 0 {
                return self.slot_local_flag(info.offset);
            }
            return true;
        }
        // `buffer-undo-list` is always present (its SharedUndoState
        // is unconditionally allocated; there's no "unset" state).
        if sym_id == buffer_undo_list_sym() {
            return true;
        }
        if !localized {
            return false;
        }
        self.local_var_alist.value(sym_id).is_some()
    }

    pub fn local_map(&self) -> Value {
        self.keymap
    }

    pub fn set_local_map(&mut self, keymap: Value) {
        self.keymap = keymap;
    }

    /// Mirror of GNU `buffer_local_value` at `buffer.c:1359-1413`.
    ///
    /// For `SYMBOL_FORWARDED` BUFFER_OBJFWD vars (our `BufferSlotInfo`
    /// slot-backed names), GNU unconditionally reads
    /// `per_buffer_value(buf, offset)` at `buffer.c:1405` — it does NOT
    /// check `PER_BUFFER_VALUE_P`. The flag only distinguishes "this
    /// buffer has a local override" from "this buffer uses the
    /// runtime default"; the slot itself is always populated.
    ///
    /// `get_buffer_local_binding` returns `None` when the flag is
    /// clear (it's the lower-level "is there a local override?"
    /// primitive), which is correct for callers like `local-variable-p`.
    /// But `buffer_local_value` should NOT return `None` in that
    /// case — it should return the slot value. Before this fix, the
    /// layout engine's mode-line read was getting `None` for every
    /// conditional slot in its "virgin" state, falling back to the
    /// obarray value cell (which for forwarded vars is `nil`), and
    /// therefore asking `format-mode-line` to render `nil`, which
    /// produced an empty mode-line containing only the buffer name.
    pub fn buffer_local_value(&self, name: &str) -> Option<Value> {
        if let Some(info) = lookup_buffer_slot(name) {
            return Some(self.slots[info.offset.index()]);
        }
        if name == BUFFER_UNDO_LIST_NAME {
            return Some(self.get_undo_list());
        }
        match self.get_buffer_local_binding(name) {
            Some(RuntimeBindingValue::Bound(value)) => Some(value),
            Some(RuntimeBindingValue::Void) | None => None,
        }
    }

    /// [`Self::buffer_local_value`] for a pre-interned symbol — the form hot
    /// callers (layout reads every redisplay) use to skip the per-call string
    /// hash + obarray probe.
    pub fn buffer_local_value_id(&self, sym_id: SymId) -> Option<Value> {
        if let Some(info) = lookup_buffer_slot_by_sym_id(sym_id) {
            return Some(self.slots[info.offset.index()]);
        }
        if sym_id == buffer_undo_list_sym() {
            return Some(self.get_undo_list());
        }
        match self.get_buffer_local_binding_by_sym_id(sym_id) {
            Some(RuntimeBindingValue::Bound(value)) => Some(value),
            Some(RuntimeBindingValue::Void) | None => None,
        }
    }

    pub fn ordered_buffer_local_bindings(&self) -> Vec<(SymId, RuntimeBindingValue)> {
        // Returns entries in REVERSED GNU order so the caller can
        // `.rev()' to get GNU's prepend-based final order.
        //
        // GNU `Fbuffer_local_variables' (`buffer.c:1471-1502'):
        //
        //   1. `buffer_lisp_local_variables(buf, 0)' walks
        //      `local_var_alist' forward, prepending each entry.
        //      Result: alist entries in REVERSE iteration order.
        //
        //   2. `FOR_EACH_PER_BUFFER_OBJECT_AT (offset)' walks slot
        //      offsets forward, prepending each applicable slot
        //      entry. Result so far: slot entries (reversed) at the
        //      FRONT of the alist entries.
        //
        //   3. Finally prepends the special `undo_list' slot via
        //      `buffer_local_variables_1(buf, ..., Qbuffer_undo_list)'.
        //
        // Final GNU order:
        //
        //     [undo_list,
        //      slot_N_rev, slot_N-1_rev, ..., slot_0_rev,
        //      alist_N_rev, alist_N-1_rev, ..., alist_0_rev]
        //
        // This function returns the REVERSE of that:
        //
        //     [alist_0, alist_1, ..., alist_N,
        //      slot_0, slot_1, ..., slot_N,
        //      undo_list]
        //
        // so `.rev()' in `builtin_buffer_local_variables' yields
        // GNU's exact order. The bare-symbol-vs-cons mapping for
        // `Qunbound' values happens at the caller's `.map()' step.
        //
        // Slot filter mirrors GNU's `buffer_local_variables_1':
        // emit when `local_flags_idx == -1' (always-local) OR
        // `PER_BUFFER_VALUE_P (buf, idx)' (the local-flag bit is
        // set). Internal-only slots (`install_as_forwarder: false')
        // are omitted because GNU skips slots with no Lisp variable
        // name (syntax_table_ etc.).
        let mut out: Vec<(SymId, RuntimeBindingValue)> = Vec::new();

        // Step 1: alist entries, walked forward, used UNREVERSED so
        // that `.rev()' in the caller flips them to match GNU's
        // `buffer_lisp_local_variables' prepend-based reversal.
        let mut cursor = self.local_var_alist.as_lisp_alist();
        while cursor.is_cons() {
            let entry = cursor.cons_car();
            cursor = cursor.cons_cdr();
            if !entry.is_cons() {
                continue;
            }
            if let Some(sym_id) = entry.cons_car().as_symbol_id() {
                let cdr = entry.cons_cdr();
                let binding = if cdr.is_unbound() {
                    RuntimeBindingValue::Void
                } else {
                    RuntimeBindingValue::Bound(cdr)
                };
                out.push((sym_id, binding));
            }
        }

        // Step 2: BUFFER_OBJFWD slots in GNU `struct buffer' order.
        // Same forward iteration; the `.rev()' in the caller flips
        // them to match GNU's prepend reversal.
        for offset in GNU_STRUCT_BUFFER_SLOT_ORDER {
            let Some(info) = buffer_slot_info_by_offset(*offset) else {
                continue;
            };
            if !info.install_as_forwarder {
                continue;
            }
            // GNU's filter: emit only when always-local
            // (local_flags_idx == -1) or the per-buffer flag bit is
            // set. Always-local slots in GNU correspond to neomacs
            // slots with `local_flags_idx < 0'.
            if info.local_flags_idx >= 0 && !self.slot_local_flag(info.offset) {
                continue;
            }
            out.push((
                intern(info.name),
                RuntimeBindingValue::Bound(self.slots[info.offset.index()]),
            ));
        }

        // Step 3: `buffer-undo-list' last in this Vec so `.rev()'
        // puts it FIRST in the final list, matching GNU's special
        // tail-prepend at `buffer.c:1496-1499'.
        out.push((
            buffer_undo_list_sym(),
            RuntimeBindingValue::Bound(self.get_undo_list()),
        ));

        out
    }

    pub fn ordered_buffer_local_names(&self) -> Vec<SymId> {
        self.ordered_buffer_local_bindings()
            .into_iter()
            .map(|(sym_id, _)| sym_id)
            .collect()
    }

    /// Walk the buffer's `local_var_alist` yielding a mutable
    /// reference into each entry's value via the cons cell's cdr
    /// field. Used by the GC-root visitor pipeline to avoid
    /// traversing the alist twice.
    pub fn bound_buffer_local_values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        // In the new design, local_var_alist IS the source of truth
        // for non-slot per-buffer bindings. Its cons cells are
        // already GC-traced via `Buffer::trace_roots` walking
        // `self.local_var_alist`, so nothing else needs a mutable
        // visitor. Keep the method signature for API compatibility
        // but return an empty iterator — the roots are reached
        // through the alist walk in `trace_roots`.
        std::iter::empty()
    }
}

impl Buffer {
    pub fn buffer_local_bound_p(&self, name: &str) -> bool {
        matches!(
            self.get_buffer_local_binding(name),
            Some(RuntimeBindingValue::Bound(_))
        )
    }

    pub fn buffer_local_void_p(&self, name: &str) -> bool {
        matches!(
            self.get_buffer_local_binding(name),
            Some(RuntimeBindingValue::Void)
        )
    }

    pub(in crate::buffer) fn set_text_properties_with_undo_range(
        &mut self,
        byte_range: EmacsByteRange,
        plist: Vec<(Value, Value)>,
    ) {
        let entries = text_properties_set_undo_entries(self, byte_range, &plist);
        record_buffer_text_property_undo_entries(self, entries);
        self.text
            .text_props_set_properties_in_emacs_byte_range(byte_range, plist);
    }
}

// ---------------------------------------------------------------------------
// BufferManager
// ---------------------------------------------------------------------------

/// Owns every live buffer, tracks the current buffer, and hands out ids.

/// The outcome of `BufferManager::add_undo_boundary`, mirroring the two paths
/// through GNU's `Fundo_boundary' (src/undo.c:251-282).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UndoBoundaryOutcome {
    /// The boundary was consed on and the editor-global saved point updated.
    /// GNU also sets `undo-auto--last-boundary-cause' to `explicit' on this
    /// path (:277); a caller that owns the obarray must do that.
    Recorded,
    /// `buffer-undo-list' is `t', so GNU returned at :258-259 and touched
    /// nothing at all -- no boundary, no saved point, no cause.
    UndoDisabled,
}

#[derive(Clone)]
pub struct BufferManager {
    // FxHashMap (not default SipHash): `buffers` is looked up on the hot
    // buffer-local access/bind path (specbind, get_buffer_local) keyed by the
    // small-int BufferId, where SipHash dominated the per-lookup cost.
    buffers: FxHashMap<BufferId, Buffer>,
    /// Killed buffer objects. GNU does not destroy the Lisp buffer object;
    /// it makes `BUFFER_LIVE_P` false while keeping slots like
    /// `last_name` and `filename` queryable.
    dead_buffers: FxHashMap<BufferId, Buffer>,
    buffer_order: Vec<BufferId>,
    current: Option<BufferId>,
    next_id: u64,
    next_marker_id: u64,
    labeled_restrictions: FxHashMap<BufferId, Vec<LabeledRestriction>>,
    default_text_backend_kind: ImplementedBufferTextBackendKind,
    /// Global default values for `BUFFER_OBJFWD` slots. Mirrors GNU's
    /// `buffer_defaults` (`buffer.c:84-90`), which is itself a
    /// sentinel `struct buffer` whose fields hold the global default
    /// for every per-buffer variable. Reads of a conditional slot
    /// (`local_flags_idx >= 0`) fall through here when the per-buffer
    /// `Buffer::local_flags` bit is clear; `setq-default` writes
    /// here directly. Phase 10D wires this in.
    pub buffer_defaults: [crate::emacs_core::value::Value; BUFFER_SLOT_COUNT],
    /// The editor's ONE saved point-before-command-or-undo (GNU's
    /// `point_before_last_command_or_undo` / `buffer_before_last_command_or_undo`,
    /// src/keyboard.c:232-233).  Every buffer this manager creates is handed a
    /// clone, which is why saving a point in one buffer supersedes the point
    /// saved in every other one -- see [`SavedPointBeforeCommand`].
    saved_point_before_command: SavedPointBeforeCommand,
}

#[derive(Clone)]
struct TextPropertyUndoRun {
    range: CharRange,
    source_range: CharRange,
    plist: Vec<(Value, Value)>,
}

impl TextPropertyUndoRun {
    fn covers_full_source_range(&self) -> bool {
        self.range == self.source_range
    }
}

struct TextPropertyUndoEntry {
    name: Value,
    old_value: Value,
    range: CharRange,
}

impl TextPropertyUndoEntry {
    fn new(name: Value, old_value: Value, range: CharRange) -> Self {
        Self {
            name,
            old_value,
            range,
        }
    }
}

fn plist_get_eq(plist: &[(Value, Value)], name: Value) -> Option<Value> {
    plist
        .iter()
        .find(|(prop, _)| eq_value(prop, &name))
        .map(|(_, value)| *value)
}

fn buffer_text_property_undo_runs(
    buf: &Buffer,
    byte_range: EmacsByteRange,
) -> Vec<TextPropertyUndoRun> {
    if byte_range.is_empty() {
        return Vec::new();
    }
    if undo::undo_list_is_disabled(&buf.get_undo_list()) {
        return Vec::new();
    }

    let changed_range = CharRange::new(
        buf.text.emacs_byte_pos_to_char_pos(byte_range.start()),
        buf.text.emacs_byte_pos_to_char_pos(byte_range.end()),
    );
    if changed_range.is_empty() {
        return Vec::new();
    }

    let mut runs = Vec::new();
    let mut cursor = changed_range.start();
    let _ = buf
        .text
        .text_props_try_for_each_interval_in_emacs_byte_range(byte_range, |source_range, plist| {
            let clipped_range = CharRange::new(
                source_range.start().max(changed_range.start()),
                source_range.end().min(changed_range.end()),
            );
            if clipped_range.is_empty() {
                return Ok::<(), ()>(());
            }
            if cursor < clipped_range.start() {
                runs.push(TextPropertyUndoRun {
                    range: CharRange::new(cursor, clipped_range.start()),
                    source_range: CharRange::new(cursor, clipped_range.start()),
                    plist: Vec::new(),
                });
            }
            runs.push(TextPropertyUndoRun {
                range: clipped_range,
                source_range,
                plist: plist.to_vec(),
            });
            cursor = clipped_range.end();
            Ok(())
        });
    if cursor < changed_range.end() {
        runs.push(TextPropertyUndoRun {
            range: CharRange::new(cursor, changed_range.end()),
            source_range: CharRange::new(cursor, changed_range.end()),
            plist: Vec::new(),
        });
    }
    runs
}

fn record_text_property_first_change(buf: &mut Buffer, undo_list: &mut Value) {
    // GNU `record_property_change` (undo.c:241) calls `record_first_change`
    // whenever `MODIFF <= SAVE_MODIFF` — the same clean->modified re-arm rule as
    // `record_point`.  Gate purely on the modified-tick comparison so the
    // sentinel is re-emitted after every `(set-buffer-modified-p nil)`.
    if undo::undo_list_is_disabled(undo_list) {
        return;
    }
    if buf.modified_tick() > buf.save_modified_tick() {
        return;
    }
    undo::undo_list_record_first_change(undo_list, buf.first_change_modtime());
    buf.undo_state.set_recorded_first_change(true);
}

fn record_buffer_text_property_undo_entries(buf: &mut Buffer, entries: Vec<TextPropertyUndoEntry>) {
    if entries.is_empty() {
        return;
    }
    let mut ul = buf.get_undo_list();
    if undo::undo_list_is_disabled(&ul) {
        return;
    }
    record_text_property_first_change(buf, &mut ul);
    for entry in entries {
        undo::undo_list_record_property_change(&mut ul, entry.name, entry.old_value, entry.range);
    }
    buf.set_undo_list(ul);
}

fn text_properties_set_undo_entries(
    buf: &Buffer,
    byte_range: EmacsByteRange,
    plist: &[(Value, Value)],
) -> Vec<TextPropertyUndoEntry> {
    let mut entries = Vec::new();
    for run in buffer_text_property_undo_runs(buf, byte_range) {
        // GNU's set_text_properties_1 calls split_interval_right/left without
        // copying properties into the changed split interval except for the
        // untouched remainder.  Therefore set_properties sees a nil old plist
        // for partial source intervals and records nil in undo.
        let old_plist: &[(Value, Value)] = if run.covers_full_source_range() {
            &run.plist
        } else {
            &[]
        };
        for (old_name, old_value) in old_plist {
            match plist_get_eq(plist, *old_name) {
                Some(new_value) if eq_value(&new_value, old_value) => {}
                _ => entries.push(TextPropertyUndoEntry::new(*old_name, *old_value, run.range)),
            }
        }
        for (new_name, _) in plist {
            if plist_get_eq(old_plist, *new_name).is_none() {
                entries.push(TextPropertyUndoEntry::new(*new_name, Value::NIL, run.range));
            }
        }
    }
    entries
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BufferSwapTextError {
    DeadBuffer,
    IndirectBuffer,
    HasIndirectBuffers,
}

impl BufferSwapTextError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::DeadBuffer => "Cannot swap a dead buffer's text",
            Self::IndirectBuffer => "Cannot swap indirect buffers's text",
            Self::HasIndirectBuffers => "One of the buffers to swap has indirect buffers",
        }
    }
}

impl BufferManager {
    /// Create a new `BufferManager` pre-populated with a `*scratch*` buffer.
    pub fn new() -> Self {
        // Phase 10D: seed `buffer_defaults` from `BUFFER_SLOT_INFO`,
        // mirroring GNU `init_buffer_once` which materializes
        // `buffer_defaults` from the per-slot `default` literals
        // (`buffer.c:4828-4889`). Slots that are not in
        // BUFFER_SLOT_INFO start as `Value::NIL`.
        let mut buffer_defaults = [crate::emacs_core::value::Value::NIL; BUFFER_SLOT_COUNT];
        for info in BUFFER_SLOT_INFO {
            buffer_defaults[info.offset.index()] = info.default.to_value();
        }
        let mut mgr = Self {
            buffers: FxHashMap::default(),
            dead_buffers: FxHashMap::default(),
            buffer_order: Vec::new(),
            current: None,
            next_id: 1,
            next_marker_id: 1,
            labeled_restrictions: FxHashMap::default(),
            default_text_backend_kind: ImplementedBufferTextBackendKind::GAP_BUFFER,
            buffer_defaults,
            saved_point_before_command: SavedPointBeforeCommand::new_editor_global(),
        };
        let scratch = mgr.create_buffer("*scratch*");
        if let Some(buf) = mgr.buffers.get_mut(&scratch) {
            buf.set_last_name_value(crate::emacs_core::value::Value::NIL);
        }
        mgr.current = Some(scratch);
        mgr.note_buffer_order_head(scratch);
        mgr
    }

    /// Allocate a new buffer with the given name and return its id.
    pub fn create_buffer(&mut self, name: &str) -> BufferId {
        self.create_buffer_with_hook_inhibition(name, false)
    }

    /// Allocate a new buffer with the given name and hook-inhibition state.
    pub fn create_buffer_with_hook_inhibition(
        &mut self,
        name: &str,
        inhibit_buffer_hooks: bool,
    ) -> BufferId {
        let id = BufferId(self.next_id);
        self.next_id += 1;
        let name_value = Value::string(name);
        let mut buf = Buffer::new_with_text_backend_kind(
            id,
            name_value,
            self.default_text_backend_kind,
            self.saved_point_before_command.clone(),
        );
        buf.set_last_name_value(name_value);
        // Phase 10D: seed every conditional slot from
        // `BufferManager::buffer_defaults` so a buffer created
        // *after* a `setq-default`/`set-default` observes the live
        // global default rather than the static `BufferSlotInfo`
        // seed. Always-local slots (-1) keep their per-buffer
        // initial value (the static seed already populated them).
        for info in BUFFER_SLOT_INFO {
            if info.local_flags_idx >= 0 {
                buf.slots[info.offset.index()] = self.buffer_defaults[info.offset.index()];
            }
        }
        buf.inhibit_buffer_hooks = inhibit_buffer_hooks;
        if let Some(default_directory) = self
            .current
            .and_then(|current| self.buffers.get(&current))
            .and_then(|current| current.buffer_local_value("default-directory"))
        {
            buf.set_buffer_local("default-directory", default_directory);
        }
        // GNU buffer.c:667 — buffers whose names start with a space have
        // undo recording disabled by default.
        if name.starts_with(' ') {
            buf.set_buffer_local(BUFFER_UNDO_LIST_NAME, crate::emacs_core::value::Value::T);
        }
        self.buffers.insert(id, buf);
        self.buffer_order.push(id);
        id
    }

    pub fn default_text_backend_kind(&self) -> BufferTextBackendKind {
        self.default_text_backend_kind.public_kind()
    }

    pub(crate) fn implemented_default_text_backend_kind(&self) -> ImplementedBufferTextBackendKind {
        self.default_text_backend_kind
    }

    pub(crate) fn set_default_text_backend_kind(&mut self, kind: ImplementedBufferTextBackendKind) {
        self.default_text_backend_kind = kind;
    }

    fn has_indirect_buffers(&self, id: BufferId) -> bool {
        self.buffers
            .values()
            .any(|buffer| buffer.base_buffer == Some(id))
    }

    pub(crate) fn swap_buffer_text(
        &mut self,
        current_id: BufferId,
        other_id: BufferId,
    ) -> Result<(), BufferSwapTextError> {
        let Some(current) = self.buffers.get(&current_id) else {
            return Err(BufferSwapTextError::DeadBuffer);
        };
        let Some(other) = self.buffers.get(&other_id) else {
            return Err(BufferSwapTextError::DeadBuffer);
        };

        if current.base_buffer.is_some() || other.base_buffer.is_some() {
            return Err(BufferSwapTextError::IndirectBuffer);
        }
        if self.has_indirect_buffers(current_id) || self.has_indirect_buffers(other_id) {
            return Err(BufferSwapTextError::HasIndirectBuffers);
        }

        if current_id == other_id {
            self.buffers
                .get_mut(&current_id)
                .ok_or(BufferSwapTextError::DeadBuffer)?
                .note_buffer_swap_text_self();
            let _ = self.record_buffer_state_markers(current_id);
            return Ok(());
        }

        let mut current = self
            .buffers
            .remove(&current_id)
            .ok_or(BufferSwapTextError::DeadBuffer)?;
        {
            let other = self
                .buffers
                .get_mut(&other_id)
                .ok_or(BufferSwapTextError::DeadBuffer)?;
            current.swap_owned_text_state_with(other);
            current
                .text
                .retarget_markers_for_buffer_swap(other_id, current_id);
            other
                .text
                .retarget_markers_for_buffer_swap(current_id, other_id);
            current.overlays.retarget_buffer(other_id, current_id);
            other.overlays.retarget_buffer(current_id, other_id);
            current.text.record_char_modification(1);
            other.text.record_char_modification(1);
            current.increment_overlay_modified_tick();
            other.increment_overlay_modified_tick();
        }
        self.buffers.insert(current_id, current);
        let _ = self.record_buffer_state_markers(current_id);
        let _ = self.record_buffer_state_markers(other_id);
        Ok(())
    }

    /// Point an indirect buffer's visited-file modtime slot at its base's, the
    /// `b->base_buffer` dereference in GNU `record_first_change`
    /// (`src/undo.c:213-214`), and clear the indirect buffer's own modtime the
    /// way `reset_buffer` does (`src/buffer.c:1092-1093`).
    ///
    /// Both buffers are looked up in `self.buffers` here on purpose: a `Buffer`
    /// clone carries a *copy* of the modtime cell (see
    /// `VisitedFileModtimeSlot`'s `Clone`), so linking through a clone would
    /// freeze the indirect buffer's view of its base.  The same trap already
    /// applies to `text` and `undo_state` in [`Self::from_dump`].
    fn link_indirect_visited_file_modtime(&mut self, indirect_id: BufferId, base_id: BufferId) {
        let Some(base_cell) = self
            .buffers
            .get(&base_id)
            .map(|base| base.modtime.share_own())
        else {
            return;
        };
        if let Some(indirect) = self.buffers.get_mut(&indirect_id) {
            indirect.modtime.reset_as_indirect_of(base_cell);
            // GNU `reset_buffer`: `b->modtime_size = -1` (`src/buffer.c:1093`).
            indirect.modtime_size = None;
        }
    }

    /// Allocate a new indirect buffer that shares its root base buffer's text.
    ///
    /// This mirrors GNU Emacs's `make-indirect-buffer` C boundary:
    /// indirect buffers share the root base buffer's text object, and double
    /// indirection is flattened so every indirect points at the same root.
    pub fn create_indirect_buffer(
        &mut self,
        base_id: BufferId,
        name: &str,
        clone: bool,
    ) -> Option<BufferId> {
        self.create_indirect_buffer_with_hook_inhibition(base_id, name, clone, false)
    }

    pub fn create_indirect_buffer_with_hook_inhibition(
        &mut self,
        base_id: BufferId,
        name: &str,
        clone: bool,
        inhibit_buffer_hooks: bool,
    ) -> Option<BufferId> {
        if name.is_empty() || self.find_buffer_by_name(name).is_some() {
            return None;
        }

        let root_id = self.shared_text_root_id(base_id)?;
        let root = self.buffers.get(&root_id)?.clone();
        let shared_text = self.buffers.get(&root_id)?.text.shared_clone();

        let id = BufferId(self.next_id);
        self.next_id += 1;

        let root_mark = if clone {
            root.mark_emacs_byte_pos()
        } else {
            None
        };

        let mut indirect = if clone {
            let mut cloned = root.clone();
            cloned.id = id;
            cloned.overlays.retarget_buffer(root_id, id);
            cloned.set_name_value(Value::string(name));
            cloned
                .local_var_alist
                .replace_alist(clone_lisp_local_variables(
                    root.local_var_alist.as_lisp_alist(),
                ));
            // GNU `clone_per_buffer_values` copies marker-valued per-buffer
            // slots by building fresh markers owned by the new buffer.  Raw
            // marker pointers copied from the base would make the indirect
            // buffer share point/narrowing/mark state with its base.
            cloned.mark_marker_id = None;
            cloned.mark_marker_ptr = std::ptr::null_mut();
            cloned.state_markers = None;
            cloned
        } else {
            let name_value = Value::string(name);
            let mut fresh = Buffer::new(id, name_value, self.saved_point_before_command.clone());
            fresh.set_last_name_value(name_value);
            if let Some(default_directory) = self
                .current
                .and_then(|current| self.buffers.get(&current))
                .and_then(|current| current.buffer_local_value("default-directory"))
            {
                fresh.set_buffer_local("default-directory", default_directory);
            }
            fresh
        };

        indirect.base_buffer = Some(root_id);
        indirect.inhibit_buffer_hooks = inhibit_buffer_hooks;
        indirect.text = shared_text;
        indirect.undo_state = root.undo_state.clone();
        indirect.narrow_to_emacs_byte_range(root.accessible_emacs_byte_range());
        indirect.goto_emacs_byte_pos(root.point_emacs_byte_pos());
        indirect.set_multibyte_value(root.get_multibyte());
        indirect.autosave_modified_tick = root.autosave_modified_tick;
        indirect.slots[BUFFER_SLOT_FILE_NAME.index()] = Value::NIL;
        if !clone {
            indirect.overlays = OverlayList::new();
            indirect.mark_marker_id = None;
            indirect.mark_marker_ptr = std::ptr::null_mut();
            /* mark_byte removed */
        }

        self.buffers.insert(id, indirect);
        self.buffer_order.push(id);
        // GNU `Fmake_indirect_buffer` runs `reset_buffer` on the new buffer,
        // which clears its own modtime (`src/buffer.c:1092-1093`) whatever
        // CLONE says -- an indirect buffer visits no file -- and sets
        // `b->base_buffer`, which is the link `record_first_change` follows.
        // Both halves are one call, and it reads the base slot out of the map
        // rather than from the `root` clone above, whose cell is a copy.
        self.link_indirect_visited_file_modtime(id, root_id);
        let _ = self.ensure_buffer_state_markers(root_id);
        let _ = self.ensure_buffer_state_markers(id);
        if let Some(mark) = root_mark {
            self.buffers.get_mut(&id)?.set_mark_emacs_byte_pos(mark);
        }
        Some(id)
    }

    fn note_buffer_order_head(&mut self, id: BufferId) {
        self.buffer_order.retain(|existing| *existing != id);
        self.buffer_order.insert(0, id);
    }

    /// Move a live buffer to the end of buffer-list order.
    ///
    /// GNU's `bury-buffer-internal` makes the buffer least preferred for
    /// `other-buffer`/`last-buffer` by placing it at the tail of the buffer
    /// list while leaving the current-buffer selection to Lisp callers.
    pub fn note_buffer_order_tail(&mut self, id: BufferId) -> bool {
        if !self.buffers.contains_key(&id) {
            return false;
        }
        self.buffer_order.retain(|existing| *existing != id);
        self.buffer_order.push(id);
        true
    }

    /// Move a live buffer immediately after another live buffer in global
    /// buffer-list order without recording display or selection.
    pub(crate) fn note_buffer_order_after(&mut self, id: BufferId, after: BufferId) -> bool {
        if id == after || !self.buffers.contains_key(&id) || !self.buffers.contains_key(&after) {
            return false;
        }
        self.buffer_order.retain(|existing| *existing != id);
        let insert_at = self
            .buffer_order
            .iter()
            .position(|existing| *existing == after)
            .map_or(self.buffer_order.len(), |index| index + 1);
        self.buffer_order.insert(insert_at, id);
        true
    }

    pub fn note_buffer_display(&mut self, id: BufferId) {
        if self.buffers.contains_key(&id) {
            self.note_buffer_order_head(id);
        }
    }

    /// Immutable access to a buffer by id.
    pub fn get(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id)
    }

    /// Immutable access to a killed buffer by id.
    pub fn get_dead(&self, id: BufferId) -> Option<&Buffer> {
        self.dead_buffers.get(&id)
    }

    /// Immutable access to a buffer object, live or killed.
    pub fn get_any(&self, id: BufferId) -> Option<&Buffer> {
        self.buffers.get(&id).or_else(|| self.dead_buffers.get(&id))
    }

    /// Mutable access to a buffer by id.
    pub fn get_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    /// Propagate a newly-installed per-buffer default into every live buffer
    /// that is still using the old default for `slot`. GNU buffers read
    /// `buffer_defaults` live, so a late `setq-default` is visible everywhere;
    /// neomacs copies the default into each buffer's slot at creation, so a
    /// default installed *after* some buffers exist must be pushed into those
    /// copies. Only buffers without an explicit local override (the slot-local
    /// flag is clear, or — for unconditional slots — the slot is still nil) are
    /// updated; a buffer that set its own value keeps it.
    pub(crate) fn seed_default_slot_into_unset_buffers(&mut self, slot: BufferSlot, value: Value) {
        let conditional =
            buffer_slot_info_by_offset(slot).is_some_and(|info| info.local_flags_idx >= 0);
        for buffer in self.buffers.values_mut() {
            let unset = if conditional {
                !buffer.slot_local_flag(slot)
            } else {
                buffer.slots[slot.index()].is_nil()
            };
            if unset {
                buffer.slots[slot.index()] = value;
            }
        }
    }

    /// Collect a raw `*mut *mut MarkerObj` pointer to every live buffer's
    /// marker-chain head slot. Used by the GC to feed
    /// `TaggedHeap::unchain_dead_markers` between mark and sweep so dead
    /// markers are spliced out of the intrusive chain before the sweep
    /// frees them. Mirrors GNU `sweep_buffer → unchain_dead_markers` (`alloc.c`).
    ///
    /// # Safety
    ///
    /// Callers must ensure no concurrent borrows of the returned storage
    /// exist. Stop-the-world GC is the only caller.
    pub unsafe fn collect_marker_chain_head_slots(
        &self,
    ) -> Vec<*mut *mut crate::tagged::header::MarkerObj> {
        self.buffers
            .values()
            .map(|buf| unsafe { buf.text.markers_head_slot_raw() })
            .collect()
    }

    /// Immutable access to the current buffer.
    pub fn current_buffer(&self) -> Option<&Buffer> {
        self.current.and_then(|id| self.buffers.get(&id))
    }

    /// Mutable access to the current buffer.
    pub fn current_buffer_mut(&mut self) -> Option<&mut Buffer> {
        self.current.and_then(|id| self.buffers.get_mut(&id))
    }

    pub(in crate::buffer) fn buffer_mut(&mut self, id: BufferId) -> Option<&mut Buffer> {
        self.buffers.get_mut(&id)
    }

    /// Return the current buffer id.
    pub fn current_buffer_id(&self) -> Option<BufferId> {
        self.current
    }

    pub fn buffer_hooks_inhibited(&self, id: BufferId) -> bool {
        self.buffers
            .get(&id)
            .is_some_and(|buffer| buffer.inhibit_buffer_hooks)
    }

    pub(in crate::buffer) fn buffer_has_state_markers(&self, id: BufferId) -> bool {
        self.buffers
            .get(&id)
            .and_then(|buffer| buffer.state_markers)
            .is_some()
    }

    fn ensure_buffer_state_markers(&mut self, buffer_id: BufferId) -> Option<()> {
        if self.buffer_has_state_markers(buffer_id) {
            return Some(());
        }
        let (pt, begv, zv) = {
            let buffer = self.buffers.get(&buffer_id)?;
            (
                buffer.point_emacs_byte_pos(),
                buffer.point_min_emacs_byte_pos(),
                buffer.point_max_emacs_byte_pos(),
            )
        };
        let (pt_marker, pt_marker_ptr) =
            self.create_marker_at_emacs_byte_pos(buffer_id, pt, InsertionType::Before);
        let (begv_marker, begv_marker_ptr) =
            self.create_marker_at_emacs_byte_pos(buffer_id, begv, InsertionType::Before);
        let (zv_marker, zv_marker_ptr) =
            self.create_marker_at_emacs_byte_pos(buffer_id, zv, InsertionType::After);
        self.buffers.get_mut(&buffer_id)?.state_markers = Some(BufferStateMarkers {
            pt_marker,
            begv_marker,
            zv_marker,
            pt_marker_ptr,
            begv_marker_ptr,
            zv_marker_ptr,
        });
        Some(())
    }

    fn record_buffer_state_markers(&mut self, buffer_id: BufferId) -> Option<()> {
        let markers = self.buffers.get(&buffer_id)?.state_markers?;
        let (pt, begv, zv) = {
            let buffer = self.buffers.get(&buffer_id)?;
            (
                buffer.point_emacs_byte_pos(),
                buffer.point_min_emacs_byte_pos(),
                buffer.point_max_emacs_byte_pos(),
            )
        };
        // State markers live on this buffer's chain already; unlink before
        // re-registering so chain_splice_at_head's precondition holds.
        if let Some(buf) = self.buffers.get(&buffer_id) {
            buf.text.chain_unlink(markers.pt_marker_ptr);
            buf.text.chain_unlink(markers.begv_marker_ptr);
            buf.text.chain_unlink(markers.zv_marker_ptr);
        }
        self.register_marker_id_at_emacs_byte_pos(
            markers.pt_marker_ptr,
            buffer_id,
            markers.pt_marker,
            pt,
            InsertionType::Before,
        )?;
        self.register_marker_id_at_emacs_byte_pos(
            markers.begv_marker_ptr,
            buffer_id,
            markers.begv_marker,
            begv,
            InsertionType::Before,
        )?;
        self.register_marker_id_at_emacs_byte_pos(
            markers.zv_marker_ptr,
            buffer_id,
            markers.zv_marker,
            zv,
            InsertionType::After,
        )?;
        Some(())
    }

    pub(in crate::buffer) fn fetch_buffer_state_markers(
        &mut self,
        buffer_id: BufferId,
    ) -> Option<()> {
        let markers = self.buffers.get(&buffer_id)?.state_markers?;
        let pt = self.marker_emacs_byte_pos(buffer_id, markers.pt_marker)?;
        let begv = self.marker_emacs_byte_pos(buffer_id, markers.begv_marker)?;
        let zv = self.marker_emacs_byte_pos(buffer_id, markers.zv_marker)?;
        let buffer = self.buffers.get_mut(&buffer_id)?;
        buffer.set_accessible_region_and_point_from_emacs_bytes(EmacsByteRange::new(begv, zv), pt);
        Some(())
    }

    fn switch_current_with_recording(&mut self, id: BufferId, record_order: bool) -> bool {
        if !self.buffers.contains_key(&id) {
            return false;
        }
        if self.current == Some(id) {
            if record_order {
                self.note_buffer_order_head(id);
            }
            return true;
        }

        let old_id = self.current;
        self.current = Some(id);
        if record_order {
            self.note_buffer_order_head(id);
        }

        if let Some(old_id) = old_id {
            let _ = self.record_buffer_state_markers(old_id);
        }
        let _ = self.fetch_buffer_state_markers(id);
        true
    }

    /// Switch the current buffer and record it as recently selected.
    ///
    /// This mirrors GNU Emacs's visible-selection path, where `record_buffer`
    /// moves a buffer to the head of the buffer lists once the selection is
    /// meant to count for `other-buffer`.
    pub fn switch_current(&mut self, id: BufferId) -> bool {
        self.switch_current_with_recording(id, true)
    }

    /// Switch the current buffer without changing buffer-list recency.
    ///
    /// GNU Emacs uses `set_buffer_internal` for temporary internal work
    /// without calling `record_buffer`; callers such as `message_dolog`
    /// rely on that to avoid making `*Messages*` the preferred
    /// `other-buffer`.
    pub fn switch_current_unrecorded(&mut self, id: BufferId) -> bool {
        self.switch_current_with_recording(id, false)
    }

    /// Backwards-compatible alias while call sites migrate to `switch_current`.
    pub fn set_current(&mut self, id: BufferId) {
        let _ = self.switch_current(id);
    }

    /// Find a buffer by name, returning its id if it exists.
    pub fn find_buffer_by_name(&self, name: &str) -> Option<BufferId> {
        self.buffers
            .values()
            .find(|b| b.has_name(name))
            .map(|b| b.id)
    }

    /// Find a killed buffer by its last known name.
    pub fn find_dead_buffer_by_name(&self, name: &str) -> Option<BufferId> {
        self.dead_buffers.iter().find_map(|(id, buffer)| {
            (buffer
                .last_name_value()
                .as_lisp_string()
                .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
                .as_deref()
                == Some(name))
            .then_some(*id)
        })
    }

    /// Remove a buffer.  Returns `true` if the buffer existed.
    ///
    /// If the killed buffer was current, `current` is set to `None`.
    pub fn kill_buffer(&mut self, id: BufferId) -> bool {
        self.kill_buffer_collect(id).is_some()
    }

    pub fn kill_buffer_collect(&mut self, id: BufferId) -> Option<Vec<BufferId>> {
        let killed_ids = self.collect_killed_buffer_ids(id)?;
        let killed_set: HashSet<BufferId> = killed_ids.iter().copied().collect();

        for killed_id in &killed_ids {
            self.replace_labeled_restrictions(*killed_id, None);
        }

        // Detach every marker that belongs to one of the dying buffers from
        // the (possibly shared, in the indirect-buffer case) chain. A single
        // chain walk filters by `LispMarker.buffer ∈ killed_set`, which is
        // correct for both kill-root (killed_set = {root, indirects}) and
        // kill-indirect (killed_set = {indirect}, root's markers untouched).
        // Mirrors GNU `kill_buffer` calling `unchain_marker` on each entry of
        // the dying buffer's marker list.
        self.buffers
            .get(&id)?
            .text
            .remove_markers_for_buffers(&killed_set);

        for killed_id in &killed_ids {
            let mut buf = self.buffers.remove(killed_id)?;
            buf.mark_killed_after_local_reset();
            self.dead_buffers.insert(*killed_id, buf);
        }
        self.buffer_order
            .retain(|buffer_id| !killed_set.contains(buffer_id));

        if self
            .current
            .is_some_and(|current| killed_set.contains(&current))
        {
            self.current = None;
        }

        Some(killed_ids)
    }

    /// Return the last known name for a dead buffer id, if available.
    pub fn dead_buffer_last_name_value(&self, id: BufferId) -> Option<Value> {
        self.dead_buffers.get(&id).map(Buffer::last_name_value)
    }

    /// List all live buffer ids in buffer-list order, with the most recently
    /// displayed or selected buffers first.
    pub fn buffer_list(&self) -> Vec<BufferId> {
        let mut ids = Vec::with_capacity(self.buffers.len());
        for id in &self.buffer_order {
            if self.buffers.contains_key(id) {
                ids.push(*id);
            }
        }
        if ids.len() < self.buffers.len() {
            let mut missing: Vec<BufferId> = self
                .buffers
                .keys()
                .copied()
                .filter(|id| !ids.contains(id))
                .collect();
            missing.sort_by_key(|id| id.0);
            ids.extend(missing);
        }
        ids
    }

    pub(in crate::buffer) fn shared_text_root_id(&self, id: BufferId) -> Option<BufferId> {
        let buf = self.buffers.get(&id)?;
        Some(buf.base_buffer.unwrap_or(buf.id))
    }

    pub(crate) fn collect_killed_buffer_ids(&self, id: BufferId) -> Option<Vec<BufferId>> {
        let buf = self.buffers.get(&id)?;
        let mut killed_ids = vec![id];
        if buf.base_buffer.is_none() {
            let mut indirects = self
                .buffers
                .values()
                .filter_map(|buffer| (buffer.base_buffer == Some(id)).then_some(buffer.id))
                .collect::<Vec<_>>();
            indirects.sort_by_key(|buffer_id| buffer_id.0);
            killed_ids.extend(indirects);
        }
        Some(killed_ids)
    }

    pub fn full_buffer_emacs_byte_range(&self, id: BufferId) -> Option<EmacsByteRange> {
        let buf = self.buffers.get(&id)?;
        Some(buf.full_emacs_byte_range())
    }

    fn labeled_restriction_at(&self, id: BufferId, outermost: bool) -> Option<&LabeledRestriction> {
        let restrictions = self.labeled_restrictions.get(&id)?;
        if outermost {
            restrictions.first()
        } else {
            restrictions.last()
        }
    }

    fn labeled_restriction_emacs_byte_range(
        &self,
        id: BufferId,
        outermost: bool,
    ) -> Option<EmacsByteRange> {
        let restriction = self.labeled_restriction_at(id, outermost)?;
        let beg = self.marker_emacs_byte_pos(id, restriction.beg_marker)?;
        let end = self.marker_emacs_byte_pos(id, restriction.end_marker)?;
        Some(EmacsByteRange::new(beg, end))
    }

    pub fn current_labeled_restriction_bounds(&self, id: BufferId) -> Option<EmacsByteRange> {
        self.labeled_restriction_emacs_byte_range(id, false)
    }

    pub fn current_labeled_restriction_char_bounds(&self, id: BufferId) -> Option<CharRange> {
        let restriction = self.labeled_restriction_at(id, false)?;
        let beg = self.marker_char_pos(id, restriction.beg_marker)?;
        let end = self.marker_char_pos(id, restriction.end_marker)?;
        Some(CharRange::new(beg, end))
    }

    pub fn current_labeled_restriction_matches_label(&self, id: BufferId, label: &Value) -> bool {
        let Some(restriction) = self.labeled_restriction_at(id, false) else {
            return false;
        };
        match restriction.label {
            LabeledRestrictionLabel::User(current) => {
                crate::emacs_core::value::eq_value(&current, label)
            }
            LabeledRestrictionLabel::Outermost => false,
        }
    }

    fn clone_marker_in_buffer(&mut self, buffer_id: BufferId, marker_id: u64) -> Option<u64> {
        let (position, insertion_type) = {
            let buf = self.buffers.get(&buffer_id)?;
            buf.marker_chain_anchor_lookup(marker_id)?
        };
        let (marker_id, _marker_ptr) =
            self.create_marker_at_anchor(buffer_id, position, insertion_type);
        Some(marker_id)
    }

    fn clone_labeled_restrictions(
        &mut self,
        buffer_id: BufferId,
    ) -> Option<Option<Vec<LabeledRestriction>>> {
        let restrictions = self.labeled_restrictions.get(&buffer_id)?.clone();
        let mut cloned = Vec::with_capacity(restrictions.len());
        for restriction in restrictions {
            let beg_marker = self.clone_marker_in_buffer(buffer_id, restriction.beg_marker)?;
            let end_marker = self.clone_marker_in_buffer(buffer_id, restriction.end_marker)?;
            cloned.push(LabeledRestriction {
                label: restriction.label,
                beg_marker,
                end_marker,
            });
        }
        Some(Some(cloned))
    }

    fn replace_labeled_restrictions(
        &mut self,
        buffer_id: BufferId,
        restrictions: Option<Vec<LabeledRestriction>>,
    ) {
        let mut live_marker_ids = std::collections::HashSet::new();
        if let Some(ref restrictions) = restrictions {
            for restriction in restrictions {
                live_marker_ids.insert(restriction.beg_marker);
                live_marker_ids.insert(restriction.end_marker);
            }
        }

        if let Some(old) = self.labeled_restrictions.remove(&buffer_id) {
            for restriction in old {
                if !live_marker_ids.contains(&restriction.beg_marker) {
                    self.remove_marker(restriction.beg_marker);
                }
                if !live_marker_ids.contains(&restriction.end_marker) {
                    self.remove_marker(restriction.end_marker);
                }
            }
        }

        if self.buffers.contains_key(&buffer_id)
            && let Some(restrictions) = restrictions.filter(|restrictions| !restrictions.is_empty())
        {
            self.labeled_restrictions.insert(buffer_id, restrictions);
        }
    }

    pub fn clear_buffer_labeled_restrictions(&mut self, buffer_id: BufferId) -> Option<()> {
        self.buffers.get(&buffer_id)?;
        self.replace_labeled_restrictions(buffer_id, None);
        Some(())
    }

    fn push_labeled_restriction_for_current_bounds(
        &mut self,
        buffer_id: BufferId,
        label: LabeledRestrictionLabel,
    ) -> Option<()> {
        let (begv, zv) = {
            let buf = self.buffers.get(&buffer_id)?;
            (
                buf.point_min_emacs_byte_pos(),
                buf.point_max_emacs_byte_pos(),
            )
        };
        let (beg_marker, _) =
            self.create_marker_at_emacs_byte_pos(buffer_id, begv, InsertionType::Before);
        let (end_marker, _) =
            self.create_marker_at_emacs_byte_pos(buffer_id, zv, InsertionType::After);
        self.labeled_restrictions
            .entry(buffer_id)
            .or_default()
            .push(LabeledRestriction {
                label,
                beg_marker,
                end_marker,
            });
        Some(())
    }

    fn pop_labeled_restriction(&mut self, buffer_id: BufferId) -> Option<LabeledRestriction> {
        let restrictions = self.labeled_restrictions.get_mut(&buffer_id)?;
        let restriction = restrictions.pop()?;
        let remove_entry = restrictions.is_empty();
        if remove_entry {
            self.labeled_restrictions.remove(&buffer_id);
        }
        self.remove_marker(restriction.beg_marker);
        self.remove_marker(restriction.end_marker);
        Some(restriction)
    }

    fn widen_buffer_fully(&mut self, id: BufferId) -> Option<()> {
        let range = self.full_buffer_emacs_byte_range(id)?;
        self.restore_buffer_emacs_byte_restriction(id, range)
    }

    pub(in crate::buffer) fn buffers_sharing_root_ids(&self, root_id: BufferId) -> Vec<BufferId> {
        self.buffers
            .values()
            .filter_map(|buf| (buf.base_buffer.unwrap_or(buf.id) == root_id).then_some(buf.id))
            .collect()
    }

    pub(crate) fn shared_text_buffer_ids(&self, root_id: BufferId) -> Vec<BufferId> {
        self.buffers_sharing_root_ids(root_id)
    }

    pub(crate) fn modified_state_root_id(&self, id: BufferId) -> Option<BufferId> {
        self.shared_text_root_id(id)
    }

    pub fn goto_buffer_emacs_byte_pos(
        &mut self,
        id: BufferId,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        {
            let buf = self.buffers.get_mut(&id)?;
            buf.goto_emacs_byte_pos(pos);
        }
        let point = self.buffers.get(&id)?.point_emacs_byte_pos();
        let _ = self.record_buffer_state_markers(id);
        Some(point)
    }

    pub fn set_buffer_point_anchor(
        &mut self,
        id: BufferId,
        point: TextPositionAnchor,
    ) -> Option<TextPositionAnchor> {
        {
            let buf = self.buffers.get_mut(&id)?;
            buf.set_point_anchor(point);
        }
        let point = self.buffers.get(&id)?.point_anchor();
        let _ = self.record_buffer_state_markers(id);
        Some(point)
    }

    pub fn delete_all_buffer_overlays(&mut self, id: BufferId) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        if !buf.overlays.is_empty() {
            buf.overlays.delete_all_overlays();
            buf.increment_overlay_modified_tick();
        }
        Some(())
    }

    pub fn delete_buffer_overlay(&mut self, id: BufferId, overlay_id: Value) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        if buf.overlays.delete_overlay(overlay_id) {
            buf.increment_overlay_modified_tick();
        }
        Some(())
    }

    pub fn put_buffer_overlay_property(
        &mut self,
        id: BufferId,
        overlay_id: Value,
        name: Value,
        value: Value,
    ) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        buf.overlays.overlay_put(overlay_id, name, value).ok()?;
        // An overlay property change (face/display/before-string/invisible/...) can
        // alter layout with no buffer-text edit; bump the overlay tick so the
        // incremental fast paths re-lay instead of reusing stale rows (GNU bumps
        // the overlay modiff on any overlay change).
        buf.increment_overlay_modified_tick();
        Some(())
    }

    pub fn narrow_buffer_to_emacs_byte_range(
        &mut self,
        id: BufferId,
        range: EmacsByteRange,
    ) -> Option<()> {
        self.buffers.get_mut(&id)?.narrow_to_emacs_byte_range(range);
        let _ = self.record_buffer_state_markers(id);
        Some(())
    }

    pub fn widen_buffer(&mut self, id: BufferId) -> Option<()> {
        self.buffers.get(&id)?;
        let Some(restriction) = self.labeled_restriction_at(id, false).copied() else {
            return self.widen_buffer_fully(id);
        };
        let Some(range) = self.labeled_restriction_emacs_byte_range(id, false) else {
            self.replace_labeled_restrictions(id, None);
            return self.widen_buffer_fully(id);
        };
        self.restore_buffer_emacs_byte_restriction(id, range)?;
        if matches!(restriction.label, LabeledRestrictionLabel::Outermost) {
            let _ = self.pop_labeled_restriction(id);
        }
        Some(())
    }

    pub fn replace_buffer_contents(&mut self, id: BufferId, text: &str) -> Option<()> {
        let delete_range = {
            let buf = self.buffers.get(&id)?;
            buf.full_emacs_byte_range()
        };
        if !delete_range.is_empty() {
            self.delete_buffer_emacs_byte_range(id, delete_range)?;
        }
        {
            let buf = self.buffers.get_mut(&id)?;
            buf.widen();
            buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
        }
        if !text.is_empty() {
            self.insert_into_buffer(id, text)?;
            self.goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(0))?;
        }
        Some(())
    }

    pub fn replace_buffer_contents_lisp_string(
        &mut self,
        id: BufferId,
        text: &crate::heap_types::LispString,
    ) -> Option<()> {
        debug_assert_eq!(
            self.buffers.get(&id)?.get_multibyte(),
            text.is_multibyte(),
            "replace_buffer_contents_lisp_string expects text already converted to target buffer representation",
        );
        let delete_range = {
            let buf = self.buffers.get(&id)?;
            buf.full_emacs_byte_range()
        };
        if !delete_range.is_empty() {
            self.delete_buffer_emacs_byte_range(id, delete_range)?;
        }
        {
            let buf = self.buffers.get_mut(&id)?;
            buf.widen();
            buf.goto_emacs_byte_pos(EmacsBytePos::new(0));
        }
        if !text.is_empty() {
            self.insert_lisp_string_into_buffer(id, text)?;
            self.goto_buffer_emacs_byte_pos(id, EmacsBytePos::new(0))?;
        }
        Some(())
    }

    pub fn clear_buffer_local_properties(
        &mut self,
        id: BufferId,
        obarray: &mut crate::emacs_core::symbol::Obarray,
        kill_permanent: bool,
    ) -> Option<()> {
        // Snapshot the runtime buffer_defaults before we take a
        // mutable borrow of the individual buffer. Mirrors GNU's
        // `reset_buffer_local_variables` at `buffer.c:1242`, which
        // reads `per_buffer_default(offset)` (the runtime default)
        // rather than the C-level static initializer.
        let defaults_snapshot = self.buffer_defaults;
        let buf = self.buffers.get_mut(&id)?;
        buf.kill_all_local_variables(obarray, kill_permanent, &defaults_snapshot);
        Some(())
    }

    pub fn put_buffer_text_property_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        name: Value,
        value: Value,
    ) -> Option<bool> {
        let buf = self.buffers.get_mut(&id)?;
        let entries = buffer_text_property_undo_runs(buf, byte_range)
            .into_iter()
            .filter_map(|run| {
                let old_value = plist_get_eq(&run.plist, name);
                if old_value.is_some_and(|old_value| eq_value(&old_value, &value)) {
                    None
                } else {
                    Some(TextPropertyUndoEntry::new(
                        name,
                        old_value.unwrap_or(Value::NIL),
                        run.range,
                    ))
                }
            })
            .collect();
        record_buffer_text_property_undo_entries(buf, entries);
        Some(buf.text_props_put_property_in_emacs_byte_range(byte_range, name, value))
    }

    pub fn append_buffer_text_properties(
        &mut self,
        id: BufferId,
        table: &TextPropertyTable,
        byte_pos: EmacsBytePos,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .text
            .text_props_append_shifted_at_emacs_byte_pos(table, byte_pos);
        Some(())
    }

    pub fn merge_missing_buffer_text_properties(
        &mut self,
        id: BufferId,
        table: &TextPropertyTable,
        byte_pos: EmacsBytePos,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .text
            .text_props_merge_missing_shifted_at_emacs_byte_pos(table, byte_pos);
        Some(())
    }

    pub fn merge_adjacent_equal_buffer_text_properties(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .text
            .text_props_merge_adjacent_equal_around_emacs_byte_range(byte_range);
        Some(())
    }

    /// Batched sibling of
    /// [`Self::remove_buffer_text_property_in_emacs_byte_range`]: one
    /// undo-run walk and one interval walk cover every name. Undo entries
    /// for distinct names commute, so per-run interleaving is equivalent
    /// to the per-name sequential order.
    pub fn remove_buffer_text_properties_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        names: &[Value],
    ) -> Option<bool> {
        let buf = self.buffers.get_mut(&id)?;
        let mut entries = Vec::new();
        for run in buffer_text_property_undo_runs(buf, byte_range) {
            for &name in names {
                if let Some(old_value) = plist_get_eq(&run.plist, name) {
                    entries.push(TextPropertyUndoEntry::new(name, old_value, run.range));
                }
            }
        }
        record_buffer_text_property_undo_entries(buf, entries);
        Some(buf.text_props_remove_properties_in_emacs_byte_range(byte_range, names))
    }

    pub fn remove_buffer_text_property_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        name: Value,
    ) -> Option<bool> {
        let buf = self.buffers.get_mut(&id)?;
        let entries = buffer_text_property_undo_runs(buf, byte_range)
            .into_iter()
            .filter_map(|run| {
                plist_get_eq(&run.plist, name)
                    .map(|old_value| TextPropertyUndoEntry::new(name, old_value, run.range))
            })
            .collect();
        record_buffer_text_property_undo_entries(buf, entries);
        Some(buf.text_props_remove_property_in_emacs_byte_range(byte_range, name))
    }

    pub fn clear_buffer_text_properties_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .set_text_properties_with_undo_range(byte_range, Vec::new());
        Some(())
    }

    /// Char-range variant: the caller already knows the inserted char range, so
    /// this skips the byte->char conversion done by the byte-range version.
    pub(crate) fn clear_inserted_plain_text_properties_in_char_range(
        &mut self,
        id: BufferId,
        char_range: CharRange,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .text
            .text_props_set_properties_in_char_range(char_range, Vec::new());
        Some(())
    }

    pub fn set_buffer_text_properties_in_emacs_byte_range(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
        plist: Vec<(Value, Value)>,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .set_text_properties_with_undo_range(byte_range, plist);
        Some(())
    }

    pub fn record_buffer_text_property_modification(
        &mut self,
        id: BufferId,
        byte_range: EmacsByteRange,
    ) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        // Bumps `modified_tick` (as before) AND the dedicated
        // `props_modified_tick` — the single choke point every text-property
        // builtin (put/add/set/remove) routes through, so redisplay sees an
        // appearance change without a char edit. The changed range also feeds
        // the unchanged-region accumulator (GNU BUF_COMPUTE_UNCHANGED parity)
        // so the change invalidates rows, not the whole window.
        buf.record_text_property_modification();
        buf.note_changed_property_region(byte_range);
        Some(())
    }

    pub fn set_buffer_multibyte_flag(&mut self, id: BufferId, flag: bool) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        buf.set_multibyte_value(flag);
        buf.set_buffer_local(
            "enable-multibyte-characters",
            if flag {
                crate::emacs_core::value::Value::T
            } else {
                crate::emacs_core::value::Value::NIL
            },
        );
        Some(())
    }

    pub fn set_buffer_modified_flag(&mut self, id: BufferId, flag: bool) -> Option<()> {
        let root_id = self.modified_state_root_id(id)?;
        self.buffers.get_mut(&root_id)?.set_modified(flag);
        Some(())
    }

    pub fn restore_buffer_modified_state(&mut self, id: BufferId, flag: Value) -> Option<Value> {
        let root_id = self.modified_state_root_id(id)?;
        let out = self.buffers.get_mut(&root_id)?.restore_modified_state(flag);
        Some(out)
    }

    pub fn set_buffer_auto_saved(&mut self, id: BufferId) -> Option<()> {
        self.buffers.get_mut(&id)?.mark_auto_saved();
        Some(())
    }

    pub fn set_buffer_modified_tick(&mut self, id: BufferId, tick: i64) -> Option<()> {
        let root_id = self.modified_state_root_id(id)?;
        let buf = self.buffers.get_mut(&root_id)?;
        buf.text.set_modified_tick(tick);
        Some(())
    }

    pub fn set_buffer_file_name(&mut self, id: BufferId, file_name: Value) -> Option<()> {
        // GNU keeps `buffer-file-name` and `buffer-file-truename` as
        // distinct BUFFER_OBJFWD slots.  `insert-file-contents` and
        // `write-region` with VISIT set only the former; file-visiting
        // code such as `find-file-noselect-1` sets the truename slot
        // separately.
        debug_assert!(file_name.is_nil() || file_name.is_string());
        let buf = self.buffers.get_mut(&id)?;
        buf.set_file_name_value(file_name);
        Some(())
    }

    pub fn set_buffer_file_truename(&mut self, id: BufferId, file_truename: Value) -> Option<()> {
        debug_assert!(file_truename.is_nil() || file_truename.is_string());
        let buf = self.buffers.get_mut(&id)?;
        buf.slots[BUFFER_SLOT_FILE_TRUENAME.index()] = file_truename;
        Some(())
    }

    pub fn set_buffer_name(&mut self, id: BufferId, name: Value) -> Option<()> {
        self.buffers.get_mut(&id)?.set_name_value(name);
        Some(())
    }

    pub fn rename_buffer(&mut self, id: BufferId, name: Value) -> Option<()> {
        debug_assert!(name.is_string());
        let buf = self.buffers.get_mut(&id)?;
        let old_name = buf.name_value();
        buf.set_last_name_value(old_name);
        buf.set_name_value(name);
        Some(())
    }

    pub fn set_buffer_mark_emacs_byte_pos(
        &mut self,
        id: BufferId,
        pos: EmacsBytePos,
    ) -> Option<()> {
        self.buffers.get_mut(&id)?.set_mark_emacs_byte_pos(pos);
        Some(())
    }

    pub fn set_buffer_last_window_start(
        &mut self,
        id: BufferId,
        lisp_char_pos: LispCharPos1,
    ) -> Option<()> {
        self.buffers.get_mut(&id)?.last_window_start = lisp_char_pos.max(LispCharPos1::ONE);
        Some(())
    }

    pub fn move_buffer_overlay_to_emacs_byte_range(
        &mut self,
        id: BufferId,
        overlay_id: Value,
        range: EmacsByteRange,
    ) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        buf.overlays
            .move_overlay_to_emacs_byte_range(overlay_id, range);
        // Moving an overlay (e.g. hl-line following the cursor) changes which
        // lines it covers with no buffer-text edit; bump the overlay tick so the
        // cursor-only / scroll fast paths re-lay the affected rows.
        buf.increment_overlay_modified_tick();
        Some(())
    }

    pub fn clear_buffer_mark(&mut self, id: BufferId) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        buf.mark_marker_id = None;
        buf.mark_marker_ptr = std::ptr::null_mut();
        /* mark_byte removed */
        Some(())
    }

    pub fn set_buffer_local_property(
        &mut self,
        id: BufferId,
        name: &str,
        value: Value,
    ) -> Option<()> {
        self.buffers.get_mut(&id)?.set_buffer_local(name, value);
        Some(())
    }

    pub fn set_buffer_local_property_by_sym_id(
        &mut self,
        id: BufferId,
        sym_id: SymId,
        value: Value,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .set_buffer_local_by_sym_id(sym_id, value);
        Some(())
    }

    pub fn buffer_local_map(&self, id: BufferId) -> Option<Value> {
        Some(self.buffers.get(&id)?.local_map())
    }

    pub fn current_local_map(&self) -> Value {
        self.current
            .and_then(|id| self.buffer_local_map(id))
            .unwrap_or(Value::NIL)
    }

    pub fn set_buffer_local_map(&mut self, id: BufferId, keymap: Value) -> Option<()> {
        self.buffers.get_mut(&id)?.set_local_map(keymap);
        Some(())
    }

    pub fn set_current_local_map(&mut self, keymap: Value) -> Option<()> {
        let id = self.current?;
        self.set_buffer_local_map(id, keymap)
    }

    pub fn set_buffer_local_void_property(&mut self, id: BufferId, name: &str) -> Option<()> {
        self.buffers.get_mut(&id)?.set_buffer_local_void(name);
        Some(())
    }

    pub fn set_buffer_local_void_property_by_sym_id(
        &mut self,
        id: BufferId,
        sym_id: SymId,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .set_buffer_local_void_by_sym_id(sym_id);
        Some(())
    }

    pub fn remove_buffer_local_property(
        &mut self,
        id: BufferId,
        name: &str,
    ) -> Option<Option<RuntimeBindingValue>> {
        let buf = self.buffers.get_mut(&id)?;
        Some(buf.kill_buffer_local(name))
    }

    /// GNU `Fundo_boundary` (src/undo.c:251-282).
    ///
    /// The whole body is guarded by one early return: a buffer whose
    /// `buffer-undo-list` is `t` gets neither the boundary nor the
    /// editor-global saved point.  That guard is load-bearing for the *other*
    /// buffers -- `lisp/` calls `(undo-boundary)` unconditionally in three
    /// dozen places, each in whatever buffer happens to be current, and a
    /// disabled buffer must not spend a point saved for one that records.
    /// Which of `Fundo_boundary`'s two paths ran (GNU src/undo.c:251-282).
    ///
    /// GNU returns early for a buffer whose `buffer-undo-list' is `t'
    /// (:258-259) -- before it sets `undo-auto--last-boundary-cause' (:277) and
    /// before it saves the point/buffer pair (:278-279).  Naming the two
    /// outcomes is what lets the callers set that variable on exactly the path
    /// GNU sets it on: the boundary itself runs below the obarray and cannot
    /// reach a Lisp symbol, so without this the caller would have to guess, and
    /// the previous `Option<()>' gave it nothing to guess from -- the disabled
    /// path and the recorded path both returned `Some(())'.
    pub fn add_undo_boundary(&mut self, id: BufferId) -> Option<UndoBoundaryOutcome> {
        let buf = self.buffers.get_mut(&id)?;
        let mut ul = buf.get_undo_list();
        match undo::UndoRecording::of(&ul) {
            undo::UndoRecording::Disabled => return Some(UndoBoundaryOutcome::UndoDisabled),
            undo::UndoRecording::Enabled => {}
        }
        undo::undo_list_boundary(&mut ul);
        // No truncation here: GNU's `Fundo_boundary' (src/undo.c:251-282)
        // only conses the boundary on.  Undo lists are shortened at garbage
        // collection, from `compact_buffer' (src/buffer.c:1854-1885) — see
        // `crate::emacs_core::undo::compact_buffers_for_gc'.
        buf.set_undo_list(ul);
        // GNU saves point AND buffer into the editor-global pair
        // (src/undo.c:278-279), overwriting whatever was there.
        buf.saved_point_before_command
            .save(id, buf.point_char_pos());
        Some(UndoBoundaryOutcome::Recorded)
    }

    pub fn record_undo_point_before_command(&mut self, id: BufferId) -> Option<()> {
        let buf = self.buffers.get_mut(&id)?;
        // GNU's command loop saves point AND buffer into the editor-global pair
        // (src/keyboard.c:1536-1537).  It runs for EVERY command-loop
        // iteration, including the minibuffer's own, which is what supersedes
        // the point saved for the buffer an `M-x` command later edits.
        buf.saved_point_before_command
            .save(id, buf.point_char_pos());
        Some(())
    }

    pub fn restore_buffer_emacs_byte_restriction(
        &mut self,
        id: BufferId,
        range: EmacsByteRange,
    ) -> Option<()> {
        self.buffers.get_mut(&id)?.narrow_to_emacs_byte_range(range);
        let _ = self.record_buffer_state_markers(id);
        Some(())
    }

    pub fn restore_buffer_accessible_region(
        &mut self,
        id: BufferId,
        snapshot: AccessibleBufferRegionSnapshot,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .restore_accessible_region(snapshot);
        let _ = self.record_buffer_state_markers(id);
        Some(())
    }

    pub fn restore_buffer_accessible_region_with_current_full_end(
        &mut self,
        id: BufferId,
        snapshot: AccessibleBufferRegionSnapshot,
    ) -> Option<()> {
        self.buffers
            .get_mut(&id)?
            .restore_accessible_region_with_current_full_end(snapshot);
        let _ = self.record_buffer_state_markers(id);
        Some(())
    }

    pub fn save_current_restriction_state(&mut self) -> Option<SavedRestrictionState> {
        let buffer_id = self.current_buffer_id()?;
        let (begv, zv, len) = {
            let buffer = self.get(buffer_id)?;
            (
                buffer.point_min_emacs_byte_pos(),
                buffer.point_max_emacs_byte_pos(),
                buffer.total_emacs_byte_len(),
            )
        };
        let restriction = if begv == EmacsBytePos::ZERO && zv.get() == len.get() {
            SavedRestrictionKind::None
        } else {
            let (beg_marker, _) =
                self.create_marker_at_emacs_byte_pos(buffer_id, begv, InsertionType::Before);
            let (end_marker, _) =
                self.create_marker_at_emacs_byte_pos(buffer_id, zv, InsertionType::After);
            SavedRestrictionKind::Markers {
                beg_marker,
                end_marker,
            }
        };
        let labeled_restrictions = self.clone_labeled_restrictions(buffer_id).unwrap_or(None);
        Some(SavedRestrictionState {
            buffer_id,
            restriction,
            labeled_restrictions,
        })
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn reset_outermost_restrictions(&mut self) -> OutermostRestrictionResetState {
        let mut affected_buffers: Vec<BufferId> =
            self.labeled_restrictions.keys().copied().collect();
        affected_buffers.sort_by_key(|buffer_id| buffer_id.0);

        let mut retained_buffers = Vec::with_capacity(affected_buffers.len());
        for buffer_id in affected_buffers {
            let Some(range) = self.labeled_restriction_emacs_byte_range(buffer_id, true) else {
                self.replace_labeled_restrictions(buffer_id, None);
                continue;
            };
            if self
                .restore_buffer_emacs_byte_restriction(buffer_id, range)
                .is_some()
            {
                retained_buffers.push(buffer_id);
            } else {
                self.replace_labeled_restrictions(buffer_id, None);
            }
        }

        OutermostRestrictionResetState {
            affected_buffers: retained_buffers,
        }
    }

    #[tracing::instrument(level = "trace", skip(self, state))]
    pub fn restore_outermost_restrictions(&mut self, state: OutermostRestrictionResetState) {
        for buffer_id in state.affected_buffers {
            if let Some(range) = self.current_labeled_restriction_bounds(buffer_id) {
                let _ = self.restore_buffer_emacs_byte_restriction(buffer_id, range);
            } else {
                self.replace_labeled_restrictions(buffer_id, None);
            }
        }
    }

    pub fn restore_saved_restriction_state(&mut self, saved: SavedRestrictionState) {
        let buffer_id = saved.buffer_id;
        if !self.buffers.contains_key(&buffer_id) {
            self.replace_labeled_restrictions(buffer_id, None);
            return;
        }
        self.replace_labeled_restrictions(buffer_id, saved.labeled_restrictions);
        match saved.restriction {
            SavedRestrictionKind::None => {
                let _ = self.widen_buffer_fully(buffer_id);
            }
            SavedRestrictionKind::Markers {
                beg_marker,
                end_marker,
            } => {
                let beg = self.marker_emacs_byte_pos(buffer_id, beg_marker);
                let end = self.marker_emacs_byte_pos(buffer_id, end_marker);
                if let (Some(begv), Some(zv), Some(len)) = (
                    beg,
                    end,
                    self.buffers
                        .get(&buffer_id)
                        .map(|buffer| buffer.total_emacs_byte_len()),
                ) {
                    let end = EmacsBytePos::ZERO.add_len(len);
                    let mut restored_begv = begv.min(end);
                    let mut restored_zv = zv.min(end);
                    if restored_begv > restored_zv {
                        std::mem::swap(&mut restored_begv, &mut restored_zv);
                    }
                    let _ = self.restore_buffer_emacs_byte_restriction(
                        buffer_id,
                        EmacsByteRange::new(restored_begv, restored_zv),
                    );
                }
                self.remove_marker(beg_marker);
                self.remove_marker(end_marker);
            }
        }
    }

    pub fn internal_labeled_narrow_to_emacs_byte_range(
        &mut self,
        buffer_id: BufferId,
        range: EmacsByteRange,
        label: Value,
    ) -> Option<()> {
        self.buffers.get(&buffer_id)?;
        if self.labeled_restriction_at(buffer_id, false).is_none() {
            self.push_labeled_restriction_for_current_bounds(
                buffer_id,
                LabeledRestrictionLabel::Outermost,
            )?;
        }
        self.restore_buffer_emacs_byte_restriction(buffer_id, range)?;
        self.push_labeled_restriction_for_current_bounds(
            buffer_id,
            LabeledRestrictionLabel::User(label),
        )?;
        Some(())
    }

    pub fn internal_labeled_widen(&mut self, buffer_id: BufferId, label: &Value) -> Option<()> {
        self.buffers.get(&buffer_id)?;
        if self.current_labeled_restriction_matches_label(buffer_id, label) {
            let _ = self.pop_labeled_restriction(buffer_id);
        }
        self.widen_buffer(buffer_id)
    }

    pub fn configure_buffer_undo_list(&mut self, id: BufferId, value: Value) -> Option<()> {
        {
            let buf = self.buffers.get_mut(&id)?;
            match value.kind() {
                ValueKind::T => {
                    buf.set_buffer_local(BUFFER_UNDO_LIST_NAME, Value::T);
                }
                ValueKind::Nil => {
                    buf.set_buffer_local(BUFFER_UNDO_LIST_NAME, Value::NIL);
                    buf.undo_state.set_recorded_first_change(false);
                }
                _other => {
                    buf.set_buffer_local(BUFFER_UNDO_LIST_NAME, value);
                }
            }
        }
        Some(())
    }

    /// Turn undo recording ON for `id`, GNU's `Fbuffer_enable_undo`
    /// (src/buffer.c:1845-1847):
    ///
    /// ```c
    ///   if (EQ (BVAR (XBUFFER (real_buffer), undo_list), Qt))
    ///     bset_undo_list (XBUFFER (real_buffer), Qnil);
    /// ```
    ///
    /// Enabling is a transition out of [`undo::UndoRecording::Disabled`], not an
    /// assignment: a buffer whose undo is already on keeps its history.  That
    /// distinction is load-bearing for indirect buffers, which share one undo
    /// list with their base (GNU copies the base's value in
    /// `make_indirect_buffer`, src/buffer.c:894, and re-syncs the pair on every
    /// buffer switch in `set_buffer_internal_2`, src/buffer.c:2352-2367), so an
    /// unconditional reset here erased the BASE buffer's history.
    pub fn enable_buffer_undo(&mut self, id: BufferId) -> Option<()> {
        let undo_list = self.buffers.get(&id)?.get_undo_list();
        match undo::UndoRecording::of(&undo_list) {
            undo::UndoRecording::Disabled => self.configure_buffer_undo_list(id, Value::NIL),
            undo::UndoRecording::Enabled => Some(()),
        }
    }

    // No `undo_buffer' here.  There used to be a third undo replay loop at
    // this spot, reachable only through a Rust `undo' subr that preloaded
    // Lisp shadowed in every real session (DIVERGENCES.md 150).  It popped
    // groups off `buffer-undo-list' DESTRUCTIVELY, where GNU's `undo'
    // (lisp/simple.el:3466) never removes anything from that list -- it
    // walks `pending-undo-list' and pushes redo records -- and it could not
    // run an `apply' entry at all, because this layer has no evaluator.
    // Replay belongs to `primitive-undo' (lisp/simple.el:3645), which is
    // Lisp, and this layer's job is RECORDING.

    /// Generate a unique buffer name.  If `base` is not taken, returns it
    /// unchanged; otherwise follows GNU `generate-new-buffer-name`.
    pub fn generate_new_buffer_name(&self, base: &str) -> String {
        self.generate_new_buffer_name_ignoring_with_random(base, None, || {
            crate::emacs_core::builtins::emacs_get_random()
        })
    }

    /// Generate a unique buffer name, allowing `ignore` to be reused even if
    /// a live buffer already owns that name.
    pub fn generate_new_buffer_name_ignoring(&self, base: &str, ignore: Option<&str>) -> String {
        self.generate_new_buffer_name_ignoring_with_random(base, ignore, || {
            crate::emacs_core::builtins::emacs_get_random()
        })
    }

    pub(crate) fn generate_new_buffer_name_ignoring_with_random(
        &self,
        base: &str,
        ignore: Option<&str>,
        mut random: impl FnMut() -> i64,
    ) -> String {
        if ignore == Some(base) || self.find_buffer_by_name(base).is_none() {
            return base.to_string();
        }
        let genbase = if base.as_bytes().first() == Some(&b' ') {
            let random_suffix = random().rem_euclid(1_000_000);
            let candidate = format!("{base}-{random_suffix}");
            if self.find_buffer_by_name(&candidate).is_none() {
                return candidate;
            }
            candidate
        } else {
            base.to_string()
        };
        let mut n = 2u64;
        loop {
            let candidate = format!("{genbase}<{n}>");
            if ignore == Some(candidate.as_str()) || self.find_buffer_by_name(&candidate).is_none()
            {
                return candidate;
            }
            n += 1;
        }
    }

    /// Allocate a unique marker id without associating it with a buffer.
    pub fn allocate_marker_id(&mut self) -> u64 {
        let id = self.next_marker_id;
        self.next_marker_id += 1;
        id
    }

    /// Create a marker in `buffer_id` at byte position `pos` with the given
    /// insertion type.  Returns the new marker's id and the raw
    /// `MarkerObj` pointer for the backing allocation.
    ///
    /// The backing `MarkerObj` is allocated via the tagged heap and
    /// spliced onto the owning buffer's intrusive chain. Chain
    /// membership is NOT a GC root: `unchain_dead_markers` splices out
    /// any MarkerObj that isn't marked by the mark phase, so Lisp-side
    /// code must keep a live `Value` reference (or an explicit root —
    /// see `BufferStateMarkers` for the pt/begv/zv case) for the
    /// marker to survive GC. Callers that need to re-register this marker
    /// later (e.g. state-marker buffer switch plumbing) should retain the
    /// returned pointer and pass it to `register_marker_id_at_emacs_byte_pos`
    /// after first calling `chain_unlink` on the
    /// owning buffer's `BufferText` to satisfy the
    /// `chain_splice_at_head` precondition.
    pub fn create_marker_at_emacs_byte_pos(
        &mut self,
        buffer_id: BufferId,
        pos: EmacsBytePos,
        insertion_type: InsertionType,
    ) -> (u64, *mut crate::tagged::header::MarkerObj) {
        let marker_id = self.next_marker_id;
        self.next_marker_id += 1;
        let marker_ptr = Self::allocate_marker_node(buffer_id, marker_id, insertion_type);
        let _ = self.register_marker_id_at_emacs_byte_pos(
            marker_ptr,
            buffer_id,
            marker_id,
            pos,
            insertion_type,
        );
        (marker_id, marker_ptr)
    }

    pub fn create_marker_at_anchor(
        &mut self,
        buffer_id: BufferId,
        position: TextPositionAnchor,
        insertion_type: InsertionType,
    ) -> (u64, *mut crate::tagged::header::MarkerObj) {
        let marker_id = self.next_marker_id;
        self.next_marker_id += 1;
        let marker_ptr = Self::allocate_marker_node(buffer_id, marker_id, insertion_type);
        let _ = self.register_marker_id_at_anchor(
            marker_ptr,
            buffer_id,
            marker_id,
            position,
            insertion_type,
        );
        (marker_id, marker_ptr)
    }

    fn allocate_marker_node(
        buffer_id: BufferId,
        marker_id: u64,
        insertion_type: InsertionType,
    ) -> *mut crate::tagged::header::MarkerObj {
        // Allocate a backing MarkerObj so the new chain has a valid node.
        // Position fields are overwritten inside register_marker; starting
        // values are placeholders.
        let marker_value =
            crate::emacs_core::value::Value::make_marker(crate::heap_types::LispMarker {
                buffer: Some(buffer_id),
                insertion_type: insertion_type == InsertionType::After,
                marker_id: Some(marker_id),
                bytepos: 0,
                charpos: 0,
                last_position_valid: true,
                next_marker: std::ptr::null_mut(),
            });
        marker_value
            .as_veclike_ptr()
            .expect("freshly allocated marker should have a veclike ptr")
            as *mut crate::tagged::header::MarkerObj
    }

    pub fn register_marker_id_at_emacs_byte_pos(
        &mut self,
        marker_ptr: *mut crate::tagged::header::MarkerObj,
        buffer_id: BufferId,
        marker_id: u64,
        pos: EmacsBytePos,
        insertion_type: InsertionType,
    ) -> Option<()> {
        let buf = self.buffers.get_mut(&buffer_id)?;
        buf.register_marker_at_emacs_byte_pos(marker_ptr, marker_id, pos, insertion_type);
        Some(())
    }

    pub fn register_marker_id_at_anchor(
        &mut self,
        marker_ptr: *mut crate::tagged::header::MarkerObj,
        buffer_id: BufferId,
        marker_id: u64,
        position: TextPositionAnchor,
        insertion_type: InsertionType,
    ) -> Option<()> {
        let buf = self.buffers.get_mut(&buffer_id)?;
        buf.register_marker_at_anchor(marker_ptr, marker_id, position, insertion_type);
        Some(())
    }

    pub fn marker_anchor_position(
        &self,
        buffer_id: BufferId,
        marker_id: u64,
    ) -> Option<TextPositionAnchor> {
        self.buffers
            .get(&buffer_id)
            .and_then(|buf| buf.marker_chain_anchor_lookup(marker_id))
            .map(|(position, _ins)| position)
    }

    pub fn marker_emacs_byte_pos(
        &self,
        buffer_id: BufferId,
        marker_id: u64,
    ) -> Option<EmacsBytePos> {
        self.marker_anchor_position(buffer_id, marker_id)
            .map(TextPositionAnchor::emacs_byte_pos)
    }

    pub fn marker_char_pos(&self, buffer_id: BufferId, marker_id: u64) -> Option<CharPos0> {
        self.marker_anchor_position(buffer_id, marker_id)
            .map(TextPositionAnchor::char_pos)
    }

    pub fn marker_value(&self, buffer_id: BufferId, marker_id: u64) -> Option<Value> {
        self.buffers
            .get(&buffer_id)
            .and_then(|buf| buf.marker_value_by_id(marker_id))
    }

    pub fn unlink_marker_ptr(
        &self,
        buffer_id: BufferId,
        ptr: *mut crate::tagged::header::MarkerObj,
    ) -> Option<()> {
        self.buffers.get(&buffer_id)?.unlink_marker_ptr(ptr);
        Some(())
    }

    pub fn move_marker_to_anchor(
        &mut self,
        buffer_id: BufferId,
        marker_id: u64,
        position: TextPositionAnchor,
    ) -> Option<()> {
        self.buffers
            .get_mut(&buffer_id)?
            .move_marker_to_anchor(marker_id, position);
        Some(())
    }

    /// Phase 10D: write the global default for a `BUFFER_OBJFWD`
    /// slot, propagating the new default to every live buffer
    /// whose `local_flags` bit for that slot is clear.
    ///
    /// Mirrors GNU `set_default_internal` SYMBOL_FORWARDED arm
    /// (`data.c:2044-2078`): the new default is stored in
    /// `buffer_defaults` and broadcast into every buffer that
    /// shares the global value (i.e. has not made the variable
    /// local). Always-local slots (`local_flags_idx == -1`) are
    /// per-buffer in every buffer, so the propagation is a no-op
    /// for them — only `buffer_defaults` is updated.
    pub fn set_buffer_default_slot(&mut self, info: &BufferSlotInfo, value: Value) {
        debug_assert!(info.offset.index() < BUFFER_SLOT_COUNT);
        self.buffer_defaults[info.offset.index()] = value;
        if info.local_flags_idx >= 0 {
            // Conditional slot: propagate to non-local buffers.
            for buf in self.buffers.values_mut() {
                if !buf.slot_local_flag(info.offset) {
                    buf.slots[info.offset.index()] = value;
                }
            }
        }
    }

    /// Remove a marker registration from any live buffer.
    pub fn remove_marker(&mut self, marker_id: u64) {
        for buf in self.buffers.values_mut() {
            buf.remove_marker_entry(marker_id);
        }
    }

    /// Update the insertion type of a registered marker across all buffers.
    pub fn update_marker_insertion_type(&mut self, marker_id: u64, ins_type: InsertionType) {
        for buf in self.buffers.values_mut() {
            // T7: chain presence check replaces the deleted Vec-based
            // `marker_entry().is_some()`.
            if buf.has_marker(marker_id) {
                buf.update_marker_insertion_type(marker_id, ins_type);
                return;
            }
        }
    }

    // pdump accessors
    pub(crate) fn dump_buffers(&self) -> &FxHashMap<BufferId, Buffer> {
        &self.buffers
    }
    pub(crate) fn dump_buffer_order(&self) -> &[BufferId] {
        &self.buffer_order
    }
    pub(crate) fn dump_current(&self) -> Option<BufferId> {
        self.current
    }
    pub(crate) fn dump_next_id(&self) -> u64 {
        self.next_id
    }
    pub(crate) fn dump_next_marker_id(&self) -> u64 {
        self.next_marker_id
    }
    pub(crate) fn from_dump(
        mut buffers: FxHashMap<BufferId, Buffer>,
        current: Option<BufferId>,
        next_id: u64,
        next_marker_id: u64,
        dumped_buffer_order: Option<&[BufferId]>,
        dumped_buffer_defaults: Option<&[crate::emacs_core::value::Value]>,
        default_text_backend_kind: ImplementedBufferTextBackendKind,
        saved_point_before_command: SavedPointBeforeCommand,
    ) -> Self {
        // The restored editor gets ONE saved-point cell, like GNU's statics on
        // a fresh startup.  Attaching here rather than trusting the caller is
        // what keeps a restored buffer from carrying a private cell forward.
        for buffer in buffers.values_mut() {
            buffer.saved_point_before_command = saved_point_before_command.clone();
        }
        let indirect_buffers: Vec<(BufferId, BufferId)> = buffers
            .iter()
            .filter_map(|(id, buffer)| buffer.base_buffer.map(|base_id| (*id, base_id)))
            .collect();
        for (buffer_id, base_id) in indirect_buffers {
            // Borrow base buffer's text/undo state directly from the
            // map. `BufferText::Clone` is a deep clone (it allocates a
            // fresh `Rc<RefCell<BufferTextStorage>>`), so cloning a
            // base Buffer first and then `shared_clone`ing its text
            // would link the indirect buffer to the *temporary* base,
            // not the one in `buffers`. Use `shared_clone` directly on
            // the base buffer's `BufferText` to preserve Rc identity.
            // The visited-file modtime cell is the third thing an indirect
            // buffer borrows from its base (GNU `record_first_change` follows
            // `b->base_buffer`, src/undo.c:213-214) and it has exactly the
            // same aliasing hazard as the two above: `share_own` hands out the
            // base's live cell, `clone()` would hand out a copy.
            let (shared_text, shared_undo, base_modtime) = match buffers.get(&base_id) {
                Some(root) => (
                    root.text.shared_clone(),
                    root.undo_state.clone(),
                    root.modtime.share_own(),
                ),
                None => continue,
            };
            let Some(buffer) = buffers.get_mut(&buffer_id) else {
                continue;
            };
            buffer.text = shared_text;
            buffer.undo_state = shared_undo;
            buffer.modtime.reset_as_indirect_of(base_modtime);
        }

        // Seed `buffer_defaults` from BUFFER_SLOT_INFO's install-time
        // defaults, then overlay any values the dump carried. The
        // overlay preserves runtime defaults set by `setq-default`
        // during pdump creation (e.g. bindings.el's rich
        // `mode-line-format` list); the seed provides backward
        // compatibility for older dumps that didn't carry the
        // `buffer_defaults` field.
        let mut buffer_defaults = [crate::emacs_core::value::Value::NIL; BUFFER_SLOT_COUNT];
        for info in BUFFER_SLOT_INFO {
            buffer_defaults[info.offset.index()] = info.default.to_value();
        }
        if let Some(dumped) = dumped_buffer_defaults {
            for (idx, value) in dumped.iter().enumerate() {
                if idx >= BUFFER_SLOT_COUNT {
                    break;
                }
                buffer_defaults[idx] = *value;
            }
        }
        let mut manager = Self {
            buffers,
            buffer_order: Vec::new(),
            current,
            next_id,
            next_marker_id,
            labeled_restrictions: FxHashMap::default(),
            dead_buffers: FxHashMap::default(),
            default_text_backend_kind,
            buffer_defaults,
            saved_point_before_command,
        };
        if let Some(dumped_order) = dumped_buffer_order {
            let mut seen = HashSet::new();
            for id in dumped_order {
                if manager.buffers.contains_key(id) && seen.insert(*id) {
                    manager.buffer_order.push(*id);
                }
            }
            let mut missing: Vec<BufferId> = manager
                .buffers
                .keys()
                .copied()
                .filter(|id| seen.insert(*id))
                .collect();
            missing.sort_by_key(|id| id.0);
            manager.buffer_order.extend(missing);
        } else {
            manager.buffer_order = manager.buffers.keys().copied().collect();
            manager.buffer_order.sort_by_key(|id| id.0);
            if let Some(current) = manager.current
                && manager.buffers.contains_key(&current)
            {
                manager.note_buffer_order_head(current);
            }
        }
        manager
    }
}

impl Default for BufferManager {
    fn default() -> Self {
        Self::new()
    }
}

impl GcTrace for BufferManager {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for buffer in self.buffers.values() {
            roots.push(buffer.name);
            roots.push(buffer.last_name);
            buffer.text.trace_text_prop_roots(roots);
            buffer.undo_state.trace_roots(roots);
            buffer.overlays.trace_roots(roots);
            // BUFFER_OBJFWD slot table holds Lisp values that must
            // be GC-rooted. Mirrors GNU's `mark_buffer` walking the
            // C-side BVAR slots in `alloc.c`.
            for slot in &buffer.slots {
                roots.push(*slot);
            }
            // Phase 10F: `local_var_alist` is the single source of
            // truth for non-slot per-buffer bindings. The cons
            // cells forming the alist must be rooted (along with
            // every entry's value). A single push of the alist
            // head is sufficient — the GC's reachability walk
            // follows the spine.
            roots.push(buffer.local_var_alist.as_lisp_alist());
            // `local_map` (buffer's keymap) must also be rooted.
            roots.push(buffer.keymap);
            // GNU stores the buffer mark in `BVAR (buffer, mark)`, so
            // `mark_vectorlike (&buffer->header)` in `mark_buffer` roots it
            // with the rest of the buffer's Lisp slots.  Neomacs stores the
            // same live marker as a raw pointer in Buffer; synthesize the
            // tagged Value here so GC does not reclaim it while the buffer
            // remains live.
            unsafe {
                if !buffer.mark_marker_ptr.is_null() {
                    roots.push(Value::from_veclike_ptr(
                        buffer.mark_marker_ptr as *const crate::tagged::header::VecLikeHeader,
                    ));
                }
            }
            // T8 C-1: the noncurrent PT/BEGV/ZV markers stashed in
            // `state_markers` are referenced only by raw pointers and
            // the intrusive marker chain. Neither is a GC root on its
            // own: the chain is spliced by `unchain_dead_markers` in
            // the mark/sweep gap, so anything not independently marked
            // would be freed here and leave `BufferStateMarkers`
            // holding dangling pointers. Synthesize a Value for each
            // `MarkerObj*` and seed it into the root set so the mark
            // phase walks them. Mirrors GNU `mark_buffer` walking the
            // buffer's `pt_marker` / `begv_marker` / `zv_marker`
            // `Lisp_Object` BVAR slots in `alloc.c`.
            if let Some(sm) = buffer.state_markers {
                // SAFETY: the pointers were obtained from
                // `as_veclike_ptr()` on freshly allocated marker
                // values in `create_marker` and stored in
                // `state_markers`; they remain valid as long as the
                // buffer exists. `from_veclike_ptr` is a pure bit-cast
                // to the tagged encoding.
                unsafe {
                    if !sm.pt_marker_ptr.is_null() {
                        roots.push(Value::from_veclike_ptr(
                            sm.pt_marker_ptr as *const crate::tagged::header::VecLikeHeader,
                        ));
                    }
                    if !sm.begv_marker_ptr.is_null() {
                        roots.push(Value::from_veclike_ptr(
                            sm.begv_marker_ptr as *const crate::tagged::header::VecLikeHeader,
                        ));
                    }
                    if !sm.zv_marker_ptr.is_null() {
                        roots.push(Value::from_veclike_ptr(
                            sm.zv_marker_ptr as *const crate::tagged::header::VecLikeHeader,
                        ));
                    }
                }
            }
        }
        for buffer in self.dead_buffers.values() {
            roots.push(buffer.name);
            roots.push(buffer.last_name);
            buffer.text.trace_text_prop_roots(roots);
            buffer.undo_state.trace_roots(roots);
            buffer.overlays.trace_roots(roots);
            for slot in &buffer.slots {
                roots.push(*slot);
            }
            roots.push(buffer.local_var_alist.as_lisp_alist());
            roots.push(buffer.keymap);
        }
        // Phase 10D: `buffer_defaults` holds the global default
        // values for every per-buffer slot. Mirrors GNU's
        // `mark_buffer (&buffer_defaults)` in `alloc.c`.
        for slot in &self.buffer_defaults {
            roots.push(*slot);
        }
        for (buffer_id, restrictions) in &self.labeled_restrictions {
            let buffer = self.buffers.get(buffer_id);
            for restriction in restrictions {
                if let LabeledRestrictionLabel::User(label) = restriction.label {
                    roots.push(label);
                }
                // The bounds markers are referenced only by u64 id from this
                // map; an unmarked marker is spliced out and freed by
                // unchain_dead_markers, silently widening the restriction.
                // Synthesize Lisp handles so the mark phase walks them
                // (same hazard and fix as state_markers above; GNU keeps
                // these alive via staticpro'd narrowing_locks, editfns.c).
                if let Some(buffer) = buffer {
                    if let Some(value) = buffer.marker_value_by_id(restriction.beg_marker) {
                        roots.push(value);
                    }
                    if let Some(value) = buffer.marker_value_by_id(restriction.end_marker) {
                        roots.push(value);
                    }
                }
            }
        }
    }
}

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
#[path = "buffer_test.rs"]
mod tests;
