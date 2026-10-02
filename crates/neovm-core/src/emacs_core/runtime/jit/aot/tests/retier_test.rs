//! AOT entry credit, cached-call reach and replacement publication (P4.2 A5).
#![cfg(target_os = "linux")]

use std::sync::atomic::Ordering;

use super::*;
use crate::emacs_core::bytecode::{ByteCodeFunction, Op};
use crate::emacs_core::eval::Context;
use crate::emacs_core::intern::{SymId, intern};
use crate::emacs_core::jit::compile::lowering::{RegallocChoice, forced_regalloc};
use crate::emacs_core::jit::compile::{CompiledLeaf, Tier2Knob, Tier2PolicyKnob};
use crate::emacs_core::jit::tier2::{T2Origin, T2State, T2Upgrade};
use crate::emacs_core::jit::{Plan, aot, bg, cache, compile, stats};
use crate::emacs_core::value::{LambdaParams, Value};

const MEMBER: &str = "aot-retier-test-member";

/// Integration hosts export shims dynamically; the lib-test host does not.
/// Bind this fixture's call-bearing imports to the same immutable code
/// addresses the JIT uses. These absolute bindings never leave this process.
fn link_call_glue_in_unit_host(object: &[u8], path: &std::path::Path) {
    use object::{Object, ObjectSymbol};

    let file = object::File::parse(object).expect("fixture object");
    let object_path = path.with_extension("o");
    std::fs::write(&object_path, object).expect("write fixture object");
    let mut cc = std::process::Command::new("cc");
    cc.arg("-shared").arg("-o").arg(path).arg(&object_path);
    for symbol in file.symbols().filter(ObjectSymbol::is_undefined) {
        let name = symbol.name().expect("fixture import name");
        if !name.starts_with("neovm_jit_") {
            continue;
        }
        let (_, address) = compile::JIT_SHIM_TABLE
            .iter()
            .find(|(candidate, _)| *candidate == name)
            .expect("AOT import belongs to the production shim registry");
        // SAFETY: ShimAddr is repr(transparent) over an immutable code
        // pointer. Copying that representation neither dereferences its
        // target nor accesses mutator state; the host owns the code forever.
        let pointer: *const () = unsafe { std::mem::transmute_copy(address) };
        cc.arg(format!("-Wl,--defsym,{name}={:#x}", pointer as usize));
    }
    let output = cc.output().expect("link call-glue fixture");
    assert!(output.status.success(), "fixture link failed: {output:?}");
    std::fs::remove_file(object_path).expect("remove fixture object");
}

fn unary(ops: Vec<Op>, constants: Vec<Value>) -> ByteCodeFunction {
    let mut f = ByteCodeFunction::new(LambdaParams {
        required: vec![SymId(1)],
        optional: Vec::new(),
        rest: None,
    });
    f.lexical = true;
    f.ops = ops;
    f.constants = constants.into();
    f.max_stack = 8;
    f.seal_hand_assembled_ops();
    f
}

/// This fixture owns one mutator's Context and its loaded unit; no pointer or
/// Lisp state is shared with another test's mutator or a backend worker.
struct Fixture {
    ctx: Context,
    function: Value,
    answer: Value,
    _unit: tempfile::TempDir,
}

impl Fixture {
    fn add5(enabled: bool, t2: bool, at: u32) -> Self {
        Self::new(enabled, t2, at, |_| {
            (
                unary(
                    vec![Op::Constant(0), Op::Add, Op::Return],
                    vec![Value::make_int(5)],
                ),
                Value::make_int(42),
            )
        })
    }

    fn new(
        enabled: bool,
        t2: bool,
        at: u32,
        build: impl FnOnce(&mut Context) -> (ByteCodeFunction, Value),
    ) -> Self {
        Self::new_with_class(enabled, t2, at, aot::PreloadClass::Prewarm, build)
    }

