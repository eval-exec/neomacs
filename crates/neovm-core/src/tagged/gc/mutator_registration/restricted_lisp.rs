//! Test-only restricted Lisp overlap. Production has no call into this module,
//! and `#[cfg(test)]` keeps it out of every `NEOVM_THREADS` mode.
//!
//! Owners allocate conses through `RunSlice::cons_into` inside `with_slice`,
//! and tables through the rooted `RunAccess` make/copy operations.
//! Hot operands live in branded `SliceRoot` slots. `SharedRoot` is cold setup.
//! A program that uses anything outside the closed subset is rejected before
//! any owner is activated.
#![cfg(not(feature = "gc-memory-telemetry"))]

use super::a8_probes;
use super::symbol_functions::FunctionOp;
use super::symbol_functions::retry::{self as contention, Deadline, Retry, SPIN_WAIT, retry_owned};
use super::symbol_globals::GlobalOp;
use super::symbol_intern::InternOp;
use super::{
    AllStopped, HeapIdentity, Mutator, MutatorPermit, OwnerRootSlot, ParallelHeapEra,
    RegistrationError, Registry, RendezvousError, RunAccess, RunSlice, SealedTableAccess,
    SliceRoot, TableMut, TableRef,
};

mod functions;
mod globals;
mod intern;
use crate::emacs_core::builtins::maybe_resize_hash_table_for_insert;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::{HashProbe, HashTableEntry, HashTableTest};
use crate::tagged::transport::SharedRoot;
use crate::tagged::value::{Fixnum, TaggedValue};
use rustc_hash::FxHashSet;
use std::collections::HashSet;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread;
use std::time::{Duration, Instant};

const STEP_CAP: u32 = 10_000;
const CHANNEL_WAIT: Duration = Duration::from_secs(90);

/// The extent body committed publication; only a successful wrapper return
/// permits heap access. Keep that cleanup obligation across unwinding.
enum NativeState {
    Running,
    Published,
}

fn retry_exec<T>(op: impl FnMut() -> Result<T, ExecError>) -> Result<T, ExecError> {
    contention::retry(Deadline::contention(), op, |error| match error {
        ExecError::Registration(error) => contention::registration(error),
        ExecError::Intern(error) => contention::initial_intern(error),
        ExecError::Rendezvous(_)
        | ExecError::ForeignCons
        | ExecError::Type
        | ExecError::Overflow
        | ExecError::Backedge
        | ExecError::Slot
        | ExecError::UserTestMutation
        | ExecError::EpochRetry
        | ExecError::VoidVariable
        | ExecError::Function(_) => Retry::Refuse,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Op {
    /// C3: an index into a session's admitted, immutable symbol-handle table.
    Global(GlobalOp),
    Function(FunctionOp),
    Intern(InternOp),
    ConstNil {
        dst: u16,
    },
    ConstT {
        dst: u16,
    },
    ConstFixnum {
        dst: u16,
        value: i64,
    },
    Move {
        dst: u16,
        src: u16,
    },
    Add {
        dst: u16,
        a: u16,
        b: u16,
    },
    Sub {
        dst: u16,
        a: u16,
        b: u16,
    },
    Lt {
        dst: u16,
        a: u16,
        b: u16,
    },
    Eq {
        dst: u16,
        a: u16,
        b: u16,
    },
    BranchIfNil {
        cond: u16,
        target: u16,
    },
    Jump {
        target: u16,
    },
    Cons {
        dst: u16,
        car: u16,
        cdr: u16,
    },
    Car {
        dst: u16,
        cell: u16,
    },
    Cdr {
        dst: u16,
        cell: u16,
    },
    SetCar {
        cell: u16,
        value: u16,
    },
    SetCdr {
        cell: u16,
        value: u16,
    },
    /// B3 sealed hash-table ops. The table operand is an owner slot planted
    /// with the table word at admission time; results go back into owner
    /// slots. Like the C3 global ops, their leases execute between slice
    /// regions ([`Flow::Lease`]).
    HtGet {
        table: u16,
        key: u16,
        found: u16,
        value: u16,
    },
    HtPut {
        table: u16,
        key: u16,
        value: u16,
    },
    HtRem {
        table: u16,
        key: u16,
        found: u16,
    },
    HtClear {
        table: u16,
    },
    /// B7 owner allocation; the initialized shell is rooted before return.
    HtMake {
        dst: u16,
        test: HashTableTest,
        size: usize,
    },
    /// Snapshot the source under a reader, then root an independent shell.
    HtCopy {
        table: u16,
        dst: u16,
    },
    HtCount {
        table: u16,
        dst: u16,
    },
    Safepoint,
}

/// Closed session admission. Each variant carries exactly the symbol table
/// admitted by that session; intern cannot be enabled independently of its
/// global handles, and function handles share their session's global table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TapeCapabilities {
    Core,
    Globals { symbols: usize },
    Functions { symbols: usize },
    InitialIntern { globals: usize },
}

impl TapeCapabilities {
    fn global_handles(self) -> Option<usize> {
        match self {
            Self::Core => None,
            Self::Globals { symbols } | Self::Functions { symbols } => Some(symbols),
            Self::InitialIntern { globals } => Some(globals),
        }
    }

    fn function_handles(self) -> Option<usize> {
        match self {
            Self::Functions { symbols } => Some(symbols),
            Self::Core | Self::Globals { .. } | Self::InitialIntern { .. } => None,
        }
    }
}

/// Surface syntax the gate sees. Excluded variants never lower into [`Op`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Insn {
    Allowed(Op),
    Global(GlobalOp),
    Function(FunctionOp),
    Intern(InternOp),
    SymbolValue,
    Obarray,
    Buffer,
    HashTable,
    Specbind,
    Jit,
    WorldManager,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Abs {
    Nil,
    T,
    Fix(i64),
    Cons,
    /// A bare symbol planted by hash-table setup: an `eq` key shape.
    Sym,
    /// A hash table planted by [`HtSetup`]: only the `Ht*` table operand
    /// accepts it, and it is the one shape a key operand refuses.
    Table,
    Top,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Reject {
    Excluded(&'static str),
    RejectedType,
    Overflow,
    Backedge,
    Slot,
    Unbounded,
}

#[derive(Debug, PartialEq, Eq)]
enum ExecError {
    Registration(RegistrationError),
    Rendezvous(RendezvousError),
    ForeignCons,
    Type,
    Overflow,
    Backedge,
    Slot,
    /// A sealed mutating op ran while this table's §3.1 nesting count was
    /// nonzero: the user test (or hash) callback tried to edit the table it
    /// was running for. GNU's "hash table test modifies table" refusal.
    UserTestMutation,
    /// A user-test lookup could not settle within [`HT_RETRY_ROUNDS`]
    /// revalidation rounds: the epoch moved under every scan.
    EpochRetry,
    VoidVariable,
    Function(super::FunctionError),
    Intern(super::InitialInternError),
}

/// Why exec handed control back to the driver. A safepoint pauses for the
/// collection protocol; a lease op needs `&mut RunAccess`, which the open
/// slice region is borrowing, so the op runs only after the region closes
/// and the driver then re-enters the slice at the same pc.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flow {
    Done,
    Safepoint,
    Lease,
}

impl From<RegistrationError> for ExecError {
    fn from(error: RegistrationError) -> Self {
        Self::Registration(error)
    }
}

impl From<RendezvousError> for ExecError {
    fn from(error: RendezvousError) -> Self {
        Self::Rendezvous(error)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Go {
    Forced,
    Threshold,
}

#[derive(Debug)]
struct Snap {
    index: usize,
    cons_count: usize,
    safepoints: usize,
    bytes: usize,
    slots: Vec<usize>,
    conses: FxHashSet<usize>,
}

#[derive(Debug)]
struct EndState {
    index: usize,
    lines: Vec<String>,
    slots: Vec<usize>,
    conses: FxHashSet<usize>,
    cons_count: usize,
    safepoints: usize,
}

struct Machine {
    pc: usize,
    backedges: u32,
    cons_count: usize,
    safepoints: usize,
    owned: FxHashSet<usize>,
    entered: bool,
    allocated: bool,
}

impl Machine {
    fn new() -> Self {
        Self {
            pc: 0,
            backedges: 0,
            cons_count: 0,
            safepoints: 0,
            owned: FxHashSet::default(),
            entered: false,
            allocated: false,
        }
    }

    fn restart(&mut self) {
        self.pc = 0;
        self.backedges = 0;
    }
}

/// One cacheline per owner: adjacent owners' progress counters must not
/// share a line, or the witness itself becomes the measured bottleneck.
#[repr(align(64))]
struct PaddedUsize(AtomicUsize);

struct Overlap {
    abort: AtomicBool,
    entered: AtomicUsize,
    allocated: AtomicUsize,
    seen_enter: Vec<AtomicUsize>,
    seen_alloc: Vec<AtomicUsize>,
    progress: Vec<PaddedUsize>,
    n: usize,
    watch_alloc: bool,
}

impl Overlap {
    fn new(n: usize, watch_alloc: bool) -> Self {
        let mut seen_enter = Vec::with_capacity(n);
        let mut seen_alloc = Vec::with_capacity(n);
        let mut progress = Vec::with_capacity(n);
        for _ in 0..n {
            seen_enter.push(AtomicUsize::new(0));
            seen_alloc.push(AtomicUsize::new(0));
            progress.push(PaddedUsize(AtomicUsize::new(0)));
        }
        Self {
            abort: AtomicBool::new(false),
            entered: AtomicUsize::new(0),
            allocated: AtomicUsize::new(0),
            seen_enter,
            seen_alloc,
            progress,
            n,
            watch_alloc,
        }
    }

    fn arrive_enter(&self, index: usize) {
        arrive(&self.entered, &self.seen_enter[index], self.n, &self.abort);
    }

    fn arrive_alloc(&self, index: usize) {
        arrive(
            &self.allocated,
            &self.seen_alloc[index],
            self.n,
            &self.abort,
        );
    }
}

fn arrive(flag: &AtomicUsize, seen: &AtomicUsize, n: usize, abort: &AtomicBool) {
    flag.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        if abort.load(Ordering::SeqCst) {
            panic!("overlap aborted");
        }
        let observed = flag.load(Ordering::SeqCst);
        if observed == n {
            // Every owner has arrived and is still inside this slice.
            seen.store(n, Ordering::SeqCst);
            return;
        }
        if Instant::now() >= deadline {
            abort.store(true, Ordering::SeqCst);
            panic!("overlap witness timed out at {observed}/{n}");
        }
        thread::yield_now();
    }
}

struct PanicFlag<'a> {
    registry: &'a Registry,
    abort: &'a AtomicBool,
}

impl Drop for PanicFlag<'_> {
    fn drop(&mut self) {
        if thread::panicking() {
            self.abort.store(true, Ordering::SeqCst);
            self.registry.poison();
        }
    }
}

fn recv<T>(rx: &Receiver<T>) -> T {
    rx.recv_timeout(CHANNEL_WAIT)
        .unwrap_or_else(|error| panic!("owner channel closed: {error}"))
}

fn retry_rv<T>(op: impl FnMut() -> Result<T, RendezvousError>) -> T {
    contention::retry(Deadline::contention(), op, contention::rendezvous)
        .unwrap_or_else(|error| panic!("rendezvous: {error:?}"))
}

fn retry_reg<T>(op: impl FnMut() -> Result<T, RegistrationError>) -> T {
    contention::retry(Deadline::contention(), op, contention::registration)
        .unwrap_or_else(|error| panic!("registration: {error:?}"))
}

fn must<T>(result: Result<T, ExecError>) -> T {
    result.unwrap_or_else(|error| panic!("restricted lisp: {error:?}"))
}

fn lower(insns: &[Insn]) -> Result<Vec<Op>, Reject> {
    let mut ops = Vec::with_capacity(insns.len());
    for insn in insns {
        ops.push(match *insn {
            Insn::Allowed(op) => op,
            Insn::Global(op) => Op::Global(op),
            Insn::Function(op) => Op::Function(op),
            Insn::Intern(op) => Op::Intern(op),
            Insn::SymbolValue => return Err(Reject::Excluded("symbol")),
            Insn::Obarray => return Err(Reject::Excluded("obarray")),
            Insn::Buffer => return Err(Reject::Excluded("buffer")),
            Insn::HashTable => return Err(Reject::Excluded("hash-table")),
            Insn::Specbind => return Err(Reject::Excluded("specbind")),
            Insn::Jit => return Err(Reject::Excluded("jit")),
            Insn::WorldManager => return Err(Reject::Excluded("world")),
        });
    }
    Ok(ops)
}

/// Count a backedge when the target is at or behind the branching instruction.
/// The `bound`-th backedge is still allowed.
fn edge(backedges: &mut u32, bound: u32, from: u16, target: u16) -> bool {
    if target <= from {
        *backedges = backedges.saturating_add(1);
        if *backedges > bound {
            return false;
        }
    }
    true
}

fn slot_index(slot: u16, len: usize) -> Result<usize, Reject> {
    let slot = usize::from(slot);
    if slot >= len {
        Err(Reject::Slot)
    } else {
        Ok(slot)
    }
}

fn target_index(target: u16, len: usize) -> Result<usize, Reject> {
    let target = usize::from(target);
    if target > len {
        Err(Reject::Slot)
    } else {
        Ok(target)
    }
}

fn prevalidate(ops: &[Op], nslots: usize, bound: u32, step_cap: u32) -> Result<(), Reject> {
    prevalidate_with(ops, nslots, bound, step_cap, &[])
}

/// Concrete execution of the constant program. Joining the head's nil and cons
/// states to Top would reject the list's own `BranchIfNil`, so the validator
/// follows the branch it can see and counts backedges.
fn prevalidate_with(
    ops: &[Op],
    nslots: usize,
    bound: u32,
    step_cap: u32,
    initial: &[(u16, Abs)],
) -> Result<(), Reject> {
    prevalidate_inner(
        ops,
        nslots,
        bound,
        step_cap,
        initial,
        TapeCapabilities::Core,
    )
}

fn prevalidate_inner(
    ops: &[Op],
    nslots: usize,
    bound: u32,
    step_cap: u32,
    initial: &[(u16, Abs)],
    capabilities: TapeCapabilities,
) -> Result<(), Reject> {
    // Scan the entire tape, including unreachable code. A global session
    // checks all participating tapes before any participant is registered.
    globals::check_tape(ops, nslots, capabilities.global_handles())?;
    functions::check_tape(ops, nslots, capabilities.function_handles())?;
    intern::check_tape(ops, nslots, capabilities)?;
    let mut slots = vec![Abs::Nil; nslots];
    for &(index, kind) in initial {
        slots[slot_index(index, nslots)?] = kind;
    }
    let mut call_selected = false;
    let mut pc = 0usize;
    let mut backedges = 0u32;
    let mut steps = 0u32;
    while pc < ops.len() {
        steps += 1;
        if steps > step_cap {
            return Err(Reject::Unbounded);
        }
        let from = u16::try_from(pc).map_err(|_| Reject::Slot)?;
        match ops[pc] {
            Op::Intern(op) => {
                intern::abstract_step(op, &mut slots)?;
                pc += 1;
            }
            Op::Function(op) => {
                functions::abstract_step(op, &mut slots, &mut call_selected)?;
                pc += 1;
            }
            Op::Global(op) => {
                globals::abstract_step(op, &mut slots)?;
                pc += 1;
            }
            Op::ConstNil { dst } => {
                slots[slot_index(dst, nslots)?] = Abs::Nil;
                pc += 1;
            }
            Op::ConstT { dst } => {
                slots[slot_index(dst, nslots)?] = Abs::T;
                pc += 1;
            }
            Op::ConstFixnum { dst, value } => {
                if Fixnum::try_from(value).is_err() {
                    return Err(Reject::Overflow);
                }
                slots[slot_index(dst, nslots)?] = Abs::Fix(value);
                pc += 1;
            }
            Op::Move { dst, src } => {
                let value = slots[slot_index(src, nslots)?];
                slots[slot_index(dst, nslots)?] = value;
                pc += 1;
            }
            Op::Add { dst, a, b } => {
                let value = fix_binop(slots.as_slice(), nslots, a, b, true)?;
                let index = slot_index(dst, nslots)?;
                slots[index] = value;
                pc += 1;
            }
            Op::Sub { dst, a, b } => {
                let value = fix_binop(slots.as_slice(), nslots, a, b, false)?;
                let index = slot_index(dst, nslots)?;
                slots[index] = value;
                pc += 1;
            }
            Op::Lt { dst, a, b } => {
                let (x, y) = fix_pair(slots.as_slice(), nslots, a, b)?;
                let index = slot_index(dst, nslots)?;
                slots[index] = if x < y { Abs::T } else { Abs::Nil };
                pc += 1;
            }
            Op::Eq { dst, a, b } => {
                let answer = match capabilities {
                    TapeCapabilities::InitialIntern { .. } => {
                        // Intern compares rooted symbol identities or NIL;
                        // older sessions retain their fixnum gate.
                        slot_index(a, nslots)?;
                        slot_index(b, nslots)?;
                        Abs::Top
                    }
                    TapeCapabilities::Core
                    | TapeCapabilities::Globals { .. }
                    | TapeCapabilities::Functions { .. } => {
                        let (x, y) = fix_pair(slots.as_slice(), nslots, a, b)?;
                        if x == y { Abs::T } else { Abs::Nil }
                    }
                };
                slots[slot_index(dst, nslots)?] = answer;
                pc += 1;
            }
            Op::BranchIfNil { cond, target } => {
                let dest = target_index(target, ops.len())?;
                match slots[slot_index(cond, nslots)?] {
                    Abs::Nil => {
                        if !edge(&mut backedges, bound, from, target) {
                            return Err(Reject::Backedge);
                        }
                        pc = dest;
                    }
                    Abs::T | Abs::Fix(_) | Abs::Cons | Abs::Sym | Abs::Table => pc += 1,
                    Abs::Top => return Err(Reject::RejectedType),
                }
            }
            Op::Jump { target } => {
                let dest = target_index(target, ops.len())?;
                if !edge(&mut backedges, bound, from, target) {
                    return Err(Reject::Backedge);
                }
                pc = dest;
            }
            Op::Cons { dst, car, cdr } => {
                let _ = slot_index(car, nslots)?;
                let _ = slot_index(cdr, nslots)?;
                slots[slot_index(dst, nslots)?] = Abs::Cons;
                pc += 1;
            }
            Op::Car { dst, cell } | Op::Cdr { dst, cell } => {
                let kind = slots[slot_index(cell, nslots)?];
                if kind != Abs::Cons
                    && !(capabilities.global_handles().is_some() && kind == Abs::Top)
                {
                    return Err(Reject::RejectedType);
                }
                slots[slot_index(dst, nslots)?] = Abs::Top;
                pc += 1;
            }
            Op::SetCar { cell, value } | Op::SetCdr { cell, value } => {
                if slots[slot_index(cell, nslots)?] != Abs::Cons {
                    return Err(Reject::RejectedType);
                }
                let _ = slot_index(value, nslots)?;
                pc += 1;
            }
            Op::HtGet {
                table,
                key,
                found,
                value,
            } => {
                table_must(slots.as_slice(), nslots, table)?;
                key_may(slots.as_slice(), nslots, key)?;
                slots[slot_index(found, nslots)?] = Abs::Top;
                slots[slot_index(value, nslots)?] = Abs::Top;
                pc += 1;
            }
            Op::HtPut { table, key, value } => {
                table_must(slots.as_slice(), nslots, table)?;
                key_may(slots.as_slice(), nslots, key)?;
                let _ = slot_index(value, nslots)?;
                pc += 1;
            }
            Op::HtRem { table, key, found } => {
                table_must(slots.as_slice(), nslots, table)?;
                key_may(slots.as_slice(), nslots, key)?;
                slots[slot_index(found, nslots)?] = Abs::Top;
                pc += 1;
            }
            Op::HtClear { table } => {
                table_must(slots.as_slice(), nslots, table)?;
                pc += 1;
            }
            Op::HtMake { dst, size, .. } => {
                if size > STEP_CAP as usize {
                    return Err(Reject::Unbounded);
                }
                slots[slot_index(dst, nslots)?] = Abs::Table;
                pc += 1;
            }
            Op::HtCopy { table, dst } => {
                table_must(slots.as_slice(), nslots, table)?;
                slots[slot_index(dst, nslots)?] = Abs::Table;
                pc += 1;
            }
            Op::HtCount { table, dst } => {
                table_must(slots.as_slice(), nslots, table)?;
                slots[slot_index(dst, nslots)?] = Abs::Top;
                pc += 1;
            }
            Op::Safepoint => pc += 1,
        }
    }
    Ok(())
}

/// The `Ht*` table operand must be a planted table word (never Top: a
/// program that might run a table op on a non-table is rejected before any
/// owner is activated, like every other type fact here).
fn table_must(slots: &[Abs], nslots: usize, table: u16) -> Result<(), Reject> {
    match slots[slot_index(table, nslots)?] {
        Abs::Table => Ok(()),
        _ => Err(Reject::RejectedType),
    }
}

/// B3 key shapes: fixnum, bare symbol, or a cons (an `equal` key). Nil and t
/// would be honest eq keys, but the closed program set never needs them, and
/// refusing keeps the fact lattice from joining them into Top.
fn key_may(slots: &[Abs], nslots: usize, key: u16) -> Result<(), Reject> {
    match slots[slot_index(key, nslots)?] {
        Abs::Fix(_) | Abs::Sym | Abs::Cons | Abs::Top => Ok(()),
        _ => Err(Reject::RejectedType),
    }
}

fn fix_pair(slots: &[Abs], nslots: usize, a: u16, b: u16) -> Result<(i64, i64), Reject> {
    match (slots[slot_index(a, nslots)?], slots[slot_index(b, nslots)?]) {
        (Abs::Fix(x), Abs::Fix(y)) => Ok((x, y)),
        _ => Err(Reject::RejectedType),
    }
}

fn fix_binop(slots: &[Abs], nslots: usize, a: u16, b: u16, add: bool) -> Result<Abs, Reject> {
    let (x, y) = fix_pair(slots, nslots, a, b)?;
    let value = if add {
        x.checked_add(y)
    } else {
        x.checked_sub(y)
    };
    let Some(value) = value else {
        return Err(Reject::Overflow);
    };
    if Fixnum::try_from(value).is_err() {
        return Err(Reject::Overflow);
    }
    Ok(Abs::Fix(value))
}

fn try_admit(
    insns: &[Insn],
    nslots: usize,
    bound: u32,
    initial: &[(u16, Abs)],
    started: &AtomicUsize,
) -> Result<Vec<Op>, Reject> {
    let ops = lower(insns)?;
    prevalidate_with(&ops, nslots, bound, STEP_CAP, initial)?;
    // Activation is this flag. It moves only after the gate accepts the program.
    started.fetch_add(1, Ordering::Relaxed);
    Ok(ops)
}

fn list_ops(base: i64) -> Vec<Op> {
    tp_list_ops(base, 8, 1)
}

/// `list_ops` with a parameterized loop limit: the A8 throughput rows need
/// longer lists than the oracle fixtures' eight cells. `spacing` unrolls the
/// loop body so a Safepoint ends the slice only every `spacing` conses; the
/// op sequence per cons is unchanged, so spacing=1 emits the classic
/// one-safepoint-per-cell program exactly.
fn tp_list_ops(base: i64, limit: i64, spacing: usize) -> Vec<Op> {
    // 0 nil, 1 base, 2 k, 3 limit, 4 head, 5 tail, 6 cell, 7 one, 8 cond, 9 car, 10 scratch
    let mut ops = vec![
        Op::ConstNil { dst: 0 },
        Op::ConstFixnum {
            dst: 1,
            value: base,
        },
        Op::ConstFixnum { dst: 2, value: 0 },
        Op::ConstFixnum {
            dst: 3,
            value: limit,
        },
        Op::ConstNil { dst: 4 },
        Op::ConstNil { dst: 5 },
        Op::ConstFixnum { dst: 7, value: 1 },
    ];
    let loop_at = ops.len() as u16;
    ops.push(Op::Lt { dst: 8, a: 2, b: 3 });
    let branch_at = ops.len();
    ops.push(Op::BranchIfNil { cond: 8, target: 0 });
    // One cons-build block: car fixnum, fresh nil cdr, cons, append to the
    // tail (first cell initializes head+tail instead), advance k. Emitted
    // `spacing` times per outer iteration so the Safepoint below ends a slice
    // only every `spacing` conses.
    let mut head_branches = Vec::with_capacity(spacing);
    let mut jump_slots = Vec::with_capacity(spacing);
    let mut copy_starts = Vec::with_capacity(spacing);
    let mut init_at = 0u16;
    for copy in 0..spacing {
        copy_starts.push(ops.len() as u16);
        ops.push(Op::Add { dst: 9, a: 1, b: 2 });
        ops.push(Op::ConstNil { dst: 10 });
        ops.push(Op::Cons {
            dst: 6,
            car: 9,
            cdr: 10,
        });
        head_branches.push(ops.len());
        ops.push(Op::BranchIfNil { cond: 4, target: 0 });
        ops.push(Op::SetCdr { cell: 5, value: 6 });
        ops.push(Op::Move { dst: 5, src: 6 });
        jump_slots.push(ops.len());
        ops.push(Op::Jump { target: 0 });
        if copy == 0 {
            // The first cons of the whole list initializes head and tail,
            // then falls through into block 1 so every outer iteration still
            // builds exactly `spacing` conses.
            init_at = ops.len() as u16;
            ops.push(Op::Move { dst: 4, src: 6 });
            ops.push(Op::Move { dst: 5, src: 6 });
        }
    }
    let join = ops.len() as u16;
    // k advances once per cons, so an unrolled iteration advances it
    // `spacing` times before the loop test sees it again.
    for _ in 0..spacing {
        ops.push(Op::Add { dst: 2, a: 2, b: 7 });
    }
    ops.push(Op::Safepoint);
    ops.push(Op::Jump { target: loop_at });
    let done = ops.len() as u16;
    ops.push(Op::ConstFixnum { dst: 9, value: 42 });
    ops.push(Op::ConstNil { dst: 10 });
    ops.push(Op::Cons {
        dst: 6,
        car: 9,
        cdr: 10,
    });
    ops.push(Op::SetCar { cell: 4, value: 6 });
    ops.push(Op::Safepoint);
    ops[branch_at] = Op::BranchIfNil {
        cond: 8,
        target: done,
    };
    for head_branch in head_branches {
        ops[head_branch] = Op::BranchIfNil {
            cond: 4,
            target: init_at,
        };
    }
    for (index, jump_slot) in jump_slots.into_iter().enumerate() {
        // Each block falls through to the next; only the last one reaches the
        // k-advance + Safepoint that closes the slice.
        let target = if index + 1 < spacing {
            copy_starts[index + 1]
        } else {
            join
        };
        ops[jump_slot] = Op::Jump { target };
    }
    ops
}

fn sum_ops() -> Vec<Op> {
    // 0 n, 1 s, 2 one, 3 cond. Result stays in slot 1.
    let mut ops = vec![
        Op::ConstFixnum { dst: 0, value: 5 },
        Op::ConstFixnum { dst: 1, value: 0 },
        Op::ConstFixnum { dst: 2, value: 1 },
    ];
    let loop_at = ops.len() as u16;
    ops.push(Op::Lt { dst: 3, a: 0, b: 2 });
    let branch_at = ops.len();
    ops.push(Op::BranchIfNil { cond: 3, target: 0 });
    let jump_done = ops.len();
    ops.push(Op::Jump { target: 0 });
    let body = ops.len() as u16;
    ops.push(Op::Add { dst: 1, a: 1, b: 0 });
    ops.push(Op::Sub { dst: 0, a: 0, b: 2 });
    ops.push(Op::Safepoint);
    ops.push(Op::Jump { target: loop_at });
    let done = ops.len() as u16;
    ops[branch_at] = Op::BranchIfNil {
        cond: 3,
        target: body,
    };
    ops[jump_done] = Op::Jump { target: done };
    ops
}

fn if_ops(cond_nil: bool) -> Vec<Op> {
    let mut ops = vec![
        if cond_nil {
            Op::ConstNil { dst: 1 }
        } else {
            Op::ConstT { dst: 1 }
        },
        Op::ConstFixnum { dst: 2, value: 1 },
        Op::ConstFixnum { dst: 3, value: 2 },
    ];
    let branch = ops.len();
    ops.push(Op::BranchIfNil { cond: 1, target: 0 });
    ops.push(Op::Move { dst: 0, src: 2 });
    let jump = ops.len();
    ops.push(Op::Jump { target: 0 });
    let else_at = ops.len() as u16;
    ops.push(Op::Move { dst: 0, src: 3 });
    let end = ops.len() as u16;
    ops[branch] = Op::BranchIfNil {
        cond: 1,
        target: else_at,
    };
    ops[jump] = Op::Jump { target: end };
    ops
}

fn arithmetic_ops() -> Vec<Vec<Op>> {
    vec![
        vec![
            Op::ConstFixnum { dst: 1, value: 40 },
            Op::ConstFixnum { dst: 2, value: 2 },
            Op::Add { dst: 0, a: 1, b: 2 },
        ],
        vec![
            Op::ConstFixnum { dst: 1, value: 10 },
            Op::ConstFixnum { dst: 2, value: 3 },
            Op::Sub { dst: 0, a: 1, b: 2 },
        ],
        vec![
            Op::ConstFixnum { dst: 1, value: 1 },
            Op::ConstFixnum { dst: 2, value: 2 },
            Op::Lt { dst: 0, a: 1, b: 2 },
        ],
        vec![
            Op::ConstFixnum { dst: 1, value: 2 },
            Op::ConstFixnum { dst: 2, value: 1 },
            Op::Lt { dst: 0, a: 1, b: 2 },
        ],
        vec![
            Op::ConstFixnum { dst: 1, value: 3 },
            Op::ConstFixnum { dst: 2, value: 3 },
            Op::Eq { dst: 0, a: 1, b: 2 },
        ],
        vec![
            Op::ConstFixnum { dst: 1, value: 3 },
            Op::ConstFixnum { dst: 2, value: 4 },
            Op::Eq { dst: 0, a: 1, b: 2 },
        ],
        if_ops(true),
        if_ops(false),
    ]
}

fn list_stop(cons_count: usize, _safepoints: usize) -> Option<Go> {
    match cons_count {
        2 | 6 | 9 => Some(Go::Forced),
        4 | 8 => Some(Go::Threshold),
        _ => None,
    }
}

fn sum_stop(_cons_count: usize, safepoints: usize) -> Option<Go> {
    if safepoints == 1 {
        Some(Go::Forced)
    } else {
        None
    }
}

fn no_stop(_cons_count: usize, _safepoints: usize) -> Option<Go> {
    None
}

fn oracle_lines() -> Vec<&'static str> {
    let lines: Vec<_> = include_str!("restricted_lisp_oracle.expected")
        .lines()
        .collect();
    assert_eq!(lines.len(), 43, "GNU 31.1 oracle fixture");
    lines
}

fn prin1(bits: usize) -> String {
    fn walk(value: TaggedValue, out: &mut String, depth: usize) {
        assert!(depth < 32, "prin1 depth");
        if value.is_nil() {
            out.push_str("nil");
            return;
        }
        if value.is_t() {
            out.push_str("t");
            return;
        }
        if let Some(number) = value.as_fixnum() {
            out.push_str(&number.to_string());
            return;
        }
        if let Some(string) = value.as_lisp_string() {
            let bytes = string.as_bytes();
            assert!(
                bytes.iter().all(|byte| (0x20..0x7f).contains(byte)),
                "prin1 of a string needing escapes"
            );
            out.push('"');
            out.push_str(&String::from_utf8_lossy(bytes));
            out.push('"');
            return;
        }
        assert!(value.is_cons(), "prin1 of {:#x}", value.bits());
        out.push('(');
        let mut cursor = value;
        let mut first = true;
        for _ in 0..32 {
            if !first {
                out.push(' ');
            }
            first = false;
            walk(cursor.cons_car_unobserved(), out, depth + 1);
            cursor = cursor.cons_cdr_unobserved();
            if cursor.is_nil() {
                out.push(')');
                return;
            }
            if !cursor.is_cons() {
                // Dotted tail: `(a . b)`, as GNU prin1 renders the (KEY . HASH)
                // pairs of the B3 bucket dumps.
                out.push_str(" . ");
                walk(cursor, out, depth + 1);
                out.push(')');
                return;
            }
        }
        panic!("prin1 length");
    }
    let mut out = String::new();
    walk(TaggedValue::from_bits(bits), &mut out, 0);
    out
}

fn home_index(slot: u16, len: usize) -> Result<usize, ExecError> {
    let slot = usize::from(slot);
    if slot >= len {
        Err(ExecError::Slot)
    } else {
        Ok(slot)
    }
}

fn jump_to(target: u16, len: usize) -> Result<usize, ExecError> {
    let target = usize::from(target);
    if target > len {
        Err(ExecError::Slot)
    } else {
        Ok(target)
    }
}

fn home_bits<'brand>(slice: &RunSlice<'brand, '_>, home: &SliceRoot<'brand>) -> usize {
    slice.word_bits_for_test(home)
}

