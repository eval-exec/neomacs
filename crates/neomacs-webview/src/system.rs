use std::collections::HashMap;
use std::marker::PhantomData;
use std::rc::Rc;

use neomacs_display_protocol::WebViewId;

use crate::backend::{
    BackendEvent, CreateOutcome, HostRegistration, MissingPrerequisites, Platform,
    PlatformCreateRequest, PlatformPresentation, PlatformUpdate,
};
use crate::platform::CurrentPlatform;
use crate::{
    BrowsingRelationship, FocusIntent, HistoryAction, HostWindowId, NavigationTarget,
    ResolvedWebViewPlacement, ResolvedWebViewScene, ScriptRequest, WebContentSize, WebViewCommand,
    WebViewCommandError, WebViewCreate, WebViewEvent, WebViewGeneration, WebViewHost,
    WebViewInitError, WebViewInput, WebViewInputTarget, WebViewPresentationEffects,
    WebViewPresentationError, WebViewState, WebViewSystemConfig, WebViewWake,
};

#[derive(Clone, Debug)]
struct DesiredWebView {
    create: WebViewCreate,
}

impl DesiredWebView {
    fn request(&self, generation: WebViewGeneration) -> PlatformCreateRequest {
        PlatformCreateRequest::new(
            self.create.id,
            generation,
            self.create.storage.clone(),
            self.create.relationship.clone(),
            self.create.initial_size,
            self.create.policy.clone(),
            self.create.initial_navigation.clone(),
        )
    }

    fn set_size(&mut self, size: WebContentSize) {
        self.create.initial_size = size;
    }

    fn set_navigation(&mut self, navigation: NavigationTarget) {
        self.create.initial_navigation = Some(navigation);
    }
}

enum Lifecycle<P: Platform> {
    Waiting(MissingPrerequisites),
    Creating(P::PendingCreate),
    Ready(P::View),
    Failed(String),
    Closing,
}

struct ViewRecord<P: Platform> {
    generation: WebViewGeneration,
    desired: DesiredWebView,
    lifecycle: Lifecycle<P>,
}

/// Public platform-erased WebView service.
pub struct WebViewSystem {
    inner: WebViewSystemImpl<CurrentPlatform>,
}

impl WebViewSystem {
    pub fn new(config: WebViewSystemConfig, wake: WebViewWake) -> Result<Self, WebViewInitError> {
        Ok(Self {
            inner: WebViewSystemImpl::new(CurrentPlatform::new(config, wake)),
        })
    }

    pub fn command(&mut self, command: WebViewCommand) -> Result<(), WebViewCommandError> {
        self.inner.command(command)
    }

    pub fn input(
        &mut self,
        target: WebViewInputTarget,
        input: WebViewInput,
    ) -> Result<(), WebViewCommandError> {
        self.inner.input(target, input)
    }

    #[must_use]
    pub fn presented_target(&self, id: WebViewId) -> Option<WebViewInputTarget> {
        self.inner.presented_target(id)
    }

    pub fn synchronize_presentation(
        &mut self,
        scene: ResolvedWebViewScene,
    ) -> Result<WebViewPresentationEffects, WebViewPresentationError> {
        self.inner.synchronize_presentation(scene)
    }

    pub fn register_host(&mut self, id: HostWindowId, host: WebViewHost) {
        self.inner.register_host(id, host);
    }

    pub fn unregister_host(&mut self, id: HostWindowId) {
        self.inner.unregister_host(id);
    }

    #[must_use]
    pub fn presented_host_ids(&self) -> Vec<HostWindowId> {
        self.inner.presented_host_ids()
    }

    #[must_use]
    pub fn state(&self, id: WebViewId) -> Option<WebViewState> {
        self.inner.state(id)
    }

    pub fn service(&mut self) {
        self.inner.service();
    }

    /// Take the newest composited frame for `id`, if this platform presents
    /// web content through renderer-owned textures.
    pub fn take_frame(&mut self, id: WebViewId) -> Option<crate::WebViewFrame> {
        self.inner.take_frame(id)
    }

