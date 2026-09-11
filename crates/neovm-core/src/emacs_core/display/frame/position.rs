//! Decode Lisp frame-position intent before resolving it against geometry.
//!
//! GNU frame.c uses the same forms in gui_figure_window_size and
//! gui_set_frame_parameters. In particular, (+ -10) is NOT the same as -10.

use crate::emacs_core::error::{Flow, LispCondition, signal};
use crate::emacs_core::value::Value;
use crate::emacs_core::window_cmds::expect_int;
use crate::window::{FrameId, FrameManager};

/// A validated position request, not a resolved parent-local coordinate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct FramePositionSpec(PositionKind);

#[derive(Clone, Copy, Debug)]
enum PositionKind {
    Absolute(i32),
    /// Signed displacement from the parent's far edge, less the child extent.
    FarEdge(i64),
    Proportional(f64),
}

/// Integer coordinate contracts differ between GNU's public frame APIs.
#[derive(Clone, Copy, Debug)]
pub(crate) enum FrameCoordinateOrigin {
    /// Negative values are offsets from the right/bottom edge.
    SignedEdges,
    /// Negative values remain outside the left/top edge.
    Absolute,
}

impl FramePositionSpec {
    fn signed_edges(offset: i32) -> Self {
        Self(if offset < 0 {
            PositionKind::FarEdge(i64::from(offset))
        } else {
            PositionKind::Absolute(offset)
        })
    }

    /// Strict integer API admission, unlike permissive frame-parameter parsing.
    pub(crate) fn from_coordinate(
        value: Value,
        origin: FrameCoordinateOrigin,
    ) -> Result<Self, Flow> {
        let offset = i32::try_from(expect_int(&value)?).map_err(|_| {
            signal(
                LispCondition::ArgsOutOfRange,
                vec![
                    value,
                    Value::fixnum(i64::from(i32::MIN)),
                    Value::fixnum(i64::from(i32::MAX)),
                ],
            )
        })?;
        Ok(match origin {
            FrameCoordinateOrigin::SignedEdges => Self::signed_edges(offset),
            FrameCoordinateOrigin::Absolute => Self(PositionKind::Absolute(offset)),
        })
    }

    pub(crate) fn from_lisp(value: Value) -> Option<Self> {
        let kind = if let Some(integer) = value.as_int() {
            return Some(Self::signed_edges(i32::try_from(integer).ok()?));
        } else if value.as_symbol_name() == Some("-") {
            PositionKind::FarEdge(0)
        } else if value.is_cons() && value.cons_cdr().is_cons() {
            let offset = i32::try_from(value.cons_cdr().cons_car().as_int()?).ok()?;
            match value.cons_car().as_symbol_name()? {
                "+" => PositionKind::Absolute(offset),
                "-" if offset != i32::MIN => PositionKind::FarEdge(-i64::from(offset)),
                _ => return None,
            }
        } else {
            let fraction = value.as_float().filter(|v| (0.0..=1.0).contains(v))?;
            PositionKind::Proportional(fraction)
        };
        Some(Self(kind))
    }

    fn resolve(self, parent_extent: Option<u32>, child_extent: u32) -> i64 {
        match self.0 {
            PositionKind::Absolute(offset) => i64::from(offset),
            PositionKind::FarEdge(offset) => parent_extent.map_or(offset, |parent| {
                i64::from(parent) - i64::from(child_extent) + offset
            }),
            PositionKind::Proportional(fraction) => {
                // No desktop workarea is available here for top-level frames.
                // Preserve the existing zero default in that case; native
                // top-level placement remains the platform host's concern.
                let available = parent_extent
                    .unwrap_or(child_extent)
                    .saturating_sub(child_extent);
                (fraction * f64::from(available)) as i64
            }
        }
    }
}

/// Resolve only after size/font changes, then publish numeric parent-local
/// coordinates. Renderers never have to interpret Lisp position expressions.
pub(crate) fn apply_frame_position(
    frames: &mut FrameManager,
    fid: FrameId,
    left: Option<FramePositionSpec>,
    top: Option<FramePositionSpec>,
) {
    let Some(frame) = frames.get(fid) else { return };
    let parent = frame
        .parent_frame
        .as_frame_id()
        .map(FrameId)
        .and_then(|id| frames.get(id));
    let left = left.map(|spec| spec.resolve(parent.map(|p| p.width), frame.width));
    let top = top.map(|spec| spec.resolve(parent.map(|p| p.height), frame.height));
    let frame = frames
        .get_mut(fid)
        .expect("position target was just resolved");
    if let Some(left) = left {
        frame.left_pos = left;
        frame.set_parameter(Value::symbol("left"), Value::fixnum(left));
    }
    if let Some(top) = top {
        frame.top_pos = top;
        frame.set_parameter(Value::symbol("top"), Value::fixnum(top));
    }
}
