//! Bridge between neovm-core data types and the layout engine.
//!
//! Provides functions to build `WindowParams` and `FrameParams` from
//! the Rust Context's state, replacing C FFI data sources.
//!
//! | Knob | Default | Values | Gate |
//! | --- | --- | --- | --- |
//! | `NEOMACS_LAYOUT_LINE_COUNT` | `on` | `off`, `on`, `verify` | Count source newlines with `memchr` over the same backend chunks; verify compares with the scalar scan. |
//! | `NEOMACS_LAYOUT_PROPERTY_KEYS_INLINE` | `on` | `off`; `on`/`1`/`true`/`yes` | Store the canonical key directly; canonical-only ON avoids allocation, and aliases retain ordered heap storage. |
//! The numeric mode is read once per process. Indexed counts and byte-range
//! clamping precede the fallback scan in every mode. Concurrent readers share
//! only the initialized numeric mode; each scan borrows its own backend view
//! and creates no buffer-specific or Lisp-state cache.

use neovm_core::emacs_core::eval::VariableView;
use neovm_core::face::realization::BasicFace;
use std::cmp::Ordering;

mod borrowed_buffer;
mod buffer_snapshot;
pub(crate) use borrowed_buffer::BorrowedLayoutBuffer;
pub(crate) use buffer_snapshot::LayoutBufferSnapshot;
use buffer_snapshot::capture_string_composition_rules;

#[cfg(test)]
#[path = "neovm_bridge/tests/property_keys_support.rs"]
pub(crate) mod property_keys_test_support;

#[cfg(test)]
#[path = "neovm_bridge/tests/property_keys_canonical_test.rs"]
mod property_keys_canonical_tests;

#[cfg(test)]
#[path = "neovm_bridge/tests/property_keys_alias_test.rs"]
mod property_keys_alias_tests;

mod property_keys;
use property_keys::PropertyKeyOrder;

mod face_colors;
use face_colors::{FaceColorAttributes, FaceColorState, TtyPaint, TtyPaintKey};
mod face_sources;
use face_sources::{FaceTableRoots, RootedFace};

use neovm_core::buffer::{
    Buffer, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange, LispCharPos1,
    buffer::{BufferSlotInfo, lookup_buffer_slot_by_sym_id},
    overlay::{OverlayList, OverlayPropertyAtPoint, OverlayPropertyFilter},
};
use neovm_core::emacs_core::effect_profile::{
    EffectScope, effect_name_from_lisp, effect_operation_from_lisp,
};
use neovm_core::emacs_core::image_catalog::image_scale_environment;
use neovm_core::emacs_core::invisibility::{Invisibility, text_prop_means_invisible};
use neovm_core::emacs_core::plist::plist_get;
use neovm_core::emacs_core::textprop::{DirectCharProperties, resolve_effective_char_property};
use neovm_core::emacs_core::value::{ValueKind, eq_value, list_to_vec};
use neovm_core::emacs_core::{Context, Value};
use neovm_core::face::{
    BoxBorder as NeoBoxBorder, BoxStyle as NeoBoxStyle, Color as NeoColor, Face as NeoFace,
    FaceDecoration, FaceHeight, FaceTable, FontWeight, UnderlinePosition as NeoUnderlinePosition,
    UnderlineStyle as NeoUnderlineStyle,
};
pub(crate) use neovm_core::window::WindowLayoutVariable as LayoutVar;
use neovm_core::window::{
    CursorTypeSymbol, Frame, FrameDivider, FrameId, VerticalScrollBarType, Window, WindowEndState,
    WindowId, resolve_window_scroll_bar_geometry,
};

use super::types::{
    DisplayLineNumbersMode, FrameParams, LineWrapMode, NobreakDisplayMode, VisualCursorSpec,
    WindowCursorRole, WindowKind, WindowParams,
};
use crate::coords::{
    clamped_lisp_charpos_to_layout_i64, layout_char_pos_from_i64, layout_emacs_byte_pos_from_i64,
    lisp_char_pos_to_layout_i64, lisp_charpos_to_layout_char_pos,
};
use crate::display_face_policy::BaseFacePolicy;
use crate::display_item::{DisplaySourceMappedFaceRun, GlyphlessAcronym, GlyphlessMethod};
use crate::display_origin::DisplayOrigin;
use crate::font::frame_metrics::{FaceSizeCandidate, FrameFontDomain, GraphicFontSizePx};
use crate::font::sizing::FontSizing;
use neomacs_display_protocol::TerminalColor;
use neomacs_display_protocol::cursor::{CursorBarWidth, CursorKind, CursorSpec};
use neomacs_display_protocol::face::{BasicFaceId, BoxLineWidth};
use neomacs_display_protocol::types::{FaceId, Rect};
use neomacs_display_protocol::{EffectsConfig, PresentedResizeEdge};
use strum::{EnumString, IntoStaticStr};

#[derive(Clone, Copy, Debug, PartialEq, Eq, EnumString, IntoStaticStr)]
#[strum(serialize_all = "kebab-case")]
enum DisplayLineNumbersSymbol {
    Relative,
    Visual,
}

impl DisplayLineNumbersSymbol {
    fn from_symbol_name(name: &str) -> Option<Self> {
        name.parse().ok()
    }

    #[cfg(test)]
    fn name(self) -> &'static str {
        self.into()
    }
}

impl DisplayLineNumbersMode {
    fn from_lisp_value(value: Option<Value>) -> Self {
        match value {
            // GNU dispatches only the two special symbols
            // (`xdisp.c` maybe_produce_line_number: `EQ (..., Qrelative)`
            // and `EQ (..., Qvisual)`); every other non-nil value — `t`,
            // the `'absolute` a user init sets, anything else truthy — is
            // ABSOLUTE line numbers. Rejecting unrecognized symbols here
            // switched the whole gutter off for
            // `(setq display-line-numbers-type 'absolute)` (issue #441).
            Some(value) if value.is_truthy() => {
                match DisplayLineNumbersSymbol::from_symbol_name(
                    value.as_bare_symbol_name().unwrap_or(""),
                ) {
                    Some(DisplayLineNumbersSymbol::Relative) => Self::Relative,
                    Some(DisplayLineNumbersSymbol::Visual) => Self::Visual,
                    // GNU's absolute: `t`, `'absolute`, any other truthy value.
                    None => Self::Absolute,
                }
            }
            _ => Self::Off,
        }
    }
}

pub(crate) trait LayoutBufferView {
    // Raw, frame-less buffer readers retain graphical classification. Window
    // snapshots must supply their actual frame target at construction.
    fn layout_display_target(&self) -> crate::display_property::DisplayPropertyTarget {
        crate::display_property::DisplayPropertyTarget::Graphical
    }
    /// Buffer-selected regexp tables for automatic composition of strings
    /// displayed in this window. Unibyte strings are excluded by their source,
    /// independently of the displayed buffer's own multibyte setting.
    fn layout_string_composition_rules(
        &self,
    ) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
        None
    }
    /// The evaluated `(when FORM . SPEC)` conditions of the walk this view
    /// serves; `structural()` for a view nothing evaluated for.
    fn layout_display_when_conditions(&self) -> crate::display_when::DisplayWhenConditions {
        crate::display_when::DisplayWhenConditions::structural()
    }
    fn layout_is_multibyte(&self) -> bool;
    fn layout_buffer_local_value(&self, var: LayoutVar) -> Option<Value>;
    fn layout_point_min_emacs_byte_pos(&self) -> EmacsBytePos;
    fn layout_point_max_emacs_byte_pos(&self) -> EmacsBytePos;
    fn layout_point_max_char_pos(&self) -> CharPos0;
    /// Readable context for complete display elements. A measurement stop can
    /// fall inside a composition; actual buffer narrowing still bounds it.
    fn layout_measurement_context_end(&self) -> Option<CharPos0> {
        None
    }
    fn layout_total_emacs_byte_len(&self) -> EmacsByteLen;
    fn layout_char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos;
    fn layout_emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0;
    fn layout_copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>);
    fn layout_try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E>;
    fn layout_emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8>;
    /// `\n`s in RANGE from the buffer's text line index, or `None` when the
    /// index does not serve this count and the caller scans.
    fn layout_indexed_newline_count(&self, _range: EmacsByteRange) -> Option<usize> {
        None
    }
    fn layout_text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value>;
    /// Return PROPERTY from CATEGORY's symbol plist as captured for this
    /// immutable layout view. Live-buffer test adapters have no evaluator
    /// symbol environment and therefore use the default empty implementation.
    fn layout_category_symbol_property(&self, _category: Value, _property: Value) -> Option<Value> {
        None
    }
    fn layout_next_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos>;
    /// Next position after `pos` where the single text property `name` changes
    /// (compared by `eq`), ignoring changes to any other property.  Mirrors the
    /// text-property half of GNU `next_single_char_property_change`.
    fn layout_next_single_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos>;
    /// Bounded variant for display scans that only need the next boundary
    /// within the visible window (invisible/display): stops at `limit` and
    /// returns it as a soft boundary if `name` has not changed by then. Must
    /// NOT be used where the exact extent matters (e.g. mouse-face highlight).
    fn layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos>;
    fn layout_previous_single_text_prop_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos>;
    fn layout_overlays(&self) -> &OverlayList;
    /// Automatic-composition span whose first character is `pos`.
    ///
    /// These spans are compiled from the live Lisp rule table when the
    /// immutable layout snapshot is built.  A live-buffer adapter deliberately
    /// has none: it lacks the evaluator environment needed to resolve rules.
    fn layout_automatic_composition_starting_at(&self, _pos: CharPos0) -> Option<CharRange> {
        None
    }
    fn layout_next_automatic_composition_start(
        &self,
        _pos: CharPos0,
        _limit: CharPos0,
    ) -> Option<CharPos0> {
        None
    }
}

/// The buffer-owned input needed while resolving named faces on display
/// strings.  Capturing this value keeps the display-source pipeline independent
/// of the much wider, non-object-safe [`LayoutBufferView`] interface.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BufferFaceRemapping {
    alist: Option<Value>,
}

impl BufferFaceRemapping {
    pub(crate) fn capture(buffer: &impl LayoutBufferView) -> Self {
        Self {
            alist: buffer.layout_buffer_local_value(LayoutVar::FaceRemappingAlist),
        }
    }

    fn alist(self) -> Option<Value> {
        self.alist
    }

    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self { alist: None }
    }

    #[cfg(test)]
    pub(crate) fn for_alist(alist: Option<Value>) -> Self {
        Self { alist }
    }
}

/// ADMITTED-WORK counters, read and reset once per accepted frame into
/// [`crate::incremental_layout::LayoutStats`].
///
/// Process-wide atomics, matching the row-route telemetry next door: the
/// buffer snapshot is built far from the engine that owns the stats, and
/// threading a counter through every constructor would be worse than the
/// problem. Counting only -- nothing here decides anything.
pub(crate) static SNAPSHOTS_BUILT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);
/// Chrome rows installed from a retained matrix rather than walked.
pub(crate) static CHROME_ROWS_REUSED: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// Take and reset both counters.
pub(crate) fn take_snapshot_work() -> (usize, usize, usize) {
    use std::sync::atomic::Ordering::Relaxed;
    (
        SNAPSHOTS_BUILT.swap(0, Relaxed),
        neovm_core::emacs_core::composite::take_bytes_scanned(),
        CHROME_ROWS_REUSED.swap(0, Relaxed),
    )
}

pub(crate) fn window_string_composition_rules(
    evaluator: &neovm_core::emacs_core::Context,
    window_id: neovm_core::window::WindowId,
) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
    let buffer_id = evaluator
        .frame_manager()
        .lookup_window(window_id)?
        .buffer_id()?;
    capture_string_composition_rules(
        evaluator.buffer_manager().get(buffer_id)?,
        evaluator.variables(),
        evaluator.symbol_mode(),
    )
}

pub(crate) fn current_string_composition_rules(
    evaluator: &neovm_core::emacs_core::Context,
) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
    capture_string_composition_rules(
        evaluator.buffer_manager().current_buffer()?,
        evaluator.variables(),
        evaluator.symbol_mode(),
    )
}

struct LayoutVarInfo {
    slot: Option<&'static BufferSlotInfo>,
    /// Whether [`LayoutBufferSnapshot`] captures this variable's DEFAULT
    /// value. Deliberately the historical curated subset: the snapshot falls
    /// back to the default only for these (e.g. `char-property-alias-alist`
    /// is a plain global that indent-bars mutates), while the rest — and the
    /// live-Buffer impl for ALL variables — return None without a local
    /// binding or slot. Widening this set changes display semantics; do it
    /// as its own change, not as a side effect of plumbing.
    captures_default: bool,
}

fn layout_var_info(var: LayoutVar) -> &'static LayoutVarInfo {
    use std::sync::OnceLock;
    use strum::VariantArray;
    static INFOS: OnceLock<Vec<LayoutVarInfo>> = OnceLock::new();
    &INFOS.get_or_init(|| {
        LayoutVar::VARIANTS
            .iter()
            .map(|var| {
                let sym_id = var.sym_id();
                LayoutVarInfo {
                    slot: lookup_buffer_slot_by_sym_id(sym_id),
                    captures_default: matches!(
                        var,
                        LayoutVar::AutoCompositionFunction
                            | LayoutVar::AutoCompositionMode
                            | LayoutVar::CharPropertyAliasAlist
                            | LayoutVar::DefaultTextProperties
                            | LayoutVar::DisplayFillColumnIndicator
                            | LayoutVar::DisplayFillColumnIndicatorCharacter
                            | LayoutVar::DisplayFillColumnIndicatorColumn
                            | LayoutVar::DisplayLineNumbers
                            | LayoutVar::DisplayLineNumbersCurrentAbsolute
                            | LayoutVar::DisplayLineNumbersMajorTick
                            | LayoutVar::DisplayLineNumbersMinorTick
                            | LayoutVar::DisplayLineNumbersOffset
                            | LayoutVar::DisplayLineNumbersWiden
                            | LayoutVar::DisplayLineNumbersWidth
                            | LayoutVar::FaceRemappingAlist
                            | LayoutVar::GlyphlessCharDisplay
                            | LayoutVar::LinePrefix
                            | LayoutVar::NeomacsCursorEffect
                            | LayoutVar::NeomacsVisualCursors
                            | LayoutVar::ShowTrailingWhitespace
                            | LayoutVar::StandardDisplayTable
                            | LayoutVar::TabStopList
                            | LayoutVar::WrapPrefix
                    ),
                }
            })
            .collect()
    })[var as usize]
}

impl LayoutBufferView for Buffer {
    fn layout_is_multibyte(&self) -> bool {
        self.get_multibyte()
    }

    fn layout_buffer_local_value(&self, var: LayoutVar) -> Option<Value> {
        self.buffer_local_value_id(var.sym_id())
    }

    fn layout_point_min_emacs_byte_pos(&self) -> EmacsBytePos {
        self.point_min_emacs_byte_pos()
    }

    fn layout_point_max_emacs_byte_pos(&self) -> EmacsBytePos {
        self.point_max_emacs_byte_pos()
    }

    fn layout_point_max_char_pos(&self) -> CharPos0 {
        self.point_max_char_pos()
    }

    fn layout_total_emacs_byte_len(&self) -> EmacsByteLen {
        self.total_emacs_byte_len()
    }

    fn layout_char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos {
        self.char_pos_to_emacs_byte_pos_clamped(charpos)
    }

    fn layout_emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0 {
        self.emacs_byte_pos_to_char_pos_clamped(bytepos)
    }

    fn layout_copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.copy_emacs_byte_range_to(range, out);
    }

    fn layout_try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.try_for_each_emacs_byte_range_chunk(range, f)
    }

    fn layout_emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.emacs_byte_at_pos(pos)
    }

    fn layout_indexed_newline_count(&self, range: EmacsByteRange) -> Option<usize> {
        // The layout's counts are the line-number gutter's, repeated for
        // every redisplay: large enough ranges may build the index.
        self.indexed_newline_count(range, neovm_core::buffer::LineIndexBuild::IfLarge)
    }

    fn layout_text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value> {
        self.text_props_get_property_at_emacs_byte_pos(pos, name)
    }

    fn layout_next_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text_props_next_change_after_emacs_byte_pos(pos)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text_props_next_single_change_after_emacs_byte_pos(pos, name)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text_props_next_single_change_after_emacs_byte_pos_bounded(pos, name, limit)
    }

    fn layout_previous_single_text_prop_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text_props_previous_single_change_before_emacs_byte_pos(pos, name)
    }

    fn layout_overlays(&self) -> &OverlayList {
        self.overlays()
    }
}

pub(crate) fn buffer_local_value<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    var: LayoutVar,
) -> Option<Value> {
    // GNU `buffer_local_value` (`buffer.c:1359-1413`) returns a buffer's
    // local binding when present and otherwise falls through to the default
    // value.  Layout uses this helper for display variables such as
    // `display-line-numbers-current-absolute`; using the local-only predicate
    // here silently loses global/default display state.
    buffer.layout_buffer_local_value(var)
}

fn effective_buffer_value(
    buffer: &Buffer,
    vars: VariableView<'_>,
    var: LayoutVar,
) -> Option<Value> {
    buffer
        .buffer_local_value_id(var.sym_id())
        .or_else(|| vars.value(var.sym_id()))
}

fn frame_parameter_int(frame: &Frame, name: &str, default: i64) -> i64 {
    frame
        .parameter(name)
        .and_then(|v| v.as_int())
        .unwrap_or(default)
}

/// Resolve a face's foreground as an Emacs packed pixel (`0x00RRGGBB`).
///
/// Mirrors the encoding GNU's display layer uses for face colors and
/// matches `NeoColor::from_pixel` (R<<16 | G<<8 | B) which the consumers
/// decode through. `resolve` follows `:inherit` chains, so faces like
/// `nobreak-space` (`:inherit escape-glyph`) report the inherited color.
/// `fallback` is returned when the face leaves its foreground unspecified.
fn face_fg_pixel(face_table: &FaceTable, name: &str, fallback: u32) -> u32 {
    face_table
        .resolve(name)
        .foreground
        .map(|c| (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32)
        .unwrap_or(fallback)
}

/// Resolve a face's background as an Emacs packed pixel (`0x00RRGGBB`).
///
/// Sibling of [`face_fg_pixel`]; see it for the encoding and inheritance
/// semantics.
fn face_bg_pixel(face_table: &FaceTable, name: &str, fallback: u32) -> u32 {
    face_table
        .resolve(name)
        .background
        .map(|c| (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32)
        .unwrap_or(fallback)
}

/// Build `FrameParams` from a neovm-core `Frame`, reading default face
/// colors from the face table.
pub fn frame_params_from_neovm(
    frame: &Frame,
    face_table: &FaceTable,
    vars: VariableView<'_>,
) -> FrameParams {
    // Read default face background from face table
    let default_face = face_table.get("default");
    let bg = default_face
        .and_then(|f| f.background)
        .map(|c| (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32)
        .unwrap_or(0x00FFFFFF); // white fallback
    let fg = default_face
        .and_then(|f| f.foreground)
        .map(|c| (c.r as u32) << 16 | (c.g as u32) << 8 | c.b as u32)
        .unwrap_or(0x00000000); // black fallback

    FrameParams {
        width: frame.width as f32,
        height: frame.height as f32,
        menu_bar_height: frame.menu_bar_height as f32,
        tool_bar_height: frame.tool_bar_height as f32,
        compact_bar_height: frame.compact_bar_height as f32,
        tab_bar_height: frame.tab_bar_height as f32,
        char_width: frame.char_width,
        char_height: frame.char_height,
        font_pixel_size: frame.font_pixel_size,
        image_scale_environment: image_scale_environment(frame, vars),
        window_system: frame.effective_window_system().is_some(),
        background: bg,
        vertical_border_fg: face_fg_pixel(face_table, "vertical-border", fg),
        zero_width_vertical_border_edge: match frame
            .known_parameter(neovm_core::window::FrameParam::VerticalScrollBars)
            .and_then(|value| VerticalScrollBarType::from_symbol_value(&value))
        {
            Some(VerticalScrollBarType::Left) => PresentedResizeEdge::Leading,
            Some(VerticalScrollBarType::Right) | None => PresentedResizeEdge::Trailing,
        },
        right_divider_width: frame.effective_divider_width(FrameDivider::Right) as i32,
        bottom_divider_width: frame.effective_divider_width(FrameDivider::Bottom) as i32,
        divider_fg: face_fg_pixel(face_table, "window-divider", fg),
        divider_first_fg: face_fg_pixel(face_table, "window-divider-first-pixel", fg),
        divider_last_fg: face_fg_pixel(face_table, "window-divider-last-pixel", fg),
    }
}

/// Helper: extract an integer buffer-local variable.
pub(crate) fn buffer_local_int<B: LayoutBufferView>(
    buffer: &B,
    var: LayoutVar,
    default: i64,
) -> i64 {
    match buffer_local_value(buffer, var) {
        Some(v) if v.is_fixnum() => v.as_fixnum().unwrap(),
        _ => default,
    }
}

fn effective_buffer_int(
    buffer: &Buffer,
    vars: VariableView<'_>,
    var: LayoutVar,
    default: i64,
) -> i64 {
    match effective_buffer_value(buffer, vars, var) {
        Some(v) if v.is_fixnum() => v.as_fixnum().unwrap(),
        _ => default,
    }
}

/// Helper: extract a boolean buffer-local variable (nil = false, anything else = true).
pub(crate) fn buffer_local_bool<B: LayoutBufferView>(buffer: &B, var: LayoutVar) -> bool {
    match buffer_local_value(buffer, var) {
        Some(v) if v.is_nil() => false,
        None => false,
        Some(_) => true,
    }
}

fn effective_buffer_bool(buffer: &Buffer, vars: VariableView<'_>, var: LayoutVar) -> bool {
    match effective_buffer_value(buffer, vars, var) {
        Some(v) if v.is_nil() => false,
        None => false,
        Some(_) => true,
    }
}

pub(crate) fn window_parameter_by_name(window: &Window, name: &str) -> Option<Value> {
    window
        .parameters()
        .iter()
        .find(|(key, _)| key.as_bare_symbol_name() == Some(name))
        .map(|(_, value)| *value)
}

fn global_bool(vars: VariableView<'_>, name: &str) -> bool {
    vars.value_by_name(name)
        .is_some_and(|value| !value.is_nil())
}

fn effective_nobreak_char_display(buffer: &Buffer, vars: VariableView<'_>) -> NobreakDisplayMode {
    // `nobreak-char-display` is a `DEFVAR_LISP`, but GNU reads `Vnobreak_char_display`
    // while displaying buffer text (xdisp.c:8522, with the window's buffer current),
    // so a buffer-local binding takes effect. Read buffer-local-then-global, not the
    // raw global.
    match effective_buffer_value(buffer, vars, LayoutVar::NobreakCharDisplay) {
        Some(value) if value.is_nil() => NobreakDisplayMode::Literal,
        Some(value) if value.is_t() => {
            if global_bool(vars, "nobreak-char-ascii-display") {
                NobreakDisplayMode::HighlightAscii
            } else {
                NobreakDisplayMode::HighlightOriginal
            }
        }
        Some(_) => NobreakDisplayMode::Escape,
        None => NobreakDisplayMode::Literal,
    }
}

fn frame_total_cols(frame: &Frame) -> i64 {
    frame
        .parameter("width")
        .and_then(|value| value.as_int())
        .unwrap_or(frame.columns() as i64)
}

fn window_total_cols(window: &Window, char_width: f32) -> i64 {
    let width = window.bounds().width;
    if char_width > 0.0 {
        (width / char_width) as i64
    } else {
        0
    }
}

fn effective_wrap_mode(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    vars: VariableView<'_>,
    hscroll: usize,
) -> LineWrapMode {
    if effective_buffer_bool(buffer, vars, LayoutVar::TruncateLines) {
        return LineWrapMode::Truncate;
    }

    // GNU `xdisp.c:init_iterator` only enables wrapping when the
    // window is not horizontally scrolled.
    if hscroll != 0 {
        return LineWrapMode::Truncate;
    }

    let total_cols = window_total_cols(window, frame.char_width);
    let frame_cols = frame_total_cols(frame);

    if total_cols >= frame_cols {
        return LineWrapMode::Wrap;
    }

    let truncate =
        match effective_buffer_value(buffer, vars, LayoutVar::TruncatePartialWidthWindows) {
            Some(value) if value.is_nil() => false,
            Some(value) if value.is_fixnum() => total_cols < value.as_fixnum().unwrap(),
            Some(_) => true,
            None => false,
        };

    if truncate {
        LineWrapMode::Truncate
    } else {
        LineWrapMode::Wrap
    }
}

fn chrome_face_pixel_height(
    face: &ResolvedFace,
    fallback_char_height: f32,
    device_scale: neomacs_display_protocol::DeviceScale,
) -> f32 {
    // GNU Emacs frame.c:1184-1185 — non-window (TTY) frames have
    //   f->column_width = 1;
    //   f->line_height  = 1;
    // and chrome rows (mode-line, header-line, tab-line) are exactly
    // one character cell tall. Face font_line_height is a GUI pixel
    // measurement and must not contribute to row sizing on a TTY
    // frame: `fallback_char_height` is set to 1.0 by
    // `bootstrap_buffers` (main.rs:1691-1694) when the frame is a
    // TTY, so detect the TTY context by the 1.0-cell marker and
    // return the cell height directly.
    //
    // Without this early return, a mode-line face with a non-zero
    // `font_line_height` (e.g. 3 from the realized Hack font under
    // cosmic-text) produced a 3-row-tall mode-line region on TTY.
    // The mode-line text painted on the first row and the remaining
    // two rows rendered as blank padding, which looked like the
    // echo area having "3 lines" instead of GNU's single row.
    if fallback_char_height <= 1.0 {
        return fallback_char_height.max(1.0);
    }
    let line_height = if face.font_line_height > 0.0 {
        face.font_line_height.ceil()
    } else {
        fallback_char_height.ceil()
    };
    let box_pixels = if face.box_type != 0 {
        2.0 * face
            .box_line_width
            .logical_geometry(device_scale)
            .row_expansion_per_edge()
            .get()
    } else {
        0.0
    };
    (line_height + box_pixels).max(1.0)
}

pub(crate) fn buffer_local_list_values<B: LayoutBufferView>(
    buffer: &B,
    var: LayoutVar,
) -> Vec<Value> {
    // `list_to_vec' takes `&Value'; feed the borrowed form since
    // `buffer_local_value' returns the `Copy' `Value' by value.
    buffer_local_value(buffer, var)
        .and_then(|v| list_to_vec(&v))
        .unwrap_or_default()
}

/// Resolve a LOGICAL fringe indicator (`empty-line`, `truncation`,
/// `continuation`, `up`, `down`, …) to a concrete fringe-bitmap registry index,
/// honoring `fringe-indicator-alist`. Direct port of GNU
/// `get_logical_fringe_bitmap` (`src/fringe.c`).
///
/// `right_p` selects the right vs left element; `partial_p` selects the
/// partial vs full element. The alist `cdr` for an indicator is either:
///   * a BARE SYMBOL — used for every (left/right/partial) case, or
///   * a list `(LEFT RIGHT [PARTIAL-LEFT PARTIAL-RIGHT])` — indexed by
///     `right_p ? (partial_p ? 3 : 1) : (partial_p ? 2 : 0)`, falling back to
///     the non-partial element (`ix1 = right_p`) when a partial element is
///     absent.
///
/// An element equal to `t` means "no bitmap here" (skip to the fallback). The
/// buffer-local alist is consulted first, then the global/default value.
///
/// Returns `None` when no bitmap applies (the `t`/missing/unregistered cases),
/// matching GNU's `NO_FRINGE_BITMAP`.
pub(crate) fn resolve_fringe_indicator_bitmap_index<B: LayoutBufferView>(
    buffer: &B,
    ctx: &Context,
    logical_sym: Value,
    right_p: bool,
    partial_p: bool,
) -> Option<u16> {
    let ix1 = usize::from(right_p);
    let ix2 = ix1 + if partial_p { 2 } else { 0 };

    // Look up the cdr of (LOGICAL . SPEC) in an alist value.
    fn assq_cdr(alist: Value, key: Value) -> Option<Value> {
        let mut cursor = alist;
        while cursor.is_cons() {
            let entry = cursor.cons_car();
            if entry.is_cons() && eq_value(&entry.cons_car(), &key) {
                return Some(entry.cons_cdr());
            }
            cursor = cursor.cons_cdr();
        }
        None
    }

    // From a resolved cdr SPEC, pick the bitmap SYMBOL for this (right/partial)
    // case. `t` (or out-of-range) yields `None`; a bare symbol is used directly.
    // When `partial_p` and the partial element is absent, fall back to `ix1`.
    fn pick_bitmap_symbol(spec: Value, ix1: usize, ix2: usize, partial_p: bool) -> Option<Value> {
        if spec.is_nil() {
            // GNU: a present-but-nil cdr means NO_FRINGE_BITMAP for the whole
            // indicator (handled by the caller treating `None` as "stop").
            return None;
        }
        if !spec.is_cons() {
            // BARE SYMBOL: used for all cases unless it is `t`.
            return (spec.bits() != Value::T.bits()).then_some(spec);
        }
        let items = list_to_vec(&spec)?;
        // Prefer the partial element when requested and present & not `t`.
        if partial_p
            && let Some(elem) = items.get(ix2).copied()
            && elem.bits() != Value::T.bits()
        {
            return Some(elem);
        }
        // Non-partial (or partial-absent) fallback: the ix1 element.
        let elem = items.get(ix1).copied()?;
        (elem.bits() != Value::T.bits()).then_some(elem)
    }

    let registry = ctx.fringe_bitmap_registry();
    let resolve_index = |sym: Value| -> Option<u16> {
        let sym_id = sym.as_bare_symbol_id()?;
        let index = registry.index_of(sym_id)?;
        u16::try_from(index).ok()
    };

    // 1. Buffer-local `fringe-indicator-alist`.
    let local = buffer.layout_buffer_local_value(LayoutVar::FringeIndicatorAlist);
    // 2. Global/default value (GNU `BVAR(&buffer_defaults, fringe_indicator_alist)`).
    // `fringe-indicator-alist` is a forwarded per-buffer slot, so its default
    // lives in `buffer_defaults` — the obarray value cell is always nil.
    let global = ctx.buffer_default_value("fringe-indicator-alist");

    // Try the buffer-local binding first. GNU only falls through to the global
    // value when the local lookup yields no usable element (a `t` element or a
    // missing partial spec); a present non-`t` element short-circuits.
    if let Some(local) = local.filter(|v| !v.is_nil())
        && let Some(cdr) = assq_cdr(local, logical_sym)
    {
        if cdr.is_nil() {
            // Explicit nil cdr => NO_FRINGE_BITMAP for this indicator.
            return None;
        }
        if let Some(sym) = pick_bitmap_symbol(cdr, ix1, ix2, partial_p) {
            return resolve_index(sym);
        }
    }

    // Fall back to the global/default alist.
    let global = global.filter(|v| !v.is_nil())?;
    let cdr = assq_cdr(global, logical_sym)?;
    if cdr.is_nil() {
        return None;
    }
    let sym = pick_bitmap_symbol(cdr, ix1, ix2, partial_p)?;
    resolve_index(sym)
}

#[cfg(test)]
pub(crate) fn buffer_display_line_numbers_mode<B: LayoutBufferView>(
    buffer: &B,
) -> DisplayLineNumbersMode {
    DisplayLineNumbersMode::from_lisp_value(buffer_local_value(
        buffer,
        LayoutVar::DisplayLineNumbers,
    ))
}

fn buffer_fill_column_indicator(buffer: &Buffer, vars: VariableView<'_>) -> Option<(i32, char)> {
    // GNU `fill_column_indicator_column` in xdisp.c enables the indicator only
    // when `display-fill-column-indicator` is non-nil, the indicator character
    // satisfies CHARACTERP, and the effective column is a nonnegative integer.
    // Read the EFFECTIVE value (buffer-local, else global/default) for all four:
    // `display-fill-column-indicator-column` in particular defaults to `t` (use
    // fill-column) and is rarely set locally, so a buffer-local-only read returns
    // None and disables the indicator even when the mode is on.
    if !effective_buffer_bool(buffer, vars, LayoutVar::DisplayFillColumnIndicator) {
        return None;
    }

    let character_value =
        effective_buffer_value(buffer, vars, LayoutVar::DisplayFillColumnIndicatorCharacter)?;
    if !character_value.is_char() {
        return None;
    }
    let character = character_value.as_char()?;

    let column_value =
        match effective_buffer_value(buffer, vars, LayoutVar::DisplayFillColumnIndicatorColumn) {
            Some(value) if value.bits() == Value::T.bits() => {
                effective_buffer_value(buffer, vars, LayoutVar::FillColumn)
            }
            value => value,
        }?;
    let column = column_value.as_fixnum()?;
    if column < 0 || column > i32::MAX as i64 {
        return None;
    }

    Some((column as i32, character))
}

/// Index of the selective-display / invisible ellipsis slot in a display
/// table's extra slots (`DISP_INVIS_VECTOR`, `disptab.h:35` -> `extras[4]`).
const DISP_INVIS_VECTOR_SLOT: usize = 4;

/// The decoded text and run-length encoded Lisp faces carried by a
/// display-table glyph vector.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BufferDisplayTableGlyphs {
    pub(crate) text: String,
    pub(crate) lisp_face_runs: Box<[DisplaySourceMappedFaceRun]>,
}

/// `make-glyph-code` (`disp-table.el`) encodes a glyph as either a bare
/// character fixnum, a fixnum packing the face id above the low 22 char bits,
/// or a `(char . face-id)` cons when the face id needs more than 6 bits.
fn glyph_code_parts(glyph: Value) -> Option<(char, Option<neovm_core::face::LispFaceId>)> {
    let (code, face_id) = if glyph.is_cons() {
        (glyph.cons_car().as_fixnum()?, glyph.cons_cdr().as_fixnum())
    } else {
        let code = glyph.as_fixnum()?;
        (code, Some(code >> 22))
    };
    let ch = char::from_u32((code as u64 & 0x3F_FFFF) as u32)?;
    Some((
        ch,
        face_id.and_then(neovm_core::face::LispFaceId::glyph_override),
    ))
}

fn glyph_code_char(glyph: Value) -> Option<char> {
    glyph_code_parts(glyph).map(|(ch, _)| ch)
}

/// The active display table for a buffer: `buffer-display-table`, else
/// `standard-display-table` (GNU `disp_char_vector`'s `dp` argument selection).
/// Per-window display tables are not yet wired into the layout buffer view.
fn active_buffer_display_table<B: LayoutBufferView + ?Sized>(buffer: &B) -> Option<Value> {
    buffer_local_value(buffer, LayoutVar::BufferDisplayTable)
        .filter(|v| !v.is_nil())
        .or_else(|| buffer_local_value(buffer, LayoutVar::StandardDisplayTable))
        .filter(|v| !v.is_nil())
}

/// Whether an active display table (buffer-local or standard) exists for
/// `buffer`. The row-acquisition classifier uses this: any active table can
/// remap arbitrary chars, so such buffers stay on the buffer pipeline.
pub(crate) fn buffer_has_active_display_table<B: LayoutBufferView + ?Sized>(buffer: &B) -> bool {
    active_buffer_display_table(buffer).is_some()
}

/// Whether the active display table maps `ch` to a glyph vector. This keeps
/// text-run scanning on the cheap presence path; decoding is deferred until
/// the mapped item is emitted.
pub(crate) fn buffer_display_table_glyph_vector_p<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    character: neovm_core::emacs_core::emacs_char::EmacsChar,
) -> bool {
    let Some(table) = active_buffer_display_table(buffer) else {
        return false;
    };
    neovm_core::emacs_core::chartable::ct_ref(&table, i64::from(character.code()))
        .as_vector_data()
        .is_some()
}

/// A captured terminal character-display environment for one buffer.
///
/// GNU's `lookup_glyphless_char_display` (`xdisp.c:8314`) selects the cdr of a
/// `(GRAPHICAL . TEXT)` entry on a TTY, then converts the Lisp value into a
/// closed display method. Capturing the validated table once lets buffer text,
/// replacement strings, and window chrome share the same policy without
/// carrying an untyped Lisp variable through each rendering pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TtyGlyphlessCharDisplay {
    table: Option<Value>,
}

