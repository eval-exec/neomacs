//! Internal pdump object/value codec.
//!
//! The old monolithic `HeapObjects` image section has been removed.  This file
//! remains as the shared codec for compact metadata sections that need to encode
//! individual `DumpValue`s or Category B/C object descriptors.

use super::DumpError;
use super::types::{
    DumpBufferId, DumpByteCodeFunction, DumpByteCodeInstructions, DumpByteCodeKeyPart,
    DumpByteData, DumpHashKey, DumpHashTableTest, DumpHashTableWeakness, DumpHeapObject,
    DumpHeapRef, DumpLambdaParams, DumpLispHashTable, DumpLispString, DumpMarker, DumpNameId,
    DumpOverlay, DumpStringTextPropertyRun, DumpSymId, DumpValue,
};

use crate::emacs_core::bytecode::opcode::Op;

const HEAP_CONS: u8 = 0;
const HEAP_VECTOR: u8 = 1;
const HEAP_HASH_TABLE: u8 = 2;
const HEAP_STRING: u8 = 3;
const HEAP_FLOAT: u8 = 4;
const HEAP_LAMBDA: u8 = 5;
const HEAP_MACRO: u8 = 6;
const HEAP_BYTE_CODE: u8 = 7;
const HEAP_RECORD: u8 = 8;
const HEAP_MARKER: u8 = 9;
const HEAP_OVERLAY: u8 = 10;
const HEAP_BUFFER: u8 = 11;
const HEAP_WINDOW: u8 = 12;
const HEAP_FRAME: u8 = 13;
const HEAP_TIMER: u8 = 14;
const HEAP_SUBR: u8 = 15;
const HEAP_FREE: u8 = 16;
const HEAP_CHAR_TABLE: u8 = 17;
const HEAP_SUB_CHAR_TABLE: u8 = 18;
const HEAP_OBARRAY: u8 = 19;

const BYTECODE_DECODED: u8 = 0;
const BYTECODE_GNU: u8 = 1;

pub(crate) fn write_heap_object(
    out: &mut Vec<u8>,
    object: &DumpHeapObject,
) -> Result<(), DumpError> {
    match object {
        DumpHeapObject::Cons { car, cdr } => {
            write_u8(out, HEAP_CONS);
            write_value(out, car)?;
            write_value(out, cdr)?;
        }
        DumpHeapObject::Vector(values) => {
            write_u8(out, HEAP_VECTOR);
            write_values(out, values)?;
        }
        DumpHeapObject::CharTable {
            defalt,
            parent,
            purpose,
            ascii,
            contents,
            extras,
        } => {
            write_u8(out, HEAP_CHAR_TABLE);
            write_value(out, defalt)?;
            write_value(out, parent)?;
            write_value(out, purpose)?;
            write_value(out, ascii)?;
            write_values(out, contents)?;
            write_values(out, extras)?;
        }
        DumpHeapObject::SubCharTable {
            depth,
            min_char,
            contents,
        } => {
            write_u8(out, HEAP_SUB_CHAR_TABLE);
            write_i64(out, *depth);
            write_i64(out, *min_char);
            write_values(out, contents)?;
        }
        DumpHeapObject::HashTable(table) => {
            write_u8(out, HEAP_HASH_TABLE);
            write_hash_table(out, table)?;
        }
        DumpHeapObject::Obarray { buckets, count } => {
            write_u8(out, HEAP_OBARRAY);
            write_values(out, buckets)?;
            write_u32(out, *count);
        }
        DumpHeapObject::Str {
            data,
            size,
            size_byte,
            text_props,
        } => {
            write_u8(out, HEAP_STRING);
            write_byte_data(out, data)?;
            write_usize(out, *size)?;
            write_i64(out, *size_byte);
            write_text_property_runs(out, text_props)?;
        }
        DumpHeapObject::Float(value) => {
            write_u8(out, HEAP_FLOAT);
            write_f64(out, *value);
        }
        DumpHeapObject::Lambda(values) => {
            write_u8(out, HEAP_LAMBDA);
            write_values(out, values)?;
        }
        DumpHeapObject::Macro(values) => {
            write_u8(out, HEAP_MACRO);
            write_values(out, values)?;
        }
        DumpHeapObject::ByteCode(function) => {
            write_u8(out, HEAP_BYTE_CODE);
            write_byte_code(out, function)?;
        }
        DumpHeapObject::Record(values) => {
            write_u8(out, HEAP_RECORD);
            write_values(out, values)?;
        }
        DumpHeapObject::Marker(marker) => {
            write_u8(out, HEAP_MARKER);
            write_marker(out, marker)?;
        }
        DumpHeapObject::Overlay(overlay) => {
            write_u8(out, HEAP_OVERLAY);
            write_overlay(out, overlay)?;
        }
        DumpHeapObject::Buffer(id) => {
            write_u8(out, HEAP_BUFFER);
            write_u64(out, id.0);
        }
        DumpHeapObject::Window(id) => {
            write_u8(out, HEAP_WINDOW);
            write_u64(out, *id);
        }
        DumpHeapObject::Frame(id) => {
            write_u8(out, HEAP_FRAME);
            write_u64(out, *id);
        }
        DumpHeapObject::Timer(id) => {
            write_u8(out, HEAP_TIMER);
            write_u64(out, *id);
        }
        DumpHeapObject::Subr {
            name,
            min_args,
            max_args,
        } => {
            write_u8(out, HEAP_SUBR);
            write_u32(out, name.0);
            write_u16(out, *min_args);
            write_opt_u16(out, *max_args);
        }
        DumpHeapObject::Free => write_u8(out, HEAP_FREE),
    }
    Ok(())
}

const BYTE_OWNED: u8 = 0;
const BYTE_MAPPED: u8 = 1;
const BYTE_STATIC_RODATA: u8 = 2;

fn write_byte_data(out: &mut Vec<u8>, data: &DumpByteData) -> Result<(), DumpError> {
    match data {
        DumpByteData::Owned(bytes) => {
            write_u8(out, BYTE_OWNED);
            write_bytes(out, bytes)?;
        }
        DumpByteData::Mapped(span) => {
            write_u8(out, BYTE_MAPPED);
            write_u64(out, span.offset);
            write_u64(out, span.len);
        }
        DumpByteData::StaticRoData { key, len } => {
            write_u8(out, BYTE_STATIC_RODATA);
            write_u64(out, *key);
            write_u64(out, *len);
        }
    }
    Ok(())
}

const VALUE_NIL: u8 = 0;
const VALUE_TRUE: u8 = 1;
const VALUE_INT: u8 = 2;
const VALUE_FLOAT: u8 = 3;
const VALUE_SYMBOL: u8 = 4;
const VALUE_STR: u8 = 5;
const VALUE_CONS: u8 = 6;
const VALUE_VECTOR: u8 = 7;
const VALUE_RECORD: u8 = 8;
const VALUE_HASH_TABLE: u8 = 9;
const VALUE_LAMBDA: u8 = 10;
const VALUE_MACRO: u8 = 11;
const VALUE_SUBR: u8 = 12;
const VALUE_BYTE_CODE: u8 = 13;
const VALUE_MARKER: u8 = 14;
const VALUE_OVERLAY: u8 = 15;
const VALUE_BUFFER: u8 = 16;
const VALUE_WINDOW: u8 = 17;
const VALUE_FRAME: u8 = 18;
const VALUE_TIMER: u8 = 19;
const VALUE_BIGNUM: u8 = 20;
const VALUE_UNBOUND: u8 = 21;
const VALUE_CHAR_TABLE: u8 = 22;
const VALUE_SUB_CHAR_TABLE: u8 = 23;
const VALUE_OBARRAY: u8 = 24;

pub(crate) fn write_value(out: &mut Vec<u8>, value: &DumpValue) -> Result<(), DumpError> {
    match value {
        DumpValue::Nil => write_u8(out, VALUE_NIL),
        DumpValue::True => write_u8(out, VALUE_TRUE),
        DumpValue::Int(n) => {
            write_u8(out, VALUE_INT);
            write_i64(out, *n);
        }
        DumpValue::Float(id) => write_heap_ref_value(out, VALUE_FLOAT, id),
        DumpValue::Symbol(id) => {
            write_u8(out, VALUE_SYMBOL);
            write_u32(out, id.0);
        }
        DumpValue::Str(id) => write_heap_ref_value(out, VALUE_STR, id),
        DumpValue::Cons(id) => write_heap_ref_value(out, VALUE_CONS, id),
        DumpValue::Vector(id) => write_heap_ref_value(out, VALUE_VECTOR, id),
        DumpValue::CharTable(id) => write_heap_ref_value(out, VALUE_CHAR_TABLE, id),
        DumpValue::SubCharTable(id) => write_heap_ref_value(out, VALUE_SUB_CHAR_TABLE, id),
        DumpValue::Record(id) => write_heap_ref_value(out, VALUE_RECORD, id),
        DumpValue::HashTable(id) => write_heap_ref_value(out, VALUE_HASH_TABLE, id),
        DumpValue::Obarray(id) => write_heap_ref_value(out, VALUE_OBARRAY, id),
        DumpValue::Lambda(id) => write_heap_ref_value(out, VALUE_LAMBDA, id),
        DumpValue::Macro(id) => write_heap_ref_value(out, VALUE_MACRO, id),
        DumpValue::Subr(id) => {
            write_u8(out, VALUE_SUBR);
            write_u32(out, id.0);
        }
        DumpValue::ByteCode(id) => write_heap_ref_value(out, VALUE_BYTE_CODE, id),
        DumpValue::Marker(id) => write_heap_ref_value(out, VALUE_MARKER, id),
        DumpValue::Overlay(id) => write_heap_ref_value(out, VALUE_OVERLAY, id),
        DumpValue::Buffer(id) => {
            write_u8(out, VALUE_BUFFER);
            write_u64(out, id.0);
        }
        DumpValue::Window(id) => {
            write_u8(out, VALUE_WINDOW);
            write_u64(out, *id);
        }
        DumpValue::Frame(id) => {
            write_u8(out, VALUE_FRAME);
            write_u64(out, *id);
        }
        DumpValue::Timer(id) => {
            write_u8(out, VALUE_TIMER);
            write_u64(out, *id);
        }
        DumpValue::Bignum(text) => {
            write_u8(out, VALUE_BIGNUM);
            write_string(out, text)?;
        }
        DumpValue::Unbound => write_u8(out, VALUE_UNBOUND),
    }
    Ok(())
}

