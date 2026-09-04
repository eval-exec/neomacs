//! Translation from host-neutral frontend observations to evaluator input.

use neovm_core::keyboard::{self, InputEvent};

use crate::frontend_event::FrontendEvent;

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
    /// Translate one host-neutral frontend event.
    pub fn from_frontend_event(event: &'a FrontendEvent) -> Self {
        match event {
            FrontendEvent::Key(key) => {
                Self::from_optional(keyboard::render_key_transport_to_input_event(
                    key.symbol().get(),
                    key.modifiers().bits(),
                    key.state().is_pressed(),
                    key.target().get(),
                ))
            }
            FrontendEvent::TextCommitted { text, target } => Self {
                inner: EvaluatorInputBatchInner::CommittedText {
                    chars: text.chars(),
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
                target_frame_id,
            } => chars.find_map(|ch| {
                keyboard::render_key_transport_to_input_event(ch as u32, 0, true, *target_frame_id)
            }),
        }
    }
}