    fn new_with_class(
        enabled: bool,
        t2: bool,
        at: u32,
        class: aot::PreloadClass,
        build: impl FnOnce(&mut Context) -> (ByteCodeFunction, Value),
    ) -> Self {
        cache::clear();
        compile::force_aot_retier_for_test(Some(enabled));
        force_heat_for_test(Some(at));
        compile::force_tier2_for_test(Some(Tier2Knob {
            on: t2,
            window: 1_000,
            loop_credit: 64,
        }));
        compile::force_tier2_policy_for_test(Some(Tier2PolicyKnob {
            stable: 1,
            attempts: 4,
            budget_pct: 0,
            floor_ms: 5,
            max_reopt: 3,
        }));
        compile::force_profit_gate_for_test(false);
        compile::force_deopt_for_test(false);
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        bg::force_mode_for_test(Some(bg::BgMode::Sync));
        bg::force_deferred_install_for_test(false);
        bg::force_queue_cap_for_test(None);
        bg::hold_publish_for_test(false);
        let _ = bg::publish_held_for_test();

        let mut ctx = Context::new_minimal_vm_harness();
        let (body, answer) = build(&mut ctx);
        let hash = aot::leaf_content_hash(body.executable_ops(), &body.constants, 1)
            .expect("hashable AOT member");
        let ops = Box::leak(body.executable_ops().to_vec().into_boxed_slice());
        let constants = Box::leak(body.constants.to_vec().into_boxed_slice());
        let (object, built) = aot::build_preload_object(
            &[aot::LoadupLeaf {
                name: MEMBER.into(),
                ops,
                constants,
                arity: 1,
            }],
            None,
        )
        .expect("emit real pure preload member");
        assert_eq!(built.prepared, 1, "{built:?}");
        let scratch = crate::test_utils::workspace_root().join("tmp");
        std::fs::create_dir_all(&scratch).expect("workspace tmp directory");
        let dir = tempfile::Builder::new()
            .prefix("aot-retier-")
            .tempdir_in(scratch)
            .expect("workspace-local fixture directory");
        let path = dir.path().join(aot::PRELOAD_SO_NAME);
        if class == aot::PreloadClass::AtTierUp {
            link_call_glue_in_unit_host(&object, &path);
        } else {
            aot::link_object_to_so(&object, &path).expect("link preload");
        }
        let library = unsafe { libloading::Library::new(&path) }.expect("dlopen preload");
        aot::test_support::set_forced_enabled(true);
        aot::test_support::inject_preload(std::sync::Arc::new(compile::LoadedUnit::new(library)));
        let mut prekeys = aot::PreKeyMap::new();
        prekeys.insert(
            MEMBER.into(),
            aot::ManifestPreKey {
                class,
                ops_len: ops.len(),
                arity: 1,
                hash,
            },
        );
        aot::test_support::inject_prekeys(prekeys);
        if class == aot::PreloadClass::AtTierUp {
            aot::test_support::set_forced_prewarm_policy(aot::PrewarmPolicy::All);
        }
        let function = Value::make_bytecode(body);
        ctx.obarray.set_symbol_function_id(intern(MEMBER), function);
        let mut prime = Vec::new();
        cache::collect_jit_reloc_gc_roots(&mut prime);
        assert_eq!(aot::mark_preload_members_prewarmed(&ctx), (1, 1));
        stats::reset_compile_stats();
        Self {
            ctx,
            function,
            answer,
            _unit: dir,
        }
    }

    fn run(&mut self) {
        assert_eq!(
            self.ctx.apply1(self.function, Value::make_int(37)).unwrap(),
            self.answer,
        );
    }

    fn data(&self) -> &'static ByteCodeFunction {
        self.function
            .get_bytecode_data()
            .expect("materialized member")
    }

    fn id(&self) -> u64 {
        self.data().jit_runtime().compiled_id().expect("member id")
    }

    fn current(&self) -> &'static CompiledLeaf {
        let ptr = cache::compiled_leaf_ptr_for_test(self.id()).expect("native member");
        // SAFETY: this mutator's cache retains current and retired leaves until
        // clear. Tests stop using these references before Fixture::drop clears.
        unsafe { &*ptr }
    }

    fn expect_aot(&self) {
        assert!(
            self.current().is_aot_backed(),
            "member must really serve AOT"
        );
        assert_eq!(stats::compile_stats_snapshot().aot_loads, 1);
    }

    fn expect_jit(&self, old: &CompiledLeaf) {
        let current = self.current();
        assert!(!current.is_aot_backed(), "replacement must be JIT code");
        assert!(!std::ptr::eq(old, current));
        assert_eq!(current.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Retier));
        assert!(old.retired.get());
        assert_eq!(old.obs.t2.state.get(), T2State::Upgraded(T2Upgrade::Retier));
        assert_eq!(stats::compile_stats_snapshot().aot_loads, 1);
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        bg::hold_publish_for_test(false);
        let _ = bg::publish_held_for_test();
        cache::clear();
        aot::test_support::reset();
        compile::force_aot_retier_for_test(None);
        force_heat_for_test(None);
        compile::force_tier2_for_test(None);
        compile::force_tier2_policy_for_test(None);
        compile::force_profit_gate_for_test(true);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        bg::force_deferred_install_for_test(false);
        bg::force_queue_cap_for_test(None);
        bg::force_mode_for_test(None);
    }
}