fn write_heap_ref_value(out: &mut Vec<u8>, tag: u8, id: &DumpHeapRef) {
    write_u8(out, tag);
    write_u32(out, id.index);
}

fn write_values(out: &mut Vec<u8>, values: &[DumpValue]) -> Result<(), DumpError> {
    write_len(out, values.len(), "value count")?;
    for value in values {
        write_value(out, value)?;
    }
    Ok(())
}

fn write_hash_table(out: &mut Vec<u8>, table: &DumpLispHashTable) -> Result<(), DumpError> {
    write_hash_table_test(out, &table.test);
    write_opt_sym_id(out, table.test_name);
    write_i64(out, table.size);
    write_opt_hash_table_weakness(out, table.weakness.as_ref());
    write_f64(out, table.rehash_size);
    write_f64(out, table.rehash_threshold);
    write_ordered_hash_entries(out, &table.ordered_entries)?;
    Ok(())
}

fn write_hash_table_test(out: &mut Vec<u8>, test: &DumpHashTableTest) {
    write_u8(out, (*test).into());
}

fn write_opt_hash_table_weakness(out: &mut Vec<u8>, weakness: Option<&DumpHashTableWeakness>) {
    match weakness {
        Some(weakness) => {
            write_bool(out, true);
            write_u8(out, (*weakness).into());
        }
        None => write_bool(out, false),
    }
}

const HASH_KEY_NIL: u8 = 0;
const HASH_KEY_TRUE: u8 = 1;
const HASH_KEY_INT: u8 = 2;
const HASH_KEY_FLOAT: u8 = 3;
const HASH_KEY_FLOAT_EQ: u8 = 4;
const HASH_KEY_SYMBOL: u8 = 5;
const HASH_KEY_KEYWORD: u8 = 6;
const HASH_KEY_STR: u8 = 7;
const HASH_KEY_CHAR: u8 = 8;
const HASH_KEY_WINDOW: u8 = 9;
const HASH_KEY_FRAME: u8 = 10;
const HASH_KEY_PTR: u8 = 11;
const HASH_KEY_HEAP_REF: u8 = 12;
const HASH_KEY_EQUAL_CONS: u8 = 13;
const HASH_KEY_EQUAL_VEC: u8 = 14;
const HASH_KEY_SYMBOL_WITH_POS: u8 = 15;
const HASH_KEY_CYCLE: u8 = 16;
const HASH_KEY_TEXT: u8 = 17;
const HASH_KEY_MARKER: u8 = 18;
const HASH_KEY_OVERLAY: u8 = 19;
const HASH_KEY_BOOL_VEC: u8 = 20;
const HASH_KEY_BIGNUM: u8 = 21;
const HASH_KEY_BYTE_CODE: u8 = 22;

const BYTE_CODE_KEY_OBSERVABLE_SLOT_COUNT: u8 = 0;
const BYTE_CODE_KEY_VALUE: u8 = 1;
const BYTE_CODE_KEY_BYTES: u8 = 2;
const BYTE_CODE_KEY_OPS: u8 = 3;
const BYTE_CODE_KEY_VALUES: u8 = 4;
const BYTE_CODE_KEY_TEXT: u8 = 5;
const BYTE_CODE_KEY_ABSENT: u8 = 6;

fn write_hash_key(out: &mut Vec<u8>, key: &DumpHashKey) -> Result<(), DumpError> {
    match key {
        DumpHashKey::Nil => write_u8(out, HASH_KEY_NIL),
        DumpHashKey::True => write_u8(out, HASH_KEY_TRUE),
        DumpHashKey::Int(value) => {
            write_u8(out, HASH_KEY_INT);
            write_i64(out, *value);
        }
        DumpHashKey::Bignum(limbs) => {
            write_u8(out, HASH_KEY_BIGNUM);
            write_len(out, limbs.len(), "bignum hash key limb count")?;
            for limb in limbs {
                write_u64(out, *limb);
            }
        }
        DumpHashKey::Float(value) => {
            write_u8(out, HASH_KEY_FLOAT);
            write_u64(out, *value);
        }
        DumpHashKey::FloatEq(value, eq_hash) => {
            write_u8(out, HASH_KEY_FLOAT_EQ);
            write_u64(out, *value);
            write_u32(out, *eq_hash);
        }
        DumpHashKey::Symbol(id) => {
            write_u8(out, HASH_KEY_SYMBOL);
            write_u32(out, id.0);
        }
        DumpHashKey::Keyword(id) => {
            write_u8(out, HASH_KEY_KEYWORD);
            write_u32(out, id.0);
        }
        DumpHashKey::Str(id) => {
            write_u8(out, HASH_KEY_STR);
            write_u32(out, id.index);
        }
        DumpHashKey::Char(ch) => {
            write_u8(out, HASH_KEY_CHAR);
            write_u32(out, *ch as u32);
        }
        DumpHashKey::Window(id) => {
            write_u8(out, HASH_KEY_WINDOW);
            write_u64(out, *id);
        }
        DumpHashKey::Frame(id) => {
            write_u8(out, HASH_KEY_FRAME);
            write_u64(out, *id);
        }
        DumpHashKey::Ptr(id) => {
            write_u8(out, HASH_KEY_PTR);
            write_u64(out, *id);
        }
        DumpHashKey::HeapRef(index) => {
            write_u8(out, HASH_KEY_HEAP_REF);
            write_u32(out, *index);
        }
        DumpHashKey::EqualCons(car, cdr) => {
            write_u8(out, HASH_KEY_EQUAL_CONS);
            write_hash_key(out, car)?;
            write_hash_key(out, cdr)?;
        }
        DumpHashKey::EqualVec(keys) => {
            write_u8(out, HASH_KEY_EQUAL_VEC);
            write_hash_keys(out, keys)?;
        }
        DumpHashKey::ByteCode(parts) => {
            write_u8(out, HASH_KEY_BYTE_CODE);
            write_len(out, parts.len(), "byte-code hash key part count")?;
            for part in parts {
                write_byte_code_key_part(out, part)?;
            }
        }
        DumpHashKey::Marker(buffer, bytepos) => {
            write_u8(out, HASH_KEY_MARKER);
            write_opt_u64(out, *buffer);
            write_usize(out, *bytepos)?;
        }
        DumpHashKey::Overlay {
            buffer,
            start,
            end,
            plist,
        } => {
            write_u8(out, HASH_KEY_OVERLAY);
            write_opt_u64(out, *buffer);
            write_usize(out, *start)?;
            write_usize(out, *end)?;
            write_hash_key(out, plist)?;
        }
        DumpHashKey::BoolVec { len, bits } => {
            write_u8(out, HASH_KEY_BOOL_VEC);
            write_u32(out, *len);
            write_u64(out, *bits as u64);
            write_u64(out, (*bits >> 64) as u64);
        }
        DumpHashKey::SymbolWithPos(symbol, pos) => {
            write_u8(out, HASH_KEY_SYMBOL_WITH_POS);
            write_hash_key(out, symbol)?;
            write_hash_key(out, pos)?;
        }
        DumpHashKey::Cycle(index) => {
            write_u8(out, HASH_KEY_CYCLE);
            write_u32(out, *index);
        }
        DumpHashKey::Text(text) => {
            write_u8(out, HASH_KEY_TEXT);
            write_string(out, text)?;
        }
    }
    Ok(())
}

fn write_byte_code_key_part(
    out: &mut Vec<u8>,
    part: &DumpByteCodeKeyPart,
) -> Result<(), DumpError> {
    match part {
        DumpByteCodeKeyPart::ObservableSlotCount(count) => {
            write_u8(out, BYTE_CODE_KEY_OBSERVABLE_SLOT_COUNT);
            write_usize(out, *count)?;
        }
        DumpByteCodeKeyPart::Value(value) => {
            write_u8(out, BYTE_CODE_KEY_VALUE);
            write_hash_key(out, value)?;
        }
        DumpByteCodeKeyPart::Bytes(bytes) => {
            write_u8(out, BYTE_CODE_KEY_BYTES);
            write_bytes(out, bytes)?;
        }
        DumpByteCodeKeyPart::Ops(ops) => {
            write_u8(out, BYTE_CODE_KEY_OPS);
            write_ops(out, ops);
        }
        DumpByteCodeKeyPart::Values(values) => {
            write_u8(out, BYTE_CODE_KEY_VALUES);
            write_hash_keys(out, values)?;
        }
        DumpByteCodeKeyPart::Text { char_count, bytes } => {
            write_u8(out, BYTE_CODE_KEY_TEXT);
            write_usize(out, *char_count)?;
            write_bytes(out, bytes)?;
        }
        DumpByteCodeKeyPart::Absent => write_u8(out, BYTE_CODE_KEY_ABSENT),
    }
    Ok(())
}

