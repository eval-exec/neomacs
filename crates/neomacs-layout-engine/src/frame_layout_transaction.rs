//! Bounded convergence for one speculative frame-layout transaction.

use crate::window_layout::WindowChromeMetrics;
use neomacs_display_protocol::types::DisplayWindowId;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum FrameRelayoutRequest {
    FrameTabBar {
        assumed_height: f32,
        measured_height: f32,
    },
    WindowChrome {
        window_id: DisplayWindowId,
        assumed: WindowChromeMetrics,
        measured: WindowChromeMetrics,
    },
    Minibuffer {
        window_id: DisplayWindowId,
        allocated_height_px: f32,
        required_height_px: f32,
    },
    /// Lisp entered during leaf-local fontification or non-chrome setup changed
    /// canonical layout inputs. Discard the speculative frame and recollect.
    LogicalInputsChanged {
        window_id: DisplayWindowId,
    },
    /// Window chrome evaluation (`mode-line-format`, `header-line-format`, or
    /// `tab-line-format`) invalidated the leaf's source or physical freshness.
    WindowChromeInputsChanged {
        window_id: DisplayWindowId,
    },
    /// Post-walk `display (when ...)` Lisp mutated canonical layout inputs
    /// after chrome evaluation already ran for this attempt.
    PostWalkDisplayWhenInputsChanged {
        window_id: DisplayWindowId,
    },
    /// A finished walk's own fontification changed source inputs that a
    /// completed body may keep. The frame stays whole. Retry while the visible
    /// pass makes progress on new unfontified stops (identified by
    /// `stops_fingerprint`) and the budget remains; stop early if the exact
    /// same stop set repeats, publishing this attempt when refused.
    SourceOnlyRetry {
        window_id: DisplayWindowId,
        stops_fingerprint: u64,
    },
    /// A successful native font open changed the display-wide matrix capacity.
    FontCapacityInputsChanged {
        before: u32,
        after: u32,
    },
    /// A `when` display clause the window's walk took by the structural rule
    /// evaluated nil after the walk. Replay the frame once with the answer.
    DisplayWhenAnswerChanged {
        window_id: DisplayWindowId,
    },
    WindowTopologyChanged {
        before: u64,
        after: u64,
    },
    /// A Lisp face callback changed the face definitions or terminal palette
    /// represented by this speculative frame's immutable resolver.
    FaceRealizationInputsChanged {
        before: u64,
        after: u64,
    },
}

/// Grouping key for bounding repeated identical relayout causes within one
/// frame-layout transaction.
///
/// `FaceRealizationInputsChanged` and `WindowTopologyChanged` are intentionally
/// keyed per frame rather than per window, matching GNU `xdisp.c:17960`
/// (frame-wide `f->redisplay` self-limiting to one extra pass) and
/// `xdisp.c:17971` (frame-wide `FRAME_GARBAGED_P` bound).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RelayoutCauseKey {
    FrameTabBar,
    WindowChrome(DisplayWindowId),
    Minibuffer(DisplayWindowId),
    LogicalInputsChanged(DisplayWindowId),
    WindowChromeInputsChanged(DisplayWindowId),
    PostWalkDisplayWhenInputsChanged(DisplayWindowId),
    SourceOnlyRetry(DisplayWindowId, u64),
    FontCapacityInputsChanged,
    DisplayWhenAnswerChanged(DisplayWindowId),
    WindowTopologyChanged,
    FaceRealizationInputsChanged,
}

fn chrome_metrics_bits(metrics: WindowChromeMetrics) -> [u32; 3] {
    [
        metrics.tab_line_height.to_bits(),
        metrics.header_line_height.to_bits(),
        metrics.mode_line_height.to_bits(),
    ]
}