#[test]
fn aot_retier_disabled_keeps_serving_preloaded_code() {
    let mut f = Fixture::add5(false, true, 4);
    for _ in 0..12 {
        f.run();
        f.expect_aot();
    }
    assert_eq!(f.current().obs.t2.budget.get(), DISARMED);
    assert_eq!(f.current().obs.t2.state.get(), T2State::Idle);
    assert!(f.current().obs.t2.source.borrow().is_none());
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 0);
}

fn exact_entry_threshold(t2: bool) {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, t2, 4);
    f.run();
    f.expect_aot();
    let old = f.current();
    assert_eq!(
        old.obs.t2.budget.get(),
        3,
        "initial dispatch is counted once"
    );
    for remaining in [2, 1] {
        f.run();
        assert!(std::ptr::eq(f.current(), old));
        assert_eq!(old.obs.t2.budget.get(), remaining);
        assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    }
    assert_eq!(
        f.data().jit_runtime().heat(),
        3,
        "no second native-entry heat credit"
    );
    f.run();
    assert!(
        std::ptr::eq(f.current(), old),
        "request leaves this activation native"
    );
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
    assert_eq!(old.obs.t2.budget.get(), DISARMED);
    assert!(!may_load(f.data().jit_runtime()));
    f.run();
    f.expect_jit(old);
    assert_eq!(f.current().regalloc, RegallocChoice::Full);
    for _ in 0..12 {
        f.run();
    }
    assert_eq!(
        stats::compile_stats_snapshot().total_compiles,
        1,
        "one replacement"
    );
}

#[test]
fn aot_retier_tier2_requests_at_exact_entry_heat() {
    exact_entry_threshold(true);
}

#[test]
fn aot_retier_legacy_entries_request_at_exact_heat() {
    exact_entry_threshold(false);
}

#[test]
fn aot_retier_legacy_cache_heat_replaces_without_reloading_aot() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, false, 4);
    f.run();
    let old = f.current();
    f.data().jit_runtime().set_heat_for_test(4);
    let function = f.function;
    let data = f.data();
    assert_eq!(
        cache::try_run_compiled(&mut f.ctx, data, function, &[Value::make_int(37)]).unwrap(),
        Some(f.answer.bits()),
    );
    f.expect_jit(old);
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 1);
}

fn cached_symbol_calls_reach_request(t2: bool) {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, t2, 6);
    f.run();
    let old = f.current();
    let member_heat = f.data().jit_runtime().heat();
    let mut caller = unary(
        vec![Op::Constant(0), Op::StackRef(2), Op::Call(1), Op::Return],
        vec![Value::from_sym_id(intern(MEMBER))],
    );
    // The optional slot keeps the caller on baseline lowering: the legacy
    // MIR pure inliner otherwise removes this tiny symbol call even with the
    // bytecode fuser disabled. StackRef skips the missing optional's nil slot.
    caller.params.optional.push(SymId(2));
    let caller = Value::make_bytecode(caller);
    f.ctx
        .obarray
        .set_symbol_function_id(intern("aot-retier-test-caller"), caller);
    let data = caller.get_bytecode_data().unwrap();
    let run = |ctx: &mut Context| {
        assert_eq!(
            cache::try_run_compiled(ctx, data, caller, &[Value::make_int(37)]).unwrap(),
            Some(Value::make_int(42).bits()),
        );
    };
    run(&mut f.ctx);
    let caller_ptr = cache::compiled_leaf_ptr_for_test(data.jit_runtime().compiled_id().unwrap())
        .expect("compiled caller");
    // SAFETY: Fixture keeps this mutator's current/retired leaves alive.
    let caller_leaf = unsafe { &*caller_ptr };
    assert_eq!(caller_leaf.spec_slots.len(), 1, "a real symbol spec site");
    assert_eq!(
        caller_leaf.spec_slots[0].leaf_ptr(),
        std::ptr::from_ref(old)
    );
    let fast_calls = compile::SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed);
    for _ in 0..3 {
        run(&mut f.ctx);
        assert!(std::ptr::eq(f.current(), old));
        assert_eq!(f.data().jit_runtime().heat(), member_heat);
        assert_eq!(old.obs.t2.state.get(), T2State::Idle);
    }
    assert!(
        compile::SPEC_SHIM_FAST_COUNT.load(Ordering::Relaxed) > fast_calls,
        "cached native symbol calls really take the shim's fast path"
    );
    assert_eq!(old.obs.t2.budget.get(), 1);
    run(&mut f.ctx);
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
    assert_eq!(f.data().jit_runtime().heat(), member_heat);
    assert!(
        caller_leaf.spec_slots[0].leaf_ptr().is_null(),
        "request unlinks cached caller"
    );
    assert!(!may_load(f.data().jit_runtime()));
    run(&mut f.ctx);
    f.expect_jit(old);
    assert_eq!(
        caller_leaf.spec_slots[0].leaf_ptr(),
        std::ptr::from_ref(f.current())
    );
}

