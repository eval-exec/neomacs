//! Compile the actual private descriptor walks; no executable mapping or entry.
use super::*;
use cranelift_codegen::ir::{Function, Opcode, Signature, UserFuncName};
use cranelift_codegen::isa::{LookupError, OwnedTargetIsa};
use cranelift_codegen::settings::{self, Configurable};

#[derive(Clone, Copy, Debug)]
enum Target {
    X86_64,
    Aarch64,
}
impl Target {
    #[doc(hidden)]
    pub fn name(self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
        }
    }
    #[doc(hidden)]
    pub fn backend(self) -> BackendAvailability {
        match cranelift_codegen::isa::lookup_by_name(self.name()) {
            Ok(builder) => {
                let mut flags = settings::builder();
                flags
                    .set("opt_level", "speed")
                    .expect("known optimization level");
                BackendAvailability::Available(
                    builder
                        .finish(settings::Flags::new(flags))
                        .expect("target ISA"),
                )
            }
            Err(LookupError::SupportDisabled) => BackendAvailability::SupportDisabled,
            Err(LookupError::Unsupported) => panic!("inventoried target is unsupported: {self:?}"),
        }
    }
}
enum BackendAvailability {
    Available(OwnedTargetIsa),
    SupportDisabled,
}
#[derive(Clone, Copy, Debug)]
enum Walk {
    Single,
    Paired,
}
#[derive(Clone, Copy, Debug)]
enum LoadControl {
    ActualAtomic,
    OrdinaryNegative,
}
struct Probe {
    optimized_clif: String,
    vcode: String,
    bytes: Vec<u8>,
    atomic_loads: usize,
}