impl FrameRelayoutRequest {
    const fn cause_key(self) -> RelayoutCauseKey {
        match self {
            Self::FrameTabBar { .. } => RelayoutCauseKey::FrameTabBar,
            Self::WindowChrome { window_id, .. } => RelayoutCauseKey::WindowChrome(window_id),
            Self::Minibuffer { window_id, .. } => RelayoutCauseKey::Minibuffer(window_id),
            Self::LogicalInputsChanged { window_id } => {
                RelayoutCauseKey::LogicalInputsChanged(window_id)
            }
            Self::WindowChromeInputsChanged { window_id } => {
                RelayoutCauseKey::WindowChromeInputsChanged(window_id)
            }
            Self::PostWalkDisplayWhenInputsChanged { window_id } => {
                RelayoutCauseKey::PostWalkDisplayWhenInputsChanged(window_id)
            }
            Self::SourceOnlyRetry {
                window_id,
                stops_fingerprint,
            } => RelayoutCauseKey::SourceOnlyRetry(window_id, stops_fingerprint),
            Self::FontCapacityInputsChanged { .. } => RelayoutCauseKey::FontCapacityInputsChanged,
            Self::DisplayWhenAnswerChanged { window_id } => {
                RelayoutCauseKey::DisplayWhenAnswerChanged(window_id)
            }
            Self::WindowTopologyChanged { .. } => RelayoutCauseKey::WindowTopologyChanged,
            Self::FaceRealizationInputsChanged { .. } => {
                RelayoutCauseKey::FaceRealizationInputsChanged
            }
        }
    }

    /// Maximum retries permitted for this specific relayout cause before the
    /// transaction declares exhaustion.
    ///
    /// Keeps legitimate multi-cause/multi-window convergence, multi-wave
    /// progress-making visible fontification (`SourceOnlyRetry` keyed by
    /// `stops_fingerprint`), non-oscillating chrome height changes
    /// (`WindowChrome` cycle-checked separately), and non-chrome prepass/query
    /// retries inside the shared `max_retries` budget.
    ///
    /// Stops non-converging post-chrome callback loops after 1 converging retry
    /// (2 evaluations):
    /// - `FaceRealizationInputsChanged`: 1 retry matches GNU `xdisp.c:17960`
    ///   (`!f_redisplay_flag && f->redisplay`).
    /// - `WindowChromeInputsChanged` / `PostWalkDisplayWhenInputsChanged`: 1
    ///   retry is the minimum required by Neomacs's rows-from-final-geometry
    ///   invariant (`status_line_layout_mutation_rejects_rows_built_from_the_old_geometry`)
    ///   to distinguish 1-shot converging chrome mutations from non-converging
    ///   loops (GNU drops in-pass window changes at `xdisp.c:18142` with 0
    ///   retries).
    /// - `SourceOnlyRetry(window_id, stops_fingerprint)`: 1 retry per distinct
    ///   visible stop fingerprint so multi-round converging fontification runs
    ///   freely up to `shared_max`, while repeating identical stops stop after
    ///   1 retry (2 evaluations).
    const fn per_cause_retry_limit(self, shared_max: usize) -> usize {
        let limit = match self {
            Self::WindowChromeInputsChanged { .. }
            | Self::PostWalkDisplayWhenInputsChanged { .. }
            | Self::FaceRealizationInputsChanged { .. }
            | Self::SourceOnlyRetry { .. } => 1,
            Self::FrameTabBar { .. } | Self::WindowTopologyChanged { .. } => 2,
            Self::WindowChrome { .. }
            | Self::LogicalInputsChanged { .. }
            | Self::Minibuffer { .. }
            | Self::FontCapacityInputsChanged { .. }
            | Self::DisplayWhenAnswerChanged { .. } => shared_max,
        };
        if limit < shared_max {
            limit
        } else {
            shared_max
        }
    }
}

