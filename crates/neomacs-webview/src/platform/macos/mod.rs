//! macOS native-overlay backend built on WKWebView.

mod focus;
#[cfg(test)]
mod focus_test;
mod view;

use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::NSView;
use objc2_foundation::{NSString, NSUUID};
use objc2_web_kit::WKWebsiteDataStore;
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

use crate::backend::{
    BackendEvent, CreateOutcome, MissingPrerequisites, Platform, PlatformCreateRequest,
    PlatformPresentation, PlatformUpdate,
};
use crate::{
    BrowsingRelationship, HostWindowId, StoragePartition, WebProfileId, WebViewGeneration,
    WebViewHost, WebViewInitError, WebViewSystemConfig, WebViewWake,
};
use view::MacWebView;

pub(crate) struct MacPlatform {
    mtm: Option<MainThreadMarker>,
    hosts: HashMap<HostWindowId, Retained<NSView>>,
    persistent_stores: HashMap<WebProfileId, Retained<WKWebsiteDataStore>>,
    ephemeral_stores: HashMap<WebProfileId, Retained<WKWebsiteDataStore>>,
    _config: WebViewSystemConfig,
    wake: WebViewWake,
}

impl MacPlatform {
    pub(crate) fn new(config: WebViewSystemConfig, wake: WebViewWake) -> Self {
        Self {
            mtm: MainThreadMarker::new(),
            hosts: HashMap::new(),
            persistent_stores: HashMap::new(),
            ephemeral_stores: HashMap::new(),
            _config: config,
            wake,
        }
    }

    fn data_store(
        &mut self,
        storage: &StoragePartition,
    ) -> Result<Retained<WKWebsiteDataStore>, String> {
        let mtm = self.mtm.ok_or_else(|| {
            "WKWebView must be created on the macOS application main thread".to_owned()
        })?;
        match *storage {
            StoragePartition::Persistent(profile) => {
                if let Some(store) = self.persistent_stores.get(&profile) {
                    return Ok(store.clone());
                }
                // Named persistent stores are available on macOS 14.  Older
                // systems use WebKit's process-wide persistent store; the
                // profile remains stable but cannot be physically isolated by
                // the OS API on those releases.
                let store = if objc2::available!(macos = 14.0) {
                    let high = profile.get() >> 48;
                    let low = profile.get() & 0x0000_ffff_ffff_ffff;
                    let uuid = format!("4e454f4d-4143-5300-{high:04x}-{low:012x}");
                    let uuid =
                        NSUUID::initWithUUIDString(NSUUID::alloc(), &NSString::from_str(&uuid))
                            .ok_or_else(|| {
                                "failed to construct the WebView profile UUID".to_owned()
                            })?;
                    unsafe { WKWebsiteDataStore::dataStoreForIdentifier(&uuid, mtm) }
                } else {
                    unsafe { WKWebsiteDataStore::defaultDataStore(mtm) }
                };
                self.persistent_stores.insert(profile, store.clone());
                Ok(store)
            }
            StoragePartition::Ephemeral(profile) => {
                if let Some(store) = self.ephemeral_stores.get(&profile) {
                    return Ok(store.clone());
                }
                let store = unsafe { WKWebsiteDataStore::nonPersistentDataStore(mtm) };
                self.ephemeral_stores.insert(profile, store.clone());
                Ok(store)
            }
        }
    }

    fn retain_host(host: &WebViewHost) -> Option<Retained<NSView>> {
        let handle = host.window().window_handle().ok()?;
        let RawWindowHandle::AppKit(appkit) = handle.as_raw() else {
            return None;
        };
        // SAFETY: winit's AppKit handle is a live NSView. Retaining it makes
        // the capability valid until unregister_host or platform teardown.
        unsafe {
            let ptr: NonNull<c_void> = appkit.ns_view;
            Retained::retain(ptr.as_ptr().cast())
        }
    }
}

impl Platform for MacPlatform {
    type Host = WebViewHost;
    type PendingCreate = ();
    type View = MacWebView;

    fn register_host(&mut self, id: HostWindowId, host: Self::Host) {
        if let Some(host) = Self::retain_host(&host) {
            self.hosts.insert(id, host);
        } else {
            tracing::warn!(
                ?id,
                "winit did not expose an AppKit NSView for WebView hosting"
            );
        }
    }

    fn unregister_host(&mut self, host: HostWindowId) {
        self.hosts.remove(&host);
    }

    fn missing_prerequisites(&self, _request: &PlatformCreateRequest) -> MissingPrerequisites {
        MissingPrerequisites::empty()
    }

    fn begin_create(
        &mut self,
        request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String> {
        let mtm = self.mtm.ok_or_else(|| {
            WebViewInitError::Backend(
                "WKWebView must be initialized on the macOS application main thread".to_owned(),
            )
            .to_string()
        })?;
        // WKWebView has no public "related view" constructor. WebKit still
        // shares its process pool; spelling the match keeps future variants a
        // compile-time decision point.
        match request.relationship() {
            BrowsingRelationship::Independent | BrowsingRelationship::Related(_) => {}
        }
        let store = self.data_store(request.storage())?;
        let view = MacWebView::new(
            request.id(),
            request.generation(),
            mtm,
            request.size(),
            request.policy(),
            &store,
            self.wake.clone(),
        );
        if let Some(navigation) = request.navigation() {
            view.navigate(navigation)?;
        }
        Ok(CreateOutcome::Ready(view))
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>> {
        Vec::new()
    }

    fn service_view(
        &mut self,
        _id: crate::WebViewId,
        _generation: WebViewGeneration,
        view: &mut Self::View,
    ) -> Vec<crate::WebViewEvent> {
        view.service_events()
    }

    fn update(&mut self, view: &mut Self::View, update: PlatformUpdate<'_>) -> Result<(), String> {
        match update {
            PlatformUpdate::ModelSize(size) => view.resize(size),
            PlatformUpdate::Navigation(target) => view.navigate(target)?,
            PlatformUpdate::History(action) => view.history(action),
            PlatformUpdate::EvaluateScript(request) => view.evaluate_script(request),
            PlatformUpdate::Focus(intent) => view.focus(intent),
        }
        Ok(())
    }

    fn present(
        &mut self,
        _generation: WebViewGeneration,
        view: &mut Self::View,
        presentation: PlatformPresentation<'_>,
    ) -> Result<(), String> {
        match presentation {
            PlatformPresentation::Hidden => view.hide(),
            PlatformPresentation::Visible { host, placement } => {
                let native_host = self
                    .hosts
                    .get(&host)
                    .ok_or_else(|| format!("host {host:?} has no registered AppKit view"))?;
                view.present(host, native_host, placement);
            }
        }
        Ok(())
    }

    fn close(&mut self, _view: Self::View) {}
}
