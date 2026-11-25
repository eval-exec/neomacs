//! Ownership of complete source-adjustment inputs. No snapshot is retained.
//!
//! | Control | Default | Opt-out |
//! | --- | --- | --- |
//! | `NEOMACS_LAYOUT_INPUT_OWNERSHIP` | in-place | any explicit value other than on/1/true/yes |
//!
//! The process policy is immutable after initialization and shares no Lisp
//! state. Both paths run synchronously on the caller's exclusive Context.
//! The legacy path keeps copied inputs for same-binary output checks; both
//! paths use the compact native parameter representation. Static layout-cost
//! comparisons therefore require the staged base binary, not this opt-out.
use super::{
    InactiveMiniWindowSource, ResolvedWindowDisplaySource, WindowDisplaySource,
    WindowLayoutWalkPurpose, WindowParams, window_source_has_fontification_callbacks,
};
use std::ffi::OsStr;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum WindowInputOwnership {
    LegacyCopies,
    InPlace,
}

impl WindowInputOwnership {
    fn from_setting(value: Option<&OsStr>) -> Self {
        match value {
            None => Self::InPlace,
            Some(value) => match value.to_str() {
                Some(value)
                    if ["on", "1", "true", "yes"]
                        .iter()
                        .any(|on| value.trim().eq_ignore_ascii_case(on)) =>
                {
                    Self::InPlace
                }
                Some(_) | None => Self::LegacyCopies,
            },
        }
    }
}

pub(super) fn mode() -> WindowInputOwnership {
    static MODE: OnceLock<WindowInputOwnership> = OnceLock::new();
    *MODE.get_or_init(|| {
        WindowInputOwnership::from_setting(
            std::env::var_os("NEOMACS_LAYOUT_INPUT_OWNERSHIP").as_deref(),
        )
    })
}

