use super::*;

fn key(_: Value, _: usize, _: &mut Vec<usize>, _: bool) -> HashKey {
    HashKey::Int(11)
}
fn equal(
    _: &Value,
    _: &Value,
    _: usize,
    _: &mut Option<HashSet<EqualSeenPair>>,
    _: bool,
    _: EqualKind,
) -> bool {
    true
}
fn try_equal(
    _: &Value,
    _: &Value,
    _: usize,
    _: &mut Option<HashSet<EqualSeenPair>>,
    _: bool,
    _: EqualKind,
) -> Result<bool, Flow> {
    Ok(true)
}
fn hash(_: &Value, _: usize, _: crate::tagged::SymbolMode) -> Option<u64> {
    Some(11)
}
fn other_hash(_: &Value, _: usize, _: crate::tagged::SymbolMode) -> Option<u64> {
    Some(22)
}
static CANONICAL: BytecodeSemantics = BytecodeSemantics::new(key, equal, try_equal, hash);
static OTHER: BytecodeSemantics = BytecodeSemantics::new(key, equal, try_equal, other_hash);

#[test]
fn bytecode_semantics_publication_retains_original_operations_after_refusal() {
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
    let installed = payload.get().unwrap();
    assert!(matches!(
        installed.to_equal_key(Value::NIL, 0, &mut Vec::new(), false),
        HashKey::Int(11)
    ));
    assert!(installed.equal(
        &Value::NIL,
        &Value::NIL,
        0,
        &mut None,
        false,
        EqualKind::Plain
    ));
    assert!(matches!(
        installed.try_equal(
            &Value::NIL,
            &Value::NIL,
            0,
            &mut None,
            false,
            EqualKind::Plain
        ),
        Ok(true)
    ));
    assert_eq!(
        installed.sxhash(&Value::NIL, 0, crate::tagged::SymbolMode::Bare),
        Some(11)
    );
    assert!(std::ptr::eq(*identity.get().unwrap(), &CANONICAL));
}

#[test]
fn bytecode_semantics_publication_same_table_is_idempotent() {
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
    assert!(std::ptr::eq(*identity.get().unwrap(), &CANONICAL));
}
