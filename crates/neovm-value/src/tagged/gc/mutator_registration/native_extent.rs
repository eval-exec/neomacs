//! The explicit native-safe extent at the equivalent of GNU
//! release-the-global-lock points (Green mutex/condition waits and select;
//! design section 4.5).
//!
//! One composed wrapper runs the whole contract. Before the body can block,
//! the owner's run, heap and manager loans have ended and its regions closed
//! (the publication refuses any live loan), its native arguments/results are
//! rooted, an immutable root record plus GC image is published, and
//! `NativeStopped` is committed under the control mutex — validated against a
//! request that is already active. There is no state where "about to block"
//! counts as stopped with missing roots: the mode transition follows the
//! completed publication.
//!
//! The body receives only [`NativeRoots`]: retained root handles plus
//! OS/native data. No heap, slice, run, World, allocation, callback, or
//! publication-release capability is reachable from that type, and a root
//! keeps its object alive without granting dereference authority. After the
//! body returns, and before any heap/TLS/Lisp access, the wrapper takes the
//! control mutex: an active stop keeps the owner `NativeStopped` at its
//! predicate; otherwise the owner passes through `Resuming`, reserving resume
//! debt and taking the latest entry-owned image exactly once.
//!
//! A blocked owner owes no resume debt, so the keeper can collect repeatedly
//! from its retained roots and keeper-updated stopped image while the
//! syscall never returns. A panic inside the body unwinds with the owner
//! still `NativeStopped` and its publication retained; the owner token's
//! Drop then poisons the era rather than resuming half-native state.

use std::marker::PhantomData;
use std::rc::Rc;
use std::time::{Duration, Instant};

use super::{HeapIdentity, Mutator, RendezvousError};
use crate::tagged::transport::SharedRoot;

/// Entry/return lock contention sits far below this bound; a Busy that
/// survives it reports contract misuse instead of hanging the owner.
const CONTENTION_WAIT: Duration = Duration::from_secs(60);

/// The retained root handles a native-safe extent may use.
///
/// Words only. A word identifies a root for native code (an FFI cookie, a
/// result slot key); it is not a Lisp value on this side of the extent and
/// grants no dereference, allocation, callback, or manager authority. The
/// handles are the words of exactly the immutable publication that covers the
/// blocked owner, and they stay identical for the whole extent.
pub struct NativeRoots<'extent> {
    heap: HeapIdentity,
    words: &'extent [usize],
    _thread: PhantomData<Rc<()>>,
}

impl std::fmt::Debug for NativeRoots<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeRoots")
            .field("heap", &self.heap)
            .field("len", &self.words.len())
            .finish_non_exhaustive()
    }
}

impl<'extent> NativeRoots<'extent> {
    /// The heap whose roots these handles retain. Diagnostic identity; it
    /// opens no heap access.
    pub fn heap_identity(&self) -> HeapIdentity {
        self.heap
    }
    pub fn len(&self) -> usize {
        self.words.len()
    }
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
    /// One retained handle's word.
    ///
    /// # Panics
    /// `index` is not below [`Self::len`].
    pub fn word(&self, index: usize) -> usize {
        self.words[index]
    }
}

/// The native body's result together with the request epoch of the GC image
/// its return applied. An epoch of zero means no collection covered the
/// extent: the owner left through the same image it entered with.
pub struct NativeReturn<R> {
    pub result: R,
    pub epoch: u64,
}

impl<R> std::fmt::Debug for NativeReturn<R>
where
    R: std::fmt::Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NativeReturn")
            .field("result", &self.result)
            .field("epoch", &self.epoch)
            .finish()
    }
}

impl<'era> Mutator<'era> {
    /// Enter one native-safe extent around a potentially blocking native
    /// call, on this owner's thread.
    ///
    /// `roots` are the native arguments/results that must survive collections
    /// while the owner is blocked; `body` may block on OS primitives (a
    /// latch, pipe, or condvar wait) and use only those roots' handles. The
    /// return applies the latest entry-owned GC image and restores `Running`
    /// before handing the result back; no heap, TLS, or Lisp access may
    /// intervene between the body's return and this wrapper's.
    ///
    /// `Busy` from control/entry contention — or from an active stop the
    /// return must wait out — is retried within a fixed bound; every other
    /// refusal is returned unchanged. The caller must not be the keeper
    /// thread, which cannot wait for its own epoch.
    pub fn with_native_extent<R>(
        &mut self,
        roots: &[SharedRoot],
        body: impl FnOnce(&NativeRoots<'_>) -> R,
    ) -> Result<NativeReturn<R>, RendezvousError> {
        // Snapshot the handles before entry: the immutable publication keeps
        // exactly these words for the whole extent.
        let mut handles = Vec::with_capacity(roots.len());
        for root in roots {
            handles.push(root.parallel_word());
        }
        let heap = self.key().heap_identity();
        // Entry Busy is control/entry contention: the publication commits
        // NativeStopped only after its roots and image are complete, so a
        // retry never widens "about to block" into stopped-with-missing-roots.
        let deadline = Instant::now() + CONTENTION_WAIT;
        loop {
            match self.enter_native(roots) {
                Ok(()) => break,
                Err(RendezvousError::Busy) if Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        }
        #[cfg(test)]
        let native_seq = super::a8_probes::native_seq();
        let result = body(&NativeRoots {
            heap,
            words: &handles,
            _thread: PhantomData,
        });
        #[cfg(test)]
        super::a8_probes::mark(
            super::a8_probes::NATIVE_RETURNED,
            native_seq,
            self.key().owner(),
        );
        // Syscall return: this gate-checked transition is the first protocol
        // step after the body, and an active stop keeps the owner
        // NativeStopped at its predicate until the keeper completes.
        let deadline = Instant::now() + CONTENTION_WAIT;
        let epoch = loop {
            match self.leave_native() {
                Ok(epoch) => break epoch,
                Err(RendezvousError::Busy) if Instant::now() < deadline => {
                    std::thread::yield_now();
                }
                Err(error) => return Err(error),
            }
        };
        #[cfg(test)]
        super::a8_probes::mark(
            super::a8_probes::NATIVE_PERMITTED,
            native_seq,
            self.key().owner(),
        );
        Ok(NativeReturn { result, epoch })
    }
}

static_assertions::assert_not_impl_any!(NativeRoots<'static>: Send, Sync, Clone, Copy);
