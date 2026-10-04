//! Terminal/TTY builtins extracted from display.rs and builtins.rs.
//!
//! Provides the terminal runtime owner, terminal parameter storage,
//! and all terminal/tty query builtins.

use crate::emacs_core::error::LispCondition;
use crate::emacs_core::error::{EvalResult, Flow, signal};
use crate::emacs_core::error::{expect_args, expect_args_range, expect_max_args};
use crate::emacs_core::heap_registry::{HeapRegistryHandle, HeapRegistrySlot};
use crate::emacs_core::value::*;
use crate::emacs_core::value::{ValueKind, VecLikeType};
use crate::window::FrameId;
use neomacs_display_protocol::tty_capabilities::TtyAttributeCapabilities;
use std::cell::{OnceCell, RefCell};
use std::collections::{HashMap, hash_map::Entry};
use std::num::NonZeroU32;

// ---------------------------------------------------------------------------
// Thread-local terminal state
// ---------------------------------------------------------------------------

thread_local! {
    static TERMINAL_MANAGER: OnceCell<RefCell<TerminalManager>> = const { OnceCell::new() };
    static TERMINAL_LISP_STATE: HeapRegistrySlot<TerminalLispRegistry> =
        HeapRegistrySlot::new(TerminalLispRegistry::default());
}

struct TerminalLispState {
    handle: Value,
    params: Vec<(Value, Value)>,
}

/// Only Lisp state travels with Context; native hosts and runtime state stay TLS.
#[derive(Default)]
pub(crate) struct TerminalLispRegistry {
    terminals: HashMap<u64, TerminalLispState>,
    // GNU prepends native terminals, then Fterminal_list reverses them again.
    // Preserve creation order independently of hash iteration and terminal ids.
    creation_order: Vec<u64>,
}

impl TerminalLispRegistry {
    fn ensure_terminal(&mut self, id: u64) -> &mut TerminalLispState {
        match self.terminals.entry(id) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => {
                self.creation_order.push(id);
                entry.insert(TerminalLispState {
                    handle: Value::make_terminal(id),
                    params: Vec::new(),
                })
            }
        }
    }
}

pub(crate) type TerminalRegistryHandle = HeapRegistryHandle<TerminalLispRegistry>;

fn ensure_current_terminal_registry() {
    let heap_identity = crate::tagged::gc::current_tagged_heap_identity()
        .unwrap_or_else(|| crate::tagged::gc::with_tagged_heap(|heap| heap.identity()));
    TERMINAL_LISP_STATE.with(|slot| {
        if slot.current().heap_identity() != heap_identity {
            slot.reset(TerminalLispRegistry::default());
        }
    });
}

pub(crate) fn current_terminal_registry_handle() -> TerminalRegistryHandle {
    ensure_current_terminal_registry();
    // Context construction captures the initial terminal along with its registry.
    terminal_handle_for_id(TERMINAL_ID);
    TERMINAL_LISP_STATE.with(HeapRegistrySlot::current)
}

pub(crate) fn install_terminal_registry_handle(handle: &TerminalRegistryHandle) {
    TERMINAL_LISP_STATE.with(|slot| slot.install(handle));
    let ids = handle.borrow().creation_order.clone();
    // A destination thread may not have seen these terminal ids before. Create
    // inactive native records without moving a host or duplicating Lisp Values.
    TERMINAL_MANAGER.with(|state| {
        let slot = state.get_or_init(|| RefCell::new(TerminalManager::new()));
        let mut manager = slot.borrow_mut();
        // A reset or another Context can leave native records absent from this
        // registry, including the default initial terminal after its deletion.
        // Keep their tombstones so next_terminal_id does not reuse their ids.
        for terminal in &mut manager.terminals {
            if !ids.contains(&terminal.id) {
                terminal.mark_deleted();
            }
        }
        for id in &ids {
            manager.ensure_lisp_terminal_record(*id);
        }
        manager.terminals.sort_by_key(|terminal| {
            ids.iter()
                .position(|id| *id == terminal.id)
                .unwrap_or(ids.len())
        });
    });
}

pub(crate) fn collect_terminal_registry_gc_roots(
    handle: &TerminalRegistryHandle,
    roots: &mut Vec<Value>,
) {
    for terminal in handle.borrow().terminals.values() {
        roots.push(terminal.handle);
        for (key, value) in &terminal.params {
            roots.push(*key);
            roots.push(*value);
        }
    }
}

/// The physical terminal records contain no Lisp Values. Only the installed
/// Context-owned registry supplies handles and parameters to terminal APIs.
fn with_terminal_manager<R>(f: impl FnOnce(&RefCell<TerminalManager>) -> R) -> R {
    ensure_current_terminal_registry();
    TERMINAL_MANAGER.with(|state| {
        let slot = state.get_or_init(|| RefCell::new(TerminalManager::new()));
        f(slot)
    })
}

pub(crate) const TERMINAL_NAME: &str = "initial_terminal";
pub(crate) const TERMINAL_ID: u64 = 0;

#[derive(Debug, Clone, PartialEq, Eq)]
struct TerminalRuntime {
    active: bool,
    tty_type: Option<String>,
    color_cells: i64,
    controlling_tty: bool,
    suspended: bool,
    /// What this terminal can render, from its terminfo entry -- GNU's `TS_*`
    /// capability strings on `struct tty_display_info`. Answers
    /// `display-supports-face-attributes-p` (GNU `tty_capable_p`) with the same
    /// record the renderer emits from, so the predicate and the output cannot
    /// disagree about, say, whether this terminal has `sitm`.
    attribute_capabilities: TtyAttributeCapabilities,
}

impl TerminalRuntime {
    fn inactive() -> Self {
        Self {
            active: false,
            tty_type: None,
            color_cells: 0,
            controlling_tty: false,
            suspended: false,
            // GNU's initial terminal has no capability strings until a real
            // terminal is initialized from terminfo, which is why
            // `display-supports-face-attributes-p' answers nil for everything in
            // `--batch'.
            attribute_capabilities: TtyAttributeCapabilities::none(),
        }
    }

    fn supports_color(&self) -> bool {
        self.color_cells > 0
    }
}

pub use super::config::{TerminalRuntimeConfig, TtyTerminalConfig};

