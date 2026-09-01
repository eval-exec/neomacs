//! Incremental decoding for raw Unix TTY input.
//!
//! The display frontend reports byte batches exactly as the OS delivered
//! them.  This module owns the state needed to apply Emacs's
//! `keyboard-coding-system` across arbitrary read boundaries.  It deliberately
//! does not interpret ESC, CSI, Meta, or terminal capabilities; those belong
//! to the keyboard translation maps after characters have been decoded.

use crate::emacs_core::emacs_char::EmacsChar;
use encoding_rs::{BIG5, CoderResult, Decoder, GBK, UTF_16BE, UTF_16LE};

pub(crate) struct KeyboardInputDecoder {
    coding_system: String,
    decoder: DecoderKind,
}

enum DecoderKind {
    Utf8 { pending: Vec<u8> },
    EmacsInternal { pending: Vec<u8> },
    SingleByte,
    BytePreserving,
    EncodingRs(Decoder),
}

impl Default for KeyboardInputDecoder {
    fn default() -> Self {
        Self {
            coding_system: String::new(),
            decoder: DecoderKind::Utf8 {
                pending: Vec::new(),
            },
        }
    }
}

impl KeyboardInputDecoder {
    /// EOL_CONVERSION is required for the reason entry 143 records: this
    /// decoder's `SingleByte` arm goes through the shared string decoder, whose
    /// end-of-line pass GNU guards with `inhibit_eol_conversion` like every
    /// other.  The decoder looks context-free, but it is driven from
    /// `Context::handle_read_char_input_event`, so the answer is always in
    /// reach at the point the bytes arrive.
    pub(crate) fn push(
        &mut self,
        bytes: &[u8],
        coding_system: &str,
        eol_conversion: crate::emacs_core::coding::EolConversion,
    ) -> Vec<EmacsChar> {
        let mut decoded = Vec::new();
        if self.coding_system != coding_system {
            decoded.extend(self.finish_current());
            self.coding_system = coding_system.to_owned();
            self.decoder = decoder_for(coding_system);
        }
        decoded.extend(decode_with(
            &mut self.decoder,
            bytes,
            coding_system,
            eol_conversion,
        ));
        decoded
    }

    fn finish_current(&mut self) -> Vec<EmacsChar> {
        match &mut self.decoder {
            DecoderKind::Utf8 { pending } | DecoderKind::EmacsInternal { pending } => pending
                .drain(..)
                .map(EmacsChar::from_unibyte_byte)
                .collect(),
            DecoderKind::EncodingRs(decoder) => decode_encoding_rs(decoder, &[], true),
            DecoderKind::SingleByte | DecoderKind::BytePreserving => Vec::new(),
        }
    }
}

fn decoder_for(coding_system: &str) -> DecoderKind {
    let family = crate::encoding::coding_system_family(coding_system);
    match family {
        "utf-8" => DecoderKind::Utf8 {
            pending: Vec::new(),
        },
        "utf-8-emacs" => DecoderKind::EmacsInternal {
            pending: Vec::new(),
        },
        "latin-1" | "iso-8859-1" | "iso-latin-1" | "iso-latin-5" | "iso-latin-9" | "ascii"
        | "us-ascii" => DecoderKind::SingleByte,
        "chinese-big5" | "chinese-big5-hkscs" => {
            DecoderKind::EncodingRs(BIG5.new_decoder_without_bom_handling())
        }
        "chinese-iso-8bit" => DecoderKind::EncodingRs(GBK.new_decoder_without_bom_handling()),
        _ if family == "utf-16" || family.starts_with("utf-16be") => {
            DecoderKind::EncodingRs(UTF_16BE.new_decoder_with_bom_removal())
        }
        _ if family.starts_with("utf-16") => {
            DecoderKind::EncodingRs(UTF_16LE.new_decoder_with_bom_removal())
        }
        "binary" | "no-conversion" | "raw-text" | "undecided" => DecoderKind::BytePreserving,
        // The general coding engine currently treats unknown runtime families
        // as UTF-8.  Keep keyboard input aligned with that behavior while the
        // stateful decoder remains behind this one replaceable boundary.
        _ => DecoderKind::Utf8 {
            pending: Vec::new(),
        },
    }
}

