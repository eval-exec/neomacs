//! Test transports and a fake clock (feature `testing`).
//!
//! The fault adapters wrap any `Read + Write` and model what real links do:
//! 1-byte I/O, random splits, a half-close, EOF in the middle of a record, a
//! stall without EOF, and a cut right after the Nth record. Each one makes a
//! pump end in a typed outcome within bounded time.

use std::collections::VecDeque;
use std::io::{self, ErrorKind, Read, Write};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::lane::Lane;
use crate::preface::PREFACE_LEN;
use crate::record::{Compression, Continuation, HEADER_LEN, RecordHeader, ServiceSlot};
use crate::{Clock, SessionTime};

/// A clock that moves only when told to, or by a fixed step per reading.
#[derive(Clone, Debug)]
pub struct FakeClock {
    nanos: Arc<AtomicU64>,
    step: Duration,
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeClock {
    #[must_use]
    pub fn new() -> Self {
        Self {
            nanos: Arc::new(AtomicU64::new(0)),
            step: Duration::ZERO,
        }
    }

    /// Every `now()` reading advances the clock by `step`, so a busy loop
    /// sees time pass deterministically.
    #[must_use]
    pub fn auto_advancing(step: Duration) -> Self {
        Self {
            nanos: Arc::new(AtomicU64::new(0)),
            step,
        }
    }

    pub fn advance(&self, by: Duration) {
        self.nanos.fetch_add(
            u64::try_from(by.as_nanos()).unwrap_or(u64::MAX),
            Ordering::SeqCst,
        );
    }
}

impl Clock for FakeClock {
    fn now(&self) -> SessionTime {
        let step = u64::try_from(self.step.as_nanos()).unwrap_or(u64::MAX);
        let nanos = self.nanos.fetch_add(step, Ordering::SeqCst);
        SessionTime::from_duration(Duration::from_nanos(nanos))
    }
}

/// Closing the write half of a duplex transport.
pub trait ShutdownWrite {
    fn shutdown_write(&mut self);
}

#[cfg(unix)]
impl ShutdownWrite for std::os::unix::net::UnixStream {
    fn shutdown_write(&mut self) {
        let _ = self.shutdown(std::net::Shutdown::Write);
    }
}

#[derive(Debug, Default)]
struct Direction {
    bytes: VecDeque<u8>,
    closed: bool,
}

#[derive(Debug, Default)]
struct PipeState {
    directions: [Direction; 2],
}

/// One end of an in-memory duplex byte stream. Reading an empty stream gives
/// `WouldBlock` until the peer shuts its write half, then end of stream.
#[derive(Debug)]
pub struct MemoryEnd {
    state: Arc<Mutex<PipeState>>,
    side: usize,
}

/// A connected pair of in-memory stream ends.
#[must_use]
pub fn memory_pipe() -> (MemoryEnd, MemoryEnd) {
    let state = Arc::new(Mutex::new(PipeState::default()));
    (
        MemoryEnd {
            state: Arc::clone(&state),
            side: 0,
        },
        MemoryEnd { state, side: 1 },
    )
}

impl MemoryEnd {
    pub fn shutdown_write(&mut self) {
        self.state.lock().expect("pipe lock").directions[self.side].closed = true;
    }
}

impl ShutdownWrite for MemoryEnd {
    fn shutdown_write(&mut self) {
        MemoryEnd::shutdown_write(self);
    }
}

impl Read for MemoryEnd {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut state = self.state.lock().expect("pipe lock");
        let incoming = &mut state.directions[1 - self.side];
        if incoming.bytes.is_empty() {
            return if incoming.closed {
                Ok(0)
            } else {
                Err(ErrorKind::WouldBlock.into())
            };
        }
        let take = buf.len().min(incoming.bytes.len());
        for (slot, byte) in buf.iter_mut().zip(incoming.bytes.drain(..take)) {
            *slot = byte;
        }
        Ok(take)
    }
}

