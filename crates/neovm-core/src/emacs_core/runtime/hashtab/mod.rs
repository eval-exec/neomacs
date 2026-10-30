//! Extended hash-table and obarray builtins.
//!
//! Supplements the basic hash-table operations in `builtins.rs` with:
//! - `maphash`
//! - `hash-table-test`, `hash-table-size`, `hash-table-rehash-size`,
//!   `hash-table-rehash-threshold`, `hash-table-weakness`
//! - `copy-hash-table`
//! - `mapatoms`, `unintern`

use super::error::{EvalResult, Flow, signal};
use super::intern::{SymId, resolve_sym};
use super::print::print_value;
use super::value::*;
use crate::buffer::{CharLen, CharPos0, CharRange};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{expect_args, expect_max_args, expect_min_args};
use crate::tagged::SymbolMode;
use crate::tagged::gc::with_tagged_heap;
use std::collections::{BTreeMap, hash_map::DefaultHasher};
use std::hash::{Hash, Hasher};

/// Entry operands for one synchronous, GC-inhibited user-test activation.
/// The owning mutator pins this on its stack and its Context selects it;
/// independent mutators never share it. Normalized scalar settings preserve
/// the entry countdown without holding Lisp values across callbacks.
#[derive(Clone, Copy)]
pub(crate) struct HashTestGcInhibitAccounting {
    pub(crate) bytes_at_start: usize,
    pub(crate) charged_bytes_at_start: usize,
    pub(crate) collector_threshold_at_start: usize,
    /// Resolved only when GC-maybe first needs the saved entry operands.
    pub(crate) threshold_at_start: Option<std::num::NonZeroUsize>,
    pub(crate) entry_threshold_bytes: usize,
    pub(crate) entry_percentage_scaled: Option<std::num::NonZeroU64>,
    pub(crate) threshold_overridden: bool,
    pub(crate) memory_full: bool,
    pub(crate) startup_ceiling: bool,
}

// Callback knobs, read once per process:
// | Knob | Values | Default | Effect |
// | NEOVM_MAPHASH_BYTECODE | on, off | on | Call a bytecode maphash callback through the rooted two-argument entry without a LispArgVec on the armed path. |
// | NEOVM_HASH_TEST_PARITY | on, off | on | Inhibit GC during user hash tests, restore mutability on unwind, make mutable copies, and preserve every custom test callback. |

/// Immutable process policy contains no Lisp state. LazyLock publishes it to
/// concurrent mutators; guarded state belongs to each table and Context.
static HASH_TEST_PARITY: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    !std::env::var("NEOVM_HASH_TEST_PARITY").is_ok_and(|value| value == "off")
});

pub(crate) fn hash_test_parity_enabled() -> bool {
    *HASH_TEST_PARITY
}

mod user_test_guard;
pub(crate) use user_test_guard::{
    gc_threshold_integer_fallback, inhibited_user_test_since_gc, with_user_test_guard,
};

/// Immutable process configuration holds no Lisp state. Concurrent mutators
/// may read it; each maphash activation owns its callback and roots.
#[cfg(feature = "jit")]
static MAPHASH_BYTECODE: std::sync::LazyLock<bool> = std::sync::LazyLock::new(|| {
    !std::env::var("NEOVM_MAPHASH_BYTECODE").is_ok_and(|value| value == "off")
});

/// A callback selected for one synchronous maphash activation. GNU resolves
/// symbol callbacks anew for every slot; only a direct bytecode identity can
/// use the specialized entry. This value and its roots belong to one mutator,
/// and no Lisp state is cached across activations or shared with other threads.
pub(crate) enum MaphashCallee {
    Generic(Value),
    #[cfg(feature = "jit")]
    ByteCode(Value),
    #[cfg(feature = "jit")]
    UnobservedByteCode(Value),
}

impl MaphashCallee {
    pub(crate) fn resolve(function: Value) -> Self {
        #[cfg(feature = "jit")]
        if *MAPHASH_BYTECODE && function.is_bytecode() {
            return if crate::tagged::collection_reads::reads_need_observation() {
                Self::ByteCode(function)
            } else {
                Self::UnobservedByteCode(function)
            };
        }
        Self::Generic(function)
    }

    #[inline]
    fn call(&self, eval: &mut super::eval::Context, key: Value, value: Value) -> EvalResult {
        match *self {
            Self::Generic(function) => eval.apply2(function, key, value),
            #[cfg(feature = "jit")]
            Self::ByteCode(function) => eval.apply2_bytecode(function, key, value),
            #[cfg(feature = "jit")]
            Self::UnobservedByteCode(function) => {
                eval.apply2_bytecode_unobserved(function, key, value)
            }
        }
    }
}

