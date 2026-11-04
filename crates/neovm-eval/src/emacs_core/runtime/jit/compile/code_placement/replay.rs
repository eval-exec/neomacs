//! A deliberately narrow proof for diagnostic x64 placement at arbitrary bytes.
//!
//! This consumes trusted Cranelift 0.134.3 VCode, never caller-supplied assembly.
//! In that version `MachBuffer::finish_constants` starts at the ISA minimum
//! alignment and raises it for constants; `VCodeConstantData::alignment` rounds
//! even scalar constants up to 8 bytes. Scalar x64 integer memory operands can
//! read those constants unaligned under the ordinary x64 ABI execution flags.
//! Code placement does not change stack or Lisp-data addresses or their flags.
//! RIP-relative LEA is refused: it could let a constant-pool address escape to
//! a callback that expects aligned data. Direct scalar pool reads do not expose
//! that address; `load_ext_name` refers to external objects whose addresses are
//! preserved by relocations.
//!
//! The whitelist below permits only integer/control instructions, the explicitly
//! unaligned vector move VMOVDQU, and the scalar 64-bit VMOVQ/VPINSRQ operations.
//! It excludes aligned SIMD loads/stores and all atomic sequences. The only
//! accepted synthetic operations are `uninit`/`unwind` (emit no instructions),
//! `load_ext_name` (MOVABS/LEA, or scalar MOV/ADD), and the `j` second branch in
//! Cranelift's conditional-branch display. Prologues, epilogues and register
//! allocator edits pass through the same VCode renderer. Unprinted constant
//! islands contain data and alignment-insensitive jump-around instructions.
//!
//! Grammar: nonempty lines are exactly `block<decimal>:` or an indented
//! instruction; each semicolon-separated instruction must have a whitelisted
//! mnemonic. Unwind records accept only the three current scalar-frame record
//! kinds. Unknown instructions/records, absent assembly, hidden block padding,
//! a changed backend version, or another ISA refuse the diagnostic. This is
//! intentionally not an x86 decoder or a production alignment policy.
//! The exact `;; implicit:` register/trap annotations emitted by the scalar
//! IMUL/CQTO/IDIV printers are metadata, and are validated separately.

use cranelift_codegen::CompiledCode;
use cranelift_codegen::isa::TargetIsa;

/// Immutable compiled code whose buffer alignment may safely be relaxed for a
/// diagnostic. Threading follows the borrowed compiler context; no Lisp state
/// or mutable process state is stored here. Fields cannot be constructed outside
/// the validator, so an unvalidated buffer cannot supply a relaxed alignment.
#[derive(Debug)]
pub(super) struct ValidatedReplay<'a> {
    code: &'a CompiledCode,
    allocation_alignment: u64,
}