fn write_hash_keys(out: &mut Vec<u8>, keys: &[DumpHashKey]) -> Result<(), DumpError> {
    write_len(out, keys.len(), "hash key count")?;
    for key in keys {
        write_hash_key(out, key)?;
    }
    Ok(())
}

fn write_ordered_hash_entries(
    out: &mut Vec<u8>,
    entries: &[(DumpHashKey, DumpValue, Option<DumpValue>)],
) -> Result<(), DumpError> {
    write_len(out, entries.len(), "hash entry count")?;
    for (key, value, snapshot) in entries {
        write_hash_key(out, key)?;
        write_value(out, value)?;
        match snapshot {
            Some(snap) => {
                write_bool(out, true);
                write_value(out, snap)?;
            }
            None => write_bool(out, false),
        }
    }
    Ok(())
}

fn write_text_property_runs(
    out: &mut Vec<u8>,
    runs: &[DumpStringTextPropertyRun],
) -> Result<(), DumpError> {
    write_len(out, runs.len(), "string text property run count")?;
    for run in runs {
        write_usize(out, run.start)?;
        write_usize(out, run.end)?;
        write_value(out, &run.plist)?;
    }
    Ok(())
}

fn write_byte_code(out: &mut Vec<u8>, function: &DumpByteCodeFunction) -> Result<(), DumpError> {
    match &function.instructions {
        DumpByteCodeInstructions::Decoded(ops) => {
            write_u8(out, BYTECODE_DECODED);
            write_ops(out, ops);
        }
        DumpByteCodeInstructions::Gnu(data) => {
            write_u8(out, BYTECODE_GNU);
            write_byte_data(out, data)?;
        }
    }
    write_values(out, &function.constants)?;
    write_u16(out, function.max_stack);
    write_lambda_params(out, &function.params)?;
    write_opt_value(out, function.arglist.as_ref())?;
    write_bool(out, function.lexical);
    write_opt_value(out, function.env.as_ref())?;
    write_opt_lisp_string(out, function.docstring.as_ref())?;
    write_opt_value(out, function.doc_form.as_ref())?;
    write_opt_value(out, function.interactive.as_ref())?;
    write_usize(out, function.closure_slot_count)?;
    write_values(out, &function.extra_slots)?;
    write_bool(out, function.ops_sealed);
    Ok(())
}

fn write_lambda_params(out: &mut Vec<u8>, params: &DumpLambdaParams) -> Result<(), DumpError> {
    write_sym_ids(out, &params.required)?;
    write_sym_ids(out, &params.optional)?;
    write_opt_sym_id(out, params.rest);
    Ok(())
}

fn write_sym_ids(out: &mut Vec<u8>, syms: &[DumpSymId]) -> Result<(), DumpError> {
    write_len(out, syms.len(), "symbol id count")?;
    for sym in syms {
        write_u32(out, sym.0);
    }
    Ok(())
}

const OP_CONSTANT: u8 = 0;
const OP_NIL: u8 = 1;
const OP_TRUE: u8 = 2;
const OP_POP: u8 = 3;
const OP_DUP: u8 = 4;
const OP_STACK_REF: u8 = 5;
const OP_STACK_SET: u8 = 6;
const OP_DISCARD_N: u8 = 7;
const OP_VAR_REF: u8 = 8;
const OP_VAR_SET: u8 = 9;
const OP_VAR_BIND: u8 = 10;
const OP_UNBIND: u8 = 11;
const OP_CALL: u8 = 12;
const OP_APPLY: u8 = 13;
const OP_GOTO: u8 = 14;
const OP_GOTO_IF_NIL: u8 = 15;
const OP_GOTO_IF_NOT_NIL: u8 = 16;
const OP_GOTO_IF_NIL_ELSE_POP: u8 = 17;
const OP_GOTO_IF_NOT_NIL_ELSE_POP: u8 = 18;
const OP_SWITCH: u8 = 19;
const OP_RETURN: u8 = 20;
const OP_ADD: u8 = 21;
const OP_SUB: u8 = 22;
const OP_MUL: u8 = 23;
const OP_DIV: u8 = 24;
const OP_REM: u8 = 25;
const OP_ADD1: u8 = 26;
const OP_SUB1: u8 = 27;
const OP_NEGATE: u8 = 28;
const OP_EQLSIGN: u8 = 29;
const OP_GTR: u8 = 30;
const OP_LSS: u8 = 31;
const OP_LEQ: u8 = 32;
const OP_GEQ: u8 = 33;
const OP_MAX: u8 = 34;
const OP_MIN: u8 = 35;
const OP_CAR: u8 = 36;
const OP_CDR: u8 = 37;
const OP_CONS: u8 = 38;
const OP_LIST: u8 = 39;
const OP_LENGTH: u8 = 40;
const OP_NTH: u8 = 41;
const OP_NTHCDR: u8 = 42;
const OP_SETCAR: u8 = 43;
const OP_SETCDR: u8 = 44;
const OP_CAR_SAFE: u8 = 45;
const OP_CDR_SAFE: u8 = 46;
const OP_ELT: u8 = 47;
const OP_NCONC: u8 = 48;
const OP_NREVERSE: u8 = 49;
const OP_MEMBER: u8 = 50;
const OP_MEMQ: u8 = 51;
const OP_ASSQ: u8 = 52;
const OP_SYMBOLP: u8 = 53;
const OP_CONSP: u8 = 54;
const OP_STRINGP: u8 = 55;
const OP_LISTP: u8 = 56;
const OP_INTEGERP: u8 = 57;
const OP_NUMBERP: u8 = 58;
const OP_NULL: u8 = 59;
const OP_NOT: u8 = 60;
const OP_EQ: u8 = 61;
const OP_EQUAL: u8 = 62;
const OP_CONCAT: u8 = 63;
const OP_SUBSTRING: u8 = 64;
const OP_STRING_EQUAL: u8 = 65;
const OP_STRING_LESSP: u8 = 66;
const OP_AREF: u8 = 67;
const OP_ASET: u8 = 68;
const OP_SYMBOL_VALUE: u8 = 69;
const OP_SYMBOL_FUNCTION: u8 = 70;
const OP_SET: u8 = 71;
const OP_FSET: u8 = 72;
const OP_GET: u8 = 73;
const OP_PUT: u8 = 74;
const OP_PUSH_CONDITION_CASE: u8 = 75;
const OP_PUSH_CONDITION_CASE_RAW: u8 = 76;
const OP_PUSH_CATCH: u8 = 77;
const OP_POP_HANDLER: u8 = 78;
const OP_UNWIND_PROTECT: u8 = 79;
const OP_UNWIND_PROTECT_POP: u8 = 80;
const OP_THROW: u8 = 81;
const OP_SAVE_CURRENT_BUFFER: u8 = 82;
const OP_SAVE_EXCURSION: u8 = 83;
const OP_SAVE_RESTRICTION: u8 = 84;
const OP_SAVE_WINDOW_EXCURSION: u8 = 85;
const OP_MAKE_CLOSURE: u8 = 86;
const OP_CALL_BUILTIN: u8 = 87;
const OP_CALL_BUILTIN_SYM: u8 = 88;
const OP_TRAP_OUT_OF_RANGE_CONSTANT: u8 = 89;

