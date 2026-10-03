//! Selected T1 array observations and stable source-kind snapshots. Future first isolates exercise emitted normal T1, not manually
//! invented kind declarations. All answers derive from Tier0; GNU Faref/Baref
//! vector/record/string cases are frozen67. No handwritten expected payloads.
//! Threading: Context/LeafObs/cache are mutator-owned; source site joins alone
//! are multiwriter atomics. Settings restore compiler-test scalar overrides.

use crate::emacs_core::jit::compile::opt_census::SelectedTier;

use super::compile_pipeline_tests::{captured_clif, function};
use super::*;
use crate::emacs_core::jit::bg::{BgMode, force_mode_for_test};
use crate::emacs_core::jit::cache;
use crate::emacs_core::jit::feedback::arrays::{
    ArraySiteFeedback, ObservedArrayKind, PlainArrayKind,
};
use crate::emacs_core::jit::feedback::{FeedbackMode, force_feedback_mode_for_test};
use crate::emacs_core::jit::stats::{ObserveOverride, force_observe_for_test};
use crate::emacs_core::jit::tier2::{CompileTier, T2Origin, T2State, T2Upgrade};
use crate::emacs_core::print::print_value;

struct Settings;
impl Settings {
    fn enter(range: bool) -> Self {
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        force_opt_passes_for_test(Some(OptPasses {
            range,
            ..OptPasses::default()
        }));
        force_tier2_for_test(Some(Tier2Knob {
            on: true,
            window: 20,
            loop_credit: 64,
        }));
        force_tier2_policy_for_test(Some(Tier2PolicyKnob {
            stable: 2,
            attempts: 4,
            budget_pct: 0,
            floor_ms: 5,
            max_reopt: 3,
        }));
        force_feedback_mode_for_test(Some(FeedbackMode::Off));
        force_mode_for_test(Some(BgMode::Sync));
        crate::emacs_core::jit::inline::force_inline_for_test(Some(false));
        force_deopt_for_test(false);
        force_profit_gate_for_test(false);
        force_observe_for_test(ObserveOverride {
            entry_count: true,
            ..Default::default()
        });
        Self
    }
}
impl Drop for Settings {
    fn drop(&mut self) {
        force_opt_for_test(None, None);
        force_opt_passes_for_test(None);
        force_tier2_for_test(None);
        force_tier2_policy_for_test(None);
        force_feedback_mode_for_test(None);
        force_mode_for_test(None);
        crate::emacs_core::jit::inline::force_inline_for_test(None);
        force_deopt_for_test(false);
        force_profit_gate_for_test(true);
        force_observe_for_test(ObserveOverride::default());
        super::shim_refs::force_lazy_shims_for_test(true);
    }
}
struct Roots(usize);
impl Roots {
    fn new(values: &[Value]) -> Self {
        let n = crate::emacs_core::eval::save_scratch_gc_roots();
        crate::emacs_core::eval::push_scratch_gc_roots(values);
        Self(n)
    }
}
impl Drop for Roots {
    fn drop(&mut self) {
        crate::emacs_core::eval::restore_scratch_gc_roots(self.0);
    }
}
fn source() -> ByteCodeFunction {
    function(
        vec![Op::StackRef(0), Op::Constant(0), Op::Aref, Op::Return],
        vec![Value::fixnum(0)],
        1,
    )
}
fn expected(ctx: &mut Context, f: &ByteCodeFunction, args: &[Value]) -> String {
    let mut vm = Vm::from_context(ctx);
    vm.force_interpreter_only_for_test();
    print_value(&vm.execute(f, args.to_vec()).unwrap())
}
fn t1(ctx: &Context, f: &ByteCodeFunction) -> CompiledLeaf {
    compile_bytecode_function_requested(
        f,
        Some(&ctx.obarray),
        CompileRequest {
            regalloc: lowering::RegallocPolicy::Auto,
            bypass_profit_gate: true,
            origin: crate::emacs_core::jit::stats::CompileOrigin::Direct,
            tier: CompileTier::T1,
        },
    )
    .expect("ordinary Aref T1 lowers before observer evidence")
}
fn native(ctx: &mut Context, f: &ByteCodeFunction, leaf: &CompiledLeaf, arg: Value) -> String {
    let NativeRun::Ok(bits) =
        leaf.call_consts(ctx as *mut Context as *mut u8, f.constants.as_ptr(), &[arg])
    else {
        panic!("recording preserves baseline native Aref success")
    };
    print_value(&Value::from_bits(bits))
}

