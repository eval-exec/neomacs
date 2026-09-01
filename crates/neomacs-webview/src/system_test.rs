use std::collections::BTreeSet;

use crate::backend::{
    BackendEvent, CreateOutcome, MissingPrerequisites, Platform, PlatformCreateRequest,
    PlatformPresentation,
};
use crate::system::WebViewSystemImpl;
use crate::{
    BrowsingRelationship, ButtonState, HistoryAction, HostWindowId, NavigationTarget,
    PointerButton, ResolvedWebViewPlacement, ResolvedWebViewScene, StoragePartition,
    WebContentPoint, WebContentSize, WebProfileId, WebViewCommand, WebViewCommandError,
    WebViewCreate, WebViewEvent, WebViewFrame, WebViewGeneration, WebViewId, WebViewInput,
    WebViewModifiers, WebViewOccurrenceId, WebViewPolicy, WebViewSceneRevision, WebViewState,
};
use neomacs_display_protocol::{DeviceScale, DisplayWindowId, RootSurfaceRect};

#[derive(Debug, Default)]
struct FakePlatform {
    hosts: BTreeSet<HostWindowId>,
    creates: Vec<PlatformCreateRequest>,
    asynchronous: bool,
    activate_on_presentation: bool,
    runtime_pending: bool,
    events: Vec<BackendEvent<FakeView>>,
    closed: Vec<WebViewGeneration>,
    inputs: Vec<(WebViewGeneration, WebViewInput)>,
    presentations: Vec<(WebViewGeneration, Option<WebViewOccurrenceId>)>,
}

#[derive(Debug)]
struct FakeView {
    generation: WebViewGeneration,
}

impl Platform for FakePlatform {
    type Host = ();
    type PendingCreate = ();
    type View = FakeView;

    fn register_host(&mut self, id: HostWindowId, (): Self::Host) {
        self.hosts.insert(id);
    }

    fn unregister_host(&mut self, host: HostWindowId) {
        self.hosts.remove(&host);
    }

    fn missing_prerequisites(&self, _request: &PlatformCreateRequest) -> MissingPrerequisites {
        let mut missing = MissingPrerequisites::empty();
        if self.hosts.is_empty() {
            missing |= MissingPrerequisites::HOST;
        }
        if self.runtime_pending {
            missing |= MissingPrerequisites::RUNTIME;
        }
        missing
    }

    fn begin_create(
        &mut self,
        request: PlatformCreateRequest,
    ) -> Result<CreateOutcome<Self::View, Self::PendingCreate>, String> {
        let generation = request.generation();
        self.creates.push(request);
        if self.asynchronous {
            Ok(CreateOutcome::Pending(()))
        } else {
            Ok(CreateOutcome::Ready(FakeView { generation }))
        }
    }

    fn drain_events(&mut self) -> Vec<BackendEvent<Self::View>> {
        std::mem::take(&mut self.events)
    }

    fn activate_pending(
        &mut self,
        generation: WebViewGeneration,
        _pending: &mut Self::PendingCreate,
        presentation: PlatformPresentation<'_>,
    ) -> Result<Option<Self::View>, String> {
        if self.activate_on_presentation
            && matches!(presentation, PlatformPresentation::Visible { .. })
        {
            Ok(Some(FakeView { generation }))
        } else {
            Ok(None)
        }
    }

    fn input(
        &mut self,
        generation: WebViewGeneration,
        _view: &mut Self::View,
        input: WebViewInput,
    ) -> Result<(), String> {
        self.inputs.push((generation, input));
        Ok(())
    }

    fn present(
        &mut self,
        generation: WebViewGeneration,
        _view: &mut Self::View,
        presentation: PlatformPresentation<'_>,
    ) -> Result<(), String> {
        self.presentations.push((
            generation,
            match presentation {
                PlatformPresentation::Hidden => None,
                PlatformPresentation::Visible { placement, .. } => Some(placement.occurrence()),
            },
        ));
        Ok(())
    }

