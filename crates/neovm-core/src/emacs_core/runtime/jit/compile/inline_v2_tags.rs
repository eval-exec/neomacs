//! Static call provenance for the opt-in v2 front. The analysis is a forward
//! must-analysis: every incoming edge must agree on a closure's prototype and
//! capture width. It selects regions; emission still guards the executing
//! object. All state is compile-local Rust data borrowing the compiling
//! mutator's rooted constant pool. No runtime cache or shared mutable state.

use std::collections::{BTreeMap, HashMap};

use super::{FusedBody, HofKind, HofSite, RegionKind};
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry;
use crate::emacs_core::bytecode::opcode::Op;
use crate::emacs_core::jit::NumericFeedback;
use crate::emacs_core::jit::compile::{self, Inline2Mode};
use crate::emacs_core::jit::inline;
use crate::emacs_core::value::Value;

/// A stack slot's compile-local provenance; indices refer only to the original
/// caller's rooted constant pool. A closure carries no creation-time captures.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tag {
    Unknown,
    Constant(u16),
    Closure { template: u16, prefix: usize },
}

fn is_named(tag: Tag, constants: &[Value], name: &str) -> bool {
    let Tag::Constant(i) = tag else {
        return false;
    };
    constants
        .get(i as usize)
        .and_then(|value| value.as_symbol_id())
        .is_some_and(|id| crate::emacs_core::intern::resolve_sym(id) == name)
}

fn transfer(op: &Op, constants: &[Value], tags: &mut Vec<Tag>) {
    match op {
        Op::Constant(i) => tags.push(Tag::Constant(*i)),
        Op::Dup => tags.push(tags.last().copied().unwrap_or(Tag::Unknown)),
        Op::StackRef(n) => tags.push(
            tags.len()
                .checked_sub(*n as usize + 1)
                .and_then(|i| tags.get(i))
                .copied()
                .unwrap_or(Tag::Unknown),
        ),
        Op::StackSet(n) => {
            let tag = tags.pop().unwrap_or(Tag::Unknown);
            if *n > 0 && tags.len() >= *n as usize {
                let at = tags.len() - *n as usize;
                tags[at] = tag;
            }
        }
        Op::DiscardN(raw) => {
            let n = (*raw & 0x7f) as usize;
            if raw & 0x80 != 0 && n > 0 {
                let top = tags.pop().unwrap_or(Tag::Unknown);
                tags.truncate(tags.len().saturating_sub(n));
                tags.push(top);
            } else {
                tags.truncate(tags.len().saturating_sub(n));
            }
        }
        Op::Call(n) | Op::Apply(n) => {
            let n = *n as usize;
            let base = tags.len().checked_sub(n + 1);
            let result = if matches!(op, Op::Call(_))
                && n > 0
                && base.is_some_and(|i| is_named(tags[i], constants, "make-closure"))
                && let Some(Tag::Constant(template)) = base.and_then(|i| tags.get(i + 1))
                && let Some(proto) = constants
                    .get(*template as usize)
                    .and_then(|v| v.get_bytecode_data())
                && proto.env.is_none()
                && n - 1 <= proto.constants.len()
            {
                Tag::Closure {
                    template: *template,
                    prefix: (n - 1).max(proto.jit_runtime().patched_prefix()),
                }
            } else {
                Tag::Unknown
            };
            tags.truncate(tags.len().saturating_sub(n + 1));
            tags.push(result);
        }
        Op::Goto(_) | Op::PopHandler | Op::Unbind(_) => {}
        other => match compile::simple_effect(other) {
            Ok((needs, delta)) => {
                tags.truncate(tags.len().saturating_sub(needs));
                tags.extend(std::iter::repeat_n(
                    Tag::Unknown,
                    (needs as i64 + delta).max(0) as usize,
                ));
            }
            Err(_) => tags.clear(),
        },
    }
}