#[test]
fn opt_array_closed_window_never_evaluates_kind_inspection() {
    let inspections = std::cell::Cell::new(0);
    // This is the actual continuation used by the runtime helper. The closed
    // path never evaluates its physical-kind inspection; no invalid Lisp word
    // or pointer is needed to make an unobserved header read fail a test.
    super::array_profile::when_open(false, || {
        inspections.set(inspections.get() + 1);
        panic!("closed tier window inspected an array kind");
    });
    assert_eq!(inspections.get(), 0);
    super::array_profile::when_open(true, || inspections.set(inspections.get() + 1));
    assert_eq!(inspections.get(), 1);
}

#[test]
fn opt_array_feedback_concurrent_kind_join_never_loses_other() {
    let site = std::sync::Arc::new(ArraySiteFeedback::default());
    std::thread::scope(|scope| {
        for kind in [
            ObservedArrayKind::PlainVector,
            ObservedArrayKind::PlainRecord,
            ObservedArrayKind::Other,
        ] {
            let site = std::sync::Arc::clone(&site);
            scope.spawn(move || {
                for _ in 0..128 {
                    site.observe(kind);
                }
            });
        }
    });
    assert_eq!(
        site.mask().plain(),
        None,
        "Other is sticky under concurrent OR joins"
    );
    assert_eq!(site.samples(), 384); // bounded diagnostics, no wrapping premise
    let site = ArraySiteFeedback::default();
    site.observe(ObservedArrayKind::PlainVector);
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::Vector));
    site.observe(ObservedArrayKind::PlainRecord);
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::VectorOrRecord));
    let stable = site.mask();
    site.observe(ObservedArrayKind::PlainVector);
    assert_eq!(
        site.mask(),
        stable,
        "more samples do not change a stable tier version"
    );
}

#[test]
fn opt_array_native_t1_records_only_live_window_and_holds_source() {
    let _settings = Settings::enter(true);
    let mut ctx = Context::new(); // before any heap operands or symbol handles
    let vector = Value::vector(vec![Value::fixnum(13)]);
    let record = Value::make_record(vec![Value::symbol("record-tag"), Value::fixnum(29)]);
    let string = Value::string("x");
    let _roots = Roots::new(&[vector, record, string]);
    let f = source();
    let answers = [vector, record, string].map(|arg| expected(&mut ctx, &f, &[arg]));
    let leaf = t1(&ctx, &f);
    assert_eq!(leaf.selected_tier(), SelectedTier::Baseline);
    assert!(
        leaf.feedback_holds
            .iter()
            .any(|held| std::sync::Arc::ptr_eq(held, &f.jit_runtime().share_state())),
        "Feedback=off still retains array source pointers"
    );
    let array_sites = f.jit_runtime().array_sites().unwrap();
    let site = array_sites.site_at(2).unwrap();
    leaf.obs.t2.budget.set(1000);
    assert_eq!(native(&mut ctx, &f, &leaf, vector), answers[0]);
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::Vector));
    assert_eq!(native(&mut ctx, &f, &leaf, record), answers[1]);
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::VectorOrRecord));
    let masks = site.mask();
    let samples = site.samples();
    for state in [
        T2State::BudgetWait,
        T2State::Due(T2Upgrade::Feedback),
        T2State::Kept,
        T2State::Upgraded(T2Upgrade::Feedback),
    ] {
        leaf.obs.t2.state.set(state);
        leaf.obs.t2.budget.set(1000);
        assert_eq!(native(&mut ctx, &f, &leaf, string), answers[2]);
        assert_eq!(site.mask(), masks);
        assert_eq!(site.samples(), samples);
    }
    leaf.obs.t2.state.set(T2State::Idle);
    leaf.obs.t2.budget.set(1000);
    assert_eq!(native(&mut ctx, &f, &leaf, string), answers[2]);
    assert_eq!(site.mask().plain(), None);
    assert_eq!(site.samples(), samples + 1);
}

