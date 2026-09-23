//! Extended CCL commands (`CCL_Extension`, opcode `0x1F`).
//!
//! Subcommand numbers are GNU's `CCL_ReadMultibyteChar2` through
//! `CCL_LookupCharConstTbl` in `src/ccl.c`.

use super::super::charset::{
    char_charset_name, charset_decode_char, charset_encode_char, charset_id_of, charset_sym_by_id,
};
use super::super::chartable::char_table_ref_and_range;
use super::{
    Flow, TranslationHashLookup, ccl_reg, code_conversion_map, invalid_ccl_program_at,
    next_ccl_i32, program_words_by_symbol, translation_hash_lookup, translation_table,
};

const UNICODE_CHARSET_ID: i32 = 2;

fn is_emacs_character(value: i64) -> bool {
    u32::try_from(value)
        .ok()
        .is_some_and(|code| code <= crate::emacs_core::emacs_char::MAX_CHAR)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, strum::FromRepr)]
#[repr(u8)]
enum ExtendedCommand {
    ReadMultibyte = 0x00,
    WriteMultibyte = 0x01,
    Translate = 0x02,
    TranslateConst = 0x03,
    IterateMultipleMap = 0x10,
    MapMultiple = 0x11,
    MapSingle = 0x12,
    LookupInteger = 0x13,
    LookupCharacter = 0x14,
}

/// GNU's `mapping_stack` and `stack_idx_of_map_multiple`.
///
/// A map entry that calls another CCL program returns to the same
/// `map-multiple` instruction. The stack stays put across that call so
/// the resume can tell a returned -1, -2, or -3 from a real mapping.
#[derive(Clone, Debug, Default)]
pub(super) struct MapMultipleState {
    stack: Vec<(i32, i32)>,
    call_mark: i32,
}

pub(super) enum ExtensionStep {
    Continue,
    Eof,
    Suspend,
    /// Run `words` and then resume at `resume_at` in the caller.
    Call {
        words: Vec<i64>,
        resume_at: usize,
    },
}