    fn close(&mut self, view: Self::View) {
        self.closed.push(view.generation);
    }
}

impl FakePlatform {
    fn complete(&mut self, id: WebViewId, generation: WebViewGeneration) {
        self.events.push(BackendEvent::CreateFinished {
            id,
            generation,
            result: Ok(FakeView { generation }),
        });
    }
}

fn create(id: WebViewId) -> WebViewCreate {
    WebViewCreate {
        id,
        storage: StoragePartition::Ephemeral(WebProfileId::new(1)),
        relationship: BrowsingRelationship::Independent,
        initial_size: WebContentSize::new(320, 200).unwrap(),
        policy: WebViewPolicy::default(),
        initial_navigation: Some(NavigationTarget::Uri("https://first.invalid/".into())),
    }
}

#[test]
fn webview_service_never_forces_a_periodic_event_loop_deadline() {
    let host = HostWindowId::new(1);
    let mut callback_platform = FakePlatform::default();
    callback_platform.hosts.insert(host);
    let mut callback_system = WebViewSystemImpl::new(callback_platform);
    callback_system
        .command(WebViewCommand::Create(create(WebViewId::new(6))))
        .unwrap();
    // This is a compile-time API assertion: neither `Platform` nor
    // `WebViewSystemImpl` exposes a polling deadline. Backends can only make
    // progress by publishing a wake and being serviced in response.
    callback_system.service();
    assert_eq!(
        callback_system.state(WebViewId::new(6)),
        Some(WebViewState::Ready)
    );
}

#[test]
fn native_runtime_completion_retries_waiting_creates_without_a_nested_loop() {
    let id = WebViewId::new(8);
    let host = HostWindowId::new(1);
    let mut platform = FakePlatform {
        runtime_pending: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);

    system.command(WebViewCommand::Create(create(id))).unwrap();
    assert_eq!(system.state(id), Some(WebViewState::Waiting));
    assert!(system.platform().creates.is_empty());

    system.platform_mut().runtime_pending = false;
    system.service();

    assert_eq!(system.state(id), Some(WebViewState::Ready));
    assert_eq!(system.platform().creates.len(), 1);
}

#[test]
fn runtime_completion_activates_a_view_whose_scene_arrived_while_waiting() {
    let id = WebViewId::new(9);
    let host = HostWindowId::new(2);
    let mut platform = FakePlatform {
        asynchronous: true,
        activate_on_presentation: true,
        runtime_pending: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();
    let placement = ResolvedWebViewPlacement::new(
        id,
        WebViewOccurrenceId::new(1),
        DisplayWindowId::new(1),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        DeviceScale::ONE,
    )
    .unwrap();
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(1), vec![placement])
                .unwrap(),
        )
        .unwrap();

    system.platform_mut().runtime_pending = false;
    system.service();

    assert_eq!(system.state(id), Some(WebViewState::Ready));
}

#[test]
fn pre_ready_state_converges_before_native_creation() {
    let id = WebViewId::new(17);
    let mut system = WebViewSystemImpl::new(FakePlatform::default());

    system.command(WebViewCommand::Create(create(id))).unwrap();
    system
        .command(WebViewCommand::SetModelSize {
            id,
            size: WebContentSize::new(800, 600).unwrap(),
        })
        .unwrap();
    system
        .command(WebViewCommand::Navigate {
            id,
            target: NavigationTarget::Uri("https://latest.invalid/".into()),
        })
        .unwrap();

    assert_eq!(system.state(id), Some(WebViewState::Waiting));
    assert!(system.platform().creates.is_empty());

    system.register_host(HostWindowId::new(9), ());

    assert_eq!(system.state(id), Some(WebViewState::Ready));
    let [request] = system.platform().creates.as_slice() else {
        panic!("native creation must occur exactly once")
    };
    assert_eq!(request.size(), WebContentSize::new(800, 600).unwrap());
    assert_eq!(
        request.navigation(),
        Some(&NavigationTarget::Uri("https://latest.invalid/".into()))
    );
    assert_eq!(
        system.drain_events(),
        vec![WebViewEvent::Ready {
            id,
            generation: WebViewGeneration::new(1),
        }]
    );
}

