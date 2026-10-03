//! A counting frontend still has to complete an accepted display transaction.
//! These fixtures measure repaint scheduling, not glyph metrics. Publish only
//! their trusted live viewports, retain fallback body/chrome measurement, then
//! acknowledge GNU ownership after validated prepare and renderer activation.
//! All state belongs to the exclusively borrowed Context/Frame; no policy or
//! Lisp values are cached globally or in thread-local storage.

use crate::emacs_core::Context;
use crate::window::{PresentedWindowRegions, WindowDisplaySnapshot};
use neomacs_display_protocol::types::Rect;

pub fn accept_all_frames(eval: &mut Context) {
    for frame_id in eval.frames.frame_list() {
        let (windows, snapshots, buffers) = {
            let frame = eval.frames.get(frame_id).expect("live mock frame");
            let windows = frame.all_leaf_ids();
            let mut snapshots = Vec::with_capacity(windows.len());
            let mut buffers = Vec::with_capacity(windows.len());
            for &window in &windows {
                let leaf = frame.find_window(window).expect("live mock leaf");
                let bounds = leaf.bounds();
                snapshots.push(WindowDisplaySnapshot {
                    window_id: window,
                    regions: PresentedWindowRegions {
                        outer: Rect::new(bounds.x, bounds.y, bounds.width, bounds.height),
                        ..PresentedWindowRegions::default()
                    },
                    // No glyph/font measurement in a counter fixture. Preserve
                    // the real body's fallback dimensions for window hooks.
                    regions_materialized: false,
                    ..WindowDisplaySnapshot::default()
                });
                if let Some(buffer) = leaf.buffer_id() {
                    buffers.push(buffer);
                }
            }
            (windows, snapshots, buffers)
        };
        let presentation =
            crate::window::geometry::PresentationId::new(eval.begin_interaction_presentation());
        let frame = eval.frames.get_mut(frame_id).expect("live mock frame");
        frame
            .prepare_live_window_presentation(presentation, snapshots)
            .expect("validated mock display preparation");
        frame
            .activate_display_presentation(presentation)
            .expect("mock renderer accepted presentation");
        assert_eq!(frame.active_presentation(), Some(presentation));
        assert!(
            windows
                .iter()
                .all(|&window| frame.redisplay_snapshot(window).is_some())
        );
        for buffer in buffers {
            eval.buffers
                .get(buffer)
                .expect("displayed mock buffer")
                .reset_unchanged_region();
        }
        eval.note_gnu_frame_display_accepted(frame_id, windows);
        // The mock completed this physical frame draw, like the TTY renderer.
        let _ = eval.gnu_take_tty_frame_redraw(frame_id);
    }
}
