//! Eval requests the sealed quit-slot receipt. The value API names no World.
//! No production caller enters this detach.

use neovm_value::quit::QuitRequest;
use neovm_value::quit::quit_detach::{
    DetachedQuitActivation, QuitDetachError, QuitRestoreFailure, detach_quit_activation,
    restore_quit_activation,
};

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant quit detach; no production caller enters it"
    )
)]
pub(crate) fn request_quit_detach(
    expected: &QuitRequest,
) -> Result<DetachedQuitActivation, QuitDetachError> {
    // SAFETY: the caller excluded every other writer of this thread's quit
    // slot for the receipt's lifetime. The receipt is a local effect. It
    // names no World and it does not resume a source.
    unsafe { detach_quit_activation(expected) }
}

#[cold]
#[cfg_attr(
    not(test),
    expect(
        dead_code,
        reason = "E2 L3 dormant quit detach; no production caller enters it"
    )
)]
pub(crate) fn request_quit_restore(
    receipt: DetachedQuitActivation,
) -> Result<(), QuitRestoreFailure> {
    restore_quit_activation(receipt)
}

#[cfg(test)]
#[path = "tests/quit_activation_test.rs"]
mod tests;
