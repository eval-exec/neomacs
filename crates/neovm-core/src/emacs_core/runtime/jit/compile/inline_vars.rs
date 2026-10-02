//! Variable ops inline in JIT code (P1.4 Stage B; design
//! `p1-4-inline-binding-blv` §4.3, p1-0-integration S2.3a-f): `varref`,
//! `varset`, `varbind` and `unbind` of a plain variable, of a buffer-local
//! variable whose cache is loaded for the current buffer (GNU
//! `swap_in_symval_forwarding`'s early-out), and of a forwarder that holds its
//! own value, done in place instead of in `neovm_jit_varref`, `_varset`,
//! `_varbind` and `_unbind`. Knob `NEOVM_JIT_INLINE_VARS` (default `read`).
//!
//! # Contract
//!
//! Each fast path is the cache-hit prefix of one Rust tier, named at its
//! emitter, and refuses what that tier refuses: the plain tiers
//! (`Context::try_set_plain_variable`, `specbind_plain_untrapped_fast`, the
//! `Let` arm of `pop_simple_specpdl_suffix`) and the Stage A cached tiers
//! (`eval/var_fast.rs`). Every guard runs before the first store, and a
//! refusal branches to the unchanged shim call with the original operands,
//! so a refused op is exactly today's op. No fast path calls, allocates,
//! signals or reaches a safe point, so none roots anything; the shim branch
//! keeps its residual roots and meets the root-window record at the join.
//!
//! # What is baked, and why it stays valid
//!
//! The class of a variable (plain, buffer-local, forwarded) is read from the
//! live obarray at compile time and picks one fast path; the class is
//! re-tested on every execution (the flags byte, the value word, the cache's
//! owner buffer and epoch, the forwarder), so a later `make-local-variable`,
//! `defvaralias`, `add-variable-watcher`, `makunbound`, `set-buffer` or
//! `kill-local-variable` costs a shim call, never a wrong answer. The
//! addresses baked are the symbol's cell (`Obarray::jit_symbol_cell_addr`:
//! chunks never move), its BLV record (made once per symbol, freed with the
//! obarray), its forwarder (leaked `'static`) and the process-global BLV
//! epoch. The JIT cache pins the obarray's generation and the heap's
//! identity (`cache::sync_cache_to_obarray`), so no leaf outlives them.
//!
//! # GC
//!
//! A store into a symbol cell or a forwarder takes the shim while the heap's
//! barrier window is ALL (a concurrent mark, owner tracking): the shim
//! brackets the chunk seqlock and logs the SATB pre-image. A store into a
//! BLV cons is inline only outside the window (P0.7c's owner test), or into
//! a dumped default cell the compile entered into the dump remembered set
//! ahead of time (`TaggedHeap::remember_mapped_cons_ahead_of_writes`) while
//! the window is not ALL. Specpdl entries are written from the probed
//! templates (`jit_layout::let_layout`), and the bind stack is kept exactly
//! as the shims keep it, so deopt, OSR, handler unwinding and the backtrace
//! walkers see the entries the shims would have pushed.
//!
//! JIT only: nothing here is emitted for an AOT leaf, and the shim set is
//! unchanged, so the AOT ABI is untouched.

use super::jit_layout::heap::{HEAP_JIT_BARRIER_LEN, HEAP_JIT_BARRIER_LO};
use super::jit_layout::{
    BLV_ALIST_EPOCH_OFFSET, BLV_DEFCELL_OFFSET, BLV_FOUND_OFFSET, BLV_FWD_OFFSET,
    BLV_LOCAL_IF_SET_OFFSET, BLV_VALCELL_OFFSET, BLV_WHERE_BUF_ID_OFFSET, CONS_CDR_OFFSET,
    CONTEXT_CURRENT_BUFFER_RAW_OFFSET, CONTEXT_JIT_BIND_STACK_OFFSET, CONTEXT_SPECPDL_OFFSET,
    EntryTemplate, LISP_BOOL_FWD_VALUE_OFFSET, LISP_INT_FWD_VALUE_OFFSET,
    LISP_KBOARD_OBJ_FWD_VALUE_OFFSET, LISP_OBJ_FWD_VALUE_OFFSET, LISP_SYMBOL_FLAGS_OFFSET,
    LISP_SYMBOL_VAL_OFFSET, LetLayout, SYMBOL_FLAGS_REDIRECT_MASK, VecOffsets,
    blv_alist_epoch_addr, jit_bind_stack_vec_offsets, let_layout, specpdl_vec_offsets,
};
use super::lowering::{
    CondRoots, PendingDispatch, SlotRep, band_imm_p, emit_cond_residual_roots_post,
    emit_model_roots_pre, iadd_imm_p, icmp_imm_p, imm64, ishl_imm_p, rootwin_carry_meet,
    rootwin_carry_snapshot, signal_target_for_site,
};
use super::*;
use crate::emacs_core::eval::SpecBinding;
use crate::emacs_core::forward::{LispFwd, LispFwdType};
use crate::emacs_core::symbol::{
    SYMCELL_INLINE_WRITE_MASK, SymbolRedirect, symcell_inline_write_value,
};
use std::cell::{Cell, RefCell};

/// The most bindings one inline `unbind` pops (a `let` or `let*` of up to
/// four specials); a longer `unbind` takes the shim.
const MAX_UNBIND: usize = 4;

/// log2 of a specpdl entry's size: generated code indexes the specpdl by
/// shifting a depth.
const ENTRY_SHIFT: i64 = std::mem::size_of::<SpecBinding>().trailing_zeros() as i64;
const _: () = assert!(std::mem::size_of::<SpecBinding>().is_power_of_two());

/// A cons's cdr, from the tagged cons word.
const TAGGED_CONS_CDR: usize = CONS_CDR_OFFSET - TAG_CONS;
const _: () = assert!(CONS_CDR_OFFSET >= TAG_CONS);

// ---------------------------------------------------------------------------
// The compile environment
// ---------------------------------------------------------------------------

/// What a compile needs from the running `Context` to classify a variable:
/// its obarray and its runtime projection mask. Raw pointers, valid for the
/// [`CompileEnvScope`] that set them.
#[derive(Clone, Copy)]
struct CompileEnv {
    obarray: *const Obarray,
    projection: *const [u64],
}

thread_local! {
    static ENV: Cell<Option<CompileEnv>> = const { Cell::new(None) };
    /// The function being lowered: for each `unbind` pc, the symbols its
    /// bindings bound, top first (`None`: not a `varbind`, or not the same
    /// symbol on every path). Set by [`begin_function`].
    static UNBIND_SITES: RefCell<HashMap<usize, SmallVec<[Option<u32>; MAX_UNBIND]>>> =
        RefCell::new(HashMap::new());
}

/// Lends the compiles inside it the running context's obarray and
/// projection mask (the JIT cache's compile entries hold one while they
/// compile). Without one, nothing is inlined.
#[must_use = "the environment lasts as long as the scope"]
pub(crate) struct CompileEnvScope {
    prev: Option<CompileEnv>,
}

impl CompileEnvScope {
    /// Enter CTX's environment (nothing when CTX is null or the knob is off).
    /// CTX must stay alive and its obarray and mask unmoved while the scope
    /// lives: the dormant seam-provided context of a compile.
    pub(crate) fn enter(ctx: *const Context) -> Self {
        let prev = ENV.with(Cell::get);
        let env = (!ctx.is_null() && jit_inline_vars().any()).then(|| {
            // SAFETY: the caller's contract above.
            let ctx = unsafe { &*ctx };
            CompileEnv {
                obarray: std::ptr::from_ref(&ctx.obarray),
                projection: std::ptr::from_ref(ctx.runtime_projection_mask()),
            }
        });
        ENV.with(|e| e.set(env));
        Self { prev }
    }
}

impl Drop for CompileEnvScope {
    fn drop(&mut self) {
        ENV.with(|e| e.set(self.prev));
    }
}

