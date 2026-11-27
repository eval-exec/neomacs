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
use neovm_core::window::DisplayFrameManagerExt;
use neovm_core::window::{FrameId, RenderFrameScope, RenderFrameVisibility};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::StartupOptions;
use super::tty_init;
use neovm_core::window::display::WindowDisplayContextExt;

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
pub(super) fn configure_gui_runtime(
    font_sizing: super::FontSizing,
    identity: Option<neomacs_display_protocol::GraphicalDisplayIdentity>,
) {
    REDISPLAY_RUNTIME.with(|runtime| {
        if let Some(identity) = identity {
            runtime.set_display_identity(identity);
        }
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

/// Install the GUI frontend's bounded display idle-maintenance hook.
///
/// This is the sole owner of `Context::display_idle_maintenance_fn`: TTY and
/// batch sessions leave it unset so periodic GUI service does not engage there.
/// Render-command delivery stays inside the declared GUI effect owner
/// (`main.rs::run_gui_evaluator_worker`), passed here as a delivery callback
/// so `frame_layout.rs` remains purely a layout/font owner.
pub(super) fn install_gui_display_idle_maintenance(
    evaluator: &mut Context,
    deliver: impl Fn(
        neomacs_display_protocol::scroll_coverage::AuxiliaryScrollCoverage,
    ) -> neomacs_display_runtime::redisplay::AuxiliaryCoverageDelivery
    + 'static,
) {
    evaluator.display_idle_maintenance_fn = Some(Box::new(move |eval| {
        REDISPLAY_RUNTIME
            .with(|runtime| runtime.maintain_scroll_coverage_with_delivery(eval, &deliver))
    }));
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
        if matches!(request.format, SnapshotFormat::DisplayDebug) {
            return capture_display_debug_bundle_json(eval, &request.target);
        }
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
            SnapshotFormat::DisplayDebug => unreachable!("handled before layout"),
        })
    }));
}

#[derive(serde::Serialize)]
struct DisplayDebugFrameSnapshotSection<'a> {
    geometry: SnapshotGeometryDoc<'a>,
    text: Vec<String>,
}

/// Top-level JSON bundle written by `M-x neomacs-dump-display-state` /
/// `neomacs--frame-snapshot`'s `'display-debug` format.
///
/// Consumes `neomacs::cursor_geometry` lines and the existing `frame-snapshot`
/// geometry/text projection verbatim without re-deriving cursor fields, and
/// pairs them with the render thread's last-presented state (blink, visibility,
/// inverse-video, retained_static vs full_render, scale/surface, animation,
/// input-scroll) keyed by `presentation_id`.
#[derive(serde::Serialize)]
struct DisplayDebugBundle<'a> {
    schema_version: u32,
    capture_unix_ms: u64,
    version: neomacs_display_protocol::DisplayDebugVersion,
    selected_frame_id: Option<u64>,
    last_interaction_presentation: u64,
    last_presented: Vec<neomacs_display_protocol::PresentedFrameDebugSnapshot>,
    render_capture_error: Option<String>,
    cursor_geometry_capture_enabled: bool,
    cursor_geometry_log: Vec<String>,
    frame_snapshot: Option<DisplayDebugFrameSnapshotSection<'a>>,
    frame_snapshot_error: Option<String>,
    diagnostics_flags: neomacs_display_protocol::DisplayDebugFlags,
}

