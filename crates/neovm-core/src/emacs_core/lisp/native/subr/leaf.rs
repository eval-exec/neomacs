//! Leaf builtins: a second, effect-typed entry point for a builtin that
//! compiled code may call without the builtin-call protocol (design
//! `p1-2-builtin-intrinsics` §2.2).
//!
//! A leaf body is `fn(&Context, Value...) -> LeafResult`. The SHARED borrow is
//! the contract: every GC safe point (`maybe_gc`), the quit poll
//! (`maybe_quit`), every entry into Lisp (`funcall`, `eval`, `apply`), a
//! `specbind` and a condition-stack push all take `&mut Context`, so a leaf
//! cannot collect, cannot run Lisp and cannot start a nested activation --
//! checked by the compiler, not by a comment. That is what lets its call site
//! root nothing, push no frame and keep raw values in registers across the
//! call. The [`Effects`] bits are metadata on top of that proof: a wrong bit
//! can cost an optimization, never a use-after-free.
//!
//! A leaf may decline an argument shape whose reference behaviour needs Lisp
//! (a user-defined hash test, a `plist-get` predicate): it answers
//! [`LeafExit::Generic`] BEFORE any side effect, and the call site runs the
//! reference builtin from scratch.
//!
//! Two shapes, after GNU's two ways of reaching a primitive from bytecode:
//!
//! * [`LeafShape::Opcode`]: an inline opcode (`Bget`, `Blength`, `Bnth`, ...)
//!   calls the primitive directly, with no frame, depth count or quit poll.
//!   The leaf answers exactly as the interpreter's opcode arm does (for `nth`
//!   that is `Bnth`, whose error datum differs from `Fnth`'s), and compiled
//!   code finds it by opcode.
//! * [`LeafShape::Bcall`]: `Op::Call` on a symbol (GNU `Bcall`), which records
//!   a backtrace frame, counts depth and polls quit. The leaf answers exactly
//!   as the registered builtin does; it is attached to that builtin's
//!   [`SubrSpec`](super::SubrSpec) (`SubrSpec::leaf`) and found through the
//!   subr the call site's symbol is bound to. The call site proves the
//!   protocol unobservable before taking it (the JIT's Bcall guard) and
//!   records the frame lazily when the leaf signals.

// Only compiled code calls leaves: without the JIT they are declarations.
#![cfg_attr(not(feature = "jit"), allow(dead_code))]
// TEMPORARY: the JIT's trampolines, the next commit, are the first users.
#![allow(dead_code)]

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::SymId;
use crate::emacs_core::value::Value;
use std::cell::RefCell;

/// Independent effects, rather than a strongest-effect ordering. Allocating
/// does not imply collecting, and signaling does not subsume state mutation.
///
/// Shared by the leaf declarations here and by the JIT's call contracts
/// (`jit::compile::calls`), which re-export it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effects(u16);

impl Effects {
    pub const PURE: Self = Self(0);
    pub const READ_HEAP: Self = Self(1 << 0);
    pub const WRITE_HEAP: Self = Self(1 << 1);
    pub const READ_BUFFER: Self = Self(1 << 2);
    pub const WRITE_BUFFER: Self = Self(1 << 3);
    pub const READ_MATCH: Self = Self(1 << 4);
    pub const WRITE_MATCH: Self = Self(1 << 5);
    pub const READ_BINDINGS: Self = Self(1 << 6);
    pub const WRITE_BINDINGS: Self = Self(1 << 7);
    pub const ALLOCATES: Self = Self(1 << 8);
    pub const MAY_GC: Self = Self(1 << 9);
    pub const MAY_REENTER: Self = Self(1 << 10);
    pub const MAY_SIGNAL: Self = Self(1 << 11);
    pub const MAY_DEOPT: Self = Self(1 << 12);
    pub const UNKNOWN: Self = Self((1 << 13) - 1);

    pub const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any bit of `other` is set here.
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// A read-only fast path still observes mutable runtime state. This is an
    /// admission fact, never permission to hoist it or elide fallback roots.
    pub const fn is_read_only(self) -> bool {
        let reads =
            Self::READ_HEAP.0 | Self::READ_BUFFER.0 | Self::READ_MATCH.0 | Self::READ_BINDINGS.0;
        self.0 & !reads == 0
    }