fn exec<'brand>(
    slice: &mut RunSlice<'brand, '_>,
    homes: &mut [SliceRoot<'brand>],
    ops: &[Op],
    machine: &mut Machine,
    bound: u32,
    overlap: Option<&Overlap>,
    index: usize,
) -> Result<Flow, ExecError> {
    if let Some(overlap) = overlap
        && !machine.entered
    {
        overlap.arrive_enter(index);
        machine.entered = true;
    }
    let mut steps = 0u32;
    while machine.pc < ops.len() {
        steps += 1;
        if steps > STEP_CAP {
            return Err(ExecError::Backedge);
        }
        let from = u16::try_from(machine.pc).map_err(|_| ExecError::Slot)?;
        match ops[machine.pc] {
            Op::ConstNil { dst } => {
                let dst = home_index(dst, homes.len())?;
                slice.store_immediate(&mut homes[dst], TaggedValue::NIL.bits())?;
                machine.pc += 1;
            }
            Op::ConstT { dst } => {
                let dst = home_index(dst, homes.len())?;
                slice.store_immediate(&mut homes[dst], TaggedValue::T.bits())?;
                machine.pc += 1;
            }
            Op::ConstFixnum { dst, value } => {
                let fix = Fixnum::try_from(value).map_err(|_| ExecError::Overflow)?;
                let dst = home_index(dst, homes.len())?;
                slice.store_immediate(&mut homes[dst], TaggedValue::from_fixnum(fix).bits())?;
                machine.pc += 1;
            }
            Op::Move { dst, src } => {
                let src = home_index(src, homes.len())?;
                let dst = home_index(dst, homes.len())?;
                let bits = home_bits(slice, &homes[src]);
                slice.plant_word_for_test(&mut homes[dst], bits);
                machine.pc += 1;
            }
            Op::Add { dst, a, b } => {
                store_fixnum(slice, homes, dst, fix_calc(slice, homes, a, b, true)?)?;
                machine.pc += 1;
            }
            Op::Sub { dst, a, b } => {
                store_fixnum(slice, homes, dst, fix_calc(slice, homes, a, b, false)?)?;
                machine.pc += 1;
            }
            Op::Lt { dst, a, b } => {
                let (x, y) = fix_operands(slice, homes, a, b)?;
                let bits = if x < y {
                    TaggedValue::T.bits()
                } else {
                    TaggedValue::NIL.bits()
                };
                let dst = home_index(dst, homes.len())?;
                slice.store_immediate(&mut homes[dst], bits)?;
                machine.pc += 1;
            }
            Op::Eq { dst, a, b } => {
                let a = home_index(a, homes.len())?;
                let b = home_index(b, homes.len())?;
                let (x, y) = (home_bits(slice, &homes[a]), home_bits(slice, &homes[b]));
                let bits = if x == y {
                    TaggedValue::T.bits()
                } else {
                    TaggedValue::NIL.bits()
                };
                let dst = home_index(dst, homes.len())?;
                slice.store_immediate(&mut homes[dst], bits)?;
                machine.pc += 1;
            }
            Op::BranchIfNil { cond, target } => {
                let cond = home_index(cond, homes.len())?;
                let dest = jump_to(target, ops.len())?;
                if TaggedValue::from_bits(home_bits(slice, &homes[cond])).is_nil() {
                    if !edge(&mut machine.backedges, bound, from, target) {
                        return Err(ExecError::Backedge);
                    }
                    machine.pc = dest;
                } else {
                    machine.pc += 1;
                }
            }
            Op::Jump { target } => {
                let dest = jump_to(target, ops.len())?;
                if !edge(&mut machine.backedges, bound, from, target) {
                    return Err(ExecError::Backedge);
                }
                machine.pc = dest;
            }
            Op::Cons { dst, car, cdr } => {
                let car_i = home_index(car, homes.len())?;
                let cdr_i = home_index(cdr, homes.len())?;
                let dst_i = home_index(dst, homes.len())?;
                let car_home = homes[car_i].clone();
                let cdr_home = homes[cdr_i].clone();
                slice.cons_into(&car_home, &cdr_home, &mut homes[dst_i])?;
                let born = home_bits(slice, &homes[dst_i]);
                machine.owned.insert(born);
                machine.cons_count += 1;
                if let Some(overlap) = overlap {
                    // Relaxed: the witness reads these counts only after a
                    // channel receive or thread join, which orders them; the
                    // per-cons SeqCst RMW would be the measured bottleneck.
                    overlap.progress[index].0.fetch_add(1, Ordering::Relaxed);
                    if overlap.watch_alloc && !machine.allocated {
                        overlap.arrive_alloc(index);
                        machine.allocated = true;
                    }
                }
                machine.pc += 1;
            }
            Op::Car { dst, cell } => {
                field_load(slice, homes, dst, cell, true)?;
                machine.pc += 1;
            }
            Op::Cdr { dst, cell } => {
                field_load(slice, homes, dst, cell, false)?;
                machine.pc += 1;
            }
            Op::SetCar { cell, value } => {
                field_store(slice, homes, &machine.owned, cell, value, true)?;
                machine.pc += 1;
            }
            Op::SetCdr { cell, value } => {
                field_store(slice, homes, &machine.owned, cell, value, false)?;
                machine.pc += 1;
            }
            Op::Safepoint => {
                machine.safepoints += 1;
                machine.pc += 1;
                return Ok(Flow::Safepoint);
            }
            Op::HtGet { .. }
            | Op::HtPut { .. }
            | Op::HtRem { .. }
            | Op::HtClear { .. }
            | Op::HtMake { .. }
            | Op::HtCopy { .. }
            | Op::HtCount { .. }
            | Op::Global(_)
            | Op::Function(_)
            | Op::Intern(_) => {
                // Leases mint from `&mut RunAccess`, and the open slice
                // region is borrowing it: leave pc on the op and let the
                // driver perform it between regions (design §6).
                return Ok(Flow::Lease);
            }
        }
    }
    Ok(Flow::Done)
}

fn fix_operands<'brand>(
    slice: &RunSlice<'brand, '_>,
    homes: &[SliceRoot<'brand>],
    a: u16,
    b: u16,
) -> Result<(i64, i64), ExecError> {
    let a = home_index(a, homes.len())?;
    let b = home_index(b, homes.len())?;
    let x = TaggedValue::from_bits(home_bits(slice, &homes[a]))
        .as_fixnum()
        .ok_or(ExecError::Type)?;
    let y = TaggedValue::from_bits(home_bits(slice, &homes[b]))
        .as_fixnum()
        .ok_or(ExecError::Type)?;
    Ok((x, y))
}

fn fix_calc<'brand>(
    slice: &RunSlice<'brand, '_>,
    homes: &[SliceRoot<'brand>],
    a: u16,
    b: u16,
    add: bool,
) -> Result<Fixnum, ExecError> {
    let (x, y) = fix_operands(slice, homes, a, b)?;
    let value = if add {
        x.checked_add(y)
    } else {
        x.checked_sub(y)
    };
    let Some(value) = value else {
        return Err(ExecError::Overflow);
    };
    Fixnum::try_from(value).map_err(|_| ExecError::Overflow)
}

fn store_fixnum<'brand>(
    slice: &mut RunSlice<'brand, '_>,
    homes: &mut [SliceRoot<'brand>],
    dst: u16,
    value: Fixnum,
) -> Result<(), ExecError> {
    let dst = home_index(dst, homes.len())?;
    slice.store_immediate(&mut homes[dst], TaggedValue::from_fixnum(value).bits())?;
    Ok(())
}

fn field_load<'brand>(
    slice: &mut RunSlice<'brand, '_>,
    homes: &mut [SliceRoot<'brand>],
    dst: u16,
    cell: u16,
    car: bool,
) -> Result<(), ExecError> {
    let cell_i = home_index(cell, homes.len())?;
    let dst_i = home_index(dst, homes.len())?;
    let cell_home = homes[cell_i].clone();
    let value = TaggedValue::from_bits(home_bits(slice, &cell_home));
    if value.is_nil() {
        // Global reads may return nil. GNU car/cdr of nil both return nil.
        slice.store_immediate(&mut homes[dst_i], TaggedValue::NIL.bits())?;
        return Ok(());
    }
    if !value.is_cons() {
        return Err(ExecError::Type);
    }
    if car {
        slice.load_car(&cell_home, &mut homes[dst_i])?;
    } else {
        slice.load_cdr(&cell_home, &mut homes[dst_i])?;
    }
    Ok(())
}

fn field_store<'brand>(
    slice: &mut RunSlice<'brand, '_>,
    homes: &mut [SliceRoot<'brand>],
    owned: &FxHashSet<usize>,
    cell: u16,
    value: u16,
    car: bool,
) -> Result<(), ExecError> {
    let cell_i = home_index(cell, homes.len())?;
    let value_i = home_index(value, homes.len())?;
    let cell_bits = home_bits(slice, &homes[cell_i]);
    if !TaggedValue::from_bits(cell_bits).is_cons() {
        return Err(ExecError::Type);
    }
    if !owned.contains(&cell_bits) {
        return Err(ExecError::ForeignCons);
    }
    let value_home = homes[value_i].clone();
    if car {
        slice.store_car(&homes[cell_i], &value_home)?;
    } else {
        slice.store_cdr(&homes[cell_i], &value_home)?;
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// B3: sealed hash-table ops through the proof-2 leases (design §6, §7 B3)
// ---------------------------------------------------------------------------

/// Revalidation rounds for a user-test scan: how many times a lookup may
/// restart after the epoch moved under its callback window (the B1 bound).
const HT_RETRY_ROUNDS: usize = 8;

/// The hash and comparison programs standing in for one table's
/// user-defined test functions. Restricted callbacks replace the engine's
/// eval-applied closures: same protocol, closed program set.
struct UserTestProg<'env> {
    /// The owner slot holding this table's word.
    slot: u16,
    hash_ops: &'env [Op],
    cmp_ops: &'env [Op],
}

/// The B3 table environment of one session: which owner slots carry
/// user-defined tests, the callback scratch slots, and the counters the
/// tests assert on. Shared immutably by every owner.
struct HtEnv<'env> {
    users: &'env [UserTestProg<'env>],
    cb_key: u16,
    cb_cand: u16,
    cb_out: u16,
    /// Scans that re-ran because the epoch moved under a callback window.
    revalidations: AtomicUsize,
    /// P8 lever: while nonzero, a comparison window first performs a legal
    /// writer edit on the table under test — a no-op edit still bumps the
    /// epoch — modeling another owner's mutation inside the window. Only a
    /// single-owner session can observe the retry this provokes; by design
    /// the guard itself makes a same-table edit by the callback impossible.
    bump_before_cb: AtomicUsize,
    /// B4 overlap witness: reader rendezvous and writer-drain observation on
    /// one designated table slot. `None` for every pre-B4 session.
    witness: Option<&'env HtWitness<'env>>,
    /// B4 editor handshake: a second owner's refused-then-allowed edits
    /// timed against one user-test comparison window. `None` unless the
    /// session runs that row.
    editor: Option<&'env HtEditor>,
    /// B5 growth witness: storage-growth accounting of every plain put,
    /// charged to the acting owner (design §4 Rev R3, §7 B5). `None` for
    /// every pre-B5 session.
    growth: Option<&'env HtGrowth>,
    /// Per-owner count of completed top-level lease ops (design §6: per-owner
    /// progress counters). Empty for single-owner sessions.
    progress: &'env [AtomicUsize],
}

impl<'env> HtEnv<'env> {
    fn new(users: &'env [UserTestProg<'env>], cb_key: u16, cb_cand: u16, cb_out: u16) -> Self {
        Self {
            users,
            cb_key,
            cb_cand,
            cb_out,
            revalidations: AtomicUsize::new(0),
            bump_before_cb: AtomicUsize::new(0),
            witness: None,
            editor: None,
            growth: None,
            progress: &[],
        }
    }

    fn user_prog(&self, slot: u16) -> Option<&'env UserTestProg<'env>> {
        self.users.iter().find(|prog| prog.slot == slot)
    }
}

/// Bit 63 of the authority word (the writer license); the low 47 bits count
/// in-flight sealed readers. Mirrors the B1 model the leases implement —
/// kept as local constants because the executor may not name the lease
/// module. The witnesses assert the live values, so a drift is a test
/// failure, not a silent copy.
const HT_WRITER_BIT: u64 = 1 << 63;
const HT_READER_MASK: u64 = (1 << 47) - 1;

/// B4 overlap witness (design §6): a reader rendezvous inside one sealed
/// reader lease, the word values observed there, and the writer-drain
/// handshake. Every field is written only from owner threads inside lease
/// windows; the test asserts from the coordinator after the era.
struct HtWitness<'env> {
    /// The owner slot whose table the witnessed ops target. Ops on any other
    /// table ignore the witness entirely.
    table_slot: u16,
    /// Reader barrier size: the number of owners that must hold their reader
    /// lease simultaneously.
    readers: usize,
    /// 0 for a pure reader-overlap session; 1 when one owner runs the
    /// witnessed writer op, so the readers wait for its attempt and observe
    /// the drain.
    writers: usize,
    /// `arrive`'s barrier: fetch_add arrivals, seen receipts per owner.
    flag: AtomicUsize,
    seen: Vec<AtomicUsize>,
    abort: &'env AtomicBool,
    /// Max READER_COUNT observed by any reader while inside its lease.
    max_readers: AtomicU64,
    /// A reader observed the word with a nonzero count, no WRITER bit, while
    /// the writer was pre-acquisition: the writer's drain wait.
    drain_observed: AtomicBool,
    /// Set by the witnessed writer op before it attempts WRITER; readers in
    /// the barrier wait for it so the drain observation cannot race past.
    writer_attempting: AtomicBool,
    /// The word the writer observed once WRITER was acquired (the whole bit).
    writer_word: AtomicU64,
}

impl<'env> HtWitness<'env> {
    fn new(table_slot: u16, readers: usize, writers: usize, abort: &'env AtomicBool) -> Self {
        assert!(writers <= 1, "one witnessed writer op per session");
        let mut seen = Vec::with_capacity(readers);
        for _ in 0..readers {
            seen.push(AtomicUsize::new(0));
        }
        Self {
            table_slot,
            readers,
            writers,
            flag: AtomicUsize::new(0),
            seen,
            abort,
            max_readers: AtomicU64::new(0),
            drain_observed: AtomicBool::new(false),
            writer_attempting: AtomicBool::new(false),
            writer_word: AtomicU64::new(0),
        }
    }
}

/// B4 editor handshake: the state machine one other owner (the editor) and
/// the owner running a user-test comparison (the reader) step through, so
/// the refused and allowed edits land at exact points of the window.
/// 0 idle → 1 comparison hold taken → 2 editor's same-table edit attempted
/// (and refused by the depth guard) → 3 hold dropped → 4 editor's edit
/// landed (epoch moved). `armed` makes the sequence one-shot: later
/// comparison windows (revalidation rounds) run undisturbed.
struct HtEditor {
    phase: AtomicUsize,
    armed: AtomicBool,
    /// The editor observed the §3.1 depth guard refusing its same-table edit.
    refusal_seen: AtomicBool,
    /// The editor's post-window edit completed under WRITER.
    edit_landed: AtomicBool,
}

impl HtEditor {
    fn new() -> Self {
        Self {
            phase: AtomicUsize::new(0),
            armed: AtomicBool::new(true),
            refusal_seen: AtomicBool::new(false),
            edit_landed: AtomicBool::new(false),
        }
    }

    /// The reader side of one step: publish `from` done, wait for `to`.
    fn step(&self, from: usize, to: usize) {
        self.phase.store(from, Ordering::SeqCst);
        let deadline = Instant::now() + SPIN_WAIT;
        while self.phase.load(Ordering::SeqCst) != to {
            if Instant::now() >= deadline {
                panic!("editor handshake stalled at {from}, waiting for {to}");
            }
            thread::yield_now();
        }
    }

    /// The editor side: wait for `from`, run one bounded step, publish `to`.
    fn wait_then(&self, from: usize, to: usize, step: impl FnOnce()) {
        let deadline = Instant::now() + SPIN_WAIT;
        while self.phase.load(Ordering::SeqCst) != from {
            if Instant::now() >= deadline {
                panic!("editor handshake stalled waiting for {from}");
            }
            thread::yield_now();
        }
        step();
        self.phase.store(to, Ordering::SeqCst);
    }
}

/// B5 growth witness (design §4 Rev R3, §7 B5): what every plain put
/// charged its acting owner's GC counters, and whether that charge agrees
/// with the storage bytes the same edit grew. Per-put accounting is
/// immune to the interleaving — each edit is serialized under WRITER and
/// both sides of the comparison are read inside that one put's extent.
struct HtGrowth {
    /// Bytes charged per owner (its `hot.bytes_since_gc` delta across each
    /// witnessed put's lease, summed).
    charged: Vec<AtomicUsize>,
    /// Puts whose charge was nonzero (a storage growth event), per owner.
    events: Vec<AtomicUsize>,
    /// Puts where the charged delta disagreed with the storage-bytes delta.
    mismatches: AtomicUsize,
}

impl HtGrowth {
    fn new(owners: usize) -> Self {
        Self {
            charged: (0..owners).map(|_| AtomicUsize::new(0)).collect(),
            events: (0..owners).map(|_| AtomicUsize::new(0)).collect(),
            mismatches: AtomicUsize::new(0),
        }
    }

    /// Record one put's accounting: the owner-counter delta `charged_delta`
    /// and the storage-bytes delta `(before, after)` measured inside the
    /// same edit.
    fn record(&self, owner: usize, charged_delta: usize, before: usize, after: usize) {
        self.charged[owner].fetch_add(charged_delta, Ordering::Relaxed);
        if charged_delta > 0 {
            self.events[owner].fetch_add(1, Ordering::Relaxed);
        }
        if charged_delta != after.saturating_sub(before) {
            self.mismatches.fetch_add(1, Ordering::Relaxed);
        }
    }
}

/// The pre-admission setup of one B3 session: elisp builds each table (and
/// the planted bare symbols) in the serial world; the harness keeps the
/// values alive as transport roots and plants them into owner slots in
/// place of nil, so the root-slot bank budget is unchanged.
struct HtSetup {
    /// Eval'd before `admit_owned_parallel`; must return the plant list.
    source: &'static str,
    /// `(slot, fact)` per element of the plant list, in order.
    facts: Vec<(u16, Abs)>,
    nslots: usize,
}

impl HtSetup {
    /// The initial abstract facts prevalidation needs: every planted slot
    /// starts at its fact, every other slot at nil.
    fn initial(&self) -> Vec<(u16, Abs)> {
        self.facts.clone()
    }
}

/// Plant result words back into owner slots: the answer of a lease op and
/// the arguments of a callback cross the lease boundary as whole tagged
/// words, never as borrowed storage references (design §3.2).
fn plant_words<'era>(
    run: &mut RunAccess<'_, 'era>,
    slots: &[OwnerRootSlot<'era>],
    words: &[(u16, usize)],
) -> Result<(), ExecError> {
    run.with_slice(|slice| {
        let mut homes = Vec::with_capacity(slots.len());
        for slot in slots {
            homes.push(slice.bind(slot)?);
        }
        for &(slot, bits) in words {
            let index = home_index(slot, homes.len())?;
            slice.plant_word_for_test(&mut homes[index], bits);
        }
        Ok(())
    })
    .map_err(ExecError::from)?
}

