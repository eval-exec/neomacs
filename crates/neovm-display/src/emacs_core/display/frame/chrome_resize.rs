//! Implied chrome changes share native resize intent with explicit dimensions.

use super::super::window_cmds::{
    FrameResizeRequest, FrameSizeParam, frame_non_text_total_height_pixels,
    frame_non_text_total_width_pixels_in_state, frame_size_param_to_pixels,
    frame_text_height_pixels,
};
use super::implied_resize::{self, ImpliedResizeReason, ResizeAxis, ResizePolicy};
use crate::window::{DisplayFrameExt, FrameId, FrameManager, GuiResizeDelivery, PendingGuiResize};
use neovm_eval::emacs_core::eval::Context;
use neovm_value::error::{Flow, signal};
use neovm_value::value::Value;

pub(super) struct ChromeResizeTarget {
    text_width: u32,
    text_height: u32,
    queued: PendingGuiResize,
}

impl ChromeResizeTarget {
    pub(super) fn is_current(&self, frames: &FrameManager, fid: FrameId) -> bool {
        frames.get(fid).and_then(|frame| frame.pending_gui_resize) == Some(self.queued)
    }

    pub(super) fn request_with_explicit_dimensions(
        &self,
        frames: &FrameManager,
        fid: FrameId,
        width: Option<FrameSizeParam>,
        height: Option<FrameSizeParam>,
    ) -> Result<FrameResizeRequest, Flow> {
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        Ok(FrameResizeRequest::TextPixels {
            width: width.map_or(self.text_width, |size| {
                frame_size_param_to_pixels(size, frame.char_width)
            }),
            height: height.map_or(self.text_height, |size| {
                frame_size_param_to_pixels(size, frame.char_height)
            }),
        })
    }
}

pub(super) struct ChromeChange {
    reason: ImpliedResizeReason,
    allocation: (u32, u32),
    non_text: (u32, u32),
}

impl ChromeChange {
    pub(super) fn capture(
        eval: &Context,
        fid: FrameId,
        name: &str,
        value: Value,
        previous: Option<&ChromeResizeTarget>,
    ) -> Result<Option<Self>, Flow> {
        // The canonical GTK profile compensates chrome changes in core. NS
        // instead follows AppKit's external NSToolbar allocation callbacks;
        // applying GTK preservation there would introduce another divergence.
        // Its existing allocation behavior stays unchanged (N-607-ns-toolbar).
        if !cfg!(target_os = "linux") {
            return Ok(None);
        }
        let reason = match name {
            "tool-bar-lines" => ImpliedResizeReason::ToolBarLines,
            "menu-bar-lines" => ImpliedResizeReason::MenuBarLines,
            "internal-border-width" => ImpliedResizeReason::InternalBorderWidth,
            _ => return Ok(None),
        };
        let frames = eval.frame_manager();
        let Some(frame) = frames.get(fid) else {
            return Ok(None);
        };
        if !super::frame_uses_window_system_pixels(frame) || frame.parameter(name) == Some(value) {
            return Ok(None);
        }
        let non_text = (
            frame_non_text_total_width_pixels_in_state(&frames, fid),
            frame_non_text_total_height_pixels(frame),
        );
        let allocation =
            if let Some(previous) = previous.filter(|target| target.is_current(&frames, fid)) {
                (
                    previous.text_width.saturating_add(non_text.0),
                    previous.text_height.saturating_add(non_text.1),
                )
            } else {
                match frame.pending_gui_resize {
                    Some(PendingGuiResize {
                        delivery: GuiResizeDelivery::Sent { width, height, .. },
                        ..
                    }) => (width, height),
                    Some(pending) => {
                        let (width, height) = FrameResizeRequest::Cells {
                            cols: pending.width_cols,
                            total_lines: pending.total_lines,
                        }
                        .text_pixels(&frames, fid)?;
                        (
                            width.saturating_add(non_text.0),
                            height.saturating_add(non_text.1),
                        )
                    }
                    None => (frame.width, frame.height),
                }
            };
        Ok(Some(Self {
            reason,
            allocation,
            non_text,
        }))
    }

