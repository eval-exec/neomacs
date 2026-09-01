//! Popup menu and tooltip overlay state.

use neomacs_display_protocol::PopupMenuItem;

pub struct MenuPanel {
    /// Position (logical pixels)
    pub x: f32,
    pub y: f32,
    /// Indices into the parent PopupMenuState.all_items for items shown in this panel
    pub item_indices: Vec<usize>,
    /// Currently hovered index within item_indices (-1 = none)
    pub hover_index: i32,
    /// Computed layout: (x, y, width, height) in logical pixels
    pub bounds: (f32, f32, f32, f32),
    /// Per-item Y offsets (relative to bounds.y)
    pub item_offsets: Vec<f32>,
    /// Item height
    pub item_height: f32,
}

pub struct PopupMenuState {
    /// All items (flat, at all depths)
    pub all_items: Vec<PopupMenuItem>,
    /// Optional title
    pub title: Option<String>,
    /// The main (root) menu panel
    pub root_panel: MenuPanel,
    /// Open submenu panels (stack: each level is one deeper)
    pub submenu_panels: Vec<MenuPanel>,
    /// Face foreground color (sRGB 0.0-1.0), None = default
    pub face_fg: Option<(f32, f32, f32)>,
    /// Face background color (sRGB 0.0-1.0), None = default
    pub face_bg: Option<(f32, f32, f32)>,
    /// Font metrics
    font_size: f32,
    line_height: f32,
    char_width: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum PopupMenuHit {
    Outside,
    PanelBody { depth: usize },
    Item { depth: usize, local_index: usize },
}

impl PopupMenuState {
    pub fn layout_panel(
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

    pub fn new(
        x: f32,
        y: f32,
        items: Vec<PopupMenuItem>,
        title: Option<String>,
        font_size: f32,
        line_height: f32,
        char_width: f32,
    ) -> Self {
        // Collect top-level item indices (depth == 0)
        let root_indices: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, item)| item.depth == 0)
            .map(|(i, _)| i)
            .collect();

        let root_panel = Self::layout_panel(
            x,
            y,
            &items,
            &root_indices,
            title.as_deref(),
            font_size,
            line_height,
            char_width,
        );

        PopupMenuState {
            all_items: items,
            title,
            root_panel,
            submenu_panels: Vec::new(),
            face_fg: None,
            face_bg: None,
            font_size,
            line_height,
            char_width,
        }
    }

    /// Construct a popup from a semantic anchor after measuring its root
    /// panel.  Final origin selection belongs here because this module owns
    /// popup layout and therefore knows the popup's actual extent.
    pub fn new_placed(
        placement: neomacs_display_protocol::PopupPlacement,
        viewport: neomacs_display_protocol::Rect,
        items: Vec<PopupMenuItem>,
        title: Option<String>,
        font_size: f32,
        line_height: f32,
        char_width: f32,
    ) -> Self {
        let mut state = Self::new(0.0, 0.0, items, title, font_size, line_height, char_width);
        let (_, _, width, height) = state.root_panel.bounds;
        let resolved =
            placement.resolve(neomacs_display_protocol::Size::new(width, height), viewport);
        let origin = resolved.origin();
        state.root_panel.x = origin.x;
        state.root_panel.y = origin.y;
        state.root_panel.bounds.0 = origin.x;
        state.root_panel.bounds.1 = origin.y;
        state
    }

    /// Get the active panel (deepest open submenu, or root)
    pub fn active_panel(&self) -> &MenuPanel {
        self.submenu_panels.last().unwrap_or(&self.root_panel)
    }

    pub fn active_panel_mut(&mut self) -> &mut MenuPanel {
        self.submenu_panels
            .last_mut()
            .unwrap_or(&mut self.root_panel)
    }

    fn panel(&self, depth: usize) -> Option<&MenuPanel> {
        if depth == 0 {
            Some(&self.root_panel)
        } else {
            self.submenu_panels.get(depth - 1)
        }
    }

