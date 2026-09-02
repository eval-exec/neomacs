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

pub(crate) fn current_layout_frame_id(evaluator: &Context) -> Option<FrameId> {
    REDISPLAY_RUNTIME.with(|runtime| runtime.current_frame_id(evaluator))
}

pub fn layout_frame_display_state(
    evaluator: &mut Context,
    frame_id: FrameId,
    purpose: FrameLayoutPurpose,
) -> Option<PreparedFrameDisplay> {
    REDISPLAY_RUNTIME.with(|runtime| runtime.prepare_frame(evaluator, frame_id, purpose))
}

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
pub fn install_window_layout_query_fn(evaluator: &mut Context) {
    REDISPLAY_RUNTIME.with(|runtime| runtime.install_window_layout_query_hook(evaluator));
}

// ── TTY layout tree and redisplay ─────────────────────────────────────────

pub fn run_tty_layout_tree(
    evaluator: &mut Context,
) -> Option<(SealedFramePresentation, Vec<SealedFramePresentation>)> {
    let selected = current_layout_frame_id(evaluator)?;
    let root_id = evaluator
        .frame_manager()
        .root_frame_id(selected)
        .unwrap_or(selected);
    let frame_order = evaluator
        .frame_manager()
        .frames_in_reverse_z_order(root_id, RenderFrameVisibility::VisibleOnly);

    let root_state = layout_frame_display_state(evaluator, root_id, FrameLayoutPurpose::Redisplay)?
        .activate(evaluator)
        .ok()?;

    let mut child_states = Vec::new();
    for frame_id in frame_order {
        if frame_id == root_id {
            continue;
        }
        let Some(prepared) =
            layout_frame_display_state(evaluator, frame_id, FrameLayoutPurpose::Redisplay)
        else {
            continue;
        };
        let Ok(state) = prepared.activate(evaluator) else {
            continue;
        };
        child_states.push(state);
    }

    Some((root_state, child_states))
}

/// Rasterize the display state into a `TtyRif` and write ANSI output to stdout.
pub fn run_tty_rif_redisplay(
    tty_rif: &mut TtyRif,
    root: &SealedFramePresentation,
    children: &[SealedFramePresentation],
) {
    let mut stdout = std::io::stdout();
    run_tty_rif_redisplay_to(tty_rif, root, children, &mut stdout);
}

pub fn run_tty_rif_redisplay_to(
    tty_rif: &mut TtyRif,
    root: &SealedFramePresentation,
    children: &[SealedFramePresentation],
    output: &mut impl std::io::Write,
) {
    tty_rif.rasterize_presentations(root, children);
    tty_rif.diff_and_render();
    let bytes = tty_rif.take_output();
    tracing::debug!("tty_rif: output {} bytes", bytes.len());
    if !bytes.is_empty() {
        let _ = output.write_all(&bytes);
        let _ = output.flush();
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
            run_tty_rif_redisplay(&mut tty_rif, &root, &children);
        }
    }));
    install_frame_snapshot_fn(evaluator);
    install_window_layout_query_fn(evaluator);
}
