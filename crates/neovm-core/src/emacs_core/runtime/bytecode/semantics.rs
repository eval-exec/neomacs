//! Structural equality, `equal`-table keys and `sxhash` for byte-code
//! functions.
//!
//! GNU compares a `PVEC_CLOSURE` like the pseudovector it is and hashes its
//! slots (`src/fns.c` `internal_equal_1` / `sxhash_obj`), so these walks must
//! read [`ByteCodeFunction::structural_parts`]. They live with the byte-code
//! representation; the value and hash-table layers reach them through the
//! cold [`BytecodeSemantics`] registry (byte-code values only), which keeps
//! ordinary `equal`/sxhash paths free of the dispatch.

use super::{ByteCodeFunction, ByteCodeStructuralPart, Op};
use crate::emacs_core::error::Flow;
use crate::emacs_core::hashtab::{
    SXHASH_FALLBACK_NONNEG_MASK, SXHASH_MAX_LEN, emacs_hash_char_array,
    emacs_sxhash_obj_with_fallback, sxhash_combine,
};
use crate::emacs_core::value::{
    ByteCodeKeyPart, BytecodeSemantics, EqualKind, EqualSeenPair, HashKey, Value,
    equal_value_inner, install_bytecode_semantics, try_equal_value_inner,
};
use crate::heap_types::LispString;
use crate::tagged::SymbolMode;
use std::collections::HashSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

pub(crate) fn install() {
    install_bytecode_semantics(BytecodeSemantics {
        to_equal_key: bytecode_to_equal_key,
        equal: bytecode_equal,
        try_equal: try_bytecode_equal,
        sxhash: emacs_sxhash_bytecode,
    });
}

/// One tag per [`Op`] variant. `#[repr(u8)]` plus the implicit sequential
/// discriminants makes the tag set injective by construction (89 variants),
/// so a packed key word cannot alias two different instructions.
#[repr(u8)]
#[derive(Clone, Copy)]
enum OpKeyTag {
    Constant,
    Nil,
    True,
    Pop,
    Dup,
    StackRef,
    StackSet,
    DiscardN,
    VarRef,
    VarSet,
    VarBind,
    Unbind,
    Call,
    Apply,
    Goto,
    GotoIfNil,
    GotoIfNotNil,
    GotoIfNilElsePop,
    GotoIfNotNilElsePop,
    Switch,
    Return,
    Add,
    Sub,
    Mul,
    Div,
    Rem,
    Add1,
    Sub1,
    Negate,
    Eqlsign,
    Gtr,
    Lss,
    Leq,
    Geq,
    Max,
    Min,
    Car,
    Cdr,
    Cons,
    List,
    Length,
    Nth,
    Nthcdr,
    Setcar,
    Setcdr,
    CarSafe,
    CdrSafe,
    Elt,
    Nconc,
    Nreverse,
    Member,
    Memq,
    Assq,
    Symbolp,
    Consp,
    Stringp,
    Listp,
    Integerp,
    Numberp,
    Null,
    Not,
    Eq,
    Equal,
    Concat,
    Substring,
    StringEqual,
    StringLessp,
    Aref,
    Aset,
    SymbolValue,
    SymbolFunction,
    Set,
    Fset,
    Get,
    Put,
    PushConditionCase,
    PushConditionCaseRaw,
    PushCatch,
    PopHandler,
    UnwindProtectPop,
    Throw,
    SaveCurrentBuffer,
    SaveExcursion,
    SaveRestriction,
    SaveWindowExcursion,
    MakeClosure,
    CallBuiltin,
    CallBuiltinSym,
    TrapOutOfRangeConstant,
}