/// Run one user-test callback program to completion and return the `cb_out`
/// word. Callback programs are safepoint-free; a nested table op inside a
/// callback ends the region like any other lease op and runs from a fresh
/// run access, where the nesting count refuses the mutating ones. Arguments
/// are planted on entry (pc == 0) only.
fn run_cb<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    ops: &[Op],
    args: &[(u16, usize)],
    index: usize,
) -> Result<usize, ExecError> {
    let mut machine = Machine::new();
    loop {
        let flow = with_run(owner, |run| {
            run.with_slice(|slice| {
                let mut homes = Vec::with_capacity(slots.len());
                for slot in slots {
                    homes.push(slice.bind(slot)?);
                }
                if machine.pc == 0 {
                    for &(slot, bits) in args {
                        let index = home_index(slot, homes.len())?;
                        slice.plant_word_for_test(&mut homes[index], bits);
                    }
                }
                exec(slice, &mut homes, ops, &mut machine, 0, None, 0)
            })
            .map_err(ExecError::from)?
        })?;
        match flow {
            Flow::Done => {
                return with_run(owner, |run| {
                    Ok(slots[usize::from(env.cb_out)].word_for_test(run))
                });
            }
            Flow::Safepoint => panic!("user test callback contains a safepoint op"),
            Flow::Lease => {
                let op = ops[machine.pc];
                machine.pc += 1;
                ht_step(owner, slots, op, Some(env), index)?;
            }
        }
    }
}

/// The shell configuration one brief reader lease can observe.
fn describe_table(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
) -> Result<(HashTableTest, bool), ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        let described = lease.describe();
        Ok((described.test, described.user_defined))
    })
}

fn epoch_of(owner: &mut Mutator<'_>, table_tv: TaggedValue) -> Result<u64, ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        Ok(lease.read(|s| s.switch_epoch))
    })
}

/// GNU's mutability refusal on the reader side: some owner is inside this
/// table's test or hash function, so a mutating op must refuse before it
/// scans (mirroring where the serial builtins check).
fn refuse_in_user_test(owner: &mut Mutator<'_>, table_tv: TaggedValue) -> Result<(), ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        if lease.user_test_depth() != 0 {
            return Err(ExecError::UserTestMutation);
        }
        Ok(())
    })
}

/// The user hash callback: hold the nesting count (no lease outstanding),
/// run the program, read a fixnum back.
fn user_wanted_hash<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    index: usize,
) -> Result<i64, ExecError> {
    let hold = with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        Ok(lease.hold_user_test()?)
    })?;
    let out = run_cb(
        owner,
        slots,
        env,
        prog.hash_ops,
        &[(env.cb_key, key_tv.bits())],
        index,
    )?;
    drop(hold);
    TaggedValue::from_bits(out)
        .as_fixnum()
        .ok_or(ExecError::Type)
}

/// The user comparison callback: optionally fire the P8 lever (a legal
/// writer edit with nothing held), then hold the nesting count across the
/// callback window and read its truthiness.
fn user_keys_equal<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_bits: usize,
    cand_bits: usize,
    index: usize,
) -> Result<bool, ExecError> {
    if env.bump_before_cb.load(Ordering::Acquire) > 0 {
        env.bump_before_cb.fetch_sub(1, Ordering::AcqRel);
        with_run(owner, |run| {
            let mut lease = TableMut::enter(run, table_tv)?;
            lease.edit(|_| ());
            Ok(())
        })?;
    }
    let hold = with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        Ok(lease.hold_user_test()?)
    })?;
    // B4: the one armed editor window. Phases 1→2 bracket the hold — the
    // editor's same-table edit attempt lands (refused by the §3.1 guard)
    // while this hold is live. Phases 3→4 bracket the return — the editor's
    // now-allowed edit lands before the scan's epoch re-read, so the
    // revalidation it provokes is a real other-owner edit.
    let editor = match env.editor {
        Some(hs) if hs.armed.swap(false, Ordering::SeqCst) => Some(hs),
        _ => None,
    };
    if let Some(hs) = editor {
        hs.step(1, 2);
    }
    let out = run_cb(
        owner,
        slots,
        env,
        prog.cmp_ops,
        &[(env.cb_key, key_bits), (env.cb_cand, cand_bits)],
        index,
    )?;
    drop(hold);
    if let Some(hs) = editor {
        hs.step(3, 4);
    }
    Ok(!TaggedValue::from_bits(out).is_nil())
}

/// Copy the remembered-hash candidates out under one reader lease, with the
/// epoch as it stood when the scan began.
fn user_candidates(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
    wanted: i64,
) -> Result<(u64, Vec<(usize, usize)>), ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        let before = lease.read(|s| s.switch_epoch);
        let candidates = lease.read(|s| {
            s.entries_in_slot_order()
                .filter(|entry| {
                    s.user_hash(&entry.key.to_hash_key(&HashTableTest::Eq)) == Some(wanted)
                })
                .map(|entry| (entry.key.bits(), entry.value.bits()))
                .collect()
        });
        Ok((before, candidates))
    })
}

/// One revalidating user-test lookup: hash, copy candidates under a lease,
/// compare with nothing held, and keep the answer only if the epoch held.
fn user_lookup<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    index: usize,
) -> Result<(usize, Option<usize>), ExecError> {
    for _ in 0..HT_RETRY_ROUNDS {
        let wanted = user_wanted_hash(owner, slots, env, prog, table_tv, key_tv, index)?;
        let (before, candidates) = user_candidates(owner, table_tv, wanted)?;
        let mut matched: Option<usize> = None;
        for &(cand_bits, value_bits) in &candidates {
            // GNU's lookup fast path (fns.c `hash_find_with_hash`): an
            // EQ-identical candidate matches without running the comparator.
            if cand_bits == key_tv.bits() {
                matched = Some(value_bits);
                break;
            }
            if user_keys_equal(
                owner,
                slots,
                env,
                prog,
                table_tv,
                key_tv.bits(),
                cand_bits,
                index,
            )? {
                matched = Some(value_bits);
                break;
            }
        }
        if epoch_of(owner, table_tv)? == before {
            let found_bits = if matched.is_some() {
                TaggedValue::T.bits()
            } else {
                TaggedValue::NIL.bits()
            };
            return Ok((found_bits, matched));
        }
        env.revalidations.fetch_add(1, Ordering::Relaxed);
    }
    Err(ExecError::EpochRetry)
}

/// The scan `user_put`/`user_rem` share with the lookup, returning the
/// matched candidate's bits (the storage key is rematerialized from them
/// under WRITER; `HashKey` never crosses a lease boundary).
fn user_scan<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    index: usize,
) -> Result<Option<usize>, ExecError> {
    for _ in 0..HT_RETRY_ROUNDS {
        let wanted = user_wanted_hash(owner, slots, env, prog, table_tv, key_tv, index)?;
        let (before, candidates) = user_candidates(owner, table_tv, wanted)?;
        let mut matched: Option<usize> = None;
        for &(cand_bits, _) in &candidates {
            // The same EQ fast path as the lookup (GNU fns.c
            // `hash_find_with_hash`): identical objects skip the comparator.
            if cand_bits == key_tv.bits() {
                matched = Some(cand_bits);
                break;
            }
            if user_keys_equal(
                owner,
                slots,
                env,
                prog,
                table_tv,
                key_tv.bits(),
                cand_bits,
                index,
            )? {
                matched = Some(cand_bits);
                break;
            }
        }
        if epoch_of(owner, table_tv)? == before {
            return Ok(matched);
        }
        env.revalidations.fetch_add(1, Ordering::Relaxed);
    }
    Err(ExecError::EpochRetry)
}

/// `gethash` on a plain table: one sealed read of the engine lookup.
fn plain_get(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    test: HashTableTest,
) -> Result<(usize, usize), ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        // The lookup's reference stays inside the read; only the whole
        // tagged word crosses back out.
        let found = lease.read(|s| {
            s.try_lookup(key_tv, test, false)
                .map_err(|_| ExecError::Type)
                .map(|found| found.map(|value| value.bits()))
        })?;
        Ok(match found {
            Some(bits) => (TaggedValue::T.bits(), bits),
            None => (TaggedValue::NIL.bits(), TaggedValue::NIL.bits()),
        })
    })
}

/// `puthash` on a plain table: the serial builtin's storage steps, executed
/// inside one writer lease instead of the serial choke point. With a B4
/// witness, the writer first waits for the readers' barrier, then publishes
/// its attempt — so `TableMut::enter`'s drain spin happens with the reader
/// count provably live, and the word observed after it is WRITER alone.
/// With a B5 growth witness, the put also measures what the edit charged
/// the owner's GC counters against what the storage actually grew.
fn plain_put(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    value_tv: TaggedValue,
    test: HashTableTest,
    witness: Option<(&HtWitness<'_>, usize)>,
    growth: Option<(&HtGrowth, usize)>,
) -> Result<(), ExecError> {
    with_run(owner, |run| {
        if let Some((w, _)) = witness {
            let deadline = Instant::now() + SPIN_WAIT;
            while w.flag.load(Ordering::SeqCst) != w.readers {
                if w.abort.load(Ordering::SeqCst) || Instant::now() >= deadline {
                    panic!("writer waited past the readers' barrier");
                }
                thread::yield_now();
            }
            w.writer_attempting.store(true, Ordering::SeqCst);
        }
        // Read before the lease: fixnum keys cons nothing on the way in, so
        // the delta across the lease is exactly the edit's own charges.
        let charged_before = growth.map(|_| run.state().hot.bytes_since_gc);
        let mut lease = TableMut::enter(run, table_tv)?;
        if lease.user_test_mutation_refused() {
            return Err(ExecError::UserTestMutation);
        }
        let storage = lease.edit(|ht| {
            let before = ht.data.known_storage_bytes();
            match ht
                .data
                .try_probe_for_insert(key_tv, test, false)
                .map_err(|_| ExecError::Type)?
            {
                HashProbe::Found(slot) => {
                    if let Some(stored) = ht.data.slot_value_mut(slot) {
                        *stored = value_tv;
                    }
                }
                HashProbe::Absent(hash) => {
                    maybe_resize_hash_table_for_insert(ht, true);
                    let key = key_tv.to_hash_key_swp(&test, false);
                    ht.data.insert_absent(hash, key, key_tv, value_tv);
                }
            }
            Ok::<(usize, usize), ExecError>((before, ht.data.known_storage_bytes()))
        })?;
        if let Some((w, _)) = witness {
            // Enter returned only after every reader count drained, so the
            // whole word is WRITER alone here — the drain's other half. The
            // exit's return to quiescent is the collection's own accounting
            // (a leaked count or bit fails the era's stop/complete).
            let word = lease.authority_word().load(Ordering::Acquire);
            w.writer_word.fetch_max(word, Ordering::SeqCst);
        }
        drop(lease);
        if let Some((g, index)) = growth {
            let charged_delta = run.state().hot.bytes_since_gc - charged_before.unwrap_or(0);
            let (before, after) = storage;
            g.record(index, charged_delta, before, after);
        }
        Ok(())
    })
}

/// `remhash` on a plain table; the word says whether a key left.
fn plain_rem(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    test: HashTableTest,
) -> Result<usize, ExecError> {
    with_run(owner, |run| {
        let mut lease = TableMut::enter(run, table_tv)?;
        if lease.user_test_mutation_refused() {
            return Err(ExecError::UserTestMutation);
        }
        let removed = lease.edit(|ht| {
            ht.data
                .try_remove_by_value(key_tv, test, false)
                .map_err(|_| ExecError::Type)
        })?;
        Ok(if removed.is_some() {
            TaggedValue::T.bits()
        } else {
            TaggedValue::NIL.bits()
        })
    })
}

/// Observe the real reader lease used by count or copy while its count is live.
fn observe_reader_witness(lease: &TableRef<'_>, w: &HtWitness<'_>, index: usize) {
    let word = lease.authority_word().load(Ordering::Acquire);
    w.max_readers
        .fetch_max(word & HT_READER_MASK, Ordering::SeqCst);
    // All `readers` owners are inside their reader lease at once:
    // each holds one count, so the word observed after the barrier
    // is exactly the overlap witness (design §6).
    arrive(&w.flag, &w.seen[index], w.readers, w.abort);
    let word = lease.authority_word().load(Ordering::Acquire);
    w.max_readers
        .fetch_max(word & HT_READER_MASK, Ordering::SeqCst);
    if w.writers > 0 {
        // The writer publishes its attempt only after every reader
        // is inside, so this wait cannot miss it; the writer cannot
        // take WRITER while our count is live, so the word observed
        // here IS its drain wait: nonzero readers, no writer bit.
        let deadline = Instant::now() + SPIN_WAIT;
        while !w.writer_attempting.load(Ordering::SeqCst) {
            if w.abort.load(Ordering::SeqCst) || Instant::now() >= deadline {
                panic!("reader waited past the writer's attempt");
            }
            thread::yield_now();
        }
        let word = lease.authority_word().load(Ordering::Acquire);
        assert!(
            word & HT_READER_MASK > 0 && word & HT_WRITER_BIT == 0,
            "a live reader count excludes the writer bit"
        );
        w.drain_observed.store(true, Ordering::SeqCst);
    }
}

/// `hash-table-count` as one sealed read. With a B4 witness, the read is
/// also the reader rendezvous: the lease holds across the barrier, and the
/// authority word observed there is the overlap evidence.
fn plain_count(
    owner: &mut Mutator<'_>,
    table_tv: TaggedValue,
    witness: Option<(&HtWitness<'_>, usize)>,
) -> Result<usize, ExecError> {
    with_run(owner, |run| {
        let lease = TableRef::enter(run, table_tv)?;
        if let Some((w, index)) = witness {
            observe_reader_witness(&lease, w, index);
        }
        Ok(lease.read(|s| s.len()))
    })
}

/// `puthash` on a user-test table: refuse inside a test, revalidating scan,
/// then the serial builtin's edit steps under WRITER — overwrite the matched
/// key, or insert and remember the hash (the fast arm's discipline).
fn user_put<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    value_tv: TaggedValue,
    index: usize,
) -> Result<(), ExecError> {
    refuse_in_user_test(owner, table_tv)?;
    let wanted = user_wanted_hash(owner, slots, env, prog, table_tv, key_tv, index)?;
    let matched = user_scan(owner, slots, env, prog, table_tv, key_tv, index)?;
    with_run(owner, |run| {
        let mut lease = TableMut::enter(run, table_tv)?;
        if lease.user_test_mutation_refused() {
            return Err(ExecError::UserTestMutation);
        }
        lease.edit(|ht| {
            let storage_key = match matched {
                Some(bits) => TaggedValue::from_bits(bits).to_hash_key(&HashTableTest::Eq),
                None => key_tv.to_hash_key(&HashTableTest::Eq),
            };
            if let Some(stored) = ht.data.get_mut(&storage_key) {
                *stored = value_tv;
            } else {
                maybe_resize_hash_table_for_insert(ht, true);
                ht.insert(storage_key.clone(), key_tv, value_tv);
            }
            ht.data.set_user_hash(storage_key, wanted);
        });
        Ok(())
    })
}

/// `remhash` on a user-test table: refuse inside a test, revalidating scan,
/// then remove the matched storage key under WRITER.
fn user_rem<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    env: &HtEnv<'_>,
    prog: &UserTestProg<'_>,
    table_tv: TaggedValue,
    key_tv: TaggedValue,
    index: usize,
) -> Result<usize, ExecError> {
    refuse_in_user_test(owner, table_tv)?;
    let matched = user_scan(owner, slots, env, prog, table_tv, key_tv, index)?;
    let removed = with_run(owner, |run| {
        let mut lease = TableMut::enter(run, table_tv)?;
        if lease.user_test_mutation_refused() {
            return Err(ExecError::UserTestMutation);
        }
        Ok(lease.edit(|ht| match matched {
            Some(bits) => {
                let storage_key = TaggedValue::from_bits(bits).to_hash_key(&HashTableTest::Eq);
                ht.data.remove(&storage_key)
            }
            None => None,
        }))
    })?;
    Ok(if removed.is_some() {
        TaggedValue::T.bits()
    } else {
        TaggedValue::NIL.bits()
    })
}

/// Read the operand words of one table op out of owner slots.
fn ht_operand<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    which: &[u16],
) -> Result<Vec<TaggedValue>, ExecError> {
    with_run(owner, |run| {
        Ok(which
            .iter()
            .map(|&slot| TaggedValue::from_bits(slots[usize::from(slot)].word_for_test(run)))
            .collect())
    })
}

/// Perform one sealed table op between slice regions. The parameter is the
/// owner (not a run access) because user tests run whole callback programs,
/// which open slice regions and mint leases of their own. `index` is the
/// owner's session index: it names the per-owner progress counter and the
/// witness barrier receipt.
fn ht_step<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    op: Op,
    env: Option<&HtEnv<'_>>,
    index: usize,
) -> Result<(), ExecError> {
    // Construction never needs an operand Value in the executor: the sealed
    // allocation bridge validates both homes, links/account-charges the shell,
    // and installs the result before this run can reach a poll point.
    match op {
        Op::HtMake { dst, test, size } => {
            let dst = home_index(dst, slots.len())?;
            return with_run(owner, |run| {
                run.make_hash_table_into(test, size, &slots[dst])?;
                Ok(())
            });
        }
        Op::HtCopy { table, dst } => {
            let source = home_index(table, slots.len())?;
            let dst = home_index(dst, slots.len())?;
            return with_run(owner, |run| {
                if let Some(witness) =
                    env.and_then(|env| env.witness.filter(|witness| witness.table_slot == table))
                {
                    run.copy_hash_table_into_observed(&slots[source], &slots[dst], |lease| {
                        observe_reader_witness(lease, witness, index);
                    })?;
                } else {
                    run.copy_hash_table_into(&slots[source], &slots[dst])?;
                }
                Ok(())
            });
        }
        _ => {}
    }
    // The user-program dispatch table: a registered program for the table
    // slot selects the user path, and the harness invariant is that those
    // tables really are user-defined (and vice versa for the plain path).
    let (table, user_path) = match op {
        Op::HtGet { table, .. }
        | Op::HtPut { table, .. }
        | Op::HtRem { table, .. }
        | Op::HtClear { table }
        | Op::HtCount { table, .. } => {
            let prog = env.and_then(|env| env.user_prog(table));
            (table, prog)
        }
        _ => unreachable!("ht_step on a non-table op"),
    };
    let table_tv = ht_operand(owner, slots, &[table])?[0];
    let (test, user_defined) = describe_table(owner, table_tv)?;
    let prog = match user_path {
        Some(prog) => {
            assert!(
                user_defined,
                "harness registered a user program for a plain table"
            );
            Some(prog)
        }
        None => {
            assert!(
                !user_defined || matches!(op, Op::HtCount { .. } | Op::HtClear { .. }),
                "user-defined table without a registered test program"
            );
            None
        }
    };
    let env_of = || env.expect("a registered user program implies an env");
    // The B4 witness only sees plain ops on its designated table slot; every
    // other op (and every session without a witness) runs unchanged.
    let witness = env
        .and_then(|env| env.witness.filter(|w| w.table_slot == table))
        .map(|w| (w, index));
    // The B5 growth witness measures every plain put of a growth session.
    let growth = env.and_then(|env| env.growth).map(|g| (g, index));
    match op {
        Op::HtGet {
            key, found, value, ..
        } => {
            let key_tv = ht_operand(owner, slots, &[key])?[0];
            let (found_bits, value_bits) = match prog {
                Some(prog) => {
                    let (found_bits, matched) =
                        user_lookup(owner, slots, env_of(), prog, table_tv, key_tv, index)?;
                    (found_bits, matched.unwrap_or(TaggedValue::NIL.bits()))
                }
                None => plain_get(owner, table_tv, key_tv, test)?,
            };
            with_run(owner, |run| {
                plant_words(run, slots, &[(found, found_bits), (value, value_bits)])
            })
        }
        Op::HtPut { key, value, .. } => {
            let words = ht_operand(owner, slots, &[key, value])?;
            match prog {
                Some(prog) => user_put(
                    owner,
                    slots,
                    env_of(),
                    prog,
                    table_tv,
                    words[0],
                    words[1],
                    index,
                ),
                None => plain_put(owner, table_tv, words[0], words[1], test, witness, growth),
            }
        }
        Op::HtRem { key, found, .. } => {
            let key_tv = ht_operand(owner, slots, &[key])?[0];
            let found_bits = match prog {
                Some(prog) => user_rem(owner, slots, env_of(), prog, table_tv, key_tv, index)?,
                None => plain_rem(owner, table_tv, key_tv, test)?,
            };
            with_run(owner, |run| plant_words(run, slots, &[(found, found_bits)]))
        }
        Op::HtClear { .. } => with_run(owner, |run| {
            let mut lease = TableMut::enter(run, table_tv)?;
            if lease.user_test_mutation_refused() {
                return Err(ExecError::UserTestMutation);
            }
            lease.edit(|ht| ht.data.clear());
            Ok(())
        }),
        Op::HtCount { dst, .. } => {
            // Counting never consults the test: one sealed read either way.
            let count = plain_count(owner, table_tv, witness)?;
            let number = i64::try_from(count).map_err(|_| ExecError::Overflow)?;
            let fix = Fixnum::try_from(number).map_err(|_| ExecError::Overflow)?;
            let bits = TaggedValue::from_fixnum(fix).bits();
            with_run(owner, |run| plant_words(run, slots, &[(dst, bits)]))
        }
        _ => unreachable!("ht_step on a non-table op"),
    }
}

fn with_run<'era, R>(
    owner: &mut Mutator<'era>,
    body: impl FnOnce(&mut RunAccess<'_, 'era>) -> Result<R, ExecError>,
) -> Result<R, ExecError> {
    let deadline = Instant::now() + SPIN_WAIT;
    loop {
        match owner.run_access() {
            Ok(mut run) => {
                let result = body(&mut run);
                // `with_slice` already closed the region. Closing again is a no-op
                // and still runs when the body refused a foreign cons.
                if let Err(error) = run.close_cons_region() {
                    return Err(error.into());
                }
                return result;
            }
            Err(RegistrationError::Busy) if Instant::now() < deadline => thread::yield_now(),
            Err(error) => return Err(error.into()),
        }
    }
}

fn one_slice<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    ops: &[Op],
    machine: &mut Machine,
    bound: u32,
    overlap: Option<&Overlap>,
    index: usize,
    ht: Option<&HtEnv<'_>>,
) -> Result<(bool, usize, Vec<usize>), ExecError> {
    let mut hit = false;
    loop {
        // A lease op ends the region and hands back `owner`, so the op runs
        // with a fresh run access; the machine stays on the same pc and the
        // next iteration re-enters the slice past the op.
        let flow = with_run(owner, |run| {
            run.with_slice(|slice| {
                let mut homes = Vec::with_capacity(slots.len());
                for slot in slots {
                    homes.push(slice.bind(slot)?);
                }
                exec(slice, &mut homes, ops, machine, bound, overlap, index)
            })
            .map_err(ExecError::from)?
        })?;
        match flow {
            Flow::Done => break,
            Flow::Safepoint => {
                hit = true;
                break;
            }
            Flow::Lease => {
                let op = ops[machine.pc];
                machine.pc += 1;
                ht_step(owner, slots, op, ht, index)?;
                // Design §6: per-owner progress counter — one increment per
                // completed top-level lease op. An empty `progress` means
                // the session does not track it (the B3 single-owner rows).
                if let Some(env) = ht {
                    if let Some(counter) = env.progress.get(index) {
                        counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        }
    }
    with_run(owner, |run| {
        let bytes = run.state().hot.bytes_since_gc;
        let words = slots.iter().map(|slot| slot.word_for_test(run)).collect();
        Ok((hit, bytes, words))
    })
}

fn activate_owner<'era>(permit: MutatorPermit<'era>, heap: HeapIdentity, id: u64) -> Mutator<'era> {
    retry_owned(
        Deadline::contention(),
        permit,
        |permit| permit.activate(heap, id),
        |failure| match contention::registration(&failure.reason) {
            Retry::Busy => ControlFlow::Continue(failure.into_permit()),
            Retry::Refuse => ControlFlow::Break(failure),
        },
    )
    .unwrap_or_else(|failure| panic!("activate: {:?}", failure.reason))
}

fn restore(owner: &mut Mutator<'_>) {
    let image = owner
        .begin_restore_retry(false)
        .unwrap_or_else(|error| panic!("restore: {error:?}"));
    retry_owned(
        Deadline::contention(),
        image,
        |image| image.apply(),
        |failure| match contention::rendezvous(&failure.reason) {
            Retry::Busy => ControlFlow::Continue(failure.into_image()),
            Retry::Refuse => ControlFlow::Break(failure),
        },
    )
    .unwrap_or_else(|failure| panic!("apply: {:?}", failure.reason))
}

fn retire(owner: Mutator<'_>) {
    retry_owned(
        Deadline::contention(),
        owner,
        |owner| owner.retire_rooted(&[]),
        |failure| match contention::rendezvous(&failure.reason) {
            Retry::Busy => ControlFlow::Continue(failure.into_owner()),
            Retry::Refuse => ControlFlow::Break(failure),
        },
    )
    .unwrap_or_else(|failure| panic!("retire: {:?}", failure.reason))
}

fn complete(stopped: AllStopped<'_>) {
    retry_owned(
        Deadline::contention(),
        stopped,
        |stopped| stopped.complete(),
        |failure| match contention::rendezvous(&failure.reason) {
            Retry::Busy => ControlFlow::Continue(failure.into_stopped()),
            Retry::Refuse => ControlFlow::Break(failure),
        },
    )
    .unwrap_or_else(|failure| panic!("complete: {:?}", failure.reason))
}

fn on_heap<R>(
    era: &ParallelHeapEra<'_>,
    stopped: &AllStopped<'_>,
    body: impl Fn(&crate::tagged::gc::TaggedHeap) -> R,
) -> R {
    retry_rv(|| era.with_stopped_heap_for_test(stopped, &body))
}

fn open_collection<'a>(
    era: &'a ParallelHeapEra<'_>,
    go_txs: &[Sender<Go>],
    kind: Go,
    next_epoch: &mut u64,
) -> AllStopped<'a> {
    // Owners are blocked on this signal, outside `with_slice`, before `stop`.
    for tx in go_txs {
        tx.send(kind)
            .unwrap_or_else(|_| panic!("go channel closed"));
    }
    let stopped = retry_rv(|| era.stop());
    let epoch = stopped.epoch().get();
    assert_eq!(epoch, *next_epoch, "collection epochs stay strictly +1");
    *next_epoch += 1;
    retry_reg(|| era.return_cons_storage(&stopped));
    stopped
}

