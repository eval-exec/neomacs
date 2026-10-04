//! The `telega-server` stdin/stdout wire protocol.
//!
//! Telega's frontend talks to its server process through a byte-length framed
//! Lisp-plist protocol (`telega-server.el`):
//!
//! ```text
//! Emacs -> server:  send <N>\n<packed plist, N bytes UTF-8>\n
//! server -> Emacs:  event <N>\n<packed plist, N bytes UTF-8>\n
//! ```
//!
//! `N` counts **bytes**, not characters: `telega-server--send` writes
//! `(string-bytes value)`.  Requests carry `:@extra`, and Telega's process
//! filter matches a reply to its callback by that value, so every reply this
//! module emits preserves the request's `:@extra`.
//!
//! This module is the fixture's half of the agreed seam.  It parses the
//! requests Telega actually sends into [`ServerRequest`] variants (with an
//! explicit [`ServerRequest::Unsupported`] for anything the fixture does not
//! model) and renders [`ServerEvent`] values back onto the wire.

use std::fmt;
use std::io::{self, BufRead, Write};

/// Largest frame the fixture accepts.  Real replies (chat batches, file
/// metadata) are far below this; the bound turns a desynchronized stream into
/// a clear error instead of an unbounded allocation.
pub const MAX_FRAME_BYTES: usize = 8 * 1024 * 1024;

/// A parsed client->server frame header.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClientFrameKind {
    Send,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ClientFrame {
    pub kind: ClientFrameKind,
    pub payload: LispValue,
}

#[derive(Debug)]
pub enum ProtocolError {
    Io(io::Error),
    /// The stream carried a command this fixture does not implement.
    UnknownCommand(String),
    /// The declared byte length exceeded [`MAX_FRAME_BYTES`].
    FrameTooLarge(usize),
    /// The payload was not a single well-formed Lisp object.
    MalformedPayload(String),
    /// The payload was not valid UTF-8.
    NotUtf8(String),
}

impl fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "telega-server protocol I/O error: {error}"),
            Self::UnknownCommand(command) => {
                write!(f, "unknown telega-server command `{command}`")
            }
            Self::FrameTooLarge(size) => {
                write!(
                    f,
                    "telega-server frame of {size} bytes exceeds the fixture limit"
                )
            }
            Self::MalformedPayload(message) => {
                write!(f, "malformed telega-server payload: {message}")
            }
            Self::NotUtf8(message) => write!(f, "telega-server payload is not UTF-8: {message}"),
        }
    }
}

impl std::error::Error for ProtocolError {}

impl From<io::Error> for ProtocolError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

/// The subset of Lisp values telega-server exchanges: plists of keywords,
/// strings, integers, symbols, `nil`/`t`, and nested lists/vectors.
#[derive(Clone, Debug, PartialEq)]
pub enum LispValue {
    Nil,
    True,
    Integer(i64),
    Float(f64),
    String(String),
    /// A bare symbol (`ok`) or a keyword (`:@type`, `:false`), stored verbatim.
    Symbol(String),
    List(Vec<LispValue>),
    Vector(Vec<LispValue>),
}

impl LispValue {
    /// `plist-get`-style lookup for `:key` entries in a plist.
    pub fn plist(&self, key: &str) -> Option<&LispValue> {
        let items = match self {
            Self::List(items) | Self::Vector(items) => items,
            _ => return None,
        };
        items
            .chunks_exact(2)
            .find_map(|pair| (pair[0] == Self::Symbol(key.to_string())).then_some(&pair[1]))
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(value) => Some(value),
            _ => None,
        }
    }

    pub fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Integer(value) => Some(*value),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::True => Some(true),
            Self::Nil => Some(false),
            _ => None,
        }
    }

    /// `:@type` as a plain Rust string, when present and a string.
    pub fn tl_type(&self) -> Option<&str> {
        self.plist(":@type").and_then(Self::as_str)
    }

    pub fn extra(&self) -> Option<i64> {
        self.plist(":@extra").and_then(Self::as_i64)
    }

    pub fn is_nil(&self) -> bool {
        matches!(self, Self::Nil)
    }
}