    fn panel_mut(&mut self, depth: usize) -> Option<&mut MenuPanel> {
        if depth == 0 {
            Some(&mut self.root_panel)
        } else {
            self.submenu_panels.get_mut(depth - 1)
        }
    }

    fn item_global_index(&self, depth: usize, local_index: usize) -> Option<usize> {
        self.panel(depth)
            .and_then(|panel| panel.item_indices.get(local_index).copied())
    }

    fn truncate_submenus_after(&mut self, depth: usize) -> bool {
        let keep_len = depth;
        let changed = self.submenu_panels.len() > keep_len;
        self.submenu_panels.truncate(keep_len);
        changed
    }

    fn set_panel_hover(&mut self, depth: usize, hover_index: i32) -> bool {
        let Some(panel) = self.panel_mut(depth) else {
            return false;
        };
        if panel.hover_index == hover_index {
            return false;
        }
        panel.hover_index = hover_index;
        true
    }

    /// Move hover in the active panel. Returns true if changed.
    pub fn move_hover(&mut self, direction: i32) -> bool {
        // Read panel state without mutable borrow
        let panel = self.active_panel();
        let len = panel.item_indices.len() as i32;
        if len == 0 {
            return false;
        }
        let current_hover = panel.hover_index;
        let indices: Vec<usize> = panel.item_indices.clone();

        let mut idx = current_hover + direction;
        for _ in 0..len {
            if idx < 0 {
                idx = len - 1;
            }
            if idx >= len {
                idx = 0;
            }
            let item_idx = indices[idx as usize];
            let item = &self.all_items[item_idx];
            if !item.separator && item.enabled {
                if idx != current_hover {
                    self.active_panel_mut().hover_index = idx;
                    return true;
                }
                return false;
            }
            idx += direction;
        }
        false
    }

    /// Open submenu for the currently hovered item (if it has one)
    pub fn open_submenu(&mut self) -> bool {
        let depth = self.submenu_panels.len();
        let hover_index = self.active_panel().hover_index;
        if hover_index < 0 {
            return false;
        }
        self.open_submenu_for(depth, hover_index as usize)
    }

    fn open_submenu_for(&mut self, depth: usize, local_index: usize) -> bool {
        let Some(parent_global_idx) = self.item_global_index(depth, local_index) else {
            return false;
        };
        let parent = &self.all_items[parent_global_idx];
        if !parent.submenu {
            return self.truncate_submenus_after(depth);
        }

        let child_depth = parent.depth + 1;
        let mut child_indices = Vec::new();
        for i in (parent_global_idx + 1)..self.all_items.len() {
            let item = &self.all_items[i];
            if item.depth < child_depth {
                break;
            }
            if item.depth == child_depth {
                child_indices.push(i);
            }
        }
        if child_indices.is_empty() {
            return self.truncate_submenus_after(depth);
        }

        let (sub_x, sub_y) = {
            let Some(panel) = self.panel(depth) else {
                return false;
            };
            let (px, py, pw, _ph) = panel.bounds;
            (
                px + pw - 2.0,
                py + panel.item_offsets.get(local_index).copied().unwrap_or(0.0),
            )
        };
        let sub_panel = Self::layout_panel(
            sub_x,
            sub_y,
            &self.all_items,
            &child_indices,
            None,
            self.font_size,
            self.line_height,
            self.char_width,
        );

        let child_panel_index = depth;
        if let Some(existing) = self.submenu_panels.get(child_panel_index)
            && existing.item_indices == sub_panel.item_indices
            && existing.bounds == sub_panel.bounds
        {
            return self.truncate_submenus_after(depth + 1);
        }

        let _ = self.truncate_submenus_after(depth);
        self.submenu_panels.push(sub_panel);
        true
    }

    /// Close the deepest open submenu. Returns true if one was closed.
    pub fn close_submenu(&mut self) -> bool {
        self.submenu_panels.pop().is_some()
    }

