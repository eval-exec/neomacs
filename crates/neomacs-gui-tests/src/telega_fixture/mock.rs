//! The fixture's `telega-server` process: a deterministic, offline mock that
//! Telega launches through its real `telega-server-command` customization.
//!
//! The mock speaks the real wire protocol ([`super::protocol`]), emits real
//! TDLib-shaped events for the synthetic [`super::scenario`], and answers
//! every request Telega makes.  It never opens a socket, reads a personal
//! path, or touches a Telegram account/database/cache.
//!
//! Requests the fixture does not model are rejected with an explicit TL
//! `error` carrying `NEOMACS_FIXTURE_UNSUPPORTED` and are recorded in the
//! structured log, so fixture drift fails the test loudly instead of
//! pretending success.

use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::thread;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::protocol::{
    AuthorizationState, BasicGroupObject, ChatList, ChatObject, ClientFrame, FileObject,
    PhotoObject, ProtocolError, ServerEvent, ServerRequest, decode_request, read_client_frame,
    write_server_event,
};
use super::scenario::{AvatarAvailability, FixtureScenario};

/// Version reported by the `-h` probe.  Must be >= `telega-server-min-version`
/// (Telega 0.8.671 requires "0.7.7") so startup never prompts to rebuild.
pub const MOCK_SERVER_VERSION: &str = "1.0.0";

/// Deterministic remote unix time reported for `getOption :unix_time`.
pub const FIXTURE_UNIX_TIME: i64 = 1_700_000_000;

/// Environment variable Telega's process inherits with the fixture root.
pub const FIXTURE_ROOT_ENV: &str = "NEOMACS_TELEGA_FIXTURE_ROOT";

/// Environment variable selecting the scenario behavior.
pub const FIXTURE_SCENARIO_ENV: &str = "NEOMACS_TELEGA_FIXTURE_SCENARIO";

/// `-h` output, matching `telega-server`'s first line contract.
pub fn version_probe_output() -> String {
    format!(
        "Version {MOCK_SERVER_VERSION}\nusage: telega-fixture-mock [-O OPT] [-v LVL] [-l FILE] [-h]\n"
    )
}

/// One structured log line.  Tests poll this file for readiness checkpoints;
/// every line is written with a single `write` + flush so a reader never
/// observes a torn record.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct LogLine {
    /// `ready`, `request`, `event`, `control`, `unsupported`, `note`.
    pub record: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub type_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extra: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub chat_id: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl LogLine {
    fn new(record: &str) -> Self {
        Self {
            record: record.to_string(),
            stage: None,
            type_name: None,
            extra: None,
            file_id: None,
            chat_id: None,
            command: None,
            message: None,
        }
    }

    pub fn ready(stage: &str) -> Self {
        Self {
            stage: Some(stage.to_string()),
            ..Self::new("ready")
        }
    }

    pub fn request(type_name: &str, extra: Option<i64>) -> Self {
        Self {
            type_name: Some(type_name.to_string()),
            extra,
            ..Self::new("request")
        }
    }

    pub fn event(type_name: &str, extra: Option<i64>) -> Self {
        Self {
            type_name: Some(type_name.to_string()),
            extra,
            ..Self::new("event")
        }
    }

    pub fn unsupported(type_name: &str, extra: Option<i64>) -> Self {
        Self {
            type_name: Some(type_name.to_string()),
            extra,
            ..Self::new("unsupported")
        }
    }

    /// A request that is in the supported vocabulary but names an unknown
    /// option/file/group/chat id.  Unlike `unsupported` (fixture drift), this
    /// is an explicit error reply for a bad address.
    pub fn violation(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            ..Self::new("violation")
        }
    }

    pub fn note(message: &str) -> Self {
        Self {
            message: Some(message.to_string()),
            ..Self::new("note")
        }
    }
}

/// Commands the test writes to `<root>/control.jsonl` to drive explicit
/// checkpoints (file delivery, photo replacement).
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(tag = "command", rename_all = "snake_case")]
pub enum ControlCommand {
    /// Announce the downloaded avatar file for FILE_ID via `updateFile`.
    DeliverFile { file_id: i64 },
    /// Replace CHAT_ID's photo with the scenario's replacement image.
    ReplacePhoto { chat_id: i64 },
    /// Exit the mock cleanly.
    Stop,
}

/// Appends newline-delimited JSON records for the test to poll.
pub struct FixtureLog {
    file: File,
}

impl FixtureLog {
    pub fn open(path: &Path) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        Ok(Self {
            file: OpenOptions::new().create(true).append(true).open(path)?,
        })
    }

    pub fn write(&mut self, line: &LogLine) -> io::Result<()> {
        let mut encoded = serde_json::to_vec(line).map_err(io::Error::other)?;
        encoded.push(b'\n');
        self.file.write_all(&encoded)?;
        self.file.flush()
    }
}

