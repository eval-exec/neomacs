//! The production LAZY prewarm path, end to end (P4.2 A0): startup marks the
//! preload manifest's members (`mark_preload_members_prewarmed`), dispatch
//! then runs a marked function native from call 1, and the cache-miss AOT
//! consult (`try_load_leaf` with the stashed manifest hash) must build its
//! leaf from the preload unit. The eager `prepopulate_aot_from_preload`
//! tests never exercise that consult.
use super::*;
use crate::emacs_core::bytecode::ByteCodeFunction;
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::value::LambdaParams;

/// A required-only, AOT-runnable `(lambda (a) ...)` body.
struct Member {
    name: &'static str,
    ops: Vec<Op>,
    constants: Vec<Value>,
}

impl Member {
    fn function(&self) -> ByteCodeFunction {
        let mut f = ByteCodeFunction::new(LambdaParams {
            required: vec![SymId(1)],
            optional: Vec::new(),
            rest: None,
        });
        f.lexical = true;
        f.ops = self.ops.clone();
        f.constants = self.constants.clone().into();
        f.max_stack = 16;
        f.seal_hand_assembled_ops();
        f
    }

    fn hash(&self) -> u128 {
        leaf_content_hash(&self.ops, &self.constants, 1).expect("hashable body")
    }
}

/// Bind every member in `ev`, build ONE preload `.so` from them (the
/// dump-time producer's object), and inject it as THE preload together with
/// the `m` pre-keys the producer's manifest would carry. Forces AOT on. The
/// returned directory keeps the `.so` on disk.
fn install_lazy_preload(ev: &mut Context, members: &[Member]) -> tempfile::TempDir {
    let mut prekeys = PreKeyMap::new();
    let mut leaves = Vec::new();
    for member in members {
        ev.obarray
            .set_symbol_function_id(intern(member.name), Value::make_bytecode(member.function()));
        prekeys.insert(
            member.name.into(),
            ManifestPreKey {
                member: true,
                ops_len: member.ops.len(),
                arity: 1,
                hash: member.hash(),
            },
        );
        leaves.push(LoadupLeaf {
            name: member.name.to_string(),
            ops: Box::leak(member.ops.clone().into_boxed_slice()),
            constants: Box::leak(member.constants.clone().into_boxed_slice()),
            arity: 1,
        });
    }
    let (obj, _) = build_preload_object(&leaves, None).expect("build preload");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    test_support::set_forced_enabled(true);
    test_support::inject_preload(std::sync::Arc::new(super::super::compile::LoadedUnit::new(
        lib,
    )));
    test_support::inject_prekeys(prekeys);
    // Prime the cache's heap-identity guard before anything is cached, as
    // the first collection does in production.
    let mut prime: Vec<Value> = Vec::new();
    super::super::cache::collect_jit_reloc_gc_roots(&mut prime);
    dir
}

fn function_of(ev: &Context, name: &str) -> Value {
    ev.obarray
        .symbol_function_id(intern(name))
        .expect("bound member")
}

#[cfg(target_os = "linux")]
#[test]
fn lazy_prewarm_serves_a_marked_member_from_the_preload() {
    let mut ev = Context::new_minimal_vm_harness();
    let add5 = Member {
        name: "lazy-prewarm-add5",
        ops: vec![Op::Constant(0), Op::Add, Op::Return],
        constants: vec![Value::make_int(5)],
    };
    let _dir = install_lazy_preload(&mut ev, std::slice::from_ref(&add5));

    assert_eq!(mark_preload_members_prewarmed(&ev), (1, 1));
    let f = function_of(&ev, add5.name);
    let id = f
        .get_bytecode_data()
        .and_then(|bc| bc.jit_runtime().compiled_id())
        .expect("marking assigns the compiled id");
    assert_eq!(prewarm_hash_for(id), Some(add5.hash()));

    super::super::stats::reset_compile_stats();
    assert_eq!(
        ev.apply1(f, Value::make_int(37)).unwrap(),
        Value::make_int(42)
    );
    let stats = super::super::stats::compile_stats_snapshot();
    // EXPECTED FAILURE (P4.2 A0, fixed by A1): the first-call consult looks
    // only in the per-hash NEOVM_AOT_DIR index, never at the preload unit,
    // so the marked member is JIT-compiled from call 1 instead.
    assert_eq!(stats.aot_loads, 0, "{stats:?}");
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(false),
        "the marked member was compiled by the JIT, not served from the preload"
    );

    super::super::cache::clear();
    test_support::reset();
}
