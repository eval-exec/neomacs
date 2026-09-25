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
//!
//! A jump table whose keys are immediates, or cons trees of immediates --
//! what `pcase` makes of symbol, fixnum and backquote patterns -- is also
//! answered INLINE ([`InlineSwitch`]): the keys' programs, merged on their
//! common prefixes, become compares and car/cdr loads, guarded by the
//! table's mutation epoch (`HashTableStorage::switch_epoch`). The shim and
//! its compare chain stay as the slow path: a table changed since the
//! compile, and a miss while `symbols-with-pos-enabled` is on (a
//! positioned symbol stands for its bare symbol, which only the lookup
//! strips). On elb-pcase the shim plus the plan walk behind it were ~180
//! of the ~260 instructions per loop iteration.

use super::lowering::{self, RtCtx};
use super::{
    HandlerStatic, JIT_SWITCH_MISS, JIT_SWITCH_STALE, PendingDispatch, emit_backedge_jump,
};
use crate::emacs_core::bytecode::Op;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;
use crate::emacs_core::value::switch_plan::{InlineKeyNode, SWITCH_EPOCH_OFFSET};
use crate::tagged::header::ConsCell;
use crate::tagged::value::{TAG_CONS, TAG_MASK};
use cranelift_codegen::ir::StackSlot;
use cranelift_codegen::ir::Value as ClifValue;
use cranelift_codegen::ir::condcodes::IntCC;
use cranelift_codegen::ir::{Block, InstBuilder, MemFlagsData, types};
use cranelift_frontend::{FunctionBuilder, Switch, Variable};
use std::collections::HashMap;

/// Program nodes, over all its keys, a jump table may have to be answered
/// inline. Bounds the code an inline dispatch adds (a compare or a load and
/// a branch per node); larger tables keep the shim.
const INLINE_MAX_NODES: usize = 64;

#[cfg(test)]
thread_local! {
    static INLINE_SITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// Test-only: how many switch sites this thread has lowered inline.
#[cfg(test)]
pub(crate) fn inline_switch_sites_for_test() -> usize {
    INLINE_SITES.with(std::cell::Cell::get)
}

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

/// A switch site answered inline: the table's epoch at compile time and
/// each key's program with the index, in the site's `targets`, of the
/// target it lands on.
pub(crate) struct InlineSwitch {
    epoch: u64,
    keys: Vec<(Vec<InlineKeyNode>, usize)>,
}

/// The inline form of the switch at instruction `site` (see
/// [`InlineSwitch`]), or `None` to keep the shim alone. The jump table is
/// the constant `ops[site - 1]` pushes, as `switch_static_targets` required
/// of the site -- unless the switch is itself a jump target (`leaders`,
/// sorted), where another path could bring another table: the inline code
/// reads the table object's epoch with no type check. A
/// `make-closure`-patched slot is per instance, so it is never inlined
/// either. `targets` is the site's static target set: every key must land
/// in it.
pub(crate) fn inline_switch_for_site(
    ops: &[Op],
    constants: &[Value],
    dynamic_prefix: usize,
    leaders: &[usize],
    site: usize,
    targets: &[(i64, usize)],
) -> Option<InlineSwitch> {
    if leaders.binary_search(&site).is_ok() {
        return None;
    }
    let Some(Op::Constant(index)) = site.checked_sub(1).map(|p| &ops[p]) else {
        return None;
    };
    let index = *index as usize;
    if index < dynamic_prefix {
        return None;
    }
    let plan = constants
        .get(index)?
        .as_hash_table()?
        .switch_inline_plan(INLINE_MAX_NODES)?;
    let nodes: usize = plan.keys.iter().map(|key| key.program.len()).sum();
    let keys = plan
        .keys
        .into_iter()
        .map(|key| {
            let k = targets.iter().position(|&(raw, _)| raw == key.target)?;
            Some((key.program, k))
        })
        .collect::<Option<Vec<_>>>()?;
    tracing::debug!(
        target: "neovm::jit::switch",
        site,
        keys = keys.len(),
        nodes,
        epoch = plan.epoch,
        "jump table answered inline"
    );
    Some(InlineSwitch {
        epoch: plan.epoch,
        keys,
    })
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
/// With `inline`, the table is first answered inline (see the module
/// documentation), and all of the above is the slow path.
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
    inline: Option<&InlineSwitch>,
) {
    if let Some(site) = inline {
        let slow = fb.create_block();
        emit_inline_dispatch(fb, rt, dispatch, table, site, miss, slow, landings);
        fb.switch_to_block(slow);
        fb.seal_block(slow);
    }
    // Without an inline dispatch every landing has one predecessor, its
    // compare, and is filled right after it: the lowering before inline
    // dispatch existed, block for block. With one, a landing an inline hit
    // made may have a compare still to come, so they are filled at the end.
    let fill_each = inline.is_none();
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
        if fill_each {
            landings.fill_pending(fb);
        }
        cur = next;
    }
    // Exhausted: a hit whose address is not in the static set.
    fb.switch_to_block(cur);
    fb.seal_block(cur);
    fb.ins().call(rt.refs.switch_stale, &[]);
    fb.ins().jump(stale, &[]);
    if !fill_each {
        landings.fill_pending(fb);
    }
}