fn entry_states(ops: &[Op], constants: &[Value], leaders: &[usize]) -> HashMap<usize, Vec<Tag>> {
    let n = ops.len();
    let next_leader = |i: usize| {
        let at = leaders.partition_point(|&l| l <= i);
        leaders.get(at).copied().unwrap_or(n)
    };
    let mut entry: HashMap<usize, Vec<Tag>> = HashMap::new();
    let mut work = Vec::new();
    let flow = |target: usize,
                incoming: &[Tag],
                entry: &mut HashMap<usize, Vec<Tag>>,
                work: &mut Vec<usize>| {
        if target >= n {
            return;
        }
        let meet = match entry.get(&target) {
            None => incoming.to_vec(),
            Some(current) => {
                let k = current.len().min(incoming.len());
                current[current.len() - k..]
                    .iter()
                    .zip(&incoming[incoming.len() - k..])
                    .map(|(&a, &b)| if a == b { a } else { Tag::Unknown })
                    .collect()
            }
        };
        let from = meet
            .iter()
            .position(|tag| *tag != Tag::Unknown)
            .unwrap_or(meet.len());
        let meet = meet[from..].to_vec();
        if entry.get(&target) != Some(&meet) {
            entry.insert(target, meet);
            work.push(target);
        }
    };
    flow(0, &[], &mut entry, &mut work);
    while let Some(block) = work.pop() {
        let mut tags = entry[&block].clone();
        let end = next_leader(block);
        let mut falls_through = true;
        for op in &ops[block..end] {
            match op {
                Op::Return | Op::Throw => {
                    falls_through = false;
                    break;
                }
                Op::Goto(t) => {
                    flow(*t as usize, &tags, &mut entry, &mut work);
                    falls_through = false;
                    break;
                }
                Op::GotoIfNil(t) | Op::GotoIfNotNil(t) => {
                    tags.pop();
                    flow(*t as usize, &tags, &mut entry, &mut work);
                }
                Op::GotoIfNilElsePop(t) | Op::GotoIfNotNilElsePop(t) => {
                    flow(*t as usize, &tags, &mut entry, &mut work);
                    tags.pop();
                }
                Op::PushConditionCase(t) | Op::PushConditionCaseRaw(t) | Op::PushCatch(t) => {
                    flow(*t as usize, &[], &mut entry, &mut work);
                    transfer(op, constants, &mut tags);
                }
                _ => transfer(op, constants, &mut tags),
            }
        }
        if falls_through {
            flow(end, &tags, &mut entry, &mut work);
        }
    }
    entry
}

fn v2_depths(
    callee: &ByteCodeFunction,
    nargs: usize,
    pool_len: usize,
) -> Result<Vec<usize>, String> {
    inline::census_callee_verdict(callee, nargs)?;
    let ops = callee.executable_ops();
    if ops.is_empty() || !matches!(ops.last(), Some(Op::Return)) {
        return Err("no-return".into());
    }
    if callee.jit_runtime().heat() == 0 {
        return Err("cold".into());
    }
    if pool_len + callee.constants.len() > u16::MAX as usize + 1 {
        return Err("constant-pool".into());
    }
    let feedback = inline::callee_feedback(callee);
    for (pc, op) in ops.iter().enumerate() {
        // These stores use the emission hook's guarded, GC-free fast path.
        // A barrier or wrong-type slow path resumes Tier-0 inside the callee.
        if !inline::op_is_inlinable(op, feedback[pc]) && !matches!(op, Op::Setcar | Op::Setcdr) {
            return Err(format!("op:{}", inline::op_census_name(op)));
        }
        if inline::jump_target(op).is_some_and(|t| t as usize <= pc) {
            return Err("back-edge".into());
        }
        if inline::jump_target(op).is_some_and(|t| t as usize >= ops.len()) {
            return Err("jump-out".into());
        }
    }
    inline::callee_depths(callee, nargs).ok_or_else(|| "depths".into())
}

pub(super) fn fuse_static(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    feedback: &[NumericFeedback],
) -> Option<StaticFront> {
    fuse_front(ops, constants, offset_map, arity, feedback, None)
}

/// The same front admits named cells at a T2 compile, preserving static
/// closure and HOF sites in mixed callers. Threading: observed function
/// objects belong to this mutator and are rooted in the resulting pool.
pub(super) fn fuse_named(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    feedback: &[NumericFeedback],
    obarray: &crate::emacs_core::symbol::Obarray,
) -> Option<StaticFront> {
    fuse_front(ops, constants, offset_map, arity, feedback, Some(obarray))
}