fn collect_major(era: &ParallelHeapEra<'_>, stopped: &mut AllStopped<'_>) {
    let report = retry_rv(|| era.collect_stopped(stopped));
    assert!(report.major, "restricted overlap collects a major cycle");
}

fn release_cycle(era: &ParallelHeapEra<'_>, stopped: AllStopped<'_>, resume: bool) {
    if resume {
        retry_reg(|| era.resume_cons_storage(&stopped));
    }
    complete(stopped);
}

fn gather<T>(rx: &Receiver<T>, n: usize, index_of: impl Fn(&T) -> usize) -> Vec<T> {
    let mut slots: Vec<Option<T>> = (0..n).map(|_| None).collect();
    for _ in 0..n {
        let item = recv(rx);
        let index = index_of(&item);
        assert!(slots[index].is_none(), "duplicate owner message");
        slots[index] = Some(item);
    }
    slots
        .into_iter()
        .map(|item| item.expect("message"))
        .collect()
}

fn seed_nils<'era>(
    owner: &mut Mutator<'era>,
    nil: &SharedRoot,
    nslots: usize,
) -> Vec<OwnerRootSlot<'era>> {
    must(with_run(owner, |run| {
        let mut slots = Vec::with_capacity(nslots);
        for _ in 0..nslots {
            slots.push(run.root_slot(nil)?);
        }
        Ok(slots)
    }))
}

/// Seed with the B3 table setup instead: designated slots take their planted
/// transport roots (tables, bare symbols) and the rest take nil. The
/// root-slot bank budget is unchanged — planted words replace nil seeds.
fn seed_planted<'era>(
    owner: &mut Mutator<'era>,
    nil: &SharedRoot,
    planted: &[Option<SharedRoot>],
) -> Vec<OwnerRootSlot<'era>> {
    must(with_run(owner, |run| {
        let mut slots = Vec::with_capacity(planted.len());
        for root in planted {
            slots.push(run.root_slot(root.as_ref().unwrap_or(nil))?);
        }
        Ok(slots)
    }))
}

fn shutdown_owner(
    mut owner: Mutator<'_>,
    index: usize,
    ready_tx: &Sender<usize>,
    done_rx: &Receiver<()>,
    retired_tx: &Sender<usize>,
) {
    must(with_run(&mut owner, |run| {
        run.clear_root_slots()?;
        Ok(())
    }));
    retry_rv(|| owner.enter_native(&[]));
    ready_tx
        .send(index)
        .unwrap_or_else(|_| panic!("ready channel closed"));
    recv(done_rx);
    retire(owner);
    retired_tx
        .send(index)
        .unwrap_or_else(|_| panic!("retired channel closed"));
}

struct OwnerLaunch<'permit, 'env> {
    permit: MutatorPermit<'permit>,
    heap: HeapIdentity,
    id: u64,
    nil: &'env SharedRoot,
    programs: Vec<Vec<Op>>,
    nslots: usize,
    bound: u32,
    line_slot: usize,
    stop_at: fn(usize, usize) -> Option<Go>,
    overlap: Option<&'env Overlap>,
    index: usize,
    pause_tx: Sender<Snap>,
    go_rx: Receiver<Go>,
    end_tx: Sender<EndState>,
    ready_tx: Sender<usize>,
    done_rx: Receiver<()>,
    retired_tx: Sender<usize>,
    registry: &'permit Registry,
    abort: &'env AtomicBool,
    /// B3: planted slot seeds (`None` = nil) and the table environment.
    /// Empty/`None` for every pre-B3 program.
    planted: &'env [Option<SharedRoot>],
    ht: Option<&'env HtEnv<'env>>,
}

impl OwnerLaunch<'_, '_> {
    fn run(self) {
        let Self {
            permit,
            heap,
            id,
            nil,
            programs,
            nslots,
            bound,
            line_slot,
            stop_at,
            overlap,
            index,
            pause_tx,
            go_rx,
            end_tx,
            ready_tx,
            done_rx,
            retired_tx,
            registry,
            abort,
            planted,
            ht,
        } = self;
        let _panic = PanicFlag { registry, abort };
        let mut owner = activate_owner(permit, heap, id);
        let slots = if planted.is_empty() {
            seed_nils(&mut owner, nil, nslots)
        } else {
            assert_eq!(planted.len(), nslots, "one seed per root-slot");
            seed_planted(&mut owner, nil, planted)
        };
        let mut machine = Machine::new();
        let mut lines = Vec::with_capacity(programs.len());
        let mut final_slots = Vec::new();
        for program in &programs {
            machine.restart();
            final_slots = run_program(
                &mut owner,
                &slots,
                program,
                &mut machine,
                bound,
                overlap,
                index,
                &pause_tx,
                &go_rx,
                stop_at,
                ht,
            );
            lines.push(prin1(final_slots[line_slot]));
        }
        end_tx
            .send(EndState {
                index,
                lines,
                slots: final_slots,
                conses: machine.owned,
                cons_count: machine.cons_count,
                safepoints: machine.safepoints,
            })
            .unwrap_or_else(|_| panic!("end channel closed"));
        shutdown_owner(owner, index, &ready_tx, &done_rx, &retired_tx);
    }
}

fn run_program<'era>(
    owner: &mut Mutator<'era>,
    slots: &[OwnerRootSlot<'era>],
    ops: &[Op],
    machine: &mut Machine,
    bound: u32,
    overlap: Option<&Overlap>,
    index: usize,
    pause_tx: &Sender<Snap>,
    go_rx: &Receiver<Go>,
    stop_at: fn(usize, usize) -> Option<Go>,
    ht: Option<&HtEnv<'_>>,
) -> Vec<usize> {
    loop {
        let (hit, bytes, words) = must(one_slice(
            owner, slots, ops, machine, bound, overlap, index, ht,
        ));
        if !hit {
            return words;
        }
        let Some(kind) = stop_at(machine.cons_count, machine.safepoints) else {
            continue;
        };
        pause_tx
            .send(Snap {
                index,
                cons_count: machine.cons_count,
                safepoints: machine.safepoints,
                bytes,
                slots: words,
                conses: machine.owned.clone(),
            })
            .unwrap_or_else(|_| panic!("pause channel closed"));
        let go = recv(go_rx);
        assert_eq!(go, kind);
        if kind == Go::Threshold {
            assert!(bytes > 0, "threshold safepoint requires bytes_since_gc > 0");
            retry_rv(|| owner.request_collection());
        }
        let epoch = retry_rv(|| owner.wait_request());
        retry_rv(|| owner.acknowledge(epoch, &[]));
        // Dropped `RunAccess` above. Restore waits through Tracing.
        restore(owner);
    }
}

fn with_admitted(body: impl FnOnce(&ParallelHeapEra, bool, &SharedRoot, &SharedRoot)) {
    let mut context = Context::new();
    let generational = context.tagged_heap.generational_enabled();
    // SAFETY: nil and a fixnum are immediates of this Context's heap.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let one = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::fixnum(1)) };
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                body(era, generational, &nil, &one);
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
}

struct Session<'body, 'heap> {
    era: &'body ParallelHeapEra<'heap>,
    go_txs: Vec<Sender<Go>>,
    done_txs: Vec<Sender<()>>,
    pause_rx: Receiver<Snap>,
    end_rx: Receiver<EndState>,
    ready_rx: Receiver<usize>,
    retired_rx: Receiver<usize>,
    next_epoch: u64,
}

impl<'body, 'heap> Session<'body, 'heap> {
    fn launch<'scope, 'env>(
        era: &'body ParallelHeapEra<'heap>,
        nil: &'env SharedRoot,
        id_base: u64,
        programs: &[Vec<Op>],
        nslots: usize,
        bound: u32,
        line_slot: usize,
        stop_at: fn(usize, usize) -> Option<Go>,
        overlap: Option<&'env Overlap>,
        abort: &'env AtomicBool,
        scope: &'scope thread::Scope<'scope, 'env>,
    ) -> Self
    where
        'body: 'env,
    {
        let n = programs.len();
        let heap = era.heap_identity();
        let registry = era.registry();
        let (pause_tx, pause_rx) = channel();
        let (end_tx, end_rx) = channel();
        let (ready_tx, ready_rx) = channel();
        let (retired_tx, retired_rx) = channel();
        let mut go_txs = Vec::with_capacity(n);
        let mut done_txs = Vec::with_capacity(n);
        // Design 3.3: fixed participants register while stopped, before any
        // owner thread exists. `OwnerLaunch::run` activates immediately, and
        // activation holds the registry gate; registering owner i+1 after
        // spawning owner i races that gate, which answers Busy instead of
        // blocking — a slow scheduler (TSan) turns the harness itself into
        // the failure. Register every permit first, then hand each to its
        // thread.
        let mut permits = Vec::with_capacity(n);
        for index in 0..n {
            let id = id_base + index as u64;
            permits.push(retry_reg(|| era.register(id, nslots)));
        }
        for (index, permit) in permits.into_iter().enumerate() {
            let id = id_base + index as u64;
            let (go_tx, go_rx) = channel();
            let (done_tx, done_rx) = channel();
            go_txs.push(go_tx);
            done_txs.push(done_tx);
            let launch = OwnerLaunch {
                permit,
                heap,
                id,
                nil,
                programs: vec![programs[index].clone()],
                nslots,
                bound,
                line_slot,
                stop_at,
                overlap,
                index,
                pause_tx: pause_tx.clone(),
                go_rx,
                end_tx: end_tx.clone(),
                ready_tx: ready_tx.clone(),
                done_rx,
                retired_tx: retired_tx.clone(),
                registry,
                abort,
                planted: &[],
                ht: None,
            };
            scope.spawn(move || launch.run());
        }
        drop(pause_tx);
        drop(end_tx);
        drop(ready_tx);
        drop(retired_tx);
        Self {
            era,
            go_txs,
            done_txs,
            pause_rx,
            end_rx,
            ready_rx,
            retired_rx,
            next_epoch: 1,
        }
    }

    fn pauses(&self) -> Vec<Snap> {
        gather(&self.pause_rx, self.go_txs.len(), |snap| snap.index)
    }

    fn round(
        &mut self,
        kind: Go,
        resume: bool,
        after: impl FnOnce(&ParallelHeapEra<'heap>, &AllStopped<'_>),
    ) {
        let mut stopped = open_collection(self.era, &self.go_txs, kind, &mut self.next_epoch);
        collect_major(self.era, &mut stopped);
        after(self.era, &stopped);
        release_cycle(self.era, stopped, resume);
    }

    fn round_with_before(
        &mut self,
        kind: Go,
        resume: bool,
        before: impl FnOnce(&ParallelHeapEra<'heap>, &AllStopped<'_>),
        after: impl FnOnce(&ParallelHeapEra<'heap>, &AllStopped<'_>),
    ) {
        let mut stopped = open_collection(self.era, &self.go_txs, kind, &mut self.next_epoch);
        before(self.era, &stopped);
        collect_major(self.era, &mut stopped);
        after(self.era, &stopped);
        release_cycle(self.era, stopped, resume);
    }

    fn finish_owners(
        &mut self,
        after: impl FnOnce(&ParallelHeapEra<'heap>, &AllStopped<'_>),
    ) -> Vec<EndState> {
        let ends = gather(&self.end_rx, self.go_txs.len(), |end| end.index);
        let mut seen = vec![false; self.go_txs.len()];
        for _ in 0..self.go_txs.len() {
            let index = recv(&self.ready_rx);
            assert!(!seen[index], "duplicate ready");
            seen[index] = true;
        }
        let mut stopped = retry_rv(|| self.era.stop());
        let epoch = stopped.epoch().get();
        assert_eq!(epoch, self.next_epoch, "reclaim epoch");
        self.next_epoch += 1;
        retry_reg(|| self.era.return_cons_storage(&stopped));
        let report = retry_rv(|| self.era.collect_stopped(&mut stopped));
        assert!(report.major);
        after(self.era, &stopped);
        // Catalog stays on the heap: `finish` refuses a suspended cons catalog.
        complete(stopped);
        for tx in &self.done_txs {
            tx.send(())
                .unwrap_or_else(|_| panic!("done channel closed"));
        }
        for _ in 0..self.go_txs.len() {
            recv(&self.retired_rx);
        }
        ends
    }
}

fn dump_witness(overlap: &Overlap, label: &str) {
    let enter: Vec<_> = (0..overlap.n)
        .map(|index| overlap.seen_enter[index].load(Ordering::SeqCst))
        .collect();
    let alloc: Vec<_> = (0..overlap.n)
        .map(|index| overlap.seen_alloc[index].load(Ordering::SeqCst))
        .collect();
    let progress: Vec<_> = (0..overlap.n)
        .map(|index| overlap.progress[index].0.load(Ordering::SeqCst))
        .collect();
    println!(
        "A6_OVERLAP {label} owners={} seen_enter={enter:?} seen_alloc={alloc:?} progress={progress:?}",
        overlap.n
    );
}

fn assert_owned_conses(heap: &crate::tagged::gc::TaggedHeap, conses: &FxHashSet<usize>) {
    for bits in conses {
        let cell = TaggedValue::from_bits(*bits);
        assert!(cell.is_cons());
        assert!(
            heap.owns_heap_value_for_test(cell),
            "collection dropped a rooted cons {bits:#x}"
        );
    }
}

fn pass_gate(programs: &[Vec<Op>], nslots: usize, bound: u32, started: &AtomicUsize) {
    for program in programs {
        prevalidate(program, nslots, bound, STEP_CAP)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

const LIST_ROUNDS: [(usize, Go); 5] = [
    (2, Go::Forced),
    (4, Go::Threshold),
    (6, Go::Forced),
    (8, Go::Threshold),
    (9, Go::Forced),
];

fn overlap_lists(owners: usize) {
    let started = AtomicUsize::new(0);
    let programs: Vec<_> = (0..owners)
        .map(|index| list_ops(index as i64 * 1_000_000))
        .collect();
    pass_gate(&programs, 11, 8, &started);
    let overlap = Overlap::new(owners, true);
    with_admitted(|era, generational, nil, _one| {
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &overlap.abort,
            };
            let mut session = Session::launch(
                era,
                nil,
                6100,
                &programs,
                11,
                8,
                4,
                list_stop,
                Some(&overlap),
                &overlap.abort,
                scope,
            );
            let mut heads = vec![0usize; owners];
            let mut head_known = vec![false; owners];
            let mut phase9 = vec![FxHashSet::default(); owners];
            for (round, &(count, kind)) in LIST_ROUNDS.iter().enumerate() {
                let snaps = session.pauses();
                if round == 0 {
                    for index in 0..owners {
                        assert_eq!(overlap.seen_enter[index].load(Ordering::SeqCst), owners);
                        assert_eq!(overlap.seen_alloc[index].load(Ordering::SeqCst), owners);
                        assert!(overlap.progress[index].0.load(Ordering::SeqCst) >= 1);
                    }
                    dump_witness(&overlap, "first-stop");
                }
                for snap in &snaps {
                    assert_eq!(snap.cons_count, count);
                    assert_eq!(snap.conses.len(), count);
                    assert_eq!(overlap.progress[snap.index].0.load(Ordering::SeqCst), count);
                    if count < 8 {
                        assert!(snap.cons_count < 8, "unfinished rooted list");
                    }
                    let head = snap.slots[4];
                    assert!(TaggedValue::from_bits(head).is_cons());
                    if head_known[snap.index] {
                        assert_eq!(
                            heads[snap.index], head,
                            "head cons moved across a collection"
                        );
                    } else {
                        heads[snap.index] = head;
                        head_known[snap.index] = true;
                    }
                    if count == 9 {
                        let young = snap.slots[6];
                        assert_ne!(young, head, "old-to-young edge uses a fresh cons");
                        assert!(snap.conses.contains(&young));
                        assert!(TaggedValue::from_bits(young).is_cons());
                        phase9[snap.index] = snap.conses.clone();
                    }
                }
                assert_eq!(
                    heads.iter().collect::<FxHashSet<_>>().len(),
                    owners,
                    "owners built one head each"
                );
                let check = |heap: &crate::tagged::gc::TaggedHeap| {
                    for snap in &snaps {
                        assert_owned_conses(heap, &snap.conses);
                        assert!(
                            heap.owns_heap_value_for_test(TaggedValue::from_bits(snap.slots[4]))
                        );
                    }
                };
                if count == 9 {
                    session.round_with_before(
                        kind,
                        true,
                        |era, stopped| {
                            on_heap(era, stopped, |heap| {
                                for snap in &snaps {
                                    let head = TaggedValue::from_bits(snap.slots[4]);
                                    assert!(heap.owns_heap_value_for_test(head));
                                    if generational {
                                        assert!(
                                            heap.owner_card_is_dirty_for_test(head),
                                            "GEN1 setcar must dirty the old cons card"
                                        );
                                        let old = heap.cons_age_is_old_for_test(head);
                                        println!("A6_GEN1_HEAD_OLD={old}");
                                    }
                                }
                            });
                        },
                        |era, stopped| on_heap(era, stopped, check),
                    );
                } else {
                    session.round(kind, true, |era, stopped| on_heap(era, stopped, check));
                }
            }
            let ends = session.finish_owners(|era, stopped| {
                on_heap(era, stopped, |heap| {
                    let mut cells = 0usize;
                    for set in &phase9 {
                        for bits in set {
                            cells += 1;
                            assert!(
                                !heap.owns_heap_value_for_test(TaggedValue::from_bits(*bits)),
                                "uncleared cons still owned {bits:#x}"
                            );
                        }
                    }
                    println!("A6_RECLAIM owners={owners} cells={cells}");
                    assert_eq!(cells, owners * 9);
                });
            });
            dump_witness(&overlap, "done");
            let oracle = oracle_lines();
            for end in &ends {
                assert_eq!(end.cons_count, 9);
                assert_eq!(end.conses, phase9[end.index]);
                assert_eq!(end.slots[4], heads[end.index]);
                assert_eq!(end.lines.len(), 1);
                assert_eq!(end.lines[0], oracle[9 + end.index]);
                assert_eq!(overlap.progress[end.index].0.load(Ordering::SeqCst), 9);
            }
        });
    });
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

#[test]
fn a6_refuses_excluded_ops_before_run() {
    let started = AtomicUsize::new(0);
    let cases = [
        (Insn::SymbolValue, "symbol"),
        (Insn::Obarray, "obarray"),
        (Insn::Buffer, "buffer"),
        (Insn::HashTable, "hash-table"),
        (Insn::Specbind, "specbind"),
        (Insn::Jit, "jit"),
        (Insn::WorldManager, "world"),
    ];
    for (insn, name) in cases {
        let error = try_admit(&[insn], 1, 0, &[], &started).unwrap_err();
        assert_eq!(error, Reject::Excluded(name));
        assert_eq!(started.load(Ordering::Relaxed), 0);
    }
    let mixed = [Insn::Allowed(Op::ConstNil { dst: 0 }), Insn::SymbolValue];
    assert_eq!(
        try_admit(&mixed, 1, 0, &[], &started).unwrap_err(),
        Reject::Excluded("symbol")
    );
    assert_eq!(started.load(Ordering::Relaxed), 0);
}

#[test]
fn a6_refuses_overflow_and_unbounded_backedge_before_run() {
    let started = AtomicUsize::new(0);
    let overflow = [
        Insn::Allowed(Op::ConstFixnum {
            dst: 0,
            value: TaggedValue::MOST_POSITIVE_FIXNUM,
        }),
        Insn::Allowed(Op::ConstFixnum { dst: 1, value: 1 }),
        Insn::Allowed(Op::Add { dst: 2, a: 0, b: 1 }),
    ];
    assert_eq!(
        try_admit(&overflow, 3, 0, &[], &started).unwrap_err(),
        Reject::Overflow
    );
    let bare = [Insn::Allowed(Op::ConstFixnum {
        dst: 0,
        value: TaggedValue::MOST_POSITIVE_FIXNUM + 1,
    })];
    assert_eq!(
        try_admit(&bare, 1, 0, &[], &started).unwrap_err(),
        Reject::Overflow
    );
    let jump = [Insn::Allowed(Op::Jump { target: 0 })];
    assert_eq!(
        try_admit(&jump, 1, 0, &[], &started).unwrap_err(),
        Reject::Backedge
    );
    assert_eq!(
        try_admit(&jump, 1, u32::MAX, &[], &started).unwrap_err(),
        Reject::Unbounded
    );
    assert_eq!(started.load(Ordering::Relaxed), 0);
}

#[test]
fn a6_refuses_ambiguous_branch_before_run() {
    let started = AtomicUsize::new(0);
    let branch = [
        Insn::Allowed(Op::ConstNil { dst: 1 }),
        Insn::Allowed(Op::ConstNil { dst: 2 }),
        Insn::Allowed(Op::Cons {
            dst: 0,
            car: 1,
            cdr: 2,
        }),
        Insn::Allowed(Op::Car { dst: 3, cell: 0 }),
        Insn::Allowed(Op::BranchIfNil { cond: 3, target: 5 }),
    ];
    assert_eq!(
        try_admit(&branch, 4, 0, &[], &started).unwrap_err(),
        Reject::RejectedType
    );
    let bare = [Insn::Allowed(Op::SetCar { cell: 0, value: 1 })];
    assert_eq!(
        try_admit(&bare, 2, 0, &[], &started).unwrap_err(),
        Reject::RejectedType
    );
    assert_eq!(started.load(Ordering::Relaxed), 0);
}

#[test]
fn a6_runtime_refuses_foreign_cons_without_writing() {
    let started = AtomicUsize::new(0);
    let program = vec![
        Op::ConstFixnum { dst: 1, value: 9 },
        Op::SetCar { cell: 0, value: 1 },
    ];
    // Slot 0 is abstractly a cons, so the gate accepts the store. The runtime
    // owned-set is empty because this cons was not born from `cons_into`.
    prevalidate_with(&program, 2, 0, STEP_CAP, &[(0, Abs::Cons)])
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
    let abort = AtomicBool::new(false);
    with_admitted(|era, _generational, nil, one| {
        let (report_tx, report_rx) = channel();
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &abort,
            };
            let heap = era.heap_identity();
            let registry = era.registry();
            let permit = era
                .register(6400, 2)
                .unwrap_or_else(|error| panic!("register: {error:?}"));
            let (ready_tx, ready_rx) = channel();
            let (done_tx, done_rx) = channel();
            let (retired_tx, retired_rx) = channel();
            let abort_flag = &abort;
            scope.spawn(move || {
                let _panic = PanicFlag {
                    registry,
                    abort: abort_flag,
                };
                let mut owner = activate_owner(permit, heap, 6400);
                let (foreign, slots) = must(with_run(&mut owner, |run| {
                    let foreign = run.cons(one, nil)?;
                    let head = run.root_slot(&foreign)?;
                    let scratch = run.root_slot(nil)?;
                    Ok((foreign, vec![head, scratch]))
                }));
                let mut machine = Machine::new();
                let outcome =
                    one_slice(&mut owner, &slots, &program, &mut machine, 0, None, 0, None);
                let car =
                    TaggedValue::from_bits(foreign.cons_word_for_test()).cons_car_unobserved();
                report_tx
                    .send((outcome.err(), car.as_fixnum()))
                    .unwrap_or_else(|_| panic!("report channel closed"));
                drop(foreign);
                shutdown_owner(owner, 0, &ready_tx, &done_rx, &retired_tx);
            });
            let (error, car) = recv(&report_rx);
            assert_eq!(error, Some(ExecError::ForeignCons));
            assert_eq!(car, Some(1));
            assert_eq!(recv(&ready_rx), 0);
            let mut next_epoch = 1u64;
            let mut stopped = retry_rv(|| era.stop());
            assert_eq!(stopped.epoch().get(), next_epoch);
            next_epoch += 1;
            let _ = next_epoch;
            retry_reg(|| era.return_cons_storage(&stopped));
            let report = retry_rv(|| era.collect_stopped(&mut stopped));
            assert!(report.major);
            complete(stopped);
            done_tx.send(()).unwrap();
            assert_eq!(recv(&retired_rx), 0);
        });
    });
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

#[test]
fn a6_arithmetic_matches_gnu_oracle() {
    let started = AtomicUsize::new(0);
    let programs = arithmetic_ops();
    pass_gate(&programs, 4, 0, &started);
    let abort = AtomicBool::new(false);
    let oracle = oracle_lines();
    with_admitted(|era, _generational, nil, _one| {
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &abort,
            };
            let heap = era.heap_identity();
            let registry = era.registry();
            let permit = era
                .register(6300, 4)
                .unwrap_or_else(|error| panic!("register: {error:?}"));
            let (end_tx, end_rx) = channel();
            let (ready_tx, ready_rx) = channel();
            let (done_tx, done_rx) = channel();
            let (retired_tx, retired_rx) = channel();
            let (pause_tx, _pause_rx) = channel();
            let (_go_tx, go_rx) = channel();
            let owned_programs = programs.clone();
            let abort_flag = &abort;
            scope.spawn(move || {
                OwnerLaunch {
                    permit,
                    heap,
                    id: 6300,
                    nil,
                    programs: owned_programs,
                    nslots: 4,
                    bound: 0,
                    line_slot: 0,
                    stop_at: no_stop,
                    overlap: None,
                    index: 0,
                    pause_tx,
                    go_rx,
                    end_tx,
                    ready_tx,
                    done_rx,
                    retired_tx,
                    registry,
                    abort: abort_flag,
                    planted: &[],
                    ht: None,
                }
                .run()
            });
            let end = recv(&end_rx);
            assert_eq!(end.lines, oracle[..8]);
            assert_eq!(recv(&ready_rx), 0);
            let mut stopped = retry_rv(|| era.stop());
            assert_eq!(stopped.epoch().get(), 1);
            retry_reg(|| era.return_cons_storage(&stopped));
            assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
            complete(stopped);
            done_tx.send(()).unwrap();
            assert_eq!(recv(&retired_rx), 0);
        });
    });
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

// ---------------------------------------------------------------------------
// B3 tests: sealed single-owner hash-table ops against GNU 31.1 oracles
// ---------------------------------------------------------------------------

