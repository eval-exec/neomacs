use super::*;
use crate::emacs_core::eval::RedisplayHookPolicyGuard;

fn split_after_record(splice: bool, new_buffer: BufferId, gnu: bool) {
    let _policy = if gnu {
        RedisplayHookPolicyGuard::gnu()
    } else {
        RedisplayHookPolicyGuard::legacy()
    };
    let mut frames = FrameManager::new();
    let frame_id = frames.create_frame("split-hook-epoch", 800, 600, BufferId(1));
    let original = frames.get(frame_id).unwrap().selected_window;
    if splice {
        frames
            .split_window(
                frame_id,
                original,
                SplitDirection::Vertical,
                BufferId(1),
                None,
                SplitPlacement::AfterTarget,
            )
            .unwrap();
    }
    frames
        .get_mut(frame_id)
        .unwrap()
        .find_window_mut(original)
        .unwrap()
        .record_change_epoch(ChangeStamp::FIRST);
    let sibling = frames
        .split_window(
            frame_id,
            original,
            SplitDirection::Vertical,
            new_buffer,
            None,
            SplitPlacement::AfterTarget,
        )
        .unwrap();
    let frame = frames.get(frame_id).unwrap();
    let old = frame.find_window(original).unwrap();
    let new = frame.find_window(sibling).unwrap();
    assert_eq!(old.old_buffer(), Some(BufferId(1)));
    assert_eq!(old.change_stamp(), Some(ChangeStamp::FIRST));
    assert_eq!(new.buffer_id(), Some(new_buffer));
    if gnu {
        assert_eq!(new.old_buffer(), None, "GNU make_window starts unrecorded");
        assert_eq!(new.change_stamp(), None);
        assert_eq!(
            frames.window_old_buffer(sibling),
            WindowOldBuffer::NeverRecorded
        );
    } else {
        assert_eq!(new.old_buffer(), Some(BufferId(1)));
        assert_eq!(new.change_stamp(), Some(ChangeStamp::FIRST));
    }
}

#[test]
fn gnu_interposed_split_leaf_has_no_recorded_hook_epoch() {
    for buffer in [BufferId(1), BufferId(2)] {
        split_after_record(false, buffer, true);
    }
}

#[test]
fn gnu_spliced_split_leaf_has_no_recorded_hook_epoch() {
    for buffer in [BufferId(1), BufferId(2)] {
        split_after_record(true, buffer, true);
    }
}

#[test]
fn legacy_split_leaf_retains_existing_epoch_policy() {
    for splice in [false, true] {
        split_after_record(splice, BufferId(1), false);
    }
}
