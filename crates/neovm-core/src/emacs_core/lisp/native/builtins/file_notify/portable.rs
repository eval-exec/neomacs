//! File-notification subrs for hosts without native filesystem watches.

use crate::emacs_core::error::Flow;
use crate::emacs_core::eval::Context;
use crate::emacs_core::value::Value;

#[path = "subrs.rs"]
mod subrs;

#[cfg(test)]
pub(crate) use self::subrs::SUBRS;
pub(crate) use self::subrs::register_subrs;

pub(crate) fn reset_file_notify_thread_locals() {}

/// Registry handle for hosts without filesystem watches. There is nothing to
/// own: the handle is inert and every registry operation is a no-op.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct FileNotifyRegistryHandle;

pub(crate) fn current_file_notify_registry_handle() -> FileNotifyRegistryHandle {
    FileNotifyRegistryHandle
}

pub(crate) fn install_file_notify_registry_handle(_handle: &FileNotifyRegistryHandle) {}

pub(crate) fn collect_file_notify_registry_gc_roots(
    _registry: &FileNotifyRegistryHandle,
    _group: &mut Vec<Value>,
) {
}

pub(crate) fn collect_file_notify_gc_roots(_roots: &mut Vec<Value>) {}

pub(crate) fn has_active_file_notify_watches() -> bool {
    false
}

pub(crate) fn drain_file_notify_events(_ctx: &mut Context) -> Result<usize, Flow> {
    Ok(0)
}
