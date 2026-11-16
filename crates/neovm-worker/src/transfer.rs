//! The copy boundary between isolates (RFC §8.2, transfer mode `Copy`).
//!
//! A `Value` is a word of one heap and never crosses threads. A task's
//! function, its arguments and its result are therefore walked into a
//! [`Transfer`] tree that owns no heap state, serialized into the opaque
//! [`neovm_host_abi::LispValue`] bytes the worker queue already carries, and
//! rebuilt as fresh objects in the receiving Context's own heap.
//!
//! What crosses: nil, t, integers (fixnums and bignums), floats, strings
//! (bytes and the multibyte flag; text properties are dropped), symbols (by
//! name, re-interned on the other side), conses (proper and dotted lists),
//! vectors, records, bool-vectors, hash tables with a standard test,
//! interpreted closures (all six slots, so a captured lexical environment is
//! copied with them), byte-code objects (through their printed `#[...]`
//! literal) and subr objects (by name). Everything that names host state --
//! buffers, markers, overlays, windows, frames, processes, terminals,
//! threads, module functions, user pointers and the rest -- is refused with
//! `neomacs-worker-transfer-error`, as is a circular structure. Shared
//! substructure is duplicated, not preserved.

use neovm_core::emacs_core::bytecode::value_api::TaggedValueBytecodeExt;
use neovm_core::emacs_core::eval::Context;
use neovm_core::emacs_core::print::print_value_with_eval;
use neovm_core::emacs_core::value::{HashTableTest, HashTableWeakness, VecLikeType};
use neovm_core::emacs_core::value_reader::read_all;
use neovm_core::heap_types::LispString;
use neovm_core::{Value, ValueKind};
use std::collections::HashSet;

/// Deepest nesting the walk follows before refusing the value. Lists grow
/// along the cdr without consuming depth; only car and element nesting does.
const MAX_DEPTH: usize = 4096;

/// A heap-free deep copy of one Lisp value.
#[derive(Clone, Debug, PartialEq)]
pub enum Transfer {
    Nil,
    T,
    Fixnum(i64),
    /// Decimal digits with an optional leading `-`.
    Bignum(String),
    /// IEEE bits, so the copy is exact.
    Float(u64),
    Str {
        bytes: Vec<u8>,
        multibyte: bool,
    },
    /// Interned by name on the receiving side.
    Symbol(String),
    /// `items` along the cdr chain; `tail` is the final cdr when the list is
    /// dotted, `None` for a proper list.
    List {
        items: Vec<Transfer>,
        tail: Option<Box<Transfer>>,
    },
    Vector(Vec<Transfer>),
    /// Slot 0 is the record type.
    Record(Vec<Transfer>),
    BoolVector {
        nbits: u64,
        words: Vec<u64>,
    },
    HashTable {
        test: HashTableTest,
        weakness: Option<HashTableWeakness>,
        entries: Vec<(Transfer, Transfer)>,
    },
    /// The six slots of an interpreted function object.
    Closure(Vec<Transfer>),
    /// A byte-code object's printed `#[...]` literal.
    ByteCode(String),
    /// A primitive, by the name of the symbol it is bound to.
    Subr(String),
}

/// One task's call: the function and the arguments it is applied to.
#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub function: Transfer,
    pub args: Vec<Transfer>,
}

/// Why a value could not cross the boundary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TransferError {
    /// The value's `type-of` name.
    Unsupported(&'static str),
    Circular,
    TooDeep,
    UserHashTableTest,
    /// A byte-code literal that the receiving reader rejected.
    ByteCodeLiteral(String),
    /// Malformed wire bytes.
    Decode(String),
}