#[test]
fn opt_array_native_range_off_preserves_t1_clif_even_eager_shims() {
    let _settings = Settings::enter(false);
    let mut ctx = Context::new();
    let arg = Value::vector(vec![Value::fixnum(7)]);
    let _roots = Roots::new(&[arg]);
    let f = source();
    let answer = expected(&mut ctx, &f, &[arg]);
    fn mask(text: &str) -> String {
        // Established campaign policy: replace ONLY canonical pointer-valued
        // hex literals 2^40..2^47; never integer tags/domain/overflow constants.
        let mut out = String::new();
        let bytes = text.as_bytes();
        let mut at = 0;
        while at < bytes.len() {
            if bytes[at..].starts_with(b"0x") {
                let mut end = at + 2;
                while end < bytes.len() && (bytes[end].is_ascii_hexdigit() || bytes[end] == b'_') {
                    end += 1;
                }
                let number = text[at + 2..end].replace('_', "");
                if u64::from_str_radix(&number, 16)
                    .is_ok_and(|n| ((1u64 << 40)..(1u64 << 47)).contains(&n))
                {
                    out.push_str("0xPTR");
                    at = end;
                    continue;
                }
            }
            out.push(bytes[at] as char);
            at += 1;
        }
        out
    }
    for lazy in [true, false] {
        super::shim_refs::force_lazy_shims_for_test(lazy);
        force_opt_for_test(Some(OptMode::Off), Some(OptAdmit::ALL));
        let off = captured_clif(|| {
            let leaf = t1(&ctx, &f);
            assert_eq!(native(&mut ctx, &f, &leaf, arg), answer);
        })
        .join("\n");
        force_opt_for_test(Some(OptMode::Opt), Some(OptAdmit::ALL));
        let none = captured_clif(|| {
            let leaf = t1(&ctx, &f);
            assert_eq!(native(&mut ctx, &f, &leaf, arg), answer);
        })
        .join("\n");
        assert_eq!(
            mask(&off),
            mask(&none),
            "range-off opt preserves old T1 import/order"
        );
        assert!(
            f.jit_runtime().array_sites().is_none(),
            "range-off allocates no array table"
        );
    }
}

