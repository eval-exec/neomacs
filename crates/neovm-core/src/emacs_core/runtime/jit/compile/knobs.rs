//! The JIT's compile-time knobs: each reads its `NEOVM_JIT_*` variable once
//! (see the knob table in `jit/mod.rs`), and tests override it per thread.
//! Every emission a knob gates is decided at compile time, so both sides of
//! an A/B run in one binary.

/// Whether an unresolved Switch table participates in ops-only loop policy.
/// Threading: immutable scalar compile configuration; no Lisp state or cache.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, strum::EnumString)]
pub(crate) enum SwitchLoopPolicy {
    #[default]
    #[strum(serialize = "off")]
    DirectOnly,
    #[strum(serialize = "on", serialize = "1")]
    Conservative,
}

pub(super) fn parse_switch_loop_policy(value: Option<&str>) -> SwitchLoopPolicy {
    value
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or_default()
}

/// Read once, before emitting a leaf. Off preserves the previous direct-edge
/// loop policy; on also classifies operandless Switch as a possible loop.
pub(crate) fn jit_switch_loop_policy() -> SwitchLoopPolicy {
    #[cfg(test)]
    if let Some(policy) = SWITCH_LOOP_POLICY_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return policy;
    }
    static POLICY: std::sync::OnceLock<SwitchLoopPolicy> = std::sync::OnceLock::new();
    *POLICY.get_or_init(|| {
        parse_switch_loop_policy(
            std::env::var("NEOVM_JIT_SWITCH_LOOP_POLICY")
                .ok()
                .as_deref(),
        )
    })
}

#[cfg(test)]
thread_local! {
    /// Compiler-test scalar only; never a mutator's Lisp state.
    static SWITCH_LOOP_POLICY_TEST_OVERRIDE: std::cell::Cell<Option<SwitchLoopPolicy>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
#[must_use]
pub(crate) fn switch_loop_policy_scope_for_test(policy: SwitchLoopPolicy) -> impl Drop {
    #[must_use]
    #[derive(Debug)]
    struct Scope {
        previous: Option<SwitchLoopPolicy>,
        _compiler_thread: std::marker::PhantomData<std::rc::Rc<()>>,
    }
    static_assertions::assert_not_impl_any!(Scope: Send, Sync);
    impl Drop for Scope {
        fn drop(&mut self) {
            let _ = SWITCH_LOOP_POLICY_TEST_OVERRIDE.try_with(|current| current.set(self.previous));
        }
    }
    Scope {
        previous: SWITCH_LOOP_POLICY_TEST_OVERRIDE.with(|current| current.replace(Some(policy))),
        _compiler_thread: std::marker::PhantomData,
    }
}

/// Full allocation for bodies with at most this many bytecode ops; zero
/// preserves the original policy. Threading: immutable process configuration,
/// safely initialized once and shared by every mutator's compiler.
pub(crate) fn jit_regalloc_small_max() -> usize {
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        parse_regalloc_small_max(
            std::env::var("NEOVM_JIT_REGALLOC_SMALL_MAX")
                .ok()
                .as_deref(),
        )
    })
}

pub(super) fn parse_regalloc_small_max(value: Option<&str>) -> usize {
    value.and_then(|s| s.trim().parse().ok()).unwrap_or(0)
}

/// P4.2 A5: replace a hot AOT leaf with a full-allocator JIT compile.
/// Threading: immutable process configuration, read by each mutator; the
/// test override contains only a scalar setting, never Lisp state.
pub(crate) fn jit_aot_retier_on() -> bool {
    #[cfg(test)]
    if let Some(on) = AOT_RETIER_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let on = matches!(
            std::env::var("NEOVM_AOT_RETIER").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        );
        if on {
            TIER_UPGRADE_ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        on
    })
}

#[cfg(test)]
thread_local! {
    static AOT_RETIER_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_aot_retier_for_test(on: Option<bool>) {
    AOT_RETIER_TEST_OVERRIDE.with(|setting| setting.set(on));
}

/// P2.3's staged inliner modes. The current stage changes the front's
/// ordering and side tables for existing constant callees only; named,
/// closure and HOF producers arrive in later stages. Threading: immutable
/// process configuration, read only by each mutator's compiler.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Inline2Mode {
    #[default]
    Off,
    Named,
    Closure,
    Hof,
    All,
}

impl Inline2Mode {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("named") => Self::Named,
            Some("closure") => Self::Closure,
            Some("hof") => Self::Hof,
            Some("all") => Self::All,
            _ => Self::Off,
        }
    }

    pub(crate) fn enabled(self) -> bool {
        self != Self::Off
    }
}

/// Compile-time mode; the default preserves the original MIR-first front
/// and all original generated CLIF. It never changes a running leaf.
pub(crate) fn jit_inline2_mode() -> Inline2Mode {
    #[cfg(test)]
    if let Some(mode) = INLINE2_TEST_OVERRIDE.with(|mode| mode.get()) {
        return mode;
    }
    static MODE: std::sync::OnceLock<Inline2Mode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| Inline2Mode::parse(std::env::var("NEOVM_JIT_INLINE2").ok().as_deref()))
}

