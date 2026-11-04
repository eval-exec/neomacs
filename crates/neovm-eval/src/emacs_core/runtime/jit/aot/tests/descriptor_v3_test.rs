//! P4.2 A3: descriptor v3 (each reloc slot names its constant-pool index)
//! and the preload's exported leaf index, which together make a leaf load a
//! binary search plus an index per reloc instead of two dlsyms by formatted
//! name, a recipe rebuild and a deep-equal search of the pool per reloc.
use super::*;

#[test]
fn descriptor_v3_round_trips_the_pool_indices() {
    let meta = super::super::compile::AotLeafMeta {
        arity: 1,
        required: 1,
        has_rest: false,
        has_binds: false,
        has_handlers: false,
        has_side_effects: false,
        max_depth: 0,
        has_precise_deopt: false,
    };
    let mut recipe = Vec::new();
    write_value_recipe(&mut recipe, Value::make_int(7)).unwrap();
    write_value_recipe(&mut recipe, Value::T).unwrap();
    let bytes = encode_descriptor(&meta, &recipe, 2, &[], &[3, RELOC_NOT_IN_POOL]);
    let desc = decode_descriptor(&bytes).expect("decodes");
    assert_eq!(desc.pool_indices, vec![3, RELOC_NOT_IN_POOL]);
    assert_eq!(desc.reloc_count, 2);
    // A truncated pool-index section fails closed.
    assert!(decode_descriptor(&bytes[..bytes.len() - 1]).is_none());
    // The recipe walker agrees with the rebuild on every kind.
    let mut all = Vec::new();
    let values = [
        Value::NIL,
        Value::T,
        Value::make_int(-5),
        Value::symbol("descriptor-v3-sym"),
    ];
    for v in values {
        let at = all.len();
        write_value_recipe(&mut all, v).unwrap();
        let (_, n) = rebuild_value(&all[at..], 0).unwrap();
        assert_eq!(recipe_len(&all[at..], 0), Some(n));
        assert!(recipe_tag_matches(&all[at..], v));
    }
    assert!(!recipe_tag_matches(&all[..1], Value::T), "nil recipe vs t");
}

/// A pooled reloc resolves to the pool's element BY INDEX. Two `equal`
/// strings in one pool are distinct objects in GNU and in the interpreter;
/// the v2 loader's deep-equal search handed the leaf the first of them for
/// both slots.
#[cfg(target_os = "linux")]
#[test]
fn a_pooled_reloc_is_the_pool_element_at_its_index() {
    let mut eval = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let first = Value::string("twin");
    let second = Value::string("twin");
    assert_ne!(first.bits(), second.bits());
    crate::emacs_core::eval::push_scratch_gc_root(first);
    crate::emacs_core::eval::push_scratch_gc_root(second);
    // (lambda (x) SECOND): the leaf returns pool slot 1.
    let ops = [Op::Constant(1), Op::Return];
    let constants = [first, second];
    let (obj, hash) = compile_leaf_to_object(&ops, &constants, 1, None)
        .expect("compile ok")
        .expect("AOT subset");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join("twin.so");
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    let leaf = load_leaf_from_unit(&unit, hash, 1, &constants, None).expect("load");
    let ctx = &mut eval as *mut crate::emacs_core::eval::Context as *mut u8;
    let bits = match leaf.call(ctx, &[Value::make_int(0)]) {
        crate::emacs_core::jit::compile::NativeRun::Ok(bits) => bits,
        other => panic!("AOT leaf: {other:?}"),
    };
    assert_eq!(bits, second.bits(), "the leaf returns pool slot 1 itself");
}

/// The preload exports one leaf index: every emitted leaf is found by hash,
/// at the same entry and descriptor its per-leaf symbols name, and nothing
/// else is.
#[cfg(target_os = "linux")]
#[test]
fn the_preload_index_finds_every_emitted_leaf() {
    let bodies: [(Vec<Op>, Vec<Value>); 3] = [
        (
            vec![Op::Constant(0), Op::Add, Op::Return],
            vec![Value::make_int(5)],
        ),
        (
            vec![Op::Constant(0), Op::Sub, Op::Return],
            vec![Value::make_int(1)],
        ),
        (vec![Op::Add1, Op::Return], vec![]),
    ];
    let leaves: Vec<LoadupLeaf> = bodies
        .iter()
        .enumerate()
        .map(|(i, (ops, constants))| LoadupLeaf {
            name: format!("index-leaf-{i}"),
            ops: Box::leak(ops.clone().into_boxed_slice()),
            constants: Box::leak(constants.clone().into_boxed_slice()),
            arity: 1,
        })
        .collect();
    let (obj, built) = build_preload_object(&leaves, None).expect("build");
    assert_eq!(built.unique_emitted, bodies.len());
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    let unit = super::super::compile::LoadedUnit::new(lib);
    let index = unit_leaf_index(&unit).expect("the preload exports an index");
    assert_eq!(index.len, bodies.len());
    for (ops, constants) in &bodies {
        let hash = leaf_content_hash(ops, constants, 1).unwrap();
        let (entry, desc) = index.lookup(hash).expect("indexed");
        unsafe {
            let lib = unit.library();
            let by_name: libloading::Symbol<*const u8> =
                lib.get(aot_entry_symbol(hash).as_bytes()).unwrap();
            assert_eq!(entry, *by_name);
            let by_name: libloading::Symbol<*const u8> =
                lib.get(aot_descriptor_symbol(hash).as_bytes()).unwrap();
            assert_eq!(desc, *by_name);
        }
        assert!(unit_has_entry(&unit, hash));
    }
    assert!(index.lookup(0x1234).is_none());
    assert!(!unit_has_entry(&unit, 0x1234));
}
