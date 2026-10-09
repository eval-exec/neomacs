//! Compile-only native probes of the actual descriptor publication seam.
//! These never execute generated code or evaluate Lisp. Optional artifacts
//! retain optimized CLIF, regallocated VCode and the final emitted buffer.

use super::*;
use cranelift_codegen::ir::{AbiParam, Function, Opcode, Signature, UserFuncName};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::FunctionBuilderContext;

#[derive(Clone, Copy, Debug)]
enum PublicationMode {
    Protected,
    UnprotectedOrdinary,
    UnprotectedAtomic,
}

#[derive(Clone, Copy, Debug)]
enum ProbePurpose {
    RepeatedPublication,
    NeighboringPlainMemory,
}

static_assertions::assert_impl_all!(PublicationMode: Copy, Clone, std::fmt::Debug, Send, Sync);
static_assertions::assert_impl_all!(ProbePurpose: Copy, Clone, std::fmt::Debug, Send, Sync);

#[derive(Debug)]
struct Probe {
    optimized_clif: String,
    vcode: String,
    machine_code: Vec<u8>,
    memory_operations: Vec<Opcode>,
    stores: usize,
    sequence_points: usize,
}

static_assertions::assert_impl_all!(Probe: std::fmt::Debug, Send, Sync);
static_assertions::assert_not_impl_any!(Probe: Copy, Clone);

fn isa() -> cranelift_codegen::isa::OwnedTargetIsa {
    let mut flags = settings::builder();
    flags.set("opt_level", "speed").expect("known setting");
    cranelift_native::builder()
        .expect("supported native target")
        .finish(settings::Flags::new(flags))
        .expect("native ISA")
}

