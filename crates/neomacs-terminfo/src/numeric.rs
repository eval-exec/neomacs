//! Numeric argument validation, not another terminfo interpreter.
use crate::Error;
use regex::bytes::Regex;
use std::ffi::CString;
use std::sync::LazyLock;

// Recognize complete directives, including printf flags, before calling C.
// In particular %s, %:10.2s and %l must never make numeric slots into pointers.
// The grammar uses the mature regex engine; ncurses still interprets programs.
// Numeric constants are additionally checked against the native signed domain.
// Division/remainder are accepted only as part of an immediately preceding
// nonnegative literal constant token. This excludes native INT_MIN / -1 traps
// even when the program computes its operands. ncurses handles a zero divisor.
static TOKEN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?x-u)(?:
    [^%\x00]+ | %% | %[i+*&|^=><AO!~?te;-]
    | %p[1-9] | %[Pg][a-zA-Z] | %'[^\x00]'
    | %\{(?P<constant>[0-9]{1,10})\}(?:%[/m])?
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
/// an error before the native call. This is a conservative numeric subset, not
/// a general string-parameter tparm interface. Division/remainder require an
/// immediately preceding nonnegative literal divisor (`%{256}%/`, for example).
/// Computed divisors are unsupported. The interpreter remains ncurses.
pub fn expand_numeric(sequence: &[u8], parameters: [i32; 9]) -> Result<Vec<u8>, Error> {
    let mut end = 0;
    let mut pushes = 0;
    let mut explicit_parameters = false;
    let mut division = false;
    for token in TOKEN.captures_iter(sequence) {
        let matched = token.get(0).expect("a capture has a full match");
        if matched.start() != end {
            return Err(Error::InvalidNumericFormat);
        }
        end = matched.end();
        let directive = matched.as_bytes();
        explicit_parameters |= directive.starts_with(b"%p");
        pushes += usize::from(matches!(
            directive.get(..2),
            Some(b"%p" | b"%g" | b"%\'" | b"%{")
        ));
        division |= directive.starts_with(b"%{")
            && (directive.ends_with(b"%/") || directive.ends_with(b"%m"));
        if let Some(constant) = token.name("constant")
            && std::str::from_utf8(constant.as_bytes())
                .ok()
                .and_then(|s| s.parse::<i32>().ok())
                .is_none()
        {
            return Err(Error::InvalidNumericFormat);
        }
    }
    if end != sequence.len() {
        return Err(Error::InvalidNumericFormat);
    }
    // ncurses has 20 stack slots and drops pushes on overflow. A dropped
    // literal could invalidate our divisor guarantee. Count every possible
    // push across all branches, reserving space for up to nine implicit
    // termcap parameters when no %p directive occurs. Arithmetic cannot grow
    // a nonempty stack. This deliberately rejects unusually large programs.
    let push_limit = if explicit_parameters { 19 } else { 10 };
    if division && pushes > push_limit {
        return Err(Error::InvalidNumericFormat);
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
