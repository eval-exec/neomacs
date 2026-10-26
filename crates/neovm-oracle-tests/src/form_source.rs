//! Completeness check for an oracle test's embedded Lisp source.
//!
//! The evaluator's read loop treats end-of-file as "no more forms" and returns
//! the last value evaluated.  A form source that is truncated (an unclosed
//! list/vector, string, character literal, or reader prefix) therefore never
//! evaluates the intended probe, and the test silently records a stale value.
//! A stray closing delimiter or trailing text has the same effect by ending
//! the read with an error before the intended form runs.
//!
//! [`check_form_source`] classifies the source before either engine runs.  The
//! rules deliberately model GNU Emacs's reader, not Neomacs's reader: this is
//! a check of the TEST SOURCE, so a source that GNU would not read completely
//! must be rejected even if another reader would accept it.  The checker is a
//! structural reader only; it does not evaluate or construct Lisp values.

/// Byte offset from the start of the embedded form source.
///
/// Kept as a private-field newtype so a character index cannot be passed where
/// the scanner means a byte position.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FormSourceByteOffset(usize);

impl FormSourceByteOffset {
    fn new(offset: usize) -> Self {
        Self(offset)
    }
}

impl std::fmt::Display for FormSourceByteOffset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "byte {}", self.0)
    }
}

/// Delimiter kind tracked while reading.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormSourceDelimiter {
    List,
    Vector,
}

impl FormSourceDelimiter {
    fn opener(self) -> char {
        match self {
            Self::List => '(',
            Self::Vector => '[',
        }
    }

    fn closer(self) -> char {
        match self {
            Self::List => ')',
            Self::Vector => ']',
        }
    }
}

/// Reader prefixes that require a following form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormSourceReaderPrefix {
    Quote,
    FunctionQuote,
    Backquote,
    Unquote,
    UnquoteSplicing,
    /// `#N=FORM` labels the following form; without a form GNU signals
    /// end-of-file and the harness would silently record the previous value.
    LabelDefinition,
}

impl std::fmt::Display for FormSourceReaderPrefix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Quote => "'",
            Self::FunctionQuote => "#'",
            Self::Backquote => "`",
            Self::Unquote => ",",
            Self::UnquoteSplicing => ",@",
            Self::LabelDefinition => "#N=",
        })
    }
}

/// Why the source ended before a form was complete.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormSourceTruncationCause {
    /// A list or vector was still open; carries the opening offset and kind.
    UnclosedDelimiter {
        opened_at: FormSourceByteOffset,
        delimiter: FormSourceDelimiter,
    },
    UnterminatedString,
    UnterminatedCharacterLiteral,
    /// A quote/backquote/unquote at the end of the source has no form.
    DanglingReaderPrefix(FormSourceReaderPrefix),
    /// A symbol ended with a backslash that escapes nothing.
    DanglingSymbolEscape,
}

/// Why input remains that GNU's reader cannot read as another form.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormSourceTrailingCause {
    /// A closing delimiter with no matching opener.
    UnmatchedClosingDelimiter(FormSourceDelimiter),
    /// A closing delimiter that does not match the innermost opener.
    MismatchedClosingDelimiter {
        opened_at: FormSourceByteOffset,
        opened: FormSourceDelimiter,
        close: FormSourceDelimiter,
    },
    /// A reader prefix is immediately followed by a closing delimiter.
    ReaderPrefixBeforeDelimiter(FormSourceReaderPrefix),
    /// A `#` dispatch with no dispatch character after it.
    BareDispatchHash,
    /// A `#` dispatch character that GNU's reader does not accept here.
    UnrecognizedDispatchHash,
    /// A lone `.` at top level (GNU's reader rejects it; `.foo` is a symbol).
    BareDot,
}

