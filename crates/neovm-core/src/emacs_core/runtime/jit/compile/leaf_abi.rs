//! Leaf builtin trampolines: the register-ABI entry points compiled code
//! calls for a leaf builtin (design `p1-2-builtin-intrinsics` §2.4, §2.5).
//!
//! A *bare* trampoline, `extern "C" fn(ctx, a0 [, a1 [, a2]]) -> u64`, serves
//! an opcode site (GNU's inline opcodes: no frame, depth count or poll). It
//! answers the result's own bits, or a tag-`001` sentinel: [`LEAF_SIGNAL`]
//! with the `Flow` stashed ([`stash_pending_flow`]), or [`LEAF_NEED_GENERIC`]
//! when the leaf declined the shape. The site tells them apart with one
//! `band`/`icmp`/`brif`, exactly as it does for the value shims.
//!
//! Compiled code calls a trampoline by its baked address (`iconst` +
//! `call_indirect`): trampolines are not registered with the JIT module, so
//! they add nothing to the per-compile symbol registration, and they are
//! JIT-only (an AOT object would need an imported table, design §2.9).
//!
//! Each trampoline passes its leaf body as a function ITEM, not through the
//! `LeafSpec`'s pointer, so the body inlines into it
//! (`trampolines_call_their_spec_bodies` holds the two to each other). The
//! body runs under `catch_unwind` (`jit_shim_contain!`), except the audited
//! fast half of a `Containment::FastOutside` leaf, whose declines take the
//! contained body in a cold helper.

use super::*;
use crate::emacs_core::builtins::leaves;
#[cfg(debug_assertions)]
use crate::emacs_core::subr::leaf::LeafOutcome;
use crate::emacs_core::subr::leaf::{
    LeafActive, LeafExit, LeafId, LeafResult, LeafShape, LeafSpec,
};

/// A leaf trampoline's signal word: the flow is stashed.
pub(crate) const LEAF_SIGNAL: i64 = VALUE_SHIM_SIGNAL;
/// A leaf trampoline's decline word: run the reference for this shape.
pub(crate) const LEAF_NEED_GENERIC: i64 = VALUE_SHIM_NEED_GENERIC;

// ---------------------------------------------------------------------------
// Counters.
// ---------------------------------------------------------------------------

/// Per-leaf engagement counters: how many sites each compile lowered to the
/// leaf (compile time), and how often a call declined or signalled (cold
/// paths only; the hot path counts nothing in release builds). Reported in
/// the `[neovm-jit-final-builtin-leaves]` line.
pub(crate) struct LeafSiteStats {
    pub(crate) opcode_sites: AtomicU64,
    pub(crate) bcall_sites: AtomicU64,
    pub(crate) generic: AtomicU64,
    pub(crate) signal: AtomicU64,
}

impl LeafSiteStats {
    const fn new() -> Self {
        Self {
            opcode_sites: AtomicU64::new(0),
            bcall_sites: AtomicU64::new(0),
            generic: AtomicU64::new(0),
            signal: AtomicU64::new(0),
        }
    }
}

pub(crate) static LEAF_STATS: [LeafSiteStats; LeafId::COUNT] =
    [const { LeafSiteStats::new() }; LeafId::COUNT];

#[cfg(test)]
thread_local! {
    /// Test hook: trampoline entries per leaf on this thread (any outcome).
    pub(crate) static LEAF_TRAMPOLINE_CALLS: [std::cell::Cell<u64>; LeafId::COUNT] =
        const { [const { std::cell::Cell::new(0) }; LeafId::COUNT] };
}

/// Test hook: trampoline entries of `id` on this thread so far.
#[cfg(test)]
pub(crate) fn leaf_trampoline_calls(id: LeafId) -> u64 {
    LEAF_TRAMPOLINE_CALLS.with(|c| c[id.index()].get())
}

