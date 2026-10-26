use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;

const CAST_BUFFER_CAPACITY: usize = 256 * 1024;
const CAST_FLUSH_INTERVAL: Duration = Duration::from_secs(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TerminalSize {
    rows: u16,
    columns: u16,
}

impl TerminalSize {
    pub(crate) const fn new(rows: u16, columns: u16) -> Self {
        Self { rows, columns }
    }

    pub(crate) const fn rows(self) -> u16 {
        self.rows
    }
    pub(crate) const fn columns(self) -> u16 {
        self.columns
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct CastHeader {
    size: TerminalSize,
    terminal_type: String,
    title: String,
}

impl CastHeader {
    pub(crate) fn new(
        size: TerminalSize,
        terminal_type: impl Into<String>,
        title: impl Into<String>,
    ) -> Self {
        Self {
            size,
            terminal_type: terminal_type.into(),
            title: title.into(),
        }
    }
}

/// Process status at the asciicast boundary; arbitrary mode integers cannot
/// enter recording finalization. The raw integer exists only in wire encoding.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[repr(transparent)]
pub(crate) struct RecordingExitStatus(i32);

impl RecordingExitStatus {
    #[cfg(test)]
    pub(crate) const SUCCESS: Self = Self(0);
    pub(crate) const FAILURE: Self = Self(1);
    const fn as_i32(self) -> i32 {
        self.0
    }
}

impl From<std::process::ExitStatus> for RecordingExitStatus {
    fn from(status: std::process::ExitStatus) -> Self {
        use std::os::unix::process::ExitStatusExt;
        Self(
            status
                .code()
                .or_else(|| status.signal().map(|signal| 128 + signal))
                .unwrap_or(1),
        )
    }
}

const _: () = assert!(std::mem::size_of::<RecordingExitStatus>() == std::mem::size_of::<i32>());

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CastEvent {
    Output(String),
    Input(String),
    Resize(TerminalSize),
    Marker(String),
    Exit(RecordingExitStatus),
}

/// Pacing applied around each key send of a recorded run, in milliseconds,
/// when `NEOMACS_TUI_RECORD_DELAY_MS` is unset. A scenario that drives
/// hundreds of keys in a few seconds otherwise produces a cast nobody can
/// follow.
///
/// One ordered send is followed by this playback delay. Paired drivers pay
/// one delay per editor. The delay proves neither redisplay nor startup
/// completion; CI disables it and retains its existing readiness policy.
pub(crate) const DEFAULT_RECORDING_DELAY: Duration = Duration::from_millis(2000);

/// Whether TUI sessions emit replayable terminal recordings.
///
/// Recording is off by default. Set `NEOMACS_TUI_RECORD=on` when replayable
/// artifacts are wanted.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum RecordingPolicy {
    #[default]
    Off,
    On,
}

impl RecordingPolicy {
    pub(crate) fn parse(value: Option<&OsStr>) -> Result<Self, String> {
        match value {
            None => Ok(Self::Off),
            Some(value) if value.is_empty() || value == "off" => Ok(Self::Off),
            Some(value) if value == "on" => Ok(Self::On),
            Some(value) => Err(format!(
                "NEOMACS_TUI_RECORD must be `on` or `off`, got {value:?}"
            )),
        }
    }
}

fn parse_recording_delay(value: Option<&OsStr>) -> Result<Duration, String> {
    let Some(value) = value else {
        return Ok(DEFAULT_RECORDING_DELAY);
    };
    // No trimming: a value is either a plain millisecond count or a mistake
    // worth reporting. `0` is a legitimate explicit opt-out of pacing.
    value
        .to_str()
        .and_then(|text| text.parse::<u64>().ok())
        .map(Duration::from_millis)
        .ok_or_else(|| {
            format!(
                "NEOMACS_TUI_RECORD_DELAY_MS must be a whole number of milliseconds, got {value:?}"
            )
        })
}

/// Whether sessions record, and how much the harness paces itself when they do.
///
/// The delay exists only to make a recording watchable, so it is resolved
/// to zero whenever the policy is [`RecordingPolicy::Off`]: `RecordingConfig`
/// is the only thing that can produce a delay, and every constructor here
/// drops it for a non-recording run. `NEOMACS_TUI_RECORD_DELAY_MS` is
/// therefore not even consulted when recording is off -- a normal run stays
/// exactly the execution it is today, a typo in a recording-only variable
/// included.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct RecordingConfig {
    policy: RecordingPolicy,
    delay: Duration,
}

impl RecordingConfig {
    pub(crate) fn new(policy: RecordingPolicy, delay: Duration) -> Self {
        Self {
            policy,
            delay: match policy {
                RecordingPolicy::Off => Duration::ZERO,
                RecordingPolicy::On => delay,
            },
        }
    }

    /// Parse `NEOMACS_TUI_RECORD` and `NEOMACS_TUI_RECORD_DELAY_MS`.
    pub(crate) fn parse(record: Option<&OsStr>, delay: Option<&OsStr>) -> Result<Self, String> {
        let policy = RecordingPolicy::parse(record)?;
        let delay = match policy {
            RecordingPolicy::Off => Duration::ZERO,
            RecordingPolicy::On => parse_recording_delay(delay)?,
        };
        Ok(Self::new(policy, delay))
    }

    pub(crate) fn policy(&self) -> RecordingPolicy {
        self.policy
    }

    /// How long a session waits on each side of a key it sends while
    /// recording: `sleep, key, sleep`.
    pub(crate) fn delay(&self) -> Duration {
        self.delay
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct RecordingIdentity {
    suite: String,
    test: String,
    session: String,
}

/// Stable directory identity for recordings produced by one TUI test or
/// package scenario.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TuiRecordingScope {
    suite: String,
    test: String,
}

impl TuiRecordingScope {
    pub fn new(suite: impl Into<String>, test: impl Into<String>) -> Self {
        Self {
            suite: suite.into(),
            test: test.into(),
        }
    }

    pub(crate) fn current() -> Self {
        let test = std::thread::current()
            .name()
            .unwrap_or("unnamed-tui-test")
            .to_owned();
        Self::new(env!("CARGO_PKG_NAME"), test)
    }

    pub(crate) fn session(&self, session: impl Into<String>) -> RecordingIdentity {
        RecordingIdentity::new(self.suite.clone(), self.test.clone(), session)
    }
}

impl RecordingIdentity {
    pub(crate) fn new(
        suite: impl Into<String>,
        test: impl Into<String>,
        session: impl Into<String>,
    ) -> Self {
        Self {
            suite: suite.into(),
            test: test.into(),
            session: session.into(),
        }
    }

    /// The `NEOMACS_TTY_DAMAGE_REPORT_TAG` of the editor this identity
    /// records: the test and the session, as one token.
    pub(crate) fn damage_report_tag(&self) -> String {
        format!("{}[{}]", self.test, self.session)
            .chars()
            .map(|ch| if ch.is_whitespace() { '_' } else { ch })
            .collect()
    }

    fn title(&self) -> String {
        format!("{} [{}]", self.test, self.session)
    }

    fn artifact_path(&self, root: &Path) -> PathBuf {
        let path = root
            .join(safe_path_component(&self.suite))
            .join(safe_path_component(&self.test))
            .join(format!("{}.cast", session_file_stem(&self.session)));
        unique_path_for_process(path)
    }
}

fn unique_path_for_process(path: PathBuf) -> PathBuf {
    static OCCURRENCES: OnceLock<Mutex<std::collections::HashMap<PathBuf, u32>>> = OnceLock::new();
    let mut occurrences = OCCURRENCES
        .get_or_init(|| Mutex::new(std::collections::HashMap::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let occurrence = occurrences.entry(path.clone()).or_default();
    *occurrence += 1;
    if *occurrence == 1 {
        return path;
    }

    let stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or("session");
    path.with_file_name(format!("{stem}-{}.cast", *occurrence))
}

fn safe_path_component(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    let mut in_separator = false;
    for character in value.chars() {
        if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
            result.push(character.to_ascii_lowercase());
            in_separator = false;
        } else if !result.is_empty() && !in_separator {
            result.push('-');
            in_separator = true;
        }
    }
    while result.ends_with('-') {
        result.pop();
    }
    if result.is_empty() {
        "unnamed".to_owned()
    } else {
        result
    }
}

fn session_file_stem(session: &str) -> String {
    match session.to_ascii_lowercase().as_str() {
        "gnu" => "gnu".to_owned(),
        "neo" | "neomacs" => "neomacs".to_owned(),
        _ => safe_path_component(session),
    }
}

pub(crate) struct AsciicastV3Writer<W> {
    output: W,
}

impl<W: Write> AsciicastV3Writer<W> {
    pub(crate) fn new(mut output: W, header: CastHeader) -> io::Result<Self> {
        let header = json!({
            "version": 3,
            "term": {
                "cols": header.size.columns,
                "rows": header.size.rows,
                "type": header.terminal_type,
            },
            "title": header.title,
        });
        serde_json::to_writer(&mut output, &header).map_err(io::Error::other)?;
        output.write_all(b"\n")?;
        Ok(Self { output })
    }

    pub(crate) fn write_event(&mut self, interval: Duration, event: CastEvent) -> io::Result<()> {
        let (code, data) = match event {
            CastEvent::Output(data) => ("o", data),
            CastEvent::Input(data) => ("i", data),
            CastEvent::Resize(size) => ("r", format!("{}x{}", size.columns, size.rows)),
            CastEvent::Marker(label) => ("m", label),
            CastEvent::Exit(status) => ("x", status.as_i32().to_string()),
        };
        let event = json!([interval.as_secs_f64(), code, data]);
        serde_json::to_writer(&mut self.output, &event).map_err(io::Error::other)?;
        self.output.write_all(b"\n")
    }

    pub(crate) fn flush(&mut self) -> io::Result<()> {
        self.output.flush()
    }

    #[cfg(test)]
    pub(crate) fn into_inner(self) -> W {
        self.output
    }
}

#[derive(Default)]
struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    fn decode(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut decoded = String::new();
        let mut consumed = 0;

        while consumed < self.pending.len() {
            match std::str::from_utf8(&self.pending[consumed..]) {
                Ok(valid) => {
                    decoded.push_str(valid);
                    consumed = self.pending.len();
                }
                Err(error) => {
                    let valid_end = consumed + error.valid_up_to();
                    // `valid_up_to` is guaranteed to end on a UTF-8 boundary.
                    decoded.push_str(
                        std::str::from_utf8(&self.pending[consumed..valid_end])
                            .expect("validated UTF-8 prefix"),
                    );
                    consumed = valid_end;
                    let Some(invalid_length) = error.error_len() else {
                        break;
                    };
                    decoded.push('\u{fffd}');
                    consumed += invalid_length;
                }
            }
        }

        if consumed != 0 {
            self.pending.drain(..consumed);
        }
        decoded
    }

    fn finish(&mut self) -> String {
        String::from_utf8_lossy(&std::mem::take(&mut self.pending)).into_owned()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecordingLifecycle {
    Open,
    Finished,
    Failed,
}

pub(crate) struct ActiveRecording {
    writer: AsciicastV3Writer<BufWriter<File>>,
    path: PathBuf,
    last_event: Instant,
    last_flush: Instant,
    dirty: bool,
    output: Utf8Stream,
    input: Utf8Stream,
    raw_input: BufWriter<File>,
    lifecycle: RecordingLifecycle,
}

impl ActiveRecording {
    fn start(
        path: PathBuf,
        identity: &RecordingIdentity,
        terminal_type: &str,
        size: TerminalSize,
    ) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        // Keep recording I/O out of timing-sensitive PTY exchanges. Most
        // sessions fit in this buffer and reach the filesystem in one write
        // when the session finishes; unusually large casts stream in bounded
        // chunks instead of growing memory without limit.
        // Every committed prefix also has a lossless length-delimited record.
        // A failed write may end halfway through UTF-8, which asciicast's text
        // input events cannot represent without replacement characters.
        let raw_input = BufWriter::new(File::create(path.with_extension("input"))?);
        let file = BufWriter::with_capacity(CAST_BUFFER_CAPACITY, File::create(&path)?);
        let mut writer =
            AsciicastV3Writer::new(file, CastHeader::new(size, terminal_type, identity.title()))?;
        // A terminated test process may never run Rust destructors. Publish a
        // valid header immediately; subsequent events are checkpointed at a
        // bounded interval while the PTY is being serviced.
        writer.flush()?;
        let now = Instant::now();
        Ok(Self {
            writer,
            path,
            last_event: now,
            last_flush: now,
            dirty: false,
            output: Utf8Stream::default(),
            input: Utf8Stream::default(),
            raw_input,
            lifecycle: RecordingLifecycle::Open,
        })
    }

    fn ensure_open(&self) -> io::Result<()> {
        match self.lifecycle {
            RecordingLifecycle::Open => {}
            RecordingLifecycle::Finished | RecordingLifecycle::Failed => {
                return Err(io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "TUI recording is closed or failed",
                ));
            }
        }
        Ok(())
    }

    fn write_at(&mut self, observed_at: Instant, event: CastEvent) -> io::Result<()> {
        self.ensure_open()?;
        let interval = observed_at
            .checked_duration_since(self.last_event)
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "out-of-order TUI recording observation",
                )
            })?;
        self.writer.write_event(interval, event)?;
        self.last_event = observed_at;
        self.dirty = true;
        Ok(())
    }

    fn write(&mut self, event: CastEvent) -> io::Result<()> {
        self.write_at(Instant::now(), event)
    }

    fn flush_if_due(&mut self) -> io::Result<()> {
        if self.dirty && self.last_flush.elapsed() >= CAST_FLUSH_INTERVAL {
            self.flush()?;
        }
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()?;
        self.last_flush = Instant::now();
        self.dirty = false;
        Ok(())
    }

    fn output_at(&mut self, observed_at: Instant, bytes: &[u8]) -> io::Result<()> {
        self.ensure_open()?;
        let decoded = self.output.decode(bytes);
        if !decoded.is_empty() {
            self.write_at(observed_at, CastEvent::Output(decoded))?;
        }
        Ok(())
    }

    fn input_at(&mut self, observed_at: Instant, bytes: &[u8]) -> io::Result<()> {
        self.ensure_open()?;
        let length = u64::try_from(bytes.len()).map_err(io::Error::other)?;
        self.raw_input.write_all(&length.to_le_bytes())?;
        self.raw_input.write_all(bytes)?;
        self.raw_input.flush()?;
        let decoded = self.input.decode(bytes);
        if !decoded.is_empty() {
            self.write_at(observed_at, CastEvent::Input(decoded))?;
        }
        Ok(())
    }

    fn finish(&mut self, status: RecordingExitStatus) -> io::Result<()> {
        match self.lifecycle {
            RecordingLifecycle::Finished => return Ok(()),
            RecordingLifecycle::Failed => {
                return Err(io::Error::other("TUI recording failed before completion"));
            }
            RecordingLifecycle::Open => {}
        }
        let output_tail = self.output.finish();
        if !output_tail.is_empty() {
            self.write(CastEvent::Output(output_tail))?;
        }
        let input_tail = self.input.finish();
        if !input_tail.is_empty() {
            self.write(CastEvent::Input(input_tail))?;
        }
        self.write(CastEvent::Exit(status))?;
        self.flush()?;
        self.raw_input.flush()?;
        self.lifecycle = RecordingLifecycle::Finished;
        Ok(())
    }
}

