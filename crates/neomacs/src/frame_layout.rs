//! Frame layout tree construction and redisplay callback, shared by both the
//! GUI and TTY frontends.
//!
//! This module used to provide the `tty-child-frames` feature on live-TTY
//! startup.  It does not any more: `features` is decided in exactly one place,
//! `crates/neovm-core/src/emacs_core/system/platform/c_features/mod.rs`, the way GNU decides it with one
//! `#ifdef` per feature.  Ledger 197.
//!
//! Mirrors the TTY child-frame compositing in GNU `src/dispnew.c`
//! (`combine_updates_for_frame`) and the redisplay callback wiring that
//! normally lives in `src/xdisp.c` / `src/dispnew.c`.

use neomacs_app::presentation::{EditorPresentationRuntime, PresentationMetrics};
pub use neomacs_app::presentation::{FrameLayoutPurpose, PreparedFrameDisplay};
use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_runtime::backend::tty::rif::TtyRif;
use neovm_core::emacs_core::eval::Context;
use neovm_core::window::{FrameId, RenderFrameVisibility};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::StartupOptions;
use super::tty_init;

thread_local! {
    /// Start without font metrics to avoid the ~500ms cosmic-text font
    /// database scan on first access. The GUI path enables cosmic metrics
    /// explicitly; the TTY path leaves it disabled.
    pub static REDISPLAY_RUNTIME: EditorPresentationRuntime =
        EditorPresentationRuntime::new(PresentationMetrics::CellGrid);
}

// ── Layout helpers ────────────────────────────────────────────────────────

#[cfg(test)]
pub(crate) fn current_layout_frame_id(evaluator: &Context) -> Option<FrameId> {
    REDISPLAY_RUNTIME.with(|runtime| runtime.current_frame_id(evaluator))
}

#[cfg(test)]
pub fn layout_frame_display_state(
    evaluator: &mut Context,
    frame_id: FrameId,
    purpose: FrameLayoutPurpose,
) -> Option<PreparedFrameDisplay> {
    REDISPLAY_RUNTIME
        .with(|runtime| layout_frame_display_state_with(runtime, evaluator, frame_id, purpose))
}

pub fn layout_frame_display_state_with(
    runtime: &EditorPresentationRuntime,
    evaluator: &mut Context,
    frame_id: FrameId,
    purpose: FrameLayoutPurpose,
) -> Option<PreparedFrameDisplay> {
    runtime.prepare_frame(evaluator, frame_id, purpose)
}

#[cfg(test)]
pub fn publish_visible_frames(
    evaluator: &mut Context,
    try_publish: impl FnMut(SealedFramePresentation) -> bool,
) -> neomacs_app::presentation::FramePublishResult {
    REDISPLAY_RUNTIME.with(|runtime| runtime.publish_visible_frames(evaluator, try_publish))
}

/// Install the `neomacs--frame-snapshot` hook (`Context::frame_snapshot_fn`).
///
/// Called by both frontends right where they install `redisplay_fn`; batch
/// mode installs nothing, so the subr signals "no display attached" there.
pub fn install_frame_snapshot_fn(evaluator: &mut Context) {
    REDISPLAY_RUNTIME.with(|runtime| runtime.install_frame_snapshot_hook(evaluator));
}

/// Install the synchronous layout-query adapter used by display primitives
/// such as `(window-end WINDOW t)` and `posn-at-point`.
///
/// This targets one window through the canonical row producer without entering
/// the renderer presentation lifecycle. Both GUI and TTY install this adapter;
/// batch mode intentionally does not.
/// Issue #447: install the font-shaping driver (GNU `font->driver->shape`)
/// on the evaluator. The driver reenters the redisplay runtime and shapes
/// ligature/composition gstrings through the layout engine's font system —
/// the same cosmic machinery the row walk uses, so the font's `liga`
/// feature applies.
pub fn install_font_shape_driver(evaluator: &mut Context) {
    REDISPLAY_RUNTIME.with(|runtime| runtime.install_font_shape_driver(evaluator));
}

pub fn install_window_layout_query_fn(evaluator: &mut Context) {
    evaluator.display_idle_maintenance_fn = Some(Box::new(|eval| {
        REDISPLAY_RUNTIME.with(|runtime| runtime.maintain_scroll_coverage(eval))
    }));
    REDISPLAY_RUNTIME.with(|runtime| runtime.install_window_layout_query_hook(evaluator));
}

// ── TTY layout tree and redisplay ─────────────────────────────────────────

