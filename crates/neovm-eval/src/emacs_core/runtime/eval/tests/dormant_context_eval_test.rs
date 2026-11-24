//! At most one active Context per thread. A second `Context::new`, pdump
//! load/restore, or `setup_thread_locals` while another live heap is installed
//! refuses in release before the first TLS write. Parking (`ParkedContext`)
//! suspends a `Context` with no eval API. Outer doors refuse when a foreign
//! heap is nested over a live Context.

use std::panic::{AssertUnwindSafe, catch_unwind};

use neovm_value::tagged::gc::{
    NestedTaggedHeapInstall, current_tagged_heap_identity, tagged_heap_is_current,
};

use super::*;

fn panic_text(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| {
            payload
                .downcast_ref::<&str>()
                .map(|text| (*text).to_string())
        })
        .unwrap_or_default()
}

#[test]
fn a_second_context_on_one_thread_refuses_while_one_is_active() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let first_id = first.tagged_heap().tagged_heap_identity();
    assert_eq!(current_tagged_heap_identity(), Some(first_id));

    let refused = catch_unwind(AssertUnwindSafe(|| {
        let _second = Context::new(crate::test_utils::unit_runtime());
    }));
    let text = panic_text(refused.expect_err("a second Context::new must refuse"));
    assert!(
        text.contains("dormant Context"),
        "constructor refusal must name the dormant Context, got {text}"
    );
    assert_eq!(
        current_tagged_heap_identity(),
        Some(first_id),
        "a refused constructor must leave the first heap installed"
    );

    drop(first);
    let second = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        current_tagged_heap_identity(),
        Some(second.tagged_heap().tagged_heap_identity())
    );
}

#[test]
fn parked_resume_refuses_heap_installed_and_has_no_eval_api() {
    let first = Context::new(crate::test_utils::unit_runtime());
    let first_id = first.tagged_heap().tagged_heap_identity();
    let parked = first.park_for_test();
    assert_ne!(current_tagged_heap_identity(), Some(first_id));

    let mut second = Context::new(crate::test_utils::unit_runtime());
    let second_id = second.tagged_heap().tagged_heap_identity();
    let refused = match parked.resume_for_test() {
        Err(error) => error,
        Ok(_restored) => panic!("resume under a live heap"),
    };
    assert_eq!(format!("{refused:?}"), "HeapInstalled");
    assert_eq!(refused.to_string(), "HeapInstalled");
    assert_eq!(current_tagged_heap_identity(), Some(second_id));
    assert_eq!(
        second
            .eval_str("7")
            .expect("the active Context still evaluates"),
        Value::fixnum(7)
    );

    let parked_second = second.park_for_test();
    // The refused resume dropped the first Context. A fresh one is the
    // empty-slot install, which is the path resume would have taken.
    drop(parked_second);
    let mut restored = Context::new(crate::test_utils::unit_runtime());
    assert_eq!(
        restored.eval_str("8").expect("empty slot constructs"),
        Value::fixnum(8)
    );
}

#[test]
fn park_then_resume_restores_the_same_context() {
    let mut first = Context::new(crate::test_utils::unit_runtime());
    first
        .eval_str("(setq park-token 3)")
        .expect("bind on the first context");
    let first_id = first.tagged_heap().tagged_heap_identity();
    let parked = first.park_for_test();

    let second = Context::new(crate::test_utils::unit_runtime());
    let parked_second = second.park_for_test();
    let mut first = parked.resume_for_test().expect("slot empty after park");
    assert_eq!(current_tagged_heap_identity(), Some(first_id));
    assert_eq!(
        first.eval_str("park-token").expect("resumed binding"),
        Value::fixnum(3)
    );
    drop(first);
    let mut second = parked_second.resume_for_test().expect("resume second");
    assert_eq!(
        second.eval_str("4").expect("second context evaluates"),
        Value::fixnum(4)
    );
}

