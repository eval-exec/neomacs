use neomacs_display_protocol::WebViewId;

use crate::{
    BrowsingRelationship, FocusIntent, HistoryAction, HostWindowId, NavigationTarget,
    ResolvedWebViewPlacement, ScriptRequest, StoragePartition, WebContentSize, WebViewEvent,
    WebViewFrame, WebViewGeneration, WebViewInput, WebViewPolicy,
};

bitflags::bitflags! {
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub(crate) struct MissingPrerequisites: u8 {
        const PROFILE = 1 << 0;
        const HOST = 1 << 1;
        const GPU = 1 << 2;
        const RELATED_VIEW = 1 << 3;
        const RUNTIME = 1 << 4;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlatformCreateRequest {
    id: WebViewId,
    generation: WebViewGeneration,
    storage: StoragePartition,
    relationship: BrowsingRelationship,
    size: WebContentSize,
    policy: WebViewPolicy,
    navigation: Option<NavigationTarget>,
}

impl PlatformCreateRequest {
    pub(crate) fn new(
        id: WebViewId,
        generation: WebViewGeneration,
        storage: StoragePartition,
        relationship: BrowsingRelationship,
        size: WebContentSize,
        policy: WebViewPolicy,
        navigation: Option<NavigationTarget>,
    ) -> Self {
        Self {
            id,
            generation,
            storage,
            relationship,
            size,
            policy,
            navigation,
        }
    }

    pub(crate) const fn id(&self) -> WebViewId {
        self.id
    }

    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub(crate) const fn generation(&self) -> WebViewGeneration {
        self.generation
    }

    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub(crate) const fn size(&self) -> WebContentSize {
        self.size
    }

    pub(crate) const fn navigation(&self) -> Option<&NavigationTarget> {
        self.navigation.as_ref()
    }

    pub(crate) const fn storage(&self) -> &StoragePartition {
        &self.storage
    }

    pub(crate) const fn relationship(&self) -> &BrowsingRelationship {
        &self.relationship
    }

    pub(crate) const fn policy(&self) -> &WebViewPolicy {
        &self.policy
    }
}

// Linux and macOS create synchronously; Windows uses `Pending` until a host
// placement supplies the HWND required by its composition controller.
#[allow(dead_code)]
pub(crate) enum CreateOutcome<V, C> {
    Ready(V),
    Pending(C),
}

#[derive(Debug)]
// Kept in the platform contract because native engines may complete creation
// asynchronously; the deterministic fake platform exercises stale completion
// handling even when the current native adapters complete inline.
#[allow(dead_code)]
pub(crate) enum BackendEvent<V> {
    CreateFinished {
        id: WebViewId,
        generation: WebViewGeneration,
        result: Result<V, String>,
    },
}

pub(crate) enum PlatformUpdate<'a> {
    ModelSize(WebContentSize),
    Navigation(&'a NavigationTarget),
    History(HistoryAction),
    EvaluateScript(&'a ScriptRequest),
    Focus(FocusIntent),
}

/// The lifecycle points that every native browser can report without
/// inventing a polling clock.
///
/// A backend may publish finer-grained progress independently, but these
/// milestones give callers deterministic start/finish events even when native
/// wakeups coalesce before the frontend services them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg(any(test, target_os = "macos", target_os = "windows"))]
#[cfg_attr(any(target_os = "macos", target_os = "windows"), allow(dead_code))]
pub(crate) enum NavigationMilestone {
    Started,
    StateChanged,
    Finished,
}

#[cfg(any(test, target_os = "macos", target_os = "windows"))]
impl NavigationMilestone {
    pub(crate) fn normalized_events(
        self,
        id: WebViewId,
        generation: WebViewGeneration,
    ) -> Vec<WebViewEvent> {
        match self {
            Self::Started => vec![WebViewEvent::LoadProgressChanged {
                id,
                generation,
                progress: 0.0,
            }],
            Self::StateChanged => Vec::new(),
            Self::Finished => vec![
                WebViewEvent::LoadProgressChanged {
                    id,
                    generation,
                    progress: 1.0,
                },
                WebViewEvent::LoadFinished {
                    id,
                    generation,
                    navigation: None,
                },
            ],
        }
    }
}

// Composited Linux views do not consume native host placement; native-overlay
// adapters on macOS and Windows do.
#[cfg_attr(target_os = "linux", allow(dead_code))]
pub(crate) enum PlatformPresentation<'a> {
    Hidden,
    Visible {
        host: HostWindowId,
        placement: &'a ResolvedWebViewPlacement,
    },
}

pub(crate) trait Platform {
    type Host;
    type PendingCreate;
    type View;

    fn register_host(&mut self, id: HostWindowId, host: Self::Host);
    fn unregister_host(&mut self, host: HostWindowId);

    fn missing_prerequisites(&self, request: &PlatformCreateRequest) -> MissingPrerequisites;

    fn begin_create(
        &mut self,
        request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String>;

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>>;

    /// Give a platform-owned pending creation the presentation capability it
    /// needs to finish. Native-overlay platforms such as WebView2 cannot
    /// create their controller until a concrete host window is known.
    fn activate_pending(
        &mut self,
        _generation: WebViewGeneration,
        _pending: &mut Self::PendingCreate,
        _presentation: PlatformPresentation<'_>,
    ) -> Result<Option<Self::View>, String> {
        Ok(None)
    }

    fn service_view(
        &mut self,
        _id: WebViewId,
        _generation: WebViewGeneration,
        _view: &mut Self::View,
    ) -> Vec<WebViewEvent> {
        Vec::new()
    }

    fn take_frame(&mut self, _view: &mut Self::View) -> Option<WebViewFrame> {
        None
    }

    fn has_pending_frame(&self, _view: &Self::View) -> bool {
        false
    }

    fn update(
        &mut self,
        _view: &mut Self::View,
        _update: PlatformUpdate<'_>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn input(
        &mut self,
        _generation: WebViewGeneration,
        _view: &mut Self::View,
        _input: WebViewInput,
    ) -> Result<(), String> {
        Ok(())
    }

    fn present(
        &mut self,
        _generation: WebViewGeneration,
        _view: &mut Self::View,
        _presentation: PlatformPresentation<'_>,
    ) -> Result<(), String> {
        Ok(())
    }

    fn close(&mut self, _view: Self::View) {}
}
