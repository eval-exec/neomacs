//! Bytecode-level inlining: splice a called function's ops into its caller's,
//! upstream of every tier.
//!
//! A compiled-to-compiled call costs 150-250 instructions — the shim frame,
//! the leaf resolution, the arity check, the backtrace push and pop, the
//! argument marshal — against a callee body that is often a dozen. The JIT
//! already has one inliner, in the MIR tier, but that tier compiles about a
//! tenth of real bodies (`gate:generic-call` alone rejects 114 of the bodies
//! a compile of elb-smie.el produces).
//!
//! So this inlines BEFORE either tier, as a transform on `&[Op]`. The fused
//! op vector goes to the same `lower_leaf_full_osr` the caller would have
//! taken, and every analysis it drives — the CFG, the known-fixnum fixpoint,
//! call speculation, the reloc vector, the per-slot variables — then runs
//! across the former call boundary with no second implementation.
//!
//! # The stack-layout identity
//!
//! At a call the caller's operand stack is `[...residual, F, a1..aN]` and the
//! callee's frame is `[a1..aN, ...locals]`: contiguous. Leaving the callee's
//! `Op::Constant(F)` push in place and replacing only the `Op::Call(N)` with
//! the callee's ops therefore keeps every stack index correct by
//! construction, because bytecode stack operands are top-relative. The one
//! number that splits a fused frame back into two interpreter frames is
//! `frame_base = depth_at_call - N`, which a deopt uses to rebuild the
//! caller's pre-call stack.
//!
//! # What is admitted here
//!
//! Only a callee that is a CONSTANT bytecode object — the shape the compiler
//! emits for a `cl-flet` local, a `lambda` literal or a closure in a variable
//! — so there is no redefinition surface at all: the object is the constant,
//! and `fset` cannot reach it. Only required arguments, and only a body of
//! ops that cannot call, bind, signal through a native edge, or dispatch
//! through a jump table (see [`op_is_inlinable`]). Arithmetic is admitted
//! only where the CALLEE's own feedback says the site is fixnum-only: the
//! caller's feedback vector says nothing about a spliced op, and reading the
//! wrong one is the mistake that made `(1+ 3.0)` return a shifted pointer.

use super::compile::analyze_cfg;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::value::Value;

/// Ops of a callee body, at most, for one splice.
pub(crate) const MAX_INLINE_BODY: usize = 40;

/// One spliced call: where the callee's ops live in the fused body, and what
/// a deopt inside them must rebuild.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct InlineRegion {
    /// Fused pc range of the spliced ops, `[start, end)`.
    pub(crate) start: usize,
    pub(crate) end: usize,
    /// The caller pc of the `Op::Call` this replaced — where a deopt resumes.
    pub(crate) call_site_pc: usize,
    /// Fused slot of the callee's first argument: the caller's pre-call stack
    /// is everything below it, plus the callee object one slot lower.
    pub(crate) frame_base: usize,
    /// The call's argument count.
    pub(crate) nargs: usize,
    /// The `Value` bits of the callee whose body was spliced here. The
    /// constant-propagation that picked this site is a SELECTION heuristic —
    /// it does not model jump-table edges, and a caller can always store a
    /// different function into the slot — so the region's first act is to
    /// check the live callee slot against these bits and deopt to the call
    /// when they differ. See `emit_region_entry_guard`.
    pub(crate) callee_bits: u64,
}

/// A caller's ops with one or more callee bodies spliced in.
#[derive(Clone, Debug)]
pub(crate) struct FusedBody {
    pub(crate) ops: Vec<Op>,
    pub(crate) constants: Vec<Value>,
    /// Per-site numeric feedback for the fused ops: the caller's own for its
    /// ops, the CALLEE's for spliced ones.
    pub(crate) feedback: Vec<NumericFeedback>,
    /// The caller's offset map with every instruction index rewritten to its
    /// fused pc (a jump table resolves through it).
    pub(crate) offset_map: Option<Vec<GnuByteOffsetMapEntry>>,
    pub(crate) regions: Vec<InlineRegion>,
    /// Fused pc -> the region it belongs to, or `None` for caller code.
    pub(crate) region_of: Vec<Option<usize>>,
    /// Fused pc -> the ORIGINAL caller pc it came from (a region's ops all map
    /// to the call they replaced). A deopt resumes the interpreter in the
    /// UNFUSED body, so every pc that escapes into deopt metadata has to come
    /// back through this map.
    pub(crate) caller_of_fused: Vec<usize>,
}