#[test]
fn nested_bare_heap_restores_the_context_heap() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let id = ctx.tagged_heap().tagged_heap_identity();
    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    {
        // SAFETY: `foreign` and `ctx` both outlive `nest` with no live heap
        // references during install or drop.
        let nest = unsafe { ctx.nest_foreign_heap_for_test(foreign.root()) };
        assert!(tagged_heap_is_current(&foreign));
        assert_ne!(current_tagged_heap_identity(), Some(id));
        drop(nest);
    }
    assert_eq!(current_tagged_heap_identity(), Some(id));
    assert_eq!(ctx.eval_str("1").expect("restored door"), Value::fixnum(1));
}

#[test]
fn outer_doors_debug_assert_under_a_foreign_heap() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let id = ctx.tagged_heap().tagged_heap_identity();
    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    // SAFETY: `foreign` and `ctx` both outlive `nest` with no live heap
    // references during install or drop.
    let nest = unsafe { NestedTaggedHeapInstall::install_bare(foreign.root()) };

    for (label, call) in [
        ("eval_str", eval_str_door as fn(&mut Context)),
        ("eval_form", eval_form_door as fn(&mut Context)),
        ("eval_value", eval_value_door as fn(&mut Context)),
        ("eval_str_each", eval_str_each_door as fn(&mut Context)),
    ] {
        let caught = catch_unwind(AssertUnwindSafe(|| call(&mut ctx)));
        let text = panic_text(caught.expect_err(label));
        assert!(
            text.contains("dormant Context"),
            "{label} must refuse under a foreign heap, got {text}"
        );
        assert!(
            tagged_heap_is_current(&foreign),
            "{label} must not install the Context heap"
        );
    }
    drop(nest);
    assert_eq!(current_tagged_heap_identity(), Some(id));
    assert_eq!(
        ctx.eval_str("1").expect("door after restore"),
        Value::fixnum(1)
    );
}

fn eval_str_door(ctx: &mut Context) {
    let _ = ctx.eval_str("1");
}

fn eval_form_door(ctx: &mut Context) {
    let _ = ctx.eval_form(Value::fixnum(1));
}

fn eval_value_door(ctx: &mut Context) {
    let _ = ctx.eval_value(&Value::fixnum(1));
}

fn eval_str_each_door(ctx: &mut Context) {
    let _ = ctx.eval_str_each("1");
}

#[test]
fn pdump_restore_snapshot_and_load_refuse_over_live_foreign_heap() {
    let mut active = Context::new(crate::test_utils::unit_runtime());
    active
        .eval_str("(setq pdump-refusal-token 99)")
        .expect("bind on active context");
    let active_id = active.tagged_heap().tagged_heap_identity();
    let proof = active.hooks_installed();
    let snapshot = crate::emacs_core::pdump::snapshot_active_evaluator(&mut active);

    let refused = catch_unwind(AssertUnwindSafe(|| {
        let _ = crate::emacs_core::pdump::restore_snapshot(&snapshot, proof);
    }));
    let text = panic_text(refused.expect_err("restore_snapshot must refuse over a live heap"));
    assert!(
        text.contains("dormant Context"),
        "restore_snapshot refusal must name dormant Context, got {text}"
    );
    assert_eq!(
        current_tagged_heap_identity(),
        Some(active_id),
        "refused restore_snapshot must leave the active heap installed"
    );

    let dump_path = std::env::temp_dir().join(format!(
        "neomacs-pdump-refusal-{}-{}.pdmp",
        std::process::id(),
        active_id
    ));
    crate::emacs_core::pdump::dump_to_file(&active, &dump_path).expect("write test dump");
    let refused_load = catch_unwind(AssertUnwindSafe(|| {
        let _ = crate::emacs_core::pdump::load_from_dump_with_proof(&dump_path, proof);
    }));
    let _ = std::fs::remove_file(&dump_path);
    let load_text = panic_text(
        refused_load.expect_err("load_from_dump_with_proof must refuse over a live heap"),
    );
    assert!(
        load_text.contains("dormant Context"),
        "load_from_dump_with_proof refusal must name dormant Context, got {load_text}"
    );
    assert_eq!(
        current_tagged_heap_identity(),
        Some(active_id),
        "refused load_from_dump_with_proof must leave the active heap installed"
    );
    assert_eq!(
        active
            .eval_str("pdump-refusal-token")
            .expect("active context stays usable after refused pdump loads"),
        Value::fixnum(99)
    );
}

