//! The front/backend split of a JIT compile (P2.4 B5, design
//! `p2-4-background-compile` §3.1).
//!
//! The FRONT is the whole of today's compile up to the finished Cranelift
//! function: gates, MIR, the fuser, spec sites, the CFG and its analyses,
//! the leaf's boxes (spec slots, deopt cells, the reloc vector, the entry
//! counter), and CLIF construction. It reads the Lisp heap, the obarray and
//! the eval thread's knobs, and it is unchanged: the builders hand their
//! function to a [`DeferredSink`], which captures it instead of defining it.
//!
//! The BACKEND is Cranelift alone: codegen, register allocation, emission
//! and finalize. Its whole input is a [`JobPayload`]: the function, its
//! entry declaration and its imports named by [`Shim`] -- never by the
//! front's module `FuncId`s -- so any module can compile it
//! ([`SharedJit::define_payload`] re-declares each import against its own
//! shim ids). Nothing in a payload points into the Lisp heap: the addresses
//! the function bakes (spec slots, deopt cells, the reloc vector, the entry
//! counter) are Rust boxes the front's leaf owns, and the only heap bits it
//! holds (a speculated callee's `expected` word) are compared, never
//! dereferenced.
//!
//! The CLIF, and so the machine code, is the same whichever path compiles
//! it (the split tests pin that on the pipeline corpus).

use cranelift_codegen::ir::{Function, UserExternalName, UserExternalNameRef};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::JITModule;
use cranelift_module::{FuncId, Linkage, Module};
use strum::IntoEnumIterator;

use super::super::lowering::{RegallocChoice, active_regalloc_choice};
use super::super::shim_refs::{Shim, ShimGroup, ShimIds};
use super::super::sink::{LeafEntry, define_with_context};
use super::super::{CompileError, LeafBacking};
use super::{JitDefined, JitSink, SharedJit, bump_stats, declare_leaf_entry, entry_is_named};
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::stats::{CompilePhase, enter_phase};

/// A leaf function the front finished, captured by a [`DeferredSink`].
pub(crate) struct Captured {
    func: Function,
    name: Box<str>,
    linkage: Linkage,
    disasm: bool,
}

/// The backend's whole input (see the module docs). Plain data, `Send`:
/// it crosses to a worker thread.
pub(crate) struct JobPayload {
    pub(crate) func: Function,
    /// The declared entry name (the perf-map label under naming).
    pub(crate) name: Box<str>,
    pub(crate) linkage: Linkage,
    /// Declare the entry under `name` (else anonymously); decided by the
    /// front, whose thread owns the naming knob.
    pub(crate) named: bool,
    /// Every external name the function imports, as the shim it calls. Empty
    /// for a payload that must be defined in the module its front declared
    /// into ([`JobPayload::in_place`]).
    pub(crate) imports: Box<[(UserExternalNameRef, Shim)]>,
    /// Whether `imports` names every import (else the function keeps its
    /// front module's ids and only that module may define it).
    pub(crate) portable: bool,
    /// The register allocator the front chose (the module to compile into).
    pub(crate) regalloc: RegallocChoice,
    /// Keep Cranelift's disassembly (`NEOVM_JIT_DUMP_ASM`).
    pub(crate) disasm: bool,
}

const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<JobPayload>();
};

impl JobPayload {
    /// Name every import of `captured` by the shim `shims` (the front
    /// module's ids) declared it as. An import that is not a declared shim
    /// (none exists today: every import goes through `RtRefs`) leaves the
    /// payload tied to the front's module.
    fn package(captured: Captured, shims: &ShimIds, regalloc: RegallocChoice) -> JobPayload {
        let named = entry_is_named(captured.linkage);
        let mut imports = Vec::new();
        let mut portable = true;
        for (reference, name) in captured.func.params.user_named_funcs().iter() {
            match shim_of(shims, name) {
                Some(shim) => imports.push((reference, shim)),
                None => portable = false,
            }
        }
        JobPayload {
            func: captured.func,
            name: captured.name,
            linkage: captured.linkage,
            named,
            imports: if portable {
                imports.into_boxed_slice()
            } else {
                Box::default()
            },
            portable,
            regalloc,
            disasm: captured.disasm,
        }
    }
}