impl TransferError {
    /// The signal data for `neomacs-worker-transfer-error`: a reason symbol
    /// followed by the detail, mirroring GNU's `(error-symbol . data)` shape.
    pub fn signal_data(&self) -> Vec<Value> {
        match self {
            Self::Unsupported(type_name) => vec![
                Value::symbol("unsupported-object"),
                Value::symbol(type_name),
            ],
            Self::Circular => vec![Value::symbol("circular-object")],
            Self::TooDeep => vec![Value::symbol("too-deep")],
            Self::UserHashTableTest => vec![Value::symbol("user-defined-hash-table-test")],
            Self::ByteCodeLiteral(message) => vec![
                Value::symbol("byte-code-literal"),
                Value::string(message.as_str()),
            ],
            Self::Decode(message) => {
                vec![
                    Value::symbol("malformed-wire"),
                    Value::string(message.as_str()),
                ]
            }
        }
    }
}

impl std::fmt::Display for TransferError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported(type_name) => write!(f, "cannot copy a {type_name} into a worker"),
            Self::Circular => f.write_str("cannot copy a circular object into a worker"),
            Self::TooDeep => f.write_str("object nests too deeply to copy into a worker"),
            Self::UserHashTableTest => {
                f.write_str("cannot copy a hash table with a user-defined test")
            }
            Self::ByteCodeLiteral(message) => write!(f, "byte-code literal: {message}"),
            Self::Decode(message) => write!(f, "malformed transfer bytes: {message}"),
        }
    }
}

impl std::error::Error for TransferError {}

/// GNU `cl-type-of` names for the heap kinds the boundary refuses.
fn refused_type_name(kind: ValueKind) -> &'static str {
    match kind {
        ValueKind::Veclike(veclike) => match veclike {
            VecLikeType::Marker => "marker",
            VecLikeType::Overlay => "overlay",
            VecLikeType::Finalizer => "finalizer",
            VecLikeType::SymbolWithPos => "symbol-with-pos",
            VecLikeType::UserPtr => "user-ptr",
            VecLikeType::Process => "process",
            VecLikeType::Frame => "frame",
            VecLikeType::Window => "window",
            VecLikeType::Buffer => "buffer",
            VecLikeType::Obarray => "obarray",
            VecLikeType::Terminal => "terminal",
            VecLikeType::WindowConfiguration => "window-configuration",
            VecLikeType::Xwidget => "xwidget",
            VecLikeType::XwidgetView => "xwidget-view",
            VecLikeType::Thread => "thread",
            VecLikeType::Mutex => "mutex",
            VecLikeType::CondVar => "condition-variable",
            VecLikeType::ModuleFunction => "module-function",
            VecLikeType::TsNode => "treesit-node",
            VecLikeType::Sqlite => "sqlite",
            VecLikeType::CharTable => "char-table",
            VecLikeType::SubCharTable => "sub-char-table",
            VecLikeType::Font => "font-object",
            VecLikeType::Macro => "macro",
            VecLikeType::Timer => "timer",
            VecLikeType::SurfaceHandle => "neomacs-surface",
            VecLikeType::VideoHandle => "neomacs-video",
            VecLikeType::Vector
            | VecLikeType::Bignum
            | VecLikeType::BoolVector
            | VecLikeType::HashTable
            | VecLikeType::Subr
            | VecLikeType::Lambda
            | VecLikeType::Record
            | VecLikeType::ByteCode => "object",
        },
        ValueKind::Unbound => "unbound",
        ValueKind::Unknown => "unknown",
        ValueKind::Nil
        | ValueKind::T
        | ValueKind::Fixnum(_)
        | ValueKind::Symbol(_)
        | ValueKind::Cons
        | ValueKind::String
        | ValueKind::Float
        | ValueKind::Subr(_) => "object",
    }
}

/// The walk from one heap into a [`Transfer`] tree.
struct Walker<'a> {
    ctx: &'a Context,
    /// Heap addresses of the containers currently being walked. A value
    /// that reaches one of them again is a cycle.
    in_progress: HashSet<usize>,
}