/// Flat fixed-width bytecode instruction records: one bounds check and a
/// direct `Op` construction per instruction. The old per-field cursor
/// encode of an intermediate `DumpOp` cost ~19M Ir of startup across the
/// dump's ~6.7k bytecode objects.
///
/// Record layout: `[tag: u8][arg: u32 LE][extra: u8]` — 6 bytes per op.
fn write_ops(out: &mut Vec<u8>, ops: &[Op]) {
    #[inline]
    fn put_op(out: &mut Vec<u8>, tag: u8, arg: u32, extra: u8) {
        out.push(tag);
        out.extend_from_slice(&arg.to_le_bytes());
        out.push(extra);
    }
    write_u64(out, ops.len() as u64);
    for op in ops {
        match op {
            Op::Constant(value) => put_op(out, OP_CONSTANT, *value as u32, 0),
            Op::StackRef(value) => put_op(out, OP_STACK_REF, *value as u32, 0),
            Op::StackSet(value) => put_op(out, OP_STACK_SET, *value as u32, 0),
            Op::VarRef(value) => put_op(out, OP_VAR_REF, *value as u32, 0),
            Op::VarSet(value) => put_op(out, OP_VAR_SET, *value as u32, 0),
            Op::VarBind(value) => put_op(out, OP_VAR_BIND, *value as u32, 0),
            Op::Unbind(value) => put_op(out, OP_UNBIND, *value as u32, 0),
            Op::Call(value) => put_op(out, OP_CALL, *value as u32, 0),
            Op::Apply(value) => put_op(out, OP_APPLY, *value as u32, 0),
            Op::List(value) => put_op(out, OP_LIST, *value as u32, 0),
            Op::Concat(value) => put_op(out, OP_CONCAT, *value as u32, 0),
            Op::MakeClosure(value) => put_op(out, OP_MAKE_CLOSURE, *value as u32, 0),
            Op::Goto(value) => put_op(out, OP_GOTO, *value, 0),
            Op::GotoIfNil(value) => put_op(out, OP_GOTO_IF_NIL, *value, 0),
            Op::GotoIfNotNil(value) => put_op(out, OP_GOTO_IF_NOT_NIL, *value, 0),
            Op::GotoIfNilElsePop(value) => put_op(out, OP_GOTO_IF_NIL_ELSE_POP, *value, 0),
            Op::PushConditionCase(value) => put_op(out, OP_PUSH_CONDITION_CASE, *value, 0),
            Op::PushCatch(value) => put_op(out, OP_PUSH_CATCH, *value, 0),
            Op::GotoIfNotNilElsePop(value) => put_op(out, OP_GOTO_IF_NOT_NIL_ELSE_POP, *value, 0),
            Op::PushConditionCaseRaw(value) => put_op(out, OP_PUSH_CONDITION_CASE_RAW, *value, 0),
            Op::TrapOutOfRangeConstant(value) => {
                put_op(out, OP_TRAP_OUT_OF_RANGE_CONSTANT, *value as u32, 0)
            }
            Op::Nil => put_op(out, OP_NIL, 0, 0),
            Op::True => put_op(out, OP_TRUE, 0, 0),
            Op::Pop => put_op(out, OP_POP, 0, 0),
            Op::Dup => put_op(out, OP_DUP, 0, 0),
            Op::Switch => put_op(out, OP_SWITCH, 0, 0),
            Op::Return => put_op(out, OP_RETURN, 0, 0),
            Op::Add => put_op(out, OP_ADD, 0, 0),
            Op::Sub => put_op(out, OP_SUB, 0, 0),
            Op::Mul => put_op(out, OP_MUL, 0, 0),
            Op::Div => put_op(out, OP_DIV, 0, 0),
            Op::Rem => put_op(out, OP_REM, 0, 0),
            Op::Add1 => put_op(out, OP_ADD1, 0, 0),
            Op::Sub1 => put_op(out, OP_SUB1, 0, 0),
            Op::Negate => put_op(out, OP_NEGATE, 0, 0),
            Op::Eqlsign => put_op(out, OP_EQLSIGN, 0, 0),
            Op::Gtr => put_op(out, OP_GTR, 0, 0),
            Op::Lss => put_op(out, OP_LSS, 0, 0),
            Op::Leq => put_op(out, OP_LEQ, 0, 0),
            Op::Geq => put_op(out, OP_GEQ, 0, 0),
            Op::Max => put_op(out, OP_MAX, 0, 0),
            Op::Min => put_op(out, OP_MIN, 0, 0),
            Op::Car => put_op(out, OP_CAR, 0, 0),
            Op::Cdr => put_op(out, OP_CDR, 0, 0),
            Op::Cons => put_op(out, OP_CONS, 0, 0),
            Op::Length => put_op(out, OP_LENGTH, 0, 0),
            Op::Nth => put_op(out, OP_NTH, 0, 0),
            Op::Nthcdr => put_op(out, OP_NTHCDR, 0, 0),
            Op::Setcar => put_op(out, OP_SETCAR, 0, 0),
            Op::Setcdr => put_op(out, OP_SETCDR, 0, 0),
            Op::CarSafe => put_op(out, OP_CAR_SAFE, 0, 0),
            Op::CdrSafe => put_op(out, OP_CDR_SAFE, 0, 0),
            Op::Elt => put_op(out, OP_ELT, 0, 0),
            Op::Nconc => put_op(out, OP_NCONC, 0, 0),
            Op::Nreverse => put_op(out, OP_NREVERSE, 0, 0),
            Op::Member => put_op(out, OP_MEMBER, 0, 0),
            Op::Memq => put_op(out, OP_MEMQ, 0, 0),
            Op::Assq => put_op(out, OP_ASSQ, 0, 0),
            Op::Symbolp => put_op(out, OP_SYMBOLP, 0, 0),
            Op::Consp => put_op(out, OP_CONSP, 0, 0),
            Op::Stringp => put_op(out, OP_STRINGP, 0, 0),
            Op::Listp => put_op(out, OP_LISTP, 0, 0),
            Op::Integerp => put_op(out, OP_INTEGERP, 0, 0),
            Op::Numberp => put_op(out, OP_NUMBERP, 0, 0),
            Op::Null => put_op(out, OP_NULL, 0, 0),
            Op::Not => put_op(out, OP_NOT, 0, 0),
            Op::Eq => put_op(out, OP_EQ, 0, 0),
            Op::Equal => put_op(out, OP_EQUAL, 0, 0),
            Op::Substring => put_op(out, OP_SUBSTRING, 0, 0),
            Op::StringEqual => put_op(out, OP_STRING_EQUAL, 0, 0),
            Op::StringLessp => put_op(out, OP_STRING_LESSP, 0, 0),
            Op::Aref => put_op(out, OP_AREF, 0, 0),
            Op::Aset => put_op(out, OP_ASET, 0, 0),
            Op::SymbolValue => put_op(out, OP_SYMBOL_VALUE, 0, 0),
            Op::SymbolFunction => put_op(out, OP_SYMBOL_FUNCTION, 0, 0),
            Op::Set => put_op(out, OP_SET, 0, 0),
            Op::Fset => put_op(out, OP_FSET, 0, 0),
            Op::Get => put_op(out, OP_GET, 0, 0),
            Op::Put => put_op(out, OP_PUT, 0, 0),
            Op::PopHandler => put_op(out, OP_POP_HANDLER, 0, 0),
            Op::UnwindProtectPop => put_op(out, OP_UNWIND_PROTECT_POP, 0, 0),
            Op::Throw => put_op(out, OP_THROW, 0, 0),
            Op::SaveCurrentBuffer => put_op(out, OP_SAVE_CURRENT_BUFFER, 0, 0),
            Op::SaveExcursion => put_op(out, OP_SAVE_EXCURSION, 0, 0),
            Op::SaveRestriction => put_op(out, OP_SAVE_RESTRICTION, 0, 0),
            Op::SaveWindowExcursion => put_op(out, OP_SAVE_WINDOW_EXCURSION, 0, 0),
            Op::DiscardN(value) => put_op(out, OP_DISCARD_N, *value as u32, 0),
            Op::CallBuiltin(index, argc) => put_op(out, OP_CALL_BUILTIN, *index as u32, *argc),
            Op::CallBuiltinSym(sym, argc) => put_op(
                out,
                OP_CALL_BUILTIN_SYM,
                super::convert::dump_sym_id(*sym).0,
                *argc,
            ),
        }
    }
}

fn write_op_u16(out: &mut Vec<u8>, tag: u8, value: u16) {
    write_u8(out, tag);
    write_u16(out, value);
}

fn write_op_u32(out: &mut Vec<u8>, tag: u8, value: u32) {
    write_u8(out, tag);
    write_u32(out, value);
}

fn write_marker(out: &mut Vec<u8>, marker: &DumpMarker) -> Result<(), DumpError> {
    write_opt_buffer_id(out, marker.buffer);
    write_bool(out, marker.insertion_type);
    write_opt_u64(out, marker.marker_id);
    write_usize(out, marker.bytepos)?;
    write_usize(out, marker.charpos)?;
    write_bool(out, marker.last_position_valid);
    Ok(())
}

fn write_overlay(out: &mut Vec<u8>, overlay: &DumpOverlay) -> Result<(), DumpError> {
    write_u64(out, overlay.serial);
    write_value(out, &overlay.plist)?;
    write_opt_buffer_id(out, overlay.buffer);
    write_usize(out, overlay.start)?;
    write_usize(out, overlay.end)?;
    write_bool(out, overlay.front_advance);
    write_bool(out, overlay.rear_advance);
    Ok(())
}

fn write_lisp_string(out: &mut Vec<u8>, string: &DumpLispString) -> Result<(), DumpError> {
    write_bytes(out, &string.data)?;
    write_usize(out, string.size)?;
    write_i64(out, string.size_byte);
    Ok(())
}

fn write_opt_lisp_string(
    out: &mut Vec<u8>,
    string: Option<&DumpLispString>,
) -> Result<(), DumpError> {
    match string {
        Some(string) => {
            write_bool(out, true);
            write_lisp_string(out, string)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn write_opt_value(out: &mut Vec<u8>, value: Option<&DumpValue>) -> Result<(), DumpError> {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_value(out, value)?;
        }
        None => write_bool(out, false),
    }
    Ok(())
}

fn write_opt_sym_id(out: &mut Vec<u8>, id: Option<DumpSymId>) {
    match id {
        Some(id) => {
            write_bool(out, true);
            write_u32(out, id.0);
        }
        None => write_bool(out, false),
    }
}

fn write_opt_buffer_id(out: &mut Vec<u8>, id: Option<DumpBufferId>) {
    match id {
        Some(id) => {
            write_bool(out, true);
            write_u64(out, id.0);
        }
        None => write_bool(out, false),
    }
}

fn write_opt_u16(out: &mut Vec<u8>, value: Option<u16>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_u16(out, value);
        }
        None => write_bool(out, false),
    }
}

