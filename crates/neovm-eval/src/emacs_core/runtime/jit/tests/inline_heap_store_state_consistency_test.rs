//! Generated journal callbacks and shim stores must use the same World state.
use super::*;
use neovm_value::tagged::collection_reads::{
    CompiledJournalMode, capture, compiled_journal_mode, journal_since_for_test,
};
use neovm_value::tagged::mutate::LispCollectionRevision;

const CHILD_ARM: &str = "NEOVM_TEST_STOREFAST_INLINE_ARM";
const EVIDENCE: &str = "STOREFAST_JOURNAL:";

#[test]
fn storefast_inline_heap_write_on_and_off_share_active_capture_journal() {
    if let Ok(arm) = std::env::var(CHILD_ARM) {
        run_arm(&arm);
        return;
    }
    // The write knob is immutable once read. Each arm starts a fresh test
    // process instead of changing process environment after initialization.
    let name = format!(
        "{}::storefast_inline_heap_write_on_and_off_share_active_capture_journal",
        module_path!().strip_prefix("neovm_core::").unwrap()
    );
    let mut evidence = Vec::new();
    for arm in ["on", "off"] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", &name, "--nocapture", "--test-threads=1"])
            .env(CHILD_ARM, arm)
            .env("NEOVM_JIT_INLINE_HEAP_WRITE", arm)
            .env("NEOVM_JIT_INLINE", "on")
            .env("NEOVM_JIT_GEN0_COLLECTION_JOURNAL", "eager")
            .env_remove("RUST_LOG")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{arm} arm failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let stdout = String::from_utf8(output.stdout).unwrap();
        let row = stdout
            .lines()
            .find_map(|line| {
                line.find(EVIDENCE)
                    .map(|offset| line[offset + EVIDENCE.len()..].to_owned())
            })
            .expect("the selected child ran and returned journal evidence");
        evidence.push(row);
    }
    assert_eq!(
        evidence[0], evidence[1],
        "inline and shim journal states agree"
    );
}

fn run_arm(arm: &str) {
    assert_eq!(super::super::jit_inline_heap_write_on(), arm == "on");
    // Eager mode exercises emit_collection_write itself while captures are
    // active. Observed mode intentionally refuses observed stores to shims.
    assert_eq!(compiled_journal_mode(), CompiledJournalMode::Eager);
    let mut context = Context::new_minimal_vm_harness(crate::test_utils::unit_runtime());
    context.set_gc_threshold(usize::MAX);
    let owners = [
        Value::cons(Value::NIL, Value::NIL),
        Value::vector(vec![Value::NIL]),
        Value::make_record(vec![Value::NIL]),
    ];
    let roots = context.save_specpdl_roots();
    for &owner in &owners {
        context.push_specpdl_root(owner);
    }
    let setcar = compile_bytecode_function(&store_fn(Op::Setcar)).expect("setcar compiles");
    let setcdr = compile_bytecode_function(&store_fn(Op::Setcdr)).expect("setcdr compiles");
    let aset = compile_bytecode_function(&aset_fn()).expect("aset compiles");
    assert_eq!(setcar.needs_vmctx, arm == "on");
    assert_eq!(setcdr.needs_vmctx, arm == "on");
    assert_eq!(aset.needs_vmctx, arm == "on");
    let (_, completed) = capture(|| {
        owners[0].cons_car();
        owners[1].as_vector_data();
        owners[2].as_record_data();
    });
    let completed = completed.unwrap();
    assert!(completed.unchanged());
    let before = LispCollectionRevision::current();
    let shims = shim_calls() + aset_shim_calls();
    let ctx = &mut context as *mut Context as *mut u8;
    let mut results = Vec::new();
    let (_, active) = capture(|| {
        owners[0].cons_car();
        owners[1].as_vector_data();
        owners[2].as_record_data();
        results.push(native(
            ctx,
            &setcar,
            &[owners[0], Value::fixnum(7)],
            "setcar",
        ));
        results.push(native(
            ctx,
            &setcdr,
            &[owners[0], Value::fixnum(7)],
            "setcdr",
        ));
        for owner in [owners[1], owners[2]] {
            results.push(native(
                ctx,
                &aset,
                &[owner, Value::fixnum(0), Value::fixnum(7)],
                "aset",
            ));
        }
    });
    let steps = LispCollectionRevision::current().steps_since_for_test(before);
    let journal: Vec<_> = journal_since_for_test(before)
        .unwrap()
        .into_iter()
        .map(|bits| {
            owners
                .iter()
                .position(|owner| owner.bits() == bits)
                .expect("a tested owner")
        })
        .collect();
    assert_eq!(steps, 4);
    assert_eq!(journal, [0, 0, 1, 2]);
    assert_eq!(results, ["7", "7", "7", "7"]);
    assert!(!completed.unchanged());
    assert!(active.is_none());
    assert_eq!(owners[0].cons_car(), Value::fixnum(7));
    assert_eq!(owners[0].cons_cdr(), Value::fixnum(7));
    assert_eq!(owners[1].as_vector_data().unwrap()[0], Value::fixnum(7));
    assert_eq!(owners[2].as_record_data().unwrap()[0], Value::fixnum(7));
    assert_eq!(
        shim_calls() + aset_shim_calls() - shims,
        if arm == "on" { 0 } else { 4 },
        "the on arm executes generated stores; the off arm executes shims"
    );
    println!(
        "{EVIDENCE}steps={steps};owners={journal:?};results={results:?};completed_valid={};active_valid={}",
        completed.unchanged(),
        active.is_some()
    );
    context.restore_specpdl_roots(roots);
}
