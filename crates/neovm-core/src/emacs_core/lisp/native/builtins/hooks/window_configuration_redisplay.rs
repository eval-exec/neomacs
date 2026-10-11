//! GNU configuration restoration keeps live window object epochs and restores
//! minibuffer geometry separately from its source. All state is local to the
//! exclusively borrowed Context; the snapshot owns no concurrent mutable cache.

use super::*;

pub(super) fn prepare(
    eval: &super::super::eval::Context,
    snapshot: &mut WindowConfigurationSnapshot,
    options: WindowConfigurationRestoreOptions,
) {
    if !crate::emacs_core::eval::gnu_redisplay_hooks_enabled() {
        return;
    }
    let Some(frame) = eval.frames.get(snapshot.frame_id) else {
        return;
    };
    let same_epoch = snapshot.change_stamp == frame.change_stamp;
    let restore_epoch = |window: &mut crate::window::Window| {
        let Some(record) = eval.frames.window_restore_change_record(window.id()) else {
            return;
        };
        if let crate::window::Window::Leaf {
            old_buffer,
            change_stamp,
            ..
        } = window
        {
            // GNU does not restore w->change_stamp. delete_all_child_windows
            // retains it while remembering the outgoing buffer in old_buffer.
            *change_stamp = record.change_stamp;
            // GNU window.c:7844 restores the saved old_buffer only when no
            // window_change_record occurred between capture and restoration.
            if !same_epoch {
                *old_buffer = record.old_buffer;
            }
        }
    };
    snapshot.tree.for_each_leaf_mut(restore_epoch);
    if let Some(mini) = &mut snapshot.minibuffer_leaf {
        restore_epoch(mini);
    }

    if options.minibuffer_window != MinibufferWindowRestoration::KeepCurrent {
        return;
    }
    let Some(mini_id) = snapshot.minibuffer_window else {
        return;
    };
    let Some(live_mini) = frame.find_window(mini_id) else {
        return;
    };
    // GNU restores geometry and other saved window properties before its
    // DONT-SET-MINIWINDOW buffer/marker predicate (window.c:7862,7917).
    // This also covers a mini-only frame whose minibuffer is its root.
    if let Some(saved_mini) = snapshot.tree.find_mut(mini_id) {
        keep_live_source(saved_mini, live_mini);
    }
    if let Some(saved_mini) = &mut snapshot.minibuffer_leaf
        && saved_mini.id() == mini_id
    {
        keep_live_source(saved_mini, live_mini);
    }
}

fn keep_live_source(saved: &mut crate::window::Window, live: &crate::window::Window) {
    if let (
        crate::window::Window::Leaf {
            buffer_id,
            window_start,
            position_markers,
            point,
            old_point,
            force_start,
            ..
        },
        crate::window::Window::Leaf {
            buffer_id: live_buffer,
            window_start: live_start,
            position_markers: live_markers,
            point: live_point,
            old_point: live_old_point,
            force_start: live_force_start,
            ..
        },
    ) = (saved, live)
    {
        *buffer_id = *live_buffer;
        *window_start = *live_start;
        *position_markers = live_markers.clone();
        *point = *live_point;
        *old_point = *live_old_point;
        *force_start = *live_force_start;
    }
}

#[cfg(test)]
#[path = "tests/window_configuration_redisplay_test.rs"]
mod tests;
