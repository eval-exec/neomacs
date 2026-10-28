//! Reconcile a restored tree through window.el's canonical resize planner.
//!
//! One exclusive Context borrow owns the restore. Geometry and IDs are copied
//! only across the immediate Lisp call; no cache, TLS slot or retained plan is
//! created, and the live root is checked again before committing staged sizes.

use super::*;
use crate::window::body::WindowBodyCellSize;
use crate::window::{Frame, FrameId, Rect, SplitDirection};
use neomacs_display_protocol::types::Px;

#[derive(Clone, Copy, Debug)]
enum ResizeDimension {
    Width,
    Height,
}

impl ResizeDimension {
    fn size(self, bounds: Rect) -> Px {
        Px(match self {
            Self::Width => bounds.width,
            Self::Height => bounds.height,
        })
    }

    fn origin(self, bounds: Rect) -> Px {
        Px(match self {
            Self::Width => bounds.x,
            Self::Height => bounds.y,
        })
    }

    fn align_root(self, frame: &mut Frame, target: Rect) {
        let top_line = frame.frame_top_margin();
        let mut bounds = *frame.root_window().bounds();
        match self {
            Self::Width => {
                bounds.x = target.x;
                frame.root_window_mut().set_left_col(0);
            }
            Self::Height => {
                bounds.y = target.y;
                frame.root_window_mut().set_top_line(top_line);
            }
        }
        frame.root_window_mut().set_bounds(bounds);
    }
}

#[derive(Clone, Copy, Debug)]
enum ResizeRestrictions {
    Respect,
    ReasonableMinimums,
}

impl From<ResizeDimension> for Value {
    fn from(dimension: ResizeDimension) -> Self {
        match dimension {
            ResizeDimension::Width => Self::T,
            ResizeDimension::Height => Self::NIL,
        }
    }
}

impl From<ResizeRestrictions> for Value {
    fn from(restrictions: ResizeRestrictions) -> Self {
        match restrictions {
            ResizeRestrictions::Respect => Self::NIL,
            ResizeRestrictions::ReasonableMinimums => Self::T,
        }
    }
}

pub(super) fn reconcile(
    eval: &mut crate::emacs_core::eval::Context,
    frame_id: FrameId,
) -> EvalResult {
    let Some(frame) = eval.frames.get(frame_id) else {
        return Ok(Value::NIL);
    };
    // Before window.el is installed, bootstrap fixtures only have native
    // geometry. A leaf root likewise has no children whose allocation needs
    // Lisp policy (GNU window.c:5066-5085). Never fall back after a Lisp error.
    if frame.root_window().children().is_empty()
        || !eval.obarray.fboundp("window--resize-root-window")
    {
        if let Some(frame) = eval.frames.get_mut(frame_id) {
            frame.reconcile_restored_window_configuration_geometry();
        }
        return Ok(Value::NIL);
    }

    let target = frame.restored_window_configuration_root_bounds();
    let root = frame.root_window().id();
    for dimension in [ResizeDimension::Width, ResizeDimension::Height] {
        let Some(frame) = eval.frames.get_mut(frame_id) else {
            return Ok(Value::NIL);
        };
        if frame.root_window().id() != root {
            return Ok(Value::NIL);
        }
        let previous = *frame.root_window().bounds();
        if dimension.size(previous) == dimension.size(target)
            && dimension.origin(previous) == dimension.origin(target)
        {
            continue;
        }
        dimension.align_root(frame, target);
        let desired = dimension.size(target);
        let delta = Px(desired.get() - dimension.size(previous).get());
        // GNU resize_frame_windows first asks window.el to honor normal
        // fractions, fixed/preserved sizes and minima, then retries with
        // reasonable minima only if the staged geometry cannot be applied
        // (window.c:5098-5115). The planner is the single donor-selection rule.
        for restrictions in [
            ResizeRestrictions::Respect,
            ResizeRestrictions::ReasonableMinimums,
        ] {
            eval.funcall_general(
                Value::symbol("window--resize-root-window"),
                vec![
                    Value::make_window(root.0),
                    Value::fixnum(delta.get().round() as i64),
                    Value::from(dimension),
                    Value::from(restrictions),
                    Value::T,
                ],
            )?;
            let Some(frame) = eval.frames.get_mut(frame_id) else {
                return Ok(Value::NIL);
            };
            if frame.root_window().id() != root {
                return Ok(Value::NIL);
            }
            let horizontal = matches!(dimension, ResizeDimension::Width);
            if frame.root_window().new_pixel() == Some(desired.get().round() as i64)
                && crate::window::window_resize_check(
                    frame.tree(),
                    root,
                    match dimension {
                        ResizeDimension::Width => SplitDirection::Horizontal,
                        ResizeDimension::Height => SplitDirection::Vertical,
                    },
                    WindowBodyCellSize::new(frame.char_width, frame.char_height),
                )
            {
                let (cw, ch) = (frame.char_width, frame.char_height);
                crate::window::window_resize_apply(frame.tree_mut(), root, horizontal, cw, ch);
                // Commit pixels first, then reconcile character totals through
                // their Lisp owner, exactly as resize_frame_windows does.
                eval.funcall_general(
                    Value::symbol("window--pixel-to-total"),
                    vec![Value::make_frame(frame_id.0), Value::from(dimension)],
                )?;
                break;
            }
        }
    }
    if let Some(frame) = eval.frames.get_mut(frame_id)
        && frame.root_window().id() == root
    {
        frame.recalculate_minibuffer_bounds();
        frame.sync_minibuffer_top_line_after_root_resize();
        frame.tty_posn_adjust_current_matrices();
    }
    Ok(Value::NIL)
}
