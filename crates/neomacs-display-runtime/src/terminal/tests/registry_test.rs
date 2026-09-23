use super::*;

fn id() -> TerminalId {
    TerminalId::new(41).expect("nonzero terminal id")
}

#[test]
fn uncommitted_create_reservation_cannot_leave_a_pending_terminal() {
    let terminals = new_shared_terminals();
    let reservation = terminals.reserve(id()).expect("reserve terminal");
    assert_eq!(
        terminals.visible_text(id()).expect("pending terminal"),
        None
    );

    drop(reservation);

    assert!(
        terminals
            .visible_text(id())
            .unwrap_err()
            .contains("unknown neo-term terminal id")
    );
}

#[test]
fn creation_failure_is_not_observable_as_an_eternally_pending_terminal() {
    let terminals = new_shared_terminals();
    terminals.reserve(id()).expect("reserve terminal").commit();

    terminals.mark_failed(id(), "shell executable not found");

    let error = terminals.visible_text(id()).unwrap_err();
    assert!(error.contains("creation failed"), "{error}");
    assert!(error.contains("shell executable not found"), "{error}");
    assert_eq!(terminals.require_active(id()).unwrap_err(), error);

    terminals
        .begin_destroy(id())
        .expect("failed terminal remains destroyable")
        .commit();
    terminals.complete_destroy(id());
    assert!(terminals.visible_text(id()).is_err());
}

#[test]
fn failed_destroy_queueing_restores_the_previous_active_state() {
    let terminals = new_shared_terminals();
    terminals.reserve(id()).expect("reserve terminal").commit();
    let transition = terminals.begin_destroy(id()).expect("begin destroy");
    assert!(
        terminals
            .require_active(id())
            .unwrap_err()
            .contains("being destroyed")
    );

    drop(transition);

    terminals
        .require_active(id())
        .expect("pending state restored");
}

#[test]
fn committed_destroy_stays_typed_until_the_renderer_acknowledges_it() {
    let terminals = new_shared_terminals();
    terminals.reserve(id()).expect("reserve terminal").commit();
    terminals
        .begin_destroy(id())
        .expect("begin destroy")
        .commit();

    assert!(
        terminals
            .require_active(id())
            .unwrap_err()
            .contains("being destroyed")
    );
    assert_eq!(terminals.visible_text(id()).expect("destroy pending"), None);

    terminals.complete_destroy(id());
    assert!(
        terminals
            .visible_text(id())
            .unwrap_err()
            .contains("unknown neo-term terminal id")
    );
}