#[cfg(test)]
thread_local! {
    /// Scalar configuration override, never mutator Lisp state.
    static INLINE2_TEST_OVERRIDE: std::cell::Cell<Option<Inline2Mode>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_inline2_for_test(mode: Option<Inline2Mode>) {
    INLINE2_TEST_OVERRIDE.with(|current| current.set(mode));
}

/// Named planning on the existing re-tier path. Only a compile
/// with re-tier origin or heat pays for named planning. The
/// oracle diagnostic admits the first compile at THRESHOLD=1. Threading:
/// configuration is immutable; heat belongs to this source's atomic state.
pub(crate) fn named_inline_tier_eligible(
    source: &crate::emacs_core::bytecode::ByteCodeFunction,
    origin: crate::emacs_core::jit::stats::CompileOrigin,
) -> bool {
    if !matches!(jit_inline2_mode(), Inline2Mode::Named | Inline2Mode::All) {
        return false;
    }
    static FIRST: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    let first = *FIRST
        .get_or_init(|| std::env::var("NEOVM_JIT_INLINE2_AT_FIRST_COMPILE").as_deref() == Ok("1"));
    first
        || origin == crate::emacs_core::jit::stats::CompileOrigin::Retier
        || crate::emacs_core::jit::retier_heat().is_some_and(|at| source.jit_runtime().heat() >= at)
}

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
    static AREF_SLOT0_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOVM_JIT_AREF_SLOT0` on/off on the current thread (tests only).
#[cfg(test)]
pub(crate) fn force_aref_slot0_for_test(on: Option<bool>) {
    AREF_SLOT0_TEST_OVERRIDE.with(|c| c.set(on));
}

/// MEASUREMENT ONLY (P3.2 L0.9 / U2.10, a same-binary A/B): re-emit the
/// retired slot-0 test in inline `aref`/`aset` of a vector or record
/// (`lowering::emit_plain_slot_address`). The test told a tagged
/// bool-vector or legacy char-table from a plain vector; those in-band tags
/// are gone (P3.2 L0.8), so it only routes a vector whose slot 0 happens to
/// be a tag symbol through the shim, which answers the same slot. Default
/// off; `NEOVM_JIT_AREF_SLOT0=on` measures what the test cost.
pub(crate) fn jit_aref_slot0_on() -> bool {
    #[cfg(test)]
    if let Some(o) = AREF_SLOT0_TEST_OVERRIDE.with(|c| c.get()) {
        return o;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = matches!(
            std::env::var("NEOVM_JIT_AREF_SLOT0").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        );
        if on {
            tracing::info!(
                target: "neovm::jit::knobs",
                "NEOVM_JIT_AREF_SLOT0=on is on in this process (measurement only)"
            );
        }
        on
    })
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

/// Retain collection-write history for observed native GEN0 owners by default.
/// The eager comparison mode retains it on every native store.
/// Read once at compile time (the vector fallback reads the same process knob).
/// Threading: immutable process configuration; the test override is scalar only.
/// Journal state remains owned by the executing mutator, as in the interpreter.
pub(crate) fn jit_gen0_collection_journal_on() -> bool {
    crate::tagged::collection_reads::compiled_journal_mode()
        != crate::tagged::collection_reads::CompiledJournalMode::Off
}

pub(crate) fn jit_gen0_collection_journal_eager() -> bool {
    crate::tagged::collection_reads::compiled_journal_mode()
        == crate::tagged::collection_reads::CompiledJournalMode::Eager
}

#[cfg(test)]
pub(crate) fn force_gen0_collection_journal_for_test(on: Option<bool>) {
    use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};
    force_compiled_journal_for_test(on.map(|on| {
        if on {
            CompiledJournalMode::Observed
        } else {
            CompiledJournalMode::Off
        }
    }));
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
/// `p1-2-builtin-intrinsics`). Read at compile time only, so both sides of
/// an A/B run in one binary and the off side emits exactly the former code.
/// Unset is [`LeafKnob::DEFAULT`]: the parts whose gates passed; a part
/// added since stays off until its own gate passes.
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
    /// The variable leaves (default off): `Op::SymbolValue` sites call the
    /// `symbol-value` leaf's bare trampoline, and `Op::Call` sites on
    /// `buffer-local-value` its armed one (which reads through P1.4 Stage
    /// A's `read_var_cached`).
    pub(crate) vars: bool,
    /// The first leaf batch (default off): `Op::Call` sites on `assoc`,
    /// `rassq`, `delq`, `copy-sequence`, `symbol-name`, `boundp` and
    /// `keywordp` call their armed trampolines.
    pub(crate) batch: bool,
}

impl LeafKnob {
    pub(crate) const OFF: Self = Self {
        opcode: false,
        bcall: false,
        string: false,
        vars: false,
        batch: false,
    };
    /// What an unset knob selects: the parts on by default since the F-B
    /// and board measurements.
    pub(crate) const DEFAULT: Self = Self {
        opcode: true,
        bcall: true,
        string: true,
        vars: false,
        batch: false,
    };
    /// Every part, the default-off ones included.
    pub(crate) const ALL: Self = Self {
        opcode: true,
        bcall: true,
        string: true,
        vars: true,
        batch: true,
    };

    /// Unset: [`Self::DEFAULT`]; `on`/`1`/`all`: every part
    /// ([`Self::ALL`]); `off`/`0`: nothing, the former code exactly;
    /// otherwise a comma list of `opcode`, `bcall`, `string`, `vars`,
    /// `batch`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::DEFAULT;
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
                "vars" => knob.vars = true,
                "batch" => knob.batch = true,
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_LEAF: unknown part ignored \
                     (expected opcode, bcall, string, vars, batch)"
                ),
            }
        }
        knob
    }
}

/// Which CLIF intrinsics `NEOVM_JIT_INTRINSICS` turns on (design
/// `p1-2-builtin-intrinsics` §2.7, `compile/intrinsics.rs`): inline fast
/// paths in front of an opcode site's call. Default OFF; read at compile
/// time only, and off emits exactly the former code (single-build A/B).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct IntrinsicKnob {
    /// I3: `Op::Length` of nil, a short proper list, a string, a plain
    /// vector or record.
    pub(crate) length: bool,
    /// I4: `Op::Nth`/`Op::Nthcdr`/`Op::Elt` of a list at a constant index
    /// in `0..=4`.
    pub(crate) nth: bool,
    /// I5: `Op::Memq`/`Op::Assq`/`Op::Member` (fixnum or bare-symbol key)
    /// over the first 16 conses.
    pub(crate) memq: bool,
    /// I6: `Op::SymbolValue` of a bare symbol with a plain, bound cell.
    pub(crate) symbol_value: bool,
}

impl IntrinsicKnob {
    pub(crate) const OFF: Self = Self {
        length: false,
        nth: false,
        memq: false,
        symbol_value: false,
    };
    pub(crate) const ALL: Self = Self {
        length: true,
        nth: true,
        memq: true,
        symbol_value: true,
    };

    /// Unset/`off`/`0`: nothing; `on`/`1`/`all`: every intrinsic; otherwise
    /// a comma list of `length`, `nth`, `memq`, `symbol-value`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::OFF;
        };
        match value {
            "" | "0" | "off" | "false" | "no" => return Self::OFF,
            "1" | "on" | "all" | "true" | "yes" => return Self::ALL,
            _ => {}
        }
        let mut knob = Self::OFF;
        for part in value.split(',').map(str::trim) {
            match part {
                "length" => knob.length = true,
                "nth" => knob.nth = true,
                "memq" => knob.memq = true,
                "symbol-value" => knob.symbol_value = true,
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_INTRINSICS: unknown part ignored \
                     (expected length, nth, memq, symbol-value)"
                ),
            }
        }
        knob
    }
}

#[cfg(test)]
std::thread_local! {
    static LEAF_EFFECTS_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOVM_JIT_LEAF_EFFECTS` on the current thread (tests only);
/// `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_leaf_effects_for_test(on: Option<bool>) {
    LEAF_EFFECTS_TEST_OVERRIDE.with(|c| c.set(on));
}

