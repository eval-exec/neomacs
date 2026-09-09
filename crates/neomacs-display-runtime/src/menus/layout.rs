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
        if all_items[idx].separator() {
            total_h += separator_height;
        } else {
            total_h += item_height;
        }
    }
    total_h += padding;

    let _ = font_size; // font_size kept in signature for future use
    let min_width = 150.0_f32;
    let rows = || {
        indices
            .iter()
            .map(|&idx| &all_items[idx])
            .filter(|item| !item.separator())
    };
    let label_width = rows()
        .map(|item| item.label.chars().count())
        .max()
        .unwrap_or(10);
    let shortcut_width = rows()
        .map(|item| item.shortcut.chars().count())
        .max()
        .unwrap_or(0);
    let shortcut_gap = if shortcut_width > 0 { 4 } else { 0 };
    let arrow_width = if rows().any(|item| item.submenu()) {
        3
    } else {
        0
    };
    let columns = label_width + shortcut_gap + shortcut_width + arrow_width;
    let title_width = title.map(|text| text.chars().count()).unwrap_or(0);
    let content_width = columns.max(title_width) as f32 * char_width;
    let total_w = (content_width + padding * 4.0).max(min_width);

    let mut panel = MenuPanel {
        x,
        y,
        item_indices: indices.to_vec(),
        hover_index: -1,
        bounds: (x, y, total_w, total_h),
        item_offsets: offsets,
        item_height,
    };
    panel.bounds.2 += panel.indicator_width(all_items);
    panel
}
