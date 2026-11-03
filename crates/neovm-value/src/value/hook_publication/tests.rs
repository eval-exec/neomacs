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
