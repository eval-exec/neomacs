//! Bounded fixnum intervals and current-array bounds proofs.
//!
//! Threading: interval, CFG and scalar census state belong to one compilation.
//! Constants are opaque bits; no runtime cache or Lisp heap access is used.

use std::collections::{HashMap, HashSet};

use super::{
    array_range_state::LengthVersions,
    array_reads::{self, ArrayReadProofs, BoundsWitness, NumericBoundsWitness},
    cfg::Dominance,
};
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::{Range, TypeSet},
    verify::VerifyError,
};

/// Actual transformations and explicit bounded-analysis skips only.
/// Threading: compiler-owned scalar metadata, immutable after leaf publication.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RangeStats {
    pub(crate) overflow_checks_elided: usize,
    pub(crate) bounds_checks_elided: usize,
    pub(crate) range_views: usize,
    pub(crate) analysis_bailed: usize,
}

/// Transactional range stage. A bounded-solver bail publishes no interval
/// views or operation changes; it reports the real skipped analysis separately.
/// Original value/phi types, frames, source states and entry IDs stay unchanged.
pub(crate) fn run(func: &mut Func) -> Result<RangeStats, VerifyError> {
    let mut proofs = func.array_reads.clone();
    run_with_array_proofs(func, &mut proofs)
}

/// The explicit table is installed in a private Func candidate before input
/// verification. This keeps the caller outputs unchanged on a bounded bail or
/// rejected candidate and prevents stale Checked sidecars during final verify.
/// The original verified table admits read-only array macros; unsupported
/// numeric witness kinds are omitted BEFORE any mutation/count increment.
pub(crate) fn run_with_array_proofs(
    func: &mut Func,
    proofs: &mut ArrayReadProofs,
) -> Result<RangeStats, VerifyError> {
    let mut candidate = func.clone();
    candidate.array_reads = proofs.clone();
    candidate.verify()?;
    let supported = if proofs.reads.is_empty() {
        HashMap::new()
    } else {
        // The query verifies the complete input table with one shared
        // position/dominance context; do not repeat that graph walk here.
        array_reads::checked_index_ranges(&candidate, proofs)
            .map_err(|e| VerifyError::InvalidInst(e.inst))?
    };
    let dom = Dominance::new(&candidate)?;
    let Some(analysis) = analyze(&candidate, &dom) else {
        return Ok(RangeStats {
            analysis_bailed: 1,
            ..RangeStats::default()
        });
    };
    let mut table = proofs.clone();
    let stats = rewrite(&mut candidate, &dom, &analysis, &mut table, &supported);
    candidate.array_reads = table.clone();
    candidate.verify()?;
    if !table.reads.is_empty() {
        array_reads::verify_reads(&candidate, &table)
            .map_err(|e| VerifyError::InvalidInst(e.inst))?;
    }
    *func = candidate;
    *proofs = table;
    Ok(stats)
}

/// A pending definition is lattice bottom, not an unknown runtime value.
/// Unknown dominates intervals; only a completed monotone fixpoint is used.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Fact {
    Pending,
    Unknown,
    Interval(Range),
}

impl Fact {
    fn join(self, other: Self) -> Self {
        match (self, other) {
            (Self::Pending, value) | (value, Self::Pending) => value,
            (Self::Interval(a), Self::Interval(b)) => Self::Interval(a.join(b)),
            _ => Self::Unknown,
        }
    }
    fn interval(self) -> Option<Range> {
        match self {
            Self::Interval(range) => Some(range),
            _ => None,
        }
    }
}

/// Immutable compiler facts. Point proofs use numeric identity origins without
/// changing original phi declarations or treating a view as a new runtime value.
struct Analysis {
    facts: Vec<Fact>,
    origins: Vec<Value>,
}

fn declared(func: &Func, value: Value) -> Fact {
    let ty = func.values[func.resolve(value).expect("verified SSA").index()].ty;
    if !ty.is_bottom() && ty.is_subset(TypeSet::FIXNUM) {
        Fact::Interval(ty.range().expect("fixnum range"))
    } else {
        Fact::Unknown
    }
}

