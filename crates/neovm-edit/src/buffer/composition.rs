//! Composition-span substrate seam (stage-3 C7).
//!
//! The buffer substrate owns the automatic-composition span data in its
//! redisplay caches and needs the composition width on the indent path.  The
//! span type, the scanned-byte counter and the fast-path policy are plain
//! data and live here; the scans that read the live Lisp
//! `composition-function-table` (and the width registration that rewrites a
//! Form-A property) stay in `emacs_core::composite` and are installed once as
//! static fn pointers at evaluator construction.  No dynamic dispatch and no
//! new TLS: a plain `OnceLock` of function pointers.

use std::sync::OnceLock;

use neovm_value::tagged::SymbolMode;
use neovm_value::value::Value;

use super::{Buffer, BufferManager, CharPos0, CharRange};

/// A half-open character range selected by GNU's automatic-composition rule
/// table.
///
/// This is deliberately distinct from a Unicode grapheme range: the Lisp
/// `composition-function-table` may compose spacing characters, look behind
/// the trigger character, or leave zero-width characters uncomposed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AutomaticCompositionSpan {
    range: CharRange,
}

impl AutomaticCompositionSpan {
    pub const fn new(start: usize, end: usize) -> Self {
        Self {
            range: CharRange::new(CharPos0::new(start), CharPos0::new(end)),
        }
    }

    pub const fn start(self) -> usize {
        self.range.start().get()
    }

    pub const fn end(self) -> usize {
        self.range.end().get()
    }
}

/// Bytes of buffer text this process has actually scanned for automatic
/// compositions. Bumped only when the memo misses, so it reports work DONE
/// rather than work asked for. Read once per accepted frame into
/// `LayoutStats::composition_bytes_scanned`.
pub static BYTES_SCANNED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// GNU's look-back bound for automatic compositions (composite.c:156).
///
/// A rule may claim up to this many characters BEFORE the one that triggered
/// it, so a scan that wants every composition starting in a range must begin
/// this far ahead of it. GNU calls the same number "a limitation imposed by
/// composition rules in composition-function-table" (composite.c:1597).
pub const MAX_AUTO_COMPOSITION_LOOKBACK: usize = 3;

/// `NEOMACS_COMPOSITION_FASTPATH` (P3.5 F): the visible scan returns at once
/// for pure-ASCII text when no ASCII character has a rule, and otherwise is
/// memoized per buffer. Read once per process; default on.
/// Explicit off, empty and unknown settings keep the full scan.
pub mod fast_path {
    use std::cell::Cell;
    use std::sync::OnceLock;

    thread_local! {
        static OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    }

    /// Force the fast path on or off on this thread; `None` returns to the
    /// knob. For tests, including other crates' (the layout engine).
    pub fn set_override(enabled: Option<bool>) {
        OVERRIDE.with(|cell| cell.set(enabled));
    }

    /// Whether the fast path is on.
    #[inline]
    pub fn enabled() -> bool {
        if let Some(enabled) = OVERRIDE.with(|cell| cell.get()) {
            return enabled;
        }
        static ENABLED: OnceLock<bool> = OnceLock::new();
        *ENABLED.get_or_init(|| {
            matches!(
                std::env::var("NEOMACS_COMPOSITION_FASTPATH")
                    .ok()
                    .map(|value| value.trim().to_ascii_lowercase())
                    .as_deref(),
                None | Some("on" | "1" | "true" | "yes")
            )
        })
    }
}

/// Text-layer composition queries used by the substrate.
///
/// Both functions are installed once by `emacs_core::composite`; they read the
/// live Lisp rule table and stay above the substrate.  Plain fn pointers keep
/// the call sites static and one indirection deep.
#[doc(hidden)]
pub struct CompositionSeams {
    pub(crate) spans: fn(&Buffer, Value, &str, usize, SymbolMode) -> Vec<AutomaticCompositionSpan>,
    pub(crate) ascii_no_rules: fn(Value, SymbolMode) -> bool,
    /// The interned `composition` property symbol (GNU `Qcomposition`).
    pub(crate) composition_symbol: fn() -> Value,
    /// Width and character length of the composition whose property value is
    /// PROP at 1-based position CHARPOS1 (GNU `get_composition_id`); the
    /// caller resolves the property itself.
    pub(crate) width: fn(Value, &BufferManager, i64) -> Option<(i64, i64)>,
}

impl CompositionSeams {
    /// The automatic-composition spans a scan of `text` selects, at absolute
    /// char coordinates offset by `char_offset`.
    #[inline]
    pub(crate) fn spans(
        &self,
        buffer: &Buffer,
        composition_function_table: Value,
        text: &str,
        char_offset: usize,
        mode: SymbolMode,
    ) -> Vec<AutomaticCompositionSpan> {
        (self.spans)(buffer, composition_function_table, text, char_offset, mode)
    }

    /// Whether the table gives ASCII characters no composition rule at all.
    #[inline]
    pub(crate) fn ascii_no_rules(&self, table: Value, mode: SymbolMode) -> bool {
        (self.ascii_no_rules)(table, mode)
    }

    /// The `composition` property symbol.
    #[inline]
    pub(crate) fn property_symbol(&self) -> Value {
        (self.composition_symbol)()
    }

    /// GNU `get_composition_id`: width and length of the composition at
    /// CHARPOS1 whose property value is PROP, registering it when this is the
    /// first sighting.
    #[inline]
    pub(crate) fn width_of(
        &self,
        prop: Value,
        buffers: &BufferManager,
        charpos1: i64,
    ) -> Option<(i64, i64)> {
        (self.width)(prop, buffers, charpos1)
    }
}

static SEAMS: OnceLock<CompositionSeams> = OnceLock::new();

#[doc(hidden)]
pub fn install(seams: CompositionSeams) {
    let _ = SEAMS.set(seams);
}

pub(crate) fn seams() -> &'static CompositionSeams {
    SEAMS
        .get()
        .expect("composition substrate seams are installed at evaluator construction")
}

#[doc(hidden)]
impl CompositionSeams {
    pub const fn new(
        spans: fn(&Buffer, Value, &str, usize, SymbolMode) -> Vec<AutomaticCompositionSpan>,
        ascii_no_rules: fn(Value, SymbolMode) -> bool,
        composition_symbol: fn() -> Value,
        width: fn(Value, &BufferManager, i64) -> Option<(i64, i64)>,
    ) -> Self {
        Self {
            spans,
            ascii_no_rules,
            composition_symbol,
            width,
        }
    }
}
