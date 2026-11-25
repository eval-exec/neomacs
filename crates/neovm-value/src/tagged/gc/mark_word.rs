//! Collector work words: Lisp values as mark queues carry them.
//!
//! A raw `TaggedValue` is confined to its mutator thread. The concurrent
//! marker is not a mutator; it receives gray and SATB work under the
//! collection protocol, where objects stay alive through the cycle's roots,
//! marks and barriers rather than through the queued words themselves. These
//! types carry that work across the thread boundary and turn back into values
//! only inside the collector.

use crate::tagged::value::TaggedValue;

/// One value word queued for the collector.
#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub(super) struct MarkWord(usize);

static_assertions::assert_impl_all!(MarkWord: Send, Sync, Copy, std::fmt::Debug);
static_assertions::assert_eq_size!(MarkWord, TaggedValue);

impl MarkWord {
    /// Immediate `nil` mark word. `nil` has no heap backing or owner lifetime,
    /// so creating this word requires no `unsafe`.
    #[inline(always)]
    pub(super) fn nil() -> Self {
        Self(TaggedValue::NIL.bits())
    }

    /// Wrap a `!Send` [`TaggedValue`] into a `Send + Sync + Copy` collector
    /// work word.
    ///
    /// # Safety
    /// Detaching a heap-backed [`TaggedValue`] into a `Send` word erases its
    /// thread-confinement and lifetime bounds. The caller must ensure this
    /// word is only stored in or transferred through the originating heap's
    /// active GC or owner-registration protocol, never escapes to a foreign
    /// heap, and is never materialized via [`Self::value`] after its target
    /// object can be reclaimed.
    #[inline(always)]
    pub(super) unsafe fn of(value: TaggedValue) -> Self {
        Self(value.bits())
    }

    /// Wrap an immediate (`nil`, `t`, or fixnum) into a [`MarkWord`].
    ///
    /// Immediates carry no heap pointer or reclamation lifetime, so this
    /// constructor is safe and returns `None` for any heap-backed value.
    #[cfg(any(test, feature = "test-support"))]
    #[inline(always)]
    pub(super) fn from_immediate(value: TaggedValue) -> Option<Self> {
        if TaggedValue::try_immediate_from_bits(value.bits()).is_some() {
            Some(Self(value.bits()))
        } else {
            None
        }
    }

    /// Raw tagged bit pattern of this queued word.
    ///
    /// Returning a `usize` does not materialize a `!Send` [`TaggedValue`] or
    /// grant dereference authority across threads.
    #[inline(always)]
    pub(super) const fn bits(self) -> usize {
        self.0
    }

    /// Re-materialize the queued word as a `!Send` [`TaggedValue`].
    ///
    /// # Safety
    /// The caller must hold collector/owner authority for the heap that minted
    /// this word: either running on the concurrent marker thread inside an
    /// active collection cycle where the target object stays live through
    /// start-of-cycle roots, marks, and barriers, or on a world-stopped
    /// collector / the owning mutator thread before any sweep can reclaim the
    /// target. The returned [`TaggedValue`] must remain confined to that
    /// thread and must not outlive the active cycle or stop boundary.
    #[inline(always)]
    pub(super) unsafe fn value(self) -> TaggedValue {
        // SAFETY: `MarkWord` is only constructed from a real `TaggedValue` via
        // `MarkWord::of` / `MarkWord::NIL` / `MarkWord::from_immediate`, and
        // the caller upholds the thread/liveness contract.
        unsafe { TaggedValue::from_bits(self.0) }
    }
}

impl std::fmt::Debug for MarkWord {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "MarkWord({:#x})", self.0)
    }
}

/// The marker thread's own gray stack.
#[derive(Default)]
pub(super) struct MarkStack(Vec<MarkWord>);

static_assertions::assert_impl_all!(MarkStack: Send, std::fmt::Debug);

impl MarkStack {
    /// Take over a world-stopped gray queue at the start handshake.
    ///
    /// # Safety
    /// Same heap-confinement and liveness contract as [`MarkWord::of`] for
    /// every element in `values`: `MarkStack` is `Send`, so the caller must
    /// ensure these words stay bound to their originating heap's active mark
    /// cycle and are never materialized after reclamation.
    pub(super) unsafe fn from_values(values: Vec<TaggedValue>) -> Self {
        // SAFETY: upheld by caller contract.
        Self(
            values
                .into_iter()
                .map(|value| unsafe { MarkWord::of(value) })
                .collect(),
        )
    }

    /// Push a [`TaggedValue`] onto this `Send` mark stack.
    ///
    /// # Safety
    /// Same heap-confinement and liveness contract as [`MarkWord::of`].
    #[inline(always)]
    pub(super) unsafe fn push(&mut self, value: TaggedValue) {
        // SAFETY: upheld by caller contract.
        self.0.push(unsafe { MarkWord::of(value) });
    }

    /// Pop the next queued work item as a [`TaggedValue`].
    ///
    /// # Safety
    /// Same collector/owner-thread and live-cycle contract as
    /// [`MarkWord::value`]: `MarkStack` is `Send`, so popping a `!Send`
    /// `TaggedValue` requires the caller to hold collector or owning-mutator
    /// authority for the heap whose words were pushed.
    #[inline(always)]
    pub(super) unsafe fn pop(&mut self) -> Option<TaggedValue> {
        // SAFETY: upheld by caller contract.
        self.0.pop().map(|word| unsafe { word.value() })
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
    ///
    /// # Safety
    /// Same collector/owner-thread and live-cycle contract as
    /// [`MarkWord::value`].
    #[cfg(any(test, feature = "test-support"))]
    pub(super) unsafe fn into_values(self) -> Vec<TaggedValue> {
        // SAFETY: upheld by caller contract.
        self.0
            .into_iter()
            .map(|word| unsafe { word.value() })
            .collect()
    }
}

impl std::fmt::Debug for MarkStack {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MarkStack")
            .field("len", &self.0.len())
            .finish()
    }
}