pub fn run_tty_layout_tree(
    evaluator: &mut Context,
) -> Option<(SealedFramePresentation, Vec<SealedFramePresentation>)> {
    REDISPLAY_RUNTIME.with(|runtime| run_tty_layout_tree_with(runtime, evaluator))
}

pub fn run_tty_layout_tree_with(
    runtime: &EditorPresentationRuntime,
    evaluator: &mut Context,
) -> Option<(SealedFramePresentation, Vec<SealedFramePresentation>)> {
    let selected = runtime.current_frame_id(evaluator)?;
    let root_id = evaluator
        .frame_manager()
        .root_frame_id(selected)
        .unwrap_or(selected);
    let frame_order = evaluator
        .frame_manager()
        .frames_in_reverse_z_order(root_id, RenderFrameVisibility::VisibleOnly);

    if neovm_core::emacs_core::xdisp::mode_line_flow_enabled() {
        return prepare_tty_tree_before_activation(runtime, evaluator, root_id, frame_order);
    }

    let root_state = layout_frame_display_state_with(
        runtime,
        evaluator,
        root_id,
        FrameLayoutPurpose::Redisplay,
    )?
    .activate(evaluator)
    .ok()?;

    let mut child_states = Vec::new();
    for frame_id in frame_order {
        if frame_id == root_id {
            continue;
        }
        let prepared = layout_frame_display_state_with(
            runtime,
            evaluator,
            frame_id,
            FrameLayoutPurpose::Redisplay,
        );
        if evaluator.has_mode_line_display_flow() {
            // The redisplay driver returns the Context-owned exit. Neither
            // primary nor auxiliary TTY may rasterize a partial frame tree.
            return None;
        }
        let Some(prepared) = prepared else {
            continue;
        };
        let Ok(state) = prepared.activate(evaluator) else {
            continue;
        };
        child_states.push(state);
    }

    Some((root_state, child_states))
}

/// A mode-line exit must leave every frame's previously active presentation
/// intact. This call-local staging belongs to the Context's current mutator;
/// no prepared ticket is activated until every child has finished evaluation.
fn prepare_tty_tree_before_activation(
    runtime: &EditorPresentationRuntime,
    evaluator: &mut Context,
    root_id: FrameId,
    frame_order: Vec<FrameId>,
) -> Option<(SealedFramePresentation, Vec<SealedFramePresentation>)> {
    let root = layout_frame_display_state_with(
        runtime,
        evaluator,
        root_id,
        FrameLayoutPurpose::Redisplay,
    )?;
    let mut children: Vec<PreparedFrameDisplay> = Vec::new();
    for frame_id in frame_order {
        if frame_id == root_id {
            continue;
        }
        let child = layout_frame_display_state_with(
            runtime,
            evaluator,
            frame_id,
            FrameLayoutPurpose::Redisplay,
        );
        if evaluator.has_mode_line_display_flow() {
            // Discard all tickets before the driver returns the original Flow.
            // Both primary and auxiliary TTYs use this tree producer.
            root.discard(evaluator);
            for prepared in children {
                prepared.discard(evaluator);
            }
            if let Some(prepared) = child {
                prepared.discard(evaluator);
            }
            return None;
        }
        if let Some(child) = child {
            children.push(child);
        }
    }
    let root = match root.activate(evaluator) {
        Ok(root) => root,
        Err(_) => {
            for prepared in children {
                prepared.discard(evaluator);
            }
            return None;
        }
    };
    let children = children
        .into_iter()
        .filter_map(|prepared| prepared.activate(evaluator).ok())
        .collect();
    Some((root, children))
}

/// Rasterize the display state into a `TtyRif` and write ANSI output to stdout.
pub fn run_tty_rif_redisplay(
    tty_rif: &mut TtyRif,
    root: &SealedFramePresentation,
    children: &[SealedFramePresentation],
) {
    tty_rif.rasterize_presentations(root, children);
    #[cfg(windows)]
    let result = super::tty_output::windows::render(tty_rif);
    #[cfg(not(windows))]
    let result = super::tty_output::primary()
        .map_err(std::io::Error::other)
        .and_then(|caps| super::tty_output::render_to(tty_rif, &mut std::io::stdout(), caps));
    if let Err(error) = result {
        tracing::error!(%error, "TTY redisplay failed");
    }
}