#[test]
fn pdump_clone_keeps_source_active_and_not_dormant() {
    let mut source = Context::new(crate::test_utils::unit_runtime());
    source
        .eval_str("(setq pdump-clone-source-token 11)")
        .expect("bind on source");
    let source_id = source.tagged_heap().tagged_heap_identity();

    // 1. clone_active_evaluator returns a ParkedContext and keeps source active.
    let cloned = crate::emacs_core::pdump::clone_active_evaluator(&mut source)
        .expect("clone_active_evaluator should succeed");
    assert_eq!(
        current_tagged_heap_identity(),
        Some(source_id),
        "clone_active_evaluator must leave source installed, not dormant"
    );
    let cons = source
        .apply(
            Value::symbol("cons"),
            vec![Value::fixnum(1), Value::fixnum(2)],
        )
        .expect("source.apply must allocate on source's own heap");
    assert!(
        source.tagged_heap().owns_heap_value_for_test(cons),
        "source.apply after clone must allocate in source's heap (#12898 E1)"
    );

    // 2. Resuming the clone refuses while source is still installed and
    // preserves the ParkedContext inside the error for retry.
    let err = match cloned.resume() {
        Err(error) => error,
        Ok(_) => panic!("resume must refuse while source is installed"),
    };
    assert_eq!(format!("{err:?}"), "HeapInstalled");
    assert_eq!(current_tagged_heap_identity(), Some(source_id));
    let cloned = err.into_parked();

    // 3. clone_evaluator(&mut Context) also returns a ParkedContext and keeps
    // source active with all heap-keyed caches (string-pos, regex) keyed to
    // source, verified through non-reactivating `source.apply` calls.
    let cloned_imm = crate::emacs_core::pdump::clone_evaluator(&mut source)
        .expect("clone_evaluator should succeed");
    assert_eq!(current_tagged_heap_identity(), Some(source_id));
    let cons_after = source
        .apply(
            Value::symbol("cons"),
            vec![Value::fixnum(10), Value::fixnum(20)],
        )
        .expect("source.apply after clone_evaluator must succeed");
    assert!(
        source.tagged_heap().owns_heap_value_for_test(cons_after),
        "source.apply after clone_evaluator must allocate on source's heap"
    );
    let multi = Value::string("αβγδεζηθικλμνξοπρστυφχψω");
    let ch = source
        .apply(Value::symbol("aref"), vec![multi, Value::fixnum(12)])
        .expect("source.apply(aref) on multibyte string after clone_evaluator");
    assert_eq!(ch, Value::fixnum('ν' as i64));
    let mut sp_roots = Vec::new();
    neovm_value::string_pos_cache::collect_string_pos_cache_gc_roots(
        &mut sp_roots,
        source_id,
        source.tagged_heap().gc_collections(),
        neovm_value::tagged::gc::CacheRootScan::Parked,
    );
    assert!(
        sp_roots.contains(&multi),
        "string-pos cache must be keyed to source's heap after clone_evaluator"
    );
    let matched = source
        .apply(
            Value::symbol("string-match"),
            vec![Value::string("γ.*ν"), multi],
        )
        .expect("source.apply(string-match) after clone_evaluator");
    assert_eq!(matched, Value::fixnum(2));
    assert_eq!(
        source
            .eval_str("pdump-clone-source-token")
            .expect("source stays usable after clone_evaluator"),
        Value::fixnum(11)
    );

    // 4. Parking source lets the clones resume one at a time.
    let parked_source = source.park_for_test();
    let mut cloned = cloned.resume().expect("resume after parking source");
    assert_ne!(current_tagged_heap_identity(), Some(source_id));
    assert_eq!(
        cloned
            .eval_str("pdump-clone-source-token")
            .expect("resumed clone evaluates"),
        Value::fixnum(11)
    );
    drop(cloned);

    let mut cloned_imm = cloned_imm.resume().expect("resume immutable clone");
    assert_eq!(
        cloned_imm
            .eval_str("pdump-clone-source-token")
            .expect("resumed immutable clone evaluates"),
        Value::fixnum(11)
    );
    drop(cloned_imm);

    let mut source = parked_source.resume_for_test().expect("resume source");
    assert_eq!(current_tagged_heap_identity(), Some(source_id));
    assert_eq!(
        source
            .eval_str("pdump-clone-source-token")
            .expect("source evaluates after resume"),
        Value::fixnum(11)
    );
}

