//! The JIT's compile-time knobs: each reads its `NEOVM_JIT_*` variable once
//! (see the knob table in `jit/mod.rs`), and tests override it per thread.
//! Every emission a knob gates is decided at compile time, so both sides of
//! an A/B run in one binary.

#[cfg(test)]
thread_local! {
    /// Per-thread override for the profitability gate, set by tests that need to
    /// compile a deliberately call-dominated body to exercise the call/spec
    /// machinery (which production would correctly decline to compile).
    static PROFIT_GATE_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force the profitability gate on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_profit_gate_for_test(on: bool) {
    PROFIT_GATE_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the JIT profitability gate enabled? Default yes; `NEOVM_JIT_PROFIT=off`
/// disables it, so the gate can be A/B-measured against the old behavior in a
/// single build.
pub(crate) fn jit_profit_gate_on() -> bool {
    #[cfg(test)]
    if let Some(o) = PROFIT_GATE_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_PROFIT").as_deref() != Ok("off"))
}

#[cfg(test)]
std::thread_local! {
    static GATE_RELAX_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the call-heavy gate-relaxation on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_gate_relax_for_test(on: bool) {
    GATE_RELAX_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Is the call-heavy gate relaxation enabled? **Default NO** — reverted to the
/// conservative `calls <= arith` on 2026-07-21 after it regressed byte-compilation
/// (see below). `NEOVM_JIT_GATE_RELAX=on` opts in (stops counting user-function
/// `Op::Call`/`Op::Apply` against profitability, so user-call-heavy bodies tier).
///
/// HISTORY: briefly default-ON (commit 20cb6190a). Motivating measurements looked
/// good on SYNTHETICS — a hot user-fn call loop 2.31x, trivial-builtin 1.21x, real
/// font-lock 1.013x (neutral) — and the oracle suite showed zero new failures. But
/// those synthetics were unrepresentative: they run the SAME hot body enough to
/// amortize the JIT compile cost. The reverted-to conservative gate exists
/// precisely to protect BYTE-COMPILATION (call-heavy, builtin-heavy, ~one-shot),
/// and a proper `perf stat instructions:u` A/B (byte-compile cl-macs.el x8,
/// release) measured the flip **21% SLOWER** (22.56B on vs 18.58B off, ratio 1.214
/// x3 runs): with the gate on, ~9% goes to runtime regalloc2+cranelift compilation
/// that never amortizes because native ≈ interp for builtin-heavy code (the
/// font-lock 1.013x), so the compile tax is pure loss. LESSON: measure the
/// workload the gate was DESIGNED for (byte-compile), not a favorable synthetic.
/// The right long-term path is AOT (compile the standard library at build time so
/// it never runtime-compiles) + a gate that distinguishes amortizing from
/// one-shot hot bodies — not this blanket relaxation. Knob kept for hot long-
/// running user-call-heavy loops that genuinely amortize.
pub(super) fn jit_gate_relax_on() -> bool {
    #[cfg(test)]
    if let Some(o) = GATE_RELAX_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_GATE_RELAX").as_deref() == Ok("on"))
}

/// Answer a record's `type-of` / `cl-type-of` inline at an armed JIT site
/// (`lowering::emit_inline_record_type_of`). Default on;
/// `NEOVM_JIT_INLINE_TYPE_OF=off` calls `neovm_jit_pred_spec` at every site.
pub(crate) fn jit_inline_type_of_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_TYPE_OF").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Read a plain vector's or record's slot inline at JIT `aref` sites
/// (`lowering::emit_inline_aref`). Default on; `NEOVM_JIT_INLINE_AREF=off`
/// calls `neovm_jit_aref` at every site — the single-build A/B.
pub(crate) fn jit_inline_aref_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_AREF").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
std::thread_local! {
    static AREF_SKIP_SLOT0_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOVM_JIT_AREF_SKIP_SLOT0` on/off on the current thread (tests
/// only).
#[cfg(test)]
pub(crate) fn force_aref_skip_slot0_for_test(on: Option<bool>) {
    AREF_SKIP_SLOT0_TEST_OVERRIDE.with(|c| c.set(on));
}

/// MEASUREMENT ONLY (falsifier F-G (b), design P3.2 F1a): inline
/// `aref`/`aset` of a vector or record without the slot-0 test that tells a
/// tagged bool-vector or legacy char-table from a plain vector
/// (`lowering::emit_plain_slot_address`), and the `aset` shim's fast path
/// without `classify_vector_slots` (`dispatch::aset_fast`). This is what
/// the inline sequences cost once P3.2 L0 retires every tagged vector.
/// Default off; `NEOVM_JIT_AREF_SKIP_SLOT0=1` turns it on. With it on,
/// `aref`/`aset` of a tagged vector answer wrongly, so a run is valid only
/// when its hot paths create none: every tagged vector created while it is
/// on is counted and the first one logs a warning
/// ([`note_tagged_vector_under_skip_slot0`]).
pub(crate) fn jit_aref_skip_slot0_on() -> bool {
    #[cfg(test)]
    if let Some(o) = AREF_SKIP_SLOT0_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        matches!(
            std::env::var("NEOVM_JIT_AREF_SKIP_SLOT0").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        )
    })
}

/// Tagged vectors (bool-vectors) created while `NEOVM_JIT_AREF_SKIP_SLOT0`
/// is on: a nonzero count means the run's inline `aref`/`aset` may have
/// answered wrongly on one of them.
pub(crate) static TAGGED_VECTORS_UNDER_SKIP_SLOT0: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

/// A tagged vector is being created: under `NEOVM_JIT_AREF_SKIP_SLOT0`,
/// count it and warn once (the measurement run is suspect).
#[cold]
#[inline(never)]
pub(crate) fn note_tagged_vector_under_skip_slot0() {
    if !jit_aref_skip_slot0_on() {
        return;
    }
    let before = TAGGED_VECTORS_UNDER_SKIP_SLOT0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if before == 0 {
        tracing::warn!(
            "NEOVM_JIT_AREF_SKIP_SLOT0 is on and a tagged vector (bool-vector) was created: \
             inline aref/aset on it answer wrongly, so this run is not a valid measurement"
        );
    }
}

/// Store `setcar`/`setcdr` inline at JIT sites when the cons lies outside
/// the write barrier's owner window (`heap_inline::emit_inline_cons_store`),
/// and `aset` of an owned plain vector or record the barrier need not see
/// (`heap_inline::emit_inline_aset`). Default on;
/// `NEOVM_JIT_INLINE_HEAP_WRITE=off` calls `neovm_jit_setcar`/`_setcdr`/
/// `_aset` at every site — the single-build A/B.
pub(crate) fn jit_inline_heap_write_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_HEAP_WRITE").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Allocate conses and box floats inline at JIT sites, bumping the heap's
/// open allocation region (`heap_inline::emit_inline_cons` /
/// `emit_inline_box_float`). Default on; `NEOVM_JIT_INLINE_ALLOC=off` calls
/// `neovm_jit_cons`/`neovm_jit_make_float` at every site — the single-build
/// A/B.
pub(crate) fn jit_inline_alloc_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_ALLOC").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
std::thread_local! {
    static INLINE_SWITCH_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force inline jump-table dispatch on/off for compiles on the current
/// thread (tests only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_inline_switch_for_test(on: Option<bool>) {
    INLINE_SWITCH_TEST_OVERRIDE.with(|c| c.set(on));
}

/// Answer a jump table whose keys are immediates, or cons trees of them,
/// inline at its `switch` site, behind the table's mutation epoch
/// (`switch_dispatch::InlineSwitch`), with the lookup shim as the slow path.
/// Default on; `NEOVM_JIT_INLINE_SWITCH=off` calls `neovm_jit_switch` at
/// every site -- the lowering before inline dispatch, CLIF-identical to it:
/// the single-build A/B. Read at compile time only.
pub(crate) fn jit_inline_switch_on() -> bool {
    #[cfg(test)]
    if let Some(on) = INLINE_SWITCH_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_SWITCH").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Where a `Float`-feedback arithmetic result may stay UNBOXED (a raw `f64`
/// in the baseline model stack, `lowering::SlotRep::Flonum`) instead of being
/// boxed by `neovm_jit_make_float` at the site. `NEOVM_JIT_FLONUM=off|local|
/// resident`; one binary answers every mode, for a same-binary A/B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FlonumMode {
    /// Box every result at its site: the lowering before unboxed floats,
    /// CLIF-identical to it.
    Off,
    /// A result stays unboxed while float arithmetic and compares, stack
    /// shuffles and variable reads consume it; every other op boxes all of
    /// them first.
    OpLocal,
    /// [`Self::OpLocal`], and an audited op (a call, `aref`, `aset`, ...;
    /// `lowering::op_keeps_residual_flonums`) boxes only its own operands:
    /// the results below them stay unboxed across it.
    Resident,
}

impl FlonumMode {
    /// The mode when `NEOVM_JIT_FLONUM` is unset (or not a known mode).
    /// `resident`: on nbody it cuts cycles 42% against `off` and 25% against
    /// `local` (same binary, knob A/B), and leaves every other
    /// elisp-benchmarks row unchanged within noise.
    pub(crate) const DEFAULT: Self = Self::Resident;

    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value {
            Some("off" | "0" | "false" | "no") => Self::Off,
            Some("local") => Self::OpLocal,
            Some("resident") => Self::Resident,
            _ => Self::DEFAULT,
        }
    }
}

#[cfg(test)]
std::thread_local! {
    static FLONUM_MODE_TEST_OVERRIDE: std::cell::Cell<Option<FlonumMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force the flonum mode for compiles on the current thread (tests only);
/// `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_flonum_mode_for_test(mode: Option<FlonumMode>) {
    FLONUM_MODE_TEST_OVERRIDE.with(|c| c.set(mode));
}

/// The [`FlonumMode`] compiles use (`NEOVM_JIT_FLONUM`, read once).
pub(crate) fn jit_flonum_mode() -> FlonumMode {
    #[cfg(test)]
    if let Some(mode) = FLONUM_MODE_TEST_OVERRIDE.with(|c| c.get()) {
        return mode;
    }
    use std::sync::OnceLock;
    static MODE: OnceLock<FlonumMode> = OnceLock::new();
    *MODE.get_or_init(|| FlonumMode::parse(std::env::var("NEOVM_JIT_FLONUM").ok().as_deref()))
}

#[cfg(test)]
std::thread_local! {
    static EQ_PREFILTER_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the `eq`/`symbolp` prefilter on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_eq_prefilter_for_test(on: bool) {
    EQ_PREFILTER_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Answer native `eq`/`symbolp` inline unless an operand is a veclike (the
/// only kind a symbol-with-pos can be), calling the slow-path shim for
/// veclikes only. Default on; `NEOVM_JIT_EQ_PREFILTER=off` calls the shim
/// for every mismatching `eq` and every non-symbol `symbolp` — the
/// single-build A/B. Read at compile time only.
pub(crate) fn jit_eq_prefilter_on() -> bool {
    #[cfg(test)]
    if let Some(o) = EQ_PREFILTER_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_EQ_PREFILTER").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Which leaf-builtin emissions `NEOVM_JIT_LEAF` turns on (design
/// `p1-2-builtin-intrinsics`; default OFF until its gate passes). Read at
/// compile time only, so both sides of an A/B run in one binary and the off
/// side emits exactly the former code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct LeafKnob {
    /// Opcode sites (`Op::Get`, `Op::Length`, `Op::Nth`, ...) call their
    /// leaf's bare trampoline instead of the `builtin1`/`builtin2` table shim.
    pub(crate) opcode: bool,
    /// `Op::Call` sites on a builtin with a Bcall leaf (`gethash`,
    /// `plist-get`, `get-char-property`) call its armed trampoline.
    pub(crate) bcall: bool,
    /// String `aref`/`aset` inline (I1/I2).
    pub(crate) string: bool,
}

impl LeafKnob {
    pub(crate) const OFF: Self = Self {
        opcode: false,
        bcall: false,
        string: false,
    };
    pub(crate) const ALL: Self = Self {
        opcode: true,
        bcall: true,
        string: true,
    };

    /// Unset/`on`/`1`/`all`: everything (the default since the F-B and
    /// board measurements); `off`/`0`: nothing, the former code exactly;
    /// otherwise a comma list of `opcode`, `bcall`, `string`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::ALL;
        };
        match value {
            "" | "0" | "off" | "false" | "no" => return Self::OFF,
            "1" | "on" | "all" | "true" | "yes" => return Self::ALL,
            _ => {}
        }
        let mut knob = Self::OFF;
        for part in value.split(',').map(str::trim) {
            match part {
                "opcode" => knob.opcode = true,
                "bcall" => knob.bcall = true,
                "string" => knob.string = true,
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_LEAF: unknown part ignored (expected opcode, bcall, string)"
                ),
            }
        }
        knob
    }
}

#[cfg(test)]
std::thread_local! {
    static LEAF_KNOB_TEST_OVERRIDE: std::cell::Cell<Option<LeafKnob>> = const { std::cell::Cell::new(None) };
    static LEAF_ONLY_TEST_OVERRIDE: std::cell::RefCell<Option<Vec<String>>> = const { std::cell::RefCell::new(None) };
}

/// Force the leaf knob for compiles on the current thread (tests only);
/// `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_leaf_knob_for_test(knob: Option<LeafKnob>) {
    LEAF_KNOB_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// Force the `NEOVM_JIT_LEAF_ONLY` filter on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_leaf_only_for_test(names: Option<&[&str]>) {
    LEAF_ONLY_TEST_OVERRIDE.with(|c| {
        *c.borrow_mut() = names.map(|n| n.iter().map(|s| s.to_string()).collect());
    });
}

/// The `NEOVM_JIT_LEAF` setting compiles use (read once).
pub(crate) fn jit_leaf_knob() -> LeafKnob {
    #[cfg(test)]
    if let Some(knob) = LEAF_KNOB_TEST_OVERRIDE.with(|c| c.get()) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<LeafKnob> = OnceLock::new();
    *KNOB.get_or_init(|| LeafKnob::parse(std::env::var("NEOVM_JIT_LEAF").ok().as_deref()))
}

/// Whether the leaf named `name` may be used at all:
/// `NEOVM_JIT_LEAF_ONLY=<name>,<name>` (the builtins' Lisp names) restricts
/// opcode and Bcall leaf sites to those leaves -- a bisection and
/// per-builtin measurement aid. Unset: every leaf. Read at compile time.
pub(crate) fn jit_leaf_selected(name: &str) -> bool {
    #[cfg(test)]
    if let Some(only) = LEAF_ONLY_TEST_OVERRIDE.with(|c| c.borrow().clone()) {
        return only.iter().any(|n| n == name);
    }
    use std::sync::OnceLock;
    static ONLY: OnceLock<Option<Vec<String>>> = OnceLock::new();
    ONLY.get_or_init(|| {
        std::env::var("NEOVM_JIT_LEAF_ONLY").ok().map(|v| {
            v.split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect()
        })
    })
    .as_ref()
    .is_none_or(|only| only.iter().any(|n| n == name))
}

#[cfg(test)]
std::thread_local! {
    static INLINE_ARITH_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force Level-B inline-arith on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_inline_arith_for_test(on: bool) {
    INLINE_ARITH_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// LEVEL-B: inline `logand`/`logior`/`logxor`/`lognot` (JIT only) as native
/// `band`/`bor`/`bxor`/`ineg` on the TAGGED fixnum bits (the tag `2` survives
/// `&`/`|`, is restored after `^`, and `ineg` maps a tagged fixnum to its
/// `lognot`), guarded by a fixnum check that deopts, instead of the armed
/// `neovm_jit_arith_spec` shim (which marshals 8 args). `mod` inlines its
/// floor-modulo on the untagged values (srem + branchless sign-fixup,
/// zero-divisor deopt). Redefinition is caught by the leaf's `inline_epoch`
/// eviction. `ash` never inlines (overflow/bignum).
/// Default ON since the shim-vs-inline A/B (list workload −11.5% wall, no
/// change elsewhere); `NEOVM_JIT_INLINE_ARITH=off` is the kill switch. AOT
/// always keeps the shim (its loader owns arm/disarm — an inline op has no
/// per-site epoch).
pub(crate) fn jit_inline_arith_on() -> bool {
    #[cfg(test)]
    if let Some(o) = INLINE_ARITH_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_INLINE_ARITH").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Which P2.5 reach admissions the MIR builder may use
/// (`NEOVM_JIT_MIR_REACH`, design `p2-5-mir-reach`; default none). Legacy
/// MIR has one bit, `dead`; the design's other admissions (`args`, `env`,
/// `vars`, `binds`, `switch`, `handlers`) belong to the opt tier's
/// `NEOVM_JIT_OPT_ADMIT` (p2-0-integration §3.2). Read at compile time only,
/// and only by the tier-up compile: the AOT paths and inline callees always
/// build with [`MirReach::OFF`]. With a bit off the builder bails exactly
/// where it did before the bit existed (the per-bit kill switch).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct MirReach {
    /// Skip a block leader no path reaches (the `Return` that `seal_ops`
    /// appends after a body's final `goto`: named-let and `cl-loop` tails)
    /// instead of bailing the body as `mir-unreachable-block`.
    pub(crate) dead: bool,
}

impl MirReach {
    pub(crate) const OFF: Self = Self { dead: false };
    pub(crate) const ALL: Self = Self { dead: true };

    /// `off`/`0`/`none`/unset: nothing; `all`/`on`/`1`: every bit; otherwise
    /// a comma list of bits (`dead`).
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::OFF;
        };
        match value {
            "" | "0" | "off" | "none" | "false" | "no" => return Self::OFF,
            "1" | "on" | "all" | "true" | "yes" => return Self::ALL,
            _ => {}
        }
        let mut reach = Self::OFF;
        for part in value.split(',').map(str::trim) {
            match part {
                "dead" => reach.dead = true,
                "" => {}
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_MIR_REACH: unknown bit ignored (legacy MIR has only `dead`)"
                ),
            }
        }
        reach
    }
}

#[cfg(test)]
std::thread_local! {
    static MIR_REACH_TEST_OVERRIDE: std::cell::Cell<Option<MirReach>> = const { std::cell::Cell::new(None) };
}

/// Force the MIR reach bits for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_mir_reach_for_test(reach: Option<MirReach>) {
    MIR_REACH_TEST_OVERRIDE.with(|c| c.set(reach));
}

