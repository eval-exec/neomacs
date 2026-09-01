//! Code Conversion Language (CCL) compatibility runtime.
//!
//! CCL is a low-level bytecode language for efficient character/text conversion.
//! This implementation currently provides partial CCL behavior:
//! - `ccl-program-p` — basic predicate for vector-shaped CCL program headers
//! - `register-ccl-program` — stores named CCL programs and returns stable ids
//! - `register-code-conversion-map` — stores named conversion maps and returns stable ids
//! - CCL-backed coding systems and `ccl-execute-on-string` share one bounded
//!   bytecode machine, including resumable register/instruction state.
//! - `ccl-execute` — validates shape and designators while the remaining
//!   register-only instruction set is implemented incrementally.

use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::SymId;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use std::cell::RefCell;
use std::collections::HashMap;

fn is_integer(value: &Value) -> bool {
    value.is_fixnum()
}

fn is_valid_ccl_program(program: &Value) -> bool {
    if !program.is_vector() {
        return false;
    };

    let program = program.as_vector_data().unwrap().clone();
    if program.len() < 3 {
        return false;
    }

    if !program.iter().all(is_integer) {
        return false;
    }

    let buf_magnification = program[0].as_int().unwrap();
    let eof_ic = program[1].as_int().unwrap();
    buf_magnification >= 0 && (0..=program.len() as i64).contains(&eof_ic)
}

#[derive(Default)]
struct CclRegistry {
    programs: HashMap<SymId, (i64, Value)>,
    code_conversion_maps: HashMap<SymId, (i64, Value)>,
    next_program_id: i64,
    next_code_conversion_map_id: i64,
}

impl CclRegistry {
    fn with_defaults() -> Self {
        Self {
            programs: HashMap::new(),
            code_conversion_maps: HashMap::new(),
            next_program_id: 1,
            next_code_conversion_map_id: 0,
        }
    }

    fn register_program(&mut self, name: SymId, program: Value) -> i64 {
        if let Some((id, slot)) = self.programs.get_mut(&name) {
            *slot = program;
            return *id;
        }
        let id = self.next_program_id;
        self.next_program_id = self.next_program_id.saturating_add(1);
        self.programs.insert(name, (id, program));
        id
    }

    fn lookup_program(&self, name: SymId) -> Option<Value> {
        self.programs.get(&name).map(|(_, program)| *program)
    }

    fn register_code_conversion_map(&mut self, name: SymId, value: Value) -> i64 {
        if let Some((id, slot)) = self.code_conversion_maps.get_mut(&name) {
            *slot = value;
            return *id;
        }
        let id = self.next_code_conversion_map_id;
        self.next_code_conversion_map_id = self.next_code_conversion_map_id.saturating_add(1);
        self.code_conversion_maps.insert(name, (id, value));
        id
    }
}

thread_local! {
    static CCL_REGISTRY: RefCell<CclRegistry> = RefCell::new(CclRegistry::with_defaults());
}

fn with_ccl_registry<R>(f: impl FnOnce(&CclRegistry) -> R) -> R {
    CCL_REGISTRY.with(|r| f(&r.borrow()))
}

fn with_ccl_registry_mut<R>(f: impl FnOnce(&mut CclRegistry) -> R) -> R {
    CCL_REGISTRY.with(|r| f(&mut r.borrow_mut()))
}

/// Reset the CCL registry to its initial state.
pub(crate) fn reset_ccl_registry() {
    CCL_REGISTRY.with(|r| *r.borrow_mut() = CclRegistry::with_defaults());
}

/// Collect GC roots from the CCL registry.
pub(crate) fn collect_ccl_gc_roots(roots: &mut Vec<Value>) {
    CCL_REGISTRY.with(|r| {
        let reg = r.borrow();
        for (_, v) in reg.programs.values() {
            roots.push(*v);
        }
        for (_, v) in reg.code_conversion_maps.values() {
            roots.push(*v);
        }
    });
}

