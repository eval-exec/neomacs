use super::*;

fn saved_depth() -> usize {
    11
}
fn other_saved_depth() -> usize {
    22
}
fn push(_: Value) {}
fn restore(_: usize) {}
static CANONICAL: ScratchRootHooks = ScratchRootHooks::new(saved_depth, push, restore);
static OTHER: ScratchRootHooks = ScratchRootHooks::new(other_saved_depth, push, restore);

#[test]
fn scratch_hook_publication_refuses_replacement_and_retains_original_callbacks() {
    let payload = OnceLock::new();
    let identity = OnceLock::new();
    assert_eq!(
        super::super::hook_publication::publish(&payload, &identity, &CANONICAL),
        Ok(HookInstallStatus::Installed)
    );
    assert_eq!(
        super::super::hook_publication::publish(&payload, &identity, &OTHER),
        Err(HookInstallError::DifferentTable)
    );
    assert_eq!((payload.get().unwrap().save)(), 11);
    assert!(std::ptr::eq(*identity.get().unwrap(), &CANONICAL));
}

#[test]
fn scratch_hook_publication_same_table_is_idempotent() {
    let payload = OnceLock::new();
    let identity = OnceLock::new();
    assert_eq!(
        super::super::hook_publication::publish(&payload, &identity, &CANONICAL),
        Ok(HookInstallStatus::Installed)
    );
    assert_eq!(
        super::super::hook_publication::publish(&payload, &identity, &CANONICAL),
        Ok(HookInstallStatus::AlreadyInstalled)
    );
    assert_eq!((payload.get().unwrap().save)(), 11);
}