    /// The effects no leaf may have: a leaf never collects, runs Lisp,
    /// deopts or writes variable bindings. The entry type already rules out
    /// the first two; [`LeafSpec::new`] asserts all four at compile time.
    pub const FORBIDDEN_IN_LEAF: Self =
        Self(Self::MAY_GC.0 | Self::MAY_REENTER.0 | Self::MAY_DEOPT.0 | Self::WRITE_BINDINGS.0);
}

/// Why a leaf body produced no value.
#[derive(Debug)]
pub(crate) enum LeafExit {
    /// A signal raised by the body and NOT yet dispatched: no signal hook,
    /// `handler-bind` handler or debugger has run. The call site dispatches
    /// it with the GNU-visible frame in place.
    Signal(Flow),
    /// "Not my case": this argument shape needs Lisp in the reference
    /// builtin. Returned before ANY side effect, so the site may run the
    /// reference builtin from scratch.
    Generic,
}

impl From<Flow> for LeafExit {
    #[inline(always)]
    fn from(flow: Flow) -> Self {
        LeafExit::Signal(flow)
    }
}

pub(crate) type LeafResult = Result<Value, LeafExit>;
pub(crate) type Leaf1 = fn(&Context, Value) -> LeafResult;
pub(crate) type Leaf2 = fn(&Context, Value, Value) -> LeafResult;
pub(crate) type Leaf3 = fn(&Context, Value, Value, Value) -> LeafResult;

/// A leaf body by argument count. (A zero- or four-slot shape joins when a
/// leaf needs it; the register ABI has room for four arguments.)
#[derive(Clone, Copy)]
pub(crate) enum LeafEntry {
    L1(Leaf1),
    L2(Leaf2),
    L3(Leaf3),
}

impl LeafEntry {
    /// How many argument slots the body takes (missing optionals are nil).
    pub(crate) const fn slots(self) -> u16 {
        match self {
            LeafEntry::L1(_) => 1,
            LeafEntry::L2(_) => 2,
            LeafEntry::L3(_) => 3,
        }
    }

    /// Call the body with `args`, nil-padded to [`Self::slots`] (the
    /// fixed-arity dispatcher's convention). For tests and the debug
    /// harness; compiled code calls the body through its trampoline.
    #[cfg(test)]
    pub(crate) fn call(self, ctx: &Context, args: &[Value]) -> LeafResult {
        let arg = |i: usize| args.get(i).copied().unwrap_or(Value::NIL);
        match self {
            LeafEntry::L1(f) => f(ctx, arg(0)),
            LeafEntry::L2(f) => f(ctx, arg(0), arg(1)),
            LeafEntry::L3(f) => f(ctx, arg(0), arg(1), arg(2)),
        }
    }
}

/// How compiled code reaches a leaf (see the module docs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafShape {
    /// An inline opcode: no frame, no depth count, no quit poll. Answers as
    /// the interpreter's opcode arm.
    Opcode,
    /// `Op::Call` on a symbol bound to the builtin: GNU `Bcall`'s protocol,
    /// proven unobservable by the site's guard. Answers as the registered
    /// builtin and is attached to its `SubrSpec`.
    Bcall,
}

/// An argument shape a leaf declines with [`LeafExit::Generic`] because the
/// reference builtin runs Lisp for it. The equivalence harness exercises
/// every listed shape and requires the bounce.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BounceShape {
    /// `gethash` on a table made with `define-hash-table-test`: the user's
    /// hash and equality functions are Lisp.
    UserHashTest,
    /// `plist-get` with a non-nil PREDICATE, which GNU calls through funcall.
    PlistPredicate,
}

/// The audited, panic-free fast half of a [`Containment::FastOutside`] leaf:
/// the answer, or `None` to fall through to the contained body. It sees the
/// call's arguments nil-padded to four.
pub(crate) type LeafFast = fn(&Context, &[Value; 4]) -> Option<Value>;