impl FusedBody {
    /// The region a fused pc belongs to.
    pub(crate) fn region_at(&self, pc: usize) -> Option<&InlineRegion> {
        self.region_of
            .get(pc)
            .copied()
            .flatten()
            .map(|r| &self.regions[r])
    }

    /// The original-body pc a fused pc stands for — what the interpreter has
    /// to resume at. Out-of-range is not expected, but answering with the
    /// input would hand the interpreter a fused index, so refuse instead.
    pub(crate) fn caller_pc(&self, fused_pc: usize) -> Option<usize> {
        self.caller_of_fused.get(fused_pc).copied()
    }
}

/// Whether one callee op may be spliced. An allowlist, never a denylist: an
/// op that reaches a runtime shim with a native signal edge would raise its
/// error with the caller's frame and no `funcall` frame of its own, which is
/// observable in a backtrace; an op that calls, binds, or dispatches through
/// a jump table breaks a structural assumption of the splice.
///
/// `feedback` is the CALLEE's for this op's pc.
fn op_is_inlinable(op: &Op, feedback: NumericFeedback) -> bool {
    match op {
        // Stack shuffles, constants, control flow inside the body.
        Op::Constant(_)
        | Op::Nil
        | Op::True
        | Op::Pop
        | Op::Dup
        | Op::StackRef(_)
        | Op::StackSet(_)
        | Op::DiscardN(_)
        | Op::Goto(_)
        | Op::GotoIfNil(_)
        | Op::GotoIfNotNil(_)
        | Op::GotoIfNilElsePop(_)
        | Op::GotoIfNotNilElsePop(_)
        | Op::Return => true,
        // Type tests and identity: no shim, no signal.
        Op::Null
        | Op::Not
        | Op::Consp
        | Op::Listp
        | Op::Symbolp
        | Op::Stringp
        | Op::Integerp
        | Op::Numberp
        | Op::Eq => true,
        // `car`/`cdr` of a non-cons DEOPTS (a precise exit at the call site),
        // it does not signal natively.
        //
        // `cons` is NOT admissible, and the reason is the region framestate:
        // a region's deopt replays the CALL, so it spills the operand stack as
        // it stood at the region's entry — including argument slots the callee
        // may since have overwritten (`Op::StackSet` on a parameter is what a
        // `setq` of an argument compiles to). Those superseded values are in no
        // root set any more, so a collection inside the region would leave the
        // framestate holding freed pointers. Keeping the region allocation-free
        // (and back-edge-free, see `inlinable_verdict`) means no collection can
        // happen between the snapshot and its use.
        Op::Car | Op::Cdr | Op::CarSafe | Op::CdrSafe => true,
        // Arithmetic and comparison: only where the callee's own feedback
        // says every operand was a fixnum, so the site neither takes the
        // float lowering nor the generic fallback (both of which change what
        // the result's type analysis may assume).
        Op::Add
        | Op::Sub
        | Op::Mul
        | Op::Add1
        | Op::Sub1
        | Op::Negate
        | Op::Eqlsign
        | Op::Lss
        | Op::Gtr
        | Op::Leq
        | Op::Geq
        | Op::Max
        | Op::Min => feedback == NumericFeedback::FixnumOnly,
        _ => false,
    }
}

/// The callee's per-op numeric feedback, read without consuming it (the
/// callee keeps recording for its own compiles).
fn callee_feedback(bc: &ByteCodeFunction) -> Vec<NumericFeedback> {
    let rt = bc.jit_runtime();
    (0..bc.executable_ops().len())
        .map(|pc| rt.numeric_feedback(pc))
        .collect()
}

/// Whether `callee` may be spliced at all: a lexical bytecode function of
/// required arguments only, small enough, with no per-instance patched
/// prefix, whose every op is inlinable.
/// Whether one call site's callee may be spliced, and if so its per-pc operand
/// depths. `pool_len` is the fused constant pool before this callee's is added.
fn site_verdict(
    callee: &ByteCodeFunction,
    nargs: usize,
    pool_len: usize,
) -> Result<Vec<usize>, String> {
    inlinable_verdict(callee, nargs)?;
    if pool_len + callee.constants.len() > u16::MAX as usize + 1 {
        return Err("constant-pool".into());
    }
    // Unreachable ops (dead code after a `Return`) have no depth; skip the one
    // site rather than abandon every other splice in the caller.
    callee_depths(callee, nargs).ok_or_else(|| "depths".into())
}