fn capture_display_debug_bundle_json(
    eval: &mut Context,
    target: &neovm_core::emacs_core::xdisp::SnapshotTarget,
) -> Result<String, String> {
    // 1. Capture the render thread's unhealed last-presented state and the
    //    evaluator's pre-snapshot `last_interaction_presentation` FIRST, before
    //    `collect_snapshot_states` allocates and discards a snapshot
    //    `PresentationId` (which advances `last_interaction_presentation()`
    //    without sending a frame to the render thread).
    let selected_frame_id = eval.frame_manager().selected_frame().map(|f| f.id.0);
    let last_interaction_presentation = eval.last_interaction_presentation();
    let (last_presented, render_capture_error) = match eval.display_host.as_deref() {
        Some(host) => match host.capture_display_debug_state() {
            Ok(raw_json) => match serde_json::from_str::<
                Vec<neomacs_display_protocol::PresentedFrameDebugSnapshot>,
            >(&raw_json)
            {
                Ok(frames) => (frames, None),
                Err(error) => (
                    Vec::new(),
                    Some(format!("failed to decode render debug state: {error}")),
                ),
            },
            Err(error) => (Vec::new(), Some(error)),
        },
        None => (
            Vec::new(),
            Some("no display host attached to capture render-thread state".to_string()),
        ),
    };

    // 2. Arm in-process `neomacs::cursor_geometry` capture (bounded 64-line
    //    ring). Capture intentionally remains armed after this dump because
    //    successive dumps need multi-frame cursor-geometry history; the
    //    user-facing command reports this in the echo area. Unless `RUST_LOG`
    //    pre-armed capture at startup, this first invocation has no prior
    //    frame events in the ring and only enables capture for subsequent
    //    frames (plus any snapshot-pass event below).
    neovm_core::logging::set_cursor_geometry_capture(true);

    // 3. Reuse the existing `frame-snapshot` layout projection (does not push
    //    to the render thread).
    let (snapshot_states, frame_snapshot_error) = match collect_snapshot_states(eval, target) {
        Ok(states) => (Some(states), None),
        Err(error) => (None, Some(error)),
    };
    let frame_snapshot = snapshot_states.as_ref().map(|states| {
        let geometry = SnapshotGeometryDoc {
            frames: states
                .iter()
                .map(|state| SnapshotGeometryFrame {
                    presentation_id: &state.presentation_id,
                    frame_cols: state.frame_cols,
                    frame_rows: state.frame_rows,
                    frame_pixel_width: state.frame_pixel_width,
                    frame_pixel_height: state.frame_pixel_height,
                    window_infos: &state.window_infos,
                })
                .collect(),
        };
        let text = states.iter().map(|state| state.render_text()).collect();
        DisplayDebugFrameSnapshotSection { geometry, text }
    });

    // 4. Snapshot the verbatim `neomacs::cursor_geometry` lines collected by
    //    the tracing layer (both pre-existing `RUST_LOG` lines and any emitted
    //    by the snapshot pass above).
    let cursor_geometry_log = neovm_core::logging::snapshot_cursor_geometry_lines();
    let cursor_geometry_capture_enabled = neovm_core::logging::cursor_geometry_capture_enabled();

    let window_system = eval
        .frame_manager()
        .selected_frame()
        .and_then(|f| f.window_system)
        .map(|ws| neovm_core::emacs_core::print::print_value(&ws))
        .unwrap_or_else(|| "none".to_string());
    let capture_unix_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let diagnostics_flags = neomacs_display_protocol::DisplayDebugFlags::compute(
        selected_frame_id,
        last_interaction_presentation,
        &last_presented,
    );
    let doc = DisplayDebugBundle {
        schema_version: neomacs_display_protocol::DISPLAY_DEBUG_SCHEMA_VERSION,
        capture_unix_ms,
        version: neomacs_display_protocol::DisplayDebugVersion {
            package_version: env!("CARGO_PKG_VERSION").to_string(),
            git_commit: super::build_info::git_commit_summary(),
            platform: format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH),
            window_system,
        },
        selected_frame_id,
        last_interaction_presentation,
        last_presented,
        render_capture_error,
        cursor_geometry_capture_enabled,
        cursor_geometry_log,
        frame_snapshot,
        frame_snapshot_error,
        diagnostics_flags,
    };
    serde_json::to_string_pretty(&doc)
        .map_err(|error| format!("display debug snapshot JSON serialization failed: {error}"))
}

#[cfg(test)]
mod display_debug_tests {
    use super::*;
    use neomacs_display_protocol::{
        DISPLAY_DEBUG_SCHEMA_VERSION, PresentedFrameDebugSnapshot, PresentedRenderPath,
    };
    use neovm_core::emacs_core::eval::DisplayHost;

    struct MockDebugDisplayHost {
        payload: String,
    }

    impl DisplayHost for MockDebugDisplayHost {
        fn realize_gui_frame(
            &mut self,
            _request: neovm_core::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn resize_gui_frame(
            &mut self,
            _request: neovm_core::emacs_core::eval::GuiFrameHostRequest,
        ) -> Result<(), String> {
            Ok(())
        }

        fn capture_display_debug_state(&self) -> Result<String, String> {
            Ok(self.payload.clone())
        }
    }

