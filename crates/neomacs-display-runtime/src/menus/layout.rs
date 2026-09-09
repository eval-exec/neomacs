//! Menu panel measurement, independent of native window placement.

use neomacs_display_protocol::{PopupMenuItem, menu::MenuPanel};

pub(super) fn measure_panel(
    x: f32,
    y: f32,
    all_items: &[PopupMenuItem],
    indices: &[usize],
    title: Option<&str>,
    font_size: f32,
    line_height: f32,
    char_width: f32,
) -> MenuPanel {
    let padding = 4.0_f32;
    let item_height = line_height + 3.0;
    let separator_height = 8.0_f32;
    let title_height = if title.is_some() {
        item_height + separator_height
    } else {
        0.0
    };

    let mut total_h = padding + title_height;
    let mut offsets = Vec::with_capacity(indices.len());
    for &idx in indices {
        offsets.push(total_h);
        if all_items[idx].separator {
            total_h += separator_height;
        } else {
            total_h += item_height;
        }
    }
    total_h += padding;

    let _ = font_size; // font_size kept in signature for future use
    let min_width = 150.0_f32;
    let max_label_len = indices
        .iter()
        .map(|&idx| &all_items[idx])
        .filter(|i| !i.separator)
        .map(|i| {
            let extra = if i.shortcut.is_empty() {
                0
            } else {
                i.shortcut.len() + 4
            };
            let arrow = if i.submenu { 3 } else { 0 };
            i.label.len() + extra + arrow
        })
        .max()
        .unwrap_or(10);
    let title_len = title.map(|t| t.len()).unwrap_or(0);
    let content_width = (max_label_len.max(title_len) as f32) * char_width;
    let total_w = (content_width + padding * 4.0).max(min_width);

    MenuPanel {
        x,
        y,
        item_indices: indices.to_vec(),
        hover_index: -1,
        bounds: (x, y, total_w, total_h),
        item_offsets: offsets,
        item_height,
    }
}
