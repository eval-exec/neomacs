//! Window-marker integration.
//!
//! GNU Emacs stores window positions (`w->start`, `w->pointm`, `w->old_pointm`)
//! as `Lisp_Marker` objects registered on the owning buffer's intrusive marker
//! chain. When text is inserted or deleted, the chain automatically adjusts
//! every marker's position, so window positions stay correct without explicit
//! per-window patching.
//!
//! neomacs mirrors this: each `Window::Leaf` owns one atomic marker state for
//! the complete `(start, point, old-point)` set alongside cached Lisp
//! positions. The markers are the source of truth; the caches are refreshed by
//! `sync_window_positions_from_markers` after every text edit.

use crate::buffer::{
    Buffer, BufferId, BufferManager, CharPos0, EmacsBytePos, InsertionType, LispCharPos1,
    TextPositionAnchor,
};
use crate::emacs_core::value::Value;
use crate::window::{
    AttachedWindowPositionMarkers, Frame, FrameManager, Window, WindowPositionMarkerState,
};

/// Window-start markers use `InsertionType::Before` so the marker stays
/// before text inserted at the window start position, matching GNU `w->start`.
const START_INSERTION_TYPE: InsertionType = InsertionType::Before;
/// Window-point markers use `InsertionType::Before` so the marker does not
/// advance past text inserted at point, matching GNU `w->pointm`.
const POINT_INSERTION_TYPE: InsertionType = InsertionType::Before;
/// Old-point markers use `InsertionType::Before`, matching GNU `w->old_pointm`.
const OLD_POINT_INSERTION_TYPE: InsertionType = InsertionType::Before;

fn lisp_position_to_restricted_marker_position(
    bm: &BufferManager,
    buffer_id: BufferId,
    lisp_position: LispCharPos1,
) -> TextPositionAnchor {
    let Some(buffer) = bm.get(buffer_id) else {
        let fallback = lisp_position_to_usize(lisp_position).saturating_sub(1);
        return TextPositionAnchor::new(CharPos0::new(fallback), EmacsBytePos::new(fallback));
    };
    restricted_marker_position(buffer, lisp_position)
}

fn restricted_marker_position(buffer: &Buffer, lisp_position: LispCharPos1) -> TextPositionAnchor {
    let lisp_position = lisp_position_to_usize(lisp_position);
    let char_pos = CharPos0::new(lisp_position.saturating_sub(1).clamp(
        buffer.point_min_char_pos().get(),
        buffer.point_max_char_pos().get(),
    ));
    let byte_pos = buffer.char_pos_to_emacs_byte_pos_clamped(char_pos);
    TextPositionAnchor::new(char_pos, byte_pos)
}

fn lisp_position_to_usize(lisp_position: LispCharPos1) -> usize {
    usize::try_from(lisp_position.as_i64().max(1)).expect("Lisp character position fits usize")
}

fn marker_lisp_position(
    bm: &BufferManager,
    buffer_id: BufferId,
    marker_id: u64,
) -> Option<LispCharPos1> {
    bm.marker_char_pos(buffer_id, marker_id)
        .map(|char_pos| LispCharPos1::from_one_based_usize(char_pos.get().saturating_add(1)))
}

/// Allocate one internal marker and retain its Lisp handle as the precise-GC
/// root owned by the live window. The marker chain itself is weak.
fn create_rooted_marker_at_anchor(
    bm: &mut BufferManager,
    buffer_id: BufferId,
    position: TextPositionAnchor,
    insertion_type: InsertionType,
) -> (u64, Value) {
    let (marker_id, _) = bm.create_marker_at_anchor(buffer_id, position, insertion_type);
    let gc_root = bm
        .marker_value(buffer_id, marker_id)
        .expect("a newly registered window marker must be present in its buffer chain");
    (marker_id, gc_root)
}