/// The census line of the exit report: every leaf with a nonzero counter.
pub(crate) fn render_leaf_stats() -> String {
    let mut out = Vec::new();
    for spec in crate::emacs_core::subr::leaf::LEAVES {
        let s = &LEAF_STATS[spec.id.index()];
        let (o, b, g, x) = (
            s.opcode_sites.load(Ordering::Relaxed),
            s.bcall_sites.load(Ordering::Relaxed),
            s.generic.load(Ordering::Relaxed),
            s.signal.load(Ordering::Relaxed),
        );
        if o + b + g + x > 0 {
            out.push(format!(
                "{}:opcode_sites={o},bcall_sites={b},generic={g},signal={x}",
                spec.name
            ));
        }
    }
    out.join(" ")
}

// ---------------------------------------------------------------------------
// The trampoline body.
// ---------------------------------------------------------------------------

/// Map a leaf's answer to the trampoline's return word. The two cold arms
/// count; the value arm is the whole hot path.
#[inline(always)]
fn leaf_word(id: LeafId, result: LeafResult) -> i64 {
    match result {
        Ok(value) => value.bits() as i64,
        Err(LeafExit::Signal(flow)) => {
            stash_pending_flow(flow);
            LEAF_STATS[id.index()]
                .signal
                .fetch_add(1, Ordering::Relaxed);
            LEAF_SIGNAL
        }
        Err(LeafExit::Generic) => {
            LEAF_STATS[id.index()]
                .generic
                .fetch_add(1, Ordering::Relaxed);
            LEAF_NEED_GENERIC
        }
    }
}

/// The contained body: `catch_unwind` around the call, one word out (a
/// 16-byte `Result` through the unwind data union would be P0.2's
/// store-forwarding pattern again).
#[inline(always)]
fn leaf_contained(
    ctx: *const Context,
    spec: &'static LeafSpec,
    body: impl FnOnce(&Context) -> LeafResult,
) -> i64 {
    jit_shim_contain!(ctx as *mut u8, LEAF_SIGNAL, {
        // SAFETY: the seam's dormant Context (the vmctx contract of
        // `neovm_jit_call`); a leaf takes only a shared borrow.
        let c = unsafe { &*ctx };
        leaf_word(spec.id, body(c))
    })
}

/// Enter a trampoline: the test counter and, in debug builds, the leaf
/// marker (a zero-sized no-op in release builds).
#[inline(always)]
fn leaf_enter<'a>(ctx: *const Context, spec: &'static LeafSpec) -> (&'a Context, LeafActive) {
    #[cfg(test)]
    LEAF_TRAMPOLINE_CALLS.with(|c| {
        let cell = &c[spec.id.index()];
        cell.set(cell.get() + 1);
    });
    // SAFETY: the seam's dormant Context (the vmctx contract of
    // `neovm_jit_call`), alive for the whole call; shared read only.
    let c: &'a Context = unsafe { &*ctx };
    (c, LeafActive::enter(spec, c))
}

/// Leave a trampoline with its word: debug builds check the witnesses
/// (unless a contained panic left its residue for the caller to heal).
#[inline(always)]
fn leaf_finish(active: LeafActive, c: &Context, word: i64) -> i64 {
    #[cfg(debug_assertions)]
    if !shim_panic_pending() {
        active.exit(c, word_outcome(word));
    }
    #[cfg(not(debug_assertions))]
    let _ = (active, c);
    word
}

/// What a return word says about the call, for the debug witnesses.
#[cfg(debug_assertions)]
#[inline(always)]
fn word_outcome(word: i64) -> LeafOutcome {
    match word {
        LEAF_SIGNAL => LeafOutcome::Signal,
        LEAF_NEED_GENERIC => LeafOutcome::Generic,
        _ => LeafOutcome::Value,
    }
}

