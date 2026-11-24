//! Evaluator-owned buffer capture for the canonical layout walk.
//!
//! Keeps captured fields private and exposes them through `LayoutBufferView`.
//! Worker input types must not depend on this module or retain its Lisp values.

use super::{
    LayoutBufferView, LayoutVar, SNAPSHOTS_BUILT, effective_buffer_value, layout_var_info,
};
use neovm_core::buffer::{
    Buffer, BufferTextSnapshot, CharPos0, CharRange, EmacsByteLen, EmacsBytePos, EmacsByteRange,
    overlay::{LiveOverlayView, OverlayRead, OverlaySnapshot},
};
use neovm_core::emacs_core::eval::VariableView;
use neovm_core::emacs_core::owned_roots::{OwnedRootRetainer, OwnedRoots, OwnedTextPropertyRoots};
use neovm_core::emacs_core::plist::plist_get;
use neovm_core::emacs_core::symbol::Obarray;
use neovm_core::emacs_core::{SymId, Value};
use neovm_core::tagged::SymbolMode;
use rustc_hash::FxHashMap;
use std::sync::OnceLock;

#[cfg(test)]
#[path = "tests/snapshot_overlay_roots_test.rs"]
mod snapshot_overlay_roots_test;

#[cfg(test)]
#[path = "tests/snapshot_vars_category_roots_test.rs"]
mod snapshot_vars_category_roots_test;

#[cfg(test)]
#[path = "tests/live_overlay_view_test.rs"]
mod live_overlay_view_test;

/// `NEOMACS_LAYOUT_OVERLAY_SOURCE`: selects how layout reads overlays.
///
/// | Value | Behavior |
/// | --- | --- |
/// | `captured` (default / unset) | Copy overlays into a captured `OverlaySnapshot` with native root lease. |
/// | `live` | Read the buffer's live `OverlayIndex` under a generation stamp (`LiveOverlayView`), with zero overlay copies or root leases. |
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverlaySourceMode {
    Captured,
    Live,
}

fn parse_overlay_source_mode(value: Option<&std::ffi::OsStr>) -> OverlaySourceMode {
    match value.and_then(|v| v.to_str()) {
        Some(raw) if raw.eq_ignore_ascii_case("live") => OverlaySourceMode::Live,
        _ => OverlaySourceMode::Captured,
    }
}

#[cfg(test)]
thread_local! {
    static TEST_OVERLAY_SOURCE_OVERRIDE: std::cell::Cell<Option<OverlaySourceMode>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn with_overlay_source_mode_for_test<R>(
    mode: OverlaySourceMode,
    f: impl FnOnce() -> R,
) -> R {
    struct Reset(Option<OverlaySourceMode>);
    impl Drop for Reset {
        fn drop(&mut self) {
            TEST_OVERLAY_SOURCE_OVERRIDE.with(|slot| slot.set(self.0));
        }
    }
    let previous = TEST_OVERLAY_SOURCE_OVERRIDE.with(|slot| slot.replace(Some(mode)));
    let _reset = Reset(previous);
    f()
}

pub(crate) fn overlay_source_mode() -> OverlaySourceMode {
    #[cfg(test)]
    if let Some(override_mode) = TEST_OVERLAY_SOURCE_OVERRIDE.with(|slot| slot.get()) {
        return override_mode;
    }
    static MODE: OnceLock<OverlaySourceMode> = OnceLock::new();
    *MODE.get_or_init(|| {
        parse_overlay_source_mode(std::env::var_os("NEOMACS_LAYOUT_OVERLAY_SOURCE").as_deref())
    })
}

#[derive(Clone)]
enum SnapshotOverlays {
    Captured(OverlaySnapshot),
    Live(LiveOverlayView),
}

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
    // Registration is constant time; detached table operands are traced at GC.
    _text_property_roots: Option<OwnedTextPropertyRoots>,
    accessible_start_emacs_byte: EmacsBytePos,
    accessible_end_emacs_byte: EmacsBytePos,
    accessible_end_char: CharPos0,
    source_context_end: CharPos0,
    /// GNU `long_line_optimizations_p` at capture (`src/xdisp.c:20646`).
    long_line_optimizations: bool,
    overlays: SnapshotOverlays,
    // Clones retain this native lease independently of the buffer cache.
    // Test-only pure captures may omit it; the production constructor requires it.
    _overlay_roots: Option<OwnedRoots>,
    // The exact variable values, category plists, and composition tables held
    // by this view, registered before it can reach a Lisp callback.
    _captured_roots: Option<OwnedRoots>,
    // Answers attach after capture, so they need a separate registration point.
    root_retainer: Option<OwnedRootRetainer>,
    _display_when_roots: Option<OwnedRoots>,
    /// Character span of the window-range overlay view. `None` is a
    /// whole-buffer view. A walk that consumes at or past the end must be
    /// retried with the whole buffer, so a range view can never commit a
    /// subset answer. The category text scan is bounded by the same span.
    overlay_view_span: Option<(CharPos0, CharPos0)>,
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
    /// GNU `composition_gstring_width` for the automatic composition that
    /// starts at each key (character position), resolved on the evaluator
    /// thread before the walk.  Plain pixels, so a prepared row job can
    /// carry them across threads.
    automatic_composition_advances: FxHashMap<usize, f32>,
    string_composition_rules: Option<neovm_core::emacs_core::composite::AutomaticCompositionRules>,
}

