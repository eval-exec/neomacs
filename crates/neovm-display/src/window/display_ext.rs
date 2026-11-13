//! Upper extension traits for evaluator-owned window data (stage-4 W2/E5).
//!
//! The data moved into `emacs_core::eval::data::window_state`; the
//! behaviour that binds this engine lives here as extension traits, the
//! N17 shape the split requires (core cannot add inherent impls to eval
//! types once eval is a crate).

use super::display::{
    canon_y_from_pixel_y, frame_line_height, left_scroll_bar_area_width,
    right_scroll_bar_area_width, vertical_scroll_bar_cols,
};
use super::parameters::alist_from_parameters;
use super::tty_posn_current::{
    TtyCurrentMatrixAuthority, TtyMatrixAllocation, TtyPosnCurrentOwner,
};
use super::*;
use crate::window::geometry::DisplayGeometryExt;
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;
use neovm_eval::emacs_core::eval::data::window_state::{
    frame_default_left_fringe_width, frame_default_right_fringe_width,
};
use neovm_eval::emacs_core::eval::data::window_state::{
    horizontal_scroll_bar_area_height, horizontal_scroll_bar_lines, vertical_scroll_bar_area_width,
};
use neovm_value::value::Value;
use neovm_value::value::eq_value;
use std::sync::Arc;

use super::{ChildExtent, Rect, Window, WindowId, WindowParameters, WindowTree, sibling_bounds};

/// Window behaviour that mints display state rather than reading data.
pub trait DisplayWindowExt {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters;
}

