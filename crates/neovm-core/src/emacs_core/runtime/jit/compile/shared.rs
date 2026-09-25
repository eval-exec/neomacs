//! The persistent per-thread JIT backend (P2.4 B4 = P0.6 L4-C3 = P1.1
//! Stage 5).
//!
//! Every JIT compile used to build a `JITBuilder` (51 shim symbols), a
//! `JITModule`, its shim declarations, a `codegen::Context` and a
//! `FunctionBuilderContext`, define one function, finalize it into freshly
//! mapped memory, and keep the whole module alive inside the leaf. Now one
//! module per register allocator lives for the thread ([`SharedJit`]): its
//! shims are registered and declared once, the Cranelift contexts are
//! reused, and its code goes into the thread's [`CodeArena`]. A leaf keeps
//! nothing but its entry pointer ([`LeafBacking::Shared`]).
//!
//! Lifetime, unchanged from the per-leaf modules: generated code is never
//! freed while the process lives. A dropped per-leaf `JITModule` leaked its
//! code pages; the arena never unmaps or rewrites a sealed page. So retired
//! and replaced leaves, OSR leaves and a cache `clear` behave as before:
//! the leaf's boxes (spec slots, deopt cells, reloc vector — the GC roots of
//! its constants) are owned by the `CompiledLeaf` exactly as before, and
//! only the code bytes outlive it.
//!
//! Bookkeeping stays bounded: a module keeps a declaration and a compiled
//! blob per leaf, so after [`module_leaf_limit`] leaves it is dropped (its
//! code stays in the arena) and the next compile starts a fresh one.
//!
//! `NEOVM_JIT_PERSISTENT_MODULE=off` compiles every leaf into its own
//! module on Cranelift's default memory, as before (the single-build A/B
//! arm). A compile that finds the backend borrowed (compiles do not nest,
//! so this is a defensive path) does the same and is counted.

use std::cell::{Cell, RefCell};

use cranelift_codegen::ir::Function;
use cranelift_frontend::FunctionBuilderContext;
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module, default_libcall_names};

use super::lowering::{RegallocChoice, active_regalloc_choice, jit_isa};
use super::shim_refs::{ShimGroups, ShimIds};
use super::sink::{LeafEntry, LeafSink, define_in_place, define_with_context};
use super::{CompileError, LeafBacking};
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::stats::{self, CompilePhase, enter_phase};

#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
use super::code_arena::CodeArena;

/// Leaves defined into one module before it is replaced (bounds the
/// module's per-leaf declarations, blobs and relocation records).
const MODULE_LEAF_LIMIT: u32 = 1024;

/// A finalized JIT leaf: its entry and what keeps the code mapped.
pub(crate) struct JitDefined {
    pub(crate) entry: *const u8,
    pub(crate) backing: LeafBacking,
}

/// Counters of this thread's persistent backend (the exit report).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct CodeMemoryStats {
    /// Leaves defined into a persistent module.
    pub(crate) shared_leaves: u64,
    /// Leaves compiled into their own module (knob off, or a re-entrant
    /// compile).
    pub(crate) per_leaf_modules: u64,
    /// Of those, compiles that found the persistent backend borrowed.
    pub(crate) reentrant_fallbacks: u64,
    /// Persistent modules created.
    pub(crate) modules_created: u64,
    /// Persistent modules replaced after [`module_leaf_limit`] leaves.
    pub(crate) modules_retired: u64,
    /// The code arena's counters (all zero off x86-64 Linux).
    pub(crate) arena_regions: u64,
    pub(crate) arena_page_bytes: u64,
    pub(crate) arena_code_bytes: u64,
    pub(crate) arena_seals: u64,
}

impl CodeMemoryStats {
    /// `key=value` rendering for the `[neovm-jit-final-code-memory]` line.
    pub(crate) fn render(&self) -> String {
        format!(
            "shared_leaves={} per_leaf_modules={} reentrant_fallbacks={} modules_created={} \
             modules_retired={} arena_regions={} arena_page_bytes={} arena_code_bytes={} \
             arena_seals={}",
            self.shared_leaves,
            self.per_leaf_modules,
            self.reentrant_fallbacks,
            self.modules_created,
            self.modules_retired,
            self.arena_regions,
            self.arena_page_bytes,
            self.arena_code_bytes,
            self.arena_seals,
        )
    }
}

/// One allocator's live module.
struct SharedModule {
    module: JITModule,
    shims: ShimIds,
    leaves: u32,
}