pub(super) fn execute_extension(
    words: &[i64],
    instruction: &mut usize,
    registers: &mut [i64; 8],
    field1: i64,
    value_register: usize,
    status_register: usize,
    error_at: usize,
    call_depth: i32,
    map_state: &mut MapMultipleState,
    read_character: &mut dyn FnMut(&mut i64) -> Result<Option<bool>, Flow>,
    write_character: &mut dyn FnMut(i64) -> Result<(), Flow>,
) -> Result<ExtensionStep, Flow> {
    let Some(command) = u8::try_from(field1 >> 6)
        .ok()
        .and_then(ExtendedCommand::from_repr)
    else {
        return Err(invalid_ccl_program_at(error_at));
    };
    let third_register = ((field1 >> 3) & 7) as usize;
    match command {
        ExtendedCommand::ReadMultibyte => {
            let mut code = 0;
            match read_character(&mut code)? {
                Some(true) => return Ok(ExtensionStep::Eof),
                None => return Ok(ExtensionStep::Suspend),
                Some(false) => {}
            }
            encode_character(registers, status_register, value_register, code);
            Ok(ExtensionStep::Continue)
        }
        ExtendedCommand::WriteMultibyte => {
            let charset_id = ccl_reg(registers, status_register);
            let code = ccl_reg(registers, value_register);
            let character = if charset_id == 0 {
                i64::from(code)
            } else {
                charset_sym_by_id(i64::from(charset_id))
                    .and_then(|name| charset_decode_char(name, i64::from(code)))
                    .unwrap_or(i64::from(code))
            };
            write_character(character)?;
            Ok(ExtensionStep::Continue)
        }
        ExtendedCommand::Translate => translate_character(
            registers,
            status_register,
            value_register,
            i64::from(ccl_reg(registers, third_register)),
            error_at,
        ),
        ExtendedCommand::TranslateConst => {
            let table_id = i64::from(next_ccl_i32(words, instruction, error_at)?);
            translate_character(
                registers,
                status_register,
                value_register,
                table_id,
                error_at,
            )
        }
        ExtendedCommand::LookupInteger => {
            let table_id = i64::from(next_ccl_i32(words, instruction, error_at)?);
            let key = ccl_reg(registers, status_register);
            // GNU requires the hash value to be a character. The instruction
            // counter has already moved past the table id, which is the index
            // reported in the error. A symbol is invalid, not a miss.
            match translation_hash_lookup(table_id, i64::from(key)) {
                TranslationHashLookup::Integer(value) if is_emacs_character(i64::from(value)) => {
                    registers[status_register] = i64::from(UNICODE_CHARSET_ID);
                    registers[value_register] = i64::from(value);
                    registers[7] = 1;
                }
                TranslationHashLookup::Miss => registers[7] = 0,
                TranslationHashLookup::Integer(_) | TranslationHashLookup::Invalid => {
                    return Err(invalid_ccl_program_at(instruction.saturating_sub(1)));
                }
            }
            Ok(ExtensionStep::Continue)
        }
        ExtendedCommand::LookupCharacter => {
            let table_id = i64::from(next_ccl_i32(words, instruction, error_at)?);
            let character = decode_register_character(registers, status_register, value_register);
            // GNU stores any C int, including negatives. A non-integer or an
            // integer outside that range is an invalid command at the same
            // index lookup-integer uses.
            match translation_hash_lookup(table_id, character) {
                TranslationHashLookup::Miss => registers[7] = 0,
                TranslationHashLookup::Integer(value) => {
                    registers[status_register] = i64::from(value);
                    registers[7] = 1;
                }
                TranslationHashLookup::Invalid => {
                    return Err(invalid_ccl_program_at(instruction.saturating_sub(1)));
                }
            }
            Ok(ExtensionStep::Continue)
        }
        ExtendedCommand::MapSingle => {
            let map_id = i64::from(next_ccl_i32(words, instruction, error_at)?);
            map_single(
                registers,
                status_register,
                value_register,
                map_id,
                *instruction,
            )
        }
        ExtendedCommand::IterateMultipleMap => iterate_multiple_map(
            words,
            instruction,
            registers,
            status_register,
            value_register,
            error_at,
        ),
        ExtendedCommand::MapMultiple => map_multiple(
            words,
            instruction,
            registers,
            status_register,
            value_register,
            error_at,
            call_depth,
            map_state,
        ),
    }
}

fn encode_character(
    registers: &mut [i64; 8],
    charset_register: usize,
    code_register: usize,
    character: i64,
) {
    let name = char_charset_name(character);
    let Some(symbol) = super::Value::symbol(name).as_symbol_id() else {
        return;
    };
    let charset_id = charset_id_of(symbol).unwrap_or(0);
    let code = charset_encode_char(symbol, character).unwrap_or(character);
    registers[charset_register] = charset_id;
    registers[code_register] = code;
}

fn decode_register_character(
    registers: &[i64; 8],
    charset_register: usize,
    code_register: usize,
) -> i64 {
    let charset_id = ccl_reg(registers, charset_register);
    let code = i64::from(ccl_reg(registers, code_register));
    if charset_id == 0 {
        return code;
    }
    charset_sym_by_id(i64::from(charset_id))
        .and_then(|name| charset_decode_char(name, code))
        .unwrap_or(code)
}

fn translate_character(
    registers: &mut [i64; 8],
    charset_register: usize,
    code_register: usize,
    table_id: i64,
    error_at: usize,
) -> Result<ExtensionStep, Flow> {
    let Some(table) = translation_table(table_id) else {
        return Err(invalid_ccl_program_at(error_at));
    };
    let character = decode_register_character(registers, charset_register, code_register);
    let (mapped, _, _) = char_table_ref_and_range(&table, character)
        .map_err(|_| invalid_ccl_program_at(error_at))?;
    let mapped = mapped.as_int().unwrap_or(character);
    encode_character(registers, charset_register, code_register, mapped);
    Ok(ExtensionStep::Continue)
}

