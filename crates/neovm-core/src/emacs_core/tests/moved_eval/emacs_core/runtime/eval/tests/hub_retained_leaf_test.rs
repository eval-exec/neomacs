//! Retained-cache readers observe the original leaf across eviction and clear.
use neovm_eval::emacs_core::bytecode::value_api::TaggedValueBytecodeExt as _;
use neovm_eval::emacs_core::bytecode::{
    ByteCodeFunction, ByteCodeFunctionExt as _, Op, StackDepth,
};
use neovm_eval::emacs_core::jit::{self, RuntimeStateTieringExt as _, cache};
use neovm_eval::test_support::authority::{RetainedLeafForTest, TestOptMode, TestProfitPolicy};
use neovm_value::value::{LambdaParams, Value};

fn with_entry(
    body: impl FnOnce(
        &mut neovm_eval::emacs_core::eval::Context,
        &ByteCodeFunction,
        &RetainedLeafForTest,
    ),
) {
    struct Knobs {
        profit: TestProfitPolicy,
        jit_off: bool,
    }
    impl Drop for Knobs {
        fn drop(&mut self) {
            neovm_eval::test_support::authority::replace_profit_gate_token_for_test(self.profit);
            jit::force_jit_off_for_test(self.jit_off);
        }
    }
    let _knobs = Knobs {
        profit: neovm_eval::test_support::authority::replace_profit_gate_token_for_test(
            TestProfitPolicy::Bypass,
        ),
        jit_off: jit::jit_forced_off_for_test(),
    };
    jit::force_jit_off_for_test(false);
    let _mode =
        neovm_eval::test_support::authority::opt_mode_token_scope_for_test(TestOptMode::Off);
    let _background =
        neovm_eval::test_support::authority::enter_test_mode_for_test(jit::BgMode::Sync);
    let mut context = crate::test_utils::new_context();
    let mut source = ByteCodeFunction::new(LambdaParams::simple(Vec::new()));
    source.lexical = true;
    source.ops = vec![Op::Constant(0), Op::Return];
    source.constants = vec![Value::fixnum(7)].into();
    source.max_stack = StackDepth::for_test(1);
    source.seal_hand_assembled_ops();
    source.jit_runtime().set_hot_for_test();
    neovm_eval::test_support::authority::prepare_entry_for_test(&mut context, &source);
    let leaf = neovm_eval::test_support::authority::retained_current_leaf_for_test(
        source.jit_runtime().compiled_id().unwrap(),
    )
    .expect("prepared entry leaf");
    body(&mut context, &source, &leaf);
}

#[test]
fn hub_retained_leaf_observes_the_original_and_resolved_entry() {
    crate::test_utils::init_test_tracing();
    with_entry(|context, source, leaf| {
        let runtime = source.jit_runtime();
        let id = runtime.compiled_id().unwrap();
        assert!(!leaf.retired());
        assert_eq!(
            neovm_eval::test_support::authority::compiled_leaf_identity_for_test(id),
            Some(leaf.identity())
        );
        let resolved =
            neovm_eval::test_support::authority::resolve_retained_leaf_for_test(context, source)
                .expect("resolved current entry");
        assert_eq!(resolved.identity(), leaf.identity());
        assert_eq!(resolved.entry_identity(), leaf.entry_identity());
        assert_eq!(
            neovm_eval::test_support::authority::resolve_compiled_leaf_identity_for_test(
                context, source
            ),
            Some(leaf.identity())
        );
        assert!(
            neovm_eval::test_support::authority::retained_current_leaf_for_test(u64::MAX).is_none()
        );
        let callable = Value::make_bytecode(source.clone());
        context
            .apply(callable, Vec::new())
            .expect("real native call");
        assert_eq!(
            neovm_eval::test_support::authority::armed_leaf_identity_for_test(
                runtime,
                cache::leaf_slot_epoch()
            ),
            Some(leaf.identity()),
            "the real apply seam arms the exact resolved leaf"
        );
        runtime.disarm_leaf_slot();
        assert_eq!(
            neovm_eval::test_support::authority::armed_leaf_identity_for_test(
                runtime,
                cache::leaf_slot_epoch()
            ),
            None
        );
    });
}

