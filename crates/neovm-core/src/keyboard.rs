//! Keyboard input and command loop.
//!
//! Implements the Emacs command loop:
//! - Key event representation
//! - Key sequence reading
//! - Command dispatch (keymap lookup → funcall)
//! - Interactive command argument parsing
//! - Minibuffer input
//! - Recursive edit support
//! - Pre/post-command hooks
//! - Prefix argument handling

use crate::buffer::{CharLen, CharPos0, CharRange};
use crate::emacs_core::error::LispCondition;
use crate::emacs_core::intern::{intern, resolve_sym};
use crate::emacs_core::keyboard::pure::KEY_CHAR_META;
use crate::emacs_core::keymap::{KeymapMarker, MenuItemProperty};
use crate::emacs_core::wait::CommandInputWaitOutcome;
// decode_storage_char_codes import removed — now using emacs_char directly
use crate::emacs_core::value::{Value, ValueKind, VecLikeType};
use crate::heap_types::LispString;
use std::collections::{HashMap, VecDeque};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum FrontendWebValue {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<FrontendWebValue>),
    Object(std::collections::BTreeMap<String, FrontendWebValue>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrontendWebProcessFailure {
    Crashed,
    ExceededMemoryLimit,
    Terminated,
    Unresponsive,
    LaunchFailed,
    Other(i32),
}

#[derive(Clone, Debug, PartialEq)]
pub enum FrontendWebViewEvent {
    Ready {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
    },
    Failed {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        error: String,
    },
    Closed {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
    },
    TitleChanged {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        title: String,
    },
    UriChanged {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        uri: String,
    },
    LoadProgressChanged {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        progress: f64,
    },
    LoadFinished {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        navigation: Option<u64>,
    },
    ScriptFinished {
        view: neomacs_display_protocol::WebViewId,
        generation: u64,
        request: u64,
        result: Result<FrontendWebValue, String>,
    },
    ProcessFailed {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        failure: FrontendWebProcessFailure,
    },
    FocusChanged {
        id: neomacs_display_protocol::WebViewId,
        generation: u64,
        focused: bool,
    },
}

/// Lisp mouse area retained by an immutable displayed presentation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PresentedMouseArea {
    TabBar,
}

/// Evaluator-owned meaning for one opaque renderer interaction id.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PresentedMouseTarget {
    pub area: PresentedMouseArea,
    pub posn_string: Value,
}

#[derive(Default)]
struct InteractionPresentation {
    targets: Vec<PresentedMouseTarget>,
}

/// Retains the Lisp meaning paired with immutable displayed frame snapshots.
pub struct PresentedInteractions {
    next_presentation: u64,
    presentations: HashMap<u64, InteractionPresentation>,
}

impl Default for PresentedInteractions {
    fn default() -> Self {
        Self {
            next_presentation: 1,
            presentations: HashMap::new(),
        }
    }
}

impl PresentedInteractions {
    pub fn begin(&mut self) -> u64 {
        let id = self.next_presentation;
        self.next_presentation = self.next_presentation.saturating_add(1);
        id
    }

    pub fn register_mouse_target(
        &mut self,
        presentation: u64,
        target: PresentedMouseTarget,
    ) -> u32 {
        let presentation = self.presentations.entry(presentation).or_default();
        let id = u32::try_from(presentation.targets.len())
            .expect("interaction presentation exceeds u32 target capacity");
        presentation.targets.push(target);
        id
    }

    pub fn resolve(&self, presentation: u64, interaction: u32) -> Option<PresentedMouseTarget> {
        self.presentations
            .get(&presentation)?
            .targets
            .get(interaction as usize)
            .copied()
    }

    pub fn retire(&mut self, presentation: u64) {
        self.presentations.remove(&presentation);
    }
}

impl crate::gc_trace::GcTrace for PresentedInteractions {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for presentation in self.presentations.values() {
            roots.extend(presentation.targets.iter().map(|target| target.posn_string));
        }
    }
}

/// Coalesced high-resolution scroll input waiting for redisplay.
///
/// Keeping the frame identity and delta in one value prevents a caller from
/// draining a delta for the wrong frame. A different target replaces the
/// pending gesture; repeated input for one frame preserves sub-pixel precision.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PendingPixelScroll {
    frame: crate::window::FrameId,
    delta_y: f32,
}

impl PendingPixelScroll {
    fn accumulate(current: Option<Self>, frame: crate::window::FrameId, delta_y: f32) -> Self {
        let accumulated = current.map_or(0.0, |pending| {
            if pending.frame == frame {
                pending.delta_y
            } else {
                0.0
            }
        });
        Self {
            frame,
            delta_y: accumulated + delta_y,
        }
    }

    pub const fn for_frame(self, frame: crate::window::FrameId) -> Option<f32> {
        if self.frame.0 == frame.0 {
            Some(self.delta_y)
        } else {
            None
        }
    }
}

// ---------------------------------------------------------------------------
// Key events
// ---------------------------------------------------------------------------

/// Modifier flags for key events.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Modifiers {
    pub ctrl: bool,
    pub meta: bool, // Alt
    pub shift: bool,
    pub super_: bool,
    pub hyper: bool,
}

impl Modifiers {
    pub fn none() -> Self {
        Self::default()
    }

    pub fn ctrl() -> Self {
        Self {
            ctrl: true,
            ..Self::default()
        }
    }

    pub fn meta() -> Self {
        Self {
            meta: true,
            ..Self::default()
        }
    }

    pub fn ctrl_meta() -> Self {
        Self {
            ctrl: true,
            meta: true,
            ..Self::default()
        }
    }

    /// Convert to Emacs modifier bitmask.
    pub fn to_bits(&self) -> u32 {
        use crate::emacs_core::keyboard::pure::{
            KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META, KEY_CHAR_SHIFT, KEY_CHAR_SUPER,
        };
        let mut bits = 0u32;
        if self.ctrl {
            bits |= KEY_CHAR_CTRL as u32;
        }
        if self.meta {
            bits |= KEY_CHAR_META as u32;
        }
        if self.shift {
            bits |= KEY_CHAR_SHIFT as u32;
        }
        if self.super_ {
            bits |= KEY_CHAR_SUPER as u32;
        }
        if self.hyper {
            bits |= KEY_CHAR_HYPER as u32;
        }
        bits
    }

    /// Parse from Emacs modifier bitmask.
    pub fn from_bits(bits: u32) -> Self {
        use crate::emacs_core::keyboard::pure::{
            KEY_CHAR_CTRL, KEY_CHAR_HYPER, KEY_CHAR_META, KEY_CHAR_SHIFT, KEY_CHAR_SUPER,
        };
        Self {
            ctrl: bits & KEY_CHAR_CTRL as u32 != 0,
            meta: bits & KEY_CHAR_META as u32 != 0,
            shift: bits & KEY_CHAR_SHIFT as u32 != 0,
            super_: bits & KEY_CHAR_SUPER as u32 != 0,
            hyper: bits & KEY_CHAR_HYPER as u32 != 0,
        }
    }

    /// Format as Emacs modifier prefix (e.g., "C-M-").
    pub fn prefix_string(&self) -> String {
        let mut s = String::new();
        if self.hyper {
            s.push_str("H-");
        }
        if self.super_ {
            s.push_str("s-");
        }
        if self.ctrl {
            s.push_str("C-");
        }
        if self.meta {
            s.push_str("M-");
        }
        if self.shift {
            s.push_str("S-");
        }
        s
    }

    pub fn is_empty(&self) -> bool {
        !self.ctrl && !self.meta && !self.shift && !self.super_ && !self.hyper
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum SpecialInputServiceActivity {
    #[default]
    None,
    Any,
    Resize,
}

impl SpecialInputServiceActivity {
    fn record(self, activity: Self) -> Self {
        match (self, activity) {
            (Self::Resize, _) | (_, Self::Resize) => Self::Resize,
            (Self::Any, _) | (_, Self::Any) => Self::Any,
            (Self::None, Self::None) => Self::None,
        }
    }

    fn any(self) -> bool {
        matches!(self, Self::Any | Self::Resize)
    }

    fn resize(self) -> bool {
        matches!(self, Self::Resize)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct SpecialInputServiceOutcome {
    redisplay_needed: bool,
    activity: SpecialInputServiceActivity,
}

impl SpecialInputServiceOutcome {
    pub(crate) fn any_activity() -> Self {
        Self {
            redisplay_needed: false,
            activity: SpecialInputServiceActivity::Any,
        }
    }

    pub(crate) fn resize_with_redisplay() -> Self {
        Self {
            redisplay_needed: true,
            activity: SpecialInputServiceActivity::Resize,
        }
    }

    pub(crate) fn merge(self, other: Self) -> Self {
        Self {
            redisplay_needed: self.redisplay_needed || other.redisplay_needed,
            activity: self.activity.record(other.activity),
        }
    }

    pub(crate) fn from_internal_effects(
        effects: crate::frontend_events::InternalEventEffects,
    ) -> Self {
        Self {
            redisplay_needed: effects.redisplay_needed,
            activity: SpecialInputServiceActivity::None,
        }
    }

    pub(crate) fn has_any_activity(self) -> bool {
        self.activity.any()
    }

    pub(crate) fn has_resize_activity(self) -> bool {
        self.activity.resize()
    }

    pub(crate) fn redisplay_needed(self) -> bool {
        self.redisplay_needed
    }
}

/// A single key event (keystroke).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// The base key (character or named key).
    pub key: Key,
    /// Active modifiers.
    pub modifiers: Modifiers,
}

/// The base key of a keystroke.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A character key (e.g., 'a', '1', space).
    Char(char),
    /// A named function key.
    Named(NamedKey),
}

/// Named (non-character) keys.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum NamedKey {
    Return,
    Tab,
    Escape,
    Backspace,
    Delete,
    Insert,
    Home,
    End,
    PageUp,
    PageDown,
    Left,
    Right,
    Up,
    Down,
    F(u8), // F1-F24
}

impl KeyEvent {
    pub fn char(c: char) -> Self {
        Self {
            key: Key::Char(c),
            modifiers: Modifiers::none(),
        }
    }

    pub fn char_with_mods(c: char, mods: Modifiers) -> Self {
        Self {
            key: Key::Char(c),
            modifiers: mods,
        }
    }

    pub fn named(key: NamedKey) -> Self {
        Self {
            key: Key::Named(key),
            modifiers: Modifiers::none(),
        }
    }

    pub fn named_with_mods(key: NamedKey, mods: Modifiers) -> Self {
        Self {
            key: Key::Named(key),
            modifiers: mods,
        }
    }

    /// Convert this host key event into the Lisp-visible Emacs event
    /// representation used by the command loop and keymap lookup.
    pub fn to_emacs_event_value(&self) -> Value {
        let event = crate::emacs_core::keymap::KeyEvent::from(self.clone());
        crate::emacs_core::keymap::key_event_to_emacs_event(&event)
    }

    /// True if this event is GNU Emacs's default `quit-char`: `C-g`.
    ///
    /// Used by the input-bridge thread to set `Context::quit_requested`
    /// without a round-trip through the evaluator. The evaluator's own
    /// `event_is_quit_char` is still consulted in `read_char` to honor
    /// customized `quit-char` values; this helper only catches the
    /// overwhelmingly common default so a blocked bytecode loop can be
    /// interrupted.
    pub fn is_default_quit_char(&self) -> bool {
        if !matches!(self.key, Key::Char('g')) {
            return false;
        }
        let m = self.modifiers;
        m.ctrl && !m.meta && !m.super_ && !m.hyper
    }

    /// Format as Emacs key description (e.g., "C-x", "M-f", "RET").
    pub fn to_description(&self) -> String {
        let emacs_event = self.to_emacs_event_value();
        // A Rust-side label (logs, messages): decode here rather than making
        // the shared builder lossy for the Lisp-facing callers.
        crate::emacs_core::keyboard::pure::describe_single_key_value(&emacs_event, false)
            .map(|bytes| crate::emacs_core::emacs_char::to_utf8_lossy(&bytes))
            .unwrap_or_else(|_| format!("{:?}", emacs_event))
    }

    /// Parse an Emacs key description (e.g., "C-x", "M-f").
    pub fn from_description(desc: &str) -> Option<Self> {
        let encoded = crate::emacs_core::kbd::parse_kbd_string(desc).ok()?;
        let events = crate::emacs_core::kbd::key_events_from_designator(&encoded).ok()?;
        let [event] = events.as_slice() else {
            return None;
        };
        Self::from_emacs_key_event(event.clone())
    }