/// The stateful fixture server.  Pure with respect to the wire: `handle`
/// returns the events to write, so unit tests drive it directly.
pub struct FixtureServer {
    pub scenario: FixtureScenario,
    log: FixtureLog,
    auth_ready: bool,
    main_loads: u32,
    /// Avatars written because Telega asked to download them.
    materialized: BTreeMap<i64, PathBuf>,
    /// Downloaded (delivered) files by id.
    delivered: BTreeMap<i64, FileObject>,
    /// Replacement photos already announced, by chat id.
    replaced: BTreeMap<i64, FileObject>,
}

impl FixtureServer {
    pub fn new(scenario: FixtureScenario, mut log: FixtureLog) -> io::Result<Self> {
        scenario.materialize_ready_avatars()?;
        log.write(&LogLine::ready("started")).ok();
        Ok(Self {
            scenario,
            log,
            auth_ready: false,
            main_loads: 0,
            materialized: BTreeMap::new(),
            delivered: BTreeMap::new(),
            replaced: BTreeMap::new(),
        })
    }

    pub fn log(&mut self) -> &mut FixtureLog {
        &mut self.log
    }

    /// The event Telega must receive before it sends `setTdlibParameters`.
    pub fn startup_events(&mut self) -> Vec<ServerEvent> {
        vec![ServerEvent::UpdateAuthorizationState {
            state: AuthorizationState::WaitTdlibParameters,
        }]
    }

    fn avatar_file(&self, file_id: i64) -> Option<FileObject> {
        let chat = self.scenario.chat_for_file(file_id)?;
        let is_replacement = chat.replacement_file_id == file_id;
        let image = if is_replacement {
            &chat.replacement
        } else {
            &chat.avatar
        };
        // A replacement photo is always handed over ready.  Initial avatars
        // follow the scenario: ready on disk, or available only once the
        // fixture has delivered the completed download.
        let ready_on_disk =
            is_replacement || self.scenario.availability == AvatarAvailability::ReadyOnDisk;
        let delivered = self.delivered.contains_key(&file_id);
        let available = delivered || (ready_on_disk && image.exists());
        Some(FileObject {
            id: file_id,
            size: (super::scenario::AVATAR_PIXELS * super::scenario::AVATAR_PIXELS * 3) as i64,
            local_path: if available {
                image.path.to_string_lossy().into_owned()
            } else {
                String::new()
            },
            local_can_be_downloaded: !available,
            is_downloading_active: self.materialized.contains_key(&file_id) && !delivered,
            is_downloading_completed: available,
        })
    }

    /// `chatPhoto` for the chat's *initial* avatar (or `None` when the chat
    /// has no photo this scenario).
    fn photo_for(&self, chat: &super::scenario::SyntheticChat) -> PhotoObject {
        let file = self
            .avatar_file(chat.file_id)
            .expect("scenario chat has an avatar file");
        PhotoObject {
            small: file.clone(),
            large: file,
        }
    }

    fn replacement_photo_for(&mut self, chat_id: i64) -> Option<PhotoObject> {
        let chat = self.scenario.chat(chat_id)?;
        let file_id = chat.replacement_file_id;
        let file = self
            .avatar_file(file_id)
            .expect("scenario chat has a replacement avatar file");
        self.replaced.insert(chat_id, file.clone());
        Some(PhotoObject {
            small: file.clone(),
            large: file,
        })
    }

    fn chat_events(&mut self) -> Vec<ServerEvent> {
        let mut events = Vec::new();
        for chat in &self.scenario.chats {
            let object = ChatObject {
                id: chat.id,
                title: chat.title.clone(),
                basic_group_id: chat.id,
                photo: Some(self.photo_for(chat)),
                positions: vec![(ChatList::Main, chat.order)],
            };
            events.push(ServerEvent::UpdateNewChat { chat: object });
            events.push(ServerEvent::ChatPosition {
                chat_id: chat.id,
                list: ChatList::Main,
                order: chat.order,
            });
        }
        events
    }

    /// Handle one decoded request; returns the events to write back.
    pub fn handle(&mut self, request: ServerRequest) -> Vec<ServerEvent> {
        self.log
            .write(&LogLine::request(request.type_name(), request.extra()))
            .ok();
        let events = self.dispatch(request);
        self.record_events(&events);
        events
    }

    /// Record the events a handler is about to put on the wire, so a test can
    /// use the log as a readiness checkpoint.
    fn record_events(&mut self, events: &[ServerEvent]) {
        for event in events {
            let (type_name, extra) = event_summary(event);
            let mut line = LogLine::event(type_name, extra);
            line.file_id = event_file_id(event);
            line.chat_id = event_chat_id(event);
            self.log.write(&line).ok();
        }
    }