impl DisplayWindowExt for Window {
    /// Return a mutable reference to this window's Lisp-visible parameter alist.
    fn parameters_mut(&mut self) -> &mut WindowParameters {
        let next_generation =
            neovm_eval::emacs_core::eval::data::window_state::next_window_parameters_generation();
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
    fn name_value(&self) -> Value;

    fn title_value(&self) -> Value;

    fn explicit_name_value(&self) -> Value;

    fn icon_name_value(&self) -> Value;

    fn focus_frame_value(&self) -> Value;

    fn title_runtime_string_owned(&self) -> Option<String>;

    fn host_title_runtime_string_owned(&self) -> String;

    fn set_name_value(&mut self, name: Value);

    fn set_generated_name_value(&mut self, name: Value);

    fn set_title_value(&mut self, title: Value);

    fn clear_title(&mut self);

    fn record_window_change_epoch(&mut self, stamp: ChangeStamp);

    fn selected_window_mut(&mut self) -> Option<&mut Window>;

    fn replace_buffer_bindings(&mut self, old_id: BufferId, new_id: BufferId);

    fn font_cell_ascent(&self) -> f32;

    fn frame_parameter_int(&self, key: &str) -> Option<i64>;

    fn frame_child_frame_border_width(&self) -> i64;

    fn effective_divider_width(&self, divider: FrameDivider) -> i64;

    fn outer_border_width(&self) -> i64;

    fn install_gnu_gui_default_parameters(&mut self);

    fn parameter(&self, key: &str) -> Option<Value>;

    fn parameter_symbol(&self, key: &neovm_value::intern::WellKnown) -> Option<Value>;

    fn set_display_identity(&mut self, identity: FrameDisplayIdentity);

    fn set_known_parameter(&mut self, key: FrameParam, value: Value) -> Option<Value>;

    fn set_parameter_key(&mut self, key: FrameParamKey, value: Value) -> Option<Value>;

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

    fn child_frame_viewport_origin(&self) -> (f32, f32);

    fn window_text_area_bounds_with_chrome(&self, reserve_chrome: bool) -> Rect;

    fn window_text_area_bounds(&self) -> Rect;

    fn sync_window_area_bounds(&mut self);

    fn set_window_layout_text_size(&mut self, cols: i64, text_lines: i64);

    fn restored_window_configuration_root_bounds(&self) -> Rect;

    fn reconcile_restored_window_configuration_geometry(&mut self);

    fn sync_minibuffer_top_line_after_root_resize(&mut self);

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

    fn leaf_window_paths(&self, generation: u64) -> Vec<(WindowId, WindowTreePath)>;

    fn window_at_path(&self, path: &WindowTreePath) -> Option<&Window>;

    fn set_tree(&mut self, tree: WindowTree);

    fn root_window_mut(&mut self) -> &mut Window;

    fn root_leaf_ids(&self) -> Vec<WindowId>;

    fn all_leaf_ids(&self) -> Vec<WindowId>;

    fn window_count(&self) -> usize;

    fn window_at(&self, px: f32, py: f32) -> Option<WindowId>;

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

    fn is_display_presentation_prepared(&self, presentation: geometry::PresentationId) -> bool;

    fn has_prepared_display_presentations(&self) -> bool;

    fn completed_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry>;

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

    fn apply_mini_window_resize(&mut self, resize: MiniWindowResize);

    fn shrink_mini_window(&mut self);

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

    fn title_runtime_string_owned(&self) -> Option<String> {
        self.title
            .as_lisp_string()
            .map(|ls| neovm_value::emacs_char::to_utf8_lossy(ls.as_bytes()))
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

    /// The default font's ascent, clamped into the frame's character cell.
    ///
    /// The clamp makes `ascent + descent == char_height` for every frame,
    /// including one whose metrics came from different sources (a test or a
    /// stub host can set `char_height` without a matching `font_ascent`), so a
    /// text-only row keeps measuring exactly one cell tall.
    fn font_cell_ascent(&self) -> f32 {
        self.font_ascent.clamp(0.0, self.char_height.max(0.0))
    }

    fn frame_parameter_int(&self, key: &str) -> Option<i64> {
        self.parameter(key).and_then(|v| v.as_int())
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

    fn parameter(&self, key: &str) -> Option<Value> {
        self.parameters.get(&Value::symbol(key)).copied()
    }

    /// [`Self::parameter`] for a fixed GNU spelling.
    ///
    /// Same parameter-map key and the same constructor accounting; the spelling
    /// is a compile-time DEFSYM id instead of a per-read intern.
    fn parameter_symbol(&self, key: &neovm_value::intern::WellKnown) -> Option<Value> {
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

    fn set_known_parameter(&mut self, key: FrameParam, value: Value) -> Option<Value> {
        self.set_parameter(key.symbol(), value)
    }

    fn set_parameter_key(&mut self, key: FrameParamKey, value: Value) -> Option<Value> {
        self.set_parameter(key.symbol(), value)
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

    fn child_frame_viewport_origin(&self) -> (f32, f32) {
        (0.0, self.chrome_top_height().min(self.height as f32))
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

    /// Number of visible windows (leaves).
    fn window_count(&self) -> usize {
        self.tree.leaf_count()
    }

    /// Find which window is at pixel coordinates.
    fn window_at(&self, px: f32, py: f32) -> Option<WindowId> {
        self.tree.window_at(px, py)
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

    fn is_display_presentation_prepared(&self, presentation: geometry::PresentationId) -> bool {
        self.presentation_state.prepared.contains_key(&presentation)
    }

    fn has_prepared_display_presentations(&self) -> bool {
        !self.presentation_state.prepared.is_empty()
    }

    /// Geometry from the latest completed redisplay, even before renderer
    /// acknowledgement. Lisp motion queries may use this only after checking
    /// the live window's snapshot freshness. Native hit testing uses active
    /// or event-captured presentation geometry instead.
    fn completed_presentation_geometry(&self) -> Option<&geometry::PresentationGeometry> {
        self.presentation_state.completed_geometry.as_deref()
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

/// FrameManager behaviour that binds frame creation, selection, layout,
/// parameters, history and redisplay (stage-4 W2/E5 FM-5e).
pub trait DisplayFrameManagerExt {
    fn navigation_intent_token(&mut self, direction: TransitionDirection) -> NavigationIntentToken;

    fn record_window_navigation_intent(
        &mut self,
        window_id: WindowId,
        direction: TransitionDirection,
    ) -> NavigationIntentToken;

    fn pending_window_navigation_intent(
        &self,
        window_id: WindowId,
    ) -> Option<NavigationIntentToken>;

    fn acknowledge_window_navigation_intent(
        &mut self,
        window_id: WindowId,
        observed: NavigationIntentToken,
    );

    fn record_frame_navigation_intent(
        &mut self,
        frame_id: FrameId,
        direction: TransitionDirection,
    ) -> NavigationIntentToken;

    fn pending_frame_navigation_intent(&self, frame_id: FrameId) -> Option<NavigationIntentToken>;

    fn acknowledge_frame_navigation_intent(
        &mut self,
        frame_id: FrameId,
        observed: NavigationIntentToken,
    );

    fn window_topology_generation(&self) -> u64;

    fn leaf_window_paths(&self, frame_id: FrameId) -> Option<Vec<(WindowId, WindowTreePath)>>;

    fn frame_and_window_at_path(
        &self,
        frame_id: FrameId,
        path: &WindowTreePath,
    ) -> Option<(&Frame, &Window)>;

    fn assign_initial_tty_frame_name(&mut self, frame_id: FrameId) -> bool;

    fn next_generated_tty_frame_name(&mut self) -> Value;

    fn set_tty_frame_name_parameter(&mut self, frame_id: FrameId, name: Value) -> bool;

    fn next_window_id(&mut self) -> WindowId;

    fn create_frame(&mut self, name: &str, width: u32, height: u32, buffer_id: BufferId)
    -> FrameId;

    fn create_frame_value(
        &mut self,
        name: Value,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId;

    fn create_frame_on_terminal(
        &mut self,
        name: &str,
        terminal_id: u64,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId;

    fn create_frame_value_on_terminal(
        &mut self,
        name: Value,
        terminal_id: u64,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId;

    fn force_resize_mini_window_to_one_line(&mut self, id: FrameId);

    fn window_count_showing_buffer(&self, buffer: BufferId) -> usize;

    fn other_window_buffer_changed(&self, buffers: &BufferManager) -> bool;

    fn select_window_for_mode_line(
        &mut self,
        window_id: WindowId,
    ) -> (Option<FrameId>, Option<(FrameId, WindowId)>);

    fn restore_selected_window_for_mode_line(
        &mut self,
        saved: (Option<FrameId>, Option<(FrameId, WindowId)>),
    );

    fn has_frame_outside(&self, affected: &[FrameId]) -> bool;

    fn max_child_z_order(&self, parent: FrameId) -> i32;

    fn set_child_z_order_above_siblings(&mut self, child: FrameId, parent: FrameId);

    fn raise_or_lower_child_frame(&mut self, id: FrameId, raise: bool);

    fn frame_origin_in_root(&self, id: FrameId) -> Option<(f32, f32)>;

    fn place_active_frame(
        &self,
        frame: FrameId,
        presentation: geometry::PresentationId,
    ) -> Result<neomacs_display_protocol::PlacedFrame, neomacs_display_protocol::PlaceChildError>;

    fn render_frame_tree(
        &self,
        selected_or_root: FrameId,
        visibility: RenderFrameVisibility,
    ) -> Option<RenderFrameTree>;

    fn render_frame_forest(
        &self,
        scope: RenderFrameScope,
        visibility: RenderFrameVisibility,
    ) -> Vec<RenderFrameTree>;

    fn split_window(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        direction: SplitDirection,
        new_buffer_id: BufferId,
        size: Option<i64>,
        placement: SplitPlacement,
    ) -> Option<WindowId>;

    fn split_window_with_combination_limit(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        direction: SplitDirection,
        new_buffer_id: BufferId,
        size: Option<i64>,
        placement: SplitPlacement,
        combination_limit: CombinationLimit,
    ) -> Option<WindowId>;

    fn apply_staged_split_sizes(
        &mut self,
        frame_id: FrameId,
        new_window_id: WindowId,
        new_size: Option<i64>,
        new_normal: Value,
        direction: SplitDirection,
    ) -> Option<()>;

    fn delete_window(&mut self, frame_id: FrameId, window_id: WindowId) -> bool;

    fn delete_window_with_resize(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        resize: DeleteResize,
    ) -> bool;

    fn keep_only_window_in_subtree(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        root_id: WindowId,
    ) -> bool;

    fn alloc_window_id(&mut self) -> WindowId;

    fn replace_buffer_in_windows(&mut self, old_id: BufferId, new_id: BufferId);

    fn find_valid_window_frame_id(&self, window_id: WindowId) -> Option<FrameId>;

    fn window_owner_frame_id(&self, window_id: WindowId) -> Option<FrameId>;

    fn is_live_window_id(&self, window_id: WindowId) -> bool;

    fn is_valid_window_id(&self, window_id: WindowId) -> bool;

    fn window_old_buffer(&self, window_id: WindowId) -> WindowOldBuffer;

    fn window_restore_change_record(&self, window_id: WindowId) -> Option<DeletedWindowRecord>;

    fn any_window_frame_id(&self, window_id: WindowId) -> Option<FrameId>;

    fn is_window_object_id(&self, window_id: WindowId) -> bool;

    fn window_new_pixel(&self, window_id: WindowId) -> Option<i64>;

    fn window_new_total(&self, window_id: WindowId) -> Option<i64>;

    fn window_new_normal(&self, window_id: WindowId) -> Value;

    fn set_window_new_pixel(
        &mut self,
        window_id: WindowId,
        size: WindowPixelStage,
    ) -> WindowPixelStage;

    fn set_window_new_total(
        &mut self,
        window_id: WindowId,
        size: WindowTotal,
        update: NewTotalUpdate,
    ) -> Result<WindowTotal, neovm_value::tagged::value::FixnumRangeError>;

    fn set_window_new_normal(&mut self, window_id: WindowId, value: Value);

    fn window_display_state(
        &self,
        window_id: WindowId,
    ) -> Option<(&Frame, &WindowDisplayState, bool)>;

    fn window_display_location(&self, window_id: WindowId) -> Option<(super::FrameId, bool)>;

    fn window_vscroll(&self, window_id: WindowId, pixelwise: bool) -> Option<Value>;

    fn set_window_vscroll(
        &mut self,
        window_id: WindowId,
        vscroll: f64,
        pixelwise: bool,
        preserve_vscroll_p: bool,
    ) -> Option<Value>;

    fn window_display_table(&self, window_id: WindowId) -> Value;

    fn set_window_display_table(&mut self, window_id: WindowId, table: Value);

    fn window_cursor_type(&self, window_id: WindowId) -> Value;

    fn set_window_cursor_type(&mut self, window_id: WindowId, cursor_type: Value);

    fn window_cursor_visible(&self, window_id: WindowId) -> bool;

    fn set_window_cursor_visible(&mut self, window_id: WindowId, visible: bool);

    fn window_fringes(&self, window_id: WindowId) -> Option<(i64, i64, bool, bool)>;

    fn set_window_fringes(
        &mut self,
        window_id: WindowId,
        left_width: Option<i32>,
        right_width: Option<i32>,
        outside_margins: bool,
        persistent: bool,
    ) -> bool;

    fn window_scroll_bars(
        &self,
        window_id: WindowId,
    ) -> Option<(Value, i64, Value, Value, i64, Value, bool)>;

    fn window_scroll_bar_area_width(&self, window_id: WindowId) -> i64;

    fn window_left_scroll_bar_area_width(&self, window_id: WindowId) -> i64;

    fn window_right_scroll_bar_area_width(&self, window_id: WindowId) -> i64;

    fn window_scroll_bar_area_height(&self, window_id: WindowId) -> i64;

    fn set_window_scroll_bars(
        &mut self,
        window_id: WindowId,
        width: Option<i32>,
        vertical_type: Value,
        height: Option<i32>,
        horizontal_type: Value,
        persistent: bool,
    ) -> bool;

    fn apply_set_window_buffer_state(
        &mut self,
        window_id: WindowId,
        buffer_id: BufferId,
        window_start: LispCharPos1,
        point: LispCharPos1,
        preserve_display_state: bool,
        defaults: WindowBufferDisplayDefaults,
    );

    fn live_window_parameters(&self, window_id: WindowId) -> Option<&[(Value, Value)]>;

    fn live_window_parameters_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut Vec<(Value, Value)>>;

    fn window_parameter(&self, window_id: WindowId, key: &Value) -> Option<Value>;

    fn set_window_parameter(&mut self, window_id: WindowId, key: Value, value: Value);

    fn window_parameters_alist(&self, window_id: WindowId) -> Value;

    fn window_parameters_pairs(&self, window_id: WindowId) -> Vec<(Value, Value)>;

    fn live_window_history_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut super::WindowHistoryState>;

    fn window_prev_buffers(&self, window_id: WindowId) -> Value;

    fn set_window_prev_buffers(&mut self, window_id: WindowId, prev_buffers: Value);

    fn window_next_buffers(&self, window_id: WindowId) -> Value;

    fn set_window_next_buffers(&mut self, window_id: WindowId, next_buffers: Value);

    fn note_window_selected(&mut self, window_id: WindowId) -> i64;

    fn bump_window_use_time(
        &mut self,
        selected_window_id: WindowId,
        window_id: WindowId,
    ) -> Option<i64>;

    fn old_selected_window(&self) -> Option<WindowId>;
}

impl DisplayFrameManagerExt for FrameManager {
    fn navigation_intent_token(&mut self, direction: TransitionDirection) -> NavigationIntentToken {
        let generation = self.next_navigation_intent_generation;
        self.next_navigation_intent_generation = std::num::NonZeroU64::new(
            generation
                .get()
                .checked_add(1)
                .expect("navigation intent generation overflow"),
        )
        .expect("incremented navigation intent generation stays nonzero");
        NavigationIntentToken {
            generation,
            direction,
        }
    }

    /// Record semantic navigation for the next accepted presentation that
    /// replaces this window's content.
    fn record_window_navigation_intent(
        &mut self,
        window_id: WindowId,
        direction: TransitionDirection,
    ) -> NavigationIntentToken {
        let intent = self.navigation_intent_token(direction);
        self.pending_content_transition_intents
            .windows
            .record(window_id, intent);
        intent
    }

    fn pending_window_navigation_intent(
        &self,
        window_id: WindowId,
    ) -> Option<NavigationIntentToken> {
        self.pending_content_transition_intents
            .windows
            .pending(window_id)
    }

    /// Acknowledge only the intent observed by the accepted presentation.
    /// This comparison prevents an older layout from consuming newer input.
    fn acknowledge_window_navigation_intent(
        &mut self,
        window_id: WindowId,
        observed: NavigationIntentToken,
    ) {
        self.pending_content_transition_intents
            .windows
            .acknowledge(window_id, observed);
    }

    /// Record semantic navigation for one whole-frame content replacement.
    fn record_frame_navigation_intent(
        &mut self,
        frame_id: FrameId,
        direction: TransitionDirection,
    ) -> NavigationIntentToken {
        let intent = self.navigation_intent_token(direction);
        self.pending_content_transition_intents
            .frames
            .record(frame_id, intent);
        intent
    }

    fn pending_frame_navigation_intent(&self, frame_id: FrameId) -> Option<NavigationIntentToken> {
        self.pending_content_transition_intents
            .frames
            .pending(frame_id)
    }

    fn acknowledge_frame_navigation_intent(
        &mut self,
        frame_id: FrameId,
        observed: NavigationIntentToken,
    ) {
        self.pending_content_transition_intents
            .frames
            .acknowledge(frame_id, observed);
    }

    fn window_topology_generation(&self) -> u64 {
        self.window_topology_generation
    }

    /// Capture generation-bound leaf routes for one frame.
    fn leaf_window_paths(&self, frame_id: FrameId) -> Option<Vec<(WindowId, WindowTreePath)>> {
        Some(
            self.frames
                .get(&frame_id)?
                .leaf_window_paths(self.window_topology_generation),
        )
    }

    /// Resolve one route only in the exact topology generation that produced
    /// it. A stale route is rejected before callers can publish live state or
    /// execute Lisp for the wrong frame attempt.
    fn frame_and_window_at_path(
        &self,
        frame_id: FrameId,
        path: &WindowTreePath,
    ) -> Option<(&Frame, &Window)> {
        if path.generation != self.window_topology_generation {
            return None;
        }
        let frame = self.frames.get(&frame_id)?;
        Some((frame, frame.window_at_path(path)?))
    }

    /// Assign GNU's special initial terminal-frame name and reset the
    /// independent TTY name sequence to one (`make_initial_frame`, frame.c).
    fn assign_initial_tty_frame_name(&mut self, frame_id: FrameId) -> bool {
        self.last_tty_frame_name = Some(TtyFrameNameOrdinal::INITIAL);
        let Some(frame) = self.frames.get_mut(&frame_id) else {
            return false;
        };
        frame.set_generated_name_value(TtyFrameNameOrdinal::INITIAL.value());
        true
    }

    /// Allocate GNU's next collision-free generated terminal-frame name.
    fn next_generated_tty_frame_name(&mut self) -> Value {
        loop {
            let ordinal = self
                .last_tty_frame_name
                .map(TtyFrameNameOrdinal::next)
                .unwrap_or(TtyFrameNameOrdinal::INITIAL);
            self.last_tty_frame_name = Some(ordinal);
            let candidate = ordinal.value();
            let collision = self.frames.values().any(|frame| {
                frame
                    .name_value()
                    .as_lisp_string()
                    .zip(candidate.as_lisp_string())
                    .is_some_and(|(left, right)| left == right)
            });
            if !collision {
                return candidate;
            }
        }
    }

    /// Apply GNU `set_term_frame_name`: nil preserves an existing `F<num>`
    /// name, otherwise it allocates from the TTY name sequence.
    fn set_tty_frame_name_parameter(&mut self, frame_id: FrameId, name: Value) -> bool {
        if !name.is_nil() {
            let Some(frame) = self.frames.get_mut(&frame_id) else {
                return false;
            };
            frame.set_name_value(name);
            return true;
        }

        if let Some(frame) = self.frames.get_mut(&frame_id)
            && is_generated_tty_frame_name(frame.name_value())
        {
            let current = frame.name_value();
            frame.set_generated_name_value(current);
            return true;
        }

        let generated = self.next_generated_tty_frame_name();
        let Some(frame) = self.frames.get_mut(&frame_id) else {
            return false;
        };
        frame.set_generated_name_value(generated);
        true
    }

    /// Allocate a new window ID.
    fn next_window_id(&mut self) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        self.deleted_windows.remove(&id);
        self.deleted_window_parameters.remove(&id);
        id
    }

    /// Create a new frame with a single window displaying `buffer_id`.
    fn create_frame(
        &mut self,
        name: &str,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId {
        self.create_frame_value(Value::string(name), width, height, buffer_id)
    }

    fn create_frame_value(
        &mut self,
        name: Value,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId {
        self.create_frame_value_on_terminal(name, 0, width, height, buffer_id)
    }

    fn create_frame_on_terminal(
        &mut self,
        name: &str,
        terminal_id: u64,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId {
        self.create_frame_value_on_terminal(
            Value::string(name),
            terminal_id,
            width,
            height,
            buffer_id,
        )
    }

    fn create_frame_value_on_terminal(
        &mut self,
        name: Value,
        terminal_id: u64,
        width: u32,
        height: u32,
        buffer_id: BufferId,
    ) -> FrameId {
        let frame_id = FrameId(self.next_frame_id);
        self.next_frame_id += 1;

        let window_id = self.next_window_id();
        let bounds = Rect::new(0.0, 0.0, width as f32, height as f32);
        let root = Window::new_leaf(window_id, buffer_id, bounds);

        // GNU `make_frame' creates the root window first, then the minibuffer
        // window, both drawing from the same global window sequence counter
        // (frame.c:1228/1232) -> root gets #1, minibuffer #2. Allocate the
        // minibuffer id from the counter here (immediately after the root) so
        // neomacs's window numbering matches GNU exactly.
        let minibuffer_window_id = self.next_window_id();

        let mut frame = Frame::new(
            frame_id,
            name,
            terminal_id,
            width,
            height,
            root,
            minibuffer_window_id,
        );
        if let Some(init) = self.frame_init_hook {
            init(&mut frame);
        }
        let selected_wid = frame.selected_window;
        self.frames.insert(frame_id, frame);
        self.terminal_top_frames
            .entry(terminal_id)
            .or_insert(frame_id);
        self.mark_window_topology_changed();
        self.note_window_selected(selected_wid);

        if self.selected.is_none() {
            self.selected = Some(frame_id);
            self.old_selected_window = Some(selected_wid);
        }

        frame_id
    }

    /// Force the frame's minibuffer window back to exactly one line, content
    /// independent.
    ///
    /// This is neomacs's analogue of GNU's unconditional
    /// `resize_mini_window (XWINDOW (minibuf_window), 0)` at the outermost
    /// minibuffer unwind (`minibuf.c:1188-1190`). Unlike the layout-engine
    /// auto-resize heuristic (which only *reacts* to measured content and is
    /// the steady-state mechanism), this asserts the end state imperatively at
    /// teardown so the mini-window cannot stay grown after the active
    /// minibuffer is gone.
    ///
    /// It reuses the existing minibuffer split / shrink machinery
    /// (`reposition_minibuffer_below_root` via `sync_window_area_bounds`) to
    /// set the mini-window height to one line and return the freed space to the
    /// root. The next redisplay re-measures the inactive mini-window from its
    /// displayed echo buffer's *content* (see the layout engine's
    /// `echo_content_rows`), so it correctly stays one line for an empty echo
    /// or "Quit", and re-grows for a genuine multi-line message — without any
    /// stale-matrix override that would suppress such a message.
    fn force_resize_mini_window_to_one_line(&mut self, id: FrameId) {
        let Some(frame) = self.frames.get_mut(&id) else {
            return;
        };
        let unit = frame.char_height.max(1.0);
        if let Some(mini) = frame.minibuffer_leaf.as_mut() {
            let mut bounds = *mini.bounds();
            bounds.height = unit;
            mini.set_bounds(bounds);
            // Reset any stale vscroll left on the mini-window by
            // vertico-posframe (which scrolls the real minibuffer out of view
            // while its child frame shows the candidates). A nonzero vscroll on
            // a one-line mini-window drives `visible_max_rows` to 0 (the
            // posframe-hiding guard in the layout engine), which silently drops
            // the echo row — so after a C-g abort the "Quit"/message text is
            // laid out but produces no glyphs. GNU's `resize_mini_window`
            // resets the window's scroll on unwind; mirror that here, matching
            // the split-reset path elsewhere in this file.
            if let Window::Leaf {
                vscroll,
                preserve_vscroll_p,
                ..
            } = mini
            {
                *vscroll = 0;
                *preserve_vscroll_p = false;
            }
        }
        // Hand the freed rows back to the root window and reposition the
        // mini-window below it, exactly like `shrink_mini_window`.
        frame.sync_window_area_bounds();
    }

    /// Number of live windows whose `buffer_id` is exactly `buffer`,
    /// including minibuffer windows' leaf windows.
    ///
    /// [`Self::buffer_window_count`] resolves indirect buffers through the
    /// live buffer table, so it reports zero for a buffer that has just been
    /// removed from that table even while windows still reference its id.
    /// GNU `replace_buffer_in_windows_safely` (window.c) gates its repair on
    /// the incoming buffer's window count, which survives the buffer being
    /// removed from `Vbuffer_alist`; this is the equivalent exact-id count.
    fn window_count_showing_buffer(&self, buffer: BufferId) -> usize {
        self.frames
            .values()
            .map(|frame| {
                frame
                    .tree()
                    .leaf_ids()
                    .into_iter()
                    .filter(|id| frame.tree().find(*id).and_then(Window::buffer_id) == Some(buffer))
                    .count()
                    + usize::from(
                        frame.minibuffer_leaf.as_ref().and_then(Window::buffer_id) == Some(buffer),
                    )
            })
            .sum()
    }

    /// GNU's `bset_redisplay` observes ordinary windows on every frame.
    /// Read only the live tree and completed display identities; detached
    /// windows and the inactive minibuffer cannot raise this predicate.
    /// The owning Context's mutator borrows these inputs; no state is cached
    /// or shared between mutators, and the walk allocates no window list.
    fn other_window_buffer_changed(&self, buffers: &BufferManager) -> bool {
        fn changed(
            frame: &Frame,
            id: WindowId,
            selected: Option<WindowId>,
            buffers: &BufferManager,
        ) -> bool {
            let Some(window) = frame.tree.find(id) else {
                return false;
            };
            if !window.children().is_empty() {
                return window
                    .children()
                    .iter()
                    .any(|id| changed(frame, *id, selected, buffers));
            }
            if Some(id) == selected {
                return false;
            }
            let Some(buffer) = window.buffer_id().and_then(|id| buffers.get(id)) else {
                return false;
            };
            if buffer.changed_char_range().is_some() {
                return true;
            }
            frame.redisplay_snapshot(id).is_some_and(|snapshot| {
                snapshot
                    .buffer_modiff
                    .is_some_and(|tick| tick != buffer.modified_tick())
                    || snapshot.layout_freshness.as_ref().is_some_and(|freshness| {
                        freshness.buffer.id != buffer.id()
                            || freshness.buffer.overlay_modified_tick
                                != buffer.overlay_modified_tick()
                    })
            })
        }
        let selected = self.selected_frame().map(|frame| frame.selected_window);
        self.frames
            .values()
            .any(|frame| changed(frame, frame.tree.root_id(), selected, buffers))
    }

    /// Temporarily make `window_id` the selected window (and its frame the
    /// selected frame) for the duration of mode/tab/header-line evaluation.
    ///
    /// Mirrors GNU `display_mode_lines` (`src/xdisp.c`), which assigns
    /// `selected_window = w` and `XFRAME (new_frame)->selected_window` before
    /// walking the format and restores them via `unwind_protect`.  Without
    /// this, `:eval` forms that read `(selected-window)` — e.g.
    /// `tab-line-tabs-window-buffers`, the default `tab-line-tabs-function`
    /// (`lisp/tab-line.el`) — see the globally selected window instead of the
    /// window being redisplayed, so every window's tab line shows the
    /// selected window's buffer.  This is a lightweight assignment (no focus
    /// redirection, no hooks), matching GNU's note that full `select_window`
    /// is only needed if the format mutates point.  Pass the returned token to
    /// [`Self::restore_selected_window_for_mode_line`].
    #[must_use]
    fn select_window_for_mode_line(
        &mut self,
        window_id: WindowId,
    ) -> (Option<FrameId>, Option<(FrameId, WindowId)>) {
        let prev_selected_frame = self.selected;
        let frame_id = self.find_window_frame_id(window_id);
        let prev_frame_window = frame_id.and_then(|fid| {
            let frame = self.frames.get_mut(&fid)?;
            let prev = frame.selected_window;
            frame.selected_window = window_id;
            Some((fid, prev))
        });
        if let Some(fid) = frame_id {
            self.selected = Some(fid);
        }
        (prev_selected_frame, prev_frame_window)
    }

    /// Undo [`Self::select_window_for_mode_line`].
    fn restore_selected_window_for_mode_line(
        &mut self,
        saved: (Option<FrameId>, Option<(FrameId, WindowId)>),
    ) {
        let (prev_selected_frame, prev_frame_window) = saved;
        if let Some((fid, prev)) = prev_frame_window
            && let Some(frame) = self.frames.get_mut(&fid)
        {
            if frame.find_window(prev).is_some() {
                frame.selected_window = prev;
            } else if frame.find_window(frame.selected_window).is_none()
                && let Some(first_live_leaf) = frame.window_list().first().copied()
            {
                frame.selected_window = first_live_leaf;
            }
        }
        self.selected = match prev_selected_frame {
            None => None,
            Some(previous) if self.frames.contains_key(&previous) => Some(previous),
            Some(_) => self
                .selected
                .filter(|current| {
                    self.frames
                        .get(current)
                        .is_some_and(|frame| frame.parent_frame.as_frame_id().is_none())
                })
                .or_else(|| {
                    self.frames.iter().find_map(|(frame_id, frame)| {
                        frame
                            .parent_frame
                            .as_frame_id()
                            .is_none()
                            .then_some(*frame_id)
                    })
                }),
        };
    }

    /// True when a live frame is absent from `affected`.
    fn has_frame_outside(&self, affected: &[FrameId]) -> bool {
        self.frames
            .keys()
            .any(|frame_id| !affected.contains(frame_id))
    }

    fn max_child_z_order(&self, parent: FrameId) -> i32 {
        self.frames
            .values()
            .filter(|frame| frame.parent_frame.as_frame_id() == Some(parent.0))
            .map(|frame| frame.z_order)
            .max()
            .unwrap_or(0)
    }

    fn set_child_z_order_above_siblings(&mut self, child: FrameId, parent: FrameId) {
        let z_order = 1 + self.max_child_z_order(parent);
        if let Some(frame) = self.frames.get_mut(&child) {
            frame.z_order = z_order;
        }
    }

    fn raise_or_lower_child_frame(&mut self, id: FrameId, raise: bool) {
        let Some(parent) = self.frame_parent_id(id) else {
            return;
        };
        let mut siblings: Vec<FrameId> = self
            .frames
            .iter()
            .filter_map(|(frame_id, frame)| {
                (frame.parent_frame.as_frame_id() == Some(parent.0)).then_some(*frame_id)
            })
            .collect();
        siblings.sort_by(|a, b| self.frame_z_order_cmp(*a, *b));

        let mut next_z = 0;
        for sibling in siblings {
            if sibling == id {
                continue;
            }
            if let Some(frame) = self.frames.get_mut(&sibling) {
                frame.z_order = next_z;
            }
            next_z += 1;
        }

        if let Some(frame) = self.frames.get_mut(&id) {
            frame.z_order = if raise { next_z } else { 0 };
        }
    }

    fn frame_origin_in_root(&self, id: FrameId) -> Option<(f32, f32)> {
        if !self.frames.contains_key(&id) {
            return None;
        }

        let mut x = 0_i64;
        let mut y = 0_i64;
        let mut viewport_x = 0.0_f32;
        let mut viewport_y = 0.0_f32;
        let mut current = Some(id);
        let mut seen = HashSet::default();
        while let Some(frame_id) = current {
            if !seen.insert(frame_id) {
                return None;
            }
            let frame = self.frames.get(&frame_id)?;
            x += frame.left_pos;
            y += frame.top_pos;
            current = self.frame_parent_id(frame_id);
            if let Some(parent_id) = current {
                let parent = self.frames.get(&parent_id)?;
                let (dx, dy) = parent.child_frame_viewport_origin();
                viewport_x += dx;
                viewport_y += dy;
            }
        }
        Some((x as f32 + viewport_x, y as f32 + viewport_y))
    }

    /// Resolve immutable parent-relative child placement through only accepted
    /// frame presentations. Parent chrome and root desktop offsets never enter
    /// this composition.
    fn place_active_frame(
        &self,
        frame: FrameId,
        presentation: geometry::PresentationId,
    ) -> Result<neomacs_display_protocol::PlacedFrame, neomacs_display_protocol::PlaceChildError>
    {
        let scene = neomacs_display_protocol::PresentedFrameScene::from_placements(
            self.frames.values().filter_map(|frame| {
                frame
                    .active_presentation_geometry()
                    .map(|geometry| geometry.frame_placement())
            }),
        )?;
        scene.place(neomacs_display_protocol::PlaceChildQuery::new(
            neomacs_display_protocol::DisplayFrameId::new(frame.0),
            neomacs_display_protocol::PresentationId::new(presentation.get()),
        ))
    }

    fn render_frame_tree(
        &self,
        selected_or_root: FrameId,
        visibility: RenderFrameVisibility,
    ) -> Option<RenderFrameTree> {
        let root_id = self.root_frame_id(selected_or_root)?;
        let frames_bottom_to_top = self
            .frames_in_reverse_z_order(root_id, visibility)
            .into_iter()
            .filter_map(|frame_id| {
                let frame = self.frames.get(&frame_id)?;
                let (origin_in_root_x, origin_in_root_y) = self.frame_origin_in_root(frame_id)?;
                Some(RenderFrameNode {
                    frame_id,
                    parent_id: self.frame_parent_id(frame_id),
                    origin_in_root_x,
                    origin_in_root_y,
                    z_order: frame.z_order,
                })
            })
            .collect();

        Some(RenderFrameTree {
            root_id,
            frames_bottom_to_top,
        })
    }

    /// Return owned render trees for the requested backend scope.
    ///
    /// Each root appears once and owns all of its visible descendants.  Native
    /// window roots are sorted by stable frame id so HashMap iteration order
    /// cannot perturb presentation submission order.
    fn render_frame_forest(
        &self,
        scope: RenderFrameScope,
        visibility: RenderFrameVisibility,
    ) -> Vec<RenderFrameTree> {
        let mut roots: Vec<FrameId> = match scope {
            RenderFrameScope::TreeContaining(frame_id) => {
                self.root_frame_id(frame_id).into_iter().collect()
            }
            RenderFrameScope::AllNativeWindowTrees => self
                .frames
                .iter()
                .filter_map(|(frame_id, frame)| {
                    (self.frame_parent_id(*frame_id).is_none()
                        && frame.effective_window_system().is_some())
                    .then_some(*frame_id)
                })
                .collect(),
        };
        roots.sort_by_key(|frame_id| frame_id.0);
        roots.dedup();
        roots
            .into_iter()
            .filter_map(|root| self.render_frame_tree(root, visibility))
            .filter(|tree| !tree.frames_bottom_to_top.is_empty())
            .collect()
    }

    /// Split a window horizontally or vertically.
    /// Returns the new window's ID, or None if the window wasn't found.
    ///
    /// `size` controls how space is divided:
    /// - `None` or `Some(0)`: split 50/50
    /// - `Some(n)` where n > 0: the **new** window gets `n` units (lines or
    ///   columns), the old window gets the remainder.
    /// - `Some(n)` where n < 0: the **old** window gets `|n|` units, the new
    ///   window gets the remainder.
    ///
    /// `placement` controls whether the new window is inserted before or after
    /// the split target in the parent child list. This mirrors GNU
    /// `split-window-internal` side ordering: `above`/`left` insert before the
    /// target, while `below`/`right` insert after it.
    fn split_window(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        direction: SplitDirection,
        new_buffer_id: BufferId,
        size: Option<i64>,
        placement: SplitPlacement,
    ) -> Option<WindowId> {
        self.split_window_with_combination_limit(
            frame_id,
            window_id,
            direction,
            new_buffer_id,
            size,
            placement,
            CombinationLimit::TreeDecides,
        )
    }

    /// Split a window, honoring the caller's `window-combination-limit`.
    ///
    /// [`Self::split_window`] is the same operation with the variable's
    /// default (`nil`) policy. Only the Lisp entry point
    /// (`split-window-internal`) has a dynamic binding to pass here; GNU reads
    /// it as `Vwindow_combination_limit` in `Fsplit_window_internal`
    /// (`src/window.c:5426`).
    #[allow(clippy::too_many_arguments)] // mirrors GNU's split parameter set
    fn split_window_with_combination_limit(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        direction: SplitDirection,
        new_buffer_id: BufferId,
        size: Option<i64>,
        placement: SplitPlacement,
        combination_limit: CombinationLimit,
    ) -> Option<WindowId> {
        let internal_id = self.alloc_window_id();
        let new_id = self.alloc_window_id();
        let frame = self.frames.get_mut(&frame_id)?;

        // GNU decides "interpose a new parent" vs "splice into the existing
        // combination" *before* touching the tree, from the dynamic variable
        // plus the target's position in it (`src/window.c:5423-5431`).
        let parent = parent_combination_of(frame.tree(), window_id)?;
        let attachment = SplitAttachment::decide(combination_limit, parent, direction);

        split_window_in_tree(
            frame.tree_mut(),
            window_id,
            direction,
            internal_id,
            new_id,
            new_buffer_id,
            size,
            placement,
            attachment,
        )?;

        // Character edges are NOT re-derived here, and that is deliberate.
        //
        // GNU assigns them at resize-apply points -- `Fwindow_resize_apply_total`
        // sets `r->top_line = FRAME_TOP_MARGIN (f)` and the recursive pass flows
        // rows to children (`src/window.c:5020`) -- rather than maintaining them
        // as an invariant over the pixel bounds.  They are therefore legitimately
        // path-dependent in GNU: with two side windows GNU reports the ROOT at
        // character row 1 and its CHILD at row 0, both with pixel top 0.
        //
        // Syncing here looks more coherent and is measurably wrong: it pulls
        // side-window leaves down a line, which is what the earlier warning
        // about `sync_window_area_bounds` was describing, and a
        // character-edge-only sync does it just the same.
        frame.recalculate_minibuffer_bounds();
        frame.tty_posn_adjust_current_matrices();
        self.mark_window_topology_changed();
        Some(new_id)
    }

    /// Commit the sizes `window.el` staged for the combination that just
    /// gained `new_window_id`.
    ///
    /// Mirrors the tail of GNU `Fsplit_window_internal`
    /// (`src/window.c:5636-5672`): the new window's own size is staged from the
    /// SIZE argument,
    ///
    /// ```c
    /// wset_new_pixel (n, pixel_size);
    /// ...
    /// window_resize_apply (p, horflag);
    /// ```
    ///
    /// and then the whole parent combination is applied, so the siblings that
    /// `window.el` shrank actually shrink. Without this the primitive would be
    /// re-deriving a layout the Lisp layer already computed — which is how
    /// `window-combination-resize` came to have no effect at all, since under
    /// that policy the new window's space comes from *every* sibling rather
    /// than only from the split target.
    fn apply_staged_split_sizes(
        &mut self,
        frame_id: FrameId,
        new_window_id: WindowId,
        new_size: Option<i64>,
        new_normal: Value,
        direction: SplitDirection,
    ) -> Option<()> {
        let frame = self.frames.get_mut(&frame_id)?;
        if let Some(window) = frame.find_window_mut(new_window_id) {
            if let Some(size) = new_size {
                window.set_new_pixel(Some(size));
            }
            // GNU `wset_new_normal (n, normal_size)`, unconditionally: a nil
            // NORMAL-SIZE leaves the new window at `make_window`'s 1.0 rather
            // than committing a value cloned from the split target.  The
            // *other* children's fractions were staged by `window.el`, so they
            // are not touched here -- `window_resize_apply` commits them all
            // together.
            window.set_new_normal(new_normal);
        }
        let parent_id = frame.tree().parent_of(new_window_id)?;
        let horflag = matches!(direction, SplitDirection::Horizontal);
        let (char_width, char_height) = (frame.char_width, frame.char_height);
        window_resize_apply(
            frame.tree_mut(),
            parent_id,
            horflag,
            char_width,
            char_height,
        );
        frame.tty_posn_adjust_current_matrices();
        Some(())
    }

    /// Delete a window from a frame. Cannot delete the last window.
    ///
    /// The freed space is spread over the remaining siblings. Callers coming
    /// from Lisp must use [`Self::delete_window_with_resize`] with
    /// [`DeleteResize::ApplyStaged`] instead, so that the sizes `window.el`
    /// already staged are honored.
    fn delete_window(&mut self, frame_id: FrameId, window_id: WindowId) -> bool {
        self.delete_window_with_resize(frame_id, window_id, DeleteResize::Redistribute)
    }

    /// Delete a window, reclaiming its space per `resize`.
    ///
    /// Mirrors GNU `Fdelete_window_internal`, which unlinks the window and then
    /// commits the staged sizes with `window_resize_apply`.
    fn delete_window_with_resize(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        resize: DeleteResize,
    ) -> bool {
        let Some(frame) = self.frames.get_mut(&frame_id) else {
            return false;
        };
        if frame.tree().leaf_count() <= 1 {
            return false; // Can't delete last window
        }

        let deleted_windows = &mut self.deleted_windows;
        let deleted_parameters = &mut self.deleted_window_parameters;
        // A promotion at the very top has no grandparent to merge into, so the
        // outcome collapses to "was it removed" here.
        let cells = body::WindowBodyCellSize::new(frame.char_width, frame.char_height);
        let removed = delete_window_in_tree(frame.tree_mut(), window_id, resize, cells, |window| {
            let id = window.id();
            let record = Self::deletion_record(Some(frame_id), Some(&window));
            let parameters = match window {
                Window::Leaf { parameters, .. } | Window::Internal { parameters, .. } => parameters,
            };
            deleted_windows.insert(id, record);
            deleted_parameters.insert(id, parameters);
        })
        .removed();
        if removed {
            // No character-edge resync here either; see the note in
            // `split_window_with_combination_limit`.  GNU assigns those rows at
            // resize-apply points, so a survivor keeps its old row until
            // `window-resize-apply-total` runs -- which is what `window.el`
            // does on the paths GNU allows into this code at all.
            frame.recalculate_minibuffer_bounds();
            frame.tty_posn_adjust_current_matrices();
        }

        // Deleting an INTERNAL window takes its whole subtree with it, so the
        // selected window can die without being the one named -- `delete-window`
        // on a member of an atomic window deletes the atom's ROOT, killing every
        // leaf beneath it.  Testing `selected_window == window_id` missed that
        // and left the frame pointing at a dead window, which the next
        // `window-normalize-window ... t` reported as
        // "#<window N> is not a live window".  Ask the tree instead.
        if removed && frame.find_window(frame.selected_window).is_none() {
            // Select the first remaining leaf. We do NOT touch
            // `old_selected_window` here — that field is recorded
            // by `window_change_record` (GNU
            // `src/window.c:3954-3990`) at redisplay time, not
            // immediately on deletion.
            if let Some(first) = frame.tree().leaf_ids().first() {
                frame.selected_window = *first;
            }
        }

        if removed {
            self.mark_window_topology_changed();
        }

        removed
    }

    /// Replace ROOT with its descendant WINDOW, deleting the other windows in
    /// ROOT's subtree.
    ///
    /// This mirrors GNU Emacs's `delete-other-windows-internal` /
    /// `replace_window`: WINDOW keeps its identity and window-local state but
    /// inherits ROOT's geometry and normal sizes.  Keeping this as one tree
    /// transition avoids the geometry drift caused by deleting sibling leaves
    /// one at a time.
    fn keep_only_window_in_subtree(
        &mut self,
        frame_id: FrameId,
        window_id: WindowId,
        root_id: WindowId,
    ) -> bool {
        let Some(frame) = self.frames.get_mut(&frame_id) else {
            return false;
        };
        let Some(root) = frame.tree().find(root_id) else {
            return false;
        };
        if frame.tree().find(window_id).is_none() {
            return false;
        }

        if window_id == root_id {
            return true;
        }

        let root_bounds = *root.bounds();
        let root_top_line = root.top_line();
        let root_left_col = root.left_col();
        let root_normal_lines = root.normal_lines();
        let root_normal_cols = root.normal_cols();

        let kept_ids: HashSet<WindowId> = frame.tree().subtree_ids(window_id).into_iter().collect();
        let removed_windows: Vec<(WindowId, WindowParameters)> = frame
            .tree()
            .subtree_ids(root_id)
            .into_iter()
            .filter(|id| !kept_ids.contains(id))
            .filter_map(|id| {
                frame
                    .tree()
                    .find(id)
                    .map(|window| (id, window.parameters().clone()))
            })
            .collect();

        // WINDOW takes ROOT's place in the tree.  GNU's `replace_window` does
        // exactly this relinking -- the kept window keeps its identity and
        // window-local state and inherits ROOT's geometry -- which under ids is
        // repointing one slot rather than copying a subtree over another.
        let parent = frame.tree().parent_of(root_id);
        match parent {
            Some(parent_id) => {
                let mut siblings = frame.tree().child_ids(parent_id).to_vec();
                for slot in &mut siblings {
                    if *slot == root_id {
                        *slot = window_id;
                    }
                }
                frame.tree_mut().set_children(parent_id, siblings);
            }
            None => frame.tree_mut().set_root(window_id),
        }

        resize_window_subtree(frame.tree_mut(), window_id, root_bounds);
        let (char_width, char_height) = (frame.char_width, frame.char_height);
        sync_window_character_edges_from_bounds_at(
            frame.tree_mut(),
            window_id,
            root_left_col,
            root_top_line,
            char_width,
            char_height,
        );
        if let Some(replacement) = frame.tree_mut().find_mut(window_id) {
            replacement.set_normal_lines(root_normal_lines);
            replacement.set_normal_cols(root_normal_cols);
            if let Window::Leaf {
                vscroll,
                preserve_vscroll_p,
                ..
            } = replacement
            {
                *vscroll = 0;
                *preserve_vscroll_p = false;
            }
        }
        // The whole kept subtree's redisplay state is stale, not just the node
        // that took ROOT's slot.
        for leaf in frame.tree().leaf_ids_of(window_id) {
            if let Some(node) = frame.tree_mut().find_mut(leaf) {
                node.invalidate_display_state();
            }
        }

        // The unlinked subtree's nodes go away with the windows they name.
        for (id, _) in &removed_windows {
            frame.tree_mut().remove(*id);
        }

        if !frame
            .tree()
            .subtree_ids(window_id)
            .contains(&frame.selected_window)
            && let Some(first) = frame.tree().leaf_ids_of(window_id).first()
        {
            frame.selected_window = *first;
        }
        frame.recalculate_minibuffer_bounds();
        frame.tty_posn_adjust_current_matrices();

        for (id, parameters) in removed_windows {
            self.deleted_windows.insert(
                id,
                DeletedWindowRecord {
                    owner: Some(frame_id),
                    old_buffer: None,
                    change_stamp: None,
                },
            );
            self.deleted_window_parameters.insert(id, parameters);
        }

        self.mark_window_topology_changed();

        true
    }

    fn alloc_window_id(&mut self) -> WindowId {
        let id = WindowId(self.next_window_id);
        self.next_window_id += 1;
        self.deleted_windows.remove(&id);
        self.deleted_window_parameters.remove(&id);
        id
    }

    /// Replace dead-buffer bindings in every live frame.
    fn replace_buffer_in_windows(&mut self, old_id: BufferId, new_id: BufferId) {
        for frame in self.frames.values_mut() {
            frame.replace_buffer_bindings(old_id, new_id);
        }
    }

    /// Return the frame containing a valid window ID, if any.
    ///
    /// Valid windows include live leaf windows, internal windows, and the
    /// minibuffer window of a live frame.
    fn find_valid_window_frame_id(&self, window_id: WindowId) -> Option<FrameId> {
        self.frames
            .iter()
            .find_map(|(frame_id, frame)| frame.find_window(window_id).map(|_| *frame_id))
    }

    /// Return the recorded owner of a live or deleted window object.
    ///
    /// A retained owner identity does not imply the frame is still live.
    /// Missing ownership remains unknown rather than naming an unrelated frame.
    fn window_owner_frame_id(&self, window_id: WindowId) -> Option<FrameId> {
        self.find_valid_window_frame_id(window_id).or_else(|| {
            self.deleted_windows
                .get(&window_id)
                .and_then(|record| record.owner)
        })
    }

    /// Return true when WINDOW-ID designates a live window in any frame.
    fn is_live_window_id(&self, window_id: WindowId) -> bool {
        self.find_window_frame_id(window_id).is_some()
    }

    /// Return true when WINDOW-ID designates a valid live or internal window.
    fn is_valid_window_id(&self, window_id: WindowId) -> bool {
        self.find_valid_window_frame_id(window_id).is_some()
    }

    /// Return true when WINDOW-ID designates a live or stale window object.
    /// The frame owning WINDOW_ID for GNU's `decode_any_window` -- `CHECK_WINDOW`,
    /// which admits a live window, an internal window, and one that has been
    /// deleted.
    /// GNU's `window-old-buffer` answer for WINDOW, as a closed set.
    ///
    /// ```c
    ///   return (NILP (w->old_buffer)                                   ? Qnil
    ///           : (w->change_stamp != WINDOW_XFRAME (w)->change_stamp) ? Qt
    ///           : w->old_buffer);
    /// ```
    ///
    /// The three arms are a sum type, not two nullable slots compared at every
    /// call: an old buffer without an epoch, or an epoch without a buffer, are
    /// states GNU can represent and this cannot.
    fn window_old_buffer(&self, window_id: WindowId) -> WindowOldBuffer {
        // A LIVE window answers from what the last window-change record left on
        // it; only a window that is no longer live falls back to what it
        // recorded on the way out.
        //
        // The order matters and is not GNU's problem: GNU holds a deleted
        // window as its own object, while neomacs keys the record on an id
        // that can come back -- `set-window-configuration` deletes and re-adds
        // windows, so consulting the deletion record first let a stale entry
        // shadow the live window and answer nil for a window that had just
        // been recorded.
        let live = self
            .find_valid_window_frame_id(window_id)
            .and_then(|frame_id| self.frames.get(&frame_id));
        let (old_buffer, stamp, frame_stamp) = if let Some(frame) = live {
            let window = frame.find_window(window_id);
            (
                window.and_then(Window::old_buffer),
                window.and_then(Window::change_stamp),
                Some(frame.change_stamp),
            )
        } else if let Some(record) = self.deleted_windows.get(&window_id) {
            let frame_stamp = self
                .any_window_frame_id(window_id)
                .and_then(|fid| self.frames.get(&fid))
                .map(|frame| frame.change_stamp);
            (record.old_buffer, record.change_stamp, frame_stamp)
        } else {
            return WindowOldBuffer::NeverRecorded;
        };

        match old_buffer {
            None => WindowOldBuffer::NeverRecorded,
            Some(buffer) if stamp == frame_stamp => WindowOldBuffer::Recorded(buffer),
            Some(_) => WindowOldBuffer::StaleEpoch,
        }
    }

    /// Numeric object state used by GNU window-configuration restoration.
    /// A live leaf supplies its outgoing buffer and epoch; a resurrected leaf
    /// retains its deletion record. Reads require an immutable FrameManager
    /// borrow within the owning Context and add no shared or thread-local state.
    fn window_restore_change_record(&self, window_id: WindowId) -> Option<DeletedWindowRecord> {
        self.lookup_window(window_id)
            .map(|window| {
                Self::deletion_record(self.find_valid_window_frame_id(window_id), Some(window))
            })
            .or_else(|| self.deleted_windows.get(&window_id).copied())
    }

    fn any_window_frame_id(&self, window_id: WindowId) -> Option<FrameId> {
        self.find_valid_window_frame_id(window_id).or_else(|| {
            self.deleted_windows
                .contains_key(&window_id)
                .then(|| self.frames.keys().copied().next())
                .flatten()
        })
    }

    fn is_window_object_id(&self, window_id: WindowId) -> bool {
        self.is_valid_window_id(window_id) || self.deleted_windows.contains_key(&window_id)
    }

    /// Read `w->new_pixel`. Mirrors GNU
    /// `Fwindow_new_pixel` (`src/window.c`). Returns `None` if the
    /// window doesn't exist or has no pending pixel size.
    fn window_new_pixel(&self, window_id: WindowId) -> Option<i64> {
        self.lookup_window(window_id).and_then(Window::new_pixel)
    }

    /// Read `w->new_total`. Mirrors GNU `Fwindow_new_total`.
    fn window_new_total(&self, window_id: WindowId) -> Option<i64> {
        self.lookup_window(window_id).and_then(Window::new_total)
    }

    /// Read `w->new_normal`. Mirrors GNU `Fwindow_new_normal`.
    fn window_new_normal(&self, window_id: WindowId) -> Value {
        self.lookup_window(window_id)
            .map(Window::new_normal)
            .unwrap_or(Value::NIL)
    }

    /// Store a checked GNU staging result before the window is mutated.
    /// Live geometry separately requires a validated WindowPixels.
    fn set_window_new_pixel(
        &mut self,
        window_id: WindowId,
        size: WindowPixelStage,
    ) -> WindowPixelStage {
        if let Some(window) = self.lookup_window_mut(window_id) {
            window.set_new_pixel(Some(i64::from(size)));
        }
        size
    }

    /// Store the canonical GNU fixnum total, replacing the staged value or
    /// adding to it (`Fset_window_new_total`). Fails only when a staged total
    /// outside the fixnum range would have to be added to.
    fn set_window_new_total(
        &mut self,
        window_id: WindowId,
        size: WindowTotal,
        update: NewTotalUpdate,
    ) -> Result<WindowTotal, neovm_value::tagged::value::FixnumRangeError> {
        let Some(window) = self.lookup_window_mut(window_id) else {
            return Ok(size);
        };
        let stored = match update {
            NewTotalUpdate::Replace => size,
            NewTotalUpdate::Add => {
                WindowTotal::try_from(window.new_total().unwrap_or(0))?.gnu_added(size)
            }
        };
        window.set_new_total(Some(i64::from(stored)));
        Ok(stored)
    }

    /// Write `w->new_normal`. Mirrors GNU `Fset_window_new_normal`.
    fn set_window_new_normal(&mut self, window_id: WindowId, value: Value) {
        if let Some(window) = self.lookup_window_mut(window_id) {
            window.set_new_normal(value);
        }
    }

    fn window_display_state(
        &self,
        window_id: WindowId,
    ) -> Option<(&Frame, &WindowDisplayState, bool)> {
        self.frames.values().find_map(|frame| {
            frame.find_window(window_id).and_then(|window| {
                window
                    .display()
                    .map(|display| (frame, display, frame.minibuffer_window == Some(window_id)))
            })
        })
    }

    fn window_display_location(&self, window_id: WindowId) -> Option<(super::FrameId, bool)> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let frame = self.get(frame_id)?;
        Some((frame_id, frame.minibuffer_window == Some(window_id)))
    }

    /// Return WINDOW-ID's Lisp-visible vertical scroll amount.
    fn window_vscroll(&self, window_id: WindowId, pixelwise: bool) -> Option<Value> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let frame = self.get(frame_id)?;
        let Window::Leaf { vscroll, .. } = frame.find_window(window_id)? else {
            return None;
        };
        if frame.effective_window_system().is_none() {
            return Some(Value::fixnum(0));
        }
        let pixels = -i64::from(*vscroll);
        Some(if pixelwise {
            Value::fixnum(pixels)
        } else {
            canon_y_from_pixel_y(frame, pixels)
        })
    }

    /// Set WINDOW-ID's vertical scroll amount and return the Lisp-visible value.
    fn set_window_vscroll(
        &mut self,
        window_id: WindowId,
        vscroll: f64,
        pixelwise: bool,
        preserve_vscroll_p: bool,
    ) -> Option<Value> {
        let frame_id = self.find_window_frame_id(window_id)?;
        let line_height = self
            .get(frame_id)
            .map(frame_line_height)
            .unwrap_or(1)
            .max(1);
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return Some(Value::fixnum(0));
        }

        let next_pixels = if pixelwise {
            vscroll.trunc() as i64
        } else {
            (vscroll * line_height as f64).trunc() as i64
        };
        let next_raw = std::cmp::min(-next_pixels, 0).clamp(i64::from(i32::MIN), 0) as i32;

        let frame = self.get_mut(frame_id)?;
        if let Some(Window::Leaf {
            vscroll,
            preserve_vscroll_p: preserve,
            ..
        }) = frame.find_window_mut(window_id)
        {
            let changed = *vscroll != next_raw;
            *vscroll = next_raw;
            *preserve = preserve_vscroll_p;
            if changed {
                frame.remove_redisplay_snapshot(window_id);
            }
        }

        let frame = self.get(frame_id)?;
        if frame.effective_window_system().is_none() {
            return Some(Value::fixnum(0));
        }
        let Window::Leaf { vscroll, .. } = frame.find_window(window_id)? else {
            return None;
        };
        let pixels = -i64::from(*vscroll);
        Some(if pixelwise {
            Value::fixnum(pixels)
        } else {
            canon_y_from_pixel_y(frame, pixels)
        })
    }

    /// Return window display table object for WINDOW-ID, or nil when unset.
    fn window_display_table(&self, window_id: WindowId) -> Value {
        self.window_display_state(window_id)
            .map(|(_, display, _)| display.display_table)
            .unwrap_or(Value::NIL)
    }

    /// Set window display table object for WINDOW-ID.
    fn set_window_display_table(&mut self, window_id: WindowId, table: Value) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.display_table = table;
        }
    }

    /// Return window cursor-type object for WINDOW-ID.
    ///
    /// GNU Emacs defaults to `t` when no explicit per-window cursor-type is set.
    fn window_cursor_type(&self, window_id: WindowId) -> Value {
        self.window_display_state(window_id)
            .map(|(_, display, _)| display.cursor_type)
            .unwrap_or(Value::T)
    }

    /// Set window cursor-type object for WINDOW-ID.
    fn set_window_cursor_type(&mut self, window_id: WindowId, cursor_type: Value) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.cursor_type = cursor_type;
        }
    }

    /// Return whether WINDOW-ID's cursor is logically visible.
    fn window_cursor_visible(&self, window_id: WindowId) -> bool {
        self.window_display_state(window_id)
            .map(|(_, display, _)| !display.cursor_off_p)
            .unwrap_or(true)
    }

    /// Set whether WINDOW-ID's cursor is logically visible.
    ///
    /// This mirrors GNU's `w->cursor_off_p`: redisplay may continue to own a
    /// physical cursor geometry while Lisp has requested that it stay hidden.
    fn set_window_cursor_visible(&mut self, window_id: WindowId, visible: bool) {
        if let Some((frame_id, _)) = self.window_display_location(window_id)
            && let Some(display) = self
                .get_mut(frame_id)
                .and_then(|frame| frame.find_window_mut(window_id))
                .and_then(Window::display_mut)
        {
            display.cursor_off_p = !visible;
        }
    }

    /// Return the effective fringe widths and raw flags for WINDOW-ID.
    fn window_fringes(&self, window_id: WindowId) -> Option<(i64, i64, bool, bool)> {
        let (frame, display, _) = self.window_display_state(window_id)?;
        if frame.effective_window_system().is_none() {
            return Some((0, 0, false, false));
        }
        Some((
            i64::from(if display.left_fringe_width >= 0 {
                display.left_fringe_width
            } else {
                frame_default_left_fringe_width(frame)
            }),
            i64::from(if display.right_fringe_width >= 0 {
                display.right_fringe_width
            } else {
                frame_default_right_fringe_width(frame)
            }),
            display.fringes_outside_margins,
            display.fringes_persistent,
        ))
    }

    /// Set raw window fringe settings. Returns true when the visible fringe state changed.
    fn set_window_fringes(
        &mut self,
        window_id: WindowId,
        left_width: Option<i32>,
        right_width: Option<i32>,
        outside_margins: bool,
        persistent: bool,
    ) -> bool {
        let previous = self.window_fringes(window_id);
        let Some((frame_id, _)) = self.window_display_location(window_id) else {
            return false;
        };
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return false;
        }
        if let Some(display) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::display_mut)
        {
            display.left_fringe_width = left_width.unwrap_or(-1);
            display.right_fringe_width = right_width.unwrap_or(-1);
            display.fringes_outside_margins = outside_margins;
            display.fringes_persistent = persistent;
        }
        self.window_fringes(window_id) != previous
    }

    /// Return raw window scroll-bar settings plus effective area metrics.
    fn window_scroll_bars(
        &self,
        window_id: WindowId,
    ) -> Option<(Value, i64, Value, Value, i64, Value, bool)> {
        let (frame, display, is_minibuffer) = self.window_display_state(window_id)?;
        Some((
            if display.scroll_bar_width >= 0 {
                Value::fixnum(i64::from(display.scroll_bar_width))
            } else {
                Value::NIL
            },
            vertical_scroll_bar_cols(frame, display),
            display.vertical_scroll_bar_type,
            if display.scroll_bar_height >= 0 {
                Value::fixnum(i64::from(display.scroll_bar_height))
            } else {
                Value::NIL
            },
            horizontal_scroll_bar_lines(frame, display, is_minibuffer),
            display.horizontal_scroll_bar_type,
            display.scroll_bars_persistent,
        ))
    }

    /// Return the effective vertical scroll-bar area width in pixels.
    fn window_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| vertical_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective left vertical scroll-bar area width in pixels.
    fn window_left_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| left_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective right vertical scroll-bar area width in pixels.
    fn window_right_scroll_bar_area_width(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, _)| right_scroll_bar_area_width(frame, display))
            .unwrap_or(0)
    }

    /// Return the effective horizontal scroll-bar area height in pixels.
    fn window_scroll_bar_area_height(&self, window_id: WindowId) -> i64 {
        self.window_display_state(window_id)
            .map(|(frame, display, is_minibuffer)| {
                horizontal_scroll_bar_area_height(frame, display, is_minibuffer)
            })
            .unwrap_or(0)
    }

    /// Set raw window scroll-bar settings. Returns true when the visible scroll-bar state changed.
    fn set_window_scroll_bars(
        &mut self,
        window_id: WindowId,
        width: Option<i32>,
        vertical_type: Value,
        height: Option<i32>,
        horizontal_type: Value,
        persistent: bool,
    ) -> bool {
        let previous = self.window_scroll_bars(window_id);
        let Some((frame_id, is_minibuffer)) = self.window_display_location(window_id) else {
            return false;
        };
        if self
            .get(frame_id)
            .is_none_or(|frame| frame.effective_window_system().is_none())
        {
            return false;
        }
        let mut next_vertical_type = vertical_type;
        if width == Some(0) {
            next_vertical_type = Value::NIL;
        }
        let mut next_horizontal_type = horizontal_type;
        if height == Some(0)
            || (is_minibuffer
                && HorizontalScrollBarType::from_symbol_value(&next_horizontal_type).is_none())
        {
            next_horizontal_type = Value::NIL;
        }
        if let Some(display) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::display_mut)
        {
            display.scroll_bar_width = width.unwrap_or(-1);
            display.vertical_scroll_bar_type = next_vertical_type;
            display.scroll_bar_height = height.unwrap_or(-1);
            display.horizontal_scroll_bar_type = next_horizontal_type;
            display.scroll_bars_persistent = persistent;
        }
        self.window_scroll_bars(window_id) != previous
    }

    /// Apply the window-owned state changes that GNU performs during
    /// `set_window_buffer`.
    fn apply_set_window_buffer_state(
        &mut self,
        window_id: WindowId,
        buffer_id: BufferId,
        window_start: LispCharPos1,
        point: LispCharPos1,
        preserve_display_state: bool,
        defaults: WindowBufferDisplayDefaults,
    ) {
        let Some(frame_id) = self.find_window_frame_id(window_id) else {
            return;
        };
        let (fringes_persistent, scroll_bars_persistent) = self
            .get(frame_id)
            .and_then(|frame| frame.find_window(window_id))
            .and_then(Window::display)
            .map(|display| (display.fringes_persistent, display.scroll_bars_persistent))
            .unwrap_or((false, false));

        if let Some(Window::Leaf {
            buffer_id: leaf_buffer_id,
            mode_line_base,
            window_start: leaf_window_start,
            position_markers,
            window_end,
            point: leaf_point,
            old_point,
            hscroll,
            min_hscroll,
            suspend_auto_hscroll,
            vscroll,
            preserve_vscroll_p,
            margins,
            ..
        }) = self
            .get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
        {
            // GNU wset_buffer invalidates the numeric line base for every
            // install, including the same buffer and identity round trips.
            *mode_line_base = None;
            *leaf_buffer_id = buffer_id;
            *leaf_window_start = window_start.max(LispCharPos1::ONE);
            *position_markers = super::WindowPositionMarkerState::Detached;
            // GNU `set_window_buffer` zeroes `w->window_end_pos` and
            // `w->window_end_vpos` for every buffer install, same buffer or
            // not (src/window.c:4360-4361): the recorded end is an offset
            // from Z and describes rows of the buffer that produced it.  An
            // unrecorded end reads back as Z, which is GNU's zero offset.
            // Kept across a buffer switch, the old buffer's end was
            // reinterpreted in the new one and told the viewport policy
            // that point lay hundreds of lines below the window, so a
            // `set-window-start` NOFORCE near EOB was recentered where
            // GNU's "same window start" branch (src/xdisp.c:20972) keeps it.
            *window_end = super::WindowEndState::Unrecorded;
            *leaf_point = point.max(LispCharPos1::ONE);
            if !preserve_display_state {
                *old_point = point.max(LispCharPos1::ONE);
                *hscroll = 0;
                // GNU resets the auto-hscroll lower bound and clears the
                // suspend flag on buffer switch (src/window.c:4368).
                *min_hscroll = 0;
                *suspend_auto_hscroll = false;
                *vscroll = 0;
                *preserve_vscroll_p = false;
            }
            if let Some(next_margins) = defaults.margins {
                *margins = next_margins;
            }
        }

        if let Some(next_fringes) = defaults.fringes
            && !fringes_persistent
        {
            let _ = self.set_window_fringes(
                window_id,
                next_fringes.left_width,
                next_fringes.right_width,
                next_fringes.outside_margins,
                false,
            );
        }
        if let Some(next_scroll_bars) = defaults.scroll_bars
            && !scroll_bars_persistent
        {
            let _ = self.set_window_scroll_bars(
                window_id,
                next_scroll_bars.vertical_width,
                next_scroll_bars.vertical_type,
                next_scroll_bars.horizontal_height,
                next_scroll_bars.horizontal_type,
                false,
            );
        }
    }

    fn live_window_parameters(&self, window_id: WindowId) -> Option<&[(Value, Value)]> {
        self.frames
            .values()
            .find_map(|frame| frame.find_window(window_id).map(Window::parameters))
            .map(Vec::as_slice)
    }

    fn live_window_parameters_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut Vec<(Value, Value)>> {
        let frame_id = self.find_valid_window_frame_id(window_id)?;
        self.get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .map(Window::parameters_mut)
    }

    /// Return window parameter KEY for WINDOW-ID, or nil when unset.
    fn window_parameter(&self, window_id: WindowId, key: &Value) -> Option<Value> {
        self.live_window_parameters(window_id)
            .or_else(|| {
                self.deleted_window_parameters
                    .get(&window_id)
                    .map(Vec::as_slice)
            })
            .and_then(|pairs| {
                pairs
                    .iter()
                    .find(|(existing_key, _)| eq_value(existing_key, key))
                    .map(|(_, value)| *value)
            })
    }

    /// Set window parameter KEY on WINDOW-ID to VALUE.
    fn set_window_parameter(&mut self, window_id: WindowId, key: Value, value: Value) {
        let parameters = if let Some(parameters) = self.live_window_parameters_mut(window_id) {
            parameters
        } else {
            self.deleted_window_parameters.entry(window_id).or_default()
        };

        if let Some((_, existing_value)) = parameters
            .iter_mut()
            .find(|(existing_key, _)| eq_value(existing_key, &key))
        {
            *existing_value = value;
        } else {
            parameters.insert(0, (key, value));
        }
    }

    /// Return window parameters alist for WINDOW-ID.
    fn window_parameters_alist(&self, window_id: WindowId) -> Value {
        self.live_window_parameters(window_id)
            .map(alist_from_parameters)
            .unwrap_or(Value::NIL)
    }

    /// Return WINDOW-ID's `(KEY . VALUE)` window-parameter pairs as an owned
    /// vec (empty when the window is unknown). The layout engine reads these to
    /// evaluate GNU `(:window PARAMETER VALUE)` `:filtered` face predicates,
    /// which it cannot express via the Lisp-alist accessor without re-parsing.
    fn window_parameters_pairs(&self, window_id: WindowId) -> Vec<(Value, Value)> {
        self.live_window_parameters(window_id)
            .map(<[(Value, Value)]>::to_vec)
            .unwrap_or_default()
    }

    fn live_window_history_mut(
        &mut self,
        window_id: WindowId,
    ) -> Option<&mut super::WindowHistoryState> {
        let frame_id = self.find_window_frame_id(window_id)?;
        self.get_mut(frame_id)
            .and_then(|frame| frame.find_window_mut(window_id))
            .and_then(Window::history_mut)
    }

    /// Return previous-buffer list object for WINDOW-ID, or nil when unset.
    fn window_prev_buffers(&self, window_id: WindowId) -> Value {
        self.live_window_history(window_id)
            .map(|history| history.prev_buffers)
            .unwrap_or(Value::NIL)
    }

    /// Set previous-buffer list object for WINDOW-ID.
    fn set_window_prev_buffers(&mut self, window_id: WindowId, prev_buffers: Value) {
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.prev_buffers = prev_buffers;
        }
    }

    /// Return next-buffer list object for WINDOW-ID, or nil when unset.
    fn window_next_buffers(&self, window_id: WindowId) -> Value {
        self.live_window_history(window_id)
            .map(|history| history.next_buffers)
            .unwrap_or(Value::NIL)
    }

    /// Set next-buffer list object for WINDOW-ID.
    fn set_window_next_buffers(&mut self, window_id: WindowId, next_buffers: Value) {
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.next_buffers = next_buffers;
        }
    }