/// Attach the complete GNU position-marker set to one leaf window.
///
/// The buffer is read from the window itself so marker ownership cannot be
/// initialized against a different buffer. Reattaching replaces and unchains
/// the old set as one lifecycle transition.
pub fn attach_window_position_markers(bm: &mut BufferManager, window: &mut Window) {
    let Window::Leaf {
        buffer_id,
        window_start,
        position_markers,
        point,
        old_point,
        ..
    } = window
    else {
        return;
    };

    let start = lisp_position_to_restricted_marker_position(bm, *buffer_id, *window_start);
    let start_marker = create_rooted_marker_at_anchor(bm, *buffer_id, start, START_INSERTION_TYPE);
    let point = lisp_position_to_restricted_marker_position(bm, *buffer_id, *point);
    let point_marker = create_rooted_marker_at_anchor(bm, *buffer_id, point, POINT_INSERTION_TYPE);
    let old_point = lisp_position_to_restricted_marker_position(bm, *buffer_id, *old_point);
    let old_point_marker =
        create_rooted_marker_at_anchor(bm, *buffer_id, old_point, OLD_POINT_INSERTION_TYPE);

    let old = std::mem::replace(
        position_markers,
        WindowPositionMarkerState::Attached(AttachedWindowPositionMarkers::new(
            start_marker,
            point_marker,
            old_point_marker,
        )),
    );
    remove_attached_markers(bm, old.attached());
}

pub fn unchain_window_markers(bm: &mut BufferManager, window: &mut Window) {
    let Window::Leaf {
        position_markers, ..
    } = window
    else {
        return;
    };

    let markers = position_markers.detach();
    remove_attached_markers(bm, markers);
}

fn remove_attached_markers(bm: &mut BufferManager, markers: Option<AttachedWindowPositionMarkers>) {
    if let Some(markers) = markers {
        bm.remove_marker(markers.start.raw());
        bm.remove_marker(markers.point.raw());
        bm.remove_marker(markers.old_point.raw());
    }
}

fn move_marker(
    bm: &mut BufferManager,
    buffer_id: BufferId,
    marker_id: Option<u64>,
    lisp_position: LispCharPos1,
) {
    let Some(marker_id) = marker_id else { return };
    let position = lisp_position_to_restricted_marker_position(bm, buffer_id, lisp_position);
    let _ = bm.move_marker_to_anchor(buffer_id, marker_id, position);
}

pub fn set_window_start_with_marker(
    bm: &mut BufferManager,
    window: &mut Window,
    lisp_position: LispCharPos1,
) {
    let Window::Leaf {
        buffer_id,
        window_start,
        position_markers,
        ..
    } = window
    else {
        return;
    };
    *window_start = lisp_position.max(LispCharPos1::ONE);
    move_marker(
        bm,
        *buffer_id,
        position_markers
            .attached()
            .map(|markers| markers.start.raw()),
        lisp_position,
    );
}

pub fn set_window_point_with_marker(
    bm: &mut BufferManager,
    window: &mut Window,
    lisp_position: LispCharPos1,
) {
    let Window::Leaf {
        buffer_id,
        point,
        position_markers,
        ..
    } = window
    else {
        return;
    };
    *point = lisp_position.max(LispCharPos1::ONE);
    move_marker(
        bm,
        *buffer_id,
        position_markers
            .attached()
            .map(|markers| markers.point.raw()),
        lisp_position,
    );
}

pub fn set_window_old_point_with_marker(
    bm: &mut BufferManager,
    window: &mut Window,
    lisp_position: LispCharPos1,
) {
    let Window::Leaf {
        buffer_id,
        old_point,
        position_markers,
        ..
    } = window
    else {
        return;
    };
    *old_point = lisp_position.max(LispCharPos1::ONE);
    move_marker(
        bm,
        *buffer_id,
        position_markers
            .attached()
            .map(|markers| markers.old_point.raw()),
        lisp_position,
    );
}

/// Refresh cached Lisp positions on every leaf window from its markers.
///
/// Call this after text edits (insert/delete) so that the window caches
/// reflect the auto-adjusted marker positions. Only windows whose buffer
/// matches `edited_buffer_id` need updating.
pub fn sync_window_positions_from_markers(
    frame: &mut Frame,
    bm: &BufferManager,
    edited_buffer_id: BufferId,
) {
    sync_subtree(&mut frame.root_window, bm, edited_buffer_id);
    if let Some(ref mut mini) = frame.minibuffer_leaf {
        sync_leaf(mini, bm, edited_buffer_id);
    }
}

fn sync_subtree(window: &mut Window, bm: &BufferManager, edited_buffer_id: BufferId) {
    match window {
        Window::Leaf { .. } => sync_leaf(window, bm, edited_buffer_id),
        Window::Internal { children, .. } => {
            for child in children {
                sync_subtree(child, bm, edited_buffer_id);
            }
        }
    }
}

