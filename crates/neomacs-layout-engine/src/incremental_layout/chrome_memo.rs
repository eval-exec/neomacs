//! P3.5 E2: reuse the previous chrome row when the freshly evaluated chrome
//! string renders the same (`NEOMACS_CHROME_MEMO`).
//!
//! The mode line is evaluated whenever GNU would evaluate it (the `:eval`
//! count is Lisp-observable), but most evaluations produce the string the
//! window already shows: typing changes neither the buffer name, the modes
//! nor, until a newline, the line number. Rendering that string again --
//! faces, glyphs, measurement -- costs more than the evaluation. The memo
//! keeps the evaluation and skips the render when every input the render
//! reads is equal to the previous frame's.
//!
//! "Every input" is spelled out in [`ChromeRowFingerprint`]: the characters,
//! the property runs reduced to plain data, the source-span shape that maps
//! glyphs back to their strings, and the request facts around the string
//! (bounds, active face, fallback metrics, tab policy, the face, character
//! table and media revisions). No Lisp `Value` is kept: symbols, fixnums and
//! floats are immediates, strings are copied, and anything else -- a marker,
//! a closure, a char-table -- makes the row unmemoizable. A `display`
//! property also does: a display spec can read variables and evaluate
//! `(when ...)` conditions that no fingerprint of the string sees.
//!
//! Mouse targets are never memoized. A hit publishes the fresh string's
//! sources, so `help-echo`, `local-map` and `keymap` come from this frame's
//! evaluation; those three properties do not reach the glyphs and are
//! fingerprinted by name only.
//!
//! The memo lives in the replay that already admits the previous frame's
//! chrome rows (their face ids stay valid for the same reason a retained
//! chrome's do), so a full layout never hits it.
//!
//! * `off` (the default): no fingerprints, no memo.
//! * `on`: a hit installs the previous row.
//! * `verify`: a hit renders anyway and compares the two rows; a mismatch is
//!   logged and counted, and the rendered row stands.

use std::sync::Arc;

use neomacs_display_protocol::frame_glyphs::GlyphRowRole;
use neomacs_display_protocol::glyph_matrix::{GlyphRow, MatrixRow};
use neomacs_display_protocol::types::Rect;
use neovm_core::emacs_core::image_catalog::ImageScaleEnvironment;
use neovm_core::emacs_core::value::{Value, ValueKind, VecLikeType};
use neovm_core::emacs_core::xdisp::ModeLineDisplayOutput;
use neovm_core::window::DisplayRowSnapshot;

use crate::display_row::builder::DisplayTabPolicy;
use crate::display_row::measured_state::WindowChromeKind;
use crate::display_row::metrics::DisplayRowFallbackMetrics;
use crate::neovm_bridge::ResolvedFace;

use super::RetainedChrome;

/// `NEOMACS_CHROME_MEMO`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ChromeMemoMode {
    Off,
    On,
    /// Render on a hit too, and compare.
    Verify,
}

impl ChromeMemoMode {
    pub(crate) fn enabled(self) -> bool {
        !matches!(self, Self::Off)
    }
}

#[cfg(test)]
thread_local! {
    static MODE_OVERRIDE: std::cell::Cell<Option<ChromeMemoMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force the mode on this thread (tests only); `None` restores the knob.
#[cfg(test)]
pub(crate) fn set_chrome_memo_mode_for_test(mode: Option<ChromeMemoMode>) {
    MODE_OVERRIDE.with(|cell| cell.set(mode));
}

/// The mode in effect. Read once per process; default off.
pub(crate) fn chrome_memo_mode() -> ChromeMemoMode {
    #[cfg(test)]
    if let Some(mode) = MODE_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    static MODE: std::sync::OnceLock<ChromeMemoMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        match std::env::var("NEOMACS_CHROME_MEMO")
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .as_deref()
        {
            Some("on" | "1" | "true" | "yes") => ChromeMemoMode::On,
            Some("verify") => ChromeMemoMode::Verify,
            _ => ChromeMemoMode::Off,
        }
    })
}

