//! Menu panel measurement, independent of native window placement.

use neomacs_display_protocol::{
    PopupMenuItem,
    menu::{MenuPanel, MenuTextLayout},
};

pub(super) fn measure_panel(
    x: f32,
    y: f32,
    all_items: &[PopupMenuItem],
    indices: &[usize],
    title: Option<&str>,
    font_size: f32,
    line_height: f32,
    text: &MenuTextLayout,
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
    let label_width = indices
        .iter()
        .filter(|&&i| !all_items[i].separator())
        .map(|&i| text.item(i).label.width())
        .fold(0.0_f32, f32::max);
    let shortcut_width = indices
        .iter()
        .filter(|&&i| !all_items[i].separator())
        .map(|&i| text.item(i).shortcut.width())
        .fold(0.0_f32, f32::max);
    let shortcut_gap = if shortcut_width > 0.0 {
        4.0 * text.space_advance()
    } else {
        0.0
    };
    let arrow_width = if rows().any(|item| item.submenu()) {
        3.0 * text.space_advance()
    } else {
        0.0
    };
    let title_width = if title.is_some() {
        text.title().map_or(0.0, |run| run.width())
    } else {
        0.0
    };
    let content_width =
        (label_width + shortcut_gap + shortcut_width + arrow_width).max(title_width);
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