fn compile_probe(case: &str, boolean: bool, mode: PublicationMode, purpose: ProbePurpose) -> Probe {
    let isa = isa();
    let pointer = isa.pointer_type();
    let width = if boolean { types::I8 } else { types::I64 };
    let mut signature = Signature::new(isa.default_call_conv());
    signature.params.push(AbiParam::new(pointer));
    signature.params.push(AbiParam::new(width));
    if matches!(purpose, ProbePurpose::NeighboringPlainMemory) {
        signature.params.push(AbiParam::new(types::I64));
    }
    signature.returns.push(AbiParam::new(types::I64));
    let mut function = Function::with_name_signature(UserFuncName::user(0, 0), signature);
    let mut builder_context = FunctionBuilderContext::new();
    {
        let mut fb = FunctionBuilder::new(&mut function, &mut builder_context);
        let entry = fb.create_block();
        fb.append_block_params_for_function_params(entry);
        fb.switch_to_block(entry);
        fb.seal_block(entry);
        let descriptor = fb.block_params(entry)[0];
        let replacement = fb.block_params(entry)[1];
        let policy = ForwardAtomics::for_isa(&*isa);
        let slow = fb.create_block();
        let permit = super::super::inline_vars::guard_probe_mark_idle(&mut fb, slow);
        let offset = if boolean { 1 } else { 8 };
        let (publications, result_before) = match purpose {
            ProbePurpose::RepeatedPublication => {
                let original = if boolean {
                    load_bool(&mut fb, descriptor, offset)
                } else {
                    load_word(&mut fb, descriptor, offset)
                };
                (2, Some(original))
            }
            ProbePurpose::NeighboringPlainMemory => {
                // Different mutable locations around the publication. No
                // readonly/can_move/alias hints permit speculative motion.
                let payload = fb.block_params(entry)[2];
                fb.ins()
                    .store(MemFlagsData::trusted(), payload, descriptor, 24);
                (1, None)
            }
        };
        for _ in 0..publications {
            match mode {
                PublicationMode::Protected => {
                    if boolean {
                        policy.store_bool(&mut fb, &permit, descriptor, offset, replacement);
                    } else {
                        policy.store_word(&mut fb, &permit, descriptor, offset, replacement);
                    }
                }
                PublicationMode::UnprotectedOrdinary => {
                    // Exact x86 control: a repeated ordinary store is
                    // redundant to alias analysis without sequence points.
                    let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
                    fb.ins()
                        .store(MemFlagsData::trusted(), replacement, address, 0);
                }
                PublicationMode::UnprotectedAtomic => {
                    // Exact pinned-compiler control: the store check happens
                    // before AtomicStore updates its fence state. A second
                    // identical atomic store can therefore be eliminated too.
                    let address = fb.ins().iadd_imm_s(descriptor, offset as i64);
                    fb.ins()
                        .atomic_store(MemFlagsData::trusted(), replacement, address);
                }
            }
        }
        let result = result_before.unwrap_or_else(|| {
            fb.ins()
                .load(types::I64, MemFlagsData::trusted(), descriptor, 32)
        });
        fb.ins().return_(&[result]);
        fb.switch_to_block(slow);
        fb.seal_block(slow);
        let refused = fb.ins().iconst(types::I64, 0);
        fb.ins().return_(&[refused]);
        fb.finalize(isa.frontend_config());
    }
    let mut context = cranelift_codegen::Context::for_function(function);
    context.set_disasm(true);
    let compiled = context
        .compile(&*isa, &mut Default::default())
        .expect("descriptor probe compiles");
    // CompiledCode's disassembly is generated during final emission, after
    // regallocation. Retain final bytes independently of that textual view.
    let vcode = compiled.vcode.clone().expect("requested VCode disassembly");
    let machine_code = compiled.buffer.data().to_vec();
    assert!(!machine_code.is_empty(), "final code buffer exists");
    let mut stores = 0;
    let mut sequence_points = 0;
    let mut memory_operations = Vec::new();
    for block in context.func.layout.blocks() {
        for inst in context.func.layout.block_insts(block) {
            let data = &context.func.dfg.insts[inst];
            let opcode = data.opcode();
            stores += usize::from(matches!(opcode, Opcode::Store | Opcode::AtomicStore));
            sequence_points += usize::from(opcode == Opcode::SequencePoint);
            if opcode.can_load() || opcode.can_store() {
                let flags = data.memflags_data(&context.func.dfg).expect("memory flags");
                assert!(flags.aligned() && flags.notrap());
                assert!(!flags.readonly() && !flags.can_move());
                assert!(flags.alias_region().is_none());
                memory_operations.push(opcode);
            }
        }
    }
    let probe = Probe {
        optimized_clif: context.func.display().to_string(),
        vcode,
        machine_code,
        memory_operations,
        stores,
        sequence_points,
    };
    if let Some(directory) = std::env::var_os("NEOVM_P74_CODEGEN_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).expect("owned codegen evidence directory");
        let shape = if boolean { "bool" } else { "word" };
        let directory = directory.join(format!("{case}-{}-{shape}", isa.name()));
        // A fresh namespace prevents a repeated gate overwriting earlier
        // observations. The runner supplies a different root for each flow.
        std::fs::create_dir(&directory).expect("fresh per-probe evidence directory");
        std::fs::write(directory.join("optimized.clif"), &probe.optimized_clif)
            .expect("archive optimized CLIF");
        std::fs::write(directory.join("regallocated.vcode"), &probe.vcode)
            .expect("archive regallocated VCode");
        std::fs::write(directory.join("machine-code.bin"), &probe.machine_code)
            .expect("archive final code buffer");
        // All string fields are private fixed test labels, closed enum
        // names, or Cranelift's backend name. No optional JSON feature is
        // needed in either of the normal core flow configurations.
        let receipt = format!(
            r#"{{
  "case": "{case}",
  "isa": "{isa_name}",
  "boolean": {boolean},
  "publication_mode": "{mode:?}",
  "purpose": "{purpose:?}",
  "stores": {stores},
  "sequence_points": {sequence_points},
  "machine_code_bytes": {machine_code_bytes},
  "scope": "compiled only; generated code not executed"
}}
"#,
            isa_name = isa.name(),
            stores = probe.stores,
            sequence_points = probe.sequence_points,
            machine_code_bytes = probe.machine_code.len(),
        );
        std::fs::write(directory.join("receipt.json"), receipt).expect("archive probe receipt");
    }
    probe
}

#[test]
fn atomic_forward_codegen_keeps_repeated_publication_stores() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "repeated-publication",
            boolean,
            PublicationMode::Protected,
            ProbePurpose::RepeatedPublication,
        );
        assert_eq!(
            probe.stores, 2,
            "both publications survive: {}",
            probe.vcode
        );
        assert_eq!(probe.sequence_points, 4);
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn atomic_forward_codegen_x86_uses_mov_without_hardware_fence_or_call() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "x86-mov",
            boolean,
            PublicationMode::Protected,
            ProbePurpose::RepeatedPublication,
        );
        assert_eq!(probe.stores, 2);
        assert_eq!(probe.sequence_points, 4);
        assert!(
            probe.vcode.contains("mov"),
            "native load/store: {}",
            probe.vcode
        );
        let tokens: Vec<_> = probe
            .vcode
            .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '_')
            .collect();
        for forbidden in ["mfence", "lfence", "sfence", "lock", "xchg", "call"] {
            assert!(
                !tokens.iter().any(|token| token.starts_with(forbidden)),
                "unexpected {forbidden}: {}",
                probe.vcode
            );
        }
    }
}

