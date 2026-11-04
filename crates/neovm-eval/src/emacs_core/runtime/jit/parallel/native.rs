//! Private native mapping owner and scalar emitter. Never a serial cache.
use super::*;
use cranelift_codegen::ir::{AbiParam, InstBuilder, MemFlagsData, condcodes::IntCC, types};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{Linkage, Module, default_libcall_names};
use neovm_value::tagged::gc::symbol_table::abi::*;
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
    #[cfg(test)]
    pub(super) emission: String,
}

impl NativeCompiledCode {
    /// Called only by the parent while it borrows the real pre-era witness.
    /// This private child has no World or shared compiler/cache side effect.
    #[doc(hidden)]
    pub fn compile(plan: EntryPlan) -> Result<Self, ParallelCompileError> {
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
        let cell_input_offset = i32::try_from(offset_of!(CellInput, slot))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let hof_mapper_offset = i32::try_from(offset_of!(HofInput, mapper))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let hof_callback_offset = i32::try_from(offset_of!(HofInput, callback))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let output_offset = i32::try_from(offset_of!(ScalarOutput, word))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let publication_offset = i32::try_from(offset_of!(ParallelRuntime<'_>, publication))
            .map_err(|_| ParallelCompileError::AbiLayout)?;
        let mut signature = module.make_signature();
        for _ in 0..4 {
            signature.params.push(AbiParam::new(pointer));
        }
        signature.returns.push(AbiParam::new(types::I32));
        let name = match &plan {
            EntryPlan::Scalar(_) => "neovm_parallel_scalar_v1",
            EntryPlan::Cell => "neovm_parallel_cell_v1",
            EntryPlan::Hof => "neovm_parallel_hof_v1",
        };
        let id = module
            .declare_function(name, Linkage::Local, &signature)
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
            match plan {
                EntryPlan::Scalar(program) => {
                    let word = match program {
                        ScalarProgram::Constant(value) => {
                            b.ins().iconst(pointer, value.word() as i64)
                        }
                        ScalarProgram::Argument => {
                            b.ins()
                                .load(pointer, MemFlagsData::trusted(), args[2], input_offset)
                        }
                        ScalarProgram::HasSlotBacking(slot) => {
                            let requested = b.ins().iconst(pointer, slot.get() as i64);
                            emit_slot_backing(
                                &mut b,
                                pointer,
                                args[0],
                                publication_offset,
                                requested,
                            )?
                        }
                    };
                    b.ins()
                        .store(MemFlagsData::trusted(), word, args[3], output_offset);
                    let returned = b.ins().iconst(types::I32, NativeStatus::Returned as i64);
                    b.ins().return_(&[returned]);
                }
                EntryPlan::Cell => {
                    let requested =
                        b.ins()
                            .load(pointer, MemFlagsData::trusted(), args[2], cell_input_offset);
                    let answer =
                        emit_slot_backing(&mut b, pointer, args[0], publication_offset, requested)?;
                    let covered =
                        b.ins()
                            .icmp_imm(IntCC::Equal, answer, ImmediateResult::True.word() as i64);
                    let ready = b
                        .ins()
                        .iconst(types::I32, NativeStatus::CellReadReady as i64);
                    let absent = b.ins().iconst(types::I32, NativeStatus::SlotAbsent as i64);
                    let status = b.ins().select(covered, ready, absent);
                    b.ins().return_(&[status]);
                }
                EntryPlan::Hof => {
                    let mapper =
                        b.ins()
                            .load(pointer, MemFlagsData::trusted(), args[2], hof_mapper_offset);
                    let callback = b.ins().load(
                        pointer,
                        MemFlagsData::trusted(),
                        args[2],
                        hof_callback_offset,
                    );
                    // A single publication observation supplies both walks.
                    // Their bounds and addresses cannot mix descriptor eras.
                    let descriptor =
                        acquire_descriptor(&mut b, pointer, args[0], publication_offset);
                    let mapper_backing =
                        slot_backing_from_descriptor(&mut b, pointer, descriptor, mapper)?;
                    let callback_backing =
                        slot_backing_from_descriptor(&mut b, pointer, descriptor, callback)?;
                    let mapper_covered = b.ins().icmp_imm(
                        IntCC::Equal,
                        mapper_backing,
                        ImmediateResult::True.word() as i64,
                    );
                    let callback_covered = b.ins().icmp_imm(
                        IntCC::Equal,
                        callback_backing,
                        ImmediateResult::True.word() as i64,
                    );
                    let covered = b.ins().band(mapper_covered, callback_covered);
                    let ready = b
                        .ins()
                        .iconst(types::I32, NativeStatus::CellReadReady as i64);
                    let absent = b.ins().iconst(types::I32, NativeStatus::SlotAbsent as i64);
                    let status = b.ins().select(covered, ready, absent);
                    b.ins().return_(&[status]);
                }
            }
            b.switch_to_block(refused);
            let unsupported = b
                .ins()
                .iconst(types::I32, NativeStatus::UnsupportedAbi as i64);
            b.ins().return_(&[unsupported]);
            b.seal_all_blocks();
            b.finalize(config);
        }
        #[cfg(test)]
        let emission = context.func.display().to_string();
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
            #[cfg(test)]
            emission,
        })
    }

    /// Validate production era/owner/result identity before native execution.
    /// The borrowed activation keeps IN_SLICE throughout the bounded call.
    #[doc(hidden)]
    pub fn run<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        input: ImmediateResult,
    ) -> Result<(), ParallelEntryError> {
        admission
            .with_activation(run, destination, |activation| {
                let activation_address = std::ptr::from_mut(activation).cast();
                let runtime = ParallelRuntime {
                    version: NativeAbiVersion::V1,
                    publication: activation.publication(),
                };
                let input = ScalarInput { word: input.word() };
                let mut output = ScalarOutput {
                    word: ImmediateResult::Nil.word(),
                };
                // SAFETY: all four arguments borrow live matching wrapper-owned
                // objects, the input is an immediate, and output is owner-local.
                // The compiled closed plan reads runtime/input and immutable
                // descriptor metadata, then writes output. The runtime borrow
                // retains the publication slot and every predecessor until this
                // return; only an atomic Acquire-or-stronger load reads the slot.
                // It neither dereferences nor retains the opaque activation.
                // It has no call, back edge or safepoint. self retains the finalized
                // mapping until this call returns; no Rust unwind crosses it.
                let status = unsafe {
                    (self.entry)(
                        &runtime,
                        activation_address,
                        std::ptr::from_ref(&input).cast(),
                        &mut output,
                    )
                };
                drop(runtime);
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
                    NativeStatus::CellReadReady | NativeStatus::SlotAbsent => {
                        Err(ParallelEntryError::UnexpectedEntryKind)
                    }
                }
            })
            .map_err(ParallelEntryError::Authority)?
    }
    #[doc(hidden)]
    pub fn run_cell<'era>(
        &self,
        operation: CellRead,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        symbol: &SymbolHandle,
    ) -> Result<(), ParallelEntryError> {
        // This self borrow retains the unique executable mapping until both
        // the native lookup and its sealed epilogue have returned.
        // SAFETY: the private constructor emits only the verified closed
        // four-pointer ABI, with no mutable cell read or helper/safepoint.
        // self retains its executable mappings through the complete epilogue.
        unsafe { super::cell::execute(self.entry, operation, admission, run, destination, symbol) }
    }
    #[doc(hidden)]
    pub fn run_named<'era>(
        &self,
        admission: &ParallelExecutionAdmission<'era>,
        run: &mut RunAccess<'_, 'era>,
        destination: &OwnerRootSlot<'era>,
        target: &SymbolHandle,
        designator: &SymbolHandle,
        expected: &OwnerRootSlot<'era>,
        selected: &mut FunctionActivation<'era>,
    ) -> Result<FunctionSelection, ParallelEntryError> {
        // SAFETY: the private compiler emits the closed descriptor kernel;
        // this borrow retains its unique mappings through rooted selection.
        unsafe {
            super::identity::execute(
                self.entry,
                admission,
                run,
                destination,
                target,
                designator,
                expected,
                selected,
            )
        }
    }
    #[doc(hidden)]
    pub fn lookup_hof(
        &self,
        activation: &mut neovm_value::tagged::gc::ExecutionActivation<'_, '_, '_>,
        mapper: &SymbolHandle,
        callback: &SymbolHandle,
    ) -> Result<(), ParallelEntryError> {
        // SAFETY: the private compiler emits only the closed four-pointer ABI.
        // This borrow retains its unique executable mappings through the
        // bounded paired descriptor lookup, and the activation retains the
        // matching publication and its predecessors until the call returns.
        unsafe { super::hof::lookup(self.entry, activation, mapper, callback) }
    }
}