/// Chrome rows installed from the memo this frame.
pub(crate) static CHROME_MEMO_HITS: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// `verify` hits whose rendered row differed from the memo's.
pub(crate) static CHROME_MEMO_VERIFY_MISMATCHES: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Take and reset the hit counter (once per accepted frame).
pub(crate) fn take_chrome_memo_hits() -> usize {
    CHROME_MEMO_HITS.swap(0, std::sync::atomic::Ordering::Relaxed)
}

/// The glyph-row role a window chrome kind renders into.
pub(crate) fn chrome_row_role(kind: WindowChromeKind) -> GlyphRowRole {
    match kind {
        WindowChromeKind::TabLine => GlyphRowRole::TabLine,
        WindowChromeKind::HeaderLine => GlyphRowRole::HeaderLine,
        WindowChromeKind::ModeLine => GlyphRowRole::ModeLine,
    }
}

/// The inputs of one chrome row render other than the string itself.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChromeRowInputs {
    pub(crate) kind: WindowChromeKind,
    pub(crate) display_row_index: usize,
    pub(crate) bounds: Rect,
    pub(crate) text_area_left_px: f32,
    pub(crate) selected: bool,
    pub(crate) metrics: DisplayRowFallbackMetrics,
    pub(crate) tab_policy: DisplayTabPolicy,
    pub(crate) base_face: ResolvedFace,
    pub(crate) image_scale_environment: ImageScaleEnvironment,
    /// Identity of the buffer's `glyphless-char-display` table, if any. Its
    /// contents are covered by `char_table_revision`.
    pub(crate) glyphless_table_bits: Option<usize>,
    pub(crate) automatic_composition: bool,
    pub(crate) face_change_count: u64,
    pub(crate) char_table_revision: neovm_core::window::CharTableLayoutRevision,
    pub(crate) media_generation: u64,
    /// `header-line-indent-width` and any other symbol value the render
    /// reads, encoded.
    pub(crate) symbol_values: Box<[u8]>,
}

/// Everything a chrome row render reads, as plain data.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ChromeRowFingerprint {
    inputs: ChromeRowInputs,
    text: Box<[u8]>,
    multibyte: bool,
    props: Box<[u8]>,
    spans: Box<[ChromeSpanShape]>,
}

/// One source span: where it sits in the output, where it starts in its
/// source, and which of the row's distinct sources it came from (the order
/// in which string ids are assigned).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ChromeSpanShape {
    output_start: usize,
    output_end: usize,
    source_start: usize,
    source_index: u32,
}

impl ChromeRowFingerprint {
    /// `None` when the row cannot be memoized: a property value that is not
    /// plain data, a `display` property, or a structure too large to walk.
    pub(crate) fn capture(
        inputs: ChromeRowInputs,
        formatted: &ModeLineDisplayOutput,
    ) -> Option<Self> {
        let root = formatted.value();
        let string = root.as_lisp_string()?;
        let mut encoder = PlainEncoder::default();
        encoder.string_props(string, PropertyScope::Root, 0).ok()?;
        let mut sources: Vec<Value> = Vec::new();
        let spans = formatted
            .source_spans()
            .iter()
            .map(|span| {
                let source = span.source();
                let source_index = match sources.iter().position(|seen| *seen == source) {
                    Some(index) => index,
                    None => {
                        sources.push(source);
                        sources.len() - 1
                    }
                };
                ChromeSpanShape {
                    output_start: span.output_start(),
                    output_end: span.output_end(),
                    source_start: span.source_start(),
                    source_index: u32::try_from(source_index).unwrap_or(u32::MAX),
                }
            })
            .collect();
        // Whether a span's source IS the root string decides its string id.
        let root_index = sources.iter().position(|source| *source == root);
        let mut text = Vec::with_capacity(string.as_bytes().len() + 8);
        text.extend_from_slice(string.as_bytes());
        text.extend_from_slice(
            &root_index
                .map_or(u64::MAX, |index| index as u64)
                .to_le_bytes(),
        );
        Some(Self {
            inputs,
            text: text.into_boxed_slice(),
            multibyte: string.is_multibyte(),
            props: encoder.finish(),
            spans,
        })
    }