impl TtyGlyphlessCharDisplay {
    /// The identity of the captured table, for a memo key; its contents are
    /// covered by the character-table revision.
    pub(crate) fn table_bits(self) -> Option<usize> {
        self.table.map(Value::bits)
    }

    pub(crate) fn capture<B: LayoutBufferView + ?Sized>(buffer: &B) -> Self {
        let table = buffer_local_value(buffer, LayoutVar::GlyphlessCharDisplay).filter(|value| {
            value
                .as_char_table_obj()
                .is_some_and(|table| !table.extras.is_empty())
        });
        Self { table }
    }

    pub(crate) fn method_for(
        self,
        character: neovm_core::emacs_core::emacs_char::EmacsChar,
    ) -> Option<GlyphlessMethod> {
        let mut method =
            neovm_core::emacs_core::chartable::ct_ref(&self.table?, i64::from(character.code()));
        if method.is_cons() {
            method = method.cons_cdr();
        }
        match method.as_bare_symbol_name() {
            Some("zero-width") => Some(GlyphlessMethod::ZeroWidth),
            Some("thin-space") => Some(GlyphlessMethod::ThinSpace),
            Some("empty-box") => Some(GlyphlessMethod::EmptyBox),
            Some("hex-code") => Some(GlyphlessMethod::HexCode),
            _ => method
                .as_runtime_string_owned()
                .and_then(|text| GlyphlessAcronym::from_ascii(&text))
                .map(GlyphlessMethod::Acronym),
        }
    }
}

/// Resolve the active terminal character mapping for one buffer character.
pub(crate) fn buffer_glyphless_char_display<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    character: neovm_core::emacs_core::emacs_char::EmacsChar,
) -> Option<GlyphlessMethod> {
    TtyGlyphlessCharDisplay::capture(buffer).method_for(character)
}

/// Resolve the ellipsis string GNU renders for invisible/selective-display
/// folds from the active display table's `DISP_INVIS_VECTOR` slot.
///
/// GNU `setup_for_ellipsis` (`xdisp.c:5654`) uses the buffer's display table
/// (`buffer-display-table`, else `standard-display-table`) selective-display
/// slot when it holds a vector of glyph codes, and otherwise falls back to the
/// default three `.` glyphs.  We mirror the character selection; returning
/// `None` lets the caller use the hard-coded `"..."` default.
pub(crate) fn buffer_invisible_ellipsis_text<B: LayoutBufferView>(buffer: &B) -> Option<String> {
    let table = active_buffer_display_table(buffer)?;
    let slot = table
        .as_char_table_obj()?
        .extras
        .get(DISP_INVIS_VECTOR_SLOT)
        .copied()?;
    let glyphs = slot.as_vector_data()?;
    if glyphs.is_empty() {
        return None;
    }
    let text: String = glyphs.iter().filter_map(|g| glyph_code_char(*g)).collect();
    (!text.is_empty()).then_some(text)
}

/// Resolve the per-character display-vector for `ch` from the active display
/// table, returning the decoded glyph characters and their GNU Lisp face runs.
///
/// Mirrors GNU `get_next_display_element` (`xdisp.c:8463`): `dv =
/// DISP_CHAR_VECTOR(it->dp, c)`; when `dv` is a non-empty vector the character
/// is displayed as the sequence of glyph codes in the vector (each decoded by
/// `GLYPH_CODE_CHAR = code & MAX_CHAR`), all sharing the original char's buffer
/// position.  We return the decoded glyph string and typed face runs so the caller
/// can emit it as a single `SourceMappedText` item over the one source char
/// (the whole vector is one display item, consumed once — matching GNU's
/// `dpvec_char_len`-once advance), and so a `?\t` glyph inside the vector
/// re-expands through the normal tab path. Face realization happens later,
/// against the saved surrounding face.
///
/// Returns `None` (the hot path) when there is no active display table, no
/// entry for `ch`, or the entry is not a vector — leaving `ch` to render
/// literally.  An EMPTY vector means "display nothing"; we return `Some("")`
/// so the char is replaced by no glyphs (GNU skips it entirely).
///
pub(crate) fn buffer_display_table_glyphs<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    character: neovm_core::emacs_core::emacs_char::EmacsChar,
) -> Option<BufferDisplayTableGlyphs> {
    let table = active_buffer_display_table(buffer)?;
    let entry = neovm_core::emacs_core::chartable::ct_ref(&table, i64::from(character.code()));
    let glyphs = entry.as_vector_data()?;
    let decoded: Vec<_> = glyphs
        .iter()
        .filter_map(|glyph| glyph_code_parts(*glyph))
        .collect();
    let text = decoded.iter().map(|(ch, _)| *ch).collect();
    let mut face_runs: Vec<DisplaySourceMappedFaceRun> = Vec::new();
    for (_, lisp_face_id) in decoded {
        if let Some(last) = face_runs.last_mut()
            && last.lisp_face_id == lisp_face_id
        {
            last.char_len = last.char_len.saturating_add(1);
        } else {
            face_runs.push(DisplaySourceMappedFaceRun::new(1, lisp_face_id));
        }
    }
    Some(BufferDisplayTableGlyphs {
        text,
        lisp_face_runs: face_runs.into(),
    })
}

pub(crate) fn buffer_selective_display<B: LayoutBufferView>(buffer: &B) -> i32 {
    match buffer_local_value(buffer, LayoutVar::SelectiveDisplay) {
        Some(v) if v.is_fixnum() => v.as_fixnum().unwrap() as i32,
        Some(v) if v.bits() == Value::T.bits() => i32::MAX,
        _ => 0,
    }
}

/// Evidence available for estimating forward scrolling before all intervening
/// display rows have been produced.
///
/// Counting source newlines can under-estimate display rows (wrapping and
/// inserted display material), which a later measured retry can correct.  It
/// must not over-estimate them: selective display, invisibility and replacing
/// display properties can collapse many source lines into one display item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ForwardScrollMeasurement {
    SourceLineEstimate,
    DisplayRowsRequired,
}

fn parse_color_pixel(value: &Value) -> Option<u32> {
    value
        .as_runtime_string_owned()
        .or_else(|| value.as_bare_symbol_name().map(str::to_string))
        .and_then(|spec| NeoColor::parse(&spec))
        .map(|color| color_to_pixel(&color))
}

fn parse_cursor_spec(value: &Value) -> Option<CursorSpec> {
    if value.is_nil() {
        return Some(CursorSpec::no_cursor());
    }

    if value.bits() == Value::T.bits() {
        return Some(CursorSpec::filled_box());
    }
    if let Some(cursor_type) = CursorTypeSymbol::from_symbol_value(value) {
        return Some(match cursor_type {
            CursorTypeSymbol::Box => CursorSpec::filled_box(),
            CursorTypeSymbol::Hollow => CursorSpec::hollow_box(),
            CursorTypeSymbol::Bar => CursorSpec::bar(CursorBarWidth::TWO),
            CursorTypeSymbol::Hbar => CursorSpec::hbar(CursorBarWidth::TWO),
        });
    }
    if value.is_cons() {
        let car = value.cons_car();
        let cdr = value.cons_cdr();
        if let (Some(cursor_type), Some(bar_width)) = (
            CursorTypeSymbol::from_symbol_value(&car),
            cdr.as_fixnum().and_then(CursorBarWidth::from_lisp_fixnum),
        ) && cursor_type.accepts_width_tail()
        {
            return Some(match cursor_type {
                CursorTypeSymbol::Box => CursorSpec::new(CursorKind::FilledBox, bar_width),
                CursorTypeSymbol::Bar => CursorSpec::bar(bar_width),
                CursorTypeSymbol::Hbar => CursorSpec::hbar(bar_width),
                CursorTypeSymbol::Hollow => unreachable!("hollow does not accept a width tail"),
            });
        }
    }

    Some(CursorSpec::hollow_box())
}

fn frame_cursor_spec(frame: &Frame) -> CursorSpec {
    frame
        .parameter("cursor-type")
        .and_then(|value| parse_cursor_spec(&value))
        .unwrap_or(CursorSpec::filled_box())
}

fn default_cursor_color_pixel(face_table: &FaceTable) -> u32 {
    face_table
        .resolve("cursor")
        .background
        .or_else(|| face_table.resolve("default").foreground)
        .map(|color| color_to_pixel(&color))
        .unwrap_or(0x000000)
}

fn frame_background_color_pixel(frame: &Frame, face_table: &FaceTable) -> u32 {
    frame
        .parameter("background-color")
        .and_then(|value| parse_color_pixel(&value))
        .or_else(|| {
            face_table
                .resolve("default")
                .background
                .map(|color| color_to_pixel(&color))
        })
        .unwrap_or(0x00ffffff)
}

fn frame_foreground_color_pixel(frame: &Frame, face_table: &FaceTable) -> u32 {
    frame
        .parameter("foreground-color")
        .and_then(|value| parse_color_pixel(&value))
        .or_else(|| {
            face_table
                .resolve("default")
                .foreground
                .map(|color| color_to_pixel(&color))
        })
        .unwrap_or(0x000000)
}

fn frame_mouse_color_pixel(frame: &Frame) -> u32 {
    frame
        .parameter("mouse-color")
        .and_then(|value| parse_color_pixel(&value))
        .unwrap_or(0x000000)
}

fn frame_cursor_color_pixel(frame: &Frame, face_table: &FaceTable) -> u32 {
    let pixel = frame
        .parameter("cursor-color")
        .and_then(|value| parse_color_pixel(&value))
        .unwrap_or_else(|| default_cursor_color_pixel(face_table));

    // GNU GUI ports resolve `cursor-color` through x_set_cursor_color
    // (xfns.c): when the requested cursor pixel equals the frame background,
    // the actual physical cursor pixel falls back to the mouse pixel so an
    // empty-line or end-of-line filled box remains visible.  TTY frames keep
    // the terminal cursor color sentinel path, so only apply this to GUI
    // frames.
    if frame.effective_window_system().is_none() {
        return pixel;
    }

    let background = frame_background_color_pixel(frame, face_table);
    if pixel != background {
        return pixel;
    }

    let mouse = frame_mouse_color_pixel(frame);
    if mouse != background {
        return mouse;
    }

    // GNU first falls back to the mouse pixel. A dark child frame can make
    // cursor, mouse, and background all identical, however. Keep the stronger
    // renderer invariant that a filled cursor background remains visible on
    // an empty EOL/EOB slot, where there is no glyph foreground to rescue it.
    frame_foreground_color_pixel(frame, face_table)
}

fn frame_cursor_foreground_pixel(
    frame: &Frame,
    face_table: &FaceTable,
    vars: VariableView<'_>,
) -> u32 {
    // GNU's Vx_cursor_fore_pixel is a color-name string when explicitly set;
    // otherwise x_set_cursor_color uses FRAME_BACKGROUND_PIXEL.
    vars.value_by_name("x-cursor-fore-pixel")
        .and_then(|value| parse_color_pixel(&value))
        .unwrap_or_else(|| frame_background_color_pixel(frame, face_table))
}

fn cursor_effect_name_from_symbol(value: Value) -> Option<String> {
    effect_name_from_lisp(value, EffectScope::Cursor).ok()
}

fn apply_cursor_effect_form(effects: &mut EffectsConfig, form: Value) -> bool {
    if form.is_nil() {
        return false;
    }
    let Ok(operation) = effect_operation_from_lisp(form, EffectScope::Cursor) else {
        return false;
    };
    let Ok(updated) = effects.apply_effects(&[operation]) else {
        return false;
    };
    *effects = updated;
    true
}

fn parse_cursor_effect_profile(value: Value) -> Option<EffectsConfig> {
    if value.is_nil() {
        return None;
    }
    let mut effects = EffectsConfig::cursor_profile_baseline();
    if value.is_cons() {
        let forms = list_to_vec(&value)?;
        let is_single_command = forms
            .first()
            .is_some_and(|head| cursor_effect_name_from_symbol(*head).is_some());
        if is_single_command {
            apply_cursor_effect_form(&mut effects, value).then_some(effects)
        } else {
            let mut any = false;
            for form in forms {
                if !apply_cursor_effect_form(&mut effects, form) {
                    return None;
                }
                any = true;
            }
            any.then_some(effects)
        }
    } else {
        apply_cursor_effect_form(&mut effects, value).then_some(effects)
    }
}

fn parse_visual_cursor_spec(
    value: Value,
    index: usize,
    default_color: u32,
) -> Option<VisualCursorSpec> {
    let items = list_to_vec(&value)?;
    let mut charpos: Option<i64> = None;
    let mut cursor_type = Value::symbol("bar");
    let mut color = default_color;
    let mut effects = None;

    let mut iter = items.chunks_exact(2);
    for pair in &mut iter {
        let key = pair[0].as_bare_symbol_name()?;
        let value = pair[1];
        match key {
            ":position" | ":pos" => {
                charpos = value.as_int().map(clamped_lisp_charpos_to_layout_i64);
            }
            ":cursor-type" | ":type" => {
                cursor_type = value;
            }
            ":color" => {
                if let Some(pixel) = parse_color_pixel(&value) {
                    color = pixel;
                }
            }
            ":effect" | ":effects" => {
                effects = parse_cursor_effect_profile(value);
            }
            _ => {}
        }
    }
    if !iter.remainder().is_empty() {
        return None;
    }

    let cursor = parse_cursor_spec(&cursor_type)?;
    Some(VisualCursorSpec {
        id: -1_000_000 - index as i32,
        charpos: charpos?,
        cursor_kind: cursor.cursor_kind,
        cursor_bar_width: cursor.bar_width,
        color,
        effects,
    })
}

fn parse_visual_cursors(buffer: &Buffer, default_color: u32) -> Vec<VisualCursorSpec> {
    let Some(value) = buffer_local_value(buffer, LayoutVar::NeomacsVisualCursors) else {
        return Vec::new();
    };
    let Some(items) = list_to_vec(&value) else {
        return Vec::new();
    };
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| parse_visual_cursor_spec(item, index, default_color))
        .collect()
}

fn effective_cursor_spec(
    frame: &Frame,
    buffer: &Buffer,
    cursor_role: WindowCursorRole,
    is_minibuffer: bool,
    window_cursor_type: Value,
) -> Option<CursorSpec> {
    let base = if window_cursor_type.bits() != Value::T.bits() {
        parse_cursor_spec(&window_cursor_type)
    } else if let Some(buffer_cursor_type) = buffer_local_value(buffer, LayoutVar::CursorType) {
        if buffer_cursor_type.bits() == Value::T.bits() {
            Some(frame_cursor_spec(frame))
        } else {
            parse_cursor_spec(&buffer_cursor_type)
        }
    } else {
        Some(frame_cursor_spec(frame))
    }?;

    if cursor_role.is_active() {
        return Some(base);
    }

    if is_minibuffer {
        return None;
    }

    let alt_cursor = buffer_local_value(buffer, LayoutVar::CursorInNonSelectedWindows);
    if let Some(value) = alt_cursor
        && value.bits() != Value::T.bits()
    {
        return parse_cursor_spec(&value);
    }

    // GNU `xdisp.c::get_window_cursor_type` applies the non-selected
    // fallback after resolving the base cursor kind: FilledBox becomes
    // HollowBox, explicit alternate cursor types win, and BAR cursors
    // narrow by one pixel when `cursor-in-non-selected-windows` is `t`.
    let mut adjusted = base;
    if adjusted.cursor_kind == CursorKind::FilledBox {
        adjusted.cursor_kind = CursorKind::HollowBox;
    } else if adjusted.cursor_kind == CursorKind::Bar {
        adjusted.bar_width = adjusted.bar_width.narrowed_for_non_selected_bar();
    }
    Some(adjusted)
}

/// Build `WindowParams` from neovm-core window + buffer + frame data.
///
/// `is_selected` indicates whether this window is the frame's selected window.
/// `is_minibuffer` indicates whether this is the minibuffer window.
///
/// Returns `None` for internal (non-leaf) windows.
#[allow(clippy::too_many_arguments)]
pub fn window_params_from_neovm(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    vars: VariableView<'_>,
    face_table: &FaceTable,
    default_font_ascent: Option<f32>,
    is_selected: bool,
    is_minibuffer: bool,
    window_cursor_type: Value,
    window_cursor_effect: Value,
) -> Option<WindowParams> {
    window_params_from_neovm_with_font_sizing(
        window,
        buffer,
        frame,
        vars,
        face_table,
        default_font_ascent,
        WindowDisplayRole {
            is_selected,
            cursor_role: WindowCursorRole::from_active(is_selected),
            mode_line_active: is_selected,
            is_minibuffer,
        },
        window_cursor_type,
        window_cursor_effect,
        FontSizing::native_gui(),
    )
}

/// Selection-dependent display roles for one window.
///
/// GNU keeps cursor selection, active mode-line selection, and minibuffer
/// identity separate: during minibuffer input the caller loses the cursor but
/// retains its active mode line.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct WindowDisplayRole {
    /// Lisp/frame selection, published to consumers as window identity.
    pub is_selected: bool,
    /// Ownership of the frame's physical cursor; GNU may redirect this to the
    /// inactive echo-area mini-window without selecting it.
    pub cursor_role: WindowCursorRole,
    pub mode_line_active: bool,
    pub is_minibuffer: bool,
}