/// Lossless per-instruction key word: tag in bits 0..8, first operand in
/// bits 8..40, second in bits 40..48. Every operand type is `u16`/`u8`/`u32`
/// (or `SymId(u32)`), so the packing is injective; equality of packed words
/// is exactly equality of the decoded instructions.
fn op_key_word(op: &Op) -> u64 {
    let (tag, a, b) = match op {
        Op::Constant(n) => (OpKeyTag::Constant, u32::from(*n), 0),
        Op::Nil => (OpKeyTag::Nil, 0, 0),
        Op::True => (OpKeyTag::True, 0, 0),
        Op::Pop => (OpKeyTag::Pop, 0, 0),
        Op::Dup => (OpKeyTag::Dup, 0, 0),
        Op::StackRef(n) => (OpKeyTag::StackRef, u32::from(*n), 0),
        Op::StackSet(n) => (OpKeyTag::StackSet, u32::from(*n), 0),
        Op::DiscardN(n) => (OpKeyTag::DiscardN, u32::from(*n), 0),
        Op::VarRef(n) => (OpKeyTag::VarRef, u32::from(*n), 0),
        Op::VarSet(n) => (OpKeyTag::VarSet, u32::from(*n), 0),
        Op::VarBind(n) => (OpKeyTag::VarBind, u32::from(*n), 0),
        Op::Unbind(n) => (OpKeyTag::Unbind, u32::from(*n), 0),
        Op::Call(n) => (OpKeyTag::Call, u32::from(*n), 0),
        Op::Apply(n) => (OpKeyTag::Apply, u32::from(*n), 0),
        Op::Goto(n) => (OpKeyTag::Goto, *n, 0),
        Op::GotoIfNil(n) => (OpKeyTag::GotoIfNil, *n, 0),
        Op::GotoIfNotNil(n) => (OpKeyTag::GotoIfNotNil, *n, 0),
        Op::GotoIfNilElsePop(n) => (OpKeyTag::GotoIfNilElsePop, *n, 0),
        Op::GotoIfNotNilElsePop(n) => (OpKeyTag::GotoIfNotNilElsePop, *n, 0),
        Op::Switch => (OpKeyTag::Switch, 0, 0),
        Op::Return => (OpKeyTag::Return, 0, 0),
        Op::Add => (OpKeyTag::Add, 0, 0),
        Op::Sub => (OpKeyTag::Sub, 0, 0),
        Op::Mul => (OpKeyTag::Mul, 0, 0),
        Op::Div => (OpKeyTag::Div, 0, 0),
        Op::Rem => (OpKeyTag::Rem, 0, 0),
        Op::Add1 => (OpKeyTag::Add1, 0, 0),
        Op::Sub1 => (OpKeyTag::Sub1, 0, 0),
        Op::Negate => (OpKeyTag::Negate, 0, 0),
        Op::Eqlsign => (OpKeyTag::Eqlsign, 0, 0),
        Op::Gtr => (OpKeyTag::Gtr, 0, 0),
        Op::Lss => (OpKeyTag::Lss, 0, 0),
        Op::Leq => (OpKeyTag::Leq, 0, 0),
        Op::Geq => (OpKeyTag::Geq, 0, 0),
        Op::Max => (OpKeyTag::Max, 0, 0),
        Op::Min => (OpKeyTag::Min, 0, 0),
        Op::Car => (OpKeyTag::Car, 0, 0),
        Op::Cdr => (OpKeyTag::Cdr, 0, 0),
        Op::Cons => (OpKeyTag::Cons, 0, 0),
        Op::List(n) => (OpKeyTag::List, u32::from(*n), 0),
        Op::Length => (OpKeyTag::Length, 0, 0),
        Op::Nth => (OpKeyTag::Nth, 0, 0),
        Op::Nthcdr => (OpKeyTag::Nthcdr, 0, 0),
        Op::Setcar => (OpKeyTag::Setcar, 0, 0),
        Op::Setcdr => (OpKeyTag::Setcdr, 0, 0),
        Op::CarSafe => (OpKeyTag::CarSafe, 0, 0),
        Op::CdrSafe => (OpKeyTag::CdrSafe, 0, 0),
        Op::Elt => (OpKeyTag::Elt, 0, 0),
        Op::Nconc => (OpKeyTag::Nconc, 0, 0),
        Op::Nreverse => (OpKeyTag::Nreverse, 0, 0),
        Op::Member => (OpKeyTag::Member, 0, 0),
        Op::Memq => (OpKeyTag::Memq, 0, 0),
        Op::Assq => (OpKeyTag::Assq, 0, 0),
        Op::Symbolp => (OpKeyTag::Symbolp, 0, 0),
        Op::Consp => (OpKeyTag::Consp, 0, 0),
        Op::Stringp => (OpKeyTag::Stringp, 0, 0),
        Op::Listp => (OpKeyTag::Listp, 0, 0),
        Op::Integerp => (OpKeyTag::Integerp, 0, 0),
        Op::Numberp => (OpKeyTag::Numberp, 0, 0),
        Op::Null => (OpKeyTag::Null, 0, 0),
        Op::Not => (OpKeyTag::Not, 0, 0),
        Op::Eq => (OpKeyTag::Eq, 0, 0),
        Op::Equal => (OpKeyTag::Equal, 0, 0),
        Op::Concat(n) => (OpKeyTag::Concat, u32::from(*n), 0),
        Op::Substring => (OpKeyTag::Substring, 0, 0),
        Op::StringEqual => (OpKeyTag::StringEqual, 0, 0),
        Op::StringLessp => (OpKeyTag::StringLessp, 0, 0),
        Op::Aref => (OpKeyTag::Aref, 0, 0),
        Op::Aset => (OpKeyTag::Aset, 0, 0),
        Op::SymbolValue => (OpKeyTag::SymbolValue, 0, 0),
        Op::SymbolFunction => (OpKeyTag::SymbolFunction, 0, 0),
        Op::Set => (OpKeyTag::Set, 0, 0),
        Op::Fset => (OpKeyTag::Fset, 0, 0),
        Op::Get => (OpKeyTag::Get, 0, 0),
        Op::Put => (OpKeyTag::Put, 0, 0),
        Op::PushConditionCase(n) => (OpKeyTag::PushConditionCase, *n, 0),
        Op::PushConditionCaseRaw(n) => (OpKeyTag::PushConditionCaseRaw, *n, 0),
        Op::PushCatch(n) => (OpKeyTag::PushCatch, *n, 0),
        Op::PopHandler => (OpKeyTag::PopHandler, 0, 0),
        Op::UnwindProtectPop => (OpKeyTag::UnwindProtectPop, 0, 0),
        Op::Throw => (OpKeyTag::Throw, 0, 0),
        Op::SaveCurrentBuffer => (OpKeyTag::SaveCurrentBuffer, 0, 0),
        Op::SaveExcursion => (OpKeyTag::SaveExcursion, 0, 0),
        Op::SaveRestriction => (OpKeyTag::SaveRestriction, 0, 0),
        Op::SaveWindowExcursion => (OpKeyTag::SaveWindowExcursion, 0, 0),
        Op::MakeClosure(n) => (OpKeyTag::MakeClosure, u32::from(*n), 0),
        Op::CallBuiltin(n, m) => (OpKeyTag::CallBuiltin, u32::from(*n), *m),
        Op::CallBuiltinSym(sym, m) => (OpKeyTag::CallBuiltinSym, sym.0, *m),
        Op::TrapOutOfRangeConstant(n) => (OpKeyTag::TrapOutOfRangeConstant, u32::from(*n), 0),
    };
    u64::from(tag as u8) | (u64::from(a) << 8) | (u64::from(b) << 40)
}