/// A typed view of the requests the fixture supports.
#[derive(Clone, Debug, PartialEq)]
pub enum ServerRequest {
    SetOption {
        name: String,
        extra: Option<i64>,
    },
    SetTdlibParameters {
        database_directory: String,
        files_directory: String,
        extra: Option<i64>,
    },
    SetNetworkType {
        extra: Option<i64>,
    },
    SetScopeNotificationSettings {
        extra: Option<i64>,
    },
    GetOption {
        name: String,
        extra: Option<i64>,
    },
    LoadChats {
        chat_list: ChatList,
        limit: i64,
        extra: Option<i64>,
    },
    DownloadFile {
        file_id: i64,
        extra: Option<i64>,
    },
    GetBlockedMessageSenders {
        extra: Option<i64>,
    },
    GetSavedMessagesTags {
        extra: Option<i64>,
    },
    GetBasicGroup {
        basic_group_id: i64,
        extra: Option<i64>,
    },
    /// A syntactically valid request the fixture does not model.  The mock
    /// rejects it explicitly instead of pretending success.
    Unsupported {
        type_name: String,
        extra: Option<i64>,
    },
}

impl ServerRequest {
    pub fn extra(&self) -> Option<i64> {
        match self {
            Self::SetOption { extra, .. }
            | Self::SetTdlibParameters { extra, .. }
            | Self::SetNetworkType { extra }
            | Self::SetScopeNotificationSettings { extra }
            | Self::GetOption { extra, .. }
            | Self::LoadChats { extra, .. }
            | Self::DownloadFile { extra, .. }
            | Self::GetBlockedMessageSenders { extra }
            | Self::GetSavedMessagesTags { extra }
            | Self::GetBasicGroup { extra, .. }
            | Self::Unsupported { extra, .. } => *extra,
        }
    }

    pub fn type_name(&self) -> &str {
        match self {
            Self::SetOption { .. } => "setOption",
            Self::SetTdlibParameters { .. } => "setTdlibParameters",
            Self::SetNetworkType { .. } => "setNetworkType",
            Self::SetScopeNotificationSettings { .. } => "setScopeNotificationSettings",
            Self::GetOption { .. } => "getOption",
            Self::LoadChats { .. } => "loadChats",
            Self::DownloadFile { .. } => "downloadFile",
            Self::GetBlockedMessageSenders { .. } => "getBlockedMessageSenders",
            Self::GetSavedMessagesTags { .. } => "getSavedMessagesTags",
            Self::GetBasicGroup { .. } => "getBasicGroup",
            Self::Unsupported { type_name, .. } => type_name,
        }
    }
}

/// TDLib's two top-level chat lists, as Telega addresses them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ChatList {
    Main,
    Archive,
}

