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
use neovm_core::emacs_core::eval::VariableView;
use neovm_core::emacs_core::plist::plist_get;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::{SymId, Value};
use neovm_core::tagged::SymbolMode;
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
    #[cfg(test)]
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
    fn capture_buffer(buffer: &Buffer, variables: Option<VariableView<'_>>) -> Self {
        Self {
            display_when: crate::display_when::DisplayWhenConditions::structural(),
            display_target: crate::display_property::DisplayPropertyTarget::Graphical,
            name: buffer.name_runtime_string_owned(),
            text_snapshot: buffer.text_snapshot(),
            accessible_start_emacs_byte: buffer.point_min_emacs_byte_pos(),
            accessible_end_emacs_byte: buffer.point_max_emacs_byte_pos(),
            accessible_end_char: buffer.point_max_char_pos(),
            source_context_end: buffer.point_max_char_pos(),
            vars: resolve_layout_vars(buffer, variables),
            overlays: buffer.overlays().snapshot(),
            category_symbol_plists: FxHashMap::default(),
            automatic_composition_spans: Vec::new(),
            string_composition_rules: None,
        }
    }

    #[cfg(test)]
    pub fn from_buffer_with_obarray(buffer: &Buffer, vars: VariableView<'_>) -> Self {
        Self::from_buffer_for_window(
            buffer,
            vars,
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
    #[cfg(test)]
    pub fn from_buffer_for_window(
        buffer: &Buffer,
        vars: VariableView<'_>,
        visible: Option<(usize, usize)>,
        target: crate::display_property::DisplayPropertyTarget,
    ) -> Self {
        Self::from_buffer_for_window_with_symbols(buffer, vars, visible, target, SymbolMode::Bare)
    }

    /// Capture the evaluator's symbol policy before the buffer walk starts.
    pub fn from_buffer_for_window_with_symbols(
        buffer: &Buffer,
        vars: VariableView<'_>,
        visible: Option<(usize, usize)>,
        target: crate::display_property::DisplayPropertyTarget,
        symbol_mode: SymbolMode,
    ) -> Self {
        let obarray = vars.obarray();
        let mut snapshot = Self::capture_buffer(buffer, Some(vars));
        snapshot.display_target = target;
        snapshot.category_symbol_plists = capture_layout_category_symbol_plists(buffer, obarray);
        snapshot.automatic_composition_spans =
            capture_automatic_composition_spans(buffer, vars, &snapshot.vars, visible, symbol_mode);
        snapshot.string_composition_rules =
            capture_string_composition_rules(buffer, vars, symbol_mode);
        SNAPSHOTS_BUILT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        snapshot
    }

    /// Attach the `(when FORM . SPEC)` results evaluated for this walk.
    /// The walk's `when` conditions, borrowed (the trait accessor clones).
    pub(crate) fn display_when_conditions(&self) -> &crate::display_when::DisplayWhenConditions {
        &self.display_when
    }

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
    variables: VariableView<'_>,
    vars: &[Option<Value>; <LayoutVar as strum::EnumCount>::COUNT],
    visible: Option<(usize, usize)>,
    symbol_mode: SymbolMode,
) -> Vec<CharRange> {
    if !buffer.get_multibyte() {
        return Vec::new();
    }
    let Some(table) = active_composition_table(
        vars[LayoutVar::AutoCompositionMode as usize],
        vars[LayoutVar::AutoCompositionFunction as usize],
        variables,
        symbol_mode,
    ) else {
        return Vec::new();
    };
    // Both paths report ABSOLUTE char positions, so nothing is added here.
    // The bounded one is memoized only in its whole-buffer form: a memo keyed
    // on the buffer alone cannot answer a question that moves with the window.
    let spans = match visible {
        Some((first_char, budget)) => buffer.automatic_composition_spans_visible_with_symbols(
            table,
            first_char,
            budget,
            symbol_mode,
        ),
        None => buffer.automatic_composition_spans_with_symbols(table, symbol_mode),
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
    vars: VariableView<'_>,
    symbol_mode: SymbolMode,
) -> Option<neovm_core::emacs_core::composite::AutomaticCompositionRules> {
    let value = |var| effective_buffer_value(buffer, vars, var);
    let table = active_composition_table(
        value(LayoutVar::AutoCompositionMode),
        value(LayoutVar::AutoCompositionFunction),
        vars,
        symbol_mode,
    )?;
    neovm_core::emacs_core::composite::AutomaticCompositionRules::new_with_symbols(
        buffer,
        table,
        symbol_mode,
    )
}

fn active_composition_table(
    mode: Option<Value>,
    function: Option<Value>,
    vars: VariableView<'_>,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    if mode.is_none_or(Value::is_nil)
        || !function.is_some_and(|value| value.is_symbol_named("auto-compose-chars", symbol_mode))
    {
        return None;
    }
    vars.value(crate::fixed_symbols::COMPOSITION_FUNCTION_TABLE.get())
}

fn capture_layout_category_symbol_plists(
    buffer: &Buffer,
    obarray: &Obarray,
) -> FxHashMap<SymId, Value> {
    fn remember(category: Value, obarray: &Obarray, plists: &mut FxHashMap<SymId, Value>) {
        if let Some(category_id) = category.as_bare_symbol_id() {
            neovm_core::emacs_core::symbol::SymbolPropertyRevision::observe(category_id);
            plists
                .entry(category_id)
                .or_insert_with(|| obarray.symbol_plist_id(category_id));
        }
    }

    let category_property = Value::symbol(&crate::fixed_symbols::CATEGORY);
    let mut plists = FxHashMap::default();
    let end = buffer.total_char_end_pos();
    let mut pos = CharPos0::ZERO;
    while pos < end {
        if let Some(category) = buffer.text_props_get_property_at_char_pos(pos, category_property) {
            remember(category, obarray, &mut plists);
        }
        // This indexed run guarantees constant syntax-table AND category
        // values. A generic single-property walk also reads every unrelated
        // plist in the gap, exhausting the layout dependency budget in long
        // histories with only a few category-bearing buttons.
        let next = buffer.syntax_prop_free_run_end_at_char_pos(pos, end);
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

#[cfg(test)]
mod category_capture_tests {
    use super::*;
    use neovm_core::emacs_core::eval::Context;
    use neovm_core::tagged::collection_reads::capture;

    #[test]
    fn absent_categories_do_not_observe_unrelated_property_plists() {
        let mut eval = Context::new();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
        buffer.insert("history\n");
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(EmacsBytePos::ZERO, buffer.point_max_emacs_byte_pos()),
            Value::symbol("face"),
            Value::symbol("default"),
        );
        let plist = buffer.interval_plist_at_char_pos(CharPos0::new(0)).unwrap();
        let (categories, reads) = capture(|| {
            capture_layout_category_symbol_plists(
                eval.buffer_manager().get(id).unwrap(),
                eval.obarray(),
            )
        });
        assert!(categories.is_empty());
        let reads = reads.expect("coherent category capture");
        assert!(reads.unchanged());
        plist.cons_cdr().set_car(Value::symbol("highlight"));
        assert!(
            reads.unchanged(),
            "proven-absent categories must not scan or depend on unrelated face plists"
        );
    }

    #[test]
    fn sparse_categories_in_long_history_preserve_capture_budget() {
        let mut eval = Context::new();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let buffer = eval.buffer_manager_mut().get_mut(id).unwrap();
        let count = 9_000;
        buffer.insert(&"x".repeat(count));
        for index in 0..count {
            buffer.text_props_put_property_in_emacs_byte_range(
                EmacsByteRange::new(EmacsBytePos::new(index), EmacsBytePos::new(index + 1)),
                Value::symbol("history-event"),
                Value::fixnum(index as i64),
            );
        }
        buffer.text_props_put_property_in_emacs_byte_range(
            EmacsByteRange::new(
                EmacsBytePos::new(count / 2),
                EmacsBytePos::new(count / 2 + 1),
            ),
            Value::symbol("category"),
            Value::symbol("history-button"),
        );
        let (categories, reads) = capture(|| {
            capture_layout_category_symbol_plists(
                eval.buffer_manager().get(id).unwrap(),
                eval.obarray(),
            )
        });
        assert_eq!(categories.len(), 1);
        assert!(
            reads.is_some(),
            "unrelated history must not exhaust a visible layout's dependency budget"
        );
    }

    #[test]
    fn category_capture_keeps_text_and_overlay_symbol_properties() {
        let mut eval = Context::new();
        eval.eval_str(
            "(progn (insert \"abc\")
               (put 'text-category 'face 'bold)
               (put 'overlay-category 'face 'italic)
               (overlay-put (make-overlay 2 3) 'category 'overlay-category))",
        )
        .unwrap();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let overlay_only = capture_layout_category_symbol_plists(
            eval.buffer_manager().get(id).unwrap(),
            eval.obarray(),
        );
        assert_eq!(overlay_only.len(), 1);
        assert!(
            overlay_only.contains_key(
                &Value::symbol("overlay-category")
                    .as_bare_symbol_id()
                    .unwrap()
            )
        );
        eval.eval_str("(put-text-property 1 2 'category 'text-category)")
            .unwrap();
        let categories = capture_layout_category_symbol_plists(
            eval.buffer_manager().get(id).unwrap(),
            eval.obarray(),
        );
        assert_eq!(categories.len(), 2);
        for (name, face) in [("text-category", "bold"), ("overlay-category", "italic")] {
            let id = Value::symbol(name).as_bare_symbol_id().unwrap();
            assert_eq!(
                plist_get(categories[&id], &Value::symbol("face")),
                Some(Value::symbol(face))
            );
        }

        // Clearing text categories keeps a conservative presence summary;
        // overlay categories still participate independently.
        eval.eval_str("(remove-text-properties 1 4 '(category nil))")
            .unwrap();
        let categories = capture_layout_category_symbol_plists(
            eval.buffer_manager().get(id).unwrap(),
            eval.obarray(),
        );
        assert_eq!(categories.len(), 1);
        assert!(
            categories.contains_key(
                &Value::symbol("overlay-category")
                    .as_bare_symbol_id()
                    .unwrap()
            )
        );
    }

    #[test]
    fn category_runs_follow_multibyte_edits_and_syntax_boundaries() {
        let mut eval = Context::new();
        eval.eval_str(
            "(progn (insert \"历史输出abc\")
               (put-text-property 2 4 'category 'first-category)
               (put-text-property 3 6 'syntax-table '(2))
               (put-text-property 6 8 'category 'last-category))",
        )
        .unwrap();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        for edit in [
            "nil",
            "(progn (goto-char 1) (insert \"新\"))",
            "(progn (remove-text-properties 1 (point-max) '(category nil))
                    (put-text-property 4 5 'category 'replacement-category))",
        ] {
            eval.eval_str(edit).unwrap();
            let buffer = eval.buffer_manager().get(id).unwrap();
            let captured = capture_layout_category_symbol_plists(buffer, eval.obarray());
            let mut expected = FxHashMap::default();
            // An exhaustive character walk is independent of the sparse index
            // and its memo; it also catches accidental byte/character mixing.
            for pos in 0..buffer.total_char_end_pos().get() {
                if let Some(category) = buffer
                    .text_props_get_property_at_char_pos(
                        CharPos0::new(pos),
                        Value::symbol("category"),
                    )
                    .and_then(|value| value.as_bare_symbol_id())
                {
                    expected.insert(category, eval.obarray().symbol_plist_id(category));
                }
            }
            assert_eq!(captured, expected);
        }
    }
}

/// Resolve every [`LayoutVar`] with the same precedence the per-query path
/// used: buffer slot, else the FIRST local-var-alist entry when bound, else
/// (for the curated captures_default subset, and only when an obarray is
/// available) the variable's default value. An alist entry that exists but
/// is unbound shadows nothing — it falls through to the default, exactly
/// like the old `assq`-then-default sequence.
pub(super) fn resolve_layout_vars(
    buffer: &Buffer,
    variables: Option<VariableView<'_>>,
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
                .then(|| variables?.default_value(var.sym_id()))
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
        self.text_snapshot
            .indexed_newline_count(range, neovm_core::buffer::LineIndexBuild::IfLarge)
    }

    fn layout_text_prop_at_emacs_byte_pos(&self, pos: EmacsBytePos, name: Value) -> Option<Value> {
        self.text_snapshot.text_prop_at_emacs_byte_pos(pos, name)
    }

    fn layout_category_symbol_property(&self, category: Value, property: Value) -> Option<Value> {
        let category_id = category.as_bare_symbol_id()?;
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

    fn layout_next_watched_text_prop_state_change_after_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        keys: &[Value],
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .next_watched_property_state_change_after_emacs_byte_pos(pos, keys)
    }

    fn layout_previous_watched_text_prop_state_change_before_emacs_byte_pos(
        &self,
        pos: EmacsBytePos,
        keys: &[Value],
    ) -> Option<EmacsBytePos> {
        self.text_snapshot
            .previous_watched_property_state_change_before_emacs_byte_pos(pos, keys)
    }

    fn layout_text_prop_present_anywhere(&self, name: Value) -> bool {
        self.text_snapshot.text_props_has_property_anywhere(name)
    }

    fn layout_syntax_prop_free_run_end_char_pos(&self, pos: CharPos0, cap: CharPos0) -> CharPos0 {
        self.text_snapshot
            .syntax_prop_free_run_end_at_char_pos(pos, cap)
    }

    fn layout_previous_syntax_prop_state_run_start_char_pos(&self, pos: CharPos0) -> CharPos0 {
        self.text_snapshot
            .previous_syntax_prop_state_run_start_at_char_pos(pos)
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