    pub fn drain_events(&mut self) -> Vec<WebViewEvent> {
        self.inner.drain_events()
    }

    #[must_use]
    pub fn view_ids(&self) -> Vec<WebViewId> {
        self.inner.view_ids()
    }

    #[must_use]
    pub fn has_pending_frame(&self) -> bool {
        self.inner.has_pending_frame()
    }
}

pub(crate) struct WebViewSystemImpl<P: Platform> {
    views: HashMap<WebViewId, ViewRecord<P>>,
    generations: HashMap<WebViewId, u64>,
    scenes: HashMap<HostWindowId, ResolvedWebViewScene>,
    events: Vec<WebViewEvent>,
    // Declared after `views` so native views are dropped before the platform
    // context/display they borrow during teardown.
    platform: P,
    _thread_affine: PhantomData<Rc<()>>,
}

impl<P: Platform> WebViewSystemImpl<P> {
    pub(crate) fn new(platform: P) -> Self {
        Self {
            views: HashMap::new(),
            generations: HashMap::new(),
            scenes: HashMap::new(),
            events: Vec::new(),
            platform,
            _thread_affine: PhantomData,
        }
    }

    pub(crate) fn command(&mut self, command: WebViewCommand) -> Result<(), WebViewCommandError> {
        match command {
            WebViewCommand::Create(create) => self.create(create),
            WebViewCommand::Close { id } => self.close(id),
            WebViewCommand::SetModelSize { id, size } => self.set_size(id, size),
            WebViewCommand::Navigate { id, target } => self.navigate(id, target),
            WebViewCommand::History { id, action } => self.history(id, action),
            WebViewCommand::EvaluateScript(request) => self.evaluate_script(request),
            WebViewCommand::Focus { id, intent } => self.focus(id, intent),
        }
    }

    fn create(&mut self, create: WebViewCreate) -> Result<(), WebViewCommandError> {
        let id = create.id;
        if self.views.contains_key(&id) {
            return Err(WebViewCommandError::AlreadyExists(id));
        }
        self.validate_relationship(&create)?;
        let next = self
            .generations
            .get(&id)
            .copied()
            .unwrap_or(0)
            .saturating_add(1);
        self.generations.insert(id, next);
        let generation = WebViewGeneration::new(next);
        let desired = DesiredWebView { create };
        let request = desired.request(generation);
        let missing = self.missing_prerequisites(&request);
        let lifecycle = if missing.is_empty() {
            self.begin_create(id, generation, request)
        } else {
            Lifecycle::Waiting(missing)
        };
        self.views.insert(
            id,
            ViewRecord {
                generation,
                desired,
                lifecycle,
            },
        );
        Ok(())
    }

    fn validate_relationship(&self, create: &WebViewCreate) -> Result<(), WebViewCommandError> {
        let BrowsingRelationship::Related(related) = create.relationship else {
            return Ok(());
        };
        let Some(parent) = self.views.get(&related) else {
            return Err(WebViewCommandError::MissingRelatedView {
                view: create.id,
                related,
            });
        };
        if parent.desired.create.storage != create.storage {
            return Err(WebViewCommandError::IncompatibleRelatedStorage {
                view: create.id,
                related,
            });
        }
        Ok(())
    }

    fn missing_prerequisites(&self, request: &PlatformCreateRequest) -> MissingPrerequisites {
        let mut missing = self.platform.missing_prerequisites(request);
        if let BrowsingRelationship::Related(related) = request.relationship()
            && !self
                .views
                .get(related)
                .is_some_and(|record| matches!(record.lifecycle, Lifecycle::Ready(_)))
        {
            missing |= MissingPrerequisites::RELATED_VIEW;
        }
        missing
    }

