//! Selected registry publication and source-lifetime cleanup. No Lisp state.

use super::*;

#[test]
fn opt_array_registry_publishes_one_table_across_mutators_and_source_clones() {
    let source = Runtime::new();
    let mut workers = Vec::new();
    for _ in 0..8 {
        let source = source.clone();
        workers.push(std::thread::spawn(move || {
            source.array_sites_for(&[Op::Aref])
        }));
    }
    let table = source.array_sites_for(&[Op::Aref]);
    for worker in workers {
        assert!(Arc::ptr_eq(
            &table,
            &worker.join().expect("registry worker")
        ));
    }
    assert!(Arc::ptr_eq(
        &table,
        &source.array_sites().expect("published table")
    ));
    table
        .site_at(0)
        .unwrap()
        .observe(ObservedArrayKind::PlainVector);
    assert_eq!(
        source.array_kind_snapshot(1)[0].plain(),
        Some(PlainArrayKind::Vector)
    );
}

#[test]
fn opt_array_registry_sweeps_dead_sources_and_retains_owned_table_views() {
    let source = Runtime::new();
    let table = source.array_sites_for(&[Op::Aref]);
    let id = source.compiled_id().expect("selected source identity");
    let clone = source.clone();
    drop(source);
    let next_source = Runtime::new();
    next_source.array_sites_for(&[Op::Aref]);
    assert!(
        ARRAY_REGISTRY
            .get()
            .unwrap()
            .read()
            .unwrap()
            .contains_key(&id)
    );
    drop(clone);
    next_source.array_sites_for(&[Op::Aref]);
    assert!(
        !ARRAY_REGISTRY
            .get()
            .unwrap()
            .read()
            .unwrap()
            .contains_key(&id)
    );
    // A copied Arc view remains valid even after the registry releases it.
    table
        .site_at(0)
        .unwrap()
        .observe(ObservedArrayKind::PlainRecord);
    assert_eq!(table.snapshot(1)[0].plain(), Some(PlainArrayKind::Record));
}