pub fn run_tty_rif_redisplay_to(
    tty_rif: &mut TtyRif,
    root: &SealedFramePresentation,
    children: &[SealedFramePresentation],
    output: &mut impl std::io::Write,
    capabilities: &super::tty_output::Capabilities,
) {
    tty_rif.rasterize_presentations(root, children);
    if let Err(error) = super::tty_output::paint_to(tty_rif, output, capabilities) {
        tracing::error!(%error, "secondary TTY redisplay failed");
    }
}

// ── Redisplay callback installation ───────────────────────────────────────

/// Install the TTY redisplay callback that drives `TtyRif` rasterization.
///
/// This function wires up:
/// 1. A `TtyRif` with the current terminal dimensions.
/// 2. Disables cosmic-text metrics (TTY uses 1×1 char cells).
/// 3. Sets `evaluator.redisplay_fn` to the layout-tree → rasterize → render
///    pipeline.
#[cfg(test)]
pub fn install_tty_redisplay_callback(evaluator: &mut Context, startup: &StartupOptions) {
    install_tty_redisplay_callback_with_popup_redraw(evaluator, startup, None, None);
}

pub(crate) type TryRenderSelectedTerminal = Box<dyn FnMut(&mut Context) -> bool>;

pub fn install_tty_redisplay_callback_with_popup_redraw(
    evaluator: &mut Context,
    startup: &StartupOptions,
    force_full_redraw: Option<Arc<AtomicBool>>,
    mut try_render_selected_auxiliary: Option<TryRenderSelectedTerminal>,
) {
    if startup.daemon.is_some() {
        // No primary terminal exists. Attached client TTYs own their renderers
        // and are the only valid destination for daemon redisplay.
        REDISPLAY_RUNTIME.with(EditorPresentationRuntime::use_cell_grid);
        evaluator.redisplay_fn = Some(Box::new(move |eval: &mut Context| {
            if let Some(render) = try_render_selected_auxiliary.as_mut() {
                render(eval);
            }
        }));
        install_frame_snapshot_fn(evaluator);
        install_window_layout_query_fn(evaluator);
        install_font_shape_driver(evaluator);
        return;
    }
    if !tty_init::should_enable_live_tty_io(startup) {
        return;
    }

    let (cols, rows) = tty_init::query_terminal_size_cells().unwrap_or((80, 25));
    let mut tty_rif = TtyRif::new_with_caps(
        cols as usize,
        rows as usize,
        super::tty_init::detect_term_caps(),
    );
    // TTY frames use 1x1 character cell metrics (GNU Emacs
    // frame.c:1184-1185). Drop the layout engine's cosmic-text
    // FontMetricsService so char_advance,
    // status_line_font_metrics, etc. fall back to the
    // char-cell grid.
    REDISPLAY_RUNTIME.with(EditorPresentationRuntime::use_cell_grid);
    evaluator.redisplay_fn = Some(Box::new(move |eval: &mut Context| {
        eval.setup_thread_locals();
        // The selected frame determines the output device.  An explicit
        // `make-terminal-frame' owns a separate TTY, so give its renderer the
        // first opportunity and touch the primary stdout terminal only when
        // the selected frame belongs there.
        if try_render_selected_auxiliary
            .as_mut()
            .is_some_and(|render| render(eval))
        {
            return;
        }
        if let Some((cols, rows)) = tty_init::query_terminal_size_cells() {
            let cols = cols as usize;
            let rows = rows as usize;
            if tty_rif.width() != cols || tty_rif.height() != rows {
                tty_rif.resize(cols, rows);
            }
        }
        if force_full_redraw
            .as_ref()
            .is_some_and(|force| force.swap(false, Ordering::AcqRel))
        {
            tty_rif.force_redraw();
        }
        if let Some((root, children)) = run_tty_layout_tree(eval) {
            // Consume only physically prepared frames, after layout and just
            // before repaint. Pending requests for unrendered devices remain
            // Context-owned; no new frontend Lisp cache or TLS is introduced.
            let mut full_redraw =
                eval.gnu_take_tty_frame_redraw(FrameId(root.frame_placement.frame().get()));
            for child in &children {
                full_redraw |=
                    eval.gnu_take_tty_frame_redraw(FrameId(child.frame_placement.frame().get()));
            }
            if full_redraw {
                tty_rif.force_redraw();
            }
            run_tty_rif_redisplay(&mut tty_rif, &root, &children);
        }
    }));
    install_frame_snapshot_fn(evaluator);
    install_window_layout_query_fn(evaluator);
    install_font_shape_driver(evaluator);
}

#[cfg(test)]
#[path = "tests/tty_mode_line_flow_test.rs"]
mod tty_mode_line_flow_test;
