use neomacs_webview::WebViewSystem;
#[cfg(not(feature = "webview"))]
use neomacs_webview::{
    BrowsingRelationship, NavigationTarget, StoragePartition, WebContentSize, WebProfileId,
    WebViewCommand, WebViewCreate, WebViewEvent, WebViewInitError, WebViewPolicy, WebViewState,
    WebViewSystemConfig, WebViewWake,
};

#[cfg(not(feature = "webview"))]
fn create() -> WebViewCreate {
    WebViewCreate {
        id: neomacs_webview::WebViewId::new(1),
        storage: StoragePartition::Ephemeral(WebProfileId::new(1)),
        relationship: BrowsingRelationship::Independent,
        initial_size: WebContentSize::new(320, 200).unwrap(),
        policy: WebViewPolicy::default(),
        initial_navigation: Some(NavigationTarget::Uri("https://example.invalid/".into())),
    }
}

#[test]
#[cfg(not(feature = "webview"))]
fn a_build_without_platform_support_reports_typed_unavailability() {
    let mut system =
        WebViewSystem::new(WebViewSystemConfig::default(), WebViewWake::noop()).unwrap();
    let create = create();
    let id = create.id;

    system.command(WebViewCommand::Create(create)).unwrap();

    assert_eq!(system.state(id), Some(WebViewState::Failed));
    assert_eq!(
        system.drain_events(),
        vec![WebViewEvent::Failed {
            id,
            generation: neomacs_webview::WebViewGeneration::new(1),
            error: WebViewInitError::NotBuilt.to_string(),
        }]
    );
}

#[test]
fn public_system_is_not_send_or_sync() {
    static_assertions::assert_not_impl_any!(WebViewSystem: Send, Sync);
}
