//! The call-shape census (`NEOVM_JIT_CALL_CENSUS=on`, P1.0 S2.5): which
//! callee shapes JIT call sites meet, counted per call, to decide which
//! direct-call shapes (design `p1-1-direct-native-calls` Stage 2) are worth
//! building -- 2c (framed callees) only if at least 30% of armed calls are
//! framed.
//!
//! Under the knob every JIT `Op::Call`/`Op::Apply` site first calls
//! [`neovm_jit_call_census`] (through the lazy shim table; JIT only) with
//! its kind and the word that names its callee, and the shim classifies the
//! callee from state the call itself would read: a named spec site from its
//! slot (the leaf and key flags its previous calls armed), a call of a value
//! from the callee's armed leaf. The shim reads only -- no allocation, no
//! heat, no arming -- so the census changes what runs only by the call
//! itself. The counts print in the `[neovm-jit-final-builtin-leaves]` line
//! under `NEOVM_JIT_COMPILE_STATS=1`. Off, nothing is emitted.
//!
//! Threading: each mutator reads its own active Context, slots and cache
//! leaves; no Lisp state is cached here. The counters are process-wide
//! relaxed atomics: concurrent increments cannot be lost. A snapshot taken
//! while mutators run need not describe one instant across all cells.

use super::*;

/// Who a counted call site calls.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CensusSite {
    /// A speculated byte-code callee named by a symbol (`SpecCalleeKind::Bytecode`).
    Named = 0,
    /// A call whose callee is provably a constant (a `cl-flet` local, a
    /// `lambda` literal) that the fuser left a call.
    Constant = 1,
    /// Any other `Op::Call` of a computed callee (a closure in a variable or
    /// an argument, a symbol the compile did not speculate on).
    Value = 2,
    /// `Op::Apply`, or an `Op::Call` of the symbol `apply`: the shape is the
    /// applied function's (its arity regardless of the spread).
    Apply = 3,
}

const SITES: usize = 4;

impl CensusSite {
    const ALL: [CensusSite; SITES] = [
        CensusSite::Named,
        CensusSite::Constant,
        CensusSite::Value,
        CensusSite::Apply,
    ];

    fn name(self) -> &'static str {
        match self {
            CensusSite::Named => "named",
            CensusSite::Constant => "const",
            CensusSite::Value => "value",
            CensusSite::Apply => "apply",
        }
    }

    fn from_word(word: i64) -> Option<Self> {
        Self::ALL.get(usize::try_from(word).ok()?).copied()
    }
}

/// The shape of one counted call's callee.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum CallShape {
    /// A frameless leaf taking the call as laid out, entered directly from
    /// the site (a named site's armed direct entry).
    ExactDirect = 0,
    /// A frameless leaf taking the call as laid out, through a shim.
    Exact = 1,
    /// A frameless `&optional` leaf called short of its slots, entered
    /// directly from the site (`NEOVM_JIT_DIRECT_SHAPES=optional`).
    OptionalDirect = 2,
    /// A frameless `&optional` leaf called short of its slots.
    Optional = 3,
    /// A frameless `&rest` leaf, entered directly from the site
    /// (`NEOVM_JIT_DIRECT_SHAPES=rest`).
    RestDirect = 4,
    /// A frameless `&rest` leaf.
    Rest = 5,
    /// A leaf with a frame of its own (dynamic bindings or handlers).
    Framed = 6,
    /// A byte-code callee with no armed leaf yet (not compiled, or a named
    /// site's first call).
    Unarmed = 7,
    /// A compiled callee the fast paths decline (an arity the leaf rejects,
    /// wider than the shim's frame buffer).
    Declined = 8,
    /// Not byte code (a builtin, an interpreted closure, a symbol).
    Other = 9,
}

const SHAPES: usize = 10;

