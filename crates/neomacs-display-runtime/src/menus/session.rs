//! Menu hierarchy and navigation, owned by the display runtime.

use neomacs_display_protocol::{PopupMenuItem, menu::MenuPanel};

/// Revision ordering and immutable results, independent of native surfaces.
#[derive(Default)]
pub(super) struct MenuLifetime {
    active: Option<neomacs_display_protocol::menu::MenuToken>,
    latest: Option<neomacs_display_protocol::menu::MenuToken>,
    results: std::collections::VecDeque<neomacs_display_protocol::menu::MenuResult>,
}

impl MenuLifetime {
    pub fn reject(&mut self, token: neomacs_display_protocol::menu::MenuToken) {
        self.results
            .push_back(neomacs_display_protocol::menu::MenuResult::from_index(
                token, -1,
            ));
    }
    pub fn show(&mut self, token: neomacs_display_protocol::menu::MenuToken) -> bool {
        if self.latest.is_some_and(|previous| {
            (previous.session, previous.revision) >= (token.session, token.revision)
        }) {
            return false;
        }
        self.latest = Some(token);
        self.active = Some(token);
        true
    }

    pub fn close(&mut self) {
        self.active = None;
    }

    pub fn hide(&mut self, token: neomacs_display_protocol::menu::MenuToken) -> bool {
        if self.latest.is_none()
            || self.latest == Some(token)
            || (self.active.is_none()
                && self.latest.is_some_and(|previous| {
                    (previous.session, previous.revision) < (token.session, token.revision)
                }))
        {
            self.latest = Some(token);
            self.close();
            return true;
        }
        false
    }

    pub fn finish(&mut self, index: i32) {
        if let Some(token) = self.active.take() {
            self.results
                .push_back(neomacs_display_protocol::menu::MenuResult::from_index(
                    token, index,
                ));
        }
    }

    pub fn take_result(&mut self) -> Option<neomacs_display_protocol::menu::MenuResult> {
        self.results.pop_front()
    }
}

pub struct MenuSession {
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

impl MenuSession {
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

        let root_panel = super::layout::measure_panel(
            x,
            y,
            &items,
            &root_indices,
            title.as_deref(),
            font_size,
            line_height,
            char_width,
        );

        MenuSession {
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

    /// Get the active panel (deepest open submenu, or root)
    pub fn active_panel(&self) -> &MenuPanel {
        self.submenu_panels.last().unwrap_or(&self.root_panel)
    }

    pub fn active_panel_mut(&mut self) -> &mut MenuPanel {
        self.submenu_panels
            .last_mut()
            .unwrap_or(&mut self.root_panel)
    }

    pub(super) fn panel(&self, depth: usize) -> Option<&MenuPanel> {
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
            if !item.separator() && item.enabled() {
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
        if !parent.enabled() || !parent.submenu() {
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
        let sub_panel = super::layout::measure_panel(
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

    fn hit_test_panel(panel: &MenuPanel, all_items: &[PopupMenuItem], mx: f32, my: f32) -> i32 {
        let (bx, by, bw, _bh) = panel.bounds;
        if mx < bx || mx > bx + bw || my < by {
            return -1;
        }
        for (i, &offset_y) in panel.item_offsets.iter().enumerate() {
            let item_idx = panel.item_indices[i];
            let item = &all_items[item_idx];
            if item.separator() {
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

    /// Get the items slice for rendering a panel.
    /// Returns: (items_ref, panel_ref) for iteration.
    pub fn panels(&self) -> Vec<&MenuPanel> {
        let mut panels = vec![&self.root_panel];
        for sub in &self.submenu_panels {
            panels.push(sub);
        }
        panels
    }

    pub(super) fn metrics(&self) -> (f32, f32) {
        (self.font_size, self.line_height)
    }

    /// Pointer coordinates are local to the actual native surface, not a
    /// guessed global origin (the compositor may flip or slide any panel).
    pub(super) fn hover_panel(&mut self, depth: usize, x: f32, y: f32) {
        let Some(panel) = self.panel(depth) else {
            return;
        };
        let local = Self::hit_test_panel(panel, &self.all_items, x + panel.x, y + panel.y);
        self.set_panel_hover(depth, local);
        if local >= 0 {
            let global = self.panel(depth).unwrap().item_indices[local as usize];
            if self.all_items[global].enabled() {
                self.open_submenu_for(depth, local as usize);
            }
        }
    }

    pub(super) fn activate_panel(&mut self, depth: usize) -> Option<i32> {
        let panel = self.panel(depth)?;
        let global = *panel
            .item_indices
            .get(usize::try_from(panel.hover_index).ok()?)?;
        let item = &self.all_items[global];
        use neomacs_display_protocol::menu::{MenuAvailability, MenuItemKind};
        match item.kind {
            MenuItemKind::Command {
                availability: MenuAvailability::Enabled,
                ..
            } => Some(global as i32),
            MenuItemKind::Submenu {
                availability: MenuAvailability::Enabled,
            } => {
                self.open_submenu_for(depth, panel.hover_index as usize);
                None
            }
            MenuItemKind::Command {
                availability: MenuAvailability::Disabled,
                ..
            }
            | MenuItemKind::Submenu {
                availability: MenuAvailability::Disabled,
            }
            | MenuItemKind::Label
            | MenuItemKind::Separator => None,
        }
    }
}

#[cfg(test)]
#[path = "session_test.rs"]
mod tests;