/// `NEOVM_JIT_LEAF_EFFECTS=on` (design `p1-2-builtin-intrinsics` §2.8,
/// commit 11): an opcode site whose whole lowering is a leaf or a GC-free
/// shim (`calls::opcode_site_effects` excludes MAY_GC, MAY_REENTER and
/// MAY_DEOPT) no longer keeps a looping body out of the MIR tier
/// (`gate:loop-opaque`), and in a MIR leaf it is no safepoint: the values
/// live across it stay raw and unrooted. Default off; read at compile time
/// (unset/`off` admits and lowers exactly as before).
pub(crate) fn jit_leaf_effects_on() -> bool {
    #[cfg(test)]
    if let Some(on) = LEAF_EFFECTS_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        matches!(
            std::env::var("NEOVM_JIT_LEAF_EFFECTS").ok().as_deref(),
            Some("1" | "on" | "true" | "yes")
        )
    })
}

#[cfg(test)]
std::thread_local! {
    static INTRINSIC_KNOB_TEST_OVERRIDE: std::cell::Cell<Option<IntrinsicKnob>> =
        const { std::cell::Cell::new(None) };
}

/// Force the intrinsic knob for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_intrinsic_knob_for_test(knob: Option<IntrinsicKnob>) {
    INTRINSIC_KNOB_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// The `NEOVM_JIT_INTRINSICS` setting compiles use (read once).
pub(crate) fn jit_intrinsic_knob() -> IntrinsicKnob {
    #[cfg(test)]
    if let Some(knob) = INTRINSIC_KNOB_TEST_OVERRIDE.with(|c| c.get()) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<IntrinsicKnob> = OnceLock::new();
    *KNOB
        .get_or_init(|| IntrinsicKnob::parse(std::env::var("NEOVM_JIT_INTRINSICS").ok().as_deref()))
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

#[cfg(test)]
std::thread_local! {
    static COLD_EXITS_TEST_OVERRIDE: std::cell::Cell<Option<ColdExitsMode>> =
        const { std::cell::Cell::new(None) };
}

/// What `NEOVM_JIT_COLD_EXITS` does to the exit blocks of both tiers
/// (`compile::cold_exits`, P2.2 O0.2). One binary answers every mode, for a
/// same-binary A/B.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ColdExitsMode {
    /// Mark nothing new: CLIF-identical to the lowering before the knob.
    Off,
    /// Mark every exit block cold (precise-deopt and rerun blocks, the
    /// signal exit and handler dispatches, the back-edge poll's slow path,
    /// the root-window grow calls), so Cranelift emits them after the
    /// function's hot code. The CLIF differs from `Off` only in the marks.
    On,
    /// [`Self::On`], and the precise-deopt blocks of a function share one
    /// cold tail that stores the pc, depth and handler cells and returns:
    /// each site keeps only its framestate spill.
    Share,
}

impl ColdExitsMode {
    /// Unset means [`Self::On`] (the default since the same-binary A/B:
    /// cycles -4.6% geomean over bubble, bubble-no-cons, dhrystone, fibn,
    /// flet, inclist and nbody, instructions +0.5%); `off`/`0` restores the
    /// former lowering exactly.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            None | Some("1" | "on" | "true" | "yes") => Self::On,
            Some("share") => Self::Share,
            _ => Self::Off,
        }
    }
}

/// Force the cold-exit mode for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_cold_exits_for_test(mode: Option<ColdExitsMode>) {
    COLD_EXITS_TEST_OVERRIDE.with(|c| c.set(mode));
}

/// `NEOVM_JIT_COLD_EXITS=off|on|share` (default on; see
/// [`ColdExitsMode`]). Read at compile time only.
pub(crate) fn jit_cold_exits() -> ColdExitsMode {
    #[cfg(test)]
    if let Some(mode) = COLD_EXITS_TEST_OVERRIDE.with(|c| c.get()) {
        return mode;
    }
    use std::sync::OnceLock;
    static MODE: OnceLock<ColdExitsMode> = OnceLock::new();
    *MODE
        .get_or_init(|| ColdExitsMode::parse(std::env::var("NEOVM_JIT_COLD_EXITS").ok().as_deref()))
}

/// Whether exit blocks are marked cold (`on` or `share`).
#[inline]
pub(crate) fn jit_cold_exits_on() -> bool {
    jit_cold_exits() != ColdExitsMode::Off
}

#[cfg(test)]
std::thread_local! {
    static REG_ABI_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    static DIRECT_CALL_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
    /// Scalar compiler configuration only, never mutator Lisp state.
    static DIRECT_MEMORY_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

/// Force the register leaf ABI on/off for compiles on the current thread
/// (tests only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_register_abi_for_test(on: Option<bool>) {
    REG_ABI_TEST_OVERRIDE.with(|c| c.set(on));
}

/// Force direct calls on/off for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_direct_call_for_test(on: Option<bool>) {
    DIRECT_CALL_TEST_OVERRIDE.with(|c| c.set(on));
}

/// Force memory-ABI direct calls on/off on the current compiler thread
/// (tests only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_direct_memory_for_test(on: Option<bool>) {
    DIRECT_MEMORY_TEST_OVERRIDE.with(|c| c.set(on));
}

/// `NEOVM_JIT_DIRECT_MEMORY=on`: a direct call may enter an exact
/// pass-through frameless leaf with the existing memory ABI. Default off;
/// when on, direct calls no longer imply the register ABI, while explicit
/// `NEOVM_JIT_REG_ABI=on` retains the register path and its shape reach.
/// Threading: immutable process configuration, read at compile time and
/// slot arming; the test override stores only a compiler-thread boolean.
pub(crate) fn jit_direct_memory_on() -> bool {
    #[cfg(test)]
    if let Some(on) = DIRECT_MEMORY_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| knob_on("NEOVM_JIT_DIRECT_MEMORY"))
}

fn knob_on(name: &str) -> bool {
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1" | "on" | "true" | "yes")
    )
}

/// JIT leaf bodies of at most `reg_abi::MAX_REG_ARGS` argument words take
/// them in registers and return `(value, status)` (`reg_abi`, design
/// `p1-1-direct-native-calls` §3.2). Default OFF; `NEOVM_JIT_REG_ABI=on`
/// turns it on, and [`jit_direct_call_on`] implies it unless
/// [`jit_direct_memory_on`] opts into the existing memory entry or the
/// self-only site policy admits only individually proven self bodies. An
/// explicit register knob keeps its existing global reach in either mode.
/// With direct calls and the explicit register knob off, every entry keeps
/// the memory ABI,
/// CLIF-identical to the lowering before the register ABI existed: the
/// single-build A/B. Read at compile time only.
pub(crate) fn jit_register_abi_on() -> bool {
    #[cfg(test)]
    if let Some(on) = REG_ABI_TEST_OVERRIDE.with(|c| c.get()) {
        return on
            || (jit_direct_call_on()
                && !jit_direct_memory_on()
                && jit_direct_sites() != DirectSitesMode::SelfOnly);
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| knob_on("NEOVM_JIT_REG_ABI"))
        || (jit_direct_call_on()
            && !jit_direct_memory_on()
            && jit_direct_sites() != DirectSitesMode::SelfOnly)
}

