//! Generated-code access to real atomic forwarder fields.
//!
//! CLIF 0.134.3 only exposes SeqCst atomic operations. Its atomic loads are
//! already the required x86 mov / arm64 ldar, but an x86 atomic_store adds
//! mfence. For x86, compiler sequence points bracket a naturally atomic
//! aligned store; x86 TSO supplies Release ordering without a hardware fence.
//! Other architectures retain CLIF's atomic_store, including arm64 stlr,
//! with the same compiler barriers: 0.134.3's alias analysis can also remove
//! a repeated atomic_store before updating its fence state.
//! The sequence points also clear Cranelift's store knowledge, preventing an
//! idempotent publication store from being removed. They emit no machine bytes.

use cranelift_codegen::ir::{InstBuilder, MemFlagsData, Value, types};
use cranelift_codegen::isa::TargetIsa;
use cranelift_frontend::FunctionBuilder;

/// Compiler-only target fact, obtained from the ISA of this function's module.
/// This avoids choosing a store policy from the host when cross-compiling.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ForwardAtomics {
    store: ReleaseStore,
}

#[derive(Clone, Copy, Debug)]
enum ReleaseStore {
    X86Tso,
    Atomic,
}

static_assertions::assert_impl_all!(ForwardAtomics: Copy, Clone, std::fmt::Debug, Send, Sync);
static_assertions::assert_impl_all!(ReleaseStore: Copy, Clone, std::fmt::Debug, Send, Sync);

impl ForwardAtomics {
    pub(crate) fn for_isa(isa: &dyn TargetIsa) -> Self {
        Self {
            // Cranelift's X64Backend::name is the closed backend fact "x64".
            // An unknown/new architecture uses the portable atomic operation.
            store: if isa.name() == "x64" {
                ReleaseStore::X86Tso
            } else {
                ReleaseStore::Atomic
            },
        }
    }

    /// Publish an aligned Value word after the caller's GC preimage gate.
    ///
    /// All forwarder stores either take the owner-mutator shim (which notes
    /// the SATB preimage), or are dominated by the JIT mark-idle guard. The
    /// mark phase cannot begin inside that guard's no-safepoint interval.
    pub(super) fn store_word(
        self,
        fb: &mut FunctionBuilder,
        _permit: &super::inline_vars::MarkIdleStorePermit,
        descriptor: Value,
        offset: usize,
        value: Value,
    ) {
        debug_assert_eq!(fb.func.dfg.value_type(value), types::I64);
        debug_assert_eq!(offset % 8, 0);
        let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
        self.store(fb, address, value);
    }

    /// A Boolean forwarder has no GC-managed preimage. Runtime Boolean
    /// setters remain Relaxed; the generated publication is conservatively
    /// Release with no extra x86 instructions.
    pub(super) fn store_bool(
        self,
        fb: &mut FunctionBuilder,
        _permit: &super::inline_vars::MarkIdleStorePermit,
        descriptor: Value,
        offset: usize,
        value: Value,
    ) {
        debug_assert_eq!(fb.func.dfg.value_type(value), types::I8);
        let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
        self.store(fb, address, value);
    }

    fn store(self, fb: &mut FunctionBuilder, address: Value, value: Value) {
        let flags = MemFlagsData::trusted();
        // `trusted` means aligned/notrap only: no readonly, can_move or
        // alias-region hint may weaken the compiler publication boundary.
        fb.ins().sequence_point();
        match self.store {
            ReleaseStore::X86Tso => {
                fb.ins().store(flags, value, address, 0);
            }
            ReleaseStore::Atomic => {
                fb.ins().atomic_store(flags, value, address);
            }
        }
        fb.ins().sequence_point();
    }
}

/// Acquire a copied Value word. CLIF's stronger SC load has the same
/// instructions as Acquire on x86 and arm64, and cannot be elided or merged.
pub(super) fn load_word(fb: &mut FunctionBuilder, descriptor: Value, offset: usize) -> Value {
    debug_assert_eq!(offset % 8, 0);
    let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
    fb.ins()
        .atomic_load(types::I64, MemFlagsData::trusted(), address)
}

/// Read an AtomicBool byte and widen it as existing guards expect. The
/// generated Acquire/SC read is stronger than the host getter's Relaxed read.
pub(super) fn load_bool(fb: &mut FunctionBuilder, descriptor: Value, offset: usize) -> Value {
    let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
    let byte = fb
        .ins()
        .atomic_load(types::I8, MemFlagsData::trusted(), address);
    fb.ins().uextend(types::I64, byte)
}

#[cfg(test)]
#[path = "tests/atomic_forward_codegen_test.rs"]
mod tests;
