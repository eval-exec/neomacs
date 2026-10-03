//! TTY current-matrix admission independent of physical numeric frame backing.
//! One exclusive Frame mutator publishes initialized Arc values. Readers borrow
//! immutable numeric records; several Lisp mutators must use the existing
//! exclusive Frame ownership discipline. No record contains Lisp values or TLS.
use super::*;
use neomacs_display_protocol::posn_frame_pool::PosnFramePool;

/// Which existing numeric producer GNU's current rows presently admit.
/// Copy-only state may be read concurrently through immutable Frame publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TtyCurrentMatrixAuthority {
    AcceptedLocal,
    FramePoolPartition,
    Undrawn,
}

/// GNU desired-matrix allocation facts, not source/display-body freshness.
/// Numeric values are initialized by the exclusive Frame mutator and immutable
/// to readers; no query advances this allocation observation.
#[derive(Clone, Debug, PartialEq, Eq)]
struct TtyMatrixAllocation {
    window: WindowId,
    x: i64,
    y: i64,
    width: i64,
    height: i64,
    left_margin_glyphs: i64,
    right_margin_glyphs: i64,
}

/// Same inline Option<Arc<_>> representation as the prior pool-only owner.
/// COW copies small numeric allocation/authority records and the pool Arc,
/// never the retained pool's row/cell vectors. All writes require &mut Frame.
#[derive(Clone, Debug)]
pub(super) struct TtyPosnCurrentOwner {
    pub(super) pool: Option<Arc<PosnFramePool>>,
    allocation: Vec<TtyMatrixAllocation>,
    dimensions: (i64, i64),
    authority: HashMap<WindowId, TtyCurrentMatrixAuthority>,
}

impl Frame {
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

    /// Successful current producer admission; geometry-only snapshots retain
    /// prior rows. Caller has already checked extent ON, TTY and prepare identity.
    #[cold]
    #[inline(never)]
    pub(super) fn note_tty_current_matrix_publication(
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
    pub(crate) fn tty_posn_apply_window_adjustment(&mut self, window: WindowId) {
        if !posn_object_extent_mode().enabled() || self.effective_window_system().is_some() {
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
    pub(crate) fn tty_posn_adjust_current_matrices(&mut self) {
        if !posn_object_extent_mode().enabled()
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

    /// Allocation-free numeric admission read. Local and pooled current rows
    /// share the same clear authority; callers have already selected extent ON.
    #[inline]
    pub(super) fn tty_posn_current_matrix_authority(
        &self,
        window: WindowId,
    ) -> Option<TtyCurrentMatrixAuthority> {
        self.tty_posn_pool.as_ref()?.authority.get(&window).copied()
    }
}