/// B3 owner-slot map (design §6): slots 0-8 tables, 9-10 bare symbols,
/// 11-13 callback scratch, 14 the line slot, 15-19 program scratch.
const HT_TA: u16 = 0;
const HT_TB: u16 = 1;
const HT_TC: u16 = 2;
const HT_TD: u16 = 3;
const HT_TE: u16 = 4;
const HT_TG: u16 = 5;
const HT_TU: u16 = 6;
const HT_TO: u16 = 7;
const HT_TO2: u16 = 8;
const HT_SYM_A: u16 = 9;
const HT_SYM_B: u16 = 10;
const HT_CB_KEY: u16 = 11;
const HT_CB_CAND: u16 = 12;
const HT_CB_OUT: u16 = 13;
const HT_LINE: u16 = 14;
const HT_NSLOTS: usize = 20;

/// Cons the already-loaded element words onto the line slot in reverse, so
/// the finished list reads in the order given.
fn finish_line(elements: &[u16]) -> Vec<Op> {
    let mut ops = vec![Op::ConstNil { dst: HT_LINE }];
    for &slot in elements.iter().rev() {
        ops.push(Op::Cons {
            dst: HT_LINE,
            car: slot,
            cdr: HT_LINE,
        });
    }
    ops
}

/// The setup the oracle twin mirrors table-for-table: five plain tables, a
/// growth table, two user-test tables (pure and other-mutating), and the
/// plain table the other-mutating comparison legally edits.
fn ht_main_setup() -> HtSetup {
    HtSetup {
        source: r#"(progn
  (setq a6-tA (make-hash-table :test 'eq))
  (setq a6-tB (make-hash-table :test 'eq))
  (setq a6-tC (make-hash-table :test 'eql))
  (setq a6-tD (make-hash-table :test 'equal))
  (setq a6-tE (make-hash-table :test 'eq))
  (setq a6-tG (make-hash-table :test 'eq))
  (fset 'a6-user-hash (lambda (x) x))
  (fset 'a6-user-cmp (lambda (a b) (eq a b)))
  (define-hash-table-test 'a6-user-test 'a6-user-cmp 'a6-user-hash)
  (setq a6-tU (make-hash-table :test 'a6-user-test))
  (fset 'a6-user-hash2 (lambda (x) 7))
  (fset 'a6-user-cmp-other (lambda (a b) (puthash 5 55 a6-tO2) (eq a b)))
  (define-hash-table-test 'a6-user-test-other 'a6-user-cmp-other 'a6-user-hash2)
  (setq a6-tO (make-hash-table :test 'a6-user-test-other))
  (setq a6-tO2 (make-hash-table :test 'eq))
  (list a6-tA a6-tB a6-tC a6-tD a6-tE a6-tG a6-tU a6-tO a6-tO2
        'a6-symA 'a6-symB))"#,
        facts: vec![
            (HT_TA, Abs::Table),
            (HT_TB, Abs::Table),
            (HT_TC, Abs::Table),
            (HT_TD, Abs::Table),
            (HT_TE, Abs::Table),
            (HT_TG, Abs::Table),
            (HT_TU, Abs::Table),
            (HT_TO, Abs::Table),
            (HT_TO2, Abs::Table),
            (HT_SYM_A, Abs::Sym),
            (HT_SYM_B, Abs::Sym),
        ],
        nslots: HT_NSLOTS,
    }
}

/// P1: eq table, fixnum keys — put, hit, miss, count → (t 100 nil nil 2).
fn ht_p1() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 10 },
        Op::ConstFixnum {
            dst: 16,
            value: 100,
        },
        Op::HtPut {
            table: HT_TA,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 20 },
        Op::ConstFixnum {
            dst: 16,
            value: 200,
        },
        Op::HtPut {
            table: HT_TA,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 10 },
        Op::HtGet {
            table: HT_TA,
            key: 15,
            found: 16,
            value: 17,
        },
        Op::ConstFixnum { dst: 15, value: 30 },
        Op::HtGet {
            table: HT_TA,
            key: 15,
            found: 18,
            value: 19,
        },
        Op::HtCount {
            table: HT_TA,
            dst: 15,
        },
    ];
    ops.extend(finish_line(&[16, 17, 18, 19, 15]));
    ops
}

/// P2: eq table, bare-symbol keys → (t 1 nil nil 2).
fn ht_p2() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 16, value: 1 },
        Op::HtPut {
            table: HT_TB,
            key: HT_SYM_A,
            value: 16,
        },
        Op::ConstFixnum { dst: 16, value: 2 },
        Op::HtPut {
            table: HT_TB,
            key: HT_SYM_B,
            value: 16,
        },
        Op::HtGet {
            table: HT_TB,
            key: HT_SYM_A,
            found: 16,
            value: 17,
        },
        Op::ConstFixnum { dst: 15, value: 99 },
        Op::HtGet {
            table: HT_TB,
            key: 15,
            found: 18,
            value: 19,
        },
        Op::HtCount {
            table: HT_TB,
            dst: 15,
        },
    ];
    ops.extend(finish_line(&[16, 17, 18, 19, 15]));
    ops
}

/// P3: eql table → (t 1000 1).
fn ht_p3() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 10 },
        Op::ConstFixnum {
            dst: 16,
            value: 1000,
        },
        Op::HtPut {
            table: HT_TC,
            key: 15,
            value: 16,
        },
        Op::HtGet {
            table: HT_TC,
            key: 15,
            found: 18,
            value: 19,
        },
        Op::HtCount {
            table: HT_TC,
            dst: 16,
        },
    ];
    ops.extend(finish_line(&[18, 19, 16]));
    ops
}

/// P4: equal table, two structurally-equal fresh cons keys → (t 5 1).
fn ht_p4() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 1 },
        Op::ConstFixnum { dst: 16, value: 2 },
        Op::Cons {
            dst: 17,
            car: 15,
            cdr: 16,
        },
        Op::ConstFixnum { dst: 18, value: 5 },
        Op::HtPut {
            table: HT_TD,
            key: 17,
            value: 18,
        },
        Op::ConstFixnum { dst: 15, value: 1 },
        Op::ConstFixnum { dst: 16, value: 2 },
        Op::Cons {
            dst: 19,
            car: 15,
            cdr: 16,
        },
        Op::HtGet {
            table: HT_TD,
            key: 19,
            found: 15,
            value: 16,
        },
        Op::HtCount {
            table: HT_TD,
            dst: 17,
        },
    ];
    ops.extend(finish_line(&[15, 16, 17]));
    ops
}

/// P5: rem removes once, refuses twice, one entry survives → (t nil 1);
/// GNU's `remhash` returns nil unconditionally, so the twin derives the
/// found flags from presence checks instead of return values.
fn ht_p5() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 1 },
        Op::ConstFixnum { dst: 16, value: 11 },
        Op::HtPut {
            table: HT_TE,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 2 },
        Op::ConstFixnum { dst: 16, value: 22 },
        Op::HtPut {
            table: HT_TE,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 1 },
        Op::HtRem {
            table: HT_TE,
            key: 15,
            found: 17,
        },
        Op::HtRem {
            table: HT_TE,
            key: 15,
            found: 18,
        },
        Op::HtCount {
            table: HT_TE,
            dst: 16,
        },
    ];
    ops.extend(finish_line(&[17, 18, 16]));
    ops
}

/// P6: 65 puts grow the table through several resizes → (65 128).
fn ht_p6() -> Vec<Op> {
    // 15 one, 16 value/scratch, 17 limit, 18 k, 19 cond
    let mut ops = vec![
        Op::ConstFixnum { dst: 17, value: 65 },
        Op::ConstFixnum { dst: 18, value: 0 },
        Op::ConstFixnum { dst: 15, value: 1 },
    ];
    let loop_at = ops.len() as u16;
    ops.push(Op::Lt {
        dst: 19,
        a: 18,
        b: 17,
    });
    let branch_at = ops.len();
    ops.push(Op::BranchIfNil {
        cond: 19,
        target: 0,
    });
    ops.push(Op::Add {
        dst: 16,
        a: 18,
        b: 18,
    });
    ops.push(Op::HtPut {
        table: HT_TG,
        key: 18,
        value: 16,
    });
    ops.push(Op::Add {
        dst: 18,
        a: 18,
        b: 15,
    });
    ops.push(Op::Jump { target: loop_at });
    let done = ops.len() as u16;
    ops.push(Op::HtCount {
        table: HT_TG,
        dst: 16,
    });
    ops.push(Op::ConstFixnum { dst: 17, value: 64 });
    ops.push(Op::HtGet {
        table: HT_TG,
        key: 17,
        found: 18,
        value: 19,
    });
    ops[branch_at] = Op::BranchIfNil {
        cond: 19,
        target: done,
    };
    ops.extend(finish_line(&[16, 19]));
    ops
}

/// P7: pure user-test table — put, hit, miss, count → (t 8 nil nil 2).
fn ht_p7() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 7 },
        Op::ConstFixnum { dst: 16, value: 8 },
        Op::HtPut {
            table: HT_TU,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 8 },
        Op::ConstFixnum { dst: 16, value: 9 },
        Op::HtPut {
            table: HT_TU,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 7 },
        Op::HtGet {
            table: HT_TU,
            key: 15,
            found: 16,
            value: 17,
        },
        Op::ConstFixnum { dst: 15, value: 30 },
        Op::HtGet {
            table: HT_TU,
            key: 15,
            found: 18,
            value: 19,
        },
        Op::HtCount {
            table: HT_TU,
            dst: 15,
        },
    ];
    ops.extend(finish_line(&[16, 17, 18, 19, 15]));
    ops
}

/// P8s: a non-EQ lookup (constant hash) forces the user comparison, which
/// legally mutates a different table → (nil nil 1). The EQ fast path (GNU
/// fns.c `hash_find_with_hash`) never calls the comparator for identical
/// objects, so exercising it needs distinct keys with one hash.
fn ht_p8s() -> Vec<Op> {
    let mut ops = vec![
        Op::ConstFixnum { dst: 15, value: 5 },
        Op::ConstFixnum { dst: 16, value: 50 },
        Op::HtPut {
            table: HT_TO,
            key: 15,
            value: 16,
        },
        Op::ConstFixnum { dst: 15, value: 9 },
        Op::HtGet {
            table: HT_TO,
            key: 15,
            found: 17,
            value: 18,
        },
        Op::HtCount {
            table: HT_TO2,
            dst: 16,
        },
    ];
    ops.extend(finish_line(&[17, 18, 16]));
    ops
}

/// One B3 single-owner session over planted tables: eval the setup in the
/// serial world, run the programs through the restricted executor inside
/// the era, then eval the dump forms once the serial world is back. The
/// caller owns the [`HtEnv`] so its observation counters survive the run.
/// Returns the program lines followed by the dump lines.
fn run_ht_session(
    setup: &HtSetup,
    programs: Vec<Vec<Op>>,
    bound: u32,
    env: &HtEnv<'_>,
    dump_forms: &[&str],
) -> (Vec<String>, Vec<String>) {
    let mut context = Context::new();
    let plant_values = context
        .eval_str(setup.source)
        .unwrap_or_else(|error| panic!("ht setup eval: {error:?}"));
    // SAFETY: every planted word belongs to this Context's heap; the roots
    // keep the tables and symbols alive for the whole era.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let mut planted: Vec<Option<SharedRoot>> = vec![None; setup.nslots];
    {
        let mut cursor = plant_values;
        for &(slot, _fact) in &setup.facts {
            assert!(cursor.is_cons(), "setup list shorter than its facts");
            // SAFETY: same heap as the context above.
            let root =
                unsafe { SharedRoot::new(&context.tagged_heap, cursor.cons_car_unobserved()) };
            planted[usize::from(slot)] = Some(root);
            cursor = cursor.cons_cdr_unobserved();
        }
        assert!(cursor.is_nil(), "setup list longer than its facts");
    }
    let abort = AtomicBool::new(false);
    let mut lines: Vec<String> = Vec::new();
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let heap = era.heap_identity();
                    let registry = era.registry();
                    let permit = retry_reg(|| era.register(6600, setup.nslots));
                    let (end_tx, end_rx) = channel();
                    let (ready_tx, ready_rx) = channel();
                    let (done_tx, done_rx) = channel();
                    let (retired_tx, retired_rx) = channel();
                    let (pause_tx, _pause_rx) = channel();
                    let (_go_tx, go_rx) = channel();
                    let launch = OwnerLaunch {
                        permit,
                        heap,
                        id: 6600,
                        nil: &nil,
                        programs,
                        nslots: setup.nslots,
                        bound,
                        line_slot: usize::from(HT_LINE),
                        stop_at: no_stop,
                        overlap: None,
                        index: 0,
                        pause_tx,
                        go_rx,
                        end_tx,
                        ready_tx,
                        done_rx,
                        retired_tx,
                        registry,
                        abort: &abort,
                        planted: &planted,
                        ht: Some(env),
                    };
                    scope.spawn(move || launch.run());
                    let end = recv(&end_rx);
                    assert_eq!(recv(&ready_rx), 0);
                    let mut stopped = retry_rv(|| era.stop());
                    assert_eq!(stopped.epoch().get(), 1);
                    retry_reg(|| era.return_cons_storage(&stopped));
                    assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                    complete(stopped);
                    done_tx.send(()).unwrap();
                    assert_eq!(recv(&retired_rx), 0);
                    lines = end.lines;
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    let dumps = dump_forms
        .iter()
        .map(|form| {
            let value = context
                .eval_str(form)
                .unwrap_or_else(|error| panic!("ht dump eval {form}: {error:?}"));
            prin1(value.bits())
        })
        .collect();
    (lines, dumps)
}

#[test]
fn a6_ht_ops_match_gnu_oracle() {
    let started = AtomicUsize::new(0);
    let programs = vec![
        ht_p1(),
        ht_p2(),
        ht_p3(),
        ht_p4(),
        ht_p5(),
        ht_p6(),
        ht_p7(),
        ht_p8s(),
    ];
    let setup = ht_main_setup();
    let initial = setup.initial();
    // The gate runs before any owner: straight-line programs admit with no
    // backedge budget, the growth loop with exactly its 65.
    for program in &programs[..5] {
        prevalidate_with(program, HT_NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    prevalidate_with(&programs[5], HT_NSLOTS, 65, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate p6: {error:?}"));
    for program in &programs[6..] {
        prevalidate_with(program, HT_NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    // Callback programs are gated too: the pure and other-mutating
    // comparisons, the identity hash, and the constant hash that makes the
    // comparison reachable past the EQ fast path.
    let hash_prog = [Op::Move {
        dst: HT_CB_OUT,
        src: HT_CB_KEY,
    }];
    let hash2_prog = [Op::ConstFixnum {
        dst: HT_CB_OUT,
        value: 7,
    }];
    let cmp_prog = [Op::Eq {
        dst: HT_CB_OUT,
        a: HT_CB_KEY,
        b: HT_CB_CAND,
    }];
    // (puthash 5 55 a6-tO2) then the comparison, mirroring the twin's
    // a6-user-cmp-other body exactly.
    let cmp_other = [
        Op::ConstFixnum { dst: 15, value: 5 },
        Op::ConstFixnum { dst: 16, value: 55 },
        Op::HtPut {
            table: HT_TO2,
            key: 15,
            value: 16,
        },
        Op::Eq {
            dst: HT_CB_OUT,
            a: HT_CB_KEY,
            b: HT_CB_CAND,
        },
    ];
    for ops in [
        &hash_prog[..],
        &hash2_prog[..],
        &cmp_prog[..],
        &cmp_other[..],
    ] {
        let mut facts = initial.clone();
        facts.extend([(HT_CB_KEY, Abs::Fix(0)), (HT_CB_CAND, Abs::Fix(0))]);
        prevalidate_with(ops, HT_NSLOTS, 0, STEP_CAP, &facts)
            .unwrap_or_else(|error| panic!("callback prevalidate: {error:?}"));
    }
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
    let users = [
        UserTestProg {
            slot: HT_TU,
            hash_ops: &hash_prog,
            cmp_ops: &cmp_prog,
        },
        UserTestProg {
            slot: HT_TO,
            hash_ops: &hash2_prog,
            cmp_ops: &cmp_other,
        },
    ];
    let env = HtEnv::new(&users, HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let dump_forms = [
        "(internal--hash-table-buckets a6-tA)",
        // EQ symbols hash by runtime identity, not their names. GNU's symbol
        // addresses and our interner IDs need not give the same collisions.
        // Compare the count and sorted contents instead of bucket geometry.
        r#"(let ((entries nil))
  (maphash (lambda (key value)
             (setq entries (cons (cons (symbol-name key) value) entries))) a6-tB)
  (list (hash-table-count a6-tB)
        (sort entries (lambda (a b)
                        (string-lessp (car a) (car b))))))"#,
        "(internal--hash-table-buckets a6-tD)",
        "(internal--hash-table-buckets a6-tE)",
        "(internal--hash-table-histogram a6-tG)",
        "(internal--hash-table-buckets a6-tU)",
    ];
    let (lines, dumps) = run_ht_session(&setup, programs, 65, &env, &dump_forms);
    assert_eq!(lines.len(), 8);
    assert_eq!(dumps.len(), 6);
    let oracle = oracle_lines();
    // Program lines and structural dumps interleave exactly as the twin
    // emits them (oracle lines 13..27; the 28th is the P7b refusal test's).
    assert_eq!(lines[0], oracle[13], "P1 eq fixnum");
    assert_eq!(dumps[0], oracle[14], "tA buckets");
    assert_eq!(lines[1], oracle[15], "P2 eq symbol");
    assert_eq!(dumps[1], oracle[16], "tB count and contents");
    assert_eq!(lines[2], oracle[17], "P3 eql");
    assert_eq!(lines[3], oracle[18], "P4 equal cons");
    assert_eq!(dumps[2], oracle[19], "tD buckets");
    assert_eq!(lines[4], oracle[20], "P5 rem");
    assert_eq!(dumps[3], oracle[21], "tE buckets after rems");
    assert_eq!(lines[5], oracle[22], "P6 growth");
    assert_eq!(dumps[4], oracle[23], "tG histogram");
    assert_eq!(lines[6], oracle[24], "P7 user pure");
    assert_eq!(dumps[5], oracle[25], "tU buckets");
    assert_eq!(lines[7], oracle[26], "P8s user other-table");
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

/// P7b: a comparison that edits the table under test must be refused with
/// GNU's message, and the table must be untouched.
#[test]
fn a6_ht_user_test_refuses_mutation_inside_the_test() {
    let started = AtomicUsize::new(0);
    // 0 tM, 1-3 callback scratch, 4-6 operands.
    const TM: u16 = 0;
    let program = vec![
        Op::ConstFixnum { dst: 4, value: 42 },
        Op::ConstFixnum { dst: 5, value: 1 },
        Op::HtPut {
            table: TM,
            key: 4,
            value: 5,
        },
        // A different key with the same (constant) hash, so the comparator
        // really runs — the EQ fast path would otherwise skip it.
        Op::ConstFixnum { dst: 4, value: 43 },
        Op::HtGet {
            table: TM,
            key: 4,
            found: 5,
            value: 6,
        },
    ];
    let cmp_bad = [
        Op::ConstFixnum { dst: 4, value: 99 },
        Op::ConstFixnum { dst: 5, value: 9 },
        Op::HtPut {
            table: TM,
            key: 4,
            value: 5,
        },
        Op::Eq { dst: 3, a: 1, b: 2 },
    ];
    let hash_prog = [Op::ConstFixnum { dst: 3, value: 7 }];
    let initial = [(TM, Abs::Table), (1, Abs::Fix(0)), (2, Abs::Fix(0))];
    prevalidate_with(&program, 7, 0, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    for ops in [&hash_prog[..], &cmp_bad[..]] {
        prevalidate_with(ops, 7, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("callback prevalidate: {error:?}"));
    }
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
    let setup = HtSetup {
        source: r#"(progn
  (fset 'a6-user-hash2 (lambda (x) 7))
  (fset 'a6-user-cmp-bad (lambda (a b) (puthash 99 9 a6-tM) (eq a b)))
  (define-hash-table-test 'a6-user-test-bad 'a6-user-cmp-bad 'a6-user-hash2)
  (setq a6-tM (make-hash-table :test 'a6-user-test-bad))
  (list a6-tM))"#,
        facts: vec![(TM, Abs::Table)],
        nslots: 7,
    };
    let users = [UserTestProg {
        slot: TM,
        hash_ops: &hash_prog,
        cmp_ops: &cmp_bad,
    }];
    let mut context = Context::new();
    let plant_values = context
        .eval_str(setup.source)
        .unwrap_or_else(|error| panic!("ht setup eval: {error:?}"));
    // The setup form returns its one table wrapped in a list; the planted
    // slot takes the table itself, exactly as `run_ht_session` unwraps its
    // setup lists (a cons in the slot would fail the lease mint's type check).
    let table_value = plant_values.cons_car_unobserved();
    // SAFETY: the planted table belongs to this Context's heap.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let table_root = unsafe { SharedRoot::new(&context.tagged_heap, table_value) };
    let mut planted: Vec<Option<SharedRoot>> = vec![None; setup.nslots];
    planted[0] = Some(table_root);
    let env = HtEnv::new(&users, 1, 2, 3);
    let abort = AtomicBool::new(false);
    let (report_tx, report_rx) = channel();
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let heap = era.heap_identity();
                    let registry = era.registry();
                    let permit = retry_reg(|| era.register(6601, setup.nslots));
                    let abort_flag = &abort;
                    let env_ref = &env;
                    let planted_ref = &planted;
                    let nil_ref = &nil;
                    let program_ref = &program;
                    let (ready_tx, ready_rx) = channel();
                    let (done_tx, done_rx) = channel();
                    let (retired_tx, retired_rx) = channel();
                    scope.spawn(move || {
                        let _panic = PanicFlag {
                            registry,
                            abort: abort_flag,
                        };
                        let mut owner = activate_owner(permit, heap, 6601);
                        let slots = seed_planted(&mut owner, nil_ref, planted_ref);
                        let mut machine = Machine::new();
                        let outcome = one_slice(
                            &mut owner,
                            &slots,
                            program_ref,
                            &mut machine,
                            0,
                            None,
                            0,
                            Some(env_ref),
                        );
                        report_tx
                            .send(outcome.err())
                            .unwrap_or_else(|_| panic!("report channel closed"));
                        shutdown_owner(owner, 0, &ready_tx, &done_rx, &retired_tx);
                    });
                    let error = recv(&report_rx);
                    assert_eq!(error, Some(ExecError::UserTestMutation));
                    assert_eq!(
                        started.load(Ordering::Relaxed),
                        1,
                        "the refusal is a runtime guard, not a gate rejection"
                    );
                    assert_eq!(recv(&ready_rx), 0);
                    let mut next_epoch = 1u64;
                    let mut stopped = retry_rv(|| era.stop());
                    assert_eq!(stopped.epoch().get(), next_epoch);
                    next_epoch += 1;
                    let _ = next_epoch;
                    retry_reg(|| era.return_cons_storage(&stopped));
                    assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                    complete(stopped);
                    done_tx.send(()).unwrap();
                    assert_eq!(recv(&retired_rx), 0);
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    // Serial world back: the refused put left the table exactly as seeded.
    let count = context
        .eval_str("(hash-table-count a6-tM)")
        .unwrap_or_else(|error| panic!("count eval: {error:?}"));
    assert_eq!(prin1(count.bits()), "1");
    let oracle = oracle_lines();
    assert_eq!(oracle[27], "\"hash table test modifies table\"");
}

/// P8b: the forced-bump lever. A legal writer edit lands inside a comparison
/// window with nothing held; the epoch moves, the scan revalidates, and the
/// second round answers — same line as the stable twin (the P8s oracle
/// line). Only the tO comparison qualifies: the EQ fast path means the pure
/// tU lookups never reach a comparator window at all.
#[test]
fn a6_ht_user_test_revalidates_after_callback_window() {
    let started = AtomicUsize::new(0);
    let program = ht_p8s();
    let setup = ht_main_setup();
    let initial = setup.initial();
    prevalidate_with(&program, HT_NSLOTS, 0, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let hash_prog = [Op::Move {
        dst: HT_CB_OUT,
        src: HT_CB_KEY,
    }];
    let hash2_prog = [Op::ConstFixnum {
        dst: HT_CB_OUT,
        value: 7,
    }];
    let cmp_prog = [Op::Eq {
        dst: HT_CB_OUT,
        a: HT_CB_KEY,
        b: HT_CB_CAND,
    }];
    let cmp_other = [
        Op::ConstFixnum { dst: 15, value: 5 },
        Op::ConstFixnum { dst: 16, value: 55 },
        Op::HtPut {
            table: HT_TO2,
            key: 15,
            value: 16,
        },
        Op::Eq {
            dst: HT_CB_OUT,
            a: HT_CB_KEY,
            b: HT_CB_CAND,
        },
    ];
    let mut facts = initial;
    facts.extend([(HT_CB_KEY, Abs::Fix(0)), (HT_CB_CAND, Abs::Fix(0))]);
    for ops in [
        &hash_prog[..],
        &hash2_prog[..],
        &cmp_prog[..],
        &cmp_other[..],
    ] {
        prevalidate_with(ops, HT_NSLOTS, 0, STEP_CAP, &facts)
            .unwrap_or_else(|error| panic!("callback prevalidate: {error:?}"));
    }
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
    let users = [
        UserTestProg {
            slot: HT_TU,
            hash_ops: &hash_prog,
            cmp_ops: &cmp_prog,
        },
        UserTestProg {
            slot: HT_TO,
            hash_ops: &hash2_prog,
            cmp_ops: &cmp_other,
        },
    ];
    let env = HtEnv::new(&users, HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.bump_before_cb.store(1, Ordering::SeqCst);
    let (lines, _dumps) = run_ht_session(&setup, vec![program], 0, &env, &[]);
    assert_eq!(
        env.revalidations.load(Ordering::SeqCst),
        1,
        "the bumped window retries exactly once and then settles"
    );
    let oracle = oracle_lines();
    assert_eq!(lines[0], oracle[26], "same answer as the stable twin");
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

/// The lease-mint runtime tests B2 deferred: both lease kinds minted from a
/// live run access on the owner's thread, the authority word's live
/// transitions matching the B1 model, and one edit read back.
#[test]
fn a6_ht_lease_mints_from_a_live_run_access() {
    const WRITER: u64 = 1 << 63;
    let abort = AtomicBool::new(false);
    let mut context = Context::new();
    let table = context
        .eval_str("(make-hash-table :test 'eq)")
        .unwrap_or_else(|error| panic!("table eval: {error:?}"));
    // SAFETY: the table belongs to this Context's heap; the root keeps it
    // alive for the whole era.
    let root = unsafe { SharedRoot::new(&context.tagged_heap, table) };
    let (report_tx, report_rx) = channel();
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let heap = era.heap_identity();
                    let registry = era.registry();
                    let permit = retry_reg(|| era.register(6602, 1));
                    let abort_flag = &abort;
                    let root = root.clone();
                    let (ready_tx, ready_rx) = channel();
                    let (done_tx, done_rx) = channel();
                    let (retired_tx, retired_rx) = channel();
                    scope.spawn(move || {
                        let _panic = PanicFlag {
                            registry,
                            abort: abort_flag,
                        };
                        let mut owner = activate_owner(permit, heap, 6602);
                        let report = must(with_run(&mut owner, |run| {
                            let table = TaggedValue::from_bits(root.cons_word_for_test());
                            let key = TaggedValue::fixnum(5);
                            let value = TaggedValue::fixnum(6);
                            // Readers nest as a count and never set the bit;
                            // both leases stay mintable from this one access.
                            let r1 = TableRef::enter(run, table)?;
                            let w1 = r1.authority_word().load(Ordering::Acquire);
                            let r2 = TableRef::enter(run, table)?;
                            let w2 = r2.authority_word().load(Ordering::Acquire);
                            drop(r2);
                            let w3 = r1.authority_word().load(Ordering::Acquire);
                            drop(r1);
                            // The writer drains the count and holds the whole
                            // word; entering right after the last reader left
                            // also proves the word returned to quiescent.
                            let mut writer = TableMut::enter(run, table)?;
                            let w4 = writer.authority_word().load(Ordering::Acquire);
                            writer.edit(|ht| {
                                let probe = ht
                                    .data
                                    .try_probe_for_insert(key, HashTableTest::Eq, false)
                                    .expect("an empty eq table probes cleanly");
                                let HashProbe::Absent(hash) = probe else {
                                    panic!("fresh table has no matching entry");
                                };
                                maybe_resize_hash_table_for_insert(ht, true);
                                let hash_key = key.to_hash_key_swp(&HashTableTest::Eq, false);
                                ht.data.insert_absent(hash, hash_key, key, value);
                            });
                            drop(writer);
                            let reader = TableRef::enter(run, table)?;
                            let seen = reader.read(|s| {
                                (
                                    s.len(),
                                    s.try_lookup(key, HashTableTest::Eq, false)
                                        .ok()
                                        .flatten()
                                        .map(|found| found.bits()),
                                )
                            });
                            drop(reader);
                            Ok((w1, w2, w3, w4, seen))
                        }));
                        report_tx
                            .send(report)
                            .unwrap_or_else(|_| panic!("report channel closed"));
                        shutdown_owner(owner, 0, &ready_tx, &done_rx, &retired_tx);
                    });
                    let (w1, w2, w3, w4, (len, found)) = recv(&report_rx);
                    assert_eq!(w1, 1, "the first reader takes count 1");
                    assert_eq!(w2, 2, "readers nest as a count, never the bit");
                    assert_eq!(w3, 1, "dropping a nested reader decrements");
                    assert_eq!(w4, WRITER, "the writer holds the whole word");
                    assert_eq!(len, 1, "the sealed edit inserted one entry");
                    assert_eq!(
                        found,
                        Some(TaggedValue::fixnum(6).bits()),
                        "the read-back sees the edit"
                    );
                    assert_eq!(recv(&ready_rx), 0);
                    let mut stopped = retry_rv(|| era.stop());
                    assert_eq!(stopped.epoch().get(), 1);
                    retry_reg(|| era.return_cons_storage(&stopped));
                    assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                    complete(stopped);
                    done_tx.send(()).unwrap();
                    assert_eq!(recv(&retired_rx), 0);
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
}

#[test]
fn a6_nonallocating_sum_loop() {
    let started = AtomicUsize::new(0);
    let programs = vec![sum_ops(), sum_ops()];
    pass_gate(&programs, 4, 5, &started);
    let overlap = Overlap::new(2, false);
    let oracle = oracle_lines();
    with_admitted(|era, _generational, nil, _one| {
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &overlap.abort,
            };
            let mut session = Session::launch(
                era,
                nil,
                6200,
                &programs,
                4,
                5,
                1,
                sum_stop,
                Some(&overlap),
                &overlap.abort,
                scope,
            );
            let snaps = session.pauses();
            for index in 0..2 {
                assert_eq!(overlap.seen_enter[index].load(Ordering::SeqCst), 2);
                assert_eq!(overlap.seen_alloc[index].load(Ordering::SeqCst), 0);
            }
            dump_witness(&overlap, "sum-enter");
            for snap in &snaps {
                assert_eq!(snap.safepoints, 1);
                assert_eq!(snap.cons_count, 0);
                assert!(snap.conses.is_empty());
                assert_eq!(
                    TaggedValue::from_bits(snap.slots[0]).as_fixnum(),
                    Some(4),
                    "mid-loop n"
                );
                assert_eq!(
                    TaggedValue::from_bits(snap.slots[1]).as_fixnum(),
                    Some(5),
                    "mid-loop s"
                );
            }
            session.round(Go::Forced, true, |_era, _stopped| {});
            let ends = session.finish_owners(|_era, _stopped| {});
            for end in &ends {
                assert_eq!(end.safepoints, 5);
                assert_eq!(end.cons_count, 0);
                assert_eq!(end.lines, vec![oracle[8]]);
            }
        });
    });
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

#[test]
fn a6_overlap_lists_two_owners() {
    overlap_lists(2);
}

#[test]
fn a6_overlap_lists_four_owners() {
    overlap_lists(4);
}

// ---------------------------------------------------------------------------
// A8 throughput rows (design section 7.2)
// ---------------------------------------------------------------------------

/// Owner-cons threshold for the throughput rows' requested collections; 0
/// disables every stop (the allocation-only row).
static TP_THRESHOLD: AtomicUsize = AtomicUsize::new(0);

fn tp_stop(cons_count: usize, _safepoints: usize) -> Option<Go> {
    let threshold = TP_THRESHOLD.load(Ordering::Relaxed);
    if threshold > 0 && cons_count > 0 && cons_count % threshold == 0 {
        Some(Go::Threshold)
    } else {
        None
    }
}

struct TpRow {
    label: String,
    owners: usize,
    cells_per_owner: usize,
    gcs: usize,
    wall_ns: u64,
    pause_ns: Vec<u64>,
    live_bytes_last: usize,
    live_bytes_max: usize,
    per_owner_conses: Vec<usize>,
    witness_enter: Vec<usize>,
    witness_alloc: Vec<usize>,
    grant_calls: u64,
    grant_ns: u64,
    grant_max_ns: u64,
    close_calls: u64,
    close_ns: u64,
    close_max_ns: u64,
}

fn reset_tp_catalog_waits() {
    use std::sync::atomic::Ordering::Relaxed;
    let waits = &crate::tagged::gc::cons_storage::CATALOG_WAITS;
    waits.grant_calls.store(0, Relaxed);
    waits.grant_ns.store(0, Relaxed);
    waits.grant_max_ns.store(0, Relaxed);
    waits.close_calls.store(0, Relaxed);
    waits.close_ns.store(0, Relaxed);
    waits.close_max_ns.store(0, Relaxed);
}

fn read_tp_catalog_waits() -> (u64, u64, u64, u64, u64, u64) {
    use std::sync::atomic::Ordering::Relaxed;
    let waits = &crate::tagged::gc::cons_storage::CATALOG_WAITS;
    (
        waits.grant_calls.load(Relaxed),
        waits.grant_ns.load(Relaxed),
        waits.grant_max_ns.load(Relaxed),
        waits.close_calls.load(Relaxed),
        waits.close_ns.load(Relaxed),
        waits.close_max_ns.load(Relaxed),
    )
}

/// One timed round: every owner is parked at its threshold safepoint before
/// the window opens, and the window closes when `complete` returns.
fn tp_round(session: &mut Session<'_, '_>) -> (usize, u64) {
    let _ = session.pauses();
    let t0 = Instant::now();
    let mut stopped = open_collection(
        session.era,
        &session.go_txs,
        Go::Threshold,
        &mut session.next_epoch,
    );
    let report = retry_rv(|| session.era.collect_stopped(&mut stopped));
    assert!(report.major, "throughput rounds collect a major cycle");
    retry_reg(|| session.era.resume_cons_storage(&stopped));
    complete(stopped);
    let ns = t0.elapsed().as_nanos() as u64;
    (report.live_bytes, ns)
}

/// [`Session::launch`] for the throughput rows: each owner runs its program
/// `reps` times, so per-owner work is a plain product.
fn tp_launch<'body, 'heap, 'scope, 'env>(
    era: &'body ParallelHeapEra<'heap>,
    nil: &'env SharedRoot,
    programs: &[Vec<Op>],
    reps: usize,
    nslots: usize,
    bound: u32,
    line_slot: usize,
    overlap: Option<&'env Overlap>,
    abort: &'env AtomicBool,
    scope: &'scope thread::Scope<'scope, 'env>,
) -> Session<'body, 'heap>
where
    'body: 'env,
{
    let n = programs.len();
    let heap = era.heap_identity();
    let registry = era.registry();
    let (pause_tx, pause_rx) = channel();
    let (end_tx, end_rx) = channel();
    let (ready_tx, ready_rx) = channel();
    let (retired_tx, retired_rx) = channel();
    let mut go_txs = Vec::with_capacity(n);
    let mut done_txs = Vec::with_capacity(n);
    // Design 3.3: register every permit before spawning any owner thread.
    let mut permits = Vec::with_capacity(n);
    for index in 0..n {
        permits.push(retry_reg(|| era.register(6300 + index as u64, nslots)));
    }
    for (index, permit) in permits.into_iter().enumerate() {
        let (go_tx, go_rx) = channel();
        let (done_tx, done_rx) = channel();
        go_txs.push(go_tx);
        done_txs.push(done_tx);
        let launch = OwnerLaunch {
            permit,
            heap,
            id: 6300 + index as u64,
            nil,
            programs: vec![programs[index].clone(); reps],
            nslots,
            bound,
            line_slot,
            stop_at: tp_stop,
            overlap,
            index,
            pause_tx: pause_tx.clone(),
            go_rx,
            end_tx: end_tx.clone(),
            ready_tx: ready_tx.clone(),
            done_rx,
            retired_tx: retired_tx.clone(),
            registry,
            abort,
            planted: &[],
            ht: None,
        };
        scope.spawn(move || launch.run());
    }
    drop(pause_tx);
    drop(end_tx);
    drop(ready_tx);
    drop(retired_tx);
    Session {
        era,
        go_txs,
        done_txs,
        pause_rx,
        end_rx,
        ready_rx,
        retired_rx,
        next_epoch: 1,
    }
}

/// Drive one row: `owners` executors, each running `reps` programs of
/// `cells` cons builds with a Safepoint every `spacing` conses. Returns the
/// measured row.
fn tp_drive(
    label: &str,
    owners: usize,
    cells: usize,
    reps: usize,
    spacing: usize,
    stop: bool,
    threshold_override: Option<usize>,
) -> TpRow {
    let threshold: usize = threshold_override.unwrap_or_else(|| {
        std::env::var("A8_TP_THRESHOLD")
            .ok()
            .and_then(|value| value.parse().ok())
            .unwrap_or(2048)
    });
    TP_THRESHOLD.store(if stop { threshold } else { 0 }, Ordering::Relaxed);
    assert!(
        cells % spacing == 0,
        "cells ({cells}) must be a multiple of the safepoint spacing ({spacing})"
    );
    let program = tp_list_ops(owners as i64 * 1_000_000, cells as i64, spacing);
    // One slice executes the whole unrolled iteration, so the step cap must
    // cover both the per-program loop and the unrolled body.
    let step_cap: u32 = ((cells * 8 + 64).max(spacing * 16 + 64)) as u32;
    // The validator walks the whole program concretely, so its step budget
    // must scale with the row (STEP_CAP alone caps at ~10k steps).
    let validate_cap: u32 = (cells * 16 + spacing * 32 + 1024) as u32;
    let programs: Vec<Vec<Op>> = (0..owners)
        .map(|index| {
            let _ = index;
            program.clone()
        })
        .collect();
    for program in &programs {
        prevalidate(program, 11, step_cap, validate_cap)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    // Each program builds `cells` loop conses plus one final cons for the
    // old-to-young SetCar edge.
    let total_per_owner = reps * (cells + 1);
    let expected_rounds = if stop { total_per_owner / threshold } else { 0 };
    reset_tp_catalog_waits();
    let overlap = Overlap::new(owners, true);
    let mut row = TpRow {
        label: label.to_string(),
        owners,
        cells_per_owner: total_per_owner,
        gcs: 0,
        wall_ns: 0,
        pause_ns: Vec::new(),
        live_bytes_last: 0,
        live_bytes_max: 0,
        per_owner_conses: Vec::new(),
        witness_enter: Vec::new(),
        witness_alloc: Vec::new(),
        grant_calls: 0,
        grant_ns: 0,
        grant_max_ns: 0,
        close_calls: 0,
        close_ns: 0,
        close_max_ns: 0,
    };
    with_admitted(|era, generational, nil, _one| {
        assert_eq!(
            generational,
            std::env::var("NEOVM_GC_GENERATIONAL").as_deref() != Ok("0"),
            "row must record the heap mode it ran under"
        );
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &overlap.abort,
            };
            let mut session = tp_launch(
                era,
                nil,
                &programs,
                reps,
                11,
                step_cap,
                2,
                Some(&overlap),
                &overlap.abort,
                scope,
            );
            let t0 = Instant::now();
            for _ in 0..expected_rounds {
                let (live, ns) = tp_round(&mut session);
                row.pause_ns.push(ns);
                row.live_bytes_last = live;
                row.live_bytes_max = row.live_bytes_max.max(live);
            }
            // The timed window ends when the last owner reports completion;
            // the teardown round below stays outside it.
            let ends = gather(&session.end_rx, owners, |end| end.index);
            let wall = t0.elapsed();
            let waits = read_tp_catalog_waits();
            row.gcs = expected_rounds;
            row.wall_ns = wall.as_nanos() as u64;
            row.per_owner_conses = ends.iter().map(|end| end.cons_count).collect();
            row.witness_enter = (0..owners)
                .map(|index| overlap.seen_enter[index].load(Ordering::SeqCst))
                .collect();
            row.witness_alloc = (0..owners)
                .map(|index| overlap.seen_alloc[index].load(Ordering::SeqCst))
                .collect();
            row.grant_calls = waits.0;
            row.grant_ns = waits.1;
            row.grant_max_ns = waits.2;
            row.close_calls = waits.3;
            row.close_ns = waits.4;
            row.close_max_ns = waits.5;
            for end in &ends {
                assert_eq!(end.cons_count, total_per_owner, "owner finished its work");
            }
            // Teardown, mirroring `finish_owners` without its callback.
            for _ in 0..owners {
                recv(&session.ready_rx);
            }
            let mut stopped = retry_rv(|| session.era.stop());
            assert_eq!(stopped.epoch().get(), session.next_epoch, "reclaim epoch");
            session.next_epoch += 1;
            retry_reg(|| session.era.return_cons_storage(&stopped));
            let report = retry_rv(|| session.era.collect_stopped(&mut stopped));
            assert!(report.major);
            complete(stopped);
            for tx in &session.done_txs {
                tx.send(())
                    .unwrap_or_else(|_| panic!("done channel closed"));
            }
            for _ in 0..owners {
                recv(&session.retired_rx);
            }
        });
    });
    row
}

fn tp_row_print(row: &TpRow, generational: bool, tsv: &mut Option<&mut String>) {
    let cells = (row.owners * row.cells_per_owner) as f64;
    let seconds = row.wall_ns as f64 / 1_000_000_000.0;
    let cells_per_s = cells / seconds;
    let bytes_per_s = cells * 16.0 / seconds;
    let mut pauses = row.pause_ns.clone();
    pauses.sort_unstable();
    let p50 = if pauses.is_empty() {
        0
    } else {
        pauses[pauses.len() / 2]
    };
    let max = pauses.last().copied().unwrap_or(0);
    let line = format!(
        "A8TP gen1={} row={} owners={} cells_per_owner={} total_cells={} wall_ms={:.3} cells_per_s={:.0} mib_per_s={:.1} gcs={} live_last={} live_max={} pause_p50_us={:.1} pause_max_us={:.1} per_owner_conses={:?} witness_enter={:?} witness_alloc={:?} grant_calls={} grant_ns={} grant_max_ns={} close_calls={} close_ns={} close_max_ns={}",
        generational,
        row.label,
        row.owners,
        row.cells_per_owner,
        row.owners * row.cells_per_owner,
        row.wall_ns as f64 / 1_000_000.0,
        cells_per_s,
        bytes_per_s / (1024.0 * 1024.0),
        row.gcs,
        row.live_bytes_last,
        row.live_bytes_max,
        p50 as f64 / 1000.0,
        max as f64 / 1000.0,
        row.per_owner_conses,
        row.witness_enter,
        row.witness_alloc,
        row.grant_calls,
        row.grant_ns,
        row.grant_max_ns,
        row.close_calls,
        row.close_ns,
        row.close_max_ns,
    );
    println!("{line}");
    if let Some(tsv) = tsv {
        tsv.push_str(&line.replace('\t', " "));
        tsv.push('\n');
    }
}

/// The A8 throughput matrix on the proof executor (design section 7.2).
/// NOT arbitrary-Lisp performance: the same restricted executor, one fixed
/// list-building program per owner.
#[test]
#[ignore = "A8 throughput matrix; run pinned under the measurement lock"]
fn a8_throughput_rows() {
    let cells: usize = std::env::var("A8_TP_CELLS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(512);
    let reps_fixed: usize = std::env::var("A8_TP_REPS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(48);
    let total: usize = std::env::var("A8_TP_TOTAL")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(cells * reps_fixed);
    let tsv_dir = std::env::var("A8_TP_TSV").ok().filter(|s| !s.is_empty());
    let threshold: usize = std::env::var("A8_TP_THRESHOLD")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(2048);
    // Row-label suffix (e.g. "x20") so several row sizes can share one TSV
    // stream without ambiguity; empty keeps the classic labels.
    let tag = std::env::var("A8_TP_TAG").unwrap_or_default();
    let tag_suffix = if tag.is_empty() {
        String::new()
    } else {
        format!("-{tag}")
    };
    // Which row groups to run (comma list of pof,tf,ao); default all three.
    // Attribution runs filter to one group so profilers see only that shape.
    let groups = std::env::var("A8_TP_ROWS").unwrap_or_else(|_| "pof,tf,ao".to_string());
    let want = |group: &str| groups.split(',').any(|entry| entry.trim() == group);
    // Safepoints per slice: 1 keeps the classic one-slice-per-cell program;
    // K>1 unrolls the loop body K times per Safepoint to quantify what an
    // owner with amortized slices (R7's RunSlice fast path) would pay.
    let spacing: usize = std::env::var("A8_TP_SPACING")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(1);
    let mut tsv = String::new();
    let generational = std::env::var("NEOVM_GC_GENERATIONAL").as_deref() != Ok("0");

    // Identical per-owner work at 1..4 tokens.
    if want("pof") {
        for owners in [1usize, 2, 3, 4] {
            let row = tp_drive(
                &format!("per-owner-fixed-{owners}{tag_suffix}"),
                owners,
                cells,
                reps_fixed,
                spacing,
                true,
                None,
            );
            tp_row_print(&row, generational, &mut Some(&mut tsv));
        }
    }
    // Identical total work at 1..4 tokens: the driver splits the same total
    // into equal per-owner shares (rounded down to whole programs).
    if want("tf") {
        for owners in [1usize, 2, 3, 4] {
            let reps = (total / owners) / (cells + 1);
            assert!(reps >= 1, "total too small for {owners} owners");
            // Scale the per-owner threshold by 1/N so every fixed-total row runs
            // the same number of collection rounds, not total/N of them.
            let row = tp_drive(
                &format!("total-fixed-{owners}{tag_suffix}"),
                owners,
                cells,
                reps,
                spacing,
                true,
                Some((threshold / owners).max(1)),
            );
            tp_row_print(&row, generational, &mut Some(&mut tsv));
        }
    }
    // Bounded allocation-only rows with every collection disabled.
    if want("ao") {
        for owners in [1usize, 4] {
            let row = tp_drive(
                &format!("alloc-only-{owners}{tag_suffix}"),
                owners,
                cells,
                reps_fixed,
                spacing,
                false,
                None,
            );
            assert_eq!(row.gcs, 0, "no collections inside the timed window");
            tp_row_print(&row, generational, &mut Some(&mut tsv));
        }
    }
    if let Some(dir) = tsv_dir {
        let path = format!(
            "{dir}/throughput-{}.tsv",
            if generational { "gen1" } else { "gen0" }
        );
        std::fs::write(&path, tsv).unwrap_or_else(|error| panic!("write {path}: {error:?}"));
        println!("tsv: {path}");
    }
}

// ---------------------------------------------------------------------------
// B4: 2-4 owner overlap on shared tables (design §6/§7)
// ---------------------------------------------------------------------------

/// One B4 multi-owner session: the shared tables are planted into every
/// owner's slots from the same transport roots, all permits register before
/// any owner thread exists (design §3.3), every owner runs its program list
/// concurrently through the restricted executor (`no_stop`: no mid-run
/// collection, so lease windows really overlap), and the coordinator drives
/// the one collection round after every owner reported. Returns each owner's
/// program lines, one list per owner.
fn run_ht_overlap_session(
    setup: &HtSetup,
    programs: &[Vec<Op>],
    reps: usize,
    bound: u32,
    env: &HtEnv<'_>,
    id_base: u64,
    dump_forms: &[&str],
) -> (Vec<Vec<String>>, Vec<String>) {
    let mut context = Context::new();
    let plant_values = context
        .eval_str(setup.source)
        .unwrap_or_else(|error| panic!("ht setup eval: {error:?}"));
    // SAFETY: every planted word belongs to this Context's heap; the roots
    // keep the shared tables alive for the whole era, and `SharedRoot` is
    // `Sync`, so every owner seeds from the same roots.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let mut planted: Vec<Option<SharedRoot>> = vec![None; setup.nslots];
    {
        let mut cursor = plant_values;
        for &(slot, _fact) in &setup.facts {
            assert!(cursor.is_cons(), "setup list shorter than its facts");
            let root =
                unsafe { SharedRoot::new(&context.tagged_heap, cursor.cons_car_unobserved()) };
            planted[usize::from(slot)] = Some(root);
            cursor = cursor.cons_cdr_unobserved();
        }
        assert!(cursor.is_nil(), "setup list longer than its facts");
    }
    let abort = AtomicBool::new(false);
    let mut lines: Vec<Vec<String>> = Vec::new();
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let heap = era.heap_identity();
                    let registry = era.registry();
                    let n = programs.len();
                    let (end_tx, end_rx) = channel();
                    let (ready_tx, ready_rx) = channel();
                    let (retired_tx, retired_rx) = channel();
                    let mut done_txs = Vec::with_capacity(n);
                    let mut permits = Vec::with_capacity(n);
                    for index in 0..n {
                        permits.push(retry_reg(|| {
                            era.register(id_base + index as u64, setup.nslots)
                        }));
                    }
                    for (index, permit) in permits.into_iter().enumerate() {
                        let (done_tx, done_rx) = channel();
                        done_txs.push(done_tx);
                        let launch = OwnerLaunch {
                            permit,
                            heap,
                            id: id_base + index as u64,
                            nil: &nil,
                            programs: vec![programs[index].clone(); reps],
                            nslots: setup.nslots,
                            bound,
                            line_slot: usize::from(HT_LINE),
                            stop_at: no_stop,
                            overlap: None,
                            index,
                            pause_tx: channel().0,
                            go_rx: channel().1,
                            end_tx: end_tx.clone(),
                            ready_tx: ready_tx.clone(),
                            done_rx,
                            retired_tx: retired_tx.clone(),
                            registry,
                            abort: &abort,
                            planted: &planted,
                            ht: Some(env),
                        };
                        scope.spawn(move || launch.run());
                    }
                    drop(end_tx);
                    drop(ready_tx);
                    drop(retired_tx);
                    let ends = gather(&end_rx, n, |end| end.index);
                    let _readies = gather(&ready_rx, n, |index| *index);
                    let mut stopped = retry_rv(|| era.stop());
                    assert_eq!(stopped.epoch().get(), 1);
                    retry_reg(|| era.return_cons_storage(&stopped));
                    assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                    complete(stopped);
                    for done in &done_txs {
                        done.send(()).unwrap();
                    }
                    let _retired = gather(&retired_rx, n, |index| *index);
                    lines = ends.into_iter().map(|end| end.lines).collect();
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    let dumps = dump_forms
        .iter()
        .map(|form| {
            let value = context
                .eval_str(form)
                .unwrap_or_else(|error| panic!("ht dump eval {form}: {error:?}"));
            prin1(value.bits())
        })
        .collect();
    (lines, dumps)
}

/// B4 reader overlap, parameterized over the owner count: every owner runs
/// one sealed reader op on the shared witnessed table inside an `arrive`
/// barrier — so all of them provably hold their reader lease simultaneously
/// — with a disjoint-key writer op and read-back around it. The word the
/// readers observe after the barrier is the design §6 witness, and the
/// disjoint puts/gets check the concurrent answers and the exact final
/// state (only owner `i` ever writes key `50+i`).
fn overlap_ht_readers(owners: usize) {
    // slots: 0 witnessed reader table, 1 disjoint-key table, 14 line.
    const TW: u16 = 0;
    const TX: u16 = 1;
    const NSLOTS: usize = 20;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tW (make-hash-table :test 'eq))
  (setq a6-tX (make-hash-table :test 'eq))
  (puthash 1 10 a6-tW)
  (puthash 2 20 a6-tW)
  (puthash 3 30 a6-tW)
  (puthash 4 40 a6-tW)
  (list a6-tW a6-tX))"#,
        facts: vec![(TW, Abs::Table), (TX, Abs::Table)],
        nslots: NSLOTS,
    };
    let mut programs = Vec::with_capacity(owners);
    for owner in 0..owners {
        let key = 50 + owner as i64;
        let value = 500 + owner as i64;
        let mut ops = vec![
            Op::ConstFixnum {
                dst: 15,
                value: key,
            },
            Op::ConstFixnum { dst: 16, value },
            Op::HtPut {
                table: TX,
                key: 15,
                value: 16,
            },
            // The witnessed reader op: its lease spans the barrier.
            Op::HtCount { table: TW, dst: 17 },
            Op::HtGet {
                table: TX,
                key: 15,
                found: 18,
                value: 19,
            },
        ];
        ops.extend(finish_line(&[18, 19, 17]));
        programs.push(ops);
    }
    let initial = setup.initial();
    for program in &programs {
        prevalidate_with(program, NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    let abort = AtomicBool::new(false);
    let witness = HtWitness::new(TW, owners, 0, &abort);
    let progress: Vec<AtomicUsize> = (0..owners).map(|_| AtomicUsize::new(0)).collect();
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.witness = Some(&witness);
    env.progress = &progress;
    let (lines, dumps) = run_ht_overlap_session(
        &setup,
        &programs,
        1,
        0,
        &env,
        6700,
        &["(hash-table-count a6-tX)", "(hash-table-count a6-tW)"],
    );
    assert_eq!(lines.len(), owners);
    for (owner, owner_lines) in lines.iter().enumerate() {
        assert_eq!(owner_lines.len(), 1, "one program per owner");
        assert_eq!(
            owner_lines[0],
            format!("(t {} 4)", 500 + owner),
            "own key hit, stable shared-table count, for owner {owner}"
        );
    }
    // The §6 witness: every one of the `owners` reader leases was live at
    // once, observed on the authority word itself.
    assert_eq!(
        witness.max_readers.load(Ordering::SeqCst),
        owners as u64,
        "all {owners} readers held the table at once"
    );
    assert!(
        witness.max_readers.load(Ordering::SeqCst) > 1,
        "max observed concurrent readers > 1"
    );
    for (owner, counter) in progress.iter().enumerate() {
        assert_eq!(counter.load(Ordering::Relaxed), 3, "owner {owner} progress");
    }
    // Exact final state: every disjoint key present, shared table untouched.
    assert_eq!(dumps[0], format!("{owners}"), "every disjoint key landed");
    assert_eq!(dumps[1], "4", "the witnessed table kept its seeded entries");
}

#[test]
fn a6_ht_overlap_shared_readers_two_owners() {
    overlap_ht_readers(2);
}

#[test]
fn a6_ht_overlap_shared_readers_three_owners() {
    overlap_ht_readers(3);
}

#[test]
fn a6_ht_overlap_shared_readers_four_owners() {
    overlap_ht_readers(4);
}

/// B4 writer drain: four readers hold their leases across the barrier while
/// one writer owner publishes its attempt and waits in the writer's drain —
/// a reader observes the wait (nonzero count, no WRITER bit), the readers
/// leave, and only then does the writer's edit run on the whole word. The
/// writer's put is a real sealed `HtPut`; the readers' counts are real
/// sealed `HtCount`s.
#[test]
fn a6_ht_overlap_writer_drains_four_readers() {
    const TW: u16 = 0;
    const NSLOTS: usize = 20;
    let readers = 4usize;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tW (make-hash-table :test 'eq))
  (puthash 1 10 a6-tW)
  (list a6-tW))"#,
        facts: vec![(TW, Abs::Table)],
        nslots: NSLOTS,
    };
    let mut programs = Vec::with_capacity(readers + 1);
    for _ in 0..readers {
        let mut ops = vec![Op::HtCount { table: TW, dst: 17 }];
        ops.extend(finish_line(&[17]));
        programs.push(ops);
    }
    let mut writer = vec![
        Op::ConstFixnum { dst: 15, value: 90 },
        Op::ConstFixnum { dst: 16, value: 91 },
        Op::HtPut {
            table: TW,
            key: 15,
            value: 16,
        },
    ];
    writer.extend(finish_line(&[]));
    programs.push(writer);
    let initial = setup.initial();
    for program in &programs {
        prevalidate_with(program, NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    let abort = AtomicBool::new(false);
    let witness = HtWitness::new(TW, readers, 1, &abort);
    let progress: Vec<AtomicUsize> = (0..=readers).map(|_| AtomicUsize::new(0)).collect();
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.witness = Some(&witness);
    env.progress = &progress;
    let (lines, dumps) = run_ht_overlap_session(
        &setup,
        &programs,
        1,
        0,
        &env,
        6710,
        &["(hash-table-count a6-tW)", "(gethash 90 a6-tW)"],
    );
    assert_eq!(lines.len(), readers + 1);
    for (owner, owner_lines) in lines.iter().take(readers).enumerate() {
        assert_eq!(owner_lines[0], "(1)", "reader {owner} counted the seed");
    }
    assert_eq!(
        witness.max_readers.load(Ordering::SeqCst),
        readers as u64,
        "all four readers overlapped before the writer could enter"
    );
    assert!(
        witness.drain_observed.load(Ordering::SeqCst),
        "a reader observed the writer's drain wait: readers live, no WRITER bit"
    );
    assert_eq!(
        witness.writer_word.load(Ordering::SeqCst),
        HT_WRITER_BIT,
        "the writer held the whole word: every reader had drained"
    );
    for (owner, counter) in progress.iter().enumerate() {
        assert_eq!(counter.load(Ordering::Relaxed), 1, "owner {owner} progress");
    }
    assert_eq!(dumps[0], "2", "seed plus the writer's key");
    assert_eq!(dumps[1], "91", "the drained writer's edit landed");
}

/// B4 final-state membership: two owners put the same key twice each with no
/// witness (pure concurrent sealed writes). Whatever interleaving ran, the
/// final state must be a member of the legal serial interleavings — the six
/// per-owner-order-respecting orders of four puts — which the test
/// enumerates exhaustively. Op atomicity shows as the count never leaving
/// one: four puts, one key, no torn duplicate ever observed (each owner
/// counts after its own puts).
#[test]
fn a6_ht_overlap_final_state_in_legal_serial_interleavings() {
    const TI: u16 = 0;
    const NSLOTS: usize = 20;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tI (make-hash-table :test 'eq))
  (list a6-tI))"#,
        facts: vec![(TI, Abs::Table)],
        nslots: NSLOTS,
    };
    // (owner, key, value): owner A then owner B, two puts each on key 77.
    let plan: [(usize, i64, i64); 4] = [(0, 77, 11), (0, 77, 12), (1, 77, 21), (1, 77, 22)];
    let mut programs = Vec::new();
    for owner in 0..2usize {
        let mut ops = Vec::new();
        for &(o, key, value) in &plan {
            if o != owner {
                continue;
            }
            ops.push(Op::ConstFixnum {
                dst: 15,
                value: key,
            });
            ops.push(Op::ConstFixnum { dst: 16, value });
            ops.push(Op::HtPut {
                table: TI,
                key: 15,
                value: 16,
            });
        }
        ops.push(Op::HtCount { table: TI, dst: 17 });
        ops.extend(finish_line(&[17]));
        programs.push(ops);
    }
    let initial = setup.initial();
    for program in &programs {
        prevalidate_with(program, NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    // Exhaustive enumeration: choose A's two positions among the four op
    // slots; B fills the rest in its own order. The final value is the last
    // put of the interleaving; the count is always one (same key).
    let mut legal_finals = HashSet::new();
    for a1 in 0..4usize {
        for a2 in (a1 + 1)..4usize {
            let mut slots: [Option<usize>; 4] = [None; 4];
            slots[a1] = Some(0);
            slots[a2] = Some(1);
            let mut next_b = 2usize;
            for slot in slots.iter_mut() {
                if slot.is_none() {
                    *slot = Some(next_b);
                    next_b += 1;
                }
            }
            let mut final_value = 0;
            for op_index in slots.iter().copied().flatten() {
                final_value = plan[op_index].2;
            }
            legal_finals.insert(final_value);
        }
    }
    assert_eq!(
        legal_finals,
        HashSet::from([12, 22]),
        "each owner's second put can win, nothing else is serial-legal"
    );
    let env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let (lines, dumps) = run_ht_overlap_session(
        &setup,
        &programs,
        1,
        0,
        &env,
        6720,
        &["(gethash 77 a6-tI)", "(hash-table-count a6-tI)"],
    );
    assert_eq!(lines.len(), 2);
    for (owner, owner_lines) in lines.iter().enumerate() {
        assert_eq!(
            owner_lines[0], "(1)",
            "owner {owner}: four concurrent puts on one key never duplicate it"
        );
    }
    let oracle = oracle_lines();
    assert_eq!(
        oracle[32].parse::<i64>().expect("serial order one final"),
        22,
        "GNU: owner B's second put wins when B runs last"
    );
    assert_eq!(
        oracle[33].parse::<i64>().expect("serial order two final"),
        12,
        "GNU: owner A's second put wins when A runs last"
    );
    let final_value = dumps[0].parse::<i64>().expect("a fixnum");
    assert!(
        final_value == oracle[32].parse::<i64>().expect("serial order one")
            || final_value == oracle[33].parse::<i64>().expect("serial order two"),
        "observed final {final_value} is outside the GNU legal serial interleavings"
    );
    assert_eq!(dumps[1], "1");
}

/// B4 string keys (design §4 Rev R3): planted strings as `equal`-test keys
/// through the sealed plain path — the puts materialize `HashKey` string
/// content under WRITER (ordinary heap traffic), the gets/rem/count are
/// sealed reads/edits — matching the GNU 31.1 oracle line for line. No
/// string is ever printed into a line: found flags and counts only, exactly
/// the P5 convention (GNU's `remhash` says nil unconditionally, so the
/// oracle twin derives found flags from sentinel presence checks).
#[test]
fn a6_ht_string_keys_match_gnu_oracle() {
    // slots: 0 string table, 5/6/7 planted keys, 14 line.
    const TS: u16 = 0;
    const S1: u16 = 5;
    const S2: u16 = 6;
    const S3: u16 = 7;
    const NSLOTS: usize = 20;
    let program = vec![
        Op::ConstFixnum {
            dst: 15,
            value: 101,
        },
        Op::HtPut {
            table: TS,
            key: S1,
            value: 15,
        },
        Op::ConstFixnum {
            dst: 15,
            value: 102,
        },
        Op::HtPut {
            table: TS,
            key: S2,
            value: 15,
        },
        Op::HtGet {
            table: TS,
            key: S1,
            found: 16,
            value: 17,
        },
        Op::HtGet {
            table: TS,
            key: S3,
            found: 18,
            value: 19,
        },
        // (remhash "a6-beta" ..) — found flag from the op, then a post-rem
        // presence check, so the twin can derive both from sentinels.
        Op::HtRem {
            table: TS,
            key: S2,
            found: 16,
        },
        Op::HtGet {
            table: TS,
            key: S2,
            found: 18,
            value: 19,
        },
        Op::HtCount { table: TS, dst: 15 },
    ];
    let ops = finish_line(&[16, 18, 15]);
    let program = [program, ops].concat();
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tS (make-hash-table :test 'equal))
  (list a6-tS "a6-alpha" "a6-beta" "a6-gamma"))"#,
        facts: vec![
            (TS, Abs::Table),
            (S1, Abs::Top),
            (S2, Abs::Top),
            (S3, Abs::Top),
        ],
        nslots: NSLOTS,
    };
    prevalidate_with(&program, NSLOTS, 0, STEP_CAP, &setup.initial())
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let (lines, dumps) = run_ht_session(
        &setup,
        vec![program],
        0,
        &env,
        &[
            // The bare context has no `not` (a subr.el defun): the sentinel
            // check spelled with `if` prints the same t/nil the GNU twin's
            // `not` does.
            "(if (eq (gethash \"a6-alpha\" a6-tS 'a6-miss) 'a6-miss) nil t)",
            "(if (eq (gethash \"a6-beta\" a6-tS 'a6-miss) 'a6-miss) nil t)",
            "(internal--hash-table-buckets a6-tS)",
        ],
    );
    assert_eq!(lines.len(), 1);
    let oracle = oracle_lines();
    assert_eq!(lines[0], oracle[28], "P9 string-key op line");
    assert_eq!(dumps[0], oracle[29], "P9 alpha still present");
    assert_eq!(dumps[1], oracle[30], "P9 beta removed");
    assert_eq!(dumps[2], oracle[31], "P9 buckets after the removal");
}

/// B4: another owner's edits around one user-test comparison window. While
/// the comparison's §3.1 nesting hold is live, the editor's same-table edit
/// is refused by the depth guard (the sealed mutating ops' own check);
/// after the window closes, its edit lands under WRITER and bumps the epoch
/// exactly between the scan's candidate copy and its re-read, so the lookup
/// revalidates — a real other-owner provocation, not the P8 lever. The
/// answer is the stable twin's either way.
#[test]
fn a6_ht_user_test_revalidates_under_another_owner() {
    let started = AtomicUsize::new(0);
    let program = ht_p8s();
    let setup = ht_main_setup();
    let initial = setup.initial();
    prevalidate_with(&program, HT_NSLOTS, 0, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let hash_prog = [Op::Move {
        dst: HT_CB_OUT,
        src: HT_CB_KEY,
    }];
    let hash2_prog = [Op::ConstFixnum {
        dst: HT_CB_OUT,
        value: 7,
    }];
    let cmp_prog = [Op::Eq {
        dst: HT_CB_OUT,
        a: HT_CB_KEY,
        b: HT_CB_CAND,
    }];
    let cmp_other = [
        Op::ConstFixnum { dst: 15, value: 5 },
        Op::ConstFixnum { dst: 16, value: 55 },
        Op::HtPut {
            table: HT_TO2,
            key: 15,
            value: 16,
        },
        Op::Eq {
            dst: HT_CB_OUT,
            a: HT_CB_KEY,
            b: HT_CB_CAND,
        },
    ];
    let mut facts = initial;
    facts.extend([(HT_CB_KEY, Abs::Fix(0)), (HT_CB_CAND, Abs::Fix(0))]);
    for ops in [
        &hash_prog[..],
        &hash2_prog[..],
        &cmp_prog[..],
        &cmp_other[..],
    ] {
        prevalidate_with(ops, HT_NSLOTS, 0, STEP_CAP, &facts)
            .unwrap_or_else(|error| panic!("callback prevalidate: {error:?}"));
    }
    started.fetch_add(1, Ordering::Relaxed);
    assert_eq!(started.load(Ordering::Relaxed), 1);
    let users = [
        UserTestProg {
            slot: HT_TU,
            hash_ops: &hash_prog,
            cmp_ops: &cmp_prog,
        },
        UserTestProg {
            slot: HT_TO,
            hash_ops: &hash2_prog,
            cmp_ops: &cmp_other,
        },
    ];
    let editor = HtEditor::new();
    let progress = [AtomicUsize::new(0), AtomicUsize::new(0)];
    let mut env = HtEnv::new(&users, HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    // The P8 lever stays disarmed: this row's mutation is the editor's.
    env.editor = Some(&editor);
    env.progress = &progress;
    let mut context = Context::new();
    let plant_values = context
        .eval_str(setup.source)
        .unwrap_or_else(|error| panic!("ht setup eval: {error:?}"));
    // SAFETY: the planted tables belong to this Context's heap; the roots
    // keep them alive for the whole era and both owners seed from them.
    let nil = unsafe { SharedRoot::new(&context.tagged_heap, TaggedValue::NIL) };
    let mut planted: Vec<Option<SharedRoot>> = vec![None; setup.nslots];
    {
        let mut cursor = plant_values;
        for &(slot, _fact) in &setup.facts {
            let root =
                unsafe { SharedRoot::new(&context.tagged_heap, cursor.cons_car_unobserved()) };
            planted[usize::from(slot)] = Some(root);
            cursor = cursor.cons_cdr_unobserved();
        }
        assert!(cursor.is_nil(), "setup list matches its facts");
    }
    let abort = AtomicBool::new(false);
    let mut lines: Vec<String> = Vec::new();
    context
        .admit_owned_parallel(|projection| {
            {
                let era = projection.era();
                era.start_cons_storage()
                    .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
                thread::scope(|scope| {
                    let _panic = PanicFlag {
                        registry: era.registry(),
                        abort: &abort,
                    };
                    let heap = era.heap_identity();
                    let registry = era.registry();
                    let abort_flag = &abort;
                    // Both permits exist before either owner thread (§3.3).
                    let permit_r = retry_reg(|| era.register(6800, setup.nslots));
                    let permit_e = retry_reg(|| era.register(6801, setup.nslots));
                    let (end_tx, end_rx) = channel();
                    let (ready_tx, ready_rx) = channel();
                    let (done_tx_r, done_rx_r) = channel();
                    let (done_tx_e, done_rx_e) = channel();
                    let (retired_tx, retired_rx) = channel();
                    let launch = OwnerLaunch {
                        permit: permit_r,
                        heap,
                        id: 6800,
                        nil: &nil,
                        programs: vec![program],
                        nslots: setup.nslots,
                        bound: 0,
                        line_slot: usize::from(HT_LINE),
                        stop_at: no_stop,
                        overlap: None,
                        index: 0,
                        pause_tx: channel().0,
                        go_rx: channel().1,
                        end_tx: end_tx.clone(),
                        ready_tx: ready_tx.clone(),
                        done_rx: done_rx_r,
                        retired_tx: retired_tx.clone(),
                        registry,
                        abort: &abort,
                        planted: &planted,
                        ht: Some(&env),
                    };
                    scope.spawn(move || launch.run());
                    // The editor owner: its two steps are the sealed
                    // mutating ops' own — the §3.1 guard read exactly where
                    // `plain_put` reads it, and the no-op WRITER edit that
                    // bumps the epoch.
                    let editor_ref = &editor;
                    let planted_ref = &planted;
                    let nil_ref = &nil;
                    let ready_tx_e = ready_tx.clone();
                    let retired_tx_e = retired_tx.clone();
                    scope.spawn(move || {
                        let _panic = PanicFlag {
                            registry,
                            abort: abort_flag,
                        };
                        let mut owner = activate_owner(permit_e, heap, 6801);
                        let slots = seed_planted(&mut owner, nil_ref, planted_ref);
                        let to_tv = must(with_run(&mut owner, |run| {
                            Ok(TaggedValue::from_bits(
                                slots[usize::from(HT_TO)].word_for_test(run),
                            ))
                        }));
                        // Phases 1→2: while the reader's comparison hold is
                        // live, the same-table edit must refuse — the word
                        // is free (holds own no authority), so the writer
                        // lease mints; only the depth check refuses.
                        editor_ref.wait_then(1, 2, || {
                            let refused = with_run(&mut owner, |run| {
                                let lease = TableMut::enter(run, to_tv)?;
                                Ok(lease.user_test_mutation_refused())
                            });
                            editor_ref
                                .refusal_seen
                                .store(matches!(refused, Ok(true)), Ordering::SeqCst);
                        });
                        // Phases 3→4: the hold is gone; the edit lands under
                        // WRITER and bumps the epoch (the bump lever's edit).
                        editor_ref.wait_then(3, 4, || {
                            must(with_run(&mut owner, |run| {
                                let mut lease = TableMut::enter(run, to_tv)?;
                                lease.edit(|_| ());
                                editor_ref.edit_landed.store(true, Ordering::SeqCst);
                                Ok(())
                            }));
                        });
                        shutdown_owner(owner, 1, &ready_tx_e, &done_rx_e, &retired_tx_e);
                    });
                    drop(end_tx);
                    drop(ready_tx);
                    drop(retired_tx);
                    let ends = gather(&end_rx, 1, |end| end.index);
                    let _readies = gather(&ready_rx, 2, |index| *index);
                    let mut stopped = retry_rv(|| era.stop());
                    assert_eq!(stopped.epoch().get(), 1);
                    retry_reg(|| era.return_cons_storage(&stopped));
                    assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                    complete(stopped);
                    done_tx_r.send(()).unwrap();
                    done_tx_e.send(()).unwrap();
                    let _retired = gather(&retired_rx, 2, |index| *index);
                    lines = ends.into_iter().next().expect("reader end").lines;
                });
            }
            let (collector, keeper) = projection
                .finish()
                .unwrap_or_else(|error| panic!("finish: {error:?}"));
            (collector, keeper, ())
        })
        .unwrap_or_else(|error| panic!("admit: {error:?}"));
    assert!(
        editor.refusal_seen.load(Ordering::SeqCst),
        "the depth guard refused the editor's edit inside the test window"
    );
    assert!(
        editor.edit_landed.load(Ordering::SeqCst),
        "the editor's edit landed once the window closed"
    );
    assert_eq!(
        env.revalidations.load(Ordering::SeqCst),
        1,
        "the real other-owner edit moved the epoch under the scan exactly once"
    );
    assert_eq!(progress[0].load(Ordering::Relaxed), 3, "reader progress");
    assert_eq!(progress[1].load(Ordering::Relaxed), 0, "editor progress");
    let oracle = oracle_lines();
    assert_eq!(lines.len(), 1);
    assert_eq!(lines[0], oracle[26], "same answer as the stable twin");
    // Serial world back: the no-op edit changed no content.
    let count = context
        .eval_str("(hash-table-count a6-tO)")
        .unwrap_or_else(|error| panic!("count eval: {error:?}"));
    assert_eq!(prin1(count.bits()), "1");
    assert_eq!(started.load(Ordering::Relaxed), 1);
}

/// B4 reader-scaling row (design §6, Rev R4): 1-4 owners, one shared
/// prefilled `eq` table, nothing but sealed `HtGet`s — the per-op cost must
/// not grow with the reader count (readers share the table under the
/// authority word; there is no reader serialization to pay for). The row
/// prints `ns/op` only under `NEOVM_P2HT_SCALE`, so the gates stay silent;
/// the structural asserts (every owner's every answer, and the exact final
/// state) hold in every run.
#[test]
fn a6_ht_reader_scaling_row() {
    const TW: u16 = 0;
    const NSLOTS: usize = 20;
    const REPS: usize = 1500;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tW (make-hash-table :test 'eq))
  (let ((k 0))
    (while (< k 8)
      (puthash k (* k 10) a6-tW)
      (setq k (1+ k))))
  (list a6-tW))"#,
        facts: vec![(TW, Abs::Table)],
        nslots: NSLOTS,
    };
    let program = {
        let mut ops = vec![
            Op::ConstFixnum { dst: 15, value: 7 },
            Op::HtGet {
                table: TW,
                key: 15,
                found: 16,
                value: 17,
            },
        ];
        ops.extend(finish_line(&[16, 17]));
        ops
    };
    prevalidate_with(&program, NSLOTS, 0, STEP_CAP, &setup.initial())
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    let want_scale_row = std::env::var("NEOVM_P2HT_SCALE").is_ok();
    for owners in 1..=4usize {
        let programs = vec![program.clone(); owners];
        let started = Instant::now();
        let (lines, dumps) = run_ht_overlap_session(
            &setup,
            &programs,
            REPS,
            0,
            &env,
            6820,
            &["(hash-table-count a6-tW)"],
        );
        let elapsed = started.elapsed();
        assert_eq!(lines.len(), owners);
        for owner_lines in &lines {
            assert_eq!(owner_lines.len(), REPS, "one line per repetition");
            assert!(
                owner_lines.iter().all(|line| line == "(t 70)"),
                "every reader answered the shared hit"
            );
        }
        assert_eq!(dumps[0], "8", "read-only owners left the table exact");
        let ns_per_op = elapsed.as_nanos() as f64 / (owners * REPS) as f64;
        if want_scale_row {
            println!("p2ht-scale owners={owners} ns_per_op={ns_per_op:.1}");
        }
    }
}

/// B4 sim-side JIT regression row (design §7 risk 4): a real
/// byte-compiler jump table (pcase) compiled inline, then edited through
/// the sealed writer lease from a registered owner — the same serial edit
/// body, now under the authority word. The compiled switch captured the
/// table's epoch, so the next native dispatch must refuse the stale inline
/// path and raise the shim's loud signal instead of jumping through a
/// word the compiler never produced.
#[cfg(feature = "jit")]
#[test]
fn a6_ht_sealed_edit_triggers_jit_switch_stale_shim() {
    if !crate::emacs_core::jit::compile::jit_inline_switch_on() {
        // The inline dispatch is off (family config `jitoff`): no captured
        // epoch exists to go stale, and the shim row is vacuous.
        return;
    }
    crate::test_utils::init_test_tracing();
    use crate::emacs_core::bytecode::Op as ByteOp;
    // A bare context admits (the bootstrapped runtime carries permanent
    // weak tables A5 refuses); the switch body is the JIT tests' own
    // `(lambda (x) (switch x TABLE))` lowering, and the jump table is
    // BUILT through the sealed path before the compile captures it.
    let mut ev = Context::new();
    let keys = [
        TaggedValue::from_sym_id(crate::emacs_core::intern::intern("a6-sw-a")),
        TaggedValue::from_sym_id(crate::emacs_core::intern::intern("a6-sw-b")),
        TaggedValue::from_sym_id(crate::emacs_core::intern::intern("a6-sw-c")),
    ];
    // Instruction index of arm j / its GNU byte offset in the body below.
    let arm = |j: usize| 5 + 2 * j;
    let offset = |j: usize| 8 + 4 * j as i64;
    let table = ev
        .eval_str("(make-hash-table :test 'eq)")
        .unwrap_or_else(|error| panic!("jump table eval: {error:?}"));
    // SAFETY: the table belongs to this Context's heap; the root keeps it
    // alive across both eras.
    let table_root = unsafe { SharedRoot::new(&ev.tagged_heap, table) };
    // SAFETY: same heap as the context above.
    let nil = unsafe { SharedRoot::new(&ev.tagged_heap, TaggedValue::NIL) };
    // Era #1 — the sealed `put`s that build the jump table.
    let abort = AtomicBool::new(false);
    let planted_fill: Vec<Option<SharedRoot>> = vec![Some(table_root.clone())];
    ev.admit_owned_parallel(|projection| {
        let era = projection.era();
        era.start_cons_storage()
            .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
        thread::scope(|scope| {
            let _panic = PanicFlag {
                registry: era.registry(),
                abort: &abort,
            };
            let heap = era.heap_identity();
            let permit = retry_reg(|| era.register(6831, 1));
            let (ready_tx, ready_rx) = channel();
            let (done_tx, done_rx) = channel();
            let (retired_tx, retired_rx) = channel();
            let nil_ref = &nil;
            let planted_ref: &[Option<SharedRoot>] = &planted_fill;
            let key_bits: Vec<usize> = keys.iter().map(|key| key.bits()).collect();
            let offsets: Vec<i64> = (0..3).map(offset).collect();
            scope.spawn(move || {
                let mut owner = activate_owner(permit, heap, 6831);
                let slots = seed_planted(&mut owner, nil_ref, planted_ref);
                let table_tv = must(with_run(&mut owner, |run| {
                    Ok(TaggedValue::from_bits(slots[0].word_for_test(run)))
                }));
                for (j, &bits) in key_bits.iter().enumerate() {
                    plain_put(
                        &mut owner,
                        table_tv,
                        TaggedValue::from_bits(bits),
                        TaggedValue::fixnum(offsets[j]),
                        HashTableTest::Eq,
                        None,
                        None,
                    )
                    .unwrap_or_else(|error| panic!("sealed put: {error:?}"));
                }
                // The full teardown `shutdown_owner` runs: clear the seeded
                // root slot, enter native, then the ready/done/retired
                // handshake — `era.stop()` below must not race these ops
                // (a stop while the owner still runs leaves `run_access`
                // answering Busy until the deadline; TSan's slowdown made
                // exactly that wedge deterministic).
                must(with_run(&mut owner, |run| {
                    run.clear_root_slots()?;
                    Ok(())
                }));
                retry_rv(|| owner.enter_native(&[]));
                ready_tx
                    .send(0)
                    .unwrap_or_else(|_| panic!("ready channel closed"));
                recv(&done_rx);
                retire(owner);
                retired_tx
                    .send(0)
                    .unwrap_or_else(|_| panic!("retired channel closed"));
            });
            assert_eq!(recv(&ready_rx), 0, "fill owner frozen");
            let mut stopped = retry_rv(|| era.stop());
            assert_eq!(stopped.epoch().get(), 1);
            retry_reg(|| era.return_cons_storage(&stopped));
            assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
            complete(stopped);
            done_tx.send(()).unwrap();
            assert_eq!(recv(&retired_rx), 0);
        });
        let (collector, keeper) = projection
            .finish()
            .unwrap_or_else(|error| panic!("fill finish: {error:?}"));
        (collector, keeper, ())
    })
    .unwrap_or_else(|error| panic!("fill admit: {error:?}"));
    // The JIT compile: `(lambda (x) (switch x TABLE))`, miss answers -1,
    // arm j answers j — the jump table answered inline behind its epoch.
    let ops = vec![
        ByteOp::StackRef(0),
        ByteOp::Constant(0),
        ByteOp::Switch,
        ByteOp::Constant(1),
        ByteOp::Return,
        ByteOp::Constant(2),
        ByteOp::Return,
        ByteOp::Constant(3),
        ByteOp::Return,
        ByteOp::Constant(4),
        ByteOp::Return,
    ];
    let constants = vec![
        table,
        TaggedValue::fixnum(-1),
        TaggedValue::fixnum(0),
        TaggedValue::fixnum(1),
        TaggedValue::fixnum(2),
    ];
    let map: Vec<crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry> = (0..3)
        .map(|j| {
            crate::emacs_core::bytecode::chunk::GnuByteOffsetMapEntry::new(
                offset(j) as usize,
                arm(j),
            )
        })
        .collect();
    let before_sites =
        crate::emacs_core::jit::compile::switch_dispatch::inline_switch_sites_for_test();
    let leaf =
        crate::emacs_core::jit::compile::lower_leaf_with_map(&ops, &constants, 1, Some(&map))
            .expect("the switch body compiles");
    assert!(
        crate::emacs_core::jit::compile::switch_dispatch::inline_switch_sites_for_test()
            > before_sites,
        "the jump table is answered inline"
    );
    let ctx_ptr = &mut ev as *mut Context as *mut u8;
    // Call #1: the fresh compile answers inline.
    match leaf.call(ctx_ptr, &[keys[0]]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(bits) => {
            assert_eq!(TaggedValue::from_bits(bits).as_fixnum(), Some(0));
        }
        other => panic!("fresh compile answered natively, got {other:?}"),
    }
    // The sealed edit, from a registered owner: retarget one existing key
    // to a byte offset the compile never saw.
    let (report_tx, report_rx) = channel();
    let planted: Vec<Option<SharedRoot>> = vec![Some(table_root.clone())];
    ev.admit_owned_parallel(|projection| {
        {
            let era = projection.era();
            era.start_cons_storage()
                .unwrap_or_else(|error| panic!("cons catalog: {error:?}"));
            thread::scope(|scope| {
                let _panic = PanicFlag {
                    registry: era.registry(),
                    abort: &abort,
                };
                let heap = era.heap_identity();
                let permit = retry_reg(|| era.register(6830, 1));
                let (ready_tx, ready_rx) = channel();
                let (done_tx, done_rx) = channel();
                let (retired_tx, retired_rx) = channel();
                let nil_ref = &nil;
                let planted_ref: &[Option<SharedRoot>] = &planted;
                scope.spawn(move || {
                    let mut owner = activate_owner(permit, heap, 6830);
                    let slots = seed_planted(&mut owner, nil_ref, planted_ref);
                    let table_tv = must(with_run(&mut owner, |run| {
                        Ok(TaggedValue::from_bits(slots[0].word_for_test(run)))
                    }));
                    // Reader side: the epoch before, the table's test, one
                    // victim entry, and every compiled target.
                    let (epoch_before, test, victim_key, targets) =
                        must(with_run(&mut owner, |run| {
                            let lease = TableRef::enter(run, table_tv)?;
                            let described = lease.describe();
                            Ok(lease.read(|s| {
                                let targets: Vec<i64> = s
                                    .entries_in_slot_order()
                                    .map(|entry| entry.value.as_fixnum().expect("byte offset"))
                                    .collect();
                                let victim = s
                                    .entries_in_slot_order()
                                    .next()
                                    .expect("a jump-table entry");
                                (s.switch_epoch, described.test, victim.key, targets)
                            }))
                        }));
                    let stale_target = targets.iter().copied().max().expect("targets") + 1;
                    assert!(
                        !targets.contains(&stale_target),
                        "the retarget is outside the compiled set"
                    );
                    // Writer side: one sealed edit — the same serial body
                    // the builtins run, under the authority word.
                    must(with_run(&mut owner, |run| {
                        let mut lease = TableMut::enter(run, table_tv)?;
                        lease.edit(|ht| {
                            match ht
                                .data
                                .try_probe_for_insert(victim_key, test, false)
                                .expect("the victim key probes")
                            {
                                HashProbe::Found(slot) => {
                                    if let Some(stored) = ht.data.slot_value_mut(slot) {
                                        *stored = TaggedValue::fixnum(stale_target);
                                    }
                                }
                                HashProbe::Absent(_) => {
                                    panic!("the victim key must already sit in the table")
                                }
                            }
                        });
                        Ok(())
                    }));
                    let epoch_after = must(with_run(&mut owner, |run| {
                        let lease = TableRef::enter(run, table_tv)?;
                        Ok(lease.read(|s| s.switch_epoch))
                    }));
                    report_tx
                        .send((epoch_before, epoch_after, victim_key.bits()))
                        .expect("report channel open");
                    must(with_run(&mut owner, |run| {
                        run.clear_root_slots()?;
                        Ok(())
                    }));
                    retry_rv(|| owner.enter_native(&[]));
                    // Same ready/done/retired handshake as the fill era:
                    // the stop below waits for the freeze, the retire waits
                    // for the completion.
                    ready_tx
                        .send(0)
                        .unwrap_or_else(|_| panic!("ready channel closed"));
                    recv(&done_rx);
                    retire(owner);
                    retired_tx
                        .send(0)
                        .unwrap_or_else(|_| panic!("retired channel closed"));
                });
                assert_eq!(recv(&ready_rx), 0, "corrupt owner frozen");
                let mut stopped = retry_rv(|| era.stop());
                assert_eq!(stopped.epoch().get(), 1);
                retry_reg(|| era.return_cons_storage(&stopped));
                assert!(retry_rv(|| era.collect_stopped(&mut stopped)).major);
                complete(stopped);
                done_tx.send(()).unwrap();
                assert_eq!(recv(&retired_rx), 0);
            });
        }
        let (collector, keeper) = projection
            .finish()
            .unwrap_or_else(|error| panic!("finish: {error:?}"));
        (collector, keeper, ())
    })
    .unwrap_or_else(|error| panic!("admit: {error:?}"));
    let (epoch_before, epoch_after, victim_key_bits) = recv(&report_rx);
    let victim_key = TaggedValue::from_bits(victim_key_bits);
    assert!(
        epoch_after > epoch_before,
        "the sealed edit moved the switch epoch ({epoch_before} -> {epoch_after})"
    );
    // Call #2: the captured epoch is stale, the retargeted key's address is
    // not in the compiled set — the shim's loud signal, never a wild jump.
    match leaf.call(ctx_ptr, &[victim_key]) {
        crate::emacs_core::jit::compile::NativeRun::Signal => (),
        other => panic!("the stale path must signal, got {other:?}"),
    }
    match crate::emacs_core::jit::compile::take_pending_flow()
        .expect("the stale-table signal is stashed")
        .into_kind()
    {
        crate::emacs_core::error::FlowKind::Signal(sig) => {
            assert_eq!(sig.symbol_name(), "error");
            assert!(
                crate::emacs_core::print::print_value(&sig.data[0])
                    .contains("jump table mutated at runtime"),
                "the shim's own message"
            );
        }
        other => panic!("expected the switch-stale signal, got {other:?}"),
    }
}

/// B5 row 1: growth racing readers (design §4 Rev R3, §7 B5). The shared
/// table is seeded to exactly hashbrown capacity 14 in the serial world;
/// three readers hold their sealed reader lease across the barrier while
/// the writer's 15th distinct insert grows the index 14→28 under WRITER,
/// after the drain. The growth charge lands on the writer's owner counters
/// alone, agrees byte-for-byte with the storage the edit grew, and every
/// answer stays the GNU answer.
#[test]
fn a6_ht_growth_races_readers() {
    // slots: 0 shared seeded table; 15/16 operands; 17 count; 18/19 get.
    const TR: u16 = 0;
    const NSLOTS: usize = 20;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tR (make-hash-table :test 'eq))
  (puthash 1 1 a6-tR)
  (puthash 2 2 a6-tR)
  (puthash 3 3 a6-tR)
  (puthash 4 4 a6-tR)
  (puthash 5 5 a6-tR)
  (puthash 6 6 a6-tR)
  (puthash 7 7 a6-tR)
  (puthash 8 8 a6-tR)
  (puthash 9 9 a6-tR)
  (puthash 10 10 a6-tR)
  (puthash 11 11 a6-tR)
  (puthash 12 12 a6-tR)
  (puthash 13 13 a6-tR)
  (puthash 14 14 a6-tR)
  (list a6-tR))"#,
        facts: vec![(TR, Abs::Table)],
        nslots: NSLOTS,
    };
    let reader = {
        let mut ops = vec![Op::HtCount { table: TR, dst: 17 }];
        ops.extend(finish_line(&[17]));
        ops
    };
    let writer = {
        let mut ops = vec![
            Op::ConstFixnum { dst: 15, value: 90 },
            Op::ConstFixnum { dst: 16, value: 91 },
            Op::HtPut {
                table: TR,
                key: 15,
                value: 16,
            },
            Op::HtGet {
                table: TR,
                key: 15,
                found: 18,
                value: 19,
            },
        ];
        ops.extend(finish_line(&[18, 19]));
        ops
    };
    let programs = vec![reader.clone(), reader.clone(), reader.clone(), writer];
    let initial = setup.initial();
    for program in &programs {
        prevalidate_with(program, NSLOTS, 0, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    let abort = AtomicBool::new(false);
    let witness = HtWitness::new(TR, 3, 1, &abort);
    let growth = HtGrowth::new(4);
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.witness = Some(&witness);
    env.growth = Some(&growth);
    let (lines, dumps) = run_ht_overlap_session(
        &setup,
        &programs,
        1,
        0,
        &env,
        6850,
        &["(hash-table-count a6-tR)", "(gethash 90 a6-tR)"],
    );
    // Readers count inside their lease, before the writer can edit: the
    // seeded 14, deterministically. The writer reads its own insert back.
    for owner in 0..3 {
        assert_eq!(lines[owner], vec!["(14)".to_string()], "reader {owner}");
    }
    assert_eq!(lines[3], vec!["(t 91)".to_string()], "writer read-back");
    assert_eq!(dumps[0], "15", "seed plus one insert");
    assert_eq!(dumps[1], "91", "the writer's entry survived the era");
    // The B4 witness held for the growth edit too: three live readers, the
    // drain observed, WRITER alone once acquired.
    assert_eq!(witness.max_readers.load(Ordering::SeqCst), 3);
    assert!(witness.drain_observed.load(Ordering::SeqCst));
    assert_eq!(witness.writer_word.load(Ordering::SeqCst), HT_WRITER_BIT);
    // Growth accounting: exactly one event, on the writer, agreeing with
    // the storage bytes (the mismatch counter is the identity).
    for owner in 0..3 {
        assert_eq!(
            growth.charged[owner].load(Ordering::Relaxed),
            0,
            "reader {owner} charged for growth"
        );
    }
    assert!(
        growth.charged[3].load(Ordering::Relaxed) > 0,
        "the growing edit charged the acting owner"
    );
    assert_eq!(growth.events[3].load(Ordering::Relaxed), 1);
    assert_eq!(growth.mismatches.load(Ordering::Relaxed), 0);
}

/// B5 row 2: bounded-growth stress (§7 B5). Four owners each insert 40
/// owner-distinct keys into one shared table — 160 racing inserts, several
/// resizes on both backings — until the final state is exactly 160
/// entries. Every put's charge must equal its own storage growth however
/// the edits interleave, and the total charge stays within the amortized
/// bound (≤ 8× the final entry footprint).
#[test]
fn a6_ht_bounded_growth_many_owners() {
    // slots: 0 shared table; 15 one; 16 count; 17 limit; 18 k; 19 cond;
    // 20 owner key base; 21 key.
    const TB: u16 = 0;
    const NSLOTS: usize = 22;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tB (make-hash-table :test 'eq))
  (list a6-tB))"#,
        facts: vec![(TB, Abs::Table)],
        nslots: NSLOTS,
    };
    let owners = 4;
    let per_owner: usize = 40;
    let mut programs = Vec::with_capacity(owners);
    for owner in 0..owners {
        let base = 1000 * (owner as i64 + 1);
        let mut ops = vec![
            Op::ConstFixnum {
                dst: 17,
                value: per_owner as i64,
            },
            Op::ConstFixnum { dst: 18, value: 0 },
            Op::ConstFixnum { dst: 15, value: 1 },
            Op::ConstFixnum {
                dst: 20,
                value: base,
            },
        ];
        let loop_at = ops.len() as u16;
        ops.push(Op::Lt {
            dst: 19,
            a: 18,
            b: 17,
        });
        let branch_at = ops.len();
        ops.push(Op::BranchIfNil {
            cond: 19,
            target: 0,
        });
        ops.push(Op::Add {
            dst: 21,
            a: 20,
            b: 18,
        });
        ops.push(Op::HtPut {
            table: TB,
            key: 21,
            value: 21,
        });
        ops.push(Op::Add {
            dst: 18,
            a: 18,
            b: 15,
        });
        ops.push(Op::Jump { target: loop_at });
        let done = ops.len() as u16;
        ops.push(Op::HtCount { table: TB, dst: 16 });
        ops[branch_at] = Op::BranchIfNil {
            cond: 19,
            target: done,
        };
        ops.extend(finish_line(&[16]));
        programs.push(ops);
    }
    let initial = setup.initial();
    for program in &programs {
        prevalidate_with(program, NSLOTS, per_owner as u32, STEP_CAP, &initial)
            .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    }
    let growth = HtGrowth::new(owners);
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.growth = Some(&growth);
    let (lines, dumps) = run_ht_overlap_session(
        &setup,
        &programs,
        1,
        per_owner as u32,
        &env,
        6860,
        &[
            "(hash-table-count a6-tB)",
            "(gethash 1000 a6-tB)",
            "(gethash 4039 a6-tB)",
        ],
    );
    assert_eq!(lines.len(), owners);
    assert_eq!(dumps[0], "160", "every racing insert landed exactly once");
    assert_eq!(dumps[1], "1000", "owner 0's first key");
    assert_eq!(dumps[2], "4039", "owner 3's last key");
    let total = owners * per_owner;
    let events: usize = (0..owners)
        .map(|o| growth.events[o].load(Ordering::Relaxed))
        .sum();
    assert!(
        events >= 3,
        "160 inserts must cross several resizes (events: {events})"
    );
    assert_eq!(
        growth.mismatches.load(Ordering::Relaxed),
        0,
        "every racing put charged exactly its own storage growth"
    );
    let charged: usize = (0..owners)
        .map(|o| growth.charged[o].load(Ordering::Relaxed))
        .sum();
    assert!(charged > 0);
    let entry_bytes = total * size_of::<Option<HashTableEntry>>();
    assert!(
        charged <= 8 * entry_bytes,
        "amortized bound: charged {charged} > 8x {entry_bytes}"
    );
}

/// B5 row 3: memory overhead (§7 B5). One owner grows a table through 100
/// inserts and shrinks it by 20 rems: the charge must be at least the
/// final entry footprint (the backings really were allocated) and within
/// the amortized 8× bound. The serial arm then proves the other half of
/// Rev R3: the identical put loop on a fresh versus an already-grown
/// table, in plain serial Sim, charges the heap identically — growth in
/// Sim stays the current code path, byte for byte.
#[test]
fn a6_ht_growth_memory_overhead() {
    // slots: 0 table; 15 one; 16 value/count/found; 17 limit/key; 18 k;
    // 19 cond.
    const TM: u16 = 0;
    const NSLOTS: usize = 20;
    let puts: usize = 100;
    let rems: usize = 20;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tM2 (make-hash-table :test 'eq))
  (list a6-tM2))"#,
        facts: vec![(TM, Abs::Table)],
        nslots: NSLOTS,
    };
    let mut ops = vec![
        Op::ConstFixnum {
            dst: 17,
            value: puts as i64,
        },
        Op::ConstFixnum { dst: 18, value: 0 },
        Op::ConstFixnum { dst: 15, value: 1 },
    ];
    let loop_at = ops.len() as u16;
    ops.push(Op::Lt {
        dst: 19,
        a: 18,
        b: 17,
    });
    let branch_at = ops.len();
    ops.push(Op::BranchIfNil {
        cond: 19,
        target: 0,
    });
    ops.push(Op::Add {
        dst: 16,
        a: 18,
        b: 18,
    });
    ops.push(Op::Add {
        dst: 16,
        a: 16,
        b: 18,
    });
    ops.push(Op::HtPut {
        table: TM,
        key: 18,
        value: 16,
    });
    ops.push(Op::Add {
        dst: 18,
        a: 18,
        b: 15,
    });
    ops.push(Op::Jump { target: loop_at });
    let done = ops.len() as u16;
    ops.push(Op::ConstFixnum {
        dst: 17,
        value: rems as i64,
    });
    ops.push(Op::ConstFixnum { dst: 18, value: 0 });
    let loop2_at = ops.len() as u16;
    ops.push(Op::Lt {
        dst: 19,
        a: 18,
        b: 17,
    });
    let branch2_at = ops.len();
    ops.push(Op::BranchIfNil {
        cond: 19,
        target: 0,
    });
    ops.push(Op::HtRem {
        table: TM,
        key: 18,
        found: 16,
    });
    ops.push(Op::Add {
        dst: 18,
        a: 18,
        b: 15,
    });
    ops.push(Op::Jump { target: loop2_at });
    let done2 = ops.len() as u16;
    ops.push(Op::HtCount { table: TM, dst: 16 });
    ops[branch_at] = Op::BranchIfNil {
        cond: 19,
        target: done,
    };
    ops[branch2_at] = Op::BranchIfNil {
        cond: 19,
        target: done2,
    };
    ops.extend(finish_line(&[16]));
    let initial = setup.initial();
    prevalidate_with(&ops, NSLOTS, (puts + rems) as u32, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let growth = HtGrowth::new(1);
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.growth = Some(&growth);
    let (lines, _dumps) = run_ht_session(
        &setup,
        vec![ops],
        (puts + rems) as u32,
        &env,
        &["(hash-table-count a6-tM2)"],
    );
    assert_eq!(lines[0], format!("({})", puts - rems), "inserts minus rems");
    assert!(
        growth.events[0].load(Ordering::Relaxed) >= 3,
        "100 inserts cross several resizes"
    );
    assert_eq!(growth.mismatches.load(Ordering::Relaxed), 0);
    let charged = growth.charged[0].load(Ordering::Relaxed);
    let entry_bytes = (puts - rems) * size_of::<Option<HashTableEntry>>();
    assert!(
        charged >= entry_bytes,
        "the backings were really allocated: charged {charged} < {entry_bytes}"
    );
    assert!(
        charged <= 8 * entry_bytes,
        "amortized bound: charged {charged} > 8x {entry_bytes}"
    );

    // Serial arm: the same closure putting 100 fresh keys (growing) versus
    // 100 overwrites (no growth) charges the serial heap identically.
    let mut context = Context::new();
    context
        .eval_str(
            r#"(progn
  (setq a6-tZ1 (make-hash-table :test 'eq))
  (setq a6-tZ2 (make-hash-table :test 'eq))
  (fset 'a6-arm (lambda (tbl)
    (let ((k 0))
      (while (< k 100)
        (puthash k 0 tbl)
        (setq k (+ k 1))))))
  (a6-arm a6-tZ2))"#,
        )
        .unwrap_or_else(|error| panic!("serial arm setup: {error:?}"));
    let before_growing = context.tagged_heap.bytes_since_gc();
    context
        .eval_str("(a6-arm a6-tZ1)")
        .unwrap_or_else(|error| panic!("growing arm: {error:?}"));
    let growing = context.tagged_heap.bytes_since_gc() - before_growing;
    let before_steady = context.tagged_heap.bytes_since_gc();
    context
        .eval_str("(a6-arm a6-tZ2)")
        .unwrap_or_else(|error| panic!("steady arm: {error:?}"));
    let steady = context.tagged_heap.bytes_since_gc() - before_steady;
    assert_eq!(
        growing, steady,
        "serial growth stays uncharged: Sim is byte-for-byte the current path"
    );
}

/// B5 row 4: the serial twin of the growth program against the GNU 31.1
/// oracle (§7 B5). One owner runs the twin's P11 exactly — 130 inserts,
/// count, get, two rems, count — through sealed leases; the answers match
/// the oracle line, and the growth accounting holds underneath.
#[test]
fn a6_ht_growth_serial_twin_matches_gnu_oracle() {
    // slots: 0 tK; 15 one; 16 value/count; 17 limit/key; 18 k/found; 19
    // cond/get-value.
    const TK: u16 = 0;
    const NSLOTS: usize = 20;
    let puts: usize = 130;
    let setup = HtSetup {
        source: r#"(progn
  (setq a6-tK (make-hash-table :test 'eq))
  (list a6-tK))"#,
        facts: vec![(TK, Abs::Table)],
        nslots: NSLOTS,
    };
    let mut ops = vec![
        Op::ConstFixnum {
            dst: 17,
            value: puts as i64,
        },
        Op::ConstFixnum { dst: 18, value: 0 },
        Op::ConstFixnum { dst: 15, value: 1 },
    ];
    let loop_at = ops.len() as u16;
    ops.push(Op::Lt {
        dst: 19,
        a: 18,
        b: 17,
    });
    let branch_at = ops.len();
    ops.push(Op::BranchIfNil {
        cond: 19,
        target: 0,
    });
    ops.push(Op::Add {
        dst: 16,
        a: 18,
        b: 18,
    });
    ops.push(Op::Add {
        dst: 16,
        a: 16,
        b: 18,
    });
    ops.push(Op::HtPut {
        table: TK,
        key: 18,
        value: 16,
    });
    ops.push(Op::Add {
        dst: 18,
        a: 18,
        b: 15,
    });
    ops.push(Op::Jump { target: loop_at });
    let done = ops.len() as u16;
    // The first count keeps its own slot: the second HtCount would
    // otherwise overwrite it before the line is built.
    ops.push(Op::HtCount { table: TK, dst: 10 });
    ops.push(Op::ConstFixnum {
        dst: 17,
        value: 129,
    });
    ops.push(Op::HtGet {
        table: TK,
        key: 17,
        found: 18,
        value: 19,
    });
    ops.push(Op::ConstFixnum { dst: 17, value: 5 });
    ops.push(Op::HtRem {
        table: TK,
        key: 17,
        found: 18,
    });
    ops.push(Op::ConstFixnum { dst: 17, value: 6 });
    ops.push(Op::HtRem {
        table: TK,
        key: 17,
        found: 18,
    });
    ops.push(Op::HtCount { table: TK, dst: 16 });
    ops[branch_at] = Op::BranchIfNil {
        cond: 19,
        target: done,
    };
    ops.extend(finish_line(&[10, 19, 16]));
    let initial = setup.initial();
    prevalidate_with(&ops, NSLOTS, puts as u32, STEP_CAP, &initial)
        .unwrap_or_else(|error| panic!("prevalidate: {error:?}"));
    let growth = HtGrowth::new(1);
    let mut env = HtEnv::new(&[], HT_CB_KEY, HT_CB_CAND, HT_CB_OUT);
    env.growth = Some(&growth);
    let (lines, _dumps) = run_ht_session(
        &setup,
        vec![ops],
        puts as u32,
        &env,
        &["(hash-table-count a6-tK)"],
    );
    let oracle = oracle_lines();
    assert_eq!(lines[0], oracle[34], "P11 growth twin, GNU 31.1");
    assert!(
        growth.events[0].load(Ordering::Relaxed) >= 3,
        "130 inserts cross several resizes"
    );
    assert_eq!(growth.mismatches.load(Ordering::Relaxed), 0);
    let charged = growth.charged[0].load(Ordering::Relaxed);
    let entry_bytes = 128 * size_of::<Option<HashTableEntry>>();
    assert!(charged >= entry_bytes && charged <= 8 * entry_bytes);
}

#[path = "restricted_lisp/gc_tables.rs"]
mod gc_tables;

#[path = "restricted_lisp/allocation_tables.rs"]
mod allocation_tables;

#[path = "restricted_lisp/copy_oracle.rs"]
mod copy_oracle;

mod weak_copy;

mod weak_tables;

mod weak_oracle;
