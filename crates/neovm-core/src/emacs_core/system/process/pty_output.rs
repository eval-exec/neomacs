//! PTY descriptor ownership outlives readiness monitoring, as in GNU's
//! delete_read_fd (process.c:514-520) versus deactivate_process (:4833-4840).

use std::io::{self, Read};

use crate::emacs_core::eval::data::process_state::{PtyOutputReader, PtyReadInterest};

impl PtyOutputReader {
    pub fn new(reader: Box<dyn Read + Send>) -> Self {
        Self {
            reader,
            interest: PtyReadInterest::Monitored,
        }
    }

    pub fn is_monitored(&self) -> bool {
        match self.interest {
            PtyReadInterest::Monitored => true,
            PtyReadInterest::DrainOnly => false,
        }
    }

    pub fn stop_monitoring(&mut self) {
        self.interest = PtyReadInterest::DrainOnly;
    }
}

impl Read for PtyOutputReader {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        // portable-pty's reader calls libc::read directly. GNU emacs_read
        // (sysdep.c:2758-2789) never exposes EINTR as EOF or a permanent read
        // failure; finish that same interrupted operation before classifying it.
        loop {
            match self.reader.read(buffer) {
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                result => return result,
            }
        }
    }
}

impl std::fmt::Debug for PtyOutputReader {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PtyOutputReader")
            .field("interest", &self.interest)
            .finish_non_exhaustive()
    }
}

static_assertions::assert_impl_all!(PtyOutputReader: Send);
static_assertions::assert_not_impl_any!(PtyOutputReader: Sync);
