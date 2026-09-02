//! Bridge between display runtime InputEvent and neovm-core keyboard::InputEvent.
//!
//! GUI and non-Unix frontends send keysyms plus modifier bitmasks; Unix TTYs
//! send uninterpreted byte batches. This module preserves that distinction
//! while converting the display transport into the core input transport.

use neomacs_app::evaluator_input::EvaluatorInputBatch;
use neomacs_display_runtime::thread_comm::{
    InputEvent as DisplayEvent, MonitorInfo as DisplayMonitorInfo, PointerAction, PointerTarget,
    PositionedPointerInput, ScrollDelta,
};
use neovm_core::emacs_core::builtins::NeomacsMonitorInfo;
use neovm_core::keyboard::{
    self, FrontendLoadPhase, FrontendWebProcessFailure, FrontendWebValue, FrontendWebViewEvent,
    InputEvent as KbInputEvent, MouseButton,
};

fn convert_web_process_failure(
    failure: &neomacs_webview::WebProcessFailure,
) -> FrontendWebProcessFailure {
    match failure {
        neomacs_webview::WebProcessFailure::Crashed => FrontendWebProcessFailure::Crashed,
        neomacs_webview::WebProcessFailure::ExceededMemoryLimit => {
            FrontendWebProcessFailure::ExceededMemoryLimit
        }
        neomacs_webview::WebProcessFailure::Terminated => FrontendWebProcessFailure::Terminated,
        neomacs_webview::WebProcessFailure::Unresponsive => FrontendWebProcessFailure::Unresponsive,
        neomacs_webview::WebProcessFailure::LaunchFailed => FrontendWebProcessFailure::LaunchFailed,
        neomacs_webview::WebProcessFailure::Other(code) => FrontendWebProcessFailure::Other(*code),
    }
}

fn convert_web_value(value: &neomacs_webview::WebValue) -> FrontendWebValue {
    match value {
        neomacs_webview::WebValue::Null => FrontendWebValue::Null,
        neomacs_webview::WebValue::Bool(value) => FrontendWebValue::Bool(*value),
        neomacs_webview::WebValue::Number(value) => FrontendWebValue::Number(*value),
        neomacs_webview::WebValue::String(value) => FrontendWebValue::String(value.clone()),
        neomacs_webview::WebValue::Array(values) => {
            FrontendWebValue::Array(values.iter().map(convert_web_value).collect())
        }
        neomacs_webview::WebValue::Object(values) => FrontendWebValue::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), convert_web_value(value)))
                .collect(),
        ),
    }
}

fn convert_webview_event(event: &neomacs_webview::WebViewEvent) -> FrontendWebViewEvent {
    use neomacs_webview::WebViewEvent;
    match event {
        WebViewEvent::Ready { id, generation } => FrontendWebViewEvent::Ready {
            id: *id,
            generation: generation.get(),
        },
        WebViewEvent::Failed {
            id,
            generation,
            error,
        } => FrontendWebViewEvent::Failed {
            id: *id,
            generation: generation.get(),
            error: error.clone(),
        },
        WebViewEvent::Closed { id, generation } => FrontendWebViewEvent::Closed {
            id: *id,
            generation: generation.get(),
        },
        WebViewEvent::TitleChanged {
            id,
            generation,
            title,
        } => FrontendWebViewEvent::TitleChanged {
            id: *id,
            generation: generation.get(),
            title: title.clone(),
        },
        WebViewEvent::UriChanged {
            id,
            generation,
            uri,
        } => FrontendWebViewEvent::UriChanged {
            id: *id,
            generation: generation.get(),
            uri: uri.clone(),
        },
        WebViewEvent::LoadProgressChanged {
            id,
            generation,
            progress,
        } => FrontendWebViewEvent::LoadProgressChanged {
            id: *id,
            generation: generation.get(),
            progress: *progress,
        },
        WebViewEvent::LoadChanged {
            id,
            generation,
            phase,
        } => FrontendWebViewEvent::LoadChanged {
            id: *id,
            generation: generation.get(),
            phase: match phase {
                neomacs_webview::LoadPhase::Started => FrontendLoadPhase::Started,
                neomacs_webview::LoadPhase::Redirected => FrontendLoadPhase::Redirected,
                neomacs_webview::LoadPhase::Committed => FrontendLoadPhase::Committed,
                neomacs_webview::LoadPhase::Finished => FrontendLoadPhase::Finished,
            },
        },
        WebViewEvent::LoadFinished {
            id,
            generation,
            navigation,
        } => FrontendWebViewEvent::LoadFinished {
            id: *id,
            generation: generation.get(),
            navigation: navigation.map(neomacs_webview::NavigationId::get),
        },
        WebViewEvent::ScriptFinished {
            view,
            generation,
            request,
            result,
        } => FrontendWebViewEvent::ScriptFinished {
            view: *view,
            generation: generation.get(),
            request: request.get(),
            result: result
                .as_ref()
                .map(convert_web_value)
                .map_err(ToString::to_string),
        },
        WebViewEvent::ProcessFailed {
            id,
            generation,
            failure,
        } => FrontendWebViewEvent::ProcessFailed {
            id: *id,
            generation: generation.get(),
            failure: convert_web_process_failure(failure),
        },
        WebViewEvent::FocusChanged {
            id,
            generation,
            focused,
        } => FrontendWebViewEvent::FocusChanged {
            id: *id,
            generation: generation.get(),
            focused: *focused,
        },
    }
}