#[allow(clippy::too_many_arguments)]
pub fn window_params_from_neovm_with_font_sizing(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    vars: VariableView<'_>,
    face_table: &FaceTable,
    default_font_ascent: Option<f32>,
    display_role: WindowDisplayRole,
    window_cursor_type: Value,
    window_cursor_effect: Value,
    font_sizing: FontSizing,
) -> Option<WindowParams> {
    let WindowDisplayRole {
        is_selected,
        cursor_role,
        mode_line_active,
        is_minibuffer,
    } = display_role;
    // GNU uses the same activity predicate for mode, header and tab lines:
    // selected/minibuffer-caller windows, or every window when this option is
    // nil (dispextern.h:1541-1558,1582-1599,1617-1633).
    let mode_line_active = mode_line_active
        || (neovm_core::face::realization::enabled()
            && neovm_core::face::realization::frame_is_tty_target(frame)
            && vars
                .value_by_name("mode-line-in-non-selected-windows")
                .is_some_and(Value::is_nil));
    // Only leaf windows can be laid out.
    let effective_window_system = frame.effective_window_system();
    let is_window_system = effective_window_system.is_some();
    let window_system =
        effective_window_system.and_then(|v| v.as_bare_symbol_name().map(|s| s.to_string()));

    let (
        win_id,
        _buf_id,
        bounds,
        window_start,
        force_start,
        window_end,
        point,
        hscroll,
        vscroll,
        margins,
        left_fringe_width,
        right_fringe_width,
    ) = match window {
        Window::Leaf {
            id,
            buffer_id,
            bounds,
            window_start,
            force_start,
            window_end,
            point,
            hscroll,
            vscroll,
            margins,
            display,
            ..
        } => (
            *id,
            *buffer_id,
            bounds,
            *window_start,
            *force_start,
            *window_end,
            *point,
            *hscroll,
            *vscroll,
            *margins,
            // Mirrors GNU window_body_width (window.c:1109-1111):
            //   - (FRAME_WINDOW_P (f) ? WINDOW_FRINGES_WIDTH (w) : 0)
            // Fringes only subtract from the text area on GUI frames.
            // TTY frames always have 0 fringes regardless of the
            // `left-fringe` / `right-fringe` frame parameter values.
            if is_window_system {
                if display.left_fringe_width >= 0 {
                    display.left_fringe_width
                } else {
                    frame_parameter_int(frame, "left-fringe", 8) as i32
                }
            } else {
                0
            },
            if is_window_system {
                if display.right_fringe_width >= 0 {
                    display.right_fringe_width
                } else {
                    frame_parameter_int(frame, "right-fringe", 8) as i32
                }
            } else {
                0
            },
        ),
        Window::Internal { .. } => return None,
    };

    let char_width = frame.char_width;
    let char_height = frame.char_height;
    let device_scale = neomacs_display_protocol::DeviceScale::new(frame.device_scale_factor as f32)
        .unwrap_or(neomacs_display_protocol::DeviceScale::ONE);
    // One authority for the default face's realized pixels, shared with the
    // image builtins so a spec keys the image cache identically from Lisp and
    // from layout (GNU: `lookup_image` via `DEFAULT_FACE_ID`).
    let (default_fg, default_bg) = face_table.default_face_colors();
    let face_resolver = FaceResolver::new_with_font_sizing(
        face_table,
        default_fg,
        default_bg,
        frame.font_pixel_size,
        window_system,
        font_sizing,
    );

    // Convert neovm-core Rect to display Rect (same fields, different types).
    let display_bounds = Rect::new(bounds.x, bounds.y, bounds.width, bounds.height);

    let scroll_bar_geometry = window
        .display()
        .map(|display| resolve_window_scroll_bar_geometry(frame, display, is_minibuffer))
        .unwrap_or_default();
    let vertical_scroll_bar_side = scroll_bar_geometry
        .vertical_type
        .and_then(|value| VerticalScrollBarType::from_symbol_value(&value))
        .map(|side| side.name().to_string());
    let left_sb = scroll_bar_geometry.left_area_width.max(0) as f32;
    let right_sb = scroll_bar_geometry.right_area_width.max(0) as f32;
    let scroll_bar_pixel_width = left_sb.max(right_sb);
    let scroll_bar_pixel_height = scroll_bar_geometry.horizontal_area_height.max(0) as f32;
    let horizontal_scroll_bar = scroll_bar_pixel_height > 0.0;

    // Compute text bounds (bounds minus scroll bars, fringes, and margins).
    let left_fringe = left_fringe_width.max(0) as f32;
    let right_fringe = right_fringe_width.max(0) as f32;
    let left_margin = margins.left() as f32 * char_width;
    let right_margin = margins.right() as f32 * char_width;
    let text_x = bounds.x + left_sb + left_fringe + left_margin;
    let text_width = (bounds.width
        - left_sb
        - right_sb
        - left_fringe
        - right_fringe
        - left_margin
        - right_margin)
        .max(0.0);
    let text_bounds = Rect::new(text_x, bounds.y, text_width, bounds.height);

    let window_kind = if is_minibuffer {
        WindowKind::Minibuffer
    } else {
        WindowKind::Main
    };

    // Read buffer-local variables.
    let wrap_mode = effective_wrap_mode(window, buffer, frame, vars, hscroll);
    let word_wrap = effective_buffer_bool(buffer, vars, LayoutVar::WordWrap);
    let tab_width = effective_buffer_int(buffer, vars, LayoutVar::TabWidth, 8) as i32;
    let display_line_numbers = DisplayLineNumbersMode::from_lisp_value(effective_buffer_value(
        buffer,
        vars,
        LayoutVar::DisplayLineNumbers,
    ))
    .for_window_kind(window_kind);
    // GNU `try_scrolling` reads `scroll-conservatively` / `scroll-margin`
    // (buffer-local with a global fallback) to choose between minimal scrolling
    // and recentering point when point jumps off-screen (src/xdisp.c).
    let scroll_conservatively =
        effective_buffer_int(buffer, vars, LayoutVar::ScrollConservatively, 0);
    let scroll_step = effective_buffer_int(buffer, vars, LayoutVar::ScrollStep, 0);
    let scroll_minibuffer_conservatively = vars
        .value_by_name("scroll-minibuffer-conservatively")
        .is_none_or(|value| !value.is_nil());
    let scroll_margin = effective_buffer_int(buffer, vars, LayoutVar::ScrollMargin, 0);

    // GNU window.c gates chrome reservation through window_wants_*:
    // a mode/header/tab line is shown only for leaf non-minibuffer
    // windows whose window parameter is not `none`, whose window
    // parameter or buffer-local format is non-nil, and whose window is
    // high enough to hold the requested chrome.
    let chrome =
        neovm_core::window::WindowChromePresence::resolve(window, buffer, frame, is_minibuffer);
    use neovm_core::window::WindowChromeLine;
    let wants_mode_line = chrome.contains(WindowChromeLine::ModeLine);
    let wants_header_line = chrome.contains(WindowChromeLine::HeaderLine);
    let wants_tab_line = chrome.contains(WindowChromeLine::TabLine);

    // Window chrome is buffer-owned in GNU: `face-remapping-alist` can remap
    // mode-line, header-line, and tab-line independently for this buffer.  Keep
    // the buffer in the resolution seam for both the geometry estimate and the
    // later row render so they cannot select different effective faces.
    let mut chrome_face_next_check = buffer.point_max_char_pos().get();
    let mut resolve_window_chrome_face = |origin: DisplayOrigin| {
        face_resolver.default_base_face_for_origin(
            Some(buffer),
            &origin,
            &mut chrome_face_next_check,
        )
    };

    // GNU xdisp.c's estimate_mode_line_height starts from the frame line
    // height and lets realized face metrics grow from there.
    let mode_line_height = if wants_mode_line {
        chrome_face_pixel_height(
            &resolve_window_chrome_face(DisplayOrigin::ModeLine {
                selected: mode_line_active,
            }),
            char_height,
            device_scale,
        )
    } else {
        0.0
    };

    let cursor_spec = effective_cursor_spec(
        frame,
        buffer,
        cursor_role,
        is_minibuffer,
        window_cursor_type,
    )
    .unwrap_or(CursorSpec {
        cursor_kind: CursorKind::NoCursor,
        bar_width: CursorBarWidth::DEFAULT,
    });
    let cursor_color = frame_cursor_color_pixel(frame, face_table);
    let cursor_foreground = frame_cursor_foreground_pixel(frame, face_table, vars);
    let cursor_effects = parse_cursor_effect_profile(window_cursor_effect).or_else(|| {
        buffer_local_value(buffer, LayoutVar::NeomacsCursorEffect)
            .and_then(parse_cursor_effect_profile)
    });
    let visual_cursors = parse_visual_cursors(buffer, cursor_color);
    let x_stretch_cursor = global_bool(vars, "x-stretch-cursor");
    let fill_column_indicator = buffer_fill_column_indicator(buffer, vars);

    let header_line_height = if wants_header_line {
        chrome_face_pixel_height(
            &resolve_window_chrome_face(DisplayOrigin::HeaderLine {
                selected: mode_line_active,
            }),
            char_height,
            device_scale,
        )
    } else {
        0.0
    };

    let tab_line_height = if wants_tab_line {
        chrome_face_pixel_height(
            &face_resolver.resolve_buffer_tab_line_face(
                buffer,
                crate::display_face_policy::TabLineFaceRole::from(mode_line_active),
            ),
            char_height,
            device_scale,
        )
    } else {
        0.0
    };

    let buffer_default_face = face_resolver.resolve_buffer_default_face(buffer);
    let default_fg = buffer_default_face.fg;
    let default_bg = buffer_default_face.bg;
    // The frame's own `default` face, un-remapped: GNU's `FRAME_FOREGROUND_PIXEL`
    // (src/image.c:6518), which a buffer's `face-remapping-alist` does not move
    // (issue #556).
    let frame_foreground = face_resolver.default_face().fg;

    Some(WindowParams {
        // Filled in by `resolve_window_display_source_params`, the one place
        // every window's params pass through that also holds the evaluator.
        space_image_catalog: None,
        window_id: win_id.0 as i64,
        buffer_id: buffer.id().0,
        bounds: display_bounds,
        text_bounds,
        selected: is_selected,
        cursor_role,
        mode_line_active,
        kind: window_kind,
        left_col: window.left_col(),
        top_line: window.top_line(),
        // Window::window_start tracks GNU marker positions (1-based).
        // Normalize to the layout engine's internal 0-based char positions.
        window_start: lisp_char_pos_to_layout_i64(window_start),
        measurement_rows: None,
        measurement_width: None,
        source_extent: crate::types::WindowSourceExtent::Viewport,
        measurement_pixels: None,
        query_target: None,
        force_start,
        // GNU stores this as an offset from Z; recover the Lisp position and
        // normalize to the layout engine's 0-based space.
        previous_visible_end: match window_end {
            WindowEndState::Current(record) => {
                let buffer_z = LispCharPos1::from_one_based_usize(
                    buffer.point_max_char_pos().get().saturating_add(1),
                );
                Some(lisp_char_pos_to_layout_i64(record.charpos_from_z(buffer_z)))
            }
            WindowEndState::Unrecorded | WindowEndState::Stale(_) => None,
        },
        // Mirror GNU `window.c:window_point` (around line 1782):
        //
        //   return (w == XWINDOW (selected_window)
        //           ? BUF_PT (XBUFFER (w->contents))
        //           : XMARKER (w->pointm)->charpos);
        //
        // For the selected window, the authoritative point lives in the
        // buffer (`BUF_PT`), because editing commands like
        // self-insert-command advance `buf->pt` but do not touch
        // `w->pointm` until the window is later deselected (via
        // `select_window`, which saves the live buffer point into the
        // outgoing window's pointm marker).  Reading `Window::point` here
        // would see a stale pre-command value and place the cursor one
        // character behind where typing just landed.  For non-selected
        // windows, `Window::point` is the right source (it was snapshotted
        // from `buf->pt` the last time the window was deselected).
        //
        // Buffer point is already 0-based (matches the layout engine's
        // internal coordinate system); `Window::point` is GNU/Lisp 1-based
        // and gets normalized with the usual `-1`.
        point: if is_selected {
            buffer.point_char_pos().get() as i64
        } else {
            lisp_char_pos_to_layout_i64(point)
        },
        buffer_size: buffer.point_max_char_pos().get() as i64,
        buffer_modiff: buffer.modified_tick(),
        buffer_begv: buffer.point_min_char_pos().get() as i64,
        display_line_numbers,
        hscroll: hscroll as i32,
        vscroll,
        wrap_mode,
        word_wrap,
        tab_width,
        scroll_conservatively,
        scroll_step,
        scroll_minibuffer_conservatively,
        scroll_margin,
        tab_stop_list: buffer_local_list_values(buffer, LayoutVar::TabStopList)
            .iter()
            .filter_map(|v| v.as_int().map(|n| n as i32))
            .collect(),
        default_fg,
        default_bg,
        frame_foreground,
        char_width,
        char_height,
        window_system: is_window_system,
        font_pixel_size: frame.font_pixel_size,
        image_scale_environment: image_scale_environment(frame, vars),
        font_ascent: if is_window_system {
            default_font_ascent
                .filter(|ascent| *ascent > 0.0)
                .unwrap_or(frame.font_pixel_size * 0.8)
        } else {
            // GNU terminal redisplay has no font object here.  Stretch
            // glyphs and ordinary rows use one terminal cell, not the
            // GUI default font pixel ascent.
            char_height.max(1.0)
        },
        mode_line_height,
        header_line_height,
        tab_line_height,
        cursor_kind: cursor_spec.cursor_kind,
        cursor_bar_width: cursor_spec.bar_width,
        x_stretch_cursor,
        cursor_color,
        cursor_foreground,
        cursor_effects,
        visual_cursors,
        left_fringe_width: left_fringe,
        right_fringe_width: right_fringe,
        fringes_outside_margins: window
            .display()
            .is_some_and(|display| display.fringes_outside_margins),
        // `indicate-empty-lines` and `show-trailing-whitespace` are per-buffer
        // display variables in GNU (DEFVAR_PER_BUFFER): a `setq-default` enables
        // them in every buffer that has no local override. Read the EFFECTIVE
        // value (buffer-local, else global/default) — `buffer_local_bool` sees
        // only an explicit local binding and so silently ignored `setq-default`.
        indicate_empty_lines: if effective_buffer_bool(buffer, vars, LayoutVar::IndicateEmptyLines)
        {
            1
        } else {
            0
        },
        show_trailing_whitespace: effective_buffer_bool(
            buffer,
            vars,
            LayoutVar::ShowTrailingWhitespace,
        ),
        trailing_ws_bg: face_bg_pixel(face_table, "trailing-whitespace", 0),
        fill_column_indicator: fill_column_indicator
            .map(|(column, _)| column)
            .unwrap_or(-1),
        fill_column_indicator_char: fill_column_indicator
            .map(|(_, character)| character)
            .unwrap_or('|'),
        fill_column_indicator_fg: face_fg_pixel(face_table, "fill-column-indicator", 0),
        extra_line_spacing: match buffer_local_value(buffer, LayoutVar::LineSpacing) {
            Some(v) if v.is_fixnum() => v.as_fixnum().unwrap() as f32,
            Some(v) if v.is_float() => v.xfloat() as f32,
            _ => 0.0,
        },
        selective_display: buffer_selective_display(buffer),
        escape_glyph_fg: face_fg_pixel(face_table, "escape-glyph", 0),
        nobreak_char_display: effective_nobreak_char_display(buffer, vars),
        // GNU merges `nobreak-space` for non-ASCII spaces (and `nobreak-hyphen`
        // for hyphens) in highlight mode; `nobreak-space` inherits `escape-glyph`,
        // so `resolve` yields the escape-glyph color when nobreak-space itself
        // sets no foreground (xdisp.c:8594-8617, faces.el `nobreak-space`).
        nobreak_char_fg: face_fg_pixel(face_table, "nobreak-space", 0),
        glyphless_char_fg: face_fg_pixel(face_table, "glyphless-char", 0),
        wrap_prefix: Vec::new(),
        line_prefix: Vec::new(),
        left_margin_width: left_margin,
        left_margin_columns: i64::try_from(margins.left())
            .expect("left margin column count fits in evaluator geometry"),
        right_margin_width: right_margin,
        right_margin_columns: i64::try_from(margins.right())
            .expect("right margin column count fits in evaluator geometry"),
        vertical_scroll_bar_side,
        horizontal_scroll_bar,
        scroll_bar_pixel_width,
        scroll_bar_pixel_height,
    })
}

/// The sole owner of a frame's active physical cursor for one redisplay.
///
/// Window selection remains a separate fact.  `InactiveEchoArea` models GNU's
/// `cursor-in-echo-area` redirection without manufacturing a selected
/// minibuffer window, while `None` prevents non-selected frames from
/// publishing an active cursor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RedisplayCursorTarget {
    None,
    LiveWindow(WindowId),
    InactiveEchoArea(WindowId),
}

impl RedisplayCursorTarget {
    pub(crate) fn role_for(self, window_id: WindowId) -> WindowCursorRole {
        WindowCursorRole::from_active(matches!(
            self,
            Self::LiveWindow(owner) | Self::InactiveEchoArea(owner) if owner == window_id
        ))
    }
}

pub(crate) fn redisplay_cursor_target(
    evaluator: &Context,
    frame_id: FrameId,
) -> RedisplayCursorTarget {
    let Some(frame) = evaluator.frame_manager().get(frame_id) else {
        return RedisplayCursorTarget::None;
    };
    let frame_is_selected = evaluator
        .frame_manager()
        .selected_frame()
        .is_some_and(|selected| selected.id == frame_id);
    if !frame_is_selected {
        return RedisplayCursorTarget::None;
    }

    if evaluator.inactive_echo_area_cursor_requested()
        && let Some(minibuffer) = frame.minibuffer_window
        && !evaluator.minibuffer_window_is_active(minibuffer)
    {
        return RedisplayCursorTarget::InactiveEchoArea(minibuffer);
    }

    RedisplayCursorTarget::LiveWindow(frame.selected_window)
}

/// Collect all leaf windows from a frame (including minibuffer) and build
/// `WindowParams` for each.
///
/// Returns `(FrameParams, Vec<WindowParams>)`, or `None` if the frame does
/// not exist.
pub fn collect_layout_params(
    evaluator: &Context,
    frame_id: FrameId,
    default_font_ascent: Option<f32>,
) -> Option<(FrameParams, Vec<WindowParams>)> {
    collect_layout_params_with_font_sizing(
        evaluator,
        frame_id,
        default_font_ascent,
        FontSizing::native_gui(),
    )
}

pub fn collect_layout_params_with_font_sizing(
    evaluator: &Context,
    frame_id: FrameId,
    default_font_ascent: Option<f32>,
    font_sizing: FontSizing,
) -> Option<(FrameParams, Vec<WindowParams>)> {
    let frame = evaluator.frame_manager().get(frame_id)?;
    let frame_is_selected = evaluator
        .frame_manager()
        .selected_frame()
        .is_some_and(|selected| selected.id == frame_id);
    let minibuffer_caller = evaluator.minibuffer_selected_window_id();
    let cursor_target = redisplay_cursor_target(evaluator, frame_id);
    let frame_params =
        frame_params_from_neovm(frame, evaluator.face_table(), evaluator.variables());

    let mut window_params = Vec::new();

    // Collect leaf windows from the root window tree.  Root leaves only --
    // the minibuffer is positioned separately below -- so this is
    // `root_leaf_ids`, not `all_leaf_ids`.
    let leaf_ids = frame.root_leaf_ids();
    for win_id in &leaf_ids {
        let Some(window) = frame.find_window(*win_id) else {
            continue;
        };
        let Some(buf_id) = window.buffer_id() else {
            continue;
        };
        let Some(buffer) = evaluator.buffer_manager().get(buf_id) else {
            continue;
        };
        let is_selected = frame_is_selected && frame.selected_window == *win_id;
        let mode_line_active = frame_is_selected
            && (frame.selected_window == *win_id || minibuffer_caller == Some(*win_id));
        let window_cursor_type = evaluator.frame_manager().window_cursor_type(*win_id);
        let window_cursor_effect = evaluator
            .frame_manager()
            .window_parameter(*win_id, &Value::symbol("neomacs-cursor-effect"))
            .unwrap_or(Value::NIL);
        if let Some(wp) = window_params_from_neovm_with_font_sizing(
            window,
            buffer,
            frame,
            evaluator.variables(),
            evaluator.face_table(),
            default_font_ascent,
            WindowDisplayRole {
                is_selected,
                cursor_role: cursor_target.role_for(*win_id),
                mode_line_active,
                is_minibuffer: frame.minibuffer_window == Some(*win_id),
            },
            window_cursor_type,
            window_cursor_effect,
            font_sizing,
        ) {
            tracing::debug!(
                "layout window cursor: win={} selected={} minibuffer={} kind={:?} width={} color=#{:06x} window-cursor-type={:?}",
                wp.window_id,
                wp.selected,
                wp.is_minibuffer(),
                wp.cursor_kind,
                wp.cursor_bar_width,
                wp.cursor_color,
                window_cursor_type,
            );
            window_params.push(wp);
        }
    }

    if window_params.len() > 1 {
        tracing::debug!(
            "collect_layout_params: {} leaf windows, root bounds=({},{} {}x{})",
            window_params.len(),
            frame.root_window().bounds().x,
            frame.root_window().bounds().y,
            frame.root_window().bounds().width,
            frame.root_window().bounds().height,
        );
    }

    // Add minibuffer window if present.
    if let Some(mini_leaf) = &frame.minibuffer_leaf {
        let buf_id = mini_leaf.buffer_id();
        let buffer = buf_id.and_then(|id| evaluator.buffer_manager().get(id));
        if let Some(buffer) = buffer {
            let is_selected = frame_is_selected && frame.selected_window == mini_leaf.id();
            let window_cursor_type = evaluator.frame_manager().window_cursor_type(mini_leaf.id());
            let window_cursor_effect = evaluator
                .frame_manager()
                .window_parameter(mini_leaf.id(), &Value::symbol("neomacs-cursor-effect"))
                .unwrap_or(Value::NIL);
            if let Some(wp) = window_params_from_neovm_with_font_sizing(
                mini_leaf,
                buffer,
                frame,
                evaluator.variables(),
                evaluator.face_table(),
                default_font_ascent,
                WindowDisplayRole {
                    is_selected,
                    cursor_role: cursor_target.role_for(mini_leaf.id()),
                    mode_line_active: is_selected,
                    is_minibuffer: true,
                },
                window_cursor_type,
                window_cursor_effect,
                font_sizing,
            ) {
                tracing::debug!(
                    "layout window cursor: win={} selected={} minibuffer=true kind={:?} width={} color=#{:06x} window-cursor-type={:?}",
                    wp.window_id,
                    wp.selected,
                    wp.cursor_kind,
                    wp.cursor_bar_width,
                    wp.cursor_color,
                    window_cursor_type,
                );
                tracing::debug!(
                    "  minibuffer id={} bounds=({},{} {}x{})",
                    wp.window_id,
                    wp.bounds.x,
                    wp.bounds.y,
                    wp.bounds.width,
                    wp.bounds.height,
                );
                window_params.push(wp);
            }
        }
    }

    Some((frame_params, window_params))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum LayoutLineCountMode {
    Off,
    On,
    Verify,
}

impl LayoutLineCountMode {
    #[inline]
    fn from_setting(setting: Option<&str>) -> Self {
        match setting
            .map(str::trim)
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("on" | "1" | "true" | "yes") => Self::On,
            Some("verify") => Self::Verify,
            None => Self::On,
            _ => Self::Off,
        }
    }
}

#[inline]
fn layout_line_count_mode() -> LayoutLineCountMode {
    static MODE: std::sync::OnceLock<LayoutLineCountMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        LayoutLineCountMode::from_setting(
            std::env::var("NEOMACS_LAYOUT_LINE_COUNT").ok().as_deref(),
        )
    })
}

/// Buffer accessor for the layout engine.
///
/// Wraps a reference to a neovm-core `Buffer` and provides the operations
/// that the layout engine needs: text byte copying, position conversion,
/// and line counting.
pub(crate) struct RustBufferAccess<'a, B: LayoutBufferView> {
    buffer: &'a B,
}

impl<'a, B: LayoutBufferView> RustBufferAccess<'a, B> {
    /// Create a new buffer accessor.
    pub fn new(buffer: &'a B) -> Self {
        Self { buffer }
    }

    /// The underlying layout view (for buffer-local lookups the accessor does
    /// not wrap).
    pub fn view(&self) -> &'a B {
        self.buffer
    }

    /// Convert an internal neovm buffer character position to a byte position.
    ///
    /// `WindowParams` used by the pure-Rust layout path carry neovm-core's
    /// internal character positions, which are 0-based and use an exclusive
    /// accessible end (`accessible_end_char` / `buffer_size`).
    pub fn charpos_to_bytepos(&self, charpos: i64) -> i64 {
        buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos).get() as i64
    }

    /// Classify an unmeasured layout-character span for forward scrolling.
    ///
    /// This is shared by pre-layout source selection and post-layout retries;
    /// neither phase may silently fall back to source-line arithmetic across a
    /// span whose display walk can consume or replace source text.
    pub(crate) fn forward_scroll_measurement(
        &self,
        char_from: i64,
        char_to: i64,
    ) -> ForwardScrollMeasurement {
        if char_to <= char_from {
            return ForwardScrollMeasurement::SourceLineEstimate;
        }
        if buffer_selective_display(self.view()) != 0
            || RustTextPropAccess::new(self.view())
                .has_display_row_collapse_between(char_from, char_to)
        {
            ForwardScrollMeasurement::DisplayRowsRequired
        } else {
            ForwardScrollMeasurement::SourceLineEstimate
        }
    }

    /// Convert a GNU Lisp-visible buffer position to a byte position.
    ///
    /// GNU Lisp positions are 1-based, so this is only appropriate for
    /// values coming from Lisp APIs such as `minibuffer-prompt-end`.
    pub fn lisp_charpos_to_bytepos(&self, charpos: i64) -> i64 {
        let Some(charpos) = lisp_charpos_to_layout_char_pos(charpos) else {
            return 0;
        };
        buffer_charpos_to_emacs_byte_pos(self.buffer, charpos).get() as i64
    }

    /// Copy buffer text bytes in the range `[byte_from, byte_to)` into `out`.
    ///
    /// Uses backend-neutral Emacs byte ranges so layout is independent of
    /// the concrete buffer storage.
    pub fn copy_text(&self, byte_from: i64, byte_to: i64, out: &mut Vec<u8>) {
        let Some(range) = clamped_layout_emacs_byte_range(self.buffer, byte_from, byte_to) else {
            out.clear();
            return;
        };
        self.buffer.layout_copy_emacs_byte_range_to(range, out);
    }

    /// Count the number of newlines in `[byte_from, byte_to)`.
    ///
    /// Used for line number display.
    pub fn count_lines(&self, byte_from: i64, byte_to: i64) -> i64 {
        let Some(range) = clamped_layout_emacs_byte_range(self.buffer, byte_from, byte_to) else {
            return 0;
        };
        if let Some(count) = self.buffer.layout_indexed_newline_count(range) {
            return count as i64;
        }
        self.count_line_chunks(range, layout_line_count_mode())
    }

    #[inline]
    fn count_line_chunks(&self, range: EmacsByteRange, mode: LayoutLineCountMode) -> i64 {
        let mut count: i64 = 0;
        self.buffer
            .layout_try_for_each_emacs_byte_range_chunk(range, |chunk| {
                let found = match mode {
                    LayoutLineCountMode::Off => chunk.iter().filter(|byte| **byte == b'\n').count(),
                    LayoutLineCountMode::On => memchr::memchr_iter(b'\n', chunk).count(),
                    LayoutLineCountMode::Verify => {
                        let found = memchr::memchr_iter(b'\n', chunk).count();
                        let scalar = chunk.iter().filter(|byte| **byte == b'\n').count();
                        assert_eq!(found, scalar, "line count verify: chunk scan disagrees");
                        found
                    }
                };
                count += found as i64;
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("newline counting is infallible");
        count
    }

    /// Find a read boundary after N source newlines that cannot be consumed by
    /// a display/invisible property, scanning only `[byte_from, byte_limit)`.
    /// Inserted strings and wrapping only add rows. Conservatively ignore every
    /// newline covered by either property, including inactive values and
    /// overlays belonging to another window. Skip hazardous extents directly,
    /// so a large fold needs no per-line property queries. The returned offset
    /// always follows a real newline.
    ///
    /// This is a line-EXISTENCE query: `None` says the bounded region holds
    /// fewer than `n` preserved newlines, never that the buffer ended. GNU
    /// asks the same question of `find_newline` (src/search.c:675), which
    /// answers it from the newline cache's newline-free region records
    /// (src/search.c:595-644 `region_cache_forward`), and it bounds its own
    /// continued-line search by the window's cell count
    /// (`WINDOW_TOTAL_LINES (w) * WINDOW_TOTAL_COLS (w)`, src/xdisp.c:19763).
    /// A caller that only needs a screenful of lines must pass such a bound,
    /// or a newline-less tail walks (and is then copied) to ZV.
    pub fn find_nth_preserved_newline_within(
        &self,
        byte_from: i64,
        byte_limit: i64,
        n: usize,
    ) -> Option<i64> {
        let bounds =
            clamped_layout_emacs_byte_range(self.buffer, byte_from, byte_limit.min(self.zv()))?;
        if n == 0 {
            return Some(bounds.start().get() as i64);
        }
        let lookups = ["display", "invisible"]
            .map(|name| LayoutCharPropertyLookup::new(self.buffer, Value::symbol(name)));
        let overlays = self.buffer.layout_overlays();
        let mut start = bounds.start();
        let mut remaining = n;
        while start < bounds.end() {
            let mut offset = start.get();
            let result = self.buffer.layout_try_for_each_emacs_byte_range_chunk(
                EmacsByteRange::new(start, bounds.end()),
                |chunk| {
                    // memchr, not a scalar byte loop: on a newline-free region
                    // this query is a scan for the first newline, which GNU
                    // answers from the newline cache without reading the bytes
                    // at all (src/search.c:595-644); the vectorized search is
                    // the closest bounded equivalent.
                    let mut searched = 0;
                    while let Some(found) = memchr::memchr(b'\n', &chunk[searched..]) {
                        let index = searched + found;
                        searched = index + 1;
                        let pos = EmacsBytePos::new(offset + index);
                        let after = EmacsBytePos::new(pos.get() + 1);
                        let mut skip_to = pos;
                        for lookup in &lookups {
                            if let Some(extent) =
                                lookup.effective_text_extent_at(self.buffer, pos, bounds)
                            {
                                skip_to = skip_to.max(extent.end());
                            }
                        }
                        for overlay in overlays.iter_overlays_in_accessible_emacs_byte_range(
                            EmacsByteRange::new(pos, after),
                            bounds.end(),
                        ) {
                            let Some(end) = overlays.overlay_end_emacs_byte_pos(overlay) else {
                                continue;
                            };
                            // Empty overlays insert strings but consume no source.
                            if end <= pos
                                || overlays
                                    .overlay_start_emacs_byte_pos(overlay)
                                    .is_none_or(|begin| begin > pos)
                            {
                                continue;
                            }
                            if lookups.iter().any(|lookup| {
                                lookup
                                    .effective_overlay_value(self.buffer, overlay)
                                    .is_some()
                            }) {
                                skip_to = skip_to.max(end.min(bounds.end()));
                            }
                        }
                        if skip_to > pos {
                            return Err((false, skip_to));
                        }
                        remaining -= 1;
                        if remaining == 0 {
                            return Err((true, after));
                        }
                    }
                    offset += chunk.len();
                    Ok(())
                },
            );
            match result {
                Err((true, end)) => return Some(end.get() as i64),
                Err((false, resume)) => start = resume,
                Ok(()) => return None,
            }
        }
        None
    }

    /// Read a single byte at the given byte position.
    ///
    /// Returns `None` if the position is out of bounds.
    pub fn byte_at(&self, byte_pos: i64) -> Option<u8> {
        let pos = layout_emacs_byte_pos_from_i64(byte_pos)?;
        if pos < layout_total_emacs_byte_end_pos(self.buffer) {
            self.buffer.layout_emacs_byte_at_pos(pos)
        } else {
            None
        }
    }

    /// Get the buffer's narrowed beginning (begv) as byte position.
    pub fn begv(&self) -> i64 {
        self.buffer.layout_point_min_emacs_byte_pos().get() as i64
    }

    /// Get the buffer's narrowed end (zv) as byte position.
    pub fn zv(&self) -> i64 {
        self.buffer.layout_point_max_emacs_byte_pos().get() as i64
    }
}

/// Soft cap (in emacs bytes) on how far ahead the display `invisible` scan
/// walks the text-property interval tree per redisplay. Beyond this the scan
/// returns a soft boundary and the checkpoint cache re-scans a screenful later;
/// a code buffer with no invisible text no longer walks to buffer-end on every
/// redisplay. Chosen to comfortably span a tall window so most redisplays scan
/// once. Mirrors GNU's `TEXT_PROP_DISTANCE_LIMIT` in `compute_stop_pos`.
const INVISIBLE_SCAN_BYTE_LIMIT: usize = 2048;

/// Whether an overlay's contributions (face, before/after strings) apply in the
/// window currently being laid out — GNU `overlay_matches_window`: an overlay
/// carrying a `window` property contributes in that window only (e.g. hl-line
/// with a non-sticky flag sets it to the selected window, so the same buffer in
/// two windows highlights only the selected one), a missing or non-window `window`
/// property is unrestricted, and `current_window_id == None` (frame chrome / TTY)
/// applies every overlay.
///
/// Delegates to `OverlayList::overlay_applies_to_window`, which the core property
/// resolvers apply themselves. The rule lives THERE, next to the precedence
/// comparator, so a resolver and a hand-written overlay scan cannot disagree about
/// which overlays speak in this window — which is how hl-line's per-window
/// highlight leaked into every window before.
pub(crate) fn overlay_applies_to_window<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    overlay_id: Value,
    current_window_id: Option<u64>,
) -> bool {
    buffer
        .layout_overlays()
        .overlay_applies_to_window(overlay_id, current_window_id)
}

/// The overlay-`face` portion of GNU `face_at_buffer_position`: overlay `face`
/// values at `bytepos` in ascending `sort_overlays` precedence (merge in order —
/// higher precedence wins by being applied last), windowed overlays filtered to
/// `window_id`.
///
/// Both the precedence order and the `window` filter delegate to
/// `OverlayList` (`sort_overlay_ids_by_priority_desc` and
/// `overlay_applies_to_window`), the same policy owners used by the
/// single-winner resolvers (`display`, `invisible`, `mouse-face`).  This keeps
/// ordering and the window rule from drifting apart — which is exactly how
/// hl-line's per-window highlight once leaked into every window.
///
/// This is the single implementation of the face scan. Every face-resolution path
/// — the per-glyph buffer face (`BufferTextSourceCursor::face_at`) and the
/// resolved-face/row-fill path (`FaceResolver::face_at_pos`) — calls it, so the
/// `font-lock-face` fallback boundary stays consistent too.
pub(crate) struct OverlayFacesAtPosition {
    /// Overlay `face` values to merge, in ascending priority.
    pub(crate) faces: Vec<Value>,
    /// Nearest overlay end strictly after `bytepos` (across ALL overlays, so a
    /// resolved-face cache invalidates at every overlay boundary). `None` if
    /// there is no overlay ending ahead.
    pub(crate) next_boundary: Option<EmacsBytePos>,
}