#[test]
fn aot_retier_exclusion_survives_cache_eviction() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, true, 2);
    f.run();
    f.run();
    f.run();
    assert!(!f.current().is_aot_backed());
    assert_eq!(stats::compile_stats_snapshot().aot_loads, 1);
    f.data().jit_runtime().set_heat_for_test(0);
    assert!(
        !may_load(f.data().jit_runtime()),
        "atomic source exclusion is independent of heat"
    );
    cache::clear();
    f.run();
    assert!(!f.current().is_aot_backed());
    assert_eq!(
        stats::compile_stats_snapshot().aot_loads,
        1,
        "eviction cannot reload AOT"
    );
}

#[test]
fn aot_retier_pending_upgrade_serves_and_roots_old_until_publication() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::new(true, true, 2, |_| {
        let answer = Value::string("AOT and pending JIT both retain this constant");
        (
            unary(vec![Op::Constant(0), Op::Return], vec![answer]),
            answer,
        )
    });
    let answer = f.answer;
    f.run();
    let old = f.current();
    f.run();
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
    bg::force_deferred_install_for_test(true);
    bg::hold_publish_for_test(true);
    let installs = bg::stats_snapshot().installed[bg::JobClass::Upgrade as usize];
    for _ in 0..12 {
        f.run();
        assert!(
            std::ptr::eq(f.current(), old),
            "old AOT still serves native"
        );
        assert!(!old.retired.get());
        assert!(matches!(f.data().jit_runtime().dispatch(), Plan::Compiled));
    }
    assert_eq!(cache::cache_entry_kind_for_test(f.id()), "upgrading");
    let data = f.data();
    assert!(cache::resolve_compiled_leaf_ptr(&mut f.ctx, data).is_none());
    let mut roots = Vec::new();
    cache::collect_jit_reloc_gc_roots(&mut roots);
    assert!(
        roots.iter().filter(|&&v| v == answer).count() >= 2,
        "both leaves are roots"
    );
    assert_eq!(bg::publish_held_for_test(), 1);
    f.run();
    f.expect_jit(old);
    assert_eq!(
        bg::stats_snapshot().installed[bg::JobClass::Upgrade as usize],
        installs + 1
    );
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
}

#[test]
fn aot_retier_call_glue_preserves_native_admission_with_the_profit_gate_on() {
    if forced_regalloc().is_some() {
        return;
    }
    for t2 in [false, true] {
        let mut f = Fixture::new_with_class(true, t2, 4, aot::PreloadClass::AtTierUp, |ctx| {
            let callee_name = intern("aot-retier-test-glue-callee");
            let mut callee = unary(
                vec![Op::StackRef(1), Op::Constant(0), Op::Add, Op::Return],
                vec![Value::make_int(5)],
            );
            // A non-inlinable callee leaves actual call glue at the member's
            // profit gate. The required argument is beneath optional nil.
            callee.params.optional.push(SymId(2));
            let callee = Value::make_bytecode(callee);
            ctx.obarray.set_symbol_function_id(callee_name, callee);
            (
                unary(
                    vec![Op::Constant(0), Op::StackRef(1), Op::Call(1), Op::Return],
                    vec![Value::from_sym_id(callee_name)],
                ),
                Value::make_int(42),
            )
        });
        assert!(compile::body_is_call_heavy(
            f.data().executable_ops(),
            &f.data().constants,
        ));
        compile::force_profit_gate_for_test(true);
        f.run();
        f.expect_aot();
        let old = f.current();
        for _ in 0..3 {
            f.run();
        }
        assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
        f.run();
        f.expect_jit(old);
        assert_eq!(
            f.current().regalloc,
            RegallocChoice::Fast,
            "the existing call-heavy allocator veto remains in force"
        );
        assert!(
            f.current().profit_gate_bypassed,
            "an already-native call-glue member earned the JIT compile"
        );
        // Cached spec calls can earn native service while source heat stays
        // low. Losing the per-mutator cache must retain that admission.
        f.data().jit_runtime().set_heat_for_test(0);
        cache::clear();
        f.run();
        assert!(
            !f.current().is_aot_backed(),
            "evicted hot call glue must return directly to JIT native service"
        );
        assert!(f.current().profit_gate_bypassed);
        assert_eq!(stats::compile_stats_snapshot().aot_loads, 1);
    }
}