    /// Mark WINDOW-ID as the most recently selected window.
    fn note_window_selected(&mut self, window_id: WindowId) -> i64 {
        self.window_select_count = self.window_select_count.saturating_add(1);
        let next_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.use_time = next_use_time;
        }
        next_use_time
    }

    /// Mark WINDOW-ID as second-most recently used.
    ///
    /// Returns the new use-time of WINDOW-ID when the bump happened, nil-like
    /// behavior (`None`) otherwise.
    fn bump_window_use_time(
        &mut self,
        selected_window_id: WindowId,
        window_id: WindowId,
    ) -> Option<i64> {
        if window_id == selected_window_id {
            return None;
        }
        if self.window_use_time(selected_window_id) != self.window_select_count {
            return None;
        }

        let bumped_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(window_id) {
            history.use_time = bumped_use_time;
        }
        self.window_select_count = self.window_select_count.saturating_add(1);
        let selected_use_time = self.window_select_count;
        if let Some(history) = self.live_window_history_mut(selected_window_id) {
            history.use_time = selected_use_time;
        }
        Some(bumped_use_time)
    }

    /// Return the old selected window, when tracked.
    fn old_selected_window(&self) -> Option<WindowId> {
        self.old_selected_window
    }
}

/// PC2 entry for the evaluator's native-resize path: clamp window sizes to
/// their buffers, then resize the frame in pixels.
#[doc(hidden)]
pub fn resize_pixelwise_with_buffer_constraints(
    frame: &mut Frame,
    buffers: &BufferManager,
    width: u32,
    height: u32,
) {
    DisplayFrameExt::resize_pixelwise_with_buffer_constraints(frame, buffers, width, height);
}