    fn begin_create(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        request: PlatformCreateRequest,
    ) -> Lifecycle<P> {
        match self.platform.begin_create(request) {
            Ok(CreateOutcome::Ready(view)) => self.accept_created_view(id, generation, view),
            Ok(CreateOutcome::Pending(mut pending)) => {
                let Some((host, placement)) = self.active_placement(id) else {
                    return Lifecycle::Creating(pending);
                };
                match self.platform.activate_pending(
                    generation,
                    &mut pending,
                    PlatformPresentation::Visible {
                        host,
                        placement: &placement,
                    },
                ) {
                    Ok(Some(view)) => self.accept_created_view(id, generation, view),
                    Ok(None) => Lifecycle::Creating(pending),
                    Err(error) => self.failed_lifecycle(id, generation, error),
                }
            }
            Err(error) => self.failed_lifecycle(id, generation, error),
        }
    }

    fn active_placement(&self, id: WebViewId) -> Option<(HostWindowId, ResolvedWebViewPlacement)> {
        self.scenes.iter().find_map(|(host, scene)| {
            scene
                .placements()
                .iter()
                .find(|placement| placement.view() == id)
                .cloned()
                .map(|placement| (*host, placement))
        })
    }

    /// Complete the single `Creating -> Ready | Failed` transition shared by
    /// synchronous and asynchronous platform creation. A logical scene may
    /// outlive a closed native generation, so the replacement must converge
    /// to that scene before clients can observe `Ready`.
    fn accept_created_view(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        mut view: P::View,
    ) -> Lifecycle<P> {
        if let Some((host, placement)) = self.active_placement(id)
            && let Err(error) = self.platform.present(
                generation,
                &mut view,
                PlatformPresentation::Visible {
                    host,
                    placement: &placement,
                },
            )
        {
            self.platform.close(view);
            return self.failed_lifecycle(id, generation, error);
        }
        self.events.push(WebViewEvent::Ready { id, generation });
        Lifecycle::Ready(view)
    }

    fn failed_lifecycle(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        error: String,
    ) -> Lifecycle<P> {
        self.events.push(WebViewEvent::Failed {
            id,
            generation,
            error: error.clone(),
        });
        Lifecycle::Failed(error)
    }

    fn set_size(&mut self, id: WebViewId, size: WebContentSize) -> Result<(), WebViewCommandError> {
        let record = self
            .views
            .get_mut(&id)
            .ok_or(WebViewCommandError::UnknownView(id))?;
        record.desired.set_size(size);
        if let Lifecycle::Ready(view) = &mut record.lifecycle {
            self.platform
                .update(view, PlatformUpdate::ModelSize(size))
                .map_err(|error| WebViewCommandError::Backend { id, error })?;
        }
        Ok(())
    }

    fn navigate(
        &mut self,
        id: WebViewId,
        target: NavigationTarget,
    ) -> Result<(), WebViewCommandError> {
        let record = self
            .views
            .get_mut(&id)
            .ok_or(WebViewCommandError::UnknownView(id))?;
        record.desired.set_navigation(target.clone());
        if let Lifecycle::Ready(view) = &mut record.lifecycle {
            self.platform
                .update(view, PlatformUpdate::Navigation(&target))
                .map_err(|error| WebViewCommandError::Backend { id, error })?;
        }
        Ok(())
    }

    fn update_ready(
        &mut self,
        id: WebViewId,
        update: PlatformUpdate<'_>,
    ) -> Result<(), WebViewCommandError> {
        let record = self
            .views
            .get_mut(&id)
            .ok_or(WebViewCommandError::UnknownView(id))?;
        let Lifecycle::Ready(view) = &mut record.lifecycle else {
            return Err(WebViewCommandError::NotReady(id));
        };
        self.platform
            .update(view, update)
            .map_err(|error| WebViewCommandError::Backend { id, error })
    }

    fn history(&mut self, id: WebViewId, action: HistoryAction) -> Result<(), WebViewCommandError> {
        self.update_ready(id, PlatformUpdate::History(action))
    }

    fn evaluate_script(&mut self, request: ScriptRequest) -> Result<(), WebViewCommandError> {
        let id = request.view;
        self.update_ready(id, PlatformUpdate::EvaluateScript(&request))
    }