fn sync_leaf(window: &mut Window, bm: &BufferManager, edited_buffer_id: BufferId) {
    let Window::Leaf {
        buffer_id,
        window_start,
        position_markers,
        point,
        old_point,
        ..
    } = window
    else {
        return;
    };

    if *buffer_id != edited_buffer_id {
        return;
    }
    let Some(markers) = position_markers.attached() else {
        return;
    };

    if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.start.raw()) {
        *window_start = position;
    }
    if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.point.raw()) {
        *point = position;
    }
    if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.old_point.raw()) {
        *old_point = position;
    }
}

/// Clone a live or saved window tree while giving every live-buffer leaf an
/// independent marker set.
///
/// `Window::clone` deliberately cannot do this by itself because allocating
/// markers requires the owning `BufferManager`.  Callers that retain a tree
/// beyond the lifetime of a live-tree borrow (notably window configurations)
/// must cross this boundary instead of retaining copied marker handles.
pub fn clone_window_tree_with_independent_position_markers(
    bm: &mut BufferManager,
    source: &Window,
) -> Window {
    let mut cloned = source.clone();
    refresh_cloned_subtree_from_shared_markers(&mut cloned, bm);
    detach_cloned_subtree_marker_handles(&mut cloned);
    attach_cloned_subtree_independent_markers(&mut cloned, bm);
    cloned
}

fn refresh_cloned_subtree_from_shared_markers(window: &mut Window, bm: &BufferManager) {
    match window {
        Window::Leaf {
            buffer_id,
            window_start,
            position_markers,
            point,
            old_point,
            ..
        } => {
            let Some(markers) = position_markers.attached() else {
                return;
            };
            if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.start.raw()) {
                *window_start = position;
            }
            if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.point.raw()) {
                *point = position;
            }
            if let Some(position) = marker_lisp_position(bm, *buffer_id, markers.old_point.raw()) {
                *old_point = position;
            }
        }
        Window::Internal { children, .. } => {
            for child in children {
                refresh_cloned_subtree_from_shared_markers(child, bm);
            }
        }
    }
}

fn detach_cloned_subtree_marker_handles(window: &mut Window) {
    match window {
        Window::Leaf {
            position_markers, ..
        } => {
            // These handles are shared with SOURCE.  Dropping this copied
            // ownership state must not unchain SOURCE's markers.
            *position_markers = WindowPositionMarkerState::Detached;
        }
        Window::Internal { children, .. } => {
            for child in children {
                detach_cloned_subtree_marker_handles(child);
            }
        }
    }
}

fn attach_cloned_subtree_independent_markers(window: &mut Window, bm: &mut BufferManager) {
    match window {
        Window::Leaf { buffer_id, .. } => {
            // A configuration may outlive its saved buffer.  Such leaves stay
            // detached until restoration chooses a live replacement buffer.
            if bm.get(*buffer_id).is_some() {
                attach_window_position_markers(bm, window);
            }
        }
        Window::Internal { children, .. } => {
            for child in children {
                attach_cloned_subtree_independent_markers(child, bm);
            }
        }
    }
}

/// Attach markers to every leaf owned by a newly constructed frame.
///
/// Frame factories call this once after choosing root/minibuffer buffers and
/// before publishing the frame. That construction seam mirrors GNU
/// `make_window`, where a live window never escapes without all three marker
/// objects.
pub fn attach_frame_window_position_markers(bm: &mut BufferManager, frame: &mut Frame) {
    attach_subtree(&mut frame.root_window, bm);
    if let Some(minibuffer) = frame.minibuffer_leaf.as_mut() {
        attach_window_position_markers(bm, minibuffer);
    }
}

fn attach_subtree(window: &mut Window, bm: &mut BufferManager) {
    match window {
        Window::Leaf { .. } => attach_window_position_markers(bm, window),
        Window::Internal { children, .. } => {
            for child in children {
                attach_subtree(child, bm);
            }
        }
    }
}

/// Walk all frames and sync windows for the given buffer.
pub fn sync_all_frames_for_buffer(
    frames: &mut FrameManager,
    bm: &BufferManager,
    edited_buffer_id: BufferId,
) {
    for frame in frames.frames_mut() {
        sync_window_positions_from_markers(frame, bm, edited_buffer_id);
    }
}