impl ChatList {
    pub fn tl_type(self) -> &'static str {
        match self {
            Self::Main => "chatListMain",
            Self::Archive => "chatListArchive",
        }
    }

    fn from_tl_type(name: &str) -> Option<Self> {
        match name {
            "chatListMain" => Some(Self::Main),
            "chatListArchive" => Some(Self::Archive),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthorizationState {
    WaitTdlibParameters,
    Ready,
}

impl AuthorizationState {
    pub fn tl_type(self) -> &'static str {
        match self {
            Self::WaitTdlibParameters => "authorizationStateWaitTdlibParameters",
            Self::Ready => "authorizationStateReady",
        }
    }
}

/// A TDLib `file`, restricted to what the fixture needs to drive Telega's
/// download pipeline.
#[derive(Clone, Debug, PartialEq)]
pub struct FileObject {
    pub id: i64,
    pub size: i64,
    pub local_path: String,
    pub local_can_be_downloaded: bool,
    pub is_downloading_active: bool,
    pub is_downloading_completed: bool,
}

impl FileObject {
    pub fn to_lisp(&self) -> LispValue {
        Plist::new("file")
            .field(":id", LispValue::Integer(self.id))
            .field(":size", LispValue::Integer(self.size))
            .field(":expected_size", LispValue::Integer(self.size))
            .field(
                ":local",
                Plist::new("localFile")
                    .field(":path", LispValue::String(self.local_path.clone()))
                    .field(
                        ":can_be_downloaded",
                        bool_value(self.local_can_be_downloaded),
                    )
                    .field(
                        ":is_downloading_active",
                        bool_value(self.is_downloading_active),
                    )
                    .field(
                        ":is_downloading_completed",
                        bool_value(self.is_downloading_completed),
                    )
                    .build(),
            )
            .field(
                ":remote",
                Plist::new("remoteFile")
                    .field(":id", LispValue::String(format!("remote-{}", self.id)))
                    .field(
                        ":unique_id",
                        LispValue::String(format!("unique-{}", self.id)),
                    )
                    .field(":is_uploading_active", LispValue::Nil)
                    .field(":is_uploading_completed", LispValue::True)
                    .build(),
            )
            .build()
    }
}

/// Builds a flat Lisp plist (`(:key value :key value ...)`) — the shape the
/// C `telega-server` prints and `plist-get` consumes.
pub struct Plist {
    items: Vec<LispValue>,
}

impl Plist {
    pub fn new(type_name: &str) -> Self {
        Self {
            items: vec![
                LispValue::Symbol(":@type".to_string()),
                LispValue::String(type_name.to_string()),
            ],
        }
    }

    pub fn field(mut self, key: &str, value: LispValue) -> Self {
        self.items.push(LispValue::Symbol(key.to_string()));
        self.items.push(value);
        self
    }

    pub fn optional_extra(mut self, extra: Option<i64>) -> Self {
        if let Some(extra) = extra {
            self.items.push(LispValue::Symbol(":@extra".to_string()));
            self.items.push(LispValue::Integer(extra));
        }
        self
    }

    pub fn build(self) -> LispValue {
        LispValue::List(self.items)
    }
}

fn bool_value(value: bool) -> LispValue {
    if value {
        LispValue::True
    } else {
        LispValue::Nil
    }
}

/// A typed `chat` object for `updateNewChat`/`getChat`.
#[derive(Clone, Debug, PartialEq)]
pub struct ChatObject {
    pub id: i64,
    pub title: String,
    pub basic_group_id: i64,
    pub photo: Option<PhotoObject>,
    pub positions: Vec<(ChatList, i64)>,
}

impl ChatObject {
    pub fn to_lisp(&self) -> LispValue {
        Plist::new("chat")
            .field(":id", LispValue::Integer(self.id))
            .field(":title", LispValue::String(self.title.clone()))
            .field(
                ":type",
                Plist::new("chatTypeBasicGroup")
                    .field(":basic_group_id", LispValue::Integer(self.basic_group_id))
                    .build(),
            )
            .field(
                ":photo",
                match &self.photo {
                    Some(photo) => photo.to_lisp(),
                    None => LispValue::Nil,
                },
            )
            .field(":permissions", Plist::new("chatPermissions").build())
            .field(":unread_count", LispValue::Integer(0))
            .field(
                ":positions",
                LispValue::Vector(
                    self.positions
                        .iter()
                        .map(|(list, order)| {
                            Plist::new("chatPosition")
                                .field(":list", Plist::new(list.tl_type()).build())
                                .field(":order", LispValue::String(order.to_string()))
                                .field(":is_pinned", LispValue::Nil)
                                .build()
                        })
                        .collect(),
                ),
            )
            .field(":last_message", LispValue::Nil)
            .field(":has_protected_content", LispValue::Nil)
            .build()
    }
}

/// A typed `chatPhoto` with its two photo sizes.
#[derive(Clone, Debug, PartialEq)]
pub struct PhotoObject {
    pub small: FileObject,
    pub large: FileObject,
}

impl PhotoObject {
    pub fn to_lisp(&self) -> LispValue {
        Plist::new("chatPhoto")
            .field(":small", self.small.to_lisp())
            .field(":big", self.large.to_lisp())
            .build()
    }
}

/// Events the fixture sends to Telega.  Every variant carries the `:@extra`
/// of the request it answers (or `None` for unsolicited updates).
#[derive(Clone, Debug, PartialEq)]
pub enum ServerEvent {
    /// Unsolicited connection state; TDLib sends these without a request.
    UpdateAuthorizationState {
        state: AuthorizationState,
    },
    /// The TDLib `optionValueInteger` result of `getOption` (int64 rendered
    /// as a string, exactly like TDLib's JSON).
    GetOptionInteger {
        name: String,
        value: i64,
        extra: Option<i64>,
    },
    /// `ok` — the generic successful reply to a TDLib call.
    Ok {
        extra: Option<i64>,
    },
    Error {
        code: i64,
        message: String,
        extra: Option<i64>,
    },
    UpdateFile {
        file: FileObject,
    },
    BlockedMessageSenders {
        extra: Option<i64>,
    },
    SavedMessagesTags {
        extra: Option<i64>,
    },
    /// A full `chat` object (`updateNewChat`).
    UpdateNewChat {
        chat: ChatObject,
    },
    /// One entry of a chat's `:positions` (`updateChatPosition`).
    ChatPosition {
        chat_id: i64,
        list: ChatList,
        order: i64,
    },
    /// The chat's photo changed (`updateChatPhoto`); `None` removes it.
    UpdateChatPhoto {
        chat_id: i64,
        photo: Option<PhotoObject>,
    },
    /// A `file` object reply to `downloadFile`.
    File {
        file: FileObject,
        extra: Option<i64>,
    },
    /// A `basicGroup` object reply to `getBasicGroup`.
    BasicGroup {
        group: BasicGroupObject,
        extra: Option<i64>,
    },
}

/// A typed `basicGroup` TDLib object.
#[derive(Clone, Debug, PartialEq)]
pub struct BasicGroupObject {
    pub id: i64,
    pub member_count: i64,
    pub is_active: bool,
    /// `true` when the fixture's own account is the group creator.
    pub is_creator: bool,
}

impl BasicGroupObject {
    pub fn to_lisp(&self) -> LispValue {
        Plist::new("basicGroup")
            .field(":id", LispValue::Integer(self.id))
            .field(":member_count", LispValue::Integer(self.member_count))
            .field(
                ":status",
                Plist::new(if self.is_creator {
                    "chatMemberStatusCreator"
                } else {
                    "chatMemberStatusMember"
                })
                .build(),
            )
            .field(":is_active", bool_value(self.is_active))
            .field(":upgraded_to_supergroup_id", LispValue::Integer(0))
            .field(":updated_version", LispValue::String("1".to_string()))
            .build()
    }
}

impl ServerEvent {
    pub fn to_lisp(&self) -> LispValue {
        match self {
            Self::UpdateAuthorizationState { state } => Plist::new("updateAuthorizationState")
                .field(":authorization_state", Plist::new(state.tl_type()).build())
                .build(),
            // TDLib serializes int64 option values as strings; the idle-time
            // callback reads `:value' through `string-to-number', and the
            // reply is a direct `getOption' result (no `:name' field).
            Self::GetOptionInteger {
                name: _,
                value,
                extra,
            } => Plist::new("optionValueInteger")
                .field(":value", LispValue::String(value.to_string()))
                .optional_extra(*extra)
                .build(),
            Self::Ok { extra } => Plist::new("ok").optional_extra(*extra).build(),
            Self::Error {
                code,
                message,
                extra,
            } => Plist::new("error")
                .field(":code", LispValue::Integer(*code))
                .field(":message", LispValue::String(message.clone()))
                .optional_extra(*extra)
                .build(),
            Self::UpdateFile { file } => Plist::new("updateFile")
                .field(":file", file.to_lisp())
                .build(),
            Self::BlockedMessageSenders { extra } => Plist::new("messageSenders")
                .field(":total_count", LispValue::Integer(0))
                .field(":senders", LispValue::Vector(Vec::new()))
                .optional_extra(*extra)
                .build(),
            Self::SavedMessagesTags { extra } => Plist::new("savedMessagesTags")
                .field(":tags", LispValue::Vector(Vec::new()))
                .optional_extra(*extra)
                .build(),
            Self::UpdateNewChat { chat } => Plist::new("updateNewChat")
                .field(":chat", chat.to_lisp())
                .build(),
            Self::ChatPosition {
                chat_id,
                list,
                order,
            } => Plist::new("updateChatPosition")
                .field(":chat_id", LispValue::Integer(*chat_id))
                .field(
                    ":position",
                    Plist::new("chatPosition")
                        .field(":list", Plist::new(list.tl_type()).build())
                        .field(":order", LispValue::String(order.to_string()))
                        .field(":is_pinned", LispValue::Nil)
                        .build(),
                )
                .build(),
            Self::UpdateChatPhoto { chat_id, photo } => Plist::new("updateChatPhoto")
                .field(":chat_id", LispValue::Integer(*chat_id))
                .field(
                    ":photo",
                    match photo {
                        Some(photo) => photo.to_lisp(),
                        None => LispValue::Nil,
                    },
                )
                .build(),
            Self::File { file, extra } => {
                let mut items = file.to_lisp();
                if let LispValue::List(items) = &mut items
                    && let Some(extra) = extra
                {
                    items.push(LispValue::Symbol(":@extra".to_string()));
                    items.push(LispValue::Integer(*extra));
                }
                items
            }
            Self::BasicGroup { group, extra } => {
                let mut items = group.to_lisp();
                if let LispValue::List(items) = &mut items
                    && let Some(extra) = extra
                {
                    items.push(LispValue::Symbol(":@extra".to_string()));
                    items.push(LispValue::Integer(*extra));
                }
                items
            }
        }
    }
}

/// Read one framed request from Telega.
///
/// Returns `Ok(None)` at a clean end of stream (Telega closed the pipe).
pub fn read_client_frame(reader: &mut impl BufRead) -> Result<Option<ClientFrame>, ProtocolError> {
    let mut header = String::new();
    let read = reader.read_line(&mut header)?;
    if read == 0 {
        return Ok(None);
    }
    let header = header.trim_end_matches(['\n', '\r']);
    let (command, size) = header
        .split_once(' ')
        .ok_or_else(|| ProtocolError::MalformedPayload(format!("invalid header `{header}`")))?;
    let size: usize = size
        .parse()
        .map_err(|_| ProtocolError::MalformedPayload(format!("invalid frame size `{size}`")))?;
    if size > MAX_FRAME_BYTES {
        return Err(ProtocolError::FrameTooLarge(size));
    }
    let kind = match command {
        "send" => ClientFrameKind::Send,
        other => return Err(ProtocolError::UnknownCommand(other.to_string())),
    };
    let mut bytes = vec![0_u8; size];
    reader.read_exact(&mut bytes)?;
    let mut newline = [0_u8; 1];
    reader.read_exact(&mut newline)?;
    if newline[0] != b'\n' {
        return Err(ProtocolError::MalformedPayload(
            "frame payload is not terminated by a newline".to_string(),
        ));
    }
    let text =
        String::from_utf8(bytes).map_err(|error| ProtocolError::NotUtf8(error.to_string()))?;
    let payload = parse_expr(&text)?;
    Ok(Some(ClientFrame { kind, payload }))
}

/// Write one server event as a `event <byte-len>\n<payload>\n` frame.
pub fn write_server_event(
    writer: &mut impl Write,
    event: &ServerEvent,
) -> Result<(), ProtocolError> {
    write_server_object(writer, &event.to_lisp())
}

/// Write a raw Lisp object as a server frame (test/helper seam).
pub fn write_server_object(
    writer: &mut impl Write,
    object: &LispValue,
) -> Result<(), ProtocolError> {
    let text = print_expr(object);
    write!(writer, "event {}\n{}\n", text.len(), text)?;
    writer.flush()?;
    Ok(())
}

/// Parse one Lisp expression (the payload text, without the frame header).
pub fn parse_expr(text: &str) -> Result<LispValue, ProtocolError> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        position: 0,
    };
    let value = parser.parse_value()?;
    parser.skip_whitespace();
    if parser.position != parser.bytes.len() {
        return Err(ProtocolError::MalformedPayload(format!(
            "trailing input after expression at byte {}",
            parser.position
        )));
    }
    Ok(value)
}