    fn focus(&mut self, id: WebViewId, intent: FocusIntent) -> Result<(), WebViewCommandError> {
        self.update_ready(id, PlatformUpdate::Focus(intent))
    }

    pub(crate) fn input(
        &mut self,
        target: WebViewInputTarget,
        input: WebViewInput,
    ) -> Result<(), WebViewCommandError> {
        let id = target.view();
        let scene = self
            .scenes
            .get(&target.host())
            .ok_or(WebViewCommandError::NotPresented(id))?;
        if scene.revision() != target.revision() {
            return Err(WebViewCommandError::StaleInputScene {
                view: id,
                current: scene.revision(),
                received: target.revision(),
            });
        }
        if !scene.placements().iter().any(|placement| {
            placement.view() == id && placement.occurrence() == target.occurrence()
        }) {
            return Err(WebViewCommandError::StaleInputOccurrence {
                view: id,
                occurrence: target.occurrence(),
            });
        }
        let record = self
            .views
            .get_mut(&id)
            .ok_or(WebViewCommandError::UnknownView(id))?;
        let Lifecycle::Ready(view) = &mut record.lifecycle else {
            return Err(WebViewCommandError::NotReady(id));
        };
        self.platform
            .input(record.generation, view, input)
            .map_err(|error| WebViewCommandError::Backend { id, error })
    }

    pub(crate) fn presented_target(&self, id: WebViewId) -> Option<WebViewInputTarget> {
        self.scenes.iter().find_map(|(host, scene)| {
            scene
                .placements()
                .iter()
                .find(|placement| placement.view() == id)
                .map(|placement| {
                    WebViewInputTarget::new(*host, scene.revision(), id, placement.occurrence())
                })
        })
    }

    pub(crate) fn synchronize_presentation(
        &mut self,
        scene: ResolvedWebViewScene,
    ) -> Result<WebViewPresentationEffects, WebViewPresentationError> {
        let host = scene.host();
        if let Some(current) = self.scenes.get(&host) {
            if scene.revision().get() < current.revision().get() {
                return Err(WebViewPresentationError::Stale {
                    host,
                    current: current.revision(),
                    received: scene.revision(),
                });
            }
            if scene.revision() == current.revision() {
                return if *current == scene {
                    Ok(WebViewPresentationEffects::new(false))
                } else {
                    Err(WebViewPresentationError::ConflictingRevision {
                        host,
                        presentation: scene.revision(),
                    })
                };
            }
        }

        for placement in scene.placements() {
            if !self.views.contains_key(&placement.view()) {
                return Err(WebViewPresentationError::UnknownView(placement.view()));
            }
            if let Some((current, _)) = self.scenes.iter().find(|(other_host, other_scene)| {
                **other_host != host
                    && other_scene
                        .placements()
                        .iter()
                        .any(|other| other.view() == placement.view())
            }) {
                return Err(WebViewPresentationError::AttachedToAnotherHost {
                    view: placement.view(),
                    current: *current,
                    requested: host,
                });
            }
        }

        let old = self.scenes.get(&host).cloned();
        let mut changes: HashMap<WebViewId, Option<ResolvedWebViewPlacement>> = HashMap::new();
        if let Some(old) = &old {
            for placement in old.placements() {
                changes.insert(placement.view(), None);
            }
        }
        for placement in scene.placements() {
            let unchanged = old.as_ref().is_some_and(|old| {
                old.placements()
                    .iter()
                    .any(|previous| previous == placement)
            });
            if unchanged {
                changes.remove(&placement.view());
            } else {
                changes.insert(placement.view(), Some(placement.clone()));
            }
        }

        for (view_id, placement) in &changes {
            let Some(record) = self.views.get_mut(view_id) else {
                continue;
            };
            let presentation = placement
                .as_ref()
                .map_or(PlatformPresentation::Hidden, |placement| {
                    PlatformPresentation::Visible { host, placement }
                });
            match &mut record.lifecycle {
                Lifecycle::Ready(view) => self
                    .platform
                    .present(record.generation, view, presentation)
                    .map_err(|error| WebViewPresentationError::Backend {
                        host,
                        view: *view_id,
                        error,
                    })?,
                Lifecycle::Creating(pending) => {
                    match self
                        .platform
                        .activate_pending(record.generation, pending, presentation)
                    {
                        Ok(Some(view)) => {
                            record.lifecycle = Lifecycle::Ready(view);
                            self.events.push(WebViewEvent::Ready {
                                id: *view_id,
                                generation: record.generation,
                            });
                        }
                        Ok(None) => {}
                        Err(error) => {
                            record.lifecycle = Lifecycle::Failed(error.clone());
                            self.events.push(WebViewEvent::Failed {
                                id: *view_id,
                                generation: record.generation,
                                error: error.clone(),
                            });
                            return Err(WebViewPresentationError::Backend {
                                host,
                                view: *view_id,
                                error,
                            });
                        }
                    }
                }
                Lifecycle::Waiting(_) | Lifecycle::Failed(_) | Lifecycle::Closing => {}
            }
        }

        self.scenes.insert(host, scene);
        Ok(WebViewPresentationEffects::new(true))
    }