const SXHASH_MAX_DEPTH: usize = 3;
pub(crate) const SXHASH_MAX_LEN: usize = 7;
const SXHASH_FIXNUM_SHIFT: u32 = 2;
const SXHASH_FIXNUM_BITS: u32 = 62;
const SXHASH_INTMASK: u64 = (1_u64 << SXHASH_FIXNUM_BITS) - 1;
pub(crate) const SXHASH_FALLBACK_NONNEG_MASK: u64 = (1_u64 << (SXHASH_FIXNUM_BITS - 1)) - 1;
const LISP_TYPE_INT0: u64 = 2;
const LISP_TYPE_INT1: u64 = 6;
const KNUTH_ALPHA: u32 = 2_654_435_769;

/// Convert a `HashKey` back into a `Value`.
pub(crate) fn hash_key_to_value(key: &HashKey) -> Value {
    match key {
        HashKey::Nil => Value::NIL,
        HashKey::True => Value::T,
        HashKey::Int(n) => Value::fixnum(*n),
        HashKey::Bignum(limbs) => Value::make_integer(
            malachite::integer::Integer::from_twos_complement_limbs_asc(limbs),
        ),
        HashKey::Float(bits) => Value::make_float(f64::from_bits(*bits)),
        HashKey::FloatEq(bits, _id) => Value::make_float(f64::from_bits(*bits)),
        HashKey::Symbol(id) => Value::from_sym_id(*id),
        HashKey::Keyword(id) => Value::keyword_id(*id),
        HashKey::Text(text) => Value::string(text.to_string()),
        HashKey::StringContent(content) => Value::heap_string(
            crate::heap_types::LispString::from_bytes_with_schars(&content.0, content.1),
        ),
        HashKey::Char(c) => Value::char(*c),
        HashKey::Window(id) => Value::make_window(*id),
        HashKey::Frame(id) => Value::make_frame(*id),
        HashKey::Ptr(_) => Value::NIL, // can't reconstruct from pointer
        HashKey::EqualCons(car, cdr) => Value::cons(hash_key_to_value(car), hash_key_to_value(cdr)),
        HashKey::EqualVec(items) => {
            let vals: Vec<Value> = items.iter().map(hash_key_to_value).collect();
            Value::vector(vals)
        }
        HashKey::ByteCode(_) => Value::NIL,
        HashKey::Marker(_) | HashKey::Overlay(_) => Value::NIL,
        HashKey::BoolVector(parts) => Value::make_bool_vector(parts.0, parts.1.to_vec()),
        HashKey::SymbolWithPos(_, _) => Value::NIL,
        HashKey::Cycle(index) => Value::string(format!("#{}", index)),
    }
}

pub(crate) fn hash_key_to_visible_value(table: &LispHashTable, key: &HashKey) -> Value {
    table
        .key_snapshot(key)
        .cloned()
        .unwrap_or_else(|| hash_key_to_value(key))
}

pub(crate) fn sxhash_combine(x: u64, y: u64) -> u64 {
    x.rotate_left(4).wrapping_add(y)
}

/// Hash a bignum by its numeric value, mirroring GNU `sxhash_bignum`:
/// seed with the sign (1 if negative, else 0) then fold each machine limb
/// of the magnitude (least-significant first). On a 64-bit host malachite's
/// limbs are `u64`, matching GMP's `mpz_getlimbn` / `EMACS_UINT`, so two
/// numerically-equal bignums hash identically regardless of heap address.
fn sxhash_bignum(value: &Value) -> Option<u64> {
    let bignum = value.as_bignum()?;
    let mut hash: u64 = if *bignum < 0 { 1 } else { 0 };
    for limb in bignum.unsigned_abs_ref().to_limbs_asc() {
        hash = sxhash_combine(hash, limb);
    }
    Some(hash)
}

/// GNU `sxhash_bool_vector` (src/fns.c:5462): the size, then the first
/// `SXHASH_MAX_LEN` data words.
fn sxhash_bool_vector(value: &Value) -> Option<u64> {
    let bv = value.as_bool_vector_obj()?;
    let mut hash = bv.nbits as u64;
    for &word in bv.words().iter().take(SXHASH_MAX_LEN) {
        hash = sxhash_combine(hash, word);
    }
    Some(hash)
}

fn read_u16_ne(bytes: &[u8]) -> u16 {
    let mut arr = [0_u8; 2];
    arr.copy_from_slice(bytes);
    u16::from_ne_bytes(arr)
}

fn read_u32_ne(bytes: &[u8]) -> u32 {
    let mut arr = [0_u8; 4];
    arr.copy_from_slice(bytes);
    u32::from_ne_bytes(arr)
}

