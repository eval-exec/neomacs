//! Bridge between neovm-core data types and the layout engine.
//!
//! Provides functions to build `WindowParams` and `FrameParams` from
//! the Rust Context's state, replacing C FFI data sources.

use std::cmp::Ordering;

use neovm_core::buffer::{
    Buffer, BufferTextSnapshot, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    LispCharPos1,
    buffer::{BUFFER_SLOT_COUNT, BufferSlotInfo, lookup_buffer_slot_by_sym_id},
    overlay::{OverlayList, OverlayPropertyAtPoint, OverlayPropertyFilter},
};
use neovm_core::emacs_core::effect_profile::{
    EffectScope, effect_name_from_lisp, effect_operation_from_lisp,
};
use neovm_core::emacs_core::image_catalog::image_scale_environment;
use neovm_core::emacs_core::invisibility::{Invisibility, text_prop_means_invisible};
use neovm_core::emacs_core::plist::plist_get;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::textprop::{DirectCharProperties, resolve_effective_char_property};
use neovm_core::emacs_core::value::{ValueKind, eq_value, list_to_vec};
use neovm_core::emacs_core::{Context, SymId, Value};
use neovm_core::face::{
    BoxStyle as NeoBoxStyle, Color as NeoColor, Face as NeoFace, FaceDecoration, FaceHeight,
    FaceTable, FontWeight, UnderlinePosition as NeoUnderlinePosition,
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
use rustc_hash::FxHashMap;
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
            Some(v) if v.bits() == Value::T.bits() => Self::Absolute,
            Some(value) => value
                .as_symbol_name()
                .and_then(DisplayLineNumbersSymbol::from_symbol_name)
                .map(|symbol| match symbol {
                    DisplayLineNumbersSymbol::Relative => Self::Relative,
                    DisplayLineNumbersSymbol::Visual => Self::Visual,
                })
                .unwrap_or(Self::Off),
            None => Self::Off,
        }
    }
}

pub(crate) trait LayoutBufferView {
    fn layout_is_multibyte(&self) -> bool;
    fn layout_buffer_local_value(&self, var: LayoutVar) -> Option<Value>;
    fn layout_point_min_emacs_byte_pos(&self) -> EmacsBytePos;
    fn layout_point_max_emacs_byte_pos(&self) -> EmacsBytePos;
    fn layout_point_max_char_pos(&self) -> CharPos0;
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
}

#[derive(Clone)]
pub(crate) struct LayoutBufferSnapshot {
    name: String,
    text_snapshot: BufferTextSnapshot,
    accessible_start_emacs_byte: EmacsBytePos,
    accessible_end_emacs_byte: EmacsBytePos,
    accessible_end_char: CharPos0,
    local_var_alist: Value,
    slots: [Value; BUFFER_SLOT_COUNT],
    overlays: OverlayList,
    /// Symbol plists for the category symbols actually referenced by this
    /// buffer's text and overlays. Capturing this sparse set keeps layout
    /// immutable without cloning the evaluator's complete obarray.
    category_symbol_plists: FxHashMap<SymId, Value>,
    /// Every [`LayoutVar`] resolved once at snapshot construction, indexed by
    /// variant. GNU redisplay reads display variables as one memory load
    /// (BVAR fields, or V-globals the buffer-local machinery keeps swapped
    /// in: xdisp.c:3424, xfaces.c:5188); resolving per QUERY instead walked
    /// the buffer's local-var alist every time and measured 3.15% of GUI
    /// typing even with pre-interned symbols.
    vars: [Option<Value>; <LayoutVar as strum::EnumCount>::COUNT],
    /// Non-overlapping ranges compiled from Lisp's live
    /// `composition-function-table`, in ascending buffer-character order.
    automatic_composition_spans: Vec<CharRange>,
}

impl LayoutBufferSnapshot {
    pub fn from_buffer(buffer: &Buffer) -> Self {
        let local_var_alist = buffer.local_var_alist_value();
        let slots = buffer.slot_values_snapshot();
        Self {
            name: buffer.name_runtime_string_owned(),
            text_snapshot: buffer.text_snapshot(),
            accessible_start_emacs_byte: buffer.point_min_emacs_byte_pos(),
            accessible_end_emacs_byte: buffer.point_max_emacs_byte_pos(),
            accessible_end_char: buffer.point_max_char_pos(),
            vars: resolve_layout_vars(local_var_alist, &slots, None),
            local_var_alist,
            slots,
            overlays: buffer.overlays().snapshot_clone(),
            category_symbol_plists: FxHashMap::default(),
            automatic_composition_spans: Vec::new(),
        }
    }

    #[cfg(test)]
    pub fn from_buffer_with_obarray(buffer: &Buffer, obarray: &Obarray) -> Self {
        Self::from_buffer_for_window(buffer, obarray, None)
    }

