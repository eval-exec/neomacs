//! Where a leaf builder's finished function goes (P2.4 B2).
//!
//! The two leaf builders (`build_leaf_fn`, `lowering::build_mir_leaf_fn`)
//! construct one Cranelift function each and then hand it over. Before this
//! seam they defined it in place into whatever `Module` they were given; now
//! they hand it to a [`LeafSink`], which decides:
//!
//! - which module the leaf's shim imports are declared into
//!   ([`LeafSink::module`]),
//! - how the entry is declared and the function defined
//!   ([`LeafSink::define_leaf`]), and
//! - whether the frontend's scratch state is reused
//!   ([`LeafSink::take_builder_context`]).
//!
//! A plain module (`JITModule`, `ObjectModule`) is its own sink and defines
//! in place, exactly the sequence the builders used to run. The persistent
//! per-thread JIT backend (`compile::shared`) is another sink; a background
//! compile worker would be a third that packages the function instead of
//! compiling it. The builders' CLIF is the same whichever sink receives it.

use cranelift_codegen::ir::{Function, Signature};
use cranelift_frontend::FunctionBuilderContext;
use cranelift_module::{FuncId, Linkage, Module};

use super::CompileError;
use crate::emacs_core::jit::backend::BackendError;
use crate::emacs_core::jit::stats::{self, CompilePhase, enter_phase};

/// How a leaf's entry is declared.
#[derive(Clone, Copy, Debug)]
pub(crate) struct LeafEntry<'a> {
    /// The declared entry name (`lisp:<fn>#<id>:<tier>` under naming, the
    /// legacy static name otherwise, or the AOT content-hash symbol).
    pub(crate) name: &'a str,
    pub(crate) linkage: Linkage,
    pub(crate) signature: &'a Signature,
}

/// The receiver of a built leaf function. See the module docs.
pub(crate) trait LeafSink {
    /// The module the leaf's imports are declared into.
    type Module: Module;

    /// The declaration module: target configuration and shim imports.
    fn module(&mut self) -> &mut Self::Module;

    /// A `FunctionBuilderContext` for the next function. The default is a
    /// fresh one; a long-lived sink hands out its cleared scratch state.
    fn take_builder_context(&mut self) -> FunctionBuilderContext {
        FunctionBuilderContext::new()
    }

    /// Take back a context a finished (`FunctionBuilder::finalize`d, hence
    /// cleared) build used. A build that bailed early never returns its
    /// context, so a half-built one is never reused.
    fn return_builder_context(&mut self, _context: FunctionBuilderContext) {}

    /// Declare the entry and define `func` under it; `disasm` asks Cranelift
    /// to keep the disassembly for `NEOVM_JIT_DUMP_ASM`.
    fn define_leaf(
        &mut self,
        entry: LeafEntry<'_>,
        func: Function,
        disasm: bool,
    ) -> Result<FuncId, CompileError>;
}

/// Declare `entry` in `module` and define `func` under it in a fresh context:
/// the in-place sequence every module-backed sink runs.
pub(crate) fn define_in_place<M: Module>(
    module: &mut M,
    entry: LeafEntry<'_>,
    func: Function,
    disasm: bool,
) -> Result<FuncId, CompileError> {
    let fid = module
        .declare_function(entry.name, entry.linkage, entry.signature)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    define_with_context(module, fid, &mut ctx, disasm)?;
    module.clear_context(&mut ctx);
    Ok(fid)
}

/// Run Cranelift on the function in `ctx` and define it as `fid` (the
/// `codegen` phase), stashing the disassembly when asked. Leaves `ctx`
/// holding the compiled function; the caller clears it.
pub(crate) fn define_with_context<M: Module>(
    module: &mut M,
    fid: FuncId,
    ctx: &mut cranelift_codegen::Context,
    disasm: bool,
) -> Result<(), CompileError> {
    // NEOVM_JIT_DUMP_ASM: Cranelift renders its disassembly only when asked.
    if disasm {
        ctx.set_disasm(true);
    }
    let codegen_phase = enter_phase(CompilePhase::Codegen);
    module
        .define_function(fid, ctx)
        .map_err(|e| CompileError::Backend(BackendError::Define(e.to_string())))?;
    drop(codegen_phase);
    if disasm {
        stats::asm_dump::stash(ctx);
    }
    Ok(())
}

impl LeafSink for cranelift_jit::JITModule {
    type Module = Self;

    fn module(&mut self) -> &mut Self {
        self
    }

    fn define_leaf(
        &mut self,
        entry: LeafEntry<'_>,
        func: Function,
        disasm: bool,
    ) -> Result<FuncId, CompileError> {
        define_in_place(self, entry, func, disasm)
    }
}

impl LeafSink for cranelift_object::ObjectModule {
    type Module = Self;

    fn module(&mut self) -> &mut Self {
        self
    }

    fn define_leaf(
        &mut self,
        entry: LeafEntry<'_>,
        func: Function,
        disasm: bool,
    ) -> Result<FuncId, CompileError> {
        define_in_place(self, entry, func, disasm)
    }
}