/// The shim `shims` declared under the module-level `name`, if any.
fn shim_of(shims: &ShimIds, name: &UserExternalName) -> Option<Shim> {
    if name.namespace != 0 {
        return None;
    }
    Shim::iter().find(|&shim| shims.get(shim).is_some_and(|id| id.as_u32() == name.index))
}

/// Point every import of `func` at `shims`' declaration of the same shim:
/// the payload was built against another module's ids, and the two modules'
/// declaration orders need not agree.
pub(crate) fn remap_imports(
    func: &mut Function,
    imports: &[(UserExternalNameRef, Shim)],
    shims: &ShimIds,
) -> Result<(), CompileError> {
    for &(reference, shim) in imports {
        let id = shims.get(shim).ok_or_else(|| {
            CompileError::Backend(BackendError::Define(format!(
                "shim {shim:?} is not declared in the backend module"
            )))
        })?;
        func.params
            .reset_user_func_name(reference, UserExternalName::new(0, id.as_u32()));
    }
    Ok(())
}

/// The front's sink: declarations from the thread's persistent module, the
/// finished function captured for the backend.
pub(crate) struct DeferredSink<'a> {
    pub(super) module: &'a mut JITModule,
    pub(super) shims: &'a ShimIds,
    pub(super) fbctx: &'a mut Option<FunctionBuilderContext>,
    captured: Option<Captured>,
}

impl DeferredSink<'_> {
    /// Keep the finished function; nothing is declared or defined yet. The
    /// returned id is a placeholder no caller of a deferred build reads.
    pub(super) fn capture(&mut self, entry: LeafEntry<'_>, func: Function, disasm: bool) -> FuncId {
        debug_assert!(self.captured.is_none(), "one leaf per build");
        debug_assert!(func.signature == *entry.signature);
        self.captured = Some(Captured {
            func,
            name: entry.name.into(),
            linkage: entry.linkage,
            disasm,
        });
        FuncId::from_u32(u32::MAX)
    }
}

/// A compiled payload: its finalized entry and code size.
pub(crate) struct DefinedCode {
    pub(crate) entry: *const u8,
    pub(crate) code_bytes: usize,
}

impl SharedJit {
    /// The backend half: compile `payload` into this backend's module for
    /// its allocator (re-declaring its imports against that module's shims),
    /// finalize it and return its entry. Runs on whichever thread owns this
    /// backend: the eval thread's in line, or a worker's.
    pub(crate) fn define_payload(
        &mut self,
        mut payload: JobPayload,
    ) -> Result<DefinedCode, CompileError> {
        let setup_phase = enter_phase(CompilePhase::Setup);
        // Workers do not inherit frontend test overrides. The payload's
        // imported names are the complete declaration requirement instead.
        let tier2_profile = payload
            .imports
            .iter()
            .any(|(_, shim)| shim.group() == ShimGroup::Tier2Profile);
        self.ensure_module(payload.regalloc, tier2_profile)?;
        drop(setup_phase);
        let SharedJit { modules, ctx, .. } = self;
        let shared = modules[payload.regalloc.index()]
            .as_mut()
            .expect("ensure_module installed it");
        if payload.portable {
            remap_imports(&mut payload.func, &payload.imports, &shared.shims)?;
        }
        let signature = payload.func.signature.clone();
        let fid = declare_leaf_entry(
            &mut shared.module,
            LeafEntry {
                name: &payload.name,
                linkage: payload.linkage,
                signature: &signature,
            },
            payload.named,
            shared.leaves,
        )?;
        ctx.clear();
        ctx.func = payload.func;
        let defined = define_with_context(&mut shared.module, fid, ctx, payload.disasm);
        let code_bytes = ctx
            .compiled_code()
            .map_or(0, |code| code.code_buffer().len());
        shared.module.clear_context(ctx);
        defined?;
        let finalize_phase = enter_phase(CompilePhase::Finalize);
        shared
            .module
            .finalize_definitions()
            .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
        let entry = shared.module.get_finalized_function(fid);
        drop(finalize_phase);
        shared.leaves += 1;
        bump_stats(|s| {
            s.shared_leaves += 1;
            s.split_payloads += 1;
        });
        Ok(DefinedCode { entry, code_bytes })
    }
}

