//! Dominator-scoped value and memory reuse with exact semantic views.
//!
//! Threading: pass state and scalar results belong to one compilation. No
//! runtime cache, Lisp object reference, or mutator-local state is retained.

use std::collections::HashMap;

use crate::emacs_core::bytecode::Op;
use crate::emacs_core::jit::opt::{
    ir::*,
    mem::{AliasClass, Effects},
    types::TypeSet,
    verify::VerifyError,
};

use super::cfg::Dominance;

/// Actual transformations only, published as immutable compile metadata.
/// Threading: owned by the compiler until reporting; never updated by Lisp.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct GvnStats {
    pub(crate) pure_reuses: usize,
    pub(crate) load_reuses: usize,
    pub(crate) store_forwards: usize,
}

/// Transform only a verified candidate, preserving every original SSA/result,
/// block, frame, entry-stack and source-state identity. Eliminated operations
/// become same-ID pure Refine views, making the leader an explicit live use.
/// No guard, store, call, allocation or Poll is removed or reordered.
/// Threading: all scratch and candidate state is exclusive to this invocation.
pub(crate) fn run(func: &mut Func) -> Result<GvnStats, VerifyError> {
    func.verify()?;
    let dom = Dominance::new(func)?;
    let mut candidate = func.clone();
    let stats = transform(&mut candidate, &dom)?;
    candidate.verify()?;
    *func = candidate;
    Ok(stats)
}

/// Immutable compiler key. Operand order, exact type and representation remain
/// significant; no numeric reassociation, equality shortcut or widening occurs.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct PureKey {
    op: PureOp,
    args: Vec<Value>,
    ty: TypeSet,
    rep: Rep,
}

/// Compiler opcode identities; static pool entries use their opaque exact bits.
/// No heap dereference, dynamic-prefix assumption or runtime cache is involved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PureOp {
    Constant(ValueBits),
    Typed(Opcode),
}

/// A compiler-only field category. Different SSA bases may still alias, so a
/// field write advances that field's epoch for every address, not just one base.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Field {
    Car,
    Cdr,
}

impl Field {
    fn index(self) -> usize {
        match self {
            Self::Car => 0,
            Self::Cdr => 1,
        }
    }

    fn alias(self) -> AliasClass {
        match self {
            Self::Car => AliasClass::ConsCar,
            Self::Cdr => AliasClass::ConsCdr,
        }
    }
}

/// Mutable availability within one compile-time epoch and dominance scope.
/// Exact result type/rep deliberately excludes forwards needing wider views
/// or new dependent-type narrowing; those remain real loads in this revision.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MemoryKey {
    base: Value,
    field: Field,
    epoch: usize,
    ty: TypeSet,
    rep: Rep,
}

/// A compiler-only origin records what was actually eliminated for census.
#[derive(Clone, Copy)]
enum Origin {
    Load,
    Store,
}

/// Compiler-owned SSA handle, never a Lisp pointer or cached heap observation.
#[derive(Clone, Copy)]
struct Available {
    value: Value,
    origin: Origin,
}

/// Scoped compiler map updates. Undoing a child restores its parent's facts;
/// no availability discovered in one sibling can escape into another sibling.
enum Undo {
    Pure(PureKey, Option<Value>),
    Memory(MemoryKey, Option<Available>),
}

/// Compile-time scope checkpoint. Active epochs roll back; their monotonically
/// increasing allocator does not, preventing coincident sibling epoch IDs.
#[derive(Clone, Copy)]
struct Mark {
    undo: usize,
    epochs: [usize; 2],
}

/// All state belongs to one candidate compilation. Space is linear in visited
/// instructions; map lookup/update is expected constant time. No table cloning
/// occurs at block boundaries and no state is retained after publication.
struct Tables {
    pure: HashMap<PureKey, Value>,
    memory: HashMap<MemoryKey, Available>,
    undo: Vec<Undo>,
    epochs: [usize; 2],
    next_epoch: usize,
}

impl Tables {
    fn new() -> Self {
        Self {
            pure: HashMap::new(),
            memory: HashMap::new(),
            undo: Vec::new(),
            epochs: [0, 1],
            next_epoch: 2,
        }
    }

    fn mark(&self) -> Mark {
        Mark {
            undo: self.undo.len(),
            epochs: self.epochs,
        }
    }