impl Write for MemoryEnd {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut state = self.state.lock().expect("pipe lock");
        let outgoing = &mut state.directions[self.side];
        if outgoing.closed {
            return Err(ErrorKind::BrokenPipe.into());
        }
        outgoing.bytes.extend(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Reads and writes at most one byte per call.
#[derive(Debug)]
pub struct OneByteIo<T>(T);

impl<T> OneByteIo<T> {
    pub fn new(inner: T) -> Self {
        Self(inner)
    }
}

impl<T: Read> Read for OneByteIo<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let len = buf.len().min(1);
        self.0.read(&mut buf[..len])
    }
}

impl<T: Write> Write for OneByteIo<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let len = buf.len().min(1);
        self.0.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}

/// SplitMix64: a small deterministic generator, so no dependency is needed.
#[derive(Clone, Debug)]
pub struct SplitMix(u64);

impl SplitMix {
    #[must_use]
    pub const fn new(seed: u64) -> Self {
        Self(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A value in `1..=max` (`max` ≥ 1).
    pub fn between_one_and(&mut self, max: usize) -> usize {
        1 + (self.next_u64() % max as u64) as usize
    }
}

/// Splits every read and write at a random length.
#[derive(Debug)]
pub struct RandomSplit<T> {
    inner: T,
    rng: SplitMix,
}

impl<T> RandomSplit<T> {
    pub fn new(inner: T, seed: u64) -> Self {
        Self {
            inner,
            rng: SplitMix::new(seed),
        }
    }
}

impl<T: Read> Read for RandomSplit<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = self.rng.between_one_and(buf.len());
        self.inner.read(&mut buf[..len])
    }
}

impl<T: Write> Write for RandomSplit<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if buf.is_empty() {
            return Ok(0);
        }
        let len = self.rng.between_one_and(buf.len());
        self.inner.write(&buf[..len])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Shuts its own write half once `after` bytes went out, at a write boundary,
/// so the peer reads a clean end of stream while this side can still read.
#[derive(Debug)]
pub struct HalfClose<T> {
    inner: T,
    after: usize,
    written: usize,
    shut: bool,
}

impl<T> HalfClose<T> {
    pub fn after_written(inner: T, after: usize) -> Self {
        Self {
            inner,
            after,
            written: 0,
            shut: false,
        }
    }
}

impl<T: Read> Read for HalfClose<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.inner.read(buf)
    }
}

impl<T: Write + ShutdownWrite> Write for HalfClose<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written >= self.after {
            if !self.shut {
                self.shut = true;
                self.inner.shutdown_write();
            }
            return Err(ErrorKind::BrokenPipe.into());
        }
        let written = self.inner.write(buf)?;
        self.written += written;
        Ok(written)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Reads end of stream after `after` bytes, wherever that falls.
#[derive(Debug)]
pub struct EofAfter<T> {
    inner: T,
    remaining: usize,
}

impl<T> EofAfter<T> {
    pub fn new(inner: T, after: usize) -> Self {
        Self {
            inner,
            remaining: after,
        }
    }
}

impl<T: Read> Read for EofAfter<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let len = buf.len().min(self.remaining);
        let read = self.inner.read(&mut buf[..len])?;
        self.remaining -= read;
        Ok(read)
    }
}

impl<T: Write> Write for EofAfter<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Stops delivering after `after` bytes without ever reporting end of stream.
#[derive(Debug)]
pub struct Stall<T> {
    inner: T,
    remaining: usize,
}

impl<T> Stall<T> {
    pub fn after_read(inner: T, after: usize) -> Self {
        Self {
            inner,
            remaining: after,
        }
    }
}

impl<T: Read> Read for Stall<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Err(ErrorKind::WouldBlock.into());
        }
        let len = buf.len().min(self.remaining);
        let read = self.inner.read(&mut buf[..len])?;
        self.remaining -= read;
        Ok(read)
    }
}

impl<T: Write> Write for Stall<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// Reads end of stream right after the Nth complete record that follows the
/// preface.
#[derive(Debug)]
pub struct CutAfterRecords<T> {
    inner: T,
    records_left: usize,
    position: usize,
    /// Bytes left in the preface, the current header or the current body.
    header: Vec<u8>,
    body_left: usize,
}