fn with_env<R>(f: impl FnOnce(&Obarray, &[u64]) -> Option<R>) -> Option<R> {
    let env = ENV.with(Cell::get)?;
    // SAFETY: `CompileEnvScope::enter`'s contract.
    f(unsafe { &*env.obarray }, unsafe { &*env.projection })
}

// ---------------------------------------------------------------------------
// Classification
// ---------------------------------------------------------------------------

/// A forwarder that holds its own value (not a per-buffer slot).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FwdKind {
    Obj,
    Bool,
    Int,
    Kboard,
}

impl FwdKind {
    fn of(ty: LispFwdType) -> Option<Self> {
        match ty {
            LispFwdType::Obj => Some(Self::Obj),
            LispFwdType::Bool => Some(Self::Bool),
            LispFwdType::Int => Some(Self::Int),
            LispFwdType::KboardObj => Some(Self::Kboard),
            LispFwdType::BufferObj => None,
        }
    }

    /// Where the descriptor keeps its value.
    fn value_offset(self) -> usize {
        match self {
            Self::Obj => LISP_OBJ_FWD_VALUE_OFFSET,
            Self::Bool => LISP_BOOL_FWD_VALUE_OFFSET,
            Self::Int => LISP_INT_FWD_VALUE_OFFSET,
            Self::Kboard => LISP_KBOARD_OBJ_FWD_VALUE_OFFSET,
        }
    }
}

/// What a variable was at compile time: which fast path its sites get.
#[derive(Clone, Copy, Debug)]
pub(crate) enum VarShape {
    /// A plain value cell -- or any shape no fast path takes (an alias, a
    /// per-buffer slot, an empty slot): the plain fast path refuses those
    /// by their redirect at run time.
    Plain,
    /// A buffer-local variable (`SYMBOL_LOCALIZED`).
    Localized {
        /// Its `LispBufferLocalValue`.
        blv: usize,
        /// The record's forwarder word as compiled against (0: none): a
        /// write re-checks it, since it decides the type rule.
        fwd: usize,
        /// The forwarder's type rule (`store_symval_forwarding`).
        rule: Option<FwdKind>,
        /// The default cell's bits when it is a dumped cons the compile put
        /// in the dump remembered set (see the module docs).
        remembered_defcell: Option<usize>,
    },
    /// A forwarder that holds its own value (`SYMBOL_FORWARDED`).
    Forwarded { desc: usize, kind: FwdKind },
}

/// One variable an op names, classified at compile time.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VarSite {
    sym: u32,
    /// The symbol's slot.
    cell: usize,
    shape: VarShape,
    /// In the runtime projection mask (`runtime_binding_has_projection`).
    projected: bool,
    /// A per-buffer slot's symbol (`lookup_buffer_slot_by_sym_id`).
    buffer_slot: bool,
}

fn classify(sym: u32) -> Option<VarSite> {
    with_env(|obarray, mask| {
        let id = SymId(sym);
        let cell = obarray.jit_symbol_cell_addr(id)?;
        let shape = match obarray.get_by_id(id) {
            Some(symbol) => match symbol.redirect() {
                SymbolRedirect::Localized => {
                    // SAFETY: `Localized` selects the BLV arm, a record the
                    // obarray owns for its life.
                    let blv_ptr = unsafe { symbol.val.blv };
                    let blv = unsafe { &*blv_ptr };
                    match blv.fwd {
                        Some(fwd) if FwdKind::of(fwd.ty).is_none() => VarShape::Plain,
                        fwd => VarShape::Localized {
                            blv: blv_ptr as usize,
                            fwd: fwd.map_or(0, |f| std::ptr::from_ref::<LispFwd>(f) as usize),
                            rule: fwd.and_then(|f| FwdKind::of(f.ty)),
                            remembered_defcell: None,
                        },
                    }
                }
                SymbolRedirect::Forwarded => match symbol.forwarded_descriptor() {
                    Some(fwd) => match FwdKind::of(fwd.ty) {
                        Some(kind) => VarShape::Forwarded {
                            desc: std::ptr::from_ref::<LispFwd>(fwd) as usize,
                            kind,
                        },
                        None => VarShape::Plain,
                    },
                    None => VarShape::Plain,
                },
                SymbolRedirect::Plainval | SymbolRedirect::Varalias => VarShape::Plain,
            },
            None => VarShape::Plain,
        };
        let bit = sym as usize;
        let projected = mask
            .get(bit / 64)
            .is_some_and(|word| word & (1 << (bit % 64)) != 0);
        Some(VarSite {
            sym,
            cell,
            shape,
            projected,
            buffer_slot: crate::buffer::buffer::lookup_buffer_slot_by_sym_id(id).is_some(),
        })
    })
}

/// SITE, with a dumped default cell entered into the dump remembered set
/// when it is one: the compile of a site that may store into the default.
fn with_remembered_defcell(mut site: VarSite) -> VarSite {
    if let VarShape::Localized {
        blv,
        ref mut remembered_defcell,
        ..
    } = site.shape
    {
        // SAFETY: a live BLV record (see `classify`).
        let defcell =
            unsafe { (*(blv as *const crate::emacs_core::symbol::LispBufferLocalValue)).defcell };
        let remembered = crate::tagged::gc::with_tagged_heap(|heap| {
            heap.remember_mapped_cons_ahead_of_writes(defcell)
        });
        *remembered_defcell = remembered.then_some(defcell.bits());
    }
    site
}

/// The specpdl and bind-stack layouts inline binds need; `None` (a probe
/// failed) turns them off.
#[derive(Clone, Copy)]
pub(crate) struct SpecLayout {
    spdl: VecOffsets,
    jbs: VecOffsets,
    lets: LetLayout,
    /// `LetLocal` and `LetDefault` keep their header and both words at the
    /// same offsets, so one store sequence (or load) serves both.
    local_default_share: bool,
}

fn spec_layout() -> Option<SpecLayout> {
    let lets = let_layout()?;
    Some(SpecLayout {
        spdl: specpdl_vec_offsets()?,
        jbs: jit_bind_stack_vec_offsets()?,
        lets,
        local_default_share: lets.let_local.header_offset == lets.let_default.header_offset
            && lets.let_local.fields[..2] == lets.let_default.fields[..2],
    })
}

/// The site a `varref` of SYM gets, if the knob inlines reads.
pub(crate) fn read_site(sym: u32) -> Option<VarSite> {
    if !jit_inline_vars().read {
        return None;
    }
    classify(sym)
}

/// The site a `varset` of SYM gets, if the knob inlines writes and SYM is a
/// write the Rust tiers would take without republishing: not in the
/// projection mask (`try_set_plain_variable`, `try_set_var_cached`), and
/// not a forwarded per-buffer slot's symbol.
pub(crate) fn set_site(sym: u32) -> Option<VarSite> {
    if !jit_inline_vars().set {
        return None;
    }
    let site = classify(sym)?;
    if site.projected || (site.buffer_slot && matches!(site.shape, VarShape::Forwarded { .. })) {
        return None;
    }
    Some(with_remembered_defcell(site))
}

/// A `varbind` site: the variable and the layouts.
pub(crate) struct BindSite {
    site: VarSite,
    layout: SpecLayout,
}

/// The site a `varbind` of SYM gets, if the knob inlines binds and the
/// layouts probed. A keyboard forwarder is bound by the general path (GNU
/// records its keyboard, `where.kbd`).
pub(crate) fn bind_site(sym: u32) -> Option<BindSite> {
    if !jit_inline_vars().bind {
        return None;
    }
    let layout = spec_layout()?;
    let site = classify(sym)?;
    match site.shape {
        VarShape::Forwarded {
            kind: FwdKind::Kboard,
            ..
        } => return None,
        VarShape::Localized { .. } if !layout.local_default_share => return None,
        _ => {}
    }
    Some(BindSite {
        site: with_remembered_defcell(site),
        layout,
    })
}