/// Render one Lisp expression the way the C `telega-server` prints plists.
pub fn print_expr(value: &LispValue) -> String {
    let mut output = String::new();
    print_into(value, &mut output);
    output
}

fn print_into(value: &LispValue, output: &mut String) {
    match value {
        LispValue::Nil => output.push_str("nil"),
        LispValue::True => output.push('t'),
        LispValue::Integer(number) => output.push_str(&number.to_string()),
        LispValue::Float(number) => output.push_str(&format!("{number}")),
        LispValue::Symbol(symbol) => output.push_str(symbol),
        LispValue::String(text) => {
            output.push('"');
            for character in text.chars() {
                match character {
                    '"' => output.push_str("\\\""),
                    '\\' => output.push_str("\\\\"),
                    '\n' => output.push_str("\\n"),
                    '\t' => output.push_str("\\t"),
                    '\r' => output.push_str("\\r"),
                    other => output.push(other),
                }
            }
            output.push('"');
        }
        LispValue::List(items) => {
            output.push('(');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                print_into(item, output);
            }
            output.push(')');
        }
        LispValue::Vector(items) => {
            output.push('[');
            for (index, item) in items.iter().enumerate() {
                if index > 0 {
                    output.push(' ');
                }
                print_into(item, output);
            }
            output.push(']');
        }
    }
}

