//! Per-source call/arith feedback data (T1-pre).
//!
//! Moved definitions from `emacs_core::jit::feedback`; the lattice logic and
//! every implementation stay in the upper module. The data holds integers,
//! `SymId`s, `Weak<RuntimeState>` (as `Weak::into_raw`), and no Lisp `Value`.

use std::sync::OnceLock;
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64};

use super::{FeedbackVec, RuntimeState};

/// One source's feedback (see the module docs).
#[derive(Debug, Default)]
pub(crate) struct SourceFeedback {
    /// Per-op word: `NumericFeedback` at arithmetic pcs (the encoding the
    /// lowering's one predicate, `arith_site_takes_generic`, reads), the
    /// legacy symbol-only `CallFeedback` at call pcs.
    pub(crate) ops: FeedbackVec,
    /// The call-target lattice of the source's non-constant call sites,
    /// built on the first record (only ever under `NEOVM_JIT_FEEDBACK`).
    pub(crate) calls: OnceLock<CallSites>,
}

/// Most sources a site tells apart before it goes megamorphic.
pub(crate) const POLY: usize = 4;

/// Executions after which a site's lattice transition counts as LATE in the
/// census: the window a later tier would have compiled against
/// (`T2_WINDOW`, design §3.4). F-1's stability premise (R3) is that few
/// counted sites transition after it.
pub(crate) const STABLE_WINDOW: u32 = 15_000;

/// What a call site records.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum SiteShape {
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
pub(crate) struct CallSiteFeedback {
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
pub(crate) struct CallSites {
    /// Per op: 0 = no recorded site at this pc, else the site's index + 1.
    pub(crate) index: Box<[u16]>,
    pub(crate) sites: Box<[CallSiteFeedback]>,
}
