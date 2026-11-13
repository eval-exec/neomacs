use super::*;

#[test]
fn ascii_reader_name_validation_rejects_non_ascii_spellings() {
    assert!(ValidatedAsciiName::try_from("reader-ascii-name").is_ok());
    assert!(ValidatedAsciiName::try_from("reader-name-λ").is_err());
    assert!(ValidatedAsciiName::try_from("reader-name-\u{80}").is_err());
}

#[test]
fn ascii_reader_hit_skips_factory_and_registry_lock() {
    use std::sync::mpsc;
    use std::time::Duration;

    let name = "gdj-ascii-reader-warm-hit-no-factory";
    let id = intern(name);
    let canonical = global_symbol_registry().read().resolve(id);
    let (send, receive) = mpsc::channel();
    let registry = global_symbol_registry().write();
    let epoch = symbol_registry_epoch_value();
    let worker = std::thread::spawn(move || {
        // Install an already validated immutable spelling while the parent
        // owns the registry. No Lisp state crosses this thread boundary.
        ensure_thread_local_cache_epoch_current();
        thread_local_record_interned_str(canonical, id);
        reset_intern_calls();
        let answer = intern_ascii_reader_name(
            ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
            || panic!("a warm reader hit must not construct a name"),
        );
        send.send((answer, intern_calls(), intern_call_names()))
            .expect("parent remains alive");
    });
    let answer = receive.recv_timeout(Duration::from_secs(2));
    let after = symbol_registry_epoch_value();
    // Release the registry and join on every result before assertions. A
    // regressed lock acquisition may finish after release rather than hang.
    drop(registry);
    let joined = worker.join();
    joined.expect("reader lookup worker completed");
    assert_eq!(after, epoch);
    let (answer, calls, names) = answer.expect("cached hit must finish while registry is locked");
    assert_eq!(answer, id);
    assert_eq!(calls, 1, "the shared interner entry must remain observable");
    assert_eq!(names, vec![name.to_owned()]);
}

#[test]
fn ascii_reader_hit_uncached_canonical_does_not_construct_a_name() {
    let name = "gdj-ascii-reader-uncached-existing-no-factory";
    let id = global_symbol_registry()
        .write()
        .intern_reader_name(LispString::from_utf8(name), ReaderSymbolKind::Canonical);
    ensure_thread_local_cache_epoch_current();
    // This test removes only its own positive entry, not any shared epoch.
    INTERN_STR_CACHE.with(|cache| cache.borrow_mut().remove(name));
    assert_eq!(thread_local_interned_str(name), None);
    reset_intern_calls();
    let found = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
        || panic!("an existing canonical name must not invoke the factory"),
    );
    assert_eq!(found, id);
    assert_eq!(intern_calls(), 1);
    assert_eq!(intern_call_names(), vec![name.to_owned()]);
    assert!(
        global_symbol_registry()
            .read()
            .resolve_name_template(id)
            .is_multibyte()
    );
}

#[test]
fn ascii_reader_name_keeps_the_first_multibyte_or_unibyte() {
    // GNU lread.c:4213-4253 retains the name only on creation. Neither later
    // source encoding may replace an existing canonical symbol's name.
    let mb_name = "gdj-ascii-reader-first-mb-retained";
    let mb_factories = Cell::new(0);
    let mb = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(mb_name).expect("ASCII test spelling"),
        || {
            mb_factories.set(mb_factories.get() + 1);
            LispString::from_utf8(mb_name)
        },
    );
    let mb_again = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(mb_name).expect("ASCII test spelling"),
        || {
            mb_factories.set(mb_factories.get() + 1);
            LispString::from_unibyte_slice(mb_name.as_bytes())
        },
    );
    assert_eq!(mb, mb_again);
    assert_eq!(mb_factories.get(), 1);

    let ub_name = "gdj-ascii-reader-first-ub-retained";
    let ub_factories = Cell::new(0);
    let ub = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(ub_name).expect("ASCII test spelling"),
        || {
            ub_factories.set(ub_factories.get() + 1);
            LispString::from_unibyte_slice(ub_name.as_bytes())
        },
    );
    let ub_again = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(ub_name).expect("ASCII test spelling"),
        || {
            ub_factories.set(ub_factories.get() + 1);
            LispString::from_utf8(ub_name)
        },
    );
    assert_eq!(ub, ub_again);
    assert_eq!(ub_factories.get(), 1);

    let registry = global_symbol_registry().read();
    let first_mb = registry.resolve_name_template(mb);
    let first_ub = registry.resolve_name_template(ub);
    assert!(first_mb.is_multibyte());
    assert!(!first_ub.is_multibyte());
    assert_eq!(first_mb.as_bytes(), mb_name.as_bytes());
    assert_eq!(first_ub.as_bytes(), ub_name.as_bytes());
    assert_eq!(first_mb.schars(), mb_name.len());
    assert_eq!(first_ub.schars(), ub_name.len());
    assert!(!first_mb.has_intervals());
    assert!(!first_ub.has_intervals());
}

#[test]
fn ascii_reader_name_invalidation_recreates_with_the_new_first_encoding() {
    let name = "gdj-ascii-reader-unintern-recreate-encoding";
    let factories = Cell::new(0);
    reset_intern_calls();
    let old = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
        || {
            factories.set(factories.get() + 1);
            LispString::from_utf8(name)
        },
    );
    assert_eq!(
        intern_ascii_reader_name(
            ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
            || panic!("cached canonical hit must not invoke the factory"),
        ),
        old,
    );
    assert!(unintern_canonical_id(old));
    let new = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
        || {
            factories.set(factories.get() + 1);
            LispString::from_unibyte_slice(name.as_bytes())
        },
    );
    assert_ne!(new, old);
    assert_eq!(factories.get(), 2);
    assert_eq!(lookup_interned(name), Some(new));
    assert_eq!(intern_calls(), 3, "UB and MB entries must both be counted");
    assert_eq!(intern_call_names(), vec![name.to_owned(); 3]);
    let registry = global_symbol_registry().read();
    assert!(registry.resolve_name_template(old).is_multibyte());
    assert!(!registry.resolve_name_template(new).is_multibyte());
}

#[test]
fn ascii_reader_factory_unwind_releases_the_unpoisoned_registry() {
    use std::panic::{AssertUnwindSafe, catch_unwind};

    let name = "gdj-ascii-reader-factory-unwind-retry";
    let failure = catch_unwind(AssertUnwindSafe(|| {
        intern_ascii_reader_name(
            ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
            || panic!("intentional factory unwind before canonical publication"),
        )
    }));
    assert!(failure.is_err());
    // A factory failure occurs before creator mutation. The parking_lot
    // guard is released during unwind; it has no std-lock poisoning state.
    let registry = global_symbol_registry()
        .try_write()
        .expect("factory unwind must release the registry guard");
    assert_eq!(registry.lookup(name), None);
    drop(registry);
    let id = intern_ascii_reader_name(
        ValidatedAsciiName::try_from(name).expect("ASCII test spelling"),
        || LispString::from_utf8(name),
    );
    assert_eq!(lookup_interned(name), Some(id));
    assert!(
        global_symbol_registry()
            .read()
            .resolve_name_template(id)
            .is_multibyte()
    );
}