/// Why a callee was (not) admitted. The `Err` strings are the census keys read
/// back by `NEOVM_JIT_COMPILE_STATS=1`, so each names ONE rule: whichever
/// dominates a real workload is the rule worth relaxing next.
fn inlinable_verdict(callee: &ByteCodeFunction, nargs: usize) -> Result<(), String> {
    if !callee.lexical {
        return Err("dynamic".into());
    }
    if callee.env.is_some() {
        return Err("env".into());
    }
    if !callee.params.optional.is_empty() || callee.params.rest.is_some() {
        return Err("arglist".into());
    }
    if callee.params.required.len() != nargs {
        return Err("arity".into());
    }
    if callee.jit_runtime().patched_prefix() > 0 {
        return Err("patched-prefix".into());
    }
    let ops = callee.executable_ops();
    if ops.is_empty() {
        return Err("empty".into());
    }
    if ops.len() > MAX_INLINE_BODY {
        return Err("size".into());
    }
    // A body that has never run has no feedback to trust.
    if callee.jit_runtime().heat() == 0 {
        return Err("cold".into());
    }
    let feedback = callee_feedback(callee);
    for (pc, op) in ops.iter().enumerate() {
        let fb = feedback
            .get(pc)
            .copied()
            .unwrap_or(NumericFeedback::FixnumOnly);
        if !op_is_inlinable(op, fb) {
            return Err(format!("op:{}", op_census_name(op)));
        }
        // A back edge inside the region would take a poll — a GC safe point —
        // between the region's framestate snapshot and the deopt that spills
        // it. See the `Op::Cons` note in `op_is_inlinable`.
        if jump_target(op).is_some_and(|t| t as usize <= pc) {
            return Err("back-edge".into());
        }
        if jump_target(op).is_some_and(|t| t as usize >= ops.len()) {
            return Err("jump-out".into());
        }
    }
    // The `Return` expansion is the splice's only stack-rebalancing act. With
    // every jump forward and in range, a body whose last op is a `Return`
    // leaves through one on every path; one that falls off its end would leave
    // the callee's frame on the caller's stack.
    if !matches!(ops.last(), Some(Op::Return)) {
        return Err("no-return".into());
    }
    Ok(())
}

/// The branch target of a jump op, if it is one.
fn jump_target(op: &Op) -> Option<u32> {
    match op {
        Op::Goto(t)
        | Op::GotoIfNil(t)
        | Op::GotoIfNotNil(t)
        | Op::GotoIfNilElsePop(t)
        | Op::GotoIfNotNilElsePop(t) => Some(*t),
        _ => None,
    }
}

/// The variant name of an op, without its operands — the census wants one
/// bucket per opcode, not one per operand value.
fn op_census_name(op: &Op) -> String {
    let rendered = format!("{op:?}");
    match rendered.find(['(', ' ', '{']) {
        Some(i) => rendered[..i].to_string(),
        None => rendered,
    }
}

/// Rewrite one spliced op's constant-pool operand into the fused body's
/// numbering (the callee's constants are appended after the caller's). Jump
/// operands are rebased separately, through the callee's own pc map.
fn rebase_constants(op: &Op, const_base: usize) -> Op {
    match op {
        Op::Constant(i) => Op::Constant((*i as usize + const_base) as u16),
        other => other.clone(),
    }
}

/// The callee-local operand-stack depth at every pc, from the callee's own
/// CFG: what a `Return` there has to discard.
fn callee_depths(callee: &ByteCodeFunction, nargs: usize) -> Option<Vec<usize>> {
    let ops = callee.executable_ops();
    let cfg = analyze_cfg(
        ops,
        &callee.constants,
        callee.executable_gnu_byte_offset_map(),
        nargs,
    )
    .ok()?;
    let mut depth = vec![usize::MAX; ops.len()];
    for (&leader, &d) in &cfg.entry_depth {
        if leader >= ops.len() {
            continue;
        }
        let mut cur = d;
        let end = cfg
            .leaders
            .iter()
            .copied()
            .find(|&l| l > leader)
            .unwrap_or(ops.len());
        for (i, op) in ops[leader..end].iter().enumerate() {
            depth[leader + i] = cur;
            // A terminator ends the block, and nothing after it in this block
            // needs a depth.
            let Ok((needs, delta)) = super::compile::simple_effect(op) else {
                break;
            };
            if cur < needs {
                return None;
            }
            cur = (cur as i64 + delta) as usize;
        }
    }
    depth.iter().all(|&d| d != usize::MAX).then_some(depth)
}

