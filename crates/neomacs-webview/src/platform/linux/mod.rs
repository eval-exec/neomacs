//! Linux WPE WebKit backend using WPEPlatform.
//!
//! This module provides WPE WebKit integration for embedding web content
//! in Emacs buffers using the WPE Platform API for GPU-accelerated rendering.
//!
//! Architecture:
//! - WPE Platform API: Modern display/view/buffer abstraction
//! - wpe-webkit: WebKit engine (GObject API)
//! - dedicated GLib reactor: event-driven ownership of every WPE object
//! - dma-buf: leased GPU buffer transfer into renderer-owned storage

// The WPE submodules are FFI-heavy: `sys` is bindgen-generated and `engine`/`view`
// wrap raw WPE/GObject C calls whose `unsafe fn` bodies call into C without an inner
// `unsafe {}` block. Scoped here (feature-gated module) instead of crate-wide.
#![allow(unsafe_op_in_unsafe_fn)]

use std::collections::HashMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::ptr::NonNull;

pub(crate) mod sys;

mod display;

mod engine;

mod native;

mod reactor;

mod view;

use reactor::{ReactorEvent, WpeReactorHandle};

use self::sys::{platform as plat, webkit as wk};

use crate::backend::{
    BackendEvent, CreateOutcome, MissingPrerequisites, Platform, PlatformCreateRequest,
    PlatformUpdate,
};
use crate::{
    BrowsingRelationship, HostWindowId, StoragePartition, WebProfileId, WebViewEvent, WebViewFrame,
    WebViewGeneration, WebViewHost, WebViewId, WebViewSystemConfig, WebViewWake,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum LinuxProfileKey {
    Persistent(WebProfileId),
    Ephemeral(WebProfileId),
}

impl From<&StoragePartition> for LinuxProfileKey {
    fn from(storage: &StoragePartition) -> Self {
        match *storage {
            StoragePartition::Persistent(id) => Self::Persistent(id),
            StoragePartition::Ephemeral(id) => Self::Ephemeral(id),
        }
    }
}

pub(super) struct NetworkSession(NonNull<wk::WebKitNetworkSession>);

impl NetworkSession {
    pub(super) fn create(
        storage: &StoragePartition,
        profile_root: Option<&Path>,
    ) -> Result<Self, String> {
        let raw = match *storage {
            StoragePartition::Ephemeral(_) => unsafe { wk::webkit_network_session_new_ephemeral() },
            StoragePartition::Persistent(profile) => {
                let root = profile_root.ok_or_else(|| {
                    "persistent WebView storage requires WebViewSystemConfig::profile_root"
                        .to_owned()
                })?;
                let profile_path = root.join(format!("profile-{}", profile.get()));
                let data = profile_path.join("data");
                let cache = profile_path.join("cache");
                std::fs::create_dir_all(&data)
                    .and_then(|()| std::fs::create_dir_all(&cache))
                    .map_err(|error| {
                        format!(
                            "failed to create persistent WebView profile {profile_path:?}: {error}"
                        )
                    })?;
                let data = path_c_string(&data)?;
                let cache = path_c_string(&cache)?;
                unsafe { wk::webkit_network_session_new(data.as_ptr(), cache.as_ptr()) }
            }
        };
        NonNull::new(raw)
            .map(Self)
            .ok_or_else(|| "WebKit failed to create a network session".to_owned())
    }

    pub(super) fn raw(&self) -> NonNull<wk::WebKitNetworkSession> {
        self.0
    }
}

impl Drop for NetworkSession {
    fn drop(&mut self) {
        unsafe { plat::g_object_unref(self.0.as_ptr().cast()) };
    }
}

fn path_c_string(path: &Path) -> Result<CString, String> {
    CString::new(path.as_os_str().as_bytes())
        .map_err(|_| format!("WebView profile path contains a NUL byte: {path:?}"))
}

pub(super) fn file_navigation_uri(path: &Path) -> Result<String, String> {
    url::Url::from_file_path(path)
        .map(String::from)
        .map_err(|()| format!("cannot convert file path {path:?} to a URI"))
}

pub(crate) struct LinuxPlatform {
    reactor: WpeReactorHandle,
    ready_generations: HashMap<WebViewId, WebViewGeneration>,
    pending_events: HashMap<(WebViewId, WebViewGeneration), Vec<WebViewEvent>>,
}

impl LinuxPlatform {
    pub(crate) fn new(config: WebViewSystemConfig, wake: WebViewWake) -> Self {
        Self {
            reactor: WpeReactorHandle::spawn(config, wake),
            ready_generations: HashMap::new(),
            pending_events: HashMap::new(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct LinuxView {
    id: WebViewId,
    generation: WebViewGeneration,
}

impl Platform for LinuxPlatform {
    type Host = WebViewHost;
    type PendingCreate = ();
    type View = LinuxView;

    fn register_host(&mut self, _id: HostWindowId, host: Self::Host) {
        let _ = host.window();
    }

    fn unregister_host(&mut self, _host: HostWindowId) {}

    fn missing_prerequisites(&self, request: &PlatformCreateRequest) -> MissingPrerequisites {
        match request.relationship() {
            BrowsingRelationship::Independent => MissingPrerequisites::empty(),
            BrowsingRelationship::Related(id) if self.ready_generations.contains_key(id) => {
                MissingPrerequisites::empty()
            }
            BrowsingRelationship::Related(_) => MissingPrerequisites::RELATED_VIEW,
        }
    }

    fn begin_create(
        &mut self,
        request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String> {
        self.reactor.create(request)?;
        Ok(CreateOutcome::Pending(()))
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>> {
        let mut lifecycle = Vec::new();
        for event in self.reactor.drain_events() {
            match event {
                ReactorEvent::CreateFinished {
                    id,
                    generation,
                    result,
                } => {
                    if result.is_ok() {
                        self.ready_generations.insert(id, generation);
                    }
                    lifecycle.push(BackendEvent::CreateFinished {
                        id,
                        generation,
                        result: result.map(|()| LinuxView { id, generation }),
                    });
                }
                ReactorEvent::View(event) => {
                    self.pending_events
                        .entry(event.identity())
                        .or_default()
                        .push(event);
                }
            }
        }
        lifecycle
    }

    fn service_view(
        &mut self,
        id: crate::WebViewId,
        generation: WebViewGeneration,
        _view: &mut Self::View,
    ) -> Vec<WebViewEvent> {
        self.pending_events
            .remove(&(id, generation))
            .unwrap_or_default()
    }

    fn take_frame(&mut self, view: &mut Self::View) -> Option<WebViewFrame> {
        self.reactor.take_frame(view.id, view.generation)
    }

    fn has_pending_frame(&self, view: &Self::View) -> bool {
        self.reactor.has_frame(view.id, view.generation)
    }

    fn update(&mut self, view: &mut Self::View, update: PlatformUpdate<'_>) -> Result<(), String> {
        self.reactor.update(view.id, view.generation, update)
    }

    fn input(
        &mut self,
        generation: WebViewGeneration,
        view: &mut Self::View,
        input: crate::WebViewInput,
    ) -> Result<(), String> {
        debug_assert_eq!(generation, view.generation);
        self.reactor.input(view.id, generation, input)
    }

    fn close(&mut self, view: Self::View) {
        if self.ready_generations.get(&view.id) == Some(&view.generation) {
            self.ready_generations.remove(&view.id);
        }
        self.pending_events.remove(&(view.id, view.generation));
        if let Err(error) = self.reactor.close(view.id, view.generation) {
            tracing::warn!(view = ?view.id, %error, "failed to close WPE reactor view");
        }
    }
}

mod error {
    pub(crate) type DisplayResult<T> = Result<T, DisplayError>;

    #[derive(Debug, thiserror::Error)]
    pub(crate) enum DisplayError {
        #[error("{0}")]
        WebKit(String),
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::file_navigation_uri;

    #[test]
    fn local_file_navigation_uses_a_percent_encoded_file_uri() {
        assert_eq!(
            file_navigation_uri(Path::new("/tmp/web view#1.html")).as_deref(),
            Ok("file:///tmp/web%20view%231.html")
        );
    }
}