fn restrict(fact: Fact, declaration: TypeSet) -> Fact {
    match fact {
        Fact::Pending => Fact::Pending,
        Fact::Interval(range) => declaration
            .range()
            .and_then(|allowed| range.meet(allowed))
            .map_or(Fact::Unknown, Fact::Interval),
        Fact::Unknown if !declaration.is_bottom() && declaration.is_subset(TypeSet::FIXNUM) => {
            Fact::Interval(declaration.range().expect("declared fixnum"))
        }
        _ => Fact::Unknown,
    }
}

fn analyze(func: &Func, dom: &Dominance) -> Option<Analysis> {
    let mut incoming = vec![Vec::new(); func.blocks.len()];
    let mut headers = vec![false; func.blocks.len()];
    for &block in dom.reverse_postorder() {
        for edge in func.blocks[block.index()].term.edges() {
            incoming[edge.target.index()].push((block, edge.args.as_slice()));
            if dom.dominates(edge.target, block) {
                headers[edge.target.index()] = true;
            }
        }
    }
    let origins = numeric_origins(func);
    let mut analysis = Analysis {
        facts: vec![Fact::Pending; func.values.len()],
        origins,
    };
    let units = func
        .insts
        .len()
        .saturating_add(func.values.len())
        .saturating_add(func.blocks.len())
        .max(1);
    let scans = (1_000_000 / units).clamp(2, 64);
    for _ in 0..scans {
        let mut changed = false;
        for &block in dom.reverse_postorder() {
            for (position, &param) in func.blocks[block.index()].params.iter().enumerate() {
                let joined = incoming[block.index()]
                    .iter()
                    .fold(Fact::Pending, |old, (_, args)| {
                        old.join(analysis.facts[args[position].index()])
                    });
                let next = restrict(joined, func.values[param.index()].ty);
                let old = analysis.facts[param.index()];
                let mut joined = old.join(next);
                if headers[block.index()] || func.blocks[block.index()].loop_header.is_some() {
                    if let (Fact::Interval(old), Fact::Interval(next)) = (old, joined) {
                        joined = Fact::Interval(old.widen(next));
                    }
                }
                joined = restrict(joined, func.values[param.index()].ty);
                if joined != old {
                    analysis.facts[param.index()] = joined;
                    changed = true;
                }
            }
            for &id in &func.blocks[block.index()].insts {
                let inst = &func.insts[id.index()];
                if let Some(result) = inst.result {
                    let next = transfer(func, inst, &analysis.facts);
                    let old = analysis.facts[result.index()];
                    let joined = old.join(next);
                    if joined != old {
                        analysis.facts[result.index()] = joined;
                        changed = true;
                    }
                }
            }
        }
        for (index, data) in func.values.iter().enumerate() {
            if let ValueDef::Alias(next) = data.def {
                let old = analysis.facts[index];
                let joined = old.join(analysis.facts[next.index()]);
                if joined != old {
                    analysis.facts[index] = joined;
                    changed = true;
                }
            }
        }
        if !changed {
            return Some(analysis);
        }
    }
    None
}

