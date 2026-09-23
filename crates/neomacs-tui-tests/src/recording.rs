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

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) enum CastEvent {
    Output(String),
    Input(String),
    Resize(TerminalSize),
    Marker(String),
    Exit(i32),
}

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
            CastEvent::Exit(status) => ("x", status.to_string()),
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

pub(crate) struct ActiveRecording {
    writer: AsciicastV3Writer<BufWriter<File>>,
    path: PathBuf,
    last_event: Instant,
    last_flush: Instant,
    dirty: bool,
    output: Utf8Stream,
    input: Utf8Stream,
    finished: bool,
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
            finished: false,
        })
    }

    fn write_at(&mut self, observed_at: Instant, event: CastEvent) -> io::Result<()> {
        let interval = observed_at.saturating_duration_since(self.last_event);
        self.writer.write_event(interval, event)?;
        self.last_event = self.last_event.max(observed_at);
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
        let decoded = self.output.decode(bytes);
        if !decoded.is_empty() {
            self.write_at(observed_at, CastEvent::Output(decoded))?;
        }
        Ok(())
    }

    fn input(&mut self, bytes: &[u8]) -> io::Result<()> {
        let decoded = self.input.decode(bytes);
        if !decoded.is_empty() {
            self.write(CastEvent::Input(decoded))?;
        }
        Ok(())
    }

    fn finish(&mut self, status: i32) -> io::Result<()> {
        if self.finished {
            return Ok(());
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
        self.finished = true;
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
    ) -> Self {
        match policy {
            RecordingPolicy::Off => Self::Off,
            RecordingPolicy::On => {
                let path = identity.artifact_path(root);
                match ActiveRecording::start(path.clone(), &identity, terminal_type, size) {
                    Ok(recording) => Self::On(recording),
                    Err(error) => {
                        eprintln!(
                            "TUI recording disabled after failing to create {}: {error}",
                            path.display()
                        );
                        Self::Off
                    }
                }
            }
        }
    }

    fn active_mut(&mut self) -> Option<&mut ActiveRecording> {
        match self {
            Self::Off => None,
            Self::On(recording) => Some(recording),
        }
    }

    fn apply(
        &mut self,
        operation: &'static str,
        action: impl FnOnce(&mut ActiveRecording) -> io::Result<()>,
    ) {
        let failure = self.active_mut().and_then(|recording| {
            action(recording)
                .err()
                .map(|error| (recording.path.clone(), error))
        });
        if let Some((path, error)) = failure {
            eprintln!(
                "TUI recording disabled after failing to {operation} {}: {error}",
                path.display()
            );
            *self = Self::Off;
        }
    }

    pub(crate) fn path(&self) -> Option<&Path> {
        match self {
            Self::Off => None,
            Self::On(recording) => Some(&recording.path),
        }
    }

    pub(crate) fn output_at(&mut self, observed_at: Instant, bytes: &[u8]) {
        self.apply("write PTY output to", |recording| {
            recording.output_at(observed_at, bytes)
        });
    }

    #[cfg(test)]
    pub(crate) fn output(&mut self, bytes: &[u8]) {
        self.output_at(Instant::now(), bytes);
    }

    pub(crate) fn input(&mut self, bytes: &[u8]) {
        self.apply("write PTY input to", |recording| recording.input(bytes));
    }

    pub(crate) fn resize(&mut self, size: TerminalSize) {
        self.apply("write a resize event to", |recording| {
            recording.write(CastEvent::Resize(size))
        });
    }

    pub(crate) fn marker(&mut self, label: &str) {
        self.apply("write a marker to", |recording| {
            recording.write(CastEvent::Marker(label.to_owned()))
        });
    }

    pub(crate) fn flush_if_due(&mut self) {
        self.apply("flush", ActiveRecording::flush_if_due);
    }

    pub(crate) fn finish(&mut self, status: i32) {
        self.apply("finish", |recording| recording.finish(status));
    }
}

#[cfg(test)]
#[path = "recording/tests/recording_test.rs"]
mod tests;
