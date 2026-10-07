//! What a `switch` jump table answers, pinned over the key shapes
//! `byte-compile-cond-jump-table` (lisp/emacs-lisp/bytecomp.el) emits and the
//! dispatch values a program can hand it.
//!
//! Both `switch` tiers answer through [`HashTableStorage::lookup`]. Over this
//! corpus that must be exactly what the materialized [`HashKey`] finds: the
//! in-place probe, the small identity scan and the materializing fallback are
//! three routes to one answer. The corpus and the table builder are shared
//! with the switch-plan tests, which hold the plan to the same answer.
use super::super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::heap_types::LispString;
use malachite::integer::Integer;

pub(in crate::emacs_core::value) const TESTS: [HashTableTest; 3] =
    [HashTableTest::Eq, HashTableTest::Eql, HashTableTest::Equal];

/// xorshift64: deterministic, so a failure names a reproducible table.
pub(in crate::emacs_core::value) struct Rng(pub(in crate::emacs_core::value) u64);

impl Rng {
    pub(in crate::emacs_core::value) fn below(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }
}

pub(in crate::emacs_core::value) fn list(items: &[Value]) -> Value {
    items
        .iter()
        .rev()
        .fold(Value::NIL, |tail, item| Value::cons(*item, tail))
}

fn sym(name: &str) -> Value {
    Value::from_sym_id(intern(name))
}

/// Every key shape a jump table can hold: nil and t, fixnums (negative,
/// extreme, a dense run, sparse), symbols and a keyword, floats whose bits
/// differ where their values do not (0.0/-0.0, two NaN payloads), strings
/// (unibyte, multibyte, the same bytes read two ways, raw bytes, propertized,
/// empty), conses (proper, dotted, nested, eight deep, holding strings and
/// floats), and the shapes a plan cannot reproduce (vector, bignum).
pub(in crate::emacs_core::value) fn key_pool() -> Vec<Value> {
    let (a, b) = (sym("a"), sym("b"));
    let mut deep = a;
    for _ in 0..8 {
        deep = list(&[deep]);
    }
    let mut keys = vec![
        Value::NIL,
        Value::T,
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(3),
        Value::fixnum(-1),
        Value::fixnum(-4),
        Value::fixnum(97),
        Value::fixnum(1000),
        Value::fixnum(Value::MOST_POSITIVE_FIXNUM),
        Value::fixnum(Value::MOST_NEGATIVE_FIXNUM),
        a,
        b,
        sym("c"),
        Value::keyword("jump-key"),
        Value::make_float(0.0),
        Value::make_float(-0.0),
        Value::make_float(1.0),
        Value::make_float(1.5),
        Value::make_float(f64::from_bits(0x7ff8_0000_0000_0000)),
        Value::make_float(f64::from_bits(0x7ff8_0000_0000_0001)),
        Value::string("alpha"),
        Value::multibyte_string("beta"),
        Value::string(""),
        Value::string("\u{e9}"),
        Value::heap_string(LispString::from_unibyte(vec![0xc3, 0xa9])),
        Value::heap_string(LispString::from_unibyte(vec![0xff])),
        Value::heap_string(LispString::from_emacs_bytes(vec![0xc1, 0xbf])),
        Value::string_with_text_properties(
            "gamma",
            vec![StringTextPropertyRun {
                start: 0,
                end: 5,
                plist: list(&[sym("face"), sym("bold")]),
            }],
        ),
        list(&[a, b]),
        Value::cons(a, b),
        list(&[list(&[a]), b]),
        list(&[a]),
        deep,
        list(&[Value::fixnum(1), Value::string("s"), Value::make_float(2.5)]),
        list(&(0..12).map(Value::fixnum).collect::<Vec<_>>()),
        Value::vector(vec![Value::fixnum(1), Value::fixnum(2)]),
        Value::bignum(Integer::from(1u128 << 64)),
    ];
    keys.extend((10..=20).map(Value::fixnum));
    keys.extend((0..24).map(|i| sym(&format!("jump-sym-{i}"))));
    keys
}

/// A structurally equal copy of `v` built from fresh objects: new conses,
/// strings, floats and bignums (the dispatch value is never `eq` to the key
/// in `pcase` over computed data).
pub(in crate::emacs_core::value) fn fresh_copy(v: Value) -> Value {
    match v.kind() {
        ValueKind::Cons => Value::cons(fresh_copy(v.cons_car()), fresh_copy(v.cons_cdr())),
        ValueKind::String => Value::heap_string(v.as_lisp_string().expect("a string").clone()),
        ValueKind::Float => Value::make_float(f64::from_bits(v.xfloat().to_bits())),
        ValueKind::Veclike(VecLikeType::Bignum) => {
            Value::bignum(v.as_bignum().expect("a bignum").clone())
        }
        ValueKind::Veclike(VecLikeType::Vector) => Value::vector(
            v.as_vector_data()
                .expect("a vector")
                .iter()
                .map(|item| fresh_copy(*item))
                .collect(),
        ),
        _ => v,
    }
}

