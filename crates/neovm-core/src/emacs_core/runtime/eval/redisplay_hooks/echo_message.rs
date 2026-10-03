//! Message-time echo resizing precedes GNU's pre-redisplay callback. The
//! exclusively borrowed Context owns each temporary callback/source scope;
//! this adds no mutable process-global or thread-local Lisp state.

use super::*;

impl Context {
    /// GNU message3_frame_nolog -> echo_area_display(true). A height change
    /// requires a complete immediate redisplay and re-arms the frame afterward
    /// (xdisp.c:13768-13779,13817). Same-height messages need no complete pass.
    #[cold]
    #[inline(never)]
    pub(crate) fn gnu_display_message_geometry_flow(&mut self) -> EvalResult {
        if !gnu_redisplay_hooks_enabled()
            || self.noninteractive()
            || self.gnu_redisplay_hooks.active.is_active()
            || self
                .special_variable_value_by_id(intern("inhibit-redisplay"))
                .is_some_and(|value| value.is_truthy())
            || self.redisplay_fn.is_none()
        {
            return Ok(Value::NIL);
        }
        let Some(mut request) = self
            .gnu_mini_geometry_request()
            .filter(|request| request.source == RedisplayMiniGeometrySource::EchoArea)
        else {
            return Ok(Value::NIL);
        };
        let Some(frame) = self
            .frames
            .get(request.frame)
            .filter(|frame| !frame.initial && frame.visibility.is_visible())
        else {
            return Ok(Value::NIL);
        };
        // GNU frame_redisplay_p also checks every TTY ancestor and the
        // terminal's displayed root. Reuse the existing typed frame-tree API.
        if frame.effective_window_system().is_none()
            && (self.frames.root_frame_id(request.frame)
                != self.frames.top_frame_on_terminal(frame.terminal_id)
                || !self
                    .frames
                    .frames_in_reverse_z_order(
                        request.frame,
                        crate::window::RenderFrameVisibility::VisibleOnly,
                    )
                    .contains(&request.frame))
        {
            return Ok(Value::NIL);
        }
        let Some(old_height) = frame
            .find_window(request.window)
            .map(|window| window.bounds().height)
        else {
            return Ok(Value::NIL);
        };
        let mini_only = frame.root_window().id() == request.window;
        // display_echo_area_1 passes false; exact command-boundary sizing
        // remains owned by the later complete redisplay transaction.
        request.exact = false;
        let callback = self.redisplay_prepare_fn.take();
        if callback.is_none() && !mini_only {
            return Ok(Value::NIL);
        }
        {
            let mut preparation = MessageEchoPreparation {
                eval: self,
                callback,
            };
            let mut source = MiniDisplaySource::enter(&mut *preparation.eval, request)?;
            let result = if let Some(prepare) = preparation.callback.as_mut() {
                prepare(source.eval, request)
            } else {
                source
                    .eval
                    .gnu_prepare_mini_only_without_renderer(request.frame, request.window)
            };
            source.finish(result)?;
        }
        self.gnu_redisplay_hooks.echo_geometry_window =
            self.has_current_message().then_some(request.window);
        let changed = self
            .frames
            .get(request.frame)
            .and_then(|frame| frame.find_window(request.window))
            .is_some_and(|window| window.bounds().height != old_height);
        if changed {
            self.gnu_mark_frame_redisplay(request.frame);
            // The preparer has returned to its owner before the complete
            // transaction takes it again. Sizing and hook Flow propagate.
            self.redisplay_with_force_flow(true)?;
            self.gnu_mark_frame_redisplay(request.frame);
        }
        Ok(Value::NIL)
    }
}

/// Exclusive Context/callback ownership across echo preparation. Drop restores
/// the callback on Lisp Flow and Rust panic; independent mutators never share
/// this guard. MiniDisplaySource owns its rooted buffer/marker unwind.
struct MessageEchoPreparation<'a> {
    eval: &'a mut Context,
    callback: Option<Box<dyn FnMut(&mut Context, RedisplayMiniGeometryRequest) -> EvalResult>>,
}

impl Drop for MessageEchoPreparation<'_> {
    fn drop(&mut self) {
        if let Some(callback) = self.callback.take() {
            self.eval.redisplay_prepare_fn = Some(callback);
        }
    }
}

#[cfg(test)]
#[path = "tests/echo_message.rs"]
mod tests;
