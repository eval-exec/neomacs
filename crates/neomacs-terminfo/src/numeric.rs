//! Numeric argument validation; ncurses owns formatting and variable state.
use crate::Error;
use regex::bytes::Regex;
use std::ffi::CString;
use std::sync::LazyLock;

pub(crate) mod division;

// Recognize complete directives, including printf flags, before calling C.
// In particular %s, %:10.2s and %l must never make numeric slots into pointers.
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x-u)(?:
    [^%\x00]+ | %% | %[i+*/m&|^=><AO!~?te;-]
    | %p[1-9] | %[Pg][a-zA-Z] | %'[^\x00]'
    | %\{(?P<constant>[0-9]{1,10})\}
    | %(?::[-\x20\#]*)?[\x20\#]*(?:[0-9]{0,4}|10000)
        (?:\.(?:[0-9]{1,4}|10000))?[doxXc]
)",
    )
    .expect("fixed numeric terminfo grammar")
});

/// Expand a numeric terminfo program with nine signed parameter slots.
///
/// Native variable state is preserved across calls, as in GNU's tparam wrapper.
/// Loading another entry selects ncurses' current terminal state. Each complete
/// expansion and result copy is serialized with database loading.
///
/// String-consuming directives and malformed numeric syntax return an error.
/// Division/remainder are checked using the actual parameters and referenced
/// native variables, under the same lock as expansion. Native signed division
/// overflow is rejected; zero division follows ncurses. Implicit termcap
/// argument counts differ between native versions, so all possible initial
/// counts are checked. Formatting and variable updates remain native.
pub fn expand_numeric(sequence: &[u8], parameters: [i32; 9]) -> Result<Vec<u8>, Error> {
    let mut end = 0;
    let mut tokens = Vec::new();
    for token in TOKEN.captures_iter(sequence) {
        let matched = token.get(0).expect("a capture has a full match");
        if matched.start() != end {
            return Err(Error::InvalidNumericFormat);
        }
        end = matched.end();
        if let Some(constant) = token.name("constant")
            && std::str::from_utf8(constant.as_bytes())
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .is_none()
        {
            return Err(Error::InvalidNumericFormat);
        }
        tokens.push(matched.range());
    }
    if end != sequence.len() {
        return Err(Error::InvalidNumericFormat);
    }
    let division = tokens
        .iter()
        .any(|range| matches!(&sequence[range.clone()], b"%/" | b"%m"))
        .then(|| division::Program::new(sequence, tokens));
    let format = CString::new(sequence).map_err(|_| Error::InvalidNumericFormat)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        crate::native::expand(&format, parameters, division.as_ref())
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (format, parameters, division);
        Err(Error::UnsupportedPlatform)
    }
}