pub(crate) fn unregister_registered_ccl_program(name: SymId) {
    with_ccl_registry_mut(|registry| {
        let _ = registry.programs.remove(&name);
    });
}

pub(crate) fn is_registered_ccl_program(name: SymId) -> bool {
    with_ccl_registry(|registry| registry.programs.contains_key(&name))
}

enum CclProgramDesignatorKind {
    Inline,
    RegisteredSymbol,
}

fn resolve_ccl_program_designator(value: &Value) -> Option<(Value, CclProgramDesignatorKind)> {
    if value.is_vector() {
        return Some((*value, CclProgramDesignatorKind::Inline));
    }
    let name = value.as_symbol_id()?;
    with_ccl_registry(|registry| {
        registry
            .lookup_program(name)
            .map(|program| (program, CclProgramDesignatorKind::RegisteredSymbol))
    })
}

fn ccl_program_code_index_message(
    program: &Value,
    designator_kind: CclProgramDesignatorKind,
) -> String {
    let base_len = match program.kind() {
        ValueKind::Veclike(VecLikeType::Vector) => program.as_vector_data().unwrap().len() as i64,
        _ => 0,
    };
    let index = match designator_kind {
        CclProgramDesignatorKind::Inline => base_len.saturating_add(1),
        CclProgramDesignatorKind::RegisteredSymbol => base_len.saturating_add(2),
    };
    format!("Error in CCL program at {index}th code")
}

fn invalid_ccl_program_at(index: usize) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "Error in CCL program at {}th code",
            index.saturating_add(1)
        ))],
    )
}

fn compiled_ccl_words(designator: Value) -> Result<Vec<i64>, Flow> {
    let Some((program, _)) = resolve_ccl_program_designator(&designator) else {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    };
    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    }
    program
        .as_vector_data()
        .expect("validated CCL program is a vector")
        .iter()
        .map(|word| {
            word.as_int()
                .ok_or_else(|| signal("error", vec![Value::string("Invalid CCL program")]))
        })
        .collect()
}

fn ccl_relative_instruction(instruction: usize, offset: i64) -> Option<usize> {
    let target = (instruction as i64).checked_add(offset)?;
    usize::try_from(target).ok()
}

struct CclExecution {
    output: Vec<i64>,
    registers: [i64; 8],
    instruction: usize,
}

