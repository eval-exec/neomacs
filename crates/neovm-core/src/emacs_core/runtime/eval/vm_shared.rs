//! Evaluation entry points that once lived on VmSharedState: lexical-arg evaluation, eval-stack depth accounting, and the bytecode call seam.
//!
//! Moved out of `eval/mod.rs` unchanged; a child module of `eval` so it keeps
//! the same view of `Context` and the parent's private items (`use super::*`).

use super::*;

impl Context {
    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn begin_eval_with_lexical_arg(
        &mut self,
        lexical_arg: Option<Value>,
    ) -> Result<ActiveEvalLexicalArgState, Flow> {
        begin_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            lexical_arg,
        )
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn finish_eval_with_lexical_arg(&mut self, state: ActiveEvalLexicalArgState) {
        finish_eval_with_lexical_arg_in_state(
            &mut self.obarray,
            &mut self.lexenv,
            &mut self.specpdl,
            state,
        );
    }

    pub(crate) fn begin_macro_expansion_scope(
        &mut self,
    ) -> Result<ActiveMacroExpansionScopeState, Flow> {
        self.macro_expansion_scope_depth += 1;
        match self.begin_macro_expansion_scope_frame() {
            Ok(state) => Ok(state),
            Err(flow) => {
                self.macro_expansion_scope_depth =
                    self.macro_expansion_scope_depth.saturating_sub(1);
                Err(flow)
            }
        }
    }

    pub(crate) fn finish_macro_expansion_scope(
        &mut self,
        state: ActiveMacroExpansionScopeState,
        result: EvalResult,
    ) -> EvalResult {
        let result = self.finish_macro_expansion_scope_frame(state, result);
        self.macro_expansion_scope_depth = self.macro_expansion_scope_depth.saturating_sub(1);
        result
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn kmacro_mut(&mut self) -> &mut KmacroManager {
        &mut self.kmacro
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn gui_frame_creation_state(
        &mut self,
    ) -> (
        &mut FrameManager,
        &mut BufferManager,
        &mut Option<Box<dyn DisplayHost>>,
    ) {
        (&mut self.frames, &mut self.buffers, &mut self.display_host)
    }

    pub(crate) fn recursive_command_loop_depth(&self) -> usize {
        // GNU's `command_loop_level` starts at -1 before entering the
        // top-level recursive edit, so ordinary interactive execution happens
        // at level 0. Neomacs stores the raw active-loop count instead
        // (0 outside the loop, 1 at top level), so translate here to the
        // GNU-visible level used by mode-line and minibuffer semantics.
        self.command_loop.recursive_depth.saturating_sub(1)
    }

    /// Lisp-visible recursive-edit depth, matching GNU's
    /// `command_loop_level + minibuf_level`.
    pub(crate) fn recursion_depth(&self) -> usize {
        self.recursive_command_loop_depth()
            .saturating_add(self.minibuffers.depth())
    }

    pub(crate) fn interactive_minibuffer_read_count(&self) -> u64 {
        self.interactive_minibuffer_read_count
    }

    // --- Post-command point adjustment (GNU `keyboard.c`) -------------------

    /// Current buffer point as a 1-based Lisp char position.
    pub(super) fn apfp_point(&self, id: crate::buffer::BufferId) -> i64 {
        self.buffers
            .get(id)
            .map(|b| b.point_char_pos().to_lisp().as_i64())
            .unwrap_or(1)
    }

    /// Raw `SET_PT` equivalent: move point without running point-motion or
    /// intangibility hooks (GNU's `adjust_point_for_property` uses `SET_PT`).
    pub(super) fn apfp_set_point(&mut self, id: crate::buffer::BufferId, lisp_pos: i64) {
        let byte = match self.buffers.get(id) {
            Some(b) => b.lisp_pos_to_accessible_emacs_byte_pos(
                crate::buffer::position::LispCharPos1::new(lisp_pos.max(1)),
            ),
            None => return,
        };
        let _ = self.buffers.goto_buffer_emacs_byte_pos(id, byte);
    }

    pub(super) fn apfp_char_property(&mut self, pos: i64, prop: Value) -> Result<Value, Flow> {
        super::super::textprop::builtin_get_char_property(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL],
        )
    }

    pub(super) fn apfp_pos_property(&mut self, pos: i64, prop: Value) -> Result<Value, Flow> {
        super::super::builtins::misc_eval::builtin_get_pos_property(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL],
        )
    }