    fn from_emacs_key_event(event: crate::emacs_core::keymap::KeyEvent) -> Option<Self> {
        match event {
            crate::emacs_core::keymap::KeyEvent::Char {
                code,
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            } => {
                if alt {
                    return None;
                }
                // GNU's `kbd` parser collapses `C-x` into raw control
                // codepoint U+0018 with `ctrl=false` (mirroring elisp
                // `?\C-x => 24`). Reverse that here so terminal-level
                // KeyEvents still carry a `ctrl` modifier for ASCII
                // letters — otherwise `(C-x).key` reads as char 0x18
                // and rendering / matching breaks.
                let (code, ctrl) = if !ctrl
                    && (code as u32) < 0x20
                    && code != '\r'
                    && code != '\t'
                    && code != '\u{1b}'
                    && code != '\u{7f}'
                {
                    let lowered = ((code as u8) | 0x60) as char;
                    if lowered.is_ascii_alphabetic() {
                        (lowered, true)
                    } else {
                        (code, false)
                    }
                } else {
                    (code, ctrl)
                };
                let key = match code {
                    '\r' => Key::Named(NamedKey::Return),
                    '\t' => Key::Named(NamedKey::Tab),
                    '\u{1b}' => Key::Named(NamedKey::Escape),
                    '\u{7f}' => Key::Named(NamedKey::Backspace),
                    other => Key::Char(other),
                };
                Some(KeyEvent {
                    key,
                    modifiers: Modifiers {
                        ctrl,
                        meta,
                        shift,
                        super_,
                        hyper,
                    },
                })
            }
            crate::emacs_core::keymap::KeyEvent::Function {
                name,
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            } => {
                if alt {
                    return None;
                }
                let key = match resolve_sym(name) {
                    "return" => Key::Named(NamedKey::Return),
                    "tab" => Key::Named(NamedKey::Tab),
                    "escape" => Key::Named(NamedKey::Escape),
                    "backspace" => Key::Named(NamedKey::Backspace),
                    "delete" => Key::Named(NamedKey::Delete),
                    "insert" => Key::Named(NamedKey::Insert),
                    "home" => Key::Named(NamedKey::Home),
                    "end" => Key::Named(NamedKey::End),
                    "prior" => Key::Named(NamedKey::PageUp),
                    "next" => Key::Named(NamedKey::PageDown),
                    "left" => Key::Named(NamedKey::Left),
                    "right" => Key::Named(NamedKey::Right),
                    "up" => Key::Named(NamedKey::Up),
                    "down" => Key::Named(NamedKey::Down),
                    other if other.starts_with('f') => {
                        let num = other.strip_prefix('f')?.parse::<u8>().ok()?;
                        Key::Named(NamedKey::F(num))
                    }
                    _ => return None,
                };
                Some(KeyEvent {
                    key,
                    modifiers: Modifiers {
                        ctrl,
                        meta,
                        shift,
                        super_,
                        hyper,
                    },
                })
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Key sequence
// ---------------------------------------------------------------------------

/// A sequence of key events forming a complete key binding.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KeySequence {
    pub events: Vec<KeyEvent>,
}

impl KeySequence {
    pub fn new() -> Self {
        Self { events: Vec::new() }
    }

    pub fn single(event: KeyEvent) -> Self {
        Self {
            events: vec![event],
        }
    }

    pub fn push(&mut self, event: KeyEvent) {
        self.events.push(event);
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Format as Emacs key sequence description.
    pub fn to_description(&self) -> String {
        self.events
            .iter()
            .map(|e| e.to_description())
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Parse an Emacs key sequence description (e.g., "C-x C-f").
    pub fn from_description(desc: &str) -> Option<Self> {
        let encoded = crate::emacs_core::kbd::parse_kbd_string(desc).ok()?;
        let emacs_events = crate::emacs_core::kbd::key_events_from_designator(&encoded).ok()?;
        let events = emacs_events
            .into_iter()
            .map(KeyEvent::from_emacs_key_event)
            .collect::<Option<Vec<_>>>()?;
        Some(Self { events })
    }
}

impl Default for KeySequence {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ReadKeySequenceState {
    raw_events: Vec<Value>,
    translated_events: Vec<Value>,
}

impl ReadKeySequenceState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        self.raw_events.clear();
        self.translated_events.clear();
    }

    pub fn push_input_event(&mut self, event: Value) {
        self.raw_events.push(event);
        self.translated_events.push(event);
    }

    pub fn replace_translated_events(&mut self, events: Vec<Value>) {
        self.translated_events = events;
    }

    pub fn replace_events(&mut self, events: Vec<Value>) {
        self.raw_events = events.clone();
        self.translated_events = events;
    }

    pub fn raw_events(&self) -> &[Value] {
        &self.raw_events
    }

    pub fn translated_events(&self) -> &[Value] {
        &self.translated_events
    }

    pub fn snapshot(&self) -> (Vec<Value>, Vec<Value>) {
        (self.translated_events.clone(), self.raw_events.clone())
    }

    /// Remove the last raw and translated event. Used by the
    /// help-char dispatch path in `read_key_sequence` to strip
    /// the help event from the sequence before running
    /// `prefix-help-command`, matching GNU
    /// `keyboard.c:10220-10230` which discards the help event so
    /// `(this-command-keys)` reports the prefix only.
    pub fn pop_last_events_for_help_char(&mut self) {
        self.raw_events.pop();
        self.translated_events.pop();
    }
}

#[derive(Clone, Copy, Debug)]
struct ReadCharEvent {
    event: Value,
    allow_input_method: bool,
    command_key_recording: CommandKeyRecording,
}

/// Whether a TTY read returns the bytes supplied by the terminal or characters
/// decoded through `keyboard-coding-system`.
///
/// GNU represents this distinction with the magic `prev_event == t` sentinel
/// passed to `read_char`.  Making it an enum keeps raw Lisp `read-event` reads
/// distinct from command/key-sequence reads at compile time.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TtyInputDecoding {
    RawBytes,
    KeyboardCodingSystem,
}

#[derive(Clone, Copy, Debug)]
enum CommandKeyRecording {
    Append,
    AppendIfEmpty,
}

impl ReadCharEvent {
    fn fresh_input_method_candidate(event: Value) -> Self {
        Self {
            event,
            allow_input_method: true,
            command_key_recording: CommandKeyRecording::Append,
        }
    }

    fn reread_input_method_candidate(event: Value) -> Self {
        Self {
            event,
            allow_input_method: true,
            command_key_recording: CommandKeyRecording::AppendIfEmpty,
        }
    }

    fn post_input_method(event: Value) -> Self {
        Self {
            event,
            allow_input_method: false,
            command_key_recording: CommandKeyRecording::AppendIfEmpty,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum InputMethodEvent {
    NotApplied,
    Consumed,
    Translated(Value),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ReadKeySequenceOptions {
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

/// A command-loop key read either resolves one complete command or reaches a
/// typed input boundary.  Keeping the boundary distinct from an ordinary
/// `(empty, nil)` tuple prevents callers from accidentally dispatching macro
/// exhaustion as an undefined key sequence.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum CommandKeySequenceRead {
    Command { keys: Vec<Value>, binding: Value },
    End(CommandKeySequenceEnd),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandKeySequenceEnd {
    /// GNU `read_key_sequence` returned zero after `at_end_of_macro_p`.
    KeyboardMacroIteration,
    /// The evaluator has no command-input source left to read.
    Input,
}

impl ReadKeySequenceOptions {
    pub(crate) fn new(
        prompt: Value,
        continue_echo: bool,
        dont_downcase_last: bool,
        can_return_switch_frame: bool,
    ) -> Self {
        Self {
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
            prompt: Value::NIL,
            dont_downcase_last: false,
            can_return_switch_frame: false,
            continue_echo: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct KeySequenceSuffixTranslation {
    start: usize,
    replacement: Vec<Value>,
}

#[derive(Clone, Debug, PartialEq)]
struct CurrentKeySequenceTranslation {
    translated_events: Vec<Value>,
    has_pending_translation_prefix: bool,
    application: KeySequenceTranslationApplication,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeySequenceTranslationApplication {
    NotApplied,
    Applied,
}

#[derive(Clone, Copy, Debug)]
enum CommandBindingResolution {
    Current(crate::emacs_core::keymap::ActiveKeyBindingResolution),
    Stale,
}

impl CommandBindingResolution {
    fn invalidate(&mut self) {
        *self = Self::Stale;
    }

    fn invalidate_if_applied(&mut self, application: KeySequenceTranslationApplication) {
        if application == KeySequenceTranslationApplication::Applied {
            self.invalidate();
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
struct KeySequenceShiftTranslation {
    index: usize,
    original_event: Value,
}

#[derive(Clone, Debug)]
enum UndefinedMouseSequenceFallback {
    Rewrite {
        events: Vec<Value>,
        resolved: crate::emacs_core::keymap::ActiveKeyBindingResolution,
    },
    Drop {
        retained_events: Vec<Value>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MouseEventFallbackStep {
    Rewrite,
    Drop,
}

// ---------------------------------------------------------------------------
// Keysym conversion (X11/winit keysyms → neovm-core KeyEvent)
// ---------------------------------------------------------------------------

// X11 keysym constants used by the render thread (winit) and TTY frontend.
pub const XK_RETURN: u32 = 0xFF0D;
pub const XK_TAB: u32 = 0xFF09;
pub const XK_BACKSPACE: u32 = 0xFF08;
pub const XK_DELETE: u32 = 0xFFFF;
pub const XK_ESCAPE: u32 = 0xFF1B;
pub const XK_LEFT: u32 = 0xFF51;
pub const XK_UP: u32 = 0xFF52;
pub const XK_RIGHT: u32 = 0xFF53;
pub const XK_DOWN: u32 = 0xFF54;
pub const XK_HOME: u32 = 0xFF50;
pub const XK_END: u32 = 0xFF57;
pub const XK_PAGE_UP: u32 = 0xFF55;
pub const XK_PAGE_DOWN: u32 = 0xFF56;
pub const XK_INSERT: u32 = 0xFF63;
pub const XK_F1: u32 = 0xFFBE;
pub const XK_F24: u32 = 0xFFD5;

// Render thread modifier bitmask constants.
pub const RENDER_SHIFT_MASK: u32 = 1 << 0;
pub const RENDER_CTRL_MASK: u32 = 1 << 1;
pub const RENDER_META_MASK: u32 = 1 << 2;
pub const RENDER_SUPER_MASK: u32 = 1 << 3;

/// Meaning of a character-shaped frontend event before it enters Emacs's
/// command loop.
///
/// A produced text character has already consumed Shift (for example,
/// `Shift+a` produced `A`).  A command chord still needs GNU's
/// `make_lispy_event` case/modifier cooking.  Control chords are distinct
/// because `make_ctrl_char` must retain Shift for letters: `C-S-f` and `C-f`
/// are different Emacs events even though both have control code 6.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FrontendCharacterInput {
    Text {
        produced: char,
        shift_pressed: bool,
    },
    NonControlChord {
        produced: char,
        modifiers: NonControlChordModifiers,
    },
    ControlChord {
        produced: char,
        modifiers: ControlChordModifiers,
    },
}

/// Modifier state whose construction proves that Control is not active but
/// another command modifier is.  Keeping this separate from a control chord
/// makes Shift cooking exhaustive at compile time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct NonControlChordModifiers(Modifiers);

/// Modifier state whose construction proves that Control is active.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct ControlChordModifiers(Modifiers);

impl FrontendCharacterInput {
    fn classify(produced: char, modifiers: Modifiers) -> Self {
        if modifiers.ctrl {
            Self::ControlChord {
                produced,
                modifiers: ControlChordModifiers(modifiers),
            }
        } else if modifiers.meta || modifiers.super_ || modifiers.hyper {
            Self::NonControlChord {
                produced,
                modifiers: NonControlChordModifiers(modifiers),
            }
        } else {
            Self::Text {
                produced,
                shift_pressed: modifiers.shift,
            }
        }
    }

    fn into_key_event(self) -> KeyEvent {
        match self {
            Self::Text {
                produced,
                shift_pressed,
            } => KeyEvent::char_with_mods(
                produced,
                Modifiers {
                    // GNU distinguishes S-SPC even though ordinary shifted
                    // text consumes Shift into the produced character.
                    shift: shift_pressed && produced == ' ',
                    ..Modifiers::none()
                },
            ),
            Self::NonControlChord {
                produced,
                modifiers: NonControlChordModifiers(mut modifiers),
            } => {
                let produced = normalize_command_character_case(produced, modifiers.shift);
                // For non-control chords the character's case/punctuation
                // carries Shift.  Space is GNU's explicit exception.
                modifiers.shift = modifiers.shift && produced == ' ';
                KeyEvent::char_with_mods(produced, modifiers)
            }
            Self::ControlChord {
                produced,
                modifiers: ControlChordModifiers(mut modifiers),
            } => {
                let mut produced = normalize_command_character_case(produced, modifiers.shift);
                // GNU make_ctrl_char converts an uppercase ASCII letter to
                // the same control code as its lowercase form and records
                // the lost case information in CHAR_SHIFT.
                let shifted_control_letter = produced.is_ascii_uppercase();
                if shifted_control_letter {
                    produced = produced.to_ascii_lowercase();
                }
                modifiers.shift = modifiers.shift && (shifted_control_letter || produced == ' ');
                KeyEvent::char_with_mods(produced, modifiers)
            }
        }
    }
}

/// Apply GNU's Caps-Lock-safe command-chord case rule from
/// `src/keyboard.c:make_lispy_event`.
fn normalize_command_character_case(character: char, shift_pressed: bool) -> char {
    if character.is_ascii_uppercase() && !shift_pressed {
        character.to_ascii_lowercase()
    } else if character.is_ascii_lowercase() && shift_pressed {
        character.to_ascii_uppercase()
    } else {
        character
    }
}

/// Convert frontend render/TTY modifier bits into the core modifier model.
pub fn render_modifiers_to_modifiers(bits: u32) -> Modifiers {
    Modifiers {
        ctrl: bits & RENDER_CTRL_MASK != 0,
        meta: bits & RENDER_META_MASK != 0,
        shift: bits & RENDER_SHIFT_MASK != 0,
        super_: bits & RENDER_SUPER_MASK != 0,
        hyper: false,
    }
}

/// Convert frontend key transport facts into the core input event model.
///
/// Key releases are ignored here so the command loop only sees the GNU-like
/// cooked keypress stream.
pub fn render_key_transport_to_input_event(
    keysym: u32,
    modifiers: u32,
    pressed: bool,
    emacs_frame_id: u64,
) -> Option<InputEvent> {
    if !pressed {
        return None;
    }

    let key_event = keysym_to_key_event(keysym, modifiers)?;
    Some(InputEvent::key_press_in_frame(key_event, emacs_frame_id))
}

/// Convert a raw keysym and modifier bitmask (from the render thread) into
/// a neovm-core `KeyEvent`.
///
/// Returns `None` for keysyms that should be ignored (modifier-only keys,
/// unknown keysyms, etc.).
pub fn keysym_to_key_event(keysym: u32, modifiers: u32) -> Option<KeyEvent> {
    let mut mods = render_modifiers_to_modifiers(modifiers);

    let key = match keysym {
        // Raw TTY ESC is GNU's `meta-prefix-char` character, not the named
        // GUI Escape function key.
        0x1B => Key::Char('\u{1b}'),
        // Raw TTY DEL is an ASCII key event in GNU's tty_read_avail_input.
        // Backends that know they saw a physical Backspace key send XK_BACKSPACE.
        0x7F => Key::Char('\u{7f}'),
        // Control characters (Ctrl + letter): winit gives us the control
        // character (0x01-0x1A) as the keysym when Ctrl is held.  Convert
        // back to the corresponding letter and force the ctrl modifier.
        0x01..=0x1A => {
            let ch = (keysym + 0x60) as u8 as char; // 0x18 → 'x'
            mods.ctrl = true;
            Key::Char(ch)
        }
        // Printable ASCII
        0x20..=0x7E => Key::Char(keysym as u8 as char),
        // Raw TTY bytes that are control characters (and thus excluded by
        // the `!ch.is_control()` guard in the catch-all below).  Emitted as
        // Key::Char so they produce the same fixnum events GNU creates in
        // tty_read_avail_input (buf.code = cbuf[i]).
        0x00 => Key::Char('\0'),
        0x1C..=0x1F | 0x80..=0xFF => Key::Char(char::from_u32(keysym).unwrap()),
        // Named keys
        XK_RETURN => Key::Named(NamedKey::Return),
        XK_TAB => Key::Named(NamedKey::Tab),
        XK_BACKSPACE => Key::Named(NamedKey::Backspace),
        XK_DELETE => Key::Named(NamedKey::Delete),
        XK_ESCAPE => Key::Named(NamedKey::Escape),
        XK_LEFT => Key::Named(NamedKey::Left),
        XK_RIGHT => Key::Named(NamedKey::Right),
        XK_UP => Key::Named(NamedKey::Up),
        XK_DOWN => Key::Named(NamedKey::Down),
        XK_HOME => Key::Named(NamedKey::Home),
        XK_END => Key::Named(NamedKey::End),
        XK_PAGE_UP => Key::Named(NamedKey::PageUp),
        XK_PAGE_DOWN => Key::Named(NamedKey::PageDown),
        XK_INSERT => Key::Named(NamedKey::Insert),
        // Function keys F1-F24
        k if (XK_F1..=XK_F24).contains(&k) => Key::Named(NamedKey::F((k - XK_F1 + 1) as u8)),
        // Printable Unicode scalar values from TTY or GUI backends.
        k if char::from_u32(k).is_some_and(|ch| !ch.is_control()) => {
            Key::Char(char::from_u32(k).unwrap())
        }
        // Ignore modifier-only keys and unknown keysyms
        _ => return None,
    };

    Some(match key {
        Key::Char(character) => FrontendCharacterInput::classify(character, mods).into_key_event(),
        Key::Named(named) => KeyEvent::named_with_mods(named, mods),
    })
}

// ---------------------------------------------------------------------------
// Input event types
// ---------------------------------------------------------------------------

/// Stable owner used to route bytes from a text terminal.
///
/// The primary frontend historically used frame id zero to mean "whichever
/// frame is selected now".  Additional terminals instead identify the
/// terminal itself: their displayed top frame can change after the input
/// thread is created, so baking the opening frame id into that thread is
/// incorrect.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TtyInputTarget {
    SelectedFrame,
    Frame(crate::window::FrameId),
    Terminal(u64),
}

impl TtyInputTarget {
    fn from_frontend_frame_id(emacs_frame_id: u64) -> Self {
        if emacs_frame_id == 0 {
            Self::SelectedFrame
        } else {
            Self::Frame(crate::window::FrameId(emacs_frame_id))
        }
    }
}

/// Input events from the display layer.
#[derive(Clone, Debug)]
pub enum InputEvent {
    /// Uninterpreted bytes from a Unix TTY.
    ///
    /// The evaluator expands this transport batch into ordered
    /// [`Self::TtyByte`] events.  The active read policy then either preserves
    /// each byte or decodes it through the selected keyboard coding system.
    RawTtyBytes {
        bytes: Vec<u8>,
        target: TtyInputTarget,
    },
    /// One still-undecoded byte produced by expanding a
    /// [`Self::RawTtyBytes`] transport batch.
    ///
    /// Keeping bytes queued in this form lets each read decide whether to
    /// preserve or decode them, like GNU's terminal event queue.
    TtyByte { byte: u8, target: TtyInputTarget },
    /// One character produced by decoding one or more [`Self::TtyByte`]
    /// events through `keyboard-coding-system`.
    /// This evaluator-internal event keeps decoded characters in the same
    /// ordered frontend queue as every other input fact.
    TtyCharacter {
        character: crate::emacs_core::emacs_char::EmacsChar,
        target: TtyInputTarget,
    },
    /// Keyboard key press.
    KeyPress { key: KeyEvent, emacs_frame_id: u64 },
    /// Mouse button press.
    MousePress {
        button: MouseButton,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        target_frame_id: u64,
    },
    /// Mouse button release.
    MouseRelease {
        button: MouseButton,
        x: f32,
        y: f32,
        target_frame_id: u64,
    },
    /// Mouse movement.
    MouseMove {
        x: f32,
        y: f32,
        modifiers: Modifiers,
        target_frame_id: u64,
    },
    /// Semantic pointer observation resolved against the displayed presentation.
    PresentedRegion {
        presentation: u64,
        hit: Option<neomacs_display_protocol::PresentedHit>,
        x: f32,
        y: f32,
        target_frame_id: u64,
    },
    /// Mouse scroll.
    MouseScroll {
        delta_x: f32,
        delta_y: f32,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        target_frame_id: u64,
    },
    /// Trackpad pixel-precise scroll (smooth scrolling, Phase 1). Unlike
    /// `MouseScroll` (which becomes a wheel event handled by elisp `mwheel`),
    /// this is accumulated and applied as a sub-line `vscroll` adjustment by the
    /// layout pass (`Engine::pixel_scroll_window`).
    PixelScroll {
        delta_x: f32,
        delta_y: f32,
        x: f32,
        y: f32,
        modifiers: Modifiers,
        target_frame_id: u64,
    },
    /// A display dependency changed and evaluator layout must be republished.
    LayoutInvalidated,
    /// Renderer image-cache lifecycle changed for a stable image identity.
    ImageStateChanged {
        event: crate::emacs_core::image_catalog::ImageStateEvent,
    },
    /// Popup menu selection.  The display layer reports the selected
    /// zero-based item index; -1 means the menu was cancelled.
    MenuSelection { index: i32 },
    /// Tool-bar item click.  The display layer reports the zero-based
    /// index in the current rendered tool-bar item vector.
    ToolBarClick { index: i32, emacs_frame_id: u64 },
    /// Pointer observation resolved against the immutable displayed presentation.
    PresentedPointer {
        presentation: u64,
        interaction: u32,
        pressed: bool,
        button: u8,
        x: f32,
        y: f32,
        emacs_frame_id: u64,
    },
    /// Renderer installed this presentation as its drawing and hit-test source.
    PresentationActivated {
        presentation: u64,
        emacs_frame_id: u64,
    },
    /// Renderer rejected or superseded this presentation before activation.
    PresentationDiscarded {
        presentation: u64,
        emacs_frame_id: u64,
    },
    /// Renderer no longer displays or generates hits for this presentation.
    PresentationRetired { presentation: u64 },
    /// Menu-bar item click.  `key` is the exact rendered top-level menu key;
    /// x/y and anchor fields are geometry for legacy Lisp and native popup
    /// placement.
    MenuBarClick {
        index: i32,
        key: String,
        menu_x: f32,
        menu_y: f32,
        anchor_x: f32,
        anchor_y: f32,
        anchor_width: f32,
        anchor_height: f32,
        emacs_frame_id: u64,
    },
    /// Window resize.
    Resize {
        width: u32,
        height: u32,
        scale_factor: f64,
        emacs_frame_id: u64,
    },
    /// Window focus change.
    Focus { focused: bool, emacs_frame_id: u64 },
    /// Monitor configuration changed.
    MonitorsChanged {
        monitors: Vec<crate::emacs_core::builtins::NeomacsMonitorInfo>,
    },
    /// Window-selection change.
    SelectWindow { window_id: crate::window::WindowId },
    /// Window-manager close request.
    WindowClose { emacs_frame_id: u64 },
    /// The display rebuilt its GPU state after a device loss. Renderer-side
    /// media objects are gone: the display host must re-resolve them and a
    /// full redisplay must be forced.
    DisplayReset,
    /// Backend-neutral browser state delivered by the frontend service.
    WebView(FrontendWebViewEvent),
    /// A shader surface failed to build on the render thread after naga
    /// pre-validation accepted it. Runs `neomacs-surface-error-functions`
    /// with the surface id and the renderer's error string.
    SurfaceCreateFailed { id: u32, error: String },
    /// A full-frame shader failed to build on the render thread after
    /// evaluator-side validation accepted it.
    FrameShaderFailed { error: String },
    /// A compositor-owned neo-term failed to create after reserving its ID.
    TerminalCreateFailed {
        id: crate::emacs_core::display_host::TerminalId,
        error: String,
    },
    /// A compositor-owned neo-term child process exited.
    TerminalExited {
        id: crate::emacs_core::display_host::TerminalId,
    },
    /// A compositor-owned neo-term published a new title.
    TerminalTitleChanged {
        id: crate::emacs_core::display_host::TerminalId,
        title: String,
    },
}

impl InputEvent {
    pub fn raw_tty_bytes(bytes: Vec<u8>, emacs_frame_id: u64) -> Self {
        Self::RawTtyBytes {
            bytes,
            target: TtyInputTarget::from_frontend_frame_id(emacs_frame_id),
        }
    }

    pub fn raw_tty_bytes_for_terminal(bytes: Vec<u8>, terminal_id: u64) -> Self {
        Self::RawTtyBytes {
            bytes,
            target: TtyInputTarget::Terminal(terminal_id),
        }
    }

    pub fn key_press(key: KeyEvent) -> Self {
        Self::KeyPress {
            key,
            emacs_frame_id: 0,
        }
    }

    pub fn key_press_in_frame(key: KeyEvent, emacs_frame_id: u64) -> Self {
        Self::KeyPress {
            key,
            emacs_frame_id,
        }
    }

    /// Whether this transport fact contains GNU Emacs's default quit input,
    /// `C-g`, and should wake an evaluator that is busy outside `read_char`.
    pub fn requests_default_quit(&self) -> bool {
        match self {
            Self::RawTtyBytes { bytes, .. } => bytes.contains(&0x07),
            Self::TtyByte { byte, .. } => *byte == 0x07,
            Self::TtyCharacter { character, .. } => character.code() == 0x07,
            Self::KeyPress { key, .. } => key.is_default_quit_char(),
            _ => false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Button4,
    Button5,
}

// ---------------------------------------------------------------------------
// Prefix argument
// ---------------------------------------------------------------------------

/// The current prefix argument state.
#[derive(Clone, Debug, PartialEq)]
pub enum PrefixArg {
    /// No prefix argument.
    None,
    /// Numeric prefix (e.g., C-u 4, M-3).
    Numeric(i64),
    /// Raw prefix (C-u without number).
    Raw(i32), // number of C-u presses: 1 = (4), 2 = (16), etc.
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MousePixelPositionState {
    pub frame_id: Option<crate::window::FrameId>,
    pub x: i64,
    pub y: i64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PresentedMouseObservation {
    pub presentation: u64,
    pub hit: Option<neomacs_display_protocol::PresentedHit>,
    pub x: f32,
    pub y: f32,
    pub frame_id: u64,
}

impl PrefixArg {
    /// Convert to Lisp value for `current-prefix-arg`.
    pub fn to_value(&self) -> Value {
        match self {
            PrefixArg::None => Value::NIL,
            PrefixArg::Numeric(n) => Value::fixnum(*n),
            PrefixArg::Raw(n) => {
                let val = 4i64.pow(*n as u32);
                Value::list(vec![Value::fixnum(val)])
            }
        }
    }

    /// Numeric value (for commands that use the prefix as a count).
    pub fn numeric_value(&self) -> i64 {
        match self {
            PrefixArg::None => 1,
            PrefixArg::Numeric(n) => *n,
            PrefixArg::Raw(n) => 4i64.pow(*n as u32),
        }
    }
}

// ---------------------------------------------------------------------------
// Command loop state
// ---------------------------------------------------------------------------

/// Ownership of the echo-area message by GNU's keyboard echo machinery.
///
/// `immediate_echo` in GNU `keyboard.c` is not just a rendering preference:
/// once the idle delay has elapsed, each subsequently read event clears the
/// old cells and immediately rebuilds the complete pending key sequence.  A
/// closed state keeps that lifecycle distinct from ordinary Lisp messages.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
enum KeyEchoState {
    #[default]
    Inactive,
    Immediate {
        /// Original Lisp prompt text, including interval properties.  GNU's
        /// kboard stores `echo_prompt` as a Lisp string rather than flattening
        /// it to bytes, so rebuilding a prefix echo must retain faces and
        /// other display properties.
        prompt: Option<LispString>,
    },
}

/// Semantic pieces of a keyboard-owned echo message.
///
/// Keeping key names distinct from prose makes it impossible to change the
/// wording while accidentally retaining GNU's old numeric property offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum KeyEchoSegment {
    Plain(&'static str),
    KeyBinding(&'static str),
}

impl KeyEchoSegment {
    const fn text(self) -> &'static str {
        match self {
            Self::Plain(text) | Self::KeyBinding(text) => text,
        }
    }
}

/// Build GNU `echo_add_key`'s help-event suffix with its semantic key names
/// carrying the `help-key-binding` face.
fn help_event_echo_suffix() -> LispString {
    const SEGMENTS: [KeyEchoSegment; 5] = [
        KeyEchoSegment::Plain(" (Type "),
        KeyEchoSegment::KeyBinding("?"),
        KeyEchoSegment::Plain(" for further options, "),
        KeyEchoSegment::KeyBinding("C-q"),
        KeyEchoSegment::Plain(" for quick help)"),
    ];

    let mut text = String::new();
    let mut key_binding_ranges = Vec::new();
    for segment in SEGMENTS {
        let start = CharPos0::new(text.chars().count());
        text.push_str(segment.text());
        if matches!(segment, KeyEchoSegment::KeyBinding(_)) {
            key_binding_ranges.push(CharRange::from_start_len(
                start,
                CharLen::new(segment.text().chars().count()),
            ));
        }
    }

    let mut suffix = LispString::from_utf8(&text);
    for range in key_binding_ranges {
        suffix.intervals_mut().put_property_in_char_range(
            range,
            Value::symbol("face"),
            Value::symbol("help-key-binding"),
        );
    }
    suffix
}

/// Keyboard-local state owned by the active terminal/keyboard.
///
/// GNU Emacs keeps unread events, command-key history, translation maps, and
/// keyboard-macro playback on `kboard` state. NeoVM still has one active
/// keyboard, but it now models that owner explicitly.
pub struct KBoard {
    /// Stateful decoder for raw byte batches from this terminal.
    tty_input_decoder: crate::keyboard_input::KeyboardInputDecoder,
    /// Deferred switch-frame/select-window event that should be delivered
    /// before ordinary unread input, matching GNU
    /// `unread_switch_frame` plus read-key-sequence delayed selection events.
    pub unread_selection_event: Option<Value>,
    /// Last frame observed by `internal-handle-focus-in`, matching GNU
    /// `internal_last_event_frame`.
    pub internal_last_event_frame: Option<crate::window::FrameId>,
    /// Last known mouse position in frame pixel coordinates.
    pub mouse_pixel_position: Option<MousePixelPositionState>,
    /// Last immutable semantic hit paired with the raw pointer coordinates.
    pub presented_mouse_observation: Option<PresentedMouseObservation>,
    /// Last queued internal `help-echo` event for deduping mouse-motion help.
    pub last_help_echo_event: Option<Value>,
    /// Unread command events in the Lisp-visible Emacs event form.
    pub unread_events: VecDeque<Value>,
    /// Current raw/translated key sequence being accumulated by `read_key_sequence`.
    pub current_key_sequence: ReadKeySequenceState,
    /// Last translated key sequence read by the command loop or `read-key*`.
    pub command_keys: Vec<Value>,
    /// Raw key sequence before translation maps, for GNU
    /// `this-single-command-raw-keys`.
    pub raw_command_keys: Vec<Value>,
    /// Recent input history published through `recent-keys`.
    pub recent_input_events: Vec<Value>,
    /// Whether the current echo-area message is owned by keyboard echoing.
    key_echo_state: KeyEchoState,
    /// Terminal-local `input-decode-map`.
    input_decode_map: Value,
    /// Terminal-local `local-function-key-map`.
    local_function_key_map: Value,
    /// Defining keyboard macro (if any).
    pub defining_kbd_macro: bool,
    /// Whether the current definition is appending to the prior macro.
    pub appending_kbd_macro: bool,
    /// Keyboard macro buffer being defined, as Lisp-visible Emacs events.
    pub kbd_macro_events: Vec<Value>,
    /// Finalized prefix of `kbd_macro_events` that belongs to completed commands.
    pub kbd_macro_end: usize,
    /// The last completed keyboard macro, matching GNU `last-kbd-macro`.
    pub last_kbd_macro: Option<Vec<Value>>,
    /// Keyboard macro being executed, as Lisp-visible Emacs events.
    pub executing_kbd_macro: Option<Vec<Value>>,
    /// Index into executing keyboard macro.
    pub kbd_macro_index: usize,
    /// Number of successful iterations for the innermost executing macro.
    pub executing_kbd_macro_iterations: usize,
    /// Open dribble file handle. GNU
    /// `src/keyboard.c:64 (FILE *dribble)` is the global file
    /// handle that `open-dribble-file` opens and
    /// `record_input_event` writes to. Keyboard audit Finding 11.
    dribble: Option<std::fs::File>,
    /// Recursion guard for `input-method-function`. GNU
    /// `keyboard.c` uses the `immediate_echo` flag to suppress
    /// re-entry; we use a dedicated bool so an input-method that
    /// calls `read-event` recursively does not re-translate the
    /// character it was given. Keyboard audit Finding 10.
    pub in_input_method_function: bool,
    /// Last mouse down event seen by the event-ingest path. GNU
    /// `keyboard.c:6041-6130` (`make_lispy_event` / the
    /// `button_down_time` / `last_mouse_click` globals) tracks
    /// the previous click's button, position, frame, and
    /// timestamp so it can compute the click count for
    /// `double-click-time` / `double-click-fuzz` comparisons.
    /// Keyboard audit Finding 12.
    pub last_mouse_click: Option<LastMouseClick>,
}

/// Snapshot of the most recent mouse click used for double/triple
/// click detection. Mirrors the GNU `button_down_info` bundle.
/// Keyboard audit Finding 12.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LastMouseClick {
    pub button: MouseButton,
    pub x: f32,
    pub y: f32,
    pub frame_id: u64,
    pub timestamp: std::time::Instant,
    /// Sequential click count: 1 = single, 2 = double, 3 = triple.
    /// Reset to 1 whenever a click falls outside
    /// `double-click-time` / `double-click-fuzz` of the previous
    /// click. Capped at 3.
    pub click_count: u32,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct ExecutingKbdMacroRuntimeSnapshot {
    pub events: Option<Vec<Value>>,
    pub index: usize,
}

impl KBoard {
    pub fn new() -> Self {
        Self {
            tty_input_decoder: crate::keyboard_input::KeyboardInputDecoder::default(),
            unread_selection_event: None,
            internal_last_event_frame: None,
            mouse_pixel_position: None,
            presented_mouse_observation: None,
            last_help_echo_event: None,
            unread_events: VecDeque::new(),
            current_key_sequence: ReadKeySequenceState::new(),
            command_keys: Vec::new(),
            raw_command_keys: Vec::new(),
            recent_input_events: Vec::new(),
            key_echo_state: KeyEchoState::Inactive,
            input_decode_map: Value::NIL,
            local_function_key_map: Value::NIL,
            defining_kbd_macro: false,
            appending_kbd_macro: false,
            kbd_macro_events: Vec::new(),
            kbd_macro_end: 0,
            last_kbd_macro: None,
            executing_kbd_macro: None,
            kbd_macro_index: 0,
            executing_kbd_macro_iterations: 0,
            dribble: None,
            in_input_method_function: false,
            last_mouse_click: None,
        }
    }

    /// Open the dribble file at PATH for input event logging.
    /// Closes any previously open file. Mirrors GNU
    /// `Fopen_dribble_file` (`src/keyboard.c:12327-12367`):
    ///
    ///   if (dribble) fclose (dribble);
    ///   dribble = fopen (file, "w");
    ///   if (! dribble) report_file_error ("Opening dribble", file);
    ///
    /// Keyboard audit Finding 11.
    pub fn open_dribble_file(&mut self, path: &std::path::Path) -> std::io::Result<()> {
        self.close_dribble_file();
        let file = std::fs::OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)?;
        self.dribble = Some(file);
        Ok(())
    }

    /// Close the dribble file. Mirrors GNU's
    /// `Fopen_dribble_file (Qnil)` path.
    pub fn close_dribble_file(&mut self) {
        if let Some(mut f) = self.dribble.take() {
            use std::io::Write;
            let _ = f.flush();
        }
    }

    /// Write an input event to the dribble file. Mirrors GNU
    /// `dribble_event` / the inline writes inside
    /// `kbd_buffer_get_event` (`src/keyboard.c:4053-4087`).
    /// A nil event is logged as `nil`; ASCII printable characters
    /// are written as themselves; other events are formatted via
    /// the standard event-to-string fallback. The dribble is
    /// flushed after every event so a crash leaves a complete
    /// record on disk.
    pub fn dribble_event(&mut self, event: Value) {
        let Some(file) = self.dribble.as_mut() else {
            return;
        };
        use std::io::Write;
        if let Some(ch) = event.as_fixnum() {
            if (32..127).contains(&ch) {
                let _ = write!(file, "{}", ch as u8 as char);
                let _ = file.flush();
                return;
            }
            let _ = write!(file, " 0x{:x}", ch);
            let _ = file.flush();
            return;
        }
        let _ = write!(file, " {}", event);
        let _ = file.flush();
    }

    pub fn set_terminal_translation_maps(
        &mut self,
        input_decode_map: Value,
        local_function_key_map: Value,
    ) {
        self.input_decode_map = input_decode_map;
        self.local_function_key_map = local_function_key_map;
    }

    pub fn set_input_decode_map(&mut self, map: Value) {
        self.input_decode_map = map;
    }

    pub fn input_decode_map(&self) -> Value {
        self.input_decode_map
    }

    pub fn set_local_function_key_map(&mut self, map: Value) {
        self.local_function_key_map = map;
    }

    pub fn local_function_key_map(&self) -> Value {
        self.local_function_key_map
    }

    pub fn unread_event(&mut self, event: Value) {
        self.unread_events.push_back(event);
    }

    pub fn set_unread_selection_event(&mut self, event: Value) {
        self.unread_selection_event = Some(event);
    }

    pub fn internal_last_event_frame(&self) -> Option<crate::window::FrameId> {
        self.internal_last_event_frame
    }

    pub fn set_internal_last_event_frame(&mut self, frame_id: crate::window::FrameId) {
        self.internal_last_event_frame = Some(frame_id);
    }

    pub fn mouse_pixel_position(&self) -> Option<MousePixelPositionState> {
        self.mouse_pixel_position
    }

    pub fn set_mouse_pixel_position(
        &mut self,
        frame_id: Option<crate::window::FrameId>,
        x: i64,
        y: i64,
    ) {
        self.mouse_pixel_position = Some(MousePixelPositionState { frame_id, x, y });
    }

    pub fn unread_key(&mut self, event: KeyEvent) {
        self.unread_event(event.to_emacs_event_value());
    }

    pub fn reset_key_sequence(&mut self) {
        self.current_key_sequence.reset();
    }

    pub fn push_key_sequence_input_event(&mut self, event: Value) {
        self.current_key_sequence.push_input_event(event);
    }

    pub fn rewrite_key_sequence_translation(&mut self, events: Vec<Value>) {
        self.current_key_sequence.replace_translated_events(events);
    }

    pub fn rewrite_key_sequence_events(&mut self, events: Vec<Value>) {
        self.current_key_sequence.replace_events(events);
    }

    pub fn key_sequence_snapshot(&self) -> (Vec<Value>, Vec<Value>) {
        self.current_key_sequence.snapshot()
    }

    pub fn set_command_key_sequences(&mut self, translated: Vec<Value>, raw: Vec<Value>) {
        self.command_keys = translated;
        self.raw_command_keys = raw;
    }

    pub fn set_translated_command_keys(&mut self, keys: Vec<Value>) {
        self.command_keys = keys;
    }

    pub fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        self.command_keys = keys.clone();
        self.raw_command_keys = keys;
    }

    pub fn append_read_command_key(&mut self, key: Value) {
        self.command_keys.push(key);
        self.raw_command_keys.push(key);
    }

    pub fn clear_read_command_keys(&mut self) {
        self.command_keys.clear();
        self.raw_command_keys.clear();
    }

    pub fn read_command_keys(&self) -> &[Value] {
        &self.command_keys
    }

    pub fn read_raw_command_keys(&self) -> &[Value] {
        &self.raw_command_keys
    }

    pub fn record_input_event(&mut self, event: Value) {
        self.recent_input_events.push(event);
        if self.recent_input_events.len() > crate::emacs_core::eval::RECENT_INPUT_EVENT_LIMIT {
            self.recent_input_events.remove(0);
        }
        // GNU `kbd_buffer_get_event` writes every read event to
        // the dribble file (if open). Mirroring that here at the
        // canonical lossage-ring entry point captures every event
        // that flows through the keyboard module.
        self.dribble_event(event);
    }

    pub fn record_recent_command(&mut self, command: Value) {
        self.recent_input_events
            .push(Value::cons(Value::NIL, command));
        if self.recent_input_events.len() > crate::emacs_core::eval::RECENT_INPUT_EVENT_LIMIT {
            self.recent_input_events.remove(0);
        }
    }

    pub fn recent_input_events(&self) -> &[Value] {
        &self.recent_input_events
    }

    pub fn clear_recent_input_events(&mut self) {
        self.recent_input_events.clear();
    }

    pub fn start_kbd_macro(&mut self) {
        self.start_kbd_macro_with_initial(None, false);
    }

    pub fn start_kbd_macro_with_initial(&mut self, initial_events: Option<&[Value]>, append: bool) {
        self.defining_kbd_macro = true;
        self.appending_kbd_macro = append;
        self.kbd_macro_events.clear();
        if let Some(initial_events) = initial_events {
            self.kbd_macro_events.extend_from_slice(initial_events);
        }
        self.kbd_macro_end = self.kbd_macro_events.len();
    }

    pub fn store_kbd_macro_event(&mut self, event: Value) {
        if self.defining_kbd_macro {
            self.kbd_macro_events.push(event);
        }
    }

    pub fn finalize_kbd_macro_chars(&mut self) {
        self.kbd_macro_end = self.kbd_macro_events.len();
    }

    pub fn cancel_kbd_macro_events(&mut self) {
        self.kbd_macro_events.truncate(self.kbd_macro_end);
    }

    pub fn end_kbd_macro(&mut self) -> Vec<Value> {
        self.defining_kbd_macro = false;
        self.appending_kbd_macro = false;
        let finalized = self.kbd_macro_events[..self.kbd_macro_end].to_vec();
        self.last_kbd_macro = Some(finalized.clone());
        finalized
    }

    pub fn last_kbd_macro(&self) -> Option<&[Value]> {
        self.last_kbd_macro.as_deref()
    }

    pub fn begin_executing_kbd_macro(&mut self, events: Vec<Value>) {
        self.executing_kbd_macro = Some(events);
        self.kbd_macro_index = 0;
        self.executing_kbd_macro_iterations = 0;
    }

    pub fn is_executing_kbd_macro(&self) -> bool {
        self.executing_kbd_macro.is_some()
    }

    pub fn finish_executing_kbd_macro(&mut self) {
        self.executing_kbd_macro = None;
        self.kbd_macro_index = 0;
    }

    pub(crate) fn snapshot_executing_kbd_macro_runtime(&self) -> ExecutingKbdMacroRuntimeSnapshot {
        ExecutingKbdMacroRuntimeSnapshot {
            events: self.executing_kbd_macro.clone(),
            index: self.kbd_macro_index,
        }
    }

    pub(crate) fn restore_executing_kbd_macro_runtime(
        &mut self,
        snapshot: ExecutingKbdMacroRuntimeSnapshot,
    ) {
        self.executing_kbd_macro = snapshot.events;
        self.kbd_macro_index = snapshot.index;
    }

    pub(crate) fn set_executing_kbd_macro_index(&mut self, index: usize) {
        self.kbd_macro_index = index;
    }

    pub(crate) fn note_executing_kbd_macro_iteration(&mut self, success_count: usize) {
        self.executing_kbd_macro_iterations = success_count;
    }
}

impl Default for KBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::gc_trace::GcTrace for KBoard {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        if let Some(event) = self.unread_selection_event {
            roots.push(event);
        }
        if let Some(event) = self.last_help_echo_event {
            roots.push(event);
        }
        roots.extend(self.unread_events.iter().copied());
        roots.extend(self.current_key_sequence.raw_events().iter().copied());
        roots.extend(
            self.current_key_sequence
                .translated_events()
                .iter()
                .copied(),
        );
        roots.extend(self.command_keys.iter().copied());
        roots.extend(self.raw_command_keys.iter().copied());
        roots.extend(self.recent_input_events.iter().copied());
        roots.push(self.input_decode_map);
        roots.push(self.local_function_key_map);
        roots.extend(self.kbd_macro_events.iter().copied());
        if let Some(events) = &self.last_kbd_macro {
            roots.extend(events.iter().copied());
        }
        if let Some(events) = &self.executing_kbd_macro {
            roots.extend(events.iter().copied());
        }
    }
}

/// Keyboard runtime state shared by the command loop.
///
/// This owns transport-facing queues plus the active keyboard-local `KBoard`
/// state, which is the nearest NeoVM equivalent to GNU `keyboard.c` +
/// `kboard`.
pub struct KeyboardRuntime {
    /// Input event queue used by unit tests and non-blocking command-loop paths.
    pub event_queue: VecDeque<InputEvent>,
    /// Input already received from the host but not yet returned by `read_char`.
    pub(crate) pending_input_events: crate::frontend_events::FrontendEventQueue,
    /// Terminal id for the currently active `kboard`.
    active_terminal_id: u64,
    /// Parked keyboard-local state for terminals that are not currently active.
    parked_kboards: HashMap<u64, KBoard>,
    /// Keyboard-local command-loop state.
    pub kboard: KBoard,
}

/// GNU's `num_nonmacro_input_events` slot, reached by its Lisp name because
/// in GNU the Lisp name *is* the slot (`src/keyboard.c:13903`).
fn num_nonmacro_input_events_symbol() -> crate::emacs_core::intern::SymId {
    static SYMBOL: std::sync::OnceLock<crate::emacs_core::intern::SymId> =
        std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("num-nonmacro-input-events"))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum InputEventHistoryDisposition {
    Record,
    SuppressDuringMacroPlayback,
}

/// Whether an event that was just recorded advances GNU's
/// `num_nonmacro_input_events` (`src/keyboard.c:3576`).
///
/// The counter is the `DEFVAR_INT` `num-nonmacro-input-events`
/// (`src/keyboard.c:13903`) -- Lisp and C read and write the *same*
/// `intmax_t`, which `maybe_call_debugger` compares against
/// `when_entered_debugger` (`src/eval.c:2212`).  A second Rust-side copy of it
/// was what made `(setq num-nonmacro-input-events 5)` invisible to the stamp
/// (ledger 183), so [`CommandLoop`] no longer holds one: it reports the
/// decision and the layer that owns the obarray performs the increment.
///
/// `#[must_use]` because dropping the answer is exactly the bug: the event was
/// recorded in the lossage ring and the counter silently did not move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[must_use = "a recorded non-macro event must advance num-nonmacro-input-events"]
pub enum NonmacroInputEvent {
    /// `record_char` reached its `num_nonmacro_input_events++`.
    Counted,
    /// The event came from an executing keyboard macro, which GNU excludes
    /// (`src/keyboard.c:3469-3590`).
    SuppressedByMacroPlayback,
}

/// GNU's two filtering modes for an `input-pending-p` command-input query.
///
/// The Lisp variable does not toggle filtering off. Non-nil uses the full
/// `while-no-input-ignore-events` set; nil still filters focus events.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputPendingFilter {
    ConfiguredIgnoreList,
    FocusEventsOnly,
}

impl InputPendingFilter {
    pub(crate) const fn from_filter_events_variable(enabled: bool) -> Self {
        if enabled {
            Self::ConfiguredIgnoreList
        } else {
            Self::FocusEventsOnly
        }
    }

    pub(crate) fn ignores(
        self,
        symbol: &str,
        ignored_while_no_input: &impl Fn(&str) -> bool,
    ) -> bool {
        match self {
            Self::ConfiguredIgnoreList => ignored_while_no_input(symbol),
            Self::FocusEventsOnly => matches!(symbol, "focus-in" | "focus-out"),
        }
    }
}

impl KeyboardRuntime {
    fn kboards(&self) -> impl Iterator<Item = &KBoard> {
        std::iter::once(&self.kboard).chain(self.parked_kboards.values())
    }

    fn input_event_history_disposition(&self) -> InputEventHistoryDisposition {
        if self.kboard.is_executing_kbd_macro() {
            InputEventHistoryDisposition::SuppressDuringMacroPlayback
        } else {
            InputEventHistoryDisposition::Record
        }
    }

    fn low_level_event_kind(event: Value) -> Option<&'static str> {
        let head = if event.is_cons() {
            event.cons_car()
        } else {
            event
        };
        head.as_symbol_name()
    }

    fn low_level_event_counts_as_pending(
        event: Value,
        filter: InputPendingFilter,
        ignored_while_no_input: &impl Fn(&str) -> bool,
    ) -> bool {
        Self::low_level_event_kind(event)
            .is_none_or(|kind| !filter.ignores(kind, ignored_while_no_input))
    }

    pub fn new() -> Self {
        Self {
            event_queue: VecDeque::new(),
            pending_input_events: crate::frontend_events::FrontendEventQueue::default(),
            active_terminal_id: crate::emacs_core::terminal::pure::TERMINAL_ID,
            parked_kboards: HashMap::new(),
            kboard: KBoard::new(),
        }
    }

    pub fn active_terminal_id(&self) -> u64 {
        self.active_terminal_id
    }

    pub fn mouse_pixel_position(&self) -> Option<MousePixelPositionState> {
        self.kboard.mouse_pixel_position()
    }

    pub fn set_mouse_pixel_position(
        &mut self,
        frame_id: Option<crate::window::FrameId>,
        x: i64,
        y: i64,
    ) {
        self.kboard.set_mouse_pixel_position(frame_id, x, y);
    }

    pub fn select_terminal(&mut self, terminal_id: u64) {
        if self.active_terminal_id == terminal_id {
            return;
        }
        let current_id = self.active_terminal_id;
        let current = std::mem::take(&mut self.kboard);
        self.parked_kboards.insert(current_id, current);
        self.kboard = self.parked_kboards.remove(&terminal_id).unwrap_or_default();
        self.active_terminal_id = terminal_id;
    }

    pub fn delete_terminal_kboard(&mut self, terminal_id: u64) {
        self.parked_kboards.remove(&terminal_id);
        if self.active_terminal_id == terminal_id {
            self.kboard = KBoard::default();
        }
    }

    fn parked_terminal_ids(&self) -> Vec<u64> {
        let mut ids = crate::emacs_core::terminal::pure::live_terminal_ids_in_keyboard_poll_order()
            .into_iter()
            .filter(|terminal_id| self.parked_kboards.contains_key(terminal_id))
            .collect::<Vec<_>>();
        let mut unknown_ids = self
            .parked_kboards
            .keys()
            .copied()
            .filter(|terminal_id| !ids.contains(terminal_id))
            .collect::<Vec<_>>();
        unknown_ids.sort_unstable();
        ids.extend(unknown_ids);
        ids
    }

    fn poll_parked_kboard<R>(&mut self, mut f: impl FnMut(&mut KBoard) -> Option<R>) -> Option<R> {
        for terminal_id in self.parked_terminal_ids() {
            let Some(kboard) = self.parked_kboards.get_mut(&terminal_id) else {
                continue;
            };
            let Some(result) = f(kboard) else {
                continue;
            };
            self.select_terminal(terminal_id);
            return Some(result);
        }
        None
    }

    pub fn take_unread_selection_event(&mut self) -> Option<Value> {
        self.kboard
            .unread_selection_event
            .take()
            .or_else(|| self.poll_parked_kboard(|kboard| kboard.unread_selection_event.take()))
    }

    pub(crate) fn set_unread_selection_event(&mut self, event: Value) {
        self.kboard.set_unread_selection_event(event);
    }

    pub(crate) fn has_unread_selection_event(&self) -> bool {
        self.kboard.unread_selection_event.is_some()
    }

    pub fn pop_unread_event(&mut self) -> Option<Value> {
        self.kboard
            .unread_events
            .pop_front()
            .or_else(|| self.poll_parked_kboard(|kboard| kboard.unread_events.pop_front()))
    }

    pub fn next_executing_kbd_macro_event(&mut self) -> Option<Value> {
        if let Some(ref macro_events) = self.kboard.executing_kbd_macro
            && self.kboard.kbd_macro_index < macro_events.len()
        {
            let event = macro_events[self.kboard.kbd_macro_index];
            self.kboard.kbd_macro_index += 1;
            return Some(event);
        }
        self.poll_parked_kboard(|kboard| {
            let macro_events = kboard.executing_kbd_macro.as_ref()?;
            if kboard.kbd_macro_index >= macro_events.len() {
                return None;
            }
            let event = macro_events[kboard.kbd_macro_index];
            kboard.kbd_macro_index += 1;
            Some(event)
        })
    }

    /// Return whether a keyboard read can consume a queued low-level or
    /// deferred-selection event before consulting the frontend transport.
    /// The filtered `input-pending-p` predicate is intentionally separate.
    pub fn has_pending_low_level_input(&self) -> bool {
        self.kboards().any(|kboard| {
            kboard.unread_selection_event.is_some() || !kboard.unread_events.is_empty()
        })
    }

    /// Whether GNU's filtered `input-pending-p` query sees a low-level event.
    ///
    /// `unread_selection_event` mirrors GNU's `unread_switch_frame`, which is
    /// intentionally absent from both `requeued_events_pending_p` and the
    /// terminal ring inspected by `readable_events`.  It remains readable but
    /// must not preempt idle work.  Events in `unread_events` mirror that
    /// terminal ring and therefore use GNU's configured event-kind filter.
    fn has_pending_low_level_input_for_query(
        &self,
        filter: InputPendingFilter,
        ignored_while_no_input: impl Fn(&str) -> bool,
    ) -> bool {
        self.kboards().any(|kboard| {
            kboard.unread_events.iter().copied().any(|event| {
                Self::low_level_event_counts_as_pending(event, filter, &ignored_while_no_input)
            })
        })
    }

    /// Apply one GNU-compatible command-input policy to every queue owned by
    /// the keyboard runtime. Deferred selection events remain readable but do
    /// not count as pending, matching GNU's separate `unread_switch_frame`.
    pub(crate) fn has_pending_command_input_for_query(
        &self,
        filter: InputPendingFilter,
        track_mouse: bool,
        ignored_while_no_input: impl Fn(&str) -> bool,
    ) -> bool {
        self.has_pending_low_level_input_for_query(filter, &ignored_while_no_input)
            || self.pending_input_events.has_pending_input(
                filter,
                track_mouse,
                &ignored_while_no_input,
            )
    }

    /// Return whether the next keyboard read can complete without waiting.
    /// Unlike `input-pending-p`, a keyboard read can consume the remaining
    /// events of an executing keyboard macro.
    pub fn has_pending_kboard_input(&self) -> bool {
        self.has_pending_low_level_input()
            || self.kboards().any(|kboard| {
                kboard
                    .executing_kbd_macro
                    .as_ref()
                    .is_some_and(|events| kboard.kbd_macro_index < events.len())
            })
    }

    pub fn set_terminal_translation_maps(
        &mut self,
        input_decode_map: Value,
        local_function_key_map: Value,
    ) {
        self.kboard
            .set_terminal_translation_maps(input_decode_map, local_function_key_map);
    }

    pub fn set_input_decode_map(&mut self, map: Value) {
        self.kboard.set_input_decode_map(map);
    }

    pub fn input_decode_map(&self) -> Value {
        self.kboard.input_decode_map()
    }

    pub fn set_local_function_key_map(&mut self, map: Value) {
        self.kboard.set_local_function_key_map(map);
    }

    pub fn local_function_key_map(&self) -> Value {
        self.kboard.local_function_key_map()
    }

    pub fn enqueue_event(&mut self, event: InputEvent) {
        self.event_queue.push_back(event);
    }

    pub fn unread_event(&mut self, event: Value) {
        self.kboard.unread_events.push_back(event);
    }

    pub fn unread_key(&mut self, event: KeyEvent) {
        self.unread_event(event.to_emacs_event_value());
    }

    pub fn read_key_event(&mut self) -> Option<Value> {
        if let Some(event) = self.pop_unread_event() {
            return Some(event);
        }

        if let Some(event) = self.next_executing_kbd_macro_event() {
            return Some(event);
        }

        while let Some(event) = self.event_queue.pop_front() {
            if let InputEvent::KeyPress { key, .. } = event {
                let emacs_event = key.to_emacs_event_value();
                self.kboard.store_kbd_macro_event(emacs_event);
                return Some(emacs_event);
            }
        }

        None
    }

    pub fn reset_key_sequence(&mut self) {
        self.kboard.current_key_sequence.reset();
    }

    pub fn push_key_sequence_input_event(&mut self, event: Value) {
        self.kboard.current_key_sequence.push_input_event(event);
    }

    pub fn rewrite_key_sequence_translation(&mut self, events: Vec<Value>) {
        self.kboard
            .current_key_sequence
            .replace_translated_events(events);
    }

    pub fn rewrite_key_sequence_events(&mut self, events: Vec<Value>) {
        self.kboard.current_key_sequence.replace_events(events);
    }

    pub fn key_sequence_snapshot(&self) -> (Vec<Value>, Vec<Value>) {
        self.kboard.current_key_sequence.snapshot()
    }

    pub fn set_command_key_sequences(&mut self, translated: Vec<Value>, raw: Vec<Value>) {
        self.kboard.command_keys = translated;
        self.kboard.raw_command_keys = raw;
    }

    pub fn set_translated_command_keys(&mut self, keys: Vec<Value>) {
        self.kboard.command_keys = keys;
    }

    pub fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        self.kboard.command_keys = keys.clone();
        self.kboard.raw_command_keys = keys;
    }

    pub fn clear_read_command_keys(&mut self) {
        self.kboard.command_keys.clear();
        self.kboard.raw_command_keys.clear();
    }

    pub fn read_command_keys(&self) -> &[Value] {
        &self.kboard.command_keys
    }

    pub fn read_raw_command_keys(&self) -> &[Value] {
        &self.kboard.raw_command_keys
    }

    pub fn record_input_event(&mut self, event: Value) {
        self.kboard.recent_input_events.push(event);
        if self.kboard.recent_input_events.len() > crate::emacs_core::eval::RECENT_INPUT_EVENT_LIMIT
        {
            self.kboard.recent_input_events.remove(0);
        }
    }

    pub fn record_recent_command(&mut self, command: Value) {
        self.kboard.record_recent_command(command);
    }

    pub fn recent_input_events(&self) -> &[Value] {
        &self.kboard.recent_input_events
    }

    pub fn clear_recent_input_events(&mut self) {
        self.kboard.recent_input_events.clear();
    }

    pub fn start_kbd_macro(&mut self) {
        self.kboard.start_kbd_macro();
    }

    pub fn start_kbd_macro_with_initial(&mut self, initial_events: Option<&[Value]>, append: bool) {
        self.kboard
            .start_kbd_macro_with_initial(initial_events, append);
    }

    pub fn store_kbd_macro_event(&mut self, event: Value) {
        self.kboard.store_kbd_macro_event(event);
    }

    pub fn finalize_kbd_macro_chars(&mut self) {
        self.kboard.finalize_kbd_macro_chars();
    }

    pub fn cancel_kbd_macro_events(&mut self) {
        self.kboard.cancel_kbd_macro_events();
    }

    pub fn end_kbd_macro(&mut self) -> Vec<Value> {
        self.kboard.end_kbd_macro()
    }

    pub fn last_kbd_macro(&self) -> Option<&[Value]> {
        self.kboard.last_kbd_macro()
    }

    pub fn begin_executing_kbd_macro(&mut self, events: Vec<Value>) {
        self.kboard.begin_executing_kbd_macro(events);
    }

    pub fn is_executing_kbd_macro(&self) -> bool {
        self.kboard.is_executing_kbd_macro()
    }

    pub fn finish_executing_kbd_macro(&mut self) {
        self.kboard.finish_executing_kbd_macro();
    }
}

impl Default for KeyboardRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::gc_trace::GcTrace for KeyboardRuntime {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.kboard.trace_roots(roots);
        for kboard in self.parked_kboards.values() {
            kboard.trace_roots(roots);
        }
    }
}

/// State of the command loop.
pub struct CommandLoop {
    /// Keyboard-local runtime state.
    pub keyboard: KeyboardRuntime,
    /// Lisp meanings paired with immutable displayed frame presentations.
    pub presented_interactions: PresentedInteractions,
    /// Current prefix argument.
    pub prefix_arg: PrefixArg,
    /// Whether we are in a recursive edit.
    pub recursive_depth: usize,
    /// Whether the command loop is running.
    pub running: bool,
    /// Whether C-g was pressed (quit flag).
    pub quit_flag: bool,
    /// Inhibit quit (during critical sections).
    pub inhibit_quit: bool,
    /// GNU-style idle timer epoch: when Emacs most recently became idle.
    idle_start_time: Option<std::time::Instant>,
    /// Last idle epoch preserved across non-user internal events.
    last_idle_start_time: Option<std::time::Instant>,
    /// Value of `num_nonmacro_input_events` the last time an
    /// auto-save fired from the command loop. GNU tracks this in
    /// `static intmax_t last_auto_save` (`src/keyboard.c:237`) -- a plain C
    /// static with no `DEFVAR`, so unlike the counter it compares against it
    /// is right for it to live here.
    pub last_auto_save_input_events: i64,
    /// Size of the most recently selected non-minibuffer buffer, in
    /// characters. GNU keeps this separately because a minibuffer input wait
    /// should scale idle auto-save latency from the edited buffer, not from
    /// the tiny minibuffer (`src/keyboard.c:229,2939-2941`).
    last_non_minibuffer_size: usize,
}

impl CommandLoop {
    pub fn new() -> Self {
        Self {
            keyboard: KeyboardRuntime::new(),
            presented_interactions: PresentedInteractions::default(),
            prefix_arg: PrefixArg::None,
            recursive_depth: 0,
            running: false,
            quit_flag: false,
            inhibit_quit: false,
            idle_start_time: None,
            last_idle_start_time: None,
            last_auto_save_input_events: 0,
            last_non_minibuffer_size: 0,
        }
    }

    /// Push an input event.
    pub fn enqueue_event(&mut self, event: InputEvent) {
        self.keyboard.enqueue_event(event);
    }

    /// Push an unread command event (to be processed before the queue).
    pub fn unread_event(&mut self, event: Value) {
        self.keyboard.unread_event(event);
    }

    /// Push an unread key event (to be processed before the queue).
    pub fn unread_key(&mut self, event: KeyEvent) {
        self.keyboard.unread_key(event);
    }

    /// Read the next key event as a Lisp-visible Emacs event.
    /// Returns from unread events first, then the event queue.
    pub fn read_key_event(&mut self) -> Option<Value> {
        self.keyboard.read_key_event()
    }

    /// Reset the key sequence accumulator.
    pub fn reset_key_sequence(&mut self) {
        self.keyboard.reset_key_sequence();
    }

    pub fn set_command_key_sequences(&mut self, translated: Vec<Value>, raw: Vec<Value>) {
        self.keyboard.set_command_key_sequences(translated, raw);
    }

    pub fn set_translated_command_keys(&mut self, keys: Vec<Value>) {
        self.keyboard.set_translated_command_keys(keys);
    }

    pub fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        self.keyboard.set_read_command_keys(keys);
    }

    pub fn clear_read_command_keys(&mut self) {
        self.keyboard.clear_read_command_keys();
    }

    pub fn read_command_keys(&self) -> &[Value] {
        self.keyboard.read_command_keys()
    }

    pub fn read_raw_command_keys(&self) -> &[Value] {
        self.keyboard.read_raw_command_keys()
    }

    pub fn record_input_event(&mut self, event: Value) -> NonmacroInputEvent {
        // GNU `read_char` publishes every accepted event through
        // `last-input-event`, but `record_char` excludes keyboard-macro
        // playback from recent-keys, the dribble file, and
        // `num_nonmacro_input_events` (keyboard.c:3385,3469-3590). Keep that
        // history policy here rather than making callers skip the whole
        // publication operation.
        //
        // The counter itself is NOT bumped here: it is the `DEFVAR_INT`
        // `num-nonmacro-input-events` (`src/keyboard.c:13903`), which lives in
        // the obarray this type cannot see.  Returning the decision instead of
        // duplicating the counter is what keeps GNU's one slot one slot; see
        // [`NonmacroInputEvent`].
        match self.keyboard.input_event_history_disposition() {
            InputEventHistoryDisposition::Record => {
                self.keyboard.record_input_event(event);
                NonmacroInputEvent::Counted
            }
            InputEventHistoryDisposition::SuppressDuringMacroPlayback => {
                NonmacroInputEvent::SuppressedByMacroPlayback
            }
        }
    }

    pub fn record_recent_command(&mut self, command: Value) {
        self.keyboard.record_recent_command(command);
    }

    pub fn recent_input_events(&self) -> &[Value] {
        self.keyboard.recent_input_events()
    }

    pub fn clear_recent_input_events(&mut self) {
        self.keyboard.clear_recent_input_events();
    }

    /// Start recording a keyboard macro.
    pub fn start_kbd_macro(&mut self) {
        self.keyboard.start_kbd_macro();
    }

    pub fn start_kbd_macro_with_initial(&mut self, initial_events: Option<&[Value]>, append: bool) {
        self.keyboard
            .start_kbd_macro_with_initial(initial_events, append);
    }

    pub fn store_kbd_macro_event(&mut self, event: Value) {
        self.keyboard.store_kbd_macro_event(event);
    }

    pub fn finalize_kbd_macro_chars(&mut self) {
        self.keyboard.finalize_kbd_macro_chars();
    }

    pub fn cancel_kbd_macro_events(&mut self) {
        self.keyboard.cancel_kbd_macro_events();
    }

    /// Stop recording a keyboard macro.
    pub fn end_kbd_macro(&mut self) -> Vec<Value> {
        self.keyboard.end_kbd_macro()
    }

    pub fn last_kbd_macro(&self) -> Option<&[Value]> {
        self.keyboard.last_kbd_macro()
    }

    pub fn begin_executing_kbd_macro(&mut self, events: Vec<Value>) {
        self.keyboard.begin_executing_kbd_macro(events);
    }

    pub fn is_executing_kbd_macro(&self) -> bool {
        self.keyboard.is_executing_kbd_macro()
    }

    pub fn finish_executing_kbd_macro(&mut self) {
        self.keyboard.finish_executing_kbd_macro();
    }

    /// Signal a quit (C-g).
    pub fn signal_quit(&mut self) {
        if !self.inhibit_quit {
            self.quit_flag = true;
        }
    }

    /// Clear the quit flag and return whether it was set.
    pub fn check_quit(&mut self) -> bool {
        let was_set = self.quit_flag;
        self.quit_flag = false;
        was_set
    }
}

impl Default for CommandLoop {
    fn default() -> Self {
        Self::new()
    }
}

impl crate::gc_trace::GcTrace for CommandLoop {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.keyboard.trace_roots(roots);
        self.presented_interactions.trace_roots(roots);
    }
}

fn apply_resize_input_event_in_keyboard_runtime(
    frames: &mut crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    width: u32,
    height: u32,
    scale_factor: f64,
    emacs_frame_id: u64,
) {
    let target_fid = if emacs_frame_id == 0 {
        frames.selected_frame().map(|frame| frame.id)
    } else {
        Some(crate::window::FrameId(emacs_frame_id))
    };

    if let Some(fid) = target_fid
        && let Some(frame) = frames.get_mut(fid)
    {
        frame.device_scale_factor = if scale_factor.is_finite() && scale_factor > 0.0 {
            scale_factor
        } else {
            1.0
        };
        frame.resize_pixelwise_with_buffer_constraints(buffers, width, height);
    }
}

fn pending_live_gui_resize_target(
    frames: &crate::window::FrameManager,
    emacs_frame_id: u64,
) -> Option<crate::window::FrameId> {
    let target_fid = if emacs_frame_id == 0 {
        frames.selected_frame().map(|frame| frame.id)
    } else {
        Some(crate::window::FrameId(emacs_frame_id))
    }?;
    frames
        .get(target_fid)
        .and_then(|frame| frame.pending_gui_resize.as_ref().map(|_| target_fid))
}

fn sync_pending_resize_events_in_keyboard_runtime(
    frames: &mut crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    input_rx: &mut Option<crossbeam_channel::Receiver<InputEvent>>,
    keyboard: &mut KeyboardRuntime,
) -> bool {
    let mut applied_resize = false;
    let mut deferred = VecDeque::new();
    let pending_input_events = &mut keyboard.pending_input_events;

    loop {
        match pending_input_events.front() {
            Some(InputEvent::Focus { .. }) => {
                if let Some(event) = pending_input_events.pop_visible_front() {
                    deferred.push_back(event);
                }
            }
            Some(InputEvent::Resize {
                width,
                height,
                scale_factor,
                emacs_frame_id,
            }) => {
                if pending_live_gui_resize_target(frames, *emacs_frame_id).is_some() {
                    break;
                }
                let (width, height, scale_factor, emacs_frame_id) =
                    (*width, *height, *scale_factor, *emacs_frame_id);
                pending_input_events.pop_visible_front();
                apply_resize_input_event_in_keyboard_runtime(
                    frames,
                    buffers,
                    width,
                    height,
                    scale_factor,
                    emacs_frame_id,
                );
                applied_resize = true;
            }
            _ => break,
        }
    }

    if !pending_input_events.is_empty() {
        while let Some(event) = deferred.pop_back() {
            pending_input_events.push_front(event);
        }
        return applied_resize;
    }

    let Some(rx) = input_rx.clone() else {
        while let Some(event) = deferred.pop_back() {
            pending_input_events.push_front(event);
        }
        return applied_resize;
    };

    loop {
        match rx.try_recv() {
            Ok(InputEvent::Resize {
                width,
                height,
                scale_factor,
                emacs_frame_id,
            }) => {
                if pending_live_gui_resize_target(frames, emacs_frame_id).is_some() {
                    // Preserve host resize acks until the geometry-query path
                    // flushes the deferred live-GUI resize request.
                    deferred.push_back(InputEvent::Resize {
                        width,
                        height,
                        scale_factor,
                        emacs_frame_id,
                    });
                    break;
                }
                apply_resize_input_event_in_keyboard_runtime(
                    frames,
                    buffers,
                    width,
                    height,
                    scale_factor,
                    emacs_frame_id,
                );
                applied_resize = true;
            }
            Ok(event @ InputEvent::Focus { .. }) => {
                deferred.push_back(event);
            }
            Ok(event) => {
                deferred.push_back(event);
                break;
            }
            Err(crossbeam_channel::TryRecvError::Empty) => break,
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                break;
            }
        }
    }

    while let Some(event) = deferred.pop_back() {
        pending_input_events.push_front(event);
    }

    applied_resize
}

fn sync_opening_gui_frame_size_from_host_in_keyboard_runtime(
    frames: &mut crate::window::FrameManager,
    buffers: &crate::buffer::BufferManager,
    display_host: Option<&dyn crate::emacs_core::eval::DisplayHost>,
) {
    let trace_host_sync = std::env::var("NEOMACS_TRACE_HOST_SYNC")
        .ok()
        .is_some_and(|value| value == "1");
    let Some(host) = display_host else {
        if trace_host_sync {
            tracing::debug!("sync_opening_gui_frame_size_from_host: no display host");
        }
        return;
    };
    if !host.opening_gui_frame_pending() {
        if trace_host_sync {
            tracing::debug!("sync_opening_gui_frame_size_from_host: no opening gui frame pending");
        }
        return;
    }
    let Some(size) = host.current_primary_window_size() else {
        if trace_host_sync {
            tracing::debug!("sync_opening_gui_frame_size_from_host: host size unavailable");
        }
        return;
    };
    if size.width == 0 || size.height == 0 {
        if trace_host_sync {
            tracing::debug!(
                "sync_opening_gui_frame_size_from_host: ignoring zero host size {}x{}",
                size.width,
                size.height
            );
        }
        return;
    }
    let Some(fid) = frames.selected_frame().map(|frame| frame.id) else {
        if trace_host_sync {
            tracing::debug!("sync_opening_gui_frame_size_from_host: no selected frame");
        }
        return;
    };
    let Some(frame) = frames.get_mut(fid) else {
        if trace_host_sync {
            tracing::debug!(
                "sync_opening_gui_frame_size_from_host: selected frame {:?} missing",
                fid
            );
        }
        return;
    };
    if frame.effective_window_system().is_none() {
        if trace_host_sync {
            tracing::debug!(
                "sync_opening_gui_frame_size_from_host: selected frame {:?} is not gui (size={}x{})",
                fid,
                frame.width,
                frame.height
            );
        }
        return;
    }
    if frame.width == size.width && frame.height == size.height {
        if trace_host_sync {
            tracing::debug!(
                "sync_opening_gui_frame_size_from_host: selected frame {:?} already matches host size {}x{}",
                fid,
                size.width,
                size.height
            );
        }
        return;
    }
    tracing::debug!(
        "sync_opening_gui_frame_size_from_host: resizing selected frame {:?} from {}x{} to {}x{}",
        fid,
        frame.width,
        frame.height,
        size.width,
        size.height
    );
    frame.resize_pixelwise_with_buffer_constraints(buffers, size.width, size.height);
}

#[derive(Clone, Copy, Debug)]
struct MousePosnMetrics {
    point: Option<i64>,
    col: Option<i64>,
    row: Option<i64>,
    width: Option<i64>,
    height: Option<i64>,
    anchor_x: Option<i64>,
    anchor_y: Option<i64>,
}

#[derive(Clone, Copy, Debug)]
struct MousePosnDescriptor {
    window_or_frame: Value,
    area: Option<&'static str>,
    x: i64,
    y: i64,
    metrics: MousePosnMetrics,
}

enum QueuedReadCharEvent {
    None,
    HandledInternally,
    Event(ReadCharEvent),
}

impl crate::emacs_core::eval::Context {
    pub(crate) fn service_leading_internal_frontend_events(
        &mut self,
    ) -> crate::frontend_events::InternalEventEffects {
        let mut effects = crate::frontend_events::InternalEventEffects::default();
        while let Some(event) = self
            .command_loop
            .keyboard
            .pending_input_events
            .take_leading_internal()
        {
            let event_effects = match event {
                crate::frontend_events::InternalFrontendEvent::PresentationRetired {
                    presentation,
                } => {
                    self.retire_interaction_presentation(presentation);
                    if let Some(presentation) =
                        crate::window::geometry::PresentationId::try_new(presentation)
                    {
                        for frame in self.frame_manager_mut().frames_mut() {
                            frame.retire_display_presentation(presentation);
                        }
                    }
                    crate::frontend_events::InternalEventEffects::default()
                }
                crate::frontend_events::InternalFrontendEvent::PresentationActivated {
                    presentation,
                    emacs_frame_id,
                } => {
                    if let Some(presentation) =
                        crate::window::geometry::PresentationId::try_new(presentation)
                        && let Some(frame) = self
                            .frame_manager_mut()
                            .get_mut(crate::window::FrameId(emacs_frame_id))
                        && let Err(error) = frame.activate_display_presentation(presentation)
                    {
                        tracing::debug!(
                            ?error,
                            emacs_frame_id,
                            "ignored activation for a presentation no longer prepared"
                        );
                    }
                    crate::frontend_events::InternalEventEffects::default()
                }
                crate::frontend_events::InternalFrontendEvent::PresentationDiscarded {
                    presentation,
                    emacs_frame_id,
                } => {
                    self.retire_interaction_presentation(presentation);
                    if let Some(presentation) =
                        crate::window::geometry::PresentationId::try_new(presentation)
                        && let Some(frame) = self
                            .frame_manager_mut()
                            .get_mut(crate::window::FrameId(emacs_frame_id))
                    {
                        frame.discard_display_presentation(presentation);
                    }
                    crate::frontend_events::InternalEventEffects::default()
                }
                crate::frontend_events::InternalFrontendEvent::LayoutInvalidated => {
                    self.invalidate_media();
                    crate::frontend_events::InternalEventEffects {
                        redisplay_needed: true,
                    }
                }
                crate::frontend_events::InternalFrontendEvent::ImageStateChanged { event } => {
                    // Async media completed or lost renderer residency, so
                    // retained image glyphs must not be reused. Reconcile the
                    // exact identity before bumping media_generation.
                    if let Some(host) = self.display_host.as_ref() {
                        host.reconcile_image_catalog_for_media_rebuild(event);
                    }
                    self.invalidate_media();
                    crate::frontend_events::InternalEventEffects {
                        redisplay_needed: true,
                    }
                }
            };
            effects = effects.merge(event_effects);
        }
        effects
    }

    pub fn begin_interaction_presentation(&mut self) -> u64 {
        self.command_loop.presented_interactions.begin()
    }

    pub fn register_presented_mouse_target(
        &mut self,
        presentation: u64,
        target: PresentedMouseTarget,
    ) -> u32 {
        self.command_loop
            .presented_interactions
            .register_mouse_target(presentation, target)
    }

    pub fn resolve_presented_mouse_target(
        &self,
        presentation: u64,
        interaction: u32,
    ) -> Option<PresentedMouseTarget> {
        self.command_loop
            .presented_interactions
            .resolve(presentation, interaction)
    }

    pub fn retire_interaction_presentation(&mut self, presentation: u64) {
        self.command_loop
            .presented_interactions
            .retire(presentation);
    }

    fn restore_delayed_selection_event(&mut self, delayed_selection_event: &mut Option<Value>) {
        if let Some(event) = delayed_selection_event.take() {
            self.command_loop.keyboard.set_unread_selection_event(event);
        }
    }

    fn restore_key_sequence_current_buffer(
        &mut self,
        saved_current_buffer: &mut Option<crate::buffer::BufferId>,
    ) {
        if let Some(buffer_id) = saved_current_buffer.take() {
            self.restore_current_buffer_if_live(buffer_id);
        }
    }

    fn has_switch_frame_event_kind(event: &Value) -> bool {
        match event.kind() {
            ValueKind::Cons => {
                let pair_car = event.cons_car();
                let _pair_cdr = event.cons_cdr();
                matches!(
                    pair_car.as_symbol_name(),
                    Some("switch-frame") | Some("select-window")
                )
            }
            _ => false,
        }
    }

    fn lispy_frame_event_target(&self, emacs_frame_id: u64) -> Option<Value> {
        if emacs_frame_id == 0 {
            self.frames
                .selected_frame()
                .map(|frame| Value::make_frame(frame.id.0))
        } else {
            let fid = crate::window::FrameId(emacs_frame_id);
            self.frames.get(fid)?;
            Some(Value::make_frame(emacs_frame_id))
        }
    }

    fn make_lispy_focus_event(&self, focused: bool, emacs_frame_id: u64) -> Option<Value> {
        let frame = self.lispy_frame_event_target(emacs_frame_id)?;
        Some(Value::list(vec![
            Value::symbol(if focused { "focus-in" } else { "focus-out" }),
            frame,
        ]))
    }

    fn make_lispy_delete_frame_event(&self, emacs_frame_id: u64) -> Option<Value> {
        let frame = self.lispy_frame_event_target(emacs_frame_id)?;
        Some(Value::list(vec![
            Value::symbol("delete-frame"),
            Value::list(vec![frame]),
        ]))
    }

    fn make_lispy_select_window_event(&self, window_id: crate::window::WindowId) -> Option<Value> {
        for frame_id in self.frames.frame_list() {
            let Some(frame) = self.frames.get(frame_id) else {
                continue;
            };
            if frame.find_window(window_id).is_some() {
                return Some(Value::list(vec![
                    Value::symbol("select-window"),
                    Value::list(vec![Value::make_window(window_id.0)]),
                ]));
            }
        }
        None
    }

    pub(crate) fn special_event_binding(&self, event: &Value) -> Option<Value> {
        let special_event_map = self.obarray.symbol_value("special-event-map").copied()?;
        // GNU `read_char` calls `access_keymap` on the event head, so a
        // full lispy event like `(focus-in FRAME)` must match a keymap entry
        // stored under just `focus-in`.
        let lookup_event = match event.kind() {
            ValueKind::Cons => event.cons_car(),
            _ => *event,
        };
        let binding = crate::emacs_core::keymap::lookup_key_in_keymaps_in_obarray(
            self.obarray(),
            &[special_event_map],
            &[lookup_event],
            true,
        );
        if binding.is_nil() || binding.is_fixnum() {
            None
        } else {
            Some(binding)
        }
    }

    fn execute_special_event_if_bound(
        &mut self,
        event: Value,
    ) -> Result<bool, crate::emacs_core::error::Flow> {
        let Some(binding) = self.special_event_binding(&event) else {
            return Ok(false);
        };
        if !self.function_value_is_callable(&binding) {
            return Ok(false);
        }

        self.assign("last-input-event", event);
        let keys = Value::vector(vec![event]);
        self.apply(
            Value::symbol("command-execute"),
            vec![binding, Value::NIL, keys, Value::T],
        )?;
        Ok(true)
    }

    fn publish_current_key_sequence_as_command_keys(&mut self) {
        let (translated, raw) = self.command_loop.keyboard.key_sequence_snapshot();
        self.command_loop.set_command_key_sequences(translated, raw);
    }

    fn resolve_key_sequence_translation_binding(
        &mut self,
        binding: Value,
        prompt: Value,
    ) -> Result<Option<Vec<Value>>, crate::emacs_core::error::Flow> {
        let binding = self.resolve_key_sequence_menu_item_filter(binding)?;
        let resolved = if self.function_value_is_callable(&binding) {
            self.apply(binding, vec![prompt])?
        } else {
            binding
        };
        Ok(key_sequence_translation_events(resolved))
    }

    fn resolve_key_sequence_menu_item_filter(
        &mut self,
        binding: Value,
    ) -> Result<Value, crate::emacs_core::error::Flow> {
        if !binding.is_cons() || !KeymapMarker::MenuItem.is_value(binding.cons_car()) {
            return Ok(binding);
        }

        let tail = binding.cons_cdr();
        if !tail.is_cons() {
            return Ok(binding);
        }
        let definition_tail = tail.cons_cdr();
        if !definition_tail.is_cons() {
            return Ok(Value::NIL);
        }

        let definition = definition_tail.cons_car();
        let mut properties = definition_tail.cons_cdr();
        while properties.is_cons() {
            let key = properties.cons_car();
            properties = properties.cons_cdr();
            if !properties.is_cons() {
                break;
            }
            let value = properties.cons_car();
            properties = properties.cons_cdr();

            if MenuItemProperty::Filter.is_value(key) {
                return self.apply_key_sequence_menu_item_filter(value, definition);
            }
        }

        Ok(definition)
    }

    fn apply_key_sequence_menu_item_filter(
        &mut self,
        filter: Value,
        definition: Value,
    ) -> Result<Value, crate::emacs_core::error::Flow> {
        match self.apply(filter, vec![definition]) {
            Ok(value) => Ok(value),
            Err(crate::emacs_core::error::Flow::Signal(signal))
                if signal.symbol != intern("quit") =>
            {
                Ok(Value::NIL)
            }
            Err(err) => Err(err),
        }
    }

    fn lookup_key_sequence_suffix_translation(
        &mut self,
        map: Value,
        events: &[Value],
        prompt: Value,
    ) -> Result<Option<KeySequenceSuffixTranslation>, crate::emacs_core::error::Flow> {
        use crate::emacs_core::keymap::is_list_keymap;

        if map.is_nil() || !is_list_keymap(&map) {
            return Ok(None);
        }

        for start in 0..events.len() {
            let lookup = crate::emacs_core::keymap::list_keymap_lookup_seq_unresolved(
                &map,
                &events[start..],
            );
            let Some(replacement) =
                self.resolve_key_sequence_translation_binding(lookup, prompt)?
            else {
                continue;
            };
            return Ok(Some(KeySequenceSuffixTranslation { start, replacement }));
        }

        Ok(None)
    }

    fn translation_map_has_pending_suffix_prefix(&self, map: Value, events: &[Value]) -> bool {
        use crate::emacs_core::keymap::{is_list_keymap, list_keymap_lookup_seq};

        if map.is_nil() || !is_list_keymap(&map) {
            return false;
        }

        (0..events.len())
            .any(|start| is_list_keymap(&list_keymap_lookup_seq(&map, &events[start..])))
    }

    fn apply_translation_map_to_events(
        &mut self,
        map: Value,
        mut translated: Vec<Value>,
        prompt: Value,
    ) -> Result<CurrentKeySequenceTranslation, crate::emacs_core::error::Flow> {
        let mut has_pending_translation_prefix = false;
        let mut application = KeySequenceTranslationApplication::NotApplied;

        if let Some(suffix_translation) =
            self.lookup_key_sequence_suffix_translation(map, &translated, prompt)?
        {
            translated.truncate(suffix_translation.start);
            translated.extend(suffix_translation.replacement);
            application = KeySequenceTranslationApplication::Applied;
        }
        has_pending_translation_prefix |=
            self.translation_map_has_pending_suffix_prefix(map, &translated);

        Ok(CurrentKeySequenceTranslation {
            translated_events: translated,
            has_pending_translation_prefix,
            application,
        })
    }

    fn translate_upper_case_key_bindings_enabled(&self) -> bool {
        self.eval_symbol("translate-upper-case-key-bindings")
            .is_ok_and(|value| !value.is_nil())
    }

    /// Fetch the effective value of `echo-keystrokes` as seconds.
    /// Returns `None` when the variable is nil/unbound or of a
    /// wrong type. Mirrors GNU `keyboard.c` consumers which call
    /// `FIXNUMP` / `FLOATP` before using the value.
    fn lisp_echo_keystrokes_seconds(&self) -> Option<f64> {
        let value = self.eval_symbol("echo-keystrokes").ok()?;
        if value.is_nil() {
            return None;
        }
        value.as_number_f64()
    }

    /// Whether GNU's keyboard reader considers the input interactive enough
    /// to display a prompt or pending key sequence.
    ///
    /// GNU `INTERACTIVE` is false in batch mode and while replaying a keyboard
    /// macro (`commands.h`).  Its delayed prefix-echo path separately rejects
    /// `noninteractive` (`keyboard.c:2854-2863`).  Neomacs currently renders a
    /// positive `echo-keystrokes` value immediately, so applying the stronger
    /// `INTERACTIVE` gate here also prevents a fast keyboard macro from
    /// spuriously materializing echo-area buffers.
    fn keyboard_input_is_interactive(&self) -> bool {
        !self.noninteractive()
            && self
                .eval_symbol("executing-kbd-macro")
                .is_ok_and(|value| value.is_nil())
    }

    /// Return true if EVENT matches `help-char` (default `Ctrl-h`
    /// == 8) or any entry in `help-event-list`.
    ///
    /// Mirrors GNU `keyboard.c:3014-3031` (`help_char_p`): the
    /// predicate used by `read_key_sequence` to decide whether an
    /// incoming event is a "help event" for the purposes of
    /// dispatching `prefix-help-command`.
    fn event_matches_help_char(&self, event: &Value) -> bool {
        let help_char = self.eval_symbol("help-char").unwrap_or(Value::NIL);
        if !help_char.is_nil() && *event == help_char {
            return true;
        }
        let help_event_list = self.eval_symbol("help-event-list").unwrap_or(Value::NIL);
        if help_event_list.is_nil() {
            return false;
        }
        let mut cursor = help_event_list;
        while cursor.is_cons() {
            if cursor.cons_car() == *event {
                return true;
            }
            cursor = cursor.cons_cdr();
        }
        false
    }

    fn prefix_echo_message(&mut self, translated_events: &[Value]) -> Option<LispString> {
        let key_vec = Value::vector(translated_events.to_vec());
        let desc =
            crate::emacs_core::builtins::keymaps::builtin_key_description(vec![key_vec]).ok()?;
        let mut message = desc.as_lisp_string()?.clone();
        if translated_events.len() == 1
            && translated_events
                .first()
                .is_some_and(|event| self.event_matches_help_char(event))
        {
            // GNU keyboard.c::echo_add_key appends this when a help event is
            // the first echoed key, while waiting for the following help-map key.
            message = message.concat(&help_event_echo_suffix());
        } else {
            // GNU keyboard.c::echo_dash turns a pending prefix into a
            // mini-prompt for the next key, then help.el appends the default
            // keystroke-help hint when `echo-keystrokes-help' is enabled.
            message = message.concat(&LispString::from_utf8("-"));
            if self
                .eval_symbol("echo-keystrokes-help")
                .unwrap_or(Value::NIL)
                .is_truthy()
            {
                // GNU `echo_dash` delegates this decision to Lisp.  That
                // function walks `help-event-list` (not merely `help-char`),
                // rejects help keys shadowed by the pending binding, and adds
                // the `help-key-binding` face.  `read-quoted-char` dynamically
                // binds `help-char` to nil, so this is what selects <f1>.
                let function = Value::symbol("help--append-keystrokes-help");
                if self.function_value_is_callable(&function)
                    && let Ok(appended) =
                        self.funcall_general(function, vec![Value::heap_string(message.clone())])
                    && let Some(appended) = appended.as_lisp_string()
                {
                    message = appended.clone();
                } else if let Some(help_desc) = self.first_key_echo_help_description() {
                    message =
                        message.concat(&LispString::from_utf8(&format!(" ({help_desc} for help)")));
                }
            }
        }
        Some(message)
    }

    /// Source-bootstrap fallback for contexts where `help.el` has not defined
    /// `help--append-keystrokes-help` yet. Full runtimes always take the Lisp
    /// path above, which owns active-map filtering and text properties.
    fn first_key_echo_help_description(&self) -> Option<String> {
        let help_char = self.eval_symbol("help-char").unwrap_or(Value::NIL);
        let mut events = self.eval_symbol("help-event-list").unwrap_or(Value::NIL);
        while events.is_cons() {
            let mut event = events.cons_car();
            events = events.cons_cdr();
            if event.is_symbol_named("help") {
                event = help_char;
            }
            if event.is_nil() {
                continue;
            }
            let desc =
                crate::emacs_core::builtins::keymaps::builtin_key_description(vec![Value::vector(
                    vec![event],
                )])
                .ok()?;
            return desc.as_utf8_str().map(ToOwned::to_owned);
        }
        None
    }

    /// Publish a keyboard-owned echo message and remember enough typed state
    /// to rebuild it when the next input event arrives.  `set_current_message`
    /// deliberately cancels any previous keyboard ownership, so ownership is
    /// installed only after the new message has been committed.
    fn publish_key_echo_message(&mut self, events: &[Value], prompt: Option<LispString>) {
        let Some(echo) = self.prefix_echo_message(events) else {
            return;
        };
        let message = match prompt.as_ref() {
            Some(prompt) => prompt.concat(&echo),
            None => echo,
        };
        self.set_current_message(Some(message));
        self.command_loop.keyboard.kboard.key_echo_state = KeyEchoState::Immediate { prompt };
    }

    /// Rebuild the complete pending keyboard echo after GNU `read_char` has
    /// cleared the previous message and appended a newly read event.
    fn refresh_immediate_key_echo(&mut self) {
        let prompt = match &self.command_loop.keyboard.kboard.key_echo_state {
            KeyEchoState::Inactive => return,
            KeyEchoState::Immediate { prompt } => prompt.clone(),
        };
        let events = self.command_loop.read_command_keys().to_vec();
        if events.is_empty() {
            return;
        }
        self.publish_key_echo_message(&events, prompt);
    }

    /// Arm GNU's delayed key echo only for an unbounded interactive read that
    /// already belongs to a command.  Timed reads, macro playback, ordinary
    /// messages, and fast input all suppress the delayed echo exactly by
    /// construction.
    fn delayed_key_echo_deadline(
        &self,
        timeout: Option<std::time::Duration>,
    ) -> Option<std::time::Instant> {
        if timeout.is_some()
            || !self.keyboard_input_is_interactive()
            || !matches!(
                self.command_loop.keyboard.kboard.key_echo_state,
                KeyEchoState::Inactive
            )
            || self.current_message_value().is_some()
            || self.command_loop.read_command_keys().is_empty()
        {
            return None;
        }
        let seconds = self
            .lisp_echo_keystrokes_seconds()
            .filter(|seconds| seconds.is_finite() && *seconds > 0.0)?;
        std::time::Instant::now().checked_add(std::time::Duration::from_secs_f64(seconds))
    }

    pub(crate) fn cancel_key_echo_state(&mut self) {
        self.command_loop.keyboard.kboard.key_echo_state = KeyEchoState::Inactive;
    }

    fn prepend_unread_post_input_method_events(
        &mut self,
        events: Value,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        if events.is_nil() {
            return Ok(());
        }

        let current = self
            .eval_symbol("unread-post-input-method-events")
            .unwrap_or(Value::NIL);
        let new_list = crate::emacs_core::builtins::builtin_nconc(vec![events, current])?;
        self.assign("unread-post-input-method-events", new_list);
        Ok(())
    }

    /// Invoke `input-method-function` on a just-read character event.
    ///
    /// Mirrors GNU `keyboard.c:3237-3311`: when the function returns a cons,
    /// the car becomes the current event immediately and only the cdr is
    /// appended ahead of the existing `unread-post-input-method-events`.  Any
    /// non-cons result means the input method produced no events, so the
    /// original character is consumed and the caller retries input.
    ///
    /// Keyboard audit Finding 10.
    fn maybe_apply_input_method_function(
        &mut self,
        event: Value,
    ) -> Result<InputMethodEvent, crate::emacs_core::error::Flow> {
        // Only translate ordinary printable characters. GNU
        // skips non-character events (function keys, mouse,
        // etc.) and the NUL byte. We apply the same filter.
        let Some(c) = event.as_fixnum() else {
            return Ok(InputMethodEvent::NotApplied);
        };
        // GNU keyboard.c::read_char only calls input-method-function for
        // single-byte printable characters: ' ' <= c < 256 && c != DEL.
        // Control bytes such as ESC must remain ordinary key-sequence input
        // so terminal Meta prefixes keep working.
        if !(i64::from(b' ') <= c && c < 256 && c != 127) {
            return Ok(InputMethodEvent::NotApplied);
        }

        let im_fn = self
            .eval_symbol("input-method-function")
            .unwrap_or(Value::NIL);
        if im_fn.is_nil() {
            return Ok(InputMethodEvent::NotApplied);
        }

        // Guard against recursive input-method invocation. GNU
        // uses the `immediate_echo` flag; we use a dedicated bool
        // on CommandLoop so a pathological input-method that
        // re-reads input via `read-event` does not re-enter.
        if self.command_loop.keyboard.kboard.in_input_method_function {
            return Ok(InputMethodEvent::NotApplied);
        }
        self.command_loop.keyboard.kboard.in_input_method_function = true;
        let call_result = self.apply(im_fn, vec![event]);
        self.command_loop.keyboard.kboard.in_input_method_function = false;
        let result = call_result?;

        if !result.is_cons() {
            return Ok(InputMethodEvent::Consumed);
        }

        let first = result.cons_car();
        self.prepend_unread_post_input_method_events(result.cons_cdr())?;
        Ok(InputMethodEvent::Translated(first))
    }

    /// Dispatch `prefix-help-command` via `call-interactively`,
    /// abandoning the current key sequence. Returns `Ok(Some(..))`
    /// when the help command ran (the read_key_sequence caller
    /// should forward the empty sequence to the command loop) or
    /// `Ok(None)` when `prefix-help-command` is unbound (fall
    /// back to the ordinary lookup path).
    ///
    /// Mirrors GNU `keyboard.c:10188-10250`: pop the help-char
    /// from the current sequence so the help command sees the
    /// prefix as `(this-command-keys)`, then run
    /// `Fcall_interactively (Vprefix_help_command, Qnil, Qnil)`.
    ///
    /// Keyboard audit Finding 5.
    fn dispatch_prefix_help_command(
        &mut self,
        delayed_selection_event: &mut Option<Value>,
    ) -> Result<Option<(Vec<Value>, Value)>, crate::emacs_core::error::Flow> {
        let prefix_help_command = self
            .eval_symbol("prefix-help-command")
            .unwrap_or(Value::NIL);
        if prefix_help_command.is_nil() {
            return Ok(None);
        }
        // Pop the trailing help-char event from the current raw
        // sequence and from the translated event buffer. GNU's
        // read_key_sequence removes the help event before running
        // the help command so `(this-command-keys)` reports the
        // prefix only — matching the classic "C-x ?" behaviour.
        self.command_loop
            .keyboard
            .kboard
            .current_key_sequence
            .pop_last_events_for_help_char();

        self.restore_delayed_selection_event(delayed_selection_event);

        // Run the help command via call-interactively so advice
        // and `this-command` bookkeeping in the Lisp interactive
        // dispatcher stay consistent with every other interactive
        // command.
        let _ = self.apply(
            Value::symbol("call-interactively"),
            vec![prefix_help_command],
        )?;

        // Return an empty key sequence — command_loop_1 treats
        // that as "nothing to dispatch this tick" and immediately
        // reads the next key.
        self.command_loop
            .set_command_key_sequences(Vec::new(), Vec::new());
        Ok(Some((Vec::new(), Value::NIL)))
    }

    fn shift_translated_key_sequence_event(event: Value) -> Option<Value> {
        let key_event = crate::emacs_core::keymap::emacs_event_to_key_event(&event)?;

        match key_event {
            crate::emacs_core::keymap::KeyEvent::Char {
                code,
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            } => {
                if shift {
                    return Some(crate::emacs_core::keymap::key_event_to_emacs_event(
                        &crate::emacs_core::keymap::KeyEvent::Char {
                            code,
                            ctrl,
                            meta,
                            shift: false,
                            super_,
                            hyper,
                            alt,
                        },
                    ));
                }

                if !code.is_uppercase() {
                    return None;
                }

                let lowered = code.to_lowercase().next().unwrap_or(code);
                if lowered == code {
                    return None;
                }

                Some(crate::emacs_core::keymap::key_event_to_emacs_event(
                    &crate::emacs_core::keymap::KeyEvent::Char {
                        code: lowered,
                        ctrl,
                        meta,
                        shift,
                        super_,
                        hyper,
                        alt,
                    },
                ))
            }
            crate::emacs_core::keymap::KeyEvent::Function {
                name,
                ctrl,
                meta,
                shift,
                super_,
                hyper,
                alt,
            } => {
                if !shift {
                    return None;
                }

                Some(crate::emacs_core::keymap::key_event_to_emacs_event(
                    &crate::emacs_core::keymap::KeyEvent::Function {
                        name,
                        ctrl,
                        meta,
                        shift: false,
                        super_,
                        hyper,
                        alt,
                    },
                ))
            }
        }
    }

    fn apply_shift_translation_to_current_key_sequence(
        &mut self,
    ) -> Option<KeySequenceShiftTranslation> {
        let translated = self
            .command_loop
            .keyboard
            .kboard
            .current_key_sequence
            .translated_events()
            .to_vec();
        let (index, original_event) = translated
            .len()
            .checked_sub(1)
            .map(|index| (index, translated[index]))?;
        let translated_event = Self::shift_translated_key_sequence_event(original_event)?;
        let mut rewritten = translated;
        rewritten[index] = translated_event;
        self.command_loop
            .keyboard
            .rewrite_key_sequence_translation(rewritten);
        Some(KeySequenceShiftTranslation {
            index,
            original_event,
        })
    }

    fn finalize_shift_translated_key_sequence(
        &mut self,
        sequence_is_undefined: bool,
        options: ReadKeySequenceOptions,
        shift_translation: Option<KeySequenceShiftTranslation>,
    ) {
        let mut shift_translated = false;

        if let Some(shift_translation) = shift_translation {
            let current_len = self
                .command_loop
                .keyboard
                .kboard
                .current_key_sequence
                .translated_events()
                .len();
            let restore_original = (options.dont_downcase_last || sequence_is_undefined)
                && shift_translation.index + 1 == current_len;
            if restore_original {
                let mut translated = self
                    .command_loop
                    .keyboard
                    .kboard
                    .current_key_sequence
                    .translated_events()
                    .to_vec();
                translated[shift_translation.index] = shift_translation.original_event;
                self.command_loop
                    .keyboard
                    .rewrite_key_sequence_translation(translated);
            } else {
                shift_translated = true;
            }
        }

        self.assign(
            "this-command-keys-shift-translated",
            if shift_translated {
                Value::T
            } else {
                Value::NIL
            },
        );
    }

    pub(crate) fn apply_resize_input_event(
        &mut self,
        width: u32,
        height: u32,
        scale_factor: f64,
        emacs_frame_id: u64,
        trigger_redisplay: bool,
    ) {
        let trace_frame_geometry = std::env::var("NEOMACS_TRACE_FRAME_GEOMETRY")
            .ok()
            .is_some_and(|value| value == "1");
        let target_fid = if emacs_frame_id == 0 {
            self.frames.selected_frame().map(|frame| frame.id)
        } else {
            Some(crate::window::FrameId(emacs_frame_id))
        };
        let selected_fid = self.frames.selected_frame().map(|selected| selected.id);
        tracing::debug!(
            "apply_resize_input_event: {}x{} emacs_frame_id=0x{:x} target_fid={:?}",
            width,
            height,
            emacs_frame_id,
            target_fid
        );
        if let Some(fid) = target_fid {
            if trace_frame_geometry && let Some(frame) = self.frames.get(fid) {
                tracing::debug!(
                    "apply_resize_input_event: before fid={:?} selected={:?} size={}x{} effective_ws={:?} param_ws={:?}",
                    fid,
                    selected_fid,
                    frame.width,
                    frame.height,
                    frame.effective_window_system(),
                    frame.parameter("window-system")
                );
            }
            apply_resize_input_event_in_keyboard_runtime(
                &mut self.frames,
                &self.buffers,
                width,
                height,
                scale_factor,
                emacs_frame_id,
            );
            if let Some(frame) = self.frames.get(fid) {
                tracing::debug!(
                    "apply_resize_input_event: resized frame {:?} to {}x{}",
                    fid,
                    frame.width,
                    frame.height
                );
                if trace_frame_geometry {
                    tracing::debug!(
                        "apply_resize_input_event: after fid={:?} selected={:?} size={}x{} effective_ws={:?} param_ws={:?}",
                        fid,
                        selected_fid,
                        frame.width,
                        frame.height,
                        frame.effective_window_system(),
                        frame.parameter("window-system")
                    );
                }
            }
        }
        if trigger_redisplay {
            self.redisplay();
        }
    }

    pub(crate) fn sync_pending_resize_events(&mut self) -> bool {
        let applied_resize = sync_pending_resize_events_in_keyboard_runtime(
            &mut self.frames,
            &self.buffers,
            &mut self.input_rx,
            &mut self.command_loop.keyboard,
        );
        sync_opening_gui_frame_size_from_host_in_keyboard_runtime(
            &mut self.frames,
            &self.buffers,
            self.display_host.as_deref(),
        );
        applied_resize
    }

    pub(crate) fn wait_for_pending_resize_events(&mut self, timeout: Duration) -> bool {
        let resize_acknowledged = self
            .wait_for_resize_ack_until(Instant::now() + timeout)
            .unwrap_or(false);
        sync_opening_gui_frame_size_from_host_in_keyboard_runtime(
            &mut self.frames,
            &self.buffers,
            self.display_host.as_deref(),
        );
        resize_acknowledged
    }

    /// Whether `event` should be drained here by the wait-request special-input
    /// service. Uses the frontend-event policy, except a
    /// `MouseMove` counts as special only when track-mouse is OFF.
    ///
    /// GNU keeps a pending mouse motion as readable command input while
    /// track-mouse is on (keyboard.c `some_mouse_moved` / `readable_events`):
    /// the motion must stay queued so the pending-input query (and the
    /// `read_char` path) can see it, rather than being silently consumed as a
    /// bare cursor-position update here. With track-mouse off it is a pure
    /// position update and is consumed as before.
    fn input_event_is_wait_request_special_now(&self, event: &InputEvent) -> bool {
        crate::frontend_events::is_wait_special(event, self.track_mouse_enabled())
    }

    fn take_next_wait_request_special_input_event(
        &mut self,
        internal_effects: &mut crate::frontend_events::InternalEventEffects,
    ) -> Result<Option<InputEvent>, crate::emacs_core::error::Flow> {
        *internal_effects =
            (*internal_effects).merge(self.service_leading_internal_frontend_events());
        if let Some(event) = self
            .command_loop
            .keyboard
            .pending_input_events
            .front()
            .cloned()
        {
            if self.input_event_is_wait_request_special_now(&event) {
                self.command_loop
                    .keyboard
                    .pending_input_events
                    .pop_visible_front();
                self.timer_stop_idle();
                return Ok(Some(event));
            }
            return Ok(None);
        }

        if self.stage_next_host_input_event_if_available()? {
            *internal_effects =
                (*internal_effects).merge(self.service_leading_internal_frontend_events());
            if let Some(event) = self
                .command_loop
                .keyboard
                .pending_input_events
                .front()
                .cloned()
                && self.input_event_is_wait_request_special_now(&event)
            {
                self.command_loop
                    .keyboard
                    .pending_input_events
                    .pop_visible_front();
                self.timer_stop_idle();
                return Ok(Some(event));
            }
            return Ok(None);
        }

        Ok(None)
    }

    pub(crate) fn stage_next_host_input_event_if_available(
        &mut self,
    ) -> Result<bool, crate::emacs_core::error::Flow> {
        let Some(ref rx) = self.input_rx else {
            return Ok(false);
        };
        match rx.try_recv() {
            Ok(event) => {
                self.command_loop
                    .keyboard
                    .pending_input_events
                    .push_back(event);
                Ok(true)
            }
            Err(crossbeam_channel::TryRecvError::Empty) => Ok(false),
            Err(crossbeam_channel::TryRecvError::Disconnected) => {
                if std::env::var_os("NEOVM_DEBUG_QUIT").is_some() {
                    eprintln!(
                        "[quit-debug] input channel Disconnected -> quit; backtrace:\n{}",
                        std::backtrace::Backtrace::force_capture()
                    );
                }
                self.handle_display_terminal_disconnect();
                Err(crate::emacs_core::error::signal(
                    LispCondition::Quit,
                    vec![],
                ))
            }
        }
    }

    pub(crate) fn wait_for_next_host_input_event(
        &mut self,
        timeout: Duration,
        waiting_for_user_input: bool,
    ) -> Result<bool, crate::emacs_core::error::Flow> {
        let Some(rx) = self.input_rx.clone() else {
            if !timeout.is_zero() {
                std::thread::sleep(timeout);
            }
            return Ok(false);
        };

        let previous_waiting_for_input = self.waiting_for_user_input;
        self.waiting_for_user_input = waiting_for_user_input;
        let wait_result = if timeout.is_zero() {
            rx.try_recv().map_err(|err| match err {
                crossbeam_channel::TryRecvError::Empty => {
                    crossbeam_channel::RecvTimeoutError::Timeout
                }
                crossbeam_channel::TryRecvError::Disconnected => {
                    crossbeam_channel::RecvTimeoutError::Disconnected
                }
            })
        } else {
            rx.recv_timeout(timeout)
        };
        self.waiting_for_user_input = previous_waiting_for_input;

        match wait_result {
            Ok(event) => {
                self.command_loop
                    .keyboard
                    .pending_input_events
                    .push_back(event);
                Ok(true)
            }
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => Ok(false),
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                self.handle_display_terminal_disconnect();
                Err(crate::emacs_core::error::signal(
                    LispCondition::Quit,
                    vec![],
                ))
            }
        }
    }

    pub(crate) fn stage_pending_command_input_for_wait_request(
        &mut self,
    ) -> Result<bool, crate::emacs_core::error::Flow> {
        self.service_leading_internal_frontend_events();
        if self.command_loop.keyboard.has_pending_kboard_input()
            || self.has_pending_frontend_input_with_configured_filter()
        {
            return Ok(true);
        }

        while self.stage_next_host_input_event_if_available()? {
            self.service_leading_internal_frontend_events();
            if self.command_loop.keyboard.has_pending_kboard_input()
                || self.has_pending_frontend_input_with_configured_filter()
            {
                return Ok(true);
            }
        }

        if self.command_loop.keyboard.has_pending_kboard_input() {
            return Ok(true);
        }

        Ok(self.has_pending_frontend_input_with_configured_filter())
    }

    pub(crate) fn service_wait_request_special_input_events(
        &mut self,
    ) -> Result<SpecialInputServiceOutcome, crate::emacs_core::error::Flow> {
        let mut outcome = SpecialInputServiceOutcome::default();
        let mut internal_effects = crate::frontend_events::InternalEventEffects::default();

        crate::emacs_core::builtins::drain_file_notify_events(self)?;

        if self.sync_pending_resize_events() {
            outcome = outcome.merge(SpecialInputServiceOutcome::resize_with_redisplay());
        }

        while let Some(event) =
            self.take_next_wait_request_special_input_event(&mut internal_effects)?
        {
            if crate::frontend_events::interrupts(&event)
                && self.interrupt_for_input_event_if_requested(event.clone())?
            {
                continue;
            }

            match event {
                InputEvent::Resize {
                    width,
                    height,
                    scale_factor,
                    emacs_frame_id,
                } => {
                    outcome = outcome.merge(SpecialInputServiceOutcome::resize_with_redisplay());
                    self.apply_resize_input_event(
                        width,
                        height,
                        scale_factor,
                        emacs_frame_id,
                        false,
                    );
                }
                InputEvent::MonitorsChanged { monitors } => {
                    outcome = outcome.merge(SpecialInputServiceOutcome::any_activity());
                    crate::emacs_core::builtins::set_neomacs_monitor_info(monitors);
                    let hook_sym = crate::emacs_core::hook_runtime::hook_symbol_by_name(
                        self,
                        "display-monitors-changed-functions",
                    );
                    let terminal = crate::emacs_core::terminal::pure::terminal_handle_value();
                    let _ = crate::emacs_core::hook_runtime::run_named_hook(
                        self,
                        hook_sym,
                        &[terminal],
                    )?;
                }
                InputEvent::MouseMove {
                    x,
                    y,
                    target_frame_id,
                    ..
                } => {
                    outcome = outcome.merge(SpecialInputServiceOutcome::any_activity());
                    self.note_mouse_move_input_event(x, y, target_frame_id);
                    self.timer_resume_idle();
                }
                InputEvent::WindowClose { emacs_frame_id } => {
                    outcome = outcome.merge(SpecialInputServiceOutcome::any_activity());
                    self.handle_window_close_input_event(emacs_frame_id)?;
                }
                InputEvent::DisplayReset => {
                    // Like Resize: apply here, let the outcome trigger the
                    // redisplay after servicing.
                    outcome = outcome.merge(SpecialInputServiceOutcome::resize_with_redisplay());
                    self.handle_display_reset_input_event();
                }
                InputEvent::WebView(event) => {
                    if self.apply_xwidget_frontend_event(&event)? {
                        outcome =
                            outcome.merge(SpecialInputServiceOutcome::resize_with_redisplay());
                    }
                }
                InputEvent::SurfaceCreateFailed { id, error } => {
                    // Not user activity and needs no redisplay — just run the
                    // hook (mirrors MonitorsChanged running its hook here).
                    self.handle_surface_create_failed_input_event(id, &error)?;
                }
                InputEvent::FrameShaderFailed { error } => {
                    let effects =
                        crate::frontend_events::report_frame_shader_failure(self, &error)?;
                    outcome =
                        outcome.merge(SpecialInputServiceOutcome::from_internal_effects(effects));
                }
                InputEvent::TerminalCreateFailed { id, error } => {
                    self.handle_terminal_create_failed_input_event(id, &error)?;
                }
                InputEvent::TerminalExited { id } => {
                    self.handle_terminal_exited_input_event(id)?;
                }
                InputEvent::TerminalTitleChanged { id, title } => {
                    self.handle_terminal_title_changed_input_event(id, &title)?;
                }
                _ => {}
            }
        }

        outcome = outcome.merge(SpecialInputServiceOutcome::from_internal_effects(
            internal_effects,
        ));

        Ok(outcome)
    }

    /// The display lost its GPU device and rebuilt from scratch
    /// (`InputEvent::DisplayReset`). Ask the display host to re-resolve
    /// everything renderer-resident (media memos, images, the frame
    /// shader), then bust the redisplay signature: nothing in buffer or
    /// window state changed, so without this the next redisplay would
    /// early-return on an unchanged signature and never republish the
    /// frame. (The WindowResize path gets the signature change for free
    /// from the geometry update.)
    fn handle_display_reset_input_event(&mut self) {
        tracing::warn!(
            "display reset: re-resolving GPU-resident media and forcing a full redisplay"
        );
        if let Some(host) = self.display_host.as_deref() {
            host.display_reset();
        }
        self.invalidate_redisplay();
    }

    /// A shader surface failed to build on the render thread past naga
    /// pre-validation (a device-specific wgpu rejection). Run
    /// `neomacs-surface-error-functions` with the surface id and the error
    /// string so Lisp can report it (the default member messages the user);
    /// without this the failure only appears in a log line while the quad
    /// renders blank. Mirrors `MonitorsChanged` running its hook here.
    fn handle_surface_create_failed_input_event(
        &mut self,
        id: u32,
        error: &str,
    ) -> crate::emacs_core::error::EvalResult {
        let args = [
            Value::symbol("neomacs-surface-error-functions"),
            Value::fixnum(i64::from(id)),
            Value::string(error),
        ];
        crate::emacs_core::hook_runtime::run_named_hook_with_args(self, &args)
    }

    fn handle_terminal_exited_input_event(
        &mut self,
        id: crate::emacs_core::display_host::TerminalId,
    ) -> crate::emacs_core::error::EvalResult {
        let args = [
            Value::symbol("neo-term-exit-functions"),
            Value::fixnum(i64::from(id.get())),
        ];
        crate::emacs_core::hook_runtime::run_named_hook_with_args(self, &args)
    }

    fn handle_terminal_create_failed_input_event(
        &mut self,
        id: crate::emacs_core::display_host::TerminalId,
        error: &str,
    ) -> crate::emacs_core::error::EvalResult {
        let args = [
            Value::symbol("neo-term-create-failed-functions"),
            Value::fixnum(i64::from(id.get())),
            Value::string(error),
        ];
        crate::emacs_core::hook_runtime::run_named_hook_with_args(self, &args)
    }

    fn handle_terminal_title_changed_input_event(
        &mut self,
        id: crate::emacs_core::display_host::TerminalId,
        title: &str,
    ) -> crate::emacs_core::error::EvalResult {
        let args = [
            Value::symbol("neo-term-title-changed-functions"),
            Value::fixnum(i64::from(id.get())),
            Value::string(title),
        ];
        crate::emacs_core::hook_runtime::run_named_hook_with_args(self, &args)
    }

    fn handle_window_close_input_event(
        &mut self,
        emacs_frame_id: u64,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        self.timer_resume_idle();
        if let Some(event) = self.make_lispy_delete_frame_event(emacs_frame_id)
            && self.execute_special_event_if_bound(event)?
        {
            return Ok(());
        }
        self.command_loop.running = false;
        Err(crate::emacs_core::error::signal(
            LispCondition::Quit,
            vec![],
        ))
    }

    /// Read a complete key sequence through keymaps.
    ///
    /// Mirrors GNU Emacs `read_key_sequence()` (keyboard.c:10098). Reads keys
    /// one at a time, following prefix keymaps until a complete binding or
    /// undefined key is found. Accepting fresh input also clears a previous
    /// logical echo-area message; GNU's `read_menu_command` deliberately uses
    /// this same reader.
    ///
    /// After each key, checks translation maps in order:
    /// 1. `input-decode-map` — terminal-specific key decoding
    /// 2. `local-function-key-map` (inherits `function-key-map`) — function
    ///    key translation
    /// 3. `key-translation-map` — user-defined key translations
    ///
    /// Returns `(key_events_as_emacs_values, binding)`; binding is nil if the
    /// key sequence is undefined.
    pub(crate) fn read_key_sequence(
        &mut self,
    ) -> Result<(Vec<Value>, Value), crate::emacs_core::error::Flow> {
        self.read_key_sequence_with_options(ReadKeySequenceOptions::default())
    }

    /// Read one command-loop key sequence, preserving GNU's typed distinction
    /// between a command and the zero-length result that ends a macro
    /// iteration.  Lisp-facing key readers continue to use
    /// `read_key_sequence_with_options`, because an exhausted macro is only a
    /// control boundary for `command_loop_1` (GNU keyboard.c/macros.c).
    pub(crate) fn read_command_key_sequence_with_options(
        &mut self,
        options: ReadKeySequenceOptions,
    ) -> Result<CommandKeySequenceRead, crate::emacs_core::error::Flow> {
        if self.command_input_kbd_macro_iteration_is_exhausted() {
            // GNU's `read_key_sequence` reaches its common `done` label and
            // calls `echo_update` before returning zero.  Neomacs does not yet
            // retain GNU's separate immediate-echo string, so the equivalent
            // terminal-cell effect is to finish the transient echo-area
            // message at this reader boundary.
            self.begin_key_sequence_read(options.continue_echo);
            self.command_loop
                .set_command_key_sequences(Vec::new(), Vec::new());
            self.clear_key_echo_message();
            return Ok(CommandKeySequenceRead::End(
                CommandKeySequenceEnd::KeyboardMacroIteration,
            ));
        }

        let (keys, binding) = self.read_key_sequence_with_options(options)?;
        if keys.is_empty() && binding.is_nil() {
            Ok(CommandKeySequenceRead::End(CommandKeySequenceEnd::Input))
        } else {
            Ok(CommandKeySequenceRead::Command { keys, binding })
        }
    }

    fn command_input_kbd_macro_iteration_is_exhausted(&self) -> bool {
        // GNU `at_end_of_macro_p` treats Lisp-visible
        // `executing-kbd-macro == t` as an explicit early-termination request.
        // Requeued events and unread selection/input-method events must drain
        // before the macro boundary is observable.
        let visible_macro_forces_end = self
            .visible_variable_value_or_nil("executing-kbd-macro")
            .is_t();
        let runtime_macro_at_end = matches!(
            self.command_loop.keyboard.kboard.executing_kbd_macro.as_ref(),
            Some(events) if self.command_loop.keyboard.kboard.kbd_macro_index >= events.len()
        );
        let macro_at_end = visible_macro_forces_end || runtime_macro_at_end;
        macro_at_end
            && !self.has_pending_requeued_events()
            && self
                .command_loop
                .keyboard
                .kboard
                .unread_selection_event
                .is_none()
            && self.command_loop.keyboard.kboard.unread_events.is_empty()
    }

    /// The prologue GNU `read_key_sequence` runs before it reads anything, at
    /// and after its `replay_sequence:` label (keyboard.c:11038-11054).
    ///
    /// Every key-sequence read starts here, including the zero-length read that
    /// discovers an exhausted keyboard macro, so the effects below are stated
    /// once above that dispatch rather than repeated in each branch.
    ///
    /// * `reset_key_sequence` clears the in-progress accumulator (GNU `t = 0`).
    /// * CONTINUE-ECHO nil also clears the COMMITTED `this-command-keys`
    ///   (`this_command_key_count = 0; this_single_command_key_start = 0;`,
    ///   keyboard.c:11919-11923), so the sequence starts fresh.  neomacs's
    ///   `reset_key_sequence` alone would leave `command_keys`/`raw_command_keys`
    ///   in place, and a command's internal `read-key` (subr.el) — which reads
    ///   with CONTINUE-ECHO nil and arms an idle timer that throws as soon as
    ///   `(this-command-keys-vector)` is non-empty (subr.el:3648-3665) — would
    ///   observe the STALE invoking sequence and return that key immediately
    ///   instead of waiting for the next keystroke (neomacs#187).
    /// * `last_nonmenu_event = Qnil` (keyboard.c:11054).  The variable holds the
    ///   key of the sequence CURRENTLY being read, assigned further down
    ///   (keyboard.c:11668-11673); it is not a durable record of the last key
    ///   pressed.  Lisp reads the cleared value after a keyboard macro finishes,
    ///   and code branches on its type: `imenu-choose-buffer-index`
    ///   (lisp/imenu.el:915) picks the mouse menu over the completing-read
    ///   prompt when `(listp last-nonmenu-event)`, so a leftover integer turns a
    ///   silent GNU `imenu` into a minibuffer prompt.
    fn begin_key_sequence_read(&mut self, continue_echo: bool) {
        self.command_loop.reset_key_sequence();
        if !continue_echo {
            self.clear_read_command_keys();
        }
        self.assign("last-nonmenu-event", Value::NIL);
    }

    pub(crate) fn read_key_sequence_with_options(
        &mut self,
        options: ReadKeySequenceOptions,
    ) -> Result<(Vec<Value>, Value), crate::emacs_core::error::Flow> {
        use crate::emacs_core::keymap::{
            DefaultBindingMode, resolve_active_key_binding,
            resolve_prefix_keymap_binding_in_obarray,
        };

        self.sync_keyboard_terminal_owner();
        self.begin_key_sequence_read(options.continue_echo);

        // GNU `read_key_sequence` stashes the PROMPT in
        // `current_kboard->echo_prompt` (keyboard.c:10990) and the echo
        // machinery shows it while reading, so an explicit prompt such as
        // `describe-key`'s "Describe the following key…" is visible before and
        // while the user types the key to describe (neomacs#187). Show it now
        // and prepend it to the typed-key echo below.
        let key_sequence_prompt: Option<LispString> = options
            .prompt
            .as_lisp_string()
            .filter(|prompt| !prompt.is_empty())
            .cloned();
        if self.keyboard_input_is_interactive()
            && let Some(prompt) = key_sequence_prompt.as_ref()
        {
            self.set_current_message(Some(prompt.clone()));
            self.command_loop.keyboard.kboard.key_echo_state = KeyEchoState::Immediate {
                prompt: key_sequence_prompt.clone(),
            };
        }

        self.assign("this-command-keys-shift-translated", Value::NIL);
        let mut shift_translation: Option<KeySequenceShiftTranslation> = None;
        let mut delayed_selection_event: Option<Value> = None;
        let mut saved_current_buffer: Option<crate::buffer::BufferId> = None;
        let mut replay_current_sequence = false;

        loop {
            if replay_current_sequence {
                replay_current_sequence = false;
                tracing::debug!("read_key_sequence: replaying buffered sequence");
            } else {
                let read_event = match self.read_char_event_with_timeout_for_key_sequence() {
                    Ok(Some(event)) => event,
                    Ok(None) => {
                        // GNU command_loop_2 treats EOF from command input as
                        // a nil return from command_loop_1. In a no-receiver
                        // context, looping here just retries the same EOF
                        // forever after queued events are exhausted.
                        self.restore_delayed_selection_event(&mut delayed_selection_event);
                        self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                        self.command_loop
                            .set_command_key_sequences(Vec::new(), Vec::new());
                        return Ok((Vec::new(), Value::NIL));
                    }
                    Err(err) => {
                        self.restore_delayed_selection_event(&mut delayed_selection_event);
                        self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                        return Err(err);
                    }
                };
                let mut emacs_event = read_event.event;
                self.clear_quit_flag_after_read_key_sequence_event(&emacs_event);
                if Self::has_switch_frame_event_kind(&emacs_event)
                    && (!self
                        .command_loop
                        .keyboard
                        .kboard
                        .current_key_sequence
                        .raw_events()
                        .is_empty()
                        || !options.can_return_switch_frame)
                {
                    delayed_selection_event = Some(emacs_event);
                    tracing::debug!("read_key_sequence: deferring selection event");
                    continue;
                }

                // Keyboard audit Finding 10: input-method-function.
                // GNU `keyboard.c:3237-3311` applies the input method only to
                // the first printable character in a key sequence, replaces
                // that current event with the returned car, and queues the
                // returned cdr in `unread-post-input-method-events`.  Events
                // read from that post-input-method queue explicitly bypass
                // this call on their second pass.
                if read_event.allow_input_method
                    && self
                        .command_loop
                        .keyboard
                        .kboard
                        .current_key_sequence
                        .raw_events()
                        .is_empty()
                {
                    match self.maybe_apply_input_method_function(emacs_event)? {
                        InputMethodEvent::NotApplied => {}
                        InputMethodEvent::Consumed => {
                            replay_current_sequence = false;
                            shift_translation = None;
                            continue;
                        }
                        InputMethodEvent::Translated(event) => {
                            emacs_event = event;
                        }
                    }
                }

                self.command_loop
                    .keyboard
                    .push_key_sequence_input_event(emacs_event);
                self.publish_current_key_sequence_as_command_keys();

                // GNU `read_char` updates `last-input-event` for every accepted
                // event before keymap lookup, including keyboard-macro events.
                // The recording layer independently suppresses macro playback
                // from recent-keys and the non-macro input counter.
                self.record_input_event(emacs_event);

                tracing::debug!(
                    "read_key_sequence: event={} starting translation",
                    crate::emacs_core::print::print_value(&emacs_event)
                );
            }

            let mut translated_events = self
                .command_loop
                .keyboard
                .kboard
                .current_key_sequence
                .translated_events()
                .to_vec();

            let input_decode_translation = match self.apply_translation_map_to_events(
                self.command_loop.keyboard.input_decode_map(),
                translated_events,
                options.prompt,
            ) {
                Ok(translation) => translation,
                Err(err) => {
                    self.restore_delayed_selection_event(&mut delayed_selection_event);
                    return Err(err);
                }
            };
            translated_events = input_decode_translation.translated_events;
            self.command_loop
                .keyboard
                .rewrite_key_sequence_translation(translated_events.clone());
            self.publish_current_key_sequence_as_command_keys();

            if self
                .command_loop
                .keyboard
                .kboard
                .current_key_sequence
                .raw_events()
                .len()
                == 1
                && let Some(prefixed) = Self::maybe_prefix_mouse_area(&translated_events)
            {
                self.command_loop
                    .keyboard
                    .rewrite_key_sequence_translation(prefixed.clone());
                translated_events = prefixed;
                self.publish_current_key_sequence_as_command_keys();
            }

            let lookup_position = Self::key_sequence_lookup_position(&translated_events);
            let pre_function_key_resolution = match resolve_active_key_binding(
                self,
                &translated_events,
                DefaultBindingMode::Accept,
                false,
                lookup_position.as_ref(),
            ) {
                Ok(resolved) => resolved,
                Err(err) => {
                    self.restore_delayed_selection_event(&mut delayed_selection_event);
                    self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                    return Err(err);
                }
            };
            let pre_function_key_binding = pre_function_key_resolution.binding;
            let pre_function_key_undefined = pre_function_key_resolution.lookup.is_nil()
                || pre_function_key_resolution.lookup == Value::symbol("undefined")
                || pre_function_key_binding.is_nil()
                || pre_function_key_binding == Value::symbol("undefined");
            let pre_function_key_is_prefix = resolve_prefix_keymap_binding_in_obarray(
                &self.obarray,
                &pre_function_key_resolution.lookup,
            )
            .is_some();
            let mut command_binding_resolution =
                CommandBindingResolution::Current(pre_function_key_resolution);

            let mut has_pending_translation_prefix =
                input_decode_translation.has_pending_translation_prefix;
            if pre_function_key_undefined || pre_function_key_is_prefix {
                let function_key_translation = match self.apply_translation_map_to_events(
                    self.command_loop.keyboard.local_function_key_map(),
                    translated_events,
                    options.prompt,
                ) {
                    Ok(translation) => translation,
                    Err(err) => {
                        self.restore_delayed_selection_event(&mut delayed_selection_event);
                        return Err(err);
                    }
                };
                translated_events = function_key_translation.translated_events;
                has_pending_translation_prefix |=
                    function_key_translation.has_pending_translation_prefix;
                command_binding_resolution
                    .invalidate_if_applied(function_key_translation.application);
                self.command_loop
                    .keyboard
                    .rewrite_key_sequence_translation(translated_events.clone());
                self.publish_current_key_sequence_as_command_keys();
            }

            let key_translation = match self.apply_translation_map_to_events(
                self.eval_symbol("key-translation-map")
                    .unwrap_or(Value::NIL),
                translated_events,
                options.prompt,
            ) {
                Ok(translation) => translation,
                Err(err) => {
                    self.restore_delayed_selection_event(&mut delayed_selection_event);
                    return Err(err);
                }
            };
            translated_events = key_translation.translated_events;
            has_pending_translation_prefix |= key_translation.has_pending_translation_prefix;
            command_binding_resolution.invalidate_if_applied(key_translation.application);
            self.command_loop
                .keyboard
                .rewrite_key_sequence_translation(translated_events.clone());
            self.publish_current_key_sequence_as_command_keys();

            // GNU `keyboard.c:11668-11673`: after keyremap / function-key-map
            // translation, `last_nonmenu_event = key` holds the TRANSLATED key
            // (e.g. the GUI `<return>` becomes RET/13), not the raw event, unless
            // a mouse popup menu was used to read it (neomacs does not synthesize
            // those yet). `last-input-event` (recorded raw above) is unchanged;
            // only `last-nonmenu-event` takes the translated event -- matching
            // GNU, where reading an unbound `<return>` yields
            // last-input-event=return but last-nonmenu-event=13.
            if let Some(translated_last) = translated_events.last() {
                self.record_nonmenu_input_event(*translated_last);
            }

            if self
                .command_loop
                .keyboard
                .kboard
                .current_key_sequence
                .raw_events()
                .len()
                == 1
                && saved_current_buffer.is_none()
                && let Some(target_buffer_id) =
                    Self::key_sequence_target_buffer_id(&translated_events, &self.frames)
                && self.buffers.current_buffer_id() != Some(target_buffer_id)
            {
                saved_current_buffer = self.buffers.current_buffer_id();
                if let Err(err) = self.switch_current_buffer(target_buffer_id) {
                    self.restore_delayed_selection_event(&mut delayed_selection_event);
                    self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                    return Err(err);
                }
                command_binding_resolution.invalidate();
            }
            let lookup_position = Self::key_sequence_lookup_position(&translated_events);

            tracing::debug!(
                "read_key_sequence: looking up binding for {:?}",
                translated_events
                    .iter()
                    .map(crate::emacs_core::print::print_value)
                    .collect::<Vec<_>>()
            );
            let mut resolved = match command_binding_resolution {
                CommandBindingResolution::Current(resolved) => resolved,
                CommandBindingResolution::Stale => {
                    match resolve_active_key_binding(
                        self,
                        &translated_events,
                        DefaultBindingMode::Accept,
                        false,
                        lookup_position.as_ref(),
                    ) {
                        Ok(resolved) => resolved,
                        Err(err) => {
                            self.restore_delayed_selection_event(&mut delayed_selection_event);
                            self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                            return Err(err);
                        }
                    }
                }
            };
            let mut binding = resolved.binding;
            let mut lookup_is_undefined =
                resolved.lookup.is_nil() || resolved.lookup == Value::symbol("undefined");
            let mut binding_is_undefined =
                binding.is_nil() || binding == Value::symbol("undefined");
            let mut sequence_is_undefined = lookup_is_undefined || binding_is_undefined;
            tracing::debug!(
                "read_key_sequence: binding={}",
                crate::emacs_core::print::print_value(&binding)
            );

            if sequence_is_undefined {
                if has_pending_translation_prefix {
                    tracing::debug!(
                        "read_key_sequence: continuing because translation suffix is still a prefix"
                    );
                    continue;
                }
                if let Some(fallback) = self.resolve_undefined_mouse_sequence_fallback(
                    &translated_events,
                    lookup_position.as_ref(),
                )? {
                    match fallback {
                        UndefinedMouseSequenceFallback::Rewrite {
                            events,
                            resolved: rewritten_resolution,
                        } => {
                            tracing::debug!(
                                "read_key_sequence: simplifying undefined mouse event to {:?}",
                                events
                                    .iter()
                                    .map(crate::emacs_core::print::print_value)
                                    .collect::<Vec<_>>()
                            );
                            self.command_loop
                                .keyboard
                                .rewrite_key_sequence_translation(events.clone());
                            translated_events = events;
                            resolved = rewritten_resolution;
                            binding = resolved.binding;
                            lookup_is_undefined = resolved.lookup.is_nil()
                                || resolved.lookup == Value::symbol("undefined");
                            binding_is_undefined =
                                binding.is_nil() || binding == Value::symbol("undefined");
                            sequence_is_undefined = lookup_is_undefined || binding_is_undefined;
                        }
                        UndefinedMouseSequenceFallback::Drop { retained_events } => {
                            tracing::debug!(
                                "read_key_sequence: dropping undefined mouse event and retaining {:?}",
                                retained_events
                                    .iter()
                                    .map(crate::emacs_core::print::print_value)
                                    .collect::<Vec<_>>()
                            );
                            self.command_loop
                                .keyboard
                                .rewrite_key_sequence_events(retained_events);
                            shift_translation = None;
                            continue;
                        }
                    }
                }
                if sequence_is_undefined {
                    // GNU `keyboard.c:11799-11812` dispatches
                    // `prefix-help-command` only after normal keymap
                    // lookup fails.  This matters for `C-h C-h`: the
                    // second `C-h` is a real `help-map` binding for
                    // `help-for-help`, not generic prefix help.
                    if self
                        .command_loop
                        .keyboard
                        .kboard
                        .current_key_sequence
                        .raw_events()
                        .len()
                        > 1
                        && let Some(last_raw) = self
                            .command_loop
                            .keyboard
                            .kboard
                            .current_key_sequence
                            .raw_events()
                            .last()
                            .copied()
                        && self.event_matches_help_char(&last_raw)
                        && let Some(result) =
                            self.dispatch_prefix_help_command(&mut delayed_selection_event)?
                    {
                        self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                        return Ok(result);
                    }
                    if self.translate_upper_case_key_bindings_enabled()
                        && let Some(applied_shift_translation) =
                            self.apply_shift_translation_to_current_key_sequence()
                    {
                        shift_translation = Some(applied_shift_translation);
                        replay_current_sequence = true;
                        tracing::debug!(
                            "read_key_sequence: replaying after shift/downcase translation"
                        );
                        continue;
                    }
                    self.finalize_shift_translated_key_sequence(
                        sequence_is_undefined,
                        options,
                        shift_translation.take(),
                    );
                    self.restore_delayed_selection_event(&mut delayed_selection_event);
                    self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
                    let (translated, raw) = self.command_loop.keyboard.key_sequence_snapshot();
                    self.command_loop
                        .set_command_key_sequences(translated.clone(), raw);
                    return Ok((translated, binding));
                }
            }

            let is_prefix =
                resolve_prefix_keymap_binding_in_obarray(&self.obarray, &resolved.lookup).is_some();

            if is_prefix {
                // GNU doesn't publish a prefix merely because lookup found a
                // prefix map.  The following unbounded `read_char` owns the
                // delay: it waits `echo-keystrokes`, verifies that no input or
                // pre-existing echo-area message intervened, and only then
                // calls `echo_now` (keyboard.c:2850-2892).  Keeping that
                // transition solely in `delayed_key_echo_deadline` prevents
                // fast sequences and a pending startup message from flashing
                // an eager `C-x- (C-h for help)` prompt.
                continue;
            }

            self.finalize_shift_translated_key_sequence(
                sequence_is_undefined,
                options,
                shift_translation.take(),
            );
            self.restore_delayed_selection_event(&mut delayed_selection_event);
            self.restore_key_sequence_current_buffer(&mut saved_current_buffer);
            let (translated, raw) = self.command_loop.keyboard.key_sequence_snapshot();
            self.command_loop
                .set_command_key_sequences(translated.clone(), raw);
            return Ok((translated, binding));
        }
    }

    /// Read a single input event, blocking if necessary.
    ///
    /// Mirrors GNU Emacs `read_char()` (keyboard.c:2489).
    /// This is THE blocking point in the command loop.
    /// Before blocking, triggers redisplay.
    fn drain_ready_input_event_for_read_char(&mut self) -> Option<InputEvent> {
        loop {
            self.service_leading_internal_frontend_events();
            if let Some(event) = self
                .command_loop
                .keyboard
                .pending_input_events
                .pop_visible_front()
            {
                self.timer_stop_idle();
                return Some(event);
            }

            let rx = self.input_rx.as_ref()?;
            match rx.try_recv() {
                Ok(event) => self
                    .command_loop
                    .keyboard
                    .pending_input_events
                    .push_back(event),
                Err(crossbeam_channel::TryRecvError::Empty) => return None,
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.handle_display_terminal_disconnect();
                    return None;
                }
            }
        }
    }

    fn pop_lisp_event_queue_unrecorded(&mut self, symbol: &str) -> Option<Value> {
        let current = self.eval_symbol(symbol).unwrap_or(Value::NIL);
        if !current.is_cons() {
            return None;
        }

        let mut head = current.cons_car();
        self.assign(symbol, current.cons_cdr());

        // GNU read_char undoes the x-popup-menu one-element event wrapping
        // when reading the input-method queues.
        if head.is_cons() && head.cons_cdr().is_nil() {
            let car = head.cons_car();
            if car.as_fixnum().is_some() || car.is_symbol() {
                head = car;
            }
        }
        Some(head)
    }

    fn pop_unread_post_input_method_event_unrecorded(&mut self) -> Option<Value> {
        self.pop_lisp_event_queue_unrecorded("unread-post-input-method-events")
    }

    fn pop_unread_input_method_event_unrecorded(&mut self) -> Option<Value> {
        self.pop_lisp_event_queue_unrecorded("unread-input-method-events")
    }

    fn pop_queued_read_char_event(
        &mut self,
    ) -> Result<QueuedReadCharEvent, crate::emacs_core::error::Flow> {
        if let Some(event) = self.pop_unread_post_input_method_event_unrecorded() {
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::post_input_method(event),
            ));
        }
        if let Some(event) = self.pop_unread_command_event_unrecorded() {
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::reread_input_method_candidate(event),
            ));
        }
        if let Some(event) = self.pop_unread_input_method_event_unrecorded() {
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::reread_input_method_candidate(event),
            ));
        }
        if let Some(event) = self.command_loop.keyboard.take_unread_selection_event() {
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::fresh_input_method_candidate(event),
            ));
        }