/// An `unbind` site: its bindings' variables, top first.
pub(crate) struct UnbindPlan {
    sites: SmallVec<[VarSite; MAX_UNBIND]>,
    layout: SpecLayout,
}

/// The plan an `unbind N` at PC gets, if the knob inlines binds, N is at
/// most [`MAX_UNBIND`] and every binding it pops is a `varbind` of one known
/// symbol on every path. A forwarded binding of a projected symbol makes
/// the whole op take the shim (its restore republishes).
pub(crate) fn unbind_plan(pc: usize, n: u16) -> Option<UnbindPlan> {
    if !jit_inline_vars().bind || n == 0 || n as usize > MAX_UNBIND {
        return None;
    }
    let layout = spec_layout()?;
    let syms = UNBIND_SITES.with(|s| s.borrow().get(&pc).cloned())?;
    if syms.len() != n as usize {
        return None;
    }
    let mut sites = SmallVec::new();
    for sym in syms {
        let site = classify(sym?)?;
        match site.shape {
            VarShape::Forwarded { .. } if site.projected => return None,
            VarShape::Localized { .. } if !layout.local_default_share => return None,
            _ => {}
        }
        sites.push(with_remembered_defcell(site));
    }
    Some(UnbindPlan { sites, layout })
}

// ---------------------------------------------------------------------------
// The static binding sites of each `unbind`
// ---------------------------------------------------------------------------

/// Start lowering a function: find, for each `unbind`, the symbols of the
/// bindings it pops (only when binds may be inlined).
pub(crate) fn begin_function(ops: &[Op], constants: &[Value], cfg: &Cfg, aot: bool) {
    UNBIND_SITES.with(|sites| {
        let mut sites = sites.borrow_mut();
        sites.clear();
        if !aot
            && jit_inline_vars().bind
            && ENV.with(Cell::get).is_some()
            && ops.iter().any(|op| matches!(op, Op::Unbind(_)))
        {
            *sites = unbind_sites(ops, constants, cfg);
        }
    });
}

/// Meet two binding-site stacks position by position: a position keeps its
/// symbol only where both agree. `true` if `into` changed.
fn meet_sites(into: &mut [Option<u32>], other: &[Option<u32>]) -> bool {
    let mut changed = false;
    for (a, b) in into.iter_mut().zip(other) {
        if a.is_some() && *a != *b {
            *a = None;
            changed = true;
        }
    }
    changed
}

