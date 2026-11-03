use super::*;
use crate::heap_types::GnuDeclaredCharacterCount;

static_assertions::assert_impl_all!(SymId: Send, Sync, Copy);

fn supplied(name: &str, chars: usize) -> LispString {
    LispString::from_emacs_bytes_with_declared_chars(
        name.as_bytes().to_vec(),
        GnuDeclaredCharacterCount::try_from(chars as i128).expect("test name count"),
    )
}

#[test]
fn successful_native_lookup_caches_the_immutable_canonical_spelling() {
    let query = String::from("gdj-native-lookup-cache-cold-hit-λ");
    let (id, canonical) = {
        let mut registry = global_symbol_registry().write();
        let id = registry.intern(&query);
        (id, registry.resolve(id))
    };
    ensure_thread_local_cache_epoch_current();
    assert_eq!(thread_local_interned_str(&query), None);
    assert_eq!(lookup_interned(&query), Some(id));
    INTERN_STR_CACHE.with(|cache| {
        let cache = cache.borrow();
        let (key, cached_id) = cache.get_key_value(query.as_str()).expect("positive cache");
        assert_eq!(*cached_id, id);
        assert_eq!(key.as_ptr(), canonical.as_ptr());
    });
    drop(query);
    assert_eq!(lookup_interned(canonical), Some(id));
}

#[test]
fn missing_native_lookup_never_interns_or_caches_a_name() {
    let name = "gdj-native-lookup-cache-permanent-miss-λ";
    assert_eq!(global_symbol_registry().read().names.lookup(name), None);
    assert_eq!(lookup_interned(name), None);
    assert_eq!(lookup_interned(name), None);
    assert_eq!(thread_local_interned_str(name), None);
    assert_eq!(global_symbol_registry().read().names.lookup(name), None);
}

#[test]
fn cached_native_lookup_does_not_acquire_the_registry_lock() {
    use std::sync::mpsc;
    use std::time::Duration;

    let cached: Vec<_> = [
        "",
        "gdj-native-lookup-cache-lock-free-hit",
        "gdj-native-lookup-cache-lock-free-hit-λ",
        "gdj-native-lookup-cache-lock-free-hit\0tail",
    ]
    .into_iter()
    .map(|name| {
        let id = intern(name);
        let canonical = global_symbol_registry().read().resolve(id);
        (id, canonical)
    })
    .collect();
    let expected: Vec<_> = cached.iter().map(|(id, _)| Some(*id)).collect();
    let (send, receive) = mpsc::channel();
    let registry = global_symbol_registry().write();
    let epoch = symbol_registry_epoch_value();
    let worker = std::thread::spawn(move || {
        // The parent owns the registry lock throughout admission. Install an
        // already validated immutable atom in this worker's existing cache;
        // no Lisp state or heap borrow crosses the thread boundary.
        ensure_thread_local_cache_epoch_current();
        for &(id, canonical) in &cached {
            thread_local_record_interned_str(canonical, id);
        }
        let answers: Vec<_> = cached
            .iter()
            .map(|(_, canonical)| lookup_interned(canonical))
            .collect();
        send.send(answers).expect("test receiver remains alive");
    });
    let answer = receive.recv_timeout(Duration::from_secs(2));
    // With epoch publication inside the write guard, unrelated uninterns
    // cannot invalidate the primed hit while this guard remains held.
    let after = symbol_registry_epoch_value();
    // Always release the lock and join before asserting. The old lookup
    // blocks, but finishes after this drop instead of hanging the test.
    drop(registry);
    worker.join().expect("lookup worker completed");
    assert_eq!(after, epoch);
    assert_eq!(
        answer.expect("cache hit must finish while lock is held"),
        expected
    );
}

#[test]
fn native_lookup_revalidates_after_unintern_and_reintern() {
    let name = "gdj-native-lookup-cache-reintern";
    let old = intern(name);
    assert_eq!(lookup_interned(name), Some(old));
    assert!(unintern_canonical_id(old));
    assert_eq!(lookup_interned(name), None);
    let new = intern(name);
    assert_ne!(new, old);
    assert_eq!(lookup_interned(name), Some(new));
}