    fn dispatch(&mut self, request: ServerRequest) -> Vec<ServerEvent> {
        match request {
            ServerRequest::SetTdlibParameters {
                database_directory,
                files_directory,
                extra,
            } => {
                self.log
                    .write(&LogLine::note(&format!(
                        "tdlib database={database_directory} files={files_directory}"
                    )))
                    .ok();
                self.auth_ready = true;
                // The authorization-state event is unsolicited; the call
                // itself is only answered when Telega used a correlating call.
                let mut events = vec![ServerEvent::UpdateAuthorizationState {
                    state: AuthorizationState::Ready,
                }];
                if let Some(extra) = extra {
                    events.insert(0, ServerEvent::Ok { extra: Some(extra) });
                }
                events
            }
            // Setters are `ok` calls when Telega used `telega-server--call`
            // (which always injects `:@extra`); a plain `send` has no extra
            // and expects no reply.
            ServerRequest::SetOption { extra, .. }
            | ServerRequest::SetNetworkType { extra }
            | ServerRequest::SetScopeNotificationSettings { extra } => extra
                .map_or_else(Vec::new, |extra| {
                    vec![ServerEvent::Ok { extra: Some(extra) }]
                }),
            ServerRequest::GetOption { name, extra } => {
                if name == "unix_time" {
                    vec![ServerEvent::GetOptionInteger {
                        name,
                        value: FIXTURE_UNIX_TIME,
                        extra,
                    }]
                } else {
                    self.log
                        .write(&LogLine::violation(&format!(
                            "getOption named unknown option `{name}`"
                        )))
                        .ok();
                    vec![ServerEvent::Error {
                        code: 400,
                        message: format!("NEOMACS_FIXTURE_UNKNOWN_OPTION {name}"),
                        extra,
                    }]
                }
            }
            ServerRequest::LoadChats {
                chat_list, extra, ..
            } => match chat_list {
                ChatList::Main if self.main_loads == 0 => {
                    self.main_loads += 1;
                    let mut events = self.chat_events();
                    events.push(ServerEvent::Ok { extra });
                    events
                }
                _ => {
                    // TDLib signals "all chats have been loaded" with a 404.
                    vec![ServerEvent::Error {
                        code: 404,
                        message: "Chats not found".to_string(),
                        extra,
                    }]
                }
            },
            ServerRequest::DownloadFile { file_id, extra } => {
                let Some(chat) = self.scenario.chat_for_file(file_id).cloned() else {
                    self.log
                        .write(&LogLine::violation(&format!(
                            "downloadFile named unknown file id {file_id}"
                        )))
                        .ok();
                    return vec![ServerEvent::Error {
                        code: 400,
                        message: format!("NEOMACS_FIXTURE_UNKNOWN_FILE {file_id}"),
                        extra,
                    }];
                };
                let mut current = self
                    .avatar_file(file_id)
                    .expect("known file belongs to a scenario chat");
                if current.is_downloading_completed {
                    // TDLib returns an already-available file unchanged; it
                    // never fabricates a download for bytes it already has.
                    return vec![ServerEvent::File {
                        file: current,
                        extra,
                    }];
                }
                // Delayed availability: the bytes appear only when Telega
                // actually asks for the download.
                let source = if chat.file_id == file_id {
                    &chat.avatar
                } else {
                    &chat.replacement
                };
                if let Err(error) = source.write() {
                    self.log
                        .write(&LogLine::note(&format!(
                            "failed to materialize avatar {file_id}: {error}"
                        )))
                        .ok();
                }
                self.materialized.insert(file_id, source.path.clone());
                current.is_downloading_active = true;
                current.local_path = String::new();
                vec![ServerEvent::File {
                    file: current,
                    extra,
                }]
            }
            ServerRequest::GetBlockedMessageSenders { extra } => {
                vec![ServerEvent::BlockedMessageSenders { extra }]
            }
            ServerRequest::GetSavedMessagesTags { extra } => {
                vec![ServerEvent::SavedMessagesTags { extra }]
            }
            ServerRequest::GetBasicGroup {
                basic_group_id,
                extra,
            } => {
                if self.scenario.chat(basic_group_id).is_none() {
                    self.log
                        .write(&LogLine::violation(&format!(
                            "getBasicGroup named unknown group id {basic_group_id}"
                        )))
                        .ok();
                    return vec![ServerEvent::Error {
                        code: 404,
                        message: format!("NEOMACS_FIXTURE_UNKNOWN_GROUP {basic_group_id}"),
                        extra,
                    }];
                }
                vec![ServerEvent::BasicGroup {
                    group: BasicGroupObject {
                        id: basic_group_id,
                        member_count: 3,
                        is_active: true,
                        is_creator: false,
                    },
                    extra,
                }]
            }
            ServerRequest::Unsupported { type_name, extra } => {
                self.log
                    .write(&LogLine::unsupported(&type_name, extra))
                    .ok();
                vec![ServerEvent::Error {
                    code: 400,
                    message: format!("NEOMACS_FIXTURE_UNSUPPORTED request {type_name}"),
                    extra,
                }]
            }
        }
    }