/// One effective GNU character-property lookup prepared for a layout buffer.
///
/// The hot source cursor resolves aliases and `default-text-properties' once
/// at construction. Each query then delegates direct/category/alias/default
/// precedence to neovm-core's shared `resolve_effective_char_property', so
/// redisplay and Lisp primitives cannot quietly implement different rules.
#[derive(Clone, Debug)]
pub(crate) struct LayoutCharPropertyLookup {
    lookup_order: PropertyKeyOrder,
    default: Option<Value>,
}

impl LayoutCharPropertyLookup {
    pub(crate) fn new<B: LayoutBufferView + ?Sized>(buffer: &B, property: Value) -> Self {
        let lookup_order = PropertyKeyOrder::capture(buffer, property);
        let default = buffer
            .layout_buffer_local_value(LayoutVar::DefaultTextProperties)
            .filter(|value| value.is_cons())
            .and_then(|defaults| plist_get(defaults, &property));
        Self {
            lookup_order,
            default,
        }
    }

    pub(crate) fn text_value_at<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        bytepos: EmacsBytePos,
    ) -> Option<Value> {
        let (canonical, aliases) = self.lookup_order.canonical_and_aliases();
        resolve_effective_char_property(
            DirectCharProperties::from_getter(
                |property| buffer.layout_text_prop_at_emacs_byte_pos(bytepos, property),
                canonical,
            ),
            |category, property| buffer.layout_category_symbol_property(category, property),
            canonical,
            aliases.iter().copied(),
            |property| buffer.layout_text_prop_at_emacs_byte_pos(bytepos, property),
            self.default,
        )
    }

    /// Maximal text range over which this lookup resolves to the same effective
    /// value.  The scan watches the canonical property, its aliases, and the
    /// `category` indirection rather than only the canonical plist key.
    pub(crate) fn effective_text_extent_at<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        bytepos: EmacsBytePos,
        bounds: EmacsByteRange,
    ) -> Option<EmacsByteRange> {
        if bounds.is_empty() || bytepos < bounds.start() || bytepos >= bounds.end() {
            return None;
        }
        let target = self
            .text_value_at(buffer, bytepos)
            .filter(|value| !value.is_nil())?;
        let mut watched: Vec<_> = self.lookup_order.ordered().collect();
        let category = Value::symbol("category");
        if !watched
            .iter()
            .any(|property| property.bits() == category.bits())
        {
            watched.push(category);
        }

        let previous_change = |cursor| {
            watched
                .iter()
                .filter_map(|property| {
                    buffer.layout_previous_single_text_prop_change_before_emacs_byte_pos(
                        cursor, *property,
                    )
                })
                .max()
        };
        let next_change = |cursor| {
            watched
                .iter()
                .filter_map(|property| {
                    buffer
                        .layout_next_single_text_prop_change_after_emacs_byte_pos(cursor, *property)
                })
                .min()
        };

        let mut start = bounds.start();
        let mut cursor = bytepos;
        while let Some(boundary) = previous_change(cursor) {
            if boundary <= bounds.start() {
                break;
            }
            let boundary_char = buffer.layout_emacs_byte_pos_to_char_pos(boundary);
            let previous_char = CharPos0::new(boundary_char.get().saturating_sub(1));
            let probe = buffer.layout_char_pos_to_emacs_byte_pos(previous_char);
            if self
                .text_value_at(buffer, probe)
                .is_none_or(|value| !eq_value(&value, &target))
            {
                start = boundary;
                break;
            }
            cursor = boundary;
        }

        let mut end = bounds.end();
        let mut cursor = bytepos;
        while let Some(boundary) = next_change(cursor) {
            if boundary >= bounds.end() {
                break;
            }
            if self
                .text_value_at(buffer, boundary)
                .is_none_or(|value| !eq_value(&value, &target))
            {
                end = boundary;
                break;
            }
            cursor = boundary;
        }

        Some(EmacsByteRange::new(start, end))
    }

    pub(crate) fn effective_overlay_value<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        overlay: Value,
    ) -> Option<Value> {
        let (canonical, aliases) = self.lookup_order.canonical_and_aliases();
        let overlays = buffer.layout_overlays();
        resolve_effective_char_property(
            DirectCharProperties::from_getter(
                |property| overlays.overlay_get_named(overlay, property),
                canonical,
            ),
            |category, property| buffer.layout_category_symbol_property(category, property),
            canonical,
            aliases.iter().copied(),
            |property| overlays.overlay_get_named(overlay, property),
            None,
        )
        .filter(|value| !value.is_nil())
    }

    /// Build a conservative endpoint-tree signature from every direct key that
    /// can supply this effective property. `category` is included because its
    /// symbol plist may provide the canonical value; configured aliases remain
    /// ordinary keys and require no index-specific policy.
    pub(crate) fn overlay_endpoint_filter(&self) -> OverlayPropertyFilter {
        OverlayPropertyFilter::for_properties(
            self.lookup_order
                .ordered()
                .chain(std::iter::once(Value::symbol("category"))),
        )
    }

    /// Resolve overlay presence at one position without starting an extent
    /// sweep.  The returned proof owns this exact lookup's resolver and is
    /// bound to the originating overlay list.
    pub(crate) fn overlay_at_point<'a, B: LayoutBufferView + ?Sized>(
        &'a self,
        buffer: &'a B,
        bytepos: EmacsBytePos,
        current_window_id: Option<u64>,
    ) -> OverlayPropertyAtPoint<'a, impl FnMut(Value) -> Option<Value> + 'a> {
        let overlays = buffer.layout_overlays();
        overlays.resolve_overlay_property_at_emacs_byte_pos(
            bytepos,
            current_window_id,
            move |overlay| self.effective_overlay_value(buffer, overlay),
        )
    }

    pub(crate) fn overlay_or_text_value_at<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        bytepos: EmacsBytePos,
        current_window_id: Option<u64>,
    ) -> Option<Value> {
        self.overlay_or_text_source_at(buffer, bytepos, current_window_id)
            .map(|source| source.value)
    }

    /// The winning value AND where it came from.
    ///
    /// Which overlay won is not bookkeeping: GNU's `display` handling bounds an
    /// OVERLAY-sourced property by that overlay's end rather than by the next
    /// property change (xdisp.c:6337-6363), so a consumer that only sees the
    /// value cannot compute the covered range correctly.
    pub(crate) fn overlay_or_text_source_at<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        bytepos: EmacsBytePos,
        current_window_id: Option<u64>,
    ) -> Option<CharPropertySource> {
        match self.overlay_at_point(buffer, bytepos, current_window_id) {
            OverlayPropertyAtPoint::Present(resolution) => Some(CharPropertySource {
                value: resolution.value(),
                overlay: Some(resolution.overlay()),
            }),
            OverlayPropertyAtPoint::Vacant(_) => {
                self.text_value_at(buffer, bytepos)
                    .map(|value| CharPropertySource {
                        value,
                        overlay: None,
                    })
            }
        }
    }

    fn overlay_values_ascending_at<B: LayoutBufferView + ?Sized>(
        &self,
        buffer: &B,
        bytepos: EmacsBytePos,
        current_window_id: Option<u64>,
    ) -> Vec<Value> {
        let (canonical, aliases) = self.lookup_order.canonical_and_aliases();
        let overlays = buffer.layout_overlays();
        let mut overlay_ids = overlays.overlays_at_emacs_byte_pos(bytepos);
        // GNU's `sort_overlays' reads `priority' through `Foverlay_get', so an
        // overlay carrying only a `category' is ordered by that symbol's
        // `priority'. The overlay layer cannot follow a category on its own;
        // this view already knows how, for the property values below.
        let priority_property = Value::symbol("priority");
        let category_property = Value::symbol("category");
        let priority_of = |overlay: Value| -> Option<Value> {
            if let Some(own) = overlays.overlay_get_named(overlay, priority_property) {
                return Some(own);
            }
            let category = overlays.overlay_get_named(overlay, category_property)?;
            buffer.layout_category_symbol_property(category, priority_property)
        };
        overlays.sort_overlay_ids_by_priority_desc_with(&mut overlay_ids, &priority_of);
        overlay_ids.reverse();
        overlay_ids
            .into_iter()
            .filter(|overlay| overlays.overlay_applies_to_window(*overlay, current_window_id))
            .filter_map(|overlay| {
                resolve_effective_char_property(
                    DirectCharProperties::from_getter(
                        |property| overlays.overlay_get_named(overlay, property),
                        canonical,
                    ),
                    |category, property| buffer.layout_category_symbol_property(category, property),
                    canonical,
                    aliases.iter().copied(),
                    |property| overlays.overlay_get_named(overlay, property),
                    None,
                )
                .filter(|value| !value.is_nil())
            })
            .collect()
    }
}

/// A resolved char-property value together with the overlay that supplied it,
/// or `None` for a text property.
#[derive(Clone, Copy, Debug)]
pub(crate) struct CharPropertySource {
    pub(crate) value: Value,
    pub(crate) overlay: Option<Value>,
}

pub(crate) fn overlay_faces_at<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    bytepos: EmacsBytePos,
    current_window_id: Option<u64>,
) -> OverlayFacesAtPosition {
    let overlays = buffer.layout_overlays();
    let mut next_boundary: Option<EmacsBytePos> = None;
    // GNU `face_at_buffer_position` narrows its `endptr` at EVERY overlay end at
    // this position, not only at those carrying `face`, so a resolved-face cache
    // re-resolves at every overlay boundary.
    for oid in overlays.iter_overlays_at_emacs_byte_pos(bytepos) {
        if let Some(end) = overlays.overlay_end_emacs_byte_pos(oid)
            && end.get() > bytepos.get()
        {
            next_boundary = Some(match next_boundary {
                Some(prev) if prev.get() <= end.get() => prev,
                _ => end,
            });
        }
    }
    // `face` is GNU's one MERGE policy: overlay faces stack over the
    // text-property face in ascending `sort_overlays` precedence. Ordering and the
    // `window`-property filter delegate to OverlayList's shared comparator and
    // applicability rule, so this path cannot drift from the single-winner
    // resolvers. It previously ordered by a bare `priority` integer, which reads
    // a `(PRIMARY . SECONDARY)` priority as 0 and drops GNU's containment rule.
    let faces = LayoutCharPropertyLookup::new(buffer, Value::symbol("face"))
        .overlay_values_ascending_at(buffer, bytepos, current_window_id);
    OverlayFacesAtPosition {
        faces,
        next_boundary,
    }
}

/// Text property and overlay accessor for the layout engine.
///
/// Wraps a reference to a neovm-core `Buffer` and provides query methods
/// for invisible text, display properties, overlay strings, and other
/// text property-based features.
pub(crate) struct RustTextPropAccess<'a, B: LayoutBufferView + ?Sized> {
    buffer: &'a B,
    window_id: Option<u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct OverlayDisplayString {
    pub(crate) string: Value,
    pub(crate) overlay_id: Value,
    /// The overlay's buffer start, carried with the display string because an
    /// integer `cursor` text property defines its coverage from this position
    /// even when an after-string is rendered at the overlay's end.
    pub(crate) overlay_start_charpos: CharPos0,
    /// True for an after-string, false for a before-string. Drives GNU's
    /// `compare_overlay_entries` interleaving order.
    pub(crate) after_string_p: bool,
    /// The overlay's `priority` plist value, captured at collection time so the
    /// comparator does not re-read the plist.
    pub(crate) priority: i64,
}

impl OverlayDisplayString {
    #[cfg(test)]
    pub(crate) fn bytes(&self) -> Option<&[u8]> {
        self.string.as_lisp_string().map(|string| string.as_bytes())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InvisiblePropertyOrigin {
    TextProperty,
    Overlay,
}

/// The resolved visibility at one buffer position.
///
/// Making visibility a sum type rules out contradictory states such as
/// "visible with ellipsis" and retains the property origin needed by GNU's
/// boundary-overlay-string rule in `handle_invisible_prop`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InvisibleStatus {
    Visible,
    Hidden {
        ellipsis: bool,
        origin: InvisiblePropertyOrigin,
    },
}

impl InvisibleStatus {
    #[cfg(test)]
    const VISIBLE: Self = Self::Visible;

    const fn from_invisibility(
        invisibility: Invisibility,
        origin: InvisiblePropertyOrigin,
    ) -> Self {
        match invisibility {
            Invisibility::Visible => Self::Visible,
            Invisibility::Hidden => Self::Hidden {
                ellipsis: false,
                origin,
            },
            Invisibility::HiddenWithEllipsis => Self::Hidden {
                ellipsis: true,
                origin,
            },
        }
    }

    pub(crate) const fn hidden(self) -> bool {
        matches!(self, Self::Hidden { .. })
    }

    pub(crate) const fn ellipsis(self) -> bool {
        matches!(self, Self::Hidden { ellipsis: true, .. })
    }

    /// GNU only reloads strings from the old stop position after invisibility
    /// came from a text property.  Overlay invisibility makes its own start and
    /// end strings indistinguishable and is handled at the overlay endpoint.
    pub(crate) const fn preserves_boundary_overlay_strings(self) -> bool {
        matches!(
            self,
            Self::Hidden {
                origin: InvisiblePropertyOrigin::TextProperty,
                ..
            }
        )
    }
}

fn layout_total_emacs_byte_end_pos<B: LayoutBufferView>(buffer: &B) -> EmacsBytePos {
    EmacsBytePos::ZERO.add_len(buffer.layout_total_emacs_byte_len())
}

fn clamped_layout_char_pos<B: LayoutBufferView + ?Sized>(buffer: &B, charpos: i64) -> CharPos0 {
    layout_char_pos_from_i64(charpos)
        .unwrap_or(CharPos0::ZERO)
        .min(buffer.layout_point_max_char_pos())
}

fn buffer_charpos_to_emacs_byte_pos<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    charpos: CharPos0,
) -> EmacsBytePos {
    buffer.layout_char_pos_to_emacs_byte_pos(charpos.min(buffer.layout_point_max_char_pos()))
}

fn buffer_i64_charpos_to_emacs_byte_pos<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    charpos: i64,
) -> EmacsBytePos {
    buffer_charpos_to_emacs_byte_pos(buffer, clamped_layout_char_pos(buffer, charpos))
}

fn buffer_emacs_byte_pos_to_charpos<B: LayoutBufferView + ?Sized>(
    buffer: &B,
    bytepos: EmacsBytePos,
) -> usize {
    buffer
        .layout_emacs_byte_pos_to_char_pos(bytepos.min(buffer.layout_point_max_emacs_byte_pos()))
        .get()
        .min(buffer.layout_point_max_char_pos().get())
}

fn clamped_layout_emacs_byte_pos<B: LayoutBufferView>(
    buffer: &B,
    bytepos: i64,
) -> Option<EmacsBytePos> {
    layout_emacs_byte_pos_from_i64(bytepos)
        .map(|pos| pos.min(layout_total_emacs_byte_end_pos(buffer)))
}

fn clamped_layout_emacs_byte_range<B: LayoutBufferView>(
    buffer: &B,
    byte_from: i64,
    byte_to: i64,
) -> Option<EmacsByteRange> {
    let from = clamped_layout_emacs_byte_pos(buffer, byte_from)?;
    let to = clamped_layout_emacs_byte_pos(buffer, byte_to)?;
    (from < to).then(|| EmacsByteRange::new(from, to))
}

fn invisible_status_with_origin(
    prop: Option<Value>,
    spec: Value,
    origin: InvisiblePropertyOrigin,
) -> InvisibleStatus {
    let invisibility = prop.map_or(Invisibility::Visible, |property| {
        text_prop_means_invisible(property, spec)
    });
    InvisibleStatus::from_invisibility(invisibility, origin)
}

impl<'a, B: LayoutBufferView + ?Sized> RustTextPropAccess<'a, B> {
    /// Create a new text property accessor.
    pub fn new(buffer: &'a B) -> Self {
        Self {
            buffer,
            window_id: None,
        }
    }

    /// Create a text property accessor scoped to the redisplay window.
    /// Accessor for a caller that already carries an optional window scope (the
    /// producer's cursor), so it need not branch on `Some`/`None` itself.
    pub fn new_for_optional_window(buffer: &'a B, window_id: Option<u64>) -> Self {
        Self { buffer, window_id }
    }

    pub fn new_for_window(buffer: &'a B, window_id: u64) -> Self {
        Self {
            buffer,
            window_id: Some(window_id),
        }
    }

    /// Check if text at `charpos` is invisible.
    ///
    /// Returns `(status, next_visible_pos)`.
    /// `next_visible_pos` is the next char position where visibility might change.
    /// If no change is found, returns `buffer.zv` as the next boundary.
    pub fn check_invisible(&self, charpos: i64) -> (InvisibleStatus, i64) {
        let status = self.invisible_status_at(charpos);
        let mut next_change = self.next_invisible_boundary(charpos);
        if status.hidden() {
            // GNU `handle_invisible_prop` collapses CONSECUTIVE invisible runs
            // into a single ellipsis even when the `invisible` value changes
            // within the run.  Example: a folded org subtree (overlay
            // `invisible` = `org-fold-outline`, shows ellipsis) that contains an
            // org-link whose URL is separately invisible (`org-link`, no
            // ellipsis) -> three runs of differing `invisible` value but all
            // hidden.  Without collapsing, each run emits its own ellipsis
            // (`... [...]  [...]  [...]`); GNU shows one.  Extend the region over
            // every consecutive hidden position; the ellipsis flag stays that of
            // the run that opened the region (`status`, matching GNU which sets
            // `display_ellipsis` from the entry position).
            let max = self.buffer.layout_point_max_char_pos().get() as i64;
            while next_change < max && self.invisible_status_at(next_change).hidden() {
                next_change = self.next_invisible_boundary(next_change);
            }
        }
        (status, next_change)
    }

    /// Classify a bounded acquisition span with the same visibility rule as
    /// the live iterator. A hidden run must close before `limit`; otherwise
    /// the caller cannot yet know its complete source extent or ellipsis.
    pub(crate) fn invisible_run_before(
        &self,
        charpos: i64,
        limit: i64,
    ) -> Option<(InvisibleStatus, i64)> {
        if charpos >= limit {
            return None;
        }
        let status = self.invisible_status_at(charpos);
        if !status.hidden() {
            return Some((status, charpos));
        }
        // Acquisition already bounds the number of source characters. Avoid
        // check_invisible's intentional coalescing all the way to buffer-end.
        for position in charpos + 1..limit {
            if !self.invisible_status_at(position).hidden() {
                return Some((status, position));
            }
        }
        None
    }

    /// Whether a REPLACING `display` spec applies at `charpos`.
    ///
    /// GNU's handler chain runs `handle_display_prop` BEFORE
    /// `handle_invisible_prop` (`it_props`, xdisp.c:1012-1021), and a replacing
    /// spec returns HANDLED_RETURN (xdisp.c:5974), which makes `handle_stop`
    /// "return immediately to the caller, to continue iteration without calling
    /// any further handlers" - so invisibility is never consulted at a position
    /// whose text a display string or image has already replaced.
    ///
    /// Resolution mirrors [`Self::invisible_status_at`] deliberately: the
    /// highest-precedence overlay carrying `display` wins outright and shadows
    /// the text property. The two questions are decided at the same position by
    /// the same chain, so they must not resolve their properties differently.
    pub fn replacing_display_at(&self, charpos: i64) -> bool {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        self.buffer
            .layout_overlays()
            .highest_priority_overlay_property_value_at_emacs_byte_pos(
                bytepos,
                Value::symbol("display"),
                self.window_id,
            )
            .or_else(|| {
                self.buffer
                    .layout_text_prop_at_emacs_byte_pos(bytepos, Value::symbol("display"))
            })
            .is_some_and(|value| {
                crate::display_property::classify_display_property_at(
                    value,
                    &self.buffer.layout_display_when_conditions(),
                    crate::display_property::DisplayPropertyObject::Buffer,
                    self.buffer.layout_display_target(),
                    crate::display_when::DisplayWhenLookup::buffer_at_char_pos(charpos),
                )
                .replacement()
                .is_some()
            })
    }

    /// Whether `[char_from, char_to)` contains an effective property that can
    /// collapse source rows during the display walk.
    ///
    /// This predicate is range- and semantics-aware.  A face-only overlay, an
    /// out-of-range overlay, a non-replacing `display` value, or an `invisible`
    /// value disabled by `buffer-invisibility-spec` cannot make source-line
    /// counting over-estimate display rows and therefore must not force
    /// screen-at-a-time visibility retries.
    fn has_display_row_collapse_between(&self, char_from: i64, char_to: i64) -> bool {
        let max = self.buffer.layout_point_max_char_pos().get() as i64;
        let mut charpos = char_from.clamp(0, max);
        let end = char_to.clamp(charpos, max);
        let end_byte = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, end);
        let display = Value::symbol("display");

        while charpos < end {
            if self.invisible_status_at(charpos).hidden() || self.replacing_display_at(charpos) {
                return true;
            }

            let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
            let next_display_text = self
                .buffer
                .layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
                    bytepos, display, end_byte,
                )
                .map(|next| buffer_emacs_byte_pos_to_charpos(self.buffer, next) as i64)
                .unwrap_or(end);
            // `next_invisible_boundary` includes every overlay endpoint, so it
            // is also the boundary at which an overlay `display` winner may
            // change. It separately tracks invisible text-property changes.
            let next = self
                .next_invisible_boundary(charpos)
                .min(next_display_text)
                .min(end);
            charpos = next.max(charpos.saturating_add(1));
        }

        false
    }

    /// Combined `invisible` status at `charpos` from the `invisible` text
    /// property and the highest-priority overlay (GNU `invisible_p`).
    pub(crate) fn invisible_status_at(&self, charpos: i64) -> InvisibleStatus {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        let spec = self
            .buffer
            .layout_buffer_local_value(LayoutVar::BufferInvisibilitySpec)
            .unwrap_or(Value::T);
        // GNU `handle_invisible_prop` resolves `invisible` through
        // `get_char_property_and_overlay (pos, Qinvisible, it->window)`: the
        // highest-precedence overlay carrying `invisible` wins OUTRIGHT and
        // shadows the text property, and its value is then judged against
        // `buffer-invisibility-spec`.
        //
        // Hand-rolling that policy produced two rendering bugs, both of which hid
        // text GNU shows: scanning on PAST the winner until some overlay happened
        // to say "hidden" (so a low-priority `invisible` overrode a
        // higher-priority overlay whose value is not in the spec), and consulting
        // the text property FIRST (so a text property hid text that a covering
        // overlay declared visible).
        let overlay_value = self
            .buffer
            .layout_overlays()
            .highest_priority_overlay_property_value_at_emacs_byte_pos(
                bytepos,
                Value::symbol("invisible"),
                self.window_id,
            );
        let (value, origin) = if let Some(value) = overlay_value {
            (Some(value), InvisiblePropertyOrigin::Overlay)
        } else {
            (
                self.buffer
                    .layout_text_prop_at_emacs_byte_pos(bytepos, Value::symbol("invisible")),
                InvisiblePropertyOrigin::TextProperty,
            )
        };
        invisible_status_with_origin(value, spec, origin)
    }

    /// Next position where the `invisible` property changes, combining the
    /// `invisible` text-property extent with overlay boundaries (the text-prop
    /// half of GNU `next_single_char_property_change(pos, Qinvisible, ...)`).
    /// Scanning only `invisible` (not *any* property) avoids fragmenting a
    /// contiguous invisible region at every `face` change in a fontified buffer.
    fn next_invisible_boundary(&self, charpos: i64) -> i64 {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        // Bound the `invisible` scan to a window-sized distance ahead instead of
        // walking the whole interval tree to buffer-end. The checkpoint cache
        // (`InvisibleTextScanCheckpoint`) only re-scans once the walk reaches the
        // returned boundary, so a soft boundary at `bytepos + LIMIT` just makes
        // the next re-scan happen a screenful later -- the invisible status at
        // every position is still looked up exactly, so nothing rendered changes.
        // Turns this per-redisplay scan from O(buffer) into O(window).
        // Mirrors GNU's TEXT_PROP_DISTANCE_LIMIT cap in `compute_stop_pos`.
        let scan_limit = EmacsBytePos::new(bytepos.get().saturating_add(INVISIBLE_SCAN_BYTE_LIMIT));
        let next_text_change = self
            .buffer
            .layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
                bytepos,
                Value::symbol("invisible"),
                scan_limit,
            )
            .map(|next| buffer_emacs_byte_pos_to_charpos(self.buffer, next))
            .unwrap_or_else(|| self.buffer.layout_point_max_char_pos().get());
        let next_overlay_change = self
            .buffer
            .layout_overlays()
            .next_boundary_after_emacs_byte_pos(bytepos)
            .map(|next| buffer_emacs_byte_pos_to_charpos(self.buffer, next))
            .unwrap_or_else(|| self.buffer.layout_point_max_char_pos().get());
        next_text_change.min(next_overlay_change) as i64
    }

    /// Next overlay start/end boundary strictly after `charpos`, if any.
    /// Overlay before/after-strings anchor exactly at overlay start and end
    /// positions, so scanning for overlay strings only needs to probe these
    /// boundaries, not every character.
    pub fn next_overlay_boundary_charpos_after(&self, charpos: i64) -> Option<i64> {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        self.buffer
            .layout_overlays()
            .next_boundary_after_emacs_byte_pos(bytepos)
            .map(|next| buffer_emacs_byte_pos_to_charpos(self.buffer, next) as i64)
    }

    /// Collect overlay before-string and after-string at `charpos`.
    ///
    /// Before-strings come from overlays starting at charpos.
    /// After-strings come from overlays ending at charpos.
    ///
    /// Returns `(before_strings, after_strings)` where each entry preserves the
    /// Lisp string object.  GNU `reseat_to_string' keeps string intervals live
    /// for overlay strings, so redisplay must not flatten these to bytes before
    /// the layout iterator has handled text properties such as `display'.
    /// Collect the overlay before/after-strings active at `charpos` into ONE
    /// list ordered by GNU's `compare_overlay_entries` (`src/xdisp.c`): after-
    /// strings come in front of before-strings from *different* overlays, before-
    /// strings precede after-strings of the *same* overlay, before-strings sort
    /// by ascending priority and after-strings by descending priority. Returns a
    /// single interleaved list (not separate before/after lists) so consumers can
    /// render them in GNU's exact visual order.
    pub fn overlay_strings_at(&self, charpos: i64) -> Vec<OverlayDisplayString> {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        let mut entries = Vec::new();

        // GNU `load_overlay_strings' (`src/xdisp.c') scans overlays that
        // start or end at the iterator position, not only overlays covering
        // the position.  Zero-length completion overlays sit at point/EOB and
        // carry their displayed candidates in `before-string', so `overlays_at'
        // would miss exactly the strings redisplay must show.
        let scan_range = EmacsByteRange::new(
            bytepos.saturating_sub_len(EmacsByteLen::new(1)),
            bytepos.add_len(EmacsByteLen::new(1)),
        );
        let mut overlay_ids = self
            .buffer
            .layout_overlays()
            .overlays_in_emacs_byte_range(scan_range);
        overlay_ids.sort();
        overlay_ids.dedup();

        for oid in overlay_ids {
            if !self.overlay_applies_to_window(oid) {
                continue;
            }
            let overlay_start_byte = self
                .buffer
                .layout_overlays()
                .overlay_start_emacs_byte_pos(oid);
            let starts_here = overlay_start_byte == Some(bytepos);
            let ends_here = self
                .buffer
                .layout_overlays()
                .overlay_end_emacs_byte_pos(oid)
                == Some(bytepos);
            // "Skip this overlay if it doesn't start or end at IT's current
            // position" (xdisp.c:7152-7155). The scan range is a window AROUND
            // the position, so most of what it returns anchors nothing and must
            // not pay for the property reads below.
            if !starts_here && !ends_here {
                continue;
            }
            let priority = overlay_string_priority(oid);
            let Some(overlay_start_charpos) = overlay_start_byte
                .map(|start| self.buffer.layout_emacs_byte_pos_to_char_pos(start))
            else {
                continue;
            };
            // GNU: "If the text ``under'' the overlay is invisible, both before-
            // and after-strings from this overlay are visible; start and end
            // position are indistinguishable" (xdisp.c:7157-7173). The iterator
            // never stops inside the hidden text, so this is what carries an
            // invisible overlay's before-string to the one position that is
            // still displayed.
            let hides_its_text = self.overlay_hides_its_own_text(oid);

            if (starts_here || (ends_here && hides_its_text))
                && let Some(string) = self.displayable_overlay_string(oid, "before-string")
            {
                entries.push(OverlayDisplayString {
                    string,
                    overlay_id: oid,
                    overlay_start_charpos,
                    after_string_p: false,
                    priority,
                });
            }

            if (ends_here || (starts_here && hides_its_text))
                && let Some(string) = self.displayable_overlay_string(oid, "after-string")
            {
                entries.push(OverlayDisplayString {
                    string,
                    overlay_id: oid,
                    overlay_start_charpos,
                    after_string_p: true,
                    priority,
                });
            }
        }

        // Stable insertion sort by GNU compare_overlay_entries. A MANUAL sort is
        // used (not slice::sort_by) because compare_overlay_entries is NOT a
        // total order: a zero-length overlay carrying both a before- and an
        // after-string can create a comparison cycle, which GNU's qsort tolerates
        // but Rust's sort_by may reject with a panic. Insertion sort applies the
        // pairwise rules without requiring transitivity and only moves an element
        // when it is strictly Less than its predecessor (so equal/cyclic entries
        // keep collection order). Overlay-string counts at a position are tiny, so
        // O(n^2) is irrelevant.
        for i in 1..entries.len() {
            let mut j = i;
            while j > 0 && compare_overlay_entries(&entries[j], &entries[j - 1]) == Ordering::Less {
                entries.swap(j, j - 1);
                j -= 1;
            }
        }
        entries
    }

    /// GNU `TEXT_PROP_MEANS_INVISIBLE` applied to the overlay's OWN `invisible`
    /// property (xdisp.c:7168-7169): does this overlay hide the text it covers?
    ///
    /// The overlay's own value is read directly, exactly as GNU's
    /// `Foverlay_get (overlay, Qinvisible)` does — this is not the resolved
    /// "is the text at this position invisible" question that
    /// [`invisible_status_at`](Self::invisible_status_at) answers by letting the
    /// highest-priority overlay win, because the rule is about THIS overlay's
    /// own endpoints collapsing.
    fn overlay_hides_its_own_text(&self, overlay_id: Value) -> bool {
        let Some(invisible) = self
            .buffer
            .layout_overlays()
            .overlay_get_named(overlay_id, Value::symbol("invisible"))
        else {
            return false;
        };
        let spec = self
            .buffer
            .layout_buffer_local_value(LayoutVar::BufferInvisibilitySpec)
            .unwrap_or(Value::T);
        invisible_status_with_origin(Some(invisible), spec, InvisiblePropertyOrigin::Overlay)
            .hidden()
    }

    /// The overlay's `before-string` / `after-string` if it is something GNU
    /// would display: `STRINGP (str) && SCHARS (str)` (xdisp.c:7171-7182).
    ///
    /// Both rejections belong HERE rather than downstream. A position whose only
    /// contribution is an empty string is not an anchor at all, and treating it
    /// as one costs a run boundary, a produced element and a route refusal for a
    /// string that will render nothing.
    fn displayable_overlay_string(&self, overlay_id: Value, property: &str) -> Option<Value> {
        let value = self
            .buffer
            .layout_overlays()
            .overlay_get_named(overlay_id, Value::symbol(property))?;
        let string = value.as_lisp_string()?;
        (!string.as_bytes().is_empty()).then_some(value)
    }

    /// Test-only helper: split the interleaved `overlay_strings_at` list back
    /// into (before-strings, after-strings), each preserving its GNU within-kind
    /// order (before ascending priority, after descending priority).
    #[cfg(test)]
    pub fn overlay_strings_split_at(
        &self,
        charpos: i64,
    ) -> (Vec<OverlayDisplayString>, Vec<OverlayDisplayString>) {
        let entries = self.overlay_strings_at(charpos);
        let before = entries
            .iter()
            .copied()
            .filter(|e| !e.after_string_p)
            .collect();
        let after = entries
            .iter()
            .copied()
            .filter(|e| e.after_string_p)
            .collect();
        (before, after)
    }

    fn overlay_applies_to_window(&self, overlay_id: Value) -> bool {
        overlay_applies_to_window(self.buffer, overlay_id, self.window_id)
    }

    /// Get the next position where any text property changes.
    ///
    /// Test-only helper for direct property-table regression coverage.
    #[cfg(test)]
    pub fn next_property_change(&self, charpos: i64) -> i64 {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        self.buffer
            .layout_next_text_prop_change_after_emacs_byte_pos(bytepos)
            .map(|next| buffer_emacs_byte_pos_to_charpos(self.buffer, next))
            .unwrap_or_else(|| self.buffer.layout_point_max_char_pos().get()) as i64
    }

    /// Get a specific text property at a position.
    pub fn get_property(&self, charpos: i64, name: Value) -> Option<Value> {
        let bytepos = buffer_i64_charpos_to_emacs_byte_pos(self.buffer, charpos);
        self.buffer
            .layout_text_prop_at_emacs_byte_pos(bytepos, name)
    }
}