/// Result of reading a complete source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FormSourceCheck {
    /// Every byte was consumed by complete top-level forms.
    Complete,
    /// The source is balanced but contains no top-level form, so evaluating
    /// it would do nothing and record the previous value.
    NoForms,
    /// The source ends inside a form.
    Truncated {
        offset: FormSourceByteOffset,
        cause: FormSourceTruncationCause,
    },
    /// The source contains input that cannot start another form.
    Trailing {
        offset: FormSourceByteOffset,
        cause: FormSourceTrailingCause,
    },
}

impl std::fmt::Display for FormSourceCheck {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Complete => f.write_str("complete"),
            Self::NoForms => f.write_str("no top-level form"),
            Self::Truncated { offset, cause } => {
                write!(f, "truncated at {offset}: ")?;
                match cause {
                    FormSourceTruncationCause::UnclosedDelimiter {
                        opened_at,
                        delimiter,
                    } => write!(
                        f,
                        "unclosed {} opened at {}",
                        delimiter_name(*delimiter),
                        opened_at
                    ),
                    FormSourceTruncationCause::UnterminatedString => {
                        f.write_str("unterminated string")
                    }
                    FormSourceTruncationCause::UnterminatedCharacterLiteral => {
                        f.write_str("unterminated character literal")
                    }
                    FormSourceTruncationCause::DanglingReaderPrefix(prefix) => {
                        write!(f, "reader prefix `{prefix}` has no following form")
                    }
                    FormSourceTruncationCause::DanglingSymbolEscape => {
                        f.write_str("symbol ends with an escaping backslash")
                    }
                }
            }
            Self::Trailing { offset, cause } => {
                write!(f, "trailing input at {offset}: ")?;
                match cause {
                    FormSourceTrailingCause::UnmatchedClosingDelimiter(delimiter) => write!(
                        f,
                        "closing {} has no matching opener",
                        delimiter_char(*delimiter)
                    ),
                    FormSourceTrailingCause::MismatchedClosingDelimiter {
                        opened_at,
                        opened,
                        close,
                    } => write!(
                        f,
                        "closing {} does not match the {} opened at {}",
                        delimiter_char(*close),
                        delimiter_name(*opened),
                        opened_at
                    ),
                    FormSourceTrailingCause::ReaderPrefixBeforeDelimiter(prefix) => write!(
                        f,
                        "reader prefix `{prefix}` is followed by a closing delimiter"
                    ),
                    FormSourceTrailingCause::BareDispatchHash => {
                        f.write_str("`#` dispatch has no dispatch character")
                    }
                    FormSourceTrailingCause::UnrecognizedDispatchHash => {
                        f.write_str("`#` dispatch is not accepted by GNU's reader")
                    }
                    FormSourceTrailingCause::BareDot => {
                        f.write_str("lone `.` is not a readable form")
                    }
                }
            }
        }
    }
}

fn delimiter_name(delimiter: FormSourceDelimiter) -> &'static str {
    match delimiter {
        FormSourceDelimiter::List => "list",
        FormSourceDelimiter::Vector => "vector",
    }
}

fn delimiter_char(delimiter: FormSourceDelimiter) -> char {
    delimiter.closer()
}

#[derive(Clone, Copy)]
struct OpenDelimiter {
    kind: FormSourceDelimiter,
    opened_at: FormSourceByteOffset,
}

#[derive(Clone, Copy)]
struct PendingPrefix {
    kind: FormSourceReaderPrefix,
    at: FormSourceByteOffset,
}