    /// Apply a control command; returns the events to write.
    pub fn apply_control(&mut self, command: ControlCommand) -> Vec<ServerEvent> {
        let events = self.dispatch_control(command);
        self.record_events(&events);
        events
    }

    fn dispatch_control(&mut self, command: ControlCommand) -> Vec<ServerEvent> {
        match command {
            ControlCommand::DeliverFile { file_id } => {
                let Some(chat) = self.scenario.chat_for_file(file_id).cloned() else {
                    self.log
                        .write(&LogLine::violation(&format!(
                            "control deliver_file named unknown file id {file_id}"
                        )))
                        .ok();
                    return vec![ServerEvent::Error {
                        code: 400,
                        message: format!("NEOMACS_FIXTURE_UNKNOWN_FILE {file_id}"),
                        extra: None,
                    }];
                };
                let image = if chat.file_id == file_id {
                    &chat.avatar
                } else {
                    &chat.replacement
                };
                if !image.exists() && image.write().is_err() {
                    return Vec::new();
                }
                let file = FileObject {
                    id: file_id,
                    size: (super::scenario::AVATAR_PIXELS * super::scenario::AVATAR_PIXELS * 3)
                        as i64,
                    local_path: image.path.to_string_lossy().into_owned(),
                    local_can_be_downloaded: false,
                    is_downloading_active: false,
                    is_downloading_completed: true,
                };
                self.delivered.insert(file_id, file.clone());
                vec![ServerEvent::UpdateFile { file }]
            }
            ControlCommand::ReplacePhoto { chat_id } => {
                let Some(chat) = self.scenario.chat(chat_id).cloned() else {
                    self.log
                        .write(&LogLine::violation(&format!(
                            "control replace_photo named unknown chat id {chat_id}"
                        )))
                        .ok();
                    return vec![ServerEvent::Error {
                        code: 404,
                        message: format!("NEOMACS_FIXTURE_UNKNOWN_CHAT {chat_id}"),
                        extra: None,
                    }];
                };
                if !chat.replacement.exists() && chat.replacement.write().is_err() {
                    return Vec::new();
                }
                let Some(photo) = self.replacement_photo_for(chat_id) else {
                    return Vec::new();
                };
                vec![ServerEvent::UpdateChatPhoto {
                    chat_id,
                    photo: Some(photo),
                }]
            }
            ControlCommand::Stop => Vec::new(),
        }
    }

    pub fn auth_ready(&self) -> bool {
        self.auth_ready
    }
}

fn event_summary(event: &ServerEvent) -> (&'static str, Option<i64>) {
    match event {
        ServerEvent::UpdateAuthorizationState { .. } => ("updateAuthorizationState", None),
        ServerEvent::GetOptionInteger { extra, .. } => ("optionValueInteger", *extra),
        ServerEvent::Ok { extra } => ("ok", *extra),
        ServerEvent::Error { extra, .. } => ("error", *extra),
        ServerEvent::UpdateFile { .. } => ("updateFile", None),
        ServerEvent::BlockedMessageSenders { extra } => ("messageSenders", *extra),
        ServerEvent::SavedMessagesTags { extra } => ("savedMessagesTags", *extra),
        ServerEvent::UpdateNewChat { .. } => ("updateNewChat", None),
        ServerEvent::ChatPosition { .. } => ("updateChatPosition", None),
        ServerEvent::UpdateChatPhoto { .. } => ("updateChatPhoto", None),
        ServerEvent::File { extra, .. } => ("file", *extra),
        ServerEvent::BasicGroup { extra, .. } => ("basicGroup", *extra),
    }
}

fn event_file_id(event: &ServerEvent) -> Option<i64> {
    match event {
        ServerEvent::UpdateFile { file } | ServerEvent::File { file, .. } => Some(file.id),
        _ => None,
    }
}

fn event_chat_id(event: &ServerEvent) -> Option<i64> {
    match event {
        ServerEvent::UpdateChatPhoto { chat_id, .. }
        | ServerEvent::ChatPosition { chat_id, .. } => Some(*chat_id),
        _ => None,
    }
}

/// Reads control commands appended to `path` since `offset`.
pub struct ControlReader {
    path: PathBuf,
    offset: u64,
}