fn decode_with(
    decoder: &mut DecoderKind,
    bytes: &[u8],
    coding_system: &str,
    eol_conversion: crate::emacs_core::coding::EolConversion,
) -> Vec<EmacsChar> {
    match decoder {
        DecoderKind::Utf8 { pending } => decode_utf8_incremental(pending, bytes),
        DecoderKind::EmacsInternal { pending } => decode_emacs_internal(pending, bytes),
        DecoderKind::SingleByte => {
            crate::encoding::decode_bytes(bytes, coding_system, eol_conversion)
                .chars()
                .map(EmacsChar::from_char)
                .collect()
        }
        DecoderKind::BytePreserving => bytes
            .iter()
            .copied()
            .map(EmacsChar::from_unibyte_byte)
            .collect(),
        DecoderKind::EncodingRs(decoder) => decode_encoding_rs(decoder, bytes, false),
    }
}

fn decode_emacs_internal(pending: &mut Vec<u8>, bytes: &[u8]) -> Vec<EmacsChar> {
    use crate::emacs_core::emacs_char::{
        bytes_by_char_head, char_head_p, multibyte_length, string_char, trailing_code_p,
    };

    pending.extend_from_slice(bytes);
    let mut decoded = Vec::new();
    let mut consumed = 0;

    while consumed < pending.len() {
        let available = &pending[consumed..];
        let first = available[0];
        if first < 0x80 {
            decoded.push(EmacsChar::from_unibyte_byte(first));
            consumed += 1;
            continue;
        }
        if !char_head_p(first) || first > 0xf8 {
            decoded.push(EmacsChar::from_byte8(first));
            consumed += 1;
            continue;
        }

        let width = bytes_by_char_head(first);
        if available.len() < width {
            if available.iter().skip(1).all(|byte| trailing_code_p(*byte)) {
                break;
            }
            decoded.push(EmacsChar::from_byte8(first));
            consumed += 1;
            continue;
        }

        if let Some(width) = multibyte_length(available, true) {
            let (code, actual_width) = string_char(available);
            debug_assert_eq!(width, actual_width);
            decoded.push(EmacsChar::from_code_unchecked(code));
            consumed += width;
        } else {
            decoded.push(EmacsChar::from_byte8(first));
            consumed += 1;
        }
    }

    pending.drain(..consumed);
    decoded
}

fn decode_encoding_rs(decoder: &mut Decoder, bytes: &[u8], last: bool) -> Vec<EmacsChar> {
    let capacity = decoder
        .max_utf8_buffer_length(bytes.len())
        .unwrap_or(bytes.len().saturating_mul(4).saturating_add(16))
        .max(16);
    let mut text = String::with_capacity(capacity);
    let mut offset = 0;
    loop {
        let (result, read, _) = decoder.decode_to_string(&bytes[offset..], &mut text, last);
        offset += read;
        match result {
            CoderResult::InputEmpty => break,
            CoderResult::OutputFull => text.reserve(capacity),
        }
    }
    text.chars().map(EmacsChar::from_char).collect()
}

fn decode_utf8_incremental(pending: &mut Vec<u8>, bytes: &[u8]) -> Vec<EmacsChar> {
    pending.extend_from_slice(bytes);
    let mut decoded = Vec::new();
    let mut consumed = 0;

    while consumed < pending.len() {
        let first = pending[consumed];
        if first < 0x80 {
            decoded.push(EmacsChar::from_unibyte_byte(first));
            consumed += 1;
            continue;
        }

        let width = match first {
            0xc2..=0xdf => 2,
            0xe0..=0xef => 3,
            0xf0..=0xf4 => 4,
            _ => {
                decoded.push(EmacsChar::from_byte8(first));
                consumed += 1;
                continue;
            }
        };
        let available = &pending[consumed..];
        let present_continuations = available
            .iter()
            .skip(1)
            .take(width - 1)
            .all(|byte| byte & 0xc0 == 0x80);
        if available.len() < width {
            if present_continuations {
                break;
            }
            decoded.push(EmacsChar::from_byte8(first));
            consumed += 1;
            continue;
        }

        let candidate = &available[..width];
        match std::str::from_utf8(candidate) {
            Ok(text) => {
                decoded.push(EmacsChar::from_char(
                    text.chars().next().expect("one UTF-8 character"),
                ));
                consumed += width;
            }
            Err(_) => {
                decoded.push(EmacsChar::from_byte8(first));
                consumed += 1;
            }
        }
    }

    pending.drain(..consumed);
    decoded
}
