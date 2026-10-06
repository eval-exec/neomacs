//! Collector work words: Lisp values as mark queues carry them.
//!
//! A raw `TaggedValue` is confined to its mutator thread. The concurrent
//! marker is not a mutator; it receives gray and SATB work under the
//! collection protocol, where objects stay alive through the cycle's roots,
//! marks and barriers rather than through the queued words themselves. These
//! types carry that work across the thread boundary and turn back into values
//! only inside the collector.

use std::sync::{Arc, Mutex};

use crate::tagged::value::TaggedValue;

/// One value word queued for the collector.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct MarkWord(usize);

static_assertions::assert_impl_all!(MarkWord: Send, Sync, Copy);
static_assertions::assert_eq_size!(MarkWord, TaggedValue);

impl MarkWord {
    #[inline(always)]
    pub(super) fn of(value: TaggedValue) -> Self {
        Self(value.bits())
    }

    /// The value, for collector code on the marker or on a world-stopped
    /// mutator.
    #[inline(always)]
    pub(super) fn value(self) -> TaggedValue {
        TaggedValue::from_bits(self.0)
    }
}

impl std::fmt::Debug for MarkWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MarkWord({:#x})", self.0)
    }
}

/// A queue shared between the mutator's SATB barrier or start handshake and
/// the marker thread.
pub(super) type SharedMarkQueue = Arc<Mutex<Vec<MarkWord>>>;

/// The marker thread's own gray stack.
#[derive(Default)]
pub(super) struct MarkStack(Vec<MarkWord>);

static_assertions::assert_impl_all!(MarkStack: Send);

impl MarkStack {
    /// Take over a world-stopped gray queue at the start handshake.
    pub(super) fn from_values(values: Vec<TaggedValue>) -> Self {
        Self(values.into_iter().map(MarkWord::of).collect())
    }

    #[inline(always)]
    pub(super) fn push(&mut self, value: TaggedValue) {
        self.0.push(MarkWord::of(value));
    }

    #[inline(always)]
    pub(super) fn pop(&mut self) -> Option<TaggedValue> {
        self.0.pop().map(MarkWord::value)
    }

    #[inline]
    pub(super) fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Append a batch drained from a shared queue.
    pub(super) fn extend_words(&mut self, words: Vec<MarkWord>) {
        self.0.extend(words);
    }

    /// Hand every remaining word to a shared queue.
    pub(super) fn drain_words(&mut self) -> std::vec::Drain<'_, MarkWord> {
        self.0.drain(..)
    }

    /// The remaining work as values, for collector tests.
    #[cfg(test)]
    pub(super) fn into_values(self) -> Vec<TaggedValue> {
        self.0.into_iter().map(MarkWord::value).collect()
    }
}

impl std::fmt::Debug for MarkStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkStack")
            .field("len", &self.0.len())
            .finish()
    }
}