/// `equal`-table key for a byte-code object, derived from
/// [`ByteCodeFunction::structural_parts`] so it agrees with [`bytecode_equal`]
/// by construction: equal objects produce equal keys, and every difference
/// `equal` sees -- including an absent versus a present-but-`nil` captured
/// environment, and a docstring's character count -- reaches the key.  GNU's
/// `sxhash_obj` -> `sxhash_vector` gives equal closures one bucket
/// (src/fns.c:5525-5536); this is the table's side of that promise.
fn bytecode_to_equal_key(
    value: Value,
    depth: usize,
    seen: &mut Vec<usize>,
    symbols_with_pos_enabled: bool,
) -> HashKey {
    use ByteCodeStructuralPart as Part;
    if depth > 200 {
        return value.to_eq_key();
    }
    let Some(bc) = value.get_bytecode_data() else {
        return value.to_eq_key();
    };
    let mut key_of =
        |slot: &Value| slot.to_equal_key_depth_swp(depth + 1, seen, symbols_with_pos_enabled);
    let parts = bc.structural_parts();
    let mut keys = Vec::with_capacity(parts.len());
    for part in parts {
        keys.push(match part {
            Part::ObservableSlotCount(count) => ByteCodeKeyPart::ObservableSlotCount(count),
            Part::Value(value) => ByteCodeKeyPart::Value(key_of(&value)),
            Part::Bytes(bytes) => ByteCodeKeyPart::Bytes(bytes.into()),
            Part::Ops(ops) => ByteCodeKeyPart::Ops(ops.iter().map(op_key_word).collect()),
            Part::Values(values) => {
                ByteCodeKeyPart::Values(values.iter().map(&mut key_of).collect())
            }
            // GNU string equality is characters + bytes + contents; the
            // representation flag is not part of it, and the character count
            // is (two raw bytes are not one multibyte character).
            Part::Text(text) => ByteCodeKeyPart::Text {
                char_count: text.schars(),
                bytes: text.as_bytes().into(),
            },
            Part::Absent => ByteCodeKeyPart::Absent,
        });
    }
    HashKey::ByteCode(keys.into_boxed_slice())
}