/// How a leaf's trampoline contains a Rust panic in its body.
#[derive(Clone, Copy)]
pub(crate) enum Containment {
    /// The whole body runs under `catch_unwind` (the default: bodies call
    /// library code such as hash maps and `RefCell`s).
    Catch,
    /// The fast half runs outside containment -- it must be audited
    /// panic-free, like the value shims' fast paths -- and anything it
    /// declines runs the whole body under `catch_unwind`.
    FastOutside(LeafFast),
}

/// Dense, stable leaf identities: the index into [`LEAVES`]. Stable because
/// an AOT leaf table (design §2.9, phase 2) would be indexed by them.
#[repr(u16)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LeafId {
    Gethash,
    PlistGet,
    GetCharProperty,
    Get,
    Length,
    Nth,
    Nthcdr,
    Elt,
    Memq,
    Assq,
    Member,
    Equal,
    StringEqual,
    StringLessp,
    // TODO(P1.4 Stage A): `symbol-value` (Op::SymbolValue) and
    // `buffer-local-value` (Bcall) leaves. Their bodies must call P1.4
    // Stage A's `read_var_cached` rather than grow a second copy of the
    // variable read tiers; that function lives on the P1.4 branch, which
    // has not landed (p1-0-integration §2 P1.2 correction 4, stage S1.5a).
}

impl LeafId {
    /// Number of leaves: the length of [`LEAVES`].
    pub(crate) const COUNT: usize = LeafId::StringLessp as usize + 1;

    /// The index into [`LEAVES`].
    pub(crate) const fn index(self) -> usize {
        self as usize
    }

    /// The leaf's declaration.
    pub(crate) fn spec(self) -> &'static LeafSpec {
        LEAVES[self.index()]
    }
}

/// One leaf builtin's declaration.
pub(crate) struct LeafSpec {
    pub(crate) id: LeafId,
    /// The builtin's Lisp name. For a [`LeafShape::Bcall`] leaf it equals its
    /// `SubrSpec`'s name (asserted at registration).
    pub(crate) name: &'static str,
    pub(crate) entry: LeafEntry,
    pub(crate) shape: LeafShape,
    /// What a successful call may do (metadata; see the module docs).
    pub(crate) effects: Effects,
    /// The shapes the body declines with [`LeafExit::Generic`].
    pub(crate) generic_when: &'static [BounceShape],
    pub(crate) containment: Containment,
}

impl LeafSpec {
    /// A leaf is never MAY_GC, MAY_REENTER, MAY_DEOPT or WRITE_BINDINGS:
    /// checked here at compile time (declarations are consts).
    pub(crate) const fn new(
        id: LeafId,
        name: &'static str,
        entry: LeafEntry,
        shape: LeafShape,
        effects: Effects,
        generic_when: &'static [BounceShape],
        containment: Containment,
    ) -> Self {
        assert!(!name.is_empty(), "a leaf must name its builtin");
        assert!(
            !effects.intersects(Effects::FORBIDDEN_IN_LEAF),
            "a leaf never collects, runs Lisp, deopts or writes bindings"
        );
        Self {
            id,
            name,
            entry,
            shape,
            effects,
            generic_when,
            containment,
        }
    }
}

/// Every leaf, indexed by [`LeafId`] (`leaf_ids_are_dense_and_stable`).
pub(crate) static LEAVES: [&LeafSpec; LeafId::COUNT] = {
    use crate::emacs_core::builtins::leaves as l;
    [
        &l::GETHASH,
        &l::PLIST_GET,
        &l::GET_CHAR_PROPERTY,
        &l::GET,
        &l::LENGTH,
        &l::NTH,
        &l::NTHCDR,
        &l::ELT,
        &l::MEMQ,
        &l::ASSQ,
        &l::MEMBER,
        &l::EQUAL,
        &l::STRING_EQUAL,
        &l::STRING_LESSP,
    ]
};

thread_local! {
    /// `SymId`-indexed: the [`LeafShape::Bcall`] leaf attached to the builtin
    /// registered under that symbol, written by `Context::register_subr` from
    /// the same `SubrSpec` that installs the subr object, so the two cannot
    /// disagree. Kept beside the global subr table rather than in
    /// `SubrEntry`, which the interpreter copies on every builtin dispatch.
    static LEAF_BY_SUBR: RefCell<Vec<Option<&'static LeafSpec>>> = const { RefCell::new(Vec::new()) };
}

