//! Jump tables answered inline by the JIT behind their mutation epoch
//! (`switch_dispatch::InlineSwitch`, P0.5 phase 2). Every answer must be the
//! hashed lookup's -- the interpreter's -- for every table the inline form
//! takes, every dispatch value and both settings of
//! `symbols-with-pos-enabled`; a table changed since the compile must be
//! answered by the shim; and the knob off must leave the shim alone.

use super::switch_dispatch::inline_switch_sites_for_test;
use super::*;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::intern;
use crate::emacs_core::value::HashTableTest;

fn sym(name: &str) -> Value {
    Value::from_sym_id(intern(name))
}

fn list(items: &[Value]) -> Value {
    Value::list(items.to_vec())
}

/// Instruction index of target `j`'s arm in [`dispatch_leaf`]'s body.
fn arm(j: usize) -> usize {
    5 + 2 * j
}

/// Byte offset of target `j` in the jump table.
fn offset(j: usize) -> i64 {
    8 + 4 * j as i64
}

/// A jump table mapping each `(key, j)` to target `j`'s byte offset.
fn jump_table(test: HashTableTest, keys: &[(Value, usize)]) -> Value {
    let table = Value::hash_table(test);
    let _ = table.with_hash_table_mut(|ht| {
        for &(key, j) in keys {
            ht.insert(key.to_hash_key(&ht.test), key, Value::fixnum(offset(j)));
        }
    });
    table
}

/// `(lambda (x) (switch x TABLE))` with `targets` arms: target `j` answers
/// `j`, a miss answers -1.
fn dispatch_leaf(table: Value, targets: usize) -> CompiledLeaf {
    let mut ops = vec![
        Op::StackRef(0), // [x x]
        Op::Constant(0), // [x x table]
        Op::Switch,      // [x]
        Op::Constant(1), // miss: -1
        Op::Return,
    ];
    let mut constants = vec![table, Value::fixnum(-1)];
    let mut map = Vec::new();
    for j in 0..targets {
        assert_eq!(ops.len(), arm(j));
        ops.push(Op::Constant(2 + j as u16));
        ops.push(Op::Return);
        constants.push(Value::fixnum(j as i64));
        map.push(GnuByteOffsetMapEntry::new(offset(j) as usize, arm(j)));
    }
    lower_leaf_with_map(&ops, &constants, 1, Some(&map)).expect("switch body compiles")
}

/// What the interpreter's lookup answers for `value`: the target index, or
/// -1 for a miss.
fn lookup_answer(table: Value, value: Value, swp: bool) -> i64 {
    let ht = table.as_hash_table().expect("a hash table");
    match ht.data.lookup(value, ht.test, swp) {
        Some(target) => (target.as_fixnum().expect("a byte offset") - 8) / 4,
        None => -1,
    }
}

/// Run the leaf on every probe under both `symbols-with-pos-enabled`
/// settings and require the lookup's answer. Returns the checks made.
fn check_like_lookup(
    ctx: &mut Context,
    leaf: &CompiledLeaf,
    table: Value,
    probes: &[Value],
    what: &str,
) -> usize {
    let mut checked = 0;
    for swp in [false, true] {
        ctx.symbols_with_pos_enabled = swp;
        for &probe in probes {
            let want = lookup_answer(table, probe, swp);
            let got = leaf.call(ctx as *mut Context as *mut u8, &[probe]);
            assert_eq!(
                got,
                NativeRun::Ok(Value::fixnum(want).bits()),
                "{what}: {} under symbols-with-pos-enabled {swp}",
                crate::emacs_core::print::print_value(&probe)
            );
            checked += 1;
        }
    }
    ctx.symbols_with_pos_enabled = false;
    checked
}