/// The persistent backend of one thread.
struct SharedJit {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    arena: CodeArena,
    modules: [Option<SharedModule>; RegallocChoice::COUNT],
    /// Reused Cranelift context: cleared before each leaf.
    ctx: cranelift_codegen::Context,
    /// Reused frontend scratch state; `None` while a build holds it (or
    /// after a build bailed with it half-used).
    fbctx: Option<FunctionBuilderContext>,
}

thread_local! {
    static SHARED_JIT: RefCell<Option<SharedJit>> = const { RefCell::new(None) };
    static STATS: Cell<CodeMemoryStats> = const {
        Cell::new(CodeMemoryStats {
            shared_leaves: 0,
            per_leaf_modules: 0,
            reentrant_fallbacks: 0,
            modules_created: 0,
            modules_retired: 0,
            arena_regions: 0,
            arena_page_bytes: 0,
            arena_code_bytes: 0,
            arena_seals: 0,
        })
    };
}

fn bump_stats(f: impl FnOnce(&mut CodeMemoryStats)) {
    // `try_with`: a compile during thread-local teardown just goes uncounted.
    let _ = STATS.try_with(|s| {
        let mut stats = s.get();
        f(&mut stats);
        s.set(stats);
    });
}

/// This thread's backend counters, the arena's included.
pub(crate) fn code_memory_stats() -> CodeMemoryStats {
    let mut stats = STATS.with(Cell::get);
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    SHARED_JIT.with(|cell| {
        if let Ok(guard) = cell.try_borrow()
            && let Some(jit) = guard.as_ref()
        {
            let arena = jit.arena.stats();
            stats.arena_regions = arena.regions;
            stats.arena_page_bytes = arena.page_bytes;
            stats.arena_code_bytes = arena.code_bytes;
            stats.arena_seals = arena.seals;
        }
    });
    stats
}

/// `NEOVM_JIT_PERSISTENT_MODULE=off`: a module per leaf, as before.
pub(crate) fn persistent_module_enabled() -> bool {
    #[cfg(test)]
    if let Some(on) = PERSISTENT_TEST_OVERRIDE.with(Cell::get) {
        return on;
    }
    use std::sync::OnceLock;
    static ON: OnceLock<bool> = OnceLock::new();
    *ON.get_or_init(|| std::env::var("NEOVM_JIT_PERSISTENT_MODULE").as_deref() != Ok("off"))
}

/// Leaves per persistent module before it is replaced.
pub(crate) fn module_leaf_limit() -> u32 {
    #[cfg(test)]
    if let Some(limit) = LEAF_LIMIT_TEST_OVERRIDE.with(Cell::get) {
        return limit;
    }
    MODULE_LEAF_LIMIT
}