/// Read `source` with the completeness rules of GNU Emacs's Lisp reader.
///
/// Only delimiters, strings, comments, reader prefixes, character literals,
/// and symbol escapes are interpreted; values and evaluation are irrelevant to
/// the question of whether every top-level form can be read.
pub(crate) fn check_form_source(source: &str) -> FormSourceCheck {
    let bytes = source.as_bytes();
    let mut stack: Vec<OpenDelimiter> = Vec::new();
    let mut pending_prefix: Option<PendingPrefix> = None;
    let mut top_level_form_open = false;
    let mut forms = 0usize;
    let mut i = 0;

    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0c => i += 1,
            b';' => {
                i += 1;
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
            }
            b'(' => {
                if stack.is_empty() {
                    top_level_form_open = true;
                }
                stack.push(OpenDelimiter {
                    kind: FormSourceDelimiter::List,
                    opened_at: FormSourceByteOffset::new(i),
                });
                pending_prefix = None;
                i += 1;
            }
            b'[' => {
                if stack.is_empty() {
                    top_level_form_open = true;
                }
                stack.push(OpenDelimiter {
                    kind: FormSourceDelimiter::Vector,
                    opened_at: FormSourceByteOffset::new(i),
                });
                pending_prefix = None;
                i += 1;
            }
            b')' | b']' => {
                let close = if bytes[i] == b')' {
                    FormSourceDelimiter::List
                } else {
                    FormSourceDelimiter::Vector
                };
                let offset = FormSourceByteOffset::new(i);
                if let Some(prefix) = pending_prefix {
                    return FormSourceCheck::Trailing {
                        offset,
                        cause: FormSourceTrailingCause::ReaderPrefixBeforeDelimiter(prefix.kind),
                    };
                }
                match stack.pop() {
                    None => {
                        return FormSourceCheck::Trailing {
                            offset,
                            cause: FormSourceTrailingCause::UnmatchedClosingDelimiter(close),
                        };
                    }
                    Some(open) if open.kind == close => {
                        i += 1;
                        if stack.is_empty() && top_level_form_open {
                            forms += 1;
                            top_level_form_open = false;
                        }
                    }
                    Some(open) => {
                        return FormSourceCheck::Trailing {
                            offset,
                            cause: FormSourceTrailingCause::MismatchedClosingDelimiter {
                                opened_at: open.opened_at,
                                opened: open.kind,
                                close,
                            },
                        };
                    }
                }
            }
            b'"' => match scan_string(bytes, i) {
                Some(end) => {
                    i = end;
                    pending_prefix = None;
                    if stack.is_empty() {
                        forms += 1;
                    }
                }
                None => {
                    return FormSourceCheck::Truncated {
                        offset: FormSourceByteOffset::new(i),
                        cause: FormSourceTruncationCause::UnterminatedString,
                    };
                }
            },
            b'?' => match scan_character(bytes, i) {
                Some(end) => {
                    i = end;
                    pending_prefix = None;
                    if stack.is_empty() {
                        forms += 1;
                    }
                }
                None => {
                    return FormSourceCheck::Truncated {
                        offset: FormSourceByteOffset::new(i),
                        cause: FormSourceTruncationCause::UnterminatedCharacterLiteral,
                    };
                }
            },
            b'\'' => {
                pending_prefix = Some(PendingPrefix {
                    kind: FormSourceReaderPrefix::Quote,
                    at: FormSourceByteOffset::new(i),
                });
                i += 1;
            }
            b'`' => {
                pending_prefix = Some(PendingPrefix {
                    kind: FormSourceReaderPrefix::Backquote,
                    at: FormSourceByteOffset::new(i),
                });
                i += 1;
            }
            b',' => {
                let (kind, consumed) = if bytes.get(i + 1) == Some(&b'@') {
                    (FormSourceReaderPrefix::UnquoteSplicing, 2)
                } else {
                    (FormSourceReaderPrefix::Unquote, 1)
                };
                pending_prefix = Some(PendingPrefix {
                    kind,
                    at: FormSourceByteOffset::new(i),
                });
                i += consumed;
            }
            b'#' => {
                let Some(&dispatch) = bytes.get(i + 1) else {
                    return FormSourceCheck::Trailing {
                        offset: FormSourceByteOffset::new(i),
                        cause: FormSourceTrailingCause::BareDispatchHash,
                    };
                };
                match dispatch {
                    b'\'' => {
                        pending_prefix = Some(PendingPrefix {
                            kind: FormSourceReaderPrefix::FunctionQuote,
                            at: FormSourceByteOffset::new(i),
                        });
                        i += 2;
                    }
                    b'(' => {
                        if stack.is_empty() {
                            top_level_form_open = true;
                        }
                        stack.push(OpenDelimiter {
                            kind: FormSourceDelimiter::List,
                            opened_at: FormSourceByteOffset::new(i + 1),
                        });
                        pending_prefix = None;
                        i += 2;
                    }
                    b'[' => {
                        if stack.is_empty() {
                            top_level_form_open = true;
                        }
                        stack.push(OpenDelimiter {
                            kind: FormSourceDelimiter::Vector,
                            opened_at: FormSourceByteOffset::new(i + 1),
                        });
                        pending_prefix = None;
                        i += 2;
                    }
                    b's' => {
                        // `#s(...)' is a record literal; the list follows the
                        // dispatch name and is handled by the next iteration.
                        // A non-list continuation is a reader error, not a
                        // truncation, so it is left to the rejecting engine.
                        i += 2;
                        pending_prefix = None;
                    }
                    b'0'..=b'9' => {
                        // `#N=FORM` labels the following form, and `#N#`
                        // references a label.  A label needs a following form;
                        // a reference is a complete token.
                        let mut j = i + 1;
                        while j < bytes.len() && bytes[j].is_ascii_digit() {
                            j += 1;
                        }
                        match bytes.get(j).copied() {
                            Some(b'=') => {
                                pending_prefix = Some(PendingPrefix {
                                    kind: FormSourceReaderPrefix::LabelDefinition,
                                    at: FormSourceByteOffset::new(i),
                                });
                                i = j + 1;
                            }
                            Some(b'#') => {
                                i = j + 1;
                                pending_prefix = None;
                                if stack.is_empty() {
                                    forms += 1;
                                }
                            }
                            _ => {
                                // Radix integers such as `#16rff' continue
                                // through the base marker and digits.
                                i = scan_symbol_tail(bytes, j);
                                pending_prefix = None;
                                if stack.is_empty() {
                                    forms += 1;
                                }
                            }
                        }
                    }
                    b'x' | b'X' | b'o' | b'O' | b'b' | b'B' => {
                        // Radix integer literals: `#xFF', `#o755', `#b1010'.
                        i = scan_symbol_tail(bytes, i + 2);
                        pending_prefix = None;
                        if stack.is_empty() {
                            forms += 1;
                        }
                    }
                    b':' | b'_' | b'$' | b'&' | b'^' | b'@' | b'.' | b'#' => {
                        i = scan_symbol_tail(bytes, i + 2);
                        pending_prefix = None;
                        if stack.is_empty() {
                            forms += 1;
                        }
                    }
                    b'<' => {
                        // An unreadable object is rejected by the engine; it
                        // cannot start a form that continues past the line.
                        i = scan_symbol_tail(bytes, i + 2);
                        pending_prefix = None;
                        if stack.is_empty() {
                            forms += 1;
                        }
                    }
                    _ => {
                        return FormSourceCheck::Trailing {
                            offset: FormSourceByteOffset::new(i),
                            cause: FormSourceTrailingCause::UnrecognizedDispatchHash,
                        };
                    }
                }
            }
            b'.' if stack.is_empty() && is_token_end(bytes, i + 1) => {
                return FormSourceCheck::Trailing {
                    offset: FormSourceByteOffset::new(i),
                    cause: FormSourceTrailingCause::BareDot,
                };
            }
            b'\\' => match scan_symbol_tail_checked(bytes, i) {
                Some(end) => {
                    i = end;
                    pending_prefix = None;
                    if stack.is_empty() {
                        forms += 1;
                    }
                }
                None => {
                    return FormSourceCheck::Truncated {
                        offset: FormSourceByteOffset::new(i),
                        cause: FormSourceTruncationCause::DanglingSymbolEscape,
                    };
                }
            },
            _ => {
                i = scan_symbol_tail(bytes, i);
                pending_prefix = None;
                if stack.is_empty() {
                    forms += 1;
                }
            }
        }
    }

    if let Some(open) = stack.pop() {
        return FormSourceCheck::Truncated {
            offset: FormSourceByteOffset::new(bytes.len()),
            cause: FormSourceTruncationCause::UnclosedDelimiter {
                opened_at: open.opened_at,
                delimiter: open.kind,
            },
        };
    }
    if let Some(prefix) = pending_prefix {
        return FormSourceCheck::Truncated {
            offset: prefix.at,
            cause: FormSourceTruncationCause::DanglingReaderPrefix(prefix.kind),
        };
    }
    if forms == 0 {
        FormSourceCheck::NoForms
    } else {
        FormSourceCheck::Complete
    }
}

