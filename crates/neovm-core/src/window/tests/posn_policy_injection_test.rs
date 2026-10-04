//! Independent fixture owners pin numeric policy without changing the process.
//! Each Frame/snapshot has one exclusive mutator; published snapshot readers
//! retain immutable copies with no Lisp values or shared mutable selectors.
use super::*;
use crate::buffer::BufferId;
use crate::window::{FrameId, FrameManager};
use std::sync::Arc;

#[test]
fn independent_frames_pin_extent_policy_without_changing_process_default() {
    let process = posn_object_extent_mode();
    let mut frames = FrameManager::new();
    let on = frames.create_frame("extent-on", 80, 40, BufferId(1));
    let off = frames.create_frame("extent-off", 80, 40, BufferId(1));
    assert_eq!(frames.posn_object_extent_mode(on), process);
    assert_eq!(frames.posn_object_extent_mode(off), process);
    frames
        .get_mut(on)
        .unwrap()
        .set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::On));
    frames
        .get_mut(off)
        .unwrap()
        .set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::Off));
    assert_eq!(frames.posn_object_extent_mode(on), PosnObjectExtentMode::On);
    assert_eq!(
        frames.posn_object_extent_mode(off),
        PosnObjectExtentMode::Off
    );
    assert_eq!(
        frames.get(on).unwrap().posn_object_extent_mode(),
        PosnObjectExtentMode::On
    );
    assert_eq!(
        frames.get(off).unwrap().posn_object_extent_mode(),
        PosnObjectExtentMode::Off
    );
    assert_eq!(frames.posn_object_extent_mode(FrameId(u64::MAX)), process);
    assert_eq!(posn_object_extent_mode(), process);
    frames
        .get_mut(on)
        .unwrap()
        .set_posn_object_extent_mode_for_test(None);
    assert_eq!(frames.posn_object_extent_mode(on), process);
    assert_eq!(
        frames.posn_object_extent_mode(off),
        PosnObjectExtentMode::Off
    );
    assert_eq!(posn_object_extent_mode(), process);
}

#[test]
fn snapshot_policy_remains_with_published_copy_and_is_independent() {
    let process = posn_object_extent_mode();
    let mut on = WindowDisplaySnapshot::default();
    let mut off = WindowDisplaySnapshot::default();
    assert_eq!(on.posn_object_extent_mode(), process);
    assert_eq!(off.posn_object_extent_mode(), process);
    on.set_posn_object_extent_mode_for_test(Some(process));
    assert_eq!(
        on, off,
        "an explicit process default preserves snapshot equality"
    );
    on.set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::On));
    off.set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::Off));
    let published = Arc::new(on.clone());
    assert_ne!(
        on, off,
        "numeric snapshot policy is part of fixture identity"
    );
    on.set_posn_object_extent_mode_for_test(Some(PosnObjectExtentMode::Off));
    assert_eq!(
        published.posn_object_extent_mode(),
        PosnObjectExtentMode::On
    );
    assert_eq!(on.posn_object_extent_mode(), PosnObjectExtentMode::Off);
    assert_eq!(off.posn_object_extent_mode(), PosnObjectExtentMode::Off);
    off.set_posn_object_extent_mode_for_test(None);
    assert_eq!(off.posn_object_extent_mode(), process);
    assert_eq!(posn_object_extent_mode(), process);
}