/// Values of every shape: the keys themselves, fresh copies of them, near
/// misses, immediates, heap objects, cyclic structure and positioned
/// symbols (bare and nested) standing for key symbols.
fn probes(ctx: &mut Context, keys: &[Value]) -> Vec<Value> {
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));
    let mut probes = keys.to_vec();
    probes.extend(keys.iter().map(|&key| copy_tree(key)));
    let circular = list(&[a, b]);
    circular.cons_cdr().set_cdr(circular);
    let self_car = list(&[a]);
    self_car.set_car(self_car);
    let pa = ctx.tagged_heap.alloc_symbol_with_pos(a, Value::fixnum(3));
    let pb = ctx.tagged_heap.alloc_symbol_with_pos(b, Value::fixnum(5));
    let pnil = ctx
        .tagged_heap
        .alloc_symbol_with_pos(Value::NIL, Value::fixnum(7));
    let mut deep = a;
    for _ in 0..40 {
        deep = list(&[deep]);
    }
    probes.extend([
        Value::NIL,
        Value::T,
        a,
        b,
        c,
        sym(":k"),
        Value::fixnum(0),
        Value::fixnum(1),
        Value::fixnum(-1),
        Value::fixnum(7),
        Value::fixnum(97),
        Value::fixnum(i64::MAX >> 3),
        Value::fixnum(-(i64::MAX >> 3) - 1),
        Value::make_float(1.0),
        Value::string("a"),
        Value::vector(vec![a, b]),
        list(&[a]),
        list(&[a, b]),
        list(&[a, b, c]),
        list(&[a, b, Value::NIL]),
        Value::cons(a, b),
        Value::cons(a, Value::cons(b, c)),
        list(&[list(&[a]), b]),
        list(&[a, list(&[b])]),
        list(&[Value::NIL]),
        list(&[b]),
        list(&[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
        circular,
        self_car,
        deep,
        pa,
        pb,
        pnil,
        list(&[pa, b]),
        list(&[a, pb]),
        list(&[pa]),
        Value::cons(pa, pb),
    ]);
    probes
}

/// A structural copy: equal, never eq (for a cons).
fn copy_tree(v: Value) -> Value {
    if v.is_cons() {
        Value::cons(copy_tree(v.cons_car()), copy_tree(v.cons_cdr()))
    } else {
        v
    }
}

/// Every table shape the inline form takes answers exactly as the lookup
/// does: bit keys under eq/eql/equal (few keys: a compare or two; many: a
/// binary search), dense fixnums, nil among the keys (a zero test), keys
/// sharing a target, and equal tables of cons trees that share prefixes,
/// dotted and nested, mixed with immediates. Tables the inline form does
/// not take (string, float, heap-identity keys) keep the shim, still exact.
///
/// Mutations this catches: a cons arm without its tag test, a trie that
/// forgets pending cdrs or loads the car for the cdr, a miss that skips the
/// symbols-with-pos check, an immediate key taken for a heap one.
#[test]
fn inline_dispatch_answers_like_the_lookup_for_every_table_shape() {
    let mut ctx = Context::new_minimal_vm_harness();
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));
    let many: Vec<Value> = (0..12).map(|n| sym(&format!("jit-swi-{n}"))).collect();
    let fixnums = |ns: &[i64]| -> Vec<Value> { ns.iter().map(|&n| Value::fixnum(n)).collect() };
    let mut inline_tables: Vec<(&str, HashTableTest, Vec<(Value, usize)>)> = vec![
        ("one symbol", HashTableTest::Eq, vec![(a, 0)]),
        (
            "three symbols",
            HashTableTest::Eq,
            vec![(a, 0), (b, 1), (c, 2)],
        ),
        (
            "nil, t and a keyword",
            HashTableTest::Eq,
            vec![(Value::NIL, 0), (Value::T, 1), (sym(":k"), 2)],
        ),
        (
            "keys sharing a target",
            HashTableTest::Eq,
            vec![(a, 0), (b, 0), (c, 1)],
        ),
        (
            "dense fixnums",
            HashTableTest::Eql,
            fixnums(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9])
                .into_iter()
                .enumerate()
                .map(|(j, k)| (k, j))
                .collect(),
        ),
        (
            "sparse and extreme fixnums",
            HashTableTest::Eql,
            fixnums(&[-4, 97, 1000, i64::MAX >> 3, -(i64::MAX >> 3) - 1, 0])
                .into_iter()
                .enumerate()
                .map(|(j, k)| (k, j))
                .collect(),
        ),
    ];
    inline_tables.push((
        "many symbols",
        HashTableTest::Eq,
        many.iter().enumerate().map(|(j, &k)| (k, j)).collect(),
    ));
    inline_tables.push((
        "elb-pcase's conses",
        HashTableTest::Equal,
        vec![(list(&[a, b]), 0), (list(&[a]), 1)],
    ));
    inline_tables.push((
        "cons trees sharing prefixes, dotted and nested, with immediates",
        HashTableTest::Equal,
        vec![
            (list(&[a, b]), 0),
            (Value::cons(a, b), 1),
            (list(&[list(&[a]), b]), 2),
            (list(&[a]), 3),
            (list(&[a, list(&[b])]), 4),
            (list(&[Value::NIL]), 5),
            (Value::cons(a, Value::cons(b, c)), 6),
            (
                list(&[Value::fixnum(1), Value::fixnum(2), Value::fixnum(3)]),
                7,
            ),
            (a, 8),
            (Value::fixnum(7), 9),
            (Value::NIL, 10),
        ],
    ));
    let mut checked = 0;
    for (what, test, keys) in &inline_tables {
        let table = jump_table(*test, keys);
        let targets = keys.iter().map(|&(_, j)| j).max().map_or(0, |j| j + 1);
        let before = inline_switch_sites_for_test();
        let leaf = dispatch_leaf(table, targets);
        assert_eq!(
            inline_switch_sites_for_test(),
            before + 1,
            "{what}: answered inline"
        );
        let key_values: Vec<Value> = keys.iter().map(|&(k, _)| k).collect();
        let probes = probes(&mut ctx, &key_values);
        checked += check_like_lookup(&mut ctx, &leaf, table, &probes, what);
    }

    let shim_tables: Vec<(&str, HashTableTest, Vec<(Value, usize)>)> = vec![
        (
            "a string key",
            HashTableTest::Equal,
            vec![(Value::string("a"), 0), (a, 1)],
        ),
        (
            "a float key",
            HashTableTest::Eql,
            vec![(Value::make_float(1.0), 0), (Value::fixnum(1), 1)],
        ),
        (
            "a heap identity key",
            HashTableTest::Eq,
            vec![(list(&[a]), 0), (b, 1)],
        ),
        (
            "a cons of a string",
            HashTableTest::Equal,
            vec![(list(&[a, Value::string("s")]), 0)],
        ),
    ];
    for (what, test, keys) in &shim_tables {
        let table = jump_table(*test, keys);
        let targets = keys.iter().map(|&(_, j)| j).max().map_or(0, |j| j + 1);
        let before = inline_switch_sites_for_test();
        let leaf = dispatch_leaf(table, targets);
        assert_eq!(
            inline_switch_sites_for_test(),
            before,
            "{what}: keeps the shim"
        );
        let mut key_values: Vec<Value> = keys.iter().map(|&(k, _)| k).collect();
        key_values.push(Value::string("a"));
        key_values.push(Value::make_float(1.0));
        let probes = probes(&mut ctx, &key_values);
        checked += check_like_lookup(&mut ctx, &leaf, table, &probes, what);
    }
    assert!(checked > 1000, "{checked} checks");
}

