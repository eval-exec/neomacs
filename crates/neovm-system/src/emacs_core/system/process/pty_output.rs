//! PTY descriptor ownership outlives readiness monitoring, as in GNU's
//! delete_read_fd (process.c:514-520) versus deactivate_process (:4833-4840).

use neovm_eval::emacs_core::eval::data::process_state::{PtyOutputReader, PtyReadInterest};

static_assertions::assert_impl_all!(PtyOutputReader: Send);
static_assertions::assert_not_impl_any!(PtyOutputReader: Sync);
