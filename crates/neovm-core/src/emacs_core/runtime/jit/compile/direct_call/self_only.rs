//! Compiler facts for `NEOVM_JIT_DIRECT_SITES=self`. The body source is an
//! immutable identity token, never baked into generated code.
//! Threading: scopes belong to the current compiler thread and restore on
//! nested compilation. They hold no Lisp values or mutable mutator state;
//! runtime slot arming never reads them. The borrowed function/source owns
//! the identity throughout planning and lowering.

use super::super::jit_layout::runtime_identity_word;
use super::*;

std::thread_local! {
    /// Scalar source token for the compile in progress, absent for direct
    /// standalone lowerings, AOT and OSR builds without a source request.
    static SELF_SOURCE: core::cell::Cell<Option<usize>> = const { core::cell::Cell::new(None) };
}

/// Whether the self policy is effective. Explicit register ABI still
/// selects register bodies globally; emission remains restricted to self.
pub(crate) fn self_only_on() -> bool {
    jit_direct_call_on() && jit_direct_sites() == DirectSitesMode::SelfOnly
}

/// Restores the preceding compiler source token when dropped.
/// Threading: compiler-thread scalar configuration only, as above.
pub(crate) struct SelfSourceScope(Option<usize>);

impl SelfSourceScope {
    pub(crate) fn enter_for(f: &ByteCodeFunction, may_call_self: bool) -> Self {
        let source = (self_only_on()
            && may_call_self
            && match jit_direct_self_heat() {
                DirectSelfHeat::Off => true,
                mode => mode.allows(
                    f.jit_runtime().heat(),
                    crate::emacs_core::jit::hot_threshold(),
                ),
            })
        .then(|| runtime_identity_word(f.jit_runtime()));
        Self(SELF_SOURCE.with(|current| current.replace(source)))
    }
}

impl Drop for SelfSourceScope {
    fn drop(&mut self) {
        SELF_SOURCE.with(|current| current.set(self.0));
    }
}

/// The expected materialized callee is of the compiler's source. Lazy
/// pdump stubs decline rather than materializing during this proof.
pub(crate) fn expected_is_source(expected: u64, source: usize) -> bool {
    Value::from_bits(expected as usize)
        .bytecode_data_if_materialized()
        .is_some_and(|bc| runtime_identity_word(bc.jit_runtime()) == source)
}

/// Whether an already selected site actually calls this exact source with
/// its original required arguments. A self symbol used only as data, or a
/// call removed by MIR inlining, cannot make the body register-convert.
pub(crate) fn has_exact_self_site(
    ops: &[Op],
    sites: &HashMap<usize, SpecSite>,
    arity: usize,
) -> bool {
    if !self_only_on() || arity > MAX_REG_ARGS {
        return false;
    }
    let Some(source) = SELF_SOURCE.with(core::cell::Cell::get) else {
        return false;
    };
    sites.iter().any(|(&pc, site)| {
        matches!(ops.get(pc), Some(Op::Call(n)) if *n as usize == arity)
            && exact_site_of_source(site, arity, source)
    })
}

/// MIR's site map contains only surviving original calls. Inspect those
/// actual calls rather than a self-name constant the inliner left as data.
pub(crate) fn has_exact_mir_self_site(
    m: &mir::MirFunction,
    sites: &HashMap<usize, SpecSite>,
) -> bool {
    if !self_only_on() || m.arity > MAX_REG_ARGS || sites.is_empty() {
        return false;
    }
    let Some(source) = SELF_SOURCE.with(core::cell::Cell::get) else {
        return false;
    };
    m.blocks.iter().flat_map(|b| &b.insts).any(|i| {
        matches!(&i.op, mir::MirOp::Opaque { op: Op::Call(n), .. } if *n as usize == m.arity)
            && sites
                .get(&i.pc)
                .is_some_and(|site| exact_site_of_source(site, m.arity, source))
    })
}

fn exact_site_of_source(site: &SpecSite, arity: usize, source: usize) -> bool {
    site.kind == SpecCalleeKind::Bytecode
        && Value::from_bits(site.expected_bits as usize)
            .bytecode_data_if_materialized()
            .is_some_and(|bc| {
                bc.params.required.len() == arity
                    && bc.params.optional.is_empty()
                    && bc.params.rest.is_none()
                    && bc.jit_runtime().patched_prefix() == 0
                    && runtime_identity_word(bc.jit_runtime()) == source
            })
}

/// A register body's parent-source token for `RtCtx`. A memory body emits
/// no self-direct site, even when a shape knob would otherwise reach it.
pub(crate) fn source_for_abi(abi: LeafAbi) -> Option<usize> {
    (self_only_on() && matches!(abi, LeafAbi::Register { .. }))
        .then(|| SELF_SOURCE.with(core::cell::Cell::get))
        .flatten()
}
