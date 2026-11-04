//! Upper extension traits for evaluator-owned window data (stage-4 W2/E5).
//!
//! The data moved into `emacs_core::eval::data::window_state`; the
//! behaviour that binds this engine lives here as extension traits, the
//! N17 shape the split requires (core cannot add inherent impls to eval
//! types once eval is a crate).

use super::display::{frame_default_left_fringe_width, frame_default_right_fringe_width};
use super::tty_posn_current::{
    TtyCurrentMatrixAuthority, TtyMatrixAllocation, TtyPosnCurrentOwner,
};
use super::*;
use crate::emacs_core::value::Value;
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;
use std::sync::Arc;

use super::{
    ChildExtent, Rect, Window, WindowId, WindowParameters, WindowTree, display, sibling_bounds,
};

/// Window behaviour that mints display state rather than reading data.
pub trait DisplayWindowExt {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters;
}

impl DisplayWindowExt for Window {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters {
        let next_generation = display::next_window_parameters_generation();
        match self {
            Window::Leaf {
                parameters,
                parameters_generation,
                ..
            }
            | Window::Internal {
                parameters,
                parameters_generation,
                ..
            } => {
                *parameters_generation = next_generation;
                parameters
            }
        }
    }
}

/// Window-tree behaviour that binds the sibling layout algorithm.
pub trait DisplayWindowTreeExt {
    fn sibling_bounds_of(&self, parent: WindowId, bounds: Rect) -> Vec<Rect>;
    fn redistribute_bounds(&mut self, parent: WindowId, bounds: Rect);
}

impl DisplayWindowTreeExt for WindowTree {
    /// The rectangles `parent`'s children should occupy inside `bounds`, in
    /// child order.
    ///
    /// Reading each child's extent finishes before any of them is written, so
    /// no two nodes are borrowed at once.  The arithmetic itself lives in
    /// [`sibling_layout`], where it is testable without a window tree.
    fn sibling_bounds_of(&self, parent: WindowId, bounds: Rect) -> Vec<Rect> {
        let extents: Vec<ChildExtent> = self
            .child_ids(parent)
            .iter()
            .filter_map(|child| self.find(*child))
            .map(|child| ChildExtent {
                bounds: *child.bounds(),
                fixed_width_cols: child.fixed_width_cols(),
                fixed_height_lines: child.fixed_height_lines(),
            })
            .collect();
        sibling_bounds(bounds, &extents)
    }

    /// Give `parent`'s children their share of `bounds`, without descending.
    fn redistribute_bounds(&mut self, parent: WindowId, bounds: Rect) {
        let children = self.child_ids(parent).to_vec();
        for (child, rect) in children
            .into_iter()
            .zip(self.sibling_bounds_of(parent, bounds))
        {
            if let Some(node) = self.find_mut(child) {
                node.set_bounds(rect);
            }
        }
    }
}

/// Frame behaviour that binds redisplay, faces, terminal geometry and the
/// evaluator's buffers (stage-4 W2/E5 FM-5d).
pub trait DisplayFrameExt {
    fn new(
        id: FrameId,
        name: Value,
        terminal_id: u64,
        width: u32,
        height: u32,
        root_window: Window,
        minibuffer_window_id: WindowId,
    ) -> Self;

    fn name_value(&self) -> Value;

    fn title_value(&self) -> Value;

    fn explicit_name_value(&self) -> Value;

    fn icon_name_value(&self) -> Value;

    fn focus_frame_value(&self) -> Value;

    fn name_runtime_string_owned(&self) -> String;

    fn title_runtime_string_owned(&self) -> Option<String>;

    fn host_title_runtime_string_owned(&self) -> String;

    fn host_title_lisp_string(&self) -> crate::heap_types::LispString;

    fn set_name_value(&mut self, name: Value);

    fn set_generated_name_value(&mut self, name: Value);

    fn set_title_value(&mut self, title: Value);

    fn clear_title(&mut self);

    fn record_window_change_epoch(&mut self, stamp: ChangeStamp);

    fn recalculate_minibuffer_bounds(&mut self);

    fn selected_window(&self) -> Option<&Window>;

    fn selected_window_mut(&mut self) -> Option<&mut Window>;

    fn replace_buffer_bindings(&mut self, old_id: BufferId, new_id: BufferId);

    fn effective_window_system(&self) -> Option<Value>;

    fn font_cell_ascent(&self) -> f32;

    fn layout_inputs(&self) -> FrameLayoutInputState;

    fn set_window_system(&mut self, window_system: Option<Value>);

    fn frame_parameter_int(&self, key: &str) -> Option<i64>;

    fn known_frame_parameter_int(&self, key: FrameParam) -> Option<i64>;

    fn nonnegative_frame_parameter_int(&self, key: FrameParam) -> Option<i64>;

    fn child_frame_border_width_raw(&self) -> Option<i64>;

    fn internal_border_width(&self) -> i64;

    fn frame_child_frame_border_width(&self) -> i64;

    fn effective_divider_width(&self, divider: FrameDivider) -> i64;

    fn outer_border_width(&self) -> i64;

    fn install_gnu_gui_default_parameters(&mut self);

    fn fixed_parameter(&self, key: FrameQueryParam) -> Option<Value>;

    fn parameter(&self, key: &str) -> Option<Value>;

    fn parameter_symbol(&self, key: &crate::emacs_core::intern::WellKnown) -> Option<Value>;

    fn set_display_identity(&mut self, identity: FrameDisplayIdentity);

    fn display_identity(&self) -> &FrameDisplayIdentity;

    fn known_parameter(&self, key: FrameParam) -> Option<Value>;

    fn parameter_key(&self, key: FrameParamKey) -> Option<Value>;

    fn set_parameter(&mut self, key: Value, value: Value) -> Option<Value>;

    fn set_known_parameter(&mut self, key: FrameParam, value: Value) -> Option<Value>;

    fn set_parameter_key(&mut self, key: FrameParamKey, value: Value) -> Option<Value>;

    fn remove_parameter(&mut self, key: Value) -> Option<Value>;

    fn remove_known_parameter(&mut self, key: FrameParam) -> Option<Value>;

    fn remove_parameter_key(&mut self, key: FrameParamKey) -> Option<Value>;

    fn face_hash_table(&self) -> Value;

    fn defer_next_gui_parameter_resize(&mut self);

    fn should_defer_gui_parameter_resize(&self) -> bool;

    fn queue_pending_gui_resize(
        &mut self,
        width_cols: i64,
        total_lines: i64,
        host_target: Option<(u32, u32)>,
    );

    fn clear_pending_gui_resize(&mut self);

    fn gui_geometry_hints(&self) -> GuiFrameGeometryHints;

    fn chrome_top_height(&self) -> f32;

    fn child_frame_viewport_origin(&self) -> (f32, f32);

    fn default_left_fringe_width(&self) -> i64;

    fn default_right_fringe_width(&self) -> i64;

    fn default_vertical_scroll_bar_side(&self) -> Option<&'static str>;

    fn default_vertical_scroll_bar_width(&self) -> i64;

    fn vertical_scroll_bar_area_width(&self) -> i64;

    fn horizontal_scroll_bar_area_height(&self) -> i64;

    fn horizontal_non_text_width(&self) -> i64;

    fn window_area_bounds_with_chrome(&self, reserve_chrome: bool) -> Rect;

    fn window_text_area_bounds_with_chrome(&self, reserve_chrome: bool) -> Rect;

    fn window_text_area_bounds(&self) -> Rect;

    fn sync_window_area_bounds(&mut self);

    fn set_window_layout_text_size(&mut self, cols: i64, text_lines: i64);

    fn restored_window_configuration_root_bounds(&self) -> Rect;

    fn reconcile_restored_window_configuration_geometry(&mut self);

    fn sync_minibuffer_top_line_after_root_resize(&mut self);

    fn reposition_minibuffer_below_root(&mut self);

    fn sync_tab_bar_height_from_parameters(&mut self);

    fn sync_menu_bar_height_from_parameters(&mut self);

    fn menu_bar_pixel_height_from_parameters(&self) -> u32;

    fn frame_top_margin(&self) -> i64;

    fn sync_tool_bar_height_from_parameters(&mut self);

    fn tool_bar_pixel_height_from_parameters(&self) -> u32;

    fn sync_compact_bar_height_from_parameters(&mut self);

    fn compact_bar_pixel_height_from_parameters(&self) -> u32;

    fn gui_button_bar_pixel_height(&self, lines: i64) -> u32;

    fn sync_gui_bar_heights_from_parameters(&mut self, presentation: GuiBarPresentation);

    fn sync_bar_heights_from_parameters(&mut self);

    fn select_window(&mut self, id: WindowId) -> bool;

    fn find_window(&self, id: WindowId) -> Option<&Window>;

    fn leaf_window_paths(&self, generation: u64) -> Vec<(WindowId, WindowTreePath)>;

    fn window_at_path(&self, path: &WindowTreePath) -> Option<&Window>;

    fn root_window(&self) -> &Window;

    fn tree(&self) -> &WindowTree;

    fn tree_mut(&mut self) -> &mut WindowTree;

    fn set_tree(&mut self, tree: WindowTree);

    fn root_window_mut(&mut self) -> &mut Window;

    fn root_leaf_ids(&self) -> Vec<WindowId>;

    fn all_leaf_ids(&self) -> Vec<WindowId>;

    fn find_window_mut(&mut self, id: WindowId) -> Option<&mut Window>;

    fn window_list(&self) -> Vec<WindowId>;

    fn live_window_ids_with_minibuffer(&self) -> Vec<WindowId>;

    fn window_count(&self) -> usize;

    fn window_at(&self, px: f32, py: f32) -> Option<WindowId>;

    fn columns(&self) -> u32;

    fn lines(&self) -> u32;

