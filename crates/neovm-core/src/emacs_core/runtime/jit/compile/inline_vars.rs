//! Variable ops inline in JIT code (P1.4 Stage B; design
//! `p1-4-inline-binding-blv` §4.3, p1-0-integration S2.3a-f): `varref`,
//! `varset`, `varbind` and `unbind` of a plain variable, of a buffer-local
//! variable whose cache is loaded for the current buffer (GNU
//! `swap_in_symval_forwarding`'s early-out), and of a forwarder that holds its
//! own value, done in place instead of in `neovm_jit_varref`, `_varset`,
//! `_varbind` and `_unbind`. Knob `NEOVM_JIT_INLINE_VARS` (default off).
//! (Only `varref` is inlined yet.)
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
//! A read stores nothing, so it needs no barrier and roots nothing.
//!
//! JIT only: nothing here is emitted for an AOT leaf, and the shim set is
//! unchanged, so the AOT ABI is untouched.

use super::jit_layout::{
    BLV_ALIST_EPOCH_OFFSET, BLV_VALCELL_OFFSET, BLV_WHERE_BUF_ID_OFFSET, CONS_CDR_OFFSET,
    CONTEXT_CURRENT_BUFFER_RAW_OFFSET, LISP_BOOL_FWD_VALUE_OFFSET, LISP_INT_FWD_VALUE_OFFSET,
    LISP_KBOARD_OBJ_FWD_VALUE_OFFSET, LISP_OBJ_FWD_VALUE_OFFSET, LISP_SYMBOL_FLAGS_OFFSET,
    LISP_SYMBOL_VAL_OFFSET, SYMBOL_FLAGS_REDIRECT_MASK, blv_alist_epoch_addr,
};
use super::lowering::{band_imm_p, icmp_imm_p, imm64};
use super::*;
use crate::emacs_core::forward::{LispFwd, LispFwdType};
use crate::emacs_core::symbol::SymbolRedirect;
use std::cell::Cell;

/// A cons's cdr, from the tagged cons word.
const TAGGED_CONS_CDR: usize = CONS_CDR_OFFSET - TAG_CONS;
const _: () = assert!(CONS_CDR_OFFSET >= TAG_CONS);

// ---------------------------------------------------------------------------
// The compile environment
// ---------------------------------------------------------------------------

/// What a compile needs from the running `Context` to classify a variable:
/// its obarray. A raw pointer, valid for the [`CompileEnvScope`] that set it.
#[derive(Clone, Copy)]
struct CompileEnv {
    obarray: *const Obarray,
}

thread_local! {
    static ENV: Cell<Option<CompileEnv>> = const { Cell::new(None) };
}

/// Lends the compiles inside it the running context's obarray (the JIT
/// cache's compile entries hold one while they compile). Without one,
/// nothing is inlined.
#[must_use = "the environment lasts as long as the scope"]
pub(crate) struct CompileEnvScope {
    prev: Option<CompileEnv>,
}

impl CompileEnvScope {
    /// Enter CTX's environment (nothing when CTX is null or the knob is off).
    /// CTX must stay alive and its obarray unmoved while the scope lives: the
    /// dormant seam-provided context of a compile.
    pub(crate) fn enter(ctx: *const Context) -> Self {
        let prev = ENV.with(Cell::get);
        let env = (!ctx.is_null() && jit_inline_vars().any()).then(|| {
            // SAFETY: the caller's contract above.
            let ctx = unsafe { &*ctx };
            CompileEnv {
                obarray: std::ptr::from_ref(&ctx.obarray),
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

fn with_env<R>(f: impl FnOnce(&Obarray) -> Option<R>) -> Option<R> {
    let env = ENV.with(Cell::get)?;
    // SAFETY: `CompileEnvScope::enter`'s contract.
    f(unsafe { &*env.obarray })
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
}

fn classify(sym: u32) -> Option<VarSite> {
    with_env(|obarray| {
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
                        _ => VarShape::Localized {
                            blv: blv_ptr as usize,
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
        Some(VarSite { sym, cell, shape })
    })
}

/// The site a `varref` of SYM gets, if the knob inlines reads.
pub(crate) fn read_site(sym: u32) -> Option<VarSite> {
    if !jit_inline_vars().read {
        return None;
    }
    classify(sym)
}

// ---------------------------------------------------------------------------
// Emission helpers
// ---------------------------------------------------------------------------

/// Which inline op a site is (the test census).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum InlineVarOp {
    Read = 0,
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