#[test]
fn aot_retier_failed_upgrade_keeps_preloaded_code_native() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, true, 2);
    f.run();
    let old = f.current();
    f.run();
    bg::force_deferred_install_for_test(true);
    bg::fail_next_backend_for_test();
    for _ in 0..12 {
        f.run();
        assert!(std::ptr::eq(f.current(), old));
        assert!(!old.retired.get());
    }
    assert_eq!(old.obs.t2.state.get(), T2State::Kept);
    assert_eq!(old.obs.t2.budget.get(), DISARMED);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(cache::cache_entry_kind_for_test(f.id()), "compiled");
    assert_eq!(stats::compile_stats_snapshot().aot_loads, 1);
    assert_eq!(
        stats::compile_stats_snapshot().total_compiles,
        1,
        "failed backend is not retried per entry"
    );
}

#[cfg(target_arch = "x86_64")]
#[test]
fn aot_retier_refused_upgrade_keeps_old_and_disarms_retries() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, true, 2);
    f.run();
    let old = f.current();
    f.run();
    bg::force_mode_for_test(Some(bg::BgMode::Threaded));
    bg::force_queue_cap_for_test(Some(0));
    let refused = bg::stats_snapshot().refused;
    for _ in 0..12 {
        f.run();
        assert!(std::ptr::eq(f.current(), old));
        assert!(!old.retired.get());
    }
    assert_eq!(old.obs.t2.state.get(), T2State::Kept);
    assert_eq!(old.obs.t2.budget.get(), DISARMED);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(
        bg::stats_snapshot().refused,
        refused + 1,
        "queue refusal is bounded"
    );
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 0);
}

#[test]
fn aot_retier_cached_symbol_calls_reach_request_without_advancing_heat() {
    cached_symbol_calls_reach_request(true);
}

#[test]
fn aot_retier_cached_legacy_symbol_calls_reach_request_without_advancing_heat() {
    cached_symbol_calls_reach_request(false);
}

fn force_compile_budget_denial(old: &CompiledLeaf) {
    old.obs.compile_us.set(u32::MAX);
    compile::force_tier2_policy_for_test(Some(Tier2PolicyKnob {
        stable: 1,
        attempts: 4,
        budget_pct: 1,
        floor_ms: 0,
        max_reopt: 3,
    }));
}

#[test]
fn aot_retier_request_budget_denial_keeps_native_without_retrying() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, true, 2);
    f.run();
    let old = f.current();
    force_compile_budget_denial(old);
    for _ in 0..12 {
        f.run();
        assert!(std::ptr::eq(f.current(), old));
        assert!(!old.retired.get());
    }
    assert_eq!(old.obs.t2.state.get(), T2State::Kept);
    assert_eq!(old.obs.t2.budget.get(), DISARMED);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 0);
}

#[test]
fn aot_retier_compile_seam_budget_denial_keeps_native_without_retrying() {
    if forced_regalloc().is_some() {
        return;
    }
    let mut f = Fixture::add5(true, true, 2);
    f.run();
    let old = f.current();
    f.run();
    assert_eq!(old.obs.t2.state.get(), T2State::Due(T2Upgrade::Retier));
    assert_eq!(old.obs.t2.reserved_us.get(), 0, "Due owns no reservation");
    force_compile_budget_denial(old);
    for _ in 0..12 {
        f.run();
        assert!(std::ptr::eq(f.current(), old));
        assert!(!old.retired.get());
    }
    assert_eq!(old.obs.t2.state.get(), T2State::Kept);
    assert_eq!(old.obs.t2.budget.get(), DISARMED);
    assert_eq!(old.obs.t2.reserved_us.get(), 0);
    assert_eq!(stats::compile_stats_snapshot().total_compiles, 0);
}