        if let Some(event) = self.command_loop.keyboard.pop_unread_event() {
            if self.handle_help_echo_event(event)? {
                return Ok(QueuedReadCharEvent::HandledInternally);
            }
            if self.execute_special_event_if_bound(event)? {
                return Ok(QueuedReadCharEvent::HandledInternally);
            }
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::fresh_input_method_candidate(event),
            ));
        }

        if let Some(event) = self.command_loop.keyboard.next_executing_kbd_macro_event() {
            self.assign(
                "executing-kbd-macro-index",
                Value::fixnum(self.command_loop.keyboard.kboard.kbd_macro_index as i64),
            );
            return Ok(QueuedReadCharEvent::Event(
                ReadCharEvent::fresh_input_method_candidate(event),
            ));
        }

        Ok(QueuedReadCharEvent::None)
    }

    fn handle_display_terminal_disconnect(&mut self) {
        self.input_rx = None;
        let _ = crate::emacs_core::terminal::pure::delete_terminal_noelisp_owned(
            self,
            crate::emacs_core::terminal::pure::TERMINAL_ID,
        );
        self.request_shutdown(0, false);
    }

    /// Translate a freshly read character event through
    /// `keyboard-translate-table`.
    ///
    /// Mirrors GNU `read_char' (src/keyboard.c:3142-3176), the "Handle things
    /// that only apply to characters" block: a fixnum event is looked up in
    /// the table when the index is in range (any character for a char-table,
    /// below the length for a string or vector), and a non-nil entry replaces
    /// the event. GNU applies this only to events that just came out of
    /// `kbd_buffer_get_event', not to rereads from `unread-command-events' or
    /// a keyboard macro, which jump past this block to `reread_for_input_method'
    /// (src/keyboard.c:3252) -- so this runs at the host-input conversion sites
    /// only.
    ///
    /// This is what makes `normal-erase-is-backspace-mode' work on a ^H-erase
    /// terminal: it `key-translate's C-h to DEL (lisp/simple.el:11178), so the
    /// 0x08 the Backspace key sends must arrive at the key-sequence layer as
    /// 127 (DIVERGENCES.md entry 67).
    fn translate_fresh_character_event(&mut self, event: Value) -> Value {
        if event.as_fixnum().is_none() {
            return event;
        }
        let table = self
            .eval_symbol("keyboard-translate-table")
            .unwrap_or(Value::NIL);
        let indexable = match table.kind() {
            // A char-table is indexed by any character; GNU's guard is
            // CHARACTERP, so modifier-bearing events (meta bit set) fall
            // through untranslated.
            ValueKind::Veclike(VecLikeType::CharTable) => event.is_char(),
            // GNU also accepts a string or vector table, bounded by its
            // length -- `aref' reports that bound by signalling, and an
            // out-of-range event stays untranslated either way.
            ValueKind::Veclike(VecLikeType::Vector) | ValueKind::String => true,
            _ => false,
        };
        if !indexable {
            return event;
        }
        match crate::emacs_core::builtins::collections::builtin_aref_2(self, table, event) {
            // nil in keyboard-translate-table means no translation.
            Ok(translated) if !translated.is_nil() => translated,
            _ => event,
        }
    }

    fn handle_read_char_input_event(
        &mut self,
        event: InputEvent,
        tty_input_decoding: TtyInputDecoding,
    ) -> Result<Option<Value>, crate::emacs_core::error::Flow> {
        if crate::frontend_events::interrupts(&event)
            && self.interrupt_for_input_event_if_requested(event.clone())?
        {
            return Ok(None);
        }

        match event {
            InputEvent::RawTtyBytes { bytes, target } => {
                self.route_tty_keyboard_input(target);
                for byte in bytes.into_iter().rev() {
                    self.command_loop
                        .keyboard
                        .pending_input_events
                        .push_front(InputEvent::TtyByte { byte, target });
                }
                Ok(None)
            }
            InputEvent::TtyByte { byte, target } => {
                self.route_tty_keyboard_input(target);
                match tty_input_decoding {
                    TtyInputDecoding::RawBytes => {
                        self.clear_current_message_for_keyboard_input();
                        let raw_event = Value::fixnum(i64::from(byte));
                        if self.event_is_quit_char(&raw_event) {
                            self.request_quit_from_keyboard_input();
                        }
                        let emacs_event = self.translate_fresh_character_event(raw_event);
                        self.command_loop.store_kbd_macro_event(emacs_event);
                        Ok(Some(emacs_event))
                    }
                    TtyInputDecoding::KeyboardCodingSystem => {
                        let coding_system = crate::emacs_core::intern::resolve_sym(
                            self.coding_systems.keyboard_coding_sym(),
                        )
                        .to_owned();
                        let eol_conversion = self.eol_conversion();
                        let characters = self.command_loop.keyboard.kboard.tty_input_decoder.push(
                            &[byte],
                            &coding_system,
                            eol_conversion,
                        );
                        for character in characters.into_iter().rev() {
                            self.command_loop
                                .keyboard
                                .pending_input_events
                                .push_front(InputEvent::TtyCharacter { character, target });
                        }
                        Ok(None)
                    }
                }
            }
            InputEvent::TtyCharacter { character, target } => {
                self.route_tty_keyboard_input(target);
                self.clear_current_message_for_keyboard_input();
                let raw_event = Value::fixnum(i64::from(character.code()));
                // GNU compares the quit character against the byte the
                // terminal delivered (kbd_buffer_store_event, before read_char
                // translates), so quit detection stays on the raw event.
                if self.event_is_quit_char(&raw_event) {
                    self.request_quit_from_keyboard_input();
                }
                let emacs_event = self.translate_fresh_character_event(raw_event);
                self.command_loop.store_kbd_macro_event(emacs_event);
                Ok(Some(emacs_event))
            }
            InputEvent::WindowClose { emacs_frame_id } => {
                self.handle_window_close_input_event(emacs_frame_id)?;
                Ok(None)
            }
            InputEvent::Resize {
                width,
                height,
                scale_factor,
                emacs_frame_id,
            } => {
                self.apply_resize_input_event(width, height, scale_factor, emacs_frame_id, true);
                self.redisplay();
                self.timer_resume_idle();
                Ok(None)
            }
            InputEvent::DisplayReset => {
                // Mirrors the WindowResize shape: apply, redisplay, resume
                // idle. The reset handler busts the redisplay signature
                // itself since no buffer/geometry state changed.
                self.handle_display_reset_input_event();
                self.redisplay();
                self.timer_resume_idle();
                Ok(None)
            }
            InputEvent::WebView(event) => {
                if self.apply_xwidget_frontend_event(&event)? {
                    self.redisplay();
                }
                Ok(None)
            }
            InputEvent::SurfaceCreateFailed { id, error } => {
                self.handle_surface_create_failed_input_event(id, &error)?;
                Ok(None)
            }
            InputEvent::FrameShaderFailed { error } => {
                let effects = crate::frontend_events::report_frame_shader_failure(self, &error)?;
                if effects.redisplay_needed {
                    self.redisplay();
                }
                Ok(None)
            }
            InputEvent::TerminalCreateFailed { id, error } => {
                self.handle_terminal_create_failed_input_event(id, &error)?;
                Ok(None)
            }
            InputEvent::TerminalExited { id } => {
                self.handle_terminal_exited_input_event(id)?;
                Ok(None)
            }
            InputEvent::TerminalTitleChanged { id, title } => {
                self.handle_terminal_title_changed_input_event(id, &title)?;
                Ok(None)
            }
            InputEvent::Focus {
                focused,
                emacs_frame_id,
            } => {
                self.timer_resume_idle();
                if let Some(event) = self.make_lispy_focus_event(focused, emacs_frame_id) {
                    if self.execute_special_event_if_bound(event)? {
                        return Ok(None);
                    }
                    if focused {
                        // GNU `frame.el` routes `handle-focus-in` through the
                        // C primitive `internal-handle-focus-in`. In source
                        // bootstrap contexts that Lisp wrapper is not loaded
                        // yet, but focus-in still should not surface as a
                        // user event and must preserve the switch-frame side
                        // effect for other frames.
                        crate::emacs_core::builtins::symbols::builtin_internal_handle_focus_in(
                            self,
                            vec![event],
                        )?;
                    }
                    return Ok(None);
                }
                Ok(None)
            }
            InputEvent::MonitorsChanged { monitors } => {
                self.timer_resume_idle();
                crate::emacs_core::builtins::set_neomacs_monitor_info(monitors);
                let hook_sym = crate::emacs_core::hook_runtime::hook_symbol_by_name(
                    self,
                    "display-monitors-changed-functions",
                );
                let terminal = crate::emacs_core::terminal::pure::terminal_handle_value();
                let _ =
                    crate::emacs_core::hook_runtime::run_named_hook(self, hook_sym, &[terminal])?;
                Ok(None)
            }
            InputEvent::SelectWindow { window_id } => {
                self.timer_resume_idle();
                Ok(self.make_lispy_select_window_event(window_id))
            }
            InputEvent::KeyPress {
                ref key,
                emacs_frame_id,
            } => {
                self.route_keyboard_input_to_frame(emacs_frame_id);
                tracing::debug!("read_char: received KeyPress {:?}", key);
                self.clear_current_message_for_keyboard_input();
                let raw_event = key.to_emacs_event_value();
                if self.event_is_quit_char(&raw_event) {
                    self.request_quit_from_keyboard_input();
                }
                let emacs_event = self.translate_fresh_character_event(raw_event);
                self.command_loop.store_kbd_macro_event(emacs_event);
                Ok(Some(emacs_event))
            }
            InputEvent::MousePress {
                button,
                x,
                y,
                modifiers,
                target_frame_id,
            } => {
                self.clear_current_message_for_keyboard_input();
                // Keyboard audit Finding 12: compute the click
                // count for this press based on the previous
                // click state and update `last_mouse_click` so
                // the matching release can read the same count.
                // Mirrors GNU `keyboard.c:6041-6130`.
                let click_count = self.classify_mouse_click_on_press(button, x, y, target_frame_id);
                let prefix = Self::mouse_event_prefix_for_click_count("down-mouse", click_count);
                let event = Self::make_mouse_event(
                    &button,
                    x,
                    y,
                    target_frame_id,
                    &modifiers,
                    &prefix,
                    self,
                );
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::MouseRelease {
                button,
                x,
                y,
                target_frame_id,
            } => {
                self.clear_current_message_for_keyboard_input();
                // Use the click count recorded on the matching
                // press so the release event carries the same
                // double/triple modifier. Keyboard audit F12.
                let click_count = self
                    .command_loop
                    .keyboard
                    .kboard
                    .last_mouse_click
                    .filter(|state| state.button == button)
                    .map(|state| state.click_count)
                    .unwrap_or(1);
                let prefix = Self::mouse_event_prefix_for_click_count("mouse", click_count);
                let event = Self::make_mouse_event(
                    &button,
                    x,
                    y,
                    target_frame_id,
                    &Modifiers::none(),
                    &prefix,
                    self,
                );
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::MouseScroll {
                delta_x: _,
                delta_y,
                x,
                y,
                modifiers,
                target_frame_id,
            } => {
                let dir = if delta_y > 0.0 {
                    "wheel-up"
                } else {
                    "wheel-down"
                };
                let mut sym = String::new();
                Self::append_modifier_prefix(&modifiers, &mut sym);
                sym.push_str(dir);
                let position = Self::make_mouse_position(x, y, target_frame_id, self);
                let event = Value::list(vec![Value::symbol(&sym), position]);
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::PixelScroll {
                delta_x: _,
                delta_y,
                x: _,
                y: _,
                modifiers: _,
                target_frame_id,
            } => {
                // Smooth scroll (Phase 1, T4): accumulate the trackpad pixel delta
                // for this frame; the layout pass drains it and calls
                // Engine::pixel_scroll_window before re-laying. Consume the event
                // (no command); the command loop redisplays when input drains.
                self.accumulate_pending_pixel_scroll(
                    crate::window::FrameId(target_frame_id),
                    delta_y,
                );
                Ok(None)
            }
            InputEvent::LayoutInvalidated | InputEvent::ImageStateChanged { .. } => {
                unreachable!("internal frontend events are serviced before read_char")
            }
            InputEvent::MenuSelection { index } => {
                // A native-menu selection is fresh user input, like the event
                // returned by GNU Emacs's read_menu_command/read_char path.
                // Clear only the logical echo-area message here; redisplay may
                // remain inhibited by the modal native-menu session.
                self.clear_current_message_for_keyboard_input();
                let event = Value::list(vec![
                    Value::symbol("menu-selection"),
                    Value::fixnum(index as i64),
                ]);
                Ok(Some(event))
            }
            InputEvent::ToolBarClick {
                index,
                emacs_frame_id,
            } => {
                let Some(key) = self.tool_bar_key_at_index(index, emacs_frame_id) else {
                    return Ok(None);
                };
                let frame = self.event_frame_value(emacs_frame_id);
                let event = Value::list(vec![
                    key,
                    Value::list(vec![frame, Value::symbol("tool-bar")]),
                ]);
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::PresentedRegion {
                presentation,
                hit,
                x,
                y,
                target_frame_id,
            } => {
                let Some(frame_id) = self.event_frame_id(target_frame_id) else {
                    return Ok(None);
                };
                let Some(frame) = self.frames.get(frame_id) else {
                    return Ok(None);
                };
                if frame.active_presentation().map(|id| id.get()) != Some(presentation) {
                    return Ok(None);
                }
                self.command_loop
                    .keyboard
                    .kboard
                    .presented_mouse_observation = Some(PresentedMouseObservation {
                    presentation,
                    hit,
                    x,
                    y,
                    frame_id: frame_id.0,
                });
                Ok(None)
            }
            InputEvent::PresentedPointer {
                presentation,
                interaction,
                pressed,
                button,
                x,
                y,
                emacs_frame_id,
            } => {
                let Some(target) = self.resolve_presented_mouse_target(presentation, interaction)
                else {
                    return Ok(None);
                };
                let Some(frame_id) = self.event_frame_id(emacs_frame_id) else {
                    return Ok(None);
                };
                if self.frames.get(frame_id).is_none() || button == 0 {
                    return Ok(None);
                }
                let area = match target.area {
                    PresentedMouseArea::TabBar => "tab-bar",
                };
                let position = Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                    window_or_frame: Value::make_frame(frame_id.0),
                    area: Some(area),
                    x: x.round() as i64,
                    y: y.round() as i64,
                    metrics: MousePosnMetrics {
                        point: None,
                        col: None,
                        row: None,
                        width: None,
                        height: None,
                        anchor_x: None,
                        anchor_y: None,
                    },
                });
                let Some(mut position_parts) = crate::emacs_core::value::list_to_vec(&position)
                else {
                    return Ok(None);
                };
                position_parts[4] = target.posn_string;
                let symbol = if pressed {
                    format!("down-mouse-{button}")
                } else {
                    format!("mouse-{button}")
                };
                let event = Value::list(vec![Value::symbol(&symbol), Value::list(position_parts)]);
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::PresentationActivated { .. }
            | InputEvent::PresentationDiscarded { .. }
            | InputEvent::PresentationRetired { .. } => {
                unreachable!("internal frontend events are serviced before read_char")
            }
            InputEvent::MenuBarClick {
                index,
                key,
                menu_x,
                menu_y,
                anchor_x,
                anchor_y,
                anchor_width,
                anchor_height,
                emacs_frame_id,
            } => {
                if index < 0 {
                    return Ok(None);
                }
                let Some(frame_id) = self.event_frame_id(emacs_frame_id) else {
                    return Ok(None);
                };
                if self.frames.get(frame_id).is_none() {
                    return Ok(None);
                }
                self.pending_menu_bar_popup_anchor = Some(crate::emacs_core::MenuBarPopupAnchor {
                    frame_id,
                    menu_key: Some(key.clone()),
                    menu_x: menu_x.round() as i64,
                    x: anchor_x.round() as i64,
                    y: anchor_y.round() as i64,
                    width: anchor_width.round() as i64,
                    height: anchor_height.round() as i64,
                });
                let Some(event) = self.chrome_mouse_click_event(
                    "menu-bar",
                    index,
                    menu_x,
                    menu_y,
                    Some((anchor_x, anchor_y)),
                    anchor_width,
                    anchor_height,
                    emacs_frame_id,
                ) else {
                    return Ok(None);
                };
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
            InputEvent::MouseMove {
                x,
                y,
                modifiers,
                target_frame_id,
            } => {
                self.note_mouse_move_input_event(x, y, target_frame_id);
                self.timer_resume_idle();
                if !self.track_mouse_enabled() {
                    return Ok(None);
                }
                self.clear_current_message_for_keyboard_input();
                let mut sym = String::new();
                Self::append_modifier_prefix(&modifiers, &mut sym);
                sym.push_str("mouse-movement");
                let position = Self::make_mouse_position(x, y, target_frame_id, self);
                let event = Value::list(vec![Value::symbol(&sym), position]);
                self.command_loop.store_kbd_macro_event(event);
                Ok(Some(event))
            }
        }
    }

    fn read_char_event_with_timeout(
        &mut self,
        timeout: Option<std::time::Duration>,
        tty_input_decoding: TtyInputDecoding,
    ) -> Result<Option<ReadCharEvent>, crate::emacs_core::error::Flow> {
        self.read_char_event_with_timeout_policy(timeout, false, tty_input_decoding)
    }

    fn read_char_event_with_timeout_for_key_sequence(
        &mut self,
    ) -> Result<Option<ReadCharEvent>, crate::emacs_core::error::Flow> {
        self.read_char_event_with_timeout_policy(None, true, TtyInputDecoding::KeyboardCodingSystem)
    }

    fn read_char_event_with_timeout_policy(
        &mut self,
        timeout: Option<std::time::Duration>,
        command_input: bool,
        tty_input_decoding: TtyInputDecoding,
    ) -> Result<Option<ReadCharEvent>, crate::emacs_core::error::Flow> {
        let deadline = timeout.map(|timeout| std::time::Instant::now() + timeout);
        let mut idle_auto_save_deadline = None;
        let mut key_echo_deadline = self.delayed_key_echo_deadline(timeout);

        loop {
            self.sync_keyboard_terminal_owner();
            // Service cross-thread tasks (e.g. diagnostics profile capture) at
            // this Lisp-safe point — between forms, never mid-primitive. Runs on
            // every idle wake, so a request is handled within one iteration.
            self.drain_eval_tasks();
            crate::emacs_core::builtins::drain_file_notify_events(self)?;
            match self.pop_queued_read_char_event()? {
                QueuedReadCharEvent::Event(event) => return Ok(Some(event)),
                QueuedReadCharEvent::HandledInternally => continue,
                QueuedReadCharEvent::None => {}
            }

            if self.sync_pending_resize_events() {
                self.redisplay();
            }
            if let Some(event) = self.drain_ready_input_event_for_read_char() {
                if let Some(value) = self.handle_read_char_input_event(event, tty_input_decoding)? {
                    return Ok(Some(ReadCharEvent::fresh_input_method_candidate(value)));
                }
                continue;
            }
            if self.shutdown_request.is_some() {
                return Err(crate::emacs_core::error::signal(
                    LispCondition::Quit,
                    vec![],
                ));
            }

            if deadline.is_some_and(|deadline| std::time::Instant::now() >= deadline) {
                self.timer_stop_idle();
                return Ok(None);
            }

            self.redisplay_for_input_wait();
            self.service_input_wait_with_redisplay()?;

            // GNU read_char re-checks Vunread_command_events after idle
            // timers/sit-for/read-event can requeue input, before consulting
            // the terminal queue.  Keep that priority so `M-x` replay from
            // sit-for cannot be overtaken by the next host byte.
            match self.pop_queued_read_char_event()? {
                QueuedReadCharEvent::Event(event) => return Ok(Some(event)),
                QueuedReadCharEvent::HandledInternally => continue,
                QueuedReadCharEvent::None => {}
            }

            tracing::debug!(
                "read_char: blocking on input (input_rx={})...",
                self.input_rx.is_some()
            );

            if self.sync_pending_resize_events() {
                self.redisplay();
            }

            if let Some(event) = self.drain_ready_input_event_for_read_char() {
                if let Some(value) = self.handle_read_char_input_event(event, tty_input_decoding)? {
                    return Ok(Some(ReadCharEvent::fresh_input_method_candidate(value)));
                }
                continue;
            }
            if self.shutdown_request.is_some() {
                return Err(crate::emacs_core::error::signal(
                    LispCondition::Quit,
                    vec![],
                ));
            }

            if deadline.is_none()
                && self.input_rx.is_none()
                && !crate::emacs_core::builtins::has_active_file_notify_watches()
                && self.processes.live_process_ids().is_empty()
            {
                // With no host input, file-notify watch, or live process, this
                // evaluator has no external source to keep an unbounded read
                // alive. A pending ordinary timer can still become due and
                // throw - GNU blocks in select with the timer deadline as its
                // timeout, which is how
                //   (with-timeout (0.01) (read-key-sequence "p"))
                // returns nil in batch: the timer fires mid-wait and its
                // handler throws out of the read. Wait out the earliest
                // pending timer and service it (a thrown Flow propagates);
                // report EOF only when no timer can ever fire.
                if let Some(timer_timeout) = self.next_ordinary_gnu_timer_timeout_before(None) {
                    if !timer_timeout.is_zero() {
                        std::thread::sleep(timer_timeout.min(std::time::Duration::from_millis(50)));
                    }
                    self.service_timers_without_redisplay()?;
                    continue;
                }
                self.timer_stop_idle();
                return Ok(None);
            }

            // GNU starts the idle epoch only for an unbounded read. A timed
            // Lisp `read-char` must not recursively activate idle timers, but
            // the command loop's unbounded read does (`keyboard.c:2869-2875`).
            if timeout.is_none() {
                self.timer_start_idle();
            }

            // `auto-save-timeout` is not a Lisp idle timer. GNU computes it
            // inside `read_char`, then folds the resulting `sit_for` deadline
            // into the same input/process/timer wait. Keep one fixed deadline
            // across ordinary timer and process wakeups so background activity
            // cannot postpone auto-save forever.
            if command_input && idle_auto_save_deadline.is_none() {
                idle_auto_save_deadline = self
                    .command_idle_auto_save_delay()
                    .and_then(|delay| std::time::Instant::now().checked_add(delay));
            }
            let wait_deadline = [deadline, idle_auto_save_deadline, key_echo_deadline]
                .into_iter()
                .flatten()
                .min();
            let wait_result = self.wait_for_command_input(wait_deadline);

            match wait_result? {
                CommandInputWaitOutcome::InputPending => {
                    self.timer_stop_idle();
                    continue;
                }
                CommandInputWaitOutcome::DeadlineElapsed => {
                    let now = std::time::Instant::now();
                    if deadline.is_some_and(|deadline| now >= deadline) {
                        self.timer_stop_idle();
                        return Ok(None);
                    }
                    if key_echo_deadline.is_some_and(|deadline| now >= deadline) {
                        key_echo_deadline = None;
                        if self.current_message_value().is_none()
                            && matches!(
                                self.command_loop.keyboard.kboard.key_echo_state,
                                KeyEchoState::Inactive
                            )
                        {
                            let events = self.command_loop.read_command_keys().to_vec();
                            if !events.is_empty() {
                                self.publish_key_echo_message(&events, None);
                                self.redisplay();
                            }
                        }
                    }
                    if idle_auto_save_deadline.is_some_and(|deadline| now >= deadline) {
                        idle_auto_save_deadline = None;
                        self.run_command_loop_auto_save("idle timeout");
                        self.redisplay();
                    }
                    continue;
                }
                CommandInputWaitOutcome::Interrupted => {
                    // Resize/focus/display maintenance is not user input. Its
                    // handlers preserve the idle epoch; do not start a fresh
                    // epoch (and re-arm repeating idle timers) merely because
                    // the unified wait was interrupted.
                    self.timer_resume_idle();
                    continue;
                }
            }
        }
    }

    /// GNU's buffer-size-scaled delay for `auto-save-timeout`.
    ///
    /// This is command-input policy, not general timer policy: ordinary Lisp
    /// `read-char` calls must never cause automatic saves. Returning `None`
    /// means there has been no input since the last auto-save or the user has
    /// disabled idle auto-saving.
    fn command_idle_auto_save_delay(&mut self) -> Option<std::time::Duration> {
        if self.num_nonmacro_input_events() <= self.command_loop.last_auto_save_input_events {
            return None;
        }

        let timeout = self
            .eval_symbol("auto-save-timeout")
            .ok()
            .and_then(|value| value.as_fixnum())
            .filter(|timeout| *timeout > 0)? as u64;

        let selected = self.frames.selected_frame().and_then(|frame| {
            frame.selected_window().map(|window| {
                (
                    frame.minibuffer_window == Some(window.id()),
                    window.buffer_id(),
                )
            })
        });
        if let Some((false, Some(buffer_id))) = selected
            && let Some(buffer) = self.buffers.get(buffer_id)
        {
            self.command_loop.last_non_minibuffer_size = buffer.accessible_char_len().get();
        }

        let mut scaled_size = (self.command_loop.last_non_minibuffer_size >> 8) + 1;
        let mut delay_level = 0_u64;
        while scaled_size > 64 {
            delay_level += 1;
            scaled_size -= scaled_size >> 2;
        }
        delay_level = delay_level.max(4);

        Some(std::time::Duration::from_secs(
            timeout.saturating_mul(delay_level) / 4,
        ))
    }

    pub(crate) fn read_char_with_timeout(
        &mut self,
        timeout: Option<std::time::Duration>,
    ) -> Result<Option<Value>, crate::emacs_core::error::Flow> {
        self.read_char_with_timeout_decoding(timeout, TtyInputDecoding::KeyboardCodingSystem)
    }

    pub(crate) fn read_char_with_timeout_decoding(
        &mut self,
        timeout: Option<std::time::Duration>,
        tty_input_decoding: TtyInputDecoding,
    ) -> Result<Option<Value>, crate::emacs_core::error::Flow> {
        let Some(read_event) = self.read_char_event_with_timeout(timeout, tty_input_decoding)?
        else {
            return Ok(None);
        };
        if timeout.is_none() {
            match read_event.command_key_recording {
                CommandKeyRecording::Append => self
                    .command_loop
                    .keyboard
                    .kboard
                    .append_read_command_key(read_event.event),
                CommandKeyRecording::AppendIfEmpty
                    if self.command_loop.read_command_keys().is_empty() =>
                {
                    self.command_loop
                        .set_read_command_keys(vec![read_event.event]);
                }
                CommandKeyRecording::AppendIfEmpty => {}
            }
            self.refresh_immediate_key_echo();
        }
        Ok(Some(read_event.event))
    }

    pub(crate) fn read_char(&mut self) -> Result<Value, crate::emacs_core::error::Flow> {
        Ok(self.read_char_with_timeout(None)?.unwrap_or(Value::NIL))
    }

    fn resolve_input_frame_id(&self, emacs_frame_id: u64) -> Option<crate::window::FrameId> {
        if emacs_frame_id == 0 {
            self.frames.selected_frame().map(|frame| frame.id)
        } else {
            let frame_id = crate::window::FrameId(emacs_frame_id);
            self.frames.get(frame_id).map(|frame| frame.id)
        }
    }

    fn record_mouse_pixel_position(
        &mut self,
        frame_id: Option<crate::window::FrameId>,
        x: i64,
        y: i64,
    ) {
        self.command_loop
            .keyboard
            .set_mouse_pixel_position(frame_id, x, y);
    }

    fn make_help_echo_event(
        frame: Value,
        help: Value,
        window: Value,
        object: Value,
        pos: Value,
    ) -> Value {
        Value::list(vec![
            Value::symbol("help-echo"),
            frame,
            help,
            window,
            object,
            pos,
        ])
    }

    fn resolve_text_area_help_echo_event(
        &mut self,
        frame_id: crate::window::FrameId,
        x: i64,
        y: i64,
    ) -> Option<Value> {
        let frame = self.frames.get(frame_id)?;
        let observation = self
            .command_loop
            .keyboard
            .kboard
            .presented_mouse_observation
            .filter(|observation| {
                observation.frame_id == frame_id.0
                    && observation.x.round() as i64 == x
                    && observation.y.round() as i64 == y
                    && frame.active_presentation().map(|id| id.get())
                        == Some(observation.presentation)
            });
        let (window_id, buffer_position) = if let Some(observation) = observation {
            let hit = observation.hit?;
            if hit.region().kind() != neomacs_display_protocol::PresentedRegionKind::TextBody {
                return None;
            }
            let window = hit.region().window()?;
            let window_id = crate::window::WindowId(window.get() as u64);
            match frame.active_window_presentation(window_id)? {
                crate::window::WindowPresentationSnapshot::LiveWindow(_) => {}
                crate::window::WindowPresentationSnapshot::GeometryOnly(_) => return None,
            }
            let position = hit.text_position()?;
            (window_id, position.buffer_position())
        } else if frame.effective_window_system().is_some() && frame.active_presentation().is_some()
        {
            // GUI pointer observations are delivered from the renderer's exact
            // presentation. Missing/mismatched observations must not silently
            // fall back to mutable live-window arithmetic.
            return None;
        } else {
            // GNU's `note_mouse_highlight` asks `window_from_coordinates` for
            // the window and the part before it looks anything up
            // (src/xdisp.c), and a mode-line or header-line `help-echo` comes
            // from `mode_line_string`'s glyph rather than from a buffer
            // position. Only a coordinate the classifier placed outside the
            // chrome lines has a position to read a text property at.
            let hit = frame.coordinate_hit(x, y)?;
            let crate::window::WindowCoordinate::Buffer { at, .. } = hit.coordinate else {
                return None;
            };
            let snapshot = frame.redisplay_snapshot(hit.window)?;
            let point = snapshot.point_at_coords(at)?;
            (hit.window, point.buffer_pos.as_i64())
        };
        let window = frame.find_window(window_id)?;
        let buffer_id = window.buffer_id()?;

        let pair = crate::emacs_core::textprop::builtin_get_char_property_and_overlay_in_state(
            &self.obarray,
            &self.buffers,
            vec![
                Value::fixnum(buffer_position),
                Value::symbol("help-echo"),
                Value::make_buffer(buffer_id),
            ],
        )
        .ok()?;
        if !pair.is_cons() {
            return None;
        };
        let pair_car = pair.cons_car();
        let pair_cdr = pair.cons_cdr();
        if pair_car.is_nil() {
            return None;
        }
        let object = if pair_cdr.is_nil() {
            Value::make_buffer(buffer_id)
        } else {
            pair_cdr
        };
        Some(Self::make_help_echo_event(
            Value::make_frame(frame_id.0),
            pair_car,
            Value::make_window(window_id.0),
            object,
            Value::fixnum(buffer_position),
        ))
    }

    fn queue_mouse_help_echo_update(
        &mut self,
        frame_id: Option<crate::window::FrameId>,
        x: i64,
        y: i64,
    ) {
        let next = frame_id.and_then(|fid| self.resolve_text_area_help_echo_event(fid, x, y));
        let previous = self.command_loop.keyboard.kboard.last_help_echo_event;
        let changed = match (previous, next) {
            (Some(prev), Some(next)) => !crate::emacs_core::value::equal_value(&prev, &next, 0),
            (None, None) => false,
            _ => true,
        };
        if !changed {
            return;
        }

        match next {
            Some(event) => {
                self.command_loop.keyboard.unread_event(event);
                self.command_loop.keyboard.kboard.last_help_echo_event = Some(event);
            }
            None => {
                if let Some(fid) = frame_id {
                    self.command_loop
                        .keyboard
                        .unread_event(Self::make_help_echo_event(
                            Value::make_frame(fid.0),
                            Value::NIL,
                            Value::NIL,
                            Value::NIL,
                            Value::fixnum(0),
                        ));
                }
                self.command_loop.keyboard.kboard.last_help_echo_event = None;
            }
        }
    }

    pub(crate) fn note_mouse_move_for_frame(
        &mut self,
        frame_id: Option<crate::window::FrameId>,
        x: i64,
        y: i64,
    ) {
        self.record_mouse_pixel_position(frame_id, x, y);
        self.queue_mouse_help_echo_update(frame_id, x, y);
    }

    fn note_mouse_move_input_event(&mut self, x: f32, y: f32, target_frame_id: u64) {
        self.note_mouse_move_for_frame(
            self.resolve_input_frame_id(target_frame_id),
            x.round() as i64,
            y.round() as i64,
        );
    }

    fn handle_help_echo_event(
        &mut self,
        event: Value,
    ) -> Result<bool, crate::emacs_core::error::Flow> {
        let Some(parts) = crate::emacs_core::value::list_to_vec(&event) else {
            return Ok(false);
        };
        if parts.len() != 6 {
            return Ok(false);
        }
        let head = parts[0];
        if !head.is_symbol_named("help-echo") {
            return Ok(false);
        }

        self.show_help_echo(parts[2], parts[3], parts[4], parts[5])?;
        Ok(true)
    }

    /// Resolve and publish GNU `show_help_echo` state for mouse regions and
    /// native menus through one path.
    pub(crate) fn show_help_echo(
        &mut self,
        mut help: Value,
        window: Value,
        object: Value,
        pos: Value,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        if !help.is_nil() && !help.is_string() {
            help = if self.function_value_is_callable(&help) {
                self.funcall_general(help, vec![window, object, pos])?
            } else {
                self.eval_value(&help)?
            };
            if !help.is_nil() && !help.is_string() {
                return Ok(());
            }
        }

        help = self.fixup_help_echo_message(help)?;

        if help.is_string() {
            help = self.substitute_help_echo_command_keys(help)?;
        }

        let show_help_function = self
            .obarray
            .symbol_value("show-help-function")
            .copied()
            .unwrap_or(Value::NIL);
        if self.function_value_is_callable(&show_help_function) {
            let _ = self.funcall_general(show_help_function, vec![help])?;
        } else if let Some(message) = help.as_lisp_string() {
            self.set_current_message(Some(message.clone()));
            self.redisplay();
        } else {
            self.clear_current_message();
        }

        self.timer_resume_idle();
        Ok(())
    }

    fn fixup_help_echo_message(
        &mut self,
        help: Value,
    ) -> Result<Value, crate::emacs_core::error::Flow> {
        // GNU `show_help_echo` applies `mouse-fixup-help-message` whenever the
        // resolved help text is a string; it is not conditional on whether the
        // current runtime is actively polling host input.
        if help.as_utf8_str().is_none() {
            return Ok(help);
        }

        match self.obarray.symbol_function("mouse-fixup-help-message") {
            Some(function) if !crate::emacs_core::autoload::is_autoload_value(&function) => {
                self.funcall_general(Value::symbol("mouse-fixup-help-message"), vec![help])
            }
            _ => Ok(help),
        }
    }

    fn substitute_help_echo_command_keys(
        &mut self,
        help: Value,
    ) -> Result<Value, crate::emacs_core::error::Flow> {
        if help.is_nil() || help.as_utf8_str().is_none() {
            return Ok(help);
        }

        let inhibit = crate::emacs_core::textprop::builtin_get_text_property_in_state(
            &self.obarray,
            &self.buffers,
            vec![
                Value::fixnum(1),
                Value::symbol("help-echo-inhibit-substitution"),
                help,
            ],
        )?;
        if inhibit.is_truthy() {
            return Ok(help);
        }

        match self.obarray.symbol_function("substitute-command-keys") {
            Some(function) if !crate::emacs_core::autoload::is_autoload_value(&function) => {
                self.funcall_general(Value::symbol("substitute-command-keys"), vec![help])
            }
            _ => Ok(help),
        }
    }

    /// Build an Emacs mouse event value.
    ///
    /// Returns `(EVENT-SYMBOL POSITION)` where EVENT-SYMBOL is e.g.
    /// `mouse-1`, `down-mouse-2`, `C-mouse-1`, etc.
    /// Compute the click-count for a just-received
    /// MousePress event and update `KBoard.last_mouse_click`.
    /// Returns the 1-based click count (1 = single, 2 = double,
    /// 3 = triple). Subsequent clicks beyond triple saturate at
    /// 3, matching GNU `keyboard.c:6120-6128` where the count
    /// wraps via `min (3, ...)`. Keyboard audit Finding 12.
    fn classify_mouse_click_on_press(
        &mut self,
        button: MouseButton,
        x: f32,
        y: f32,
        frame_id: u64,
    ) -> u32 {
        let now = std::time::Instant::now();
        let double_click_time_ms = self
            .eval_symbol("double-click-time")
            .ok()
            .and_then(|v| v.as_fixnum())
            .filter(|&n| n > 0)
            .map(|n| n as u64)
            .unwrap_or(500);
        let double_click_fuzz = self
            .eval_symbol("double-click-fuzz")
            .ok()
            .and_then(|v| v.as_fixnum())
            .filter(|&n| n >= 0)
            .map(|n| n as f32)
            .unwrap_or(3.0);

        let count = match self.command_loop.keyboard.kboard.last_mouse_click {
            Some(prev)
                if prev.button == button
                    && prev.frame_id == frame_id
                    && (prev.x - x).abs() <= double_click_fuzz
                    && (prev.y - y).abs() <= double_click_fuzz
                    && now.saturating_duration_since(prev.timestamp).as_millis()
                        <= double_click_time_ms as u128 =>
            {
                (prev.click_count + 1).min(3)
            }
            _ => 1,
        };

        self.command_loop.keyboard.kboard.last_mouse_click = Some(LastMouseClick {
            button,
            x,
            y,
            frame_id,
            timestamp: now,
            click_count: count,
        });
        count
    }

    /// Build the event symbol prefix for a mouse click, taking
    /// the click count into account. `base` is either
    /// `down-mouse` (for presses) or `mouse` (for releases).
    /// For `count == 1`, returns `base` unchanged. For 2, prefix
    /// with `double-`; for 3, `triple-`. Keyboard audit Finding
    /// 12.
    fn mouse_event_prefix_for_click_count(base: &str, count: u32) -> String {
        match count {
            0 | 1 => base.to_string(),
            2 => format!("double-{}", base),
            _ => format!("triple-{}", base),
        }
    }

    pub(crate) fn make_mouse_event(
        button: &MouseButton,
        x: f32,
        y: f32,
        target_frame_id: u64,
        modifiers: &Modifiers,
        prefix: &str,
        eval: &Self,
    ) -> Value {
        let button_num = match button {
            MouseButton::Left => 1,
            MouseButton::Middle => 2,
            MouseButton::Right => 3,
            MouseButton::Button4 => 4,
            MouseButton::Button5 => 5,
        };
        let mut sym = String::new();
        Self::append_modifier_prefix(modifiers, &mut sym);
        sym.push_str(&format!("{}-{}", prefix, button_num));

        let position = Self::make_mouse_position(x, y, target_frame_id, eval);
        Value::list(vec![Value::symbol(&sym), position])
    }

    fn event_position(event: &Value) -> Option<Value> {
        let event_slots = crate::emacs_core::value::list_to_vec(event)?;
        let position = *event_slots.get(1)?;
        let position_slots = crate::emacs_core::value::list_to_vec(&position)?;
        if position_slots.len() >= 4 || Self::event_position_area(&position).is_some() {
            Some(position)
        } else {
            None
        }
    }

    fn key_sequence_lookup_position(events: &[Value]) -> Option<Value> {
        events.iter().find_map(Self::event_position)
    }

    fn key_sequence_target_buffer_id(
        events: &[Value],
        frames: &crate::window::FrameManager,
    ) -> Option<crate::buffer::BufferId> {
        let position = Self::key_sequence_lookup_position(events)?;
        let slots = crate::emacs_core::value::list_to_vec(&position)?;
        let first = *slots.first()?;
        let wid = first.as_window_id()?;
        let window_id = crate::window::WindowId(wid);

        for frame_id in frames.frame_list() {
            let Some(frame) = frames.get(frame_id) else {
                continue;
            };
            let Some(window) = frame.find_window(window_id) else {
                continue;
            };
            if let Some(buffer_id) = window.buffer_id() {
                return Some(buffer_id);
            }
        }

        None
    }

    fn event_position_area(position: &Value) -> Option<Value> {
        let slots = crate::emacs_core::value::list_to_vec(position)?;
        let area_or_pos = *slots.get(1)?;
        match area_or_pos.kind() {
            ValueKind::Symbol(_) => Some(area_or_pos),
            ValueKind::Cons => {
                let head = area_or_pos.cons_car();
                head.as_symbol_name().map(|_| head)
            }
            _ => None,
        }
    }

    fn maybe_prefix_mouse_area(events: &[Value]) -> Option<Vec<Value>> {
        let first = events.first()?;
        let position = Self::event_position(first)?;
        let area = Self::event_position_area(&position)?;
        let mut prefixed = Vec::with_capacity(events.len() + 1);
        prefixed.push(area);
        prefixed.extend_from_slice(events);
        Some(prefixed)
    }

    fn mouse_event_symbol_name(event: &Value) -> Option<String> {
        match event.kind() {
            ValueKind::Symbol(_) => event.as_symbol_name().map(str::to_owned),
            ValueKind::Cons => {
                let slots = crate::emacs_core::value::list_to_vec(event)?;
                slots.first()?.as_symbol_name().map(str::to_owned)
            }
            _ => None,
        }
    }

    fn rewrite_mouse_event_symbol(event: Value, symbol_name: &str) -> Option<Value> {
        match event.kind() {
            ValueKind::Symbol(_) => Some(Value::symbol(symbol_name)),
            ValueKind::Cons => {
                let mut slots = crate::emacs_core::value::list_to_vec(&event)?;
                let head = slots.first_mut()?;
                head.as_symbol_name()?;
                *head = Value::symbol(symbol_name);
                Some(Value::list(slots))
            }
            _ => None,
        }
    }

    fn simplify_mouse_event_once(event: Value) -> Option<(MouseEventFallbackStep, Option<Value>)> {
        let symbol_name = Self::mouse_event_symbol_name(&event)?;
        let (modifier_prefix, base) =
            crate::emacs_core::keyboard::pure::split_symbol_modifiers(&symbol_name);
        let mut rewritten_name = modifier_prefix;

        let rewritten_base = if let Some(rest) = base.strip_prefix("triple-") {
            rewritten_name.push_str("double-");
            rewritten_name.push_str(rest);
            return Some((
                MouseEventFallbackStep::Rewrite,
                Self::rewrite_mouse_event_symbol(event, &rewritten_name),
            ));
        } else if let Some(rest) = base.strip_prefix("double-") {
            rest
        } else if let Some(rest) = base.strip_prefix("drag-") {
            rest
        } else if base.starts_with("down-") || base.starts_with("up-") {
            return Some((MouseEventFallbackStep::Drop, None));
        } else {
            return None;
        };

        rewritten_name.push_str(rewritten_base);
        Some((
            MouseEventFallbackStep::Rewrite,
            Self::rewrite_mouse_event_symbol(event, &rewritten_name),
        ))
    }

    fn retained_key_sequence_len_after_mouse_drop(events: &[Value]) -> usize {
        let Some(last_event) = events.last() else {
            return 0;
        };

        let mut retained_len = events.len().saturating_sub(1);
        if retained_len > 0
            && let Some(position) = Self::event_position(last_event)
            && let Some(area) = Self::event_position_area(&position)
            && events[retained_len - 1] == area
        {
            retained_len -= 1;
        }
        retained_len
    }

    fn resolve_undefined_mouse_sequence_fallback(
        &mut self,
        events: &[Value],
        lookup_position: Option<&Value>,
    ) -> Result<Option<UndefinedMouseSequenceFallback>, crate::emacs_core::error::Flow> {
        use crate::emacs_core::keymap::{DefaultBindingMode, resolve_active_key_binding};

        let Some(last_index) = events.len().checked_sub(1) else {
            return Ok(None);
        };
        let mut rewritten_events = events.to_vec();

        loop {
            let Some((step, rewritten_event)) =
                Self::simplify_mouse_event_once(rewritten_events[last_index])
            else {
                return Ok(None);
            };

            match step {
                MouseEventFallbackStep::Rewrite => {
                    let Some(rewritten_event) = rewritten_event else {
                        return Ok(None);
                    };
                    rewritten_events[last_index] = rewritten_event;
                    let resolved = resolve_active_key_binding(
                        self,
                        &rewritten_events,
                        DefaultBindingMode::Accept,
                        false,
                        lookup_position,
                    )?;
                    let lookup_is_undefined =
                        resolved.lookup.is_nil() || resolved.lookup == Value::symbol("undefined");
                    let binding_is_undefined =
                        resolved.binding.is_nil() || resolved.binding == Value::symbol("undefined");
                    if !(lookup_is_undefined || binding_is_undefined) {
                        return Ok(Some(UndefinedMouseSequenceFallback::Rewrite {
                            events: rewritten_events,
                            resolved,
                        }));
                    }
                }
                MouseEventFallbackStep::Drop => {
                    let retained_len =
                        Self::retained_key_sequence_len_after_mouse_drop(&rewritten_events);
                    rewritten_events.truncate(retained_len);
                    return Ok(Some(UndefinedMouseSequenceFallback::Drop {
                        retained_events: rewritten_events,
                    }));
                }
            }
        }
    }

    fn window_point(window: &crate::window::Window) -> Option<i64> {
        match window {
            crate::window::Window::Leaf { point, .. } => Some(point.as_i64()),
            crate::window::Window::Internal { .. } => None,
        }
    }

    fn mouse_posn_descriptor_value(desc: MousePosnDescriptor) -> Value {
        let area_or_pos = match desc.area {
            Some(area) => Value::symbol(area),
            None => desc.metrics.point.map(Value::fixnum).unwrap_or(Value::NIL),
        };
        let pos = desc.metrics.point.map(Value::fixnum).unwrap_or(Value::NIL);
        let col_row = match (desc.metrics.col, desc.metrics.row) {
            (Some(col), Some(row)) => Value::cons(Value::fixnum(col), Value::fixnum(row)),
            _ => Value::NIL,
        };
        let width_height = match (desc.metrics.width, desc.metrics.height) {
            (Some(width), Some(height)) => Value::cons(Value::fixnum(width), Value::fixnum(height)),
            _ => Value::NIL,
        };

        Value::list(vec![
            desc.window_or_frame,
            area_or_pos,
            Value::cons(Value::fixnum(desc.x), Value::fixnum(desc.y)),
            Value::fixnum(0),
            Value::NIL,
            pos,
            col_row,
            Value::NIL,
            Value::cons(
                Value::fixnum(desc.metrics.anchor_x.unwrap_or(0)),
                Value::fixnum(desc.metrics.anchor_y.unwrap_or(0)),
            ),
            width_height,
        ])
    }

    fn event_frame_id(&self, emacs_frame_id: u64) -> Option<crate::window::FrameId> {
        if emacs_frame_id == 0 {
            self.frames.selected_frame().map(|frame| frame.id)
        } else {
            Some(crate::window::FrameId(emacs_frame_id))
        }
    }

    fn event_frame_value(&self, emacs_frame_id: u64) -> Value {
        self.event_frame_id(emacs_frame_id)
            .map(|frame_id| Value::make_frame(frame_id.0))
            .unwrap_or(Value::NIL)
    }

    #[allow(clippy::too_many_arguments)] // event geometry mirrors the Lisp mouse-event payload
    fn chrome_mouse_click_event(
        &self,
        area: &'static str,
        index: i32,
        x: f32,
        y: f32,
        anchor: Option<(f32, f32)>,
        width: f32,
        height: f32,
        emacs_frame_id: u64,
    ) -> Option<Value> {
        if index < 0 {
            return None;
        }
        let frame_id = self.event_frame_id(emacs_frame_id)?;
        self.frames.get(frame_id)?;
        let window_or_frame = if area == "menu-bar" {
            Value::NIL
        } else {
            Value::make_frame(frame_id.0)
        };
        let position = Self::mouse_posn_descriptor_value(MousePosnDescriptor {
            window_or_frame,
            area: Some(area),
            x: x.round() as i64,
            y: y.round() as i64,
            metrics: MousePosnMetrics {
                point: None,
                col: None,
                row: None,
                width: Some(width.round() as i64),
                height: Some(height.round() as i64),
                anchor_x: anchor.map(|(x, _)| x.round() as i64),
                anchor_y: anchor.map(|(_, y)| y.round() as i64),
            },
        });
        Some(Value::list(vec![Value::symbol("mouse-1"), position]))
    }

    fn tool_bar_key_at_index(&self, index: i32, emacs_frame_id: u64) -> Option<Value> {
        if index < 0 {
            return None;
        }
        let raw_map = self.tool_bar_map_for_frame(emacs_frame_id);
        let keymap = crate::emacs_core::keymap::maybe_keymap_in_obarray(self.obarray(), &raw_map)?;
        let mut remaining = index as usize;
        let mut found = None;
        crate::emacs_core::keymap::list_keymap_for_each_binding(
            &keymap,
            Some(self.obarray()),
            |key, def| {
                if found.is_some() {
                    return;
                }
                if !Self::is_rendered_tool_bar_item(&key, &def) {
                    return;
                }
                if remaining == 0 {
                    found = Some(key);
                } else {
                    remaining -= 1;
                }
            },
        );
        found
    }

    fn tool_bar_map_for_frame(&self, emacs_frame_id: u64) -> Value {
        if emacs_frame_id == 0 {
            return self.current_tool_bar_map();
        }
        if let Some(frame_id) = self.event_frame_id(emacs_frame_id)
            && let Some(buffer_id) = self
                .frames
                .get(frame_id)
                .and_then(|frame| frame.selected_window())
                .and_then(|window| window.buffer_id())
            && let Some(buffer) = self.buffers.get(buffer_id)
            && let Some(local) = buffer.buffer_local_value("tool-bar-map")
        {
            return local;
        }
        self.default_tool_bar_map()
    }

    fn default_tool_bar_map(&self) -> Value {
        self.obarray()
            .default_value_id(intern("tool-bar-map"))
            .copied()
            .unwrap_or(Value::NIL)
    }

    fn current_tool_bar_map(&self) -> Value {
        if let Some(buffer) = self.buffers.current_buffer()
            && let Some(local) = buffer.buffer_local_value("tool-bar-map")
        {
            return local;
        }
        self.default_tool_bar_map()
    }

    fn is_rendered_tool_bar_item(key: &Value, def: &Value) -> bool {
        let key_name = key.as_symbol_name().unwrap_or_default();
        if key_name.starts_with("separator") {
            return true;
        }
        let def = if def.is_cons() && def.cons_cdr().is_nil() {
            def.cons_car()
        } else {
            *def
        };
        if !def.is_cons() {
            return def.as_symbol_name() == Some("menu-bar-separator");
        }
        let car = def.cons_car();
        KeymapMarker::MenuItem.is_value(car) || car.is_string()
    }

    fn make_mouse_position(x: f32, y: f32, target_frame_id: u64, eval: &Self) -> Value {
        let frame_id = if target_frame_id == 0 {
            match eval.frames.selected_frame() {
                Some(frame) => frame.id.0,
                None => 0,
            }
        } else {
            target_frame_id
        };

        let Some(frame) = (if frame_id == 0 {
            eval.frames.selected_frame()
        } else {
            eval.frames.get(crate::window::FrameId(frame_id))
        }) else {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::NIL,
                area: None,
                x: x.round() as i64,
                y: y.round() as i64,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        };

        if let Some(position) = Self::make_presented_mouse_position(frame, frame_id, x, y, eval) {
            return position;
        }

        let frame_x = x.round() as i64;
        let frame_y = y.round() as i64;
        if frame.effective_window_system().is_some() && frame.active_presentation().is_some() {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::make_frame(frame.id.0),
                area: None,
                x: frame_x,
                y: frame_y,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        }
        let frame_height = frame.height as i64;
        let menu_bar_height = frame.menu_bar_height as i64;
        let tool_bar_height = frame.tool_bar_height as i64;
        let tab_bar_height = frame.tab_bar_height as i64;

        if menu_bar_height > 0 && frame_y < menu_bar_height {
            let menu_bar_x = (x / frame.char_width.max(1.0)).floor() as i64;
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::NIL,
                area: Some("menu-bar"),
                x: menu_bar_x,
                y: frame_y,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        }
        if tool_bar_height > 0 && frame_y < menu_bar_height + tool_bar_height {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::NIL,
                area: Some("tool-bar"),
                x: frame_x,
                y: frame_y - menu_bar_height,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        }
        if tab_bar_height > 0 && frame_y < menu_bar_height + tool_bar_height + tab_bar_height {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::NIL,
                area: Some("tab-bar"),
                x: frame_x,
                y: frame_y - menu_bar_height - tool_bar_height,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        }

        // GNU asks this question ONCE, in `window_from_coordinates`, and
        // `make_lispy_position` branches on the part it answers before any
        // buffer position is looked up (src/keyboard.c:5793 and 5862-5975).
        // `posn-at-x-y` reaches the same classifier, so the two cannot disagree
        // about the same coordinate.
        let Some(hit) = frame.coordinate_hit(frame_x, frame_y) else {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::make_frame(frame.id.0),
                area: None,
                x: frame_x,
                y: frame_y.min(frame_height.saturating_sub(1)),
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        };
        let window_id = hit.window;
        let Some(window) = frame.find_window(window_id) else {
            return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::make_frame(frame.id.0),
                area: None,
                x: frame_x,
                y: frame_y,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            });
        };

        let fallback_metrics = MousePosnMetrics {
            point: Self::window_point(window),
            col: None,
            row: None,
            width: None,
            height: None,
            anchor_x: None,
            anchor_y: None,
        };
        let column_width = frame.char_width.max(1.0).round() as i64;

        match hit.coordinate {
            crate::window::WindowCoordinate::ChromeLine {
                line,
                window_x,
                window_y,
            } => {
                // `mode_line_string` reads the window's current matrix and
                // never re-runs a walk (src/dispnew.c:6444-6519), and GNU sets
                // `textpos = -1` for this branch (src/keyboard.c:5900) -- so a
                // chrome posn has no buffer position at all.
                let unfilled = crate::window::WindowDisplaySnapshot::default();
                let retained = frame.redisplay_snapshot(window_id).unwrap_or(&unfilled);
                let chrome = retained.chrome_line_hit(
                    line,
                    window_x,
                    window_y,
                    hit.geometry.bottom_y - hit.geometry.top_y,
                    frame.char_height.max(1.0).round() as i64,
                    column_width,
                );
                Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                    window_or_frame: Value::make_window(window_id.0),
                    area: line.part().area_symbol(),
                    x: window_x,
                    y: window_y,
                    metrics: MousePosnMetrics {
                        point: None,
                        col: Some(chrome.col),
                        row: Some(chrome.row),
                        width: Some(chrome.width),
                        height: Some(chrome.height),
                        anchor_x: Some(chrome.dx),
                        anchor_y: Some(chrome.dy),
                    },
                })
            }
            crate::window::WindowCoordinate::Buffer {
                part,
                window_x,
                window_y,
                at,
            } => {
                // Which click the posn reports depends on the part: the text
                // area, the margins and the fringes report Y relative to the
                // top of the TEXT area, everything else relative to the
                // window's own corner (src/keyboard.c:5878-5975).
                let (report_x, report_y) = match part {
                    crate::window::WindowPart::Text => (at.text_area_x(), at.text_area_y()),
                    crate::window::WindowPart::LeftMargin
                    | crate::window::WindowPart::RightMargin
                    | crate::window::WindowPart::LeftFringe
                    | crate::window::WindowPart::RightFringe => (window_x, at.text_area_y()),
                    _ => (window_x, window_y),
                };
                if let Some(snapshot) = frame.redisplay_snapshot(window_id)
                    && let Some(point) = snapshot.point_at_coords(at)
                {
                    return Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                        window_or_frame: Value::make_window(window_id.0),
                        area: part.area_symbol(),
                        x: report_x,
                        y: report_y,
                        metrics: MousePosnMetrics {
                            point: Some(point.buffer_pos.as_i64()),
                            // GNU has ONE function here: `make_lispy_position`
                            // builds the posn for a real mouse event and for
                            // `posn-at-x-y` alike, and
                            // `buffer_posn_from_coords` counts the columns past
                            // the end of a line for both
                            // (src/dispnew.c:6428-6430). Share the rule so the
                            // two cannot drift apart.
                            col: Some(point.column_for_click(report_x, column_width)),
                            row: Some(point.row),
                            width: Some(point.width.max(1)),
                            height: Some(point.height.max(1)),
                            anchor_x: None,
                            anchor_y: None,
                        },
                    });
                }
                Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                    window_or_frame: Value::make_window(window_id.0),
                    area: part.area_symbol(),
                    x: report_x,
                    y: report_y,
                    metrics: fallback_metrics,
                })
            }
        }
    }

    fn make_presented_mouse_position(
        frame: &crate::window::Frame,
        frame_id: u64,
        x: f32,
        y: f32,
        eval: &Self,
    ) -> Option<Value> {
        let observation = eval
            .command_loop
            .keyboard
            .kboard
            .presented_mouse_observation?;
        if observation.frame_id != frame_id
            || observation.x.to_bits() != x.to_bits()
            || observation.y.to_bits() != y.to_bits()
            || frame.active_presentation().map(|id| id.get()) != Some(observation.presentation)
        {
            return None;
        }
        let hit = observation.hit?;
        let region = hit.region();
        let area = match region.kind() {
            neomacs_display_protocol::PresentedRegionKind::TextBody => None,
            neomacs_display_protocol::PresentedRegionKind::LeftMargin => Some("left-margin"),
            neomacs_display_protocol::PresentedRegionKind::RightMargin => Some("right-margin"),
            neomacs_display_protocol::PresentedRegionKind::LeftFringe => Some("left-fringe"),
            neomacs_display_protocol::PresentedRegionKind::RightFringe => Some("right-fringe"),
            neomacs_display_protocol::PresentedRegionKind::LeftScrollBar
            | neomacs_display_protocol::PresentedRegionKind::RightScrollBar => {
                Some("vertical-scroll-bar")
            }
            neomacs_display_protocol::PresentedRegionKind::HorizontalScrollBar => {
                Some("horizontal-scroll-bar")
            }
            neomacs_display_protocol::PresentedRegionKind::TabLine => Some("tab-line"),
            neomacs_display_protocol::PresentedRegionKind::HeaderLine => Some("header-line"),
            neomacs_display_protocol::PresentedRegionKind::ModeLine => Some("mode-line"),
            neomacs_display_protocol::PresentedRegionKind::RightDivider => Some("vertical-line"),
            neomacs_display_protocol::PresentedRegionKind::BottomDivider => {
                Some("horizontal-scroll-bar")
            }
            neomacs_display_protocol::PresentedRegionKind::MenuBar => Some("menu-bar"),
            neomacs_display_protocol::PresentedRegionKind::ToolBar => Some("tool-bar"),
            neomacs_display_protocol::PresentedRegionKind::CompactBar => Some("menu-bar"),
            neomacs_display_protocol::PresentedRegionKind::TabBar => Some("tab-bar"),
        };
        let Some(window_id) = region.window() else {
            return Some(Self::mouse_posn_descriptor_value(MousePosnDescriptor {
                window_or_frame: Value::make_frame(frame.id.0),
                area,
                x: x.round() as i64,
                y: y.round() as i64,
                metrics: MousePosnMetrics {
                    point: None,
                    col: None,
                    row: None,
                    width: None,
                    height: None,
                    anchor_x: None,
                    anchor_y: None,
                },
            }));
        };
        let window_id = crate::window::WindowId(window_id.get() as u64);
        let publication = frame.active_presentation_geometry()?;
        let presented = publication
            .resolve(crate::window::geometry::WindowGeometryQuery::new(
                crate::window::geometry::PresentationId::new(observation.presentation),
                window_id,
            ))
            .ok()?;
        let outer = presented.regions().outer();
        let coordinate_origin =
            if region.kind() == neomacs_display_protocol::PresentedRegionKind::TextBody {
                presented.regions().text_body().origin()
            } else {
                outer.origin()
            };
        let fallback_point = frame.find_window(window_id).and_then(Self::window_point);
        // A presented text position is only a position in the window's own
        // buffer for the `LiveWindow` variant. An inactive mini-window's
        // geometry describes an echo buffer, not `w->contents`; matching the
        // publication enum keeps that semantic permission attached to the
        // snapshot instead of reconstructing it with a separate boolean.
        let metrics = match (
            frame.active_window_presentation(window_id)?,
            hit.text_position(),
        ) {
            (crate::window::WindowPresentationSnapshot::LiveWindow(_), Some(point)) => {
                let bounds = point.bounds();
                MousePosnMetrics {
                    point: Some(point.buffer_position()),
                    col: Some(point.column()),
                    row: Some(point.row()),
                    width: Some(bounds.width().round().max(1.0) as i64),
                    height: Some(bounds.height().round().max(1.0) as i64),
                    anchor_x: None,
                    anchor_y: None,
                }
            }
            (crate::window::WindowPresentationSnapshot::LiveWindow(_), None)
            | (crate::window::WindowPresentationSnapshot::GeometryOnly(_), _) => MousePosnMetrics {
                point: fallback_point,
                col: None,
                row: None,
                width: None,
                height: None,
                anchor_x: None,
                anchor_y: None,
            },
        };
        let position = Self::mouse_posn_descriptor_value(MousePosnDescriptor {
            window_or_frame: Value::make_window(window_id.0),
            area,
            x: (x - coordinate_origin.x().get()).round() as i64,
            y: (y - coordinate_origin.y().get()).round() as i64,
            metrics,
        });
        let Some(posn_string) = Self::presented_string_position_value(frame, window_id, hit) else {
            return Some(position);
        };
        let Some(mut parts) = crate::emacs_core::value::list_to_vec(&position) else {
            return Some(position);
        };
        parts[4] = posn_string;
        Some(Value::list(parts))
    }

    fn presented_string_position_value(
        frame: &crate::window::Frame,
        window: crate::window::WindowId,
        hit: neomacs_display_protocol::PresentedHit,
    ) -> Option<Value> {
        let position = hit.string_position()?;
        let area = position.area();
        let snapshot = frame.active_window_presentation(window)?.display_snapshot();
        let source = snapshot
            .chrome_strings
            .iter()
            .find(|source| source.area() == area && source.string_id() == position.string())?;
        Some(Value::cons(
            source.value(),
            Value::fixnum(position.char_index().min(i64::MAX as u64) as i64),
        ))
    }

    /// Append modifier prefix characters to a symbol name string.
    pub(crate) fn append_modifier_prefix(modifiers: &Modifiers, out: &mut String) {
        if modifiers.ctrl {
            out.push_str("C-");
        }
        if modifiers.meta {
            out.push_str("M-");
        }
        if modifiers.shift {
            out.push_str("S-");
        }
        if modifiers.super_ {
            out.push_str("s-");
        }
        if modifiers.hyper {
            out.push_str("H-");
        }
    }

    pub(crate) fn current_idle_duration(&self) -> Option<std::time::Duration> {
        self.command_loop
            .idle_start_time
            .map(|start| start.elapsed())
    }

    pub(crate) fn current_idle_time_value(&self) -> Value {
        let Some(idle_duration) = self.current_idle_duration() else {
            return Value::NIL;
        };
        let secs = idle_duration.as_secs() as i64;
        let usecs = idle_duration.subsec_micros() as i64;
        Value::list(vec![
            Value::fixnum((secs >> 16) & 0xFFFF_FFFF),
            Value::fixnum(secs & 0xFFFF),
            Value::fixnum(usecs),
            Value::fixnum(0),
        ])
    }

    pub(crate) fn timer_start_idle(&mut self) {
        if self.command_loop.idle_start_time.is_some() {
            return;
        }
        let now = std::time::Instant::now();
        self.command_loop.idle_start_time = Some(now);
        self.command_loop.last_idle_start_time = Some(now);

        if self.obarray.fboundp("internal-timer-start-idle")
            && let Err(err) = self.apply(Value::symbol("internal-timer-start-idle"), vec![])
        {
            tracing::warn!("internal-timer-start-idle failed: {:?}", err);
        }
    }

    #[cfg(test)]
    pub(crate) fn idle_timer_running(&self) -> bool {
        self.command_loop.idle_start_time.is_some()
    }

    pub(crate) fn timer_stop_idle(&mut self) {
        if let Some(start) = self.command_loop.idle_start_time.take() {
            self.command_loop.last_idle_start_time = Some(start);
        }
    }

    pub(crate) fn timer_resume_idle(&mut self) {
        if self.command_loop.idle_start_time.is_none() {
            self.command_loop.idle_start_time = self.command_loop.last_idle_start_time;
        }
    }

    pub(crate) fn record_input_event(&mut self, event: Value) {
        self.assign("last-input-event", event);
        match self.command_loop.record_input_event(event) {
            NonmacroInputEvent::Counted => self.advance_num_nonmacro_input_events(),
            NonmacroInputEvent::SuppressedByMacroPlayback => {}
        }
    }

    /// GNU's `num_nonmacro_input_events` (`src/keyboard.c:106`), which is the
    /// `DEFVAR_INT` `num-nonmacro-input-events` (`src/keyboard.c:13903`) and
    /// not a separate global: `record_char` increments the same `intmax_t`
    /// Lisp reads, and `maybe_call_debugger` compares it against
    /// `when_entered_debugger` (`src/eval.c:2212`).
    ///
    /// Reading it through the forwarder rather than from a Rust field is the
    /// whole point -- a `setq` of the Lisp name has to move the counter, and
    /// before ledger 183 it did not, because there were two slots.
    pub(crate) fn num_nonmacro_input_events(&self) -> i64 {
        self.obarray
            .int_forwarder(num_nonmacro_input_events_symbol())
            .map_or(0, crate::emacs_core::forward::LispIntFwd::get_i64)
    }

    /// GNU `record_char`'s `num_nonmacro_input_events++` (`src/keyboard.c:3576`).
    fn advance_num_nonmacro_input_events(&mut self) {
        let next = self.num_nonmacro_input_events().saturating_add(1);
        if let Some(fwd) = self
            .obarray
            .int_forwarder(num_nonmacro_input_events_symbol())
        {
            fwd.set(crate::emacs_core::forward::LispInteger::from_i64(next));
        }
    }

    pub(crate) fn record_recent_command(&mut self, command: Value) {
        self.command_loop.record_recent_command(command);
    }

    pub(crate) fn record_nonmenu_input_event(&mut self, event: Value) {
        self.assign("last-nonmenu-event", event);
    }

    pub(crate) fn recent_input_events(&self) -> &[Value] {
        self.command_loop.recent_input_events()
    }

    pub(crate) fn clear_recent_input_events(&mut self) {
        self.command_loop.clear_recent_input_events();
    }

    pub(crate) fn set_command_key_sequences(&mut self, translated: Vec<Value>, raw: Vec<Value>) {
        self.command_loop.set_command_key_sequences(translated, raw);
    }

    pub(crate) fn set_translated_command_keys(&mut self, keys: Vec<Value>) {
        self.command_loop.set_translated_command_keys(keys);
    }

    pub(crate) fn set_read_command_keys(&mut self, keys: Vec<Value>) {
        self.command_loop.set_read_command_keys(keys);
    }

    pub(crate) fn clear_read_command_keys(&mut self) {
        self.command_loop.clear_read_command_keys();
    }

    pub(crate) fn read_command_keys(&self) -> &[Value] {
        self.command_loop.read_command_keys()
    }

    pub(crate) fn read_raw_command_keys(&self) -> &[Value] {
        self.command_loop.read_raw_command_keys()
    }

    pub(crate) fn sync_keyboard_macro_runtime_vars(&mut self) {
        self.assign(
            "defining-kbd-macro",
            if self.command_loop.keyboard.kboard.defining_kbd_macro {
                if self.command_loop.keyboard.kboard.appending_kbd_macro {
                    Value::symbol("append")
                } else {
                    Value::T
                }
            } else {
                Value::NIL
            },
        );
        // GNU stores the recorded macro directly in the variable cell
        // (KVAR(current_kboard, Vlast_kbd_macro) IS `last-kbd-macro'). Only
        // publish when a macro was actually recorded; otherwise leave the
        // variable alone so a user `(setq last-kbd-macro ...)' is preserved
        // (running a macro must not reset it to nil).
        if let Some(events) = self.command_loop.last_kbd_macro() {
            let last_kbd_macro = Value::vector(events.to_vec());
            self.assign("last-kbd-macro", last_kbd_macro);
        }
        let executing_kbd_macro = self
            .command_loop
            .keyboard
            .kboard
            .executing_kbd_macro
            .as_ref()
            .map(|events| Value::vector(events.clone()))
            .unwrap_or(Value::NIL);
        self.assign("executing-kbd-macro", executing_kbd_macro);
        self.assign(
            "executing-kbd-macro-index",
            Value::fixnum(self.command_loop.keyboard.kboard.kbd_macro_index as i64),
        );
    }

    pub(crate) fn start_kbd_macro_runtime(
        &mut self,
        initial_events: Option<&[Value]>,
        append: bool,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        if self.command_loop.keyboard.kboard.defining_kbd_macro {
            return Err(crate::emacs_core::error::signal(
                "error",
                vec![Value::string("Already defining a keyboard macro")],
            ));
        }
        self.command_loop
            .start_kbd_macro_with_initial(initial_events, append);
        self.sync_keyboard_macro_runtime_vars();
        Ok(())
    }

    pub(crate) fn store_kbd_macro_runtime_event(&mut self, event: Value) {
        self.command_loop.store_kbd_macro_event(event);
    }

    pub(crate) fn finalize_kbd_macro_runtime_chars(&mut self) {
        self.command_loop.finalize_kbd_macro_chars();
    }

    pub(crate) fn cancel_kbd_macro_runtime_events(&mut self) {
        self.command_loop.cancel_kbd_macro_events();
    }

    pub(crate) fn end_kbd_macro_runtime(
        &mut self,
    ) -> Result<Vec<Value>, crate::emacs_core::error::Flow> {
        if !self.command_loop.keyboard.kboard.defining_kbd_macro {
            return Err(crate::emacs_core::error::signal(
                "error",
                vec![Value::string("Not defining a keyboard macro")],
            ));
        }
        let previous = self
            .command_loop
            .last_kbd_macro()
            .map(|events| events.to_vec());
        let recorded = self.command_loop.end_kbd_macro();
        if let Some(previous) = previous {
            self.kmacro.macro_ring.push(previous);
        }
        self.sync_keyboard_macro_runtime_vars();
        Ok(recorded)
    }

    pub(crate) fn begin_executing_kbd_macro_runtime(&mut self, events: Vec<Value>) {
        self.command_loop.begin_executing_kbd_macro(events);
        self.sync_keyboard_macro_runtime_vars();
    }

    pub(crate) fn snapshot_executing_kbd_macro_runtime(&self) -> ExecutingKbdMacroRuntimeSnapshot {
        self.command_loop
            .keyboard
            .kboard
            .snapshot_executing_kbd_macro_runtime()
    }

    pub(crate) fn restore_executing_kbd_macro_runtime(
        &mut self,
        snapshot: ExecutingKbdMacroRuntimeSnapshot,
    ) {
        self.command_loop
            .keyboard
            .kboard
            .restore_executing_kbd_macro_runtime(snapshot);
        self.sync_keyboard_macro_runtime_vars();
    }

    pub(crate) fn set_executing_kbd_macro_runtime_index(&mut self, index: usize) {
        self.command_loop
            .keyboard
            .kboard
            .set_executing_kbd_macro_index(index);
        self.sync_keyboard_macro_runtime_vars();
    }

    pub(crate) fn note_executing_kbd_macro_iteration(&mut self, success_count: usize) {
        self.command_loop
            .keyboard
            .kboard
            .note_executing_kbd_macro_iteration(success_count);
        self.sync_keyboard_macro_runtime_vars();
    }

    #[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
    pub(crate) fn finish_executing_kbd_macro_runtime(&mut self) {
        self.command_loop.finish_executing_kbd_macro();
        self.sync_keyboard_macro_runtime_vars();
    }

    pub(crate) fn clear_command_key_state(&mut self, keep_record: bool) {
        self.clear_read_command_keys();
        if !keep_record {
            self.clear_recent_input_events();
        }
    }

    pub(crate) fn set_this_command_keys_from_string(
        &mut self,
        keys: &LispString,
    ) -> Result<(), crate::emacs_core::error::Flow> {
        let key_bytes = keys.as_bytes();
        let mut translated = Vec::new();
        let mut pos = 0;
        let mut idx = 0;
        while pos < key_bytes.len() {
            let (mut code, len) = crate::emacs_core::emacs_char::string_char(&key_bytes[pos..]);
            // Match GNU `keyboard.c:12239-12252`: byte8 chars are normalized
            // back to raw 8-bit bytes before the `M-x` special case runs.
            if crate::emacs_core::emacs_char::char_byte8_p(code) {
                code = crate::emacs_core::emacs_char::char_to_byte8(code) as u32;
            }
            let event = if idx == 0 && code == 248 {
                Value::fixnum(('x' as i64) | KEY_CHAR_META)
            } else {
                Value::fixnum(code as i64)
            };
            translated.push(event);
            pos += len;
            idx += 1;
        }
        self.set_command_key_sequences(translated, Vec::new());
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Interactive spec parsing
// ---------------------------------------------------------------------------

/// Parsed interactive argument specification.
#[derive(Clone, Debug)]
pub enum InteractiveCode {
    /// No arguments.
    None,
    /// Buffer name (with completion).
    BufferName(LispString),
    /// Character.
    Character(LispString),
    /// Point (cursor position).
    Point,
    /// Mark.
    Mark,
    /// Region (point and mark).
    Region,
    /// String from minibuffer.
    StringArg(LispString),
    /// Number from minibuffer.
    NumberArg(LispString),
    /// File name (with completion).
    FileName(LispString),
    /// Directory name.
    DirectoryName(LispString),
    /// Prefix argument (numeric).
    PrefixNumeric,
    /// Raw prefix argument.
    PrefixRaw,
    /// Function name (with completion).
    FunctionName(LispString),
    /// Variable name (with completion).
    VariableName(LispString),
    /// Command name (with completion).
    CommandName(LispString),
    /// Key sequence.
    KeySequenceArg(LispString),
    /// Lisp expression.
    Expression(LispString),
}

fn interactive_prompt_lisp_string(prompt: &str) -> LispString {
    crate::emacs_core::builtins::plain_str_to_lisp_string(prompt, !prompt.is_ascii())
}

/// Parse an interactive specification string.
/// Example: "sSearch for: \nnRepeat count: "
pub fn parse_interactive_spec(spec: &str) -> Vec<InteractiveCode> {
    if spec.is_empty() {
        return vec![InteractiveCode::None];
    }

    let mut codes = Vec::new();
    let parts: Vec<&str> = spec.split('\n').collect();

    for part in parts {
        if part.is_empty() {
            continue;
        }
        let code = part.chars().next().unwrap();
        let prompt = &part[1..];
        let prompt = interactive_prompt_lisp_string(prompt);

        codes.push(match code {
            'b' => InteractiveCode::BufferName(prompt.clone()),
            'B' => InteractiveCode::BufferName(prompt.clone()),
            'c' => InteractiveCode::Character(prompt.clone()),
            'd' => InteractiveCode::Point,
            'm' => InteractiveCode::Mark,
            'r' => InteractiveCode::Region,
            's' => InteractiveCode::StringArg(prompt.clone()),
            'S' => InteractiveCode::StringArg(prompt.clone()),
            'n' => InteractiveCode::NumberArg(prompt.clone()),
            'N' => InteractiveCode::NumberArg(prompt.clone()),
            'f' => InteractiveCode::FileName(prompt.clone()),
            'F' => InteractiveCode::FileName(prompt.clone()),
            'D' => InteractiveCode::DirectoryName(prompt.clone()),
            'p' => InteractiveCode::PrefixNumeric,
            'P' => InteractiveCode::PrefixRaw,
            'a' => InteractiveCode::FunctionName(prompt.clone()),
            'C' => InteractiveCode::CommandName(prompt.clone()),
            'v' => InteractiveCode::VariableName(prompt.clone()),
            'k' => InteractiveCode::KeySequenceArg(prompt.clone()),
            'x' | 'X' => InteractiveCode::Expression(prompt.clone()),
            _ => InteractiveCode::StringArg(prompt),
        });
    }

    codes
}

fn key_sequence_translation_events(translation: Value) -> Option<Vec<Value>> {
    if translation.is_nil() || translation.is_fixnum() {
        return None;
    }
    if crate::emacs_core::keymap::is_list_keymap(&translation) {
        return None;
    }

    if translation.is_vector() {
        return Some(translation.as_vector_data()?.to_vec());
    }

    if let Some(s) = translation.as_utf8_str() {
        return Some(s.chars().map(|ch| Value::fixnum(ch as i64)).collect());
    }

    Some(vec![translation])
}

// ===========================================================================
// Tests
// ===========================================================================

impl crate::emacs_core::eval::Context {
    /// Smooth scroll (Phase 1, T4): accumulate a trackpad pixel-scroll delta for
    /// `target_frame_id`. Deltas for the same frame sum (sub-pixel precision); a
    /// delta for a different frame replaces the pending one. Drained + applied by
    /// the layout pass via `Engine::pixel_scroll_window`.
    pub fn accumulate_pending_pixel_scroll(
        &mut self,
        target_frame: crate::window::FrameId,
        delta_y: f32,
    ) {
        self.pending_pixel_scroll = Some(PendingPixelScroll::accumulate(
            self.pending_pixel_scroll,
            target_frame,
            delta_y,
        ));
    }

    /// Observe pending scroll without taking ownership from the next redisplay.
    pub fn pending_pixel_scroll_for_frame(&self, frame: crate::window::FrameId) -> Option<f32> {
        self.pending_pixel_scroll
            .and_then(|pending| pending.for_frame(frame))
    }

    /// Smooth scroll (Phase 1): take the pending trackpad pixel-scroll delta if it
    /// targets `frame`, clearing it; returns the accumulated `delta_y`. The layout
    /// pass converts it to pixels and applies it via `Engine::pixel_scroll_window`.
    pub fn take_pending_pixel_scroll_for_frame(
        &mut self,
        frame: crate::window::FrameId,
    ) -> Option<f32> {
        match self.pending_pixel_scroll {
            Some(pending) if pending.frame == frame => {
                self.pending_pixel_scroll = None;
                Some(pending.delta_y)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod pixel_scroll_accumulate_tests {
    #[test]
    fn accumulate_pixel_scroll_sums_same_frame_and_replaces_other() {
        let mut eval = crate::emacs_core::eval::Context::new();
        let frame_7 = crate::window::FrameId(7);
        let frame_9 = crate::window::FrameId(9);
        eval.accumulate_pending_pixel_scroll(frame_7, 3.5);
        eval.accumulate_pending_pixel_scroll(frame_7, 2.0);
        assert_eq!(
            eval.pending_pixel_scroll_for_frame(frame_7),
            Some(5.5),
            "same-frame deltas sum (sub-pixel accumulation)"
        );
        eval.accumulate_pending_pixel_scroll(frame_9, -1.0);
        assert_eq!(
            eval.pending_pixel_scroll_for_frame(frame_9),
            Some(-1.0),
            "a different frame replaces the pending delta"
        );
        assert_eq!(eval.pending_pixel_scroll_for_frame(frame_7), None);
    }
}

#[cfg(test)]
#[path = "keyboard_test.rs"]
mod tests;