    pub(crate) fn kind(&self) -> WindowChromeKind {
        self.inputs.kind
    }

    /// The first part of `self` that differs from `other`, for the miss
    /// diagnostics (`RUST_LOG=neomacs::chrome_memo=debug`).
    pub(crate) fn first_difference(&self, other: &Self) -> Option<&'static str> {
        let a = &self.inputs;
        let b = &other.inputs;
        [
            (a.kind != b.kind, "kind"),
            (
                a.display_row_index != b.display_row_index,
                "display_row_index",
            ),
            (a.bounds != b.bounds, "bounds"),
            (
                a.text_area_left_px != b.text_area_left_px,
                "text_area_left_px",
            ),
            (a.selected != b.selected, "selected"),
            (a.metrics != b.metrics, "metrics"),
            (a.tab_policy != b.tab_policy, "tab_policy"),
            (a.base_face != b.base_face, "base_face"),
            (
                a.image_scale_environment != b.image_scale_environment,
                "image_scale",
            ),
            (
                a.glyphless_table_bits != b.glyphless_table_bits,
                "glyphless_table",
            ),
            (
                a.automatic_composition != b.automatic_composition,
                "composition",
            ),
            (
                a.face_change_count != b.face_change_count,
                "face_change_count",
            ),
            (
                a.char_table_revision != b.char_table_revision,
                "char_table_revision",
            ),
            (a.media_generation != b.media_generation, "media_generation"),
            (a.symbol_values != b.symbol_values, "symbol_values"),
            (self.text != other.text, "text"),
            (self.multibyte != other.multibyte, "multibyte"),
            (self.props != other.props, "props"),
            (self.spans != other.spans, "spans"),
        ]
        .into_iter()
        .find_map(|(differs, name)| differs.then_some(name))
    }
}

/// Encode symbol values the chrome render reads (`header-line-indent-width`)
/// in a stable order. `None` when one is not plain data.
pub(crate) fn encode_symbol_values<'a>(
    values: impl IntoIterator<Item = (&'a str, Value)>,
) -> Option<Box<[u8]>> {
    let mut sorted: Vec<(&str, Value)> = values.into_iter().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let mut encoder = PlainEncoder::default();
    for (name, value) in sorted {
        encoder.bytes(name.as_bytes());
        encoder.value(value, 0).ok()?;
    }
    Some(encoder.finish())
}

/// Encoding gave up: the value is not plain data, or too large.
#[derive(Debug)]
struct NotPlain;

/// Nodes one row may encode before it is declared too large to memoize.
const NODE_BUDGET: usize = 16 * 1024;
/// Nesting depth (cars and string properties) before giving up.
const MAX_DEPTH: u32 = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PropertyScope {
    /// The formatted chrome string: `display` refuses the memo, and the
    /// three mouse-target properties are fingerprinted by name.
    Root,
    /// A string inside a property value: encoded whole.
    Nested,
}

#[derive(Default)]
struct PlainEncoder {
    out: Vec<u8>,
    nodes: usize,
}

mod tag {
    pub(super) const IMMEDIATE: u8 = 1;
    pub(super) const FLOAT: u8 = 2;
    pub(super) const STRING: u8 = 3;
    pub(super) const CONS: u8 = 4;
    pub(super) const LIST_TAIL: u8 = 5;
    pub(super) const VECTOR: u8 = 6;
    pub(super) const RUN: u8 = 7;
    pub(super) const NAME_ONLY: u8 = 8;
    pub(super) const BYTES: u8 = 9;
}

impl PlainEncoder {
    fn finish(self) -> Box<[u8]> {
        self.out.into_boxed_slice()
    }

    fn u64(&mut self, value: u64) {
        self.out.extend_from_slice(&value.to_le_bytes());
    }