/// A split compile on this thread's persistent backend: run the front
/// (`build`) against the module of the compile's allocator, package the
/// function it hands over, and compile the package in line.
pub(super) fn define_split(
    jit: &mut Option<SharedJit>,
    build: impl FnOnce(&mut JitSink<'_>) -> Result<FuncId, CompileError>,
) -> Result<JitDefined, CompileError> {
    let setup_phase = enter_phase(CompilePhase::Setup);
    let jit = jit.get_or_insert_with(SharedJit::fresh);
    let choice = active_regalloc_choice();
    jit.ensure_module(choice, super::super::jit_tier2().on)?;
    drop(setup_phase);
    let payload = {
        let SharedJit { modules, fbctx, .. } = &mut *jit;
        let shared = modules[choice.index()]
            .as_mut()
            .expect("ensure_module installed it");
        let mut sink = JitSink::Deferred(DeferredSink {
            module: &mut shared.module,
            shims: &shared.shims,
            fbctx,
            captured: None,
        });
        build(&mut sink)?;
        let JitSink::Deferred(sink) = sink else {
            unreachable!("the sink stays deferred")
        };
        let captured = sink
            .captured
            .expect("a finished leaf build hands over its function");
        JobPayload::package(captured, &shared.shims, choice)
    };
    // A compile the cache lets defer leaves its leaf pending: the entry
    // stays null until a probe installs the backend's (`jit::bg`). Only a
    // portable payload may go: another module could not link the rest.
    let mut payload = payload;
    if payload.portable
        && let Some((class, route)) = crate::emacs_core::jit::bg::defer_route()
    {
        use crate::emacs_core::jit::bg::{self, DeferRoute};
        let pending = JitDefined {
            entry: std::ptr::null(),
            backing: LeafBacking::Shared,
        };
        match route {
            DeferRoute::Worker => match bg::enqueue(class, payload) {
                Ok(()) => return Ok(pending),
                // No worker could start: compile in line after all.
                Err(back) => payload = back,
            },
            DeferRoute::InLine => {
                bg::defer_in_line(class, || {
                    jit.define_payload(payload).map(|code| code.entry as usize)
                });
                return Ok(pending);
            }
        }
    }
    let code = jit.define_payload(payload)?;
    Ok(JitDefined {
        entry: code.entry,
        backing: LeafBacking::Shared,
    })
}

#[cfg(test)]
#[path = "split/tests/remap_test.rs"]
mod remap_tests;

// BEGIN T35 SELECTED SHIM BACKEND
/// Owned imports are the complete selected requirement. No frontend knob or
/// mutator state is read by this worker-side dispatcher.
pub(super) fn selected_payload(payload: &JobPayload) -> bool {
    let array_profile = payload
        .imports
        .iter()
        .any(|(_, shim)| shim.group() == ShimGroup::Tier2ArrayProfile);
    let sink_versions = payload
        .imports
        .iter()
        .any(|(_, shim)| shim.group() == ShimGroup::OptSink);

    array_profile || sink_versions
}

impl SharedJit {
    pub(crate) fn define_payload_with_groups(
        &mut self,
        payload: JobPayload,
    ) -> Result<DefinedCode, CompileError> {
        let array_profile = payload
            .imports
            .iter()
            .any(|(_, shim)| shim.group() == ShimGroup::Tier2ArrayProfile);
        let sink_versions = payload
            .imports
            .iter()
            .any(|(_, shim)| shim.group() == ShimGroup::OptSink);

        if !array_profile && !sink_versions {
            return self.define_payload(payload);
        }
        self.define_payload_selected(payload, array_profile, sink_versions)
    }

    fn define_payload_selected(
        &mut self,
        mut payload: JobPayload,
        array_profile: bool,
        sink_versions: bool,
    ) -> Result<DefinedCode, CompileError> {
        let setup_phase = enter_phase(CompilePhase::Setup);
        // Workers do not inherit frontend test overrides. The payload's
        // imported names are the complete declaration requirement instead.
        let tier2_profile = payload
            .imports
            .iter()
            .any(|(_, shim)| shim.group() == ShimGroup::Tier2Profile);
        self.ensure_module_selected(
            payload.regalloc,
            tier2_profile,
            array_profile,
            sink_versions,
        )?;
        drop(setup_phase);
        let SharedJit { modules, ctx, .. } = self;
        let shared = modules[payload.regalloc.index()]
            .as_mut()
            .expect("ensure_module installed it");
        if payload.portable {
            remap_imports(&mut payload.func, &payload.imports, &shared.shims)?;
        }
        let signature = payload.func.signature.clone();
        let fid = declare_leaf_entry(
            &mut shared.module,
            LeafEntry {
                name: &payload.name,
                linkage: payload.linkage,
                signature: &signature,
            },
            payload.named,
            shared.leaves,
        )?;
        ctx.clear();
        ctx.func = payload.func;
        let defined = define_with_context(&mut shared.module, fid, ctx, payload.disasm);
        let code_bytes = ctx
            .compiled_code()
            .map_or(0, |code| code.code_buffer().len());
        shared.module.clear_context(ctx);
        defined?;
        let finalize_phase = enter_phase(CompilePhase::Finalize);
        shared
            .module
            .finalize_definitions()
            .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
        let entry = shared.module.get_finalized_function(fid);
        drop(finalize_phase);
        shared.leaves += 1;
        bump_stats(|s| {
            s.shared_leaves += 1;
            s.split_payloads += 1;
        });
        Ok(DefinedCode { entry, code_bytes })
    }
}

/// Selected twin of the original frontend packaging path.
pub(super) fn define_split_selected(
    jit: &mut Option<SharedJit>,
    array_profile: bool,
    sink_versions: bool,
    build: impl FnOnce(&mut JitSink<'_>) -> Result<FuncId, CompileError>,
) -> Result<JitDefined, CompileError> {
    let setup_phase = enter_phase(CompilePhase::Setup);
    let jit = jit.get_or_insert_with(SharedJit::fresh);
    let choice = active_regalloc_choice();
    jit.ensure_module_selected(
        choice,
        super::super::jit_tier2().on,
        array_profile,
        sink_versions,
    )?;
    drop(setup_phase);
    let payload = {
        let SharedJit { modules, fbctx, .. } = &mut *jit;
        let shared = modules[choice.index()]
            .as_mut()
            .expect("ensure_module installed it");
        let mut sink = JitSink::Deferred(DeferredSink {
            module: &mut shared.module,
            shims: &shared.shims,
            fbctx,
            captured: None,
        });
        build(&mut sink)?;
        let JitSink::Deferred(sink) = sink else {
            unreachable!("the sink stays deferred")
        };
        let captured = sink
            .captured
            .expect("a finished leaf build hands over its function");
        JobPayload::package(captured, &shared.shims, choice)
    };
    // A compile the cache lets defer leaves its leaf pending: the entry
    // stays null until a probe installs the backend's (`jit::bg`). Only a
    // portable payload may go: another module could not link the rest.
    let mut payload = payload;
    if payload.portable
        && let Some((class, route)) = crate::emacs_core::jit::bg::defer_route()
    {
        use crate::emacs_core::jit::bg::{self, DeferRoute};
        let pending = JitDefined {
            entry: std::ptr::null(),
            backing: LeafBacking::Shared,
        };
        match route {
            DeferRoute::Worker => match bg::enqueue(class, payload) {
                Ok(()) => return Ok(pending),
                // No worker could start: compile in line after all.
                Err(back) => payload = back,
            },
            DeferRoute::InLine => {
                bg::defer_in_line(class, || {
                    jit.define_payload_with_groups(payload)
                        .map(|code| code.entry as usize)
                });
                return Ok(pending);
            }
        }
    }
    let code = jit.define_payload_with_groups(payload)?;
    Ok(JitDefined {
        entry: code.entry,
        backing: LeafBacking::Shared,
    })
}
// END T35 SELECTED SHIM BACKEND