/// Rust port of GNU `compare_overlay_entries` (`src/xdisp.c`). Orders overlay
/// before/after-strings into one visual sequence:
/// - differing kind: same overlay → before-string precedes after-string;
///   different overlays → after-string precedes before-string;
/// - same kind, differing priority: after-strings sort by *decreasing* priority,
///   before-strings by *increasing* priority;
/// - else equal (a stable sort keeps collection order).
fn compare_overlay_entries(e1: &OverlayDisplayString, e2: &OverlayDisplayString) -> Ordering {
    if e1.after_string_p != e2.after_string_p {
        if e1.overlay_id.bits() == e2.overlay_id.bits() {
            // Same overlay: before-string in front of after-string.
            if e1.after_string_p {
                Ordering::Greater
            } else {
                Ordering::Less
            }
        } else {
            // Different overlays: after-strings in front of before-strings.
            if e1.after_string_p {
                Ordering::Less
            } else {
                Ordering::Greater
            }
        }
    } else if e1.priority != e2.priority {
        if e1.after_string_p {
            // After-strings: decreasing priority.
            e2.priority.cmp(&e1.priority)
        } else {
            // Before-strings: increasing priority.
            e1.priority.cmp(&e2.priority)
        }
    } else {
        Ordering::Equal
    }
}

fn overlay_string_priority(overlay: Value) -> i64 {
    let Some(data) = overlay.as_overlay_data() else {
        return 0;
    };
    let Some(priority) =
        neovm_core::emacs_core::plist::plist_get(data.plist, &Value::symbol("priority"))
    else {
        return 0;
    };
    match priority.kind() {
        ValueKind::Fixnum(n) => n,
        _ => 0,
    }
}

// ---------------------------------------------------------------------------
// ResolvedFace — pure-Rust equivalent of FaceDataFFI
// ---------------------------------------------------------------------------

/// Convert a neovm-core `Color` to a packed sRGB pixel (0x00RRGGBB).
fn color_to_pixel(c: &NeoColor) -> u32 {
    c.to_pixel()
}

/// Check if two colors are perceptually close.
///
/// GNU Emacs uses this for `:distant-foreground`: when the foreground
/// is too similar to the background, swap to the distant foreground
/// for readability.  Uses simple RGB distance threshold.
fn colors_close(a: u32, b: u32) -> bool {
    let ar = (a >> 16) & 0xFF;
    let ag = (a >> 8) & 0xFF;
    let ab = a & 0xFF;
    let br = (b >> 16) & 0xFF;
    let bg = (b >> 8) & 0xFF;
    let bb = b & 0xFF;
    let dr = ar.abs_diff(br);
    let dg = ag.abs_diff(bg);
    let db = ab.abs_diff(bb);
    // Weighted Euclidean distance (human perception weights R more than B)
    // Threshold ~30 in each channel ≈ 2700 squared distance
    (dr * dr * 3 + dg * dg * 4 + db * db * 2) < 3000
}

/// Resolve a `:stipple` string — a built-in bitmap name (`gray3`, …) or an XBM
/// file — to a [`StipplePattern`](neomacs_display_protocol::StipplePattern),
/// mirroring GNU's `image_create_bitmap_from_file`. Built-ins are portable and
/// need no I/O; files are read and parsed once, then cached, because face
/// realization runs during layout and must never touch disk per frame. The
/// `x-bitmap-file-path` Lisp search list is not reachable from the layout
/// bridge; explicit paths and the standard X11 bitmap directories are searched.
fn stipple_from_name_or_file(name: &str) -> Option<neomacs_display_protocol::StipplePattern> {
    use neomacs_display_protocol::StipplePattern;
    if let Some(pattern) = StipplePattern::builtin(name) {
        return Some(pattern);
    }
    static CACHE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, Option<StipplePattern>>>,
    > = std::sync::OnceLock::new();
    let cache = CACHE.get_or_init(std::sync::Mutex::default);
    if let Ok(guard) = cache.lock()
        && let Some(hit) = guard.get(name)
    {
        return hit.clone();
    }
    let pattern = stipple_file_search_paths(name)
        .into_iter()
        .find_map(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| StipplePattern::from_xbm_source(&text));
    if let Ok(mut guard) = cache.lock() {
        guard.insert(name.to_string(), pattern.clone());
    }
    pattern
}

/// Candidate paths for a `:stipple` bitmap file name. An absolute or
/// cwd-relative path is used directly; a bare name is also looked up in the
/// standard X11 bitmap directories (GNU's `x-bitmap-file-path` default subset).
fn stipple_file_search_paths(name: &str) -> Vec<std::path::PathBuf> {
    let mut paths = vec![std::path::PathBuf::from(name)];
    if !std::path::Path::new(name).is_absolute() {
        for dir in [
            "/usr/include/X11/bitmaps",
            "/usr/share/X11/bitmaps",
            "/usr/X11R6/include/X11/bitmaps",
        ] {
            paths.push(std::path::Path::new(dir).join(name));
        }
    }
    paths
}

/// Resolved face attributes ready for the layout engine.
///
/// This is the neovm-core equivalent of `FaceDataFFI`.  All attributes are
/// fully realized (no `Option`s) so the layout engine can use them directly.
#[derive(Clone, Debug)]
pub struct ResolvedFace {
    /// Foreground color (sRGB pixel: 0x00RRGGBB).
    pub fg: u32,
    /// Background color (sRGB pixel: 0x00RRGGBB).
    pub bg: u32,
    /// What a TERMINAL frame writes for `fg`: the index `tty-color-desc`
    /// returned, which is the whole of GNU's `face->foreground` on a tty
    /// (`map_tty_color`, src/xfaces.c:6640-6648).  `None` on a GUI frame, and
    /// on a tty for a colour the palette could not resolve -- GNU's
    /// `FACE_TTY_DEFAULT_COLOR`, which `turn_on_face` emits nothing for.
    pub terminal_fg: Option<TerminalColor>,
    /// What a TERMINAL frame writes for `bg`; see [`Self::terminal_fg`].
    pub terminal_bg: Option<TerminalColor>,
    /// Use the terminal's default foreground instead of `fg`.
    pub use_default_foreground: bool,
    /// Use the terminal's default background instead of `bg`.
    pub use_default_background: bool,
    /// Font family name.
    pub font_family: String,
    /// Font weight (CSS 100-900).
    pub font_weight: u16,
    /// Italic flag.
    pub italic: bool,
    /// Font size in pixels.
    pub font_size: f32,
    /// Underline style, GNU's `enum face_underline_type`
    /// (src/dispextern.h:1760-1765): 0=none, 1=line, 2=double-line, 3=wave,
    /// 4=dots, 5=dashes.
    pub underline_style: u8,
    /// GNU underline placement semantics retained until glyph-row layout.
    pub underline_position: NeoUnderlinePosition,
    /// Underline color (sRGB pixel, 0 = use foreground).
    pub underline_color: u32,
    /// What a TERMINAL frame writes for the underline colour: GNU's
    /// `face->underline_color` (src/dispextern.h:1811), which
    /// `realize_tty_face` fills through `map_tty_color` (src/xfaces.c:6748,
    /// :6777) exactly as it fills the foreground and the background.
    ///
    /// This is a different quantity from [`Self::underline_color`], not a
    /// different spelling of it.  That one defaults to `fg` so the GUI draws a
    /// plain `:underline t` in the text colour; GNU's terminal slot stays 0 in
    /// that case and `turn_on_face` emits no underline colour at all
    /// (src/term.c:2120).  `None` here is that 0.
    pub terminal_underline_color: Option<TerminalColor>,
    /// Strike-through enabled.
    pub strike_through: bool,
    /// Strike-through color (sRGB pixel, 0 = use foreground).
    pub strike_through_color: u32,
    /// Overline enabled.
    pub overline: bool,
    /// Overline color (sRGB pixel, 0 = use foreground).
    pub overline_color: u32,
    /// Box type (0=none, 1=line).
    pub box_type: u8,
    /// Box color (sRGB pixel).
    pub box_color: u32,
    /// GNU scalar box line width, including inside/outside sign semantics.
    pub box_line_width: BoxLineWidth,
    /// Extend background to end of line.
    pub extend: bool,
    /// Simulate bold by drawing twice at x and x+1.
    pub overstrike: bool,
    /// Preserve terminal inverse-video when both colors are terminal defaults.
    pub terminal_inverse_video: bool,
    /// Original color attributes retained across merges; paint colors are
    /// intentionally not reversible (inverse defaults and distant colors).
    color_state: FaceColorState,
    /// Fully merged logical attributes, before terminal color callbacks.
    /// The owning frame/layout attempt confines this immutable snapshot to its
    /// mutator; Arc shares strings and attributes without sharing Lisp mutation.
    source_attributes: Option<std::sync::Arc<RootedFace>>,
    /// Canonical forced basic realization, distinct even when its logical
    /// attributes equal another basic role's. Every logical merge clears it.
    canonical_tty_role: Option<BasicFace>,
    /// Per-face measured character advance width (from FontMetricsService, 0.0 = use default).
    font_char_width: f32,
    /// Per-face font ascent (from FontMetricsService, 0.0 = use default).
    pub font_ascent: f32,
    /// Per-face line height (from FontMetricsService, 0.0 = use default).
    pub font_line_height: f32,
    /// Face cache ID — matches [`BasicFaceId`] for basic faces (0–19)
    /// or a dynamically allocated ID (≥20) for other faces.
    pub face_id: u32,
    /// Lisp face name this face was resolved from, when it came from a
    /// named face (GNU keeps the name reachable via `struct face::lface`).
    /// `None` for anonymous attribute-plist faces.
    pub lisp_name: Option<String>,
    /// Realized `:stipple` bitmap, if the face specified one. GNU realizes the
    /// `LFACE_STIPPLE_INDEX` spec to a pixmap id in `face->stipple`; neomacs
    /// realizes it directly to the XBM `StipplePattern` the renderer tiles.
    pub stipple: Option<neomacs_display_protocol::StipplePattern>,
}

impl Default for ResolvedFace {
    fn default() -> Self {
        Self {
            fg: 0x00000000,
            bg: 0x00FFFFFF,
            terminal_fg: None,
            terminal_bg: None,
            use_default_foreground: false,
            use_default_background: false,
            font_family: String::new(),
            font_weight: 400,
            italic: false,
            font_size: 14.0,
            underline_style: 0,
            underline_position: NeoUnderlinePosition::FontMetric,
            underline_color: 0,
            terminal_underline_color: None,
            strike_through: false,
            strike_through_color: 0,
            overline: false,
            overline_color: 0,
            box_type: 0,
            box_color: 0,
            box_line_width: BoxLineWidth::default(),
            extend: false,
            overstrike: false,
            terminal_inverse_video: false,
            color_state: FaceColorState::Seed,
            source_attributes: None,
            canonical_tty_role: None,
            font_char_width: 0.0,
            font_ascent: 0.0,
            font_line_height: 0.0,
            face_id: 0, // DEFAULT_FACE_ID
            lisp_name: None,
            stipple: None,
        }
    }
}

/// GNU compares lface attributes `EQ` first and only falls back to `equal`
/// per slot (`face_attr_equal_p`, src/xfaces.c:4483-4497), because a
/// re-derived face re-reads the same definition objects and identical slots
/// are the overwhelmingly common verdict. [`ResolvedFace`] carries the same
/// shortcut structurally: `source_attributes` is the interned merged source,
/// and every consumer of one composition shares one `Arc`, so allocation
/// identity proves content identity and skips the whole `Face`/`Value` walk.
/// Distinct allocations still compare by content -- a face id must stay
/// stable for content-equal sources across transactions, and those re-fold
/// into fresh allocations whenever a definition change retired the cache.
impl PartialEq for ResolvedFace {
    fn eq(&self, other: &Self) -> bool {
        let shared_source = match (&self.source_attributes, &other.source_attributes) {
            (Some(left), Some(right)) => std::sync::Arc::ptr_eq(left, right),
            (None, None) => true,
            (Some(_), None) | (None, Some(_)) => return false,
        };
        self.fg == other.fg
            && self.bg == other.bg
            && self.terminal_fg == other.terminal_fg
            && self.terminal_bg == other.terminal_bg
            && self.use_default_foreground == other.use_default_foreground
            && self.use_default_background == other.use_default_background
            && self.font_family == other.font_family
            && self.font_weight == other.font_weight
            && self.italic == other.italic
            && self.font_size == other.font_size
            && self.underline_style == other.underline_style
            && self.underline_position == other.underline_position
            && self.underline_color == other.underline_color
            && self.terminal_underline_color == other.terminal_underline_color
            && self.strike_through == other.strike_through
            && self.strike_through_color == other.strike_through_color
            && self.overline == other.overline
            && self.overline_color == other.overline_color
            && self.box_type == other.box_type
            && self.box_color == other.box_color
            && self.box_line_width == other.box_line_width
            && self.extend == other.extend
            && self.overstrike == other.overstrike
            && self.terminal_inverse_video == other.terminal_inverse_video
            && self.color_state == other.color_state
            && self.canonical_tty_role == other.canonical_tty_role
            && self.font_char_width == other.font_char_width
            && self.font_ascent == other.font_ascent
            && self.font_line_height == other.font_line_height
            && self.face_id == other.face_id
            && self.lisp_name == other.lisp_name
            && self.stipple == other.stipple
            && (shared_source || self.source_attributes == other.source_attributes)
    }
}

impl ResolvedFace {
    fn install_realized_tty_colors(&mut self, realized: &NeoFace) {
        // Read the retained pre-inverse channel state, never reverse-engineer
        // source colors from this face's already-inverted paint.
        FaceColorAttributes::from_base(self)
            .merge_terminal_realization(realized)
            .realize()
            .install(self);
        self.terminal_underline_color = realized
            .underline
            .enabled()
            .and_then(|underline| underline.color.as_ref())
            .and_then(|color| color.terminal);
        if let Some(underline) = realized.underline.enabled() {
            self.underline_color = underline
                .color
                .as_ref()
                .map(color_to_pixel)
                .unwrap_or(self.fg);
        }
    }

    fn retain_source_over(
        &mut self,
        resolver: &FaceResolver,
        base: &ResolvedFace,
        overlay: &NeoFace,
        overlay_identity: Option<&std::sync::Arc<NeoFace>>,
    ) {
        self.canonical_tty_role = None;
        let Some(source) = base.source_attributes.clone() else {
            return;
        };
        if let Some(identity) = overlay_identity
            && let Some(shared) = resolver.cached_composed_identity_over(&source, identity)
        {
            self.source_attributes = Some(shared);
            return;
        }
        if let Some(shared) = resolver.cached_composed_over(&source, overlay) {
            if let Some(identity) = overlay_identity {
                resolver.record_composed_identity_over(source, identity.clone(), shared.clone());
            }
            self.source_attributes = Some(shared);
            return;
        }
        let mut merged = source.attributes().merge(overlay);
        merged.inherit = None;
        merged.doc = None;
        let Some(shared) = resolver.shared_composed_source(merged, Some(&source)) else {
            self.source_attributes = None;
            return;
        };
        resolver.record_composed_over(source.clone(), overlay.clone(), shared.clone());
        if let Some(identity) = overlay_identity {
            resolver.record_composed_identity_over(source, identity.clone(), shared.clone());
        }
        self.source_attributes = Some(shared);
    }

    pub(crate) fn has_same_color_source(&self, other: &Self) -> bool {
        FaceColorAttributes::from_base(self) == FaceColorAttributes::from_base(other)
    }

    /// Identity fast path for repeated checkpoint probes: one interned
    /// composed source shared by allocation, plus the entry state that
    /// realization reads.
    ///
    /// GNU's frame face cache keys a realized face by its merged lface
    /// vector — `lookup_face` hashes and compares the lface, never the
    /// realized `struct face` (src/xfaces.c:5064-5075) — because every
    /// realized field is a deterministic function of that merged lface.
    /// Allocation identity of an interned source is the strict refinement of
    /// the same key: the source IS the merged content. Two fields escape
    /// that implication and carry their own comparison — `font_size` may
    /// fold a relative `:height` against different base faces, and
    /// `lisp_name` is assigned by the named-face resolver after interning.
    /// Faces without a retained source answer `false`; callers fall back to
    /// full equality.
    pub(crate) fn shares_resolved_identity(&self, other: &Self) -> bool {
        let (Some(left), Some(right)) = (&self.source_attributes, &other.source_attributes) else {
            return false;
        };
        std::sync::Arc::ptr_eq(left, right)
            && self.font_size == other.font_size
            && self.lisp_name == other.lisp_name
            && self.canonical_tty_role == other.canonical_tty_role
            && self.color_state == other.color_state
            && self.underline_color == other.underline_color
            && self.terminal_underline_color == other.terminal_underline_color
    }

    /// Typed view of this bridge-side face id. `ResolvedFace.face_id` stays a
    /// raw u32 (the neovm bridge boundary keeps raw reprs); this is THE
    /// conversion point where ids leave the bridge as [`FaceId`].
    pub(crate) fn display_face_id(&self) -> FaceId {
        FaceId::new(self.face_id)
    }

    /// Store a typed face id back into the raw bridge-side field.
    pub(crate) fn set_display_face_id(&mut self, id: FaceId) {
        self.face_id = id.get();
    }

    pub(crate) fn measured_char_width_px(&self) -> f32 {
        self.font_char_width
    }

    pub(crate) fn set_measured_char_width_px(&mut self, width: f32) {
        self.font_char_width = width;
    }

    /// Assign GNU's whole realized foreground from one realized colour.
    ///
    /// The pixel and the terminal index are two readings of the same
    /// `map_tty_color` result (src/xfaces.c:6620-6694), so they are assigned
    /// together and never separately -- a site that set one and forgot the
    /// other would put the writer back to guessing an index from RGB.
    fn set_foreground(&mut self, color: &NeoColor) {
        self.canonical_tty_role = None;
        self.fg = color_to_pixel(color);
        self.terminal_fg = color.terminal;
        self.use_default_foreground = false;
    }

    /// Assign GNU's whole realized background from one realized colour.
    fn set_background(&mut self, color: &NeoColor) {
        self.canonical_tty_role = None;
        self.bg = color_to_pixel(color);
        self.terminal_bg = color.terminal;
        self.use_default_background = false;
    }

    /// The frame's fallback foreground, used when the default face specifies
    /// none.  It never came from `tty-color-desc`, so it carries no terminal
    /// colour: GNU's `FACE_TTY_DEFAULT_FG_COLOR`, which `turn_on_face` emits
    /// no colour for at all.
    fn set_frame_default_foreground(&mut self, pixel: u32) {
        self.canonical_tty_role = None;
        self.fg = pixel;
        self.terminal_fg = None;
        self.use_default_foreground = true;
    }

    /// The frame's fallback background; see
    /// [`Self::set_frame_default_foreground`].
    fn set_frame_default_background(&mut self, pixel: u32) {
        self.canonical_tty_role = None;
        self.bg = pixel;
        self.terminal_bg = None;
        self.use_default_background = true;
    }
}

// ---------------------------------------------------------------------------
// FaceResolver
// ---------------------------------------------------------------------------

/// Outcome of testing whether a face-spec list is a GNU
/// `(:filtered FILTER SPEC)` form and, if so, whether FILTER matches.
enum FilteredFaceSpec {
    /// Not a `:filtered` form at all — the caller handles `items` as an inline
    /// face plist or a list of face specs.
    NotFiltered,
    /// A `:filtered` form whose FILTER did NOT match (or is malformed). The
    /// wrapped SPEC must be DROPPED — contribute no attributes — never
    /// re-interpreted as an inline plist. GNU `evaluate_face_filter` returns
    /// false here and the spec is skipped.
    Rejected,
    /// A `:filtered` form whose FILTER matched — the caller recurses into the
    /// unwrapped SPEC.
    Matched(Vec<Value>),
}

/// One source in GNU's `face_at_buffer_position` merge order.
///
/// Keeping text and overlay sources distinct makes the ordering contract
/// explicit at call sites: the text property is lower precedence, followed by
/// overlays in ascending `sort_overlays` order.
#[derive(Clone, Copy, Debug)]
enum OrderedFaceSource {
    TextProperty(Value),
    Overlay(Value),
}

/// Logical face sources which must be merged before terminal realization.
///
/// GNU accumulates all lface attributes first and calls `lookup_face` once.
/// In particular, `:inverse-video` is not applied between the text property
/// and an overlay.  This type prevents those sources from being passed around
/// as an already-realized [`ResolvedFace`] chain.
#[derive(Clone, Debug, Default)]
pub(crate) struct OrderedFaceSources {
    sources: Vec<OrderedFaceSource>,
}

impl OrderedFaceSources {
    pub(crate) fn from_text_and_overlays(
        text_property: Option<Value>,
        overlays_ascending: Vec<Value>,
    ) -> Self {
        let mut sources = Vec::with_capacity(
            usize::from(text_property.is_some()).saturating_add(overlays_ascending.len()),
        );
        if let Some(value) = text_property {
            sources.push(OrderedFaceSource::TextProperty(value));
        }
        sources.extend(
            overlays_ascending
                .into_iter()
                .map(OrderedFaceSource::Overlay),
        );
        Self { sources }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.sources.is_empty()
    }

    pub(crate) fn single_value(&self) -> Option<Value> {
        if self.sources.len() == 1 {
            self.values().next()
        } else {
            None
        }
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = Value> + '_ {
        self.sources.iter().map(|source| match source {
            OrderedFaceSource::TextProperty(value) | OrderedFaceSource::Overlay(value) => *value,
        })
    }
}

/// One resolved face source: its logical attributes plus, when the source is
/// a single named face folded in this resolver, the shared specification it
/// was folded into. The identity lets a repeated composition be recognized by
/// pointer equality instead of re-digesting the content.
struct FaceSpecContribution {
    face: NeoFace,
    identity: Option<std::sync::Arc<NeoFace>>,
}

impl FaceSpecContribution {
    fn anonymous(face: NeoFace) -> Self {
        Self {
            face,
            identity: None,
        }
    }
}

/// Accumulates GNU lface attributes without realizing colors or decorations.
///
/// `ResolvedFace` is deliberately absent from the stored state.  Consequently
/// inverse-video, distant-foreground, and terminal-default color mapping can
/// only run in [`Self::realize`], after every source has contributed.
#[derive(Default)]
struct UnresolvedFaceComposition {
    attributes: Option<NeoFace>,
    identity: Option<std::sync::Arc<NeoFace>>,
}

/// GNU's face merger reports invalid references only at the display-property
/// boundary.  Once a valid named face has been entered, its remapping and
/// stored `:inherit` graph are merged with `err_msgs=false`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceReferenceDiagnostics {
    Report,
    Suppress,
}

impl UnresolvedFaceComposition {
    /// The identity survives only a single-source composition; once a second
    /// source merges in, the accumulated attributes no longer equal any one
    /// shared specification.
    fn merge_ided(&mut self, contribution: FaceSpecContribution) {
        match self.attributes.take() {
            Some(attributes) => {
                self.attributes = Some(attributes.merge(&contribution.face));
                self.identity = None;
            }
            None => {
                self.attributes = Some(contribution.face);
                self.identity = contribution.identity;
            }
        }
    }

    fn merge_optional(&mut self, contribution: Option<FaceSpecContribution>) {
        if let Some(contribution) = contribution {
            self.merge_ided(contribution);
        }
    }

    fn realize(self, resolver: &FaceResolver, base: &ResolvedFace) -> Option<ResolvedFace> {
        self.attributes.map(|attributes| {
            resolver.apply_specified_face_over(base, &attributes, self.identity.as_ref())
        })
    }
}

/// One real-TTY source snapshot, captured only after core initialized its
/// frame cache. Every face-source invalidation advances the global counter;
/// equal generations therefore need no frame-table identity lookup. A change
/// on another frame may retag this source without invalidating its identity.
/// This immutable token belongs to the resolver's exclusive owning mutator.
#[derive(Clone, Copy)]
struct BoundTtyFaceSource {
    frame_id: FrameId,
    generation: u64,
}

impl BoundTtyFaceSource {
    #[inline]
    fn is_current(self, evaluator: &Context, table: &FaceTable) -> bool {
        self.generation == evaluator.face_change_count()
            || evaluator.display_face_source_table_is_current(self.frame_id, table)
    }
}

enum FaceRealizationSource {
    MergeOverDefault,
    Complete(std::sync::Arc<RootedFace>),
}

/// A render-local capability for pure resolution plus optional cold-entry
/// observation. Ordinary rendering uses FaceResolver itself, so its existing
/// one-pointer services borrow and empty observation specialize unchanged.
pub(crate) trait FaceServices {
    fn resolver(&self) -> &FaceResolver;
    fn realize_for_display(&self, face: &mut ResolvedFace, evaluator: &mut Context);
}

trait ColdFaceEntryObserver {
    fn before_entry(&self);
}

struct NoColdFaceEntryObserver;
impl ColdFaceEntryObserver for NoColdFaceEntryObserver {
    #[inline]
    fn before_entry(&self) {}
}

impl ColdFaceEntryObserver for std::cell::Cell<bool> {
    #[inline]
    fn before_entry(&self) {
        self.set(true);
    }
}

/// This borrowed capability is created only for an actual synchronous Rows
/// producer. The sticky Cell outlives every physical retry of that producer.
pub(crate) struct ObservedFaceServices<'a> {
    resolver: &'a FaceResolver,
    cold_entry: &'a std::cell::Cell<bool>,
}

impl<'a> ObservedFaceServices<'a> {
    pub(crate) fn new(resolver: &'a FaceResolver, cold_entry: &'a std::cell::Cell<bool>) -> Self {
        Self {
            resolver,
            cold_entry,
        }
    }
}

impl FaceServices for ObservedFaceServices<'_> {
    fn resolver(&self) -> &FaceResolver {
        self.resolver
    }

    fn realize_for_display(&self, face: &mut ResolvedFace, evaluator: &mut Context) {
        // Pending hook flow must keep the original placeholder behavior. In
        // the quiescent case a native warm lookup cannot enter user Lisp.
        if !evaluator.redisplay_hook_flow_pending()
            && self.resolver.realize_cached_for_display(face, evaluator)
        {
            return;
        }
        self.resolver
            .realize_with_cold_observer(face, evaluator, self.cold_entry);
    }
}

impl FaceServices for FaceResolver {
    #[inline]
    fn resolver(&self) -> &FaceResolver {
        self
    }