struct Parser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl<'a> Parser<'a> {
    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.position)
            .is_some_and(u8::is_ascii_whitespace)
        {
            self.position += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.position).copied()
    }

    fn parse_value(&mut self) -> Result<LispValue, ProtocolError> {
        self.skip_whitespace();
        match self.peek() {
            None => Err(ProtocolError::MalformedPayload(
                "unexpected end of payload".to_string(),
            )),
            Some(b'(') | Some(b'[') => self.parse_sequence(),
            Some(b'"') => self.parse_string(),
            Some(_) => self.parse_atom(),
        }
    }

    fn parse_sequence(&mut self) -> Result<LispValue, ProtocolError> {
        let open = self.peek().expect("caller checked the open delimiter");
        let close = if open == b'(' { b')' } else { b']' };
        self.position += 1;
        let mut items = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(byte) if byte == close => {
                    self.position += 1;
                    return Ok(if open == b'(' {
                        LispValue::List(items)
                    } else {
                        LispValue::Vector(items)
                    });
                }
                None => {
                    return Err(ProtocolError::MalformedPayload(
                        "unterminated sequence".to_string(),
                    ));
                }
                Some(_) => items.push(self.parse_value()?),
            }
        }
    }

    fn parse_string(&mut self) -> Result<LispValue, ProtocolError> {
        self.position += 1; // opening quote
        let mut text = String::new();
        loop {
            let byte = self.peek().ok_or_else(|| {
                ProtocolError::MalformedPayload("unterminated string".to_string())
            })?;
            self.position += 1;
            match byte {
                b'"' => return Ok(LispValue::String(text)),
                b'\\' => {
                    let escaped = self.peek().ok_or_else(|| {
                        ProtocolError::MalformedPayload("dangling string escape".to_string())
                    })?;
                    self.position += 1;
                    match escaped {
                        b'"' => text.push('"'),
                        b'\\' => text.push('\\'),
                        b'n' => text.push('\n'),
                        b't' => text.push('\t'),
                        b'r' => text.push('\r'),
                        other => {
                            return Err(ProtocolError::MalformedPayload(format!(
                                "unsupported string escape \\\\{}",
                                other as char
                            )));
                        }
                    }
                }
                _ => {
                    // Copy the full UTF-8 sequence for this byte.
                    let start = self.position - 1;
                    let width = utf8_width(byte);
                    let end = start + width;
                    let slice = self.bytes.get(start..end).ok_or_else(|| {
                        ProtocolError::MalformedPayload("truncated UTF-8 string".to_string())
                    })?;
                    let character = std::str::from_utf8(slice)
                        .map_err(|error| ProtocolError::NotUtf8(error.to_string()))?;
                    text.push_str(character);
                    self.position = end;
                }
            }
        }
    }

    fn parse_atom(&mut self) -> Result<LispValue, ProtocolError> {
        let start = self.position;
        while self.peek().is_some_and(|byte| {
            !byte.is_ascii_whitespace() && !matches!(byte, b'(' | b')' | b'[' | b']' | b'"')
        }) {
            self.position += 1;
        }
        let atom = std::str::from_utf8(&self.bytes[start..self.position])
            .map_err(|error| ProtocolError::NotUtf8(error.to_string()))?;
        if atom.is_empty() {
            return Err(ProtocolError::MalformedPayload(
                "empty atom in payload".to_string(),
            ));
        }
        if atom == "nil" {
            return Ok(LispValue::Nil);
        }
        if atom == "t" {
            return Ok(LispValue::True);
        }
        if let Ok(integer) = atom.parse::<i64>() {
            return Ok(LispValue::Integer(integer));
        }
        if let Ok(float) = atom.parse::<f64>()
            && (atom.contains('.') || atom.contains('e') || atom.contains('E'))
        {
            return Ok(LispValue::Float(float));
        }
        Ok(LispValue::Symbol(atom.to_string()))
    }
}

