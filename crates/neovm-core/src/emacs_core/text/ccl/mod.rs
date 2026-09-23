//! Code Conversion Language (CCL) compatibility runtime.
//!
//! CCL is a low-level bytecode language for efficient character/text conversion.
//! This implementation currently provides partial CCL behavior:
//! - `ccl-program-p` — basic predicate for vector-shaped CCL program headers
//! - `register-ccl-program` — stores named CCL programs and returns stable ids
//! - `register-code-conversion-map` — stores named conversion maps and returns stable ids
//! - CCL-backed coding systems and `ccl-execute-on-string` share one bounded
//!   bytecode machine, including resumable register/instruction state.
//! - Each 5-bit opcode decodes to [`command::CclCommand`]. The driver matches
//!   that enum exhaustively; commands not yet executed still signal
//!   `Error in CCL program`.
//! - `ccl-execute` — validates shape and designators while the remaining
//!   register-only instruction set is implemented incrementally.

mod command;
mod expr;
mod extension;

use self::command::CclCommand;
use self::expr::{eval_expr_self, eval_set_expr};
use self::extension::{ExtensionStep, execute_extension};
use super::error::{EvalResult, Flow, signal};
use super::value::*;
use crate::emacs_core::SymId;
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use std::cell::Cell;
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

    if !program.iter().all(ccl_word_shape) {
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
    /// Integer-to-integer tables for `lookup-integer` / `lookup-character`
    /// when a test registers them directly. Lisp execution prefers the
    /// `translation-hash-table-vector` variable.
    translation_hashes: Vec<HashMap<i64, i64>>,
    next_program_id: i64,
    next_code_conversion_map_id: i64,
}