/// The inline dispatch: while the table's epoch is the compiled one, walk
/// the dispatch value against the key trie; a hit lands directly, a miss
/// continues at `miss` unless `symbols-with-pos-enabled` is on. Every other
/// path, and a changed table, continues at `slow` (left for the caller to
/// fill). Leaves no block open.
#[allow(clippy::too_many_arguments)] // one switch site's complete description
fn emit_inline_dispatch(
    fb: &mut FunctionBuilder,
    rt: &RtCtx,
    dispatch: ClifValue,
    table: ClifValue,
    site: &InlineSwitch,
    miss: Block,
    slow: Block,
    landings: &mut dyn SwitchLandings,
) {
    #[cfg(test)]
    INLINE_SITES.with(|sites| sites.set(sites.get() + 1));
    // The table is the compile-time constant (the leaf's reloc load of it),
    // so its object is the one the plan was read from: its epoch is the
    // plan's until something mutates it.
    let object = lowering::band_imm_p(fb, table, !(TAG_MASK as i64));
    let epoch = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        object,
        SWITCH_EPOCH_OFFSET as i32,
    );
    let current = lowering::icmp_imm_p(fb, IntCC::Equal, epoch, site.epoch as i64);
    let tree = fb.create_block();
    fb.ins().brif(current, tree, &[], slow, &[]);
    fb.switch_to_block(tree);
    fb.seal_block(tree);

    let mut trie = KeyTrie::default();
    for (program, k) in &site.keys {
        trie.insert(program, *k);
    }
    let no_key = fb.create_block();
    emit_trie(fb, &trie, dispatch, &mut Vec::new(), no_key, landings);

    // No key matched. Exact unless `symbols-with-pos-enabled` is on: then a
    // positioned symbol, anywhere in the value, may stand for a key's bare
    // symbol, and only the lookup strips it.
    fb.switch_to_block(no_key);
    fb.seal_block(no_key);
    let vmctx = fb.use_var(rt.vmctx_var);
    let swp = fb.ins().load(
        types::I8,
        MemFlagsData::trusted(),
        vmctx,
        core::mem::offset_of!(Context, symbols_with_pos_enabled) as i32,
    );
    fb.ins().brif(swp, slow, &[], miss, &[]);
}

/// The keys' programs merged on their common prefixes. Programs are
/// pre-order walks, so keys that agree up to a node visit the same position
/// of the value there, in the same order: each position is loaded and
/// tested once. The alternatives at a node exclude each other (a value is
/// one immediate, or a cons), and a program ends only where no cdr is
/// pending, so no program is a prefix of another.
#[derive(Default)]
struct KeyTrie {
    /// The key (its index in the site's targets) whose program ends here.
    hit: Option<usize>,
    /// Immediate alternatives for the next node, by bits.
    imm: Vec<(usize, KeyTrie)>,
    /// The cons alternative for the next node.
    cons: Option<Box<KeyTrie>>,
}

impl KeyTrie {
    fn insert(&mut self, program: &[InlineKeyNode], k: usize) {
        let mut node = self;
        for &step in program {
            node = match step {
                InlineKeyNode::Imm(bits) => {
                    let at = match node.imm.iter().position(|&(b, _)| b == bits) {
                        Some(at) => at,
                        None => {
                            node.imm.push((bits, KeyTrie::default()));
                            node.imm.len() - 1
                        }
                    };
                    &mut node.imm[at].1
                }
                InlineKeyNode::Cons => node.cons.get_or_insert_with(Default::default),
            };
        }
        debug_assert!(
            node.hit.is_none() && node.imm.is_empty() && node.cons.is_none(),
            "key programs are unique and prefix-free"
        );
        node.hit = Some(k);
    }
}