    fn close(&mut self, id: WebViewId) -> Result<(), WebViewCommandError> {
        let mut record = self
            .views
            .remove(&id)
            .ok_or(WebViewCommandError::UnknownView(id))?;
        if let Lifecycle::Ready(view) = std::mem::replace(&mut record.lifecycle, Lifecycle::Closing)
        {
            self.platform.close(view);
        }
        self.events.push(WebViewEvent::Closed {
            id,
            generation: record.generation,
        });
        Ok(())
    }

    pub(crate) fn register_host(&mut self, id: HostWindowId, host: P::Host) {
        let registration = self.platform.register_host(id, host);
        if registration == HostRegistration::Replaced {
            self.reapply_host_scene(id);
        }
        let waiting: Vec<_> = self
            .views
            .iter()
            .filter_map(|(id, record)| {
                matches!(record.lifecycle, Lifecycle::Waiting(_)).then_some(*id)
            })
            .collect();
        for id in waiting {
            self.retry_waiting(id);
        }
    }

    /// Rebind every active native overlay after the platform reports that the
    /// native capability behind a stable logical host was replaced.
    fn reapply_host_scene(&mut self, host: HostWindowId) {
        let Some(scene) = self.scenes.get(&host).cloned() else {
            return;
        };
        for placement in scene.placements() {
            let Some(record) = self.views.get_mut(&placement.view()) else {
                continue;
            };
            if let Lifecycle::Ready(view) = &mut record.lifecycle
                && let Err(error) = self.platform.present(
                    record.generation,
                    view,
                    PlatformPresentation::Visible { host, placement },
                )
            {
                tracing::warn!(
                    ?host,
                    view = ?placement.view(),
                    %error,
                    "failed to rebind WebView to replacement native host"
                );
            }
        }
    }

    pub(crate) fn unregister_host(&mut self, host: HostWindowId) {
        // Hide while the native host capability is still valid. Removing the
        // scene also releases the one-host attachment invariant, so the same
        // logical WebView may later be presented in a replacement window.
        if let Some(scene) = self.scenes.remove(&host) {
            for placement in scene.placements() {
                let Some(record) = self.views.get_mut(&placement.view()) else {
                    continue;
                };
                if let Lifecycle::Ready(view) = &mut record.lifecycle
                    && let Err(error) =
                        self.platform
                            .present(record.generation, view, PlatformPresentation::Hidden)
                {
                    tracing::warn!(
                        ?host,
                        view = ?placement.view(),
                        %error,
                        "failed to hide WebView before host removal"
                    );
                }
            }
        }
        self.platform.unregister_host(host);
    }

