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
                class: PreloadClass::Prewarm,
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
    let (obj, built) = build_preload_object(&leaves, None).expect("build preload");
    assert_eq!(
        built.prepared,
        members.len(),
        "every battery member must be in the AOT subset: {built:?}"
    );
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
    for (arg, want) in [(37, 42), (-5, 0), (1 << 40, (1 << 40) + 5)] {
        assert_eq!(
            ev.apply1(f, Value::make_int(arg)).unwrap(),
            Value::make_int(want)
        );
    }
    let stats = super::super::stats::compile_stats_snapshot();
    // P4.2 A1: the first call's consult builds the leaf from the preload
    // unit (one AOT load, no JIT compile), and later calls reuse it.
    assert_eq!(stats.aot_loads, 1, "{stats:?}");
    assert_eq!(stats.total_compiles, 0, "{stats:?}");
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true),
        "the marked member must be served from the preload"
    );

    super::super::cache::clear();
    test_support::reset();
}

/// The lazy-prewarm battery (P4.2 A7): a corpus of loadup-shaped members
/// (arithmetic, comparison, branches, and symbol, string and fixnum
/// constants), each marked from the manifest and served from the preload on
/// its first call, must answer exactly as a copy pinned to the Tier-0
/// interpreter, over a spread of arguments. The battery is only meaningful
/// if AOT really served: it requires one AOT load per member and no JIT
/// compile (the pre-A1 path passed every answer check while serving none).
#[cfg(target_os = "linux")]
#[test]
fn lazy_prewarm_battery_matches_the_interpreter_and_serves_every_member() {
    let mut ev = Context::new_minimal_vm_harness();
    let members = [
        Member {
            name: "lazy-battery-add5",
            ops: vec![Op::Constant(0), Op::Add, Op::Return],
            constants: vec![Value::make_int(5)],
        },
        Member {
            name: "lazy-battery-sub1",
            ops: vec![Op::Constant(0), Op::Sub, Op::Return],
            constants: vec![Value::make_int(1)],
        },
        Member {
            name: "lazy-battery-square",
            ops: vec![Op::StackRef(0), Op::Mul, Op::Return],
            constants: vec![],
        },
        Member {
            name: "lazy-battery-inc",
            ops: vec![Op::Add1, Op::Return],
            constants: vec![],
        },
        Member {
            name: "lazy-battery-negp",
            ops: vec![Op::Constant(0), Op::Lss, Op::Return],
            constants: vec![Value::make_int(0)],
        },
        Member {
            name: "lazy-battery-sign",
            ops: vec![
                Op::Dup,
                Op::Constant(0),
                Op::Lss,
                Op::GotoIfNil(6),
                Op::Constant(1),
                Op::Return,
                Op::Constant(2),
                Op::Return,
            ],
            constants: vec![
                Value::make_int(0),
                Value::symbol("lazy-battery-negative"),
                Value::symbol("lazy-battery-non-negative"),
            ],
        },
        Member {
            name: "lazy-battery-label",
            ops: vec![Op::Constant(0), Op::Return],
            constants: vec![Value::string("lazy-battery label")],
        },
    ];
    let _dir = install_lazy_preload(&mut ev, &members);
    assert_eq!(
        mark_preload_members_prewarmed(&ev),
        (members.len(), members.len())
    );

    super::super::stats::reset_compile_stats();
    for member in &members {
        let served = function_of(&ev, member.name);
        let reference = Value::make_bytecode(member.function());
        reference
            .get_bytecode_data()
            .unwrap()
            .jit_runtime()
            .set_cold_for_test();
        crate::emacs_core::eval::push_scratch_gc_root(reference);
        for arg in [0i64, 1, 2, 7, -3, 42, 1000, -1000, (1 << 40) - 1] {
            let arg = Value::make_int(arg);
            let got = ev.apply1(served, arg).expect("served call");
            let want = ev.apply1(reference, arg).expect("interpreted call");
            assert_eq!(
                crate::emacs_core::print::print_value(&got),
                crate::emacs_core::print::print_value(&want),
                "{} {arg:?}",
                member.name
            );
        }
        let id = served
            .get_bytecode_data()
            .and_then(|bc| bc.jit_runtime().compiled_id())
            .expect("marked");
        assert_eq!(
            super::super::cache::cached_leaf_is_aot_for_test(id),
            Some(true),
            "{} must be served from the preload",
            member.name
        );
    }
    let stats = super::super::stats::compile_stats_snapshot();
    assert_eq!(stats.aot_loads, members.len() as u64, "{stats:?}");
    assert_eq!(stats.total_compiles, 0, "{stats:?}");

    super::super::cache::clear();
    test_support::reset();
}

