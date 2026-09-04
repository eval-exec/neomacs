use crate::backend::{
    BackendEvent, CreateOutcome, HostRegistration, MissingPrerequisites, Platform,
    PlatformCreateRequest,
};
use crate::{HostWindowId, WebViewHost, WebViewInitError, WebViewSystemConfig, WebViewWake};

pub(crate) struct UnsupportedPlatform {
    _config: WebViewSystemConfig,
    _wake: WebViewWake,
}

impl UnsupportedPlatform {
    pub(crate) fn new(config: WebViewSystemConfig, wake: WebViewWake) -> Self {
        Self {
            _config: config,
            _wake: wake,
        }
    }
}

impl Platform for UnsupportedPlatform {
    type Host = WebViewHost;
    type PendingCreate = ();
    type View = ();

    fn register_host(&mut self, _id: HostWindowId, _host: Self::Host) -> HostRegistration {
        HostRegistration::Unavailable
    }

    fn unregister_host(&mut self, _host: HostWindowId) {}

    fn missing_prerequisites(&self, _request: &PlatformCreateRequest) -> MissingPrerequisites {
        MissingPrerequisites::empty()
    }

    fn begin_create(
        &mut self,
        _request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String> {
        Err(WebViewInitError::NotBuilt.to_string())
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>> {
        Vec::new()
    }
}