fn transfer(func: &Func, inst: &InstData, facts: &[Fact]) -> Fact {
    let result = inst.result.expect("transfer result");
    let declaration = func.values[result.index()].ty;
    let seed = declared(func, result);
    let input = |index: usize| facts[inst.args[index].index()];
    match inst.op {
        Opcode::Const(index) if index as usize >= func.dynamic_prefix => {
            let ty = TypeSet::for_constant(func.consts[index as usize]);
            if !ty.is_bottom() && ty.is_subset(TypeSet::FIXNUM) {
                restrict(
                    Fact::Interval(ty.range().expect("literal range")),
                    declaration,
                )
            } else {
                Fact::Unknown
            }
        }
        Opcode::Refine(_) | Opcode::TagFix | Opcode::UntagFix => restrict(input(0), declaration),
        Opcode::CheckType(_) => match input(0) {
            Fact::Pending => Fact::Pending,
            fact => restrict(fact, declaration),
        },
        Opcode::FixAdd { .. } | Opcode::FixSub { .. } | Opcode::FixMul { .. } => {
            match (input(0), input(1)) {
                (Fact::Pending, _) | (_, Fact::Pending) => Fact::Pending,
                (Fact::Interval(a), Fact::Interval(b)) => {
                    if matches!(
                        inst.op,
                        Opcode::FixAdd { checked: false }
                            | Opcode::FixSub { checked: false }
                            | Opcode::FixMul { checked: false }
                    ) && !arithmetic_fits(&inst.op, a, b)
                    {
                        return Fact::Unknown;
                    }
                    let (lo, hi) = arithmetic_endpoints(&inst.op, a, b).expect("arithmetic opcode");
                    let lo = lo.max(i128::from(Range::FULL.lo));
                    let hi = hi.min(i128::from(Range::FULL.hi));
                    if lo <= hi {
                        restrict(
                            Fact::Interval(Range {
                                lo: lo as i64,
                                hi: hi as i64,
                            }),
                            declaration,
                        )
                    } else {
                        Fact::Unknown
                    }
                }
                _ => seed,
            }
        }
        Opcode::CheckBounds if inst.args.len() == 2 => match (input(0), input(1)) {
            (Fact::Pending, _) | (_, Fact::Pending) => Fact::Pending,
            (Fact::Interval(index), Fact::Interval(len)) => {
                let interval = Range {
                    lo: 0,
                    hi: len.hi.saturating_sub(1),
                };
                index.meet(interval).map_or(Fact::Unknown, |range| {
                    restrict(Fact::Interval(range), declaration)
                })
            }
            _ => seed,
        },
        Opcode::Select => restrict(input(1).join(input(2)), declaration),
        Opcode::LoadVecLen => restrict(
            Fact::Interval(Range {
                lo: 0,
                hi: Range::FULL.hi,
            }),
            declaration,
        ),
        _ => seed,
    }
}

/// Wider arithmetic is exact for every combination of two fixnum endpoints.
/// Div/rem, min/-1, shifts and numeric unions have separate prerequisites and
/// cannot acquire an unchecked proof from this function.
pub(crate) fn arithmetic_endpoints(op: &Opcode, a: Range, b: Range) -> Option<(i128, i128)> {
    let (alo, ahi, blo, bhi) = (
        i128::from(a.lo),
        i128::from(a.hi),
        i128::from(b.lo),
        i128::from(b.hi),
    );
    match op {
        Opcode::FixAdd { .. } => Some((alo + blo, ahi + bhi)),
        Opcode::FixSub { .. } => Some((alo - bhi, ahi - blo)),
        Opcode::FixMul { .. } => {
            let candidates = [alo * blo, alo * bhi, ahi * blo, ahi * bhi];
            Some((*candidates.iter().min()?, *candidates.iter().max()?))
        }
        _ => None,
    }
}

/// Shared compile-time predicate for native independent unchecked admission.
pub(crate) fn arithmetic_fits(op: &Opcode, a: Range, b: Range) -> bool {
    arithmetic_endpoints(op, a, b).is_some_and(|(lo, hi)| {
        lo >= i128::from(Range::FULL.lo) && hi <= i128::from(Range::FULL.hi)
    })
}

/// Exact producer/consumer contract for unchecked arithmetic. A successful
/// fixnum-domain proof alone is insufficient if independent interval endpoints
/// are wider than the retained output declaration. Do not widen that original
/// declaration or count an elimination which native admission must refuse.
/// Correlated inputs (x-x, x*x) may have a narrower valid result; this bounded
/// interval stage deliberately retains their check unless endpoints prove it.
pub(crate) fn arithmetic_result_fits(op: &Opcode, a: Range, b: Range, output: TypeSet) -> bool {
    if output.is_bottom() || !output.is_subset(TypeSet::FIXNUM) || !arithmetic_fits(op, a, b) {
        return false;
    }
    let Some((lo, hi)) = arithmetic_endpoints(op, a, b) else {
        return false;
    };
    TypeSet::fixnum_range(Range {
        lo: lo as i64,
        hi: hi as i64,
    })
    .is_subset(output)
}