/// A table changed after the compile fails the epoch guard, and the shim
/// answers from the new contents: a new key, a retargeted key, a removed
/// key, a cleared table, a wholesale replacement -- and a target outside
/// the compiled set (the table's values at compile time; arm 2 exists in
/// the body but no key targeted it) still raises the stale-table signal.
#[test]
fn a_changed_table_is_answered_by_the_shim() {
    let mut ctx = Context::new_minimal_vm_harness();
    let ctx_ptr = &mut ctx as *mut Context as *mut u8;
    let (a, b, c) = (sym("a"), sym("b"), sym("c"));
    let eq = HashTableTest::Eq;
    let table = jump_table(eq, &[(a, 0), (b, 1)]);
    let before = inline_switch_sites_for_test();
    let leaf = dispatch_leaf(table, 3);
    assert_eq!(inline_switch_sites_for_test(), before + 1);
    let answer = |arg: Value| match leaf.call(ctx_ptr, &[arg]) {
        NativeRun::Ok(bits) => Value::from_bits(bits).as_fixnum(),
        other => panic!("expected a native answer, got {other:?}"),
    };
    let put = |key: Value, target: Value| {
        let _ = table.with_hash_table_mut(|ht| ht.insert(key.to_hash_key(&eq), key, target));
    };
    assert_eq!(
        (answer(a), answer(b), answer(c)),
        (Some(0), Some(1), Some(-1))
    );
    put(c, Value::fixnum(offset(1)));
    assert_eq!(answer(c), Some(1), "a new key");
    put(a, Value::fixnum(offset(1)));
    assert_eq!(answer(a), Some(1), "a retargeted key");
    let _ = table.with_hash_table_mut(|ht| ht.data.remove_by_value(b, eq, false));
    assert_eq!(answer(b), Some(-1), "a removed key");
    let _ = table.with_hash_table_mut(|ht| ht.data.clear());
    assert_eq!(
        (answer(a), answer(c)),
        (Some(-1), Some(-1)),
        "a cleared table"
    );
    assert!(table.replace_hash_table({
        let fresh = jump_table(eq, &[(b, 0)]);
        fresh.as_hash_table().expect("a hash table").clone()
    }));
    assert_eq!(
        (answer(a), answer(b)),
        (Some(-1), Some(0)),
        "a replaced table"
    );
    put(a, Value::fixnum(offset(2)));
    assert_eq!(
        leaf.call(ctx_ptr, &[a]),
        NativeRun::Signal,
        "an uncompiled target"
    );
    match take_pending_flow().expect("the stale-table signal is stashed") {
        crate::emacs_core::error::Flow::Signal(sig) => assert_eq!(sig.symbol_name(), "error"),
        other => panic!("expected an error signal, got {other:?}"),
    }
}

