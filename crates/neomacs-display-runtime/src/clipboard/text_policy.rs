//! The one clipboard text policy: MIME preference, decoding, and absence
//! classification.
//!
//! Three backends can answer a text read — smithay-clipboard's `wl_data_device`
//! path, the `ext-data-control-v1` fallback, and arboard (X11 and the other
//! platforms) — and they must all answer it the same way.  This module owns
//! the parts of that answer we control:
//!
//! * which MIME spellings count as text and in what order (`TextMime::choose`,
//!   matching smithay-clipboard's `mime.rs::find_allowed`);
//! * how transferred bytes become a `String` (`TextMime::decode`, matching
//!   smithay-clipboard's `state.rs` post-processing);
//! * which native errors mean "nothing to paste" rather than failure
//!   (`classify_smithay`, `classify_arboard`);
//! * when the data-control fallback may answer for smithay
//!   (`text_or_fallback`).
//!
//! # What this module does not do
//!
//! It does not decode for smithay-clipboard or arboard.  Both pick and decode a
//! representation internally and hand back a `String` (arboard even decodes
//! X11's Latin-1 `STRING` atom), and smithay-clipboard never exposes the
//! offer's MIME list.  Their choice is dependency behaviour we cannot
//! intercept, so the policy classifies their outcomes instead.  The raw-bytes
//! decode path (the data-control reader) is the one this module fully owns; a
//! backend that receives bytes routes them through `choose`/`decode` here
//! rather than growing a fourth copy.
//!
//! Only the smithay-clipboard rows are Linux-gated.  `TextRead` and
//! `classify_arboard` serve every platform, because arboard is the X11, macOS,
//! and Windows backend alike and none of them needs a different text policy
//! today.

#[cfg(target_os = "linux")]
use crate::thread_comm::ClipboardSelection;

/// Why a text read produced no text: a recognized absence, not a failure.
///
/// The classifiers answer `Some` for these and `None` for a real error, so the
/// name matches the side of the `Option` it appears on.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextAbsence {
    /// No selection owner.
    NoSelection,
    /// An owner exists, but it offers none of the text MIMEs this backend reads.
    ///
    /// Only a Wayland backend can tell this apart from `NoSelection`, so the
    /// variant exists only on a platform that has one.
    #[cfg(target_os = "linux")]
    TargetUnavailable,
    /// The backend cannot tell those two apart: it reports one error for both
    /// an empty clipboard and contents in a format it cannot read (arboard's
    /// `ContentNotAvailable`).  Guessing either way would state a fact the
    /// backend does not know.
    Indeterminate,
}

impl From<TextAbsence> for TextRead {
    fn from(absence: TextAbsence) -> Self {
        match absence {
            TextAbsence::NoSelection => Self::NoSelection,
            #[cfg(target_os = "linux")]
            TextAbsence::TargetUnavailable => Self::TargetUnavailable,
            TextAbsence::Indeterminate => Self::Indeterminate,
        }
    }
}

/// What a best-available text read produced.
#[derive(Debug, Eq, PartialEq)]
pub(super) enum TextRead {
    /// An owner published text (possibly the empty string).
    Text(String),
    /// No selection owner.
    NoSelection,
    /// An owner exists, but it offers none of the text MIMEs we read; see
    /// `TextAbsence::TargetUnavailable` for why this is Linux-only.
    #[cfg(target_os = "linux")]
    TargetUnavailable,
    /// The backend cannot tell "no owner" from "unreadable contents"; see
    /// `TextAbsence::Indeterminate`.
    Indeterminate,
}

impl TextRead {
    /// The answer the evaluator-facing wire carries: text, or `None` for any
    /// absence.  The absence reasons stay distinct inside the runtime so
    /// callers can tell "nothing is selected" from "something is selected that
    /// we cannot read", and can see when the backend does not know which.
    pub(super) fn into_option(self) -> Option<String> {
        match self {
            Self::Text(text) => Some(text),
            Self::NoSelection | Self::Indeterminate => None,
            #[cfg(target_os = "linux")]
            Self::TargetUnavailable => None,
        }
    }
}

/// The text MIME spellings, in smithay-clipboard's preference order.
#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum TextMime {
    TextPlainUtf8,
    Utf8String,
    TextPlain,
}