#[test]
fn history_is_rejected_until_the_native_view_is_ready() {
    let id = WebViewId::new(18);
    let mut system = WebViewSystemImpl::new(FakePlatform::default());
    system.command(WebViewCommand::Create(create(id))).unwrap();

    assert_eq!(
        system.command(WebViewCommand::History {
            id,
            action: HistoryAction::Back,
        }),
        Err(WebViewCommandError::NotReady(id))
    );
}

#[test]
fn command_and_event_protocol_is_shareable_but_system_is_thread_affine() {
    static_assertions::assert_impl_all!(WebViewCommand: Send, Sync);
    static_assertions::assert_impl_all!(WebViewEvent: Send, Sync);
    static_assertions::assert_impl_all!(WebViewFrame: Send);
    static_assertions::assert_impl_all!(WebViewInput: Send, Sync);
    static_assertions::assert_not_impl_any!(WebViewSystemImpl<FakePlatform>: Send, Sync);
}

#[test]
fn input_is_typed_and_only_reaches_a_ready_generation() {
    let id = WebViewId::new(19);
    let host = HostWindowId::new(1);
    let occurrence = WebViewOccurrenceId::new(9);
    let input = WebViewInput::PointerButton {
        position: WebContentPoint::new(12.5, 8.0),
        button: PointerButton::Primary,
        state: ButtonState::Pressed,
        modifiers: WebViewModifiers::CONTROL | WebViewModifiers::SHIFT,
    };
    let mut system = WebViewSystemImpl::new(FakePlatform::default());

    system.command(WebViewCommand::Create(create(id))).unwrap();
    assert_eq!(system.presented_target(id), None);

    system.register_host(host, ());
    assert_eq!(system.presented_target(id), None);
    let rect = RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap();
    let placement = ResolvedWebViewPlacement::new(
        id,
        occurrence,
        DisplayWindowId::new(1),
        rect,
        rect,
        DeviceScale::ONE,
    )
    .unwrap();
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(
                host,
                WebViewSceneRevision::new(1),
                vec![placement.clone()],
            )
            .unwrap(),
        )
        .unwrap();

    let target = system.presented_target(id).unwrap();
    system.input(target, input).unwrap();

    assert_eq!(
        system.platform().inputs,
        vec![(WebViewGeneration::new(1), input)]
    );

    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(2), vec![placement])
                .unwrap(),
        )
        .unwrap();
    assert_eq!(
        system.input(target, input),
        Err(WebViewCommandError::StaleInputScene {
            view: id,
            current: WebViewSceneRevision::new(2),
            received: WebViewSceneRevision::new(1),
        })
    );
}

#[test]
fn presentation_sync_attaches_and_hides_one_typed_occurrence() {
    let id = WebViewId::new(20);
    let host = HostWindowId::new(3);
    let occurrence = WebViewOccurrenceId::new(44);
    let mut platform = FakePlatform::default();
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();

    let placement = ResolvedWebViewPlacement::new(
        id,
        occurrence,
        DisplayWindowId::new(5),
        RootSurfaceRect::new(10.0, 20.0, 320.0, 200.0).unwrap(),
        RootSurfaceRect::new(10.0, 20.0, 320.0, 200.0).unwrap(),
        DeviceScale::ONE,
    )
    .unwrap();
    let visible =
        ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(7), vec![placement]).unwrap();

    assert!(system.synchronize_presentation(visible).unwrap().changed());
    assert_eq!(
        system.platform().presentations,
        vec![(WebViewGeneration::new(1), Some(occurrence))]
    );

    let hidden =
        ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(8), Vec::new()).unwrap();
    assert!(system.synchronize_presentation(hidden).unwrap().changed());
    assert_eq!(
        system.platform().presentations,
        vec![
            (WebViewGeneration::new(1), Some(occurrence)),
            (WebViewGeneration::new(1), None),
        ]
    );
}