/// P4.2 A2: marking a pdump image's members must not materialize their lazy
/// stubs (it cost 33M instructions of startup for 1,696 members). The mark
/// lands when a stub first materializes, and the member is still served from
/// the preload on that first call.
#[cfg(target_os = "linux")]
#[test]
fn lazy_prewarm_marks_pdump_stubs_without_materializing_them() {
    crate::test_utils::init_test_tracing();
    let scratch = crate::emacs_core::eval::save_scratch_gc_roots();
    let mut ctx = Context::new();
    // GNU bytecode (the reader literal): dumped as a lazy stub.
    ctx.eval_str("(defalias 'lazy-stub-add5 #[257 \"\\211\\300\\\\\\207\" [5] 3])")
        .expect("defalias");
    // The dump-time producer's view: the member's body and pre-key.
    let leaves: Vec<_> = enumerate_loadup_leaves(&ctx, true)
        .into_iter()
        .filter(|leaf| leaf.name == "lazy-stub-add5")
        .collect();
    assert_eq!(leaves.len(), 1, "the member must be an AOT candidate");
    let leaf = &leaves[0];
    let mut prekeys = PreKeyMap::new();
    prekeys.insert(
        leaf.name.as_str().into(),
        ManifestPreKey {
            class: PreloadClass::Prewarm,
            ops_len: leaf.ops.len(),
            arity: leaf.arity,
            hash: leaf_content_hash(leaf.ops, leaf.constants, leaf.arity).expect("hashable"),
        },
    );
    let (obj, built) = build_preload_object(&leaves, None).expect("build preload");
    assert_eq!(built.unique_emitted, 1, "{built:?}");
    let dir = tempfile::tempdir().expect("tempdir");
    let so_path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &so_path).expect("link");
    let dump_path = dir.path().join("lazy-stubs.pdump");
    crate::emacs_core::pdump::dump_to_file(&ctx, &dump_path).expect("dump");
    crate::emacs_core::eval::restore_scratch_gc_roots(scratch);
    let mut loaded = crate::emacs_core::pdump::load_from_dump(&dump_path).expect("load");

    let lib = unsafe { libloading::Library::new(&so_path) }.expect("dlopen");
    test_support::set_forced_enabled(true);
    test_support::inject_preload(std::sync::Arc::new(super::super::compile::LoadedUnit::new(
        lib,
    )));
    test_support::inject_prekeys(prekeys);
    let mut prime: Vec<Value> = Vec::new();
    super::super::cache::collect_jit_reloc_gc_roots(&mut prime);

    let f = function_of(&loaded, "lazy-stub-add5");
    assert!(
        f.bytecode_data_if_materialized().is_none(),
        "the dumped member loads as a lazy stub"
    );
    assert_eq!(mark_preload_members_prewarmed(&loaded), (1, 1));
    assert!(
        f.bytecode_data_if_materialized().is_none(),
        "marking must not materialize the stub"
    );

    super::super::stats::reset_compile_stats();
    assert_eq!(
        loaded.apply1(f, Value::make_int(37)).unwrap(),
        Value::make_int(42)
    );
    let bc = f
        .bytecode_data_if_materialized()
        .expect("the call materialized it");
    let id = bc
        .jit_runtime()
        .compiled_id()
        .expect("marked on materialization");
    assert_eq!(
        prewarm_hash_for(id),
        Some(leaves_hash(&loaded, "lazy-stub-add5"))
    );
    let stats = super::super::stats::compile_stats_snapshot();
    assert_eq!(stats.aot_loads, 1, "{stats:?}");
    assert_eq!(stats.total_compiles, 0, "{stats:?}");
    assert_eq!(
        super::super::cache::cached_leaf_is_aot_for_test(id),
        Some(true)
    );

    super::super::cache::clear();
    test_support::reset();
}

/// The content hash of the live (materialized) function bound to `name`.
fn leaves_hash(ctx: &Context, name: &str) -> u128 {
    let bc = function_of(ctx, name)
        .get_bytecode_data()
        .expect("byte code");
    leaf_content_hash(bc.executable_ops(), &bc.constants, bc.params.required.len())
        .expect("hashable")
}

