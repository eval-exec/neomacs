//! Hit-test infrastructure: maps pixel coordinates to buffer char positions.
//!
//! Built during layout and queried from FFI for mouse interaction.

/// Per-row hit-test data: maps a Y range to a charpos range.
#[derive(Clone)]
pub(crate) struct HitRow {
    pub y_start: f32,
    pub y_end: f32,
    pub charpos_start: i64,
    pub charpos_end: i64,
}

/// Per-window hit-test data built during layout.
#[derive(Clone)]
pub(crate) struct WindowHitData {
    pub window_id: i64,
    pub content_x: f32,
    pub char_w: f32,
    pub rows: Vec<HitRow>,
}

/// Global hit-test data for all windows, updated each frame.
/// Safe to use without Mutex because layout and queries happen on the same (Emacs) thread.
pub(crate) static mut FRAME_HIT_DATA: Option<Vec<WindowHitData>> = None;

/// Query charpos at a given frame-relative pixel coordinate.
/// Searches all windows for the one containing (px, py).
/// Returns charpos, or -1 if not found.
pub fn hit_test_charpos_at_pixel(px: f32, py: f32) -> i64 {
    unsafe {
        let data = match &*std::ptr::addr_of!(FRAME_HIT_DATA) {
            Some(d) => d,
            None => return -1,
        };
        for win in data {
            // Find row by Y
            for row in &win.rows {
                if py >= row.y_start && py < row.y_end {
                    // Compute approximate column from X (guard zero char_w)
                    let cw = if win.char_w > 0.0 { win.char_w } else { 8.0 };
                    let col = ((px - win.content_x) / cw).max(0.0) as i64;
                    let charpos = (row.charpos_start + col).min(row.charpos_end);
                    return charpos;
                }
            }
        }
        -1
    }
}

/// Query charpos for a specific window at window-relative pixel coordinates.
pub fn hit_test_window_charpos(window_id: i64, wx: f32, wy: f32) -> i64 {
    unsafe {
        let data = match &*std::ptr::addr_of!(FRAME_HIT_DATA) {
            Some(d) => d,
            None => return -1,
        };
        for win in data {
            if win.window_id != window_id {
                continue;
            }
            for row in &win.rows {
                if wy >= row.y_start && wy < row.y_end {
                    let cw = if win.char_w > 0.0 { win.char_w } else { 8.0 };
                    let col = ((wx - win.content_x) / cw).max(0.0) as i64;
                    return (row.charpos_start + col).min(row.charpos_end);
                }
            }
            // Past last row: return last charpos
            if let Some(last) = win.rows.last() {
                return last.charpos_end;
            }
            return -1;
        }
        -1
    }
}
