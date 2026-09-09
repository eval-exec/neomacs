//! Numeric argument validation; ncurses owns formatting and variable state.
use crate::Error;
use regex::bytes::Regex;
use std::ffi::CString;
use std::sync::LazyLock;

mod division;

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
/// String-consuming directives and malformed/unsupported numeric syntax return
/// an error before the native call. Division/remainder require a statically
/// proven safe divisor, including constants computed on the stack or stored in
/// variables within this program. Values inherited from native variables or
/// supplied as parameters are unknown to the proof. This remains a conservative
/// numeric subset, not a general string-parameter tparm interface.
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
    if tokens
        .iter()
        .any(|range| matches!(&sequence[range.clone()], b"%/" | b"%m"))
    {
        division::validate(sequence, &tokens)?;
    }
    let format = CString::new(sequence).map_err(|_| Error::InvalidNumericFormat)?;
    #[cfg(any(target_os = "linux", target_os = "macos"))]
    {
        crate::native::expand(&format, parameters)
    }
    #[cfg(not(any(target_os = "linux", target_os = "macos")))]
    {
        let _ = (format, parameters);
        Err(Error::UnsupportedPlatform)
    }
}