    fn restore(&mut self, mark: Mark) {
        while self.undo.len() > mark.undo {
            match self.undo.pop().expect("scope has updates") {
                Undo::Pure(key, old) => match old {
                    Some(value) => {
                        self.pure.insert(key, value);
                    }
                    None => {
                        self.pure.remove(&key);
                    }
                },
                Undo::Memory(key, old) => match old {
                    Some(value) => {
                        self.memory.insert(key, value);
                    }
                    None => {
                        self.memory.remove(&key);
                    }
                },
            }
        }
        self.epochs = mark.epochs;
    }

    fn pure(&mut self, key: PureKey, value: Value) {
        let old = self.pure.insert(key.clone(), value);
        self.undo.push(Undo::Pure(key, old));
    }

    fn memory(&mut self, key: MemoryKey, value: Available) {
        let old = self.memory.insert(key.clone(), value);
        self.undo.push(Undo::Memory(key, old));
    }

    fn kill(&mut self, field: Field) {
        // At most two epoch IDs are allocated per visited block/instruction.
        // A representable allocated Func cannot approach usize::MAX updates.
        self.epochs[field.index()] = self.next_epoch;
        self.next_epoch += 1;
    }

    fn kill_all(&mut self) {
        self.kill(Field::Car);
        self.kill(Field::Cdr);
    }
}

/// Iterative traversal event; avoids recursion proportional to CFG depth.
enum Visit {
    Enter(Block),
    Exit(Mark),
}