/// Test `v` against the alternatives at `node`. `pending` holds the
/// untagged cons cells whose cdr the walk visits after the current car
/// subtree, innermost last. A matched key lands on its target; every
/// failed test continues at `no_key`. Leaves no block open.
fn emit_trie(
    fb: &mut FunctionBuilder,
    node: &KeyTrie,
    v: ClifValue,
    pending: &mut Vec<ClifValue>,
    no_key: Block,
    landings: &mut dyn SwitchLandings,
) {
    debug_assert!(node.hit.is_none(), "a key's end is branched to, not walked");
    // Where each immediate continues: its key's landing when the program
    // ends with it (no cdr pending), else the test of the next position.
    let arms: Vec<(usize, Block, &KeyTrie)> = node
        .imm
        .iter()
        .map(|(bits, child)| {
            debug_assert_eq!(child.hit.is_some(), pending.is_empty());
            let block = match child.hit {
                Some(k) => landings.landing(fb, k),
                None => fb.create_block(),
            };
            (*bits, block, child)
        })
        .collect();
    if arms.is_empty() {
        // Only a cons can match here: test it in the current block.
        match &node.cons {
            Some(child) => emit_cons_arm(fb, child, v, pending, no_key, landings),
            None => {
                fb.ins().jump(no_key, &[]);
            }
        }
        return;
    }
    let not_imm = if node.cons.is_some() {
        fb.create_block()
    } else {
        no_key
    };
    let mut switch = Switch::new();
    for &(bits, block, _) in &arms {
        switch.set_entry(bits as u64 as u128, block);
    }
    switch.emit(fb, v, not_imm);

    let cdr_offset = core::mem::offset_of!(ConsCell, cdr_or_next) as i32;
    for &(_, block, child) in &arms {
        if child.hit.is_some() {
            continue;
        }
        fb.switch_to_block(block);
        fb.seal_block(block);
        // The leaf closed a car subtree: the walk goes on at the cdr of the
        // innermost cons whose car it was.
        let cell = pending
            .pop()
            .expect("a program continuing after a leaf has a cdr pending");
        let cdr = fb
            .ins()
            .load(types::I64, MemFlagsData::trusted(), cell, cdr_offset);
        emit_trie(fb, child, cdr, pending, no_key, landings);
        pending.push(cell);
    }
    if let Some(child) = &node.cons {
        fb.switch_to_block(not_imm);
        fb.seal_block(not_imm);
        emit_cons_arm(fb, child, v, pending, no_key, landings);
    }
}

/// The cons alternative, in the current block: `v` must be a cons; its car
/// is walked against `child`, its cdr later.
fn emit_cons_arm(
    fb: &mut FunctionBuilder,
    child: &KeyTrie,
    v: ClifValue,
    pending: &mut Vec<ClifValue>,
    no_key: Block,
    landings: &mut dyn SwitchLandings,
) {
    let tag = lowering::band_imm_p(fb, v, TAG_MASK as i64);
    let is_cons = lowering::icmp_imm_p(fb, IntCC::Equal, tag, TAG_CONS as i64);
    let cons = fb.create_block();
    fb.ins().brif(is_cons, cons, &[], no_key, &[]);
    fb.switch_to_block(cons);
    fb.seal_block(cons);
    let cell = lowering::band_imm_p(fb, v, !(TAG_MASK as i64));
    let car = fb.ins().load(
        types::I64,
        MemFlagsData::trusted(),
        cell,
        core::mem::offset_of!(ConsCell, car) as i32,
    );
    pending.push(cell);
    emit_trie(fb, child, car, pending, no_key, landings);
    pending.pop();
}

/// The baseline's landings for [`switch_dispatch::emit_switch_dispatch`]: a
/// forward target is its leader block; a backward one is a trampoline that
/// polls through [`emit_backedge_jump`], exactly like a `Goto` back-edge, and
/// is created once per target however many hits branch to it.
pub(super) struct BaselineSwitchLandings<'a> {
    /// The switch's instruction index: a target at or before it is backward.
    pub(super) site: usize,
    pub(super) targets: &'a [(i64, usize)],
    pub(super) block_for: &'a HashMap<usize, Block>,
    pub(super) entry_depth: &'a HashMap<usize, usize>,
    pub(super) rt: &'a RtCtx,
    pub(super) backedge_counter: Option<StackSlot>,
    pub(super) signal_exit: &'a mut Option<Block>,
    pub(super) vars: &'a [Variable],
    pub(super) variable_raw: &'a [bool],
    pub(super) handlers: &'a [HandlerStatic],
    pub(super) pending: &'a mut Vec<PendingDispatch>,
    /// The trampoline made for each backward target, by target index.
    pub(super) trampolines: Vec<(usize, Block)>,
    /// Trampolines made but not yet filled, in the order they were made.
    pub(super) unfilled: Vec<(usize, Block)>,
}

impl SwitchLandings for BaselineSwitchLandings<'_> {
    fn landing(&mut self, fb: &mut FunctionBuilder, k: usize) -> Block {
        let target = self.targets[k].1;
        if target > self.site {
            return self.block_for[&target];
        }
        if let Some(&(_, tramp)) = self.trampolines.iter().find(|&&(made, _)| made == k) {
            return tramp;
        }
        let tramp = fb.create_block();
        self.trampolines.push((k, tramp));
        self.unfilled.push((k, tramp));
        tramp
    }

    fn fill_pending(&mut self, fb: &mut FunctionBuilder) {
        for (k, tramp) in std::mem::take(&mut self.unfilled) {
            let target = self.targets[k].1;
            fb.switch_to_block(tramp);
            fb.seal_block(tramp);
            emit_backedge_jump(
                fb,
                self.rt,
                self.backedge_counter.expect("backedge implies counter"),
                self.signal_exit,
                self.vars,
                self.variable_raw,
                self.entry_depth[&target],
                self.block_for[&target],
                self.handlers,
                self.pending,
            );
        }
    }
}
