//! A switch plan must answer exactly what the hashed lookup answers, for
//! every table and dispatch value, and must never outlive a mutation.
use super::super::tests::jump_table_lookup::{
    Rng, TESTS, key_pool, list, probe_pool, random_table,
};
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::{HashTableWeakness, VecLikeType};
use malachite::integer::Integer;

fn sym(name: &str) -> Value {
    Value::from_sym_id(intern(name))
}

fn bits(v: Option<Value>) -> Option<usize> {
    v.map(|v| v.bits())
}

/// A table of `keys`, each targeting the byte offset `8 * (i + 1)`.
fn table_of(test: HashTableTest, keys: &[Value]) -> LispHashTable {
    let mut table = LispHashTable::new(test);
    for (i, key) in keys.iter().enumerate() {
        table.insert(
            key.to_hash_key_swp(&test, false),
            *key,
            Value::fixnum(8 * (i as i64 + 1)),
        );
    }
    table
}

/// Dispatch twice (the plan is built on the second) and report the shape.
fn planned(table: &LispHashTable) -> Option<PlanShape> {
    table.switch_target(Value::NIL, false);
    table.switch_target(Value::NIL, false);
    table.data.switch_plan.shape()
}

fn assert_answers_like_lookup(table: &LispHashTable, probes: &[Value], what: &str) -> usize {
    let mut checked = 0;
    for &probe in probes {
        for swp in [false, true] {
            assert_eq!(
                bits(table.switch_target(probe, swp)),
                bits(table.data.lookup(probe, table.test, swp).copied()),
                "{what}: {probe:?} under {:?} (swp {swp}), plan {:?}",
                table.test,
                table.data.switch_plan.shape()
            );
            checked += 1;
        }
    }
    checked
}