pub(crate) enum SessionRecording {
    Off,
    On(ActiveRecording),
}

impl SessionRecording {
    pub(crate) fn start(
        policy: RecordingPolicy,
        root: &Path,
        identity: RecordingIdentity,
        terminal_type: &str,
        size: TerminalSize,
    ) -> io::Result<Self> {
        match policy {
            RecordingPolicy::Off => Ok(Self::Off),
            RecordingPolicy::On => {
                let path = identity.artifact_path(root);
                ActiveRecording::start(path, &identity, terminal_type, size).map(Self::On)
            }
        }
    }

    fn apply(
        &mut self,
        action: impl FnOnce(&mut ActiveRecording) -> io::Result<()>,
    ) -> io::Result<()> {
        match self {
            Self::Off => Ok(()),
            Self::On(recording) => match action(recording) {
                Ok(()) => Ok(()),
                Err(error) => {
                    recording.lifecycle = RecordingLifecycle::Failed;
                    Err(error)
                }
            },
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::Off => None,
            Self::On(recording) => Some(&recording.path),
        }
    }

    pub(crate) fn output_at(&mut self, observed_at: Instant, bytes: &[u8]) -> io::Result<()> {
        self.apply(|recording| recording.output_at(observed_at, bytes))
    }

    pub(crate) fn input_at(&mut self, observed_at: Instant, bytes: &[u8]) -> io::Result<()> {
        self.apply(|recording| recording.input_at(observed_at, bytes))
    }

