//! Coding fields are decoded at their native percent stop, never at entry.
//!
//! GNU xdisp.c `decode_mode_spec` (29368, 29779-29812) reads the current
//! buffer and original window frame when it encounters %z/%Z. The registry
//! borrow below exists only for that native span; it cannot cross :eval/%M.

use super::{ModeLineNativeFieldWidth, ModeLineRendered, Value, VariableView};
use crate::emacs_core::coding::CodingSystemManager;

/// Immutable original-frame class; selecting another frame in :eval cannot
/// change whether this formatter's frame has a terminal-coding prefix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum CodingFrame {
    WindowSystem,
    Tty,
}

impl From<&crate::window::Frame> for CodingFrame {
    fn from(frame: &crate::window::Frame) -> Self {
        if frame.effective_window_system().is_none() {
            Self::Tty
        } else {
            Self::WindowSystem
        }
    }
}

/// A native decoder borrows the authoritative registry anew after each Lisp
/// callback. The split-state test seam has no Context/registry authority.
#[derive(Clone, Copy)]
pub(super) enum CodingAccess<'native> {
    Registry(&'native CodingSystemManager),
    #[cfg(test)]
    SplitState,
}

#[derive(Clone, Copy)]
pub(super) enum CodingDetail {
    Mnemonic,
    WithEndOfLine,
}

