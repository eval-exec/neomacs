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

/// A display-policy selector evaluated at a redisplay boundary.
pub type DisplayFlagQuery = fn() -> bool;

/// The redisplay-policy queries eval reads through the cold table.
#[derive(Clone, Copy)]
pub struct DisplayQueryHooks {
    redisplay_idle_skip: DisplayFlagQuery,
    mode_line_flow: DisplayFlagQuery,
    face_realization: DisplayFlagQuery,
}

impl DisplayQueryHooks {
    /// Build the one canonical descriptor core publishes.
    pub const fn new(
        redisplay_idle_skip: DisplayFlagQuery,
        mode_line_flow: DisplayFlagQuery,
        face_realization: DisplayFlagQuery,
    ) -> Self {
        Self {
            redisplay_idle_skip,
            mode_line_flow,
            face_realization,
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