impl CallShape {
    const ALL: [CallShape; SHAPES] = [
        CallShape::ExactDirect,
        CallShape::Exact,
        CallShape::OptionalDirect,
        CallShape::Optional,
        CallShape::RestDirect,
        CallShape::Rest,
        CallShape::Framed,
        CallShape::Unarmed,
        CallShape::Declined,
        CallShape::Other,
    ];

    fn name(self) -> &'static str {
        match self {
            CallShape::ExactDirect => "exact_direct",
            CallShape::Exact => "exact",
            CallShape::OptionalDirect => "optional_direct",
            CallShape::Optional => "optional",
            CallShape::RestDirect => "rest_direct",
            CallShape::Rest => "rest",
            CallShape::Framed => "framed",
            CallShape::Unarmed => "unarmed",
            CallShape::Declined => "declined",
            CallShape::Other => "other",
        }
    }
}

#[allow(clippy::declare_interior_mutable_const)]
const ZERO: AtomicU64 = AtomicU64::new(0);
#[allow(clippy::declare_interior_mutable_const)]
const ROW: [AtomicU64; SHAPES] = [ZERO; SHAPES];

/// Calls counted, by site kind and callee shape.
static COUNTS: [[AtomicU64; SHAPES]; SITES] = [ROW; SITES];

/// Exact accepted spec-shim entries. Only the compile-time-selected counted
/// shim updates these; the ordinary shim has no counter or runtime knob test.
/// Threading: process-wide diagnostics, relaxed atomic increments from every
/// mutator; snapshots are nontransactional and contain no Lisp state.
static SPEC_FAST_ARMED: AtomicU64 = AtomicU64::new(0);
static SPEC_FAST_FRAMED: AtomicU64 = AtomicU64::new(0);

#[inline]
pub(super) fn record_spec_fast_for_census(framed: bool) {
    SPEC_FAST_ARMED.fetch_add(1, Ordering::Relaxed);
    if framed {
        SPEC_FAST_FRAMED.fetch_add(1, Ordering::Relaxed);
    }
}

#[cfg(test)]
fn spec_fast_counts_for_test() -> (u64, u64) {
    (
        SPEC_FAST_ARMED.load(Ordering::Relaxed),
        SPEC_FAST_FRAMED.load(Ordering::Relaxed),
    )
}

#[cfg(test)]
fn reset_for_test() {
    for row in &COUNTS {
        for count in row {
            count.store(0, Ordering::Relaxed);
        }
    }
    SPEC_FAST_ARMED.store(0, Ordering::Relaxed);
    SPEC_FAST_FRAMED.store(0, Ordering::Relaxed);
}

/// The shape of a named site's callee, from its spec slot.
fn named_shape(slot: &SpecSlot) -> CallShape {
    let leaf = slot.leaf_ptr();
    if leaf.is_null() {
        return CallShape::Unarmed;
    }
    let key = slot.direct_consts.load(Ordering::Relaxed);
    if key == 0 {
        return CallShape::Declined;
    }
    // SAFETY: a slot's leaf names a live or retired cache leaf, which stays
    // allocated (`resolve_compiled_leaf_ptr`).
    let leaf = unsafe { &*leaf };
    // KEY_FRAMED also covers a frameless AOT leaf with a sidecar. Stage
    // 2c's census concerns bindings and handlers, not the AOT entry ABI.
    if leaf.has_binds || leaf.has_handlers {
        return CallShape::Framed;
    }
    let direct = slot.direct_entry.load(Ordering::Relaxed) != 0;
    match (leaf.has_rest, key & SpecSlot::KEY_SHORT_CALL != 0, direct) {
        (true, _, true) => CallShape::RestDirect,
        (true, _, false) => CallShape::Rest,
        (false, true, true) => CallShape::OptionalDirect,
        (false, true, false) => CallShape::Optional,
        (false, false, true) => CallShape::ExactDirect,
        (false, false, false) => CallShape::Exact,
    }
}