fn read_u64_ne(bytes: &[u8]) -> u64 {
    let mut arr = [0_u8; 8];
    arr.copy_from_slice(bytes);
    u64::from_ne_bytes(arr)
}

pub(crate) fn emacs_hash_char_array(bytes: &[u8]) -> u64 {
    let mut hash = bytes.len() as u64;
    let word_bytes = std::mem::size_of::<u64>();
    if bytes.len() >= word_bytes {
        let mut p = 0_usize;
        let step = word_bytes.max(bytes.len() >> 3);
        while p + word_bytes <= bytes.len() {
            let chunk = read_u64_ne(&bytes[p..p + word_bytes]);
            hash = sxhash_combine(hash, chunk);
            p = p.saturating_add(step);
        }
        let tail = read_u64_ne(&bytes[bytes.len() - word_bytes..]);
        hash = sxhash_combine(hash, tail);
    } else {
        let mut tail = 0_u64;
        let mut p = 0_usize;
        if bytes.len().saturating_sub(p) >= 4 {
            let chunk = read_u32_ne(&bytes[p..p + 4]) as u64;
            tail = (tail << 32).wrapping_add(chunk);
            p += 4;
        }
        if bytes.len().saturating_sub(p) >= 2 {
            let chunk = read_u16_ne(&bytes[p..p + 2]) as u64;
            tail = (tail << 16).wrapping_add(chunk);
            p += 2;
        }
        if p < bytes.len() {
            tail = (tail << 8).wrapping_add(bytes[p] as u64);
        }
        hash = sxhash_combine(hash, tail);
    }
    hash
}

fn fallback_sxhash_emacs_uint(value: &Value, test: HashTableTest, symbol_mode: SymbolMode) -> u64 {
    let mut hasher = DefaultHasher::new();
    match test {
        HashTableTest::Equal => hash_value_for_equal(value, &mut hasher, 0, symbol_mode),
        _ => value.to_hash_key(&test).hash(&mut hasher),
    }
    // Keep fallback hashes in the non-negative fixnum lane. Exact Emacs
    // parity is only guaranteed for covered immediate/sequence fast paths.
    hasher.finish() & SXHASH_FALLBACK_NONNEG_MASK
}

pub(crate) fn emacs_sxhash_obj_with_fallback(
    value: &Value,
    depth: usize,
    symbol_mode: SymbolMode,
) -> u64 {
    emacs_sxhash_obj(value, depth, symbol_mode)
        .unwrap_or_else(|| fallback_sxhash_emacs_uint(value, HashTableTest::Equal, symbol_mode))
}

fn emacs_sxhash_list(value: &Value, depth: usize, symbol_mode: SymbolMode) -> u64 {
    let mut hash = 0_u64;
    let mut cursor = *value;
    if depth < SXHASH_MAX_DEPTH {
        for _ in 0..SXHASH_MAX_LEN {
            if !cursor.is_cons() {
                break;
            };
            let pair_car = cursor.cons_car();
            let pair_cdr = cursor.cons_cdr();
            hash = sxhash_combine(
                hash,
                emacs_sxhash_obj_with_fallback(&pair_car, depth + 1, symbol_mode),
            );
            cursor = pair_cdr;
        }
    }
    if !cursor.is_nil() {
        hash = sxhash_combine(
            hash,
            emacs_sxhash_obj_with_fallback(&cursor, depth + 1, symbol_mode),
        );
    }
    hash
}

fn emacs_sxhash_structural_pseudovector(
    value: &Value,
    depth: usize,
    symbol_mode: SymbolMode,
) -> Option<u64> {
    let kind = value.veclike_type()?;
    // GNU deliberately does not traverse the packed prefix and deep tree of a
    // sub-char-table. All such nodes use the same valid (collision-heavy)
    // equal hash.
    if kind == VecLikeType::SubCharTable {
        return Some(42);
    }
    let view = StructuralPseudovectorView::from_value(*value, kind)?;
    let mut hash = view.len() as u64;
    let count = view.len().min(SXHASH_MAX_LEN);
    for index in 0..count {
        hash = sxhash_combine(
            hash,
            emacs_sxhash_obj_with_fallback(&view.slot(index), depth + 1, symbol_mode),
        );
    }
    Some(hash)
}

fn emacs_sxhash_marker(value: &Value) -> Option<u64> {
    let marker = value.as_marker_data()?;
    let bytepos = if marker.buffer.is_attached() {
        marker.bytepos.get() as u64
    } else {
        0
    };
    Some(sxhash_combine(
        marker.buffer_id().map(|buffer| buffer.0).unwrap_or(0),
        bytepos,
    ))
}