const MAX_TRANSLATION_LIST_DEPTH: usize = 1024;

fn map_single(
    registers: &mut [i64; 8],
    status_register: usize,
    value_register: usize,
    map_id: i64,
    resume_at: usize,
) -> Result<ExtensionStep, Flow> {
    let Some(map) = code_conversion_map(map_id) else {
        registers[status_register] = -1;
        return Ok(ExtensionStep::Continue);
    };
    let slots = vector_slots(&map);
    let value = ccl_reg(registers, value_register);
    let Some(start) = slots.first().and_then(ValueSlot::as_int) else {
        registers[status_register] = -1;
        return Ok(ExtensionStep::Continue);
    };
    let index = i64::from(value) - start + 1;
    if index < 1 || index >= slots.len() as i64 {
        registers[status_register] = -1;
        return Ok(ExtensionStep::Continue);
    }
    apply_map_content(
        &slots[index as usize],
        registers,
        status_register,
        value_register,
        resume_at,
    )
}

fn iterate_multiple_map(
    words: &[i64],
    instruction: &mut usize,
    registers: &mut [i64; 8],
    status_register: usize,
    value_register: usize,
    error_at: usize,
) -> Result<ExtensionStep, Flow> {
    let count = i64::from(next_ccl_i32(words, instruction, error_at)?);
    let start = *instruction;
    let end = start
        .checked_add(usize::try_from(count).unwrap_or(usize::MAX))
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    if end > words.len() {
        return Err(invalid_ccl_program_at(error_at));
    }
    let from = ccl_reg(registers, status_register);
    if i64::from(from) >= count || from < 0 {
        registers[status_register] = -1;
        *instruction = end;
        return Ok(ExtensionStep::Continue);
    }
    let value = i64::from(ccl_reg(registers, value_register));
    for offset in i64::from(from)..count {
        let map_id = words[start + offset as usize];
        match lookup_map_slot(map_id, value) {
            MapHit::Number(mapped) => {
                registers[status_register] = offset;
                registers[value_register] = mapped;
                *instruction = end;
                return Ok(ExtensionStep::Continue);
            }
            // `t` and `lambda` record the map index and leave the value alone.
            MapHit::Identity | MapHit::Lambda => {
                registers[status_register] = offset;
                *instruction = end;
                return Ok(ExtensionStep::Continue);
            }
            // A symbol calls that CCL program and resumes after this map list,
            // not back at the iterate instruction.
            MapHit::Call(symbol) => {
                if let Some(program) = program_words_by_symbol(symbol) {
                    return Ok(ExtensionStep::Call {
                        words: program,
                        resume_at: end,
                    });
                }
            }
            MapHit::Miss => {}
        }
    }
    registers[status_register] = -1;
    *instruction = end;
    Ok(ExtensionStep::Continue)
}

