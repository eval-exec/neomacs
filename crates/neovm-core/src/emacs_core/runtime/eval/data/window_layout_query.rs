//! Synchronous frontend layout-query boundary types (stage-4 K2 S1).
//!
//! These are pure records over eval-data window snapshots, lower-crate ids
//! and the evaluator Context; keeping them here lets `Context` and the
//! display walk name the same boundary without reaching up into
//! `crate::window`.

use super::window_state::WindowDisplaySnapshot;
use neovm_edit::buffer::{BufferId, LispCharPos1};
use neovm_edit::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;

/// Everything one synchronous single-window layout query produced.
///
/// GNU needs no such type, because GNU serves no display query from a retained
/// matrix. `pos_visible_p` (src/xdisp.c) and `buffer_posn_from_coords`
/// (src/dispnew.c) each run `start_display` from `w->start` and `move_it_to`
/// on every call; `buffer_posn_from_coords` reads `w->current_matrix` exactly
/// once, to fill in the WIDTH/HEIGHT cell of the posn, and that read is guarded
/// — `if (it_vpos < w->current_matrix->nrows && row->enabled_p) ... else
/// { *width = *height = 0; }`. An unpopulated matrix therefore costs GNU one
/// cell of a ten-element list and nothing else.
///
/// This port serves the same queries from a retained snapshot, so it needs a
/// way to *produce* one on demand. That is this type. It carries both answers
/// because one row walk produces both, and a caller that took only the end
/// record would otherwise be tempted to run the walk twice.
#[derive(Clone, Debug)]
pub struct WindowLayoutQuery {
    /// Absolute Lisp character position produced against the final live
    /// source buffer. Keeping this conversion inside the layout transaction
    /// prevents a caller from combining a new end record with an old buffer Z.
    end: LispCharPos1,
    /// Boxed so the query stays pointer-sized: a snapshot is over a kilobyte
    /// and would otherwise dominate every enum that carries this query.
    geometry: Option<Box<WindowDisplaySnapshot>>,
}

/// The display walk's extent, independent of the live viewport. Measurement
/// never changes window-start, point, vscroll or the accepted presentation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum WindowLayoutQueryScope {
    #[default]
    Viewport,
    /// Inert pixel measurement of a source buffer, including an offscreen one.
    /// Width is absolute logical pixels; None means unbounded. The source
    /// projection never changes the live window or publishes a presentation.
    TextExtent {
        buffer: BufferId,
        start: LispCharPos1,
        end: LispCharPos1,
        width: Option<usize>,
        height: Option<std::num::NonZeroUsize>,
    },
    /// A prefix of the live viewport through the complete target row. If the
    /// target is not reached, walk the viewport. Placement and clipping retain
    /// the live start and vscroll; the returned end describes this prefix or
    /// a larger certified observation reused from the query cache.
    Position { target: LispCharPos1 },
    Rows {
        start: LispCharPos1,
        count: std::num::NonZeroUsize,
    },
    /// At least this pixel extent at an explicit source start (or through EOB).
    /// A certified observation may supply a larger complete prefix. Its end
    /// and geometry always describe that returned prefix, without cropping.
    Pixels {
        start: LispCharPos1,
        height: std::num::NonZeroUsize,
    },
}

/// Why an installed frontend query could not produce a coherent row walk.
///
/// This is distinct from [`WindowLayoutQueryOutcome::Unavailable`], which
/// means no display adapter exists (batch/initial startup). A failed installed
/// adapter must never silently degrade an UPDATE query to retained stale data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowLayoutQueryFailure {
    DidNotConverge,
}

impl WindowLayoutQueryFailure {
    pub const fn message(self) -> &'static str {
        match self {
            Self::DidNotConverge => "Window layout query did not converge",
        }
    }
}

/// Result of crossing the frontend's synchronous layout-query boundary.
///
/// Keeping reentrancy distinct from ordinary absence prevents an exclusive
/// layout borrow from silently degrading an UPDATE query to stale retained
/// geometry. Lisp-facing callers can signal the busy state; batch/initial
/// frames can continue to use `Unavailable` as GNU's no-display case.
#[derive(Clone, Debug)]
pub enum WindowLayoutQueryOutcome {
    Ready(WindowLayoutQuery),
    Unavailable,
    LayoutBusy,
    Failed(WindowLayoutQueryFailure),
}

impl WindowLayoutQuery {
    pub fn new(end: LispCharPos1, geometry: Option<WindowDisplaySnapshot>) -> Self {
        Self {
            end,
            geometry: geometry.map(Box::new),
        }
    }

    pub const fn end(&self) -> LispCharPos1 {
        self.end
    }

    pub fn geometry(&self) -> Option<&WindowDisplaySnapshot> {
        self.geometry.as_deref()
    }

    pub fn into_geometry(self) -> Option<WindowDisplaySnapshot> {
        self.geometry.map(|geometry| *geometry)
    }
}

/// Frontend-owned synchronous layout-query callback state.
///
/// Keeping the active call as an enum makes reentrant entry explicit and
/// prevents a temporarily removed callback from being mistaken for an
/// unavailable display. This is Neomacs dependency-boundary machinery, not a
/// GNU `eval.c` primitive, so it belongs beside the window display adapter.
pub(crate) enum WindowLayoutQueryAdapter {
    Unavailable,
    Ready(WindowLayoutQueryCallback),
    Running,
}

/// The frontend's synchronous layout-query callback held by
/// [`WindowLayoutQueryAdapter::Ready`].
pub(crate) type WindowLayoutQueryCallback = Box<
    dyn FnMut(
        &mut crate::emacs_core::eval::Context,
        FrameId,
        WindowId,
        WindowLayoutQueryScope,
    ) -> WindowLayoutQueryOutcome,
>;