impl Walker<'_> {
    fn enter(&mut self, value: Value) -> Result<usize, TransferError> {
        let key = value.heap_ptr().map(|p| p as usize).unwrap_or(0);
        if key != 0 && !self.in_progress.insert(key) {
            return Err(TransferError::Circular);
        }
        Ok(key)
    }

    fn leave(&mut self, key: usize) {
        if key != 0 {
            self.in_progress.remove(&key);
        }
    }

    fn walk(&mut self, value: Value, depth: usize) -> Result<Transfer, TransferError> {
        if depth > MAX_DEPTH {
            return Err(TransferError::TooDeep);
        }
        let kind = value.kind();
        match kind {
            ValueKind::Nil => Ok(Transfer::Nil),
            ValueKind::T => Ok(Transfer::T),
            ValueKind::Fixnum(n) => Ok(Transfer::Fixnum(n)),
            ValueKind::Float => Ok(Transfer::Float(
                value.as_float().unwrap_or(f64::NAN).to_bits(),
            )),
            ValueKind::String => {
                let string = value
                    .as_lisp_string()
                    .ok_or(TransferError::Unsupported("string"))?;
                Ok(Transfer::Str {
                    bytes: string.as_bytes().to_vec(),
                    multibyte: string.is_multibyte(),
                })
            }
            ValueKind::Symbol(_) => value
                .as_bare_symbol_name()
                .map(|name| Transfer::Symbol(name.to_string()))
                .ok_or(TransferError::Unsupported("symbol")),
            ValueKind::Subr(id) => Ok(Transfer::Subr(
                neovm_core::emacs_core::intern::resolve_sym(id).to_string(),
            )),
            ValueKind::Veclike(VecLikeType::Subr) => value
                .as_subr_id()
                .map(|id| {
                    Transfer::Subr(neovm_core::emacs_core::intern::resolve_sym(id).to_string())
                })
                .ok_or(TransferError::Unsupported("primitive-function")),
            ValueKind::Cons => self.walk_list(value, depth),
            ValueKind::Veclike(VecLikeType::Bignum) => value
                .as_bignum()
                .map(|n| Transfer::Bignum(n.to_string()))
                .ok_or(TransferError::Unsupported("bignum")),
            ValueKind::Veclike(VecLikeType::Vector) => {
                let key = self.enter(value)?;
                let data = value
                    .as_vector_data()
                    .ok_or(TransferError::Unsupported("vector"))?;
                let items = self.walk_slice(data, depth);
                self.leave(key);
                Ok(Transfer::Vector(items?))
            }
            ValueKind::Veclike(VecLikeType::Record) => {
                let key = self.enter(value)?;
                let data = value
                    .as_record_data()
                    .ok_or(TransferError::Unsupported("record"))?;
                let items = self.walk_slice(data, depth);
                self.leave(key);
                Ok(Transfer::Record(items?))
            }
            ValueKind::Veclike(VecLikeType::Lambda) => {
                let key = self.enter(value)?;
                let slots = value
                    .closure_slots()
                    .ok_or(TransferError::Unsupported("interpreted-function"))?;
                let items = self.walk_slice(slots, depth);
                self.leave(key);
                Ok(Transfer::Closure(items?))
            }
            ValueKind::Veclike(VecLikeType::BoolVector) => {
                let bv = value
                    .as_bool_vector_obj()
                    .ok_or(TransferError::Unsupported("bool-vector"))?;
                Ok(Transfer::BoolVector {
                    nbits: bv.nbits as u64,
                    words: bv.words().to_vec(),
                })
            }
            ValueKind::Veclike(VecLikeType::HashTable) => {
                let key = self.enter(value)?;
                let table = value
                    .as_hash_table()
                    .ok_or(TransferError::Unsupported("hash-table"))?;
                let result = if table.test_name.is_some() {
                    Err(TransferError::UserHashTableTest)
                } else {
                    let mut entries = Vec::new();
                    let mut failure = None;
                    for entry in table.entries_in_slot_order() {
                        match (
                            self.walk(entry.key, depth + 1),
                            self.walk(entry.value, depth + 1),
                        ) {
                            (Ok(k), Ok(v)) => entries.push((k, v)),
                            (Err(err), _) | (_, Err(err)) => {
                                failure = Some(err);
                                break;
                            }
                        }
                    }
                    match failure {
                        Some(err) => Err(err),
                        None => Ok(Transfer::HashTable {
                            test: table.test,
                            weakness: table.weakness,
                            entries,
                        }),
                    }
                };
                self.leave(key);
                result
            }
            ValueKind::Veclike(VecLikeType::ByteCode) => {
                let key = self.enter(value)?;
                // The constants pool is the only slot that can hold host
                // objects; validate it before trusting the printed literal.
                let result = match value.get_bytecode_data() {
                    Some(bytecode) => {
                        let mut checked = Ok(());
                        for constant in bytecode.constants.iter().copied() {
                            if let Err(err) = self.walk(constant, depth + 1) {
                                checked = Err(err);
                                break;
                            }
                        }
                        checked
                            .map(|()| Transfer::ByteCode(print_value_with_eval(self.ctx, &value)))
                    }
                    None => Err(TransferError::Unsupported("byte-code-function")),
                };
                self.leave(key);
                result
            }
            other => Err(TransferError::Unsupported(refused_type_name(other))),
        }
    }

    fn walk_slice(
        &mut self,
        slice: &[Value],
        depth: usize,
    ) -> Result<Vec<Transfer>, TransferError> {
        slice
            .iter()
            .map(|item| self.walk(*item, depth + 1))
            .collect()
    }

    /// Walk a cons chain. The head is an in-progress container so a car that
    /// points back at it is a cycle; the spine itself is checked with Brent's
    /// algorithm, which costs no memory for long lists.
    fn walk_list(&mut self, head: Value, depth: usize) -> Result<Transfer, TransferError> {
        let key = self.enter(head)?;
        let result = (|| {
            let mut items = Vec::new();
            let mut cell = head;
            let mut tortoise = head;
            let mut power = 1usize;
            let mut lambda = 1usize;
            loop {
                items.push(self.walk(cell.cons_car(), depth + 1)?);
                let next = cell.cons_cdr();
                match next.kind() {
                    ValueKind::Nil => return Ok(Transfer::List { items, tail: None }),
                    ValueKind::Cons => {
                        if next.bits() == tortoise.bits() {
                            return Err(TransferError::Circular);
                        }
                        if power == lambda {
                            tortoise = next;
                            power *= 2;
                            lambda = 0;
                        }
                        lambda += 1;
                        cell = next;
                    }
                    _ => {
                        let tail = self.walk(next, depth + 1)?;
                        return Ok(Transfer::List {
                            items,
                            tail: Some(Box::new(tail)),
                        });
                    }
                }
            }
        })();
        self.leave(key);
        result
    }
}