fn fuse_front(
    ops: &[Op],
    constants: &[Value],
    offset_map: Option<&[GnuByteOffsetMapEntry]>,
    arity: usize,
    feedback: &[NumericFeedback],
    obarray: Option<&crate::emacs_core::symbol::Obarray>,
) -> Option<StaticFront> {
    if offset_map.is_none() && ops.iter().any(|op| matches!(op, Op::Switch)) {
        return None;
    }
    let cfg = compile::analyze_cfg(ops, constants, offset_map, arity).ok()?;
    let depth = inline::op_depths(ops, &cfg)?;
    let entries = entry_states(ops, constants, &cfg.leaders);
    let closures = matches!(
        compile::jit_inline2_mode(),
        Inline2Mode::Closure | Inline2Mode::All
    );
    let hofs = matches!(
        compile::jit_inline2_mode(),
        Inline2Mode::Hof | Inline2Mode::All
    );
    let mut tags = Vec::new();
    let mut sites = Vec::new();
    let mut kinds = Vec::new();
    let mut hof_candidates = BTreeMap::new();
    let mut pool_len = constants.len();
    let mut pool = constants.to_vec();
    let mut growth = ops.len();
    let mut handlers = 0;
    for (pc, op) in ops.iter().enumerate() {
        if cfg.leaders.binary_search(&pc).is_ok() {
            tags = entries.get(&pc).cloned().unwrap_or_default();
            handlers = cfg.entry_handlers.get(&pc).map_or(0, Vec::len);
        }
        if let Op::Call(n) = op {
            let nargs = *n as usize;
            let tag = tags
                .len()
                .checked_sub(nargs + 1)
                .and_then(|i| tags.get(i))
                .copied()
                .unwrap_or(Tag::Unknown);
            if hofs && nargs == 2 && handlers == 0 && compile::call_site_inlinable_at(pc) {
                let kind = if is_named(tag, constants, "mapc") {
                    Some(HofKind::Mapc)
                } else if is_named(tag, constants, "mapcar") {
                    Some(HofKind::Mapcar)
                } else {
                    None
                };
                let callback = tags
                    .len()
                    .checked_sub(2)
                    .and_then(|at| tags.get(at))
                    .copied()
                    .unwrap_or(Tag::Unknown);
                let target = match callback {
                    Tag::Constant(i) => Some((i, false, 0)),
                    Tag::Closure { template, prefix } => Some((template, true, prefix)),
                    Tag::Unknown => None,
                };
                if let Some(kind) = kind
                    && let Some((index, closure, prefix)) = target
                    && let Some(callback) = constants.get(index as usize)
                    && let Some(callee) = callback.get_bytecode_data()
                    && v2_depths(callee, 1, pool_len + 1).is_ok()
                {
                    hof_candidates.insert(
                        pc,
                        HofSite {
                            callback: *callback,
                            closure,
                            prefix: prefix.max(callee.jit_runtime().patched_prefix()),
                            kind,
                            call_site_pc: pc,
                            const_base: 0,
                        },
                    );
                }
            }
            let candidate = match tag {
                Tag::Constant(i) => constants.get(i as usize).and_then(|&target| {
                    if target.is_bytecode() {
                        return Some((i, RegionKind::Constant, target));
                    }
                    let symbol = target.as_symbol_id()?;
                    if crate::emacs_core::jit::cache::inline_callee_is_unstable(symbol) {
                        return None;
                    }
                    let target = obarray?.symbol_function_id(symbol)?;
                    Some((i, RegionKind::Named { symbol }, target))
                }),
                Tag::Closure { template, prefix } if closures => Some((
                    template,
                    RegionKind::Closure {
                        prefix,
                        const_base: pool_len,
                    },
                    *constants.get(template as usize)?,
                )),
                _ => None,
            };
            if handlers == 0
                && depth[pc] > nargs
                && let Some((template, kind, target)) = candidate
                && let Some(callee) = target.get_bytecode_data()
            {
                // Match the existing fuser's census for a proven bytecode
                // target that reoptimization requires to remain a call.
                if !compile::call_site_inlinable_at(pc) {
                    crate::emacs_core::jit::stats::record_inline("reject:reopt");
                    transfer(op, constants, &mut tags);
                    continue;
                }
                let named = matches!(kind, RegionKind::Named { .. });
                let extra = callee.executable_ops().len()
                    + callee
                        .executable_ops()
                        .iter()
                        .filter(|op| matches!(op, Op::Return))
                        .count();
                let verdict = match kind {
                    RegionKind::Constant | RegionKind::Named { .. }
                        if callee.jit_runtime().patched_prefix() > 0 =>
                    {
                        Err("patched-prefix".into())
                    }
                    RegionKind::Named { .. }
                        if growth.saturating_add(extra) > ops.len().saturating_mul(4).min(1024) =>
                    {
                        Err("named-growth".into())
                    }
                    _ => v2_depths(callee, nargs, pool_len + usize::from(named)),
                };
                match verdict {
                    Ok(depths) => {
                        let template = if named {
                            let index = u16::try_from(pool.len()).ok()?;
                            pool.push(target);
                            index
                        } else {
                            template
                        };
                        pool_len += callee.constants.len() + usize::from(named);
                        growth += extra;
                        sites.push((pc, template, nargs, depths));
                        kinds.push(kind);
                    }
                    Err(why) => {
                        crate::emacs_core::jit::stats::record_inline(format!("reject:{why}"))
                    }
                }
            }
        }
        transfer(op, constants, &mut tags);
        match op {
            Op::PushConditionCase(_) | Op::PushConditionCaseRaw(_) | Op::PushCatch(_) => {
                handlers += 1
            }
            Op::PopHandler => handlers = handlers.saturating_sub(1),
            _ => {}
        }
    }
    if sites.is_empty() && hof_candidates.is_empty() {
        return None;
    }
    let mut body = if sites.is_empty() {
        FusedBody {
            v2: None,
            ops: ops.to_vec(),
            constants: constants.to_vec(),
            feedback: (0..ops.len())
                .map(|pc| {
                    feedback
                        .get(pc)
                        .copied()
                        .unwrap_or(NumericFeedback::FixnumOnly)
                })
                .collect(),
            offset_map: offset_map.map(<[_]>::to_vec),
            regions: Vec::new(),
            region_of: vec![None; ops.len()],
            caller_of_fused: (0..ops.len()).collect(),
        }
    } else {
        inline::splice_sites(ops, &pool, offset_map, feedback, &depth, &sites)?
    };
    // Prefix loads read the executing object. The prototype's placeholder (or
    // a previously captured fixnum) must never prove an SSA slot fixnum or a
    // call target constant before the runtime load hook replaces the Constant.
    let mut const_base = pool.len();
    for (kind, region) in kinds.iter_mut().zip(&body.regions) {
        if let RegionKind::Closure {
            prefix,
            const_base: base,
        } = kind
        {
            *base = const_base;
            let end = const_base.checked_add(*prefix)?;
            body.constants.get_mut(const_base..end)?.fill(Value::NIL);
        }
        const_base += Value::from_bits(region.callee_bits as usize)
            .get_bytecode_data()?
            .constants
            .len();
    }
    let mut hof_at = BTreeMap::new();
    for (fused_pc, &original_pc) in body.caller_of_fused.iter().enumerate() {
        if body.region_of[fused_pc].is_some() {
            continue;
        }
        if let Some(mut site) = hof_candidates.remove(&original_pc) {
            let callee = site
                .callback
                .get_bytecode_data()
                .expect("admitted callback");
            if body.constants.len() + callee.constants.len() + 1 > u16::MAX as usize + 1 {
                crate::emacs_core::jit::stats::record_inline("reject:constant-pool");
                continue;
            }
            site.const_base = body.constants.len();
            body.constants.extend_from_slice(&callee.constants);
            let end = site.const_base.checked_add(site.prefix)?;
            body.constants
                .get_mut(site.const_base..end)?
                .fill(Value::NIL);
            body.constants.push(site.callback);
            hof_at.insert(fused_pc, site);
        }
    }
    if body.regions.is_empty() && hof_at.is_empty() {
        return None;
    }
    for kind in &kinds {
        crate::emacs_core::jit::stats::record_inline("fused");
        if matches!(kind, RegionKind::Named { .. }) {
            crate::emacs_core::jit::stats::record_inline("fused:named");
        }
    }
    Some(StaticFront {
        body,
        region_kind: kinds.into_boxed_slice(),
        hof_at,
    })
}

/// A complete compile-local front result. Lisp handles are rooted by `body`;
/// no running mutator state or shared cache survives this construction.
pub(super) struct StaticFront {
    pub(super) body: FusedBody,
    pub(super) region_kind: Box<[RegionKind]>,
    pub(super) hof_at: BTreeMap<usize, HofSite>,
}