pub trait TerminalHost {
    fn suspend_tty(&mut self) -> Result<(), String>;
    fn resume_tty(&mut self) -> Result<(), String>;
    fn delete_terminal(&mut self) -> Result<(), String> {
        Ok(())
    }
    /// GNU `Fsend_string_to_terminal`'s `fwrite`+`fflush` on the terminal's
    /// output stream (src/dispnew.c:6838-6843).  The host owns the fd, so the
    /// write and its flush belong behind this method.  A host that owns no
    /// output stream rejects; the caller distinguishes GNU's "not a termcap
    /// terminal device" from "Terminal is currently suspended" using the
    /// terminal record, not this error.
    fn write_bytes(&mut self, _bytes: &[u8]) -> Result<(), String> {
        Err("terminal host has no output stream".to_owned())
    }
}

/// Validated identity of a text terminal a frontend must open for one frame.
///
/// The Lisp-facing `tty` and `tty-type` parameters are loose values; this is
/// the narrow, owned request that crosses from the VM into platform code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TtyFrameOpenRequest {
    terminal_id: u64,
    frame_id: FrameId,
    device: String,
    terminal_type: String,
}

impl TtyFrameOpenRequest {
    pub fn new(
        terminal_id: u64,
        frame_id: FrameId,
        device: String,
        terminal_type: String,
    ) -> Result<Self, String> {
        if device.is_empty() {
            return Err("Invalid terminal device".to_string());
        }
        if terminal_type.is_empty() {
            return Err("Invalid terminal type".to_string());
        }
        Ok(Self {
            terminal_id,
            frame_id,
            device,
            terminal_type,
        })
    }

    pub fn terminal_id(&self) -> u64 {
        self.terminal_id
    }

    pub fn frame_id(&self) -> FrameId {
        self.frame_id
    }

    pub fn device(&self) -> &str {
        &self.device
    }

    pub fn terminal_type(&self) -> &str {
        &self.terminal_type
    }
}

/// Character-cell dimensions of an opened TTY. Zero-sized terminals cannot
/// enter the frame model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TtyFrameSize {
    columns: NonZeroU32,
    rows: NonZeroU32,
}

impl TtyFrameSize {
    pub fn new(columns: u32, rows: u32) -> Option<Self> {
        Some(Self {
            columns: NonZeroU32::new(columns)?,
            rows: NonZeroU32::new(rows)?,
        })
    }

    pub fn columns(self) -> u32 {
        self.columns.get()
    }

    pub fn rows(self) -> u32 {
        self.rows.get()
    }
}

/// Resources returned only after platform code has successfully opened and
/// initialized a TTY.
pub struct OpenedTtyFrameHost {
    size: TtyFrameSize,
    attribute_capabilities: TtyAttributeCapabilities,
    /// The ERASE byte of the modes this terminal had before raw mode was
    /// entered.  The host reads it at open because only the host owns the
    /// descriptor; the evaluator publishes it as `tty-erase-char`, as GNU's
    /// `init_sys_modes` does for every terminal it initializes
    /// (src/sysdep.c:1130) — a daemon must not keep the answer it read from
    /// its own stdin.
    erase_char: u8,
    host: Box<dyn TerminalHost>,
}

impl OpenedTtyFrameHost {
    pub fn new(
        size: TtyFrameSize,
        attribute_capabilities: TtyAttributeCapabilities,
        erase_char: u8,
        host: Box<dyn TerminalHost>,
    ) -> Self {
        Self {
            size,
            attribute_capabilities,
            erase_char,
            host,
        }
    }

    pub fn erase_char(&self) -> u8 {
        self.erase_char
    }
}

/// Frontend-owned factory for OS terminal resources.
///
/// `neovm-core` owns Lisp/frame/terminal identity; the binary owns file
/// descriptors, raw mode, input threads, and the renderer bound to them.
pub trait TtyFrameHostFactory {
    fn open_tty(&mut self, request: TtyFrameOpenRequest) -> Result<OpenedTtyFrameHost, String>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeleteTerminalMode {
    Public { force_non_nil: bool },
    Noelisp,
}

impl DeleteTerminalMode {
    fn runs_hooks_immediately(self) -> bool {
        matches!(self, Self::Public { .. })
    }

    fn bypasses_active_terminal_check(self) -> bool {
        !matches!(
            self,
            Self::Public {
                force_non_nil: false
            }
        )
    }

    fn ignore_host_delete_errors(self) -> bool {
        matches!(self, Self::Noelisp)
    }
}

/// GNU's `enum output_method` (src/termhooks.h), as far as neomacs models it:
/// what KIND of display a terminal drives.
///
/// GNU tells these apart by allocating one `struct terminal` per display --
/// `init_initial_terminal` makes the `output_initial` one, `init_tty`
/// (src/term.c) an `output_termcap` one, `x_term_init` an `output_x_window`
/// one -- and deletes the initial terminal once a real one exists.  We keep ONE
/// record and re-describe it in place, so the kind has to be STATED rather than
/// inferred from what happens to be true of the record:
///
/// * not from the id -- GNU's tty terminal is `#<terminal 1 on /dev/tty>` and
///   ours is `#<terminal 0 on /dev/tty>`, because ours is the same record the
///   bootstrap started with;
/// * not from the name -- terminal names are connection labels, not a
///   discriminator for the terminal's output method;
/// * not from liveness or activity -- `terminal-live-p` deliberately reports
///   `output_initial` and `output_termcap` alike as `t` (src/terminal.c:456-459),
///   which is exactly why `turn-on-xterm-mouse-tracking-on-terminal`
///   (lisp/xt-mouse.el:510-512) needs a SECOND question to separate them.
///
/// That second question is `frame-initial-p`, and its terminal branch is one
/// comparison against this type: `t->type == output_initial`
/// (src/terminal.c:499).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TerminalOutputMethod {
    /// GNU `output_initial`: the bootstrap terminal used during daemon mode,
    /// batch mode and the early stages of startup, and which holds the initial
    /// frame.
    Initial,
    /// GNU `output_termcap`: a text terminal on a tty device.
    Termcap,
    /// GNU `output_x_window` / `output_pgtk` / `output_ns` / …: a window-system
    /// display connection.
    WindowSystem,
}

impl TerminalOutputMethod {
    /// GNU `Fframe_initial_p`'s terminal branch: `t->type == output_initial`.
    fn is_initial(self) -> bool {
        matches!(self, Self::Initial)
    }
}

struct TerminalRecord {
    id: u64,
    name: String,
    runtime: TerminalRuntime,
    /// GNU `struct terminal.type`.  See [`TerminalOutputMethod`].
    output_method: TerminalOutputMethod,
    deleted: bool,
    host: Option<Box<dyn TerminalHost>>,
}

impl TerminalRecord {
    fn new(id: u64, name: String) -> Self {
        Self {
            id,
            name,
            runtime: TerminalRuntime::inactive(),
            // GNU's first terminal is `init_initial_terminal`'s, and every
            // record starts as that one until a display init re-describes it.
            output_method: TerminalOutputMethod::Initial,
            deleted: false,
            host: None,
        }
    }

