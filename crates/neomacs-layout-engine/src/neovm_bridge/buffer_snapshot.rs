//! Evaluator-owned buffer capture for the canonical layout walk.
//!
//! Keeps captured fields private and exposes them through `LayoutBufferView`.
//! Worker input types must not depend on this module or retain its Lisp values.

use super::{
    LayoutBufferView, LayoutVar, SNAPSHOTS_BUILT, effective_buffer_value, layout_var_info,
};
use neovm_core::buffer::{
    Buffer, BufferTextSnapshot, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    overlay::{OverlayList, OverlaySnapshot},
};
use neovm_core::emacs_core::plist::plist_get;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::{SymId, Value};
use rustc_hash::FxHashMap;

/// Immutable view for a walk on the evaluator thread.
///
/// Captured properties and composition rules still contain live Lisp values.
/// This is not an owned worker job; resolving those values belongs here before
/// any future row-layout job crosses to another thread.
#[derive(Clone)]
pub(crate) struct LayoutBufferSnapshot {
    display_target: crate::display_property::DisplayPropertyTarget,
    /// `(when FORM . SPEC)` results for the window walk this snapshot serves.
    display_when: crate::display_when::DisplayWhenConditions,
    name: String,
    text_snapshot: BufferTextSnapshot,
    accessible_start_emacs_byte: EmacsBytePos,
    accessible_end_emacs_byte: EmacsBytePos,
    accessible_end_char: CharPos0,
    source_context_end: CharPos0,
    overlays: OverlaySnapshot,
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
    string_composition_rules: Option<neovm_core::emacs_core::composite::AutomaticCompositionRules>,
}

impl LayoutBufferSnapshot {
    pub fn from_buffer(buffer: &Buffer) -> Self {
        Self::capture_buffer(buffer, None)
    }

    /// Project a query's source boundary without narrowing the live buffer.
    /// Ordinary runs stop here, while complete composed elements retain the
    /// original narrowed buffer as their readable context.
    pub(crate) fn with_accessible_end(mut self, end: CharPos0) -> Self {
        self.accessible_end_char = end.min(self.accessible_end_char);
        self.accessible_end_emacs_byte = self
            .text_snapshot
            .char_pos_to_emacs_byte_pos(self.accessible_end_char);
        self
    }

    // Resolve layout variables once, with the caller's global defaults already
    // available. Window snapshots must not build and then replace a complete
    // buffer-local-only variable table on every layout attempt.
    fn capture_buffer(buffer: &Buffer, obarray: Option<&Obarray>) -> Self {
        Self {
            display_when: crate::display_when::DisplayWhenConditions::structural(),
            display_target: crate::display_property::DisplayPropertyTarget::Graphical,
            name: buffer.name_runtime_string_owned(),
            text_snapshot: buffer.text_snapshot(),
            accessible_start_emacs_byte: buffer.point_min_emacs_byte_pos(),
            accessible_end_emacs_byte: buffer.point_max_emacs_byte_pos(),
            accessible_end_char: buffer.point_max_char_pos(),
            source_context_end: buffer.point_max_char_pos(),
            vars: resolve_layout_vars(buffer, obarray),
            overlays: buffer.overlays().snapshot(),
            category_symbol_plists: FxHashMap::default(),
            automatic_composition_spans: Vec::new(),
            string_composition_rules: None,
        }
    }

    #[cfg(test)]
    pub fn from_buffer_with_obarray(buffer: &Buffer, obarray: &Obarray) -> Self {
        Self::from_buffer_for_window(
            buffer,
            obarray,
            None,
            crate::display_property::DisplayPropertyTarget::Graphical,
        )
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
        target: crate::display_property::DisplayPropertyTarget,
    ) -> Self {
        let mut snapshot = Self::capture_buffer(buffer, Some(obarray));
        snapshot.display_target = target;
        snapshot.category_symbol_plists = capture_layout_category_symbol_plists(buffer, obarray);
        snapshot.automatic_composition_spans =
            capture_automatic_composition_spans(buffer, obarray, &snapshot.vars, visible);
        snapshot.string_composition_rules = capture_string_composition_rules(buffer, obarray);
        SNAPSHOTS_BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        snapshot
    }