const MAX_TRACKED_CAUSES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LayoutConvergenceFailureReason {
    /// Total retries across all causes reached the shared `max_retries` cap.
    SharedBudgetExhausted,
    /// One specific relayout cause reached its per-cause retry bound.
    CauseRetryBudgetExhausted { cause_limit: usize },
    /// Window chrome height returned to an earlier assumed height for the same window.
    ChromeHeightCycle,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutConvergenceError {
    pub(crate) retry_count: usize,
    pub(crate) max_retries: usize,
    pub(crate) reason: LayoutConvergenceFailureReason,
    pub(crate) last_request: FrameRelayoutRequest,
}

pub(crate) struct FrameLayoutCoordinator {
    retry_count: usize,
    max_retries: usize,
    cause_counts: [Option<(RelayoutCauseKey, usize)>; MAX_TRACKED_CAUSES],
    cause_count_len: usize,
    /// Previously assumed `(window_id, chrome_metrics_bits)` pairs for
    /// oscillation detection on `FrameRelayoutRequest::WindowChrome`.
    seen_chrome_heights: [Option<(DisplayWindowId, [u32; 3])>; MAX_TRACKED_CAUSES],
    seen_chrome_height_len: usize,
}

impl FrameLayoutCoordinator {
    pub(crate) fn new(max_retries: usize) -> Self {
        Self {
            retry_count: 0,
            max_retries,
            cause_counts: [None; MAX_TRACKED_CAUSES],
            cause_count_len: 0,
            seen_chrome_heights: [None; MAX_TRACKED_CAUSES],
            seen_chrome_height_len: 0,
        }
    }

    pub(crate) const fn retry_count(&self) -> usize {
        self.retry_count
    }

    fn record_chrome_assumed_and_check_cycle(
        &mut self,
        window_id: DisplayWindowId,
        assumed: WindowChromeMetrics,
        measured: WindowChromeMetrics,
    ) -> bool {
        let assumed_bits = chrome_metrics_bits(assumed);
        let measured_bits = chrome_metrics_bits(measured);
        let assumed_seen = self.seen_chrome_heights[..self.seen_chrome_height_len]
            .iter()
            .any(|slot| *slot == Some((window_id, assumed_bits)));
        if !assumed_seen && self.seen_chrome_height_len < MAX_TRACKED_CAUSES {
            self.seen_chrome_heights[self.seen_chrome_height_len] = Some((window_id, assumed_bits));
            self.seen_chrome_height_len += 1;
        }
        // If `measured` returns to any previously assumed height for this
        // window, treat the chrome height as oscillating (e.g. toggling
        // 1.0 <-> 2.0). Known limitation (W3): a stateful chrome callback that
        // makes a single excursion A -> B -> A and then stabilizes at A is also
        // flagged here; tracking repeated `(assumed, measured)` transition pairs
        // would allow single excursions at the cost of one extra retry on real
        // cycles.
        self.seen_chrome_heights[..self.seen_chrome_height_len]
            .iter()
            .any(|slot| *slot == Some((window_id, measured_bits)))
    }

    pub(crate) fn request_retry(
        &mut self,
        request: FrameRelayoutRequest,
    ) -> Result<(), LayoutConvergenceError> {
        if self.retry_count >= self.max_retries {
            return Err(LayoutConvergenceError {
                retry_count: self.retry_count,
                max_retries: self.max_retries,
                reason: LayoutConvergenceFailureReason::SharedBudgetExhausted,
                last_request: request,
            });
        }
        if let FrameRelayoutRequest::WindowChrome {
            window_id,
            assumed,
            measured,
        } = request
            && self.record_chrome_assumed_and_check_cycle(window_id, assumed, measured)
        {
            return Err(LayoutConvergenceError {
                retry_count: self.retry_count,
                max_retries: self.max_retries,
                reason: LayoutConvergenceFailureReason::ChromeHeightCycle,
                last_request: request,
            });
        }
        let cause = request.cause_key();
        let cause_limit = request.per_cause_retry_limit(self.max_retries);
        // `self.retry_count >= self.max_retries` above guarantees `max_retries > 0`,
        // and every per-cause limit is at least 1.
        debug_assert!(cause_limit > 0);
        for slot in &mut self.cause_counts[..self.cause_count_len] {
            if let Some((seen_cause, count)) = slot
                && *seen_cause == cause
            {
                if *count >= cause_limit {
                    return Err(LayoutConvergenceError {
                        retry_count: self.retry_count,
                        max_retries: self.max_retries,
                        reason: LayoutConvergenceFailureReason::CauseRetryBudgetExhausted {
                            cause_limit,
                        },
                        last_request: request,
                    });
                }
                *count += 1;
                self.retry_count += 1;
                return Ok(());
            }
        }
        // If more than MAX_TRACKED_CAUSES distinct causes occur in one frame
        // transaction, additional causes fall back harmlessly to `max_retries`.
        if self.cause_count_len < MAX_TRACKED_CAUSES {
            self.cause_counts[self.cause_count_len] = Some((cause, 1));
            self.cause_count_len += 1;
        }
        self.retry_count += 1;
        Ok(())
    }
}

#[cfg(test)]
#[path = "frame_layout_transaction/tests/frame_layout_transaction_test.rs"]
mod tests;