/// Generate a leaf's bare trampoline, `$bare(ctx, a0..) -> word`.
///
/// With `fast = F, rest = R`, the audited fast half `F` runs first outside
/// containment, and `R` -- a cold function generated here -- runs the
/// contained body for whatever `F` declines.
macro_rules! bare_trampoline {
    ($bare:ident, $spec:path, $body:path, 1) => {
        #[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI trampoline: vmctx contract.
        #[unsafe(no_mangle)]
        pub extern "C" fn $bare(ctx: *const Context, a: i64) -> i64 {
            let a = Value::from_bits(a as usize);
            let (c, active) = leaf_enter(ctx, &$spec);
            let word = leaf_contained(ctx, &$spec, |c| $body(c, a));
            leaf_finish(active, c, word)
        }
    };
    ($bare:ident, $spec:path, $body:path, 2) => {
        #[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI trampoline: vmctx contract.
        #[unsafe(no_mangle)]
        pub extern "C" fn $bare(ctx: *const Context, a: i64, b: i64) -> i64 {
            let (a, b) = (Value::from_bits(a as usize), Value::from_bits(b as usize));
            let (c, active) = leaf_enter(ctx, &$spec);
            let word = leaf_contained(ctx, &$spec, |c| $body(c, a, b));
            leaf_finish(active, c, word)
        }
    };
    ($bare:ident, $spec:path, $body:path, 2, fast = $fast:path, rest = $rest:ident) => {
        #[allow(clippy::not_unsafe_ptr_arg_deref)] // C-ABI trampoline: vmctx contract.
        #[unsafe(no_mangle)]
        pub extern "C" fn $bare(ctx: *const Context, a: i64, b: i64) -> i64 {
            let (a, b) = (Value::from_bits(a as usize), Value::from_bits(b as usize));
            let (c, active) = leaf_enter(ctx, &$spec);
            let word = match $fast(c, &[a, b, Value::NIL, Value::NIL]) {
                Some(value) => value.bits() as i64,
                None => $rest(ctx, a, b),
            };
            leaf_finish(active, c, word)
        }

        #[cold]
        #[inline(never)]
        fn $rest(ctx: *const Context, a: Value, b: Value) -> i64 {
            leaf_contained(ctx, &$spec, |c| $body(c, a, b))
        }
    };
}

bare_trampoline!(neovm_leaf_bare_get, leaves::GET, leaves::get, 2);
bare_trampoline!(neovm_leaf_bare_length, leaves::LENGTH, leaves::length, 1);
bare_trampoline!(
    neovm_leaf_bare_nth,
    leaves::NTH,
    leaves::nth,
    2,
    fast = leaves::fast::nth_fast,
    rest = nth_rest
);
bare_trampoline!(neovm_leaf_bare_nthcdr, leaves::NTHCDR, leaves::nthcdr, 2);
bare_trampoline!(neovm_leaf_bare_elt, leaves::ELT, leaves::elt, 2);
bare_trampoline!(neovm_leaf_bare_member, leaves::MEMBER, leaves::member, 2);
bare_trampoline!(neovm_leaf_bare_equal, leaves::EQUAL, leaves::equal, 2);
bare_trampoline!(
    neovm_leaf_bare_string_equal,
    leaves::STRING_EQUAL,
    leaves::string_equal,
    2
);
bare_trampoline!(
    neovm_leaf_bare_string_lessp,
    leaves::STRING_LESSP,
    leaves::string_lessp,
    2
);

/// The opcode leaf an opcode site calls, if the opcode has one wired to a
/// bare trampoline. `memq`/`assq` stay on their value shims (whose fast
/// halves already run outside containment), `aref`/`aset` on theirs plus the
/// inline paths.
pub(crate) fn opcode_leaf(op: &Op) -> Option<LeafId> {
    Some(match op {
        Op::Get => LeafId::Get,
        Op::Length => LeafId::Length,
        Op::Nth => LeafId::Nth,
        Op::Nthcdr => LeafId::Nthcdr,
        Op::Elt => LeafId::Elt,
        Op::Member => LeafId::Member,
        Op::Equal => LeafId::Equal,
        Op::StringEqual => LeafId::StringEqual,
        Op::StringLessp => LeafId::StringLessp,
        _ => return None,
    })
}

/// A leaf's bare trampoline address, for the leaves [`opcode_leaf`] maps.
pub(crate) fn bare_trampoline(id: LeafId) -> Option<*const u8> {
    let f: *const u8 = match id {
        LeafId::Get => neovm_leaf_bare_get as *const u8,
        LeafId::Length => neovm_leaf_bare_length as *const u8,
        LeafId::Nth => neovm_leaf_bare_nth as *const u8,
        LeafId::Nthcdr => neovm_leaf_bare_nthcdr as *const u8,
        LeafId::Elt => neovm_leaf_bare_elt as *const u8,
        LeafId::Member => neovm_leaf_bare_member as *const u8,
        LeafId::Equal => neovm_leaf_bare_equal as *const u8,
        LeafId::StringEqual => neovm_leaf_bare_string_equal as *const u8,
        LeafId::StringLessp => neovm_leaf_bare_string_lessp as *const u8,
        LeafId::Gethash
        | LeafId::PlistGet
        | LeafId::GetCharProperty
        | LeafId::Memq
        | LeafId::Assq => return None,
    };
    Some(f)
}