    /// Attach the `(when FORM . SPEC)` results evaluated for this walk.
    pub fn with_display_when(
        mut self,
        display_when: crate::display_when::DisplayWhenConditions,
    ) -> Self {
        self.display_when = display_when;
        self
    }

    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

pub(super) fn capture_automatic_composition_spans(
    buffer: &Buffer,
    obarray: &Obarray,
    vars: &[Option<Value>; <LayoutVar as strum::EnumCount>::COUNT],
    visible: Option<(usize, usize)>,
) -> Vec<CharRange> {
    if !buffer.get_multibyte() {
        return Vec::new();
    }
    let Some(table) = active_composition_table(
        vars[LayoutVar::AutoCompositionMode as usize],
        vars[LayoutVar::AutoCompositionFunction as usize],
        obarray,
    ) else {
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

/// Resolve the same automatic composer as buffer redisplay, but leave the
/// multibyte gate to the string object, as GNU composition_compute_stop_pos
/// does. The caller keeps the live Lisp tables rooted during redisplay.
pub(super) fn capture_string_composition_rules(
    buffer: &Buffer,
    obarray: &Obarray,
) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
    let value = |var| effective_buffer_value(buffer, obarray, var);
    let table = active_composition_table(
        value(LayoutVar::AutoCompositionMode),
        value(LayoutVar::AutoCompositionFunction),
        obarray,
    )?;
    neovm_core::emacs_core::composite::AutomaticCompositionRules::new(buffer, table)
}

fn active_composition_table(
    mode: Option<Value>,
    function: Option<Value>,
    obarray: &Obarray,
) -> Option<Value> {
    if mode.is_none_or(Value::is_nil)
        || !function.is_some_and(|value| value.is_symbol_named("auto-compose-chars"))
    {
        return None;
    }
    obarray.symbol_value("composition-function-table").copied()
}

fn capture_layout_category_symbol_plists(
    buffer: &Buffer,
    obarray: &Obarray,
) -> FxHashMap<SymId, Value> {
    fn remember(category: Value, obarray: &Obarray, plists: &mut FxHashMap<SymId, Value>) {
        if let Some(category_id) = category.as_symbol_id() {
            neovm_core::emacs_core::symbol::SymbolPropertyRevision::observe(category_id);
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

    if !buffer.overlays().may_contain_property(category_property) {
        return plists;
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
pub(super) fn resolve_layout_vars(
    buffer: &Buffer,
    obarray: Option<&Obarray>,
) -> [Option<Value>; <LayoutVar as strum::EnumCount>::COUNT] {
    use strum::EnumCount;
    use strum::VariantArray;
    const N: usize = <LayoutVar as EnumCount>::COUNT;
    let mut vars: [Option<Value>; N] = [None; N];
    let slots = buffer.slot_values_snapshot();
    for var in LayoutVar::VARIANTS {
        let info = layout_var_info(*var);
        // The buffer's derived index preserves first-binding and unbound
        // semantics. Scanning every alist entry here also observes unrelated
        // binding conses: command-local writes such as deactivate-mark then
        // invalidate otherwise reusable query geometry.
        let local = if let Some(slot) = info.slot {
            Some(slots[slot.offset.index()])
        } else {
            buffer.buffer_local_value_id(var.sym_id())
        };
        vars[*var as usize] = local.or_else(|| {
            info.captures_default
                .then(|| obarray?.default_value_id(var.sym_id()).copied())
                .flatten()
        });
    }
    vars
}

impl LayoutBufferView for LayoutBufferSnapshot {
    fn layout_display_target(&self) -> crate::display_property::DisplayPropertyTarget {
        self.display_target
    }
    fn layout_string_composition_rules(
        &self,
    ) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
        self.string_composition_rules
    }
    fn layout_display_when_conditions(&self) -> crate::display_when::DisplayWhenConditions {
        self.display_when.clone()
    }
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

    fn layout_measurement_context_end(&self) -> Option<CharPos0> {
        (self.accessible_end_char < self.source_context_end).then_some(self.source_context_end)
    }

    fn layout_total_emacs_byte_len(&self) -> EmacsByteLen {
        self.text_snapshot.emacs_byte_len()
    }

    fn layout_char_pos_to_emacs_byte_pos(&self, charpos: CharPos0) -> EmacsBytePos {
        self.text_snapshot
            .char_pos_to_emacs_byte_pos(charpos.min(self.source_context_end))
    }

    fn layout_emacs_byte_pos_to_char_pos(&self, bytepos: EmacsBytePos) -> CharPos0 {
        self.text_snapshot.emacs_byte_pos_to_char_pos(
            bytepos.min(
                self.text_snapshot
                    .char_pos_to_emacs_byte_pos(self.source_context_end),
            ),
        )
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

    fn layout_indexed_newline_count(&self, range: EmacsByteRange) -> Option<usize> {
        self.text_snapshot.indexed_newline_count(range)
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