    fn is_live(&self) -> bool {
        !self.deleted
    }

    fn mark_deleted(&mut self) {
        self.deleted = true;
        self.runtime = TerminalRuntime::inactive();
        self.host = None;
    }

    fn is_active(&self) -> bool {
        if !self.is_live() {
            return false;
        }
        if self.runtime.controlling_tty || self.runtime.tty_type.is_some() {
            self.runtime.active && !self.runtime.suspended
        } else {
            true
        }
    }
}

struct TerminalManager {
    terminals: Vec<TerminalRecord>,
}

impl TerminalManager {
    fn new() -> Self {
        let mut this = Self {
            terminals: Vec::new(),
        };
        this.ensure_initial_terminal();
        this
    }

    fn ensure_lisp_terminal_record(&mut self, id: u64) {
        if self.get(id).is_some_and(TerminalRecord::is_live) {
            return;
        }
        let record = TerminalRecord::new(
            id,
            if id == TERMINAL_ID {
                TERMINAL_NAME.to_owned()
            } else {
                format!("terminal-{id}")
            },
        );
        if let Some(terminal) = self.get_mut(id) {
            *terminal = record;
        } else {
            self.terminals.push(record);
        }
    }

    fn ensure_initial_terminal(&mut self) -> &mut TerminalRecord {
        if let Some(idx) = self
            .terminals
            .iter()
            .position(|terminal| terminal.id == TERMINAL_ID)
        {
            if self.terminals[idx].deleted {
                self.terminals[idx].deleted = false;
                self.terminals[idx].runtime = TerminalRuntime::inactive();
                // Re-created from nothing is re-created as GNU's
                // `init_initial_terminal` terminal, whatever it drove before.
                self.terminals[idx].output_method = TerminalOutputMethod::Initial;
                self.terminals[idx].host = None;
            }
            return &mut self.terminals[idx];
        }
        self.terminals
            .push(TerminalRecord::new(TERMINAL_ID, TERMINAL_NAME.to_string()));
        self.terminals.last_mut().expect("initial terminal present")
    }

    fn get(&self, id: u64) -> Option<&TerminalRecord> {
        self.terminals.iter().find(|terminal| terminal.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut TerminalRecord> {
        self.terminals.iter_mut().find(|terminal| terminal.id == id)
    }

    fn live_terminals(&self) -> impl Iterator<Item = &TerminalRecord> {
        self.terminals.iter().filter(|terminal| terminal.is_live())
    }

    fn active_live_terminal_count(&self) -> usize {
        self.live_terminals()
            .filter(|terminal| terminal.is_active())
            .count()
    }

    fn live_terminal_ids_in_keyboard_poll_order(&self) -> Vec<u64> {
        self.terminals
            .iter()
            .rev()
            .filter(|terminal| terminal.is_live())
            .map(|terminal| terminal.id)
            .collect()
    }

    fn ensure_terminal(
        &mut self,
        id: u64,
        name: String,
        runtime: TerminalRuntime,
        output_method: TerminalOutputMethod,
    ) -> &mut TerminalRecord {
        if let Some(idx) = self.terminals.iter().position(|terminal| terminal.id == id) {
            let terminal = &mut self.terminals[idx];
            terminal.name = name;
            terminal.deleted = false;
            terminal.runtime = runtime;
            terminal.output_method = output_method;
            return terminal;
        }
        self.terminals.push(TerminalRecord {
            id,
            name,
            runtime,
            output_method,
            deleted: false,
            host: None,
        });
        self.terminals.last_mut().expect("terminal present")
    }
}

/// Install all primary-terminal facts together; graphical configuration always
/// replaces the bootstrap name along with the output method.
pub fn configure_terminal_runtime(config: impl Into<TerminalRuntimeConfig>) {
    let (name, output_method, runtime) = terminal_configuration_parts(config.into());
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        let terminal = manager.ensure_initial_terminal();
        terminal.name = name;
        terminal.output_method = output_method;
        terminal.runtime = runtime;
    });
}

fn terminal_configuration_parts(
    config: TerminalRuntimeConfig,
) -> (String, TerminalOutputMethod, TerminalRuntime) {
    match config {
        TerminalRuntimeConfig::Bootstrap => (
            TERMINAL_NAME.into(),
            TerminalOutputMethod::Initial,
            TerminalRuntime::inactive(),
        ),
        TerminalRuntimeConfig::Graphical(identity) => (
            identity.terminal_name().into(),
            TerminalOutputMethod::WindowSystem,
            TerminalRuntime::inactive(),
        ),
        TerminalRuntimeConfig::Tty(config) => {
            let runtime = TerminalRuntime {
                active: true,
                tty_type: config.tty_type,
                color_cells: config.attribute_capabilities.color_cells().max(0),
                controlling_tty: true,
                suspended: false,
                attribute_capabilities: config.attribute_capabilities,
            };
            (
                config.name.unwrap_or_else(|| "/dev/tty".into()),
                TerminalOutputMethod::Termcap,
                runtime,
            )
        }
    }
}

pub fn ensure_terminal_runtime_owner(
    id: u64,
    name: impl Into<String>,
    config: impl Into<TerminalRuntimeConfig>,
) -> Value {
    let (configured_name, output_method, runtime) = terminal_configuration_parts(config.into());
    // Explicit owner names describe bootstrap/TTY records. A graphical owner
    // takes its validated connection identity, never an independent override.
    let name = match output_method {
        TerminalOutputMethod::WindowSystem => configured_name,
        TerminalOutputMethod::Initial | TerminalOutputMethod::Termcap => name.into(),
    };
    with_terminal_manager(|slot| {
        slot.borrow_mut()
            .ensure_terminal(id, name, runtime, output_method);
    });
    terminal_handle_for_id(id)
}

pub(crate) fn next_terminal_id() -> u64 {
    with_terminal_manager(|slot| {
        slot.borrow()
            .terminals
            .iter()
            .map(|terminal| terminal.id)
            .max()
            .unwrap_or(TERMINAL_ID)
            .checked_add(1)
            .expect("terminal id exhausted")
    })
}

/// GNU `get_named_terminal`: find an active termcap terminal already owning
/// DEVICE so a second frame shares its renderer, input source, and kboard
/// instead of opening the same tty twice.
pub(crate) fn active_tty_terminal_id_by_name(device: &str) -> Option<u64> {
    with_terminal_manager(|slot| {
        slot.borrow()
            .terminals
            .iter()
            .find(|terminal| {
                terminal.output_method == TerminalOutputMethod::Termcap
                    && terminal.name == device
                    && terminal.is_active()
            })
            .map(|terminal| terminal.id)
    })
}

