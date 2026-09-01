use std::fs;
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::thread::JoinHandle;
use std::time::Duration;

#[cfg(target_os = "linux")]
use std::fs::File;
#[cfg(target_os = "linux")]
use std::io::{Read, Write};
#[cfg(target_os = "linux")]
use std::net::{Ipv4Addr, TcpListener, TcpStream};
#[cfg(target_os = "linux")]
use std::thread;
#[cfg(target_os = "linux")]
use std::time::Instant;

#[cfg(target_os = "linux")]
use rustix::event::{PollFd, PollFlags, Timespec, poll};
#[cfg(target_os = "linux")]
use rustix::fd::AsFd;
#[cfg(target_os = "linux")]
use rustix::fs::{CWD, Mode, OFlags, mkfifoat, open};
#[cfg(target_os = "linux")]
use rustix::io::Errno;

#[cfg(target_os = "linux")]
const CANCELLATION_POLL_INTERVAL: Duration = Duration::from_millis(50);
#[cfg(target_os = "linux")]
const MAX_COMMAND_BYTES: usize = 16;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct PerfControlPaths {
    pub(crate) command: PathBuf,
    pub(crate) acknowledgement: PathBuf,
}

pub(crate) struct ProfileGate {
    endpoint: SocketAddr,
    control: PerfControlPaths,
    cancellation: Arc<AtomicBool>,
    worker: Option<JoinHandle<Result<(), String>>>,
}

impl ProfileGate {
    #[cfg(target_os = "linux")]
    pub(crate) fn start(directory: &Path, timeout: Duration) -> Result<Self, String> {
        let control = PerfControlPaths {
            command: directory.join("perf-control.fifo"),
            acknowledgement: directory.join("perf-ack.fifo"),
        };
        let mut cleanup = ControlFileCleanup::new(control.clone());
        for path in [&control.command, &control.acknowledgement] {
            if path.as_os_str().to_string_lossy().contains(',') {
                return Err(format!(
                    "perf control FIFO path cannot contain a comma: {}",
                    path.display()
                ));
            }
            mkfifoat(CWD, path, Mode::RUSR | Mode::WUSR).map_err(|error| {
                format!(
                    "failed to create perf control FIFO {}: {error}",
                    path.display()
                )
            })?;
        }
        let command = open(
            &control.command,
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(|error| {
            format!(
                "failed to open perf command FIFO {}: {error}",
                control.command.display()
            )
        })?;
        let acknowledgement = open(
            &control.acknowledgement,
            OFlags::RDWR | OFlags::NONBLOCK | OFlags::CLOEXEC,
            Mode::empty(),
        )
        .map(File::from)
        .map_err(|error| {
            format!(
                "failed to open perf acknowledgement FIFO {}: {error}",
                control.acknowledgement.display()
            )
        })?;
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .map_err(|error| format!("failed to bind edit-loop profile gate: {error}"))?;
        listener
            .set_nonblocking(true)
            .map_err(|error| format!("failed to configure edit-loop profile gate: {error}"))?;
        let endpoint = listener
            .local_addr()
            .map_err(|error| format!("failed to inspect edit-loop profile gate: {error}"))?;
        let cancellation = Arc::new(AtomicBool::new(false));
        let worker_cancellation = Arc::clone(&cancellation);
        let worker = thread::Builder::new()
            .name("neomacs-perf-gate".to_string())
            .spawn(move || {
                run_profile_gate(
                    listener,
                    command,
                    acknowledgement,
                    timeout,
                    &worker_cancellation,
                )
            })
            .map_err(|error| format!("failed to start edit-loop profile gate: {error}"))?;
        cleanup.disarm();
        Ok(Self {
            endpoint,
            control,
            cancellation,
            worker: Some(worker),
        })
    }

    #[cfg(not(target_os = "linux"))]
    pub(crate) fn start(_directory: &Path, _timeout: Duration) -> Result<Self, String> {
        Err("edit-loop profile control requires Linux perf".to_string())
    }

    pub(crate) const fn endpoint(&self) -> SocketAddr {
        self.endpoint
    }

    pub(crate) const fn control_paths(&self) -> &PerfControlPaths {
        &self.control
    }

    pub(crate) fn finish(&mut self) -> Result<(), String> {
        let Some(worker) = self.worker.take() else {
            return Err("edit-loop profile gate was already finished".to_string());
        };
        // The editor process has exited before this method is called, so its
        // socket is closed and the worker can validate that no bytes followed
        // the acknowledged `disable`. Cancelling here would race that final
        // EOF and could turn a valid profile into a false rejection.
        let result = worker
            .join()
            .map_err(|_| "edit-loop profile gate panicked".to_string())
            .and_then(|result| result);
        remove_control_files(&self.control);
        result
    }
}

impl Drop for ProfileGate {
    fn drop(&mut self) {
        self.cancellation.store(true, Ordering::Release);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        remove_control_files(&self.control);
    }
}

struct ControlFileCleanup {
    control: PerfControlPaths,
    armed: bool,
}

impl ControlFileCleanup {
    const fn new(control: PerfControlPaths) -> Self {
        Self {
            control,
            armed: true,
        }
    }

