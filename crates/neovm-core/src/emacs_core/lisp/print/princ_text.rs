//! Encoding decisions shared by every no-escape string printer.

use crate::emacs_core::emacs_char;
use crate::heap_types::LispString;
use std::borrow::Cow;

/// Whether GNU's stream preparation enabled escaping of raw byte8 characters.
/// This is per-render state, borrowed by one mutator; there is no shared cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrincStringOutput {
    PreserveByte8,
    OctalByte8,
    CallbackCharacters,
}

/// The print target determines how unibyte string bytes become characters.
/// Per-render state only; no mutator-local or shared cache.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum PrincStringDestination {
    Text,
    Callable,
}

/// An immutable source borrowed only while constructing output, before callbacks.
/// The LispString constructor establishes each variant's encoding invariant.
#[derive(Clone, Copy, Debug)]
enum PrincStringSource<'a> {
    Unibyte(&'a [u8]),
    Multibyte(&'a [u8]),
}

// These contain immutable bytes or scalar encoding policy only. No heap handle
// or mutator state is transferred with them; the source borrow bounds validity.
static_assertions::assert_impl_all!(PrincStringOutput: Send, Sync);
static_assertions::assert_impl_all!(PrincStringDestination: Send, Sync);
static_assertions::assert_impl_all!(PrincStringSource<'static>: Send, Sync);

impl<'a> From<&'a LispString> for PrincStringSource<'a> {
    fn from(string: &'a LispString) -> Self {
        if string.is_multibyte() {
            Self::Multibyte(string.as_bytes())
        } else {
            Self::Unibyte(string.as_bytes())
        }
    }
}

/// Canonical Emacs bytes suitable for a multibyte sink. Unlike a callable
/// stream, an unibyte high byte becomes byte8 when preserved (print.c:479-495).
/// GNU's print_string_1 / string_escape_byte8 escape byte8 in either encoding
/// and preserve ordinary Unicode and non-Unicode characters (print.c:472-479;
/// character.c:773-835).
pub(crate) fn princ_string_bytes(
    string: &LispString,
    escaping: PrincStringOutput,
) -> Cow<'_, [u8]> {
    match (PrincStringSource::from(string), escaping) {
        (PrincStringSource::Unibyte(bytes), PrincStringOutput::PreserveByte8) => {
            if bytes.is_ascii() {
                Cow::Borrowed(bytes)
            } else {
                Cow::Owned(emacs_char::str_to_multibyte(bytes))
            }
        }
        (PrincStringSource::Unibyte(bytes), PrincStringOutput::OctalByte8) => {
            if bytes.is_ascii() {
                Cow::Borrowed(bytes)
            } else {
                Cow::Owned(crate::emacs_core::string_escape::octal_escape_unibyte_eight_bit(bytes))
            }
        }
        (PrincStringSource::Unibyte(bytes), PrincStringOutput::CallbackCharacters) => {
            if bytes.is_ascii() {
                return Cow::Borrowed(bytes);
            }
            let mut out = Vec::with_capacity(bytes.len());
            for &byte in bytes {
                let mut encoded = [0; emacs_char::MAX_MULTIBYTE_LENGTH];
                let len = emacs_char::char_string(u32::from(byte), &mut encoded);
                out.extend_from_slice(&encoded[..len]);
            }
            Cow::Owned(out)
        }
        (PrincStringSource::Multibyte(bytes), PrincStringOutput::CallbackCharacters) => {
            Cow::Borrowed(bytes)
        }
        (PrincStringSource::Multibyte(bytes), PrincStringOutput::PreserveByte8) => {
            Cow::Borrowed(bytes)
        }
        (PrincStringSource::Multibyte(bytes), PrincStringOutput::OctalByte8) => {
            // GNU character.c:786 skips ASCII via cached character/byte counts.
            // In canonical Emacs multibyte text, C0/C1 occur only as the heads
            // of two-byte raw-byte characters, never as continuation bytes.
            if string.schars() == string.sbytes() {
                return Cow::Borrowed(bytes);
            }
            let Some(first_byte8) = memchr::memchr2(0xC0, 0xC1, bytes) else {
                return Cow::Borrowed(bytes);
            };
            // Count both canonical heads with specialized bulk byte scans.
            // Their sets are disjoint; continuation bytes cannot be either head.
            let tail = &bytes[first_byte8..];
            let byte8_count =
                memchr::memchr_iter(0xC0, tail).count() + memchr::memchr_iter(0xC1, tail).count();
            // Each two-byte byte8 pair becomes four ASCII octal bytes (GNU
            // character.c:798-805). Checked reservation arithmetic cannot
            // introduce a capacity-overflow panic. If the final capacity is
            // unrepresentable, retain the existing incremental Vec growth.
            let capacity = byte8_count
                .checked_mul(2)
                .and_then(|extra| bytes.len().checked_add(extra))
                .filter(|size| *size <= isize::MAX as usize)
                .unwrap_or(bytes.len());
            let mut out = Vec::with_capacity(capacity);
            let mut ordinary_start = 0;
            let mut raw_start = first_byte8;
            loop {
                out.extend_from_slice(&bytes[ordinary_start..raw_start]);
                // Decode adjacent canonical byte8 pairs locally. Search only
                // when an ordinary span separates runs, not once per head.
                while raw_start < bytes.len() && matches!(bytes[raw_start], 0xC0 | 0xC1) {
                    let (code, len) = emacs_char::string_char(&bytes[raw_start..]);
                    let byte = emacs_char::char_to_byte8(code.code());
                    out.extend_from_slice(&[
                        b'\\',
                        b'0' + (byte >> 6),
                        b'0' + ((byte >> 3) & 7),
                        b'0' + (byte & 7),
                    ]);
                    raw_start += len;
                }
                ordinary_start = raw_start;
                let Some(offset) = memchr::memchr2(0xC0, 0xC1, &bytes[ordinary_start..]) else {
                    break;
                };
                raw_start = ordinary_start + offset;
            }
            out.extend_from_slice(&bytes[ordinary_start..]);
            Cow::Owned(out)
        }
    }
}