pub(crate) fn install_opened_tty(
    request: &TtyFrameOpenRequest,
    opened: OpenedTtyFrameHost,
) -> TtyFrameSize {
    let size = opened.size;
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        let runtime = TerminalRuntime {
            active: true,
            tty_type: Some(request.terminal_type.clone()),
            color_cells: opened.attribute_capabilities.color_cells().max(0),
            controlling_tty: true,
            suspended: false,
            attribute_capabilities: opened.attribute_capabilities,
        };
        let terminal = manager.ensure_terminal(
            request.terminal_id,
            request.device.clone(),
            runtime,
            TerminalOutputMethod::Termcap,
        );
        terminal.host = Some(opened.host);
    });
    size
}

pub fn reset_terminal_runtime() {
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        let terminal = manager.ensure_initial_terminal();
        terminal.name = TERMINAL_NAME.to_string();
        terminal.output_method = TerminalOutputMethod::Initial;
        terminal.runtime = TerminalRuntime::inactive();
    });
}

pub fn set_terminal_host(host: Box<dyn TerminalHost>) {
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        manager.ensure_initial_terminal().host = Some(host);
    });
}

pub fn reset_terminal_host() {
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        manager.ensure_initial_terminal().host = None;
    });
}

fn terminal_runtime() -> TerminalRuntime {
    with_terminal_manager(|slot| {
        slot.borrow()
            .get(TERMINAL_ID)
            .map(|terminal| terminal.runtime.clone())
            .unwrap_or_else(TerminalRuntime::inactive)
    })
}

pub(crate) fn terminal_runtime_color_cells() -> i64 {
    terminal_runtime().color_cells
}

pub(crate) fn terminal_runtime_supports_color() -> bool {
    terminal_runtime().supports_color()
}

/// What the current terminal can render -- GNU's `struct tty_display_info`
/// capability strings, the input to `tty_capable_p`.
pub(crate) fn terminal_runtime_attribute_capabilities() -> TtyAttributeCapabilities {
    terminal_runtime().attribute_capabilities
}

/// Clear cached terminal thread-locals (called from `reset_display_thread_locals`).
pub(crate) fn reset_terminal_thread_locals() {
    ensure_current_terminal_registry();
    TERMINAL_LISP_STATE.with(|slot| slot.reset(TerminalLispRegistry::default()));
    TERMINAL_MANAGER.with(|state| {
        let manager = TerminalManager::new();
        if let Some(slot) = state.get() {
            *slot.borrow_mut() = manager;
        } else {
            let _ = state.set(RefCell::new(manager));
        }
    });
    terminal_handle_for_id(TERMINAL_ID);
}

/// Reset only the terminal handle (stale reference safety on heap reset).
/// Retain runtime configuration and hosts; parameters survive only when the
/// handle is reset within the same heap.
pub(crate) fn reset_terminal_handle() {
    ensure_current_terminal_registry();
    let ids = TERMINAL_MANAGER.with(|state| {
        let slot = state.get_or_init(|| RefCell::new(TerminalManager::new()));
        slot.borrow()
            .live_terminals()
            .map(|terminal| terminal.id)
            .collect::<Vec<_>>()
    });
    TERMINAL_LISP_STATE.with(|slot| {
        let mut registry = slot.borrow_mut();
        for id in ids {
            if let Some(terminal) = registry.terminals.get_mut(&id) {
                terminal.handle = Value::make_terminal(id);
            } else {
                registry.ensure_terminal(id);
            }
        }
    });
}

/// Collect GC roots from terminal thread-locals.
#[cfg(test)]
pub(crate) fn collect_terminal_gc_roots(roots: &mut Vec<Value>, heap_identity: usize) {
    // Root enumeration never allocates or walks another thread's native records.
    TERMINAL_LISP_STATE.with(|slot| {
        let handle = slot.current();
        if handle.heap_identity() == heap_identity {
            collect_terminal_registry_gc_roots(&handle, roots);
        }
    });
}

// ---------------------------------------------------------------------------
// Terminal handle helpers
// ---------------------------------------------------------------------------

fn terminal_handle_for_id(id: u64) -> Value {
    ensure_current_terminal_registry();
    TERMINAL_LISP_STATE.with(|slot| slot.borrow_mut().ensure_terminal(id).handle)
}

pub(crate) fn terminal_handle_value() -> Value {
    terminal_handle_value_for_id(TERMINAL_ID).unwrap_or(Value::NIL)
}

pub(crate) fn terminal_handle_value_for_id(id: u64) -> Option<Value> {
    with_terminal_manager(|slot| {
        slot.borrow()
            .get(id)
            .filter(|terminal| terminal.is_live())
            .map(|terminal| terminal_handle_for_id(terminal.id))
    })
}

pub(crate) fn is_terminal_handle(value: &Value) -> bool {
    terminal_handle_id(value).is_some()
}

pub(crate) fn terminal_handle_id(value: &Value) -> Option<u64> {
    ensure_current_terminal_registry();
    TERMINAL_LISP_STATE.with(|slot| {
        slot.borrow()
            .terminals
            .iter()
            .find_map(|(id, terminal)| eq_value(&terminal.handle, value).then_some(*id))
    })
}

pub(crate) fn print_terminal_handle(value: &Value) -> Option<String> {
    let id = terminal_handle_id(value)?;
    with_terminal_manager(|slot| {
        slot.borrow()
            .get(id)
            .map(|terminal| format!("#<terminal {} on {}>", terminal.id, terminal.name))
    })
}

// ---------------------------------------------------------------------------
// Terminal designator predicates
// ---------------------------------------------------------------------------

pub(crate) fn terminal_designator_p(value: &Value) -> bool {
    value.is_nil() || is_terminal_handle(value)
}

fn live_terminal_id_by_handle(value: &Value) -> Option<u64> {
    let id = terminal_handle_id(value)?;
    with_terminal_manager(|slot| {
        slot.borrow()
            .get(id)
            .filter(|terminal| terminal.is_live())
            .map(|terminal| terminal.id)
    })
}

fn selected_terminal_id(eval: &crate::emacs_core::eval::Context) -> Option<u64> {
    eval.frames
        .selected_frame()
        .map(|frame| frame.terminal_id)
        .or_else(|| {
            with_terminal_manager(|slot| {
                slot.borrow()
                    .get(TERMINAL_ID)
                    .filter(|terminal| terminal.is_live())
                    .map(|terminal| terminal.id)
            })
        })
}