impl CclRegistry {
    fn with_defaults() -> Self {
        Self {
            programs: HashMap::new(),
            code_conversion_maps: HashMap::new(),
            translation_hashes: Vec::new(),
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

    fn program_by_id(&self, id: i64) -> Option<Value> {
        self.programs
            .values()
            .find(|(program_id, _)| *program_id == id)
            .map(|(_, program)| *program)
    }

    fn program_id(&self, name: SymId) -> Option<i64> {
        self.programs.get(&name).map(|(id, _)| *id)
    }

    fn map_by_id(&self, id: i64) -> Option<Value> {
        self.code_conversion_maps
            .values()
            .find(|(map_id, _)| *map_id == id)
            .map(|(_, map)| *map)
    }

    fn map_id(&self, name: SymId) -> Option<i64> {
        self.code_conversion_maps.get(&name).map(|(id, _)| *id)
    }

    fn add_translation_hash(&mut self, entries: HashMap<i64, i64>) -> i64 {
        let id = self.translation_hashes.len() as i64;
        self.translation_hashes.push(entries);
        id
    }
}

thread_local! {
    static CCL_OBARRAY: Cell<*const super::symbol::Obarray> = const { Cell::new(std::ptr::null()) };
}

/// Run `body` while CCL symbol resolution can see this obarray's plists and
/// the translation vectors `define-translation-hash-table` fills.
pub(crate) fn with_ccl_obarray<R>(obarray: &super::symbol::Obarray, body: impl FnOnce() -> R) -> R {
    CCL_OBARRAY.with(|cell| {
        let previous = cell.get();
        cell.set(obarray as *const super::symbol::Obarray);
        let result = body();
        cell.set(previous);
        result
    })
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

fn resolve_ccl_program_designator(value: &Value) -> Option<Value> {
    if value.is_vector() {
        return Some(*value);
    }
    let name = value.as_symbol_id()?;
    with_ccl_registry(|registry| registry.lookup_program(name))
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

fn ccl_word_shape(word: &Value) -> bool {
    if word.as_int().is_some() || word.as_symbol_id().is_some() {
        return true;
    }
    word.is_cons()
        && word.cons_car().as_symbol_id().is_some()
        && word.cons_cdr().as_symbol_id().is_some()
}

fn property_symbol(name: &str) -> Option<SymId> {
    Value::symbol(name).as_symbol_id()
}

fn symbol_property_integer(symbol: SymId, property: SymId) -> Option<i64> {
    let obarray = CCL_OBARRAY.with(|cell| cell.get());
    if obarray.is_null() {
        return None;
    }
    let plist = unsafe { &*obarray }.symbol_plist_id(symbol);
    super::plist::plist_get(plist, &Value::from_sym_id(property))
        .and_then(|value| value.as_int())
        .filter(|id| *id >= 0)
}

fn resolved_symbol_id(symbol: SymId, property: SymId) -> Option<i64> {
    if let Some(id) = symbol_property_integer(symbol, property) {
        return Some(id);
    }
    let program = property_symbol("ccl-program-idx")?;
    let map = property_symbol("code-conversion-map-id")?;
    with_ccl_registry(|registry| {
        if property == program {
            registry.program_id(symbol)
        } else if property == map {
            registry.map_id(symbol)
        } else {
            None
        }
    })
}

fn resolve_ccl_word(word: Value) -> Result<i64, Flow> {
    if let Some(number) = word.as_int() {
        return Ok(number);
    }
    let invalid = || signal("error", vec![Value::string("Invalid CCL program")]);
    if word.is_cons() {
        let symbol = word.cons_car().as_symbol_id().ok_or_else(invalid)?;
        let property = word.cons_cdr().as_symbol_id().ok_or_else(invalid)?;
        return resolved_symbol_id(symbol, property).ok_or_else(invalid);
    }
    let symbol = word.as_symbol_id().ok_or_else(invalid)?;
    for name in [
        "translation-table-id",
        "code-conversion-map-id",
        "ccl-program-idx",
    ] {
        if let Some(property) = property_symbol(name)
            && let Some(id) = resolved_symbol_id(symbol, property)
        {
            return Ok(id);
        }
    }
    Err(invalid())
}

fn compiled_ccl_words(designator: Value) -> Result<Vec<i64>, Flow> {
    let Some(program) = resolve_ccl_program_designator(&designator) else {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    };
    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    }
    program
        .as_vector_data()
        .expect("validated CCL program is a vector")
        .iter()
        .copied()
        .map(resolve_ccl_word)
        .collect()
}

pub(super) fn code_conversion_map(id: i64) -> Option<Value> {
    with_ccl_registry(|registry| registry.map_by_id(id))
}

pub(super) fn program_words(id: i64) -> Option<Vec<i64>> {
    let program = with_ccl_registry(|registry| registry.program_by_id(id))?;
    compiled_ccl_words(program).ok()
}

pub(super) fn program_words_by_symbol(symbol: SymId) -> Option<Vec<i64>> {
    let id = with_ccl_registry(|registry| registry.program_id(symbol))?;
    program_words(id)
}

pub(super) fn install_translation_hash(entries: HashMap<i64, i64>) -> i64 {
    with_ccl_registry_mut(|registry| registry.add_translation_hash(entries))
}

pub(super) fn translation_hash_lookup(id: i64, key: i64) -> Option<i64> {
    if let Some(value) = lisp_translation_hash_lookup(id, key) {
        return Some(value);
    }
    with_ccl_registry(|registry| {
        registry
            .translation_hashes
            .get(usize::try_from(id).ok()?)
            .and_then(|table| table.get(&key).copied())
    })
}

pub(super) fn translation_table(id: i64) -> Option<Value> {
    lisp_vector_slot("translation-table-vector", id).map(|slot| {
        if slot.is_cons() {
            slot.cons_cdr()
        } else {
            slot
        }
    })
}

fn lisp_vector_slot(name: &str, id: i64) -> Option<Value> {
    let obarray = CCL_OBARRAY.with(|cell| cell.get());
    if obarray.is_null() {
        return None;
    }
    let vector = unsafe { &*obarray }.symbol_value(name).copied()?;
    let data = vector.as_vector_data()?;
    let index = usize::try_from(id).ok()?;
    data.get(index).copied()
}

fn lisp_translation_hash_lookup(id: i64, key: i64) -> Option<i64> {
    let slot = lisp_vector_slot("translation-hash-table-vector", id)?;
    let table = if slot.is_cons() {
        slot.cons_cdr()
    } else {
        slot
    };
    let table = table.as_hash_table()?;
    table
        .data
        .lookup(Value::fixnum(key), table.test, false)
        .and_then(|value| value.as_int())
}

fn write_embedded_characters(
    words: &[i64],
    at: usize,
    length: usize,
    error_at: usize,
    write_character: &mut dyn FnMut(i64) -> Result<(), Flow>,
) -> Result<(), Flow> {
    let first = *words
        .get(at)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    if first & 0x1000000 != 0 {
        let characters = words
            .get(at..at + length)
            .ok_or_else(|| invalid_ccl_program_at(error_at))?;
        for word in characters {
            write_character(word & 0x00ff_ffff)?;
        }
    } else {
        let packed_words = length.saturating_add(2) / 3;
        let packed = words
            .get(at..at + packed_words)
            .ok_or_else(|| invalid_ccl_program_at(error_at))?;
        for character_index in 0..length {
            let word = packed[character_index / 3];
            let shift = (2 - (character_index % 3)) * 8;
            write_character((word >> shift) & 0xff)?;
        }
    }
    Ok(())
}

fn ccl_relative_instruction(instruction: usize, offset: i64) -> Option<usize> {
    let target = (instruction as i64).checked_add(offset)?;
    usize::try_from(target).ok()
}

/// GNU `CCL_Branch` (`src/ccl.c`). `table_head` is the first jump-table word.
/// `length` table entries are followed by one out-of-range entry. Each entry
/// is a raw relative offset from `table_head`, not a packed command.
fn ccl_branch_target(
    words: &[i64],
    table_head: usize,
    length: i64,
    selector: i64,
    error_at: usize,
) -> Result<usize, Flow> {
    let slot = if (0..length).contains(&selector) {
        selector
    } else {
        length
    };
    let slot = usize::try_from(slot).map_err(|_| invalid_ccl_program_at(error_at))?;
    let entry = table_head
        .checked_add(slot)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    let offset = *words
        .get(entry)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    ccl_relative_instruction(table_head, offset).ok_or_else(|| invalid_ccl_program_at(error_at))
}

struct CclExecution {
    output: Vec<i64>,
    registers: [i64; 8],
    instruction: usize,
}

fn ccl_reg(registers: &[i64; 8], index: usize) -> i32 {
    registers[index] as i32
}

fn next_ccl_i32(words: &[i64], instruction: &mut usize, error_at: usize) -> Result<i32, Flow> {
    let word = *words
        .get(*instruction)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    *instruction += 1;
    Ok(word as i32)
}

/// GNU `CCL_JumpCondExprConst` / `CCL_JumpCondExprReg` after the optional read.
/// A zero result in `r7` takes `jump_target`; otherwise execution continues
/// after the operator words.
fn eval_jump_cond_const(
    words: &[i64],
    registers: &mut [i64; 8],
    mut instruction: usize,
    field1: i64,
    left: i32,
    error_at: usize,
) -> Result<usize, Flow> {
    let jump_target = ccl_relative_instruction(instruction, field1)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    let operator_code = i64::from(next_ccl_i32(words, &mut instruction, error_at)?);
    let right = next_ccl_i32(words, &mut instruction, error_at)?;
    finish_jump_cond(
        registers,
        operator_code,
        left,
        right,
        instruction,
        jump_target,
        error_at,
    )
}

fn eval_jump_cond_reg(
    words: &[i64],
    registers: &mut [i64; 8],
    mut instruction: usize,
    field1: i64,
    left: i32,
    error_at: usize,
) -> Result<usize, Flow> {
    let jump_target = ccl_relative_instruction(instruction, field1)
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    let operator_code = i64::from(next_ccl_i32(words, &mut instruction, error_at)?);
    let register_number = next_ccl_i32(words, &mut instruction, error_at)?;
    if !(0..=7).contains(&register_number) {
        return Err(invalid_ccl_program_at(error_at));
    }
    let right = ccl_reg(registers, register_number as usize);
    finish_jump_cond(
        registers,
        operator_code,
        left,
        right,
        instruction,
        jump_target,
        error_at,
    )
}

fn finish_jump_cond(
    registers: &mut [i64; 8],
    operator_code: i64,
    left: i32,
    right: i32,
    fallthrough: usize,
    jump_target: usize,
    error_at: usize,
) -> Result<usize, Flow> {
    eval_set_expr(registers, 7, operator_code, left, right, error_at)?;
    if registers[7] == 0 {
        Ok(jump_target)
    } else {
        Ok(fallthrough)
    }
}

fn execute_compiled_ccl_with_state(
    designator: Value,
    input: &[i64],
    last_block: bool,
    allows_io: bool,
    mut registers: [i64; 8],
    initial_instruction: Option<usize>,
) -> Result<CclExecution, Flow> {
    const HEADER_MAIN: usize = 2;
    const MAX_STEPS_PER_WORD: usize = 4096;

    let mut words = compiled_ccl_words(designator)?;
    let mut eof_instruction = usize::try_from(words[1])
        .ok()
        .filter(|instruction| *instruction < words.len())
        .ok_or_else(|| invalid_ccl_program_at(1))?;
    let mut call_stack: Vec<(Vec<i64>, usize, usize)> = Vec::new();
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
        let command = CclCommand::from_repr((code & 0x1f) as u8)
            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;

        let mut read_character = |destination: &mut i64| -> Result<Option<bool>, Flow> {
            if !allows_io {
                return Err(invalid_ccl_program_at(this_instruction));
            }
            if let Some(value) = input.get(source) {
                *destination = *value;
                source += 1;
                Ok(Some(false))
            } else if last_block {
                *destination = -1;
                Ok(Some(true))
            } else {
                Ok(None)
            }
        };
        let mut write_character = |value: i64| -> Result<(), Flow> {
            if !allows_io {
                return Err(invalid_ccl_program_at(this_instruction));
            }
            output.push(value);
            Ok(())
        };

        match command {
            CclCommand::SetRegister => registers[register] = registers[other_register],
            CclCommand::SetShortConst => registers[register] = field1,
            CclCommand::SetConst => {
                registers[register] = *words
                    .get(instruction)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                instruction += 1;
            }
            // GNU `CCL_SetArray`: `reg[rrr] = ELEMENT[reg[RRR]]` when the
            // index is inside the table, then skip the table either way.
            CclCommand::SetArray => {
                let length = field1 >> 3;
                let index = ccl_reg(&registers, other_register);
                if index >= 0 && i64::from(index) < length {
                    let slot = instruction
                        .checked_add(usize::try_from(index).unwrap_or(usize::MAX))
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    registers[register] = i64::from(
                        *words
                            .get(slot)
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?
                            as i32,
                    );
                }
                let skip = usize::try_from(length)
                    .ok()
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                instruction = instruction
                    .checked_add(skip)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::Jump => {
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::JumpCond if registers[register] == 0 => {
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::JumpCond => {}
            CclCommand::WriteRegisterJump => {
                write_character(registers[register])?;
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            // The compiler stores a paired ReadJump word after this fused
            // instruction. GNU skips it after a successful read, but resumes
            // at that word when input is exhausted in a non-final block.
            CclCommand::WriteRegisterReadJump => {
                write_character(registers[register])?;
                instruction = instruction
                    .checked_add(1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                match read_character(&mut registers[register])? {
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
            CclCommand::WriteConstJump => {
                write_character(
                    *words
                        .get(instruction)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?,
                )?;
                instruction = ccl_relative_instruction(instruction, field1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::ReadJump => match read_character(&mut registers[register])? {
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
            // `instruction` already points at the jump table. GNU indexes that
            // table by the register, or by `field1` when the register is
            // outside `0..field1`.
            CclCommand::Branch => {
                instruction = ccl_branch_target(
                    &words,
                    instruction,
                    field1,
                    registers[register],
                    this_instruction,
                )?;
            }
            // GNU reads one character, then falls through into CCL_Branch.
            // EOF skips the table and runs the eof program. A suspended read
            // resumes on this same word.
            CclCommand::ReadBranch => match read_character(&mut registers[register])? {
                Some(true) => instruction = eof_instruction,
                Some(false) => {
                    instruction = ccl_branch_target(
                        &words,
                        instruction,
                        field1,
                        registers[register],
                        this_instruction,
                    )?;
                }
                None => {
                    return Ok(CclExecution {
                        output,
                        registers,
                        instruction: this_instruction,
                    });
                }
            },
            // Consecutive encoded operands read into one or more registers; a
            // zero field terminates the sequence.
            CclCommand::ReadRegister => {
                let mut read_field = field1;
                let mut read_register = register;
                // GNU resumes a suspended multi-register read at the operand
                // that blocked, not at the first word of the command.
                let mut resume_at = this_instruction;
                loop {
                    match read_character(&mut registers[read_register])? {
                        Some(true) => {
                            instruction = eof_instruction;
                            break;
                        }
                        Some(false) => {}
                        None => {
                            return Ok(CclExecution {
                                output,
                                registers,
                                instruction: resume_at,
                            });
                        }
                    }
                    if read_field == 0 {
                        break;
                    }
                    resume_at = instruction;
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
            CclCommand::WriteRegister => {
                let mut write_field = field1;
                let mut write_register = register;
                loop {
                    write_character(registers[write_register])?;
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
            // A zero register field embeds one character directly in FIELD1.
            // A nonzero field stores an ASCII string three octets per following
            // word, most-significant octet first (GNU `ccl-embed-string`).
            CclCommand::WriteConstString if register == 0 => write_character(field1)?,
            CclCommand::WriteConstString => {
                let length = usize::try_from(field1)
                    .ok()
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                let first = *words
                    .get(instruction)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                if first & 0x1000000 != 0 {
                    // One character per following word, low 24 bits. GNU still
                    // advances by the packed-ASCII word count.
                    let end = instruction
                        .checked_add(length)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    let characters = words
                        .get(instruction..end)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    for word in characters {
                        write_character(word & 0x00ff_ffff)?;
                    }
                    instruction = instruction
                        .checked_add(length.saturating_add(2) / 3)
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                } else {
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
                        write_character((word >> shift) & 0xff)?;
                    }
                    instruction = end;
                }
            }
            // GNU leaves IC pointing at the End instruction so a completed
            // STATUS cannot accidentally resume beyond the vector.
            CclCommand::End => {
                if let Some((caller, return_at, caller_eof)) = call_stack.pop() {
                    words = caller;
                    instruction = return_at;
                    eof_instruction = caller_eof;
                } else {
                    return Ok(CclExecution {
                        output,
                        registers,
                        instruction: this_instruction,
                    });
                }
            }
            CclCommand::WriteExprConst => {
                let left = ccl_reg(&registers, other_register);
                let right = next_ccl_i32(&words, &mut instruction, this_instruction)?;
                eval_set_expr(
                    &mut registers,
                    7,
                    field1 >> 6,
                    left,
                    right,
                    this_instruction,
                )?;
                write_character(registers[7])?;
            }
            CclCommand::WriteExprRegister => {
                let left = ccl_reg(&registers, other_register);
                let right = ccl_reg(&registers, ((field1 >> 3) & 7) as usize);
                eval_set_expr(
                    &mut registers,
                    7,
                    field1 >> 6,
                    left,
                    right,
                    this_instruction,
                )?;
                write_character(registers[7])?;
            }
            CclCommand::ExprSelfConst => {
                let operand = next_ccl_i32(&words, &mut instruction, this_instruction)?;
                eval_expr_self(
                    &mut registers,
                    register,
                    field1 >> 6,
                    operand,
                    this_instruction,
                )?;
            }
            CclCommand::ExprSelfReg => {
                let operand = ccl_reg(&registers, other_register);
                eval_expr_self(
                    &mut registers,
                    register,
                    field1 >> 6,
                    operand,
                    this_instruction,
                )?;
            }
            CclCommand::SetExprConst => {
                let left = ccl_reg(&registers, other_register);
                let right = next_ccl_i32(&words, &mut instruction, this_instruction)?;
                eval_set_expr(
                    &mut registers,
                    register,
                    field1 >> 6,
                    left,
                    right,
                    this_instruction,
                )?;
            }
            CclCommand::SetExprReg => {
                let left = ccl_reg(&registers, other_register);
                let right = ccl_reg(&registers, ((field1 >> 3) & 7) as usize);
                eval_set_expr(
                    &mut registers,
                    register,
                    field1 >> 6,
                    left,
                    right,
                    this_instruction,
                )?;
            }
            CclCommand::ReadJumpCondExprConst => match read_character(&mut registers[register])? {
                Some(true) => instruction = eof_instruction,
                Some(false) => {
                    let left = ccl_reg(&registers, register);
                    instruction = eval_jump_cond_const(
                        &words,
                        &mut registers,
                        instruction,
                        field1,
                        left,
                        this_instruction,
                    )?;
                }
                None => {
                    return Ok(CclExecution {
                        output,
                        registers,
                        instruction: this_instruction,
                    });
                }
            },
            CclCommand::JumpCondExprConst => {
                let left = ccl_reg(&registers, register);
                instruction = eval_jump_cond_const(
                    &words,
                    &mut registers,
                    instruction,
                    field1,
                    left,
                    this_instruction,
                )?;
            }
            CclCommand::ReadJumpCondExprReg => match read_character(&mut registers[register])? {
                Some(true) => instruction = eof_instruction,
                Some(false) => {
                    let left = ccl_reg(&registers, register);
                    instruction = eval_jump_cond_reg(
                        &words,
                        &mut registers,
                        instruction,
                        field1,
                        left,
                        this_instruction,
                    )?;
                }
                None => {
                    return Ok(CclExecution {
                        output,
                        registers,
                        instruction: this_instruction,
                    });
                }
            },
            CclCommand::JumpCondExprReg => {
                let left = ccl_reg(&registers, register);
                instruction = eval_jump_cond_reg(
                    &words,
                    &mut registers,
                    instruction,
                    field1,
                    left,
                    this_instruction,
                )?;
            }
            // GNU `CCL_WriteArray`: write `ELEMENT[reg]` when the index is
            // inside the table, then skip the table either way.
            CclCommand::WriteArray => {
                let length = field1;
                let index = ccl_reg(&registers, register);
                if index >= 0 && i64::from(index) < length {
                    let slot = instruction
                        .checked_add(usize::try_from(index).unwrap_or(usize::MAX))
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    write_character(i64::from(
                        *words
                            .get(slot)
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?
                            as i32,
                    ))?;
                }
                let skip = usize::try_from(length)
                    .ok()
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                instruction = instruction
                    .checked_add(skip)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::WriteConstReadJump => {
                let constant = i64::from(next_ccl_i32(&words, &mut instruction, this_instruction)?);
                write_character(constant)?;
                let read_at = instruction;
                match read_character(&mut registers[register])? {
                    Some(true) => instruction = eof_instruction,
                    Some(false) => {
                        instruction = ccl_relative_instruction(read_at, field1 - 1)
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    }
                    None => {
                        return Ok(CclExecution {
                            output,
                            registers,
                            instruction: read_at,
                        });
                    }
                }
            }
            CclCommand::WriteStringJump => {
                let length =
                    usize::try_from(next_ccl_i32(&words, &mut instruction, this_instruction)?)
                        .map_err(|_| invalid_ccl_program_at(this_instruction))?;
                let string_at = instruction;
                write_embedded_characters(
                    &words,
                    string_at,
                    length,
                    this_instruction,
                    &mut write_character,
                )?;
                instruction = ccl_relative_instruction(string_at, field1 - 1)
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
            }
            CclCommand::WriteArrayReadJump => {
                // `instruction` still names the length word. GNU writes
                // `ELEMENT[reg]` from the following words, skips the length,
                // the array, and the paired read-jump, then reads.
                let length_at = instruction;
                let length = i64::from(next_ccl_i32(&words, &mut instruction, this_instruction)?);
                let index = ccl_reg(&registers, register);
                if index >= 0 && i64::from(index) < length {
                    let slot = usize::try_from(index)
                        .ok()
                        .and_then(|index| length_at.checked_add(1)?.checked_add(index))
                        .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    write_character(i64::from(
                        *words
                            .get(slot)
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?
                            as i32,
                    ))?;
                }
                let after = length_at
                    .checked_add(usize::try_from(length + 2).unwrap_or(usize::MAX))
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                match read_character(&mut registers[register])? {
                    Some(true) => instruction = eof_instruction,
                    Some(false) => {
                        instruction = ccl_relative_instruction(after, field1 - (length + 2))
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                    }
                    None => {
                        return Ok(CclExecution {
                            output,
                            registers,
                            instruction: after.saturating_sub(1),
                        });
                    }
                }
            }
            CclCommand::Call => {
                if call_stack.len() >= 256 {
                    return Err(invalid_ccl_program_at(this_instruction));
                }
                let program_id = if register != 0 {
                    i64::from(next_ccl_i32(&words, &mut instruction, this_instruction)?)
                } else {
                    field1
                };
                let Some(callee) = program_words(program_id) else {
                    return Err(invalid_ccl_program_at(this_instruction));
                };
                let callee_eof = usize::try_from(callee.get(1).copied().unwrap_or(-1))
                    .ok()
                    .filter(|eof| *eof < callee.len())
                    .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                let caller = std::mem::replace(&mut words, callee);
                call_stack.push((caller, instruction, eof_instruction));
                instruction = 2;
                eof_instruction = callee_eof;
            }
            CclCommand::Extension => {
                match execute_extension(
                    &words,
                    &mut instruction,
                    &mut registers,
                    field1,
                    register,
                    other_register,
                    this_instruction,
                    &mut read_character,
                    &mut write_character,
                )? {
                    ExtensionStep::Continue => {}
                    ExtensionStep::Eof => instruction = eof_instruction,
                    ExtensionStep::Suspend => {
                        return Ok(CclExecution {
                            output,
                            registers,
                            instruction: this_instruction,
                        });
                    }
                    ExtensionStep::Call {
                        words: callee,
                        resume_at,
                    } => {
                        if call_stack.len() >= 256 {
                            return Err(invalid_ccl_program_at(this_instruction));
                        }
                        let callee_eof = usize::try_from(callee.get(1).copied().unwrap_or(-1))
                            .ok()
                            .filter(|eof| *eof < callee.len())
                            .ok_or_else(|| invalid_ccl_program_at(this_instruction))?;
                        let caller = std::mem::replace(&mut words, callee);
                        call_stack.push((caller, resume_at, eof_instruction));
                        instruction = 2;
                        eof_instruction = callee_eof;
                    }
                }
            }
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
    execute_compiled_ccl_with_state(designator, input, last_block, true, [0; 8], None)
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
        .is_some_and(|program| is_valid_ccl_program(&program));
    Ok(Value::bool_val(is_program))
}

/// (ccl-execute CCL-PROGRAM REGISTERS) -> nil
///
/// Runs a program that does not read or write. Register results are written
/// back into the 8-element vector.
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

    let Some(program) = resolve_ccl_program_designator(&args[0]) else {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    };
    if !is_valid_ccl_program(&program) {
        return Err(signal("error", vec![Value::string("Invalid CCL program")]));
    }

    let status = args[1]
        .as_vector_data()
        .expect("validated REGISTERS vector");
    let mut registers = [0i64; 8];
    for (register, value) in registers.iter_mut().zip(status.iter()) {
        if let Some(integer) = value.as_int()
            && (i64::from(i32::MIN)..=i64::from(i32::MAX)).contains(&integer)
        {
            *register = integer;
        }
    }
    let execution = execute_compiled_ccl_with_state(args[0], &[], true, false, registers, None)?;
    for (index, register) in execution.registers.into_iter().enumerate() {
        let updated = args[1].set_vector_slot(index, Value::fixnum(register));
        debug_assert!(updated, "validated REGISTERS vector remains mutable");
    }
    Ok(Value::NIL)
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

    let Some(program) = resolve_ccl_program_designator(&args[0]) else {
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
        true,
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