#[cfg(target_os = "linux")]
impl TextMime {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::TextPlainUtf8 => "text/plain;charset=utf-8",
            Self::Utf8String => "UTF8_STRING",
            Self::TextPlain => "text/plain",
        }
    }

    /// The first UTF-8 type offered, else plain text as a fallback; mirrors
    /// smithay-clipboard's `MimeType::find_allowed` exactly, so the
    /// data-control reader and smithay answer the same offer identically.
    pub(super) fn choose(offered: &[String]) -> Option<Self> {
        let mut fallback = None;
        for mime in offered {
            match mime.as_str() {
                "text/plain;charset=utf-8" => return Some(Self::TextPlainUtf8),
                "UTF8_STRING" => return Some(Self::Utf8String),
                "text/plain" => fallback = Some(Self::TextPlain),
                _ => {}
            }
        }
        fallback
    }

    /// Decode transferred bytes the way smithay-clipboard does: lossy UTF-8,
    /// and CR/CRLF line ends normalized to LF for the `text/plain` types.
    pub(super) fn decode(self, bytes: &[u8]) -> String {
        let text = String::from_utf8_lossy(bytes).into_owned();
        match self {
            Self::TextPlainUtf8 | Self::TextPlain => text.replace("\r\n", "\n").replace('\r', "\n"),
            Self::Utf8String => text,
        }
    }
}

/// Classify a smithay-clipboard read error.
///
/// This is the only place its message strings appear.  They are matched
/// because smithay-clipboard 0.7.3 exposes no typed error for them: an unowned
/// selection is `Error::other("selection is empty")` and an offer without a
/// readable text type is `Error::new(NotFound, "supported mime-type is not
/// found")` (`state.rs:149-186`).  Anything else — no keyboard focus, no seat,
/// a dead worker — is a real failure and returns `None`.
#[cfg(target_os = "linux")]
pub(super) fn classify_smithay(err: &std::io::Error) -> Option<TextAbsence> {
    if err.kind() == std::io::ErrorKind::NotFound
        || err.to_string() == "supported mime-type is not found"
    {
        return Some(TextAbsence::TargetUnavailable);
    }
    match err.to_string().as_str() {
        "selection is empty" => Some(TextAbsence::NoSelection),
        _ => None,
    }
}

/// Classify an arboard read error.
///
/// arboard's `ContentNotAvailable` covers both "the clipboard is empty" and
/// "the contents have an incompatible format" (`common.rs:19-24`), so it can
/// never name a specific absence and maps to `Indeterminate`.  Every other
/// variant is a real failure.  (`Error` is `#[non_exhaustive]`, so the
/// catch-all is required.)
pub(super) fn classify_arboard(err: &arboard::Error) -> Option<TextAbsence> {
    match err {
        arboard::Error::ContentNotAvailable => Some(TextAbsence::Indeterminate),
        _ => None,
    }
}

/// Classify a smithay read, consulting `fallback` only when smithay saw no
/// usable text offer.
///
/// A failed fallback is logged and smithay's answer is kept: absence stays
/// absence, and the caller must not turn it into an error.  A smithay error
/// that does not classify (no focus, no seat, dead worker) is a real failure
/// and is returned without asking the fallback.
#[cfg(target_os = "linux")]
pub(super) fn text_or_fallback(
    result: std::io::Result<String>,
    fallback: impl FnOnce() -> Result<Option<TextRead>, String>,
) -> Result<TextRead, String> {
    let native = match result {
        Ok(text) => return Ok(TextRead::Text(text)),
        Err(err) => match classify_smithay(&err) {
            Some(absence) => TextRead::from(absence),
            None => return Err(err.to_string()),
        },
    };
    match fallback() {
        Ok(Some(read)) => Ok(read),
        Ok(None) => Ok(native),
        Err(err) => {
            tracing::warn!("clipboard data-control read failed: {err}");
            Ok(native)
        }
    }
}

/// The Wayland read policy for one selection: consult the data-control
/// fallback only for CLIPBOARD.
///
/// smithay-clipboard owns the only primary-selection device and the reader
/// speaks CLIPBOARD only, so a PRIMARY absence is final.  The native result is
/// passed in rather than read here so the routing is testable without a live
/// display.
#[cfg(target_os = "linux")]
pub(super) fn wayland_read(
    selection: ClipboardSelection,
    native: std::io::Result<String>,
    data_control: impl FnOnce() -> Result<Option<TextRead>, String>,
) -> Result<TextRead, String> {
    match selection {
        ClipboardSelection::Clipboard => text_or_fallback(native, data_control),
        ClipboardSelection::Primary => text_or_fallback(native, || Ok(None)),
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tests/text_policy_test.rs"]
mod tests;
