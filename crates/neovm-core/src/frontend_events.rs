//! Semantic boundary for events arriving from the display frontend.
//!
//! `InputEvent` is a transport enum.  Consumers must use this module instead
//! of inferring command-input semantics from the transport variant directly.

use crate::keyboard::{InputEvent, InputPendingFilter};
use std::collections::VecDeque;

/// Report a renderer/device-specific full-frame shader failure without
/// putting Neomacs frontend policy in GNU's keyboard mirror.
///
/// The primitive is globally callable under `-Q`, while the optional Lisp
/// helper library owns the customizable hook. Until that library has defined
/// the hook, preserve user visibility with an echo-area fallback.
pub(crate) fn report_frame_shader_failure(
    eval: &mut crate::emacs_core::Context,
    error: &str,
) -> Result<InternalEventEffects, crate::emacs_core::error::Flow> {
    let hook = "neomacs-frame-shader-error-functions";
    if eval.obarray.symbol_value(hook).is_none() {
        let message = format!("neomacs frame shader failed to build: {error}");
        eval.set_current_message(Some(crate::heap_types::LispString::from_utf8(&message)));
    } else {
        let args = [
            crate::emacs_core::Value::symbol(hook),
            crate::emacs_core::Value::string(error),
        ];
        crate::emacs_core::hook_runtime::run_named_hook_with_args(eval, &args)?;
    }
    Ok(InternalEventEffects {
        redisplay_needed: true,
    })
}

/// The evaluator's single ordered queue of transport events from the frontend.
///
/// Storage mechanics stay here so semantic servicing cannot accidentally grow
/// another side queue with different ordering rules.
#[derive(Default)]
pub(crate) struct FrontendEventQueue {
    events: VecDeque<InputEvent>,
}

impl FrontendEventQueue {
    pub(crate) fn front(&self) -> Option<&InputEvent> {
        self.events.front()
    }

    pub(crate) fn pop_visible_front(&mut self) -> Option<InputEvent> {
        debug_assert!(
            !self.events.front().is_some_and(is_internal),
            "internal frontend events must be serviced before visible input is popped"
        );
        self.events.pop_front()
    }

    pub(crate) fn push_front(&mut self, event: InputEvent) {
        self.events.push_front(event);
    }