    pub(super) fn apply(
        self,
        eval: &mut Context,
        fid: FrameId,
        previous: Option<ChromeResizeTarget>,
    ) -> Result<Option<ChromeResizeTarget>, Flow> {
        if let Some(frame) = eval.frame_manager_mut().get_mut(fid) {
            frame.sync_bar_heights_from_parameters();
        }
        // GNU adjust_frame_size obtains both minima before evaluating the
        // inhibition predicate. These calls can invoke Lisp and change policy.
        let min_width =
            super::super::font::frame_minimum_inner_pixels(eval, fid, ResizeAxis::Horizontal)?;
        let min_height =
            super::super::font::frame_minimum_inner_pixels(eval, fid, ResizeAxis::Vertical)?;
        let (non_text, horizontal, vertical) = {
            let symbols = eval.symbol_mode();
            let inhibit = eval
                .eval_symbol_by_id(neovm_value::intern::intern("frame-inhibit-implied-resize"))?;
            let frames = eval.frame_manager();
            let frame = frames
                .get(fid)
                .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
            let phase = super::implied_resize_creation_phase(frame);
            let border = (frame.internal_border_width().max(0) as u32).saturating_mul(2);
            // A pending target is resize intent, not the current allocation
            // used by GNU's short-circuit checks (frame.c:895-900). In
            // particular, an invalid policy list must not be traversed for an
            // axis whose existing inner size already requires enlargement.
            let horizontal = if frame.width.saturating_sub(border) >= min_width {
                implied_resize::policy(
                    frame,
                    inhibit,
                    phase,
                    self.reason,
                    ResizeAxis::Horizontal,
                    symbols,
                )?
            } else {
                ResizePolicy::PreserveTextGrid
            };
            let vertical = if frame_text_height_pixels(frame) >= min_height {
                implied_resize::policy(
                    frame,
                    inhibit,
                    phase,
                    self.reason,
                    ResizeAxis::Vertical,
                    symbols,
                )?
            } else {
                ResizePolicy::PreserveTextGrid
            };
            (
                (
                    frame_non_text_total_width_pixels_in_state(&frames, fid),
                    frame_non_text_total_height_pixels(frame),
                ),
                horizontal,
                vertical,
            )
        };
        let frames = eval.frame_manager();
        let frame = frames
            .get(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        let border = (frame.internal_border_width().max(0) as u32).saturating_mul(2);
        let width = adjusted_allocation(
            self.allocation.0,
            self.non_text.0,
            non_text.0,
            horizontal,
            min_width.saturating_add(border),
        );
        let height = adjusted_allocation(
            self.allocation.1,
            self.non_text.1,
            non_text.1,
            vertical,
            min_height.saturating_add(non_text.1),
        );
        let previous_current = previous
            .as_ref()
            .is_some_and(|target| target.is_current(&frames, fid));
        let needs_request = (width, height) != self.allocation || previous_current;
        tracing::debug!(
            frame_id = fid.0,
            reason = ?self.reason,
            phase = ?frame.creation_phase,
            ?horizontal,
            ?vertical,
            old_allocation = ?self.allocation,
            old_non_text = ?self.non_text,
            new_non_text = ?non_text,
            requested_allocation = ?(width, height),
            "implied frame chrome geometry"
        );

        // Inhibition still commits the new line accounting/window tree. No
        // native acknowledgement will do this for an unchanged allocation.
        let (frames, buffers) = eval.frame_manager_and_buffers_mut();
        if let Some(frame) = frames.get_mut(fid) {
            frame.refresh_geometry_for_changed_font(buffers);
        }
        if !needs_request {
            return Ok(previous);
        }
        let request = FrameResizeRequest::TextPixels {
            width: width.saturating_sub(non_text.0).max(1),
            height: height.saturating_sub(non_text.1).max(1),
        };
        let (cols, lines) = request.logical_size(frames, fid)?;
        let frame = frames
            .get_mut(fid)
            .ok_or_else(|| signal("error", vec![Value::string("Frame not found")]))?;
        let request_id = frame.queue_pending_gui_resize(cols, lines, None);
        Ok(Some(ChromeResizeTarget {
            text_width: width.saturating_sub(non_text.0).max(1),
            text_height: height.saturating_sub(non_text.1).max(1),
            queued: PendingGuiResize {
                request_id,
                width_cols: cols,
                total_lines: lines,
                delivery: GuiResizeDelivery::Queued,
            },
        }))
    }
}

fn adjusted_allocation(
    old: u32,
    old_non_text: u32,
    new_non_text: u32,
    policy: ResizePolicy,
    minimum: u32,
) -> u32 {
    match policy {
        ResizePolicy::PreserveAllocation if old >= minimum => old,
        ResizePolicy::PreserveAllocation | ResizePolicy::PreserveTextGrid => old
            .saturating_sub(old_non_text)
            .saturating_add(new_non_text)
            .max(minimum),
    }
}