fn transform(func: &mut Func, dom: &Dominance) -> Result<GvnStats, VerifyError> {
    let mut children = vec![Vec::new(); func.blocks.len()];
    let mut barrier = vec![false; func.blocks.len()];
    for &block in dom.reverse_postorder() {
        if let Some(parent) = dom.immediate_dominator(block) {
            children[parent.index()].push(block);
        }
        let mut preds = func.blocks[block.index()].preds.clone();
        preds.retain(|&pred| dom.is_reachable(pred));
        preds.sort_unstable();
        preds.dedup();
        // A store in one arm need not dominate its join. Reset there even on a
        // clean diamond, and at every backedge/declared loop entry, rather than
        // let dominator rollback expose a stale pre-branch/preheader load.
        barrier[block.index()] = preds.len() >= 2
            || func.blocks[block.index()].loop_header.is_some()
            || preds.iter().any(|&pred| dom.dominates(block, pred));
    }
    let canonical = alias_numbers(func)?;
    let mut numbers = canonical.clone();
    let mut tables = Tables::new();
    let mut stats = GvnStats::default();
    let mut pending = vec![Visit::Enter(func.entry)];
    while let Some(visit) = pending.pop() {
        match visit {
            Visit::Exit(mark) => tables.restore(mark),
            Visit::Enter(block) => {
                let mark = tables.mark();
                if barrier[block.index()] {
                    tables.kill_all();
                }
                for position in 0..func.blocks[block.index()].insts.len() {
                    let inst = func.blocks[block.index()].insts[position];
                    transform_inst(
                        func,
                        inst,
                        &canonical,
                        &mut numbers,
                        &mut tables,
                        &mut stats,
                    );
                }
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
    Ok(stats)
}

/// Resolve each initial alias chain once. Only validated SSA handles are read.
fn alias_numbers(func: &Func) -> Result<Vec<Value>, VerifyError> {
    let mut numbers = vec![None; func.values.len()];
    let mut path = Vec::new();
    for index in 0..func.values.len() {
        let mut value = Value(index as u32);
        while numbers[value.index()].is_none() {
            path.push(value);
            match func.values[value.index()].def {
                ValueDef::Alias(next) => value = next,
                _ => {
                    numbers[value.index()] = Some(value);
                    break;
                }
            }
        }
        let leader = numbers[value.index()].ok_or(VerifyError::InvalidValue(value))?;
        for value in path.drain(..) {
            numbers[value.index()] = Some(leader);
        }
    }
    numbers
        .into_iter()
        .enumerate()
        .map(|(index, value)| value.ok_or(VerifyError::InvalidValue(Value(index as u32))))
        .collect()
}

fn number(numbers: &[Value], mut value: Value) -> Value {
    while numbers[value.index()] != value {
        value = numbers[value.index()];
    }
    value
}

fn transform_inst(
    func: &mut Func,
    id: Inst,
    canonical: &[Value],
    numbers: &mut [Value],
    tables: &mut Tables,
    stats: &mut GvnStats,
) {
    let mut inst = func.insts[id.index()].clone();
    // An actual Tagged LIST proof makes these four exact bytecodes total:
    // cons reads its field, nil returns nil. Unknown opaque operations remain
    // safepoints/barriers. Retain the original identity, PC/frame and all
    // preceding guards; the selected native LIST adapter branches before any
    // dereference and neither calls Lisp nor polls on this successful path.
    if let Some(field) = lift_list_read(func, &inst, canonical) {
        inst.op = match field {
            Field::Car => Opcode::LoadCar,
            Field::Cdr => Opcode::LoadCdr,
        };
        inst.eff = Effects::READ_HEAP;
        func.insts[id.index()] = inst.clone();
    }
    if let Some(field) = precise_store(&inst) {
        tables.kill(field);
        let stored = inst.args[1];
        let resolved = canonical[stored.index()];
        let data = &func.values[resolved.index()];
        let key = MemoryKey {
            base: number(numbers, inst.args[0]),
            field,
            epoch: tables.epochs[field.index()],
            ty: data.ty,
            rep: data.rep,
        };
        tables.memory(
            key,
            Available {
                value: stored,
                origin: Origin::Store,
            },
        );
        return;
    }
    kill_effects(tables, &inst);
    let Some(result) = inst.result else {
        return;
    };
    if let Some(field) = eligible_load(func, &inst, canonical) {
        let data = &func.values[result.index()];
        let key = MemoryKey {
            base: number(numbers, inst.args[0]),
            field,
            epoch: tables.epochs[field.index()],
            ty: data.ty,
            rep: data.rep,
        };
        if let Some(available) = tables.memory.get(&key).copied() {
            replace(func, id, available.value, numbers);
            match available.origin {
                Origin::Load => stats.load_reuses += 1,
                Origin::Store => stats.store_forwards += 1,
            }
        } else {
            tables.memory(
                key,
                Available {
                    value: result,
                    origin: Origin::Load,
                },
            );
        }
        return;
    }
    // CheckType remains executable. Its successful output and pure Refine are
    // exact physical views, useful only for keying later operations/addresses.
    // Their result declarations, guards, frame observations and PCs are kept.
    if matches!(inst.op, Opcode::CheckType(_) | Opcode::Refine(_)) {
        let input = inst.args[0];
        let actual = func.values[canonical[input.index()].index()].rep;
        let output = func.values[result.index()].rep;
        if actual == output || actual.is_tagged() && output.is_tagged() {
            numbers[result.index()] = number(numbers, input);
        }
        return;
    }
    if let Some(key) = pure_key(func, &inst, canonical, numbers) {
        if let Some(&leader) = tables.pure.get(&key) {
            replace(func, id, leader, numbers);
            stats.pure_reuses += 1;
        } else {
            tables.pure(key, result);
        }
    }
}

fn replace(func: &mut Func, id: Inst, leader: Value, numbers: &mut [Value]) {
    let inst = &mut func.insts[id.index()];
    let result = inst.result.expect("available operation has a result");
    let ty = func.values[result.index()].ty;
    inst.op = Opcode::Refine(ty);
    inst.args = vec![leader];
    inst.eff = Effects::PURE;
    inst.mem = AliasClass::None;
    numbers[result.index()] = number(numbers, leader);
}

fn precise_store(inst: &InstData) -> Option<Field> {
    let field = match inst.op {
        Opcode::StoreCar | Opcode::Opaque(Op::Setcar) => Field::Car,
        Opcode::StoreCdr | Opcode::Opaque(Op::Setcdr) => Field::Cdr,
        _ => return None,
    };
    let allowed = Effects::WRITE_HEAP.with(Effects::MAY_DEOPT);
    (inst.args.len() == 2
        && inst.mem == field.alias()
        && (inst.eff == Effects::WRITE_HEAP || inst.eff == allowed))
        .then_some(field)
}

/// This selected pass exposes only guarded exact list field bytecodes. A
/// generic Opaque/OpaqueBool still clears availability even with small hints.
/// No unchecked TOP operand or extra GC/reentry/effect declaration qualifies.
fn lift_list_read(func: &Func, inst: &InstData, canonical: &[Value]) -> Option<Field> {
    let field = match inst.op {
        Opcode::Opaque(Op::Car | Op::CarSafe) => Field::Car,
        Opcode::Opaque(Op::Cdr | Op::CdrSafe) => Field::Cdr,
        _ => return None,
    };
    if inst.args.len() != 1
        || inst.eff != Effects::READ_HEAP.with(Effects::MAY_DEOPT)
        || inst.mem != field.alias()
    {
        return None;
    }
    let result = inst.result?;
    if func.values[result.index()].rep != Rep::Tagged {
        return None;
    }
    let input = &func.values[canonical[inst.args[0].index()].index()];
    (!input.ty.is_bottom() && input.ty.is_subset(TypeSet::LIST) && input.rep == Rep::Tagged)
        .then_some(field)
}

fn eligible_load(func: &Func, inst: &InstData, canonical: &[Value]) -> Option<Field> {
    let field = match inst.op {
        Opcode::LoadCar => Field::Car,
        Opcode::LoadCdr => Field::Cdr,
        _ => return None,
    };
    let base = canonical[inst.args[0].index()];
    let data = &func.values[base.index()];
    let allowed = Effects::READ_HEAP.with(Effects::MAY_DEOPT);
    (!data.ty.is_bottom()
        && data.ty.is_subset(TypeSet::LIST)
        && data.rep == Rep::Tagged
        && inst.mem == field.alias()
        && (inst.eff == Effects::READ_HEAP || inst.eff == allowed))
        .then_some(field)
}

fn kill_effects(tables: &mut Tables, inst: &InstData) {
    // The conservative allocation boundary also covers unmodeled boxing slow
    // paths. It is not a claim that ALLOCATES always collects. Opaque/Builtin
    // operations not modeled here start a new epoch regardless of tiny hints.
    if matches!(
        inst.op,
        Opcode::Poll
            | Opcode::Call { .. }
            | Opcode::Opaque(_)
            | Opcode::OpaqueBool(_)
            | Opcode::Builtin(_)
            | Opcode::AllocCons
            | Opcode::AllocFloat
            | Opcode::StoreCar
            | Opcode::StoreCdr
            | Opcode::StoreVecElem
            | Opcode::StoreSymValue(_)
    ) || inst.eff.intersects(
        Effects::MAY_GC
            .with(Effects::MAY_REENTER)
            .with(Effects::MAY_SIGNAL)
            .with(Effects::ALLOCATES),
    ) {
        tables.kill_all();
    } else if inst.eff.contains(Effects::WRITE_HEAP) {
        match inst.mem {
            AliasClass::ConsCar => tables.kill(Field::Car),
            AliasClass::ConsCdr => tables.kill(Field::Cdr),
            _ => tables.kill_all(),
        }
    }
}

fn pure_key(
    func: &Func,
    inst: &InstData,
    canonical: &[Value],
    numbers: &[Value],
) -> Option<PureKey> {
    if inst.eff != Effects::PURE || !matches!(inst.mem, AliasClass::None | AliasClass::Immutable) {
        return None;
    }
    let result = inst.result?;
    let data = &func.values[result.index()];
    if data.ty.is_bottom()
        || inst
            .args
            .iter()
            .any(|&value| func.values[canonical[value.index()].index()].ty.is_bottom())
    {
        return None;
    }
    let op = match &inst.op {
        Opcode::Const(index) if *index as usize >= func.dynamic_prefix => {
            PureOp::Constant(func.consts[*index as usize])
        }
        Opcode::BoolConst(_)
        | Opcode::BoolToLisp
        | Opcode::TagFix
        | Opcode::UntagFix
        | Opcode::IsNonNil
        | Opcode::FixCmp(_)
        | Opcode::FixAdd { checked: false }
        | Opcode::FixSub { checked: false }
        | Opcode::FixMul { checked: false } => PureOp::Typed(inst.op.clone()),
        Opcode::TypeTest(ty)
            if !ty.is_bottom()
                && ty.is_subset(
                    TypeSet::FIXNUM
                        .join(TypeSet::BOOLEAN)
                        .join(TypeSet::CONS)
                        .join(TypeSet::STRING),
                ) =>
        {
            PureOp::Typed(inst.op.clone())
        }
        Opcode::Select
            if matches!(
                data.rep,
                Rep::Tagged | Rep::TaggedFix | Rep::RawInt | Rep::Bool
            ) =>
        {
            PureOp::Typed(inst.op.clone())
        }
        // EnvConst/patched-prefix, heap Eq/SWP observations, all checked
        // arithmetic, Float payload/boxing/allocation identities, derived
        // pointers and every unlisted operation remain outside pure numbering.
        _ => return None,
    };
    Some(PureKey {
        op,
        args: inst
            .args
            .iter()
            .map(|&value| number(numbers, value))
            .collect(),
        ty: data.ty,
        rep: data.rep,
    })
}

#[cfg(test)]
#[path = "tests/gvn_test.rs"]
mod tests;
