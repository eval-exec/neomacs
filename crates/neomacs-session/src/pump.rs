//! A generic pump: drive an endpoint over any `Read + Write` (DESIGN §6.1).
//!
//! The pump never blocks on its own: a transport that has nothing to read
//! returns `WouldBlock` (or `TimedOut` from a read timeout), and the pump
//! ticks the endpoint's timers with the clock's `now` on every step. So a
//! stalled peer ends in the endpoint's keepalive timeout, a typed outcome.
//!
//! Nor does it wait. Its host calls [`Pump::step`] when the transport is
//! ready or a timer is due (a readiness poll, or a read timeout); a loop that
//! steps a non-blocking transport with nothing to do in between spins.

use std::io::{ErrorKind, Read, Write};

use crate::session::{CloseOutcome, EvaluatorEndpoint, FrontendEndpoint, LinkId};
use crate::{Clock, SessionTime};

/// What a pump needs from a link's state machine.
pub trait Driver {
    fn receive(&mut self, bytes: &[u8], now: SessionTime);
    fn receive_eof(&mut self, now: SessionTime);
    fn transport_failed(&mut self, kind: ErrorKind, now: SessionTime);
    fn poll_transmit(&mut self, now: SessionTime) -> Option<Vec<u8>>;
    fn tick(&mut self, now: SessionTime);
    /// `Some` once the link has ended and handed out its last bytes.
    fn closed(&self) -> Option<CloseOutcome>;
}

impl Driver for FrontendEndpoint {
    fn receive(&mut self, bytes: &[u8], now: SessionTime) {
        FrontendEndpoint::receive(self, bytes, now);
    }

    fn receive_eof(&mut self, now: SessionTime) {
        FrontendEndpoint::receive_eof(self, now);
    }

    fn transport_failed(&mut self, kind: ErrorKind, now: SessionTime) {
        FrontendEndpoint::transport_failed(self, kind, now);
    }

    fn poll_transmit(&mut self, now: SessionTime) -> Option<Vec<u8>> {
        FrontendEndpoint::poll_transmit(self, now)
    }

    fn tick(&mut self, now: SessionTime) {
        FrontendEndpoint::tick(self, now);
    }

    fn closed(&self) -> Option<CloseOutcome> {
        FrontendEndpoint::closed(self).cloned()
    }
}

/// One link of an evaluator endpoint, as a [`Driver`].
#[derive(Debug)]
pub struct EvaluatorLink<'a> {
    endpoint: &'a mut EvaluatorEndpoint,
    link: LinkId,
}

impl<'a> EvaluatorLink<'a> {
    #[must_use]
    pub fn new(endpoint: &'a mut EvaluatorEndpoint, link: LinkId) -> Self {
        Self { endpoint, link }
    }
}

impl Driver for EvaluatorLink<'_> {
    fn receive(&mut self, bytes: &[u8], now: SessionTime) {
        self.endpoint.receive(self.link, bytes, now);
    }

    fn receive_eof(&mut self, now: SessionTime) {
        self.endpoint.receive_eof(self.link, now);
    }

    fn transport_failed(&mut self, kind: ErrorKind, now: SessionTime) {
        self.endpoint.transport_failed(self.link, kind, now);
    }

    fn poll_transmit(&mut self, now: SessionTime) -> Option<Vec<u8>> {
        self.endpoint.poll_transmit(self.link, now)
    }

    fn tick(&mut self, now: SessionTime) {
        self.endpoint.tick(now);
    }

    fn closed(&self) -> Option<CloseOutcome> {
        self.endpoint.link_closed(self.link).cloned()
    }
}

/// The result of one pump step.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StepOutcome {
    Progress,
    /// Nothing moved: the host waits for readiness or a timer before the
    /// next step.
    Idle,
    Closed(CloseOutcome),
}

const READ_BUFFER: usize = 16 * 1024;

/// Moves bytes between one transport and one [`Driver`].
#[derive(Debug)]
pub struct Pump<IO, C> {
    io: IO,
    clock: C,
    unwritten: Vec<u8>,
    read_buffer: Box<[u8]>,
    eof: bool,
}

fn retry_later(kind: ErrorKind) -> bool {
    matches!(
        kind,
        ErrorKind::WouldBlock | ErrorKind::TimedOut | ErrorKind::Interrupted
    )
}

impl<IO: Read + Write, C: Clock> Pump<IO, C> {
    pub fn new(io: IO, clock: C) -> Self {
        Self {
            io,
            clock,
            unwritten: Vec::new(),
            read_buffer: vec![0; READ_BUFFER].into_boxed_slice(),
            eof: false,
        }
    }

    pub fn io_mut(&mut self) -> &mut IO {
        &mut self.io
    }

    pub fn into_inner(self) -> IO {
        self.io
    }

    fn finished<D: Driver>(&self, driver: &D) -> Option<CloseOutcome> {
        driver.closed().filter(|_| self.unwritten.is_empty())
    }

    /// Write what the driver has, read what the transport has, tick timers.
    pub fn step<D: Driver>(&mut self, driver: &mut D) -> StepOutcome {
        let now = self.clock.now();
        driver.tick(now);
        let mut progress = false;
        loop {
            if self.unwritten.is_empty() {
                match driver.poll_transmit(now) {
                    Some(bytes) => self.unwritten = bytes,
                    None => break,
                }
            }
            match self.io.write(&self.unwritten) {
                Ok(0) => {
                    driver.transport_failed(ErrorKind::WriteZero, now);
                    self.unwritten.clear();
                    break;
                }
                Ok(written) => {
                    self.unwritten.drain(..written);
                    progress = true;
                }
                Err(error) if retry_later(error.kind()) => break,
                Err(error) => {
                    driver.transport_failed(error.kind(), now);
                    self.unwritten.clear();
                    break;
                }
            }
        }
        if let Err(error) = self.io.flush()
            && !retry_later(error.kind())
        {
            driver.transport_failed(error.kind(), now);
        }
        if let Some(outcome) = self.finished(driver) {
            return StepOutcome::Closed(outcome);
        }
        if !self.eof {
            match self.io.read(&mut self.read_buffer) {
                Ok(0) => {
                    self.eof = true;
                    driver.receive_eof(now);
                    progress = true;
                }
                Ok(read) => {
                    driver.receive(&self.read_buffer[..read], now);
                    progress = true;
                }
                Err(error) if retry_later(error.kind()) => {}
                Err(error) => driver.transport_failed(error.kind(), now),
            }
        }
        if let Some(outcome) = self.finished(driver) {
            return StepOutcome::Closed(outcome);
        }
        if progress {
            StepOutcome::Progress
        } else {
            StepOutcome::Idle
        }
    }
}

/// A connected pair of non-blocking Unix stream sockets, for a spawned
/// frontend on an inherited descriptor and for tests.
#[cfg(unix)]
pub fn socket_pair() -> std::io::Result<(
    std::os::unix::net::UnixStream,
    std::os::unix::net::UnixStream,
)> {
    let (a, b) = std::os::unix::net::UnixStream::pair()?;
    a.set_nonblocking(true)?;
    b.set_nonblocking(true)?;
    Ok((a, b))
}