fn map_multiple(
    words: &[i64],
    instruction: &mut usize,
    registers: &mut [i64; 8],
    status_register: usize,
    value_register: usize,
    error_at: usize,
    call_depth: i32,
    state: &mut MapMultipleState,
) -> Result<ExtensionStep, Flow> {
    if state.call_mark > 0 {
        if state.call_mark <= call_depth {
            state.call_mark = 0;
            state.stack.clear();
            return Err(invalid_ccl_program_at(error_at));
        }
    } else {
        state.stack.clear();
    }
    state.call_mark = 0;

    let count = i64::from(next_ccl_i32(words, instruction, error_at)?);
    let list_at = *instruction;
    let end = list_at
        .checked_add(usize::try_from(count).unwrap_or(usize::MAX))
        .ok_or_else(|| invalid_ccl_program_at(error_at))?;
    if end > words.len() {
        return Err(invalid_ccl_program_at(error_at));
    }
    let mut op = i64::from(ccl_reg(registers, value_register));
    let mut index = i64::from(ccl_reg(registers, status_register));
    let mut rest = count;
    let mut cursor = list_at as i64;
    if count > index && index >= 0 {
        cursor += index;
        rest -= index;
    } else {
        registers[status_register] = -1;
        state.stack.clear();
        *instruction = end;
        return Ok(ExtensionStep::Continue);
    }

    if state.stack.len() <= 1 {
        state.stack.clear();
        state.stack.push((0, op as i32));
        registers[status_register] = -1;
    } else {
        let Some((_rest, orig_op)) = state.stack.pop() else {
            return Err(invalid_ccl_program_at(error_at));
        };
        let Some((rest_b, saved_value)) = state.stack.pop() else {
            return Err(invalid_ccl_program_at(error_at));
        };
        rest = i64::from(rest_b);
        registers[value_register] = i64::from(saved_value);
        match op {
            -1 => {
                op = i64::from(orig_op);
                index += 1;
                cursor += 1;
                rest -= 1;
            }
            -2 => {
                op = i64::from(saved_value);
                index += 1;
                cursor += 1;
                rest -= 1;
            }
            -3 => {
                op = i64::from(orig_op);
                index += rest;
                cursor += rest;
                rest = 0;
            }
            _ => {
                index += rest;
                cursor += rest;
                if let Some((rest_c, saved)) = state.stack.pop() {
                    rest = i64::from(rest_c);
                    registers[value_register] = i64::from(saved);
                }
            }
        }
    }

    while rest > 0 {
        let point = *words
            .get(usize::try_from(cursor).unwrap_or(usize::MAX))
            .ok_or_else(|| invalid_ccl_program_at(error_at))?;
        if point < 0 {
            let span = -point + 1;
            if state.stack.len() >= 30 {
                return Err(invalid_ccl_program_at(error_at));
            }
            state.stack.push((
                rest as i32 - span as i32,
                ccl_reg(registers, value_register),
            ));
            rest = span;
            registers[value_register] = op;
        } else {
            match lookup_map_slot(point, op) {
                MapHit::Miss => {}
                MapHit::Number(mapped) => {
                    registers[status_register] = index;
                    op = mapped;
                    index += rest - 1;
                    cursor += rest - 1;
                    if let Some((popped_rest, saved)) = state.stack.pop() {
                        rest = i64::from(popped_rest);
                        registers[value_register] = i64::from(saved);
                    }
                    rest += 1;
                }
                MapHit::Identity => {
                    registers[status_register] = index;
                    op = i64::from(ccl_reg(registers, value_register));
                }
                MapHit::Lambda => {
                    index += rest;
                    cursor += rest;
                    break;
                }
                MapHit::Call(symbol) => {
                    if state.stack.len() >= 30 {
                        return Err(invalid_ccl_program_at(error_at));
                    }
                    let saved_value = ccl_reg(registers, value_register);
                    state.stack.push((rest as i32, saved_value));
                    state.stack.push((rest as i32, op as i32));
                    state.call_mark = call_depth + 1;
                    registers[status_register] = index;
                    if let Some(program) = program_words_by_symbol(symbol) {
                        return Ok(ExtensionStep::Call {
                            words: program,
                            resume_at: error_at,
                        });
                    }
                }
            }
        }
        index += 1;
        cursor += 1;
        rest -= 1;
    }
    while state.stack.len() > 1 {
        let Some((popped_rest, saved)) = state.stack.pop() else {
            break;
        };
        rest = i64::from(popped_rest);
        registers[value_register] = i64::from(saved);
        index += rest;
        cursor += rest;
        if state.stack.len() <= 1 {
            break;
        }
        if let Some((popped_rest, saved)) = state.stack.pop() {
            rest = i64::from(popped_rest);
            registers[value_register] = i64::from(saved);
        }
    }
    registers[value_register] = op;
    *instruction = end;
    Ok(ExtensionStep::Continue)
}

enum MapHit {
    Number(i64),
    Identity,
    Miss,
    Lambda,
    Call(crate::emacs_core::SymId),
}