fn emacs_sxhash_overlay(value: &Value, depth: usize, symbol_mode: SymbolMode) -> Option<u64> {
    let overlay = value.as_overlay_data()?;
    let (start, end) = overlay.current_range();
    let mut hash = start as u64;
    hash = sxhash_combine(hash, end as u64);
    hash = sxhash_combine(
        hash,
        emacs_sxhash_obj_with_fallback(&overlay.plist, depth, symbol_mode),
    );
    Some(hash)
}

fn emacs_sxhash_obj(value: &Value, depth: usize, symbol_mode: SymbolMode) -> Option<u64> {
    if depth > SXHASH_MAX_DEPTH {
        return Some(0);
    }
    let value = &symbol_mode.unwrap(*value);
    match value.kind() {
        ValueKind::Fixnum(n) => Some(n as u64),
        ValueKind::Float => Some(value.xfloat().to_bits()),
        ValueKind::String => Some(emacs_hash_char_array(
            value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload")
                .as_bytes(),
        )),
        ValueKind::Cons => Some(emacs_sxhash_list(value, depth, symbol_mode)),
        ValueKind::Veclike(VecLikeType::Vector)
        | ValueKind::Veclike(VecLikeType::Record)
        | ValueKind::Veclike(VecLikeType::CharTable)
        | ValueKind::Veclike(VecLikeType::SubCharTable) => {
            emacs_sxhash_structural_pseudovector(value, depth, symbol_mode)
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            (crate::emacs_core::value::bytecode_semantics().sxhash)(value, depth, symbol_mode)
        }
        ValueKind::Veclike(VecLikeType::Bignum) => sxhash_bignum(value),
        ValueKind::Veclike(VecLikeType::BoolVector) => sxhash_bool_vector(value),
        ValueKind::Veclike(VecLikeType::Marker) => emacs_sxhash_marker(value),
        ValueKind::Veclike(VecLikeType::Overlay) => emacs_sxhash_overlay(value, depth, symbol_mode),
        _ => None,
    }
}

fn plist_pairs_to_value(plist: &[(Value, Value)]) -> Value {
    let mut out = Value::NIL;
    for (key, value) in plist.iter().rev() {
        out = Value::cons(*value, out);
        out = Value::cons(*key, out);
    }
    out
}

fn sxhash_equal_including_properties_emacs_uint(value: &Value, symbol_mode: SymbolMode) -> u64 {
    let mut hash = sxhash_emacs_uint_for(value, HashTableTest::Equal, symbol_mode);
    if !value.is_string() {
        return hash;
    }

    let Some(string) = value.as_lisp_string() else {
        return hash;
    };
    if let Some(table) = get_string_text_properties_table_for_value(*value) {
        let _ = table.try_for_each_interval_in_char_range(
            CharRange::from_start_len(CharPos0::ZERO, CharLen::new(string.schars())),
            |range, plist| {
                hash = sxhash_combine(hash, range.start().get() as u64);
                hash = sxhash_combine(hash, range.len().get() as u64);
                let plist_value = plist_pairs_to_value(plist);
                hash = sxhash_combine(
                    hash,
                    emacs_sxhash_obj_with_fallback(&plist_value, 0, symbol_mode),
                );
                Ok::<(), ()>(())
            },
        );
    }
    hash
}

fn reduce_emacs_uint_to_fixnum(x: u64) -> i64 {
    let reduced = (x ^ (x >> SXHASH_FIXNUM_SHIFT)) & SXHASH_INTMASK;
    let sign_bit = 1_u64 << (SXHASH_FIXNUM_BITS - 1);
    if (reduced & sign_bit) != 0 {
        (reduced as i64) - ((1_i64) << SXHASH_FIXNUM_BITS)
    } else {
        reduced as i64
    }
}

pub(crate) fn reduce_emacs_uint_to_hash_hash(x: u64) -> u32 {
    (x ^ (x >> 32)) as u32
}

fn sxhash_eq_fixnum_uint(raw: u64) -> u64 {
    let raw = raw & SXHASH_INTMASK;
    // GNU Emacs `sxhash-eq` uses `XHASH ^ XTYPE` for fixnums/chars.
    let xtype = if (raw & 1) == 0 {
        LISP_TYPE_INT0
    } else {
        LISP_TYPE_INT1
    };
    raw ^ xtype
}

pub(crate) fn knuth_hash_index(hash: u32, bits: u32) -> usize {
    if bits == 0 {
        return 0;
    }
    let product = hash.wrapping_mul(KNUTH_ALPHA);
    ((product as u64) >> (32 - bits)) as usize
}