#[test]
fn hub_retained_leaf_reports_original_retirement_after_replacement() {
    crate::test_utils::init_test_tracing();
    with_entry(|context, source, leaf| {
        let id = source.jit_runtime().compiled_id().unwrap();
        let identity = leaf.identity();
        let entry_identity = leaf.entry_identity();
        neovm_eval::test_support::authority::evict_compiled_for_test(id);
        assert!(leaf.retired());
        assert!(neovm_eval::test_support::authority::retained_current_leaf_for_test(id).is_none());
        neovm_eval::test_support::authority::prepare_entry_for_test(context, source);
        let replacement = neovm_eval::test_support::authority::retained_current_leaf_for_test(id)
            .expect("replacement leaf");
        assert_ne!(replacement.identity(), identity);
        assert!(!replacement.retired());
        assert_eq!(leaf.identity(), identity);
        assert_eq!(leaf.entry_identity(), entry_identity);
        assert!(leaf.retired());
        // SAFETY: this live owner Context stays dormant for the retained
        // constant-only leaf call; no heap replacement or collection occurs.
        assert_eq!(
            unsafe { leaf.call(context as *mut _ as *mut u8, &[]) },
            neovm_eval::emacs_core::jit::compile::NativeRun::Ok(Value::fixnum(7).bits()),
            "retained original code remains callable for its suspended continuation"
        );
    });
}

#[test]
fn hub_retained_leaf_keeps_its_observations_live_after_cache_clear() {
    crate::test_utils::init_test_tracing();
    with_entry(|_, source, leaf| {
        let id = source.jit_runtime().compiled_id().unwrap();
        let identity = leaf.identity();
        let entry_identity = leaf.entry_identity();
        neovm_eval::test_support::authority::clear_jit_cache_for_test();
        assert!(neovm_eval::test_support::authority::retained_current_leaf_for_test(id).is_none());
        assert_eq!(leaf.identity(), identity);
        assert_eq!(leaf.entry_identity(), entry_identity);
        let _ = leaf.retired();
    });
}

#[test]
fn hub_osr_callback_retains_its_leaf_across_eviction_and_cache_clear() {
    crate::test_utils::init_test_tracing();
    with_entry(|context, _, _| {
        let mut source =
            ByteCodeFunction::new(LambdaParams::simple(vec![neovm_value::intern::intern(
                "hub-osr-counter",
            )]));
        source.lexical = true;
        source.ops = vec![
            Op::StackRef(0),
            Op::Constant(0),
            Op::Lss,
            Op::GotoIfNil(6),
            Op::Add1,
            Op::Goto(0),
            Op::Pop,
            Op::Constant(1),
            Op::Return,
        ];
        source.constants = vec![Value::fixnum(1), Value::fixnum(7)].into();
        source.max_stack = StackDepth::for_test(16);
        source.seal_hand_assembled_ops();
        source.jit_runtime().set_hot_for_test();
        neovm_eval::test_support::authority::prepare_osr_for_test(context, &source, 0);
        let id = source.jit_runtime().compiled_id().unwrap();
        let retained = neovm_eval::test_support::authority::retained_osr_leaf_for_test(&source, 0)
            .expect("original OSR leaf retained before callback");
        assert_eq!(
            neovm_eval::test_support::authority::osr_leaf_identity_for_test(&source, 0),
            Some(retained.identity())
        );
        let observed =
            neovm_eval::test_support::authority::with_osr_leaf_for_test(&source, 0, |leaf| {
                let arity = leaf.arity();
                let relocations = leaf.reloc_values().to_vec();
                neovm_eval::test_support::authority::evict_compiled_for_test(id);
                neovm_eval::test_support::authority::clear_jit_cache_for_test();
                // Both cache stores have released their Rc; this read is kept
                // valid by the reference retained INSIDE the callback fence.
                assert_eq!(leaf.arity(), arity);
                assert_eq!(leaf.reloc_values(), relocations);
                arity
            });
        assert!(observed.is_some());
        assert!(
            retained.retired(),
            "the retained original OSR leaf reports eviction"
        );
        assert!(
            neovm_eval::test_support::authority::retained_osr_leaf_for_test(&source, 0).is_none()
        );
        assert!(
            neovm_eval::test_support::authority::with_osr_leaf_for_test(&source, 0, |_| ())
                .is_none()
        );
    });
}