    #[inline]
    fn realize_for_display(&self, face: &mut ResolvedFace, evaluator: &mut Context) {
        FaceResolver::realize_for_display(self, face, evaluator);
    }
}

/// Resolves face attributes at buffer positions using the neovm-core
/// `FaceTable`, text properties, and overlays.
///
/// Replaces the C FFI `face_at_buffer_position()` path for the pure-Rust
/// backend.
pub struct FaceResolver {
    face_table: FaceTable,
    /// The bound TTY snapshot may inspect any definition after a color callback
    /// replaces the live table and collects. Root the whole snapshot once.
    face_table_roots: Option<FaceTableRoots>,
    default_face: ResolvedFace,
    /// Frame ownership is supplied by production layout. Unit resolvers have
    /// no evaluator capability and remain pure. No evaluator or Lisp state is
    /// retained here or in thread-local storage.
    tty_face_source: Option<BoundTtyFaceSource>,
    /// Only real TTY bindings need the large enum-indexed basic projection.
    /// Unbound/default-off/GUI resolvers retain one empty pointer-sized option.
    realized_basic_faces: Option<Box<enum_map::EnumMap<BasicFace, Option<ResolvedFace>>>>,
    /// Window system in use: `None` for TTY, `Some("x")` for X11,
    /// `Some("wayland")` for Wayland, etc.  Used to evaluate
    /// `:filtered` face spec predicates.
    window_system: Option<String>,
    /// The window-parameters `(PARAM . VALUE)` alist of the window whose faces
    /// are currently being resolved. Set per-window (see
    /// `set_current_window_parameters`) so the GNU `(:window PARAM VALUE)`
    /// `:filtered` face predicate can match — the `FaceResolver` is created
    /// once per frame and shared `&` across windows, so this is interior
    /// mutability updated at each window boundary rather than a constructor arg.
    /// Empty for frame chrome / TTY / any non-window context, matching GNU's
    /// "no window ⇒ filter fails".
    current_window_parameters: std::cell::RefCell<Vec<(Value, Value)>>,
    /// Numeric id of the window whose faces are currently being resolved, or
    /// `None` for frame chrome / TTY / any non-window context. Set per-window
    /// alongside [`set_current_window_parameters`]. Used to honor an overlay's
    /// `window` property: GNU restricts such an overlay (e.g. hl-line with a
    /// non-sticky flag, which sets `window` to the selected window) to that one
    /// window, so the same buffer shown in two windows highlights only the
    /// selected one — see the overlay-face loop in `face_at_pos`.
    current_window_id: std::cell::Cell<Option<u64>>,
    /// Diagnostics produced while merging buffer face references. Layout can
    /// retry speculatively, so the resolver records them without mutating the
    /// evaluator; the engine logs only the accepted attempt.
    invalid_face_references: std::cell::RefCell<Vec<String>>,
    /// One shared [`RootedFace`] per distinct composed source, so repeated
    /// compositions of the same logical face compare by `Arc` identity in
    /// every downstream probe, the way GNU's merged lface vectors compare by
    /// EQ on shared slots (src/xfaces.c:4483-4497). Content-keyed: an entry
    /// is a function of its merged attributes, never of their provenance, so
    /// the cache cannot go stale against this resolver's immutable table
    /// snapshot; the cap bounds churning inline-plist faces.
    ///
    /// Held behind an `Rc` shared with the engine: a resolver lives for one
    /// redisplay transaction, but its folded named specifications and merged
    /// sources stay valid for as long as the face table snapshot does — GNU
    /// keeps a frame's whole face cache across redisplays and frees it only
    /// when a definition changes (`face_change`, src/xfaces.c:3098-3105).
    composed_sources: std::rc::Rc<std::cell::RefCell<ComposedSourceCache>>,
    font_sizing: FontSizing,
    frame_font_domain: FrameFontDomain,
}

/// Shared-source interning for face compositions, mirroring GNU's face cache
/// keyed by the merged lface vector (`lookup_face`, src/xfaces.c:5064-5093).
#[derive(Default)]
struct ComposedSourceCache {
    /// Distinct merged sources by content digest.
    entries: rustc_hash::FxHashMap<u64, Vec<std::sync::Arc<RootedFace>>>,
    /// `base + overlay` compositions, so a repeated composition reuses its
    /// merged source without rebuilding it. The base is pinned by a weak
    /// reference: a freed base cannot match, and a live allocation cannot be
    /// confused with another face that later reuses its address.
    compositions: rustc_hash::FxHashMap<(usize, u64), Vec<ComposedOverlayEntry>>,
    /// `base + overlay` compositions keyed by allocation identity, GNU's
    /// `EQ`-first bucket verification (src/xfaces.c:4483-4497): both keys are
    /// pinned by weak references, so a hit is two pointer comparisons.
    identity_compositions: rustc_hash::FxHashMap<(usize, usize), Vec<IdentityComposedEntry>>,
    /// Fully folded overlay specifications per named face — GNU resolves a
    /// named face through the immutable `face_face` frame table every time
    /// (`get_lface_attributes`), which this resolver snapshots once, so the
    /// folded result is a function of the name. Buffer folds carry the
    /// identity of the `face-remapping-alist` object they ran against; see
    /// [`NamedSpecEntry`].
    named_specs: rustc_hash::FxHashMap<Box<str>, NamedSpecEntry>,
    len: usize,
    /// Routed-row speculation depth: insert methods journal their undo
    /// records while this is non-zero (see [`FaceCacheRewind`]). The route
    /// arms before its first face probe and rewinds on any refusal, so a
    /// refused row leaves the caches exactly as it found them.
    speculation_depth: u32,
    speculation_journal: Vec<FaceCacheRewind>,
}

/// One folded named-face specification plus the buffer-local
/// `face-remapping-alist` object it was folded against.
///
/// GNU re-runs its live remapping check on every lookup
/// (`merge_named_face`, src/xfaces.c:3269-3295): an `assq` for the face name
/// against the current alist, then the cached definition fold. The memo
/// keeps exactly that shape — the fold path re-runs the name's `assq` miss
/// check per lookup (a hit takes the remapping branch before the memo is
/// consulted), so the entry only has to record which alist OBJECT the fold
/// saw. Top-level lookups may instead consult the memo before the `assq`
/// (`resolve_buffer_named_face_overlay_spec_inner`): every recorded fold ran
/// with an empty remap stack, so its own `assq` miss is part of what the
/// entry encodes for that alist object. `setq`-replacing the alist (the only
/// mutation `face-remap.el` and
/// friends perform) produces a new list object, misses the recorded bits,
/// and re-folds. In-place splicing an entry for one of the fold's
/// *inherited* names is the one change an identity key cannot observe; the
/// pin's table snapshot has the same identity-implies-content trust for
/// `Rc::make_mut` storage. Pure table folds (`resolve_named_face_overlay_spec`)
/// never read the alist and are recorded under [`ALIST_INDEPENDENT`].
#[derive(Clone)]
struct NamedSpecEntry {
    shared: std::sync::Arc<NeoFace>,
    alist_bits: usize,
}

/// Recorded for folds that never consulted a buffer-local remapping alist.
const ALIST_INDEPENDENT: usize = usize::MAX;

struct ComposedOverlayEntry {
    base: std::sync::Weak<RootedFace>,
    overlay: NeoFace,
    merged: std::sync::Arc<RootedFace>,
}

struct IdentityComposedEntry {
    base: std::sync::Weak<RootedFace>,
    overlay: std::sync::Weak<NeoFace>,
    merged: std::sync::Arc<RootedFace>,
}

impl ComposedSourceCache {
    fn clear(&mut self) {
        self.entries.clear();
        self.compositions.clear();
        self.identity_compositions.clear();
        self.named_specs.clear();
        self.len = 0;
        // The wholesale reset only runs at speculation depth 0 (see the
        // insert methods), where the journal is already empty; clearing it
        // here keeps that invariant explicit rather than accidental.
        self.speculation_journal.clear();
    }
}

/// One undo record of a face-cache insert performed inside a speculative
/// routed-row attempt. GNU lets a retried display keep the faces it
/// realized — the frame face cache empties only on `face_change`
/// (src/xfaces.c:3098-3105) — but a routed row here must be INVISIBLE when
/// refused: the buffer pipeline re-renders it, and the attempt's cache
/// entries would otherwise change which later insert trips the wholesale
/// [`COMPOSED_SOURCE_CACHE_LIMIT`] reset or which paint a one-slot memo
/// still answers for. Records apply in reverse insertion order, so a
/// pushed entry is still the last of its bucket when its undo runs.
enum FaceCacheRewind {
    /// A realized terminal paint recorded beside its source; the previous
    /// slot contents are restored wholesale (the slot is one deep).
    TtyPaint {
        source: std::sync::Arc<RootedFace>,
        previous: Option<face_colors::TtyPaintMemo>,
    },
    /// An interned composed source appended to a digest bucket.
    ComposedEntry {
        hash: u64,
        pushed: std::sync::Arc<RootedFace>,
    },
    /// A content-keyed `base + overlay` composition.
    Composition {
        key: (usize, u64),
        pushed: std::sync::Arc<RootedFace>,
    },
    /// An identity-keyed `base + overlay` composition.
    IdentityComposition {
        key: (usize, usize),
        pushed: std::sync::Arc<RootedFace>,
    },
    /// A folded named-face specification; `previous` restores a replaced
    /// fold, or removes a fresh one.
    NamedSpec {
        name: Box<str>,
        previous: Option<NamedSpecEntry>,
    },
}

impl FaceCacheRewind {
    /// Undo the insert this record was journaled for. Bucket entries are
    /// matched by allocation identity, never by content: an equal-content
    /// entry from before the attempt must survive the rewind.
    fn apply(self, cache: &mut ComposedSourceCache) {
        let removed = match self {
            FaceCacheRewind::TtyPaint { source, previous } => {
                source.rewind_tty_paint(previous);
                return;
            }
            FaceCacheRewind::ComposedEntry { hash, pushed } => cache
                .entries
                .get_mut(&hash)
                .is_some_and(|bucket| remove_by_identity(bucket, &pushed, |entry| entry)),
            FaceCacheRewind::Composition { key, pushed } => cache
                .compositions
                .get_mut(&key)
                .is_some_and(|bucket| remove_by_identity(bucket, &pushed, |entry| &entry.merged)),
            FaceCacheRewind::IdentityComposition { key, pushed } => cache
                .identity_compositions
                .get_mut(&key)
                .is_some_and(|bucket| remove_by_identity(bucket, &pushed, |entry| &entry.merged)),
            FaceCacheRewind::NamedSpec { name, previous } => {
                match previous {
                    Some(entry) => cache.named_specs.insert(name, entry),
                    None => cache.named_specs.remove(&name),
                };
                // The insert always counted one entry, even when it replaced
                // a fold; the undo owes the same one back.
                true
            }
        };
        if removed {
            cache.len = cache.len.saturating_sub(1);
        }
    }
}

/// Drop `pushed` from `bucket` by allocation identity. Bucket order carries
/// no meaning (probes scan linearly), so a swap-remove is exact.
fn remove_by_identity<E>(
    bucket: &mut Vec<E>,
    pushed: &std::sync::Arc<RootedFace>,
    merged_of: impl Fn(&E) -> &std::sync::Arc<RootedFace>,
) -> bool {
    bucket
        .iter()
        .position(|entry| std::sync::Arc::ptr_eq(merged_of(entry), pushed))
        .is_some_and(|position| {
            bucket.swap_remove(position);
            true
        })
}

/// Marker of a face-cache speculation window handed back by
/// [`FaceResolver::arm_face_cache_speculation`]. Opaque by design: only the
/// resolver's accept/rewind pair consumes it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FaceCacheSpeculationMark {
    journal_len: usize,
}

/// Beyond this many distinct composed sources the cache resets wholesale: a
/// session composing thousands of distinct inline-plist faces keeps bounded
/// memory, and steady workloads stay far under the cap.
const COMPOSED_SOURCE_CACHE_LIMIT: usize = 4096;

/// Engine-side carry of one resolver's [`ComposedSourceCache`] across redisplay
/// transactions.
///
/// A resolver is rebuilt per transaction (twice when a TTY color callback
/// forces a mid-transaction refresh), and with it every folded named-face
/// specification and interned composed source used to be refolded from the
/// table on every keystroke. GNU instead keeps each frame's face cache across
/// redisplays and discards it only when a definition changes: `face_change` /
/// `f->face_change` (src/xfaces.c:349, src/xfaces.c:746), set by the
/// definition writers (src/xfaces.c:3098-3105). Entries are keyed by merged
/// content or by allocation identity, so only the folded named specifications
/// can go stale: [`ComposedSourcePin::apply_to`] adopts the cache across a
/// changed-table refresh after dropping those folds. A changed table is a
/// different snapshot by construction — evaluation mutates through
/// `Rc::make_mut`, which reallocates rather than disturb this pin.
pub(crate) struct ComposedSourcePin {
    table: FaceTable,
    cache: std::rc::Rc<std::cell::RefCell<ComposedSourceCache>>,
}

impl ComposedSourcePin {
    /// Hand the cache to `resolver`: wholesale when its table snapshot still
    /// matches the one this pin was folded against, otherwise with the folded
    /// named specifications dropped (see below).
    pub(crate) fn apply_to(&self, resolver: &mut FaceResolver) {
        if resolver.face_table.same_snapshot(&self.table) {
            resolver.composed_sources = std::rc::Rc::clone(&self.cache);
            return;
        }
        // A definitions refresh re-derived at least one face, so this pin's
        // table snapshot is stale — but staleness is scoped. The folded named
        // specifications are functions of the OLD table's definitions and must
        // re-fold. The composition and identity entries are functions of their
        // inputs: their keys name the base allocation and the overlay content
        // or allocation, and an input the new table no longer produces simply
        // never probes again — a definition change cannot corrupt a hit,
        // because equal content realizes equal paint and unequal content
        // misses by key. Keeping them preserves the merged-source allocations,
        // so unchanged faces keep their source identity across the refresh,
        // GNU's lface vectors surviving a `face_change` that re-realizes only
        // what changed (src/xfaces.c:349, src/xfaces.c:3098-3105). Realized
        // terminal paint stays retirement-stamped by `face_change_count`.
        let mut cache = self.cache.borrow_mut();
        cache.len = cache.len.saturating_sub(cache.named_specs.len());
        cache.named_specs.clear();
        resolver.composed_sources = std::rc::Rc::clone(&self.cache);
    }
}

/// Engine-side carry of one resolver's realized terminal faces.
///
/// `prepare_tty_realization` re-derives the default and every basic face from
/// core's per-frame display cache, roots each source, and installs its
/// realized colors. A redisplay where no face definition changed needs none of
/// that work: GNU's realized faces survive on the frame until `face_change`
/// invalidates them (src/xfaces.c:3098-3105), and the binding generation —
/// the `face_change_count` at realization time — is that stamp here.
pub(crate) struct TtyRealizationPin {
    frame_id: FrameId,
    generation: u64,
    table: FaceTable,
    default_face: ResolvedFace,
    basic_faces: Box<enum_map::EnumMap<BasicFace, Option<ResolvedFace>>>,
}

/// GNU basic-face lookup has two realization outcomes.  A canonical lookup
/// keeps the fixed `enum face_id` slot.  A window-named lookup starts from the
/// frame's unremapped default, then may incorporate direct or inherited entries
/// from `face-remapping-alist`, so it needs a content-addressed dynamic slot.
/// Keeping the base in the variant name records GNU's non-obvious ordering:
/// start realization from the frame default, then apply remapping while walking
/// the named face and its `:inherit` graph.  Starting from an already-remapped
/// default would apply a `default` text scale twice.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum BufferBasicFaceLookup {
    Canonical(BasicFaceId),
    WindowNamedOverFrameDefault(BasicFaceId),
}

impl FaceResolver {
    fn resolve_face_height(&self, height: &FaceHeight, inherited_size: f32) -> f32 {
        let candidate = match height {
            FaceHeight::Absolute(tenths) => {
                FaceSizeCandidate::Absolute(self.font_sizing.face_height_to_layout_pixels(*tenths))
            }
            FaceHeight::Relative(factor) => {
                FaceSizeCandidate::Relative(inherited_size * *factor as f32)
            }
        };
        self.frame_font_domain
            .resolve_face_size(candidate, inherited_size)
    }

    /// Commit the default font that the graphic backend actually opened.
    /// Every subsequently realized face then inherits from the same size that
    /// supplied frame geometry and will be sent to rasterization.
    pub(crate) fn retain_opened_default_font_size(&mut self, opened_size: GraphicFontSizePx) {
        self.default_face.font_size = opened_size.get();
        self.frame_font_domain
            .retain_opened_graphic_size(opened_size);
    }

    pub(crate) fn is_window_system(&self) -> bool {
        self.window_system.is_some()
    }

    fn face_spec_is_plist(items: &[Value]) -> bool {
        match items.first() {
            Some(v) if v.is_bare_keyword() => true,
            Some(item) => item
                .as_bare_symbol_name()
                .is_some_and(|name| name.starts_with(':')),
            None => false,
        }
    }

    /// Create a new `FaceResolver`.
    ///
    /// Clones the `FaceTable` so the resolver owns its data and does not
    /// borrow from the `Context`.  This allows `layout_window_rust` to
    /// take `&mut Context` for `format-mode-line` evaluation while
    /// still using the `FaceResolver`.
    pub fn new(
        face_table: &FaceTable,
        default_fg: u32,
        default_bg: u32,
        default_font_size: f32,
        window_system: Option<String>,
    ) -> Self {
        Self::new_with_font_sizing(
            face_table,
            default_fg,
            default_bg,
            default_font_size,
            window_system,
            FontSizing::native_gui(),
        )
    }

    pub fn new_with_font_sizing(
        face_table: &FaceTable,
        default_fg: u32,
        default_bg: u32,
        default_font_size: f32,
        window_system: Option<String>,
        font_sizing: FontSizing,
    ) -> Self {
        let frame_font_domain =
            FrameFontDomain::for_frame(window_system.is_some(), default_font_size);
        let neo_default = face_table.resolve("default");
        let mut df = ResolvedFace::default();
        if let Some(color) = neo_default.foreground.as_ref() {
            df.set_foreground(color);
        } else {
            df.set_frame_default_foreground(default_fg);
        }
        if let Some(color) = neo_default.background.as_ref() {
            df.set_background(color);
        } else {
            df.set_frame_default_background(default_bg);
        }
        df.font_family = neo_default
            .family_runtime_string_owned()
            .unwrap_or_default();
        df.font_weight = neo_default
            .weight
            .map(FontWeight::css_weight)
            .unwrap_or(FontWeight::NORMAL.css_weight());
        df.italic = neo_default.slant.map(|s| s.is_italic()).unwrap_or(false);
        let requested_default_size = match &neo_default.height {
            Some(FaceHeight::Absolute(tenths)) => font_sizing.face_height_to_layout_pixels(*tenths),
            _ => default_font_size,
        };
        df.font_size = frame_font_domain.resolve_face_size(
            FaceSizeCandidate::Absolute(requested_default_size),
            default_font_size,
        );
        df.extend = neo_default.extend.unwrap_or(false);
        df.overstrike = neo_default.overstrike;

        // Underline
        if let Some(ul) = neo_default.underline.enabled() {
            df.underline_style = underline_style_to_u8(&ul.style);
            df.underline_color = ul.color.as_ref().map(color_to_pixel).unwrap_or(0);
            df.terminal_underline_color = ul.color.as_ref().and_then(|color| color.terminal);
        }
        // Overline
        if neo_default.overline == Some(true) {
            df.overline = true;
        }
        // Strike-through
        if neo_default.strike_through == Some(true) {
            df.strike_through = true;
        }
        FaceColorAttributes::from_base(&df)
            .merge(&neo_default)
            .realize()
            .install(&mut df);
        apply_box_border(&mut df, &neo_default.box_border);

        Self {
            face_table: face_table.clone(),
            face_table_roots: None,
            default_face: df,
            tty_face_source: None,
            realized_basic_faces: None,
            window_system,
            current_window_parameters: std::cell::RefCell::new(Vec::new()),
            current_window_id: std::cell::Cell::new(None),
            invalid_face_references: std::cell::RefCell::new(Vec::new()),
            composed_sources: std::rc::Rc::new(std::cell::RefCell::default()),
            font_sizing,
            frame_font_domain,
        }
    }

    /// Bind the pure resolver to the real terminal frame. Basic-face colors
    /// were realized by core's GNU lifecycle; importing them is read-only and
    /// must not demand synthetic or missing basic names (notably tab-line).
    ///
    /// `previous` carries the last bound resolver's realized faces: when the
    /// frame, the `face_change_count` stamp, and the table snapshot all match,
    /// the realization is reused instead of re-derived — GNU keeps a frame's
    /// realized faces until a definition change invalidates them
    /// (src/xfaces.c:3098-3105).
    pub(crate) fn prepare_tty_realization(
        &mut self,
        frame_id: FrameId,
        evaluator: &mut Context,
        previous: Option<TtyRealizationPin>,
    ) {
        if !neovm_core::face::realization::enabled()
            || self.window_system.is_some()
            || !evaluator.frame_is_tty_face_target(frame_id)
        {
            return;
        }
        if self.face_table_roots.is_none() {
            match evaluator.display_face_table_root_lease(frame_id, &self.face_table) {
                Ok(roots) => self.face_table_roots = Some(FaceTableRoots::from(roots)),
                Err(error) => {
                    tracing::error!(
                        ?error,
                        "terminal face definitions require an installed mutator heap"
                    );
                    return;
                }
            }
        }
        if let Some(pin) = previous
            && pin.frame_id == frame_id
            && pin.generation == evaluator.face_change_count()
            && self.face_table.same_snapshot(&pin.table)
            && pin.basic_faces[BasicFace::Default].is_some()
        {
            self.default_face = pin.default_face;
            self.tty_face_source = Some(BoundTtyFaceSource {
                frame_id,
                generation: pin.generation,
            });
            self.realized_basic_faces = Some(pin.basic_faces);
            return;
        }
        let Some((default_source, default_paint)) =
            evaluator.cached_basic_display_face_for_frame(frame_id, BasicFace::Default.name())
        else {
            return;
        };
        let rooted_default = match RootedFace::new(default_source.clone()) {
            Ok(source) => std::sync::Arc::new(source),
            Err(error) => {
                tracing::error!(
                    ?error,
                    "terminal face binding requires an installed mutator heap"
                );
                return;
            }
        };
        let mut default = self.realize_face_with_source(
            &default_source,
            FaceRealizationSource::Complete(rooted_default),
        );
        default.install_realized_tty_colors(&default_paint);
        default.canonical_tty_role = Some(BasicFace::Default);
        self.default_face = default;
        self.tty_face_source = Some(BoundTtyFaceSource {
            frame_id,
            generation: evaluator.face_change_count(),
        });
        let mut named_default = self.default_face.clone();
        named_default.lisp_name = Some(BasicFace::Default.name().to_owned());
        let mut basic_faces = Box::new(enum_map::EnumMap::default());
        basic_faces[BasicFace::Default] = Some(named_default);
        for name in neovm_core::face::realization::basic_face_names() {
            let Some(role) = BasicFace::from_name(name) else {
                continue;
            };
            if role == BasicFace::Default {
                continue;
            }
            let Some((source, paint)) =
                evaluator.cached_basic_display_face_for_frame(frame_id, name)
            else {
                continue;
            };
            let rooted_source = match RootedFace::with_base(
                source.clone(),
                self.default_face.source_attributes.as_deref(),
            ) {
                Ok(source) => std::sync::Arc::new(source),
                Err(error) => {
                    tracing::error!(
                        ?error,
                        "basic face source requires an installed mutator heap"
                    );
                    continue;
                }
            };
            let mut resolved = self
                .realize_face_with_source(&source, FaceRealizationSource::Complete(rooted_source));
            resolved.install_realized_tty_colors(&paint);
            resolved.canonical_tty_role = Some(role);
            resolved.lisp_name = Some(name.to_owned());
            resolved.face_id =
                BasicFaceId::from_name(name).map_or(BasicFaceId::SENTINEL, u32::from);
            basic_faces[role] = Some(resolved);
        }
        self.realized_basic_faces = Some(basic_faces);
    }

    pub(crate) fn has_tty_realization(&self) -> bool {
        self.tty_face_source.is_some()
    }

    /// The realized terminal faces and their binding stamp, for the engine to
    /// carry into the next redisplay's resolver (see [`TtyRealizationPin`]).
    pub(crate) fn tty_realization_pin(&self) -> Option<TtyRealizationPin> {
        let bound = self.tty_face_source?;
        let basic_faces = self.realized_basic_faces.as_ref()?;
        Some(TtyRealizationPin {
            frame_id: bound.frame_id,
            generation: bound.generation,
            table: self.face_table.clone(),
            default_face: self.default_face.clone(),
            basic_faces: basic_faces.clone(),
        })
    }

    /// The composed-source cache together with the face table snapshot it was
    /// folded against, so the engine can carry it to the next redisplay's
    /// resolver (see [`ComposedSourcePin`]).
    pub(crate) fn composed_source_pin(&self) -> ComposedSourcePin {
        ComposedSourcePin {
            table: self.face_table.clone(),
            cache: std::rc::Rc::clone(&self.composed_sources),
        }
    }

    /// A speculative row probe may consume an existing realization, but must
    /// never enter Lisp for a face the iterator has not reached yet.
    pub(crate) fn realize_cached_for_display(
        &self,
        face: &mut ResolvedFace,
        evaluator: &Context,
    ) -> bool {
        let Some(source) = self.tty_face_source else {
            return true;
        };
        if !source.is_current(evaluator, &self.face_table) {
            return false;
        }
        if face.canonical_tty_role.is_some() {
            return true;
        }
        let frame_id = source.frame_id;
        let Some(attributes) = face.source_attributes.clone() else {
            return true;
        };
        if let Some(paint) =
            self.cached_tty_paint(face, &attributes, frame_id, evaluator.face_change_count())
        {
            paint.install(face);
            return true;
        }
        let Some(realized) =
            evaluator.cached_display_face_for_frame(frame_id, attributes.attributes())
        else {
            return false;
        };
        // The memo must answer probes of the UNREALIZED entry state, so the
        // key is read before the install writes realized colors into `face`.
        let probe_key = TtyPaintKey::of(face);
        face.install_realized_tty_colors(&realized);
        self.record_tty_paint(
            probe_key,
            face,
            attributes,
            frame_id,
            evaluator.face_change_count(),
        );
        true
    }

    pub(crate) fn realize_for_display(&self, face: &mut ResolvedFace, evaluator: &mut Context) {
        self.realize_with_cold_observer(face, evaluator, &NoColdFaceEntryObserver);
    }

    #[inline]
    fn realize_with_cold_observer<O: ColdFaceEntryObserver>(
        &self,
        face: &mut ResolvedFace,
        evaluator: &mut Context,
        observer: &O,
    ) {
        if let Some(source) = self.tty_face_source
            && source.is_current(evaluator, &self.face_table)
        {
            if face.canonical_tty_role.is_some() {
                return;
            }
            // A prior color callback may have replaced this frame's source
            // definitions, including through nested redisplay. Leave temporary
            // paint alone until the transaction rejects/restarts; entering Lisp
            // here would realize a later glyph from obsolete source attributes.
            let Some(attributes) = face.source_attributes.clone() else {
                return;
            };
            if let Some(paint) = self.cached_tty_paint(
                face,
                &attributes,
                source.frame_id,
                evaluator.face_change_count(),
            ) {
                paint.install(face);
                return;
            }
            // Observe the actual demand entry, including pending/error/no-
            // insertion returns. Cache journals record only later successes.
            observer.before_entry();
            let realized =
                evaluator.realize_display_face_for_frame(source.frame_id, attributes.attributes());
            // See realize_cached_for_display: the memo key is the
            // pre-realization entry state the next probe arrives in.
            let probe_key = TtyPaintKey::of(face);
            face.install_realized_tty_colors(&realized);
            // A deferred or in-flight hook flow realized against a placeholder
            // (`realize_display_face_for_frame` returns the source unchanged);
            // such paint is transactional and must not seed the memo.
            if !evaluator.redisplay_hook_flow_pending() {
                self.record_tty_paint(
                    probe_key,
                    face,
                    attributes,
                    source.frame_id,
                    evaluator.face_change_count(),
                );
            }
        }
    }

    /// A bound TTY lifecycle retains plist strings until the complete lface
    /// reaches the evaluator; no palette approximation can replace a callback.
    fn plist_palette(&self) -> Option<&neomacs_display_protocol::TtyPalette> {
        if self.has_tty_realization() {
            None
        } else {
            self.face_table.tty_palette()
        }
    }

    /// Discard diagnostics from a speculative frame-layout attempt.
    pub(crate) fn clear_diagnostics(&self) {
        self.invalid_face_references.borrow_mut().clear();
    }

    /// Drain invalid named face references from the accepted layout attempt.
    pub(crate) fn take_invalid_face_references(&self) -> Vec<String> {
        self.invalid_face_references.take()
    }

    /// Begin journaling this resolver's face-cache inserts for a speculative
    /// routed-row attempt. While the window is open every insert appends an
    /// undo record; [`Self::rewind_face_cache_speculation`] restores the
    /// caches to this point so a refused row leaves no face-cache side
    /// effects visible, and [`Self::accept_face_cache_speculation`] keeps
    /// what a routed row earned. The journal rides the shared composed-source
    /// cache, so it survives a mid-attempt resolver refresh unchanged. A Lisp
    /// color callback that runs a nested layout inside the window journals
    /// those inserts too; undoing them is safe because every cache here is a
    /// pure function of its inputs — the nested layout's published state
    /// never reads the memo back through this journal.
    pub(crate) fn arm_face_cache_speculation(&self) -> FaceCacheSpeculationMark {
        let mut cache = self.composed_sources.borrow_mut();
        cache.speculation_depth += 1;
        FaceCacheSpeculationMark {
            journal_len: cache.speculation_journal.len(),
        }
    }

