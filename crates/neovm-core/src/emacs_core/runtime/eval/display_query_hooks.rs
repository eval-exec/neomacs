//! Cold display-policy query table (stage-4 W4/E9).
//!
//! The two GNU redisplay policy selectors stay in the display layer: the
//! retained-idle-row skip and the mode-line flow policy are display-engine
//! decisions read from process-wide knobs. The evaluator's redisplay
//! transaction reads them through this one cold PC2 table, installed by core
//! at `Context::new_inner` and `Context::from_dump`.
//!
//! Frequency: both slots are read once per redisplay / command boundary
//! (inside the inhibit-redisplay and idle-skip checks), never per bytecode
//! step or per char.

use super::*;
use neovm_value::{HookInstallError, HookInstallStatus};
use std::sync::OnceLock;

use super::data::window_state::{Frame, FrameManager, WindowDomain};
use crate::buffer::BufferManager;
use crate::emacs_core::display_metrics::WindowId;
use neovm_value::display_vocab::FrameId;

/// A display-policy selector evaluated at a redisplay boundary.
pub type DisplayFlagQuery = fn() -> bool;

/// Whether this frame is an actual termcap face target.
pub type FrameTTYTargetQuery = fn(&Frame) -> bool;

/// Resolve (and, in batch mode, synthesize) the selected frame.
pub type EnsureSelectedFrameId = fn(&mut FrameManager, &mut BufferManager) -> FrameId;
/// Mirror the selected window's point into its marker.
pub type RememberSelectedWindowPoint = fn(&mut FrameManager, &mut BufferManager, FrameId);
/// Logical body dimensions for GNU window-change records.
pub type WindowBodyDimensions =
    fn(&FrameManager, &BufferManager, FrameId, WindowId) -> Result<(i64, i64), Flow>;
/// Commit a `resize-mini-window-internal` request.
pub type ResizeMiniWindow = fn(&mut Context, Vec<Value>) -> EvalResult;
/// Apply GNU auto-hscroll before a redisplay.
pub type UpdateAutoHscroll = fn(&mut Context);
/// The fontset registry generation in the redisplay signature.
pub type FontsetGeneration = fn() -> u64;
/// GNU's frame-domain window lookup (live/valid/any).
pub type WindowDomainFrameOf = fn(WindowDomain, &FrameManager, WindowId) -> Option<FrameId>;

/// Window/redisplay operations eval invokes at command/redisplay boundaries.
///
/// These are display-engine behaviours that the evaluator reaches through the
/// same one cold table as the policy queries; the display installer publishes
/// them at the two construction seams. Frequency: per redisplay / command
/// boundary / minibuffer activation, never per bytecode step or per char.
#[derive(Clone, Copy)]
pub struct DisplayWindowHooks {
    ensure_selected_frame_id: EnsureSelectedFrameId,
    remember_selected_window_point: RememberSelectedWindowPoint,
    window_body_dimensions: WindowBodyDimensions,
    resize_mini_window: ResizeMiniWindow,
    update_auto_hscroll: UpdateAutoHscroll,
    fontset_generation: FontsetGeneration,
    window_domain_frame_of: WindowDomainFrameOf,
}

impl DisplayWindowHooks {
    /// Build the window/redisplay group.
    #[allow(clippy::too_many_arguments)] // one cold descriptor, one installer
    pub const fn new(
        ensure_selected_frame_id: EnsureSelectedFrameId,
        remember_selected_window_point: RememberSelectedWindowPoint,
        window_body_dimensions: WindowBodyDimensions,
        resize_mini_window: ResizeMiniWindow,
        update_auto_hscroll: UpdateAutoHscroll,
        fontset_generation: FontsetGeneration,
        window_domain_frame_of: WindowDomainFrameOf,
    ) -> Self {
        Self {
            ensure_selected_frame_id,
            remember_selected_window_point,
            window_body_dimensions,
            resize_mini_window,
            update_auto_hscroll,
            fontset_generation,
            window_domain_frame_of,
        }
    }

    fn ensure_selected_frame_id(&self) -> EnsureSelectedFrameId {
        self.ensure_selected_frame_id
    }

    fn remember_selected_window_point(&self) -> RememberSelectedWindowPoint {
        self.remember_selected_window_point
    }

    fn window_body_dimensions(&self) -> WindowBodyDimensions {
        self.window_body_dimensions
    }

    fn resize_mini_window(&self) -> ResizeMiniWindow {
        self.resize_mini_window
    }

    fn update_auto_hscroll(&self) -> UpdateAutoHscroll {
        self.update_auto_hscroll
    }

    fn fontset_generation(&self) -> FontsetGeneration {
        self.fontset_generation
    }

    fn window_domain_frame_of(&self) -> WindowDomainFrameOf {
        self.window_domain_frame_of
    }
}

/// The redisplay-policy queries eval reads through the cold table.
#[derive(Clone, Copy)]
pub struct DisplayQueryHooks {
    redisplay_idle_skip: DisplayFlagQuery,
    mode_line_flow: DisplayFlagQuery,
    face_realization: DisplayFlagQuery,
    frame_tty_target: FrameTTYTargetQuery,
    window: DisplayWindowHooks,
}

