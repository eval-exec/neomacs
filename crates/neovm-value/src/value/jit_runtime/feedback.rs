//! Per-source call/arith feedback data (T1-pre).
//!
//! Moved definitions from `emacs_core::jit::feedback`; the lattice logic and
//! every implementation stay in the upper module. The data holds integers,
//! `SymId`s, `Weak<RuntimeState>` (as `Weak::into_raw`), and no Lisp `Value`.

use super::{FeedbackVec, RuntimeState};
/// One source's feedback (see the module docs).
use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
#[derive(Debug, Default)]
#[doc(hidden)]
pub struct SourceFeedback {
    /// Per-op word: `NumericFeedback` at arithmetic pcs (the encoding the
    /// lowering's one predicate, `arith_site_takes_generic`, reads), the
    /// legacy symbol-only `CallFeedback` at call pcs.
    pub(crate) ops: FeedbackVec,
    /// The call-target lattice of the source's non-constant call sites,
    /// built on the first record (only ever under `NEOVM_JIT_FEEDBACK`).
    pub(crate) calls: OnceLock<CallSites>,
}

/// Most sources a site tells apart before it goes megamorphic.
#[doc(hidden)]
pub const POLY: usize = 4;

/// Executions after which a site's lattice transition counts as LATE in the
/// census: the window a later tier would have compiled against
/// (`T2_WINDOW`, design §3.4). F-1's stability premise (R3) is that few
/// counted sites transition after it.
#[doc(hidden)]
pub const STABLE_WINDOW: u32 = 15_000;

/// What a call site records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
#[doc(hidden)]
pub enum SiteShape {
    /// The called value itself (the callee is not a constant: a variable,
    /// a closure, an argument).
    Callee = 0,
    /// The function `apply` spreads into: argument 0 of a call whose
    /// callee is the constant `apply`.
    ApplyArg = 1,
    /// The callback of a mapping builtin (`mapc`, `mapcar`, `mapcan`,
    /// `mapconcat`): argument 0.
    CallbackArg = 2,
}

/// One call site's feedback.
#[derive(Debug)]
#[doc(hidden)]
pub struct CallSiteFeedback {
    /// The lattice word: `STATE_UNINIT`, a symbol (`id << 3 | TAG_SYM`),
    /// one source (its identity word `| TAG_SOURCE`), `n` sources
    /// (`n << 3 | TAG_POLY`) or `STATE_MEGA`.
    pub(crate) state: AtomicU64,
    /// The sources seen, as `Weak::into_raw` (null = none): each keeps its
    /// allocation reserved while the site lives. Written before the state
    /// that names them.
    pub(crate) sources: [AtomicPtr<RuntimeState>; POLY],
    /// Executions counted by compiled code's recording call shims
    /// (saturating; the interpreter counts none).
    pub(crate) count: AtomicU32,
    /// Lattice transitions made at or after [`STABLE_WINDOW`] counted
    /// executions.
    pub(crate) late: AtomicU32,
    /// The site's pc in its body.
    pub(crate) pc: u32,
    /// What it records.
    pub(crate) shape: SiteShape,
}

/// A source's call sites: one [`CallSiteFeedback`] per call whose target is
/// not a constant the compile can read (see [`SiteShape`]).
#[derive(Debug)]
#[doc(hidden)]
pub struct CallSites {
    /// Per op: 0 = no recorded site at this pc, else the site's index + 1.
    pub(crate) index: Box<[u16]>,
    pub(crate) sites: Box<[CallSiteFeedback]>,
}

impl Drop for CallSiteFeedback {
    fn drop(&mut self) {
        for slot in &self.sources {
            let ptr = slot.load(Ordering::Relaxed);
            if !ptr.is_null() {
                // SAFETY: every non-null slot holds a `Weak::into_raw`
                // this site owns.
                drop(unsafe { Weak::from_raw(ptr) });
            }
        }
    }
}

impl SourceFeedback {
    #[doc(hidden)]
    #[inline]
    pub fn ops(&self) -> &FeedbackVec {
        &self.ops
    }

    #[doc(hidden)]
    #[inline]
    pub fn calls(&self) -> &OnceLock<CallSites> {
        &self.calls
    }
}

