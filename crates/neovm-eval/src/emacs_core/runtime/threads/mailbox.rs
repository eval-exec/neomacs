//! Admission and delivery of target-owned THREAD_EVENT children.

use super::*;
use neovm_value::tagged::transport::SharedRootError;

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub(crate) enum MailboxError {
    #[error("thread event target has no live registration")]
    MissingTarget,
    #[error(transparent)]
    Transport(#[from] SharedRootError),
}

pub(super) fn event_error(error: MailboxError) -> Flow {
    signal(
        "error",
        vec![Value::string(format!(
            "THREAD_EVENT admission failed: {error}"
        ))],
    )
}

impl super::super::eval::Context {
    /// GNU thread.c:1007-1013 queues on the main reader only. Durable heap
    /// edges become input-queue transport leases before their source clears.
    #[doc(hidden)]
    pub fn stage_pending_thread_events(&mut self) -> Result<(), Flow> {
        if self.threads.current_thread_id() != 0 {
            return Ok(());
        }
        let target = self.threads.ids.main_thread();
        for event in self
            .threads
            .take_thread_events(target)
            .map_err(event_error)?
        {
            self.command_loop.keyboard.pending_input_events.push_back(
                crate::emacs_core::eval::keyboard::InputEvent::ThreadEvent {
                    sender: event.sender,
                    error_symbol: event.error_symbol,
                    data: event.data,
                },
            );
        }
        Ok(())
    }

    #[doc(hidden)]
    pub fn thread_event_value(&self, root: &SharedRoot) -> Result<Value, Flow> {
        self.materialize(root)
            .map(|local| local.value())
            .map_err(|error| event_error(MailboxError::Transport(error)))
    }
}