fn write_opt_u64(out: &mut Vec<u8>, value: Option<u64>) {
    match value {
        Some(value) => {
            write_bool(out, true);
            write_u64(out, value);
        }
        None => write_bool(out, false),
    }
}

fn write_len(out: &mut Vec<u8>, len: usize, what: &str) -> Result<(), DumpError> {
    let len = u64::try_from(len)
        .map_err(|_| DumpError::SerializationError(format!("{what} overflows u64")))?;
    write_u64(out, len);
    Ok(())
}

fn write_usize(out: &mut Vec<u8>, value: usize) -> Result<(), DumpError> {
    let value = u64::try_from(value)
        .map_err(|_| DumpError::SerializationError("usize value overflows u64".into()))?;
    write_u64(out, value);
    Ok(())
}

fn write_string(out: &mut Vec<u8>, text: &str) -> Result<(), DumpError> {
    write_bytes(out, text.as_bytes())
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) -> Result<(), DumpError> {
    write_len(out, bytes.len(), "byte payload length")?;
    out.extend_from_slice(bytes);
    Ok(())
}

pub(crate) fn write_bool(out: &mut Vec<u8>, value: bool) {
    write_u8(out, u8::from(value));
}

pub(crate) fn write_u8(out: &mut Vec<u8>, value: u8) {
    out.push(value);
}

pub(crate) fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_ne_bytes());
}

pub(crate) fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_ne_bytes());
}