/// GNU string equality (src/fns.c `internal_equal_1`, `Lisp_String`):
/// character count, byte count and contents.  The unibyte/multibyte
/// representation flag `LispString::eq` also compares is not part of it: an
/// ASCII string stored unibyte equals the same text stored multibyte, while
/// two raw bytes never equal the one multibyte character with those bytes.
fn gnu_string_contents_equal(left: &LispString, right: &LispString) -> bool {
    left.schars() == right.schars()
        && left.sbytes() == right.sbytes()
        && left.as_bytes() == right.as_bytes()
}

/// The slot walk behind both [`bytecode_equal`] and [`try_bytecode_equal`]:
/// GNU's `ASIZE` check first, then [`ByteCodeFunction::structural_parts`]
/// pairwise, with `value_equal` deciding the Lisp values (constants at one
/// extra level of depth, as elements of the vector they sit in).
fn bytecode_slots_equal(
    left: &ByteCodeFunction,
    right: &ByteCodeFunction,
    depth: usize,
    value_equal: &mut dyn FnMut(&Value, &Value, usize) -> Result<bool, Flow>,
) -> Result<bool, Flow> {
    use ByteCodeStructuralPart as Part;
    let (left_parts, right_parts) = (left.structural_parts(), right.structural_parts());
    if left_parts.len() != right_parts.len() {
        return Ok(false);
    }
    for (left, right) in left_parts.zip(right_parts) {
        let equal = match (left, right) {
            (Part::ObservableSlotCount(a), Part::ObservableSlotCount(b)) => a == b,
            (Part::Value(a), Part::Value(b)) => value_equal(&a, &b, depth + 1)?,
            (Part::Bytes(a), Part::Bytes(b)) => a == b,
            (Part::Ops(a), Part::Ops(b)) => a == b,
            (Part::Values(a), Part::Values(b)) => {
                if a.len() != b.len() {
                    return Ok(false);
                }
                for (a, b) in a.iter().zip(b) {
                    if !value_equal(a, b, depth + 2)? {
                        return Ok(false);
                    }
                }
                true
            }
            (Part::Text(a), Part::Text(b)) => gnu_string_contents_equal(a, b),
            (Part::Absent, Part::Absent) => true,
            _ => false,
        };
        if !equal {
            return Ok(false);
        }
    }
    Ok(true)
}

