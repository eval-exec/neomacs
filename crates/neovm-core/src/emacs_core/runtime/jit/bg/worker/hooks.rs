// A callback exists only in tests, is owned by one service call, and
// cannot affect process-global workers. Release builds have a zero-sized
// hooks value whose notification methods do nothing.
#[derive(Default)]
pub(super) struct BatchHooks {
    #[cfg(test)]
    pub(super) callback: Option<Box<dyn FnMut(BatchEvent)>>,
}

#[cfg(test)]
pub(super) enum BatchEvent {
    Prepared { seq: u64, index: usize },
    BeforeFinalize { members: usize },
    BeforePublish { members: usize, success: bool },
}

impl BatchHooks {
    pub(super) fn prepared(&mut self, _seq: u64, _index: usize) {
        #[cfg(test)]
        if let Some(callback) = self.callback.as_mut() {
            callback(BatchEvent::Prepared {
                seq: _seq,
                index: _index,
            });
        }
    }

    pub(super) fn before_finalize(&mut self, _members: usize) {
        #[cfg(test)]
        if let Some(callback) = self.callback.as_mut() {
            callback(BatchEvent::BeforeFinalize { members: _members });
        }
    }

    pub(super) fn before_publish(&mut self, _members: usize, _success: bool) {
        #[cfg(test)]
        if let Some(callback) = self.callback.as_mut() {
            callback(BatchEvent::BeforePublish {
                members: _members,
                success: _success,
            });
        }
    }
}