    #[cfg(test)]
    fn commit_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>);

    #[cfg(test)]
    fn prepare_and_activate_display_presentation_for_test(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError>;

    fn prepare_live_window_presentation(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError>;

    fn prepare_display_presentation(
        &mut self,
        presentation: geometry::PresentationId,
        publications: Vec<WindowPresentationSnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError>;

    fn prepare_display_presentation_with_tty_posn_pool(
        &mut self,
        presentation: geometry::PresentationId,
        publications: Vec<WindowPresentationSnapshot>,
        pool: Option<Arc<neomacs_display_protocol::posn_frame_pool::PosnFramePool>>,
    ) -> Result<(), geometry::PresentationPrepareError>;

    fn activate_display_presentation(
        &mut self,
        presentation: geometry::PresentationId,
    ) -> Result<Option<geometry::PresentationId>, geometry::PresentationActivateError>;

    fn discard_display_presentation(&mut self, presentation: geometry::PresentationId) -> bool;

    fn retire_display_presentation(&mut self, presentation: geometry::PresentationId) -> bool;

    fn is_display_presentation_prepared(&self, presentation: geometry::PresentationId) -> bool;

    fn has_prepared_display_presentations(&self) -> bool;

    fn active_presentation(&self) -> Option<geometry::PresentationId>;

    fn active_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry>;

    fn completed_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry>;

    fn active_window_presentation(&self, window: WindowId) -> Option<&WindowPresentationSnapshot>;

    fn resolve_active_visual_anchor(
        &self,
        query: geometry::VisualAnchorQuery,
    ) -> Result<geometry::VisualAnchorGeometry, geometry::GeometryQueryError>;

    fn begin_display_output_pass(&mut self);

    fn window_output_update(&mut self, window_id: WindowId) -> Option<WindowOutputUpdate<'_>>;

    fn replay_window_output_snapshot(&mut self, snapshot: &WindowDisplaySnapshot);

    fn commit_completed_window_output(
        &mut self,
        generation: geometry::PresentationId,
        publications: &[WindowPresentationSnapshot],
    );

    #[cfg(test)]
    fn replace_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>);

    fn redisplay_snapshot(&self, id: WindowId) -> Option<&WindowDisplaySnapshot>;

    fn tty_posn_live_margins_clear(&self) -> bool;

    fn tty_posn_pool(
        &self,
    ) -> Option<&Arc<neomacs_display_protocol::posn_frame_pool::PosnFramePool>>;

    fn retained_tty_posn_extent(
        &self,
        id: WindowId,
        row: i64,
        column: i64,
    ) -> neomacs_display_protocol::posn_object_extent::PosnObjectExtent;

    fn window_part_geometry(&self, id: WindowId) -> Option<WindowPartGeometry>;

    fn window_part_geometry_from(
        &self,
        id: WindowId,
        snapshot: Option<&WindowDisplaySnapshot>,
    ) -> Option<WindowPartGeometry>;

    fn coordinate_hit(&self, x: i64, y: i64) -> Option<FrameCoordinateHit>;

    fn coordinate_hit_with(
        &self,
        x: i64,
        y: i64,
        recomputed: Option<(WindowId, &WindowDisplaySnapshot)>,
    ) -> Option<FrameCoordinateHit>;

    fn remove_redisplay_snapshot(&mut self, id: WindowId);

    fn resize_pixelwise(&mut self, width: u32, height: u32);

    fn sync_window_size_fixed_from_buffers(&mut self, buffers: &BufferManager);

    fn refresh_geometry_for_changed_font(&mut self, buffers: &BufferManager);

    fn resize_pixelwise_with_buffer_constraints(
        &mut self,
        buffers: &BufferManager,
        width: u32,
        height: u32,
    );

    fn grow_mini_window(&mut self, delta_rows: i32);

    fn grow_mini_window_with_max_lines(&mut self, delta_rows: i32, max_lines: f32);

    fn plan_mini_window_resize(
        &self,
        content_height_px: f32,
        max_lines: f32,
    ) -> Option<MiniWindowResize>;

    fn apply_mini_window_resize(&mut self, resize: MiniWindowResize);

    fn shrink_mini_window(&mut self);

    fn window_layout_inputs(&self, window_id: WindowId) -> Option<WindowLayoutInputState>;

    fn note_tty_current_matrix_publication(
        &mut self,
        publications: &[WindowPresentationSnapshot],
        pool: Option<Arc<PosnFramePool>>,
    );

    fn tty_posn_apply_window_adjustment(&mut self, window: WindowId);

    fn tty_posn_clear_current_window(&mut self, window: WindowId);

    fn tty_posn_adjust_current_matrices(&mut self);

    fn tty_posn_adjust_current_matrices_enabled(&mut self);

    /// Port of dispnew.c allocate_matrices_for_frame_redisplay's TTY branch.
    /// Dimensions are total cols/lines; origins come from recursive chains,
    /// not pixel painting, point/start, text-body or named chrome geometry.
    /// Port of dispnew.c allocate_matrices_for_frame_redisplay's TTY branch.
    /// Dimensions are total cols/lines; origins come from recursive chains,
    /// not pixel painting, point/start, text-body or named chrome geometry.
    fn tty_current_matrix_allocation(&self) -> (Vec<TtyMatrixAllocation>, (i64, i64));

    /// Allocation-free numeric admission read. Local and pooled current rows
    /// share the same clear authority; callers have already selected extent ON.
    /// Allocation-free numeric admission read. Local and pooled current rows
    /// share the same clear authority; callers have already selected extent ON.
    fn tty_posn_current_matrix_authority(
        &self,
        window: WindowId,
    ) -> Option<TtyCurrentMatrixAuthority>;
}

impl DisplayFrameExt for Frame {
    fn new(
        id: FrameId,
        name: Value,
        terminal_id: u64,
        width: u32,
        height: u32,
        mut root_window: Window,
        minibuffer_window_id: WindowId,
    ) -> Self {
        // The minibuffer window id is allocated from the shared window sequence
        // counter by the caller (right after the root), so it gets #2 like GNU.
        let minibuffer_window = minibuffer_window_id;
        let minibuffer_buffer_id = root_window.buffer_id().unwrap_or(BufferId(0));
        let minibuffer_height = 16.0_f32.min(height as f32);
        let root_bounds = Rect::new(
            root_window.bounds().x,
            root_window.bounds().y,
            width as f32,
            (height as f32 - minibuffer_height).max(0.0),
        );
        root_window.set_bounds(root_bounds);
        let mut minibuffer_leaf = Window::new_leaf(
            minibuffer_window,
            minibuffer_buffer_id,
            Rect::new(
                root_bounds.x,
                root_bounds.y + root_bounds.height,
                width as f32,
                minibuffer_height,
            ),
        );
        if let Window::Leaf {
            window_start,
            point,
            ..
        } = &mut minibuffer_leaf
        {
            *window_start = LispCharPos1::ONE;
            *point = LispCharPos1::ONE;
        }
        let selected = root_window.id();
        Self {
            id,
            change_stamp: ChangeStamp::FIRST,
            name,
            explicit_name: false,
            icon_name: Value::NIL,
            focus_frame: Value::NIL,
            parent_frame: Value::NIL,
            terminal_id,
            initial: false,
            tree: WindowTree::new(root_window),
            selected_window: selected,
            // GNU `make_frame_without_minibuffer` leaves
            // `old_selected_window` as Qnil. The first
            // `select-window` records the outgoing selection.
            old_selected_window: None,
            minibuffer_window: Some(minibuffer_window),
            minibuffer_leaf: Some(minibuffer_leaf),
            width,
            height,
            left_pos: 0,
            top_pos: 0,
            window_system: None,
            window_system_parameter: None,
            display_identity: FrameDisplayIdentity::default(),
            // GNU terminal frames expose the terminal's default colors as
            // string sentinel values, not concrete RGB colors.  `term.c`
            // initializes FRAME_FOREGROUND_PIXEL / FRAME_BACKGROUND_PIXEL to
            // FACE_TTY_DEFAULT_FG_COLOR / FACE_TTY_DEFAULT_BG_COLOR, and
            // `xfaces.c::tty_color_name` exposes those as "unspecified-fg" /
            // "unspecified-bg".  GUI frame creation overwrites these with
            // concrete black/white defaults after it installs a window system.
            parameters: {
                let mut params = HashMap::default();
                params.insert(
                    FrameParam::ForegroundColor.symbol(),
                    Value::string("unspecified-fg"),
                );
                params.insert(
                    FrameParam::BackgroundColor.symbol(),
                    Value::string("unspecified-bg"),
                );
                params.insert(FrameParam::CursorColor.symbol(), Value::string("white"));
                // GNU terminal frames expose a numeric tab-bar-lines frame
                // parameter even when the tab bar is disabled. Lisp window
                // deletion code compares it with `>`, so nil is not compatible.
                params.insert(FrameParam::TabBarLines.symbol(), Value::fixnum(0));
                params.insert(Value::symbol("minibuffer"), Value::T);
                params
            },
            background_alpha: 1.0,
            frame_alpha: [-1.0; 2],
            visibility: FrameVisibility::Visible,
            // Set true only once an interactive frontend displays this frame.
            displays_chrome: false,
            title: Value::NIL,
            gui_bar_presentation: GuiBarPresentation::Separate,
            menu_bar_height: 0,
            tool_bar_height: 0,
            compact_bar_height: 0,
            tab_bar_height: 0,
            font_pixel_size: 16.0,
            char_width: 8.0,
            char_height: 16.0,
            // The cell convention for a frame whose font has not been probed:
            // the whole cell is above the baseline, so `ascent + descent` stays
            // equal to `char_height` for every consumer (see `font_ascent`).
            font_ascent: 16.0,
            device_scale_factor: 1.0,
            defer_next_gui_parameter_resize: false,
            pending_gui_resize: None,
            presentation_state: FramePresentationState::default(),
            redisplay_cache: HashMap::default(),
            tty_posn_pool: None,
            tty_posn_pool_can_repartition: false,
            #[cfg(any(test, feature = "redisplay-test-policy"))]
            test_posn_object_extent_mode: None,
            window_hook_record: FrameWindowHookRecord::default(),
            window_state_change: false,
            face_hash_table: Value::hash_table(HashTableTest::Eq),
            face_realization: Default::default(),
            z_order: 0,
            undecorated: false,
            no_accept_focus: false,
            no_split: false,
            buffer_list: Vec::new(),
            buried_buffer_list: Vec::new(),
        }
    }

    fn name_value(&self) -> Value {
        self.name
    }

    fn title_value(&self) -> Value {
        self.title
    }

    fn explicit_name_value(&self) -> Value {
        Value::bool_val(self.explicit_name)
    }

    fn icon_name_value(&self) -> Value {
        self.icon_name
    }

    fn focus_frame_value(&self) -> Value {
        self.focus_frame
    }

    fn name_runtime_string_owned(&self) -> String {
        self.name
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
            .unwrap_or_default()
    }

    fn title_runtime_string_owned(&self) -> Option<String> {
        self.title
            .as_lisp_string()
            .map(|ls| crate::emacs_core::emacs_char::to_utf8_lossy(ls.as_bytes()))
    }

    fn host_title_runtime_string_owned(&self) -> String {
        self.title_runtime_string_owned()
            .filter(|title| !title.is_empty())
            .or_else(|| {
                let name = self.name_runtime_string_owned();
                (!name.is_empty()).then_some(name)
            })
            .unwrap_or_else(|| "Neomacs".to_string())
    }

    fn host_title_lisp_string(&self) -> crate::heap_types::LispString {
        self.title
            .as_lisp_string()
            .filter(|ls| !ls.as_bytes().is_empty())
            .or_else(|| {
                self.name
                    .as_lisp_string()
                    .filter(|ls| !ls.as_bytes().is_empty())
            })
            .cloned()
            .unwrap_or_else(|| crate::heap_types::LispString::from_utf8("Neomacs"))
    }

    fn set_name_value(&mut self, name: Value) {
        self.explicit_name = true;
        self.name = name;
    }

    fn set_generated_name_value(&mut self, name: Value) {
        self.explicit_name = false;
        self.name = name;
    }

    fn set_title_value(&mut self, title: Value) {
        self.title = title;
    }

    fn clear_title(&mut self) {
        self.title = Value::NIL;
    }

    /// Recalculate minibuffer bounds after an operation that changes only the
    /// window tree.
    ///
    /// This must not call `sync_window_area_bounds`: GNU's `window-resize-apply`
    /// has already computed child sizes, and resyncing the whole frame would
    /// redistribute the tree and lose those sizes.
    /// Stamp every live leaf with STAMP and record the buffer it shows.
    ///
    /// GNU's `window_change_record_windows` (`src/window.c`) walks the tree,
    /// recursing past internal windows and recording only live leaves -- which
    /// is why an internal window never reports an old buffer.
    fn record_window_change_epoch(&mut self, stamp: ChangeStamp) {
        for window_id in self.window_list() {
            if let Some(window) = self.find_window_mut(window_id) {
                window.record_change_epoch(stamp);
            }
        }
        if let Some(minibuffer) = self.minibuffer_leaf.as_mut() {
            minibuffer.record_change_epoch(stamp);
        }
    }

    fn recalculate_minibuffer_bounds(&mut self) {
        self.reposition_minibuffer_below_root();
    }

    /// Get the selected window.
    fn selected_window(&self) -> Option<&Window> {
        self.find_window(self.selected_window)
    }

    /// Get a mutable reference to the selected window.
    fn selected_window_mut(&mut self) -> Option<&mut Window> {
        self.find_window_mut(self.selected_window)
    }

    /// Replace all leaf window buffer bindings for `old_id` with `new_id`.
    fn replace_buffer_bindings(&mut self, old_id: BufferId, new_id: BufferId) {
        self.tree.replace_buffer_id(old_id, new_id);
        if let Some(minibuffer_leaf) = self.minibuffer_leaf.as_mut() {
            minibuffer_leaf.replace_buffer_id(old_id, new_id);
        }
    }

    /// Return the effective window-system symbol for this frame.
    ///
    /// GNU separates the backend output method (`FRAME_WINDOW_P`) from the
    /// Lisp `window-system` frame parameter.  The internal kind wins; the
    /// parameter is the fallback that lets frames no backend created (batch
    /// fixtures, tests) act as window-system frames when Lisp says so.
    /// [`Frame::window_system_parameter`] mirrors that one map entry so this
    /// accessor -- called dozens of times per command by geometry queries,
    /// layout inputs and redisplay signatures -- does not hash the parameter
    /// alist on every call.
    fn effective_window_system(&self) -> Option<Value> {
        self.window_system
            .filter(|value| value.is_truthy())
            .or_else(|| {
                self.window_system_parameter
                    .filter(|value| value.is_truthy())
            })
    }

    /// The default font's ascent, clamped into the frame's character cell.
    ///
    /// The clamp makes `ascent + descent == char_height` for every frame,
    /// including one whose metrics came from different sources (a test or a
    /// stub host can set `char_height` without a matching `font_ascent`), so a
    /// text-only row keeps measuring exactly one cell tall.
    fn font_cell_ascent(&self) -> f32 {
        self.font_ascent.clamp(0.0, self.char_height.max(0.0))
    }

    fn layout_inputs(&self) -> FrameLayoutInputState {
        let window_system = self.effective_window_system();
        FrameLayoutInputState {
            id: self.id,
            geometry: (
                self.width,
                self.height,
                redisplay_f32_bits(self.char_width),
                redisplay_f32_bits(self.char_height),
                redisplay_f32_bits(self.font_pixel_size),
                redisplay_f32_bits(self.font_ascent),
            ),
            device_scale_bits: self.device_scale_factor.to_bits(),
            visible: self.visibility.is_visible(),
            displays_chrome: self.displays_chrome,
            has_window_system: window_system.is_some(),
            window_system_symbol: window_system.and_then(Value::as_bare_symbol_id),
        }
    }

    /// Update the frame's internal window-system kind and keep the Lisp-visible
    /// frame parameter in sync.
    ///
    /// The field is the backend kind; the parameter is its Lisp projection.
    /// This is the only path that can retract the kind (pass `None`).
    fn set_window_system(&mut self, window_system: Option<Value>) {
        self.window_system = window_system;
        match window_system {
            Some(value) => {
                self.set_parameter(FrameQueryParam::WindowSystem.symbol(), value);
            }
            None => {
                self.remove_parameter(FrameQueryParam::WindowSystem.symbol());
            }
        }
    }

    fn frame_parameter_int(&self, key: &str) -> Option<i64> {
        self.parameter(key).and_then(|v| v.as_int())
    }

    fn known_frame_parameter_int(&self, key: FrameParam) -> Option<i64> {
        self.known_parameter(key).and_then(|v| v.as_int())
    }

    fn nonnegative_frame_parameter_int(&self, key: FrameParam) -> Option<i64> {
        self.known_frame_parameter_int(key)
            .map(|value| value.max(0))
    }

    fn child_frame_border_width_raw(&self) -> Option<i64> {
        self.nonnegative_frame_parameter_int(FrameParam::ChildFrameBorderWidth)
    }

    fn internal_border_width(&self) -> i64 {
        if self.effective_window_system().is_none() {
            return 0;
        }
        if self.parent_frame.as_frame_id().is_some()
            && let Some(width) = self.child_frame_border_width_raw()
        {
            return width;
        }
        self.nonnegative_frame_parameter_int(FrameParam::InternalBorderWidth)
            .unwrap_or(0)
    }

    fn frame_child_frame_border_width(&self) -> i64 {
        if self.effective_window_system().is_none() {
            return 0;
        }
        self.child_frame_border_width_raw()
            .unwrap_or_else(|| self.internal_border_width())
    }

    /// Divider width realized by this frame's display backend.
    ///
    /// This is deliberately distinct from the stored frame parameter.  GNU's
    /// `Fmodify_frame_parameters` retains arbitrary parameters on terminal
    /// frames, but calls the backend setters that update effective divider
    /// geometry only for window-system frames.
    fn effective_divider_width(&self, divider: FrameDivider) -> i64 {
        if self.effective_window_system().is_none() {
            return 0;
        }
        self.nonnegative_frame_parameter_int(divider.parameter())
            .unwrap_or(0)
    }

    fn outer_border_width(&self) -> i64 {
        if self.effective_window_system().is_none() {
            return 0;
        }
        self.nonnegative_frame_parameter_int(FrameParam::BorderWidth)
            .unwrap_or(0)
    }

    fn install_gnu_gui_default_parameters(&mut self) {
        // GNU GUI ports seed these through gui_default_parameter before the
        // frame's Lisp face defaults are realized.
        self.set_known_parameter(FrameParam::ForegroundColor, Value::string("black"));
        self.set_known_parameter(FrameParam::BackgroundColor, Value::string("white"));
        self.set_known_parameter(FrameParam::MouseColor, Value::string("black"));
        self.set_known_parameter(FrameParam::CursorColor, Value::string("black"));
        self.set_known_parameter(FrameParam::BorderColor, Value::string("black"));
        self.set_known_parameter(FrameParam::CursorType, Value::symbol("box"));

        // Chrome parameters that GNU's xfns.c seeds through gui_default_parameter
        // for every GUI frame. Without these, `(frame-parameter f 'vertical-scroll-bars)`
        // and friends report nil even though the layout already reserves and draws
        // the corresponding chrome — the frame parameter lies about what is on screen.
        //
        // Each value equals the fallback the layout (window/display.rs) already
        // applies for the missing parameter, so seeding them changes reporting only,
        // never geometry: GTK builds default vertical scroll bars to the right,
        // horizontal off, the standard 8px fringes, and a zero internal/outer border.
        // scroll-bar-width/height are intentionally left unset: their shared
        // fallback is GNU's toolkit default (16 px, xfns.c:2393-2438), which does
        // not follow the frame font either; `frame-parameter` reports that same
        // number for an unset frame (frame.c:5437-5456).
        self.set_known_parameter(FrameParam::VerticalScrollBars, Value::symbol("right"));
        self.set_known_parameter(FrameParam::HorizontalScrollBars, Value::NIL);
        self.set_known_parameter(FrameParam::LeftFringe, Value::fixnum(8));
        self.set_known_parameter(FrameParam::RightFringe, Value::fixnum(8));
        self.set_known_parameter(FrameParam::InternalBorderWidth, Value::fixnum(0));
        self.set_known_parameter(FrameParam::BorderWidth, Value::fixnum(0));
    }

    fn fixed_parameter(&self, key: FrameQueryParam) -> Option<Value> {
        self.parameter_key(key.into())
    }

    fn parameter(&self, key: &str) -> Option<Value> {
        self.parameters.get(&Value::symbol(key)).copied()
    }

    /// [`Self::parameter`] for a fixed GNU spelling.
    ///
    /// Same parameter-map key and the same constructor accounting; the spelling
    /// is a compile-time DEFSYM id instead of a per-read intern.
    fn parameter_symbol(&self, key: &crate::emacs_core::intern::WellKnown) -> Option<Value> {
        self.parameters.get(&Value::symbol(key)).copied()
    }

    fn set_display_identity(&mut self, identity: FrameDisplayIdentity) {
        if let Some(display) = identity.native_display() {
            self.set_parameter(Value::symbol("display"), Value::string(display));
        } else {
            self.remove_parameter(Value::symbol("display"));
        }
        self.display_identity = identity;
    }

    fn display_identity(&self) -> &FrameDisplayIdentity {
        &self.display_identity
    }

    fn known_parameter(&self, key: FrameParam) -> Option<Value> {
        self.parameters.get(&key.symbol()).copied()
    }

    fn parameter_key(&self, key: FrameParamKey) -> Option<Value> {
        self.parameters.get(&key.symbol()).copied()
    }

    fn set_parameter(&mut self, key: Value, value: Value) -> Option<Value> {
        // Only successful decoding changes the backend state. The raw value
        // remains observable even when the GUI parameter handler later errors.
        match key.as_bare_symbol_id().and_then(FrameParam::from_symbol_id) {
            Some(FrameParam::Alpha) => {
                if let Ok(alpha) = frame_alpha::pair(value) {
                    self.frame_alpha = alpha;
                }
            }
            Some(FrameParam::AlphaBackground) => {
                if let Ok(alpha) = frame_alpha::component(value, 1.0) {
                    self.background_alpha = alpha;
                }
            }
            _ => {}
        }
        if key.as_bare_symbol_id() == Some(FrameQueryParam::WindowSystem.symbol_id()) {
            self.window_system_parameter = Some(value);
        }
        self.parameters.insert(key, value)
    }

    fn set_known_parameter(&mut self, key: FrameParam, value: Value) -> Option<Value> {
        self.set_parameter(key.symbol(), value)
    }

    fn set_parameter_key(&mut self, key: FrameParamKey, value: Value) -> Option<Value> {
        self.set_parameter(key.symbol(), value)
    }

    /// Remove one Lisp-visible frame parameter.
    ///
    /// Removing `window-system` drops only the parameter fallback: an internal
    /// kind installed by [`Frame::set_window_system`] stays in force, matching
    /// GNU, where deleting the parameter cannot change `FRAME_WINDOW_P`.
    fn remove_parameter(&mut self, key: Value) -> Option<Value> {
        if key.as_bare_symbol_id() == Some(FrameQueryParam::WindowSystem.symbol_id()) {
            self.window_system_parameter = None;
        }
        self.parameters.remove(&key)
    }

    fn remove_known_parameter(&mut self, key: FrameParam) -> Option<Value> {
        self.remove_parameter(key.symbol())
    }

    fn remove_parameter_key(&mut self, key: FrameParamKey) -> Option<Value> {
        self.remove_parameter(key.symbol())
    }

    fn face_hash_table(&self) -> Value {
        self.face_hash_table
    }

    fn defer_next_gui_parameter_resize(&mut self) {
        self.defer_next_gui_parameter_resize = true;
    }

    fn should_defer_gui_parameter_resize(&self) -> bool {
        self.defer_next_gui_parameter_resize || self.pending_gui_resize.is_some()
    }

    fn queue_pending_gui_resize(
        &mut self,
        width_cols: i64,
        total_lines: i64,
        host_target: Option<(u32, u32)>,
    ) {
        self.defer_next_gui_parameter_resize = false;
        self.pending_gui_resize = Some(PendingGuiResize {
            width_cols,
            total_lines,
            delivery: host_target.map_or(GuiResizeDelivery::Queued, |(width, height)| {
                GuiResizeDelivery::Requested {
                    width,
                    height,
                    wait: true,
                }
            }),
        });
    }

    fn clear_pending_gui_resize(&mut self) {
        self.defer_next_gui_parameter_resize = false;
        self.pending_gui_resize = None;
    }

    fn gui_geometry_hints(&self) -> GuiFrameGeometryHints {
        let width_inc = self.char_width.max(1.0).round() as u32;
        let height_inc = self.char_height.max(1.0).round() as u32;
        let base_width = width_inc.saturating_add(self.horizontal_non_text_width().max(0) as u32);
        let base_height = height_inc.saturating_add(
            self.menu_bar_height
                .saturating_add(self.tool_bar_height)
                .saturating_add(self.compact_bar_height)
                .saturating_add(self.tab_bar_height),
        );
        GuiFrameGeometryHints {
            base_width,
            base_height,
            min_width: base_width,
            min_height: base_height,
            width_inc,
            height_inc,
        }
    }

    fn chrome_top_height(&self) -> f32 {
        self.menu_bar_height
            .saturating_add(self.tool_bar_height)
            .saturating_add(self.compact_bar_height)
            .saturating_add(self.tab_bar_height) as f32
    }

    fn child_frame_viewport_origin(&self) -> (f32, f32) {
        (0.0, self.chrome_top_height().min(self.height as f32))
    }

    fn default_left_fringe_width(&self) -> i64 {
        self.known_parameter(FrameParam::LeftFringe)
            .and_then(|v| v.as_int())
            .unwrap_or(8)
            .max(0)
    }

    fn default_right_fringe_width(&self) -> i64 {
        self.known_parameter(FrameParam::RightFringe)
            .and_then(|v| v.as_int())
            .unwrap_or(8)
            .max(0)
    }

    fn default_vertical_scroll_bar_side(&self) -> Option<&'static str> {
        let raw = self
            .known_parameter(FrameParam::VerticalScrollBars)
            .unwrap_or_else(|| {
                if self.effective_window_system().is_some() {
                    Value::symbol("right")
                } else {
                    Value::NIL
                }
            });
        match VerticalScrollBarType::from_symbol_value(&raw) {
            Some(side) => Some(side.name()),
            _ if raw.is_nil() => None,
            _ if raw.is_truthy() => Some("right"),
            _ => None,
        }
    }

    fn default_vertical_scroll_bar_width(&self) -> i64 {
        i64::from(display::frame_config_scroll_bar_width(self))
    }

    fn vertical_scroll_bar_area_width(&self) -> i64 {
        if self.effective_window_system().is_some()
            && self.default_vertical_scroll_bar_side().is_some()
        {
            self.default_vertical_scroll_bar_width()
        } else {
            0
        }
    }

    /// GNU `FRAME_SCROLL_BAR_AREA_HEIGHT` (src/frame.h:1342-1345): the
    /// configured horizontal scroll-bar height only while the frame enables
    /// horizontal scroll bars, zero otherwise.  This is what
    /// `frame-scroll-bar-height` returns (src/frame.c:4496-4501); the
    /// `scroll-bar-height` frame *parameter* keeps reporting the configured
    /// height even with the bars turned off (src/frame.c:5437-5456).
    fn horizontal_scroll_bar_area_height(&self) -> i64 {
        if self.effective_window_system().is_some()
            && display::frame_has_horizontal_scroll_bar(self)
        {
            i64::from(display::frame_config_scroll_bar_height(self))
        } else {
            0
        }
    }

    fn horizontal_non_text_width(&self) -> i64 {
        if self.effective_window_system().is_none() {
            return 0;
        }

        let left_fringe = self.default_left_fringe_width();
        let right_fringe = self.default_right_fringe_width();
        let scroll_bar_width = self.vertical_scroll_bar_area_width();

        left_fringe
            .saturating_add(right_fringe)
            .saturating_add(scroll_bar_width)
    }

    fn window_area_bounds_with_chrome(&self, reserve_chrome: bool) -> Rect {
        let frame_w = self.width as f32;
        let frame_h = self.height as f32;
        // The menu / tab / tool bars reduce the window text area only when they
        // are actually displayed.  GNU realizes `FRAME_MENU_BAR_LINES` (and the
        // tab/tool bars) into the frame's top margin only on a frame that is
        // being shown; a non-displayed frame (e.g. `--batch`, where the oracle
        // checks `window-edges`) keeps `frame-total-lines == frame-text-lines`
        // and the root window at line 0 even though `menu-bar-lines` is 1.
        // `displays_chrome` mirrors that: it is set on interactively displayed
        // frames and left false otherwise, so window-edge coordinates match GNU
        // in batch while interactive frames place windows below the chrome.
        let chrome_top = if reserve_chrome {
            self.chrome_top_height().min(frame_h)
        } else {
            0.0
        };
        let border = self.internal_border_width().max(0) as f32;
        let horizontal_border = border.min(frame_w / 2.0);
        let available_height = (frame_h - chrome_top).max(0.0);
        let vertical_border = border.min(available_height / 2.0);
        let content_height = (available_height - 2.0 * vertical_border).max(0.0);
        Rect::new(
            horizontal_border,
            chrome_top + vertical_border,
            (frame_w - 2.0 * horizontal_border).max(0.0),
            content_height,
        )
    }

    fn window_text_area_bounds_with_chrome(&self, reserve_chrome: bool) -> Rect {
        let mut bounds = self.window_area_bounds_with_chrome(reserve_chrome);
        let minibuffer_height = self
            .minibuffer_leaf
            .as_ref()
            .map(|mini| mini.bounds().height.max(0.0))
            .unwrap_or(0.0)
            .min(bounds.height);
        bounds.height -= minibuffer_height;
        bounds
    }

    fn window_text_area_bounds(&self) -> Rect {
        self.window_text_area_bounds_with_chrome(self.displays_chrome)
    }

    fn sync_window_area_bounds(&mut self) {
        let root_bounds = self.window_text_area_bounds();
        // Copy the cell metrics out first: the calls below borrow the tree
        // mutably, and reading `self.char_*` through that borrow is what the
        // old direct field access did not have to worry about.
        let (char_width, char_height) = (self.char_width, self.char_height);
        let root = self.tree.root_id();
        resize_window_subtree(&mut self.tree, root, root_bounds);
        sync_window_character_edges_from_bounds(&mut self.tree, root, char_width, char_height);

        self.reposition_minibuffer_below_root();
    }

    /// Lay this frame's windows out to an explicit TEXT size in characters.
    ///
    /// GNU `Fset_frame_size` calls `adjust_frame_size` for every frame.  On a
    /// terminal frame that does not resize the terminal -- `frame-width` and
    /// `frame-height` go on reporting it -- but the windows are laid out to
    /// the size asked for, and GNU does not clamp them to the terminal: asking
    /// a 80x25 batch frame for 100x40 gives a root window 39 lines tall and
    /// 100 columns wide.
    ///
    /// TEXT size follows GNU's meaning: it INCLUDES the minibuffer and
    /// EXCLUDES the top margin.  So the root takes the requested lines less
    /// the minibuffer's, sits at the margin's character row while keeping its
    /// pixel origin at the text area's top -- GNU reports `window-pixel-top` 0
    /// beside `window-top-line` 1 for exactly that reason -- and the
    /// minibuffer follows by the sum rule in
    /// [`Frame::reposition_minibuffer_below_root`].
    fn set_window_layout_text_size(&mut self, cols: i64, text_lines: i64) {
        let char_width = self.char_width.max(1.0);
        let char_height = self.char_height.max(1.0);
        let top_margin = self.frame_top_margin();
        let minibuffer_lines = i64::from(self.minibuffer_leaf.is_some());
        let root_lines = (text_lines - minibuffer_lines).max(1);
        if self.posn_object_extent_mode().enabled()
            && ((self.root_window().bounds().width / char_width).round() as i64 != cols.max(1)
                || (self.root_window().bounds().height / char_height).round() as i64 != root_lines)
        {
            self.tty_posn_pool = None;
            self.tty_posn_pool_can_repartition = false;
        }
        let root_bounds = Rect::new(
            0.0,
            top_margin as f32 * char_height,
            cols.max(1) as f32 * char_width,
            root_lines as f32 * char_height,
        );
        let root = self.tree.root_id();
        resize_window_subtree(&mut self.tree, root, root_bounds);
        self.root_window_mut().set_left_col(0);
        self.root_window_mut().set_top_line(top_margin);
        sync_window_character_edges_from_bounds(&mut self.tree, root, char_width, char_height);
        self.reposition_minibuffer_below_root();
    }

    /// Root pixel budget shared by Lisp-planned and native configuration restore.
    fn restored_window_configuration_root_bounds(&self) -> Rect {
        self.window_text_area_bounds_with_chrome(true)
    }

    /// Reconcile a restored window tree with the frame's current geometry.
    ///
    /// GNU Fset_window_configuration performs a final adjust_frame_size pass,
    /// realizing the current top margin even for an initial batch-frame tree.
    /// Ordinary initial batch geometry intentionally retains its (0, 0) root.
    fn reconcile_restored_window_configuration_geometry(&mut self) {
        let root_bounds = self.restored_window_configuration_root_bounds();
        // Everything read off `self` is gathered before the tree is borrowed
        // mutably; each was previously a direct field read that could sit
        // inside the same expression.
        let top_margin = self.frame_top_margin();
        let (char_width, char_height) = (self.char_width, self.char_height);
        let root = self.tree.root_id();
        resize_window_subtree(&mut self.tree, root, root_bounds);
        self.root_window_mut().set_left_col(0);
        self.root_window_mut().set_top_line(top_margin);
        sync_window_character_edges_from_bounds(&mut self.tree, root, char_width, char_height);
        self.reposition_minibuffer_below_root();
    }

    /// Commit the minibuffer's character origin after a root resize without
    /// deriving it again from rounded pixel geometry (GNU resize_frame_windows).
    fn sync_minibuffer_top_line_after_root_resize(&mut self) {
        let top_line =
            self.root_window().top_line() + self.root_window().total_lines(self.char_height);
        if self.minibuffer_window.is_some()
            && let Some(minibuffer) = self.minibuffer_leaf.as_mut()
        {
            minibuffer.set_top_line(top_line);
        }
    }

    fn reposition_minibuffer_below_root(&mut self) {
        let root_bounds = *self.root_window().bounds();
        // The minibuffer's character row follows its PIXEL row here.
        //
        // GNU's `m->top_line = r->top_line + r->total_lines` is NOT this
        // function's job: GNU applies that sum in exactly three places --
        // `Fwindow_resize_apply_total` (`src/window.c:5030`),
        // `resize_frame_windows` (`:5131`) and `resize_mini_window_apply`
        // (`:5874`) -- and nowhere else.  It is a resize-time assignment, not
        // an invariant maintained on every tree mutation.
        //
        // Putting the sum here instead was measurably wrong: this runs after
        // every split and delete, and GNU leaves the minibuffer's row alone
        // across those.  Creating a side window moves GNU's root to `top_line`
        // 1 while its minibuffer stays at 24, so a sum applied here reported
        // 25 where GNU says 24.  `builtin_window_resize_apply_total` carries
        // the sum, as GNU does.
        let root_left_col = self.root_window().left_col();
        let char_h = self.char_height.max(1.0);
        let mini_top_line = ((root_bounds.y + root_bounds.height) / char_h).round() as i64;
        if let Some(mini) = self.minibuffer_leaf.as_mut() {
            // The minibuffer keeps its own height.  This used to also clamp it
            // to what was left of the frame below the root, which reads as a
            // safety net and is really a divergence: GNU lets a window tree be
            // laid out larger than its terminal -- `(set-frame-size f 100 40)`
            // on an 80x25 batch frame gives a 39-line root and STILL a 1-line
            // minibuffer -- and under that clamp the minibuffer silently
            // collapsed to zero height.  GNU's own paths size the root to
            // leave room, so the clamp never fired for them either.
            let mini_h = mini.bounds().height.max(0.0);
            mini.set_bounds(Rect::new(
                root_bounds.x,
                root_bounds.y + root_bounds.height,
                root_bounds.width,
                mini_h,
            ));
            mini.set_top_line(mini_top_line);
            mini.set_left_col(root_left_col);
            mini.invalidate_display_state();
        }

        self.tree.invalidate_display_state();
        self.redisplay_cache.clear();
    }

    fn sync_tab_bar_height_from_parameters(&mut self) {
        let lines = self
            .known_frame_parameter_int(FrameParam::TabBarLines)
            .unwrap_or(0)
            .max(0) as u32;
        let char_height = self.char_height.max(1.0).round() as u32;
        self.tab_bar_height = lines.saturating_mul(char_height);
        self.sync_window_area_bounds();
    }

    /// Recompute `menu_bar_height` from the `menu-bar-lines` frame parameter.
    ///
    /// Mirrors GNU's internal menu-bar geometry. The mode supplies a default
    /// at frame creation; subsequent geometry follows the frame parameter.
    /// On TTY the menu bar takes one character row, identical to GNU's
    /// behaviour, so the resulting pixel height is `lines * char_height`
    /// where `char_height` is 1 for TTY frames.
    ///
    /// `chrome_top_height()` already adds `menu_bar_height` into the
    /// reserved top region used by `window_text_area_bounds()`, so calling
    /// `sync_window_area_bounds()` here is enough to push the root window
    /// (and its mode line / minibuffer) down to make room.
    fn sync_menu_bar_height_from_parameters(&mut self) {
        self.sync_bar_heights_from_parameters();
    }

    fn menu_bar_pixel_height_from_parameters(&self) -> u32 {
        let lines = self
            .known_frame_parameter_int(FrameParam::MenuBarLines)
            .unwrap_or(0)
            .max(0) as u32;
        let char_height = self.char_height.max(1.0).round() as u32;
        lines.saturating_mul(char_height)
    }

    /// GNU `FRAME_TOP_MARGIN(f)` (`frame.h:1132`) = `FRAME_MENU_BAR_LINES` +
    /// `FRAME_TAB_BAR_LINES` (+ tool-bar top lines) in CHARACTER lines. Unlike
    /// `chrome_top_height` (pixels, gated on `displays_chrome`), this row count
    /// applies even in batch, so a window's `top_line` sits below the menu/tab
    /// bar rows while its pixel top may be 0 (the bars have no pixel height
    /// without a display).
    fn frame_top_margin(&self) -> i64 {
        let menu = self
            .known_frame_parameter_int(FrameParam::MenuBarLines)
            .unwrap_or(0)
            .max(0);
        let tab = self
            .known_frame_parameter_int(FrameParam::TabBarLines)
            .unwrap_or(0)
            .max(0);
        menu + tab
    }

    /// Recompute `tool_bar_height` from the `tool-bar-lines` frame parameter.
    ///
    /// GNU stores `tool-bar-lines` as a row count and separately tracks the
    /// pixel height needed by toolbar images plus button margin/relief.  For
    /// GUI frames Neomacs follows that pixel model, scaled to the frame font
    /// pixels because our renderer works in physical frame pixels.
    fn sync_tool_bar_height_from_parameters(&mut self) {
        self.sync_bar_heights_from_parameters();
    }

    fn tool_bar_pixel_height_from_parameters(&self) -> u32 {
        self.gui_button_bar_pixel_height(
            self.known_frame_parameter_int(FrameParam::ToolBarLines)
                .unwrap_or(0),
        )
    }

    fn sync_compact_bar_height_from_parameters(&mut self) {
        self.sync_bar_heights_from_parameters();
    }

    fn compact_bar_pixel_height_from_parameters(&self) -> u32 {
        self.gui_button_bar_pixel_height(self.frame_parameter_int("compact-bar-lines").unwrap_or(0))
    }

    fn gui_button_bar_pixel_height(&self, lines: i64) -> u32 {
        let line_height = if self.effective_window_system().is_some() {
            default_gui_tool_bar_line_height(self.font_pixel_size)
        } else {
            self.char_height.max(1.0).round() as u32
        };
        (lines.max(0) as u32).saturating_mul(line_height)
    }

    /// Project requested frame-local bars into effective pixel geometry, then
    /// reflow once. The read-only height helpers cannot overwrite parameters;
    /// the exhaustive presentation choice cannot reserve both replacement and
    /// original bands. Global Lisp modes are deliberately absent from this API.
    fn sync_gui_bar_heights_from_parameters(&mut self, presentation: GuiBarPresentation) {
        self.gui_bar_presentation = presentation;
        self.sync_bar_heights_from_parameters();
    }

    /// Reproject bar geometry using this frame's retained presentation.
    /// Parameter and font mutations must preserve the same exclusive bands.
    fn sync_bar_heights_from_parameters(&mut self) {
        match self.gui_bar_presentation {
            GuiBarPresentation::Separate => {
                self.menu_bar_height = self.menu_bar_pixel_height_from_parameters();
                self.tool_bar_height = self.tool_bar_pixel_height_from_parameters();
                self.compact_bar_height = 0;
            }
            GuiBarPresentation::Compact => {
                self.menu_bar_height = 0;
                self.tool_bar_height = 0;
                self.compact_bar_height = self.compact_bar_pixel_height_from_parameters();
            }
        }
        self.sync_window_area_bounds();
    }

    /// Select a window by ID.
    fn select_window(&mut self, id: WindowId) -> bool {
        if self.find_window(id).is_some() {
            // GNU `Fselect_window` does NOT touch
            // `frame->old_selected_window`. That field is only
            // updated by `window_change_record`, which runs from
            // `run_window_change_functions` at redisplay time
            // (`src/window.c:3954-3990`). neomacs's analog lives
            // in `builtins/hooks.rs::frame_window_hook_record_from_live_state`
            // — it stores the new "old" inside `window_hook_record`
            // and propagates it back to `Frame::old_selected_window`
            // there. Window audit Critical 8 in
            // `drafts/window-system-audit.md`.
            self.selected_window = id;
            true
        } else {
            false
        }
    }

    /// Find a window by ID.
    fn find_window(&self, id: WindowId) -> Option<&Window> {
        if let Some(window) = self.tree.find(id) {
            return Some(window);
        }
        self.minibuffer_leaf.as_ref().and_then(|window| {
            if window.id() == id {
                Some(window)
            } else {
                None
            }
        })
    }

    /// Build one checked route per live leaf.
    ///
    /// Copying root-relative routes costs O(sum of leaf depths), which is
    /// linear for ordinary shallow split trees but can be quadratic for a
    /// deliberately degenerate tree.
    fn leaf_window_paths(&self, generation: u64) -> Vec<(WindowId, WindowTreePath)> {
        let mut paths = Vec::new();
        collect_leaf_window_paths(
            &self.tree,
            self.tree.root_id(),
            generation,
            &mut Vec::new(),
            &mut paths,
        );
        if let Some(minibuffer) = self.minibuffer_leaf.as_ref() {
            paths.push((
                minibuffer.id(),
                WindowTreePath {
                    generation,
                    target: minibuffer.id(),
                    route: WindowTreeRoute::Minibuffer,
                },
            ));
        }
        paths
    }

    /// Resolve a route captured by [`Self::leaf_window_paths`].
    fn window_at_path(&self, path: &WindowTreePath) -> Option<&Window> {
        let window = match &path.route {
            WindowTreeRoute::Minibuffer => self.minibuffer_leaf.as_ref()?,
            WindowTreeRoute::Root(route) => {
                let mut id = self.tree.root_id();
                for index in route {
                    id = *self.tree.child_ids(id).get(*index)?;
                }
                self.tree.find(id)?
            }
        };
        (window.id() == path.target && window.is_leaf()).then_some(window)
    }

    /// Find a mutable window by ID.
    /// The root of this frame's window tree.
    fn root_window(&self) -> &Window {
        self.tree.root()
    }

    /// This frame's window tree.
    ///
    /// Prefer this over [`Frame::root_window`] for anything that walks or
    /// edits more than one node: the tree is what owns the nodes, and is the
    /// only handle that can hand out one node at a time.
    fn tree(&self) -> &WindowTree {
        &self.tree
    }

    /// Mutable counterpart of [`Frame::tree`].
    fn tree_mut(&mut self) -> &mut WindowTree {
        &mut self.tree
    }

    /// Replace this frame's window tree wholesale, as
    /// `set-window-configuration` does.
    fn set_tree(&mut self, tree: WindowTree) {
        self.tree = tree;
    }

    fn root_window_mut(&mut self) -> &mut Window {
        self.tree.root_mut()
    }

    /// The leaf windows of the root subtree, WITHOUT the minibuffer.
    ///
    /// The companion to [`Frame::all_leaf_ids`], and the distinction is real:
    /// layout collects the root's leaves because the minibuffer is positioned
    /// separately, while hscroll wants every leaf including it.  Naming both
    /// keeps a caller from expressing the choice by reaching into
    /// `root_window` and thereby depending on how the tree is stored.
    fn root_leaf_ids(&self) -> Vec<WindowId> {
        self.tree.leaf_ids()
    }

    /// Every leaf window on this frame, the minibuffer included.
    ///
    /// `WindowTree::leaf_ids` walks the root tree, so a caller that wants the
    /// frame's leaves has to append the minibuffer leaf itself -- which is
    /// duplicated knowledge of where the minibuffer lives, and the same
    /// duplication `find_window` already absorbs for lookups.
    fn all_leaf_ids(&self) -> Vec<WindowId> {
        let mut ids = self.tree.leaf_ids();
        if let Some(mini) = &self.minibuffer_leaf {
            ids.push(mini.id());
        }
        ids
    }

    fn find_window_mut(&mut self, id: WindowId) -> Option<&mut Window> {
        // `self.tree` rather than `self.root_window_mut()`: the accessor
        // borrows all of `self`, which would keep the tree borrow alive across
        // the `minibuffer_leaf` arm below.  Naming the field lets the borrow
        // checker split the two, exactly as the old direct field access did.
        if let Some(window) = self.tree.find_mut(id) {
            return Some(window);
        }
        self.minibuffer_leaf.as_mut().and_then(|window| {
            if window.id() == id {
                Some(window)
            } else {
                None
            }
        })
    }

    /// All leaf window IDs.
    fn window_list(&self) -> Vec<WindowId> {
        self.tree.leaf_ids()
    }

    fn live_window_ids_with_minibuffer(&self) -> Vec<WindowId> {
        let mut ids = self.window_list();
        if let Some(minibuffer_leaf) = self.minibuffer_leaf.as_ref() {
            ids.push(minibuffer_leaf.id());
        }
        ids
    }

    /// Number of visible windows (leaves).
    fn window_count(&self) -> usize {
        self.tree.leaf_count()
    }

    /// Find which window is at pixel coordinates.
    fn window_at(&self, px: f32, py: f32) -> Option<WindowId> {
        self.tree.window_at(px, py)
    }

    /// Columns (based on default char width).
    fn columns(&self) -> u32 {
        (self.width as f32 / self.char_width) as u32
    }

    /// Lines (based on default char height).
    fn lines(&self) -> u32 {
        (self.height as f32 / self.char_height) as u32
    }

    /// Test fixture for installing a completed redisplay and replaying its live
    /// output state. Production redisplay uses the presentation transaction.
    #[cfg(test)]
    fn commit_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>) {
        let generation = self
            .presentation_state
            .last_identity
            .unwrap_or_else(|| geometry::PresentationId::new(1));
        let publications: Vec<_> = snapshots
            .iter()
            .cloned()
            .map(WindowPresentationSnapshot::live)
            .collect();
        self.commit_completed_window_output(generation, &publications);
        self.replace_redisplay_cache_for_test(snapshots);
    }

    /// Test fixture for the synchronous prepare/activate lifecycle.
    #[cfg(test)]
    fn prepare_and_activate_display_presentation_for_test(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError> {
        self.prepare_live_window_presentation(presentation, snapshots)?;
        self.activate_display_presentation(presentation)
            .expect("a successfully prepared presentation must activate");
        Ok(())
    }

    /// Prepare an immutable display publication without making it visible to
    /// evaluator geometry queries. The renderer owns the later activation.
    fn prepare_live_window_presentation(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError> {
        self.prepare_display_presentation(
            presentation,
            snapshots
                .into_iter()
                .map(WindowPresentationSnapshot::live)
                .collect(),
        )
    }

    /// Prepare renderer geometry while admitting only live-window snapshots
    /// to evaluator-visible output and redisplay caches.
    fn prepare_display_presentation(
        &mut self,
        presentation: geometry::PresentationId,
        publications: Vec<WindowPresentationSnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError> {
        self.prepare_display_presentation_with_tty_posn_pool(presentation, publications, None)
    }

    /// Admit a completed producer's pool with the same validated publication.
    /// Numeric state is owned by this exclusive Frame mutator; readers never
    /// mutate the Arc, and rejected/reused presentations cannot replace it.
    fn prepare_display_presentation_with_tty_posn_pool(
        &mut self,
        presentation: geometry::PresentationId,
        publications: Vec<WindowPresentationSnapshot>,
        pool: Option<Arc<neomacs_display_protocol::posn_frame_pool::PosnFramePool>>,
    ) -> Result<(), geometry::PresentationPrepareError> {
        let publications: Vec<_> = publications
            .into_iter()
            .filter(|publication| self.find_window(publication.window_id()).is_some())
            .collect();
        let snapshots: Vec<std::sync::Arc<WindowDisplaySnapshot>> = publications
            .iter()
            .map(|publication| publication.shared_display_snapshot().clone())
            .collect();
        let candidate = geometry::PresentationGeometry::new_with_frame_placement(
            self.id,
            presentation,
            self.parent_frame.as_frame_id().map(FrameId),
            self.left_pos,
            self.top_pos,
            self.width,
            self.height,
            self.z_order,
            snapshots.clone(),
        )
        .map_err(geometry::PresentationPrepareError::InvalidGeometry)?;
        let prepared = PreparedDisplayPresentation {
            geometry: Arc::new(candidate),
            publications,
        };
        if self
            .presentation_state
            .last_identity
            .is_some_and(|last| presentation <= last)
        {
            if self.presentation_state.active.as_ref() == Some(&prepared)
                || self.presentation_state.prepared.get(&presentation) == Some(&prepared)
            {
                return Ok(());
            }
            return Err(geometry::PresentationPrepareError::ReusedPresentation(
                presentation,
            ));
        }
        if self.posn_object_extent_mode().enabled() && self.effective_window_system().is_none() {
            self.note_tty_current_matrix_publication(&prepared.publications, pool);
        }
        self.commit_completed_window_output(presentation, &prepared.publications);
        let geometry_only_windows: HashSet<_> = prepared
            .publications
            .iter()
            .filter(|publication| publication.live_window_snapshot().is_none())
            .map(WindowPresentationSnapshot::window_id)
            .collect();
        self.redisplay_cache
            .retain(|window_id, _| geometry_only_windows.contains(window_id));
        self.redisplay_cache.extend(
            prepared
                .publications
                .iter()
                .filter_map(WindowPresentationSnapshot::shared_live_window_snapshot)
                .cloned()
                .map(|snapshot| (snapshot.window_id, snapshot)),
        );
        self.presentation_state.completed_geometry = Some(Arc::clone(&prepared.geometry));
        self.presentation_state
            .prepared
            .insert(presentation, prepared);
        self.presentation_state.last_identity = Some(presentation);
        Ok(())
    }

    /// Activate a renderer-confirmed presentation and return the identity it
    /// replaced. Re-activating the current presentation is idempotent.
    fn activate_display_presentation(
        &mut self,
        presentation: geometry::PresentationId,
    ) -> Result<Option<geometry::PresentationId>, geometry::PresentationActivateError> {
        if self
            .presentation_state
            .active
            .as_ref()
            .is_some_and(|active| active.geometry.presentation() == presentation)
        {
            return Ok(None);
        }
        let prepared = self
            .presentation_state
            .prepared
            .remove(&presentation)
            .ok_or(geometry::PresentationActivateError::UnknownPresentation(
                presentation,
            ))?;
        let replaced = self
            .presentation_state
            .active
            .replace(prepared)
            .map(|active| active.geometry.presentation());
        Ok(replaced)
    }

    /// Discard a presentation that never became renderer-visible.
    /// Its accepted window output and completed geometry remain Lisp redisplay
    /// evidence until another completed layout replaces them.
    fn discard_display_presentation(&mut self, presentation: geometry::PresentationId) -> bool {
        self.presentation_state
            .prepared
            .remove(&presentation)
            .is_some()
    }

    fn retire_display_presentation(&mut self, presentation: geometry::PresentationId) -> bool {
        if self
            .presentation_state
            .active
            .as_ref()
            .is_some_and(|active| active.geometry.presentation() == presentation)
        {
            self.presentation_state.active = None;
            true
        } else {
            false
        }
    }

    fn is_display_presentation_prepared(&self, presentation: geometry::PresentationId) -> bool {
        self.presentation_state.prepared.contains_key(&presentation)
    }

    fn has_prepared_display_presentations(&self) -> bool {
        !self.presentation_state.prepared.is_empty()
    }

    fn active_presentation(&self) -> Option<geometry::PresentationId> {
        match &self.presentation_state.active {
            Some(active) => Some(active.geometry.presentation()),
            _ => None,
        }
    }

    /// Geometry for the presentation currently used by renderer drawing and
    /// hit testing. Prepared geometry is deliberately inaccessible here.
    fn active_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry> {
        match &self.presentation_state.active {
            Some(active) => Some(active.geometry.as_ref()),
            None => None,
        }
    }

    /// Geometry from the latest completed redisplay, even before renderer
    /// acknowledgement. Lisp motion queries may use this only after checking
    /// the live window's snapshot freshness. Native hit testing uses active
    /// or event-captured presentation geometry instead.
    fn completed_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry> {
        self.presentation_state.completed_geometry.as_deref()
    }

    /// Typed publication for WINDOW in the renderer-active presentation.
    ///
    /// Rectangles are available through either variant, but callers that want
    /// semantic evidence about the live buffer must explicitly match
    /// [`WindowPresentationSnapshot::LiveWindow`]. Keeping the domain attached
    /// to the snapshot prevents a hit and an unrelated boolean from drifting
    /// apart as they pass through interaction code.
    fn active_window_presentation(&self, window: WindowId) -> Option<&WindowPresentationSnapshot> {
        self.presentation_state
            .active
            .as_ref()?
            .publications
            .iter()
            .find(|publication| publication.window_id() == window)
    }

    /// Resolve an exact visual anchor only from renderer-active geometry.
    fn resolve_active_visual_anchor(
        &self,
        query: geometry::VisualAnchorQuery,
    ) -> Result<geometry::VisualAnchorGeometry, geometry::GeometryQueryError> {
        let geometry = self
            .active_presentation_geometry()
            .ok_or(geometry::GeometryQueryError::NotYetActive { frame: self.id })?;
        geometry.resolve(query)
    }

    /// Begin a GNU-shaped output pass for all live windows on this frame.
    fn begin_display_output_pass(&mut self) {
        let live_window_ids = self.live_window_ids_with_minibuffer();
        for wid in &live_window_ids {
            if let Some(window) = self.find_window_mut(*wid)
                && let Some(display) = window.display_mut()
            {
                display.begin_output_pass();
            }
        }
    }

    fn window_output_update(&mut self, window_id: WindowId) -> Option<WindowOutputUpdate<'_>> {
        let display = self.find_window_mut(window_id)?.display_mut()?;
        Some(WindowOutputUpdate::new(display))
    }

    /// Replay a completed window snapshot through the live output lifecycle.
    fn replay_window_output_snapshot(&mut self, snapshot: &WindowDisplaySnapshot) {
        if let Some(mut update) = self.window_output_update(snapshot.window_id) {
            update.replay_snapshot(snapshot);
        }
    }

    /// Commit one accepted redisplay attempt into GNU-shaped live per-window
    /// output state. Speculative layout never calls this; all window cursors
    /// and output progress become visible together at the presentation prepare
    /// boundary.
    fn commit_completed_window_output(
        &mut self,
        generation: geometry::PresentationId,
        publications: &[WindowPresentationSnapshot],
    ) {
        self.begin_display_output_pass();
        for snapshot in publications
            .iter()
            .filter_map(WindowPresentationSnapshot::live_window_snapshot)
        {
            self.replay_window_output_snapshot(snapshot);
            let Some(window_end) = snapshot.window_end_record else {
                continue;
            };
            if let Some(window) = self.find_window_mut(snapshot.window_id) {
                window.accept_redisplay_output(window_redisplay_output_from_snapshot(
                    generation, snapshot, window_end,
                ));
            }
        }
    }

    /// Test fixture for replacing only the latest redisplay cache, without
    /// mutating live cursor/output state.
    #[cfg(test)]
    fn replace_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>) {
        self.redisplay_cache = snapshots
            .into_iter()
            .filter(|snapshot| self.find_window(snapshot.window_id).is_some())
            .map(|snapshot| (snapshot.window_id, std::sync::Arc::new(snapshot)))
            .collect();
    }

    /// Latest completed redisplay output for WINDOW-ID, independent of which
    /// presentation is renderer-active.
    fn redisplay_snapshot(&self, id: WindowId) -> Option<&WindowDisplaySnapshot> {
        self.redisplay_cache.get(&id).map(|snapshot| &**snapshot)
    }

    /// GNU checks the entire live tree, including the minibuffer, before
    /// preserving a pool through topology changes. This borrowed walk allocates
    /// no leaf list and does not treat detached windows as live partitions.
    fn tty_posn_live_margins_clear(&self) -> bool {
        fn clear(tree: &WindowTree, id: WindowId) -> bool {
            match tree.find(id) {
                Some(Window::Leaf { margins, .. }) => *margins == WindowMargins::ZERO,
                Some(Window::Internal { children, .. }) => {
                    children.iter().all(|id| clear(tree, *id))
                }
                _ => false,
            }
        }
        clear(&self.tree, self.tree.root_id()) && self.minibuffer_leaf.as_ref().is_none_or(|leaf| {
            matches!(leaf, Window::Leaf { margins, .. } if *margins == WindowMargins::ZERO)
        })
    }

    /// Accepted numeric producer input for the next speculative TTY capture.
    /// Callers must already have the process policy ON; this read is inert.
    fn tty_posn_pool(
        &self,
    ) -> Option<&Arc<neomacs_display_protocol::posn_frame_pool::PosnFramePool>> {
        self.tty_posn_pool.as_ref()?.pool.as_ref()
    }

    /// GNU current-matrix lookup before after-EOL iterator column advancement.
    /// A local accepted matrix stays authoritative. Without one, a new leaf
    /// can still slice the unchanged accepted frame pool (fake_current_matrices).
    /// Pure numeric read: no allocation, Arc cloning, or source-row publication.
    fn retained_tty_posn_extent(
        &self,
        id: WindowId,
        row: i64,
        column: i64,
    ) -> neomacs_display_protocol::posn_object_extent::PosnObjectExtent {
        use crate::window::tty_posn_current::TtyCurrentMatrixAuthority;
        use neomacs_display_protocol::glyph_matrix::GlyphArea;
        use neomacs_display_protocol::posn_object_extent::PosnObjectExtent;
        let authority = self.tty_posn_current_matrix_authority(id);
        if authority == Some(TtyCurrentMatrixAuthority::Undrawn) {
            return PosnObjectExtent::Undrawn;
        }
        if authority != Some(TtyCurrentMatrixAuthority::FramePoolPartition)
            && let Some(snapshot) = self
                .redisplay_snapshot(id)
                .filter(|snapshot| snapshot.posn_matrix.is_some())
        {
            return retained_posn_extent(Some(snapshot), row, column, GlyphArea::Text);
        }
        if !self.tty_posn_pool_can_repartition || !self.tty_posn_live_margins_clear() {
            return PosnObjectExtent::Undrawn;
        }
        let Some(pool) = self.tty_posn_pool() else {
            return PosnObjectExtent::Undrawn;
        };
        if pool.columns as i64 != (self.width as f32 / self.char_width.max(1.0)).round() as i64
            || pool.lines as i64 != (self.height as f32 / self.char_height.max(1.0)).round() as i64
        {
            return PosnObjectExtent::Undrawn;
        }
        let Some(window) = self.find_window(id) else {
            return PosnObjectExtent::Undrawn;
        };
        let Window::Leaf { margins, .. } = window else {
            return PosnObjectExtent::Undrawn;
        };
        // GNU's unchanged-pool topology preservation is disabled for margins.
        // A local accepted margin matrix above is still exact; no such matrix
        // means this path must not infer its current rows from painted pixels.
        if *margins != WindowMargins::ZERO {
            return PosnObjectExtent::Undrawn;
        }
        let width = usize::try_from(window.total_columns(self.char_width)).unwrap_or(0);
        let height = usize::try_from(window.total_lines(self.char_height)).unwrap_or(0);
        pool.at_partition(
            window.top_line(),
            height,
            [window.left_col(); GlyphArea::COUNT],
            [0, width, 0],
            row,
            column,
            GlyphArea::Text,
        )
    }

    /// GNU `coordinates_in_window`'s inputs for one live window of this frame
    /// (src/window.c:1348-1489).
    ///
    /// The chrome heights and the horizontal box come from the window's last
    /// redisplay, which is GNU's `CURRENT_MODE_LINE_HEIGHT` and
    /// `window_box_left_offset` reading `w->current_matrix` and the window's
    /// own margin/fringe settings. A window that has never been redisplayed
    /// contributes zeroes, which is GNU's answer too: an unfilled matrix has no
    /// chrome rows.
    fn window_part_geometry(&self, id: WindowId) -> Option<WindowPartGeometry> {
        self.window_part_geometry_from(id, self.redisplay_snapshot(id))
    }

    /// [`Self::window_part_geometry`] against a snapshot the caller supplies.
    ///
    /// Ledger 201 gave this port an on-demand row producer for the case GNU
    /// covers by running an iterator inside `buffer_posn_from_coords`: a window
    /// redisplay has not drawn yet. The classification has to see the SAME
    /// window that answers -- a header line the recomputation drew but the
    /// retained matrix has not got would otherwise shift every text
    /// coordinate below it by one row.
    fn window_part_geometry_from(
        &self,
        id: WindowId,
        snapshot: Option<&WindowDisplaySnapshot>,
    ) -> Option<WindowPartGeometry> {
        let window = self.find_window(id)?;
        let bounds = *window.bounds();
        let (tab_line_height, header_line_height, mode_line_height, text_area_left_offset) =
            snapshot.map_or((0, 0, 0, 0), |snapshot| {
                (
                    snapshot.tab_line_height,
                    snapshot.header_line_height,
                    snapshot.mode_line_height,
                    snapshot.text_area_left_offset,
                )
            });
        // GNU takes the horizontal box off the WINDOW, not off the matrix:
        // `window_box_width (w, TEXT_AREA)` subtracts the fringes, margins,
        // scroll bars and dividers from `w->pixel_width` (src/window.c:1439-1441).
        // This port publishes the left part of that subtraction as
        // `text_area_left_offset` and nothing on the right, which is exact on a
        // terminal frame: it has no fringes, no scroll bars and no dividers, so
        // everything between the window's left edge and its text is the LEFT
        // MARGIN and the text runs to the window's own right edge.
        let terminal = self.effective_window_system().is_none();
        let text_area_width = (bounds.width.round() as i64 - text_area_left_offset).max(0);
        Some(WindowPartGeometry::new(
            bounds,
            tab_line_height,
            header_line_height,
            mode_line_height,
            text_area_left_offset,
            text_area_width,
            if terminal { text_area_left_offset } else { 0 },
            0,
            self.char_width.max(1.0).round() as i64,
            bounds.right().round() as i64 >= self.width as i64,
        ))
    }

    /// GNU `window_from_coordinates` (src/window.c:1686-1750): the first window
    /// of this frame whose `coordinates_in_window` is not `ON_NOTHING`, with
    /// the part it landed on.
    ///
    /// The order is GNU's `foreach_window`: the root window's leaves in tree
    /// order, then the minibuffer window, which is the root's `next` sibling in
    /// GNU's tree (`make_frame` links them, src/frame.c) and so is visited by
    /// the same walk. That is why a Y one row below a window with no mode line
    /// answers the MINIBUFFER's position rather than nothing.
    fn coordinate_hit(&self, x: i64, y: i64) -> Option<FrameCoordinateHit> {
        self.coordinate_hit_with(x, y, None)
    }

    /// [`Self::coordinate_hit`] with one window's geometry taken from a
    /// snapshot the caller has just produced rather than from the retained one.
    fn coordinate_hit_with(
        &self,
        x: i64,
        y: i64,
        recomputed: Option<(WindowId, &WindowDisplaySnapshot)>,
    ) -> Option<FrameCoordinateHit> {
        for id in self.live_window_ids_with_minibuffer() {
            let snapshot = match recomputed {
                Some((recomputed_id, snapshot)) if recomputed_id == id => Some(snapshot),
                _ => self.redisplay_snapshot(id),
            };
            let Some(geometry) = self.window_part_geometry_from(id, snapshot) else {
                continue;
            };
            if let Some(coordinate) = geometry.resolve(x, y) {
                return Some(FrameCoordinateHit {
                    window: id,
                    geometry,
                    coordinate,
                });
            }
        }
        None
    }

    fn remove_redisplay_snapshot(&mut self, id: WindowId) {
        self.redisplay_cache.remove(&id);
    }

    /// Resize the frame and window tree to new pixel dimensions.
    fn resize_pixelwise(&mut self, width: u32, height: u32) {
        if self.posn_object_extent_mode().enabled()
            && (self.width != width || self.height != height)
        {
            self.tty_posn_pool = None;
            self.tty_posn_pool_can_repartition = false;
        }
        let horizontal_geometry_changed = self.width != width;
        // GNU resize_frame_windows resets the ordinary minibuffer's text
        // body to one line on a vertical frame resize, before redisplay
        // regrows it through window.el's root minimum policy. Preserve the
        // nontext geometry published by its current matrix.
        let resized_mini_height = (self.height != height)
            .then(|| {
                self.minibuffer_leaf.as_ref().map(|mini| {
                    let nontext = self.redisplay_snapshot(mini.id()).map_or(0.0, |snapshot| {
                        if snapshot.regions_materialized {
                            (snapshot.regions.outer.height - snapshot.regions.text_body.height)
                                .max(0.0)
                        } else {
                            (snapshot.tab_line_height.max(0)
                                + snapshot.header_line_height.max(0)
                                + snapshot.mode_line_height.max(0))
                                as f32
                        }
                    });
                    self.char_height.max(1.0) + nontext
                })
            })
            .flatten();
        self.clear_pending_gui_resize();
        self.width = width;
        self.height = height;
        if let (Some(mini), Some(height)) = (self.minibuffer_leaf.as_mut(), resized_mini_height) {
            let mut bounds = *mini.bounds();
            bounds.height = height;
            mini.set_bounds(bounds);
        }
        self.sync_window_area_bounds();
        if horizontal_geometry_changed {
            self.tree.invalidate_automatic_hscroll_for_geometry_change();
            if let Some(minibuffer) = self.minibuffer_leaf.as_mut() {
                minibuffer.invalidate_automatic_hscroll_for_geometry_change();
            }
        }

        let char_width = self.char_width.max(1.0).round();
        let char_height = self.char_height.max(1.0).round();
        let text_width = (i64::from(width) - self.horizontal_non_text_width()).max(1) as f32;
        let cols = (text_width / char_width).floor().max(1.0) as i64;
        self.set_parameter(Value::symbol("width"), Value::fixnum(cols));

        if self.effective_window_system().is_none() && self.parent_frame.as_frame_id().is_none() {
            // Top-level terminal frame. Mirror GNU frame.c line accounting:
            // FRAME_TOTAL_LINES is the whole terminal, and the 'height'
            // parameter is FRAME_LINES = FRAME_TOTAL_LINES - FRAME_MENU_BAR_LINES
            // - FRAME_TAB_BAR_LINES (the minibuffer is INCLUDED in FRAME_LINES).
            // Derive the menu/tab reservation from the LINE parameters
            // (`frame_top_margin`) rather than the transient `menu_bar_height`
            // pixel cache, which may still be 0 when a resize event lands before
            // the next redisplay syncs it. Only realized (displayed) chrome
            // reduces FRAME_LINES; a non-displayed frame (--batch) keeps
            // FRAME_TOTAL_LINES == FRAME_LINES, matching GNU's batch geometry.
            let total_terminal_lines = (self.height as f32 / char_height).floor().max(1.0) as i64;
            let top_margin = if self.displays_chrome {
                self.frame_top_margin()
            } else {
                0
            };
            let frame_lines = (total_terminal_lines - top_margin).max(1);
            let minibuffer_lines = i64::from(self.minibuffer_leaf.is_some());
            let text_lines = (frame_lines - minibuffer_lines).max(1);
            self.set_parameter(Value::symbol("height"), Value::fixnum(frame_lines));
            self.set_parameter(
                Value::symbol("neovm--frame-total-lines"),
                Value::fixnum(total_terminal_lines),
            );
            self.set_parameter(
                Value::symbol("neovm--frame-text-lines"),
                Value::fixnum(text_lines),
            );
        } else {
            let root_height = self.root_window().bounds().height;
            let text_lines = (root_height / char_height).floor().max(1.0) as i64;
            // GNU FRAME_LINES includes the entire minibuffer allocation. A
            // grown minibuffer redistributes the text area without losing rows.
            let mini_height = self
                .minibuffer_leaf
                .as_ref()
                .map_or(0.0, |mini| mini.bounds().height);
            let total_lines = ((root_height + mini_height) / char_height).floor().max(1.0) as i64;
            self.set_parameter(Value::symbol("height"), Value::fixnum(total_lines));
            self.set_parameter(
                Value::symbol("neovm--frame-text-lines"),
                Value::fixnum(text_lines),
            );
        }
    }

    /// Refresh fixed-size constraints from buffers currently displayed in the
    /// window tree.
    ///
    /// GNU's `window-size-fixed-p` reads the buffer-local `window-size-fixed`
    /// of a live window's buffer. Display backends only report physical frame
    /// sizes, so Neomacs materializes that dynamic Lisp state onto the window
    /// tree immediately before low-level frame resize layout.
    fn sync_window_size_fixed_from_buffers(&mut self, buffers: &BufferManager) {
        let char_width = self.char_width.max(1.0);
        let char_height = self.char_height.max(1.0);

        fn fixed_axes(buffers: &BufferManager, buffer_id: BufferId) -> (bool, bool) {
            let value = buffers
                .get(buffer_id)
                .and_then(|buffer| buffer.buffer_local_value("window-size-fixed"))
                .unwrap_or(Value::NIL);
            if value.is_nil() {
                return (false, false);
            }
            if value == Value::T {
                return (true, true);
            }
            match value.as_bare_symbol_name() {
                Some("width") => (true, false),
                Some("height") => (false, true),
                _ => (true, true),
            }
        }

        // Only leaves show a buffer, so only leaves can be fixed; an internal
        // window's fixed size is a property of what it contains, computed by
        // layout rather than stored here.
        fn sync_leaf(
            window: &mut Window,
            buffers: &BufferManager,
            char_width: f32,
            char_height: f32,
        ) {
            let Window::Leaf {
                buffer_id, bounds, ..
            } = window
            else {
                return;
            };
            let (fixed_width, fixed_height) = fixed_axes(buffers, *buffer_id);
            let fixed_cols = if fixed_width {
                (bounds.width / char_width).round().max(1.0) as usize
            } else {
                0
            };
            let fixed_lines = if fixed_height {
                (bounds.height / char_height).round().max(1.0) as usize
            } else {
                0
            };
            window.set_fixed_width_cols(fixed_cols);
            window.set_fixed_height_lines(fixed_lines);
        }

        self.tree
            .for_each_leaf_mut(|leaf| sync_leaf(leaf, buffers, char_width, char_height));
        if let Some(minibuffer) = self.minibuffer_leaf.as_mut() {
            sync_leaf(minibuffer, buffers, char_width, char_height);
        }
    }

    /// Recompute character/window geometry at the current pixel allocation
    /// after a font change. This local refresh acknowledges no native request;
    /// outstanding explicit resize intent must remain available to flush/wait.
    fn refresh_geometry_for_changed_font(&mut self, buffers: &BufferManager) {
        let pending = self.pending_gui_resize.take();
        let defer = self.defer_next_gui_parameter_resize;
        self.resize_pixelwise_with_buffer_constraints(buffers, self.width, self.height);
        self.pending_gui_resize = pending;
        self.defer_next_gui_parameter_resize = defer;
    }

    /// Apply a live physical frame resize while honoring Lisp-visible window
    /// constraints carried by displayed buffers.
    fn resize_pixelwise_with_buffer_constraints(
        &mut self,
        buffers: &BufferManager,
        width: u32,
        height: u32,
    ) {
        self.sync_window_size_fixed_from_buffers(buffers);
        self.resize_pixelwise(width, height);
    }

    /// Grow the minibuffer window by `delta_rows` character-cell rows.
    ///
    /// Mirrors GNU `grow_mini_window` at `src/window.c:5896-5930`.
    /// The minibuffer height is clamped to the range [1 row,
    /// `max-mini-window-height` fraction of frame inner height].
    /// After adjusting the minibuffer bounds,
    /// `sync_window_area_bounds` propagates the change to the root
    /// window tree (the root shrinks by the same delta).
    fn grow_mini_window(&mut self, delta_rows: i32) {
        // The default `max-mini-window-height` is the float 0.25 (a fraction of
        // the frame's inner height). Resolve it to a line count so it matches
        // the lines-only contract of `*_with_max_lines`.
        let char_h = self.char_height.max(1.0);
        let frame_inner_rows = ((self.height as f32 - self.chrome_top_height()) / char_h).max(1.0);
        self.grow_mini_window_with_max_lines(delta_rows, 0.25 * frame_inner_rows);
    }

    /// Grow the minibuffer window using GNU's `max-mini-window-height`
    /// semantics resolved by the caller.
    ///
    /// `max_lines` is either an absolute line count or a frame-height
    /// fraction already converted into lines.
    fn grow_mini_window_with_max_lines(&mut self, delta_rows: i32, max_lines: f32) {
        let Some(mini) = self.minibuffer_leaf.as_ref() else {
            return;
        };
        let unit = self.char_height.max(1.0);
        let current_rows = mini_window_rows(mini.bounds().height, unit) as f32;
        if let Some(resize) =
            self.plan_mini_window_resize((current_rows + delta_rows as f32) * unit, max_lines)
        {
            self.apply_mini_window_resize(resize);
        }
    }

    /// Plan an absolute content-height resize without changing live geometry.
    ///
    /// GNU `resize_mini_window` compares measured pixels, rounding down to a
    /// line multiple only when content exceeds the maximum (xdisp.c:13361).
    /// A missing plan means the achievable allocation already matches. Do not
    /// turn pixels into row deltas: stale font sizes and fractional rows lose
    /// information in that conversion and can request endless no-op retries.
    fn plan_mini_window_resize(
        &self,
        content_height_px: f32,
        max_lines: f32,
    ) -> Option<MiniWindowResize> {
        let mini = self.minibuffer_leaf.as_ref()?;
        if !content_height_px.is_finite() {
            return None;
        }
        let unit = self.char_height.max(1.0);
        let inner_height = self
            .window_area_bounds_with_chrome(self.displays_chrome)
            .height;
        let maximum = (max_lines * unit).max(unit).min(inner_height);
        let requested = content_height_px.max(unit);
        let height = if requested > maximum {
            mini_window_rows(maximum, unit).max(1) as f32 * unit
        } else {
            requested
        }
        .min(inner_height);
        let previous_height = mini.bounds().height;
        (height != previous_height).then_some(MiniWindowResize {
            frame: self.id,
            window: mini.id(),
            previous_height,
            height,
        })
    }

    /// Apply a plan before any intervening geometry mutation. Planning lets
    /// redisplay reject its speculative rows before resizing the live frame.
    fn apply_mini_window_resize(&mut self, resize: MiniWindowResize) {
        assert_eq!(
            resize.frame, self.id,
            "minibuffer resize belongs to this frame"
        );
        let mini = self
            .minibuffer_leaf
            .as_mut()
            .expect("planned minibuffer is live");
        assert_eq!(
            resize.window,
            mini.id(),
            "planned minibuffer identity is unchanged"
        );
        assert_eq!(
            resize.previous_height,
            mini.bounds().height,
            "resize plan is fresh"
        );
        let mut bounds = *mini.bounds();
        bounds.height = resize.height;
        mini.set_bounds(bounds);
        self.sync_window_area_bounds();
    }

    /// Shrink the minibuffer window to its minimum height (1 row).
    ///
    /// Mirrors GNU `shrink_mini_window` at `src/window.c:5938-5960`.
    /// The freed space is returned to the root window via
    /// `sync_window_area_bounds`.
    fn shrink_mini_window(&mut self) {
        if let Some(resize) = self.plan_mini_window_resize(self.char_height, 1.0) {
            self.apply_mini_window_resize(resize);
        }
    }

    /// Canonical projection of every live-window input that can affect rows.
    ///
    /// Keeping effective margins, fringes, and scroll-bar partitions here
    /// makes redisplay skipping and retained-row freshness share one source of
    /// truth, including frame-default values inherited by a window.
    fn window_layout_inputs(&self, window_id: WindowId) -> Option<WindowLayoutInputState> {
        let window = self.find_window(window_id)?;
        let state = window.redisplay_state()?;
        let Window::Leaf {
            margins, display, ..
        } = window
        else {
            return None;
        };
        let is_window_system = self.effective_window_system().is_some();
        let left_fringe_width = if is_window_system {
            if display.left_fringe_width >= 0 {
                display.left_fringe_width
            } else {
                frame_default_left_fringe_width(self)
            }
        } else {
            0
        };
        let right_fringe_width = if is_window_system {
            if display.right_fringe_width >= 0 {
                display.right_fringe_width
            } else {
                frame_default_right_fringe_width(self)
            }
        } else {
            0
        };
        let is_minibuffer = self.minibuffer_window == Some(window_id);
        let scroll_bars = resolve_window_scroll_bar_geometry(self, display, is_minibuffer);

        Some(WindowLayoutInputState {
            id: state.id,
            buffer_id: state.buffer_id,
            bounds: state.bounds,
            window_start: state.window_start,
            point: state.point,
            hscroll: state.hscroll,
            vscroll: state.vscroll,
            preserve_vscroll_p: state.preserve_vscroll_p,
            margins: *margins,
            display_table_identity: display.display_table.bits(),
            char_table_mutation_epoch: current_char_table_layout_revision(),
            window_parameters_generation: window.parameters_generation(),
            left_fringe_width,
            right_fringe_width,
            fringes_outside_margins: display.fringes_outside_margins,
            left_scroll_bar_width: scroll_bars.left_area_width,
            right_scroll_bar_width: scroll_bars.right_area_width,
            horizontal_scroll_bar_height: scroll_bars.horizontal_area_height,
        })
    }

    /// Successful current producer admission; geometry-only snapshots retain
    /// prior rows. Caller has already checked extent ON, TTY and prepare identity.
    #[cold]
    #[inline(never)]
    fn note_tty_current_matrix_publication(
        &mut self,
        publications: &[WindowPresentationSnapshot],
        pool: Option<Arc<PosnFramePool>>,
    ) {
        let current_windows: Vec<_> = publications
            .iter()
            .filter_map(WindowPresentationSnapshot::live_window_snapshot)
            .map(|snapshot| (snapshot.window_id, snapshot.posn_matrix.is_some()))
            .collect();
        if current_windows.is_empty() {
            return;
        }
        let (allocation, dimensions) = self.tty_current_matrix_allocation();
        let pool_present = pool.is_some();
        if pool_present {
            self.tty_posn_pool_can_repartition = self.tty_posn_live_margins_clear();
        } else {
            self.tty_posn_pool_can_repartition = false;
        }
        let state = self.tty_posn_pool.get_or_insert_with(|| {
            Arc::new(TtyPosnCurrentOwner {
                pool: None,
                allocation: Vec::new(),
                dimensions: (0, 0),
                authority: HashMap::default(),
            })
        });
        let state = Arc::make_mut(state);
        // A new live matrix without a completed full-frame pool cannot silently
        // leave an older pool authoritative. Local rows remain valid; geometry-
        // only publications returned above and keep the previous numeric owner.
        state.pool = pool;
        state.allocation = allocation;
        state.dimensions = dimensions;
        for (window, local_matrix) in current_windows {
            state.authority.insert(
                window,
                if local_matrix {
                    TtyCurrentMatrixAuthority::AcceptedLocal
                } else if pool_present {
                    TtyCurrentMatrixAuthority::FramePoolPartition
                } else {
                    TtyCurrentMatrixAuthority::Undrawn
                },
            );
        }
    }

    /// GNU apply_window_adjustment: clear target rows before allocation and
    /// before any eager Lisp callback. KEEP-MARGINS true never calls this.
    #[inline]
    fn tty_posn_apply_window_adjustment(&mut self, window: WindowId) {
        if !self.posn_object_extent_mode().enabled() || self.effective_window_system().is_some() {
            return;
        }
        self.tty_posn_clear_current_window(window);
        self.tty_posn_adjust_current_matrices();
    }

    #[cold]
    #[inline(never)]
    fn tty_posn_clear_current_window(&mut self, window: WindowId) {
        if self.tty_posn_pool.is_none() {
            let (allocation, dimensions) = self.tty_current_matrix_allocation();
            self.tty_posn_pool = Some(Arc::new(TtyPosnCurrentOwner {
                pool: None,
                allocation,
                dimensions,
                authority: HashMap::default(),
            }));
        }
        let state = Arc::make_mut(self.tty_posn_pool.as_mut().expect("current owner"));
        state
            .authority
            .insert(window, TtyCurrentMatrixAuthority::Undrawn);
    }

    /// Actual adjust_frame_glyphs seam. No query, source mutation, unchanged
    /// allocation, or event on another Frame can reactivate disabled rows.
    #[inline]
    fn tty_posn_adjust_current_matrices(&mut self) {
        if !self.posn_object_extent_mode().enabled()
            || self.effective_window_system().is_some()
            || self.tty_posn_pool.is_none()
        {
            return;
        }
        self.tty_posn_adjust_current_matrices_enabled();
    }

    #[cold]
    #[inline(never)]
    fn tty_posn_adjust_current_matrices_enabled(&mut self) {
        let (allocation, dimensions) = self.tty_current_matrix_allocation();
        let old = self.tty_posn_pool.as_ref().expect("current owner");
        if old.allocation == allocation && old.dimensions == dimensions {
            return;
        }
        let preserved = self.tty_posn_pool_can_repartition
            && self.tty_posn_live_margins_clear()
            && old.pool.as_ref().is_some_and(|pool| {
                (pool.columns as i64, pool.lines as i64) == dimensions
                    && pool.columns as i64
                        == (self.width as f32 / self.char_width.max(1.0)).round() as i64
                    && pool.lines as i64
                        == (self.height as f32 / self.char_height.max(1.0)).round() as i64
            });
        let state = Arc::make_mut(self.tty_posn_pool.as_mut().expect("current owner"));
        state.authority.clear();
        for leaf in &allocation {
            state.authority.insert(
                leaf.window,
                if preserved {
                    TtyCurrentMatrixAuthority::FramePoolPartition
                } else {
                    TtyCurrentMatrixAuthority::Undrawn
                },
            );
        }
        state.allocation = allocation;
        state.dimensions = dimensions;
        if !preserved {
            self.tty_posn_pool_can_repartition = false;
        }
    }

    /// Port of dispnew.c allocate_matrices_for_frame_redisplay's TTY branch.
    /// Dimensions are total cols/lines; origins come from recursive chains,
    /// not pixel painting, point/start, text-body or named chrome geometry.
    #[cold]
    #[inline(never)]
    fn tty_current_matrix_allocation(&self) -> (Vec<TtyMatrixAllocation>, (i64, i64)) {
        fn margin(width: i64, requested: usize) -> i64 {
            if requested == 0 {
                0
            } else {
                i64::try_from(requested)
                    .unwrap_or(i64::MAX)
                    .min(width / 2 - 1)
                    .max(1)
            }
        }
        fn leaf(
            window: &Window,
            x: i64,
            y: i64,
            cw: f32,
            ch: f32,
            allocations: &mut Vec<TtyMatrixAllocation>,
        ) -> (i64, i64) {
            let width = window.total_columns(cw);
            let height = window.total_lines(ch);
            let margins = match window {
                Window::Leaf { margins, .. } => *margins,
                Window::Internal { .. } => WindowMargins::ZERO,
            };
            allocations.push(TtyMatrixAllocation {
                window: window.id(),
                x,
                y,
                width,
                height,
                left_margin_glyphs: margin(width, margins.left()),
                right_margin_glyphs: margin(width, margins.right()),
            });
            (width, height)
        }
        fn walk(
            tree: &WindowTree,
            id: WindowId,
            x: i64,
            y: i64,
            cw: f32,
            ch: f32,
            allocations: &mut Vec<TtyMatrixAllocation>,
        ) -> (i64, i64) {
            match tree.find(id) {
                Some(window @ Window::Leaf { .. }) => leaf(window, x, y, cw, ch, allocations),
                Some(Window::Internal {
                    direction,
                    children,
                    ..
                }) => {
                    let (mut width, mut height) = (0, 0);
                    for child in children {
                        let (child_x, child_y) = match direction {
                            SplitDirection::Horizontal => (x + width, y),
                            SplitDirection::Vertical => (x, y + height),
                        };
                        let (w, h) = walk(tree, *child, child_x, child_y, cw, ch, allocations);
                        match direction {
                            SplitDirection::Horizontal => {
                                width += w;
                                height = height.max(h);
                            }
                            SplitDirection::Vertical => {
                                width = width.max(w);
                                height += h;
                            }
                        }
                    }
                    (width, height)
                }
                None => (0, 0),
            }
        }
        let top = self.frame_top_margin();
        let mut allocations = Vec::new();
        let (mut width, mut height) = walk(
            &self.tree,
            self.tree.root_id(),
            0,
            top,
            self.char_width,
            self.char_height,
            &mut allocations,
        );
        if let Some(mini) = self.minibuffer_leaf.as_ref()
            && mini.id() != self.tree.root_id()
        {
            let (w, h) = leaf(
                mini,
                0,
                top + height,
                self.char_width,
                self.char_height,
                &mut allocations,
            );
            width = width.max(w);
            height += h;
        }
        // Chain order alone cannot change a matrix whose numeric allocation
        // stayed the same. Identity/origin/size/reserved margins are sufficient.
        allocations.sort_unstable_by_key(|allocation| allocation.window.0);
        (allocations, (width, top + height))
    }

    /// Allocation-free numeric admission read. Local and pooled current rows
    /// share the same clear authority; callers have already selected extent ON.
    #[inline]
    fn tty_posn_current_matrix_authority(
        &self,
        window: WindowId,
    ) -> Option<TtyCurrentMatrixAuthority> {
        self.tty_posn_pool.as_ref()?.authority.get(&window).copied()
    }
}