#[derive(Clone, Debug, thiserror::Error)]
pub(crate) enum ReplayError {
    #[error("unaligned replay supports only the validated Cranelift 0.134.3 backend, found {0}")]
    UnsupportedVersion(&'static str),
    #[error("unaligned replay requires the x64 backend, found {0}")]
    UnsupportedIsa(&'static str),
    #[error("unaligned replay does not support hidden basic-block padding")]
    HiddenPadding,
    #[error("unaligned replay does not support a configured minimum function alignment")]
    ConfiguredAlignment,
    #[error("unaligned replay cannot establish the source of buffer alignment {0}")]
    UnsupportedBufferAlignment(u32),
    #[error("unaligned replay requires nonempty Cranelift VCode")]
    MissingAssembly,
    #[error("unaligned replay refuses VCode line {line}: {text}")]
    UnsupportedInstruction { line: usize, text: String },
}

impl<'a> ValidatedReplay<'a> {
    #[doc(hidden)]
    pub fn validate(code: &'a CompiledCode, isa: &dyn TargetIsa) -> Result<Self, ReplayError> {
        if cranelift_codegen::VERSION != "0.134.3" {
            return Err(ReplayError::UnsupportedVersion(cranelift_codegen::VERSION));
        }
        if isa.name() != "x64" {
            return Err(ReplayError::UnsupportedIsa(isa.name()));
        }
        if isa.flags().bb_padding_log2_minus_one() != 0 {
            return Err(ReplayError::HiddenPadding);
        }
        if isa.flags().log2_min_function_alignment() != 0 {
            return Err(ReplayError::ConfiguredAlignment);
        }
        let allocation_alignment =
            u64::from(isa.function_alignment().minimum).max(isa.symbol_alignment());
        // With the reviewed backend the only extra buffer alignment comes
        // from scalar (8-byte) or vector (16-byte) constants. Their accesses
        // are independently checked by the instruction whitelist below.
        if u64::from(code.buffer.alignment) != allocation_alignment
            && !matches!(code.buffer.alignment, 8 | 16)
        {
            return Err(ReplayError::UnsupportedBufferAlignment(
                code.buffer.alignment,
            ));
        }
        validate_vcode(code.vcode.as_deref())?;
        Ok(Self {
            code,
            allocation_alignment,
        })
    }

    #[doc(hidden)]
    pub fn allocation_alignment(&self) -> u64 {
        self.allocation_alignment
    }

    #[doc(hidden)]
    pub fn code_buffer(&self) -> &'a [u8] {
        self.code.code_buffer()
    }
}

fn validate_vcode(vcode: Option<&str>) -> Result<(), ReplayError> {
    let text = vcode
        .filter(|text| !text.trim().is_empty())
        .ok_or(ReplayError::MissingAssembly)?;
    let mut instructions = 0;
    for (index, line) in text.lines().enumerate() {
        let unsupported = || ReplayError::UnsupportedInstruction {
            line: index + 1,
            text: line.to_owned(),
        };
        if line.trim().is_empty() {
            continue;
        }
        if let Some(number) = line
            .strip_prefix("block")
            .and_then(|line| line.strip_suffix(':'))
        {
            if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            return Err(unsupported());
        }
        let instruction = line.strip_prefix("  ").ok_or_else(unsupported)?;
        let instruction =
            if let Some((instruction, annotation)) = instruction.split_once(" ;; implicit: ") {
                let mnemonic = instruction
                    .split_whitespace()
                    .next()
                    .ok_or_else(unsupported)?;
                let valid = match mnemonic {
                    "imulq" => annotation == "%rax, %rdx",
                    "cqto" => annotation == "%rdx, %rax",
                    "idivq" => annotation
                        .strip_prefix("%rax, %rdx, trap=")
                        .is_some_and(|code| code.parse::<u8>().is_ok()),
                    _ => false,
                };
                if !valid {
                    return Err(unsupported());
                }
                instruction
            } else {
                instruction
            };
        let mut previous_mnemonic = None;
        for (part_index, part) in instruction.split(';').enumerate() {
            let mut words = part.split_whitespace();
            let mnemonic = words.next().ok_or_else(unsupported)?;
            if mnemonic == "leaq" && part.contains("%rip") {
                return Err(unsupported());
            }
            if mnemonic == "unwind" {
                if part_index != 0 || instruction.contains(';') {
                    return Err(unsupported());
                }
                match words.next() {
                    Some("PushFrameRegs" | "DefineNewFrame" | "SaveReg") => continue,
                    _ => return Err(unsupported()),
                }
            }
            // The unsuffixed `j` is only the second part of a displayed
            // conditional branch (e.g. "jz label1; j label2").
            if mnemonic == "j" {
                if part_index != 1 || !previous_mnemonic.is_some_and(is_conditional_branch) {
                    return Err(unsupported());
                }
            } else if part_index != 0 {
                return Err(unsupported());
            } else if !is_alignment_insensitive(mnemonic) {
                return Err(unsupported());
            }
            if mnemonic != "uninit" {
                instructions += 1;
            }
            previous_mnemonic = Some(mnemonic);
        }
    }
    if instructions == 0 {
        return Err(ReplayError::MissingAssembly);
    }
    Ok(())
}

fn is_conditional_branch(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "jb" | "jbe" | "jl" | "jnb" | "jnbe" | "jnl" | "jnle" | "jnz" | "jz"
    )
}

fn is_alignment_insensitive(mnemonic: &str) -> bool {
    matches!(
        mnemonic,
        "addq"
            | "andl"
            | "andq"
            | "call"
            | "cmoveq"
            | "cmovgeq"
            | "cmovgq"
            | "cmovleq"
            | "cmovlq"
            | "cmovneq"
            | "cmpb"
            | "cmpq"
            | "cqto"
            | "idivq"
            | "imulq"
            | "jb"
            | "jbe"
            | "jl"
            | "jmp"
            | "jnb"
            | "jnbe"
            | "jnl"
            | "jnle"
            | "jnz"
            | "jz"
            | "leaq"
            | "load_ext_name"
            | "movabsq"
            | "movb"
            | "movl"
            | "movq"
            | "movzbq"
            | "movzwq"
            | "orl"
            | "orq"
            | "popq"
            | "pushq"
            | "retq"
            | "sarq"
            | "sbbq"
            | "setb"
            | "setbe"
            | "sete"
            | "setge"
            | "setle"
            | "setne"
            | "seto"
            | "shlq"
            | "shrq"
            | "subq"
            | "testb"
            | "testq"
            | "uninit"
            | "vmovdqu"
            | "vmovq"
            | "vpinsrq"
            | "xorl"
            | "xorq"
    )
}

#[cfg(test)]
#[path = "replay/tests/replay_test.rs"]
mod tests;