fn utf8_width(first: u8) -> usize {
    match first {
        0x00..=0x7f => 1,
        0xc0..=0xdf => 2,
        0xe0..=0xef => 3,
        _ => 4,
    }
}

/// Decode a parsed payload into a typed request.
pub fn decode_request(payload: &LispValue) -> Result<ServerRequest, ProtocolError> {
    let extra = payload.extra();
    let type_name = payload.tl_type().ok_or_else(|| {
        ProtocolError::MalformedPayload("request has no :@type string".to_string())
    })?;
    let string_field = |key: &str| -> Result<String, ProtocolError> {
        payload
            .plist(key)
            .and_then(LispValue::as_str)
            .map(str::to_string)
            .ok_or_else(|| {
                ProtocolError::MalformedPayload(format!("request {type_name} lacks {key}"))
            })
    };
    let integer_field = |key: &str| -> Result<i64, ProtocolError> {
        payload
            .plist(key)
            .and_then(LispValue::as_i64)
            .ok_or_else(|| {
                ProtocolError::MalformedPayload(format!("request {type_name} lacks {key}"))
            })
    };
    let chat_list = || -> Result<ChatList, ProtocolError> {
        payload
            .plist(":chat_list")
            .and_then(LispValue::tl_type)
            .and_then(ChatList::from_tl_type)
            .ok_or_else(|| {
                ProtocolError::MalformedPayload(format!(
                    "request {type_name} has an unsupported :chat_list"
                ))
            })
    };
    Ok(match type_name {
        "setOption" => ServerRequest::SetOption {
            name: string_field(":name")?,
            extra,
        },
        "setTdlibParameters" => ServerRequest::SetTdlibParameters {
            database_directory: string_field(":database_directory")?,
            files_directory: string_field(":files_directory")?,
            extra,
        },
        "setNetworkType" => ServerRequest::SetNetworkType { extra },
        "setScopeNotificationSettings" => ServerRequest::SetScopeNotificationSettings { extra },
        "getOption" => ServerRequest::GetOption {
            name: string_field(":name")?,
            extra,
        },
        "loadChats" => ServerRequest::LoadChats {
            chat_list: chat_list()?,
            limit: integer_field(":limit").unwrap_or(0),
            extra,
        },
        "downloadFile" => ServerRequest::DownloadFile {
            file_id: integer_field(":file_id")?,
            extra,
        },
        "getBlockedMessageSenders" => ServerRequest::GetBlockedMessageSenders { extra },
        "getSavedMessagesTags" => ServerRequest::GetSavedMessagesTags { extra },
        "getBasicGroup" => ServerRequest::GetBasicGroup {
            basic_group_id: integer_field(":basic_group_id")?,
            extra,
        },
        other => ServerRequest::Unsupported {
            type_name: other.to_string(),
            extra,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn framed(payload: &str) -> Vec<u8> {
        format!("send {}\n{}\n", payload.len(), payload).into_bytes()
    }

    #[test]
    fn reads_frames_by_utf8_byte_length_and_stays_in_sync() {
        // The Cyrillic title makes char count < byte count, so a reader that
        // framed by characters would desynchronize on the second frame.
        let first = r#"(:@type "getOption" :name "language_pack_id" :title "Привет" :@extra 7)"#;
        let second =
            r#"(:@type "loadChats" :chat_list (:@type "chatListMain") :limit 1000 :@extra 8)"#;
        let mut stream = framed(first);
        stream.extend(framed(second));
        assert_ne!(
            first.chars().count(),
            first.len(),
            "test payload must contain multibyte characters"
        );

        let mut reader = Cursor::new(stream);
        let frame = read_client_frame(&mut reader)
            .expect("first frame parses")
            .expect("stream has a first frame");
        assert_eq!(frame.kind, ClientFrameKind::Send);
        assert_eq!(frame.payload.tl_type(), Some("getOption"));
        assert_eq!(
            frame.payload.plist(":title").and_then(LispValue::as_str),
            Some("Привет")
        );
        assert_eq!(frame.payload.extra(), Some(7));

        let frame = read_client_frame(&mut reader)
            .expect("second frame parses")
            .expect("stream has a second frame");
        assert_eq!(frame.payload.tl_type(), Some("loadChats"));
        assert_eq!(frame.payload.extra(), Some(8));
        assert!(read_client_frame(&mut reader).expect("clean end").is_none());
    }

    #[test]
    fn round_trips_an_event_frame_with_multibyte_paths() {
        let event = ServerEvent::UpdateFile {
            file: FileObject {
                id: 42,
                size: 512,
                local_path: "/tmp/фикстура/аватар-42.png".to_string(),
                local_can_be_downloaded: false,
                is_downloading_active: false,
                is_downloading_completed: true,
            },
        };
        let mut bytes = Vec::new();
        write_server_event(&mut bytes, &event).expect("frame writes");
        let text = String::from_utf8(bytes).expect("frame is UTF-8");
        let (header, payload) = text.split_once('\n').expect("header line");
        let declared: usize = header
            .strip_prefix("event ")
            .expect("event header")
            .parse()
            .expect("byte length");
        let payload = payload.strip_suffix('\n').expect("trailing newline");
        assert_eq!(declared, payload.len());
        assert_ne!(declared, payload.chars().count());

        let parsed = parse_expr(payload).expect("payload parses");
        assert_eq!(parsed.tl_type(), Some("updateFile"));
        let local = parsed
            .plist(":file")
            .and_then(|file| file.plist(":local"))
            .expect("file has :local");
        assert_eq!(
            local.plist(":path").and_then(LispValue::as_str),
            Some("/tmp/фикстура/аватар-42.png")
        );
        assert_eq!(
            local
                .plist(":is_downloading_completed")
                .and_then(LispValue::as_bool),
            Some(true)
        );
    }

    #[test]
    fn decodes_supported_requests_and_rejects_unknown_ones_explicitly() {
        let cases = [
            (
                r#"(:@type "setOption" :name "online" :value t :@extra 1)"#,
                ServerRequest::SetOption {
                    name: "online".to_string(),
                    extra: Some(1),
                },
            ),
            (
                r#"(:@type "loadChats" :chat_list (:@type "chatListMain") :limit 1000 :@extra 2)"#,
                ServerRequest::LoadChats {
                    chat_list: ChatList::Main,
                    limit: 1000,
                    extra: Some(2),
                },
            ),
            (
                r#"(:@type "downloadFile" :file_id 9 :priority 1 :offset 0 :limit 0 :synchronous t :@extra 3)"#,
                ServerRequest::DownloadFile {
                    file_id: 9,
                    extra: Some(3),
                },
            ),
            (
                r#"(:@type "getOption" :name "unix_time" :@extra 4)"#,
                ServerRequest::GetOption {
                    name: "unix_time".to_string(),
                    extra: Some(4),
                },
            ),
        ];
        for (text, expected) in cases {
            let payload = parse_expr(text).expect("payload parses");
            assert_eq!(decode_request(&payload).expect("decodes"), expected);
        }

        let unknown =
            parse_expr(r#"(:@type "sendMessage" :chat_id 1 :@extra 11)"#).expect("payload parses");
        assert_eq!(
            decode_request(&unknown).expect("decodes as unsupported"),
            ServerRequest::Unsupported {
                type_name: "sendMessage".to_string(),
                extra: Some(11),
            }
        );
    }

    #[test]
    fn error_replies_keep_the_correlation_extra() {
        let event = ServerEvent::Error {
            code: 404,
            message: "Chats not found".to_string(),
            extra: Some(35),
        };
        let wire = print_expr(&event.to_lisp());
        assert!(
            wire.starts_with(r#"(:@type "error" :code 404 :message "Chats not found""#),
            "{wire}"
        );
        assert!(wire.ends_with(":@extra 35)"), "{wire}");
        // The wire form is a plist the fixture can read back, not a list of
        // key/value pairs.
        let parsed = parse_expr(&wire).expect("printed event parses");
        assert_eq!(parsed.tl_type(), Some("error"));
        assert_eq!(parsed.extra(), Some(35));
    }

    #[test]
    fn unix_time_reply_is_a_correlated_tdlib_option_value_integer() {
        // TDLib answers `getOption' with `optionValueInteger' whose int64
        // `:value' is a *string*; Telega's idle callback reads it through
        // `string-to-number'.  This is not an `updateOption' event.
        let event = ServerEvent::GetOptionInteger {
            name: "unix_time".to_string(),
            value: 1_700_000_000,
            extra: Some(21),
        };
        let wire = print_expr(&event.to_lisp());
        assert_eq!(
            wire,
            r#"(:@type "optionValueInteger" :value "1700000000" :@extra 21)"#
        );
        let parsed = parse_expr(&wire).expect("option reply parses");
        assert_eq!(parsed.tl_type(), Some("optionValueInteger"));
        assert_eq!(
            parsed.plist(":value").and_then(LispValue::as_str),
            Some("1700000000")
        );
        assert_eq!(parsed.extra(), Some(21));
        assert!(
            parsed.plist(":name").is_none(),
            "a getOption result carries only :value: {wire}"
        );
    }

    #[test]
    fn unsupported_and_malformed_payloads_are_rejected_with_context() {
        assert!(matches!(
            parse_expr("(:\"unterminated"),
            Err(ProtocolError::MalformedPayload(_))
        ));
        assert!(matches!(
            parse_expr("(:@type \"ok\") trailing"),
            Err(ProtocolError::MalformedPayload(_))
        ));
        assert!(matches!(
            parse_expr("(:@type \"ok\" \"odd\""),
            Err(ProtocolError::MalformedPayload(_))
        ));
        // A non-UTF-8 byte sequence in a string is a clear error, not a panic.
        let mut payload = b"(:name \"".to_vec();
        payload.extend_from_slice(&[0xff, 0xfe, 0xfd]);
        payload.push(b'"');
        payload.push(b')');
        let mut bytes = format!("send {}\n", payload.len()).into_bytes();
        bytes.extend_from_slice(&payload);
        bytes.push(b'\n');
        let mut reader = Cursor::new(bytes);
        assert!(matches!(
            read_client_frame(&mut reader),
            Err(ProtocolError::NotUtf8(_))
        ));
    }
}