/// Dispatch values: every key, a fresh structural copy of every key, near
/// misses, and the shapes a lookup must survive -- a circular list, a
/// 100,000-deep car chain, shared (DAG) structure, and positioned symbols at
/// the top level and nested inside conses.
pub(in crate::emacs_core::value) fn probe_pool(ctx: &mut Context, keys: &[Value]) -> Vec<Value> {
    let (a, b) = (sym("a"), sym("b"));
    let mut probes: Vec<Value> = keys.to_vec();
    probes.extend(keys.iter().map(|k| fresh_copy(*k)));

    let circular = list(&[a, b]);
    circular.cons_cdr().set_cdr(circular);
    let mut chain = a;
    for _ in 0..100_000 {
        chain = Value::cons(chain, Value::NIL);
    }
    let shared = list(&[a]);
    let pa = ctx.tagged_heap.alloc_symbol_with_pos(a, Value::fixnum(3));
    let pb = ctx.tagged_heap.alloc_symbol_with_pos(b, Value::fixnum(5));
    let pnil = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::NIL, Value::fixnum(7));
    let pkey = ctx
        .tagged_heap
        .alloc_symbol_with_pos(sym("jump-sym-3"), Value::fixnum(9));
    probes.extend([
        circular,
        chain,
        list(&[shared, shared]),
        list(&[list(&[a]), list(&[a])]),
        pa,
        pb,
        pnil,
        pkey,
        list(&[pa, b]),
        list(&[a, pb]),
        list(&[pa, pb]),
        Value::cons(pa, pb),
        list(&[list(&[pa]), b]),
        list(&[pa]),
        sym("d"),
        Value::fixnum(2),
        Value::fixnum(21),
        Value::fixnum(9),
        Value::make_float(2.0),
        Value::make_float(f64::from_bits(0xfff8_0000_0000_0000)),
        Value::string("zzz"),
        Value::string("alph"),
        list(&[a, b, sym("c")]),
        list(&[b]),
        Value::cons(b, a),
        Value::vector(vec![a, b]),
        Value::bignum(Integer::from(1u128 << 65)),
    ]);
    probes
}

/// Weird targets a program can `puthash` into a jump table: a negative
/// fixnum and non-fixnums.
fn odd_target(rng: &mut Rng) -> Value {
    [
        Value::fixnum(-3),
        Value::NIL,
        Value::cons(Value::fixnum(1), Value::NIL),
    ][rng.below(3)]
}

/// A jump table of `size` insertions drawn from `keys`, keyed the way
/// `puthash` keys them with `symbols-with-pos-enabled` nil. Targets are byte
/// offsets (non-negative fixnums) unless `odd_targets` mixes in others, and a
/// sixth of the insertions are followed by a removal, which leaves holes.
pub(in crate::emacs_core::value) fn random_table(
    rng: &mut Rng,
    keys: &[Value],
    test: HashTableTest,
    size: usize,
    odd_targets: bool,
) -> LispHashTable {
    let mut table = LispHashTable::new(test);
    for i in 0..size {
        let key = keys[rng.below(keys.len())];
        let target = if odd_targets && rng.below(10) == 0 {
            odd_target(rng)
        } else {
            Value::fixnum(3 * i as i64 + 1)
        };
        table.insert(key.to_hash_key_swp(&test, false), key, target);
        if rng.below(6) == 0 {
            let gone = keys[rng.below(keys.len())];
            table.data.remove(&gone.to_hash_key_swp(&test, false));
        }
    }
    table
}

fn bits(v: Option<&Value>) -> Option<usize> {
    v.map(|v| v.bits())
}

#[test]
fn jump_table_lookup_matches_the_materialized_key() {
    let mut ctx = Context::new();
    let keys = key_pool();
    let probes = probe_pool(&mut ctx, &keys);
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut checked = 0usize;
    for test in TESTS {
        for size in 0..=48 {
            let table = random_table(&mut rng, &keys, test, size, true);
            for &probe in &probes {
                for swp in [false, true] {
                    let by_key = bits(table.data.get(&probe.to_hash_key_swp(&test, swp)));
                    assert_eq!(
                        bits(table.data.lookup(probe, test, swp)),
                        by_key,
                        "{probe:?} under {test:?} (swp {swp}) in a {size}-insertion table"
                    );
                    checked += 1;
                }
            }
        }
    }
    assert!(checked >= 20_000, "checked {checked}");
}

/// A key cons mutated after insertion answers for neither shape, as in GNU:
/// the entry stays filed under the hash of the key as it was (`h->hash[i]`),
/// and a candidate is compared with `equal` against the LIVE key object
/// (`hash_find_with_hash`, src/fns.c). So the old shape hashes alike but is
/// no longer `equal`, and the new shape hashes elsewhere.
#[test]
fn a_key_mutated_after_insertion_answers_like_gnu() {
    let (a, b, z) = (sym("a"), sym("b"), sym("z"));
    let key = list(&[a, b]);
    let mut table = LispHashTable::new(HashTableTest::Equal);
    table.insert(
        key.to_hash_key_swp(&HashTableTest::Equal, false),
        key,
        Value::fixnum(8),
    );
    key.set_car(z);
    let answer = |probe: Value| bits(table.data.lookup(probe, HashTableTest::Equal, false));
    assert_eq!(
        answer(list(&[a, b])),
        None,
        "the key object now reads (z b)"
    );
    assert_eq!(answer(list(&[z, b])), None, "filed under the hash of (a b)");
}
