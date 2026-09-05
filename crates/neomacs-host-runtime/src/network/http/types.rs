//! Owned HTTP values: never Lisp objects or browser handles.
use std::num::NonZeroU32;

/// Instance-local identifier, never reused during the host's lifetime.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RequestId(NonZeroU32);

impl RequestId {
    /// Validate a host identifier. Zero denotes a rejected request.
    pub fn new(value: u32) -> Option<Self> {
        NonZeroU32::new(value).map(Self)
    }
    /// Encode the identifier at the host ABI.
    pub fn get(self) -> u32 {
        self.0.get()
    }
}

/// An anonymous HTTP request. Login-cookie policy is not implemented yet.
#[derive(Debug)]
pub struct Request {
    /// Absolute HTTP(S) address.
    pub url: String,
    /// HTTP method.
    pub method: String,
    /// Caller-supplied headers; browser forbidden-header rules still apply.
    pub headers: Vec<(String, String)>,
    /// Undecoded request bytes, when present.
    pub body: Option<Vec<u8>>,
}

/// Browser-decoded response, not a raw HTTP wire message.
#[derive(Debug)]
pub struct Response {
    /// Final address after redirects.
    pub url: String,
    /// HTTP status, including error statuses.
    pub status: u16,
    /// Response headers exposed by the browser.
    pub headers: Vec<(String, String)>,
    /// Bytes after browser transfer/content decoding, before character decoding.
    pub body: Vec<u8>,
}

/// A transport failure; HTTP 4xx/5xx are instead ordinary responses.
#[derive(Debug)]
pub enum Error {
    /// This runtime has not installed browser HTTP support.
    Unavailable,
    /// Request has completed, been cancelled, or was never allocated.
    UnknownRequest,
    /// Host transport rejected or failed the operation.
    Failed(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unavailable => f.write_str("browser HTTP is unavailable"),
            Self::UnknownRequest => f.write_str("unknown HTTP request"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for Error {}
