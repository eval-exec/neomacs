//! One-word owning carrier for internal evaluator control flow.
//!
//! Payload boxes and their Context-owned GC pins keep their existing lifetime.
//! The carrier stays local to a mutator's Rust call stack (`!Send` and `!Sync`);
//! independent mutators use independent carriers and owning Context registries.

use std::fmt;
use std::marker::PhantomData;
use std::mem::{ManuallyDrop, align_of};
use std::num::NonZeroUsize;
use std::ptr::NonNull;

use super::{FlowKind, FlowRef, SignalData, ThreadBlockedData, ThrowData};
use crate::emacs_core::eval::ShutdownRequest;

/// Low two bits of the word.  Payload boxes are 8-aligned (asserted below),
/// so the bits are free; `Signal` is 0 so its pointer needs no masking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(usize)]
enum FlowTag {
    Signal = 0,
    Throw = 1,
    ThreadBlocked = 2,
    Shutdown = 3,
}
const TAG_MASK: usize = 3;
const SHUTDOWN_RESTART_BIT: usize = 1 << 2;
const SHUTDOWN_CODE_SHIFT: u32 = 32;

const _: () = assert!(usize::BITS == 64);
const _: () = assert!(align_of::<SignalData>() > TAG_MASK);
const _: () = assert!(align_of::<ThrowData>() > TAG_MASK);
const _: () = assert!(align_of::<ThreadBlockedData>() > TAG_MASK);

/// One machine word, so `Result<Value, Flow>` is a ScalarPair (rax:rdx).
/// Owns its payload box; `!Send`/`!Sync` like the enum carrier. Its payload's
/// pin keeps roots in the owning Context registry, independent of activation.
///
/// ```compile_fail
/// fn require_send<T: Send>() {}
/// require_send::<neovm_core::emacs_core::error::Flow>();
/// ```
#[repr(transparent)]
pub struct Flow {
    word: NonNull<()>,
    _not_send: PhantomData<*const ()>,
}

impl Flow {
    #[inline]
    fn tag(&self) -> FlowTag {
        match self.word.addr().get() & TAG_MASK {
            0 => FlowTag::Signal,
            1 => FlowTag::Throw,
            2 => FlowTag::ThreadBlocked,
            _ => FlowTag::Shutdown,
        }
    }
    #[inline]
    fn untagged<T>(&self) -> *mut T {
        self.word.as_ptr().map_addr(|a| a & !TAG_MASK).cast::<T>()
    }
    #[inline]
    fn from_box<T>(b: Box<T>, tag: FlowTag) -> Flow {
        let p = NonNull::from(Box::leak(b)).cast::<()>();
        debug_assert_eq!(p.addr().get() & TAG_MASK, 0);
        Flow {
            word: p.map_addr(|a| a | tag as usize),
            _not_send: PhantomData,
        }
    }

    #[inline]
    pub fn from_kind(kind: FlowKind) -> Flow {
        match kind {
            FlowKind::Signal(b) => Flow::from_box(b, FlowTag::Signal),
            FlowKind::Throw(b) => Flow::from_box(b, FlowTag::Throw),
            FlowKind::ThreadBlocked(b) => Flow::from_box(b, FlowTag::ThreadBlocked),
            FlowKind::Shutdown(r) => {
                let bits = ((r.exit_code as u32 as usize) << SHUTDOWN_CODE_SHIFT)
                    | if r.restart { SHUTDOWN_RESTART_BIT } else { 0 }
                    | FlowTag::Shutdown as usize;
                // SAFETY-free: an immediate, never dereferenced.
                let word = NonNull::without_provenance(NonZeroUsize::new(bits).unwrap());
                Flow {
                    word,
                    _not_send: PhantomData,
                }
            }
        }
    }

    #[inline]
    pub fn into_kind(self) -> FlowKind {
        let this = ManuallyDrop::new(self);
        // SAFETY: the word was built by `from_kind` from a leaked box of the
        // type its tag names, and `this` is never dropped, so ownership moves
        // back into exactly one `Box`.
        unsafe {
            match this.tag() {
                FlowTag::Signal => {
                    FlowKind::Signal(Box::from_raw(this.word.as_ptr().cast::<SignalData>()))
                }
                FlowTag::Throw => FlowKind::Throw(Box::from_raw(this.untagged())),
                FlowTag::ThreadBlocked => FlowKind::ThreadBlocked(Box::from_raw(this.untagged())),
                FlowTag::Shutdown => FlowKind::Shutdown(this.shutdown_unchecked()),
            }
        }
    }