/// Resolve the buffer and range that this window actually displays.
///
/// GNU's `with_echo_area_buffer` temporarily installs the echo-area buffer in an
/// inactive mini-window before redisplay measures or walks it **while a message
/// is displayed**; resolve that semantic source once, before fontification and
/// incremental-key creation, so every phase observes the same buffer identity,
/// ticks, range, and point.
pub(super) fn resolve_window_display_source_params_legacy(
    evaluator: &mut neovm_core::emacs_core::Context,
    params: &WindowParams,
    purpose: WindowLayoutWalkPurpose,
) -> ResolvedWindowDisplaySource {
    let window_id = neovm_core::window::WindowId(params.window_id as u64);
    // Hand layout a shared handle to the image catalog: `(space :align-to …)`
    // may embed an `(image …)` operand whose intrinsic size decides the result
    // (GNU resolves it inline with `lookup_image`). This is the single point
    // every window's params pass through that also holds the evaluator.
    let mut params = params.clone();
    if purpose == WindowLayoutWalkPurpose::MiniPreparation {
        params.window_start = params.buffer_begv;
        params.vscroll = 0;
        params.force_start = true;
        params.previous_visible_end = None;
        params.source_extent = crate::types::WindowSourceExtent::AccessibleEnd;
    }
    if let WindowLayoutWalkPurpose::SynchronousQuery(scope) = purpose {
        use neovm_core::window::WindowLayoutQueryScope;
        let start = match scope {
            WindowLayoutQueryScope::TextExtent {
                start,
                end,
                width,
                height,
                ..
            } => {
                params.buffer_size = end
                    .as_i64()
                    .saturating_sub(1)
                    .clamp(params.buffer_begv, params.buffer_size);
                params.point = params.point.min(params.buffer_size);
                params.source_extent = crate::types::WindowSourceExtent::AccessibleEnd;
                params.measurement_width =
                    Some(width.unwrap_or(i32::MAX as usize).min(i32::MAX as usize));
                params.measurement_pixels = height;
                params.hscroll = 0;
                params.force_start = true;
                Some(start)
            }
            WindowLayoutQueryScope::Rows { start, count } => {
                params.measurement_rows = Some(count);
                Some(start)
            }
            WindowLayoutQueryScope::Pixels { start, height } => {
                params.measurement_pixels = Some(height);
                Some(start)
            }
            WindowLayoutQueryScope::Position { target } => {
                // A viewport can reach sparse fontification sites after a
                // fold beyond the target. Preserve that callback extent.
                if !window_source_has_fontification_callbacks(evaluator, params.buffer_id) {
                    params.query_target = Some(crate::types::LayoutCharPos0::new(
                        target.as_i64().saturating_sub(1),
                    ));
                }
                None
            }
            WindowLayoutQueryScope::Viewport => None,
        };
        if let Some(start) = start {
            params.window_start = start
                .as_i64()
                .saturating_sub(1)
                .clamp(params.buffer_begv, params.buffer_size);
            params.vscroll = 0;
            params.previous_visible_end = None;
        }
    }
    params.space_image_catalog = evaluator
        .display_terminals
        .services()
        .and_then(|host| host.image_catalog_shared())
        .map(crate::types::SharedImageCatalog);
    let params = &params;

    if matches!(
        purpose,
        WindowLayoutWalkPurpose::SynchronousQuery(_) | WindowLayoutWalkPurpose::MiniPreparation
    ) || !params.is_minibuffer()
        || evaluator.minibuffer_window_is_active(window_id)
    {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    }

    // The inactive mini-window renders the echo buffer only while a message
    // is displayed; once the message clears it must fall back to its own
    // buffer (`" *Minibuf-0*"`), which is what makes idle mini-window content
    // — `minibuffer-line' status text — visible.  The match is exhaustive on
    // purpose: a new source state must decide its buffer here.
    match InactiveMiniWindowSource::for_context(evaluator) {
        InactiveMiniWindowSource::OwnBuffer => {
            return ResolvedWindowDisplaySource {
                params: params.clone(),
                source: WindowDisplaySource::LiveWindow,
            };
        }
        InactiveMiniWindowSource::EchoMessage => {}
    }

    evaluator.ensure_echo_area_buffers();
    let Some(buf_id) = evaluator.echo_area_display_buffer() else {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    };
    let Some(buffer_size) = evaluator
        .buffer_manager()
        .get(buf_id)
        .map(|buffer| buffer.point_max_char_pos().get() as i64)
    else {
        return ResolvedWindowDisplaySource {
            params: params.clone(),
            source: WindowDisplaySource::LiveWindow,
        };
    };

    let mut resolved = params.clone();
    resolved.buffer_id = buf_id.0;
    resolved.window_start = 0;
    resolved.previous_visible_end = None;
    resolved.point = if resolved.cursor_role.is_active() {
        // GNU's echo-area cursor is the insertion position immediately after
        // the displayed message, not point in the minibuffer's live buffer.
        buffer_size
    } else {
        0
    };
    resolved.buffer_begv = 0;
    resolved.buffer_size = buffer_size;
    ResolvedWindowDisplaySource {
        params: resolved,
        source: WindowDisplaySource::InactiveEchoArea,
    }
}