#[test]
fn opt_array_native_kind_transition_rearms_stable_version_but_samples_do_not() {
    let _settings = Settings::enter(true);
    force_tier2_for_test(Some(Tier2Knob {
        on: true,
        window: 2,
        loop_credit: 64,
    }));
    let mut ctx = Context::new();
    let vector = Value::vector(vec![Value::fixnum(13)]);
    let record = Value::make_record(vec![Value::symbol("record-tag"), Value::fixnum(29)]);
    let _roots = Roots::new(&[vector, record]);
    let f = source();
    let a = expected(&mut ctx, &f, &[vector]);
    let b = expected(&mut ctx, &f, &[record]);
    let run = |ctx: &mut Context, arg: Value, answer: &str| {
        let bits = cache::try_run_compiled(ctx, &f, Value::NIL, &[arg])
            .unwrap()
            .unwrap();
        assert_eq!(print_value(&Value::from_bits(bits)), answer);
    };
    run(&mut ctx, vector, &a);
    run(&mut ctx, vector, &a);
    let id = f.jit_runtime().compiled_id().unwrap();
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    // SAFETY: current/retained source and leaves remain cached on this mutator.
    let original = unsafe { &*ptr };
    assert_eq!(original.selected_tier(), SelectedTier::Baseline);
    assert_eq!(original.obs.t2.state.get(), T2State::Idle);
    let unstable = crate::emacs_core::jit::tier2::stats().unstable;
    let array_sites = f.jit_runtime().array_sites().unwrap();
    let site = array_sites.site_at(2).unwrap();
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::Vector));
    run(&mut ctx, record, &b);
    run(&mut ctx, record, &b);
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::VectorOrRecord));
    assert_eq!(
        crate::emacs_core::jit::tier2::stats().unstable,
        unstable + 1,
        "new physical kind must change the exact stability snapshot"
    );
    assert_eq!(original.obs.t2.state.get(), T2State::Idle);
    assert_eq!(
        original.obs.t2.budget.get(),
        2,
        "transition opens a full new stable window"
    );
    let version_mask = site.mask();
    let samples = site.samples();
    run(&mut ctx, record, &b);
    run(&mut ctx, record, &b);
    assert_eq!(site.mask(), version_mask);
    assert!(site.samples() >= samples);
    assert_eq!(
        crate::emacs_core::jit::tier2::stats().unstable,
        unstable + 1,
        "ordinary sample-count increments cannot make the version unstable"
    );
    assert_eq!(
        original.obs.t2.state.get(),
        T2State::Due(T2Upgrade::Feedback)
    );
    run(&mut ctx, record, &b);
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    let opt = unsafe { &*ptr };
    assert_eq!(opt.selected_tier(), SelectedTier::Opt);
    assert_eq!(opt.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Feedback));
    let frozen = site.samples();
    for _ in 0..20 {
        run(&mut ctx, record, &b);
    }
    assert_eq!(
        site.samples(),
        frozen,
        "T2 serves entries without reopening T1 observation"
    );
}

#[test]
fn opt_array_native_observed_dynamic_arefs_upgrade_and_serve_normal_entries() {
    const CALLS: u64 = 400;
    let _settings = Settings::enter(true);
    let mut ctx = Context::new();
    let arg = Value::vector(vec![Value::fixnum(13), Value::fixnum(23)]);
    let _roots = Roots::new(&[arg]);
    // A high then lower index in the same straight-line region has real BCE.
    let f = function(
        vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Aref,
            Op::StackRef(1),
            Op::Constant(1),
            Op::Aref,
            Op::List(2),
            Op::Return,
        ],
        vec![Value::fixnum(1), Value::fixnum(0)],
        1,
    );
    let answer = expected(&mut ctx, &f, &[arg]);
    for _ in 0..CALLS {
        let bits = cache::try_run_compiled(&mut ctx, &f, Value::NIL, &[arg])
            .unwrap()
            .expect("ordinary cache entry ran natively");
        assert_eq!(print_value(&Value::from_bits(bits)), answer);
    }
    let id = f.jit_runtime().compiled_id().unwrap();
    let ptr = cache::compiled_leaf_ptr_for_test(id).unwrap();
    // SAFETY: current/retained leaves stay cached on this test's mutator; no
    // cache clear occurs while pointers are inspected or the source is live.
    let opt = unsafe { &*ptr };
    assert_eq!(opt.selected_tier(), SelectedTier::Opt);
    assert_eq!(opt.obs.t2.origin, T2Origin::Upgrade(T2Upgrade::Feedback));
    let fallback = opt.tier1_fallback.borrow();
    let fallback = fallback.as_ref().expect("normal T1 retained");
    let sites = f.jit_runtime().array_sites().unwrap();
    for pc in [2, 5] {
        assert_eq!(
            sites.site_at(pc).unwrap().mask().plain(),
            Some(PlainArrayKind::Vector)
        );
    }
    let ranges = crate::emacs_core::jit::compile::opt_census::snapshot(&opt.obs)
        .opt_range
        .expect("native selected range census");
    assert_eq!(
        ranges.bounds_checks_elided, 1,
        "real source Aref BCE entered normally"
    );
    assert!(
        opt.obs.entries.get() * 10 >= (CALLS - 24) * 9,
        "actual opt entry counts serve at least90% after initial stable windows"
    );
    assert_eq!(
        fallback.obs.t2.state.get(),
        T2State::Upgraded(T2Upgrade::Feedback)
    );
    let frozen = sites.site_at(2).unwrap().samples();
    for _ in 0..20 {
        let bits = cache::try_run_compiled(&mut ctx, &f, Value::NIL, &[arg])
            .unwrap()
            .unwrap();
        assert_eq!(print_value(&Value::from_bits(bits)), answer);
    }
    assert_eq!(
        sites.site_at(2).unwrap().samples(),
        frozen,
        "Opt records no T1 shapes"
    );
}