    /// Close a speculation window whose row routed: the inserts stay, their
    /// undo records go.
    pub(crate) fn accept_face_cache_speculation(&self, mark: FaceCacheSpeculationMark) {
        let mut cache = self.composed_sources.borrow_mut();
        cache.speculation_depth = cache.speculation_depth.saturating_sub(1);
        cache.speculation_journal.truncate(mark.journal_len);
    }

    /// Close a speculation window whose row was refused: undo its inserts in
    /// reverse order. Records apply newest-first, so a pushed bucket entry is
    /// still the last of its bucket when its undo runs.
    pub(crate) fn rewind_face_cache_speculation(&self, mark: FaceCacheSpeculationMark) {
        let mut cache = self.composed_sources.borrow_mut();
        cache.speculation_depth = cache.speculation_depth.saturating_sub(1);
        while cache.speculation_journal.len() > mark.journal_len {
            let Some(undo) = cache.speculation_journal.pop() else {
                break;
            };
            undo.apply(&mut cache);
        }
    }

    /// Point the resolver at the window whose faces are about to be resolved,
    /// so a `(:window PARAM VALUE)` `:filtered` predicate can consult that
    /// window's parameters. Call at each window boundary (and with an empty
    /// alist for frame chrome / non-window contexts). GNU threads the window
    /// `w` into `evaluate_face_filter`; the resolver being frame-shared and
    /// used behind `&`, this is the equivalent interior-mutable hook.
    pub fn set_current_window_parameters(&self, params: Vec<(Value, Value)>) {
        self.current_window_parameters.replace(params);
    }

    /// Point the resolver at the window whose faces are about to be resolved so
    /// an overlay's `window` property can be honored (a windowed overlay applies
    /// only in its window). `None` for frame chrome / non-window contexts, which
    /// then applies every overlay (matching GNU's "no window ⇒ unrestricted").
    /// Call at each window boundary alongside `set_current_window_parameters`.
    pub fn set_current_window_id(&self, window_id: Option<u64>) {
        self.current_window_id.set(window_id);
    }

    /// The window whose faces are currently being resolved (see
    /// [`set_current_window_id`](Self::set_current_window_id)); `None` for
    /// frame chrome / TTY / non-window contexts.
    pub(crate) fn current_window_id(&self) -> Option<u64> {
        self.current_window_id.get()
    }

    /// Return a reference to the resolved default face.
    pub fn default_face(&self) -> &ResolvedFace {
        &self.default_face
    }

    /// Resolve a named face from the face table, assigning a stable
    /// face-cache ID.
    ///
    /// Basic faces (see [`BasicFaceId`]) get their fixed enum value.
    /// Other faces get a dynamically allocated ID ≥
    /// [`BasicFaceId::SENTINEL`] (20).
    pub fn resolve_named_face(&self, name: &str) -> ResolvedFace {
        use neomacs_display_protocol::face::BasicFaceId;
        if let Some(role) = BasicFace::from_name(name)
            && let Some(faces) = &self.realized_basic_faces
            && let Some(face) = &faces[role]
        {
            return face.clone();
        }
        let face = self.face_table.resolve(name);
        let mut resolved = self.realize_face(&face);
        resolved.lisp_name = Some(name.to_string());
        if let Some(basic) = BasicFaceId::from_name(name) {
            resolved.face_id = basic.into();
        } else {
            // Non-basic faces carry no stable id from here. Every consumer that
            // needs a matrix face id re-keys via the frame-scoped allocator
            // (GNU's single `face_cache->used`); the sentinel makes that
            // "must be re-keyed" contract explicit.
            resolved.face_id = BasicFaceId::SENTINEL;
        }
        resolved
    }

    /// Resolve a named face while ignoring its final `:inverse-video`
    /// attribute.
    ///
    /// GNU's toolkit-backed menu bars use the `menu` face resources for
    /// foreground/background/font, but the default `menu` defface has an
    /// empty `x-toolkit` branch instead of the TTY/fallback inverse-video
    /// branch.  Neomacs' GUI menu bar is custom-rendered, so use this helper
    /// at that toolkit boundary: preserve the face's concrete attributes, but
    /// do not swap foreground/background for `:inverse-video`.
    pub fn resolve_named_face_without_inverse_video(&self, name: &str) -> ResolvedFace {
        use neomacs_display_protocol::face::BasicFaceId;
        let mut face = self.face_table.resolve(name);
        face.inverse_video = Some(false);
        let mut resolved = self.realize_face(&face);
        resolved.lisp_name = Some(name.to_string());
        if let Some(basic) = BasicFaceId::from_name(name) {
            resolved.face_id = basic.into();
        } else {
            // See `resolve_named_face`: non-basic faces are re-keyed by consumers
            // via the frame-scoped allocator (GNU `face_cache->used`).
            resolved.face_id = BasicFaceId::SENTINEL;
        }
        resolved.terminal_inverse_video = false;
        resolved
    }

    /// One `Arc<RootedFace>` per distinct composed source.
    ///
    /// Every glyph-run composition of the same logical face reuses the shared
    /// source, so memo probes (`face_id_for_resolved`, `remember_face`) and
    /// frame-realization lookups compare by pointer identity instead of
    /// walking merged attributes again — GNU's merged lface vectors get the
    /// same effect by sharing slot objects, making `lface_equal_p`'s EQ
    /// fast path the common case (src/xfaces.c:4483-4497).
    fn shared_composed_source(
        &self,
        merged: neovm_core::face::Face,
        base: Option<&RootedFace>,
    ) -> Option<std::sync::Arc<RootedFace>> {
        let hash = merged.content_hash();
        {
            let cache = self.composed_sources.borrow();
            if let Some(bucket) = cache.entries.get(&hash)
                && let Some(shared) = bucket.iter().find(|shared| *shared.attributes() == merged)
            {
                return Some(std::sync::Arc::clone(shared));
            }
        }
        // Rooting allocates leases; run it without the cache borrowed.
        let rooted = match RootedFace::with_base(merged, base) {
            Ok(rooted) => std::sync::Arc::new(rooted),
            Err(error) => {
                tracing::error!(
                    ?error,
                    "bound face composition lost its installed mutator heap"
                );
                return None;
            }
        };
        let mut cache = self.composed_sources.borrow_mut();
        if cache.len >= COMPOSED_SOURCE_CACHE_LIMIT {
            if cache.speculation_depth > 0 {
                // A speculative insert must not trip the wholesale reset
                // (undoing one would mean snapshotting the whole cache);
                // leave this source uncached and let the next plain insert
                // take the reset.
                return Some(rooted);
            }
            cache.clear();
        }
        let speculative = cache.speculation_depth > 0;
        cache.entries.entry(hash).or_default().push(rooted.clone());
        cache.len += 1;
        if speculative {
            cache
                .speculation_journal
                .push(FaceCacheRewind::ComposedEntry {
                    hash,
                    pushed: rooted.clone(),
                });
        }
        Some(rooted)
    }

    /// The folded overlay specification for `name`, when it was already folded
    /// in this resolver. Folding resolves the table entry plus its `:inherit`
    /// graph, a pure function of the immutable snapshot, so one fold per name
    /// per resolver is enough; GNU keeps the same economy by reusing the
    /// frame's cached lface for a named face (`get_lface_attributes`,
    /// src/xfaces.c:2124-2163).
    fn cached_named_overlay_spec(&self, name: &str) -> Option<std::sync::Arc<NeoFace>> {
        self.composed_sources
            .borrow()
            .named_specs
            .get(name)
            .filter(|entry| entry.alist_bits == ALIST_INDEPENDENT)
            .map(|entry| std::sync::Arc::clone(&entry.shared))
    }

    fn record_named_overlay_spec(&self, name: &str, folded: NeoFace) -> std::sync::Arc<NeoFace> {
        self.record_named_overlay_spec_for_alist(name, folded, ALIST_INDEPENDENT)
    }

    /// The fold for `name` under this buffer's remapping alist object, when
    /// already folded against that same object (see [`NamedSpecEntry`]). A
    /// buffer with no alist accepts pure table folds too: without an alist
    /// there is nothing for the fold to consult, so the two coincide.
    fn cached_buffer_named_overlay_spec(
        &self,
        name: &str,
        alist_bits: usize,
    ) -> Option<std::sync::Arc<NeoFace>> {
        self.composed_sources
            .borrow()
            .named_specs
            .get(name)
            .filter(|entry| {
                entry.alist_bits == alist_bits
                    || (alist_bits == 0 && entry.alist_bits == ALIST_INDEPENDENT)
            })
            .map(|entry| std::sync::Arc::clone(&entry.shared))
    }

    fn record_named_overlay_spec_for_alist(
        &self,
        name: &str,
        folded: NeoFace,
        alist_bits: usize,
    ) -> std::sync::Arc<NeoFace> {
        let shared = std::sync::Arc::new(folded);
        let mut cache = self.composed_sources.borrow_mut();
        if cache.len >= COMPOSED_SOURCE_CACHE_LIMIT {
            if cache.speculation_depth > 0 {
                // See `shared_composed_source`: a speculative fold stays out
                // of the cache rather than trip the wholesale reset.
                return shared;
            }
            cache.clear();
        }
        let previous = (cache.speculation_depth > 0)
            .then(|| cache.named_specs.get(name).cloned())
            .flatten();
        cache.named_specs.insert(
            name.into(),
            NamedSpecEntry {
                shared: std::sync::Arc::clone(&shared),
                alist_bits,
            },
        );
        cache.len += 1;
        if cache.speculation_depth > 0 {
            cache.speculation_journal.push(FaceCacheRewind::NamedSpec {
                name: name.into(),
                previous,
            });
        }
        shared
    }

    /// The previously realized terminal paint for `face`'s composed source,
    /// when this exact realization ran against the same frame definitions.
    ///
    /// Paint is a pure function of the entry color state, the underline slot
    /// the install preserves, and the rooted source — whose realization is
    /// stamped by the binding `face_change_count`. A hit is GNU serving a
    /// repeated merged-lface lookup from the frame face cache
    /// (`lookup_face`, src/xfaces.c:5064-5093) without re-running
    /// `realize_tty_face`.
    fn cached_tty_paint(
        &self,
        face: &ResolvedFace,
        source: &std::sync::Arc<RootedFace>,
        frame: FrameId,
        face_change_count: u64,
    ) -> Option<TtyPaint> {
        source.cached_tty_paint(frame, face_change_count, face)
    }

    /// Retain one realized terminal paint for reuse beside its source; the
    /// allocation holds the memo, so its address cannot be recycled under an
    /// entry and a retired source releases its paint with everything else.
    /// `probe_key` is the face's PRE-realization entry state — see
    /// [`RootedFace::record_tty_paint`].
    fn record_tty_paint(
        &self,
        probe_key: TtyPaintKey,
        face: &ResolvedFace,
        source: std::sync::Arc<RootedFace>,
        frame: FrameId,
        face_change_count: u64,
    ) {
        // The slot is one deep, so a speculative record would evict whatever
        // paint the source carried into this attempt; journal the previous
        // contents for the rewind (see `FaceCacheRewind::TtyPaint`).
        let previous =
            (self.composed_sources.borrow().speculation_depth > 0).then(|| source.tty_paint_slot());
        source.record_tty_paint(frame, face_change_count, probe_key, face);
        if let Some(previous) = previous {
            self.composed_sources
                .borrow_mut()
                .speculation_journal
                .push(FaceCacheRewind::TtyPaint { source, previous });
        }
    }

    /// The previously merged source for `overlay` over `base`, when this exact
    /// composition was already built. Composition is a pure function of the
    /// base's source identity and the overlay's content, so a hit is exact.
    fn cached_composed_over(
        &self,
        base: &std::sync::Arc<RootedFace>,
        overlay: &NeoFace,
    ) -> Option<std::sync::Arc<RootedFace>> {
        let key = (
            std::sync::Arc::as_ptr(base) as usize,
            overlay.content_hash(),
        );
        let cache = self.composed_sources.borrow();
        let bucket = cache.compositions.get(&key)?;
        bucket
            .iter()
            .find(|entry| {
                entry
                    .base
                    .upgrade()
                    .is_some_and(|pinned| std::sync::Arc::ptr_eq(&pinned, base))
                    && &entry.overlay == overlay
            })
            .map(|entry| std::sync::Arc::clone(&entry.merged))
    }

    /// Identity-keyed composition reuse: both the base's rooted source and the
    /// overlay's folded specification are shared allocations, so the probe is
    /// pointer equality on both, GNU's `EQ` fast path for shared lface slots
    /// (src/xfaces.c:4483-4497). Weak pins on both sides keep address reuse
    /// from aliasing a different face.
    fn cached_composed_identity_over(
        &self,
        base: &std::sync::Arc<RootedFace>,
        overlay: &std::sync::Arc<NeoFace>,
    ) -> Option<std::sync::Arc<RootedFace>> {
        let key = (
            std::sync::Arc::as_ptr(base) as usize,
            std::sync::Arc::as_ptr(overlay) as usize,
        );
        let cache = self.composed_sources.borrow();
        let bucket = cache.identity_compositions.get(&key)?;
        bucket
            .iter()
            .find(|entry| {
                entry
                    .base
                    .upgrade()
                    .is_some_and(|pinned| std::sync::Arc::ptr_eq(&pinned, base))
                    && entry
                        .overlay
                        .upgrade()
                        .is_some_and(|pinned| std::sync::Arc::ptr_eq(&pinned, overlay))
            })
            .map(|entry| std::sync::Arc::clone(&entry.merged))
    }

    fn record_composed_identity_over(
        &self,
        base: std::sync::Arc<RootedFace>,
        overlay: std::sync::Arc<NeoFace>,
        merged: std::sync::Arc<RootedFace>,
    ) {
        let key = (
            std::sync::Arc::as_ptr(&base) as usize,
            std::sync::Arc::as_ptr(&overlay) as usize,
        );
        let mut cache = self.composed_sources.borrow_mut();
        if cache.len >= COMPOSED_SOURCE_CACHE_LIMIT {
            if cache.speculation_depth > 0 {
                // See `shared_composed_source`.
                return;
            }
            cache.clear();
        }
        let speculative = cache.speculation_depth > 0;
        cache
            .identity_compositions
            .entry(key)
            .or_default()
            .push(IdentityComposedEntry {
                base: std::sync::Arc::downgrade(&base),
                overlay: std::sync::Arc::downgrade(&overlay),
                merged: std::sync::Arc::clone(&merged),
            });
        cache.len += 1;
        if speculative {
            cache
                .speculation_journal
                .push(FaceCacheRewind::IdentityComposition {
                    key,
                    pushed: merged,
                });
        }
    }

    fn record_composed_over(
        &self,
        base: std::sync::Arc<RootedFace>,
        overlay: NeoFace,
        merged: std::sync::Arc<RootedFace>,
    ) {
        let key = (
            std::sync::Arc::as_ptr(&base) as usize,
            overlay.content_hash(),
        );
        let mut cache = self.composed_sources.borrow_mut();
        if cache.len >= COMPOSED_SOURCE_CACHE_LIMIT {
            if cache.speculation_depth > 0 {
                // See `shared_composed_source`.
                return;
            }
            cache.clear();
        }
        let speculative = cache.speculation_depth > 0;
        cache
            .compositions
            .entry(key)
            .or_default()
            .push(ComposedOverlayEntry {
                base: std::sync::Arc::downgrade(&base),
                overlay,
                merged: std::sync::Arc::clone(&merged),
            });
        cache.len += 1;
        if speculative {
            cache
                .speculation_journal
                .push(FaceCacheRewind::Composition {
                    key,
                    pushed: merged,
                });
        }
    }

    fn apply_specified_face_over(
        &self,
        base: &ResolvedFace,
        face: &NeoFace,
        face_identity: Option<&std::sync::Arc<NeoFace>>,
    ) -> ResolvedFace {
        let mut rf = base.clone();
        rf.retain_source_over(self, base, face, face_identity);
        FaceColorAttributes::from_base(base)
            .merge(face)
            .realize()
            .install(&mut rf);

        if let Some(family) = face.family_runtime_string_owned() {
            rf.font_family = family;
        }
        if let Some(weight) = face.weight {
            rf.font_weight = weight.css_weight();
        }
        if let Some(slant) = face.slant {
            rf.italic = slant.is_italic();
        }
        if let Some(height) = &face.height {
            rf.font_size = self.resolve_face_height(height, rf.font_size);
        }

        match &face.underline {
            FaceDecoration::Unspecified => {}
            FaceDecoration::Disabled => {
                rf.underline_style = 0;
                rf.underline_position = NeoUnderlinePosition::FontMetric;
                rf.underline_color = 0;
                rf.terminal_underline_color = None;
            }
            FaceDecoration::Enabled(underline) => {
                rf.underline_style = underline_style_to_u8(&underline.style);
                rf.underline_position = underline.position;
                // GNU draws `:underline t` (no explicit color) in the face's
                // foreground -- e.g. `nobreak-space` inherits `escape-glyph`'s
                // brown fg and underlines in that same brown. Default the
                // unspecified color to `rf.fg` here (fg is applied earlier in
                // this merge), mirroring the box-border resolution below so a
                // downstream pixel-0 underline color means "explicit black"
                // uniformly with box.
                rf.underline_color = underline
                    .color
                    .as_ref()
                    .map(color_to_pixel)
                    .unwrap_or(rf.fg);
                // GNU does NOT default the terminal slot to the foreground:
                // `realize_tty_face` leaves `face->underline_color` at 0 for an
                // underline with no `:color` of its own (src/xfaces.c:6741,
                // :6756) and for `(:color foreground-color)` (:6772-6773), and
                // `turn_on_face` emits nothing for 0 (src/term.c:2120).
                rf.terminal_underline_color =
                    underline.color.as_ref().and_then(|color| color.terminal);
            }
        }
        if let Some(overline) = face.overline {
            rf.overline = overline;
            // GNU draws `:overline t` (no explicit color) in the face
            // foreground, like `:underline t` and `:box t` above. An explicit
            // `:overline COLOR` (handled just below) overrides.
            if overline {
                rf.overline_color = rf.fg;
            }
        }
        if let Some(color) = &face.overline_color {
            rf.overline_color = color_to_pixel(color);
        }
        if let Some(strike) = face.strike_through {
            rf.strike_through = strike;
            // Same rule for `:strike-through t` -> face foreground.
            if strike {
                rf.strike_through_color = rf.fg;
            }
        }
        if let Some(color) = &face.strike_through_color {
            rf.strike_through_color = color_to_pixel(color);
        }
        apply_box_border(&mut rf, &face.box_border);
        if let Some(extend) = face.extend {
            rf.extend = extend;
        }
        if face.overstrike {
            rf.overstrike = true;
        }

        // Stipple: a face that specifies `:stipple` overrides the inherited
        // one; leaving it unspecified keeps the base face's stipple (GNU merge
        // semantics). This is the path buffer-text (text-property / overlay /
        // font-lock) faces take, e.g. `indent-bars`.
        if let Some(pat) = self.realize_stipple(face) {
            rf.stipple = Some(pat);
        }

        rf
    }

