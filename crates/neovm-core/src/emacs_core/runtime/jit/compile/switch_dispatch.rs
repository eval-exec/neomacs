//! `Op::Switch` -- GNU `Bswitch` (src/bytecode.c) -- lowered by one emitter
//! for every tier that models the op.
//!
//! A tier supplies only its landings: how a hit on one of the switch's
//! statically resolved targets reaches that target's code
//! ([`SwitchLandings`]). The baseline lands a forward target on its leader
//! block and a backward one on a poll trampoline; the MIR tier, once it
//! models `Switch`, lands with its own edge arguments. Everything else --
//! the lookup shim, the miss and stale-table exits and the compare chain
//! over the raw addresses the shim answers -- is emitted here, once.

use super::lowering::{self, RtCtx};
use super::{JIT_SWITCH_MISS, JIT_SWITCH_STALE};
use cranelift_codegen::ir::Value as ClifValue;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder};
use cranelift_frontend::FunctionBuilder;

/// How a hit on one of a switch's static targets reaches that target's
/// code. Target `k` is `targets[k]` of [`emit_switch_dispatch`].
pub(crate) trait SwitchLandings {
    /// The block a hit on target `k` branches to (with no arguments). A
    /// landing that needs a body of its own, such as a back-edge trampoline,
    /// is created on the first call for `k` and filled by
    /// [`Self::fill_pending`]; later calls answer the same block.
    fn landing(&mut self, fb: &mut FunctionBuilder, k: usize) -> Block;

    /// Emit the bodies of the landings created since the last call. The
    /// emitter calls this only when no block is open and every branch into
    /// those landings has been emitted, so a landing may be sealed here.
    fn fill_pending(&mut self, fb: &mut FunctionBuilder);
}

/// Lower `switch` on the tagged `dispatch` value through the jump table
/// `table` (the compile-time constant `targets` was resolved from, as the
/// tier loads it).
///
/// `neovm_jit_switch` answers with the interpreter's exact key semantics
/// (the table's switch plan): the raw fixnum address of a hit,
/// [`JIT_SWITCH_MISS`] or [`JIT_SWITCH_STALE`]. A miss continues at `miss`;
/// a stale table -- the shim has stashed the signal -- at `stale`, the
/// site's signal target. A hit is mapped onto `targets` (deduplicated `(raw
/// address, instruction index)` pairs) with a compare chain; an address
/// outside that set means the table was mutated after compilation, so the
/// stale-table signal is stashed and control goes to `stale` too.
///
/// The caller has written the edge state its landings read (the baseline:
/// the operand stack to its variables) and holds the current block open.
#[allow(clippy::too_many_arguments)] // one switch site's complete description
pub(crate) fn emit_switch_dispatch(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    dispatch: ClifValue,
    table: ClifValue,
    targets: &[(i64, usize)],
    miss: Block,
    stale: Block,
    landings: &mut dyn SwitchLandings,
) {
    let vmctx = fb.use_var(rt.vmctx_var);
    let call = fb
        .ins()
        .call(rt.refs.switch_lookup, &[vmctx, dispatch, table]);
    let addr = fb.inst_results(call)[0];
    let is_miss = lowering::icmp_imm_p(fb, IntCC::Equal, addr, JIT_SWITCH_MISS);
    let chain = fb.create_block();
    fb.ins().brif(is_miss, miss, &[], chain, &[]);
    fb.switch_to_block(chain);
    fb.seal_block(chain);
    // Stale (-2): the shim stashed the signal already.
    let is_stale = lowering::icmp_imm_p(fb, IntCC::Equal, addr, JIT_SWITCH_STALE);
    let mut cur = fb.create_block();
    fb.ins().brif(is_stale, stale, &[], cur, &[]);
    for (k, &(raw, _)) in targets.iter().enumerate() {
        fb.switch_to_block(cur);
        fb.seal_block(cur);
        let next = fb.create_block();
        let hit = lowering::icmp_imm_p(fb, IntCC::Equal, addr, raw);
        let landing = landings.landing(fb, k);
        fb.ins().brif(hit, landing, &[], next, &[]);
        landings.fill_pending(fb);
        cur = next;
    }
    // Exhausted: a hit whose address is not in the static set.
    fb.switch_to_block(cur);
    fb.seal_block(cur);
    fb.ins().call(rt.refs.switch_stale, &[]);
    fb.ins().jump(stale, &[]);
}
