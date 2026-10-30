use super::*;

fn selection_size() -> FontSelectionSize {
    FontSelectionSize::new(
        14.0,
        neomacs_display_protocol::DeviceScale::new(1.0).unwrap(),
    )
}

#[test]
fn worker_capture_budget_is_independent_of_owner_chain_length() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.eval_str(
        "(set-fontset-font t ?好 (font-spec :family \"first\" :registry \"iso10646-1\"))",
    )
    .unwrap();
    let requests = [("Base Mono", '好', 400, false, selection_size())];
    let before = FrozenCharacterPolicies::capture(&requests, 4096).unwrap();
    eval.eval_str(
        "(let ((n 0))
           (while (< n 96)
             (set-fontset-font t ?好
               (font-spec :family (format \"candidate-%d\" n) :registry \"iso10646-1\")
               nil 'append)
             (setq n (1+ n))))",
    )
    .unwrap();
    let after = FrozenCharacterPolicies::capture(&requests, before.bytes()).unwrap();
    assert_eq!(after.query_count(), 1);
    assert_eq!(after.bytes(), before.bytes());
    assert!(after.queries.values().next().unwrap().chain.iter().count() >= 97);
    assert_ne!(before.generation, after.generation);
    assert_eq!(before.merged(&after, 2), Err(FontPolicyMergeError::Changed));
}

#[test]
fn worker_capture_keeps_long_configured_names_in_shared_owner_storage() {
    let mut eval = neovm_core::emacs_core::Context::new();
    eval.eval_str(
        "(set-fontset-font t ?好 (font-spec :family \"first\" :registry \"iso10646-1\"))",
    )
    .unwrap();
    let requests = [("Base Mono", '好', 400, false, selection_size())];
    let before = FrozenCharacterPolicies::capture(&requests, 4096).unwrap();
    eval.eval_str(
        "(set-fontset-font t ?好
           (font-spec :family (make-string 8192 ?x) :registry \"iso10646-1\")
           nil 'append)",
    )
    .unwrap();
    let after = FrozenCharacterPolicies::capture(&requests, before.bytes()).unwrap();
    assert_eq!(after.bytes(), before.bytes());
    let retained = after
        .queries
        .values()
        .next()
        .unwrap()
        .chain
        .iter()
        .filter_map(|entry| match entry {
            FontsetRenderingEntry::Font(spec) => spec.family.as_deref(),
            FontsetRenderingEntry::ExplicitNone => None,
        })
        .find(|name| name.len() == 8192);
    assert_eq!(retained, Some("x".repeat(8192).as_str()));
}

#[test]
fn worker_capture_admits_long_request_names_with_an_explicit_byte_budget() {
    let family = "x".repeat(2048);
    let requests = [(family.as_str(), '好', 400, false, selection_size())];
    let captured = FrozenCharacterPolicies::capture(&requests, 16 * 1024).unwrap();
    assert_eq!(captured.query_count(), 1);
    assert_eq!(
        FrozenCharacterPolicies::capture(&requests, captured.bytes() - 1),
        Err(FontPolicyCaptureError::Budget)
    );
}

#[test]
fn repeated_queries_do_not_consume_a_second_byte_or_query_slot() {
    let requests: Vec<_> = (0..128)
        .map(|offset| {
            (
                "Base Mono",
                char::from_u32(0x4e00 + offset).unwrap(),
                400,
                false,
                selection_size(),
            )
        })
        .collect();
    let captured = FrozenCharacterPolicies::capture(&requests, 128 * 4096).unwrap();
    let mut repeated = requests.clone();
    repeated.push(requests[0]);
    let deduplicated = FrozenCharacterPolicies::capture(&repeated, captured.bytes()).unwrap();
    assert_eq!(deduplicated.query_count(), 128);
    assert_eq!(deduplicated.bytes(), captured.bytes());
    let merged = captured.merged(&deduplicated, 128).unwrap();
    assert_eq!(merged.query_count(), 128);
    assert_eq!(merged.bytes(), captured.bytes());
}
