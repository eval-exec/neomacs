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

    /// `off`/`0`/unset: nothing; `on`/`1`/`all`: everything; otherwise a
    /// comma list of `opcode`, `bcall`, `string`.
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