pub(crate) fn should_log_display_event(event: &DisplayEvent) -> bool {
    !matches!(
        event,
        DisplayEvent::PositionedPointer(PositionedPointerInput {
            action: PointerAction::Move { .. },
            ..
        })
    )
}

pub(crate) fn convert_monitor_infos(monitors: &[DisplayMonitorInfo]) -> Vec<NeomacsMonitorInfo> {
    monitors
        .iter()
        .map(|monitor| NeomacsMonitorInfo {
            x: monitor.x,
            y: monitor.y,
            width: monitor.width,
            height: monitor.height,
            scale: monitor.scale,
            width_mm: monitor.width_mm,
            height_mm: monitor.height_mm,
            name: monitor.name.clone(),
        })
        .collect()
}

/// Convert a display runtime input event to a neovm-core keyboard input event.
///
/// The batch is empty for input that should be silently dropped (for example,
/// key releases and modifier-only keys). Presented pointer input expands to an
/// observation immediately followed by its raw evaluator action.
pub(crate) fn convert_display_event(event: &DisplayEvent) -> EvaluatorInputBatch<'_> {
    if let DisplayEvent::Frontend(event) = event {
        return EvaluatorInputBatch::from_frontend_event(event);
    }
    if let DisplayEvent::PositionedPointer(input) = event {
        return convert_positioned_pointer_input(*input);
    }
    EvaluatorInputBatch::from_optional(convert_single_display_event(event))
}

fn convert_positioned_pointer_input(input: PositionedPointerInput) -> EvaluatorInputBatch<'static> {
    let position = input.position;
    let observation = match input.target {
        PointerTarget::Presented { presentation, hit } => Some(KbInputEvent::PresentedRegion {
            presentation,
            hit,
            x: position.x,
            y: position.y,
            target_frame_id: position.target_frame_id,
        }),
        PointerTarget::Unpresented => None,
    };
    let action = match input.action {
        PointerAction::Button {
            button,
            pressed,
            modifiers,
            ..
        } => {
            let button = match button {
                1 => MouseButton::Left,
                2 => MouseButton::Middle,
                3 => MouseButton::Right,
                4 => MouseButton::Button4,
                5 => MouseButton::Button5,
                _ => return EvaluatorInputBatch::empty(),
            };
            if pressed {
                KbInputEvent::MousePress {
                    button,
                    x: position.x,
                    y: position.y,
                    modifiers: keyboard::render_modifiers_to_modifiers(modifiers),
                    target_frame_id: position.target_frame_id,
                }
            } else {
                KbInputEvent::MouseRelease {
                    button,
                    x: position.x,
                    y: position.y,
                    target_frame_id: position.target_frame_id,
                }
            }
        }
        PointerAction::Move { modifiers } => KbInputEvent::MouseMove {
            x: position.x,
            y: position.y,
            modifiers: keyboard::render_modifiers_to_modifiers(modifiers),
            target_frame_id: position.target_frame_id,
        },
        PointerAction::Scroll {
            delta, modifiers, ..
        } => {
            let modifiers = keyboard::render_modifiers_to_modifiers(modifiers);
            match delta {
                ScrollDelta::Lines { x, y } => KbInputEvent::MouseScroll {
                    delta_x: x,
                    delta_y: y,
                    x: position.x,
                    y: position.y,
                    modifiers,
                    target_frame_id: position.target_frame_id,
                },
                ScrollDelta::Pixels { x, y } => KbInputEvent::PixelScroll {
                    delta_x: x,
                    delta_y: y,
                    x: position.x,
                    y: position.y,
                    modifiers,
                    target_frame_id: position.target_frame_id,
                },
            }
        }
    };
    EvaluatorInputBatch::ordered(observation, action)
}