impl DisplayQueryHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        redisplay_idle_skip: DisplayFlagQuery,
        mode_line_flow: DisplayFlagQuery,
        face_realization: DisplayFlagQuery,
        frame_tty_target: FrameTTYTargetQuery,
        window: DisplayWindowHooks,
    ) -> Self {
        Self {
            redisplay_idle_skip,
            mode_line_flow,
            face_realization,
            frame_tty_target,
            window,
        }
    }

    pub fn redisplay_idle_skip(&self) -> DisplayFlagQuery {
        self.redisplay_idle_skip
    }

    pub fn mode_line_flow(&self) -> DisplayFlagQuery {
        self.mode_line_flow
    }

    pub fn face_realization(&self) -> DisplayFlagQuery {
        self.face_realization
    }

    pub fn frame_tty_target(&self) -> FrameTTYTargetQuery {
        self.frame_tty_target
    }

    /// The window/redisplay operation group.
    pub fn window(&self) -> DisplayWindowHooks {
        self.window
    }
}

static DISPLAY_QUERY_HOOKS: OnceLock<DisplayQueryHooks> = OnceLock::new();
static DISPLAY_QUERY_HOOKS_IDENTITY: OnceLock<&'static DisplayQueryHooks> = OnceLock::new();

/// Publish the display layer's redisplay-policy queries.
///
/// Idempotent for the same descriptor; a different table is a typed refusal
/// (`HookInstallError::DifferentTable`), never a silent overwrite. Core calls
/// this at `Context::new_inner` and `Context::from_dump`.
#[must_use = "handle a conflicting display-query publication"]
#[cold]
#[inline(never)]
pub fn install_display_query_hooks(
    hooks: &'static DisplayQueryHooks,
) -> Result<HookInstallStatus, HookInstallError> {
    let selected = DISPLAY_QUERY_HOOKS_IDENTITY.get_or_init(|| hooks);
    if !std::ptr::eq(*selected, hooks) {
        return Err(HookInstallError::DifferentTable);
    }
    match DISPLAY_QUERY_HOOKS.set(**selected) {
        Ok(()) => Ok(HookInstallStatus::Installed),
        Err(_same_descriptor) => Ok(HookInstallStatus::AlreadyInstalled),
    }
}

/// The installed table, if core has run one of the two pinned install seams.
pub fn display_query_hooks() -> Option<&'static DisplayQueryHooks> {
    DISPLAY_QUERY_HOOKS.get()
}

/// Typed refusal for the unreachable uninstalled path: reaching it means the
/// construction seam itself is broken, so never default a policy selector.
fn hooks_or_refusal() -> Result<&'static DisplayQueryHooks, Flow> {
    display_query_hooks().ok_or_else(|| {
        signal(
            "error",
            vec![Value::string("display-query hooks are not installed")],
        )
    })
}

/// Whether GNU's retained-idle-row skip policy is enabled.
pub fn redisplay_idle_skip_enabled() -> Result<bool, Flow> {
    Ok((hooks_or_refusal()?.redisplay_idle_skip())())
}

/// Whether the mode-line flow policy is enabled.
pub fn mode_line_flow_enabled() -> Result<bool, Flow> {
    Ok((hooks_or_refusal()?.mode_line_flow())())
}

/// Whether the face-realization lifecycle is enabled.
pub fn face_realization_enabled() -> Result<bool, Flow> {
    Ok((hooks_or_refusal()?.face_realization())())
}

/// Whether `frame` is an actual termcap face target.
pub fn frame_is_tty_target(frame: &Frame) -> Result<bool, Flow> {
    Ok((hooks_or_refusal()?.frame_tty_target())(frame))
}

/// The installed window/redisplay group.
fn installed_window_hooks() -> &'static DisplayWindowHooks {
    &display_query_hooks()
        .expect("display-query hooks install at Context construction")
        .window
}

/// Ensure a selected frame exists (synthesizing the batch frame if needed).
pub fn ensure_selected_frame_id(frames: &mut FrameManager, buffers: &mut BufferManager) -> FrameId {
    (installed_window_hooks().ensure_selected_frame_id())(frames, buffers)
}

/// Mirror the selected window's point into its marker.
pub fn remember_selected_window_point(
    frames: &mut FrameManager,
    buffers: &mut BufferManager,
    frame_id: FrameId,
) {
    (installed_window_hooks().remember_selected_window_point())(frames, buffers, frame_id)
}

/// Logical body dimensions for GNU window-change records.
pub fn window_body_dimensions(
    frames: &FrameManager,
    buffers: &BufferManager,
    frame_id: FrameId,
    window_id: WindowId,
) -> Result<(i64, i64), Flow> {
    (installed_window_hooks().window_body_dimensions())(frames, buffers, frame_id, window_id)
}

/// Commit a `resize-mini-window-internal` request.
pub fn resize_mini_window(eval: &mut Context, args: Vec<Value>) -> EvalResult {
    (installed_window_hooks().resize_mini_window())(eval, args)
}

/// Apply GNU auto-hscroll before a redisplay.
pub fn update_auto_hscroll(eval: &mut Context) {
    (installed_window_hooks().update_auto_hscroll())(eval)
}

/// The fontset registry generation in the redisplay signature.
pub fn fontset_generation() -> u64 {
    (installed_window_hooks().fontset_generation())()
}

/// GNU's frame-domain window lookup (live/valid/any).
pub fn window_domain_frame_of(
    domain: WindowDomain,
    frames: &FrameManager,
    window: WindowId,
) -> Option<FrameId> {
    (installed_window_hooks().window_domain_frame_of())(domain, frames, window)
}
