//! Cross-thread tooltip intent and visibility. No synchronous GUI reply is needed.
use std::sync::{
    Arc,
    atomic::{AtomicU8, AtomicU64, Ordering},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TooltipGeneration(u64);
impl TooltipGeneration {
    pub fn from_raw(value: u64) -> Self {
        Self(value)
    }
    pub fn raw(self) -> u64 {
        self.0
    }
}

#[derive(Default, Debug)]
pub struct TooltipContext(AtomicU64);

impl TooltipContext {
    pub fn generation(&self) -> TooltipGeneration {
        TooltipGeneration(self.0.load(Ordering::Acquire))
    }
    pub fn invalidate(&self) {
        self.0.fetch_add(1, Ordering::AcqRel);
    }
}

#[derive(Clone, Copy, Debug, strum::FromRepr)]
#[repr(u8)]
enum Visibility {
    Ready,
    Visible,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct TooltipTicket {
    generation: TooltipGeneration,
    context: Arc<TooltipContext>,
    visibility: Arc<AtomicU8>,
}

impl TooltipTicket {
    pub fn is_current(&self) -> bool {
        self.context.generation() == self.generation
            && self.visibility.load(Ordering::Acquire) != Visibility::Cancelled as u8
    }
    pub fn same_request(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.visibility, &other.visibility)
    }
    pub fn mark_visible(&self) -> bool {
        self.is_current()
            && self
                .visibility
                .compare_exchange(
                    Visibility::Ready as u8,
                    Visibility::Visible as u8,
                    Ordering::AcqRel,
                    Ordering::Acquire,
                )
                .is_ok()
    }
    pub fn cancel(&self) -> bool {
        matches!(
            Visibility::from_repr(
                self.visibility
                    .swap(Visibility::Cancelled as u8, Ordering::AcqRel)
            ),
            Some(Visibility::Visible)
        )
    }
}

/// Evaluator-side intent owner. Capture before Lisp schedules delayed help.
#[derive(Default)]
pub struct TooltipClient {
    context: Arc<TooltipContext>,
    current: Option<TooltipTicket>,
}

impl TooltipClient {
    pub fn new(context: Arc<TooltipContext>) -> Self {
        Self {
            context,
            ..Self::default()
        }
    }
    pub fn generation(&self) -> TooltipGeneration {
        self.context.generation()
    }
    pub fn present(&mut self, generation: Option<TooltipGeneration>) -> TooltipTicket {
        let ticket = TooltipTicket {
            generation: generation.unwrap_or_else(|| self.context.generation()),
            context: self.context.clone(),
            visibility: Arc::new(AtomicU8::new(Visibility::Ready as u8)),
        };
        if !ticket.is_current() {
            ticket.cancel();
            return ticket;
        }
        if let Some(previous) = self.current.take() {
            previous.cancel();
        }
        self.current = Some(ticket.clone());
        ticket
    }
    pub fn dismiss(&mut self) -> Option<(TooltipTicket, bool)> {
        self.current.take().map(|ticket| {
            let visible = ticket.cancel();
            (ticket, visible)
        })
    }
}

#[cfg(test)]
#[path = "lifetime_test.rs"]
mod tests;
