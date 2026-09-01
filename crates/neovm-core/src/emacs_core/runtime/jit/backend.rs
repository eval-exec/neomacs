//! Cranelift codegen toolchain smoke test.
//!
//! Compiled only with the `jit` cargo feature (it links Cranelift). This module
//! is **not** the lowering path — all real bytecode lowering lives in
//! `jit/compile.rs` (baseline + Tier-2 MIR) and has since well before this
//! comment was corrected. What survives here is the original bring-up probe:
//! [`smoke_compile_add`] builds a trivial native function, commits it to
//! executable memory, and calls it, proving the whole codegen toolchain
//! (`JITBuilder` → `JITModule` → `FunctionBuilder` → `finalize_definitions` →
//! `get_finalized_function` → indirect call) is live inside neovm-core's own
//! build. It stays because it isolates a toolchain/linking failure from a
//! lowering bug — when it passes and compilation still fails, the fault is in
//! `compile.rs`, not in Cranelift or the build.
//!
//! (The earlier note here about cross-checking against a `neovm-compiler` crate
//! is obsolete: that crate was deleted from the workspace on 2026-07-03.)

use cranelift_codegen::ir::{AbiParam, Function, InstBuilder, Signature, UserFuncName, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};

/// Failures that can arise while building or finalizing native code.
///
/// Matched exhaustively at call sites (no catch-all), in keeping with the JIT
/// subsystem's compiler-enforced-completeness rule.
#[derive(Debug)]
pub enum BackendError {
    /// Constructing the JIT builder/module failed (e.g. unsupported target).
    ModuleInit(String),
    /// Declaring or defining the function in the module failed.
    Define(String),
    /// Finalizing relocations / committing executable memory failed.
    Finalize(String),
}

impl core::fmt::Display for BackendError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BackendError::ModuleInit(m) => write!(f, "JIT module init failed: {m}"),
            BackendError::Define(m) => write!(f, "JIT function define failed: {m}"),
            BackendError::Finalize(m) => write!(f, "JIT finalize failed: {m}"),
        }
    }
}

impl std::error::Error for BackendError {}

/// Smoke test: JIT-compile `fn(i64) -> i64 { arg + addend }` (with `addend` baked
/// in as an `iconst`), execute it with `arg`, and return the native result.
///
/// Exercises the full toolchain end-to-end inside neovm-core: a signature with a
/// parameter, an entry block, `iconst` + `iadd`, `return`, `finalize_definitions`,
/// and a real indirect call into freshly-committed executable memory. A success
/// (`Ok(arg + addend)`) is the proof that Cranelift codegen is usable here.
pub fn smoke_compile_add(arg: i64, addend: i64) -> Result<i64, BackendError> {
    // 1. Build the JIT module — it owns the executable-memory allocator and the
    //    native ISA selected for the host.
    let builder = JITBuilder::new(default_libcall_names())
        .map_err(|e| BackendError::ModuleInit(e.to_string()))?;
    let mut module = JITModule::new(builder);

    // 2-4. Build, declare, and define the function into the module via the
    //       module-generic build seam (the R1b proof: this same body will later
    //       drive an `ObjectModule` for AOT, with no JIT-specific calls inside).
    let fid = build_smoke_fn(&mut module, addend)?;

    // 5. Finalize: apply relocations and commit the code to executable memory.
    //    (`finalize_definitions` is JITModule-only — it is *the* seam that AOT
    //    replaces with `ObjectModule::finish()`, so it stays in this wrapper.)
    module
        .finalize_definitions()
        .map_err(|e| BackendError::Finalize(e.to_string()))?;

    // 6. Fetch the finalized code pointer and call it.
    let code_ptr = module.get_finalized_function(fid);
    // SAFETY: `code_ptr` points at freshly-finalized native code whose ABI is
    // exactly `extern "C" fn(i64) -> i64` — it matches the Cranelift signature
    // built above (one i64 param, one i64 return, platform call conv). `module`
    // owns the executable memory and is kept alive until after the call returns,
    // so the code is not unmapped while we are running it.
    let result = unsafe {
        let f: extern "C" fn(i64) -> i64 = core::mem::transmute(code_ptr);
        f(arg)
    };

    // `module` drops here, releasing the JIT memory — strictly after the call.
    Ok(result)
}

/// Module-generic build seam for [`smoke_compile_add`]: builds the signature,
/// the `block0(v0: i64): return v0 + addend` body, then declares + defines the
/// function into `module`. Returns the `FuncId`.
///
/// This is the cheapest proof of the R1b `M: Module` seam: the body contains
/// **none** of the three `ObjectModule`-incompatible operations, all of which
/// stay in the JIT wrapper:
///   * `builder.symbol(...)`   — for AOT becomes a `Linkage::Import` + dlopen.
///   * `finalize_definitions`  — for AOT becomes `ObjectModule::finish()`.
///   * `get_finalized_function`— for AOT becomes a `dlsym` lookup.
fn build_smoke_fn<M: Module>(
    module: &mut M,
    addend: i64,
) -> Result<cranelift_module::FuncId, BackendError> {
    // The platform C calling convention (SystemV on Linux x86-64), which matches
    // the `extern "C" fn` type the JIT wrapper transmutes the finalized pointer
    // to. Derived from the module's own target config, so it is correct for both
    // the JIT host ISA and any AOT target ISA.
    let frontend_config = module.target_config();
    let call_conv = frontend_config.default_call_conv;

    // Signature: (i64) -> i64.
    let mut sig = Signature::new(call_conv);
    sig.params.push(AbiParam::new(types::I64));
    sig.returns.push(AbiParam::new(types::I64));

    // Build the function body: `block0(v0: i64): return v0 + addend`.
    let mut func = Function::with_name_signature(UserFuncName::user(0, 0), sig.clone());
    let mut fbctx = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut func, &mut fbctx);
        let block = fb.create_block();
        fb.append_block_params_for_function_params(block);
        fb.switch_to_block(block);
        fb.seal_block(block);

        let arg_val = fb.block_params(block)[0];
        let addend_val = fb.ins().iconst(types::I64, addend);
        let sum = fb.ins().iadd(arg_val, addend_val);
        fb.ins().return_(&[sum]);

        fb.finalize(frontend_config);
    }

    // Declare + define the function in the module.
    let fid = module
        .declare_function("__neovm_jit_smoke_add", Linkage::Local, &sig)
        .map_err(|e| BackendError::Define(e.to_string()))?;
    let mut ctx = module.make_context();
    ctx.func = func;
    module
        .define_function(fid, &mut ctx)
        .map_err(|e| BackendError::Define(e.to_string()))?;
    module.clear_context(&mut ctx);

    Ok(fid)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_add_roundtrips_through_native_code() {
        // Proves the Cranelift toolchain compiles + runs native code in-build.
        assert_eq!(smoke_compile_add(40, 2).unwrap(), 42);
        assert_eq!(smoke_compile_add(0, 0).unwrap(), 0);
        assert_eq!(smoke_compile_add(-5, 5).unwrap(), 0);
        assert_eq!(smoke_compile_add(i64::MAX - 1, 1).unwrap(), i64::MAX);
    }

    #[test]
    fn smoke_add_is_repeatable() {
        // Each call builds + tears down its own JITModule; doing it many times
        // must not leak, crash, or corrupt — a basic exec-memory lifecycle check.
        for i in 0..64 {
            assert_eq!(smoke_compile_add(i, 100).unwrap(), i + 100);
        }
    }
}
