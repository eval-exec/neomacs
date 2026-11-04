use super::*;

#[test]
fn repeated_emissions_have_distinct_ids_and_retained_counter_addresses() {
    let first = register_site_enabled(0, None);
    let second = register_site_enabled(0, None);
    assert_ne!(first.id, second.id);
    assert_ne!(
        core::ptr::from_ref(&first.attempts),
        core::ptr::from_ref(&second.attempts)
    );
    let retained = Arc::downgrade(&first);
    drop(first);
    assert!(
        retained.upgrade().is_some(),
        "registry retains retired sites"
    );
}

#[test]
fn reports_include_sites_with_no_hits_or_executions() {
    let site = register_site_enabled(3, None);
    assert!(site.render().contains("site=3"));
    assert!(site.render().contains("attempts=0,hits=0,misses=0"));
    site.attempts.fetch_add(7, Ordering::Relaxed);
    assert!(site.render().contains("attempts=7,hits=0,misses=7"));
}

#[test]
fn concurrent_mutators_preserve_attempt_and_hit_counts() {
    let site = register_site_enabled(1, None);
    std::thread::scope(|scope| {
        for _ in 0..16 {
            let site = Arc::clone(&site);
            scope.spawn(move || {
                for i in 0..128 {
                    site.attempts.fetch_add(1, Ordering::Relaxed);
                    if i % 2 == 0 {
                        site.hits.fetch_add(1, Ordering::Relaxed);
                    }
                }
            });
        }
    });
    assert_eq!(site.attempts.load(Ordering::Relaxed), 2048);
    assert_eq!(site.hits.load(Ordering::Relaxed), 1024);
    assert!(
        site.render()
            .contains("attempts=2048,hits=1024,misses=1024")
    );
}

#[test]
fn absent_profiles_emit_no_clif_instructions() {
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Module, default_libcall_names};

    let module =
        JITModule::new(JITBuilder::new(default_libcall_names()).expect("host JIT builder"));
    let config = module.target_config();
    let mut function = cranelift_codegen::ir::Function::new();
    let mut context = cranelift_frontend::FunctionBuilderContext::new();
    let mut fb = FunctionBuilder::new(&mut function, &mut context);
    let entry = fb.create_block();
    fb.switch_to_block(entry);
    fb.seal_block(entry);
    let before = fb.func.dfg.num_insts();
    emit_attempt(&mut fb, types::I64, None);
    emit_hit(&mut fb, types::I64, None);
    assert_eq!(fb.func.dfg.num_insts(), before);
    fb.ins().return_(&[]);
    fb.finalize(config);
}

#[test]
fn generated_atomic_increments_execute_and_preserve_concurrent_counts() {
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{Linkage, Module, default_libcall_names};

    let site = register_site_enabled(2, None);
    let mut module =
        JITModule::new(JITBuilder::new(default_libcall_names()).expect("host JIT builder"));
    let config = module.target_config();
    let mut build = |name: &str, hit: bool| {
        let mut context = module.make_context();
        // A zero-argument, void-returning function with the host's C ABI.
        context.func.signature = module.make_signature();
        let id = module
            .declare_function(name, Linkage::Local, &context.func.signature)
            .expect("declare diagnostic entry");
        let mut fb_context = cranelift_frontend::FunctionBuilderContext::new();
        {
            let mut fb = FunctionBuilder::new(&mut context.func, &mut fb_context);
            let entry = fb.create_block();
            fb.switch_to_block(entry);
            fb.seal_block(entry);
            emit_attempt(&mut fb, config.pointer_type(), Some(&site));
            if hit {
                emit_hit(&mut fb, config.pointer_type(), Some(&site));
            }
            fb.ins().return_(&[]);
            fb.finalize(config);
        }
        module
            .define_function(id, &mut context)
            .expect("compile diagnostic entry");
        id
    };
    let miss = build("direct_profile_test_miss", false);
    let hit = build("direct_profile_test_hit", true);
    module
        .finalize_definitions()
        .expect("finalize diagnostic entries");
    // SAFETY: both functions were compiled with the host C ABI and the
    // declared () -> () signature. The module remains live until every
    // scoped caller has joined; only retained AtomicU64 cells are accessed.
    let miss: unsafe extern "C" fn() =
        unsafe { core::mem::transmute(module.get_finalized_function(miss)) };
    let hit: unsafe extern "C" fn() =
        unsafe { core::mem::transmute(module.get_finalized_function(hit)) };
    std::thread::scope(|scope| {
        for _ in 0..16 {
            scope.spawn(move || {
                for _ in 0..128 {
                    // SAFETY: immutable live generated code; all shared
                    // writes are emitted atomic RMWs, as explained above.
                    unsafe {
                        miss();
                        hit();
                    }
                }
            });
        }
    });
    assert_eq!(site.attempts.load(Ordering::Relaxed), 4096);
    assert_eq!(site.hits.load(Ordering::Relaxed), 2048);
    assert!(
        site.render()
            .contains("attempts=4096,hits=2048,misses=2048")
    );
    // SAFETY: every caller has joined and no generated entry is used again.
    unsafe { module.free_memory() };
}
