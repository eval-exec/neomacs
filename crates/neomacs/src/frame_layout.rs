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

use neomacs_display_protocol::SealedFramePresentation;
use neomacs_display_protocol::glyph_matrix::FrameDisplayState;
use neomacs_display_runtime::backend::tty::rif::TtyRif;
use neomacs_display_runtime::redisplay::RedisplayRuntime;
pub use neomacs_display_runtime::redisplay::{FrameLayoutPurpose, PreparedFrameDisplay};
use neomacs_display_runtime::tty_input::{
    TerminalCellSize, TerminalColumns, TerminalRows, TerminalSizeQuery,
};
use neovm_core::emacs_core::eval::Context;
use neovm_core::window::{FrameId, RenderFrameScope, RenderFrameVisibility};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::StartupOptions;
use super::tty_init;

thread_local! {
    /// Start without font metrics to avoid the ~500ms cosmic-text font
    /// database scan on first access. The GUI path enables cosmic metrics
    /// explicitly; the TTY path leaves it disabled.
    ///
    /// The runtime lives on the heap behind a fixed-size handle. Stored inline,
    /// its ~19 KB were part of every thread's static TLS block, so each new
    /// field grew TLS and moved the `%fs` offsets of every later thread-local
    /// (no-new-TLS-bytes rule; b184 measured +16 bytes from two new fields).
    static REDISPLAY_RUNTIME: Box<RedisplayRuntime> =
        Box::new(RedisplayRuntime::new_without_font_metrics());
}

// The thread-local slot holds only the handle, whatever `RedisplayRuntime` grows to.
const _: () = assert!(std::mem::size_of::<Box<RedisplayRuntime>>() == std::mem::size_of::<usize>());

/// Configure the direct GUI owner without exposing its native runtime slot.
#[inline(always)]
pub(super) fn configure_gui_runtime(font_sizing: super::FontSizing) {
    REDISPLAY_RUNTIME.with(|runtime| {
        runtime.enable_cosmic_metrics();
        runtime.set_font_sizing(font_sizing);
    });
}

/// Resolve native preview work inside the frontend effect owner.
#[inline(always)]
pub(super) fn resolved_scroll_preview(
    evaluator: &Context,
    frame: FrameId,
    window: neovm_core::window::WindowId,
    inputs: Vec<neomacs_display_protocol::input_progress::InputReceipt>,
) -> Option<neomacs_display_protocol::scroll_coverage::ResolvedScrollIntent> {
    REDISPLAY_RUNTIME
        .with(|runtime| runtime.resolved_scroll_preview(evaluator, frame, window, inputs))
}

#[cfg(test)]
pub(super) fn enable_cosmic_metrics_for_test() {
    REDISPLAY_RUNTIME.with(|runtime| runtime.enable_cosmic_metrics());
}

#[cfg(test)]
pub(super) fn disable_cosmic_metrics_for_test() {
    REDISPLAY_RUNTIME.with(|runtime| runtime.disable_cosmic_metrics());
}

#[cfg(test)]
pub(super) fn last_layout_stats_for_test() -> neomacs_layout_engine::incremental_layout::LayoutStats
{
    REDISPLAY_RUNTIME.with(|runtime| runtime.last_layout_stats())
}

// ── Layout helpers ────────────────────────────────────────────────────────

pub(crate) fn current_layout_frame_id(evaluator: &Context) -> Option<FrameId> {
    evaluator
        .frame_manager()
        .selected_frame()
        .map(|frame| frame.id)
}

pub fn layout_frame_display_state(
    evaluator: &mut Context,
    frame_id: FrameId,
    purpose: FrameLayoutPurpose,
) -> Option<PreparedFrameDisplay> {
    REDISPLAY_RUNTIME.with(|runtime| runtime.prepare_frame(evaluator, frame_id, purpose))
}

