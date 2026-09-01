use neomacs_webview::{DeveloperToolsPolicy, JavaScriptPolicy, WebViewPolicy};

#[test]
fn policy_construction_uses_distinct_capability_types() {
    let policy = WebViewPolicy::new(JavaScriptPolicy::Disabled, DeveloperToolsPolicy::Enabled);

    assert_eq!(policy.javascript_policy(), JavaScriptPolicy::Disabled);
    assert_eq!(
        policy.developer_tools_policy(),
        DeveloperToolsPolicy::Enabled
    );
    assert!(!policy.javascript());
    assert!(policy.developer_tools());
}