pub(crate) fn decode_terminal_id_eval(
    eval: &crate::emacs_core::eval::Context,
    value: &Value,
) -> Option<u64> {
    if value.is_nil() {
        return selected_terminal_id(eval);
    }
    if let Some(id) = live_terminal_id_by_handle(value) {
        return Some(id);
    }
    match value.kind() {
        ValueKind::Veclike(VecLikeType::Frame) => eval
            .frames
            .get(crate::window::FrameId(value.as_frame_id().unwrap()))
            .and_then(|frame| {
                with_terminal_manager(|slot| {
                    slot.borrow()
                        .get(frame.terminal_id)
                        .filter(|terminal| terminal.is_live())
                        .map(|terminal| terminal.id)
                })
            }),
        _ => None,
    }
}

pub(crate) fn terminal_designator_eval_p(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> bool {
    decode_terminal_id_eval(eval, value).is_some()
}

/// What a `frame-initial-p` argument turned out to be.
///
/// GNU's `Fframe_initial_p` (src/terminal.c:482-500) resolves its argument
/// twice: `FRAMEP` first, `decode_terminal` otherwise.  The two branches ask
/// different questions of different objects -- `FRAME_INITIAL_P (f)` of a frame,
/// `t->type == output_initial` of a terminal -- and `decode_terminal`
/// (src/terminal.c:223-233) answers NULL, never a signal, for everything else.
///
/// Naming the three outcomes is what keeps the frame-only reading from creeping
/// back: a port that transcribes only the `if (FRAMEP …)` body loses the branch
/// silently, because the `if` is the only trace of the `else`.  Here the subr
/// matches on this enum, so the terminal case cannot be dropped without the
/// compiler saying so, and there is nowhere left to put a raise.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FrameOrTerminal {
    /// GNU `FRAMEP` + `FRAME_LIVE_P`: a live frame.
    Frame(crate::window::FrameId),
    /// GNU `decode_terminal`: a live terminal.  A deleted one does not qualify
    /// -- `delete_terminal` frees `t->name` and `decode_terminal`'s last line is
    /// `return t && t->name ? t : NULL`.
    Terminal(u64),
    /// GNU's NULL, and GNU's dead frame: the caller answers nil.
    Neither,
}

/// GNU `Fframe_initial_p`'s argument resolution, performed once.
///
/// nil resolves to the selected frame BEFORE the `FRAMEP` test, so nil always
/// takes the frame branch -- in batch we materialize that frame the same way
/// every other frame subr does.
pub(crate) fn decode_frame_or_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    arg: Option<&Value>,
) -> FrameOrTerminal {
    let Some(value) = arg.filter(|value| !value.is_nil()) else {
        return FrameOrTerminal::Frame(
            crate::emacs_core::window_cmds::ensure_selected_frame_id_in_state(
                &mut eval.frames,
                &mut eval.buffers,
            ),
        );
    };
    if let ValueKind::Veclike(VecLikeType::Frame) = value.kind() {
        let frame_id = crate::window::FrameId(value.as_frame_id().expect("frame value"));
        return if eval.frames.get(frame_id).is_some() {
            FrameOrTerminal::Frame(frame_id)
        } else {
            // GNU reaches `FRAME_LIVE_P (f)` here and answers nil.
            FrameOrTerminal::Neither
        };
    }
    match live_terminal_id_by_handle(value) {
        Some(id) => FrameOrTerminal::Terminal(id),
        None => FrameOrTerminal::Neither,
    }
}

pub(crate) fn expect_terminal_designator_eval(
    eval: &mut crate::emacs_core::eval::Context,
    value: &Value,
) -> Result<(), Flow> {
    if terminal_designator_eval_p(eval, value) {
        Ok(())
    } else {
        Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), *value],
        ))
    }
}

// ---------------------------------------------------------------------------
// Terminal parameter helpers
// ---------------------------------------------------------------------------

/// Fallback values for terminal parameters that GNU's own startup Lisp always
/// stores before anything reads them, so a bare `Context` (no `command-line`
/// pass) still answers like a booted GNU session.
///
/// GNU itself has NO terminal-parameter defaults: `terminal-parameter` is a
/// plain assq over the terminal's alist (src/terminal.c, store_terminal_param)
/// and every entry starts absent. In particular `normal-erase-is-backspace`
/// must NOT appear here: `normal-erase-is-backspace-setup-frame`
/// (lisp/simple.el:11097) is guarded by `(unless (terminal-parameter nil
/// 'normal-erase-is-backspace) ...)`, so a fabricated 0 permanently vetoes the
/// real decision `command-line` (lisp/startup.el:1638) makes AFTER
/// `init_sys_modes` publishes the tty's ERASE character -- the mode's
/// `:variable` setter stores the genuine 0/1 (DIVERGENCES.md entry 67).
fn terminal_parameter_default_value(key: &Value) -> Option<Value> {
    match key.as_symbol_name() {
        Some("keyboard-coding-saved-meta-mode") => Some(Value::list(vec![Value::T])),
        _ => None,
    }
}

fn terminal_parameter_default_entries() -> Vec<(Value, Value)> {
    vec![(
        Value::symbol("keyboard-coding-saved-meta-mode"),
        Value::list(vec![Value::T]),
    )]
}

fn lookup_terminal_parameter_value(params: &[(Value, Value)], key: &Value) -> Value {
    params
        .iter()
        .find_map(|(stored_key, stored_value)| {
            if eq_value(stored_key, key) {
                Some(*stored_value)
            } else {
                None
            }
        })
        .or_else(|| terminal_parameter_default_value(key))
        .unwrap_or(Value::NIL)
}

fn terminal_parameters_with_defaults(params: &[(Value, Value)]) -> Vec<(Value, Value)> {
    let mut merged = terminal_parameter_default_entries();
    for (key, value) in params {
        if let Some((_, existing_value)) = merged
            .iter_mut()
            .find(|(existing_key, _)| eq_value(existing_key, key))
        {
            *existing_value = *value;
        } else {
            merged.push((*key, *value));
        }
    }
    merged
}

fn expect_symbol_key(value: &Value) -> Result<Value, Flow> {
    match value.kind() {
        ValueKind::Nil | ValueKind::T | ValueKind::Symbol(_) => Ok(*value),
        _other => Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("symbolp"), *value],
        )),
    }
}

fn terminal_name_for_id(id: u64) -> Option<String> {
    with_terminal_manager(|slot| slot.borrow().get(id).map(|terminal| terminal.name.clone()))
}