#[test]
fn pdump_clone_evaluator_refuses_under_foreign_heap() {
    let mut source = Context::new(crate::test_utils::unit_runtime());
    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    let foreign_id = foreign.tagged_heap_identity();
    // SAFETY: `foreign` and `source` both outlive `nest`.
    let nest = unsafe { NestedTaggedHeapInstall::install_bare(foreign.root()) };
    for (label, call) in [
        (
            "clone_evaluator",
            crate::emacs_core::pdump::clone_evaluator
                as fn(&mut Context) -> Result<super::parked_context::ParkedContext, crate::emacs_core::pdump::DumpError>,
        ),
        (
            "clone_active_evaluator",
            crate::emacs_core::pdump::clone_active_evaluator
                as fn(&mut Context) -> Result<super::parked_context::ParkedContext, crate::emacs_core::pdump::DumpError>,
        ),
    ] {
        let refused = catch_unwind(AssertUnwindSafe(|| {
            let _ = call(&mut source);
        }));
        let text = panic_text(refused.expect_err(label));
        assert!(
            text.contains("dormant Context"),
            "{label} refusal must name dormant Context, got {text}"
        );
        assert_eq!(current_tagged_heap_identity(), Some(foreign_id));
    }
    drop(nest);
}

#[test]
fn pdump_reload_written_bootstrap_dump_round_trips_ok_and_err() {
    let dir = tempfile::tempdir().expect("tempdir");
    let good_path = dir.path().join("good.pdmp");
    let bad_path = dir.path().join("bad.pdmp");
    let late_bad_path = dir.path().join("late_bad.pdmp");
    std::fs::write(&bad_path, b"not a valid pdump image").expect("write bad dump");

    // Early Err path: invalid magic header fails before reconstruction and resumes eval.
    let mut eval = Context::new(crate::test_utils::unit_runtime());
    eval.eval_str("(setq reload-roundtrip-token 77)")
        .expect("bind on original eval");
    let orig_id = eval.tagged_heap().tagged_heap_identity();
    let proof = eval.hooks_installed();
    let (mut eval, _err) =
        match crate::emacs_core::load::reload_written_bootstrap_dump(eval, &bad_path, proof) {
            Err(pair) => pair,
            Ok(_) => panic!("corrupt dump must fail reload"),
        };
    assert_eq!(
        current_tagged_heap_identity(),
        Some(orig_id),
        "failed reload must resume the original Context"
    );
    assert_eq!(
        eval.eval_str("reload-roundtrip-token")
            .expect("resumed eval stays usable"),
        Value::fixnum(77)
    );

    // Late Err path: dump fails AFTER symbol-table loading (corrupted ObjectStarts),
    // and still restores fontset, runtime bindings, and eval cleanly.
    crate::emacs_core::pdump::dump_to_file(&eval, &good_path).expect("dump valid image");
    std::fs::copy(&good_path, &late_bad_path).expect("copy good dump for late failure");
    crate::emacs_core::pdump::mmap_image::corrupt_section_on_disk_for_test(
        &late_bad_path,
        crate::emacs_core::pdump::mmap_image::DumpSectionKind::ObjectStarts,
    )
    .expect("corrupt ObjectStarts section");
    let (mut eval, _late_err) =
        match crate::emacs_core::load::reload_written_bootstrap_dump(eval, &late_bad_path, proof) {
            Err(pair) => pair,
            Ok(_) => panic!("late-corrupt dump must fail reload"),
        };
    assert_eq!(
        current_tagged_heap_identity(),
        Some(orig_id),
        "late-failed reload must resume the original Context"
    );
    assert_eq!(
        eval.eval_str("reload-roundtrip-token")
            .expect("resumed eval after late failure stays usable"),
        Value::fixnum(77)
    );

    // Ok path: valid reload parks eval, loads the new Context, and drops parked eval.
    let mut loaded =
        match crate::emacs_core::load::reload_written_bootstrap_dump(eval, &good_path, proof) {
            Ok(loaded) => loaded,
            Err((_, err)) => panic!("valid dump must reload cleanly: {err}"),
        };
    assert_ne!(
        current_tagged_heap_identity(),
        Some(orig_id),
        "successful reload installs the reloaded heap"
    );
    assert_eq!(
        loaded
            .eval_str("reload-roundtrip-token")
            .expect("reloaded eval preserves binding"),
        Value::fixnum(77)
    );
}