fn lookup_map_slot(map_id: i64, value: i64) -> MapHit {
    let Some(map) = code_conversion_map(map_id) else {
        return MapHit::Miss;
    };
    let slots = vector_slots(&map);
    if slots.len() <= 1 {
        return MapHit::Miss;
    }
    // `[t ELEMENT START END]` covers `START <= value < END`. `map-single`
    // never uses this shape; `map-multiple` and `iterate-multiple-map` do.
    if matches!(slots[0], ValueSlot::True) && slots.len() == 4 {
        let (Some(start), Some(end)) = (slots[2].as_int(), slots[3].as_int()) else {
            return MapHit::Miss;
        };
        if start <= value && value < end {
            return hit_of(&slots[1]);
        }
        return MapHit::Miss;
    }
    let Some(start) = slots[0].as_int() else {
        return MapHit::Miss;
    };
    let index = value - start + 1;
    if index < 1 || index >= slots.len() as i64 {
        return MapHit::Miss;
    }
    hit_of(&slots[index as usize])
}

fn hit_of(content: &ValueSlot) -> MapHit {
    match content {
        ValueSlot::Int(number) => MapHit::Number(*number),
        ValueSlot::Nil => MapHit::Miss,
        ValueSlot::True => MapHit::Identity,
        ValueSlot::Lambda => MapHit::Lambda,
        ValueSlot::Symbol(symbol) => MapHit::Call(*symbol),
        ValueSlot::Other => MapHit::Miss,
    }
}

fn apply_map_content(
    content: &ValueSlot,
    registers: &mut [i64; 8],
    status_register: usize,
    value_register: usize,
    resume_at: usize,
) -> Result<ExtensionStep, Flow> {
    match content {
        ValueSlot::Nil => {
            registers[status_register] = -1;
            Ok(ExtensionStep::Continue)
        }
        ValueSlot::Int(number) => {
            registers[status_register] = 0;
            registers[value_register] = *number;
            Ok(ExtensionStep::Continue)
        }
        ValueSlot::True => {
            registers[status_register] = 0;
            Ok(ExtensionStep::Continue)
        }
        ValueSlot::Symbol(symbol) => match program_words_by_symbol(*symbol) {
            Some(words) => Ok(ExtensionStep::Call { words, resume_at }),
            None => {
                registers[status_register] = -1;
                Ok(ExtensionStep::Continue)
            }
        },
        ValueSlot::Lambda | ValueSlot::Other => {
            registers[status_register] = -1;
            Ok(ExtensionStep::Continue)
        }
    }
}

enum ValueSlot {
    Int(i64),
    Nil,
    True,
    Lambda,
    Symbol(crate::emacs_core::SymId),
    Other,
}

impl ValueSlot {
    fn as_int(&self) -> Option<i64> {
        match self {
            Self::Int(number) => Some(*number),
            _ => None,
        }
    }
}

fn vector_slots(value: &super::Value) -> Vec<ValueSlot> {
    let Some(data) = value.as_vector_data() else {
        return Vec::new();
    };
    data.iter().copied().map(classify_slot).collect()
}

fn classify_slot(value: super::Value) -> ValueSlot {
    if let Some(number) = value.as_int() {
        return ValueSlot::Int(number);
    }
    // `(ATTRIB . VALUE)` stores VALUE. GNU ignores the attribute once both
    // halves are integers.
    if value.is_cons()
        && value.cons_car().as_int().is_some()
        && let Some(mapped) = value.cons_cdr().as_int()
    {
        return ValueSlot::Int(mapped);
    }
    if value.is_nil() {
        return ValueSlot::Nil;
    }
    if value.is_t() {
        return ValueSlot::True;
    }
    if value.as_symbol_id() == super::Value::symbol("lambda").as_symbol_id() {
        return ValueSlot::Lambda;
    }
    if let Some(symbol) = value.as_symbol_id() {
        return ValueSlot::Symbol(symbol);
    }
    ValueSlot::Other
}