    #[test]
    fn display_debug_snapshot_bundles_last_presented_frame_snapshot_and_cursor_geometry_log() {
        let _guard = neovm_core::logging::init(neovm_core::logging::LogTarget::Test);
        neovm_core::logging::clear_cursor_geometry_lines();
        neovm_core::logging::set_cursor_geometry_capture(false);

        let mut eval = neovm_core::emacs_core::eval_hook_installers::new_context();
        let buf_id = eval
            .edit
            .buffers
            .current_buffer_id()
            .expect("initial buffer");
        eval.frame_manager_mut()
            .create_frame("F1", 800, 600, buf_id);
        install_frame_snapshot_fn(&mut eval);

        let redisplay_count = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let rc = std::sync::Arc::clone(&redisplay_count);
        eval.redisplay_fn = Some(Box::new(move |_| {
            rc.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }));

        eval.eval_str("(progn (insert \"alpha\\nbeta\\ngamma\") (goto-char 9))")
            .expect("populate buffer");

        let selected_frame_id = eval
            .frame_manager()
            .selected_frame()
            .expect("selected frame")
            .id
            .0;
        // Simulate a prior presented frame (presentation_id = 1) so the test
        // also verifies that `collect_snapshot_states` allocating and
        // discarding its own snapshot PresentationId does not falsely flag
        // `presentation_lagging`.
        let pre_snapshot_presentation = eval.begin_interaction_presentation();
        let mock_frame = PresentedFrameDebugSnapshot {
            frame_id: selected_frame_id,
            is_primary: true,
            presentation_id: pre_snapshot_presentation,
            frame_seq: 12,
            render_path: PresentedRenderPath::RetainedStatic,
            scale_factor: 1.0,
            surface_px: [800, 600],
            logical_px: [800.0, 600.0],
            has_active_cursor: true,
            cursor_visible: false,
            blink_enabled: true,
            inverse_video_active: false,
            is_animating: false,
            animated_cursor_rect: None,
            input_scroll_active: false,
            input_scroll_offset: 0.0,
        };
        eval.display_host = Some(Box::new(MockDebugDisplayHost {
            payload: serde_json::to_string(&vec![mock_frame]).expect("encode mock frame"),
        }));

        // Emit one sample `neomacs::cursor_geometry` line with capture enabled
        // to prove the bundle consumes the target verbatim.
        neovm_core::logging::set_cursor_geometry_capture(true);
        tracing::debug!(
            target: neovm_core::logging::CURSOR_GEOMETRY_TARGET,
            "frame={selected_frame_id} stage=draw presentation={pre_snapshot_presentation} window=1 drawn_in_row=true"
        );
        neovm_core::logging::set_cursor_geometry_capture(false);

        let val = eval
            .eval_str("(neomacs--frame-snapshot nil 'display-debug)")
            .expect("display-debug snapshot");
        let json = val.as_utf8_str().expect("snapshot string");
        let parsed: serde_json::Value = serde_json::from_str(json).expect("parse bundle JSON");

        assert_eq!(
            parsed["schema_version"].as_u64(),
            Some(u64::from(DISPLAY_DEBUG_SCHEMA_VERSION))
        );
        assert_eq!(
            parsed["last_interaction_presentation"].as_u64(),
            Some(pre_snapshot_presentation),
            "last_interaction_presentation must be sampled before snapshot layout"
        );
        assert_eq!(
            parsed["diagnostics_flags"]["presentation_lagging"].as_bool(),
            Some(false),
            "discarded snapshot presentation must not cause false presentation_lagging"
        );
        assert_eq!(
            redisplay_count.load(std::sync::atomic::Ordering::SeqCst),
            0,
            "'display-debug must not force redisplay"
        );
        assert!(parsed["render_capture_error"].is_null());
        assert_eq!(
            parsed["cursor_geometry_capture_enabled"].as_bool(),
            Some(true)
        );
        let log_lines = parsed["cursor_geometry_log"]
            .as_array()
            .expect("cursor_geometry_log array");
        assert!(
            log_lines
                .iter()
                .any(|line| line.as_str().is_some_and(|s| s.contains(&format!(
                    "stage=draw presentation={pre_snapshot_presentation}"
                )))),
            "bundle must embed verbatim neomacs::cursor_geometry lines: {log_lines:?}"
        );
        assert_eq!(parsed["last_presented"][0]["frame_seq"].as_u64(), Some(12));
        assert_eq!(
            parsed["last_presented"][0]["render_path"].as_str(),
            Some("retained_static")
        );
        assert_eq!(
            parsed["diagnostics_flags"]["cursor_hidden_or_blinked_off"].as_bool(),
            Some(true)
        );
        assert!(parsed["frame_snapshot"]["text"].is_array());
        assert!(parsed["frame_snapshot"]["geometry"]["frames"].is_array());

        // Control: `'text` format still forces redisplay before capturing.
        let _ = eval
            .eval_str("(neomacs--frame-snapshot nil 'text)")
            .expect("text snapshot");
        assert_eq!(
            redisplay_count.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "'text control must still invoke redisplay_fn"
        );
        neovm_core::logging::set_cursor_geometry_capture(false);
    }
}

/// Single frontend font-driver vtable (`N-font-driver-vtable`, GNU
/// `struct font_driver`) backed by `REDISPLAY_RUNTIME`. Zero-sized so boxing
/// it performs no heap allocation.
struct RedisplayFontDriver;

impl neovm_core::emacs_core::font::FontDriver for RedisplayFontDriver {
    fn shape_gstring(
        &mut self,
        _eval: &mut Context,
        gstring: neovm_core::Value,
        direction: neovm_core::Value,
    ) -> neovm_core::emacs_core::font::GstringShapeOutcome {
        REDISPLAY_RUNTIME.with(|runtime| runtime.shape_gstring(gstring, direction))
    }

