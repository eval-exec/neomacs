//! Android IME event grouping; editor state stays exclusively in the VM.

use neomacs_app::frontend_event::{FrontendEvent, FrontendFrameId, ImeOperation, ImeSessionId};
use winit::event::Ime;

#[derive(Default)]
pub(super) struct InputMethod {
    next: u64,
    active: Option<Session>,
}

struct Session {
    id: ImeSessionId,
    deletion: Option<(usize, usize)>,
}

impl InputMethod {
    pub(super) fn event(&mut self, event: Ime, target: FrontendFrameId) -> Option<FrontendEvent> {
        let (session, operation) = match event {
            Ime::Enabled => {
                self.next = self
                    .next
                    .checked_add(1)
                    .expect("IME session identity exhausted");
                let id = ImeSessionId(self.next);
                self.active = Some(Session { id, deletion: None });
                (id, ImeOperation::Begin)
            }
            Ime::Disabled => (self.active.take()?.id, ImeOperation::End),
            Ime::DeleteSurrounding {
                before_bytes,
                after_bytes,
            } => {
                // The Android backend pairs this with Commit, including an
                // empty Commit for deletion-only edits. Never expose a half
                // replacement to Lisp or a different insertion context.
                self.active.as_mut()?.deletion = Some((before_bytes, after_bytes));
                return None;
            }
            Ime::Commit(text) => {
                let active = self.active.as_mut()?;
                let (before_bytes, after_bytes) = active.deletion.take().unwrap_or_default();
                (
                    active.id,
                    ImeOperation::Replace {
                        before_bytes,
                        after_bytes,
                        text,
                    },
                )
            }
            Ime::Preedit(..) => return None,
        };
        Some(FrontendEvent::Ime {
            session,
            operation,
            target,
        })
    }
}