/// The armed leaf of a called value, when it is byte code with one.
fn armed_leaf_of(callee: Value) -> Result<&'static CompiledLeaf, CallShape> {
    if !callee.is_bytecode() {
        return Err(CallShape::Other);
    }
    // Never materialize a mapped stub here: the census only reads.
    let bc = callee
        .bytecode_data_if_materialized()
        .ok_or(CallShape::Unarmed)?;
    let epoch = crate::emacs_core::jit::cache::leaf_slot_epoch();
    let leaf = bc
        .jit_runtime()
        .armed_leaf_slot(epoch)
        .ok_or(CallShape::Unarmed)?;
    // SAFETY: armed under the current leaf-slot epoch (every retire and
    // clear bumps it, and retired leaves stay allocated); the census reads
    // it before the call returns.
    Ok(unsafe { &*leaf })
}

/// The shape of a called value with `nargs` arguments, from its armed leaf.
fn value_shape(callee: Value, nargs: usize) -> CallShape {
    let leaf = match armed_leaf_of(callee) {
        Ok(leaf) => leaf,
        Err(shape) => return shape,
    };
    if !leaf.accepts(nargs) {
        CallShape::Declined
    } else if leaf.has_binds || leaf.has_handlers {
        CallShape::Framed
    } else if leaf.has_rest {
        CallShape::Rest
    } else if nargs < leaf.arity {
        CallShape::Optional
    } else {
        CallShape::Exact
    }
}

/// The shape of a function `apply` spreads arguments into: its lambda
/// list's, whatever the spread's length.
fn applied_shape(function: Value) -> CallShape {
    let leaf = match armed_leaf_of(function) {
        Ok(leaf) => leaf,
        Err(shape) => return shape,
    };
    if leaf.has_binds || leaf.has_handlers {
        CallShape::Framed
    } else if leaf.has_rest {
        CallShape::Rest
    } else if leaf.required < leaf.arity {
        CallShape::Optional
    } else {
        CallShape::Exact
    }
}

/// Count one call (see the module docs). `site` is a [`CensusSite`]; `word`
/// is a named site's slot address, or the called value's bits; `arg0` the
/// call's first argument (nil when it has none); `nargs` the call's
/// argument count. A called symbol is classified by its function cell.
///
/// SAFETY: called only by generated code, with its leaf's dormant Context
/// and a `Named` site's own live spec slot.
#[allow(clippy::not_unsafe_ptr_arg_deref)]
#[cold]
#[inline(never)]
#[unsafe(no_mangle)]
pub(crate) extern "C" fn neovm_jit_call_census(
    ctx: *mut u8,
    site: i64,
    word: i64,
    arg0: i64,
    nargs: i64,
) {
    let Some(mut site) = CensusSite::from_word(site) else {
        return;
    };
    // SAFETY: the dormant seam Context (the contract above); reads only.
    let ctx = unsafe { &*(ctx as *const crate::emacs_core::eval::Context) };
    // A symbol names its function cell's contents (no alias chasing: an
    // alias counts as `other`).
    let resolve = |v: Value| match v.as_symbol_id() {
        Some(sym) => ctx.obarray.symbol_function_id(sym).unwrap_or(Value::NIL),
        None => v,
    };
    let nargs = nargs as usize;
    let mut callee = Value::from_bits(word as usize);
    if site == CensusSite::Value
        && callee.as_symbol_id() == Some(crate::emacs_core::bytecode::Vm::apply_builtin_id())
    {
        site = CensusSite::Apply;
        // A call of the builtin carries `apply` as its callee and the
        // applied function as arg0. Op::Apply already carries the applied
        // function itself, which may be a symbol too.
        callee = Value::from_bits(arg0 as usize);
    }
    let shape = match site {
        CensusSite::Named => {
            // SAFETY: the executing leaf's slot (the contract above).
            named_shape(unsafe { &*(word as usize as *const SpecSlot) })
        }
        CensusSite::Constant | CensusSite::Value => value_shape(resolve(callee), nargs),
        CensusSite::Apply => applied_shape(resolve(callee)),
    };
    COUNTS[site as usize][shape as usize].fetch_add(1, Ordering::Relaxed);
}

