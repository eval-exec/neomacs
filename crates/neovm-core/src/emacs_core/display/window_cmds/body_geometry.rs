//! Body-height projection shared by the Lisp body/text-height queries.

use super::*;
use crate::window::{WindowChromeLine, WindowChromePresence};

pub(super) fn body_height_pixels(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<i64, Flow> {
    let window = get_leaf(frames, fid, wid)?;
    if let Some(regions) = redisplay_window_regions(frames, fid, wid)? {
        return Ok(regions.text_body().height().get() as i64);
    }
    let frame = frames.get(fid).expect("live window has a frame");
    let mut height = window_height_pixels(window);
    for line in [
        WindowChromeLine::ModeLine,
        WindowChromeLine::HeaderLine,
        WindowChromeLine::TabLine,
    ] {
        height -= chrome_height_pixels(frames, buffers, fid, wid, line)?;
    }
    height -= frames.window_scroll_bar_area_height(wid);
    if !window_is_bottommost(frame, wid) {
        height -= frame.effective_divider_width(FrameDivider::Bottom);
    }
    Ok(height.max(0))
}

pub(super) fn chrome_height_pixels(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
    line: WindowChromeLine,
) -> Result<i64, Flow> {
    if let Some(regions) = redisplay_window_regions(frames, fid, wid)? {
        return Ok(match line {
            WindowChromeLine::ModeLine => regions.mode_line(),
            WindowChromeLine::HeaderLine => regions.header_line(),
            WindowChromeLine::TabLine => regions.tab_line(),
        }
        .map_or(0, |rect| rect.height().get() as i64));
    }
    let window = get_leaf(frames, fid, wid)?;
    let frame = frames.get(fid).expect("live window has a frame");
    let buffer = window
        .buffer_id()
        .and_then(|id| buffers.get(id))
        .ok_or_else(|| {
            signal(
                LispCondition::Error,
                vec![Value::string("Window buffer is not live")],
            )
        })?;
    let presence = WindowChromePresence::resolve(
        window,
        buffer,
        frame,
        is_minibuffer_window(frames, fid, wid),
    );
    if !presence.contains(line) {
        return Ok(0);
    }
    // A missing/zero recorded height is not evidence that a requested line
    // is absent. GNU estimates its height until redisplay measures it.
    let measured = frame
        .redisplay_snapshot(wid)
        .map(|snapshot| match line {
            WindowChromeLine::ModeLine => snapshot.mode_line_height,
            WindowChromeLine::HeaderLine => snapshot.header_line_height,
            WindowChromeLine::TabLine => snapshot.tab_line_height,
        })
        .filter(|height| *height > 0);
    Ok(measured.unwrap_or(frame.char_height.max(1.0) as i64))
}