    /// Snapshot a buffer for one window.
    ///
    /// `visible` bounds the automatic-composition scan to what that window
    /// could possibly display, as `(first_char, char_budget)`. `None` keeps
    /// the whole-buffer scan, which is what a caller with no window in hand
    /// (a test, a display query) must use.
    pub fn from_buffer_for_window(
        buffer: &Buffer,
        obarray: &Obarray,
        visible: Option<(usize, usize)>,
    ) -> Self {
        let mut snapshot = Self::from_buffer(buffer);
        snapshot.vars =
            resolve_layout_vars(snapshot.local_var_alist, &snapshot.slots, Some(obarray));
        snapshot.category_symbol_plists = capture_layout_category_symbol_plists(buffer, obarray);
        snapshot.automatic_composition_spans =
            capture_automatic_composition_spans(buffer, obarray, &snapshot.vars, visible);
        SNAPSHOTS_BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        snapshot
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
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

fn capture_automatic_composition_spans(
    buffer: &Buffer,
    obarray: &Obarray,
    vars: &[Option<Value>; <LayoutVar as strum::EnumCount>::COUNT],
    visible: Option<(usize, usize)>,
) -> Vec<CharRange> {
    if !buffer.get_multibyte()
        || vars[LayoutVar::AutoCompositionMode as usize].is_none_or(Value::is_nil)
        || !vars[LayoutVar::AutoCompositionFunction as usize]
            .is_some_and(|value| value.is_symbol_named("auto-compose-chars"))
    {
        return Vec::new();
    }

    let table_id = Value::symbol("composition-function-table")
        .as_symbol_id()
        .expect("interned symbol has an id");
    let Some(table) = obarray.symbol_value_id(table_id).copied() else {
        return Vec::new();
    };
    // Both paths report ABSOLUTE char positions, so nothing is added here.
    // The bounded one is memoized only in its whole-buffer form: a memo keyed
    // on the buffer alone cannot answer a question that moves with the window.
    let spans = match visible {
        Some((first_char, budget)) => {
            buffer.automatic_composition_spans_visible(table, first_char, budget)
        }
        None => buffer.automatic_composition_spans(table),
    };
    spans
        .iter()
        .map(|span| CharRange::new(CharPos0::new(span.start()), CharPos0::new(span.end())))
        .collect()
}

fn capture_layout_category_symbol_plists(
    buffer: &Buffer,
    obarray: &Obarray,
) -> FxHashMap<SymId, Value> {
    fn remember(category: Value, obarray: &Obarray, plists: &mut FxHashMap<SymId, Value>) {
        if let Some(category_id) = category.as_symbol_id() {
            plists
                .entry(category_id)
                .or_insert_with(|| obarray.symbol_plist_id(category_id));
        }
    }

    let category_property = Value::symbol("category");
    let mut plists = FxHashMap::default();
    let end = buffer.total_emacs_byte_end_pos();
    let mut pos = EmacsBytePos::ZERO;
    while pos < end {
        if let Some(category) =
            buffer.text_props_get_property_at_emacs_byte_pos(pos, category_property)
        {
            remember(category, obarray, &mut plists);
        }
        let Some(next) =
            buffer.text_props_next_single_change_after_emacs_byte_pos(pos, category_property)
        else {
            break;
        };
        if next <= pos {
            break;
        }
        pos = next.min(end);
    }

    for overlay in buffer.overlays().overlays_in_gnu_lists_order() {
        if let Some(category) = buffer
            .overlays()
            .overlay_get_named(overlay, category_property)
        {
            remember(category, obarray, &mut plists);
        }
    }

    plists
}

/// Resolve every [`LayoutVar`] with the same precedence the per-query path
/// used: buffer slot, else the FIRST local-var-alist entry when bound, else
/// (for the curated captures_default subset, and only when an obarray is
/// available) the variable's default value. An alist entry that exists but
/// is unbound shadows nothing — it falls through to the default, exactly
/// like the old `assq`-then-default sequence.
fn resolve_layout_vars(
    local_var_alist: Value,
    slots: &[Value; BUFFER_SLOT_COUNT],
    obarray: Option<&Obarray>,
) -> [Option<Value>; <LayoutVar as strum::EnumCount>::COUNT] {
    use strum::EnumCount;
    use strum::VariantArray;
    const N: usize = <LayoutVar as EnumCount>::COUNT;
    let mut vars: [Option<Value>; N] = [None; N];
    let mut seen_in_alist = [false; N];

    for var in LayoutVar::VARIANTS {
        if let Some(info) = layout_var_info(*var).slot {
            vars[*var as usize] = Some(slots[info.offset.index()]);
        }
    }

    // One walk over the alist for all variables (first entry per symbol
    // wins, matching assq).
    let mut cursor = local_var_alist;
    while cursor.is_cons() {
        let entry = cursor.cons_car();
        cursor = cursor.cons_cdr();
        if !entry.is_cons() {
            continue;
        }
        let Some(var) = entry
            .cons_car()
            .as_symbol_id()
            .and_then(layout_var_by_sym_id)
        else {
            continue;
        };
        let index = var as usize;
        if vars[index].is_some() || seen_in_alist[index] {
            continue;
        }
        seen_in_alist[index] = true;
        let value = entry.cons_cdr();
        if !value.is_unbound() {
            vars[index] = Some(value);
        }
    }

    if let Some(obarray) = obarray {
        for var in LayoutVar::VARIANTS {
            let index = *var as usize;
            if vars[index].is_none() && layout_var_info(*var).captures_default {
                vars[index] = obarray.default_value_id(var.sym_id()).copied();
            }
        }
    }

    vars
}

/// Reverse map sym_id -> LayoutVar for the single alist walk above.
fn layout_var_by_sym_id(sym_id: neovm_core::emacs_core::intern::SymId) -> Option<LayoutVar> {
    use std::sync::OnceLock;
    use strum::VariantArray;
    static MAP: OnceLock<rustc_hash::FxHashMap<neovm_core::emacs_core::intern::SymId, LayoutVar>> =
        OnceLock::new();
    MAP.get_or_init(|| {
        LayoutVar::VARIANTS
            .iter()
            .map(|var| (var.sym_id(), *var))
            .collect()
    })
    .get(&sym_id)
    .copied()
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

impl LayoutBufferView for LayoutBufferSnapshot {
    fn layout_is_multibyte(&self) -> bool {
        self.text_snapshot.is_multibyte()
    }

    fn layout_buffer_local_value(&self, var: LayoutVar) -> Option<Value> {
        self.vars[var as usize]
    }

    fn layout_point_min_emacs_byte_pos(&self) -> EmacsBytePos {
        self.accessible_start_emacs_byte
    }

    fn layout_point_max_emacs_byte_pos(&self) -> EmacsBytePos {
        self.accessible_end_emacs_byte
    }

    fn layout_point_max_char_pos(&self) -> CharPos0 {
        self.accessible_end_char
    }

    fn layout_total_emacs_byte_len(&self) -> EmacsByteLen {
        self.text_snapshot.emacs_byte_len()
    }

    fn layout_char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos {
        self.text_snapshot
            .char_pos_to_emacs_byte_pos(charpos.min(self.accessible_end_char))
    }

    fn layout_emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0 {
        self.text_snapshot
            .emacs_byte_pos_to_char_pos(bytepos.min(self.accessible_end_emacs_byte))
    }

    fn layout_copy_emacs_byte_range_to(&self, range: EmacsByteRange, out: &mut Vec<u8>) {
        self.text_snapshot.copy_emacs_byte_range_to(range, out);
    }

    fn layout_try_for_each_emacs_byte_range_chunk<E>(
        &self,
        range: EmacsByteRange,
        f: impl FnMut(&[u8]) -> Result<(), E>,
    ) -> Result<(), E> {
        self.text_snapshot
            .try_for_each_emacs_byte_range_chunk(range, f)
    }

    fn layout_emacs_byte_at_pos(&self, pos: EmacsBytePos) -> Option<u8> {
        self.text_snapshot.emacs_byte_at_pos(pos)
    }

    fn layout_text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value> {
        self.text_snapshot.text_prop_at_emacs_byte_pos(pos, name)
    }

    fn layout_category_symbol_property(&self, category: Value, property: Value) -> Option<Value> {
        let category_id = category.as_symbol_id()?;
        let plist = self.category_symbol_plists.get(&category_id).copied()?;
        plist_get(plist, &property)
    }

    fn layout_next_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .next_text_prop_change_after_emacs_byte_pos(pos)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .next_single_text_prop_change_after_emacs_byte_pos(pos, name)
    }

    fn layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(
        &self,
        pos: EmacsBytePos,
        name: Value,
        limit: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .next_single_text_prop_change_after_emacs_byte_pos_bounded(pos, name, limit)
    }

    fn layout_previous_single_text_prop_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        name: Value,
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .previous_single_text_prop_change_before_emacs_byte_pos(pos, name)
    }

    fn layout_overlays(&self) -> &OverlayList {
        &self.overlays
    }

    fn layout_automatic_composition_starting_at(&self, pos: CharPos0) -> Option<CharRange> {
        let index = self
            .automatic_composition_spans
            .partition_point(|range| range.start() < pos);
        self.automatic_composition_spans
            .get(index)
            .copied()
            .filter(|range| range.start() == pos)
    }

    fn layout_next_automatic_composition_start(
        &self,
        pos: CharPos0,
        limit: CharPos0,
    ) -> Option<CharPos0> {
        let index = self
            .automatic_composition_spans
            .partition_point(|range| range.start() < pos);
        self.automatic_composition_spans
            .get(index)
            .map(|range| range.start())
            .filter(|start| *start < limit)
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

fn effective_buffer_value(buffer: &Buffer, obarray: &Obarray, var: LayoutVar) -> Option<Value> {
    buffer
        .buffer_local_value_id(var.sym_id())
        .or_else(|| obarray.symbol_value_id(var.sym_id()).copied())
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
    obarray: &Obarray,
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
        image_scale_environment: image_scale_environment(frame, obarray),
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

fn effective_buffer_int(buffer: &Buffer, obarray: &Obarray, var: LayoutVar, default: i64) -> i64 {
    match effective_buffer_value(buffer, obarray, var) {
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

fn effective_buffer_bool(buffer: &Buffer, obarray: &Obarray, var: LayoutVar) -> bool {
    match effective_buffer_value(buffer, obarray, var) {
        Some(v) if v.is_nil() => false,
        None => false,
        Some(_) => true,
    }
}

fn value_non_nil(value: Option<Value>) -> bool {
    value.is_some_and(|value| !value.is_nil())
}

fn value_is_symbol(value: Option<Value>, name: &str) -> bool {
    value.is_some_and(|value| value.as_symbol_name() == Some(name))
}

pub(crate) fn window_parameter_by_name(window: &Window, name: &str) -> Option<Value> {
    window
        .parameters()
        .iter()
        .find(|(key, _)| key.as_symbol_name() == Some(name))
        .map(|(_, value)| *value)
}

fn window_wants_mode_line(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    obarray: &Obarray,
    is_minibuffer: bool,
) -> bool {
    let window_mode_line_format = window_parameter_by_name(window, "mode-line-format");
    window.is_leaf()
        && !is_minibuffer
        && !value_is_symbol(window_mode_line_format, "none")
        && (value_non_nil(window_mode_line_format)
            || value_non_nil(effective_buffer_value(
                buffer,
                obarray,
                LayoutVar::ModeLineFormat,
            )))
        && window.bounds().height > frame.char_height
}

fn window_wants_header_line(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    obarray: &Obarray,
    is_minibuffer: bool,
    wants_mode_line: bool,
) -> bool {
    let window_header_line_format = window_parameter_by_name(window, "header-line-format");
    let required_rows = if wants_mode_line { 2.0 } else { 1.0 };
    window.is_leaf()
        && !is_minibuffer
        && !value_is_symbol(window_header_line_format, "none")
        && (value_non_nil(window_header_line_format)
            || value_non_nil(effective_buffer_value(
                buffer,
                obarray,
                LayoutVar::HeaderLineFormat,
            )))
        && window.bounds().height > required_rows * frame.char_height
}

fn window_wants_tab_line(
    window: &Window,
    buffer: &Buffer,
    frame: &Frame,
    obarray: &Obarray,
    is_minibuffer: bool,
    wants_mode_line: bool,
    wants_header_line: bool,
) -> bool {
    let window_tab_line_format = window_parameter_by_name(window, "tab-line-format");
    let required_rows = (if wants_mode_line { 1.0 } else { 0.0 })
        + (if wants_header_line { 1.0 } else { 0.0 })
        + 1.0;
    window.is_leaf()
        && !is_minibuffer
        && !value_is_symbol(window_tab_line_format, "none")
        && (value_non_nil(window_tab_line_format)
            || value_non_nil(effective_buffer_value(
                buffer,
                obarray,
                LayoutVar::TabLineFormat,
            )))
        && window.bounds().height > required_rows * frame.char_height
}

fn global_bool(obarray: &Obarray, name: &str) -> bool {
    obarray
        .symbol_value(name)
        .is_some_and(|value| !value.is_nil())
}

fn effective_nobreak_char_display(buffer: &Buffer, obarray: &Obarray) -> NobreakDisplayMode {
    // `nobreak-char-display` is a `DEFVAR_LISP`, but GNU reads `Vnobreak_char_display`
    // while displaying buffer text (xdisp.c:8522, with the window's buffer current),
    // so a buffer-local binding takes effect. Read buffer-local-then-global, not the
    // raw global.
    match effective_buffer_value(buffer, obarray, LayoutVar::NobreakCharDisplay) {
        Some(value) if value.is_nil() => NobreakDisplayMode::Literal,
        Some(value) if value.is_t() => {
            if global_bool(obarray, "nobreak-char-ascii-display") {
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
    obarray: &Obarray,
    hscroll: usize,
) -> LineWrapMode {
    if effective_buffer_bool(buffer, obarray, LayoutVar::TruncateLines) {
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
        match effective_buffer_value(buffer, obarray, LayoutVar::TruncatePartialWidthWindows) {
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
        let sym_id = sym.as_symbol_id()?;
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

fn buffer_fill_column_indicator(buffer: &Buffer, obarray: &Obarray) -> Option<(i32, char)> {
    // GNU `fill_column_indicator_column` in xdisp.c enables the indicator only
    // when `display-fill-column-indicator` is non-nil, the indicator character
    // satisfies CHARACTERP, and the effective column is a nonnegative integer.
    // Read the EFFECTIVE value (buffer-local, else global/default) for all four:
    // `display-fill-column-indicator-column` in particular defaults to `t` (use
    // fill-column) and is rarely set locally, so a buffer-local-only read returns
    // None and disables the indicator even when the mode is on.
    if !effective_buffer_bool(buffer, obarray, LayoutVar::DisplayFillColumnIndicator) {
        return None;
    }

    let character_value = effective_buffer_value(
        buffer,
        obarray,
        LayoutVar::DisplayFillColumnIndicatorCharacter,
    )?;
    if !character_value.is_char() {
        return None;
    }
    let character = character_value.as_char()?;

    let column_value = match effective_buffer_value(
        buffer,
        obarray,
        LayoutVar::DisplayFillColumnIndicatorColumn,
    ) {
        Some(value) if value.bits() == Value::T.bits() => {
            effective_buffer_value(buffer, obarray, LayoutVar::FillColumn)
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
        match method.as_symbol_name() {
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
        .or_else(|| value.as_symbol_name().map(str::to_string))
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

fn frame_cursor_foreground_pixel(frame: &Frame, face_table: &FaceTable, obarray: &Obarray) -> u32 {
    // GNU's Vx_cursor_fore_pixel is a color-name string when explicitly set;
    // otherwise x_set_cursor_color uses FRAME_BACKGROUND_PIXEL.
    obarray
        .symbol_value("x-cursor-fore-pixel")
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
        let key = pair[0].as_symbol_name()?;
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
    obarray: &Obarray,
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
        obarray,
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
    obarray: &Obarray,
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
    // Only leaf windows can be laid out.
    let effective_window_system = frame.effective_window_system();
    let is_window_system = effective_window_system.is_some();
    let window_system =
        effective_window_system.and_then(|v| v.as_symbol_name().map(|s| s.to_string()));

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
    let wrap_mode = effective_wrap_mode(window, buffer, frame, obarray, hscroll);
    let word_wrap = effective_buffer_bool(buffer, obarray, LayoutVar::WordWrap);
    let tab_width = effective_buffer_int(buffer, obarray, LayoutVar::TabWidth, 8) as i32;
    let display_line_numbers = DisplayLineNumbersMode::from_lisp_value(effective_buffer_value(
        buffer,
        obarray,
        LayoutVar::DisplayLineNumbers,
    ))
    .for_window_kind(window_kind);
    // GNU `try_scrolling` reads `scroll-conservatively` / `scroll-margin`
    // (buffer-local with a global fallback) to choose between minimal scrolling
    // and recentering point when point jumps off-screen (src/xdisp.c).
    let scroll_conservatively =
        effective_buffer_int(buffer, obarray, LayoutVar::ScrollConservatively, 0);
    let scroll_step = effective_buffer_int(buffer, obarray, LayoutVar::ScrollStep, 0);
    let scroll_minibuffer_conservatively = obarray
        .symbol_value("scroll-minibuffer-conservatively")
        .is_none_or(|value| !value.is_nil());
    let scroll_margin = effective_buffer_int(buffer, obarray, LayoutVar::ScrollMargin, 0);

    // GNU window.c gates chrome reservation through window_wants_*:
    // a mode/header/tab line is shown only for leaf non-minibuffer
    // windows whose window parameter is not `none`, whose window
    // parameter or buffer-local format is non-nil, and whose window is
    // high enough to hold the requested chrome.
    let wants_mode_line = window_wants_mode_line(window, buffer, frame, obarray, is_minibuffer);
    let wants_header_line = window_wants_header_line(
        window,
        buffer,
        frame,
        obarray,
        is_minibuffer,
        wants_mode_line,
    );
    let wants_tab_line = window_wants_tab_line(
        window,
        buffer,
        frame,
        obarray,
        is_minibuffer,
        wants_mode_line,
        wants_header_line,
    );

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
    let cursor_foreground = frame_cursor_foreground_pixel(frame, face_table, obarray);
    let cursor_effects = parse_cursor_effect_profile(window_cursor_effect).or_else(|| {
        buffer_local_value(buffer, LayoutVar::NeomacsCursorEffect)
            .and_then(parse_cursor_effect_profile)
    });
    let visual_cursors = parse_visual_cursors(buffer, cursor_color);
    let x_stretch_cursor = global_bool(obarray, "x-stretch-cursor");
    let fill_column_indicator = buffer_fill_column_indicator(buffer, obarray);

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
            &resolve_window_chrome_face(DisplayOrigin::TabLine),
            char_height,
            device_scale,
        )
    } else {
        0.0
    };

    let buffer_default_face = face_resolver.resolve_buffer_default_face(buffer);
    let default_fg = buffer_default_face.fg;
    let default_bg = buffer_default_face.bg;

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
        char_width,
        char_height,
        window_system: is_window_system,
        font_pixel_size: frame.font_pixel_size,
        image_scale_environment: image_scale_environment(frame, obarray),
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
        indicate_empty_lines: if effective_buffer_bool(
            buffer,
            obarray,
            LayoutVar::IndicateEmptyLines,
        ) {
            1
        } else {
            0
        },
        show_trailing_whitespace: effective_buffer_bool(
            buffer,
            obarray,
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
        nobreak_char_display: effective_nobreak_char_display(buffer, obarray),
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
    let frame_params = frame_params_from_neovm(frame, evaluator.face_table(), evaluator.obarray());

    let mut window_params = Vec::new();

    // Collect leaf windows from the root window tree.
    let leaf_ids = frame.root_window.leaf_ids();
    for win_id in &leaf_ids {
        let Some(window) = frame.root_window.find(*win_id) else {
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
            evaluator.obarray(),
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
            frame.root_window.bounds().x,
            frame.root_window.bounds().y,
            frame.root_window.bounds().width,
            frame.root_window.bounds().height,
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
                evaluator.obarray(),
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
        let mut count: i64 = 0;
        self.buffer
            .layout_try_for_each_emacs_byte_range_chunk(range, |chunk| {
                count += chunk.iter().filter(|byte| **byte == b'\n').count() as i64;
                Ok::<(), std::convert::Infallible>(())
            })
            .expect("newline counting is infallible");
        count
    }

    /// Byte position just AFTER the `n`-th newline at or after `byte_from`,
    /// or `None` when fewer than `n` newlines remain. Chunked scan with early
    /// exit — cost is proportional to the distance to the `n`-th newline,
    /// not to the buffer tail.
    pub fn find_nth_newline_after(&self, byte_from: i64, n: usize) -> Option<i64> {
        if n == 0 {
            return Some(byte_from);
        }
        let range = clamped_layout_emacs_byte_range(self.buffer, byte_from, self.zv())?;
        let mut remaining = n;
        let mut offset: i64 = byte_from;
        let mut found: Option<i64> = None;
        let _ = self
            .buffer
            .layout_try_for_each_emacs_byte_range_chunk(range, |chunk| {
                for (i, byte) in chunk.iter().enumerate() {
                    if *byte == b'\n' {
                        remaining -= 1;
                        if remaining == 0 {
                            found = Some(offset + i as i64 + 1);
                            return Err(());
                        }
                    }
                }
                offset += chunk.len() as i64;
                Ok(())
            });
        found
    }

    /// Whether any structure-affecting source exists in `[byte_from, byte_to)`
    /// that could make the layout walk CONSUME buffer text beyond simple
    /// line-by-line reading: overlays (display/invisible/before/after
    /// strings), or `display` / `invisible` text properties. Used to gate the
    /// bounded window read — when any is present the caller falls back to
    /// reading the full accessible tail.
    pub fn has_walk_consumption_hazard(&self, byte_from: i64, byte_to: i64) -> bool {
        if !self.buffer.layout_overlays().is_empty() {
            return true;
        }
        let Some(from) = layout_emacs_byte_pos_from_i64(byte_from) else {
            return true;
        };
        let Some(to) = layout_emacs_byte_pos_from_i64(byte_to) else {
            return true;
        };
        for prop in ["display", "invisible"] {
            let name = Value::symbol(prop);
            let mut pos = from;
            loop {
                if pos >= to {
                    break;
                }
                if self
                    .buffer
                    .layout_text_prop_at_emacs_byte_pos(pos, name)
                    .is_some()
                {
                    return true;
                }
                match self
                    .buffer
                    .layout_next_single_text_prop_change_after_emacs_byte_pos_bounded(pos, name, to)
                {
                    Some(next) if next > pos => pos = next,
                    _ => break,
                }
            }
        }
        false
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
    lookup_order: Vec<Value>,
    default: Option<Value>,
}

impl LayoutCharPropertyLookup {
    pub(crate) fn new<B: LayoutBufferView + ?Sized>(buffer: &B, property: Value) -> Self {
        let mut lookup_order = vec![property];
        if let Some(mut alist) = buffer.layout_buffer_local_value(LayoutVar::CharPropertyAliasAlist)
        {
            while alist.is_cons() {
                let entry = alist.cons_car();
                alist = alist.cons_cdr();
                if !entry.is_cons() || entry.cons_car().bits() != property.bits() {
                    continue;
                }
                let mut aliases = entry.cons_cdr();
                while aliases.is_cons() {
                    let alias = aliases.cons_car();
                    if !lookup_order
                        .iter()
                        .any(|existing| existing.bits() == alias.bits())
                    {
                        lookup_order.push(alias);
                    }
                    aliases = aliases.cons_cdr();
                }
                break;
            }
        }
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
        let (canonical, aliases) = self.lookup_order.split_first()?;
        resolve_effective_char_property(
            DirectCharProperties::from_getter(
                |property| buffer.layout_text_prop_at_emacs_byte_pos(bytepos, property),
                *canonical,
            ),
            |category, property| buffer.layout_category_symbol_property(category, property),
            *canonical,
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
        let mut watched = self.lookup_order.clone();
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
        let (canonical, aliases) = self.lookup_order.split_first()?;
        let overlays = buffer.layout_overlays();
        resolve_effective_char_property(
            DirectCharProperties::from_getter(
                |property| overlays.overlay_get_named(overlay, property),
                *canonical,
            ),
            |category, property| buffer.layout_category_symbol_property(category, property),
            *canonical,
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
                .iter()
                .copied()
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
        let Some((canonical, aliases)) = self.lookup_order.split_first() else {
            return Vec::new();
        };
        let overlays = buffer.layout_overlays();
        let mut overlay_ids = overlays.overlays_at_emacs_byte_pos(bytepos);
        overlays.sort_overlay_ids_by_priority_desc(&mut overlay_ids);
        overlay_ids.reverse();
        overlay_ids
            .into_iter()
            .filter(|overlay| overlays.overlay_applies_to_window(*overlay, current_window_id))
            .filter_map(|overlay| {
                resolve_effective_char_property(
                    DirectCharProperties::from_getter(
                        |property| overlays.overlay_get_named(overlay, property),
                        *canonical,
                    ),
                    |category, property| buffer.layout_category_symbol_property(category, property),
                    *canonical,
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
                crate::display_property::classify_display_property(value)
                    .replacement()
                    .is_some()
            })
    }

    /// Whether `[char_from, char_to)` contains an effective property that can
    /// collapse source rows during the display walk.
    ///
    /// Unlike [`RustBufferAccess::has_walk_consumption_hazard`], this predicate
    /// is deliberately range- and semantics-aware.  A face-only overlay, an
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
    fn invisible_status_at(&self, charpos: i64) -> InvisibleStatus {
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
#[derive(Clone, Debug, PartialEq)]
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
    /// Family inherited from the frame's realized base fontset for non-ASCII
    /// lookup.  GNU changes `font_family` when an inline face specifies
    /// `:family`, but the face's realized fontset still descends from the
    /// frame default face (`xfaces.c:6305-6370`).
    pub fontset_base_family: String,
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
            fontset_base_family: String::new(),
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
            font_char_width: 0.0,
            font_ascent: 0.0,
            font_line_height: 0.0,
            face_id: 0, // DEFAULT_FACE_ID
            lisp_name: None,
            stipple: None,
        }
    }
}

impl ResolvedFace {
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
    pub(crate) fn set_foreground(&mut self, color: &NeoColor) {
        self.fg = color_to_pixel(color);
        self.terminal_fg = color.terminal;
        self.use_default_foreground = false;
    }

    /// Assign GNU's whole realized background from one realized colour.
    pub(crate) fn set_background(&mut self, color: &NeoColor) {
        self.bg = color_to_pixel(color);
        self.terminal_bg = color.terminal;
        self.use_default_background = false;
    }

    /// The frame's fallback foreground, used when the default face specifies
    /// none.  It never came from `tty-color-desc`, so it carries no terminal
    /// colour: GNU's `FACE_TTY_DEFAULT_FG_COLOR`, which `turn_on_face` emits
    /// no colour for at all.
    pub(crate) fn set_frame_default_foreground(&mut self, pixel: u32) {
        self.fg = pixel;
        self.terminal_fg = None;
        self.use_default_foreground = true;
    }

    /// The frame's fallback background; see
    /// [`Self::set_frame_default_foreground`].
    pub(crate) fn set_frame_default_background(&mut self, pixel: u32) {
        self.bg = pixel;
        self.terminal_bg = None;
        self.use_default_background = true;
    }
}

/// The terminal channel a default-color sentinel belongs to.
///
/// ANSI has distinct "default foreground" and "default background" values;
/// a boolean attached to the destination slot cannot represent one after
/// inverse-video moves it to the other slot.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FaceColorSlot {
    Foreground,
    Background,
}

/// A face color before it is assigned to its post-inverse destination slot.
///
/// It carries the realized terminal index next to the pixel because inverse
/// video MOVES a colour between slots: GNU `realize_tty_face` maps both source
/// colours through `map_tty_color` and then swaps the results
/// (src/xfaces.c:6800-6810), so whatever the writer emits for the foreground
/// must be exactly what was realized for the background.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalFaceColor {
    Concrete {
        pixel: u32,
        terminal: Option<TerminalColor>,
    },
    TerminalDefault {
        slot: FaceColorSlot,
        fallback_pixel: u32,
    },
}

impl TerminalFaceColor {
    fn from_resolved_slot(
        pixel: u32,
        terminal: Option<TerminalColor>,
        defaulted: bool,
        slot: FaceColorSlot,
    ) -> Self {
        if defaulted {
            Self::TerminalDefault {
                slot,
                fallback_pixel: pixel,
            }
        } else {
            Self::Concrete { pixel, terminal }
        }
    }

    fn materialize_in(self, destination: FaceColorSlot) -> (u32, Option<TerminalColor>, bool) {
        match self {
            Self::Concrete { pixel, terminal } => (pixel, terminal, false),
            Self::TerminalDefault {
                slot,
                fallback_pixel,
            } if slot == destination => (fallback_pixel, None, true),
            // ANSI cannot select the terminal's default background as a
            // foreground (or vice versa). GNU realizes the frame color first
            // and swaps that concrete color, so use the carried fallback.
            //
            // That fallback is a frame pixel, not a `tty-color-desc` answer, so
            // it carries no terminal colour: GNU's `FACE_TTY_DEFAULT_COLOR`,
            // which `turn_on_face` emits nothing for.
            Self::TerminalDefault { fallback_pixel, .. } => (fallback_pixel, None, false),
        }
    }

    fn is_terminal_default(self) -> bool {
        matches!(self, Self::TerminalDefault { .. })
    }
}

/// Apply GNU TTY inverse-video realization to a fully merged face.
///
/// GNU `realize_tty_face` maps both source colors and then swaps them. When
/// both are terminal defaults it can preserve that intent with reverse-video;
/// otherwise a default color crossing channels must become its concrete frame
/// fallback instead of changing into the other channel's default sentinel.
fn apply_resolved_face_inverse_video(face: &mut ResolvedFace) {
    let foreground = TerminalFaceColor::from_resolved_slot(
        face.fg,
        face.terminal_fg,
        face.use_default_foreground,
        FaceColorSlot::Foreground,
    );
    let background = TerminalFaceColor::from_resolved_slot(
        face.bg,
        face.terminal_bg,
        face.use_default_background,
        FaceColorSlot::Background,
    );

    if foreground.is_terminal_default() && background.is_terminal_default() {
        face.terminal_inverse_video = true;
        return;
    }

    (face.fg, face.terminal_fg, face.use_default_foreground) =
        background.materialize_in(FaceColorSlot::Foreground);
    (face.bg, face.terminal_bg, face.use_default_background) =
        foreground.materialize_in(FaceColorSlot::Background);
    face.terminal_inverse_video = false;
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

    pub(crate) fn values(&self) -> impl Iterator<Item = Value> + '_ {
        self.sources.iter().map(|source| match source {
            OrderedFaceSource::TextProperty(value) | OrderedFaceSource::Overlay(value) => *value,
        })
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
    fn merge(&mut self, contribution: NeoFace) {
        self.attributes = Some(match self.attributes.take() {
            Some(attributes) => attributes.merge(&contribution),
            None => contribution,
        });
    }

    fn merge_optional(&mut self, contribution: Option<NeoFace>) {
        if let Some(contribution) = contribution {
            self.merge(contribution);
        }
    }

    fn realize(self, resolver: &FaceResolver, base: &ResolvedFace) -> Option<ResolvedFace> {
        self.attributes
            .map(|attributes| resolver.apply_specified_face_over(base, &attributes))
    }
}

/// Resolves face attributes at buffer positions using the neovm-core
/// `FaceTable`, text properties, and overlays.
///
/// Replaces the C FFI `face_at_buffer_position()` path for the pure-Rust
/// backend.
pub struct FaceResolver {
    face_table: FaceTable,
    default_face: ResolvedFace,
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
    font_sizing: FontSizing,
    frame_font_domain: FrameFontDomain,
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
            Some(v) if v.is_keyword() => true,
            Some(item) => item
                .as_symbol_name()
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
        df.fontset_base_family = df.font_family.clone();
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
        // Box
        if let Some(bb) = neo_default.box_border.enabled() {
            df.box_type = box_style_to_u8(&bb.style);
            df.box_color = bb.color.as_ref().map(color_to_pixel).unwrap_or(0);
            df.box_line_width = BoxLineWidth::from_gnu(bb.width);
        }

        Self {
            face_table: face_table.clone(),
            default_face: df,
            window_system,
            current_window_parameters: std::cell::RefCell::new(Vec::new()),
            current_window_id: std::cell::Cell::new(None),
            invalid_face_references: std::cell::RefCell::new(Vec::new()),
            font_sizing,
            frame_font_domain,
        }
    }

    /// The terminal palette an ANONYMOUS attribute plist realizes against, or
    /// `None` on a GUI frame.
    ///
    /// Named faces are realized in neovm-core, which calls `tty-color-desc`
    /// itself, as GNU does.  A plist -- a `face` text property, an overlay, a
    /// `face-remapping-alist` entry -- is realized HERE, and this crate calls no
    /// Lisp function anywhere, so it runs GNU's own search over the terminal's
    /// own palette instead.  It comes off the face table so it cannot be a
    /// different palette from the one the named faces used.
    fn plist_palette(&self) -> Option<&neomacs_display_protocol::TtyPalette> {
        self.face_table.tty_palette()
    }

    /// Discard diagnostics from a speculative frame-layout attempt.
    pub(crate) fn clear_diagnostics(&self) {
        self.invalid_face_references.borrow_mut().clear();
    }

    /// Drain invalid named face references from the accepted layout attempt.
    pub(crate) fn take_invalid_face_references(&self) -> Vec<String> {
        self.invalid_face_references.take()
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
        face.inverse_video = None;
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

    fn apply_specified_face_over(&self, base: &ResolvedFace, face: &NeoFace) -> ResolvedFace {
        let mut rf = base.clone();
        if let Some(c) = &face.foreground {
            rf.set_foreground(c);
        }
        if let Some(c) = &face.background {
            rf.set_background(c);
        }
        match face.inverse_video {
            Some(true) => apply_resolved_face_inverse_video(&mut rf),
            Some(false) => rf.terminal_inverse_video = false,
            None => {}
        }

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
        match &face.box_border {
            FaceDecoration::Unspecified => {}
            FaceDecoration::Disabled => {
                rf.box_type = 0;
                rf.box_line_width = BoxLineWidth::default();
            }
            FaceDecoration::Enabled(box_border) => {
                rf.box_type = box_style_to_u8(&box_border.style);
                rf.box_color = box_border
                    .color
                    .as_ref()
                    .map(color_to_pixel)
                    .unwrap_or(rf.fg);
                rf.box_line_width = BoxLineWidth::from_gnu(box_border.width);
            }
        }
        if let Some(extend) = face.extend {
            rf.extend = extend;
        }
        if face.overstrike {
            rf.overstrike = true;
        }

        // Distant-foreground: swap fg when too close to bg
        if let Some(dfg) = &face.distant_foreground
            && colors_close(rf.fg, rf.bg)
        {
            rf.set_foreground(dfg);
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
    ) -> NeoFace {
        if depth > 40 {
            return NeoFace::default();
        }
        if name == "default" && depth > 0 {
            return NeoFace::default();
        }

        let Some(face) = self.face_table.get(name).cloned() else {
            if diagnostics == FaceReferenceDiagnostics::Report {
                self.invalid_face_references
                    .borrow_mut()
                    .push(name.to_owned());
            }
            return NeoFace::default();
        };
        self.resolve_face_overlay_spec(face, depth, FaceReferenceDiagnostics::Suppress)
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
            .unwrap_or_default()
    }

    /// Resolve one GNU face reference to logical attributes, without realizing
    /// it against a base face yet.
    fn resolve_face_value_overlay_spec(
        &self,
        face_ref: Value,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> Option<NeoFace> {
        if depth > 40 || face_ref.is_nil() || face_ref.is_symbol_named("nil") {
            return None;
        }

        if let Some(name) = Self::face_name_from_value(&face_ref) {
            return Some(self.resolve_named_face_overlay_spec(name, depth, diagnostics));
        }

        let Some(items) = list_to_vec(&face_ref) else {
            return None;
        };
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
            return Some(self.resolve_face_overlay_spec(face, depth + 1, diagnostics));
        }

        let mut composition = UnresolvedFaceComposition::default();
        for item in items.iter().rev() {
            composition.merge_optional(self.resolve_face_value_overlay_spec(
                *item,
                depth + 1,
                diagnostics,
            ));
        }
        composition.attributes
    }

    fn face_name_from_value(value: &Value) -> Option<&str> {
        match value.kind() {
            ValueKind::Symbol(_) => value.as_symbol_name(),
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
        let Some(name) = items.first().and_then(|first| first.as_symbol_name()) else {
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
            let Some(pred_name) = filter_items.get(i).and_then(|pred| pred.as_symbol_name()) else {
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
                        .and_then(|val| val.as_symbol_name())
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
            .is_some_and(|inherit| !inherit.is_nil() && !inherit.is_symbol_named("unspecified"));

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
    ) -> Option<NeoFace> {
        if depth > 40 || name == "nil" {
            return None;
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
            return Some(NeoFace::default());
        }

        let Some(mut face) = self.face_table.get(name).cloned() else {
            if diagnostics == FaceReferenceDiagnostics::Report {
                self.invalid_face_references
                    .borrow_mut()
                    .push(name.to_owned());
            }
            return Some(NeoFace::default());
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
        Some(match parent {
            Some(parent) => parent.merge(&face),
            None => face,
        })
    }

    fn resolve_buffer_face_value_overlay_spec_inner(
        &self,
        remapping: BufferFaceRemapping,
        val: &Value,
        remap_stack: &mut Vec<String>,
        depth: usize,
        diagnostics: FaceReferenceDiagnostics,
    ) -> Option<NeoFace> {
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
                        Some(parent) => parent.merge(&inline),
                        None => inline,
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
                composition.attributes
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
        .map(|attributes| self.apply_specified_face_over(base, &attributes))
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
        resolved.lisp_name = Some(name.to_string());
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
        let mut remap_stack = Vec::new();
        self.resolve_buffer_face_value_overlay_spec_inner(
            remapping,
            &Value::symbol("default"),
            &mut remap_stack,
            0,
            FaceReferenceDiagnostics::Report,
        )
        .map(|attributes| self.apply_specified_face_over(&self.default_face, &attributes))
        .unwrap_or_else(|| self.default_face.clone())
    }

    pub fn resolve_face_value_over(
        &self,
        base: &ResolvedFace,
        val: &Value,
    ) -> Option<ResolvedFace> {
        self.resolve_face_value_overlay_spec(*val, 0, FaceReferenceDiagnostics::Report)
            .map(|attributes| self.apply_specified_face_over(base, &attributes))
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
            .map(|attributes| self.apply_specified_face_over(base, &attributes))
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
                if let Some(name) = val.as_symbol_name() {
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
                                item.as_symbol_name()
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
    pub fn realize_face(&self, face: &NeoFace) -> ResolvedFace {
        let mut rf = self.default_face.clone();
        // The clone starts from the default face; an anonymous face must not
        // inherit its *name*. Named resolvers overwrite this after realizing.
        rf.lisp_name = None;

        // Foreground
        if let Some(c) = &face.foreground {
            rf.set_foreground(c);
        }
        // Background
        if let Some(c) = &face.background {
            rf.set_background(c);
        }
        // Inverse video: swap fg and bg
        match face.inverse_video {
            Some(true) => apply_resolved_face_inverse_video(&mut rf),
            Some(false) => rf.terminal_inverse_video = false,
            None => {}
        }

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
        match &face.box_border {
            FaceDecoration::Unspecified => {}
            FaceDecoration::Disabled => {
                rf.box_type = 0;
                rf.box_line_width = BoxLineWidth::default();
            }
            FaceDecoration::Enabled(bb) => {
                rf.box_type = box_style_to_u8(&bb.style);
                rf.box_color = bb.color.as_ref().map(color_to_pixel).unwrap_or(rf.fg);
                rf.box_line_width = BoxLineWidth::from_gnu(bb.width);
            }
        }
        // Extend
        if let Some(ext) = face.extend {
            rf.extend = ext;
        }
        // Overstrike
        if face.overstrike {
            rf.overstrike = true;
        }

        // Distant-foreground: GNU Emacs (xfaces.c) uses this when the
        // foreground is too close to the background, improving readability.
        // Check if fg ≈ bg and substitute distant-foreground if available.
        if let Some(dfg) = &face.distant_foreground
            && colors_close(rf.fg, rf.bg)
        {
            rf.set_foreground(dfg);
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

fn box_style_to_u8(style: &NeoBoxStyle) -> u8 {
    style.gnu_code()
}

#[cfg(test)]
#[path = "neovm_bridge_test.rs"]
mod tests;