    fn bytes(&mut self, bytes: &[u8]) {
        self.out.push(tag::BYTES);
        self.u64(bytes.len() as u64);
        self.out.extend_from_slice(bytes);
    }

    fn visit(&mut self, depth: u32) -> Result<(), NotPlain> {
        self.nodes += 1;
        if self.nodes > NODE_BUDGET || depth > MAX_DEPTH {
            return Err(NotPlain);
        }
        Ok(())
    }

    fn value(&mut self, value: Value, depth: u32) -> Result<(), NotPlain> {
        self.visit(depth)?;
        match value.kind() {
            ValueKind::Nil
            | ValueKind::T
            | ValueKind::Fixnum(_)
            | ValueKind::Symbol(_)
            | ValueKind::Subr(_) => {
                self.out.push(tag::IMMEDIATE);
                self.u64(value.bits() as u64);
                Ok(())
            }
            ValueKind::Float => {
                self.out.push(tag::FLOAT);
                self.u64(value.xfloat().to_bits());
                Ok(())
            }
            ValueKind::String => {
                let string = value.as_lisp_string().ok_or(NotPlain)?;
                self.out.push(tag::STRING);
                self.out.push(u8::from(string.is_multibyte()));
                self.bytes(string.as_bytes());
                self.string_props(string, PropertyScope::Nested, depth + 1)
            }
            ValueKind::Cons => {
                let mut cursor = value;
                while cursor.is_cons() {
                    self.visit(depth)?;
                    self.out.push(tag::CONS);
                    self.value(cursor.cons_car(), depth + 1)?;
                    cursor = cursor.cons_cdr();
                }
                self.out.push(tag::LIST_TAIL);
                self.value(cursor, depth + 1)
            }
            ValueKind::Veclike(VecLikeType::Vector) => {
                let elements = value.as_vector_data().ok_or(NotPlain)?;
                self.out.push(tag::VECTOR);
                self.u64(elements.len() as u64);
                for element in elements.iter() {
                    self.value(*element, depth + 1)?;
                }
                Ok(())
            }
            _ => Err(NotPlain),
        }
    }

    fn string_props(
        &mut self,
        string: &neovm_core::heap_types::LispString,
        scope: PropertyScope,
        depth: u32,
    ) -> Result<(), NotPlain> {
        let intervals = string.intervals();
        if intervals.is_empty() {
            return Ok(());
        }
        let display = Value::symbol("display");
        let name_only = [
            Value::symbol("help-echo"),
            Value::symbol("local-map"),
            Value::symbol("keymap"),
        ];
        let runs = intervals.object_interval_plist_runs_for_char_len(
            neovm_core::buffer::CharLen::new(string.schars()),
        );
        let mut properties: Vec<(Value, Value)> = Vec::new();
        for run in runs {
            let mut plist = run.plist();
            if plist.is_nil() {
                continue;
            }
            self.visit(depth)?;
            self.out.push(tag::RUN);
            self.u64(run.start().get() as u64);
            self.u64(run.end().get() as u64);
            // The formatter merges property lists in no fixed order, and
            // the order means nothing to the render: encode by name, the
            // first binding of a name winning as in `plist-get`.
            properties.clear();
            while plist.is_cons() {
                self.visit(depth)?;
                let name = plist.cons_car();
                let rest = plist.cons_cdr();
                let value = if rest.is_cons() {
                    rest.cons_car()
                } else {
                    Value::NIL
                };
                if !properties.iter().any(|(seen, _)| *seen == name) {
                    properties.push((name, value));
                }
                plist = if rest.is_cons() {
                    rest.cons_cdr()
                } else {
                    Value::NIL
                };
            }
            properties.sort_by_key(|(name, _)| name.bits());
            for &(name, value) in &properties {
                self.value(name, depth + 1)?;
                if scope == PropertyScope::Root {
                    if name == display && !value.is_nil() {
                        return Err(NotPlain);
                    }
                    if name_only.contains(&name) {
                        self.out.push(tag::NAME_ONLY);
                        continue;
                    }
                }
                self.value(value, depth + 1)?;
            }
        }
        Ok(())
    }
}