impl ControlReader {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            offset: 0,
        }
    }

    /// Return every command appended since the previous call.
    pub fn poll(&mut self) -> io::Result<Vec<ControlCommand>> {
        let mut file = match File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let length = file.metadata()?.len();
        if length < self.offset {
            // The test truncated/rewrote the file; restart at 0.
            self.offset = 0;
        }
        if length == self.offset {
            return Ok(Vec::new());
        }
        file.seek(SeekFrom::Start(self.offset))?;
        let mut appended = String::new();
        file.read_to_string(&mut appended)?;
        self.offset = length;
        let mut commands = Vec::new();
        for line in appended.lines() {
            if line.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<ControlCommand>(line) {
                Ok(command) => commands.push(command),
                Err(error) => {
                    return Err(io::Error::other(format!(
                        "invalid fixture control line `{line}`: {error}"
                    )));
                }
            }
        }
        Ok(commands)
    }
}

/// Run the mock against real stdin/stdout until EOF or a `stop` control
/// command.  A reader thread decouples blocking stdin reads from control
/// polling so checkpoints never depend on fixed sleeps.
pub fn run_server(
    mut server: FixtureServer,
    stdout: impl Write,
    control: ControlReader,
) -> io::Result<()> {
    let mut stdout = stdout;
    // A dropped sender (clean EOF) is distinct from a protocol error, which
    // the main loop records as a fixture violation before terminating.
    let (sender, receiver) = mpsc::channel::<Result<ClientFrame, String>>();
    thread::spawn(move || {
        let stdin = io::stdin();
        let mut reader = BufReader::new(stdin.lock());
        loop {
            match read_client_frame(&mut reader) {
                Ok(Some(frame)) => {
                    if sender.send(Ok(frame)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(error) => {
                    let _ = sender.send(Err(error.to_string()));
                    break;
                }
            }
        }
    });

    for event in server.startup_events() {
        write_server_event(&mut stdout, &event).map_err(protocol_io)?;
    }
    server
        .log()
        .write(&LogLine::ready("auth_probe_sent"))
        .map_err(io::Error::other)?;

    let mut control = control;
    loop {
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(Ok(frame)) => {
                let request = match decode_request(&frame.payload) {
                    Ok(request) => request,
                    Err(error) => {
                        server
                            .log()
                            .write(&LogLine::violation(&format!(
                                "undecodable request: {error}"
                            )))
                            .ok();
                        return Err(io::Error::other(error.to_string()));
                    }
                };
                // Unmodeled requests get an explicit error event and a
                // recorded violation; the mock keeps serving so the test can
                // report every drift instead of only the first one.
                for event in server.handle(request) {
                    write_server_event(&mut stdout, &event).map_err(protocol_io)?;
                }
            }
            Ok(Err(message)) => {
                server
                    .log()
                    .write(&LogLine::violation(&format!(
                        "telega-server protocol error: {message}"
                    )))
                    .ok();
                return Err(io::Error::other(message));
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }
        let commands = match control.poll() {
            Ok(commands) => commands,
            Err(error) => {
                server
                    .log()
                    .write(&LogLine::violation(&format!(
                        "invalid control channel: {error}"
                    )))
                    .ok();
                return Err(error);
            }
        };
        for command in commands {
            let is_stop = matches!(command, ControlCommand::Stop);
            for event in server.apply_control(command) {
                write_server_event(&mut stdout, &event).map_err(protocol_io)?;
            }
            if is_stop {
                server
                    .log()
                    .write(&LogLine::ready("stopped"))
                    .map_err(io::Error::other)?;
                return Ok(());
            }
        }
    }
    server
        .log()
        .write(&LogLine::ready("eof"))
        .map_err(io::Error::other)?;
    Ok(())
}

fn protocol_io(error: ProtocolError) -> io::Error {
    io::Error::other(error.to_string())
}

/// Command-line handling: accept the flags Telega passes (`-v 0`,
/// `-O <n>`, `-l <file>`, `-z`, `-L <n>`) and answer the version probe.
pub enum Invocation {
    Probe(String),
    Serve,
    Error(String),
}

pub fn parse_invocation(args: &[String]) -> Invocation {
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "-h" => return Invocation::Probe(version_probe_output()),
            "-z" => index += 1,
            "-v" | "-O" | "-L" | "-l" => {
                if index + 1 >= args.len() {
                    return Invocation::Error(format!("missing value for {}", args[index]));
                }
                index += 2;
            }
            other => return Invocation::Error(format!("unsupported argument `{other}`")),
        }
    }
    Invocation::Serve
}

/// Root layout shared between the GUI test (writer) and the mock (reader).
#[derive(Clone, Debug)]
pub struct FixturePaths {
    pub root: PathBuf,
    pub photos: PathBuf,
    pub log: PathBuf,
    pub control: PathBuf,
}