/// What [`record_subr_leaf`] found for the symbol before this registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafChange {
    /// The symbol keeps the leaf (or the lack of one) it had.
    Unchanged,
    /// The symbol's leaf changed after it had one registered: code compiled
    /// against the old leaf must not run (`install_subr` clears the JIT
    /// cache). Only a test re-registers a builtin with a different leaf.
    Replaced,
}

/// Record the leaf (if any) of the builtin registered under `sym`.
pub(crate) fn record_subr_leaf(sym: SymId, leaf: Option<&'static LeafSpec>) -> LeafChange {
    LEAF_BY_SUBR.with(|table| {
        let mut table = table.borrow_mut();
        let idx = sym.0 as usize;
        if table.len() <= idx {
            if leaf.is_none() {
                return LeafChange::Unchanged;
            }
            table.resize(idx + 1, None);
        }
        let old = std::mem::replace(&mut table[idx], leaf);
        match (old, leaf) {
            (Some(old), Some(new)) if std::ptr::eq(old, new) => LeafChange::Unchanged,
            (Some(_), _) => LeafChange::Replaced,
            (None, _) => LeafChange::Unchanged,
        }
    })
}

/// The [`LeafShape::Bcall`] leaf of the builtin registered under `sym`.
pub(crate) fn subr_leaf(sym: SymId) -> Option<&'static LeafSpec> {
    LEAF_BY_SUBR.with(|table| table.borrow().get(sym.0 as usize).copied().flatten())
}

// ---------------------------------------------------------------------------
// The debug leaf guard (design §6.2, §6.3).
//
// The type proof has two holes a comment cannot close: a thread-local or raw
// pointer route to `&mut Context` (the dynamic-module `MODULE_CTX`), and
// interior mutability. So in debug builds every leaf call runs under a
// `LeafActive` marker, the evaluator's GC safe points, Lisp entries and
// binding pushes assert that no marker is live, and the marker checks on
// exit that the leaf left the evaluator's shared state where it found it.
// Release builds compile all of it away.
// ---------------------------------------------------------------------------

#[cfg(debug_assertions)]
thread_local! {
    /// How many leaf bodies are running on this thread.
    static LEAF_ACTIVE: std::cell::Cell<u32> = const { std::cell::Cell::new(0) };
}

/// Debug builds: panic when a leaf body is running. `site` names a GC safe
/// point, a Lisp entry or a binding push -- a place the leaf contract says
/// no leaf can reach.
#[cfg(debug_assertions)]
#[track_caller]
pub(crate) fn assert_no_leaf_active(site: &'static str) {
    let active = LEAF_ACTIVE.with(std::cell::Cell::get);
    assert!(
        active == 0,
        "{site} reached from inside a leaf builtin: the leaf contract \
         (no GC, no Lisp, no bindings) is broken"
    );
}

/// `debug_assert_no_leaf_active!("site")`: [`assert_no_leaf_active`] in
/// debug builds, nothing in release builds.
macro_rules! debug_assert_no_leaf_active {
    ($site:literal) => {
        #[cfg(debug_assertions)]
        $crate::emacs_core::subr::leaf::assert_no_leaf_active($site);
    };
}
pub(crate) use debug_assert_no_leaf_active;

/// How a leaf call ended, for [`LeafActive::exit`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LeafOutcome {
    Value,
    Signal,
    Generic,
}

impl LeafOutcome {
    pub(crate) fn of(result: &LeafResult) -> Self {
        match result {
            Ok(_) => LeafOutcome::Value,
            Err(LeafExit::Signal(_)) => LeafOutcome::Signal,
            Err(LeafExit::Generic) => LeafOutcome::Generic,
        }
    }
}