    pub(super) fn apfp_next_change(&mut self, pos: i64, prop: Value, zv: i64) -> Result<i64, Flow> {
        let v = super::super::builtins::misc_eval::builtin_next_single_char_property_change(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL, Value::NIL],
        )?;
        Ok(v.as_fixnum().unwrap_or(zv))
    }

    pub(super) fn apfp_prev_change(
        &mut self,
        pos: i64,
        prop: Value,
        begv: i64,
    ) -> Result<i64, Flow> {
        let v = super::super::builtins::misc_eval::builtin_previous_single_char_property_change(
            self,
            vec![Value::fixnum(pos), prop, Value::NIL, Value::NIL],
        )?;
        Ok(v.as_fixnum().unwrap_or(begv))
    }

    /// Port of GNU `keyboard.c:adjust_point_for_property`, invisible-text
    /// branch.  After a command moves point, GNU never leaves point resting
    /// inside an `invisible` region — it relocates point to a region boundary
    /// so the cursor is visible.  Without this, motion commands (e.g. evil
    /// `e`) that land inside org's invisible link-target text leave the cursor
    /// parked where the display collapses the hidden run to a single column,
    /// so it appears frozen.
    ///
    /// GNU also adjusts for `display`-intangible and composition here; those
    /// branches are not yet ported (they only add further adjustments that
    /// neomacs does not otherwise perform).  The invisible branch is iterated
    /// to a fixpoint, mirroring GNU's `check_*` re-entry loop.
    pub(crate) fn adjust_point_for_property(
        &mut self,
        last_pt: i64,
        modified: bool,
    ) -> Result<(), Flow> {
        let Some(id) = self.buffers.current_buffer_id() else {
            return Ok(());
        };
        let inv = Value::symbol("invisible");
        let spec = self
            .eval_symbol_by_id(intern("buffer-invisibility-spec"))
            .unwrap_or(Value::NIL);
        let display_sym = Value::symbol("display");
        // GNU's `FRAME_WINDOW_P (selected_frame)`: image/xwidget `display`
        // specs replace text (and so make it intangible) only on a GUI frame.
        let frame_window_p = self
            .frames
            .selected_frame()
            .map(|frame| frame.effective_window_system().is_some())
            .unwrap_or(false);

        // `orig_pt` mirrors GNU: point on entry, used to detect "we have not
        // moved yet" so the boundary-choice heuristic stays free.
        let mut orig_pt: i64 = self.apfp_point(id);

        for _ in 0..50 {
            let pt = self.apfp_point(id);
            let (begv, zv) = match self.buffers.get(id) {
                Some(b) => (
                    b.point_min_lisp_char_pos().as_i64(),
                    b.point_max_lisp_char_pos().as_i64(),
                ),
                None => return Ok(()),
            };
            if !(pt > begv && pt < zv) {
                break;
            }

            // GNU `adjust_point_for_property` display-intangible branch: never
            // leave point inside text that a `display` property replaces. Moving
            // forward relocates to the run end; moving backward to its start (an
            // empty replacing string relocates one char before the start). A
            // relocation re-enters the loop so the invisible branch re-checks
            // the new position, mirroring GNU's `check_display`/`check_invisible`
            // cycling.
            let disp = self.apfp_char_property(pt, display_sym)?;
            if !disp.is_nil() && super::super::xdisp::display_prop_replacing_p(disp, frame_window_p)
            {
                // Maximal run [dbeg, dend) around PT whose `display` value is
                // `eq` to the one at PT (GNU `get_property_and_range`). Stepped
                // boundary-by-boundary (checking each edge before advancing, as
                // the invisible branch does) so a change-scan is never issued
                // from a run edge, where it would jump past the adjacent run.
                let mut dend = pt;
                while dend < zv
                    && crate::emacs_core::value::eq_value(
                        &self.apfp_char_property(dend, display_sym)?,
                        &disp,
                    )
                {
                    dend = self.apfp_next_change(dend, display_sym, zv)?;
                }
                let mut dbeg = pt;
                while dbeg > begv
                    && crate::emacs_core::value::eq_value(
                        &self.apfp_char_property(dbeg - 1, display_sym)?,
                        &disp,
                    )
                {
                    dbeg = self.apfp_prev_change(dbeg, display_sym, begv)?;
                }
                let empty_string = disp
                    .as_lisp_string()
                    .map(|s| s.as_bytes().is_empty())
                    .unwrap_or(false);
                if dbeg < pt || (dbeg <= pt && empty_string) {
                    let target = if pt < last_pt {
                        if empty_string {
                            (dbeg - 1).max(begv)
                        } else {
                            dbeg
                        }
                    } else {
                        dend
                    };
                    self.apfp_set_point(id, target);
                    continue;
                }
            }

            let pt_before_invis = pt;
            let mut ellipsis = false;
            let mut beg = pt;
            let mut end = pt;

            // Find boundaries `beg`..`end` of the invisible run around PT.
            while end < zv {
                let prop = self.apfp_char_property(end, inv)?;
                let invisibility = super::super::xdisp::text_prop_means_invisible(prop, spec);
                if !invisibility.hides_source() {
                    break;
                }
                ellipsis = ellipsis || invisibility.shows_ellipsis();
                end = self.apfp_next_change(end, inv, zv)?;
            }
            while beg > begv {
                let prop = self.apfp_char_property(beg - 1, inv)?;
                let invisibility = super::super::xdisp::text_prop_means_invisible(prop, spec);
                if !invisibility.hides_source() {
                    break;
                }
                ellipsis = ellipsis || invisibility.shows_ellipsis();
                beg = self.apfp_prev_change(beg, inv, begv)?;
            }

            let mut moved = false;

            // Move away from the inside of the region.
            if beg < pt && end > pt {
                let target = if orig_pt == pt && (last_pt < beg || last_pt > end) {
                    orig_pt = -1;
                    if pt < last_pt { end } else { beg }
                } else if pt < last_pt {
                    beg
                } else {
                    end
                };
                self.apfp_set_point(id, target);
                moved = true;
            }

            // GNU keyboard.c: skip the boundary nudge when the invisible run's
            // start carries a replacing `display` property — the display engine
            // then positions the cursor, so point need not move (`shown`).
            let shown = {
                let dprop = self.apfp_char_property(beg, display_sym)?;
                !dprop.is_nil()
                    && super::super::xdisp::display_prop_replacing_p(dprop, frame_window_p)
            };

            if !modified && !shown && !ellipsis && beg < end {
                let pt2 = self.apfp_point(id);
                if last_pt == beg && pt2 == end && end < zv {
                    self.apfp_set_point(id, end + 1);
                    moved = true;
                } else if last_pt == end && pt2 == beg && beg > begv {
                    self.apfp_set_point(id, beg - 1);
                    moved = true;
                } else if pt2 == (if pt2 < last_pt { beg } else { end }) {
                    // Already as far as we can go; avoid an infinite loop.
                } else {
                    let here = self.apfp_pos_property(pt2, inv)?;
                    if super::super::xdisp::text_prop_means_invisible(here, spec).hides_source() {
                        let other = if pt2 == beg { end } else { beg };
                        let other_val = self.apfp_pos_property(other, inv)?;
                        if !super::super::xdisp::text_prop_means_invisible(other_val, spec)
                            .hides_source()
                        {
                            self.apfp_set_point(id, other);
                            moved = true;
                        }
                    }
                }
            }

            let _ = pt_before_invis;
            if !moved {
                break;
            }
        }
        Ok(())
    }

    pub(crate) fn note_interactive_minibuffer_read(&mut self) {
        self.interactive_minibuffer_read_count =
            self.interactive_minibuffer_read_count.saturating_add(1);
    }

    pub(super) fn sync_current_buffer_to_selected_window(&mut self) {
        let Some(frame_id) = self.frames.selected_frame().map(|frame| frame.id) else {
            return;
        };
        super::super::window_cmds::remember_selected_window_point_in_state(
            &mut self.frames,
            &mut self.buffers,
            frame_id,
        );
        super::super::window_cmds::sync_selected_window_buffer_in_state(
            &self.frames,
            &mut self.buffers,
            frame_id,
        );
        let _ = self.sync_current_buffer_runtime_state();
    }
}