/// Speculated calls to compiled byte-code leaves call their register-ABI
/// entry directly from the site (`lowering::emit_direct_bytecode_call`,
/// design `p1-1-direct-native-calls` §3.4), with `neovm_jit_call_spec` as
/// the slow path. Default on with the profitable self-only policy;
/// `NEOVM_JIT_DIRECT_CALL=off` disables it. Other site policies imply the
/// register ABI unless `NEOVM_JIT_DIRECT_MEMORY=on`. Off,
/// no spec slot arms a direct entry and no
/// site emits a direct call: the lowering is CLIF-identical to the one
/// before direct calls, for the single-build A/B. Read when a spec slot is
/// armed (`Vm::call_armed_callee_native`) and at compile time (emission).
pub(crate) fn jit_direct_call_on() -> bool {
    #[cfg(test)]
    if let Some(on) = DIRECT_CALL_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_DIRECT_CALL").ok().as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

/// Per-emitted-site atomic attempt/hit diagnostics. Default off; read only
/// while compiling or arming a cold slot. Threading: immutable process
/// configuration, with no Lisp state or mutator-local cache.
pub(crate) fn jit_direct_profile_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| knob_on("NEOVM_JIT_DIRECT_PROFILE"))
}

#[cfg(test)]
std::thread_local! {
    // Compiler configuration only; no Lisp values or mutator runtime state.
    static DIRECT_SELF_HEAT_TEST_OVERRIDE: std::cell::Cell<Option<super::direct_call::DirectSelfHeat>> = const { std::cell::Cell::new(None) };
    static DIRECT_SELF_KERNEL_TEST_OVERRIDE: std::cell::Cell<Option<bool>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_direct_self_kernel_for_test(on: Option<bool>) {
    DIRECT_SELF_KERNEL_TEST_OVERRIDE.with(|current| current.set(on));
}

/// Admit self sites and their implicit register entry only when the existing
/// profitability classifier does not find more calls than arithmetic.
/// Default on; `NEOVM_JIT_DIRECT_SELF_KERNEL=off` restores unrestricted
/// self admission. Threading: immutable process configuration, compiler-only;
/// the test override is a scalar and never holds Lisp or mutator state.
pub(crate) fn jit_direct_self_kernel_on() -> bool {
    #[cfg(test)]
    if let Some(on) = DIRECT_SELF_KERNEL_TEST_OVERRIDE.with(core::cell::Cell::get) {
        return on;
    }
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        !matches!(
            std::env::var("NEOVM_JIT_DIRECT_SELF_KERNEL")
                .ok()
                .as_deref(),
            Some("0" | "off" | "false" | "no")
        )
    })
}

#[cfg(test)]
pub(crate) fn force_direct_self_heat_for_test(mode: Option<super::direct_call::DirectSelfHeat>) {
    DIRECT_SELF_HEAT_TEST_OVERRIDE.with(|current| current.set(mode));
}

/// Existing source heat required before a self body gains direct sites and
/// an implicit register entry. Default off retains the original policy.
/// Threading: immutable process configuration, read only by the compiler.
pub(crate) fn jit_direct_self_heat() -> super::direct_call::DirectSelfHeat {
    #[cfg(test)]
    if let Some(mode) = DIRECT_SELF_HEAT_TEST_OVERRIDE.with(core::cell::Cell::get) {
        return mode;
    }
    static MODE: std::sync::OnceLock<super::direct_call::DirectSelfHeat> =
        std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        super::direct_call::DirectSelfHeat::parse(
            std::env::var("NEOVM_JIT_DIRECT_SELF_HEAT").ok().as_deref(),
        )
    })
}

/// Which variable ops `NEOVM_JIT_INLINE_VARS` inlines in JIT code (design
/// `p1-4-inline-binding-blv` Stage B, `inline_vars`; default `read`).
/// Read at compile time only, so both sides of an A/B run in one binary
/// and the off side emits exactly the former code.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct InlineVarsKnob {
    /// `varref`: a plain cell read from its baked address, a buffer-local
    /// variable's loaded cache, a forwarder's own slot.
    pub(crate) read: bool,
    /// `varset`: a plain cell, a buffer-local variable's loaded binding or
    /// default, a forwarder's own slot.
    pub(crate) set: bool,
    /// `varbind` and `unbind`: the specpdl entry and the store, in place.
    pub(crate) bind: bool,
}

impl InlineVarsKnob {
    /// The measured parts selected when the environment knob is unset.
    pub(crate) const DEFAULT: Self = Self {
        read: true,
        ..Self::OFF
    };
    pub(crate) const OFF: Self = Self {
        read: false,
        set: false,
        bind: false,
    };
    pub(crate) const ALL: Self = Self {
        read: true,
        set: true,
        bind: true,
    };

    /// Whether any op is inlined.
    pub(crate) fn any(self) -> bool {
        self.read || self.set || self.bind
    }

    /// Unset: `read`; `off`/`0`/`none`: nothing; `all`/`on`/`1`:
    /// everything; otherwise a comma list of `read`, `set`, `bind`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::DEFAULT;
        };
        match value.to_ascii_lowercase().as_str() {
            "" | "0" | "off" | "false" | "no" | "none" => return Self::OFF,
            "1" | "on" | "all" | "true" | "yes" => return Self::ALL,
            _ => {}
        }
        let mut knob = Self::OFF;
        for part in value.split(',').map(str::trim) {
            match part {
                "read" => knob.read = true,
                "set" => knob.set = true,
                "bind" => knob.bind = true,
                "" => {}
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_INLINE_VARS: unknown part ignored (expected read, set, bind)"
                ),
            }
        }
        knob
    }
}

#[cfg(test)]
std::thread_local! {
    static INLINE_VARS_TEST_OVERRIDE: std::cell::Cell<Option<InlineVarsKnob>> =
        const { std::cell::Cell::new(None) };
}

/// Force the inline-variables knob for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_inline_vars_for_test(knob: Option<InlineVarsKnob>) {
    INLINE_VARS_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// The `NEOVM_JIT_INLINE_VARS` setting compiles use (read once).
pub(crate) fn jit_inline_vars() -> InlineVarsKnob {
    #[cfg(test)]
    if let Some(knob) = INLINE_VARS_TEST_OVERRIDE.with(|c| c.get()) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<InlineVarsKnob> = OnceLock::new();
    *KNOB.get_or_init(|| {
        InlineVarsKnob::parse(std::env::var("NEOVM_JIT_INLINE_VARS").ok().as_deref())
    })
}