fn numeric_origins(func: &Func) -> Vec<Value> {
    let mut origins = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut value = Value(index as u32);
        while origins[value.index()].is_none() {
            path.push(value);
            let next = match func.values[value.index()].def {
                ValueDef::Alias(next) => next,
                ValueDef::Inst(id)
                    if matches!(
                        func.insts[id.index()].op,
                        Opcode::CheckType(_)
                            | Opcode::Refine(_)
                            | Opcode::TagFix
                            | Opcode::UntagFix
                    ) =>
                {
                    func.insts[id.index()].args[0]
                }
                _ => {
                    origins[value.index()] = Some(value);
                    break;
                }
            };
            value = next;
        }
        let origin = origins[value.index()].expect("verified noncyclic identity views");
        for value in path.drain(..) {
            origins[value.index()] = Some(origin);
        }
    }
    origins
        .into_iter()
        .map(|origin| origin.expect("all identities visited"))
        .collect()
}

/// Dominance-scoped point facts. Only ranges proved on every path to the current
/// block survive; original SSA/phi declarations are never narrowed from an edge.
struct Points {
    facts: HashMap<Value, Range>,
    undo: Vec<(Value, Option<Range>)>,
}
impl Points {
    fn put(&mut self, value: Value, range: Range) {
        let range = match self.facts.get(&value).copied() {
            Some(old) => match old.meet(range) {
                Some(range) => range,
                None => return,
            },
            None => range,
        };
        let old = self.facts.insert(value, range);
        self.undo.push((value, old));
    }
    fn restore(&mut self, mark: usize) {
        while self.undo.len() > mark {
            let (value, old) = self.undo.pop().expect("point scope");
            match old {
                Some(range) => {
                    self.facts.insert(value, range);
                }
                None => {
                    self.facts.remove(&value);
                }
            }
        }
    }
}

fn point(analysis: &Analysis, points: &Points, value: Value) -> Option<Range> {
    let global = analysis.facts[value.index()].interval();
    let local = points.facts.get(&analysis.origins[value.index()]).copied();
    match (global, local) {
        (Some(global), Some(local)) => global.meet(local),
        (global, local) => global.or(local),
    }
}

enum Visit {
    Enter(Block),
    Exit(usize),
}

