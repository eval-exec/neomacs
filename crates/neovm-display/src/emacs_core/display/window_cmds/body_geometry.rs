//! Body-height projection shared by the Lisp body/text-height queries.

use super::*;
use crate::window::DisplayFrameExt;
use crate::window::DisplayFrameManagerExt;
use crate::window::{WindowChromeLine, WindowChromePresence};

const CHROME_LINES: [WindowChromeLine; 3] = [
    WindowChromeLine::ModeLine,
    WindowChromeLine::HeaderLine,
    WindowChromeLine::TabLine,
];

fn live_chrome_presence(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<WindowChromePresence, Flow> {
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
    Ok(WindowChromePresence::resolve(
        window,
        buffer,
        frame,
        is_minibuffer_window(frames, fid, wid),
    ))
}

pub(super) fn body_height_pixels(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<i64, Flow> {
    let window = get_leaf(frames, fid, wid)?;
    let presence = live_chrome_presence(frames, buffers, fid, wid)?;
    // GNU `window_box_height` (xdisp.c:1307-1370) subtracts only the chrome
    // lines the window currently wants (`WINDOW_*_LINE_HEIGHT`). Reuse the
    // published body rectangle only when the window's outer height and live
    // chrome presence still match the presentation that produced it.
    if let Some(regions) = published_window_regions(frames, fid, wid)? {
        let mut chrome_matches = true;
        for line in CHROME_LINES {
            let published_present = published_geometry(regions.chrome_height_pixels(line))? > 0;
            if presence.contains(line) != published_present {
                chrome_matches = false;
                break;
            }
        }
        let outer_matches =
            published_geometry(regions.outer_height_pixels())? == window_height_pixels(window);
        if chrome_matches && outer_matches {
            return published_geometry(regions.text_body_height_pixels());
        }
    }
    current_body_height_pixels(frames, buffers, fid, wid)
}

/// GNU window_box_height uses committed bounds and currently requested chrome,
/// not the rectangle of the previous presentation.
pub(super) fn current_body_height_pixels(
    frames: &FrameManager,
    buffers: &BufferManager,
    fid: FrameId,
    wid: WindowId,
) -> Result<i64, Flow> {
    let window = get_leaf(frames, fid, wid)?;
    let frame = frames.get(fid).expect("live window has a frame");
    let mut height = window_height_pixels(window);
    for line in CHROME_LINES {
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
    // GNU `WINDOW_{MODE,HEADER,TAB}_LINE_HEIGHT` (window.h:1020-1043) gates
    // recorded chrome heights behind live `window_wants_*_line`.
    let presence = live_chrome_presence(frames, buffers, fid, wid)?;
    if !presence.contains(line) {
        return Ok(0);
    }
    if let Some(regions) = published_window_regions(frames, fid, wid)? {
        let published = published_geometry(regions.chrome_height_pixels(line))?;
        if published > 0 {
            return Ok(published);
        }
    }
    let frame = frames.get(fid).expect("live window has a frame");
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
