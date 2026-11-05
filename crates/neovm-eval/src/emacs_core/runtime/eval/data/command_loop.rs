//! Command-loop and keyboard state owned by the evaluator (stage-4 W2/E6b).
//!
//! Plain data moved verbatim out of `keyboard.rs`: the command loop, the
//! keyboard runtime with its active and parked `KBoard` state, prefix
//! arguments, presented-interaction records and the echo-keystroke state,
//! together with the fixed keyboard symbol table the read path shares with it.
//! Display and system behaviour operate on this state from above; the GC walk
//! keeps the same per-field visit order (`gc_pacing.rs`, group `command_loop`).

use std::collections::{HashMap, VecDeque};
use std::time::Instant;

use neovm_value::display_vocab::FrameId;
use neovm_value::heap_types::LispString;
use neovm_value::system_vocab::input::{ExecutingKbdMacroRuntimeSnapshot, InputPendingFilter};

use crate::emacs_core::eval::frontend_events::FrontendEventQueue;
use crate::emacs_core::eval::keyboard::{InputEvent, KeyEvent, MouseButton};
use crate::emacs_core::eval::keyboard_input::KeyboardInputDecoder;
use neovm_edit::buffer::{CharLen, CharPos0, CharRange};
use neovm_value::intern::{WellKnown, intern};
use neovm_value::tagged::value::SymbolMode;
use neovm_value::value::Value;

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

impl neovm_value::gc_trace::GcTrace for PresentedInteractions {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        for presentation in self.presentations.values() {
            roots.extend(presentation.targets.iter().map(|target| target.posn_string));
        }
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
}

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
    pub frame_id: Option<FrameId>,
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
#[cfg_attr(feature = "test-support", visibility::make(pub))]
pub(crate) enum KeyEchoState {
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

/// Which of GNU's two echo shapes a pending key string is built in.
///
/// `echo_now` starts echoing and decorates the key string with the "type more"
/// dash and the keystroke-help hint (`echo_dash`, keyboard.c:552-600).  Every
/// later key rebuilds the string from `this_command_keys` alone (`echo_update`,
/// keyboard.c:596-626), which removes the dash and hint; the next read's
/// `echo_dash` puts them back before it waits (keyboard.c:2759-2768).  A
/// pending sub-read therefore stays decorated, while a command that completed
/// keeps the plain string the last `echo_update` displayed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyEchoDecoration {
    /// An echo starts or is resumed for a new read: GNU `echo_now` /
    /// `read_char` -> `echo_dash`.
    Start,
    /// A key arrived while echoing: GNU `echo_update` rebuilds without them.
    Refresh,
}

impl neovm_value::gc_trace::GcTrace for KeyEchoState {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.trace_roots_with(&mut |root| roots.push(root));
    }

    fn trace_roots_with(&self, visit: &mut dyn FnMut(Value)) {
        match self {
            Self::Inactive => {}
            Self::Immediate { prompt } => {
                if let Some(text) = prompt {
                    text.trace_roots_with(visit);
                }
            }
        }
    }
}

/// Semantic pieces of a keyboard-owned echo message.
///
/// Keeping key names distinct from prose makes it impossible to change the
/// wording while accidentally retaining GNU's old numeric property offsets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum KeyEchoSegment {
    Plain(&'static str),
    KeyBinding(&'static str),
}

impl KeyEchoSegment {
    #[doc(hidden)]
    pub const fn text(self) -> &'static str {
        match self {
            Self::Plain(text) | Self::KeyBinding(text) => text,
        }
    }
}

