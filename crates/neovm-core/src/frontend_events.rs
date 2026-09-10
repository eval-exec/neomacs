//! Semantic boundary for events arriving from the display frontend.
//!
//! `InputEvent` is a transport enum.  Consumers must use this module instead
//! of inferring command-input semantics from the transport variant directly.

use crate::keyboard::{InputEvent, InputPendingFilter};
use std::collections::VecDeque;

/// GNU's readable_events distinguishes a blocked reader from a filtered
/// input-pending-p query. A focus event can be ignored by the latter but must
/// still wake the former so read_char can handle it and advance the FIFO.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrontendInputQuery {
    Readable,
    Pending(InputPendingFilter),
}

impl FrontendInputQuery {
    fn ignores(self, symbol: &str, ignored_while_no_input: &impl Fn(&str) -> bool) -> bool {
        match self {
            Self::Readable => false,
            Self::Pending(filter) => filter.ignores(symbol, ignored_while_no_input),
        }
    }
}

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
        let FrontendEventSemantics::Internal(action) = semantics(self.events.front()?) else {
            return None;
        };
        self.events.pop_front();
        Some(action)
    }

    pub(crate) fn has_input(
        &self,
        query: FrontendInputQuery,
        track_mouse: bool,
        ignored_while_no_input: impl Fn(&str) -> bool,
    ) -> bool {
        self.events
            .iter()
            .any(|event| counts_as_input(event, query, track_mouse, &ignored_while_no_input))
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum InternalFrontendEvent {
    PresentedRegion {
        presentation: u64,
        hit: Option<neomacs_display_protocol::PresentedHit>,
        x: f32,
        y: f32,
        target_frame_id: u64,
    },
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

/// A Lisp-visible event may be filtered, but cannot unconditionally opt out
/// of both command input and wait servicing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PendingInputPolicy {
    Always,
    Focus { focused: bool },
    Filterable(&'static str),
}

/// Scheduling is a choice, not independent flags. In particular, an event
/// that is never command input must carry an internal service action or be
/// serviced during waits. Mouse motion is readable exactly when track-mouse
/// is enabled, and serviced during waits otherwise (GNU some_mouse_moved).
#[derive(Clone, Copy, Debug, PartialEq)]
enum FrontendEventSemantics {
    Command,
    /// Ordered work answered by read_char, without becoming a Lisp command.
    ReadControl,
    Internal(InternalFrontendEvent),
    MouseMotion,
    ServiceDuringWait,
    SpecialInput {
        pending: PendingInputPolicy,
        interrupts: bool,
        service_during_wait: bool,
    },
}

const fn special_input(
    pending: PendingInputPolicy,
    interrupts: bool,
    service_during_wait: bool,
) -> FrontendEventSemantics {
    FrontendEventSemantics::SpecialInput {
        pending,
        interrupts,
        service_during_wait,
    }
}

/// Return the complete semantic policy for an input transport variant.
///
/// This match is deliberately exhaustive: adding a frontend event must force
/// an explicit choice about command visibility and scheduler behavior.
fn semantics(event: &InputEvent) -> FrontendEventSemantics {
    use FrontendEventSemantics::{Command, Internal, MouseMotion, ServiceDuringWait};

    match event {
        InputEvent::RawTtyBytes { .. }
        | InputEvent::TtyByte { .. }
        | InputEvent::TtyCharacter { .. }
        | InputEvent::KeyPress { .. }
        | InputEvent::Ime { .. }
        | InputEvent::MousePress { .. }
        | InputEvent::MouseRelease { .. }
        | InputEvent::MouseScroll { .. }
        | InputEvent::MenuSelection { .. }
        | InputEvent::ToolBarClick { .. }
        | InputEvent::PresentedPointer { .. }
        | InputEvent::MenuBarClick { .. } => Command,
        // Answer ordered control requests at the next input read, after edits.
        InputEvent::ImeRequest(_) => FrontendEventSemantics::ReadControl,
        InputEvent::MouseMove { .. } => MouseMotion,
        InputEvent::PixelScroll { .. } => special_input(PendingInputPolicy::Always, true, false),
        InputEvent::PresentedRegion {
            presentation,
            hit,
            x,
            y,
            target_frame_id,
        } => Internal(InternalFrontendEvent::PresentedRegion {
            presentation: *presentation,
            hit: *hit,
            x: *x,
            y: *y,
            target_frame_id: *target_frame_id,
        }),
        InputEvent::LayoutInvalidated => Internal(InternalFrontendEvent::LayoutInvalidated),
        InputEvent::ImageStateChanged { event } => {
            Internal(InternalFrontendEvent::ImageStateChanged { event: *event })
        }
        InputEvent::PresentationActivated {
            presentation,
            emacs_frame_id,
        } => Internal(InternalFrontendEvent::PresentationActivated {
            presentation: *presentation,
            emacs_frame_id: *emacs_frame_id,
        }),
        InputEvent::PresentationDiscarded {
            presentation,
            emacs_frame_id,
        } => Internal(InternalFrontendEvent::PresentationDiscarded {
            presentation: *presentation,
            emacs_frame_id: *emacs_frame_id,
        }),
        InputEvent::PresentationRetired { presentation } => {
            Internal(InternalFrontendEvent::PresentationRetired {
                presentation: *presentation,
            })
        }
        // Native geometry and host notifications progress without a keystroke.
        InputEvent::Resize { .. }
        | InputEvent::DisplayReset
        | InputEvent::WebView(..)
        | InputEvent::SurfaceCreateFailed { .. }
        | InputEvent::FrameShaderFailed { .. }
        | InputEvent::TerminalCreateFailed { .. }
        | InputEvent::TerminalExited { .. }
        | InputEvent::TerminalTitleChanged { .. }
        | InputEvent::SystemFontsChanged { .. } => ServiceDuringWait,
        InputEvent::Focus { focused, .. } => special_input(
            PendingInputPolicy::Focus { focused: *focused },
            false,
            false,
        ),
        InputEvent::MonitorsChanged { .. } => special_input(
            PendingInputPolicy::Filterable("monitors-changed"),
            false,
            true,
        ),
        InputEvent::SelectWindow { .. } => {
            special_input(PendingInputPolicy::Filterable("select-window"), true, false)
        }
        InputEvent::WindowClose { .. } => special_input(PendingInputPolicy::Always, true, true),
    }
}

pub(crate) fn is_internal(event: &InputEvent) -> bool {
    matches!(semantics(event), FrontendEventSemantics::Internal(_))
}

pub(crate) fn interrupts(event: &InputEvent) -> bool {
    match semantics(event) {
        FrontendEventSemantics::Command => true,
        FrontendEventSemantics::SpecialInput { interrupts, .. } => interrupts,
        FrontendEventSemantics::ReadControl
        | FrontendEventSemantics::Internal(_)
        | FrontendEventSemantics::MouseMotion
        | FrontendEventSemantics::ServiceDuringWait => false,
    }
}

pub(crate) fn is_wait_special(event: &InputEvent, track_mouse: bool) -> bool {
    match semantics(event) {
        FrontendEventSemantics::Command
        | FrontendEventSemantics::ReadControl
        | FrontendEventSemantics::Internal(_) => false,
        FrontendEventSemantics::MouseMotion => !track_mouse,
        FrontendEventSemantics::ServiceDuringWait => true,
        FrontendEventSemantics::SpecialInput {
            service_during_wait,
            ..
        } => service_during_wait,
    }
}

fn counts_as_input(
    event: &InputEvent,
    query: FrontendInputQuery,
    track_mouse: bool,
    ignored_while_no_input: &impl Fn(&str) -> bool,
) -> bool {
    match semantics(event) {
        FrontendEventSemantics::Command => true,
        FrontendEventSemantics::ReadControl
        | FrontendEventSemantics::Internal(_)
        | FrontendEventSemantics::ServiceDuringWait => false,
        FrontendEventSemantics::MouseMotion => track_mouse,
        FrontendEventSemantics::SpecialInput { pending, .. } => match pending {
            PendingInputPolicy::Always => true,
            PendingInputPolicy::Focus { focused } => !query.ignores(
                if focused { "focus-in" } else { "focus-out" },
                ignored_while_no_input,
            ),
            PendingInputPolicy::Filterable(symbol) => {
                !query.ignores(symbol, ignored_while_no_input)
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Behavior expectations, independent of the production scheduling enum.
    #[derive(Debug, PartialEq, Eq)]
    enum FrontendEventClass {
        Command,
        ReadControl,
        LispSpecial,
        Internal,
    }

    #[derive(Clone, Copy)]
    enum PendingPolicy {
        Always,
        Never,
        TrackMouse,
        Focus { focused: bool },
        Filterable(&'static str),
    }

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
        expected_interrupts: bool,
        wait_special: bool,
    ) {
        let actual_class = match semantics(&event) {
            FrontendEventSemantics::Command => FrontendEventClass::Command,
            FrontendEventSemantics::ReadControl => FrontendEventClass::ReadControl,
            FrontendEventSemantics::Internal(_) => FrontendEventClass::Internal,
            FrontendEventSemantics::MouseMotion
            | FrontendEventSemantics::ServiceDuringWait
            | FrontendEventSemantics::SpecialInput { .. } => FrontendEventClass::LispSpecial,
        };
        assert_eq!(actual_class, class, "class for {event:?}");
        assert_eq!(
            interrupts(&event),
            expected_interrupts,
            "interrupt policy for {event:?}"
        );
        for track_mouse in [false, true] {
            let (expected_pending, ignored_symbol) = match pending {
                PendingPolicy::Always => (true, None),
                PendingPolicy::Never => (false, None),
                PendingPolicy::TrackMouse => (track_mouse, None),
                PendingPolicy::Focus { focused } => {
                    (true, Some(if focused { "focus-in" } else { "focus-out" }))
                }
                PendingPolicy::Filterable(symbol) => (true, Some(symbol)),
            };
            assert_eq!(
                counts_as_input(
                    &event,
                    FrontendInputQuery::Pending(InputPendingFilter::ConfiguredIgnoreList),
                    track_mouse,
                    &|_| false
                ),
                expected_pending,
                "pending policy for {event:?}, track-mouse={track_mouse}"
            );
            assert_eq!(
                counts_as_input(
                    &event,
                    FrontendInputQuery::Pending(InputPendingFilter::ConfiguredIgnoreList),
                    track_mouse,
                    &|symbol| Some(symbol) == ignored_symbol
                ),
                expected_pending && ignored_symbol.is_none(),
                "filtered pending policy for {event:?}, track-mouse={track_mouse}"
            );
            assert_eq!(
                counts_as_input(&event, FrontendInputQuery::Readable, track_mouse, &|_| true),
                expected_pending,
                "a reader must not apply input-pending-p filters for {event:?}"
            );
            assert_eq!(
                is_wait_special(&event, track_mouse),
                wait_special && !(matches!(pending, PendingPolicy::TrackMouse) && track_mouse),
                "wait policy for {event:?}, track-mouse={track_mouse}"
            );
        }
    }

    #[test]
    fn every_transport_variant_has_locked_down_semantics() {
        use crate::keyboard::{KeyEvent, Modifiers, MouseButton};

        let command_events = [
            InputEvent::raw_tty_bytes(vec![0x1b], 0),
            InputEvent::TtyByte {
                byte: b'k',
                target: crate::keyboard::TtyInputTarget::SelectedFrame,
            },
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
            InputEvent::MenuSelection {
                index: 0,
                token: None,
            },
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
                request_id: None,
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
            InputEvent::PresentedRegion {
                presentation: 1,
                hit: None,
                x: 22.0,
                y: 12.0,
                target_frame_id: 0,
            },
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
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
        assert_policy(
            InputEvent::PresentationRetired { presentation: 1 },
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
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
        let mut queue = FrontendEventQueue::default();
        queue.push_back(InputEvent::LayoutInvalidated);

        assert_policy(
            InputEvent::LayoutInvalidated,
            FrontendEventClass::Internal,
            PendingPolicy::Never,
            false,
            false,
        );
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