impl<T> CutAfterRecords<T> {
    pub fn new(inner: T, records: usize) -> Self {
        Self {
            inner,
            records_left: records,
            position: 0,
            header: Vec::new(),
            body_left: 0,
        }
    }

    /// How many of the next bytes may pass before the cut.
    fn allowance(&self) -> usize {
        if self.records_left == 0 {
            return 0;
        }
        if self.position < PREFACE_LEN {
            return PREFACE_LEN - self.position;
        }
        if self.body_left > 0 {
            return self.body_left;
        }
        HEADER_LEN - self.header.len()
    }

    fn consume(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            if self.position < PREFACE_LEN {
                self.position += 1;
                continue;
            }
            self.position += 1;
            if self.body_left > 0 {
                self.body_left -= 1;
                if self.body_left == 0 {
                    self.records_left -= 1;
                }
                continue;
            }
            self.header.push(byte);
            if self.header.len() == HEADER_LEN {
                let len = u32::from_le_bytes([
                    self.header[0],
                    self.header[1],
                    self.header[2],
                    self.header[3],
                ]) as usize;
                self.header.clear();
                if len == 0 {
                    self.records_left -= 1;
                } else {
                    self.body_left = len;
                }
            }
        }
    }
}

impl<T: Read> Read for CutAfterRecords<T> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let allowance = self.allowance();
        if allowance == 0 {
            return Ok(0);
        }
        let len = buf.len().min(allowance);
        let read = self.inner.read(&mut buf[..len])?;
        self.consume(&buf[..read]);
        Ok(read)
    }
}

impl<T: Write> Write for CutAfterRecords<T> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.inner.flush()
    }
}

/// A one-way link with a fixed byte rate. It accepts a write only when
/// everything written before has been delivered, like a socket whose small
/// send buffer becomes writable again.
#[derive(Debug)]
pub struct SimLink {
    bytes_per_second: u64,
    in_flight: VecDeque<Vec<u8>>,
    front_delivered: usize,
    carry_nanos: u128,
}

impl SimLink {
    #[must_use]
    pub fn new(bytes_per_second: u64) -> Self {
        Self {
            bytes_per_second,
            in_flight: VecDeque::new(),
            front_delivered: 0,
            carry_nanos: 0,
        }
    }

    #[must_use]
    pub fn is_writable(&self) -> bool {
        self.in_flight.is_empty()
    }

    pub fn write(&mut self, bytes: &[u8]) {
        self.in_flight.push_back(bytes.to_vec());
    }

    /// Deliver what the rate allows in `elapsed`, as byte runs in order.
    pub fn deliver(&mut self, elapsed: Duration) -> Vec<Vec<u8>> {
        let budget_nanos =
            elapsed.as_nanos() * u128::from(self.bytes_per_second) + self.carry_nanos;
        let mut budget = (budget_nanos / 1_000_000_000) as usize;
        self.carry_nanos = budget_nanos % 1_000_000_000;
        let mut out = Vec::new();
        while budget > 0 {
            let Some(front) = self.in_flight.front() else {
                break;
            };
            let take = budget.min(front.len() - self.front_delivered);
            out.push(front[self.front_delivered..self.front_delivered + take].to_vec());
            self.front_delivered += take;
            budget -= take;
            if self.front_delivered == front.len() {
                self.in_flight.pop_front();
                self.front_delivered = 0;
            }
        }
        out
    }
}

/// A complete record of a service message, as a peer would write it.
#[must_use]
pub fn service_record(lane: Lane, service: ServiceSlot, body: &[u8]) -> Vec<u8> {
    let header = RecordHeader {
        len: u32::try_from(body.len()).expect("test records are small"),
        lane,
        compression: Compression::Raw,
        continuation: Continuation::Last,
        service,
    };
    let mut record = header.to_bytes().to_vec();
    record.extend_from_slice(body);
    record
}