impl FixturePaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root = root.into();
        Self {
            photos: root.join("telega-db/photos"),
            log: root.join("mock-log.jsonl"),
            control: root.join("control.jsonl"),
            root,
        }
    }

    pub fn create(&self) -> io::Result<()> {
        fs::create_dir_all(&self.photos)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::telega_fixture::protocol::print_expr;
    use crate::telega_fixture::scenario::{AVATAR_COLOR, AvatarAvailability, FixtureScenario};
    use std::path::Path;

    fn temp_root(name: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("neomacs-telega-mock-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).expect("create temp root");
        root
    }

    fn server(name: &str, availability: AvatarAvailability) -> (FixtureServer, FixturePaths) {
        let paths = FixturePaths::new(temp_root(name));
        paths.create().expect("fixture paths");
        let scenario = FixtureScenario::new(&paths.photos, availability);
        let log = FixtureLog::open(&paths.log).expect("open log");
        let mut server = FixtureServer::new(scenario, log).expect("create server");
        let startup = server.startup_events();
        assert_eq!(
            startup,
            vec![ServerEvent::UpdateAuthorizationState {
                state: AuthorizationState::WaitTdlibParameters
            }]
        );
        (server, paths)
    }

    fn read_log(paths: &FixturePaths) -> Vec<LogLine> {
        fs::read_to_string(&paths.log)
            .unwrap_or_default()
            .lines()
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect()
    }

    #[test]
    fn handshake_waits_for_set_tdlib_parameters_then_reports_ready() {
        let (mut server, paths) = server("handshake", AvatarAvailability::ReadyOnDisk);
        assert!(!server.auth_ready());
        let events = server.handle(ServerRequest::SetOption {
            name: "online".to_string(),
            extra: None,
        });
        assert!(events.is_empty(), "a plain send expects no reply");
        assert!(!server.auth_ready());

        let events = server.handle(ServerRequest::SetTdlibParameters {
            database_directory: "/fixture/db".to_string(),
            files_directory: "/fixture/files".to_string(),
            extra: None,
        });
        assert!(server.auth_ready());
        assert_eq!(
            events,
            vec![ServerEvent::UpdateAuthorizationState {
                state: AuthorizationState::Ready
            }]
        );
        let log = read_log(&paths);
        assert!(
            log.iter()
                .any(|line| line.record == "ready" && line.stage.as_deref() == Some("started"))
        );
        assert!(log.iter().any(|line| line.record == "request"
            && line.type_name.as_deref() == Some("setTdlibParameters")));
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn correlated_setter_calls_are_acknowledged_with_ok() {
        let (mut server, paths) = server("setters", AvatarAvailability::ReadyOnDisk);
        // `telega-server--call` injects `:@extra`; the fixture must answer so
        // the synchronous call returns instead of waiting for its timeout.
        for request in [
            ServerRequest::SetOption {
                name: "online".to_string(),
                extra: Some(41),
            },
            ServerRequest::SetNetworkType { extra: Some(42) },
            ServerRequest::SetScopeNotificationSettings { extra: Some(43) },
        ] {
            let extra = request.extra();
            assert_eq!(server.handle(request), vec![ServerEvent::Ok { extra }]);
        }
        // Plain sends stay unanswered.
        assert!(
            server
                .handle(ServerRequest::SetOption {
                    name: "language_pack_id".to_string(),
                    extra: None,
                })
                .is_empty()
        );

        // setTdlibParameters keeps its unsolicited authorization event and
        // additionally acknowledges a correlating call.
        let events = server.handle(ServerRequest::SetTdlibParameters {
            database_directory: "/fixture/db".to_string(),
            files_directory: "/fixture/files".to_string(),
            extra: Some(44),
        });
        assert_eq!(
            events,
            vec![
                ServerEvent::Ok { extra: Some(44) },
                ServerEvent::UpdateAuthorizationState {
                    state: AuthorizationState::Ready
                }
            ]
        );
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn unix_time_is_answered_with_a_string_valued_option_value_integer() {
        let (mut server, paths) = server("unix-time", AvatarAvailability::ReadyOnDisk);
        let events = server.handle(ServerRequest::GetOption {
            name: "unix_time".to_string(),
            extra: Some(7),
        });
        assert_eq!(
            events,
            vec![ServerEvent::GetOptionInteger {
                name: "unix_time".to_string(),
                value: FIXTURE_UNIX_TIME,
                extra: Some(7),
            }]
        );
        assert_eq!(
            print_expr(&events[0].to_lisp()),
            format!(
                r#"(:@type "optionValueInteger" :value "{}" :@extra 7)"#,
                FIXTURE_UNIX_TIME
            )
        );
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn unknown_option_file_group_and_control_ids_are_rejected_and_recorded() {
        let (mut server, paths) = server("unknown-ids", AvatarAvailability::ReadyOnDisk);

        let events = server.handle(ServerRequest::GetOption {
            name: "made_up_option".to_string(),
            extra: Some(1),
        });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 400,
                message: "NEOMACS_FIXTURE_UNKNOWN_OPTION made_up_option".to_string(),
                extra: Some(1),
            }]
        );

        let events = server.handle(ServerRequest::DownloadFile {
            file_id: 999_999,
            extra: Some(2),
        });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 400,
                message: "NEOMACS_FIXTURE_UNKNOWN_FILE 999999".to_string(),
                extra: Some(2),
            }]
        );

        let events = server.handle(ServerRequest::GetBasicGroup {
            basic_group_id: 424_242,
            extra: Some(3),
        });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 404,
                message: "NEOMACS_FIXTURE_UNKNOWN_GROUP 424242".to_string(),
                extra: Some(3),
            }]
        );

        let events = server.apply_control(ControlCommand::DeliverFile { file_id: 5_001_000 });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 400,
                message: "NEOMACS_FIXTURE_UNKNOWN_FILE 5001000".to_string(),
                extra: None,
            }]
        );
        let events = server.apply_control(ControlCommand::ReplacePhoto { chat_id: 77 });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 404,
                message: "NEOMACS_FIXTURE_UNKNOWN_CHAT 77".to_string(),
                extra: None,
            }]
        );

        let violations: Vec<_> = read_log(&paths)
            .into_iter()
            .filter(|line| line.record == "violation")
            .collect();
        assert_eq!(
            violations.len(),
            5,
            "every rejected address must be recorded: {violations:?}"
        );
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn first_main_load_returns_synthetic_chats_and_later_loads_report_exhausted() {
        let (mut server, paths) = server("chats", AvatarAvailability::ReadyOnDisk);
        let events = server.handle(ServerRequest::LoadChats {
            chat_list: ChatList::Main,
            limit: 1000,
            extra: Some(3),
        });
        let new_chats = events
            .iter()
            .filter(|event| matches!(event, ServerEvent::UpdateNewChat { .. }))
            .count();
        assert_eq!(new_chats, super::super::scenario::CHAT_COUNT);
        assert_eq!(events.last(), Some(&ServerEvent::Ok { extra: Some(3) }));
        // Every chat is in the main list, so `is-known` matches and the root
        // buffer can render it.
        assert!(events.iter().any(|event| matches!(
            event,
            ServerEvent::ChatPosition {
                list: ChatList::Main,
                ..
            }
        )));

        let events = server.handle(ServerRequest::LoadChats {
            chat_list: ChatList::Main,
            limit: 1000,
            extra: Some(4),
        });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 404,
                message: "Chats not found".to_string(),
                extra: Some(4)
            }]
        );
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn ready_avatars_are_reported_downloaded_with_a_local_path() {
        let (mut server, paths) = server("ready-avatar", AvatarAvailability::ReadyOnDisk);
        server.handle(ServerRequest::LoadChats {
            chat_list: ChatList::Main,
            limit: 1000,
            extra: Some(1),
        });
        let chat = server.scenario.chats[0].clone();
        // A file that is already available is returned unchanged, never
        // restarted as a download.
        let events = server.handle(ServerRequest::DownloadFile {
            file_id: chat.file_id,
            extra: Some(9),
        });
        let ServerEvent::File { file, extra } = &events[0] else {
            panic!("expected a file reply: {events:?}");
        };
        assert_eq!(*extra, Some(9));
        assert!(file.is_downloading_completed);
        assert!(!file.is_downloading_active);
        assert_eq!(file.local_path, chat.avatar.path.to_string_lossy());
        assert!(Path::new(&file.local_path).is_file());
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn delayed_avatar_is_not_available_until_the_download_completes() {
        let (mut server, paths) =
            server("delayed-avatar", AvatarAvailability::DelayedUntilDownload);
        let loaded = server.handle(ServerRequest::LoadChats {
            chat_list: ChatList::Main,
            limit: 1000,
            extra: Some(1),
        });
        let chat = server.scenario.chats[0].clone();
        let advertised = loaded
            .iter()
            .find_map(|event| match event {
                ServerEvent::UpdateNewChat { chat } => Some(chat),
                _ => None,
            })
            .and_then(|chat| chat.photo.as_ref())
            .expect("every synthetic chat advertises a photo");
        assert!(
            !advertised.small.is_downloading_completed && advertised.small.local_path.is_empty(),
            "a delayed avatar must start without a local path"
        );
        assert!(
            !chat.avatar.exists(),
            "delayed avatar bytes must not exist before the download request"
        );

        // Telega asks to download; the fixture materializes the bytes and
        // marks the download active, but does NOT announce completion yet.
        let started = server.handle(ServerRequest::DownloadFile {
            file_id: chat.file_id,
            extra: Some(3),
        });
        let ServerEvent::File { file: started, .. } = &started[0] else {
            panic!("expected a file reply");
        };
        assert!(started.is_downloading_active);
        assert!(!started.is_downloading_completed);
        assert!(started.local_path.is_empty());
        assert!(
            chat.avatar.exists(),
            "download request materializes the file"
        );

        let events = server.apply_control(ControlCommand::DeliverFile {
            file_id: chat.file_id,
        });
        let ServerEvent::UpdateFile { file } = &events[0] else {
            panic!("expected updateFile: {events:?}");
        };
        assert!(file.is_downloading_completed);
        assert_eq!(file.local_path, chat.avatar.path.to_string_lossy());

        // After delivery the file reports completed on any later request.
        let repeat = server.handle(ServerRequest::DownloadFile {
            file_id: chat.file_id,
            extra: Some(4),
        });
        let ServerEvent::File { file: repeat, .. } = &repeat[0] else {
            panic!("expected a file reply");
        };
        assert!(repeat.is_downloading_completed && !repeat.is_downloading_active);

        let log = read_log(&paths);
        assert!(log.iter().any(|line| line.record == "event"
            && line.type_name.as_deref() == Some("updateFile")
            && line.file_id == Some(chat.file_id)));
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn replacement_photo_is_a_new_file_and_reported_through_update_chat_photo() {
        let (mut server, paths) = server("replacement", AvatarAvailability::ReadyOnDisk);
        server.handle(ServerRequest::LoadChats {
            chat_list: ChatList::Main,
            limit: 1000,
            extra: Some(1),
        });
        let chat = server.scenario.chats[0].clone();
        assert_ne!(chat.file_id, chat.replacement_file_id);
        let events = server.apply_control(ControlCommand::ReplacePhoto { chat_id: chat.id });
        let ServerEvent::UpdateChatPhoto { chat_id, photo } = &events[0] else {
            panic!("expected updateChatPhoto: {events:?}");
        };
        assert_eq!(*chat_id, chat.id);
        let photo = photo.as_ref().expect("replacement photo");
        assert_eq!(photo.small.id, chat.replacement_file_id);
        assert!(photo.small.is_downloading_completed);
        assert_eq!(
            photo.small.local_path,
            chat.replacement.path.to_string_lossy()
        );
        assert!(chat.replacement.exists());
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn unsupported_requests_are_rejected_and_recorded_as_fixture_drift() {
        let (mut server, paths) = server("unsupported", AvatarAvailability::ReadyOnDisk);
        let events = server.handle(ServerRequest::Unsupported {
            type_name: "sendMessage".to_string(),
            extra: Some(12),
        });
        assert_eq!(
            events,
            vec![ServerEvent::Error {
                code: 400,
                message: "NEOMACS_FIXTURE_UNSUPPORTED request sendMessage".to_string(),
                extra: Some(12)
            }]
        );
        let log = read_log(&paths);
        assert!(
            log.iter().any(|line| line.record == "unsupported"
                && line.type_name.as_deref() == Some("sendMessage"))
        );
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn control_reader_returns_only_new_commands_and_surfaces_invalid_lines() {
        let paths = FixturePaths::new(temp_root("control"));
        paths.create().expect("fixture paths");
        let mut reader = ControlReader::new(&paths.control);
        assert!(reader.poll().expect("first poll").is_empty());
        fs::write(
            &paths.control,
            "{\"command\":\"deliver_file\",\"file_id\":5000}\n",
        )
        .expect("append control");
        assert_eq!(
            reader.poll().expect("second poll"),
            vec![ControlCommand::DeliverFile { file_id: 5000 }]
        );
        assert!(reader.poll().expect("third poll").is_empty());
        fs::write(&paths.control, "not json\n").expect("rewrite control");
        assert!(reader.poll().is_err());
        let _ = fs::remove_dir_all(&paths.root);
    }

    #[test]
    fn invoked_flags_match_telega_launch_and_answer_the_version_probe() {
        assert!(matches!(
            parse_invocation(&["-h".to_string()]),
            Invocation::Probe(output) if output.starts_with("Version 1.0.0\n")
        ));
        let args = ["-v", "0", "-O", "127", "-l", "/tmp/log"].map(str::to_string);
        assert!(matches!(parse_invocation(&args), Invocation::Serve));
        assert!(matches!(
            parse_invocation(&["-z".to_string()]),
            Invocation::Serve
        ));
        assert!(matches!(
            parse_invocation(&["--telega".to_string()]),
            Invocation::Error(_)
        ));
    }

    #[test]
    fn generated_avatar_bytes_match_the_scenario_color() {
        let bytes = super::super::scenario::encode_png(AVATAR_COLOR, 8);
        let decoded = image::load_from_memory(&bytes).expect("decode").to_rgb8();
        assert_eq!(decoded.get_pixel(4, 4).0, AVATAR_COLOR);
    }
}