fn hash_value_for_equal(
    value: &Value,
    hasher: &mut DefaultHasher,
    depth: usize,
    symbol_mode: SymbolMode,
) {
    if depth > 4096 {
        0_u8.hash(hasher);
        return;
    }
    let value = &symbol_mode.unwrap(*value);
    match value.kind() {
        ValueKind::Nil => 0_u8.hash(hasher),
        ValueKind::T => 1_u8.hash(hasher),
        ValueKind::Fixnum(n) => {
            // `equal` treats chars and ints with same codepoint as equal.
            2_u8.hash(hasher);
            n.hash(hasher);
        }
        ValueKind::Float => {
            3_u8.hash(hasher);
            value.xfloat().to_bits().hash(hasher);
        }
        ValueKind::Symbol(id) => {
            4_u8.hash(hasher);
            resolve_sym(id).hash(hasher);
        }
        ValueKind::String => {
            6_u8.hash(hasher);
            let string = value
                .as_lisp_string()
                .expect("ValueKind::String must carry LispString payload");
            string.is_multibyte().hash(hasher);
            string.as_bytes().hash(hasher);
        }
        ValueKind::Cons => {
            7_u8.hash(hasher);
            let pair_car = value.cons_car();
            let pair_cdr = value.cons_cdr();
            hash_value_for_equal(&pair_car, hasher, depth + 1, symbol_mode);
            hash_value_for_equal(&pair_cdr, hasher, depth + 1, symbol_mode);
        }
        ValueKind::Veclike(VecLikeType::Vector) | ValueKind::Veclike(VecLikeType::Record) => {
            8_u8.hash(hasher);
            // Records aren't `is_vector()`, so as_vector_data() returns None for
            // them -- fall back to as_record_data() (mirrors GNU sxhash hashing a
            // record's slots like a vector). Previously this unwrap() panicked on
            // any record / cl-defstruct, aborting the whole process.
            let items = value
                .as_vector_data()
                .or_else(|| value.as_record_data())
                .unwrap()
                .clone();
            items.len().hash(hasher);
            for item in items.iter() {
                hash_value_for_equal(item, hasher, depth + 1, symbol_mode);
            }
        }
        ValueKind::Veclike(VecLikeType::Window) => {
            9_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Frame) => {
            10_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Buffer) => {
            11_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Timer) => {
            12_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Subr(id) => {
            13_u8.hash(hasher);
            resolve_sym(id).hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Subr) => {
            13_u8.hash(hasher);
            let id = value.as_subr_id().unwrap();
            resolve_sym(id).hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Lambda) => {
            14_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Macro) => {
            15_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::HashTable) => {
            16_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::ByteCode) => {
            17_u8.hash(hasher);
            value.bits().hash(hasher);
        }
        ValueKind::Veclike(VecLikeType::Marker) => {
            18_u8.hash(hasher);
            if let Some(marker) = value.as_marker_data() {
                marker.buffer_id().map(|buffer| buffer.0).hash(hasher);
                if marker.buffer.is_attached() {
                    marker.bytepos.get().hash(hasher);
                } else {
                    0_usize.hash(hasher);
                }
            } else {
                value.bits().hash(hasher);
            }
        }
        ValueKind::Veclike(VecLikeType::Overlay) => {
            19_u8.hash(hasher);
            if let Some(overlay) = value.as_overlay_data() {
                let (start, end) = overlay.current_range();
                overlay.buffer.map(|buffer| buffer.0).hash(hasher);
                start.hash(hasher);
                end.hash(hasher);
                hash_value_for_equal(&overlay.plist, hasher, depth + 1, symbol_mode);
            } else {
                value.bits().hash(hasher);
            }
        }
        _ => {
            // Unknown or other types: hash by pointer bits
            20_u8.hash(hasher);
            value.bits().hash(hasher);
        }
    }
}

fn sxhash_emacs_uint_for(value: &Value, test: HashTableTest, symbol_mode: SymbolMode) -> u64 {
    let value = &symbol_mode.unwrap(*value);
    match test {
        HashTableTest::Equal => emacs_sxhash_obj_with_fallback(value, 0, symbol_mode),
        HashTableTest::Eq | HashTableTest::Eql => match value.kind() {
            ValueKind::Fixnum(n) => sxhash_eq_fixnum_uint(n as u64),
            ValueKind::Float if matches!(test, HashTableTest::Eql) => value.xfloat().to_bits(),
            // GNU `sxhash_eql` hashes floats and bignums by value (full
            // `sxhash`); only `eq` uses object identity for bignums.
            ValueKind::Veclike(VecLikeType::Bignum) if matches!(test, HashTableTest::Eql) => {
                sxhash_bignum(value)
                    .unwrap_or_else(|| fallback_sxhash_emacs_uint(value, test, symbol_mode))
            }
            _ => fallback_sxhash_emacs_uint(value, test, symbol_mode),
        },
    }
}

/// Hash one operation using its owning mutator's symbol policy. This copied
/// policy owns no Lisp state and is threaded through recursive hashing; it is
/// never recovered from global/TLS state. GNU fns.c sxhash_obj unwraps a
/// positioned symbol only while symbols_with_pos_enabled (line 5555).
pub(crate) fn sxhash_for(value: &Value, test: HashTableTest, symbol_mode: SymbolMode) -> i64 {
    reduce_emacs_uint_to_fixnum(sxhash_emacs_uint_for(value, test, symbol_mode))
}

fn next_pow2_saturating(value: usize) -> usize {
    value.checked_next_power_of_two().unwrap_or(usize::MAX)
}

fn internal_hash_table_index_size(table: &LispHashTable) -> usize {
    if table.size <= 0 {
        1
    } else {
        next_pow2_saturating((table.size as usize).saturating_add(1)).max(1)
    }
}

fn internal_hash_table_diagnostic_hash(key: &HashKey, test: HashTableTest) -> u32 {
    match test {
        HashTableTest::Eq => match key {
            // Eq tables carry pointer-identity keys; preserve that identity in
            // diagnostic hash output instead of collapsing through `nil`.
            HashKey::Ptr(ptr) => reduce_emacs_uint_to_hash_hash(*ptr as u64),
            _ => {
                let value = hash_key_to_value(key);
                reduce_emacs_uint_to_hash_hash(sxhash_emacs_uint_for(
                    &value,
                    HashTableTest::Eq,
                    SymbolMode::Bare,
                ))
            }
        },
        HashTableTest::Eql => match key {
            HashKey::Ptr(ptr) => reduce_emacs_uint_to_hash_hash(*ptr as u64),
            _ => {
                let value = hash_key_to_value(key);
                reduce_emacs_uint_to_hash_hash(sxhash_emacs_uint_for(
                    &value,
                    HashTableTest::Eql,
                    SymbolMode::Bare,
                ))
            }
        },
        HashTableTest::Equal => match key {
            HashKey::Float(bits) => reduce_emacs_uint_to_hash_hash(*bits),
            HashKey::Int(n) => reduce_emacs_uint_to_hash_hash(*n as u64),
            HashKey::Char(c) => reduce_emacs_uint_to_hash_hash((*c as u32) as u64),
            HashKey::Text(text) => {
                let hash = emacs_hash_char_array(text.as_bytes());
                reduce_emacs_uint_to_hash_hash(hash)
            }
            _ => {
                let value = hash_key_to_value(key);
                reduce_emacs_uint_to_hash_hash(sxhash_emacs_uint_for(
                    &value,
                    HashTableTest::Equal,
                    SymbolMode::Bare,
                ))
            }
        },
    }
}

fn internal_hash_table_nonempty_buckets(table: &LispHashTable) -> Vec<Vec<(Value, i64)>> {
    if table.data.is_empty() {
        return Vec::new();
    }
    let bucket_count = internal_hash_table_index_size(table).max(1);
    let index_bits = bucket_count.trailing_zeros();
    let test = table.test;
    let mut buckets: Vec<Vec<(Value, i64)>> = vec![Vec::new(); bucket_count];
    for (key, entry) in table.data.keyed_entries_in_slot_order() {
        let hash = internal_hash_table_diagnostic_hash(key, test);
        let index = knuth_hash_index(hash, index_bits);
        buckets[index].push((entry.key, hash as i64));
    }
    for bucket in &mut buckets {
        bucket.sort_by_key(|(key, hash)| (print_value(key), *hash));
    }
    buckets
        .into_iter()
        .filter(|bucket| !bucket.is_empty())
        .collect()
}

// ---------------------------------------------------------------------------
// Pure builtins
// ---------------------------------------------------------------------------

/// (hash-table-test TABLE) -> symbol
pub(crate) fn builtin_hash_table_test(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-test", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap();
            if let Some(id) = table.test_name {
                Ok(Value::from_sym_id(id))
            } else {
                Ok(Value::symbol(table.test.name()))
            }
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-size TABLE) -> integer
pub(crate) fn builtin_hash_table_size(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-size", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap();
            Ok(Value::fixnum(table.size))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-rehash-size TABLE) -> float
pub(crate) fn builtin_hash_table_rehash_size(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-rehash-size", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap();
            Ok(Value::make_float(table.rehash_size))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-rehash-threshold TABLE) -> float
pub(crate) fn builtin_hash_table_rehash_threshold(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-rehash-threshold", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap();
            Ok(Value::make_float(table.rehash_threshold))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-weakness TABLE) -> nil | symbol
pub(crate) fn builtin_hash_table_weakness(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-weakness", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap();
            Ok(match table.weakness {
                None => Value::NIL,
                Some(weakness) => Value::symbol(weakness.name()),
            })
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (copy-hash-table TABLE) -> new hash table with same entries
pub(crate) fn builtin_copy_hash_table(args: Vec<Value>) -> EvalResult {
    expect_args("copy-hash-table", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let mut new_table = args[0].as_hash_table().unwrap().clone();
            if hash_test_parity_enabled() {
                // GNU copies entries and test functions, but a running test's
                // logical immutability belongs only to its original table:
                // the copy's nesting count starts at zero even when the
                // source is mid-test (Rev R2).
                new_table.test_depth = std::sync::atomic::AtomicU8::new(0);
            }
            Ok(with_tagged_heap(|h| h.alloc_hash_table(new_table)))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-keys TABLE) -> list of keys
#[cfg(test)]
pub(crate) fn builtin_hash_table_keys(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-keys", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap().clone();
            let keys: Vec<Value> = table
                .data
                .keyed_entries_in_slot_order()
                .into_iter()
                .map(|(_, entry)| entry.key)
                .collect();
            Ok(Value::list(keys))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// (hash-table-values TABLE) -> list of values
#[cfg(test)]
pub(crate) fn builtin_hash_table_values(args: Vec<Value>) -> EvalResult {
    expect_args("hash-table-values", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap().clone();
            let values: Vec<Value> = table
                .data
                .keyed_entries_in_slot_order()
                .into_iter()
                .map(|(_, entry)| entry.value)
                .collect();
            Ok(Value::list(values))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// `(sxhash-eq OBJECT)` -- hash OBJECT according to `eq` semantics.
pub(crate) fn builtin_sxhash_eq_with_mode(args: Vec<Value>, symbol_mode: SymbolMode) -> EvalResult {
    expect_args("sxhash-eq", &args, 1)?;
    Ok(Value::fixnum(sxhash_for(
        &args[0],
        HashTableTest::Eq,
        symbol_mode,
    )))
}

/// `(sxhash-eql OBJECT)` -- hash OBJECT according to `eql` semantics.
pub(crate) fn builtin_sxhash_eql_with_mode(
    args: Vec<Value>,
    symbol_mode: SymbolMode,
) -> EvalResult {
    expect_args("sxhash-eql", &args, 1)?;
    Ok(Value::fixnum(sxhash_for(
        &args[0],
        HashTableTest::Eql,
        symbol_mode,
    )))
}

/// `(sxhash-equal OBJECT)` -- hash OBJECT according to `equal` semantics.
pub(crate) fn builtin_sxhash_equal_with_mode(
    args: Vec<Value>,
    symbol_mode: SymbolMode,
) -> EvalResult {
    expect_args("sxhash-equal", &args, 1)?;
    Ok(Value::fixnum(sxhash_for(
        &args[0],
        HashTableTest::Equal,
        symbol_mode,
    )))
}

/// `(sxhash-equal-including-properties OBJECT)` -- hash OBJECT like
/// `equal-including-properties`. NeoVM currently has no text properties, so
/// this matches `sxhash-equal`.
pub(crate) fn builtin_sxhash_equal_including_properties_with_mode(
    args: Vec<Value>,
    symbol_mode: SymbolMode,
) -> EvalResult {
    expect_args("sxhash-equal-including-properties", &args, 1)?;
    Ok(Value::fixnum(reduce_emacs_uint_to_fixnum(
        sxhash_equal_including_properties_emacs_uint(&args[0], symbol_mode),
    )))
}

// Bare adapters are available only to existing low-level unit tests.
#[cfg(test)]
pub(crate) fn builtin_sxhash_eq(args: Vec<Value>) -> EvalResult {
    builtin_sxhash_eq_with_mode(args, SymbolMode::Bare)
}

#[cfg(test)]
pub(crate) fn builtin_sxhash_eql(args: Vec<Value>) -> EvalResult {
    builtin_sxhash_eql_with_mode(args, SymbolMode::Bare)
}

#[cfg(test)]
pub(crate) fn builtin_sxhash_equal(args: Vec<Value>) -> EvalResult {
    builtin_sxhash_equal_with_mode(args, SymbolMode::Bare)
}

#[cfg(test)]
pub(crate) fn builtin_sxhash_equal_including_properties(args: Vec<Value>) -> EvalResult {
    builtin_sxhash_equal_including_properties_with_mode(args, SymbolMode::Bare)
}

/// `(internal--hash-table-index-size TABLE)` -- report hash index width.
pub(crate) fn builtin_internal_hash_table_index_size(args: Vec<Value>) -> EvalResult {
    expect_args("internal--hash-table-index-size", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap().clone();
            Ok(Value::fixnum(
                internal_hash_table_index_size(&table).min(i64::MAX as usize) as i64,
            ))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// `(internal--hash-table-buckets TABLE)` -- return non-empty bucket alists.
pub(crate) fn builtin_internal_hash_table_buckets(args: Vec<Value>) -> EvalResult {
    expect_args("internal--hash-table-buckets", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap().clone();
            let buckets = internal_hash_table_nonempty_buckets(&table);
            if buckets.is_empty() {
                return Ok(Value::NIL);
            }
            let rendered = buckets
                .into_iter()
                .map(|bucket| {
                    let alist_items: Vec<Value> = bucket
                        .into_iter()
                        .map(|(key, hash)| Value::cons(key, Value::fixnum(hash)))
                        .collect();
                    Value::list(alist_items)
                })
                .collect();
            Ok(Value::list(rendered))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

/// `(internal--hash-table-histogram TABLE)` -- return (bucket-size . count)
/// alist for non-empty buckets.
pub(crate) fn builtin_internal_hash_table_histogram(args: Vec<Value>) -> EvalResult {
    expect_args("internal--hash-table-histogram", &args, 1)?;
    match args[0].kind() {
        ValueKind::Veclike(VecLikeType::HashTable) => {
            let table = args[0].as_hash_table().unwrap().clone();
            let buckets = internal_hash_table_nonempty_buckets(&table);
            if buckets.is_empty() {
                return Ok(Value::NIL);
            }
            let mut histogram: BTreeMap<i64, i64> = BTreeMap::new();
            for bucket in buckets {
                let size = bucket.len() as i64;
                *histogram.entry(size).or_insert(0) += 1;
            }
            let entries: Vec<Value> = histogram
                .into_iter()
                .map(|(size, count)| Value::cons(Value::fixnum(size), Value::fixnum(count)))
                .collect();
            Ok(Value::list(entries))
        }
        _ => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[0]],
        )),
    }
}

// ---------------------------------------------------------------------------
// Eval-dependent builtins
// ---------------------------------------------------------------------------

/// (maphash FUNCTION TABLE) — call FUNCTION with each (KEY VALUE) pair.
pub(crate) fn builtin_maphash(eval: &mut super::eval::Context, args: Vec<Value>) -> EvalResult {
    expect_args("maphash", &args, 2)?;
    if !args[1].is_hash_table() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[1]],
        ));
    }

    let roots = eval.save_specpdl_roots();
    for arg in &args {
        eval.push_specpdl_root(*arg);
    }
    let func = args[0];
    let table = args[1];
    let callee = MaphashCallee::resolve(func);
    // Native proof stays within this mutator's activation and owns no roots.
    let native = if super::eval::native_callback_cache_enabled() {
        let epoch = eval.obarray().function_epoch();
        eval.resolve_mapped_subr_callee(func).and_then(|(subr, _)| {
            super::eval::CheckedNativeCallback::resolve(subr, 2).map(|proof| (subr, epoch, proof))
        })
    } else {
        None
    };
    let result = (|| -> EvalResult {
        let mut slot = 0_usize;
        loop {
            let Some((key, val)) = maphash_entry_at_slot(table, slot) else {
                if slot >= maphash_slot_len(table) {
                    break;
                }
                slot += 1;
                continue;
            };
            eval.push_specpdl_root(key);
            eval.push_specpdl_root(val);
            match native {
                Some((subr, epoch, proof)) => {
                    eval.apply2_checked_subr(func, subr, epoch, proof, key, val)?;
                }
                None => {
                    callee.call(eval, key, val)?;
                }
            }
            slot += 1;
        }
        Ok(Value::NIL)
    })();
    eval.restore_specpdl_roots(roots);
    result
}

pub(crate) fn validate_maphash_args(args: &[Value]) -> Result<(Value, Value), Flow> {
    expect_args("maphash", args, 2)?;
    if !args[1].is_hash_table() {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("hash-table-p"), args[1]],
        ));
    }
    Ok((args[0], args[1]))
}

pub(crate) fn maphash_slot_len(table: Value) -> usize {
    table
        .as_hash_table()
        .map(LispHashTable::entry_slot_count)
        .unwrap_or(0)
}

pub(crate) fn maphash_entry_at_slot(table: Value, slot: usize) -> Option<(Value, Value)> {
    let table = table.as_hash_table()?;
    let entry = table.entry_at(slot)?;
    Some((entry.key, entry.value))
}

#[cfg(test)]
#[path = "tests/bignum_sxhash_test.rs"]
mod bignum_sxhash_tests;
#[cfg(test)]
#[path = "tests/hashtab_test.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/symbol_mode_sxhash_test.rs"]
mod symbol_mode_sxhash_tests;
