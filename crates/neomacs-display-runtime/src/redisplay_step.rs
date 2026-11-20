//! Dormant InlineSerialMain driver for the display-runtime redisplay entries.
//!
//! Hosted activation remains refused. Nothing in this module is installed on
//! the serial call path. The named [`RedisplayRuntime`] methods stay the
//! direct bodies, and each function here is a one-arm call of that body.
//! There is no thread test, TLS read, env read, or second algorithm.

use std::time::Duration;

use neomacs_layout_engine::RowsQueryPolicy;
use neovm_core::Value;
use neovm_core::emacs_core::error::Flow;
use neovm_core::emacs_core::eval::{Context, RedisplayMiniGeometryRequest};
use neovm_core::emacs_core::font::GstringShapeOutcome;
use neovm_core::window::{FrameId, WindowId, WindowLayoutQueryOutcome, WindowLayoutQueryScope};

use crate::redisplay::{FrameLayoutPurpose, PreparedFrameDisplay, RedisplayRuntime};

/// Serial installer. Owned drivers are not constructible in this slice.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RedisplayInstaller {
    InlineSerialMain,
}

/// Serial step. Suspension is uninhabited: the only arm is the direct result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RedisplayStep<T> {
    Done(T),
}

impl<T> RedisplayStep<T> {
    #[cfg_attr(
        not(test),
        expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
    )]
    pub(crate) fn into_direct(self) -> T {
        let RedisplayStep::Done(value) = self;
        value
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn resolved_scroll_preview(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &Context,
    frame: FrameId,
    window: WindowId,
    inputs: Vec<neomacs_display_protocol::input_progress::InputReceipt>,
) -> RedisplayStep<Option<neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent>> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.resolved_scroll_preview(evaluator, frame, window, inputs))
        }
    }
}

#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(crate) fn maintain_scroll_coverage(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &Context,
) -> RedisplayStep<(Option<Duration>, bool)> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.maintain_scroll_coverage(evaluator))
        }
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn shape_gstring(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    gstring: Value,
    direction: Value,
) -> RedisplayStep<GstringShapeOutcome> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.shape_gstring(gstring, direction))
        }
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn prepare_frame(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &mut Context,
    frame_id: FrameId,
    purpose: FrameLayoutPurpose,
) -> RedisplayStep<Option<PreparedFrameDisplay>> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.prepare_frame(evaluator, frame_id, purpose))
        }
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn prepare_minibuffer_geometry(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &mut Context,
    request: RedisplayMiniGeometryRequest,
) -> RedisplayStep<Result<Value, Flow>> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.prepare_minibuffer_geometry(evaluator, request))
        }
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn query_window(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &mut Context,
    frame_id: FrameId,
    window_id: WindowId,
    scope: WindowLayoutQueryScope,
) -> RedisplayStep<WindowLayoutQueryOutcome> {
    match installer {
        RedisplayInstaller::InlineSerialMain => {
            RedisplayStep::Done(runtime.query_window(evaluator, frame_id, window_id, scope))
        }
    }
}

#[expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")]
pub(crate) fn query_window_with_policy(
    installer: RedisplayInstaller,
    runtime: &RedisplayRuntime,
    evaluator: &mut Context,
    frame_id: FrameId,
    window_id: WindowId,
    scope: WindowLayoutQueryScope,
    policy: &RowsQueryPolicy,
) -> RedisplayStep<WindowLayoutQueryOutcome> {
    match installer {
        RedisplayInstaller::InlineSerialMain => RedisplayStep::Done(
            runtime.query_window_with_policy(evaluator, frame_id, window_id, scope, policy),
        ),
    }
}

/// Display-runtime serial entries whose lowering must match the parent.
#[cfg_attr(
    not(test),
    expect(dead_code, reason = "dormant until hosted activation (S2 step 2)")
)]
pub(crate) const SERIAL_REDISPLAY_ENTRIES: &[&str] = &[
    "resolved_scroll_preview",
    "maintain_scroll_coverage",
    "shape_gstring",
    "prepare_frame",
    "prepare_minibuffer_geometry",
    "query_window",
    "query_window_with_policy",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serial_bodies_do_not_call_the_executor() {
        let source = include_str!("redisplay.rs");
        assert!(
            !source.contains("redisplay_step") && !source.contains("RedisplayInstaller"),
            "serial redisplay bodies must stay the direct methods"
        );
        for name in SERIAL_REDISPLAY_ENTRIES {
            assert!(
                source.contains(&format!("fn {name}(")),
                "missing serial entry {name}"
            );
        }
    }

    #[test]
    fn inline_serial_maintain_matches_the_direct_body() {
        let eval = Context::new(neovm_core::emacs_core::eval_hook_installers::install_eval_hooks());
        let direct_runtime = RedisplayRuntime::new_without_font_metrics();
        let step_runtime = RedisplayRuntime::new_without_font_metrics();
        let direct = direct_runtime.maintain_scroll_coverage(&eval);
        let via =
            maintain_scroll_coverage(RedisplayInstaller::InlineSerialMain, &step_runtime, &eval)
                .into_direct();
        assert_eq!(direct, via);
    }
}
