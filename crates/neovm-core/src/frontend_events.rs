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
        | InputEvent::MousePress { .. }
        | InputEvent::MouseRelease { .. }
        | InputEvent::MouseScroll { .. }
        | InputEvent::PixelScroll { .. }
        | InputEvent::MenuSelection { .. }
        | InputEvent::ToolBarClick { .. }
        | InputEvent::PresentedPointer { .. }
        | InputEvent::MenuBarClick { .. } => Command,
        InputEvent::MouseMove { .. } => MouseMotion,
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
        FrontendEventSemantics::Internal(_)
        | FrontendEventSemantics::MouseMotion
        | FrontendEventSemantics::ServiceDuringWait => false,
    }
}

pub(crate) fn is_wait_special(event: &InputEvent, track_mouse: bool) -> bool {
    match semantics(event) {
        FrontendEventSemantics::Command | FrontendEventSemantics::Internal(_) => false,
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
        FrontendEventSemantics::Internal(_) | FrontendEventSemantics::ServiceDuringWait => false,
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
#[path = "frontend_events/tests/frontend_events_test.rs"]
mod tests;