/// The `NEOVM_JIT_MIR_REACH` bits tier-up compiles use (read once).
pub(crate) fn jit_mir_reach() -> MirReach {
    #[cfg(test)]
    if let Some(reach) = MIR_REACH_TEST_OVERRIDE.with(|c| c.get()) {
        return reach;
    }
    use std::sync::OnceLock;
    static REACH: OnceLock<MirReach> = OnceLock::new();
    *REACH.get_or_init(|| MirReach::parse(std::env::var("NEOVM_JIT_MIR_REACH").ok().as_deref()))
}

#[cfg(test)]
std::thread_local! {
    static TAIL_UNROOTED_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force tail-call residual elision on/off for compiles on the current
/// thread (tests only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_tail_unrooted_for_test(on: Option<bool>) {
    TAIL_UNROOTED_TEST_OVERRIDE.with(|c| c.set(on));
}

/// A call in tail position roots no residuals
/// (`lowering::tail_call_dead_residuals`). Default on;
/// `NEOVM_JIT_TAIL_UNROOTED=off` roots them as before, CLIF-identical to the
/// lowering without the elision: the single-build A/B. Read at compile time
/// only.
pub(crate) fn jit_tail_unrooted_on() -> bool {
    #[cfg(test)]
    if let Some(on) = TAIL_UNROOTED_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_TAIL_UNROOTED").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// A MIR leaf reads its arguments first in its entry, before the hoisted
/// root-window check, so the argument pointer is not live across that
/// check's cold grow call. Default on; `NEOVM_JIT_ARGS_FIRST=off` reads them
/// last, as before, CLIF-identical: the single-build A/B. Read at compile
/// time only.
pub(crate) fn jit_args_first_on() -> bool {
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_ARGS_FIRST").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}