#[test]
fn completed_cross_thread_unintern_invalidates_native_lookup() {
    let name = "gdj-native-lookup-cache-cross-thread-reintern";
    let old = intern(name);
    assert_eq!(lookup_interned(name), Some(old));
    assert!(
        std::thread::spawn(move || unintern_canonical_id(old))
            .join()
            .unwrap()
    );
    assert_eq!(lookup_interned(name), None);
    let new = intern(name);
    assert_ne!(new, old);
    assert_eq!(lookup_interned(name), Some(new));
}

#[test]
fn native_cache_cannot_alias_supplied_counts_for_identical_bytes() {
    for (name, count) in [
        ("gdj-native-lookup-cache-zero-count", 0),
        (
            "gdj-native-lookup-cache-byte-count-λ",
            "gdj-native-lookup-cache-byte-count-λ".len(),
        ),
    ] {
        let declared = supplied(name, count);
        let unusual = intern_lisp_string(&declared);
        assert_eq!(lookup_interned(name), None);
        assert_eq!(thread_local_interned_str(name), None);
        assert_eq!(lookup_interned_lisp_string(&declared), Some(unusual));
        let native = intern(name);
        assert_ne!(native, unusual);
        assert_eq!(lookup_interned(name), Some(native));
        assert_eq!(lookup_interned_lisp_string(&declared), Some(unusual));
    }
}

#[test]
fn supplied_proof_with_equivalent_count_can_enter_native_cache() {
    let name = "gdj-native-lookup-cache-equivalent-count-λ";
    let declared = supplied(name, name.chars().count());
    let id = intern_lisp_string(&declared);
    assert_eq!(lookup_interned(name), Some(id));
    assert_eq!(thread_local_interned_str(name), Some(id));
    assert_eq!(lookup_interned_lisp_string(&declared), Some(id));
}

#[test]
fn native_canonical_cache_does_not_grant_local_obarray_membership() {
    let name = "gdj-native-lookup-cache-local-membership";
    let mut first = crate::symbol::Obarray::new();
    let mut second = crate::symbol::Obarray::new();
    first.intern(name);
    assert!(lookup_interned(name).is_some());
    assert!(first.intern_soft(name).is_some());
    assert!(second.intern_soft(name).is_none());
    second.intern(name);
    assert!(second.intern_soft(name).is_some());
}

#[test]
fn delayed_cache_admission_keeps_its_old_epoch_until_validation() {
    use std::sync::mpsc;

    let name = "gdj-native-lookup-cache-delayed-admission";
    let old = intern(name);
    let (snapshot_send, snapshot_receive) = mpsc::channel();
    let (resume_send, resume_receive) = mpsc::channel();
    let worker = std::thread::spawn(move || {
        ensure_thread_local_cache_epoch_current();
        let observed_epoch = THREAD_CACHE_EPOCH.get();
        let registry = global_symbol_registry().read();
        let observed = registry.lookup(name).expect("canonical before unintern");
        let canonical = registry.resolve(observed);
        drop(registry);
        snapshot_send.send(observed).expect("parent remains alive");
        resume_receive.recv().expect("parent releases admission");
        // Model a successful query paused immediately after dropping its read
        // guard. Admission must not adopt a newer epoch for the old result.
        thread_local_record_interned_str(canonical, observed);
        assert_eq!(THREAD_CACHE_EPOCH.get(), observed_epoch);
        assert_eq!(thread_local_interned_str(name), Some(observed));
        assert_eq!(lookup_interned(name), None);
        assert_eq!(thread_local_interned_str(name), None);
    });
    assert_eq!(snapshot_receive.recv().expect("worker snapshot"), old);
    let removed = unintern_canonical_id(old);
    resume_send.send(()).expect("worker remains alive");
    worker.join().expect("admission worker completed");
    assert!(removed);
}