#[test]
fn setup_thread_locals_refuses_over_foreign_heap_without_mutating_tls() {
    let mut ctx = Context::new(crate::test_utils::unit_runtime());
    let ctx_id = ctx.tagged_heap().tagged_heap_identity();
    let ctx_ccl = neovm_text::emacs_core::ccl::current_ccl_registry_handle();

    let foreign =
        neovm_value::tagged::gc::AliasableBox::new(neovm_value::tagged::gc::TaggedHeap::new());
    let foreign_id = foreign.tagged_heap_identity();
    // SAFETY: `foreign` and `ctx` both outlive `nest` with no live heap
    // references during install or drop.
    let nest = unsafe { NestedTaggedHeapInstall::install_bare(foreign.root()) };
    neovm_text::emacs_core::ccl::reset_ccl_registry();
    let foreign_ccl = neovm_text::emacs_core::ccl::current_ccl_registry_handle();
    assert!(!neovm_value::heap_registry::HeapRegistryHandle::registry_ptr_eq_for_test(
        &ctx_ccl,
        &foreign_ccl
    ));
    let _foreign_quit = neovm_value::quit::install_quit_requested_for_test(true);
    assert!(neovm_value::quit::quit_pending());

    for (label, call) in [
        ("setup_thread_locals", Context::setup_thread_locals as fn(&mut Context)),
        (
            "setup_owner_thread_locals",
            Context::setup_owner_thread_locals as fn(&mut Context),
        ),
        ("gc_collect_exact", Context::gc_collect_exact as fn(&mut Context)),
        (
            "gc_collect_from_current_roots",
            Context::gc_collect_from_current_roots as fn(&mut Context),
        ),
    ] {
        let refused = catch_unwind(AssertUnwindSafe(|| call(&mut ctx)));
        let text = panic_text(refused.expect_err(label));
        assert!(
            text.contains("dormant Context"),
            "{label} must refuse with dormant Context, got {text}"
        );
        assert_eq!(
            current_tagged_heap_identity(),
            Some(foreign_id),
            "{label} must leave the foreign heap installed"
        );
        assert!(
            neovm_value::heap_registry::HeapRegistryHandle::registry_ptr_eq_for_test(
                &neovm_text::emacs_core::ccl::current_ccl_registry_handle(),
                &foreign_ccl,
            ),
            "{label} must refuse before mutating CCL_REGISTRY TLS"
        );
        assert!(
            neovm_value::quit::quit_pending(),
            "{label} must refuse before mutating QUIT_REQUESTED TLS"
        );
    }

    drop(nest);
    assert_eq!(current_tagged_heap_identity(), Some(ctx_id));
    // Same-heap idempotent reinstall succeeds and restores ctx's TLS handles.
    ctx.setup_thread_locals();
    assert_eq!(current_tagged_heap_identity(), Some(ctx_id));
    assert!(
        neovm_value::heap_registry::HeapRegistryHandle::registry_ptr_eq_for_test(
            &neovm_text::emacs_core::ccl::current_ccl_registry_handle(),
            &ctx_ccl,
        ),
        "same-heap setup_thread_locals must restore ctx's CCL_REGISTRY"
    );
    assert!(
        !neovm_value::quit::quit_pending(),
        "same-heap setup_thread_locals must restore ctx's QUIT_REQUESTED"
    );

    // Empty-slot install (via park + resume) also succeeds.
    let mut ctx = ctx.park_for_test().resume_for_test().expect("resume on empty slot");
    assert_eq!(current_tagged_heap_identity(), Some(ctx_id));
    assert_eq!(ctx.eval_str("42").expect("resumed eval"), Value::fixnum(42));
}
