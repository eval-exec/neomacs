//! Buffer creation seeds every slot from values materialized once per
//! storage, as GNU `reset_buffer` and `reset_buffer_local_variables`
//! fill a new buffer (buffer.c:1088, 1136).

use super::*;

fn slot(manager: &BufferManager, id: BufferId, slot: BufferSlot) -> Value {
    manager.get(id).expect("live buffer").slots[slot.index()]
}

#[test]
fn create_buffer_never_looks_up_the_process_cwd() {
    crate::test_utils::init_test_tracing();
    let mut manager = BufferManager::new();
    SLOT_DEFAULT_CWD_LOOKUPS.set(0);
    for n in 0..1_000 {
        manager.create_buffer(&format!(" *slot-seed-{n}*"));
    }
    assert_eq!(SLOT_DEFAULT_CWD_LOOKUPS.get(), 0);
}

#[test]
fn create_buffer_default_directory_follows_the_current_buffer() {
    crate::test_utils::init_test_tracing();
    let mut manager = BufferManager::new();
    let first = manager.current_buffer_id().expect("*scratch*");
    manager
        .get_mut(first)
        .unwrap()
        .set_buffer_local("default-directory", Value::string("/slot-seed-a/"));
    let dir = |manager: &BufferManager, id| {
        manager
            .get(id)
            .unwrap()
            .buffer_local_value("default-directory")
            .and_then(|value| value.as_utf8_str().map(str::to_owned))
    };
    let second = manager.create_buffer("*slot-seed-a*");
    assert_eq!(dir(&manager, second).as_deref(), Some("/slot-seed-a/"));
    // A mid-way `cd' only rebinds the current buffer's `default-directory'.
    assert!(manager.switch_current(second));
    manager
        .current_buffer_mut()
        .unwrap()
        .set_buffer_local("default-directory", Value::string("/slot-seed-b/"));
    let third = manager.create_buffer("*slot-seed-b*");
    assert_eq!(dir(&manager, third).as_deref(), Some("/slot-seed-b/"));
}

#[test]
fn create_buffer_shares_the_mode_name_buffer_slot_seed() {
    crate::test_utils::init_test_tracing();
    let mut manager = BufferManager::new();
    let a = manager.create_buffer("*slot-seed-mode-a*");
    let b = manager.create_buffer("*slot-seed-mode-b*");
    let (name_a, name_b) = (
        slot(&manager, a, BUFFER_SLOT_MODE_NAME),
        slot(&manager, b, BUFFER_SLOT_MODE_NAME),
    );
    assert_eq!(name_a.as_utf8_str(), Some("Fundamental"));
    // GNU hands every fresh buffer the one `QSFundamental' string
    // (buffer.c:1146, 4911), so an `aset' on it is seen by both buffers.
    assert!(eq_value(&name_a, &name_b));
}

#[test]
fn create_buffer_slot_seeds_are_not_the_default_values() {
    crate::test_utils::init_test_tracing();
    let mut manager = BufferManager::new();
    let fresh = manager.create_buffer("*slot-seed-default*");
    // GNU sets `buffer_defaults.mode_name' (buffer.c:4818) before it builds
    // `QSFundamental' (4911), so `(eq mode-name (default-value 'mode-name))'
    // is nil in a new buffer, and an `aset' on the default value does not
    // reach later buffers.
    for slot_id in [BUFFER_SLOT_MODE_NAME, BUFFER_SLOT_DEFAULT_DIRECTORY] {
        let local = slot(&manager, fresh, slot_id);
        let default = manager.buffer_defaults_mut()[slot_id.index()];
        assert_eq!(local.as_utf8_str(), default.as_utf8_str());
        assert!(!eq_value(&local, &default));
    }
}

#[test]
fn buffer_slot_seeds_survive_a_gc_before_create_buffer() {
    crate::test_utils::init_test_tracing();
    let mut manager = BufferManager::new();
    let scratch = manager.current_buffer_id().expect("*scratch*");
    let seed = manager.slot_seeds[BUFFER_SLOT_MODE_NAME.index()];
    // Leave the seed array as the string's only holder.
    manager.buffer_defaults_mut()[BUFFER_SLOT_MODE_NAME.index()] = Value::NIL;
    manager.get_mut(scratch).unwrap().slots[BUFFER_SLOT_MODE_NAME.index()] = Value::NIL;
    let mut roots = Vec::new();
    manager.trace_roots(&mut roots);
    assert!(roots.iter().any(|root| eq_value(root, &seed)));
    neovm_value::tagged::gc::with_tagged_heap(|heap| {
        heap.collect_rooted(roots.into_iter());
        assert!(heap.owns_heap_value_for_test(seed));
    });
    let fresh = manager.create_buffer("*slot-seed-after-gc*");
    let mode_name = slot(&manager, fresh, BUFFER_SLOT_MODE_NAME);
    assert!(eq_value(&mode_name, &seed));
    assert_eq!(mode_name.as_utf8_str(), Some("Fundamental"));
}