/// Emit a census call for the site about to be lowered (the knob is on and
/// the build is JIT): `word` is the slot address (`Named`) or the callee,
/// `args` the call's arguments.
pub(crate) fn emit_census_call(
    fb: &mut FunctionBuilder,
    rt: &super::lowering::RtCtx,
    site: CensusSite,
    word: ClifValue,
    args: &[ClifValue],
) {
    let census = rt
        .refs
        .try_get(fb.func, Shim::CallCensus)
        .expect("the enabled JIT census declares its shim");
    let site_v = fb.ins().iconst(types::I64, site as i64);
    let arg0 = match args.first() {
        Some(&a) => a,
        None => fb.ins().iconst(types::I64, Value::NIL.bits() as i64),
    };
    let n_v = fb.ins().iconst(types::I64, args.len() as i64);
    let vmctx = fb.use_var(rt.vmctx_var);
    fb.ins().call(census, &[vmctx, site_v, word, arg0, n_v]);
}

/// Emit the census call of the `Op::Call`/`Op::Apply` site `op` about to be
/// lowered (the knob is on and the build is JIT): `spec` is the site's
/// speculation, `callee_and_args` the stack's `[f a1 .. aN]`. Builtin
/// speculations are not counted.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_site_census(
    fb: &mut FunctionBuilder,
    rt: &super::lowering::RtCtx,
    op: &Op,
    spec: Option<(u32, u64, i64, usize, SpecCalleeKind)>,
    callee_and_args: &[ClifValue],
    reloc_base: Option<ClifValue>,
    reloc_index: &HashMap<usize, u32>,
) {
    let Some((&callee, args)) = callee_and_args.split_first() else {
        return;
    };
    let (site, word) = match (op, spec) {
        (Op::Apply(_), _) => (CensusSite::Apply, callee),
        (_, Some((_, _, slot_ptr, _, SpecCalleeKind::Bytecode))) => {
            (CensusSite::Named, fb.ins().iconst(types::I64, slot_ptr))
        }
        (_, Some((_, _, _, _, SpecCalleeKind::Source))) => (CensusSite::Value, callee),
        (_, Some((_, _, _, _, SpecCalleeKind::Constant))) => (CensusSite::Constant, callee),
        (_, Some(_)) => return,
        (_, None) => {
            let constant = super::lowering::const_value_bits(fb, callee, reloc_base, reloc_index)
                .is_some_and(|bits| Value::from_bits(bits as usize).is_bytecode());
            if constant {
                (CensusSite::Constant, callee)
            } else {
                (CensusSite::Value, callee)
            }
        }
    };
    emit_census_call(fb, rt, site, word, args);
}

/// The counts so far, `(site, shape, calls)` for every nonzero cell.
pub(crate) fn census_counts() -> Vec<(&'static str, &'static str, u64)> {
    let mut out = Vec::new();
    for site in CensusSite::ALL {
        for shape in CallShape::ALL {
            let n = COUNTS[site as usize][shape as usize].load(Ordering::Relaxed);
            if n > 0 {
                out.push((site.name(), shape.name(), n));
            }
        }
    }
    out
}

/// The census entry of the `[neovm-jit-final-builtin-leaves]` line, when a
/// call was counted: `call-census:SITE.SHAPE=N,...`.
pub(crate) fn render_call_census() -> Option<String> {
    let counts = census_counts();
    (!counts.is_empty()).then(|| {
        let cells: Vec<String> = counts
            .into_iter()
            .map(|(site, shape, n)| format!("{site}.{shape}={n}"))
            .collect();
        format!(
            "call-census:{} spec-shim-fast:armed={},framed={}",
            cells.join(","),
            SPEC_FAST_ARMED.load(Ordering::Relaxed),
            SPEC_FAST_FRAMED.load(Ordering::Relaxed),
        )
    })
}

#[cfg(test)]
#[path = "call_census/tests/call_census_test.rs"]
mod tests;