/// The previous frame's chrome rows with the fingerprints they were rendered
/// from. Carried by a replay whose chrome is evaluated this frame.
#[derive(Clone, Debug)]
pub(crate) struct ChromeMemo {
    chrome: RetainedChrome,
    fingerprints: Arc<[ChromeRowFingerprint]>,
}

/// A memo row that may stand for this frame's render.
pub(crate) struct ChromeMemoHit<'a> {
    pub(crate) index: usize,
    pub(crate) row: &'a MatrixRow,
    pub(crate) snapshot: &'a DisplayRowSnapshot,
}

impl ChromeMemo {
    pub(crate) fn new(chrome: RetainedChrome, fingerprints: Arc<[ChromeRowFingerprint]>) -> Self {
        Self {
            chrome,
            fingerprints,
        }
    }

    /// Every row the memo may install (their face ids must be admitted).
    pub(crate) fn rows(&self) -> impl Iterator<Item = &GlyphRow> {
        self.chrome.rows.iter().map(|(_, row)| row.as_ref())
    }

    /// Why `fingerprint` misses, for the diagnostics.
    pub(crate) fn miss_reason(&self, fingerprint: &ChromeRowFingerprint) -> &'static str {
        self.fingerprints
            .iter()
            .find(|previous| previous.kind() == fingerprint.kind())
            .map_or("no previous row of this kind", |previous| {
                previous
                    .first_difference(fingerprint)
                    .unwrap_or("no retained row")
            })
    }

    /// The previous row for `fingerprint`'s kind, when it was rendered from
    /// an equal fingerprint.
    pub(crate) fn hit(&self, fingerprint: &ChromeRowFingerprint) -> Option<ChromeMemoHit<'_>> {
        let role = chrome_row_role(fingerprint.kind());
        self.fingerprints
            .iter()
            .find(|previous| previous.kind() == fingerprint.kind())
            .filter(|previous| *previous == fingerprint)?;
        let (index, row) = self.chrome.rows.iter().find(|(_, row)| row.role == role)?;
        let snapshot = self
            .chrome
            .row_snapshots
            .iter()
            .find(|snapshot| snapshot.row as usize == *index)?;
        Some(ChromeMemoHit {
            index: *index,
            row,
            snapshot,
        })
    }
}

// Which chrome rows each window rendered this layout, with the fingerprint of
// each; read once at commit into the retained matrix. Keyed like
// `display_status_line`'s generation record.
thread_local! {
    static FINGERPRINT_RECORD: std::cell::RefCell<
        rustc_hash::FxHashMap<i64, Vec<ChromeRowFingerprint>>,
    > = std::cell::RefCell::new(rustc_hash::FxHashMap::default());
}

/// Forget the previous layout's record (start of a frame layout).
pub(crate) fn reset_fingerprint_record() {
    FINGERPRINT_RECORD.with(|record| record.borrow_mut().clear());
}

/// Record the fingerprint `window_id`'s `kind` row was rendered from, or that
/// it had none (`None`: the row cannot be memoized).
pub(crate) fn record_fingerprint(
    window_id: i64,
    kind: WindowChromeKind,
    fingerprint: Option<ChromeRowFingerprint>,
) {
    FINGERPRINT_RECORD.with(|record| {
        let mut record = record.borrow_mut();
        let rows = record.entry(window_id).or_default();
        rows.retain(|row| row.kind() != kind);
        if let Some(fingerprint) = fingerprint {
            rows.push(fingerprint);
        }
    });
}

/// The fingerprints `window_id`'s chrome was rendered from this layout.
pub(crate) fn recorded_fingerprints(window_id: i64) -> Option<Arc<[ChromeRowFingerprint]>> {
    FINGERPRINT_RECORD.with(|record| {
        record
            .borrow()
            .get(&window_id)
            .filter(|rows| !rows.is_empty())
            .map(|rows| Arc::from(rows.as_slice()))
    })
}

#[cfg(test)]
#[path = "tests/chrome_memo_test.rs"]
mod tests;
