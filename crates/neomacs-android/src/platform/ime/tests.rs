use super::InputMethod;
use neomacs_app::frontend_event::{FrontendEvent, FrontendFrameId, ImeOperation, ImeSessionId};
use winit::event::Ime;

#[test]
fn deletion_and_commit_form_one_ordered_replacement() {
    let mut input = InputMethod::default();
    let target = FrontendFrameId::PRIMARY;
    assert_eq!(
        input.event(Ime::Enabled, target),
        Some(FrontendEvent::Ime {
            session: ImeSessionId(1),
            operation: ImeOperation::Begin,
            target,
        })
    );
    assert!(
        input
            .event(
                Ime::DeleteSurrounding {
                    before_bytes: 4,
                    after_bytes: 2
                },
                target
            )
            .is_none()
    );
    assert_eq!(
        input.event(Ime::Commit("好".into()), target),
        Some(FrontendEvent::Ime {
            session: ImeSessionId(1),
            operation: ImeOperation::Replace {
                before_bytes: 4,
                after_bytes: 2,
                text: "好".into()
            },
            target,
        })
    );
    assert_eq!(
        input.event(Ime::Commit("!".into()), target),
        Some(FrontendEvent::Ime {
            session: ImeSessionId(1),
            operation: ImeOperation::Replace {
                before_bytes: 0,
                after_bytes: 0,
                text: "!".into()
            },
            target,
        })
    );
}

#[test]
fn disabled_session_drops_pending_deletion_and_does_not_reuse_identity() {
    let mut input = InputMethod::default();
    let target = FrontendFrameId::PRIMARY;
    input.event(Ime::Enabled, target);
    input.event(
        Ime::DeleteSurrounding {
            before_bytes: 6,
            after_bytes: 0,
        },
        target,
    );
    assert_eq!(
        input.event(Ime::Disabled, target),
        Some(FrontendEvent::Ime {
            session: ImeSessionId(1),
            operation: ImeOperation::End,
            target,
        })
    );
    assert!(input.event(Ime::Commit("stale".into()), target).is_none());
    input.event(Ime::Enabled, target);
    assert_eq!(
        input.event(Ime::Commit(String::new()), target),
        Some(FrontendEvent::Ime {
            session: ImeSessionId(2),
            operation: ImeOperation::Replace {
                before_bytes: 0,
                after_bytes: 0,
                text: String::new()
            },
            target,
        })
    );
}

#[test]
fn preedit_is_not_an_accepted_editor_edit() {
    let mut input = InputMethod::default();
    let target = FrontendFrameId::PRIMARY;
    input.event(Ime::Enabled, target);
    assert!(
        input
            .event(Ime::Preedit("你好".into(), Some((6, 6))), target)
            .is_none()
    );
}