/// One atomic descriptor identity supplies every bound and pointer. This
/// structural probe never dereferences a symbol cell or returns a raw address.
fn emit_slot_backing(
    b: &mut FunctionBuilder<'_>,
    pointer: cranelift_codegen::ir::Type,
    runtime: cranelift_codegen::ir::Value,
    publication_offset: i32,
    requested: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, ParallelCompileError> {
    let descriptor = acquire_descriptor(b, pointer, runtime, publication_offset);
    slot_backing_from_descriptor(b, pointer, descriptor, requested)
}

fn acquire_descriptor(
    b: &mut FunctionBuilder<'_>,
    pointer: cranelift_codegen::ir::Type,
    runtime: cranelift_codegen::ir::Value,
    publication_offset: i32,
) -> cranelift_codegen::ir::Value {
    let slot_address = b.ins().load(
        pointer,
        MemFlagsData::trusted(),
        runtime,
        publication_offset,
    );
    // Cranelift 0.134's atomic_load is SeqCst, hence at least Acquire:
    // x86 TSO mov, aarch64 load_acquire. MemFlags alone supply no ordering.
    b.ins()
        .atomic_load(pointer, MemFlagsData::trusted(), slot_address)
}

fn slot_backing_from_descriptor(
    b: &mut FunctionBuilder<'_>,
    pointer: cranelift_codegen::ir::Type,
    descriptor: cranelift_codegen::ir::Value,
    requested: cranelift_codegen::ir::Value,
) -> Result<cranelift_codegen::ir::Value, ParallelCompileError> {
    let offset = |position: DescriptorOffset| {
        i32::try_from(position.get()).map_err(|_| ParallelCompileError::AbiLayout)
    };
    let slots_offset = offset(TABLE_SLOT_COUNT_OFFSET)?;
    let chunks_offset = offset(TABLE_CHUNK_COUNT_OFFSET)?;
    let chunk_slots_offset = offset(TABLE_CHUNK_SLOTS_OFFSET)?;
    let addresses_offset = offset(TABLE_ADDRESSES_OFFSET)?;
    let stride =
        i64::try_from(TABLE_ADDRESS_STRIDE.get()).map_err(|_| ParallelCompileError::AbiLayout)?;
    let within = b.create_block();
    let chunk_check = b.create_block();
    let address = b.create_block();
    let absent = b.create_block();
    let joined = b.create_block();
    b.append_block_param(joined, pointer);
    let slots = b
        .ins()
        .load(pointer, MemFlagsData::trusted(), descriptor, slots_offset);
    let covered = b.ins().icmp(IntCC::UnsignedLessThan, requested, slots);
    b.ins().brif(covered, within, &[], absent, &[]);
    b.switch_to_block(within);
    let chunk_slots = b.ins().load(
        pointer,
        MemFlagsData::trusted(),
        descriptor,
        chunk_slots_offset,
    );
    // The descriptor type guarantees NonZeroUsize. Keep the ABI decoder
    // total even if a later ABI version changes that representation.
    let nonzero = b.ins().icmp_imm(IntCC::NotEqual, chunk_slots, 0);
    b.ins().brif(nonzero, chunk_check, &[], absent, &[]);
    b.switch_to_block(chunk_check);
    let index = b.ins().udiv(requested, chunk_slots);
    let chunks = b
        .ins()
        .load(pointer, MemFlagsData::trusted(), descriptor, chunks_offset);
    let valid = b.ins().icmp(IntCC::UnsignedLessThan, index, chunks);
    b.ins().brif(valid, address, &[], absent, &[]);
    b.switch_to_block(address);
    let addresses = b.ins().load(
        pointer,
        MemFlagsData::trusted(),
        descriptor,
        addresses_offset,
    );
    // The immutable validated shape bounds index; its allocated address-array
    // length times this stride fits isize::MAX. No integer-derived pointer.
    let displacement = b.ins().imul_imm(index, stride);
    let entry = b.ins().iadd(addresses, displacement);
    let chunk = b.ins().load(pointer, MemFlagsData::trusted(), entry, 0);
    let present = b.ins().icmp_imm(IntCC::NotEqual, chunk, 0);
    let yes = b.ins().iconst(pointer, ImmediateResult::True.word() as i64);
    let no = b.ins().iconst(pointer, ImmediateResult::Nil.word() as i64);
    let answer = b.ins().select(present, yes, no);
    b.ins().jump(joined, &[answer.into()]);
    b.switch_to_block(absent);
    let no = b.ins().iconst(pointer, ImmediateResult::Nil.word() as i64);
    b.ins().jump(joined, &[no.into()]);
    b.switch_to_block(joined);
    Ok(b.block_params(joined)[0])
}

#[cfg(test)]
#[path = "native/tests/lowering_test.rs"]
mod lowering_tests;