/// The plan's answer against the hashed lookup's over every table shape the
/// corpus can make: eq/eql/equal tables of 0-48 insertions with removal
/// holes and odd targets, plus tables no plan may take over (weak, user
/// test, keys positioned under `symbols-with-pos-enabled`). Probes include
/// fresh structural copies, a circular list, a 100,000-deep car chain,
/// shared structure and positioned symbols, each with
/// `symbols-with-pos-enabled` nil and t.
///
/// Mutations this catches: drop the position strip at nested leaves, admit an
/// identity key under `equal`, drop the tag prefilter's fixnum parity,
/// compare floats by value instead of bits.
#[test]
fn plan_answers_exactly_what_the_hashed_lookup_answers() {
    let mut ctx = Context::new();
    let keys = key_pool();
    let probes = probe_pool(&mut ctx, &keys);
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let mut checked = 0usize;
    let mut planned_shapes = std::collections::HashSet::new();
    let mut record = |table: &LispHashTable| {
        let shape = table.data.switch_plan.shape();
        assert!(shape.is_some(), "a dispatched table is planned");
        planned_shapes.insert(shape);
    };
    // Keys a plan can reproduce, so random tables of them are planned rather
    // than declined for one vector among the keys.
    let plannable: Vec<Value> = keys
        .iter()
        .copied()
        .filter(|key| {
            let mut probe = LispHashTable::new(HashTableTest::Equal);
            probe.insert(
                key.to_hash_key(&HashTableTest::Equal),
                *key,
                Value::fixnum(0),
            );
            planned(&probe) != Some(PlanShape::Generic)
        })
        .collect();
    assert!(plannable.len() + 3 <= keys.len() && plannable.len() > 60);
    for test in TESTS {
        for size in 0..=48 {
            for (pool, odd_targets) in [(&keys, true), (&plannable, false)] {
                let table = random_table(&mut rng, pool, test, size, odd_targets);
                checked += assert_answers_like_lookup(&table, &probes, "random table");
                record(&table);
            }
        }
        let dense = (0..20).map(Value::fixnum).collect::<Vec<_>>();
        let many = (0..40)
            .map(|i| sym(&format!("switch-plan-many-{i}")))
            .collect::<Vec<_>>();
        for table in [table_of(test, &dense), table_of(test, &many)] {
            checked += assert_answers_like_lookup(&table, &probes, "fixed table");
            record(&table);
        }
    }
    for shape in [
        PlanShape::Bits,
        PlanShape::DenseFixnum,
        PlanShape::Nodes,
        PlanShape::Hashed,
        PlanShape::Generic,
    ] {
        assert!(
            planned_shapes.contains(&Some(shape)),
            "the corpus never planned a {shape:?} table: {planned_shapes:?}"
        );
    }

    // Keys positioned under `symbols-with-pos-enabled`: the index keys the
    // bare symbol while the slot keeps the positioned object, so the small
    // identity scan and the hash disagree. The plan must answer like
    // `lookup` anyway (it declines such tables).
    let (a, b) = (sym("a"), sym("b"));
    let pa = ctx.tagged_heap.alloc_symbol_with_pos(a, Value::fixnum(1));
    for test in TESTS {
        for filler in [0usize, 40] {
            let mut table = LispHashTable::new(test);
            table.insert(pa.to_hash_key_swp(&test, true), pa, Value::fixnum(8));
            table.insert(b.to_hash_key_swp(&test, true), b, Value::fixnum(16));
            for i in 0..filler {
                let key = sym(&format!("switch-plan-filler-{i}"));
                table.insert(key.to_hash_key(&test), key, Value::fixnum(24));
            }
            checked += assert_answers_like_lookup(&table, &probes, "positioned keys");
        }
    }

    // Identity keys under a test that does not key by identity: an `eq`
    // table whose test changed afterwards (pdump restore rewrites the test
    // under `with_hash_table_mut`). The key objects themselves must still
    // answer like the hashed lookup, which materializes them structurally.
    let objects = [
        Value::string("s"),
        Value::make_float(1.5),
        list(&[a]),
        Value::bignum(Integer::from(1u128 << 70)),
        a,
    ];
    let mut object_probes = probes.clone();
    object_probes.extend(objects);
    for to in [HashTableTest::Eql, HashTableTest::Equal] {
        let mut table = table_of(HashTableTest::Eq, &objects);
        table.test = to;
        checked += assert_answers_like_lookup(&table, &object_probes, "retested table");
    }

    // Tables a plan must not take over.
    let mut weak = LispHashTable::new_with_options(
        HashTableTest::Eq,
        0,
        Some(HashTableWeakness::Key),
        1.5,
        0.8125,
    );
    let mut user = LispHashTable::new(HashTableTest::Equal);
    user.user_cmp_function = Some(sym("switch-plan-user-cmp"));
    user.user_hash_function = Some(sym("switch-plan-user-hash"));
    for (i, key) in keys.iter().take(20).enumerate() {
        weak.insert(
            key.to_hash_key(&HashTableTest::Eq),
            *key,
            Value::fixnum(i as i64),
        );
        user.insert(
            key.to_hash_key(&HashTableTest::Equal),
            *key,
            Value::fixnum(i as i64),
        );
    }
    checked += assert_answers_like_lookup(&weak, &probes, "weak table");
    checked += assert_answers_like_lookup(&user, &probes, "user-test table");
    assert_eq!(
        weak.data.switch_plan.shape(),
        None,
        "a weak table is never planned"
    );
    assert_eq!(user.data.switch_plan.shape(), Some(PlanShape::Generic));

    assert!(checked >= 20_000, "checked {checked}");
}