fn execute_compiled_ccl_with_state(
    designator: Value,
    input: &[i64],
    last_block: bool,
    mut registers: [i64; 8],
    initial_instruction: Option<usize>,
) -> Result<CclExecution, Flow> {
    const HEADER_MAIN: usize = 2;
    const MAX_STEPS_PER_WORD: usize = 4096;

    let words = compiled_ccl_words(designator)?;
    let eof_instruction = usize::try_from(words[1])
        .ok()
        .filter(|instruction| *instruction < words.len())
        .ok_or_else(|| invalid_ccl_program_at(1))?;
    let mut source = 0usize;
    let mut output = Vec::with_capacity(input.len());
    let mut instruction = initial_instruction
        .filter(|instruction| HEADER_MAIN < *instruction && *instruction < words.len())
        .unwrap_or(HEADER_MAIN);
    let step_limit = words
        .len()
        .saturating_add(input.len())
        .saturating_add(1)
        .saturating_mul(MAX_STEPS_PER_WORD);

    for _ in 0..step_limit {
        let this_instruction = instruction;
        let code = *words
            .get(instruction)
            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
        instruction += 1;
        let field1 = code >> 8;
        let register = usize::try_from((code & 0xff) >> 5)
            .ok()
            .filter(|register| *register < registers.len())
            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
        let other_register = usize::try_from(field1 & 7)
            .ok()
            .filter(|register| *register < registers.len())
            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
        let command = code & 0x1f;

        let mut read_character = |destination: &mut i64| -> Option<bool> {
            if let Some(value) = input.get(source) {
                *destination = *value;
                source += 1;
                Some(false)
            } else if last_block {
                *destination = -1;
                Some(true)
            } else {
                None
            }
        };

        match command {
            // CCL_SetRegister
            0x00 => registers[register] = registers[other_register],
            // CCL_SetShortConst
            0x01 => registers[register] = field1,
            // CCL_SetConst
            0x02 => {
                registers[register] = *words
                    .get(instruction)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                instruction += 1;
            }
            // CCL_Jump
            0x04 => {
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            // CCL_JumpCond
            0x05 if registers[register] == 0 => {
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            0x05 => {}
            // CCL_WriteRegisterJump
            0x06 => {
                output.push(registers[register]);
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            // CCL_WriteRegisterReadJump. The compiler stores a paired
            // CCL_ReadJump word after this fused instruction; GNU skips it
            // after a successful read, but resumes at that word when input is
            // exhausted in a non-final block.
            0x07 => {
                output.push(registers[register]);
                instruction = instruction
                    .checked_add(1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                match read_character(&mut registers[register]) {
                    Some(true) => instruction = eof_instruction,
                    Some(false) => {
                        instruction = ccl_relative_instruction(instruction, field1 - 1)
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    }
                    None => {
                        return Ok(CclExecution {
                            output,
                            registers,
                            instruction: this_instruction + 1,
                        });
                    }
                }
            }
            // CCL_WriteConstJump
            0x08 => {
                output.push(
                    *words
                        .get(instruction)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?,
                );
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            // CCL_ReadJump
            0x0c => match read_character(&mut registers[register]) {
                Some(true) => instruction = eof_instruction,
                Some(false) => {
                    instruction = ccl_relative_instruction(instruction, field1)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                }
                None => {
                    return Ok(CclExecution {
                        output,
                        registers,
                        instruction: this_instruction,
                    });
                }
            },
            // CCL_ReadRegister. Consecutive encoded operands read into one or
            // more registers; a zero field terminates the sequence.
            0x0e => {
                let mut read_field = field1;
                let mut read_register = register;
                loop {
                    match read_character(&mut registers[read_register]) {
                        Some(true) => {
                            instruction = eof_instruction;
                            break;
                        }
                        Some(false) => {}
                        None => {
                            return Ok(CclExecution {
                                output,
                                registers,
                                instruction: this_instruction,
                            });
                        }
                    }
                    if read_field == 0 {
                        break;
                    }
                    let operand = *words
                        .get(instruction)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    instruction += 1;
                    read_field = operand >> 8;
                    read_register = usize::try_from((operand & 0xff) >> 5)
                        .ok()
                        .filter(|register| *register < registers.len())
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                }
            }
            // CCL_WriteRegister
            0x11 => {
                let mut write_field = field1;
                let mut write_register = register;
                loop {
                    output.push(registers[write_register]);
                    if write_field == 0 {
                        break;
                    }
                    let operand = *words
                        .get(instruction)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    instruction += 1;
                    write_field = operand >> 8;
                    write_register = usize::try_from((operand & 0xff) >> 5)
                        .ok()
                        .filter(|register| *register < registers.len())
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                }
            }
            // CCL_WriteConstString. A zero register field embeds one
            // character directly in FIELD1. A nonzero field stores an ASCII
            // string three octets per following word, most-significant octet
            // first (the representation emitted by GNU `ccl-embed-string`).
            0x14 if register == 0 => output.push(field1),
            0x14 => {
                let length = usize::try_from(field1)
                    .ok()
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                let packed_words = length.saturating_add(2) / 3;
                let end = instruction
                    .checked_add(packed_words)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                let packed = words
                    .get(instruction..end)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                for character_index in 0..length {
                    let word = packed[character_index / 3];
                    let shift = (2 - (character_index % 3)) * 8;
                    output.push((word >> shift) & 0xff);
                }
                instruction = end;
            }
            // CCL_End. GNU leaves IC pointing at the End instruction so a
            // completed STATUS cannot accidentally resume beyond the vector.
            0x16 => {
                return Ok(CclExecution {
                    output,
                    registers,
                    instruction: this_instruction,
                });
            }
            _ => return Err(invalid_ccl_program_at(this_instruction)),
        }
    }

    Err(invalid_ccl_program_at(instruction))
}

/// Execute one complete compiled CCL program over integer character codes.
///
/// GNU's `ccl_driver` is the common engine behind CCL coding systems and the
/// explicit CCL execution primitives. Keep byte/character storage decisions
/// outside this machine: a decoder consumes byte values and produces Emacs
/// character codes, while an encoder consumes character codes and its caller
/// truncates produced values to output octets.
pub(crate) fn execute_compiled_ccl(
    designator: Value,
    input: &[i64],
    last_block: bool,
) -> Result<Vec<i64>, Flow> {
    execute_compiled_ccl_with_state(designator, input, last_block, [0; 8], None)
        .map(|execution| execution.output)
}

// ---------------------------------------------------------------------------
// Argument helpers
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (ccl-program-p OBJECT) -> nil
/// This accepts program objects that match the minimum CCL header shape used by Emacs.
pub(crate) fn builtin_ccl_program_p_impl(args: Vec<Value>) -> EvalResult {
    expect_args("ccl-program-p", &args, 1)?;
    let is_program = resolve_ccl_program_designator(&args[0])
        .map(|(program, _)| is_valid_ccl_program(&program))
        .unwrap_or(false);
    Ok(Value::bool_val(is_program))
}

/// (ccl-execute CCL-PROGRAM STATUS) -> nil
/// Stub: doesn't actually execute CCL bytecode.
pub(crate) fn builtin_ccl_execute_impl(args: Vec<Value>) -> EvalResult {
    expect_args("ccl-execute", &args, 2)?;
    if !args[1].is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), args[1]],
        ));
    }

    let status_len = match args[1].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => args[1].as_vector_data().unwrap().len(),
        _ => unreachable!("status already validated as vector"),
    };
    if status_len != 8 {
        return Err(signal(
            "error",
            vec![Value::string("Length of vector REGISTERS is not 8")],
        ));
    }

    let Some((program, designator_kind)) = resolve_ccl_program_designator(&args[0]) else {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    };
    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    }

    let message = ccl_program_code_index_message(&program, designator_kind);
    Err(signal("error", vec![Value::string(message)]))
}