/// `symbols-with-pos-enabled`: a positioned symbol misses every inline
/// compare, and the miss asks the shim, which strips it -- at the top level
/// under eq, at every level under equal. With it off, the same values miss.
#[test]
fn an_inline_miss_asks_the_shim_under_symbols_with_pos() {
    let mut ctx = Context::new_minimal_vm_harness();
    let (a, b) = (sym("a"), sym("b"));
    let pa = ctx.tagged_heap.alloc_symbol_with_pos(a, Value::fixnum(3));
    let pb = ctx.tagged_heap.alloc_symbol_with_pos(b, Value::fixnum(5));
    let eq_table = jump_table(HashTableTest::Eq, &[(a, 0), (b, 1)]);
    let equal_table = jump_table(HashTableTest::Equal, &[(list(&[a, b]), 0), (list(&[a]), 1)]);
    let before = inline_switch_sites_for_test();
    let eq_leaf = dispatch_leaf(eq_table, 2);
    let equal_leaf = dispatch_leaf(equal_table, 2);
    assert_eq!(inline_switch_sites_for_test(), before + 2);
    let run = |ctx: &mut Context, leaf: &CompiledLeaf, arg: Value, swp: bool| {
        ctx.symbols_with_pos_enabled = swp;
        let got = leaf.call(ctx as *mut Context as *mut u8, &[arg]);
        ctx.symbols_with_pos_enabled = false;
        got
    };
    let fixnum = |n: i64| NativeRun::Ok(Value::fixnum(n).bits());
    assert_eq!(run(&mut ctx, &eq_leaf, pa, true), fixnum(0));
    assert_eq!(run(&mut ctx, &eq_leaf, pb, true), fixnum(1));
    assert_eq!(run(&mut ctx, &eq_leaf, pa, false), fixnum(-1));
    assert_eq!(run(&mut ctx, &equal_leaf, list(&[pa, pb]), true), fixnum(0));
    assert_eq!(run(&mut ctx, &equal_leaf, list(&[a, pb]), true), fixnum(0));
    assert_eq!(run(&mut ctx, &equal_leaf, list(&[pa]), true), fixnum(1));
    assert_eq!(
        run(&mut ctx, &equal_leaf, list(&[pa, pb]), false),
        fixnum(-1)
    );
    assert_eq!(run(&mut ctx, &equal_leaf, list(&[a, b]), true), fixnum(0));
}

/// `NEOVM_JIT_INLINE_SWITCH=off` (here the per-thread override) compiles
/// every switch to the shim alone, answering the same.
#[test]
fn the_knob_off_keeps_the_shim_alone() {
    let mut ctx = Context::new_minimal_vm_harness();
    let (a, b) = (sym("a"), sym("b"));
    let table = jump_table(HashTableTest::Equal, &[(list(&[a, b]), 0), (a, 1)]);
    force_inline_switch_for_test(Some(false));
    let before = inline_switch_sites_for_test();
    let leaf = dispatch_leaf(table, 2);
    force_inline_switch_for_test(None);
    assert_eq!(inline_switch_sites_for_test(), before, "no inline site");
    let probes = probes(&mut ctx, &[list(&[a, b]), a]);
    check_like_lookup(&mut ctx, &leaf, table, &probes, "knob off");
}

/// A weak jump table (never planned: the GC's sweep mutates it without the
/// epoch) keeps the shim.
#[test]
fn a_weak_table_keeps_the_shim() {
    use crate::emacs_core::value::HashTableWeakness;
    let a = sym("a");
    let table = jump_table(HashTableTest::Eq, &[(a, 0)]);
    let _ = table.with_hash_table_mut(|ht| ht.weakness = Some(HashTableWeakness::Key));
    let before = inline_switch_sites_for_test();
    let leaf = dispatch_leaf(table, 1);
    assert_eq!(inline_switch_sites_for_test(), before);
    let mut ctx = Context::new_minimal_vm_harness();
    assert_eq!(
        leaf.call(&mut ctx as *mut Context as *mut u8, &[a]),
        NativeRun::Ok(Value::fixnum(0).bits())
    );
}