pub(super) fn resolve_window_display_source_params(
    evaluator: &mut neovm_core::emacs_core::Context,
    params: &mut WindowParams,
    purpose: WindowLayoutWalkPurpose,
) -> WindowDisplaySource {
    let window_id = neovm_core::window::WindowId(params.window_id as u64);
    // Hand layout a shared handle to the image catalog: `(space :align-to …)`
    // may embed an `(image …)` operand whose intrinsic size decides the result
    // (GNU resolves it inline with `lookup_image`). This is the single point
    // every window's params pass through that also holds the evaluator.
    if purpose == WindowLayoutWalkPurpose::MiniPreparation {
        params.window_start = params.buffer_begv;
        params.vscroll = 0;
        params.force_start = true;
        params.previous_visible_end = None;
        params.source_extent = crate::types::WindowSourceExtent::AccessibleEnd;
    }
    if let WindowLayoutWalkPurpose::SynchronousQuery(scope) = purpose {
        use neovm_core::window::WindowLayoutQueryScope;
        let start = match scope {
            WindowLayoutQueryScope::TextExtent {
                start,
                end,
                width,
                height,
                ..
            } => {
                params.buffer_size = end
                    .as_i64()
                    .saturating_sub(1)
                    .clamp(params.buffer_begv, params.buffer_size);
                params.point = params.point.min(params.buffer_size);
                params.source_extent = crate::types::WindowSourceExtent::AccessibleEnd;
                params.measurement_width =
                    Some(width.unwrap_or(i32::MAX as usize).min(i32::MAX as usize));
                params.measurement_pixels = height;
                params.hscroll = 0;
                params.force_start = true;
                Some(start)
            }
            WindowLayoutQueryScope::Rows { start, count } => {
                params.measurement_rows = Some(count);
                Some(start)
            }
            WindowLayoutQueryScope::Pixels { start, height } => {
                params.measurement_pixels = Some(height);
                Some(start)
            }
            WindowLayoutQueryScope::Position { target } => {
                // A viewport can reach sparse fontification sites after a
                // fold beyond the target. Preserve that callback extent.
                if !window_source_has_fontification_callbacks(evaluator, params.buffer_id) {
                    params.query_target = Some(crate::types::LayoutCharPos0::new(
                        target.as_i64().saturating_sub(1),
                    ));
                }
                None
            }
            WindowLayoutQueryScope::Viewport => None,
        };
        if let Some(start) = start {
            params.window_start = start
                .as_i64()
                .saturating_sub(1)
                .clamp(params.buffer_begv, params.buffer_size);
            params.vscroll = 0;
            params.previous_visible_end = None;
        }
    }
    params.space_image_catalog = evaluator
        .display_terminals
        .services()
        .and_then(|host| host.image_catalog_shared())
        .map(crate::types::SharedImageCatalog);

    if matches!(
        purpose,
        WindowLayoutWalkPurpose::SynchronousQuery(_) | WindowLayoutWalkPurpose::MiniPreparation
    ) || !params.is_minibuffer()
        || evaluator.minibuffer_window_is_active(window_id)
    {
        return WindowDisplaySource::LiveWindow;
    }

    // The inactive mini-window renders the echo buffer only while a message
    // is displayed; once the message clears it must fall back to its own
    // buffer (`" *Minibuf-0*"`), which is what makes idle mini-window content
    // — `minibuffer-line' status text — visible.  The match is exhaustive on
    // purpose: a new source state must decide its buffer here.
    match InactiveMiniWindowSource::for_context(evaluator) {
        InactiveMiniWindowSource::OwnBuffer => {
            return WindowDisplaySource::LiveWindow;
        }
        InactiveMiniWindowSource::EchoMessage => {}
    }

    evaluator.ensure_echo_area_buffers();
    let Some(buf_id) = evaluator.echo_area_display_buffer() else {
        return WindowDisplaySource::LiveWindow;
    };
    let Some(buffer_size) = evaluator
        .buffer_manager()
        .get(buf_id)
        .map(|buffer| buffer.point_max_char_pos().get() as i64)
    else {
        return WindowDisplaySource::LiveWindow;
    };

    params.buffer_id = buf_id.0;
    params.window_start = 0;
    params.previous_visible_end = None;
    params.point = if params.cursor_role.is_active() {
        // GNU's echo-area cursor is the insertion position immediately after
        // the displayed message, not point in the minibuffer's live buffer.
        buffer_size
    } else {
        0
    };
    params.buffer_begv = 0;
    params.buffer_size = buffer_size;
    WindowDisplaySource::InactiveEchoArea
}

#[cfg(test)]
#[path = "window_input_ownership/tests/window_input_ownership_policy_test.rs"]
mod tests;
