//! Extended CCL commands (`CCL_Extension`, opcode `0x1F`).
//!
//! Subcommand numbers are GNU's `CCL_ReadMultibyteChar2` through
//! `CCL_LookupCharConstTbl` in `src/ccl.c`.

use super::super::charset::{
    char_charset_name, charset_decode_char, charset_encode_char, charset_id_of, charset_sym_by_id,
};
use super::super::chartable::char_table_ref_and_range;
use super::{
    Flow, ccl_reg, code_conversion_map, invalid_ccl_program_at, next_ccl_i32,
    program_words_by_symbol, translation_hash_lookup, translation_table,
};

const UNICODE_CHARSET_ID: i32 = 2;

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
            match translation_hash_lookup(table_id, i64::from(key)) {
                Some(value) => {
                    registers[status_register] = i64::from(UNICODE_CHARSET_ID);
                    registers[value_register] = value;
                    registers[7] = 1;
                }
                None => registers[7] = 0,
            }
            Ok(ExtensionStep::Continue)
        }
        ExtendedCommand::LookupCharacter => {
            let table_id = i64::from(next_ccl_i32(words, instruction, error_at)?);
            let character = decode_register_character(registers, status_register, value_register);
            match translation_hash_lookup(table_id, character) {
                Some(value) => {
                    registers[status_register] = value;
                    registers[7] = 1;
                }
                None => registers[7] = 0,
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
    let value = ccl_reg(registers, value_register);
    for offset in i64::from(from)..count {
        let map_id = words[start + offset as usize];
        if let Some(mapped) = lookup_map_integer(map_id, i64::from(value)) {
            registers[status_register] = offset;
            registers[value_register] = mapped;
            *instruction = end;
            return Ok(ExtensionStep::Continue);
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
    let mut value = i64::from(ccl_reg(registers, value_register));
    let mut index = i64::from(from);
    while index < count {
        let word = words[start + index as usize];
        if word < 0 {
            index += 1;
            continue;
        }
        match lookup_map_slot(word, value) {
            MapHit::Number(mapped) => {
                value = mapped;
                registers[status_register] = index;
            }
            MapHit::Identity | MapHit::Miss => {}
            MapHit::Lambda => break,
            MapHit::Call(symbol) => {
                if let Some(program) = program_words_by_symbol(symbol) {
                    registers[value_register] = value;
                    *instruction = error_at;
                    return Ok(ExtensionStep::Call {
                        words: program,
                        resume_at: error_at,
                    });
                }
            }
        }
        index += 1;
    }
    registers[value_register] = value;
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

fn lookup_map_integer(map_id: i64, value: i64) -> Option<i64> {
    match lookup_map_slot(map_id, value) {
        MapHit::Number(mapped) => Some(mapped),
        _ => None,
    }
}

fn lookup_map_slot(map_id: i64, value: i64) -> MapHit {
    let Some(map) = code_conversion_map(map_id) else {
        return MapHit::Miss;
    };
    let slots = vector_slots(&map);
    let Some(start) = slots.first().and_then(ValueSlot::as_int) else {
        return MapHit::Miss;
    };
    let index = value - start + 1;
    if index < 1 || index >= slots.len() as i64 {
        return MapHit::Miss;
    }
    match &slots[index as usize] {
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