#[cfg(test)]
std::thread_local! {
    static SPEC_SOURCES_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force `NEOVM_JIT_SPEC_SOURCES` on/off on the current thread (tests only;
/// `None` returns to the process setting).
#[cfg(test)]
pub(crate) fn force_spec_sources_for_test(on: Option<bool>) {
    SPEC_SOURCES_TEST_OVERRIDE.with(|c| c.set(on));
}

/// `NEOVM_JIT_SPEC_SOURCES=on` (P2.1 C5, `compile::source_slots`): a T1 or
/// OSR compile speculates an `Op::Call` whose callee is not a constant and
/// whose recorded target is one closure source, behind the source's
/// identity guard. Default off (the lowering is unchanged); `on` implies
/// `NEOVM_JIT_FEEDBACK=use`. Read at compile time only.
pub(crate) fn jit_spec_sources_on() -> bool {
    #[cfg(test)]
    if let Some(on) = SPEC_SOURCES_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| {
        let on = matches!(
            std::env::var("NEOVM_JIT_SPEC_SOURCES").ok().as_deref(),
            Some("on" | "1" | "true" | "yes")
        );
        if on {
            tracing::info!(target: "neovm_jit", "NEOVM_JIT_SPEC_SOURCES=on");
        }
        on
    })
}

/// The tier spine's trigger knobs (design `p2-0-integration` §3.1-§3.2,
/// P2.1 C6 + W1; `jit::tier2`). Read once per process (tests override them
/// per thread), at compile time and on the cold request path only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tier2Knob {
    /// `NEOVM_JIT_TIER2=on`: a T1 entry leaf profiles (the prologue
    /// countdown, the back-edge poll credit, the mapping-builtin credit) and
    /// its upgrade is decided by `tier2::tier_request`; the heat-driven
    /// re-tier of `cache::try_run_compiled` is off. `off` (the default):
    /// today's code, CLIF-identical.
    pub(crate) on: bool,
    /// `NEOVM_JIT_T2_WINDOW`: T1 entries before the first request (15,000:
    /// the old re-tier crossing, `RETIER_FACTOR - 1` thresholds after the
    /// tier-up).
    pub(crate) window: u32,
    /// `NEOVM_JIT_T2_LOOP_CREDIT`: what one back-edge poll tick (255 taken
    /// back edges) takes from the countdown (64). `0` = entry-only: no poll
    /// credit and no mapping-builtin credit.
    pub(crate) loop_credit: u32,
}

impl Tier2Knob {
    pub(crate) const DEFAULT_WINDOW: u32 = 15_000;
    pub(crate) const DEFAULT_LOOP_CREDIT: u32 = 64;

    /// The knobs from the environment (`get` reads one variable).
    pub(crate) fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let on = matches!(
            get("NEOVM_JIT_TIER2").as_deref().map(str::trim),
            Some("on" | "1" | "true" | "yes")
        );
        let num = |name: &str, default: u32| {
            get(name)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(default)
        };
        Self {
            on,
            // A zero window would request on the first entry of every leaf
            // and again after each re-arm: one entry is the smallest.
            window: if matches!(get("NEOVM_JIT_T2_STRESS").as_deref(), Some("1" | "on")) {
                1
            } else {
                num("NEOVM_JIT_T2_WINDOW", Self::DEFAULT_WINDOW).max(1)
            },
            loop_credit: num("NEOVM_JIT_T2_LOOP_CREDIT", Self::DEFAULT_LOOP_CREDIT),
        }
    }
}

#[cfg(test)]
std::thread_local! {
    static TIER2_TEST_OVERRIDE: std::cell::Cell<Option<Tier2Knob>> =
        const { std::cell::Cell::new(None) };
}

/// Force the tier-spine knobs on the current thread (tests only; `None`
/// returns to the process setting).
#[cfg(test)]
pub(crate) fn force_tier2_for_test(knob: Option<Tier2Knob>) {
    TIER2_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// Monotone process flag for the entry seams, enabled by either T2 or AOT
/// re-tier knob initialization before an upgrade-capable leaf is published.
/// This scalar carries no code pointer or Lisp state; OnceLock configuration
/// and each mutator's cache retain their own publication, so Relaxed suffices.
/// With both knobs off the existing entry seam retains its single gate load.
static TIER_UPGRADE_ENABLED: std::sync::atomic::AtomicBool =
    std::sync::atomic::AtomicBool::new(false);

/// Cheap entry-seam gate; test overrides retain their existing per-thread scope.
#[inline]
pub(crate) fn jit_tier_upgrade_enabled() -> bool {
    #[cfg(test)]
    {
        let t2 = TIER2_TEST_OVERRIDE.with(std::cell::Cell::get);
        let aot = AOT_RETIER_TEST_OVERRIDE.with(std::cell::Cell::get);
        if t2.is_some() || aot.is_some() {
            return t2.map_or_else(|| jit_tier2().on, |knob| knob.on)
                || aot.unwrap_or_else(jit_aot_retier_on);
        }
    }
    TIER_UPGRADE_ENABLED.load(std::sync::atomic::Ordering::Relaxed)
}

/// The tier-spine knobs this thread's compiles and requests use.
pub(crate) fn jit_tier2() -> Tier2Knob {
    #[cfg(test)]
    if let Some(knob) = TIER2_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<Tier2Knob> = OnceLock::new();
    *KNOB.get_or_init(|| {
        let knob = Tier2Knob::from_env(|name| std::env::var(name).ok());
        if knob.on {
            tracing::info!(
                target: "neovm::jit::knobs",
                window = knob.window,
                loop_credit = knob.loop_credit,
                "NEOVM_JIT_TIER2=on is on in this process"
            );
        }
        if knob.on {
            TIER_UPGRADE_ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        knob
    })
}

/// The tier-spine knobs this test thread forced, if any.
#[cfg(test)]
pub(crate) fn tier2_forced_for_test() -> Option<Tier2Knob> {
    TIER2_TEST_OVERRIDE.with(std::cell::Cell::get)
}

/// Cold Tier-2 policy controls, process immutable; test overrides are per mutator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Tier2PolicyKnob {
    pub(crate) stable: u32,
    pub(crate) attempts: u32,
    pub(crate) budget_pct: u32,
    pub(crate) floor_ms: u32,
    pub(crate) max_reopt: u8,
}
impl Tier2PolicyKnob {
    pub(crate) fn from_env(get: impl Fn(&str) -> Option<String>) -> Self {
        let num = |name: &str, default: u32| {
            get(name)
                .and_then(|v| v.trim().parse().ok())
                .unwrap_or(default)
        };
        let stress = matches!(get("NEOVM_JIT_T2_STRESS").as_deref(), Some("1" | "on"));
        Self {
            stable: if stress {
                1
            } else {
                num("NEOVM_JIT_T2_STABLE", 4_000).max(1)
            },
            attempts: num("NEOVM_JIT_T2_ATTEMPTS", 4).max(1),
            budget_pct: if stress {
                0
            } else {
                num("NEOVM_JIT_T2_BUDGET", 2)
            },
            floor_ms: num("NEOVM_JIT_T2_BUDGET_FLOOR_MS", 5),
            max_reopt: num("NEOVM_JIT_T2_MAX_REOPT", 3).min(u8::MAX.into()) as u8,
        }
    }
}
#[cfg(test)]
std::thread_local! {
    static TIER2_POLICY_TEST_OVERRIDE: std::cell::Cell<Option<Tier2PolicyKnob>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn force_tier2_policy_for_test(knob: Option<Tier2PolicyKnob>) {
    TIER2_POLICY_TEST_OVERRIDE.with(|c| c.set(knob));
}
pub(crate) fn jit_tier2_policy() -> Tier2PolicyKnob {
    #[cfg(test)]
    if let Some(knob) = TIER2_POLICY_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return knob;
    }
    static KNOB: std::sync::OnceLock<Tier2PolicyKnob> = std::sync::OnceLock::new();
    *KNOB.get_or_init(|| Tier2PolicyKnob::from_env(|name| std::env::var(name).ok()))
}

/// Which bodies emit direct call sites (`direct_call`), when direct calls
/// are on: `NEOVM_JIT_DIRECT_SITES=self` (default), `all`, or `unbounded`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectSitesMode {
    /// Every body.
    All,
    /// A body whose work per entry is unbounded -- a back edge, a call of
    /// itself -- or a re-tier of one that proved hot (the bodies the full
    /// allocator is for, call-heavy ones included). A direct site costs its
    /// caller's compile about a hundred IR instructions and fourteen blocks
    /// more than a shim call; in a straight-line body entered a few
    /// thousand times it never pays that back (elb-bytecomp: 163 of 170
    /// sites, +12% codegen, ~2,000 calls a site).
    Unbounded,
    /// Only proven named calls of a body's own materialized source.
    /// Without explicit REG_ABI, only an exact frameless self-call body
    /// takes the register ABI; other bodies keep memory entries.
    SelfOnly,
}