/// Whether an opcode site lowers `op` to its leaf in this compile: JIT only,
/// `NEOVM_JIT_LEAF` with `opcode`, the leaf past the `NEOVM_JIT_LEAF_ONLY`
/// filter, and a leaf that never bounces (an opcode site has no reference
/// call to bounce to; none of today's opcode leaves declares a shape).
pub(crate) fn opcode_leaf_site(op: &Op, aot: bool) -> Option<LeafId> {
    if aot || !super::jit_leaf_knob().opcode {
        return None;
    }
    let id = opcode_leaf(op)?;
    let spec = id.spec();
    if spec.shape != LeafShape::Opcode
        || !spec.generic_when.is_empty()
        || !super::jit_leaf_selected(spec.name)
    {
        return None;
    }
    bare_trampoline(id)?;
    Some(id)
}

/// Emit the call of `id`'s bare trampoline with `vmctx` and `operands` and
/// return its word. Counts the site.
pub(crate) fn emit_bare_leaf_call(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    id: LeafId,
    operands: &[ClifValue],
) -> ClifValue {
    let tramp = bare_trampoline(id).expect("an opcode leaf site has a bare trampoline");
    let mut sig = Signature::new(rt.refs.call_conv);
    sig.params.push(AbiParam::new(rt.ptr_ty));
    for _ in operands {
        sig.params.push(AbiParam::new(types::I64));
    }
    sig.returns.push(AbiParam::new(types::I64));
    let sig = fb.import_signature(sig);
    let callee = fb.ins().iconst(rt.ptr_ty, tramp as i64);
    let vmctx = fb.use_var(rt.vmctx_var);
    let mut args: SmallVec<[ClifValue; 4]> = SmallVec::new();
    args.push(vmctx);
    args.extend_from_slice(operands);
    let call = fb.ins().call_indirect(sig, callee, &args);
    LEAF_STATS[id.index()]
        .opcode_sites
        .fetch_add(1, Ordering::Relaxed);
    fb.inst_results(call)[0]
}

/// The trampolines' bodies and fast halves are the ones their specs declare
/// (the trampolines name them as items so they inline).
#[cfg(test)]
pub(crate) fn trampoline_bodies_for_test() -> Vec<(LeafId, usize, Option<usize>)> {
    use crate::emacs_core::subr::leaf::LeafEntry;
    let two = |f: crate::emacs_core::subr::leaf::Leaf2| f as usize;
    let one = |f: crate::emacs_core::subr::leaf::Leaf1| f as usize;
    let _ = LeafEntry::L1;
    vec![
        (LeafId::Get, two(leaves::get), None),
        (LeafId::Length, one(leaves::length), None),
        (
            LeafId::Nth,
            two(leaves::nth),
            Some(leaves::fast::nth_fast as crate::emacs_core::subr::leaf::LeafFast as usize),
        ),
        (LeafId::Nthcdr, two(leaves::nthcdr), None),
        (LeafId::Elt, two(leaves::elt), None),
        (LeafId::Member, two(leaves::member), None),
        (LeafId::Equal, two(leaves::equal), None),
        (LeafId::StringEqual, two(leaves::string_equal), None),
        (LeafId::StringLessp, two(leaves::string_lessp), None),
    ]
}

/// The containment a spec declares, as the trampoline tests compare it.
#[cfg(test)]
pub(crate) fn declared_fast_half(spec: &LeafSpec) -> Option<usize> {
    use crate::emacs_core::subr::leaf::Containment;
    match spec.containment {
        Containment::Catch => None,
        Containment::FastOutside(f) => Some(f as usize),
    }
}