pub(crate) fn write_u64(out: &mut Vec<u8>, value: u64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn write_i64(out: &mut Vec<u8>, value: i64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

fn write_f64(out: &mut Vec<u8>, value: f64) {
    out.extend_from_slice(&value.to_ne_bytes());
}

pub(crate) struct Cursor<'a> {
    section: &'a [u8],
    offset: usize,
}

impl<'a> Cursor<'a> {
    pub(crate) fn new(section: &'a [u8]) -> Self {
        Self { section, offset: 0 }
    }

    pub(crate) fn new_at(section: &'a [u8], offset: usize) -> Self {
        Self { section, offset }
    }

    pub(crate) fn is_empty(&self) -> bool {
        self.offset == self.section.len()
    }

    pub(crate) fn remaining(&self) -> usize {
        self.section.len() - self.offset
    }

    pub(crate) fn read_heap_object(&mut self) -> Result<DumpHeapObject, DumpError> {
        let tag = self.read_u8("heap object tag")?;
        self.read_heap_object_from_tag(tag)
    }

    fn read_heap_object_from_tag(&mut self, tag: u8) -> Result<DumpHeapObject, DumpError> {
        match tag {
            HEAP_CONS => Ok(DumpHeapObject::Cons {
                car: self.read_value()?,
                cdr: self.read_value()?,
            }),
            HEAP_VECTOR => Ok(DumpHeapObject::Vector(self.read_values()?)),
            HEAP_CHAR_TABLE => Ok(DumpHeapObject::CharTable {
                defalt: self.read_value()?,
                parent: self.read_value()?,
                purpose: self.read_value()?,
                ascii: self.read_value()?,
                contents: self.read_values()?,
                extras: self.read_values()?,
            }),
            HEAP_SUB_CHAR_TABLE => Ok(DumpHeapObject::SubCharTable {
                depth: self.read_i64("sub-char-table depth")?,
                min_char: self.read_i64("sub-char-table min-char")?,
                contents: self.read_values()?,
            }),
            HEAP_HASH_TABLE => Ok(DumpHeapObject::HashTable(self.read_hash_table()?)),
            HEAP_OBARRAY => Ok(DumpHeapObject::Obarray {
                buckets: self.read_values()?,
                count: self.read_u32("obarray count")?,
            }),
            HEAP_STRING => Ok(DumpHeapObject::Str {
                data: self.read_byte_data()?,
                size: self.read_usize("string char size")?,
                size_byte: self.read_i64("string byte size")?,
                text_props: self.read_text_property_runs()?,
            }),
            HEAP_FLOAT => Ok(DumpHeapObject::Float(self.read_f64("float object")?)),
            HEAP_LAMBDA => Ok(DumpHeapObject::Lambda(self.read_values()?)),
            HEAP_MACRO => Ok(DumpHeapObject::Macro(self.read_values()?)),
            HEAP_BYTE_CODE => Ok(DumpHeapObject::ByteCode(self.read_byte_code()?)),
            HEAP_RECORD => Ok(DumpHeapObject::Record(self.read_values()?)),
            HEAP_MARKER => Ok(DumpHeapObject::Marker(self.read_marker()?)),
            HEAP_OVERLAY => Ok(DumpHeapObject::Overlay(self.read_overlay()?)),
            HEAP_BUFFER => Ok(DumpHeapObject::Buffer(DumpBufferId(
                self.read_u64("buffer object id")?,
            ))),
            HEAP_WINDOW => Ok(DumpHeapObject::Window(self.read_u64("window object id")?)),
            HEAP_FRAME => Ok(DumpHeapObject::Frame(self.read_u64("frame object id")?)),
            HEAP_TIMER => Ok(DumpHeapObject::Timer(self.read_u64("timer object id")?)),
            HEAP_SUBR => Ok(DumpHeapObject::Subr {
                name: DumpNameId(self.read_u32("subr name id")?),
                min_args: self.read_u16("subr min args")?,
                max_args: self.read_opt_u16()?,
            }),
            HEAP_FREE => Ok(DumpHeapObject::Free),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown heap object tag {other}"
            ))),
        }
    }

    fn read_byte_data(&mut self) -> Result<DumpByteData, DumpError> {
        match self.read_u8("byte data tag")? {
            BYTE_OWNED => Ok(DumpByteData::owned(self.read_bytes()?)),
            BYTE_MAPPED => Ok(DumpByteData::mapped(
                self.read_u64("mapped byte offset")?,
                self.read_u64("mapped byte length")?,
            )),
            BYTE_STATIC_RODATA => Ok(DumpByteData::static_rodata(
                self.read_u64("static rodata key")?,
                self.read_u64("static rodata length")?,
            )),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown byte data tag {other}"
            ))),
        }
    }

    pub(crate) fn read_value(&mut self) -> Result<DumpValue, DumpError> {
        match self.read_u8("dump value tag")? {
            VALUE_NIL => Ok(DumpValue::Nil),
            VALUE_TRUE => Ok(DumpValue::True),
            VALUE_INT => Ok(DumpValue::Int(self.read_i64("fixnum value")?)),
            VALUE_FLOAT => Ok(DumpValue::Float(self.read_heap_ref("float heap ref")?)),
            VALUE_SYMBOL => Ok(DumpValue::Symbol(DumpSymId(self.read_u32("symbol id")?))),
            VALUE_STR => Ok(DumpValue::Str(self.read_heap_ref("string heap ref")?)),
            VALUE_CONS => Ok(DumpValue::Cons(self.read_heap_ref("cons heap ref")?)),
            VALUE_VECTOR => Ok(DumpValue::Vector(self.read_heap_ref("vector heap ref")?)),
            VALUE_CHAR_TABLE => Ok(DumpValue::CharTable(
                self.read_heap_ref("char-table heap ref")?,
            )),
            VALUE_SUB_CHAR_TABLE => Ok(DumpValue::SubCharTable(
                self.read_heap_ref("sub-char-table heap ref")?,
            )),
            VALUE_RECORD => Ok(DumpValue::Record(self.read_heap_ref("record heap ref")?)),
            VALUE_HASH_TABLE => Ok(DumpValue::HashTable(
                self.read_heap_ref("hash table heap ref")?,
            )),
            VALUE_OBARRAY => Ok(DumpValue::Obarray(self.read_heap_ref("obarray heap ref")?)),
            VALUE_LAMBDA => Ok(DumpValue::Lambda(self.read_heap_ref("lambda heap ref")?)),
            VALUE_MACRO => Ok(DumpValue::Macro(self.read_heap_ref("macro heap ref")?)),
            VALUE_SUBR => Ok(DumpValue::Subr(DumpNameId(self.read_u32("subr id")?))),
            VALUE_BYTE_CODE => Ok(DumpValue::ByteCode(
                self.read_heap_ref("bytecode heap ref")?,
            )),
            VALUE_MARKER => Ok(DumpValue::Marker(self.read_heap_ref("marker heap ref")?)),
            VALUE_OVERLAY => Ok(DumpValue::Overlay(self.read_heap_ref("overlay heap ref")?)),
            VALUE_BUFFER => Ok(DumpValue::Buffer(DumpBufferId(self.read_u64("buffer id")?))),
            VALUE_WINDOW => Ok(DumpValue::Window(self.read_u64("window id")?)),
            VALUE_FRAME => Ok(DumpValue::Frame(self.read_u64("frame id")?)),
            VALUE_TIMER => Ok(DumpValue::Timer(self.read_u64("timer id")?)),
            VALUE_BIGNUM => Ok(DumpValue::Bignum(self.read_string()?)),
            VALUE_UNBOUND => Ok(DumpValue::Unbound),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown dump value tag {other}"
            ))),
        }
    }

    fn read_heap_ref(&mut self, what: &str) -> Result<DumpHeapRef, DumpError> {
        Ok(DumpHeapRef {
            index: self.read_u32(what)?,
        })
    }

    fn read_values(&mut self) -> Result<Vec<DumpValue>, DumpError> {
        let len = self.read_len("value count")?;
        let mut values = Vec::with_capacity(len);
        for _ in 0..len {
            values.push(self.read_value()?);
        }
        Ok(values)
    }

    fn read_hash_table(&mut self) -> Result<DumpLispHashTable, DumpError> {
        Ok(DumpLispHashTable {
            test: self.read_hash_table_test()?,
            test_name: self.read_opt_sym_id()?,
            size: self.read_i64("hash table size")?,
            weakness: self.read_opt_hash_table_weakness()?,
            rehash_size: self.read_f64("hash table rehash size")?,
            rehash_threshold: self.read_f64("hash table rehash threshold")?,
            ordered_entries: self.read_ordered_hash_entries()?,
        })
    }

    fn read_hash_table_test(&mut self) -> Result<DumpHashTableTest, DumpError> {
        let tag = self.read_u8("hash table test")?;
        DumpHashTableTest::try_from(tag)
            .map_err(|_| DumpError::ImageFormatError(format!("unknown hash table test tag {tag}")))
    }

    fn read_opt_hash_table_weakness(&mut self) -> Result<Option<DumpHashTableWeakness>, DumpError> {
        if !self.read_bool("hash table weakness present")? {
            return Ok(None);
        }
        let tag = self.read_u8("hash table weakness")?;
        DumpHashTableWeakness::try_from(tag).map(Some).map_err(|_| {
            DumpError::ImageFormatError(format!("unknown hash table weakness tag {tag}"))
        })
    }

    fn read_hash_key(&mut self) -> Result<DumpHashKey, DumpError> {
        match self.read_u8("hash key tag")? {
            HASH_KEY_NIL => Ok(DumpHashKey::Nil),
            HASH_KEY_TRUE => Ok(DumpHashKey::True),
            HASH_KEY_INT => Ok(DumpHashKey::Int(self.read_i64("hash int key")?)),
            HASH_KEY_BIGNUM => {
                let len = self.read_len("bignum hash key limb count")?;
                let mut limbs = Vec::with_capacity(len);
                for _ in 0..len {
                    limbs.push(self.read_u64("bignum hash key limb")?);
                }
                Ok(DumpHashKey::Bignum(limbs))
            }
            HASH_KEY_FLOAT => Ok(DumpHashKey::Float(self.read_u64("hash float key")?)),
            HASH_KEY_FLOAT_EQ => Ok(DumpHashKey::FloatEq(
                self.read_u64("hash float eq key")?,
                self.read_u32("hash float eq hash")?,
            )),
            HASH_KEY_SYMBOL => Ok(DumpHashKey::Symbol(DumpSymId(
                self.read_u32("hash symbol key")?,
            ))),
            HASH_KEY_KEYWORD => Ok(DumpHashKey::Keyword(DumpSymId(
                self.read_u32("hash keyword key")?,
            ))),
            HASH_KEY_STR => Ok(DumpHashKey::Str(DumpHeapRef {
                index: self.read_u32("hash string key")?,
            })),
            HASH_KEY_CHAR => {
                let raw = self.read_u32("hash char key")?;
                let ch = char::from_u32(raw).ok_or_else(|| {
                    DumpError::ImageFormatError(format!("invalid hash char scalar {raw}"))
                })?;
                Ok(DumpHashKey::Char(ch))
            }
            HASH_KEY_WINDOW => Ok(DumpHashKey::Window(self.read_u64("hash window key")?)),
            HASH_KEY_FRAME => Ok(DumpHashKey::Frame(self.read_u64("hash frame key")?)),
            HASH_KEY_PTR => Ok(DumpHashKey::Ptr(self.read_u64("hash ptr key")?)),
            HASH_KEY_HEAP_REF => Ok(DumpHashKey::HeapRef(self.read_u32("hash heap ref key")?)),
            HASH_KEY_EQUAL_CONS => Ok(DumpHashKey::EqualCons(
                Box::new(self.read_hash_key()?),
                Box::new(self.read_hash_key()?),
            )),
            HASH_KEY_EQUAL_VEC => Ok(DumpHashKey::EqualVec(self.read_hash_keys()?)),
            HASH_KEY_BYTE_CODE => {
                let len = self.read_len("byte-code hash key part count")?;
                let mut parts = Vec::with_capacity(len);
                for _ in 0..len {
                    parts.push(self.read_byte_code_key_part()?);
                }
                Ok(DumpHashKey::ByteCode(parts))
            }
            HASH_KEY_MARKER => Ok(DumpHashKey::Marker(
                self.read_opt_u64()?,
                self.read_usize("hash marker byte position")?,
            )),
            HASH_KEY_OVERLAY => Ok(DumpHashKey::Overlay {
                buffer: self.read_opt_u64()?,
                start: self.read_usize("hash overlay start")?,
                end: self.read_usize("hash overlay end")?,
                plist: Box::new(self.read_hash_key()?),
            }),
            HASH_KEY_BOOL_VEC => {
                let len = self.read_u32("bool-vector hash key length")?;
                let low = self.read_u64("bool-vector hash key low bits")?;
                let high = self.read_u64("bool-vector hash key high bits")?;
                Ok(DumpHashKey::BoolVec {
                    len,
                    bits: u128::from(low) | (u128::from(high) << 64),
                })
            }
            HASH_KEY_SYMBOL_WITH_POS => Ok(DumpHashKey::SymbolWithPos(
                Box::new(self.read_hash_key()?),
                Box::new(self.read_hash_key()?),
            )),
            HASH_KEY_CYCLE => Ok(DumpHashKey::Cycle(self.read_u32("hash cycle key")?)),
            HASH_KEY_TEXT => Ok(DumpHashKey::Text(self.read_string()?)),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown hash key tag {other}"
            ))),
        }
    }

    fn read_hash_keys(&mut self) -> Result<Vec<DumpHashKey>, DumpError> {
        let len = self.read_len("hash key count")?;
        let mut keys = Vec::with_capacity(len);
        for _ in 0..len {
            keys.push(self.read_hash_key()?);
        }
        Ok(keys)
    }

    fn read_byte_code_key_part(&mut self) -> Result<DumpByteCodeKeyPart, DumpError> {
        match self.read_u8("byte-code hash key part tag")? {
            BYTE_CODE_KEY_OBSERVABLE_SLOT_COUNT => Ok(DumpByteCodeKeyPart::ObservableSlotCount(
                self.read_usize("byte-code observable slot count")?,
            )),
            BYTE_CODE_KEY_VALUE => Ok(DumpByteCodeKeyPart::Value(self.read_hash_key()?)),
            BYTE_CODE_KEY_BYTES => Ok(DumpByteCodeKeyPart::Bytes(self.read_bytes()?)),
            BYTE_CODE_KEY_OPS => Ok(DumpByteCodeKeyPart::Ops(self.read_ops()?)),
            BYTE_CODE_KEY_VALUES => Ok(DumpByteCodeKeyPart::Values(self.read_hash_keys()?)),
            BYTE_CODE_KEY_TEXT => Ok(DumpByteCodeKeyPart::Text {
                char_count: self.read_usize("byte-code hash key character count")?,
                bytes: self.read_bytes()?,
            }),
            BYTE_CODE_KEY_ABSENT => Ok(DumpByteCodeKeyPart::Absent),
            other => Err(DumpError::ImageFormatError(format!(
                "unknown byte-code hash key part tag {other}"
            ))),
        }
    }

    fn read_ordered_hash_entries(
        &mut self,
    ) -> Result<Vec<(DumpHashKey, DumpValue, Option<DumpValue>)>, DumpError> {
        let len = self.read_len("hash entry count")?;
        let mut entries = Vec::with_capacity(len);
        for _ in 0..len {
            let key = self.read_hash_key()?;
            let value = self.read_value()?;
            let snapshot = if self.read_bool("hash key snapshot present")? {
                Some(self.read_value()?)
            } else {
                None
            };
            entries.push((key, value, snapshot));
        }
        Ok(entries)
    }

    pub(crate) fn read_text_property_runs(
        &mut self,
    ) -> Result<Vec<DumpStringTextPropertyRun>, DumpError> {
        let len = self.read_len("string text property run count")?;
        let mut runs = Vec::with_capacity(len);
        for _ in 0..len {
            runs.push(DumpStringTextPropertyRun {
                start: self.read_usize("string text property start")?,
                end: self.read_usize("string text property end")?,
                plist: self.read_value()?,
            });
        }
        Ok(runs)
    }

    fn read_byte_code(&mut self) -> Result<DumpByteCodeFunction, DumpError> {
        let instructions = match self.read_u8("bytecode instruction source")? {
            BYTECODE_DECODED => DumpByteCodeInstructions::Decoded(self.read_ops()?),
            BYTECODE_GNU => DumpByteCodeInstructions::Gnu(self.read_byte_data()?),
            other => {
                return Err(DumpError::ImageFormatError(format!(
                    "unknown bytecode instruction source {other}"
                )));
            }
        };
        Ok(DumpByteCodeFunction {
            instructions,
            constants: self.read_values()?,
            max_stack: self.read_u16("bytecode max stack")?,
            params: self.read_lambda_params()?,
            arglist: self.read_opt_value()?,
            lexical: self.read_bool("bytecode lexical flag")?,
            env: self.read_opt_value()?,
            docstring: self.read_opt_lisp_string()?,
            doc_form: self.read_opt_value()?,
            interactive: self.read_opt_value()?,
            closure_slot_count: self.read_usize("bytecode closure slot count")?,
            extra_slots: self.read_values()?,
            ops_sealed: self.read_bool("bytecode ops sealed flag")?,
        })
    }

    fn read_lambda_params(&mut self) -> Result<DumpLambdaParams, DumpError> {
        Ok(DumpLambdaParams {
            required: self.read_sym_ids()?,
            optional: self.read_sym_ids()?,
            rest: self.read_opt_sym_id()?,
        })
    }

    fn read_sym_ids(&mut self) -> Result<Vec<DumpSymId>, DumpError> {
        let len = self.read_len("symbol id count")?;
        let mut syms = Vec::with_capacity(len);
        for _ in 0..len {
            syms.push(DumpSymId(self.read_u32("symbol id")?));
        }
        Ok(syms)
    }

    fn read_ops(&mut self) -> Result<Vec<Op>, DumpError> {
        let len = self.read_len("bytecode op count")?;
        let mut ops = Vec::with_capacity(len);
        for _ in 0..len {
            let start = self.read_fixed_start(6, "bytecode op record")?;
            let rec = &self.section[start..start + 6];
            let tag = rec[0];
            let arg = u32::from_le_bytes([rec[1], rec[2], rec[3], rec[4]]);
            let extra = rec[5];
            let op = match tag {
                OP_CONSTANT => Op::Constant(arg as u16),
                OP_STACK_REF => Op::StackRef(arg as u16),
                OP_STACK_SET => Op::StackSet(arg as u16),
                OP_VAR_REF => Op::VarRef(arg as u16),
                OP_VAR_SET => Op::VarSet(arg as u16),
                OP_VAR_BIND => Op::VarBind(arg as u16),
                OP_UNBIND => Op::Unbind(arg as u16),
                OP_CALL => Op::Call(arg as u16),
                OP_APPLY => Op::Apply(arg as u16),
                OP_LIST => Op::List(arg as u16),
                OP_CONCAT => Op::Concat(arg as u16),
                OP_MAKE_CLOSURE => Op::MakeClosure(arg as u16),
                OP_GOTO => Op::Goto(arg),
                OP_GOTO_IF_NIL => Op::GotoIfNil(arg),
                OP_GOTO_IF_NOT_NIL => Op::GotoIfNotNil(arg),
                OP_GOTO_IF_NIL_ELSE_POP => Op::GotoIfNilElsePop(arg),
                OP_PUSH_CONDITION_CASE => Op::PushConditionCase(arg),
                OP_PUSH_CATCH => Op::PushCatch(arg),
                OP_GOTO_IF_NOT_NIL_ELSE_POP => Op::GotoIfNotNilElsePop(arg),
                OP_PUSH_CONDITION_CASE_RAW => Op::PushConditionCaseRaw(arg),
                OP_TRAP_OUT_OF_RANGE_CONSTANT => Op::TrapOutOfRangeConstant(arg as u16),
                OP_NIL => Op::Nil,
                OP_TRUE => Op::True,
                OP_POP => Op::Pop,
                OP_DUP => Op::Dup,
                OP_SWITCH => Op::Switch,
                OP_RETURN => Op::Return,
                OP_ADD => Op::Add,
                OP_SUB => Op::Sub,
                OP_MUL => Op::Mul,
                OP_DIV => Op::Div,
                OP_REM => Op::Rem,
                OP_ADD1 => Op::Add1,
                OP_SUB1 => Op::Sub1,
                OP_NEGATE => Op::Negate,
                OP_EQLSIGN => Op::Eqlsign,
                OP_GTR => Op::Gtr,
                OP_LSS => Op::Lss,
                OP_LEQ => Op::Leq,
                OP_GEQ => Op::Geq,
                OP_MAX => Op::Max,
                OP_MIN => Op::Min,
                OP_CAR => Op::Car,
                OP_CDR => Op::Cdr,
                OP_CONS => Op::Cons,
                OP_LENGTH => Op::Length,
                OP_NTH => Op::Nth,
                OP_NTHCDR => Op::Nthcdr,
                OP_SETCAR => Op::Setcar,
                OP_SETCDR => Op::Setcdr,
                OP_CAR_SAFE => Op::CarSafe,
                OP_CDR_SAFE => Op::CdrSafe,
                OP_ELT => Op::Elt,
                OP_NCONC => Op::Nconc,
                OP_NREVERSE => Op::Nreverse,
                OP_MEMBER => Op::Member,
                OP_MEMQ => Op::Memq,
                OP_ASSQ => Op::Assq,
                OP_SYMBOLP => Op::Symbolp,
                OP_CONSP => Op::Consp,
                OP_STRINGP => Op::Stringp,
                OP_LISTP => Op::Listp,
                OP_INTEGERP => Op::Integerp,
                OP_NUMBERP => Op::Numberp,
                OP_NULL => Op::Null,
                OP_NOT => Op::Not,
                OP_EQ => Op::Eq,
                OP_EQUAL => Op::Equal,
                OP_SUBSTRING => Op::Substring,
                OP_STRING_EQUAL => Op::StringEqual,
                OP_STRING_LESSP => Op::StringLessp,
                OP_AREF => Op::Aref,
                OP_ASET => Op::Aset,
                OP_SYMBOL_VALUE => Op::SymbolValue,
                OP_SYMBOL_FUNCTION => Op::SymbolFunction,
                OP_SET => Op::Set,
                OP_FSET => Op::Fset,
                OP_GET => Op::Get,
                OP_PUT => Op::Put,
                OP_POP_HANDLER => Op::PopHandler,
                OP_UNWIND_PROTECT_POP => Op::UnwindProtectPop,
                OP_THROW => Op::Throw,
                OP_SAVE_CURRENT_BUFFER => Op::SaveCurrentBuffer,
                OP_SAVE_EXCURSION => Op::SaveExcursion,
                OP_SAVE_RESTRICTION => Op::SaveRestriction,
                OP_SAVE_WINDOW_EXCURSION => Op::SaveWindowExcursion,
                OP_DISCARD_N => Op::DiscardN(arg as u8),
                OP_CALL_BUILTIN => Op::CallBuiltin(arg as u16, extra),
                OP_CALL_BUILTIN_SYM => {
                    Op::CallBuiltinSym(super::convert::load_sym_id(&DumpSymId(arg)), extra)
                }
                other => {
                    return Err(DumpError::ImageFormatError(format!(
                        "unknown bytecode op tag {other}"
                    )));
                }
            };
            ops.push(op);
        }
        Ok(ops)
    }

    fn read_marker(&mut self) -> Result<DumpMarker, DumpError> {
        Ok(DumpMarker {
            buffer: self.read_opt_buffer_id()?,
            insertion_type: self.read_bool("marker insertion type")?,
            marker_id: self.read_opt_u64()?,
            bytepos: self.read_usize("marker byte position")?,
            charpos: self.read_usize("marker char position")?,
            last_position_valid: self.read_bool("marker last_position_valid")?,
        })
    }

    fn read_overlay(&mut self) -> Result<DumpOverlay, DumpError> {
        Ok(DumpOverlay {
            serial: self.read_u64("overlay serial")?,
            plist: self.read_value()?,
            buffer: self.read_opt_buffer_id()?,
            start: self.read_usize("overlay start")?,
            end: self.read_usize("overlay end")?,
            front_advance: self.read_bool("overlay front advance")?,
            rear_advance: self.read_bool("overlay rear advance")?,
        })
    }

    fn read_lisp_string(&mut self) -> Result<DumpLispString, DumpError> {
        Ok(DumpLispString {
            data: self.read_bytes()?,
            size: self.read_usize("lisp string char size")?,
            size_byte: self.read_i64("lisp string byte size")?,
        })
    }

    fn read_opt_lisp_string(&mut self) -> Result<Option<DumpLispString>, DumpError> {
        if self.read_bool("lisp string present")? {
            Ok(Some(self.read_lisp_string()?))
        } else {
            Ok(None)
        }
    }

    fn read_opt_value(&mut self) -> Result<Option<DumpValue>, DumpError> {
        if self.read_bool("value present")? {
            Ok(Some(self.read_value()?))
        } else {
            Ok(None)
        }
    }

    fn read_opt_sym_id(&mut self) -> Result<Option<DumpSymId>, DumpError> {
        if self.read_bool("symbol id present")? {
            Ok(Some(DumpSymId(self.read_u32("symbol id")?)))
        } else {
            Ok(None)
        }
    }

    fn read_opt_buffer_id(&mut self) -> Result<Option<DumpBufferId>, DumpError> {
        if self.read_bool("buffer id present")? {
            Ok(Some(DumpBufferId(self.read_u64("buffer id")?)))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn read_opt_u16(&mut self) -> Result<Option<u16>, DumpError> {
        if self.read_bool("u16 present")? {
            Ok(Some(self.read_u16("u16 option")?))
        } else {
            Ok(None)
        }
    }

    fn read_opt_u64(&mut self) -> Result<Option<u64>, DumpError> {
        if self.read_bool("u64 present")? {
            Ok(Some(self.read_u64("u64 option")?))
        } else {
            Ok(None)
        }
    }

    pub(crate) fn read_string(&mut self) -> Result<String, DumpError> {
        let bytes = self.read_bytes()?;
        String::from_utf8(bytes)
            .map_err(|e| DumpError::ImageFormatError(format!("invalid UTF-8 string: {e}")))
    }

    pub(crate) fn read_bytes(&mut self) -> Result<Vec<u8>, DumpError> {
        let len = self.read_len("byte payload length")?;
        Ok(self.read_exact(len, "byte payload")?.to_vec())
    }

    pub(crate) fn read_bytes_fixed(&mut self, len: usize) -> Result<Vec<u8>, DumpError> {
        Ok(self.read_exact(len, "fixed byte payload")?.to_vec())
    }

    pub(crate) fn read_len(&mut self, what: &str) -> Result<usize, DumpError> {
        let len = self.read_u64(what)?;
        usize::try_from(len)
            .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
    }

    pub(crate) fn read_usize(&mut self, what: &str) -> Result<usize, DumpError> {
        let value = self.read_u64(what)?;
        usize::try_from(value)
            .map_err(|_| DumpError::ImageFormatError(format!("{what} overflows usize")))
    }

    pub(crate) fn read_bool(&mut self, what: &str) -> Result<bool, DumpError> {
        match self.read_u8(what)? {
            0 => Ok(false),
            1 => Ok(true),
            other => Err(DumpError::ImageFormatError(format!(
                "{what} has invalid bool byte {other}"
            ))),
        }
    }

    pub(crate) fn read_u8(&mut self, what: &str) -> Result<u8, DumpError> {
        let start = self.read_fixed_start(1, what)?;
        Ok(self.section[start])
    }

    pub(crate) fn read_u16(&mut self, what: &str) -> Result<u16, DumpError> {
        self.read_unaligned::<u16>(what)
    }

    pub(crate) fn read_u32(&mut self, what: &str) -> Result<u32, DumpError> {
        self.read_unaligned::<u32>(what)
    }

    pub(crate) fn read_u64(&mut self, what: &str) -> Result<u64, DumpError> {
        self.read_unaligned::<u64>(what)
    }

    pub(crate) fn read_i64(&mut self, what: &str) -> Result<i64, DumpError> {
        self.read_unaligned::<i64>(what)
    }

    pub(crate) fn read_f64(&mut self, what: &str) -> Result<f64, DumpError> {
        self.read_unaligned::<f64>(what)
    }

    pub(crate) fn read_exact(&mut self, len: usize, what: &str) -> Result<&'a [u8], DumpError> {
        let start = self.read_fixed_start(len, what)?;
        Ok(&self.section[start..self.offset])
    }

    #[inline]
    fn read_unaligned<T: Copy>(&mut self, what: &str) -> Result<T, DumpError> {
        let start = self.read_fixed_start(std::mem::size_of::<T>(), what)?;
        Ok(unsafe { std::ptr::read_unaligned(self.section.as_ptr().add(start).cast::<T>()) })
    }

    #[inline]
    fn read_fixed_start(&mut self, len: usize, what: &str) -> Result<usize, DumpError> {
        let end = self
            .offset
            .checked_add(len)
            .ok_or_else(|| DumpError::ImageFormatError(format!("{what} read range overflows")))?;
        if end > self.section.len() {
            return Err(DumpError::ImageFormatError(format!(
                "{what} extends past object codec payload"
            )));
        }
        let start = self.offset;
        self.offset = end;
        Ok(start)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heap_object_codec_round_trips_representative_objects() {
        let objects = vec![
            DumpHeapObject::Str {
                data: DumpByteData::mapped(24, 3),
                size: 3,
                size_byte: 3,
                text_props: vec![DumpStringTextPropertyRun {
                    start: 0,
                    end: 1,
                    plist: DumpValue::Symbol(DumpSymId(7)),
                }],
            },
            DumpHeapObject::Str {
                data: DumpByteData::static_rodata(0xfeed_beef, 6),
                size: 6,
                size_byte: -2,
                text_props: Vec::new(),
            },
            DumpHeapObject::Cons {
                car: DumpValue::Int(42),
                cdr: DumpValue::Str(DumpHeapRef { index: 0 }),
            },
            DumpHeapObject::ByteCode(DumpByteCodeFunction {
                instructions: DumpByteCodeInstructions::Gnu(DumpByteData::owned(vec![0xC0, 0x87])),
                constants: vec![DumpValue::Bignum("12345678901234567890".into())],
                max_stack: 4,
                params: DumpLambdaParams {
                    required: vec![DumpSymId(1)],
                    optional: vec![DumpSymId(2)],
                    rest: Some(DumpSymId(3)),
                },
                arglist: Some(DumpValue::Nil),
                lexical: true,
                env: Some(DumpValue::Vector(DumpHeapRef { index: 4 })),
                docstring: Some(DumpLispString {
                    data: b"doc".to_vec(),
                    size: 3,
                    size_byte: 3,
                }),
                doc_form: Some(DumpValue::True),
                interactive: Some(DumpValue::Nil),
                closure_slot_count: 6,
                extra_slots: vec![],
                ops_sealed: false,
            }),
            DumpHeapObject::ByteCode(DumpByteCodeFunction {
                instructions: DumpByteCodeInstructions::Decoded(vec![
                    Op::Constant(1),
                    // CallBuiltinSym needs the load-time symbol remap TLS
                    // (installed by real loads); CallBuiltin has the same
                    // record shape and keeps the tag+arg+extra encode covered.
                    Op::CallBuiltin(9, 2),
                    Op::Return,
                ]),
                constants: vec![DumpValue::Bignum("12345678901234567890".into())],
                max_stack: 4,
                params: DumpLambdaParams {
                    required: vec![DumpSymId(1)],
                    optional: vec![DumpSymId(2)],
                    rest: Some(DumpSymId(3)),
                },
                arglist: Some(DumpValue::Nil),
                lexical: true,
                env: Some(DumpValue::Vector(DumpHeapRef { index: 4 })),
                docstring: Some(DumpLispString {
                    data: b"doc".to_vec(),
                    size: 3,
                    size_byte: 3,
                }),
                doc_form: Some(DumpValue::True),
                interactive: Some(DumpValue::Nil),
                closure_slot_count: 6,
                extra_slots: vec![],
                ops_sealed: false,
            }),
            DumpHeapObject::HashTable(DumpLispHashTable {
                test: DumpHashTableTest::Equal,
                test_name: Some(DumpSymId(11)),
                size: 17,
                weakness: Some(DumpHashTableWeakness::KeyOrValue),
                rehash_size: 1.5,
                rehash_threshold: 0.8,
                ordered_entries: vec![
                    (
                        DumpHashKey::EqualCons(
                            Box::new(DumpHashKey::Text("a".into())),
                            Box::new(DumpHashKey::Cycle(1)),
                        ),
                        DumpValue::Cons(DumpHeapRef { index: 1 }),
                        Some(DumpValue::Int(8)),
                    ),
                    (
                        DumpHashKey::Bignum(vec![0, 0xFFFF_FFFF_FFFF_FFFF, 1]),
                        DumpValue::Int(9),
                        None,
                    ),
                    (
                        DumpHashKey::ByteCode(vec![
                            DumpByteCodeKeyPart::ObservableSlotCount(5),
                            DumpByteCodeKeyPart::Value(DumpHashKey::Int(257)),
                            DumpByteCodeKeyPart::Bytes(vec![0xC0, 0x87]),
                            DumpByteCodeKeyPart::Ops(vec![Op::Constant(0), Op::Return]),
                            DumpByteCodeKeyPart::Values(vec![DumpHashKey::Int(42)]),
                            DumpByteCodeKeyPart::Text {
                                char_count: 3,
                                bytes: b"doc".to_vec(),
                            },
                            DumpByteCodeKeyPart::Absent,
                        ]),
                        DumpValue::Int(10),
                        None,
                    ),
                    (DumpHashKey::Char('x'), DumpValue::Int(8), None),
                    (DumpHashKey::HeapRef(1), DumpValue::True, None),
                ],
            }),
            DumpHeapObject::Marker(DumpMarker {
                buffer: Some(DumpBufferId(5)),
                insertion_type: true,
                marker_id: Some(6),
                bytepos: 7,
                charpos: 8,
                last_position_valid: true,
            }),
            DumpHeapObject::Overlay(DumpOverlay {
                serial: 17,
                plist: DumpValue::Nil,
                buffer: Some(DumpBufferId(9)),
                start: 10,
                end: 11,
                front_advance: true,
                rear_advance: false,
            }),
            DumpHeapObject::Subr {
                name: DumpNameId(13),
                min_args: 1,
                max_args: Some(2),
            },
        ];

        let mut bytes = Vec::new();
        for object in &objects {
            write_heap_object(&mut bytes, object).expect("encode heap object");
        }
        let mut cursor = Cursor::new(&bytes);
        let mut decoded = Vec::new();
        for _ in 0..objects.len() {
            decoded.push(cursor.read_heap_object().expect("decode heap object"));
        }
        assert!(cursor.is_empty());

        assert_eq!(format!("{decoded:?}"), format!("{objects:?}"));
    }

    #[test]
    fn heap_object_codec_rejects_bad_tag() {
        let bytes = [u8::MAX];
        let mut cursor = Cursor::new(&bytes);
        let err = cursor
            .read_heap_object()
            .expect_err("bad object tag should fail");
        assert!(matches!(err, DumpError::ImageFormatError(_)));
    }
}