#[cfg(target_arch = "x86_64")]
#[test]
fn atomic_forward_codegen_without_sequence_points_folds_repeated_store() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "unprotected-ordinary",
            boolean,
            PublicationMode::UnprotectedOrdinary,
            ProbePurpose::RepeatedPublication,
        );
        assert_eq!(
            probe.stores, 1,
            "ordinary negative control: {}",
            probe.vcode
        );
        assert_eq!(probe.sequence_points, 0);
    }
}

#[test]
fn atomic_forward_codegen_unprotected_atomic_store_exposes_pinned_idempotent_folding() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "unprotected-atomic",
            boolean,
            PublicationMode::UnprotectedAtomic,
            ProbePurpose::RepeatedPublication,
        );
        assert_eq!(probe.stores, 1, "atomic negative control: {}", probe.vcode);
        assert_eq!(probe.sequence_points, 0);
    }
}

#[test]
fn atomic_forward_codegen_preserves_neighboring_plain_memory_order() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "neighboring-memory",
            boolean,
            PublicationMode::Protected,
            ProbePurpose::NeighboringPlainMemory,
        );
        let publication = if cfg!(target_arch = "x86_64") {
            Opcode::Store
        } else {
            Opcode::AtomicStore
        };
        assert_eq!(
            probe.memory_operations,
            [Opcode::Store, publication, Opcode::Load],
            "optimized payload store / publication / payload load: {}",
            probe.optimized_clif
        );
        assert_eq!(probe.sequence_points, 2);
        // The sequence-point pseudo instructions remain in regallocated
        // VCode even though they emit zero final bytes. Separate plain
        // payload memory operations must remain outside those boundaries.
        let lines: Vec<_> = probe.vcode.lines().collect();
        let points: Vec<_> = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| line.contains("sequence_point").then_some(index))
            .collect();
        assert_eq!(
            points.len(),
            2,
            "VCode sequence boundaries: {}",
            probe.vcode
        );
        let has_offset = |line: &str, decimal: &str, hexadecimal: &str| {
            line.split(|ch: char| !ch.is_ascii_alphanumeric())
                .any(|token| token == decimal || token == hexadecimal)
        };
        let payload_store = |line: &&str| {
            has_offset(line, "24", "0x18")
                && if cfg!(target_arch = "x86_64") {
                    line.contains("mov")
                        && (line.contains('(') || line.contains('['))
                        && !line.contains("rsp")
                        && !line.contains("rbp")
                } else {
                    line.contains("str")
                }
        };
        let payload_load = |line: &&str| {
            has_offset(line, "32", "0x20")
                && if cfg!(target_arch = "x86_64") {
                    line.contains("mov")
                        && (line.contains('(') || line.contains('['))
                        && !line.contains("rsp")
                        && !line.contains("rbp")
                } else {
                    line.contains("ldr")
                }
        };
        assert!(
            lines[..points[0]].iter().any(payload_store),
            "payload store remains before publication: {}",
            probe.vcode
        );
        assert!(
            lines[points[1] + 1..].iter().any(payload_load),
            "payload load remains after publication: {}",
            probe.vcode
        );
    }
}

#[cfg(target_arch = "aarch64")]
#[test]
fn atomic_forward_codegen_arm64_uses_acquire_release_instructions() {
    for boolean in [false, true] {
        let probe = compile_probe(
            "arm64-acquire-release",
            boolean,
            PublicationMode::Protected,
            ProbePurpose::RepeatedPublication,
        );
        assert_eq!(probe.stores, 2);
        assert_eq!(probe.sequence_points, 4);
        let (load, store) = if boolean {
            ("ldarb", "stlrb")
        } else {
            ("ldar", "stlr")
        };
        assert!(
            probe.vcode.contains(load),
            "missing acquire load: {}",
            probe.vcode
        );
        assert!(
            probe.vcode.contains(store),
            "missing release store: {}",
            probe.vcode
        );
        for forbidden in ["dmb", "bl ", "blr "] {
            assert!(
                !probe.vcode.contains(forbidden),
                "unexpected {forbidden}: {}",
                probe.vcode
            );
        }
    }
}