#[test]
fn plan_shapes_follow_the_key_set() {
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));

    let dense = table_of(
        HashTableTest::Eq,
        &(0..10).map(Value::fixnum).collect::<Vec<_>>(),
    );
    assert_eq!(planned(&dense), Some(PlanShape::DenseFixnum));
    let negative = table_of(
        HashTableTest::Eql,
        &[Value::fixnum(-4), Value::fixnum(-1), Value::fixnum(200)],
    );
    assert_eq!(planned(&negative), Some(PlanShape::DenseFixnum));

    let symbols = table_of(HashTableTest::Eq, &[a, b, c]);
    assert_eq!(planned(&symbols), Some(PlanShape::Bits));
    let Some(PlanSlot::Plan { body, .. }) = symbols.data.switch_plan.slot.get() else {
        panic!("a symbol table has a plan body");
    };
    assert_eq!(
        body.ranges[a.bits() & TAG_MASK],
        (0, 3),
        "a symbol scans the symbol keys"
    );
    for (tag, range) in body.ranges.iter().enumerate() {
        if tag != a.bits() & TAG_MASK {
            assert_eq!(range.0, range.1, "tag {tag} holds no key");
        }
    }

    let sparse = table_of(
        HashTableTest::Eq,
        &[
            Value::fixnum(0),
            Value::fixnum(1000),
            a,
            Value::NIL,
            Value::T,
        ],
    );
    assert_eq!(planned(&sparse), Some(PlanShape::Bits));
    let identity = table_of(
        HashTableTest::Eq,
        &[Value::string("s"), Value::make_float(1.5), list(&[a])],
    );
    assert_eq!(planned(&identity), Some(PlanShape::Bits));

    let structural = table_of(
        HashTableTest::Equal,
        &[list(&[a, b]), Value::cons(a, b), Value::string("s"), a],
    );
    assert_eq!(planned(&structural), Some(PlanShape::Nodes));
    let floats = table_of(
        HashTableTest::Eql,
        &[Value::make_float(0.0), Value::make_float(-0.0), a],
    );
    assert_eq!(planned(&floats), Some(PlanShape::Nodes));

    let many_symbols = table_of(
        HashTableTest::Eq,
        &(0..40)
            .map(|i| sym(&format!("switch-plan-many-{i}")))
            .collect::<Vec<_>>(),
    );
    assert_eq!(planned(&many_symbols), Some(PlanShape::Hashed));
    let many_lists = table_of(
        HashTableTest::Equal,
        &(0..20)
            .map(|i| list(&[a, Value::fixnum(i)]))
            .collect::<Vec<_>>(),
    );
    assert_eq!(planned(&many_lists), Some(PlanShape::Hashed));
    let wide_dense = table_of(
        HashTableTest::Eq,
        &(0..50).map(|i| Value::fixnum(i * 7)).collect::<Vec<_>>(),
    );
    assert_eq!(planned(&wide_dense), Some(PlanShape::Hashed));

    let vector = table_of(
        HashTableTest::Equal,
        &[Value::vector(vec![Value::fixnum(1)]), a],
    );
    assert_eq!(planned(&vector), Some(PlanShape::Generic));
    let bignum = table_of(
        HashTableTest::Eql,
        &[Value::bignum(Integer::from(1u128 << 64)), a],
    );
    assert_eq!(planned(&bignum), Some(PlanShape::Generic));
    let mut too_deep = a;
    for _ in 0..8 {
        too_deep = list(&[too_deep]);
    }
    assert_eq!(
        planned(&table_of(HashTableTest::Equal, &[too_deep])),
        Some(PlanShape::Generic),
        "car nesting past the pending-cdr stack"
    );
    let too_long = list(&(0..40).map(Value::fixnum).collect::<Vec<_>>());
    assert_eq!(
        planned(&table_of(HashTableTest::Equal, &[too_long])),
        Some(PlanShape::Generic),
        "a key program past MAX_KEY_NODES"
    );

    for odd in [
        Value::fixnum(-8),
        Value::NIL,
        Value::cons(Value::fixnum(8), Value::NIL),
    ] {
        let mut table = table_of(HashTableTest::Eq, &[a, b]);
        table.insert(c.to_hash_key(&HashTableTest::Eq), c, odd);
        assert_eq!(planned(&table), Some(PlanShape::Generic), "target {odd:?}");
        assert_eq!(bits(table.switch_target(c, false)), Some(odd.bits()));
    }

    let mut user = table_of(HashTableTest::Equal, &[a, b]);
    user.user_cmp_function = Some(sym("switch-plan-user-cmp"));
    assert_eq!(planned(&user), Some(PlanShape::Generic));

    let mut weak = LispHashTable::new_with_options(
        HashTableTest::Eq,
        0,
        Some(HashTableWeakness::Value),
        1.5,
        0.8125,
    );
    weak.insert(a.to_hash_key(&HashTableTest::Eq), a, Value::fixnum(8));
    for _ in 0..5 {
        assert_eq!(
            bits(weak.switch_target(a, false)),
            Some(Value::fixnum(8).bits())
        );
    }
    assert_eq!(
        weak.data.switch_plan.shape(),
        None,
        "a weak table's cache is never populated"
    );
}