#[cfg(test)]
thread_local! {
    static PERSISTENT_TEST_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
    static LEAF_LIMIT_TEST_OVERRIDE: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Force the persistent module on or off for this thread (tests only).
#[cfg(test)]
pub(crate) fn force_persistent_module_for_test(on: bool) {
    PERSISTENT_TEST_OVERRIDE.with(|c| c.set(Some(on)));
}

/// Replace a persistent module after `limit` leaves (tests only).
#[cfg(test)]
pub(crate) fn force_module_leaf_limit_for_test(limit: u32) {
    LEAF_LIMIT_TEST_OVERRIDE.with(|c| c.set(Some(limit)));
}

/// Drop this thread's backend and start the next compile on a fresh one
/// whose arena reserves `region_bytes` per region (tests only: exercises
/// region rollover). Code already handed out stays mapped.
#[cfg(test)]
pub(crate) fn reset_backend_for_test(region_bytes: Option<usize>) {
    SHARED_JIT.with(|cell| {
        let mut guard = cell.borrow_mut();
        *guard = None;
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        if let Some(bytes) = region_bytes {
            *guard = Some(SharedJit::new(CodeArena::with_region_bytes(bytes)));
        }
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        let _ = region_bytes;
    });
}

/// The live persistent module count and the leaves defined into them
/// (tests only).
#[cfg(test)]
pub(crate) fn live_modules_for_test() -> Vec<u32> {
    SHARED_JIT.with(|cell| {
        cell.borrow()
            .as_ref()
            .map(|jit| jit.modules.iter().flatten().map(|m| m.leaves).collect())
            .unwrap_or_default()
    })
}

/// Build one leaf with `build`, finalize it and return its entry: into the
/// thread's persistent module, or into a module of its own when the knob is
/// off or the backend is already borrowed. `per_leaf_shims`: whether a
/// per-leaf module registers the shim symbols (the MIR tier skips them for
/// a runtime-free body, as it always did).
pub(crate) fn define_jit_leaf(
    per_leaf_shims: bool,
    build: impl FnOnce(&mut JitSink<'_>) -> Result<FuncId, CompileError>,
) -> Result<JitDefined, CompileError> {
    let mut build = Some(build);
    if persistent_module_enabled() {
        // `try_with`: a compile during thread-local teardown (the backend
        // already destroyed) takes the per-leaf path like a re-entrant one.
        let shared = SHARED_JIT
            .try_with(|cell| {
                let mut guard = cell.try_borrow_mut().ok()?;
                let build = build.take().expect("build runs once");
                Some(define_shared(&mut guard, build))
            })
            .ok()
            .flatten();
        match shared {
            Some(result) => return result,
            None => bump_stats(|s| s.reentrant_fallbacks += 1),
        }
    }
    define_per_leaf(per_leaf_shims, build.take().expect("build runs once"))
}

/// The pre-B4 path: a fresh module for this leaf, kept alive by the leaf.
fn define_per_leaf(
    register_shims: bool,
    build: impl FnOnce(&mut JitSink<'_>) -> Result<FuncId, CompileError>,
) -> Result<JitDefined, CompileError> {
    let setup_phase = enter_phase(CompilePhase::Setup);
    let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
    if register_shims {
        // Every shim, from the one table (see `shims::JIT_SHIM_TABLE`).
        super::shims::register_shims(&mut builder);
    }
    let mut module = JITModule::new(builder);
    drop(setup_phase);
    let fid = build(&mut JitSink::PerLeaf(&mut module))?;
    let finalize_phase = enter_phase(CompilePhase::Finalize);
    module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
    let entry = module.get_finalized_function(fid);
    drop(finalize_phase);
    bump_stats(|s| s.per_leaf_modules += 1);
    Ok(JitDefined {
        entry,
        backing: LeafBacking::Jit(module),
    })
}

impl SharedJit {
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    fn new(arena: CodeArena) -> SharedJit {
        SharedJit {
            arena,
            modules: [None, None],
            ctx: cranelift_codegen::Context::new(),
            fbctx: Some(FunctionBuilderContext::new()),
        }
    }

    #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
    fn new() -> SharedJit {
        SharedJit {
            modules: [None, None],
            ctx: cranelift_codegen::Context::new(),
            fbctx: Some(FunctionBuilderContext::new()),
        }
    }

    fn fresh() -> SharedJit {
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        return SharedJit::new(CodeArena::new());
        #[cfg(not(all(target_os = "linux", target_arch = "x86_64")))]
        return SharedJit::new();
    }

    /// Make sure `choice`'s module exists and has room, creating it (or
    /// replacing a full one) under the ISA of the compile in progress.
    fn ensure_module(&mut self, choice: RegallocChoice) -> Result<(), CompileError> {
        let slot = &mut self.modules[choice.index()];
        if slot
            .as_ref()
            .is_some_and(|m| m.leaves >= module_leaf_limit())
        {
            // Frees the module's per-leaf bookkeeping; its code pages stay
            // in the arena (the handle never unmaps them).
            *slot = None;
            bump_stats(|s| s.modules_retired += 1);
        }
        if slot.is_some() {
            return Ok(());
        }
        let mut builder = JITBuilder::with_isa(jit_isa()?, default_libcall_names());
        super::shims::register_shims(&mut builder);
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        builder.memory_provider(Box::new(self.arena.handle()));
        let mut module = JITModule::new(builder);
        let config = module.target_config();
        let shims = ShimIds::declare(
            &mut module,
            config.default_call_conv,
            config.pointer_type(),
            ShimGroups {
                subr_spec: true,
                cbsym_spec: true,
            },
        )?;
        *slot = Some(SharedModule {
            module,
            shims,
            leaves: 0,
        });
        bump_stats(|s| s.modules_created += 1);
        Ok(())
    }
}

fn define_shared(
    jit: &mut Option<SharedJit>,
    build: impl FnOnce(&mut JitSink<'_>) -> Result<FuncId, CompileError>,
) -> Result<JitDefined, CompileError> {
    let setup_phase = enter_phase(CompilePhase::Setup);
    let jit = jit.get_or_insert_with(SharedJit::fresh);
    let choice = active_regalloc_choice();
    jit.ensure_module(choice)?;
    drop(setup_phase);
    let SharedJit {
        modules,
        ctx,
        fbctx,
        ..
    } = jit;
    let shared = modules[choice.index()]
        .as_mut()
        .expect("ensure_module installed it");
    let fid = build(&mut JitSink::Shared(SharedSink {
        module: &mut shared.module,
        shims: shared.shims,
        ctx,
        fbctx,
        seq: shared.leaves,
    }))?;
    let finalize_phase = enter_phase(CompilePhase::Finalize);
    shared
        .module
        .finalize_definitions()
        .map_err(|e| CompileError::Backend(BackendError::Finalize(e.to_string())))?;
    let entry = shared.module.get_finalized_function(fid);
    drop(finalize_phase);
    shared.leaves += 1;
    bump_stats(|s| s.shared_leaves += 1);
    Ok(JitDefined {
        entry,
        backing: LeafBacking::Shared,
    })
}

/// The persistent module's side of [`JitSink`].
pub(crate) struct SharedSink<'a> {
    module: &'a mut JITModule,
    shims: ShimIds,
    ctx: &'a mut cranelift_codegen::Context,
    fbctx: &'a mut Option<FunctionBuilderContext>,
    /// The module's leaf count: a uniquifier for named entries.
    seq: u32,
}

/// Where a JIT wrapper's builder defines its leaf.
pub(crate) enum JitSink<'a> {
    /// A module of the leaf's own (the pre-B4 path).
    PerLeaf(&'a mut JITModule),
    /// The thread's persistent module.
    Shared(SharedSink<'a>),
}

impl LeafSink for JitSink<'_> {
    type Module = JITModule;

    fn module(&mut self) -> &mut JITModule {
        match self {
            JitSink::PerLeaf(module) => module,
            JitSink::Shared(sink) => sink.module,
        }
    }

    fn shim_ids(
        &mut self,
        call_conv: cranelift_codegen::isa::CallConv,
        ptr_ty: cranelift_codegen::ir::Type,
        groups: ShimGroups,
    ) -> Result<ShimIds, CompileError> {
        match self {
            JitSink::PerLeaf(module) => ShimIds::declare(*module, call_conv, ptr_ty, groups),
            // Declared once, every group, when the module was created.
            JitSink::Shared(sink) => Ok(sink.shims),
        }
    }

    fn take_builder_context(&mut self) -> FunctionBuilderContext {
        match self {
            JitSink::PerLeaf(_) => FunctionBuilderContext::new(),
            JitSink::Shared(sink) => sink.fbctx.take().unwrap_or_default(),
        }
    }

    fn return_builder_context(&mut self, context: FunctionBuilderContext) {
        if let JitSink::Shared(sink) = self {
            *sink.fbctx = Some(context);
        }
    }

    fn define_leaf(
        &mut self,
        entry: LeafEntry<'_>,
        func: Function,
        disasm: bool,
    ) -> Result<FuncId, CompileError> {
        match self {
            JitSink::PerLeaf(module) => define_in_place(*module, entry, func, disasm),
            JitSink::Shared(sink) => sink.define(entry, func, disasm),
        }
    }
}

impl SharedSink<'_> {
    fn define(
        &mut self,
        entry: LeafEntry<'_>,
        func: Function,
        disasm: bool,
    ) -> Result<FuncId, CompileError> {
        let fid = self.declare_entry(entry)?;
        self.ctx.clear();
        self.ctx.func = func;
        let defined = define_with_context(self.module, fid, self.ctx, disasm);
        self.module.clear_context(self.ctx);
        defined?;
        Ok(fid)
    }

    /// Declare the entry under a name no other leaf of this module has: a
    /// persistent module sees the same label again for a re-tier or a
    /// recompile. Without per-function names (the default) the entry is
    /// anonymous: no name string, no symbol-table insert. With them (perf
    /// map, dumps, reports) the label is kept, suffixed `.N` on reuse.
    fn declare_entry(&mut self, entry: LeafEntry<'_>) -> Result<FuncId, CompileError> {
        let declared = match entry.linkage {
            Linkage::Local if !stats::naming_enabled() => {
                self.module.declare_anonymous_function(entry.signature)
            }
            linkage => {
                let taken = self.module.declarations().get_name(entry.name).is_some();
                let name = if taken {
                    format!("{}.{}", entry.name, self.seq)
                } else {
                    entry.name.to_string()
                };
                self.module
                    .declare_function(&name, linkage, entry.signature)
            }
        };
        declared.map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))
    }
}