static_assertions::assert_eq_size!(CodingAccess<'static>, usize);
static_assertions::assert_eq_size!(CodingFrame, u8);

impl CodingAccess<'_> {
    fn frame_mnemonics(self, frame: CodingFrame, _vars: VariableView<'_>) -> FrameCodingMnemonics {
        match frame {
            CodingFrame::WindowSystem => FrameCodingMnemonics::WindowSystem,
            CodingFrame::Tty => {
                let (keyboard, terminal) = match self {
                    Self::Registry(manager) => (
                        coding_system_mnemonic_char(self, manager.keyboard_coding_sym()),
                        coding_system_mnemonic_char(self, manager.terminal_coding_sym()),
                    ),
                    #[cfg(test)]
                    Self::SplitState => {
                        let keyboard = _vars
                            .value_by_name("keyboard-coding-system")
                            .and_then(|value| value.as_bare_symbol_id());
                        let terminal = _vars
                            .value_by_name("terminal-coding-system")
                            .and_then(|value| value.as_bare_symbol_id());
                        (
                            keyboard
                                .map(|name| coding_system_mnemonic_char(self, name))
                                .unwrap_or('-'),
                            terminal
                                .map(|name| coding_system_mnemonic_char(self, name))
                                .unwrap_or('-'),
                        )
                    }
                };
                FrameCodingMnemonics::Tty { keyboard, terminal }
            }
        }
    }

    /// The sole coding-field renderer. Nothing in this borrow span can call
    /// Lisp. Any emitted EOL string/properties join the existing accumulator
    /// roots before the walker can enter a later callback; none is retained
    /// by the formatter context or by this access token.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn append_field(
        self,
        detail: CodingDetail,
        frame: CodingFrame,
        vars: VariableView<'_>,
        buffers: &crate::buffer::BufferManager,
        result: &mut ModeLineRendered,
        properties: &std::collections::HashMap<Value, Value>,
        width: ModeLineNativeFieldWidth,
    ) {
        let buffer = buffers.current_buffer();
        let coding_name = buffer
            .and_then(|buffer| buffer.buffer_local_value("buffer-file-coding-system"))
            .and_then(|value| value.as_bare_symbol_id());
        let mnemonic = coding_name
            .map(|name| coding_system_mnemonic_char(self, name))
            .unwrap_or('-');
        let buffer_coding = if buffer.is_some_and(|buffer| !buffer.get_multibyte()) {
            BufferCodingDisplay::UnibyteBlank
        } else {
            BufferCodingDisplay::Multibyte { mnemonic }
        };
        let text = buffer_coding.with_frame(self.frame_mnemonics(frame, vars));
        match detail {
            CodingDetail::Mnemonic => super::append_mode_line_percent_string_spec(
                result,
                &text,
                properties,
                i64::from(width),
            ),
            CodingDetail::WithEndOfLine => {
                let mut segment = ModeLineRendered::plain(text);
                let eol =
                    coding_name.and_then(|name| coding_system_eol_indicator_value(vars, name));
                match eol {
                    Some(indicator) => {
                        // GNU copies EOL bytes into decode_mode_spec_buf, not
                        // a Lisp-string property/source run (xdisp.c:29325-29346).
                        // The format's percent properties are applied below.
                        if let Some(string) = indicator.as_lisp_string() {
                            segment.append_lisp_string_text(string);
                        } else if let Some(character) = indicator.as_char() {
                            segment.push_plain_char(character);
                        }
                    }
                    None => segment.push_plain_char(':'),
                }
                segment.overlay_property_map(properties.clone());
                super::append_mode_line_rendered_segment(result, &segment, i64::from(width), 0);
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrameCodingMnemonics {
    WindowSystem,
    Tty { keyboard: char, terminal: char },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BufferCodingDisplay {
    Multibyte { mnemonic: char },
    UnibyteBlank,
}

impl BufferCodingDisplay {
    /// GNU `decode_mode_spec` emits keyboard, terminal, then buffer coding on a
    /// non-window-system frame.  It calls the same helper for all three, and
    /// that helper blanks every mnemonic when the current buffer is unibyte
    /// (`src/xdisp.c:29238-29276`).
    fn with_frame(self, frame: FrameCodingMnemonics) -> String {
        match (self, frame) {
            (Self::Multibyte { mnemonic }, FrameCodingMnemonics::WindowSystem) => {
                mnemonic.to_string()
            }
            (Self::Multibyte { mnemonic }, FrameCodingMnemonics::Tty { keyboard, terminal }) => {
                format!("{keyboard}{terminal}{mnemonic}")
            }
            (Self::UnibyteBlank, FrameCodingMnemonics::WindowSystem) => " ".to_owned(),
            (Self::UnibyteBlank, FrameCodingMnemonics::Tty { .. }) => "   ".to_owned(),
        }
    }
}

/// Derive coding system mnemonic character from coding system name.
///
/// Matches GNU `decode_mode_spec_coding` heuristics for common systems.
fn coding_system_mnemonic_char(
    access: CodingAccess<'_>,
    cs_name: crate::emacs_core::intern::SymId,
) -> char {
    let registered = match access {
        CodingAccess::Registry(manager) => manager.mode_line_mnemonic(cs_name),
        #[cfg(test)]
        CodingAccess::SplitState => None,
    };
    if let Some(mnemonic) = registered {
        return mnemonic;
    }
    let cs_name = crate::emacs_core::intern::resolve_sym(cs_name);
    let base = cs_name
        .strip_suffix("-unix")
        .or_else(|| cs_name.strip_suffix("-dos"))
        .or_else(|| cs_name.strip_suffix("-mac"))
        .unwrap_or(cs_name);
    match base {
        "utf-8"
        | "utf-8-emacs"
        | "utf-8-auto"
        | "mule-utf-8"
        | "utf-16"
        | "utf-16-be"
        | "utf-16-le"
        | "utf-16be"
        | "utf-16le"
        | "utf-16be-with-signature"
        | "utf-16le-with-signature" => 'U',
        "undecided" | "prefer-utf-8" | "nil" => '-',
        "raw-text" => '=',
        "no-conversion" | "binary" => '=',
        "us-ascii" | "ascii" => '.',
        "iso-8859-1" | "iso-latin-1" | "latin-1" => '1',
        "iso-8859-2" | "iso-latin-2" | "latin-2" => '2',
        "iso-8859-3" | "latin-3" => '3',
        "iso-8859-4" | "latin-4" => '4',
        "iso-8859-5" | "latin-5" => '5',
        "iso-2022-jp" | "junet" => 'J',
        "euc-jp" => 'E',
        "shift_jis" | "sjis" => 'S',
        "iso-2022-kr" => 'K',
        "euc-kr" => 'e',
        "gb2312" | "euc-cn" | "cn-gb" => 'C',
        "big5" => 'B',
        _ => '-',
    }
}

/// Derive EOL type indicator from coding system name, using the
/// `eol-mnemonic-*` variables from the obarray (matches GNU semantics).
fn coding_system_eol_indicator_value(
    vars: VariableView<'_>,
    cs_name: crate::emacs_core::intern::SymId,
) -> Option<Value> {
    let cs_name = crate::emacs_core::intern::resolve_sym(cs_name);
    let var_name = if cs_name.ends_with("-dos") {
        "eol-mnemonic-dos"
    } else if cs_name.ends_with("-mac") {
        "eol-mnemonic-mac"
    } else if cs_name.ends_with("-unix") {
        "eol-mnemonic-unix"
    } else {
        "eol-mnemonic-undecided"
    };
    vars.value_by_name(var_name)
        .filter(|value| value.is_string() || value.as_char().is_some())
}
