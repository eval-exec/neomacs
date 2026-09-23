use super::JIT_SHIM_TABLE;

#[test]
fn runtime_shims_link_without_dynamic_exports_and_preserve_host_fallback() {
    use cranelift_codegen::ir::Signature;
    use cranelift_jit::{JITBuilder, JITModule};
    use cranelift_module::{DataDescription, Linkage, Module, default_libcall_names};

    extern "C" fn host_fallback() {}
    let mut builder = JITBuilder::new(default_libcall_names()).unwrap();
    // Lookup callbacks run in reverse registration order. Reject a shim
    // reaching the host fallback: exported test-binary symbols must not
    // conceal a missing runtime registration.
    builder.symbol_lookup_fn(Box::new(|name| {
        assert!(!name.starts_with("neovm_jit_"), "unregistered shim: {name}");
        (name == "neomacs_test_host_fallback").then_some(host_fallback as *const () as *const u8)
    }));
    super::register_shims(&mut builder);
    let mut module = JITModule::new(builder);
    let mut symbols: Vec<_> = JIT_SHIM_TABLE
        .iter()
        .map(|(name, addr)| (*name, addr.0 as usize))
        .collect();
    symbols.push((
        "neomacs_test_host_fallback",
        host_fallback as *const () as usize,
    ));
    let word = std::mem::size_of::<usize>();
    let mut data = DataDescription::new();
    data.define_zeroinit(symbols.len() * word);
    let signature = Signature::new(module.target_config().default_call_conv);
    for (i, (name, _)) in symbols.iter().enumerate() {
        let id = module
            .declare_function(name, Linkage::Import, &signature)
            .unwrap();
        let reference = module.declare_func_in_data(id, &mut data);
        data.write_function_addr((i * word) as u32, reference);
    }
    let id = module
        .declare_data("runtime_shim_addresses", Linkage::Local, false, false)
        .unwrap();
    module.define_data(id, &data).unwrap();
    module.finalize_definitions().unwrap();
    let (ptr, size) = module.get_finalized_data(id);
    assert_eq!(size, symbols.len() * word);
    for (i, (name, expected)) in symbols.iter().enumerate() {
        // SAFETY: each relocated pointer lies inside the checked live data
        // allocation. Read addresses only; no shim is called with a dummy ABI.
        let actual = unsafe { ptr.add(i * word).cast::<usize>().read_unaligned() };
        assert_eq!(actual, *expected, "{name}");
    }
    // SAFETY: no compiled function ran and the data pointer is not used again.
    unsafe { module.free_memory() };
}

/// `shim_names.rs` (what an AOT `.so` may import; exported by the build
/// scripts) and [`JIT_SHIM_TABLE`] (what the JIT can resolve) must name
/// exactly the same shims, or a shim is exported that the JIT cannot
/// call — or callable by the JIT but silently unexported for AOT.
#[test]
fn the_shim_table_and_the_exported_name_list_are_the_same_set() {
    let names: std::collections::BTreeSet<&str> = crate::emacs_core::jit::aot::MIR_SHIM_NAMES
        .iter()
        .copied()
        .collect();
    let table: std::collections::BTreeSet<&str> = JIT_SHIM_TABLE.iter().map(|(n, _)| *n).collect();
    assert_eq!(
        table.len(),
        JIT_SHIM_TABLE.len(),
        "duplicate name in JIT_SHIM_TABLE"
    );
    let only_names: Vec<_> = names.difference(&table).collect();
    let only_table: Vec<_> = table.difference(&names).collect();
    assert!(
        only_names.is_empty() && only_table.is_empty(),
        "shim sets drifted: in shim_names.rs only {only_names:?}; in JIT_SHIM_TABLE only {only_table:?}"
    );
}