fn terminal_runtime_for_id(id: u64) -> TerminalRuntime {
    with_terminal_manager(|slot| {
        slot.borrow()
            .get(id)
            .map(|terminal| terminal.runtime.clone())
            .unwrap_or_else(TerminalRuntime::inactive)
    })
}

/// GNU `t->type` for a terminal that still exists.
fn terminal_output_method_for_id(id: u64) -> Option<TerminalOutputMethod> {
    with_terminal_manager(|slot| slot.borrow().get(id).map(|terminal| terminal.output_method))
}

/// GNU `Fsend_string_to_terminal`'s terminal dispatch
/// (src/dispnew.c:6819-6843): route STRING's bytes to the terminal's output
/// stream, unaltered.  The Lisp-visible argument decoding lives with the
/// builtin; everything that reads the terminal record lives here.
pub(crate) fn write_bytes_to_terminal(terminal_id: u64, bytes: &[u8]) -> Result<(), Flow> {
    match terminal_output_method_for_id(terminal_id) {
        // GNU: `out = stdout' for `output_initial' -- what batch, daemon and
        // pre-tty sessions run on (src/dispnew.c:6823-6824).
        Some(TerminalOutputMethod::Initial) => {
            use std::io::Write as _;
            // GNU ignores the fwrite result here; the tty has nowhere better
            // to report a failed diagnostic write.
            let _ = std::io::stdout().write_all(bytes);
            let _ = std::io::stdout().flush();
            Ok(())
        }
        // GNU: `error("Device %d is not a termcap terminal device", t->id)'
        // (src/dispnew.c:6826-6827).  `None' cannot survive the caller's
        // live-terminal decode; treat it with the same answer for totality.
        Some(TerminalOutputMethod::WindowSystem) | None => Err(signal(
            "error",
            vec![Value::string(format!(
                "Device {terminal_id} is not a termcap terminal device"
            ))],
        )),
        Some(TerminalOutputMethod::Termcap) => {
            // GNU: `!tty->output' means the terminal is suspended
            // (src/dispnew.c:6833-6835).
            if terminal_runtime_for_id(terminal_id).suspended {
                return Err(signal(
                    "error",
                    vec![Value::string("Terminal is currently suspended")],
                ));
            }
            with_terminal_host_for_id(terminal_id, |host| host.write_bytes(bytes))
        }
    }
}

/// Mark the selected terminal as having a controlling tty, so it can host a
/// text-terminal frame. Used by tests that exercise `make-frame` /
/// `make-terminal-frame`, which in a real session run on an interactive
/// terminal (the production batch path deliberately has neither a controlling
/// tty nor a type, so frame creation errors like GNU).
#[allow(dead_code)] // grandfathered when dead_code lint was enabled; delete or wire up
pub(crate) fn mark_selected_terminal_usable_for_test(eval: &crate::emacs_core::eval::Context) {
    if let Some(id) = decode_terminal_id_eval(eval, &Value::NIL) {
        with_terminal_manager(|slot| {
            if let Some(record) = slot.borrow_mut().get_mut(id) {
                record.runtime.controlling_tty = true;
            }
        });
    }
}

/// Whether the selected terminal can host a text-terminal frame: it has a
/// controlling tty or a known terminal type. GNU's `init_tty` signals
/// "Unknown terminal type" when neither holds (batch / no real terminal), which
/// is why `make-frame` / `make-terminal-frame` error in `--batch`.
pub(crate) fn selected_terminal_is_usable_tty(eval: &crate::emacs_core::eval::Context) -> bool {
    decode_terminal_id_eval(eval, &Value::NIL)
        .map(|id| {
            let runtime = terminal_runtime_for_id(id);
            runtime.controlling_tty || runtime.tty_type.is_some()
        })
        .unwrap_or(false)
}

fn terminal_params_for_id(id: u64) -> Vec<(Value, Value)> {
    ensure_current_terminal_registry();
    TERMINAL_LISP_STATE.with(|slot| {
        slot.borrow()
            .terminals
            .get(&id)
            .map(|terminal| terminal.params.clone())
            .unwrap_or_default()
    })
}

fn update_terminal_param(id: u64, key: Value, value: Value) -> Value {
    terminal_handle_for_id(id);
    TERMINAL_LISP_STATE.with(|slot| {
        let mut registry = slot.borrow_mut();
        let terminal = registry
            .terminals
            .get_mut(&id)
            .expect("terminal Lisp state");
        if let Some((_, stored_value)) = terminal
            .params
            .iter_mut()
            .find(|(stored_key, _)| eq_value(stored_key, &key))
        {
            let previous = *stored_value;
            *stored_value = value;
            return previous;
        }
        let previous = terminal_parameter_default_value(&key).unwrap_or(Value::NIL);
        terminal.params.push((key, value));
        previous
    })
}

fn with_terminal_host_for_id<R>(
    id: u64,
    f: impl FnOnce(&mut dyn TerminalHost) -> Result<R, String>,
) -> Result<R, Flow> {
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        let Some(host) = manager
            .get_mut(id)
            .and_then(|terminal| terminal.host.as_deref_mut())
        else {
            return Err(signal(
                "error",
                vec![Value::string("TTY terminal host unavailable")],
            ));
        };
        f(host).map_err(|message| signal("error", vec![Value::string(message)]))
    })
}

fn delete_terminal_record(id: u64) {
    // Deletion is Context-owned too: otherwise reinstall would recreate this
    // terminal and retain its parameters as roots on every destination thread.
    TERMINAL_LISP_STATE.with(|slot| {
        let mut registry = slot.borrow_mut();
        registry.terminals.remove(&id);
        registry
            .creation_order
            .retain(|terminal_id| *terminal_id != id);
    });
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(id) {
            terminal.mark_deleted();
        }
    });
}

pub(crate) fn live_terminal_ids_in_keyboard_poll_order() -> Vec<u64> {
    with_terminal_manager(|slot| slot.borrow().live_terminal_ids_in_keyboard_poll_order())
}

// ---------------------------------------------------------------------------
// Alist helper
// ---------------------------------------------------------------------------

pub(crate) fn make_alist(pairs: Vec<(Value, Value)>) -> Value {
    let entries: Vec<Value> = pairs.into_iter().map(|(k, v)| Value::cons(k, v)).collect();
    Value::list(entries)
}

// ---------------------------------------------------------------------------
// Argument helpers (local copies — identical to display.rs)
// ---------------------------------------------------------------------------

// ---------------------------------------------------------------------------
// Terminal builtins
// ---------------------------------------------------------------------------

/// (terminal-name &optional TERMINAL) -> "initial_terminal"
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_name(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("terminal-name", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::string(
        terminal_name_for_id(terminal_id).unwrap_or_else(|| TERMINAL_NAME.to_string()),
    ))
}