/// Lay out the frames a snapshot request covers — freshest state, on
/// demand — preserving the canonical parent-relative placement published by
/// layout. Shared by the TTY and GUI frontends (both use the same thread-local
/// redisplay runtime).
pub fn collect_snapshot_states(
    evaluator: &mut Context,
    target: &neovm_core::emacs_core::xdisp::SnapshotTarget,
) -> Result<Vec<FrameDisplayState>, String> {
    use neovm_core::emacs_core::xdisp::SnapshotTarget;

    let selected =
        current_layout_frame_id(evaluator).ok_or_else(|| "no selected frame".to_string())?;
    let tree = evaluator
        .frame_manager()
        .render_frame_forest(
            RenderFrameScope::TreeContaining(selected),
            RenderFrameVisibility::VisibleOnly,
        )
        .into_iter()
        .next()
        .ok_or_else(|| "no render frame tree for the selected frame".to_string())?;

    let mut states = Vec::new();
    for node in tree.frames_bottom_to_top {
        let keep = match target {
            SnapshotTarget::All => true,
            SnapshotTarget::Selected => node.frame_id == selected,
            SnapshotTarget::Frame(id) => node.frame_id.0 == *id,
        };
        if !keep {
            continue;
        }
        let prepared =
            layout_frame_display_state(evaluator, node.frame_id, FrameLayoutPurpose::Snapshot);
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
            // This callback carries String errors. Keep the original Flow in
            // Context for run_frame_snapshot to return after reinstalling it.
            return Err("frame snapshot: mode-line evaluation exited nonlocally".to_string());
        }
        let Some(prepared) = prepared else {
            continue;
        };
        states.push(prepared.discard(evaluator));
    }

    // A live frame outside the selected frame's tree (another top-level
    // frame): lay it out directly with its canonical root placement.
    if states.is_empty()
        && let SnapshotTarget::Frame(id) = target
    {
        let prepared =
            layout_frame_display_state(evaluator, FrameId(*id), FrameLayoutPurpose::Snapshot);
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
            // The core drains the Context-owned Flow after this callback.
            return Err("frame snapshot: mode-line evaluation exited nonlocally".to_string());
        }
        if let Some(prepared) = prepared {
            states.push(prepared.discard(evaluator));
        }
    }

    if states.is_empty() {
        return Err("frame snapshot: no frame produced display state".to_string());
    }
    Ok(states)
}

/// JSON envelope of a snapshot: `{"frames":[FrameDisplayState...]}` — the
/// array form is uniform for one frame or many, and the wrapper leaves room
/// for future metadata without a schema break.
#[derive(serde::Serialize)]
struct SnapshotDoc<'a> {
    frames: &'a [FrameDisplayState],
}

/// Geometry-only diagnostics borrow the protocol's authoritative window data.
/// Serializing the whole state can repeat large in-memory font assets in the
/// resolved font tables and off-screen coverage (notably CoreText fonts).
#[derive(serde::Serialize)]
struct SnapshotGeometryFrame<'a> {
    presentation_id: &'a neomacs_display_protocol::frame_chrome::PresentationId,
    frame_cols: usize,
    frame_rows: usize,
    frame_pixel_width: f32,
    frame_pixel_height: f32,
    window_infos: &'a [neomacs_display_protocol::frame_glyphs::WindowInfo],
}

#[derive(serde::Serialize)]
struct SnapshotGeometryDoc<'a> {
    frames: Vec<SnapshotGeometryFrame<'a>>,
}

/// Install the `neomacs--frame-snapshot` hook (`Context::frame_snapshot_fn`).
///
/// Called by both frontends right where they install `redisplay_fn`; batch
/// mode installs nothing, so the subr signals "no display attached" there.
pub fn install_frame_snapshot_fn(evaluator: &mut Context) {
    evaluator.redisplay_prepare_fn = Some(Box::new(|eval, request| {
        REDISPLAY_RUNTIME.with(|runtime| runtime.prepare_minibuffer_geometry(eval, request))
    }));
    use neovm_core::emacs_core::xdisp::SnapshotFormat;

    evaluator.frame_snapshot_fn = Some(Box::new(|eval, request| {
        let states = collect_snapshot_states(eval, &request.target)?;
        Ok(match request.format {
            SnapshotFormat::Json => serde_json::to_string(&SnapshotDoc { frames: &states })
                .map_err(|error| format!("frame snapshot JSON serialization failed: {error}"))?,
            SnapshotFormat::JsonGeometry => {
                let frames = states
                    .iter()
                    .map(|state| SnapshotGeometryFrame {
                        presentation_id: &state.presentation_id,
                        frame_cols: state.frame_cols,
                        frame_rows: state.frame_rows,
                        frame_pixel_width: state.frame_pixel_width,
                        frame_pixel_height: state.frame_pixel_height,
                        window_infos: &state.window_infos,
                    })
                    .collect();
                serde_json::to_string(&SnapshotGeometryDoc { frames })
                    .map_err(|error| format!("frame geometry JSON serialization failed: {error}"))?
            }
            SnapshotFormat::Text => states
                .iter()
                .map(|state| state.render_text())
                .collect::<Vec<_>>()
                .join("\n"),
            SnapshotFormat::TextFaces => states
                .iter()
                .map(|state| state.render_text_faces())
                .collect::<Vec<_>>()
                .join("\n"),
        })
    }));
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
    evaluator.font_shape_fn = Some(Box::new(|eval, gstring, direction| {
        REDISPLAY_RUNTIME.with(|runtime| runtime.shape_gstring(gstring, direction))
    }));
}

