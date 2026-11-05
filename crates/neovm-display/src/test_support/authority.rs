//! Signed display authority fence (peer 19:36Z follow-up).
//!
//! The three DisplayFrameExt test fixtures are cfg-gated trait methods used
//! by the re-homed load/reader oracles and by display/eval tests. Each has
//! one AUTHORITY-ITEMS row; these delegates are the signed fence path. The
//! trait methods keep their bodies and cfg gate unchanged (the N17 extension
//! trait cannot expose pub(crate) inherent methods on the eval-owned Frame).

use crate::window::DisplayFrameExt;
use crate::window::geometry;
use crate::window::{Frame, WindowDisplaySnapshot};

pub fn commit_redisplay_cache_for_test(frame: &mut Frame, snapshots: Vec<WindowDisplaySnapshot>) {
    frame.commit_redisplay_cache_for_test(snapshots);
}

pub fn prepare_and_activate_display_presentation_for_test(
    frame: &mut Frame,
    presentation: geometry::PresentationId,
    snapshots: Vec<WindowDisplaySnapshot>,
) -> Result<(), geometry::PresentationPrepareError> {
    frame.prepare_and_activate_display_presentation_for_test(presentation, snapshots)
}

pub fn replace_redisplay_cache_for_test(frame: &mut Frame, snapshots: Vec<WindowDisplaySnapshot>) {
    frame.replace_redisplay_cache_for_test(snapshots);
}