/// Scan a string literal starting at its opening quote.  Returns the offset
/// just past the closing quote, or `None` when the source ends inside it.
fn scan_string(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    while i < bytes.len() {
        match bytes[i] {
            b'\\' => i += 2,
            b'"' => return Some(i + 1),
            _ => i += 1,
        }
    }
    None
}

/// Scan a character literal starting at `?`.  GNU accepts an unescaped
/// delimiter as the literal character (`?(` and `?)`), so the delimiter never
/// participates in list structure.
fn scan_character(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start + 1;
    if i >= bytes.len() {
        return None;
    }
    if bytes[i] != b'\\' {
        return Some(i + 1);
    }
    i += 1;
    loop {
        if i >= bytes.len() {
            return None;
        }
        if starts_modifier(bytes, i) {
            i += 2;
            if i >= bytes.len() {
                return None;
            }
            if bytes[i] == b'\\' {
                i += 1;
                continue;
            }
            return Some(consume_final_character(bytes, i));
        }
        if bytes[i] == b'^' {
            i += 1;
            if i >= bytes.len() {
                return None;
            }
            return Some(consume_final_character(bytes, i));
        }
        return Some(consume_escaped_final(bytes, i));
    }
}

/// Modifier prefixes in GNU's character syntax (`?\C-x`, `?\M-\C-x`, ...).
fn starts_modifier(bytes: &[u8], i: usize) -> bool {
    matches!(
        bytes.get(i).copied(),
        Some(b'C' | b'M' | b'S' | b'H' | b'A' | b's')
    ) && bytes.get(i + 1) == Some(&b'-')
}