#[test]
fn unregistering_a_host_hides_its_views_and_releases_the_attachment() {
    let id = WebViewId::new(24);
    let first_host = HostWindowId::new(6);
    let second_host = HostWindowId::new(7);
    let mut platform = FakePlatform::default();
    platform.hosts.insert(first_host);
    platform.hosts.insert(second_host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();

    let placement = |occurrence| {
        ResolvedWebViewPlacement::new(
            id,
            WebViewOccurrenceId::new(occurrence),
            DisplayWindowId::new(8),
            RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
            RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
            DeviceScale::ONE,
        )
        .unwrap()
    };
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(
                first_host,
                WebViewSceneRevision::new(11),
                vec![placement(47)],
            )
            .unwrap(),
        )
        .unwrap();

    system.unregister_host(first_host);
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(
                second_host,
                WebViewSceneRevision::new(12),
                vec![placement(48)],
            )
            .unwrap(),
        )
        .unwrap();

    assert_eq!(
        system.platform().presentations,
        vec![
            (
                WebViewGeneration::new(1),
                Some(WebViewOccurrenceId::new(47))
            ),
            (WebViewGeneration::new(1), None),
            (
                WebViewGeneration::new(1),
                Some(WebViewOccurrenceId::new(48))
            ),
        ]
    );
}

#[test]
fn presentation_arriving_during_async_creation_converges_when_ready() {
    let id = WebViewId::new(21);
    let host = HostWindowId::new(4);
    let occurrence = WebViewOccurrenceId::new(45);
    let mut platform = FakePlatform {
        asynchronous: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();
    let placement = ResolvedWebViewPlacement::new(
        id,
        occurrence,
        DisplayWindowId::new(6),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        DeviceScale::ONE,
    )
    .unwrap();
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(9), vec![placement])
                .unwrap(),
        )
        .unwrap();
    assert!(system.platform().presentations.is_empty());

    system
        .platform_mut()
        .complete(id, WebViewGeneration::new(1));
    system.service();

    assert_eq!(
        system.platform().presentations,
        vec![(WebViewGeneration::new(1), Some(occurrence))]
    );
}

#[test]
fn recreated_view_converges_to_the_existing_presentation() {
    let id = WebViewId::new(34);
    let host = HostWindowId::new(8);
    let occurrence = WebViewOccurrenceId::new(49);
    let mut platform = FakePlatform::default();
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();
    let placement = ResolvedWebViewPlacement::new(
        id,
        occurrence,
        DisplayWindowId::new(9),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        DeviceScale::ONE,
    )
    .unwrap();
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(13), vec![placement])
                .unwrap(),
        )
        .unwrap();

    system.command(WebViewCommand::Close { id }).unwrap();
    system.command(WebViewCommand::Create(create(id))).unwrap();

    assert_eq!(
        system.platform().presentations,
        vec![
            (WebViewGeneration::new(1), Some(occurrence)),
            (WebViewGeneration::new(2), Some(occurrence)),
        ]
    );
}