/// Copy `value` out of `ctx`'s heap.
pub fn from_value(ctx: &Context, value: Value) -> Result<Transfer, TransferError> {
    let mut walker = Walker {
        ctx,
        in_progress: HashSet::new(),
    };
    walker.walk(value, 0)
}

/// Rebuild `transfer` as fresh objects in `ctx`'s heap. Nothing here reaches
/// a GC safepoint: the constructors only allocate, the byte-code reader
/// materializes without evaluating, and a standard-test `puthash` runs no
/// Lisp, so the partial structure needs no rooting while it is built.
pub fn to_value(ctx: &mut Context, transfer: &Transfer) -> Result<Value, TransferError> {
    match transfer {
        Transfer::Nil => Ok(Value::NIL),
        Transfer::T => Ok(Value::T),
        Transfer::Fixnum(n) => Ok(Value::fixnum(*n)),
        Transfer::Bignum(digits) => Ok(Value::make_integer_from_str_or_zero(digits)),
        Transfer::Float(bits) => Ok(Value::make_float(f64::from_bits(*bits))),
        Transfer::Str { bytes, multibyte } => {
            let string = if *multibyte {
                LispString::from_emacs_bytes(bytes.clone())
            } else {
                LispString::from_unibyte(bytes.clone())
            };
            Ok(Value::heap_string(string))
        }
        Transfer::Symbol(name) => Ok(Value::symbol(name.as_str())),
        Transfer::Subr(name) => Ok(Value::symbol(name.as_str())),
        Transfer::List { items, tail } => {
            let mut result = match tail {
                Some(tail) => to_value(ctx, tail)?,
                None => Value::NIL,
            };
            for item in items.iter().rev() {
                let car = to_value(ctx, item)?;
                result = Value::cons(car, result);
            }
            Ok(result)
        }
        Transfer::Vector(items) => Ok(Value::vector(to_values(ctx, items)?)),
        Transfer::Record(items) => Ok(Value::make_record(to_values(ctx, items)?)),
        Transfer::Closure(slots) => Ok(Value::make_lambda_with_slots(to_values(ctx, slots)?)),
        Transfer::BoolVector { nbits, words } => {
            Ok(Value::make_bool_vector(*nbits as usize, words.clone()))
        }
        Transfer::HashTable {
            test,
            weakness,
            entries,
        } => {
            let table =
                Value::hash_table_with_options(*test, entries.len() as i64, *weakness, 1.5, 0.8125);
            for (key, value) in entries {
                let key = to_value(ctx, key)?;
                let value = to_value(ctx, value)?;
                neovm_core::emacs_core::builtins::builtin_puthash_3(ctx, key, value, table)
                    .map_err(|flow| TransferError::Decode(format!("puthash: {flow:?}")))?;
            }
            Ok(table)
        }
        Transfer::ByteCode(literal) => {
            let forms = read_all(literal, ctx.variables())
                .map_err(|err| TransferError::ByteCodeLiteral(err.message))?;
            forms
                .into_iter()
                .next()
                .ok_or_else(|| TransferError::ByteCodeLiteral("empty literal".to_string()))
        }
    }
}