/// Build GNU `echo_add_key`'s help-event suffix with its semantic key names
/// carrying the `help-key-binding` face.
pub(crate) fn help_event_echo_suffix() -> LispString {
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
    pub(crate) tty_input_decoder: KeyboardInputDecoder,
    /// Deferred switch-frame/select-window event that should be delivered
    /// before ordinary unread input, matching GNU
    /// `unread_switch_frame` plus read-key-sequence delayed selection events.
    pub unread_selection_event: Option<Value>,
    /// Last frame observed by `internal-handle-focus-in`, matching GNU
    /// `internal_last_event_frame`.
    pub internal_last_event_frame: Option<FrameId>,
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
    pub(crate) key_echo_state: KeyEchoState,
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
    /// Neomacs's recursion guard for `input-method-function`.
    /// Quail also binds the Lisp function to nil during its nested reads.
    /// The saved-reader call scopes this flag across normal and error returns.
    pub in_input_method_function: bool,
    /// Last mouse down event seen by the event-ingest path. GNU
    /// `keyboard.c:6041-6130` (`make_lispy_event` / the
    /// `button_down_time` / `last_mouse_click` globals) tracks
    /// the previous click's button, position, frame, and
    /// timestamp so it can compute the click count for
    /// `double-click-time` / `double-click-fuzz` comparisons.
    /// Keyboard audit Finding 12.
    pub last_mouse_click: Option<LastMouseClick>,
    /// Per-button down positions while the button is held.  GNU keeps one
    /// `start_pos` per button in `button_down_location`; taking the entry on
    /// release is what lets a stray up event be dropped
    /// (`src/keyboard.c:6564-6566`, `6628-6632`).
    pub button_down: [Option<MousePressState>; 5],
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

/// One entry of GNU's `button_down_location` table: the down event's
/// position while its button is held.  The entry is taken on the matching
/// release, so a later up event without a new down is dropped
/// (`src/keyboard.c:6564-6566`, `6628-6632`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MousePressState {
    pub x: f32,
    pub y: f32,
    pub frame_id: u64,
    /// Click count computed for this down event; the release reuses it.
    pub click_count: u32,
    /// The full Lisp position copied from the down event: a drag event's
    /// START (`src/keyboard.c:6615`, `6739-6741`).
    pub position: Value,
}

/// Payload shape of a mouse event, mirroring GNU `make_lispy_event`
/// (`src/keyboard.c:6739-6745`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum MouseEventPayload {
    /// `(HEAD POSITION)`.
    Position(Value),
    /// `(HEAD START END)`: START is the down event's position.
    Drag { start: Value, end: Value },
    /// `(HEAD POSITION COUNT)` for a double/triple click.
    WithClickCount { position: Value, count: u32 },
}

impl KBoard {
    /// Copied key-echo state for the re-homed keyboard oracle; the KBoard
    /// field stays private (N1).
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn key_echo_state_copied(&self) -> KeyEchoState {
        self.key_echo_state.clone()
    }

