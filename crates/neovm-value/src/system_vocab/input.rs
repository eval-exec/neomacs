//! Input-path vocabulary shared by the evaluator and the keyboard engines
//! (stage-4 W1/E3: moved from `neovm-core`'s `keyboard` module).
//!
//! These are the option/record types that travel between the Lisp reader,
//! the command loop, the `input-pending-p` query and keyboard-macro scope
//! bookkeeping.  They bind [`Value`] or plain std data only: no evaluator
//! authority, no display, no host state.  Core keeps every historical
//! `crate::keyboard` path as a re-export.

use crate::value::Value;

/// Whether a TTY read returns the bytes supplied by the terminal or characters
/// decoded through `keyboard-coding-system`.
///
/// GNU represents this distinction with the magic `prev_event == t` sentinel
/// passed to `read_char`.  Making it an enum keeps raw Lisp `read-event` reads
/// distinct from command/key-sequence reads at compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyInputDecoding {
    RawBytes,
    KeyboardCodingSystem,
}

/// GNU `read_key_sequence`'s `fix_current_buffer` distinction: the command
/// loop follows the selected window, while Lisp key readers retain the caller's
/// current buffer even when another buffer is displayed.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum KeySequenceBufferSelection {
    CurrentBuffer,
    SelectedWindow,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ReadKeySequenceOptions {
    pub buffer_selection: KeySequenceBufferSelection,
    pub prompt: Value,
    pub dont_downcase_last: bool,
    pub can_return_switch_frame: bool,
    /// GNU `read_key_sequence_vs` CONTINUE-ECHO argument (keyboard.c:11904).
    /// When false (the common case, and the default for the command loop and
    /// for `read-key`), the committed `this-command-keys` of the *previous*
    /// sequence is cleared at entry so the freshly read sequence starts from
    /// scratch (keyboard.c:11919-11923). When true (e.g.
    /// `(read-key-sequence-vector PROMPT t ...)`), the new events are appended
    /// to the existing `this-command-keys` instead.
    pub continue_echo: bool,
}

impl ReadKeySequenceOptions {
    pub fn for_command_loop() -> Self {
        Self {
            buffer_selection: KeySequenceBufferSelection::SelectedWindow,
            can_return_switch_frame: true,
            ..Self::default()
        }
    }

    pub fn new(
        prompt: Value,
        continue_echo: bool,
        dont_downcase_last: bool,
        can_return_switch_frame: bool,
    ) -> Self {
        Self {
            buffer_selection: KeySequenceBufferSelection::CurrentBuffer,
            prompt,
            dont_downcase_last,
            can_return_switch_frame,
            continue_echo,
        }
    }
}

impl Default for ReadKeySequenceOptions {
    fn default() -> Self {
        Self {
            buffer_selection: KeySequenceBufferSelection::CurrentBuffer,
            prompt: Value::NIL,
            dont_downcase_last: false,
            can_return_switch_frame: false,
            continue_echo: false,
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct ExecutingKbdMacroRuntimeSnapshot {
    pub events: Option<Vec<Value>>,
    pub index: usize,
}

/// GNU's two filtering modes for an `input-pending-p` command-input query.
///
/// The Lisp variable does not toggle filtering off. Non-nil uses the full
/// `while-no-input-ignore-events` set; nil still filters focus events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputPendingFilter {
    ConfiguredIgnoreList,
    FocusEventsOnly,
}

impl InputPendingFilter {
    pub const fn from_filter_events_variable(enabled: bool) -> Self {
        if enabled {
            Self::ConfiguredIgnoreList
        } else {
            Self::FocusEventsOnly
        }
    }

    pub fn ignores(self, symbol: &str, ignored_while_no_input: &impl Fn(&str) -> bool) -> bool {
        match self {
            Self::ConfiguredIgnoreList => ignored_while_no_input(symbol),
            Self::FocusEventsOnly => matches!(symbol, "focus-in" | "focus-out"),
        }
    }
}

/// GNU `read_char` distinguishes forced fresh input, ordinary rereads, and
/// events already recorded by Quail (keyboard.c, unread-command-events).
#[derive(Clone, Copy, Debug)]
pub enum UnreadCommandEvent {
    Fresh(Value),
    Reread(Value),
    AlreadyRecorded(Value),
}

impl UnreadCommandEvent {
    pub fn decode(value: Value) -> Self {
        if value.is_cons() {
            let tag = value.cons_car();
            if tag.is_t() {
                return Self::Fresh(value.cons_cdr());
            }
            if tag == Value::symbol("no-record") {
                return Self::AlreadyRecorded(value.cons_cdr());
            }
        }
        Self::Reread(value)
    }

    pub fn event(self) -> Value {
        match self {
            Self::Fresh(event) | Self::Reread(event) | Self::AlreadyRecorded(event) => event,
        }
    }
}