#[test]
fn a_visible_presentation_can_activate_a_host_bound_pending_view() {
    let id = WebViewId::new(22);
    let host = HostWindowId::new(5);
    let occurrence = WebViewOccurrenceId::new(46);
    let mut platform = FakePlatform {
        asynchronous: true,
        activate_on_presentation: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(host);
    let mut system = WebViewSystemImpl::new(platform);
    system.command(WebViewCommand::Create(create(id))).unwrap();

    let placement = ResolvedWebViewPlacement::new(
        id,
        occurrence,
        DisplayWindowId::new(7),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        RootSurfaceRect::new(0.0, 0.0, 320.0, 200.0).unwrap(),
        DeviceScale::ONE,
    )
    .unwrap();
    system
        .synchronize_presentation(
            ResolvedWebViewScene::try_new(host, WebViewSceneRevision::new(10), vec![placement])
                .unwrap(),
        )
        .unwrap();

    assert_eq!(system.state(id), Some(WebViewState::Ready));
    assert_eq!(
        system.drain_events(),
        vec![WebViewEvent::Ready {
            id,
            generation: WebViewGeneration::new(1),
        }]
    );
}

#[test]
fn stale_async_create_completion_cannot_replace_a_new_generation() {
    let id = WebViewId::new(23);
    let mut platform = FakePlatform {
        asynchronous: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(HostWindowId::new(1));
    let mut system = WebViewSystemImpl::new(platform);

    system.command(WebViewCommand::Create(create(id))).unwrap();
    assert_eq!(system.state(id), Some(WebViewState::Creating));
    system.command(WebViewCommand::Close { id }).unwrap();
    assert_eq!(
        system.drain_events(),
        vec![WebViewEvent::Closed {
            id,
            generation: WebViewGeneration::new(1),
        }]
    );

    system.command(WebViewCommand::Create(create(id))).unwrap();
    system
        .platform_mut()
        .complete(id, WebViewGeneration::new(1));
    system
        .platform_mut()
        .complete(id, WebViewGeneration::new(2));
    system.service();

    assert_eq!(system.state(id), Some(WebViewState::Ready));
    assert_eq!(
        system.drain_events(),
        vec![WebViewEvent::Ready {
            id,
            generation: WebViewGeneration::new(2),
        }]
    );
    assert_eq!(system.platform().closed, vec![WebViewGeneration::new(1)]);
}

#[test]
fn related_view_must_already_exist_and_use_the_same_storage_partition() {
    let parent = WebViewId::new(30);
    let child = WebViewId::new(31);
    let mut platform = FakePlatform::default();
    platform.hosts.insert(HostWindowId::new(1));
    let mut system = WebViewSystemImpl::new(platform);
    let mut related = create(child);
    related.relationship = BrowsingRelationship::Related(parent);

    assert_eq!(
        system.command(WebViewCommand::Create(related.clone())),
        Err(WebViewCommandError::MissingRelatedView {
            view: child,
            related: parent,
        })
    );
    assert_eq!(system.state(child), None);

    system
        .command(WebViewCommand::Create(create(parent)))
        .unwrap();
    related.storage = StoragePartition::Persistent(WebProfileId::new(1));
    assert_eq!(
        system.command(WebViewCommand::Create(related)),
        Err(WebViewCommandError::IncompatibleRelatedStorage {
            view: child,
            related: parent,
        })
    );
    assert_eq!(system.state(child), None);
}

#[test]
fn related_view_waits_for_its_parent_generation_to_be_ready() {
    let parent = WebViewId::new(32);
    let child = WebViewId::new(33);
    let mut platform = FakePlatform {
        asynchronous: true,
        ..FakePlatform::default()
    };
    platform.hosts.insert(HostWindowId::new(1));
    let mut system = WebViewSystemImpl::new(platform);
    system
        .command(WebViewCommand::Create(create(parent)))
        .unwrap();
    let mut related = create(child);
    related.relationship = BrowsingRelationship::Related(parent);

    system.command(WebViewCommand::Create(related)).unwrap();

    assert_eq!(system.state(parent), Some(WebViewState::Creating));
    assert_eq!(system.state(child), Some(WebViewState::Waiting));
    assert_eq!(system.platform().creates.len(), 1);

    system
        .platform_mut()
        .complete(parent, WebViewGeneration::new(1));
    system.service();

    assert_eq!(system.state(parent), Some(WebViewState::Ready));
    assert_eq!(system.state(child), Some(WebViewState::Creating));
    assert_eq!(system.platform().creates.len(), 2);
}