fn compile(target: Target, walk: Walk, control: LoadControl, isa: OwnedTargetIsa) -> Probe {
    let pointer = isa.pointer_type();
    assert_eq!(
        pointer,
        types::I64,
        "descriptor ABI requires 64-bit pointers"
    );
    let publication_offset = i32::try_from(offset_of!(ParallelRuntime<'_>, publication))
        .expect("cold descriptor ABI offset fits the instruction");
    let mut signature = Signature::new(isa.default_call_conv());
    for _ in 0..3 {
        signature.params.push(AbiParam::new(pointer));
    }
    signature.returns.push(AbiParam::new(pointer));
    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let mut frontend = FunctionBuilderContext::new();
    {
        let mut b = FunctionBuilder::new(&mut function, &mut frontend);
        let entry = b.create_block();
        b.append_block_params_for_function_params(entry);
        b.switch_to_block(entry);
        let args = b.block_params(entry).to_vec();
        let result = match (walk, control) {
            (Walk::Single, LoadControl::ActualAtomic) => {
                emit_slot_backing(&mut b, pointer, args[0], publication_offset, args[1])
                    .expect("actual single descriptor walk")
            }
            (Walk::Single, LoadControl::OrdinaryNegative)
            | (Walk::Paired, LoadControl::ActualAtomic)
            | (Walk::Paired, LoadControl::OrdinaryNegative) => {
                let descriptor = match control {
                    LoadControl::ActualAtomic => {
                        acquire_descriptor(&mut b, pointer, args[0], publication_offset)
                    }
                    LoadControl::OrdinaryNegative => {
                        // Negative control changes ONLY this descriptor publication load.
                        let slot = b.ins().load(
                            pointer,
                            MemFlagsData::trusted(),
                            args[0],
                            publication_offset,
                        );
                        b.ins().load(pointer, MemFlagsData::trusted(), slot, 0)
                    }
                };
                let first = slot_backing_from_descriptor(&mut b, pointer, descriptor, args[1])
                    .expect("first actual bounded walk");
                match walk {
                    Walk::Single => first,
                    Walk::Paired => {
                        let second =
                            slot_backing_from_descriptor(&mut b, pointer, descriptor, args[2])
                                .expect("second actual bounded walk");
                        // Both dynamic operands contribute; the compiler cannot discard a walk.
                        b.ins().band(first, second)
                    }
                }
            }
        };
        b.ins().return_(&[result]);
        b.seal_all_blocks();
        b.finalize(isa.frontend_config());
    }
    let mut context = cranelift_codegen::Context::for_function(function);
    context.set_disasm(true);
    let compiled = context
        .compile(&*isa, &mut Default::default())
        .expect("descriptor lowering");
    let vcode = compiled.vcode.clone().expect("requested final VCode");
    let bytes = compiled.buffer.data().to_vec();
    assert!(!bytes.is_empty());
    let mut atomic_loads = 0;
    for block in context.func.layout.blocks() {
        for inst in context.func.layout.block_insts(block) {
            let opcode = context.func.dfg.insts[inst].opcode();
            atomic_loads += usize::from(opcode == Opcode::AtomicLoad);
            assert!(
                !opcode.is_call(),
                "descriptor walk must not call: {opcode:?}"
            );
            assert!(
                !opcode.can_store(),
                "descriptor walk must not store: {opcode:?}"
            );
            assert!(
                !matches!(
                    opcode,
                    Opcode::AtomicRmw | Opcode::AtomicCas | Opcode::Fence
                ),
                "descriptor walk must not add RMW/fences: {opcode:?}"
            );
        }
    }
    let optimized_clif = context.func.display().to_string();
    let hex = bytes
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    println!(
        "NMJIT_N4_LOWERING_ARTIFACT {target:?} {walk:?} {control:?}\nCLIF:\n{optimized_clif}\nVCODE:\n{vcode}\nBYTES:{hex}"
    );
    Probe {
        optimized_clif,
        vcode,
        bytes,
        atomic_loads,
    }
}

fn check(target: Target, walk: Walk, control: LoadControl) {
    let isa = match target.backend() {
        BackendAvailability::Available(isa) => isa,
        BackendAvailability::SupportDisabled => {
            println!(
                "NMJIT_N4_LOWERING_BACKEND_REFUSED {target:?} {walk:?} {control:?} SupportDisabled"
            );
            return;
        }
    };
    let probe = compile(target, walk, control, isa);
    assert!(!probe.optimized_clif.is_empty() && !probe.bytes.is_empty());
    match control {
        LoadControl::ActualAtomic => assert_eq!(
            probe.atomic_loads, 1,
            "paired walks share one publication load"
        ),
        LoadControl::OrdinaryNegative => assert_eq!(probe.atomic_loads, 0),
    }
    let instructions = probe.vcode.to_ascii_lowercase();
    match target {
        Target::X86_64 => {
            assert!(
                instructions.contains("mov"),
                "x86 descriptor walk must load memory"
            );
            assert!(
                !instructions.split_whitespace().any(|word| word == "lock")
                    && !instructions.contains("mfence")
                    && !instructions.contains("lfence")
                    && !instructions.contains("sfence")
            );
            println!("NMJIT_N4_LOWERING_X86_MOV {walk:?} {control:?}");
        }
        Target::Aarch64 => match control {
            LoadControl::ActualAtomic => {
                assert_eq!(
                    instructions.matches("ldar ").count(),
                    1,
                    "one target Acquire publication read"
                );
                println!("NMJIT_N4_LOWERING_AARCH64_LDAR {walk:?}");
            }
            LoadControl::OrdinaryNegative => {
                assert!(!instructions.contains("ldar") && !instructions.contains("load_acquire"));
                assert!(
                    instructions.contains("ldr"),
                    "ordinary control must still load its descriptor"
                );
                println!("NMJIT_N4_ORDINARY_DESCRIPTOR_LOAD_LACKS_ACQUIRE {walk:?}");
            }
        },
    }
}

#[test]
fn nmjit_n4_actual_single_walk_lowers_publication_acquire() {
    for target in [Target::X86_64, Target::Aarch64] {
        check(target, Walk::Single, LoadControl::ActualAtomic);
    }
}
#[test]
fn nmjit_n4_actual_paired_walk_shares_one_publication_acquire() {
    for target in [Target::X86_64, Target::Aarch64] {
        check(target, Walk::Paired, LoadControl::ActualAtomic);
    }
}
#[test]
fn nmjit_n4_ordinary_descriptor_control_names_missing_acquire() {
    for target in [Target::X86_64, Target::Aarch64] {
        for walk in [Walk::Single, Walk::Paired] {
            check(target, walk, LoadControl::OrdinaryNegative);
        }
    }
}