// ---------------------------------------------------------------------------
// Display test fixtures (peer 20:16Z condition): the three former pub-trait
// methods are now pub(crate) FREE FUNCTIONS taking &mut Frame. The trait no
// longer exposes any test method, so the signed
// `neovm_display::test_support::authority` delegates are the only public
// test-support route. Display's own tests keep method-call syntax through the
// crate-private cfg(test) `DisplayFrameTestExt` convenience trait below.

/// Install a completed redisplay and replay its live output state.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn commit_redisplay_cache_for_test(
    frame: &mut Frame,
    snapshots: Vec<WindowDisplaySnapshot>,
) {
    let generation = frame
        .presentation_state
        .last_identity
        .unwrap_or_else(|| geometry::PresentationId::new(1));
    let publications: Vec<_> = snapshots
        .iter()
        .cloned()
        .map(WindowPresentationSnapshot::live)
        .collect();
    frame.commit_completed_window_output(generation, &publications);
    replace_redisplay_cache_for_test(frame, snapshots);
}

/// Synchronous prepare/activate lifecycle fixture.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn prepare_and_activate_display_presentation_for_test(
    frame: &mut Frame,
    presentation: geometry::PresentationId,
    snapshots: Vec<WindowDisplaySnapshot>,
) -> Result<(), geometry::PresentationPrepareError> {
    frame.prepare_live_window_presentation(presentation, snapshots)?;
    frame
        .activate_display_presentation(presentation)
        .expect("a successfully prepared presentation must activate");
    Ok(())
}