fn to_values(ctx: &mut Context, items: &[Transfer]) -> Result<Vec<Value>, TransferError> {
    items.iter().map(|item| to_value(ctx, item)).collect()
}

// ---------------------------------------------------------------------------
// Wire format: one tag byte, then little-endian lengths and payloads.

const TAG_NIL: u8 = 0;
const TAG_T: u8 = 1;
const TAG_FIXNUM: u8 = 2;
const TAG_BIGNUM: u8 = 3;
const TAG_FLOAT: u8 = 4;
const TAG_STR: u8 = 5;
const TAG_SYMBOL: u8 = 6;
const TAG_LIST: u8 = 7;
const TAG_VECTOR: u8 = 8;
const TAG_RECORD: u8 = 9;
const TAG_BOOL_VECTOR: u8 = 10;
const TAG_HASH_TABLE: u8 = 11;
const TAG_CLOSURE: u8 = 12;
const TAG_BYTE_CODE: u8 = 13;
const TAG_SUBR: u8 = 14;
const CALL_MAGIC: &[u8; 4] = b"NWC1";

/// Serialize one value.
pub fn encode(transfer: &Transfer) -> Vec<u8> {
    let mut out = Vec::new();
    write_transfer(&mut out, transfer);
    out
}

/// Serialize a call.
pub fn encode_call(call: &Call) -> Vec<u8> {
    let mut out = Vec::from(*CALL_MAGIC);
    write_transfer(&mut out, &call.function);
    write_len(&mut out, call.args.len());
    for arg in &call.args {
        write_transfer(&mut out, arg);
    }
    out
}

/// Parse one value.
pub fn decode(bytes: &[u8]) -> Result<Transfer, TransferError> {
    let mut reader = Reader { bytes, at: 0 };
    let value = reader.transfer()?;
    reader.finish()?;
    Ok(value)
}