    pub fn new() -> Self {
        Self {
            tty_input_decoder: KeyboardInputDecoder::default(),
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
            button_down: [None; 5],
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

    pub fn internal_last_event_frame(&self) -> Option<FrameId> {
        self.internal_last_event_frame
    }

    pub fn set_internal_last_event_frame(&mut self, frame_id: FrameId) {
        self.internal_last_event_frame = Some(frame_id);
    }

    #[doc(hidden)]
    pub fn clear_internal_last_event_frame(&mut self) {
        self.internal_last_event_frame = None;
    }

    pub fn mouse_pixel_position(&self) -> Option<MousePixelPositionState> {
        self.mouse_pixel_position
    }

    pub fn set_mouse_pixel_position(&mut self, frame_id: Option<FrameId>, x: i64, y: i64) {
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

    #[doc(hidden)]
    pub fn snapshot_executing_kbd_macro_runtime(&self) -> ExecutingKbdMacroRuntimeSnapshot {
        ExecutingKbdMacroRuntimeSnapshot {
            events: self.executing_kbd_macro.clone(),
            index: self.kbd_macro_index,
        }
    }

    #[doc(hidden)]
    pub fn restore_executing_kbd_macro_runtime(
        &mut self,
        snapshot: ExecutingKbdMacroRuntimeSnapshot,
    ) {
        self.executing_kbd_macro = snapshot.events;
        self.kbd_macro_index = snapshot.index;
    }

    #[doc(hidden)]
    pub fn set_executing_kbd_macro_index(&mut self, index: usize) {
        self.kbd_macro_index = index;
    }

    #[doc(hidden)]
    pub fn note_executing_kbd_macro_iteration(&mut self, success_count: usize) {
        self.executing_kbd_macro_iterations = success_count;
    }
}

impl Default for KBoard {
    fn default() -> Self {
        Self::new()
    }
}

impl neovm_value::gc_trace::GcTrace for KBoard {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        // GNU marks echo_prompt for every keyboard, including parked ones
        // (keyboard.c:14638); the Context echo message is only the active copy.
        self.key_echo_state.trace_roots(roots);
        if let Some(event) = self.unread_selection_event {
            roots.push(event);
        }
        if let Some(event) = self.last_help_echo_event {
            roots.push(event);
        }
        for press in self.button_down.iter().flatten() {
            roots.push(press.position);
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
    pub pending_input_events: FrontendEventQueue,
    /// Terminal id for the currently active `kboard`.
    active_terminal_id: u64,
    /// Parked keyboard-local state for terminals that are not currently active.
    parked_kboards: HashMap<u64, KBoard>,
    /// Keyboard-local command-loop state.
    pub kboard: KBoard,
}

/// GNU's `num_nonmacro_input_events` slot, reached by its Lisp name because
/// in GNU the Lisp name *is* the slot (`src/keyboard.c:13903`).
pub(crate) fn num_nonmacro_input_events_symbol() -> neovm_value::intern::SymId {
    static SYMBOL: std::sync::OnceLock<neovm_value::intern::SymId> = std::sync::OnceLock::new();
    *SYMBOL.get_or_init(|| intern("num-nonmacro-input-events"))
}

// Fixed keyboard spellings with GNU DEFSYM ids. GNU holds these as the
// `Qundefined` DEFSYM and the DEFVAR_LISP queue variables; the read-char path
// compared the printed spelling of `undefined` and re-interned both queue
// names on every input-method probe.
pub(crate) static UNDEFINED_SYMBOL: WellKnown = WellKnown::Undefined;
pub(crate) static SPECIAL_EVENT_MAP: WellKnown = WellKnown::SpecialEventMap;
pub(crate) static LAST_INPUT_EVENT: WellKnown = WellKnown::LastInputEvent;
pub(crate) static COMMAND_EXECUTE: WellKnown = WellKnown::CommandExecute;
pub(crate) static INTERNAL_TIMER_START_IDLE: WellKnown = WellKnown::InternalTimerStartIdle;
pub(crate) static HELP_CHAR: WellKnown = WellKnown::HelpChar;
pub(crate) static HELP_EVENT_LIST: WellKnown = WellKnown::HelpEventList;
pub(crate) static ECHO_KEYSTROKES: WellKnown = WellKnown::EchoKeystrokes;
pub(crate) static ECHO_KEYSTROKES_HELP: WellKnown = WellKnown::EchoKeystrokesHelp;
pub(crate) static EXECUTING_KBD_MACRO: WellKnown = WellKnown::ExecutingKbdMacro;
pub(crate) static INPUT_METHOD_FUNCTION: WellKnown = WellKnown::InputMethodFunction;
pub(crate) static KEY_TRANSLATION_MAP: WellKnown = WellKnown::KeyTranslationMap;
pub(crate) static KEYBOARD_TRANSLATE_TABLE: WellKnown = WellKnown::KeyboardTranslateTable;
pub(crate) static LAST_NONMENU_EVENT: WellKnown = WellKnown::LastNonmenuEvent;
pub(crate) static THIS_COMMAND_KEYS_SHIFT_TRANSLATED: WellKnown =
    WellKnown::ThisCommandKeysShiftTranslated;
pub(crate) static TRANSLATE_UPPER_CASE_KEY_BINDINGS: WellKnown =
    WellKnown::TranslateUpperCaseKeyBindings;
pub(crate) static PREFIX_HELP_COMMAND: WellKnown = WellKnown::PrefixHelpCommand;
pub(crate) static LAST_KBD_MACRO: WellKnown = WellKnown::LastKbdMacro;
pub(crate) static AUTO_SAVE_TIMEOUT: WellKnown = WellKnown::AutoSaveTimeout;
pub(crate) static UNREAD_POST_INPUT_METHOD_EVENTS: WellKnown =
    WellKnown::UnreadPostInputMethodEvents;
pub(crate) static UNREAD_INPUT_METHOD_EVENTS: WellKnown = WellKnown::UnreadInputMethodEvents;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum InputEventHistoryDisposition {
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

impl KeyboardRuntime {
    #[doc(hidden)]
    pub fn kboards(&self) -> impl Iterator<Item = &KBoard> {
        std::iter::once(&self.kboard).chain(self.parked_kboards.values())
    }

    #[doc(hidden)]
    pub fn input_event_history_disposition(&self) -> InputEventHistoryDisposition {
        if self.kboard.is_executing_kbd_macro() {
            InputEventHistoryDisposition::SuppressDuringMacroPlayback
        } else {
            InputEventHistoryDisposition::Record
        }
    }

    #[doc(hidden)]
    pub fn low_level_event_kind(event: Value, mode: SymbolMode) -> Option<&'static str> {
        let head = if event.is_cons() {
            event.cons_car()
        } else {
            event
        };
        head.as_symbol_name(mode)
    }

    #[doc(hidden)]
    pub fn low_level_event_counts_as_pending(
        event: Value,
        filter: InputPendingFilter,
        ignored_while_no_input: &impl Fn(&str) -> bool,
        mode: SymbolMode,
    ) -> bool {
        Self::low_level_event_kind(event, mode)
            .is_none_or(|kind| !filter.ignores(kind, ignored_while_no_input))
    }

    pub fn new() -> Self {
        Self {
            event_queue: VecDeque::new(),
            pending_input_events: FrontendEventQueue::default(),
            active_terminal_id: crate::emacs_core::eval::protocol::tty_host::TERMINAL_ID,
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

    pub fn set_mouse_pixel_position(&mut self, frame_id: Option<FrameId>, x: i64, y: i64) {
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

    #[doc(hidden)]
    pub fn parked_terminal_ids(&self) -> Vec<u64> {
        let mut ids =
            crate::emacs_core::eval::lifecycle::live_terminal_ids_in_keyboard_poll_order()
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

    #[doc(hidden)]
    pub fn poll_parked_kboard<R>(
        &mut self,
        mut f: impl FnMut(&mut KBoard) -> Option<R>,
    ) -> Option<R> {
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

    #[doc(hidden)]
    pub fn set_unread_selection_event(&mut self, event: Value) {
        self.kboard.set_unread_selection_event(event);
    }

    #[doc(hidden)]
    pub fn has_unread_selection_event(&self) -> bool {
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
    #[doc(hidden)]
    pub fn has_pending_low_level_input_for_query(
        &self,
        filter: InputPendingFilter,
        ignored_while_no_input: impl Fn(&str) -> bool,
        mode: SymbolMode,
    ) -> bool {
        self.kboards().any(|kboard| {
            kboard.unread_events.iter().copied().any(|event| {
                Self::low_level_event_counts_as_pending(
                    event,
                    filter,
                    &ignored_while_no_input,
                    mode,
                )
            })
        })
    }

    /// Apply one GNU-compatible command-input policy to every queue owned by
    /// the keyboard runtime. Deferred selection events remain readable but do
    /// not count as pending, matching GNU's separate `unread_switch_frame`.
    #[doc(hidden)]
    pub fn has_pending_command_input_for_query(
        &self,
        filter: InputPendingFilter,
        track_mouse: bool,
        ignored_while_no_input: impl Fn(&str) -> bool,
        mode: SymbolMode,
    ) -> bool {
        self.has_pending_low_level_input_for_query(filter, &ignored_while_no_input, mode)
            || self.pending_input_events.has_input(
                crate::emacs_core::eval::frontend_events::FrontendInputQuery::Pending(filter),
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

impl neovm_value::gc_trace::GcTrace for KeyboardRuntime {
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
    /// Next bounded GUI service opportunity while command input stays queued.
    pub(crate) gui_display_deadline: Option<Instant>,
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
    pub(crate) idle_start_time: Option<std::time::Instant>,
    /// Last idle epoch preserved across non-user internal events.
    pub(crate) last_idle_start_time: Option<std::time::Instant>,
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
    pub(crate) last_non_minibuffer_size: usize,
}

impl CommandLoop {
    /// Copied idle epoch observers for the re-homed keyboard oracle; the
    /// fields stay private (N1).
    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn idle_start_time_for_test(&self) -> Option<std::time::Instant> {
        self.idle_start_time
    }

    #[cfg(any(test, feature = "test-support"))]
    #[cfg_attr(feature = "test-support", visibility::make(pub))]
    pub(crate) fn last_idle_start_time_for_test(&self) -> Option<std::time::Instant> {
        self.last_idle_start_time
    }

    pub fn new() -> Self {
        Self {
            keyboard: KeyboardRuntime::new(),
            presented_interactions: PresentedInteractions::default(),
            gui_display_deadline: None,
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

impl neovm_value::gc_trace::GcTrace for CommandLoop {
    fn trace_roots(&self, roots: &mut Vec<Value>) {
        self.keyboard.trace_roots(roots);
        self.presented_interactions.trace_roots(roots);
    }
}