/// Replace only the latest redisplay cache, without mutating live cursor or
/// output state.
#[cfg(any(test, feature = "test-support"))]
pub(crate) fn replace_redisplay_cache_for_test(
    frame: &mut Frame,
    snapshots: Vec<WindowDisplaySnapshot>,
) {
    frame.redisplay_cache = snapshots
        .into_iter()
        .filter(|snapshot| frame.find_window(snapshot.window_id).is_some())
        .map(|snapshot| (snapshot.window_id, std::sync::Arc::new(snapshot)))
        .collect();
}

/// Crate-private test convenience: keeps display's own test call sites on
/// method syntax. Not a public route (cfg(test), pub(crate)); the AUTHORITY
/// fence remains the only cross-crate path.
#[cfg(test)]
pub(crate) trait DisplayFrameTestExt {
    fn commit_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>);
    fn prepare_and_activate_display_presentation_for_test(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError>;
    fn replace_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>);
}

#[cfg(test)]
impl DisplayFrameTestExt for Frame {
    fn commit_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>) {
        commit_redisplay_cache_for_test(self, snapshots);
    }
    fn prepare_and_activate_display_presentation_for_test(
        &mut self,
        presentation: geometry::PresentationId,
        snapshots: Vec<WindowDisplaySnapshot>,
    ) -> Result<(), geometry::PresentationPrepareError> {
        prepare_and_activate_display_presentation_for_test(self, presentation, snapshots)
    }
    fn replace_redisplay_cache_for_test(&mut self, snapshots: Vec<WindowDisplaySnapshot>) {
        replace_redisplay_cache_for_test(self, snapshots);
    }
}
