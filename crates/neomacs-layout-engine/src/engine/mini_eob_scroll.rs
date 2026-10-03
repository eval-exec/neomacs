//! GNU EOB mini scroll continuation owned by one logical frame walk.
//!
//! The actual producer proved that point has no admitted cursor row. GNU
//! enters try_scrolling, then recenter if the fresh producer still lacks one.
//! Decision identity is window-local program-counter ownership: changing the
//! displayed buffer inside a hook does not re-enter try_scrolling. Each
//! exclusive LayoutEngine/Context mutator owns a disjoint numeric ledger;
//! this stores no Lisp owners or shared mutable caches.

use crate::buffer_source::tail_render::GnuMiniEobSourceBoundary;
use crate::buffer_source::window_source::ResolvedWindowStart;
use crate::layout_effect::{MiniEobScrollDecision, WindowScrollHookSite};
use crate::scroll_policy::ScrollPolicy;
use crate::types::WindowParams;
use neovm_core::buffer::{BufferId, CharPos0};
use neovm_core::emacs_core::Context;
use neovm_core::window::{FrameId, WindowId};

/// Acknowledged numeric program counter owned by the exclusive ledger.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Continuation {
    ConservativeAcknowledged,
    RecenterAcknowledged,
}

/// Per-logical-attempt ownership; independent Context mutators never share
/// this map, which stores only numeric window identities and closed phases.
#[derive(Default)]
pub(super) struct MiniEobScrollLedger {
    continuations: rustc_hash::FxHashMap<WindowId, Continuation>,
}

impl MiniEobScrollLedger {
    #[inline]
    pub(super) fn acknowledge(&mut self, site: WindowScrollHookSite) {
        if let Some((window, _buffer, decision)) = site.mini_eob_decision() {
            let continuation = match decision {
                MiniEobScrollDecision::Conservative => Continuation::ConservativeAcknowledged,
                MiniEobScrollDecision::Recenter => Continuation::RecenterAcknowledged,
            };
            self.continuations.insert(window, continuation);
        }
    }

    #[inline]
    pub(super) fn completed(&self, window: WindowId, _buffer: BufferId) -> bool {
        matches!(
            self.continuations.get(&window),
            Some(Continuation::RecenterAcknowledged)
        )
    }

    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    pub(super) fn next_site(
        &self,
        evaluator: &mut Context,
        frame: FrameId,
        window: WindowId,
        buffer: BufferId,
        start: ResolvedWindowStart,
        params: &WindowParams,
        text_height: f32,
        topology: u64,
        boundary: GnuMiniEobSourceBoundary,
    ) -> Result<WindowScrollHookSite, ()> {
        let policy = ScrollPolicy::from_window_params(params);
        let decision = match self.continuations.get(&window) {
            None if policy != ScrollPolicy::Recenter => MiniEobScrollDecision::Conservative,
            None | Some(Continuation::ConservativeAcknowledged) => MiniEobScrollDecision::Recenter,
            Some(Continuation::RecenterAcknowledged) => {
                unreachable!("completed GNU fallback cannot request another scroll site")
            }
        };
        let candidate = match decision {
            MiniEobScrollDecision::Conservative => start,
            MiniEobScrollDecision::Recenter => {
                let projection = evaluator
                    .window_layout_attempt_freshness(frame, window, buffer)
                    .ok_or(())?;
                let rows_above = match boundary {
                    // At ZV inside a source line, backward iterator validation
                    // crosses the exhausted overlay rows. GNU's current_y<=0
                    // repair then reseats at PT's source screen line (xdisp.c
                    // 11286-11345, 21225-21237), not inside a virtual string.
                    GnuMiniEobSourceBoundary::BufferScreenLine => 0,
                    // A hard newline reached ZV on a fresh buffer row. GNU
                    // move_it_to stops before EOB strings, so backward motion
                    // retains the ordinary mini half-height centering distance.
                    GnuMiniEobSourceBoundary::FreshLineAfterBufferNewline => {
                        let rows =
                            (text_height / params.char_height.max(1.0)).floor().max(0.0) as i64;
                        rows / 2
                    }
                };
                let point = CharPos0::new(params.point.max(0) as usize);
                let candidate = evaluator
                    .redisplay_start_before_point_by_display_rows(buffer, window, point, rows_above)
                    .map_or(start, |position| {
                        ResolvedWindowStart::from_layout_charpos(position.get() as i64)
                    });
                if evaluator.frame_manager().window_topology_generation() != topology
                    || evaluator
                        .window_layout_attempt_freshness(frame, window, buffer)
                        .as_ref()
                        != Some(&projection)
                {
                    return Err(());
                }
                candidate
            }
        };
        Ok(WindowScrollHookSite::mini_eob(
            window, candidate, buffer, decision,
        ))
    }
}