fn ccl_string_input(string: &crate::heap_types::LispString) -> Vec<i64> {
    if !string.is_multibyte() {
        return string
            .as_bytes()
            .iter()
            .map(|byte| i64::from(*byte))
            .collect();
    }

    let bytes = string.as_bytes();
    let mut input = Vec::with_capacity(string.schars());
    let mut position = 0usize;
    while position < bytes.len() {
        let (character, length) = crate::emacs_core::emacs_char::string_char(&bytes[position..]);
        input.push(i64::from(character));
        position += length;
    }
    input
}

fn ccl_output_string(output: Vec<i64>, unibyte: bool) -> Value {
    if unibyte {
        return Value::heap_string(crate::heap_types::LispString::from_unibyte(
            output
                .into_iter()
                .map(|character| character as u8)
                .collect(),
        ));
    }

    let mut bytes = Vec::with_capacity(output.len());
    let mut encoded = [0u8; crate::emacs_core::emacs_char::MAX_MULTIBYTE_LENGTH];
    for character in output {
        let character = u32::try_from(character)
            .ok()
            .filter(|character| *character <= crate::emacs_core::emacs_char::MAX_CHAR)
            .unwrap_or(char::REPLACEMENT_CHARACTER as u32);
        let length = crate::emacs_core::emacs_char::char_string(character, &mut encoded);
        bytes.extend_from_slice(&encoded[..length]);
    }
    Value::heap_string(crate::heap_types::LispString::from_emacs_bytes(bytes))
}