/// GNU `internal_equal_1` compares a `PVEC_CLOSURE` exactly like a vector
/// (src/fns.c:2984-2998 in emacs-31.1, :2987-3001 on master): the `ASIZE`
/// test first -- which is also the type test, so a five-slot closure never
/// equals a four-slot one -- then every slot element-wise.  This port keeps
/// a byte-code function's GNU slots in typed fields, so the walk reads
/// [`ByteCodeFunction::structural_parts`], the one place that spells the
/// schema; `make-closure` (bytecode.c `Fmake_closure`) copies the prototype
/// and replaces the leading constants, so two instances of one prototype
/// with `equal` captures are `equal` -- the property `remove-hook` needs to
/// find a closure it was handed a freshly rebuilt twin of.
fn bytecode_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> bool {
    let (Some(l), Some(r)) = (left.get_bytecode_data(), right.get_bytecode_data()) else {
        return false;
    };
    if depth > 10 {
        let pair = EqualSeenPair::new(*left, *right);
        if !seen.get_or_insert_with(HashSet::new).insert(pair) {
            return true;
        }
    }
    bytecode_slots_equal(l, r, depth, &mut |a, b, depth| {
        Ok(equal_value_inner(
            a,
            b,
            depth,
            seen,
            symbols_with_pos_enabled,
            kind,
        ))
    })
    .unwrap_or(false)
}

/// [`bytecode_equal`] for the error-propagating walk.
fn try_bytecode_equal(
    left: &Value,
    right: &Value,
    depth: usize,
    seen: &mut Option<HashSet<EqualSeenPair>>,
    symbols_with_pos_enabled: bool,
    kind: EqualKind,
) -> Result<bool, Flow> {
    let (Some(l), Some(r)) = (left.get_bytecode_data(), right.get_bytecode_data()) else {
        return Ok(false);
    };
    if depth > 10 {
        let pair = EqualSeenPair::new(*left, *right);
        if !seen.get_or_insert_with(HashSet::new).insert(pair) {
            return Ok(true);
        }
    }
    bytecode_slots_equal(l, r, depth, &mut |a, b, depth| {
        try_equal_value_inner(a, b, depth, seen, symbols_with_pos_enabled, kind)
    })
}

/// GNU `sxhash_obj` (src/fns.c:5525-5536) sends a `PVEC_CLOSURE` through
/// `sxhash_vector` like any pseudovector `internal_equal` walks, so two
/// `equal` byte-code objects hash alike and share an `equal` hash-table
/// bucket.  The elements are [`ByteCodeFunction::structural_parts`] -- the
/// same sequence `equal` compares, so everything `equal` distinguishes,
/// including a captured environment beside the constants, moves the hash --
/// folded like `sxhash_vector`: seeded with the slot count, at most
/// `SXHASH_MAX_LEN` elements.
fn emacs_sxhash_bytecode(value: &Value, depth: usize, symbol_mode: SymbolMode) -> Option<u64> {
    use ByteCodeStructuralPart as Part;
    let bc = value.get_bytecode_data()?;
    let mut parts = bc.structural_parts();
    let Part::ObservableSlotCount(slot_count) = parts.next()? else {
        unreachable!("byte-code structure starts with its observable shape")
    };
    let mut hash = slot_count as u64;
    for part in parts.take(SXHASH_MAX_LEN) {
        let part_hash = match part {
            Part::ObservableSlotCount(_) => {
                unreachable!("byte-code structure contains one leading shape")
            }
            Part::Value(value) => emacs_sxhash_obj_with_fallback(&value, depth + 1, symbol_mode),
            Part::Bytes(bytes) => emacs_hash_char_array(bytes),
            Part::Ops(ops) => {
                let mut hasher = DefaultHasher::new();
                ops.hash(&mut hasher);
                hasher.finish() & SXHASH_FALLBACK_NONNEG_MASK
            }
            Part::Values(values) => {
                let mut vector_hash = values.len() as u64;
                for value in values.iter().take(SXHASH_MAX_LEN) {
                    vector_hash = sxhash_combine(
                        vector_hash,
                        emacs_sxhash_obj_with_fallback(value, depth + 2, symbol_mode),
                    );
                }
                vector_hash
            }
            // GNU `sxhash_string` hashes the bytes; equal strings have equal
            // bytes whatever their representation.
            Part::Text(text) => emacs_hash_char_array(text.as_bytes()),
            Part::Absent => 42,
        };
        hash = sxhash_combine(hash, part_hash);
    }
    Some(hash)
}