/// `(frame-initial-p &optional FRAME)` -- GNU `Fframe_initial_p`,
/// src/terminal.c:482-500.
///
/// FRAME is a frame OR a terminal, and GNU's doc string says both: "If FRAME is
/// a terminal object, return non-nil if it holds the initial frame."  The
/// terminal branch has a caller that depends on it --
/// `turn-on-xterm-mouse-tracking-on-terminal` (lisp/xt-mouse.el:508-512) hands
/// it a TERMINAL to skip "the initial terminal which is not a termcap device" --
/// and that caller runs during startup on every TERM matching
/// `xterm--auto-xt-mouse-allowed-types` (lisp/term/xterm.el:134-140).  It runs
/// inside `tty-run-terminal-initialization`, i.e. before `command-line-1`, so a
/// raise here does not merely print: it costs the whole command line, `-l` and
/// `--eval` included.
///
/// Nothing handed to this subr can raise.  GNU's `decode_terminal` answers NULL
/// for a non-designator and for a deleted terminal, and `FRAME_LIVE_P` covers a
/// dead frame, so every unusable argument answers nil.
pub(crate) fn builtin_frame_initial_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-initial-p", &args, 1)?;
    let initial = match decode_frame_or_terminal(eval, args.first()) {
        // GNU: `FRAME_LIVE_P (f) && FRAME_INITIAL_P (f)`; the resolution above
        // has already established the frame is live.
        FrameOrTerminal::Frame(frame_id) => {
            eval.frames.get(frame_id).is_some_and(|frame| frame.initial)
        }
        // GNU: `t->type == output_initial`.
        FrameOrTerminal::Terminal(terminal_id) => {
            terminal_output_method_for_id(terminal_id).is_some_and(TerminalOutputMethod::is_initial)
        }
        FrameOrTerminal::Neither => false,
    };
    Ok(Value::bool_val(initial))
}

/// (terminal-list) -> list of live terminal handles.
pub(crate) fn builtin_terminal_list(args: Vec<Value>) -> EvalResult {
    expect_max_args("terminal-list", &args, 0)?;
    let terminals = with_terminal_manager(|slot| {
        slot.borrow()
            .live_terminals()
            .map(|terminal| terminal_handle_for_id(terminal.id))
            .collect::<Vec<_>>()
    });
    Ok(Value::list(terminals))
}

/// (selected-terminal) -> currently selected terminal handle.
#[cfg(test)]
pub(crate) fn builtin_selected_terminal(args: Vec<Value>) -> EvalResult {
    expect_args("selected-terminal", &args, 0)?;
    Ok(terminal_handle_value())
}

/// (frame-terminal &optional FRAME) -> opaque terminal handle.
///
/// Accepts live frame designators in addition to nil.
pub(crate) fn builtin_frame_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("frame-terminal", &args, 1)?;
    let terminal_id = if let Some(frame) = args.first() {
        if frame.is_nil() {
            selected_terminal_id(eval)
        } else {
            match frame.kind() {
                ValueKind::Veclike(VecLikeType::Frame) => eval
                    .frames
                    .get(crate::window::FrameId(frame.as_frame_id().unwrap()))
                    .map(|frame| frame.terminal_id),
                _ => None,
            }
        }
    } else {
        selected_terminal_id(eval)
    };
    let Some(terminal_id) = terminal_id else {
        let bad = args.first().copied().unwrap_or(Value::NIL);
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("frame-live-p"), bad],
        ));
    };
    Ok(terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value))
}

/// (terminal-live-p TERMINAL) -> t
///
/// In GNU Emacs, terminal-live-p returns the terminal type symbol
/// (e.g. 'x, 'w32) for GUI terminals, or t for TTY.  This is used
/// by framep-on-display to determine the window system type.
pub(crate) fn builtin_terminal_live_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("terminal-live-p", &args, 1, 1)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Ok(Value::NIL);
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    let mut terminal_has_frame = false;
    let window_system = eval
        .frames
        .frame_list()
        .into_iter()
        .filter_map(|frame_id| eval.frames.get(frame_id))
        .filter(|frame| frame.terminal_id == terminal_id)
        .find_map(|frame| {
            terminal_has_frame = true;
            frame.effective_window_system()
        });
    // Return the window system type so framep-on-display works correctly.
    if let Some(window_system) = window_system {
        Ok(window_system)
    } else if terminal_has_frame || runtime.controlling_tty || runtime.tty_type.is_some() {
        Ok(Value::T)
    } else if crate::emacs_core::display::x_window_system_active(eval) {
        Ok(Value::symbol(
            crate::emacs_core::display::gui_window_system_symbol(),
        ))
    } else {
        Ok(Value::T)
    }
}

/// (terminal-parameter TERMINAL PARAMETER) -> value
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_parameter(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("terminal-parameter", &args, 2)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), args[0]],
        ));
    };
    let key = expect_symbol_key(&args[1])?;
    Ok(lookup_terminal_parameter_value(
        &terminal_params_for_id(terminal_id),
        &key,
    ))
}

/// (terminal-parameters &optional TERMINAL) -> alist of terminal parameters
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_terminal_parameters(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("terminal-parameters", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let merged = terminal_parameters_with_defaults(&terminal_params_for_id(terminal_id));
    Ok(make_alist(merged))
}

/// (set-terminal-parameter TERMINAL PARAMETER VALUE) -> previous value
///
/// Accepts live frame designators in addition to terminal designators.
pub(crate) fn builtin_set_terminal_parameter(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args("set-terminal-parameter", &args, 3)?;
    let Some(terminal_id) = decode_terminal_id_eval(eval, &args[0]) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), args[0]],
        ));
    };
    if args[1].is_string() {
        return Ok(Value::NIL);
    }
    let key = args[1];
    Ok(update_terminal_param(terminal_id, key, args[2]))
}

// ---------------------------------------------------------------------------
// TTY builtins (we are not a TTY, so these return nil)
// ---------------------------------------------------------------------------

/// (tty-type &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_type(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-type", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(terminal_runtime_for_id(terminal_id)
        .tty_type
        .map(Value::string)
        .unwrap_or(Value::NIL))
}

/// (tty-top-frame &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_top_frame(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-top-frame", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Ok(Value::NIL);
    }
    let top = eval
        .frames
        .top_frame_on_terminal(terminal_id)
        .map(|frame_id| Value::make_frame(frame_id.0))
        .unwrap_or(Value::NIL);
    Ok(top)
}