fn rewrite(
    func: &mut Func,
    dom: &Dominance,
    analysis: &Analysis,
    proofs: &mut ArrayReadProofs,
    supported: &HashMap<Inst, Range>,
) -> RangeStats {
    let mut children = vec![Vec::new(); func.blocks.len()];
    for &block in dom.reverse_postorder() {
        if let Some(parent) = dom.immediate_dominator(block) {
            children[parent.index()].push(block);
        }
    }
    let mut points = Points {
        facts: HashMap::new(),
        undo: Vec::new(),
    };
    let mut lengths = LengthVersions::new();
    let mut by_bounds = HashMap::<Inst, Vec<Inst>>::new();
    let mut known_loads = HashSet::new();
    for (&read, proof) in &proofs.reads {
        by_bounds.entry(proof.bounds).or_default().push(read);
        known_loads.insert(proof.length_read);
    }
    // Hash-map iteration affects neither chosen provenance nor emitted order.
    for owners in by_bounds.values_mut() {
        owners.sort_unstable();
    }
    let mut stats = RangeStats::default();
    let mut pending = vec![Visit::Enter(func.entry)];
    while let Some(visit) = pending.pop() {
        match visit {
            Visit::Exit(mark) => points.restore(mark),
            Visit::Enter(block) => {
                let mark = points.undo.len();
                // Match final native verifier: no layout fact crosses ANY
                // block boundary, including unique-pred blocks/backedges.
                lengths.begin_block();
                refine_single_edge(func, dom, analysis, &mut points, block);
                let original = func.blocks[block.index()].insts.clone();
                let mut rewritten = Vec::with_capacity(original.len());
                let mut views = HashMap::new();
                for id in original {
                    let inst = func.insts[id.index()].clone();
                    if array_reads::layout_barrier(&inst, proofs.reads.contains_key(&id)) {
                        lengths.kill();
                    }
                    if known_loads.contains(&id) {
                        let base = analysis.origins[inst.args[0].index()];
                        lengths.loaded(func, id, base);
                    }
                    let checked = matches!(
                        inst.op,
                        Opcode::FixAdd { checked: true }
                            | Opcode::FixSub { checked: true }
                            | Opcode::FixMul { checked: true }
                    );
                    if checked {
                        if let (Some(a), Some(b)) = (
                            point(analysis, &points, inst.args[0]),
                            point(analysis, &points, inst.args[1]),
                        ) {
                            let result = inst.result.expect("verified checked arithmetic result");
                            let output = func.values[result.index()].ty;
                            if arithmetic_result_fits(&inst.op, a, b, output) {
                                let args = [
                                    interval_view(
                                        func,
                                        &mut rewritten,
                                        &mut views,
                                        inst.pc,
                                        inst.args[0],
                                        a,
                                        &mut stats,
                                    ),
                                    interval_view(
                                        func,
                                        &mut rewritten,
                                        &mut views,
                                        inst.pc,
                                        inst.args[1],
                                        b,
                                        &mut stats,
                                    ),
                                ];
                                let current = &mut func.insts[id.index()];
                                current.args = args.to_vec();
                                current.op = match current.op {
                                    Opcode::FixAdd { .. } => Opcode::FixAdd { checked: false },
                                    Opcode::FixSub { .. } => Opcode::FixSub { checked: false },
                                    _ => Opcode::FixMul { checked: false },
                                };
                                current.eff = Effects::PURE;
                                current.mem = AliasClass::None;
                                stats.overflow_checks_elided += 1;
                            }
                        }
                    } else if inst.op == Opcode::CheckBounds {
                        if let Some(owners) = by_bounds.get(&id) {
                            // Native array omission requires an actual source
                            // interval and a frozen prior executed-guard floor.
                            // Do not substitute stronger branch/induction facts.
                            if let (Some(&index), Some(&first)) =
                                (supported.get(&id), owners.first())
                            {
                                let proof = proofs.reads[&first].clone();
                                if let Some(fact) = lengths.fact(proof.length).cloned() {
                                    let input =
                                        func.resolve(proof.checked_index).expect("verified index");
                                    let output = &func.values[proof.bounds_result.index()];
                                    let actual = &func.values[input.index()];
                                    let output_ty = output.ty;
                                    if matches!(proof.witness, BoundsWitness::Checked)
                                        && fact.base == analysis.origins[proof.guarded_base.index()]
                                        && fact.load == proof.length_read
                                        && fact.value == proof.length
                                        && index.lo >= 0
                                        && index.hi < fact.range.lo
                                        && output.rep == actual.rep
                                        && output_ty.is_subset(actual.ty)
                                    {
                                        if let Some(floor) = fact.floor {
                                            let index_view = interval_view(
                                                func,
                                                &mut rewritten,
                                                &mut views,
                                                inst.pc,
                                                proof.checked_index,
                                                index,
                                                &mut stats,
                                            );
                                            let length_view = interval_view(
                                                func,
                                                &mut rewritten,
                                                &mut views,
                                                inst.pc,
                                                proof.length,
                                                fact.range,
                                                &mut stats,
                                            );
                                            let current = &mut func.insts[id.index()];
                                            current.op = Opcode::Refine(output_ty);
                                            // Full original result declaration stays;
                                            // narrowed metadata views are separate.
                                            current.args = vec![proof.checked_index];
                                            current.eff = Effects::PURE;
                                            current.mem = AliasClass::None;
                                            let witness = NumericBoundsWitness {
                                                index_view,
                                                length_view,
                                                floor,
                                            };
                                            for &owner in owners {
                                                proofs
                                                    .reads
                                                    .get_mut(&owner)
                                                    .expect("known bounds owner")
                                                    .witness =
                                                    BoundsWitness::Elided(witness.clone());
                                            }
                                            stats.bounds_checks_elided += 1;
                                        }
                                    }
                                }
                            }
                        } else if let (Some(index), Some(len), Some(result)) = (
                            point(analysis, &points, inst.args[0]),
                            point(analysis, &points, inst.args[1]),
                            inst.result,
                        ) {
                            // Scalar Bounds not certifying an Aref preserves the
                            // original general numeric proof path.
                            let input = func.resolve(inst.args[0]).expect("verified index");
                            let output = &func.values[result.index()];
                            let actual = &func.values[input.index()];
                            if index.lo >= 0
                                && index.hi < len.lo
                                && output.rep == actual.rep
                                && output.ty.is_subset(actual.ty)
                            {
                                let current = &mut func.insts[id.index()];
                                current.op = Opcode::Refine(output.ty);
                                current.args = vec![inst.args[0]];
                                current.eff = Effects::PURE;
                                current.mem = AliasClass::None;
                                stats.bounds_checks_elided += 1;
                            }
                        }
                    }
                    // CURRENT opcode is inspected inside the state helper.
                    // An elided Refine can use a prior witness, never create one.
                    if let Some(&native_index) = supported.get(&id) {
                        lengths.successful_bounds(func, id, native_index);
                    }
                    rewritten.push(id);
                    // Successful original guards establish point facts even
                    // when a proved check was replaced by its semantic view.
                    if matches!(inst.op, Opcode::CheckType(_) | Opcode::Refine(_)) {
                        if let Some(result) = inst.result {
                            if let Some(range) = analysis.facts[result.index()].interval() {
                                points.put(analysis.origins[result.index()], range);
                            }
                        }
                    } else if inst.op == Opcode::CheckBounds {
                        if let (Some(index), Some(len)) = (
                            point(analysis, &points, inst.args[0]),
                            point(analysis, &points, inst.args[1]),
                        ) {
                            if let Some(range) = index.meet(Range {
                                lo: 0,
                                hi: len.hi.saturating_sub(1),
                            }) {
                                points.put(analysis.origins[inst.args[0].index()], range);
                            }
                            if let Some(range) = len.meet(Range {
                                lo: index.lo.saturating_add(1),
                                hi: Range::FULL.hi,
                            }) {
                                points.put(analysis.origins[inst.args[1].index()], range);
                            }
                        }
                    }
                }
                func.blocks[block.index()].insts = rewritten;
                pending.push(Visit::Exit(mark));
                pending.extend(
                    children[block.index()]
                        .iter()
                        .rev()
                        .map(|&child| Visit::Enter(child)),
                );
            }
        }
    }
    stats
}