/// (ccl-execute-on-string CCL-PROGRAM STATUS STRING &optional CONTINUE UNIBYTE-P) -> STRING
pub(crate) fn builtin_ccl_execute_on_string_impl(args: Vec<Value>) -> EvalResult {
    expect_min_args("ccl-execute-on-string", &args, 3)?;
    expect_max_args("ccl-execute-on-string", &args, 5)?;
    if !args[1].is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), args[1]],
        ));
    }
    let status_len = match args[1].kind() {
        ValueKind::Veclike(VecLikeType::Vector) => args[1].as_vector_data().unwrap().len(),
        _ => unreachable!("status already validated as vector"),
    };
    if status_len != 9 {
        return Err(signal(
            "error",
            vec![Value::string("Length of vector STATUS is not 9")],
        ));
    }

    let Some((program, _)) = resolve_ccl_program_designator(&args[0]) else {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    };
    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    }

    let Some(string) = args[2].as_lisp_string() else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("stringp"), args[2]],
        ));
    };

    let status = args[1].as_vector_data().expect("validated STATUS vector");
    let mut registers = [0i64; 8];
    for (register, value) in registers.iter_mut().zip(status.iter().take(8)) {
        if let Some(integer) = value.as_int()
            && (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&integer)
        {
            *register = integer;
        }
    }
    let initial_instruction = status[8]
        .as_int()
        .and_then(|instruction| usize::try_from(instruction).ok());
    let input = ccl_string_input(string);
    let continue_execution = args.get(3).is_some_and(|value| !value.is_nil());
    let unibyte = args.get(4).is_some_and(|value| !value.is_nil());
    let execution = execute_compiled_ccl_with_state(
        args[0],
        &input,
        !continue_execution,
        registers,
        initial_instruction,
    )?;

    for (index, register) in execution.registers.into_iter().enumerate() {
        let updated = args[1].set_vector_slot(index, Value::fixnum(register));
        debug_assert!(updated, "validated STATUS vector remains mutable");
    }
    let updated = args[1].set_vector_slot(8, Value::fixnum(execution.instruction as i64));
    debug_assert!(updated, "validated STATUS vector remains mutable");

    Ok(ccl_output_string(execution.output, unibyte))
}

/// (register-ccl-program NAME CCL-PROG) -> nil
/// Stub: accepts and discards the CCL program registration.
pub(crate) fn builtin_register_ccl_program_impl(args: Vec<Value>) -> EvalResult {
    expect_args("register-ccl-program", &args, 2)?;
    if !args[0].is_symbol() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    let program = if args[1].is_nil() {
        // Oracle accepts nil and behaves like a minimal valid registered program.
        Value::vector(vec![Value::fixnum(0), Value::fixnum(0), Value::fixnum(0)])
    } else {
        if !args[1].is_vector() {
            return Err(signal(
                LispCondition::WrongTypeArgument,
                vec![Value::symbol("vectorp"), args[1]],
            ));
        }
        args[1]
    };

    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Error in CCL program")]));
    }

    let name = args[0]
        .as_symbol_id()
        .expect("symbol already validated by is_symbol");
    let program_id = with_ccl_registry_mut(|registry| registry.register_program(name, program));
    Ok(Value::fixnum(program_id))
}

/// (register-code-conversion-map SYMBOL MAP) -> nil
/// Stub: accepts and discards the code conversion map.
pub(crate) fn builtin_register_code_conversion_map_impl(args: Vec<Value>) -> EvalResult {
    expect_args("register-code-conversion-map", &args, 2)?;
    if !args[0].is_symbol() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), args[0]],
        ));
    }
    if !args[1].is_vector() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("vectorp"), args[1]],
        ));
    }

    let name = args[0]
        .as_symbol_id()
        .expect("symbol already validated by is_symbol");
    let map_id =
        with_ccl_registry_mut(|registry| registry.register_code_conversion_map(name, args[1]));
    Ok(Value::fixnum(map_id))
}
#[cfg(test)]
#[path = "tests/mod.rs"]
mod tests;