/// A forward walk of the CFG `analyze_cfg` built, with the binding stack as
/// its state: a `varbind` pushes its symbol, a `save-*` or
/// `unwind-protect` record pushes `None`, an `unbind N` pops N and records
/// them. The byte compiler's structured binding keeps the stack depth the
/// same on every path (`analyze_cfg` checks it); where two paths bound
/// different symbols the position is `None`. Empty when anything is off
/// the expected shape (then no `unbind` is inlined).
fn unbind_sites(
    ops: &[Op],
    constants: &[Value],
    cfg: &Cfg,
) -> HashMap<usize, SmallVec<[Option<u32>; MAX_UNBIND]>> {
    let n = ops.len();
    let leaders = &cfg.leaders;
    let next_leader = |i: usize| {
        let k = leaders.partition_point(|&l| l <= i);
        leaders.get(k).copied().unwrap_or(n)
    };
    let mut entry: HashMap<usize, Vec<Option<u32>>> = HashMap::new();
    let mut out: HashMap<usize, SmallVec<[Option<u32>; MAX_UNBIND]>> = HashMap::new();
    let mut work = vec![0usize];
    entry.insert(0, Vec::new());
    let push = |entry: &mut HashMap<usize, Vec<Option<u32>>>,
                work: &mut Vec<usize>,
                target: usize,
                state: &[Option<u32>]|
     -> bool {
        match entry.get_mut(&target) {
            None => {
                entry.insert(target, state.to_vec());
                work.push(target);
                true
            }
            Some(old) if old.len() != state.len() => false,
            Some(old) => {
                if meet_sites(old, state) {
                    work.push(target);
                }
                true
            }
        }
    };
    while let Some(l) = work.pop() {
        let mut state = entry[&l].clone();
        let end = next_leader(l);
        let mut falls_through = true;
        let mut targets: SmallVec<[usize; 4]> = SmallVec::new();
        for (pc, op) in ops.iter().enumerate().take(end).skip(l) {
            match op {
                Op::VarBind(idx) => state.push(const_sym_id(constants, *idx).ok()),
                Op::SaveCurrentBuffer
                | Op::SaveExcursion
                | Op::SaveRestriction
                | Op::UnwindProtectPop => state.push(None),
                Op::Unbind(k) => {
                    let k = *k as usize;
                    if k > state.len() {
                        return HashMap::new();
                    }
                    let top: SmallVec<[Option<u32>; MAX_UNBIND]> =
                        state.iter().rev().take(k).copied().collect();
                    match out.get_mut(&pc) {
                        Some(old) if old.len() == top.len() => {
                            meet_sites(old, &top);
                        }
                        Some(_) => return HashMap::new(),
                        None => {
                            out.insert(pc, top);
                        }
                    }
                    state.truncate(state.len() - k);
                }
                Op::Goto(t) => {
                    targets.push(*t as usize);
                    falls_through = false;
                    break;
                }
                Op::GotoIfNil(t)
                | Op::GotoIfNotNil(t)
                | Op::GotoIfNilElsePop(t)
                | Op::GotoIfNotNilElsePop(t)
                | Op::PushConditionCase(t)
                | Op::PushConditionCaseRaw(t)
                | Op::PushCatch(t) => {
                    // A handler is entered with the binding stack of its push.
                    targets.push(*t as usize);
                    targets.push(end);
                    falls_through = false;
                    break;
                }
                Op::Switch => {
                    targets.extend(
                        cfg.switch_targets
                            .get(&pc)
                            .into_iter()
                            .flatten()
                            .map(|&(_, t)| t),
                    );
                    targets.push(end);
                    falls_through = false;
                    break;
                }
                Op::Return | Op::Throw => {
                    falls_through = false;
                    break;
                }
                _ => {}
            }
        }
        if falls_through {
            targets.push(end);
        }
        for t in targets {
            if t < n && !push(&mut entry, &mut work, t, &state) {
                return HashMap::new();
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Emission helpers
// ---------------------------------------------------------------------------

/// Whether the baseline lowers OP with an inline variable write that reads
/// the heap's barrier window (`heap_inline::hoist_heap_ptr` counts these
/// with its own sites). JIT only.
pub(crate) fn op_reads_heap_window(op: &Op) -> bool {
    let knob = jit_inline_vars();
    match op {
        Op::VarSet(_) => knob.set,
        Op::VarBind(_) | Op::Unbind(_) => knob.bind,
        _ => false,
    }
}

/// Which inline op a site is (the test census).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum InlineVarOp {
    Read = 0,
    Set = 1,
    Bind = 2,
    Unbind = 3,
}

#[cfg(test)]
thread_local! {
    static SITES_EMITTED: [Cell<u32>; 4] = const { [const { Cell::new(0) }; 4] };
}

fn note_site(op: InlineVarOp) {
    #[cfg(test)]
    SITES_EMITTED.with(|s| s[op as usize].set(s[op as usize].get() + 1));
    #[cfg(not(test))]
    let _ = op;
}

/// Inline sites of OP emitted on this thread since the last reset (tests).
#[cfg(test)]
pub(crate) fn inline_var_sites(op: InlineVarOp) -> u32 {
    SITES_EMITTED.with(|s| s[op as usize].get())
}

/// Forget the census (tests).
#[cfg(test)]
pub(crate) fn reset_inline_var_sites() {
    SITES_EMITTED.with(|s| s.iter().for_each(|c| c.set(0)));
}

/// Run F with CTX's compile environment entered: the compiles of a test
/// that lowers bytecode directly (`lower_leaf`) instead of through the JIT
/// cache.
#[cfg(test)]
pub(crate) fn with_compile_env_for_test<R>(ctx: &Context, f: impl FnOnce() -> R) -> R {
    let _scope = CompileEnvScope::enter(ctx);
    f()
}

fn trusted() -> MemFlagsData {
    MemFlagsData::trusted()
}

fn load_word(fb: &mut FunctionBuilder, base: ClifValue, offset: usize) -> ClifValue {
    fb.ins().load(types::I64, trusted(), base, offset as i32)
}

fn store_word(fb: &mut FunctionBuilder, value: ClifValue, base: ClifValue, offset: usize) {
    fb.ins().store(trusted(), value, base, offset as i32);
}

fn baked(fb: &mut FunctionBuilder, address: usize) -> ClifValue {
    fb.ins().iconst(types::I64, address as i64)
}

fn eq(fb: &mut FunctionBuilder, a: ClifValue, b: ClifValue) -> ClifValue {
    fb.ins().icmp(IntCC::Equal, a, b)
}

fn eq_imm(fb: &mut FunctionBuilder, a: ClifValue, k: i64) -> ClifValue {
    icmp_imm_p(fb, IntCC::Equal, a, k)
}

fn ne_imm(fb: &mut FunctionBuilder, a: ClifValue, k: i64) -> ClifValue {
    icmp_imm_p(fb, IntCC::NotEqual, a, k)
}

/// The conjunction of CONDS (`icmp` truth values).
fn all(fb: &mut FunctionBuilder, conds: &[ClifValue]) -> ClifValue {
    let mut acc = conds[0];
    for &c in &conds[1..] {
        acc = fb.ins().band(acc, c);
    }
    acc
}

/// Continue in a fresh block when OK holds, else branch to SLOW.
fn guard(fb: &mut FunctionBuilder, ok: ClifValue, slow: Block) {
    let next = fb.create_block();
    fb.ins().brif(ok, next, &[], slow, &[]);
    fb.switch_to_block(next);
    fb.seal_block(next);
}

fn load_vmctx(fb: &mut FunctionBuilder, rt: &RtCtx) -> ClifValue {
    fb.use_var(rt.vmctx_var)
}

/// Whether the symbol at CELL has REDIRECT (a read's test).
fn redirect_is(fb: &mut FunctionBuilder, cell: ClifValue, redirect: SymbolRedirect) -> ClifValue {
    let flags = fb
        .ins()
        .uload8(types::I64, trusted(), cell, LISP_SYMBOL_FLAGS_OFFSET as i32);
    let bits = band_imm_p(fb, flags, SYMBOL_FLAGS_REDIRECT_MASK as i64);
    eq_imm(fb, bits, redirect as i64)
}

/// Whether the symbol at CELL may be written without the general path, as a
/// cell of REDIRECT: [`SYMCELL_INLINE_WRITE_MASK`] over its write window
/// (untrapped, not flag-projected, interned), the one test every inline and
/// cached symbol-cell write makes.
fn window_is(fb: &mut FunctionBuilder, cell: ClifValue, redirect: SymbolRedirect) -> ClifValue {
    let window = fb
        .ins()
        .uload16(types::I64, trusted(), cell, LISP_SYMBOL_FLAGS_OFFSET as i32);
    let bits = band_imm_p(fb, window, i64::from(SYMCELL_INLINE_WRITE_MASK));
    eq_imm(fb, bits, i64::from(symcell_inline_write_value(redirect)))
}

/// The heap's published barrier window (`JitHeapState`): its length, loaded
/// at once, and the heap its start is loaded from when a cons store needs
/// it (a symbol-cell or forwarder store tests the length only).
#[derive(Clone, Copy)]
struct Window {
    heap: ClifValue,
    len: ClifValue,
}

fn barrier_window(fb: &mut FunctionBuilder, rt: &RtCtx) -> Window {
    let heap = super::heap_inline::heap_ptr(fb, rt);
    Window {
        heap,
        len: load_word(fb, heap, HEAP_JIT_BARRIER_LEN),
    }
}

/// The window is not ALL: no concurrent mark and no owner tracking, so a
/// symbol cell or forwarder may be stored without the seqlock or SATB note.
fn not_marking(fb: &mut FunctionBuilder, window: Window) -> ClifValue {
    ne_imm(fb, window.len, -1)
}

/// A plain store into the tagged cons CONS needs no barrier: its owner lies
/// outside the window, or it is REMEMBERED (a dumped cell already in the
/// remembered set) and the window is not ALL.
fn cons_store_ok(
    fb: &mut FunctionBuilder,
    window: Window,
    cons: ClifValue,
    remembered: Option<usize>,
) -> ClifValue {
    let lo = load_word(fb, window.heap, HEAP_JIT_BARRIER_LO);
    let owner = iadd_imm_p(fb, cons, -(TAG_CONS as i64));
    let offset = fb.ins().isub(owner, lo);
    let outside = fb
        .ins()
        .icmp(IntCC::UnsignedGreaterThanOrEqual, offset, window.len);
    match remembered {
        None => outside,
        Some(bits) => {
            let is_remembered = eq_imm(fb, cons, bits as i64);
            let open = not_marking(fb, window);
            let skip = fb.ins().band(is_remembered, open);
            fb.ins().bor(outside, skip)
        }
    }
}

/// The current buffer's raw id (0 for none).
fn current_buffer(fb: &mut FunctionBuilder, rt: &RtCtx) -> ClifValue {
    let vmctx = load_vmctx(fb, rt);
    load_word(fb, vmctx, CONTEXT_CURRENT_BUFFER_RAW_OFFSET)
}

/// The BLV at BLV is loaded for the buffer whose raw id is CUR at the
/// current epoch (`LispSymbol::blv_cache_hit`). CUR 0 (no buffer) never
/// matches: `where_buf_id` is a buffer id or `NO_WHERE_BUF`.
fn blv_hit(fb: &mut FunctionBuilder, blv: ClifValue, cur: ClifValue) -> ClifValue {
    let owner = load_word(fb, blv, BLV_WHERE_BUF_ID_OFFSET);
    let loaded_at = load_word(fb, blv, BLV_ALIST_EPOCH_OFFSET);
    let epoch_addr = baked(fb, blv_alist_epoch_addr());
    let epoch = load_word(fb, epoch_addr, 0);
    let mine = eq(fb, owner, cur);
    let fresh = eq(fb, loaded_at, epoch);
    fb.ins().band(mine, fresh)
}

fn fixnum_p(fb: &mut FunctionBuilder, v: ClifValue) -> ClifValue {
    let tag = band_imm_p(fb, v, FIXNUM_CHECK_MASK as i64);
    eq_imm(fb, tag, FIXNUM_CHECK_VALUE as i64)
}

/// `t` when V is non-nil, else nil (`Value::bool_val (!NILP (v))`).
fn canonical_bool(fb: &mut FunctionBuilder, v: ClifValue) -> ClifValue {
    let truthy = ne_imm(fb, v, Value::NIL.bits() as i64);
    let t = imm64(fb, Value::T.bits() as i64);
    let nil = imm64(fb, Value::NIL.bits() as i64);
    fb.ins().select(truthy, t, nil)
}

/// A buffer-local value's forwarder type rule (`var_fast::forward_rule`):
/// the condition the inline store needs (`None`: none) and the value to
/// store. A Boolean canonicalises; an integer slot takes only a fixnum
/// inline (a bignum's range check and a non-integer's signal are the
/// shim's).
fn blv_rule(
    fb: &mut FunctionBuilder,
    rule: Option<FwdKind>,
    v: ClifValue,
) -> (Option<ClifValue>, ClifValue) {
    match rule {
        None | Some(FwdKind::Obj) | Some(FwdKind::Kboard) => (None, v),
        Some(FwdKind::Bool) => (None, canonical_bool(fb, v)),
        Some(FwdKind::Int) => (Some(fixnum_p(fb, v)), v),
    }
}

/// `LispFwd::load` of the descriptor at DESC.
fn fwd_load(fb: &mut FunctionBuilder, desc: ClifValue, kind: FwdKind) -> ClifValue {
    match kind {
        FwdKind::Bool => {
            let flag = fb.ins().uload8(
                types::I64,
                trusted(),
                desc,
                LISP_BOOL_FWD_VALUE_OFFSET as i32,
            );
            let set = ne_imm(fb, flag, 0);
            let t = imm64(fb, Value::T.bits() as i64);
            let nil = imm64(fb, Value::NIL.bits() as i64);
            fb.ins().select(set, t, nil)
        }
        _ => load_word(fb, desc, kind.value_offset()),
    }
}

/// `LispFwd::commit (store (v))` into the descriptor at DESC, for a V its
/// rule accepts inline (an integer slot's caller checked for a fixnum).
fn fwd_store(fb: &mut FunctionBuilder, desc: ClifValue, kind: FwdKind, v: ClifValue) {
    match kind {
        FwdKind::Bool => {
            let flag = ne_imm(fb, v, Value::NIL.bits() as i64);
            fb.ins()
                .store(trusted(), flag, desc, LISP_BOOL_FWD_VALUE_OFFSET as i32);
        }
        _ => store_word(fb, v, desc, kind.value_offset()),
    }
}

// ---------------------------------------------------------------------------
// varref
// ---------------------------------------------------------------------------

/// `varref` of SITE inline -- GNU `Bvarref`: defines RES as the value and
/// jumps to CONT, or branches to SLOW (the caller's shim block).
///
/// - Plain: the cell's value when bound (and, for a dedicated buffer-local
///   such as `buffer-undo-list`, not nil) -- `neovm_jit_varref`'s first
///   branch, from the baked cell instead of the obarray spine.
/// - Buffer-local: the loaded cell's cdr when the cache is loaded for the
///   current buffer at the current epoch and the value is not void --
///   `Context::read_var_cached`'s `read_localized_cached`.
/// - Forwarded: the descriptor's value (`LispFwd::load`); a void object
///   slot takes the shim.
pub(crate) fn emit_varref_fast(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    site: &VarSite,
    res: Variable,
    slow: Block,
    cont: Block,
) {
    fb.set_cold_block(slow);
    let cell = baked(fb, site.cell);
    let (ok, value) = match site.shape {
        VarShape::Plain => {
            let plain = redirect_is(fb, cell, SymbolRedirect::Plainval);
            let val = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let bound = ne_imm(fb, val, Value::UNBOUND.bits() as i64);
            let mut conds: SmallVec<[ClifValue; 3]> = smallvec::smallvec![plain, bound];
            if crate::buffer::buffer::DedicatedBufferLocal::from_sym_id(SymId(site.sym)).is_some() {
                conds.push(ne_imm(fb, val, Value::NIL.bits() as i64));
            }
            (all(fb, &conds), val)
        }
        VarShape::Localized { blv, .. } => {
            let localized = redirect_is(fb, cell, SymbolRedirect::Localized);
            let val = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let blv = baked(fb, blv);
            let same = eq(fb, val, blv);
            let cur = current_buffer(fb, rt);
            let hit = blv_hit(fb, blv, cur);
            let shape_ok = all(fb, &[localized, same, hit]);
            // Only a live BLV's loaded cell is dereferenced.
            guard(fb, shape_ok, slow);
            let valcell = load_word(fb, blv, BLV_VALCELL_OFFSET);
            let value = load_word(fb, valcell, TAGGED_CONS_CDR);
            (ne_imm(fb, value, Value::UNBOUND.bits() as i64), value)
        }
        VarShape::Forwarded { desc, kind } => {
            let forwarded = redirect_is(fb, cell, SymbolRedirect::Forwarded);
            let val = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let desc = baked(fb, desc);
            let same = eq(fb, val, desc);
            let value = fwd_load(fb, desc, kind);
            let mut conds: SmallVec<[ClifValue; 3]> = smallvec::smallvec![forwarded, same];
            if matches!(kind, FwdKind::Obj | FwdKind::Kboard) {
                conds.push(ne_imm(fb, value, Value::UNBOUND.bits() as i64));
            }
            (all(fb, &conds), value)
        }
    };
    let fast = fb.create_block();
    fb.ins().brif(ok, fast, &[], slow, &[]);
    fb.switch_to_block(fast);
    fb.seal_block(fast);
    fb.def_var(res, value);
    fb.ins().jump(cont, &[]);
    note_site(InlineVarOp::Read);
}

// ---------------------------------------------------------------------------
// varset
// ---------------------------------------------------------------------------

/// `varset` of SITE to VAL inline -- GNU `Bvarset` / `set_internal (SET)` on
/// a cache hit; jumps to CONT after the store, or branches to SLOW having
/// stored nothing.
///
/// - Plain: `try_set_plain_variable` (a write-window match, no mark).
/// - Buffer-local: `try_set_var_cached`'s `set_localized_cached` -- the
///   cache loaded here, the buffer's own binding (`found`), or no binding
///   and not `local_if_set` (the default, `valcell == defcell`); the
///   forwarder unchanged since compile time and its type rule; a cons the
///   barrier need not see.
/// - Forwarded: `set_forwarded_cached` (no mark; an integer slot takes a
///   fixnum only).
fn emit_varset_fast(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    site: &VarSite,
    val: ClifValue,
    slow: Block,
    cont: Block,
) {
    let cell = baked(fb, site.cell);
    match site.shape {
        VarShape::Plain => {
            let writable = window_is(fb, cell, SymbolRedirect::Plainval);
            let window = barrier_window(fb, rt);
            let open = not_marking(fb, window);
            let ok = fb.ins().band(writable, open);
            guard(fb, ok, slow);
            store_word(fb, val, cell, LISP_SYMBOL_VAL_OFFSET);
        }
        VarShape::Localized {
            blv,
            fwd,
            rule,
            remembered_defcell,
        } => {
            let writable = window_is(fb, cell, SymbolRedirect::Localized);
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let blv = baked(fb, blv);
            let same = eq(fb, word, blv);
            let cur = current_buffer(fb, rt);
            let hit = blv_hit(fb, blv, cur);
            let fwd_word = load_word(fb, blv, BLV_FWD_OFFSET);
            let fwd_same = eq_imm(fb, fwd_word, fwd as i64);
            // `local_if_set | found << 8`.
            let flags =
                fb.ins()
                    .uload16(types::I64, trusted(), blv, BLV_LOCAL_IF_SET_OFFSET as i32);
            let found = icmp_imm_p(fb, IntCC::UnsignedGreaterThanOrEqual, flags, 0x100);
            let valcell = load_word(fb, blv, BLV_VALCELL_OFFSET);
            let defcell = load_word(fb, blv, BLV_DEFCELL_OFFSET);
            let neither = eq_imm(fb, flags, 0);
            let default_loaded = eq(fb, valcell, defcell);
            let to_default = fb.ins().band(neither, default_loaded);
            let own_cell = fb.ins().bor(found, to_default);
            let window = barrier_window(fb, rt);
            let plain_store = (!rt.generational_enabled())
                .then(|| cons_store_ok(fb, window, valcell, remembered_defcell));
            let (rule_ok, stored) = blv_rule(fb, rule, val);
            let mut conds: SmallVec<[ClifValue; 8]> =
                smallvec::smallvec![writable, same, hit, fwd_same, own_cell];
            conds.extend(plain_store);
            conds.extend(rule_ok);
            let ok = all(fb, &conds);
            guard(fb, ok, slow);
            if rt.generational_enabled() {
                let owner = iadd_imm_p(fb, valcell, -(TAG_CONS as i64));
                super::heap_inline::emit_cons_store_barrier(fb, rt, owner, stored, slow);
            }
            store_word(fb, stored, valcell, TAGGED_CONS_CDR);
        }
        VarShape::Forwarded { desc, kind } => {
            let writable = window_is(fb, cell, SymbolRedirect::Forwarded);
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let desc = baked(fb, desc);
            let same = eq(fb, word, desc);
            let window = barrier_window(fb, rt);
            let open = not_marking(fb, window);
            let mut conds: SmallVec<[ClifValue; 4]> = smallvec::smallvec![writable, same, open];
            if kind == FwdKind::Int {
                conds.push(fixnum_p(fb, val));
            }
            let ok = all(fb, &conds);
            guard(fb, ok, slow);
            fwd_store(fb, desc, kind, val);
        }
    }
    fb.ins().jump(cont, &[]);
    note_site(InlineVarOp::Set);
}

/// `Op::VarSet` with SITE's fast path, the unchanged `neovm_jit_varset` call
/// (roots, status, signal dispatch) as its slow path. STACK is the residual
/// operand stack (VAL popped).
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_varset(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    site: &VarSite,
    sym_v: ClifValue,
    val: ClifValue,
    stack: &[ClifValue],
    reps: &[SlotRep],
    signal_exit: &mut Option<Block>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
) {
    let slow = fb.create_block();
    let cont = fb.create_block();
    fb.set_cold_block(slow);
    emit_varset_fast(fb, rt, site, val, slow, cont);
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    let carry_fast = rootwin_carry_snapshot();
    let saved = if stack.is_empty() {
        CondRoots::NONE
    } else {
        emit_model_roots_pre(fb, rt, stack, reps)
    };
    let vmctx = load_vmctx(fb, rt);
    let varset = rt.refs.get(fb.func, Shim::Varset);
    let call = fb.ins().call(varset, &[vmctx, sym_v, val]);
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    rootwin_carry_meet(&carry_fast);
    let se = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
    let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, cont, &[], se, &[]);
    fb.switch_to_block(cont);
    fb.seal_block(cont);
}

// ---------------------------------------------------------------------------
// varbind
// ---------------------------------------------------------------------------

/// The specpdl's and bind stack's length and capacity, loaded once.
struct Stacks {
    spdl_len: ClifValue,
    spdl_cap: ClifValue,
    jbs_len: ClifValue,
    jbs_cap: ClifValue,
}

fn load_stacks(fb: &mut FunctionBuilder, rt: &RtCtx, layout: &SpecLayout) -> Stacks {
    let vmctx = load_vmctx(fb, rt);
    Stacks {
        spdl_len: load_word(fb, vmctx, CONTEXT_SPECPDL_OFFSET + layout.spdl.len),
        spdl_cap: load_word(fb, vmctx, CONTEXT_SPECPDL_OFFSET + layout.spdl.cap),
        jbs_len: load_word(fb, vmctx, CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.len),
        jbs_cap: load_word(fb, vmctx, CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.cap),
    }
}

/// Both stacks have room for one more element: a full one takes the shim,
/// which grows it (the fast path never calls).
fn stacks_have_room(fb: &mut FunctionBuilder, s: &Stacks) -> ClifValue {
    let spdl = fb
        .ins()
        .icmp(IntCC::UnsignedLessThan, s.spdl_len, s.spdl_cap);
    let jbs = fb.ins().icmp(IntCC::UnsignedLessThan, s.jbs_len, s.jbs_cap);
    fb.ins().band(spdl, jbs)
}

/// The address of the specpdl entry at DEPTH.
fn entry_at(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    layout: &SpecLayout,
    depth: ClifValue,
) -> ClifValue {
    let vmctx = load_vmctx(fb, rt);
    let base = load_word(fb, vmctx, CONTEXT_SPECPDL_OFFSET + layout.spdl.ptr);
    let offset = ishl_imm_p(fb, depth, ENTRY_SHIFT);
    fb.ins().iadd(base, offset)
}

/// Push the entry TEMPLATE describes, header HEADER and word fields FIELDS,
/// then the bind depth, as `push_specpdl_with` and the `varbind` shim do:
/// the entry is written before the length that publishes it.
fn push_binding(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    layout: &SpecLayout,
    s: &Stacks,
    template: &EntryTemplate,
    header: ClifValue,
    fields: &[ClifValue],
) {
    let entry = entry_at(fb, rt, layout, s.spdl_len);
    store_word(fb, header, entry, template.header_offset as usize);
    for (i, &field) in fields.iter().enumerate() {
        store_word(fb, field, entry, template.field(i) as usize);
    }
    let vmctx = load_vmctx(fb, rt);
    let spdl_len = iadd_imm_p(fb, s.spdl_len, 1);
    store_word(
        fb,
        spdl_len,
        vmctx,
        CONTEXT_SPECPDL_OFFSET + layout.spdl.len,
    );
    let jbs = load_word(fb, vmctx, CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.ptr);
    let slot = ishl_imm_p(fb, s.jbs_len, 3);
    let slot = fb.ins().iadd(jbs, slot);
    store_word(fb, s.spdl_len, slot, 0);
    let jbs_len = iadd_imm_p(fb, s.jbs_len, 1);
    store_word(
        fb,
        jbs_len,
        vmctx,
        CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.len,
    );
}

/// `varbind` of the variable to VAL inline -- GNU `specbind` +
/// `do_specbind` on a cache hit; jumps to CONT after the push, or branches
/// to SLOW having changed nothing. Both stacks need room.
///
/// - Plain: `specbind_plain_untrapped_fast` (write window, no mark): swap
///   the cell, push `Let`.
/// - Forwarded (Obj, Bool, Int): `specbind_cached`'s forwarded arm -- push
///   `Let` with the descriptor's (non-void) value, store the new one; an
///   integer slot takes a fixnum only.
/// - Buffer-local: `specbind_cached`'s `specbind_localized_hit` -- the cache
///   loaded here with a non-void value, the forwarder unchanged, a cons the
///   barrier need not see: push `LetLocal` (the buffer's own binding) or
///   `LetDefault` (the default, `valcell == defcell`) with the loaded value
///   and the current buffer, and store the value through the type rule.
fn emit_varbind_fast(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    bind: &BindSite,
    val: ClifValue,
    slow: Block,
    cont: Block,
) {
    let BindSite { site, layout } = bind;
    let lets = &layout.lets;
    let sym = site.sym;
    let s = load_stacks(fb, rt, layout);
    let room = stacks_have_room(fb, &s);
    let cell = baked(fb, site.cell);
    match site.shape {
        VarShape::Plain => {
            let writable = window_is(fb, cell, SymbolRedirect::Plainval);
            let window = barrier_window(fb, rt);
            let open = not_marking(fb, window);
            let ok = all(fb, &[room, writable, open]);
            guard(fb, ok, slow);
            let old = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            store_word(fb, val, cell, LISP_SYMBOL_VAL_OFFSET);
            let header = imm64(fb, lets.let_.header_with(sym) as i64);
            push_binding(fb, rt, layout, &s, &lets.let_, header, &[old]);
        }
        VarShape::Forwarded { desc, kind } => {
            let writable = window_is(fb, cell, SymbolRedirect::Forwarded);
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let desc = baked(fb, desc);
            let same = eq(fb, word, desc);
            let window = barrier_window(fb, rt);
            let open = not_marking(fb, window);
            let old = fwd_load(fb, desc, kind);
            let mut conds: SmallVec<[ClifValue; 6]> =
                smallvec::smallvec![room, writable, same, open];
            match kind {
                FwdKind::Obj | FwdKind::Kboard => {
                    conds.push(ne_imm(fb, old, Value::UNBOUND.bits() as i64));
                }
                FwdKind::Int => conds.push(fixnum_p(fb, val)),
                FwdKind::Bool => {}
            }
            let ok = all(fb, &conds);
            guard(fb, ok, slow);
            let header = imm64(fb, lets.let_.header_with(sym) as i64);
            push_binding(fb, rt, layout, &s, &lets.let_, header, &[old]);
            fwd_store(fb, desc, kind, val);
        }
        VarShape::Localized {
            blv,
            fwd,
            rule,
            remembered_defcell,
        } => {
            let writable = window_is(fb, cell, SymbolRedirect::Localized);
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let blv = baked(fb, blv);
            let same = eq(fb, word, blv);
            let cur = current_buffer(fb, rt);
            let hit = blv_hit(fb, blv, cur);
            let fwd_word = load_word(fb, blv, BLV_FWD_OFFSET);
            let fwd_same = eq_imm(fb, fwd_word, fwd as i64);
            let shape_ok = all(fb, &[room, writable, same, hit, fwd_same]);
            // Only a live BLV's loaded cell is dereferenced.
            guard(fb, shape_ok, slow);
            let found = fb
                .ins()
                .uload8(types::I64, trusted(), blv, BLV_FOUND_OFFSET as i32);
            let found = ne_imm(fb, found, 0);
            let valcell = load_word(fb, blv, BLV_VALCELL_OFFSET);
            let defcell = load_word(fb, blv, BLV_DEFCELL_OFFSET);
            let default_loaded = eq(fb, valcell, defcell);
            let own_cell = fb.ins().bor(found, default_loaded);
            let old = load_word(fb, valcell, TAGGED_CONS_CDR);
            let bound = ne_imm(fb, old, Value::UNBOUND.bits() as i64);
            let window = barrier_window(fb, rt);
            let plain_store = (!rt.generational_enabled())
                .then(|| cons_store_ok(fb, window, valcell, remembered_defcell));
            let (rule_ok, stored) = blv_rule(fb, rule, val);
            let mut conds: SmallVec<[ClifValue; 5]> = smallvec::smallvec![own_cell, bound];
            conds.extend(plain_store);
            conds.extend(rule_ok);
            let ok = all(fb, &conds);
            guard(fb, ok, slow);
            if rt.generational_enabled() {
                let owner = iadd_imm_p(fb, valcell, -(TAG_CONS as i64));
                super::heap_inline::emit_cons_store_barrier(fb, rt, owner, stored, slow);
            }
            let local = imm64(fb, lets.let_local.header_with(sym) as i64);
            let default = imm64(fb, lets.let_default.header_with(sym) as i64);
            let header = fb.ins().select(found, local, default);
            // `local_default_share`: one store sequence writes either.
            push_binding(fb, rt, layout, &s, &lets.let_local, header, &[old, cur]);
            store_word(fb, stored, valcell, TAGGED_CONS_CDR);
        }
    }
    fb.ins().jump(cont, &[]);
    note_site(InlineVarOp::Bind);
}

/// `Op::VarBind` with BIND's fast path, the unchanged `neovm_jit_varbind`
/// call as its slow path. STACK is the residual operand stack (VAL popped).
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_varbind(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    bind: &BindSite,
    sym_v: ClifValue,
    val: ClifValue,
    stack: &[ClifValue],
    reps: &[SlotRep],
    signal_exit: &mut Option<Block>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
) {
    let slow = fb.create_block();
    let cont = fb.create_block();
    fb.set_cold_block(slow);
    emit_varbind_fast(fb, rt, bind, val, slow, cont);
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    let carry_fast = rootwin_carry_snapshot();
    let vmctx = load_vmctx(fb, rt);
    let saved = if stack.is_empty() {
        CondRoots::NONE
    } else {
        emit_model_roots_pre(fb, rt, stack, reps)
    };
    let varbind = rt.refs.get(fb.func, Shim::Varbind);
    let call = fb.ins().call(varbind, &[vmctx, sym_v, val]);
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    rootwin_carry_meet(&carry_fast);
    let signal = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
    let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, cont, &[], signal, &[]);
    fb.switch_to_block(cont);
    fb.seal_block(cont);
}

// ---------------------------------------------------------------------------
// unbind
// ---------------------------------------------------------------------------

/// One restore an inline `unbind` makes once every entry passed.
enum Restore {
    /// A plain cell's value word.
    Cell { cell: ClifValue, value: ClifValue },
    /// A forwarder's slot.
    Fwd {
        desc: ClifValue,
        kind: FwdKind,
        value: ClifValue,
    },
    /// A BLV cons's cdr.
    Cons { cons: ClifValue, value: ClifValue },
}

/// The checks of one popped entry at ENTRY, bound by SITE, and the restore
/// it makes -- `pop_simple_specpdl_suffix`'s arm for the entry, with the
/// symbol's shape read now (a watcher or local made inside the binding
/// sends it to the general unwinder):
///
/// - Plain: a `Let` of the symbol, write window of a plain cell (the `Let`
///   arm's `swap_plain_untrapped_value_id`).
/// - Forwarded: a `Let` of the symbol, the descriptor still its forwarder, a
///   non-void old value its type rule accepts (`pop_forwarded_let_cached`).
/// - Buffer-local: a `LetLocal` of the symbol for the current buffer whose
///   cache is loaded here with its own binding (`pop_let_local_cached`: the
///   cdr of the loaded cell), or a `LetDefault` (`pop_let_default_cached`,
///   not for a projected symbol: the default cell, through the type rule);
///   a non-void old value; a cons the barrier need not see.
#[allow(clippy::too_many_arguments)]
fn unbind_entry(
    fb: &mut FunctionBuilder,
    lets: &LetLayout,
    site: &VarSite,
    entry: ClifValue,
    cur: Option<ClifValue>,
    window: Window,
    conds: &mut SmallVec<[ClifValue; 16]>,
    generational: bool,
) -> Restore {
    let cell = baked(fb, site.cell);
    let sym = site.sym;
    let is_entry = |fb: &mut FunctionBuilder, template: &EntryTemplate, header: ClifValue| {
        let masked = band_imm_p(fb, header, template.header_mask as i64);
        eq_imm(fb, masked, template.header_with(sym) as i64)
    };
    match site.shape {
        VarShape::Plain => {
            let header = load_word(fb, entry, lets.let_.header_offset as usize);
            conds.push(is_entry(fb, &lets.let_, header));
            conds.push(window_is(fb, cell, SymbolRedirect::Plainval));
            let value = load_word(fb, entry, lets.let_.field(0) as usize);
            Restore::Cell { cell, value }
        }
        VarShape::Forwarded { desc, kind } => {
            let header = load_word(fb, entry, lets.let_.header_offset as usize);
            conds.push(is_entry(fb, &lets.let_, header));
            conds.push(window_is(fb, cell, SymbolRedirect::Forwarded));
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let desc = baked(fb, desc);
            conds.push(eq(fb, word, desc));
            let value = load_word(fb, entry, lets.let_.field(0) as usize);
            conds.push(ne_imm(fb, value, Value::UNBOUND.bits() as i64));
            if kind == FwdKind::Int {
                conds.push(fixnum_p(fb, value));
            }
            Restore::Fwd { desc, kind, value }
        }
        VarShape::Localized {
            blv,
            fwd,
            rule,
            remembered_defcell,
        } => {
            let cur = cur.expect("loaded for a buffer-local binding");
            let header = load_word(fb, entry, lets.let_local.header_offset as usize);
            let is_local = is_entry(fb, &lets.let_local, header);
            conds.push(window_is(fb, cell, SymbolRedirect::Localized));
            let word = load_word(fb, cell, LISP_SYMBOL_VAL_OFFSET);
            let blv = baked(fb, blv);
            conds.push(eq(fb, word, blv));
            // `local_default_share`: both variants keep the old value and
            // the buffer at `let_local`'s offsets.
            let old = load_word(fb, entry, lets.let_local.field(0) as usize);
            conds.push(ne_imm(fb, old, Value::UNBOUND.bits() as i64));
            let valcell = load_word(fb, blv, BLV_VALCELL_OFFSET);
            // LetLocal: made in this buffer, still its loaded binding.
            let buffer = load_word(fb, entry, lets.let_local.field(1) as usize);
            let here = eq(fb, buffer, cur);
            let hit = blv_hit(fb, blv, cur);
            let found = fb
                .ins()
                .uload8(types::I64, trusted(), blv, BLV_FOUND_OFFSET as i32);
            let found = ne_imm(fb, found, 0);
            let local_store = (!generational).then(|| cons_store_ok(fb, window, valcell, None));
            let mut local_conds: SmallVec<[ClifValue; 5]> =
                smallvec::smallvec![is_local, here, hit, found];
            local_conds.extend(local_store);
            let local_ok = all(fb, &local_conds);
            if site.projected {
                // The default's restore republishes a projected symbol.
                conds.push(local_ok);
                return Restore::Cons {
                    cons: valcell,
                    value: old,
                };
            }
            // LetDefault: the default cell, whatever buffer is current.
            let is_default = is_entry(fb, &lets.let_default, header);
            let defcell = load_word(fb, blv, BLV_DEFCELL_OFFSET);
            let fwd_word = load_word(fb, blv, BLV_FWD_OFFSET);
            let fwd_same = eq_imm(fb, fwd_word, fwd as i64);
            let (rule_ok, ruled) = blv_rule(fb, rule, old);
            let default_store =
                (!generational).then(|| cons_store_ok(fb, window, defcell, remembered_defcell));
            let mut default_conds: SmallVec<[ClifValue; 4]> =
                smallvec::smallvec![is_default, fwd_same];
            default_conds.extend(default_store);
            default_conds.extend(rule_ok);
            let default_ok = all(fb, &default_conds);
            conds.push(fb.ins().bor(local_ok, default_ok));
            let cons = fb.ins().select(is_local, valcell, defcell);
            let value = fb.ins().select(is_local, old, ruled);
            Restore::Cons { cons, value }
        }
    }
}

/// `unbind N` inline -- GNU `Bunbind`'s `unbind_to`, the
/// `neovm_jit_unbind` shim's pop of `pop_simple_specpdl_suffix` arms: when
/// the bind stack's top N depths are consecutive and exactly N entries sit
/// above them, and each entry passes [`unbind_entry`], restore them top
/// down, then drop the entries and the depths; otherwise branch to SLOW
/// having changed nothing. No arm runs Lisp, so GNU's quit bracket around
/// `unbind_to` is unobservable.
fn emit_unbind_fast(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    plan: &UnbindPlan,
    slow: Block,
    cont: Block,
) {
    let UnbindPlan { sites, layout } = plan;
    let n = sites.len();
    let vmctx = load_vmctx(fb, rt);
    let jbs_len = load_word(fb, vmctx, CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.len);
    let enough = icmp_imm_p(fb, IntCC::UnsignedGreaterThanOrEqual, jbs_len, n as i64);
    guard(fb, enough, slow);
    let jbs = load_word(fb, vmctx, CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.ptr);
    let top = ishl_imm_p(fb, jbs_len, 3);
    let top = fb.ins().iadd(jbs, top);
    // depths[k] = the depth the k-th binding from the top was made at.
    let depths: SmallVec<[ClifValue; MAX_UNBIND]> = (0..n)
        .map(|k| {
            fb.ins()
                .load(types::I64, trusted(), top, -8 * (k as i32 + 1))
        })
        .collect();
    let spdl_len = load_word(fb, vmctx, CONTEXT_SPECPDL_OFFSET + layout.spdl.len);
    let above = iadd_imm_p(fb, depths[0], 1);
    let mut conds: SmallVec<[ClifValue; MAX_UNBIND]> = smallvec::smallvec![eq(fb, spdl_len, above)];
    for (k, &depth) in depths.iter().enumerate().skip(1) {
        let want = iadd_imm_p(fb, depths[0], -(k as i64));
        conds.push(eq(fb, depth, want));
    }
    let consecutive = all(fb, &conds);
    guard(fb, consecutive, slow);
    let window = barrier_window(fb, rt);
    let cur = sites
        .iter()
        .any(|site| matches!(site.shape, VarShape::Localized { .. }))
        .then(|| current_buffer(fb, rt));
    // Only localized restores write heap conses. Plain/forwarded restores
    // must not consult a possibly inactive allocation view during lowering.
    let generational = cur.is_some() && rt.generational_enabled();
    let mut conds: SmallVec<[ClifValue; 16]> = SmallVec::new();
    if sites
        .iter()
        .any(|site| !matches!(site.shape, VarShape::Localized { .. }))
    {
        conds.push(not_marking(fb, window));
    }
    let mut restores: SmallVec<[Restore; MAX_UNBIND]> = SmallVec::new();
    for (site, &depth) in sites.iter().zip(&depths) {
        let entry = entry_at(fb, rt, layout, depth);
        restores.push(unbind_entry(
            fb,
            &layout.lets,
            site,
            entry,
            cur,
            window,
            &mut conds,
            generational,
        ));
    }
    let ok = all(fb, &conds);
    guard(fb, ok, slow);
    if generational {
        // All shapes are valid before touching a trailer, and every selected
        // restore is checked before any write. A later refusal cannot leave
        // partial unbinding effects for the unchanged fallback shim.
        for restore in &restores {
            if let Restore::Cons { cons, value } = restore {
                let owner = iadd_imm_p(fb, *cons, -(TAG_CONS as i64));
                super::heap_inline::emit_cons_store_barrier(fb, rt, owner, *value, slow);
            }
        }
    }
    for restore in restores {
        match restore {
            Restore::Cell { cell, value } => store_word(fb, value, cell, LISP_SYMBOL_VAL_OFFSET),
            Restore::Fwd { desc, kind, value } => fwd_store(fb, desc, kind, value),
            Restore::Cons { cons, value } => store_word(fb, value, cons, TAGGED_CONS_CDR),
        }
    }
    let vmctx = load_vmctx(fb, rt);
    store_word(
        fb,
        depths[n - 1],
        vmctx,
        CONTEXT_SPECPDL_OFFSET + layout.spdl.len,
    );
    let jbs_len = iadd_imm_p(fb, jbs_len, -(n as i64));
    store_word(
        fb,
        jbs_len,
        vmctx,
        CONTEXT_JIT_BIND_STACK_OFFSET + layout.jbs.len,
    );
    fb.ins().jump(cont, &[]);
    note_site(InlineVarOp::Unbind);
}

/// `Op::Unbind(N)` with PLAN's fast path, the unchanged `neovm_jit_unbind`
/// call as its slow path. STACK is the live operand stack.
#[allow(clippy::too_many_arguments)]
pub(crate) fn lower_unbind(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    plan: &UnbindPlan,
    n: u16,
    stack: &[ClifValue],
    reps: &[SlotRep],
    signal_exit: &mut Option<Block>,
    handlers: &[HandlerStatic],
    pending: &mut Vec<PendingDispatch>,
) {
    let slow = fb.create_block();
    let cont = fb.create_block();
    fb.set_cold_block(slow);
    emit_unbind_fast(fb, rt, plan, slow, cont);
    fb.switch_to_block(slow);
    fb.seal_block(slow);
    let carry_fast = rootwin_carry_snapshot();
    let vmctx = load_vmctx(fb, rt);
    let n_v = fb.ins().iconst(types::I64, i64::from(n));
    let saved = if stack.is_empty() {
        CondRoots::NONE
    } else {
        emit_model_roots_pre(fb, rt, stack, reps)
    };
    let unbind = rt.refs.get(fb.func, Shim::Unbind);
    let call = fb.ins().call(unbind, &[vmctx, n_v]);
    let status = fb.inst_results(call)[0];
    emit_cond_residual_roots_post(fb, rt, saved);
    rootwin_carry_meet(&carry_fast);
    let signal = signal_target_for_site(fb, signal_exit, handlers, pending, stack, reps);
    let ok = icmp_imm_p(fb, IntCC::Equal, status, STATUS_OK);
    fb.ins().brif(ok, cont, &[], signal, &[]);
    fb.switch_to_block(cont);
    fb.seal_block(cont);
}