    #[inline]
    fn shutdown_unchecked(&self) -> ShutdownRequest {
        let bits = self.word.addr().get();
        ShutdownRequest {
            exit_code: (bits >> SHUTDOWN_CODE_SHIFT) as u32 as i32,
            restart: bits & SHUTDOWN_RESTART_BIT != 0,
        }
    }

    #[inline]
    pub fn kind(&self) -> FlowRef<'_> {
        // SAFETY: as in `into_kind`; the borrow is tied to `&self`.
        unsafe {
            match self.tag() {
                FlowTag::Signal => FlowRef::Signal(&*self.untagged()),
                FlowTag::Throw => FlowRef::Throw(&*self.untagged()),
                FlowTag::ThreadBlocked => FlowRef::ThreadBlocked(&*self.untagged()),
                FlowTag::Shutdown => FlowRef::Shutdown(self.shutdown_unchecked()),
            }
        }
    }

    #[inline]
    pub fn is_signal(&self) -> bool {
        self.tag() == FlowTag::Signal
    }
    #[inline]
    pub fn as_signal(&self) -> Option<&SignalData> {
        // SAFETY: tag Signal => the word is an unmasked Box<SignalData>.
        self.is_signal()
            .then(|| unsafe { &*self.word.as_ptr().cast::<SignalData>() })
    }
    #[inline]
    pub fn as_signal_mut(&mut self) -> Option<&mut SignalData> {
        // SAFETY: as `as_signal`, with `&mut self` giving unique access.
        self.is_signal()
            .then(|| unsafe { &mut *self.word.as_ptr().cast::<SignalData>() })
    }
    #[inline]
    pub fn signal_boxed(data: Box<SignalData>) -> Flow {
        Self::from_box(data, FlowTag::Signal)
    }
    #[inline]
    pub fn shutdown(request: ShutdownRequest) -> Flow {
        Self::from_kind(FlowKind::Shutdown(request))
    }
    #[inline]
    pub fn is_throw(&self) -> bool {
        self.tag() == FlowTag::Throw
    }
    #[inline]
    pub fn is_thread_blocked(&self) -> bool {
        self.tag() == FlowTag::ThreadBlocked
    }
    #[inline]
    pub fn as_thread_blocked(&self) -> Option<&ThreadBlockedData> {
        // SAFETY: this tag names the live ThreadBlockedData box; the borrow
        // cannot outlive the carrier, which owns that allocation.
        self.is_thread_blocked()
            .then(|| unsafe { &*self.untagged::<ThreadBlockedData>() })
    }
    #[inline]
    pub fn is_shutdown(&self) -> bool {
        self.tag() == FlowTag::Shutdown
    }
    #[inline]
    pub fn as_throw(&self) -> Option<&ThrowData> {
        // SAFETY: tag Throw => a Box<ThrowData> with tag bit 1 set.
        (self.tag() == FlowTag::Throw).then(|| unsafe { &*self.untagged::<ThrowData>() })
    }
    #[inline]
    pub fn shutdown_request(&self) -> Option<ShutdownRequest> {
        (self.tag() == FlowTag::Shutdown).then(|| self.shutdown_unchecked())
    }
}

impl Drop for Flow {
    #[inline]
    fn drop(&mut self) {
        // SAFETY: `self` is being dropped; the read moves ownership into a
        // temporary whose `into_kind` frees the box exactly once.
        let owned = unsafe { std::ptr::read(self) };
        drop(owned.into_kind());
    }
}

impl Clone for Flow {
    fn clone(&self) -> Flow {
        let kind = match self.kind() {
            FlowRef::Signal(s) => FlowKind::Signal(Box::new(s.clone())),
            FlowRef::Throw(t) => FlowKind::Throw(Box::new(t.clone())),
            FlowRef::ThreadBlocked(b) => FlowKind::ThreadBlocked(Box::new(b.clone())),
            FlowRef::Shutdown(r) => FlowKind::Shutdown(r),
        };
        Flow::from_kind(kind)
    }
}

impl fmt::Debug for Flow {
    // Same text as `#[derive(Debug)]` on the old enum (Box<T>: Debug is T's).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind() {
            FlowRef::Signal(s) => f.debug_tuple("Signal").field(s).finish(),
            FlowRef::Throw(t) => f.debug_tuple("Throw").field(t).finish(),
            FlowRef::ThreadBlocked(b) => f.debug_tuple("ThreadBlocked").field(b).finish(),
            FlowRef::Shutdown(r) => f.debug_tuple("Shutdown").field(&r).finish(),
        }
    }
}
