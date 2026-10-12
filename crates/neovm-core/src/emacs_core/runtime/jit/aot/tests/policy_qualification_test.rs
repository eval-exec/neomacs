//! Supplied manifest keys must not bypass the native emission policy.
use super::*;
use crate::tagged::collection_reads::{CompiledJournalMode, force_compiled_journal_for_test};

struct ResetPolicy;

impl Drop for ResetPolicy {
    fn drop(&mut self) {
        test_support::reset();
        force_compiled_journal_for_test(None);
    }
}

#[cfg(target_os = "linux")]
fn preload_for(
    ops: &[Op],
    constants: &[Value],
) -> (
    tempfile::TempDir,
    std::sync::Arc<super::super::compile::LoadedUnit>,
    u128,
) {
    let hash = leaf_content_hash(ops, constants, 1).expect("hashable");
    let leaf = LoadupLeaf {
        name: "policy-qualified-preload".into(),
        ops: Box::leak(ops.to_vec().into_boxed_slice()),
        constants: Box::leak(constants.to_vec().into_boxed_slice()),
        arity: 1,
    };
    let (obj, stats) = build_preload_object(&[leaf], None).expect("build preload");
    assert_eq!(stats.unique_emitted, 1);
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(PRELOAD_SO_NAME);
    link_object_to_so(&obj, &path).expect("link");
    // SAFETY: the library was linked from our validated native object; the
    // LoadedUnit retains its handle throughout every lookup and invocation.
    let lib = unsafe { libloading::Library::new(path) }.expect("dlopen");
    let unit = std::sync::Arc::new(super::super::compile::LoadedUnit::new(lib));
    assert!(unit_has_entry(&unit, hash));
    (dir, unit, hash)
}

#[cfg(target_os = "linux")]
fn assert_add_result(
    leaf: &super::super::compile::CompiledLeaf,
    ev: &mut crate::emacs_core::eval::Context,
) {
    let ctx = ev as *mut crate::emacs_core::eval::Context as *mut u8;
    match leaf.call(ctx, &[Value::make_int(10)]) {
        super::super::compile::NativeRun::Ok(bits) => {
            assert_eq!(bits, Value::make_int(1_234_577).bits());
        }
        other => panic!("qualified AOT leaf: {other:?}"),
    }
}

#[cfg(target_os = "linux")]
#[test]
fn matching_qualified_preloads_serve_under_each_journal_policy() {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let _reset = ResetPolicy;
    let ops = [Op::Constant(0), Op::Add, Op::Return];
    let constants = [Value::make_int(1_234_567)];
    let mut hashes = Vec::new();
    for mode in [
        CompiledJournalMode::Off,
        CompiledJournalMode::Observed,
        CompiledJournalMode::Eager,
    ] {
        force_compiled_journal_for_test(Some(mode));
        let (_dir, unit, hash) = preload_for(&ops, &constants);
        hashes.push(hash);
        test_support::set_forced_enabled(true);
        test_support::inject_preload(unit);
        let leaf = try_load_leaf(&ops, &constants, 1, Some(hash), None)
            .expect("matching qualified member must still serve from preload");
        assert_add_result(&leaf, &mut ev);
        // A foreign supplied key must select the current qualified member,
        // rather than discarding preload eligibility or trusting that key.
        let leaf = try_load_leaf(&ops, &constants, 1, Some(hash ^ 1), None)
            .expect("corrected key retains preload membership lookup");
        assert_add_result(&leaf, &mut ev);
        test_support::reset();
    }
    assert_ne!(hashes[0], hashes[1]);
    assert_ne!(hashes[1], hashes[2]);
    assert_ne!(hashes[0], hashes[2]);
}

#[cfg(target_os = "linux")]
#[test]
fn observed_manifest_key_cannot_serve_foreign_policy_body() {
    let mut ev = crate::emacs_core::eval::Context::new_minimal_vm_harness();
    let _reset = ResetPolicy;
    let ops = [Op::Constant(0), Op::Add, Op::Return];
    let constants = [Value::make_int(1_234_567)];
    force_compiled_journal_for_test(Some(CompiledJournalMode::Observed));
    let (_observed_dir, observed_unit, observed_hash) = preload_for(&ops, &constants);
    test_support::set_forced_enabled(true);
    test_support::inject_preload(observed_unit.clone());
    for mode in [CompiledJournalMode::Off, CompiledJournalMode::Eager] {
        force_compiled_journal_for_test(Some(mode));
        let current_hash = leaf_content_hash(&ops, &constants, 1).unwrap();
        assert_ne!(current_hash, observed_hash);
        assert!(!unit_has_entry(&observed_unit, current_hash));
        assert!(
            try_load_leaf(&ops, &constants, 1, Some(observed_hash), None).is_none(),
            "a supplied Observed key cannot serve its body under {mode:?}"
        );
        // A correctly qualified per-leaf unit remains a valid fallback when
        // the supplied manifest's preload has only the incompatible body.
        let (_current_dir, current_unit, built_hash) = preload_for(&ops, &constants);
        assert_eq!(current_hash, built_hash);
        test_support::inject_unit(current_hash, current_unit);
        let leaf = try_load_leaf(&ops, &constants, 1, Some(observed_hash), None)
            .expect("fall back to current qualified per-leaf unit");
        assert_add_result(&leaf, &mut ev);
        test_support::reset();
        test_support::set_forced_enabled(true);
        test_support::inject_preload(observed_unit.clone());
    }
}

/// Compatibility fixture for a pointer-free body in the existing content-key
/// format: Off/Eager retain their old bytes; Observed intentionally does not.
#[test]
fn admission_v2_changes_only_the_observed_content_key() {
    use sha2::{Digest, Sha256};
    let _reset = ResetPolicy;
    for (mode, policy) in [
        (CompiledJournalMode::Off, b"collection-off".as_slice()),
        (
            CompiledJournalMode::Observed,
            b"collection-observed".as_slice(),
        ),
        (CompiledJournalMode::Eager, b"collection-eager".as_slice()),
    ] {
        let mut legacy = Sha256::new();
        legacy.update(ABI_TAG.to_le_bytes());
        legacy.update(policy);
        legacy.update(1_u64.to_le_bytes()); // arity
        legacy.update(2_u64.to_le_bytes()); // two operations
        legacy.update(4_u64.to_le_bytes());
        legacy.update(b"Add1");
        legacy.update(6_u64.to_le_bytes());
        legacy.update(b"Return");
        legacy.update(0_u64.to_le_bytes()); // no constants
        let digest = legacy.finalize();
        let legacy_hash = u128::from_le_bytes(digest[..16].try_into().unwrap());
        force_compiled_journal_for_test(Some(mode));
        let current = leaf_content_hash(&[Op::Add1, Op::Return], &[], 1).unwrap();
        if mode == CompiledJournalMode::Observed {
            assert_ne!(current, legacy_hash);
        } else {
            assert_eq!(current, legacy_hash, "unchanged {mode:?} key bytes");
        }
    }
}