#[test]
fn opt_array_snapshot_keeps_fused_caller_mapping_and_owned_worker_masks() {
    let _settings = Settings::enter(true);
    let mut ctx = Context::new();
    let arg = Value::vector(vec![Value::fixnum(13)]);
    let callee = Value::make_bytecode(function(
        vec![Op::StackRef(0), Op::Add1, Op::Return],
        vec![],
        1,
    ));
    let _roots = Roots::new(&[arg, callee]);
    let f = function(
        vec![
            Op::Constant(0),
            Op::Constant(1),
            Op::Call(1),
            Op::Pop,
            Op::StackRef(0),
            Op::Constant(2),
            Op::Aref,
            Op::Return,
        ],
        vec![callee, Value::fixnum(1), Value::fixnum(0)],
        1,
    );
    let answer = expected(&mut ctx, &f, &[arg]);
    let leaf = t1(&ctx, &f);
    leaf.obs.t2.budget.set(1000);
    assert_eq!(native(&mut ctx, &f, &leaf, arg), answer);
    let array_sites = f.jit_runtime().array_sites().unwrap();
    let site = array_sites.site_at(6).unwrap();
    assert_eq!(site.mask().plain(), Some(PlainArrayKind::Vector));

    let snapshot = super::snapshot::SelectedFeedbackSnapshot::take(&f);
    let _scope = super::array_snapshot::ArrayFeedbackScope::enter(snapshot.array.clone());
    crate::emacs_core::jit::inline::force_inline_for_test(Some(true));
    let feedback =
        vec![crate::emacs_core::jit::NumericFeedback::FixnumOnly; f.executable_ops().len()];
    let body = std::rc::Rc::new(
        crate::emacs_core::jit::inline::fuse_calls(
            f.executable_ops(),
            &f.constants,
            f.executable_gnu_byte_offset_map(),
            1,
            &feedback,
        )
        .expect("actual small fixnum callee is fusible"),
    );
    let _fused = crate::emacs_core::jit::inline::FusedScope::enter(body.clone());
    let caller_aref = body
        .ops
        .iter()
        .enumerate()
        .find(|&(pc, op)| *op == Op::Aref && body.region_at(pc).is_none())
        .unwrap()
        .0;
    assert_ne!(
        caller_aref, 6,
        "real fuser shifts the caller's original Aref pc"
    );
    assert_eq!(
        super::array_snapshot::active_kind(caller_aref),
        Some(PlainArrayKind::Vector)
    );
    for pc in 0..body.ops.len() {
        if body.region_at(pc).is_some() {
            assert_eq!(
                super::array_snapshot::active_kind(pc),
                None,
                "callee region cannot borrow a caller array profile"
            );
        }
    }
    let hints = super::array_snapshot::admission(body.ops.len(), &body.constants, 0);
    assert_eq!(
        hints.site_types[caller_aref],
        crate::emacs_core::jit::opt::types::TypeSet::VECTOR
    );
    // A later shared-source transition cannot mutate an already owned worker
    // snapshot. This is scalar table publication, not cross-thread Lisp use.
    site.observe(ObservedArrayKind::Other);
    assert_eq!(site.mask().plain(), None);
    let worker = std::thread::spawn(move || {
        let _scope = super::array_snapshot::ArrayFeedbackScope::enter(snapshot.array);
        assert_eq!(
            super::array_snapshot::active_kind(6),
            Some(PlainArrayKind::Vector)
        );
        assert_eq!(
            hints.site_types[caller_aref],
            crate::emacs_core::jit::opt::types::TypeSet::VECTOR
        );
    });
    worker.join().unwrap();
}
