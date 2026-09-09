//! Tooltip overlay state.

pub struct TooltipState {
    /// Position (logical pixels, near mouse cursor)
    pub x: f32,
    pub y: f32,
    /// Tooltip text (may be multi-line)
    pub lines: Vec<String>,
    /// Foreground color (sRGB)
    pub fg: (f32, f32, f32),
    /// Background color (sRGB)
    pub bg: (f32, f32, f32),
    /// Computed bounds (x, y, w, h)
    pub bounds: (f32, f32, f32, f32),
}

impl TooltipState {
    pub fn new(
        x: f32,
        y: f32,
        text: &str,
        fg: (f32, f32, f32),
        bg: (f32, f32, f32),
        screen_w: f32,
        screen_h: f32,
        font_size: f32,
        line_height: f32,
        char_width: f32,
    ) -> Self {
        let padding = 6.0_f32;
        let _ = font_size; // kept in signature for future use

        let lines: Vec<String> = text.lines().map(|l| l.to_string()).collect();
        let max_line_len = lines.iter().map(|l| l.len()).max().unwrap_or(1);
        let w = (max_line_len as f32 * char_width + padding * 2.0).max(40.0);
        let h = lines.len() as f32 * line_height + padding * 2.0;

        // Position tooltip below and to the right of cursor, clamping to screen
        let mut tx = x + 10.0;
        let mut ty = y + 20.0;
        if tx + w > screen_w {
            tx = screen_w - w - 2.0;
        }
        if ty + h > screen_h {
            ty = y - h - 5.0;
        } // flip above cursor
        if tx < 0.0 {
            tx = 0.0;
        }
        if ty < 0.0 {
            ty = 0.0;
        }

        TooltipState {
            x: tx,
            y: ty,
            lines,
            fg,
            bg,
            bounds: (tx, ty, w, h),
        }
    }
}

#[cfg(test)]
#[path = "overlay_state_test.rs"]
mod tests;