    pub(crate) fn push_back(&mut self, event: InputEvent) {
        self.events.push_back(event);
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    pub(crate) fn len(&self) -> usize {
        self.events.len()
    }

    pub(crate) fn take_leading_internal(&mut self) -> Option<InternalFrontendEvent> {
        if !self.events.front().is_some_and(is_internal) {
            return None;
        }
        let event = self.events.pop_front().expect("queue front was present");
        Some(match event {
            InputEvent::PresentationActivated {
                presentation,
                emacs_frame_id,
            } => InternalFrontendEvent::PresentationActivated {
                presentation,
                emacs_frame_id,
            },
            InputEvent::PresentationDiscarded {
                presentation,
                emacs_frame_id,
            } => InternalFrontendEvent::PresentationDiscarded {
                presentation,
                emacs_frame_id,
            },
            InputEvent::PresentationRetired { presentation } => {
                InternalFrontendEvent::PresentationRetired { presentation }
            }
            InputEvent::LayoutInvalidated => InternalFrontendEvent::LayoutInvalidated,
            InputEvent::ImageStateChanged { event } => {
                InternalFrontendEvent::ImageStateChanged { event }
            }
            _ => unreachable!("all internal frontend events require an explicit service action"),
        })
    }

    pub(crate) fn has_pending_input(
        &self,
        filter: InputPendingFilter,
        track_mouse: bool,
        ignored_while_no_input: impl Fn(&str) -> bool,
    ) -> bool {
        self.events
            .iter()
            .any(|event| counts_as_pending(event, filter, track_mouse, &ignored_while_no_input))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InternalFrontendEvent {
    PresentationActivated {
        presentation: u64,
        emacs_frame_id: u64,
    },
    PresentationDiscarded {
        presentation: u64,
        emacs_frame_id: u64,
    },
    PresentationRetired {
        presentation: u64,
    },
    LayoutInvalidated,
    ImageStateChanged {
        event: crate::emacs_core::image_catalog::ImageStateEvent,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct InternalEventEffects {
    pub(crate) redisplay_needed: bool,
}

impl InternalEventEffects {
    pub(crate) fn merge(self, other: Self) -> Self {
        Self {
            redisplay_needed: self.redisplay_needed || other.redisplay_needed,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrontendEventClass {
    Command,
    LispSpecial,
    Internal,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingPolicy {
    Always,
    Never,
    TrackMouse,
    Focus { focused: bool },
    Filterable(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct FrontendEventSemantics {
    class: FrontendEventClass,
    pending: PendingPolicy,
    interrupts: bool,
    wait_special: bool,
}

const fn command() -> FrontendEventSemantics {
    FrontendEventSemantics {
        class: FrontendEventClass::Command,
        pending: PendingPolicy::Always,
        interrupts: true,
        wait_special: false,
    }
}

const fn special(
    pending: PendingPolicy,
    interrupts: bool,
    wait_special: bool,
) -> FrontendEventSemantics {
    FrontendEventSemantics {
        class: FrontendEventClass::LispSpecial,
        pending,
        interrupts,
        wait_special,
    }
}

/// Return the complete semantic policy for an input transport variant.
///
/// This match is deliberately exhaustive: adding a frontend event must force
/// an explicit choice about command visibility and scheduler behavior.
fn semantics(event: &InputEvent) -> FrontendEventSemantics {
    match event {
        InputEvent::RawTtyBytes { .. } | InputEvent::TtyCharacter { .. } => command(),
        InputEvent::KeyPress { .. } => command(),
        InputEvent::MousePress { .. } => command(),
        InputEvent::MouseRelease { .. } => command(),
        InputEvent::MouseMove { .. } => special(PendingPolicy::TrackMouse, false, true),
        InputEvent::PresentedRegion { .. } => special(PendingPolicy::Never, false, false),
        InputEvent::MouseScroll { .. } => command(),
        InputEvent::PixelScroll { .. } => special(PendingPolicy::Always, true, false),
        InputEvent::LayoutInvalidated | InputEvent::ImageStateChanged { .. } => {
            FrontendEventSemantics {
                class: FrontendEventClass::Internal,
                pending: PendingPolicy::Never,
                interrupts: false,
                wait_special: false,
            }
        }
        InputEvent::MenuSelection { .. } => command(),
        InputEvent::ToolBarClick { .. } => command(),
        InputEvent::PresentedPointer { .. } => command(),
        InputEvent::PresentationActivated { .. }
        | InputEvent::PresentationDiscarded { .. }
        | InputEvent::PresentationRetired { .. } => FrontendEventSemantics {
            class: FrontendEventClass::Internal,
            pending: PendingPolicy::Never,
            interrupts: false,
            wait_special: false,
        },
        InputEvent::MenuBarClick { .. } => command(),
        InputEvent::Resize { .. } => special(PendingPolicy::Never, false, true),
        // Same policy as Resize: not command input, serviced during waits so
        // recovery does not sit behind a keystroke.
        InputEvent::DisplayReset => special(PendingPolicy::Never, false, true),
        InputEvent::WebView(..) => special(PendingPolicy::Never, false, true),
        // A shader-surface build failure: not command input; serviced during
        // waits so the error surfaces promptly instead of behind a keystroke.
        InputEvent::SurfaceCreateFailed { .. } => special(PendingPolicy::Never, false, true),
        InputEvent::FrameShaderFailed { .. } => special(PendingPolicy::Never, false, true),
        InputEvent::TerminalCreateFailed { .. }
        | InputEvent::TerminalExited { .. }
        | InputEvent::TerminalTitleChanged { .. } => special(PendingPolicy::Never, false, true),
        InputEvent::Focus { focused, .. } => {
            special(PendingPolicy::Focus { focused: *focused }, false, false)
        }
        InputEvent::MonitorsChanged { .. } => {
            special(PendingPolicy::Filterable("monitors-changed"), false, true)
        }
        InputEvent::SelectWindow { .. } => {
            special(PendingPolicy::Filterable("select-window"), true, false)
        }
        InputEvent::WindowClose { .. } => special(PendingPolicy::Always, true, true),
    }
}

pub(crate) fn is_internal(event: &InputEvent) -> bool {
    semantics(event).class == FrontendEventClass::Internal
}

pub(crate) fn interrupts(event: &InputEvent) -> bool {
    semantics(event).interrupts
}

pub(crate) fn is_wait_special(event: &InputEvent, track_mouse: bool) -> bool {
    if matches!(event, InputEvent::MouseMove { .. }) {
        return !track_mouse;
    }
    semantics(event).wait_special
}

fn counts_as_pending(
    event: &InputEvent,
    filter: InputPendingFilter,
    track_mouse: bool,
    ignored_while_no_input: &impl Fn(&str) -> bool,
) -> bool {
    match semantics(event).pending {
        PendingPolicy::Always => true,
        PendingPolicy::Never => false,
        PendingPolicy::TrackMouse => track_mouse,
        PendingPolicy::Focus { focused } => !filter.ignores(
            if focused { "focus-in" } else { "focus-out" },
            ignored_while_no_input,
        ),
        PendingPolicy::Filterable(symbol) => !filter.ignores(symbol, ignored_while_no_input),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_shader_failure_is_visible_without_optional_lisp_library() {
        let mut eval = crate::emacs_core::Context::new();
        assert!(
            eval.obarray
                .symbol_value("neomacs-frame-shader-error-functions")
                .is_none(),
            "the -Q primitive path starts without neomacs-surface.el"
        );

        let effects = report_frame_shader_failure(&mut eval, "device rejected module")
            .expect("the fallback reporter must not hide the original failure");

        assert_eq!(
            eval.current_message_text().as_deref(),
            Some("neomacs frame shader failed to build: device rejected module")
        );
        assert!(effects.redisplay_needed);
    }

    #[test]
    fn frame_shader_failure_uses_customizable_hook_when_loaded() {
        let mut eval = crate::emacs_core::Context::new();
        eval.eval_str(
            r#"(setq neomacs-frame-shader-error-functions
                     (list (lambda (error)
                             (setq neomacs-frame-shader-test-error error))))"#,
        )
        .expect("install frame shader error hook");

        let effects = report_frame_shader_failure(&mut eval, "backend detail")
            .expect("frame shader hook should run");

        let captured = eval
            .obarray
            .symbol_value("neomacs-frame-shader-test-error")
            .copied()
            .expect("hook captured the renderer error");
        assert_eq!(
            captured
                .as_lisp_string()
                .and_then(|string| string.as_utf8_str()),
            Some("backend detail")
        );
        assert!(effects.redisplay_needed);
    }

    fn assert_policy(
        event: InputEvent,
        class: FrontendEventClass,
        pending: PendingPolicy,
        interrupts: bool,
        wait_special: bool,
    ) {
        let policy = semantics(&event);
        assert_eq!(policy.class, class, "class for {event:?}");
        assert_eq!(policy.pending, pending, "pending policy for {event:?}");
        assert_eq!(
            policy.interrupts, interrupts,
            "interrupt policy for {event:?}"
        );
        assert_eq!(
            policy.wait_special, wait_special,
            "wait policy for {event:?}"
        );
    }

    #[test]
    fn every_transport_variant_has_locked_down_semantics() {
        use crate::keyboard::{KeyEvent, Modifiers, MouseButton};

        let command_events = [
            InputEvent::raw_tty_bytes(vec![0x1b], 0),
            InputEvent::TtyCharacter {
                character: crate::emacs_core::emacs_char::EmacsChar::from_char('k'),
                target: crate::keyboard::TtyInputTarget::SelectedFrame,
            },
            InputEvent::key_press(KeyEvent::char('k')),
            InputEvent::MousePress {
                button: MouseButton::Left,
                x: 0.0,
                y: 0.0,
                modifiers: Modifiers::none(),
                target_frame_id: 0,
            },
            InputEvent::MouseRelease {
                button: MouseButton::Left,
                x: 0.0,
                y: 0.0,
                target_frame_id: 0,
            },
            InputEvent::MouseScroll {
                delta_x: 0.0,
                delta_y: 1.0,
                x: 0.0,
                y: 0.0,
                modifiers: Modifiers::none(),
                target_frame_id: 0,
            },
            InputEvent::MenuSelection { index: 0 },
            InputEvent::ToolBarClick {
                index: 0,
                emacs_frame_id: 0,
            },
            InputEvent::PresentedPointer {
                presentation: 1,
                interaction: 0,
                pressed: true,
                button: 1,
                x: 0.0,
                y: 0.0,
                emacs_frame_id: 0,
            },
            InputEvent::MenuBarClick {
                index: 0,
                key: "file".to_string(),
                menu_x: 0.0,
                menu_y: 0.0,
                anchor_x: 0.0,
                anchor_y: 0.0,
                anchor_width: 0.0,
                anchor_height: 0.0,
                emacs_frame_id: 0,
            },
        ];
        for event in command_events {
            assert_policy(
                event,
                FrontendEventClass::Command,
                PendingPolicy::Always,
                true,
                false,
            );
        }

        assert_policy(
            InputEvent::MouseMove {
                x: 0.0,
                y: 0.0,
                modifiers: Modifiers::none(),
                target_frame_id: 0,
            },
            FrontendEventClass::LispSpecial,
            PendingPolicy::TrackMouse,
            false,
            true,
        );
        assert_policy(
            InputEvent::PixelScroll {
                delta_x: 0.0,
                delta_y: 1.0,
                x: 0.0,
                y: 0.0,
                modifiers: Modifiers::none(),
                target_frame_id: 0,
            },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Always,
            true,
            false,
        );
        assert_policy(
            InputEvent::Resize {
                width: 1,
                height: 1,
                scale_factor: 1.0,
                emacs_frame_id: 0,
            },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Never,
            false,
            true,
        );
        assert_policy(
            InputEvent::DisplayReset,
            FrontendEventClass::LispSpecial,
            PendingPolicy::Never,
            false,
            true,
        );
        assert_policy(
            InputEvent::Focus {
                focused: true,
                emacs_frame_id: 0,
            },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Focus { focused: true },
            false,
            false,
        );
        assert_policy(
            InputEvent::MonitorsChanged { monitors: vec![] },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Filterable("monitors-changed"),
            false,
            true,
        );
        assert_policy(
            InputEvent::SelectWindow {
                window_id: crate::window::WindowId(1),
            },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Filterable("select-window"),
            true,
            false,
        );
        assert_policy(
            InputEvent::WindowClose { emacs_frame_id: 0 },
            FrontendEventClass::LispSpecial,
            PendingPolicy::Always,
            true,
            true,
        );
        assert_policy(
            InputEvent::LayoutInvalidated,
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
        assert_policy(
            InputEvent::ImageStateChanged {
                event: neomacs_display_protocol::ImageStateEvent::Evicted(
                    neomacs_display_protocol::ImageId::new(7),
                ),
            },
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
        assert_policy(
            InputEvent::PresentationRetired { presentation: 1 },
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
    }

    #[test]
    fn presentation_retirement_is_internal_scheduler_noise() {
        let policy = semantics(&InputEvent::PresentationRetired { presentation: 1 });

        assert_eq!(policy.class, FrontendEventClass::Internal);
        assert_eq!(policy.pending, PendingPolicy::Never);
        assert!(!policy.interrupts);
        assert!(!policy.wait_special);
    }

    #[test]
    fn presentation_activation_and_discard_are_internal_service_actions() {
        let mut queue = FrontendEventQueue::default();
        queue.push_back(InputEvent::PresentationActivated {
            presentation: 41,
            emacs_frame_id: 0x1_0000_0000,
        });
        queue.push_back(InputEvent::PresentationDiscarded {
            presentation: 42,
            emacs_frame_id: 0x1_0000_0000,
        });

        for event in [
            InputEvent::PresentationActivated {
                presentation: 41,
                emacs_frame_id: 0x1_0000_0000,
            },
            InputEvent::PresentationDiscarded {
                presentation: 42,
                emacs_frame_id: 0x1_0000_0000,
            },
        ] {
            assert_policy(
                event,
                FrontendEventClass::Internal,
                PendingPolicy::Never,
                false,
                false,
            );
        }

        assert_eq!(
            queue.take_leading_internal(),
            Some(InternalFrontendEvent::PresentationActivated {
                presentation: 41,
                emacs_frame_id: 0x1_0000_0000,
            })
        );
        assert_eq!(
            queue.take_leading_internal(),
            Some(InternalFrontendEvent::PresentationDiscarded {
                presentation: 42,
                emacs_frame_id: 0x1_0000_0000,
            })
        );
        assert!(queue.is_empty());
    }

    #[test]
    fn layout_invalidation_is_internal_with_an_explicit_service_action() {
        let policy = semantics(&InputEvent::LayoutInvalidated);
        let mut queue = FrontendEventQueue::default();
        queue.push_back(InputEvent::LayoutInvalidated);

        assert_eq!(policy.class, FrontendEventClass::Internal);
        assert_eq!(policy.pending, PendingPolicy::Never);
        assert!(!policy.interrupts);
        assert!(!policy.wait_special);
        assert_eq!(
            queue.take_leading_internal(),
            Some(InternalFrontendEvent::LayoutInvalidated)
        );
    }

    #[test]
    fn image_state_change_preserves_identity_and_reason_as_internal_input() {
        let mut queue = FrontendEventQueue::default();
        let event = neomacs_display_protocol::ImageStateEvent::Evicted(
            neomacs_display_protocol::ImageId::new(41),
        );
        queue.push_back(InputEvent::ImageStateChanged { event });

        assert_eq!(
            queue.take_leading_internal(),
            Some(InternalFrontendEvent::ImageStateChanged { event })
        );
    }
}