impl CallSites {
    /// Owned feedback values and scalar indices contain no forgeable raw owner.
    #[doc(hidden)]
    #[inline]
    pub fn from_parts(index: Box<[u16]>, sites: Box<[CallSiteFeedback]>) -> Self {
        Self { index, sites }
    }

    #[doc(hidden)]
    #[inline]
    pub fn index(&self) -> &[u16] {
        &self.index
    }

    #[doc(hidden)]
    #[inline]
    pub fn entries(&self) -> &[CallSiteFeedback] {
        &self.sites
    }
}

impl CallSiteFeedback {
    /// # Safety
    /// No concurrent swap/release of a source slot may overlap a borrowed
    /// load of that slot. Background compile reads vs mutator recording have
    /// a known pre-existing gap: see post-split item FEEDBACK-UAF. The caller
    /// must uphold this exclusion; this extraction preserves existing bodies
    /// and orderings and does not claim to repair the gap.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn new_storage(pc: u32, shape: SiteShape, initial_state: u64) -> Self {
        Self {
            state: AtomicU64::new(initial_state),
            sources: std::array::from_fn(|_| AtomicPtr::new(std::ptr::null_mut())),
            count: AtomicU32::new(0),
            late: AtomicU32::new(0),
            pc,
            shape,
        }
    }

    /// # Safety
    /// No concurrent swap/release of a source slot may overlap a borrowed
    /// load of that slot. Background compile reads vs mutator recording have
    /// a known pre-existing gap: see post-split item FEEDBACK-UAF. The caller
    /// must uphold this exclusion; this extraction preserves existing bodies
    /// and orderings and does not claim to repair the gap.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn install_source(&self, i: usize, source: Weak<RuntimeState>) {
        let weak = Weak::into_raw(source);
        let old = self.sources[i].swap(weak.cast_mut(), Ordering::Relaxed);
        if !old.is_null() {
            // SAFETY: every non-null slot holds a `Weak::into_raw`.
            drop(unsafe { Weak::from_raw(old) });
        }
    }

    /// # Safety
    /// No concurrent swap/release of a source slot may overlap a borrowed
    /// load of that slot. Background compile reads vs mutator recording have
    /// a known pre-existing gap: see post-split item FEEDBACK-UAF. The caller
    /// must uphold this exclusion; this extraction preserves existing bodies
    /// and orderings and does not claim to repair the gap.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn source_matches(&self, n: usize, source: &Arc<RuntimeState>) -> bool {
        let ptr = Arc::as_ptr(source);
        self.sources[..n.min(POLY)]
            .iter()
            .any(|s| std::ptr::eq(s.load(Ordering::Relaxed), ptr))
    }

    /// # Safety
    /// No concurrent swap/release of a source slot may overlap a borrowed
    /// load of that slot. Background compile reads vs mutator recording have
    /// a known pre-existing gap: see post-split item FEEDBACK-UAF. The caller
    /// must uphold this exclusion; this extraction preserves existing bodies
    /// and orderings and does not claim to repair the gap.
    #[doc(hidden)]
    #[inline]
    pub unsafe fn upgrade_source(&self, i: usize) -> Option<Arc<RuntimeState>> {
        let ptr = self.sources[i].load(Ordering::Relaxed);
        if ptr.is_null() {
            return None;
        }
        // SAFETY: a `Weak::into_raw` this site still owns; borrowed,
        // not dropped.
        let weak = std::mem::ManuallyDrop::new(unsafe { Weak::from_raw(ptr) });
        weak.upgrade()
    }

    #[doc(hidden)]
    #[inline]
    pub fn state_word(&self) -> &AtomicU64 {
        &self.state
    }

    #[doc(hidden)]
    #[inline]
    pub fn counter(&self) -> &AtomicU32 {
        &self.count
    }

    #[doc(hidden)]
    #[inline]
    pub fn late_counter(&self) -> &AtomicU32 {
        &self.late
    }

    #[doc(hidden)]
    #[inline]
    pub fn pc_value(&self) -> u32 {
        self.pc
    }

    #[doc(hidden)]
    #[inline]
    pub fn shape_value(&self) -> SiteShape {
        self.shape
    }
}
