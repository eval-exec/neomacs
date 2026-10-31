//! Private native mapping owner and scalar emitter. Never a serial cache.
use super::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, condcodes::IntCC, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use std::mem::{ManuallyDrop, offset_of};

// A finalized code address is viewed at its one verified C signature, as in
// the existing fixed-builtin ABI leaf. Neither union nor raw pointer escapes
// the constructor. This is executable-code ABI conversion, never enum decoding.
union FinalizedEntry {
    code: *const u8,
    call: ParallelEntry,
}
const _: () = assert!(size_of::<*const u8>() == size_of::<ParallelEntry>());

/// Own executable mappings even if definition/finalization fails partway.
/// JITModule's ordinary Drop does not release those mappings in Cranelift.
struct ExecutableMemory {
    module: ManuallyDrop<JITModule>,
}
impl Drop for ExecutableMemory {
    fn drop(&mut self) {
        // SAFETY: this owner is unique, cannot expose an entry address, and
        // is dropped exactly once. A successful entry borrows its containing
        // code owner until the native stack returns. Failed compilation has
        // never published a callable entry. Thus no live or future call can
        // refer to these mappings when free_memory invalidates them.
        unsafe { ManuallyDrop::take(&mut self.module).free_memory() }
    }
}

#[must_use = "keep the unique executable owner alive until every entry returns"]
pub(super) struct NativeCompiledCode {
    entry: ParallelEntry,
    _memory: ExecutableMemory,
}

impl NativeCompiledCode {
    /// Called only by the parent while it borrows the real pre-era witness.
    /// This private child has no World or shared compiler/cache side effect.
    pub(super) fn compile(program: ScalarProgram) -> Result<Self, ParallelCompileError> {
        let builder = JITBuilder::new(default_libcall_names())
            .map_err(|error| ParallelCompileError::Module(error.to_string()))?;
        let mut memory = ExecutableMemory {
            module: ManuallyDrop::new(JITModule::new(builder)),
        };
        let module = &mut *memory.module;
        let config = module.target_config();
        let pointer = config.pointer_type();
        if pointer != types::I64 {
            return Err(ParallelCompileError::UnsupportedPointerWidth);
        }
        let version_offset = i32::try_from(offset_of!(ParallelRuntime, version))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let input_offset = i32::try_from(offset_of!(ScalarInput, word))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let output_offset = i32::try_from(offset_of!(ScalarOutput, word))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let mut signature = module.make_signature();
        for _ in 0..4 {
            signature.params.push(AbiParam::new(pointer));
        }
        signature.returns.push(AbiParam::new(types::I32));
        let id = module
            .declare_function("neovm_parallel_scalar_v1", Linkage::Local, &signature)
            .map_err(|error| ParallelCompileError::Define(error.to_string()))?;
        let mut context = module.make_context();
        context.func.signature = signature;
        let mut frontend = FunctionBuilderContext::new();
        {
            let mut b = FunctionBuilder::new(&mut context.func, &mut frontend);
            let entry = b.create_block();
            let supported = b.create_block();
            let refused = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let args = b.block_params(entry).to_vec();
            let version =
                b.ins()
                    .load(types::I32, MemFlagsData::trusted(), args[0], version_offset);
            let matches =
                b.ins()
                    .icmp_imm(IntCC::Equal, version, i64::from(NativeAbiVersion::V1.0));
            b.ins().brif(matches, supported, &[], refused, &[]);
            b.switch_to_block(supported);
            let word = match program {
                // The cast preserves this already checked immediate's machine
                // bit pattern in an I64 iconst; it performs no pointer decode.
                ScalarProgram::Constant(value) => b.ins().iconst(pointer, value.word() as i64),
                ScalarProgram::Argument => {
                    b.ins()
                        .load(pointer, MemFlagsData::trusted(), args[2], input_offset)
                }
            };
            b.ins()
                .store(MemFlagsData::trusted(), word, args[3], output_offset);
            let returned = b.ins().iconst(types::I32, NativeStatus::Returned as i64);
            b.ins().return_(&[returned]);
            b.switch_to_block(refused);
            let unsupported = b
                .ins()
                .iconst(types::I32, NativeStatus::UnsupportedAbi as i64);
            b.ins().return_(&[unsupported]);
            b.seal_all_blocks();
            b.finalize(config);
        }
        module
            .define_function(id, &mut context)
            .map_err(|error| ParallelCompileError::Define(error.to_string()))?;
        module.clear_context(&mut context);
        module
            .finalize_definitions()
            .map_err(|error| ParallelCompileError::Finalize(error.to_string()))?;
        let pointer = module.get_finalized_function(id);
        // SAFETY: this sole constructor just finalized exactly the four-pointer
        // platform C signature above, returning I32. ExecutableMemory retains
        // its mappings. The typed pointer never escapes this owning wrapper.
        let entry = unsafe { FinalizedEntry { code: pointer }.call };
        Ok(Self {
            entry,
            _memory: memory,
        })
    }

    /// Validate production era/owner/result identity before native execution.
    /// The borrowed activation keeps IN_SLICE throughout the bounded call.
    pub(super) fn run<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        input: ImmediateResult,
    ) -> Result<(), ParallelEntryError> {
        admission
            .with_activation(run, destination, |activation| {
                let runtime = ParallelRuntime {
                    version: NativeAbiVersion::V1,
                };
                let input = ScalarInput { word: input.word() };
                let mut output = ScalarOutput {
                    word: ImmediateResult::Nil.word(),
                };
                // SAFETY: all four arguments borrow live matching wrapper-owned
                // objects, the input is an immediate, and output is owner-local.
                // The compiled closed plan only reads runtime/input and writes
                // output; it neither dereferences nor retains the opaque activation.
                // It has no call, back edge or safepoint. self retains the finalized
                // mapping until this call returns; no Rust unwind crosses it.
                let status = unsafe {
                    (self.entry)(
                        &runtime,
                        std::ptr::from_mut(activation).cast(),
                        &input,
                        &mut output,
                    )
                };
                match NativeStatus::checked(status)? {
                    NativeStatus::Returned => {
                        let result =
                            ImmediateResult::from_word(output.word).map_err(
                                |error| match error {
                                    ImmediateResultError::NotImmediate => {
                                        ParallelEntryError::NonImmediateResult
                                    }
                                },
                            )?;
                        activation
                            .store_immediate(result)
                            .map_err(ParallelEntryError::Authority)
                    }
                    NativeStatus::UnsupportedAbi => Err(ParallelEntryError::UnsupportedAbi),
                }
            })
            .map_err(ParallelEntryError::Authority)?
    }
}