/// Splice every admissible constant-bytecode call in `ops`. `None` when there
/// is nothing to inline (the caller then compiles unchanged).
pub(crate) fn fuse_calls(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    caller_feedback: &[NumericFeedback],
) -> Option<FusedBody> {
    // A jump table resolves through the GNU byte-offset map, which this pass
    // rewrites. Without one, the table's fixnums ARE instruction indices, held
    // inside a constant hash table nothing here touches — and every splice
    // shifts the caller's tail, so an arm would dispatch into the wrong op.
    if offset_map.is_none() && ops.iter().any(|op| matches!(op, Op::Switch)) {
        super::stats::record_inline("reject:caller-index-switch");
        return None;
    }
    let cfg = analyze_cfg(ops, constants, offset_map, arity).ok()?;
    let caller_depth = op_depths(ops, &cfg)?;
    // The callee slot of an `Op::Call`, when it provably holds a constant.
    let mut tags: Vec<Option<u16>> = Vec::new();
    let entry = super::compile::spec_tag_entry_states(ops, constants, &cfg.leaders);
    // (pc, const idx, nargs, the callee's per-pc depths)
    let mut sites: Vec<(usize, u16, usize, Vec<usize>)> = Vec::new();
    // Every splice appends the callee's whole constant pool, and a spliced
    // `Op::Constant` names its slot in a u16: past that the index would wrap
    // onto an unrelated constant.
    let mut pool_len = constants.len();
    for (i, op) in ops.iter().enumerate() {
        if cfg.leaders.binary_search(&i).is_ok() {
            tags.clear();
            if let Some(agreed) = entry.get(&i) {
                tags.extend_from_slice(agreed);
            }
        }
        if let Op::Call(n) = op {
            let nargs = *n as usize;
            if tags.len() > nargs
                && let Some(cidx) = tags[tags.len() - 1 - nargs]
                && let Some(callee) = constants.get(cidx as usize)
                && let Some(bc) = callee.get_bytecode_data()
                && caller_depth[i] > nargs
            {
                // A deopt showed this site must stay a call (`jit::reopt`).
                // `i` is an original pc: no fused scope is active yet.
                if !super::compile::call_site_inlinable_at(i) {
                    super::stats::record_inline("reject:reopt");
                    super::compile::spec_tag_transfer(op, constants, &mut tags);
                    continue;
                }
                match site_verdict(bc, nargs, pool_len) {
                    Ok(depths) => {
                        pool_len += bc.constants.len();
                        sites.push((i, cidx, nargs, depths));
                    }
                    Err(why) => super::stats::record_inline(format!("reject:{why}")),
                }
            }
        }
        super::compile::spec_tag_transfer(op, constants, &mut tags);
    }
    if sites.is_empty() {
        return None;
    }
    let fused = splice_sites(
        ops,
        constants,
        offset_map,
        caller_feedback,
        &caller_depth,
        &sites,
    );
    match &fused {
        Some(body) => {
            for _ in &body.regions {
                super::stats::record_inline("fused");
            }
        }
        None => super::stats::record_inline("reject:splice"),
    }
    fused
}