    const fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for ControlFileCleanup {
    fn drop(&mut self) {
        if self.armed {
            remove_control_files(&self.control);
        }
    }
}

fn remove_control_files(control: &PerfControlPaths) {
    for path in [&control.command, &control.acknowledgement] {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => {}
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SamplingCommand {
    Enable,
    Disable,
}

#[cfg(target_os = "linux")]
impl SamplingCommand {
    fn parse(line: &[u8]) -> Result<Self, String> {
        match line {
            b"enable\n" => Ok(Self::Enable),
            b"disable\n" => Ok(Self::Disable),
            _ => Err(format!(
                "unknown edit-loop profile command {:?}",
                String::from_utf8_lossy(line)
            )),
        }
    }

    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::Enable => b"enable\n",
            Self::Disable => b"disable\n",
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Enable => "enable",
            Self::Disable => "disable",
        }
    }
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum SamplingState {
    Disabled,
    Enabled,
    Finished,
}

#[cfg(target_os = "linux")]
impl SamplingState {
    const fn expected(self) -> Option<SamplingCommand> {
        match self {
            Self::Disabled => Some(SamplingCommand::Enable),
            Self::Enabled => Some(SamplingCommand::Disable),
            Self::Finished => None,
        }
    }

    const fn advance(self) -> Self {
        match self {
            Self::Disabled => Self::Enabled,
            Self::Enabled => Self::Finished,
            Self::Finished => Self::Finished,
        }
    }
}

#[cfg(target_os = "linux")]
enum EditorMessage {
    Command(SamplingCommand),
    Disconnected,
}

#[cfg(target_os = "linux")]
struct GateDeadline<'a> {
    started: Instant,
    timeout: Duration,
    cancellation: &'a AtomicBool,
}

#[cfg(target_os = "linux")]
impl<'a> GateDeadline<'a> {
    fn new(timeout: Duration, cancellation: &'a AtomicBool) -> Self {
        Self {
            started: Instant::now(),
            timeout,
            cancellation,
        }
    }

    fn wait_for<Fd: AsFd>(
        &self,
        descriptor: &Fd,
        interest: PollFlags,
        operation: &str,
    ) -> Result<(), String> {
        loop {
            let remaining = self.remaining(operation)?;
            let wait = remaining.min(CANCELLATION_POLL_INTERVAL);
            let timeout = Timespec::try_from(wait)
                .map_err(|_| format!("edit-loop profile timeout is too large while {operation}"))?;
            let mut descriptor = [PollFd::new(descriptor, interest)];
            match poll(&mut descriptor, Some(&timeout)) {
                Ok(0) => {}
                Ok(_) => {
                    let readiness = descriptor[0].revents();
                    if readiness.contains(PollFlags::NVAL) {
                        return Err(format!(
                            "edit-loop profile descriptor became invalid while {operation}"
                        ));
                    }
                    if readiness.intersects(interest | PollFlags::ERR | PollFlags::HUP) {
                        return Ok(());
                    }
                }
                Err(Errno::INTR) => {}
                Err(error) => {
                    return Err(format!(
                        "failed to poll edit-loop profile descriptor while {operation}: {error}"
                    ));
                }
            }
        }
    }