    pub(crate) fn resize_at(&mut self, observed_at: Instant, size: TerminalSize) -> io::Result<()> {
        self.apply(|recording| recording.write_at(observed_at, CastEvent::Resize(size)))
    }

    pub(crate) fn marker_at(&mut self, observed_at: Instant, label: &str) -> io::Result<()> {
        self.apply(|recording| recording.write_at(observed_at, CastEvent::Marker(label.to_owned())))
    }

    pub(crate) fn flush_if_due(&mut self) -> io::Result<()> {
        self.apply(ActiveRecording::flush_if_due)
    }

    pub(crate) fn finish(&mut self, status: RecordingExitStatus) -> io::Result<()> {
        self.apply(|recording| recording.finish(status))
    }

    #[cfg(test)]
    pub(crate) fn output(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.output_at(Instant::now(), bytes)
    }
    #[cfg(test)]
    pub(crate) fn input(&mut self, bytes: &[u8]) -> io::Result<()> {
        self.input_at(Instant::now(), bytes)
    }
    #[cfg(test)]
    pub(crate) fn resize(&mut self, size: TerminalSize) -> io::Result<()> {
        self.resize_at(Instant::now(), size)
    }
    #[cfg(test)]
    pub(crate) fn marker(&mut self, label: &str) -> io::Result<()> {
        self.marker_at(Instant::now(), label)
    }
}

#[cfg(test)]
#[path = "recording/tests/recording_test.rs"]
mod tests;