/// Build the fused body for the selected `sites`. `None` abandons the whole
/// caller, which then compiles unfused.
fn splice_sites(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    caller_feedback: &[NumericFeedback],
    caller_depth: &[usize],
    sites: &[(usize, u16, usize, Vec<usize>)],
) -> Option<FusedBody> {
    let mut out: Vec<Op> = Vec::with_capacity(ops.len() + MAX_INLINE_BODY * sites.len());
    let mut out_feedback: Vec<NumericFeedback> = Vec::with_capacity(out.capacity());
    let mut fused_constants = constants.to_vec();
    let mut regions: Vec<InlineRegion> = Vec::new();
    let mut region_of: Vec<Option<usize>> = Vec::new();
    let mut caller_of_fused: Vec<usize> = Vec::new();
    let mut fused_of_caller = vec![usize::MAX; ops.len()];
    let mut site_iter = sites.iter().peekable();
    for (i, op) in ops.iter().enumerate() {
        fused_of_caller[i] = out.len();
        let splice = site_iter.peek().is_some_and(|(pc, _, _, _)| *pc == i);
        if !splice {
            out.push(op.clone());
            out_feedback.push(
                caller_feedback
                    .get(i)
                    .copied()
                    .unwrap_or(NumericFeedback::FixnumOnly),
            );
            region_of.push(None);
            caller_of_fused.push(i);
            continue;
        }
        let (_, cidx, nargs, depths) = site_iter.next().expect("peeked");
        let (cidx, nargs) = (*cidx, *nargs);
        let callee = fused_constants[cidx as usize];
        let bc = callee.get_bytecode_data().expect("checked");
        let cops = bc.executable_ops();
        let feedback = callee_feedback(bc);
        let const_base = fused_constants.len();
        let region_start = out.len();
        let region_id = regions.len();
        let mut return_gotos: Vec<usize> = Vec::new();
        // A `Return` expands into a discard and a jump, so a callee pc is not
        // its fused pc plus a constant: every internal jump rebases through
        // this map.
        let mut fused_of_callee = vec![usize::MAX; cops.len()];
        let mut callee_jumps: Vec<usize> = Vec::new();
        for (pc, cop) in cops.iter().enumerate() {
            fused_of_callee[pc] = out.len();
            if let Op::Return = cop {
                // The fused stack here is `[...residual, F, <d callee slots
                // ending in the result>]`; the caller expects
                // `[...residual, result]`, so discard `d` slots BELOW the top
                // (the preserve bit keeps it). `d` counts the callee frame
                // plus the callee object itself.
                let d = depths[pc];
                if d == 0 {
                    return None;
                }
                let mut left = d; // callee slots below the result, plus F
                while left > 0 {
                    let chunk = left.min(0x7F);
                    out.push(Op::DiscardN(0x80 | chunk as u8));
                    out_feedback.push(NumericFeedback::FixnumOnly);
                    region_of.push(Some(region_id));
                    caller_of_fused.push(i);
                    left -= chunk;
                }
                return_gotos.push(out.len());
                out.push(Op::Goto(0)); // patched to the region end below
                out_feedback.push(NumericFeedback::FixnumOnly);
                region_of.push(Some(region_id));
                caller_of_fused.push(i);
                continue;
            }
            if matches!(
                cop,
                Op::Goto(_)
                    | Op::GotoIfNil(_)
                    | Op::GotoIfNotNil(_)
                    | Op::GotoIfNilElsePop(_)
                    | Op::GotoIfNotNilElsePop(_)
            ) {
                callee_jumps.push(out.len());
            }
            out.push(rebase_constants(cop, const_base));
            out_feedback.push(
                feedback
                    .get(pc)
                    .copied()
                    .unwrap_or(NumericFeedback::FixnumOnly),
            );
            region_of.push(Some(region_id));
            caller_of_fused.push(i);
        }
        fused_constants.extend_from_slice(&bc.constants);
        let region_end = out.len();
        for g in return_gotos {
            out[g] = Op::Goto(region_end as u32);
        }
        for j in callee_jumps {
            let retarget = |t: &u32| match fused_of_callee.get(*t as usize) {
                Some(&fused) if fused != usize::MAX => Some(fused as u32),
                _ => None,
            };
            out[j] = match &out[j] {
                Op::Goto(t) => Op::Goto(retarget(t)?),
                Op::GotoIfNil(t) => Op::GotoIfNil(retarget(t)?),
                Op::GotoIfNotNil(t) => Op::GotoIfNotNil(retarget(t)?),
                Op::GotoIfNilElsePop(t) => Op::GotoIfNilElsePop(retarget(t)?),
                Op::GotoIfNotNilElsePop(t) => Op::GotoIfNotNilElsePop(retarget(t)?),
                other => other.clone(),
            };
        }
        regions.push(InlineRegion {
            start: region_start,
            end: region_end,
            call_site_pc: i,
            frame_base: caller_depth[i] - nargs,
            nargs,
            callee_bits: callee.bits() as u64,
        });
    }
    // The caller's own jumps still name caller pcs; a spliced op's jumps are
    // already fused indices.
    for (pc, op) in out.iter_mut().enumerate() {
        if region_of[pc].is_some() {
            continue;
        }
        let map = |t: &u32| fused_of_caller[*t as usize] as u32;
        *op = match &*op {
            Op::Goto(t) => Op::Goto(map(t)),
            Op::GotoIfNil(t) => Op::GotoIfNil(map(t)),
            Op::GotoIfNotNil(t) => Op::GotoIfNotNil(map(t)),
            Op::GotoIfNilElsePop(t) => Op::GotoIfNilElsePop(map(t)),
            Op::GotoIfNotNilElsePop(t) => Op::GotoIfNotNilElsePop(map(t)),
            Op::PushConditionCase(t) => Op::PushConditionCase(map(t)),
            Op::PushConditionCaseRaw(t) => Op::PushConditionCaseRaw(map(t)),
            Op::PushCatch(t) => Op::PushCatch(map(t)),
            other => other.clone(),
        };
    }
    // A jump table resolves a GNU byte offset to an instruction index: every
    // caller index moves.
    let fused_map = offset_map.map(|m| {
        m.iter()
            .map(|e| {
                GnuByteOffsetMapEntry::new(
                    e.byte_offset,
                    fused_of_caller
                        .get(e.instruction_index)
                        .copied()
                        .unwrap_or(e.instruction_index),
                )
            })
            .collect()
    });
    // Every pc the lowering can hand `deopt_site` must map back to the
    // original body; a gap would resume the interpreter at a fused index.
    if caller_of_fused.len() != out.len() || region_of.len() != out.len() {
        return None;
    }
    Some(FusedBody {
        ops: out,
        constants: fused_constants,
        feedback: out_feedback,
        offset_map: fused_map,
        regions,
        region_of,
        caller_of_fused,
    })
}

