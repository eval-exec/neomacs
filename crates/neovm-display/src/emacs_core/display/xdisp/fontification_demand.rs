//! Attempt-local evidence from the canonical fontification walk; never a cache.
use neovm_edit::buffer::{BufferId, CharRange, LispCharPos1};
use std::marker::PhantomData;

const MAX_DEMAND_SPANS: usize = 64;

/// Immutable numeric coverage. Only a completed canonical observation constructs it.
#[derive(Debug)]
pub struct RedisplayFontificationCoverage {
    spans: Vec<(BufferId, CharRange)>,
    _mutator: PhantomData<*const ()>,
}

impl RedisplayFontificationCoverage {
    pub fn spans(&self) -> impl Iterator<Item = (BufferId, CharRange)> + '_ {
        self.spans.iter().copied()
    }

    /// Positive union coverage; gaps and another buffer remain uncovered.
    pub fn covers(&self, buffer: BufferId, range: CharRange) -> bool {
        let mut cursor = range.start();
        while cursor < range.end() {
            let next = self
                .spans
                .iter()
                .filter(|(owner, span)| *owner == buffer && span.start() <= cursor)
                .map(|(_, span)| span.end())
                .max()
                .unwrap_or(cursor);
            if next <= cursor {
                return false;
            }
            cursor = next;
        }
        true
    }
}

/// This is producer evidence only; it grants neither reuse nor lifetime authority.
#[derive(Debug)]
pub enum RedisplayFontificationWitness {
    NoDemand(RedisplayFontificationCoverage),
    CallbacksRan,
    Unknown,
    Aborted,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum DemandState {
    #[default]
    Observing,
    Unknown,
    CallbacksRan,
    Aborted,
}

#[derive(Debug)]
struct ActiveSpan {
    buffer: BufferId,
    start: LispCharPos1,
    end: LispCharPos1,
    cursor: LispCharPos1,
}

/// One attempt's bounded numeric builder. No Lisp values, roots or global state.
#[derive(Debug, Default)]
pub struct RedisplayFontificationDemand {
    state: DemandState,
    spans: Vec<(BufferId, CharRange)>,
    active: Option<ActiveSpan>,
    _mutator: PhantomData<*const ()>,
}

static_assertions::assert_not_impl_any!(RedisplayFontificationDemand: Send, Sync);
static_assertions::assert_not_impl_any!(RedisplayFontificationCoverage: Send, Sync);
static_assertions::assert_not_impl_any!(RedisplayFontificationWitness: Send, Sync);

impl RedisplayFontificationDemand {
    pub fn new() -> Self {
        Self::default()
    }

    /// The owner calls this if its leaf attempt fails, retries or unwinds.
    pub fn abort(&mut self) {
        self.state = DemandState::Aborted;
    }

    pub fn finish(self) -> RedisplayFontificationWitness {
        if self.active.is_some() {
            return RedisplayFontificationWitness::Aborted;
        }
        match self.state {
            DemandState::CallbacksRan => RedisplayFontificationWitness::CallbacksRan,
            DemandState::Unknown => RedisplayFontificationWitness::Unknown,
            DemandState::Aborted => RedisplayFontificationWitness::Aborted,
            DemandState::Observing => {
                if self.spans.is_empty() {
                    RedisplayFontificationWitness::Unknown
                } else {
                    RedisplayFontificationWitness::NoDemand(RedisplayFontificationCoverage {
                        spans: self.spans,
                        _mutator: PhantomData,
                    })
                }
            }
        }
    }
}

pub(super) trait FontificationDemandObserver {
    const ENABLED: bool;
    fn begin(&mut self, _buffer: BufferId, _start: LispCharPos1, _end: LispCharPos1) {}
    fn unknown(&mut self) {}
    fn callback(&mut self) {}
    fn advance(&mut self, _from: LispCharPos1, _to: LispCharPos1, _was_fontified: bool) {}
    fn complete(&mut self) {}
}

pub(super) struct NoFontificationDemand;
impl FontificationDemandObserver for NoFontificationDemand {
    const ENABLED: bool = false;
}

impl FontificationDemandObserver for RedisplayFontificationDemand {
    const ENABLED: bool = true;

    fn begin(&mut self, buffer: BufferId, start: LispCharPos1, end: LispCharPos1) {
        if self.active.is_some() {
            self.abort();
        }
        self.active = Some(ActiveSpan {
            buffer,
            start,
            end,
            cursor: start,
        });
        if start >= end || self.spans.len() >= MAX_DEMAND_SPANS {
            self.unknown();
        }
    }

    fn unknown(&mut self) {
        if self.state == DemandState::Observing {
            self.state = DemandState::Unknown;
        }
    }

    fn callback(&mut self) {
        if self.state != DemandState::Aborted {
            self.state = DemandState::CallbacksRan;
        }
    }

    fn advance(&mut self, from: LispCharPos1, to: LispCharPos1, was_fontified: bool) {
        let valid = self.active.as_ref().is_some_and(|span| {
            span.cursor == from && to > from && to <= span.end && was_fontified
        });
        if !valid {
            self.unknown();
        }
        if let Some(span) = &mut self.active {
            span.cursor = to;
        }
    }

    fn complete(&mut self) {
        let Some(span) = self.active.take() else {
            self.abort();
            return;
        };
        if span.cursor != span.end || span.start >= span.end {
            self.unknown();
            return;
        }
        if self.state == DemandState::Observing {
            self.spans.push((
                span.buffer,
                CharRange::new(span.start.to_char_pos(), span.end.to_char_pos()),
            ));
        }
    }
}
