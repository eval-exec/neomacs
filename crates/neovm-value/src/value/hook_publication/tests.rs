use super::*;
use std::sync::{Arc, Barrier};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Descriptor(u64);
static FIRST: Descriptor = Descriptor(11);
static SAME_DATA_DIFFERENT_TABLE: Descriptor = Descriptor(11);

#[test]
fn hook_publication_same_descriptor_is_idempotent() {
    let payload = OnceLock::new();
    let identity = OnceLock::new();
    assert_eq!(
        publish(&payload, &identity, &FIRST),
        Ok(HookInstallStatus::Installed)
    );
    assert_eq!(
        publish(&payload, &identity, &FIRST),
        Ok(HookInstallStatus::AlreadyInstalled)
    );
    assert!(std::ptr::eq(*identity.get().unwrap(), &FIRST));
    assert_eq!(payload.get(), Some(&FIRST));
}

#[test]
fn hook_publication_equal_data_in_a_different_table_is_refused() {
    let payload = OnceLock::new();
    let identity = OnceLock::new();
    assert_eq!(
        publish(&payload, &identity, &FIRST),
        Ok(HookInstallStatus::Installed)
    );
    assert_eq!(
        publish(&payload, &identity, &SAME_DATA_DIFFERENT_TABLE),
        Err(HookInstallError::DifferentTable)
    );
    assert!(std::ptr::eq(*identity.get().unwrap(), &FIRST));
    assert_eq!(payload.get(), Some(&FIRST));
}

#[test]
fn hook_publication_concurrent_same_table_returns_only_after_payload_exists() {
    let slots = Arc::new((OnceLock::new(), OnceLock::new(), Barrier::new(2)));
    let threads: Vec<_> = (0..2)
        .map(|_| {
            let slots = Arc::clone(&slots);
            std::thread::spawn(move || {
                slots.2.wait();
                let result = publish(&slots.0, &slots.1, &FIRST);
                assert_eq!(slots.0.get(), Some(&FIRST));
                result
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Ok(HookInstallStatus::Installed))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|r| **r == Ok(HookInstallStatus::AlreadyInstalled))
            .count(),
        1
    );
}

#[test]
fn hook_publication_concurrent_different_tables_preserve_the_winning_descriptor() {
    let slots = Arc::new((OnceLock::new(), OnceLock::new(), Barrier::new(2)));
    let threads: Vec<_> = [&FIRST, &SAME_DATA_DIFFERENT_TABLE]
        .into_iter()
        .map(|table| {
            let slots = Arc::clone(&slots);
            std::thread::spawn(move || {
                slots.2.wait();
                (table, publish(&slots.0, &slots.1, table))
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    assert_eq!(
        results
            .iter()
            .filter(|(_, r)| *r == Ok(HookInstallStatus::Installed))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|(_, r)| *r == Err(HookInstallError::DifferentTable))
            .count(),
        1
    );
    let winner = results
        .iter()
        .find_map(|(table, result)| (*result == Ok(HookInstallStatus::Installed)).then_some(*table))
        .unwrap();
    assert!(std::ptr::eq(*slots.1.get().unwrap(), winner));
    assert_eq!(slots.0.get(), Some(winner));
}

#[test]
fn select_published_identity_answers_with_the_resident_descriptor() {
    let identity = OnceLock::new();
    assert!(std::ptr::eq(
        select_published_identity(&identity, &FIRST).unwrap(),
        &FIRST
    ));
    // The same descriptor is idempotent at the identity layer.
    assert!(std::ptr::eq(
        select_published_identity(&identity, &FIRST).unwrap(),
        &FIRST
    ));
    assert_eq!(
        select_published_identity(&identity, &SAME_DATA_DIFFERENT_TABLE),
        Err(HookInstallError::DifferentTable)
    );
    // The refusal never replaces the resident descriptor.
    assert!(std::ptr::eq(*identity.get().unwrap(), &FIRST));
}

/// Two statically distinct tables, sixteen threads, one barrier: whichever
/// descriptor initializes the identity cell is the only one any thread may
/// see accepted.  Every other offer must be the typed refusal, and no rejected
/// descriptor may become resident (the old `OnceLock::set` shape could not
/// answer this: its `Err` payload is the caller's own table).
#[test]
fn hook_publication_different_table_race_has_one_resident_descriptor() {
    const THREADS: usize = 16;
    static TABLES: [Descriptor; 2] = [Descriptor(21), Descriptor(22)];
    let slots = Arc::new((OnceLock::new(), Barrier::new(THREADS)));
    let threads: Vec<_> = (0..THREADS)
        .map(|index| {
            let slots = Arc::clone(&slots);
            std::thread::spawn(move || {
                let offered = &TABLES[index % TABLES.len()];
                slots.1.wait();
                (offered, select_published_identity(&slots.0, offered))
            })
        })
        .collect();
    let results: Vec<_> = threads
        .into_iter()
        .map(|thread| thread.join().unwrap())
        .collect();
    let resident = *slots.0.get().expect("one offer must initialize the cell");
    assert!(
        TABLES.iter().any(|table| std::ptr::eq(table, resident)),
        "the resident descriptor must be one of the offered statics"
    );
    let mut accepted = 0;
    let mut refused = 0;
    for (offered, result) in &results {
        match result {
            Ok(selected) => {
                assert!(std::ptr::eq(*selected, resident));
                assert!(std::ptr::eq(*offered, resident));
                accepted += 1;
            }
            Err(HookInstallError::DifferentTable) => {
                assert!(!std::ptr::eq(*offered, resident));
                refused += 1;
            }
        }
    }
    // Half the threads offered each table, so exactly the resident table's
    // half is accepted and the other half is refused -- race order cannot
    // change either count.
    assert_eq!(accepted, THREADS / TABLES.len());
    assert_eq!(refused, THREADS / TABLES.len());
    assert_eq!(accepted + refused, THREADS);
}