/// Operand-stack depth before every op, from the body's own CFG.
fn op_depths(ops: &[Op], cfg: &super::compile::Cfg) -> Option<Vec<usize>> {
    let mut depth = vec![usize::MAX; ops.len()];
    for (&leader, &d) in &cfg.entry_depth {
        if leader >= ops.len() {
            continue;
        }
        let mut cur = d;
        let end = cfg
            .leaders
            .iter()
            .copied()
            .find(|&l| l > leader)
            .unwrap_or(ops.len());
        for (i, op) in ops[leader..end].iter().enumerate() {
            depth[leader + i] = cur;
            let Some((needs, delta)) = caller_stack_effect(op) else {
                break;
            };
            if cur < needs {
                return None;
            }
            cur = (cur as i64 + delta) as usize;
        }
    }
    depth.iter().all(|&d| d != usize::MAX).then_some(depth)
}

/// The operand-stack effect of one caller op for the depth walk, mirroring
/// `analyze_cfg`: `PopHandler` touches no operand (it only drops a handler
/// frame), so the walk continues through it; a handler push and every other
/// unmodelled op ends the block — its successors are leaders with their own
/// entry depth.
fn caller_stack_effect(op: &Op) -> Option<(usize, i64)> {
    match op {
        Op::PopHandler => Some((0, 0)),
        other => super::compile::simple_effect(other).ok(),
    }
}

thread_local! {
    /// The fused body the compile in progress is lowering, so the block walk
    /// can tell a spliced op from the caller's own.
    static ACTIVE_FUSED: std::cell::RefCell<Option<std::rc::Rc<FusedBody>>> =
        const { std::cell::RefCell::new(None) };
}

/// The fused body being lowered, if this compile inlined anything.
pub(crate) fn active_fused() -> Option<std::rc::Rc<FusedBody>> {
    ACTIVE_FUSED.with(|f| f.borrow().clone())
}

/// RAII scope publishing `fused` for the compile inside it.
pub(crate) struct FusedScope(Option<std::rc::Rc<FusedBody>>);

impl FusedScope {
    pub(crate) fn enter(fused: std::rc::Rc<FusedBody>) -> Self {
        Self(ACTIVE_FUSED.with(|f| f.borrow_mut().replace(fused)))
    }
}

impl Drop for FusedScope {
    fn drop(&mut self) {
        let prev = self.0.take();
        ACTIVE_FUSED.with(|f| *f.borrow_mut() = prev);
        super::compile::lowering::set_active_region(None);
    }
}

/// Splice constant-bytecode callees into their caller before lowering. On by
/// default; `NEOVM_JIT_INLINE=off` (or `0`/`false`/`no`) disables it, which is
/// the A/B switch the measurements use.
pub(crate) fn jit_inline_on() -> bool {
    #[cfg(test)]
    if let Some(forced) = FORCE_INLINE.with(|f| f.get()) {
        return forced;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
thread_local! {
    static FORCE_INLINE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Test-only override of [`jit_inline_on`].
#[cfg(test)]
pub(crate) fn force_inline_for_test(on: Option<bool>) {
    FORCE_INLINE.with(|f| f.set(on));
}