impl LayoutBufferSnapshot {
    #[cfg(test)]
    pub fn from_buffer(buffer: &Buffer) -> Self {
        Self::capture_buffer(buffer, None, None, None, None, false)
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

    /// Exclusive character end of the window-range overlay view, or `None` for
    /// a whole-buffer view. A walk that consumes at or past it is answered
    /// from a subset and must be retried with the whole buffer.
    pub(crate) fn overlay_view_end_char(&self) -> Option<CharPos0> {
        self.overlay_view_span.map(|(_, end)| end)
    }

    // Resolve layout variables once, with the caller's global defaults already
    // available. Window snapshots must not build and then replace a complete
    // buffer-local-only variable table on every layout attempt.
    fn capture_buffer(
        buffer: &Buffer,
        variables: Option<VariableView<'_>>,
        visible: Option<(usize, usize)>,
        overlay_span: Option<(usize, usize)>,
        root_retainer: Option<&OwnedRootRetainer>,
        force_captured: bool,
    ) -> Self {
        let use_live = !force_captured && overlay_source_mode() == OverlaySourceMode::Live;
        let (overlays, overlay_view_span, overlay_roots) = if use_live {
            // Live view reads the buffer's generation-stamped Arc core directly:
            // whole-buffer coverage, zero copies, zero view entries, zero roots.
            (
                SnapshotOverlays::Live(buffer.overlays().live_view()),
                None,
                None,
            )
        } else {
            // `overlay_span` is the character span the walk consumed last time
            // (plus margin), so the view covers what this walk can consume. A
            // caller with no previous end keeps the whole-buffer view; the walk
            // retries with that view if it consumes past a range view.
            let (captured, span) = match overlay_span {
                Some((first, end)) => {
                    let total = buffer.total_char_end_pos().get();
                    let end = end.min(total);
                    let view = buffer.overlay_snapshot_in_char_span(first, end);
                    // A span that reaches the buffer end has nothing beyond it:
                    // do not arm the exhaustion retry for it.
                    let span = (end < total)
                        .then_some((CharPos0::new(first.min(end)), CharPos0::new(end)));
                    (view, span)
                }
                None => (buffer.overlays().snapshot(), None),
            };
            // Capture runs on this buffer's evaluator with no callback or GC
            // between copying the overlays and registering their native lease.
            // Tracing an overlay header also traces its copied plist spine.
            let roots = root_retainer.map(|retainer| {
                let dumped = captured.dump_overlays();
                neovm_core::buffer::overlay::note_overlay_roots_retained(dumped.len());
                retainer
                    .retain(dumped)
                    .expect("layout capture requires its live evaluator root registry")
            });
            (SnapshotOverlays::Captured(captured), span, roots)
        };
        let text_snapshot = buffer.text_snapshot();
        let text_property_roots = root_retainer.map(|retainer| {
            retainer
                .retain_text_properties(text_snapshot.text_property_roots())
                .expect("layout capture requires its live evaluator root registry")
        });
        Self {
            display_when: crate::display_when::DisplayWhenConditions::structural(),
            display_target: crate::display_property::DisplayPropertyTarget::Graphical,
            name: buffer.name_runtime_string_owned(),
            text_snapshot,
            _text_property_roots: text_property_roots,
            accessible_start_emacs_byte: buffer.point_min_emacs_byte_pos(),
            accessible_end_emacs_byte: buffer.point_max_emacs_byte_pos(),
            accessible_end_char: buffer.point_max_char_pos(),
            source_context_end: buffer.point_max_char_pos(),
            long_line_optimizations: buffer.long_line_optimizations_p(),
            vars: resolve_layout_vars(buffer, variables),
            overlays,
            _overlay_roots: overlay_roots,
            _captured_roots: None,
            root_retainer: root_retainer.cloned(),
            _display_when_roots: None,
            overlay_view_span,
            category_symbol_plists: FxHashMap::default(),
            automatic_composition_spans: Vec::new(),
            automatic_composition_advances: FxHashMap::default(),
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
        Self::capture_window_with_symbols(
            buffer,
            vars,
            visible,
            visible,
            target,
            SymbolMode::Bare,
            None,
            false,
        )
    }

    /// Capture the evaluator's symbol policy before the buffer walk starts.
    ///
    /// `overlay_span` is the character span `(first, end)` the previous walk
    /// of this window consumed, or `None` for a whole-buffer overlay view.
    #[cfg(test)]
    pub fn from_buffer_for_window_with_symbols(
        buffer: &Buffer,
        vars: VariableView<'_>,
        visible: Option<(usize, usize)>,
        overlay_span: Option<(usize, usize)>,
        target: crate::display_property::DisplayPropertyTarget,
        symbol_mode: SymbolMode,
        root_retainer: &OwnedRootRetainer,
    ) -> Self {
        Self::capture_window_with_symbols(
            buffer,
            vars,
            visible,
            overlay_span,
            target,
            symbol_mode,
            Some(root_retainer),
            false,
        )
    }

    /// Capture for a window, optionally forcing the captured overlay fallback
    /// after a repeated live-view staleness retry.
    pub fn from_buffer_for_window_with_symbols_and_fallback(
        buffer: &Buffer,
        vars: VariableView<'_>,
        visible: Option<(usize, usize)>,
        overlay_span: Option<(usize, usize)>,
        target: crate::display_property::DisplayPropertyTarget,
        symbol_mode: SymbolMode,
        root_retainer: &OwnedRootRetainer,
        force_captured_overlays: bool,
    ) -> Self {
        Self::capture_window_with_symbols(
            buffer,
            vars,
            visible,
            overlay_span,
            target,
            symbol_mode,
            Some(root_retainer),
            force_captured_overlays,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn capture_window_with_symbols(
        buffer: &Buffer,
        vars: VariableView<'_>,
        visible: Option<(usize, usize)>,
        overlay_span: Option<(usize, usize)>,
        target: crate::display_property::DisplayPropertyTarget,
        symbol_mode: SymbolMode,
        root_retainer: Option<&OwnedRootRetainer>,
        force_captured_overlays: bool,
    ) -> Self {
        let obarray = vars.obarray();
        let mut snapshot = Self::capture_buffer(
            buffer,
            Some(vars),
            visible,
            overlay_span,
            root_retainer,
            force_captured_overlays,
        );
        snapshot.display_target = target;
        snapshot.category_symbol_plists = match &snapshot.overlays {
            SnapshotOverlays::Captured(overlays) => {
                capture_layout_category_symbol_plists_from_overlays(
                    buffer,
                    &**overlays,
                    obarray,
                    snapshot.overlay_view_span,
                )
            }
            SnapshotOverlays::Live(live) => capture_layout_category_symbol_plists_from_overlays(
                buffer,
                live,
                obarray,
                snapshot.overlay_view_span,
            ),
        };
        snapshot.automatic_composition_spans =
            capture_automatic_composition_spans(buffer, vars, &snapshot.vars, visible, symbol_mode);
        snapshot.string_composition_rules =
            capture_string_composition_rules(buffer, vars, symbol_mode);
        // Capture helpers above do not call Lisp or collect. Register the
        // captured versions now, before fontification can replace the live
        // sources. A cloned snapshot shares these roots until its final drop.
        snapshot._captured_roots = root_retainer.map(|retainer| {
            retainer
                .retain(
                    snapshot
                        .vars
                        .iter()
                        .flatten()
                        .copied()
                        .chain(snapshot.category_symbol_plists.values().copied())
                        .chain(
                            snapshot
                                .string_composition_rules
                                .iter()
                                .flat_map(|rules| rules.gc_roots()),
                        ),
                )
                .expect("layout capture requires its live evaluator root registry")
        });
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
        // Initial false answers retain FORM/object identity just like true
        // answers. Register before a later fontifier can detach their sources.
        self._display_when_roots = self.root_retainer.as_ref().map(|retainer| {
            retainer
                .retain(display_when.gc_roots())
                .expect("layout answers require their live evaluator root registry")
        });
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
        buffer,
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
        buffer,
        vars,
        symbol_mode,
    )?;
    neovm_core::emacs_core::composite::AutomaticCompositionRules::new_with_symbols(
        buffer,
        table,
        symbol_mode,
    )
}

pub(super) fn active_composition_table(
    mode: Option<Value>,
    function: Option<Value>,
    buffer: &Buffer,
    vars: VariableView<'_>,
    symbol_mode: SymbolMode,
) -> Option<Value> {
    if mode.is_none_or(Value::is_nil)
        || !function.is_some_and(|value| value.is_symbol_named("auto-compose-chars", symbol_mode))
    {
        return None;
    }
    // GNU composite.c reads Vcomposition_function_table (1105-1189) while
    // computing compositions for the buffer being redisplayed, so the
    // buffer-local binding (set by ligature/auto-composition setups) applies
    // to that buffer only; the old `vars.value` read answered the default
    // cell for every buffer.
    vars.value_in_buffer(
        Some(buffer),
        crate::fixed_symbols::COMPOSITION_FUNCTION_TABLE.id(),
    )
}

fn capture_layout_category_symbol_plists(
    buffer: &Buffer,
    obarray: &Obarray,
) -> FxHashMap<SymId, Value> {
    capture_layout_category_symbol_plists_from_overlays(buffer, buffer.overlays(), obarray, None)
}

/// Capture the category plists a layout walk can read. `overlays` is the
/// walk's own view, so a window snapshot only visits the overlays it copied
/// rather than every overlay in the buffer.
fn capture_layout_category_symbol_plists_from_overlays<O: OverlayRead + ?Sized>(
    buffer: &Buffer,
    overlays: &O,
    obarray: &Obarray,
    span: Option<(CharPos0, CharPos0)>,
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
    // Only the span the walk can consume can contribute a category to the
    // layout. Bounding the text scan turns a whole-buffer walk into a
    // window-sized one; an unbounded view keeps the old behavior.
    let end = span
        .map(|(_, end)| end.min(buffer.total_char_end_pos()))
        .unwrap_or_else(|| buffer.total_char_end_pos());
    let mut pos = span
        .map(|(start, _)| start.min(end))
        .unwrap_or(CharPos0::ZERO);
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

    if !overlays.may_contain_property(category_property) {
        return plists;
    }
    for overlay in overlays.overlays_in_gnu_lists_order() {
        if let Some(category) = overlays.overlay_get_named(overlay, category_property) {
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
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
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
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
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
    fn window_snapshot_copies_only_the_visible_overlay_range() {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        eval.eval_str(
            "(progn (insert (make-string 2048 ?x))
                    (overlay-put (make-overlay 6 7) 'face 'bold)
                    (overlay-put (make-overlay 2000 2001) 'face 'italic))",
        )
        .unwrap();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let buffer = eval.buffer_manager().get(id).unwrap();
        let snapshot = LayoutBufferSnapshot::from_buffer_for_window(
            buffer,
            eval.variables(),
            Some((0, 64)),
            crate::display_property::DisplayPropertyTarget::Graphical,
        );
        let view = snapshot.layout_overlays();
        assert_eq!(
            view.overlays_at_emacs_byte_pos(EmacsBytePos::new(5)).len(),
            1,
            "the in-window overlay is missing from the view"
        );
        assert!(
            view.overlays_at_emacs_byte_pos(EmacsBytePos::new(1999))
                .is_empty(),
            "an overlay outside the window was copied"
        );
    }

    #[test]
    fn window_snapshot_reports_an_overlay_query_below_the_span_start() {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        eval.eval_str(
            "(progn (insert (make-string 2048 ?x))
                    (overlay-put (make-overlay 6 7) 'face 'bold)
                    (overlay-put (make-overlay 200 201) 'face 'italic))",
        )
        .unwrap();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let buffer = eval.buffer_manager().get(id).unwrap();
        let snapshot = LayoutBufferSnapshot::from_buffer_for_window_with_symbols(
            buffer,
            eval.variables(),
            Some((0, 64)),
            Some((100, 200)),
            crate::display_property::DisplayPropertyTarget::Graphical,
            SymbolMode::Bare,
            &eval.owned_root_retainer(),
        );
        let _ = snapshot
            .layout_overlays()
            .overlays_at_emacs_byte_pos(EmacsBytePos::new(150));
        assert!(
            !snapshot.layout_overlay_view_start_underflow(),
            "an in-span query is complete"
        );
        let _ = snapshot
            .layout_overlays()
            .overlays_at_emacs_byte_pos(EmacsBytePos::new(50));
        assert!(
            snapshot.layout_overlay_view_start_underflow(),
            "a layout query below the span start must request the whole-buffer retry"
        );
    }

    #[test]
    fn category_capture_keeps_text_and_overlay_symbol_properties() {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
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
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
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

    #[test]
    fn window_span_skips_text_categories_outside_the_consumed_span() {
        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        eval.eval_str(
            "(progn (insert (make-string 4000 ?x))
                    (put-text-property 10 11 'category 'near-category)
                    (put-text-property 3500 3501 'category 'far-category))",
        )
        .unwrap();
        let id = eval.buffer_manager().current_buffer().unwrap().id();
        let buffer = eval.buffer_manager().get(id).unwrap();
        let full = capture_layout_category_symbol_plists(buffer, eval.obarray());
        assert_eq!(
            full.len(),
            2,
            "the whole-buffer capture sees both categories"
        );
        let view = buffer.overlay_snapshot_in_char_span(0, 100);
        let bounded = capture_layout_category_symbol_plists_from_overlays(
            buffer,
            &*view,
            eval.obarray(),
            Some((CharPos0::ZERO, CharPos0::new(100))),
        );
        assert_eq!(bounded.len(), 1, "only the in-span category is captured");
        assert!(bounded.contains_key(&Value::symbol("near-category").as_bare_symbol_id().unwrap()));
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
            if !info.captures_default {
                return None;
            }
            // The fallback is GNU's visible read with this buffer current,
            // not the default-only cell: a dynamic binding of a display
            // variable must reach the captured value (LD-1).
            variables?.value_in_buffer(Some(buffer), var.sym_id())
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

    fn layout_long_line_optimizations_p(&self) -> bool {
        self.long_line_optimizations
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

    fn layout_nth_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        limit: EmacsBytePos,
        n: usize,
    ) -> (EmacsBytePos, usize) {
        self.text_snapshot.nth_newline_emacs_byte(from, limit, n)
    }

    fn layout_prev_newline_emacs_byte(
        &self,
        from: EmacsBytePos,
        floor: EmacsBytePos,
    ) -> Option<EmacsBytePos> {
        self.text_snapshot.prev_newline_emacs_byte(from, floor)
    }

    fn layout_text_props_is_empty(&self) -> bool {
        self.text_snapshot.text_props_is_empty()
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

    fn layout_overlays(&self) -> super::LayoutOverlays<'_> {
        match &self.overlays {
            SnapshotOverlays::Captured(overlays) => super::LayoutOverlays::Snapshot(overlays),
            SnapshotOverlays::Live(live) => super::LayoutOverlays::Live(live),
        }
    }

    fn layout_overlay_view_end_char(&self) -> Option<CharPos0> {
        self.overlay_view_span.map(|(_, end)| end)
    }

    fn layout_overlay_view_start_underflow(&self) -> bool {
        match &self.overlays {
            SnapshotOverlays::Captured(overlays) => overlays.overlay_view_query_underflowed(),
            SnapshotOverlays::Live(_) => false,
        }
    }

    fn layout_overlay_view_stale(&self) -> bool {
        match &self.overlays {
            SnapshotOverlays::Captured(_) => false,
            SnapshotOverlays::Live(live) => live.is_stale(),
        }
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

    fn layout_automatic_composition_advance_px(&self, pos: CharPos0) -> Option<f32> {
        self.automatic_composition_advances.get(&pos.get()).copied()
    }
}

/// GNU `composition_gstring_width` over a whole gstring: the sum of the
/// glyph WIDTH slots, using WADJUST when the glyph carries an adjustment
/// vector (`compose-gstring-for-graphic`'s lone-mark branch stores one).
///
/// `None` for a terminal gstring (no font object): its slots are character
/// cells, and the terminal walk keeps its own `terminal.width_cols`.
fn gstring_pixel_advance(gstring: Value) -> Option<f32> {
    let slots = gstring.as_vector_data()?;
    let header = slots.first()?.as_vector_data()?;
    if !header.first().is_some_and(|font| font.is_font_object()) {
        return None;
    }
    let mut total: i64 = 0;
    let mut glyphs = 0usize;
    for slot in slots.iter().skip(2) {
        let Some(glyph) = slot.as_vector_data() else {
            break;
        };
        let adjustment = glyph
            .get(9)
            .and_then(|value| value.as_vector_data())
            .and_then(|adjustment| adjustment.get(2))
            .and_then(|value| value.as_int());
        total += adjustment
            .or_else(|| glyph.get(4).and_then(|value| value.as_int()))
            .unwrap_or(0);
        glyphs += 1;
    }
    (glyphs > 0).then(|| total.max(0) as f32)
}

impl LayoutBufferSnapshot {
    /// Start positions of every automatic-composition span this snapshot
    /// captured, ascending. The leaf's post-walk coverage check uses them to
    /// prove each painted composition had a resolved gstring width (and to
    /// widen the resolution range otherwise), instead of letting the row
    /// measure the raw text as a silent fallback.
    pub(crate) fn automatic_composition_span_starts(&self) -> impl Iterator<Item = usize> + '_ {
        self.automatic_composition_spans
            .iter()
            .map(|range| range.start().get())
    }

    /// The captured automatic-composition spans as `(start, end)` char ranges,
    /// ascending.  The leaf retains the resolved widths for exactly these
    /// spans, so a later layout with unchanged composition inputs can seed
    /// them without re-entering `font-at`/`find-composition-internal`.
    pub(crate) fn automatic_composition_span_ranges(
        &self,
    ) -> impl Iterator<Item = (usize, usize)> + '_ {
        self.automatic_composition_spans
            .iter()
            .map(|range| (range.start().get(), range.end().get()))
    }

    /// The advances resolved for this snapshot, keyed by span start.
    pub(crate) fn resolved_automatic_composition_advances(&self) -> &FxHashMap<usize, f32> {
        &self.automatic_composition_advances
    }

    /// Resolve every automatic composition in `resolve` to its gstring width
    /// on the evaluator thread (GNU `autocmp_chars` +
    /// `composition_gstring_width`).
    ///
    /// The snapshot owns the rule table and spans but cannot evaluate Lisp,
    /// and a prepared row job must not cross threads with live evaluator
    /// state, so the widths are resolved once, before any row walk.  A rule
    /// may shape the gstring or (for `compose-gstring-for-graphic`'s
    /// `nchars = 1` branch) only store a spacing adjustment; both contribute
    /// `LGLYPH_WADJUST` when present, exactly as `composition_gstring_width`.
    /// The search runs in the window's buffer so `font-at`, the buffer-local
    /// `auto-composition-function`, and the rule's spacing facts see the
    /// same context as a redisplay walk.
    ///
    /// `resolve` starts as the visible rows plus a line of margin; `usize::MAX`
    /// (the widened retry) resolves everything the snapshot captured.  Each
    /// resolved rule gstring is put in the evaluator's GNU-shaped-gstring
    /// cache (`composition_gstring_put_cache`), so later layouts reuse it
    /// without running the rule.
    /// `seeded` supplies widths retained by the previous layout for spans
    /// whose composition inputs are unchanged; those spans skip the Lisp
    /// search entirely.
    pub(crate) fn resolve_automatic_composition_advances(
        &mut self,
        evaluator: &mut neovm_core::emacs_core::eval::Context,
        buffer_id: neovm_core::buffer::BufferId,
        resolve: std::ops::Range<usize>,
        seeded: &FxHashMap<(usize, usize), f32>,
    ) {
        if self.automatic_composition_spans.is_empty() {
            return;
        }
        tracing::debug!(
            target: "neomacs_layout_engine::composition_resolver",
            spans = self.automatic_composition_spans.len(),
            resolve_start = resolve.start,
            resolve_end = resolve.end,
            "resolve automatic composition advances"
        );
        use neovm_core::emacs_core::edit_host::EditHost;
        let spans = self.automatic_composition_spans.clone();
        let mut advances = FxHashMap::default();
        let _ = evaluator.with_current_buffer_scope(buffer_id, |eval| {
            for span in &spans {
                let start = span.start().get();
                // Resolve only what this window's rows can reach.  The
                // snapshot capture bound is several screenfuls; eagerly
                // shaping every composition there costs Lisp per span per
                // layout attempt and starved input on rich buffers.
                if !resolve.contains(&start) {
                    continue;
                }
                if let Some(width) = seeded.get(&(start, span.end().get())) {
                    advances.insert(start, *width);
                    continue;
                }
                let pos = Value::fixnum(start as i64 + 1);
                let Ok(result) = neovm_core::emacs_core::composite::find_composition_internal(
                    eval,
                    vec![pos, Value::NIL, Value::NIL, Value::NIL],
                ) else {
                    continue;
                };
                if !result.is_cons() {
                    continue;
                }
                tracing::debug!(
                    target: "neomacs_layout_engine::composition_resolver",
                    start,
                    "automatic composition span found"
                );
                let rest = result.cons_cdr();
                if !rest.is_cons() || result.cons_car().as_int() != Some(start as i64 + 1) {
                    continue;
                }
                let gstring = rest.cons_cdr().cons_car();
                // GNU's display path caches the rule's gstring after
                // `autocmp_chars` (`composition_reseat_it`,
                // src/composite.c:1410-1412), even when the rule only stored
                // an adjustment (`compose-gstring-for-graphic`'s lone-mark
                // branch). The next layout's search then hits
                // `composition_get_gstring`'s cache and skips the rule.
                let gstring = eval.composition_gstring_put_cache(gstring);
                if let Some(width) = gstring_pixel_advance(gstring) {
                    advances.insert(start, width);
                }
            }
            Ok(Value::NIL)
        });
        self.automatic_composition_advances = advances;
    }
}