/// The character after a modifier is part of the literal even when it is a
/// delimiter (`?\C-)`).
fn consume_final_character(bytes: &[u8], i: usize) -> usize {
    i + 1
}

/// An escaped character after `?\`.  `\N{...}` is the only escape whose name
/// can contain a delimiter or whitespace; it is consumed through its closing
/// brace so a `)` in a Unicode character name never closes a list.
fn consume_escaped_final(bytes: &[u8], i: usize) -> usize {
    if bytes.get(i) == Some(&b'N') && bytes.get(i + 1) == Some(&b'{') {
        let mut j = i + 2;
        while j < bytes.len() {
            if bytes[j] == b'}' {
                return j + 1;
            }
            j += 1;
        }
        return bytes.len();
    }
    i + 1
}

/// Scan the rest of a symbol/atom starting at `start`, which is known not to
/// be a delimiter, quote prefix, string, or comment.  Symbol escapes consume
/// the escaped byte so a delimiter cannot be misread as structure.
fn scan_symbol_tail(bytes: &[u8], start: usize) -> usize {
    scan_symbol_tail_checked(bytes, start).unwrap_or(bytes.len())
}

fn scan_symbol_tail_checked(bytes: &[u8], start: usize) -> Option<usize> {
    let mut i = start;
    while i < bytes.len() {
        match bytes[i] {
            b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'(' | b')' | b'[' | b']' | b'"' | b';'
            | b'\'' | b',' | b'#' => return Some(i),
            b'\\' => {
                if i + 1 >= bytes.len() {
                    return None;
                }
                i += 2;
            }
            _ => i += 1,
        }
    }
    Some(i)
}

