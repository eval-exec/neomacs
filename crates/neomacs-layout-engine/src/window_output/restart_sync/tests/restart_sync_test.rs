use super::*;

fn emitter() -> WindowOutputEmitter {
    WindowOutputEmitter::new_speculative(
        neovm_core::window::FrameId(1),
        neovm_core::window::WindowId(1),
        0,
        0.0,
        0.0,
    )
}

#[test]
fn an_emitter_without_preparation_cannot_adopt_a_builder_frontier() {
    let mut emitter = emitter();
    assert!(matches!(
        emitter.finish_edit_sync(Some(EditSyncReached {
            display_row_index: 3,
            y: 48.0
        })),
        Err(SyncStateError::UnownedBuilderReach),
    ));
    assert_eq!(emitter.restart_sync_state(), RestartSyncState::Continuing);
    assert!(matches!(emitter.take_edit_sync_for_retry(), Ok(None)));
}

#[test]
fn invalid_sync_state_is_reported_to_both_completion_and_retry_callers() {
    let mut emitter = emitter();
    emitter.sync_attempt = SyncAttempt::Invalid(SyncStateError::ConflictingReach);
    assert_eq!(emitter.restart_sync_state(), RestartSyncState::Invalid);
    assert!(matches!(
        emitter.finish_edit_sync(None),
        Err(SyncStateError::ConflictingReach),
    ));
    emitter.sync_attempt = SyncAttempt::Invalid(SyncStateError::ConflictingReach);
    assert!(matches!(
        emitter.take_edit_sync_for_retry(),
        Err(SyncStateError::ConflictingReach),
    ));
}