/// The shapes answer, not just exist: a hit, a miss by tag, and a miss
/// within the tag, for each.
#[test]
fn every_plan_shape_hits_and_misses() {
    let (a, b) = (sym("a"), sym("b"));
    let cases: Vec<(LispHashTable, Vec<Value>)> = vec![
        (
            table_of(
                HashTableTest::Eq,
                &(0..10).map(Value::fixnum).collect::<Vec<_>>(),
            ),
            vec![
                Value::fixnum(0),
                Value::fixnum(9),
                Value::fixnum(10),
                Value::fixnum(-1),
                Value::fixnum(Value::MOST_NEGATIVE_FIXNUM),
                a,
            ],
        ),
        (
            table_of(HashTableTest::Eq, &[a, Value::fixnum(3), Value::NIL]),
            vec![
                a,
                b,
                Value::fixnum(3),
                Value::fixnum(4),
                Value::NIL,
                Value::T,
            ],
        ),
        (
            table_of(
                HashTableTest::Equal,
                &[
                    list(&[a, b]),
                    Value::cons(a, b),
                    list(&[list(&[a]), b]),
                    Value::string("s"),
                    Value::make_float(-0.0),
                ],
            ),
            vec![
                list(&[a, b]),
                Value::cons(a, b),
                list(&[list(&[a]), b]),
                list(&[list(&[b]), b]),
                list(&[a, b, a]),
                list(&[a]),
                Value::string("s"),
                Value::multibyte_string("s"),
                Value::string("t"),
                Value::make_float(-0.0),
                Value::make_float(0.0),
                Value::fixnum(0),
            ],
        ),
    ];
    for (table, probes) in &cases {
        planned(table);
        assert!(matches!(
            table.data.switch_plan.shape(),
            Some(PlanShape::DenseFixnum | PlanShape::Bits | PlanShape::Nodes)
        ));
        assert_answers_like_lookup(table, probes, "shape case");
        assert!(
            probes
                .iter()
                .any(|p| table.switch_target(*p, false).is_some())
        );
        assert!(
            probes
                .iter()
                .any(|p| table.switch_target(*p, false).is_none())
        );
    }
}

/// Every way a published table changes drops its plan, so the next dispatch
/// answers from the new contents.
#[test]
fn every_table_mutation_drops_the_plan() {
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));
    let eq = HashTableTest::Eq;
    let table = Value::hash_table(eq);
    let _ = table.with_hash_table_mut(|ht| {
        for (key, target) in [(a, 8), (b, 16)] {
            ht.insert(key.to_hash_key(&eq), key, Value::fixnum(target));
        }
    });
    let ht = || table.as_hash_table().expect("a hash table");
    let target = |key: Value| bits(ht().switch_target(key, false)).map(|v| v >> 2);
    let replan = || {
        target(Value::NIL);
        target(Value::NIL);
        assert!(ht().data.switch_plan.shape().is_some(), "planned again");
    };

    replan();
    assert_eq!(target(c), None);
    // puthash of a new key.
    let _ = table.with_hash_table_mut(|ht| ht.insert(c.to_hash_key(&eq), c, Value::fixnum(8)));
    assert_eq!(
        ht().data.switch_plan.shape(),
        None,
        "puthash drops the plan"
    );
    assert_eq!(target(c), Some(8));
    replan();
    // puthash of an existing key with a new target.
    let _ = table.with_hash_table_mut(|ht| ht.insert(a.to_hash_key(&eq), a, Value::fixnum(24)));
    assert_eq!(target(a), Some(24));
    replan();
    // remhash.
    let _ = table.with_hash_table_mut(|ht| ht.data.remove_by_value(b, eq, false));
    assert_eq!(target(b), None);
    replan();
    // clrhash.
    let _ = table.with_hash_table_mut(|ht| ht.data.clear());
    assert_eq!(target(a), None);
    replan();
    // A wholesale replacement.
    let mut other = LispHashTable::new(eq);
    other.insert(b.to_hash_key(&eq), b, Value::fixnum(40));
    assert!(table.replace_hash_table(other));
    assert_eq!(ht().data.switch_plan.shape(), None);
    assert_eq!(target(b), Some(40));
    replan();
    // A test change.
    let _ = table.with_hash_table_mut(|ht| ht.test = HashTableTest::Eql);
    assert_eq!(ht().data.switch_plan.shape(), None);
    assert_eq!(target(b), Some(40));
    replan();
    // The weak sweep's removal path.
    let _ = table.with_hash_table_mut(|ht| ht.data.retain_entries(|_, _| false));
    assert_eq!(target(b), None);

    // The storage's bulk mutators drop the plan themselves, without the
    // `with_hash_table_mut` choke point.
    type Mutator = fn(&mut LispHashTable);
    let bulk: [(&str, Mutator); 3] = [
        ("clear", |t| t.data.clear()),
        ("retain", |t| t.data.retain(|_, _| false)),
        ("retain_entries", |t| t.data.retain_entries(|_, _| false)),
    ];
    for (name, mutate) in bulk {
        let mut owned = table_of(eq, &[a, b]);
        assert!(planned(&owned).is_some());
        mutate(&mut owned);
        assert_eq!(owned.data.switch_plan.shape(), None, "{name}");
        assert_eq!(owned.switch_target(a, false), None, "{name}");
    }

    // `copy-hash-table` starts cold.
    let original = table_of(eq, &[a]);
    assert!(planned(&original).is_some());
    assert_eq!(original.clone().data.switch_plan.shape(), None);
}

