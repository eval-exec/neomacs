use neomacs_app::lifecycle::{FrontendLifecycle, LifecycleAction, LifecycleEvent, LifecycleState};

#[test]
fn first_resume_creates_the_frontend_and_duplicate_resume_is_idempotent() {
    let mut lifecycle = FrontendLifecycle::new();

    assert_eq!(
        lifecycle.transition(LifecycleEvent::Resumed),
        LifecycleAction::CreateFrontend
    );
    assert_eq!(lifecycle.state(), LifecycleState::Active);
    assert_eq!(
        lifecycle.transition(LifecycleEvent::Resumed),
        LifecycleAction::None
    );
}

#[test]
fn suspension_releases_frontend_resources_and_can_resume_again() {
    let mut lifecycle = FrontendLifecycle::new();
    assert_eq!(
        lifecycle.transition(LifecycleEvent::Resumed),
        LifecycleAction::CreateFrontend
    );

    assert_eq!(
        lifecycle.transition(LifecycleEvent::Suspended),
        LifecycleAction::DestroyFrontend
    );
    assert_eq!(lifecycle.state(), LifecycleState::Suspended);
    assert_eq!(
        lifecycle.transition(LifecycleEvent::Resumed),
        LifecycleAction::CreateFrontend
    );
}

#[test]
fn exit_is_terminal_and_late_resume_cannot_recreate_the_frontend() {
    let mut lifecycle = FrontendLifecycle::new();
    lifecycle.transition(LifecycleEvent::Resumed);

    assert_eq!(
        lifecycle.transition(LifecycleEvent::ExitRequested),
        LifecycleAction::Exit
    );
    assert_eq!(lifecycle.state(), LifecycleState::Exiting);
    assert_eq!(
        lifecycle.transition(LifecycleEvent::Resumed),
        LifecycleAction::None
    );
}