fn convert_single_display_event(event: &DisplayEvent) -> Option<KbInputEvent> {
    match event {
        DisplayEvent::Frontend(_) => unreachable!("handled by convert_display_event"),
        DisplayEvent::PositionedPointer(_) => unreachable!("handled by convert_display_event"),
        DisplayEvent::RawTtyBytes {
            bytes,
            emacs_frame_id,
        } => Some(if *emacs_frame_id == 0 {
            // The startup TTY is terminal 0.  Its input must continue to use
            // that terminal's remembered top frame even while a frame on an
            // auxiliary TTY is globally selected.
            KbInputEvent::raw_tty_bytes_for_terminal(bytes.clone(), 0)
        } else {
            KbInputEvent::raw_tty_bytes(bytes.clone(), *emacs_frame_id)
        }),
        DisplayEvent::MenuSelection { index } => {
            Some(KbInputEvent::MenuSelection { index: *index })
        }
        // The display runtime already reports drops, but the evaluator has no
        // corresponding input type yet. Keep that existing behavior explicit
        // so adding WebView events does not rely on a catch-all arm.
        DisplayEvent::FileDrop { .. } => None,
        DisplayEvent::ImageStateChanged { event } => {
            Some(KbInputEvent::ImageStateChanged { event: *event })
        }
        DisplayEvent::ToolBarClick {
            index,
            emacs_frame_id,
        } => Some(KbInputEvent::ToolBarClick {
            index: *index,
            emacs_frame_id: *emacs_frame_id,
        }),
        DisplayEvent::PresentedPointer {
            presentation,
            interaction,
            pressed,
            button,
            x,
            y,
            emacs_frame_id,
        } => Some(KbInputEvent::PresentedPointer {
            presentation: *presentation,
            interaction: *interaction,
            pressed: *pressed,
            button: *button,
            x: *x,
            y: *y,
            emacs_frame_id: *emacs_frame_id,
        }),
        DisplayEvent::PresentationActivated {
            presentation,
            emacs_frame_id,
        } => Some(KbInputEvent::PresentationActivated {
            presentation: *presentation,
            emacs_frame_id: *emacs_frame_id,
        }),
        DisplayEvent::PresentationDiscarded {
            presentation,
            emacs_frame_id,
        } => Some(KbInputEvent::PresentationDiscarded {
            presentation: *presentation,
            emacs_frame_id: *emacs_frame_id,
        }),
        DisplayEvent::PresentationRetired { presentation } => {
            Some(KbInputEvent::PresentationRetired {
                presentation: *presentation,
            })
        }
        DisplayEvent::MenuBarClick {
            index,
            key,
            menu_x,
            anchor,
            emacs_frame_id,
        } => Some(KbInputEvent::MenuBarClick {
            index: *index,
            key: key.clone(),
            menu_x: *menu_x,
            menu_y: 0.0,
            anchor_x: anchor.x,
            anchor_y: anchor.y,
            anchor_width: anchor.width,
            anchor_height: anchor.height,
            emacs_frame_id: *emacs_frame_id,
        }),
        DisplayEvent::MonitorsChanged { monitors } => Some(KbInputEvent::MonitorsChanged {
            monitors: convert_monitor_infos(monitors),
        }),
        // GPU device lost and rebuilt: the evaluator re-resolves media and
        // forces a full redisplay.
        DisplayEvent::DisplayReset => Some(KbInputEvent::DisplayReset),
        DisplayEvent::WebView(event) => Some(KbInputEvent::WebView(convert_webview_event(event))),
        // A shader surface failed to build on the render thread past naga
        // pre-validation: hand it to the evaluator to surface to Lisp.
        DisplayEvent::SurfaceCreateFailed { id, error } => {
            Some(KbInputEvent::SurfaceCreateFailed {
                id: *id,
                error: error.clone(),
            })
        }
        DisplayEvent::FrameShaderFailed { error } => Some(KbInputEvent::FrameShaderFailed {
            error: error.clone(),
        }),
        #[cfg(feature = "neo-term")]
        DisplayEvent::TerminalCreateFailed { id, error } => {
            Some(KbInputEvent::TerminalCreateFailed {
                id: *id,
                error: error.clone(),
            })
        }
        #[cfg(feature = "neo-term")]
        DisplayEvent::TerminalExited { id } => Some(KbInputEvent::TerminalExited { id: *id }),
        #[cfg(feature = "neo-term")]
        DisplayEvent::TerminalTitleChanged { id, title } => {
            Some(KbInputEvent::TerminalTitleChanged {
                id: *id,
                title: title.clone(),
            })
        }
    }
}

#[cfg(test)]
#[path = "input_bridge_test.rs"]
mod tests;
