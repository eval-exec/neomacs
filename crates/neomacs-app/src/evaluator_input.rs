//! Translation from host-neutral frontend observations to evaluator input.

use neovm_core::keyboard::{self, InputEvent};

use crate::frontend_event::FrontendEvent;

impl EvaluatorInputBatch<'static> {
    pub fn from_positioned_pointer(
        input: neomacs_display_protocol::PositionedPointerInput,
    ) -> Self {
        use neomacs_display_protocol::{PointerAction, PointerTarget, ScrollDelta};
        use neovm_core::keyboard::{InputEvent as KbInputEvent, MouseButton};
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
}

/// Allocation-free, lazily expanded evaluator input produced by one host event.
///
/// Ordinary events stay inline. A committed IME string borrows the frontend
/// event and produces key events one character at a time, avoiding a second
/// string allocation and preserving commit order.
#[derive(Debug)]
#[must_use]
pub struct EvaluatorInputBatch<'a> {
    inner: EvaluatorInputBatchInner<'a>,
}

#[derive(Debug)]
// The inline variant deliberately owns at most two events so the hot input
// path does not allocate. Its size is bounded below with a compile-time check.
#[allow(clippy::large_enum_variant)]
enum EvaluatorInputBatchInner<'a> {
    Inline(InlineInputEvents),
    CommittedText {
        chars: std::str::Chars<'a>,
        modifiers: u32,
        target_frame_id: u64,
    },
}

#[derive(Debug)]
struct InlineInputEvents {
    events: [Option<InputEvent>; 2],
}

impl Iterator for InlineInputEvents {
    type Item = InputEvent;

    fn next(&mut self) -> Option<Self::Item> {
        self.events[0].take().or_else(|| self.events[1].take())
    }
}

// Prevent future InputEvent growth from silently turning every translated
// host observation into a disproportionately large stack value.
const _: () = assert!(std::mem::size_of::<EvaluatorInputBatch<'static>>() <= 384);

impl<'a> EvaluatorInputBatch<'a> {
    /// Decorate the final action of an inline input batch, after its observations.
    pub fn map_inline_action(mut self, decorate: impl FnOnce(InputEvent) -> InputEvent) -> Self {
        if let EvaluatorInputBatchInner::Inline(events) = &mut self.inner
            && let Some(action) = events.events.iter_mut().rev().find(|event| event.is_some()) {
            *action = Some(decorate(action.take().unwrap()));
        }
        self
    }

    /// Translate one host-neutral frontend event.
    pub fn from_frontend_event(event: &'a FrontendEvent) -> Self {
        match event {
            FrontendEvent::Key(key) => {
                Self::from_optional(keyboard::render_key_transport_to_input_event(
                    keyboard::FrontendKey::Keysym(key.symbol().get()),
                    key.modifiers().bits(),
                    key.state().is_pressed(),
                    key.target().get(),
                ))
            }
            FrontendEvent::TextCommitted {
                text,
                modifiers,
                target,
            } => Self {
                inner: EvaluatorInputBatchInner::CommittedText {
                    chars: text.chars(),
                    modifiers: modifiers.bits(),
                    target_frame_id: target.get(),
                },
            },
            FrontendEvent::ViewportChanged(viewport) => {
                let extent = viewport.logical_extent();
                Self::single(InputEvent::Resize {
                    width: extent.width(),
                    height: extent.height(),
                    scale_factor: viewport.scale().get(),
                    emacs_frame_id: viewport.target().get(),
                })
            }
            FrontendEvent::TerminalViewportChanged(viewport) => {
                let extent = viewport.extent();
                Self::single(InputEvent::Resize {
                    width: extent.columns(),
                    height: extent.rows(),
                    scale_factor: 1.0,
                    emacs_frame_id: viewport.target().get(),
                })
            }
            FrontendEvent::CloseRequested { target } => Self::single(InputEvent::WindowClose {
                emacs_frame_id: target.get(),
            }),
            FrontendEvent::FocusChanged { focused, target } => Self::single(InputEvent::Focus {
                focused: *focused,
                emacs_frame_id: target.get(),
            }),
            FrontendEvent::PresentationActivated {
                presentation,
                target,
            } => Self::single(InputEvent::PresentationActivated {
                presentation: presentation.get(),
                emacs_frame_id: target.get(),
            }),
            FrontendEvent::PresentationDiscarded {
                presentation,
                target,
            } => Self::single(InputEvent::PresentationDiscarded {
                presentation: presentation.get(),
                emacs_frame_id: target.get(),
            }),
            FrontendEvent::PresentationRetired { presentation } => {
                Self::single(InputEvent::PresentationRetired {
                    presentation: presentation.get(),
                })
            }
        }
    }

    /// An empty batch for a host observation with no evaluator meaning.
    pub fn empty() -> Self {
        Self::inline([None, None])
    }

    /// A batch containing exactly one evaluator event.
    pub fn single(event: InputEvent) -> Self {
        Self::inline([Some(event), None])
    }

    /// A zero-or-one event batch.
    pub fn from_optional(event: Option<InputEvent>) -> Self {
        event.map_or_else(Self::empty, Self::single)
    }

    /// An optional observation followed by its required action.
    pub fn ordered(observation: Option<InputEvent>, action: InputEvent) -> Self {
        Self::inline([observation, Some(action)])
    }

    fn inline(events: [Option<InputEvent>; 2]) -> Self {
        Self {
            inner: EvaluatorInputBatchInner::Inline(InlineInputEvents { events }),
        }
    }
}

impl Iterator for EvaluatorInputBatch<'_> {
    type Item = InputEvent;

    fn next(&mut self) -> Option<Self::Item> {
        match &mut self.inner {
            EvaluatorInputBatchInner::Inline(events) => events.next(),
            EvaluatorInputBatchInner::CommittedText {
                chars,
                modifiers,
                target_frame_id,
            } => chars.find_map(|ch| {
                // Text identity must survive transport (#458): committed
                // characters classify as characters, never as numeric keysyms.
                // NUL is not text (terminal `Ctrl-2`), matching the frontend
                // filters.
                if ch == '\0' {
                    return None;
                }
                keyboard::render_key_transport_to_input_event(
                    keyboard::FrontendKey::Character(ch),
                    *modifiers,
                    true,
                    *target_frame_id,
                )
            }),
        }
    }
}