    /// Hit test across all panels (deepest first). Returns (panel_depth, item_global_index).
    /// panel_depth: 0 = root, 1+ = submenu level. Returns (-1, -1) for miss.
    pub fn hit_test_all(&self, mx: f32, my: f32) -> (i32, i32) {
        // Check submenu panels deepest first
        for (level, panel) in self.submenu_panels.iter().enumerate().rev() {
            let result = Self::hit_test_panel(panel, &self.all_items, mx, my);
            if result >= 0 {
                return ((level + 1) as i32, result);
            }
            // Check if inside panel bounds (even if not on an item)
            let (bx, by, bw, bh) = panel.bounds;
            if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
                return ((level + 1) as i32, -1);
            }
        }
        // Check root panel
        let result = Self::hit_test_panel(&self.root_panel, &self.all_items, mx, my);
        if result >= 0 {
            return (0, result);
        }
        let (bx, by, bw, bh) = self.root_panel.bounds;
        if mx >= bx && mx <= bx + bw && my >= by && my <= by + bh {
            return (0, -1);
        }
        (-1, -1)
    }

    fn hit_at(&self, mx: f32, my: f32) -> PopupMenuHit {
        match self.hit_test_all(mx, my) {
            (depth, _) if depth < 0 => PopupMenuHit::Outside,
            (depth, local_index) if local_index < 0 => PopupMenuHit::PanelBody {
                depth: depth as usize,
            },
            (depth, local_index) => PopupMenuHit::Item {
                depth: depth as usize,
                local_index: local_index as usize,
            },
        }
    }

    pub fn update_hover_at(&mut self, mx: f32, my: f32) -> bool {
        match self.hit_at(mx, my) {
            PopupMenuHit::Outside => false,
            PopupMenuHit::PanelBody { depth } => self.set_panel_hover(depth, -1),
            PopupMenuHit::Item { depth, local_index } => {
                let mut changed = self.set_panel_hover(depth, local_index as i32);
                let Some(global_index) = self.item_global_index(depth, local_index) else {
                    return changed;
                };
                if self.all_items[global_index].submenu {
                    changed |= self.open_submenu_for(depth, local_index);
                } else {
                    changed |= self.truncate_submenus_after(depth);
                }
                changed
            }
        }
    }

    fn hit_test_panel(panel: &MenuPanel, all_items: &[PopupMenuItem], mx: f32, my: f32) -> i32 {
        let (bx, by, bw, _bh) = panel.bounds;
        if mx < bx || mx > bx + bw || my < by {
            return -1;
        }
        for (i, &offset_y) in panel.item_offsets.iter().enumerate() {
            let item_idx = panel.item_indices[i];
            let item = &all_items[item_idx];
            if item.separator {
                continue;
            }
            let iy = by + offset_y;
            let ih = panel.item_height;
            if my >= iy && my < iy + ih && mx >= bx && mx <= bx + bw {
                return i as i32;
            }
        }
        -1
    }

    /// Convenience: hit_test on the active panel only (for selection)
    pub fn hit_test(&self, mx: f32, my: f32) -> i32 {
        // Check all panels, return global item index of hit
        let (depth, local_idx) = self.hit_test_all(mx, my);
        if local_idx < 0 || depth < 0 {
            return -1;
        }
        let panel = if depth == 0 {
            &self.root_panel
        } else {
            &self.submenu_panels[(depth - 1) as usize]
        };
        if local_idx >= 0 && (local_idx as usize) < panel.item_indices.len() {
            let global_idx = panel.item_indices[local_idx as usize];
            let item = &self.all_items[global_idx];
            if item.enabled && !item.submenu {
                return global_idx as i32;
            }
        }
        -1
    }

    /// Get the items slice for rendering a panel.
    /// Returns: (items_ref, panel_ref) for iteration.
    pub fn panels(&self) -> Vec<&MenuPanel> {
        let mut panels = vec![&self.root_panel];
        for sub in &self.submenu_panels {
            panels.push(sub);
        }
        panels
    }
}

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