/// Evaluator state a leaf may not change (design §6.3): always the control
/// stacks, the depth and the function epoch; the current buffer's identity,
/// point, narrowing and modification tick unless the leaf declares
/// WRITE_BUFFER; the heap's allocation count on a value unless it declares
/// ALLOCATES (an error path allocates its signal data).
#[cfg(debug_assertions)]
#[derive(Debug, PartialEq, Eq)]
struct LeafWitness {
    specpdl: usize,
    conditions: usize,
    bc_buf: usize,
    depth: usize,
    function_epoch: u64,
    buffer: Option<(
        crate::buffer::BufferId,
        crate::buffer::CharPos0,
        crate::buffer::CharPos0,
        crate::buffer::CharPos0,
        i64,
    )>,
    allocated: usize,
}

#[cfg(debug_assertions)]
impl LeafWitness {
    fn take(ctx: &Context) -> Self {
        Self {
            specpdl: ctx.specpdl.len(),
            conditions: ctx.condition_stack.len(),
            bc_buf: ctx.bc_buf.len(),
            depth: ctx.depth,
            function_epoch: ctx.obarray.function_epoch(),
            buffer: ctx.buffers.current_buffer().map(|buf| {
                (
                    buf.id(),
                    buf.point_char_pos(),
                    buf.point_min_char_pos(),
                    buf.point_max_char_pos(),
                    buf.modified_tick(),
                )
            }),
            allocated: ctx.tagged_heap.allocated_count(),
        }
    }
}

/// Debug-build marker that a leaf body is running (see the section
/// comment). Construct it with [`LeafActive::enter`] around exactly one leaf
/// body and end it with [`LeafActive::exit`]; the count drops on unwind too.
/// In release builds it is a zero-sized no-op.
pub(crate) struct LeafActive {
    #[cfg(debug_assertions)]
    spec: &'static LeafSpec,
    #[cfg(debug_assertions)]
    before: LeafWitness,
}

impl LeafActive {
    #[inline(always)]
    pub(crate) fn enter(spec: &'static LeafSpec, ctx: &Context) -> Self {
        #[cfg(debug_assertions)]
        {
            LEAF_ACTIVE.with(|c| c.set(c.get() + 1));
            Self {
                spec,
                before: LeafWitness::take(ctx),
            }
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = (spec, ctx);
            Self {}
        }
    }

    /// Check the witnesses against how the call ended (debug builds).
    #[inline(always)]
    pub(crate) fn exit(self, ctx: &Context, outcome: LeafOutcome) {
        #[cfg(debug_assertions)]
        {
            let mut after = LeafWitness::take(ctx);
            let before = &self.before;
            let effects = self.spec.effects;
            if effects.contains(Effects::WRITE_BUFFER) && outcome != LeafOutcome::Generic {
                after.buffer = before.buffer;
            }
            let may_allocate = match outcome {
                LeafOutcome::Value => effects.contains(Effects::ALLOCATES),
                LeafOutcome::Signal => true,
                LeafOutcome::Generic => false,
            };
            if may_allocate {
                after.allocated = before.allocated;
            }
            assert_eq!(
                &after, before,
                "leaf `{}` ({outcome:?}) changed evaluator state it does not declare",
                self.spec.name
            );
            if outcome == LeafOutcome::Generic {
                assert!(
                    !self.spec.generic_when.is_empty(),
                    "leaf `{}` bounced but declares no bounce shape",
                    self.spec.name
                );
            }
        }
        #[cfg(not(debug_assertions))]
        let _ = (ctx, outcome);
    }
}

impl Drop for LeafActive {
    #[inline(always)]
    fn drop(&mut self) {
        #[cfg(debug_assertions)]
        LEAF_ACTIVE.with(|c| c.set(c.get() - 1));
    }
}

/// Run a leaf body as compiled code does in a debug build: under
/// [`LeafActive`], witnesses checked on the way out. The harnesses call
/// leaves through this.
#[cfg(test)]
pub(crate) fn call_checked(spec: &'static LeafSpec, ctx: &Context, args: &[Value]) -> LeafResult {
    let active = LeafActive::enter(spec, ctx);
    let result = spec.entry.call(ctx, args);
    active.exit(ctx, LeafOutcome::of(&result));
    result
}

#[cfg(test)]
#[path = "tests/leaf_contract.rs"]
mod leaf_contract_tests;