/// Startup marking streams the manifest's member lines (P4.2 A2, A4): `m`
/// and `c` members only, escaped names decoded, the interlock header
/// enforced, malformed lines skipped.
#[test]
fn manifest_member_stream_yields_members_behind_the_interlock() {
    let key = |class, ops_len, arity, hash| ManifestPreKey {
        class,
        ops_len,
        arity,
        hash,
    };
    let header = |fingerprint: &str| {
        format!(
            "version {PRELOAD_MANIFEST_VERSION}\nabi_tag {ABI_TAG:08x}\nfingerprint {fingerprint}\nleaves 6\n"
        )
    };
    use PreloadClass::{AtTierUp, NonMember, Prewarm};
    let mut body = String::new();
    body.push_str(&manifest_leaf_line(Prewarm, 3, 1, 0xabc, "plain-member"));
    body.push_str(&manifest_leaf_line(NonMember, 4, 1, 0xdef, "non-member"));
    body.push_str(&manifest_leaf_line(Prewarm, 5, 2, 7, "with space"));
    body.push_str("leaf m 1 1 zz broken-hash\n");
    body.push_str(&manifest_leaf_line(AtTierUp, 9, 1, 0x99, "call-glue"));
    body.push_str(&manifest_leaf_line(Prewarm, 6, 0, u128::MAX, "%leading"));

    let running = crate::emacs_core::pdump::fingerprint_hex();
    let text = header(running) + &body;
    let mut seen = Vec::new();
    assert!(for_each_manifest_member(&text, |name, key| {
        seen.push((name.to_string(), key))
    }));
    assert_eq!(
        seen,
        vec![
            ("plain-member".to_string(), key(Prewarm, 3, 1, 0xabc)),
            ("with space".to_string(), key(Prewarm, 5, 2, 7)),
            ("call-glue".to_string(), key(AtTierUp, 9, 1, 0x99)),
            ("%leading".to_string(), key(Prewarm, 6, 0, u128::MAX)),
        ]
    );

    // A manifest for another image streams nothing.
    let mut stale = Vec::new();
    assert!(!for_each_manifest_member(
        &(header("not-this-image") + &body),
        |name, _| stale.push(name.to_string())
    ));
    assert!(stale.is_empty());
}

/// P4.2 A4: a `c` member (call glue) is not prewarmed. It runs in the
/// interpreter like any cold function, and the consult serves it from the
/// preload exactly when the JIT would compile it: the AOT leaf replaces that
/// compile instead of moving it to call 1. `NEOVM_AOT_PREWARM=all` restores
/// the old from-call-1 behavior.
#[cfg(target_os = "linux")]
#[test]
fn call_glue_members_are_served_at_tier_up_not_from_call_1() {
    for policy in [PrewarmPolicy::Profitable, PrewarmPolicy::All] {
        let mut ev = Context::new_minimal_vm_harness();
        let glue = Member {
            name: "lazy-glue-add5",
            ops: vec![Op::Constant(0), Op::Add, Op::Return],
            constants: vec![Value::make_int(5)],
        };
        let _dir = install_lazy_preload(&mut ev, std::slice::from_ref(&glue));
        // The same body classed `c` by the producer.
        let mut prekeys = PreKeyMap::new();
        prekeys.insert(
            glue.name.into(),
            ManifestPreKey {
                class: PreloadClass::AtTierUp,
                ops_len: glue.ops.len(),
                arity: 1,
                hash: glue.hash(),
            },
        );
        test_support::inject_prekeys(prekeys);
        test_support::set_forced_prewarm_policy(policy);

        let prewarmed = policy == PrewarmPolicy::All;
        assert_eq!(
            mark_preload_members_prewarmed(&ev),
            (1, usize::from(prewarmed))
        );
        let f = function_of(&ev, glue.name);
        let bc = f.get_bytecode_data().unwrap();
        let rt = bc.jit_runtime();
        let id = rt.compiled_id().expect("stashed");
        assert_eq!(prewarm_hash_for(id), Some(glue.hash()), "{policy:?}");
        assert_eq!(rt.is_aot_prewarmed(), prewarmed, "{policy:?}");

        super::super::stats::reset_compile_stats();
        assert_eq!(
            ev.apply1(f, Value::make_int(1)).unwrap(),
            Value::make_int(6)
        );
        let loads_after_first_call = super::super::stats::compile_stats_snapshot().aot_loads;
        assert_eq!(
            loads_after_first_call,
            u64::from(prewarmed),
            "{policy:?}: call glue runs interpreted until its tier-up"
        );
        if !prewarmed {
            // Reach the tier-up heat: the next call is the one the JIT
            // would compile, and the preload serves it instead.
            rt.set_heat_for_test(crate::emacs_core::jit::Runtime::HOT_THRESHOLD);
            assert_eq!(
                ev.apply1(f, Value::make_int(2)).unwrap(),
                Value::make_int(7)
            );
        }
        let stats = super::super::stats::compile_stats_snapshot();
        assert_eq!(stats.aot_loads, 1, "{policy:?} {stats:?}");
        assert_eq!(stats.total_compiles, 0, "{policy:?} {stats:?}");
        assert_eq!(
            super::super::cache::cached_leaf_is_aot_for_test(id),
            Some(true)
        );

        super::super::cache::clear();
        test_support::reset();
    }
}

/// The consult's schedule for call glue is the JIT's profit gate: a
/// call-heavy body is served only on the deferred re-attempt that bypasses
/// the gate; a body the gate passes is served on its first tier-up.
#[test]
fn call_glue_is_servable_when_the_jit_would_compile_it() {
    let glue_ops = [Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return];
    let glue_consts = [Value::symbol("lazy-glue-callee")];
    assert!(!jit_would_compile_now(&glue_ops, &glue_consts, false));
    assert!(jit_would_compile_now(&glue_ops, &glue_consts, true));
    let arith_ops = [Op::Constant(0), Op::Add, Op::Return];
    let arith_consts = [Value::make_int(5)];
    assert!(jit_would_compile_now(&arith_ops, &arith_consts, false));
}