pub fn install_window_layout_query_fn(evaluator: &mut Context) {
    install_window_layout_query_with_policy(
        evaluator,
        neomacs_layout_engine::RowsQueryPolicy::disabled(),
    );
}

fn install_window_layout_query_with_policy(
    evaluator: &mut Context,
    policy: neomacs_layout_engine::RowsQueryPolicy,
) {
    evaluator.display_idle_maintenance_fn = Some(Box::new(|eval| {
        REDISPLAY_RUNTIME.with(|runtime| runtime.maintain_scroll_coverage(eval))
    }));
    // The immutable native policy lives in this existing heap allocation.
    // Context, LayoutEngine and the runtime's TLS representation do not grow.
    evaluator.install_window_layout_query(move |eval, frame_id, window_id, scope| {
        REDISPLAY_RUNTIME.with(|runtime| {
            runtime.query_window_with_policy(eval, frame_id, window_id, scope, &policy)
        })
    });
}

pub(crate) fn tty_rows_host_authority()
-> neovm_core::emacs_core::display_host::InertRowsHostAuthority {
    // SAFETY: TtyPopupDisplayHost stores only the popup redraw AtomicBool.
    // In every instance state its geometry/image/font services are inherited
    // inert defaults: image catalogs are absent and no external font geometry
    // is supplied. Popup/clipboard/terminal I/O methods are not part of the
    // certified Rows producer. Any change to these services requires revisiting
    // this all-instances contract. Core checks the actual host's Any TypeId,
    // current face lifetime, installed owner and collector on every use.
    unsafe {
        neovm_core::emacs_core::display_host::InertRowsHostAuthority::for_type::<
            super::tty_frontend::TtyPopupDisplayHost,
        >()
    }
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

    if neovm_core::emacs_core::xdisp::mode_line_flow_enabled()
        || neovm_core::face::realization::enabled()
    {
        return prepare_tty_tree_before_activation(evaluator, root_id, frame_order);
    }

    let root_state = layout_frame_display_state(evaluator, root_id, FrameLayoutPurpose::Redisplay)?
        .activate(evaluator)
        .ok()?;

    let mut child_states = Vec::new();
    for frame_id in frame_order {
        if frame_id == root_id {
            continue;
        }
        let prepared =
            layout_frame_display_state(evaluator, frame_id, FrameLayoutPurpose::Redisplay);
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
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
    evaluator: &mut Context,
    root_id: FrameId,
    frame_order: Vec<FrameId>,
) -> Option<(SealedFramePresentation, Vec<SealedFramePresentation>)> {
    let root = layout_frame_display_state(evaluator, root_id, FrameLayoutPurpose::Redisplay)?;
    let mut children: Vec<PreparedFrameDisplay> = Vec::new();
    for frame_id in frame_order {
        if frame_id == root_id {
            continue;
        }
        let child = layout_frame_display_state(evaluator, frame_id, FrameLayoutPurpose::Redisplay);
        if evaluator.has_mode_line_display_flow() || evaluator.redisplay_hook_flow_pending() {
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

/// Lisp `visible-cursor' for this redisplay: GNU `tty_show_cursor` reads
/// the C variable when it shows the cursor at `update_end`
/// (src/term.c:362-372); its default is 1, so an unbound variable reads
/// as non-nil here too.  One variable read per redisplay.
pub(crate) fn very_visible_cursor(evaluator: &Context) -> bool {
    evaluator
        .variable_value(neovm_core::emacs_core::intern::WellKnown::VisibleCursor.id())
        .is_none_or(|value| value.is_truthy())
}

/// Rasterize the display state into a `TtyRif` and write ANSI output to stdout.
pub fn run_tty_rif_redisplay(
    tty_rif: &mut TtyRif,
    root: &SealedFramePresentation,
    children: &[SealedFramePresentation],
    very_visible_cursor: bool,
) {
    tty_rif.set_very_visible_cursor(very_visible_cursor);
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
    very_visible_cursor: bool,
) {
    tty_rif.set_very_visible_cursor(very_visible_cursor);
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

/// Redisplay for a session that has no primary terminal.
///
/// GNU batch does not call `init_display`. A terminal exists only after
/// `init_tty` (`src/frame.c:1811`). Constructing [`REDISPLAY_RUNTIME`] scans
/// the system font database, which TTY frames do not use (`src/frame.c:1184-1185`).
/// Call this only once a daemon is serving clients or an auxiliary TTY has opened.
pub(crate) fn install_headless_tty_redisplay(
    evaluator: &mut Context,
    mut try_render_selected_auxiliary: Option<TryRenderSelectedTerminal>,
) {
    // No primary terminal exists. Explicit/attached TTYs own their renderers
    // and are the only valid destination for this redisplay.
    REDISPLAY_RUNTIME.with(|runtime| runtime.disable_cosmic_metrics());
    evaluator.redisplay_fn = Some(Box::new(move |eval: &mut Context| {
        if let Some(render) = try_render_selected_auxiliary.as_mut() {
            render(eval);
        }
    }));
    install_frame_snapshot_fn(evaluator);
    install_window_layout_query_fn(evaluator);
    install_font_shape_driver(evaluator);
}

pub fn install_tty_redisplay_callback_with_popup_redraw(
    evaluator: &mut Context,
    startup: &StartupOptions,
    force_full_redraw: Option<Arc<AtomicBool>>,
    mut try_render_selected_auxiliary: Option<TryRenderSelectedTerminal>,
) {
    if startup.daemon.is_some() {
        install_headless_tty_redisplay(evaluator, try_render_selected_auxiliary);
        return;
    }
    if !tty_init::should_enable_live_tty_io(startup) {
        return;
    }

    // This callback and TtyRif own the same lifetime. Retain the terminal
    // resource, while every redisplay still samples its current dimensions.
    let terminal_size_query = TerminalSizeQuery::new();
    let size =
        tty_init::query_terminal_size_cells_with(&terminal_size_query).unwrap_or_else(|| {
            TerminalCellSize::new(TerminalColumns::from(80_u32), TerminalRows::from(25_u32))
        });
    let mut tty_rif = TtyRif::new_with_caps(
        u32::from(size.columns()) as usize,
        u32::from(size.rows()) as usize,
        super::tty_init::detect_term_caps(),
    );
    // TTY frames use 1x1 character cell metrics (GNU Emacs
    // frame.c:1184-1185). Drop the layout engine's cosmic-text
    // FontMetricsService so char_advance,
    // status_line_font_metrics, etc. fall back to the
    // char-cell grid.
    REDISPLAY_RUNTIME.with(|runtime| runtime.disable_cosmic_metrics());
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
        if let Some(size) = tty_init::query_terminal_size_cells_with(&terminal_size_query) {
            let cols = u32::from(size.columns()) as usize;
            let rows = u32::from(size.rows()) as usize;
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
            let very_visible = very_visible_cursor(eval);
            run_tty_rif_redisplay(&mut tty_rif, &root, &children, very_visible);
        }
    }));
    install_frame_snapshot_fn(evaluator);
    install_window_layout_query_with_policy(
        evaluator,
        neomacs_layout_engine::RowsQueryPolicy::terminal(
            startup.rows_query_admission,
            tty_rows_host_authority(),
        ),
    );
    install_font_shape_driver(evaluator);
}

#[cfg(test)]
#[path = "tests/tty_mode_line_flow_test.rs"]
mod tty_mode_line_flow_test;

// `REDISPLAY_RUNTIME` is CarrierIntrinsic and is not reset at a Green carrier
// park: it holds layout caches and the font metrics service, never a Lisp
// `Value`, and rebuilding the font service per park is a system-font scan
// (measured ~52M instructions per Lisp thread on the many_threads rows).