/// A jump table mutated between dispatches (not what the byte compiler
/// emits) is replanned at most MAX_REBUILDS times, then stays on the hashed
/// lookup, still exact.
#[test]
fn a_table_mutated_between_dispatches_stops_replanning() {
    let (a, b) = (sym("a"), sym("b"));
    let eq = HashTableTest::Eq;
    let table = Value::hash_table(eq);
    let _ = table.with_hash_table_mut(|ht| ht.insert(a.to_hash_key(&eq), a, Value::fixnum(8)));
    let ht = || table.as_hash_table().expect("a hash table");
    for round in 0..(MAX_REBUILDS as i64 + 3) {
        ht().switch_target(a, false);
        ht().switch_target(b, false);
        let shape = ht().data.switch_plan.shape();
        if round < MAX_REBUILDS as i64 {
            assert_eq!(shape, Some(PlanShape::Bits), "round {round}");
        } else {
            assert_eq!(shape, Some(PlanShape::Generic), "round {round}");
        }
        let _ = table.with_hash_table_mut(|ht| {
            ht.insert(b.to_hash_key(&eq), b, Value::fixnum(8 * (round + 2)))
        });
        assert_eq!(
            bits(ht().switch_target(b, false)),
            Some(Value::fixnum(8 * (round + 2)).bits())
        );
    }
}

/// `setcar` on a key cons AFTER the plan is built: the plan still answers for
/// the key's old shape, while the hashed lookup compares the live key object
/// as GNU does (`hash_find_with_hash`, src/fns.c) and no longer matches it.
/// A jump table's keys are byte-code constants, so this is the plan's one
/// documented exception; the untouched key still answers alike.
#[test]
fn a_key_mutated_after_planning_answers_from_the_plan() {
    let (a, b, z) = (sym("a"), sym("b"), sym("z"));
    let key = list(&[a, b]);
    let table = table_of(HashTableTest::Equal, &[key, list(&[z])]);
    assert_eq!(planned(&table), Some(PlanShape::Nodes));
    key.set_car(z);
    assert_eq!(
        bits(table.switch_target(list(&[a, b]), false)),
        Some(Value::fixnum(8).bits())
    );
    assert_eq!(
        bits(table.data.lookup(list(&[a, b]), table.test, false).copied()),
        None
    );
    assert_eq!(table.switch_target(list(&[z, b]), false), None);
    assert_answers_like_lookup(&table, &[list(&[z])], "the untouched key");
}

#[test]
fn first_dispatch_does_not_build() {
    let (a, b) = (sym("a"), sym("b"));
    let table = table_of(HashTableTest::Eq, &[a, b]);
    let before = plan_builds_for_test();
    assert_eq!(
        bits(table.switch_target(a, false)),
        Some(Value::fixnum(8).bits())
    );
    assert_eq!(
        plan_builds_for_test(),
        before,
        "one dispatch builds nothing"
    );
    assert_eq!(table.data.switch_plan.shape(), None);
    assert_eq!(
        bits(table.switch_target(b, false)),
        Some(Value::fixnum(16).bits())
    );
    assert_eq!(plan_builds_for_test(), before + 1, "the second builds once");
    for _ in 0..10 {
        table.switch_target(a, true);
    }
    assert_eq!(plan_builds_for_test(), before + 1, "and never again");
}