/// (tty-display-color-p &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_display_color_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-display-color-p", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::bool_val(
        terminal_runtime_for_id(terminal_id).supports_color(),
    ))
}

/// (tty-display-color-cells &optional TERMINAL) -> 0
pub(crate) fn builtin_tty_display_color_cells(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-display-color-cells", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::fixnum(
        terminal_runtime_for_id(terminal_id).color_cells,
    ))
}

/// (tty-no-underline &optional TERMINAL) -> nil
pub(crate) fn builtin_tty_no_underline(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("tty-no-underline", &args, 1)?;
    if let Some(terminal) = args.first()
        && decode_terminal_id_eval(eval, terminal).is_none()
    {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), *terminal],
        ));
    }
    Ok(Value::NIL)
}

/// (controlling-tty-p &optional TERMINAL) -> nil
pub(crate) fn builtin_controlling_tty_p(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("controlling-tty-p", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    Ok(Value::bool_val(
        terminal_runtime_for_id(terminal_id).controlling_tty,
    ))
}

/// (suspend-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_suspend_tty(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("suspend-tty", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to suspend a non-text terminal device",
            )],
        ));
    }

    if runtime.suspended {
        return Ok(Value::NIL);
    }

    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    let hook_sym =
        crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "suspend-tty-functions");
    let _ = crate::emacs_core::hook_runtime::run_named_hook(eval, hook_sym, &[terminal])?;
    with_terminal_host_for_id(terminal_id, |host| host.suspend_tty())?;
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(terminal_id) {
            terminal.runtime.suspended = true;
        }
    });
    Ok(Value::NIL)
}

/// (resume-tty &optional TTY) -> error in GUI/non-text terminal context.
pub(crate) fn builtin_resume_tty(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_max_args("resume-tty", &args, 1)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Err(signal(
            LispCondition::WrongTypeArgument,
            vec![Value::symbol("terminal-live-p"), designator],
        ));
    };
    let runtime = terminal_runtime_for_id(terminal_id);
    if !runtime.active {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to resume a non-text terminal device",
            )],
        ));
    }

    if !runtime.suspended {
        return Ok(Value::NIL);
    }

    with_terminal_host_for_id(terminal_id, |host| host.resume_tty())?;
    with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        if let Some(terminal) = manager.get_mut(terminal_id) {
            terminal.runtime.suspended = false;
        }
    });
    // GNU `resume-tty` reinitializes the terminal modes and marks its frames
    // garbaged. The host's pending full repaint must reach the redisplay
    // closure even when suspension changed no Lisp-visible display state.
    eval.invalidate_redisplay();
    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    let hook_sym =
        crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "resume-tty-functions");
    let _ = crate::emacs_core::hook_runtime::run_named_hook(eval, hook_sym, &[terminal])?;
    Ok(Value::NIL)
}

// ---------------------------------------------------------------------------
// Builtins moved from builtins.rs
// ---------------------------------------------------------------------------

pub(crate) fn delete_terminal_owned(
    eval: &mut crate::emacs_core::eval::Context,
    terminal_id: u64,
    mode: DeleteTerminalMode,
) -> EvalResult {
    let active_live_count =
        with_terminal_manager(|slot| slot.borrow().active_live_terminal_count());
    if !mode.bypasses_active_terminal_check() && active_live_count <= 1 {
        return Err(signal(
            "error",
            vec![Value::string(
                "Attempt to delete the sole active display terminal",
            )],
        ));
    }
    let terminal = terminal_handle_value_for_id(terminal_id).unwrap_or_else(terminal_handle_value);
    if mode.runs_hooks_immediately() {
        let hook_sym =
            crate::emacs_core::hook_runtime::hook_symbol_by_name(eval, "delete-terminal-functions");
        let _ = crate::emacs_core::hook_runtime::safe_run_named_hook(eval, hook_sym, &[terminal])?;
    } else {
        eval.queue_pending_safe_hook("delete-terminal-functions", &[terminal]);
    }
    let host_delete = with_terminal_manager(|slot| {
        let mut manager = slot.borrow_mut();
        let Some(host) = manager
            .get_mut(terminal_id)
            .and_then(|terminal| terminal.host.as_deref_mut())
        else {
            return Ok(());
        };
        host.delete_terminal()
    });
    if let Err(message) = host_delete {
        if mode.ignore_host_delete_errors() {
            tracing::warn!(
                "terminal owner: ignoring host delete failure during noelisp teardown: {}",
                message
            );
        } else {
            return Err(signal("error", vec![Value::string(message)]));
        }
    }

    let frames_to_delete = eval
        .frames
        .frame_list()
        .into_iter()
        .filter(|frame_id| {
            eval.frames
                .get(*frame_id)
                .is_some_and(|frame| frame.terminal_id == terminal_id)
        })
        .collect::<Vec<_>>();
    for frame_id in frames_to_delete {
        let _ = crate::emacs_core::window_cmds::delete_frame_owned(
            eval,
            frame_id,
            crate::emacs_core::window_cmds::DeleteFrameMode::Noelisp,
        )?;
    }
    delete_terminal_record(terminal_id);
    eval.command_loop
        .keyboard
        .delete_terminal_kboard(terminal_id);
    if eval.frames.selected_frame().is_none()
        && let Some(next_selected) = eval.frames.frame_list().into_iter().next()
    {
        let _ = eval.frames.select_frame(next_selected);
    }
    eval.sync_keyboard_terminal_owner();
    Ok(Value::NIL)
}

pub(crate) fn delete_terminal_noelisp_owned(
    eval: &mut crate::emacs_core::eval::Context,
    terminal_id: u64,
) -> EvalResult {
    delete_terminal_owned(eval, terminal_id, DeleteTerminalMode::Noelisp)
}

/// (delete-terminal &optional TERMINAL FORCE) -> nil or error
pub(crate) fn builtin_delete_terminal(
    eval: &mut crate::emacs_core::eval::Context,
    args: Vec<Value>,
) -> EvalResult {
    expect_args_range("delete-terminal", &args, 0, 2)?;
    let designator = args.first().copied().unwrap_or(Value::NIL);
    let Some(terminal_id) = decode_terminal_id_eval(eval, &designator) else {
        return Ok(Value::NIL);
    };
    let force_non_nil = args.get(1).copied().unwrap_or(Value::NIL).is_truthy();
    delete_terminal_owned(
        eval,
        terminal_id,
        DeleteTerminalMode::Public { force_non_nil },
    )
}

#[cfg(test)]
#[path = "tests/gc_context_migration.rs"]
mod gc_context_migration_tests;

#[cfg(test)]
#[path = "tests/registry_reinstall.rs"]
mod registry_reinstall_tests;