    fn remaining(&self, operation: &str) -> Result<Duration, String> {
        if self.cancellation.load(Ordering::Acquire) {
            return Err(format!(
                "edit-loop profile gate cancelled while {operation}"
            ));
        }
        self.timeout
            .checked_sub(self.started.elapsed())
            .ok_or_else(|| format!("edit-loop profile gate timed out while {operation}"))
    }
}

#[cfg(target_os = "linux")]
fn run_profile_gate(
    listener: TcpListener,
    mut control: File,
    mut acknowledgement: File,
    timeout: Duration,
    cancellation: &AtomicBool,
) -> Result<(), String> {
    let deadline = GateDeadline::new(timeout, cancellation);
    let mut stream = accept_editor(&listener, &deadline)?;
    let mut state = SamplingState::Disabled;
    let mut editor_bytes = Vec::new();
    let mut acknowledgement_bytes = Vec::new();

    loop {
        let message = read_editor_message(&mut stream, &mut editor_bytes, &deadline)?;
        let EditorMessage::Command(command) = message else {
            return match state {
                SamplingState::Finished => Ok(()),
                SamplingState::Disabled => {
                    Err("editor disconnected before enabling edit-loop sampling".to_string())
                }
                SamplingState::Enabled => {
                    Err("editor disconnected before disabling edit-loop sampling".to_string())
                }
            };
        };
        let Some(expected) = state.expected() else {
            return reject_editor(
                &mut stream,
                &deadline,
                format!(
                    "invalid edit-loop profile transition: sampling already finished, received `{}`",
                    command.name()
                ),
            );
        };
        if command != expected {
            return reject_editor(
                &mut stream,
                &deadline,
                format!(
                    "invalid edit-loop profile transition: expected `{}`, received `{}`",
                    expected.name(),
                    command.name()
                ),
            );
        }
        write_nonblocking(
            &mut control,
            command.bytes(),
            &deadline,
            "perf control FIFO",
        )?;
        read_perf_acknowledgement(&mut acknowledgement, &mut acknowledgement_bytes, &deadline)?;
        state = state.advance();
        write_nonblocking(
            &mut stream,
            b"ack\n",
            &deadline,
            "editor sampling acknowledgement",
        )?;
    }
}

#[cfg(target_os = "linux")]
fn accept_editor(listener: &TcpListener, deadline: &GateDeadline<'_>) -> Result<TcpStream, String> {
    loop {
        deadline.wait_for(listener, PollFlags::IN, "waiting for editor connection")?;
        match listener.accept() {
            Ok((stream, _)) => {
                stream
                    .set_nonblocking(true)
                    .map_err(|error| format!("failed to configure profile gate client: {error}"))?;
                return Ok(stream);
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("failed to accept profile gate client: {error}")),
        }
    }
}

#[cfg(target_os = "linux")]
fn read_editor_message(
    stream: &mut TcpStream,
    buffered: &mut Vec<u8>,
    deadline: &GateDeadline<'_>,
) -> Result<EditorMessage, String> {
    loop {
        if let Some(newline) = buffered.iter().position(|byte| *byte == b'\n') {
            let line_length = newline + 1;
            if line_length > MAX_COMMAND_BYTES {
                return reject_editor(
                    stream,
                    deadline,
                    format!("edit-loop profile command exceeded {MAX_COMMAND_BYTES} bytes"),
                );
            }
            let line = buffered.drain(..line_length).collect::<Vec<_>>();
            return SamplingCommand::parse(&line)
                .map(EditorMessage::Command)
                .or_else(|message| reject_editor(stream, deadline, message));
        }
        if buffered.len() > MAX_COMMAND_BYTES {
            return reject_editor(
                stream,
                deadline,
                format!("edit-loop profile command exceeded {MAX_COMMAND_BYTES} bytes"),
            );
        }
        deadline.wait_for(stream, PollFlags::IN, "waiting for sampling boundary")?;
        let mut chunk = [0_u8; 64];
        match stream.read(&mut chunk) {
            Ok(0) if buffered.is_empty() => return Ok(EditorMessage::Disconnected),
            Ok(0) => {
                return reject_editor(
                    stream,
                    deadline,
                    "editor disconnected with an incomplete sampling command".to_string(),
                );
            }
            Ok(read) => buffered.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("failed to read profile gate command: {error}")),
        }
    }
}

#[cfg(target_os = "linux")]
fn reject_editor<T>(
    stream: &mut TcpStream,
    deadline: &GateDeadline<'_>,
    message: String,
) -> Result<T, String> {
    let _ = write_nonblocking(stream, b"error\n", deadline, "editor rejection response");
    Err(message)
}

#[cfg(target_os = "linux")]
fn write_nonblocking<Writer: Write + AsFd>(
    writer: &mut Writer,
    mut bytes: &[u8],
    deadline: &GateDeadline<'_>,
    target: &str,
) -> Result<(), String> {
    while !bytes.is_empty() {
        deadline.wait_for(writer, PollFlags::OUT, target)?;
        match writer.write(bytes) {
            Ok(0) => return Err(format!("{target} closed while forwarding command")),
            Ok(written) => bytes = &bytes[written..],
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("failed to write {target}: {error}")),
        }
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn read_perf_acknowledgement(
    reader: &mut File,
    buffered: &mut Vec<u8>,
    deadline: &GateDeadline<'_>,
) -> Result<(), String> {
    loop {
        let padding = buffered
            .iter()
            .position(|byte| *byte != 0)
            .unwrap_or(buffered.len());
        buffered.drain(..padding);
        if buffered.len() >= b"ack\n".len() {
            if !buffered.starts_with(b"ack\n") {
                return Err(format!(
                    "perf returned an invalid acknowledgement: {:?}",
                    String::from_utf8_lossy(buffered)
                ));
            }
            buffered.drain(..b"ack\n".len());
            return Ok(());
        }
        deadline.wait_for(reader, PollFlags::IN, "waiting for perf acknowledgement")?;
        let mut chunk = [0_u8; 16];
        match reader.read(&mut chunk) {
            Ok(0) => {
                return Err("perf acknowledgement FIFO closed before acknowledgement".to_string());
            }
            Ok(read) => buffered.extend_from_slice(&chunk[..read]),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(format!("failed to read perf acknowledgement: {error}")),
        }
    }
}