/// Positioned symbols with `symbols-with-pos-enabled`: stripped at the top
/// level under every test, and at every leaf under `equal`.
#[test]
fn positioned_symbols_match_their_bare_symbols_only_under_swp() {
    let mut ctx = Context::new();
    let (a, b) = (sym("a"), sym("b"));
    let pa = ctx.tagged_heap.alloc_symbol_with_pos(a, Value::fixnum(1));
    let pb = ctx.tagged_heap.alloc_symbol_with_pos(b, Value::fixnum(2));
    let pnil = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::NIL, Value::fixnum(3));
    let immediate = table_of(HashTableTest::Eq, &[a, Value::NIL]);
    let structural = table_of(HashTableTest::Equal, &[list(&[a, b]), a]);
    let probes = [
        pa,
        pnil,
        list(&[pa, pb]),
        list(&[a, pb]),
        Value::cons(pa, pb),
    ];
    for table in [&immediate, &structural] {
        planned(table);
        assert_answers_like_lookup(table, &probes, "positioned");
    }
    assert_eq!(immediate.switch_target(pa, false), None);
    assert_eq!(
        bits(immediate.switch_target(pa, true)),
        Some(Value::fixnum(8).bits())
    );
    assert_eq!(
        bits(immediate.switch_target(pnil, true)),
        Some(Value::fixnum(16).bits())
    );
    assert_eq!(structural.switch_target(list(&[pa, pb]), false), None);
    assert_eq!(
        bits(structural.switch_target(list(&[pa, pb]), true)),
        Some(Value::fixnum(8).bits())
    );
    assert!(pa.veclike_type() == Some(VecLikeType::SymbolWithPos));
}

/// Every pass through `with_hash_table_mut` raises the table object's
/// epoch -- a wholesale replacement too, by a fresh table (epoch 0) or by a
/// copy carrying the same or a higher epoch -- and nothing else changes it:
/// JIT code guarding a jump table on its epoch never sees a compiled epoch
/// again once the table has changed.
#[test]
fn every_table_mutation_raises_the_epoch() {
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));
    let eq = HashTableTest::Eq;
    let table = Value::hash_table(eq);
    let epoch = || {
        table
            .as_hash_table()
            .expect("a hash table")
            .data
            .switch_epoch
    };
    let last = std::cell::Cell::new(epoch());
    let raised = |what: &str| {
        let now = epoch();
        assert!(now > last.get(), "{what}: epoch {now} after {}", last.get());
        last.set(now);
    };
    let _ = table.with_hash_table_mut(|ht| ht.insert(a.to_hash_key(&eq), a, Value::fixnum(8)));
    raised("puthash of a new key");
    let _ = table.with_hash_table_mut(|ht| ht.insert(a.to_hash_key(&eq), a, Value::fixnum(16)));
    raised("puthash of an existing key");
    let _ = table.with_hash_table_mut(|ht| ht.insert(b.to_hash_key(&eq), b, Value::fixnum(8)));
    raised("another puthash");
    let _ = table.with_hash_table_mut(|ht| ht.data.remove_by_value(b, eq, false));
    raised("remhash");
    let _ = table.with_hash_table_mut(|ht| ht.data.clear());
    raised("clrhash");
    let _ = table.with_hash_table_mut(|ht| ht.test = HashTableTest::Eql);
    raised("a test change");
    let _ = table.with_hash_table_mut(|_| ());
    raised("a mutation that changes nothing");
    assert!(table.replace_hash_table(table_of(eq, &[c])));
    raised("a replacement by a fresh table");
    let mut copy = table.as_hash_table().expect("a hash table").clone();
    assert_eq!(
        copy.data.switch_epoch,
        last.get(),
        "a copy carries the epoch"
    );
    assert!(table.replace_hash_table(copy.clone()));
    raised("a replacement by a copy of the same epoch");
    copy.data.switch_epoch = last.get() + 1000;
    assert!(table.replace_hash_table(copy));
    raised("a replacement by a copy of a higher epoch");

    // Reads, dispatches and plan builds leave it alone.
    let ht = table.as_hash_table().expect("a hash table");
    for _ in 0..3 {
        ht.switch_target(c, false);
        ht.switch_target(a, true);
    }
    assert!(ht.data.switch_plan.shape().is_some(), "planned");
    assert!(ht.data.get(&c.to_hash_key(&eq)).is_some());
    assert_eq!(epoch(), last.get(), "reads do not move the epoch");
}

/// `SWITCH_EPOCH_OFFSET` addresses the epoch through a hash table value's
/// untagged pointer, which is how JIT code reads it.
#[test]
fn the_epoch_offset_reads_the_epoch() {
    let table = Value::hash_table(HashTableTest::Equal);
    for _ in 0..5 {
        let _ = table.with_hash_table_mut(|_| ());
    }
    let object = table.as_veclike_ptr().expect("a veclike") as *const u8;
    // SAFETY: a live hash table object; the offset is `offset_of!` into it.
    let read = unsafe { *(object.add(SWITCH_EPOCH_OFFSET) as *const u64) };
    assert_eq!(
        read,
        table
            .as_hash_table()
            .expect("a hash table")
            .data
            .switch_epoch
    );
    assert_eq!(read, 5);
}