#[cfg(test)]
std::thread_local! {
    static DIRECT_SITES_TEST_OVERRIDE: std::cell::Cell<Option<DirectSitesMode>> =
        const { std::cell::Cell::new(None) };
}

/// Force the direct-sites mode for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_direct_sites_for_test(mode: Option<DirectSitesMode>) {
    DIRECT_SITES_TEST_OVERRIDE.with(|c| c.set(mode));
}

/// `NEOVM_JIT_DIRECT_SITES` (see [`DirectSitesMode`]). Read at compile time.
pub(crate) fn jit_direct_sites() -> DirectSitesMode {
    #[cfg(test)]
    if let Some(mode) = DIRECT_SITES_TEST_OVERRIDE.with(|c| c.get()) {
        return mode;
    }
    use std::sync::OnceLock;
    static MODE: OnceLock<DirectSitesMode> = OnceLock::new();
    *MODE.get_or_init(
        || match std::env::var("NEOVM_JIT_DIRECT_SITES").ok().as_deref() {
            Some("all") => DirectSitesMode::All,
            Some("unbounded") => DirectSitesMode::Unbounded,
            _ => DirectSitesMode::SelfOnly,
        },
    )
}

/// The call shapes beyond a named exact-arity call that direct sites take
/// (`NEOVM_JIT_DIRECT_SHAPES`, design `p1-1-direct-native-calls` Stage 2,
/// P1.0 S2.5), with direct calls on. Optional/rest parts also permit their
/// frameless bodies to use the register ABI (`LeafAbi::for_build`); framed
/// bodies retain the memory ABI and their own native cleanup extent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) struct DirectShapesKnob {
    /// `optional` (2a): a named call of an `&optional` callee with fewer
    /// arguments than its slots; the site passes nil for each missing one.
    pub(crate) optional: bool,
    /// `rest` (2d): a named call of a `&rest` callee; the site conses the
    /// rest list.
    pub(crate) rest: bool,
    /// `constant` (2b): a call whose callee is a constant byte-code object
    /// (a `cl-flet` local, a `lambda` literal) the fuser left a call.
    pub(crate) constant: bool,
    /// `framed` (2c): an exact-arity JIT body with dynamic bindings or
    /// handler frames, entered through the contained framed trampoline.
    pub(crate) framed: bool,
}

impl DirectShapesKnob {
    pub(crate) const OFF: Self = Self {
        optional: false,
        rest: false,
        constant: false,
        framed: false,
    };
    pub(crate) const ALL: Self = Self {
        optional: true,
        rest: true,
        constant: true,
        framed: true,
    };

    /// Unset/`off`/`0`/`none`: nothing (the default); `all`/`on`/`1`:
    /// every shape; otherwise a comma list of `optional`, `rest`,
    /// `constant`, `framed`.
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let Some(value) = value.map(str::trim) else {
            return Self::OFF;
        };
        match value.to_ascii_lowercase().as_str() {
            "" | "0" | "off" | "false" | "no" | "none" => return Self::OFF,
            "1" | "on" | "all" | "true" | "yes" => return Self::ALL,
            _ => {}
        }
        let mut knob = Self::OFF;
        for part in value.split(',').map(str::trim) {
            match part {
                "optional" => knob.optional = true,
                "rest" => knob.rest = true,
                "constant" => knob.constant = true,
                "framed" => knob.framed = true,
                "" => {}
                other => tracing::warn!(
                    target: "neovm_jit",
                    part = other,
                    "NEOVM_JIT_DIRECT_SHAPES: unknown part ignored (expected optional, rest, constant, framed)"
                ),
            }
        }
        knob
    }
}

#[cfg(test)]
std::thread_local! {
    static DIRECT_SHAPES_TEST_OVERRIDE: std::cell::Cell<Option<DirectShapesKnob>> =
        const { std::cell::Cell::new(None) };
}

/// Force the direct-shapes knob for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_direct_shapes_for_test(knob: Option<DirectShapesKnob>) {
    DIRECT_SHAPES_TEST_OVERRIDE.with(|c| c.set(knob));
}

/// The `NEOVM_JIT_DIRECT_SHAPES` setting (read once), and nothing while
/// direct calls are off ([`jit_direct_call_on`]). Read at compile time, and
/// when a slot or a body's ABI is decided.
pub(crate) fn jit_direct_shapes() -> DirectShapesKnob {
    if !jit_direct_call_on() {
        return DirectShapesKnob::OFF;
    }
    #[cfg(test)]
    if let Some(knob) = DIRECT_SHAPES_TEST_OVERRIDE.with(|c| c.get()) {
        return knob;
    }
    use std::sync::OnceLock;
    static KNOB: OnceLock<DirectShapesKnob> = OnceLock::new();
    *KNOB.get_or_init(|| {
        DirectShapesKnob::parse(std::env::var("NEOVM_JIT_DIRECT_SHAPES").ok().as_deref())
    })
}