    fn retry_waiting(&mut self, id: WebViewId) {
        let Some(record) = self.views.get(&id) else {
            return;
        };
        let generation = record.generation;
        let request = record.desired.request(generation);
        let missing = self.missing_prerequisites(&request);
        if !missing.is_empty() {
            if let Some(record) = self.views.get_mut(&id) {
                record.lifecycle = Lifecycle::Waiting(missing);
            }
            return;
        }
        let lifecycle = self.begin_create(id, generation, request);
        if let Some(record) = self.views.get_mut(&id) {
            record.lifecycle = lifecycle;
        }
    }

    pub(crate) fn service(&mut self) {
        for event in self.platform.drain_events() {
            match event {
                BackendEvent::CreateFinished {
                    id,
                    generation,
                    result,
                } => self.finish_create(id, generation, result),
            }
        }

        // Related views are a creation dependency. A parent can become ready
        // asynchronously without any host registration, so retry all waiting
        // records after consuming native completion events.
        let waiting: Vec<_> = self
            .views
            .iter()
            .filter_map(|(id, record)| {
                matches!(record.lifecycle, Lifecycle::Waiting(_)).then_some(*id)
            })
            .collect();
        for id in waiting {
            self.retry_waiting(id);
        }

        let ready: Vec<_> = self
            .views
            .iter()
            .filter_map(|(id, record)| {
                matches!(record.lifecycle, Lifecycle::Ready(_)).then_some((*id, record.generation))
            })
            .collect();
        for (id, generation) in ready {
            let Some(record) = self.views.get_mut(&id) else {
                continue;
            };
            let Lifecycle::Ready(view) = &mut record.lifecycle else {
                continue;
            };
            self.events
                .extend(self.platform.service_view(id, generation, view));
        }
    }

    pub(crate) fn take_frame(&mut self, id: WebViewId) -> Option<crate::WebViewFrame> {
        let record = self.views.get_mut(&id)?;
        let Lifecycle::Ready(view) = &mut record.lifecycle else {
            return None;
        };
        self.platform.take_frame(view)
    }

    fn finish_create(
        &mut self,
        id: WebViewId,
        generation: WebViewGeneration,
        result: Result<P::View, String>,
    ) {
        let is_current_create = self.views.get(&id).is_some_and(|record| {
            record.generation == generation && matches!(record.lifecycle, Lifecycle::Creating(_))
        });
        if !is_current_create {
            if let Ok(view) = result {
                self.platform.close(view);
            }
            return;
        }

        let lifecycle = match result {
            Ok(view) => self.accept_created_view(id, generation, view),
            Err(error) => self.failed_lifecycle(id, generation, error),
        };
        self.views
            .get_mut(&id)
            .expect("the current create was checked above")
            .lifecycle = lifecycle;
    }

    pub(crate) fn state(&self, id: WebViewId) -> Option<WebViewState> {
        self.views.get(&id).map(|record| match &record.lifecycle {
            Lifecycle::Waiting(missing) => {
                debug_assert!(!missing.is_empty());
                WebViewState::Waiting
            }
            Lifecycle::Creating(_pending) => WebViewState::Creating,
            Lifecycle::Ready(_view) => WebViewState::Ready,
            Lifecycle::Failed(_error) => WebViewState::Failed,
            Lifecycle::Closing => WebViewState::Closing,
        })
    }

    pub(crate) fn drain_events(&mut self) -> Vec<WebViewEvent> {
        std::mem::take(&mut self.events)
    }

    pub(crate) fn view_ids(&self) -> Vec<WebViewId> {
        self.views.keys().copied().collect()
    }

    pub(crate) fn presented_host_ids(&self) -> Vec<HostWindowId> {
        self.scenes.keys().copied().collect()
    }

    pub(crate) fn has_pending_frame(&self) -> bool {
        self.views.values().any(|record| match &record.lifecycle {
            Lifecycle::Ready(view) => self.platform.has_pending_frame(view),
            _ => false,
        })
    }

    #[cfg(test)]
    pub(crate) const fn platform(&self) -> &P {
        &self.platform
    }

    #[cfg(test)]
    pub(crate) const fn platform_mut(&mut self) -> &mut P {
        &mut self.platform
    }
}