/// Whether the byte at `i` (or end of source) terminates a bare `.` token.
fn is_token_end(bytes: &[u8], i: usize) -> bool {
    match bytes.get(i) {
        None => true,
        Some(
            b' ' | b'\t' | b'\n' | b'\r' | 0x0c | b'(' | b')' | b'[' | b']' | b'"' | b';' | b'\''
            | b',' | b'#',
        ) => true,
        Some(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(source: &str) -> FormSourceCheck {
        check_form_source(source)
    }

    #[test]
    fn accepts_complete_forms_and_sequences() {
        for source in [
            "(a)",
            "\n(progn (a) (b))\n",
            "(a) (b)",
            "(a) ; trailing comment without newline",
            "\"a string with ) and [ inside\"",
            "(?\\()",
            "(?C-x ?\\))",
            "(?\\C-x ?\\M-\\C-))",
            "(?\\^?)",
            "(?\\N{LEFT PARENTHESIS})",
            "(foo\\(bar foo\\)bar)",
            "(#'car '(1 2))",
            "(`(a ,b ,@c))",
            "([1 2 3] #s(a 1) #1=(a . #1#))",
            "#xFF #o755 #b1010 #16rff #1=(a)",
            "(insert ?\\;)",
            "(a) .foo",
            "(a . b)",
        ] {
            assert_eq!(check(source), FormSourceCheck::Complete, "{source:?}");
        }
    }

    #[test]
    fn rejects_sources_without_a_form() {
        for source in ["", " \n\t\r\n", "; only a comment", "; a\n; b\n"] {
            assert_eq!(check(source), FormSourceCheck::NoForms, "{source:?}");
        }
    }

    #[test]
    fn rejects_truncated_forms() {
        assert!(matches!(
            check("(a"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::UnclosedDelimiter {
                    delimiter: FormSourceDelimiter::List,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            check("(a [b"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::UnclosedDelimiter {
                    delimiter: FormSourceDelimiter::Vector,
                    ..
                },
                ..
            }
        ));
        assert!(matches!(
            check("(a \"unterminated"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::UnterminatedString,
                ..
            }
        ));
        assert!(matches!(
            check("(a ?\\"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::UnterminatedCharacterLiteral,
                ..
            }
        ));
        assert!(matches!(
            check("(a) '"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::DanglingReaderPrefix(
                    FormSourceReaderPrefix::Quote
                ),
                ..
            }
        ));
        assert!(matches!(
            check("(a) \\"),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::DanglingSymbolEscape,
                ..
            }
        ));
        assert!(matches!(
            check("#1="),
            FormSourceCheck::Truncated {
                cause: FormSourceTruncationCause::DanglingReaderPrefix(
                    FormSourceReaderPrefix::LabelDefinition
                ),
                ..
            }
        ));
    }

    #[test]
    fn rejects_trailing_input() {
        for (source, expected) in [
            (
                "(a) )",
                FormSourceTrailingCause::UnmatchedClosingDelimiter(FormSourceDelimiter::List),
            ),
            (
                "(a) ]",
                FormSourceTrailingCause::UnmatchedClosingDelimiter(FormSourceDelimiter::Vector),
            ),
            (
                "(a]",
                FormSourceTrailingCause::MismatchedClosingDelimiter {
                    opened_at: FormSourceByteOffset::new(0),
                    opened: FormSourceDelimiter::List,
                    close: FormSourceDelimiter::Vector,
                },
            ),
            ("(a) #", FormSourceTrailingCause::BareDispatchHash),
            ("(a) .", FormSourceTrailingCause::BareDot),
            (
                "(a ')",
                FormSourceTrailingCause::ReaderPrefixBeforeDelimiter(FormSourceReaderPrefix::Quote),
            ),
        ] {
            match check(source) {
                FormSourceCheck::Trailing { cause, .. } => {
                    assert_eq!(cause, expected, "{source:?}")
                }
                other => panic!("expected trailing input for {source:?}, got {other:?}"),
            }
        }
    }
}