#[cfg(test)]
std::thread_local! {
    static CALL_CENSUS_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

/// Force the call census on/off for compiles on the current thread (tests
/// only); `None` returns to the environment's.
#[cfg(test)]
pub(crate) fn force_call_census_for_test(on: Option<bool>) {
    CALL_CENSUS_TEST_OVERRIDE.with(|c| c.set(on));
}

/// `NEOVM_JIT_CALL_CENSUS=on` (a measurement mode, `call_census`): every JIT
/// `Op::Call`/`Op::Apply` site first reports its callee's shape to a counter
/// shim. Default off; off, the lowering is CLIF-identical. Read at compile
/// time only.
pub(crate) fn jit_call_census_on() -> bool {
    #[cfg(test)]
    if let Some(on) = CALL_CENSUS_TEST_OVERRIDE.with(|c| c.get()) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| knob_on("NEOVM_JIT_CALL_CENSUS"))
}

/// Mid-end selection, independent of the T2 countdown trigger. Threading:
/// immutable process configuration; test overrides contain configuration only.
/// Legacy is the default, so the new backend is off and existing CLIF is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptMode {
    #[default]
    Legacy,
    Off,
    Opt,
}
impl OptMode {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("off") => Self::Off,
            Some("opt") => Self::Opt,
            _ => Self::Legacy,
        }
    }
}
pub(crate) fn jit_opt_mode() -> OptMode {
    #[cfg(test)]
    if let Some(mode) = OPT_TEST_OVERRIDE.with(|v| v.get()) {
        return mode;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.mode;
    }
    static MODE: std::sync::OnceLock<OptMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT").as_deref(),
            || super::opt_profile::selected().mode,
            OptMode::parse,
        )
    })
}

/// Reach admissions for the new backend. Threading: an immutable scalar mask,
/// shared by compilers; it contains neither Lisp values nor mutator state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptAdmit {
    pub args: bool,
    pub env: bool,
    pub vars: bool,
    pub binds: bool,
    pub switch: bool,
    pub handlers: bool,
}
impl OptAdmit {
    pub(crate) const ALL: Self = Self {
        args: true,
        env: true,
        vars: true,
        binds: true,
        switch: true,
        handlers: true,
    };
    pub(crate) fn parse(value: Option<&str>) -> Self {
        let mut bits = Self::default();
        for bit in value.unwrap_or("").split(',').map(str::trim) {
            match bit {
                "all" => return Self::ALL,
                "args" => bits.args = true,
                "env" => bits.env = true,
                "vars" => bits.vars = true,
                "binds" => bits.binds = true,
                "switch" => bits.switch = true,
                "handlers" => bits.handlers = true,
                _ => {}
            }
        }
        bits
    }
}
pub(crate) fn jit_opt_admit() -> OptAdmit {
    #[cfg(test)]
    if let Some(bits) = OPT_ADMIT_TEST_OVERRIDE.with(|v| v.get()) {
        return bits;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.admit;
    }
    static BITS: std::sync::OnceLock<OptAdmit> = std::sync::OnceLock::new();
    *BITS.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_ADMIT").as_deref(),
            || super::opt_profile::selected().admit,
            OptAdmit::parse,
        )
    })
}
#[cfg(test)]
thread_local! {
    /// Configuration only, never Lisp state.
    static OPT_TEST_OVERRIDE: std::cell::Cell<Option<OptMode>> = const { std::cell::Cell::new(None) };
    /// Configuration only, never Lisp state.
    static OPT_ADMIT_TEST_OVERRIDE: std::cell::Cell<Option<OptAdmit>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn force_opt_for_test(mode: Option<OptMode>, admit: Option<OptAdmit>) {
    OPT_TEST_OVERRIDE.with(|v| v.set(mode));
    OPT_ADMIT_TEST_OVERRIDE.with(|v| v.set(admit));
}

/// Select the backend a test asserts, independently of the process configuration.
/// Threading: test-thread compiler configuration only, never Lisp state; nested
/// scopes restore the exact previous override, including its absence.
#[cfg(test)]
pub(crate) fn opt_mode_scope_for_test(mode: OptMode) -> impl Drop {
    struct Scope(Option<OptMode>);
    impl Drop for Scope {
        fn drop(&mut self) {
            OPT_TEST_OVERRIDE.with(|value| value.set(self.0));
        }
    }
    Scope(OPT_TEST_OVERRIDE.with(|value| value.replace(Some(mode))))
}

/// Independently selected mid-end passes. Threading: immutable process-wide
/// compiler configuration only; it contains no Lisp values or mutator state.
/// An empty list runs no transformations. Negative entries support bisection
/// of an explicit list (for example `all,-gvn`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct OptPasses {
    pub fold: bool,
    pub bool_rep: bool,
    pub reps: bool,
    pub gvn: bool,
    pub range: bool,
    pub licm: bool,
    pub sink: bool,
}

impl OptPasses {
    pub(crate) const ALL: Self = Self {
        fold: true,
        bool_rep: true,
        reps: true,
        gvn: true,
        range: true,
        licm: true,
        sink: true,
    };

    pub(crate) fn parse(value: Option<&str>) -> Self {
        let mut passes = Self::default();
        for token in value.unwrap_or("").split(',').map(str::trim) {
            let (name, on) = token
                .strip_prefix('-')
                .map_or((token, true), |name| (name, false));
            match name {
                "none" if on => passes = Self::default(),
                "all" => passes = if on { Self::ALL } else { Self::default() },
                "fold" => passes.fold = on,
                "bool" => passes.bool_rep = on,
                "reps" => passes.reps = on,
                "gvn" => passes.gvn = on,
                "range" => passes.range = on,
                "licm" => passes.licm = on,
                "sink" => passes.sink = on,
                _ => {}
            }
        }
        passes
    }
}

pub(crate) fn jit_opt_passes() -> OptPasses {
    #[cfg(test)]
    if let Some(passes) = OPT_PASSES_TEST_OVERRIDE.with(|v| v.get()) {
        return passes;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.passes;
    }
    static PASSES: std::sync::OnceLock<OptPasses> = std::sync::OnceLock::new();
    *PASSES.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_PASSES").as_deref(),
            || super::opt_profile::selected().passes,
            OptPasses::parse,
        )
    })
}