    fn glyph_text_metrics(
        &mut self,
        font: neovm_core::Value,
        ch: u32,
    ) -> Option<neovm_core::display_vocab::GlyphTextMetrics> {
        REDISPLAY_RUNTIME.with(|runtime| runtime.glyph_text_metrics(font, ch))
    }

    fn realized_fontset(
        &mut self,
        eval: &mut Context,
        frame_id: FrameId,
        window_id: neovm_core::window::WindowId,
        lisp_pos: i64,
    ) -> Option<(u32, u64)> {
        REDISPLAY_RUNTIME.with(|runtime| {
            runtime.realized_fontset_for_lisp_pos(eval, frame_id, window_id, lisp_pos)
        })
    }
}

/// Install the frontend font driver vtable (`font->driver->shape`,
/// `font->driver->text_extents`, and sealed realized-fontset lookup for
/// `font-at` / positioned `internal-char-font`).
pub fn install_font_driver(evaluator: &mut Context) {
    evaluator.font_driver = Some(Box::new(RedisplayFontDriver));
}

/// Install the synchronous layout-query adapter used by display primitives
/// such as `(window-end WINDOW t)` and `posn-at-point`.
///
/// This targets one window through the canonical row producer without entering
/// the renderer presentation lifecycle. Both GUI and TTY install this adapter;
/// batch mode intentionally does not.
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
    // The immutable native policy lives in this existing heap allocation.
    // Context, LayoutEngine and the runtime's TLS representation do not grow.
    evaluator
        .install_window_layout_query(move |eval, frame_id, window_id, scope| {
            REDISPLAY_RUNTIME.with(|runtime| {
                runtime.query_window_with_policy(eval, frame_id, window_id, scope, &policy)
            })
        })
        .expect("install the layout-query handler");
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
    // term.c:5246 DEFVAR_BOOL, read from the C global in tty_show_cursor
    // (term.c:362-372); a live `let` is visible there.
    evaluator
        .builtin_var_value(neovm_core::emacs_core::intern::WellKnown::VisibleCursor.id())
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
    install_font_driver(evaluator);
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
    let mut last_pty_size = (
        u32::from(size.columns()) as usize,
        u32::from(size.rows()) as usize,
    );
    let mut tty_rif = TtyRif::new_with_caps(
        last_pty_size.0,
        last_pty_size.1,
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
        if !tty_init::is_primary_tty_active() {
            return;
        }
        let mut pty_resized = false;
        if let Some(size) = tty_init::query_terminal_size_cells_with(&terminal_size_query) {
            let pty_size = (
                u32::from(size.columns()) as usize,
                u32::from(size.rows()) as usize,
            );
            if pty_size != last_pty_size {
                last_pty_size = pty_size;
                pty_resized = true;
            }
        }
        if force_full_redraw
            .as_ref()
            .is_some_and(|force| force.swap(false, Ordering::AcqRel))
        {
            tty_rif.force_redraw();
        }
        if let Some((root, children)) = run_tty_layout_tree(eval) {
            let root_id = FrameId(root.frame_placement.frame().get());
            // Mirror GNU `FrameCols(tty)` / `FrameRows(tty)` (`src/frame.c:1053,1088`):
            // non-`pretend` `set-frame-size`/`width`/`height` updates the TTY
            // device grid independently of the kernel PTY size until a real
            // physical terminal resize arrives.
            let (target_cols, target_rows) = if pty_resized {
                last_pty_size
            } else {
                eval.frame_manager()
                    .get(root_id)
                    .and_then(|frame| frame.tty_device_size)
                    .map_or(last_pty_size, |dev| (dev.cols as usize, dev.rows as usize))
            };
            if tty_rif.width() != target_cols || tty_rif.height() != target_rows {
                tty_rif.resize(target_cols, target_rows);
            }
            // Consume only physically prepared frames, after layout and just
            // before repaint. Pending requests for unrendered devices remain
            // Context-owned; no new frontend Lisp cache or TLS is introduced.
            let mut full_redraw = eval.gnu_take_tty_frame_redraw(root_id);
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
    install_font_driver(evaluator);
}

#[cfg(test)]
#[path = "tests/tty_mode_line_flow_test.rs"]
mod tty_mode_line_flow_test;

// `REDISPLAY_RUNTIME` is CarrierIntrinsic and is not reset at a Green carrier
// park: it holds layout caches and the font metrics service, never a Lisp
// `Value`, and rebuilding the font service per park is a system-font scan
// (measured ~52M instructions per Lisp thread on the many_threads rows).