    fn resolve_named_face_overlay_spec(
        &self,
        name: &str,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> std::sync::Arc<NeoFace> {
        if depth > 40 {
            return std::sync::Arc::new(NeoFace::default());
        }
        if name == "default" && depth > 0 {
            return std::sync::Arc::new(NeoFace::default());
        }
        // `default` folds differently per depth, so only ordinary names are
        // shared; invalid names stay unmemoized to keep per-lookup
        // diagnostics.
        if name != "default"
            && let Some(shared) = self.cached_named_overlay_spec(name)
        {
            return shared;
        }

        let Some(face) = self.face_table.get(name).cloned() else {
            if diagnostics == FaceReferenceDiagnostics::Report {
                self.invalid_face_references
                    .borrow_mut()
                    .push(name.to_owned());
            }
            return std::sync::Arc::new(NeoFace::default());
        };
        let folded =
            self.resolve_face_overlay_spec(face, depth, FaceReferenceDiagnostics::Suppress);
        if name != "default" {
            self.record_named_overlay_spec(name, folded)
        } else {
            std::sync::Arc::new(folded)
        }
    }

    fn resolve_face_overlay_spec(
        &self,
        mut face: NeoFace,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> NeoFace {
        if depth > 40 {
            return NeoFace::default();
        }

        let parent = match face.inherit.take() {
            Some(inherit_ref) => {
                self.resolve_face_ref_overlay_spec(inherit_ref, depth + 1, diagnostics)
            }
            None => NeoFace::default(),
        };
        parent.merge(&face)
    }

    fn resolve_face_ref_overlay_spec(
        &self,
        face_ref: Value,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> NeoFace {
        self.resolve_face_value_overlay_spec(face_ref, depth, diagnostics)
            .map(|contribution| contribution.face)
            .unwrap_or_default()
    }

    /// Resolve one GNU face reference to logical attributes, without realizing
    /// it against a base face yet.
    fn resolve_face_value_overlay_spec(
        &self,
        face_ref: Value,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> Option<FaceSpecContribution> {
        if depth > 40 || face_ref.is_nil() || face_ref.is_bare_symbol_named("nil") {
            return None;
        }

        if let Some(name) = Self::face_name_from_value(&face_ref) {
            let shared = self.resolve_named_face_overlay_spec(name, depth, diagnostics);
            return Some(FaceSpecContribution {
                face: (*shared).clone(),
                identity: Some(shared),
            });
        }

        let items = list_to_vec(&face_ref)?;
        if items.is_empty() {
            return None;
        }

        match self.eval_filtered_face_spec(&items) {
            FilteredFaceSpec::Matched(filtered_spec) => {
                return self.resolve_face_value_overlay_spec(
                    Value::list(filtered_spec),
                    depth + 1,
                    diagnostics,
                );
            }
            // Filter didn't match → the wrapped spec contributes nothing.
            FilteredFaceSpec::Rejected => return None,
            FilteredFaceSpec::NotFiltered => {}
        }
        if Self::face_spec_is_plist(&items) {
            let face = NeoFace::from_plist_realized("--inline--", &items, self.plist_palette());
            let face = self.resolve_face_overlay_spec(face, depth + 1, diagnostics);
            return Some(FaceSpecContribution::anonymous(face));
        }

        let mut composition = UnresolvedFaceComposition::default();
        for item in items.iter().rev() {
            composition.merge_optional(self.resolve_face_value_overlay_spec(
                *item,
                depth + 1,
                diagnostics,
            ));
        }
        composition.attributes.map(|face| FaceSpecContribution {
            face,
            identity: composition.identity,
        })
    }

    fn face_name_from_value(value: &Value) -> Option<&str> {
        match value.kind() {
            ValueKind::Symbol(_) => value.as_bare_symbol_name(),
            ValueKind::String => value.as_utf8_str(),
            _ => None,
        }
    }

    /// Classify `items` as a GNU `(:filtered FILTER SPEC…)` form and evaluate
    /// FILTER against the current context. Distinguishing the three outcomes
    /// matters: a REJECTED filter must drop its wrapped SPEC entirely — the
    /// earlier `Option` return conflated "rejected" with "not a filter", so the
    /// caller fell through and mis-applied `(:filtered …)` as an inline plist,
    /// ignoring the filter.
    ///
    /// Supported filter predicates:
    ///   `:window PARAMETER VALUE` — GNU's only real face filter
    ///                          (`evaluate_face_filter`, src/xfaces.c): matches
    ///                          when the current window's window-parameter
    ///                          PARAMETER is `eq` to VALUE. This is what
    ///                          indent-bars' per-window stipple-rotation remap
    ///                          uses (`(:window indent-bars-whr WHR)`).
    ///   `:window-system SYM`  — matches when `self.window_system == SYM`
    ///                          (nil for TTY, "x" for X11, etc.). Non-GNU
    ///                          neomacs extension, retained.
    fn eval_filtered_face_spec(&self, items: &[Value]) -> FilteredFaceSpec {
        let Some(name) = items.first().and_then(|first| first.as_bare_symbol_name()) else {
            return FilteredFaceSpec::NotFiltered;
        };
        if name != "filtered" && name != ":filtered" {
            return FilteredFaceSpec::NotFiltered; // not a :filtered form
        }
        // From here it IS a `:filtered` form: a malformed or unmatched filter
        // DROPS the spec (GNU returns false); it is never re-read as a plist.
        if items.len() < 3 {
            return FilteredFaceSpec::Rejected; // malformed: need (:filtered FILTER SPEC)
        }

        let filter = &items[1];
        let spec = &items[2..];

        let ValueKind::Cons = filter.kind() else {
            return FilteredFaceSpec::Rejected; // non-list filter — malformed
        };
        // Evaluate filter predicates. All predicates must pass; mirrors GNU's
        // `face_spec_match_p` (`src/xfaces.c`).
        let filter_items = list_to_vec(filter).unwrap_or_default();
        let mut i = 0;
        while i < filter_items.len() {
            let Some(pred_name) = filter_items
                .get(i)
                .and_then(|pred| pred.as_bare_symbol_name())
            else {
                return FilteredFaceSpec::Rejected;
            };
            match pred_name {
                ":window" | "window" => {
                    // GNU `evaluate_face_filter` (src/xfaces.c): the filter is
                    // `(:window PARAMETER VALUE)` and matches only when the
                    // current window has window-parameter PARAMETER `eq` VALUE.
                    // No current window (frame chrome / TTY) ⇒ no match.
                    let (Some(parameter), Some(value)) =
                        (filter_items.get(i + 1), filter_items.get(i + 2))
                    else {
                        return FilteredFaceSpec::Rejected; // malformed (:window …)
                    };
                    let params = self.current_window_parameters.borrow();
                    let matches = params.iter().any(|(param, val)| {
                        param.bits() == parameter.bits() && val.bits() == value.bits()
                    });
                    if !matches {
                        return FilteredFaceSpec::Rejected;
                    }
                    i += 2; // consumed PARAMETER + VALUE (pred skipped below)
                }
                ":window-system" | "window-system" => {
                    i += 1;
                    let ws_name = filter_items
                        .get(i)
                        .and_then(|val| val.as_bare_symbol_name())
                        .unwrap_or("");
                    let current = self.window_system.as_deref().unwrap_or("nil");
                    if current != ws_name && ws_name != "nil" {
                        return FilteredFaceSpec::Rejected;
                    }
                    if ws_name == "nil" && self.window_system.is_some() {
                        return FilteredFaceSpec::Rejected; // TTY filter, but we're on GUI
                    }
                }
                _ => {
                    // Unknown predicate — fail (matches GNU).
                    return FilteredFaceSpec::Rejected;
                }
            }
            i += 1;
        }
        FilteredFaceSpec::Matched(spec.to_vec())
    }

    fn buffer_face_remapping_specs(
        remapping: BufferFaceRemapping,
        face_name: &str,
    ) -> Option<Value> {
        let mut cursor = remapping.alist()?;
        loop {
            if !cursor.is_cons() {
                return None;
            }
            let entry_car = cursor.cons_car();
            let entry_cdr = cursor.cons_cdr();
            if entry_car.is_cons() {
                let mapping_car = entry_car.cons_car();
                let mapping_cdr = entry_car.cons_cdr();
                if Self::face_name_from_value(&mapping_car).is_some_and(|name| name == face_name) {
                    return Some(mapping_cdr);
                }
            }
            cursor = entry_cdr;
        }
    }

    fn buffer_basic_face_lookup(
        &self,
        remapping: BufferFaceRemapping,
        face_id: BasicFaceId,
    ) -> BufferBasicFaceLookup {
        let Some(remapping_alist) = remapping.alist() else {
            return BufferBasicFaceLookup::Canonical(face_id);
        };
        if remapping_alist.is_nil() {
            return BufferBasicFaceLookup::Canonical(face_id);
        }

        let face_name = face_id.name();
        let has_direct_mapping = Self::buffer_face_remapping_specs(remapping, face_name).is_some();
        let inherits = self
            .face_table
            .get(face_name)
            .and_then(|face| face.inherit)
            .is_some_and(|inherit| {
                !inherit.is_nil() && !inherit.is_bare_symbol_named("unspecified")
            });

        // This is GNU `lookup_basic_face`'s fast path: an unrelated, non-nil
        // remapping alist cannot change a basic face that neither has a direct
        // mapping nor inherits from another face.
        if !has_direct_mapping && !inherits {
            BufferBasicFaceLookup::Canonical(face_id)
        } else {
            BufferBasicFaceLookup::WindowNamedOverFrameDefault(face_id)
        }
    }

    /// Resolve one of GNU's fixed basic faces in a displayed window.
    ///
    /// The result is canonical when this buffer's remapping cannot affect the
    /// face. Otherwise it is a dynamically keyed realization over the frame's
    /// canonical default, matching `lookup_basic_face` -> `lookup_named_face`.
    fn resolve_remapped_basic_face(
        &self,
        remapping: BufferFaceRemapping,
        face_id: BasicFaceId,
    ) -> ResolvedFace {
        match self.buffer_basic_face_lookup(remapping, face_id) {
            BufferBasicFaceLookup::Canonical(face_id) => {
                let mut resolved = self.resolve_named_face(face_id.name());
                resolved.face_id = u32::from(face_id);
                resolved
            }
            BufferBasicFaceLookup::WindowNamedOverFrameDefault(face_id) => {
                // GNU `lookup_named_face` initializes ATTRS from the frame's
                // canonical DEFAULT_FACE_ID before merging the named face and
                // its buffer-local remappings. Starting from the remapped
                // default would apply a default-only text scale twice.
                let base = self.default_face.clone();
                let mut resolved = self
                    .resolve_remapped_face_value_over(
                        remapping,
                        &base,
                        &Value::symbol(face_id.name()),
                    )
                    .unwrap_or(base);
                // Zero asks the frame face arena for a content-addressed
                // dynamic slot; a remapped basic face cannot occupy its fixed
                // frame slot.
                resolved.face_id = 0;
                resolved.canonical_tty_role = None;
                resolved.lisp_name = Some(face_id.name().to_string());
                resolved
            }
        }
    }

    fn resolve_buffer_named_face_overlay_spec_inner(
        &self,
        remapping: BufferFaceRemapping,
        name: &str,
        remap_stack: &mut Vec<String>,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> Option<FaceSpecContribution> {
        if depth > 40 || name == "nil" {
            return None;
        }

        let alist_bits = remapping.alist().map_or(0, Value::bits);

        // A fold recorded for this exact alist object already encodes that
        // alist's remapping decision for `name`, so a top-level lookup can
        // consult it before walking the alist at all. Recursive resolutions
        // keep the full order below: inside an active remap cycle the plain
        // definition is what the guard must reach, not the completed fold.
        if remap_stack.is_empty()
            && let Some(shared) = self.cached_buffer_named_overlay_spec(name, alist_bits)
        {
            return Some(FaceSpecContribution {
                face: (*shared).clone(),
                identity: Some(shared),
            });
        }

        // GNU face remapping is non-recursive for the face being remapped:
        // `(FACE REMAP FACE)` applies REMAP over FACE's ordinary definition.
        if !remap_stack.iter().any(|active| active == name)
            && let Some(specs) = Self::buffer_face_remapping_specs(remapping, name)
        {
            remap_stack.push(name.to_string());
            let remapped = self.resolve_buffer_face_value_overlay_spec_inner(
                remapping,
                &specs,
                remap_stack,
                depth + 1,
                FaceReferenceDiagnostics::Suppress,
            );
            remap_stack.pop();
            if remapped.is_some() {
                return remapped;
            }
        }

        // Every logical composition is ultimately realized over the already
        // resolved buffer default.  GNU therefore treats an inherited
        // `default` here as a merge point, not as a second fully specified
        // face.  Re-merging its weight/colors into each named source would
        // let a higher-priority `italic` overlay erase a lower-priority
        // `bold` text face.
        if name == "default" {
            return Some(FaceSpecContribution::anonymous(NeoFace::default()));
        }

        // Reaching here means the name's own remapping `assq` missed above,
        // exactly GNU's per-lookup liveness check (merge_named_face,
        // src/xfaces.c:3269-3295). The fold below is then shared per name and
        // alist object identity; see [`NamedSpecEntry`] for the invalidation
        // contract. This second probe still serves recursive resolutions,
        // which skip the hoisted lookup above.
        if let Some(shared) = self.cached_buffer_named_overlay_spec(name, alist_bits) {
            return Some(FaceSpecContribution {
                face: (*shared).clone(),
                identity: Some(shared),
            });
        }

        let Some(mut face) = self.face_table.get(name).cloned() else {
            if diagnostics == FaceReferenceDiagnostics::Report {
                self.invalid_face_references
                    .borrow_mut()
                    .push(name.to_owned());
            }
            return Some(FaceSpecContribution::anonymous(NeoFace::default()));
        };
        let parent = face.inherit.take().and_then(|inherit_ref| {
            self.resolve_buffer_face_value_overlay_spec_inner(
                remapping,
                &inherit_ref,
                remap_stack,
                depth + 1,
                FaceReferenceDiagnostics::Suppress,
            )
        });
        let folded = match parent {
            Some(parent) => parent.face.merge(&face),
            None => face,
        };
        // Only top-level folds enter the memo: one recorded inside an active
        // remap cycle skipped this name's remapping by the guard, and the
        // hoisted lookup above must never answer a remapped name with that
        // plain fold. Cycle folds simply re-fold on their rare revisits.
        let shared = if remap_stack.is_empty() {
            self.record_named_overlay_spec_for_alist(name, folded, alist_bits)
        } else {
            std::sync::Arc::new(folded)
        };
        Some(FaceSpecContribution {
            face: (*shared).clone(),
            identity: Some(shared),
        })
    }

    fn resolve_buffer_face_value_overlay_spec_inner(
        &self,
        remapping: BufferFaceRemapping,
        val: &Value,
        remap_stack: &mut Vec<String>,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> Option<FaceSpecContribution> {
        if depth > 40 {
            return None;
        }
        match val.kind() {
            ValueKind::Nil => None,
            ValueKind::Symbol(_) | ValueKind::String => {
                let name = Self::face_name_from_value(val)?;
                if name == "nil" {
                    return None;
                }
                self.resolve_buffer_named_face_overlay_spec_inner(
                    remapping,
                    name,
                    remap_stack,
                    depth + 1,
                    diagnostics,
                )
            }
            ValueKind::Cons => {
                let items = list_to_vec(val)?;
                if items.is_empty() {
                    return None;
                }
                match self.eval_filtered_face_spec(&items) {
                    FilteredFaceSpec::Matched(filtered_spec) => {
                        // Recurse into the filtered spec (unwrap the :filtered wrapper)
                        return self.resolve_buffer_face_value_overlay_spec_inner(
                            remapping,
                            &Value::list(filtered_spec),
                            remap_stack,
                            depth + 1,
                            diagnostics,
                        );
                    }
                    // Filter didn't match → the remap contributes nothing.
                    FilteredFaceSpec::Rejected => return None,
                    FilteredFaceSpec::NotFiltered => {}
                }
                if Self::face_spec_is_plist(&items) {
                    let mut inline =
                        NeoFace::from_plist_realized("--inline--", &items, self.plist_palette());
                    let parent = inline.inherit.take().and_then(|inherit_ref| {
                        self.resolve_buffer_face_value_overlay_spec_inner(
                            remapping,
                            &inherit_ref,
                            remap_stack,
                            depth + 1,
                            diagnostics,
                        )
                    });
                    return Some(match parent {
                        Some(parent) => FaceSpecContribution::anonymous(parent.face.merge(&inline)),
                        None => FaceSpecContribution::anonymous(inline),
                    });
                }

                let mut composition = UnresolvedFaceComposition::default();
                for item in items.iter().rev() {
                    composition.merge_optional(self.resolve_buffer_face_value_overlay_spec_inner(
                        remapping,
                        item,
                        remap_stack,
                        depth + 1,
                        diagnostics,
                    ));
                }
                composition.attributes.map(|face| FaceSpecContribution {
                    face,
                    identity: composition.identity,
                })
            }
            _ => None,
        }
    }

    pub(crate) fn resolve_buffer_face_value_over<B: LayoutBufferView>(
        &self,
        buffer: &B,
        base: &ResolvedFace,
        val: &Value,
    ) -> Option<ResolvedFace> {
        self.resolve_remapped_face_value_over(BufferFaceRemapping::capture(buffer), base, val)
    }

    pub(crate) fn resolve_remapped_face_value_over(
        &self,
        remapping: BufferFaceRemapping,
        base: &ResolvedFace,
        val: &Value,
    ) -> Option<ResolvedFace> {
        let mut remap_stack = Vec::new();
        self.resolve_buffer_face_value_overlay_spec_inner(
            remapping,
            val,
            &mut remap_stack,
            0,
            FaceReferenceDiagnostics::Report,
        )
        .map(|contribution| {
            self.apply_specified_face_over(base, &contribution.face, contribution.identity.as_ref())
        })
    }

    /// Resolve NAME in a captured displayed-window face environment.
    ///
    /// Keeping this operation beside the remapping interpreter prevents
    /// structural window decorations from reimplementing GNU's
    /// `lookup_named_face(window, ...)` semantics. Basic faces retain their
    /// canonical cache slot only when remapping provably cannot affect them;
    /// other named faces are content-addressed by the frame arena.
    pub(crate) fn resolve_remapped_named_face(
        &self,
        remapping: BufferFaceRemapping,
        name: &str,
    ) -> ResolvedFace {
        if let Some(face_id) = BasicFaceId::from_name(name) {
            return self.resolve_remapped_basic_face(remapping, face_id);
        }
        if remapping.alist().is_none_or(Value::is_nil) {
            return self.resolve_named_face(name);
        }

        let base = self.default_face.clone();
        let mut resolved = self
            .resolve_remapped_face_value_over(remapping, &base, &Value::symbol(name))
            .unwrap_or(base);
        resolved.face_id = BasicFaceId::SENTINEL;
        resolved.canonical_tty_role = None;
        resolved.lisp_name = Some(name.to_string());
        resolved
    }

    /// GNU display_mode_line initializes its iterator with the selected
    /// active/inactive tab-line basic source, then lookup_basic_face applies
    /// buffer remapping (xdisp.c:3257-3290,28145; xfaces.c:5198-5199).
    pub(crate) fn resolve_buffer_tab_line_face<B: LayoutBufferView>(
        &self,
        buffer: &B,
        role: crate::display_face_policy::TabLineFaceRole,
    ) -> ResolvedFace {
        let remapping = BufferFaceRemapping::capture(buffer);
        if !self.has_tty_realization() {
            return self.resolve_remapped_basic_face(remapping, BasicFaceId::TabLine);
        }
        let mut resolved = self.resolve_remapped_named_face(remapping, role.name());
        // Keep every protocol basic ID unchanged. Both GNU tab-line roles can
        // coexist in one frame, so they cannot share the renderer's old slot
        // 19. The row request API explicitly uses zero to allocate a content-
        // addressed ID (display_row/mod.rs DisplayRowSourceRenderRequest::from_base_face);
        // its ordinary nonbasic SENTINEL would be treated as an assigned ID.
        resolved.face_id = 0;
        resolved
    }

    /// Merge all buffer face sources logically, then realize exactly once.
    pub(crate) fn resolve_buffer_face_sources_over<B: LayoutBufferView>(
        &self,
        buffer: &B,
        base: &ResolvedFace,
        sources: &OrderedFaceSources,
    ) -> Option<ResolvedFace> {
        self.resolve_remapped_face_sources_over(BufferFaceRemapping::capture(buffer), base, sources)
    }

    pub(crate) fn resolve_remapped_face_sources_over(
        &self,
        remapping: BufferFaceRemapping,
        base: &ResolvedFace,
        sources: &OrderedFaceSources,
    ) -> Option<ResolvedFace> {
        let mut remap_stack = Vec::new();
        let mut composition = UnresolvedFaceComposition::default();
        for value in sources.values() {
            composition.merge_optional(self.resolve_buffer_face_value_overlay_spec_inner(
                remapping,
                &value,
                &mut remap_stack,
                0,
                FaceReferenceDiagnostics::Report,
            ));
        }
        composition.realize(self, base)
    }

    pub(crate) fn resolve_buffer_default_face<B: LayoutBufferView>(
        &self,
        buffer: &B,
    ) -> ResolvedFace {
        self.resolve_remapped_default_face(BufferFaceRemapping::capture(buffer))
    }

    pub(crate) fn resolve_remapped_default_face(
        &self,
        remapping: BufferFaceRemapping,
    ) -> ResolvedFace {
        // With no buffer-local remapping entry for `default`, the merged
        // result IS the frame default face: the fold below would apply an
        // all-unspecified overlay (`FaceSpecContribution::anonymous`) over
        // `self.default_face` — GNU's `merge_face_ref` over an un-remapped
        // `default` returns the definition unchanged (`merge_named_face`
        // assq miss, src/xfaces.c:3269-3295). Answer the common case with the
        // assq alone and skip the anonymous composition probe entirely.
        if Self::buffer_face_remapping_specs(remapping, "default").is_none() {
            return self.default_face.clone();
        }
        let mut remap_stack = Vec::new();
        self.resolve_buffer_face_value_overlay_spec_inner(
            remapping,
            &Value::symbol("default"),
            &mut remap_stack,
            0,
            FaceReferenceDiagnostics::Report,
        )
        .map(|contribution| {
            self.apply_specified_face_over(
                &self.default_face,
                &contribution.face,
                contribution.identity.as_ref(),
            )
        })
        .unwrap_or_else(|| self.default_face.clone())
    }

    pub fn resolve_face_value_over(
        &self,
        base: &ResolvedFace,
        val: &Value,
    ) -> Option<ResolvedFace> {
        self.resolve_face_value_overlay_spec(*val, 0, FaceReferenceDiagnostics::Report)
            .map(|contribution| {
                self.apply_specified_face_over(
                    base,
                    &contribution.face,
                    contribution.identity.as_ref(),
                )
            })
    }

    /// Merge a frame-local GNU Lisp face over `base` without converting the
    /// numeric lface identity back through a string name.
    pub(crate) fn resolve_lisp_face_over(
        &self,
        base: &ResolvedFace,
        lisp_face_id: neovm_core::face::LispFaceId,
    ) -> Option<ResolvedFace> {
        let face_ref = self.face_table.lisp_face_ref(lisp_face_id)?;
        self.resolve_face_value_overlay_spec(face_ref, 0, FaceReferenceDiagnostics::Suppress)
            .map(|contribution| {
                self.apply_specified_face_over(
                    base,
                    &contribution.face,
                    contribution.identity.as_ref(),
                )
            })
    }

    pub(crate) fn lisp_face_ref(
        &self,
        lisp_face_id: neovm_core::face::LispFaceId,
    ) -> Option<Value> {
        self.face_table.lisp_face_ref(lisp_face_id)
    }

    pub(crate) fn resolve_face_sources_over(
        &self,
        base: &ResolvedFace,
        sources: &OrderedFaceSources,
    ) -> Option<ResolvedFace> {
        let mut composition = UnresolvedFaceComposition::default();
        for value in sources.values() {
            composition.merge_optional(self.resolve_face_value_overlay_spec(
                value,
                0,
                FaceReferenceDiagnostics::Report,
            ));
        }
        composition.realize(self, base)
    }

    /// Resolve face attributes at a buffer position.
    ///
    /// Reads "face" and "font-lock-face" text properties, collects overlay
    /// faces (sorted by priority), merges them via `FaceTable`, and produces
    /// a fully-realized `ResolvedFace`.
    ///
    /// `next_check` is set to the minimum of all property change positions
    /// so the caller can skip per-character lookups until that boundary.
    fn face_at_pos<B: LayoutBufferView>(
        &self,
        buffer: &B,
        charpos: usize,
        next_check: &mut usize,
    ) -> ResolvedFace {
        let bytepos = buffer_charpos_to_emacs_byte_pos(buffer, CharPos0::new(charpos));
        let mut min_next = buffer.layout_point_max_char_pos().get();
        let base = self.resolve_buffer_default_face(buffer);

        // GNU redisplay asks `Fget_text_property' for the effective `face':
        // direct value, category fallback, configured aliases such as
        // `font-lock-face', then `default-text-properties'.
        let face_prop = LayoutCharPropertyLookup::new(buffer, Value::symbol("face"))
            .text_value_at(buffer, bytepos);

        // Update next_check from text property boundaries
        if let Some(nc) = buffer.layout_next_text_prop_change_after_emacs_byte_pos(bytepos) {
            min_next = min_next.min(buffer_emacs_byte_pos_to_charpos(buffer, nc));
        }

        // Overlay faces are collected once by the shared resolver (ascending
        //    priority, windowed overlays filtered to this window). Merge each in
        //    order; higher priority wins by being applied last.
        let overlays = overlay_faces_at(buffer, bytepos, self.current_window_id.get());
        if let Some(boundary) = overlays.next_boundary {
            min_next = min_next.min(buffer_emacs_byte_pos_to_charpos(buffer, boundary));
        }
        let sources = OrderedFaceSources::from_text_and_overlays(face_prop, overlays.faces);
        let resolved = self
            .resolve_buffer_face_sources_over(buffer, &base, &sources)
            .unwrap_or(base);

        // Also consider overlay boundaries so next_check doesn't skip past
        // positions where an overlay starts or ends.
        if let Some(nb) = buffer
            .layout_overlays()
            .next_boundary_after_emacs_byte_pos(bytepos)
        {
            min_next = min_next.min(buffer_emacs_byte_pos_to_charpos(buffer, nb));
        }

        *next_check = min_next;
        resolved
    }

    /// Resolve the base face for an overlay `before-string' or
    /// `after-string'.
    ///
    /// GNU redisplay treats overlay strings specially: their base face depends
    /// only on text properties at the anchor position and ignores overlay
    /// faces/current iterator face.  The overlay string's own text properties
    /// are merged later by the Lisp string display-source iterator.
    pub(crate) fn face_for_overlay_string<B: LayoutBufferView>(
        &self,
        buffer: &B,
        anchor_charpos: usize,
        next_check: &mut usize,
    ) -> ResolvedFace {
        let bytepos = buffer_charpos_to_emacs_byte_pos(buffer, CharPos0::new(anchor_charpos));
        let mut min_next = buffer.layout_point_max_char_pos().get();
        let mut resolved = self.resolve_buffer_default_face(buffer);

        if let Some(face_prop) = LayoutCharPropertyLookup::new(buffer, Value::symbol("face"))
            .text_value_at(buffer, bytepos)
            && let Some(next) = self.resolve_buffer_face_value_over(buffer, &resolved, &face_prop)
        {
            resolved = next;
        }

        if let Some(nc) = buffer.layout_next_text_prop_change_after_emacs_byte_pos(bytepos) {
            min_next = min_next.min(buffer_emacs_byte_pos_to_charpos(buffer, nc));
        }

        *next_check = min_next;
        resolved
    }

    pub(crate) fn base_face_for_origin<B: LayoutBufferView>(
        &self,
        buffer: Option<&B>,
        origin: &DisplayOrigin,
        policy: BaseFacePolicy,
        next_check: &mut usize,
    ) -> ResolvedFace {
        match policy {
            BaseFacePolicy::BufferFaceIncludingOverlays => {
                let buffer = buffer.expect("buffer text face policy requires a buffer");
                let DisplayOrigin::BufferText { charpos } = origin else {
                    unreachable!(
                        "buffer text face policy received incompatible origin: {origin:?}"
                    );
                };
                self.face_at_pos(buffer, charpos.get(), next_check)
            }
            BaseFacePolicy::OverlayStringAtAnchor => {
                let buffer = buffer.expect("overlay string face policy requires a buffer");
                let DisplayOrigin::OverlayString { anchor_charpos, .. } = origin else {
                    unreachable!(
                        "overlay string face policy received incompatible origin: {origin:?}"
                    );
                };
                self.face_for_overlay_string(buffer, anchor_charpos.get(), next_check)
            }
            BaseFacePolicy::DisplayPropertyUnderlyingFace => {
                let buffer = buffer.expect("display property face policy requires a buffer");
                let DisplayOrigin::DisplayPropertyString { anchor_charpos, .. } = origin else {
                    unreachable!(
                        "display property face policy received incompatible origin: {origin:?}"
                    );
                };
                self.face_at_pos(buffer, anchor_charpos.get(), next_check)
            }
            BaseFacePolicy::BufferRemappedBasicFace(face_id) => {
                let buffer = buffer.expect("buffer-remapped basic face policy requires a buffer");
                self.resolve_remapped_basic_face(BufferFaceRemapping::capture(buffer), face_id)
            }
            BaseFacePolicy::FrameBasicFace(face_id) => self.resolve_named_face(face_id.name()),
        }
    }

    pub(crate) fn default_base_face_for_origin<B: LayoutBufferView>(
        &self,
        buffer: Option<&B>,
        origin: &DisplayOrigin,
        next_check: &mut usize,
    ) -> ResolvedFace {
        self.base_face_for_origin(
            buffer,
            origin,
            origin.default_base_face_policy(),
            next_check,
        )
    }

    pub(crate) fn default_base_face_for_origin_without_buffer(
        &self,
        origin: &DisplayOrigin,
    ) -> ResolvedFace {
        match origin.default_base_face_policy() {
            BaseFacePolicy::FrameBasicFace(face_id) => self.resolve_named_face(face_id.name()),
            BaseFacePolicy::BufferRemappedBasicFace(_) => {
                panic!("display origin {origin:?} requires a buffer for basic-face remapping")
            }
            policy => {
                panic!(
                    "display origin {origin:?} requires a buffer for base face policy {policy:?}"
                )
            }
        }
    }

    /// Extract face name(s) from a Lisp Value.
    ///
    /// Face property values can be:
    /// - A symbol naming a face: `Value::Symbol(id)` -> `vec!["face-name"]`
    /// - A list of symbols: each element is a face name
    /// - Nil: no face
    /// - Otherwise: empty vec (unrecognized format)
    pub fn resolve_face_value(val: &Value) -> Vec<String> {
        match val.kind() {
            ValueKind::Nil => Vec::new(),
            ValueKind::Symbol(_) => {
                if let Some(name) = val.as_bare_symbol_name() {
                    if name == "nil" {
                        Vec::new()
                    } else {
                        vec![name.to_string()]
                    }
                } else {
                    Vec::new()
                }
            }
            ValueKind::Cons => {
                // Could be a list of face names, or a plist of face attributes.
                if let Some(items) = list_to_vec(val) {
                    // Check if first item is a keyword (plist like :foreground "red")
                    if Self::face_spec_is_plist(&items) {
                        // Plist face — handled by face_at_pos via face_from_plist().
                        // Return a sentinel that face_at_pos recognizes.
                        vec!["--plist-face--".to_string()]
                    } else {
                        // List of face name symbols.
                        items
                            .iter()
                            .filter_map(|item| {
                                item.as_bare_symbol_name()
                                    .filter(|n| *n != "nil")
                                    .map(|n| n.to_string())
                            })
                            .collect()
                    }
                } else {
                    Vec::new()
                }
            }
            _ => Vec::new(),
        }
    }

    /// Parse an inline face plist like `(:foreground "red" :weight bold)` into
    /// a `Face` object.  Handles the same keywords as GNU Emacs face specs.
    pub fn face_from_plist(&self, val: &Value) -> Option<NeoFace> {
        let items = list_to_vec(val)?;
        Some(NeoFace::from_plist_realized(
            "--inline--",
            &items,
            self.plist_palette(),
        ))
    }

    /// Convert a neovm-core `Face` into a fully-realized `ResolvedFace`.
    ///
    /// Unset fields fall back to the default face.  Handles `inverse_video`,
    /// `FaceHeight` (absolute/relative), underline, overline, strike-through,
    /// box, overstrike, and extend.
    #[inline]
    pub fn realize_face(&self, face: &NeoFace) -> ResolvedFace {
        self.realize_face_with_source(face, FaceRealizationSource::MergeOverDefault)
    }

    fn realize_face_with_source(
        &self,
        face: &NeoFace,
        source: FaceRealizationSource,
    ) -> ResolvedFace {
        let mut rf = self.default_face.clone();
        // The clone starts from the default face; an anonymous face must not
        // inherit its *name*. Named resolvers overwrite this after realizing.
        rf.lisp_name = None;
        match source {
            FaceRealizationSource::MergeOverDefault => {
                rf.retain_source_over(self, &self.default_face, face, None)
            }
            FaceRealizationSource::Complete(source) => {
                rf.source_attributes = Some(source);
                rf.canonical_tty_role = None;
            }
        }

        FaceColorAttributes::from_base(&self.default_face)
            .merge(face)
            .realize()
            .install(&mut rf);

        // Font family
        if let Some(family) = face.family_runtime_string_owned() {
            rf.font_family = family;
        }
        // Font weight
        if let Some(w) = &face.weight {
            rf.font_weight = w.css_weight();
        }
        // Font slant
        if let Some(s) = &face.slant {
            rf.italic = s.is_italic();
        }
        // Font height
        if let Some(h) = &face.height {
            rf.font_size = self.resolve_face_height(h, self.default_face.font_size);
        }

        // Underline
        match &face.underline {
            FaceDecoration::Unspecified => {}
            FaceDecoration::Disabled => {
                rf.underline_style = 0;
                rf.underline_position = NeoUnderlinePosition::FontMetric;
                rf.underline_color = 0;
                rf.terminal_underline_color = None;
            }
            FaceDecoration::Enabled(underline) => {
                rf.underline_style = underline_style_to_u8(&underline.style);
                rf.underline_position = underline.position;
                // GNU draws `:underline t` (no explicit color) in the face's
                // foreground -- e.g. `nobreak-space` inherits `escape-glyph`'s
                // brown fg and underlines in that same brown. Default the
                // unspecified color to `rf.fg` here (fg is applied earlier in
                // this merge), mirroring the box-border resolution below so a
                // downstream pixel-0 underline color means "explicit black"
                // uniformly with box.
                rf.underline_color = underline
                    .color
                    .as_ref()
                    .map(color_to_pixel)
                    .unwrap_or(rf.fg);
                // GNU does NOT default the terminal slot to the foreground:
                // `realize_tty_face` leaves `face->underline_color` at 0 for an
                // underline with no `:color` of its own (src/xfaces.c:6741,
                // :6756) and for `(:color foreground-color)` (:6772-6773), and
                // `turn_on_face` emits nothing for 0 (src/term.c:2120).
                rf.terminal_underline_color =
                    underline.color.as_ref().and_then(|color| color.terminal);
            }
        }
        // Overline
        if let Some(over) = face.overline {
            rf.overline = over;
            // GNU draws `:overline t` (no explicit color) in the face
            // foreground, like `:underline t` and `:box t`. Explicit
            // `:overline COLOR` (just below) overrides.
            if over {
                rf.overline_color = rf.fg;
            }
        }
        if let Some(c) = &face.overline_color {
            rf.overline_color = color_to_pixel(c);
        }
        // Strike-through
        if let Some(st) = face.strike_through {
            rf.strike_through = st;
            // Same rule for `:strike-through t` -> face foreground.
            if st {
                rf.strike_through_color = rf.fg;
            }
        }
        if let Some(c) = &face.strike_through_color {
            rf.strike_through_color = color_to_pixel(c);
        }
        // Box border
        apply_box_border(&mut rf, &face.box_border);
        // Extend
        if let Some(ext) = face.extend {
            rf.extend = ext;
        }
        // Overstrike
        if face.overstrike {
            rf.overstrike = true;
        }

        // Stipple: realize the `:stipple` spec to the XBM pattern the renderer
        // tiles behind glyphs. GNU realizes `LFACE_STIPPLE_INDEX` to a pixmap
        // id in `face->stipple`; neomacs keeps the small bitmap on the face.
        if let Some(pat) = self.realize_stipple(face) {
            rf.stipple = Some(pat);
        }

        rf
    }

    /// Realize a face's inline `:stipple (WIDTH HEIGHT DATA)` bitmap spec into
    /// the XBM [`StipplePattern`](neomacs_display_protocol::StipplePattern) the
    /// renderer tiles. Returns `None` when the face specifies no stipple (or a
    /// non-inline file/symbol form, which is not yet loaded); callers keep the
    /// base face's stipple in that case, matching GNU merge semantics.
    fn realize_stipple(&self, face: &NeoFace) -> Option<neomacs_display_protocol::StipplePattern> {
        let spec = face.stipple.as_ref()?;
        // Inline `(WIDTH HEIGHT DATA)` bitmap spec (what `indent-bars` emits).
        if let Some(items) = list_to_vec(spec) {
            if items.len() == 3
                && let Some(w) = items[0].as_fixnum()
                && let Some(h) = items[1].as_fixnum()
                && let Some(data) = items[2].as_lisp_string()
                && w > 0
                && h > 0
            {
                return Some(neomacs_display_protocol::StipplePattern {
                    width: w as u32,
                    height: h as u32,
                    bits: data.as_bytes().to_vec(),
                });
            }
            return None;
        }
        // A named built-in bitmap (`gray`/`gray1`/`gray3`/...) or an XBM file.
        if let Some(name) = spec.as_utf8_str() {
            return stipple_from_name_or_file(name);
        }
        None
    }

    /// Resolve a face from a Lisp Value (as found in overlay "face" property).
    ///
    /// Returns None if the value doesn't specify any known face names.
    pub fn resolve_face_from_value(&self, val: &Value) -> Option<ResolvedFace> {
        self.resolve_face_value_over(&self.default_face, val)
    }
}

fn underline_style_to_u8(style: &NeoUnderlineStyle) -> u8 {
    style.gnu_code()
}

fn apply_box_border(face: &mut ResolvedFace, border: &FaceDecoration<NeoBoxBorder>) {
    match border {
        FaceDecoration::Unspecified => {}
        FaceDecoration::Disabled => {
            face.box_type = 0;
            face.box_line_width = BoxLineWidth::default();
        }
        FaceDecoration::Enabled(border) => {
            face.box_type = border.style.gnu_code();
            face.box_color = border
                .color
                .as_ref()
                .map(color_to_pixel)
                .unwrap_or_else(|| {
                    match border.style {
                        NeoBoxStyle::Simple => face.fg,
                        // GNU flat-button uses the face background directly;
                        // 3D relief uses it as the base of the shadow colors.
                        NeoBoxStyle::Flat | NeoBoxStyle::Raised | NeoBoxStyle::Pressed => face.bg,
                    }
                });
            face.box_line_width = BoxLineWidth::from_gnu(border.width);
        }
    }
}

#[cfg(test)]
#[path = "neovm_bridge/tests/neovm_bridge_test.rs"]
mod tests;

#[cfg(test)]
#[path = "neovm_bridge/tests/tty_paint_memo_test.rs"]
mod tty_paint_memo_test;

#[cfg(test)]
#[path = "neovm_bridge/tests/face_cache_speculation_test.rs"]
mod face_cache_speculation_test;

#[cfg(test)]
#[path = "neovm_bridge/tests/resolved_identity_test.rs"]
mod resolved_identity_test;

#[cfg(test)]
#[path = "neovm_bridge/tests/composed_source_pin_test.rs"]
mod composed_source_pin_test;

#[cfg(test)]
#[path = "neovm_bridge/tests/line_count_test.rs"]
mod line_count_test;

#[cfg(test)]
#[path = "neovm_bridge/tests/rows_face_witness_test.rs"]
mod rows_face_witness_test;