#[cfg(test)]
thread_local! {
    /// Compiler configuration only, never mutator or Lisp state.
    static OPT_PASSES_TEST_OVERRIDE: std::cell::Cell<Option<OptPasses>> = const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_opt_passes_for_test(passes: Option<OptPasses>) {
    OPT_PASSES_TEST_OVERRIDE.with(|v| v.set(passes));
}

/// Default-OFF compile-time pass work elision. Threading: immutable process
/// configuration only, never Lisp state, mutator pointers or compiler scratch.
pub(crate) fn jit_opt_fast() -> bool {
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.fast;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_FAST").as_deref(),
            || super::opt_profile::selected().fast,
            |value| value == Some("on"),
        )
    })
}

/// Compile-only profitability policy. Threading: immutable process configuration;
/// test overrides hold only the scalar policy, never Lisp or mutator state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptProfitMode {
    #[default]
    Off,
    Loops,
    Lists,
    PrimitiveLists,
    Kernels,
}
impl OptProfitMode {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("loops") => Self::Loops,
            Some("lists") => Self::Lists,
            Some("primitive-lists") => Self::PrimitiveLists,
            Some("kernels") => Self::Kernels,
            _ => Self::Off,
        }
    }
}
pub(crate) fn jit_opt_profit() -> OptProfitMode {
    #[cfg(test)]
    if let Some(mode) = OPT_PROFIT_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return mode;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.profit;
    }
    static MODE: std::sync::OnceLock<OptProfitMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_PROFIT").as_deref(),
            || super::opt_profile::selected().profit,
            OptProfitMode::parse,
        )
    })
}
#[cfg(test)]
thread_local! {
    /// Compiler configuration only, never Lisp state or runtime feedback.
    static OPT_PROFIT_TEST_OVERRIDE: std::cell::Cell<Option<OptProfitMode>> =
        const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn force_opt_profit_for_test(mode: Option<OptProfitMode>) {
    OPT_PROFIT_TEST_OVERRIDE.with(|value| value.set(mode));
}

/// Default-OFF normal-entry prerequisite. Threading: process configuration
/// is an immutable scalar; test overrides belong only to the current compiler
/// invocation, with no Lisp state, cache entries or runtime recording.
pub(crate) fn jit_opt_require_osr() -> bool {
    #[cfg(test)]
    if let Some(value) = OPT_REQUIRE_OSR_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.require_osr;
    }
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_REQUIRE_OSR").as_deref(),
            || super::opt_profile::selected().require_osr,
            |value| value == Some("on"),
        )
    })
}

#[cfg(test)]
thread_local! {
    /// Test-thread scalar compiler configuration only, never Lisp/mutator state.
    static OPT_REQUIRE_OSR_TEST_OVERRIDE: std::cell::Cell<Option<bool>> =
        const { std::cell::Cell::new(None) };
}

#[cfg(test)]
pub(crate) fn force_opt_require_osr_for_test(value: Option<bool>) {
    OPT_REQUIRE_OSR_TEST_OVERRIDE.with(|setting| setting.set(value));
}

/// Test-owned scalar selection. Threading: restores the exact previous override
/// for this compiler thread, retaining no Lisp value or cache ownership.
#[cfg(test)]
pub(crate) fn opt_require_osr_scope_for_test(value: bool) -> impl Drop {
    /// Compiler-local scalar override; no mutator/Lisp state is stored here.
    struct Scope(Option<bool>);
    impl Drop for Scope {
        fn drop(&mut self) {
            OPT_REQUIRE_OSR_TEST_OVERRIDE.with(|setting| setting.set(self.0));
        }
    }
    Scope(OPT_REQUIRE_OSR_TEST_OVERRIDE.with(|setting| setting.replace(Some(value))))
}

/// Compile-time early-loop policy. Threading: immutable configuration, never
/// runtime recording or mutator state. Off keeps the original profit policy.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum OptEarlyMode {
    #[default]
    Off,
    Hot,
    On,
}
impl OptEarlyMode {
    pub(crate) fn parse(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some("hot") => Self::Hot,
            Some("on") => Self::On,
            _ => Self::Off,
        }
    }
}
pub(crate) fn jit_opt_early() -> OptEarlyMode {
    #[cfg(test)]
    if let Some(value) = OPT_EARLY_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.early;
    }
    static MODE: std::sync::OnceLock<OptEarlyMode> = std::sync::OnceLock::new();
    *MODE.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_EARLY").as_deref(),
            || super::opt_profile::selected().early,
            OptEarlyMode::parse,
        )
    })
}
/// Optional original/final selected-front size bound; zero is disabled.
/// Threading: immutable compiler configuration only. The builder retains its
/// independent 1000-op hard ceiling.
pub(crate) fn jit_opt_max_ops() -> usize {
    #[cfg(test)]
    if let Some(value) = OPT_MAX_OPS_TEST_OVERRIDE.with(std::cell::Cell::get) {
        return value;
    }
    #[cfg(test)]
    if let Some(defaults) = super::opt_profile::test_defaults() {
        return defaults.max_ops;
    }
    static MAX: std::sync::OnceLock<usize> = std::sync::OnceLock::new();
    *MAX.get_or_init(|| {
        super::opt_profile::resolve(
            std::env::var("NEOVM_JIT_OPT_MAX_OPS").as_deref(),
            || super::opt_profile::selected().max_ops,
            |value| {
                value
                    .and_then(|value| value.trim().parse().ok())
                    .unwrap_or(0)
            },
        )
    })
}
#[cfg(test)]
thread_local! {
    /// Scalar compiler configuration only; never Lisp/runtime state.
    static OPT_EARLY_TEST_OVERRIDE: std::cell::Cell<Option<OptEarlyMode>> = const { std::cell::Cell::new(None) };
    /// Scalar compiler configuration only; never Lisp/runtime state.
    static OPT_MAX_OPS_TEST_OVERRIDE: std::cell::Cell<Option<usize>> = const { std::cell::Cell::new(None) };
}
#[cfg(test)]
pub(crate) fn force_opt_early_for_test(value: Option<OptEarlyMode>) {
    OPT_EARLY_TEST_OVERRIDE.with(|mode| mode.set(value));
}
#[cfg(test)]
pub(crate) fn force_opt_max_ops_for_test(value: Option<usize>) {
    OPT_MAX_OPS_TEST_OVERRIDE.with(|mode| mode.set(value));
}

/// Independent cold-only opt construction report. Threading: safely published
/// immutable process path; it does not enable report_requested, native entry
/// counters, leaf naming, feedback or source identity assignment. Absent/off
/// disables it; no file is opened during compilation.
pub(crate) fn jit_opt_report_path() -> Option<&'static std::path::Path> {
    static PATH: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();
    PATH.get_or_init(|| {
        std::env::var_os("NEOVM_JIT_OPT_REPORT_FILE")
            .filter(|value| !value.is_empty() && value.as_os_str() != std::ffi::OsStr::new("off"))
            .map(std::path::PathBuf::from)
    })
    .as_deref()
}