fn interval_view(
    func: &mut Func,
    attached: &mut Vec<Inst>,
    views: &mut HashMap<(Value, Range), Value>,
    pc: u32,
    input: Value,
    range: Range,
    stats: &mut RangeStats,
) -> Value {
    let actual = func.resolve(input).expect("verified operand");
    let data = &func.values[actual.index()];
    if data.ty.range() == Some(range) {
        return input;
    }
    if let Some(&view) = views.get(&(input, range)) {
        return view;
    }
    let ty = data.ty.meet(TypeSet::fixnum_range(range));
    let rep = data.rep;
    let id = Inst(func.insts.len() as u32);
    let value = Value(func.values.len() as u32);
    func.values.push(ValueData {
        ty,
        rep,
        def: ValueDef::Inst(id),
    });
    func.insts.push(InstData {
        op: Opcode::Refine(ty),
        args: vec![input],
        result: Some(value),
        eff: Effects::PURE,
        mem: AliasClass::None,
        frame: None,
        pc,
    });
    attached.push(id);
    views.insert((input, range), value);
    stats.range_views += 1;
    value
}

fn refine_single_edge(
    func: &Func,
    dom: &Dominance,
    analysis: &Analysis,
    points: &mut Points,
    block: Block,
) {
    let mut preds = func.blocks[block.index()].preds.clone();
    preds.retain(|&pred| dom.is_reachable(pred));
    preds.sort_unstable();
    preds.dedup();
    if preds.len() != 1 {
        return;
    }
    let parent = preds[0];
    if !dom.dominates(parent, block) || parent == block {
        return;
    }
    let Term::Branch {
        flag,
        if_true,
        if_false,
    } = &func.blocks[parent.index()].term
    else {
        return;
    };
    if if_true.target == if_false.target {
        return;
    }
    let truth = if if_true.target == block {
        true
    } else if if_false.target == block {
        false
    } else {
        return;
    };
    let Some((cmp, a, b)) = comparison(func, *flag) else {
        return;
    };
    let (Some(ar), Some(br)) = (point(analysis, points, a), point(analysis, points, b)) else {
        return;
    };
    let cmp = if truth {
        cmp
    } else {
        match cmp {
            Cmp::Lt => Cmp::Ge,
            Cmp::Le => Cmp::Gt,
            Cmp::Gt => Cmp::Le,
            Cmp::Ge => Cmp::Lt,
            Cmp::Eq => Cmp::Ne,
            Cmp::Ne => Cmp::Eq,
        }
    };
    let refined = match cmp {
        Cmp::Lt => (
            ar.meet(Range {
                lo: Range::FULL.lo,
                hi: br.hi.saturating_sub(1),
            }),
            br.meet(Range {
                lo: ar.lo.saturating_add(1),
                hi: Range::FULL.hi,
            }),
        ),
        Cmp::Le => (
            ar.meet(Range {
                lo: Range::FULL.lo,
                hi: br.hi,
            }),
            br.meet(Range {
                lo: ar.lo,
                hi: Range::FULL.hi,
            }),
        ),
        Cmp::Gt => (
            ar.meet(Range {
                lo: br.lo.saturating_add(1),
                hi: Range::FULL.hi,
            }),
            br.meet(Range {
                lo: Range::FULL.lo,
                hi: ar.hi.saturating_sub(1),
            }),
        ),
        Cmp::Ge => (
            ar.meet(Range {
                lo: br.lo,
                hi: Range::FULL.hi,
            }),
            br.meet(Range {
                lo: Range::FULL.lo,
                hi: ar.hi,
            }),
        ),
        Cmp::Eq => (ar.meet(br), br.meet(ar)),
        Cmp::Ne => return,
    };
    if let (Some(a_range), Some(b_range)) = refined {
        points.put(analysis.origins[a.index()], a_range);
        points.put(analysis.origins[b.index()], b_range);
    }
}

fn comparison(func: &Func, mut value: Value) -> Option<(Cmp, Value, Value)> {
    for _ in 0..=func.values.len() {
        value = func.resolve(value)?;
        let ValueDef::Inst(id) = func.values[value.index()].def else {
            return None;
        };
        let inst = &func.insts[id.index()];
        match inst.op {
            Opcode::FixCmp(cmp) => return Some((cmp, inst.args[0], inst.args[1])),
            Opcode::Refine(_) | Opcode::BoolToLisp => value = inst.args[0],
            Opcode::IsNonNil
                if func.values[func.resolve(inst.args[0])?.index()]
                    .ty
                    .is_subset(TypeSet::BOOLEAN) =>
            {
                value = inst.args[0]
            }
            _ => return None,
        }
    }
    None
}

#[cfg(test)]
#[path = "tests/range_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/array_range_test.rs"]
mod array_tests;