/// Parse a call.
pub fn decode_call(bytes: &[u8]) -> Result<Call, TransferError> {
    let mut reader = Reader { bytes, at: 0 };
    if reader.take(CALL_MAGIC.len())? != CALL_MAGIC {
        return Err(TransferError::Decode("not a call".to_string()));
    }
    let function = reader.transfer()?;
    let count = reader.len()?;
    let mut args = Vec::with_capacity(count.min(1024));
    for _ in 0..count {
        args.push(reader.transfer()?);
    }
    reader.finish()?;
    Ok(Call { function, args })
}

fn write_len(out: &mut Vec<u8>, len: usize) {
    out.extend_from_slice(&(len as u64).to_le_bytes());
}

fn write_bytes(out: &mut Vec<u8>, bytes: &[u8]) {
    write_len(out, bytes.len());
    out.extend_from_slice(bytes);
}

fn write_items(out: &mut Vec<u8>, items: &[Transfer]) {
    write_len(out, items.len());
    for item in items {
        write_transfer(out, item);
    }
}

fn write_transfer(out: &mut Vec<u8>, transfer: &Transfer) {
    match transfer {
        Transfer::Nil => out.push(TAG_NIL),
        Transfer::T => out.push(TAG_T),
        Transfer::Fixnum(n) => {
            out.push(TAG_FIXNUM);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Transfer::Bignum(digits) => {
            out.push(TAG_BIGNUM);
            write_bytes(out, digits.as_bytes());
        }
        Transfer::Float(bits) => {
            out.push(TAG_FLOAT);
            out.extend_from_slice(&bits.to_le_bytes());
        }
        Transfer::Str { bytes, multibyte } => {
            out.push(TAG_STR);
            out.push(u8::from(*multibyte));
            write_bytes(out, bytes);
        }
        Transfer::Symbol(name) => {
            out.push(TAG_SYMBOL);
            write_bytes(out, name.as_bytes());
        }
        Transfer::Subr(name) => {
            out.push(TAG_SUBR);
            write_bytes(out, name.as_bytes());
        }
        Transfer::List { items, tail } => {
            out.push(TAG_LIST);
            write_items(out, items);
            match tail {
                Some(tail) => {
                    out.push(1);
                    write_transfer(out, tail);
                }
                None => out.push(0),
            }
        }
        Transfer::Vector(items) => {
            out.push(TAG_VECTOR);
            write_items(out, items);
        }
        Transfer::Record(items) => {
            out.push(TAG_RECORD);
            write_items(out, items);
        }
        Transfer::Closure(items) => {
            out.push(TAG_CLOSURE);
            write_items(out, items);
        }
        Transfer::BoolVector { nbits, words } => {
            out.push(TAG_BOOL_VECTOR);
            out.extend_from_slice(&nbits.to_le_bytes());
            write_len(out, words.len());
            for word in words {
                out.extend_from_slice(&word.to_le_bytes());
            }
        }
        Transfer::HashTable {
            test,
            weakness,
            entries,
        } => {
            out.push(TAG_HASH_TABLE);
            out.push(match test {
                HashTableTest::Eq => 0,
                HashTableTest::Eql => 1,
                HashTableTest::Equal => 2,
            });
            out.push(match weakness {
                None => 0,
                Some(HashTableWeakness::Key) => 1,
                Some(HashTableWeakness::Value) => 2,
                Some(HashTableWeakness::KeyOrValue) => 3,
                Some(HashTableWeakness::KeyAndValue) => 4,
            });
            write_len(out, entries.len());
            for (key, value) in entries {
                write_transfer(out, key);
                write_transfer(out, value);
            }
        }
        Transfer::ByteCode(literal) => {
            out.push(TAG_BYTE_CODE);
            write_bytes(out, literal.as_bytes());
        }
    }
}

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], TransferError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|end| *end <= self.bytes.len())
            .ok_or_else(|| TransferError::Decode("truncated".to_string()))?;
        let slice = &self.bytes[self.at..end];
        self.at = end;
        Ok(slice)
    }

    fn byte(&mut self) -> Result<u8, TransferError> {
        Ok(self.take(1)?[0])
    }

    fn u64(&mut self) -> Result<u64, TransferError> {
        let mut raw = [0u8; 8];
        raw.copy_from_slice(self.take(8)?);
        Ok(u64::from_le_bytes(raw))
    }

    fn len(&mut self) -> Result<usize, TransferError> {
        let len = self.u64()?;
        usize::try_from(len)
            .ok()
            .filter(|len| *len <= self.bytes.len())
            .ok_or_else(|| TransferError::Decode("length out of range".to_string()))
    }

    fn bytes(&mut self) -> Result<Vec<u8>, TransferError> {
        let len = self.len()?;
        Ok(self.take(len)?.to_vec())
    }

    fn string(&mut self) -> Result<String, TransferError> {
        String::from_utf8(self.bytes()?)
            .map_err(|err| TransferError::Decode(format!("invalid utf-8: {err}")))
    }

    fn items(&mut self) -> Result<Vec<Transfer>, TransferError> {
        let count = self.len()?;
        let mut items = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            items.push(self.transfer()?);
        }
        Ok(items)
    }

    fn transfer(&mut self) -> Result<Transfer, TransferError> {
        Ok(match self.byte()? {
            TAG_NIL => Transfer::Nil,
            TAG_T => Transfer::T,
            TAG_FIXNUM => Transfer::Fixnum(self.u64()? as i64),
            TAG_BIGNUM => Transfer::Bignum(self.string()?),
            TAG_FLOAT => Transfer::Float(self.u64()?),
            TAG_STR => {
                let multibyte = self.byte()? != 0;
                Transfer::Str {
                    bytes: self.bytes()?,
                    multibyte,
                }
            }
            TAG_SYMBOL => Transfer::Symbol(self.string()?),
            TAG_SUBR => Transfer::Subr(self.string()?),
            TAG_LIST => {
                let items = self.items()?;
                let tail = match self.byte()? {
                    0 => None,
                    _ => Some(Box::new(self.transfer()?)),
                };
                Transfer::List { items, tail }
            }
            TAG_VECTOR => Transfer::Vector(self.items()?),
            TAG_RECORD => Transfer::Record(self.items()?),
            TAG_CLOSURE => Transfer::Closure(self.items()?),
            TAG_BOOL_VECTOR => {
                let nbits = self.u64()?;
                let count = self.len()?;
                let mut words = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    words.push(self.u64()?);
                }
                Transfer::BoolVector { nbits, words }
            }
            TAG_HASH_TABLE => {
                let test = match self.byte()? {
                    0 => HashTableTest::Eq,
                    1 => HashTableTest::Eql,
                    2 => HashTableTest::Equal,
                    other => {
                        return Err(TransferError::Decode(format!("hash test {other}")));
                    }
                };
                let weakness = match self.byte()? {
                    0 => None,
                    1 => Some(HashTableWeakness::Key),
                    2 => Some(HashTableWeakness::Value),
                    3 => Some(HashTableWeakness::KeyOrValue),
                    4 => Some(HashTableWeakness::KeyAndValue),
                    other => {
                        return Err(TransferError::Decode(format!("hash weakness {other}")));
                    }
                };
                let count = self.len()?;
                let mut entries = Vec::with_capacity(count.min(4096));
                for _ in 0..count {
                    let key = self.transfer()?;
                    let value = self.transfer()?;
                    entries.push((key, value));
                }
                Transfer::HashTable {
                    test,
                    weakness,
                    entries,
                }
            }
            TAG_BYTE_CODE => Transfer::ByteCode(self.string()?),
            other => return Err(TransferError::Decode(format!("tag {other}"))),
        })
    }

    fn finish(&self) -> Result<(), TransferError> {
        if self.at == self.bytes.len() {
            Ok(())
        } else {
            Err(TransferError::Decode("trailing bytes".to_string()))
        }
    }
}
