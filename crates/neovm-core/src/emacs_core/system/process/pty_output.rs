//! PTY descriptor ownership outlives readiness monitoring, as in GNU's
//! delete_read_fd (process.c:514-520) versus deactivate_process (:4833-4840).

use std::io::{self, Read};

use crate::emacs_core::eval::data::process_state::{PtyOutputReader, PtyReadInterest};

impl std::fmt::Debug for PtyOutputReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyOutputReader")
            .field("interest", &self.interest)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(PtyOutputReader: Send);
static_assertions::assert_not_impl_any!(PtyOutputReader: Sync);
